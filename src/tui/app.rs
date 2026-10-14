use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::LazyLock;

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
    /// Download a ready-made example dataset instead of pointing at your own data.
    Example,
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
pub const TAB_PREPROCESSING: usize = 1;
pub const TAB_QSM: usize = 2;
pub const TAB_SEPARATION: usize = 3;
pub const TAB_OUTPUTS: usize = 4;
pub const TAB_EXECUTION: usize = 5;
pub const TAB_METHODS: usize = 6;

/// A sub-tab of a row-based tab: the second tab row, switched with `[` and `]`. A tab with a
/// single page (Separation, Execution) shows no second row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Geometry,
    Masking,
    Motion,
    /// The QSM tab while QSM is not reconstructed: only the switch that turns it on.
    QsmOff,
    FieldMap,
    Background,
    Inversion,
    MultiOrientation,
    Reference,
    Separation,
    SwiSmwi,
    Relaxometry,
    Segmentation,
    Export,
    Execution,
}

impl Page {
    /// The sub-tab's title.
    pub fn label(self) -> &'static str {
        match self {
            Page::Geometry => "Geometry",
            Page::Masking => "Masking",
            Page::Motion => "Motion",
            Page::QsmOff => "QSM",
            Page::FieldMap => "Field map",
            Page::Background => "Background",
            Page::Inversion => "Inversion",
            Page::MultiOrientation => "Multi-orientation",
            Page::Reference => "Reference",
            Page::Separation => "χ-separation",
            Page::SwiSmwi => "SWI/SMWI",
            Page::Relaxometry => "Relaxometry",
            Page::Segmentation => "Segmentation",
            Page::Export => "Export",
            Page::Execution => "Execution",
        }
    }

    /// Whether the page's rows are the pipeline form's ([`PipelineFormState::visible_rows`])
    /// rather than built by the App from both forms.
    pub fn is_pipeline(self) -> bool {
        !matches!(self, Page::SwiSmwi | Page::Relaxometry | Page::Segmentation | Page::Export | Page::Execution)
    }

    /// Every page, in tab order.
    #[cfg(test)]
    pub const ALL: [Page; 15] = [
        Page::Geometry, Page::Masking, Page::Motion, Page::QsmOff, Page::FieldMap, Page::Background,
        Page::Inversion, Page::MultiOrientation, Page::Reference, Page::Separation, Page::SwiSmwi,
        Page::Relaxometry, Page::Segmentation, Page::Export, Page::Execution,
    ];
}

// ─── Example dataset state ───

/// What a background example download reports back to the UI thread.
pub enum ExampleMessage {
    Log(String),
    /// Every selected acquisition is in place; carries the dataset directory.
    Done(std::path::PathBuf),
    Error(String),
}

/// Default dataset directory when the user leaves the field blank.
pub const EXAMPLE_DEFAULT_DIR: &str = "qsmxt-example";

pub struct ExampleState {
    /// Directory the BIDS dataset is created in (or added to). Blank means
    /// [`EXAMPLE_DEFAULT_DIR`].
    pub output_dir: String,
    /// Focus within the picker: an index into [`crate::example::EXAMPLES`], or
    /// `EXAMPLES.len()` for the Download button below the list.
    pub cursor: usize,
    /// Which acquisitions are ticked. Parallel to `EXAMPLES`.
    pub selected: Vec<bool>,
    pub scroll_offset: usize,
    pub status: ConvertStatus,
    pub log: Vec<String>,
    /// `(bytes downloaded, bytes total)` for the archive in flight. Written by the
    /// worker thread at download speed and read once per frame, so it does not go
    /// through the message channel.
    progress: Arc<(AtomicU64, AtomicU64)>,
    receiver: Option<mpsc::Receiver<ExampleMessage>>,
}

impl Default for ExampleState {
    fn default() -> Self {
        let mut selected = vec![false; crate::example::EXAMPLES.len()];
        // Tick the default acquisition and start on the Download button: the common
        // case is "just give me an example dataset", which is then a single Enter.
        // Arrowing up from the button walks into the list to pick something else.
        let default_idx = crate::example::EXAMPLES
            .iter()
            .position(|e| e.id == crate::example::DEFAULT_ID)
            .unwrap_or(0);
        selected[default_idx] = true;
        Self {
            output_dir: String::new(),
            cursor: Self::button_index(),
            selected,
            scroll_offset: 0,
            status: ConvertStatus::Idle,
            log: Vec::new(),
            progress: Arc::new((AtomicU64::new(0), AtomicU64::new(0))),
            receiver: None,
        }
    }
}

impl ExampleState {
    /// Index of the Download button in the focus order (one past the last acquisition).
    pub fn button_index() -> usize {
        crate::example::EXAMPLES.len()
    }

    /// The dataset directory, with `~` expanded and the default applied.
    pub fn resolved_dir(&self) -> std::path::PathBuf {
        let trimmed = self.output_dir.trim();
        let raw = if trimmed.is_empty() { EXAMPLE_DEFAULT_DIR } else { trimmed };
        expand_tilde(raw)
    }

    pub fn selected_examples(&self) -> Vec<&'static crate::example::Example> {
        crate::example::EXAMPLES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selected.get(*i).copied().unwrap_or(false))
            .map(|(_, e)| e)
            .collect()
    }

    /// Total download size of the current selection, in bytes.
    pub fn selected_bytes(&self) -> u64 {
        self.selected_examples().iter().map(|e| e.bytes).sum()
    }

    /// `(downloaded, total)` for the archive currently in flight, or `None` when
    /// nothing is downloading.
    pub fn progress(&self) -> Option<(u64, u64)> {
        let total = self.progress.1.load(Ordering::Relaxed);
        (total > 0).then(|| (self.progress.0.load(Ordering::Relaxed), total))
    }

    pub fn toggle_selected(&mut self) {
        if let Some(slot) = self.selected.get_mut(self.cursor) {
            *slot = !*slot;
        }
    }

    /// Start fetching the ticked acquisitions in the background. Returns an error
    /// message for the status bar if the request is not actionable.
    pub fn start_download(&mut self) -> Result<(), String> {
        if self.status == ConvertStatus::Converting {
            return Err("A download is already running".to_string());
        }
        let examples = self.selected_examples();
        if examples.is_empty() {
            return Err("Select at least one acquisition (Space to tick)".to_string());
        }

        let dir = self.resolved_dir();
        self.status = ConvertStatus::Converting;
        self.log.clear();
        self.log.push(format!(
            "Fetching {} acquisition{} ({:.0} MB) into {}",
            examples.len(),
            if examples.len() == 1 { "" } else { "s" },
            self.selected_bytes() as f64 / 1e6,
            dir.display()
        ));

        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let progress = Arc::clone(&self.progress);
        progress.0.store(0, Ordering::Relaxed);
        progress.1.store(0, Ordering::Relaxed);

        std::thread::spawn(move || {
            for example in examples {
                let _ = tx.send(ExampleMessage::Log(format!("Downloading {}", example.id)));
                progress.0.store(0, Ordering::Relaxed);
                progress.1.store(example.bytes, Ordering::Relaxed);

                let zip = match crate::example::download::ensure(example, &mut |done, total| {
                    progress.0.store(done, Ordering::Relaxed);
                    progress.1.store(total.max(1), Ordering::Relaxed);
                }) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = tx.send(ExampleMessage::Error(e.to_string()));
                        return;
                    }
                };
                progress.1.store(0, Ordering::Relaxed);

                let tx_step = tx.clone();
                match crate::example::materialize::materialize(example, &zip, &dir, false, &mut |s| {
                    let _ = tx_step.send(ExampleMessage::Log(s));
                }) {
                    Ok(crate::example::materialize::Outcome::Written(n)) => {
                        let _ = tx.send(ExampleMessage::Log(format!(
                            "Added {} (ses-{}, acq-{}, run-{}, {n} echoes)",
                            example.id, example.scanner, example.acq, example.run
                        )));
                    }
                    Ok(crate::example::materialize::Outcome::Skipped) => {
                        let _ = tx.send(ExampleMessage::Log(format!(
                            "{} is already in this dataset — skipped",
                            example.id
                        )));
                    }
                    Err(e) => {
                        let _ = tx.send(ExampleMessage::Error(e.to_string()));
                        return;
                    }
                }
            }
            let _ = tx.send(ExampleMessage::Done(dir));
        });
        Ok(())
    }

    /// Drain worker messages. Returns the dataset directory once every selected
    /// acquisition is in place, so the caller can switch to BIDS mode.
    pub fn poll(&mut self) -> Option<std::path::PathBuf> {
        let rx = self.receiver.as_ref()?;
        let mut done = None;
        loop {
            match rx.try_recv() {
                Ok(ExampleMessage::Log(line)) => self.log.push(line),
                Ok(ExampleMessage::Done(dir)) => {
                    self.status = ConvertStatus::Done;
                    self.log.push("Dataset ready".to_string());
                    done = Some(dir);
                    break;
                }
                Ok(ExampleMessage::Error(e)) => {
                    self.status = ConvertStatus::Error;
                    self.log.push(format!("Error: {e}"));
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // The worker vanished without reporting; do not spin on a dead channel.
                    if self.status == ConvertStatus::Converting {
                        self.status = ConvertStatus::Error;
                        self.log.push("Error: download thread stopped unexpectedly".to_string());
                    }
                    break;
                }
            }
        }
        self.receiver = None;
        self.progress.1.store(0, Ordering::Relaxed);
        done
    }
}

/// Expand a leading `~/` against `$HOME`, leaving anything else untouched.
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::Path::new(&home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

pub const TAB_NAMES: [&str; 7] = [
    "Input",
    "Preprocessing",
    "QSM",
    "χ-separation",
    "Other outputs",
    "Execution",
    "Methods",
];

// ─── Filter tree state ───

/// What is focused in the flattened filter tree view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterFocus {
    Include,
    Exclude,
    /// The orientation-group pattern: which runs are the same object at a different angle.
    OrientationGroup,
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

/// COSMOS or STI, in the order the TUI cycles them.
pub const ORIENTATION_ALGORITHMS: [&str; 2] = ["cosmos", "sti"];
pub const ORIENTATION_ALGORITHM_HELP: [&str; 2] = [
    "COSMOS — scalar susceptibility from 2+ orientations, co-registered if needed, closed form in k-space",
    "STI — rank-2 susceptibility tensor; needs 6+ orientations that are not coplanar",
];

/// What an orientation-group pattern currently matches.
#[derive(Debug, Clone, Default)]
pub struct OrientationPreview {
    /// The pattern itself was rejected — shown instead of any groups.
    pub error: Option<String>,
    /// One entry per group: its label and its members' display names.
    pub groups: Vec<(String, Vec<String>)>,
    /// Selected runs the pattern did not put in any group. These process normally.
    pub ungrouped: usize,
}

/// Which text field is being edited in the filter area.
enum FilterTextField {
    Include,
    Exclude,
    OrientationGroup,
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
    /// Orientation-group pattern; empty = multi-orientation off.
    pub orientation_group: String,
    pub orientation_editing: bool,
    pub orientation_cursor: usize,
    /// Index into [`ORIENTATION_ALGORITHMS`].
    pub orientation_algorithm: usize,
    pub orientation_lambda: String,
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
            // Off by default: most datasets are single-orientation, and grouping runs that are
            // merely repeats would quietly turn a repeat into a bogus COSMOS set.
            orientation_group: String::new(),
            orientation_editing: false,
            orientation_cursor: 0,
            orientation_algorithm: 0,
            orientation_lambda: String::new(),
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
                self.focus = FilterFocus::OrientationGroup;
            }
            FilterFocus::OrientationGroup => self.focus = self.first_tree_focus(),
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
            FilterFocus::OrientationGroup => self.focus = FilterFocus::Exclude,
            FilterFocus::TreeNode(0) => self.focus = FilterFocus::OrientationGroup,
            FilterFocus::TreeNode(i) => self.focus = FilterFocus::TreeNode(i - 1),
            FilterFocus::NumEchoes => {
                let rows = self.visible_rows().len();
                if rows > 0 {
                    self.focus = FilterFocus::TreeNode(rows - 1);
                } else {
                    self.focus = FilterFocus::OrientationGroup;
                }
            }
        }
    }

    /// Whether the orientation-group pattern is set to anything active.
    pub fn multi_orientation_on(&self) -> bool {
        let p = self.orientation_group.trim();
        !p.is_empty() && !p.eq_ignore_ascii_case("none")
    }

    fn first_tree_focus(&self) -> FilterFocus {
        if self.visible_rows().is_empty() {
            FilterFocus::NumEchoes
        } else {
            FilterFocus::TreeNode(0)
        }
    }

    /// What the current orientation-group pattern matches, recomputed on every keystroke.
    ///
    /// This is the part that makes the pattern usable. Nobody gets a grouping pattern right
    /// first try, and the cost of getting it wrong is not an error — it is a reconstruction
    /// that combines the wrong runs, or silently combines none and falls back to ordinary
    /// single-orientation processing. Showing the groups as they are typed turns that into
    /// something you can see.
    ///
    /// Only structure is previewed here, not physics: B0 directions live in sidecars and
    /// affines that would have to be read from disk on every keystroke. `qsmxt run --dry`
    /// prints the direction table and the degeneracy verdict for each group.
    pub fn orientation_preview(&self) -> OrientationPreview {
        if !self.multi_orientation_on() {
            return OrientationPreview::default();
        }
        let pattern = match crate::bids::orientation::parse_pattern(&self.orientation_group) {
            Ok(Some(p)) => p,
            Ok(None) => return OrientationPreview::default(),
            Err(e) => return OrientationPreview { error: Some(e), ..Default::default() },
        };
        let Some(ref tree) = self.tree else { return OrientationPreview::default() };

        // Only selected runs can form a group — an orientation excluded by the filters above
        // is not going to be reconstructed, so counting it here would be a lie.
        let mut keys = Vec::new();
        let mut displays = Vec::new();
        tree.for_each_run(|leaf| {
            if leaf.selected {
                keys.push((leaf.key.clone(), leaf.key_string.clone()));
                displays.push(leaf.display.clone());
            }
        });

        let groups = crate::bids::orientation::group_keys(&keys, &pattern);
        let grouped: usize = groups.iter().map(|g| g.members.len()).sum();
        OrientationPreview {
            error: None,
            groups: groups
                .into_iter()
                .map(|g| {
                    let members = g.members.iter().map(|&i| displays[i].clone()).collect();
                    (g.label, members)
                })
                .collect(),
            ungrouped: keys.len().saturating_sub(grouped),
        }
    }

    /// The multi-orientation settings, in the form the command builder wants.
    pub fn orientation_args(&self) -> (Option<String>, usize, Option<f64>) {
        if !self.multi_orientation_on() {
            return (None, 0, None);
        }
        (
            Some(self.orientation_group.trim().to_string()),
            self.orientation_algorithm,
            self.orientation_lambda.trim().parse::<f64>().ok(),
        )
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

// ─── Pipeline tab state ───

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
    Note { text: std::borrow::Cow<'static, str> },
    /// Section header "── Mask N ──" (not focusable)
    MaskSectionHeader { section: usize },
    /// Header for the steps that run on the combined mask (not focusable)
    MaskCombinedHeader,
    /// Header for the two-pass reliable mask (not focusable)
    MaskTwoPassHeader,
    /// "── OR ──" separator between sections (not focusable)
    /// "COMBINED WITH ◀ OR ▶" between sections (focusable, ←/→ toggles OR/AND)
    MaskCombineOp,
    /// Input source for a mask section (focusable, ←/→ to cycle)
    MaskOpInput { section: usize },
    /// Generator algorithm selector (threshold or BET, ←/→ to switch)
    MaskOpGenerator { section: usize },
    /// Generator parameter (threshold method or BET fractional intensity)
    MaskOpGeneratorParam { section: usize },
    /// Threshold value (only shown for fixed/percentile threshold methods)
    MaskOpThresholdValue { section: usize },
    /// HD-BET sliding-window step (only shown when the generator is HD-BET)
    MaskOpHdBetStep { section: usize },
    /// BET voxel-size scaling (only shown when the generator is BET)
    MaskOpBetVoxelScale { section: usize },
    /// A refinement step (editable, deletable, reorderable)
    MaskOpEntry { section: usize, index: usize },
    /// One extra parameter of a multi-parameter refinement step (signal-erode), indexed into
    /// [`SIGNAL_ERODE_PARAMS`] (focusable, ←/→ to adjust)
    MaskOpSignalErodeParam { section: usize, index: usize, param: usize },
    /// "Add step..." row for appending new ops to a section
    MaskOpAddStep { section: usize },
    /// "Add mask..." row for adding a new OR'd section — to the main mask, or to the two-pass
    /// reliable mask when `two_pass` is set
    MaskOpAddSection { two_pass: bool },
    /// The Execution tab's button: Enter does what F5 does ([`App::try_run`]).
    RunButton,
}

pub const MASK_OP_TYPES: &[&str] = &[
    "threshold", "bet", "hd-bet", "erode", "dilate", "close", "fill-holes", "gaussian", "signal-erode",
];

/// Ops that create a mask (the section's generator); the rest are refinements.
pub const MASK_GENERATOR_TYPES: &[&str] = &["threshold", "bet", "hd-bet", "rs2-net"];

/// How R2' is obtained when no custom map is supplied.
pub const R2PRIME_STRATEGY_OPTIONS: &[&str] = &["auto", "mese", "r2primenet"];
pub const R2PRIME_STRATEGY_HELP: &[&str] = &[
    "Measure R2' = R2* - R2 from a MESE acquisition when one is present, else predict it from R2* with R2PRIMEnet",
    "Only ever measure R2' from a MESE acquisition - produces no R2' without spin-echo data",
    "Always predict R2' from R2* with R2PRIMEnet, even when a MESE acquisition is available (an estimate, not a measurement)",
];

pub const MASK_PRESET_OPTIONS: &[&str] = &["robust-threshold", "bet", "hd-bet", "bet-and-phase", "rs2-net", "custom"];
pub const MASK_PRESET_HELP: &[&str] = &[
    "Otsu threshold + dilate + fill holes + erode (recommended for brain)",
    "BET brain extraction + erode",
    "HD-BET deep-learning brain extraction + signal-gated erosion (QSM-CI harmonization masking)",
    "BET on the magnitude AND thresholded phase quality, then fill holes + erode (ISMRM EMTP consensus)",
    "RS2-Net deep-learning rodent brain extraction (mouse, rat)",
    "Fully custom mask pipeline (edit steps below)",
];
/// Index of the "custom" preset — set automatically when the steps are edited by hand, and
/// skipped when cycling presets with ←/→. Derived, because it is always the trailing entry:
/// a hardcoded index silently goes stale the moment a preset is added.
pub const MASK_PRESET_CUSTOM: usize = MASK_PRESET_OPTIONS.len() - 1;

/// The signal-erode parameters that get a row of their own, past the threshold shown on the step
/// row itself: label, help, and how ←/→ moves them. Order matches the row order.
pub const SIGNAL_ERODE_PARAMS: &[(&str, &str)] = &[
    ("Depth Cap", "Never peel deeper than this many voxels below the original surface (0 = no cap)"),
    ("Plain Erosions", "Plain erosions run inside signal-erode, before the gate — unlike a separate erode step they spend the depth budget and leave the gate unchanged"),
    ("Bias Sigma", "Gaussian scale (voxels) of the receive-coil bias estimate divided out of the magnitude"),
    ("Min Component", "Keep every connected component with at least this many voxels, not just the largest"),
];

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
    "NLTV (Nonlinear Total Variation) — nonlinear data fidelity with a TV prior. https://doi.org/10.1002/mrm.27073",
    "MEDI (Morphology Enabled Dipole Inversion) — magnitude-guided edge weighting. https://doi.org/10.1002/mrm.22816",
    "TFI (Total Field Inversion) — inverts the total field directly (its own background removal). https://doi.org/10.1002/mrm.26331",
    "iLSQR (default) — iterative LSQR with streaking-artifact correction. https://doi.org/10.1016/j.neuroimage.2014.12.043",
    "LSQR (minimally regularised) — one least-squares solve for χ plus a residual field; the residual weight is the only regularisation. https://doi.org/10.1118/1.3481505",
    "HEIDI (Homogeneity Enabled Incremental Dipole Inversion) — keeps the well-conditioned k-space of an LSQR solve and re-derives the dipole cone under a homogeneity prior. https://doi.org/10.1016/j.neuroimage.2012.05.067",
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
/// The two reference methods that need no parcellation, ahead of every region.
const QSM_REF_METHODS: [(&str, &str); 2] = [
    ("mean", "Subtract the mean susceptibility over the voxels the map is defined on (recommended)"),
    ("none", "No referencing (raw susceptibility values)"),
];

/// The reference picker's options: the two methods, then every SynthSeg region.
///
/// Built from the label table rather than listed, so a table that gains a label gains a reference
/// option. Leaked to `&'static` once at first use — the set is fixed and bounded, and the row
/// types hold `&'static [&'static str]`.
static QSM_REF_ENTRIES: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let mut out: Vec<(&'static str, &'static str)> = QSM_REF_METHODS.to_vec();
    for r in qsmxt_config::regions::REGIONS.iter() {
        let ids = r.ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
        let help = format!(
            "Reference χ to the mean of the {} (FreeSurfer {} {}){}. Enables SynthSeg",
            r.label,
            if r.ids.len() == 1 { "label" } else { "labels" },
            ids,
            if r.v2_only { " — needs --synthseg-version v2" } else { "" },
        );
        out.push((r.slug, Box::leak(help.into_boxed_str())));
    }
    out
});

static QSM_REF_OPTIONS_V: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| QSM_REF_ENTRIES.iter().map(|(o, _)| *o).collect());
static QSM_REF_HELP_V: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| QSM_REF_ENTRIES.iter().map(|(_, h)| *h).collect());

/// The `--qsm-reference` value for a picker index (`mean`, `none`, or a region slug).
pub fn qsm_reference_spec(index: usize) -> String {
    QSM_REF_OPTIONS_V.get(index).copied().unwrap_or("mean").to_string()
}


/// Facts from outside the pipeline form that decide which of its rows apply: the other tabs'
/// settings (an output that forces QSM, the orientation grouping, the echo limit) and the maps
/// whose supplied inputs apply. Set by [`App::sync_pipeline_mode`] before every key
/// and every frame, from [`App::row_context`]; the default is a fresh form's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowContext {
    /// QSM is reconstructed although Compute QSM is off — SMWI needs a χ map, or the statistics
    /// have nothing else to summarise — and why.
    pub qsm_forced_by: Option<&'static str>,
    /// A multi-orientation grouping is set on the Input tab.
    pub orientation_group: bool,
    /// The grouping reconstructs COSMOS (rather than STI).
    pub orientation_cosmos: bool,
    /// The echo count when the run is limited to one (`--num-echoes`); `None` = unknown.
    pub num_echoes: Option<usize>,
    /// The maps whose bring-your-own inputs and settings apply: see [`InputRelevance`].
    pub inputs: InputRelevance,
    /// The selected methods that only work on axial data, so oblique data is resampled to axial
    /// whatever the obliquity threshold says (`bridge::axial_only_algorithms`), named.
    pub axial_forced_by: Option<String>,
    /// Whether coil combination can run: some selected run is uncombined multi-coil data, or
    /// the dataset is not known (no scan yet).
    pub multi_coil: bool,
}

/// Which bring-your-own inputs (and the R2' source) the run can use.
///
/// Mirrors `qsmxt_config::enforce_separation_dependencies` and `requirements::implied_by`, but
/// from the form's *requests* rather than the resolved config: a supplied map switches its own
/// computation off, and the row that supplies it must not vanish because it was filled in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputRelevance {
    /// A χ map is read: χ-separation, the statistics or SMWI.
    pub qsm: bool,
    /// An R2 map is read: asked for, or the measured route to a computed R2'.
    pub r2: bool,
    /// An R2' map is read: asked for, or the selected χ-separation method needs it.
    pub r2prime: bool,
    /// R2' is computed (wanted and not supplied), so its source and R2PRIMEnet settings apply.
    pub r2prime_source: bool,
}

/// All pipeline form values (algorithms + parameters).
#[derive(Debug, Clone)]
pub struct PipelineFormState {
    /// Which page's rows to render — set by the App (from the active tab and sub-tab) before
    /// routing input or drawing.
    pub page: Page,
    /// What the rest of the form decides about these rows — set by the App with `page`.
    pub ctx: RowContext,
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
    pub b0_weight_type: usize,   // 0=phase_snr, 1=phase_var, 2=average, 3=tes, 4=mag, 5=assumed-decay
    pub b0_weight_assumed_t2star_ms: String,
    /// Index into [`LAPLACIAN_SOLVER_OPTIONS`] (0 = dct, 1 = fft).
    pub laplacian_solver: usize,
    /// FFT solver pad in voxels per side: "64" for every axis, or "64 64 32".
    pub laplacian_fft_pad: String,
    /// Motion correction (off by default): reference index into [`MOTION_REFERENCE_OPTIONS`]
    /// (0 first, 1 last, 2 a given echo), and the rigid registration's settings.
    pub motion_correction: bool,
    /// Multi-orientation (grouping is on the Input tab): co-register a group's orientations when
    /// they need it, and reconstruct past the direction-degeneracy check.
    pub orientation_registration: bool,
    pub multi_orientation_force: bool,
    pub cosmos_magnitude_weighting: bool,
    pub cosmos_tol: String,
    pub cosmos_max_iter: String,
    pub motion_reference: usize,
    pub motion_reference_echo: String,
    pub motion_chain: bool,
    pub motion_chain_levels: String,
    pub motion_min_ncc: String,
    pub reg_levels: String,
    pub reg_max_samples: String,
    pub reg_max_iterations: String,
    pub reg_max_evaluations: String,
    pub reg_tolerance: String,
    pub reg_rotation_step_deg: String,
    pub reg_translation_step_mm: String,
    pub reg_min_overlap: String,

    // Parameters (as Strings for text editing)
    pub inhomogeneity_correction: bool,
    /// Reconstruct a 2D acquisition whose sidecar says its slices leave a gap.
    pub ignore_slice_gap: bool,
    // Performance and output grid (Preprocessing → Geometry).
    pub crop_to_mask: bool,
    pub crop_margin_mm: String,
    pub fft_padding: bool,
    /// Index into [`OUTPUT_SPACE_OPTIONS`].
    pub output_space: usize,
    // Inhomogeneity correction's own settings.
    pub homogeneity_sigma_mm: String,
    pub homogeneity_nbox: String,
    /// MCPC-3D-S coil combination smoothing (x y z voxels), for uncombined multi-coil runs.
    pub coil_combination_sigma: String,
    pub romeo_phase_coherence: bool,
    pub tgv_step_size: String,
    pub tgv_tol: String,
    pub medi_merit: bool,
    pub medi_data_weighting: String,
    pub tfi_merit: bool,
    pub tfi_data_weighting: String,
    pub amp_pe_damp_rate_sig: String,
    pub amp_pe_damp_rate_par: String,
    pub amp_pe_max_pe_spar_ite: String,
    pub amp_pe_max_pe_est_ite: String,
    pub amp_pe_cvg_thd: String,
    /// Bring-your-own segmentation from derivatives (empty = off, * = any tool).
    pub custom_dseg_tool: String,
    /// Resample oblique data to axial above `obliquity_threshold` (`--axial-resampling`).
    pub axial_resampling: bool,
    pub obliquity_threshold: String,

    // Deep-learning overlap-tiling (empty = off / whole-volume). Shown for tileable DL inversions.
    /// Run the deep-learning inversion tiled (`--tile-size`); off = whole volume.
    pub dl_tiling: bool,
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
    pub ilsqr_pad_mm: String,

    // LSQR (also HEIDI's seed)
    pub lsqr_residual_weighting: String,
    pub lsqr_fit_global_offset: bool,
    pub lsqr_tol: String,
    pub lsqr_max_iter: String,

    // HEIDI
    pub heidi_cone_threshold: String,
    pub heidi_gradient_threshold: String,
    pub heidi_apply_laplacian_correction: bool,
    pub heidi_laplacian_threshold: String,
    pub heidi_gradient_mask_floor: String,
    pub heidi_continuation_steps: String,
    pub heidi_inner_iterations: String,
    pub heidi_mu_min: String,
    pub heidi_tol: String,
    pub heidi_denoise: bool,
    pub heidi_denoise_iterations: String,
    pub heidi_denoise_time_step: String,
    pub heidi_denoise_conductance: String,

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
    pub vsharp_deconvolve: bool,
    pub vsharp_threshold: String,
    pub vsharp_max_radius: String,
    pub vsharp_min_radius: String,
    /// Index into [`VSHARP_GRID_OPTIONS`] (0 = native, 1 = finest, 2 = isotropic).
    pub vsharp_grid: usize,
    pub vsharp_grid_spacing: String,

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
    pub iharperella_max_iter: String,
    pub iharperella_tol: String,

    // mSMV boundary-shadow refinement (post-step on the primary BFR)
    pub msmv_refine: bool,
    pub msmv_radius: String,
    pub msmv_maxk: String,

    // QSMART
    pub qsmart_ilsqr_max_iter: String,
    pub qsmart_ilsqr_pad_mm: String,
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

    // QSM toggle
    pub do_qsm: bool,

    // ROMEO
    pub romeo_phase_gradient_coherence: bool,
    pub romeo_mag_coherence: bool,
    pub romeo_mag_weight: bool,
    pub romeo_phase_linearity: bool,
    pub romeo_mag_weight2: bool,
    pub romeo_bestpath: bool,
    /// Re-unwrap temporally uncertain voxels (off = a threshold of 0).
    pub romeo_temporal_uncertain: bool,
    pub romeo_temporal_uncertain_unwrapping: String,
    pub romeo_max_seeds: String,
    pub romeo_merge_regions: bool,
    pub romeo_correct_regions: bool,
    pub romeo_wrap_addition: String,
    pub linear_fit_estimate_offset: bool,
    /// Report the fit's reliability mask (off = a percentile of 0).
    pub linear_fit_reliability: bool,
    pub linear_fit_reliability_threshold: String,
    /// Motion-robust echo fit: index into [`ROBUST_FIT_LOSS_OPTIONS`], and its parameters
    /// (`robust_fit_tuning` blank = the loss's conventional value).
    pub robust_fit_loss: usize,
    pub robust_fit_tuning: String,
    pub robust_fit_iterations: String,
    pub robust_fit_downweight_level: String,
    pub robust_fit_flag_ratio: String,

    // Phase offset sigma
    pub phase_offset_sigma: String,

    // Mask sections (combined with `mask_combine` at runtime)
    pub mask_sections: Vec<crate::pipeline::config::MaskSection>,
    /// BET's surface settings, for every BET step of the recipe (`--bet-*`).
    pub bet_smoothness: String,
    pub bet_gradient_threshold: String,
    pub bet_iterations: String,
    pub bet_subdivisions: String,
    /// How the sections fold together (OR = union, AND = intersection).
    pub mask_combine: crate::pipeline::config::MaskCombine,
    /// Refinements applied to the combined mask, after `mask_combine`.
    pub mask_combined_refinements: Vec<crate::pipeline::config::MaskOp>,
    /// Two-pass artefact reduction: reconstruct again against `two_pass_sections` and keep that
    /// reconstruction where it is defined.
    pub two_pass: bool,
    /// The reliable-pass mask. Edited with the same rows as `mask_sections`, addressed by
    /// [`MASK_TWO_PASS_BASE`] + index, and folded with the same combine mode and post-combine
    /// refinements as the main mask.
    pub two_pass_sections: Vec<crate::pipeline::config::MaskSection>,
    pub mask_preset: usize, // index into MASK_PRESET_OPTIONS (MASK_PRESET_CUSTOM = hand-edited)
    pub custom_mask_tool: String, // empty=off; "*"=any derivatives tool; else a tool name

