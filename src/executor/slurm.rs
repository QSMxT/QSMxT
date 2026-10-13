use std::path::{Path, PathBuf};
use std::process::Command;

use log::info;

use crate::bids::discovery::QsmRun;
use crate::pipeline::config::PipelineConfig;

/// Shell-quote a string for safe interpolation into bash scripts.
/// Uses single quotes and escapes any embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Scheduler settings shared by every generated job.
pub struct SlurmOptions<'a> {
    pub account: &'a str,
    pub partition: Option<&'a str>,
    pub time: &'a str,
    /// Memory per job in GB; `None` sizes each job from the memory estimate.
    pub mem_gb: Option<usize>,
    pub cpus: usize,
    /// Discovery options the jobs must repeat to see the same runs.
    pub num_echoes: Option<usize>,
    pub force: bool,
    pub clean_intermediates: bool,
    /// The `qsmxt slurm …` invocation, recorded as the provenance command.
    pub command_line: String,
}

/// What [`generate_all_slurm`] wrote.
#[derive(Debug)]
pub struct SlurmJobs {
    /// One per run: `qsmxt run --no-dataset-steps --include <run>`.
    pub run_scripts: Vec<PathBuf>,
    /// The final job: `qsmxt aggregate` over every run, once all of them have succeeded.
    pub aggregate_script: PathBuf,
    /// Submits the run jobs, then the aggregation job with `--dependency=afterok:<their ids>`.
    pub submit_script: PathBuf,
}

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Memory to request for a job whose pipeline peaks at about `estimate_bytes`: the estimate
/// plus a quarter and a GB for the process itself, in whole GB, at least 2.
fn mem_gb_for(estimate_bytes: usize) -> usize {
    (((estimate_bytes as f64 * 1.25) / GIB).ceil() as usize + 1).max(2)
}

fn run_estimate(run: &QsmRun, config: &PipelineConfig) -> usize {
    let (nx, ny, nz) = run.dims;
    crate::pipeline::memory::estimate_peak_memory_bytes(
        nx, ny, nz, run.echoes.len(), run.coils.as_ref().map_or(1, |c| c.len()), run.has_magnitude, config,
    )
}

/// The aggregation job's estimate: a multi-orientation group holds every member's field and
/// mask at once, so its largest group's members together; otherwise one run's.
fn aggregate_estimate(runs: &[QsmRun], config: &PipelineConfig) -> usize {
    let largest_run = runs.iter().map(|r| run_estimate(r, config)).max().unwrap_or(0);
    let groups = if config.multi_orientation.enabled() {
        crate::bids::orientation::parse_pattern(&config.multi_orientation.group_by).ok().flatten()
            .map(|p| crate::bids::orientation::group_runs(runs, &p)).unwrap_or_default()
    } else {
        Vec::new()
    };
    groups.iter()
        .map(|g| g.members.iter().map(|&i| run_estimate(&runs[i], config)).sum::<usize>())
        .max().unwrap_or(0).max(largest_run)
}

fn sbatch_header(job_name: &str, slurm_dir: &Path, opts: &SlurmOptions, mem_gb: usize) -> String {
    let mut h = format!("#!/bin/bash\n#SBATCH --job-name={job_name}\n#SBATCH --account={}\n", opts.account);
    if let Some(p) = opts.partition {
        h.push_str(&format!("#SBATCH --partition={p}\n"));
    }
    let log = |ext: &str| shell_quote(&slurm_dir.join(format!("{job_name}_%j.{ext}")).display().to_string());
    h.push_str(&format!(
        "#SBATCH --time={}\n#SBATCH --mem={mem_gb}G\n#SBATCH --cpus-per-task={}\n\
         #SBATCH --output={}\n#SBATCH --error={}\n\n",
        opts.time, opts.cpus, log("out"), log("err"),
    ));
    h
}

