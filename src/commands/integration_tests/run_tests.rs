use super::*;

// --- Run (dry) ---

#[test]
fn test_run_dry_single_echo() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_single_echo_bids(&bids);

    let mut args = default_run_args(bids, out);
    args.mem_limit_gb = Some(4.0);
    super::run::execute(args).unwrap();
}

#[test]
fn test_run_dry_multi_echo() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);

    let mut args = default_run_args(bids, out);
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.no_mem_limit = true;
    super::run::execute(args).unwrap();
}

#[test]
fn test_run_dry_empty_bids() {
    let dir = tempfile::tempdir().unwrap();
    let args = default_run_args(dir.path().to_path_buf(), dir.path().join("out"));
    let err = super::run::execute(args).unwrap_err().to_string();
    assert!(err.contains("no QSM-compatible runs found") && !err.contains("--include"), "{err}");
}

/// A filter that leaves nothing says so.
#[test]
fn run_with_a_filter_that_matches_nothing_fails() {
    let dir = tempfile::tempdir().unwrap();
    testutils::create_single_echo_bids(dir.path());
    let mut args = default_run_args(dir.path().to_path_buf(), dir.path().join("out"));
    args.include = Some(vec!["*nothing-like-this*".to_string()]);
    let err = super::run::execute(args).unwrap_err().to_string();
    assert!(err.contains("no QSM-compatible runs found") && err.contains("--include"), "{err}");
}

/// A BIDS directory that does not exist (a mistyped path) is an error in every command that
/// reads one, and nothing is created in its place (the default output goes inside it).
#[test]
fn a_missing_bids_dir_is_an_error_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-dataset");
    let expect = |r: qsmxt_io::Result<()>| {
        let err = r.unwrap_err().to_string();
        assert!(err.contains("does not exist"), "{err}");
    };
    let mut run = default_run_args(missing.clone(), missing.clone());
    run.output_dir = None;
    expect(super::run::execute(run));
    let mut real = default_run_args(missing.clone(), missing.clone());
    real.output_dir = None;
    real.dry = false;
    expect(super::run::execute(real));
    expect(super::validate::execute(ValidateArgs { bids_dir: missing.clone(), include: None, exclude: None }));
    let mut agg = aggregate_args(missing.clone(), missing.clone(), vec![]);
    agg.output_dir = None;
    expect(super::aggregate::execute(agg));
    expect(super::slurm::execute(SlurmArgs {
        accept_recommendations: false, accept_reference_settings: false,
        bids_dir: missing.clone(), output_dir: None, account: "a".to_string(), partition: None,
        config: None, time: "01:00:00".to_string(), mem: None, cpus_per_task: 1, submit: false,
        include: None, exclude: None, num_echoes: None, force: false, clean_intermediates: false,
        debug: false, source_dicom: None, dicom_outputs: None, pipeline: Default::default(),
    }));
    assert!(!missing.exists(), "nothing is created for a missing dataset");

    // A file is not a dataset either.
    let file = dir.path().join("a-file");
    std::fs::write(&file, "").unwrap();
    let err = super::validate::execute(ValidateArgs { bids_dir: file, include: None, exclude: None })
        .unwrap_err().to_string();
    assert!(err.contains("is not a directory"), "{err}");
}

// --- Run (actual execution) ---

