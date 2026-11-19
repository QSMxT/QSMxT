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

impl MaskRecipe {
    /// The recipes the store holds.
    pub fn of(st: &SettingsStore) -> Self {
        MaskRecipe {
            sections: read(st, SECTIONS).unwrap_or_default(),
            combine: read(st, COMBINE).unwrap_or(MaskCombine::Or),
            refinements: read(st, REFINEMENTS).unwrap_or_default(),
            two_pass: read(st, TWO_PASS).unwrap_or(false),
            // The form always holds a reliable mask (`settings_rows`' FORM_DEFAULTS); unset, it
            // is the one the run resolves it to.
            two_pass_sections: read::<Option<Vec<MaskSection>>>(st, TWO_PASS_SECTIONS).flatten()
                .unwrap_or_else(qsmxt_engine::default_two_pass_sections),
            two_pass_combine: read::<Option<MaskCombine>>(st, TWO_PASS_COMBINE).flatten(),
        }
    }

    /// Write the recipes into the store (the two-pass switch is its row's).
    fn store(&self, st: &mut SettingsStore) {
        st.set(SECTIONS, json(&self.sections));
        st.set(COMBINE, json(&self.combine));
        st.set(REFINEMENTS, json(&self.refinements));
        st.set(TWO_PASS_SECTIONS, json(&self.two_pass_sections));
        st.set(TWO_PASS_COMBINE, json(&self.two_pass_combine));
    }

    /// The mask list a section index addresses, and the index within it.
    pub fn mask_section_slot(section: usize) -> (bool, usize) {
        if is_two_pass_section(section) {
            (true, section - MASK_TWO_PASS_BASE)
        } else {
            (false, section)
        }
    }

    /// The section a mask row edits — from the main mask or the two-pass reliable mask.
    pub fn mask_section(&self, section: usize) -> Option<&MaskSection> {
        let (two_pass, i) = Self::mask_section_slot(section);
        if two_pass { self.two_pass_sections.get(i) } else { self.sections.get(i) }
    }

    /// Mutable counterpart of [`Self::mask_section`].
    pub fn mask_section_mut(&mut self, section: usize) -> Option<&mut MaskSection> {
        let (two_pass, i) = Self::mask_section_slot(section);
        if two_pass { self.two_pass_sections.get_mut(i) } else { self.sections.get_mut(i) }
    }

    /// How many sections the mask a row belongs to has.
    pub fn mask_section_count(&self, two_pass: bool) -> usize {
        if two_pass { self.two_pass_sections.len() } else { self.sections.len() }
    }

    /// The refinement list a mask-step row edits — a section's, or the combined mask's.
    pub fn mask_refinements(&self, section: usize) -> Option<&Vec<MaskOp>> {
        if section == MASK_COMBINED_SECTION {
            Some(&self.refinements)
        } else {
            self.mask_section(section).map(|s| &s.refinements)
        }
    }

    /// Mutable counterpart of [`Self::mask_refinements`].
    pub fn mask_refinements_mut(&mut self, section: usize) -> Option<&mut Vec<MaskOp>> {
        if section == MASK_COMBINED_SECTION {
            Some(&mut self.refinements)
        } else {
            self.mask_section_mut(section).map(|s| &mut s.refinements)
        }
    }

    /// The first section, main mask then two-pass mask, whose generator is BET.
    fn first_bet_generator_section(&self) -> Option<usize> {
        let is_bet = |s: &MaskSection| matches!(s.generator, MaskOp::Bet { .. });
        self.sections.iter().position(is_bet).or_else(|| {
            if !self.two_pass { return None; }
            self.two_pass_sections.iter().position(is_bet).map(|i| MASK_TWO_PASS_BASE + i)
        })
    }

    /// The main mask's combine mode as the pipeline sees it: only editable — and only shown —
    /// with more than one section, so a single-section recipe reports the default.
    fn main_combine(&self) -> MaskCombine {
        if self.sections.len() < 2 { MaskCombine::Or } else { self.combine }
    }

    /// How the reliable mask's sections combine: its own mode, or the main mask's.
    pub fn two_pass_combine_mode(&self) -> MaskCombine {
        self.two_pass_combine.unwrap_or(self.main_combine())
    }

    /// How the sections of the mask `section` belongs to combine.
    pub fn combine_mode(&self, section: usize) -> MaskCombine {
        if is_two_pass_section(section) { self.two_pass_combine_mode() } else { self.combine }
    }

    /// Toggle the combine mode (OR ↔ AND) of the mask `section` belongs to.
    pub fn toggle_combine(&mut self, section: usize) {
        let flip = |c: MaskCombine| match c { MaskCombine::Or => MaskCombine::And, MaskCombine::And => MaskCombine::Or };
        if is_two_pass_section(section) {
            self.two_pass_combine = Some(flip(self.two_pass_combine_mode()));
        } else {
            self.combine = flip(self.combine);
        }
    }

    /// The mask section a row belongs to and that `d` removes: the section a "Combined with" row
    /// comes before, or the section of one of its input, generator and generator-setting rows.
    /// `None` for a refinement step's rows (`d` removes the step) and for every other row.
    pub fn removable_section(row: &PipelineRow) -> Option<usize> {
        match *row {
            PipelineRow::MaskCombineOp { section }
            | PipelineRow::MaskOpInput { section }
            | PipelineRow::MaskOpGenerator { section }
            | PipelineRow::MaskOpGeneratorParam { section }
            | PipelineRow::MaskOpThresholdValue { section }
            | PipelineRow::MaskOpHdBetStep { section }
            | PipelineRow::MaskOpBetVoxelScale { section } => Some(section),
            PipelineRow::MaskOpAddStep { section } if section != MASK_COMBINED_SECTION => Some(section),
            _ => None,
        }
    }

