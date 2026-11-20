use super::*;

// --- Mask ---

#[test]
fn test_mask_otsu() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);

    super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs {
        common: common_mask(input, output.clone()),
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_mask_value_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);

    let mut c = common_mask(input, output.clone());
    c.ops = vec!["erode:1".to_string()];
    super::mask::execute(MaskCommand::Value(MaskValueArgs {
        common: c,
        threshold: 500.0,
    })).unwrap();
    assert!(output.exists());
}

/// A 4D multi-echo magnitude is combined over its echoes by root-sum-of-squares before a
/// generator runs (as `qsmxt run` does for a `magnitude` section), not read as echo 1 alone.
#[test]
fn test_mask_combines_4d_magnitude_by_rss() {
    let dir = tempfile::tempdir().unwrap();
    let (e1, e2) = (dir.path().join("e1.nii"), dir.path().join("e2.nii"));
    testutils::write_magnitude(&e1);
    testutils::write_magnitude_decayed(&e2, 0.5);
    let a = qsm_core::io::read_nifti_file(&e1).unwrap();
    let b = qsm_core::io::read_nifti_file(&e2).unwrap();
    let four_d = dir.path().join("mag4d.nii");
    qsmxt_io::nifti::write::write_volumes_4d(&four_d, &[&a.data, &b.data], a.dims, a.voxel_size, &a.affine).unwrap();

    // Echo 1 alone and the RSS cross a threshold at different voxels.
    let rss: Vec<f64> = a.data.iter().zip(&b.data).map(|(x, y)| (x * x + y * y).sqrt()).collect();
    let threshold = {
        // Halfway between two neighbouring values, so rounding cannot move a voxel across it.
        let mut v: Vec<f64> = rss.iter().copied().filter(|&x| x > 0.0).collect();
        v.sort_by(f64::total_cmp);
        v.dedup();
        (v[v.len() / 2] + v[v.len() / 2 + 1]) / 2.0
    };
    let output = dir.path().join("mask.nii");
    super::mask::execute(MaskCommand::Value(MaskValueArgs {
        common: common_mask(four_d.clone(), output.clone()), threshold,
    })).unwrap();
    let got = qsm_core::io::read_nifti_file(&output).unwrap().data;
    let want: Vec<f64> = rss.iter().map(|&x| f64::from(u8::from(x > threshold))).collect();
    let echo1: Vec<f64> = a.data.iter().map(|&x| f64::from(u8::from(x > threshold))).collect();
    assert_ne!(want, echo1, "the test needs echo 1 and the RSS to give different masks");
    assert_eq!(got, want);

    // A mask-to-mask subcommand refuses a 4D input.
    let err = super::mask::execute(MaskCommand::Erode(crate::cli::MaskErodeArgs {
        common: common_mask(four_d, dir.path().join("eroded.nii")), iterations: 1,
    })).unwrap_err();
    assert!(format!("{err}").contains("2 volumes"), "{err}");
}

#[test]
fn test_mask_bet() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);

    super::mask::execute(MaskCommand::Bet(MaskBetArgs {
        common: common_mask(input, output.clone()),
        fractional_intensity: 0.5,
        voxel_scale: None,
        orientation: None,
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_mask_signal_erode_op() {
    // signal-erode refines a threshold mask using the (magnitude) input image.
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);
    let mut c = common_mask(input, output.clone());
    c.ops = vec!["signal-erode:0.8:2:0:4:1".to_string()];
    super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs { common: c })).unwrap();
    assert!(output.exists());
}

fn read_mask(path: &Path) -> Vec<u8> {
    super::common::load_mask(path).expect("read mask").0
}

