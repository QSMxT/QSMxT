//! Methods prose for field mapping: phase offsets, unwrapping and the B0 estimate.

use super::*;

/// PRELUDE's settings that differ from their defaults, as prose ("6 phase partitions").
pub(super) fn prelude_changes(fm: &FieldMappingConfig, d: &FieldMappingConfig) -> Changed {
    let (p, pd) = (&fm.prelude, &d.prelude);
    let mut changed = Changed::default();
    changed.add(p.num_phase_partitions != pd.num_phase_partitions,
        || format!("{} phase partition{}", p.num_phase_partitions, if p.num_phase_partitions == 1 { "" } else { "s" }));
    changed.add(p.labelling != pd.labelling, || match p.labelling {
        PreludeLabelling::Auto => "regions labelled slice by slice above 64 voxels in-plane".to_string(),
        PreludeLabelling::Slices => "regions labelled slice by slice".to_string(),
        PreludeLabelling::Volume => "regions labelled over the whole volume".to_string(),
    });
    changed
}

/// What phase was converted to ppm with: the scanner's centre frequency, which is QSMxT's rule,
/// unless runs recorded falling back to the nominal field strength ([`Topic::CentreFrequency`]):
/// all of them, or some ("where the data gave it").
pub(super) fn describe_centre_frequency(ran: Ran, doc: &mut Draft) {
    let fell_back = ran.count(Topic::CentreFrequency, Choice::Unavailable);
    doc.say(&[Stage::FieldMap], if fell_back == 0 {
        "Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency)."
    } else if fell_back >= ran.runs() {
        "Phase was converted to ppm with the nominal field strength, as the data gave no centre \
         frequency (ImagingFrequency)."
    } else {
        "Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency) where the \
         data gave it, and with the nominal field strength otherwise."
    }.to_string());
}

/// Describe the field-mapping stage: phase offset / bipolar correction, phase
/// unwrapping, and B0 estimation. Shared by the standard pipeline and QSMART
/// (which consumes a pre-computed total field, so these steps still run).
/// Whether the text describes a per-echo QSM (`qsm.per_echo`, where nothing rules it out): said
/// "for multi-echo data", as a single echo's QSM is the only one there is.
pub(super) fn says_per_echo(config: &PipelineConfig) -> bool {
    config.pipeline.do_qsm && config.qsm.per_echo && crate::bridge::per_echo_conflict(config).is_none()
}

pub(super) fn describe_field_mapping(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    describe_centre_frequency(ran, doc);
    let fm = &config.field_mapping;
    let d = FieldMappingConfig::default();
    describe_motion_correction(config, ran, doc);
    let sigma = if fm.phase_offset_sigma_mm.iter().zip(d.phase_offset_sigma_mm).any(|(a, b)| differs(*a, b)) {
        format!("; smoothing σ = {} mm",
            fm.phase_offset_sigma_mm.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" × "))
    } else {
        String::new()
    };
    // A per-echo QSM: each echo unwrapped on its own; nothing combines or compares the echoes.
    let per_echo = says_per_echo(config);
    if per_echo {
        doc.say(&[Stage::FieldMap, Stage::BackgroundRemoval(Pass::Main), Stage::Inversion(Pass::Main), Stage::Reference],
            "For multi-echo data, QSM was reconstructed separately for each echo: each echo's unwrapped \
             phase was divided by its echo time to give that echo's field map, and background field \
             removal, dipole inversion and referencing were applied to each echo's field map on its own.".to_string());
    }
    // MEDI's complex fit fits its own phase offset; with bipolar correction it is MEDI's bipolar fit.
    if fm.b0_estimation == B0Estimation::ComplexFit && !per_echo {
        describe_complex_fit(config, doc);
        return;
    }
    match (fm.phase_offset_removal && !per_echo, fm.bipolar_correction && fm.phase_offset_removal && !per_echo) {
        (true, true) => {
            doc.cite_method(MethodStage::PhaseCorrection, "mcpc-3d-s");
            doc.cite_method(MethodStage::PhaseCorrection, "bipolar-correction");
            doc.say(&[Stage::FieldMap], format!("Phase offset removal (Eckstein et al., 2018{sigma}) and bipolar gradient correction (Eckstein, 2021) were applied."));
        }
        (true, false) => {
            doc.cite_method(MethodStage::PhaseCorrection, "mcpc-3d-s");
            doc.say(&[Stage::FieldMap], format!("Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018{sigma})."));
        }
        (false, true) => {
            doc.cite_method(MethodStage::PhaseCorrection, "bipolar-correction");
            doc.say(&[Stage::FieldMap], "Bipolar gradient correction (Eckstein, 2021) was applied.".to_string());
        }
        (false, false) => {}
    }
    // Where the phase offset could not be estimated there is no field ([`Topic::FieldMask`]); said
    // when a run left any voxel out.
    if ran.count(Topic::FieldMask, Choice::Partial) > 0 {
        doc.say(&[Stage::FieldMap],
            "Voxels where the phase offset could not be estimated were excluded from the field map and all later steps.".to_string());
    }

    // Unwrapping
    describe_unwrapping(config, doc);

    // A per-echo QSM combines no echoes into a B0 field map.
    if per_echo {
        return;
    }
    describe_b0_estimation(config, doc);
}