    /// Whether `d` on `row` removes a section now: it belongs to one, and that mask has others.
    pub fn row_removes_section(&self, row: &PipelineRow) -> bool {
        Self::removable_section(row).is_some_and(|s| {
            let (two_pass, i) = Self::mask_section_slot(s);
            i < self.mask_section_count(two_pass) && self.mask_section_count(two_pass) > 1
        })
    }

    /// Remove the section `section` (main or reliable mask), unless it is its mask's only one.
    /// Returns whether it was removed.
    fn remove_section(&mut self, section: usize) -> bool {
        let (two_pass, i) = Self::mask_section_slot(section);
        let count = self.mask_section_count(two_pass);
        if count < 2 || i >= count {
            return false;
        }
        if two_pass { self.two_pass_sections.remove(i); } else { self.sections.remove(i); }
        true
    }

    /// The main mask's recipe in a few words, as Basic mode shows it in place of its editor:
    /// each section's input, generator and steps (`phase-quality › otsu threshold › dilate 1 ›
    /// fill-holes › erode 1`), wrapped to lines that fit beside the sidebar on an 80-column
    /// terminal.
    pub fn mask_recipe_summary(&self) -> Vec<String> {
        const WIDTH: usize = 46;
        let step = |op: &MaskOp| {
            let (kind, value) = Self::mask_op_label_value(op);
            match op {
                MaskOp::Threshold { .. } => format!("{value} threshold"),
                _ if value.parse::<f64>().is_ok() => format!("{kind} {value}"),
                _ => kind.to_string(),
            }
        };
        let mut words: Vec<String> = Vec::new();
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                words.push(format!("{}", self.combine).to_uppercase());
            }
            words.push(section.input.to_string());
            if section.has_generator() {
                words.push(step(&section.generator));
            }
            words.extend(section.refinements.iter().map(step));
        }
        if self.sections.len() > 1 {
            words.extend(self.refinements.iter().map(step));
        }
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        for (i, w) in words.iter().enumerate() {
            let joint = if i == 0 { "" } else if matches!(w.as_str(), "OR" | "AND") || i > 0 && matches!(words[i - 1].as_str(), "OR" | "AND") { " " } else { " › " };
            if !line.is_empty() && line.chars().count() + joint.chars().count() + w.chars().count() > WIDTH {
                lines.push(std::mem::take(&mut line));
                if joint == " › " {
                    line.push_str("› ");
                }
            } else {
                line.push_str(joint);
            }
            line.push_str(w);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    pub fn mask_op_label_value(op: &MaskOp) -> (&'static str, String) {
        match op {
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => ("threshold", "otsu".to_string()),
            MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } =>
                ("threshold", format!("fixed:{}", value.unwrap_or(0.5))),
            MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } =>
                ("threshold", format!("percentile:{}", value.unwrap_or(75.0))),
            MaskOp::Threshold { method: MaskThresholdMethod::StiGui, .. } =>
                ("threshold", "sti-gui".to_string()),
            MaskOp::Bet { fractional_intensity, voxel_scale } => ("bet", format!("{}{}", fractional_intensity,
                if (*voxel_scale - qsmxt_engine::bet_default_voxel_scale()).abs() > f64::EPSILON {
                    format!(" scale={voxel_scale}")
                } else {
                    String::new()
                })),
            MaskOp::Erode { iterations } => ("erode", format!("{}", iterations)),
            MaskOp::Dilate { iterations } => ("dilate", format!("{}", iterations)),
            MaskOp::Close { radius } => ("close", format!("{}", radius)),
            MaskOp::MinIntensity { value } => ("min-intensity", format!("{}", value)),
            MaskOp::Majority => ("majority", "3×3×3".to_string()),
            MaskOp::LargestComponent => ("largest-component", "26-connected".to_string()),
            MaskOp::Imfill => ("imfill", "all holes".to_string()),
            MaskOp::FillHoles { max_size } => ("fill-holes", if *max_size == 0 { "auto".to_string() } else { format!("{}", max_size) }),
            MaskOp::GaussianSmooth { sigma_mm } => ("gaussian", format!("{}", sigma_mm)),
            MaskOp::HdBet { patch, tta, tile_step } => {
                let step = if patch.is_some()
                    && (*tile_step - qsmxt_engine::hd_bet_default_tile_step()).abs() > f64::EPSILON {
                    format!(" step={tile_step}")
                } else {
                    String::new()
                };
                let inference = match patch {
                    None => "full".to_string(),
                    Some(p) => format!("{}x{}x{}", p[0], p[1], p[2]),
                };
                ("hd-bet", format!("{inference}{step}{}", if *tta { " tta" } else { "" }))
            }
            MaskOp::Rs2Net { tta, tile_step } => {
                let step = if (*tile_step - qsmxt_engine::rs2_net_default_tile_step()).abs() > f64::EPSILON {
                    format!(" step={tile_step}")
                } else {
                    String::new()
                };
                ("rs2-net", format!("rodent{}{}", step, if *tta { " tta" } else { "" }))
            }
            MaskOp::MindGrab { tta } => ("mindgrab", if *tta { "flip tta" } else { "default" }.to_string()),
            // Only the threshold: every other parameter has a row of its own below.
            MaskOp::SignalErode { threshold, .. } => ("signal-erode", format!("{:.2}", threshold)),
        }
    }

    /// Help for a mask step's row: what the value it shows does (the mask schema's wording),
    /// and how to change it.
    pub fn mask_op_help(op: &MaskOp) -> String {
        let step = mask_step_of(op);
        match op {
            MaskOp::Threshold { .. } => format!("{} — Enter: choose method", step.help),
            MaskOp::HdBet { .. } | MaskOp::Rs2Net { .. } | MaskOp::MindGrab { .. } =>
                format!("{} (needs a deep-learning build)", step.help),
            // Nothing to adjust.
            MaskOp::Majority | MaskOp::LargestComponent | MaskOp::Imfill => step.help.to_string(),
            // The row shows the step's first parameter (signal-erode's threshold); the rest have
            // rows of their own.
            _ => format!("{} (Enter to edit, +/- to nudge)", step.params.first().map_or(step.help, |p| p.help)),
        }
    }

    /// Create a default mask op for the given type name: the mask schema's default for the
    /// first step type of that name (a threshold is Otsu's).
    pub fn default_mask_op(type_name: &str) -> Option<MaskOp> {
        let step = mask_schema().steps.iter().find(|s| s.label == type_name)?;
        serde_json::from_value(step.default.clone()).ok()
    }

    /// Index of a section's generator in [`MASK_GENERATOR_TYPES`].
    pub fn mask_generator_index(&self, section: usize) -> usize {
        self.mask_section(section)
            .and_then(|s| MASK_GENERATOR_TYPES.iter().position(|&t| t == Self::mask_op_label_value(&s.generator).0))
            .unwrap_or(0)
    }

    /// HD-BET's sliding-window steps worth offering. qsm-core accepts any stride in `(0, 1]`:
    /// 0.5 is HD-BET's own default, and larger steps trade patch-seam quality for a shorter run.
    pub const HD_BET_STEPS: [f64; 4] = [0.5, 0.625, 0.75, 1.0];

    /// BET's voxel-size scalings worth offering. Any positive factor is valid, but only two
    /// matter in practice: 1.0 for clinical data at its own geometry, and 10.0 for rodent brains,
    /// which BET's millimetre-scale surface model otherwise cannot find. The steps between are
    /// there for the in-between field strengths.
    pub const BET_VOXEL_SCALES: [f64; 4] = [1.0, 2.0, 5.0, 10.0];

    /// The values a list of offered `choices` shows for `current`: the offered ones, with
    /// `current` added in order when it is not one of them (a value from a loaded config), and
    /// the index of `current`.
    fn value_choices(choices: &[f64], current: f64) -> (Vec<f64>, usize) {
        let mut values = choices.to_vec();
        if !values.iter().any(|v| (v - current).abs() < 1e-9) {
            values.push(current);
            values.sort_by(f64::total_cmp);
        }
        let at = values.iter().position(|v| (v - current).abs() < 1e-9).unwrap_or(0);
        (values, at)
    }

    /// The HD-BET patch steps offered for a section's generator, and the current one's index.
    pub fn hd_bet_step_choices(&self, section: usize) -> (Vec<f64>, usize) {
        let current = match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::HdBet { tile_step, .. }) => *tile_step,
            _ => qsmxt_engine::hd_bet_default_tile_step(),
        };
        Self::value_choices(&Self::HD_BET_STEPS, current)
    }

    /// Set a section's HD-BET patch step (no change unless its generator is HD-BET).
    fn set_hd_bet_tile_step(&mut self, section: usize, step: f64) -> bool {
        let Some(MaskOp::HdBet { tile_step, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *tile_step = step;
        true
    }

    /// The BET voxel scalings offered for a section's generator, and the current one's index.
    pub fn bet_voxel_scale_choices(&self, section: usize) -> (Vec<f64>, usize) {
        let current = match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::Bet { voxel_scale, .. }) => *voxel_scale,
            _ => qsmxt_engine::bet_default_voxel_scale(),
        };
        Self::value_choices(&Self::BET_VOXEL_SCALES, current)
    }

    /// Set a section's BET voxel scaling (no change unless its generator is BET).
    fn set_bet_voxel_scale(&mut self, section: usize, scale: f64) -> bool {
        let Some(MaskOp::Bet { voxel_scale, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *voxel_scale = scale;
        true
    }

    /// HD-BET's inference choices (whole volume, native sliding window, low-memory window), in
    /// the order of the mask schema's `hd_bet_inference`.
    pub fn hd_bet_inference_patches() -> [Option<[usize; 3]>; 3] {
        [None, Some(MaskOp::hd_bet_default_patch()), Some(qsmxt_engine::hd_bet_low_memory_patch())]
    }

    /// The index of a section's HD-BET inference in [`Self::hd_bet_inference_patches`].
    pub fn hd_bet_inference_index(&self, section: usize) -> usize {
        match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::HdBet { patch, .. }) => Self::hd_bet_inference_patches().iter().position(|p| p == patch).unwrap_or(0),
            _ => 0,
        }
    }

    /// Set a section's HD-BET inference by index into [`Self::hd_bet_inference_patches`].
    fn set_hd_bet_inference(&mut self, section: usize, idx: usize) -> bool {
        let Some(&new) = Self::hd_bet_inference_patches().get(idx) else { return false };
        let Some(MaskOp::HdBet { patch, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *patch = new;
        true
    }

    /// Turn a section's MindGrab test-time augmentation on or off (no change for another
    /// generator).
    fn toggle_mindgrab_tta(&mut self, section: usize) -> bool {
        let Some(MaskOp::MindGrab { tta }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *tta = !*tta;
        true
    }

    /// Ordered masking input sources, matching the modal/cycle option order.
    pub const MASK_INPUT_SOURCES: [MaskingInput; 4] = {
        use MaskingInput::*;
        [MagnitudeFirst, Magnitude, MagnitudeLast, PhaseQuality]
    };

    /// Current masking-input source index for a section (for populating the modal).
    pub fn mask_input_index(&self, section: usize) -> usize {
        self.mask_section(section)
            .and_then(|s| Self::MASK_INPUT_SOURCES.iter().position(|x| *x == s.input))
            .unwrap_or(0)
    }

    /// Set a section's masking input source by index (from the modal).
    fn set_mask_input(&mut self, section: usize, idx: usize) -> bool {
        let Some(&src) = Self::MASK_INPUT_SOURCES.get(idx) else { return false };
        let Some(section_ref) = self.mask_section_mut(section) else { return false };
        section_ref.input = src;
        true
    }

    /// Set a section's generator algorithm by index into [`MASK_GENERATOR_TYPES`]. Only rewrites
    /// the generator when the algorithm type actually changes, preserving existing parameters.
    fn set_mask_generator(&mut self, section: usize, idx: usize) -> bool {
        if self.mask_section(section).is_none() || idx == self.mask_generator_index(section) { return false; }
        let Some(op) = MASK_GENERATOR_TYPES.get(idx).and_then(|t| Self::default_mask_op(t)) else { return false };
        if let Some(section_ref) = self.mask_section_mut(section) {
            section_ref.generator = op;
        }
        true
    }

    /// Ordered threshold methods, matching the modal/cycle option order.
    pub const MASK_THRESHOLD_METHODS: [MaskThresholdMethod; 4] = {
        use MaskThresholdMethod::*;
        [Otsu, Fixed, Percentile, StiGui]
    };

    /// Current threshold-method index for a section (0 if the generator isn't a threshold).
    pub fn mask_threshold_method_index(&self, section: usize) -> usize {
        match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::Threshold { method, .. }) => {
                Self::MASK_THRESHOLD_METHODS.iter().position(|m| m == method).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Set a section's threshold method by index (from the modal). No change unless the
    /// generator is a threshold; the existing threshold value is preserved.
    fn set_mask_threshold_method(&mut self, section: usize, idx: usize) -> bool {
        let Some(&new_method) = Self::MASK_THRESHOLD_METHODS.get(idx) else { return false };
        let Some(MaskOp::Threshold { method, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *method = new_method;
        true
    }

    /// Set the method of a refinement step that is a threshold (from a loaded config), by index
    /// into [`Self::MASK_THRESHOLD_METHODS`].
    fn set_refinement_threshold_method(&mut self, section: usize, index: usize, idx: usize) -> bool {
        let Some(&new_method) = Self::MASK_THRESHOLD_METHODS.get(idx) else { return false };
        let Some(MaskOp::Threshold { method, .. }) = self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return false };
        *method = new_method;
        true
    }

    /// The number a mask row edits, as text to start editing from, when the row edits a number:
    /// a threshold's value, BET's fractional intensity, a refinement step's value, or one of
    /// signal-erode's settings. `None` for every other row (selectors and switches).
    pub fn mask_value_text(&self, row: &PipelineRow) -> Option<String> {
        match *row {
            PipelineRow::MaskOpThresholdValue { section } => match self.mask_section(section).map(|s| &s.generator) {
                Some(MaskOp::Threshold { value, .. }) => Some(value.map(|v| format!("{v}")).unwrap_or_default()),
                _ => None,
            },
            PipelineRow::MaskOpGeneratorParam { section } => match self.mask_section(section).map(|s| &s.generator) {
                Some(MaskOp::Bet { fractional_intensity, .. }) => Some(format!("{fractional_intensity}")),
                _ => None,
            },
            PipelineRow::MaskOpEntry { section, index } => match self.mask_refinements(section)?.get(index)? {
                MaskOp::Bet { fractional_intensity, .. } => Some(format!("{fractional_intensity}")),
                MaskOp::Erode { iterations } | MaskOp::Dilate { iterations } => Some(format!("{iterations}")),
                MaskOp::Close { radius } => Some(format!("{radius}")),
                MaskOp::MinIntensity { value } => Some(format!("{value}")),
                MaskOp::FillHoles { max_size } => Some(format!("{max_size}")),
                MaskOp::GaussianSmooth { sigma_mm } => Some(format!("{sigma_mm}")),
                MaskOp::SignalErode { threshold, .. } => Some(format!("{threshold}")),
                _ => None,
            },
            PipelineRow::MaskOpSignalErodeParam { section, index, param } => match self.mask_refinements(section)?.get(index)? {
                MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. } => Some(match param {
                    0 => format!("{depth_cap}"),
                    1 => format!("{global_erosions}"),
                    2 => format!("{bias_sigma}"),
                    _ => format!("{min_component}"),
                }),
                _ => None,
            },
            _ => None,
        }
    }

    /// Nudge the number a mask row edits by one step up (`delta` 1, `+`) or down (-1, `-`), within
    /// its range: BET's fractional intensity by 0.05 (0.1 for a refinement step), an iteration
    /// count or radius by 1, a minimum intensity by a tenth of its order of magnitude, the
    /// largest hole by 100, a smoothing σ by 0.5 mm, signal-erode's threshold by 0.05 and its
    /// settings by 1 (the smallest component by 100). Returns whether the row has such a number.
    fn nudge_mask_value(&mut self, row: &PipelineRow, delta: isize) -> bool {
        let d = delta as f64;
        let step_usize = |v: &mut usize, by: isize, min: usize| *v = (*v as isize + delta * by).max(min as isize) as usize;
        match *row {
            PipelineRow::MaskOpGeneratorParam { section } => {
                let Some(MaskOp::Bet { fractional_intensity, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
                *fractional_intensity = ((*fractional_intensity + d * 0.05).clamp(0.05, 1.0) * 100.0).round() / 100.0;
            }
            PipelineRow::MaskOpEntry { section, index } => {
                let Some(op) = self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return false };
                match op {
                    MaskOp::Bet { fractional_intensity, .. } => *fractional_intensity = ((*fractional_intensity + d * 0.1).clamp(0.1, 1.0) * 10.0).round() / 10.0,
                    MaskOp::Erode { iterations } | MaskOp::Dilate { iterations } => step_usize(iterations, 1, 1),
                    MaskOp::Close { radius } => step_usize(radius, 1, 1),
                    MaskOp::MinIntensity { value } => *value = step_min_intensity(*value, delta),
                    MaskOp::FillHoles { max_size } => step_usize(max_size, 100, 0),
                    MaskOp::GaussianSmooth { sigma_mm } => *sigma_mm = (*sigma_mm + d * 0.5).max(0.5),
                    MaskOp::SignalErode { threshold, .. } => *threshold = ((*threshold + d * 0.05).clamp(0.05, 0.95) * 100.0).round() / 100.0,
                    _ => return false,
                }
            }
            PipelineRow::MaskOpSignalErodeParam { section, index, param } => {
                let Some(MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. }) =
                    self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return false };
                match param {
                    0 => step_usize(depth_cap, 1, 0),
                    1 => step_usize(global_erosions, 1, 0),
                    // Voxels, like qsm-core's `bias_sigma`; 0 would divide out nothing.
                    2 => *bias_sigma = (*bias_sigma + d).max(1.0),
                    _ => step_usize(min_component, 100, 0),
                }
            }
            _ => return false,
        }
        true
    }

    /// Set the number a mask row edits ([`Self::mask_value_text`]) from typed text, and whether
    /// it did. A threshold value left empty goes back to its default; any other text that is not
    /// a number in the setting's range is refused with why, and nothing changes.
    fn set_mask_value(&mut self, row: &PipelineRow, text: &str) -> Result<bool, String> {
        let text = text.trim();
        let real = |what: &str, ok: &dyn Fn(f64) -> bool, range: &str| -> Result<f64, String> {
            text.parse::<f64>().ok().filter(|v| v.is_finite() && ok(*v))
                .ok_or_else(|| format!("{what} is a number {range}, not \"{text}\""))
        };
        let count = |what: &str, min: usize| -> Result<usize, String> {
            text.parse::<usize>().ok().filter(|v| *v >= min)
                .ok_or_else(|| format!("{what} is a whole number of at least {min}, not \"{text}\""))
        };
        let fraction = |what: &str| real(what, &|v| v > 0.0 && v <= 1.0, "above 0 and at most 1");
        match *row {
            PipelineRow::MaskOpThresholdValue { section } => {
                let value = if text.is_empty() { None } else { Some(real("The threshold", &|_| true, "")?) };
                match self.mask_section_mut(section).map(|s| &mut s.generator) {
                    Some(MaskOp::Threshold { value: v, .. }) => *v = value,
                    _ => return Ok(false),
                }
            }
            PipelineRow::MaskOpGeneratorParam { section } => {
                let v = fraction("BET's fractional intensity")?;
                match self.mask_section_mut(section).map(|s| &mut s.generator) {
                    Some(MaskOp::Bet { fractional_intensity, .. }) => *fractional_intensity = v,
                    _ => return Ok(false),
                }
            }
            PipelineRow::MaskOpEntry { section, index } => {
                let Some(op) = self.mask_refinements(section).and_then(|r| r.get(index)).cloned() else { return Ok(false) };
                let new = match op {
                    MaskOp::Bet { voxel_scale, .. } => MaskOp::Bet { fractional_intensity: fraction("BET's fractional intensity")?, voxel_scale },
                    MaskOp::Erode { .. } => MaskOp::Erode { iterations: count("Erode's iterations", 1)? },
                    MaskOp::Dilate { .. } => MaskOp::Dilate { iterations: count("Dilate's iterations", 1)? },
                    MaskOp::Close { .. } => MaskOp::Close { radius: count("Close's radius", 1)? },
                    MaskOp::MinIntensity { .. } => MaskOp::MinIntensity { value: real("The minimum intensity", &|v| v > 0.0, "above 0")? },
                    MaskOp::FillHoles { .. } => MaskOp::FillHoles { max_size: count("The largest hole (0: any size)", 0)? },
                    MaskOp::GaussianSmooth { .. } => MaskOp::GaussianSmooth { sigma_mm: real("The smoothing σ", &|v| v > 0.0, "above 0")? },
                    MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. } => MaskOp::SignalErode {
                        threshold: real("Signal-erode's threshold", &|v| v > 0.0 && v < 1.0, "between 0 and 1")?,
                        depth_cap, global_erosions, bias_sigma, min_component,
                    },
                    _ => return Ok(false),
                };
                if let Some(op) = self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) { *op = new; }
            }
            PipelineRow::MaskOpSignalErodeParam { section, index, param } => {
                let label = signal_erode_params().get(param).map_or("The setting", |p| p.0);
                let Some(MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. }) =
                    self.mask_refinements(section).and_then(|r| r.get(index)).cloned() else { return Ok(false) };
                let (mut d, mut g, mut b, mut m) = (depth_cap, global_erosions, bias_sigma, min_component);
                match param {
                    0 => d = count(label, 0)?,
                    1 => g = count(label, 0)?,
                    // Voxels, like qsm-core's `bias_sigma`; 0 would divide out nothing.
                    2 => b = real(label, &|v| v > 0.0, "above 0")?,
                    _ => m = count(label, 0)?,
                }
                if let Some(MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. }) =
                    self.mask_refinements_mut(section).and_then(|r| r.get_mut(index))
                {
                    (*depth_cap, *global_erosions, *bias_sigma, *min_component) = (d, g, b, m);
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Label and current value of one extra signal-erode parameter, for its own row.
    pub fn signal_erode_param(&self, section: usize, index: usize, param: usize) -> Option<(&'static str, String)> {
        let MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. } =
            self.mask_refinements(section)?.get(index)? else { return None };
        let (label, _) = *signal_erode_params().get(param)?;
        let value = match param {
            0 => if *depth_cap == 0 { "off (no cap)".to_string() } else { format!("{depth_cap}") },
            1 => format!("{global_erosions}"),
            2 => format!("{bias_sigma:.1}"),
            _ => format!("{min_component}"),
        };
        Some((label, value))
    }

    /// The step types offered for adding refinement steps (morphological only): a section's
    /// generator is chosen on its own row.
    pub fn available_op_types() -> Vec<&'static str> {
        MASK_OP_TYPES.iter()
            .filter(|t| !MASK_GENERATOR_TYPES.contains(t))
            .copied()
            .collect()
    }
}

