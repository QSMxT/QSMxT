use std::io::Write;
use std::sync::Mutex;

use log::{error, info, warn};

use crate::bids::discovery::{self, DiscoveryFilter};
use crate::bids::derivatives::DerivativeOutputs;
use crate::cli::RunArgs;
use crate::executor;
use crate::pipeline::config::PipelineConfig;
use crate::pipeline::memory;

/// Where log records go besides stderr.
pub(crate) enum LogSink {
    /// stderr only (a dry run: it must not create files in the dataset).
    Discard,
    /// Held until the log file is known: a `--no-dataset-steps` worker names its log after the
    /// runs it discovers, and `qsmxt aggregate` puts the workers' logs first.
    Buffer(Vec<String>),
    File(std::fs::File),
}

/// One sink for the process, swapped by each command that runs the pipeline. A global rather
/// than state captured by the logger, because `env_logger` can be installed only once.
static LOG_SINK: Mutex<LogSink> = Mutex::new(LogSink::Discard);

fn log_line(line: &str) {
    let mut sink = LOG_SINK.lock().unwrap_or_else(|e| e.into_inner());
    match &mut *sink {
        LogSink::Discard => {}
        LogSink::Buffer(lines) => lines.push(line.to_string()),
        LogSink::File(f) => {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Send the log to `file` from here on, writing whatever was buffered first.
pub(crate) fn attach_log_file(mut file: std::fs::File) -> std::io::Result<()> {
    let mut sink = LOG_SINK.lock().unwrap_or_else(|e| e.into_inner());
    if let LogSink::Buffer(lines) = &*sink {
        for line in lines {
            writeln!(file, "{line}")?;
        }
    }
    *sink = LogSink::File(file);
    Ok(())
}

/// Route `log` records to stderr (through the progress-bar multiplexer) and to `sink`
/// (`derivatives/qsmxt/qsmxt.log` for a real run).
pub(crate) fn init_logging(sink: LogSink, debug: bool) {
    *LOG_SINK.lock().unwrap_or_else(|e| e.into_inner()) = sink;
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
            // When stderr is not a terminal — a SLURM job, a CI log, any redirect — indicatif's
            // draw target is hidden and `println` silently discards the line. That would take
            // every warning and every fatal error with it, leaving a bare exit code and no
            // explanation, so write straight to stderr in that case.
            if cfg!(test) {
                // The logger is process-global, so once a test has run the pipeline every later
                // test's warnings would reach the test output (tens of thousands of lines from the
                // TUI parity test alone). Tests keep only the log sink.
            } else if crate::pipeline::runner::MULTI_PROGRESS.is_hidden() {
                let _ = writeln!(std::io::stderr(), "[{level:5} {}] {}",
                                 record.target(), record.args());
            } else {
                let _ = crate::pipeline::runner::MULTI_PROGRESS.println(line);
            }
            // Plain text to the log file
            log_line(&format!("[{level:5} {}] {}", record.target(), record.args()));
            Ok(())
        })
        .try_init()
        .ok();
}

/// The configuration a pipeline command runs with: the file, then the command-line overrides,
/// then every check that can be made before a run starts. Shared by `qsmxt run`, `qsmxt slurm`
/// and `qsmxt aggregate`, so the three cannot disagree about what a set of flags means.
pub(crate) fn build_config(
    config_file: Option<&std::path::Path>, pipeline: &crate::cli::PipelineArgs,
) -> crate::Result<PipelineConfig> {
    let mut config = match config_file {
        Some(path) => crate::pipeline::config::load_config(path)?,
        None => PipelineConfig::default(),
    };
    crate::pipeline::config::apply_run_overrides(&mut config, pipeline)?;
    crate::pipeline::config::validate_reference_region(&config)?;
    crate::pipeline::config::validate_segmentation(&config)?;
    crate::pipeline::config::validate_multi_orientation(&config)?;
    crate::pipeline::config::validate_inversion_input(&config)?;
    qsmxt_engine::schema::check_ranges(&config)
        .map_err(|e| crate::error::QsmxtError::Config(e.to_string()))?;
    Ok(config)
}

/// The log of a `--no-dataset-steps` worker: `logs/<run>.log`, named after the run it processed
/// (`<first run>_and_<n>_more.log` when it was given several). `qsmxt aggregate` gathers these
/// into `qsmxt.log`.
pub(crate) fn worker_log_path(derivatives_dir: &std::path::Path, runs: &[discovery::QsmRun]) -> std::path::PathBuf {
    let name = match runs {
        [] => "qsmxt".to_string(),
        [one] => one.key.to_string(),
        [first, rest @ ..] => format!("{}_and_{}_more", first.key, rest.len()),
    };
    derivatives_dir.join("logs").join(format!("{name}.log"))
}

/// The dataset-level files written whatever happens to the runs: the BIDS derivative
/// description (required for a valid derivative dataset), `.bidsignore` and README, the resolved
/// configuration, and the methods text and pipeline graph as planned for `runs`. Once the runs
/// are done, [`write_dataset_summary`] replaces the last two with what the runs did.
pub(crate) fn write_dataset_files(
    derivatives_dir: &std::path::Path, bids_dir: &std::path::Path, config: &PipelineConfig,
    runs: &[discovery::QsmRun],
) -> crate::Result<()> {
    crate::bids::dataset_description::write(derivatives_dir, bids_dir)?;
    crate::bids::dataset_description::write_bidsignore(derivatives_dir)?;
    crate::bids::dataset_description::write_readme(derivatives_dir)?;
    std::fs::write(derivatives_dir.join("pipeline_config.toml"), config.to_toml().unwrap_or_default())?;
    std::fs::write(derivatives_dir.join("methods.md"), crate::pipeline::methods::generate_methods(config))?;
    if let Some(graph) = dataset_graph(config, runs) {
        crate::bids::derivatives::write_graph(&DerivativeOutputs::new(derivatives_dir).dataset_graph_path(), &graph)?;
    }
    Ok(())
}

/// The dataset's methods text and pipeline graph from what its runs did
/// ([`qsmxt_engine::dataset::summarise`]), over every current run record in the output
/// directory ([`current_run_records`]), whichever invocation processed it. `methods.md` and
/// `pipeline.mmd`/`.svg` are replaced, and `changes/` is rebuilt to hold the runs that are
/// exceptions among them: each one's methods text and its graph as it ran. With no current
/// record, the planned files stay.
pub(crate) fn write_dataset_summary(derivatives_dir: &std::path::Path, config: &PipelineConfig) -> crate::Result<()> {
    use crate::bids::derivatives::write_graph;
    let output = DerivativeOutputs::new(derivatives_dir);
    let done: Vec<(String, qsmxt_engine::plan::Plan, qsmxt_engine::scan::ScanFacts, qsmxt_engine::record::RunRecord)> =
        current_run_records(derivatives_dir, config).into_iter()
            .map(|(name, file)| (name, qsmxt_engine::plan::plan(config, &file.scan), file.scan, file.record))
            .collect();
    let inputs: Vec<_> = done.iter().map(|(_, p, f, r)| (p, f, r)).collect();
    let Some(summary) = qsmxt_engine::dataset::summarise(config, &inputs, "qsmxt") else { return Ok(()) };
    std::fs::write(derivatives_dir.join("methods.md"), summary.methods.to_markdown())?;
    write_graph(&output.dataset_graph_path(), &summary.graph)?;
    let changes = output.changes_dir();
    if changes.exists() {
        std::fs::remove_dir_all(&changes)?;
    }
    for &i in &summary.changed {
        let name = &done[i].0;
        std::fs::create_dir_all(&changes)?;
        std::fs::write(output.change_path(name, false), summary.runs[i].methods.to_markdown())?;
        write_graph(&output.change_path(name, true), &summary.runs[i].after)?;
    }
    info!("Methods and pipeline graph from {} run(s)", done.len());
    if !summary.changed.is_empty() {
        info!("{} run(s) processed differently from the rest -> {}", summary.changed.len(), changes.display());
    }
    Ok(())
}

/// Every current run record under `workflow/` (`*_runrecord.json`), by the run's name (its BIDS
/// basename), in path order. A record is current when the pipeline state beside it
/// (`.pipeline_state.json`) says its run completed under `config`: a run since processed with
/// other settings, or one whose latest attempt did not finish, is left out, so a record can
/// only describe outputs that are there now. Unreadable records, and records written before
/// they carried the run's scan facts, are left out with a warning.
pub(crate) fn current_run_records(
    derivatives_dir: &std::path::Path, config: &PipelineConfig,
) -> Vec<(String, crate::pipeline::runner::RunRecordFile)> {
    const SUFFIX: &str = "_runrecord.json";
    let pattern = derivatives_dir.join("workflow").join("**").join(format!("*{SUFFIX}"));
    let mut paths: Vec<std::path::PathBuf> = glob::glob(&pattern.to_string_lossy())
        .map(|g| g.flatten().collect()).unwrap_or_default();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(SUFFIX)) else { continue };
        let state = path.with_file_name(".pipeline_state.json");
        if let Err(why) = crate::pipeline::graph::state_completed_under(&state, config) {
            log::debug!("{name}: its run record is not current: the run {why}");
            continue;
        }
        match std::fs::read_to_string(&path).map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<crate::pipeline::runner::RunRecordFile>(&t).map_err(|e| e.to_string())) {
            Ok(file) => out.push((name.to_string(), file)),
            Err(e) => warn!("{name}: its run record cannot be read ({}: {e}); re-run it to include it", path.display()),
        }
    }
    out
}

