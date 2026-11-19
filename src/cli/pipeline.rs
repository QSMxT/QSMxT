use clap::Parser;
use std::path::PathBuf;

use super::*;

// ─── Pipeline commands ───

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt/)
    pub output_dir: Option<PathBuf>,

    /// Pipeline configuration file (TOML)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Include only runs matching these glob patterns (e.g. "sub-1*" "*ses-pre*")
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns (e.g. "*mygrea*")
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,

    /// Limit number of echoes to process
    #[arg(long)]
    pub num_echoes: Option<usize>,

    #[command(flatten)]
    pub pipeline: crate::cli_settings::SettingsArgs,

    /// Number of parallel threads
    #[arg(long)]
    pub n_procs: Option<usize>,

    /// Optional source DICOM directory; inherit patient/study identity from the
    /// original DICOMs when exporting (used with --export-dicom)
    #[arg(long)]
    pub source_dicom: Option<PathBuf>,

    /// Restrict DICOM export to these maps (default: all produced); repeat the flag or
    /// comma-separate them (used with --export-dicom)
    #[arg(long, value_delimiter = ',', action = clap::ArgAction::Append, ignore_case = true,
          value_parser = clap::builder::PossibleValuesParser::new(DICOM_OUTPUT_TOKENS))]
    pub dicom_outputs: Option<Vec<String>>,

    /// Print the processing plan, the pipeline graph and the methods text without running
    /// anything or writing any file. Also lists what the configuration departs from in the
    /// chosen methods' reference pipelines, for every run's scan, and each covered setting's
    /// reference value; with --accept-recommendations or --accept-reference-settings, the
    /// settings that would change
    #[arg(long)]
    pub dry: bool,

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

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,

    /// Memory limit in GB for concurrent run scheduling (if not specified: the memory available,
    /// within a Slurm job's or container's memory limit when there is one)
    #[arg(long)]
    pub mem_limit_gb: Option<f64>,

    /// Disable memory-aware concurrency limiting
    #[arg(long)]
    pub no_mem_limit: bool,

    /// Force re-run, ignoring cached pipeline state
    #[arg(long)]
    pub force: bool,

    /// Once every run (and any multi-orientation combination) is done, delete the intermediate
    /// images under workflow/. Published outputs, per-step provenance and the cache state are
    /// kept: a re-run with the same configuration recomputes nothing, one with a changed
    /// configuration recomputes the intermediates it needs
    #[arg(long)]
    pub clean_intermediates: bool,

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
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt/)
    pub output_dir: Option<PathBuf>,

    /// Pipeline configuration file (TOML): the one the runs were processed with
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Include only runs matching these glob patterns (e.g. "sub-1*" "*ses-pre*")
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns (e.g. "*mygrea*")
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,

    /// Limit number of echoes to process
    #[arg(long)]
    pub num_echoes: Option<usize>,

    #[command(flatten)]
    pub pipeline: crate::cli_settings::SettingsArgs,

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,

    /// Recompute the multi-orientation reconstructions, ignoring their cached state
    #[arg(long)]
    pub force: bool,

    /// Once the dataset-level steps are done, delete the intermediate images under workflow/
    /// (as `qsmxt run --clean-intermediates`)
    #[arg(long)]
    pub clean_intermediates: bool,

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

    /// Include only runs matching these glob patterns
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,
}

#[derive(Parser, Debug)]
pub struct SlurmArgs {
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt/)
    pub output_dir: Option<PathBuf>,

    /// SLURM account name
    #[arg(long)]
    pub account: String,

    /// SLURM partition
    #[arg(long)]
    pub partition: Option<String>,

    /// Pipeline configuration file (TOML)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Wall time limit (e.g., 02:00:00)
    #[arg(long, default_value = "02:00:00")]
    pub time: String,

    /// Memory per job in GB [default: estimated per run from its size and the pipeline]
    #[arg(long)]
    pub mem: Option<usize>,

    /// CPUs per task
    #[arg(long, default_value_t = 4)]
    pub cpus_per_task: usize,

    /// Auto-submit scripts via sbatch
    #[arg(long)]
    pub submit: bool,

    /// Include only runs matching these glob patterns (e.g. "sub-1*" "*ses-pre*")
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns (e.g. "*mygrea*")
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,

    /// Limit number of echoes to process
    #[arg(long)]
    pub num_echoes: Option<usize>,

    /// Force re-run, ignoring cached pipeline state (passed to every job)
    #[arg(long)]
    pub force: bool,

    /// Once every run and the dataset-level steps are done, delete the intermediate images
    /// under workflow/ (done by the aggregation job)
    #[arg(long)]
    pub clean_intermediates: bool,

    /// Enable debug logging (in every job)
    #[arg(long)]
    pub debug: bool,

    /// Optional source DICOM directory; inherit patient/study identity from the original DICOMs
    /// when exporting (used with --export-dicom; each run's job exports its maps)
    #[arg(long)]
    pub source_dicom: Option<PathBuf>,

    /// Restrict DICOM export to these maps (default: all produced); repeat the flag or
    /// comma-separate them (used with --export-dicom)
    #[arg(long, value_delimiter = ',', action = clap::ArgAction::Append, ignore_case = true,
          value_parser = clap::builder::PossibleValuesParser::new(DICOM_OUTPUT_TOKENS))]
    pub dicom_outputs: Option<Vec<String>>,

    /// Apply the recommended value of every reasoned recommendation that is not dismissed
    /// before writing the jobs (as `qsmxt run --accept-recommendations`)
    #[arg(long)]
    pub accept_recommendations: bool,

    /// Apply the original authors' settings of the chosen methods before writing the jobs (as
    /// `qsmxt run --accept-reference-settings`)
    #[arg(long)]
    pub accept_reference_settings: bool,

    #[command(flatten)]
    pub pipeline: crate::cli_settings::SettingsArgs,
}
