//! QSM.rs's settings in QSMxT's terms: which of QSMxT's settings each QSM.rs setting is, and
//! its values' names.

use super::*;

// ─── QSM.rs's settings in QSMxT's terms ───

/// How a QSM.rs setting is one of QSMxT's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mapping {
    /// The same value at `qsmxt` (choices by the same names; numbers, flags and vectors as they
    /// are).
    Same,
    /// Each mask section's generator (`MaskOp::id`, the step's `op`).
    MaskGenerator,
    /// Each HD-BET step's inference (`full-volume`, `sliding-window`, `low-memory`,
    /// `custom-window`), its `patch` in QSMxT.
    HdBetInference,
    /// `dct`, `fft-12`, `fft-64`, `fft`, `mirror` or `mgpcg`: the solver and, for FFT, its padding.
    LaplacianSolver,
    /// `native`, `isotropic-finest`, `isotropic-1mm` or `isotropic`: the grid and its spacing.
    VsharpGrid,
    /// `full-volume`, `low-memory` or `sliding-window`: SynthSeg's `patch`.
    SynthSegInference,
    /// SynthSeg's window edge: its `patch`.
    SynthSegPatch,
    /// Each mask section's input image (`magnitude-first`, `magnitude`, ...): every section's.
    MaskInput,
    /// `none`, `makehomogeneous` or `n4`: `masking.inhomogeneity_correction` and its method.
    Inhomogeneity,
    /// Each BET step's fractional intensity: every BET step's.
    BetFraction,
    /// Each mask section's erosion, voxels: its `erode` refinements, replaced by one of that many
    /// iterations (none for 0).
    Erosion,
    /// Each mask section's lowest input intensity kept: its `min-intensity` refinement (0 =
    /// none), placed first, right after the generator (QSMART's `brainmask.m` thresholds BET's
    /// brain image before anything else).
    MinIntensity,
    /// Each mask section's majority filter (a flag): its `majority` refinement, placed after any
    /// `min-intensity`.
    Majority,
    /// Each threshold section's method (`otsu`, `fixed`, `percentile`, `sti-gui`). STI Suite's
    /// threshold is the start of the GUI's own recipe, so switching to it drops the section's
    /// refinements (the robust-threshold preset's dilation, hole-filling and erosion); the
    /// recipe's other steps are rules of their own.
    ThresholdMethod,
    /// Each mask section keeps its largest 26-connected component (a flag): its
    /// `largest-component` refinement, placed before any `imfill`.
    LargestComponent,
    /// Each mask section's holes filled whatever their size (a flag): its `imfill` refinement,
    /// placed last.
    Imfill,
    /// Each mask section's closing radius, voxels: its first `close` refinement's radius (0 = no
    /// closing); a new one is placed after any `min-intensity` and `majority`.
    CloseRadius,
    /// QSMART's echo-fit threshold (`absolute-40`, `percentile-99`, ...): its mode and value.
    QsmartFitThreshold,
    /// ROMEO's edge weights by ROMEO.jl's preset name (`romeo4`, `romeo3`, `romeo2`, `romeo6`,
    /// `bestpath`; qsm-core's `RomeoParams::weights_id`): QSMxT's six weight flags and best-path
    /// switch, set together. Shown on the phase-linearity flag, the one that tells `romeo4` (QSMxT's
    /// default) from `romeo3` (the `romeo` command line's).
    RomeoWeights,
    /// ROMEO's template echo, counted from 1 as ROMEO's `--template` and `--romeo-template` count
    /// it; QSMxT's config counts from 0.
    RomeoTemplate,
}