#[test]
fn test_run_single_echo_tkd() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_single_echo_bids(&bids);

    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    args.pipeline.add(&["--bf-algorithm", "vsharp"]);
    args.pipeline.add(&["--masking-input", "magnitude-first"]);
    args.dry = false;
    args.no_mem_limit = true;
    super::run::execute(args).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    assert!(deriv.join("sub-1/anat/sub-1_Chimap.nii").exists());
    assert!(deriv.join("sub-1/anat/sub-1_mask.nii").exists());
    assert!(deriv.join("sub-1/anat/sub-1_part-mag_T2starw.nii").exists());
    assert!(deriv.join("pipeline_config.toml").exists());
    assert!(deriv.join("dataset_description.json").exists());

    // BEP028 (BIDS-Prov) records
    assert!(deriv.join("prov/prov-qsmxt_base.json").exists());
    assert!(deriv.join("prov/prov-qsmxt_soft.json").exists());
    assert!(deriv.join("prov/prov-qsmxt_env.json").exists());
    assert!(deriv.join("prov/prov-qsmxt_act.json").exists());
    assert!(deriv.join("prov/prov-qsmxt_ent.json").exists());

    // The final QSM map has a GeneratedBy sidecar referencing the
    // reference-step activity that produced it.
    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(deriv.join("sub-1/anat/sub-1_Chimap.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(sidecar["GeneratedBy"], "bids::prov/#reference-sub-1");
    // SkullStripped is required for derivative anat images; the QSM map is
    // referenced within the brain mask.
    assert_eq!(sidecar["SkullStripped"], true);
    // The support the map is defined on is published beside it, generated by referencing.
    let support: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(deriv.join("sub-1/anat/sub-1_desc-qsm_mask.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(support["GeneratedBy"], "bids::prov/#reference-sub-1");
    assert!(support.get("SkullStripped").is_none(), "a mask takes no SkullStripped");
    assert!(deriv.join(".bidsignore").exists());

    // The referenced activity exists in the activity records.
    let act: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(deriv.join("prov/prov-qsmxt_act.json")).unwrap(),
    )
    .unwrap();
    assert!(act.get("bids::prov/#reference-sub-1").is_some());
}

#[test]
fn test_run_caching() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_single_echo_bids(&bids);

    let make_args = || {
        let mut args = default_run_args(bids.clone(), out.clone());
        args.pipeline.add(&["--qsm-algorithm", "tkd"]);
        args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
        args.pipeline.add(&["--bf-algorithm", "vsharp"]);
        args.pipeline.add(&["--masking-input", "magnitude-first"]);
        args.dry = false;
        args.no_mem_limit = true;
        args
    };

    // First run: produces all outputs
    super::run::execute(make_args()).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    let qsm = deriv.join("sub-1/anat/sub-1_Chimap.nii");
    let mask = deriv.join("sub-1/anat/sub-1_mask.nii");
    let mag = deriv.join("sub-1/anat/sub-1_part-mag_T2starw.nii");
    assert!(qsm.exists());
    assert!(mask.exists());
    assert!(mag.exists());

    // Record modification times
    let qsm_mtime = std::fs::metadata(&qsm).unwrap().modified().unwrap();
    let mask_mtime = std::fs::metadata(&mask).unwrap().modified().unwrap();

    // Brief pause so any re-written files would have a different mtime
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Second run: should skip all cached steps
    super::run::execute(make_args()).unwrap();

    // Outputs should still exist
    assert!(qsm.exists());
    assert!(mask.exists());
    assert!(mag.exists());

    // Files should NOT have been rewritten (mtimes unchanged)
    assert_eq!(std::fs::metadata(&qsm).unwrap().modified().unwrap(), qsm_mtime);
    assert_eq!(std::fs::metadata(&mask).unwrap().modified().unwrap(), mask_mtime);
}

#[test]
fn test_run_caching_survives_unrelated_config_change() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);

    let make_args = |do_t2star: bool| {
        let mut args = default_run_args(bids.clone(), out.clone());
        args.pipeline.add(&["--qsm-algorithm", "tkd"]);
        args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
        args.pipeline.add(&["--bf-algorithm", "vsharp"]);
        args.pipeline.add(&["--masking-input", "magnitude"]);
        args.dry = false;
        args.no_mem_limit = true;
        if do_t2star {
            args.pipeline.add(&["--do-t2starmap"]);
        }
        args
    };

    // First run: QSM only (no T2*)
    super::run::execute(make_args(false)).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    let qsm = deriv.join("sub-1/anat/sub-1_Chimap.nii");
    assert!(qsm.exists());
    let qsm_mtime = std::fs::metadata(&qsm).unwrap().modified().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(50));

    // Second run: add --do-t2starmap — should reuse cached QSM steps
    super::run::execute(make_args(true)).unwrap();

    // QSM should not have been rewritten
    assert_eq!(std::fs::metadata(&qsm).unwrap().modified().unwrap(), qsm_mtime);
    // T2* should now exist
    assert!(deriv.join("sub-1/anat/sub-1_T2starmap.nii").exists());
}

/// End to end, the behaviour #242 is actually about: a MESE on a grid of its own produces R2
/// and R2'. It used to be dropped on a voxel-count check, so neither map was ever written and
/// chi-separation quietly fell back to whatever was left — with nothing in the output saying
/// an acquisition had been discarded.
#[test]
fn test_run_computes_r2_from_a_mese_on_its_own_grid() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);

    // A 2D-style spin-echo companion: half the matrix, twice the slice pitch, covering the
    // same box as the 8x8x8 1 mm GRE. Nothing about it matches the GRE's grid.
    let anat = bids.join("sub-1/anat");
    let mese_dims = (4, 4, 4);
    let mese_affine = [
        2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for echo in 1..=4 {
        let te = 0.012 * echo as f64;
        // Decaying, so the EPG fit has something to find rather than R2 = 0 everywhere.
        let signal = 1000.0 * (-15.0 * te).exp();
        let data = vec![signal; mese_dims.0 * mese_dims.1 * mese_dims.2];
        qsm_core::io::save_nifti_to_file(
            &anat.join(format!("sub-1_echo-{}_MESE.nii", echo)),
            &data, mese_dims, (2.0, 2.0, 2.0), &mese_affine,
        ).unwrap();
        testutils::write_sidecar(
            &anat.join(format!("sub-1_echo-{}_MESE.json", echo)), te, 3.0);
    }

    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    args.pipeline.add(&["--bf-algorithm", "vsharp"]);
    args.pipeline.add(&["--masking-input", "magnitude"]);
    args.dry = false;
    args.no_mem_limit = true;
    args.pipeline.add(&["--do-r2starmap"]);
    args.pipeline.add(&["--do-r2map"]);
    args.pipeline.add(&["--do-r2primemap"]);
    // `mese` rather than `auto` deliberately: it removes the R2PRIMEnet fallback, so this can
    // only pass by actually measuring R2' from the spin-echo data, and the test never reaches
    // for network weights. Under the old voxel-count behaviour `auto` quietly went and
    // downloaded R2PRIMEnet instead — which is the "falls back to whatever remains" in #242,
    // and took eight minutes to do it.
    args.pipeline.add(&["--r2prime-strategy", "mese"]);
    super::run::execute(args).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    assert!(deriv.join("sub-1/anat/sub-1_R2map.nii").exists(),
            "a differently-gridded MESE must still yield R2");
    assert!(deriv.join("sub-1/anat/sub-1_R2primemap.nii").exists(),
            "and so R2' = R2* - R2");
    // On the GRE's grid, not the MESE's: everything downstream indexes it that way.
    let r2 = qsm_core::io::read_nifti_file(&deriv.join("sub-1/anat/sub-1_R2map.nii")).unwrap();
    assert_eq!(r2.dims, (8, 8, 8), "R2 belongs on the reconstruction grid");

    // The R2' sidecar says it was measured, not predicted (#236), next to its provenance.
    let side: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        deriv.join("sub-1/anat/sub-1_R2primemap.json")).unwrap()).unwrap();
    assert_eq!(side["R2primeSource"], "measured", "{side}");
    assert!(side["R2primeMethod"].as_str().unwrap().contains("EPG"), "{side}");
    assert!(side["GeneratedBy"].is_string(), "BIDS-Prov kept the source fields: {side}");
}