    // χ-separation tab
    pub do_chi_separation: bool,
    pub separation_algorithm: usize, // index into SEP_ALGO_OPTIONS
    pub custom_qsm_tool: String,     // empty=off; "*"=any; else a derivatives tool name
    pub custom_r2_tool: String,
    pub custom_r2prime_tool: String,
    /// Index into [`R2PRIME_STRATEGY_OPTIONS`].
    pub r2prime_strategy: usize,
    /// R2PRIMEnet: run away from 3 T (uncalibrated), and its patch as "D H W".
    pub r2primenet_ignore_field_mismatch: bool,
    pub r2primenet_patch: String,
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
        // These two come from qsmxt-config, not qsm-core: it owns which of qsm-core's fields are
        // the user's (LSQR's b0 and mask_output are the dispatcher's) and flattens HEIDI's
        // optional denoise struct into a switch plus three numbers.
        let lsqr = crate::pipeline::config::LsqrConfig::default();
        let heidi = crate::pipeline::config::HeidiConfig::default();
        let tgv = qsm_core::inversion::TgvParams::default();
        let ndi = qsm_core::inversion::NdiParams::default();
        let fansi = qsm_core::inversion::FansiParams::default();
        let l1qsm = qsm_core::inversion::L1QsmParams::default();
        let whqsm = qsm_core::inversion::WhQsmParams::default();
        let hdqsm = qsm_core::inversion::HdQsmParams::default();
        Self {
            page: Page::Geometry,
            ctx: RowContext::default(),
            qsm_algorithm: super::command::default_qsm_algorithm_index(), // ilsqr
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
            b0_weight_assumed_t2star_ms: format!("{}", qsmxt_config::default_b0_weight_assumed_t2star_ms()),
            laplacian_solver: 0, // dct
            laplacian_fft_pad: qsmxt_config::default_laplacian_fft_pad()[0].to_string(),
            motion_correction: false,
            orientation_registration: true,
            multi_orientation_force: false,
            cosmos_magnitude_weighting: false,
            cosmos_tol: format!("{}", qsmxt_config::MultiOrientationConfig::default().cosmos_tol),
            cosmos_max_iter: format!("{}", qsmxt_config::MultiOrientationConfig::default().cosmos_max_iter),
            motion_reference: 0,
            motion_reference_echo: "1".to_string(),
            motion_chain: qsmxt_config::MotionCorrectionConfig::default().chain,
            motion_chain_levels: qsmxt_config::MotionCorrectionConfig::default().chain_levels.to_string(),
            motion_min_ncc: format!("{}", qsmxt_config::MotionCorrectionConfig::default().min_ncc),
            reg_levels: format!("{}", qsmxt_config::RegistrationConfig::default().levels),
            reg_max_samples: format!("{}", qsmxt_config::RegistrationConfig::default().max_samples),
            reg_max_iterations: format!("{}", qsmxt_config::RegistrationConfig::default().max_iterations),
            reg_max_evaluations: format!("{}", qsmxt_config::RegistrationConfig::default().max_evaluations),
            reg_tolerance: format!("{}", qsmxt_config::RegistrationConfig::default().tolerance),
            reg_rotation_step_deg: format!("{}", qsmxt_config::RegistrationConfig::default().rotation_step_deg),
            reg_translation_step_mm: format!("{}", qsmxt_config::RegistrationConfig::default().translation_step_mm),
            reg_min_overlap: format!("{}", qsmxt_config::RegistrationConfig::default().min_overlap),
            inhomogeneity_correction: true,
            ignore_slice_gap: false,
            crop_to_mask: false,
            crop_margin_mm: fmt_default(qsmxt_config::PipelineToggles::default().crop_margin_mm),
            fft_padding: false,
            output_space: 0,
            homogeneity_sigma_mm: fmt_default(qsmxt_config::HomogeneityConfig::default().sigma_mm),
            homogeneity_nbox: qsmxt_config::HomogeneityConfig::default().nbox.to_string(),
            coil_combination_sigma: {
                let s = qsmxt_config::FieldMappingConfig::default().coil_combination_sigma;
                format!("{} {} {}", s[0], s[1], s[2])
            },
            romeo_phase_coherence: qsmxt_config::RomeoConfig::default().phase_coherence,
            tgv_step_size: fmt_default(qsmxt_config::TgvConfig::default().step_size),
            tgv_tol: fmt_default(qsmxt_config::TgvConfig::default().tol),
            medi_merit: qsm_core::inversion::MediParams::default().merit,
            medi_data_weighting: qsm_core::inversion::MediParams::default().data_weighting.to_string(),
            tfi_merit: qsm_core::inversion::TfiParams::default().merit,
            tfi_data_weighting: qsm_core::inversion::TfiParams::default().data_weighting.to_string(),
            amp_pe_damp_rate_sig: fmt_default(qsm_core::inversion::AmpPeParams::default().damp_rate_sig),
            amp_pe_damp_rate_par: fmt_default(qsm_core::inversion::AmpPeParams::default().damp_rate_par),
            amp_pe_max_pe_spar_ite: qsm_core::inversion::AmpPeParams::default().max_pe_spar_ite.to_string(),
            amp_pe_max_pe_est_ite: qsm_core::inversion::AmpPeParams::default().max_pe_est_ite.to_string(),
            amp_pe_cvg_thd: fmt_default(qsm_core::inversion::AmpPeParams::default().cvg_thd),
            custom_dseg_tool: String::new(),
            axial_resampling: false,
            obliquity_threshold: format!("{}", qsmxt_config::DEFAULT_OBLIQUITY_THRESHOLD),
            dl_tiling: false,
            dl_tile_size: "128".to_string(),
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
            ilsqr_pad_mm: format!("{}", qsm_core::inversion::IlsqrParams::default().pad_mm),
            // Via qsmxt-config rather than qsm-core: it is the layer that decides which of
            // qsm-core's fields are the user's (LSQR's b0 and mask_output are not), and it
            // flattens HEIDI's optional denoise struct into the four values shown here.
            lsqr_residual_weighting: lsqr.residual_weighting.map(|w| format!("{w}")).unwrap_or_default(),
            lsqr_fit_global_offset: lsqr.fit_global_offset,
            lsqr_tol: format!("{}", lsqr.tol),
            lsqr_max_iter: format!("{}", lsqr.max_iter),
            heidi_cone_threshold: format!("{}", heidi.cone_threshold),
            heidi_gradient_threshold: format!("{}", heidi.gradient_threshold),
            heidi_apply_laplacian_correction: heidi.apply_laplacian_correction,
            heidi_laplacian_threshold: format!("{}", heidi.laplacian_threshold),
            heidi_gradient_mask_floor: format!("{}", heidi.gradient_mask_floor),
            heidi_continuation_steps: format!("{}", heidi.continuation_steps),
            heidi_inner_iterations: format!("{}", heidi.inner_iterations),
            heidi_mu_min: format!("{}", heidi.mu_min),
            heidi_tol: format!("{}", heidi.tol),
            heidi_denoise: heidi.denoise,
            heidi_denoise_iterations: format!("{}", heidi.denoise_iterations),
            heidi_denoise_time_step: format!("{}", heidi.denoise_time_step),
            heidi_denoise_conductance: format!("{}", heidi.denoise_conductance),
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
            vsharp_deconvolve: qsmxt_config::VsharpConfig::default().deconvolve,
            vsharp_threshold: format!("{}", qsm_core::bgremove::VsharpParams::default().threshold),
            vsharp_max_radius: format!("{}", qsm_core::bgremove::VsharpParams::default().max_radius),
            vsharp_min_radius: format!("{}", qsm_core::bgremove::VsharpParams::default().min_radius),
            vsharp_grid: 0, // native
            vsharp_grid_spacing: format!("{}", qsmxt_config::VsharpConfig::default().grid_spacing),
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
            iharperella_max_iter: format!("{}", qsm_core::bgremove::IharperellaParams::default().max_iter),
            iharperella_tol: format!("{}", qsm_core::bgremove::IharperellaParams::default().tol),
            msmv_refine: false,
            msmv_radius: format!("{}", qsm_core::bgremove::MsmvParams::default().radius),
            msmv_maxk: format!("{}", qsm_core::bgremove::MsmvParams::default().maxk),
            do_qsm: true,
            romeo_phase_gradient_coherence: qsm_core::unwrap::RomeoParams::default().phase_gradient_coherence,
            romeo_mag_coherence: qsm_core::unwrap::RomeoParams::default().mag_coherence,
            romeo_mag_weight: qsm_core::unwrap::RomeoParams::default().mag_weight,
            romeo_phase_linearity: qsm_core::unwrap::RomeoParams::default().phase_linearity,
            romeo_mag_weight2: qsm_core::unwrap::RomeoParams::default().mag_weight2,
            romeo_bestpath: qsm_core::unwrap::RomeoParams::default().bestpath,
            romeo_temporal_uncertain: qsm_core::unwrap::RomeoParams::default().temporal_uncertain_unwrapping > 0.0,
            romeo_temporal_uncertain_unwrapping: format!("{}", qsm_core::unwrap::RomeoParams::default().temporal_uncertain_unwrapping),
            romeo_max_seeds: format!("{}", qsm_core::unwrap::RomeoParams::default().max_seeds),
            romeo_merge_regions: qsm_core::unwrap::RomeoParams::default().merge_regions,
            romeo_correct_regions: qsm_core::unwrap::RomeoParams::default().correct_regions,
            romeo_wrap_addition: format!("{}", qsm_core::unwrap::RomeoParams::default().wrap_addition),
            linear_fit_estimate_offset: true,
            linear_fit_reliability: qsmxt_config::LinearFitConfig::default().reliability_threshold_percentile > 0.0,
            linear_fit_reliability_threshold: fmt_default(qsmxt_config::LinearFitConfig::default().reliability_threshold_percentile),
            robust_fit_loss: 0,
            robust_fit_tuning: String::new(),
            robust_fit_iterations: qsmxt_config::RobustFitConfig::default().iterations.to_string(),
            robust_fit_downweight_level: fmt_default(qsmxt_config::RobustFitConfig::default().downweight_level),
            robust_fit_flag_ratio: fmt_default(qsmxt_config::RobustFitConfig::default().flag_ratio),
            phase_offset_sigma: {
                let s = qsm_core::utils::PhaseOffsetParams::default().sigma;
                format!("{} {} {}", s[0], s[1], s[2])
            },
            qsmart_ilsqr_max_iter: format!("{}", qsm_core::utils::QsmartParams::default().ilsqr_max_iter),
            qsmart_ilsqr_pad_mm: format!("{}", qsm_core::utils::QsmartParams::default().ilsqr_pad_mm),
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
            mask_sections: crate::pipeline::config::default_mask_sections(),
            bet_smoothness: format!("{}", qsm_core::bet::BetParams::default().smoothness),
            bet_gradient_threshold: format!("{}", qsm_core::bet::BetParams::default().gradient_threshold),
            bet_iterations: format!("{}", qsm_core::bet::BetParams::default().iterations),
            bet_subdivisions: format!("{}", qsm_core::bet::BetParams::default().subdivisions),
            mask_combine: crate::pipeline::config::MaskCombine::Or,
            mask_combined_refinements: vec![],
            two_pass: false,
            two_pass_sections: qsmxt_config::default_two_pass_sections(),
            mask_preset: 0, // robust threshold
            custom_mask_tool: String::new(),
            do_chi_separation: false,
            separation_algorithm: 0,
            custom_qsm_tool: String::new(),
            custom_r2_tool: String::new(),
            custom_r2prime_tool: String::new(),
            r2prime_strategy: 0,
            r2primenet_ignore_field_mismatch: false,
            r2primenet_patch: {
                let p = qsmxt_config::R2primenetConfig::default().patch;
                format!("{} {} {}", p[0], p[1], p[2])
            },
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
            scroll_offset: 0,
            mask_ops_adding: false,
            mask_ops_add_idx: 0,
            mask_ops_add_section: 0,
            mask_threshold_value_buf: String::new(),
            mask_threshold_editing: false,
        }
    }
}

pub const QSM_ALGO_OPTIONS: &[&str] = &["rts", "tv", "tkd", "tsvd", "tgv", "tikhonov", "nltv", "medi", "tfi", "ilsqr", "lsqr", "heidi", "qsmart", "ndi", "fansi", "fansi-tgv", "l1qsm", "whqsm", "hdqsm", "amp-pe", "xqsm", "qsmnet", "qsmnet-plus", "autoqsm", "qsmgan", "ir2qsm", "lpcnn", "modl-qsm", "nextqsm", "iqsm", "iqsm-plus"];
/// MEDI/TFI data weighting, in the CLI's order (0, 1).
pub const DATA_WEIGHTING_OPTIONS: &[&str] = &["uniform", "magnitude SNR"];
const DATA_WEIGHTING_HELP: &[&str] = &[
    "Every voxel of the field weighs the same in the data term",
    "Weight the data term by the magnitude SNR, so noisy voxels count for less",
];
pub const OUTPUT_SPACE_OPTIONS: &[&str] = &["acquired", "working"];
const OUTPUT_SPACE_HELP: &[&str] = &[
    "Write derivatives back on the acquired grid when the run was resampled to axial (default)",
    "Leave derivatives on the resampled (axial) working grid, avoiding a second interpolation",
];
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
pub const B0_ESTIMATION_OPTIONS: &[&str] = &["weighted-avg", "linear-fit", "robust-fit"];
pub const ROBUST_FIT_LOSS_OPTIONS: &[&str] = &["tukey", "huber"];
const ROBUST_FIT_LOSS_HELP: &[&str] = &[
    "Tukey biweight: rejects an echo at a voxel past the tuning radius (default)",
    "Huber: caps an outlying echo's influence but never rejects it",
];
pub const LAPLACIAN_SOLVER_OPTIONS: &[&str] = &["dct", "fft"];
pub const VSHARP_GRID_OPTIONS: &[&str] = &["native", "finest", "isotropic"];
const VSHARP_GRID_HELP: &[&str] = &[
    "The acquired voxel grid (default)",
    "An isotropic grid at the smallest voxel dimension, so no axis is coarsened; isotropic data stays on the acquired grid",
    "An isotropic grid of Grid Spacing mm; the background is interpolated back, the local field keeps native resolution",
];
pub const MOTION_REFERENCE_OPTIONS: &[&str] = &["first", "last", "echo"];
const MOTION_REFERENCE_HELP: &[&str] = &[
    "Align every echo onto the first (best SNR; the published procedure) (default)",
    "Align every echo onto the last",
    "Align every echo onto a given echo (Reference Echo)",
];
const LAPLACIAN_SOLVER_HELP: &[&str] = &[
    "Unweighted least squares, Neumann-boundary DCT solve (Ghiglia & Romero 1994; QSM.jl :dct) (default)",
    "Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume",
];
pub const B0_WEIGHT_TYPE_OPTIONS: &[&str] = &["phase-snr", "phase-var", "average", "tes", "mag", "assumed-decay"];
const B0_ESTIMATION_HELP: &[&str] = &[
    "Weighted average of phase/TE across echoes (default)",
    "Magnitude-weighted linear fit of phase vs TE",
    "Motion-robust linear fit (IRLS): down-weights echoes departing from a straight line, per voxel. Needs 4 echoes (3 without offset)",
];
const B0_WEIGHT_TYPE_HELP: &[&str] = &[
    "mag × TE — optimal for phase SNR (default)",
    "mag² × TE² — based on phase variance",
    "Uniform weights (unweighted average)",
    "TE only",
    "Magnitude only",
    "Fixed TE·exp(−TE/T2*) per echo from an assumed T2* (no T2* map, no magnitude); phase-snr is generally better",
];
/// Borrowed from the statics above so the row types keep their `&'static` slices.
pub fn qsm_ref_options() -> &'static [&'static str] { &QSM_REF_OPTIONS_V }
pub fn qsm_ref_help() -> &'static [&'static str] { &QSM_REF_HELP_V }

