use super::*;

/// `qsmxt invert iqsm`/`iqsm-plus` and `qsmxt bgremove iqfm` parse, and read every echo of
/// a 4D phase (scaled into [-π, π], as `qsmxt run` reads it), every echo's magnitude and the
/// scan's echo times; a mismatch is an error naming both counts. The networks themselves are
/// QSM.rs's to test.
#[test]
fn test_phase_net_subcommands_load_every_echo() {
    use clap::Parser;
    let dir = tempfile::tempdir().unwrap();
    let p = |n: &str| dir.path().join(n);
    testutils::write_phase(&p("e1.nii"));
    testutils::write_magnitude(&p("m1.nii"));
    testutils::write_mask(&p("mask.nii"));
    let ph = qsm_core::io::read_nifti_file(&p("e1.nii")).unwrap();
    let mag = qsm_core::io::read_nifti_file(&p("m1.nii")).unwrap();
    let half: Vec<f64> = ph.data.iter().map(|v| v / 2.0).collect();
    let w = |name: &str, vols: &[&[f64]]| {
        qsmxt_io::nifti::write::write_volumes_4d(&p(name), vols, ph.dims, ph.voxel_size, &ph.affine).unwrap();
        p(name).display().to_string()
    };
    let (phase, magnitude) = (w("phase.nii", &[&ph.data, &half]), w("mag.nii", &[&mag.data, &mag.data]));
    let mag1 = w("mag1.nii", &[&mag.data]);
    let mask = p("mask.nii").display().to_string();

    let parse = |args: &[String]| crate::cli::Cli::try_parse_from(args).map(|c| c.command);
    let base = |sub: &[&'static str], mag: &str| -> Vec<String> {
        let mut v: Vec<String> = ["qsmxt"].iter().chain(sub).map(|s| s.to_string()).collect();
        v.extend([phase.clone(), "-m".into(), mask.clone(), "-o".into(), "o.nii".into(), "--magnitude".into(), mag.into(),
                  "--field-strength".into(), "3".into(), "--echo-times".into(), "0.005".into(), "0.01".into()]);
        v
    };
    for sub in [&["invert", "iqsm"][..], &["invert", "iqsm-plus"], &["bgremove", "iqfm"]] {
        let args = base(sub, &magnitude);
        let cmd = parse(&args).unwrap();
        let a = match cmd {
            Command::Invert { command: InvertCommand::Iqsm(a) } => a.net,
            Command::Invert { command: InvertCommand::IqsmPlus(a) } => a,
            Command::Bgremove { command: BgremoveCommand::Iqfm(a) } => a.net,
            other => panic!("{sub:?} parsed as {other:?}"),
        };
        let inputs = super::invert::load_phase_net_inputs(&a).unwrap();
        assert_eq!((inputs.phases.len(), inputs.magnitudes.len()), (2, 2), "{sub:?}");
        assert_eq!(inputs.metadata.echo_times, [0.005, 0.01]);
        assert!(inputs.phases.iter().flatten().all(|v| v.abs() <= std::f64::consts::PI + 1e-12));
    }

    // One echo time for two echoes, and a magnitude with one echo for a phase with two.
    let mut args = base(&["invert", "iqsm"], &magnitude);
    args.pop();
    let Command::Invert { command: InvertCommand::Iqsm(a) } = parse(&args).unwrap() else { panic!() };
    let err = super::invert::load_phase_net_inputs(&a.net).err().unwrap();
    assert!(format!("{err}").contains("2 echo(es) but 1 echo time"), "{err}");
    let args = base(&["invert", "iqsm"], &mag1);
    let Command::Invert { command: InvertCommand::Iqsm(a) } = parse(&args).unwrap() else { panic!() };
    let err = super::invert::load_phase_net_inputs(&a.net).err().unwrap();
    assert!(format!("{err}").contains("1 echo(es)"), "{err}");
}