/// `mask and` / `mask or` are set intersection and union, voxel for voxel.
#[test]
fn test_mask_and_or() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.nii"), dir.path().join("b.nii"));
    // Overlapping halves, so neither is a subset of the other.
    let n = testutils::N_VOXELS;
    testutils::write_mask_where(&a, |i| i < n * 2 / 3);
    testutils::write_mask_where(&b, |i| i >= n / 3);

    let combine = |mode: fn(crate::cli::MaskCombineCliArgs) -> MaskCommand, out: &Path, ops: Vec<String>| {
        let cmd = crate::cli::MaskCombineCliArgs {
            inputs: vec![a.clone(), b.clone()], output: out.to_path_buf(), ops, magnitude: None,
        };
        super::mask::execute(mode(cmd)).unwrap();
        read_mask(out)
    };

    let and = combine(MaskCommand::And, &dir.path().join("and.nii"), vec![]);
    let or = combine(MaskCommand::Or, &dir.path().join("or.nii"), vec![]);
    let (ma, mb) = (read_mask(&a), read_mask(&b));
    for i in 0..n {
        assert_eq!(and[i], ma[i] & mb[i], "voxel {i}");
        assert_eq!(or[i], ma[i] | mb[i], "voxel {i}");
    }
    let count = |m: &[u8]| m.iter().map(|&v| v as usize).sum::<usize>();
    assert_eq!(count(&and) + count(&or), count(&ma) + count(&mb), "inclusion-exclusion");

    // --op runs on the combined mask, not on the inputs.
    let eroded = combine(MaskCommand::And, &dir.path().join("e.nii"), vec!["erode:1".to_string()]);
    assert!(count(&eroded) < count(&and), "erode:1 did not shrink the combined mask");
}

/// Masks on different grids cannot be combined, and say so by name.
#[test]
fn test_mask_combine_rejects_mismatched_grids() {
    let dir = tempfile::tempdir().unwrap();
    let (a, small) = (dir.path().join("a.nii"), dir.path().join("small.nii"));
    testutils::write_mask_where(&a, |_| true);
    testutils::write_mismatched_volume(&small, 1.0);

    let err = super::mask::execute(MaskCommand::And(crate::cli::MaskCombineCliArgs {
        inputs: vec![a, small], output: dir.path().join("out.nii"), ops: vec![], magnitude: None,
    })).unwrap_err();
    assert!(format!("{err}").contains("same grid"), "{err}");
}

/// signal-erode after a combine needs a magnitude, and `--magnitude` is how it gets one.
#[test]
fn test_mask_combine_signal_erode_needs_magnitude() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.nii"), dir.path().join("b.nii"));
    testutils::write_mask_where(&a, |_| true);
    testutils::write_mask_where(&b, |_| true);
    let args = |magnitude| crate::cli::MaskCombineCliArgs {
        inputs: vec![a.clone(), b.clone()], output: dir.path().join("out.nii"),
        ops: vec!["signal-erode:0.8:2:0:4:1".to_string()], magnitude,
    };

    let err = super::mask::execute(MaskCommand::And(args(None))).unwrap_err();
    assert!(format!("{err}").contains("--magnitude"), "{err}");

    let mag = dir.path().join("mag.nii");
    testutils::write_magnitude(&mag);
    super::mask::execute(MaskCommand::And(args(Some(mag)))).unwrap();
    assert!(dir.path().join("out.nii").exists());
}

#[test]
fn test_mask_generator_op_is_rejected() {
    // A generator passed as --op used to be silently ignored.
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    testutils::write_magnitude(&input);
    for op in ["bet:0.5", "hd-bet", "threshold:otsu"] {
        let mut c = common_mask(input.clone(), dir.path().join("mask.nii"));
        c.ops = vec![op.to_string()];
        let err = super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs { common: c })).unwrap_err();
        assert!(format!("{err}").contains("creates a mask"), "{op}: {err}");
    }
}

#[cfg(not(feature = "dl"))]
#[test]
fn test_mask_hd_bet_needs_dl() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    testutils::write_magnitude(&input);
    let err = super::mask::execute(MaskCommand::HdBet(MaskHdBetArgs {
        common: common_mask(input, dir.path().join("mask.nii")),
        window: false, low_memory: false, patch: None, tta: false, tile_step: None,
    })).unwrap_err();
    assert!(format!("{err}").contains("deep-learning"), "{err}");
}

#[cfg(not(feature = "dl"))]
#[test]
fn test_mask_mindgrab_needs_dl() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    testutils::write_magnitude(&input);
    let err = super::mask::execute(MaskCommand::Mindgrab(crate::cli::MaskMindGrabArgs {
        common: common_mask(input, dir.path().join("mask.nii")), tta: false,
    })).unwrap_err();
    assert!(format!("{err}").contains("deep-learning"), "{err}");
}

