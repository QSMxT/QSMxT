use super::*;

/// `qsmxt r2prime --r2primenet` needs `--field-strength`, takes no R2 map, and refuses a
/// field strength other than 3 T (before any weights are fetched) unless told to run anyway.
#[test]
fn test_r2prime_r2primenet_flags() {
    use clap::Parser;
    let parse = |extra: &[&str]| {
        let mut v = vec!["qsmxt", "r2prime", "--r2star", "r2s.nii", "-m", "m.nii", "-o", "o.nii"];
        v.extend_from_slice(extra);
        crate::cli::Cli::try_parse_from(v).map(|c| c.command)
    };
    assert!(parse(&[]).is_err(), "neither --r2 nor --r2primenet");
    assert!(parse(&["--r2primenet"]).is_err(), "--r2primenet without --field-strength");
    assert!(parse(&["--r2", "r2.nii", "--r2primenet", "--field-strength", "3"]).is_err());
    assert!(parse(&["--r2", "r2.nii", "--field-strength", "3"]).is_err(), "--field-strength is R2PRIMEnet's");
    assert!(parse(&["--r2", "r2.nii"]).is_ok());
    let Command::R2prime(args) = parse(&["--r2primenet", "--field-strength", "7"]).unwrap() else { panic!() };
    let err = super::r2prime::execute(args).unwrap_err();
    assert!(format!("{err}").contains("--r2primenet-ignore-field-mismatch"), "{err}");
    let Command::R2prime(args) = parse(&["--r2primenet", "--field-strength", "3", "--r2primenet-patch", "96", "96", "40"]).unwrap() else { panic!() };
    let err = super::r2prime::execute(args).unwrap_err();
    assert!(format!("{err}").contains("multiple of 16"), "{err}");
}

/// `qsmxt qsmart --intermediates DIR` writes QSMART's stage maps there under the names QSM-CI
/// compares them by (TKD as the inner inversion, to keep it quick on 8³).
#[test]
fn test_qsmart_writes_its_intermediates() {
    use clap::Parser;
    let dir = tempfile::tempdir().unwrap();
    let p = |n: &str| dir.path().join(n).display().to_string();
    testutils::write_field(&dir.path().join("field.nii"));
    testutils::write_mask(&dir.path().join("mask.nii"));
    let cli = crate::cli::Cli::try_parse_from([
        "qsmxt", "qsmart", &p("field.nii"), "-m", &p("mask.nii"), "-o", &p("chi.nii"),
        "--field-strength", "3", "--echo-time", "0.02", "--qsmart-inversion", "tkd",
        "--intermediates", &p("inter"),
    ]).unwrap();
    let Command::Qsmart(args) = cli.command else { panic!() };
    super::qsmart::execute(args).unwrap();
    assert!(dir.path().join("chi.nii").exists());
    let mut written: Vec<String> = std::fs::read_dir(dir.path().join("inter")).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    written.sort();
    assert_eq!(written, ["chimap_stage1.nii", "chimap_stage2.nii", "localfield_stage1.nii",
                         "localfield_stage2.nii", "vessel_mask.nii"]);
    let vessels = qsm_core::io::read_nifti_file(&dir.path().join("inter/vessel_mask.nii")).unwrap();
    assert!(vessels.data.iter().all(|&v| v == 0.0 || v == 1.0));
}

#[test]
fn test_bgremove_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("local.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    // PDF weights the fit by a noise map made from the echoes' magnitudes.
    let magnitude = dir.path().join("mag.nii");
    testutils::write_magnitude(&magnitude);
    super::bgremove::execute(BgremoveCommand::Pdf(BgremovePdfArgs {
        common: common_bgremove(input.clone(), mask.clone(), output.clone()),
        scan: scan(&[0.02]),
        magnitude: vec![magnitude.clone()],
        settings: Default::default(),
    })).unwrap();
    assert!(output.exists());
    // One magnitude for two echo times is refused before PDF runs.
    let e = super::bgremove::execute(BgremoveCommand::Pdf(BgremovePdfArgs {
        common: common_bgremove(input, mask, output),
        scan: scan(&[0.01, 0.02]),
        magnitude: vec![magnitude],
        settings: Default::default(),
    })).unwrap_err().to_string();
    assert!(e.contains("magnitude of every echo"), "{e}");
}

// --- Homogeneity ---

#[test]
fn test_homogeneity() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("corrected.nii");
    testutils::write_magnitude(&input);

    super::homogeneity::execute(HomogeneityArgs {
        input, output: output.clone(), method: InhomogeneityMethodArg::Makehomogeneous,
        sigma: 4.0, nbox: 2, n4_params: Default::default(),
    }).unwrap();
    assert!(output.exists());
}