/// A minimum intensity one `+`/`-` nudge on: by a tenth of its order of magnitude (100 → 110 or
/// 90, 0.5 → 0.55 or 0.45), so a magnitude in any units can be adjusted. Stays above 0.
fn step_min_intensity(value: f64, delta: isize) -> f64 {
    // Going down from a power of ten steps by the smaller order (100 → 99, not 90 → 0 eventually).
    let base = if delta < 0 { value * (1.0 - 1e-9) } else { value };
    let step = 10f64.powf(base.max(f64::MIN_POSITIVE).log10().floor() - 1.0);
    let next = value + delta as f64 * step;
    // Round to the step, so repeated steps do not accumulate floating-point noise.
    let next = (next / step).round() * step;
    if next > 0.0 { next } else { value }
}

/// A part of a recipe as the store holds it.
fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or_default()
}

impl App {
    /// The mask recipes, as the settings store holds them.
    pub fn mask(&self) -> MaskRecipe {
        MaskRecipe::of(&self.form.settings)
    }

    /// Change the mask recipes with `edit`, which says whether it changed them. A change to the
    /// main mask (`section` is one of its sections, or the combined mask) leaves the preset on
    /// "custom".
    pub fn edit_mask(&mut self, section: usize, edit: impl FnOnce(&mut MaskRecipe) -> bool) -> bool {
        let mut m = self.mask();
        if !edit(&mut m) {
            return false;
        }
        m.store(&mut self.form.settings);
        self.mark_mask_custom(section);
        true
    }

