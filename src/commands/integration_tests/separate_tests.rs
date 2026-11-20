use super::*;

// ── `qsmxt separate` and `qsmxt segment` ──
// Neither had any coverage: separate.rs was 0/151 lines and segment.rs 0/47, which put the
// whole workspace under the release gate.

fn separate_common(dir: &Path, prefix: &str) -> SeparateCommonArgs {
    let qsm = dir.join("qsm.nii");
    let mask = dir.join("mask.nii");
    testutils::write_field(&qsm);
    testutils::write_mask(&mask);
    SeparateCommonArgs {
        qsm,
        mask,
        output: dir.join(prefix),
        field_strength: 3.0,
        b0_direction: vec![0.0, 0.0, 1.0],
    }
}

/// The three maps land under the prefix, and χ_total is the sum of its parts — the property
/// that makes a separation a separation.
#[test]
fn test_separate_r2star_qsm() {
    let dir = tempfile::tempdir().unwrap();
    let common = separate_common(dir.path(), "sep");
    let r2star = dir.path().join("r2star.nii");
    testutils::write_field(&r2star);

    super::separate::execute(SeparateCommand::R2starQsm(SeparateR2starQsmArgs {
        common,
        r2star: Some(r2star),
        magnitude: vec![],
        echo_times: vec![],
        settings: Default::default(),
    }))
    .unwrap();

    let para = dir.path().join("sep_desc-paramagnetic_Chimap.nii");
    let dia = dir.path().join("sep_desc-diamagnetic_Chimap.nii");
    let total = dir.path().join("sep_desc-total_Chimap.nii");
    for p in [&para, &dia, &total] {
        assert!(p.exists(), "{} was not written", p.display());
    }
    let load = |p: &Path| qsm_core::io::read_nifti_file(p).unwrap().data;
    let (p, d, t) = (load(&para), load(&dia), load(&total));
    for i in 0..t.len() {
        assert!((p[i] - d[i] - t[i]).abs() < 1e-6 || (p[i] + d[i] - t[i]).abs() < 1e-6,
            "total is not the combination of the two sources at {i}: {} / {} / {}",
            p[i], d[i], t[i]);
    }
}

/// r2star-qsm needs an R2* map or the magnitude to fit one from; with neither it must say so
/// rather than run on nothing.
/// A patch χ-sepnet cannot take is refused before the images load or the weights download.
#[test]
fn separate_chi_sepnet_refuses_a_bad_frame_patch_first() {
    use clap::Parser;
    let cli = crate::cli::Cli::try_parse_from([
        "qsmxt", "separate", "chi-sepnet", "--local-field", "absent.nii", "--mask", "absent.nii",
        "--r2star", "absent.nii", "--output", "out", "--frame-patch", "100", "128", "64",
    ]).unwrap();
    let crate::cli::Command::Separate { command } = cli.command else { panic!() };
    let e = super::separate::execute(command).unwrap_err().to_string();
    assert!(e.contains("χ-sepnet frame patch D = 100 must be a positive multiple of 16"), "{e}");
}

#[test]
fn test_separate_r2star_qsm_needs_an_input() {
    let dir = tempfile::tempdir().unwrap();
    let err = super::separate::execute(SeparateCommand::R2starQsm(SeparateR2starQsmArgs {
        common: separate_common(dir.path(), "sep"),
        r2star: None,
        magnitude: vec![],
        echo_times: vec![],
        settings: Default::default(),
    }))
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("--r2star") && msg.contains("--magnitude"),
            "the error should name both ways in: {msg}");
}

/// A method taking R2' rather than R2*, to cover the other input-loading path.
#[test]
fn test_separate_wavesep() {
    let dir = tempfile::tempdir().unwrap();
    let r2prime = dir.path().join("r2prime.nii");
    testutils::write_field(&r2prime);

    super::separate::execute(SeparateCommand::Wavesep(SeparateWavesepArgs {
        common: separate_common(dir.path(), "wv"),
        r2prime,
        // Two iterations: this is a plumbing test, not a convergence one.
        settings: SettingsOf::from_flags(&["--wavesep-max-iter", "2"]),
    }))
    .unwrap();
    assert!(dir.path().join("wv_desc-paramagnetic_Chimap.nii").exists());
    assert!(dir.path().join("wv_desc-diamagnetic_Chimap.nii").exists());
}

/// DECOMPOSE fits the multi-echo magnitude signal directly, so it exercises the multi-echo
/// loading path the R2*/R2' methods do not.
#[test]
fn test_separate_decompose() {
    let dir = tempfile::tempdir().unwrap();
    let mags: Vec<PathBuf> = (1..=3)
        .map(|e| {
            let p = dir.path().join(format!("mag{e}.nii"));
            testutils::write_magnitude(&p);
            p
        })
        .collect();

    let common = separate_common(dir.path(), "dc");
    let args = |qsm_per_echo: Vec<PathBuf>| SeparateDecomposeArgs {
        common: common.clone(),
        magnitude: mags.clone(),
        qsm_per_echo,
        echo_times: vec![0.004, 0.008, 0.012],
        settings: SettingsOf::from_flags(&["--decompose-n-inner", "1", "--decompose-max-iterations", "1"]),
    };
    // The per-echo QSM needs one map per echo.
    let err = super::separate::execute(SeparateCommand::Decompose(args(vec![common.qsm.clone(); 2])))
        .unwrap_err().to_string();
    assert!(err.contains("--qsm-per-echo has 2 echoes but the magnitude has 3"), "{err}");
    super::separate::execute(SeparateCommand::Decompose(args(vec![common.qsm.clone(); 3]))).unwrap();
    assert!(dir.path().join("dc_desc-paramagnetic_Chimap.nii").exists());
    assert!(dir.path().join("dc_desc-total_Chimap.nii").exists());
}