/// `--method n4` runs N4 with the --n4-* flags, and a grid it cannot shrink is an error naming
/// the subcommand's own flags.
#[test]
fn test_homogeneity_n4() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("corrected.nii");
    testutils::write_magnitude(&input);
    let n4 = |shrink| crate::cli::N4ParamArgs {
        n4_shrink_factor: Some(shrink), n4_max_iterations: Some(vec![2, 2]), ..Default::default()
    };
    let args = |shrink| HomogeneityArgs {
        input: input.clone(), output: output.clone(), method: InhomogeneityMethodArg::N4,
        sigma: 7.0, nbox: 15, n4_params: n4(shrink),
    };
    super::homogeneity::execute(args(2)).unwrap();
    assert!(output.exists());
    // --n4-geometry make-nii takes make_nii's header for the file's voxel size.
    std::fs::remove_file(&output).unwrap();
    let mut a = args(2);
    a.n4_params.n4_geometry = Some(qsmxt_engine::N4Geometry::MakeNii);
    super::homogeneity::execute(a).unwrap();
    assert!(output.exists());
    let e = super::homogeneity::execute(args(5)).unwrap_err().to_string();
    assert!(e.contains("--n4-shrink-factor") && e.contains("--method makehomogeneous"), "{e}");
}

// --- Resample ---

#[test]
fn test_resample() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("vol.nii");
    let output = dir.path().join("resampled.nii");
    testutils::write_magnitude(&input);

    super::resample::execute(ResampleArgs {
        input: Some(input), output: Some(output.clone()),
        phase: None, magnitude: None, phase_out: None, magnitude_out: None,
        noise_fill: false, no_noise_fill: false,
    }).unwrap();
    assert!(output.exists());
}

/// Wrapped phase goes through the complex domain, so it needs its magnitude alongside.
#[test]
fn test_resample_phase_with_magnitude() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let magnitude = dir.path().join("mag.nii");
    let phase_out = dir.path().join("phase_axial.nii");
    let magnitude_out = dir.path().join("mag_axial.nii");
    testutils::write_phase(&phase);
    testutils::write_magnitude(&magnitude);

    super::resample::execute(ResampleArgs {
        input: None, output: None,
        phase: Some(phase), magnitude: Some(magnitude),
        phase_out: Some(phase_out.clone()), magnitude_out: Some(magnitude_out.clone()),
        noise_fill: true, no_noise_fill: false,
    }).unwrap();
    assert!(phase_out.exists() && magnitude_out.exists());
}

/// `--noise-fill` only applies to the phase/magnitude pair, and the pair's outputs only
/// to the pair: given with a plain volume they are refused rather than ignored (#279).
#[test]
fn resample_refuses_flags_that_do_not_apply() {
    use clap::Parser;
    let parse = |a: &[&str]| crate::cli::Cli::try_parse_from(
        [&["qsmxt", "resample"][..], a].concat());
    assert!(parse(&["vol.nii", "--noise-fill"]).is_err());
    assert!(parse(&["vol.nii", "--no-noise-fill"]).is_err());
    assert!(parse(&["vol.nii", "--phase-out", "p.nii"]).is_err());
    assert!(parse(&["vol.nii", "--phase", "p.nii", "--magnitude", "m.nii"]).is_err());
    assert!(parse(&["--phase", "p.nii", "--magnitude", "m.nii", "--output", "o.nii"]).is_err());
    assert!(parse(&["--phase", "p.nii", "--magnitude", "m.nii", "--noise-fill"]).is_ok());
    // the old opt-out still parses, so existing scripts keep running
    assert!(parse(&["--phase", "p.nii", "--magnitude", "m.nii", "--no-noise-fill"]).is_ok());
    assert!(parse(&["vol.nii", "--output", "o.nii"]).is_ok());
}

/// `qsmxt segment` computes a segmentation, so the run's "use a supplied one" flag is refused.
#[test]
fn segment_refuses_use_custom_dseg() {
    use clap::Parser;
    let cli = crate::cli::Cli::try_parse_from(
        ["qsmxt", "segment", "mag.nii", "--output", "seg.nii", "--use-custom-dseg", "*"]).unwrap();
    let crate::cli::Command::Segment(args) = cli.command else { panic!("expected segment") };
    let err = format!("{}", super::segment::execute(args).unwrap_err());
    assert!(err.contains("--use-custom-dseg"), "{err}");
}

#[test]
fn test_resample_requires_an_input() {
    let err = super::resample::execute(ResampleArgs {
        input: None, output: None, phase: None, magnitude: None,
        phase_out: None, magnitude_out: None, noise_fill: false, no_noise_fill: false,
    });
    assert!(err.is_err(), "no input should be an error, not a panic");
}

// --- Quality Map ---

#[test]
fn test_quality_map() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let output = dir.path().join("quality.nii");
    testutils::write_phase(&phase);

    super::quality_map::execute(QualityMapArgs {
        phase, output: output.clone(),
        magnitude: None, phase2: None, te1: 0.02, te2: 0.04,
    }).unwrap();
    assert!(output.exists());
}

// --- SWI ---