    /// Mark the preset as "Custom" when the user edits the main mask by hand.
    ///
    /// The preset names the main mask's recipe, so editing the two-pass reliable mask leaves it
    /// alone — otherwise turning two-pass on and adjusting its threshold would appear to abandon
    /// the `robust-threshold` preset the main mask still follows.
    pub(super) fn mark_mask_custom(&mut self, section: usize) {
        if !is_two_pass_section(section) {
            self.pipeline_state.mask_preset = *MASK_PRESET_CUSTOM;
        }
    }

    /// Apply a mask preset, overwriting the main mask's whole recipe (sections, combine mode and
    /// the refinements that run on the combined mask). The trailing "custom" entry leaves the
    /// recipe alone.
    pub fn apply_mask_preset(&mut self, preset: usize) {
        let Some((_, recipe)) = qsmxt_engine::mask_presets().into_iter().nth(preset) else { return };
        let mut m = self.mask();
        m.sections = recipe.sections;
        m.combine = recipe.combine;
        m.refinements = recipe.refinements;
        m.store(&mut self.form.settings);
    }

    /// Choose a preset on its picker: the preset, and its recipe.
    pub(super) fn choose_mask_preset(&mut self, preset: usize) {
        self.pipeline_state.mask_preset = preset;
        self.apply_mask_preset(preset);
    }