/// TFI is a total-field method (#265): it gets the field map and the brain mask, and no
/// background-removal stage runs in front of it.
#[test]
fn tfi_inverts_the_total_field_with_no_background_removal() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);
    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tfi"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    args.pipeline.add(&["--masking-input", "magnitude"]);
    args.dry = false;
    args.no_mem_limit = true;
    super::run::execute(args).unwrap();
    let deriv = out.join("derivatives/qsmxt");
    assert!(deriv.join("sub-1/anat/sub-1_Chimap.nii").exists());
    assert!(!deriv.join("workflow/sub-1/bgremove").exists(), "background removal ran in front of TFI");
    let prov: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        deriv.join("workflow/sub-1/invert/provenance.json")).unwrap()).unwrap();
    let inputs = prov["inputs"].to_string();
    assert!(inputs.contains("field-ppm") || inputs.contains("unwrap"), "TFI read {inputs}");
}

/// iQFM makes a local field; with an inversion that takes the total field or wrapped phase
/// it would do nothing, and used to leave the field map unwritten (#276).
#[test]
fn iqfm_with_a_total_field_inversion_is_refused_before_processing() {
    for alg in ["tgv", "qsmart", "autoqsm", "nextqsm", "tfi"] {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        testutils::create_multi_echo_bids(&bids);
        let mut args = default_run_args(bids, dir.path().join("out"));
        args.pipeline.add(&["--qsm-algorithm", alg]);
        args.pipeline.add(&["--bf-algorithm", "iqfm"]);
        let err = super::run::execute(args).unwrap_err().to_string();
        assert!(err.contains("iqfm") && err.contains("no effect"), "{alg:?}: {err}");
    }
}

#[test]
fn test_run_multi_echo_with_extras() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);

    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    args.pipeline.add(&["--bf-algorithm", "vsharp"]);
    args.pipeline.add(&["--masking-input", "magnitude"]);
    args.pipeline.add(&["--phase-offset-removal", "true"]);
    args.dry = false;
    args.no_mem_limit = true;
    args.pipeline.add(&["--do-swi"]);
    args.pipeline.add(&["--do-t2starmap"]);
    args.pipeline.add(&["--do-r2starmap"]);
    super::run::execute(args).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    assert!(deriv.join("sub-1/anat/sub-1_Chimap.nii").exists());
    assert!(deriv.join("sub-1/anat/sub-1_swi.nii").exists());
    assert!(deriv.join("sub-1/anat/sub-1_T2starmap.nii").exists());
    assert!(deriv.join("sub-1/anat/sub-1_R2starmap.nii").exists());
}

