use super::*;

/// A preclinical mask was not fitted to the acquisition's own geometry, which is exactly
/// the kind of thing a methods section exists to state.
#[test]
fn scaled_bet_says_so_in_the_methods() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Bet { fractional_intensity: 0.5, voxel_scale: 10.0 },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("voxel sizes scaled by 10x"),
            "out: {out}");

    // Unscaled BET at the default f states neither.
    config.masking.sections[0].generator = MaskOp::bet(0.5);
    let out = generate_methods(&config);
    assert!(out.contains("BET brain extraction (Smith, 2002) of"), "out: {out}");
    assert!(!out.contains("voxel sizes scaled"), "out: {out}");
    assert!(!out.contains("f="), "out: {out}");
}

/// The `bet-and-phase` recipe names the operation, the intersection it means, and the
/// consensus it comes from — with the citation in the reference list.
#[test]
fn test_consensus_masking_methods() {
    let recipe = bet_and_phase_mask_recipe();
    let mut config = PipelineConfig::default();
    config.masking.sections = recipe.sections.clone();
    config.masking.combine = recipe.combine;
    config.masking.refinements = recipe.refinements.clone();

    let out = generate_methods(&config);
    assert!(out.contains("AND operation, i.e. their intersection"), "out: {out}");
    assert!(out.contains("The combined mask was then refined by hole-filling"), "out: {out}");
    assert!(out.contains("electro-magnetic tissue properties study group consensus"), "out: {out}");
    assert!(out.contains("10.1002/mrm.30006"), "consensus citation missing: {out}");

    // A different recipe combined the same way is not the consensus one, and must not claim to be.
    config.masking.refinements.clear();
    let out = generate_methods(&config);
    assert!(out.contains("AND operation"));
    assert!(!out.contains("study group consensus"), "claimed the consensus for a modified recipe: {out}");
}

// ─── Masking description tests ───

#[test]
fn test_masking_otsu_threshold() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("Otsu thresholding"));
    assert!(out.contains("Otsu, 1979"));
}

#[test]
fn test_masking_bet() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::bet(0.35),
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("BET brain extraction"));
    assert!(out.contains("Smith, 2002"));
    assert!(out.contains("f=0.35"));
}

#[test]
fn test_masking_rs2_net() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::rs2_net_default(),
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("RS2-Net deep-learning rodent brain extraction (Lin et al., 2024) of"), "{out}");
    assert!(!out.contains("stepped by"), "the default step is not stated: {out}");
    assert!(out.contains("https://doi.org/10.1016/j.neuroimage.2024.120769"), "{out}");
    // Otsu is not cited by a run that never thresholded.
    assert!(!out.contains("Otsu, 1979"), "{out}");

    config.masking.sections[0].generator = MaskOp::Rs2Net { tta: true, tile_step: 0.5 };
    let tta = generate_methods(&config);
    assert!(tta.contains("mirroring test-time augmentation"), "{tta}");
}

#[test]
fn test_masking_mindgrab() {
    let mut config = PipelineConfig::default();
    config.masking.sections = crate::masking::mindgrab_mask_sections();
    let out = generate_methods(&config);
    assert!(out.contains("MindGrab deep-learning brain extraction (Fani et al., 2025) of"), "{out}");
    assert!(!out.contains("test-time augmentation"), "TTA is off by default: {out}");
    assert!(out.contains("https://doi.org/10.48550/arXiv.2506.11860"), "{out}");
    assert!(!out.contains("Otsu, 1979"), "{out}");

    config.masking.sections[0].generator = MaskOp::MindGrab { tta: true };
    let tta = generate_methods(&config);
    assert!(tta.contains("MindGrab deep-learning brain extraction (Fani et al., 2025; left-right flip test-time augmentation) of"), "{tta}");
}

#[test]
fn test_masking_hd_bet_signal_erode() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::hd_bet_default(),
        refinements: vec![MaskOp::signal_erode_default()],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("HD-BET deep-learning brain extraction (Isensee et al., 2019) of"), "{out}");
    assert!(out.contains("signal-gated erosion (removal of low-signal boundary voxels; QSM-CI)"), "{out}");
    assert!(!out.contains("0.80") && !out.contains("voxels deep"), "{out}");
    assert!(out.contains("https://doi.org/10.1002/hbm.24750"), "{out}");
    assert!(out.contains("https://github.com/QSMxT/QSM-CI"), "{out}");

    config.masking.sections[0].generator = MaskOp::HdBet { patch: Some(MaskOp::hd_bet_default_patch()), tta: true, tile_step: 0.25 };
    config.masking.sections[0].refinements = vec![parse_mask_op("signal-erode:0.7").unwrap()];
    let out = generate_methods(&config);
    assert!(out.contains("(Isensee et al., 2019; 192x192x96-voxel sliding-window patches, windows stepped by 25% of the patch, and mirroring test-time augmentation)"), "{out}");
    assert!(out.contains("signal-gated erosion (removal of low-signal boundary voxels, threshold 0.70 of the median bias-corrected magnitude; QSM-CI)"), "{out}");
}

