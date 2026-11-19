use super::*;

// --- Slurm command ---

#[test]
fn test_slurm_command() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_single_echo_bids(&bids);

    super::slurm::execute(SlurmArgs {
        accept_recommendations: false,
        accept_reference_settings: false,
        bids_dir: bids,
        output_dir: Some(out.clone()),
        account: "testacct".to_string(),
        partition: Some("gpu".to_string()),
        config: None,
        time: "01:00:00".to_string(),
        mem: Some(16), cpus_per_task: 2, submit: false,
        include: None, exclude: None, num_echoes: None, force: false, clean_intermediates: false,
        debug: false, source_dicom: None, dicom_outputs: None,
        pipeline: Default::default(),
    }).unwrap();

    assert!(out.join("derivatives/qsmxt/slurm").exists());
}

/// The jobs run `qsmxt run --config <pipeline_config.toml>`, so the written
/// config must carry the requested pipeline — not the defaults.
#[test]
fn test_slurm_writes_requested_pipeline_config() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_single_echo_bids(&bids);

    super::slurm::execute(SlurmArgs {
        accept_recommendations: false,
        accept_reference_settings: false,
        bids_dir: bids,
        output_dir: Some(out.clone()),
        account: "testacct".to_string(),
        partition: None,
        config: None,
        time: "01:00:00".to_string(),
        mem: Some(16), cpus_per_task: 2, submit: false,
        include: None, exclude: None, num_echoes: None, force: false, clean_intermediates: false,
        debug: false, source_dicom: None, dicom_outputs: None,
        pipeline: crate::cli_settings::SettingsArgs::from_flags(&[
            "--qsm-algorithm", "tkd", "--unwrapping-algorithm", "laplacian", "--bf-algorithm", "pdf",
        ]),
    }).unwrap();

    let written = std::fs::read_to_string(out.join("derivatives/qsmxt/pipeline_config.toml")).unwrap();
    let config: crate::pipeline::config::PipelineConfig = toml::from_str(&written).unwrap();
    assert_eq!(config.inversion.algorithm, crate::pipeline::config::QsmAlgorithm::Tkd);
    assert_eq!(
        config.field_mapping.unwrapping_algorithm,
        crate::pipeline::config::UnwrappingAlgorithm::Laplacian
    );
    assert_eq!(config.bg_removal.algorithm, crate::pipeline::config::BfAlgorithm::Pdf);
}

/// The words of a generated job's command, as bash would split them: single-quoted strings
/// (with `'\''` escapes) and backslash-newline continuations; the `exec <binary>` dropped.
fn job_command(script: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(script).unwrap();
    let body: String = text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .collect::<Vec<_>>().join("\n").replace("\\\n", " ");
    let (mut words, mut cur, mut in_word, mut quoted) = (Vec::new(), String::new(), false, false);
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if quoted {
            if c == '\'' { quoted = false; } else { cur.push(c); }
        } else if c == '\'' {
            quoted = true;
            in_word = true;
        } else if c == '\\' && i + 1 < chars.len() {
            cur.push(chars[i + 1]);
            in_word = true;
            i += 1;
        } else if c.is_whitespace() {
            if in_word { words.push(std::mem::take(&mut cur)); in_word = false; }
        } else {
            cur.push(c);
            in_word = true;
        }
        i += 1;
    }
    if in_word { words.push(cur); }
    assert_eq!(words[0], "exec", "{text}");
    let mut argv = vec!["qsmxt".to_string()];
    argv.extend(words.into_iter().skip(2));
    argv
}

/// Run a generated job's command in-process, through the CLI parser.
fn run_job(script: &Path) {
    use clap::Parser;
    let argv = job_command(script);
    let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
    match cli.command {
        Command::Run(mut args) => {
            args.no_mem_limit = true;
            super::run::execute(args).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        }
        Command::Aggregate(args) => super::aggregate::execute(args).unwrap_or_else(|e| panic!("{argv:?}: {e}")),
        other => panic!("unexpected job command {other:?}"),
    }
}

/// Every file under `root`, relative to it, outside the directories that are not the
/// dataset's results.
fn derivative_files(root: &Path) -> std::collections::BTreeSet<PathBuf> {
    fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeSet<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_path_buf();
            if ["workflow", "logs", "slurm", "qsmxt.log"].iter().any(|x| rel == Path::new(x)) {
                continue;
            }
            if p.is_dir() { walk(root, &p, out) } else { out.insert(rel); }
        }
    }
    let mut out = Default::default();
    walk(root, root, &mut out);
    out
}

