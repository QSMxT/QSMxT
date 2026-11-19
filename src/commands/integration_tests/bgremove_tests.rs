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
        deconvolve: None, threshold: None, max_radius: None, min_radius: None, radius_step: None, grid: None, grid_spacing: None,
        ball: None,
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
        deconvolve: Some(true), threshold: None, max_radius: None, min_radius: None, radius_step: None, grid: None, grid_spacing: None,
        ball: None,
    })).unwrap();
    let on = read(&output);
    // `--ball qsmjl` reaches it too.
    let mut c = common_bgremove(input.clone(), mask.clone(), output.clone());
    c.output_mask = None;
    super::bgremove::execute(BgremoveCommand::Vsharp(BgremoveVsharpArgs {
        common: c,
        deconvolve: None, threshold: None, max_radius: None, min_radius: None, radius_step: None, grid: None, grid_spacing: None,
        ball: Some(crate::cli::VsharpBallArg::Qsmjl),
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
        tol: None, depth: None, peel: None, n1: None, n2: None, n3: None, precision: None,
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
        variant: None, tol: None, max_iter: Some(5), radius: None,
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
        threshold: None, radius: None, variant: Some(crate::cli::SharpVariantArg::Qsmjl),
    })).unwrap();
    assert!(output.exists());
}