/// The per-structure tables are the point of `--do-analysis`, so the run has to produce one
/// covering every map it made — not χ alone — and gather them into a dataset-level table.
/// The segmentation is supplied so the test needs no network weights.
#[test]
fn analysis_tabulates_every_map_per_structure() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    // Large enough that χ is defined inside the structures after background removal's erosion.
    testutils::create_multi_echo_bids_on(&bids, testutils::STRUCTURES_N);
    testutils::write_dseg_on(&bids.join("derivatives/mydseg/sub-1/anat/sub-1_dseg.nii"), testutils::STRUCTURES_N);

    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    args.pipeline.add(&["--bf-algorithm", "vsharp"]);
    args.pipeline.add(&["--masking-input", "magnitude"]);
    args.dry = false;
    args.no_mem_limit = true;
    args.pipeline.add(&["--do-t2starmap"]);
    args.pipeline.add(&["--do-r2starmap"]);
    args.pipeline.add(&["--do-analysis"]);
    args.pipeline.add(&["--use-custom-dseg", "mydseg"]);
    super::run::execute(args).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    let per_run = std::fs::read_to_string(
        deriv.join("sub-1/anat/sub-1_desc-segmentation_stats.tsv")).unwrap();
    let header: Vec<&str> = per_run.lines().next().unwrap().split('\t').collect();
    for col in ["map", "unit", "index", "name", "n_voxels", "n_valid",
                "mean", "sd", "median", "min", "max", "p5", "p95"] {
        assert!(header.contains(&col), "column {col} missing from {header:?}");
    }
    let maps: std::collections::HashSet<&str> =
        per_run.lines().skip(1).map(|l| l.split('\t').next().unwrap()).collect();
    assert!(maps.contains("Chimap"), "{maps:?}");
    assert!(maps.contains("T2starmap"), "{maps:?}");
    assert!(maps.contains("R2starmap"), "{maps:?}");

    // Every row is complete, and min <= median <= max on all of them.
    for line in per_run.lines().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        assert_eq!(c.len(), header.len(), "ragged row: {line}");
        let f = |i: usize| c[i].parse::<f64>().unwrap();
        let (median, min, max) = (f(9), f(10), f(11));
        assert!(min <= median && median <= max, "{line}");
        assert!(f(5) >= 1.0, "a row must summarise at least one voxel: {line}");
    }

    // A figure per map, beside the table, named so it sorts next to it.
    let figs: Vec<String> = std::fs::read_dir(deriv.join("sub-1/anat")).unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .filter(|n| n.ends_with("_stats.png"))
        .collect();
    for want in ["sub-1_desc-chimap_stats.png", "sub-1_desc-t2starmap_stats.png",
                 "sub-1_desc-r2starmap_stats.png"] {
        assert!(figs.contains(&want.to_string()), "missing {want}: {figs:?}");
    }
    // Real PNGs, not empty files.
    for f in &figs {
        let p = deriv.join("sub-1/anat").join(f);
        let head = std::fs::read(&p).unwrap();
        assert!(head.starts_with(&[0x89, b'P', b'N', b'G']), "{f} is not a PNG");
        assert!(head.len() > 1000, "{f} is suspiciously small");
    }

    // The dataset-level table repeats those rows behind the run's BIDS entities.
    let group = std::fs::read_to_string(deriv.join("desc-segmentation_stats.tsv")).unwrap();
    assert!(group.lines().next().unwrap().starts_with("subject\tsession\t"));
    assert_eq!(group.lines().count(), per_run.lines().count());
    assert!(group.lines().skip(1).all(|l| l.starts_with("1\t\t\t\t\t\t")),
            "every row should carry sub-1 and no other entities");
}

/// Every NIfTI under `dir`, recursively, with its modification time.
fn niftis_with_mtime(dir: &std::path::Path) -> Vec<(PathBuf, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "nii") {
                out.push((p.clone(), e.metadata().unwrap().modified().unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// `--clean-intermediates` deleted published outputs recorded under a non-final step (the
/// combined `rec-mcpc3ds` echoes, under `scale_phase`) and the state file, so the next run
/// recomputed everything (#271). Now it removes only `workflow/` images, keeps the state, and
/// an immediate re-run with the same configuration recomputes nothing.
#[test]
fn cleaning_intermediates_keeps_published_outputs_and_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_coil_bids(&bids, 2);
    let mut args = default_run_args(bids, out.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.clean_intermediates = true;
    // What is cleaned does not depend on the algorithms; TKD and Laplacian are the cheapest.
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    let mut rerun = default_run_args(args.bids_dir.clone(), out.clone());
    rerun.pipeline.add(&["--qsm-algorithm", "tkd"]);
    rerun.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    super::run::execute(args).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    let anat = deriv.join("sub-1/ses-1/anat");
    let combined: Vec<_> = std::fs::read_dir(&anat).unwrap().flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("rec-mcpc3ds") && n.ends_with(".nii"))
        .collect();
    assert!(!combined.is_empty(), "the combined echoes were cleaned out of anat/");
    assert!(niftis_with_mtime(&deriv.join("workflow")).is_empty(), "workflow/ still holds images");
    let state = std::fs::read_dir(deriv.join("workflow")).is_ok();
    assert!(state, "the workflow directory (and its state file) is gone");

    let before = niftis_with_mtime(&deriv);
    rerun.dry = false;
    rerun.no_mem_limit = true;
    rerun.clean_intermediates = true;
    super::run::execute(rerun).unwrap();
    assert_eq!(niftis_with_mtime(&deriv), before, "an immediate re-run recomputed something");
}

/// The multi-orientation fan-in reads each member's local field, an intermediate: cleaning
/// has to wait until it has run (#271).
#[test]
fn cleaning_intermediates_waits_for_the_multi_orientation_fan_in() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_orientation_bids(&bids);
    let mut args = default_run_args(bids, out.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.clean_intermediates = true;
    args.include = Some(vec!["*acq-dir*".to_string()]);
    args.pipeline.add(&["--orientation-group", "acq"]);
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    super::run::execute(args).unwrap();
    let deriv = out.join("derivatives/qsmxt");
    assert!(deriv.join("sub-1/ses-1/anat/sub-1_ses-1_desc-cosmos_Chimap.nii").exists());
    assert!(niftis_with_mtime(&deriv.join("workflow")).is_empty(), "workflow/ still holds images");
}

/// `--orientation-reference` decides the group's frame: the sidecar names it, whichever run
/// sorts first.
#[test]
fn the_orientation_reference_is_the_groups_frame() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_orientation_bids(&bids);
    let mut args = default_run_args(bids, out.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.include = Some(vec!["*acq-dir*".to_string()]);
    args.pipeline.add(&["--orientation-group", "acq"]);
    args.pipeline.add(&["--orientation-reference", "dir3"]);
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    super::run::execute(args).unwrap();
    let sidecar = out.join("derivatives/qsmxt/sub-1/ses-1/anat/sub-1_ses-1_desc-cosmos_Chimap.json");
    let sidecar: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(sidecar).unwrap()).unwrap();
    let reference = sidecar["ReferenceOrientation"].as_str().unwrap();
    assert!(reference.contains("acq-dir3"), "{reference}");

    // The group was combined, so each member's methods and the dataset's say so (#496).
    let deriv = out.join("derivatives/qsmxt");
    let combined = "Local field maps from multiple head orientations";
    let dataset = std::fs::read_to_string(deriv.join("methods.md")).unwrap();
    assert!(dataset.contains(combined) && dataset.contains("(COSMOS; Liu et al., 2009)"), "{dataset}");
    let run_methods: Vec<_> = glob::glob(&deriv.join("workflow/**/*_methods.md").to_string_lossy())
        .unwrap().flatten().collect();
    assert_eq!(run_methods.len(), 3, "{run_methods:?}");
    for path in &run_methods {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains(combined), "{}: {text}", path.display());
    }
}