/// Each of QSM.rs's rule settings ([`core::SETTINGS`]), as a QSMxT setting.
pub(super) const SETTINGS: &[(&str, &str, Mapping)] = &[
    ("masking.generator", "masking.sections", Mapping::MaskGenerator),
    ("masking.hd_bet.inference", "masking.sections", Mapping::HdBetInference),
    ("field_mapping.unwrapping_algorithm", "field_mapping.unwrapping_algorithm", Mapping::Same),
    ("field_mapping.b0_estimation", "field_mapping.b0_estimation", Mapping::Same),
    ("field_mapping.b0_estimation.weighting", "field_mapping.b0_weight_type", Mapping::Same),
    ("field_mapping.phase_offset_removal", "field_mapping.phase_offset_removal", Mapping::Same),
    ("field_mapping.phase_offset_sigma_mm", "field_mapping.phase_offset_sigma_mm", Mapping::Same),
    ("field_mapping.bipolar_correction", "field_mapping.bipolar_correction", Mapping::Same),
    ("field_mapping.laplacian_solver", "field_mapping.laplacian_solver", Mapping::LaplacianSolver),
    ("field_mapping.laplacian_unmasked", "field_mapping.laplacian_unmasked", Mapping::Same),
    ("bg_removal.algorithm", "bg_removal.algorithm", Mapping::Same),
    ("bg_removal.vsharp.deconvolve", "bg_removal.vsharp.deconvolve", Mapping::Same),
    ("bg_removal.vsharp.max_radius", "bg_removal.vsharp.max_radius", Mapping::Same),
    ("bg_removal.vsharp.min_radius", "bg_removal.vsharp.min_radius", Mapping::Same),
    ("bg_removal.vsharp.radius_step", "bg_removal.vsharp.radius_step", Mapping::Same),
    ("bg_removal.vsharp.grid", "bg_removal.vsharp.grid", Mapping::VsharpGrid),
    ("bg_removal.iharperella.rim", "bg_removal.iharperella.rim", Mapping::Same),
    ("bg_removal.ismv.variant", "bg_removal.ismv.variant", Mapping::Same),
    ("bg_removal.ismv.tol", "bg_removal.ismv.tol", Mapping::Same),
    ("bg_removal.ismv.max_iter", "bg_removal.ismv.max_iter", Mapping::Same),
    ("bg_removal.resharp.radius", "bg_removal.resharp.radius", Mapping::Same),
    ("bg_removal.resharp.max_iter", "bg_removal.resharp.max_iter", Mapping::Same),
    ("inversion.algorithm", "inversion.algorithm", Mapping::Same),
    ("inversion.star.pad", "inversion.star.pad", Mapping::Same),
    ("inversion.ilsqr.max_iter", "inversion.ilsqr.max_iter", Mapping::Same),
    ("inversion.ilsqr.pad_mm", "inversion.ilsqr.pad_mm", Mapping::Same),
    ("inversion.whqsm.weight", "inversion.whqsm.weight", Mapping::Same),
    ("inversion.star.te", "inversion.star.te", Mapping::Same),
    ("inversion.fansi.te", "inversion.fansi.te", Mapping::Same),
    ("inversion.ndi.te", "inversion.ndi.te", Mapping::Same),
    ("inversion.ndi.auto_stop", "inversion.ndi.auto_stop", Mapping::Same),
    ("inversion.ndi.tau", "inversion.ndi.tau", Mapping::Same),
    ("inversion.ndi.alpha", "inversion.ndi.alpha", Mapping::Same),
    ("inversion.ndi.max_iter", "inversion.ndi.max_iter", Mapping::Same),
    ("inversion.ndi.weight_norm", "inversion.ndi.weight_norm", Mapping::Same),
    ("inversion.l1qsm.te", "inversion.l1qsm.te", Mapping::Same),
    ("inversion.whqsm.te", "inversion.whqsm.te", Mapping::Same),
    ("inversion.medi.msmv", "inversion.medi.msmv", Mapping::Same),
    ("inversion.medi.msmv.prefilter", "inversion.medi.msmv_prefilter", Mapping::Same),
    ("inversion.medi.te", "inversion.medi.te", Mapping::Same),
    ("inversion.tfi.te", "inversion.tfi.te", Mapping::Same),
    ("inversion.apply_heidi", "inversion.heidi.enabled", Mapping::Same),
    ("inversion.heidi.te", "inversion.heidi.te", Mapping::Same),
    ("inversion.qsmart.inversion", "inversion.qsmart.inversion", Mapping::Same),
    ("inversion.qsmart.ilsqr_max_iter", "inversion.qsmart.ilsqr_max_iter", Mapping::Same),
    ("inversion.qsmart.ilsqr_pad_mm", "inversion.qsmart.ilsqr_pad_mm", Mapping::Same),
    ("inversion.qsmart.frangi_c_mode", "inversion.qsmart.frangi_c_mode", Mapping::Same),
    ("masking.input", "masking.sections", Mapping::MaskInput),
    ("masking.inhomogeneity_correction", "masking.inhomogeneity_correction", Mapping::Inhomogeneity),
    ("masking.bet.fractional_intensity", "masking.sections", Mapping::BetFraction),
    ("masking.erosion", "masking.sections", Mapping::Erosion),
    ("masking.min_intensity", "masking.sections", Mapping::MinIntensity),
    ("masking.majority", "masking.sections", Mapping::Majority),
    ("masking.threshold_method", "masking.sections", Mapping::ThresholdMethod),
    ("masking.largest_component", "masking.sections", Mapping::LargestComponent),
    ("masking.imfill", "masking.sections", Mapping::Imfill),
    ("masking.close_radius", "masking.sections", Mapping::CloseRadius),
    ("masking.bet.orientation", "bet.orientation", Mapping::Same),
    ("inversion.heidi.denoise_itk", "inversion.heidi.denoise_reference_itk", Mapping::Same),
    ("field_mapping.b0_estimation.qsmart_fit_threshold", "field_mapping.qsmart_fit_threshold_mode", Mapping::QsmartFitThreshold),
    // `individual`, `correct_global` and `temporal_uncertain_unwrapping` are at QSMxT's own paths.
    ("field_mapping.romeo.weights", "field_mapping.romeo.phase_linearity", Mapping::RomeoWeights),
    ("field_mapping.romeo.template", "field_mapping.romeo.template", Mapping::RomeoTemplate),
    ("reference", "qsm.reference", Mapping::Same),
    ("qsm_per_echo", "qsm.per_echo", Mapping::Same),
    ("tukey_window", "pipeline.tukey_window", Mapping::Same),
    ("separation.algorithm", "separation.algorithm", Mapping::Same),
    ("separation.decompose.unwrap_phase", "separation.decompose.unwrap_phase", Mapping::Same),
    ("segmentation.inference", "segmentation.patch", Mapping::SynthSegInference),
    ("segmentation.inference.patch", "segmentation.patch", Mapping::SynthSegPatch),
];