/// Motion correction before field mapping.
fn describe_motion_correction(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let m = &config.motion_correction;
    // Decided per run ([`Topic::MotionCorrection`]), worded as MEDI+0 is: applied to every run,
    // plainly; to none, that it was not; where the runs differ, or nothing is known of them, with
    // the condition.
    if m.enabled && ran.only(Topic::MotionCorrection, &[Choice::NotApplied]) {
        doc.say(&[Stage::Motion],
            "Motion correction was not applied: it needs at least two echoes with magnitude images, and no run had them.");
    } else if m.enabled {
        let scope = if ran.only(Topic::MotionCorrection, &[Choice::Applied]) { "" }
            else { ", in runs with more than one echo and magnitude images" };
        // The reference echo says what was done (which echo the others were moved onto), so it
        // is always named; the registration settings only when changed.
        let reference = match m.reference {
            crate::motion::MotionReference::First => "the first echo".to_string(),
            crate::motion::MotionReference::Last => "the last echo".to_string(),
            crate::motion::MotionReference::Echo(n) => format!("echo {n}"),
        };
        let mut changed = registration_changes(config);
        changed.add_fields(m, &crate::motion::MotionCorrectionConfig::default(),
            &["enabled", "reference", "min_ncc"]);
        doc.say(&[Stage::Motion], format!(
            "Before field mapping{scope}, every echo was rigidly co-registered to {reference} by maximising the normalised cross-correlation of the magnitude images over a multi-resolution pyramid{}, and the phase was resampled with it through the complex signal; the brain mask was restricted to voxels covered by every echo.",
            changed.paren()));
    }
}

