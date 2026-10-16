//! The parts of a mask recipe (`masking.sections`, `masking.refinements`), described for a mask
//! builder: the images a section can start from, the step types and their parameters, HD-BET's
//! inference choices, and the presets.
//!
//! The TUI's mask editor reads its wording from here, so the two front ends say the same.

use serde::Serialize;
use serde_json::{json, Value};

use super::{EnumOption, ValueType};
use crate::masking::{
    hd_bet_low_memory_patch, mask_presets, MaskCombine, MaskOp,
    MaskSection, MaskThresholdMethod, MaskingInput,
};

/// A mask recipe's parts. See [`mask_schema`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaskSchema {
    /// The images a section can start from, in the TUI's order.
    pub inputs: Vec<MaskInput>,
    /// The step types: the ones that start a section, then the ones that refine a mask.
    pub steps: Vec<MaskStep>,
    /// HD-BET's inference choices (its `patch`), with what each costs in memory.
    pub hd_bet_inference: Vec<HdBetChoice>,
    /// The `--mask-preset` recipes, in the TUI's order.
    pub presets: Vec<MaskPreset>,
}

/// An image a mask section can start from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaskInput {
    /// The value as it is written in the config (`phase-quality`).
    pub value: MaskingInput,
    /// Its display name.
    pub label: &'static str,
    pub help: &'static str,
}

/// A step type of a mask recipe.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaskStep {
    /// The step type's id. A threshold is three types, one per method (`threshold-otsu`); the
    /// others are named by their `op`.
    pub kind: &'static str,
    /// The `op` it is written with in the config.
    pub op: &'static str,
    /// How the TUI names it, and how `--mask` spells it (`gaussian`).
    pub label: &'static str,
    /// Its display name.
    pub name: &'static str,
    pub help: &'static str,
    /// Whether it makes a mask from the section's image, and so starts a section (a section's
    /// `generator`), rather than refining the mask before it.
    pub starts_section: bool,
    /// Whether it reads the magnitude (the section's: first echo, last echo, or every echo
    /// combined), whatever image the section thresholds.
    pub needs_magnitude: bool,
    /// Whether it runs a neural network, which needs a deep-learning build and its weights.
    pub deep_learning: bool,
    /// The step as written in the config, with every parameter at its default.
    pub default: Value,
    pub params: Vec<MaskParam>,
    /// Settings that apply to every step of this type (BET's surface settings), by path.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<&'static str>,
}

/// A parameter of a mask step: a field of the step as written in the config.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaskParam {
    /// The field it is written as (`fractional_intensity`).
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    #[serde(flatten)]
    pub value_type: ValueType,
    /// Whether the value may be `null`.
    pub optional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclusive_min: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclusive_max: bool,
    pub default: Value,
    /// The parameter applies only while this other parameter of the step is set (not `null`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only_with: Option<&'static str>,
}

/// One of HD-BET's inference choices: the whole volume, or a sliding window of patches.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HdBetChoice {
    /// The step's `patch` for it: `null` for the whole volume, else `[x, y, z]` in voxels at 1 mm.
    pub value: Value,
    /// How `--mask` spells it (`hd-bet:low-memory`).
    pub label: &'static str,
    pub name: &'static str,
    pub help: &'static str,
    /// The peak memory QSMxT budgets for the network, in bytes (the working volumes of the scan
    /// itself come on top). For the whole volume, a 176×256×256 head at 1 mm.
    pub peak_memory_bytes: u64,
}

/// A `--mask-preset`: a whole recipe.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaskPreset {
    /// The preset's name, as `--mask-preset` takes it.
    pub value: &'static str,
    pub name: &'static str,
    pub help: &'static str,
    /// `masking.sections`, `masking.combine` and `masking.refinements` as the preset sets them.
    pub sections: Vec<MaskSection>,
    pub combine: MaskCombine,
    pub refinements: Vec<MaskOp>,
}

/// Every image a section can start from, in the TUI's order.
pub const MASK_INPUTS: [MaskingInput; 4] =
    [MaskingInput::MagnitudeFirst, MaskingInput::Magnitude, MaskingInput::MagnitudeLast, MaskingInput::PhaseQuality];