#[test]
fn test_masking_hd_bet_window() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::HdBet { patch: Some([128, 128, 64]), tta: true, tile_step: 0.5 },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("(Isensee et al., 2019; 128x128x64-voxel sliding-window patches and mirroring \
                          test-time augmentation)"), "{out}");
}

#[test]
fn test_masking_fixed_threshold() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.3) },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("fixed thresholding"));
    assert!(out.contains("value=0.3"));
}

#[test]
fn test_masking_percentile_threshold() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(80.0) },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("percentile thresholding"));
    assert!(out.contains("80th percentile"));
}

#[test]
fn test_masking_phase_quality_input() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::PhaseQuality,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("ROMEO phase quality map"));
}

#[test]
fn test_masking_magnitude_first_input() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::MagnitudeFirst,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("first-echo magnitude image"));
}

#[test]
fn test_masking_magnitude_last_input() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::MagnitudeLast,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("last-echo magnitude image"));
}

#[test]
fn test_masking_with_inhomogeneity() {
    let mut config = PipelineConfig::default();
    config.masking.inhomogeneity_correction = true;
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("Inhomogeneity correction"));
    assert!(out.contains("Eckstein et al., 2019"));
    assert!(out.contains("inhomogeneity-corrected RSS-combined magnitude"));
}

#[test]
fn n4_is_named_and_cited_and_makehomogeneous_is_not() {
    let mut config = PipelineConfig::default();
    config.masking.inhomogeneity_method = InhomogeneityMethod::N4;
    let out = generate_methods(&config);
    assert!(out.contains("N4 inhomogeneity correction (Tustison et al., 2010) was applied"), "out: {out}");
    assert!(out.contains("N4ITK: Improved N3 Bias Correction"), "out: {out}");
    assert!(!out.contains("Eckstein et al., 2019") && !out.contains("A Simple Homogeneity Correction"), "out: {out}");
}

#[test]
fn test_masking_inhomogeneity_first_echo() {
    let mut config = PipelineConfig::default();
    config.masking.inhomogeneity_correction = true;
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::MagnitudeFirst,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("first-echo magnitude image"));
    assert!(out.contains("inhomogeneity-corrected"));
}

#[test]
fn test_masking_no_inhomogeneity() {
    let mut config = PipelineConfig::default();
    config.masking.inhomogeneity_correction = false;
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    let out = generate_methods(&config);
    assert!(!out.contains("Inhomogeneity correction"));
    assert!(out.contains("the RSS-combined magnitude image"));
    assert!(!out.contains("inhomogeneity-corrected"));
}

#[test]
fn test_masking_refinements() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![
            MaskOp::Dilate { iterations: 1 },
            MaskOp::FillHoles { max_size: 0 },
            MaskOp::Erode { iterations: 1 },
        ],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("followed by"));
    assert!(out.contains("dilation"));
    assert!(out.contains("hole-filling"));
    assert!(out.contains("erosion"));
}

#[test]
fn test_masking_erode_singular() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![MaskOp::Erode { iterations: 1 }],
    }];
    let out = generate_methods(&config);
    // One iteration is what a bare `erode` means, so it is not stated.
    let text = out.split("## Note").next().unwrap();
    assert!(text.contains("followed by erosion."), "{out}");
    assert!(!text.contains("iteration"), "{out}");
}

#[test]
fn test_masking_erode_plural() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![MaskOp::Erode { iterations: 3 }],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("erosion (3 iterations)"));
}

#[test]
fn test_masking_close_refinement() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![MaskOp::Close { radius: 5 }],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("morphological closing (radius 5 voxels)"), "{out}");
}

#[test]
fn test_masking_gaussian_refinement() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![MaskOp::GaussianSmooth { sigma_mm: 2.5 }],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("Gaussian smoothing (σ = 2.5 mm)"), "{out}");
}

#[test]
fn test_masking_fill_holes_nonzero() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![MaskOp::FillHoles { max_size: 500 }],
    }];
    let out = generate_methods(&config);
    assert!(out.contains("hole-filling (holes of at most 500 voxels)"), "{out}");
}

#[test]
fn test_masking_multiple_sections_or() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![
        MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        },
        MaskSection {
            input: MaskingInput::PhaseQuality,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        },
    ];
    let out = generate_methods(&config);
    assert!(out.contains("combining"));
    assert!(out.contains("2 mask sections"));
    assert!(out.contains("OR operation"));
}

#[test]
fn test_masking_empty_sections() {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![];
    let out = generate_methods(&config);
    assert!(!out.contains("A brain mask was generated"));
}

#[test]
fn test_masking_inhomogeneity_mixed_inputs() {
    let mut config = PipelineConfig::default();
    config.masking.inhomogeneity_correction = true;
    config.masking.sections = vec![
        MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        },
        MaskSection {
            input: MaskingInput::MagnitudeFirst,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        },
    ];
    let out = generate_methods(&config);
    assert!(out.contains("applied to each echo's magnitude"), "{out}");
}