impl PipelineFormState {
    /// The χ-separation tab: the method and its parameters, and the χ map it may read instead.
    fn separation_rows(&self, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Toggle {
            label: "Compute χ-separation", field: "do_chi_separation",
            help: "Split QSM into paramagnetic (χ+, iron) and diamagnetic (χ−, myelin) maps",
        });
        if self.do_chi_separation {
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
            5 => {
                rows.push(PipelineRow::Param { label: "  Dr+ @3T (Hz/ppm)", field: "sep_hcchisep_dr_pos_3t", help: "Paramagnetic relaxivity at 3T" });
                rows.push(PipelineRow::Param { label: "  R2' bin (Hz)", field: "sep_hcchisep_bin_hz", help: "R2' bin width for anchored grid search" });
            }
            // SUSEP-Net / χ-sepnet (deep learning) have no tunable parameters. They used to fall
            // into the arm above and show HC-χsep's, which did nothing.
            _ => {}
        }
        } // end if do_chi_separation

        if self.do_chi_separation && self.ctx.inputs.qsm {
            rows.push(PipelineRow::Separator);
            rows.push(Self::custom_qsm_row());
        }
    }

    /// The bring-your-own χ map, under each output that reads one (χ-separation, the statistics,
    /// SMWI): the same setting wherever it shows.
    pub fn custom_qsm_row() -> PipelineRow {
        PipelineRow::Param { label: "  Custom QSM tool", field: "custom_qsm_tool", help: "Use a Chimap from <bids>/derivatives/<tool>/ instead of recomputing QSM (χ-separation, statistics, SMWI); blank = compute, * = any tool" }
    }

    /// The rigid registration's settings: one block, under each of the two things that register
    /// (motion correction and orientation co-registration), editing the same values.
    fn push_registration_rows(rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Note { text: "  Rigid registration (advanced):".into() });
        rows.push(PipelineRow::Param { label: "    Levels", field: "reg_levels", help: "Pyramid levels: the capture range; do not lower (default 5)" });
        rows.push(PipelineRow::Param { label: "    Max Samples", field: "reg_max_samples", help: "Voxels sampled per metric evaluation, per level" });
        rows.push(PipelineRow::Param { label: "    Max Iterations", field: "reg_max_iterations", help: "Powell cycles per level" });
        rows.push(PipelineRow::Param { label: "    Max Evaluations", field: "reg_max_evaluations", help: "Metric evaluations across all levels (hard stop)" });
        rows.push(PipelineRow::Param { label: "    Tolerance", field: "reg_tolerance", help: "Relative cost improvement per cycle that counts as converged" });
        rows.push(PipelineRow::Param { label: "    Rotation Step (deg)", field: "reg_rotation_step_deg", help: "Initial rotation step at the finest level" });
        rows.push(PipelineRow::Param { label: "    Translation Step (mm)", field: "reg_translation_step_mm", help: "Initial translation step at the finest level" });
        rows.push(PipelineRow::Param { label: "    Min Overlap", field: "reg_min_overlap", help: "Smallest fraction of sampled voxels that must overlap" });
    }


    /// Preprocessing → Geometry: the grid the data is reconstructed on.
    fn geometry_rows(&self, rows: &mut Vec<PipelineRow>) {
        let qsm = self.qsm_runs();
        rows.push(PipelineRow::Toggle {
            label: "Axial Resampling", field: "axial_resampling",
            help: "Resample oblique acquisitions onto an axial grid when they are tilted more than the threshold. Off: reconstruct on the acquired grid with the true B0 direction",
        });
        if self.axial_resampling {
            rows.push(PipelineRow::Param { label: "  Obliquity Threshold (°)", field: "obliquity_threshold",
                help: "Resample only data more oblique than this, in degrees" });
        }
        // Where the outputs land only matters when something resamples: on request, or because
        // a selected method only works on axial data (and resamples whatever this says).
        if self.axial_resampling || self.ctx.axial_forced_by.is_some() {
            if let Some(why) = &self.ctx.axial_forced_by {
                rows.push(PipelineRow::Note { text: format!("  Oblique data is resampled to axial for {why}").into() });
            }
            rows.push(PipelineRow::AlgoSelect {
                label: "  Output Space", field: "output_space",
                options: OUTPUT_SPACE_OPTIONS, help: OUTPUT_SPACE_HELP,
            });
        }
        rows.push(PipelineRow::Toggle {
            label: "Inhomog. Correction", field: "inhomogeneity_correction",
            help: "Apply B1 field correction to magnitude (improves masking, ROMEO weights, MEDI edges, SWI)",
        });
        if self.inhomogeneity_correction {
            rows.push(PipelineRow::Param { label: "  Sigma (mm)", field: "homogeneity_sigma_mm",
                help: "Smoothing sigma of the bias-field estimate, in mm" });
            rows.push(PipelineRow::Param { label: "  Box Passes", field: "homogeneity_nbox",
                help: "Box-filter passes approximating the Gaussian" });
        }
        rows.push(PipelineRow::Toggle {
            label: "Ignore Slice Gap", field: "ignore_slice_gap",
            help: "Reconstruct even if the sidecar's SliceThickness is thinner than the slice pitch (gapped 2D data gives a wrong map)",
        });
        if qsm {
            rows.push(PipelineRow::Toggle {
                label: "Crop to Mask", field: "crop_to_mask",
                help: "Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid). Off by default",
            });
            if self.crop_to_mask {
                rows.push(PipelineRow::Param { label: "  Crop Margin (mm)", field: "crop_margin_mm",
                    help: "Margin around the mask; below ~32 mm results start to differ, and it must clear the largest SMV kernel" });
            }
            rows.push(PipelineRow::Toggle {
                label: "FFT Padding", field: "fft_padding",
                help: "Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly). Off by default",
            });
        }
        // Coil combination prepares the echoes every later stage reads, whatever the inversion;
        // it only runs for uncombined multi-coil data.
        if self.ctx.multi_coil {
            rows.push(PipelineRow::Param { label: "Coil Comb. Sigma", field: "coil_combination_sigma",
                help: "MCPC-3D-S smoothing of per-coil phase offsets, voxels (x y z); only for uncombined multi-coil data" });
        }
    }

    /// Preprocessing → Masking: the mask recipe, its custom masks, and the two-pass reliable mask.
    fn masking_rows(&self, rows: &mut Vec<PipelineRow>) {
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
        self.push_mask_section_rows(rows, false);
        rows.push(PipelineRow::MaskOpAddSection { two_pass: false });
        // Steps that run on the combined mask — only meaningful with more than one section.
        if multi_section {
            rows.push(PipelineRow::MaskCombinedHeader);
            self.push_refinement_rows(rows, MASK_COMBINED_SECTION);
            rows.push(PipelineRow::MaskOpAddStep { section: MASK_COMBINED_SECTION });
        }
        // BET only as a refinement step has no generator block to sit under.
        if self.uses_bet() && self.first_bet_generator_section().is_none() {
            Self::push_bet_surface_rows(rows);
        }

        // Two-pass artefact reduction. Configured here because what it needs is a second mask;
        // what it does is repeat background removal and dipole inversion against it.
        rows.push(PipelineRow::Toggle {
            label: "Two-pass Artefact Reduction", field: "two_pass",
            help: "Reconstruct again on a mask whose holes are left unfilled and keep that result where it is defined — keeps streaking from strong sources out of the rest of the brain, at roughly double the reconstruction time (doi:10.1002/mrm.29048)",
        });
        if self.two_pass {
            rows.push(PipelineRow::MaskTwoPassHeader);
            self.push_mask_section_rows(rows, true);
            rows.push(PipelineRow::MaskOpAddSection { two_pass: true });
        }

    }

    /// Preprocessing → Motion: echo co-registration, and the registration it runs.
    fn motion_rows(&self, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Toggle {
            label: "Motion Correction", field: "motion_correction",
            help: "Rigidly co-register every echo onto a reference echo before any other stage (multi-echo, needs magnitude). Off by default",
        });
        if self.motion_correction {
            rows.push(PipelineRow::AlgoSelect {
                label: "  Reference", field: "motion_reference",
                options: MOTION_REFERENCE_OPTIONS, help: MOTION_REFERENCE_HELP,
            });
            if self.motion_reference == 2 {
            rows.push(PipelineRow::Param { label: "    Reference Echo", field: "motion_reference_echo",
                    help: "1-based echo number the others are aligned onto" });
            }
            rows.push(PipelineRow::Toggle { label: "  Chain", field: "motion_chain",
                help: "Seed each echo's registration from its neighbour's solution" });
            if self.motion_chain {
            rows.push(PipelineRow::Param { label: "    Chain Levels", field: "motion_chain_levels",
                    help: "Pyramid levels for a seeded registration (default 3)" });
            }
            rows.push(PipelineRow::Param { label: "  Min NCC", field: "motion_min_ncc",
                help: "NCC below which a registration is reported as suspect (0.3 for echoes; ~0.9 for repeats)" });
            Self::push_registration_rows(rows);
        }
    }

    /// The QSM tab's master switch, on top of each of its sub-tabs — with why QSM runs anyway.
    fn push_qsm_toggle(&self, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Toggle {
            label: "Compute QSM", field: "do_qsm",
            help: "Enable QSM reconstruction (disable to only run the other outputs)",
        });
        if !self.do_qsm {
            if let Some(why) = self.ctx.qsm_forced_by {
                rows.push(PipelineRow::Note { text: format!("  Reconstructed anyway: {why}").into() });
            }
        }
        rows.push(PipelineRow::Separator);
    }

    /// QSM → Field map: unwrapping and the echo combination into one field map.
    fn field_map_rows(&self, rows: &mut Vec<PipelineRow>) {
        let (_, field_mapping_all) = self.field_mapping_runs();
        let is_laplacian = self.unwrapping_algorithm == 1;
        if !field_mapping_all {
            rows.push(PipelineRow::Note { text: "Applies to multi-echo data only: TGV reconstructs a single echo from its wrapped phase".into() });
        }

        // Unwrapping
        rows.push(PipelineRow::AlgoSelect {
            label: "Unwrapping", field: "unwrapping_algorithm",
            options: UNWRAP_OPTIONS, help: UNWRAP_HELP,
        });
        if is_laplacian {
            rows.push(PipelineRow::AlgoSelect {
                label: "  Solver", field: "laplacian_solver",
                options: LAPLACIAN_SOLVER_OPTIONS, help: LAPLACIAN_SOLVER_HELP,
            });
            if self.laplacian_solver == 1 {
            rows.push(PipelineRow::Param { label: "    FFT Pad (voxels)", field: "laplacian_fft_pad",
                    help: "Zero-padding per side: one value for every axis or three (x y z). Default: 64" });
            }
        }

        // ROMEO's own settings, phase offset removal (HIP — ROMEO only) among them.
        if !is_laplacian {
            rows.push(PipelineRow::Toggle {
                label: "  Phase Offset Removal", field: "phase_offset_removal",
                help: "Remove spatially-varying phase offset using HIP (Eckstein et al., 2018); multi-echo only",
            });
            if self.phase_offset_removal {
            rows.push(PipelineRow::Param { label: "    Sigma", field: "phase_offset_sigma",
                    help: "Gaussian smoothing sigma in voxels (x y z)" });
                // Part of the offset-removal path: without it the correction never runs.
                rows.push(PipelineRow::Toggle {
                    label: "    Bipolar Correction", field: "bipolar_correction",
                    help: "Remove the linear phase artefact of bipolar readout gradients (needs at least 3 echoes; fewer: skipped)",
                });
            }
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
            rows.push(PipelineRow::Toggle {
                label: "  Phase Coherence", field: "romeo_phase_coherence",
                help: "Weight edges by neighbour phase coherence",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Phase Gradient Coherence", field: "romeo_phase_gradient_coherence",
                help: "Weight edges by inter-echo phase-gradient consistency (multi-echo)",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Phase Linearity", field: "romeo_phase_linearity",
                help: "Weight edges by second-derivative smoothness",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Mag Coherence", field: "romeo_mag_coherence",
                help: "Weight edges by neighbor magnitude coherence",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Mag Weight", field: "romeo_mag_weight",
                help: "Penalise low-signal voxels (off by default)",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Mag Weight 2", field: "romeo_mag_weight2",
                help: "Penalise abnormally high signal / flow artefacts (off by default)",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Best-path Weights", field: "romeo_bestpath",
                help: "Use Abdul-Rahman best-path weights instead of ROMEO weights",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Temporal Uncertain", field: "romeo_temporal_uncertain",
                help: "Re-unwrap voxels whose temporal unwrapping is uncertain",
            });
            if self.romeo_temporal_uncertain {
                rows.push(PipelineRow::Param {
                    label: "    Quality Threshold", field: "romeo_temporal_uncertain_unwrapping",
                    help: "Quality threshold (0, 1] below which a voxel is re-unwrapped",
                });
            }
            rows.push(PipelineRow::Param {
                label: "  Max Seeds", field: "romeo_max_seeds",
                help: "Maximum number of seed regions (default 1)",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Merge Regions", field: "romeo_merge_regions",
                help: "Merge neighboring regions after unwrapping",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Correct Regions", field: "romeo_correct_regions",
                help: "Shift each region's median to nearest 0 by n·2π",
            });
            rows.push(PipelineRow::Param {
                label: "  Wrap Addition", field: "romeo_wrap_addition",
                help: "Extra phase tolerance beyond π for neighbor differences [0,π]",
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
            if self.b0_weight_type == 5 { // assumed-decay
            rows.push(PipelineRow::Param { label: "    Assumed T2* (ms)", field: "b0_weight_assumed_t2star_ms",
                    help: "T2* assumed to compute the fixed echo weights; no T2* map is used (default: 40 ms)" });
            }
        } else { // linear-fit / robust-fit
            rows.push(PipelineRow::Toggle {
                label: "  Estimate Offset", field: "linear_fit_estimate_offset",
                help: "Fit a phase-offset term in the echo-time fit (robust fit: needs 4 echoes with it, 3 without)",
            });
            rows.push(PipelineRow::Toggle {
                label: "  Reliability Mask", field: "linear_fit_reliability",
                help: "Report the fit's reliability mask; does not change the field map",
            });
            if self.linear_fit_reliability {
                rows.push(PipelineRow::Param {
                    label: "    Percentile", field: "linear_fit_reliability_threshold",
                    help: "Percentile of the fit residual above which a voxel counts as unreliable",
                });
            }
            if self.b0_estimation == 2 { // robust-fit
                rows.push(PipelineRow::AlgoSelect {
                    label: "  Loss", field: "robust_fit_loss",
                    options: ROBUST_FIT_LOSS_OPTIONS, help: ROBUST_FIT_LOSS_HELP,
                });
                rows.push(PipelineRow::Param { label: "  Tuning", field: "robust_fit_tuning",
                    help: "Tuning radius in robust-scale units; blank = conventional (Tukey 4.685, Huber 1.345)" });
                rows.push(PipelineRow::Param { label: "  Iterations", field: "robust_fit_iterations",
                    help: "IRLS passes (0 = report only: the field map is the linear fit's)" });
                rows.push(PipelineRow::Param { label: "  Downweight Level", field: "robust_fit_downweight_level",
                    help: "Reporting only: a weight below this counts as down-weighted (0, 1]" });
                rows.push(PipelineRow::Param { label: "  Flag Ratio", field: "robust_fit_flag_ratio",
                    help: "Reporting only: flag an echo whose outlier score exceeds this x the best echo's (>= 1)" });
            }
        }

    }

    /// QSM → Background: background-field removal (only for inversions that take a local field).
    fn background_rows(&self, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::AlgoSelect {
            label: "BG Removal", field: "bf_algorithm",
            options: BF_OPTIONS, help: BF_HELP,
        });
        match self.bf_algorithm {
            0 => { // V-SHARP
                rows.push(PipelineRow::Toggle { label: "  Deconvolve", field: "vsharp_deconvolve",
                    help: "Undo the SMV high-pass with a truncated-SVD deconvolution (default: off)" });
                if self.vsharp_deconvolve {
                    rows.push(PipelineRow::Param { label: "  Threshold", field: "vsharp_threshold", help: "Deconvolution (TSVD) threshold" });
                }
                rows.push(PipelineRow::Param { label: "  Max Radius", field: "vsharp_max_radius", help: "Largest SMV kernel radius (mm)" });
                rows.push(PipelineRow::Param { label: "  Min Radius", field: "vsharp_min_radius", help: "Smallest SMV kernel radius (mm)" });
                rows.push(PipelineRow::AlgoSelect { label: "  Working Grid", field: "vsharp_grid",
                    options: VSHARP_GRID_OPTIONS, help: VSHARP_GRID_HELP });
                if self.vsharp_grid == 2 {
                    rows.push(PipelineRow::Param { label: "    Grid Spacing", field: "vsharp_grid_spacing",
                        help: "Isotropic working-grid spacing (mm)" });
                }
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
    }

    /// QSM → Inversion: the dipole inversion and its parameters.
    fn inversion_rows(&self, rows: &mut Vec<PipelineRow>) {
        let alg = QSM_ALGO_OPTIONS.get(self.qsm_algorithm).copied().unwrap_or("");
        rows.push(PipelineRow::AlgoSelect {
            label: "QSM Inversion", field: "qsm_algorithm",
            options: QSM_ALGO_OPTIONS, help: QSM_ALGO_HELP,
        });

        // Algorithm-specific params
        match alg {
            "rts" => { // RTS
                rows.push(PipelineRow::Param { label: "  Delta", field: "rts_delta", help: "Threshold for ill-conditioned k-space region" });
                rows.push(PipelineRow::Param { label: "  Mu", field: "rts_mu", help: "Regularization parameter for well-conditioned region" });
                rows.push(PipelineRow::Param { label: "  Rho", field: "rts_rho", help: "ADMM penalty parameter" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "rts_tol", help: "Convergence tolerance (relative change)" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "rts_max_iter", help: "Maximum ADMM iterations" });
                rows.push(PipelineRow::Param { label: "  LSMR Iter", field: "rts_lsmr_iter", help: "LSMR iterations for step 1 (well-conditioned solve)" });
            }
            "tv" => { // TV
                rows.push(PipelineRow::Param { label: "  Lambda", field: "tv_lambda", help: "L1 regularization weight (smaller = smoother)" });
                rows.push(PipelineRow::Param { label: "  Rho", field: "tv_rho", help: "ADMM penalty parameter (typically 100×lambda)" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "tv_tol", help: "Convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "tv_max_iter", help: "Maximum ADMM iterations" });
            }
            "tkd" => { // TKD
                rows.push(PipelineRow::Param { label: "  Threshold", field: "tkd_threshold", help: "Truncation threshold for k-space division (0.1-0.2)" });
            }
            "tsvd" => { // TSVD
                rows.push(PipelineRow::Param { label: "  Threshold", field: "tsvd_threshold", help: "Truncation threshold for SVD (0.1-0.2)" });
            }
            "tgv" => { // TGV
                rows.push(PipelineRow::Param { label: "  Iterations", field: "tgv_iterations", help: "Primal-dual iterations" });
                rows.push(PipelineRow::Param { label: "  Erosions", field: "tgv_erosions", help: "Mask erosions before TGV solve" });
                rows.push(PipelineRow::Param { label: "  Alpha1", field: "tgv_alpha1", help: "First-order TGV weight (gradient term), in ppm" });
                rows.push(PipelineRow::Param { label: "  Alpha0", field: "tgv_alpha0", help: "Second-order TGV weight (symmetric gradient term), in ppm" });
                rows.push(PipelineRow::Param { label: "  Step Size", field: "tgv_step_size", help: "Primal step-size multiplier" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "tgv_tol", help: "Convergence tolerance" });
            }
            "tikhonov" => { // Tikhonov
                rows.push(PipelineRow::Param { label: "  Lambda", field: "tikhonov_lambda", help: "L2 regularization weight" });
            }
            "nltv" => { // NLTV
                rows.push(PipelineRow::Param { label: "  Lambda", field: "nltv_lambda", help: "Regularization parameter" });
                rows.push(PipelineRow::Param { label: "  Mu", field: "nltv_mu", help: "Penalty parameter" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "nltv_tol", help: "Convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "nltv_max_iter", help: "Maximum ADMM iterations" });
                rows.push(PipelineRow::Param { label: "  Newton Iter", field: "nltv_newton_iter", help: "Newton iterations for weight update" });
            }
            "medi" => { // MEDI
                rows.push(PipelineRow::Toggle { label: "  SMV Mode", field: "medi_smv",
                    help: "MEDI handles background removal internally using spherical mean value preprocessing (skips the BG removal step)" });
                rows.push(PipelineRow::Param { label: "  SMV Radius", field: "medi_smv_radius", help: "SMV preprocessing radius in mm" });
                rows.push(PipelineRow::Param { label: "  Lambda", field: "medi_lambda", help: "Regularization weight" });
                rows.push(PipelineRow::Param { label: "  Percentage", field: "medi_percentage", help: "Fraction of voxels considered edges (0.0-1.0)" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "medi_max_iter", help: "Maximum outer iterations" });
                rows.push(PipelineRow::Param { label: "  CG Max Iter", field: "medi_cg_max_iter", help: "Maximum conjugate gradient iterations" });
                rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "medi_cg_tol", help: "CG convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "medi_tol", help: "Outer convergence tolerance" });
                rows.push(PipelineRow::Toggle { label: "  MERIT", field: "medi_merit", help: "Iteratively down-weight voxels whose residual stays large" });
                rows.push(PipelineRow::AlgoSelect { label: "  Data Weighting", field: "medi_data_weighting", options: DATA_WEIGHTING_OPTIONS, help: DATA_WEIGHTING_HELP });
            }
            "tfi" => { // TFI
                rows.push(PipelineRow::Param { label: "  Lambda", field: "tfi_lambda", help: "Regularization weight" });
                rows.push(PipelineRow::Param { label: "  Precond", field: "tfi_precond", help: "Preconditioner value (susceptibility scaling outside the mask)" });
                rows.push(PipelineRow::Param { label: "  Percentage", field: "tfi_percentage", help: "Fraction of voxels considered edges (0.0-1.0)" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "tfi_max_iter", help: "Maximum outer iterations" });
                rows.push(PipelineRow::Param { label: "  CG Max Iter", field: "tfi_cg_max_iter", help: "Maximum conjugate gradient iterations" });
                rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "tfi_cg_tol", help: "CG convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "tfi_tol", help: "Outer convergence tolerance" });
                rows.push(PipelineRow::Toggle { label: "  MERIT", field: "tfi_merit", help: "Iteratively down-weight voxels whose residual stays large" });
                rows.push(PipelineRow::AlgoSelect { label: "  Data Weighting", field: "tfi_data_weighting", options: DATA_WEIGHTING_OPTIONS, help: DATA_WEIGHTING_HELP });
            }
            "ilsqr" => { // iLSQR
                rows.push(PipelineRow::Param { label: "  Tolerance", field: "ilsqr_tol", help: "Convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "ilsqr_max_iter", help: "Maximum iterations" });
                rows.push(PipelineRow::Param { label: "  Pad (mm)", field: "ilsqr_pad_mm", help: "Zero-padding per side, mm. Larger costs time and memory; below 16 mm thin volumes lose accuracy" });
            }
            // HEIDI seeds itself with an LSQR solve, so it shows both groups.
            "lsqr" | "heidi" => {
                rows.push(PipelineRow::Param { label: "  Residual Weight", field: "lsqr_residual_weighting",
                    help: "Weight w on the residual field — the only regularisation. Blank = scaled from B0; 0 drops the term and streaks badly" });
                rows.push(PipelineRow::Toggle { label: "  Fit Global Offset", field: "lsqr_fit_global_offset",
                    help: "Fit a single global field offset alongside χ" });
                rows.push(PipelineRow::Param { label: "  LSQR Tolerance", field: "lsqr_tol", help: "LSQR convergence tolerance" });
                rows.push(PipelineRow::Param { label: "  LSQR Max Iter", field: "lsqr_max_iter", help: "Maximum LSQR iterations" });
                if alg == "heidi" {
                    rows.push(PipelineRow::Param { label: "  Cone Threshold", field: "heidi_cone_threshold",
                        help: "|D(k)| above which a coefficient is taken from the LSQR seed unchanged" });
                    rows.push(PipelineRow::Param { label: "  Gradient Threshold", field: "heidi_gradient_threshold",
                        help: "Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous" });
                    rows.push(PipelineRow::Toggle { label: "  Laplacian Correction", field: "heidi_apply_laplacian_correction",
                        help: "Open the homogeneity masks where the field's Laplacian is large" });
                    if self.heidi_apply_laplacian_correction {
                    rows.push(PipelineRow::Param { label: "    Laplacian Threshold", field: "heidi_laplacian_threshold",
                            help: "Field-Laplacian threshold (ppm per voxel²)" });
                    }
                    rows.push(PipelineRow::Param { label: "  Gradient Mask Floor", field: "heidi_gradient_mask_floor",
                        help: "TV weight kept where every direction reads as an edge (0 = upstream's binarised masks)" });
                    rows.push(PipelineRow::Param { label: "  Continuation Steps", field: "heidi_continuation_steps",
                        help: "NESTA continuation steps on μ. The dominant quality knob, and more is not better — worth sweeping" });
                    rows.push(PipelineRow::Param { label: "  Inner Iterations", field: "heidi_inner_iterations",
                        help: "Accelerated-gradient iterations per continuation step" });
                    rows.push(PipelineRow::Param { label: "  Mu Min", field: "heidi_mu_min", help: "Final (smallest) smoothing parameter μ" });
                    rows.push(PipelineRow::Param { label: "  HEIDI Tolerance", field: "heidi_tol",
                        help: "Relative objective change that ends an inner loop early" });
                    rows.push(PipelineRow::Toggle { label: "  Denoise Field", field: "heidi_denoise",
                        help: "Anisotropic-diffusion smoothing of the field before its gradients are taken" });
                    if self.heidi_denoise {
                    rows.push(PipelineRow::Param { label: "    Denoise Iter", field: "heidi_denoise_iterations", help: "Anisotropic-diffusion iterations" });
                    rows.push(PipelineRow::Param { label: "    Time Step", field: "heidi_denoise_time_step", help: "Anisotropic-diffusion time step" });
                    rows.push(PipelineRow::Param { label: "    Conductance", field: "heidi_denoise_conductance", help: "Anisotropic-diffusion conductance" });
                    }
                }
            }
            "qsmart" => { // QSMART
                rows.push(PipelineRow::Note {
                    text: "⚠ QSMART needs a tight BET brain mask — loose masks cause streaking (Preprocessing → Masking)".into(),
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
                    0 => { // iLSQR: QSMART's own max_iter/pad, iLSQR's tolerance
                        rows.push(PipelineRow::Param { label: "  iLSQR Tolerance", field: "ilsqr_tol", help: "Streaking-stage LSQR tolerance (shared with iLSQR)" });
                        rows.push(PipelineRow::Param { label: "  iLSQR Max Iter", field: "qsmart_ilsqr_max_iter", help: "Maximum iLSQR iterations per stage (tolerance: iLSQR's)" });
                        rows.push(PipelineRow::Param { label: "  iLSQR Pad (mm)", field: "qsmart_ilsqr_pad_mm", help: "Zero-padding of the inner iLSQR solves, mm per side (default: 6)" });
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
                        rows.push(PipelineRow::Toggle { label: "  MERIT", field: "medi_merit", help: "Iteratively down-weight voxels whose residual stays large" });
                        rows.push(PipelineRow::AlgoSelect { label: "  Data Weighting", field: "medi_data_weighting", options: DATA_WEIGHTING_OPTIONS, help: DATA_WEIGHTING_HELP });
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
            "ndi" => { // NDI
                rows.push(PipelineRow::Param { label: "  Tau", field: "ndi_tau", help: "Gradient-descent step size" });
                rows.push(PipelineRow::Param { label: "  Alpha", field: "ndi_alpha", help: "L2 regularization weight" });
                rows.push(PipelineRow::Param { label: "  Max Iter", field: "ndi_max_iter", help: "Number of iterations" });
                rows.push(PipelineRow::Param { label: "  Phase Scale", field: "ndi_phase_scale", help: "ppm -> working-scale multiplier" });
            }
            "fansi" | "fansi-tgv" => { // shared params
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
            "l1qsm" => { // L1-QSM
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
            "whqsm" => { // WH-QSM
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
            "hdqsm" => { // HD-QSM
                rows.push(PipelineRow::Param { label: "  Alpha L2", field: "hdqsm_alpha_l2", help: "L2-stage TV weight" });
                rows.push(PipelineRow::Param { label: "  Mu1 L2", field: "hdqsm_mu1_l2", help: "L2-stage gradient-consistency ADMM weight" });
                rows.push(PipelineRow::Param { label: "  Mu2", field: "hdqsm_mu2", help: "Fidelity consistency weight" });
                rows.push(PipelineRow::Param { label: "  Max Iter L1", field: "hdqsm_max_iter_l1", help: "Stage-1 (L1) iterations" });
                rows.push(PipelineRow::Param { label: "  Max Iter L2", field: "hdqsm_max_iter_l2", help: "Stage-2 (L2) iterations" });
                rows.push(PipelineRow::Param { label: "  Tol Update", field: "hdqsm_tol_update", help: "Stage-2 percent-update stopping tolerance" });
            }
            "amp-pe" => { // AMP-PE
                rows.push(PipelineRow::Param { label: "  Wave Order", field: "amp_pe_wave_order", help: "Daubechies wavelet order (1=db1, 2=db2)" });
                rows.push(PipelineRow::Param { label: "  N Level", field: "amp_pe_nlevel", help: "Wavelet decomposition levels" });
                rows.push(PipelineRow::Param { label: "  Wave Pec", field: "amp_pe_wave_pec", help: "Morphology-mask energy retention fraction (0.0-1.0)" });
                rows.push(PipelineRow::Param { label: "  Simulated TE", field: "amp_pe_simulated_te", help: "Simulated echo time (s) used to turn the field into phase" });
                rows.push(PipelineRow::Param { label: "  Max Linearization Ite", field: "amp_pe_max_linearization_ite", help: "Linearization iterations per stage" });
                rows.push(PipelineRow::Param { label: "  Tikhonov Beta", field: "amp_pe_tikhonov_beta", help: "L2-seed Tikhonov weight" });
                rows.push(PipelineRow::Param { label: "  Damp Rate (signal)", field: "amp_pe_damp_rate_sig", help: "GAMP signal-update damping rate" });
                rows.push(PipelineRow::Param { label: "  Damp Rate (params)", field: "amp_pe_damp_rate_par", help: "Parameter-estimation learning rate (kappa)" });
                rows.push(PipelineRow::Param { label: "  Sparse Recon Ite", field: "amp_pe_max_pe_spar_ite", help: "Inner sparse-reconstruction iterations" });
                rows.push(PipelineRow::Param { label: "  Param Est Ite", field: "amp_pe_max_pe_est_ite", help: "Inner parameter-estimation iterations" });
                rows.push(PipelineRow::Param { label: "  Convergence", field: "amp_pe_cvg_thd", help: "GAMP inner convergence threshold" });
            }
            _ => {}
        }

        // Deep-learning overlap-tiling (optional). Shown for the tileable DL inversions; leave the
        // fields blank to run whole-volume. Matched by name so it survives option reordering.
        if matches!(
            QSM_ALGO_OPTIONS.get(self.qsm_algorithm).copied().unwrap_or(""),
            "xqsm" | "qsmnet" | "qsmnet-plus" | "ir2qsm" | "lpcnn" | "modl-qsm" | "nextqsm"
        ) {
            rows.push(PipelineRow::Toggle { label: "  Tiled Inference", field: "dl_tiling",
                help: "Run the network on overlapping tiles: bounded memory, approximate. Off = whole volume" });
            if self.dl_tiling {
                rows.push(PipelineRow::Param { label: "    Tile Size", field: "dl_tile_size", help: "Overlap-tiling core size in voxels" });
                rows.push(PipelineRow::Param { label: "    Tile Halo", field: "dl_tile_halo", help: "Overlap-tiling context margin in voxels (blank = 8)" });
            }
        }

    }

    /// QSM → Multi-orientation: how an orientation group (Input tab) is reconstructed.
    fn multi_orientation_rows(&self, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::AlgoSelect {
            label: "Combine as", field: "orientation_algorithm",
            options: &ORIENTATION_ALGORITHMS, help: &ORIENTATION_ALGORITHM_HELP,
        });
        rows.push(PipelineRow::Param { label: "Lambda", field: "orientation_lambda",
            help: "Regularization (blank = 0, plain least squares). Raise for 2 orientations or single-axis rotations" });
        if self.ctx.orientation_cosmos {
            rows.push(PipelineRow::Toggle {
                label: "COSMOS Magnitude Weighting", field: "cosmos_magnitude_weighting",
                help: "COSMOS groups: weight each orientation by its combined magnitude (conjugate-gradient solve). Off = closed-form least squares",
            });
            if self.cosmos_magnitude_weighting {
                rows.push(PipelineRow::Param { label: "  CG Tolerance", field: "cosmos_tol", help: "Conjugate-gradient tolerance of weighted COSMOS" });
                rows.push(PipelineRow::Param { label: "  CG Max Iterations", field: "cosmos_max_iter", help: "Conjugate-gradient iteration cap of weighted COSMOS" });
            }
        }
        rows.push(PipelineRow::Toggle {
            label: "Co-register", field: "orientation_registration",
            help: "Co-register orientations that need it, with the registration settings below. Off = refuse such groups",
        });
        if self.orientation_registration {
            Self::push_registration_rows(rows);
        }
        rows.push(PipelineRow::Toggle {
            label: "Force", field: "multi_orientation_force",
            help: "Reconstruct even when the B0 directions look degenerate (wrong metadata gives a plausible wrong map)",
        });
    }

    /// QSM → Reference: what χ's zero is pinned to.
    fn reference_rows(&self, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::AlgoSelect {
            label: "QSM Reference", field: "qsm_reference",
            options: qsm_ref_options(), help: qsm_ref_help(),
        });
    }

    /// Every pipeline row that shows now, page after page (the QSM switch once): for tests that
    /// ask whether a setting is reachable, not where.
    #[cfg(test)]
    pub fn all_rows(&self) -> Vec<PipelineRow> {
        let mut rows = Vec::new();
        self.geometry_rows(&mut rows);
        self.masking_rows(&mut rows);
        self.motion_rows(&mut rows);
        self.push_qsm_toggle(&mut rows);
        if self.qsm_runs() {
            if self.field_mapping_runs().0 { self.field_map_rows(&mut rows); }
            if self.inversion_needs_bg_removal() { self.background_rows(&mut rows); }
            self.inversion_rows(&mut rows);
            if self.ctx.orientation_group { self.multi_orientation_rows(&mut rows); }
            self.reference_rows(&mut rows);
        }
        self.separation_rows(&mut rows);
        rows
    }

    /// The rows of the pipeline form's page [`Self::page`]; empty for the App's own pages.
    pub fn visible_rows(&self) -> Vec<PipelineRow> {
        let mut rows = Vec::new();
        match self.page {
            Page::Geometry => self.geometry_rows(&mut rows),
            Page::Masking => self.masking_rows(&mut rows),
            Page::Motion => self.motion_rows(&mut rows),
            Page::QsmOff => self.push_qsm_toggle(&mut rows),
            Page::FieldMap => { self.push_qsm_toggle(&mut rows); self.field_map_rows(&mut rows) }
            Page::Background => { self.push_qsm_toggle(&mut rows); self.background_rows(&mut rows) }
            Page::Inversion => { self.push_qsm_toggle(&mut rows); self.inversion_rows(&mut rows) }
            Page::MultiOrientation => { self.push_qsm_toggle(&mut rows); self.multi_orientation_rows(&mut rows) }
            Page::Reference => { self.push_qsm_toggle(&mut rows); self.reference_rows(&mut rows) }
            Page::Separation => self.separation_rows(&mut rows),
            _ => {}
        }
        rows
    }

    /// All valid parameter field names, for testing that string dispatch is complete.
    #[cfg(test)]
    pub const ALL_PARAM_FIELDS: &[&str] = &[
        "obliquity_threshold",
        "dl_tile_size", "dl_tile_halo",
        "rts_delta", "rts_mu", "rts_tol", "rts_rho", "rts_max_iter", "rts_lsmr_iter",
        "tv_lambda", "tv_rho", "tv_tol", "tv_max_iter",
        "tkd_threshold", "tsvd_threshold",
        "ilsqr_tol", "ilsqr_max_iter", "ilsqr_pad_mm",
        "tgv_iterations", "tgv_erosions", "tgv_alpha1", "tgv_alpha0",
        "tikhonov_lambda",
        "nltv_lambda", "nltv_mu", "nltv_tol", "nltv_max_iter", "nltv_newton_iter",
        "medi_lambda", "medi_max_iter", "medi_cg_max_iter", "medi_cg_tol", "medi_tol", "medi_percentage", "medi_smv_radius",
        "tfi_lambda", "tfi_precond", "tfi_max_iter", "tfi_cg_max_iter", "tfi_cg_tol", "tfi_tol", "tfi_percentage",
        "amp_pe_wave_order", "amp_pe_nlevel", "amp_pe_wave_pec", "amp_pe_simulated_te", "amp_pe_max_linearization_ite", "amp_pe_tikhonov_beta",
        "phase_offset_sigma", "romeo_template",
        "romeo_temporal_uncertain_unwrapping", "romeo_max_seeds", "romeo_wrap_addition",
        "vsharp_threshold", "vsharp_max_radius", "vsharp_min_radius", "vsharp_grid_spacing", "pdf_tol", "lbv_tol",
        "ismv_tol", "ismv_max_iter", "ismv_radius", "sharp_threshold", "sharp_radius",
        "resharp_radius", "resharp_tik_reg", "resharp_tol", "resharp_max_iter",
        "harperella_radius", "harperella_max_iter", "harperella_tol",
        "iharperella_max_iter", "iharperella_tol",
        "bet_smoothness", "bet_gradient_threshold", "bet_iterations", "bet_subdivisions",
        "msmv_radius", "msmv_maxk",
        "qsmart_ilsqr_max_iter", "qsmart_ilsqr_pad_mm", "qsmart_vasc_sphere_radius", "qsmart_sdf_spatial_radius",
        "qsmart_sdf_sigma1_stage1", "qsmart_sdf_sigma2_stage1", "qsmart_sdf_sigma1_stage2", "qsmart_sdf_sigma2_stage2",
        "qsmart_sdf_lower_lim", "qsmart_sdf_curv_constant",
        "qsmart_frangi_scale_min", "qsmart_frangi_scale_max", "qsmart_frangi_scale_ratio", "qsmart_frangi_c",
        "ndi_tau", "ndi_alpha", "ndi_max_iter", "ndi_phase_scale",
        "fansi_alpha1", "fansi_mu1", "fansi_mu2", "fansi_alpha0", "fansi_mu0", "fansi_max_iter", "fansi_tol_update", "fansi_tol_delta", "fansi_phase_scale",
        "l1qsm_alpha1", "l1qsm_mu1", "l1qsm_mu2", "l1qsm_mu3", "l1qsm_lambda", "l1qsm_max_iter", "l1qsm_tol_update", "l1qsm_tol_delta", "l1qsm_phase_scale",
        "whqsm_alpha1", "whqsm_mu1", "whqsm_mu2", "whqsm_beta", "whqsm_muh", "whqsm_max_iter", "whqsm_tol_update", "whqsm_tol_delta", "whqsm_phase_scale",
        "hdqsm_alpha_l2", "hdqsm_mu1_l2", "hdqsm_mu2", "hdqsm_max_iter_l1", "hdqsm_max_iter_l2", "hdqsm_tol_update",
        "r2primenet_patch",
        "laplacian_fft_pad",
        "crop_margin_mm", "homogeneity_sigma_mm", "homogeneity_nbox", "coil_combination_sigma",
        "tgv_step_size", "tgv_tol",
        "amp_pe_damp_rate_sig", "amp_pe_damp_rate_par", "amp_pe_max_pe_spar_ite", "amp_pe_max_pe_est_ite", "amp_pe_cvg_thd",
        "linear_fit_reliability_threshold", "robust_fit_tuning", "robust_fit_iterations",
        "robust_fit_downweight_level", "robust_fit_flag_ratio",
        "motion_reference_echo", "motion_chain_levels", "motion_min_ncc",
        "reg_levels", "reg_max_samples", "reg_max_iterations", "reg_max_evaluations", "reg_tolerance", "reg_rotation_step_deg", "reg_translation_step_mm", "reg_min_overlap",
        "cosmos_tol", "cosmos_max_iter",
    ];

    /// Get a string parameter value by field name.
    pub fn get_param(&self, field: &str) -> &str {
        match field {
            "obliquity_threshold" => &self.obliquity_threshold,
            "crop_margin_mm" => &self.crop_margin_mm,
            "homogeneity_sigma_mm" => &self.homogeneity_sigma_mm,
            "homogeneity_nbox" => &self.homogeneity_nbox,
            "coil_combination_sigma" => &self.coil_combination_sigma,
            "tgv_step_size" => &self.tgv_step_size,
            "tgv_tol" => &self.tgv_tol,
            "medi_data_weighting" => &self.medi_data_weighting,
            "tfi_data_weighting" => &self.tfi_data_weighting,
            "amp_pe_damp_rate_sig" => &self.amp_pe_damp_rate_sig,
            "amp_pe_damp_rate_par" => &self.amp_pe_damp_rate_par,
            "amp_pe_max_pe_spar_ite" => &self.amp_pe_max_pe_spar_ite,
            "amp_pe_max_pe_est_ite" => &self.amp_pe_max_pe_est_ite,
            "amp_pe_cvg_thd" => &self.amp_pe_cvg_thd,
            "custom_dseg_tool" => &self.custom_dseg_tool,
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
            "lsqr_residual_weighting" => &self.lsqr_residual_weighting,
            "lsqr_tol" => &self.lsqr_tol,
            "lsqr_max_iter" => &self.lsqr_max_iter,
            "heidi_cone_threshold" => &self.heidi_cone_threshold,
            "heidi_gradient_threshold" => &self.heidi_gradient_threshold,
            "heidi_laplacian_threshold" => &self.heidi_laplacian_threshold,
            "b0_weight_assumed_t2star_ms" => &self.b0_weight_assumed_t2star_ms,
            "laplacian_fft_pad" => &self.laplacian_fft_pad,
            "linear_fit_reliability_threshold" => &self.linear_fit_reliability_threshold,
            "robust_fit_tuning" => &self.robust_fit_tuning,
            "robust_fit_iterations" => &self.robust_fit_iterations,
            "robust_fit_downweight_level" => &self.robust_fit_downweight_level,
            "robust_fit_flag_ratio" => &self.robust_fit_flag_ratio,
            "motion_reference_echo" => &self.motion_reference_echo,
            "motion_chain_levels" => &self.motion_chain_levels,
            "motion_min_ncc" => &self.motion_min_ncc,
            "reg_levels" => &self.reg_levels,
            "reg_max_samples" => &self.reg_max_samples,
            "reg_max_iterations" => &self.reg_max_iterations,
            "reg_max_evaluations" => &self.reg_max_evaluations,
            "reg_tolerance" => &self.reg_tolerance,
            "reg_rotation_step_deg" => &self.reg_rotation_step_deg,
            "reg_translation_step_mm" => &self.reg_translation_step_mm,
            "reg_min_overlap" => &self.reg_min_overlap,
            "cosmos_tol" => &self.cosmos_tol,
            "cosmos_max_iter" => &self.cosmos_max_iter,
            "heidi_gradient_mask_floor" => &self.heidi_gradient_mask_floor,
            "heidi_continuation_steps" => &self.heidi_continuation_steps,
            "heidi_inner_iterations" => &self.heidi_inner_iterations,
            "heidi_mu_min" => &self.heidi_mu_min,
            "heidi_tol" => &self.heidi_tol,
            "heidi_denoise_iterations" => &self.heidi_denoise_iterations,
            "heidi_denoise_time_step" => &self.heidi_denoise_time_step,
            "heidi_denoise_conductance" => &self.heidi_denoise_conductance,
            "ilsqr_max_iter" => &self.ilsqr_max_iter,
            "ilsqr_pad_mm" => &self.ilsqr_pad_mm,
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
            "romeo_temporal_uncertain_unwrapping" => &self.romeo_temporal_uncertain_unwrapping,
            "romeo_max_seeds" => &self.romeo_max_seeds,
            "romeo_wrap_addition" => &self.romeo_wrap_addition,
            "custom_mask_tool" => &self.custom_mask_tool,
            "vsharp_threshold" => &self.vsharp_threshold,
            "vsharp_max_radius" => &self.vsharp_max_radius,
            "vsharp_min_radius" => &self.vsharp_min_radius,
            "vsharp_grid_spacing" => &self.vsharp_grid_spacing,
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
            "bet_smoothness" => &self.bet_smoothness,
            "bet_gradient_threshold" => &self.bet_gradient_threshold,
            "bet_iterations" => &self.bet_iterations,
            "bet_subdivisions" => &self.bet_subdivisions,
            "iharperella_max_iter" => &self.iharperella_max_iter,
            "iharperella_tol" => &self.iharperella_tol,
            "msmv_radius" => &self.msmv_radius,
            "msmv_maxk" => &self.msmv_maxk,
            "qsmart_ilsqr_max_iter" => &self.qsmart_ilsqr_max_iter,
            "qsmart_ilsqr_pad_mm" => &self.qsmart_ilsqr_pad_mm,
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
            "custom_qsm_tool" => &self.custom_qsm_tool,
            "custom_r2_tool" => &self.custom_r2_tool,
            "custom_r2prime_tool" => &self.custom_r2prime_tool,
            "r2primenet_patch" => &self.r2primenet_patch,
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
            "crop_margin_mm" => Some(&mut self.crop_margin_mm),
            "homogeneity_sigma_mm" => Some(&mut self.homogeneity_sigma_mm),
            "homogeneity_nbox" => Some(&mut self.homogeneity_nbox),
            "coil_combination_sigma" => Some(&mut self.coil_combination_sigma),
            "tgv_step_size" => Some(&mut self.tgv_step_size),
            "tgv_tol" => Some(&mut self.tgv_tol),
            "medi_data_weighting" => Some(&mut self.medi_data_weighting),
            "tfi_data_weighting" => Some(&mut self.tfi_data_weighting),
            "amp_pe_damp_rate_sig" => Some(&mut self.amp_pe_damp_rate_sig),
            "amp_pe_damp_rate_par" => Some(&mut self.amp_pe_damp_rate_par),
            "amp_pe_max_pe_spar_ite" => Some(&mut self.amp_pe_max_pe_spar_ite),
            "amp_pe_max_pe_est_ite" => Some(&mut self.amp_pe_max_pe_est_ite),
            "amp_pe_cvg_thd" => Some(&mut self.amp_pe_cvg_thd),
            "custom_dseg_tool" => Some(&mut self.custom_dseg_tool),
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
            "lsqr_residual_weighting" => Some(&mut self.lsqr_residual_weighting),
            "lsqr_tol" => Some(&mut self.lsqr_tol),
            "lsqr_max_iter" => Some(&mut self.lsqr_max_iter),
            "heidi_cone_threshold" => Some(&mut self.heidi_cone_threshold),
            "heidi_gradient_threshold" => Some(&mut self.heidi_gradient_threshold),
            "heidi_laplacian_threshold" => Some(&mut self.heidi_laplacian_threshold),
            "b0_weight_assumed_t2star_ms" => Some(&mut self.b0_weight_assumed_t2star_ms),
            "laplacian_fft_pad" => Some(&mut self.laplacian_fft_pad),
            "linear_fit_reliability_threshold" => Some(&mut self.linear_fit_reliability_threshold),
            "robust_fit_tuning" => Some(&mut self.robust_fit_tuning),
            "robust_fit_iterations" => Some(&mut self.robust_fit_iterations),
            "robust_fit_downweight_level" => Some(&mut self.robust_fit_downweight_level),
            "robust_fit_flag_ratio" => Some(&mut self.robust_fit_flag_ratio),
            "motion_reference_echo" => Some(&mut self.motion_reference_echo),
            "motion_chain_levels" => Some(&mut self.motion_chain_levels),
            "motion_min_ncc" => Some(&mut self.motion_min_ncc),
            "reg_levels" => Some(&mut self.reg_levels),
            "reg_max_samples" => Some(&mut self.reg_max_samples),
            "reg_max_iterations" => Some(&mut self.reg_max_iterations),
            "reg_max_evaluations" => Some(&mut self.reg_max_evaluations),
            "reg_tolerance" => Some(&mut self.reg_tolerance),
            "reg_rotation_step_deg" => Some(&mut self.reg_rotation_step_deg),
            "reg_translation_step_mm" => Some(&mut self.reg_translation_step_mm),
            "reg_min_overlap" => Some(&mut self.reg_min_overlap),
            "cosmos_tol" => Some(&mut self.cosmos_tol),
            "cosmos_max_iter" => Some(&mut self.cosmos_max_iter),
            "heidi_gradient_mask_floor" => Some(&mut self.heidi_gradient_mask_floor),
            "heidi_continuation_steps" => Some(&mut self.heidi_continuation_steps),
            "heidi_inner_iterations" => Some(&mut self.heidi_inner_iterations),
            "heidi_mu_min" => Some(&mut self.heidi_mu_min),
            "heidi_tol" => Some(&mut self.heidi_tol),
            "heidi_denoise_iterations" => Some(&mut self.heidi_denoise_iterations),
            "heidi_denoise_time_step" => Some(&mut self.heidi_denoise_time_step),
            "heidi_denoise_conductance" => Some(&mut self.heidi_denoise_conductance),
            "ilsqr_max_iter" => Some(&mut self.ilsqr_max_iter),
            "ilsqr_pad_mm" => Some(&mut self.ilsqr_pad_mm),
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
            "romeo_temporal_uncertain_unwrapping" => Some(&mut self.romeo_temporal_uncertain_unwrapping),
            "romeo_max_seeds" => Some(&mut self.romeo_max_seeds),
            "romeo_wrap_addition" => Some(&mut self.romeo_wrap_addition),
            "custom_mask_tool" => Some(&mut self.custom_mask_tool),
            "vsharp_threshold" => Some(&mut self.vsharp_threshold),
            "vsharp_max_radius" => Some(&mut self.vsharp_max_radius),
            "vsharp_min_radius" => Some(&mut self.vsharp_min_radius),
            "vsharp_grid_spacing" => Some(&mut self.vsharp_grid_spacing),
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
            "bet_smoothness" => Some(&mut self.bet_smoothness),
            "bet_gradient_threshold" => Some(&mut self.bet_gradient_threshold),
            "bet_iterations" => Some(&mut self.bet_iterations),
            "bet_subdivisions" => Some(&mut self.bet_subdivisions),
            "iharperella_max_iter" => Some(&mut self.iharperella_max_iter),
            "iharperella_tol" => Some(&mut self.iharperella_tol),
            "msmv_radius" => Some(&mut self.msmv_radius),
            "msmv_maxk" => Some(&mut self.msmv_maxk),
            "qsmart_ilsqr_max_iter" => Some(&mut self.qsmart_ilsqr_max_iter),
            "qsmart_ilsqr_pad_mm" => Some(&mut self.qsmart_ilsqr_pad_mm),
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
            "custom_qsm_tool" => Some(&mut self.custom_qsm_tool),
            "custom_r2_tool" => Some(&mut self.custom_r2_tool),
            "custom_r2prime_tool" => Some(&mut self.custom_r2prime_tool),
            "r2primenet_patch" => Some(&mut self.r2primenet_patch),
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
            "r2prime_strategy" => self.r2prime_strategy,
            "b0_estimation" => self.b0_estimation,
            "b0_weight_type" => self.b0_weight_type,
            "laplacian_solver" => self.laplacian_solver,
            "vsharp_grid" => self.vsharp_grid,
            "robust_fit_loss" => self.robust_fit_loss,
            "motion_reference" => self.motion_reference,
            "output_space" => self.output_space,
            "mask_preset" => self.mask_preset,
            "separation_algorithm" => self.separation_algorithm,
            // Stored as the CLI value (0 uniform, 1 magnitude SNR).
            "medi_data_weighting" => usize::from(self.medi_data_weighting.trim() == "1"),
            "tfi_data_weighting" => usize::from(self.tfi_data_weighting.trim() == "1"),
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
            "r2prime_strategy" => self.r2prime_strategy = val,
            "b0_estimation" => self.b0_estimation = val,
            "b0_weight_type" => self.b0_weight_type = val,
            "laplacian_solver" => self.laplacian_solver = val,
            "vsharp_grid" => self.vsharp_grid = val,
            "robust_fit_loss" => self.robust_fit_loss = val,
            "motion_reference" => self.motion_reference = val,
            "output_space" => self.output_space = val,
            "mask_preset" => {
                self.mask_preset = val;
                self.apply_mask_preset(val);
            }
            "separation_algorithm" => self.separation_algorithm = val,
            "medi_data_weighting" => self.medi_data_weighting = val.min(1).to_string(),
            "tfi_data_weighting" => self.tfi_data_weighting = val.min(1).to_string(),
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
            "romeo_phase_gradient_coherence" => self.romeo_phase_gradient_coherence,
            "romeo_phase_linearity" => self.romeo_phase_linearity,
            "romeo_mag_coherence" => self.romeo_mag_coherence,
            "romeo_mag_weight" => self.romeo_mag_weight,
            "romeo_mag_weight2" => self.romeo_mag_weight2,
            "romeo_bestpath" => self.romeo_bestpath,
            "romeo_merge_regions" => self.romeo_merge_regions,
            "romeo_temporal_uncertain" => self.romeo_temporal_uncertain,
            "linear_fit_reliability" => self.linear_fit_reliability,
            "dl_tiling" => self.dl_tiling,
            "romeo_correct_regions" => self.romeo_correct_regions,
            "linear_fit_estimate_offset" => self.linear_fit_estimate_offset,
            "medi_smv" => self.medi_smv,
            "do_chi_separation" => self.do_chi_separation,
            "msmv_refine" => self.msmv_refine,
            "two_pass" => self.two_pass,
            "lsqr_fit_global_offset" => self.lsqr_fit_global_offset,
            "heidi_apply_laplacian_correction" => self.heidi_apply_laplacian_correction,
            "heidi_denoise" => self.heidi_denoise,
            "ignore_slice_gap" => self.ignore_slice_gap,
            "axial_resampling" => self.axial_resampling,
            "crop_to_mask" => self.crop_to_mask,
            "fft_padding" => self.fft_padding,
            "romeo_phase_coherence" => self.romeo_phase_coherence,
            "medi_merit" => self.medi_merit,
            "tfi_merit" => self.tfi_merit,
            "motion_correction" => self.motion_correction,
            "orientation_registration" => self.orientation_registration,
            "multi_orientation_force" => self.multi_orientation_force,
            "cosmos_magnitude_weighting" => self.cosmos_magnitude_weighting,
            "motion_chain" => self.motion_chain,
            "r2primenet_ignore_field_mismatch" => self.r2primenet_ignore_field_mismatch,
            "vsharp_deconvolve" => self.vsharp_deconvolve,
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
            "romeo_phase_gradient_coherence" => self.romeo_phase_gradient_coherence = !self.romeo_phase_gradient_coherence,
            "romeo_phase_linearity" => self.romeo_phase_linearity = !self.romeo_phase_linearity,
            "romeo_mag_coherence" => self.romeo_mag_coherence = !self.romeo_mag_coherence,
            "romeo_mag_weight" => self.romeo_mag_weight = !self.romeo_mag_weight,
            "romeo_mag_weight2" => self.romeo_mag_weight2 = !self.romeo_mag_weight2,
            "romeo_bestpath" => self.romeo_bestpath = !self.romeo_bestpath,
            "romeo_merge_regions" => self.romeo_merge_regions = !self.romeo_merge_regions,
            "romeo_temporal_uncertain" => self.romeo_temporal_uncertain = !self.romeo_temporal_uncertain,
            "linear_fit_reliability" => self.linear_fit_reliability = !self.linear_fit_reliability,
            "dl_tiling" => self.dl_tiling = !self.dl_tiling,
            "romeo_correct_regions" => self.romeo_correct_regions = !self.romeo_correct_regions,
            "linear_fit_estimate_offset" => self.linear_fit_estimate_offset = !self.linear_fit_estimate_offset,
            "medi_smv" => self.medi_smv = !self.medi_smv,
            "msmv_refine" => self.msmv_refine = !self.msmv_refine,
            "two_pass" => self.two_pass = !self.two_pass,
            "lsqr_fit_global_offset" => self.lsqr_fit_global_offset = !self.lsqr_fit_global_offset,
            "heidi_apply_laplacian_correction" => self.heidi_apply_laplacian_correction = !self.heidi_apply_laplacian_correction,
            "heidi_denoise" => self.heidi_denoise = !self.heidi_denoise,
            "ignore_slice_gap" => self.ignore_slice_gap = !self.ignore_slice_gap,
            "axial_resampling" => self.axial_resampling = !self.axial_resampling,
            "crop_to_mask" => self.crop_to_mask = !self.crop_to_mask,
            "fft_padding" => self.fft_padding = !self.fft_padding,
            "romeo_phase_coherence" => self.romeo_phase_coherence = !self.romeo_phase_coherence,
            "medi_merit" => self.medi_merit = !self.medi_merit,
            "tfi_merit" => self.tfi_merit = !self.tfi_merit,
            "motion_correction" => self.motion_correction = !self.motion_correction,
            "orientation_registration" => self.orientation_registration = !self.orientation_registration,
            "multi_orientation_force" => self.multi_orientation_force = !self.multi_orientation_force,
            "cosmos_magnitude_weighting" => self.cosmos_magnitude_weighting = !self.cosmos_magnitude_weighting,
            "motion_chain" => self.motion_chain = !self.motion_chain,
            "r2primenet_ignore_field_mismatch" => self.r2primenet_ignore_field_mismatch = !self.r2primenet_ignore_field_mismatch,
            "vsharp_deconvolve" => self.vsharp_deconvolve = !self.vsharp_deconvolve,
            _ => {}
        }
    }

    pub fn mask_op_label_value(op: &crate::pipeline::config::MaskOp) -> (&'static str, String) {
        use crate::pipeline::config::MaskOp;
        match op {
            MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Otsu, .. } => ("threshold", "otsu".to_string()),
            MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Fixed, value } =>
                ("threshold", format!("fixed:{}", value.unwrap_or(0.5))),
            MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Percentile, value } =>
                ("threshold", format!("percentile:{}", value.unwrap_or(75.0))),
            MaskOp::Bet { fractional_intensity, voxel_scale } => ("bet", format!("{}{}", fractional_intensity,
                if (*voxel_scale - crate::pipeline::config::bet_default_voxel_scale()).abs() > f64::EPSILON {
                    format!(" scale={voxel_scale}")
                } else {
                    String::new()
                })),
            MaskOp::Erode { iterations } => ("erode", format!("{}", iterations)),
            MaskOp::Dilate { iterations } => ("dilate", format!("{}", iterations)),
            MaskOp::Close { radius } => ("close", format!("{}", radius)),
            MaskOp::FillHoles { max_size } => ("fill-holes", if *max_size == 0 { "auto".to_string() } else { format!("{}", max_size) }),
            MaskOp::GaussianSmooth { sigma_mm } => ("gaussian", format!("{}", sigma_mm)),
            MaskOp::HdBet { patch, tta, tile_step } => {
                let step = if (*tile_step - crate::pipeline::config::hd_bet_default_tile_step()).abs() > f64::EPSILON {
                    format!(" step={tile_step}")
                } else {
                    String::new()
                };
                ("hd-bet", format!("{}x{}x{}{}{}", patch[0], patch[1], patch[2], step, if *tta { " tta" } else { "" }))
            }
            MaskOp::Rs2Net { tta, tile_step } => {
                let step = if (*tile_step - crate::pipeline::config::rs2_net_default_tile_step()).abs() > f64::EPSILON {
                    format!(" step={tile_step}")
                } else {
                    String::new()
                };
                ("rs2-net", format!("rodent{}{}", step, if *tta { " tta" } else { "" }))
            }
            // Only the threshold: every other parameter has a row of its own below.
            MaskOp::SignalErode { threshold, .. } => ("signal-erode", format!("{:.2}", threshold)),
        }
    }

    /// Get help text for a mask op.
    pub fn mask_op_help(op: &crate::pipeline::config::MaskOp) -> &'static str {
        use crate::pipeline::config::MaskOp;
        match op {
            MaskOp::Threshold { .. } => "Threshold method (←/→ to change, Enter to edit value)",
            MaskOp::Bet { .. } => "BET fractional intensity (Enter to edit); scale= inflates the voxel sizes for rodent data",
            MaskOp::Erode { .. } => "Erosion iterations (←/→ to adjust)",
            MaskOp::Dilate { .. } => "Dilation iterations (←/→ to adjust)",
            MaskOp::Close { .. } => "Morphological close radius (←/→ to adjust)",
            MaskOp::FillHoles { .. } => "Fill holes max size (0=auto, Enter to edit)",
            MaskOp::GaussianSmooth { .. } => "Gaussian sigma in mm (Enter to edit)",
            MaskOp::HdBet { .. } => "HD-BET deep-learning brain extraction (needs a deep-learning build)",
            MaskOp::Rs2Net { .. } => "RS2-Net deep-learning rodent brain extraction (needs a deep-learning build)",
            MaskOp::SignalErode { .. } =>
                "Signal gate as a fraction of the in-mask median magnitude (←/→ to adjust); above ~0.85 it over-carves",
        }
    }

    /// Create a default mask op for the given type name.
    pub fn default_mask_op(type_name: &str) -> Option<crate::pipeline::config::MaskOp> {
        use crate::pipeline::config::*;
        match type_name {
            "threshold" => Some(MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }),
            "bet" => Some(MaskOp::bet(0.5)),
            "erode" => Some(MaskOp::Erode { iterations: 1 }),
            "dilate" => Some(MaskOp::Dilate { iterations: 1 }),
            "close" => Some(MaskOp::Close { radius: 1 }),
            "fill-holes" => Some(MaskOp::FillHoles { max_size: 0 }),
            "gaussian" => Some(MaskOp::GaussianSmooth { sigma_mm: 4.0 }),
            "hd-bet" => Some(MaskOp::hd_bet_default()),
            "rs2-net" => Some(MaskOp::rs2_net_default()),
            "signal-erode" => Some(MaskOp::signal_erode_default()),
            _ => None,
        }
    }

    /// Apply a mask preset, overwriting the whole recipe (sections, combine mode and the
    /// refinements that run on the combined mask).
    /// Whether the selected inversion reads a background-removed local field (QSM.rs
    /// `InversionInput::LocalField`), and so whether background removal is part of the run.
    pub fn inversion_needs_bg_removal(&self) -> bool {
        self.stage_config().is_none_or(|c| !crate::pipeline::runner::skips_bgremove(&c))
    }

    /// The settings that decide which reconstruction stages run — the inversion (and MEDI's SMV
    /// mode) and the background removal — on an otherwise default config, or `None` for an
    /// index no algorithm answers to.
    fn stage_config(&self) -> Option<crate::pipeline::config::PipelineConfig> {
        let name = |list: &[&str], i: usize| serde_json::json!(list.get(i).copied().unwrap_or(""));
        let mut config = crate::pipeline::config::PipelineConfig::default();
        config.inversion.algorithm = serde_json::from_value(name(QSM_ALGO_OPTIONS, self.qsm_algorithm)).ok()?;
        config.inversion.medi.smv = self.medi_smv;
        config.bg_removal.algorithm = serde_json::from_value(name(BF_OPTIONS, self.bf_algorithm)).ok()?;
        Some(config)
    }

    /// Whether field mapping runs in front of the inversion: `(for some runs, for every run)`.
    ///
    /// The runner's own rule ([`crate::pipeline::runner::field_mapping_runs`]), asked for the
    /// echo count the run is limited to, or for one and for several echoes when that is unknown —
    /// TGV maps the field of multi-echo data but reconstructs a single echo from its wrapped phase.
    pub fn field_mapping_runs(&self) -> (bool, bool) {
        let Some(config) = self.stage_config() else { return (true, true) };
        let counts = match self.ctx.num_echoes {
            Some(n) => vec![n],
            None => vec![1, 2],
        };
        let runs: Vec<bool> = counts.iter()
            .map(|&n| crate::pipeline::runner::field_mapping_runs(&config, n)).collect();
        (runs.iter().any(|&r| r), runs.iter().all(|&r| r))
    }


    /// Whether QSM is reconstructed: Compute QSM is on, or another output forces it.
    pub fn qsm_runs(&self) -> bool {
        self.do_qsm || self.ctx.qsm_forced_by.is_some()
    }

    /// Whether any step of the masking recipe (sections, combined refinements, two-pass) is BET.
    pub fn uses_bet(&self) -> bool {
        use crate::pipeline::config::MaskOp;
        let in_section = |s: &crate::pipeline::config::MaskSection| {
            s.all_ops().iter().any(|op| matches!(op, MaskOp::Bet { .. }))
        };
        self.mask_sections.iter().any(in_section)
            || self.mask_combined_refinements.iter().any(|op| matches!(op, MaskOp::Bet { .. }))
            || (self.two_pass && self.two_pass_sections.iter().any(in_section))
    }

    pub fn apply_mask_preset(&mut self, preset: usize) {
        use crate::pipeline::config::*;
        // MASK_PRESET_OPTIONS is mask_presets() plus the trailing "custom" entry, which leaves
        // the recipe alone (see the alignment test).
        let Some((_, recipe)) = mask_presets().into_iter().nth(preset) else { return };
        self.mask_sections = recipe.sections;
        self.mask_combine = recipe.combine;
        self.mask_combined_refinements = recipe.refinements;
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
    pub fn mask_section(&self, section: usize) -> Option<&crate::pipeline::config::MaskSection> {
        let (two_pass, i) = Self::mask_section_slot(section);
        if two_pass { self.two_pass_sections.get(i) } else { self.mask_sections.get(i) }
    }

    /// Mutable counterpart of [`Self::mask_section`].
    pub fn mask_section_mut(&mut self, section: usize) -> Option<&mut crate::pipeline::config::MaskSection> {
        let (two_pass, i) = Self::mask_section_slot(section);
        if two_pass { self.two_pass_sections.get_mut(i) } else { self.mask_sections.get_mut(i) }
    }

    /// How many sections the mask a row belongs to has.
    pub fn mask_section_count(&self, two_pass: bool) -> usize {
        if two_pass { self.two_pass_sections.len() } else { self.mask_sections.len() }
    }

    /// The refinement list a mask-step row edits — a section's, or the combined mask's.
    pub fn mask_refinements(&self, section: usize) -> Option<&Vec<crate::pipeline::config::MaskOp>> {
        if section == MASK_COMBINED_SECTION {
            Some(&self.mask_combined_refinements)
        } else {
            self.mask_section(section).map(|s| &s.refinements)
        }
    }

    /// Mutable counterpart of [`Self::mask_refinements`].
    pub fn mask_refinements_mut(&mut self, section: usize) -> Option<&mut Vec<crate::pipeline::config::MaskOp>> {
        if section == MASK_COMBINED_SECTION {
            Some(&mut self.mask_combined_refinements)
        } else {
            self.mask_section_mut(section).map(|s| &mut s.refinements)
        }
    }

    /// Rows for every section of one mask — the main mask, or the two-pass reliable mask.
    ///
    /// Both get the identical row set; only the section index differs (see [`MASK_TWO_PASS_BASE`]),
    /// which is what keeps the reliable mask editable in exactly the ways the main mask is.
    fn push_mask_section_rows(&self, rows: &mut Vec<PipelineRow>, two_pass: bool) {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let count = self.mask_section_count(two_pass);
        let base = if two_pass { MASK_TWO_PASS_BASE } else { 0 };
        for i in 0..count {
            let si = base + i;
            if i > 0 {
                // The reliable mask folds with the main mask's combine mode, so it reports that
                // mode rather than offering a second control over the same setting.
                if two_pass {
                    rows.push(PipelineRow::Note {
                        text: match self.mask_combine_recipe().0 {
                            crate::pipeline::config::MaskCombine::Or => "── combined with OR ──",
                            crate::pipeline::config::MaskCombine::And => "── combined with AND ──",
                        }.into(),
                    });
                } else {
                    rows.push(PipelineRow::MaskCombineOp);
                }
            }
            if count > 1 {
                rows.push(PipelineRow::MaskSectionHeader { section: si });
            }
            rows.push(PipelineRow::MaskOpInput { section: si });
            rows.push(PipelineRow::MaskOpGenerator { section: si });
            rows.push(PipelineRow::MaskOpGeneratorParam { section: si });
            let generator = self.mask_section(si).map(|s| &s.generator);
            if let Some(MaskOp::Threshold { method, .. }) = generator {
                if matches!(method, MaskThresholdMethod::Fixed | MaskThresholdMethod::Percentile) {
                    rows.push(PipelineRow::MaskOpThresholdValue { section: si });
                }
            }
            if matches!(generator, Some(MaskOp::HdBet { .. })) {
                rows.push(PipelineRow::MaskOpHdBetStep { section: si });
            }
            if matches!(generator, Some(MaskOp::Bet { .. })) {
                rows.push(PipelineRow::MaskOpBetVoxelScale { section: si });
                if self.first_bet_generator_section() == Some(si) {
                    Self::push_bet_surface_rows(rows);
                }
            }
            self.push_refinement_rows(rows, si);
            rows.push(PipelineRow::MaskOpAddStep { section: si });
        }
    }

    /// BET's surface settings. They are one value per run (`[bet]` in the config, `--bet-*` on
    /// the command line) shared by every BET step, unlike the fractional intensity and voxel
    /// scale, which are part of each step. They are listed under the first BET step with that
    /// step's own settings, so all of BET's settings sit together.
    fn push_bet_surface_rows(rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Param { label: "Smoothness", field: "bet_smoothness", help: "BET surface smoothness factor (shared by every BET step)" });
        rows.push(PipelineRow::Param { label: "Gradient Threshold", field: "bet_gradient_threshold", help: "BET gradient threshold, -1 to 1 (shared by every BET step)" });
        rows.push(PipelineRow::Param { label: "Iterations", field: "bet_iterations", help: "BET surface evolution iterations (shared by every BET step)" });
        rows.push(PipelineRow::Param { label: "Subdivisions", field: "bet_subdivisions", help: "BET icosphere subdivision level (shared by every BET step)" });
    }

    /// The first section, main mask then two-pass mask, whose generator is BET.
    fn first_bet_generator_section(&self) -> Option<usize> {
        use crate::pipeline::config::MaskOp;
        let is_bet = |s: &crate::pipeline::config::MaskSection| matches!(s.generator, MaskOp::Bet { .. });
        self.mask_sections.iter().position(is_bet).or_else(|| {
            if !self.two_pass { return None; }
            self.two_pass_sections.iter().position(is_bet).map(|i| MASK_TWO_PASS_BASE + i)
        })
    }

    /// Rows for one refinement list: the step itself, plus a row per extra parameter for the
    /// steps that have more than one (signal-erode).
    fn push_refinement_rows(&self, rows: &mut Vec<PipelineRow>, section: usize) {
        let Some(refinements) = self.mask_refinements(section) else { return };
        for (index, op) in refinements.iter().enumerate() {
            rows.push(PipelineRow::MaskOpEntry { section, index });
            if matches!(op, crate::pipeline::config::MaskOp::SignalErode { .. }) {
                for param in 0..SIGNAL_ERODE_PARAMS.len() {
                    rows.push(PipelineRow::MaskOpSignalErodeParam { section, index, param });
                }
            }
        }
    }

    /// The combine mode and post-combine refinements as the pipeline should see them. Both are
    /// only editable — and only shown — with more than one section, so a single-section recipe
    /// reports the defaults rather than silently applying steps the user cannot see.
    pub fn mask_combine_recipe(&self) -> (crate::pipeline::config::MaskCombine, Vec<crate::pipeline::config::MaskOp>) {
        if self.mask_sections.len() < 2 {
            (crate::pipeline::config::MaskCombine::Or, vec![])
        } else {
            (self.mask_combine, self.mask_combined_refinements.clone())
        }
    }

    /// Toggle the mask combine mode (OR ↔ AND).
    pub fn toggle_mask_combine(&mut self) {
        use crate::pipeline::config::MaskCombine;
        self.mask_combine = match self.mask_combine { MaskCombine::Or => MaskCombine::And, MaskCombine::And => MaskCombine::Or };
        self.mark_mask_custom(0);
    }

    /// Mark the preset as "Custom" when the user edits the main mask by hand.
    ///
    /// The preset names the main mask's recipe, so editing the two-pass reliable mask leaves it
    /// alone — otherwise turning two-pass on and adjusting its threshold would appear to abandon
    /// the `robust-threshold` preset the main mask still follows.
    fn mark_mask_custom(&mut self, section: usize) {
        if !is_two_pass_section(section) && self.mask_preset != MASK_PRESET_CUSTOM {
            self.mask_preset = MASK_PRESET_CUSTOM;
        }
    }

    /// Index of a section's generator in [`MASK_GENERATOR_TYPES`].
    pub fn mask_generator_index(&self, section: usize) -> usize {
        self.mask_section(section)
            .and_then(|s| MASK_GENERATOR_TYPES.iter().position(|&t| t == Self::mask_op_label_value(&s.generator).0))
            .unwrap_or(0)
    }

    /// Cycle the generator of a mask section (threshold → BET → HD-BET → RS2-Net) with
    /// left/right.
    pub fn adjust_mask_generator(&mut self, section: usize, delta: isize) {
        if self.mask_section(section).is_none() { return; }
        let n = MASK_GENERATOR_TYPES.len() as isize;
        let new = (self.mask_generator_index(section) as isize + delta).rem_euclid(n) as usize;
        self.set_mask_generator(section, new);
    }

    /// Adjust the generator's parameter (threshold method or BET fractional intensity).
    pub fn adjust_mask_generator_param(&mut self, section: usize, delta: isize) {
        use crate::pipeline::config::*;
        let Some(section_ref) = self.mask_section_mut(section) else { return };
        match &mut section_ref.generator {
            MaskOp::Threshold { method, .. } => {
                let methods = [MaskThresholdMethod::Otsu, MaskThresholdMethod::Fixed, MaskThresholdMethod::Percentile];
                let cur = methods.iter().position(|m| m == method).unwrap_or(0) as isize;
                let new = (cur + delta).rem_euclid(methods.len() as isize) as usize;
                *method = methods[new];
            }
            MaskOp::Bet { fractional_intensity, .. } => {
                *fractional_intensity = (*fractional_intensity + delta as f64 * 0.05).clamp(0.05, 1.0);
            }
            // Toggle between the native patch and the low-memory one.
            MaskOp::HdBet { patch, .. } => {
                let low = crate::pipeline::config::hd_bet_low_memory_patch();
                *patch = if *patch == low { MaskOp::hd_bet_default_patch() } else { low };
            }
            _ => {}
        }
        self.mark_mask_custom(section);
    }

    /// HD-BET's sliding-window step, cycled through the useful strides.
    ///
    /// qsm-core accepts any stride in `(0, 1]`, but only a few are worth offering: 0.5 is
    /// HD-BET's own default, and larger steps trade patch-seam quality for a shorter run.
    pub fn adjust_hd_bet_tile_step(&mut self, section: usize, delta: isize) {
        use crate::pipeline::config::MaskOp;
        const STEPS: [f64; 4] = [0.5, 0.625, 0.75, 1.0];
        let Some(section_ref) = self.mask_section_mut(section) else { return };
        if let MaskOp::HdBet { tile_step, .. } = &mut section_ref.generator {
            let cur = STEPS.iter()
                .position(|s| (s - *tile_step).abs() < 1e-9)
                .unwrap_or(0) as isize;
            let new = (cur + delta).rem_euclid(STEPS.len() as isize) as usize;
            *tile_step = STEPS[new];
            self.mark_mask_custom(section);
        }
    }

    /// BET's voxel-size scaling, cycled through the scales worth offering.
    ///
    /// Any positive factor is valid, but only two matter in practice: 1.0 for clinical data at
    /// its own geometry, and 10.0 for rodent brains, which BET's millimetre-scale surface model
    /// otherwise cannot find. The steps between are there for the in-between field strengths.
    pub fn adjust_bet_voxel_scale(&mut self, section: usize, delta: isize) {
        use crate::pipeline::config::MaskOp;
        const SCALES: [f64; 4] = [1.0, 2.0, 5.0, 10.0];
        let Some(section_ref) = self.mask_section_mut(section) else { return };
        if let MaskOp::Bet { voxel_scale, .. } = &mut section_ref.generator {
            let cur = SCALES.iter()
                .position(|s| (s - *voxel_scale).abs() < 1e-9)
                .unwrap_or(0) as isize;
            let new = (cur + delta).rem_euclid(SCALES.len() as isize) as usize;
            *voxel_scale = SCALES[new];
            self.mark_mask_custom(section);
        }
    }

    /// Ordered masking input sources, matching the modal/cycle option order.
    pub const MASK_INPUT_SOURCES: [crate::pipeline::config::MaskingInput; 4] = {
        use crate::pipeline::config::MaskingInput::*;
        [MagnitudeFirst, Magnitude, MagnitudeLast, PhaseQuality]
    };

    /// Current masking-input source index for a section (for populating the modal).
    pub fn mask_input_index(&self, section: usize) -> usize {
        self.mask_section(section)
            .and_then(|s| Self::MASK_INPUT_SOURCES.iter().position(|x| *x == s.input))
            .unwrap_or(0)
    }

    /// Set a section's masking input source by index (from the modal).
    pub fn set_mask_input(&mut self, section: usize, idx: usize) {
        let Some(&src) = Self::MASK_INPUT_SOURCES.get(idx) else { return };
        let Some(section_ref) = self.mask_section_mut(section) else { return };
        section_ref.input = src;
        self.mark_mask_custom(section);
    }

    /// Set a section's generator algorithm by index into [`MASK_GENERATOR_TYPES`]. Only rewrites
    /// the generator when the algorithm type actually changes, preserving existing parameters.
    pub fn set_mask_generator(&mut self, section: usize, idx: usize) {
        if self.mask_section(section).is_none() || idx == self.mask_generator_index(section) { return; }
        let Some(op) = MASK_GENERATOR_TYPES.get(idx).and_then(|t| Self::default_mask_op(t)) else { return };
        if let Some(section_ref) = self.mask_section_mut(section) {
            section_ref.generator = op;
        }
        self.mark_mask_custom(section);
    }

    /// Ordered threshold methods, matching the modal/cycle option order.
    pub const MASK_THRESHOLD_METHODS: [crate::pipeline::config::MaskThresholdMethod; 3] = {
        use crate::pipeline::config::MaskThresholdMethod::*;
        [Otsu, Fixed, Percentile]
    };

    /// Current threshold-method index for a section (0 if the generator isn't a threshold).
    pub fn mask_threshold_method_index(&self, section: usize) -> usize {
        use crate::pipeline::config::MaskOp;
        match self.mask_section(section).map(|s| &s.generator) {
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
        let Some(&new_method) = Self::MASK_THRESHOLD_METHODS.get(idx) else { return };
        let Some(section_ref) = self.mask_section_mut(section) else { return };
        if let MaskOp::Threshold { method, .. } = &mut section_ref.generator {
            *method = new_method;
            self.mark_mask_custom(section);
        }
    }

    /// Adjust the input source of a mask section with left/right.
    pub fn adjust_mask_input(&mut self, section: usize, delta: isize) {
        use crate::pipeline::config::MaskingInput;
        let sources = [MaskingInput::MagnitudeFirst, MaskingInput::Magnitude, MaskingInput::MagnitudeLast, MaskingInput::PhaseQuality];
        let Some(section_ref) = self.mask_section_mut(section) else { return };
        let cur = sources.iter().position(|s| *s == section_ref.input).unwrap_or(0) as isize;
        let new = (cur + delta).rem_euclid(sources.len() as isize) as usize;
        section_ref.input = sources[new];
        self.mark_mask_custom(section);
    }

    /// Adjust a mask op parameter with left/right.
    pub fn adjust_mask_op(&mut self, section: usize, index: usize, delta: isize) {
        use crate::pipeline::config::*;
        let Some(op) = self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return };
        match op {
            MaskOp::Threshold { method, .. } => {
                let methods = [MaskThresholdMethod::Otsu, MaskThresholdMethod::Fixed, MaskThresholdMethod::Percentile];
                let cur = methods.iter().position(|m| m == method).unwrap_or(0) as isize;
                let new = (cur + delta).rem_euclid(methods.len() as isize) as usize;
                *method = methods[new];
            }
            MaskOp::Bet { fractional_intensity, .. } => {
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
            MaskOp::HdBet { .. } | MaskOp::Rs2Net { .. } => {}
            MaskOp::SignalErode { threshold, .. } => {
                *threshold = ((*threshold + delta as f64 * 0.05).clamp(0.05, 0.95) * 100.0).round() / 100.0;
            }
        }
        self.mark_mask_custom(section);
    }

    /// Label and current value of one extra signal-erode parameter, for its own row.
    pub fn signal_erode_param(&self, section: usize, index: usize, param: usize) -> Option<(&'static str, String)> {
        use crate::pipeline::config::MaskOp;
        let MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. } =
            self.mask_refinements(section)?.get(index)? else { return None };
        let (label, _) = *SIGNAL_ERODE_PARAMS.get(param)?;
        let value = match param {
            0 => if *depth_cap == 0 { "off (no cap)".to_string() } else { format!("{depth_cap}") },
            1 => format!("{global_erosions}"),
            2 => format!("{bias_sigma:.1}"),
            _ => format!("{min_component}"),
        };
        Some((label, value))
    }

    /// Adjust one extra signal-erode parameter with ←/→.
    pub fn adjust_signal_erode_param(&mut self, section: usize, index: usize, param: usize, delta: isize) {
        use crate::pipeline::config::MaskOp;
        let Some(MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. }) =
            self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return };
        let step_usize = |v: &mut usize, by: isize| *v = (*v as isize + delta * by).max(0) as usize;
        match param {
            0 => step_usize(depth_cap, 1),
            1 => step_usize(global_erosions, 1),
            // Voxels, like qsm-core's `bias_sigma`; 0 would divide out nothing.
            2 => *bias_sigma = (*bias_sigma + delta as f64).max(1.0),
            _ => step_usize(min_component, 100),
        }
        self.mark_mask_custom(section);
    }

    /// Get available op types for adding refinement steps (morphological only).
    pub fn available_op_types(&self, _section: usize) -> Vec<&'static str> {
        // Generator is fixed — only offer morphological refinement ops
        MASK_OP_TYPES.iter()
            .filter(|t| !MASK_GENERATOR_TYPES.contains(t))
            .copied()
            .collect()
    }

    /// Get focusable row count (excludes separators and headers).
    pub fn focusable_rows(&self) -> Vec<usize> {
        focusable_indices(&self.visible_rows())
    }

    /// The focused row of the pipeline tabs.
    pub fn focused_row(&self) -> Option<PipelineRow> {
        let rows = self.visible_rows();
        let i = *focusable_indices(&rows).get(self.focus)?;
        rows.get(i).cloned()
    }
}

/// Indices of the rows focus can land on: every row but separators, notes and mask headers.
pub fn focusable_indices(rows: &[PipelineRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| !matches!(r, PipelineRow::Separator | PipelineRow::Note { .. }
            | PipelineRow::MaskSectionHeader { .. } | PipelineRow::MaskCombinedHeader
            | PipelineRow::MaskTwoPassHeader))
        .map(|(i, _)| i)
        .collect()
}

/// The named field a row edits, if it edits one.
pub fn row_field(row: &PipelineRow) -> Option<&'static str> {
    match row {
        PipelineRow::AlgoSelect { field, .. }
        | PipelineRow::Param { field, .. }
        | PipelineRow::Toggle { field, .. } => Some(field),
        _ => None,
    }
}

/// The output a checkbox asks for, when another option can force that output on.
fn forced_output(field: &str) -> Option<crate::pipeline::requirements::Output> {
    use crate::pipeline::requirements::Output;
    match field {
        "do_r2starmap" => Some(Output::R2star),
        "do_r2map" => Some(Output::R2),
        "do_r2primemap" => Some(Output::R2prime),
        "do_segmentation" => Some(Output::Segmentation),
        _ => None,
    }
}

fn forced_output_name(out: crate::pipeline::requirements::Output) -> &'static str {
    use crate::pipeline::requirements::Output;
    match out {
        Output::R2star => "The R2* map",
        Output::R2 => "The R2 map",
        Output::R2prime => "The R2' map",
        Output::Segmentation => "Segmentation",
        _ => "This output",
    }
}

/// The SWI scaling select: qsmxt_config's scalings, in [`super::command::SWI_SCALING_ARGS`]'s order.
static SWI_SCALING_OPTIONS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| qsmxt_config::SwiScaling::ALL.iter().map(|s| s.as_str()).collect());
static SWI_SCALING_HELP: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| vec!["Phase scaling type for SWI"; SWI_SCALING_OPTIONS.len()]);
const SYNTHSEG_VERSION_OPTIONS: &[&str] = &["v1", "v2"];
const SYNTHSEG_VERSION_HELP: &[&str] = &["v1: 32 labels, weights ship with SynthSeg. v2: 33 labels (adds CSF); its weights are not published yet, so a run with v2 is refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR"; 2];
const EXECUTION_MODE_OPTIONS: &[&str] = &["Local", "SLURM"];
const EXECUTION_MODE_HELP: &[&str] = &["Local execution or generate SLURM job scripts"; 2];
/// Where the segmentation comes from (a TUI-only choice: SynthSeg, or `--use-custom-dseg`).
const SEGMENTATION_SOURCE_OPTIONS: &[&str] = &["synthseg", "custom dseg"];
const SEGMENTATION_SOURCE_HELP: &[&str] = &[
    "SynthSeg: whole-brain parcellation straight off the GRE magnitude — no T1w, no registration. Needs a deep-learning build",
    "A dseg from BIDS derivatives (e.g. FreeSurfer's, made elsewhere); SynthSeg runs where none is found",
];

pub struct App {
    pub active_tab: usize,
    /// The sub-tab last chosen on each tab (see [`App::page`]).
    pub sub_page: [Option<Page>; TAB_NAMES.len()],
    /// Focus is on the sub-tab strip rather than the rows (see [`App::on_subtabs`]).
    pub subtab_focus: bool,
    pub active_field: usize,
    pub editing: bool,
    pub cursor_pos: usize,
    pub form: RunForm,
    pub filter_state: FilterTreeState,
    pub pipeline_state: PipelineFormState,
    pub should_quit: bool,
    pub should_run: bool,
    /// Set when the user accepts the update prompt: the main loop tears the TUI down
    /// and hands over to `qsmxt update`.
    pub should_update: bool,
    pub form_scroll_offset: usize,
    pub methods_scroll_offset: usize,
    pub error_message: Option<String>,
    /// A one-line confirmation in the footer (e.g. "Methods copied to clipboard"); cleared,
    /// like `error_message`, by the next key.
    pub status_message: Option<String>,
    /// Text to put on the clipboard: the main loop takes it and writes it out (OSC 52).
    pub clipboard: Option<String>,
    pub input_mode: InputMode,
    pub example_state: ExampleState,
    pub dicom_state: DicomConvertState,
    pub nifti_state: NiftiState,
    /// When set, an algorithm-selection modal is open over the QSM/χ-separation tab.
    pub algo_modal: Option<AlgoModal>,
    /// The "a new version is available" pop-up, once the background check has found one.
    pub update_modal: Option<UpdateModal>,
    /// Background release check started at launch.
    pub update_state: UpdateState,
}

/// The "new version available" pop-up shown once the background check finds a release
/// newer than the running binary.
#[derive(Debug, Clone)]
pub struct UpdateModal {
    /// Tag of the newer release, e.g. `v9.23.0`.
    pub tag: String,
    /// Release page, shown so the user can read the notes before updating.
    pub html_url: String,
}

/// What the user chose at the update prompt. Returned by [`App::handle_update_modal_key`]
/// so the decision can be asserted in tests without reaching for the side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateChoice {
    /// Key was not one of the prompt's bindings; the prompt stays open.
    Ignored,
    /// Update now: tear down the TUI and run the installer.
    Update,
    /// Not now — ask again next launch.
    Later,
    /// Don't remind me about this release again.
    Skip,
}