/// `--echo-weighting` of `qsmxt invert iqsm` and `qsmxt bgremove iqfm`: defaults to
/// (magnitude × TE)², maps each value to QSM.rs's weighting, and is refused by `invert iqsm-plus`.
#[test]
fn test_iqsm_echo_weighting_flag() {
    use clap::Parser;
    use qsm_core::inversion::IqsmEchoWeighting as W;
    let parse = |sub: &[&str], extra: &[&str]| {
        let mut v = vec!["qsmxt"];
        v.extend_from_slice(sub);
        v.extend(["ph.nii", "-m", "m.nii", "-o", "o.nii", "--field-strength", "3", "--echo-times", "0.01"]);
        v.extend_from_slice(extra);
        Cli::try_parse_from(v).map(|c| c.command)
    };
    let weighting = |sub: &[&str], extra: &[&str]| -> W {
        let a: IqsmArgs = match parse(sub, extra).unwrap() {
            Command::Invert { command: InvertCommand::Iqsm(a) } => a,
            Command::Bgremove { command: BgremoveCommand::Iqfm(a) } => a,
            other => panic!("{sub:?} parsed as {other:?}"),
        };
        a.echo_weighting.into()
    };
    for sub in [&["invert", "iqsm"][..], &["bgremove", "iqfm"]] {
        assert_eq!(weighting(sub, &[]), W::MagnitudeSquaredTeSquared, "{sub:?}");
        assert_eq!(W::default(), W::MagnitudeSquaredTeSquared, "the pipeline's weighting");
        assert_eq!(weighting(sub, &["--echo-weighting", "mag-te2"]), W::MagnitudeTeSquared, "{sub:?}");
        assert_eq!(weighting(sub, &["--echo-weighting", "mag2-te2"]), W::MagnitudeSquaredTeSquared, "{sub:?}");
        assert!(parse(sub, &["--echo-weighting", "te2"]).is_err(), "{sub:?}");
    }
    assert!(parse(&["invert", "iqsm-plus"], &["--echo-weighting", "mag2-te2"]).is_err());
}

// --- Dipole Inversion ---

#[test]
fn test_invert_tkd() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    super::invert::execute(InvertCommand::Tkd(InvertTkdArgs {
        common: common_invert(input, mask, output.clone()),
        settings: Default::default(),
    })).unwrap();
    assert!(output.exists());
}

/// `invert whqsm` takes a magnitude only for a magnitude data weight, and refuses that weight
/// without one; an echo time it cannot resolve from the scan is refused.
#[test]
fn test_invert_whqsm_weight_and_echo_time() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);
    let whqsm = |flags: &[&str], echoes: &[f64]| InvertCommand::Whqsm(crate::cli::InvertWhqsmArgs {
        common: common_invert(input.clone(), mask.clone(), output.clone()),
        scan: scan(echoes),
        magnitude: vec![],
        settings: SettingsOf::from_flags(&[&["--whqsm-max-iter", "1"], flags].concat()),
    });
    let e = super::invert::execute(whqsm(&["--whqsm-weight", "in-vivo"], &[0.01, 0.02]))
        .unwrap_err().to_string();
    assert!(e.contains("--whqsm-weight in-vivo needs --magnitude"), "{e}");
    // flat-in-mask, like flat, takes no magnitude; the old name of the flag still works
    super::invert::execute(whqsm(&["--weight", "flat-in-mask"], &[0.01, 0.02])).unwrap();
    let e = super::invert::execute(whqsm(&["--whqsm-te", "spacing"], &[0.02, 0.01]))
        .unwrap_err().to_string();
    assert!(e.contains("echo spacing"), "{e}");
    super::invert::execute(whqsm(&["--te", "last"], &[0.01, 0.02])).unwrap();
    assert!(output.exists());
}

/// `invert tgv` takes a field in ppm like the other inversions, with the echo times and field
/// strength its weights are in units of. One iteration is enough for the call site.
#[test]
fn test_invert_tgv_runs_on_a_ppm_field() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    super::invert::execute(InvertCommand::Tgv(InvertTgvArgs {
        common: common_invert(input, mask, output.clone()),
        scan: scan(&[0.005, 0.01]),
        reference_divergence: true, reference_edge_erosion: true, reference_output_box: true,
        settings: SettingsOf::from_flags(&["--tgv-iterations", "1", "--tgv-erosions", "0"]),
    })).unwrap();
    assert!(output.exists());
}

// --- Invert algorithms ---