    /// The recipes back to the form's defaults, and the preset with them.
    pub(super) fn reset_mask(&mut self) {
        for p in RECIPE_PATHS {
            self.form.settings.reset(p, false);
        }
        self.pipeline_state.mask_preset = PipelineFormState::default().mask_preset;
    }

    /// Remove the section `section` (main or reliable mask), unless it is its mask's only one.
    /// Returns whether it was removed.
    pub fn remove_mask_section(&mut self, section: usize) -> bool {
        if !self.edit_mask(section, |m| m.remove_section(section)) {
            return false;
        }
        let max = self.focusable_rows().len().saturating_sub(1);
        if self.focus() > max { self.set_focus(max); }
        true
    }

    /// Set how the sections of the mask `section` belongs to combine.
    pub fn set_combine(&mut self, section: usize, mode: MaskCombine) {
        self.edit_mask(section, |m| {
            let change = m.combine_mode(section) != mode;
            if change { m.toggle_combine(section) }
            change
        });
    }

    pub fn set_hd_bet_tile_step(&mut self, section: usize, step: f64) {
        self.edit_mask(section, |m| m.set_hd_bet_tile_step(section, step));
    }

    pub fn set_bet_voxel_scale(&mut self, section: usize, scale: f64) {
        self.edit_mask(section, |m| m.set_bet_voxel_scale(section, scale));
    }

