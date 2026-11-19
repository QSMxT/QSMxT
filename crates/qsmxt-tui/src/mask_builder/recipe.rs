//! Reading and writing the recipes, their sections and how they combine.

use super::*;

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
    pub(super) fn store(&self, st: &mut SettingsStore) {
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
    pub(super) fn first_bet_generator_section(&self) -> Option<usize> {
        let is_bet = |s: &MaskSection| matches!(s.generator, MaskOp::Bet { .. });
        self.sections.iter().position(is_bet).or_else(|| {
            if !self.two_pass { return None; }
            self.two_pass_sections.iter().position(is_bet).map(|i| MASK_TWO_PASS_BASE + i)
        })
    }

    /// The main mask's combine mode as the pipeline sees it: only editable — and only shown —
    /// with more than one section, so a single-section recipe reports the default.
    pub(super) fn main_combine(&self) -> MaskCombine {
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
    pub(super) fn remove_section(&mut self, section: usize) -> bool {
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
}
