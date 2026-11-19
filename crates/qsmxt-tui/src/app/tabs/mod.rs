//! Key handling: the tab bar, dispatch to the focused tab, and resets.

use super::*;

mod methods;
mod rows;

/// Find the position of the previous word boundary (for Ctrl/Alt+Backspace).
pub(super) fn word_boundary_left(text: &str, cursor: usize) -> usize {
    if cursor == 0 { return 0; }
    let bytes = text.as_bytes();
    let mut pos = cursor - 1;
    // Skip whitespace/separators
    while pos > 0 && matches!(bytes[pos], b' ' | b'/' | b'_' | b'-') {
        pos -= 1;
    }
    // Skip word chars
    while pos > 0 && !matches!(bytes[pos - 1], b' ' | b'/' | b'_' | b'-') {
        pos -= 1;
    }
    pos
}

impl App {
    /// Main-tab switching, the same in every tab's key handler: the digits `1`..=`N` (one per
    /// entry of [`TAB_NAMES`]), Tab and Shift-Tab. Returns whether `key` switched tabs.
    pub fn switch_tab(&mut self, key: KeyEvent) -> bool {
        let n = TAB_NAMES.len();
        let target = match key.code {
            KeyCode::Char(c) => match c.to_digit(10) {
                Some(d) if (1..=n as u32).contains(&d) => d as usize - 1,
                _ => return false,
            },
            KeyCode::Tab => (self.active_tab + 1) % n,
            KeyCode::BackTab => (self.active_tab + n - 1) % n,
            _ => return false,
        };
        self.active_tab = target;
        self.reset_view();
        self.sync_pipeline_mode();
        // A stage with sub-tabs opens on its sub-tab strip: ←/→ choose one, ↓ enters it.
        self.subtab_focus = self.has_subtabs();
        true
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // The recommendation pop-up captures input under the update prompt, the announcements
        // and the tips.
        let above = self.update_modal.is_some() || self.news_modal.is_some() || self.tip_modal.is_some();
        if self.rec_modal.is_some() && !above {
            self.handle_rec_modal_key(key);
            return;
        }
        if self.refs.modal.is_some() && !above {
            self.handle_ref_modal_key(key);
            return;
        }
        if key.code == KeyCode::Char('!') && !self.is_typing() && Self::is_rows_tab(self.active_tab) {
            self.review_recommendations();
            return;
        }
        // A: Basic or Advanced mode, on the Reconstruction tab (its sidebar or a section).
        if key.code == KeyCode::Char('A') && !self.is_typing() && self.active_tab == TAB_RECONSTRUCTION && !self.tab_focus {
            self.toggle_advanced();
            return;
        }
        // P: the Pipeline tab, from any settings page.
        if key.code == KeyCode::Char('P') && !self.is_typing() && Self::is_rows_tab(self.active_tab) && !self.tab_focus {
            self.active_tab = TAB_PIPELINE;
            self.reset_view();
            return;
        }
        self.handle_key_unchecked(key);
    }

    fn handle_key_unchecked(&mut self, key: KeyEvent) {
        // The update prompt sits on top of everything, including an algorithm modal.
        if self.update_modal.is_some() {
            self.handle_update_modal_key(key);
            return;
        }

        if self.news_modal.is_some() {
            self.handle_news_modal_key(key);
            return;
        }

        if self.tip_modal.is_some() {
            self.handle_tip_modal_key(key);
            return;
        }

        // An open algorithm-selection modal captures all input until dismissed.
        if self.algo_modal.is_some() {
            self.handle_algo_modal_key(key);
            return;
        }

        if key.code == KeyCode::Char('?') && !self.is_typing() {
            self.open_tip();
            return;
        }

        // Clear error on any non-F5 key press
        if key.code != KeyCode::F(5) {
            self.error_message = None;
        }
        self.status_message = None;

        if self.tab_focus {
            self.handle_tab_bar_key(key);
            return;
        }

        // Route tab 0 (Input) to its combined IO + filter handler
        if self.active_tab == TAB_INPUT {
            self.handle_input_tab_key(key);
            return;
        }
        if self.active_tab == TAB_PIPELINE {
            self.handle_pipeline_tab_key(key);
            return;
        }
        // The row-based tabs share one handler (the page is synced first).
        if Self::is_rows_tab(self.active_tab) {
            self.sync_pipeline_mode();
            self.handle_rows_key(key);
            return;
        }

        if self.active_tab == TAB_METHODS {
            self.handle_methods_tab_key(key);
        }
    }

