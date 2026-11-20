use super::*;

// --- Background Removal ---

#[test]
fn test_bgremove_vsharp() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("local.nii");
    let output_mask = dir.path().join("bgmask.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    let mut c = common_bgremove(input.clone(), mask.clone(), output.clone());
    c.output_mask = Some(output_mask.clone());
    super::bgremove::execute(BgremoveCommand::Vsharp(BgremoveVsharpArgs {
        common: c,
        ball: None,
        settings: Default::default(),
    })).unwrap();
    assert!(output.exists());
    assert!(output_mask.exists());

    // The defaults are qsm-core's V-SHARP, and `--deconvolve true` reaches it.
    let field = qsm_core::io::read_nifti_file(&input).unwrap();
    let (m, _) = super::common::load_mask(&mask).unwrap();
    let (nx, ny, nz) = field.dims;
    let (vx, vy, vz) = field.voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    let core = |p: &qsm_core::bgremove::VsharpParams| {
        qsm_core::bgremove::vsharp(&field.data, &m, &grid, p, |_, _| {}).unwrap().0
    };
    let read = |p: &std::path::Path| qsm_core::io::read_nifti_file(p).unwrap().data;
    let off = read(&output);
    let mut c = common_bgremove(input.clone(), mask.clone(), output.clone());
    c.output_mask = None;
    super::bgremove::execute(BgremoveCommand::Vsharp(BgremoveVsharpArgs {
        common: c,
        ball: None,
        settings: SettingsOf::from_flags(&["--vsharp-deconvolve", "true"]),
    })).unwrap();
    let on = read(&output);
    // `--ball qsmjl` reaches it too.
    let mut c = common_bgremove(input.clone(), mask.clone(), output.clone());
    c.output_mask = None;
    super::bgremove::execute(BgremoveCommand::Vsharp(BgremoveVsharpArgs {
        common: c,
        ball: Some(crate::cli::VsharpBallArg::Qsmjl),
        settings: Default::default(),
    })).unwrap();
    let qsmjl_ball = read(&output);
    let as_f32 = |v: Vec<f64>| v.into_iter().map(|x| x as f32).collect::<Vec<_>>();
    assert_eq!(as_f32(on), as_f32(core(&qsm_core::bgremove::VsharpParams {
        deconvolve: true, ..Default::default()
    })));
    assert_eq!(as_f32(off), as_f32(core(&qsm_core::bgremove::VsharpParams::default())));
    assert_eq!(as_f32(qsmjl_ball), as_f32(core(&qsm_core::bgremove::VsharpParams {
        ball: qsm_core::bgremove::VsharpBall::QsmJl, ..Default::default()
    })));
}

// --- Bgremove remaining algorithms ---

#[test]
fn test_bgremove_lbv() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("local.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    super::bgremove::execute(BgremoveCommand::Lbv(BgremoveLbvArgs {
        common: common_bgremove(input, mask, output.clone()),
        scan: crate::cli::ScanParamArgs { field_strength: 3.0, echo_times: vec![0.004, 0.008] },
        settings: Default::default(),
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_bgremove_ismv() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("local.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    super::bgremove::execute(BgremoveCommand::Ismv(BgremoveIsmvArgs {
        common: common_bgremove(input, mask, output.clone()),
        settings: SettingsOf::from_flags(&["--ismv-max-iter", "5"]),
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_bgremove_sharp() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("field.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("local.nii");
    testutils::write_field(&input);
    testutils::write_mask(&mask);

    super::bgremove::execute(BgremoveCommand::Sharp(BgremoveSharpArgs {
        common: common_bgremove(input, mask, output.clone()),
        variant: Some(crate::cli::SharpVariantArg::Qsmjl),
        settings: Default::default(),
    })).unwrap();
    assert!(output.exists());
}