fn input(value: MaskingInput) -> MaskInput {
    let (label, help) = match value {
        MaskingInput::MagnitudeFirst => ("First-echo magnitude", "The first echo's magnitude"),
        MaskingInput::Magnitude => ("Magnitude", "The magnitude of every echo, combined (root sum of squares)"),
        MaskingInput::MagnitudeLast => ("Last-echo magnitude", "The last echo's magnitude"),
        MaskingInput::PhaseQuality => ("Phase quality",
            "ROMEO's voxel quality from the phase of the first two echoes (the first alone for one echo), \
             weighted by the combined magnitude: low where the phase is noisy or changes fast"),
    };
    MaskInput { value, label, help }
}

/// The step type of an op as written in a config.
pub fn step_kind(op: &MaskOp) -> &'static str {
    match op {
        MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => "threshold-otsu",
        MaskOp::Threshold { method: MaskThresholdMethod::Fixed, .. } => "threshold-fixed",
        MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. } => "threshold-percentile",
        MaskOp::Bet { .. } => "bet",
        MaskOp::HdBet { .. } => "hd-bet",
        MaskOp::Rs2Net { .. } => "rs2-net",
        MaskOp::MindGrab { .. } => "mindgrab",
        MaskOp::Erode { .. } => "erode",
        MaskOp::Dilate { .. } => "dilate",
        MaskOp::Close { .. } => "close",
        MaskOp::FillHoles { .. } => "fill-holes",
        MaskOp::GaussianSmooth { .. } => "gaussian-smooth",
        MaskOp::SignalErode { .. } => "signal-erode",
    }
}

/// Each step type's default op, in order: the ones that start a section, then the refinements.
fn default_ops() -> Vec<MaskOp> {
    use MaskThresholdMethod::*;
    vec![
        MaskOp::Threshold { method: Otsu, value: None },
        MaskOp::Threshold { method: Fixed, value: Some(0.5) },
        MaskOp::Threshold { method: Percentile, value: Some(75.0) },
        MaskOp::bet(0.5),
        MaskOp::hd_bet_default(),
        MaskOp::mindgrab_default(),
        MaskOp::rs2_net_default(),
        MaskOp::Erode { iterations: 1 },
        MaskOp::Dilate { iterations: 1 },
        MaskOp::Close { radius: 1 },
        MaskOp::FillHoles { max_size: 0 },
        MaskOp::GaussianSmooth { sigma_mm: 4.0 },
        MaskOp::signal_erode_default(),
    ]
}

/// A parameter, with its default read from the step's default op.
struct P {
    key: &'static str,
    label: &'static str,
    help: &'static str,
    ty: ValueType,
    units: Option<&'static str>,
    min: Option<(f64, bool)>,
    max: Option<(f64, bool)>,
    optional: bool,
    only_with: Option<&'static str>,
}

fn p(key: &'static str, label: &'static str, help: &'static str, ty: ValueType) -> P {
    P { key, label, help, ty, units: None, min: None, max: None, optional: false, only_with: None }
}