/// Phase unwrapping.
fn describe_unwrapping(config: &PipelineConfig, doc: &mut Draft) {
    let fm = &config.field_mapping;
    let d = FieldMappingConfig::default();
    match fm.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => {
            let mode = if fm.romeo.individual == d.romeo.individual {
                ""
            } else if fm.romeo.individual {
                " in individual per-echo mode"
            } else {
                " in template-based temporal mode"
            };
            let mut changed = Changed::default();
            changed.add_fields(&fm.romeo, &d.romeo, &["individual", "temporal_uncertain_unwrapping"]);
            // Template mode re-unwraps the echoes' uncertain voxels spatially; individual mode
            // has no temporal step to be uncertain about.
            let uncertain = match fm.romeo.temporal_uncertain_unwrapping {
                t if t > 0.0 && !fm.romeo.individual => format!(
                    "; for multi-echo data, voxels whose temporal unwrapping was uncertain (quality below {t}) were re-unwrapped spatially"),
                _ => String::new(),
            };
            doc.cite_method(MethodStage::Unwrapping, "romeo");
            doc.say(&[Stage::FieldMap], format!("Phase unwrapping was performed using ROMEO (Dymerska et al., 2021){mode}{}{uncertain}.", changed.with()));
        }
        UnwrappingAlgorithm::Laplacian => {
            let solver = match fm.laplacian_solver {
                _ if fm.laplacian_solver == d.laplacian_solver => String::new(),
                LaplacianSolver::Dct => " solved as unweighted least squares with a Neumann-boundary DCT (Ghiglia & Romero, 1994)".to_string(),
                LaplacianSolver::Fft => " with an FFT Poisson solve".to_string(),
                LaplacianSolver::Mirror => " with an FFT Poisson solve on the volume mirrored along each axis".to_string(),
                LaplacianSolver::Mgpcg => {
                    doc.cite_method(MethodStage::BackgroundRemoval, "lbv");
                    ", solved inside the brain mask with the phase held at zero outside it by multigrid-preconditioned \
                     conjugate gradients as in QSM.jl, which removes the background field in the same step, \
                     equivalent to LBV background removal (Zhou et al., 2014)".to_string()
                }
            };
            // The phase is set to zero outside the brain mask first unless it is unwrapped raw.
            let whole = if fm.laplacian_unmasked { ", on the raw phase over the whole volume," } else { "" };
            // The padding only exists for the FFT solve.
            let pad = if fm.laplacian_solver == LaplacianSolver::Fft && fm.laplacian_fft_pad != d.laplacian_fft_pad {
                format!(" on the volume zero-padded by {} voxels", describe_pad(fm.laplacian_fft_pad))
            } else {
                String::new()
            };
            doc.cite_method(MethodStage::Unwrapping, "laplacian");
            doc.say(&[Stage::FieldMap], format!(
                "Phase unwrapping was performed separately for each echo{whole} using the Laplacian method (Schofield & Zhu, 2003){solver}{pad}."));
        }
        UnwrappingAlgorithm::Prelude => {
            doc.cite_method(MethodStage::Unwrapping, "prelude");
            doc.say(&[Stage::FieldMap], format!(
                "Phase unwrapping was performed separately for each echo using PRELUDE (Jenkinson, 2003){}.",
                prelude_changes(fm, &d).with()));
        }
        UnwrappingAlgorithm::MediRegionGrowing => {
            doc.cite_method(MethodStage::Unwrapping, "medi-region-growing");
            doc.say(&[Stage::FieldMap],
                "Phase unwrapping was performed separately for each echo using MEDI's magnitude-guided region growing (Liu et al., 2013).".to_string());
        }
    }
}

/// How the unwrapped echoes were combined into the B0 field map.
fn describe_b0_estimation(config: &PipelineConfig, doc: &mut Draft) {
    let fm = &config.field_mapping;
    let d = FieldMappingConfig::default();
    // B0 estimation. The linear fit's settings are shared with the robust fit.
    let mut linear = Changed::default();
    linear.add_fields(&fm.linear_fit, &d.linear_fit, &[]);
    match fm.b0_estimation {
        B0Estimation::WeightedAvg => {
            if fm.b0_weight_type == B0WeightType::AssumedDecay {
                let t2star = if differs(fm.b0_weight_assumed_t2star_ms, d.b0_weight_assumed_t2star_ms) {
                    format!(" of {} ms", fm.b0_weight_assumed_t2star_ms)
                } else {
                    String::new()
                };
                doc.cite(&paper("wang2022"));
                doc.say(&[Stage::FieldMap], format!(
                    "The B0 field map was estimated as a weighted average of the unwrapped echo phases with per-echo weights TE·exp(−TE/T2*), computed from the echo times and an assumed T2*{t2star} and identical in every voxel (Wang et al., 2022)."));
            } else {
                let mut changed = Changed::default();
                changed.add(fm.b0_weight_type != d.b0_weight_type, || format!("{} weighting", fm.b0_weight_type));
                doc.say(&[Stage::FieldMap], format!("The B0 field map was estimated as a weighted average of the unwrapped echo phases{}.", changed.with()));
            }
        }
        B0Estimation::LinearFit => {
            doc.say(&[Stage::FieldMap], format!("The B0 field map was estimated using magnitude-weighted linear fit of phase vs echo time{}.", linear.with()));
        }
        B0Estimation::RobustFit => {
            let (r, rd) = (&fm.robust_fit, &d.robust_fit);
            let conventional = match r.loss { RobustLoss::Tukey => 4.685, RobustLoss::Huber => 1.345 };
            let mut changed = Changed::default();
            changed.add(r.loss != rd.loss, || format!("a {} loss", if r.loss == RobustLoss::Tukey { "Tukey biweight" } else { "Huber" }));
            changed.add(r.tuning.is_some_and(|t| differs(t, conventional)), || format!("tuning constant {}", r.tuning.unwrap_or(conventional)));
            changed.0.extend(linear.0);
            doc.say(&[Stage::FieldMap], if r.iterations == 0 {
                format!("The B0 field map was estimated using magnitude-weighted linear fit of phase vs echo time, with per-echo residual statistics reported to flag echoes inconsistent with the fit{}.",
                    if changed.is_empty() { String::new() } else { format!("; {}", join_list(&changed.0)) })
            } else {
                changed.add(r.iterations != rd.iterations, || format!("{} iterations", r.iterations));
                format!("The B0 field map was estimated with a motion-robust magnitude-weighted linear fit of phase vs echo time by iteratively reweighted least squares{}.", changed.with())
            });
        }
        B0Estimation::QsmartEchofit => {
            doc.cite_method(MethodStage::EchoCombination, "qsmart-echofit");
            let threshold = match fm.qsmart_fit_threshold() {
                qsm_core::utils::QsmartFitThreshold::Percentile(p) => format!("its {p}th percentile"),
                qsm_core::utils::QsmartFitThreshold::Absolute(t) => format!("{t}"),
            };
            // The voxels the fit marks unreliable are left out of QSMART only.
            let excluded = if config.inversion.algorithm == QsmAlgorithm::Qsmart {
                format!(", and voxels whose blurred fit residual exceeded {threshold} were excluded from the QSMART reconstruction")
            } else {
                String::new()
            };
            doc.say(&[Stage::FieldMap], format!(
                "The B0 field map was estimated by a magnitude-weighted fit of phase vs echo time through the origin, as in QSMART (Yaghmaie et al., 2021){excluded}."));
        }
        // Worded by describe_complex_fit, which returned above.
        B0Estimation::ComplexFit => {}
    }
}

