//! `qsmxt tui`: the TUI, then what it was left to do. The TUI builds a run's options as plain
//! data; here they become `qsmxt run`'s or `qsmxt slurm`'s arguments, and the run starts as that
//! command would.

use crate::cli::{RunArgs, SlurmArgs, UpdateArgs};
use crate::tui::{Outcome, RunOptions, SlurmOptions};

pub fn execute() -> crate::Result<()> {
    match crate::tui::run_tui()? {
        Outcome::Quit => Ok(()),
        Outcome::Update => {
            let result = super::update::execute(UpdateArgs { yes: true, force: false, main: false });
            if result.is_ok() {
                println!("\nRestart qsmxt to use the new version.");
            }
            result
        }
        Outcome::Run(options) => super::run::execute(run_args(options)),
        Outcome::Slurm(options) => super::slurm::execute(slurm_args(options)),
    }
}

/// `qsmxt run`'s arguments for the TUI's run.
pub fn run_args(o: RunOptions) -> RunArgs {
    let RunOptions {
        accept_recommendations, accept_reference_settings, bids_dir, output_dir, config, include, exclude,
        num_echoes, n_procs, source_dicom, dicom_outputs, dry, debug, mem_limit_gb, no_mem_limit, force,
        clean_intermediates, no_dataset_steps, pipeline,
    } = o;
    RunArgs {
        accept_recommendations, accept_reference_settings, bids_dir, output_dir, config, include, exclude,
        num_echoes, n_procs, source_dicom, dicom_outputs, dry, debug, mem_limit_gb, no_mem_limit, force,
        clean_intermediates, no_dataset_steps, pipeline: pipeline.into(),
    }
}

/// `qsmxt slurm`'s arguments for the TUI's SLURM run.
pub fn slurm_args(o: SlurmOptions) -> SlurmArgs {
    let SlurmOptions {
        accept_recommendations, accept_reference_settings, bids_dir, output_dir, account, partition, config,
        time, mem, cpus_per_task, submit, include, exclude, num_echoes, force, clean_intermediates, debug,
        source_dicom, dicom_outputs, pipeline,
    } = o;
    SlurmArgs {
        accept_recommendations, accept_reference_settings, bids_dir, output_dir, account, partition, config,
        time, mem, cpus_per_task, submit, include, exclude, num_echoes, force, clean_intermediates, debug,
        source_dicom, dicom_outputs, pipeline: pipeline.into(),
    }
}