/// The pipeline the dataset's runs will take, as planned from each run's scan: the graph they
/// share, or, where their scans make them differ, all of them merged
/// ([`qsmxt_engine::pipeline_graph::PipelineGraph::merged`]).
fn dataset_graph(config: &PipelineConfig, runs: &[discovery::QsmRun]) -> Option<qsmxt_engine::pipeline_graph::PipelineGraph> {
    let graphs: Vec<_> = runs.iter().map(|run| {
        let facts = crate::pipeline::requirements::scan_facts(run, config);
        let plan = qsmxt_engine::plan::plan(config, &facts);
        qsmxt_engine::pipeline_graph::pipeline_graph(&plan, config, &facts, None)
    }).collect();
    qsmxt_engine::pipeline_graph::PipelineGraph::merged(&graphs)
}

/// One row per referenced susceptibility map of each run: what it was referenced to, and what
/// that removed.
///
/// The same facts are in each map's JSON sidecar, but reading forty sidecars to find the subject
/// whose reference region came out empty is not a thing anyone does. This is the file you scan.
fn write_reference_summary(
    derivatives_dir: &std::path::Path, output: &DerivativeOutputs,
    keys: &[crate::bids::entities::AcquisitionKey],
) {
    let mut body = String::new();
    for k in keys {
        // One row per referenced map — the two-pass single-pass map has its own offset — each
        // read from that map's own sidecar.
        for spec in crate::pipeline::stats::candidate_maps(&output.published(k), k) {
            if !crate::pipeline::stats::is_susceptibility(spec.name) {
                continue;
            }
            let Some(sidecar) = crate::bids::entities::sidecar_path(&spec.path) else { continue };
            let Ok(text) = std::fs::read_to_string(&sidecar) else { continue };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let Some(reference) = v.get("QsmReference").and_then(|r| r.as_str()) else { continue };
            let num = |k: &str| v.get(k).and_then(|x| x.as_f64())
                .map(|x| format!("{x:.6}")).unwrap_or_default();
            let row = [
                k.subject.as_str(),
                k.session.as_deref().unwrap_or(""),
                k.acquisition.as_deref().unwrap_or(""),
                k.reconstruction.as_deref().unwrap_or(""),
                k.inversion.as_deref().unwrap_or(""),
                k.run.as_deref().unwrap_or(""),
                spec.name,
                reference,
                &num("QsmReferenceOffsetPpm"),
                &v.get("QsmReferenceVoxels").and_then(|x| x.as_u64())
                    .map(|x| x.to_string()).unwrap_or_default(),
            ];
            body.push_str(&row.join("\t"));
            body.push('\n');
        }
    }
    if body.is_empty() {
        return;
    }
    let path = derivatives_dir.join("desc-reference_summary.tsv");
    let header = REFERENCE_SUMMARY_COLUMNS.join("\t");
    match std::fs::write(&path, format!("{header}\n{body}")) {
        Ok(()) => info!("QSM reference per run -> {}", path.display()),
        Err(e) => warn!("Could not write the reference summary: {}", e),
    }
}