    /// Reset the focused field on a generic form tab to its default.
    pub(super) fn reset_current_field(&mut self) {
        let defaults = RunForm::default();
        match (self.active_tab, self.active_field) {
            // Tab 0 (Input) IO fields
            (0, 0) => self.input_mode = InputMode::Bids,
            (0, 1) => match self.input_mode {
                InputMode::Bids => self.form.bids_dir = defaults.bids_dir.clone(),
                InputMode::NIfTI => {
                    self.nifti_state.input_dir = String::new();
                    self.nifti_state.magnitude_files.clear();
                    self.nifti_state.phase_files.clear();
                    self.nifti_state.echo_times.clear();
                    self.nifti_state.scan_log.clear();
                }
                InputMode::DicomToBids => {
                    self.dicom_state.dicom_dir = String::new();
                    self.dicom_state.scanned_dir = None;
                    self.dicom_state.session = None;
                }
                InputMode::Example => self.example_state.output_dir = String::new(),
            },
            (0, 2) => match self.input_mode {
                InputMode::Bids => self.form.output_dir = defaults.output_dir.clone(),
                InputMode::NIfTI => self.nifti_state.output_dir = String::new(),
                InputMode::DicomToBids => self.dicom_state.output_dir = String::new(),
                InputMode::Example => self.form.output_dir = defaults.output_dir.clone(),
            },
            (0, 3) => self.form.config_file = defaults.config_file.clone(),
            _ => {}
        }
    }

    /// Reset all fields on the current generic form tab to defaults.
    pub(super) fn reset_current_tab(&mut self) {
        let defaults = RunForm::default();
        match self.active_tab {
            TAB_INPUT => {
                self.input_mode = InputMode::Bids;
                self.form.bids_dir = defaults.bids_dir.clone();
                self.form.output_dir = defaults.output_dir.clone();
                self.form.config_file = defaults.config_file.clone();
                self.dicom_state = DicomConvertState::default();
                self.nifti_state = NiftiState::default();
                self.example_state = ExampleState::default();
            }
            tab if Self::is_rows_tab(tab) => self.reset_current_page(),
            _ => {}
        }
    }

    /// Is a text field taking keystrokes? Then `?` is typed rather than opening a tip.
    pub(crate) fn is_typing(&self) -> bool {
        let fs = &self.filter_state;
        self.editing
            || self.pipeline_state.editing
            || self.pipeline_state.mask_value_editing
            || fs.include_editing
            || fs.exclude_editing
            || fs.num_echoes_editing
            || fs.orientation_editing
            || self.nifti_state.editing
    }

    /// Keys while the main tab bar has focus: ←/→ switch tabs (wrapping), ↓ or Enter go into
    /// the tab (onto its sub-tab strip, if it has one). Digits, Tab, F5 and quitting work as
    /// anywhere else.
    fn handle_tab_bar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => {
                self.switch_tab(KeyEvent::from(KeyCode::BackTab));
            }
            KeyCode::Right => {
                self.switch_tab(KeyEvent::from(KeyCode::Tab));
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => self.tab_focus = false,
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::F(5) => self.try_run(),
            KeyCode::Char('n') => self.open_news_history(),
            _ => {
                self.switch_tab(key);
            }
        }
    }
}
