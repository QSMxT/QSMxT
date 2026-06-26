use std::io::Write;
use std::sync::Mutex;

use log::{error, info};

use crate::bids::discovery::{self, DiscoveryFilter};
use crate::bids::derivatives::DerivativeOutputs;
use crate::cli::RunArgs;
use crate::executor;
use crate::pipeline::config::PipelineConfig;
use crate::pipeline::memory;

pub fn execute(args: RunArgs) -> crate::Result<()> {
    // Build config: file -> CLI overrides
    let mut config = if let Some(ref path) = args.config {
        crate::pipeline::config::load_config(path)?
    } else {
        PipelineConfig::default()
    };

    crate::pipeline::config::apply_run_overrides(&mut config, &args);
    

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
        println!("  Masking:           {}", config.masking.sections.iter()
            .map(|s| format!("{}", s))
            .collect::<Vec<_>>()
            .join(" | "));
        println!("  Unwrapping:        {}", config.field_mapping.unwrapping_algorithm);
        println!("  BG Removal:        {}", config.bg_removal.algorithm);
        println!("  QSM Algorithm:     {:?}", config.inversion.algorithm);
        println!("  QSM Reference:     {:?}", config.qsm.reference);
        println!();
        for run in &runs {
            let (nx, ny, nz) = run.dims;
            let est = memory::estimate_peak_memory_bytes(
                nx, ny, nz, run.echoes.len(), run.has_magnitude, &config,
            );
            println!(
                "  {} ({} echo(es), {}x{}x{}, B0={:.1}T, est. {})",
                run.key,
                run.echoes.len(),
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
                        r.echoes.len(), r.has_magnitude, &config,
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

    // Resolve output: <dir>/derivatives/qsmxt.rs/
    let base_dir = args.output_dir.as_deref().unwrap_or(&args.bids_dir);
    let derivatives_dir = base_dir.join("derivatives").join("qsmxt.rs");
    std::fs::create_dir_all(&derivatives_dir)?;

    // Set up logger: write to both stderr and a log file
    let log_path = derivatives_dir.join("qsmxt.log");
    let log_file = std::fs::File::create(&log_path)?;
    let log_file = Mutex::new(log_file);
    let log_level = if args.debug { log::LevelFilter::Debug } else { log::LevelFilter::Info };
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
            if let Ok(mut f) = log_file.lock() {
                let _ = writeln!(f, "[{level:5} {}] {}", record.target(), record.args());
            }
            Ok(())
        })
        .try_init()
        .ok();

    // Log version info
    info!("qsmxt.rs {}", env!("CARGO_PKG_VERSION"));
    info!("QSM.rs {} ({})", env!("QSM_CORE_VERSION"), env!("QSM_CORE_GIT_HASH"));
    info!(
        "Processing {} run(s) across {} subject(s)",
        runs.len(),
        subjects.len()
    );

    let output = DerivativeOutputs::new(&derivatives_dir);

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
    };

    let results = executor::local::execute_local(runs, &config, &output, &exec_config);

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