/// A pattern that groups nothing processes every run alone, and neither the run's methods nor
/// the dataset's say otherwise (#496).
#[test]
fn a_group_that_never_formed_is_not_in_the_methods() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_orientation_bids(&bids);
    let mut args = default_run_args(bids, out.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.include = Some(vec!["*acq-highres*".to_string()]);
    args.pipeline.add(&["--orientation-group", "acq"]);
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    super::run::execute(args).unwrap();
    let deriv = out.join("derivatives/qsmxt");
    let mut files: Vec<_> = glob::glob(&deriv.join("workflow/**/*_methods.md").to_string_lossy())
        .unwrap().flatten().collect();
    assert_eq!(files.len(), 1, "{files:?}");
    files.push(deriv.join("methods.md"));
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(!text.contains("COSMOS") && !text.contains("orientation"), "{}: {text}", path.display());
    }
}

/// DICOM export happens after the run and can come to nothing without failing it, so the
/// methods say it only of a run that wrote series, and a later run's export is taken back into
/// them (#504).
#[test]
fn methods_say_dicom_export_only_when_series_were_written() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);
    let args = |outputs: Option<Vec<String>>| {
        let mut args = default_run_args(bids.clone(), out.clone());
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.add(&["--export-dicom"]);
        args.pipeline.add(&["--qsm-algorithm", "tkd"]);
        args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
        args.dicom_outputs = outputs;
        args
    };
    let deriv = out.join("derivatives/qsmxt");
    let texts = || {
        let mut files: Vec<_> = glob::glob(&deriv.join("workflow/**/*_methods.md").to_string_lossy())
            .unwrap().flatten().collect();
        assert_eq!(files.len(), 1, "{files:?}");
        files.push(deriv.join("methods.md"));
        files.iter().map(|p| std::fs::read_to_string(p).unwrap()).collect::<Vec<_>>()
    };
    // Only R2 maps, which this run does not make: nothing is written.
    super::run::execute(args(Some(vec!["R2map".to_string()]))).unwrap();
    for text in texts() {
        assert!(!text.contains("DICOM"), "{text}");
    }
    super::run::execute(args(None)).unwrap();
    for text in texts() {
        assert!(text.contains("Final maps were additionally exported as DICOM series."), "{text}");
    }
}

/// A reference that matches no orientation is refused before any run's pipeline starts,
/// not after hours of them at the fan-in.
#[test]
fn an_unmatched_orientation_reference_refuses_the_run_up_front() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_orientation_bids(&bids);
    let mut args = default_run_args(bids, out.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.include = Some(vec!["*acq-dir*".to_string()]);
    args.pipeline.add(&["--orientation-group", "acq"]);
    args.pipeline.add(&["--orientation-reference", "Neutral"]);
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    let err = super::run::execute(args).unwrap_err().to_string();
    assert!(err.contains("orientation reference 'Neutral' matches none"), "{err}");
    assert!(!out.join("derivatives/qsmxt/workflow").exists(), "a pipeline ran before the refusal");
}

