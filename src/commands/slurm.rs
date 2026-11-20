use crate::cli::{DicomExportArgs, ExecArgs, SlurmArgs};

pub fn execute(args: SlurmArgs) -> qsmxt_io::Result<()> {
    // Every option is taken apart here, so one that nothing reads is a warning (an unused
    // variable), which CI's clippy refuses.
    let SlurmArgs { dataset, account, partition, time, mem, cpus_per_task, submit, exec, dicom, accept, pipeline } = args;
    let ExecArgs { force, debug, clean_intermediates } = exec;
    let DicomExportArgs { source_dicom, dicom_outputs } = dicom;
    // As `qsmxt run` prepares a run: file -> CLI/TUI overrides -> recommendations asked for ->
    // every check that can be made before a run starts. Without the overrides the generated jobs
    // silently fall back to the pipeline defaults; without the checks a bad setting is found
    // once per job, after submission. Applied here, so every job reads the same configuration.
    let super::run::Prepared { config, runs, .. } =
        super::run::prepare(&dataset, &pipeline, &accept, dicom_outputs.as_deref(), true)?;
    super::recommend::log_found(&config, &runs);

    let jobs = qsmxt_io::executor::slurm::generate_all_slurm(
        &runs,
        &dataset.bids_dir,
        dataset.base_dir(),
        &config,
        &qsmxt_io::executor::slurm::SlurmOptions {
            account: &account,
            partition: partition.as_deref(),
            time: &time,
            mem_gb: mem,
            cpus: cpus_per_task,
            num_echoes: dataset.num_echoes,
            force,
            clean_intermediates,
            debug,
            source_dicom: source_dicom.as_deref(),
            dicom_outputs: dicom_outputs.as_deref(),
            command_line: std::env::args().collect::<Vec<_>>().join(" "),
        },
    )?;

    println!("Generated {} per-run SLURM script(s):", jobs.run_scripts.len());
    for s in &jobs.run_scripts {
        println!("  {}", s.display());
    }
    println!("and the aggregation job (dataset-level steps, after every run job succeeds):");
    println!("  {}", jobs.aggregate_script.display());

    if submit {
        let (ids, agg) = qsmxt_io::executor::slurm::submit_jobs(&jobs)?;
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

    use qsmxt_io::executor::slurm::{generate_all_slurm, SlurmOptions};
    use qsmxt_engine::PipelineConfig;
    use qsmxt_io::testutils::slurm::{dummy_run_with_key, opts};

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
        let run = fs_err::read_to_string(&jobs.run_scripts[0]).unwrap();
        assert!(run.contains("--debug"), "{run}");
        assert!(run.contains("--source-dicom '/dicom dir'"), "{run}");
        assert!(run.contains("--dicom-outputs 'Chimap,swi'"), "{run}");
        let agg = fs_err::read_to_string(&jobs.aggregate_script).unwrap();
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
        assert!(r.exec.debug);
        assert_eq!(r.dicom.source_dicom.as_deref(), Some(Path::new("/dicom dir")));
        assert_eq!(r.dicom.dicom_outputs, Some(outputs.clone()));
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
