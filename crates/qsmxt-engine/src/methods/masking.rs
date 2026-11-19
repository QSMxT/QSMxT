//! Methods prose for masking: the mask's input, generator and refinements.

use super::*;

/// Prose for a list of mask refinement ops, in the order they run.
/// An op's settings are compared with what the op is when written without arguments
/// (`erode` is one iteration, `gaussian` is 4 mm), so only the ones the user chose are stated.
pub(super) fn describe_refinements(ops: &[MaskOp], doc: &mut Draft) -> Vec<String> {
    let default_of = |spec: &str| parse_mask_op(spec).expect("bare mask op parses");
    ops.iter().map(|op| match op {
        MaskOp::Erode { iterations } | MaskOp::Dilate { iterations } => {
            let (name, spec) = if matches!(op, MaskOp::Erode { .. }) { ("erosion", "erode") } else { ("dilation", "dilate") };
            if *op == default_of(spec) {
                name.to_string()
            } else {
                format!("{name} ({iterations} iteration{})", if *iterations != 1 { "s" } else { "" })
            }
        }
        MaskOp::Close { radius } if *op != default_of("close") => format!("morphological closing (radius {radius} voxels)"),
        MaskOp::Close { .. } => "morphological closing".to_string(),
        MaskOp::MinIntensity { value } => format!("removal of voxels with an intensity below {value}"),
        MaskOp::Majority => "a 3×3×3 majority filter".to_string(),
        MaskOp::LargestComponent => "the largest connected component kept".to_string(),
        MaskOp::Imfill => "every hole filled".to_string(),
        MaskOp::FillHoles { max_size: 0 } => "hole-filling".to_string(),
        MaskOp::FillHoles { max_size } => format!("hole-filling (holes of at most {} voxels)", max_size),
        MaskOp::GaussianSmooth { sigma_mm } if *op != default_of("gaussian") => format!("Gaussian smoothing (σ = {sigma_mm} mm)"),
        MaskOp::GaussianSmooth { .. } => "Gaussian smoothing".to_string(),
        MaskOp::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } => {
            doc.cite(&CITE_QSMCI_SIGNAL_EROSION);
            let MaskOp::SignalErode {
                threshold: dt, depth_cap: dd, global_erosions: dg, bias_sigma: db, min_component: dm,
            } = MaskOp::signal_erode_default() else { unreachable!() };
            let mut changed = Changed::default();
            changed.add(*global_erosions != dg, || format!("{} plain erosion{} first", global_erosions, if *global_erosions != 1 { "s" } else { "" }));
            changed.add(differs(*threshold, dt), || format!("threshold {threshold:.2} of the median bias-corrected magnitude"));
            changed.add(*depth_cap != dd, || if *depth_cap == 0 { "no depth limit".to_string() } else { format!("at most {depth_cap} voxels deep") });
            changed.add(differs(*bias_sigma, db), || format!("bias-field σ = {bias_sigma}"));
            changed.add(*min_component != dm, || format!("components under {min_component} voxels discarded"));
            format!("signal-gated erosion (removal of low-signal boundary voxels{}; QSM-CI)",
                if changed.is_empty() { String::new() } else { format!(", {}", join_list(&changed.0)) })
        }
        _ => format!("{}", op),
    }).collect()
}

