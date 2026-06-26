use crate::bids::discovery::{self, DiscoveryFilter};
use crate::cli::SlurmArgs;
use crate::pipeline::config::PipelineConfig;

pub fn execute(args: SlurmArgs) -> crate::Result<()> {
    let config = if let Some(ref path) = args.config {
        crate::pipeline::config::load_config(path)?
    } else {
        PipelineConfig::default()
    };

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
