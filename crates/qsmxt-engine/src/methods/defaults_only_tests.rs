use super::*;

/// Fragments that only a stated parameter value produces.
const VALUE_MARKERS: [&str; 16] = [
    "=", " mm", "iteration", "%", "slices", "slab", "weighting)", "scaling", "power", "σ",
    "radii", "threshold ", "tolerance", "oblique", " mode", "tuning",
];

fn paragraph(config: &PipelineConfig) -> String {
    let out = generate_methods(config);
    out.split("\n## ").next().unwrap().to_string()
}

fn assert_no_values(config: &PipelineConfig, what: &str) {
    assert_no_values_except(config, what, &[]);
}

fn assert_no_values_except(config: &PipelineConfig, what: &str, allowed: &[&str]) {
    let text = paragraph(config);
    for m in VALUE_MARKERS.iter().filter(|m| !allowed.contains(m)) {
        assert!(!text.contains(m), "{what}: `{m}` in default methods:\n{text}");
    }
}

/// Every section, switched on with its default settings, names its method and no values.
#[test]
fn default_settings_state_no_parameter_values() {
    let base = || {
        let mut c = PipelineConfig::default();
        c.pipeline.do_swi = true;
        c.pipeline.do_smwi = true;
        c.pipeline.do_t2starmap = true;
        c.pipeline.do_r2starmap = true;
        c.pipeline.do_segmentation = true;
        c.pipeline.do_analysis = true;
        c.pipeline.axial_resampling = true;
        c.pipeline.crop_to_mask = true;
        c.pipeline.fft_padding = true;
        c.masking.two_pass = true;
        c.motion_correction.enabled = true;
        c.inversion.medi.msmv = true;
        c
    };
    assert_no_values(&base(), "everything on");
    for alg in &QsmAlgorithm::ALL {
        let mut c = base();
        c.inversion.algorithm = *alg;
        c.inversion.tile_size = Some(64);
        assert_no_values_except(&c, &format!("inversion {alg}"), &[]);
    }
    for alg in [BfAlgorithm::Pdf, BfAlgorithm::Lbv, BfAlgorithm::Ismv, BfAlgorithm::Sharp,
                BfAlgorithm::Resharp, BfAlgorithm::Harperella, BfAlgorithm::Iharperella] {
        let mut c = base();
        c.bg_removal.algorithm = alg;
        assert_no_values(&c, &format!("background removal {alg:?}"));
    }
    for alg in [SeparationAlgorithm::R2starQsm, SeparationAlgorithm::Decompose,
                SeparationAlgorithm::ChiSepIlsqr, SeparationAlgorithm::ChiSepMedi,
                SeparationAlgorithm::WaveSep, SeparationAlgorithm::HcChisep] {
        let mut c = base();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = alg;
        crate::config::enforce_separation_dependencies(&mut c);
        assert_no_values(&c, &format!("separation {alg:?}"));
    }
    for b0 in [B0Estimation::LinearFit, B0Estimation::RobustFit] {
        let mut c = base();
        c.field_mapping.b0_estimation = b0;
        assert_no_values(&c, &format!("B0 {b0:?}"));
    }
    let mut c = base();
    c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
    c.field_mapping.b0_weight_type = B0WeightType::AssumedDecay;
    assert_no_values(&c, "laplacian + assumed decay");
    for (name, recipe) in mask_presets() {
        let mut c = base();
        c.masking.sections = recipe.sections;
        c.masking.combine = recipe.combine;
        c.masking.refinements = recipe.refinements;
        // A recipe's own refinement counts (the `bet` preset erodes twice) are part of the
        // recipe, not a setting with a default.
        assert_no_values_except(&c, &format!("mask preset {name}"), &["iteration"]);
    }
    let mut c = base();
    c.pipeline.do_r2primemap = true;
    c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet;
    assert_no_values(&c, "R2PRIMEnet");
    let mut c = base();
    c.multi_orientation.group_by = "acq".into();
    c.multi_orientation.magnitude_weighting = true;
    assert_no_values(&c, "COSMOS");
}