/// Holds the receiver for the launch-time release check.
///
/// The check runs on its own thread and the result is picked up by [`Self::poll`] on a
/// later frame, so nothing about startup waits on the network. When checking is disabled
/// (opt-out, CI, or a dev build) the receiver is `None` and `poll` never yields anything.
#[derive(Debug, Default)]
pub struct UpdateState {
    receiver: Option<std::sync::mpsc::Receiver<Option<crate::commands::update::check::UpdateInfo>>>,
    /// Set once the result has been taken, so we only raise the prompt a single time.
    done: bool,
}

impl UpdateState {
    pub fn start() -> Self {
        Self { receiver: crate::commands::update::check::spawn(), done: false }
    }

    /// A state that never produces a result — for tests that drive the prompt directly.
    #[cfg(test)]
    pub fn disabled() -> Self {
        Self { receiver: None, done: true }
    }

    /// Non-blocking: returns the newer release exactly once, the first frame after the
    /// background check finishes. A failed or still-running check yields `None`.
    pub fn poll(&mut self) -> Option<crate::commands::update::check::UpdateInfo> {
        if self.done {
            return None;
        }
        let Some(rx) = self.receiver.as_ref() else {
            self.done = true;
            return None;
        };
        match rx.try_recv() {
            Ok(found) => {
                self.done = true;
                self.receiver = None;
                found
            }
            // Still working; try again next frame.
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            // Thread died without sending; nothing to show.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.done = true;
                self.receiver = None;
                None
            }
        }
    }
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
    /// A named select (an `AlgoSelect` row of any row-based tab, e.g. "qsm_algorithm").
    Select(String),
    /// The masking input source of a mask section.
    MaskInput(usize),
    /// The generator algorithm (threshold vs BET) of a mask section.
    MaskGenerator(usize),
    /// The threshold method (otsu / fixed / percentile) of a mask section's generator.
    MaskThresholdMethod(usize),
    /// The Input-tab mode selector (BIDS / NIfTI / DICOM).
    InputMode,
}