/// A QSM.rs rule setting as QSMxT's: listed in [`SETTINGS`], or else the QSMxT setting at the same
/// path, taken as it is ([`Mapping::Same`]). So a rule QSM.rs adds on a setting QSMxT already has
/// (`separation.decompose.chi_bound`, ...) is offered without a change here.
pub(super) fn setting_of(core_path: &str) -> Option<(&'static str, Mapping)> {
    SETTINGS.iter().find(|(c, _, _)| *c == core_path).map(|&(_, q, k)| (q, k))
        .or_else(|| setting(core_path).map(|s| (s.path, Mapping::Same)))
}

/// The QSM.rs setting path of rule `id` (several QSM.rs settings can be parts of one QSMxT
/// setting); `None` for an unknown rule or one of QSMxT's own.
pub fn core_setting(id: &str) -> Option<&'static str> {
    core::RULES.iter().find(|r| r.id == id).map(|r| r.setting)
}

/// Whether `id` names one of QSM.rs's rules.
pub fn is_rule(id: &str) -> bool {
    core::RULES.iter().any(|r| r.id == id)
}

/// Whether `id` is a reference-pipeline rule (`reference` strength): it is never applied by
/// `--accept-recommendations` and the methods Note lists its differences either way, so it has no
/// dismissal ([`keep`], [`dismiss`]).
pub fn is_reference_rule(id: &str) -> bool {
    core::RULES.iter().any(|r| r.id == id && r.strength == core::Strength::Reference)
}

/// The QSMxT config path of a QSM.rs rule setting (`reference` → `qsm.reference`).
pub fn qsmxt_path(core_path: &str) -> Option<&'static str> {
    setting_of(core_path).map(|(q, _)| q)
}

