use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::bids::discovery::{self, BidsTree};
use crate::dicom;
use crate::nifti;

// ─── Input mode ───

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Bids,
    NIfTI,
    DicomToBids,
}

// ─── DICOM conversion state ───

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DicomFocus {
    Series(usize), // index into flat series list
    ConvertButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertStatus {
    Idle,
    Converting,
    Done,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStatus {
    Idle,
    Scanning,
    Done,
    Error,
}

pub struct DicomConvertState {
    pub session: Option<dicom::DicomSession>,
    pub convert_status: ConvertStatus,
    pub convert_log: Vec<String>,
    pub convert_errors: Vec<String>,
    pub focus: DicomFocus,
    pub scanned_dir: Option<String>,
    pub scroll_offset: usize,
    pub dicom_dir: String,
    pub output_dir: String,
    pub scan_status: ScanStatus,
    pub scan_error: Option<String>,
    scan_receiver: Option<mpsc::Receiver<Result<dicom::DicomSession, String>>>,
    scan_progress: Arc<AtomicUsize>,
    convert_receiver: Option<mpsc::Receiver<dicom::convert::ConvertMessage>>,
    /// Cached dcm2niix resolution for the availability indicator. `None` until
    /// first checked; the inner `Option` is the resolved path (or `None` if not
    /// found). The convert action re-checks live, so this is display-only.
    dcm2niix_resolved: Option<Option<std::path::PathBuf>>,
}

impl Default for DicomConvertState {
    fn default() -> Self {
        Self {
            session: None,
            convert_status: ConvertStatus::Idle,
            convert_log: Vec::new(),
            convert_errors: Vec::new(),
            focus: DicomFocus::Series(0),
            scanned_dir: None,
            scroll_offset: 0,
            dicom_dir: String::new(),
            output_dir: String::new(),
            scan_status: ScanStatus::Idle,
            scan_error: None,
            scan_receiver: None,
            scan_progress: Arc::new(AtomicUsize::new(0)),
            convert_receiver: None,
            dcm2niix_resolved: None,
        }
    }
}

impl DicomConvertState {
    /// Resolve dcm2niix once for the availability indicator, caching the result.
    /// Cheap on repeat calls (no PATH subprocess spawn after the first check).
    pub fn ensure_dcm2niix_checked(&mut self) {
        if self.dcm2niix_resolved.is_none() {
            self.dcm2niix_resolved = Some(dicom::convert::find_dcm2niix());
        }
    }

    /// Cached dcm2niix path for display (`None` if not found or not yet checked).
    pub fn dcm2niix_path(&self) -> Option<&std::path::PathBuf> {
        self.dcm2niix_resolved.as_ref().and_then(|o| o.as_ref())
    }

    /// Kick off a background scan if the directory changed. Non-blocking.
    /// Also polls for completion of any in-progress scan.
    pub fn maybe_rescan(&mut self) {
        // Always poll for background scan completion first
        self.poll_scan();

        let trimmed = self.dicom_dir.trim();
        let dir = if let Some(rest) = trimmed.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                format!("{}/{}", home.to_string_lossy(), rest)
            } else {
                trimmed.to_string()
            }
        } else {
            trimmed.to_string()
        };
        if dir.is_empty() {
            return;
        }
        // Don't start a new scan if one is already running
        if self.scan_status == ScanStatus::Scanning {
            return;
        }
        // Don't rescan the same directory
        if self.scanned_dir.as_deref() == Some(&dir) {
            return;
        }
        // Only scan if path is actually a directory
        if !Path::new(&dir).is_dir() {
            return;
        }

        // Launch background scan
        let (tx, rx) = mpsc::channel();
        let dir_clone = dir.clone();
        self.scan_progress = Arc::new(AtomicUsize::new(0));
        let progress = Arc::clone(&self.scan_progress);
        std::thread::spawn(move || {
            let result = dicom::scan_dicom_directory(Path::new(&dir_clone), progress);
            let _ = tx.send(result);
        });

        self.scan_receiver = Some(rx);
        self.scan_status = ScanStatus::Scanning;
        self.scan_error = None;
        self.session = None;
        self.scanned_dir = Some(dir);
    }

    /// Number of files examined so far during a scan.
    pub fn scan_files_examined(&self) -> usize {
        self.scan_progress.load(Ordering::Relaxed)
    }

    /// Poll for background conversion messages. Called each frame.
    pub fn poll_convert(&mut self) -> Option<std::path::PathBuf> {
        let rx = self.convert_receiver.as_ref()?;
        let mut completed_bids_dir = None;
        // Drain all available messages
        loop {
            match rx.try_recv() {
                Ok(dicom::convert::ConvertMessage::Log(line)) => {
                    self.convert_log.push(line);
                }
                Ok(dicom::convert::ConvertMessage::Error(err)) => {
                    self.convert_log.push(format!("ERROR: {}", err));
                    self.convert_errors.push(err);
                }
                Ok(dicom::convert::ConvertMessage::Done { bids_dir }) => {
                    if self.convert_errors.is_empty() {
                        self.convert_status = ConvertStatus::Done;
                    } else {
                        self.convert_status = ConvertStatus::Error;
                    }
                    self.convert_receiver = None;
                    completed_bids_dir = Some(bids_dir);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.convert_status = ConvertStatus::Error;
                    self.convert_log.push("ERROR: Conversion thread crashed".to_string());
                    self.convert_receiver = None;
                    break;
                }
            }
        }
        completed_bids_dir
    }

    /// Poll for background scan completion. Called each frame.
    pub fn poll_scan(&mut self) {
        let Some(ref rx) = self.scan_receiver else { return };
        match rx.try_recv() {
            Ok(Ok(session)) => {
                self.session = Some(session);
                self.focus = DicomFocus::Series(0);
                self.scan_status = ScanStatus::Done;
                self.scan_receiver = None;
            }
            Ok(Err(e)) => {
                self.session = None;
                self.scan_status = ScanStatus::Error;
                self.scan_error = Some(e);
                self.scan_receiver = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Still scanning
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.scan_status = ScanStatus::Error;
                self.scan_error = Some("Scan thread crashed".to_string());
                self.scan_receiver = None;
            }
        }
    }

    pub fn focus_next(&mut self) {
        // Navigation is over UNIQUE series (one row per series identity), not per-subject instances.
        let series_count = self.session.as_ref().map(|s| s.unique_series_count()).unwrap_or(0);
        match self.focus {
            DicomFocus::Series(i) => {
                if i + 1 < series_count {
                    self.focus = DicomFocus::Series(i + 1);
                } else {
                    self.focus = DicomFocus::ConvertButton;
                }
            }
            DicomFocus::ConvertButton => {}
        }
    }

    pub fn focus_prev(&mut self) {
        match self.focus {
            DicomFocus::Series(0) => {} // at top of series, handle_dicom_tab_key goes to IO
            DicomFocus::Series(i) => self.focus = DicomFocus::Series(i - 1),
            DicomFocus::ConvertButton => {
                let series_count = self.session.as_ref().map(|s| s.unique_series_count()).unwrap_or(0);
                if series_count > 0 {
                    self.focus = DicomFocus::Series(series_count - 1);
                }
            }
        }
    }
}

// ─── NIfTI input state ───

#[derive(Debug, Clone, PartialEq, Eq)]
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
            let json_path = crate::dicom::convert::nii_to_json_path(path);
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
                self.focus = items[pos + 1].clone();
            }
        }
    }

    pub fn focus_prev(&mut self) -> bool {
        let items = self.focusable_items();
        if let Some(pos) = items.iter().position(|f| f == &self.focus) {
            if pos > 0 {
                self.focus = items[pos - 1].clone();
                return true;
            }
        }
        false // at the top, caller should go back to IO fields
    }
}

// Tab indices. Defined as named constants (usable directly in `match` patterns) so the tab
// order can be changed in ONE place — reorder `TAB_NAMES` and update these — without hunting
// down hardcoded `(tab, field)` literals across the file.
pub const TAB_INPUT: usize = 0;
pub const TAB_QSM: usize = 1;
pub const TAB_SEPARATION: usize = 2;
pub const TAB_SUPPLEMENTARY: usize = 3;
pub const TAB_EXECUTION: usize = 4;
pub const TAB_METHODS: usize = 5;

pub const TAB_NAMES: [&str; 6] = [
    "Input",
    "QSM",
    "Separation",
    "Supplementary",
    "Execution",
    "Methods",
];

// ─── Filter tree state ───

/// What is focused in the flattened filter tree view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterFocus {
    Include,
    Exclude,
    TreeNode(usize), // index into the flattened visible node list
    NumEchoes,
}

/// Find the position of the previous word boundary (for Ctrl/Alt+Backspace).
fn word_boundary_left(text: &str, cursor: usize) -> usize {
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

/// Which text field is being edited in the filter area.
enum FilterTextField {
    Include,
    Exclude,
}

/// A single visible row in the flattened tree (for navigation/rendering).
#[derive(Debug, Clone)]
pub enum TreeRow {
    Subject(usize),                 // index into tree.subjects
    Session(usize, usize),         // (subject_idx, session_idx)
    Run { sub: usize, ses: Option<usize>, run: usize }, // leaf run
}

/// Filter tab state: tree of BIDS runs with selection and navigation.
#[derive(Debug, Clone)]
pub struct FilterTreeState {
    pub tree: Option<BidsTree>,
    pub collapsed: HashSet<String>,
    pub focus: FilterFocus,
    pub include_pattern: String,
    pub include_editing: bool,
    pub include_cursor: usize,
    pub exclude_pattern: String,
    pub exclude_editing: bool,
    pub exclude_cursor: usize,
    pub num_echoes: String,
    pub num_echoes_editing: bool,
    pub num_echoes_cursor: usize,
    pub scanned_bids_dir: Option<String>,
    pub scroll_offset: usize,
    /// Set when the user manually toggles tree checkboxes (pattern may not match)
    pub manual_override: bool,
}

impl Default for FilterTreeState {
    fn default() -> Self {
        Self {
            tree: None,
            collapsed: HashSet::new(),
            focus: FilterFocus::Include,
            include_pattern: "*".to_string(),
            include_editing: false,
            include_cursor: 0,
            exclude_pattern: String::new(),
            exclude_editing: false,
            exclude_cursor: 0,
            num_echoes: String::new(),
            num_echoes_editing: false,
            num_echoes_cursor: 0,
            scanned_bids_dir: None,
            scroll_offset: 0,
            manual_override: false,
        }
    }
}

impl FilterTreeState {
    /// Build the flattened list of visible tree rows (respecting collapsed state).
    pub fn visible_rows(&self) -> Vec<TreeRow> {
        let Some(ref tree) = self.tree else { return Vec::new() };
        let mut rows = Vec::new();
        for (si, sub) in tree.subjects.iter().enumerate() {
            rows.push(TreeRow::Subject(si));
            if self.collapsed.contains(&format!("sub-{}", sub.name)) {
                continue;
            }
            // Direct runs (no session)
            for ri in 0..sub.runs.len() {
                rows.push(TreeRow::Run { sub: si, ses: None, run: ri });
            }
            // Sessions
            for (sei, ses) in sub.sessions.iter().enumerate() {
                rows.push(TreeRow::Session(si, sei));
                if self.collapsed.contains(&format!("sub-{}/ses-{}", sub.name, ses.name)) {
                    continue;
                }
                for ri in 0..ses.runs.len() {
                    rows.push(TreeRow::Run { sub: si, ses: Some(sei), run: ri });
                }
            }
        }
        rows
    }

    /// Total number of focusable items: pattern + tree rows + num_echoes.
    /// Move focus down.
    pub fn focus_next(&mut self) {
        match self.focus {
            FilterFocus::Include => {
                self.focus = FilterFocus::Exclude;
            }
            FilterFocus::Exclude => {
                if !self.visible_rows().is_empty() {
                    self.focus = FilterFocus::TreeNode(0);
                } else {
                    self.focus = FilterFocus::NumEchoes;
                }
            }
            FilterFocus::TreeNode(i) => {
                let rows = self.visible_rows().len();
                if i + 1 < rows {
                    self.focus = FilterFocus::TreeNode(i + 1);
                } else {
                    self.focus = FilterFocus::NumEchoes;
                }
            }
            FilterFocus::NumEchoes => {} // already at bottom
        }
    }

    /// Move focus up.
    pub fn focus_prev(&mut self) {
        match self.focus {
            FilterFocus::Include => {} // already at top
            FilterFocus::Exclude => self.focus = FilterFocus::Include,
            FilterFocus::TreeNode(0) => self.focus = FilterFocus::Exclude,
            FilterFocus::TreeNode(i) => self.focus = FilterFocus::TreeNode(i - 1),
            FilterFocus::NumEchoes => {
                let rows = self.visible_rows().len();
                if rows > 0 {
                    self.focus = FilterFocus::TreeNode(rows - 1);
                } else {
                    self.focus = FilterFocus::Exclude;
                }
            }
        }
    }

    /// Scan BIDS directory if it changed since last scan.
    pub fn maybe_rescan(&mut self, bids_dir: &str) {
        let trimmed = bids_dir.trim();
        let dir = if let Some(rest) = trimmed.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                format!("{}/{}", home.to_string_lossy(), rest)
            } else {
                trimmed.to_string()
            }
        } else {
            trimmed.to_string()
        };
        if dir.is_empty() {
            return;
        }
        if self.scanned_bids_dir.as_deref() == Some(&dir) {
            return;
        }
        match discovery::scan_bids_tree(Path::new(&dir)) {
            Ok(tree) => {
                self.tree = Some(tree);
                self.focus = FilterFocus::Include;
                self.scroll_offset = 0;
                self.apply_include_exclude();
            }
            Err(_) => {
                self.tree = None;
            }
        }
        self.scanned_bids_dir = Some(dir);
    }

    /// Apply include/exclude patterns to update tree checkbox state.
    pub fn apply_include_exclude(&mut self) {
        use crate::bids::discovery::passes_include_exclude;
        let Some(ref mut tree) = self.tree else { return };

        let include = if self.include_pattern.trim().is_empty() {
            None
        } else {
            Some(self.include_pattern.split_whitespace().map(String::from).collect::<Vec<_>>())
        };
        let exclude = if self.exclude_pattern.trim().is_empty() {
            None
        } else {
            Some(self.exclude_pattern.split_whitespace().map(String::from).collect::<Vec<_>>())
        };

        tree.for_each_run_mut(|run| {
            run.selected = passes_include_exclude(&run.key_string, &include, &exclude);
        });
        self.manual_override = false;
    }

    /// Toggle the focused tree node (run leaf or subject/session toggle).
    pub fn toggle_focused(&mut self) {
        let rows = self.visible_rows();
        let FilterFocus::TreeNode(idx) = self.focus else { return };
        let Some(row) = rows.get(idx) else { return };
        let Some(ref mut tree) = self.tree else { return };

        match row {
            TreeRow::Subject(si) => {
                let sub = &tree.subjects[*si];
                let new_val = sub.selected_runs() < sub.total_runs();
                tree.subjects[*si].set_all(new_val);
            }
            TreeRow::Session(si, sei) => {
                let ses = &tree.subjects[*si].sessions[*sei];
                let new_val = ses.runs.iter().any(|r| !r.selected);
                tree.subjects[*si].sessions[*sei].set_all(new_val);
            }
            TreeRow::Run { sub, ses, run } => {
                match ses {
                    Some(sei) => {
                        let r = &mut tree.subjects[*sub].sessions[*sei].runs[*run];
                        r.selected = !r.selected;
                    }
                    None => {
                        let r = &mut tree.subjects[*sub].runs[*run];
                        r.selected = !r.selected;
                    }
                }
            }
        }
    }

    /// Toggle collapse on the focused subject or session.
    pub fn toggle_collapse(&mut self) {
        let rows = self.visible_rows();
        let FilterFocus::TreeNode(idx) = self.focus else { return };
        let Some(row) = rows.get(idx) else { return };
        let Some(ref tree) = self.tree else { return };

        let key = match row {
            TreeRow::Subject(si) => format!("sub-{}", tree.subjects[*si].name),
            TreeRow::Session(si, sei) => format!("sub-{}/ses-{}", tree.subjects[*si].name, tree.subjects[*si].sessions[*sei].name),
            _ => return,
        };
        if self.collapsed.contains(&key) {
            self.collapsed.remove(&key);
        } else {
            self.collapsed.insert(key);
        }
    }

    /// Get include/exclude filter args for command building.
    /// When manual_override is set, returns exact key list as include patterns.
    pub fn get_include_exclude(&self) -> (Option<Vec<String>>, Option<Vec<String>>) {
        if self.manual_override {
            // Manual toggle: emit whichever of --include or --exclude is shorter
            let Some(ref tree) = self.tree else { return (None, None) };
            let total = tree.total_runs();
            let selected = tree.selected_runs();
            if selected == total {
                return (None, None); // all selected, no filter needed
            }
            let mut included = Vec::new();
            let mut excluded = Vec::new();
            tree.for_each_run(|run| {
                if run.selected {
                    included.push(run.key_string.clone());
                } else {
                    excluded.push(run.key_string.clone());
                }
            });
            if included.is_empty() {
                return (None, None);
            }
            // Pick the shorter representation
            if excluded.len() <= included.len() {
                excluded.sort();
                (None, Some(excluded))
            } else {
                included.sort();
                (Some(included), None)
            }
        } else {
            // Pattern mode: pass through include/exclude patterns
            let include = if self.include_pattern.trim().is_empty() || self.include_pattern.trim() == "*" {
                None
            } else {
                Some(self.include_pattern.split_whitespace().map(String::from).collect())
            };
            let exclude = if self.exclude_pattern.trim().is_empty() {
                None
            } else {
                Some(self.exclude_pattern.split_whitespace().map(String::from).collect())
            };
            (include, exclude)
        }
    }
}

#[derive(Clone)]
pub enum FieldKind {
    Text,
    Select { options: Vec<&'static str> },
    Checkbox,
}

#[derive(Clone)]
pub struct FieldDef {
    pub label: &'static str,
    pub kind: FieldKind,
    pub help: &'static str,
}

// ─── Pipeline tab state ───

/// Which row set `PipelineFormState` renders — the QSM tab or the Separation tab. Both tabs share
/// the same state + navigation/edit machinery; only `visible_rows()` differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PipelineTabMode {
    #[default]
    Qsm,
    Separation,
}

/// A visible row in the pipeline tab.
#[derive(Debug, Clone)]
pub enum PipelineRow {
    /// Algorithm selector: ◀ value ▶
    AlgoSelect {
        label: &'static str,
        field: &'static str,
        options: &'static [&'static str],
        help: &'static [&'static str], // help text per option
    },
    /// Text parameter input
    Param {
        label: &'static str,
        field: &'static str,
        help: &'static str,
    },
    /// Checkbox toggle
    Toggle {
        label: &'static str,
        field: &'static str,
        help: &'static str,
    },
    /// Section separator (blank line, not focusable)
    Separator,
    /// Informational hint line (styled, not focusable)
    Note { text: &'static str },
    /// Collapsible stage header on the QSM pipeline page (General, Field mapping, Masking,
    /// Background removal, Dipole inversion, Referencing). Focusable; Enter/Space or ←/→ toggles
    /// it. `summary` shows the stage's current choice when collapsed; `id` is a stable key for the
    /// collapse state and `label` the display name.
    SectionHeader { id: &'static str, label: &'static str, summary: String, collapsed: bool },
    /// Section header "── Mask N ──" (not focusable)
    MaskSectionHeader { section: usize },
    /// "── OR ──" separator between sections (not focusable)
    MaskOrSeparator,
    /// Input source for a mask section (focusable, ←/→ to cycle)
    MaskOpInput { section: usize },
    /// Generator algorithm selector (threshold or BET, ←/→ to switch)
    MaskOpGenerator { section: usize },
    /// Generator parameter (threshold method or BET fractional intensity)
    MaskOpGeneratorParam { section: usize },
    /// Threshold value (only shown for fixed/percentile threshold methods)
    MaskOpThresholdValue { section: usize },
    /// A refinement step (editable, deletable, reorderable)
    MaskOpEntry { section: usize, index: usize },
    /// "Add step..." row for appending new ops to a section
    MaskOpAddStep { section: usize },
    /// "Add mask..." row for adding a new OR'd section
    MaskOpAddSection,
}

pub const MASK_OP_TYPES: &[&str] = &[
    "threshold", "bet", "erode", "dilate", "close", "fill-holes", "gaussian",
];

pub const MASK_PRESET_OPTIONS: &[&str] = &["robust-threshold", "bet", "custom"];
pub const MASK_PRESET_HELP: &[&str] = &[
    "Otsu threshold + dilate + fill holes + erode (recommended for brain)",
    "BET brain extraction + erode",
    "Fully custom mask pipeline (edit steps below)",
];

// ─── Algorithm help text (name + DOI) ───

// One entry per QSM_ALGO_OPTIONS entry, in the same order. Consistent format:
//   "NAME (expansion) — one-line description. https://doi.org/…"
// Deep-learning nets are marked "(deep learning; weights auto-downloaded)". Keep this aligned
// with QSM_ALGO_OPTIONS — the coverage test asserts equal lengths.
const QSM_ALGO_HELP: &[&str] = &[
    "RTS (Rapid Two-Step) — fast two-step k-space inversion with streaking removal. https://doi.org/10.1016/j.neuroimage.2017.11.018",
    "TV-ADMM (Total Variation) — ADMM inversion with a total-variation prior. https://doi.org/10.1002/mrm.25029",
    "TKD (Thresholded K-space Division) — direct k-space division with thresholding. https://doi.org/10.1002/mrm.22135",
    "TSVD (Truncated SVD) — zeroes the small singular values of the dipole kernel. https://doi.org/10.1002/mrm.22135",
    "TGV (Total Generalized Variation) — single-step: unwrapping, background removal and inversion together. https://doi.org/10.1016/j.neuroimage.2015.02.041",
    "Tikhonov — closed-form L2-regularized inversion. https://doi.org/10.1002/jmri.24365",
    "NLTV (Nonlinear Total Variation) — nonlinear data fidelity with a TV prior. https://doi.org/10.1016/j.neuroimage.2017.11.018",
    "MEDI (Morphology Enabled Dipole Inversion) — magnitude-guided edge weighting. https://doi.org/10.1002/mrm.22816",
    "TFI (Total Field Inversion) — inverts the total field directly (its own background removal). https://doi.org/10.1002/mrm.26331",
    "iLSQR — iterative LSQR with streaking-artifact correction. https://doi.org/10.1016/j.neuroimage.2014.12.043",
    "QSMART — two-stage spatially-dependent filtering plus inversion. https://doi.org/10.1016/j.neuroimage.2020.117701",
    "NDI (Nonlinear Dipole Inversion) — gradient-descent nonlinear fit. https://doi.org/10.1002/nbm.4271",
    "FANSI (Nonlinear TV) — fast nonlinear susceptibility inversion with a TV prior. https://doi.org/10.1002/mrm.27073",
    "FANSI (Nonlinear TGV) — FANSI with a total-generalized-variation prior. https://doi.org/10.1002/mrm.27073",
    "L1-QSM (L1 Data Fidelity) — L1 fidelity for streaking suppression. https://doi.org/10.1002/mrm.29057",
    "WH-QSM (Weak-Harmonic QSM) — jointly fits a residual harmonic background field. https://doi.org/10.1002/mrm.27483",
    "HD-QSM (Hybrid Data Fidelity) — combined L1/L2 data fidelity. https://doi.org/10.1002/mrm.29296",
    "AMP-PE (Approximate Message Passing with Parameter Estimation) — self-tuning Bayesian inversion. https://doi.org/10.1002/mrm.29725",
    "xQSM (deep learning; weights auto-downloaded) — octave-convolution U-net dipole inversion. https://doi.org/10.1002/nbm.4461",
    "QSMnet (deep learning; weights auto-downloaded) — 3D U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2018.06.030",
    "QSMnet+ (deep learning; weights auto-downloaded) — QSMnet trained for a wider susceptibility range. https://doi.org/10.1016/j.neuroimage.2020.116579",
    "AutoQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2019.116064",
    "QSMGAN (deep learning; weights auto-downloaded) — GAN-refined U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2019.116389",
    "IR2QSM (deep learning; weights auto-downloaded) — iterative unrolled U-net inversion. https://doi.org/10.1002/mp.17747",
    "LPCNN (deep learning; weights auto-downloaded) — learned-proximal unrolled inversion. https://doi.org/10.1007/978-3-030-59713-9_13",
    "MoDL-QSM (deep learning; weights auto-downloaded) — model-based unrolled inversion (χ33/STI). https://doi.org/10.1016/j.neuroimage.2021.118376",
    "NeXtQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2022.119729",
    "iQSM (deep learning; weights auto-downloaded) — end-to-end reconstruction from wrapped phase. https://doi.org/10.1016/j.neuroimage.2022.119410",
    "iQSM+ (deep learning; weights auto-downloaded) — orientation-adaptive end-to-end reconstruction. https://doi.org/10.1016/j.media.2024.103160",
];
const UNWRAP_HELP: &[&str] = &[
    "ROMEO region-growing unwrapping — https://doi.org/10.1002/mrm.28563",
    "Laplacian phase unwrapping (FFT-based) — https://doi.org/10.1364/OL.28.001194",
];
const BF_HELP: &[&str] = &[
    "Variable-kernel SHARP (V-SHARP) — https://doi.org/10.1002/mrm.23000",
    "Projection onto Dipole Fields (PDF) — https://doi.org/10.1002/nbm.1670",
    "Laplacian Boundary Value (LBV) — https://doi.org/10.1002/nbm.3064",
    "Iterative Spherical Mean Value (iSMV) — https://doi.org/10.1002/mrm.24998",
    "SHARP (Sophisticated Harmonic Artifact Reduction) — https://doi.org/10.1016/j.neuroimage.2010.10.070",
    "Regularized SHARP (RESHARP) with Tikhonov — https://doi.org/10.1002/mrm.25032",
    "HARPERELLA integrated unwrap+BFR — https://doi.org/10.1002/nbm.3056",
    "Improved HARPERELLA (iHARPERELLA) — Li et al., Proc. ISMRM 2015, p.3313",
    "BFRnet deep-learning background removal (weights auto-downloaded) — https://github.com/sunhongfu/BFRnet",
    "iQFM deep-learning joint unwrapping + background removal from phase (weights auto-downloaded) — https://doi.org/10.1016/j.neuroimage.2022.119410",
];
const QSM_REF_HELP: &[&str] = &[
    "Subtract mean susceptibility within mask (recommended)",
    "No referencing (raw susceptibility values)",
];

/// All pipeline form values (algorithms + parameters).
#[derive(Debug, Clone)]
pub struct PipelineFormState {
    /// Which tab's rows to render (QSM vs Separation) — set by the App before routing input.
    pub mode: PipelineTabMode,
    // Algorithm selections (as indices)
    pub qsm_algorithm: usize,
    pub unwrapping_algorithm: usize,
    pub bf_algorithm: usize,
    pub qsm_reference: usize,

    // Field mapping
    pub phase_offset_removal: bool,
    pub bipolar_correction: bool,
    pub romeo_individual: bool,
    pub romeo_correct_global: bool,
    pub romeo_template: String,
    pub b0_estimation: usize,    // 0=weighted_avg, 1=linear_fit
    pub b0_weight_type: usize,   // 0=phase_snr, 1=phase_var, 2=average, 3=tes, 4=mag

    // Parameters (as Strings for text editing)
    pub inhomogeneity_correction: bool,
    pub obliquity_threshold: String,

    // Deep-learning overlap-tiling (empty = off / whole-volume). Shown for tileable DL inversions.
    pub dl_tile_size: String,
    pub dl_tile_halo: String,

    // RTS
    pub rts_delta: String,
    pub rts_mu: String,
    pub rts_tol: String,
    pub rts_rho: String,
    pub rts_max_iter: String,
    pub rts_lsmr_iter: String,

    // TV
    pub tv_lambda: String,
    pub tv_rho: String,
    pub tv_tol: String,
    pub tv_max_iter: String,

    // TKD
    pub tkd_threshold: String,

    // TSVD
    pub tsvd_threshold: String,

    // iLSQR
    pub ilsqr_tol: String,
    pub ilsqr_max_iter: String,

    // TGV
    pub tgv_iterations: String,
    pub tgv_erosions: String,
    pub tgv_alpha1: String,
    pub tgv_alpha0: String,

    // Tikhonov
    pub tikhonov_lambda: String,

    // NLTV
    pub nltv_lambda: String,
    pub nltv_mu: String,
    pub nltv_tol: String,
    pub nltv_max_iter: String,
    pub nltv_newton_iter: String,

    // MEDI
    pub medi_smv: bool,
    pub medi_lambda: String,
    pub medi_max_iter: String,
    pub medi_cg_max_iter: String,
    pub medi_cg_tol: String,
    pub medi_tol: String,
    pub medi_percentage: String,
    pub medi_smv_radius: String,

    // TFI
    pub tfi_lambda: String,
    pub tfi_precond: String,
    pub tfi_max_iter: String,
    pub tfi_cg_max_iter: String,
    pub tfi_cg_tol: String,
    pub tfi_tol: String,
    pub tfi_percentage: String,

    // AMP-PE
    pub amp_pe_wave_order: String,
    pub amp_pe_nlevel: String,
    pub amp_pe_wave_pec: String,
    pub amp_pe_simulated_te: String,
    pub amp_pe_max_linearization_ite: String,
    pub amp_pe_tikhonov_beta: String,

    // V-SHARP
    pub vsharp_threshold: String,
    pub vsharp_max_radius: String,
    pub vsharp_min_radius: String,

    // PDF
    pub pdf_tol: String,

    // LBV
    pub lbv_tol: String,

    // iSMV
    pub ismv_tol: String,
    pub ismv_max_iter: String,
    pub ismv_radius: String,

    // SHARP
    pub sharp_threshold: String,
    pub sharp_radius: String,

    // RESHARP
    pub resharp_radius: String,
    pub resharp_tik_reg: String,
    pub resharp_tol: String,
    pub resharp_max_iter: String,

    // HARPERELLA
    pub harperella_radius: String,
    pub harperella_max_iter: String,
    pub harperella_tol: String,

    // iHARPERELLA
    pub iharperella_radius: String,
    pub iharperella_max_iter: String,
    pub iharperella_tol: String,

    // mSMV boundary-shadow refinement (post-step on the primary BFR)
    pub msmv_refine: bool,
    pub msmv_radius: String,
    pub msmv_maxk: String,

    // QSMART
    pub qsmart_ilsqr_tol: String,
    pub qsmart_ilsqr_max_iter: String,
    pub qsmart_vasc_sphere_radius: String,
    pub qsmart_sdf_spatial_radius: String,
    pub qsmart_inversion: usize,
    pub qsmart_sdf_sigma1_stage1: String,
    pub qsmart_sdf_sigma2_stage1: String,
    pub qsmart_sdf_sigma1_stage2: String,
    pub qsmart_sdf_sigma2_stage2: String,
    pub qsmart_sdf_lower_lim: String,
    pub qsmart_sdf_curv_constant: String,
    pub qsmart_frangi_scale_min: String,
    pub qsmart_frangi_scale_max: String,
    pub qsmart_frangi_scale_ratio: String,
    pub qsmart_frangi_c: String,

    // NDI
    pub ndi_tau: String,
    pub ndi_alpha: String,
    pub ndi_max_iter: String,
    pub ndi_phase_scale: String,

    // FANSI (nlTV / nlTGV — shared)
    pub fansi_alpha1: String,
    pub fansi_mu1: String,
    pub fansi_mu2: String,
    pub fansi_alpha0: String,
    pub fansi_mu0: String,
    pub fansi_max_iter: String,
    pub fansi_tol_update: String,
    pub fansi_tol_delta: String,
    pub fansi_phase_scale: String,

    // L1-QSM
    pub l1qsm_alpha1: String,
    pub l1qsm_mu1: String,
    pub l1qsm_mu2: String,
    pub l1qsm_mu3: String,
    pub l1qsm_lambda: String,
    pub l1qsm_max_iter: String,
    pub l1qsm_tol_update: String,
    pub l1qsm_tol_delta: String,
    pub l1qsm_phase_scale: String,

    // WH-QSM
    pub whqsm_alpha1: String,
    pub whqsm_mu1: String,
    pub whqsm_mu2: String,
    pub whqsm_beta: String,
    pub whqsm_muh: String,
    pub whqsm_max_iter: String,
    pub whqsm_tol_update: String,
    pub whqsm_tol_delta: String,
    pub whqsm_phase_scale: String,

    // HD-QSM
    pub hdqsm_alpha_l2: String,
    pub hdqsm_mu1_l2: String,
    pub hdqsm_mu2: String,
    pub hdqsm_max_iter_l1: String,
    pub hdqsm_max_iter_l2: String,
    pub hdqsm_tol_update: String,

    // BET
    pub bet_fractional_intensity: String,
    pub bet_smoothness: String,
    pub bet_gradient_threshold: String,
    pub bet_iterations: String,
    pub bet_subdivisions: String,

    // QSM toggle
    pub do_qsm: bool,

    // ROMEO
    pub romeo_phase_gradient_coherence: bool,
    pub romeo_mag_coherence: bool,
    pub romeo_mag_weight: bool,

    // Phase offset sigma
    pub phase_offset_sigma: String,

    // Mask sections (OR'd together at runtime)
    pub mask_sections: Vec<crate::pipeline::config::MaskSection>,
    pub mask_preset: usize, // 0=robust threshold, 1=BET, 2=custom
    pub custom_mask_tool: String, // empty=off; "*"=any derivatives tool; else a tool name

    // Separation tab
    pub do_chi_separation: bool,
    pub separation_algorithm: usize, // index into SEP_ALGO_OPTIONS
    pub custom_qsm_tool: String,     // empty=off; "*"=any; else a derivatives tool name
    pub custom_r2_tool: String,
    pub custom_r2prime_tool: String,
    // R2*-QSM
    pub sep_r2starqsm_r_const_3t: String,
    // DECOMPOSE
    pub sep_decompose_n_inner: String,
    pub sep_decompose_chi_bound: String,
    pub sep_decompose_max_lm_iter: String,
    // χ-sep iLSQR
    pub sep_ilsqr_dr_pos: String,
    pub sep_ilsqr_dr_neg: String,
    pub sep_ilsqr_lambda1: String,
    pub sep_ilsqr_percentage: String,
    pub sep_ilsqr_r2p_min: String,
    pub sep_ilsqr_r2p_max: String,
    pub sep_ilsqr_max_iter: String,
    pub sep_ilsqr_tol: String,
    pub sep_ilsqr_cg_max_iter: String,
    pub sep_ilsqr_cg_tol: String,
    // χ-sep MEDI
    pub sep_medi_lambda_para: String,
    pub sep_medi_lambda_dia: String,
    pub sep_medi_lambda_cpl: String,
    pub sep_medi_dr_pos: String,
    pub sep_medi_dr_neg: String,
    pub sep_medi_percentage: String,
    pub sep_medi_cg_tol: String,
    pub sep_medi_cg_max_iter: String,
    pub sep_medi_max_iter: String,
    pub sep_medi_tol: String,
    // WaveSep
    pub sep_wavesep_dr_pos: String,
    pub sep_wavesep_dr_neg: String,
    pub sep_wavesep_alpha: String,
    pub sep_wavesep_lambda: String,
    pub sep_wavesep_wavelet_order: String,
    pub sep_wavesep_max_iter: String,
    pub sep_wavesep_tol: String,
    // HC-ChiSep
    pub sep_hcchisep_dr_pos_3t: String,
    pub sep_hcchisep_bin_hz: String,