/// The table summarises what the run made, so a run that makes no susceptibility map at all
/// still produces one. `--no-qsm --do-r2starmap --do-analysis` has to stay a no-QSM run:
/// reconstruction must not be switched back on behind the request.
#[test]
fn analysis_works_without_qsm() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);
    testutils::write_dseg(&bids.join("derivatives/mydseg/sub-1/anat/sub-1_dseg.nii"));

    let mut args = default_run_args(bids, out.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.pipeline.add(&["--no-qsm"]);
    args.pipeline.add(&["--do-r2starmap"]);
    args.pipeline.add(&["--do-analysis"]);
    args.pipeline.add(&["--use-custom-dseg", "mydseg"]);
    super::run::execute(args).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    assert!(!deriv.join("sub-1/anat/sub-1_Chimap.nii").exists(),
            "--no-qsm must survive: the analysis needs a map, not χ specifically");

    let table = std::fs::read_to_string(
        deriv.join("sub-1/anat/sub-1_desc-segmentation_stats.tsv")).unwrap();
    let maps: std::collections::HashSet<&str> =
        table.lines().skip(1).map(|l| l.split('\t').next().unwrap()).collect();
    // The R2*/T2* stage always writes both — T2* is 1/R2* — so both are there to summarise,
    // and χ is not.
    assert_eq!(maps, ["R2starmap", "T2starmap"].into_iter().collect(), "{maps:?}");
    assert!(deriv.join("desc-segmentation_stats.tsv").exists(), "the group table too");
}

/// Referencing to a structure means that structure's mean χ ends up at zero — that is the
/// whole contract, and it is worth asserting on a real run rather than on the arithmetic
/// alone, because the map, the mask and the parcellation all have to line up for it to hold.
#[test]
fn qsm_can_be_referenced_to_a_parcellation_region() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    // Large enough that the thalami survive background removal's erosion.
    testutils::create_multi_echo_bids_on(&bids, testutils::STRUCTURES_N);
    let dseg_path = bids.join("derivatives/mydseg/sub-1/anat/sub-1_dseg.nii");
    testutils::write_dseg_on(&dseg_path, testutils::STRUCTURES_N);

    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    args.pipeline.add(&["--bf-algorithm", "vsharp"]);
    args.pipeline.add(&["--masking-input", "magnitude"]);
    args.dry = false;
    args.no_mem_limit = true;
    // Both thalami, supplied rather than segmented so the test needs no network weights.
    args.pipeline.add(&["--qsm-reference", "thalamus"]);
    args.pipeline.add(&["--do-analysis"]);
    args.pipeline.add(&["--use-custom-dseg", "mydseg"]);
    super::run::execute(args).unwrap();

    let deriv = out.join("derivatives/qsmxt");
    let chi = qsm_core::io::read_nifti_file(&deriv.join("sub-1/anat/sub-1_Chimap.nii")).unwrap();
    let dseg = qsm_core::io::read_nifti_file(&dseg_path).unwrap();
    let mask = qsm_core::io::read_nifti_file(&deriv.join("sub-1/anat/sub-1_mask.nii")).unwrap();

    let vals: Vec<f64> = chi.data.iter().zip(&dseg.data).zip(&mask.data)
        .filter(|((_, &l), &m)| m > 0.5 && matches!(l.round() as i32, 10 | 49))
        .map(|((&c, _), _)| c)
        .collect();
    assert!(!vals.is_empty(), "the test dseg should put thalamus voxels inside the mask");
    let mean = vals.iter().sum::<f64>() / vals.len() as f64;
    assert!(mean.abs() < 1e-9, "thalamus mean should be 0 after referencing, was {mean}");

    // Only the offset moved: voxels outside the brain mask stay zero, as the other
    // referencing modes leave them.
    assert!(chi.data.iter().zip(&mask.data).all(|(&c, &m)| m > 0.5 || c == 0.0));

    // The reference is recorded beside the map it produced, with what it removed.
    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(deriv.join("sub-1/anat/sub-1_Chimap.json")).unwrap()).unwrap();
    assert_eq!(sidecar["QsmReference"], "thalamus");
    assert_eq!(sidecar["QsmReferenceLabels"], serde_json::json!([10, 49]));
    assert!(sidecar["QsmReferenceVoxels"].as_u64().unwrap() > 0);
    let offset = sidecar["QsmReferenceOffsetPpm"].as_f64().unwrap();
    assert!(offset.is_finite());
    // BIDS-Prov writes to the same sidecar and must not have wiped those fields.
    assert!(sidecar.get("GeneratedBy").is_some(), "provenance clobbered the reference fields");

    // And on every susceptibility row of the run's table, so the spreadsheet stands alone.
    let table = std::fs::read_to_string(
        deriv.join("sub-1/anat/sub-1_desc-segmentation_stats.tsv")).unwrap();
    let cols: Vec<&str> = table.lines().next().unwrap().split('\t').collect();
    let (ir, io) = (cols.iter().position(|c| *c == "reference").unwrap(),
                    cols.iter().position(|c| *c == "reference_offset_ppm").unwrap());
    let mut chi_rows = 0;
    for line in table.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f[0] == "Chimap" {
            chi_rows += 1;
            assert_eq!(f[ir], "thalamus", "{line}");
            assert!((f[io].parse::<f64>().unwrap() - offset).abs() < 1e-6, "{line}");
        } else {
            assert_eq!(f[ir], "", "{} has no reference to claim: {line}", f[0]);
        }
    }
    assert!(chi_rows > 0, "the run produced no susceptibility rows to check");

    // The reference summary names the run and what it referenced to.
    let summary = std::fs::read_to_string(deriv.join("desc-reference_summary.tsv")).unwrap();
    let mut lines = summary.lines();
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    assert_eq!(header, [
        "subject", "session", "acquisition", "reconstruction", "inversion", "run",
        "map", "reference", "offset_ppm", "n_voxels",
    ], "{summary}");
    let row: Vec<&str> = lines.next().unwrap().split('\t').collect();
    assert_eq!(row.len(), header.len(), "{summary}");
    assert_eq!((row[0], row[6], row[7]), ("1", "Chimap", "thalamus"), "{summary}");
    assert!((row[8].parse::<f64>().unwrap() - offset).abs() < 1e-6, "{summary}");
}