impl P {
    fn units(mut self, u: &'static str) -> Self { self.units = Some(u); self }
    fn min(mut self, v: f64) -> Self { self.min = Some((v, false)); self }
    fn above(mut self, v: f64) -> Self { self.min = Some((v, true)); self }
    fn max(mut self, v: f64) -> Self { self.max = Some((v, false)); self }
    fn optional(mut self) -> Self { self.optional = true; self }
    fn only_with(mut self, k: &'static str) -> Self { self.only_with = Some(k); self }
}

const TTA_HELP: &str = "8-fold mirroring test-time augmentation: about 8 times slower";

/// The wording and parameters of a step type.
fn describe(op: &MaskOp) -> (&'static str, &'static str, &'static str, Vec<P>) {
    use ValueType::*;
    match op {
        MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => ("threshold", "Otsu threshold",
            "Keep the voxels of the section's image above Otsu's threshold", vec![]),
        MaskOp::Threshold { method: MaskThresholdMethod::Fixed, .. } => ("threshold", "Fixed threshold",
            "Keep the voxels of the section's image above a value", vec![
                p("value", "Threshold", "Voxels above this value are kept", Float).optional(),
            ]),
        MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. } => ("threshold", "Percentile threshold",
            "Keep the voxels of the section's image above a percentile of its positive values", vec![
                p("value", "Percentile", "The percentile of the image's positive values above which voxels are kept", Float)
                    .min(0.0).max(100.0).optional(),
            ]),
        MaskOp::Bet { .. } => ("bet", "BET",
            "BET brain extraction: a surface fitted to the edge of the brain in the magnitude", vec![
                p("fractional_intensity", "Fractional Intensity", "Smaller values give a larger brain", Float)
                    .min(0.0).max(1.0),
                // masking.rs parse_bet_voxel_scale refuses a scale that is not > 0.
                p("voxel_scale", "Voxel Scale",
                  "Voxel sizes are multiplied by this before BET runs: its surface model is tuned to a human \
                   brain in mm, so preclinical data needs presenting at human scale (10 for mouse); 1 keeps the \
                   acquired geometry", Float).above(0.0),
            ]),
        MaskOp::HdBet { .. } => ("hd-bet", "HD-BET", "HD-BET deep-learning brain extraction from the magnitude (Isensee et al., 2019)", vec![
            p("patch", "Inference", "One pass over the whole volume, or sliding-window patches, which bound the memory",
              Enum { options: hd_bet_choices().into_iter().map(|c| option(c.value, c.label, c.name, c.help)).collect() })
                .optional(),
            p("tta", "Mirroring TTA", TTA_HELP, Bool),
            // masking.rs parse_window_step refuses a step outside (0, 1].
            p("tile_step", "Patch Step",
              "Sliding-window step, as a fraction of the patch: larger steps mean fewer patches and a shorter \
               run, at softer patch seams", Float).above(0.0).max(1.0).only_with("patch"),
        ]),
        MaskOp::Rs2Net { .. } => ("rs2-net", "RS2-Net",
            "RS2-Net deep-learning rodent brain extraction (mouse, rat) from the magnitude (Lin et al., 2024). Its patch is fixed by \
             the network; larger volumes are tiled", vec![
                p("tta", "Mirroring TTA", TTA_HELP, Bool),
                // masking.rs parse_window_step refuses a step outside (0, 1].
                p("tile_step", "Patch Step",
                  "Sliding-window step, as a fraction of the patch: larger steps mean fewer patches and a shorter \
                   run, at softer patch seams", Float).above(0.0).max(1.0),
            ]),
        MaskOp::MindGrab { .. } => ("mindgrab", "MindGrab",
            "MindGrab deep-learning brain extraction from the magnitude, for any contrast (Fani et al., 2025)", vec![
                p("tta", "Flip TTA", "Left-right flip test-time augmentation: a second, mirrored pass; about twice as slow", Bool),
            ]),
        MaskOp::Erode { .. } => ("erode", "Erode", "Remove the mask's boundary voxels, once per iteration", vec![
            p("iterations", "Iterations", "How many layers of voxels to remove", Int).min(1.0),
        ]),
        MaskOp::Dilate { .. } => ("dilate", "Dilate", "Add a layer of voxels around the mask, once per iteration", vec![
            p("iterations", "Iterations", "How many layers of voxels to add", Int).min(1.0),
        ]),
        MaskOp::Close { .. } => ("close", "Close",
            "Morphological closing, a dilation then an erosion with a sphere: closes gaps and dents in the edge", vec![
                p("radius", "Radius", "Radius of the sphere", Int).units("voxels").min(1.0),
            ]),
        MaskOp::FillHoles { .. } => ("fill-holes", "Fill holes", "Fill the holes enclosed by the mask, up to a size", vec![
            p("max_size", "Largest Hole", "The largest hole filled; 0 fills holes up to 5% of the volume", Int)
                .units("voxels").min(0.0),
        ]),
        MaskOp::GaussianSmooth { .. } => ("gaussian", "Smooth",
            "Blur the mask with a Gaussian and keep the voxels above one half: rounds off its edge", vec![
                p("sigma_mm", "Sigma", "Gaussian sigma of the blur", Float).units("mm").above(0.0),
            ]),
        MaskOp::SignalErode { .. } => ("signal-erode", "Signal-gated erosion",
            "Remove only low-signal boundary voxels (sinus and skull-base dropout), with the receive-coil bias \
             divided out of the magnitude, down to a depth cap", vec![
                p("threshold", "Threshold",
                  "Signal gate as a fraction of the in-mask median magnitude; above about 0.85 it over-carves", Float)
                    .above(0.0).max(1.0),
                p("depth_cap", "Depth Cap", "How far below the original surface removal may reach; 0 for no cap", Int)
                    .units("voxels").min(0.0),
                p("global_erosions", "Plain Erosions",
                  "Plain erosions before the gate: unlike a separate erode step they spend the depth budget and \
                   leave the gate unchanged", Int).min(0.0),
                p("bias_sigma", "Bias Sigma", "Gaussian scale of the receive-coil bias estimate divided out of the magnitude", Float)
                    .units("voxels").above(0.0),
                p("min_component", "Minimum Component",
                  "Keep every connected component at least this large, not just the largest", Int)
                    .units("voxels").min(0.0),
            ]),
    }
}

fn option(value: Value, label: &'static str, name: &'static str, help: &'static str) -> EnumOption {
    EnumOption { value, label, name: name.into(), help: help.into(), url: None, tui_help: help.into() }
}

/// HD-BET's inference choices.
pub fn hd_bet_choices() -> Vec<HdBetChoice> {
    let full = None;
    let window = Some(MaskOp::hd_bet_default_patch());
    let low = Some(hd_bet_low_memory_patch());
    [
        (full, "full", "Whole volume", "One pass over the head's bounding box at 1 mm: the fastest, and the most memory"),
        (window, "window", "Sliding window", "Patches of HD-BET's training size, half overlapping"),
        (low, "low-memory", "Low memory", "Smaller patches, half overlapping: the least memory, and agrees with the larger patches to Dice 0.99"),
    ]
    .into_iter()
    .map(|(patch, label, name, help)| HdBetChoice {
        value: json!(patch),
        label,
        name,
        help,
        peak_memory_bytes: crate::memory::hd_bet_network_bytes(patch),
    })
    .collect()
}

/// A preset's name and help.
fn preset_wording(name: &str) -> (&'static str, &'static str) {
    match name {
        "robust-threshold" => ("Robust threshold",
            "Otsu threshold on the phase quality, then dilate, fill holes and erode. Recommended for brain"),
        "bet" => ("BET", "BET brain extraction on the magnitude, then erode"),
        "hd-bet" => ("HD-BET", "HD-BET deep-learning brain extraction, then signal-gated erosion (QSM-CI harmonisation masking)"),
        "bet-and-phase" => ("BET and phase quality",
            "BET on the magnitude AND a thresholded phase quality, then fill holes and erode (ISMRM EMTP consensus)"),
        "rs2-net" => ("RS2-Net", "RS2-Net deep-learning rodent brain extraction (mouse, rat)"),
        "mindgrab" => ("MindGrab", "MindGrab deep-learning brain extraction, for any contrast"),
        other => panic!("mask preset {other} has no wording"),
    }
}

/// The mask recipe's parts: inputs, step types, HD-BET's inference choices and the presets.
pub fn mask_schema() -> MaskSchema {
    let steps = default_ops().into_iter().map(|op| {
        let (label, name, help, params) = describe(&op);
        let default = serde_json::to_value(&op).expect("a mask op serialises");
        let tag = default["op"].as_str().expect("a mask op is tagged");
        let op_name = OPS.iter().copied().find(|o| *o == tag).unwrap_or_else(|| panic!("mask op {tag} is not in OPS"));
        let params = params.into_iter().map(|q| MaskParam {
            key: q.key,
            label: q.label,
            help: q.help,
            default: default.get(q.key).cloned().unwrap_or(Value::Null),
            value_type: q.ty,
            optional: q.optional,
            units: q.units,
            min: q.min.map(|m| m.0),
            max: q.max.map(|m| m.0),
            exclusive_min: q.min.is_some_and(|m| m.1),
            exclusive_max: q.max.is_some_and(|m| m.1),
            only_with: q.only_with,
        }).collect();
        let deep_learning = matches!(op, MaskOp::HdBet { .. } | MaskOp::Rs2Net { .. } | MaskOp::MindGrab { .. });
        let needs_magnitude = deep_learning || matches!(op, MaskOp::Bet { .. } | MaskOp::SignalErode { .. });
        MaskStep {
            kind: step_kind(&op),
            op: op_name,
            label,
            name,
            help,
            starts_section: op.is_generator(),
            needs_magnitude,
            deep_learning,
            settings: if matches!(op, MaskOp::Bet { .. }) {
                vec!["bet.smoothness", "bet.gradient_threshold", "bet.iterations", "bet.subdivisions"]
            } else {
                vec![]
            },
            default,
            params,
        }
    }).collect();
    let presets = mask_presets().into_iter().map(|(value, r)| {
        let (name, help) = preset_wording(value);
        MaskPreset { value, name, help, sections: r.sections, combine: r.combine, refinements: r.refinements }
    }).collect();
    MaskSchema { inputs: MASK_INPUTS.into_iter().map(input).collect(), steps, hd_bet_inference: hd_bet_choices(), presets }
}

/// Every `op` a mask step is written with.
const OPS: &[&str] = &[
    "threshold", "bet", "hd-bet", "rs2-net", "mindgrab", "erode", "dilate", "close", "fill-holes", "gaussian-smooth",
    "signal-erode",
];

/// The step type `kind`.
pub fn step(kind: &str) -> Option<MaskStep> {
    mask_schema().steps.into_iter().find(|s| s.kind == kind)
}

impl MaskParam {
    /// Whether one number is inside the parameter's range.
    pub fn in_range(&self, x: f64) -> bool {
        let lo = self.min.is_none_or(|m| if self.exclusive_min { x > m } else { x >= m });
        let hi = self.max.is_none_or(|m| if self.exclusive_max { x < m } else { x <= m });
        lo && hi
    }
}

/// What is wrong with a list of mask steps, if anything: each parameter outside its range, and
/// an HD-BET patch that is not a multiple of 32×32×16 (what `--mask` refuses). `at` names where
/// the steps are ("section 1").
pub(crate) fn check_steps(ops: &[MaskOp], at: &str) -> Vec<String> {
    let steps = mask_schema().steps;
    let mut out = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        let kind = step_kind(op);
        let Some(step) = steps.iter().find(|s| s.kind == kind) else { continue };
        let v = serde_json::to_value(op).unwrap_or(Value::Null);
        for q in &step.params {
            let x = &v[q.key];
            let applies = q.only_with.is_none_or(|k| !v[k].is_null());
            let bad = match x.as_f64() {
                Some(x) => applies && !q.in_range(x),
                // A non-finite number serialises as null.
                None => matches!(q.value_type, ValueType::Float) && !q.optional && x.is_null(),
            };
            if bad {
                let range = super::range_words(q.min, q.max, q.exclusive_min, q.exclusive_max);
                out.push(format!("{at}, step {} ({}): {} must be {range}", i + 1, step.name, q.label));
            }
        }
        if let MaskOp::HdBet { patch: Some(p), .. } = op {
            if !(p[0] > 0 && p[1] > 0 && p[2] > 0 && p[0] % 32 == 0 && p[1] % 32 == 0 && p[2] % 16 == 0) {
                out.push(format!("{at}, step {} (HD-BET): the patch must be multiples of 32×32×16", i + 1));
            }
        }
    }
    out
}

/// What is wrong with a recipe's sections, if anything.
pub(crate) fn check_sections(sections: &[MaskSection]) -> Vec<String> {
    sections.iter().enumerate().flat_map(|(i, s)| check_steps(&s.all_ops(), &format!("Section {}", i + 1))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::masking::{bet_default_voxel_scale, hd_bet_default_tile_step};

    #[test]
    fn the_bet_voxel_scale_default_is_the_acquired_geometry() {
        let bet = step("bet").unwrap();
        assert_eq!(bet.params[1].default, json!(bet_default_voxel_scale()));
        let hd = step("hd-bet").unwrap();
        assert_eq!(hd.params[2].default, json!(hd_bet_default_tile_step()));
    }
}