/// A rule's value (or a QSM.rs current value) as JSON, as QSM.rs serialises it.
pub(crate) fn core_json(v: &core::Value) -> Value {
    match *v {
        core::Value::Bool(b) => json!(b),
        core::Value::Number(x) => number(x),
        core::Value::Choice(c) => json!(c),
        core::Value::Vector(xs) => Value::Array(xs.iter().map(|&x| number(x)).collect()),
    }
}

pub(super) fn current_json(v: &Current) -> Value {
    match v {
        Current::Bool(b) => json!(b),
        Current::Number(x) => number(*x),
        Current::Choice(c) => json!(c),
        Current::Vector(xs) => Value::Array(xs.iter().map(|&x| number(x)).collect()),
    }
}

/// A whole number as an integer (so an integer setting accepts it), anything else as a float.
pub(super) fn number(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 { json!(x as i64) } else { json!(x) }
}

/// A schema setting, borrowed (looked up often: [`crate::schema::setting`] clones).
pub(super) fn setting(path: &str) -> Option<&'static crate::schema::Setting> {
    crate::schema::settings().iter().find(|s| s.path == path)
}

/// The mask recipe's step types, built once.
pub(super) fn mask_steps() -> &'static [crate::schema::mask::MaskStep] {
    static STEPS: std::sync::LazyLock<Vec<crate::schema::mask::MaskStep>> =
        std::sync::LazyLock::new(|| crate::schema::mask_schema().steps);
    &STEPS
}

/// A number as it is said (`2`, not `2.0`); anything else as JSON.
fn num_text(v: &Value) -> String {
    v.as_f64().map_or_else(|| v.to_string(), |x| x.to_string())
}

/// A value's display name for the QSMxT setting at `path` (of `kind`).
pub(super) fn name_of(path: &str, kind: Mapping, value: &Value) -> String {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::Null => "none".into(),
        Value::Array(xs) => xs.iter().map(num_text).collect::<Vec<_>>().join(" × "),
        other => num_text(other),
    };
    match kind {
        Mapping::MaskGenerator => {
            let op = value.as_str().unwrap_or_default();
            mask_steps().iter().find(|s| s.op == op)
                .map_or_else(|| op.to_string(), |s| s.name.to_string())
        }
        Mapping::HdBetInference => match value.as_str().unwrap_or_default() {
            "full-volume" => "whole volume".into(),
            "sliding-window" => "sliding window".into(),
            "low-memory" => "low-memory sliding window".into(),
            other => other.replace('-', " "),
        },
        Mapping::LaplacianSolver => match value.as_str().unwrap_or_default() {
            "dct" => "DCT".into(),
            "fft-12" => "FFT, 12-voxel padding".into(),
            "fft-64" => "FFT, 64-voxel padding".into(),
            "fft" => "FFT".into(),
            "mirror" => "mirror".into(),
            "mgpcg" => "MGPCG".into(),
            other => other.into(),
        },
        Mapping::VsharpGrid => match value.as_str().unwrap_or_default() {
            "native" => "acquired grid".into(),
            "isotropic-finest" => "isotropic at the finest voxel size".into(),
            "isotropic-1mm" => "1 mm isotropic grid".into(),
            "isotropic" => "isotropic grid".into(),
            other => other.into(),
        },
        Mapping::SynthSegInference => match value.as_str().unwrap_or_default() {
            "full-volume" => "whole volume".into(),
            "low-memory" => "low-memory sliding window".into(),
            "sliding-window" => "sliding window".into(),
            other => other.into(),
        },
        Mapping::SynthSegPatch => format!("{}-voxel window", text(value)),
        Mapping::MaskInput => match value.as_str().unwrap_or_default() {
            "magnitude-first" => "first echo".into(),
            "magnitude" => "RSS magnitude".into(),
            "magnitude-last" => "last echo".into(),
            "phase-quality" => "phase quality".into(),
            other => other.into(),
        },
        Mapping::Inhomogeneity => match value.as_str().unwrap_or_default() {
            "none" => "none".into(),
            "makehomogeneous" => "makehomogeneous".into(),
            "n4" => "N4".into(),
            other => other.into(),
        },
        Mapping::BetFraction => format!("f = {}", text(value)),
        Mapping::Erosion => match value.as_f64() {
            Some(0.0) => "no erosion".into(),
            Some(1.0) => "1 voxel".into(),
            _ => format!("{} voxels", text(value)),
        },
        Mapping::MinIntensity => match value.as_f64() {
            Some(0.0) => "no intensity threshold".into(),
            _ => format!("intensity threshold {}", text(value)),
        },
        Mapping::Majority => match value.as_bool() {
            Some(true) => "majority filter".into(),
            _ => "no majority filter".into(),
        },
        Mapping::ThresholdMethod => match value.as_str().unwrap_or_default() {
            "sti-gui" => "thresh_tool's starting level".into(),
            "otsu" => "Otsu's threshold".into(),
            "none" => "no threshold".into(),
            other => format!("{other} threshold"),
        },
        Mapping::LargestComponent => match value.as_bool() {
            Some(true) => "largest component only".into(),
            _ => "every component".into(),
        },
        Mapping::Imfill => match value.as_bool() {
            Some(true) => "every hole filled".into(),
            _ => "holes not filled".into(),
        },
        Mapping::CloseRadius => match value.as_f64() {
            Some(0.0) => "no closing".into(),
            _ => format!("closing radius {}", text(value)),
        },
        Mapping::QsmartFitThreshold => match value.as_str().unwrap_or_default() {
            "absolute-40" => "fixed residual 40 (QSMART v1.0)".into(),
            "absolute" => "a fixed residual".into(),
            "percentile-99" => "99th percentile".into(),
            "percentile" => "a percentile".into(),
            other => other.into(),
        },
        Mapping::RomeoWeights => match value.as_str().unwrap_or_default() {
            "romeo4" => "romeo4 (with phase linearity)".into(),
            "romeo3" => "romeo3 (without phase linearity)".into(),
            "romeo2" => "romeo2 (phase and magnitude coherence)".into(),
            "romeo6" => "romeo6 (with magnitude weights)".into(),
            "bestpath" => "best-path weights".into(),
            "custom" => "a custom combination of weights".into(),
            other => other.into(),
        },
        Mapping::RomeoTemplate => text(value),
        Mapping::Same => {
            let Some(s) = setting(path) else { return text(value) };
            if let Some(o) = s.value_type.options().into_iter().find(|o| o.value == *value) {
                return o.name.clone();
            }
            match (s.units, value) {
                (Some(u), Value::Number(_) | Value::Array(_)) => format!("{} {u}", text(value)),
                _ => text(value),
            }
        }
    }
}

