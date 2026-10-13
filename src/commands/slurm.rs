use crate::bids::discovery::{self, DiscoveryFilter};
use crate::cli::SlurmArgs;

pub fn execute(args: SlurmArgs) -> crate::Result<()> {
    // Build config the same way `qsmxt run` does: file -> CLI/TUI overrides -> every check that
    // can be made before a run starts. Without the overrides the generated jobs silently fall
    // back to the pipeline defaults; without the checks a bad setting is found once per job,
    // after submission.
    let config = super::run::build_config(args.config.as_deref(), &args.pipeline)?;

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

    // Refuse before writing or submitting a job that would fail on its node.
    crate::pipeline::requirements::check_runs(&runs, &config)?;

    let base_dir = args.output_dir.as_deref().unwrap_or(&args.bids_dir);

    let jobs = crate::executor::slurm::generate_all_slurm(
        &runs,
        &args.bids_dir,
        base_dir,
        &config,
        &crate::executor::slurm::SlurmOptions {
            account: &args.account,
            partition: args.partition.as_deref(),
            time: &args.time,
            mem_gb: args.mem,
            cpus: args.cpus_per_task,
            num_echoes: args.num_echoes,
            force: args.force,
            clean_intermediates: args.clean_intermediates,
            command_line: std::env::args().collect::<Vec<_>>().join(" "),
        },
    )?;

    println!("Generated {} per-run SLURM script(s):", jobs.run_scripts.len());
    for s in &jobs.run_scripts {
        println!("  {}", s.display());
    }
    println!("and the aggregation job (dataset-level steps, after every run job succeeds):");
    println!("  {}", jobs.aggregate_script.display());

    if args.submit {
        let (ids, agg) = crate::executor::slurm::submit_jobs(&jobs)?;
        println!("Submitted {} run job(s): {}", ids.len(), ids.join(" "));
        println!("Submitted aggregation job {agg} (afterok:{})", ids.join(":"));
    } else {
        println!("Submit them all with:\n  bash {}", jobs.submit_script.display());
    }

    Ok(())
}
