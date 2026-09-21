use std::io::Write;
use std::sync::Mutex;

use log::{error, info, warn};

use crate::bids::discovery::{self, DiscoveryFilter};
use crate::bids::derivatives::DerivativeOutputs;
use crate::cli::RunArgs;
use crate::executor;
use crate::pipeline::config::PipelineConfig;
use crate::pipeline::memory;

/// Route `log` records to stderr (through the progress-bar multiplexer) and, for a real run, to
/// `derivatives/qsmxt/qsmxt.log`.
fn init_logging(log_file: Option<std::fs::File>, debug: bool) {
    let log_file = log_file.map(Mutex::new);
    let log_level = if debug { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    env_logger::Builder::new()
        .filter_level(log_level)
        .format_timestamp(None)
        .format(move |_buf, record| {
            use env_logger::fmt::style::{AnsiColor, Style};
            let level = record.level();
            let style = match level {
                log::Level::Error => Style::new().fg_color(Some(AnsiColor::Red.into())),
                log::Level::Warn  => Style::new().fg_color(Some(AnsiColor::Yellow.into())),
                log::Level::Info  => Style::new().fg_color(Some(AnsiColor::Green.into())),
                log::Level::Debug => Style::new().fg_color(Some(AnsiColor::Blue.into())),
                log::Level::Trace => Style::new().fg_color(Some(AnsiColor::Cyan.into())),
            };
            // Use MultiProgress.println to properly coordinate with progress bars
            let line = format!("[{style}{level:5}{style:#} {}] {}",
                record.target(), record.args());
            let _ = crate::pipeline::runner::MULTI_PROGRESS.println(line);
            // Plain text to log file
            if let Some(ref f) = log_file {
                if let Ok(mut f) = f.lock() {
                    let _ = writeln!(f, "[{level:5} {}] {}", record.target(), record.args());
                }
            }
            Ok(())
        })
        .try_init()
        .ok();
}

pub fn execute(args: RunArgs) -> crate::Result<()> {
    // Resolve output: <dir>/derivatives/qsmxt/. Both come from args alone, so this can happen
    // before anything else — which it must, because the logger writes there.
    let base_dir = args.output_dir.as_deref().unwrap_or(&args.bids_dir);
    let derivatives_dir = base_dir.join("derivatives").join("qsmxt");

    // The logger goes up before the config is built, because building it is the step that
    // validates the CLI: every "ignoring invalid ..." warning apply_run_overrides emits was
    // being written to a logger that did not exist yet, and silently dropped. A dry run gets
    // stderr only — it must not create directories in the dataset, and its whole purpose is to
    // show what a real run would do, warnings included.
    if args.dry {
        init_logging(None, args.debug);
    } else {
        std::fs::create_dir_all(&derivatives_dir)?;
        let log_file = std::fs::File::create(derivatives_dir.join("qsmxt.log"))?;
        init_logging(Some(log_file), args.debug);
    }

    // Build config: file -> CLI overrides
    let mut config = if let Some(ref path) = args.config {
        crate::pipeline::config::load_config(path)?
    } else {
        PipelineConfig::default()
    };

    crate::pipeline::config::apply_run_overrides(&mut config, &args.pipeline);
    

    // Discover BIDS runs
    let filter = DiscoveryFilter {
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        num_echoes: args.num_echoes,
    };

    let runs = discovery::discover_runs(&args.bids_dir, &filter)?;

    if runs.is_empty() {
        eprintln!("No QSM-compatible runs found in {}", args.bids_dir.display());
        return Ok(());
    }

    // Count unique subjects
    let mut subjects: Vec<&str> = runs.iter().map(|r| r.key.subject.as_str()).collect();
    subjects.sort();
    subjects.dedup();

    info!(
        "Discovered {} run(s) across {} subject(s)",
        runs.len(),
        subjects.len()
    );

    // Compute execution parameters
    let n_procs = args.n_procs.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    });

    let mem_limit_bytes = if args.no_mem_limit {
        None
    } else if let Some(gb) = args.mem_limit_gb {
        Some((gb * 1024.0 * 1024.0 * 1024.0) as usize)
    } else {
        // Auto-detect: use MemAvailable, reserve 1 GB for OS
        let available = memory::available_memory_bytes();
        let reserved = 1024 * 1024 * 1024; // 1 GB
        Some(available.saturating_sub(reserved))
    };

    if args.dry {
        println!("Pipeline:");
        println!("  Phase Offset Removal: {}", if config.field_mapping.phase_offset_removal { "enabled" } else { "disabled" });
        if config.masking.inhomogeneity_correction {
            println!("  Inhomogeneity:     enabled");
        }
        let joiner = format!(" {} ", config.masking.combine.to_string().to_uppercase());
        let mut masking = config.masking.sections.iter()
            .map(|s| format!("{}", s))
            .collect::<Vec<_>>()
            .join(&joiner);
        for op in &config.masking.refinements {
            masking.push_str(&format!(" → {op}"));
        }
        println!("  Masking:           {masking}");
        println!("  Unwrapping:        {}", config.field_mapping.unwrapping_algorithm);
        println!("  BG Removal:        {}", config.bg_removal.algorithm);
        println!("  QSM Algorithm:     {:?}", config.inversion.algorithm);
        println!("  QSM Reference:     {:?}", config.qsm.reference);
        println!();
        for run in &runs {
            let (nx, ny, nz) = run.dims;
            let n_coils = run.coils.as_ref().map_or(1, |c| c.len());
            let est = memory::estimate_peak_memory_bytes(
                nx, ny, nz, run.echoes.len(), n_coils, run.has_magnitude, &config,
            );
            println!(
                "  {} ({} echo(es){}, {}x{}x{}, B0={:.1}T, est. {})",
                run.key,
                run.echoes.len(),
                if n_coils > 1 { format!(" x {} uncombined coils (MCPC-3D-S)", n_coils) } else { String::new() },
                nx, ny, nz,
                run.magnetic_field_strength,
                memory::format_bytes(est),
            );
        }
        if let Some(mem) = mem_limit_bytes {
            let per_run_max = runs
                .iter()
                .map(|r| {
                    memory::estimate_peak_memory_bytes(
                        r.dims.0, r.dims.1, r.dims.2,
                        r.echoes.len(), r.coils.as_ref().map_or(1, |c| c.len()), r.has_magnitude, &config,
                    )
                })
                .max()
                .unwrap_or(0);
            let max_concurrent = (mem.checked_div(per_run_max))
                .map(|v| v.max(1).min(n_procs))
                .unwrap_or(n_procs);
            println!();
            println!(
                "Memory: {} available, max {} concurrent run(s)",
                memory::format_bytes(mem),
                max_concurrent,
            );
        }
        return Ok(());
    }

    // Log version info
    info!("qsmxt {}", env!("CARGO_PKG_VERSION"));
    info!("QSM.rs {} ({})", env!("QSM_CORE_VERSION"), env!("QSM_CORE_GIT_HASH"));
    info!(
        "Processing {} run(s) across {} subject(s)",
        runs.len(),
        subjects.len()
    );

    let output = DerivativeOutputs::new(&derivatives_dir);

    // Write the BIDS derivative dataset_description.json (required for a valid
    // derivative dataset) and a .bidsignore for non-BIDS pipeline artefacts.
    crate::bids::dataset_description::write(&derivatives_dir, &args.bids_dir)?;
    crate::bids::dataset_description::write_bidsignore(&derivatives_dir)?;
    crate::bids::dataset_description::write_readme(&derivatives_dir)?;

    // Save config to derivatives dir
    let config_path = derivatives_dir.join("pipeline_config.toml");
    std::fs::write(&config_path, config.to_toml().unwrap_or_default())?;

    // Save methods description
    let methods_path = derivatives_dir.join("methods.md");
    std::fs::write(&methods_path, crate::pipeline::methods::generate_methods(&config))?;

    // Execute
    let exec_config = executor::local::ExecutionConfig {
        n_procs,
        mem_limit_bytes,
        force: args.force,
        clean_intermediates: args.clean_intermediates,
        source_dicom: args.source_dicom.clone(),
        dicom_outputs: args.dicom_outputs.clone(),
    };

    let results = executor::local::execute_local(&runs, &config, &output, &exec_config);

    // Write BEP028 (BIDS-Prov) records and per-output GeneratedBy sidecars.
    // Best-effort: provenance output must never fail an otherwise-successful run.
    let command_line = std::env::args().collect::<Vec<_>>().join(" ");
    if let Err(e) = crate::bids::bidsprov::write_provenance(&derivatives_dir, &runs, &output, &command_line) {
        warn!("Failed to write BIDS-Prov provenance records: {}", e);
    }

    let failures: Vec<_> = results.iter().filter(|r| r.is_err()).collect();
    if !failures.is_empty() {
        error!("{} run(s) failed:", failures.len());
        for f in &failures {
            if let Err(e) = f {
                error!("  {}", e);
            }
        }
        return Err(crate::error::QsmxtError::Algorithm {
            stage: "pipeline".to_string(),
            message: format!("{} run(s) failed", failures.len()),
        });
    }

    info!("All runs completed successfully — results in {}", derivatives_dir.display());
    Ok(())
}