/// Columns of `desc-reference_summary.tsv`: the run's entities, which map (its BIDS suffix with
/// any `desc-`, as in the statistics table), and what it was referenced to.
pub(crate) const REFERENCE_SUMMARY_COLUMNS: [&str; 10] = [
    "subject", "session", "acquisition", "reconstruction", "inversion", "run",
    "map", "reference", "offset_ppm", "n_voxels",
];

/// `--clean-intermediates` for one completed run: its workflow images, and the working-grid copies
/// of outputs returned to the acquired grid. Best-effort: a run whose outputs are all written must
/// not fail because a temporary file could not be removed.
pub(crate) fn clean_run(output: &DerivativeOutputs, run: &discovery::QsmRun) {
    if let Err(e) = crate::pipeline::graph::clean_intermediates(&output.output_dir, &run.key) {
        warn!("{}: could not clean the intermediates: {}", run.key, e);
    }
    let stash = output.working_grid_dir(&run.key);
    if stash.exists() {
        if let Err(e) = std::fs::remove_dir_all(&stash) {
            warn!("{}: could not remove {}: {}", run.key, stash.display(), e);
        }
    }
}

/// Draw the cohort figures from the dataset-level table.
///
/// Each bar is the mean of the runs' means and its whisker is the spread *across* runs — a
/// different quantity from the per-run figures, whose whiskers are the spread of voxels inside one
/// structure. The subtitle says which, because the two look identical and a reader would otherwise
/// take cohort variability for within-structure variability.
///
/// Best-effort throughout: the table is the deliverable and is already written.
fn write_group_figures(derivatives_dir: &std::path::Path, table_path: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(table_path) else { return };
    let (rows, n_runs) = crate::pipeline::figure::aggregate_runs(
        &crate::pipeline::stats::parse_tsv(&text));
    if rows.is_empty() || n_runs < 2 {
        // With a single run the cohort view would just restate that run's figure, with every
        // whisker at zero.
        return;
    }
    let subtitle = format!("Mean of {n_runs} run means; whiskers ±1 SD across runs");
    let vol_subtitle = format!("Mean volume over {n_runs} runs; ± is 1 SD across runs");
    match crate::pipeline::figure::render_all(
        &rows, &format!("All runs (n = {n_runs})"), &subtitle, &vol_subtitle) {
        Ok(figs) => for f in figs {
            match f.save_in(derivatives_dir, "") {
                Ok(p) => info!("Cohort figure -> {}", p.display()),
                Err(e) => warn!("Could not write {}: {}", f.stem, e),
            }
        },
        Err(e) => warn!("Could not draw the cohort figures: {}", e),
    }
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
    // show what a real run would do, warnings included. A worker (`--no-dataset-steps`) holds its
    // lines until it knows which runs it has, which name its log.
    if args.dry {
        init_logging(LogSink::Discard, args.debug);
    } else if args.no_dataset_steps {
        std::fs::create_dir_all(&derivatives_dir)?;
        init_logging(LogSink::Buffer(Vec::new()), args.debug);
    } else {
        std::fs::create_dir_all(&derivatives_dir)?;
        let log_file = std::fs::File::create(derivatives_dir.join("qsmxt.log"))?;
        init_logging(LogSink::File(log_file), args.debug);
    }

    if args.no_dataset_steps && args.clean_intermediates {
        return Err(crate::error::QsmxtError::Config(
            "--clean-intermediates runs after the dataset-level steps, which --no-dataset-steps \
             skips (a multi-orientation group still needs its members' intermediates); pass it to \
             `qsmxt aggregate` instead".to_string(),
        ));
    }

    // Build config: file -> CLI overrides -> checks
    let mut config = build_config(args.config.as_deref(), &args.pipeline)?;

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

    // What the chosen methods' rules recommend is only ever applied when asked, before the
    // checks (a required one would otherwise refuse the run).
    let as_configured = config.clone();
    let accepted = if args.accept_recommendations {
        super::recommend::accept(&mut config, &runs)
    } else {
        super::recommend::Accepted::default()
    };

    // Every requested output every run can produce, before any run starts (#193, #273).
    crate::pipeline::requirements::check_runs(&runs, &config)?;

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
        Some((gb * 1024.0 * 1024.0 * 1024.0) as u64)
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
            println!("  Inhomogeneity:     {}", config.masking.inhomogeneity_method);
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
        if let Err(e) = print_orientation_groups(&runs, &config) {
            eprintln!("  orientation grouping: {e}");
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
                .map(|v| (v.max(1) as usize).min(n_procs))
                .unwrap_or(n_procs);
            println!();
            println!(
                "Memory: {} available, max {} concurrent run(s)",
                memory::format_bytes(mem),
                max_concurrent,
            );
        }
        print_planned(&derivatives_dir, &config, &runs);
        super::recommend::print_dry(&as_configured, &runs, args.accept_recommendations);
        return Ok(());
    }
    super::recommend::log_found(&config, &runs);

    if args.no_dataset_steps {
        let path = worker_log_path(&derivatives_dir, &runs);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        attach_log_file(std::fs::File::create(&path)?)?;
    }

    // Log version info
    info!("qsmxt {}", env!("CARGO_PKG_VERSION"));
    info!("QSM.rs {}", env!("QSM_CORE_DESCRIBE"));
    info!(
        "Processing {} run(s) across {} subject(s)",
        runs.len(),
        subjects.len()
    );

    let output = DerivativeOutputs::new(&derivatives_dir);

    if !args.no_dataset_steps {
        write_dataset_files(&derivatives_dir, &args.bids_dir, &config, &runs)?;
    }

    // Execute
    let exec_config = executor::local::ExecutionConfig {
        n_procs,
        mem_limit_bytes,
        force: args.force,
        source_dicom: args.source_dicom.clone(),
        dicom_outputs: args.dicom_outputs.clone(),
        applied: accepted.per_run.clone(),
    };

    let results = executor::local::execute_local(&runs, &config, &output, &exec_config);
    let run_failures: Vec<String> = runs.iter().zip(&results)
        .filter_map(|(run, r)| r.as_ref().err().map(|e| format!("{}: {e}", run.key))).collect();

    if args.no_dataset_steps {
        info!("Dataset-level steps skipped (--no-dataset-steps): run `qsmxt aggregate` once every run is done");
        super::recommend::log_summary(&config, &runs, &accepted);
        return finish(&run_failures, &[], &derivatives_dir);
    }

    let completed: Vec<bool> = results.iter().map(|r| r.is_ok()).collect();
    let group_failures = run_dataset_steps(&DatasetSteps {
        derivatives_dir: &derivatives_dir,
        runs: &runs,
        completed: &completed,
        config: &config,
        force: args.force,
        clean_intermediates: args.clean_intermediates,
        command_line: &std::env::args().collect::<Vec<_>>().join(" "),
    });
    super::recommend::log_summary(&config, &runs, &accepted);
    finish(&run_failures, &group_failures, &derivatives_dir)
}

