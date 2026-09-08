use crate::bids::discovery::{self, DiscoveryFilter};
use crate::cli::SlurmArgs;
use crate::pipeline::config::PipelineConfig;

pub fn execute(args: SlurmArgs) -> crate::Result<()> {
    // Build config the same way `qsmxt run` does: file -> CLI/TUI overrides.
    // Without the overrides the generated jobs silently fall back to the
    // pipeline defaults instead of the requested settings.
    let mut config = if let Some(ref path) = args.config {
        crate::pipeline::config::load_config(path)?
    } else {
        PipelineConfig::default()
    };

    crate::pipeline::config::apply_run_overrides(&mut config, &args.pipeline);

    let filter = DiscoveryFilter {
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        num_echoes: args.num_echoes,
    };
    let runs = discovery::discover_runs(&args.bids_dir, &filter)?;

    if runs.is_empty() {
        println!("No QSM-compatible runs found");
        return Ok(());
    }

    let base_dir = args.output_dir.as_deref().unwrap_or(&args.bids_dir);

    let scripts = crate::executor::slurm::generate_all_slurm(
        &runs,
        &args.bids_dir,
        base_dir,
        &config,
        &args.account,
        args.partition.as_deref(),
        &args.time,
        args.mem,
        args.cpus_per_task,
    )?;

    println!("Generated {} SLURM scripts:", scripts.len());
    for s in &scripts {
        println!("  {}", s.display());
    }

    if args.submit {
        crate::executor::slurm::submit_scripts(&scripts)?;
    }

    Ok(())
}