/// JSON with what legitimately differs between two executions masked: wall-clock times and
/// durations, and the command line (a test binary's argv here, `qsmxt slurm …` there).
fn normalised_json(text: &str) -> serde_json::Value {
    fn walk(key: &str, v: &mut serde_json::Value) {
        let volatile = key.contains("Time") || key.contains("time") || key == "Command"
            || key.contains("elapsed") || key.contains("duration") || key.contains("Duration");
        match v {
            serde_json::Value::Object(m) => for (k, x) in m.iter_mut() { walk(k, x) },
            serde_json::Value::Array(a) => for x in a { walk(key, x) },
            _ if volatile => *v = serde_json::Value::String("<volatile>".into()),
            serde_json::Value::String(s) if chrono::DateTime::parse_from_rfc3339(s).is_ok() => {
                *v = serde_json::Value::String("<timestamp>".into())
            }
            _ => {}
        }
    }
    let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
    walk("", &mut v);
    v
}

/// The acceptance test for #269: `qsmxt slurm`'s jobs — each run's `qsmxt run
/// --no-dataset-steps`, then `qsmxt aggregate` — leave the same dataset as one local `qsmxt
/// run`: the same files, the same dataset-level tables, provenance and metadata, and the same
/// two-orientation COSMOS map.
#[test]
fn slurm_jobs_produce_what_a_local_run_does() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    // Large enough that χ is defined inside the structures after background removal's erosion.
    testutils::create_multi_orientation_bids_on(&bids, testutils::STRUCTURES_N);
    testutils::write_dseg_on(&bids.join("derivatives/mydseg/sub-1/ses-1/anat/sub-1_ses-1_dseg.nii"), testutils::STRUCTURES_N);
    let pipeline = || crate::cli_settings::SettingsArgs::from_flags(&[
        // What is under test is SLURM/local parity, not the algorithms: the cheapest ones.
        "--qsm-algorithm", "tkd", "--unwrapping-algorithm", "laplacian", "--orientation-group", "acq",
        "--masking-input", "magnitude", "--do-analysis", "--use-custom-dseg", "mydseg",
    ]);
    let include = Some(vec!["*acq-dir1*".to_string(), "*acq-dir2*".to_string()]);

    let local = dir.path().join("local");
    let mut args = default_run_args(bids.clone(), local.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.include = include.clone();
    args.num_echoes = Some(1);
    args.pipeline = pipeline();
    super::run::execute(args).unwrap();

    let dist = dir.path().join("dist");
    super::slurm::execute(SlurmArgs {
        accept_recommendations: false,
        accept_reference_settings: false,
        bids_dir: bids.clone(), output_dir: Some(dist.clone()),
        account: "acct".into(), partition: None, config: None, time: "01:00:00".into(),
        mem: None, cpus_per_task: 1, submit: false,
        include, exclude: None, num_echoes: Some(1), force: false, clean_intermediates: false,
        debug: false, source_dicom: None, dicom_outputs: None,
        pipeline: pipeline(),
    }).unwrap();
    let slurm_dir = dist.join("derivatives/qsmxt/slurm");
    let mut run_scripts: Vec<PathBuf> = std::fs::read_dir(&slurm_dir).unwrap().flatten().map(|e| e.path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("qsmxt_sub-")).collect();
    run_scripts.sort();
    assert_eq!(run_scripts.len(), 2);

    for s in &run_scripts {
        run_job(s);
    }
    // The workers wrote nothing at dataset level: each left only its own log.
    let deriv = dist.join("derivatives/qsmxt");
    for f in ["qsmxt.log", "desc-reference_summary.tsv", "desc-segmentation_stats.tsv",
              "dataset_description.json", "methods.md", "prov"] {
        assert!(!deriv.join(f).exists(), "a per-run job wrote {f}");
    }
    assert!(!deriv.join("sub-1/ses-1/anat/sub-1_ses-1_desc-cosmos_Chimap.nii").exists());
    let logs: Vec<String> = std::fs::read_dir(deriv.join("logs")).unwrap().flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(logs.len(), 2, "{logs:?}");
    assert!(logs.contains(&"sub-1_ses-1_acq-dir1_MEGRE.log".to_string()), "{logs:?}");

    run_job(&slurm_dir.join("qsmxt_aggregate.sh"));

    let local_deriv = local.join("derivatives/qsmxt");
    let (want, got) = (derivative_files(&local_deriv), derivative_files(&deriv));
    assert_eq!(want, got, "the two runs produced different files");
    for must in ["desc-reference_summary.tsv", "desc-segmentation_stats.tsv", "dataset_description.json",
                 "pipeline_config.toml", "methods.md", ".bidsignore", "README",
                 "sub-1/ses-1/anat/sub-1_ses-1_desc-cosmos_Chimap.nii"] {
        assert!(want.contains(Path::new(must)), "{must} missing from the local run: {want:?}");
    }
    assert!(want.iter().any(|p| p.starts_with("prov")), "no provenance records");

    let mut differ = Vec::new();
    for rel in &want {
        let (a, b) = (std::fs::read(local_deriv.join(rel)).unwrap(), std::fs::read(deriv.join(rel)).unwrap());
        if a == b { continue; }
        let name = rel.to_string_lossy();
        // The two outputs live in different directories, which provenance names.
        let a = match std::str::from_utf8(&a) {
            Ok(t) => t.replace(&local.display().to_string(), &dist.display().to_string()).into_bytes(),
            Err(_) => a,
        };
        let same = if a == b {
            true
        } else if name.ends_with(".json") || name.ends_with(".jsonld") {
            normalised_json(&String::from_utf8_lossy(&a)) == normalised_json(&String::from_utf8_lossy(&b))
        } else if name.ends_with(".nii") {
            // Same voxels to float precision; FFT-based solvers on a thread pool are not
            // bit-reproducible run to run.
            let (x, y) = (qsm_core::io::read_nifti_file(&local_deriv.join(rel)).unwrap().data,
                          qsm_core::io::read_nifti_file(&deriv.join(rel)).unwrap().data);
            x.len() == y.len() && x.iter().zip(&y).all(|(p, q)| (p - q).abs() <= 1e-6 * (1.0 + p.abs()))
        } else {
            false
        };
        if !same { differ.push(rel.clone()); }
    }
    assert!(differ.is_empty(), "files that differ between the local run and the SLURM jobs: {differ:?}");

    // qsmxt.log carries every worker's log, then the aggregation's own lines.
    let log = std::fs::read_to_string(deriv.join("qsmxt.log")).unwrap();
    for l in &logs {
        let worker = std::fs::read_to_string(deriv.join("logs").join(l)).unwrap();
        assert!(log.contains(&format!("==> logs/{l} <==")), "{log}");
        assert!(log.contains(&worker), "qsmxt.log is missing {l}");
    }
    // Provenance records the command the user ran, not the job's.
    let prov: String = std::fs::read_dir(deriv.join("prov")).unwrap().flatten()
        .filter(|e| e.path().is_file())
        .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default()).collect();
    assert!(!prov.contains(" aggregate "), "the aggregation command leaked into provenance");
}