/// What the dataset-level steps work from.
pub(crate) struct DatasetSteps<'a> {
    pub derivatives_dir: &'a std::path::Path,
    /// Every run of the dataset (as discovered), in discovery order.
    pub runs: &'a [discovery::QsmRun],
    /// Whether each of `runs` completed its per-run pipeline.
    pub completed: &'a [bool],
    pub config: &'a PipelineConfig,
    /// Recompute the multi-orientation groups rather than reuse them.
    pub force: bool,
    pub clean_intermediates: bool,
    /// The command recorded in the provenance.
    pub command_line: &'a str,
}

/// Everything after the per-run pipelines: the multi-orientation fan-in, the reference summary,
/// the dataset-level statistics table and cohort figures, BIDS-Prov, and `--clean-intermediates`.
///
/// `qsmxt run` calls this once its runs are done, and `qsmxt aggregate` (the final SLURM job)
/// calls it over runs processed by `qsmxt run --no-dataset-steps`, so a distributed run and a
/// local one cannot drift apart. Returns the orientation groups that failed.
pub(crate) fn run_dataset_steps(steps: &DatasetSteps) -> Vec<String> {
    let DatasetSteps { derivatives_dir, runs, completed, config, force, clean_intermediates, command_line } = *steps;
    let output = DerivativeOutputs::new(derivatives_dir);

    // The methods text and pipeline graph from what the runs did, and the runs that differ.
    // Best-effort: the planned ones are already written.
    if let Err(e) = write_dataset_summary(derivatives_dir, config) {
        warn!("Could not describe the dataset from its runs' records: {}", e);
    }

    // Multi-orientation fan-in. Members have just produced their own local fields (and their
    // own single-orientation maps, which are what you check registration against), so the
    // groups can now be combined — before the tables below, which include the group maps. A
    // group that fails is reported and the rest continue: one bad direction table should not
    // throw away a dataset's worth of work.
    let mut group_failures = Vec::new();
    let mut prov_groups = Vec::new();
    let mut group_keys: Vec<crate::bids::entities::AcquisitionKey> = Vec::new();
    if config.multi_orientation.enabled() {
        match crate::bids::orientation::parse_pattern(&config.multi_orientation.group_by) {
            Err(e) => group_failures.push(e),
            Ok(None) => {}
            Ok(Some(pattern)) => {
                let groups = crate::bids::orientation::group_runs(runs, &pattern);
                if groups.is_empty() {
                    warn!(
                        "--orientation-group '{}' matched no set of 2+ runs in a session; \
                        everything was processed one orientation at a time",
                        config.multi_orientation.group_by
                    );
                }
                for group in &groups {
                    let members: Vec<&discovery::QsmRun> =
                        group.members.iter().map(|&i| &runs[i]).collect();
                    let progress = |msg: &str| info!("{msg}");
                    match crate::pipeline::multiorient::reconstruct_group(
                        group, &members, config, &output, force, &progress,
                    ) {
                        Ok(paths) => {
                            for p in paths {
                                info!("  wrote {}", p.display());
                            }
                            group_keys.push(crate::pipeline::multiorient::group_key(&members));
                            prov_groups.push(crate::bids::bidsprov::ProvGroup {
                                key: crate::pipeline::multiorient::group_key(&members),
                                magnetic_field_strength: members[0].magnetic_field_strength,
                            });
                        }
                        Err(e) => {
                            error!("{}: {}", group.label, e);
                            group_failures.push(format!("{}: {}", group.label, e));
                        }
                    }
                }
            }
        }
    }

    // Every key with outputs: the runs, then the orientation groups.
    let all_keys: Vec<crate::bids::entities::AcquisitionKey> = runs.iter().map(|r| r.key.clone())
        .chain(group_keys.iter().cloned()).collect();

    // What each run's map was referenced to — written whether or not the statistics were asked
    // for, since it describes the susceptibility maps themselves.
    if config.pipeline.do_qsm {
        write_reference_summary(derivatives_dir, &output, &all_keys);
    }

    // Gather every run's per-structure table into one dataset-level TSV. Best-effort, and after
    // the runs rather than inside them: it is a convenience view of files already written, so a
    // failure here must not fail an otherwise-successful run.
    if config.pipeline.do_analysis {
        let keys: Vec<_> = all_keys.iter().collect();
        match crate::pipeline::stats::write_group_table(derivatives_dir, &output, &keys) {
            Ok(Some(path)) => {
                info!("Per-structure statistics for all runs -> {}", path.display());
                write_group_figures(derivatives_dir, &path);
            }
            Ok(None) => {}
            Err(e) => warn!("Failed to write the dataset-level statistics table: {}", e),
        }
    }

    // Write BEP028 (BIDS-Prov) records and per-output GeneratedBy sidecars — last, so the
    // multi-orientation outputs are covered too. Best-effort: provenance output must never fail
    // an otherwise-successful run.
    if let Err(e) = crate::bids::bidsprov::write_provenance(
        derivatives_dir, runs, &prov_groups, &output, command_line,
    ) {
        warn!("Failed to write BIDS-Prov provenance records: {}", e);
    }

    // Last of all, once nothing else will read them (the multi-orientation fan-in reads each
    // member's local field): the intermediates of every run that completed.
    if clean_intermediates {
        for (run, &done) in runs.iter().zip(completed) {
            if done {
                clean_run(&output, run);
            }
        }
    }
    group_failures
}

