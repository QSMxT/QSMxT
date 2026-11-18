use crate::bids::discovery::{self, DiscoveryFilter};
use crate::cli::SlurmArgs;

pub fn execute(args: SlurmArgs) -> crate::Result<()> {
    // Build config the same way `qsmxt run` does: file -> CLI/TUI overrides -> every check that
    // can be made before a run starts. Without the overrides the generated jobs silently fall
    // back to the pipeline defaults; without the checks a bad setting is found once per job,
    // after submission.
    let mut config = super::run::build_config(args.config.as_deref(), &args.pipeline)?;

    let filter = DiscoveryFilter {
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        num_echoes: args.num_echoes,
    };
    let runs = discovery::discover_runs(&args.bids_dir, &filter)?;

    if runs.is_empty() {
        return Err(discovery::no_runs(&args.bids_dir, &filter));
    }

    // Applied here, so every job reads the same configuration file.
    let which = super::recommend::which(args.accept_recommendations, args.accept_reference_settings);
    if which.any() {
        super::recommend::accept(&mut config, &runs, which);
    }

    // Refuse before writing or submitting a job that would fail on its node.
    crate::pipeline::requirements::check_runs(&runs, &config)?;
    // A DICOM SEG is drawn on another exported series: refuse a selection that leaves it none.
    crate::dicom::export::check_outputs(args.dicom_outputs.as_deref(), &config).map_err(crate::error::QsmxtError::Config)?;
    super::run::orientation_groups(&runs, &config).map_err(crate::error::QsmxtError::Config)?;
    super::recommend::log_found(&config, &runs);

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
            debug: args.debug,
            source_dicom: args.source_dicom.as_deref(),
            dicom_outputs: args.dicom_outputs.as_deref(),
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::executor::slurm::{generate_all_slurm, SlurmOptions};
    use crate::pipeline::config::PipelineConfig;
    use crate::testutils::slurm::{dummy_run_with_key, opts};

    /// `--debug` reaches every job; the DICOM export settings reach the run jobs, where the
    /// export happens, and not the aggregation job, which has no such flags.
    #[test]
    fn debug_and_dicom_export_settings_reach_the_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None)];
        let mut config = PipelineConfig::default();
        config.pipeline.export_dicom = true;
        let outputs = vec!["Chimap".to_string(), "swi".to_string()];
        let o = SlurmOptions {
            debug: true,
            source_dicom: Some(Path::new("/dicom dir")),
            dicom_outputs: Some(&outputs),
            ..opts("acct", None, Some(8))
        };
        let jobs = generate_all_slurm(&runs, Path::new("/bids"), dir.path(), &config, &o).unwrap();
        let run = std::fs::read_to_string(&jobs.run_scripts[0]).unwrap();
        assert!(run.contains("--debug"), "{run}");
        assert!(run.contains("--source-dicom '/dicom dir'"), "{run}");
        assert!(run.contains("--dicom-outputs 'Chimap,swi'"), "{run}");
        let agg = std::fs::read_to_string(&jobs.aggregate_script).unwrap();
        assert!(agg.contains("--debug"), "{agg}");
        assert!(!agg.contains("--source-dicom") && !agg.contains("--dicom-outputs"), "{agg}");

        // The run job's command line is one `qsmxt run` invocation clap accepts.
        let cmd: String = run.lines().skip_while(|l| !l.starts_with("exec "))
            .map(|l| l.trim_end_matches('\\').trim()).collect::<Vec<_>>().join(" ");
        let argv = shell_words(&cmd["exec ".len()..]);
        let mut argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        argv[0] = "qsmxt";
        let parsed = <crate::cli::Cli as clap::Parser>::try_parse_from(&argv).unwrap_or_else(|e| panic!("{cmd}: {e}"));
        let crate::cli::Command::Run(r) = parsed.command else { panic!("not run: {cmd}") };
        assert!(r.debug);
        assert_eq!(r.source_dicom.as_deref(), Some(Path::new("/dicom dir")));
        assert_eq!(r.dicom_outputs, Some(outputs.clone()));
    }

    /// Split a command line the way the job's bash does, for the single-quoted words it uses.
    fn shell_words(s: &str) -> Vec<String> {
        let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
        for c in s.chars() {
            match c {
                '\'' => { quoted = !quoted; any = true; }
                c if c.is_whitespace() && !quoted => {
                    if any { out.push(std::mem::take(&mut cur)); any = false; }
                }
                c => { cur.push(c); any = true; }
            }
        }
        if any { out.push(cur); }
        out
    }
}