    // Pipeline tab UI state
    pub focus: usize,
    pub editing: bool,
    pub cursor: usize,
    pub scroll_offset: usize,
    /// Collapsed QSM-pipeline stage ids (see [`PipelineFormState::SECTION_IDS`]). Collapsed stages
    /// show only their header + a one-line summary; expanded stages show their full controls.
    pub collapsed_sections: std::collections::HashSet<&'static str>,

    // Mask ops editor state
    pub mask_ops_adding: bool,      // true when "Add step..." selector is active
    pub mask_ops_add_idx: usize,    // index into available op types during add
    pub mask_ops_add_section: usize, // which section we're adding to
    pub mask_threshold_value_buf: String, // text buffer for editing threshold value
    pub mask_threshold_editing: bool, // true when editing threshold value
}

impl Default for PipelineFormState {
    fn default() -> Self {
        let sep_r2starqsm = qsm_core::separation::R2starQsmParams::default();
        let sep_decompose = qsm_core::separation::DecomposeParams::default();
        let sep_ilsqr = qsm_core::separation::ChiSepIlsqrParams::default();
        let sep_medi = qsm_core::separation::ChiSepParams::default();
        let sep_wavesep = qsm_core::separation::WaveSepParams::default();
        let sep_hcchisep = qsm_core::separation::HcChisepParams::default();
        let fmt_default = |x: f64| format!("{}", x);
        let rts = qsm_core::inversion::RtsParams::default();
        let tv = qsm_core::inversion::TvParams::default();
        let tkd = qsm_core::inversion::TkdParams::default();
        let tgv = qsm_core::inversion::TgvParams::default();
        let ndi = qsm_core::inversion::NdiParams::default();
        let fansi = qsm_core::inversion::FansiParams::default();
        let l1qsm = qsm_core::inversion::L1QsmParams::default();
        let whqsm = qsm_core::inversion::WhQsmParams::default();
        let hdqsm = qsm_core::inversion::HdQsmParams::default();
        let bet = qsm_core::bet::BetParams::default();
        Self {
            mode: PipelineTabMode::Qsm,
            qsm_algorithm: 0, // rts
            unwrapping_algorithm: 0, // romeo
            bf_algorithm: 0, // vsharp
            qsm_reference: 0, // mean
            phase_offset_removal: true,
            bipolar_correction: false,
            romeo_individual: true,
            romeo_correct_global: true,
            romeo_template: "1".to_string(),
            b0_estimation: 0,    // weighted_avg
            b0_weight_type: 0,   // phase_snr
            inhomogeneity_correction: true,
            obliquity_threshold: "-1".to_string(),
            dl_tile_size: String::new(),
            dl_tile_halo: String::new(),
            rts_delta: format!("{}", rts.delta),
            rts_mu: format!("{}", rts.mu),
            rts_tol: format!("{}", rts.tol),
            rts_rho: format!("{}", rts.rho),
            rts_max_iter: format!("{}", rts.max_iter),
            rts_lsmr_iter: format!("{}", rts.lsmr_iter),
            tv_lambda: format!("{}", tv.lambda),
            tv_rho: format!("{}", tv.rho),
            tv_tol: format!("{}", tv.tol),
            tv_max_iter: format!("{}", tv.max_iter),
            tkd_threshold: format!("{}", tkd.threshold),
            tsvd_threshold: format!("{}", tkd.threshold),
            ilsqr_tol: format!("{}", qsm_core::inversion::IlsqrParams::default().tol),
            ilsqr_max_iter: format!("{}", qsm_core::inversion::IlsqrParams::default().max_iter),
            tgv_iterations: format!("{}", tgv.iterations),
            tgv_erosions: format!("{}", tgv.erosions),
            tgv_alpha1: format!("{}", tgv.alpha1),
            tgv_alpha0: format!("{}", tgv.alpha0),
            tikhonov_lambda: format!("{}", qsm_core::inversion::TikhonovParams::default().lambda),
            nltv_lambda: format!("{}", qsm_core::inversion::NltvParams::default().lambda),
            nltv_mu: format!("{}", qsm_core::inversion::NltvParams::default().mu),
            nltv_tol: format!("{}", qsm_core::inversion::NltvParams::default().tol),
            nltv_max_iter: format!("{}", qsm_core::inversion::NltvParams::default().max_iter),
            nltv_newton_iter: format!("{}", qsm_core::inversion::NltvParams::default().newton_iter),
            medi_smv: qsm_core::inversion::MediParams::default().smv,
            medi_lambda: format!("{}", qsm_core::inversion::MediParams::default().lambda),
            medi_max_iter: format!("{}", qsm_core::inversion::MediParams::default().max_iter),
            medi_cg_max_iter: format!("{}", qsm_core::inversion::MediParams::default().cg_max_iter),
            medi_cg_tol: format!("{}", qsm_core::inversion::MediParams::default().cg_tol),
            medi_tol: format!("{}", qsm_core::inversion::MediParams::default().tol),
            medi_percentage: format!("{}", qsm_core::inversion::MediParams::default().percentage),
            medi_smv_radius: format!("{}", qsm_core::inversion::MediParams::default().smv_radius),
            tfi_lambda: format!("{}", qsm_core::inversion::TfiParams::default().lambda),
            tfi_precond: format!("{}", qsm_core::inversion::TfiParams::default().precond),
            tfi_max_iter: format!("{}", qsm_core::inversion::TfiParams::default().max_iter),
            tfi_cg_max_iter: format!("{}", qsm_core::inversion::TfiParams::default().cg_max_iter),
            tfi_cg_tol: format!("{}", qsm_core::inversion::TfiParams::default().cg_tol),
            tfi_tol: format!("{}", qsm_core::inversion::TfiParams::default().tol),
            tfi_percentage: format!("{}", qsm_core::inversion::TfiParams::default().percentage),
            amp_pe_wave_order: format!("{}", qsm_core::inversion::AmpPeParams::default().wave_order),
            amp_pe_nlevel: format!("{}", qsm_core::inversion::AmpPeParams::default().nlevel),
            amp_pe_wave_pec: format!("{}", qsm_core::inversion::AmpPeParams::default().wave_pec),
            amp_pe_simulated_te: format!("{}", qsm_core::inversion::AmpPeParams::default().simulated_te),
            amp_pe_max_linearization_ite: format!("{}", qsm_core::inversion::AmpPeParams::default().max_linearization_ite),
            amp_pe_tikhonov_beta: format!("{}", qsm_core::inversion::AmpPeParams::default().tikhonov_beta),
            vsharp_threshold: format!("{}", qsm_core::bgremove::VsharpParams::default().threshold),
            vsharp_max_radius: format!("{}", qsm_core::bgremove::VsharpParams::default().max_radius),
            vsharp_min_radius: format!("{}", qsm_core::bgremove::VsharpParams::default().min_radius),
            pdf_tol: format!("{}", qsm_core::bgremove::PdfParams::default().tol),
            lbv_tol: format!("{}", qsm_core::bgremove::LbvParams::default().tol),
            ismv_tol: format!("{}", qsm_core::bgremove::IsmvParams::default().tol),
            ismv_max_iter: format!("{}", qsm_core::bgremove::IsmvParams::default().max_iter),
            ismv_radius: format!("{}", qsm_core::bgremove::IsmvParams::default().radius),
            sharp_threshold: format!("{}", qsm_core::bgremove::SharpParams::default().threshold),
            sharp_radius: format!("{}", qsm_core::bgremove::SharpParams::default().radius),
            resharp_radius: format!("{}", qsm_core::bgremove::ResharpParams::default().radius),
            resharp_tik_reg: format!("{}", qsm_core::bgremove::ResharpParams::default().tik_reg),
            resharp_tol: format!("{}", qsm_core::bgremove::ResharpParams::default().tol),
            resharp_max_iter: format!("{}", qsm_core::bgremove::ResharpParams::default().max_iter),
            harperella_radius: format!("{}", qsm_core::bgremove::HarperellaParams::default().radius),
            harperella_max_iter: format!("{}", qsm_core::bgremove::HarperellaParams::default().max_iter),
            harperella_tol: format!("{}", qsm_core::bgremove::HarperellaParams::default().tol),
            iharperella_radius: format!("{}", qsm_core::bgremove::HarperellaParams::default().radius),
            iharperella_max_iter: format!("{}", qsm_core::bgremove::HarperellaParams::default().max_iter),
            iharperella_tol: format!("{}", qsm_core::bgremove::HarperellaParams::default().tol),
            msmv_refine: false,
            msmv_radius: format!("{}", qsm_core::bgremove::MsmvParams::default().radius),
            msmv_maxk: format!("{}", qsm_core::bgremove::MsmvParams::default().maxk),
            do_qsm: true,
            romeo_phase_gradient_coherence: qsm_core::unwrap::RomeoParams::default().phase_gradient_coherence,
            romeo_mag_coherence: qsm_core::unwrap::RomeoParams::default().mag_coherence,
            romeo_mag_weight: qsm_core::unwrap::RomeoParams::default().mag_weight,
            phase_offset_sigma: {
                let s = qsm_core::utils::PhaseOffsetParams::default().sigma;
                format!("{} {} {}", s[0], s[1], s[2])
            },
            qsmart_ilsqr_tol: format!("{}", qsm_core::utils::QsmartParams::default().ilsqr_tol),
            qsmart_ilsqr_max_iter: format!("{}", qsm_core::utils::QsmartParams::default().ilsqr_max_iter),
            qsmart_vasc_sphere_radius: format!("{}", qsm_core::utils::QsmartParams::default().vasc_sphere_radius),
            qsmart_sdf_spatial_radius: format!("{}", qsm_core::utils::QsmartParams::default().sdf_spatial_radius),
            qsmart_inversion: 0, // ilsqr (default)
            qsmart_sdf_sigma1_stage1: format!("{}", qsm_core::utils::QsmartParams::default().sdf_sigma1_stage1),
            qsmart_sdf_sigma2_stage1: format!("{}", qsm_core::utils::QsmartParams::default().sdf_sigma2_stage1),
            qsmart_sdf_sigma1_stage2: format!("{}", qsm_core::utils::QsmartParams::default().sdf_sigma1_stage2),
            qsmart_sdf_sigma2_stage2: format!("{}", qsm_core::utils::QsmartParams::default().sdf_sigma2_stage2),
            qsmart_sdf_lower_lim: format!("{}", qsm_core::utils::QsmartParams::default().sdf_lower_lim),
            qsmart_sdf_curv_constant: format!("{}", qsm_core::utils::QsmartParams::default().sdf_curv_constant),
            qsmart_frangi_scale_min: format!("{}", qsm_core::utils::QsmartParams::default().frangi_scale_range[0]),
            qsmart_frangi_scale_max: format!("{}", qsm_core::utils::QsmartParams::default().frangi_scale_range[1]),
            qsmart_frangi_scale_ratio: format!("{}", qsm_core::utils::QsmartParams::default().frangi_scale_ratio),
            qsmart_frangi_c: format!("{}", qsm_core::utils::QsmartParams::default().frangi_c),

            ndi_tau: format!("{}", ndi.tau),
            ndi_alpha: format!("{}", ndi.alpha),
            ndi_max_iter: format!("{}", ndi.max_iter),
            ndi_phase_scale: format!("{}", ndi.phase_scale),

            fansi_alpha1: format!("{}", fansi.alpha1),
            fansi_mu1: format!("{}", fansi.mu1),
            fansi_mu2: format!("{}", fansi.mu2),
            fansi_alpha0: format!("{}", fansi.alpha0),
            fansi_mu0: format!("{}", fansi.mu0),
            fansi_max_iter: format!("{}", fansi.max_iter),
            fansi_tol_update: format!("{}", fansi.tol_update),
            fansi_tol_delta: format!("{}", fansi.tol_delta),
            fansi_phase_scale: format!("{}", fansi.phase_scale),

            l1qsm_alpha1: format!("{}", l1qsm.alpha1),
            l1qsm_mu1: format!("{}", l1qsm.mu1),
            l1qsm_mu2: format!("{}", l1qsm.mu2),
            l1qsm_mu3: format!("{}", l1qsm.mu3),
            l1qsm_lambda: format!("{}", l1qsm.lambda),
            l1qsm_max_iter: format!("{}", l1qsm.max_iter),
            l1qsm_tol_update: format!("{}", l1qsm.tol_update),
            l1qsm_tol_delta: format!("{}", l1qsm.tol_delta),
            l1qsm_phase_scale: format!("{}", l1qsm.phase_scale),

            whqsm_alpha1: format!("{}", whqsm.alpha1),
            whqsm_mu1: format!("{}", whqsm.mu1),
            whqsm_mu2: format!("{}", whqsm.mu2),
            whqsm_beta: format!("{}", whqsm.beta),
            whqsm_muh: format!("{}", whqsm.muh),
            whqsm_max_iter: format!("{}", whqsm.max_iter),
            whqsm_tol_update: format!("{}", whqsm.tol_update),
            whqsm_tol_delta: format!("{}", whqsm.tol_delta),
            whqsm_phase_scale: format!("{}", whqsm.phase_scale),

            hdqsm_alpha_l2: format!("{}", hdqsm.alpha_l2),
            hdqsm_mu1_l2: format!("{}", hdqsm.mu1_l2),
            hdqsm_mu2: format!("{}", hdqsm.mu2),
            hdqsm_max_iter_l1: format!("{}", hdqsm.max_iter_l1),
            hdqsm_max_iter_l2: format!("{}", hdqsm.max_iter_l2),
            hdqsm_tol_update: format!("{}", hdqsm.tol_update),

            bet_fractional_intensity: format!("{}", bet.fractional_intensity),
            bet_smoothness: format!("{}", bet.smoothness),
            bet_gradient_threshold: format!("{}", bet.gradient_threshold),
            bet_iterations: format!("{}", bet.iterations),
            bet_subdivisions: format!("{}", bet.subdivisions),
            mask_sections: crate::pipeline::config::default_mask_sections(),
            mask_preset: 0, // robust threshold
            custom_mask_tool: String::new(),
            do_chi_separation: false,
            separation_algorithm: 0,
            custom_qsm_tool: String::new(),
            custom_r2_tool: String::new(),
            custom_r2prime_tool: String::new(),
            sep_r2starqsm_r_const_3t: fmt_default(sep_r2starqsm.r_const_3t),
            sep_decompose_n_inner: sep_decompose.n_inner.to_string(),
            sep_decompose_chi_bound: fmt_default(sep_decompose.chi_bound),
            sep_decompose_max_lm_iter: sep_decompose.max_lm_iter.to_string(),
            sep_ilsqr_dr_pos: fmt_default(sep_ilsqr.dr_pos),
            sep_ilsqr_dr_neg: fmt_default(sep_ilsqr.dr_neg),
            sep_ilsqr_lambda1: fmt_default(sep_ilsqr.lambda1),
            sep_ilsqr_percentage: fmt_default(sep_ilsqr.percentage),
            sep_ilsqr_r2p_min: fmt_default(sep_ilsqr.r2p_min),
            sep_ilsqr_r2p_max: fmt_default(sep_ilsqr.r2p_max),
            sep_ilsqr_max_iter: sep_ilsqr.max_iter.to_string(),
            sep_ilsqr_tol: fmt_default(sep_ilsqr.tol),
            sep_ilsqr_cg_max_iter: sep_ilsqr.cg_max_iter.to_string(),
            sep_ilsqr_cg_tol: fmt_default(sep_ilsqr.cg_tol),
            sep_medi_lambda_para: fmt_default(sep_medi.lambda_para),
            sep_medi_lambda_dia: fmt_default(sep_medi.lambda_dia),
            sep_medi_lambda_cpl: fmt_default(sep_medi.lambda_cpl),
            sep_medi_dr_pos: fmt_default(sep_medi.dr_pos),
            sep_medi_dr_neg: fmt_default(sep_medi.dr_neg),
            sep_medi_percentage: fmt_default(sep_medi.percentage),
            sep_medi_cg_tol: fmt_default(sep_medi.cg_tol),
            sep_medi_cg_max_iter: sep_medi.cg_max_iter.to_string(),
            sep_medi_max_iter: sep_medi.max_iter.to_string(),
            sep_medi_tol: fmt_default(sep_medi.tol),
            sep_wavesep_dr_pos: fmt_default(sep_wavesep.dr_pos),
            sep_wavesep_dr_neg: fmt_default(sep_wavesep.dr_neg),
            sep_wavesep_alpha: fmt_default(sep_wavesep.alpha),
            sep_wavesep_lambda: fmt_default(sep_wavesep.lambda),
            sep_wavesep_wavelet_order: sep_wavesep.wavelet_order.to_string(),
            sep_wavesep_max_iter: sep_wavesep.max_iter.to_string(),
            sep_wavesep_tol: fmt_default(sep_wavesep.tol),
            sep_hcchisep_dr_pos_3t: fmt_default(sep_hcchisep.dr_pos_3t),
            sep_hcchisep_bin_hz: fmt_default(sep_hcchisep.bin_hz),
            focus: 0,
            editing: false,
            cursor: 0,
            // Start with every stage collapsed → the page opens as a compact pipeline overview.
            collapsed_sections: Self::SECTION_IDS.iter().copied().collect(),
            scroll_offset: 0,
            mask_ops_adding: false,
            mask_ops_add_idx: 0,
            mask_ops_add_section: 0,
            mask_threshold_value_buf: String::new(),
            mask_threshold_editing: false,
        }
    }
}

pub const QSM_ALGO_OPTIONS: &[&str] = &["rts", "tv", "tkd", "tsvd", "tgv", "tikhonov", "nltv", "medi", "tfi", "ilsqr", "qsmart", "ndi", "fansi", "fansi-tgv", "l1qsm", "whqsm", "hdqsm", "amp-pe", "xqsm", "qsmnet", "qsmnet-plus", "autoqsm", "qsmgan", "ir2qsm", "lpcnn", "modl-qsm", "nextqsm", "iqsm", "iqsm-plus"];
pub const SEP_ALGO_OPTIONS: &[&str] = &["r2star-qsm", "decompose", "chi-sep-ilsqr", "chi-sep-medi", "wavesep", "hc-chisep", "susep-net", "chi-sepnet"];
const SEP_ALGO_HELP: &[&str] = &[
    "R2*-QSM closed-form (Dimov 2022) — QSM + R2*, GRE-only",
    "DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude, GRE-only",
    "χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM",
    "χ-separation MEDI — local field + R2' + magnitude",
    "WaveSep wavelet-L1 (Fang 2023) — QSM + R2'",
    "Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude",
    "SUSEP-Net deep-learning separation — QSM + R2' + local field (weights auto-downloaded)",
    "χ-sepnet deep-learning separation — QSM + R2' + local field (weights auto-downloaded)",
];
// QSMART's inner dipole inversion (excludes the two end-to-end algorithms tgv/qsmart).
pub const QSMART_INV_OPTIONS: &[&str] = &["ilsqr", "rts", "tv", "tkd", "tsvd", "tikhonov", "nltv", "medi"];
const QSMART_INV_HELP: &[&str] = &[
    "iLSQR (default) — QSMART's original inner inversion",
    "Rapid Two-Step (RTS)",
    "Total Variation ADMM (TV)",
    "Truncated K-space Division (TKD)",
    "Truncated SVD (TSVD)",
    "Tikhonov L2 regularization",
    "Nonlinear Total Variation (NLTV)",
    "Morphology Enabled Dipole Inversion (MEDI)",
];
pub const UNWRAP_OPTIONS: &[&str] = &["romeo", "laplacian"];
pub const BF_OPTIONS: &[&str] = &["vsharp", "pdf", "lbv", "ismv", "sharp", "resharp", "harperella", "iharperella", "bfrnet", "iqfm"];
pub const B0_ESTIMATION_OPTIONS: &[&str] = &["weighted-avg", "linear-fit"];
pub const B0_WEIGHT_TYPE_OPTIONS: &[&str] = &["phase-snr", "phase-var", "average", "tes", "mag"];
const B0_ESTIMATION_HELP: &[&str] = &[
    "Weighted average of phase/TE across echoes (default)",
    "Magnitude-weighted linear fit of phase vs TE",
];
const B0_WEIGHT_TYPE_HELP: &[&str] = &[
    "mag × TE — optimal for phase SNR (default)",
    "mag² × TE² — based on phase variance",
    "Uniform weights (unweighted average)",
    "TE only",
    "Magnitude only",
];
pub const QSM_REF_OPTIONS: &[&str] = &["mean", "none"];