#[test]
fn test_invert_rts() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    super::invert::execute(InvertCommand::Rts(InvertRtsArgs {
        common: common_invert(input, mask, output.clone()),
        // A loose tolerance for speed.
        settings: SettingsOf::from_flags(&["--rts-tol", "0.5", "--rts-pad", "-1", "2", "0"]),
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_invert_tv() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    super::invert::execute(InvertCommand::Tv(InvertTvArgs {
        common: common_invert(input, mask, output.clone()),
        settings: Default::default(),
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_invert_lsqr() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    let magnitude = dir.path().join("mag.nii");
    testutils::write_magnitude(&magnitude);
    let args = |magnitude: Vec<std::path::PathBuf>| InvertLsqrArgs {
        common: common_invert(input.clone(), mask.clone(), output.clone()),
        magnitude,
        b0: 3.0,
        settings: SettingsOf::from_flags(&["--lsqr-max-iter", "5"]),
    };
    // LSQR is weighted by the magnitude, as the HEIDI package weights it.
    let err = super::invert::execute(InvertCommand::Lsqr(args(vec![]))).unwrap_err().to_string();
    assert!(err.contains("--magnitude"), "{err}");
    super::invert::execute(InvertCommand::Lsqr(args(vec![magnitude]))).unwrap();
    assert!(output.exists());
}

/// HEIDI runs its LSQR seed and then the NESTA loop; both budgets are cut to keep the test
/// quick, since what is under test is the wiring, not the reconstruction.
#[test]
fn test_invert_heidi() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    let magnitude = dir.path().join("mag.nii");
    testutils::write_magnitude(&magnitude);
    super::invert::execute(InvertCommand::Heidi(InvertHeidiArgs {
        common: common_invert(input, mask, output.clone()),
        scan: scan(&[0.02]),
        magnitude: vec![magnitude],
        seed: None,
        settings: SettingsOf::from_flags(&["--lsqr-max-iter", "5", "--heidi-continuation-steps", "1", "--heidi-inner-iterations", "2"]),
    })).unwrap();
    assert!(output.exists());
}

/// `invert heidi --seed` fills the cone of a map another inversion made (no LSQR), on the
/// same grid; a seed of another size is refused.
#[test]
fn test_invert_heidi_on_a_seed() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let seed = dir.path().join("seed.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);
    testutils::write_field(&seed);
    let args = |seed: std::path::PathBuf| InvertHeidiArgs {
        common: common_invert(input.clone(), mask.clone(), output.clone()),
        scan: scan(&[0.02]),
        magnitude: vec![],
        seed: Some(seed),
        settings: SettingsOf::from_flags(&["--heidi-continuation-steps", "1", "--heidi-inner-iterations", "2"]),
    };
    super::invert::execute(InvertCommand::Heidi(args(seed))).unwrap();
    assert!(output.exists());
    let small = dir.path().join("small.nii");
    testutils::write_mismatched_volume(&small, 0.0);
    assert!(super::invert::execute(InvertCommand::Heidi(args(small))).is_err());
}

/// `invert medi` takes every echo's magnitude (one file per echo here) with the echo times,
/// as `qsmxt run` gives MEDI them, and MEDI-mSMV with three echoes; a magnitude count that
/// does not match the echo times is refused.
#[test]
fn test_invert_medi_takes_every_echo() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);
    let mags: Vec<PathBuf> = (0..3).map(|e| {
        let p = dir.path().join(format!("mag{e}.nii"));
        testutils::write_magnitude_decayed(&p, (-0.2 * e as f64).exp());
        p
    }).collect();
    let medi = |magnitude: Vec<PathBuf>, msmv: bool| InvertCommand::Medi(InvertMediArgs {
        common: common_invert(input.clone(), mask.clone(), output.clone()),
        scan: scan(&[0.005, 0.010, 0.015]),
        magnitude,
        smv: None, smv_shrink_mask: None,
        settings: SettingsOf::from_flags(&[
            &["--medi-merit", "false", "--no-medi-smv", "--medi-cg-max-iter", "2", "--medi-max-iter", "1"][..],
            if msmv { &["--medi-msmv"] } else { &[] },
        ].concat()),
    });
    super::invert::execute(medi(mags.clone(), false)).unwrap();
    assert!(output.exists());
    super::invert::execute(medi(mags.clone(), true)).unwrap();
    let e = super::invert::execute(medi(mags[..2].to_vec(), false)).unwrap_err().to_string();
    assert!(e.contains("magnitude of every echo") && e.contains("2 magnitude volume(s) for 3 echo time(s)"), "{e}");
}

/// NDI's data weight is the combined magnitude, and it converts the field with the scan's
/// echo times and field strength.
#[test]
fn test_invert_ndi_weights_by_the_magnitude() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    let magnitude = dir.path().join("mag.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);
    testutils::write_magnitude(&magnitude);
    super::invert::execute(InvertCommand::Ndi(InvertNdiArgs {
        common: common_invert(input, mask, output.clone()),
        scan: scan(&[0.02]),
        magnitude: vec![magnitude],
        settings: SettingsOf::from_flags(&["--ndi-max-iter", "2"]),
    })).unwrap();
    assert!(output.exists());
}

/// STAR-QSM takes no magnitude; it converts the field with the scan's echo times and field
/// strength, and a scan whose echo spacing is not positive is refused.
#[test]
fn test_invert_star() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("chi.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);
    let star = |te: &[&str], echoes: &[f64]| InvertCommand::Star(InvertStarArgs {
        common: common_invert(input.clone(), mask.clone(), output.clone()),
        scan: scan(echoes),
        settings: SettingsOf::from_flags(&[&["--star-pad", "2", "2", "2"], te].concat()),
    });
    super::invert::execute(star(&[], &[0.01, 0.02])).unwrap();
    assert!(output.exists());
    let err = super::invert::execute(star(&["--star-te", "spacing"], &[0.02, 0.01])).unwrap_err();
    assert!(err.to_string().contains("STAR-QSM") && err.to_string().contains("echo spacing"), "{err}");
}