/// Prose for a mask section's input image and generator op — "Otsu thresholding of the ROMEO
/// phase quality map", and so on. Shared by the main mask and the two-pass reliable mask, so the
/// same recipe never gets described two different ways.
pub(super) fn describe_generator(
    section: &MaskSection, config: &PipelineConfig, doc: &mut Draft,
) -> String {
    // Input description — clarify which magnitude goes into ROMEO
    let corrected = config.masking.inhomogeneity_correction;
    let input_desc = match section.input {
        MaskingInput::PhaseQuality => {
            doc.cite_method(MethodStage::Unwrapping, "romeo");
            if corrected {
                "the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude)"
            } else {
                "the ROMEO phase quality map (computed from phase data and the RSS-combined magnitude)"
            }
        }
        MaskingInput::Magnitude => {
            if corrected { "the inhomogeneity-corrected RSS-combined magnitude image" }
            else { "the RSS-combined magnitude image" }
        }
        MaskingInput::MagnitudeFirst => {
            if corrected { "the inhomogeneity-corrected first-echo magnitude image" }
            else { "the first-echo magnitude image" }
        }
        MaskingInput::MagnitudeLast => {
            if corrected { "the inhomogeneity-corrected last-echo magnitude image" }
            else { "the last-echo magnitude image" }
        }
    };

    match &section.generator {
        MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => {
            doc.cite_method(MethodStage::MaskThreshold, "otsu");
            format!("Otsu thresholding (Otsu, 1979) of {}", input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => {
            format!("fixed thresholding (value={:.4}) of {}", value.unwrap_or(0.5), input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } => {
            format!("percentile thresholding ({}th percentile) of {}", value.unwrap_or(75.0), input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::StiGui, .. } => {
            doc.cite_method(MethodStage::MaskThreshold, "otsu");
            format!("thresholding of {} at STI Suite's default level (Otsu's threshold of the middle slice; Otsu, 1979)", input_desc)
        }
        MaskOp::Bet { fractional_intensity, voxel_scale } => {
            doc.cite_method(MethodStage::Masking, "bet");
            let MaskOp::Bet { fractional_intensity: df, .. } = parse_mask_op("bet").expect("bare bet parses")
                else { unreachable!() };
            let mut changed = Changed::default();
            changed.add(differs(*fractional_intensity, df), || format!("f={fractional_intensity:.2}"));
            // The surface settings, from the shared BET config.
            let (b, d) = (&config.bet, crate::config::BetConfig::default());
            changed.add(differs(b.smoothness, d.smoothness), || format!("smoothness {}", b.smoothness));
            changed.add(differs(b.gradient_threshold, d.gradient_threshold), || format!("gradient threshold {}", b.gradient_threshold));
            changed.add(b.iterations != d.iterations, || format!("{} iterations", b.iterations));
            changed.add(b.subdivisions != d.subdivisions, || format!("icosphere subdivision level {}", b.subdivisions));
            // A scaled run did not see the acquisition's own geometry, which changes what BET's
            // millimetre-scale surface model was fitted to.
            changed.add(differs(*voxel_scale, crate::masking::bet_default_voxel_scale()),
                || format!("voxel sizes scaled by {voxel_scale}x"));
            // BET as MEDI's BET.m runs bet2: on the array, so on the voxels as they are stored.
            let orientation = match b.orientation {
                crate::enums::BetOrientation::Fsl => "",
                crate::enums::BetOrientation::AsStored => ", in its stored voxel order as MEDI's BET.m runs it",
            };
            format!("BET brain extraction (Smith, 2002{}) of {}{orientation}", changed.semi(), input_desc)
        }
        MaskOp::Rs2Net { tta, tile_step } => {
            doc.cite_method(MethodStage::Masking, "rs2-net");
            let mut changed = Changed::default();
            changed.add(differs(*tile_step, crate::masking::rs2_net_default_tile_step()),
                || format!("sliding-window patches stepped by {:.0}% of the patch", tile_step * 100.0));
            changed.add(*tta, || "mirroring test-time augmentation".to_string());
            format!("RS2-Net deep-learning rodent brain extraction (Lin et al., 2024{}) of {}", changed.semi(), input_desc)
        }
        MaskOp::MindGrab { tta } => {
            doc.cite_method(MethodStage::Masking, "mindgrab");
            let mut changed = Changed::default();
            changed.add(*tta, || "left-right flip test-time augmentation".to_string());
            format!("MindGrab deep-learning brain extraction (Fani et al., 2025{}) of {}", changed.semi(), input_desc)
        }
        MaskOp::HdBet { patch, tta, tile_step } => {
            doc.cite_method(MethodStage::Masking, "hd-bet");
            let mut changed = Changed::default();
            if let Some(p) = patch {
                changed.add(true, || format!("{}x{}x{}-voxel sliding-window patches", p[0], p[1], p[2]));
                changed.add(differs(*tile_step, crate::masking::hd_bet_default_tile_step()),
                    || format!("windows stepped by {:.0}% of the patch", tile_step * 100.0));
            }
            changed.add(*tta, || "mirroring test-time augmentation".to_string());
            format!("HD-BET deep-learning brain extraction (Isensee et al., 2019{}) of {}", changed.semi(), input_desc)
        }
        _ => format!("{} of {}", section.generator, input_desc),
    }
}

pub(super) fn describe_masking(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    if config.masking.sections.is_empty() {
        return;
    }

    // Prefer a bring-your-own mask from BIDS derivatives, where available. Every run having used
    // one, or none, says which.
    let tool = config.masking.custom_mask_tool.as_deref();
    let all_supplied = tool.is_some() && ran.only(Topic::MaskSource, &[Choice::Supplied]);
    let none_supplied = ran.only(Topic::MaskSource, &[Choice::Computed]);
    match tool {
        // One run: its mask was the one supplied with it.
        Some(tool) if all_supplied && ran.runs() == 1 => {
            let from = if tool == "*" { "its".to_string() } else { format!("the '{tool}'") };
            doc.say(&[Stage::Mask(Pass::Main)], format!("The brain mask was supplied with the scan, from {from} BIDS derivatives."));
        }
        Some(tool) if all_supplied => {
            let from = if tool == "*" { String::new() } else { format!("'{tool}' ") };
            doc.say(&[Stage::Mask(Pass::Main)], format!("Brain masks were sourced from the {from}BIDS derivatives."));
        }
        _ if none_supplied => {}
        Some("*") => doc.say(&[Stage::Mask(Pass::Main)], 
            "Where available, existing brain masks were sourced from the BIDS derivatives; \
             otherwise they were generated as follows.".to_string()),
        Some(tool) => doc.say(&[Stage::Mask(Pass::Main)], format!(
            "Where available, brain masks were sourced from the '{}' BIDS derivatives; \
             otherwise they were generated as follows.", tool)),
        None => {}
    }

    // The inhomogeneity correction, when on, and which echoes it corrected: those the mask reads
    // (the RSS for RSS and phase-quality inputs, the first or last echo for those), and every echo
    // for the combined magnitude.
    if config.masking.inhomogeneity_correction {
        describe_inhomogeneity_correction(config, doc);
    }

    // No run generated a mask: there is no recipe to describe.
    if all_supplied {
        return;
    }

    let section_count = config.masking.sections.len();
    let mut section_descs = Vec::new();

    for section in &config.masking.sections {
        let mut parts = vec![describe_generator(section, config, doc)];

        // Refinements
        let refinement_descs = describe_refinements(&section.refinements, doc);

        if !refinement_descs.is_empty() {
            parts.push(format!("followed by {}", join_list(&refinement_descs)));
        }

        section_descs.push(parts.join(", "));
    }

    if section_count == 1 {
        doc.say(&[Stage::Mask(Pass::Main)], format!("A brain mask was generated using {}.", section_descs[0]));
    } else {
        let (op, how) = match config.masking.combine {
            MaskCombine::Or => ("OR", "union"),
            MaskCombine::And => ("AND", "intersection"),
        };
        doc.say(&[Stage::Mask(Pass::Main)], format!(
            "A brain mask was generated by combining {} mask sections ({} operation, i.e. their {}): {}.",
            section_count, op, how, join_list(&section_descs)
        ));
    }

    let combined_descs = describe_refinements(&config.masking.refinements, doc);
    if !combined_descs.is_empty() {
        doc.say(&[Stage::Mask(Pass::Main)], format!(
            "The {} mask was then refined by {}.",
            if section_count == 1 { "resulting" } else { "combined" },
            join_list(&combined_descs)
        ));
    }
    // The scanner's own masking of the phase ([`Topic::ScannerMaskedPhase`]): said when a run had any.
    if ran.count(Topic::ScannerMaskedPhase, Choice::Partial) > 0 {
        doc.say(&[Stage::Mask(Pass::Main)],
            "Voxels where the scanner recorded no phase (a constant value in every echo) were left out of the mask.".to_string());
    }

    // Exactly the consensus recipe — say so, and cite it.
    let consensus = bet_and_phase_mask_recipe();
    if config.masking.sections == consensus.sections
        && config.masking.combine == consensus.combine
        && config.masking.refinements == consensus.refinements
    {
        doc.cite(&CITE_QSM_CONSENSUS);
        doc.say(&[Stage::Mask(Pass::Main)], 
            "This masking follows the recommendation of the ISMRM electro-magnetic tissue \
             properties study group consensus (Bilgic et al., 2024).".to_string());
    }
}

/// The inhomogeneity correction, and which echoes it corrected.
fn describe_inhomogeneity_correction(config: &PipelineConfig, doc: &mut Draft) {
    let mut changed = Changed::default();
    let correction = match config.masking.inhomogeneity_method {
        InhomogeneityMethod::Makehomogeneous => {
            doc.cite_method(MethodStage::InhomogeneityCorrection, "makehomogeneous");
            changed.add_fields(&config.homogeneity, &HomogeneityConfig::default(), &[]);
            "Inhomogeneity correction (Eckstein et al., 2019"
        }
        InhomogeneityMethod::N4 => {
            doc.cite_method(MethodStage::InhomogeneityCorrection, "n4");
            let (n4, d) = (&config.n4, N4Config::default());
            if n4.max_iterations != d.max_iterations {
                let levels: Vec<String> = n4.max_iterations.iter().map(|i| i.to_string()).collect();
                let n = levels.len();
                changed.0.push(format!("at most {} iterations at {n} fitting level{}",
                                       join_list(&levels), if n == 1 { "" } else { "s" }));
            }
            changed.add_fields(n4, &d, &["max_iterations"]);
            "N4 inhomogeneity correction (Tustison et al., 2010"
        }
    };
    let settings = changed.semi();

    let inputs: Vec<MaskingInput> = config.masking.sections.iter().map(|s| s.input).collect();
    let uses_rss = inputs.iter().any(|i| matches!(i, MaskingInput::Magnitude | MaskingInput::PhaseQuality));
    let uses_first = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeFirst));
    let uses_last = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeLast));

    // Each echo the mask reads is corrected on its own, and every echo when the combined
    // magnitude (the RSS of the corrected echoes) is published.
    let all = uses_rss || config.pipeline.do_magnitude
        || crate::stages::magnitude::refinements_read_magnitude(&config.masking.refinements);
    let what = if all || (uses_first && uses_last) {
        "each echo's magnitude"
    } else if uses_first {
        "the first-echo magnitude"
    } else {
        "the last-echo magnitude"
    };
    doc.say(&[Stage::InhomogeneityCorrection], format!("{correction}{settings}) was applied to {what}."));
}
