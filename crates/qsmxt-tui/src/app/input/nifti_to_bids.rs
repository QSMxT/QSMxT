//! NIfTI to BIDS: the input files, their acquisition details, and the keys that edit them.

use super::*;

// ─── NIfTI input state ───

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NiftiFocus {
    AddMagnitude,
    MagFile(usize),
    AddPhase,
    PhaseFile(usize),
    EchoTimes,
    FieldStrength,
    B0Direction,
    ConvertButton,
}

pub struct NiftiState {
    pub input_dir: String,
    pub output_dir: String,
    pub magnitude_files: Vec<std::path::PathBuf>,
    pub phase_files: Vec<std::path::PathBuf>,
    pub echo_times: String,
    pub field_strength: String,
    pub b0_direction: String,
    pub focus: NiftiFocus,
    pub editing: bool,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub add_pattern: String,
    pub adding_to: Option<nifti::convert::NiftiPartType>,
    pub scan_log: Vec<String>,
    pub convert_status: ConvertStatus,
    pub convert_log: Vec<String>,
}

impl Default for NiftiState {
    fn default() -> Self {
        Self {
            input_dir: String::new(),
            output_dir: String::new(),
            magnitude_files: Vec::new(),
            phase_files: Vec::new(),
            echo_times: String::new(),
            field_strength: String::new(),
            b0_direction: "0, 0, 1".to_string(),
            focus: NiftiFocus::AddMagnitude,
            editing: false,
            cursor: 0,
            scroll_offset: 0,
            add_pattern: String::new(),
            adding_to: None,
            scan_log: Vec::new(),
            convert_status: ConvertStatus::Idle,
            convert_log: Vec::new(),
        }
    }
}