/// χ-sep iLSQR takes the local field and R2' together — the four-input path.
#[test]
fn test_separate_chi_sep_ilsqr() {
    let dir = tempfile::tempdir().unwrap();
    let local_field = dir.path().join("lfs.nii");
    let r2prime = dir.path().join("r2p.nii");
    let mag = dir.path().join("mag.nii");
    testutils::write_field(&local_field);
    testutils::write_field(&r2prime);
    testutils::write_magnitude(&mag);

    super::separate::execute(SeparateCommand::ChiSepIlsqr(SeparateChiSepIlsqrArgs {
        common: separate_common(dir.path(), "il"),
        local_field,
        r2prime,
        settings: SettingsOf::from_flags(&["--chi-sep-ilsqr-pad", "2", "2", "2", "--chi-sep-ilsqr-cg-max-iter-lsqr", "1",
            "--chi-sep-ilsqr-cg-max-iter-sa", "1"]),
    }))
    .unwrap();
    assert!(dir.path().join("il_desc-paramagnetic_Chimap.nii").exists());
    assert!(dir.path().join("il_desc-diamagnetic_Chimap.nii").exists());
}

/// χ-sep MEDI makes its noise map and CSF mask from the echoes' magnitudes, so it needs their
/// echo times, one per echo.
#[test]
fn test_separate_chi_sep_medi() {
    let dir = tempfile::tempdir().unwrap();
    let local_field = dir.path().join("lfs.nii");
    let r2prime = dir.path().join("r2p.nii");
    let mag = dir.path().join("mag.nii");
    testutils::write_field(&local_field);
    testutils::write_field(&r2prime);
    testutils::write_magnitude(&mag);
    let args = |echo_times: Vec<f64>, csf_mask: Option<PathBuf>, prefix: &str| SeparateChiSepMediArgs {
        common: separate_common(dir.path(), prefix),
        local_field: local_field.clone(),
        r2prime: r2prime.clone(),
        magnitude: vec![mag.clone(); 3],
        echo_times,
        csf_mask,
        settings: SettingsOf::from_flags(&["--chi-sep-medi-pad", "0", "--chi-sep-medi-max-iter", "1", "--chi-sep-medi-cg-max-iter", "1"]),
    };
    let tes = vec![0.004, 0.008, 0.012];
    let err = super::separate::execute(SeparateCommand::ChiSepMedi(args(vec![0.004, 0.008], None, "me"))).unwrap_err().to_string();
    assert!(err.contains("2 echo times but the magnitude has 3 echoes"), "{err}");
    super::separate::execute(SeparateCommand::ChiSepMedi(args(tes.clone(), None, "me"))).unwrap();
    assert!(dir.path().join("me_desc-paramagnetic_Chimap.nii").exists());
    // `--csf-r2star nnls`: the CSF mask from the χ-separation toolbox's NNLS R2*.
    let mut nnls = args(tes.clone(), None, "nnls");
    nnls.settings.0.extend(SettingsOf::<crate::cli::ChiSepMediFlags>::from_flags(&["--chi-sep-medi-csf-r2star", "nnls"]).0);
    super::separate::execute(SeparateCommand::ChiSepMedi(nnls)).unwrap();
    assert!(dir.path().join("nnls_desc-paramagnetic_Chimap.nii").exists());

    // `--csf-mask none` is an all-zero CSF mask (no CSF term), the same as passing one.
    let empty = dir.path().join("no_csf.nii");
    qsm_core::io::save_nifti_to_file(&empty, &vec![0.0; testutils::N_VOXELS], (8, 8, 8), (1.0, 1.0, 1.0),
        &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]).unwrap();
    super::separate::execute(SeparateCommand::ChiSepMedi(args(tes.clone(), Some("none".into()), "none"))).unwrap();
    super::separate::execute(SeparateCommand::ChiSepMedi(args(tes, Some(empty), "file"))).unwrap();
    let read = |p: &str| fs_err::read(dir.path().join(format!("{p}_desc-paramagnetic_Chimap.nii"))).unwrap();
    assert_eq!(read("none"), read("file"));
}

/// `qsmxt segment` rejects a version it does not know rather than silently choosing one — the
/// version selects the label table, so the wrong one mislabels every structure.
#[test]
fn test_segment_rejects_an_unknown_version() {
    let dir = tempfile::tempdir().unwrap();
    let mag = dir.path().join("mag.nii");
    testutils::write_magnitude(&mag);
    let argv = ["qsmxt", "segment", &mag.display().to_string(), "-o", "dseg.nii", "--synthseg-version", "v3"];
    let msg = match <crate::cli::Cli as clap::Parser>::try_parse_from(argv) {
        Err(e) => e.to_string(),
        Ok(cli) => {
            let crate::cli::Command::Segment(args) = cli.command else { panic!("segment") };
            super::segment::execute(args).unwrap_err().to_string()
        }
    };
    assert!(msg.contains("v1") && msg.contains("v2"), "the error should name the options: {msg}");
}
