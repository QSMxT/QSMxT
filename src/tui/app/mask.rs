//! The mask editor's presets, step types and pseudo-sections.

use super::*;

pub const MASK_OP_TYPES: &[&str] = &[
    "threshold", "bet", "hd-bet", "erode", "dilate", "close", "min-intensity", "majority", "largest-component",
    "imfill", "fill-holes", "gaussian", "signal-erode",
];

/// Ops that create a mask (the section's generator); the rest are refinements.
pub const MASK_GENERATOR_TYPES: &[&str] = &["threshold", "bet", "hd-bet", "mindgrab", "rs2-net"];


/// The mask preset picker: the engine's presets, then "custom" ([`MASK_PRESET_CUSTOM`]).
pub static MASK_PRESET_OPTIONS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| mask_schema().presets.iter().map(|p| p.value).chain(["custom"]).collect());
/// The presets' help, from the engine's mask schema, then the "custom" entry's.
static MASK_PRESET_HELP_V: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    mask_schema().presets.iter().map(|p| p.help).chain(["Fully custom mask pipeline (edit steps below)"]).collect()
});
pub fn mask_preset_help() -> &'static [&'static str] { &MASK_PRESET_HELP_V }

/// The engine's description of a mask recipe's parts, whose wording the mask editor shows.
pub fn mask_schema() -> &'static qsmxt_engine::schema::MaskSchema {
    static SCHEMA: LazyLock<qsmxt_engine::schema::MaskSchema> = LazyLock::new(qsmxt_engine::schema::mask_schema);
    &SCHEMA
}

/// The mask step type `kind` (`erode`, `threshold-otsu`).
pub fn mask_step(kind: &str) -> &'static qsmxt_engine::schema::mask::MaskStep {
    mask_schema().steps.iter().find(|s| s.kind == kind).unwrap_or_else(|| panic!("no mask step {kind}"))
}

/// The parameter `key` of the mask step type `kind`.
pub fn mask_param(kind: &str, key: &str) -> &'static qsmxt_engine::schema::mask::MaskParam {
    mask_step(kind).params.iter().find(|p| p.key == key).unwrap_or_else(|| panic!("no parameter {key} of {kind}"))
}

/// The step type of `op`'s, from the mask schema.
pub fn mask_step_of(op: &crate::pipeline::config::MaskOp) -> &'static qsmxt_engine::schema::mask::MaskStep {
    mask_step(qsmxt_engine::schema::mask::step_kind(op))
}
/// Index of the "custom" preset — set automatically when the steps are edited by hand, and
/// skipped when cycling presets with ←/→. Derived, because it is always the trailing entry:
/// a hardcoded index silently goes stale the moment a preset is added.
pub static MASK_PRESET_CUSTOM: LazyLock<usize> = LazyLock::new(|| MASK_PRESET_OPTIONS.len() - 1);

/// The signal-erode parameters that get a row of their own, past the threshold shown on the step
/// row itself: label and help. Order matches the row order.
/// The wording is the mask schema's; the threshold is its first parameter.
static SIGNAL_ERODE_PARAMS_V: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    mask_step("signal-erode").params.iter().skip(1).map(|p| (p.label, p.help)).collect()
});
pub fn signal_erode_params() -> &'static [(&'static str, &'static str)] { &SIGNAL_ERODE_PARAMS_V }

/// Pseudo-section index for the refinements that run on the combined mask, so the mask-step rows
/// and their key handling are shared with the per-section refinements.
pub const MASK_COMBINED_SECTION: usize = usize::MAX;

/// Base of the pseudo-section range addressing the two-pass reliable mask: `MASK_TWO_PASS_BASE + i`
/// is `two_pass_sections[i]`.
///
/// The reliable mask is an ordinary mask, so it gets the ordinary mask editor — every row type,
/// key binding and validation rule, unchanged — rather than a parallel set that could drift. Only
/// the indexing distinguishes it.
pub const MASK_TWO_PASS_BASE: usize = usize::MAX / 2;

/// Whether a section index addresses the two-pass reliable mask.
pub fn is_two_pass_section(section: usize) -> bool {
    section >= MASK_TWO_PASS_BASE && section != MASK_COMBINED_SECTION
}