impl NiftiState {
    /// Scan input directory for NIfTI files, auto-classifying from JSON sidecars.
    pub fn scan_input_directory(&mut self) {
        let trimmed = self.input_dir.trim();
        if trimmed.is_empty() {
            return;
        }
        let dir_str = if let Some(rest) = trimmed.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                format!("{}/{}", home.to_string_lossy(), rest)
            } else {
                trimmed.to_string()
            }
        } else {
            trimmed.to_string()
        };
        let dir = std::path::Path::new(&dir_str);
        if !dir.is_dir() {
            return;
        }

        let result = nifti::convert::scan_nifti_directory(dir);
        self.magnitude_files = result.magnitude_files;
        self.phase_files = result.phase_files;

        // Auto-fill echo times from scan (convert seconds to ms for display)
        if !result.echo_times_s.is_empty() {
            self.echo_times = result
                .echo_times_s
                .iter()
                .map(|et| {
                    let ms = et * 1000.0;
                    if ms == ms.round() {
                        format!("{:.0}", ms)
                    } else {
                        format!("{:.2}", ms)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
        }
        if let Some(fs) = result.field_strength {
            self.field_strength = format!("{}", fs);
        }
        if let Some(b0) = result.b0_dir {
            self.b0_direction = b0.iter().map(|v| format!("{}", v)).collect::<Vec<_>>().join(", ");
        }

        self.scan_log.clear();
        for path in &result.unclassified {
            self.scan_log.push(format!(
                "Unclassified: {}",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
            ));
        }
    }

    /// Add files from a glob pattern or single path to the specified list.
    pub fn add_files_from_pattern(&mut self, pattern: &str, part: nifti::convert::NiftiPartType) {
        let expanded = if let Some(rest) = pattern.trim().strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                format!("{}/{}", home.to_string_lossy(), rest)
            } else {
                pattern.trim().to_string()
            }
        } else {
            pattern.trim().to_string()
        };

        let list = match part {
            nifti::convert::NiftiPartType::Magnitude => &mut self.magnitude_files,
            nifti::convert::NiftiPartType::Phase => &mut self.phase_files,
        };

        // Try glob expansion first
        if let Ok(paths) = glob::glob(&expanded) {
            let mut found = false;
            for entry in paths.flatten() {
                if entry.is_file() {
                    list.push(entry);
                    found = true;
                }
            }
            if found {
                // Try to auto-fill metadata from sidecars of newly added files
                self.try_autofill_from_sidecars();
                return;
            }
        }

        // Fall back to treating as a single path
        let path = std::path::PathBuf::from(&expanded);
        if path.is_file() {
            list.push(path);
            self.try_autofill_from_sidecars();
        }
    }

    /// Try to auto-fill echo times, field strength, and B0 direction from sidecars.
    fn try_autofill_from_sidecars(&mut self) {
        let mut echo_times: Vec<(usize, f64)> = Vec::new();

        for (i, path) in self.magnitude_files.iter().enumerate() {
            let json_path = qsmxt_io::dicom::convert::nii_to_json_path(path);
            if let Some(info) = nifti::convert::read_nifti_sidecar(&json_path) {
                if let Some(et) = info.echo_time {
                    echo_times.push((i, et));
                }
                if self.field_strength.is_empty() {
                    if let Some(fs) = info.field_strength {
                        self.field_strength = format!("{}", fs);
                    }
                }
                if self.b0_direction == "0, 0, 1" {
                    if let Some(ref b0) = info.b0_dir {
                        if b0.len() == 3 {
                            self.b0_direction = b0.iter().map(|v| format!("{}", v)).collect::<Vec<_>>().join(", ");
                        }
                    }
                }
            }
        }

        // Auto-fill echo times if we got them for all magnitude files
        if echo_times.len() == self.magnitude_files.len() && !echo_times.is_empty() {
            // Sort magnitude files by echo time
            echo_times.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            let sorted_files: Vec<_> = echo_times.iter().map(|(i, _)| self.magnitude_files[*i].clone()).collect();
            self.magnitude_files = sorted_files;

            self.echo_times = echo_times
                .iter()
                .map(|(_, et)| {
                    let ms = et * 1000.0;
                    if ms == ms.round() {
                        format!("{:.0}", ms)
                    } else {
                        format!("{:.2}", ms)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
        }
    }

    /// Total number of focusable items in the NIfTI section.
    pub fn focusable_items(&self) -> Vec<NiftiFocus> {
        let mut items = vec![NiftiFocus::AddMagnitude];
        for i in 0..self.magnitude_files.len() {
            items.push(NiftiFocus::MagFile(i));
        }
        items.push(NiftiFocus::AddPhase);
        for i in 0..self.phase_files.len() {
            items.push(NiftiFocus::PhaseFile(i));
        }
        items.extend_from_slice(&[
            NiftiFocus::EchoTimes,
            NiftiFocus::FieldStrength,
            NiftiFocus::B0Direction,
            NiftiFocus::ConvertButton,
        ]);
        items
    }

    pub fn focus_next(&mut self) {
        let items = self.focusable_items();
        if let Some(pos) = items.iter().position(|f| f == &self.focus) {
            if pos + 1 < items.len() {
                self.focus = items[pos + 1];
            }
        }
    }

    pub fn focus_prev(&mut self) -> bool {
        let items = self.focusable_items();
        if let Some(pos) = items.iter().position(|f| f == &self.focus) {
            if pos > 0 {
                self.focus = items[pos - 1];
                return true;
            }
        }
        false // at the top, caller should go back to IO fields
    }
}

impl App {
    /// Run the NIfTI → BIDS conversion synchronously (fast — just file copies + optional 4D split).
    fn run_nifti_conversion(&mut self) {
        if self.nifti_state.magnitude_files.is_empty() {
            self.error_message = Some("Add at least one magnitude file".to_string());
            return;
        }
        if self.nifti_state.phase_files.is_empty() {
            self.error_message = Some("Add at least one phase file".to_string());
            return;
        }
        if self.nifti_state.echo_times.trim().is_empty() {
            self.error_message = Some("Echo times are required".to_string());
            return;
        }
        if self.nifti_state.field_strength.trim().is_empty() {
            self.error_message = Some("Field strength is required".to_string());
            return;
        }
        let field_strength: f64 = match self.nifti_state.field_strength.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.error_message = Some("Field strength must be a number".to_string());
                return;
            }
        };

        if self.nifti_state.convert_status == ConvertStatus::Converting {
            return;
        }

        let echo_times_s: Vec<f64> = self.nifti_state.echo_times
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok().map(|ms| ms / 1000.0))
            .collect();

        let b0_dir: Vec<f64> = self.nifti_state.b0_direction
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .collect();
        let b0_dir = if b0_dir.len() == 3 { b0_dir } else { vec![0.0, 0.0, 1.0] };

        // Determine output directory
        let output_dir_str = if self.nifti_state.output_dir.trim().is_empty() {
            let input_dir = self.nifti_state.input_dir.trim();
            if !input_dir.is_empty() {
                let expanded = if let Some(rest) = input_dir.strip_prefix("~/") {
                    if let Some(home) = std::env::var_os("HOME") {
                        format!("{}/{}", home.to_string_lossy(), rest)
                    } else {
                        input_dir.to_string()
                    }
                } else {
                    input_dir.to_string()
                };
                let p = std::path::Path::new(&expanded);
                let parent = p.parent().unwrap_or(p);
                parent.join("bids_output").to_string_lossy().to_string()
            } else {
                "bids_output".to_string()
            }
        } else {
            let d = self.nifti_state.output_dir.trim().to_string();
            if let Some(rest) = d.strip_prefix("~/") {
                if let Some(home) = std::env::var_os("HOME") {
                    format!("{}/{}", home.to_string_lossy(), rest)
                } else {
                    d
                }
            } else {
                d
            }
        };

        self.nifti_state.convert_status = ConvertStatus::Converting;
        self.nifti_state.convert_log.clear();
        self.nifti_state.convert_log.push("Converting NIfTI files to BIDS...".to_string());

        let params = nifti::convert::NiftiToBidsParams {
            magnitude_files: self.nifti_state.magnitude_files.clone(),
            phase_files: self.nifti_state.phase_files.clone(),
            echo_times_s,
            field_strength,
            b0_dir,
            output_dir: std::path::PathBuf::from(&output_dir_str),
        };

        match nifti::convert::convert_to_bids(&params) {
            Ok(bids_dir) => {
                self.nifti_state.convert_log.push(format!("Done! BIDS directory: {}", bids_dir.display()));
                self.nifti_state.convert_status = ConvertStatus::Done;
                // Switch to BIDS mode with the generated directory
                self.form.bids_dir = bids_dir.to_string_lossy().to_string();
                self.input_mode = InputMode::Bids;
                self.form_scroll_offset = 0;
                self.filter_state.scanned_bids_dir = None; // force rescan
            }
            Err(e) => {
                self.nifti_state.convert_log.push(format!("ERROR: {}", e));
                self.nifti_state.convert_status = ConvertStatus::Error;
                self.error_message = Some(format!("NIfTI conversion failed: {}", e));
            }
        }
    }

    // ─── NIfTI tab key handling ───

    pub(super) fn handle_nifti_tab_key(&mut self, key: KeyEvent) {
        // Handle text editing mode
        if self.nifti_state.editing {
            self.handle_nifti_editing(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}

            // Navigation
            KeyCode::Up | KeyCode::Char('k')
                if !self.nifti_state.focus_prev() =>
            {
                // At top of NIfTI section, go back to IO fields
                self.active_field = Self::INPUT_IO_FIELDS - 1;
            }
            KeyCode::Down | KeyCode::Char('j') => self.nifti_state.focus_next(),

            // Reorder files
            KeyCode::Char('K') => {
                match &self.nifti_state.focus {
                    NiftiFocus::MagFile(i) if *i > 0 => {
                        let i = *i;
                        self.nifti_state.magnitude_files.swap(i, i - 1);
                        self.nifti_state.focus = NiftiFocus::MagFile(i - 1);
                    }
                    NiftiFocus::PhaseFile(i) if *i > 0 => {
                        let i = *i;
                        self.nifti_state.phase_files.swap(i, i - 1);
                        self.nifti_state.focus = NiftiFocus::PhaseFile(i - 1);
                    }
                    _ => {}
                }
            }
            KeyCode::Char('J') => {
                match &self.nifti_state.focus {
                    NiftiFocus::MagFile(i) if *i + 1 < self.nifti_state.magnitude_files.len() => {
                        let i = *i;
                        self.nifti_state.magnitude_files.swap(i, i + 1);
                        self.nifti_state.focus = NiftiFocus::MagFile(i + 1);
                    }
                    NiftiFocus::PhaseFile(i) if *i + 1 < self.nifti_state.phase_files.len() => {
                        let i = *i;
                        self.nifti_state.phase_files.swap(i, i + 1);
                        self.nifti_state.focus = NiftiFocus::PhaseFile(i + 1);
                    }
                    _ => {}
                }
            }

            // Delete file
            KeyCode::Delete | KeyCode::Char('d') => {
                match &self.nifti_state.focus {
                    NiftiFocus::MagFile(i) => {
                        let i = *i;
                        self.nifti_state.magnitude_files.remove(i);
                        if self.nifti_state.magnitude_files.is_empty() {
                            self.nifti_state.focus = NiftiFocus::AddMagnitude;
                        } else if i >= self.nifti_state.magnitude_files.len() {
                            self.nifti_state.focus = NiftiFocus::MagFile(self.nifti_state.magnitude_files.len() - 1);
                        }
                    }
                    NiftiFocus::PhaseFile(i) => {
                        let i = *i;
                        self.nifti_state.phase_files.remove(i);
                        if self.nifti_state.phase_files.is_empty() {
                            self.nifti_state.focus = NiftiFocus::AddPhase;
                        } else if i >= self.nifti_state.phase_files.len() {
                            self.nifti_state.focus = NiftiFocus::PhaseFile(self.nifti_state.phase_files.len() - 1);
                        }
                    }
                    _ => {}
                }
            }

            // Enter: start editing text fields or add files
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_nifti_focus(),

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    /// Enter or Space on the NIfTI section: start editing a text field, add files, or convert.
    fn activate_nifti_focus(&mut self) {
        match &self.nifti_state.focus {
            NiftiFocus::AddMagnitude => {
                self.nifti_state.editing = true;
                self.nifti_state.add_pattern.clear();
                self.nifti_state.cursor = 0;
                self.nifti_state.adding_to = Some(nifti::convert::NiftiPartType::Magnitude);
            }
            NiftiFocus::AddPhase => {
                self.nifti_state.editing = true;
                self.nifti_state.add_pattern.clear();
                self.nifti_state.cursor = 0;
                self.nifti_state.adding_to = Some(nifti::convert::NiftiPartType::Phase);
            }
            NiftiFocus::EchoTimes => {
                self.nifti_state.editing = true;
                self.nifti_state.cursor = self.nifti_state.echo_times.len();
                self.nifti_state.adding_to = None;
            }
            NiftiFocus::FieldStrength => {
                self.nifti_state.editing = true;
                self.nifti_state.cursor = self.nifti_state.field_strength.len();
                self.nifti_state.adding_to = None;
            }
            NiftiFocus::B0Direction => {
                self.nifti_state.editing = true;
                self.nifti_state.cursor = self.nifti_state.b0_direction.len();
                self.nifti_state.adding_to = None;
            }
            NiftiFocus::MagFile(_) | NiftiFocus::PhaseFile(_) => {}
            NiftiFocus::ConvertButton => {
                self.run_nifti_conversion();
            }
        }
    }

    fn handle_nifti_editing(&mut self, key: KeyEvent) {
        let is_adding_file = self.nifti_state.adding_to.is_some();

        // Get a mutable reference to the string being edited
        let (text, cursor) = if is_adding_file {
            (&mut self.nifti_state.add_pattern, &mut self.nifti_state.cursor)
        } else {
            match &self.nifti_state.focus {
                NiftiFocus::EchoTimes => (&mut self.nifti_state.echo_times, &mut self.nifti_state.cursor),
                NiftiFocus::FieldStrength => (&mut self.nifti_state.field_strength, &mut self.nifti_state.cursor),
                NiftiFocus::B0Direction => (&mut self.nifti_state.b0_direction, &mut self.nifti_state.cursor),
                _ => {
                    self.nifti_state.editing = false;
                    return;
                }
            }
        };

        match key.code {
            KeyCode::Esc => {
                self.nifti_state.editing = false;
                self.nifti_state.adding_to = None;
            }
            KeyCode::Enter => {
                if is_adding_file {
                    let pattern = self.nifti_state.add_pattern.clone();
                    let part = self.nifti_state.adding_to.unwrap();
                    self.nifti_state.add_files_from_pattern(&pattern, part);
                    self.nifti_state.add_pattern.clear();
                    self.nifti_state.adding_to = None;
                }
                self.nifti_state.editing = false;
            }
            KeyCode::Char(c) => {
                text.insert(*cursor, c);
                *cursor += 1;
            }
            KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && *cursor > 0 => {
                let new_pos = word_boundary_left(text, *cursor);
                text.drain(new_pos..*cursor);
                *cursor = new_pos;
            }
            KeyCode::Backspace if *cursor > 0 => {
                *cursor -= 1;
                text.remove(*cursor);
            }
            KeyCode::Left => *cursor = cursor.saturating_sub(1),
            KeyCode::Right => {
                let len = text.len();
                if *cursor < len {
                    *cursor += 1;
                }
            }
            KeyCode::Home => *cursor = 0,
            KeyCode::End => *cursor = text.len(),
            _ => {}
        }
    }
}