/// Changing one setting states exactly that setting.
#[test]
fn a_changed_setting_is_stated() {
    type Edit = fn(&mut PipelineConfig);
    let cases: Vec<(Edit, &str)> = vec![
        (|c| c.bg_removal.vsharp.min_radius = 3.0, "V-SHARP (Wu et al., 2012) with SMV kernel radii from 12 to 3 mm in 1 mm steps."),
        (|c| c.bg_removal.vsharp.radius_step = 2.0, "V-SHARP (Wu et al., 2012) with SMV kernel radii from 12 to 2 mm in 2 mm steps."),
        (|c| c.bg_removal.vsharp.deconvolve = true, "V-SHARP (Wu et al., 2012) with truncated-SVD deconvolution enabled."),
        (|c| { c.bg_removal.algorithm = BfAlgorithm::Ismv; c.bg_removal.ismv.radius = Some(7.5); },
            "iSMV (Wen et al., 2014) with SMV radius = 7.5 mm."),
        (|c| { c.bg_removal.algorithm = BfAlgorithm::Pdf; c.bg_removal.pdf.tol = 0.123; },
            "PDF (Liu et al., 2011) with tolerance = 0.123."),
        (|c| { c.inversion.algorithm = QsmAlgorithm::Medi; c.inversion.medi.msmv = true; c.inversion.medi.msmv_maxk = 9; },
            "(Roberts et al., 2024; at most 8 boundary-correction passes)."),
        (|c| c.inversion.ilsqr.max_iter = 123, "iLSQR (Li et al., 2015) with at most 123 iterations."),
        (|c| { c.inversion.algorithm = QsmAlgorithm::Rts; c.inversion.rts.delta = 0.37; },
            "(Kames et al., 2018) with δ = 0.37."),
        (|c| { c.inversion.algorithm = QsmAlgorithm::Medi; c.inversion.medi.merit = !MediConfig::default().merit; },
            "MERIT"),
        (|c| { c.inversion.algorithm = QsmAlgorithm::Xqsm; c.inversion.tile_size = Some(64); c.inversion.tile_halo = Some(4); },
            "(tile size 64 voxels, halo 4 voxels)"),
        (|c| { c.inversion.algorithm = QsmAlgorithm::Qsmart; c.inversion.qsmart.frangi_c = 123.0; },
            "tissue/vasculature separation, with frangi c = 123."),
        (|c| { c.inversion.algorithm = QsmAlgorithm::Qsmart; c.inversion.qsmart.frangi_c_mode = crate::enums::QsmartFrangiCMode::Relative; },
            "tissue/vasculature separation, with Frangi's c set to 0.25 × the median of the vessel magnitude inside the brain mask."),
        (|c| c.field_mapping.romeo.correct_global = true, "ROMEO (Dymerska et al., 2021) with correct global enabled."),
        (|c| c.field_mapping.phase_offset_sigma_mm = [3.0, 3.0, 3.0], "(Eckstein et al., 2018; smoothing σ = 3 × 3 × 3 mm)"),
        (|c| { c.field_mapping.b0_estimation = B0Estimation::LinearFit; c.field_mapping.linear_fit.estimate_offset = !LinearFitConfig::default().estimate_offset; },
            "echo time with phase-offset estimation"),
        (|c| { c.pipeline.axial_resampling = true; c.pipeline.obliquity_threshold = 5.0; },
            "Acquisitions more than 5° oblique were resampled"),
        (|c| { c.pipeline.axial_resampling = true; c.pipeline.output_space = crate::config::OutputSpace::Working; },
            "and the derivatives were written on the resampled grid."),
        (|c| { c.pipeline.crop_to_mask = true; c.pipeline.crop_margin_mm = 16.0; },
            "within a box around the brain mask with a 16 mm margin."),
        (|c| { c.motion_correction.enabled = true; c.registration.levels = 2; },
            "over a multi-resolution pyramid (2 pyramid levels)"),
        (|c| { c.homogeneity.sigma_mm = 5.0; }, "(Eckstein et al., 2019; σ = 5 mm)"),
        (|c| { c.masking.inhomogeneity_method = InhomogeneityMethod::N4; c.n4.max_iterations = vec![20, 10]; c.n4.shrink_factor = 2; },
            "N4 inhomogeneity correction (Tustison et al., 2010; at most 20 and 10 iterations at 2 fitting levels and shrink factor = 2) was applied"),
        (|c| c.masking.sections[0].refinements[0] = MaskOp::Dilate { iterations: 2 }, "followed by dilation (2 iterations), hole-filling, and erosion."),
        (|c| { c.pipeline.do_swi = true; c.swi.hp_sigma = [2.0, 2.0, 0.0]; },
            "CLEAR-SWI (Eckstein et al., 2021) using a high-pass filter σ of 2 × 2 × 0 voxels."),
        (|c| { c.pipeline.do_swi = true; c.swi.strength = 2.0; },
            "CLEAR-SWI (Eckstein et al., 2021) using phase-mask strength 2."),
        (|c| { c.pipeline.do_swi = true; c.swi.scaling = SwiScaling::Positive; },
            "CLEAR-SWI (Eckstein et al., 2021) using positive phase-mask scaling."),
        (|c| { c.pipeline.do_swi = true; c.swi.mip_window = 5; }, "taken over 5-slice slabs."),
        (|c| { c.pipeline.do_smwi = true; c.smwi.power = 2.5; },
            "derived from the susceptibility map (mask raised to the power 2.5), separately"),
        (|c| { c.pipeline.do_smwi = true; c.smwi.mip_window = 5; }, "minimum-intensity projection over 5 slices."),
        (|c| { c.pipeline.do_segmentation = true; c.segmentation.flip_averaging = !SegmentationConfig::default().flip_averaging; },
            "structures, with left-right flip averaging of the posteriors"),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepNet; c.separation.chi_sepnet.frame_patch = Some([128, 128, 64]); },
            "the networks applied as a sliding window of 128×128×64-voxel patches."),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::SusepNet; c.separation.susep_net.patch = Some([128, 128, 64]); },
            ", with the network applied as a sliding window of 128×128×64-voxel patches."),
        (|c| { c.pipeline.do_segmentation = true; c.segmentation.patch = Some(100); },
            "structures, with the network applied as a sliding window of 112×112×112-voxel patches overlapping by at least half a patch, their posteriors Gaussian-weighted."),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepMedi; c.separation.chi_sep_medi.lambda_csf = 10.0; },
            "λ (CSF) = 10"),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::Decompose; c.separation.decompose.lsqcurvefit.max_iterations = 50; c.separation.decompose.unwrap_phase = true; },
            "with at most 50 iterations."),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::Decompose; c.separation.decompose.unwrap_phase = true; },
            "The phase was unwrapped along the echoes."),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::Decompose; c.separation.decompose.chi_bound = ChiBoundChoice::Ppm(0.3); },
            "with χ bound = 0.3 ppm."),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::Decompose; c.separation.decompose.chi_bound = ChiBoundChoice::EchoSpacing; },
            "The χ search was bounded from the field strength and echo spacing."),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.r2star_qsm.r_const_3t = 123.0; },
            "with relaxometric constant at 3 T = 123."),
        (|c| { c.pipeline.do_r2primemap = true; c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet; c.separation.r2primenet.patch = [160, 160, 128]; },
            "(Kim et al., 2025; patch = 160 × 160 × 128 voxels)"),
        (|c| { c.multi_orientation.group_by = "acq".into(); c.multi_orientation.magnitude_weighting = true; c.multi_orientation.cosmos_max_iter = 77; },
            "solved by conjugate gradient (at most 77 iterations)."),
        (|c| { c.multi_orientation.group_by = "acq".into(); c.registration.levels = 2; },
            "multi-resolution pyramid; 2 pyramid levels)"),
    ];
    for (edit, expected) in cases {
        let mut c = PipelineConfig::default();
        edit(&mut c);
        crate::config::enforce_separation_dependencies(&mut c);
        let text = paragraph(&c);
        assert!(text.contains(expected), "expected `{expected}` in:\n{text}");
    }
}

#[test]
fn floats_compare_with_a_tolerance() {
    assert!(!differs(0.1 + 0.2, 0.3));
    assert!(differs(0.3, 0.31));
    let mut c = PipelineConfig::default();
    c.bg_removal.vsharp.max_radius += 1e-12;
    assert!(!paragraph(&c).contains("radii"));
}