/// Generate SLURM job scripts: one per run, running only that run's pipeline, and a final
/// aggregation job that runs the dataset-level steps (multi-orientation reconstruction, summary
/// tables, provenance, …) once every run job has succeeded — together the same work as one local
/// `qsmxt run`. Also writes `submit.sh`, which submits them with that dependency.
pub fn generate_all_slurm(
    runs: &[QsmRun],
    bids_dir: &Path,
    output_dir: &Path,
    config: &PipelineConfig,
    opts: &SlurmOptions,
) -> crate::Result<SlurmJobs> {
    let derivatives_dir = output_dir.join("derivatives").join("qsmxt");
    let slurm_dir = derivatives_dir.join("slurm");
    std::fs::create_dir_all(&slurm_dir)?;

    // The jobs all run against this file, so they cannot disagree about the configuration.
    let config_path = derivatives_dir.join("pipeline_config.toml");
    std::fs::write(&config_path, config.to_toml().unwrap_or_default())?;

    let qsmxt_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("qsmxt"));
    let q = |p: &Path| shell_quote(&p.display().to_string());
    let common = format!("{} {} --config {}", q(bids_dir), q(output_dir), q(&config_path));
    let mut shared_flags = String::new();
    if let Some(n) = opts.num_echoes {
        shared_flags.push_str(&format!(" \\\n  --num-echoes {n}"));
    }
    if opts.force {
        shared_flags.push_str(" \\\n  --force");
    }

    let mut run_scripts = Vec::new();
    for run in runs {
        let job_name = format!("qsmxt_{}", run.key);
        let estimate = run_estimate(run, config);
        let mem_gb = match opts.mem_gb {
            Some(gb) => {
                if (gb as f64) * GIB < estimate as f64 {
                    log::warn!(
                        "{}: estimated to peak at {}, more than --mem {gb}G; the job may be killed \
                         for exceeding its memory",
                        run.key, crate::pipeline::memory::format_bytes(estimate)
                    );
                }
                gb
            }
            None => mem_gb_for(estimate),
        };
        let script = format!(
            "{header}exec {bin} run {common} \\\n  --include {include} \\\n  --no-dataset-steps \\\n  --n-procs {cpus}{shared_flags}\n",
            header = sbatch_header(&job_name, &slurm_dir, opts, mem_gb),
            bin = q(&qsmxt_bin),
            include = shell_quote(&run.key.to_string()),
            cpus = opts.cpus,
        );
        let script_path = slurm_dir.join(format!("{job_name}.sh"));
        std::fs::write(&script_path, script)?;
        run_scripts.push(script_path);
    }

    let agg_mem = opts.mem_gb.unwrap_or_else(|| mem_gb_for(aggregate_estimate(runs, config)));
    let includes: Vec<String> = runs.iter().map(|r| shell_quote(&r.key.to_string())).collect();
    let mut agg_flags = shared_flags.clone();
    if opts.clean_intermediates {
        agg_flags.push_str(" \\\n  --clean-intermediates");
    }
    let aggregate = format!(
        "{header}exec {bin} aggregate {common} \\\n  --include {includes}{agg_flags} \\\n  --command-line {cmd}\n",
        header = sbatch_header("qsmxt_aggregate", &slurm_dir, opts, agg_mem),
        bin = q(&qsmxt_bin),
        includes = includes.join(" "),
        cmd = shell_quote(&opts.command_line),
    );
    let aggregate_script = slurm_dir.join("qsmxt_aggregate.sh");
    std::fs::write(&aggregate_script, aggregate)?;

    let mut submit = String::from(
        "#!/bin/bash\n\
         # Submit one job per run, then the aggregation job (the dataset-level steps), which\n\
         # starts only once every run job has succeeded (--dependency=afterok).\n\
         set -euo pipefail\n\
         ids=()\n",
    );
    for s in &run_scripts {
        submit.push_str(&format!("ids+=(\"$(sbatch --parsable {} | cut -d';' -f1)\")\n", q(s)));
    }
    submit.push_str(&format!(
        "deps=$(IFS=:; echo \"${{ids[*]}}\")\n\
         agg=$(sbatch --parsable --dependency=afterok:\"$deps\" {} | cut -d';' -f1)\n\
         echo \"Submitted ${{#ids[@]}} run job(s): ${{ids[*]}}\"\n\
         echo \"Submitted aggregation job $agg (afterok:$deps)\"\n",
        q(&aggregate_script),
    ));
    let submit_script = slurm_dir.join("submit.sh");
    std::fs::write(&submit_script, submit)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in run_scripts.iter().chain([&aggregate_script, &submit_script]) {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
        }
    }

    Ok(SlurmJobs { run_scripts, aggregate_script, submit_script })
}