#[test]
fn test_swi() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("swi.nii");
    let (mut phase, mut magnitude) = (vec![], vec![]);
    for e in 1..=2 {
        let (p, m) = (dir.path().join(format!("phase{e}.nii")), dir.path().join(format!("mag{e}.nii")));
        testutils::write_phase(&p);
        testutils::write_magnitude(&m);
        phase.push(p);
        magnitude.push(m);
    }

    // Two echoes, CLEAR-SWI's own mask.
    super::swi::execute(SwiArgs {
        phase: phase.clone(), magnitude: magnitude.clone(), tes: Some(vec![0.004, 0.008]), mask: None,
        output: output.clone(),
        mip: false, mip_output: None, filtered_phase: true, filtered_phase_output: None,
        swi_params: Default::default(),
    }).unwrap();
    assert!(output.exists());
    // The filtered phase beside it, by default.
    assert!(dir.path().join("swi_desc-swi_phase.nii").exists());

    // Several echoes need their echo times: none given, no sidecars.
    let err = super::swi::execute(SwiArgs {
        phase: phase.clone(), magnitude: magnitude.clone(), tes: None, mask: None,
        output: output.clone(), mip: false, mip_output: None, filtered_phase: false, filtered_phase_output: None,
        swi_params: Default::default(),
    }).unwrap_err().to_string();
    assert!(err.contains("echo times"), "{err}");
    // One magnitude per phase.
    let err = super::swi::execute(SwiArgs {
        phase, magnitude: magnitude[..1].to_vec(), tes: Some(vec![0.004, 0.008]), mask: None,
        output, mip: false, mip_output: None, filtered_phase: false, filtered_phase_output: None,
        swi_params: Default::default(),
    }).unwrap_err().to_string();
    assert!(err.contains("magnitude"), "{err}");
}

// --- R2* ---

#[test]
fn test_r2star() {
    let dir = tempfile::tempdir().unwrap();
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("r2star.nii");
    testutils::write_mask(&mask);

    let mut inputs = Vec::new();
    for i in 1..=3 {
        let p = dir.path().join(format!("echo{}.nii", i));
        testutils::write_magnitude(&p);
        inputs.push(p);
    }

    super::r2star::execute(R2starArgs {
        inputs, mask, output: output.clone(),
        echo_times: vec![0.004, 0.008, 0.012],
    }).unwrap();
    assert!(output.exists());
}

#[test]
fn test_r2star_mismatched_inputs() {
    let result = super::r2star::execute(R2starArgs {
        inputs: vec![PathBuf::from("a.nii"), PathBuf::from("b.nii"), PathBuf::from("c.nii")],
        mask: PathBuf::from("mask.nii"),
        output: PathBuf::from("out.nii"),
        echo_times: vec![0.004, 0.008],
    });
    assert!(result.is_err());
}

// --- T2* ---

#[test]
fn test_t2star() {
    let dir = tempfile::tempdir().unwrap();
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("t2star.nii");
    testutils::write_mask(&mask);

    let mut inputs = Vec::new();
    for i in 1..=3 {
        let p = dir.path().join(format!("echo{}.nii", i));
        testutils::write_magnitude(&p);
        inputs.push(p);
    }

    super::t2star::execute(T2starArgs {
        inputs, mask, output: output.clone(),
        echo_times: vec![0.004, 0.008, 0.012],
    }).unwrap();
    assert!(output.exists());
}

// --- Init ---

#[test]
fn test_init_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("config.toml");
    super::init::execute(InitArgs { output: Some(output.clone()) }).unwrap();
    assert!(output.exists());
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("[inversion]"));
}

#[test]
fn test_init_to_stdout() {
    super::init::execute(InitArgs { output: None }).unwrap();
}

// --- SWI with MIP ---

#[test]
fn test_swi_with_mip() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let mag = dir.path().join("mag.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("swi.nii");
    let mip = dir.path().join("mip.nii");
    testutils::write_phase(&phase);
    testutils::write_magnitude(&mag);
    testutils::write_mask(&mask);

    super::swi::execute(SwiArgs {
        phase: vec![phase], magnitude: vec![mag], tes: None, mask: Some(mask),
        output: output.clone(),
        mip: true, mip_output: Some(mip.clone()),
        filtered_phase: false, filtered_phase_output: Some(dir.path().join("hp.nii")),
        swi_params: Default::default(),
    }).unwrap();
    assert!(output.exists());
    assert!(mip.exists());
    assert!(dir.path().join("hp.nii").exists(), "--filtered-phase-output implies --filtered-phase");
}

// --- Quality map with all optional inputs ---

#[test]
fn test_quality_map_with_magnitude_and_phase2() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let mag = dir.path().join("mag.nii");
    let phase2 = dir.path().join("phase2.nii");
    let output = dir.path().join("quality.nii");
    testutils::write_phase(&phase);
    testutils::write_magnitude(&mag);
    testutils::write_phase(&phase2);

    super::quality_map::execute(QualityMapArgs {
        phase, output: output.clone(),
        magnitude: Some(mag), phase2: Some(phase2),
        te1: 0.004, te2: 0.008,
    }).unwrap();
    assert!(output.exists());
}