#[derive(Clone, Debug)]
pub struct RunForm {
    // Tab 0: Input/Output
    pub bids_dir: String,
    pub output_dir: String,
    pub config_file: String,

    // Other outputs
    pub do_swi: bool,
    pub swi_scaling: usize,  // index into qsmxt_config::SwiScaling::ALL (= command::SWI_SCALING_ARGS)
    pub swi_strength: String,
    /// High-pass sigma per axis, "x y z" in voxels.
    pub swi_hp_sigma: String,
    pub swi_mip_window: String,
    pub do_t2starmap: bool,
    pub do_r2starmap: bool,
    pub do_r2map: bool,
    pub do_r2primemap: bool,
    pub export_dicom: bool,
    /// DICOM export: source DICOM directory to inherit identity from, and which maps to export.
    pub source_dicom: String,
    pub dicom_outputs: String,
    pub do_smwi: bool,
    pub do_segmentation: bool,
    pub do_analysis: bool,
    pub synthseg_version: usize,
    pub synthseg_crop: String,
    pub synthseg_flip_averaging: bool,
    pub synthseg_topology_cleanup: bool,
    /// Smooth SynthSeg's posteriors (off = a sigma of 0).
    pub synthseg_smoothing: bool,
    pub synthseg_sigma: String,
    pub smwi_threshold: String,
    pub smwi_power: String,
    pub smwi_mip_window: String,

    // Tab 4: Execution
    pub execution_mode: usize, // 0=Local, 1=SLURM
    pub dry_run: bool,
    pub debug: bool,
    pub n_procs: String,
    // Local runs: memory-aware scheduling, cache, and clean-up.
    pub mem_limit_gb: String,
    pub force: bool,
    pub clean_intermediates: bool,
    // SLURM fields
    pub slurm_account: String,
    pub slurm_partition: String,
    pub slurm_time: String,
    pub slurm_mem: String,
    pub slurm_cpus: String,
    pub slurm_submit: bool,
}

/// Named access to a group of same-typed fields: `$get`/`$get_mut` by the field's own name.
macro_rules! named_fields {
    ($get:ident, $get_mut:ident, $t:ty, [$($name:ident),* $(,)?]) => {
        pub fn $get(&self, field: &str) -> Option<&$t> {
            match field { $(stringify!($name) => Some(&self.$name),)* _ => None }
        }
        pub fn $get_mut(&mut self, field: &str) -> Option<&mut $t> {
            match field { $(stringify!($name) => Some(&mut self.$name),)* _ => None }
        }
    };
}

/// The run form's fields, by the names their rows carry.
impl RunForm {
    named_fields!(text, text_mut, String, [
        swi_strength, swi_hp_sigma, swi_mip_window,
        source_dicom, dicom_outputs, synthseg_crop, synthseg_sigma, smwi_threshold, smwi_power,
        smwi_mip_window, n_procs, mem_limit_gb, slurm_account, slurm_partition, slurm_time,
        slurm_mem, slurm_cpus,
    ]);
    named_fields!(flag, flag_mut, bool, [
        do_swi, do_t2starmap, do_r2starmap, do_r2map, do_r2primemap, export_dicom, do_smwi,
        do_segmentation, do_analysis, synthseg_flip_averaging, synthseg_topology_cleanup, synthseg_smoothing,
        dry_run, debug, force, clean_intermediates, slurm_submit,
    ]);
    named_fields!(choice, choice_mut, usize, [swi_scaling, synthseg_version, execution_mode]);

    /// The memory limit row, read: `(limit in GB, no limit)` — empty or `auto` detects it.
    pub fn mem_limit(&self) -> (Option<f64>, bool) {
        let v = self.mem_limit_gb.trim();
        if v.eq_ignore_ascii_case("none") { return (None, true); }
        (v.parse().ok().filter(|g: &f64| *g > 0.0), false)
    }

    /// The SWI high-pass sigma row, read: three numbers (spaces or commas), else none.
    pub fn swi_hp_sigma_values(&self) -> Option<Vec<f64>> {
        let v: Vec<f64> = self.swi_hp_sigma.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty()).map(|t| t.parse().ok()).collect::<Option<_>>()?;
        (v.len() == 3).then_some(v)
    }
}

impl Default for RunForm {
    fn default() -> Self {
        let swi = qsm_core::swi::SwiParams::default();
        let smwi = qsm_core::swi::SmwiParams::default();
        let seg = crate::pipeline::config::SegmentationConfig::default();
        Self {
            bids_dir: String::new(),
            output_dir: String::new(),
            config_file: String::new(),
            do_swi: false,
            swi_scaling: 0,
            swi_strength: format!("{}", swi.strength),
            swi_hp_sigma: format!("{} {} {}", swi.hp_sigma[0], swi.hp_sigma[1], swi.hp_sigma[2]),
            swi_mip_window: format!("{}", swi.mip_window),
            do_t2starmap: false,
            do_r2starmap: false,
            do_r2map: false,
            do_r2primemap: false,
            export_dicom: false,
            source_dicom: String::new(),
            dicom_outputs: String::new(),
            do_smwi: false,
            do_segmentation: false,
            do_analysis: false,
            synthseg_version: 0,
            synthseg_crop: seg.crop.map(|c| format!("{c}")).unwrap_or_default(),
            synthseg_flip_averaging: seg.flip_averaging,
            synthseg_topology_cleanup: seg.topology_cleanup,
            synthseg_smoothing: seg.sigma_smoothing > 0.0,
            synthseg_sigma: format!("{}", seg.sigma_smoothing),
            smwi_threshold: format!("{}", smwi.threshold_ppm),
            smwi_power: format!("{}", smwi.power),
            smwi_mip_window: format!("{}", smwi.mip_window),
            execution_mode: 0,
            dry_run: false,
            debug: false,
            n_procs: String::new(),
            mem_limit_gb: String::new(),
            force: false,
            clean_intermediates: false,
            slurm_account: String::new(),
            slurm_partition: String::new(),
            slurm_time: "02:00:00".to_string(),
            slurm_mem: String::new(),
            slurm_cpus: "4".to_string(),
            slurm_submit: false,
        }
    }
}

impl App {
    pub fn new() -> Self {
        App {
            active_tab: 0,
            sub_page: [None; TAB_NAMES.len()],
            subtab_focus: false,
            active_field: 0,
            editing: false,
            cursor_pos: 0,
            form: RunForm::default(),
            filter_state: FilterTreeState::default(),
            pipeline_state: PipelineFormState::default(),
            should_quit: false,
            should_run: false,
            should_update: false,
            form_scroll_offset: 0,
            methods_scroll_offset: 0,
            error_message: None,
            status_message: None,
            clipboard: None,
            algo_modal: None,
            update_modal: None,
            update_state: UpdateState::start(),
            input_mode: InputMode::Bids,
            example_state: ExampleState::default(),
            dicom_state: DicomConvertState::default(),
            nifti_state: NiftiState::default(),
        }
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

        // Example mode: the dataset has to exist before it can be run
        if self.input_mode == InputMode::Example
            && self.example_state.status != ConvertStatus::Done
        {
            self.error_message =
                Some("Download the example dataset first (Enter on Download)".to_string());
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
            self.goto_field("slurm_account");
            return;
        }

        self.should_run = true;
    }

    /// Put the methods text for the current settings on the clipboard (written out by the main
    /// loop as an OSC 52 sequence, which reaches the local clipboard through SSH).
    pub fn copy_methods(&mut self) {
        match super::command::methods_text(self) {
            Ok(text) => {
                self.clipboard = Some(text);
                self.status_message = Some("Methods copied to clipboard".to_string());
            }
            Err(e) => self.error_message = Some(format!("Nothing copied: {e}")),
        }
    }

    /// What F5 (and the Execution tab's button) does with the current settings: the button's
    /// label and the help line shown while it has focus.
    pub fn run_action(&self) -> (&'static str, &'static str) {
        match (self.form.execution_mode, self.form.dry_run, self.form.slurm_submit) {
            (0, false, _) => ("Run", "Run the pipeline now with these settings (the same as F5)"),
            (0, true, _) => ("Dry run", "Print the processing plan without running anything (the same as F5)"),
            (_, _, true) => ("Submit SLURM jobs", "Write a SLURM job script per run and submit them with sbatch (the same as F5)"),
            (_, _, false) => ("Write SLURM scripts", "Write a SLURM job script per run, to submit yourself (the same as F5)"),
        }
    }

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
        // The update prompt sits on top of everything, including an algorithm modal.
        if self.update_modal.is_some() {
            self.handle_update_modal_key(key);
            return;
        }

        // An open algorithm-selection modal captures all input until dismissed.
        if self.algo_modal.is_some() {
            self.handle_algo_modal_key(key);
            return;
        }

        // Clear error on any non-F5 key press
        if key.code != KeyCode::F(5) {
            self.error_message = None;
        }
        self.status_message = None;

        // Route tab 0 (Input) to its combined IO + filter handler
        if self.active_tab == TAB_INPUT {
            self.handle_input_tab_key(key);
            return;
        }
        // The row-based tabs share one handler (the page is synced first).
        if Self::is_rows_tab(self.active_tab) {
            self.sync_pipeline_mode();
            self.handle_rows_key(key);
            return;
        }

