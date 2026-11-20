use clap::{Args, Parser};
use std::path::PathBuf;

use super::*;

// ─── Pipeline commands ───

/// Which runs of a dataset to process: the `--include` and `--exclude` patterns.
#[derive(Args, Debug, Default)]
pub struct RunFilterArgs {
    /// Include only runs matching these glob patterns (e.g. "sub-1*" "*ses-pre*")
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns (e.g. "*mygrea*")
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,
}

/// The dataset a pipeline command works on: where it is and its outputs go, the configuration
/// file, and which runs and echoes.
#[derive(Args, Debug, Default)]
pub struct DatasetArgs {
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt/)
    pub output_dir: Option<PathBuf>,

    /// Pipeline configuration file (TOML)
    #[arg(long)]
    pub config: Option<PathBuf>,

    #[command(flatten)]
    pub filter: RunFilterArgs,

    /// Limit number of echoes to process
    #[arg(long)]
    pub num_echoes: Option<usize>,
}

impl DatasetArgs {
    /// Where the outputs go: `<output_dir or bids_dir>/derivatives/qsmxt/`'s parent.
    pub fn base_dir(&self) -> &std::path::Path {
        self.output_dir.as_deref().unwrap_or(&self.bids_dir)
    }

    /// The runs to discover.
    pub fn discovery_filter(&self) -> qsmxt_io::bids::discovery::DiscoveryFilter {
        qsmxt_io::bids::discovery::DiscoveryFilter {
            include: self.filter.include.clone(),
            exclude: self.filter.exclude.clone(),
            num_echoes: self.num_echoes,
        }
    }
}

/// How a pipeline command runs: the cache, the log and the intermediates.
#[derive(Args, Debug, Default)]
pub struct ExecArgs {
    /// Force re-run, ignoring cached pipeline state
    #[arg(long)]
    pub force: bool,

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,

    /// Once every run (and any multi-orientation combination) is done, delete the intermediate
    /// images under workflow/. Published outputs, per-step provenance and the cache state are
    /// kept: a re-run with the same configuration recomputes nothing, one with a changed
    /// configuration recomputes the intermediates it needs
    #[arg(long)]
    pub clean_intermediates: bool,
}

/// DICOM export's run options (the export itself is `--export-dicom`, a setting).
#[derive(Args, Debug, Default)]
pub struct DicomExportArgs {
    /// Optional source DICOM directory; inherit patient/study identity from the
    /// original DICOMs when exporting (used with --export-dicom)
    #[arg(long)]
    pub source_dicom: Option<PathBuf>,

    /// Restrict DICOM export to these maps (default: all produced); repeat the flag or
    /// comma-separate them (used with --export-dicom)
    #[arg(long, value_delimiter = ',', action = clap::ArgAction::Append, ignore_case = true,
          value_parser = clap::builder::PossibleValuesParser::new(DICOM_OUTPUT_TOKENS))]
    pub dicom_outputs: Option<Vec<String>>,
}

/// Which of the chosen methods' recommendations to apply before running.
#[derive(Args, Debug, Default, Clone, Copy)]
pub struct AcceptArgs {
    /// Apply the recommended value of every reasoned recommendation that is not dismissed
    /// (required, recommended and discouraged findings, each with its reason) before running.
    /// The original authors' settings are not applied; see --accept-reference-settings. Two
    /// equally strong recommendations that disagree on a setting are left for you to choose.
    /// Without it, no setting is ever changed
    #[arg(long)]
    pub accept_recommendations: bool,

    /// Apply the original authors' settings of the chosen methods (what their code, paper or
    /// README does, where no reason is given that it is better) before running, so the run
    /// follows the methods' reference pipelines. Two methods whose authors' settings disagree on
    /// a setting are left for you to choose; a reasoned recommendation on the same setting wins
    /// when --accept-recommendations is also given
    #[arg(long)]
    pub accept_reference_settings: bool,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    #[command(flatten)]
    pub dataset: DatasetArgs,

    #[command(flatten)]
    pub pipeline: crate::cli_settings::SettingsArgs,

    /// Number of parallel threads
    #[arg(long)]
    pub n_procs: Option<usize>,

    #[command(flatten)]
    pub dicom: DicomExportArgs,

    /// Print the processing plan, the pipeline graph and the methods text without running
    /// anything or writing any file. Also lists what the configuration departs from in the
    /// chosen methods' reference pipelines, for every run's scan, and each covered setting's
    /// reference value; with --accept-recommendations or --accept-reference-settings, the
    /// settings that would change
    #[arg(long)]
    pub dry: bool,

    #[command(flatten)]
    pub accept: AcceptArgs,

    #[command(flatten)]
    pub exec: ExecArgs,

    /// Memory limit in GB for concurrent run scheduling (if not specified: the memory available,
    /// within a Slurm job's or container's memory limit when there is one)
    #[arg(long)]
    pub mem_limit_gb: Option<f64>,

    /// Disable memory-aware concurrency limiting
    #[arg(long)]
    pub no_mem_limit: bool,

    /// Process each run and stop: skip the dataset-level steps (multi-orientation
    /// reconstruction, summary tables, cohort figures, provenance, dataset_description.json,
    /// pipeline_config.toml, methods.md, the pipeline graph, changes/) and log to
    /// logs/<run>.log instead of qsmxt.log.
    /// `qsmxt aggregate` does those steps once every run is done; `qsmxt slurm` uses the pair
    #[arg(long, conflicts_with = "dry")]
    pub no_dataset_steps: bool,
}

#[derive(Parser, Debug)]
pub struct AggregateArgs {
    /// The dataset, configuration and runs the workers processed
    #[command(flatten)]
    pub dataset: DatasetArgs,

    #[command(flatten)]
    pub pipeline: crate::cli_settings::SettingsArgs,

    #[command(flatten)]
    pub exec: ExecArgs,

    /// The command recorded as the provenance Activity's `Command` (default: this command).
    /// `qsmxt slurm` sets it to the `qsmxt slurm ...` invocation that produced the outputs
    #[arg(long, hide = true)]
    pub command_line: Option<String>,
}

#[derive(Parser, Debug)]
pub struct InitArgs {
    /// Output file path (prints to stdout if not specified)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct ValidateArgs {
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    #[command(flatten)]
    pub filter: RunFilterArgs,
}

#[derive(Parser, Debug)]
pub struct SlurmArgs {
    #[command(flatten)]
    pub dataset: DatasetArgs,

    /// SLURM account name
    #[arg(long)]
    pub account: String,

    /// SLURM partition
    #[arg(long)]
    pub partition: Option<String>,

    /// Wall time limit (e.g., 02:00:00)
    #[arg(long, default_value = qsmxt_io::executor::slurm::DEFAULT_TIME)]
    pub time: String,

    /// Memory per job in GB [default: estimated per run from its size and the pipeline]
    #[arg(long)]
    pub mem: Option<usize>,

    /// CPUs per task
    #[arg(long, default_value_t = qsmxt_io::executor::slurm::DEFAULT_CPUS)]
    pub cpus_per_task: usize,

    /// Auto-submit scripts via sbatch
    #[arg(long)]
    pub submit: bool,

    /// Passed to every job; the intermediates are cleaned by the aggregation job
    #[command(flatten)]
    pub exec: ExecArgs,

    #[command(flatten)]
    pub dicom: DicomExportArgs,

    #[command(flatten)]
    pub accept: AcceptArgs,

    #[command(flatten)]
    pub pipeline: crate::cli_settings::SettingsArgs,
}
