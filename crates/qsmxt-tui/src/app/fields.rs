//! Named field access: the pickers' options, and reading, setting and resetting a field.

use super::*;

/// The supplied segmentation's tool: whether one is named is the segmentation's source.
const DSEG: &str = "segmentation.custom_dseg_tool";

pub const QSM_ALGO_OPTIONS: &[&str] = &["rts", "tv", "tkd", "tsvd", "tgv", "tikhonov", "nltv", "medi", "tfi", "ilsqr", "star", "lsqr", "qsmart", "ndi", "fansi", "fansi-tgv", "l1qsm", "whqsm", "hdqsm", "amp-pe", "xqsm", "qsmnet", "qsmnet-plus", "qsmnetme", "autoqsm", "qsmgan", "ir2qsm", "lpcnn", "modl-qsm", "nextqsm", "iqsm", "iqsm-plus"];
// QSMART's inner dipole inversion (excludes the two end-to-end algorithms tgv/qsmart).
/// The inversion picker's alias for LSQR with the HEIDI box ticked. The form never holds it: it
/// keeps `inversion.algorithm` on `lsqr`, and `inversion.heidi.enabled` on.
pub const LSQR_HEIDI: &str = qsmxt_engine::schema::aliases::LSQR_HEIDI.name;

/// `list`, with `alias` after the entry for LSQR.
fn with_lsqr_heidi<T: Copy>(list: &[T], alias: T) -> Vec<T> {
    let lsqr = qsmxt_engine::schema::aliases::LSQR_HEIDI.value;
    let at = QSM_ALGO_OPTIONS.iter().position(|o| *o == lsqr).expect("LSQR is an inversion") + 1;
    let mut out = list.to_vec();
    out.insert(at, alias);
    out
}

/// The inversion row's options: [`QSM_ALGO_OPTIONS`] and the LSQR+HEIDI alias. The row's selection
/// is an index into these.
pub static QSM_PICKER_OPTIONS: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| with_lsqr_heidi(QSM_ALGO_OPTIONS, LSQR_HEIDI));

/// The background-removal select's options, in its order: the tests' names for its indices
/// (`bf_options_are_the_selects` holds them to it).
#[cfg(test)]
pub const BF_OPTIONS: &[&str] = &["vsharp", "pdf", "lbv", "ismv", "sharp", "resharp", "harperella", "iharperella", "bfrnet", "iqfm"];
/// The position of an echo-time choice among an echo-time select's options: the named times,
/// then a given time.
pub fn echo_time_index(te: qsmxt_engine::EchoTimeChoice) -> usize {
    use qsmxt_engine::EchoTimeChoice as E;
    match te { E::First => 0, E::Last => 1, E::Spacing => 2, E::Sum => 3, E::Seconds(_) => 4 }
}
/// The position of a χ bound among its select's options: the bound in ppm, then from the echo
/// spacing.
pub fn chi_bound_index(b: qsmxt_engine::ChiBoundChoice) -> usize {
    match b { qsmxt_engine::ChiBoundChoice::Ppm(_) => 0, qsmxt_engine::ChiBoundChoice::EchoSpacing => 1 }
}

/// The reference picker's options: the two methods, then every region
/// ([`crate::settings_rows`]).
pub fn qsm_ref_options() -> &'static [&'static str] { crate::settings_rows::options("qsm.reference").0 }

/// The output a checkbox asks for, when another option can force that output on.
fn forced_output(field: &str) -> Option<qsmxt_engine::requirements::Output> {
    qsmxt_engine::requirements::Output::of_setting(field)
}

impl App {
    // --- Field value accessors ---

    pub fn text_value(&self) -> &str {
        match (self.active_tab, self.active_field) {
            (0, 1) => match self.input_mode {
                InputMode::Bids => &self.form.bids_dir,
                InputMode::NIfTI => &self.nifti_state.input_dir,
                InputMode::DicomToBids => &self.dicom_state.dicom_dir,
                InputMode::Example => &self.example_state.output_dir,
            },
            (0, 2) => match self.input_mode {
                InputMode::Bids => &self.form.output_dir,
                InputMode::NIfTI => &self.nifti_state.output_dir,
                InputMode::DicomToBids => &self.dicom_state.output_dir,
                InputMode::Example => &self.form.output_dir,
            },
            (0, 3) => &self.form.config_file,
            _ => "",
        }
    }