/// The current value(s) of a QSM.rs setting in QSMxT's terms: for a [`Mapping::Same`] setting, the
/// config's own value (QSMxT's region referencing is `none` to QSM.rs, but `region` here).
pub(super) fn current_named(config: &Value, core_path: &str, core_current: &[Current]) -> Vec<Named> {
    let Some((path, kind)) = setting_of(core_path) else { return vec![] };
    match kind {
        Mapping::Same => {
            let v = path.split('.').try_fold(config, |v, k| v.get(k)).cloned().unwrap_or(Value::Null);
            vec![Named { name: name_of(path, kind, &v), value: v }]
        }
        _ => core_current.iter().map(|c| {
            let v = current_json(c);
            Named { name: name_of(path, kind, &v), value: v }
        }).collect(),
    }
}

pub(super) fn named(core_path: &str, v: &core::Value) -> Named {
    let (path, kind) = setting_of(core_path).unwrap_or(("", Mapping::Same));
    let v = core_json(v);
    Named { name: name_of(path, kind, &v), value: v }
}

pub(super) fn label_of(core_path: &str) -> String {
    match core_path {
        "masking.generator" => "Brain mask".into(),
        "masking.hd_bet.inference" => "HD-BET inference".into(),
        "segmentation.inference" => "SynthSeg inference".into(),
        "field_mapping.romeo.weights" => "ROMEO weights".into(),
        _ => {
            let path = qsmxt_path(core_path).unwrap_or(core_path);
            setting(path).map_or_else(|| path.to_string(), |s| match s.context {
                Some(ctx) if !s.full_label.contains(ctx) => format!("{} ({ctx})", s.full_label),
                _ => s.full_label.to_string(),
            })
        }
    }
}