/// Report what failed, as the command's result.
pub(crate) fn finish(
    run_failures: &[String], group_failures: &[String], derivatives_dir: &std::path::Path,
) -> crate::Result<()> {
    if !run_failures.is_empty() {
        error!("{} run(s) failed:", run_failures.len());
        for f in run_failures {
            error!("  {}", f);
        }
        return Err(crate::error::QsmxtError::Algorithm {
            stage: "pipeline".to_string(),
            message: format!("{} run(s) failed: {}", run_failures.len(), run_failures.join("; ")),
        });
    }

    if !group_failures.is_empty() {
        error!("{} orientation group(s) failed:", group_failures.len());
        for f in group_failures {
            error!("  {}", f);
        }
        return Err(crate::error::QsmxtError::Algorithm {
            stage: "multi-orientation".to_string(),
            message: format!("{} orientation group(s) failed: {}", group_failures.len(), group_failures.join("; ")),
        });
    }

    info!("All runs completed successfully — results in {}", derivatives_dir.display());
    Ok(())
}


/// What a `--dry` run shows of the methods text and pipeline graph a run would write, as planned
/// from the configuration and each run's scan. Nothing is written: a dry run leaves the dataset
/// alone.
fn print_planned(derivatives_dir: &std::path::Path, config: &PipelineConfig, runs: &[discovery::QsmRun]) {
    let output = DerivativeOutputs::new(derivatives_dir);
    if let Some(graph) = dataset_graph(config, runs) {
        println!();
        println!("Pipeline graph (a run writes it to {} and .svg):", output.dataset_graph_path().display());
        print!("{}", graph.to_text());
    }
    println!();
    println!("Methods (a run writes them to {}):", derivatives_dir.join("methods.md").display());
    println!();
    print!("{}", crate::pipeline::methods::generate_methods(config));
}