    pub(super) fn text_value_mut(&mut self) -> &mut String {
        match (self.active_tab, self.active_field) {
            (0, 1) => match self.input_mode {
                InputMode::Bids => &mut self.form.bids_dir,
                InputMode::NIfTI => &mut self.nifti_state.input_dir,
                InputMode::DicomToBids => &mut self.dicom_state.dicom_dir,
                InputMode::Example => &mut self.example_state.output_dir,
            },
            (0, 2) => match self.input_mode {
                InputMode::Bids => &mut self.form.output_dir,
                InputMode::NIfTI => &mut self.nifti_state.output_dir,
                InputMode::DicomToBids => &mut self.dicom_state.output_dir,
                InputMode::Example => &mut self.form.output_dir,
            },
            (0, 3) => &mut self.form.config_file,
            _ => unreachable!("text_value_mut called on non-text field"),
        }
    }

    /// The option that turns an output on although its own checkbox is off, if one does.
    ///
    /// The run's own rule (`requirements::implied_by` on the resolved config), so what the run
    /// makes, the refusal to untick it and the required-change pop-up cannot disagree: a supplied map
    /// (custom dseg, R2 or R2') satisfies the need and forces nothing.
    fn forced_by(&self, out: qsmxt_engine::requirements::Output) -> Option<String> {
        // On the config as the run takes it: an output needed by one that is itself needed (R2
        // for an R2′ that χ-separation needs) counts.
        let config = crate::command::config_from_app(self).ok()?.normalised().0;
        qsmxt_engine::requirements::implied_by(&config, out)
    }

    /// [`Self::forced_by`], naming the setting by its label rather than its flag ("Reference =
    /// ventricles").
    fn forced_by_label(&self, out: qsmxt_engine::requirements::Output) -> Option<String> {
        let config = crate::command::config_from_app(self).ok()?.normalised().0;
        qsmxt_engine::requirements::implied_by_label(&config, out)
    }

