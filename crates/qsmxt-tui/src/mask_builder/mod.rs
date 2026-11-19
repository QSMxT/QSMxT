//! The mask builder: the Masking page, and its editor of the mask recipes.
//!
//! The recipes are `config.masking`'s (the main mask's sections, how they combine and the steps
//! on the combined mask; two-pass's reliable mask and how its sections combine), kept in the
//! form's settings store by path like every other setting of the page. The editor reads them as a
//! [`MaskRecipe`], changes that copy, and writes it back ([`App::edit_mask`]). The page's other
//! settings (custom masks, BET's surface, the magnitude correction, two-pass) are rows from the
//! registry, one widget per value type.

use qsmxt_engine::schema::{self, Section};
use qsmxt_engine::PipelineConfig;

use qsmxt_engine::{MaskCombine, MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};

use super::app::*;
use super::settings_rows::{depth_of, setting, SettingsStore};

/// The recipe's settings, by path.
const SECTIONS: &str = "masking.sections";
const COMBINE: &str = "masking.combine";
const REFINEMENTS: &str = "masking.refinements";
const TWO_PASS: &str = "masking.two_pass";
const TWO_PASS_SECTIONS: &str = "masking.two_pass_sections";
const TWO_PASS_COMBINE: &str = "masking.two_pass_combine";

/// The settings the editor edits: put back to their defaults with the page.
pub const RECIPE_PATHS: &[&str] = &[SECTIONS, COMBINE, REFINEMENTS, TWO_PASS_SECTIONS, TWO_PASS_COMBINE];

/// BET's settings that are one value per run, shared by every BET step, unlike its fractional
/// intensity and voxel scale, which are part of each step.
const BET_SURFACE: &[&str] = &["bet.smoothness", "bet.gradient_threshold", "bet.iterations", "bet.subdivisions", "bet.orientation"];

/// The mask recipes as the form holds them: the settings store's, read for the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskRecipe {
    /// The main mask's sections, folded together with `combine`.
    pub sections: Vec<MaskSection>,
    /// How the sections fold together (OR = union, AND = intersection).
    pub combine: MaskCombine,
    /// Refinements applied to the combined mask, after `combine`.
    pub refinements: Vec<MaskOp>,
    /// Whether two-pass artefact reduction runs (its switch is a row of its own).
    pub two_pass: bool,
    /// The reliable-pass mask. Edited with the same rows as `sections`, addressed by
    /// [`MASK_TWO_PASS_BASE`] + index, folded with `two_pass_combine` and then the main mask's
    /// post-combine refinements.
    pub two_pass_sections: Vec<MaskSection>,
    /// How the reliable mask's sections combine: `None` = as the main mask's do
    /// ([`MaskRecipe::two_pass_combine_mode`]).
    pub two_pass_combine: Option<MaskCombine>,
}

fn read<T: serde::de::DeserializeOwned>(st: &SettingsStore, path: &str) -> Option<T> {
    serde_json::from_value(st.get(path)).ok()
}

mod editor;
mod recipe;
mod steps;

use steps::*;

#[cfg(test)]
mod tests;