    pub fn set_hd_bet_inference(&mut self, section: usize, idx: usize) {
        self.edit_mask(section, |m| m.set_hd_bet_inference(section, idx));
    }

    pub fn toggle_mindgrab_tta(&mut self, section: usize) {
        self.edit_mask(section, |m| m.toggle_mindgrab_tta(section));
    }

    pub fn set_mask_input(&mut self, section: usize, idx: usize) {
        self.edit_mask(section, |m| m.set_mask_input(section, idx));
    }

    pub fn set_mask_generator(&mut self, section: usize, idx: usize) {
        self.edit_mask(section, |m| m.set_mask_generator(section, idx));
    }

    pub fn set_mask_threshold_method(&mut self, section: usize, idx: usize) {
        self.edit_mask(section, |m| m.set_mask_threshold_method(section, idx));
    }

    pub fn set_refinement_threshold_method(&mut self, section: usize, index: usize, idx: usize) {
        self.edit_mask(section, |m| m.set_refinement_threshold_method(section, index, idx));
    }

    /// [`MaskRecipe::nudge_mask_value`] on the recipes.
    pub fn nudge_mask_value(&mut self, row: &PipelineRow, delta: isize) -> bool {
        let Some(section) = mask_row_section(row) else { return false };
        self.edit_mask(section, |m| m.nudge_mask_value(row, delta))
    }

    /// [`MaskRecipe::set_mask_value`] on the recipes.
    pub fn set_mask_value(&mut self, row: &PipelineRow, text: &str) -> Result<(), String> {
        let Some(section) = mask_row_section(row) else { return Ok(()) };
        let mut m = self.mask();
        if m.set_mask_value(row, text)? {
            self.edit_mask(section, |r| { *r = m; true });
        }
        Ok(())
    }

    /// Add a refinement step of type `kind` to the end of a refinement list.
    pub(super) fn add_mask_step(&mut self, section: usize, kind: &str) {
        let Some(op) = MaskRecipe::default_mask_op(kind) else { return };
        self.edit_mask(section, |m| {
            if let Some(refs) = m.mask_refinements_mut(section) { refs.push(op); }
            true
        });
    }