    /// A text field's value, by name: the run form's, the Input tab's, else the pipeline form's.
    pub fn field_text(&self, field: &str) -> std::borrow::Cow<'_, str> {
        if let Some(path) = stored_path(field) {
            return self.form.settings.text(path);
        }
        std::borrow::Cow::Borrowed(self.form.text(field).map_or("", |s| s.as_str()))
    }

    pub fn field_text_mut(&mut self, field: &str) -> Option<&mut String> {
        if let Some(path) = stored_path(field) {
            Some(self.form.settings.text_mut(path))
        } else {
            self.form.text_mut(field)
        }
    }

    /// A select's option index, by name.
    pub fn field_select(&self, field: &str) -> usize {
        // A segmentation from derivatives, or SynthSeg's.
        if field == "segmentation_source" {
            return usize::from(!self.form.settings.text(DSEG).trim().is_empty());
        }
        if field == "inversion.algorithm" {
            return self.inversion_picked();
        }
        if let Some(path) = stored_path(field) {
            return self.form.settings.select(path);
        }
        if let Some(&c) = self.form.choice(field) {
            return c;
        }
        if field == "mask_preset" {
            return self.pipeline_state.mask_preset;
        }
        0
    }

    /// The inversion picker's choice: the algorithm, or LSQR+HEIDI for LSQR with HEIDI ticked
    /// (not offered for an orientation group, which runs no HEIDI).
    fn inversion_picked(&self) -> usize {
        let st = &self.form.settings;
        let algorithm = st.get("inversion.algorithm");
        let name = algorithm.as_str().unwrap_or("");
        let name = if name == "lsqr" && st.toggle("inversion.heidi.enabled") && !self.pipeline_state.ctx.orientation_group {
            LSQR_HEIDI
        } else {
            name
        };
        QSM_PICKER_OPTIONS.iter().position(|o| *o == name).unwrap_or(0)
    }

    /// Pick the inversion: LSQR+HEIDI and plain LSQR are two choices of one row, which set
    /// HEIDI's checkbox with the algorithm.
    fn pick_inversion(&mut self, val: usize) {
        let st = &mut self.form.settings;
        let name = QSM_PICKER_OPTIONS.get(val).copied().unwrap_or("");
        if name == "lsqr" {
            st.set("inversion.heidi.enabled", false);
        }
        let name = if name == LSQR_HEIDI {
            st.set("inversion.heidi.enabled", true);
            "lsqr"
        } else {
            name
        };
        if QSM_ALGO_OPTIONS.contains(&name) {
            st.set("inversion.algorithm", name);
        }
    }

    pub fn set_field_select(&mut self, field: &str, val: usize) {
        // One representation per choice: a supplied segmentation (any tool, until named) is not
        // also a SynthSeg request, and SynthSeg reads no derivatives.
        if field == "segmentation_source" {
            let st = &mut self.form.settings;
            if val == 1 {
                if st.text(DSEG).trim().is_empty() {
                    st.set(DSEG, "*");
                }
                st.set("pipeline.do_segmentation", false);
            } else {
                st.reset(DSEG, false);
                st.set("pipeline.do_segmentation", true);
            }
            return;
        }
        if field == "inversion.algorithm" {
            return self.pick_inversion(val);
        }
        if let Some(path) = stored_path(field) {
            return self.form.settings.set_select(path, val);
        }
        if let Some(c) = self.form.choice_mut(field) {
            *c = val;
            return;
        }
        if field == "mask_preset" {
            self.choose_mask_preset(val);
        }
    }

    /// Whether the run makes an output: its checkbox is ticked, or another setting needs it (a
    /// required change the run makes itself when it is left off, [`App::forced_by`]).
    pub fn output_made(&self, field: &str) -> bool {
        if field == "pipeline.do_segmentation" {
            return self.field_toggle(field)
                || self.forced_by(qsmxt_engine::requirements::Output::Segmentation).is_some();
        }
        if field == "pipeline.do_qsm" && !self.form.settings.toggle("pipeline.do_qsm") {
            return self.row_context().qsm_forced_by.is_some();
        }
        if forced_output(field).is_some_and(|out| self.forced_by(out).is_some()) {
            return true;
        }
        self.field_toggle(field)
    }

    /// A checkbox as the user set it. An output another setting needs is not shown as ticked: the
    /// change that needs it offers to tick it ("Required change"), and the setting is marked
    /// while it is left off.
    pub fn field_toggle(&self, field: &str) -> bool {
        if let Some(m) = dicom_map(field) {
            return !self.form.dicom_unticked.iter().any(|u| u.eq_ignore_ascii_case(m.suffix));
        }
        if field == "pipeline.do_segmentation" {
            let st = &self.form.settings;
            return st.toggle("pipeline.do_segmentation") || !st.text(DSEG).trim().is_empty();
        }
        if let Some(path) = stored_path(field) {
            return self.form.settings.toggle(path);
        }
        self.form.flag(field).is_some_and(|&b| b)
    }

    /// Flip a checkbox. Turning off an output another setting needs is refused, with the reason.
    pub fn toggle_field(&mut self, field: &str) {
        if let Some(m) = dicom_map(field) {
            return self.toggle_dicom_map(m);
        }
        if field == "pipeline.do_qsm" && self.form.settings.toggle("pipeline.do_qsm") {
            // QSM another output needs: refuse turning it off, as for the other maps.
            self.form.settings.set("pipeline.do_qsm", false);
            let why = self.row_context().qsm_forced_by;
            self.form.settings.set("pipeline.do_qsm", true);
            if let Some(why) = why {
                self.error_message = Some(format!("QSM is required: {why} — change or turn that off first."));
                return;
            }
        }
        if field == "pipeline.do_segmentation" {
            if !self.field_toggle(field) {
                self.form.settings.set("pipeline.do_segmentation", true);
                return;
            }
            // What needs a segmentation whichever its source: asked with no dseg supplied.
            let kept = self.form.settings.clone();
            self.form.settings.reset(DSEG, false);
            let why = self.forced_by_label(qsmxt_engine::requirements::Output::Segmentation);
            self.form.settings = kept;
            if let Some(why) = why {
                self.error_message = Some(format!(
                    "Segmentation is required by {why} — change or turn that off first."));
                return;
            }
            self.form.settings.set("pipeline.do_segmentation", false);
            self.form.settings.reset(DSEG, false);
            return;
        }
        if let Some(out) = forced_output(field) {
            if self.field_toggle(field) {
                if let Some(why) = self.forced_by_label(out) {
                    self.error_message = Some(format!(
                        "{} is required by {why} — change or turn that off first (or supply the map).",
                        out.noun()));
                    return;
                }
            }
        }
        if let Some(path) = stored_path(field) {
            return self.form.settings.flip(path);
        }
        if let Some(b) = self.form.flag_mut(field) {
            *b = !*b;
        }
    }

    /// What an empty text field reads as.
    pub fn field_placeholder(&self, field: &str) -> &'static str {
        match field {
            "slurm_account" => "(required)",
            "mem_limit_gb" | "n_procs" => "auto",
            "multi_orientation.lambda" => "0 (none)",
            "multi_orientation.reference" => "first by name",
            _ if stored_path(field).is_some() => stored_placeholder(stored_path(field).unwrap_or_default()),
            _ if self.form.text(field).is_some() => "(empty)",
            _ => "(default)",
        }
    }

    /// Reset the focused row's field to its default.
    pub(super) fn reset_row_field(&mut self) {
        if let Some(field) = self.focused_field_name() {
            self.reset_field(&field);
        }
    }

    /// Reset one named field to its default, whichever form it lives in.
    pub(crate) fn reset_field(&mut self, field: &str) {
        let form = RunForm::default();
        if let Some(m) = dicom_map(field) {
            return self.form.dicom_unticked.retain(|u| !u.eq_ignore_ascii_case(m.suffix));
        }
        match field {
            "segmentation_source" | "pipeline.do_segmentation" => {
                self.form.settings.reset(DSEG, false);
                self.form.settings.reset("pipeline.do_segmentation", false);
                return;
            }
            _ => {}
        }
        if let Some(path) = stored_path(field) {
            let value_row = crate::settings_rows::is_value_row(field, path);
            return self.form.settings.reset(path, value_row);
        }
        if let (Some(d), Some(v)) = (form.text(field), self.form.text_mut(field)) {
            *v = d.clone();
        } else if let (Some(d), Some(v)) = (form.flag(field), self.form.flag_mut(field)) {
            *v = *d;
        } else if let (Some(d), Some(v)) = (form.choice(field), self.form.choice_mut(field)) {
            *v = *d;
        } else if field == "mask_preset" {
            self.set_field_select(field, PipelineFormState::default().mask_preset);
        }
    }

    /// Reset every setting of the active sub-tab: each field it shows, again for the rows a
    /// reset reveals, until none are new (and the mask recipe, on the Masking page).
    pub(super) fn reset_current_page(&mut self) {
        let Some(page) = self.page() else { return };
        self.reset_page(page);
        self.reset_view();
    }

    /// Every setting on `page` back to its default (`R`, and a pipeline's stages).
    pub(crate) fn reset_page(&mut self, page: Page) {
        if page == Page::Masking {
            self.reset_mask();
        }
        let mut done = std::collections::HashSet::new();
        loop {
            let fields: Vec<&'static str> = self.page_rows(page).iter().filter_map(row_field)
                .filter(|f| done.insert(*f)).collect();
            if fields.is_empty() { break; }
            fields.iter().for_each(|f| self.reset_field(f));
        }
    }
}