/// MEDI's field map (QSM.rs `B0EstimationMethod::ComplexFit`): `Fit_ppm_complex` on the complex
/// echoes, then the fitted phase per echo spacing unwrapped and divided by the echo spacing. No
/// phase offsets are removed first (the fit has its own).
pub(super) fn describe_complex_fit(config: &PipelineConfig, doc: &mut Draft) {
    let (fm, d) = (&config.field_mapping, FieldMappingConfig::default());
    let mut fit = Changed::default();
    fit.add_fields(&fm.complex_fit, &d.complex_fit, &[]);
    doc.cite_method(MethodStage::EchoCombination, "complex-fit");
    let unwrapping = match fm.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => {
            let mut changed = Changed::default();
            // One volume is unwrapped: the echo settings do not apply.
            changed.add_fields(&fm.romeo, &d.romeo, &["individual", "template", "temporal_uncertain_unwrapping"]);
            doc.cite_method(MethodStage::Unwrapping, "romeo");
            format!("ROMEO (Dymerska et al., 2021){}", changed.with())
        }
        UnwrappingAlgorithm::Laplacian => {
            let solver = if fm.laplacian_solver == d.laplacian_solver {
                String::new()
            } else {
                format!(" with the {} solver", fm.laplacian_solver)
            };
            doc.cite_method(MethodStage::Unwrapping, "laplacian");
            format!("the Laplacian method (Schofield & Zhu, 2003){solver}")
        }
        UnwrappingAlgorithm::Prelude => {
            doc.cite_method(MethodStage::Unwrapping, "prelude");
            format!("PRELUDE (Jenkinson, 2003){}", prelude_changes(fm, &d).with())
        }
        UnwrappingAlgorithm::MediRegionGrowing => {
            "MEDI's magnitude-guided region growing (unwrapPhase, MEDI toolbox)".to_string()
        }
    };
    let (fit_name, bipolar) = if fm.bipolar_correction {
        ("Fit_ppm_complex_bipolar", ", with a separate phase for the even echoes to account for the bipolar readout")
    } else {
        ("Fit_ppm_complex", "")
    };
    doc.say(&[Stage::FieldMap], format!(
        "The B0 field map was estimated by a magnitude-weighted nonlinear fit of the complex echo signal{bipolar} ({fit_name}, MEDI toolbox; Liu et al., 2013{}); the fitted phase per echo spacing was unwrapped using {unwrapping} and divided by the echo spacing.",
        fit.semi()));
}