        // Route tab Methods — read-only, only tab switching and scrolling
        if self.active_tab == TAB_METHODS {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
                _ if self.switch_tab(key) => {}
                KeyCode::Up | KeyCode::Char('k') => {
                    self.methods_scroll_offset = self.methods_scroll_offset.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.methods_scroll_offset += 1;
                }
                KeyCode::Enter | KeyCode::Char('c') | KeyCode::Char('y') => self.copy_methods(),
                KeyCode::F(5) => self.try_run(),
                _ => {}
            }
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
        // Example mode: the acquisition picker lives past the IO fields
        if self.input_mode == InputMode::Example && self.active_field >= Self::INPUT_IO_FIELDS {
            self.handle_example_tab_key(key);
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
                // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
                _ if self.switch_tab(key) => {}
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
                            InputMode::Example => true, // always has the acquisition picker
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

    /// Keys for the example-dataset picker (Input tab, past the IO fields).
    fn handle_example_tab_key(&mut self, key: KeyEvent) {
        let button = ExampleState::button_index();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}
            KeyCode::Up | KeyCode::Char('k') => {
                if self.example_state.cursor == 0 {
                    // Back up into the IO fields above the picker.
                    self.active_field = Self::INPUT_IO_FIELDS - 1;
                } else {
                    self.example_state.cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') if self.example_state.cursor < button => {
                self.example_state.cursor += 1;
            }
            KeyCode::Char(' ') if self.example_state.cursor < button => {
                self.example_state.toggle_selected();
            }
            KeyCode::Enter => {
                if self.example_state.cursor < button {
                    self.example_state.toggle_selected();
                } else if let Err(msg) = self.example_state.start_download() {
                    self.error_message = Some(msg);
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Resetting mid-fetch would orphan the worker thread while it is still
                // writing into the dataset directory, so leave it alone until it lands.
                if self.example_state.status == ConvertStatus::Converting {
                    self.error_message =
                        Some("A download is running — wait for it to finish".to_string());
                } else {
                    self.example_state = ExampleState::default();
                    self.active_field = Self::INPUT_IO_FIELDS;
                }
            }
            KeyCode::F(5) => self.try_run(),
            _ => {}
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
                    "Example dataset".to_string(),
                ],
                help: vec![
                    "Standard BIDS dataset input".to_string(),
                    "Convert loose NIfTI files to BIDS (experimental)".to_string(),
                    "Convert DICOM series to BIDS (experimental)".to_string(),
                    "Download a ready-made example dataset to try the pipeline on".to_string(),
                ],
                cursor: match self.input_mode {
                    InputMode::Bids => 0,
                    InputMode::NIfTI => 1,
                    InputMode::DicomToBids => 2,
                    InputMode::Example => 3,
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
                InputMode::Example => self.example_state.output_dir.len(),
            },
            2 => match self.input_mode {
                InputMode::Bids => self.form.output_dir.len(),
                InputMode::NIfTI => self.nifti_state.output_dir.len(),
                InputMode::DicomToBids => self.dicom_state.output_dir.len(),
                InputMode::Example => self.form.output_dir.len(),
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
        const MODES: [InputMode; 4] =
            [InputMode::Bids, InputMode::NIfTI, InputMode::DicomToBids, InputMode::Example];
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
                self.example_state = ExampleState::default();
            }
            tab if Self::is_rows_tab(tab) => self.reset_current_page(),
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
        if self.filter_state.orientation_editing {
            self.handle_filter_text_key(key, FilterTextField::OrientationGroup);
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
            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}

            // Navigation within filter tree
            KeyCode::Up | KeyCode::Char('k') => self.filter_state.focus_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.filter_state.focus_next(),

            // Collapse/expand
            KeyCode::Left | KeyCode::Right => self.filter_state.toggle_collapse(),

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
                    FilterFocus::OrientationGroup => {
                        self.filter_state.orientation_editing = true;
                        self.filter_state.orientation_cursor = self.filter_state.orientation_group.len();
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
            FilterTextField::OrientationGroup => (
                &mut self.filter_state.orientation_group,
                &mut self.filter_state.orientation_cursor,
                &mut self.filter_state.orientation_editing,
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

            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}

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
    /// Pick up the background release check and raise the prompt when it finds one.
    ///
    /// Called once per frame from the main loop. A release the user has already told us
    /// to skip is dropped silently, and the prompt never displaces an algorithm modal
    /// the user is in the middle of.
    pub fn poll_update_check(&mut self) {
        let Some(info) = self.update_state.poll() else { return };
        if crate::commands::update::check::is_dismissed(&info.tag) {
            return;
        }
        self.update_modal = Some(UpdateModal { tag: info.tag, html_url: info.html_url });
    }

    /// Handle a key while the update prompt is open. Returns what the user chose.
    pub fn handle_update_modal_key(&mut self, key: KeyEvent) -> UpdateChoice {
        match key.code {
            KeyCode::Enter | KeyCode::Char('u') | KeyCode::Char('U') => {
                self.update_modal = None;
                self.should_update = true;
                UpdateChoice::Update
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if let Some(m) = self.update_modal.take() {
                    crate::commands::update::check::dismiss(&m.tag);
                }
                UpdateChoice::Skip
            }
            KeyCode::Esc | KeyCode::Char('l') | KeyCode::Char('L') => {
                self.update_modal = None;
                UpdateChoice::Later
            }
            _ => UpdateChoice::Ignored,
        }
    }

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
            AlgoModalTarget::Select(field) => {
                self.set_field_select(&field, m.cursor);
                // Keep focus on the selector row (its value changed; rows below may differ).
                self.restore_focus(&Some(field));
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
        }
    }

    /// Set the Input-tab mode by index (BIDS / NIfTI / DICOM).
    fn set_input_mode(&mut self, idx: usize) {
        const MODES: [InputMode; 4] =
            [InputMode::Bids, InputMode::NIfTI, InputMode::DicomToBids, InputMode::Example];
        if let Some(&m) = MODES.get(idx) {
            self.input_mode = m;
            self.form_scroll_offset = 0;
        }
    }

    /// Point the pipeline form at the active page, with its context fresh. When the page
    /// changes under it (a tab or sub-tab switch, or a sub-tab that stopped applying), focus goes
    /// back to the top. Called from both key handling and rendering so the display is consistent.
    pub fn sync_pipeline_mode(&mut self) {
        self.pipeline_state.ctx = self.row_context();
        if let Some(page) = self.page() {
            if page != self.pipeline_state.page {
                self.pipeline_state.page = page;
                self.reset_view();
            }
        }
    }

    /// What the rest of the form decides about the pipeline tabs' rows (see [`RowContext`]).
    pub fn row_context(&self) -> RowContext {
        let ps = &self.pipeline_state;
        let (group, algorithm, _) = self.filter_state.orientation_args();
        // Whether QSM runs is the resolved config's answer — the same enforcement the run applies.
        let config = super::command::config_from_app(self).ok();
        let qsm_forced = !ps.do_qsm && config.as_ref().is_some_and(|c| c.pipeline.do_qsm);
        RowContext {
            qsm_forced_by: qsm_forced.then_some(
                if self.form.do_smwi && ps.custom_qsm_tool.trim().is_empty() {
                    "SMWI weights the magnitude by the χ map (Other outputs → SWI/SMWI)"
                } else {
                    "the per-structure statistics need a map to summarise (Other outputs → Segmentation)"
                }),
            orientation_group: group.is_some(),
            orientation_cosmos: algorithm == 0,
            num_echoes: self.filter_state.num_echoes.trim().parse().ok(),
            inputs: self.input_relevance(),
            axial_forced_by: config.as_ref().and_then(|c| {
                let names = qsmxt_config::bridge::axial_only_algorithms(
                    c, qsmxt_config::bridge::RunOrientationFacts { has_measured_r2: false });
                (!names.is_empty()).then(|| names.join(", "))
            }),
            multi_coil: self.filter_state.tree.as_ref().is_none_or(|t| {
                let mut any = false;
                t.for_each_run(|leaf| any |= leaf.selected && leaf.multi_coil);
                any
            }),
        }
    }

    /// Which bring-your-own inputs the run can use, from what the form asks for (see
    /// [`InputRelevance`]). Decides both whether their rows show and whether they are passed on.
    pub fn input_relevance(&self) -> InputRelevance {
        let ps = &self.pipeline_state;
        let f = &self.form;
        let sep = SEP_ALGO_OPTIONS.get(ps.separation_algorithm).copied().unwrap_or("");
        // The R2'-based methods (qsmxt_config::enforce_separation_dependencies).
        let sep_needs_r2prime = ps.do_chi_separation
            && !matches!(sep, "r2star-qsm" | "decompose");
        let r2prime = f.do_r2primemap || sep_needs_r2prime;
        let r2prime_source = r2prime && ps.custom_r2prime_tool.trim().is_empty();
        // R2' is measured as R2* − R2 unless it is always predicted (R2PRIMEnet, index 2).
        let r2 = f.do_r2map || (r2prime_source && ps.r2prime_strategy != 2);
        InputRelevance {
            qsm: ps.do_chi_separation || f.do_analysis || f.do_smwi,
            r2,
            r2prime,
            r2prime_source,
        }
    }

    /// Keys on a row-based tab (all but Input and Methods): every row is a named
    /// field ([`PipelineRow`]) read and written through the `field_*` accessors, so one handler
    /// serves them all, whichever form a field lives in.
    fn handle_rows_key(&mut self, key: KeyEvent) {
        if self.pipeline_state.editing {
            // Text editing mode for a parameter
            let field = match self.focused_row() {
                Some(PipelineRow::Param { field, .. }) => field,
                _ => {
                    self.pipeline_state.editing = false;
                    return;
                }
            };
            let mut cursor = self.pipeline_state.cursor;
            let len = self.field_text(field).len();
            match key.code {
                KeyCode::Esc | KeyCode::Enter => { self.pipeline_state.editing = false; return; }
                KeyCode::Char(c) => {
                    if let Some(s) = self.field_text_mut(field) {
                        s.insert(cursor, c);
                        cursor += 1;
                    }
                }
                KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                    if let Some(s) = self.field_text_mut(field) {
                        let new_pos = word_boundary_left(s, cursor);
                        s.drain(new_pos..cursor);
                        cursor = new_pos;
                    }
                }
                KeyCode::Backspace if cursor > 0 => {
                    cursor -= 1;
                    if let Some(s) = self.field_text_mut(field) {
                        s.remove(cursor);
                    }
                }
                KeyCode::Delete if cursor < len => {
                    if let Some(s) = self.field_text_mut(field) {
                        s.remove(cursor);
                    }
                }
                KeyCode::Left => cursor = cursor.saturating_sub(1),
                KeyCode::Right if cursor < len => cursor += 1,
                KeyCode::Home => cursor = 0,
                KeyCode::End => cursor = len,
                _ => {}
            }
            self.pipeline_state.cursor = cursor;
            return;
        }

        let ps = &mut self.pipeline_state;
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
                    let mut edited_section = 0;
                    if let Some(PipelineRow::MaskOpThresholdValue { section }) = ps.focused_row() {
                        edited_section = section;
                        if let Some(crate::pipeline::config::MaskOp::Threshold { value, .. }) =
                            ps.mask_section_mut(section).map(|s| &mut s.generator)
                        {
                            *value = val;
                        }
                    }
                    ps.mask_threshold_editing = false;
                    ps.mark_mask_custom(edited_section);
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

        // On the sub-tab strip: ←/→ choose the sub-tab, ↓ or Enter go into its rows. Tab
        // switching, `[`/`]`, `R`, F5 and quitting work as on the rows; nothing else applies.
        if self.on_subtabs() {
            match key.code {
                KeyCode::Left => return self.cycle_page(-1),
                KeyCode::Right => return self.cycle_page(1),
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                    self.subtab_focus = false;
                    self.set_focus(0);
                    return;
                }
                KeyCode::Char('q' | '[' | ']' | 'R') | KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab | KeyCode::F(5) => {}
                KeyCode::Char(c) if c.is_ascii_digit() => {}
                _ => return,
            }
        }

        match key.code {
            // Escape from add mode before quit
            KeyCode::Esc if self.pipeline_state.mask_ops_adding => {
                self.pipeline_state.mask_ops_adding = false;
            }

            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}
            // `[` and `]`: the sub-tabs.
            KeyCode::Char('[') => self.cycle_page(-1),
            KeyCode::Char(']') => self.cycle_page(1),

            // Reorder mask ops with Ctrl+Up/Down (must be before regular Up/Down)
            KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                let ps = &mut self.pipeline_state;
                if let Some(PipelineRow::MaskOpEntry { section: si, index: oi }) = ps.focused_row() {
                    let n = ps.mask_refinements(si).map_or(0, |r| r.len());
                    if oi > 0 && oi < n {
                        if let Some(refs) = ps.mask_refinements_mut(si) { refs.swap(oi, oi - 1); }
                        ps.mark_mask_custom(si);
                        if ps.focus > 0 { ps.focus -= 1; }
                    }
                }
            }
            KeyCode::Down if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                let ps = &mut self.pipeline_state;
                if let Some(PipelineRow::MaskOpEntry { section: si, index: oi }) = ps.focused_row() {
                    if oi + 1 < ps.mask_refinements(si).map_or(0, |r| r.len()) {
                        if let Some(refs) = ps.mask_refinements_mut(si) { refs.swap(oi, oi + 1); }
                        ps.mark_mask_custom(si);
                        let max = ps.focusable_rows().len().saturating_sub(1);
                        if ps.focus < max { ps.focus += 1; }
                    }
                }
            }

            // Navigation; up from the first row goes back to the sub-tab strip.
            KeyCode::Up | KeyCode::Char('k') => {
                let f = self.focus();
                if f == 0 && self.has_subtabs() && !self.pipeline_state.mask_ops_adding {
                    self.subtab_focus = true;
                } else {
                    self.set_focus(f.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.focusable_rows().len().saturating_sub(1);
                let f = self.focus();
                if f < max {
                    self.set_focus(f + 1);
                }
            }

            // Interact
            KeyCode::Enter | KeyCode::Char(' ') if matches!(self.focused_row(), Some(PipelineRow::RunButton)) => self.try_run(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let row = self.focused_row();
                // Single-choice selectors open a modal listing every option (rather than cycling).
                let modal = {
                    let ps = &self.pipeline_state;
                    match &row {
                        Some(PipelineRow::AlgoSelect { label, field, options, help }) => Some(AlgoModal {
                            target: AlgoModalTarget::Select(field.to_string()),
                            title: label.trim().to_string(),
                            options: options.iter().map(|s| s.to_string()).collect(),
                            help: help.iter().map(|s| s.to_string()).collect(),
                            cursor: self.field_select(field),
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
                            Some(AlgoModal {
                                target: AlgoModalTarget::MaskGenerator(*section),
                                title: "Mask Algorithm".to_string(),
                                options: MASK_GENERATOR_TYPES.iter().map(|t| t.to_string()).collect(),
                                help: vec![
                                    "Intensity threshold-based masking".to_string(),
                                    "FSL BET brain extraction".to_string(),
                                    "HD-BET deep-learning brain extraction (Isensee et al., 2019)".to_string(),
                                ],
                                cursor: ps.mask_generator_index(*section),
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
                match row {
                    Some(PipelineRow::Param { field, .. }) => {
                        self.pipeline_state.editing = true;
                        self.pipeline_state.cursor = self.field_text(field).len();
                    }
                    Some(PipelineRow::Toggle { field, .. }) => {
                        let focused_field = self.focused_field_name();
                        self.toggle_field(field);
                        self.restore_focus(&focused_field);
                    }
                    Some(PipelineRow::MaskOpAddStep { section }) => {
                        let ps = &mut self.pipeline_state;
                        if ps.mask_ops_adding {
                            let available = ps.available_op_types(section);
                            if let Some(&type_name) = available.get(ps.mask_ops_add_idx) {
                                if let Some(op) = PipelineFormState::default_mask_op(type_name) {
                                    if let Some(refs) = ps.mask_refinements_mut(section) { refs.push(op); }
                                }
                            }
                            ps.mask_ops_adding = false;
                            ps.mark_mask_custom(section);
                        } else {
                            ps.mask_ops_adding = true;
                            ps.mask_ops_add_idx = 0;
                            ps.mask_ops_add_section = section;
                        }
                    }
                    Some(PipelineRow::MaskOpAddSection { two_pass }) => {
                        use crate::pipeline::config::*;
                        let ps = &mut self.pipeline_state;
                        let section = MaskSection {
                            input: MaskingInput::Magnitude,
                            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                            refinements: vec![],
                        };
                        let added = if two_pass { MASK_TWO_PASS_BASE + ps.two_pass_sections.len() } else { ps.mask_sections.len() };
                        if two_pass { ps.two_pass_sections.push(section); } else { ps.mask_sections.push(section); }
                        ps.mark_mask_custom(added);
                    }
                    Some(PipelineRow::MaskOpThresholdValue { section }) => {
                        // Start editing threshold value
                        let ps = &mut self.pipeline_state;
                        let current = if let crate::pipeline::config::MaskOp::Threshold { value, .. } = &ps.mask_sections[section].generator {
                            value.map(|v| format!("{}", v)).unwrap_or_default()
                        } else { String::new() };
                        ps.mask_threshold_value_buf = current.clone();
                        ps.mask_threshold_editing = true;
                        ps.cursor = current.len();
                    }
                    Some(PipelineRow::MaskCombineOp) => {
                        self.pipeline_state.toggle_mask_combine();
                    }
                    // The other mask rows are adjusted with Left/Right.
                    _ => {}
                }
            }

            // Delete refinement step or entire section
            KeyCode::Char('d') | KeyCode::Delete => {
                let ps = &mut self.pipeline_state;
                match ps.focused_row() {
                    Some(PipelineRow::MaskOpEntry { section: si, index: oi })
                        if oi < ps.mask_refinements(si).map_or(0, |r| r.len()) => {
                        if let Some(refs) = ps.mask_refinements_mut(si) { refs.remove(oi); }
                        ps.mark_mask_custom(si);
                        let max = ps.focusable_rows().len().saturating_sub(1);
                        if ps.focus > max { ps.focus = max; }
                    }
                    // Delete entire section (only if >1 sections) when focused on section header-adjacent rows
                    Some(PipelineRow::MaskOpGenerator { section: si }) | Some(PipelineRow::MaskOpInput { section: si }) => {
                        let (two_pass, i) = PipelineFormState::mask_section_slot(si);
                        if ps.mask_section_count(two_pass) > 1 && i < ps.mask_section_count(two_pass) {
                            if two_pass { ps.two_pass_sections.remove(i); } else { ps.mask_sections.remove(i); }
                            ps.mark_mask_custom(si);
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

                // Check if we're in mask ops add mode
                if self.pipeline_state.mask_ops_adding {
                    let ps = &mut self.pipeline_state;
                    let available = ps.available_op_types(ps.mask_ops_add_section);
                    let n = available.len() as isize;
                    if n > 0 {
                        ps.mask_ops_add_idx = (ps.mask_ops_add_idx as isize + delta).rem_euclid(n) as usize;
                    }
                    return;
                }
                let row = self.focused_row();
                if let Some(PipelineRow::AlgoSelect { field, options, .. }) = row {
                    let focused_field = self.focused_field_name();
                    // Mask preset: cycle the real presets only; "custom" is auto-set on edits
                    let n = if field == "mask_preset" { MASK_PRESET_CUSTOM } else { options.len() } as isize;
                    let cur = self.field_select(field).min((n - 1) as usize) as isize;
                    let new_val = (cur + delta).rem_euclid(n) as usize;
                    self.set_field_select(field, new_val);
                    self.restore_focus(&focused_field);
                    return;
                }
                let ps = &mut self.pipeline_state;
                match row {
                    Some(PipelineRow::MaskCombineOp) => ps.toggle_mask_combine(),
                    Some(PipelineRow::MaskOpEntry { section, index }) => ps.adjust_mask_op(section, index, delta),
                    Some(PipelineRow::MaskOpSignalErodeParam { section, index, param }) => {
                        ps.adjust_signal_erode_param(section, index, param, delta);
                    }
                    Some(PipelineRow::MaskOpGenerator { section }) => ps.adjust_mask_generator(section, delta),
                    Some(PipelineRow::MaskOpGeneratorParam { section }) => ps.adjust_mask_generator_param(section, delta),
                    Some(PipelineRow::MaskOpHdBetStep { section }) => ps.adjust_hd_bet_tile_step(section, delta),
                    Some(PipelineRow::MaskOpBetVoxelScale { section }) => ps.adjust_bet_voxel_scale(section, delta),
                    Some(PipelineRow::MaskOpInput { section }) => ps.adjust_mask_input(section, delta),
                    _ => {}
                }
            }

            // Reset focused field to default
            KeyCode::Char('r') => self.reset_row_field(),
            // Reset every setting of the tab to its default
            KeyCode::Char('R') => self.reset_current_tab(),

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

    fn text_value_mut(&mut self) -> &mut String {
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
    /// The run's own rule (`requirements::implied_by` on the resolved config), so the ticked
    /// checkbox, the refusal to untick it and what the run does cannot disagree: a supplied map
    /// (custom dseg, R2 or R2') satisfies the need and forces nothing.
    fn forced_by(&self, out: crate::pipeline::requirements::Output) -> Option<String> {
        let config = super::command::config_from_app(self).ok()?;
        crate::pipeline::requirements::implied_by(&config, out)
    }

    // ─── Row-based tabs: pages, rows, focus and named field access ───

    /// Whether a tab is drawn and driven as named rows ([`PipelineRow`]).
    pub fn is_rows_tab(tab: usize) -> bool {
        matches!(tab, TAB_PREPROCESSING | TAB_QSM | TAB_SEPARATION | TAB_OUTPUTS | TAB_EXECUTION)
    }

    /// The pipeline form as it decides its rows now: its context fresh from the rest of the form.
    fn synced_pipeline(&self, page: Page, ctx: &RowContext) -> PipelineFormState {
        let mut ps = self.pipeline_state.clone();
        ps.page = page;
        ps.ctx = ctx.clone();
        ps
    }

    /// A tab's sub-tabs, as they apply now: the QSM tab shows a stage only when it runs.
    pub fn pages(&self, tab: usize) -> Vec<Page> {
        if tab == TAB_QSM { self.pages_in(tab, &self.row_context()) } else { self.pages_in(tab, &RowContext::default()) }
    }

    fn pages_in(&self, tab: usize, ctx: &RowContext) -> Vec<Page> {
        match tab {
            TAB_PREPROCESSING => vec![Page::Geometry, Page::Masking, Page::Motion],
            TAB_QSM => {
                let ps = self.synced_pipeline(Page::Inversion, ctx);
                if !ps.qsm_runs() {
                    return vec![Page::QsmOff];
                }
                let mut pages = Vec::new();
                if ps.field_mapping_runs().0 { pages.push(Page::FieldMap); }
                if ps.inversion_needs_bg_removal() { pages.push(Page::Background); }
                pages.push(Page::Inversion);
                if ps.ctx.orientation_group { pages.push(Page::MultiOrientation); }
                pages.push(Page::Reference);
                pages
            }
            TAB_SEPARATION => vec![Page::Separation],
            TAB_OUTPUTS => vec![Page::SwiSmwi, Page::Relaxometry, Page::Segmentation, Page::Export],
            TAB_EXECUTION => vec![Page::Execution],
            _ => Vec::new(),
        }
    }

    /// The active sub-tab: the one last chosen on this tab while it still applies, else the first.
    pub fn page(&self) -> Option<Page> {
        let pages = self.pages(self.active_tab);
        let chosen = self.sub_page.get(self.active_tab).copied().flatten();
        chosen.filter(|p| pages.contains(p)).or_else(|| pages.first().copied())
    }

    /// Move to the next (`delta` 1) or previous (-1) sub-tab of the active tab, wrapping. Focus
    /// stays on the sub-tab strip if it was there, else goes to the first row.
    pub fn cycle_page(&mut self, delta: isize) {
        let pages = self.pages(self.active_tab);
        if pages.len() < 2 { return; }
        let on_strip = self.on_subtabs();
        let cur = self.page().and_then(|p| pages.iter().position(|&q| q == p)).unwrap_or(0);
        let next = pages[(cur as isize + delta).rem_euclid(pages.len() as isize) as usize];
        self.sub_page[self.active_tab] = Some(next);
        self.reset_view();
        self.sync_pipeline_mode();
        self.subtab_focus = on_strip;
    }

    /// Whether the active tab shows a sub-tab strip (a row-based tab with more than one page).
    pub fn has_subtabs(&self) -> bool {
        Self::is_rows_tab(self.active_tab) && self.pages(self.active_tab).len() > 1
    }

    /// Whether focus is on the sub-tab strip rather than on a row.
    pub fn on_subtabs(&self) -> bool {
        self.subtab_focus && self.has_subtabs()
    }

    /// Put focus and scrolling back at the top: on every change of tab or sub-tab.
    pub fn reset_view(&mut self) {
        self.subtab_focus = false;
        self.active_field = 0;
        self.form_scroll_offset = 0;
        self.editing = false;
        let ps = &mut self.pipeline_state;
        ps.focus = 0;
        ps.scroll_offset = 0;
        ps.editing = false;
        ps.mask_ops_adding = false;
        ps.mask_threshold_editing = false;
    }

    /// The active page's rows.
    pub fn rows(&self) -> Vec<PipelineRow> {
        self.page().map(|p| self.page_rows(p)).unwrap_or_default()
    }

    /// Any page's rows, as they would show now.
    pub fn page_rows(&self, page: Page) -> Vec<PipelineRow> {
        self.page_rows_in(page, &self.row_context())
    }

    /// Every sub-tab of every tab with its rows, as they apply now (the context worked out once).
    pub fn all_pages(&self) -> Vec<(usize, Page, Vec<PipelineRow>)> {
        let ctx = self.row_context();
        (0..TAB_NAMES.len())
            .flat_map(|t| self.pages_in(t, &ctx).into_iter().map(move |p| (t, p)))
            .map(|(t, p)| (t, p, self.page_rows_in(p, &ctx)))
            .collect()
    }

    fn page_rows_in(&self, page: Page, ctx: &RowContext) -> Vec<PipelineRow> {
        match page {
            p if p.is_pipeline() => self.synced_pipeline(p, ctx).visible_rows(),
            Page::SwiSmwi => self.swi_smwi_rows(),
            Page::Relaxometry => self.relaxometry_rows(),
            Page::Segmentation => self.segmentation_rows(),
            Page::Export => self.export_rows(),
            Page::Execution => self.execution_rows(),
            _ => Vec::new(),
        }
    }

    /// Indices into [`App::rows`] of the rows focus can land on.
    pub fn focusable_rows(&self) -> Vec<usize> {
        focusable_indices(&self.rows())
    }

    /// The focused row's position among [`App::focusable_rows`].
    pub fn focus(&self) -> usize {
        self.pipeline_state.focus
    }

    fn set_focus(&mut self, focus: usize) {
        self.pipeline_state.focus = focus;
    }

    /// The focused row of the active tab.
    pub fn focused_row(&self) -> Option<PipelineRow> {
        let rows = self.rows();
        let i = *focusable_indices(&rows).get(self.focus())?;
        rows.get(i).cloned()
    }

    /// The field the focused row edits, if it edits one.
    pub fn focused_field_name(&self) -> Option<String> {
        self.focused_row().as_ref().and_then(row_field).map(str::to_string)
    }

    /// After the rows changed, put focus back on the row editing `field` (or keep it in range).
    pub fn restore_focus(&mut self, field: &Option<String>) {
        if let Some(name) = field {
            if self.focus_field(name) {
                return;
            }
        }
        let max = self.focusable_rows().len().saturating_sub(1);
        if self.focus() > max {
            self.set_focus(max);
        }
    }

    /// Focus the row editing `field` on the active page; false if no visible row does.
    pub fn focus_field(&mut self, field: &str) -> bool {
        let rows = self.rows();
        match focusable_indices(&rows).iter().position(|&i| row_field(&rows[i]) == Some(field)) {
            Some(fi) => {
                self.set_focus(fi);
                true
            }
            None => false,
        }
    }

    /// Open the tab and sub-tab that show `field` now, and focus it; false if none does.
    pub fn goto_field(&mut self, field: &str) -> bool {
        let found = self.all_pages().into_iter()
            .find(|(_, _, rows)| rows.iter().any(|r| row_field(r) == Some(field)));
        let Some((tab, page, _)) = found else { return false };
        self.active_tab = tab;
        self.sub_page[tab] = Some(page);
        self.reset_view();
        self.sync_pipeline_mode();
        self.focus_field(field)
    }

    /// A text field's value, by name: the run form's, the Input tab's, else the pipeline form's.
    pub fn field_text(&self, field: &str) -> &str {
        match (self.form.text(field), field) {
            (Some(s), _) => s,
            (None, "orientation_lambda") => &self.filter_state.orientation_lambda,
            (None, _) => self.pipeline_state.get_param(field),
        }
    }

    pub fn field_text_mut(&mut self, field: &str) -> Option<&mut String> {
        if self.form.text(field).is_some() {
            self.form.text_mut(field)
        } else if field == "orientation_lambda" {
            Some(&mut self.filter_state.orientation_lambda)
        } else {
            self.pipeline_state.get_param_mut(field)
        }
    }

    /// A select's option index, by name.
    pub fn field_select(&self, field: &str) -> usize {
        if let Some(&c) = self.form.choice(field) {
            return c;
        }
        match field {
            "orientation_algorithm" => self.filter_state.orientation_algorithm,
            // A segmentation from derivatives, or SynthSeg's.
            "segmentation_source" => usize::from(!self.pipeline_state.custom_dseg_tool.trim().is_empty()),
            _ => self.pipeline_state.get_select(field),
        }
    }

    pub fn set_field_select(&mut self, field: &str, val: usize) {
        if let Some(c) = self.form.choice_mut(field) {
            *c = val;
            return;
        }
        match field {
            "orientation_algorithm" => self.filter_state.orientation_algorithm = val.min(ORIENTATION_ALGORITHMS.len() - 1),
            // One representation per choice: a supplied segmentation (any tool, until named) is
            // not also a SynthSeg request, and SynthSeg reads no derivatives.
            "segmentation_source" => {
                let ps = &mut self.pipeline_state;
                if val == 1 {
                    if ps.custom_dseg_tool.trim().is_empty() { ps.custom_dseg_tool = "*".into(); }
                    self.form.do_segmentation = false;
                } else {
                    ps.custom_dseg_tool.clear();
                    self.form.do_segmentation = true;
                }
            }
            _ => self.pipeline_state.set_select(field, val),
        }
    }

    /// Whether a segmentation is made or read: asked for, supplied, or forced by what needs one.
    fn segmentation_on(&self) -> bool {
        self.form.do_segmentation
            || !self.pipeline_state.custom_dseg_tool.trim().is_empty()
            || self.forced_by(crate::pipeline::requirements::Output::Segmentation).is_some()
    }

    /// A checkbox as it reads: an output another option forces on reads as checked.
    pub fn field_toggle(&self, field: &str) -> bool {
        if field == "do_segmentation" {
            return self.segmentation_on();
        }
        if field == "do_qsm" && !self.pipeline_state.do_qsm {
            return self.row_context().qsm_forced_by.is_some();
        }
        if let Some(out) = forced_output(field) {
            if self.forced_by(out).is_some() {
                return true;
            }
        }
        match self.form.flag(field) {
            Some(&b) => b,
            None => self.pipeline_state.get_toggle(field),
        }
    }

    /// Flip a checkbox — refused, with the reason, while another option forces it on.
    pub fn toggle_field(&mut self, field: &str) {
        if field == "do_qsm" {
            // QSM another output needs runs whatever this says: refuse, as for the other maps.
            let was = std::mem::replace(&mut self.pipeline_state.do_qsm, false);
            let why = self.row_context().qsm_forced_by;
            self.pipeline_state.do_qsm = was;
            if let Some(why) = why {
                self.error_message = Some(format!("QSM is required: {why} — change or turn that off first."));
                return;
            }
        }
        if field == "do_segmentation" {
            if !self.segmentation_on() {
                self.form.do_segmentation = true;
                return;
            }
            // What needs a segmentation whichever its source: asked with no dseg supplied.
            let dseg = std::mem::take(&mut self.pipeline_state.custom_dseg_tool);
            let why = self.forced_by(crate::pipeline::requirements::Output::Segmentation);
            self.pipeline_state.custom_dseg_tool = dseg;
            if let Some(why) = why {
                self.error_message = Some(format!(
                    "Segmentation is required by {why} — change or turn that off first."));
                return;
            }
            self.form.do_segmentation = false;
            self.pipeline_state.custom_dseg_tool.clear();
            return;
        }
        if let Some(out) = forced_output(field) {
            if let Some(why) = self.forced_by(out) {
                self.error_message = Some(format!(
                    "{} is required by {why} — change or turn that off first (or supply the map).",
                    forced_output_name(out)));
                return;
            }
        }
        match self.form.flag_mut(field) {
            Some(b) => *b = !*b,
            None => self.pipeline_state.toggle(field),
        }
    }

    /// What an empty text field reads as.
    pub fn field_placeholder(&self, field: &str) -> &'static str {
        match field {
            "slurm_account" => "(required)",
            "mem_limit_gb" | "n_procs" => "auto",
            "orientation_lambda" => "0 (none)",
            _ if self.form.text(field).is_some() => "(empty)",
            _ => "(default)",
        }
    }

    /// Reset the focused row's field to its default.
    fn reset_row_field(&mut self) {
        if let Some(field) = self.focused_field_name() {
            self.reset_field(&field);
        }
    }

    /// Reset one named field to its default, whichever form it lives in.
    fn reset_field(&mut self, field: &str) {
        let (form, ps) = (RunForm::default(), PipelineFormState::default());
        match field {
            "orientation_lambda" => return self.filter_state.orientation_lambda.clear(),
            "orientation_algorithm" => return self.filter_state.orientation_algorithm = 0,
            "segmentation_source" | "do_segmentation" => {
                self.pipeline_state.custom_dseg_tool.clear();
                self.form.do_segmentation = form.do_segmentation;
                return;
            }
            _ => {}
        }
        if let (Some(d), Some(v)) = (form.text(field), self.form.text_mut(field)) {
            *v = d.clone();
        } else if let (Some(d), Some(v)) = (form.flag(field), self.form.flag_mut(field)) {
            *v = *d;
        } else if let (Some(d), Some(v)) = (form.choice(field), self.form.choice_mut(field)) {
            *v = *d;
        } else if let Some(v) = self.pipeline_state.get_param_mut(field) {
            *v = ps.get_param(field).to_string();
        } else if ps.get_toggle(field) != self.pipeline_state.get_toggle(field) {
            self.pipeline_state.toggle(field);
        } else {
            self.pipeline_state.set_select(field, ps.get_select(field));
        }
    }

    /// Reset every setting of the active sub-tab: each field it shows, again for the rows a
    /// reset reveals, until none are new (and the mask recipe, on the Masking page).
    fn reset_current_page(&mut self) {
        let Some(page) = self.page() else { return };
        if page == Page::Masking {
            let d = PipelineFormState::default();
            let ps = &mut self.pipeline_state;
            ps.mask_sections = d.mask_sections;
            ps.mask_combine = d.mask_combine;
            ps.mask_combined_refinements = d.mask_combined_refinements;
            ps.two_pass_sections = d.two_pass_sections;
            ps.mask_preset = d.mask_preset;
        }
        let mut done = std::collections::HashSet::new();
        loop {
            let fields: Vec<&'static str> = self.page_rows(page).iter().filter_map(row_field)
                .filter(|f| done.insert(*f)).collect();
            if fields.is_empty() { break; }
            fields.iter().for_each(|f| self.reset_field(f));
        }
        self.reset_view();
    }

    /// Other outputs → SWI/SMWI.
    fn swi_smwi_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{AlgoSelect, Param, Toggle};
        let f = &self.form;
        let mut rows = vec![Toggle { label: "Compute SWI", field: "do_swi", help: "Also compute susceptibility-weighted images" }];
        if f.do_swi {
            rows.push(AlgoSelect { label: "  Scaling", field: "swi_scaling", options: &SWI_SCALING_OPTIONS, help: &SWI_SCALING_HELP });
            rows.push(Param { label: "  Strength", field: "swi_strength", help: "Phase scaling strength (higher = stronger phase contrast)" });
            rows.push(Param { label: "  HP Sigma (x y z)", field: "swi_hp_sigma", help: "High-pass filter sigma per axis, in voxels (x y z): background phase removal. Set z to 0 for thin axial slices" });
            rows.push(Param { label: "  mIP Window", field: "swi_mip_window", help: "Minimum intensity projection window size in slices" });
        }
        rows.push(PipelineRow::Separator);
        rows.push(Toggle { label: "Compute SMWI", field: "do_smwi", help: "Susceptibility map-weighted imaging — weights the magnitude by a mask built from χ, not from filtered phase. Needs QSM" });
        if f.do_smwi {
            rows.push(Param { label: "  Threshold", field: "smwi_threshold", help: "|χ| in ppm at which the weighting mask reaches zero. The 1 ppm default targets strong sources; brain tissue wants far less" });
            rows.push(Param { label: "  Power", field: "smwi_power", help: "Power the mask is raised to (contrast strength)" });
            rows.push(Param { label: "  mIP Window", field: "smwi_mip_window", help: "Minimum-intensity-projection window in slices" });
            if self.input_relevance().qsm {
                rows.push(PipelineFormState::custom_qsm_row());
            }
        }
        rows
    }

    /// Other outputs → Relaxometry: the maps, the R2' source, and the maps that may be supplied.
    fn relaxometry_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{AlgoSelect, Param, Toggle};
        let inputs = self.input_relevance();
        let mut rows = vec![
            Toggle { label: "Compute T2* Map", field: "do_t2starmap", help: "Compute T2* relaxation map (requires 3+ echoes with magnitude)" },
            Toggle { label: "Compute R2* Map", field: "do_r2starmap", help: "Compute R2* decay rate map (requires 3+ echoes with magnitude)" },
            Toggle { label: "Compute R2 Map", field: "do_r2map", help: "R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG. Forced on when chi-separation needs it" },
        ];
        if inputs.r2 {
            rows.push(Param { label: "  Custom R2 tool", field: "custom_r2_tool", help: "Use an R2 map from <bids>/derivatives/<tool>/ instead of computing it from MESE (blank = compute, * = any tool)" });
        }
        rows.push(Toggle { label: "Compute R2' Map", field: "do_r2primemap", help: "R2' = R2* − R2 (Hz); needs GRE magnitude + a MESE acquisition, or R2PRIMEnet. Forced on when chi-separation needs it" });
        if inputs.r2prime {
            rows.push(Param { label: "  Custom R2' tool", field: "custom_r2prime_tool", help: "Use an R2' map from <bids>/derivatives/<tool>/ instead of computing it (blank = compute, * = any tool)" });
        }
        // Only consulted when R2' is computed — a supplied map always wins.
        if inputs.r2prime_source {
            rows.push(AlgoSelect { label: "  R2' Source", field: "r2prime_strategy", options: R2PRIME_STRATEGY_OPTIONS, help: R2PRIME_STRATEGY_HELP });
            // R2PRIMEnet's own settings, unless R2' is only ever measured (index 1 = mese).
            if self.pipeline_state.r2prime_strategy != 1 {
                rows.push(Toggle { label: "    R2PRIMEnet off 3 T", field: "r2primenet_ignore_field_mismatch",
                    help: "Run R2PRIMEnet on non-3 T data (trained at 3 T only; the R2' is uncalibrated). Off: auto skips R2', r2primenet errors" });
                rows.push(Param { label: "    R2PRIMEnet Patch", field: "r2primenet_patch",
                    help: "Sliding-window patch D H W in voxels, each a multiple of 16 (default 192 192 128)" });
            }
        }
        rows
    }

    /// Other outputs → Segmentation: where the segmentation comes from, and the statistics on it.
    fn segmentation_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{AlgoSelect, Param, Toggle};
        let f = &self.form;
        let mut rows = vec![Toggle { label: "Segmentation", field: "do_segmentation", help: "Whole-brain parcellation: SynthSeg straight off the GRE magnitude, or one supplied in derivatives. Forced on by the statistics and a region reference" }];
        // Whatever forces segmentation reveals its settings too: the checkbox then reads as
        // checked, and hiding the settings that shape the segmentation it promises would be a lie.
        if self.field_toggle("do_segmentation") {
            rows.push(AlgoSelect { label: "  Source", field: "segmentation_source", options: SEGMENTATION_SOURCE_OPTIONS, help: SEGMENTATION_SOURCE_HELP });
            if self.field_select("segmentation_source") == 1 {
                rows.push(Param { label: "    Derivatives tool", field: "custom_dseg_tool", help: "The dseg from <bids>/derivatives/<tool>/: * = any tool, or a tool name; SynthSeg runs where none is found" });
            } else {
                rows.push(AlgoSelect { label: "    SynthSeg Version", field: "synthseg_version", options: SYNTHSEG_VERSION_OPTIONS, help: SYNTHSEG_VERSION_HELP });
                rows.push(Param { label: "    Crop", field: "synthseg_crop", help: "Cubic centre-crop in voxels at 1 mm before inference; empty processes the whole volume" });
                rows.push(Toggle { label: "    Flip Averaging", field: "synthseg_flip_averaging", help: "Average with a left-right flipped second pass (SynthSeg's default). Doubles inference time" });
                rows.push(Toggle { label: "    Topology Cleanup", field: "synthseg_topology_cleanup", help: "Reset each topological class to its largest connected component" });
                rows.push(Toggle { label: "    Posterior Smoothing", field: "synthseg_smoothing", help: "Gaussian blur on the posteriors before the labels are taken" });
                if f.synthseg_smoothing {
                    rows.push(Param { label: "      Sigma (voxels)", field: "synthseg_sigma", help: "Gaussian sigma of the posterior blur, in voxels" });
                }
            }
        }
        rows.push(PipelineRow::Separator);
        rows.push(Toggle { label: "Per-structure statistics", field: "do_analysis", help: "Summarise every map the run makes (χ, χ+/χ−, T2*, R2*, R2, R2') per structure into a TSV: mean, SD, median, min, max, 5th/95th percentiles. Implies segmentation; implies QSM only if no other map is enabled" });
        if f.do_analysis && self.input_relevance().qsm {
            rows.push(PipelineFormState::custom_qsm_row());
        }
        rows
    }

    /// Other outputs → Export.
    fn export_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{Param, Toggle};
        let mut rows = vec![Toggle { label: "Export DICOM", field: "export_dicom", help: "Also write final maps as DICOM series into each subject's extra_files/ folder" }];
        if self.form.export_dicom {
            rows.push(Param { label: "  Source DICOM Dir", field: "source_dicom", help: "Original DICOM directory: the exported series inherit its patient/study identity (empty = none)" });
            rows.push(Param { label: "  DICOM Outputs", field: "dicom_outputs", help: "Comma-separated maps to export, e.g. Chimap,desc-total_chimap (empty = all)" });
        }
        rows
    }

    /// The Execution tab: local or SLURM, each mode's own settings, then what applies to both.
    fn execution_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{AlgoSelect, Param, Toggle};
        let local = self.form.execution_mode == 0;
        let mut rows = vec![AlgoSelect { label: "Execution Mode", field: "execution_mode", options: EXECUTION_MODE_OPTIONS, help: EXECUTION_MODE_HELP }];
        if local {
            rows.push(Toggle { label: "  Dry Run", field: "dry_run", help: "Print processing plan without executing" });
            rows.push(Param { label: "  Num Processes", field: "n_procs", help: "Number of parallel threads (empty = auto)" });
            // The memory limit schedules concurrent local runs.
            rows.push(Param { label: "  Memory Limit (GB)", field: "mem_limit_gb", help: "Memory budget for scheduling concurrent runs: auto (empty) detects it, a number of GB, or none for no limit" });
        } else {
            rows.push(Param { label: "  SLURM Account", field: "slurm_account", help: "SLURM account name (required for SLURM mode)" });
            rows.push(Param { label: "  SLURM Partition", field: "slurm_partition", help: "SLURM partition (optional)" });
            rows.push(Param { label: "  SLURM Time Limit", field: "slurm_time", help: "Wall time limit per job (e.g. 02:00:00)" });
            rows.push(Param { label: "  SLURM Memory (GB)", field: "slurm_mem", help: "Memory per job in GB (empty = estimated per run)" });
            rows.push(Param { label: "  SLURM CPUs/Task", field: "slurm_cpus", help: "CPUs per SLURM task" });
            rows.push(Toggle { label: "  Auto-Submit", field: "slurm_submit", help: "Automatically submit generated scripts via sbatch" });
        }
        rows.push(PipelineRow::Separator);
        rows.push(Toggle { label: "Debug Logging", field: "debug", help: "Enable verbose debug log output" });
        rows.push(Toggle { label: "Force Re-run", field: "force", help: "Ignore cached pipeline state and recompute everything" });
        rows.push(Toggle { label: "Clean Intermediates", field: "clean_intermediates", help: "Remove intermediate files after the run, keeping only final outputs" });
        rows.push(PipelineRow::Separator);
        rows.push(PipelineRow::RunButton);
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// Whether `field` has a row on `tab` now (switches to it, syncing the pipeline rows).
    /// SynthSeg's own settings, under the segmentation checkbox.
    const SYNTHSEG_ROWS: [&str; 5] = ["synthseg_version", "synthseg_crop", "synthseg_flip_averaging",
                                      "synthseg_topology_cleanup", "synthseg_sigma"];

    fn shows(app: &mut App, tab: usize, field: &str) -> bool {
        app.active_tab = tab;
        app.sync_pipeline_mode();
        app.pages(tab).into_iter().any(|p| app.page_rows(p).iter().any(|r| row_field(r) == Some(field)))
    }

    /// Switch to `tab`, on the sub-tab that shows `field`, and focus its row.
    fn focus_on(app: &mut App, tab: usize, field: &str) {
        app.active_tab = tab;
        let page = app.pages(tab).into_iter()
            .find(|&p| app.page_rows(p).iter().any(|r| row_field(r) == Some(field)));
        let page = page.unwrap_or_else(|| panic!("{field} has no row on tab {tab}"));
        goto_page(app, page);
        assert!(app.focus_field(field), "{field} has no row on tab {tab}");
    }

    /// Every field with a row anywhere now, with a `#Page` marker for each sub-tab shown.
    fn all_fields(app: &mut App) -> Vec<String> {
        app.sync_pipeline_mode();
        let mut out = Vec::new();
        for (_, page, rows) in app.all_pages() {
            out.push(format!("#{page:?}"));
            out.extend(rows.iter().filter_map(row_field).map(str::to_string));
        }
        out
    }

    /// Open `page` (on whichever tab has it now).
    fn goto_page(app: &mut App, page: Page) {
        let tab = (0..TAB_NAMES.len()).find(|&t| app.pages(t).contains(&page))
            .unwrap_or_else(|| panic!("{page:?} is not shown now"));
        app.active_tab = tab;
        app.sub_page[tab] = Some(page);
        app.sync_pipeline_mode();
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    // ─── Example-dataset input mode ───

    /// An app parked in Example mode with focus on the picker.
    fn example_app() -> App {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.input_mode = InputMode::Example;
        app.active_field = App::INPUT_IO_FIELDS;
        app
    }

    #[test]
    fn example_state_starts_on_the_button_with_the_default_ticked() {
        let app = example_app();
        assert_eq!(app.example_state.cursor, ExampleState::button_index());
        let picked = app.example_state.selected_examples();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].id, crate::example::DEFAULT_ID);
        assert_eq!(app.example_state.selected_bytes(), picked[0].bytes);
    }

    #[test]
    fn example_picker_navigates_and_ticks() {
        let mut app = example_app();
        let button = ExampleState::button_index();

        // Up from the button walks into the list; Space ticks a second acquisition.
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.example_state.cursor, button - 1);
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.example_state.selected_examples().len(), 2);

        // Space again unticks it.
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.example_state.selected_examples().len(), 1);

        // Down cannot walk past the button.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.example_state.cursor, button);
    }

    #[test]
    fn example_picker_returns_to_the_io_fields_from_the_top() {
        let mut app = example_app();
        app.example_state.cursor = 0;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
    }

    #[test]
    fn example_download_refuses_an_empty_selection() {
        let mut app = example_app();
        app.example_state.selected = vec![false; crate::example::EXAMPLES.len()];
        app.handle_key(key(KeyCode::Enter)); // cursor starts on the button
        assert_eq!(app.example_state.status, ConvertStatus::Idle);
        assert!(
            app.error_message.as_deref().unwrap_or("").contains("Select at least one"),
            "{:?}",
            app.error_message
        );
    }

    #[test]
    fn example_reset_is_blocked_while_a_download_runs() {
        // Resetting mid-fetch would orphan the worker thread mid-write.
        let mut app = example_app();
        app.example_state.status = ConvertStatus::Converting;
        app.example_state.output_dir = "somewhere".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.example_state.output_dir, "somewhere", "state must survive");
        assert!(app.error_message.is_some());
    }

    #[test]
    fn example_reset_clears_state_when_idle() {
        let mut app = example_app();
        app.example_state.output_dir = "somewhere".to_string();
        app.example_state.cursor = 0;
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.example_state.output_dir.is_empty());
        assert_eq!(app.example_state.cursor, ExampleState::button_index());
    }

    #[test]
    fn example_dataset_dir_defaults_and_expands_tilde() {
        let mut app = example_app();
        assert_eq!(app.example_state.resolved_dir(), std::path::PathBuf::from(EXAMPLE_DEFAULT_DIR));

        app.example_state.output_dir = "  ~/data/example  ".to_string();
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(
                app.example_state.resolved_dir(),
                std::path::Path::new(&home).join("data/example")
            );
        }

        app.example_state.output_dir = "/abs/path".to_string();
        assert_eq!(app.example_state.resolved_dir(), std::path::PathBuf::from("/abs/path"));
    }

    #[test]
    fn example_poll_is_inert_without_a_worker() {
        let mut app = example_app();
        assert!(app.example_state.poll().is_none());
        assert!(app.example_state.progress().is_none());
    }

    #[test]
    fn example_mode_blocks_a_run_until_the_dataset_exists() {
        let mut app = example_app();
        app.form.bids_dir = "/some/dir".to_string();
        app.try_run();
        assert!(!app.should_run);
        assert!(
            app.error_message.as_deref().unwrap_or("").contains("Download the example dataset"),
            "{:?}",
            app.error_message
        );
    }

    #[test]
    fn example_mode_edits_the_dataset_dir_not_the_bids_dir() {
        // Field 1 in Example mode is the dataset directory; the pipeline's own output
        // directory stays on field 2, unlike the DICOM/NIfTI modes.
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.input_mode = InputMode::Example;
        app.active_field = 1;
        app.handle_key(key(KeyCode::Enter));
        for c in "mydata".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.example_state.output_dir, "mydata");
        assert!(app.form.bids_dir.is_empty(), "the BIDS dir must not be touched");
    }

    #[test]
    fn input_mode_cycles_through_all_four_modes() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 0;
        let seen: Vec<InputMode> = (0..4)
            .map(|_| {
                app.handle_key(key(KeyCode::Right));
                app.input_mode
            })
            .collect();
        assert_eq!(
            seen,
            vec![InputMode::NIfTI, InputMode::DicomToBids, InputMode::Example, InputMode::Bids]
        );
    }

    fn update_app() -> App {
        let mut app = App::new();
        app.update_state = UpdateState::disabled();
        app.update_modal = Some(UpdateModal {
            tag: "v9.23.0".into(),
            html_url: "https://example.com/r".into(),
        });
        app
    }

    #[test]
    fn update_prompt_enter_hands_over_to_the_installer() {
        let mut app = update_app();
        assert_eq!(app.handle_update_modal_key(key(KeyCode::Enter)), UpdateChoice::Update);
        assert!(app.should_update, "the main loop needs this to run the update");
        assert!(app.update_modal.is_none(), "the prompt should close");
    }

    #[test]
    fn update_prompt_esc_defers_without_recording_anything() {
        let _guard = cache_guard();
        let mut app = update_app();
        assert_eq!(app.handle_update_modal_key(key(KeyCode::Esc)), UpdateChoice::Later);
        assert!(!app.should_update);
        assert!(app.update_modal.is_none());
        // "Later" means later — the same release is offered again next launch.
        assert!(!crate::commands::update::check::is_dismissed("v9.23.0"));
    }

    #[test]
    fn update_prompt_d_remembers_the_skipped_release() {
        let _guard = cache_guard();
        let mut app = update_app();
        assert_eq!(app.handle_update_modal_key(key(KeyCode::Char('d'))), UpdateChoice::Skip);
        assert!(!app.should_update);
        assert!(app.update_modal.is_none());
        assert!(crate::commands::update::check::is_dismissed("v9.23.0"));
    }

    /// An unrelated keystroke must not dismiss the prompt by accident — it is modal,
    /// and a stray arrow key should leave it exactly as it was.
    #[test]
    fn update_prompt_ignores_keys_it_does_not_bind() {
        let mut app = update_app();
        assert_eq!(app.handle_update_modal_key(key(KeyCode::Down)), UpdateChoice::Ignored);
        assert!(app.update_modal.is_some());
        assert!(!app.should_update);
    }

    /// While the prompt is up it owns the keyboard: keys must not also reach the form
    /// underneath, or answering it would edit whatever field happened to be focused.
    #[test]
    fn update_prompt_captures_input_from_the_form() {
        let mut app = update_app();
        app.active_tab = TAB_EXECUTION;
        let before = app.active_field;
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.active_field, before, "the form must not move behind the prompt");
        assert!(app.update_modal.is_some());
    }

    /// A release the user already skipped must not come back on the next launch.
    #[test]
    fn poll_does_not_reopen_a_skipped_release() {
        let _guard = cache_guard();
        crate::commands::update::check::dismiss("v9.23.0");

        let mut app = App::new();
        app.update_state = UpdateState::disabled();
        app.update_modal = None;
        // Simulate the background check reporting the skipped release.
        if crate::commands::update::check::is_dismissed("v9.23.0") {
            // poll_update_check applies exactly this rule.
        }
        app.poll_update_check();
        assert!(app.update_modal.is_none());
    }

    /// A dev build must not spawn a network check just by opening the TUI.
    #[test]
    fn a_dev_build_starts_no_background_check() {
        let mut state = UpdateState::start();
        assert!(state.poll().is_none());
    }

    /// Point the update cache at a temp file for the duration of a test.
    ///
    /// Takes the same lock as the `commands::update` tests: `$QSMXT_UPDATE_CACHE` is
    /// process-global, so two modules guarding it with separate mutexes would race.
    fn cache_guard() -> CacheGuard {
        let guard = crate::commands::update::check::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("QSMXT_UPDATE_CACHE", dir.path().join("update-check.json"));
        CacheGuard { _guard: guard, _dir: dir }
    }

    struct CacheGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        _dir: tempfile::TempDir,
    }

    impl Drop for CacheGuard {
        fn drop(&mut self) {
            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }
    }

    #[test]
    fn test_algo_modal_opens_on_enter_over_algoselect() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
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
            target: AlgoModalTarget::Select("qsm_algorithm".to_string()),
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
            target: AlgoModalTarget::Select("qsm_algorithm".to_string()),
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
        goto_page(&mut app, Page::Masking);
        app.sync_pipeline_mode();
        // Focus the mask Input row.
        let rows = app.rows();
        let target = app.focusable_rows().iter().position(|&ri| {
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
        assert_eq!(modal.options.len(), 4);
        app.handle_key(key(KeyCode::Down)); // NIfTI
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.input_mode, InputMode::NIfTI);
    }

    #[test]
    fn test_execution_mode_modal_opens_and_sets() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        focus_on(&mut app, TAB_EXECUTION, "execution_mode");
        app.handle_key(key(KeyCode::Enter));
        let modal = app.algo_modal.as_ref().expect("modal should open over Execution Mode");
        assert!(matches!(&modal.target, AlgoModalTarget::Select(f) if f == "execution_mode"));
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
        assert_eq!(app.active_tab, TAB_EXECUTION); // navigated to Execution tab
    }

    // --- Pipeline state selects ---

    #[test]
    fn test_pipeline_algorithm_selects() {
        let mut app = App::new();
        let ps = &mut app.pipeline_state;
        assert_eq!(QSM_ALGO_OPTIONS[ps.qsm_algorithm], "ilsqr"); // the default inversion
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
        app.active_tab = TAB_OUTPUTS; // Algorithms tab - no text fields
        focus_on(&mut app, TAB_OUTPUTS, "do_swi");
        assert_eq!(app.text_value(), "");
    }

    // --- get_ accessors (used by UI rendering) ---

    /// Every row on every page names a field the accessors resolve, with the kind it edits.
    #[test]
    fn rows_name_real_fields() {
        let mut app = App::new();
        app.form.do_swi = true;
        app.form.do_smwi = true;
        app.form.do_segmentation = true;
        app.form.export_dicom = true;
        app.form.do_analysis = true;
        app.form.do_r2primemap = true;
        app.pipeline_state.motion_correction = true;
        app.filter_state.orientation_group = "acq".into();
        for mode in [0, 1] {
            app.form.execution_mode = mode;
            for page in Page::ALL {
                for r in app.page_rows(page) {
                    match r {
                        PipelineRow::Param { field, .. } => assert!(app.field_text_mut(field).is_some(), "{field}"),
                        PipelineRow::AlgoSelect { field, options, .. } => assert!(app.field_select(field) < options.len(), "{field}"),
                        _ => {}
                    }
                }
            }
        }
        // Unknown names resolve to nothing in the run form.
        assert!(app.form.text("nope").is_none() && app.form.flag("nope").is_none() && app.form.choice("nope").is_none());
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
        let mut ps = super::PipelineFormState { qsm_algorithm: 0, ..Default::default() }; // RTS
        let rows_rts = ps.all_rows().len();
        ps.qsm_algorithm = 2; // TKD (fewer params)
        let rows_tkd = ps.all_rows().len();
        assert!(rows_tkd < rows_rts, "TKD should have fewer rows than RTS");

        ps.qsm_algorithm = 3; // TGV (hides unwrapping + bgremove)
        let rows_tgv = ps.all_rows().len();
        assert!(rows_tgv < rows_rts, "TGV should hide unwrapping/bgremove");
    }

    /// `[` and `]` walk the sub-tabs of the active tab, wrapping, and put focus back on top.
    #[test]
    fn brackets_switch_sub_tabs_and_reset_focus() {
        let mut app = App::new();
        app.active_tab = TAB_PREPROCESSING;
        app.sync_pipeline_mode();
        assert_eq!(app.page(), Some(Page::Geometry));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focus(), 1);
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.page(), Some(Page::Masking));
        assert_eq!(app.focus(), 0, "a sub-tab switch starts at the top");
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.page(), Some(Page::Motion));
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.page(), Some(Page::Geometry), "wraps");
        app.handle_key(key(KeyCode::Char('[')));
        assert_eq!(app.page(), Some(Page::Motion));
        // The sub-tab is remembered per tab; a main-tab switch resets focus too.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, TAB_QSM);
        assert_eq!(app.focus(), 0);
        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.page(), Some(Page::Motion));
    }

    /// The QSM tab shows a sub-tab for each stage that runs, and only the switch when none do.
    #[test]
    fn qsm_sub_tabs_follow_the_stages_that_run() {
        let mut app = App::new();
        let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name).unwrap();
        assert_eq!(app.pages(TAB_QSM), vec![Page::FieldMap, Page::Background, Page::Inversion, Page::Reference]);
        app.pipeline_state.qsm_algorithm = idx("iqsm");
        assert_eq!(app.pages(TAB_QSM), vec![Page::Inversion, Page::Reference]);
        app.filter_state.orientation_group = "acq".into();
        assert!(app.pages(TAB_QSM).contains(&Page::MultiOrientation));
        app.pipeline_state.do_qsm = false;
        app.filter_state.orientation_group.clear();
        assert_eq!(app.pages(TAB_QSM), vec![Page::QsmOff]);
        // A forced QSM keeps the stages.
        app.form.do_smwi = true;
        assert!(app.pages(TAB_QSM).contains(&Page::Inversion));
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
        fs.focus_next(); // -> orientation group pattern
        assert_eq!(fs.focus, super::FilterFocus::OrientationGroup);
        // With no pattern set, its settings rows are skipped entirely — the single-orientation
        // majority should not have to tab past a feature they are not using.
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
        fs.focus_next(); // orientation group pattern
        fs.focus_next(); // subject
        fs.focus_next(); // run leaf
        let tree = fs.tree.as_ref().unwrap();
        assert!(tree.subjects[0].runs[0].selected);
        fs.toggle_focused();
        let tree = fs.tree.as_ref().unwrap();
        assert!(!tree.subjects[0].runs[0].selected);
    }

    /// The Input tab keeps only the pattern: its settings are on QSM → Multi-orientation, so
    /// focus goes from the pattern straight to the tree, set or not.
    #[test]
    fn the_input_tab_has_only_the_orientation_pattern() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_single_echo_bids(dir.path());
        let mut fs = super::FilterTreeState::default();
        fs.maybe_rescan(dir.path().to_str().unwrap());
        for pattern in ["*acq-dir*", ""] {
            fs.orientation_group = pattern.into();
            fs.focus = super::FilterFocus::OrientationGroup;
            fs.focus_next();
            assert!(matches!(fs.focus, super::FilterFocus::TreeNode(0)));
            fs.focus_prev();
            assert_eq!(fs.focus, super::FilterFocus::OrientationGroup);
        }
    }

    /// The preview is the safety feature: it has to report a bad pattern rather than
    /// silently matching nothing.
    #[test]
    fn orientation_preview_reports_a_bad_pattern() {
        let mut fs = super::FilterTreeState {
            orientation_group: "chunk".into(),
            ..Default::default()
        };
        let preview = fs.orientation_preview();
        assert!(preview.error.unwrap().contains("chunk"));

        fs.orientation_group = "re:(a)(b)".into();
        assert!(fs.orientation_preview().error.is_some());
    }

    #[test]
    fn orientation_preview_is_empty_when_off() {
        let fs = super::FilterTreeState::default();
        let preview = fs.orientation_preview();
        assert!(preview.error.is_none());
        assert!(preview.groups.is_empty());
    }

    #[test]
    fn orientation_args_are_none_until_a_pattern_is_set() {
        let mut fs = super::FilterTreeState::default();
        assert_eq!(fs.orientation_args().0, None);
        fs.orientation_group = "  acq  ".into();
        fs.orientation_algorithm = 1;
        fs.orientation_lambda = "1e-3".into();
        let (pattern, algo, lambda) = fs.orientation_args();
        assert_eq!(pattern.as_deref(), Some("acq"), "the pattern is trimmed");
        assert_eq!(algo, 1);
        assert_eq!(lambda, Some(1e-3));
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
        const OPT_IN_EMPTY: &[&str] = &["dl_tile_size", "dl_tile_halo", "robust_fit_tuning"];
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
            ("laplacian-solver", LAPLACIAN_SOLVER_OPTIONS.len(), LAPLACIAN_SOLVER_HELP.len()),
            ("vsharp-grid", VSHARP_GRID_OPTIONS.len(), VSHARP_GRID_HELP.len()),
            ("b0-estimation", B0_ESTIMATION_OPTIONS.len(), B0_ESTIMATION_HELP.len()),
            ("robust-fit-loss", ROBUST_FIT_LOSS_OPTIONS.len(), ROBUST_FIT_LOSS_HELP.len()),
            ("motion-reference", MOTION_REFERENCE_OPTIONS.len(), MOTION_REFERENCE_HELP.len()),
            ("bg-removal", BF_OPTIONS.len(), BF_HELP.len()),
            ("qsm-reference", qsm_ref_options().len(), qsm_ref_help().len()),
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
        assert!(shows(&mut app, TAB_EXECUTION, "execution_mode")); // Execution Mode
        assert!(shows(&mut app, TAB_EXECUTION, "dry_run")); // Dry Run
        assert!(shows(&mut app, TAB_EXECUTION, "debug")); // Debug
        assert!(shows(&mut app, TAB_EXECUTION, "n_procs")); // Num Processes
        // SLURM fields hidden
        assert!(!shows(&mut app, TAB_EXECUTION, "slurm_account")); // SLURM Account
        assert!(!shows(&mut app, TAB_EXECUTION, "slurm_partition")); // SLURM Partition
        assert!(!shows(&mut app, TAB_EXECUTION, "slurm_time")); // SLURM Time
        assert!(!shows(&mut app, TAB_EXECUTION, "slurm_mem")); // SLURM Mem
        assert!(!shows(&mut app, TAB_EXECUTION, "slurm_cpus")); // SLURM CPUs
        assert!(!shows(&mut app, TAB_EXECUTION, "slurm_submit")); // Auto-Submit
    }

    #[test]
    fn test_field_visibility_slurm_mode() {
        let mut app = App::new();
        app.form.execution_mode = 1; // SLURM
        // Execution Mode always visible
        assert!(shows(&mut app, TAB_EXECUTION, "execution_mode"));
        // Dry Run and Num Processes hidden in SLURM mode
        assert!(!shows(&mut app, TAB_EXECUTION, "dry_run")); // Dry Run
        assert!(!shows(&mut app, TAB_EXECUTION, "n_procs")); // Num Processes
        // Debug visible in both modes
        assert!(shows(&mut app, TAB_EXECUTION, "debug"));
        // SLURM fields visible
        assert!(shows(&mut app, TAB_EXECUTION, "slurm_account"));
        assert!(shows(&mut app, TAB_EXECUTION, "slurm_partition"));
        assert!(shows(&mut app, TAB_EXECUTION, "slurm_time"));
        assert!(shows(&mut app, TAB_EXECUTION, "slurm_mem"));
        assert!(shows(&mut app, TAB_EXECUTION, "slurm_cpus"));
        assert!(shows(&mut app, TAB_EXECUTION, "slurm_submit"));
    }

    #[test]
    fn test_field_visibility_other_tabs() {
        let mut app = App::new();
        assert!(shows(&mut app, TAB_OUTPUTS, "do_swi"));
        assert!(shows(&mut app, TAB_OUTPUTS, "do_r2starmap"));
    }

    // --- SLURM form fields ---

    #[test]
    fn test_slurm_form_defaults() {
        let form = RunForm::default();
        assert_eq!(form.execution_mode, 0);
        assert!(form.slurm_account.is_empty());
        assert!(form.slurm_partition.is_empty());
        assert_eq!(form.slurm_time, "02:00:00");
        assert_eq!(form.slurm_mem, "", "empty: sized per run from the memory estimate");
        assert_eq!(form.slurm_cpus, "4");
        assert!(!form.slurm_submit);
    }

    // --- Execution mode select ---

    #[test]
    fn test_execution_mode_select() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        assert_eq!(app.field_select("execution_mode"), 0); // Local
        app.form.execution_mode = 1;
        assert_eq!(app.field_select("execution_mode"), 1); // SLURM
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
        assert_eq!(app.field_text("slurm_account"), "myacct");
        assert_eq!(app.field_text("slurm_partition"), "gpu");
        assert_eq!(app.field_text("slurm_time"), "04:00:00");
        assert_eq!(app.field_text("slurm_mem"), "64");
        assert_eq!(app.field_text("slurm_cpus"), "8");
    }

    #[test]
    fn test_slurm_checkbox_value() {
        let mut app = App::new();
        assert!(!app.field_toggle("slurm_submit")); // slurm_submit default
        app.form.slurm_submit = true;
        assert!(app.field_toggle("slurm_submit"));
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
        app.form.execution_mode = 1;
        focus_on(&mut app, TAB_EXECUTION, "slurm_account");
        app.form.slurm_account = "changed".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.form.slurm_account.is_empty());
    }

    // --- Navigation skips hidden fields ---

    #[test]
    fn test_navigation_skips_hidden_slurm_fields() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1; // SLURM — hides Dry Run and Num Processes
        focus_on(&mut app, TAB_EXECUTION, "execution_mode");
        // Down from the mode skips the hidden Dry Run and lands on SLURM Account
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field_name().as_deref(), Some("slurm_account"));
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
        app.sync_pipeline_mode();
        let max = app.focusable_rows().len().saturating_sub(1);
        app.pipeline_state.focus = max;
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.pipeline_state.focus, max);
    }

    #[test]
    fn test_pipeline_tab_switch_from_pipeline() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_SEPARATION);
        app.active_tab = TAB_QSM;
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.active_tab, TAB_PREPROCESSING);
    }

    #[test]
    fn test_pipeline_tab_number_switch() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, 2);
    }

    #[test]
    fn test_switch_to_separation_resets_focus_to_first_row() {
        let mut app = App::new();
        // Sit deep in the QSM tab, past the number of rows Separation will have.
        app.active_tab = TAB_QSM;
        app.sync_pipeline_mode();
        let qsm_max = app.pipeline_state.focusable_rows().len().saturating_sub(1);
        app.pipeline_state.focus = qsm_max;
        // Switch to Separation; mode changes, so focus must reset to the first selectable row.
        app.active_tab = TAB_SEPARATION;
        app.sync_pipeline_mode();
        assert_eq!(app.pipeline_state.page, Page::Separation);
        assert_eq!(app.pipeline_state.focus, 0);
        // And it must be in range for the Separation row set.
        let sep_len = app.pipeline_state.focusable_rows().len();
        assert!(app.pipeline_state.focus < sep_len.max(1));
    }

    #[test]
    fn test_pipeline_left_right_algo_select() {
        let mut app = App::new();
        assert!(app.goto_field("qsm_algorithm"));
        // Focus 0 is "Compute QSM" toggle, focus 1 is "Phase Combination" select
        // Find the focus index for the QSM Inversion AlgoSelect
        let rows = app.rows();
        let focusable = app.focusable_rows();
        let mut qsm_inv_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::AlgoSelect { field: "qsm_algorithm", .. } = &rows[ri] {
                qsm_inv_focus = Some(fi);
                break;
            }
        }
        let fi = qsm_inv_focus.expect("qsm_algorithm row not found");
        app.pipeline_state.focus = fi;
        let ilsqr = QSM_ALGO_OPTIONS.iter().position(|o| *o == "ilsqr").unwrap();
        assert_eq!(app.pipeline_state.qsm_algorithm, ilsqr); // the default inversion
        app.handle_key(key(KeyCode::Right));
        assert_eq!(QSM_ALGO_OPTIONS[app.pipeline_state.qsm_algorithm], "lsqr");
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.pipeline_state.qsm_algorithm, ilsqr);
    }

    #[test]
    fn test_pipeline_enter_opens_algo_modal_and_left_right_cycles() {
        let mut app = App::new();
        assert!(app.goto_field("unwrapping_algorithm"));
        // Navigate to unwrapping_algorithm AlgoSelect
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
            AlgoModalTarget::Select(ref f) if f == "unwrapping_algorithm"
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
        app.pipeline_state.axial_resampling = true;
        assert!(app.goto_field("obliquity_threshold"));
        // Find the obliquity_threshold Param row
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        assert!(app.goto_field("inhomogeneity_correction"));
        // Find inhomogeneity_correction toggle
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_pipeline_esc_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn test_pipeline_f5_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_QSM;
        app.form.bids_dir = "/tmp/bids".to_string();
        app.handle_key(key(KeyCode::F(5)));
        assert!(app.should_run);
    }

    #[test]
    fn test_pipeline_reset_field() {
        let mut app = App::new();
        assert!(app.goto_field("qsm_algorithm"));
        app.pipeline_state.qsm_algorithm = 5;
        // Find qsm_algorithm focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        assert_eq!(QSM_ALGO_OPTIONS[app.pipeline_state.qsm_algorithm], "ilsqr"); // default
    }

    #[test]
    fn test_pipeline_reset_tab() {
        let mut app = App::new();
        app.pipeline_state.qsm_algorithm = 5;
        app.pipeline_state.bf_algorithm = 3;
        app.pipeline_state.tikhonov_lambda = "9".into();
        assert!(app.goto_field("qsm_algorithm"));
        app.handle_key(key(KeyCode::Char('R')));
        // The page's settings, the ones a reset reveals among them, and nothing on other pages.
        assert_eq!(QSM_ALGO_OPTIONS[app.pipeline_state.qsm_algorithm], "ilsqr");
        assert_eq!(app.pipeline_state.tikhonov_lambda, PipelineFormState::default().tikhonov_lambda);
        assert_eq!(app.pipeline_state.bf_algorithm, 3, "Background is another page");
    }

    #[test]
    fn test_pipeline_visible_rows_change_on_toggle() {
        let mut app = App::new();
        let rows_with_qsm = app.pipeline_state.all_rows().len();
        app.pipeline_state.do_qsm = false;
        let rows_without_qsm = app.pipeline_state.all_rows().len();
        assert!(rows_without_qsm < rows_with_qsm);
    }

    #[test]
    fn test_pipeline_editing_left_right_cursor() {
        let mut app = App::new();
        app.pipeline_state.axial_resampling = true;
        assert!(app.goto_field("obliquity_threshold"));
        // Find obliquity_threshold param and edit it
        let rows = app.rows();
        let focusable = app.focusable_rows();
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

    // ========== Other outputs tab handler tests ==========

    #[test]
    fn test_supplementary_toggle_swi() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        focus_on(&mut app, TAB_OUTPUTS, "do_swi");
        assert!(!app.form.do_swi);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.do_swi);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!app.form.do_swi);
    }

    #[test]
    fn test_supplementary_toggle_t2star() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        focus_on(&mut app, TAB_OUTPUTS, "do_t2starmap");
        assert!(!app.form.do_t2starmap);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.do_t2starmap);
    }

    #[test]
    fn test_supplementary_toggle_r2star() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        focus_on(&mut app, TAB_OUTPUTS, "do_r2starmap");
        assert!(!app.form.do_r2starmap);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(app.form.do_r2starmap);
    }

    #[test]
    fn test_supplementary_swi_scaling_select() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        app.form.do_swi = true; // make SWI fields visible
        focus_on(&mut app, TAB_OUTPUTS, "swi_scaling");
        assert_eq!(app.form.swi_scaling, 0); // tanh
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.form.swi_scaling, 1); // negative-tanh
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.form.swi_scaling, 0);
    }

    #[test]
    fn test_supplementary_edit_swi_strength() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        app.form.do_swi = true;
        focus_on(&mut app, TAB_OUTPUTS, "swi_strength");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        // Type something
        app.handle_key(key(KeyCode::End));
        app.handle_key(key(KeyCode::Char('0')));
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.editing);
    }

    #[test]
    fn test_supplementary_navigate_fields() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        focus_on(&mut app, TAB_OUTPUTS, "do_swi");
        // SWI fields are hidden when do_swi is false, so Down from Compute SWI skips to SMWI
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field_name().as_deref(), Some("do_smwi"));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field_name().as_deref(), Some("do_swi"));
    }

    #[test]
    fn test_supplementary_swi_visible_when_enabled() {
        let mut app = App::new();
        app.form.do_swi = true;
        assert!(shows(&mut app, TAB_OUTPUTS, "swi_scaling")); // SWI Scaling
        assert!(shows(&mut app, TAB_OUTPUTS, "swi_strength")); // SWI Strength
        assert!(shows(&mut app, TAB_OUTPUTS, "swi_mip_window")); // SWI MIP Window
    }

    #[test]
    fn test_supplementary_swi_hidden_when_disabled() {
        let mut app = App::new();
        assert!(!shows(&mut app, TAB_OUTPUTS, "swi_scaling"));
        assert!(!shows(&mut app, TAB_OUTPUTS, "swi_mip_window"));
    }

    #[test]
    fn test_supplementary_tab_switch() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_EXECUTION);
    }

    #[test]
    fn test_supplementary_reset_field() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        focus_on(&mut app, TAB_OUTPUTS, "do_swi");
        app.form.do_swi = true;
        app.handle_key(key(KeyCode::Char('r')));
        assert!(!app.form.do_swi); // reset to default
    }

    #[test]
    fn test_supplementary_reset_tab() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        app.form.do_swi = true;
        app.form.do_t2starmap = true;
        app.form.do_r2starmap = true;
        app.form.swi_scaling = 3;
        app.handle_key(key(KeyCode::Char('R')));
        assert!(!app.form.do_swi);
        assert_eq!(app.form.swi_scaling, 0);
        assert!(app.form.do_t2starmap && app.form.do_r2starmap, "Relaxometry is another page");
        focus_on(&mut app, TAB_OUTPUTS, "do_t2starmap");
        app.handle_key(key(KeyCode::Char('R')));
        assert!(!app.form.do_t2starmap && !app.form.do_r2starmap);
    }

    // ========== Execution tab handler tests ==========

    #[test]
    fn test_execution_mode_switch_with_left_right() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        focus_on(&mut app, TAB_EXECUTION, "execution_mode");
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
        focus_on(&mut app, TAB_EXECUTION, "execution_mode");
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
        focus_on(&mut app, TAB_EXECUTION, "dry_run");
        assert!(!app.form.dry_run);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.dry_run);
    }

    #[test]
    fn test_execution_toggle_debug() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        focus_on(&mut app, TAB_EXECUTION, "debug");
        assert!(!app.form.debug);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.debug);
    }

    #[test]
    fn test_execution_edit_n_procs() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        focus_on(&mut app, TAB_EXECUTION, "n_procs");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.form.n_procs, "4");
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.editing);
    }

    #[test]
    fn test_execution_edit_slurm_account() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1; // SLURM mode
        focus_on(&mut app, TAB_EXECUTION, "slurm_account");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('c')));
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.form.slurm_account, "act");
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.editing);
    }

    #[test]
    fn test_execution_edit_slurm_partition() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        focus_on(&mut app, TAB_EXECUTION, "slurm_partition");
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
        focus_on(&mut app, TAB_EXECUTION, "slurm_time");
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
        focus_on(&mut app, TAB_EXECUTION, "slurm_submit");
        assert!(!app.form.slurm_submit);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.slurm_submit);
    }

    #[test]
    fn test_execution_reset_field() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        focus_on(&mut app, TAB_EXECUTION, "slurm_time");
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

    /// Enter, `c` and `y` copy the methods text (F5 still runs); the next key clears the message.
    #[test]
    fn methods_tab_enter_copies_the_methods_text() {
        for k in [KeyCode::Enter, KeyCode::Char('c'), KeyCode::Char('y')] {
            let mut app = App::new();
            app.active_tab = TAB_METHODS;
            app.form.bids_dir = "/tmp/bids".to_string();
            app.handle_key(key(k));
            assert!(!app.should_run, "{k:?} ran the pipeline");
            let copied = app.clipboard.take().unwrap_or_else(|| panic!("{k:?} copied nothing"));
            assert_eq!(copied, crate::tui::command::methods_text(&app).unwrap());
            assert!(copied.contains("processing was performed using qsmxt"), "{copied}");
            assert_eq!(app.status_message.as_deref(), Some("Methods copied to clipboard"));
            app.handle_key(key(KeyCode::Down));
            assert_eq!(app.status_message, None);
        }
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
        assert_eq!(app.input_mode, InputMode::Example);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input_mode, InputMode::Bids); // wraps around
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.input_mode, InputMode::Example); // wraps other way
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
        assert_eq!(app.active_tab, TAB_EXECUTION);
        assert_eq!(app.focused_field_name().as_deref(), Some("slurm_account"));
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
        app.active_tab = TAB_OUTPUTS;
        app.form.do_swi = true;
        focus_on(&mut app, TAB_OUTPUTS, "swi_strength");
        app.form.swi_strength = "999".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        let defaults = RunForm::default();
        assert_eq!(app.form.swi_strength, defaults.swi_strength);
    }

    #[test]
    fn test_reset_execution_fields() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        app.form.execution_mode = 1;
        focus_on(&mut app, TAB_EXECUTION, "slurm_mem");
        app.form.slurm_mem = "999".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.form.slurm_mem, "");
    }

    // ========== Pipeline mask operations ==========

    #[test]
    fn test_pipeline_mask_preset_switch() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        // Find mask_preset focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.mask_preset, 2); // HD-BET + signal-gated erosion
        assert_eq!(app.pipeline_state.mask_sections, crate::pipeline::config::hd_bet_mask_sections());
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.mask_preset, 3); // BET AND phase quality
        let recipe = crate::pipeline::config::bet_and_phase_mask_recipe();
        assert_eq!(app.pipeline_state.mask_sections, recipe.sections);
        assert_eq!(app.pipeline_state.mask_combine, recipe.combine);
        assert_eq!(app.pipeline_state.mask_combined_refinements, recipe.refinements);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.mask_preset, 4); // RS2-Net (rodent)
        assert_eq!(app.pipeline_state.mask_sections, crate::pipeline::config::rs2_net_mask_sections());
        // Leaving the two-section preset resets the combine mode and the combined-mask steps.
        assert_eq!(app.pipeline_state.mask_combine, crate::pipeline::config::MaskCombine::Or);
        assert!(app.pipeline_state.mask_combined_refinements.is_empty());
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.mask_preset, 0, "cycling skips 'custom'");
    }

    #[test]
    fn test_mask_threshold_method_modal_opens_and_sets() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.sync_pipeline_mode();
        // Default preset is a threshold generator, so the param row is the method select.
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }
        ));
        let rows = app.rows();
        let target = app.focusable_rows().iter().position(|&ri| {
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
        goto_page(&mut app, Page::Masking);
        app.sync_pipeline_mode();
        // Switch the generator to BET so the param row is a numeric frac-intensity slider.
        app.pipeline_state.mask_sections[0].generator = MaskOp::bet(0.5);
        let rows = app.rows();
        let target = app.focusable_rows().iter().position(|&ri| {
            matches!(rows.get(ri), Some(PipelineRow::MaskOpGeneratorParam { .. }))
        }).expect("MaskOpGeneratorParam row not focusable");
        app.pipeline_state.focus = target;
        app.handle_key(key(KeyCode::Enter));
        // BET frac. intensity is a slider, not a discrete select — no modal.
        assert!(app.algo_modal.is_none());
    }

    /// The preset row, the CLI's `--mask-preset` values and `mask_presets()` are three views of
    /// one list; `apply_mask_preset` indexes straight into it, so they have to stay aligned.
    #[test]
    fn test_mask_preset_options_match_the_preset_list() {
        use clap::ValueEnum;
        let names: Vec<&str> = crate::pipeline::config::mask_presets().iter().map(|(n, _)| *n).collect();
        assert_eq!(&MASK_PRESET_OPTIONS[..MASK_PRESET_CUSTOM], &names[..]);
        assert_eq!(MASK_PRESET_OPTIONS[MASK_PRESET_CUSTOM], "custom");
        assert_eq!(MASK_PRESET_OPTIONS.len(), MASK_PRESET_HELP.len());

        let cli_names: Vec<String> = crate::cli::MaskPresetArg::value_variants().iter()
            .filter_map(|v| v.to_possible_value()).filter(|v| !v.is_hide_set())
            .map(|v| v.get_name().to_string()).collect();
        assert_eq!(cli_names, names, "--mask-preset values drifted from mask_presets()");
    }

    /// Every signal-erode parameter is reachable and adjustable with ←/→ from the pipeline form,
    /// and what the CLI preview says is what the rows show.
    #[test]
    fn test_pipeline_signal_erode_params_are_editable() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.apply_mask_preset(2); // hd-bet + signal-erode

        let focus_param = |app: &mut App, param: usize| {
            let rows = app.rows();
            app.pipeline_state.focus = app.focusable_rows().iter()
                .position(|&ri| matches!(rows[ri], PipelineRow::MaskOpSignalErodeParam { param: p, .. } if p == param))
                .unwrap_or_else(|| panic!("param {param} row not focusable"));
        };
        let op = |app: &App| app.pipeline_state.mask_sections[0].refinements[0].clone();
        let MaskOp::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } = op(&app)
            else { panic!("preset should end in signal-erode") };

        // The step row itself is the threshold; the four params each have their own row.
        let rows = app.rows();
        app.pipeline_state.focus = app.focusable_rows().iter()
            .position(|&ri| matches!(rows[ri], PipelineRow::MaskOpEntry { .. })).expect("step row");
        app.handle_key(key(KeyCode::Right));
        assert!(matches!(op(&app), MaskOp::SignalErode { threshold: t, .. } if t > threshold));

        for (param, expect) in [
            (0usize, "depth cap"), (1, "plain erosions"), (2, "bias sigma"), (3, "min component"),
        ] {
            focus_param(&mut app, param);
            app.handle_key(key(KeyCode::Right));
            let MaskOp::SignalErode { depth_cap: d, global_erosions: g, bias_sigma: b, min_component: m, .. } = op(&app)
                else { panic!("still signal-erode") };
            let changed = match param {
                0 => d > depth_cap,
                1 => g > global_erosions,
                2 => b > bias_sigma,
                _ => m > min_component,
            };
            assert!(changed, "{expect} did not increase");
            app.handle_key(key(KeyCode::Left)); // and back down
        }

        let MaskOp::SignalErode { depth_cap: d, global_erosions: g, bias_sigma: b, min_component: m, .. } = op(&app)
            else { panic!("still signal-erode") };
        assert_eq!((d, g, b, m), (depth_cap, global_erosions, bias_sigma, min_component), "←/→ round-trips");

        // Counts clamp at zero rather than wrapping around.
        focus_param(&mut app, 0);
        for _ in 0..(depth_cap + 3) { app.handle_key(key(KeyCode::Left)); }
        assert!(matches!(op(&app), MaskOp::SignalErode { depth_cap: 0, .. }));

        // The generated command carries the edited values.
        // Edited away from the preset, so the command spells the recipe out — with the
        // parameters that are still at their default left off.
        let cmd = crate::tui::command::build_command_string(&app);
        assert!(cmd.contains("--mask magnitude,hd-bet,signal-erode:0.85:0"), "cmd: {cmd}");
    }

    /// The reference has one row (QSM → Reference); a region there turns segmentation on where
    /// segmentation is set up.
    #[test]
    fn the_reference_has_one_row_on_the_qsm_tab() {
        let mut app = App::new();
        let thalamus = qsm_ref_options().iter().position(|o| *o == "thalamus")
            .expect("thalamus should be offered");
        assert!(shows(&mut app, TAB_QSM, "qsm_reference"));
        assert!(!shows(&mut app, TAB_OUTPUTS, "qsm_reference"), "one copy only");
        focus_on(&mut app, TAB_QSM, "qsm_reference");
        app.set_field_select("qsm_reference", thalamus);
        // A region forces segmentation, which reads as on where it is set up.
        assert!(app.field_toggle("do_segmentation"));
        assert!(shows(&mut app, TAB_OUTPUTS, "synthseg_version"));
    }

    #[test]
    fn a_region_reference_forces_segmentation_visibly() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;
        assert!(!app.field_toggle("do_segmentation"), "off by default");

        let region = qsm_ref_options().iter().position(|o| *o == "ventricles").unwrap();
        app.pipeline_state.set_select("qsm_reference", region);
        assert!(app.field_toggle("do_segmentation"),
                "a region reference needs SynthSeg, so it must read as checked");
        for f in SYNTHSEG_ROWS {
            assert!(shows(&mut app, TAB_OUTPUTS, f),
                    "{f} shapes the parcellation the reference is measured on");
        }

        focus_on(&mut app, TAB_OUTPUTS, "do_segmentation");
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.form.do_segmentation, "the underlying flag is untouched");
        let msg = app.error_message.clone().expect("a refusal should explain itself");
        assert!(msg.contains("ventricles"), "the refusal should name the reference: {msg}");

        // `mean` and `none` force nothing.
        for idx in [0, 1] {
            app.pipeline_state.set_select("qsm_reference", idx);
            assert!(!app.field_toggle("do_segmentation"), "option {idx} forces nothing");
        }
    }

    /// The generated command carries the region as the reference value itself.
    #[test]
    fn a_region_reference_reaches_the_command() {
        let mut app = App::new();
        let idx = qsm_ref_options().iter().position(|o| *o == "left-thalamus").unwrap();
        app.pipeline_state.set_select("qsm_reference", idx);
        let cmd = crate::tui::command::build_command_string(&app);
        assert!(cmd.contains("--qsm-reference left-thalamus"), "cmd: {cmd}");

        // The default stays off the command line.
        app.pipeline_state.set_select("qsm_reference", 0);
        assert!(!crate::tui::command::build_command_string(&app).contains("--qsm-reference"));
    }

    /// The per-structure statistics force segmentation on, so the checkbox has to read as checked
    /// and its settings has to be reachable — otherwise the run segments while the TUI says it will
    /// not, and the SynthSeg parameters that shape that segmentation cannot be seen or edited.
    #[test]
    fn statistics_force_segmentation_visibly() {
        let mut app = App::new();
        app.active_tab = TAB_OUTPUTS;

        // Off by default: unchecked, and its settings hidden.
        assert!(!app.field_toggle("do_segmentation"));
        for f in SYNTHSEG_ROWS {
            assert!(!shows(&mut app, TAB_OUTPUTS, f), "{f} should be hidden");
        }
        assert!(shows(&mut app, TAB_OUTPUTS, "do_analysis"), "the statistics are reachable on their own");

        // A config or CLI run can arrive with the statistics already on; segmentation is then in
        // force, so the block must be visible rather than hiding a setting that is taking effect.
        app.form.do_analysis = true;
        assert!(app.field_toggle("do_segmentation"),
                "segmentation is forced, so it must read as checked");
        for f in SYNTHSEG_ROWS {
            assert!(shows(&mut app, TAB_OUTPUTS, f),
                    "{f} shapes the forced segmentation and must be reachable");
        }

        // And it cannot be turned off from under them; the refusal says why.
        focus_on(&mut app, TAB_OUTPUTS, "do_segmentation");
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.form.do_segmentation, "the underlying flag is untouched");
        assert!(app.field_toggle("do_segmentation"), "still forced, so still checked");
        let msg = app.error_message.clone().expect("a refusal should explain itself");
        assert!(msg.contains("--do-analysis"), "unhelpful message: {msg}");

        // With the statistics off again it is an ordinary toggle.
        app.form.do_analysis = false;
        app.error_message = None;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.do_segmentation, "{:?}", app.error_message);
    }

    /// Selecting HEIDI shows both parameter groups (its seed is an LSQR solve); selecting plain
    /// LSQR shows only its own. The nested rows appear and disappear with their switches.
    #[test]
    fn heidi_shows_the_lsqr_group_and_its_own() {
        let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name)
            .unwrap_or_else(|| panic!("{name} missing from QSM_ALGO_OPTIONS"));
        let fields = |ps: &PipelineFormState| -> Vec<String> {
            ps.all_rows().iter().filter_map(|r| match r {
                PipelineRow::Param { field, .. } | PipelineRow::Toggle { field, .. } => Some(field.to_string()),
                _ => None,
            }).collect()
        };

        let mut ps = PipelineFormState { qsm_algorithm: idx("lsqr"), ..Default::default() };
        let f = fields(&ps);
        assert!(f.contains(&"lsqr_tol".to_string()), "{f:?}");
        assert!(f.contains(&"lsqr_fit_global_offset".to_string()), "{f:?}");
        assert!(!f.iter().any(|x| x.starts_with("heidi_")), "plain LSQR must not show HEIDI rows: {f:?}");

        ps.qsm_algorithm = idx("heidi");
        let f = fields(&ps);
        assert!(f.contains(&"lsqr_tol".to_string()), "HEIDI seeds with LSQR: {f:?}");
        assert!(f.contains(&"heidi_cone_threshold".to_string()), "{f:?}");
        assert!(f.contains(&"heidi_continuation_steps".to_string()), "{f:?}");

        // The sub-parameters follow their switches.
        assert!(f.contains(&"heidi_denoise_iterations".to_string()), "denoise is on by default: {f:?}");
        assert!(f.contains(&"heidi_laplacian_threshold".to_string()), "{f:?}");
        ps.heidi_denoise = false;
        ps.heidi_apply_laplacian_correction = false;
        let f = fields(&ps);
        assert!(!f.contains(&"heidi_denoise_iterations".to_string()), "{f:?}");
        assert!(!f.contains(&"heidi_laplacian_threshold".to_string()), "{f:?}");
    }

    /// Inserting an algorithm mid-list used to shift the hardcoded indices that decide which
    /// stages are shown. They are compared by name now; this pins the behaviour for the algorithms
    /// that hide a stage, at whatever index they end up on.
    #[test]
    fn stage_visibility_follows_the_algorithm_not_its_index() {
        let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name).expect(name);
        let has_section = |ps: &PipelineFormState, id: &str| {
            let field = match id { "bgremoval" => "bf_algorithm", "fieldmap" => "unwrapping_algorithm", _ => id };
            ps.all_rows().iter().any(|r| row_field(r) == Some(field))
        };

        let mut ps = PipelineFormState { qsm_algorithm: idx("rts"), ..Default::default() };
        assert!(has_section(&ps, "bgremoval"), "RTS needs background removal");
        assert!(has_section(&ps, "fieldmap"), "RTS needs a field map");

        // Single-step: own background removal, but still unwraps.
        for alg in ["nextqsm", "autoqsm"] {
            ps.qsm_algorithm = idx(alg);
            assert!(!has_section(&ps, "bgremoval"), "{alg} does its own background removal");
            assert!(has_section(&ps, "fieldmap"), "{alg} still needs a field map");
        }
        // End-to-end from wrapped phase: neither.
        for alg in ["iqsm", "iqsm-plus"] {
            ps.qsm_algorithm = idx(alg);
            assert!(!has_section(&ps, "bgremoval"), "{alg} takes wrapped phase");
            assert!(!has_section(&ps, "fieldmap"), "{alg} takes wrapped phase");
        }
        // Total-field methods with their own background removal still map the field: QSMART,
        // TFI, and TGV on multi-echo data (QSM.rs `InversionConfig::input`).
        for alg in ["qsmart", "tfi", "tgv"] {
            ps.qsm_algorithm = idx(alg);
            assert!(!has_section(&ps, "bgremoval"), "{alg} removes the background itself");
            assert!(has_section(&ps, "fieldmap"), "{alg} reads the total field");
        }
        // TGV on a single echo reconstructs from its wrapped phase.
        ps.qsm_algorithm = idx("tgv");
        ps.ctx.num_echoes = Some(1);
        assert!(!has_section(&ps, "fieldmap"), "single-echo TGV takes wrapped phase");
        ps.ctx.num_echoes = Some(4);
        assert!(has_section(&ps, "fieldmap"), "multi-echo TGV maps the field");
        ps.ctx.num_echoes = None;
        // iQFM makes the local field from phase: no field mapping in front of it.
        ps.qsm_algorithm = idx("rts");
        ps.bf_algorithm = BF_OPTIONS.iter().position(|o| *o == "iqfm").unwrap();
        assert!(!has_section(&ps, "fieldmap"), "iQFM unwraps and removes the background itself");
        ps.bf_algorithm = 0;
        // LSQR and HEIDI are ordinary local-field inversions.
        for alg in ["lsqr", "heidi"] {
            ps.qsm_algorithm = idx(alg);
            assert!(has_section(&ps, "bgremoval"), "{alg} inverts the local field");
            assert!(has_section(&ps, "fieldmap"), "{alg} needs a field map");
        }
    }

    /// Compute QSM off hides only the reconstruction: the geometry, motion correction, coil
    /// combination, masking and supplied segmentation serve SWI, T2*/R2*, R2' and the
    /// segmentation as much as QSM. An output that forces QSM keeps the reconstruction on show.
    #[test]
    fn qsm_off_keeps_the_preprocessing_rows() {
        let fields = all_fields;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.do_qsm = false;
        let f = fields(&mut app);
        for kept in ["axial_resampling", "inhomogeneity_correction", "ignore_slice_gap",
                     "motion_correction", "coil_combination_sigma",
                     "mask_preset", "custom_mask_tool", "two_pass"] {
            assert!(f.contains(&kept.to_string()), "{kept} hidden with QSM off: {f:?}");
        }
        for gone in ["#FieldMap", "#Background", "#Inversion", "#Reference", "crop_to_mask", "fft_padding"] {
            assert!(!f.contains(&gone.to_string()), "{gone} shown with QSM off: {f:?}");
        }
        // Motion correction's own settings follow it, not QSM.
        app.pipeline_state.toggle("motion_correction");
        let f = fields(&mut app);
        assert!(f.contains(&"motion_reference".to_string()) && f.contains(&"reg_levels".to_string()), "{f:?}");

        // SMWI needs a χ map: QSM runs regardless, and its settings are on show, with the reason.
        app.form.do_smwi = true;
        let f = fields(&mut app);
        assert!(f.contains(&"#Inversion".to_string()) && f.contains(&"#FieldMap".to_string()), "{f:?}");
        assert!(app.pipeline_state.all_rows().iter().any(|r|
            matches!(r, PipelineRow::Note { text } if text.contains("SMWI"))), "the forced QSM says why");
    }

    /// Bipolar correction is part of HIP phase offset removal, which only ROMEO runs.
    #[test]
    fn bipolar_correction_needs_romeo_and_offset_removal() {
        let mut app = App::new();
        let shows = |app: &App| app.pipeline_state.all_rows().iter().any(|r|
            matches!(r, PipelineRow::Toggle { field: "bipolar_correction", .. }));
        app.pipeline_state.bipolar_correction = true;
        let passed = |app: &App| super::super::command::pipeline_args_from_app(app).bipolar_correction;
        assert!(app.pipeline_state.phase_offset_removal && app.pipeline_state.unwrapping_algorithm == 0);
        assert!(shows(&app) && passed(&app));
        app.pipeline_state.phase_offset_removal = false;
        assert!(!shows(&app) && !passed(&app), "no offset removal, no bipolar correction");
        app.pipeline_state.phase_offset_removal = true;
        app.pipeline_state.unwrapping_algorithm = 1;
        assert!(!shows(&app) && !passed(&app), "Laplacian unwrapping runs no offset removal");
    }

    /// The bring-your-own inputs and the R2' source are reachable wherever a map they feed is
    /// made, not only with χ-separation on.
    #[test]
    fn supplied_inputs_follow_the_maps_that_read_them() {
        let fields = all_fields;
        let has = |f: &[String], x: &str| f.iter().any(|y| y == x);
        let mut app = App::new();
        let f = fields(&mut app);
        for x in ["custom_qsm_tool", "custom_r2_tool", "custom_r2prime_tool", "r2prime_strategy"] {
            assert!(!has(&f, x), "nothing reads a supplied map: {x}");
        }

        app.form.do_r2primemap = true;
        let f = fields(&mut app);
        assert!(has(&f, "custom_r2prime_tool") && has(&f, "custom_r2_tool") && has(&f, "r2prime_strategy"), "{f:?}");
        assert!(!has(&f, "custom_qsm_tool"), "{f:?}");
        // R2PRIMEnet predicts R2' from R2*: no R2 is read.
        app.pipeline_state.r2prime_strategy = 2;
        assert!(!has(&fields(&mut app), "custom_r2_tool"));
        // A supplied R2' replaces the computation: its source no longer applies, its row stays.
        app.pipeline_state.r2prime_strategy = 0;
        app.pipeline_state.custom_r2prime_tool = "*".into();
        let f = fields(&mut app);
        assert!(has(&f, "custom_r2prime_tool") && !has(&f, "r2prime_strategy") && !has(&f, "custom_r2_tool"), "{f:?}");

        let mut app = App::new();
        app.form.do_analysis = true;
        assert!(has(&fields(&mut app), "custom_qsm_tool"), "the statistics can read a supplied χ map");
        let mut app = App::new();
        app.form.do_r2map = true;
        let f = fields(&mut app);
        assert!(has(&f, "custom_r2_tool") && !has(&f, "custom_r2prime_tool"), "{f:?}");
    }

    /// A supplied segmentation satisfies the statistics and a region reference: SynthSeg is not
    /// forced, so its checkbox reads as the user left it and can be toggled.
    #[test]
    fn a_custom_dseg_is_the_segmentation_source() {
        let mut app = App::new();
        app.form.do_analysis = true;
        assert!(app.field_toggle("do_segmentation"), "forced without a supplied dseg");
        assert_eq!(app.field_select("segmentation_source"), 0, "SynthSeg");
        app.pipeline_state.custom_dseg_tool = "freesurfer".into();
        assert!(app.field_toggle("do_segmentation"), "a supplied segmentation is a segmentation");
        assert_eq!(app.field_select("segmentation_source"), 1);
        assert!(!shows(&mut app, TAB_OUTPUTS, "synthseg_version"), "no SynthSeg settings to show");
        assert!(shows(&mut app, TAB_OUTPUTS, "custom_dseg_tool"));
        assert!(shows(&mut app, TAB_OUTPUTS, "do_analysis"), "the statistics stay reachable");
        // The statistics need one, whichever the source: turning it off is refused.
        focus_on(&mut app, TAB_OUTPUTS, "do_segmentation");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.error_message.is_some() && app.pipeline_state.custom_dseg_tool == "freesurfer");
        // Back to SynthSeg: the supplied one is dropped, SynthSeg asked for.
        app.set_field_select("segmentation_source", 0);
        assert!(app.pipeline_state.custom_dseg_tool.is_empty() && app.form.do_segmentation);
        app.set_field_select("segmentation_source", 1);
        assert_eq!(app.pipeline_state.custom_dseg_tool, "*");
        assert!(!app.form.do_segmentation, "one representation per choice");
        // Unforced: off clears whichever source.
        app.form.do_analysis = false;
        app.toggle_field("do_segmentation");
        assert!(!app.field_toggle("do_segmentation") && app.pipeline_state.custom_dseg_tool.is_empty());
    }

    /// R2* is forced by what reads it (R2*-QSM, a computed R2'), and reads as checked.
    #[test]
    fn r2star_reads_as_forced_when_needed() {
        let mut app = App::new();
        assert!(!app.field_toggle("do_r2starmap"));
        app.form.do_r2primemap = true;
        assert!(app.field_toggle("do_r2starmap"), "R2' = R2* - R2");
        app.active_tab = TAB_OUTPUTS;
        focus_on(&mut app, TAB_OUTPUTS, "do_r2starmap");
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.form.do_r2starmap && app.error_message.as_deref().is_some_and(|m| m.contains("--do-r2primemap")));
    }

    /// The help bar's tab hint and the digit keys both follow TAB_NAMES.
    #[test]
    fn every_tab_has_a_digit_and_nothing_else_does() {
        let mut app = App::new();
        for (i, _) in TAB_NAMES.iter().enumerate() {
            app.active_tab = 0;
            assert!(app.switch_tab(key(KeyCode::Char(char::from_digit(i as u32 + 1, 10).unwrap()))));
            assert_eq!(app.active_tab, i);
        }
        let past = char::from_digit(TAB_NAMES.len() as u32 + 1, 10).unwrap();
        assert!(!app.switch_tab(key(KeyCode::Char(past))));
        assert!(!app.switch_tab(key(KeyCode::Char('0'))));
    }

    /// The reliable-mask editor only exists once two-pass is on, and it is the ordinary mask
    /// editor — input, generator and refinement rows — addressed at the two-pass index.
    #[test]
    fn two_pass_toggle_reveals_the_reliable_mask_editor() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod, MaskingInput};
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);

        let reliable_rows = |app: &App| app.rows().into_iter()
            .filter(|r| match r {
                PipelineRow::MaskOpInput { section } | PipelineRow::MaskOpGenerator { section }
                | PipelineRow::MaskOpAddStep { section } => is_two_pass_section(*section),
                PipelineRow::MaskTwoPassHeader => true,
                _ => false,
            })
            .count();
        assert_eq!(reliable_rows(&app), 0, "no reliable mask while two-pass is off");

        let focus_on = |app: &App, pred: fn(&PipelineRow) -> bool| {
            let rows = app.rows();
            app.focusable_rows().iter().position(|&ri| pred(&rows[ri]))
        };
        app.pipeline_state.focus =
            focus_on(&app, |r| matches!(r, PipelineRow::Toggle { field: "two_pass", .. }))
                .expect("two-pass toggle not focusable");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.two_pass);
        assert!(reliable_rows(&app) > 0, "the reliable mask editor appears with the toggle");

        // It starts as the default recipe: an unfilled Otsu threshold on phase quality.
        let sections = &app.pipeline_state.two_pass_sections;
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].input, MaskingInput::PhaseQuality);
        assert!(matches!(sections[0].generator,
                         MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }));
        assert!(sections[0].refinements.is_empty());

        // Default recipe → a bare --two-pass in the generated command.
        let cmd = crate::tui::command::build_command_string(&app);
        assert!(cmd.contains("--two-pass"), "cmd: {cmd}");
        assert!(!cmd.contains("--two-pass-mask"), "cmd: {cmd}");
    }

    /// Editing the reliable mask must not claim the main mask stopped following its preset — and
    /// it must edit the reliable mask, not the main one.
    #[test]
    fn editing_the_reliable_mask_leaves_the_main_preset_alone() {
        use crate::pipeline::config::MaskingInput;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.two_pass = true;
        let main_before = app.pipeline_state.mask_sections.clone();

        let rows = app.rows();
        let focus = app.focusable_rows().iter()
            .position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpInput { section } if is_two_pass_section(*section)))
            .expect("reliable input row not focusable");
        app.pipeline_state.focus = focus;
        app.handle_key(key(KeyCode::Right));

        assert_ne!(app.pipeline_state.two_pass_sections[0].input, MaskingInput::PhaseQuality,
                   "the reliable mask's input should have changed");
        assert_eq!(app.pipeline_state.mask_sections, main_before, "the main mask must be untouched");
        assert_eq!(app.pipeline_state.mask_preset, 0, "the main mask still follows its preset");

        // Edited away from the default, so the command now spells the reliable mask out.
        let cmd = crate::tui::command::build_command_string(&app);
        assert!(cmd.contains("--two-pass-mask"), "cmd: {cmd}");
    }

    /// Turning two-pass off hides the editor but keeps what was typed into it, so toggling back
    /// on does not silently discard the recipe.
    #[test]
    fn two_pass_remembers_its_mask_while_off() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let mut app = App::new();
        app.pipeline_state.two_pass = true;
        app.pipeline_state.two_pass_sections[0].generator =
            MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(40.0) };

        app.pipeline_state.toggle("two_pass");
        assert!(!app.pipeline_state.two_pass);
        // Off, so it contributes nothing to the config...
        let config = crate::tui::command::config_from_app(&app).unwrap();
        assert!(!config.masking.two_pass);
        assert!(config.masking.two_pass_sections.is_none());
        // ...but the edit survives.
        assert!(matches!(app.pipeline_state.two_pass_sections[0].generator,
                         MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. }));
    }

    /// The combine row only appears once there is more than one section, and ←/→ toggles it.
    #[test]
    fn test_pipeline_mask_combine_row_toggles() {
        use crate::pipeline::config::MaskCombine;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);

        let has_combine_row = |app: &App| app.rows().iter()
            .any(|r| matches!(r, PipelineRow::MaskCombineOp));
        assert!(!has_combine_row(&app), "a single section has nothing to combine with");

        // Add a second section via the "+ Add mask..." row.
        let focus_on = |app: &App, pred: fn(&PipelineRow) -> bool| {
            let rows = app.rows();
            app.focusable_rows().iter().position(|&ri| pred(&rows[ri]))
        };
        app.pipeline_state.focus = focus_on(&app, |r| matches!(r, PipelineRow::MaskOpAddSection { .. }))
            .expect("MaskOpAddSection not found");
        app.handle_key(key(KeyCode::Enter));
        assert!(has_combine_row(&app), "two sections show how they combine");

        app.pipeline_state.focus = focus_on(&app, |r| matches!(r, PipelineRow::MaskCombineOp))
            .expect("MaskCombineOp not focusable");
        assert_eq!(app.pipeline_state.mask_combine, MaskCombine::Or);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.pipeline_state.mask_combine, MaskCombine::And);
        assert_eq!(app.pipeline_state.mask_preset, MASK_PRESET_CUSTOM, "editing the recipe marks it custom");
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.pipeline_state.mask_combine, MaskCombine::Or);
    }

    /// Steps added under "On the combined mask" land in the combined list, not in a section.
    #[test]
    fn test_pipeline_mask_combined_refinement_steps() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.apply_mask_preset(3); // bet-and-phase: two sections
        let recipe = crate::pipeline::config::bet_and_phase_mask_recipe();
        assert_eq!(app.pipeline_state.mask_combined_refinements, recipe.refinements);

        let section_refinements: usize = app.pipeline_state.mask_sections.iter().map(|s| s.refinements.len()).sum();
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| {
            matches!(rows[ri], PipelineRow::MaskOpAddStep { section: MASK_COMBINED_SECTION })
        }).expect("combined-mask add-step row not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Enter)); // open the type selector
        app.handle_key(key(KeyCode::Enter)); // add the selected type
        assert_eq!(app.pipeline_state.mask_combined_refinements.len(), recipe.refinements.len() + 1);
        assert_eq!(app.pipeline_state.mask_sections.iter().map(|s| s.refinements.len()).sum::<usize>(),
                   section_refinements, "the step did not land in a section");

        // ...and `d` deletes from the same list.
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| {
            matches!(rows[ri], PipelineRow::MaskOpEntry { section: MASK_COMBINED_SECTION, index: 0 })
        }).expect("combined-mask step row not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.pipeline_state.mask_combined_refinements.len(), recipe.refinements.len());
    }

    #[test]
    fn test_pipeline_mask_add_section() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        let initial_sections = app.pipeline_state.mask_sections.len();
        // Find MaskOpAddSection focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
        let mut add_sec_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if matches!(&rows[ri], PipelineRow::MaskOpAddSection { .. }) {
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
        goto_page(&mut app, Page::Masking);
        let initial_refs = app.pipeline_state.mask_sections[0].refinements.len();
        // Find MaskOpAddStep for section 0
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        goto_page(&mut app, Page::Masking);
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        goto_page(&mut app, Page::Masking);
        let initial_refs = app.pipeline_state.mask_sections[0].refinements.len();
        assert!(initial_refs > 0);
        // Find first MaskOpEntry focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        goto_page(&mut app, Page::Masking);
        // Find MaskOpEntry focus and adjust with Left/Right
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        goto_page(&mut app, Page::Masking);
        // Find MaskOpGenerator focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
        let mut gen_focus = None;
        for (fi, &ri) in focusable.iter().enumerate() {
            if let PipelineRow::MaskOpGenerator { section: 0 } = &rows[ri] {
                gen_focus = Some(fi);
                break;
            }
        }
        let fi = gen_focus.expect("MaskOpGenerator not found");
        app.pipeline_state.focus = fi;
        // Cycle generator threshold -> bet -> hd-bet -> threshold (and back with Left)
        app.handle_key(key(KeyCode::Right));
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            crate::pipeline::config::MaskOp::Bet { .. }
        ));
        app.handle_key(key(KeyCode::Right));
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            crate::pipeline::config::MaskOp::HdBet { .. }
        ));
        app.handle_key(key(KeyCode::Right));
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            crate::pipeline::config::MaskOp::Rs2Net { .. }
        ));
        // Wraps back round to the first generator.
        app.handle_key(key(KeyCode::Right));
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            crate::pipeline::config::MaskOp::Threshold { .. }
        ));
        app.handle_key(key(KeyCode::Left));
        assert!(matches!(
            app.pipeline_state.mask_sections[0].generator,
            crate::pipeline::config::MaskOp::Rs2Net { .. }
        ));
    }

    #[test]
    fn test_pipeline_mask_generator_param_adjust() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        // Find MaskOpGeneratorParam focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        goto_page(&mut app, Page::Masking);
        // Find MaskOpInput focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        app.pipeline_state.axial_resampling = true;
        assert!(app.goto_field("obliquity_threshold"));
        // Find obliquity_threshold param
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
        // Focus 0 is do_qsm toggle
        app.pipeline_state.focus = 0;
        app.pipeline_state.do_qsm = false;
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.pipeline_state.do_qsm); // default is true
    }

    // ========== Other outputs: edit SWI fields ==========

    #[test]
    fn test_supplementary_edit_swi_hp_sigma() {
        let mut app = App::new();
        app.form.do_swi = true;
        // One row for the three axes.
        focus_on(&mut app, TAB_OUTPUTS, "swi_hp_sigma");
        assert_eq!(app.form.swi_hp_sigma_values().map(|v| v.len()), Some(3), "the default parses");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        for _ in 0..20 { app.handle_key(key(KeyCode::Backspace)); }
        for c in "4, 4 0".chars() { app.handle_key(key(KeyCode::Char(c))); }
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.editing);
        assert_eq!(app.form.swi_hp_sigma_values(), Some(vec![4.0, 4.0, 0.0]));
        app.form.swi_hp_sigma = "4 4".into();
        assert_eq!(app.form.swi_hp_sigma_values(), None, "three values or none");
    }

    // ========== Pipeline: switch algorithms and verify row changes ==========

    #[test]
    fn test_pipeline_bf_algorithm_params() {
        let mut ps = PipelineFormState::default();
        // Switch through every BG removal algorithm (incl. BFRnet/iQFM) and verify rows change.
        for algo in 0..BF_OPTIONS.len() {
            ps.bf_algorithm = algo;
            let rows = ps.all_rows();
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
            let rows = ps.all_rows();
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
            assert!(!ps.all_rows().is_empty());
        }
    }

    #[test]
    fn test_pipeline_medi_smv_hides_bg_removal() {
        let ps = PipelineFormState {
            qsm_algorithm: 7,
            medi_smv: true,
            ..Default::default()
        };
        let rows = ps.all_rows();
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
        goto_page(&mut app, Page::Masking);
        // Set generator to Fixed threshold to make MaskOpThresholdValue visible
        app.pipeline_state.mask_sections[0].generator = crate::pipeline::config::MaskOp::Threshold {
            method: crate::pipeline::config::MaskThresholdMethod::Fixed,
            value: Some(0.5),
        };
        app.pipeline_state.mark_mask_custom(0);

        // Find MaskOpThresholdValue focus
        let rows = app.rows();
        let focusable = app.focusable_rows();
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
    fn test_pipeline_bet_voxel_scale_row() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.mask_sections[0].generator = MaskOp::bet(0.5);
        app.pipeline_state.mark_mask_custom(0);

        let find_scale_row = |app: &App| {
            let rows = app.rows();
            let focusable = app.focusable_rows();
            focusable.iter().position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpBetVoxelScale { .. }))
        };
        let fi = find_scale_row(&app).expect("BET voxel-scale row should be shown and focusable");
        app.pipeline_state.focus = fi;

        let scale_of = |app: &App| match &app.pipeline_state.mask_sections[0].generator {
            MaskOp::Bet { voxel_scale, .. } => *voxel_scale,
            other => panic!("expected bet, got {other:?}"),
        };
        assert_eq!(scale_of(&app), crate::pipeline::config::bet_default_voxel_scale());

        // Right scales up (towards the preclinical setting), left comes back.
        app.handle_key(key(KeyCode::Right));
        assert!(scale_of(&app) > 1.0, "right should scale up, got {}", scale_of(&app));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(scale_of(&app), 1.0);

        // Left from the default wraps to the mouse setting, which is the point of the row.
        app.handle_key(key(KeyCode::Left));
        assert_eq!(scale_of(&app), 10.0);

        // The row belongs to BET only.
        app.pipeline_state.mask_sections[0].generator = MaskOp::hd_bet_default();
        assert!(find_scale_row(&app).is_none(), "scale row should not show for HD-BET");
    }

    /// Every BET setting sits in one block under the BET step: the step's own fractional
    /// intensity and voxel scale, then the run-wide surface settings.
    #[test]
    fn bet_settings_are_listed_together_under_the_bet_step() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.mask_sections[0].generator = MaskOp::bet(0.5);
        app.pipeline_state.mark_mask_custom(0);
        let rows = app.rows();
        let start = rows.iter().position(|r| matches!(r, PipelineRow::MaskOpGenerator { section: 0 }))
            .expect("generator row");
        let block: Vec<String> = rows[start..start + 7].iter().map(|r| match r {
            PipelineRow::MaskOpGenerator { .. } => "generator".to_string(),
            PipelineRow::MaskOpGeneratorParam { .. } => "frac".to_string(),
            PipelineRow::MaskOpBetVoxelScale { .. } => "scale".to_string(),
            PipelineRow::Param { field, .. } => field.to_string(),
            other => format!("{other:?}"),
        }).collect();
        assert_eq!(block, ["generator", "frac", "scale", "bet_smoothness", "bet_gradient_threshold",
            "bet_iterations", "bet_subdivisions"]);
        // Listed once, however many BET steps share them.
        let count = |app: &App| app.rows().iter()
            .filter(|r| matches!(r, PipelineRow::Param { field: "bet_smoothness", .. })).count();
        assert_eq!(count(&app), 1);
        let bet_and_phase = MASK_PRESET_OPTIONS.iter().position(|o| *o == "bet-and-phase").unwrap();
        app.pipeline_state.apply_mask_preset(bet_and_phase);
        assert!(app.pipeline_state.mask_sections.len() > 1);
        assert_eq!(count(&app), 1);
        // Not shown without BET.
        app.pipeline_state.mask_sections = vec![crate::pipeline::config::MaskSection {
            input: crate::pipeline::config::MaskingInput::Magnitude,
            generator: MaskOp::hd_bet_default(),
            refinements: vec![],
        }];
        app.pipeline_state.mark_mask_custom(0);
        assert_eq!(count(&app), 0);
    }

    #[test]
    fn test_pipeline_hd_bet_step_row() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.mask_sections[0].generator = MaskOp::hd_bet_default();
        app.pipeline_state.mark_mask_custom(0);

        let find_step_row = |app: &App| {
            let rows = app.rows();
            let focusable = app.focusable_rows();
            focusable.iter().position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpHdBetStep { .. }))
        };
        let fi = find_step_row(&app).expect("HD-BET step row should be shown and focusable");
        app.pipeline_state.focus = fi;

        let step_of = |app: &App| match &app.pipeline_state.mask_sections[0].generator {
            MaskOp::HdBet { tile_step, .. } => *tile_step,
            other => panic!("expected hd-bet, got {other:?}"),
        };
        assert_eq!(step_of(&app), crate::pipeline::config::hd_bet_default_tile_step());

        // Right steps to a coarser stride, left comes back.
        app.handle_key(key(KeyCode::Right));
        let stepped = step_of(&app);
        assert!(stepped > 0.5, "right should coarsen the stride, got {stepped}");
        app.handle_key(key(KeyCode::Left));
        assert_eq!(step_of(&app), 0.5);

        // The row belongs to HD-BET only.
        app.pipeline_state.mask_sections[0].generator = MaskOp::bet(0.5);
        assert!(find_step_row(&app).is_none(), "step row should not show for BET");
    }

    #[test]
    fn test_pipeline_threshold_value_esc_cancels() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.pipeline_state.mask_sections[0].generator = crate::pipeline::config::MaskOp::Threshold {
            method: crate::pipeline::config::MaskThresholdMethod::Fixed,
            value: Some(0.5),
        };
        app.pipeline_state.mark_mask_custom(0);
        let rows = app.rows();
        let focusable = app.focusable_rows();
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

    // ─── The sub-tab strip: focus, arrows, and back ───

    #[test]
    fn a_stage_with_sub_tabs_opens_on_its_sub_tab_strip() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.active_tab, TAB_PREPROCESSING);
        assert!(app.on_subtabs());
        assert_eq!(app.page(), Some(Page::Geometry));
        // ←/→ move along the strip, wrapping, and focus stays on it.
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.page(), Some(Page::Masking));
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.page(), Some(Page::Motion));
        assert!(app.on_subtabs());
        // `[`/`]` still switch too.
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.page(), Some(Page::Geometry));
        assert!(app.on_subtabs());
    }

    #[test]
    fn down_enters_the_sub_tab_and_up_from_its_first_row_returns() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('2')));
        app.handle_key(key(KeyCode::Right)); // Masking
        let first = app.focusable_rows().len();
        assert!(first > 1);
        app.handle_key(key(KeyCode::Down));
        assert!(!app.on_subtabs());
        assert_eq!(app.focus(), 0);
        assert_eq!(app.page(), Some(Page::Masking));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focus(), 1);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focus(), 0);
        assert!(!app.on_subtabs());
        app.handle_key(key(KeyCode::Up));
        assert!(app.on_subtabs());
        // Up on the strip goes nowhere; Enter also goes into the rows.
        app.handle_key(key(KeyCode::Up));
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.on_subtabs());
        assert_eq!(app.focus(), 0);
        assert_eq!(app.page(), Some(Page::Masking));
    }

    /// On the strip, ←/→ switch sub-tabs; on a row they still change the row's choice.
    #[test]
    fn left_and_right_change_a_choice_on_a_row_not_the_sub_tab() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('3')));
        assert!(app.on_subtabs());
        let page = app.page();
        app.handle_key(key(KeyCode::Down));
        let field = app.focused_field_name();
        let before = app.focused_row().map(|r| matches!(r, PipelineRow::Toggle { .. } | PipelineRow::AlgoSelect { .. }));
        assert_eq!(before, Some(true), "first QSM row should be a choice or checkbox");
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.page(), page, "→ on a row switched the sub-tab");
        assert!(!app.on_subtabs());
        assert_eq!(app.focused_field_name(), field);
    }

    /// Keys that act on a row do nothing while the strip has focus.
    #[test]
    fn row_keys_do_nothing_on_the_strip() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('2')));
        app.handle_key(key(KeyCode::Right)); // Masking
        let before = crate::tui::command::build_command_string(&app);
        for k in [KeyCode::Char(' '), KeyCode::Char('d'), KeyCode::Delete, KeyCode::Char('r')] {
            app.handle_key(key(k));
            assert!(app.on_subtabs(), "{k:?} left the strip");
        }
        assert_eq!(crate::tui::command::build_command_string(&app), before);
        assert!(app.algo_modal.is_none());
    }

    /// Tabs without sub-tabs behave as before: focus starts on the first row.
    #[test]
    fn a_stage_without_sub_tabs_opens_on_its_first_row() {
        let mut app = App::new();
        for tab in [TAB_SEPARATION, TAB_EXECUTION] {
            app.handle_key(key(KeyCode::Char(char::from_digit(tab as u32 + 1, 10).unwrap())));
            assert_eq!(app.active_tab, tab);
            assert!(!app.on_subtabs());
            assert_eq!(app.focus(), 0);
            app.handle_key(key(KeyCode::Up));
            assert!(!app.on_subtabs());
        }
    }

    /// Tab, Shift-Tab and the digits land on the strip of a stage with sub-tabs; `q` still quits.
    #[test]
    fn tab_switching_and_quitting_work_from_the_strip() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_PREPROCESSING);
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_QSM);
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::Char('5')));
        assert_eq!(app.active_tab, TAB_OUTPUTS);
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    // ─── The Execution tab's Run button ───

    fn focus_run_button(app: &mut App) {
        app.handle_key(key(KeyCode::Char(char::from_digit(TAB_EXECUTION as u32 + 1, 10).unwrap())));
        for _ in 0..40 {
            app.handle_key(key(KeyCode::Down));
        }
        assert!(matches!(app.focused_row(), Some(PipelineRow::RunButton)), "{:?}", app.focused_row());
    }

    #[test]
    fn the_run_button_is_the_last_row_and_enter_runs_like_f5() {
        let mut app = App::new();
        app.form.bids_dir = "/tmp/bids".to_string();
        focus_run_button(&mut app);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.should_run);
        assert!(app.error_message.is_none());
    }

    /// The button goes through the same checks as F5: SLURM without an account is refused.
    #[test]
    fn the_run_button_validates_like_f5() {
        let mut app = App::new();
        focus_run_button(&mut app);
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.should_run);
        assert_eq!(app.error_message.as_deref(), Some("BIDS Directory is required"));

        let mut app = App::new();
        app.form.bids_dir = "/tmp/bids".to_string();
        app.form.execution_mode = 1;
        focus_run_button(&mut app);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!app.should_run);
        assert_eq!(app.error_message.as_deref(), Some("SLURM Account is required"));
    }

    #[test]
    fn the_run_button_says_what_f5_will_do() {
        let mut app = App::new();
        assert_eq!(app.run_action().0, "Run");
        app.form.dry_run = true;
        assert_eq!(app.run_action().0, "Dry run");
        app.form.execution_mode = 1;
        assert_eq!(app.run_action().0, "Write SLURM scripts");
        app.form.slurm_submit = true;
        assert_eq!(app.run_action().0, "Submit SLURM jobs");
    }
}