    /// Add a section (a threshold of the magnitude) to the main mask or the reliable mask.
    pub(super) fn add_mask_section(&mut self, two_pass: bool) {
        let section = MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        };
        let m = self.mask();
        let added = if two_pass { MASK_TWO_PASS_BASE + m.two_pass_sections.len() } else { m.sections.len() };
        self.edit_mask(added, |m| {
            if two_pass { m.two_pass_sections.push(section) } else { m.sections.push(section) }
            true
        });
    }

    /// Move the refinement step `index` of a list one place up (`up`) or down; whether it moved.
    pub(super) fn move_mask_step(&mut self, section: usize, index: usize, up: bool) -> bool {
        self.edit_mask(section, |m| {
            let Some(refs) = m.mask_refinements_mut(section) else { return false };
            let to = if up { index.checked_sub(1) } else { Some(index + 1) };
            match to {
                Some(to) if index < refs.len() && to < refs.len() => {
                    refs.swap(index, to);
                    true
                }
                _ => false,
            }
        })
    }

    /// Remove the refinement step `index` of a list; whether there was one.
    pub(super) fn remove_mask_step(&mut self, section: usize, index: usize) -> bool {
        self.edit_mask(section, |m| match m.mask_refinements_mut(section) {
            Some(refs) if index < refs.len() => {
                refs.remove(index);
                true
            }
            _ => false,
        })
    }

    /// Preprocessing → Masking: the mask recipe, its custom masks, the magnitude correction, and
    /// the two-pass reliable mask. The settings that are not the recipe show while the registry
    /// says they apply to `config`.
    pub(super) fn masking_rows(&self, config: &PipelineConfig) -> Vec<PipelineRow> {
        let visible = self.visible_settings(config);
        let shows = |path: &str| visible.contains(path);
        let mask = self.mask();
        let mut rows = vec![PipelineRow::AlgoSelect {
            label: "Mask preset", field: "mask_preset",
            options: &MASK_PRESET_OPTIONS, help: mask_preset_help(),
        }];
        if shows("masking.custom_mask_tool") {
            self.push_rows(&mut rows, setting("masking.custom_mask_tool"), 0);
        }

        self.push_mask_section_rows(&mask, &mut rows, false, &shows);
        rows.push(PipelineRow::MaskOpAddSection { two_pass: false });
        // Steps that run on the combined mask — only meaningful with more than one section.
        if shows(REFINEMENTS) {
            rows.push(PipelineRow::MaskCombinedHeader);
            push_refinement_rows(&mask, &mut rows, MASK_COMBINED_SECTION);
            rows.push(PipelineRow::MaskOpAddStep { section: MASK_COMBINED_SECTION });
        }
        // BET only as a refinement step has no generator block to sit under.
        if mask.first_bet_generator_section().is_none() {
            self.push_bet_surface_rows(&mut rows, &shows);
        }

        // The inhomogeneity correction of the magnitude masking reads (and the combined magnitude
        // is made from), with its method's settings under it.
        let under_correction = |mut s: &'static schema::Setting| loop {
            if s.path == "masking.inhomogeneity_correction" {
                return true;
            }
            match s.parent {
                Some(p) => s = setting(p),
                None => return false,
            }
        };
        for s in schema::settings().iter().filter(|s| s.section == Section::Masking && under_correction(s) && shows(s.path)) {
            self.push_rows(&mut rows, s, depth_of(s));
        }

        // Two-pass artefact reduction. Configured here because what it needs is a second mask;
        // what it does is repeat background removal and dipole inversion against it.
        if shows(TWO_PASS) {
            self.push_rows(&mut rows, setting(TWO_PASS), 0);
        }
        if shows(TWO_PASS_SECTIONS) {
            rows.push(PipelineRow::MaskTwoPassHeader);
            self.push_mask_section_rows(&mask, &mut rows, true, &shows);
            rows.push(PipelineRow::MaskOpAddSection { two_pass: true });
            if shows("masking.two_pass_mode") {
                self.push_rows(&mut rows, setting("masking.two_pass_mode"), 0);
            }
        }
        rows
    }

    /// Rows for every section of one mask — the main mask, or the two-pass reliable mask.
    ///
    /// Both get the identical row set; only the section index differs (see [`MASK_TWO_PASS_BASE`]),
    /// which is what keeps the reliable mask editable in exactly the ways the main mask is.
    fn push_mask_section_rows(&self, mask: &MaskRecipe, rows: &mut Vec<PipelineRow>, two_pass: bool, shows: &dyn Fn(&str) -> bool) {
        let count = mask.mask_section_count(two_pass);
        let base = if two_pass { MASK_TWO_PASS_BASE } else { 0 };
        for i in 0..count {
            let si = base + i;
            if i > 0 {
                rows.push(PipelineRow::MaskCombineOp { section: si });
            }
            if count > 1 {
                rows.push(PipelineRow::MaskSectionHeader { section: si });
            }
            rows.push(PipelineRow::MaskOpInput { section: si });
            rows.push(PipelineRow::MaskOpGenerator { section: si });
            rows.push(PipelineRow::MaskOpGeneratorParam { section: si });
            let generator = mask.mask_section(si).map(|s| &s.generator);
            if let Some(MaskOp::Threshold { method, .. }) = generator {
                if matches!(method, MaskThresholdMethod::Fixed | MaskThresholdMethod::Percentile) {
                    rows.push(PipelineRow::MaskOpThresholdValue { section: si });
                }
            }
            if matches!(generator, Some(MaskOp::HdBet { patch: Some(_), .. })) {
                rows.push(PipelineRow::MaskOpHdBetStep { section: si });
            }
            if matches!(generator, Some(MaskOp::Bet { .. })) {
                rows.push(PipelineRow::MaskOpBetVoxelScale { section: si });
                if mask.first_bet_generator_section() == Some(si) {
                    self.push_bet_surface_rows(rows, shows);
                }
            }
            push_refinement_rows(mask, rows, si);
            rows.push(PipelineRow::MaskOpAddStep { section: si });
        }
    }

    /// BET's surface settings ([`BET_SURFACE`]). They are listed under the first BET step with
    /// that step's own settings, so all of BET's settings sit together, as settings of the page
    /// rather than under the recipe the registry lists them under.
    fn push_bet_surface_rows(&self, rows: &mut Vec<PipelineRow>, shows: &dyn Fn(&str) -> bool) {
        for path in BET_SURFACE.iter().filter(|p| shows(p)) {
            self.push_rows(rows, setting(path), 0);
        }
    }
}