impl PipelineFormState {
    /// Build the visible rows based on current algorithm selections.
    /// Rows for the Separation tab: enable toggle, method selector, the selected method's
    /// parameters, and bring-your-own input tool fields.
    fn separation_visible_rows(&self) -> Vec<PipelineRow> {
        let mut rows = Vec::new();
        rows.push(PipelineRow::Toggle {
            label: "Compute χ-separation", field: "do_chi_separation",
            help: "Split QSM into paramagnetic (χ+, iron) and diamagnetic (χ−, myelin) maps",
        });
        if !self.do_chi_separation {
            return rows;
        }
        rows.push(PipelineRow::Separator);
        rows.push(PipelineRow::AlgoSelect {
            label: "Method", field: "separation_algorithm",
            options: SEP_ALGO_OPTIONS, help: SEP_ALGO_HELP,
        });
        rows.push(PipelineRow::Separator);
        match self.separation_algorithm {
            0 => {
                rows.push(PipelineRow::Param { label: "  Relaxometric const @3T", field: "sep_r2starqsm_r_const_3t", help: "Hz/ppm at 3T (scales with B0)" });
            }
            1 => {
                rows.push(PipelineRow::Param { label: "  Inner passes", field: "sep_decompose_n_inner", help: "Alternating 3-stage fit passes per voxel" });
                rows.push(PipelineRow::Param { label: "  Chi bound (ppm)", field: "sep_decompose_chi_bound", help: "Upper bound on |χ| in the fit" });
                rows.push(PipelineRow::Param { label: "  Max LM iter", field: "sep_decompose_max_lm_iter", help: "Levenberg–Marquardt max iterations" });
            }
            2 => {
                rows.push(PipelineRow::Param { label: "  Dr+ (Hz/ppm)", field: "sep_ilsqr_dr_pos", help: "Paramagnetic relaxometric constant" });
                rows.push(PipelineRow::Param { label: "  Dr- (Hz/ppm)", field: "sep_ilsqr_dr_neg", help: "Diamagnetic relaxometric constant" });
                rows.push(PipelineRow::Param { label: "  Lambda1", field: "sep_ilsqr_lambda1", help: "L1 edge-masked TV weight" });
                rows.push(PipelineRow::Param { label: "  Percentage", field: "sep_ilsqr_percentage", help: "Edge-mask keep fraction (0-1)" });
                rows.push(PipelineRow::Param { label: "  R2' min (Hz)", field: "sep_ilsqr_r2p_min", help: "R2' reliability window lower" });
                rows.push(PipelineRow::Param { label: "  R2' max (Hz)", field: "sep_ilsqr_r2p_max", help: "R2' reliability window upper" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "sep_ilsqr_max_iter", help: "Outer Gauss-Newton iterations" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "sep_ilsqr_tol", help: "Outer relative-change tolerance" });
                rows.push(PipelineRow::Param { label: "  CG Max Iter", field: "sep_ilsqr_cg_max_iter", help: "Inner CG max iterations" });
                rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "sep_ilsqr_cg_tol", help: "Inner CG relative tolerance" });
            }
            3 => {
                rows.push(PipelineRow::Param { label: "  Lambda para", field: "sep_medi_lambda_para", help: "Paramagnetic L1 weight" });
                rows.push(PipelineRow::Param { label: "  Lambda dia", field: "sep_medi_lambda_dia", help: "Diamagnetic L1 weight" });
                rows.push(PipelineRow::Param { label: "  Lambda cpl", field: "sep_medi_lambda_cpl", help: "Field/R2' coupling weight" });
                rows.push(PipelineRow::Param { label: "  Dr+ (Hz/ppm)", field: "sep_medi_dr_pos", help: "Paramagnetic relaxivity" });
                rows.push(PipelineRow::Param { label: "  Dr- (Hz/ppm)", field: "sep_medi_dr_neg", help: "Diamagnetic relaxivity" });
                rows.push(PipelineRow::Param { label: "  Percentage", field: "sep_medi_percentage", help: "Edge-mask percentage (0-1)" });
                rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "sep_medi_cg_tol", help: "Inner CG tolerance" });
                rows.push(PipelineRow::Param { label: "  CG Max Iter", field: "sep_medi_cg_max_iter", help: "Inner CG max iterations" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "sep_medi_max_iter", help: "Outer Gauss-Newton iterations" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "sep_medi_tol", help: "Outer convergence tolerance" });
            }
            4 => {
                rows.push(PipelineRow::Param { label: "  Dr+ (Hz/ppm)", field: "sep_wavesep_dr_pos", help: "Paramagnetic relaxivity" });
                rows.push(PipelineRow::Param { label: "  Dr- (Hz/ppm)", field: "sep_wavesep_dr_neg", help: "Diamagnetic relaxivity" });
                rows.push(PipelineRow::Param { label: "  Alpha", field: "sep_wavesep_alpha", help: "Proximal-gradient step size" });
                rows.push(PipelineRow::Param { label: "  Lambda", field: "sep_wavesep_lambda", help: "Wavelet L1 soft-threshold weight" });
                rows.push(PipelineRow::Param { label: "  Wavelet order", field: "sep_wavesep_wavelet_order", help: "Daubechies order" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "sep_wavesep_max_iter", help: "ISTA max iterations" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "sep_wavesep_tol", help: "Relative-change stop tolerance" });
            }
            _ => {
                rows.push(PipelineRow::Param { label: "  Dr+ @3T (Hz/ppm)", field: "sep_hcchisep_dr_pos_3t", help: "Paramagnetic relaxivity at 3T" });
                rows.push(PipelineRow::Param { label: "  R2' bin (Hz)", field: "sep_hcchisep_bin_hz", help: "R2' bin width for anchored grid search" });
            }
        }
        rows.push(PipelineRow::Separator);
        rows.push(PipelineRow::Note { text: "Bring-your-own inputs (derivatives tool name; blank = compute, * = any tool):" });
        rows.push(PipelineRow::Param { label: "  Custom QSM tool", field: "custom_qsm_tool", help: "Use a Chimap from <bids>/derivatives/<tool>/ instead of recomputing QSM" });
        rows.push(PipelineRow::Param { label: "  Custom R2 tool", field: "custom_r2_tool", help: "Use an R2 map from derivatives instead of computing from MESE" });
        rows.push(PipelineRow::Param { label: "  Custom R2' tool", field: "custom_r2prime_tool", help: "Use an R2' map from derivatives instead of computing" });
        rows
    }

    /// Stable ids of the collapsible QSM-pipeline stages, top to bottom.
    pub const SECTION_IDS: &[&'static str] =
        &["general", "fieldmap", "masking", "bgremoval", "inversion", "reference"];

    /// Build a stage header row, reading the current collapse state for `id`.
    fn section_header(&self, id: &'static str, label: &'static str, summary: String) -> PipelineRow {
        PipelineRow::SectionHeader {
            id,
            label,
            summary,
            collapsed: self.collapsed_sections.contains(id),
        }
    }

    /// Toggle a stage's collapsed state.
    pub fn toggle_section(&mut self, id: &'static str) {
        if !self.collapsed_sections.remove(id) {
            self.collapsed_sections.insert(id);
        }
    }

    /// Drop the body rows of collapsed sections: keep every `SectionHeader`, and skip the rows
    /// after a collapsed header until the next header. Rows before the first header are always kept.
    fn apply_section_collapse(rows: Vec<PipelineRow>) -> Vec<PipelineRow> {
        let mut out = Vec::with_capacity(rows.len());
        let mut skipping = false;
        for r in rows {
            if let PipelineRow::SectionHeader { collapsed, .. } = &r {
                skipping = *collapsed;
                out.push(r);
            } else if !skipping {
                out.push(r);
            }
        }
        out
    }

    pub fn visible_rows(&self) -> Vec<PipelineRow> {
        if self.mode == PipelineTabMode::Separation {
            return self.separation_visible_rows();
        }
        let mut rows = Vec::new();
        let is_tgv = self.qsm_algorithm == 4;
        let is_qsmart = self.qsm_algorithm == 10;
        let is_medi_smv = self.qsm_algorithm == 7 && self.medi_smv;
        // AutoQSM / NeXtQSM are single-step (own background removal from the total field), so
        // the BG-removal section is hidden — but field mapping/unwrapping still runs.
        let is_autoqsm = self.qsm_algorithm == 21;
        let is_nextqsm = self.qsm_algorithm == 26;
        let is_single_step = is_autoqsm || is_nextqsm;
        // iQSM / iQSM+ reconstruct end-to-end from raw phase: no field mapping or BG removal.
        let is_iqsm = self.qsm_algorithm == 27 || self.qsm_algorithm == 28;

        // QSM toggle
        rows.push(PipelineRow::Toggle {
            label: "Compute QSM", field: "do_qsm",
            help: "Enable QSM reconstruction (disable to only run supplementary outputs)",
        });

        rows.push(PipelineRow::Separator);

        // General settings (QSM-only)
        if self.do_qsm {
        rows.push(self.section_header("general", "General",
            format!("inhomogeneity correction {}", if self.inhomogeneity_correction { "on" } else { "off" })));
        rows.push(PipelineRow::Param {
            label: "Obliquity", field: "obliquity_threshold",
            help: "Resample oblique acquisitions to axial if obliquity exceeds this (degrees, -1 = disabled)",
        });
        rows.push(PipelineRow::Toggle {
            label: "Inhomog. Correction", field: "inhomogeneity_correction",
            help: "Apply B1 field correction to magnitude (improves masking, ROMEO weights, MEDI edges, SWI)",
        });

        rows.push(PipelineRow::Separator);
        } // end if do_qsm (general settings)

        if self.do_qsm {
        // Field Mapping (hidden if TGV or QSMART)
        if !is_tgv && !is_qsmart && !is_iqsm {
            let is_laplacian = self.unwrapping_algorithm == 1;
            rows.push(self.section_header("fieldmap", "Field mapping",
                format!("{} unwrapping", UNWRAP_OPTIONS.get(self.unwrapping_algorithm).copied().unwrap_or(""))));

            // Phase offset removal (disabled for Laplacian)
            if !is_laplacian {
                rows.push(PipelineRow::Toggle {
                    label: "Phase Offset Removal", field: "phase_offset_removal",
                    help: "Remove spatially-varying phase offset using HIP (Eckstein et al., 2018)",
                });
                if self.phase_offset_removal {
                    rows.push(PipelineRow::Param { label: "  Sigma", field: "phase_offset_sigma",
                        help: "Gaussian smoothing sigma in voxels (x y z)" });
                }

                // Bipolar correction (disabled for Laplacian)
                rows.push(PipelineRow::Toggle {
                    label: "Bipolar Correction", field: "bipolar_correction",
                    help: "Remove linear phase artefact from bipolar readout gradients (requires 3+ echoes)",
                });
            }

            // Unwrapping
            rows.push(PipelineRow::AlgoSelect {
                label: "Unwrapping", field: "unwrapping_algorithm",
                options: UNWRAP_OPTIONS, help: UNWRAP_HELP,
            });

            // ROMEO multi-echo params (only for ROMEO)
            if !is_laplacian {
                rows.push(PipelineRow::Toggle {
                    label: "  Individual Mode", field: "romeo_individual",
                    help: "Unwrap each echo independently (recommended). Off = template-based temporal unwrapping.",
                });
                if !self.romeo_individual {
                    rows.push(PipelineRow::Param {
                        label: "  Template Echo", field: "romeo_template",
                        help: "Echo index for spatial unwrapping (1-indexed)",
                    });
                }
                rows.push(PipelineRow::Toggle {
                    label: "  Correct Global", field: "romeo_correct_global",
                    help: "Correct inter-echo 2π offsets after unwrapping",
                });
            }

            // B0 estimation
            rows.push(PipelineRow::AlgoSelect {
                label: "B0 Estimation", field: "b0_estimation",
                options: B0_ESTIMATION_OPTIONS, help: B0_ESTIMATION_HELP,
            });
            if self.b0_estimation == 0 { // weighted_avg
                rows.push(PipelineRow::AlgoSelect {
                    label: "  Weight Type", field: "b0_weight_type",
                    options: B0_WEIGHT_TYPE_OPTIONS, help: B0_WEIGHT_TYPE_HELP,
                });
            }

            rows.push(PipelineRow::Separator);
        }

        // Mask preset selector (always visible — needed for SWI/T2*/R2* too)
        rows.push(self.section_header("masking", "Masking",
            MASK_PRESET_OPTIONS.get(self.get_select("mask_preset")).copied().unwrap_or("").to_string()));
        rows.push(PipelineRow::AlgoSelect {
            label: "Mask Preset", field: "mask_preset",
            options: MASK_PRESET_OPTIONS, help: MASK_PRESET_HELP,
        });
        rows.push(PipelineRow::Param {
            label: "Use custom masks", field: "custom_mask_tool",
            help: "BYO mask from BIDS derivatives: empty=off, * =any tool, or a tool name (e.g. bet); falls back if absent",
        });

        // Mask sections
        let multi_section = self.mask_sections.len() > 1;
        for si in 0..self.mask_sections.len() {
            if si > 0 {
                rows.push(PipelineRow::MaskOrSeparator);
            }
            if multi_section {
                rows.push(PipelineRow::MaskSectionHeader { section: si });
            }
            rows.push(PipelineRow::MaskOpInput { section: si });
            rows.push(PipelineRow::MaskOpGenerator { section: si });
            rows.push(PipelineRow::MaskOpGeneratorParam { section: si });
            if let crate::pipeline::config::MaskOp::Threshold { method, .. } = &self.mask_sections[si].generator {
                if matches!(method, crate::pipeline::config::MaskThresholdMethod::Fixed | crate::pipeline::config::MaskThresholdMethod::Percentile) {
                    rows.push(PipelineRow::MaskOpThresholdValue { section: si });
                }
            }
            for oi in 0..self.mask_sections[si].refinements.len() {
                rows.push(PipelineRow::MaskOpEntry { section: si, index: oi });
            }
            rows.push(PipelineRow::MaskOpAddStep { section: si });
        }
        rows.push(PipelineRow::MaskOpAddSection);

        rows.push(PipelineRow::Separator);

        // BG Removal (hidden for TGV, QSMART, MEDI+SMV, single-step DL, and end-to-end iQSM)
        if !is_tgv && !is_qsmart && !is_medi_smv && !is_single_step && !is_iqsm {
            rows.push(self.section_header("bgremoval", "Background removal",
                BF_OPTIONS.get(self.bf_algorithm).copied().unwrap_or("").to_string()));
            rows.push(PipelineRow::AlgoSelect {
                label: "BG Removal", field: "bf_algorithm",
                options: BF_OPTIONS, help: BF_HELP,
            });
            match self.bf_algorithm {
                0 => { // V-SHARP
                    rows.push(PipelineRow::Param { label: "  Threshold", field: "vsharp_threshold", help: "Deconvolution threshold" });
                    rows.push(PipelineRow::Param { label: "  Max Radius", field: "vsharp_max_radius", help: "Largest SMV kernel radius (mm)" });
                    rows.push(PipelineRow::Param { label: "  Min Radius", field: "vsharp_min_radius", help: "Smallest SMV kernel radius (mm)" });
                }
                1 => { // PDF
                    rows.push(PipelineRow::Param { label: "  Tolerance", field: "pdf_tol", help: "Convergence tolerance" });
                }
                2 => { // LBV
                    rows.push(PipelineRow::Param { label: "  Tolerance", field: "lbv_tol", help: "Convergence tolerance" });
                }
                3 => { // iSMV
                    rows.push(PipelineRow::Param { label: "  Tolerance", field: "ismv_tol", help: "Convergence tolerance" });
                    rows.push(PipelineRow::Param { label: "  Max Iter", field: "ismv_max_iter", help: "Maximum iterations" });
                    rows.push(PipelineRow::Param { label: "  Radius", field: "ismv_radius", help: "SMV kernel radius (mm)" });
                }
                4 => { // SHARP
                    rows.push(PipelineRow::Param { label: "  Threshold", field: "sharp_threshold", help: "Deconvolution threshold" });
                    rows.push(PipelineRow::Param { label: "  Radius", field: "sharp_radius", help: "SMV kernel radius (mm)" });
                }
                5 => { // RESHARP
                    rows.push(PipelineRow::Param { label: "  Radius", field: "resharp_radius", help: "SMV kernel radius in mm" });
                    rows.push(PipelineRow::Param { label: "  Tik Reg", field: "resharp_tik_reg", help: "Tikhonov regularization parameter" });
                    rows.push(PipelineRow::Param { label: "  Tolerance", field: "resharp_tol", help: "CG convergence tolerance" });
                    rows.push(PipelineRow::Param { label: "  Max Iter", field: "resharp_max_iter", help: "Maximum CG iterations" });
                }
                6 => { // HARPERELLA
                    rows.push(PipelineRow::Param { label: "  Radius", field: "harperella_radius", help: "SMV kernel radius in mm" });
                    rows.push(PipelineRow::Param { label: "  Max Iter", field: "harperella_max_iter", help: "Maximum iterations" });
                    rows.push(PipelineRow::Param { label: "  Tolerance", field: "harperella_tol", help: "Convergence tolerance" });
                }
                7 => { // iHARPERELLA
                    rows.push(PipelineRow::Param { label: "  Radius", field: "iharperella_radius", help: "SMV kernel radius in mm" });
                    rows.push(PipelineRow::Param { label: "  Max Iter", field: "iharperella_max_iter", help: "Maximum iterations" });
                    rows.push(PipelineRow::Param { label: "  Tolerance", field: "iharperella_tol", help: "Convergence tolerance" });
                }
                _ => {}
            }
            // mSMV boundary-shadow refinement, layered on top of the primary BFR (Roberts 2024).
            rows.push(PipelineRow::Toggle {
                label: "mSMV refinement", field: "msmv_refine",
                help: "Remove residual boundary-field shadows with mSMV after the primary BFR (Roberts 2024)",
            });
            if self.msmv_refine {
                rows.push(PipelineRow::Param { label: "  mSMV Radius", field: "msmv_radius", help: "SMV prefilter kernel radius in mm" });
                rows.push(PipelineRow::Param { label: "  mSMV Max Iter", field: "msmv_maxk", help: "Maximum boundary-correction iterations" });
            }
            rows.push(PipelineRow::Separator);
        }

        // QSM Inversion
        rows.push(self.section_header("inversion", "Dipole inversion",
            QSM_ALGO_OPTIONS.get(self.qsm_algorithm).copied().unwrap_or("").to_string()));
        rows.push(PipelineRow::AlgoSelect {
            label: "QSM Inversion", field: "qsm_algorithm",
            options: QSM_ALGO_OPTIONS, help: QSM_ALGO_HELP,
        });

        // Algorithm-specific params
        match self.qsm_algorithm {
            0 => { // RTS
                rows.push(PipelineRow::Param { label: "  Delta", field: "rts_delta", help: "Threshold for ill-conditioned k-space region" });
                rows.push(PipelineRow::Param { label: "  Mu", field: "rts_mu", help: "Regularization parameter for well-conditioned region" });
                rows.push(PipelineRow::Param { label: "  Rho", field: "rts_rho", help: "ADMM penalty parameter" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "rts_tol", help: "Convergence tolerance (relative change)" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "rts_max_iter", help: "Maximum ADMM iterations" });
                rows.push(PipelineRow::Param { label: "  LSMR Iter", field: "rts_lsmr_iter", help: "LSMR iterations for step 1 (well-conditioned solve)" });
            }
            1 => { // TV
                rows.push(PipelineRow::Param { label: "  Lambda", field: "tv_lambda", help: "L1 regularization weight (smaller = smoother)" });
                rows.push(PipelineRow::Param { label: "  Rho", field: "tv_rho", help: "ADMM penalty parameter (typically 100×lambda)" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "tv_tol", help: "Convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "tv_max_iter", help: "Maximum ADMM iterations" });
            }
            2 => { // TKD
                rows.push(PipelineRow::Param { label: "  Threshold", field: "tkd_threshold", help: "Truncation threshold for k-space division (0.1-0.2)" });
            }
            3 => { // TSVD
                rows.push(PipelineRow::Param { label: "  Threshold", field: "tsvd_threshold", help: "Truncation threshold for SVD (0.1-0.2)" });
            }
            4 => { // TGV
                rows.push(PipelineRow::Param { label: "  Iterations", field: "tgv_iterations", help: "Primal-dual iterations" });
                rows.push(PipelineRow::Param { label: "  Erosions", field: "tgv_erosions", help: "Mask erosions before TGV solve" });
                rows.push(PipelineRow::Param { label: "  Alpha1", field: "tgv_alpha1", help: "First-order TGV weight (gradient term)" });
                rows.push(PipelineRow::Param { label: "  Alpha0", field: "tgv_alpha0", help: "Second-order TGV weight (symmetric gradient term)" });
            }
            5 => { // Tikhonov
                rows.push(PipelineRow::Param { label: "  Lambda", field: "tikhonov_lambda", help: "L2 regularization weight" });
            }
            6 => { // NLTV
                rows.push(PipelineRow::Param { label: "  Lambda", field: "nltv_lambda", help: "Regularization parameter" });
                rows.push(PipelineRow::Param { label: "  Mu", field: "nltv_mu", help: "Penalty parameter" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "nltv_tol", help: "Convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "nltv_max_iter", help: "Maximum ADMM iterations" });
                rows.push(PipelineRow::Param { label: "  Newton Iter", field: "nltv_newton_iter", help: "Newton iterations for weight update" });
            }
            7 => { // MEDI
                rows.push(PipelineRow::Toggle { label: "  SMV Mode", field: "medi_smv",
                    help: "MEDI handles background removal internally using spherical mean value preprocessing (skips the BG removal step)" });
                rows.push(PipelineRow::Param { label: "  SMV Radius", field: "medi_smv_radius", help: "SMV preprocessing radius in mm" });
                rows.push(PipelineRow::Param { label: "  Lambda", field: "medi_lambda", help: "Regularization weight" });
                rows.push(PipelineRow::Param { label: "  Percentage", field: "medi_percentage", help: "Fraction of voxels considered edges (0.0-1.0)" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "medi_max_iter", help: "Maximum outer iterations" });
                rows.push(PipelineRow::Param { label: "  CG Max Iter", field: "medi_cg_max_iter", help: "Maximum conjugate gradient iterations" });
                rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "medi_cg_tol", help: "CG convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "medi_tol", help: "Outer convergence tolerance" });
            }
            8 => { // TFI
                rows.push(PipelineRow::Param { label: "  Lambda", field: "tfi_lambda", help: "Regularization weight" });
                rows.push(PipelineRow::Param { label: "  Precond", field: "tfi_precond", help: "Preconditioner value (susceptibility scaling outside the mask)" });
                rows.push(PipelineRow::Param { label: "  Percentage", field: "tfi_percentage", help: "Fraction of voxels considered edges (0.0-1.0)" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "tfi_max_iter", help: "Maximum outer iterations" });
                rows.push(PipelineRow::Param { label: "  CG Max Iter", field: "tfi_cg_max_iter", help: "Maximum conjugate gradient iterations" });
                rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "tfi_cg_tol", help: "CG convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "tfi_tol", help: "Outer convergence tolerance" });
            }
            9 => { // iLSQR
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "ilsqr_tol", help: "Convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "ilsqr_max_iter", help: "Maximum iterations" });
            }
            10 => { // QSMART
                rows.push(PipelineRow::Note {
                    text: "⚠ QSMART needs a tight BET brain mask — loose masks cause streaking (set mask below)",
                });
                rows.push(PipelineRow::AlgoSelect {
                    label: "  Inner Inversion", field: "qsmart_inversion",
                    options: QSMART_INV_OPTIONS, help: QSMART_INV_HELP,
                });
                // Params for the selected inner inversion. QSMART_INV_OPTIONS order:
                // 0 ilsqr, 1 rts, 2 tv, 3 tkd, 4 tsvd, 5 tikhonov, 6 nltv, 7 medi.
                // Non-iLSQR algorithms reuse the shared inversion params (config.inversion.<algo>),
                // which run_qsmart applies to both QSMART stages.
                match self.qsmart_inversion {
                    0 => { // iLSQR (uses QSMART's own tol/max_iter)
                        rows.push(PipelineRow::Param { label: "  iLSQR Tol", field: "qsmart_ilsqr_tol", help: "iLSQR convergence tolerance" });
                        rows.push(PipelineRow::Param { label: "  iLSQR Max Iter", field: "qsmart_ilsqr_max_iter", help: "Maximum iLSQR iterations per stage" });
                    }
                    1 => { // RTS
                        rows.push(PipelineRow::Param { label: "  Delta", field: "rts_delta", help: "Threshold for ill-conditioned k-space region" });
                        rows.push(PipelineRow::Param { label: "  Mu", field: "rts_mu", help: "Regularization parameter for well-conditioned region" });
                        rows.push(PipelineRow::Param { label: "  Rho", field: "rts_rho", help: "ADMM penalty parameter" });
                        rows.push(PipelineRow::Param { label: "  Tolerance", field: "rts_tol", help: "Convergence tolerance (relative change)" });
                        rows.push(PipelineRow::Param { label: "  Max Iter", field: "rts_max_iter", help: "Maximum ADMM iterations" });
                        rows.push(PipelineRow::Param { label: "  LSMR Iter", field: "rts_lsmr_iter", help: "LSMR iterations for step 1 (well-conditioned solve)" });
                    }
                    2 => { // TV
                        rows.push(PipelineRow::Param { label: "  Lambda", field: "tv_lambda", help: "L1 regularization weight (smaller = smoother)" });
                        rows.push(PipelineRow::Param { label: "  Rho", field: "tv_rho", help: "ADMM penalty parameter (typically 100×lambda)" });
                        rows.push(PipelineRow::Param { label: "  Tolerance", field: "tv_tol", help: "Convergence tolerance" });
                        rows.push(PipelineRow::Param { label: "  Max Iter", field: "tv_max_iter", help: "Maximum ADMM iterations" });
                    }
                    3 => { // TKD
                        rows.push(PipelineRow::Param { label: "  Threshold", field: "tkd_threshold", help: "Truncation threshold for k-space division (0.1-0.2)" });
                    }
                    4 => { // TSVD
                        rows.push(PipelineRow::Param { label: "  Threshold", field: "tsvd_threshold", help: "Truncation threshold for SVD (0.1-0.2)" });
                    }
                    5 => { // Tikhonov
                        rows.push(PipelineRow::Param { label: "  Lambda", field: "tikhonov_lambda", help: "L2 regularization weight" });
                    }
                    6 => { // NLTV
                        rows.push(PipelineRow::Param { label: "  Lambda", field: "nltv_lambda", help: "Regularization parameter" });
                        rows.push(PipelineRow::Param { label: "  Mu", field: "nltv_mu", help: "Penalty parameter" });
                        rows.push(PipelineRow::Param { label: "  Tolerance", field: "nltv_tol", help: "Convergence tolerance" });
                        rows.push(PipelineRow::Param { label: "  Max Iter", field: "nltv_max_iter", help: "Maximum ADMM iterations" });
                        rows.push(PipelineRow::Param { label: "  Newton Iter", field: "nltv_newton_iter", help: "Newton iterations for weight update" });
                    }
                    7 => { // MEDI (QSMART already removes background, so SMV mode is omitted)
                        rows.push(PipelineRow::Param { label: "  Lambda", field: "medi_lambda", help: "Regularization weight" });
                        rows.push(PipelineRow::Param { label: "  Percentage", field: "medi_percentage", help: "Fraction of voxels considered edges (0.0-1.0)" });
                        rows.push(PipelineRow::Param { label: "  Max Iter", field: "medi_max_iter", help: "Maximum outer iterations" });
                        rows.push(PipelineRow::Param { label: "  CG Max Iter", field: "medi_cg_max_iter", help: "Maximum conjugate gradient iterations" });
                        rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "medi_cg_tol", help: "CG convergence tolerance" });
                        rows.push(PipelineRow::Param { label: "  Tolerance", field: "medi_tol", help: "Outer convergence tolerance" });
                    }
                    _ => {}
                }
                // SDF background-removal parameters
                rows.push(PipelineRow::Param { label: "  SDF Sigma1 (s1)", field: "qsmart_sdf_sigma1_stage1", help: "Stage 1 SDF kernel sigma 1 (voxels)" });
                rows.push(PipelineRow::Param { label: "  SDF Sigma2 (s1)", field: "qsmart_sdf_sigma2_stage1", help: "Stage 1 SDF kernel sigma 2 (voxels)" });
                rows.push(PipelineRow::Param { label: "  SDF Sigma1 (s2)", field: "qsmart_sdf_sigma1_stage2", help: "Stage 2 SDF kernel sigma 1 (voxels)" });
                rows.push(PipelineRow::Param { label: "  SDF Sigma2 (s2)", field: "qsmart_sdf_sigma2_stage2", help: "Stage 2 SDF kernel sigma 2 (voxels)" });
                rows.push(PipelineRow::Param { label: "  SDF Radius", field: "qsmart_sdf_spatial_radius", help: "SDF spatial filtering radius (voxels)" });
                rows.push(PipelineRow::Param { label: "  SDF Lower Lim", field: "qsmart_sdf_lower_lim", help: "SDF proximity lower limit" });
                rows.push(PipelineRow::Param { label: "  SDF Curv Const", field: "qsmart_sdf_curv_constant", help: "SDF curvature constant" });
                // Vasculature detection (Frangi) — radii in mm, auto-scaled to voxels
                rows.push(PipelineRow::Param { label: "  Vasc Radius (mm)", field: "qsmart_vasc_sphere_radius", help: "Bottom-hat sphere radius for vasculature detection (mm)" });
                rows.push(PipelineRow::Param { label: "  Frangi Min (mm)", field: "qsmart_frangi_scale_min", help: "Frangi minimum vessel radius (mm)" });
                rows.push(PipelineRow::Param { label: "  Frangi Max (mm)", field: "qsmart_frangi_scale_max", help: "Frangi maximum vessel radius (mm)" });
                rows.push(PipelineRow::Param { label: "  Frangi Step (mm)", field: "qsmart_frangi_scale_ratio", help: "Frangi scale step (mm)" });
                rows.push(PipelineRow::Param { label: "  Frangi C", field: "qsmart_frangi_c", help: "Frangi C noise threshold" });
            }
            11 => { // NDI
                rows.push(PipelineRow::Param { label: "  Tau", field: "ndi_tau", help: "Gradient-descent step size" });
                rows.push(PipelineRow::Param { label: "  Alpha", field: "ndi_alpha", help: "L2 regularization weight" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "ndi_max_iter", help: "Number of iterations" });
                rows.push(PipelineRow::Param { label: "  Phase Scale", field: "ndi_phase_scale", help: "ppm -> working-scale multiplier" });
            }
            12 | 13 => { // FANSI nlTV (12) / nlTGV (13) — shared params
                rows.push(PipelineRow::Param { label: "  Alpha1", field: "fansi_alpha1", help: "First-order (TV/TGV) L1 penalty weight" });
                rows.push(PipelineRow::Param { label: "  Mu1", field: "fansi_mu1", help: "Gradient-consistency ADMM weight" });
                rows.push(PipelineRow::Param { label: "  Mu2", field: "fansi_mu2", help: "Fidelity-consistency ADMM weight" });
                rows.push(PipelineRow::Param { label: "  Alpha0", field: "fansi_alpha0", help: "Second-order L1 penalty weight (nlTGV only)" });
                rows.push(PipelineRow::Param { label: "  Mu0", field: "fansi_mu0", help: "Second-order consistency ADMM weight (nlTGV only)" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "fansi_max_iter", help: "Number of outer ADMM iterations" });
                rows.push(PipelineRow::Param { label: "  Tol Update", field: "fansi_tol_update", help: "Percent-update stopping tolerance" });
                rows.push(PipelineRow::Param { label: "  Tol Delta", field: "fansi_tol_delta", help: "Inner Newton convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Phase Scale", field: "fansi_phase_scale", help: "ppm -> working-scale multiplier" });
            }
            14 => { // L1-QSM
                rows.push(PipelineRow::Param { label: "  Alpha1", field: "l1qsm_alpha1", help: "Gradient (TV) L1 penalty weight" });
                rows.push(PipelineRow::Param { label: "  Mu1", field: "l1qsm_mu1", help: "Gradient-consistency ADMM weight" });
                rows.push(PipelineRow::Param { label: "  Mu2", field: "l1qsm_mu2", help: "Fidelity-consistency ADMM weight" });
                rows.push(PipelineRow::Param { label: "  Mu3", field: "l1qsm_mu3", help: "L1 proximal ADMM weight" });
                rows.push(PipelineRow::Param { label: "  Lambda", field: "l1qsm_lambda", help: "L1 fidelity strength" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "l1qsm_max_iter", help: "Number of outer ADMM iterations" });
                rows.push(PipelineRow::Param { label: "  Tol Update", field: "l1qsm_tol_update", help: "Percent-update stopping tolerance" });
                rows.push(PipelineRow::Param { label: "  Tol Delta", field: "l1qsm_tol_delta", help: "Inner Newton convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Phase Scale", field: "l1qsm_phase_scale", help: "ppm -> working-scale multiplier" });
            }
            15 => { // WH-QSM
                rows.push(PipelineRow::Param { label: "  Alpha1", field: "whqsm_alpha1", help: "TV regularization weight" });
                rows.push(PipelineRow::Param { label: "  Mu1", field: "whqsm_mu1", help: "ADMM penalty for TV splitting" });
                rows.push(PipelineRow::Param { label: "  Mu2", field: "whqsm_mu2", help: "ADMM penalty for data-fidelity splitting" });
                rows.push(PipelineRow::Param { label: "  Beta", field: "whqsm_beta", help: "Weak-harmonic ROI penalty" });
                rows.push(PipelineRow::Param { label: "  Muh", field: "whqsm_muh", help: "ADMM penalty for harmonic-field splitting" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "whqsm_max_iter", help: "Maximum outer iterations" });
                rows.push(PipelineRow::Param { label: "  Tol Update", field: "whqsm_tol_update", help: "Percent-update stopping tolerance" });
                rows.push(PipelineRow::Param { label: "  Tol Delta", field: "whqsm_tol_delta", help: "Inner Newton stopping tolerance" });
                rows.push(PipelineRow::Param { label: "  Phase Scale", field: "whqsm_phase_scale", help: "ppm -> working-scale multiplier" });
            }
            16 => { // HD-QSM
                rows.push(PipelineRow::Param { label: "  Alpha L2", field: "hdqsm_alpha_l2", help: "L2-stage TV weight" });
                rows.push(PipelineRow::Param { label: "  Mu1 L2", field: "hdqsm_mu1_l2", help: "L2-stage gradient-consistency ADMM weight" });
                rows.push(PipelineRow::Param { label: "  Mu2", field: "hdqsm_mu2", help: "Fidelity consistency weight" });
                rows.push(PipelineRow::Param { label: "  Max Iter L1", field: "hdqsm_max_iter_l1", help: "Stage-1 (L1) iterations" });
                rows.push(PipelineRow::Param { label: "  Max Iter L2", field: "hdqsm_max_iter_l2", help: "Stage-2 (L2) iterations" });
                rows.push(PipelineRow::Param { label: "  Tol Update", field: "hdqsm_tol_update", help: "Stage-2 percent-update stopping tolerance" });
            }
            17 => { // AMP-PE
                rows.push(PipelineRow::Param { label: "  Wave Order", field: "amp_pe_wave_order", help: "Daubechies wavelet order (1=db1, 2=db2)" });
                rows.push(PipelineRow::Param { label: "  N Level", field: "amp_pe_nlevel", help: "Wavelet decomposition levels" });
                rows.push(PipelineRow::Param { label: "  Wave Pec", field: "amp_pe_wave_pec", help: "Morphology-mask energy retention fraction (0.0-1.0)" });
                rows.push(PipelineRow::Param { label: "  Simulated TE", field: "amp_pe_simulated_te", help: "Simulated echo time (s) used to turn the field into phase" });
                rows.push(PipelineRow::Param { label: "  Max Linearization Ite", field: "amp_pe_max_linearization_ite", help: "Linearization iterations per stage" });
                rows.push(PipelineRow::Param { label: "  Tikhonov Beta", field: "amp_pe_tikhonov_beta", help: "L2-seed Tikhonov weight" });
            }
            _ => {}
        }

        // Deep-learning overlap-tiling (optional). Shown for the tileable DL inversions; leave the
        // fields blank to run whole-volume. Matched by name so it survives option reordering.
        if matches!(
            QSM_ALGO_OPTIONS.get(self.qsm_algorithm).copied().unwrap_or(""),
            "xqsm" | "qsmnet" | "qsmnet-plus" | "ir2qsm" | "lpcnn" | "modl-qsm" | "nextqsm"
        ) {
            rows.push(PipelineRow::Param { label: "  Tile size", field: "dl_tile_size", help: "Overlap-tiling core size in voxels; set to run tiled (bounded memory, approximate). Blank = whole-volume." });
            rows.push(PipelineRow::Param { label: "  Tile halo", field: "dl_tile_halo", help: "Overlap-tiling context margin in voxels (default 8 when tile size is set)." });
        }

        rows.push(PipelineRow::Separator);

        // QSM Reference
        rows.push(self.section_header("reference", "Referencing",
            QSM_REF_OPTIONS.get(self.get_select("qsm_reference")).copied().unwrap_or("").to_string()));
        rows.push(PipelineRow::AlgoSelect {
            label: "QSM Reference", field: "qsm_reference",
            options: QSM_REF_OPTIONS, help: QSM_REF_HELP,
        });
        } // end if do_qsm (unwrapping/inversion/reference)

        Self::apply_section_collapse(rows)
    }

    /// All valid parameter field names, for testing that string dispatch is complete.
    #[cfg(test)]
    pub const ALL_PARAM_FIELDS: &[&str] = &[
        "obliquity_threshold",
        "dl_tile_size", "dl_tile_halo",
        "rts_delta", "rts_mu", "rts_tol", "rts_rho", "rts_max_iter", "rts_lsmr_iter",
        "tv_lambda", "tv_rho", "tv_tol", "tv_max_iter",
        "tkd_threshold", "tsvd_threshold",
        "ilsqr_tol", "ilsqr_max_iter",
        "tgv_iterations", "tgv_erosions", "tgv_alpha1", "tgv_alpha0",
        "tikhonov_lambda",
        "nltv_lambda", "nltv_mu", "nltv_tol", "nltv_max_iter", "nltv_newton_iter",
        "medi_lambda", "medi_max_iter", "medi_cg_max_iter", "medi_cg_tol", "medi_tol", "medi_percentage", "medi_smv_radius",
        "tfi_lambda", "tfi_precond", "tfi_max_iter", "tfi_cg_max_iter", "tfi_cg_tol", "tfi_tol", "tfi_percentage",
        "amp_pe_wave_order", "amp_pe_nlevel", "amp_pe_wave_pec", "amp_pe_simulated_te", "amp_pe_max_linearization_ite", "amp_pe_tikhonov_beta",
        "phase_offset_sigma", "romeo_template",
        "vsharp_threshold", "vsharp_max_radius", "vsharp_min_radius", "pdf_tol", "lbv_tol",
        "ismv_tol", "ismv_max_iter", "ismv_radius", "sharp_threshold", "sharp_radius",
        "resharp_radius", "resharp_tik_reg", "resharp_tol", "resharp_max_iter",
        "harperella_radius", "harperella_max_iter", "harperella_tol",
        "iharperella_radius", "iharperella_max_iter", "iharperella_tol",
        "msmv_radius", "msmv_maxk",
        "qsmart_ilsqr_tol", "qsmart_ilsqr_max_iter", "qsmart_vasc_sphere_radius", "qsmart_sdf_spatial_radius",
        "qsmart_sdf_sigma1_stage1", "qsmart_sdf_sigma2_stage1", "qsmart_sdf_sigma1_stage2", "qsmart_sdf_sigma2_stage2",
        "qsmart_sdf_lower_lim", "qsmart_sdf_curv_constant",
        "qsmart_frangi_scale_min", "qsmart_frangi_scale_max", "qsmart_frangi_scale_ratio", "qsmart_frangi_c",
        "ndi_tau", "ndi_alpha", "ndi_max_iter", "ndi_phase_scale",
        "fansi_alpha1", "fansi_mu1", "fansi_mu2", "fansi_alpha0", "fansi_mu0", "fansi_max_iter", "fansi_tol_update", "fansi_tol_delta", "fansi_phase_scale",
        "l1qsm_alpha1", "l1qsm_mu1", "l1qsm_mu2", "l1qsm_mu3", "l1qsm_lambda", "l1qsm_max_iter", "l1qsm_tol_update", "l1qsm_tol_delta", "l1qsm_phase_scale",
        "whqsm_alpha1", "whqsm_mu1", "whqsm_mu2", "whqsm_beta", "whqsm_muh", "whqsm_max_iter", "whqsm_tol_update", "whqsm_tol_delta", "whqsm_phase_scale",
        "hdqsm_alpha_l2", "hdqsm_mu1_l2", "hdqsm_mu2", "hdqsm_max_iter_l1", "hdqsm_max_iter_l2", "hdqsm_tol_update",
        "bet_fractional_intensity", "bet_smoothness", "bet_gradient_threshold", "bet_iterations", "bet_subdivisions",
    ];

    /// Get a string parameter value by field name.
    pub fn get_param(&self, field: &str) -> &str {
        match field {
            "obliquity_threshold" => &self.obliquity_threshold,
            "dl_tile_size" => &self.dl_tile_size,
            "dl_tile_halo" => &self.dl_tile_halo,
            "rts_delta" => &self.rts_delta,
            "rts_mu" => &self.rts_mu,
            "rts_tol" => &self.rts_tol,
            "rts_rho" => &self.rts_rho,
            "rts_max_iter" => &self.rts_max_iter,
            "rts_lsmr_iter" => &self.rts_lsmr_iter,
            "tv_lambda" => &self.tv_lambda,
            "tv_rho" => &self.tv_rho,
            "tv_tol" => &self.tv_tol,
            "tv_max_iter" => &self.tv_max_iter,
            "tkd_threshold" => &self.tkd_threshold,
            "tsvd_threshold" => &self.tsvd_threshold,
            "ilsqr_tol" => &self.ilsqr_tol,
            "ilsqr_max_iter" => &self.ilsqr_max_iter,
            "tgv_iterations" => &self.tgv_iterations,
            "tgv_erosions" => &self.tgv_erosions,
            "tgv_alpha1" => &self.tgv_alpha1,
            "tgv_alpha0" => &self.tgv_alpha0,
            "tikhonov_lambda" => &self.tikhonov_lambda,
            "nltv_lambda" => &self.nltv_lambda,
            "nltv_mu" => &self.nltv_mu,
            "nltv_tol" => &self.nltv_tol,
            "nltv_max_iter" => &self.nltv_max_iter,
            "nltv_newton_iter" => &self.nltv_newton_iter,
            "medi_lambda" => &self.medi_lambda,
            "medi_max_iter" => &self.medi_max_iter,
            "medi_cg_max_iter" => &self.medi_cg_max_iter,
            "medi_cg_tol" => &self.medi_cg_tol,
            "medi_tol" => &self.medi_tol,
            "medi_percentage" => &self.medi_percentage,
            "medi_smv_radius" => &self.medi_smv_radius,
            "tfi_lambda" => &self.tfi_lambda,
            "tfi_precond" => &self.tfi_precond,
            "tfi_max_iter" => &self.tfi_max_iter,
            "tfi_cg_max_iter" => &self.tfi_cg_max_iter,
            "tfi_cg_tol" => &self.tfi_cg_tol,
            "tfi_tol" => &self.tfi_tol,
            "tfi_percentage" => &self.tfi_percentage,
            "amp_pe_wave_order" => &self.amp_pe_wave_order,
            "amp_pe_nlevel" => &self.amp_pe_nlevel,
            "amp_pe_wave_pec" => &self.amp_pe_wave_pec,
            "amp_pe_simulated_te" => &self.amp_pe_simulated_te,
            "amp_pe_max_linearization_ite" => &self.amp_pe_max_linearization_ite,
            "amp_pe_tikhonov_beta" => &self.amp_pe_tikhonov_beta,
            "phase_offset_sigma" => &self.phase_offset_sigma,
            "romeo_template" => &self.romeo_template,
            "custom_mask_tool" => &self.custom_mask_tool,
            "vsharp_threshold" => &self.vsharp_threshold,
            "vsharp_max_radius" => &self.vsharp_max_radius,
            "vsharp_min_radius" => &self.vsharp_min_radius,
            "pdf_tol" => &self.pdf_tol,
            "lbv_tol" => &self.lbv_tol,
            "ismv_tol" => &self.ismv_tol,
            "ismv_max_iter" => &self.ismv_max_iter,
            "ismv_radius" => &self.ismv_radius,
            "sharp_threshold" => &self.sharp_threshold,
            "sharp_radius" => &self.sharp_radius,
            "resharp_radius" => &self.resharp_radius,
            "resharp_tik_reg" => &self.resharp_tik_reg,
            "resharp_tol" => &self.resharp_tol,
            "resharp_max_iter" => &self.resharp_max_iter,
            "harperella_radius" => &self.harperella_radius,
            "harperella_max_iter" => &self.harperella_max_iter,
            "harperella_tol" => &self.harperella_tol,
            "iharperella_radius" => &self.iharperella_radius,
            "iharperella_max_iter" => &self.iharperella_max_iter,
            "iharperella_tol" => &self.iharperella_tol,
            "msmv_radius" => &self.msmv_radius,
            "msmv_maxk" => &self.msmv_maxk,
            "qsmart_ilsqr_tol" => &self.qsmart_ilsqr_tol,
            "qsmart_ilsqr_max_iter" => &self.qsmart_ilsqr_max_iter,
            "qsmart_vasc_sphere_radius" => &self.qsmart_vasc_sphere_radius,
            "qsmart_sdf_spatial_radius" => &self.qsmart_sdf_spatial_radius,
            "qsmart_sdf_sigma1_stage1" => &self.qsmart_sdf_sigma1_stage1,
            "qsmart_sdf_sigma2_stage1" => &self.qsmart_sdf_sigma2_stage1,
            "qsmart_sdf_sigma1_stage2" => &self.qsmart_sdf_sigma1_stage2,
            "qsmart_sdf_sigma2_stage2" => &self.qsmart_sdf_sigma2_stage2,
            "qsmart_sdf_lower_lim" => &self.qsmart_sdf_lower_lim,
            "qsmart_sdf_curv_constant" => &self.qsmart_sdf_curv_constant,
            "qsmart_frangi_scale_min" => &self.qsmart_frangi_scale_min,
            "qsmart_frangi_scale_max" => &self.qsmart_frangi_scale_max,
            "qsmart_frangi_scale_ratio" => &self.qsmart_frangi_scale_ratio,
            "qsmart_frangi_c" => &self.qsmart_frangi_c,
            "ndi_tau" => &self.ndi_tau,
            "ndi_alpha" => &self.ndi_alpha,
            "ndi_max_iter" => &self.ndi_max_iter,
            "ndi_phase_scale" => &self.ndi_phase_scale,
            "fansi_alpha1" => &self.fansi_alpha1,
            "fansi_mu1" => &self.fansi_mu1,
            "fansi_mu2" => &self.fansi_mu2,
            "fansi_alpha0" => &self.fansi_alpha0,
            "fansi_mu0" => &self.fansi_mu0,
            "fansi_max_iter" => &self.fansi_max_iter,
            "fansi_tol_update" => &self.fansi_tol_update,
            "fansi_tol_delta" => &self.fansi_tol_delta,
            "fansi_phase_scale" => &self.fansi_phase_scale,
            "l1qsm_alpha1" => &self.l1qsm_alpha1,
            "l1qsm_mu1" => &self.l1qsm_mu1,
            "l1qsm_mu2" => &self.l1qsm_mu2,
            "l1qsm_mu3" => &self.l1qsm_mu3,
            "l1qsm_lambda" => &self.l1qsm_lambda,
            "l1qsm_max_iter" => &self.l1qsm_max_iter,
            "l1qsm_tol_update" => &self.l1qsm_tol_update,
            "l1qsm_tol_delta" => &self.l1qsm_tol_delta,
            "l1qsm_phase_scale" => &self.l1qsm_phase_scale,
            "whqsm_alpha1" => &self.whqsm_alpha1,
            "whqsm_mu1" => &self.whqsm_mu1,
            "whqsm_mu2" => &self.whqsm_mu2,
            "whqsm_beta" => &self.whqsm_beta,
            "whqsm_muh" => &self.whqsm_muh,
            "whqsm_max_iter" => &self.whqsm_max_iter,
            "whqsm_tol_update" => &self.whqsm_tol_update,
            "whqsm_tol_delta" => &self.whqsm_tol_delta,
            "whqsm_phase_scale" => &self.whqsm_phase_scale,
            "hdqsm_alpha_l2" => &self.hdqsm_alpha_l2,
            "hdqsm_mu1_l2" => &self.hdqsm_mu1_l2,
            "hdqsm_mu2" => &self.hdqsm_mu2,
            "hdqsm_max_iter_l1" => &self.hdqsm_max_iter_l1,
            "hdqsm_max_iter_l2" => &self.hdqsm_max_iter_l2,
            "hdqsm_tol_update" => &self.hdqsm_tol_update,
            "bet_fractional_intensity" => &self.bet_fractional_intensity,
            "bet_smoothness" => &self.bet_smoothness,
            "bet_gradient_threshold" => &self.bet_gradient_threshold,
            "bet_iterations" => &self.bet_iterations,
            "bet_subdivisions" => &self.bet_subdivisions,
            "custom_qsm_tool" => &self.custom_qsm_tool,
            "custom_r2_tool" => &self.custom_r2_tool,
            "custom_r2prime_tool" => &self.custom_r2prime_tool,
            "sep_r2starqsm_r_const_3t" => &self.sep_r2starqsm_r_const_3t,
            "sep_decompose_n_inner" => &self.sep_decompose_n_inner,
            "sep_decompose_chi_bound" => &self.sep_decompose_chi_bound,
            "sep_decompose_max_lm_iter" => &self.sep_decompose_max_lm_iter,
            "sep_ilsqr_dr_pos" => &self.sep_ilsqr_dr_pos,
            "sep_ilsqr_dr_neg" => &self.sep_ilsqr_dr_neg,
            "sep_ilsqr_lambda1" => &self.sep_ilsqr_lambda1,
            "sep_ilsqr_percentage" => &self.sep_ilsqr_percentage,
            "sep_ilsqr_r2p_min" => &self.sep_ilsqr_r2p_min,
            "sep_ilsqr_r2p_max" => &self.sep_ilsqr_r2p_max,
            "sep_ilsqr_max_iter" => &self.sep_ilsqr_max_iter,
            "sep_ilsqr_tol" => &self.sep_ilsqr_tol,
            "sep_ilsqr_cg_max_iter" => &self.sep_ilsqr_cg_max_iter,
            "sep_ilsqr_cg_tol" => &self.sep_ilsqr_cg_tol,
            "sep_medi_lambda_para" => &self.sep_medi_lambda_para,
            "sep_medi_lambda_dia" => &self.sep_medi_lambda_dia,
            "sep_medi_lambda_cpl" => &self.sep_medi_lambda_cpl,
            "sep_medi_dr_pos" => &self.sep_medi_dr_pos,
            "sep_medi_dr_neg" => &self.sep_medi_dr_neg,
            "sep_medi_percentage" => &self.sep_medi_percentage,
            "sep_medi_cg_tol" => &self.sep_medi_cg_tol,
            "sep_medi_cg_max_iter" => &self.sep_medi_cg_max_iter,
            "sep_medi_max_iter" => &self.sep_medi_max_iter,
            "sep_medi_tol" => &self.sep_medi_tol,
            "sep_wavesep_dr_pos" => &self.sep_wavesep_dr_pos,
            "sep_wavesep_dr_neg" => &self.sep_wavesep_dr_neg,
            "sep_wavesep_alpha" => &self.sep_wavesep_alpha,
            "sep_wavesep_lambda" => &self.sep_wavesep_lambda,
            "sep_wavesep_wavelet_order" => &self.sep_wavesep_wavelet_order,
            "sep_wavesep_max_iter" => &self.sep_wavesep_max_iter,
            "sep_wavesep_tol" => &self.sep_wavesep_tol,
            "sep_hcchisep_dr_pos_3t" => &self.sep_hcchisep_dr_pos_3t,
            "sep_hcchisep_bin_hz" => &self.sep_hcchisep_bin_hz,
            _ => "",
        }
    }

    /// Get a mutable reference to a string parameter.
    pub fn get_param_mut(&mut self, field: &str) -> Option<&mut String> {
        match field {
            "obliquity_threshold" => Some(&mut self.obliquity_threshold),
            "dl_tile_size" => Some(&mut self.dl_tile_size),
            "dl_tile_halo" => Some(&mut self.dl_tile_halo),
            "rts_delta" => Some(&mut self.rts_delta),
            "rts_mu" => Some(&mut self.rts_mu),
            "rts_tol" => Some(&mut self.rts_tol),
            "rts_rho" => Some(&mut self.rts_rho),
            "rts_max_iter" => Some(&mut self.rts_max_iter),
            "rts_lsmr_iter" => Some(&mut self.rts_lsmr_iter),
            "tv_lambda" => Some(&mut self.tv_lambda),
            "tv_rho" => Some(&mut self.tv_rho),
            "tv_tol" => Some(&mut self.tv_tol),
            "tv_max_iter" => Some(&mut self.tv_max_iter),
            "tkd_threshold" => Some(&mut self.tkd_threshold),
            "tsvd_threshold" => Some(&mut self.tsvd_threshold),
            "ilsqr_tol" => Some(&mut self.ilsqr_tol),
            "ilsqr_max_iter" => Some(&mut self.ilsqr_max_iter),
            "tgv_iterations" => Some(&mut self.tgv_iterations),
            "tgv_erosions" => Some(&mut self.tgv_erosions),
            "tgv_alpha1" => Some(&mut self.tgv_alpha1),
            "tgv_alpha0" => Some(&mut self.tgv_alpha0),
            "tikhonov_lambda" => Some(&mut self.tikhonov_lambda),
            "nltv_lambda" => Some(&mut self.nltv_lambda),
            "nltv_mu" => Some(&mut self.nltv_mu),
            "nltv_tol" => Some(&mut self.nltv_tol),
            "nltv_max_iter" => Some(&mut self.nltv_max_iter),
            "nltv_newton_iter" => Some(&mut self.nltv_newton_iter),
            "medi_lambda" => Some(&mut self.medi_lambda),
            "medi_max_iter" => Some(&mut self.medi_max_iter),
            "medi_cg_max_iter" => Some(&mut self.medi_cg_max_iter),
            "medi_cg_tol" => Some(&mut self.medi_cg_tol),
            "medi_tol" => Some(&mut self.medi_tol),
            "medi_percentage" => Some(&mut self.medi_percentage),
            "medi_smv_radius" => Some(&mut self.medi_smv_radius),
            "tfi_lambda" => Some(&mut self.tfi_lambda),
            "tfi_precond" => Some(&mut self.tfi_precond),
            "tfi_max_iter" => Some(&mut self.tfi_max_iter),
            "tfi_cg_max_iter" => Some(&mut self.tfi_cg_max_iter),
            "tfi_cg_tol" => Some(&mut self.tfi_cg_tol),
            "tfi_tol" => Some(&mut self.tfi_tol),
            "tfi_percentage" => Some(&mut self.tfi_percentage),
            "amp_pe_wave_order" => Some(&mut self.amp_pe_wave_order),
            "amp_pe_nlevel" => Some(&mut self.amp_pe_nlevel),
            "amp_pe_wave_pec" => Some(&mut self.amp_pe_wave_pec),
            "amp_pe_simulated_te" => Some(&mut self.amp_pe_simulated_te),
            "amp_pe_max_linearization_ite" => Some(&mut self.amp_pe_max_linearization_ite),
            "amp_pe_tikhonov_beta" => Some(&mut self.amp_pe_tikhonov_beta),
            "phase_offset_sigma" => Some(&mut self.phase_offset_sigma),
            "romeo_template" => Some(&mut self.romeo_template),
            "custom_mask_tool" => Some(&mut self.custom_mask_tool),
            "vsharp_threshold" => Some(&mut self.vsharp_threshold),
            "vsharp_max_radius" => Some(&mut self.vsharp_max_radius),
            "vsharp_min_radius" => Some(&mut self.vsharp_min_radius),
            "pdf_tol" => Some(&mut self.pdf_tol),
            "lbv_tol" => Some(&mut self.lbv_tol),
            "ismv_tol" => Some(&mut self.ismv_tol),
            "ismv_max_iter" => Some(&mut self.ismv_max_iter),
            "ismv_radius" => Some(&mut self.ismv_radius),
            "sharp_threshold" => Some(&mut self.sharp_threshold),
            "sharp_radius" => Some(&mut self.sharp_radius),
            "resharp_radius" => Some(&mut self.resharp_radius),
            "resharp_tik_reg" => Some(&mut self.resharp_tik_reg),
            "resharp_tol" => Some(&mut self.resharp_tol),
            "resharp_max_iter" => Some(&mut self.resharp_max_iter),
            "harperella_radius" => Some(&mut self.harperella_radius),
            "harperella_max_iter" => Some(&mut self.harperella_max_iter),
            "harperella_tol" => Some(&mut self.harperella_tol),
            "iharperella_radius" => Some(&mut self.iharperella_radius),
            "iharperella_max_iter" => Some(&mut self.iharperella_max_iter),
            "iharperella_tol" => Some(&mut self.iharperella_tol),
            "msmv_radius" => Some(&mut self.msmv_radius),
            "msmv_maxk" => Some(&mut self.msmv_maxk),
            "qsmart_ilsqr_tol" => Some(&mut self.qsmart_ilsqr_tol),
            "qsmart_ilsqr_max_iter" => Some(&mut self.qsmart_ilsqr_max_iter),
            "qsmart_vasc_sphere_radius" => Some(&mut self.qsmart_vasc_sphere_radius),
            "qsmart_sdf_spatial_radius" => Some(&mut self.qsmart_sdf_spatial_radius),
            "qsmart_sdf_sigma1_stage1" => Some(&mut self.qsmart_sdf_sigma1_stage1),
            "qsmart_sdf_sigma2_stage1" => Some(&mut self.qsmart_sdf_sigma2_stage1),
            "qsmart_sdf_sigma1_stage2" => Some(&mut self.qsmart_sdf_sigma1_stage2),
            "qsmart_sdf_sigma2_stage2" => Some(&mut self.qsmart_sdf_sigma2_stage2),
            "qsmart_sdf_lower_lim" => Some(&mut self.qsmart_sdf_lower_lim),
            "qsmart_sdf_curv_constant" => Some(&mut self.qsmart_sdf_curv_constant),
            "qsmart_frangi_scale_min" => Some(&mut self.qsmart_frangi_scale_min),
            "qsmart_frangi_scale_max" => Some(&mut self.qsmart_frangi_scale_max),
            "qsmart_frangi_scale_ratio" => Some(&mut self.qsmart_frangi_scale_ratio),
            "qsmart_frangi_c" => Some(&mut self.qsmart_frangi_c),
            "ndi_tau" => Some(&mut self.ndi_tau),
            "ndi_alpha" => Some(&mut self.ndi_alpha),
            "ndi_max_iter" => Some(&mut self.ndi_max_iter),
            "ndi_phase_scale" => Some(&mut self.ndi_phase_scale),
            "fansi_alpha1" => Some(&mut self.fansi_alpha1),
            "fansi_mu1" => Some(&mut self.fansi_mu1),
            "fansi_mu2" => Some(&mut self.fansi_mu2),
            "fansi_alpha0" => Some(&mut self.fansi_alpha0),
            "fansi_mu0" => Some(&mut self.fansi_mu0),
            "fansi_max_iter" => Some(&mut self.fansi_max_iter),
            "fansi_tol_update" => Some(&mut self.fansi_tol_update),
            "fansi_tol_delta" => Some(&mut self.fansi_tol_delta),
            "fansi_phase_scale" => Some(&mut self.fansi_phase_scale),
            "l1qsm_alpha1" => Some(&mut self.l1qsm_alpha1),
            "l1qsm_mu1" => Some(&mut self.l1qsm_mu1),
            "l1qsm_mu2" => Some(&mut self.l1qsm_mu2),
            "l1qsm_mu3" => Some(&mut self.l1qsm_mu3),
            "l1qsm_lambda" => Some(&mut self.l1qsm_lambda),
            "l1qsm_max_iter" => Some(&mut self.l1qsm_max_iter),
            "l1qsm_tol_update" => Some(&mut self.l1qsm_tol_update),
            "l1qsm_tol_delta" => Some(&mut self.l1qsm_tol_delta),
            "l1qsm_phase_scale" => Some(&mut self.l1qsm_phase_scale),
            "whqsm_alpha1" => Some(&mut self.whqsm_alpha1),
            "whqsm_mu1" => Some(&mut self.whqsm_mu1),
            "whqsm_mu2" => Some(&mut self.whqsm_mu2),
            "whqsm_beta" => Some(&mut self.whqsm_beta),
            "whqsm_muh" => Some(&mut self.whqsm_muh),
            "whqsm_max_iter" => Some(&mut self.whqsm_max_iter),
            "whqsm_tol_update" => Some(&mut self.whqsm_tol_update),
            "whqsm_tol_delta" => Some(&mut self.whqsm_tol_delta),
            "whqsm_phase_scale" => Some(&mut self.whqsm_phase_scale),
            "hdqsm_alpha_l2" => Some(&mut self.hdqsm_alpha_l2),
            "hdqsm_mu1_l2" => Some(&mut self.hdqsm_mu1_l2),
            "hdqsm_mu2" => Some(&mut self.hdqsm_mu2),
            "hdqsm_max_iter_l1" => Some(&mut self.hdqsm_max_iter_l1),
            "hdqsm_max_iter_l2" => Some(&mut self.hdqsm_max_iter_l2),
            "hdqsm_tol_update" => Some(&mut self.hdqsm_tol_update),
            "bet_fractional_intensity" => Some(&mut self.bet_fractional_intensity),
            "bet_smoothness" => Some(&mut self.bet_smoothness),
            "bet_gradient_threshold" => Some(&mut self.bet_gradient_threshold),
            "bet_iterations" => Some(&mut self.bet_iterations),
            "bet_subdivisions" => Some(&mut self.bet_subdivisions),
            "custom_qsm_tool" => Some(&mut self.custom_qsm_tool),
            "custom_r2_tool" => Some(&mut self.custom_r2_tool),
            "custom_r2prime_tool" => Some(&mut self.custom_r2prime_tool),
            "sep_r2starqsm_r_const_3t" => Some(&mut self.sep_r2starqsm_r_const_3t),
            "sep_decompose_n_inner" => Some(&mut self.sep_decompose_n_inner),
            "sep_decompose_chi_bound" => Some(&mut self.sep_decompose_chi_bound),
            "sep_decompose_max_lm_iter" => Some(&mut self.sep_decompose_max_lm_iter),
            "sep_ilsqr_dr_pos" => Some(&mut self.sep_ilsqr_dr_pos),
            "sep_ilsqr_dr_neg" => Some(&mut self.sep_ilsqr_dr_neg),
            "sep_ilsqr_lambda1" => Some(&mut self.sep_ilsqr_lambda1),
            "sep_ilsqr_percentage" => Some(&mut self.sep_ilsqr_percentage),
            "sep_ilsqr_r2p_min" => Some(&mut self.sep_ilsqr_r2p_min),
            "sep_ilsqr_r2p_max" => Some(&mut self.sep_ilsqr_r2p_max),
            "sep_ilsqr_max_iter" => Some(&mut self.sep_ilsqr_max_iter),
            "sep_ilsqr_tol" => Some(&mut self.sep_ilsqr_tol),
            "sep_ilsqr_cg_max_iter" => Some(&mut self.sep_ilsqr_cg_max_iter),
            "sep_ilsqr_cg_tol" => Some(&mut self.sep_ilsqr_cg_tol),
            "sep_medi_lambda_para" => Some(&mut self.sep_medi_lambda_para),
            "sep_medi_lambda_dia" => Some(&mut self.sep_medi_lambda_dia),
            "sep_medi_lambda_cpl" => Some(&mut self.sep_medi_lambda_cpl),
            "sep_medi_dr_pos" => Some(&mut self.sep_medi_dr_pos),
            "sep_medi_dr_neg" => Some(&mut self.sep_medi_dr_neg),
            "sep_medi_percentage" => Some(&mut self.sep_medi_percentage),
            "sep_medi_cg_tol" => Some(&mut self.sep_medi_cg_tol),
            "sep_medi_cg_max_iter" => Some(&mut self.sep_medi_cg_max_iter),
            "sep_medi_max_iter" => Some(&mut self.sep_medi_max_iter),
            "sep_medi_tol" => Some(&mut self.sep_medi_tol),
            "sep_wavesep_dr_pos" => Some(&mut self.sep_wavesep_dr_pos),
            "sep_wavesep_dr_neg" => Some(&mut self.sep_wavesep_dr_neg),
            "sep_wavesep_alpha" => Some(&mut self.sep_wavesep_alpha),
            "sep_wavesep_lambda" => Some(&mut self.sep_wavesep_lambda),
            "sep_wavesep_wavelet_order" => Some(&mut self.sep_wavesep_wavelet_order),
            "sep_wavesep_max_iter" => Some(&mut self.sep_wavesep_max_iter),
            "sep_wavesep_tol" => Some(&mut self.sep_wavesep_tol),
            "sep_hcchisep_dr_pos_3t" => Some(&mut self.sep_hcchisep_dr_pos_3t),
            "sep_hcchisep_bin_hz" => Some(&mut self.sep_hcchisep_bin_hz),
            _ => None,
        }
    }

    /// Get a select value by field name.
    pub fn get_select(&self, field: &str) -> usize {
        match field {
            "qsm_algorithm" => self.qsm_algorithm,
            "qsmart_inversion" => self.qsmart_inversion,
            "unwrapping_algorithm" => self.unwrapping_algorithm,
            "bf_algorithm" => self.bf_algorithm,
            "qsm_reference" => self.qsm_reference,
            "b0_estimation" => self.b0_estimation,
            "b0_weight_type" => self.b0_weight_type,
            "mask_preset" => self.mask_preset,
            "separation_algorithm" => self.separation_algorithm,
            _ => 0,
        }
    }

    /// Set a select value by field name.
    pub fn set_select(&mut self, field: &str, val: usize) {
        match field {
            "qsm_algorithm" => self.qsm_algorithm = val,
            "qsmart_inversion" => self.qsmart_inversion = val,
            "unwrapping_algorithm" => self.unwrapping_algorithm = val,
            "bf_algorithm" => self.bf_algorithm = val,
            "qsm_reference" => self.qsm_reference = val,
            "b0_estimation" => self.b0_estimation = val,
            "b0_weight_type" => self.b0_weight_type = val,
            "mask_preset" => {
                self.mask_preset = val;
                self.apply_mask_preset(val);
            }
            "separation_algorithm" => self.separation_algorithm = val,
            _ => {}
        }
    }

    /// Get a toggle value by field name.
    pub fn get_toggle(&self, field: &str) -> bool {
        match field {
            "do_qsm" => self.do_qsm,
            "inhomogeneity_correction" => self.inhomogeneity_correction,
            "phase_offset_removal" => self.phase_offset_removal,
            "bipolar_correction" => self.bipolar_correction,
            "romeo_individual" => self.romeo_individual,
            "romeo_correct_global" => self.romeo_correct_global,
            "medi_smv" => self.medi_smv,
            "do_chi_separation" => self.do_chi_separation,
            "msmv_refine" => self.msmv_refine,
            _ => false,
        }
    }

    /// Toggle a boolean by field name.
    pub fn toggle(&mut self, field: &str) {
        match field {
            "do_qsm" => self.do_qsm = !self.do_qsm,
            "do_chi_separation" => self.do_chi_separation = !self.do_chi_separation,
            "inhomogeneity_correction" => self.inhomogeneity_correction = !self.inhomogeneity_correction,
            "phase_offset_removal" => self.phase_offset_removal = !self.phase_offset_removal,
            "bipolar_correction" => self.bipolar_correction = !self.bipolar_correction,
            "romeo_individual" => self.romeo_individual = !self.romeo_individual,
            "romeo_correct_global" => self.romeo_correct_global = !self.romeo_correct_global,
            "medi_smv" => self.medi_smv = !self.medi_smv,
            "msmv_refine" => self.msmv_refine = !self.msmv_refine,
            _ => {}
        }
    }

    /// Get the field name of the currently focused row.
    pub fn focused_field_name(&self) -> Option<String> {
        let rows = self.visible_rows();
        let focusable = self.focusable_rows();
        let focus_idx = focusable.get(self.focus).copied()?;
        match rows.get(focus_idx) {
            Some(PipelineRow::AlgoSelect { field, .. }) => Some(field.to_string()),
            Some(PipelineRow::Param { field, .. }) => Some(field.to_string()),
            Some(PipelineRow::Toggle { field, .. }) => Some(field.to_string()),
            _ => None,
        }
    }

    /// After rows change, restore focus to the row with the given field name.
    pub fn restore_focus(&mut self, field_name: &Option<String>) {
        let Some(name) = field_name else { return };
        let rows = self.visible_rows();
        let focusable = self.focusable_rows();
        for (fi, &ri) in focusable.iter().enumerate() {
            let matches = match rows.get(ri) {
                Some(PipelineRow::AlgoSelect { field, .. }) => *field == name.as_str(),
                Some(PipelineRow::Param { field, .. }) => *field == name.as_str(),
                Some(PipelineRow::Toggle { field, .. }) => *field == name.as_str(),
                _ => false,
            };
            if matches {
                self.focus = fi;
                return;
            }
        }
        // Field not found in new layout — clamp focus
        let max = focusable.len().saturating_sub(1);
        if self.focus > max {
            self.focus = max;
        }
    }

    /// Get the display label and value for a mask op.
    pub fn mask_op_label_value(op: &crate::pipeline::config::MaskOp) -> (&'static str, String) {
        use crate::pipeline::config::MaskOp;
        match op {
            MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Otsu, .. } => ("threshold", "otsu".to_string()),
            MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Fixed, value } =>
                ("threshold", format!("fixed:{}", value.unwrap_or(0.5))),
            MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Percentile, value } =>
                ("threshold", format!("percentile:{}", value.unwrap_or(75.0))),
            MaskOp::Bet { fractional_intensity } => ("bet", format!("{}", fractional_intensity)),
            MaskOp::Erode { iterations } => ("erode", format!("{}", iterations)),
            MaskOp::Dilate { iterations } => ("dilate", format!("{}", iterations)),
            MaskOp::Close { radius } => ("close", format!("{}", radius)),
            MaskOp::FillHoles { max_size } => ("fill-holes", if *max_size == 0 { "auto".to_string() } else { format!("{}", max_size) }),
            MaskOp::GaussianSmooth { sigma_mm } => ("gaussian", format!("{}", sigma_mm)),
        }
    }

    /// Get help text for a mask op.
    pub fn mask_op_help(op: &crate::pipeline::config::MaskOp) -> &'static str {
        use crate::pipeline::config::MaskOp;
        match op {
            MaskOp::Threshold { .. } => "Threshold method (←/→ to change, Enter to edit value)",
            MaskOp::Bet { .. } => "BET fractional intensity (Enter to edit)",
            MaskOp::Erode { .. } => "Erosion iterations (←/→ to adjust)",
            MaskOp::Dilate { .. } => "Dilation iterations (←/→ to adjust)",
            MaskOp::Close { .. } => "Morphological close radius (←/→ to adjust)",
            MaskOp::FillHoles { .. } => "Fill holes max size (0=auto, Enter to edit)",
            MaskOp::GaussianSmooth { .. } => "Gaussian sigma in mm (Enter to edit)",
        }
    }

    /// Create a default mask op for the given type name.
    pub fn default_mask_op(type_name: &str) -> Option<crate::pipeline::config::MaskOp> {
        use crate::pipeline::config::*;
        match type_name {
            "threshold" => Some(MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }),
            "bet" => Some(MaskOp::Bet { fractional_intensity: 0.5 }),
            "erode" => Some(MaskOp::Erode { iterations: 1 }),
            "dilate" => Some(MaskOp::Dilate { iterations: 1 }),
            "close" => Some(MaskOp::Close { radius: 1 }),
            "fill-holes" => Some(MaskOp::FillHoles { max_size: 0 }),
            "gaussian" => Some(MaskOp::GaussianSmooth { sigma_mm: 4.0 }),
            _ => None,
        }
    }

    /// Apply a mask preset, overwriting mask_sections.
    pub fn apply_mask_preset(&mut self, preset: usize) {
        use crate::pipeline::config::*;
        match preset {
            0 => { // Robust threshold
                self.mask_sections = default_mask_sections();
            }
            1 => { // BET
                self.mask_sections = vec![MaskSection {
                    input: MaskingInput::Magnitude,
                    generator: MaskOp::Bet { fractional_intensity: 0.5 },
                    refinements: vec![MaskOp::Erode { iterations: 2 }],
                }];
            }
            2 => { /* Custom: don't touch sections */ }
            _ => {}
        }
    }

    /// Mark preset as "Custom" when user manually edits mask sections.
    fn mark_mask_custom(&mut self) {
        if self.mask_preset != 2 {
            self.mask_preset = 2;
        }
    }

    /// Adjust the generator of a mask section (switch between threshold and BET).
    pub fn adjust_mask_generator(&mut self, section: usize, delta: isize) {
        use crate::pipeline::config::*;
        if section >= self.mask_sections.len() { return; }
        let gen = &self.mask_sections[section].generator;
        let new_gen = match gen {
            MaskOp::Threshold { .. } if delta > 0 => MaskOp::Bet { fractional_intensity: 0.5 },
            MaskOp::Bet { .. } if delta < 0 => MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            // Also handle wrapping
            MaskOp::Threshold { .. } => MaskOp::Bet { fractional_intensity: 0.5 },
            MaskOp::Bet { .. } => MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            _ => return,
        };
        self.mask_sections[section].generator = new_gen;
        self.mark_mask_custom();
    }

    /// Adjust the generator's parameter (threshold method or BET fractional intensity).
    pub fn adjust_mask_generator_param(&mut self, section: usize, delta: isize) {
        use crate::pipeline::config::*;
        if section >= self.mask_sections.len() { return; }
        match &mut self.mask_sections[section].generator {
            MaskOp::Threshold { method, .. } => {
                let methods = [MaskThresholdMethod::Otsu, MaskThresholdMethod::Fixed, MaskThresholdMethod::Percentile];
                let cur = methods.iter().position(|m| m == method).unwrap_or(0) as isize;
                let new = (cur + delta).rem_euclid(methods.len() as isize) as usize;
                *method = methods[new];
            }
            MaskOp::Bet { fractional_intensity } => {
                *fractional_intensity = (*fractional_intensity + delta as f64 * 0.05).clamp(0.05, 1.0);
            }
            _ => {}
        }
        self.mark_mask_custom();
    }

    /// Ordered masking input sources, matching the modal/cycle option order.
    pub const MASK_INPUT_SOURCES: [crate::pipeline::config::MaskingInput; 4] = {
        use crate::pipeline::config::MaskingInput::*;
        [MagnitudeFirst, Magnitude, MagnitudeLast, PhaseQuality]
    };

    /// Current masking-input source index for a section (for populating the modal).
    pub fn mask_input_index(&self, section: usize) -> usize {
        self.mask_sections.get(section)
            .and_then(|s| Self::MASK_INPUT_SOURCES.iter().position(|x| *x == s.input))
            .unwrap_or(0)
    }

    /// Set a section's masking input source by index (from the modal).
    pub fn set_mask_input(&mut self, section: usize, idx: usize) {
        if section >= self.mask_sections.len() { return; }
        if let Some(&src) = Self::MASK_INPUT_SOURCES.get(idx) {
            self.mask_sections[section].input = src;
            self.mark_mask_custom();
        }
    }

    /// Set a section's generator algorithm by index (0 = threshold, 1 = BET). Only rewrites the
    /// generator when the algorithm type actually changes, preserving existing parameters.
    pub fn set_mask_generator(&mut self, section: usize, idx: usize) {
        use crate::pipeline::config::*;
        if section >= self.mask_sections.len() { return; }
        let is_threshold = matches!(self.mask_sections[section].generator, MaskOp::Threshold { .. });
        match idx {
            0 if !is_threshold => {
                self.mask_sections[section].generator = MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None };
                self.mark_mask_custom();
            }
            1 if is_threshold => {
                self.mask_sections[section].generator = MaskOp::Bet { fractional_intensity: 0.5 };
                self.mark_mask_custom();
            }
            _ => {}
        }
    }

    /// Ordered threshold methods, matching the modal/cycle option order.
    pub const MASK_THRESHOLD_METHODS: [crate::pipeline::config::MaskThresholdMethod; 3] = {
        use crate::pipeline::config::MaskThresholdMethod::*;
        [Otsu, Fixed, Percentile]
    };

    /// Current threshold-method index for a section (0 if the generator isn't a threshold).
    pub fn mask_threshold_method_index(&self, section: usize) -> usize {
        use crate::pipeline::config::MaskOp;
        match self.mask_sections.get(section).map(|s| &s.generator) {
            Some(MaskOp::Threshold { method, .. }) => {
                Self::MASK_THRESHOLD_METHODS.iter().position(|m| m == method).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Set a section's threshold method by index (from the modal). No-op unless the
    /// generator is a threshold; the existing threshold value is preserved.
    pub fn set_mask_threshold_method(&mut self, section: usize, idx: usize) {
        use crate::pipeline::config::MaskOp;
        if section >= self.mask_sections.len() { return; }
        if let (MaskOp::Threshold { method, .. }, Some(&new_method)) =
            (&mut self.mask_sections[section].generator, Self::MASK_THRESHOLD_METHODS.get(idx))
        {
            *method = new_method;
            self.mark_mask_custom();
        }
    }

    /// Adjust the input source of a mask section with left/right.
    pub fn adjust_mask_input(&mut self, section: usize, delta: isize) {
        use crate::pipeline::config::MaskingInput;
        if section >= self.mask_sections.len() { return; }
        let sources = [MaskingInput::MagnitudeFirst, MaskingInput::Magnitude, MaskingInput::MagnitudeLast, MaskingInput::PhaseQuality];
        let cur = sources.iter().position(|s| *s == self.mask_sections[section].input).unwrap_or(0) as isize;
        let new = (cur + delta).rem_euclid(sources.len() as isize) as usize;
        self.mask_sections[section].input = sources[new];
        self.mark_mask_custom();
    }

    /// Adjust a mask op parameter with left/right.
    pub fn adjust_mask_op(&mut self, section: usize, index: usize, delta: isize) {
        use crate::pipeline::config::*;
        if section >= self.mask_sections.len() { return; }
        if index >= self.mask_sections[section].refinements.len() { return; }
        match &mut self.mask_sections[section].refinements[index] {
            MaskOp::Threshold { method, .. } => {
                let methods = [MaskThresholdMethod::Otsu, MaskThresholdMethod::Fixed, MaskThresholdMethod::Percentile];
                let cur = methods.iter().position(|m| m == method).unwrap_or(0) as isize;
                let new = (cur + delta).rem_euclid(methods.len() as isize) as usize;
                *method = methods[new];
            }
            MaskOp::Bet { fractional_intensity } => {
                *fractional_intensity = (*fractional_intensity + delta as f64 * 0.1).clamp(0.0, 1.0);
            }
            MaskOp::Erode { iterations } => {
                *iterations = (*iterations as isize + delta).max(1) as usize;
            }
            MaskOp::Dilate { iterations } => {
                *iterations = (*iterations as isize + delta).max(1) as usize;
            }
            MaskOp::Close { radius } => {
                *radius = (*radius as isize + delta).max(1) as usize;
            }
            MaskOp::FillHoles { max_size } => {
                *max_size = (*max_size as isize + delta * 100).max(0) as usize;
            }
            MaskOp::GaussianSmooth { sigma_mm } => {
                *sigma_mm = (*sigma_mm + delta as f64 * 0.5).max(0.5);
            }
        }
        self.mark_mask_custom();
    }

    /// Get available op types for adding refinement steps (morphological only).
    pub fn available_op_types(&self, _section: usize) -> Vec<&'static str> {
        // Generator is fixed — only offer morphological refinement ops
        MASK_OP_TYPES.iter()
            .filter(|&&t| t != "threshold" && t != "bet")
            .copied()
            .collect()
    }

    /// Get focusable row count (excludes separators and headers).
    pub fn focusable_rows(&self) -> Vec<usize> {
        self.visible_rows()
            .iter()
            .enumerate()
            .filter(|(_, r)| !matches!(r, PipelineRow::Separator | PipelineRow::Note { .. } | PipelineRow::MaskSectionHeader { .. } | PipelineRow::MaskOrSeparator))
            .map(|(i, _)| i)
            .collect()
    }
}

pub struct App {
    pub active_tab: usize,
    pub active_field: usize,
    pub editing: bool,
    pub cursor_pos: usize,
    pub form: RunForm,
    pub filter_state: FilterTreeState,
    pub pipeline_state: PipelineFormState,
    pub should_quit: bool,
    pub should_run: bool,
    pub tab_fields: Vec<Vec<FieldDef>>,
    pub form_scroll_offset: usize,
    pub methods_scroll_offset: usize,
    pub error_message: Option<String>,
    pub input_mode: InputMode,
    pub dicom_state: DicomConvertState,
    pub nifti_state: NiftiState,
    /// When set, an algorithm-selection modal is open over the QSM/Separation tab.
    pub algo_modal: Option<AlgoModal>,
}

/// A pop-up list for choosing one option of a pipeline `AlgoSelect` (all options at once).
#[derive(Debug, Clone)]
pub struct AlgoModal {
    /// What this modal edits, applied on confirm.
    pub target: AlgoModalTarget,
    /// Modal title (the row label, e.g. "QSM Inversion").
    pub title: String,
    pub options: Vec<String>,
    /// Per-option help text (same length as `options`).
    pub help: Vec<String>,
    /// Currently highlighted option index.
    pub cursor: usize,
}

/// The selection an [`AlgoModal`] applies when confirmed. Lets a single pop-up list serve every
/// single-choice selector in the TUI regardless of how that selector stores its value.
#[derive(Debug, Clone)]
pub enum AlgoModalTarget {
    /// A keyed `PipelineFormState` select (an `AlgoSelect` row, e.g. "qsm_algorithm").
    PipelineSelect(String),
    /// The masking input source of a mask section.
    MaskInput(usize),
    /// The generator algorithm (threshold vs BET) of a mask section.
    MaskGenerator(usize),
    /// The threshold method (otsu / fixed / percentile) of a mask section's generator.
    MaskThresholdMethod(usize),
    /// The Input-tab mode selector (BIDS / NIfTI / DICOM).
    InputMode,
    /// A generic `tab_fields` Select at the given (tab, field) — e.g. Execution Mode, SWI Scaling.
    TabSelect { tab: usize, field: usize },
}

pub struct RunForm {
    // Tab 0: Input/Output
    pub bids_dir: String,
    pub output_dir: String,
    pub config_file: String,

    // Tab 3: Supplementary
    pub do_swi: bool,
    pub swi_scaling: usize,  // 0=tanh, 1=negative-tanh, 2=positive, 3=negative, 4=triangular
    pub swi_strength: String,
    pub swi_hp_sigma_x: String,
    pub swi_hp_sigma_y: String,
    pub swi_hp_sigma_z: String,
    pub swi_mip_window: String,
    pub do_t2starmap: bool,
    pub do_r2starmap: bool,
    pub do_r2map: bool,
    pub do_r2primemap: bool,
    pub export_dicom: bool,

    // Tab 4: Execution
    pub execution_mode: usize, // 0=Local, 1=SLURM
    pub dry_run: bool,
    pub debug: bool,
    pub n_procs: String,
    // SLURM fields
    pub slurm_account: String,
    pub slurm_partition: String,
    pub slurm_time: String,
    pub slurm_mem: String,
    pub slurm_cpus: String,
    pub slurm_submit: bool,
}

impl Default for RunForm {
    fn default() -> Self {
        let swi = qsm_core::swi::SwiParams::default();
        Self {
            bids_dir: String::new(),
            output_dir: String::new(),
            config_file: String::new(),
            do_swi: false,
            swi_scaling: 0,
            swi_strength: format!("{}", swi.strength),
            swi_hp_sigma_x: format!("{}", swi.hp_sigma[0]),
            swi_hp_sigma_y: format!("{}", swi.hp_sigma[1]),
            swi_hp_sigma_z: format!("{}", swi.hp_sigma[2]),
            swi_mip_window: format!("{}", swi.mip_window),
            do_t2starmap: false,
            do_r2starmap: false,
            do_r2map: false,
            do_r2primemap: false,
            export_dicom: false,
            execution_mode: 0,
            dry_run: false,
            debug: false,
            n_procs: String::new(),
            slurm_account: String::new(),
            slurm_partition: String::new(),
            slurm_time: "02:00:00".to_string(),
            slurm_mem: "32".to_string(),
            slurm_cpus: "4".to_string(),
            slurm_submit: false,
        }
    }
}

impl App {
    pub fn new() -> Self {
        let tab_fields = vec![
            // Tab 0: Input (custom rendering — IO fields + filter tree)
            vec![],
            // Tab 1: QSM (custom rendering — see PipelineFormState)
            vec![],
            // Tab 2: Separation (custom rendering — see PipelineFormState::separation_visible_rows)
            vec![],
            // Tab 3: Supplementary
            vec![
                FieldDef {
                    label: "Compute SWI",
                    kind: FieldKind::Checkbox,
                    help: "Also compute susceptibility-weighted images",
                },
                FieldDef {
                    label: "SWI Scaling",
                    kind: FieldKind::Select { options: vec!["tanh", "negative-tanh", "positive", "negative", "triangular"] },
                    help: "Phase scaling type for SWI",
                },
                FieldDef {
                    label: "SWI Strength",
                    kind: FieldKind::Text,
                    help: "Phase scaling strength (higher = stronger phase contrast)",
                },
                FieldDef {
                    label: "SWI HP Sigma X",
                    kind: FieldKind::Text,
                    help: "High-pass filter sigma in X (voxels). Controls background phase removal.",
                },
                FieldDef {
                    label: "SWI HP Sigma Y",
                    kind: FieldKind::Text,
                    help: "High-pass filter sigma in Y (voxels).",
                },
                FieldDef {
                    label: "SWI HP Sigma Z",
                    kind: FieldKind::Text,
                    help: "High-pass filter sigma in Z (voxels). Set to 0 for thin axial slices.",
                },
                FieldDef {
                    label: "SWI MIP Window",
                    kind: FieldKind::Text,
                    help: "Minimum intensity projection window size in slices",
                },
                FieldDef {
                    label: "Compute T2* Map",
                    kind: FieldKind::Checkbox,
                    help: "Compute T2* relaxation map (requires 3+ echoes with magnitude)",
                },
                FieldDef {
                    label: "Compute R2* Map",
                    kind: FieldKind::Checkbox,
                    help: "Compute R2* decay rate map (requires 3+ echoes with magnitude)",
                },
                FieldDef {
                    label: "Compute R2 Map",
                    kind: FieldKind::Checkbox,
                    help: "R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG. Forced on when chi-separation is enabled.",
                },
                FieldDef {
                    label: "Compute R2' Map",
                    kind: FieldKind::Checkbox,
                    help: "R2' = R2* − R2 (Hz); needs GRE magnitude + a MESE acquisition. Forced on when chi-separation is enabled.",
                },
                FieldDef {
                    label: "Export DICOM",
                    kind: FieldKind::Checkbox,
                    help: "Also write final maps as DICOM series into each subject's extra_files/ folder",
                },
            ],
            // Tab 4: Execution
            vec![
                FieldDef {
                    label: "Execution Mode",
                    kind: FieldKind::Select { options: vec!["Local", "SLURM"] },
                    help: "Local execution or generate SLURM job scripts",
                },
                FieldDef {
                    label: "Dry Run",
                    kind: FieldKind::Checkbox,
                    help: "Print processing plan without executing",
                },
                FieldDef {
                    label: "Debug Logging",
                    kind: FieldKind::Checkbox,
                    help: "Enable verbose debug log output",
                },
                FieldDef {
                    label: "Num Processes",
                    kind: FieldKind::Text,
                    help: "Number of parallel threads (empty = auto)",
                },
                // SLURM-specific fields
                FieldDef {
                    label: "SLURM Account",
                    kind: FieldKind::Text,
                    help: "SLURM account name (required for SLURM mode)",
                },
                FieldDef {
                    label: "SLURM Partition",
                    kind: FieldKind::Text,
                    help: "SLURM partition (optional)",
                },
                FieldDef {
                    label: "SLURM Time Limit",
                    kind: FieldKind::Text,
                    help: "Wall time limit per job (e.g. 02:00:00)",
                },
                FieldDef {
                    label: "SLURM Memory (GB)",
                    kind: FieldKind::Text,
                    help: "Memory per job in GB",
                },
                FieldDef {
                    label: "SLURM CPUs/Task",
                    kind: FieldKind::Text,
                    help: "CPUs per SLURM task",
                },
                FieldDef {
                    label: "Auto-Submit",
                    kind: FieldKind::Checkbox,
                    help: "Automatically submit generated scripts via sbatch",
                },
            ],
            // Tab 4: Methods (read-only, custom rendering)
            vec![],
        ];

        App {
            active_tab: 0,
            active_field: 0,
            editing: false,
            cursor_pos: 0,
            form: RunForm::default(),
            filter_state: FilterTreeState::default(),
            pipeline_state: PipelineFormState::default(),
            should_quit: false,
            should_run: false,
            tab_fields,
            form_scroll_offset: 0,
            methods_scroll_offset: 0,
            error_message: None,
            algo_modal: None,
            input_mode: InputMode::Bids,
            dicom_state: DicomConvertState::default(),
            nifti_state: NiftiState::default(),
        }
    }

    pub fn field_count(&self) -> usize {
        self.tab_fields[self.active_tab].len()
    }

    pub fn current_field(&self) -> &FieldDef {
        &self.tab_fields[self.active_tab][self.active_field]
    }

    /// Validate required fields and either set should_run or show error.
    pub fn try_run(&mut self) {
        self.error_message = None;

        // If in DICOM mode, must convert first
        if self.input_mode == InputMode::DicomToBids
            && self.dicom_state.convert_status != ConvertStatus::Done
        {
            self.error_message = Some("Convert DICOM to BIDS first (Enter on Convert button)".to_string());
            self.active_tab = 0;
            return;
        }

        // NIfTI mode: must convert first
        if self.input_mode == InputMode::NIfTI
            && self.nifti_state.convert_status != ConvertStatus::Done
        {
            self.error_message = Some("Convert NIfTI to BIDS first (Enter on Convert button)".to_string());
            self.active_tab = 0;
            return;
        }

        // BIDS directory is always required
        if self.form.bids_dir.trim().is_empty() {
            self.error_message = Some("BIDS Directory is required".to_string());
            self.active_tab = 0;
            self.active_field = 0;
            return;
        }

        // SLURM mode requires account
        if self.form.execution_mode == 1 && self.form.slurm_account.trim().is_empty() {
            self.error_message = Some("SLURM Account is required".to_string());
            self.active_tab = 3;
            self.active_field = 4;
            return;
        }

        self.should_run = true;
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // An open algorithm-selection modal captures all input until dismissed.
        if self.algo_modal.is_some() {
            self.handle_algo_modal_key(key);
            return;
        }

        // Clear error on any non-F5 key press
        if key.code != KeyCode::F(5) {
            self.error_message = None;
        }

        // Route tab 0 (Input) to its combined IO + filter handler
        if self.active_tab == TAB_INPUT {
            self.handle_input_tab_key(key);
            return;
        }
        // Route the QSM and Separation tabs to the shared pipeline-row handler (the row set
        // is selected by `pipeline_state.mode`).
        if self.active_tab == TAB_QSM || self.active_tab == TAB_SEPARATION {
            self.sync_pipeline_mode();
            self.handle_pipeline_key(key);
            return;
        }

        // Route tab Methods — read-only, only tab switching and scrolling
        if self.active_tab == TAB_METHODS {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char(c @ '1'..='6') => {
                    self.active_tab = (c as usize) - ('1' as usize);
                    self.active_field = 0;
                }
                KeyCode::Tab => {
                    self.active_tab = (self.active_tab + 1) % TAB_NAMES.len();
                    self.active_field = 0;
                }
                KeyCode::BackTab => {
                    self.active_tab = (self.active_tab + TAB_NAMES.len() - 1) % TAB_NAMES.len();
                    self.active_field = 0;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.methods_scroll_offset = self.methods_scroll_offset.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.methods_scroll_offset += 1;
                }
                KeyCode::F(5) | KeyCode::Enter => self.try_run(),
                _ => {}
            }
            return;
        }

        if self.editing {
            self.handle_editing_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // Tab switching
            KeyCode::Char(c @ '1'..='6') => {
                self.active_tab = (c as usize) - ('1' as usize);
                self.active_field = 0;
            }
            KeyCode::Tab => {
                self.active_tab = (self.active_tab + 1) % TAB_NAMES.len();
                self.active_field = 0;
            }
            KeyCode::BackTab => {
                self.active_tab = (self.active_tab + TAB_NAMES.len() - 1) % TAB_NAMES.len();
                self.active_field = 0;
            }

            // Field navigation (skip hidden fields)
            KeyCode::Up | KeyCode::Char('k')
                if self.active_field > 0 => {
                    let mut f = self.active_field - 1;
                    while f > 0 && !self.is_field_visible(self.active_tab, f) {
                        f -= 1;
                    }
                    if self.is_field_visible(self.active_tab, f) {
                        self.active_field = f;
                    }
                }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.field_count().saturating_sub(1);
                let mut f = self.active_field + 1;
                while f <= max && !self.is_field_visible(self.active_tab, f) {
                    f += 1;
                }
                if f <= max && self.is_field_visible(self.active_tab, f) {
                    self.active_field = f;
                }
            }

            // Field interaction
            KeyCode::Enter | KeyCode::Char(' ') => self.interact_field(),
            KeyCode::Left => self.adjust_select(-1),
            KeyCode::Right => self.adjust_select(1),

            // Reset focused field to default
            KeyCode::Char('r') => self.reset_current_field(),
            // Reset all fields on current tab to defaults
            KeyCode::Char('R') => self.reset_current_tab(),

            // Run
            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    // ─── Filter tab key handling ───

    /// Number of IO fields at the top of the Input tab
    /// Field 0: Input Mode selector, Fields 1-3: text fields
    pub const INPUT_IO_FIELDS: usize = 4; // mode, bids_dir/dicom_dir, output_dir, config_file

    fn handle_input_tab_key(&mut self, key: KeyEvent) {
        // If in DICOM mode and past the IO fields, delegate to DICOM handler
        if self.input_mode == InputMode::DicomToBids && self.active_field >= Self::INPUT_IO_FIELDS {
            self.handle_dicom_tab_key(key);
            return;
        }
        // If in NIfTI mode and past the IO fields, delegate to NIfTI handler
        if self.input_mode == InputMode::NIfTI && self.active_field >= Self::INPUT_IO_FIELDS {
            self.handle_nifti_tab_key(key);
            return;
        }

        let in_io = self.active_field < Self::INPUT_IO_FIELDS;

        if in_io {
            // Handle IO field editing (not for field 0 which is a selector)
            if self.editing && self.active_field > 0 {
                let was_editing = self.editing;
                self.handle_editing_key(key);
                let stopped_editing = was_editing && !self.editing;

                // BIDS mode: rescan on every keystroke (fast, just globs)
                // DICOM/NIfTI mode: only rescan when editing finishes (slow, reads files)
                if self.input_mode == InputMode::Bids {
                    let bids_dir = self.form.bids_dir.clone();
                    self.filter_state.maybe_rescan(&bids_dir);
                } else if self.input_mode == InputMode::NIfTI && stopped_editing && self.active_field == 1 {
                    self.nifti_state.scan_input_directory();
                } else if stopped_editing && self.active_field == 1 {
                    // Only scan when user finishes editing the DICOM directory field
                    self.dicom_state.maybe_rescan();
                }
                return;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char(c @ '1'..='6') => {
                    self.active_tab = (c as usize) - ('1' as usize);
                    self.active_field = 0;
                }
                KeyCode::Tab => {
                    self.active_tab = (self.active_tab + 1) % TAB_NAMES.len();
                    self.active_field = 0;
                }
                KeyCode::BackTab => {
                    self.active_tab = (self.active_tab + TAB_NAMES.len() - 1) % TAB_NAMES.len();
                    self.active_field = 0;
                }
                KeyCode::Up | KeyCode::Char('k') if self.active_field > 0 => {
                    self.active_field -= 1;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.active_field + 1 < Self::INPUT_IO_FIELDS {
                        self.active_field += 1;
                    } else {
                        let has_content = match self.input_mode {
                            InputMode::Bids => self.filter_state.tree.is_some(),
                            InputMode::NIfTI => true, // always has NIfTI fields below
                            InputMode::DicomToBids => self.dicom_state.session.is_some(),
                        };
                        if has_content {
                            self.active_field = Self::INPUT_IO_FIELDS;
                        }
                    }
                }
                KeyCode::Enter | KeyCode::Char(' ') => self.interact_io_field(),
                KeyCode::Left => self.adjust_io_select(-1),
                KeyCode::Right => self.adjust_io_select(1),
                KeyCode::Char('r') => self.reset_current_field(),
                KeyCode::Char('R') => self.reset_current_tab(),
                KeyCode::F(5) => self.try_run(),
                _ => {}
            }
            // Trigger rescan
            if self.input_mode == InputMode::Bids {
                let bids_dir = self.form.bids_dir.clone();
                self.filter_state.maybe_rescan(&bids_dir);
            } else if self.input_mode == InputMode::DicomToBids {
                self.dicom_state.maybe_rescan();
            }
            // NIfTI mode: no auto-rescan on navigation, only on edit finish
        } else if self.input_mode == InputMode::Bids {
            // BIDS filter tree
            if self.filter_state.include_editing || self.filter_state.exclude_editing || self.filter_state.num_echoes_editing {
                self.handle_filter_key(key);
                return;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') if self.filter_state.focus == FilterFocus::Include => {
                    self.active_field = Self::INPUT_IO_FIELDS - 1;
                }
                _ => self.handle_filter_key(key),
            }
        } else {
            // DICOM series tree (active_field >= INPUT_IO_FIELDS)
            self.handle_dicom_tab_key(key);
        }
    }

    fn interact_io_field(&mut self) {
        if self.active_field == 0 {
            // Mode selector: open a modal listing all input modes.
            self.algo_modal = Some(AlgoModal {
                target: AlgoModalTarget::InputMode,
                title: "Input Mode".to_string(),
                options: vec![
                    "BIDS".to_string(),
                    "NIfTI -> BIDS".to_string(),
                    "DICOM -> BIDS".to_string(),
                ],
                help: vec![
                    "Standard BIDS dataset input".to_string(),
                    "Convert loose NIfTI files to BIDS (experimental)".to_string(),
                    "Convert DICOM series to BIDS (experimental)".to_string(),
                ],
                cursor: match self.input_mode {
                    InputMode::Bids => 0,
                    InputMode::NIfTI => 1,
                    InputMode::DicomToBids => 2,
                },
            });
            return;
        }
        // Text fields (1-3)
        self.editing = true;
        self.cursor_pos = match self.active_field {
            1 => match self.input_mode {
                InputMode::Bids => self.form.bids_dir.len(),
                InputMode::NIfTI => self.nifti_state.input_dir.len(),
                InputMode::DicomToBids => self.dicom_state.dicom_dir.len(),
            },
            2 => match self.input_mode {
                InputMode::Bids => self.form.output_dir.len(),
                InputMode::NIfTI => self.nifti_state.output_dir.len(),
                InputMode::DicomToBids => self.dicom_state.output_dir.len(),
            },
            3 => self.form.config_file.len(),
            _ => 0,
        };
    }

    fn adjust_io_select(&mut self, delta: isize) {
        if self.active_field == 0 {
            // Mode selector: Left/Right cycles through modes
            self.cycle_input_mode(delta);
        }
    }

    fn cycle_input_mode(&mut self, delta: isize) {
        const MODES: [InputMode; 3] = [InputMode::Bids, InputMode::NIfTI, InputMode::DicomToBids];
        let cur = MODES.iter().position(|m| *m == self.input_mode).unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(MODES.len() as isize) as usize;
        self.input_mode = MODES[next];
        self.form_scroll_offset = 0;
    }

    /// Reset the focused field on a generic form tab to its default.
    fn reset_current_field(&mut self) {
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
            },
            (0, 2) => match self.input_mode {
                InputMode::Bids => self.form.output_dir = defaults.output_dir.clone(),
                InputMode::NIfTI => self.nifti_state.output_dir = String::new(),
                InputMode::DicomToBids => self.dicom_state.output_dir = String::new(),
            },
            (0, 3) => self.form.config_file = defaults.config_file.clone(),
            // Tab 2 (Supplementary)
            (TAB_SUPPLEMENTARY, 0) => self.form.do_swi = defaults.do_swi,
            (TAB_SUPPLEMENTARY, 1) => self.form.swi_scaling = defaults.swi_scaling,
            (TAB_SUPPLEMENTARY, 2) => self.form.swi_strength = defaults.swi_strength.clone(),
            (TAB_SUPPLEMENTARY, 3) => self.form.swi_hp_sigma_x = defaults.swi_hp_sigma_x.clone(),
            (TAB_SUPPLEMENTARY, 4) => self.form.swi_hp_sigma_y = defaults.swi_hp_sigma_y.clone(),
            (TAB_SUPPLEMENTARY, 5) => self.form.swi_hp_sigma_z = defaults.swi_hp_sigma_z.clone(),
            (TAB_SUPPLEMENTARY, 6) => self.form.swi_mip_window = defaults.swi_mip_window.clone(),
            (TAB_SUPPLEMENTARY, 7) => self.form.do_t2starmap = defaults.do_t2starmap,
            (TAB_SUPPLEMENTARY, 8) => self.form.do_r2starmap = defaults.do_r2starmap,
            (TAB_SUPPLEMENTARY, 9) => self.form.do_r2map = defaults.do_r2map,
            (TAB_SUPPLEMENTARY, 10) => self.form.do_r2primemap = defaults.do_r2primemap,
            (TAB_SUPPLEMENTARY, 11) => self.form.export_dicom = defaults.export_dicom,
            // Tab 3 (Execution)
            (TAB_EXECUTION, 0) => self.form.execution_mode = defaults.execution_mode,
            (TAB_EXECUTION, 1) => self.form.dry_run = defaults.dry_run,
            (TAB_EXECUTION, 2) => self.form.debug = defaults.debug,
            (TAB_EXECUTION, 3) => self.form.n_procs = defaults.n_procs.clone(),
            (TAB_EXECUTION, 4) => self.form.slurm_account = defaults.slurm_account.clone(),
            (TAB_EXECUTION, 5) => self.form.slurm_partition = defaults.slurm_partition.clone(),
            (TAB_EXECUTION, 6) => self.form.slurm_time = defaults.slurm_time.clone(),
            (TAB_EXECUTION, 7) => self.form.slurm_mem = defaults.slurm_mem.clone(),
            (TAB_EXECUTION, 8) => self.form.slurm_cpus = defaults.slurm_cpus.clone(),
            (TAB_EXECUTION, 9) => self.form.slurm_submit = defaults.slurm_submit,
            _ => {}
        }
    }

    /// Reset all fields on the current generic form tab to defaults.
    fn reset_current_tab(&mut self) {
        let defaults = RunForm::default();
        match self.active_tab {
            TAB_INPUT => {
                self.input_mode = InputMode::Bids;
                self.form.bids_dir = defaults.bids_dir.clone();
                self.form.output_dir = defaults.output_dir.clone();
                self.form.config_file = defaults.config_file.clone();
                self.dicom_state = DicomConvertState::default();
                self.nifti_state = NiftiState::default();
            }
            TAB_SUPPLEMENTARY => {
                self.form.do_swi = defaults.do_swi;
                self.form.swi_scaling = defaults.swi_scaling;
                self.form.swi_strength = defaults.swi_strength.clone();
                self.form.swi_hp_sigma_x = defaults.swi_hp_sigma_x.clone();
                self.form.swi_hp_sigma_y = defaults.swi_hp_sigma_y.clone();
                self.form.swi_hp_sigma_z = defaults.swi_hp_sigma_z.clone();
                self.form.swi_mip_window = defaults.swi_mip_window.clone();
                self.form.do_t2starmap = defaults.do_t2starmap;
                self.form.do_r2starmap = defaults.do_r2starmap;
                self.form.do_r2map = defaults.do_r2map;
                self.form.do_r2primemap = defaults.do_r2primemap;
                self.form.export_dicom = defaults.export_dicom;
            }
            TAB_EXECUTION => {
                self.form.execution_mode = defaults.execution_mode;
                self.form.dry_run = defaults.dry_run;
                self.form.debug = defaults.debug;
                self.form.n_procs = defaults.n_procs.clone();
                self.form.slurm_account = defaults.slurm_account.clone();
                self.form.slurm_partition = defaults.slurm_partition.clone();
                self.form.slurm_time = defaults.slurm_time.clone();
                self.form.slurm_mem = defaults.slurm_mem.clone();
                self.form.slurm_cpus = defaults.slurm_cpus.clone();
                self.form.slurm_submit = defaults.slurm_submit;
            }
            _ => {}
        }
    }

    /// Reset the focused pipeline field to its default.
    fn reset_pipeline_field(&mut self) {
        let ps = &mut self.pipeline_state;
        let defaults = PipelineFormState::default();
        let rows = ps.visible_rows();
        let focusable = ps.focusable_rows();
        let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);

        match rows.get(focus_idx) {
            Some(PipelineRow::AlgoSelect { field, .. }) => {
                ps.set_select(field, defaults.get_select(field));
            }
            Some(PipelineRow::Param { field, .. }) => {
                let default_val = defaults.get_param(field).to_string();
                if let Some(s) = ps.get_param_mut(field) {
                    *s = default_val;
                }
            }
            Some(PipelineRow::Toggle { field, .. }) => {
                let default_val = defaults.get_toggle(field);
                if ps.get_toggle(field) != default_val {
                    ps.toggle(field);
                }
            }
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        // Handle editing mode (include, exclude, or num_echoes text input)
        if self.filter_state.include_editing {
            self.handle_filter_text_key(key, FilterTextField::Include);
            return;
        }
        if self.filter_state.exclude_editing {
            self.handle_filter_text_key(key, FilterTextField::Exclude);
            return;
        }
        if self.filter_state.num_echoes_editing {
            self.handle_filter_num_echoes_key(key);
            return;
        }

        // Navigation mode
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // Tab switching (same as other tabs)
            KeyCode::Char(c @ '1'..='6') => {
                self.active_tab = (c as usize) - ('1' as usize);
                self.active_field = 0;
            }
            KeyCode::Tab => {
                self.active_tab = (self.active_tab + 1) % TAB_NAMES.len();
                self.active_field = 0;
            }
            KeyCode::BackTab => {
                self.active_tab = (self.active_tab + TAB_NAMES.len() - 1) % TAB_NAMES.len();
                self.active_field = 0;
            }

            // Navigation within filter tree
            KeyCode::Up | KeyCode::Char('k') => self.filter_state.focus_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.filter_state.focus_next(),

            // Collapse/expand
            KeyCode::Left => self.filter_state.toggle_collapse(),
            KeyCode::Right => self.filter_state.toggle_collapse(),

            // Toggle / interact
            KeyCode::Char(' ') => {
                self.filter_state.toggle_focused();
                self.filter_state.manual_override = true;
            }
            KeyCode::Enter => {
                match self.filter_state.focus {
                    FilterFocus::Include => {
                        self.filter_state.include_editing = true;
                        self.filter_state.include_cursor = self.filter_state.include_pattern.len();
                    }
                    FilterFocus::Exclude => {
                        self.filter_state.exclude_editing = true;
                        self.filter_state.exclude_cursor = self.filter_state.exclude_pattern.len();
                    }
                    FilterFocus::TreeNode(_) => {
                        self.filter_state.toggle_focused();
                        self.filter_state.manual_override = true;
                    }
                    FilterFocus::NumEchoes => {
                        self.filter_state.num_echoes_editing = true;
                        self.filter_state.num_echoes_cursor = self.filter_state.num_echoes.len();
                    }
                }
            }

            // Select all / none
            KeyCode::Char('a') => {
                if let Some(ref mut tree) = self.filter_state.tree {
                    tree.set_all(true);
                }
                self.filter_state.manual_override = true;
            }
            KeyCode::Char('n') => {
                if let Some(ref mut tree) = self.filter_state.tree {
                    tree.set_all(false);
                }
                self.filter_state.manual_override = true;
            }

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    fn handle_filter_text_key(&mut self, key: KeyEvent, field: FilterTextField) {
        let (text, cursor, editing) = match field {
            FilterTextField::Include => (
                &mut self.filter_state.include_pattern,
                &mut self.filter_state.include_cursor,
                &mut self.filter_state.include_editing,
            ),
            FilterTextField::Exclude => (
                &mut self.filter_state.exclude_pattern,
                &mut self.filter_state.exclude_cursor,
                &mut self.filter_state.exclude_editing,
            ),
        };
        match key.code {
            KeyCode::Esc => {
                *editing = false;
            }
            KeyCode::Enter => {
                *editing = false;
                self.filter_state.apply_include_exclude();
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
            KeyCode::Delete if *cursor < text.len() => {
                text.remove(*cursor);
            }
            KeyCode::Left => *cursor = cursor.saturating_sub(1),
            KeyCode::Right if *cursor < text.len() => {
                *cursor += 1;
            }
            KeyCode::Home => *cursor = 0,
            KeyCode::End => *cursor = text.len(),
            _ => {}
        }
    }

    fn handle_filter_num_echoes_key(&mut self, key: KeyEvent) {
        let fs = &mut self.filter_state;
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                fs.num_echoes_editing = false;
            }
            KeyCode::Char(c) => {
                fs.num_echoes.insert(fs.num_echoes_cursor, c);
                fs.num_echoes_cursor += 1;
            }
            KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && fs.num_echoes_cursor > 0 => {
                let new_pos = word_boundary_left(&fs.num_echoes, fs.num_echoes_cursor);
                fs.num_echoes.drain(new_pos..fs.num_echoes_cursor);
                fs.num_echoes_cursor = new_pos;
            }
            KeyCode::Backspace if fs.num_echoes_cursor > 0 => {
                fs.num_echoes_cursor -= 1;
                fs.num_echoes.remove(fs.num_echoes_cursor);
            }
            KeyCode::Delete
                if fs.num_echoes_cursor < fs.num_echoes.len() => {
                    fs.num_echoes.remove(fs.num_echoes_cursor);
                }
            KeyCode::Left => fs.num_echoes_cursor = fs.num_echoes_cursor.saturating_sub(1),
            KeyCode::Right
                if fs.num_echoes_cursor < fs.num_echoes.len() => {
                    fs.num_echoes_cursor += 1;
                }
            KeyCode::Home => fs.num_echoes_cursor = 0,
            KeyCode::End => fs.num_echoes_cursor = fs.num_echoes.len(),
            _ => {}
        }
    }

    // ─── DICOM series tree key handling ───
    // Handles navigation within the DICOM series tree area (active_field >= INPUT_IO_FIELDS).
    // IO fields (mode selector, directories) are handled by handle_input_tab_key.

    fn handle_dicom_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            KeyCode::Char(c @ '1'..='6') => {
                self.active_tab = (c as usize) - ('1' as usize);
                self.active_field = 0;
            }
            KeyCode::Tab => {
                self.active_tab = (self.active_tab + 1) % TAB_NAMES.len();
                self.active_field = 0;
            }
            KeyCode::BackTab => {
                self.active_tab = (self.active_tab + TAB_NAMES.len() - 1) % TAB_NAMES.len();
                self.active_field = 0;
            }

            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if self.dicom_state.focus == DicomFocus::Series(0) {
                    // Go back to IO fields
                    self.active_field = Self::INPUT_IO_FIELDS - 1;
                } else {
                    self.dicom_state.focus_prev();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.dicom_state.focus_next(),

            // Cycle series type — relabel the focused UNIQUE series, propagating to
            // every subject instance that shares its identity.
            KeyCode::Left => self.relabel_focused_unique_series(false),
            KeyCode::Right => self.relabel_focused_unique_series(true),

            // Interact
            KeyCode::Enter | KeyCode::Char(' ') => {
                match self.dicom_state.focus {
                    DicomFocus::Series(_) => self.relabel_focused_unique_series(true),
                    DicomFocus::ConvertButton => {
                        self.run_dicom_conversion();
                    }
                }
            }

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    /// Launch the DICOM → BIDS conversion in a background thread.
    /// Cycle the classification of the focused unique series and propagate the new
    /// type to every subject instance sharing its identity.
    fn relabel_focused_unique_series(&mut self, forward: bool) {
        let DicomFocus::Series(i) = self.dicom_state.focus else { return };
        let Some(session) = self.dicom_state.session.as_mut() else { return };
        let groups = session.unique_series();
        if let Some(g) = groups.get(i) {
            let cur = session.series_ref(&g.refs[0]).series_type;
            let new_type = if forward { cur.next() } else { cur.prev() };
            session.set_type_for_refs(&g.refs, new_type);
        }
    }

    fn run_dicom_conversion(&mut self) {
        if self.dicom_state.session.is_none() {
            self.error_message = Some("No DICOM session loaded".to_string());
            return;
        }

        if dicom::convert::find_dcm2niix().is_none() {
            self.error_message = Some(
                "dcm2niix not found (no bundled copy in ~/.qsmxt/bin and not on PATH). \
                 Reinstall qsmxt or install dcm2niix."
                    .to_string(),
            );
            return;
        }

        if self.dicom_state.convert_status == ConvertStatus::Converting {
            return; // already running
        }

        // Determine output directory
        let output_dir = if self.dicom_state.output_dir.trim().is_empty() {
            let dicom_dir = self.dicom_state.dicom_dir.trim().to_string();
            let expanded = if let Some(rest) = dicom_dir.strip_prefix("~/") {
                if let Some(home) = std::env::var_os("HOME") {
                    format!("{}/{}", home.to_string_lossy(), rest)
                } else {
                    dicom_dir.clone()
                }
            } else {
                dicom_dir.clone()
            };
            let p = std::path::Path::new(&expanded);
            let parent = p.parent().unwrap_or(p);
            parent.join("bids_output").to_string_lossy().to_string()
        } else {
            let d = self.dicom_state.output_dir.trim().to_string();
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

        self.dicom_state.convert_status = ConvertStatus::Converting;
        self.dicom_state.convert_log.clear();
        self.dicom_state.convert_errors.clear();
        self.dicom_state.convert_log.push("Starting DICOM to BIDS conversion...".to_string());

        let session = self.dicom_state.session.clone().unwrap();
        let output_path = std::path::PathBuf::from(&output_dir);
        let (tx, rx) = mpsc::channel();
        self.dicom_state.convert_receiver = Some(rx);

        std::thread::spawn(move || {
            dicom::convert::convert_session_streaming(&session, &output_path, &tx);
        });
    }

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

    fn handle_nifti_tab_key(&mut self, key: KeyEvent) {
        // Handle text editing mode
        if self.nifti_state.editing {
            self.handle_nifti_editing(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            KeyCode::Char(c @ '1'..='6') => {
                self.active_tab = (c as usize) - ('1' as usize);
                self.active_field = 0;
            }
            KeyCode::Tab => {
                self.active_tab = (self.active_tab + 1) % TAB_NAMES.len();
                self.active_field = 0;
            }
            KeyCode::BackTab => {
                self.active_tab = (self.active_tab + TAB_NAMES.len() - 1) % TAB_NAMES.len();
                self.active_field = 0;
            }

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
            KeyCode::Enter | KeyCode::Char(' ') => {
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

            KeyCode::F(5) => self.try_run(),

            _ => {}
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

    // ─── Pipeline tab key handling ───

    /// Keys while the algorithm-selection modal is open: ↑↓/jk move, Enter/Space confirm, Esc/q cancel.
    fn handle_algo_modal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(m) = self.algo_modal.as_mut() {
                    m.cursor = m.cursor.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(m) = self.algo_modal.as_mut() {
                    if m.cursor + 1 < m.options.len() {
                        m.cursor += 1;
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(m) = self.algo_modal.take() {
                    self.apply_algo_modal(m);
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.algo_modal = None,
            _ => {}
        }
    }

    /// Apply a confirmed [`AlgoModal`] selection to whatever it targets.
    fn apply_algo_modal(&mut self, m: AlgoModal) {
        match m.target {
            AlgoModalTarget::PipelineSelect(field) => {
                self.pipeline_state.set_select(&field, m.cursor);
                // Keep focus on the selector row (its value changed; rows below may differ).
                self.pipeline_state.restore_focus(&Some(field));
            }
            AlgoModalTarget::MaskInput(section) => {
                self.pipeline_state.set_mask_input(section, m.cursor);
            }
            AlgoModalTarget::MaskGenerator(section) => {
                self.pipeline_state.set_mask_generator(section, m.cursor);
            }
            AlgoModalTarget::MaskThresholdMethod(section) => {
                self.pipeline_state.set_mask_threshold_method(section, m.cursor);
            }
            AlgoModalTarget::InputMode => self.set_input_mode(m.cursor),
            AlgoModalTarget::TabSelect { tab, field } => {
                self.active_tab = tab;
                self.active_field = field;
                self.set_select_value(m.cursor);
            }
        }
    }

    /// Set the Input-tab mode by index (BIDS / NIfTI / DICOM).
    fn set_input_mode(&mut self, idx: usize) {
        const MODES: [InputMode; 3] = [InputMode::Bids, InputMode::NIfTI, InputMode::DicomToBids];
        if let Some(&m) = MODES.get(idx) {
            self.input_mode = m;
            self.form_scroll_offset = 0;
        }
    }

    /// Select the pipeline row set (QSM vs Separation) from the active tab. When the mode
    /// actually changes — i.e. the user switched between the QSM and Separation tabs — reset
    /// focus to the first selectable row so we never land on an out-of-range (invisible) row.
    /// Called from both key handling and rendering so the display is always consistent.
    pub fn sync_pipeline_mode(&mut self) {
        let new_mode = if self.active_tab == TAB_SEPARATION {
            PipelineTabMode::Separation
        } else {
            PipelineTabMode::Qsm
        };
        if new_mode != self.pipeline_state.mode {
            self.pipeline_state.mode = new_mode;
            self.pipeline_state.focus = 0;
        }
    }

    fn handle_pipeline_key(&mut self, key: KeyEvent) {
        let ps = &mut self.pipeline_state;

        if ps.editing {
            // Text editing mode for a parameter
            let rows = ps.visible_rows();
            let focusable = ps.focusable_rows();
            let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);
            if let Some(PipelineRow::Param { field, .. }) = rows.get(focus_idx) {
                let field = field.to_string();
                let mut cursor = ps.cursor;
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => { ps.editing = false; return; }
                    KeyCode::Char(c) => {
                        if let Some(s) = ps.get_param_mut(&field) {
                            s.insert(cursor, c);
                            cursor += 1;
                        }
                    }
                    KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                        if let Some(s) = ps.get_param_mut(&field) {
                            let new_pos = word_boundary_left(s, cursor);
                            s.drain(new_pos..cursor);
                            cursor = new_pos;
                        }
                    }
                    KeyCode::Backspace if cursor > 0 => {
                        cursor -= 1;
                        if let Some(s) = ps.get_param_mut(&field) {
                            s.remove(cursor);
                        }
                    }
                    KeyCode::Left => cursor = cursor.saturating_sub(1),
                    KeyCode::Right => {
                        let len = ps.get_param(&field).len();
                        if cursor < len { cursor += 1; }
                    }
                    KeyCode::Home => cursor = 0,
                    KeyCode::End => cursor = ps.get_param(&field).len(),
                    _ => {}
                }
                ps.cursor = cursor;
            } else {
                ps.editing = false;
            }
            return;
        }

        // Threshold value editing mode
        if ps.mask_threshold_editing {
            let mut cursor = ps.cursor;
            match key.code {
                KeyCode::Esc => {
                    ps.mask_threshold_editing = false;
                    return;
                }
                KeyCode::Enter => {
                    // Save the value back to the generator
                    let val: Option<f64> = ps.mask_threshold_value_buf.trim().parse().ok();
                    let rows = ps.visible_rows();
                    let focusable = ps.focusable_rows();
                    let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);
                    if let Some(PipelineRow::MaskOpThresholdValue { section }) = rows.get(focus_idx) {
                        if let crate::pipeline::config::MaskOp::Threshold { value, .. } = &mut ps.mask_sections[*section].generator {
                            *value = val;
                        }
                    }
                    ps.mask_threshold_editing = false;
                    ps.mark_mask_custom();
                    return;
                }
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                    ps.mask_threshold_value_buf.insert(cursor, c);
                    cursor += 1;
                }
                KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                    let new_pos = word_boundary_left(&ps.mask_threshold_value_buf, cursor);
                    ps.mask_threshold_value_buf.drain(new_pos..cursor);
                    cursor = new_pos;
                }
                KeyCode::Backspace if cursor > 0 => {
                    cursor -= 1;
                    ps.mask_threshold_value_buf.remove(cursor);
                }
                KeyCode::Left => cursor = cursor.saturating_sub(1),
                KeyCode::Right if cursor < ps.mask_threshold_value_buf.len() => cursor += 1,
                _ => {}
            }
            ps.cursor = cursor;
            return;
        }

        match key.code {
            // Escape from add mode before quit
            KeyCode::Esc if self.pipeline_state.mask_ops_adding => {
                self.pipeline_state.mask_ops_adding = false;
            }

            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            KeyCode::Char(c @ '1'..='6') => {
                self.active_tab = (c as usize) - ('1' as usize);
                self.active_field = 0;
            }
            KeyCode::Tab => {
                self.active_tab = (self.active_tab + 1) % TAB_NAMES.len();
                self.active_field = 0;
            }
            KeyCode::BackTab => {
                self.active_tab = (self.active_tab + TAB_NAMES.len() - 1) % TAB_NAMES.len();
                self.active_field = 0;
            }

            // Reorder mask ops with Ctrl+Up/Down (must be before regular Up/Down)
            KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                let ps = &mut self.pipeline_state;
                let rows = ps.visible_rows();
                let focusable = ps.focusable_rows();
                let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);
                if let Some(PipelineRow::MaskOpEntry { section, index }) = rows.get(focus_idx) {
                    let (si, oi) = (*section, *index);
                    if oi > 0 && si < ps.mask_sections.len() && oi < ps.mask_sections[si].refinements.len() {
                        ps.mask_sections[si].refinements.swap(oi, oi - 1);
                        ps.mark_mask_custom();
                        if ps.focus > 0 { ps.focus -= 1; }
                    }
                }
            }
            KeyCode::Down if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                let ps = &mut self.pipeline_state;
                let rows = ps.visible_rows();
                let focusable = ps.focusable_rows();
                let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);
                if let Some(PipelineRow::MaskOpEntry { section, index }) = rows.get(focus_idx) {
                    let (si, oi) = (*section, *index);
                    if si < ps.mask_sections.len() && oi + 1 < ps.mask_sections[si].refinements.len() {
                        ps.mask_sections[si].refinements.swap(oi, oi + 1);
                        ps.mark_mask_custom();
                        let max = ps.focusable_rows().len().saturating_sub(1);
                        if ps.focus < max { ps.focus += 1; }
                    }
                }
            }

            // Navigation
            KeyCode::Up | KeyCode::Char('k')
                if self.pipeline_state.focus > 0 => {
                    self.pipeline_state.focus -= 1;
                }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.pipeline_state.focusable_rows().len().saturating_sub(1);
                if self.pipeline_state.focus < max {
                    self.pipeline_state.focus += 1;
                }
            }

            // Interact
            KeyCode::Enter | KeyCode::Char(' ') => {
                // A collapsible stage header toggles open/closed instead of opening a selector.
                let section_id = {
                    let ps = &self.pipeline_state;
                    let focus_idx = ps.focusable_rows().get(ps.focus).copied().unwrap_or(0);
                    match ps.visible_rows().get(focus_idx) {
                        Some(PipelineRow::SectionHeader { id, .. }) => Some(*id),
                        _ => None,
                    }
                };
                if let Some(id) = section_id {
                    self.pipeline_state.toggle_section(id);
                    return;
                }
                // Single-choice selectors open a modal listing every option (rather than cycling).
                let modal = {
                    let ps = &self.pipeline_state;
                    let rows = ps.visible_rows();
                    let focus_idx = ps.focusable_rows().get(ps.focus).copied().unwrap_or(0);
                    match rows.get(focus_idx) {
                        Some(PipelineRow::AlgoSelect { label, field, options, help }) => Some(AlgoModal {
                            target: AlgoModalTarget::PipelineSelect(field.to_string()),
                            title: label.to_string(),
                            options: options.iter().map(|s| s.to_string()).collect(),
                            help: help.iter().map(|s| s.to_string()).collect(),
                            cursor: ps.get_select(field),
                        }),
                        Some(PipelineRow::MaskOpInput { section }) => Some(AlgoModal {
                            target: AlgoModalTarget::MaskInput(*section),
                            title: "Masking Input".to_string(),
                            options: PipelineFormState::MASK_INPUT_SOURCES.iter().map(|s| format!("{}", s)).collect(),
                            help: vec![
                                "First-echo magnitude".to_string(),
                                "Magnitude (single- or combined-echo)".to_string(),
                                "Last-echo magnitude".to_string(),
                                "Phase-quality map".to_string(),
                            ],
                            cursor: ps.mask_input_index(*section),
                        }),
                        Some(PipelineRow::MaskOpGenerator { section }) => {
                            let is_threshold = matches!(
                                ps.mask_sections.get(*section).map(|s| &s.generator),
                                Some(crate::pipeline::config::MaskOp::Threshold { .. })
                            );
                            Some(AlgoModal {
                                target: AlgoModalTarget::MaskGenerator(*section),
                                title: "Mask Algorithm".to_string(),
                                options: vec!["threshold".to_string(), "bet".to_string()],
                                help: vec![
                                    "Intensity threshold-based masking".to_string(),
                                    "FSL BET brain extraction".to_string(),
                                ],
                                cursor: if is_threshold { 0 } else { 1 },
                            })
                        }
                        // Threshold method is a discrete select → modal. (BET frac. intensity is a
                        // numeric slider, so it keeps left/right and opens no modal.)
                        Some(PipelineRow::MaskOpGeneratorParam { section })
                            if matches!(
                                ps.mask_sections.get(*section).map(|s| &s.generator),
                                Some(crate::pipeline::config::MaskOp::Threshold { .. })
                            ) =>
                        {
                            Some(AlgoModal {
                                target: AlgoModalTarget::MaskThresholdMethod(*section),
                                title: "Threshold Method".to_string(),
                                options: vec!["otsu".to_string(), "fixed".to_string(), "percentile".to_string()],
                                help: vec![
                                    "Otsu automatic threshold".to_string(),
                                    "Fixed intensity threshold".to_string(),
                                    "Percentile-based threshold".to_string(),
                                ],
                                cursor: ps.mask_threshold_method_index(*section),
                            })
                        }
                        _ => None,
                    }
                };
                if let Some(m) = modal {
                    self.algo_modal = Some(m);
                    return;
                }
                let ps = &mut self.pipeline_state;
                let focused_field = ps.focused_field_name();
                let rows = ps.visible_rows();
                let focusable = ps.focusable_rows();
                let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);
                match rows.get(focus_idx).cloned() {
                    Some(PipelineRow::AlgoSelect { field, options, .. }) => {
                        // (Unreached — handled by the modal above; kept for exhaustiveness.)
                        let cur = ps.get_select(field);
                        ps.set_select(field, (cur + 1) % options.len());
                        ps.restore_focus(&focused_field);
                    }
                    Some(PipelineRow::Param { field, .. }) => {
                        ps.editing = true;
                        ps.cursor = ps.get_param(field).len();
                    }
                    Some(PipelineRow::Toggle { field, .. }) => {
                        let focused_field = ps.focused_field_name();
                        ps.toggle(field);
                        ps.restore_focus(&focused_field);
                    }
                    Some(PipelineRow::MaskOpAddStep { section }) => {
                        if ps.mask_ops_adding {
                            let available = ps.available_op_types(section);
                            if let Some(&type_name) = available.get(ps.mask_ops_add_idx) {
                                if let Some(op) = PipelineFormState::default_mask_op(type_name) {
                                    if section < ps.mask_sections.len() {
                                        ps.mask_sections[section].refinements.push(op);
                                    }
                                }
                            }
                            ps.mask_ops_adding = false;
                            ps.mark_mask_custom();
                        } else {
                            ps.mask_ops_adding = true;
                            ps.mask_ops_add_idx = 0;
                            ps.mask_ops_add_section = section;
                        }
                    }
                    Some(PipelineRow::MaskOpAddSection) => {
                        use crate::pipeline::config::*;
                        ps.mask_sections.push(MaskSection {
                            input: MaskingInput::Magnitude,
                            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                            refinements: vec![],
                        });
                        ps.mark_mask_custom();
                    }
                    Some(PipelineRow::MaskOpThresholdValue { section }) => {
                        // Start editing threshold value
                        let current = if let crate::pipeline::config::MaskOp::Threshold { value, .. } = &ps.mask_sections[section].generator {
                            value.map(|v| format!("{}", v)).unwrap_or_default()
                        } else { String::new() };
                        ps.mask_threshold_value_buf = current.clone();
                        ps.mask_threshold_editing = true;
                        ps.cursor = current.len();
                    }
                    Some(PipelineRow::MaskOpEntry { .. }) | Some(PipelineRow::MaskOpInput { .. })
                    | Some(PipelineRow::MaskOpGenerator { .. }) | Some(PipelineRow::MaskOpGeneratorParam { .. }) => {
                        // Handled by Left/Right
                    }
                    _ => {}
                }
            }

            // Delete refinement step or entire section
            KeyCode::Char('d') | KeyCode::Delete => {
                let ps = &mut self.pipeline_state;
                let rows = ps.visible_rows();
                let focusable = ps.focusable_rows();
                let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);
                match rows.get(focus_idx) {
                    Some(PipelineRow::MaskOpEntry { section, index }) => {
                        let (si, oi) = (*section, *index);
                        if si < ps.mask_sections.len() && oi < ps.mask_sections[si].refinements.len() {
                            ps.mask_sections[si].refinements.remove(oi);
                            ps.mark_mask_custom();
                            let max = ps.focusable_rows().len().saturating_sub(1);
                            if ps.focus > max { ps.focus = max; }
                        }
                    }
                    // Delete entire section (only if >1 sections) when focused on section header-adjacent rows
                    Some(PipelineRow::MaskOpGenerator { section }) | Some(PipelineRow::MaskOpInput { section }) => {
                        let si = *section;
                        if ps.mask_sections.len() > 1 && si < ps.mask_sections.len() {
                            ps.mask_sections.remove(si);
                            ps.mark_mask_custom();
                            let max = ps.focusable_rows().len().saturating_sub(1);
                            if ps.focus > max { ps.focus = max; }
                        }
                    }
                    _ => {}
                }
            }

            // Left/Right for selects and mask ops
            KeyCode::Left | KeyCode::Right => {
                let delta = if key.code == KeyCode::Left { -1isize } else { 1 };
                let ps = &mut self.pipeline_state;

                // Check if we're in mask ops add mode
                if ps.mask_ops_adding {
                    let available = ps.available_op_types(ps.mask_ops_add_section);
                    let n = available.len() as isize;
                    if n > 0 {
                        ps.mask_ops_add_idx = (ps.mask_ops_add_idx as isize + delta).rem_euclid(n) as usize;
                    }
                } else {
                    let focused_field = ps.focused_field_name();
                    let rows = ps.visible_rows();
                    let focusable = ps.focusable_rows();
                    let focus_idx = focusable.get(ps.focus).copied().unwrap_or(0);
                    match rows.get(focus_idx) {
                        Some(PipelineRow::AlgoSelect { field, options, .. }) => {
                            // Mask preset: only cycle between 0 (robust) and 1 (bet); "custom" is auto-set
                            let n = if *field == "mask_preset" { 2 } else { options.len() } as isize;
                            let cur = ps.get_select(field).min((n - 1) as usize) as isize;
                            let new_val = (cur + delta).rem_euclid(n) as usize;
                            ps.set_select(field, new_val);
                            ps.restore_focus(&focused_field);
                        }
                        Some(PipelineRow::MaskOpEntry { section, index }) => {
                            ps.adjust_mask_op(*section, *index, delta);
                        }
                        Some(PipelineRow::MaskOpGenerator { section }) => {
                            ps.adjust_mask_generator(*section, delta);
                        }
                        Some(PipelineRow::MaskOpGeneratorParam { section }) => {
                            ps.adjust_mask_generator_param(*section, delta);
                        }
                        Some(PipelineRow::MaskOpInput { section }) => {
                            ps.adjust_mask_input(*section, delta);
                        }
                        // Stage headers: ← collapses, → expands (in addition to Enter/Space toggle).
                        Some(PipelineRow::SectionHeader { id, .. }) => {
                            let id = *id;
                            if delta < 0 {
                                ps.collapsed_sections.insert(id);
                            } else {
                                ps.collapsed_sections.remove(id);
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Reset focused field to default
            KeyCode::Char('r') => self.reset_pipeline_field(),
            // Reset all pipeline settings to defaults
            KeyCode::Char('R') => {
                self.pipeline_state = PipelineFormState::default();
            }

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    fn handle_editing_key(&mut self, key: KeyEvent) {
        let mut cursor = self.cursor_pos;

        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.editing = false;
                return;
            }
            KeyCode::Char(c) => {
                self.text_value_mut().insert(cursor, c);
                cursor += 1;
            }
            KeyCode::Backspace
                if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                    let new_pos = word_boundary_left(self.text_value(), cursor);
                    self.text_value_mut().drain(new_pos..cursor);
                    cursor = new_pos;
                }
            KeyCode::Backspace
                if cursor > 0 => {
                    cursor -= 1;
                    self.text_value_mut().remove(cursor);
                }
            KeyCode::Delete => {
                let len = self.text_value().len();
                if cursor < len {
                    self.text_value_mut().remove(cursor);
                }
            }
            KeyCode::Left => {
                cursor = cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                let len = self.text_value().len();
                if cursor < len {
                    cursor += 1;
                }
            }
            KeyCode::Home => cursor = 0,
            KeyCode::End => cursor = self.text_value().len(),
            _ => {}
        }

        self.cursor_pos = cursor;
    }

    fn interact_field(&mut self) {
        match self.current_field().kind.clone() {
            FieldKind::Text => {
                self.editing = true;
                self.cursor_pos = self.text_value().len();
            }
            FieldKind::Checkbox => self.toggle_checkbox(),
            FieldKind::Select { options } => {
                // Open a modal listing all options rather than cycling by one.
                let field = self.current_field();
                let (title, help) = (field.label.to_string(), field.help.to_string());
                let opts: Vec<String> = options.iter().map(|s| s.to_string()).collect();
                self.algo_modal = Some(AlgoModal {
                    target: AlgoModalTarget::TabSelect { tab: self.active_tab, field: self.active_field },
                    title,
                    help: vec![help; opts.len()],
                    options: opts,
                    cursor: self.select_value(),
                });
            }
        }
    }

    fn adjust_select(&mut self, delta: isize) {
        if let FieldKind::Select { options } = &self.current_field().kind {
            let n = options.len() as isize;
            let val = self.select_value() as isize;
            let new_val = (val + delta).rem_euclid(n) as usize;
            self.set_select_value(new_val);
        }
    }

    // --- Field value accessors ---

    pub fn text_value(&self) -> &str {
        match (self.active_tab, self.active_field) {
            (0, 1) => match self.input_mode {
                InputMode::Bids => &self.form.bids_dir,
                InputMode::NIfTI => &self.nifti_state.input_dir,
                InputMode::DicomToBids => &self.dicom_state.dicom_dir,
            },
            (0, 2) => match self.input_mode {
                InputMode::Bids => &self.form.output_dir,
                InputMode::NIfTI => &self.nifti_state.output_dir,
                InputMode::DicomToBids => &self.dicom_state.output_dir,
            },
            (0, 3) => &self.form.config_file,
            (TAB_SUPPLEMENTARY, 2) => &self.form.swi_strength,
            (TAB_SUPPLEMENTARY, 3) => &self.form.swi_hp_sigma_x,
            (TAB_SUPPLEMENTARY, 4) => &self.form.swi_hp_sigma_y,
            (TAB_SUPPLEMENTARY, 5) => &self.form.swi_hp_sigma_z,
            (TAB_SUPPLEMENTARY, 6) => &self.form.swi_mip_window,
            (TAB_EXECUTION, 3) => &self.form.n_procs,
            (TAB_EXECUTION, 4) => &self.form.slurm_account,
            (TAB_EXECUTION, 5) => &self.form.slurm_partition,
            (TAB_EXECUTION, 6) => &self.form.slurm_time,
            (TAB_EXECUTION, 7) => &self.form.slurm_mem,
            (TAB_EXECUTION, 8) => &self.form.slurm_cpus,
            _ => "",
        }
    }

    fn text_value_mut(&mut self) -> &mut String {
        match (self.active_tab, self.active_field) {
            (0, 1) => match self.input_mode {
                InputMode::Bids => &mut self.form.bids_dir,
                InputMode::NIfTI => &mut self.nifti_state.input_dir,
                InputMode::DicomToBids => &mut self.dicom_state.dicom_dir,
            },
            (0, 2) => match self.input_mode {
                InputMode::Bids => &mut self.form.output_dir,
                InputMode::NIfTI => &mut self.nifti_state.output_dir,
                InputMode::DicomToBids => &mut self.dicom_state.output_dir,
            },
            (0, 3) => &mut self.form.config_file,
            (TAB_SUPPLEMENTARY, 2) => &mut self.form.swi_strength,
            (TAB_SUPPLEMENTARY, 3) => &mut self.form.swi_hp_sigma_x,
            (TAB_SUPPLEMENTARY, 4) => &mut self.form.swi_hp_sigma_y,
            (TAB_SUPPLEMENTARY, 5) => &mut self.form.swi_hp_sigma_z,
            (TAB_SUPPLEMENTARY, 6) => &mut self.form.swi_mip_window,
            (TAB_EXECUTION, 3) => &mut self.form.n_procs,
            (TAB_EXECUTION, 4) => &mut self.form.slurm_account,
            (TAB_EXECUTION, 5) => &mut self.form.slurm_partition,
            (TAB_EXECUTION, 6) => &mut self.form.slurm_time,
            (TAB_EXECUTION, 7) => &mut self.form.slurm_mem,
            (TAB_EXECUTION, 8) => &mut self.form.slurm_cpus,
            _ => unreachable!("text_value_mut called on non-text field"),
        }
    }

    pub fn select_value(&self) -> usize {
        match (self.active_tab, self.active_field) {
            (TAB_SUPPLEMENTARY, 1) => self.form.swi_scaling,
            (TAB_EXECUTION, 0) => self.form.execution_mode,
            _ => 0,
        }
    }

    fn set_select_value(&mut self, val: usize) {
        match (self.active_tab, self.active_field) {
            (TAB_SUPPLEMENTARY, 1) => self.form.swi_scaling = val,
            (TAB_EXECUTION, 0) => {
                self.form.execution_mode = val;
                // Clamp active_field if it landed on a now-hidden field
                if !self.is_field_visible(self.active_tab, self.active_field) {
                    self.active_field = 0;
                }
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    fn checkbox_value(&self) -> bool {
        match (self.active_tab, self.active_field) {
            (TAB_SUPPLEMENTARY, 0) => self.form.do_swi,
            (TAB_SUPPLEMENTARY, 7) => self.form.do_t2starmap,
            (TAB_SUPPLEMENTARY, 8) => self.form.do_r2starmap,
            (TAB_SUPPLEMENTARY, 9) => self.form.do_r2map || self.r2_map_forced(),
            (TAB_SUPPLEMENTARY, 10) => self.form.do_r2primemap || self.r2prime_map_forced(),
            (TAB_SUPPLEMENTARY, 11) => self.form.export_dicom,
            (TAB_EXECUTION, 1) => self.form.dry_run,
            (TAB_EXECUTION, 2) => self.form.debug,
            (TAB_EXECUTION, 9) => self.form.slurm_submit,
            _ => false,
        }
    }

    /// Whether the selected chi-separation method needs R2' (the R2'-based methods:
    /// chi-sep-ilsqr/medi, wavesep, hc-chisep — indices ≥ 2 in SEP_ALGO_OPTIONS).
    fn separation_needs_r2prime(&self) -> bool {
        self.pipeline_state.do_chi_separation && self.pipeline_state.separation_algorithm >= 2
    }
    /// R2' is forced when the method needs it and no custom R2' map is supplied.
    fn r2prime_map_forced(&self) -> bool {
        self.separation_needs_r2prime() && self.pipeline_state.custom_r2prime_tool.trim().is_empty()
    }
    /// R2 is forced only when needed to compute a forced R2' (and no custom R2 map).
    fn r2_map_forced(&self) -> bool {
        self.r2prime_map_forced() && self.pipeline_state.custom_r2_tool.trim().is_empty()
    }

    fn toggle_checkbox(&mut self) {
        match (self.active_tab, self.active_field) {
            (TAB_SUPPLEMENTARY, 0) => self.form.do_swi = !self.form.do_swi,
            (TAB_SUPPLEMENTARY, 7) => self.form.do_t2starmap = !self.form.do_t2starmap,
            (TAB_SUPPLEMENTARY, 8) => self.form.do_r2starmap = !self.form.do_r2starmap,
            (TAB_SUPPLEMENTARY, 9) => {
                if self.r2_map_forced() {
                    self.error_message = Some("R2 map is required by the selected chi-separation method — change method, disable chi-separation, or set a custom R2 tool first.".to_string());
                } else {
                    self.form.do_r2map = !self.form.do_r2map;
                }
            }
            (TAB_SUPPLEMENTARY, 10) => {
                if self.r2prime_map_forced() {
                    self.error_message = Some("R2' map is required by the selected chi-separation method — change method, disable chi-separation, or set a custom R2' tool first.".to_string());
                } else {
                    self.form.do_r2primemap = !self.form.do_r2primemap;
                }
            }
            (TAB_SUPPLEMENTARY, 11) => self.form.export_dicom = !self.form.export_dicom,
            (TAB_EXECUTION, 1) => self.form.dry_run = !self.form.dry_run,
            (TAB_EXECUTION, 2) => self.form.debug = !self.form.debug,
            (TAB_EXECUTION, 9) => self.form.slurm_submit = !self.form.slurm_submit,
            _ => {}
        }
    }

    // Generalized accessors for rendering arbitrary (tab, field) pairs
    /// Whether a field is visible (used to hide SLURM fields in Local mode and vice versa).
    pub fn is_field_visible(&self, tab: usize, field: usize) -> bool {
        match (tab, field) {
            // SWI settings (1-6) only visible when Compute SWI is checked
            (TAB_SUPPLEMENTARY, 1..=6) => self.form.do_swi,
            // SLURM fields (4-9) only visible in SLURM mode
            (TAB_EXECUTION, 4..=9) => self.form.execution_mode == 1,
            // Dry Run and Num Processes only in Local mode
            (TAB_EXECUTION, 1) | (TAB_EXECUTION, 3) => self.form.execution_mode == 0,
            _ => true,
        }
    }

    pub fn get_text_value(&self, tab: usize, field: usize) -> &str {
        match (tab, field) {
            (TAB_SUPPLEMENTARY, 2) => &self.form.swi_strength,
            (TAB_SUPPLEMENTARY, 3) => &self.form.swi_hp_sigma_x,
            (TAB_SUPPLEMENTARY, 4) => &self.form.swi_hp_sigma_y,
            (TAB_SUPPLEMENTARY, 5) => &self.form.swi_hp_sigma_z,
            (TAB_SUPPLEMENTARY, 6) => &self.form.swi_mip_window,
            (TAB_EXECUTION, 3) => &self.form.n_procs,
            (TAB_EXECUTION, 4) => &self.form.slurm_account,
            (TAB_EXECUTION, 5) => &self.form.slurm_partition,
            (TAB_EXECUTION, 6) => &self.form.slurm_time,
            (TAB_EXECUTION, 7) => &self.form.slurm_mem,
            (TAB_EXECUTION, 8) => &self.form.slurm_cpus,
            _ => "",
        }
    }

    pub fn get_select_value(&self, tab: usize, field: usize) -> usize {
        match (tab, field) {
            (TAB_SUPPLEMENTARY, 1) => self.form.swi_scaling,
            (TAB_EXECUTION, 0) => self.form.execution_mode,
            _ => 0,
        }
    }

    pub fn get_checkbox_value(&self, tab: usize, field: usize) -> bool {
        match (tab, field) {
            (TAB_SUPPLEMENTARY, 0) => self.form.do_swi,
            (TAB_SUPPLEMENTARY, 7) => self.form.do_t2starmap,
            (TAB_SUPPLEMENTARY, 8) => self.form.do_r2starmap,
            (TAB_SUPPLEMENTARY, 9) => self.form.do_r2map || self.r2_map_forced(),
            (TAB_SUPPLEMENTARY, 10) => self.form.do_r2primemap || self.r2prime_map_forced(),
            (TAB_SUPPLEMENTARY, 11) => self.form.export_dicom,
            (TAB_EXECUTION, 1) => self.form.dry_run,
            (TAB_EXECUTION, 2) => self.form.debug,
            (TAB_EXECUTION, 9) => self.form.slurm_submit,
            _ => false,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_algo_modal_opens_on_enter_over_algoselect() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.pipeline_state.mode = PipelineTabMode::Qsm;
        // The QSM tab's first focusable row is the "Compute QSM" toggle; move to the inversion
        // AlgoSelect and open it.
        let opened = (0..40).any(|_| {
            app.handle_key(key(KeyCode::Enter)); // opens modal iff focused row is an AlgoSelect
            if app.algo_modal.is_some() { return true; }
            app.handle_key(key(KeyCode::Down));
            false
        });
        assert!(opened, "expected to open an AlgoSelect modal while navigating the QSM tab");
        assert!(!app.algo_modal.as_ref().unwrap().options.is_empty());
    }

    #[test]
    fn test_algo_modal_selects_option() {
        let mut app = App::new();
        app.algo_modal = Some(AlgoModal {
            target: AlgoModalTarget::PipelineSelect("qsm_algorithm".to_string()),
            title: "QSM Inversion".to_string(),
            options: QSM_ALGO_OPTIONS.iter().map(|s| s.to_string()).collect(),
            help: vec![String::new(); QSM_ALGO_OPTIONS.len()],
            cursor: 0,
        });
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.algo_modal.as_ref().unwrap().cursor, 2);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.pipeline_state.get_select("qsm_algorithm"), 2);
    }

    #[test]
    fn test_algo_modal_cancel_leaves_value() {
        let mut app = App::new();
        let before = app.pipeline_state.get_select("qsm_algorithm");
        app.algo_modal = Some(AlgoModal {
            target: AlgoModalTarget::PipelineSelect("qsm_algorithm".to_string()),
            title: "QSM Inversion".to_string(),
            options: vec!["a".to_string(), "b".to_string()],
            help: vec![String::new(), String::new()],
            cursor: 1,
        });
        app.handle_key(key(KeyCode::Esc));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.pipeline_state.get_select("qsm_algorithm"), before);
    }

    #[test]
    fn test_mask_input_modal_opens_and_sets() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.sync_pipeline_mode();
        // Focus the mask Input row.
        let rows = app.pipeline_state.visible_rows();
        let target = app.pipeline_state.focusable_rows().iter().position(|&ri| {
            matches!(rows.get(ri), Some(PipelineRow::MaskOpInput { .. }))
        }).expect("MaskOpInput row not focusable");
        app.pipeline_state.focus = target;
        app.handle_key(key(KeyCode::Enter));
        let modal = app.algo_modal.as_ref().expect("modal should open over mask Input");
        assert!(matches!(modal.target, AlgoModalTarget::MaskInput(_)));
        assert_eq!(modal.options.len(), 4);
        // Pick the last option (phase-quality) and confirm.
        while app.algo_modal.as_ref().unwrap().cursor < 3 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(
            app.pipeline_state.mask_sections[0].input,
            crate::pipeline::config::MaskingInput::PhaseQuality
        );
    }

    #[test]
    fn test_input_mode_modal_opens_and_sets() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 0;
        app.handle_key(key(KeyCode::Enter));
        let modal = app.algo_modal.as_ref().expect("modal should open over Input Mode");
        assert!(matches!(modal.target, AlgoModalTarget::InputMode));
        assert_eq!(modal.options.len(), 3);
        app.handle_key(key(KeyCode::Down)); // NIfTI
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.input_mode, InputMode::NIfTI);
    }

    #[test]
    fn test_execution_mode_modal_opens_and_sets() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 0; // Execution Mode select
        app.handle_key(key(KeyCode::Enter));
        let modal = app.algo_modal.as_ref().expect("modal should open over Execution Mode");
        assert!(matches!(modal.target, AlgoModalTarget::TabSelect { .. }));
        app.handle_key(key(KeyCode::Down)); // SLURM
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.form.execution_mode, 1);
    }

    // --- App::new ---



    // --- Navigation ---

    #[test]
    fn test_quit_on_q() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_on_esc() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn test_tab_switching_numbers() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, 2);
        app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(app.active_tab, 0);
        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.active_tab, 3);
    }



    #[test]
    fn test_tab_switch_resets_field() {
        let mut app = App::new();
        app.active_field = 2;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_field, 0);
    }

    #[test]
    fn test_field_navigation_down() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.active_field, 1);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.active_field, 2);
    }

    #[test]
    fn test_field_navigation_up() {
        let mut app = App::new();
        app.active_field = 2;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.active_field, 1);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.active_field, 0);
    }


    // --- Text editing ---

    #[test]
    fn test_enter_editing_text_field() {
        let mut app = App::new();
        // Tab 0, field 1 is BIDS Directory (Text) — field 0 is mode selector
        app.active_field = 1;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.editing);
    }

    #[test]
    fn test_type_characters() {
        let mut app = App::new();
        app.active_field = 1; // BIDS Directory
        app.handle_key(key(KeyCode::Enter)); // enter editing
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('a')));
        assert_eq!(app.form.bids_dir, "/data");
        assert_eq!(app.cursor_pos, 5);
    }

    #[test]
    fn test_backspace() {
        let mut app = App::new();
        app.active_field = 1;
        app.form.bids_dir = "abc".to_string();
        app.handle_key(key(KeyCode::Enter)); // enter editing, cursor at end (3)
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.form.bids_dir, "ab");
        assert_eq!(app.cursor_pos, 2);
    }

    #[test]
    fn test_backspace_at_start_does_nothing() {
        let mut app = App::new();
        app.form.bids_dir = "x".to_string();
        app.editing = true;
        app.cursor_pos = 0;
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.form.bids_dir, "x");
    }

    #[test]
    fn test_delete_key() {
        let mut app = App::new();
        app.active_field = 1;
        app.form.bids_dir = "abc".to_string();
        app.editing = true;
        app.cursor_pos = 0;
        app.handle_key(key(KeyCode::Delete));
        assert_eq!(app.form.bids_dir, "bc");
    }

    #[test]
    fn test_cursor_left_right() {
        let mut app = App::new();
        app.active_field = 1;
        app.form.bids_dir = "abc".to_string();
        app.editing = true;
        app.cursor_pos = 2;
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.cursor_pos, 1);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.cursor_pos, 2);
    }

    #[test]
    fn test_cursor_left_at_zero() {
        let mut app = App::new();
        app.editing = true;
        app.cursor_pos = 0;
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn test_home_end_keys() {
        let mut app = App::new();
        app.active_field = 1;
        app.form.bids_dir = "abcdef".to_string();
        app.editing = true;
        app.cursor_pos = 3;
        app.handle_key(key(KeyCode::Home));
        assert_eq!(app.cursor_pos, 0);
        app.handle_key(key(KeyCode::End));
        assert_eq!(app.cursor_pos, 6);
    }

    #[test]
    fn test_esc_exits_editing() {
        let mut app = App::new();
        app.active_field = 1;
        app.editing = true;
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.editing);
        // Should NOT trigger quit while editing
        assert!(!app.should_quit);
    }

    #[test]
    fn test_enter_exits_editing() {
        let mut app = App::new();
        app.active_field = 1;
        app.editing = true;
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.editing);
    }

    // --- Checkbox fields ---



    #[test]
    fn test_pipeline_phase_offset_removal() {
        let mut app = App::new();
        assert!(app.pipeline_state.phase_offset_removal);
        app.pipeline_state.toggle("phase_offset_removal");
        assert!(!app.pipeline_state.phase_offset_removal);
    }

    // --- F5 triggers run ---

    #[test]
    fn test_f5_requires_bids_dir() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::F(5)));
        assert!(!app.should_run);
        assert!(app.error_message.is_some());
    }

    #[test]
    fn test_f5_triggers_run_with_bids() {
        let mut app = App::new();
        app.form.bids_dir = "/tmp/bids".to_string();
        app.handle_key(key(KeyCode::F(5)));
        assert!(app.should_run);
        assert!(app.error_message.is_none());
    }

    #[test]
    fn test_f5_requires_slurm_account() {
        let mut app = App::new();
        app.form.bids_dir = "/tmp/bids".to_string();
        app.form.execution_mode = 1; // SLURM
        app.handle_key(key(KeyCode::F(5)));
        assert!(!app.should_run);
        assert!(app.error_message.as_deref() == Some("SLURM Account is required"));
        assert_eq!(app.active_tab, 3); // navigated to Execution tab
    }

    // --- Pipeline state selects ---

    #[test]
    fn test_pipeline_algorithm_selects() {
        let mut app = App::new();
        let ps = &mut app.pipeline_state;
        assert_eq!(ps.qsm_algorithm, 0); // rts
        ps.set_select("qsm_algorithm", 1);
        assert_eq!(ps.qsm_algorithm, 1); // tv
        ps.set_select("qsm_algorithm", 3);
        assert_eq!(ps.qsm_algorithm, 3); // tgv
    }

    // --- Text value accessors for different tabs ---

    #[test]
    fn test_filter_tab_routes_to_filter_handler() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Should not crash — filter handler takes over
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('n')));
    }

    #[test]
    fn test_pipeline_get_param() {
        let mut app = App::new();
        app.pipeline_state.rts_delta = "0.2".to_string();
        assert_eq!(app.pipeline_state.get_param("rts_delta"), "0.2");
    }


    #[test]
    fn test_text_value_unknown_returns_empty() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY; // Algorithms tab - no text fields
        app.active_field = 0;
        assert_eq!(app.text_value(), "");
    }

    // --- get_ accessors (used by UI rendering) ---

    #[test]
    fn test_get_text_value_all_fields() {
        let app = App::new();
        // Should not panic for any valid (tab, field) combo
        for tab in 0..5 {
            for field in 0..12 {
                let _ = app.get_text_value(tab, field);
            }
        }
    }

    #[test]
    fn test_get_select_value_defaults() {
        let app = App::new();
        assert_eq!(app.get_select_value(99, 99), 0); // unknown returns 0
        // Algorithm selects are now in pipeline_state, not tab-indexed
        assert_eq!(app.pipeline_state.get_select("qsm_algorithm"), 0);
    }

    #[test]
    fn test_get_checkbox_value_defaults() {
        let app = App::new();
        assert!(!app.get_checkbox_value(4, 0)); // do_swi
        assert!(!app.get_checkbox_value(4, 4)); // dry_run
        assert!(!app.get_checkbox_value(99, 99)); // unknown returns false
    }

    // --- checkbox_value (private, exercised for coverage) ---


    // --- RunForm default ---

    #[test]
    fn test_run_form_default() {
        let form = RunForm::default();
        assert!(form.bids_dir.is_empty());
        assert!(form.output_dir.is_empty());
        assert!(!form.do_swi);
    }


    // --- Editing output_dir ---

    #[test]
    fn test_edit_output_dir() {
        let mut app = App::new();
        app.active_field = 2; // output_dir (field 0=mode, 1=bids_dir, 2=output_dir)
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('o')));
        app.handle_key(key(KeyCode::Char('u')));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.form.output_dir, "/out");
    }

    // --- Editing config_file ---

    #[test]
    fn test_edit_config_file() {
        let mut app = App::new();
        app.active_field = 3; // config_file (field 0=mode, 1=bids_dir, 2=output_dir, 3=config)
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('c')));
        assert_eq!(app.form.config_file, "c");
    }

    // --- Editing all parameter text fields ---

    #[test]
    fn test_pipeline_param_mutation() {
        let mut ps = super::PipelineFormState::default();
        assert!(!ps.rts_delta.is_empty()); // has QSM.rs default

        // Test get_param_mut
        if let Some(s) = ps.get_param_mut("rts_delta") {
            *s = "0.25".to_string();
        }
        assert_eq!(ps.get_param("rts_delta"), "0.25");

        // Test select (qsm_algorithm)
        ps.set_select("qsm_algorithm", 2);
        assert_eq!(ps.get_select("qsm_algorithm"), 2);
    }

    #[test]
    fn test_pipeline_visible_rows_change_with_algorithm() {
        let mut ps = super::PipelineFormState::default();
        ps.collapsed_sections.clear(); // expand so algorithm-specific rows are visible
        let rows_rts = ps.visible_rows().len();
        ps.qsm_algorithm = 2; // TKD (fewer params)
        let rows_tkd = ps.visible_rows().len();
        assert!(rows_tkd < rows_rts, "TKD should have fewer rows than RTS");

        ps.qsm_algorithm = 3; // TGV (hides unwrapping + bgremove)
        let rows_tgv = ps.visible_rows().len();
        assert!(rows_tgv < rows_rts, "TGV should hide unwrapping/bgremove");
    }

    #[test]
    fn test_pipeline_section_collapse() {
        let mut ps = super::PipelineFormState::default();
        let has_param = |ps: &super::PipelineFormState, field: &str| {
            ps.visible_rows().iter().any(|r| matches!(r, PipelineRow::Param { field: f, .. } if *f == field))
        };
        let has_header = |ps: &super::PipelineFormState, id: &str| {
            ps.visible_rows().iter().any(|r| matches!(r, PipelineRow::SectionHeader { id: hid, .. } if *hid == id))
        };
        // Default is all-collapsed: headers present, bodies hidden.
        assert!(has_header(&ps, "inversion"));
        assert!(!has_param(&ps, "rts_delta"), "collapsed inversion should hide its params");
        // Expanding shows the body; the header stays; collapsing hides it again.
        ps.toggle_section("inversion");
        assert!(has_param(&ps, "rts_delta"), "expanded inversion should show its params");
        assert!(has_header(&ps, "inversion"), "header stays when expanded");
        ps.toggle_section("inversion");
        assert!(!has_param(&ps, "rts_delta"));
    }

    #[test]
    fn test_pipeline_section_arrow_and_enter_toggle() {
        let mut app = App::new();
        app.active_tab = TAB_QSM; // (intentionally not clearing collapsed_sections: test the default)
        // Focus the collapsed "inversion" header.
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let hi = focusable.iter()
            .position(|&ri| matches!(&rows[ri], PipelineRow::SectionHeader { id: "inversion", .. }))
            .expect("inversion header present");
        app.pipeline_state.focus = hi;
        assert!(app.pipeline_state.collapsed_sections.contains("inversion"));
        // → expands, ← collapses, Enter toggles.
        app.handle_key(key(KeyCode::Right));
        assert!(!app.pipeline_state.collapsed_sections.contains("inversion"), "→ should expand");
        app.handle_key(key(KeyCode::Left));
        assert!(app.pipeline_state.collapsed_sections.contains("inversion"), "← should collapse");
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.collapsed_sections.contains("inversion"), "Enter should toggle open");
    }

    // --- Filter tree tests ---

    #[test]
    fn test_filter_state_default() {
        let fs = super::FilterTreeState::default();
        assert!(fs.tree.is_none());
        assert_eq!(fs.include_pattern, "*");
        assert!(fs.exclude_pattern.is_empty());
        assert_eq!(fs.focus, super::FilterFocus::Include);
    }

    #[test]
    fn test_filter_scan_with_bids() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_multi_echo_bids(dir.path());
        let mut fs = super::FilterTreeState::default();
        fs.maybe_rescan(dir.path().to_str().unwrap());
        assert!(fs.tree.is_some());
        let tree = fs.tree.as_ref().unwrap();
        assert_eq!(tree.subjects.len(), 1);
        assert_eq!(tree.subjects[0].name, "1");
    }

    #[test]
    fn test_filter_scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut fs = super::FilterTreeState::default();
        fs.maybe_rescan(dir.path().to_str().unwrap());
        // Tree may be Some with empty subjects, or None
        if let Some(ref tree) = fs.tree {
            assert!(tree.subjects.is_empty());
        }
    }

    #[test]
    fn test_filter_scan_caches() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut fs = super::FilterTreeState::default();
        let path = dir.path().to_str().unwrap();
        fs.maybe_rescan(path);
        assert!(fs.tree.is_some());
        // Second call should not rescan
        fs.maybe_rescan(path);
        assert_eq!(fs.scanned_bids_dir.as_deref(), Some(path));
    }

    #[test]
    fn test_filter_navigation() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut fs = super::FilterTreeState::default();
        fs.maybe_rescan(dir.path().to_str().unwrap());

        assert_eq!(fs.focus, super::FilterFocus::Include);
        fs.focus_next(); // -> Exclude
        assert_eq!(fs.focus, super::FilterFocus::Exclude);
        fs.focus_next(); // -> tree node 0 (subject)
        assert!(matches!(fs.focus, super::FilterFocus::TreeNode(0)));
        fs.focus_next(); // -> tree node 1 (run)
        fs.focus_next(); // -> NumEchoes
        assert_eq!(fs.focus, super::FilterFocus::NumEchoes);
        fs.focus_next(); // stays at NumEchoes
        assert_eq!(fs.focus, super::FilterFocus::NumEchoes);
        fs.focus_prev(); // back up
        assert!(matches!(fs.focus, super::FilterFocus::TreeNode(_)));
    }

    #[test]
    fn test_filter_toggle_run() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut fs = super::FilterTreeState::default();
        fs.maybe_rescan(dir.path().to_str().unwrap());

        // Navigate to the run leaf
        fs.focus_next(); // subject
        fs.focus_next(); // run leaf
        let tree = fs.tree.as_ref().unwrap();
        assert!(tree.subjects[0].runs[0].selected);
        fs.toggle_focused();
        let tree = fs.tree.as_ref().unwrap();
        assert!(!tree.subjects[0].runs[0].selected);
    }

    #[test]
    fn test_filter_select_all_none() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_multi_echo_bids(dir.path());
        let mut fs = super::FilterTreeState::default();
        fs.maybe_rescan(dir.path().to_str().unwrap());

        let tree = fs.tree.as_mut().unwrap();
        tree.set_all(false);
        assert_eq!(tree.selected_runs(), 0);
        tree.set_all(true);
        assert_eq!(tree.selected_runs(), tree.total_runs());
    }



    #[test]
    fn test_filter_collapse() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut fs = super::FilterTreeState::default();
        fs.maybe_rescan(dir.path().to_str().unwrap());

        let rows_before = fs.visible_rows().len();
        fs.focus = super::FilterFocus::TreeNode(0); // subject node
        fs.toggle_collapse();
        let rows_after = fs.visible_rows().len();
        assert!(rows_after < rows_before, "Collapsing should hide children");

        // Expand again
        fs.toggle_collapse();
        assert_eq!(fs.visible_rows().len(), rows_before);
    }

    // --- Right arrow on non-select does nothing ---

    #[test]
    fn test_left_right_on_text_field_does_nothing() {
        let mut app = App::new();
        app.active_field = 0; // Text field
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Right));
        // No crash, no state change
        assert_eq!(app.active_field, 0);
    }

    #[test]
    fn test_all_param_fields_are_valid() {
        // Opt-in fields that intentionally default to empty (blank = feature off).
        const OPT_IN_EMPTY: &[&str] = &["dl_tile_size", "dl_tile_halo"];
        let state = PipelineFormState::default();
        for &field_name in PipelineFormState::ALL_PARAM_FIELDS {
            let val = state.get_param(field_name);
            if OPT_IN_EMPTY.contains(&field_name) {
                continue;
            }
            assert_ne!(val, "", "get_param returned empty for field: {}", field_name);
        }
    }

    #[test]
    fn test_all_param_fields_are_mutable() {
        let mut state = PipelineFormState::default();
        for &field_name in PipelineFormState::ALL_PARAM_FIELDS {
            assert!(
                state.get_param_mut(field_name).is_some(),
                "get_param_mut returned None for field: {}", field_name,
            );
        }
    }

    #[test]
    fn test_algo_option_help_arrays_aligned() {
        // Every selector's help array must have one entry per option, in order — otherwise the
        // modal shows the wrong (or an empty) tooltip. Guards against silent drift.
        use super::*;
        for (label, opts, help) in [
            ("qsm", QSM_ALGO_OPTIONS.len(), QSM_ALGO_HELP.len()),
            ("separation", SEP_ALGO_OPTIONS.len(), SEP_ALGO_HELP.len()),
            ("qsmart-inner", QSMART_INV_OPTIONS.len(), QSMART_INV_HELP.len()),
            ("unwrap", UNWRAP_OPTIONS.len(), UNWRAP_HELP.len()),
            ("bg-removal", BF_OPTIONS.len(), BF_HELP.len()),
            ("qsm-reference", QSM_REF_OPTIONS.len(), QSM_REF_HELP.len()),
            ("mask-preset", MASK_PRESET_OPTIONS.len(), MASK_PRESET_HELP.len()),
        ] {
            assert_eq!(opts, help, "{label}: {opts} options but {help} help entries");
        }
    }

    // --- SLURM field visibility ---

    #[test]
    fn test_field_visibility_local_mode() {
        let mut app = App::new();
        app.form.execution_mode = 0; // Local
        // Local-only fields visible
        assert!(app.is_field_visible(TAB_EXECUTION, 0)); // Execution Mode
        assert!(app.is_field_visible(TAB_EXECUTION, 1)); // Dry Run
        assert!(app.is_field_visible(TAB_EXECUTION, 2)); // Debug
        assert!(app.is_field_visible(TAB_EXECUTION, 3)); // Num Processes
        // SLURM fields hidden
        assert!(!app.is_field_visible(TAB_EXECUTION, 4)); // SLURM Account
        assert!(!app.is_field_visible(TAB_EXECUTION, 5)); // SLURM Partition
        assert!(!app.is_field_visible(TAB_EXECUTION, 6)); // SLURM Time
        assert!(!app.is_field_visible(TAB_EXECUTION, 7)); // SLURM Mem
        assert!(!app.is_field_visible(TAB_EXECUTION, 8)); // SLURM CPUs
        assert!(!app.is_field_visible(TAB_EXECUTION, 9)); // Auto-Submit
    }

    #[test]
    fn test_field_visibility_slurm_mode() {
        let mut app = App::new();
        app.form.execution_mode = 1; // SLURM
        // Execution Mode always visible
        assert!(app.is_field_visible(TAB_EXECUTION, 0));
        // Dry Run and Num Processes hidden in SLURM mode
        assert!(!app.is_field_visible(TAB_EXECUTION, 1)); // Dry Run
        assert!(!app.is_field_visible(TAB_EXECUTION, 3)); // Num Processes
        // Debug visible in both modes
        assert!(app.is_field_visible(TAB_EXECUTION, 2));
        // SLURM fields visible
        assert!(app.is_field_visible(TAB_EXECUTION, 4));
        assert!(app.is_field_visible(TAB_EXECUTION, 5));
        assert!(app.is_field_visible(TAB_EXECUTION, 6));
        assert!(app.is_field_visible(TAB_EXECUTION, 7));
        assert!(app.is_field_visible(TAB_EXECUTION, 8));
        assert!(app.is_field_visible(TAB_EXECUTION, 9));
    }

    #[test]
    fn test_field_visibility_other_tabs() {
        let app = App::new();
        // All fields on other tabs are always visible
        assert!(app.is_field_visible(0, 0));
        assert!(app.is_field_visible(TAB_SUPPLEMENTARY, 0));
        assert!(app.is_field_visible(TAB_SUPPLEMENTARY, 8));
    }

    // --- SLURM form fields ---

    #[test]
    fn test_slurm_form_defaults() {
        let form = RunForm::default();
        assert_eq!(form.execution_mode, 0);
        assert!(form.slurm_account.is_empty());
        assert!(form.slurm_partition.is_empty());
        assert_eq!(form.slurm_time, "02:00:00");
        assert_eq!(form.slurm_mem, "32");
        assert_eq!(form.slurm_cpus, "4");
        assert!(!form.slurm_submit);
    }

    // --- Execution mode select ---

    #[test]
    fn test_execution_mode_select() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 0;
        assert_eq!(app.select_value(), 0); // Local
        app.form.execution_mode = 1;
        assert_eq!(app.select_value(), 1); // SLURM
    }

    // --- SLURM field accessors ---

    #[test]
    fn test_slurm_text_values() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.slurm_account = "myacct".to_string();
        app.form.slurm_partition = "gpu".to_string();
        app.form.slurm_time = "04:00:00".to_string();
        app.form.slurm_mem = "64".to_string();
        app.form.slurm_cpus = "8".to_string();
        assert_eq!(app.get_text_value(TAB_EXECUTION, 4), "myacct");
        assert_eq!(app.get_text_value(TAB_EXECUTION, 5), "gpu");
        assert_eq!(app.get_text_value(TAB_EXECUTION, 6), "04:00:00");
        assert_eq!(app.get_text_value(TAB_EXECUTION, 7), "64");
        assert_eq!(app.get_text_value(TAB_EXECUTION, 8), "8");
    }

    #[test]
    fn test_slurm_checkbox_value() {
        let mut app = App::new();
        assert!(!app.get_checkbox_value(TAB_EXECUTION, 9)); // slurm_submit default
        app.form.slurm_submit = true;
        assert!(app.get_checkbox_value(TAB_EXECUTION, 9));
    }

    // --- Reset SLURM fields ---

    #[test]
    fn test_reset_slurm_fields() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        app.form.slurm_account = "changed".to_string();
        app.form.slurm_time = "99:00:00".to_string();
        // Reset entire tab
        app.reset_current_tab();
        assert_eq!(app.form.execution_mode, 0);
        assert!(app.form.slurm_account.is_empty());
        assert_eq!(app.form.slurm_time, "02:00:00");
    }

    #[test]
    fn test_reset_single_slurm_field() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 4; // slurm_account
        app.form.slurm_account = "changed".to_string();
        app.reset_current_field();
        assert!(app.form.slurm_account.is_empty());
    }

    // --- Navigation skips hidden fields ---

    #[test]
    fn test_navigation_skips_hidden_slurm_fields() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 2; // Debug (last visible in Local mode before Num Processes at 3)
        app.form.execution_mode = 1; // SLURM — hides fields 1 (Dry Run) and 3 (Num Processes)
        // Navigate down from Debug (2) — should skip 3 (hidden) and go to 4 (SLURM Account)
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.active_field, 4);
    }

    // --- Cursor doesn't go below Config File when no BIDS tree ---

    #[test]
    fn test_input_tab_no_tree_cursor_stays() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 3; // Config File (field 0=mode, 1=bids_dir, 2=output_dir, 3=config)
        // No BIDS tree loaded
        assert!(app.filter_state.tree.is_none());
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.active_field, 3); // Should stay on Config File
    }

    // --- NIfTI mode tests ---

    #[test]
    fn test_nifti_mode_switching() {
        let mut app = App::new();
        assert_eq!(app.input_mode, InputMode::Bids);
        // Field 0 is the mode selector; Right arrow cycles Bids -> NIfTI
        app.active_field = 0;
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input_mode, InputMode::NIfTI);
    }

    #[test]
    fn test_nifti_focus_navigation_into_nifti_section() {
        let mut app = App::new();
        app.active_field = 0;
        app.handle_key(key(KeyCode::Right)); // switch to NIfTI mode
        assert_eq!(app.input_mode, InputMode::NIfTI);
        // Navigate down past IO fields (0..3) into NIfTI section
        app.active_field = 3;
        app.handle_key(key(KeyCode::Down));
        // NIfTI mode always has content below, so active_field should be INPUT_IO_FIELDS (4)
        assert_eq!(app.active_field, App::INPUT_IO_FIELDS);
        // Default NIfTI focus is AddMagnitude
        assert_eq!(app.nifti_state.focus, NiftiFocus::AddMagnitude);
    }

    #[test]
    fn test_nifti_state_focus_next_and_prev() {
        let mut ns = NiftiState::default();
        assert_eq!(ns.focus, NiftiFocus::AddMagnitude);
        // focus_next cycles through items (no mag/phase files, so: AddMagnitude -> AddPhase -> EchoTimes -> ...)
        ns.focus_next();
        assert_eq!(ns.focus, NiftiFocus::AddPhase);
        ns.focus_next();
        assert_eq!(ns.focus, NiftiFocus::EchoTimes);
        ns.focus_next();
        assert_eq!(ns.focus, NiftiFocus::FieldStrength);
        ns.focus_next();
        assert_eq!(ns.focus, NiftiFocus::B0Direction);
        ns.focus_next();
        assert_eq!(ns.focus, NiftiFocus::ConvertButton);

        // focus_prev returns true when it moves
        assert!(ns.focus_prev());
        assert_eq!(ns.focus, NiftiFocus::B0Direction);

        // Go back to the top
        ns.focus = NiftiFocus::AddMagnitude;
        // focus_prev at the top returns false
        assert!(!ns.focus_prev());
    }

    #[test]
    fn test_nifti_editing_echo_times() {
        let mut app = App::new();
        app.active_field = 0;
        app.handle_key(key(KeyCode::Right)); // switch to NIfTI
        // Navigate into NIfTI section
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.focus = NiftiFocus::EchoTimes;
        // Press Enter to start editing
        app.handle_key(key(KeyCode::Enter));
        assert!(app.nifti_state.editing);
        // Type characters
        app.handle_key(key(KeyCode::Char('4')));
        app.handle_key(key(KeyCode::Char(',')));
        app.handle_key(key(KeyCode::Char(' ')));
        app.handle_key(key(KeyCode::Char('8')));
        assert_eq!(app.nifti_state.echo_times, "4, 8");
    }

    #[test]
    fn test_nifti_editing_field_strength() {
        let mut app = App::new();
        app.active_field = 0;
        app.handle_key(key(KeyCode::Right)); // switch to NIfTI
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.focus = NiftiFocus::FieldStrength;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.nifti_state.editing);
        app.handle_key(key(KeyCode::Char('3')));
        app.handle_key(key(KeyCode::Char('.')));
        app.handle_key(key(KeyCode::Char('0')));
        // Escape exits editing
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.nifti_state.editing);
        assert_eq!(app.nifti_state.field_strength, "3.0");
    }

    #[test]
    fn test_nifti_editing_b0_direction() {
        let mut app = App::new();
        app.active_field = 0;
        app.handle_key(key(KeyCode::Right)); // switch to NIfTI
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.focus = NiftiFocus::B0Direction;
        // Clear default value first
        app.nifti_state.b0_direction.clear();
        app.handle_key(key(KeyCode::Enter));
        assert!(app.nifti_state.editing);
        app.handle_key(key(KeyCode::Char('1')));
        app.handle_key(key(KeyCode::Char(',')));
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Char(',')));
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.nifti_state.b0_direction, "1,0,0");
    }

    #[test]
    fn test_dicom_mode_switching() {
        let mut app = App::new();
        assert_eq!(app.input_mode, InputMode::Bids);
        app.active_field = 0;
        // Right twice: Bids -> NIfTI -> DicomToBids
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input_mode, InputMode::DicomToBids);
    }

    #[test]
    fn test_dicom_state_default_focus() {
        let ds = DicomConvertState::default();
        assert_eq!(ds.focus, DicomFocus::Series(0));
    }

    #[test]
    fn test_nifti_scan_input_directory_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut ns = NiftiState {
            input_dir: dir.path().to_str().unwrap().to_string(),
            ..Default::default()
        };
        ns.scan_input_directory();
        assert!(ns.magnitude_files.is_empty());
        assert!(ns.phase_files.is_empty());
    }

    #[test]
    fn test_nifti_add_magnitude_editing() {
        let mut app = App::new();
        app.active_field = 0;
        app.handle_key(key(KeyCode::Right)); // switch to NIfTI
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.focus = NiftiFocus::AddMagnitude;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.nifti_state.editing);
        assert_eq!(
            app.nifti_state.adding_to,
            Some(crate::nifti::convert::NiftiPartType::Magnitude)
        );
    }

    #[test]
    fn test_nifti_add_phase_editing() {
        let mut app = App::new();
        app.active_field = 0;
        app.handle_key(key(KeyCode::Right)); // switch to NIfTI
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.focus = NiftiFocus::AddPhase;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.nifti_state.editing);
        assert_eq!(
            app.nifti_state.adding_to,
            Some(crate::nifti::convert::NiftiPartType::Phase)
        );
    }

    #[test]
    fn test_nifti_focus_wrapping_at_convert_button() {
        let mut ns = NiftiState {
            focus: NiftiFocus::ConvertButton,
            ..Default::default()
        };
        // focus_next at ConvertButton should stay at ConvertButton
        ns.focus_next();
        assert_eq!(ns.focus, NiftiFocus::ConvertButton);
    }

    // ========== Pipeline tab handler tests ==========

    #[test]
    fn test_pipeline_tab_navigate_down_up() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        assert_eq!(app.pipeline_state.focus, 0);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.pipeline_state.focus, 1);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.pipeline_state.focus, 2);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.pipeline_state.focus, 1);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.pipeline_state.focus, 0);
        // Up at 0 stays at 0
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.pipeline_state.focus, 0);
    }

    #[test]
    fn test_pipeline_tab_navigate_clamp_bottom() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        let max = app.pipeline_state.focusable_rows().len().saturating_sub(1);
        app.pipeline_state.focus = max;
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.pipeline_state.focus, max);
    }

    #[test]
    fn test_pipeline_tab_switch_from_pipeline() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, 2);
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.active_tab, 0);
    }

    #[test]
    fn test_pipeline_tab_number_switch() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, 2);
    }

    #[test]
    fn test_switch_to_separation_resets_focus_to_first_row() {
        let mut app = App::new();
        // Sit deep in the QSM tab, past the number of rows Separation will have.
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.sync_pipeline_mode();
        let qsm_max = app.pipeline_state.focusable_rows().len().saturating_sub(1);
        app.pipeline_state.focus = qsm_max;
        // Switch to Separation; mode changes, so focus must reset to the first selectable row.
        app.active_tab = TAB_SEPARATION;
        app.sync_pipeline_mode();
        assert_eq!(app.pipeline_state.mode, PipelineTabMode::Separation);
        assert_eq!(app.pipeline_state.focus, 0);
        // And it must be in range for the Separation row set.
        let sep_len = app.pipeline_state.focusable_rows().len();
        assert!(app.pipeline_state.focus < sep_len.max(1));
    }

    #[test]
    fn test_pipeline_left_right_algo_select() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Focus 0 is "Compute QSM" toggle, focus 1 is "Phase Combination" select
        // Find the focus index for the QSM Inversion AlgoSelect
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut qsm_inv_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::AlgoSelect { field: "qsm_algorithm", .. } = &rows[ri] {
                qsm_inv_focus = Some(fi);
                break;
            }
        }
        let fi = qsm_inv_focus.expect("qsm_algorithm row not found");
        app.pipeline_state.focus = fi;
        assert_eq!(app.pipeline_state.qsm_algorithm, 0); // rts
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.qsm_algorithm, 1); // tv
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.pipeline_state.qsm_algorithm, 0); // rts
    }

    #[test]
    fn test_pipeline_enter_opens_algo_modal_and_left_right_cycles() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Navigate to unwrapping_algorithm AlgoSelect
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut uw_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::AlgoSelect { field: "unwrapping_algorithm", .. } = &rows[ri] {
                uw_focus = Some(fi);
                break;
            }
        }
        let fi = uw_focus.expect("unwrapping_algorithm row not found");
        app.pipeline_state.focus = fi;
        assert_eq!(app.pipeline_state.unwrapping_algorithm, 0); // romeo

        // Enter opens the selection modal for this field (does not cycle).
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            app.algo_modal.as_ref().unwrap().target,
            AlgoModalTarget::PipelineSelect(ref f) if f == "unwrapping_algorithm"
        ));
        assert_eq!(app.pipeline_state.unwrapping_algorithm, 0); // still unchanged
        // Pick the next option (laplacian) and confirm.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.pipeline_state.unwrapping_algorithm, 1); // laplacian

        // Focus is restored to the selector after confirming; Left still cycles inline (no modal).
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.pipeline_state.unwrapping_algorithm, 0);
        assert!(app.algo_modal.is_none());
    }

    #[test]
    fn test_pipeline_edit_text_param() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find the obliquity_threshold Param row
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut obl_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::Param { field: "obliquity_threshold", .. } = &rows[ri] {
                obl_focus = Some(fi);
                break;
            }
        }
        let fi = obl_focus.expect("obliquity_threshold row not found");
        app.pipeline_state.focus = fi;
        // Enter editing
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        // Type some chars
        app.handle_key(key(KeyCode::Home));
        app.handle_key(key(KeyCode::End));
        // Backspace to clear
        for _ in 0..app.pipeline_state.get_param("obliquity_threshold").len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        app.handle_key(key(KeyCode::Char('5')));
        assert_eq!(app.pipeline_state.get_param("obliquity_threshold"), "5");
        // Escape exits editing
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.editing);
    }

    #[test]
    fn test_pipeline_toggle_do_qsm() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Focus 0 should be the do_qsm toggle
        app.pipeline_state.focus = 0;
        assert!(app.pipeline_state.do_qsm);
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.do_qsm);
        // Toggling back
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(app.pipeline_state.do_qsm);
    }

    #[test]
    fn test_pipeline_toggle_inhomogeneity_correction() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find inhomogeneity_correction toggle
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut ic_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::Toggle { field: "inhomogeneity_correction", .. } = &rows[ri] {
                ic_focus = Some(fi);
                break;
            }
        }
        let fi = ic_focus.expect("inhomogeneity_correction not found");
        app.pipeline_state.focus = fi;
        assert!(app.pipeline_state.inhomogeneity_correction);
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.inhomogeneity_correction);
    }

    #[test]
    fn test_pipeline_quit_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_pipeline_esc_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn test_pipeline_f5_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.form.bids_dir = "/tmp/bids".to_string();
        app.handle_key(key(KeyCode::F(5)));
        assert!(app.should_run);
    }

    #[test]
    fn test_pipeline_reset_field() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.pipeline_state.qsm_algorithm = 5;
        // Find qsm_algorithm focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut fi_found = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::AlgoSelect { field: "qsm_algorithm", .. } = &rows[ri] {
                fi_found = Some(fi);
                break;
            }
        }
        let fi = fi_found.expect("qsm_algorithm not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.pipeline_state.qsm_algorithm, 0); // default
    }

    #[test]
    fn test_pipeline_reset_tab() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.pipeline_state.qsm_algorithm = 5;
        app.pipeline_state.bf_algorithm = 3;
        app.handle_key(key(KeyCode::Char('R')));
        assert_eq!(app.pipeline_state.qsm_algorithm, 0);
        assert_eq!(app.pipeline_state.bf_algorithm, 0);
    }

    #[test]
    fn test_pipeline_visible_rows_change_on_toggle() {
        let mut app = App::new();
        let rows_with_qsm = app.pipeline_state.visible_rows().len();
        app.pipeline_state.do_qsm = false;
        let rows_without_qsm = app.pipeline_state.visible_rows().len();
        assert!(rows_without_qsm < rows_with_qsm);
    }

    #[test]
    fn test_pipeline_editing_left_right_cursor() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find obliquity_threshold param and edit it
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut fi = 0;
        for (f, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::Param { field: "obliquity_threshold", .. } = &rows[ri] {
                fi = f;
                break;
            }
        }
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        // Move cursor left and right
        let len = app.pipeline_state.get_param("obliquity_threshold").len();
        assert_eq!(app.pipeline_state.cursor, len);
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.pipeline_state.cursor, len - 1);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.cursor, len);
        // Enter exits editing
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.editing);
    }

    // ========== Supplementary tab handler tests ==========

    #[test]
    fn test_supplementary_toggle_swi() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.active_field = 0; // Compute SWI checkbox
        assert!(!app.form.do_swi);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.do_swi);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!app.form.do_swi);
    }

    #[test]
    fn test_supplementary_toggle_t2star() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.active_field = 7; // Compute T2* Map
        assert!(!app.form.do_t2starmap);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.do_t2starmap);
    }

    #[test]
    fn test_supplementary_toggle_r2star() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.active_field = 8; // Compute R2* Map
        assert!(!app.form.do_r2starmap);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(app.form.do_r2starmap);
    }

    #[test]
    fn test_supplementary_swi_scaling_select() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.form.do_swi = true; // make SWI fields visible
        app.active_field = 1; // SWI Scaling select
        assert_eq!(app.form.swi_scaling, 0); // tanh
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.form.swi_scaling, 1); // negative-tanh
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.form.swi_scaling, 0);
    }

    #[test]
    fn test_supplementary_edit_swi_strength() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.form.do_swi = true;
        app.active_field = 2; // SWI Strength text field
        app.handle_key(key(KeyCode::Enter));
        assert!(app.editing);
        // Type something
        app.handle_key(key(KeyCode::End));
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.editing);
    }

    #[test]
    fn test_supplementary_navigate_fields() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.active_field = 0;
        // SWI fields are hidden when do_swi is false, so Down from 0 skips to 7
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.active_field, 7);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.active_field, 8);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.active_field, 7);
    }

    #[test]
    fn test_supplementary_swi_visible_when_enabled() {
        let mut app = App::new();
        app.form.do_swi = true;
        assert!(app.is_field_visible(TAB_SUPPLEMENTARY, 1)); // SWI Scaling
        assert!(app.is_field_visible(TAB_SUPPLEMENTARY, 2)); // SWI Strength
        assert!(app.is_field_visible(TAB_SUPPLEMENTARY, 6)); // SWI MIP Window
    }

    #[test]
    fn test_supplementary_swi_hidden_when_disabled() {
        let app = App::new();
        assert!(!app.is_field_visible(TAB_SUPPLEMENTARY, 1));
        assert!(!app.is_field_visible(TAB_SUPPLEMENTARY, 6));
    }

    #[test]
    fn test_supplementary_tab_switch() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_EXECUTION);
    }

    #[test]
    fn test_supplementary_reset_field() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.active_field = 0;
        app.form.do_swi = true;
        app.handle_key(key(KeyCode::Char('r')));
        assert!(!app.form.do_swi); // reset to default
    }

    #[test]
    fn test_supplementary_reset_tab() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.form.do_swi = true;
        app.form.do_t2starmap = true;
        app.form.do_r2starmap = true;
        app.form.swi_scaling = 3;
        app.handle_key(key(KeyCode::Char('R')));
        assert!(!app.form.do_swi);
        assert!(!app.form.do_t2starmap);
        assert!(!app.form.do_r2starmap);
        assert_eq!(app.form.swi_scaling, 0);
    }

    // ========== Execution tab handler tests ==========

    #[test]
    fn test_execution_mode_switch_with_left_right() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 0; // Execution Mode select
        assert_eq!(app.form.execution_mode, 0); // Local
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.form.execution_mode, 1); // SLURM
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.form.execution_mode, 0); // Local
    }

    #[test]
    fn test_execution_mode_enter_opens_modal() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 0;
        // Enter opens the modal (rather than cycling); selecting SLURM applies it.
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_some());
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.form.execution_mode, 1); // SLURM
        // Left/Right still cycle the select inline without opening the modal.
        app.handle_key(key(KeyCode::Left));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.form.execution_mode, 0); // Local
    }

    #[test]
    fn test_execution_toggle_dry_run() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 1; // Dry Run checkbox
        assert!(!app.form.dry_run);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.dry_run);
    }

    #[test]
    fn test_execution_toggle_debug() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 2; // Debug checkbox
        assert!(!app.form.debug);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.debug);
    }

    #[test]
    fn test_execution_edit_n_procs() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 3; // Num Processes
        app.handle_key(key(KeyCode::Enter));
        assert!(app.editing);
        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.form.n_procs, "4");
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.editing);
    }

    #[test]
    fn test_execution_edit_slurm_account() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1; // SLURM mode
        app.active_field = 4; // SLURM Account
        app.handle_key(key(KeyCode::Enter));
        assert!(app.editing);
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('c')));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.form.slurm_account, "act");
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.editing);
    }

    #[test]
    fn test_execution_edit_slurm_partition() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        app.active_field = 5;
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('g')));
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('u')));
        assert_eq!(app.form.slurm_partition, "gpu");
    }

    #[test]
    fn test_execution_edit_slurm_time() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        app.active_field = 6;
        app.handle_key(key(KeyCode::Enter));
        // Clear existing "02:00:00"
        for _ in 0..8 {
            app.handle_key(key(KeyCode::Backspace));
        }
        app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(app.form.slurm_time, "1");
    }

    #[test]
    fn test_execution_toggle_auto_submit() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        app.active_field = 9; // Auto-Submit checkbox
        assert!(!app.form.slurm_submit);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.slurm_submit);
    }

    #[test]
    fn test_execution_reset_field() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 6; // SLURM Time
        app.form.slurm_time = "99:99:99".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.form.slurm_time, "02:00:00");
    }

    #[test]
    fn test_execution_reset_tab() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        app.form.dry_run = true;
        app.form.debug = true;
        app.form.slurm_account = "x".to_string();
        app.handle_key(key(KeyCode::Char('R')));
        assert_eq!(app.form.execution_mode, 0);
        assert!(!app.form.dry_run);
        assert!(!app.form.debug);
        assert!(app.form.slurm_account.is_empty());
    }

    // ========== Methods tab handler tests ==========

    #[test]
    fn test_methods_tab_scroll_down() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        assert_eq!(app.methods_scroll_offset, 0);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.methods_scroll_offset, 1);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.methods_scroll_offset, 2);
    }

    #[test]
    fn test_methods_tab_scroll_up() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.methods_scroll_offset = 3;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.methods_scroll_offset, 2);
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.methods_scroll_offset, 1);
    }

    #[test]
    fn test_methods_tab_scroll_up_at_zero() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.methods_scroll_offset, 0);
    }

    #[test]
    fn test_methods_tab_quit() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_methods_tab_esc() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn test_methods_tab_switch() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, 0);
    }

    #[test]
    fn test_methods_tab_backtab() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.active_tab, TAB_EXECUTION);
    }

    #[test]
    fn test_methods_tab_number_switch() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn test_methods_tab_f5_triggers_run() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.form.bids_dir = "/tmp/bids".to_string();
        app.handle_key(key(KeyCode::F(5)));
        assert!(app.should_run);
    }

    #[test]
    fn test_methods_tab_enter_triggers_run() {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.form.bids_dir = "/tmp/bids".to_string();
        app.handle_key(key(KeyCode::Enter));
        assert!(app.should_run);
    }

    // ========== Filter tree key handler tests ==========

    #[test]
    fn test_filter_enter_include_editing() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS; // in filter tree area
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.focus = FilterFocus::Include;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.filter_state.include_editing);
    }

    #[test]
    fn test_filter_enter_exclude_editing() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.focus = FilterFocus::Exclude;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.filter_state.exclude_editing);
    }

    #[test]
    fn test_filter_type_in_include() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.focus = FilterFocus::Include;
        app.filter_state.include_editing = true;
        app.filter_state.include_pattern.clear();
        app.filter_state.include_cursor = 0;
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Char('u')));
        app.handle_key(key(KeyCode::Char('b')));
        assert_eq!(app.filter_state.include_pattern, "sub");
        assert_eq!(app.filter_state.include_cursor, 3);
    }

    #[test]
    fn test_filter_esc_stops_include_editing() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.include_editing = true;
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.filter_state.include_editing);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_filter_enter_stops_include_editing_and_applies() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.include_editing = true;
        app.filter_state.include_pattern = "sub-1*".to_string();
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.filter_state.include_editing);
    }

    #[test]
    fn test_filter_type_in_exclude() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.focus = FilterFocus::Exclude;
        app.filter_state.exclude_editing = true;
        app.filter_state.exclude_cursor = 0;
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.filter_state.exclude_pattern, "x");
    }

    #[test]
    fn test_filter_text_backspace() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.include_editing = true;
        app.filter_state.include_pattern = "abc".to_string();
        app.filter_state.include_cursor = 3;
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.filter_state.include_pattern, "ab");
        assert_eq!(app.filter_state.include_cursor, 2);
    }

    #[test]
    fn test_filter_text_delete() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.include_editing = true;
        app.filter_state.include_pattern = "abc".to_string();
        app.filter_state.include_cursor = 0;
        app.handle_key(key(KeyCode::Delete));
        assert_eq!(app.filter_state.include_pattern, "bc");
    }

    #[test]
    fn test_filter_text_home_end() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.include_editing = true;
        app.filter_state.include_pattern = "hello".to_string();
        app.filter_state.include_cursor = 3;
        app.handle_key(key(KeyCode::Home));
        assert_eq!(app.filter_state.include_cursor, 0);
        app.handle_key(key(KeyCode::End));
        assert_eq!(app.filter_state.include_cursor, 5);
    }

    #[test]
    fn test_filter_text_left_right() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.include_editing = true;
        app.filter_state.include_pattern = "abc".to_string();
        app.filter_state.include_cursor = 2;
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.filter_state.include_cursor, 1);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.filter_state.include_cursor, 2);
    }

    #[test]
    fn test_filter_num_echoes_editing() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.focus = FilterFocus::NumEchoes;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.filter_state.num_echoes_editing);
        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.filter_state.num_echoes, "2");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.filter_state.num_echoes, "");
        app.handle_key(key(KeyCode::Char('4')));
        app.handle_key(key(KeyCode::Home));
        assert_eq!(app.filter_state.num_echoes_cursor, 0);
        app.handle_key(key(KeyCode::End));
        assert_eq!(app.filter_state.num_echoes_cursor, 1);
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.filter_state.num_echoes_cursor, 0);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.filter_state.num_echoes_cursor, 1);
        // Delete at cursor
        app.handle_key(key(KeyCode::Home));
        app.handle_key(key(KeyCode::Delete));
        assert_eq!(app.filter_state.num_echoes, "");
        // Enter exits editing
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.filter_state.num_echoes_editing);
    }

    #[test]
    fn test_filter_select_all_via_key() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut app = App::new();
        app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.focus = FilterFocus::TreeNode(0);
        // Select none
        app.handle_key(key(KeyCode::Char('n')));
        assert!(app.filter_state.manual_override);
        let tree = app.filter_state.tree.as_ref().unwrap();
        assert_eq!(tree.selected_runs(), 0);
        // Select all
        app.handle_key(key(KeyCode::Char('a')));
        let tree = app.filter_state.tree.as_ref().unwrap();
        assert_eq!(tree.selected_runs(), tree.total_runs());
    }

    #[test]
    fn test_filter_space_toggles_node() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut app = App::new();
        app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        // Navigate to a run leaf
        app.filter_state.focus = FilterFocus::Exclude;
        app.filter_state.focus_next(); // TreeNode(0) subject
        app.filter_state.focus_next(); // TreeNode(1) run
        let tree = app.filter_state.tree.as_ref().unwrap();
        let was_selected = tree.subjects[0].runs[0].selected;
        app.handle_key(key(KeyCode::Char(' ')));
        let tree = app.filter_state.tree.as_ref().unwrap();
        assert_ne!(tree.subjects[0].runs[0].selected, was_selected);
        assert!(app.filter_state.manual_override);
    }

    #[test]
    fn test_filter_left_right_collapse() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut app = App::new();
        app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.focus = FilterFocus::TreeNode(0); // subject
        let rows_before = app.filter_state.visible_rows().len();
        app.handle_key(key(KeyCode::Left));
        let rows_after = app.filter_state.visible_rows().len();
        assert!(rows_after < rows_before);
        // Expand with Right
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.filter_state.visible_rows().len(), rows_before);
    }

    #[test]
    fn test_filter_up_at_include_goes_to_io() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut app = App::new();
        app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.focus = FilterFocus::Include;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
    }

    #[test]
    fn test_filter_tab_switch() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.focus = FilterFocus::Exclude;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn test_filter_f5() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
        app.filter_state.focus = FilterFocus::Include;
        app.form.bids_dir = "/tmp/bids".to_string();
        app.handle_key(key(KeyCode::F(5)));
        assert!(app.should_run);
    }

    // ========== NIfTI handler tests (delete, reorder) ==========

    #[test]
    fn test_nifti_delete_magnitude_file() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.magnitude_files = vec![
            std::path::PathBuf::from("/a.nii"),
            std::path::PathBuf::from("/b.nii"),
        ];
        app.nifti_state.focus = NiftiFocus::MagFile(0);
        app.handle_key(key(KeyCode::Delete));
        assert_eq!(app.nifti_state.magnitude_files.len(), 1);
        assert_eq!(app.nifti_state.magnitude_files[0], std::path::PathBuf::from("/b.nii"));
    }

    #[test]
    fn test_nifti_delete_last_magnitude_resets_focus() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.magnitude_files = vec![std::path::PathBuf::from("/a.nii")];
        app.nifti_state.focus = NiftiFocus::MagFile(0);
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.nifti_state.magnitude_files.is_empty());
        assert_eq!(app.nifti_state.focus, NiftiFocus::AddMagnitude);
    }

    #[test]
    fn test_nifti_delete_phase_file() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.phase_files = vec![
            std::path::PathBuf::from("/p1.nii"),
            std::path::PathBuf::from("/p2.nii"),
        ];
        app.nifti_state.focus = NiftiFocus::PhaseFile(1);
        app.handle_key(key(KeyCode::Delete));
        assert_eq!(app.nifti_state.phase_files.len(), 1);
        // Focus should clamp to 0 since we deleted index 1
        assert_eq!(app.nifti_state.focus, NiftiFocus::PhaseFile(0));
    }

    #[test]
    fn test_nifti_delete_last_phase_resets_focus() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.phase_files = vec![std::path::PathBuf::from("/p.nii")];
        app.nifti_state.focus = NiftiFocus::PhaseFile(0);
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.nifti_state.phase_files.is_empty());
        assert_eq!(app.nifti_state.focus, NiftiFocus::AddPhase);
    }

    #[test]
    fn test_nifti_reorder_magnitude_down() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.magnitude_files = vec![
            std::path::PathBuf::from("/a.nii"),
            std::path::PathBuf::from("/b.nii"),
        ];
        app.nifti_state.focus = NiftiFocus::MagFile(0);
        app.handle_key(key(KeyCode::Char('J'))); // Shift+J = move down
        assert_eq!(app.nifti_state.magnitude_files[0], std::path::PathBuf::from("/b.nii"));
        assert_eq!(app.nifti_state.magnitude_files[1], std::path::PathBuf::from("/a.nii"));
        assert_eq!(app.nifti_state.focus, NiftiFocus::MagFile(1));
    }

    #[test]
    fn test_nifti_reorder_magnitude_up() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.magnitude_files = vec![
            std::path::PathBuf::from("/a.nii"),
            std::path::PathBuf::from("/b.nii"),
        ];
        app.nifti_state.focus = NiftiFocus::MagFile(1);
        app.handle_key(key(KeyCode::Char('K'))); // Shift+K = move up
        assert_eq!(app.nifti_state.magnitude_files[0], std::path::PathBuf::from("/b.nii"));
        assert_eq!(app.nifti_state.magnitude_files[1], std::path::PathBuf::from("/a.nii"));
        assert_eq!(app.nifti_state.focus, NiftiFocus::MagFile(0));
    }

    #[test]
    fn test_nifti_reorder_phase_down() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.phase_files = vec![
            std::path::PathBuf::from("/p1.nii"),
            std::path::PathBuf::from("/p2.nii"),
        ];
        app.nifti_state.focus = NiftiFocus::PhaseFile(0);
        app.handle_key(key(KeyCode::Char('J')));
        assert_eq!(app.nifti_state.phase_files[0], std::path::PathBuf::from("/p2.nii"));
        assert_eq!(app.nifti_state.focus, NiftiFocus::PhaseFile(1));
    }

    #[test]
    fn test_nifti_reorder_phase_up() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.phase_files = vec![
            std::path::PathBuf::from("/p1.nii"),
            std::path::PathBuf::from("/p2.nii"),
        ];
        app.nifti_state.focus = NiftiFocus::PhaseFile(1);
        app.handle_key(key(KeyCode::Char('K')));
        assert_eq!(app.nifti_state.phase_files[0], std::path::PathBuf::from("/p2.nii"));
        assert_eq!(app.nifti_state.focus, NiftiFocus::PhaseFile(0));
    }

    #[test]
    fn test_nifti_up_at_top_goes_to_io() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.focus = NiftiFocus::AddMagnitude;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
    }

    #[test]
    fn test_nifti_tab_switch() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn test_nifti_quit() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_nifti_editing_backspace_home_end_left_right() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.nifti_state.focus = NiftiFocus::EchoTimes;
        app.nifti_state.echo_times = "12, 24".to_string();
        app.handle_key(key(KeyCode::Enter));
        assert!(app.nifti_state.editing);
        // cursor at end = 6
        app.handle_key(key(KeyCode::Home));
        assert_eq!(app.nifti_state.cursor, 0);
        app.handle_key(key(KeyCode::End));
        assert_eq!(app.nifti_state.cursor, 6);
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.nifti_state.cursor, 5);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.nifti_state.cursor, 6);
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.nifti_state.echo_times, "12, 2");
    }

    // ========== DICOM tab handler tests ==========

    #[test]
    fn test_dicom_tab_navigate_up_to_io() {
        let mut app = App::new();
        app.input_mode = InputMode::DicomToBids;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.dicom_state.focus = DicomFocus::Series(0);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
    }

    #[test]
    fn test_dicom_tab_switch() {
        let mut app = App::new();
        app.input_mode = InputMode::DicomToBids;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn test_dicom_tab_quit() {
        let mut app = App::new();
        app.input_mode = InputMode::DicomToBids;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_dicom_focus_next_prev_no_session() {
        let mut ds = DicomConvertState::default();
        // No session, so focus_next from Series(0) goes to ConvertButton
        ds.focus_next();
        assert_eq!(ds.focus, DicomFocus::ConvertButton);
        // focus_prev from ConvertButton with no session stays at ConvertButton
        ds.focus_prev();
        assert_eq!(ds.focus, DicomFocus::ConvertButton);
    }

    #[test]
    fn test_dicom_navigate_down() {
        let mut app = App::new();
        app.input_mode = InputMode::DicomToBids;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        // No session, so focus_next just goes to ConvertButton
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.dicom_state.focus, DicomFocus::ConvertButton);
    }

    // ========== IO select (input mode cycling) ==========

    #[test]
    fn test_adjust_io_select_cycles_mode() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 0;
        assert_eq!(app.input_mode, InputMode::Bids);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input_mode, InputMode::NIfTI);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input_mode, InputMode::DicomToBids);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input_mode, InputMode::Bids); // wraps around
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.input_mode, InputMode::DicomToBids); // wraps other way
    }

    #[test]
    fn test_enter_on_io_mode_opens_modal() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 0;
        assert_eq!(app.input_mode, InputMode::Bids);
        // Enter opens the input-mode modal; picking NIfTI applies it.
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_some());
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.input_mode, InputMode::NIfTI);
        // Left/Right still cycles inline without the modal.
        app.handle_key(key(KeyCode::Left));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.input_mode, InputMode::Bids);
    }

    // ========== try_run validation tests ==========

    #[test]
    fn test_try_run_dicom_not_converted() {
        let mut app = App::new();
        app.input_mode = InputMode::DicomToBids;
        app.form.bids_dir = "/tmp/bids".to_string();
        app.try_run();
        assert!(!app.should_run);
        assert!(app.error_message.as_deref().unwrap().contains("Convert DICOM"));
    }

    #[test]
    fn test_try_run_nifti_not_converted() {
        let mut app = App::new();
        app.input_mode = InputMode::NIfTI;
        app.form.bids_dir = "/tmp/bids".to_string();
        app.try_run();
        assert!(!app.should_run);
        assert!(app.error_message.as_deref().unwrap().contains("Convert NIfTI"));
    }

    #[test]
    fn test_try_run_empty_bids_dir() {
        let mut app = App::new();
        app.try_run();
        assert!(!app.should_run);
        assert!(app.error_message.as_deref().unwrap().contains("BIDS Directory"));
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.active_field, 0);
    }

    #[test]
    fn test_try_run_slurm_no_account() {
        let mut app = App::new();
        app.form.bids_dir = "/tmp/bids".to_string();
        app.form.execution_mode = 1;
        app.try_run();
        assert!(!app.should_run);
        assert!(app.error_message.as_deref().unwrap().contains("SLURM Account"));
        assert_eq!(app.active_tab, 3);
        assert_eq!(app.active_field, 4);
    }

    #[test]
    fn test_try_run_slurm_with_account() {
        let mut app = App::new();
        app.form.bids_dir = "/tmp/bids".to_string();
        app.form.execution_mode = 1;
        app.form.slurm_account = "myacct".to_string();
        app.try_run();
        assert!(app.should_run);
    }

    // ========== Reset tests on various tabs ==========

    #[test]
    fn test_reset_input_mode_field() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 0;
        app.input_mode = InputMode::DicomToBids;
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.input_mode, InputMode::Bids);
    }

    #[test]
    fn test_reset_bids_dir_field() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 1;
        app.form.bids_dir = "/something".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.form.bids_dir.is_empty());
    }

    #[test]
    fn test_reset_output_dir_field() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 2;
        app.form.output_dir = "/out".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.form.output_dir.is_empty());
    }

    #[test]
    fn test_reset_config_file_field() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 3;
        app.form.config_file = "cfg.toml".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.form.config_file.is_empty());
    }

    #[test]
    fn test_reset_input_tab() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.input_mode = InputMode::DicomToBids;
        app.form.bids_dir = "/x".to_string();
        app.form.output_dir = "/y".to_string();
        app.handle_key(key(KeyCode::Char('R')));
        assert_eq!(app.input_mode, InputMode::Bids);
        assert!(app.form.bids_dir.is_empty());
        assert!(app.form.output_dir.is_empty());
    }

    #[test]
    fn test_reset_nifti_input_dir() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 1;
        app.input_mode = InputMode::NIfTI;
        app.nifti_state.input_dir = "/data".to_string();
        app.nifti_state.magnitude_files.push(std::path::PathBuf::from("/a.nii"));
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.nifti_state.input_dir.is_empty());
        assert!(app.nifti_state.magnitude_files.is_empty());
    }

    #[test]
    fn test_reset_nifti_output_dir() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 2;
        app.input_mode = InputMode::NIfTI;
        app.nifti_state.output_dir = "/out".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.nifti_state.output_dir.is_empty());
    }

    #[test]
    fn test_reset_dicom_dir() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 1;
        app.input_mode = InputMode::DicomToBids;
        app.dicom_state.dicom_dir = "/dcm".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.dicom_state.dicom_dir.is_empty());
        assert!(app.dicom_state.scanned_dir.is_none());
    }

    #[test]
    fn test_reset_dicom_output_dir() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 2;
        app.input_mode = InputMode::DicomToBids;
        app.dicom_state.output_dir = "/out".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.dicom_state.output_dir.is_empty());
    }

    #[test]
    fn test_reset_supplementary_fields() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.active_field = 2;
        app.form.swi_strength = "999".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        let defaults = RunForm::default();
        assert_eq!(app.form.swi_strength, defaults.swi_strength);
    }

    #[test]
    fn test_reset_execution_fields() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.active_field = 7;
        app.form.slurm_mem = "999".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.form.slurm_mem, "32");
    }

    // ========== Pipeline mask operations ==========

    #[test]
    fn test_pipeline_mask_preset_switch() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find mask_preset focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut mp_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::AlgoSelect { field: "mask_preset", .. } = &rows[ri] {
                mp_focus = Some(fi);
                break;
            }
        }
        let fi = mp_focus.expect("mask_preset not found");
        app.pipeline_state.focus = fi;
        assert_eq!(app.pipeline_state.mask_preset, 0); // robust threshold
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.mask_preset, 1); // BET
    }

    #[test]
    fn test_mask_threshold_method_modal_opens_and_sets() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.sync_pipeline_mode();
        // Default preset is a threshold generator, so the param row is the method select.
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }
        ));
        let rows = app.pipeline_state.visible_rows();
        let target = app.pipeline_state.focusable_rows().iter().position(|&ri| {
            matches!(rows.get(ri), Some(PipelineRow::MaskOpGeneratorParam { .. }))
        }).expect("MaskOpGeneratorParam row not focusable");
        app.pipeline_state.focus = target;
        app.handle_key(key(KeyCode::Enter));
        let modal = app.algo_modal.as_ref().expect("modal should open over threshold Method");
        assert!(matches!(modal.target, AlgoModalTarget::MaskThresholdMethod(_)));
        assert_eq!(modal.options.len(), 3);
        // Pick "percentile" (index 2) and confirm.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. }
        ));
    }

    #[test]
    fn test_mask_bet_frac_intensity_opens_no_modal() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.sync_pipeline_mode();
        // Switch the generator to BET so the param row is a numeric frac-intensity slider.
        app.pipeline_state.mask_sections[0].generator = MaskOp::Bet { fractional_intensity: 0.5 };
        let rows = app.pipeline_state.visible_rows();
        let target = app.pipeline_state.focusable_rows().iter().position(|&ri| {
            matches!(rows.get(ri), Some(PipelineRow::MaskOpGeneratorParam { .. }))
        }).expect("MaskOpGeneratorParam row not focusable");
        app.pipeline_state.focus = target;
        app.handle_key(key(KeyCode::Enter));
        // BET frac. intensity is a slider, not a discrete select — no modal.
        assert!(app.algo_modal.is_none());
    }

    #[test]
    fn test_pipeline_mask_add_section() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        let initial_sections = app.pipeline_state.mask_sections.len();
        // Find MaskOpAddSection focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut add_sec_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if matches!(&rows[ri], PipelineRow::MaskOpAddSection) {
                add_sec_focus = Some(fi);
                break;
            }
        }
        let fi = add_sec_focus.expect("MaskOpAddSection not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.pipeline_state.mask_sections.len(), initial_sections + 1);
    }

    #[test]
    fn test_pipeline_mask_add_step() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        let initial_refs = app.pipeline_state.mask_sections[0].refinements.len();
        // Find MaskOpAddStep for section 0
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut add_step_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpAddStep { section: 0 } = &rows[ri] {
                add_step_focus = Some(fi);
                break;
            }
        }
        let fi = add_step_focus.expect("MaskOpAddStep not found");
        app.pipeline_state.focus = fi;
        // Enter to start adding
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.mask_ops_adding);
        // Enter again to confirm the add
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.mask_ops_adding);
        assert_eq!(app.pipeline_state.mask_sections[0].refinements.len(), initial_refs + 1);
    }

    #[test]
    fn test_pipeline_mask_add_step_cycle_ops() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut add_step_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpAddStep { section: 0 } = &rows[ri] {
                add_step_focus = Some(fi);
                break;
            }
        }
        let fi = add_step_focus.expect("MaskOpAddStep not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Enter)); // start adding
        assert!(app.pipeline_state.mask_ops_adding);
        assert_eq!(app.pipeline_state.mask_ops_add_idx, 0);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.mask_ops_add_idx, 1);
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.pipeline_state.mask_ops_add_idx, 0);
        // Escape cancels
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.mask_ops_adding);
    }

    #[test]
    fn test_pipeline_mask_delete_refinement() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        let initial_refs = app.pipeline_state.mask_sections[0].refinements.len();
        assert!(initial_refs > 0);
        // Find first MaskOpEntry focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut entry_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpEntry { section: 0, .. } = &rows[ri] {
                entry_focus = Some(fi);
                break;
            }
        }
        let fi = entry_focus.expect("MaskOpEntry not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.pipeline_state.mask_sections[0].refinements.len(), initial_refs - 1);
    }

    #[test]
    fn test_pipeline_mask_adjust_op_left_right() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find MaskOpEntry focus and adjust with Left/Right
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut entry_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpEntry { section: 0, .. } = &rows[ri] {
                entry_focus = Some(fi);
                break;
            }
        }
        let fi = entry_focus.expect("MaskOpEntry not found");
        app.pipeline_state.focus = fi;
        // Just verify no crash from left/right
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Left));
    }

    #[test]
    fn test_pipeline_mask_generator_adjust() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find MaskOpGenerator focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut gen_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpGenerator { section: 0 } = &rows[ri] {
                gen_focus = Some(fi);
                break;
            }
        }
        let fi = gen_focus.expect("MaskOpGenerator not found");
        app.pipeline_state.focus = fi;
        // Switch generator threshold <-> bet
        app.handle_key(key(KeyCode::Right));
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            crate::pipeline::config::MaskOp::Bet { .. }
        ));
        app.handle_key(key(KeyCode::Left));
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            crate::pipeline::config::MaskOp::Threshold { .. }
        ));
    }

    #[test]
    fn test_pipeline_mask_generator_param_adjust() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find MaskOpGeneratorParam focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut gp_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpGeneratorParam { section: 0 } = &rows[ri] {
                gp_focus = Some(fi);
                break;
            }
        }
        let fi = gp_focus.expect("MaskOpGeneratorParam not found");
        app.pipeline_state.focus = fi;
        // Adjust threshold method
        app.handle_key(key(KeyCode::Right));
        // Verify the method changed (Otsu -> Fixed)
        if let crate::pipeline::config::MaskOp::Threshold { method, .. } = &app.pipeline_state.mask_sections[0].generator {
            assert_eq!(*method, crate::pipeline::config::MaskThresholdMethod::Fixed);
        }
    }

    #[test]
    fn test_pipeline_mask_input_adjust() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find MaskOpInput focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut inp_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpInput { section: 0 } = &rows[ri] {
                inp_focus = Some(fi);
                break;
            }
        }
        let fi = inp_focus.expect("MaskOpInput not found");
        app.pipeline_state.focus = fi;
        let before = app.pipeline_state.mask_sections[0].input;
        app.handle_key(key(KeyCode::Right));
        // Should have cycled to a different input
        assert_ne!(app.pipeline_state.mask_sections[0].input, before);
    }

    // ========== word_boundary_left tests ==========

    #[test]
    fn test_word_boundary_left() {
        assert_eq!(word_boundary_left("hello world", 11), 6);
        assert_eq!(word_boundary_left("hello world", 5), 0);
        assert_eq!(word_boundary_left("a/b/c", 5), 4);
        assert_eq!(word_boundary_left("", 0), 0);
        assert_eq!(word_boundary_left("abc", 0), 0);
    }

    // ========== Error message clears on non-F5 key ==========

    #[test]
    fn test_error_clears_on_keypress() {
        let mut app = App::new();
        app.error_message = Some("test error".to_string());
        app.handle_key(key(KeyCode::Down));
        assert!(app.error_message.is_none());
    }

    #[test]
    fn test_error_does_not_clear_on_f5() {
        let mut app = App::new();
        // F5 with no bids_dir sets an error
        app.handle_key(key(KeyCode::F(5)));
        assert!(app.error_message.is_some());
    }

    // ========== Pipeline reset_pipeline_field for Param and Toggle ==========

    #[test]
    fn test_reset_pipeline_param_field() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Find obliquity_threshold param
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut fi = 0;
        for (f, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::Param { field: "obliquity_threshold", .. } = &rows[ri] {
                fi = f;
                break;
            }
        }
        app.pipeline_state.focus = fi;
        // Modify the value
        if let Some(s) = app.pipeline_state.get_param_mut("obliquity_threshold") {
            *s = "999".to_string();
        }
        // Reset
        app.handle_key(key(KeyCode::Char('r')));
        let defaults = PipelineFormState::default();
        assert_eq!(
            app.pipeline_state.get_param("obliquity_threshold"),
            defaults.get_param("obliquity_threshold")
        );
    }

    #[test]
    fn test_reset_pipeline_toggle_field() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Focus 0 is do_qsm toggle
        app.pipeline_state.focus = 0;
        app.pipeline_state.do_qsm = false;
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.pipeline_state.do_qsm); // default is true
    }

    // ========== Supplementary: edit SWI fields ==========

    #[test]
    fn test_supplementary_edit_swi_hp_sigmas() {
        let mut app = App::new();
        app.active_tab = TAB_SUPPLEMENTARY;
        app.form.do_swi = true;
        // Edit SWI HP Sigma X (field 3)
        app.active_field = 3;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.editing);
        app.handle_key(key(KeyCode::End));
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.editing);

        // Edit SWI HP Sigma Y (field 4)
        app.active_field = 4;
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('1')));
        app.handle_key(key(KeyCode::Esc));

        // Edit SWI HP Sigma Z (field 5)
        app.active_field = 5;
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('2')));
        app.handle_key(key(KeyCode::Esc));

        // Edit SWI MIP Window (field 6)
        app.active_field = 6;
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('3')));
        app.handle_key(key(KeyCode::Esc));
    }

    // ========== Pipeline: switch algorithms and verify row changes ==========

    #[test]
    fn test_pipeline_bf_algorithm_params() {
        let mut ps = PipelineFormState::default();
        // Switch through every BG removal algorithm (incl. BFRnet/iQFM) and verify rows change.
        for algo in 0..BF_OPTIONS.len() {
            ps.bf_algorithm = algo;
            let rows = ps.visible_rows();
            assert!(!rows.is_empty());
        }
    }

    #[test]
    fn test_pipeline_qsm_algorithm_all_params() {
        let mut ps = PipelineFormState::default();
        // Switch through EVERY QSM algorithm (incl. the deep-learning ones: single-step
        // NeXtQSM/AutoQSM hide BG removal; end-to-end iQSM/iQSM+ hide field mapping too).
        for algo in 0..QSM_ALGO_OPTIONS.len() {
            ps.qsm_algorithm = algo;
            let rows = ps.visible_rows();
            assert!(!rows.is_empty());
            let focusable = ps.focusable_rows();
            assert!(!focusable.is_empty());
        }
    }

    #[test]
    fn test_pipeline_separation_algorithm_all_params() {
        let mut ps = PipelineFormState { do_chi_separation: true, ..Default::default() };
        // Every separation method incl. SUSEP-Net / χ-sepnet.
        for algo in 0..SEP_ALGO_OPTIONS.len() {
            ps.separation_algorithm = algo;
            assert!(!ps.visible_rows().is_empty());
        }
    }

    #[test]
    fn test_pipeline_medi_smv_hides_bg_removal() {
        let ps = PipelineFormState {
            qsm_algorithm: 7,
            medi_smv: true,
            ..Default::default()
        };
        let rows = ps.visible_rows();
        // BG Removal should not be in rows
        let has_bg = rows.iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "bf_algorithm", .. }));
        assert!(!has_bg, "MEDI+SMV should hide BG removal");
    }

    // ========== NIfTI: focusable_items with files ==========

    #[test]
    fn test_nifti_focusable_items_with_files() {
        let ns = NiftiState {
            magnitude_files: vec![
                std::path::PathBuf::from("/a.nii"),
                std::path::PathBuf::from("/b.nii"),
            ],
            phase_files: vec![
                std::path::PathBuf::from("/p.nii"),
            ],
            ..Default::default()
        };
        let items = ns.focusable_items();
        // AddMagnitude, MagFile(0), MagFile(1), AddPhase, PhaseFile(0),
        // EchoTimes, FieldStrength, B0Direction, ConvertButton = 9
        assert_eq!(items.len(), 9);
    }

    // ========== Pipeline threshold value editing ==========

    #[test]
    fn test_pipeline_threshold_value_editing() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        // Set generator to Fixed threshold to make MaskOpThresholdValue visible
        app.pipeline_state.mask_sections[0].generator = crate::pipeline::config::MaskOp::Threshold {
            method: crate::pipeline::config::MaskThresholdMethod::Fixed,
            value: Some(0.5),
        };
        app.pipeline_state.mark_mask_custom();

        // Find MaskOpThresholdValue focus
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut tv_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if matches!(&rows[ri], PipelineRow::MaskOpThresholdValue { .. }) {
                tv_focus = Some(fi);
                break;
            }
        }
        let fi = tv_focus.expect("MaskOpThresholdValue not found");
        app.pipeline_state.focus = fi;
        // Enter starts editing
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.mask_threshold_editing);
        // Type a new value -- clear via backspace from end
        let buf_len = app.pipeline_state.mask_threshold_value_buf.len();
        for _ in 0..buf_len {
            app.handle_key(key(KeyCode::Backspace));
        }
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Char('.')));
        app.handle_key(key(KeyCode::Char('7')));
        // Cursor movement
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Right));
        // Enter saves
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.mask_threshold_editing);
        if let crate::pipeline::config::MaskOp::Threshold { value, .. } = &app.pipeline_state.mask_sections[0].generator {
            assert!((value.unwrap() - 0.7).abs() < 0.001);
        }
    }

    #[test]
    fn test_pipeline_threshold_value_esc_cancels() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.pipeline_state.collapsed_sections.clear();
        app.pipeline_state.mask_sections[0].generator = crate::pipeline::config::MaskOp::Threshold {
            method: crate::pipeline::config::MaskThresholdMethod::Fixed,
            value: Some(0.5),
        };
        app.pipeline_state.mark_mask_custom();
        let rows = app.pipeline_state.visible_rows();
        let focusable = app.pipeline_state.focusable_rows();
        let mut tv_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if matches!(&rows[ri], PipelineRow::MaskOpThresholdValue { .. }) {
                tv_focus = Some(fi);
                break;
            }
        }
        let fi = tv_focus.expect("MaskOpThresholdValue not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.mask_threshold_editing);
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.mask_threshold_editing);
        // Value should be unchanged
        if let crate::pipeline::config::MaskOp::Threshold { value, .. } = &app.pipeline_state.mask_sections[0].generator {
            assert!((value.unwrap() - 0.5).abs() < 0.001);
        }
    }

    // ========== get_include_exclude tests ==========

    #[test]
    fn test_filter_get_include_exclude_pattern_mode() {
        let fs = FilterTreeState {
            include_pattern: "sub-1*".to_string(),
            exclude_pattern: "sub-2*".to_string(),
            ..Default::default()
        };
        let (inc, exc) = fs.get_include_exclude();
        assert_eq!(inc, Some(vec!["sub-1*".to_string()]));
        assert_eq!(exc, Some(vec!["sub-2*".to_string()]));
    }

    #[test]
    fn test_filter_get_include_exclude_star_only() {
        let fs = FilterTreeState::default(); // include_pattern = "*"
        let (inc, exc) = fs.get_include_exclude();
        assert!(inc.is_none());
        assert!(exc.is_none());
    }
}