/// `sbatch --parsable [extra] script`, returning the job id.
fn sbatch(sbatch_bin: &std::ffi::OsStr, extra: &[String], script: &Path) -> crate::Result<String> {
    info!("Submitting {}", script.display());
    let output = Command::new(sbatch_bin)
        .arg("--parsable")
        .args(extra)
        .arg(script)
        .output()
        .map_err(|e| crate::error::QsmxtError::Slurm(format!("sbatch failed: {}", e)))?;
    if !output.status.success() {
        return Err(crate::error::QsmxtError::Slurm(format!(
            "sbatch failed for {}: {}",
            script.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    // `--parsable` prints `<id>` or `<id>;<cluster>`.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let id = stdout.trim().split(';').next().unwrap_or("").to_string();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return Err(crate::error::QsmxtError::Slurm(format!(
            "sbatch printed no job id for {}: {:?}", script.display(), stdout.trim()
        )));
    }
    info!("  job {id}");
    Ok(id)
}

/// Submit the run jobs, then the aggregation job depending on all of them. Returns the run job
/// ids and the aggregation job id.
pub fn submit_jobs(jobs: &SlurmJobs) -> crate::Result<(Vec<String>, String)> {
    submit_jobs_with(std::ffi::OsStr::new("sbatch"), jobs)
}