/// Runs HD-BET for real (downloads the ~120 MB weights on first use).
#[cfg(feature = "dl")]
#[test]
#[ignore]
fn test_mask_hd_bet() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);
    let mut c = common_mask(input, output.clone());
    c.ops = vec!["signal-erode".to_string()];
    super::mask::execute(MaskCommand::HdBet(MaskHdBetArgs {
        common: c, window: false, low_memory: true, patch: None, tta: false, tile_step: None,
    })).unwrap();
    assert!(output.exists());
}

// --- Mask: percentile, robust, erode ---

#[test]
fn test_mask_percentile() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);

    super::mask::execute(MaskCommand::Percentile(MaskPercentileArgs {
        common: common_mask(input, output.clone()),
        percentile: 50.0,
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_mask_robust() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);

    super::mask::execute(MaskCommand::Robust(MaskRobustArgs {
        common: common_mask(input.clone(), output.clone()),
    })).unwrap();
    assert!(output.exists());

    // `robust` is `preset robust-threshold`, not a third copy of the recipe.
    let via_preset = dir.path().join("preset.nii");
    super::mask::execute(MaskCommand::Preset(MaskPresetArgs { bet_orientation: None,
        preset: MaskPresetArg::RobustThreshold, common: common_mask(input, via_preset.clone()), quality: None, phase: None,
    })).unwrap();
    assert_eq!(read_mask(&output), read_mask(&via_preset));
}

/// A multi-section preset from files: BET on the input AND Otsu on `--quality`, then the
/// recipe's own refinements. Without `--quality` the phase-quality section reads the input.
#[test]
fn test_mask_preset_bet_and_phase() {
    let dir = tempfile::tempdir().unwrap();
    let (mag, quality) = (dir.path().join("mag.nii"), dir.path().join("quality.nii"));
    testutils::write_magnitude(&mag);
    // A quality map that is 1 everywhere except one dark slice, so the AND has something to remove.
    testutils::write_mask_where(&quality, |i| i >= 64);

    let with_quality = dir.path().join("with.nii");
    super::mask::execute(MaskCommand::Preset(MaskPresetArgs { bet_orientation: None,
        preset: MaskPresetArg::BetAndPhase, common: common_mask(mag.clone(), with_quality.clone()),
        quality: Some(quality.clone()), phase: None,
    })).unwrap();
    let without = dir.path().join("without.nii");
    super::mask::execute(MaskCommand::Preset(MaskPresetArgs { bet_orientation: None,
        preset: MaskPresetArg::BetAndPhase, common: common_mask(mag.clone(), without.clone()), quality: None, phase: None,
    })).unwrap();
    let (w, wo) = (read_mask(&with_quality), read_mask(&without));
    assert_ne!(w, wo, "--quality was not read");
    assert!(w[..64].iter().all(|&v| v == 0), "the dark slice of the quality map is masked out");
    assert!(w.contains(&1), "the AND of BET and the quality map is not empty");

    // --quality must be on the input's grid.
    let small = dir.path().join("small.nii");
    testutils::write_mismatched_volume(&small, 1.0);
    let err = super::mask::execute(MaskCommand::Preset(MaskPresetArgs { bet_orientation: None,
        preset: MaskPresetArg::BetAndPhase, common: common_mask(mag, dir.path().join("x.nii")),
        quality: Some(small), phase: None,
    })).unwrap_err();
    assert!(format!("{err}").contains("--quality"), "{err}");
}

/// `--phase`: the voxels where the scanner wrote a constant phase leave the mask.
#[test]
fn test_mask_preset_leaves_out_scanner_masked_phase() {
    let dir = tempfile::tempdir().unwrap();
    let (mag, phase) = (dir.path().join("mag.nii"), dir.path().join("phase.nii"));
    testutils::write_magnitude(&mag);
    // One constant value everywhere but the first slice.
    testutils::write_mask_where(&phase, |i| i >= 64);
    let preset = |output: &std::path::Path, phase: Option<std::path::PathBuf>| super::mask::execute(MaskCommand::Preset(MaskPresetArgs { bet_orientation: None,
        preset: MaskPresetArg::RobustThreshold, common: common_mask(mag.clone(), output.to_path_buf()), quality: None, phase,
    }));
    let (with, without) = (dir.path().join("with.nii"), dir.path().join("without.nii"));
    preset(&with, Some(phase)).unwrap();
    preset(&without, None).unwrap();
    assert!(read_mask(&without)[64..].contains(&1));
    assert!(read_mask(&with)[64..].iter().all(|&v| v == 0));

    // --phase must be on the input's grid.
    let small = dir.path().join("small.nii");
    testutils::write_mismatched_volume(&small, 1.0);
    let err = preset(&dir.path().join("x.nii"), Some(small)).unwrap_err();
    assert!(format!("{err}").contains("--phase"), "{err}");
}

#[test]
fn test_mask_otsu_with_ops() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mag.nii");
    let output = dir.path().join("mask.nii");
    testutils::write_magnitude(&input);

    let mut c = common_mask(input, output.clone());
    c.ops = vec!["dilate:1".to_string(), "fill-holes:0".to_string(), "erode:1".to_string()];
    super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs { common: c })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_mask_erode() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mask.nii");
    let output = dir.path().join("eroded.nii");
    testutils::write_mask(&input);

    super::mask::execute(MaskCommand::Erode(MaskErodeArgs {
        common: common_mask(input.clone(), output.clone()), iterations: 1,
    })).unwrap();
    assert!(output.exists());

    // Every subcommand is the first link of a chain: --op runs after its own operation.
    let count = |p: &std::path::Path| read_mask(p).iter().map(|&v| v as usize).sum::<usize>();
    let chained = dir.path().join("chained.nii");
    let mut c = common_mask(input, chained.clone());
    c.ops = vec!["erode:1".to_string()];
    super::mask::execute(MaskCommand::Erode(MaskErodeArgs { common: c, iterations: 1 })).unwrap();
    assert!(count(&chained) < count(&output), "erode --op erode:1 erodes twice");
}