/// Print the orientation groups a `--dry` run would reconstruct, with the direction table and
/// the go/no-go verdict for each.
///
/// This is where a multi-orientation run should be checked before it is started. The TUI shows
/// which runs group together; only here are the B0 directions actually read, and they are what
/// decides whether the reconstruction means anything.
fn print_orientation_groups(
    runs: &[discovery::QsmRun],
    config: &crate::pipeline::config::PipelineConfig,
) -> Result<(), String> {
    if !config.multi_orientation.enabled() {
        return Ok(());
    }
    let kind = crate::pipeline::multiorient::kind_of(config);
    let pattern = crate::bids::orientation::parse_pattern(&config.multi_orientation.group_by)?
        .ok_or_else(|| "pattern is empty".to_string())?;
    let groups = crate::bids::orientation::group_runs(runs, &pattern);

    println!();
    if groups.is_empty() {
        println!(
            "Multi-orientation ({kind}): '{}' matched no set of 2+ runs in a session — every \
            run would be processed on its own",
            config.multi_orientation.group_by
        );
        return Ok(());
    }
    println!("Multi-orientation ({kind}), {} group(s):", groups.len());
    for group in &groups {
        let members: Vec<&discovery::QsmRun> = group.members.iter().map(|&i| &runs[i]).collect();
        use crate::pipeline::multiorient::RegistrationPlan;
        let (orientations, check, plan) = crate::pipeline::multiorient::preview_group(
            &members, kind, config.multi_orientation.register,
        );
        // A group that will be registered must not be marked REFUSED on directions read off
        // affines that registration is about to replace — that is the whole point of the plan.
        let mark = match plan {
            RegistrationPlan::WillRegister => "will co-register",
            RegistrationPlan::Disabled => "REFUSED",
            RegistrationPlan::NotNeeded if check.is_ok() => "ok",
            RegistrationPlan::NotNeeded => "REFUSED",
        };
        println!("  {} ({} orientations) [{mark}]", group.label, members.len());
        for row in crate::multiorient::direction_table(&orientations) {
            println!("      {row}");
        }
        match plan {
            RegistrationPlan::WillRegister => {
                println!(
                    "      directions above are from the affines; they will be replaced by ones \
                     recovered from the registration against {}",
                    members[0].key
                );
            }
            RegistrationPlan::Disabled => {
                println!(
                    "      needs co-registration, but --no-orientation-registration is set — \
                     this group will be refused"
                );
                println!("      {}", check.summary());
            }
            RegistrationPlan::NotNeeded => println!("      {}", check.summary()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    fn pipeline_args(argv: &[&str]) -> crate::cli::PipelineArgs {
        let mut full = vec!["qsmxt", "run", "<bids>"];
        full.extend_from_slice(argv);
        match crate::cli::Cli::try_parse_from(full).expect("clap accepts it").command {
            crate::cli::Command::Run(a) => a.pipeline,
            _ => panic!("expected run"),
        }
    }

    /// A value outside a setting's range is refused before the run, from the command line and
    /// from a config file alike, naming the setting, its value and its range.
    #[test]
    fn a_setting_out_of_range_is_refused_before_the_run() {
        let e = super::build_config(None, &pipeline_args(&["--qsm-algorithm", "tkd", "--tkd-threshold", "0"]))
            .unwrap_err().to_string();
        assert!(e.contains("inversion.tkd.threshold (--tkd-threshold) is 0.0; it must be from 0 to 2/3 (not 0 or 2/3)"), "{e}");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.toml");
        std::fs::write(&path, "[bg_removal.vsharp]\nmin_radius = 0.0\n").unwrap();
        let e = super::build_config(Some(&path), &pipeline_args(&[])).unwrap_err().to_string();
        assert!(e.contains("bg_removal.vsharp.min_radius"), "{e}");

        // In range, or set on a method that does not run: accepted.
        assert!(super::build_config(None, &pipeline_args(&["--tkd-threshold", "0"])).is_ok());
        assert!(super::build_config(None, &pipeline_args(&[])).is_ok());
    }
}