/// `qsmxt aggregate` over a run no worker processed — or one processed with other settings —
/// fails naming it, as a local run fails on a run that failed; the rest are still aggregated.
#[test]
fn aggregate_names_runs_that_were_not_processed_under_its_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_orientation_bids(&bids);
    let mut worker = default_run_args(bids.clone(), out.clone());
    worker.dry = false;
    worker.no_mem_limit = true;
    worker.no_dataset_steps = true;
    worker.include = Some(vec!["*acq-dir1*".to_string()]);
    worker.pipeline = cheap_pipeline();
    super::run::execute(worker).unwrap();

    let keys = vec!["*acq-dir1*".to_string(), "*acq-dir2*".to_string()];
    let err = super::aggregate::execute(aggregate_args(bids.clone(), out.clone(), keys.clone()))
        .unwrap_err().to_string();
    assert!(err.contains("1 run(s) failed") && err.contains("acq-dir2") && err.contains("has not been processed"), "{err}");
    let deriv = out.join("derivatives/qsmxt");
    assert!(deriv.join("desc-reference_summary.tsv").exists(), "the processed run is still summarised");

    let mut other = aggregate_args(bids, out.clone(), vec!["*acq-dir1*".to_string()]);
    other.pipeline.add(&["--qsm-algorithm", "rts"]);
    let err = super::aggregate::execute(other).unwrap_err().to_string();
    assert!(err.contains("was processed with a different configuration"), "{err}");
}

/// Cleaning happens after the dataset-level steps, so asking a worker to clean is refused
/// rather than silently skipped.
#[test]
fn a_worker_refuses_clean_intermediates() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    testutils::create_single_echo_bids(&bids);
    let mut args = default_run_args(bids, dir.path().join("out"));
    args.dry = false;
    args.no_dataset_steps = true;
    args.clean_intermediates = true;
    let err = super::run::execute(args).unwrap_err().to_string();
    assert!(err.contains("qsmxt aggregate"), "{err}");
}