// --- Mask morphological operations ---

#[test]
fn test_mask_dilate() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mask.nii");
    let output = dir.path().join("dilated.nii");
    testutils::write_mask(&input);

    super::mask::execute(MaskCommand::Dilate(MaskDilateArgs {
        common: common_mask(input, output.clone()), iterations: 1,
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_mask_close() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mask.nii");
    let output = dir.path().join("closed.nii");
    testutils::write_mask(&input);

    super::mask::execute(MaskCommand::Close(MaskCloseArgs {
        common: common_mask(input, output.clone()), radius: 1,
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_mask_fill_holes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mask.nii");
    let output = dir.path().join("filled.nii");
    testutils::write_mask(&input);

    super::mask::execute(MaskCommand::FillHoles(MaskFillHolesArgs {
        common: common_mask(input, output.clone()), max_size: 0,
    })).unwrap();
    assert!(output.exists());
    // The bare op agrees with the subcommand default and the presets: 0 = automatic.
    assert_eq!(qsmxt_engine::parse_mask_op("fill-holes").unwrap(),
               qsmxt_engine::MaskOp::FillHoles { max_size: 0 });
}

#[test]
fn test_mask_smooth() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("mask.nii");
    let output = dir.path().join("smoothed.nii");
    testutils::write_mask(&input);

    super::mask::execute(MaskCommand::Smooth(MaskSmoothArgs {
        common: common_mask(input, output.clone()), sigma: 2.0,
    })).unwrap();
    assert!(output.exists());

    // Binarises at 0.5 like every other subcommand — it used to take anything above 0.
    let faint = dir.path().join("faint.nii");
    let n = testutils::N_VOXELS;
    qsm_core::io::save_nifti_to_file(&faint, &vec![0.3; n], (8, 8, 8), (1.0, 1.0, 1.0),
        &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]).unwrap();
    let out = dir.path().join("faint_smoothed.nii");
    super::mask::execute(MaskCommand::Smooth(MaskSmoothArgs {
        common: common_mask(faint, out.clone()), sigma: 1.0,
    })).unwrap();
    assert!(read_mask(&out).iter().all(|&v| v == 0), "0.3 everywhere is not a mask");
}
