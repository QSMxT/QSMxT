use super::*;

/// Which of `n` echoes `echoes` (1-based, ascending) are: "each echo", "the first echo", "the
/// last echo", or "echoes 1 and 5".
pub(crate) fn echoes(echoes: &[usize], n: usize) -> String {
    match echoes {
        e if e.len() == n && n > 1 => "each echo".to_string(),
        [1] => "the first echo".to_string(),
        [e] if *e == n => "the last echo".to_string(),
        [e] => format!("echo {e}"),
        [rest @ .., last] => format!("echoes {} and {last}",
            rest.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")),
        [] => String::new(),
    }
}

/// The name the methods text gives an algorithm, without what follows it in parentheses:
/// "RTS (Rapid Two-Step)" is "RTS" in a box.
fn short(name: &'static str) -> String {
    name.split(" (").next().unwrap_or(name).to_string()
}

pub(crate) fn inversion(alg: QsmAlgorithm) -> String {
    short(inversion_name_cite(alg).0)
}

pub(crate) fn bg_removal(alg: BfAlgorithm) -> String {
    short(bg_removal_name_cite(alg).0)
}

pub(crate) fn separation(alg: SeparationAlgorithm) -> String {
    short(separation_name_cite(alg).0)
}

/// The selected inversion's settings that differ from their defaults (`"λ = 0.002"`).
pub(crate) fn inversion_settings(config: &PipelineConfig, alg: QsmAlgorithm) -> Vec<String> {
    inversion_changes(config, alg).0
}

/// The selected background removal's settings: V-SHARP's kernel radii always (they are what
/// says which V-SHARP ran), and otherwise what differs from the defaults.
pub(crate) fn bg_removal_settings(config: &PipelineConfig) -> Vec<String> {
    let b = &config.bg_removal;
    if b.algorithm != BfAlgorithm::Vsharp {
        return bg_removal_changes(config).0;
    }
    let (v, d) = (&b.vsharp, VsharpConfig::default());
    let mut c = Changed::default();
    c.add(true, || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
    c.add(v.deconvolve != d.deconvolve, || format!(
        "truncated-SVD deconvolution {}", if v.deconvolve { "enabled" } else { "disabled" }));
    c.add(v.deconvolve && differs(v.threshold, d.threshold), || format!("deconvolution threshold = {}", v.threshold));
    c.0
}

pub(crate) fn separation_settings(config: &PipelineConfig) -> Vec<String> {
    separation_changes(config).0
}

pub(crate) fn registration_settings(config: &PipelineConfig) -> Vec<String> {
    registration_changes(config).0
}

/// ROMEO's settings that differ from their defaults; Laplacian unwrapping's solver, when it
/// is not the default one, and the raw phase, when it is unwrapped unmasked; PRELUDE's
/// partitions and labelling, when they are not the defaults.
pub(crate) fn unwrapping_settings(config: &PipelineConfig) -> Vec<String> {
    let (fm, d) = (&config.field_mapping, FieldMappingConfig::default());
    let mut c = Changed::default();
    match fm.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => c.add_fields(&fm.romeo, &d.romeo, &[]),
        UnwrappingAlgorithm::Laplacian => {
            c.add(fm.laplacian_solver != d.laplacian_solver, || format!("{} solver", fm.laplacian_solver));
            c.add(fm.laplacian_unmasked != d.laplacian_unmasked, || "raw phase over the whole volume".to_string());
            c.add(fm.laplacian_keep_outside != d.laplacian_keep_outside, || "field kept over the whole volume".to_string());
        }
        UnwrappingAlgorithm::Prelude => c = prelude_changes(fm, &d),
        UnwrappingAlgorithm::MediRegionGrowing => {}
    }
    c.0
}

/// Each mask section as one line: its input, how the mask is made from it and how it is
/// refined ("phase quality: Otsu threshold, dilation, hole-filling, erosion").
pub(crate) fn mask_sections(sections: &[MaskSection]) -> Vec<String> {
    sections.iter().map(|section| {
        let input = match section.input {
            MaskingInput::PhaseQuality => "phase quality",
            MaskingInput::Magnitude => "magnitude",
            MaskingInput::MagnitudeFirst => "first-echo magnitude",
            MaskingInput::MagnitudeLast => "last-echo magnitude",
        };
        let generator = match &section.generator {
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => "Otsu threshold".to_string(),
            MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => format!("threshold {}", value.unwrap_or(0.5)),
            MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } =>
                format!("{}th-percentile threshold", value.unwrap_or(75.0)),
            MaskOp::Threshold { method: MaskThresholdMethod::StiGui, .. } => "STI Suite threshold".to_string(),
            MaskOp::Bet { .. } => "BET".to_string(),
            MaskOp::HdBet { .. } => "HD-BET".to_string(),
            MaskOp::Rs2Net { .. } => "RS2-Net".to_string(),
            MaskOp::MindGrab { .. } => "MindGrab".to_string(),
            other => describe_refinements(std::slice::from_ref(other), &mut Draft::default()).join(", "),
        };
        let steps: Vec<String> = std::iter::once(generator)
            .chain(describe_refinements(&section.refinements, &mut Draft::default()))
            .collect();
        format!("{input}: {}", steps.join(", "))
    }).collect()
}

/// QSMART's settings that differ from their defaults (its inner inversion is said apart).
pub(crate) fn qsmart_settings(config: &PipelineConfig) -> Vec<String> {
    let mut c = qsmart_changes(config);
    c.add(config.inversion.qsmart.vessel_magnitude == QsmartVesselMagnitude::Combined,
        || "vessels on the combined magnitude".to_string());
    c.0
}
