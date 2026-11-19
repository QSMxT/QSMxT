mod cli;
mod cli_settings;
#[cfg(test)]
mod cli_reference;
#[cfg(test)]
mod cli_snapshots;
#[cfg(test)]
#[path = "../tests/support/snapshot.rs"]
mod snapshot;
mod commands;
mod pipeline;
#[cfg(test)]
mod tui_cli_tests;

// The file side lives in qsmxt-io; under the same paths here.
use qsmxt_io::{bids, dicom, error, example, executor, multiorient, nifti};
#[cfg(test)]
use qsmxt_io::testutils;

pub use error::Result;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();

    // Run, aggregate and TUI init their own loggers (to write to log files).
    // All other commands use a simple stderr logger.
    if !matches!(cli.command, Command::Run(_) | Command::Aggregate(_) | Command::Tui) {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .format_timestamp(None)
            .init();
    }

    let result = match cli.command {
        Command::Run(args) => commands::run::execute(args),
        Command::Init(args) => commands::init::execute(args),
        Command::Validate(args) => commands::validate::execute(args),
        Command::DicomConvert(args) => commands::dicom::execute(args),
        Command::Slurm(args) => commands::slurm::execute(args),
        Command::Aggregate(args) => commands::aggregate::execute(args),
        Command::Mask { command } => commands::mask::execute(command),
        Command::Combine { command } => commands::combine::execute(command),
        Command::Unwrap { command } => commands::unwrap::execute(command),
        Command::Fieldmap { command } => commands::fieldmap::execute(command),
        Command::Bgremove { command } => commands::bgremove::execute(command),
        Command::Invert { command } => commands::invert::execute(command),
        Command::Separate { command } => commands::separate::execute(command),
        Command::Qsmart(args) => commands::qsmart::execute(args),
        Command::Swi(args) => commands::swi::execute(args),
        Command::Segment(args) => commands::segment::execute(args),
        Command::R2star(args) => commands::r2star::execute(args),
        Command::T2star(args) => commands::t2star::execute(args),
        Command::R2(args) => commands::r2::execute(args),
        Command::R2prime(args) => commands::r2prime::execute(args),
        Command::Homogeneity(args) => commands::homogeneity::execute(args),
        Command::Resample(args) => commands::resample::execute(args),
        Command::QualityMap(args) => commands::quality_map::execute(args),
        Command::Example(args) => commands::example::execute(args),
        Command::Tui => commands::tui::execute(),
        Command::Update(args) => commands::update::execute(args),
        Command::News => commands::news::execute(),
    };

    if let Err(e) = result {
        log::error!("{}", e);
        std::process::exit(1);
    }
}