fn submit_jobs_with(sbatch_bin: &std::ffi::OsStr, jobs: &SlurmJobs) -> crate::Result<(Vec<String>, String)> {
    let mut ids = Vec::new();
    for script in &jobs.run_scripts {
        match sbatch(sbatch_bin, &[], script) {
            Ok(id) => ids.push(id),
            Err(e) if !ids.is_empty() => {
                return Err(crate::error::QsmxtError::Slurm(format!(
                    "{e}. Already submitted: {} (cancel with `scancel {}` before resubmitting)",
                    ids.join(", "), ids.join(" ")
                )));
            }
            Err(e) => return Err(e),
        }
    }
    let dependency = format!("--dependency=afterok:{}", ids.join(":"));
    let agg = sbatch(sbatch_bin, &[dependency], &jobs.aggregate_script).map_err(|e| {
        crate::error::QsmxtError::Slurm(format!(
            "{e}. The run jobs ({}) are submitted; submit the aggregation once they finish with \
             `sbatch {}`", ids.join(", "), jobs.aggregate_script.display()
        ))
    })?;
    Ok((ids, agg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::discovery::{EchoFiles, QsmRun};
    use crate::bids::entities::AcquisitionKey;

    fn dummy_run_with_key(subject: &str, session: Option<&str>) -> QsmRun {
        QsmRun {
            key: AcquisitionKey {
                subject: subject.to_string(),
                session: session.map(|s| s.to_string()),
                acquisition: None,
                reconstruction: None,
                inversion: None,
                run: None,
                suffix: "MEGRE".to_string(),
            },
            echoes: vec![EchoFiles {
                echo_number: 1,
                phase_nifti: PathBuf::from("fake.nii"),
                phase_json: PathBuf::from("fake.json"),
                magnitude_nifti: None,
                magnitude_json: None,
                imaginary_nifti: None,
            }],
            coils: None,
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02],
            b0_dir: None,
            dims: (64, 64, 64),
            has_magnitude: false,
            mese: None,
        }
    }

    fn opts(account: &str, partition: Option<&'static str>, mem_gb: Option<usize>) -> SlurmOptions<'static> {
        SlurmOptions {
            account: Box::leak(account.to_string().into_boxed_str()),
            partition,
            time: "02:00:00",
            mem_gb,
            cpus: 4,
            num_echoes: None,
            force: false,
            clean_intermediates: false,
            command_line: "qsmxt slurm /bids --account acct".to_string(),
        }
    }

    #[test]
    fn test_slurm_script_contains_sbatch_directives() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None)];
        let config = PipelineConfig::default();
        let jobs = generate_all_slurm(
            &runs, Path::new("/bids"), dir.path(), &config, &opts("myaccount", Some("gpu"), Some(32)),
        )
        .unwrap();

        for script in [&jobs.run_scripts[0], &jobs.aggregate_script] {
            let content = std::fs::read_to_string(script).unwrap();
            assert!(content.contains("#SBATCH --job-name="));
            assert!(content.contains("#SBATCH --account=myaccount"));
            assert!(content.contains("#SBATCH --partition=gpu"));
            assert!(content.contains("#SBATCH --time=02:00:00"));
            assert!(content.contains("#SBATCH --mem=32G"));
            assert!(content.contains("#SBATCH --cpus-per-task=4"));
            // Job logs land beside the scripts, not wherever sbatch happened to be run from.
            assert!(content.contains(&format!("#SBATCH --output='{}", dir.path().join("derivatives/qsmxt/slurm").display())));
        }
    }

    #[test]
    fn test_slurm_script_partition_omitted_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None)];
        let config = PipelineConfig::default();
        let jobs = generate_all_slurm(&runs, Path::new("/bids"), dir.path(), &config, &opts("acct", None, Some(16))).unwrap();

        let content = std::fs::read_to_string(&jobs.run_scripts[0]).unwrap();
        assert!(!content.contains("--partition"), "Partition should be omitted");
    }

    #[test]
    fn test_slurm_script_include_flag() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", Some("pre"))];
        let config = PipelineConfig::default();
        let jobs = generate_all_slurm(&runs, Path::new("/bids"), dir.path(), &config, &opts("acct", None, Some(16))).unwrap();

        let content = std::fs::read_to_string(&jobs.run_scripts[0]).unwrap();
        assert!(content.contains("--include 'sub-01_ses-pre_MEGRE'"), "Should contain --include with run key");
    }

    #[test]
    fn test_shell_quote_simple() {
        assert_eq!(shell_quote("hello"), "'hello'");
    }

    #[test]
    fn test_shell_quote_with_spaces() {
        assert_eq!(shell_quote("/path/with spaces/dir"), "'/path/with spaces/dir'");
    }

    #[test]
    fn test_shell_quote_with_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_slurm_script_paths_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None)];
        let config = PipelineConfig::default();
        let jobs = generate_all_slurm(&runs, Path::new("/bids/my data"), dir.path(), &config, &opts("acct", None, Some(16))).unwrap();

        let content = std::fs::read_to_string(&jobs.run_scripts[0]).unwrap();
        assert!(content.contains("'/bids/my data'"), "Paths with spaces should be quoted");
    }

    /// Without `--mem`, each job asks for what its run is estimated to need: a bigger run gets
    /// more, and the request covers the estimate.
    #[test]
    fn memory_is_sized_per_run_from_the_estimate() {
        let dir = tempfile::tempdir().unwrap();
        let small = dummy_run_with_key("01", None);
        let mut big = dummy_run_with_key("02", None);
        big.dims = (384, 384, 256);
        big.echoes = (0..8).map(|_| small.echoes[0].clone()).collect();
        let config = PipelineConfig::default();
        let jobs = generate_all_slurm(&[small.clone(), big.clone()], Path::new("/bids"), dir.path(), &config, &opts("acct", None, None)).unwrap();

        let mem_of = |p: &Path| -> usize {
            let text = std::fs::read_to_string(p).unwrap();
            let line = text.lines().find(|l| l.starts_with("#SBATCH --mem=")).unwrap();
            line.trim_start_matches("#SBATCH --mem=").trim_end_matches('G').parse().unwrap()
        };
        let (m_small, m_big) = (mem_of(&jobs.run_scripts[0]), mem_of(&jobs.run_scripts[1]));
        assert!(m_big > m_small, "a bigger run gets more memory ({m_big}G vs {m_small}G)");
        assert!(m_small >= 2);
        let est_big = run_estimate(&big, &config) as f64 / GIB;
        assert!(m_big as f64 >= est_big, "{m_big}G must cover the {est_big:.1}G estimate");
        // The aggregation job is sized for at least the largest run.
        assert!(mem_of(&jobs.aggregate_script) >= m_big);
    }

    /// The acceptance test for #269's scripts: a two-orientation dataset gives one job per run,
    /// each running only its run's pipeline, and an aggregation job over all of them that is
    /// submitted with `afterok` on every run job; `--num-echoes` reaches every job.
    #[test]
    fn a_two_orientation_dataset_gets_run_jobs_and_a_dependent_aggregation_job() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        crate::testutils::create_multi_orientation_bids(&bids);
        let filter = crate::bids::discovery::DiscoveryFilter {
            include: Some(vec!["*acq-dir1*".into(), "*acq-dir2*".into()]),
            num_echoes: Some(1),
            ..Default::default()
        };
        let runs = crate::bids::discovery::discover_runs(&bids, &filter).unwrap();
        assert_eq!(runs.len(), 2);
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        let mut o = opts("acct", None, None);
        o.num_echoes = Some(1);
        o.clean_intermediates = true;
        let out = dir.path().join("out");
        let jobs = generate_all_slurm(&runs, &bids, &out, &config, &o).unwrap();

        assert_eq!(jobs.run_scripts.len(), 2, "one job per run");
        for (run, script) in runs.iter().zip(&jobs.run_scripts) {
            let text = std::fs::read_to_string(script).unwrap();
            assert!(text.contains(" run "), "{text}");
            assert!(text.contains(&format!("--include '{}'", run.key)), "{text}");
            assert!(text.contains("--no-dataset-steps"), "a run job must not do dataset-level steps:\n{text}");
            assert!(text.contains("--num-echoes 1"), "--num-echoes must reach the run job:\n{text}");
            assert!(!text.contains("--clean-intermediates"), "cleaning waits for the aggregation:\n{text}");
        }

        let agg = std::fs::read_to_string(&jobs.aggregate_script).unwrap();
        assert!(agg.contains(" aggregate "), "{agg}");
        let includes = format!("--include '{}' '{}'", runs[0].key, runs[1].key);
        assert!(agg.contains(&includes), "the aggregation covers every run:\n{agg}");
        assert!(agg.contains("--num-echoes 1"), "{agg}");
        assert!(agg.contains("--clean-intermediates"), "{agg}");
        assert!(agg.contains(&format!("--config '{}'", out.join("derivatives/qsmxt/pipeline_config.toml").display())));
        assert!(agg.contains("--command-line 'qsmxt slurm /bids --account acct'"), "{agg}");

        let submit = std::fs::read_to_string(&jobs.submit_script).unwrap();
        for s in &jobs.run_scripts {
            assert!(submit.contains(&format!("sbatch --parsable '{}'", s.display())), "{submit}");
        }
        assert!(submit.contains(&format!(
            "sbatch --parsable --dependency=afterok:\"$deps\" '{}'", jobs.aggregate_script.display()
        )), "{submit}");

        // And the submission itself, through a stand-in sbatch that hands out ids 101, 102, …
        // and records each call.
        let calls = dir.path().join("calls.txt");
        let fake = dir.path().join("sbatch");
        std::fs::write(&fake, format!(
            "#!/bin/sh\necho \"$@\" >> '{c}'\nn=$(wc -l < '{c}')\necho $((100 + n))\n",
            c = calls.display()
        )).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            let (ids, agg_id) = submit_jobs_with(fake.as_os_str(), &jobs).unwrap();
            assert_eq!(ids, ["101", "102"]);
            assert_eq!(agg_id, "103");
            let log = std::fs::read_to_string(&calls).unwrap();
            let lines: Vec<&str> = log.lines().collect();
            assert_eq!(lines.len(), 3);
            assert_eq!(lines[2], format!("--parsable --dependency=afterok:101:102 {}", jobs.aggregate_script.display()));

            // The generated submit.sh does the same.
            std::fs::remove_file(&calls).unwrap();
            let status = Command::new("bash").arg(&jobs.submit_script)
                .env("PATH", format!("{}:{}", dir.path().display(), std::env::var("PATH").unwrap_or_default()))
                .output().unwrap();
            assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
            let log = std::fs::read_to_string(&calls).unwrap();
            assert_eq!(log.lines().last().unwrap(), format!("--parsable --dependency=afterok:101:102 {}", jobs.aggregate_script.display()));
        }
    }

    /// A failure part-way through submission names the jobs already queued, so they can be
    /// cancelled rather than left to run with no aggregation behind them.
    #[cfg(unix)]
    #[test]
    fn a_failed_submission_names_the_jobs_already_queued() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None), dummy_run_with_key("02", None)];
        let jobs = generate_all_slurm(&runs, Path::new("/bids"), dir.path(), &PipelineConfig::default(), &opts("acct", None, Some(4))).unwrap();
        let fake = dir.path().join("sbatch");
        let marker = dir.path().join("once");
        std::fs::write(&fake, format!(
            "#!/bin/sh\nif [ -e '{m}' ]; then echo 'quota exceeded' >&2; exit 1; fi\ntouch '{m}'\necho 7\n",
            m = marker.display()
        )).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let err = submit_jobs_with(fake.as_os_str(), &jobs).unwrap_err().to_string();
        assert!(err.contains("quota exceeded") && err.contains("scancel 7"), "{err}");
    }
}
