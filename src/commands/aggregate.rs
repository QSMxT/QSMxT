//! `qsmxt aggregate`: the dataset-level steps over runs already processed one by one.
//!
//! `qsmxt slurm` runs each run as its own job (`qsmxt run --no-dataset-steps`), then this as a
//! final job that depends on all of them. It does exactly what a local `qsmxt run` does after its
//! runs finish — through the same [`run::run_dataset_steps`] — so the two produce the same
//! dataset-level files.

use log::{error, info};

use super::run;
use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::{self, DiscoveryFilter};
use crate::cli::AggregateArgs;

pub fn execute(args: AggregateArgs) -> crate::Result<()> {
    let base_dir = args.output_dir.as_deref().unwrap_or(&args.bids_dir);
    let derivatives_dir = base_dir.join("derivatives").join("qsmxt");
    std::fs::create_dir_all(&derivatives_dir)?;

    // Held until qsmxt.log is opened below, where the workers' logs go first.
    run::init_logging(run::LogSink::Buffer(Vec::new()), args.debug);

    let config = run::build_config(args.config.as_deref(), &args.pipeline)?;

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

    // qsmxt.log: what each worker logged, in run order, then this step's own lines — the same
    // record a local run leaves, grouped by run rather than interleaved.
    let mut log = std::fs::File::create(derivatives_dir.join("qsmxt.log"))?;
    for path in worker_logs(&derivatives_dir, &runs) {
        use std::io::Write;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let name = path.strip_prefix(&derivatives_dir).unwrap_or(&path);
        writeln!(log, "==> {} <==", name.display())?;
        log.write_all(text.as_bytes())?;
        if !text.is_empty() && !text.ends_with('\n') {
            writeln!(log)?;
        }
    }
    run::attach_log_file(log)?;

    info!("qsmxt {}", env!("CARGO_PKG_VERSION"));
    info!("QSM.rs {}", env!("QSM_CORE_DESCRIBE"));
    info!("Dataset-level steps over {} run(s)", runs.len());

    run::write_dataset_files(&derivatives_dir, &args.bids_dir, &config)?;

    // A local run knows which of its runs succeeded; here the pipeline state says, and it has to
    // be the state of this configuration — a run processed with other settings is not one of
    // this dataset's.
    let output = DerivativeOutputs::new(&derivatives_dir);
    let mut run_failures = Vec::new();
    let completed: Vec<bool> = runs.iter().map(|r| {
        match crate::pipeline::graph::run_completed_under(&output.output_dir, &r.key, &config) {
            Ok(()) => true,
            Err(why) => {
                error!("{}: {why}", r.key);
                run_failures.push(format!(
                    "{}: {why}; process it with `qsmxt run --no-dataset-steps --include {}` \
                     and the same configuration",
                    r.key, r.key
                ));
                false
            }
        }
    }).collect();

    let command_line = args.command_line.clone()
        .unwrap_or_else(|| std::env::args().collect::<Vec<_>>().join(" "));
    let group_failures = run::run_dataset_steps(&run::DatasetSteps {
        derivatives_dir: &derivatives_dir,
        runs: &runs,
        completed: &completed,
        config: &config,
        force: args.force,
        clean_intermediates: args.clean_intermediates,
        command_line: &command_line,
    });
    run::finish(&run_failures, &group_failures, &derivatives_dir)
}

/// The worker logs (`logs/<run>.log`, or `logs/<run>_and_<n>_more.log` for a worker given
/// several runs) belonging to `runs`, in run order.
fn worker_logs(derivatives_dir: &std::path::Path, runs: &[discovery::QsmRun]) -> Vec<std::path::PathBuf> {
    let dir = derivatives_dir.join("logs");
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut names: Vec<String> = entries.flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.ends_with(".log"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for run in runs {
        let key = run.key.to_string();
        for n in &names {
            let stem = &n[..n.len() - 4];
            let own = stem == key
                || stem.strip_prefix(&key).is_some_and(|rest| rest.starts_with("_and_") && rest.ends_with("_more"));
            if own {
                out.push(dir.join(n));
            }
        }
    }
    out
}