/// A mistyped region has to fail before any data is touched — the alternative is an hours-long
/// run whose maps are referenced to something nobody asked for.
#[test]
fn an_unknown_reference_region_fails_before_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);

    let mut args = default_run_args(bids, out.clone());
    args.dry = false;
    args.no_mem_limit = true;
    args.pipeline.add(&["--qsm-reference", "thalmus"]); // typo
    let err = format!("{}", super::run::execute(args).unwrap_err());
    assert!(err.contains("unknown reference region"), "{err}");
    assert!(err.contains("thalamus"), "the error should list the valid names: {err}");
    assert!(!out.join("derivatives/qsmxt/sub-1/anat/sub-1_Chimap.nii").exists(),
            "nothing should have been reconstructed");
}

/// The one single-echo TGV run, for the call site that hands TGV echo 1's wrapped phase (what
/// it is handed is `tgv_gets_the_field_in_ppm_or_echo_1_phase_with_te_in_s_and_b0_in_t`), with
/// one iteration; also inhomogeneity correction and a phase-quality mask on a single echo.
#[test]
fn test_run_single_echo_tgv() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_single_echo_bids(&bids);

    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tgv"]);
    args.pipeline.add(&["--masking-input", "magnitude-first"]);
    args.pipeline.add(&["--tgv-iterations", "1"]);
    args.pipeline.add(&["--tgv-erosions", "0"]);
    args.dry = false;
    args.no_mem_limit = true;
    args.pipeline.add(&["--inhomogeneity-correction"]);
    args.pipeline.add(&["--mask", "phase-quality,threshold:otsu"]);
    super::run::execute(args).unwrap();

    assert!(out.join("derivatives/qsmxt/sub-1/anat/sub-1_Chimap.nii").exists());
}

#[test]
fn test_run_with_mask_ops() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_single_echo_bids(&bids);

    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
    args.pipeline.add(&["--bf-algorithm", "vsharp"]);
    args.dry = false;
    args.no_mem_limit = true;
    args.clean_intermediates = true;
    args.pipeline.add(&["--mask", "phase-quality,threshold:otsu,dilate:1,erode:1"]);
    super::run::execute(args).unwrap();

    assert!(out.join("derivatives/qsmxt/sub-1/anat/sub-1_Chimap.nii").exists());
}

// --- TGV's inputs (multi-echo TGV used to get the field in ppm as if it were rad) ---

/// What a `qsmxt run` hands TGV, for a dataset with `echo_times` (s) at `b0_t` (T): the
/// plan's TGV step (what it reads), the run's metadata and the step's parameters.
fn tgv_wiring(echo_times: &[f64], b0_t: f64)
    -> (qsmxt_engine::plan::Step, qsmxt_engine::scan::RunMetadata, qsmxt_engine::PipelineConfig)
{
    let dir = tempfile::tempdir().unwrap();
    let anat = dir.path().join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();
    let suffix = if echo_times.len() > 1 { "MEGRE" } else { "T2starw" };
    for (e, &te) in echo_times.iter().enumerate() {
        let echo = if echo_times.len() > 1 { format!("_echo-{}", e + 1) } else { String::new() };
        for part in ["phase", "mag"] {
            let stem = anat.join(format!("sub-1{echo}_part-{part}_{suffix}"));
            if part == "phase" { testutils::write_phase(&stem.with_extension("nii")) }
            else { testutils::write_magnitude(&stem.with_extension("nii")) }
            testutils::write_sidecar(&stem.with_extension("json"), te, b0_t);
        }
    }
    let mut args = default_run_args(dir.path().to_path_buf(), dir.path().join("out"));
    args.pipeline.add(&["--qsm-algorithm", "tgv"]);
    args.pipeline.add(&["--tgv-alpha1", "0.0021"]);
    args.pipeline.add(&["--tgv-alpha0", "0.0037"]);
    let config = super::run::build_config(None, &args.pipeline).unwrap();
    let runs = qsmxt_io::bids::discovery::discover_runs(dir.path(), &Default::default()).unwrap();
    let facts = qsmxt_io::pipeline::requirements::scan_facts(&runs[0], &config);
    let plan = qsmxt_engine::plan::plan(&config, &facts);
    let step = plan.steps.iter()
        .find(|s| s.stage == qsmxt_engine::plan::Stage::Tgv(qsmxt_engine::plan::Pass::Main))
        .expect("a TGV run has a TGV step").clone();
    let header = qsm_core::io::read_nifti_file(&runs[0].echoes[0].phase_nifti).unwrap();
    let meta = qsmxt_engine::scan::resolve_geometry(header.dims, header.voxel_size, header.affine, &facts, &config);
    (step, meta, config)
}