/// Rows for one refinement list: the step itself, plus a row per extra parameter for the
/// steps that have more than one (signal-erode).
fn push_refinement_rows(mask: &MaskRecipe, rows: &mut Vec<PipelineRow>, section: usize) {
    let Some(refinements) = mask.mask_refinements(section) else { return };
    for (index, op) in refinements.iter().enumerate() {
        rows.push(PipelineRow::MaskOpEntry { section, index });
        if matches!(op, MaskOp::SignalErode { .. }) {
            for param in 0..signal_erode_params().len() {
                rows.push(PipelineRow::MaskOpSignalErodeParam { section, index, param });
            }
        }
    }
}

/// The section of the mask a row with a number edits.
fn mask_row_section(row: &PipelineRow) -> Option<usize> {
    match *row {
        PipelineRow::MaskOpThresholdValue { section } | PipelineRow::MaskOpGeneratorParam { section }
        | PipelineRow::MaskOpEntry { section, .. } | PipelineRow::MaskOpSignalErodeParam { section, .. } => Some(section),
        _ => None,
    }
}

#[cfg(test)]
impl App {
    /// The mask recipes to change in a test, written back when the guard is dropped. The preset
    /// is left as it is, as a recipe set from a config leaves it.
    pub fn mask_mut(&mut self) -> MaskGuard<'_> {
        let mask = self.mask();
        MaskGuard { app: self, mask }
    }
}

/// See [`App::mask_mut`].
#[cfg(test)]
pub struct MaskGuard<'a> {
    app: &'a mut App,
    mask: MaskRecipe,
}

#[cfg(test)]
impl std::ops::Deref for MaskGuard<'_> {
    type Target = MaskRecipe;
    fn deref(&self) -> &MaskRecipe {
        &self.mask
    }
}

#[cfg(test)]
impl std::ops::DerefMut for MaskGuard<'_> {
    fn deref_mut(&mut self) -> &mut MaskRecipe {
        &mut self.mask
    }
}

#[cfg(test)]
impl Drop for MaskGuard<'_> {
    fn drop(&mut self) {
        self.mask.store(&mut self.app.form.settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::command::config_from_app;

    /// The recipe the editor edits is the config's: an edit reaches the config, marks the preset
    /// custom, and a fresh form set to that config holds the same recipe.
    #[test]
    fn the_recipe_is_the_configs() {
        let mut app = App::new_for_trial();
        app.set_field_select("mask_preset", 3); // BET and phase quality: two sections, AND
        app.add_mask_step(MASK_COMBINED_SECTION, "erode");
        assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM);
        let m = app.mask();
        let c = config_from_app(&app).unwrap();
        assert_eq!((&c.masking.sections, c.masking.combine, &c.masking.refinements), (&m.sections, m.combine, &m.refinements));
        assert_eq!(c.masking.refinements.last(), Some(&MaskOp::Erode { iterations: 1 }));
        let mut fresh = App::new_for_trial();
        fresh.set_form_from_config(&c);
        assert_eq!(fresh.mask(), m);
    }

    /// The combine mode and the combined mask's steps apply only with more than one section: with
    /// one, the form keeps them but the run does not take them.
    #[test]
    fn a_single_sections_combine_settings_are_kept_but_not_run() {
        let mut app = App::new_for_trial();
        app.set_field_select("mask_preset", 3);
        app.add_mask_step(MASK_COMBINED_SECTION, "erode");
        assert!(app.remove_mask_section(1));
        assert_eq!(app.mask().refinements.last(), Some(&MaskOp::Erode { iterations: 1 }), "kept");
        let c = config_from_app(&app).unwrap();
        assert!(c.masking.refinements.is_empty(), "not run");
    }

    /// The run takes the recipe as the command line spells it, as it always has from the form: a
    /// fixed threshold without a value is the default one, BET's fractional intensity two places.
    #[test]
    fn the_run_takes_the_recipe_as_the_command_line_spells_it() {
        let mut app = App::new_for_trial();
        app.set_mask_threshold_method(0, 1);
        assert!(matches!(app.mask().sections[0].generator, MaskOp::Threshold { value: None, .. }));
        let generator = |app: &App| config_from_app(app).unwrap().masking.sections[0].generator.clone();
        assert_eq!(generator(&app), MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.5) });
        app.set_mask_generator(0, MASK_GENERATOR_TYPES.iter().position(|t| *t == "bet").unwrap());
        app.set_mask_value(&PipelineRow::MaskOpGeneratorParam { section: 0 }, "0.123").unwrap();
        assert_eq!(app.mask().mask_value_text(&PipelineRow::MaskOpGeneratorParam { section: 0 }).as_deref(), Some("0.123"));
        assert!(matches!(generator(&app), MaskOp::Bet { fractional_intensity, .. } if fractional_intensity == 0.12));
    }

    /// The Masking page's settings rows refuse unreadable text while they apply, as the
    /// registry's rows elsewhere do.
    #[test]
    fn unreadable_masking_values_are_refused() {
        let mut app = App::new_for_trial();
        *app.field_text_mut("custom_mask_tool").unwrap() = " bet ".into();
        assert_eq!(config_from_app(&app).unwrap().masking.custom_mask_tool.as_deref(), Some("bet"));
        *app.field_text_mut("homogeneity_sigma_mm").unwrap() = "wide".into();
        let err = config_from_app(&app).expect_err("refused").to_string();
        assert!(err.contains("--homogeneity-sigma-mm wide: not a number"), "{err}");
    }
}
