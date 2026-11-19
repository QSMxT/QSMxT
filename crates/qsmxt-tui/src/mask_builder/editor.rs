//! The editor on the form: edits of the recipe, and the Masking page's rows.

use super::*;

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
    pub(crate) fn mark_mask_custom(&mut self, section: usize) {
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
    pub(crate) fn choose_mask_preset(&mut self, preset: usize) {
        self.pipeline_state.mask_preset = preset;
        self.apply_mask_preset(preset);
    }

    /// The recipes back to the form's defaults, and the preset with them.
    pub(crate) fn reset_mask(&mut self) {
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
    pub(crate) fn add_mask_step(&mut self, section: usize, kind: &str) {
        let Some(op) = MaskRecipe::default_mask_op(kind) else { return };
        self.edit_mask(section, |m| {
            if let Some(refs) = m.mask_refinements_mut(section) { refs.push(op); }
            true
        });
    }

    /// Add a section (a threshold of the magnitude) to the main mask or the reliable mask.
    pub(crate) fn add_mask_section(&mut self, two_pass: bool) {
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
    pub(crate) fn move_mask_step(&mut self, section: usize, index: usize, up: bool) -> bool {
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
    pub(crate) fn remove_mask_step(&mut self, section: usize, index: usize) -> bool {
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
    pub(crate) fn masking_rows(&self, config: &PipelineConfig) -> Vec<PipelineRow> {
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
    pub(super) fn push_mask_section_rows(&self, mask: &MaskRecipe, rows: &mut Vec<PipelineRow>, two_pass: bool, shows: &dyn Fn(&str) -> bool) {
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
    pub(super) fn push_bet_surface_rows(&self, rows: &mut Vec<PipelineRow>, shows: &dyn Fn(&str) -> bool) {
        for path in BET_SURFACE.iter().filter(|p| shows(p)) {
            self.push_rows(rows, setting(path), 0);
        }
    }
}

/// Rows for one refinement list: the step itself, plus a row per extra parameter for the
/// steps that have more than one (signal-erode).
pub(super) fn push_refinement_rows(mask: &MaskRecipe, rows: &mut Vec<PipelineRow>, section: usize) {
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
pub(super) fn mask_row_section(row: &PipelineRow) -> Option<usize> {
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