/// Multi-echo TGV gets the B0 field map in ppm (QSM.rs#170); single-echo TGV gets echo 1's
/// wrapped phase, with TE1 in s and B0 in T for qsm-core to convert it; the alphas are in
/// radians at TE1 and B0 whichever the input, so both are in the step's parameters. `stage_tgv` used to pass the ppm field to a solver expecting rad at
/// TE1, which made χ 2π·γ·B0·TE1 too small. That TGV then lands on the right scale is
/// QSM.rs's test (`chi_comes_back_on_the_phantom_scale`, QSM.rs#204); this checks what QSMxT
/// passes it, from the BIDS sidecars and the flags.
#[test]
fn tgv_gets_the_field_in_ppm_or_echo_1_phase_with_te_in_s_and_b0_in_t() {
    use qsmxt_engine::store::Item;
    use qsmxt_engine::stages::inversion::{tgv_params, tgv_params_from_config, tgv_source, TgvSource};

    let (step, meta, config) = tgv_wiring(&[0.005, 0.010, 0.015], 7.0);
    assert!(step.reads.contains(&Item::FieldPpm), "{:?}", step.reads);
    assert!(!step.reads.iter().any(|i| matches!(i, Item::Phase(_))), "{:?}", step.reads);
    assert_eq!(tgv_source(&meta), TgvSource::FieldPpm);
    let p = tgv_params(&config, &meta);
    assert_eq!(p["input"], "field_ppm", "{p}");
    assert_eq!((p["te_ms"].as_f64(), p["field_strength"].as_f64()), (Some(5.0), Some(7.0)), "{p}");
    assert_eq!(p["alphas_rad"], serde_json::json!([0.0021, 0.0037]), "{p}");
    let core = tgv_params_from_config(&config.inversion.tgv);
    assert_eq!((core.alpha1, core.alpha0), (0.0021, 0.0037));

    let (step, meta, config) = tgv_wiring(&[0.010], 7.0);
    assert!(step.reads.contains(&Item::Phase(1)), "{:?}", step.reads);
    assert!(!step.reads.contains(&Item::FieldPpm), "{:?}", step.reads);
    assert_eq!(tgv_source(&meta), TgvSource::WrappedPhase);
    // What `stages::inversion::tgv` gives qsm-core as `te_s` and `b0_t`.
    assert_eq!((meta.echo_times[0], meta.field_strength), (0.010, 7.0));
    let p = tgv_params(&config, &meta);
    assert_eq!(p["input"], "wrapped_phase", "{p}");
    assert_eq!((p["te_ms"].as_f64(), p["field_strength"].as_f64()), (Some(10.0), Some(7.0)), "{p}");
    assert_eq!(p["alphas_rad"], serde_json::json!([0.0021, 0.0037]), "{p}");
}

/// A requested output the data cannot produce stops the command before any run starts,
/// naming the run and what is missing; the dry run says so too (#273).
#[test]
fn swi_on_a_phase_only_dataset_fails_before_processing() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_echo_bids(&bids);
    for e in std::fs::read_dir(bids.join("sub-1/anat")).unwrap().flatten() {
        if e.file_name().to_string_lossy().contains("part-mag") {
            std::fs::remove_file(e.path()).unwrap();
        }
    }
    let mut args = default_run_args(bids, out.clone());
    args.pipeline.add(&["--do-swi"]);
    let err = super::run::execute(args).unwrap_err().to_string();
    assert!(err.contains("sub-1") && err.contains("--do-swi") && err.contains("magnitude"), "{err}");
    assert!(!out.join("derivatives/qsmxt/sub-1").exists(), "a run was processed anyway");
}

/// MCPC-3D-S needs two echoes. A single-echo uncombined-coil run used to reach an assertion in
/// qsm-core; now `qsmxt run` refuses it, and the stage itself returns an error to any direct
/// caller (#273).
#[test]
fn a_single_echo_uncombined_coil_run_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    let out = dir.path().join("out");
    testutils::create_multi_coil_bids(&bids, 2);
    let mut args = default_run_args(bids.clone(), out.clone());
    args.num_echoes = Some(1);
    let err = super::run::execute(args).unwrap_err().to_string();
    assert!(err.contains("MCPC-3D-S") && err.contains("2 echoes"), "{err}");

    let filter = qsmxt_io::bids::discovery::DiscoveryFilter { num_echoes: Some(1), ..Default::default() };
    let runs = qsmxt_io::bids::discovery::discover_runs(&bids, &filter).unwrap();
    let coil_run = runs.iter().find(|r| r.coils.is_some()).expect("an uncombined-coil run");
    let output = qsmxt_io::bids::derivatives::DerivativeOutputs::new(&out.join("direct"));
    let err = qsmxt_io::pipeline::runner::run_pipeline_cached(
        coil_run, &qsmxt_engine::PipelineConfig::default(), &output, true, &|_| {},
    ).unwrap_err().to_string();
    assert!(err.contains("MCPC-3D-S"), "{err}");
}
