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

use super::mask_builder::MaskRecipe;
use super::settings_rows::{stored_path, stored_placeholder};

/// The supplied segmentation's tool: whether one is named is the segmentation's source.
const DSEG: &str = "segmentation.custom_dseg_tool";

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
    /// Keep the study's DICOM UIDs in the sidecars ([`dicom::convert::ConvertOptions`]).
    KeepUids,
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
    /// Convert with `--keep-dicom-uids`.
    pub keep_uids: bool,
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
            keep_uids: false,
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
                    self.focus = DicomFocus::KeepUids;
                }
            }
            DicomFocus::KeepUids => self.focus = DicomFocus::ConvertButton,
            DicomFocus::ConvertButton => {}
        }
    }

    pub fn focus_prev(&mut self) {
        match self.focus {
            DicomFocus::Series(0) => {} // at top of series, handle_dicom_tab_key goes to IO
            DicomFocus::Series(i) => self.focus = DicomFocus::Series(i - 1),
            DicomFocus::ConvertButton => self.focus = DicomFocus::KeepUids,
            DicomFocus::KeepUids => {
                let series_count = self.session.as_ref().map(|s| s.unique_series_count()).unwrap_or(0);
                if series_count > 0 {
                    self.focus = DicomFocus::Series(series_count - 1);
                }
            }
        }
    }
}

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

// Tab indices. Defined as named constants (usable directly in `match` patterns) so the tab
// order can be changed in ONE place — reorder `TAB_NAMES` and update these — without hunting
// down hardcoded `(tab, field)` literals across the file.

pub const TAB_INPUT: usize = 0;
/// The pipeline to start from ([`super::pipelines_tab`]).
pub const TAB_PIPELINE: usize = 1;
/// A map DICOM export can write: its tick row's field, the series suffix (`--dicom-outputs`
/// token), the label, and the output it belongs to (for "not made by these settings").
pub struct DicomMap {
    pub field: &'static str,
    pub suffix: &'static str,
    pub label: &'static str,
    pub group: &'static str,
}

/// Every map DICOM export writes, in series order ([`crate::dicom::export`]).
pub const DICOM_MAPS: [DicomMap; 17] = [
    DicomMap { field: "dicom_out_magnitude", suffix: "magnitude", label: "  Magnitude", group: "magnitude" },
    DicomMap { field: "dicom_out_chimap", suffix: "Chimap", label: "  χ (QSM)", group: "χ" },
    DicomMap { field: "dicom_out_swi", suffix: "swi", label: "  SWI", group: "SWI" },
    DicomMap { field: "dicom_out_minip", suffix: "minIP", label: "  SWI minIP", group: "SWI" },
    DicomMap { field: "dicom_out_swi_phase", suffix: "desc-swi_phase", label: "  SWI filtered phase", group: "SWI phase" },
    DicomMap { field: "dicom_out_smwi_para", suffix: "desc-paramagnetic_smwi", label: "  Paramagnetic SMWI", group: "SMWI" },
    DicomMap { field: "dicom_out_smwi_dia", suffix: "desc-diamagnetic_smwi", label: "  Diamagnetic SMWI", group: "SMWI" },
    DicomMap { field: "dicom_out_smwi_para_minip", suffix: "desc-paramagneticsmwi_minIP", label: "  Paramagnetic SMWI minIP", group: "SMWI" },
    DicomMap { field: "dicom_out_smwi_dia_minip", suffix: "desc-diamagneticsmwi_minIP", label: "  Diamagnetic SMWI minIP", group: "SMWI" },
    DicomMap { field: "dicom_out_t2starmap", suffix: "T2starmap", label: "  T2* map", group: "T2*" },
    DicomMap { field: "dicom_out_r2starmap", suffix: "R2starmap", label: "  R2* map", group: "R2*" },
    DicomMap { field: "dicom_out_r2map", suffix: "R2map", label: "  R2 map", group: "R2" },
    DicomMap { field: "dicom_out_r2primemap", suffix: "R2primemap", label: "  R2′ map", group: "R2′" },
    DicomMap { field: "dicom_out_chi_para", suffix: "desc-paramagnetic_Chimap", label: "  Paramagnetic χ", group: "χ-separation" },
    DicomMap { field: "dicom_out_chi_dia", suffix: "desc-diamagnetic_Chimap", label: "  Diamagnetic χ", group: "χ-separation" },
    DicomMap { field: "dicom_out_chi_total", suffix: "desc-total_Chimap", label: "  Total χ", group: "χ-separation" },
    DicomMap { field: "dicom_out_dseg", suffix: "dseg", label: "  Segmentation (DICOM SEG)", group: "segmentation" },
];

/// The Export map a tick row's field names.
pub fn dicom_map(field: &str) -> Option<&'static DicomMap> {
    DICOM_MAPS.iter().find(|m| m.field == field)
}

/// Where a Reconstruction section stands for the current settings ([`App::section_state`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionState {
    On,
    /// Its output is not made (χ-separation, SWI, SMWI, …, DICOM export, or QSM itself).
    Off,
    /// The settings skip its stage: no field mapping, no background removal, no orientation groups.
    NotApplicable,
}

/// Every reconstruction setting, its sections ([`Page::RECONSTRUCTION`]) in a sidebar.
pub const TAB_RECONSTRUCTION: usize = 2;
pub const TAB_EXECUTION: usize = 3;
pub const TAB_METHODS: usize = 4;

/// The footer's confirmation after the methods text is copied.
pub const METHODS_COPIED: &str = "Methods copied to clipboard";

/// A page of a tab: a section of the Reconstruction tab's sidebar, or a sub-tab, switched with
/// `[` and `]`. A tab with a single page (Execution) shows neither. The Methods tab's two pages
/// are views, not rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Geometry,
    Masking,
    Motion,
    /// The QSM group's switch: whether QSM is reconstructed at all.
    ComputeQsm,
    FieldMap,
    Background,
    Inversion,
    MultiOrientation,
    Reference,
    Separation,
    /// SMWI: weighted by χ, so in the QSM group.
    Smwi,
    /// The combined magnitude, published or not.
    Magnitude,
    /// SWI: from the phase alone, so an other output.
    Swi,
    Relaxometry,
    Segmentation,
    Export,
    Execution,
    /// The Methods tab's methods text.
    MethodsText,
    /// The Methods tab's pipeline graph ([`super::pipeline_view`]).
    Pipeline,
}

impl Page {
    /// The sub-tab's title.
    pub fn label(self) -> &'static str {
        match self {
            Page::Geometry => "Geometry",
            Page::Masking => "Masking",
            Page::Motion => "Motion",
            Page::ComputeQsm => "Compute QSM",
            Page::FieldMap => "Field map",
            Page::Background => "Background",
            Page::Inversion => "Inversion",
            Page::MultiOrientation => "COSMOS/STI",
            Page::Reference => "Reference",
            Page::Separation => "χ-separation",
            Page::Smwi => "SMWI",
            Page::Magnitude => "Magnitude",
            Page::Swi => "SWI",
            Page::Relaxometry => "Relaxometry",
            Page::Segmentation => "Segmentation",
            Page::Export => "DICOM",
            Page::Execution => "Execution",
            Page::MethodsText => "Methods text",
            Page::Pipeline => "Graph",
        }
    }

    /// Whether the page's rows are the pipeline form's ([`PipelineFormState::visible_rows`])
    /// rather than built by the App from both forms.
    pub fn is_pipeline(self) -> bool {
        !matches!(self, Page::Magnitude | Page::Swi | Page::Smwi | Page::Relaxometry | Page::Segmentation | Page::Export | Page::Execution
            | Page::MethodsText | Page::Pipeline)
    }

    /// The Reconstruction tab's sections, in sidebar order.
    pub const RECONSTRUCTION: [Page; 16] = [
        Page::Geometry, Page::Masking, Page::Motion, Page::ComputeQsm, Page::FieldMap, Page::Background,
        Page::Inversion, Page::MultiOrientation, Page::Reference, Page::Separation, Page::Smwi, Page::Magnitude, Page::Swi,
        Page::Relaxometry, Page::Segmentation, Page::Export,
    ];

    /// The sidebar group a Reconstruction section is in. The QSM group holds QSM's stages and
    /// the outputs made from χ (χ-separation needs the QSM and its local field; SMWI weights by
    /// χ); the other outputs need no QSM.
    pub fn group(self) -> &'static str {
        match self {
            Page::Geometry | Page::Masking | Page::Motion => "Preprocessing",
            Page::ComputeQsm | Page::FieldMap | Page::Background | Page::Inversion | Page::MultiOrientation
            | Page::Reference | Page::Separation | Page::Smwi => "QSM",
            Page::Magnitude | Page::Swi | Page::Relaxometry | Page::Segmentation => "Other outputs",
            Page::Export => "Export",
            Page::Execution => "Execution",
            Page::MethodsText | Page::Pipeline => "Methods",
        }
    }

    /// Every page, in tab order.
    #[cfg(test)]
    pub const ALL: [Page; 19] = [
        Page::Geometry, Page::Masking, Page::Motion, Page::ComputeQsm, Page::FieldMap, Page::Background,
        Page::Inversion, Page::MultiOrientation, Page::Reference, Page::Separation, Page::Smwi, Page::Magnitude, Page::Swi,
        Page::Relaxometry, Page::Segmentation, Page::Export, Page::Execution, Page::MethodsText,
        Page::Pipeline,
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
    /// Focus within the picker: [`Self::BUTTON`] for the Download button above the list, or
    /// the row of an acquisition ([`Self::acquisition`]).
    pub cursor: usize,
    /// Which acquisitions are ticked. Parallel to `EXAMPLES`.
    pub selected: Vec<bool>,
    /// How far the acquisition list is scrolled.
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
        // Arrowing down from the button walks into the list to pick something else.
        let default_idx = crate::example::EXAMPLES
            .iter()
            .position(|e| e.id == crate::example::DEFAULT_ID)
            .unwrap_or(0);
        selected[default_idx] = true;
        Self {
            output_dir: String::new(),
            cursor: Self::BUTTON,
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
    /// The Download button's place in the focus order: first, above the list.
    pub const BUTTON: usize = 0;

    /// The last place in the focus order: the last acquisition.
    pub fn last_row() -> usize {
        crate::example::EXAMPLES.len()
    }

    /// The acquisition the cursor is on, as an index into [`crate::example::EXAMPLES`]; `None`
    /// on the button.
    pub fn acquisition(&self) -> Option<usize> {
        self.cursor.checked_sub(1)
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
    /// Set the download's progress, as the download thread does.
    #[cfg(test)]
    pub fn set_progress(&self, done: u64, total: u64) {
        self.progress.0.store(done, Ordering::Relaxed);
        self.progress.1.store(total, Ordering::Relaxed);
    }

    pub fn progress(&self) -> Option<(u64, u64)> {
        let total = self.progress.1.load(Ordering::Relaxed);
        (total > 0).then(|| (self.progress.0.load(Ordering::Relaxed), total))
    }

    pub fn toggle_selected(&mut self) {
        if let Some(slot) = self.acquisition().and_then(|i| self.selected.get_mut(i)) {
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

pub const TAB_NAMES: [&str; 5] = [
    "Input",
    "Pipeline",
    "Reconstruction",
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


/// Where ←/→ can put a run of the tree ([`FilterTreeState::orientation_slots`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientationSlot {
    /// In no orientation group.
    None,
    /// In group `n` (from 0), not its reference.
    Member(usize),
    /// In group `n`, its reference.
    Reference(usize),
    /// In a group of its own, to put others with.
    New,
}

/// A run's subject and session, which an orientation group stays within.
fn session_of(key: &crate::bids::entities::AcquisitionKey) -> String {
    format!("{}/{}", key.subject, key.session.as_deref().unwrap_or(""))
}

/// What an orientation-group pattern currently matches.
#[derive(Debug, Clone, Default)]
pub struct OrientationPreview {
    /// The pattern itself was rejected — shown instead of any groups.
    pub error: Option<String>,
    /// One entry per group: its label and its members' display names.
    pub groups: Vec<(String, Vec<String>)>,
    /// Selected runs the pattern did not put in any group. These process normally.
    pub ungrouped: usize,
    /// Each grouped run, by its key: its group's number (from 1, in group order) and whether it
    /// is the group's reference. Shown against the run in the tree.
    pub member_of: std::collections::HashMap<String, (usize, bool)>,
    /// The selected runs' keys, in discovery order.
    pub order: Vec<String>,
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
    ///
    /// `reference` is the orientation the others are registered onto (a setting of the config,
    /// `multi_orientation.reference`).
    pub fn orientation_preview(&self, reference: &str) -> OrientationPreview {
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

        let mut groups = crate::bids::orientation::group_keys(&keys, &pattern);
        let grouped: usize = groups.iter().map(|g| g.members.len()).sum();
        // The reference is listed first, as the run will use it; one that does not resolve is
        // shown as the error the run would refuse with.
        let strings: Vec<String> = keys.iter().map(|(_, s)| s.clone()).collect();
        let error = crate::bids::orientation::apply_reference(
            &mut groups, &strings, reference).err();
        let resolved = error.is_none();
        let member_of = groups.iter().enumerate()
            .flat_map(|(g, group)| group.members.iter().enumerate()
                .map(move |(j, &i)| (i, (g + 1, j == 0 && resolved))))
            .map(|(i, tag)| (strings[i].clone(), tag))
            .collect();
        let error = error.or_else(|| {
            let run_keys: Vec<_> = keys.iter().map(|(k, _)| k.clone()).collect();
            crate::bids::orientation::distinct_outputs(&groups, &run_keys).err()
        });
        OrientationPreview {
            member_of,
            order: strings.clone(),
            error,
            groups: groups
                .into_iter()
                .map(|g| {
                    let mut members: Vec<String> = g.members.iter().map(|&i| displays[i].clone()).collect();
                    if let Some(first) = members.first_mut() {
                        first.push_str(" [reference]");
                    }
                    (g.label, members)
                })
                .collect(),
            ungrouped: keys.len().saturating_sub(grouped),
        }
    }

    /// The orientation grouping, trimmed, while one is set.
    pub fn orientation_group_by(&self) -> Option<String> {
        self.multi_orientation_on().then(|| self.orientation_group.trim().to_string())
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

    /// The focused run of the tree, if a run is focused.
    fn focused_run(&mut self) -> Option<&mut crate::bids::discovery::BidsRunLeaf> {
        let FilterFocus::TreeNode(idx) = self.focus else { return None };
        let row = self.visible_rows().get(idx).cloned()?;
        let tree = self.tree.as_mut()?;
        match row {
            TreeRow::Run { sub, ses: Some(sei), run } => Some(&mut tree.subjects[sub].sessions[sei].runs[run]),
            TreeRow::Run { sub, ses: None, run } => Some(&mut tree.subjects[sub].runs[run]),
            _ => None,
        }
    }

    /// The orientation groups as run keys, each with its reference first: as listed (`runs:`),
    /// or as a typed pattern groups the selected runs now.
    pub fn orientation_groups(&self, reference: &str) -> Vec<Vec<String>> {
        if let Some(groups) = crate::bids::orientation::listed_groups(&self.orientation_group) {
            return groups;
        }
        let preview = self.orientation_preview(reference);
        let mut groups: Vec<Vec<String>> = vec![Vec::new(); preview.groups.len()];
        let mut members: Vec<(&String, &(usize, bool))> = preview.member_of.iter().collect();
        members.sort_by_key(|(k, (g, reference))| (*g, !*reference, preview.order.iter().position(|o| o == *k)));
        for (key, (g, _)) in members {
            groups[g - 1].push(key.clone());
        }
        groups
    }

    /// Where ←/→ can put the focused run, in order, and where it is now: no group, then each
    /// group of its session as a member and as the reference, then a new group.
    pub fn orientation_slots(&mut self, reference: &str) -> Option<(Vec<OrientationSlot>, usize)> {
        let groups = self.orientation_groups(reference);
        let run = self.focused_run()?;
        let (key, session) = (run.key_string.clone(), session_of(&run.key));
        let keys = self.run_keys();
        let same_session = |k: &String| keys.get(k).is_some_and(|s| *s == session);
        let mut slots = vec![OrientationSlot::None];
        let mut now = 0;
        for (g, members) in groups.iter().enumerate() {
            let others: Vec<&String> = members.iter().filter(|k| **k != key).collect();
            if others.is_empty() || !others.iter().all(|k| same_session(k)) {
                continue;
            }
            for slot in [OrientationSlot::Member(g), OrientationSlot::Reference(g)] {
                let here = match slot {
                    OrientationSlot::Member(_) => members.iter().skip(1).any(|k| *k == key),
                    _ => members.first() == Some(&key),
                };
                if here {
                    now = slots.len();
                }
                slots.push(slot);
            }
        }
        if groups.iter().any(|g| g.as_slice() == [key.clone()]) {
            now = slots.len();
        }
        slots.push(OrientationSlot::New);
        Some((slots, now))
    }

    /// ←/→ on a run: move it to the slot before or after its own ([`Self::orientation_slots`]).
    /// The groups are kept as the pattern itself, listed (`runs:`), each with its reference
    /// first, so the config and the command line need nothing new. A typed pattern's groups
    /// are where this starts from.
    ///
    /// Each group's reference is then its first run: the caller clears the reference setting.
    pub fn move_orientation(&mut self, forward: bool, reference: &str) {
        let Some((slots, now)) = self.orientation_slots(reference) else { return };
        let next = if forward { (now + 1).min(slots.len() - 1) } else { now.saturating_sub(1) };
        if next == now {
            return;
        }
        let mut groups = self.orientation_groups(reference);
        let Some(run) = self.focused_run() else { return };
        let key = run.key_string.clone();
        if slots[next] != OrientationSlot::None {
            // Only a selected run is processed, so grouping one selects it.
            run.selected = true;
        }
        for g in &mut groups {
            g.retain(|k| *k != key);
        }
        match slots[next] {
            OrientationSlot::None => {}
            OrientationSlot::Member(g) => groups[g].push(key),
            OrientationSlot::Reference(g) => groups[g].insert(0, key),
            OrientationSlot::New => groups.push(vec![key]),
        }
        self.orientation_group = crate::bids::orientation::runs_pattern(&groups);
        self.orientation_cursor = self.orientation_group.len();
    }

    /// Every run's key, with its subject and session.
    fn run_keys(&self) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
        if let Some(ref tree) = self.tree {
            tree.for_each_run(|leaf| {
                out.insert(leaf.key_string.clone(), session_of(&leaf.key));
            });
        }
        out
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

/// The settings Basic mode shows, by path ([`App::basic_paths`]).
type BasicPaths = std::rc::Rc<std::collections::BTreeSet<&'static str>>;

/// A visible row on a row-based tab.
#[derive(Debug, Clone, PartialEq)]
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
    /// "COMBINED WITH ◀ OR ▶" before `section`, the second or later section of the main or the
    /// reliable mask (focusable, ←/→ toggles that mask's OR/AND; `d` removes `section`)
    MaskCombineOp { section: usize },
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
    /// [`signal_erode_params`] (focusable, ←/→ to adjust)
    MaskOpSignalErodeParam { section: usize, index: usize, param: usize },
    /// "Add step..." row for appending new ops to a section
    MaskOpAddStep { section: usize },
    /// "Add mask..." row for adding a new OR'd section — to the main mask, or to the two-pass
    /// reliable mask when `two_pass` is set
    MaskOpAddSection { two_pass: bool },
    /// The Execution tab's button: Enter does what F5 does ([`App::try_run`]).
    RunButton,
    /// A line that reads as a value (not focusable): the mask recipe, collapsed in Basic mode.
    Summary { text: String },
    /// The last row of a Reconstruction section in Basic mode: how many settings it hides.
    /// Enter (or a click) shows every setting, as `A` does.
    AdvancedFooter { hidden: usize },
}

/// The config setting a TUI row edits, by the row's field name. Rows that edit no config
/// setting (the Execution tab, DICOM export's run options, the mask preset, which writes the
/// recipe) are not listed. The settings tests (`schema_parity`) hold the TUI to it.
pub const FIELD_PATHS: &[(&str, &str)] = &[
    ("axial_resampling", "pipeline.axial_resampling"), ("obliquity_threshold", "pipeline.obliquity_threshold"),
    ("output_space", "pipeline.output_space"), ("inhomogeneity_correction", "masking.inhomogeneity_correction"),
    ("homogeneity_sigma_mm", "homogeneity.sigma_mm"), ("homogeneity_nbox", "homogeneity.nbox"),
    ("inhomogeneity_method", "masking.inhomogeneity_method"), ("n4_shrink_factor", "n4.shrink_factor"),
    ("n4_max_iterations", "n4.max_iterations"), ("n4_convergence_threshold", "n4.convergence_threshold"),
    ("n4_mesh", "n4.mesh"), ("n4_spline_order", "n4.spline_order"), ("n4_bias_field_fwhm", "n4.bias_field_fwhm"),
    ("n4_wiener_noise", "n4.wiener_noise"), ("n4_histogram_bins", "n4.histogram_bins"),
    ("n4_geometry", "masking.n4_geometry"),
    ("ignore_slice_gap", "pipeline.ignore_slice_gap"), ("tukey_window", "pipeline.tukey_window"),
    ("crop_to_mask", "pipeline.crop_to_mask"),
    ("crop_margin_mm", "pipeline.crop_margin_mm"), ("fft_padding", "pipeline.fft_padding"),
    ("coil_combination_sigma", "field_mapping.coil_combination_sigma"),
    ("custom_mask_tool", "masking.custom_mask_tool"), ("two_pass", "masking.two_pass"),
    ("two_pass_mode", "masking.two_pass_mode"),
    ("bet_smoothness", "bet.smoothness"), ("bet_gradient_threshold", "bet.gradient_threshold"),
    ("bet_iterations", "bet.iterations"), ("bet_subdivisions", "bet.subdivisions"),
    ("bet_orientation", "bet.orientation"),
    ("motion_correction", "motion_correction.enabled"), ("motion_reference", "motion_correction.reference"),
    ("motion_reference_echo", "motion_correction.reference"), ("motion_chain", "motion_correction.chain"),
    ("motion_chain_levels", "motion_correction.chain_levels"), ("motion_min_ncc", "motion_correction.min_ncc"),
    ("reg_levels", "registration.levels"), ("reg_max_samples", "registration.max_samples"),
    ("reg_max_iterations", "registration.max_iterations"), ("reg_max_evaluations", "registration.max_evaluations"),
    ("reg_tolerance", "registration.tolerance"), ("reg_rotation_step_deg", "registration.rotation_step_deg"),
    ("reg_translation_step_mm", "registration.translation_step_mm"), ("reg_min_overlap", "registration.min_overlap"),
    ("do_qsm", "pipeline.do_qsm"),
    ("unwrapping_algorithm", "field_mapping.unwrapping_algorithm"), ("laplacian_solver", "field_mapping.laplacian_solver"),
    ("laplacian_fft_pad", "field_mapping.laplacian_fft_pad"), ("laplacian_unmasked", "field_mapping.laplacian_unmasked"), ("laplacian_keep_outside", "field_mapping.laplacian_keep_outside"),
    ("prelude_labelling", "field_mapping.prelude.labelling"), ("prelude_partitions", "field_mapping.prelude.num_phase_partitions"),
    ("qsm_per_echo", "qsm.per_echo"), ("phase_offset_removal", "field_mapping.phase_offset_removal"),
    ("phase_offset_sigma_mm", "field_mapping.phase_offset_sigma_mm"), ("bipolar_correction", "field_mapping.bipolar_correction"),
    ("romeo_individual", "field_mapping.romeo.individual"), ("romeo_template", "field_mapping.romeo.template"),
    ("romeo_correct_global", "field_mapping.romeo.correct_global"), ("romeo_phase_coherence", "field_mapping.romeo.phase_coherence"),
    ("romeo_phase_gradient_coherence", "field_mapping.romeo.phase_gradient_coherence"),
    ("romeo_phase_linearity", "field_mapping.romeo.phase_linearity"), ("romeo_mag_coherence", "field_mapping.romeo.mag_coherence"),
    ("romeo_mag_weight", "field_mapping.romeo.mag_weight"), ("romeo_mag_weight2", "field_mapping.romeo.mag_weight2"),
    ("romeo_bestpath", "field_mapping.romeo.bestpath"),
    ("romeo_temporal_uncertain", "field_mapping.romeo.temporal_uncertain_unwrapping"),
    ("romeo_temporal_uncertain_unwrapping", "field_mapping.romeo.temporal_uncertain_unwrapping"),
    ("romeo_max_seeds", "field_mapping.romeo.max_seeds"), ("romeo_merge_regions", "field_mapping.romeo.merge_regions"),
    ("romeo_correct_regions", "field_mapping.romeo.correct_regions"), ("romeo_wrap_addition", "field_mapping.romeo.wrap_addition"),
    ("b0_estimation", "field_mapping.b0_estimation"), ("b0_weight_type", "field_mapping.b0_weight_type"),
    ("b0_weight_assumed_t2star_ms", "field_mapping.b0_weight_assumed_t2star_ms"),
    ("linear_fit_estimate_offset", "field_mapping.linear_fit.estimate_offset"),
    ("robust_fit_loss", "field_mapping.robust_fit.loss"), ("robust_fit_tuning", "field_mapping.robust_fit.tuning"),
    ("robust_fit_iterations", "field_mapping.robust_fit.iterations"),
    ("robust_fit_downweight_level", "field_mapping.robust_fit.downweight_level"),
    ("robust_fit_flag_ratio", "field_mapping.robust_fit.flag_ratio"),
    ("complex_fit_reltol", "field_mapping.complex_fit.reltol"), ("complex_fit_max_iter", "field_mapping.complex_fit.max_iter"),
    ("bf_algorithm", "bg_removal.algorithm"), ("vsharp_deconvolve", "bg_removal.vsharp.deconvolve"),
    ("vsharp_threshold", "bg_removal.vsharp.threshold"), ("vsharp_max_radius", "bg_removal.vsharp.max_radius"),
    ("vsharp_min_radius", "bg_removal.vsharp.min_radius"),
    ("vsharp_radius_step", "bg_removal.vsharp.radius_step"), ("vsharp_grid", "bg_removal.vsharp.grid"),
    ("vsharp_grid_spacing", "bg_removal.vsharp.grid_spacing"), ("pdf_tol", "bg_removal.pdf.tol"), ("pdf_n_cg", "bg_removal.pdf.n_cg"), ("bf_polish_mask", "bg_removal.polish_mask"), ("bf_ukb_mask_trim", "bg_removal.ukb_mask_trim"), ("bf_erode_voxels", "bg_removal.erode_voxels"),
    ("pdf_space", "bg_removal.pdf.space"), ("pdf_n_pad", "bg_removal.pdf.n_pad"), ("lbv_tol", "bg_removal.lbv.tol"),
    ("lbv_depth", "bg_removal.lbv.depth"), ("lbv_peel", "bg_removal.lbv.peel"), ("lbv_n1", "bg_removal.lbv.n1"),
    ("lbv_n2", "bg_removal.lbv.n2"), ("lbv_n3", "bg_removal.lbv.n3"), ("lbv_precision", "bg_removal.lbv.precision"),
    ("ismv_variant", "bg_removal.ismv.variant"),
    ("ismv_tol", "bg_removal.ismv.tol"), ("ismv_max_iter", "bg_removal.ismv.max_iter"), ("ismv_radius", "bg_removal.ismv.radius"),
    ("sharp_threshold", "bg_removal.sharp.threshold"), ("sharp_radius", "bg_removal.sharp.radius"),
    ("resharp_radius", "bg_removal.resharp.radius"), ("resharp_tik_reg", "bg_removal.resharp.tik_reg"),
    ("resharp_tol", "bg_removal.resharp.tol"), ("resharp_max_iter", "bg_removal.resharp.max_iter"),
    ("harperella_radius", "bg_removal.harperella.radius"), ("harperella_max_iter", "bg_removal.harperella.max_iter"),
    ("harperella_tol", "bg_removal.harperella.tol"), ("harperella_pad", "bg_removal.harperella.pad"), ("iharperella_max_iter", "bg_removal.iharperella.max_iter"),
    ("iharperella_tol", "bg_removal.iharperella.tol"), ("iharperella_pad", "bg_removal.iharperella.pad"),
    ("harperella_rim", "bg_removal.harperella.rim"), ("iharperella_rim", "bg_removal.iharperella.rim"),
    ("qsm_algorithm", "inversion.algorithm"),
    ("nltv_te", "inversion.nltv.te"), ("nltv_te_ms", "inversion.nltv.te"),
    ("medi_te", "inversion.medi.te"), ("medi_te_ms", "inversion.medi.te"),
    ("tfi_te", "inversion.tfi.te"), ("tfi_te_ms", "inversion.tfi.te"),
    ("ndi_te", "inversion.ndi.te"), ("ndi_te_ms", "inversion.ndi.te"),
    ("ndi_weight_norm", "inversion.ndi.weight_norm"),
    ("fansi_te", "inversion.fansi.te"), ("fansi_te_ms", "inversion.fansi.te"),
    ("l1qsm_te", "inversion.l1qsm.te"), ("l1qsm_te_ms", "inversion.l1qsm.te"),
    ("whqsm_te", "inversion.whqsm.te"), ("whqsm_te_ms", "inversion.whqsm.te"),
    ("heidi_te", "inversion.heidi.te"), ("heidi_te_ms", "inversion.heidi.te"),
    ("tgv_te", "inversion.tgv.te"), ("tgv_te_ms", "inversion.tgv.te"),
    ("rts_delta", "inversion.rts.delta"), ("rts_mu", "inversion.rts.mu"), ("rts_rho", "inversion.rts.rho"),
    ("rts_tol", "inversion.rts.tol"), ("rts_max_iter", "inversion.rts.max_iter"), ("rts_lsmr_iter", "inversion.rts.lsmr_iter"),
    ("rts_tau", "inversion.rts.tau"), ("rts_gamma", "inversion.rts.gamma"),
    ("tv_lambda", "inversion.tv.lambda"), ("tv_rho", "inversion.tv.rho"), ("tv_tol", "inversion.tv.tol"),
    ("tv_max_iter", "inversion.tv.max_iter"), ("tkd_threshold", "inversion.tkd.threshold"),
    ("tsvd_threshold", "inversion.tsvd.threshold"), ("tgv_iterations", "inversion.tgv.iterations"),
    ("rts_pad", "inversion.rts.pad"), ("tv_pad", "inversion.tv.pad"), ("tkd_pad", "inversion.tkd.pad"),
    ("tsvd_pad", "inversion.tsvd.pad"), ("tikhonov_pad", "inversion.tikhonov.pad"), ("nltv_pad", "inversion.nltv.pad"),
    ("tgv_erosions", "inversion.tgv.erosions"), ("tgv_alpha1", "inversion.tgv.alpha1"), ("tgv_alpha0", "inversion.tgv.alpha0"),
    ("tgv_step_size", "inversion.tgv.step_size"),
    ("tikhonov_lambda", "inversion.tikhonov.lambda"), ("nltv_lambda", "inversion.nltv.lambda"), ("nltv_mu", "inversion.nltv.mu"),
    ("nltv_tol", "inversion.nltv.tol"), ("nltv_max_iter", "inversion.nltv.max_iter"), ("nltv_newton_iter", "inversion.nltv.max_iter_newton"),
    ("nltv_rho", "inversion.nltv.rho"), ("nltv_tol_newton", "inversion.nltv.tol_newton"),
    ("medi_smv", "inversion.medi.smv"), ("medi_smv_radius", "inversion.medi.smv_radius"),
    ("medi_smv_shrink_mask", "inversion.medi.smv_shrink_mask"),
    ("medi_msmv", "inversion.medi.msmv"), ("medi_msmv_maxk", "inversion.medi.msmv_maxk"),
    ("medi_msmv_vessel_radius", "inversion.medi.msmv_vessel_radius"), ("medi_msmv_prefilter", "inversion.medi.msmv_prefilter"), ("medi_lambda", "inversion.medi.lambda"),
    ("medi_percentage", "inversion.medi.percentage"), ("medi_max_iter", "inversion.medi.max_iter"),
    ("medi_cg_max_iter", "inversion.medi.cg_max_iter"), ("medi_cg_tol", "inversion.medi.cg_tol"), ("medi_tol", "inversion.medi.tol"),
    ("medi_merit", "inversion.medi.merit"), ("medi_data_weighting", "inversion.medi.data_weighting"),
    ("tfi_lambda", "inversion.tfi.lambda"), ("tfi_precond", "inversion.tfi.precond"), ("tfi_percentage", "inversion.tfi.percentage"),
    ("tfi_max_iter", "inversion.tfi.max_iter"), ("tfi_cg_max_iter", "inversion.tfi.cg_max_iter"), ("tfi_cg_tol", "inversion.tfi.cg_tol"),
    ("tfi_tol", "inversion.tfi.tol"), ("tfi_data_weighting", "inversion.tfi.data_weighting"),
    ("medi_csf", "inversion.medi.csf"), ("medi_lambda_csf", "inversion.medi.lambda_csf"),
    ("medi_csf_r2s_threshold", "inversion.medi.csf_r2s_threshold"), ("medi_csf_erode", "inversion.medi.csf_erode"),
    ("medi_zeropad", "inversion.medi.zeropad"), ("tfi_merit", "inversion.tfi.merit"), ("tfi_linear", "inversion.tfi.linear"),
    ("ilsqr_tol", "inversion.ilsqr.tol"), ("ilsqr_max_iter", "inversion.ilsqr.max_iter"), ("ilsqr_pad_mm", "inversion.ilsqr.pad_mm"),
    ("star_pad", "inversion.star.pad"), ("star_te", "inversion.star.te"), ("star_te_ms", "inversion.star.te"),
    ("lsqr_residual_weighting", "inversion.lsqr.residual_weighting"), ("lsqr_fit_global_offset", "inversion.lsqr.fit_global_offset"),
    ("lsqr_tol", "inversion.lsqr.tol"), ("lsqr_max_iter", "inversion.lsqr.max_iter"),
    ("heidi_enabled", "inversion.heidi.enabled"),
    ("heidi_cone_threshold", "inversion.heidi.cone_threshold"), ("heidi_gradient_threshold", "inversion.heidi.gradient_threshold"),
    ("heidi_apply_laplacian_correction", "inversion.heidi.apply_laplacian_correction"),
    ("heidi_laplacian_threshold", "inversion.heidi.laplacian_threshold"), ("heidi_gradient_mask_floor", "inversion.heidi.gradient_mask_floor"),
    ("heidi_continuation_steps", "inversion.heidi.continuation_steps"), ("heidi_inner_iterations", "inversion.heidi.inner_iterations"),
    ("heidi_mu_min", "inversion.heidi.mu_min"), ("heidi_tol", "inversion.heidi.tol"), ("heidi_denoise", "inversion.heidi.denoise"),
    ("heidi_denoise_iterations", "inversion.heidi.denoise_iterations"), ("heidi_denoise_time_step", "inversion.heidi.denoise_time_step"),
    ("heidi_denoise_conductance", "inversion.heidi.denoise_conductance"),
    ("heidi_denoise_reference_itk", "inversion.heidi.denoise_reference_itk"),
    ("qsmart_inversion", "inversion.qsmart.inversion"), ("qsmart_ilsqr_max_iter", "inversion.qsmart.ilsqr_max_iter"),
    ("qsmart_ilsqr_pad_mm", "inversion.qsmart.ilsqr_pad_mm"), ("qsmart_sdf_sigma1_stage1", "inversion.qsmart.sdf_sigma1_stage1"),
    ("qsmart_sdf_sigma2_stage1", "inversion.qsmart.sdf_sigma2_stage1"), ("qsmart_sdf_sigma1_stage2", "inversion.qsmart.sdf_sigma1_stage2"),
    ("qsmart_sdf_sigma2_stage2", "inversion.qsmart.sdf_sigma2_stage2"),
    ("qsmart_sdf_lower_lim", "inversion.qsmart.sdf_lower_lim"), ("qsmart_sdf_curv_constant", "inversion.qsmart.sdf_curv_constant"),
    ("qsmart_vasc_sphere_radius", "inversion.qsmart.vasc_sphere_radius"), ("qsmart_frangi_scale_min", "inversion.qsmart.frangi_scale_min"),
    ("qsmart_frangi_scale_max", "inversion.qsmart.frangi_scale_max"), ("qsmart_frangi_scale_ratio", "inversion.qsmart.frangi_scale_ratio"),
    ("qsmart_frangi_c", "inversion.qsmart.frangi_c"), ("qsmart_frangi_c_mode", "inversion.qsmart.frangi_c_mode"),
    ("qsmart_frangi_c_factor", "inversion.qsmart.frangi_c_factor"),
    ("qsmart_vessel_magnitude", "inversion.qsmart.vessel_magnitude"), ("qsmart_offset_grid", "inversion.qsmart.offset_grid"), ("qsmart_vessel_units", "inversion.qsmart.vessel_size_units"),
    ("qsmart_fit_threshold_mode", "field_mapping.qsmart_fit_threshold_mode"),
    ("qsmart_fit_percentile", "field_mapping.qsmart_fit_percentile"), ("qsmart_fit_threshold", "field_mapping.qsmart_fit_threshold"),
    ("ndi_tau", "inversion.ndi.tau"), ("ndi_alpha", "inversion.ndi.alpha"), ("ndi_max_iter", "inversion.ndi.max_iter"),
    ("ndi_auto_stop", "inversion.ndi.auto_stop"),
    ("fansi_alpha1", "inversion.fansi.alpha1"), ("fansi_mu1", "inversion.fansi.mu1"), ("fansi_mu2", "inversion.fansi.mu2"),
    ("fansi_alpha0", "inversion.fansi.alpha0"), ("fansi_mu0", "inversion.fansi.mu0"), ("fansi_max_iter", "inversion.fansi.max_iter"),
    ("fansi_tol_update", "inversion.fansi.tol_update"), ("fansi_tol_delta", "inversion.fansi.tol_delta"),
    ("l1qsm_alpha1", "inversion.l1qsm.alpha1"), ("l1qsm_mu1", "inversion.l1qsm.mu1"), ("l1qsm_mu2", "inversion.l1qsm.mu2"),
    ("l1qsm_mu3", "inversion.l1qsm.mu3"), ("l1qsm_lambda", "inversion.l1qsm.lambda"), ("l1qsm_max_iter", "inversion.l1qsm.max_iter"),
    ("l1qsm_tol_update", "inversion.l1qsm.tol_update"), ("l1qsm_tol_delta", "inversion.l1qsm.tol_delta"),
    ("whqsm_alpha1", "inversion.whqsm.alpha1"), ("whqsm_mu1", "inversion.whqsm.mu1"), ("whqsm_mu2", "inversion.whqsm.mu2"),
    ("whqsm_beta", "inversion.whqsm.beta"), ("whqsm_muh", "inversion.whqsm.muh"), ("whqsm_max_iter", "inversion.whqsm.max_iter"),
    ("whqsm_tol_update", "inversion.whqsm.tol_update"), ("whqsm_tol_delta", "inversion.whqsm.tol_delta"),
    ("whqsm_weight", "inversion.whqsm.weight"), ("ilsqr_precision", "inversion.ilsqr.precision"),
    ("hdqsm_alpha_l2", "inversion.hdqsm.alpha_l2"), ("hdqsm_mu1_l2", "inversion.hdqsm.mu1_l2"), ("hdqsm_mu2", "inversion.hdqsm.mu2"),
    ("hdqsm_max_iter_l1", "inversion.hdqsm.max_iter_l1"), ("hdqsm_max_iter_l2", "inversion.hdqsm.max_iter_l2"),
    ("hdqsm_tol_update", "inversion.hdqsm.tol_update"),
    ("amp_pe_wave_order", "inversion.amp_pe.wave_order"), ("amp_pe_nlevel", "inversion.amp_pe.nlevel"),
    ("amp_pe_wave_pec", "inversion.amp_pe.wave_pec"), ("amp_pe_simulated_te", "inversion.amp_pe.simulated_te"),
    ("amp_pe_max_linearization_ite", "inversion.amp_pe.max_linearization_ite"), ("amp_pe_tikhonov_beta", "inversion.amp_pe.tikhonov_beta"),
    ("amp_pe_damp_rate_sig", "inversion.amp_pe.damp_rate_sig"), ("amp_pe_damp_rate_par", "inversion.amp_pe.damp_rate_par"),
    ("amp_pe_max_pe_spar_ite", "inversion.amp_pe.max_pe_spar_ite"), ("amp_pe_max_pe_est_ite", "inversion.amp_pe.max_pe_est_ite"),
    ("amp_pe_cvg_thd", "inversion.amp_pe.cvg_thd"),
    ("dl_tiling", "inversion.tile_size"), ("dl_tile_size", "inversion.tile_size"), ("dl_tile_halo", "inversion.tile_halo"),
    ("orientation_algorithm", "multi_orientation.algorithm"), ("orientation_lambda", "multi_orientation.lambda"),
    ("orientation_reference", "multi_orientation.reference"),
    ("cosmos_magnitude_weighting", "multi_orientation.magnitude_weighting"), ("cosmos_tol", "multi_orientation.cosmos_tol"),
    ("cosmos_max_iter", "multi_orientation.cosmos_max_iter"), ("orientation_registration", "multi_orientation.register"),
    ("multi_orientation_force", "multi_orientation.force"), ("qsm_reference", "qsm.reference"),
    ("do_chi_separation", "pipeline.do_chi_separation"), ("separation_algorithm", "separation.algorithm"),
    ("sep_r2starqsm_r_const_3t", "separation.r2star_qsm.r_const_3t"), ("sep_decompose_n_inner", "separation.decompose.n_inner"),
    ("sep_decompose_chi_bound_mode", "separation.decompose.chi_bound"), ("sep_decompose_chi_bound", "separation.decompose.chi_bound"),
    ("sep_decompose_unwrap_phase", "separation.decompose.unwrap_phase"),
    ("sep_decompose_fit", "separation.decompose.fit"),
    ("sep_decompose_max_iterations", "separation.decompose.lsqcurvefit.max_iterations"),
    ("sep_decompose_max_fevals", "separation.decompose.lsqcurvefit.max_function_evaluations"),
    ("sep_decompose_function_tol", "separation.decompose.lsqcurvefit.function_tolerance"),
    ("sep_decompose_step_tol", "separation.decompose.lsqcurvefit.step_tolerance"),
    ("sep_decompose_optimality_tol", "separation.decompose.lsqcurvefit.optimality_tolerance"),
    ("sep_decompose_fd_step", "separation.decompose.lsqcurvefit.finite_difference_step"),
    ("sep_decompose_typical_x", "separation.decompose.lsqcurvefit.typical_x"),
    ("sep_ilsqr_dr_pos", "separation.chi_sep_ilsqr.dr_pos"), ("sep_ilsqr_dr_neg", "separation.chi_sep_ilsqr.dr_neg"),
    ("sep_ilsqr_pad", "separation.chi_sep_ilsqr.pad"),
    ("sep_ilsqr_cg_max_iter_lsqr", "separation.chi_sep_ilsqr.cg_max_iter_lsqr"),
    ("sep_ilsqr_cg_tol_lsqr", "separation.chi_sep_ilsqr.cg_tol_lsqr"),
    ("sep_ilsqr_cg_max_iter_sa", "separation.chi_sep_ilsqr.cg_max_iter_sa"),
    ("sep_ilsqr_cg_tol_sa", "separation.chi_sep_ilsqr.cg_tol_sa"), ("sep_ilsqr_d2_thr", "separation.chi_sep_ilsqr.d2_thr"),
    ("sep_ilsqr_prc_w_i_min", "separation.chi_sep_ilsqr.prc_w_i_min"), ("sep_ilsqr_prc_w_i_max", "separation.chi_sep_ilsqr.prc_w_i_max"),
    ("sep_ilsqr_prc_w_g_min", "separation.chi_sep_ilsqr.prc_w_g_min"), ("sep_ilsqr_prc_w_g_max", "separation.chi_sep_ilsqr.prc_w_g_max"),
    ("sep_ilsqr_qsm_max_iter", "separation.chi_sep_ilsqr.qsm_ilsqr.max_iter"),
    ("sep_ilsqr_qsm_tol", "separation.chi_sep_ilsqr.qsm_ilsqr.tol"),
    ("sep_ilsqr_qsm_pad_mm", "separation.chi_sep_ilsqr.qsm_ilsqr.pad_mm"),
    ("sep_ilsqr_qsm_precision", "separation.chi_sep_ilsqr.qsm_ilsqr.precision"),
    ("sep_medi_lambda", "separation.chi_sep_medi.lambda"), ("sep_medi_lambda_csf", "separation.chi_sep_medi.lambda_csf"),
    ("sep_medi_dr_pos", "separation.chi_sep_medi.dr_pos"), ("sep_medi_dr_neg", "separation.chi_sep_medi.dr_neg"),
    ("sep_medi_pad", "separation.chi_sep_medi.pad"), ("sep_medi_tol", "separation.chi_sep_medi.tol"),
    ("sep_medi_max_iter", "separation.chi_sep_medi.max_iter"), ("sep_medi_cg_tol", "separation.chi_sep_medi.cg_tol"),
    ("sep_medi_cg_max_iter", "separation.chi_sep_medi.cg_max_iter"),
    ("sep_medi_qsm_max_iter", "separation.chi_sep_medi.qsm_ilsqr.max_iter"),
    ("sep_medi_qsm_tol", "separation.chi_sep_medi.qsm_ilsqr.tol"),
    ("sep_medi_qsm_pad_mm", "separation.chi_sep_medi.qsm_ilsqr.pad_mm"),
    ("sep_medi_qsm_precision", "separation.chi_sep_medi.qsm_ilsqr.precision"),
    ("sep_medi_csf_r2star", "separation.chi_sep_medi.csf_r2star"),
    ("sep_wavesep_dr_pos", "separation.wavesep.dr_pos"), ("sep_wavesep_dr_neg", "separation.wavesep.dr_neg"),
    ("sep_wavesep_alpha", "separation.wavesep.alpha"), ("sep_wavesep_lambda", "separation.wavesep.lambda"),
    ("sep_wavesep_wavelet_order", "separation.wavesep.wavelet_order"), ("sep_wavesep_max_iter", "separation.wavesep.max_iter"),
    ("sep_wavesep_tol", "separation.wavesep.tol"), ("sep_hcchisep_dr_pos_3t", "separation.hc_chisep.dr_pos_3t"),
    ("sep_hcchisep_bin_hz", "separation.hc_chisep.bin_hz"), ("sep_chisepnet_route", "separation.chi_sepnet.route"), ("sep_chisepnet_patch", "separation.chi_sepnet.patch"), ("sep_chisepnet_frame_patch", "separation.chi_sepnet.frame_patch"), ("sep_susepnet_patch", "separation.susep_net.patch"), ("custom_qsm_tool", "separation.custom_qsm_tool"),
    ("do_magnitude", "pipeline.do_magnitude"), ("do_swi", "pipeline.do_swi"), ("swi_unwrap", "swi.unwrap"), ("swi_scaling", "swi.scaling"), ("swi_strength", "swi.strength"),
    ("swi_hp_sigma", "swi.hp_sigma"), ("swi_mip_window", "swi.mip_window"),
    ("swi_filtered_phase", "swi.filtered_phase"), ("do_smwi", "pipeline.do_smwi"),
    ("smwi_threshold", "smwi.threshold_ppm"), ("smwi_power", "smwi.power"), ("smwi_mip_window", "smwi.mip_window"),
    ("do_t2starmap", "pipeline.do_t2starmap"), ("do_r2starmap", "pipeline.do_r2starmap"), ("do_r2map", "pipeline.do_r2map"),
    ("custom_r2_tool", "separation.custom_r2_tool"), ("do_r2primemap", "pipeline.do_r2primemap"),
    ("custom_r2prime_tool", "separation.custom_r2prime_tool"), ("r2prime_strategy", "separation.r2prime_strategy"),
    ("r2primenet_ignore_field_mismatch", "separation.r2primenet.ignore_field_mismatch"), ("r2primenet_patch", "separation.r2primenet.patch"),
    ("do_segmentation", "pipeline.do_segmentation"), ("segmentation_source", "segmentation.custom_dseg_tool"),
    ("custom_dseg_tool", "segmentation.custom_dseg_tool"), ("synthseg_version", "segmentation.version"),
    ("synthseg_crop", "segmentation.crop"), ("synthseg_patch", "segmentation.patch"), ("synthseg_flip_averaging", "segmentation.flip_averaging"),
    ("synthseg_topology_cleanup", "segmentation.topology_cleanup"), ("synthseg_smoothing", "segmentation.sigma_smoothing"),
    ("synthseg_sigma", "segmentation.sigma_smoothing"), ("do_analysis", "pipeline.do_analysis"), ("export_dicom", "pipeline.export_dicom"),
];

/// The config setting a TUI row edits, by the row's field name ([`FIELD_PATHS`]).
pub fn field_path(field: &str) -> Option<&'static str> {
    FIELD_PATHS.iter().find(|(f, _)| *f == field).map(|(_, p)| *p)
}

/// The form field of a config setting path: [`field_path`]'s inverse.
pub fn field_of_path(path: &str) -> Option<&'static str> {
    FIELD_PATHS.iter().find(|(_, p)| *p == path).map(|(f, _)| *f)
}

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

// ─── Algorithm help text (name + DOI) ───

// One entry per QSM_ALGO_OPTIONS entry, in the same order. Consistent format:
//   "NAME (expansion) — one-line description. https://doi.org/…"
// Deep-learning nets are marked "(deep learning; weights auto-downloaded)". Keep this aligned
// with QSM_ALGO_OPTIONS — the coverage test asserts equal lengths.
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

/// Which bring-your-own inputs (and the R2' source) the run can use: the engine's
/// [`qsmxt_engine::requirements::InputsRead`], from the form's requests.
pub type InputRelevance = qsmxt_engine::requirements::InputsRead;

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
    pub bf_algorithm: usize,


    // The inversion's settings that stay here while the form's rule for which stages run
    // (stage_config) or the inversion picker reads them; the others are in the settings store.
    /// HEIDI after the inversion (built into the LSQR+HEIDI choice).
    pub heidi_enabled: bool,
    /// MEDI's SMV mode.
    pub medi_smv: bool,
    /// Compute QSM.
    pub do_qsm: bool,

    /// The recommendations dismissed (Keep mine on a `recommended` or `discouraged` finding), each
    /// for the value its setting had then: `--dismiss-recommendation`, and so the run's config.
    pub dismissed: Vec<qsmxt_engine::Dismissal>,
    /// The toolbox pipeline chosen on the Pipeline tab: the config's `reference_pipeline`, while
    /// the form still makes its choices ([`super::pipelines_tab`]).
    pub reference_pipeline: Option<String>,
    /// The mask preset picker's choice: an index into [`MASK_PRESET_OPTIONS`], or
    /// [`MASK_PRESET_CUSTOM`] once the recipe is edited by hand. Not a setting: it names where
    /// the recipe (in the settings store, [`super::mask_builder`]) came from.
    pub mask_preset: usize,

    // Pipeline tab UI state
    pub focus: usize,
    pub editing: bool,
    pub cursor: usize,
    pub scroll_offset: usize,

    // Mask ops editor state
    /// Text buffer while a mask row's number (a threshold value, BET's fractional intensity, a
    /// refinement step's value, a signal-erode setting) is edited.
    pub mask_value_buf: String,
    /// Whether the focused mask row's number is being edited ([`Self::mask_value_buf`]).
    pub mask_value_editing: bool,
}

impl Default for PipelineFormState {
    fn default() -> Self {
        // From qsmxt-engine, not qsm-core: it owns which of qsm-core's fields are the user's.
        let heidi = crate::pipeline::config::HeidiConfig::default();
        Self {
            page: Page::Geometry,
            ctx: RowContext::default(),
            qsm_algorithm: super::command::default_qsm_algorithm_index(), // ilsqr
            bf_algorithm: 0, // vsharp
            heidi_enabled: heidi.enabled,
            medi_smv: qsm_core::inversion::MediParams::default().smv,
            do_qsm: true,
            dismissed: vec![],
            reference_pipeline: None,
            mask_preset: 0, // robust threshold
            focus: 0,
            editing: false,
            cursor: 0,
            scroll_offset: 0,
            mask_value_buf: String::new(),
            mask_value_editing: false,
        }
    }
}

pub const QSM_ALGO_OPTIONS: &[&str] = &["rts", "tv", "tkd", "tsvd", "tgv", "tikhonov", "nltv", "medi", "tfi", "ilsqr", "star", "lsqr", "qsmart", "ndi", "fansi", "fansi-tgv", "l1qsm", "whqsm", "hdqsm", "amp-pe", "xqsm", "qsmnet", "qsmnet-plus", "qsmnetme", "autoqsm", "qsmgan", "ir2qsm", "lpcnn", "modl-qsm", "nextqsm", "iqsm", "iqsm-plus"];
// QSMART's inner dipole inversion (excludes the two end-to-end algorithms tgv/qsmart).
/// The inversion picker's alias for LSQR with the HEIDI box ticked. The form never holds it: it
/// keeps `qsm_algorithm` on `lsqr`, and `heidi_enabled`.
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
/// is an index into these; `qsm_algorithm` is an index into `QSM_ALGO_OPTIONS`.
pub static QSM_PICKER_OPTIONS: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| with_lsqr_heidi(QSM_ALGO_OPTIONS, LSQR_HEIDI));

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
/// What a QSM section says while QSM is off.
const QSM_OFF_NOTE: &str = "QSM is off: turn it on in Compute QSM, the first QSM section.";

/// What each threshold method does, for the threshold-method pop-up.
fn threshold_method_help() -> Vec<String> {
    ["Otsu automatic threshold", "Fixed intensity threshold", "Percentile-based threshold",
     "STI Suite QSM_GUI's starting level: Otsu's threshold of the middle slice"].map(String::from).to_vec()
}

/// What each DICOM series type means, for the series-type pop-up.
fn series_type_help(t: dicom::SeriesType) -> &'static str {
    use dicom::SeriesType as S;
    match t {
        S::Magnitude => "Gradient-echo magnitude (part-mag)",
        S::Phase => "Gradient-echo phase (part-phase)",
        S::Real => "Real part of a complex gradient-echo image (part-real)",
        S::Imaginary => "Imaginary part of a complex gradient-echo image (part-imag)",
        S::T1w => "T1-weighted structural (T1w)",
        S::Inversion => "An MP2RAGE inversion image (inv-N_MP2RAGE)",
        S::T1map => "The scanner's T1 map (T1map)",
        S::Swi => "The scanner's susceptibility-weighted image (rec-swi_part-mag_T2starw)",
        S::MinIp => "The scanner's minimum-intensity projection (rec-mip_T2starw)",
        S::ScannerQsm => "The scanner's own susceptibility map (rec-scanner_Chimap)",
        S::T2starMap => "The scanner's T2* map (rec-scanner_T2starmap)",
        S::R2starMap => "The scanner's R2* map (rec-scanner_R2starmap)",
        S::Extra => "Kept as dcm2niix writes it, in extra_data/",
        S::Skip => "Not converted",
    }
}

/// The reference picker's options: the two methods, then every region
/// ([`super::settings_rows`]).
pub fn qsm_ref_options() -> &'static [&'static str] { super::settings_rows::options("qsm.reference").0 }

impl PipelineFormState {
    /// QSM → Compute QSM: the QSM group's switch, with what turning it off does, or why QSM is
    /// reconstructed anyway.
    fn compute_qsm_rows(&self, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Toggle {
            label: "Compute QSM", field: "do_qsm",
            help: "Enable QSM reconstruction (disable to only run the other outputs)",
        });
        if !self.do_qsm {
            match self.ctx.qsm_forced_by {
                Some(why) => rows.push(PipelineRow::Note { text: format!("  Required: {why}. Left off, the run turns it on").into() }),
                None => {
                    rows.push(PipelineRow::Note { text: "  Off: no field map, background removal, inversion or reference.".into() });
                    rows.push(PipelineRow::Note { text: "  χ-separation and SMWI need QSM, or a supplied one.".into() });
                }
            }
        }
    }

    /// Every pipeline row that shows now, page after page (the QSM switch once): for tests that
    /// ask whether a setting is reachable, not where.
    #[cfg(test)]
    pub fn all_rows(&self) -> Vec<PipelineRow> {
        let mut rows = Vec::new();
        self.compute_qsm_rows(&mut rows);
        rows
    }

    /// The rows of the pipeline form's page [`Self::page`]; empty for the App's own pages.
    pub fn visible_rows(&self) -> Vec<PipelineRow> {
        if let Some(notes) = self.page_notes() {
            return notes;
        }
        let mut rows = Vec::new();
        if self.page == Page::ComputeQsm {
            self.compute_qsm_rows(&mut rows);
        }
        rows
    }

    /// The notes a page of a stage that does not run shows instead of its settings: a QSM section
    /// says where QSM is turned on while it is off, and why when the settings skip its stage (the
    /// sidebar lists every section whatever the settings). `None` while the stage runs.
    pub fn page_notes(&self) -> Option<Vec<PipelineRow>> {
        let notes = |lines: &[&'static str]| Some(lines.iter().map(|&l| PipelineRow::Note { text: l.into() }).collect());
        match self.page {
            Page::FieldMap | Page::Background | Page::Inversion | Page::MultiOrientation | Page::Reference
                if !self.qsm_runs() => notes(&[QSM_OFF_NOTE]),
            Page::FieldMap if !self.field_mapping_runs().0 => notes(&[
                "No field mapping with these settings: the inversion takes the wrapped phase",
                "itself (iQSM, iQSM+, single-echo TGV), or iQFM makes the field from it.",
            ]),
            Page::Background if !self.inversion_needs_bg_removal() => notes(&[
                "No background removal with these settings: the inversion takes the total field",
                "or the wrapped phase itself (see Inversion).",
            ]),
            Page::MultiOrientation if !self.ctx.orientation_group => notes(&[
                "No orientation groups: runs of one head at different angles, reconstructed together.",
                "Choose them on 1 Input → Orientations, and these settings apply.",
            ]),
            _ => None,
        }
    }

    /// Get a select value by field name.
    pub fn get_select(&self, field: &str) -> usize {
        match field {
            "qsm_algorithm" => {
                let name = QSM_ALGO_OPTIONS.get(self.qsm_algorithm).copied().unwrap_or("");
                let name = if name == "lsqr" && self.heidi_enabled && !self.ctx.orientation_group { LSQR_HEIDI } else { name };
                QSM_PICKER_OPTIONS.iter().position(|o| *o == name).unwrap_or(0)
            }
            "bf_algorithm" => self.bf_algorithm,
            "mask_preset" => self.mask_preset,
            // Stored as the CLI value (0 uniform, 1 magnitude SNR).
            _ => 0,
        }
    }

    /// Set a select value by field name.
    pub fn set_select(&mut self, field: &str, val: usize) {
        match field {
            "qsm_algorithm" => {
                let name = QSM_PICKER_OPTIONS.get(val).copied().unwrap_or("");
                // LSQR+HEIDI and plain LSQR are two choices of one row.
                if name == "lsqr" {
                    self.heidi_enabled = false;
                }
                let name = if name == LSQR_HEIDI {
                    self.heidi_enabled = true;
                    "lsqr"
                } else {
                    name
                };
                self.qsm_algorithm = QSM_ALGO_OPTIONS.iter().position(|o| *o == name).unwrap_or(self.qsm_algorithm);
            }
            "bf_algorithm" => self.bf_algorithm = val,
            // The preset's recipe is the settings store's: chosen through the App
            // ([`App::choose_mask_preset`]).
            _ => {}
        }
    }

    /// Get a toggle value by field name.
    pub fn get_toggle(&self, field: &str) -> bool {
        match field {
            "do_qsm" => self.do_qsm,
            "medi_smv" => self.medi_smv,
            "heidi_enabled" => self.heidi_enabled,
            _ => false,
        }
    }

    /// Toggle a boolean by field name.
    pub fn toggle(&mut self, field: &str) {
        match field {
            "do_qsm" => self.do_qsm = !self.do_qsm,
            "medi_smv" => self.medi_smv = !self.medi_smv,
            "heidi_enabled" => self.heidi_enabled = !self.heidi_enabled,
            _ => {}
        }
    }

    /// Whether the selected inversion reads a background-removed local field (QSM.rs
    /// `InversionInput::LocalField`), and so whether background removal is part of the run.
    pub fn inversion_needs_bg_removal(&self) -> bool {
        self.stage_config().is_none_or(|c| !qsmxt_engine::bridge::skips_bgremove(&c))
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
        self.field_mapping_runs_for(self.ctx.num_echoes)
    }

    /// [`Self::field_mapping_runs`], for a run limited to `num_echoes` echoes (`None`: unknown).
    pub fn field_mapping_runs_for(&self, num_echoes: Option<usize>) -> (bool, bool) {
        let Some(config) = self.stage_config() else { return (true, true) };
        let counts = match num_echoes {
            Some(n) => vec![n],
            None => vec![1, 2],
        };
        let runs: Vec<bool> = counts.iter()
            .map(|&n| qsmxt_engine::bridge::field_mapping_runs(&config, n)).collect();
        (runs.iter().any(|&r| r), runs.iter().all(|&r| r))
    }


    /// Whether QSM is reconstructed: Compute QSM is on, or another output forces it.
    pub fn qsm_runs(&self) -> bool {
        self.do_qsm || self.ctx.qsm_forced_by.is_some()
    }

}

/// Indices of the rows focus can land on: every row but separators, notes and mask headers.
pub fn focusable_indices(rows: &[PipelineRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| !matches!(r, PipelineRow::Separator | PipelineRow::Note { .. }
            | PipelineRow::MaskSectionHeader { .. } | PipelineRow::MaskCombinedHeader
            | PipelineRow::MaskTwoPassHeader | PipelineRow::Summary { .. }))
        .map(|(i, _)| i)
        .collect()
}

/// Notes wrapped to `width` columns (0 leaves them as written). A note continued on the next
/// row (one that does not end a sentence or a heading) is joined to it first, so a note written
/// over two rows reflows as one; each wrapped row keeps the note's indent.
pub fn wrap_notes(rows: Vec<PipelineRow>, width: usize) -> Vec<PipelineRow> {
    if width == 0 {
        return rows;
    }
    let mut joined: Vec<PipelineRow> = Vec::with_capacity(rows.len());
    for row in rows {
        if let (PipelineRow::Note { text }, Some(PipelineRow::Note { text: prev })) = (&row, joined.last_mut()) {
            if !prev.trim_end().ends_with(['.', ':', ')']) {
                *prev = format!("{} {}", prev.trim_end(), text.trim_start()).into();
                continue;
            }
        }
        joined.push(row);
    }
    let mut out = Vec::with_capacity(joined.len());
    for row in joined {
        let PipelineRow::Note { text } = &row else {
            out.push(row);
            continue;
        };
        let indent = text.len() - text.trim_start().len();
        let pad = " ".repeat(indent);
        let mut line = String::new();
        for word in text.split_whitespace() {
            if !line.is_empty() && pad.len() + line.chars().count() + 1 + word.chars().count() > width {
                out.push(PipelineRow::Note { text: format!("{pad}{line}").into() });
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(PipelineRow::Note { text: format!("{pad}{line}").into() });
    }
    out
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
    crate::pipeline::requirements::Output::of_setting(&format!("pipeline.{field}"))
}

const EXECUTION_MODE_OPTIONS: &[&str] = &["Local", "SLURM"];
const EXECUTION_MODE_HELP: &[&str] = &["Local execution or generate SLURM job scripts"; 2];
pub struct App {
    pub active_tab: usize,
    /// The sub-tab last chosen on each tab (see [`App::page`]).
    pub sub_page: [Option<Page>; TAB_NAMES.len()],
    /// Focus is on the sub-tab strip rather than the rows (see [`App::on_subtabs`]).
    pub subtab_focus: bool,
    /// Whether the last frame drew the Reconstruction tab's sections as a sidebar (wide enough)
    /// rather than as a strip of pills; the keys follow what is on screen.
    pub sidebar_layout: std::cell::Cell<bool>,
    /// The width the last frame gave a section's settings, which its notes wrap to; 0 (before
    /// any frame) leaves them as written.
    pub note_width: std::cell::Cell<u16>,
    /// Focus is on the main tab bar: ←/→ switch tabs, ↓ or Enter go into the tab.
    pub tab_focus: bool,
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
    /// The Pipeline tab: its scroll, the scan it plans on, where its SVG goes.
    pub pipeline_view: super::pipeline_view::PipelineView,
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
    /// A newer release the check found, waiting for its turn among the pop-ups
    /// ([`App::show_next_pop_up`]).
    pub pending_update: Option<UpdateModal>,
    /// An announcement pop-up, or the history of them all (`n` on the tab bar).
    pub news_modal: Option<NewsModal>,
    /// Background fetch of the announcements started at launch.
    pub news_state: NewsState,
    /// What that fetch came back with, once it has.
    pub news: Option<crate::commands::news::Loaded>,
    /// Announcements still to pop up, in order (critical first, then newest first).
    pub news_queue: Vec<crate::commands::news::Announcement>,
    /// The "Did you know?" pop-up: shown at launch (unless tips are off) and on `?`.
    pub tip_modal: Option<TipModal>,
    /// The recommendation pop-up, opened when a change creates a recommendation
    /// ([`super::recommend`]).
    pub rec_modal: Option<super::recommend::RecModal>,
    /// What the engine works the pop-ups after a change out from: the findings open after the
    /// last change, the ones waiting for a pop-up, the methods chosen ([`qsmxt_engine::advice::edit`]).
    pub advice: qsmxt_engine::advice::edit::State,
    /// The form at the last check (empty before the first): a key that changed nothing is not
    /// checked.
    pub rec_form: String,
    /// The form as it was at the same point: what "Undo my change" in a required-change pop-up
    /// sets back ([`super::recommend::Undo`]).
    pub rec_snapshot: Option<Box<super::recommend::FormSnapshot>>,
    /// The reference pipeline pop-up opened when a method is chosen, and what it is worked out
    /// from ([`super::recommend::RefState`]).
    pub refs: super::recommend::RefState,
    /// The Pipeline tab ([`super::pipelines_tab`]).
    pub pipelines: super::pipelines_tab::PipelinesState,
    /// Where the clickable things of the last frame are ([`super::mouse`]).
    pub hits: super::mouse::Hits,
    /// Advanced mode: every setting shows. In Basic mode a Reconstruction section shows the
    /// basic settings and any that are changed or flagged ([`qsmxt_engine::schema::regular_view`]).
    /// Advanced until the launch reads the user's saved choice ([`super::prefs`]), which is Basic
    /// unless they chose Advanced.
    pub advanced: bool,
    /// The settings Basic mode shows, for the config they were worked out from.
    basic_cache: std::cell::RefCell<Option<(String, BasicPaths)>>,
    /// Which settings apply, for the config and facts they were worked out for
    /// ([`super::settings_rows`]).
    pub(super) visible_cache: super::settings_rows::VisibleCache,
    /// The config the form builds ([`super::command::config_from_app`]), or why it builds none,
    /// as of the last [`App::prepare_frame`]: what the frame shows. Read through
    /// [`App::current_config`], which uses it only while the form cannot have changed since.
    pub config: Option<Result<crate::pipeline::config::PipelineConfig, String>>,
    /// Set while a frame is prepared and drawn, when [`Self::config`] is the form's: nothing
    /// changes the form then. At any other time the form may have changed since, so the config
    /// is built afresh.
    pub in_frame: bool,
}

/// The "Did you know?" pop-up, with two buttons: OK, and one that turns startup tips off
/// (or back on, when it was opened with `?` while they are off).
#[derive(Debug, Clone)]
pub struct TipModal {
    pub tip: super::tips::Tip,
    /// Whether startup tips were on when the pop-up opened; picks the second button.
    pub tips_enabled: bool,
    /// The selected button: 0 is OK, 1 turns startup tips off (or on).
    pub cursor: usize,
}

impl TipModal {
    pub fn new(tip: super::tips::Tip, tips_enabled: bool) -> Self {
        TipModal { tip, tips_enabled, cursor: 0 }
    }

    /// The labels of the two buttons, OK first.
    pub fn buttons(&self) -> [&'static str; 2] {
        ["OK", if self.tips_enabled { "Don't show tips" } else { "Show tips at startup" }]
    }
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
}

/// An announcement pop-up ([`crate::commands::news`]), or the history of all of them.
#[derive(Debug, Clone)]
pub struct NewsModal {
    /// The announcement popping up, or every announcement (newest first) in the history.
    pub items: Vec<crate::commands::news::Announcement>,
    /// The history (opened with `n`), rather than a pop-up.
    pub history: bool,
    /// A line above the history: that the announcements could not be read and these are the
    /// saved copy, or that they are still being fetched.
    pub note: Option<String>,
    pub scroll: u16,
}

/// Holds the receiver for a launch-time background job (the release check, the announcements).
///
/// The job runs on its own thread and the result is picked up by [`Self::poll`] on a
/// later frame, so nothing about startup waits on the network. When the job is disabled
/// (opt-out, CI, or for the release check a dev build) the receiver is `None` and `poll`
/// never yields anything. Nothing joins the thread, so it never delays exit either.
#[derive(Debug)]
pub struct Background<T> {
    receiver: Option<std::sync::mpsc::Receiver<T>>,
    /// Set once the result has been taken (or there will be none).
    done: bool,
}

/// The launch-time release check: `Some` newer release, or `None`.
pub type UpdateState = Background<Option<crate::commands::update::check::UpdateInfo>>;

/// The launch-time announcements fetch.
pub type NewsState = Background<crate::commands::news::Loaded>;

impl UpdateState {
    pub fn start() -> Self {
        Self { receiver: crate::commands::update::check::spawn(), done: false }
    }
}

impl NewsState {
    /// Tests never fetch: a dev build would, and the tests must stay off the network.
    pub fn start() -> Self {
        #[cfg(not(test))]
        return Self { receiver: crate::commands::news::spawn(), done: false };
        #[cfg(test)]
        Self::disabled()
    }
}

impl<T> Background<T> {
    /// A job that never produces a result.
    pub fn disabled() -> Self {
        Self { receiver: None, done: true }
    }

    /// A job that has already produced `value`, for tests.
    #[cfg(test)]
    pub fn ready(value: T) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(value).unwrap();
        Self { receiver: Some(rx), done: false }
    }

    /// Has the result been taken, or is there none to come?
    pub fn finished(&self) -> bool {
        self.done
    }

    /// Non-blocking: returns the result exactly once, the first frame after the
    /// background job finishes. A failed or still-running job yields `None`.
    pub fn poll(&mut self) -> Option<T> {
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
                Some(found)
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
    /// Modal title (the row label, e.g. "QSM inversion").
    pub title: String,
    pub options: Vec<String>,
    /// Per-option help text (same length as `options`).
    pub help: Vec<String>,
    /// Currently highlighted option index.
    pub cursor: usize,
    /// The first option shown: the list scrolls only when the cursor would leave the window.
    pub offset: std::cell::Cell<usize>,
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
    /// How the sections of the mask a section belongs to combine (OR / AND).
    MaskCombine(usize),
    /// HD-BET's inference (whole volume / sliding window / low memory) of a section's generator.
    MaskHdBetInference(usize),
    /// HD-BET's patch step of a section's generator ([`PipelineFormState::hd_bet_step_choices`]).
    MaskHdBetStep(usize),
    /// BET's voxel scaling of a section's generator ([`PipelineFormState::bet_voxel_scale_choices`]).
    MaskBetVoxelScale(usize),
    /// The method of a refinement step that is a threshold.
    MaskRefinementThresholdMethod { section: usize, index: usize },
    /// The type of a refinement step to add to a section ([`MaskRecipe::available_op_types`]).
    MaskAddStep(usize),
    /// The type of the focused DICOM series (Input tab, DICOM mode), in [`dicom::SeriesType::ALL`]'s order.
    DicomSeriesType,
}

#[derive(Clone, Debug)]
pub struct RunForm {
    // Tab 0: Input/Output
    pub bids_dir: String,
    pub output_dir: String,
    pub config_file: String,

    /// The settings of the sections whose rows come from the registry, in a config
    /// ([`super::settings_rows`]).
    pub settings: super::settings_rows::SettingsStore,

    // Other outputs
    pub export_dicom: bool,
    /// DICOM export: source DICOM directory to inherit identity from, and the maps unticked on
    /// the Export section (by series suffix, lower case). The unticked ones are kept rather than
    /// the ticked, so a map turned on later is exported unless it is unticked too.
    pub source_dicom: String,
    pub dicom_unticked: Vec<String>,

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
        source_dicom, n_procs, mem_limit_gb, slurm_account,
        slurm_partition, slurm_time, slurm_mem, slurm_cpus,
    ]);
    named_fields!(flag, flag_mut, bool, [
        export_dicom,
        dry_run, debug, force, clean_intermediates, slurm_submit,
    ]);
    named_fields!(choice, choice_mut, usize, [execution_mode]);

    /// The memory limit row, read: `(limit in GB, no limit)` — empty or `auto` detects it.
    pub fn mem_limit(&self) -> (Option<f64>, bool) {
        let v = self.mem_limit_gb.trim();
        if v.eq_ignore_ascii_case("none") { return (None, true); }
        (v.parse().ok().filter(|g: &f64| *g > 0.0), false)
    }

}

impl Default for RunForm {
    fn default() -> Self {
        Self {
            bids_dir: String::new(),
            output_dir: String::new(),
            config_file: String::new(),
            settings: Default::default(),
            export_dicom: false,
            source_dicom: String::new(),
            dicom_unticked: Vec::new(),
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
        Self::with_background(UpdateState::start(), NewsState::start())
    }

    /// A form with no release check or announcements, for trying a value on
    /// ([`super::recommend`]).
    pub(super) fn new_for_trial() -> Self {
        Self::with_background(UpdateState::disabled(), NewsState::disabled())
    }

    fn with_background(update_state: UpdateState, news_state: NewsState) -> Self {
        App {
            active_tab: 0,
            sub_page: [None; TAB_NAMES.len()],
            subtab_focus: false,
            sidebar_layout: std::cell::Cell::new(true),
            note_width: std::cell::Cell::new(0),
            tab_focus: false,
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
            pipeline_view: Default::default(),
            error_message: None,
            status_message: None,
            clipboard: None,
            algo_modal: None,
            update_modal: None,
            update_state,
            pending_update: None,
            news_modal: None,
            news_state,
            news: None,
            news_queue: Vec::new(),
            tip_modal: None,
            input_mode: InputMode::Bids,
            example_state: ExampleState::default(),
            dicom_state: DicomConvertState::default(),
            nifti_state: NiftiState::default(),
            rec_modal: None,
            advice: Default::default(),
            rec_form: String::new(),
            rec_snapshot: None,
            refs: Default::default(),
            pipelines: Default::default(),
            hits: Default::default(),
            advanced: true,
            basic_cache: Default::default(),
            visible_cache: Default::default(),
            config: None,
            in_frame: false,
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
            self.error_message = Some("BIDS directory is required".to_string());
            self.active_tab = 0;
            self.active_field = 0;
            return;
        }

        // SLURM mode requires account
        if self.form.execution_mode == 1 && self.form.slurm_account.trim().is_empty() {
            self.error_message = Some("SLURM account is required".to_string());
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
                self.status_message = Some(METHODS_COPIED.to_string());
            }
            Err(e) => self.error_message = Some(format!("Nothing copied: {e}")),
        }
    }

    /// Whether the methods text was just copied (the button says so until the next key).
    pub fn methods_copied(&self) -> bool {
        self.status_message.as_deref() == Some(METHODS_COPIED)
    }

    /// The Methods tab's scroll position on the page shown (the text or the graph).
    pub(super) fn methods_page_scroll(&mut self) -> &mut usize {
        if self.page() == Some(Page::Pipeline) { &mut self.pipeline_view.scroll } else { &mut self.methods_scroll_offset }
    }

    /// The Methods tab's button: copy the methods text, or open the graph.
    pub(super) fn press_page_button(&mut self) {
        if self.page() == Some(Page::Pipeline) { self.open_pipeline_svg() } else { self.copy_methods() }
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

    /// The Methods tab: its sub-tab strip, then the methods text (scrolling, copying) or the
    /// pipeline graph (scrolling, the graph as SVG). Both are read-only.
    fn handle_methods_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return self.should_quit = true,
            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => return,
            KeyCode::Char('[') => return self.cycle_page(-1),
            KeyCode::Char(']') => return self.cycle_page(1),
            KeyCode::F(5) => return self.try_run(),
            // The page's button works from the sidebar too: `c` copies the text, `s` opens the
            // graph (Enter as well, inside the page).
            KeyCode::Char('c') | KeyCode::Char('y') if self.page() != Some(Page::Pipeline) => return self.copy_methods(),
            KeyCode::Char('s') if self.page() == Some(Page::Pipeline) => return self.open_pipeline_svg(),
            _ => {}
        }
        // The sidebar, as the Reconstruction tab's: ↑/↓ choose the page, → or Enter go into it,
        // ↑ from the first goes up to the tab bar.
        if self.on_subtabs() && self.sidebar() {
            let first = self.page() != Some(Page::Pipeline);
            match key.code {
                KeyCode::Up | KeyCode::Char('k') if first => self.tab_focus = true,
                KeyCode::Up | KeyCode::Char('k') => self.cycle_page(-1),
                KeyCode::Down | KeyCode::Char('j') if first => self.cycle_page(1),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.subtab_focus = false,
                _ => {}
            }
            return;
        }
        if self.on_subtabs() {
            match key.code {
                KeyCode::Left => self.cycle_page(-1),
                KeyCode::Right => self.cycle_page(1),
                KeyCode::Up | KeyCode::Char('k') => self.tab_focus = true,
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => self.subtab_focus = false,
                _ => {}
            }
            return;
        }
        let pipeline = self.page() == Some(Page::Pipeline);
        let sidebar = self.sidebar();
        let scroll = if pipeline { &mut self.pipeline_view.scroll } else { &mut self.methods_scroll_offset };
        match key.code {
            // ← goes back to the sidebar, and so does ↑ from the top.
            KeyCode::Left | KeyCode::Char('h') if sidebar => self.subtab_focus = true,
            KeyCode::Up | KeyCode::Char('k') if *scroll == 0 => self.subtab_focus = true,
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *scroll += 1,
            KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
            KeyCode::PageDown => *scroll += 10,
            KeyCode::Home => *scroll = 0,
            KeyCode::Enter => self.press_page_button(),
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
                KeyCode::Up | KeyCode::Char('k') => self.tab_focus = true,
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
        let on_button = self.example_state.cursor == ExampleState::BUTTON;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}
            KeyCode::Up | KeyCode::Char('k') => {
                if on_button {
                    // Back up into the IO fields above the picker.
                    self.active_field = Self::INPUT_IO_FIELDS - 1;
                } else {
                    self.example_state.cursor -= 1;
                }
            }
            // Once a download has started the list is hidden: the button is all there is.
            KeyCode::Down | KeyCode::Char('j') if self.example_state.cursor < ExampleState::last_row()
                && self.example_state.status == ConvertStatus::Idle => {
                self.example_state.cursor += 1;
            }
            KeyCode::Char(' ') if !on_button => {
                self.example_state.toggle_selected();
            }
            KeyCode::Enter => {
                if !on_button {
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
                title: "Input mode".to_string(),
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
                offset: Default::default(),
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

            // A run: its orientation group. A subject or session: collapse/expand.
            KeyCode::Left | KeyCode::Right if self.filter_state.focused_run().is_some() => {
                let reference = self.field_text("orientation_reference").into_owned();
                self.filter_state.move_orientation(key.code == KeyCode::Right, &reference);
                // Each group's reference is now its first run.
                self.form.settings.reset("multi_orientation.reference", false);
            }
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

            // Interact: a series opens the list of types to give it (and every subject
            // instance that shares its identity).
            KeyCode::Enter | KeyCode::Char(' ') => {
                match self.dicom_state.focus {
                    DicomFocus::Series(_) => self.open_series_type_modal(),
                    DicomFocus::KeepUids => self.dicom_state.keep_uids = !self.dicom_state.keep_uids,
                    DicomFocus::ConvertButton => {
                        self.run_dicom_conversion();
                    }
                }
            }

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    /// The focused unique series' type: a pop-up listing every type, the current one highlighted.
    fn open_series_type_modal(&mut self) {
        let DicomFocus::Series(i) = self.dicom_state.focus else { return };
        let Some(session) = self.dicom_state.session.as_ref() else { return };
        let Some(g) = session.unique_series().into_iter().nth(i) else { return };
        let cur = session.series_ref(&g.refs[0]).series_type;
        let all = dicom::SeriesType::ALL;
        self.algo_modal = Some(AlgoModal {
            target: AlgoModalTarget::DicomSeriesType,
            title: "Series type".to_string(),
            options: all.iter().map(|t| t.label().to_string()).collect(),
            help: all.iter().map(|&t| series_type_help(t).to_string()).collect(),
            cursor: all.iter().position(|&t| t == cur).unwrap_or(0),
            offset: Default::default(),
        });
    }

    /// Give the focused unique series a type, and every subject instance sharing its identity.
    fn set_focused_unique_series_type(&mut self, new_type: dicom::SeriesType) {
        let DicomFocus::Series(i) = self.dicom_state.focus else { return };
        let Some(session) = self.dicom_state.session.as_mut() else { return };
        let groups = session.unique_series();
        if let Some(g) = groups.get(i) {
            session.set_type_for_refs(&g.refs, new_type);
        }
    }

    /// Launch the DICOM → BIDS conversion in a background thread.
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
        let options = dicom::convert::ConvertOptions { keep_dicom_uids: self.dicom_state.keep_uids };
        let (tx, rx) = mpsc::channel();
        self.dicom_state.convert_receiver = Some(rx);

        std::thread::spawn(move || {
            dicom::convert::convert_session_streaming(&session, &output_path, options, &tx);
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
    /// Pick up the launch-time release check and announcements, and open the next pop-up
    /// that is due ([`Self::show_next_pop_up`]). Called once per frame from the main loop.
    pub fn poll_launch_checks(&mut self) {
        if let Some(Some(info)) = self.update_state.poll() {
            self.pending_update = Some(UpdateModal { tag: info.tag, html_url: info.html_url });
        }
        if let Some(loaded) = self.news_state.poll() {
            self.receive_news(loaded);
        }
        self.show_next_pop_up();
    }

    /// Take the announcements the background fetch came back with: queue those due to pop up.
    pub fn receive_news(&mut self, loaded: crate::commands::news::Loaded) {
        use crate::commands::{news, update};
        self.news_queue = news::pop_ups(&loaded.items, update::current_version(), &news::load_seen());
        self.news = Some(loaded);
    }

    /// Open the next pop-up that is due, when its turn comes. One pop-up at a time, in this
    /// order: critical announcements, then the update prompt, then ordinary announcements. The
    /// update prompt waits until the announcements are in (so a critical one is not shown after
    /// it), and ordinary announcements until the release check is done. Announcements never
    /// open over another pop-up (a tip, an algorithm list, advice); the update prompt does, as
    /// it always has.
    pub fn show_next_pop_up(&mut self) {
        if self.update_modal.is_some() || self.news_modal.is_some() {
            return;
        }
        let other_open = self.tip_modal.is_some()
            || self.algo_modal.is_some()
            || self.rec_modal.is_some()
            || self.refs.modal.is_some();
        if self.news_queue.first().is_some_and(|a| a.critical) {
            if !other_open {
                let next = self.news_queue.remove(0);
                self.open_news(next);
            }
            return;
        }
        if self.pending_update.is_some() {
            if self.news_state.finished() {
                self.update_modal = self.pending_update.take();
            }
            return;
        }
        if !other_open && self.update_state.finished() && !self.news_queue.is_empty() {
            let next = self.news_queue.remove(0);
            self.open_news(next);
        }
    }

    fn open_news(&mut self, item: crate::commands::news::Announcement) {
        self.news_modal = Some(NewsModal { items: vec![item], history: false, note: None, scroll: 0 });
    }

    /// Open the history of announcements, newest first: what the launch fetch got, or the copy
    /// saved last time while it is still running.
    pub fn open_news_history(&mut self) {
        let (items, note) = match &self.news {
            Some(loaded) => (
                loaded.items.clone(),
                loaded.error.as_ref().map(|_| {
                    "The announcements could not be read, so these are the ones saved the last time they could.".to_string()
                }),
            ),
            None if self.news_state.finished() => (crate::commands::news::load_cached(), None),
            None => (
                crate::commands::news::load_cached(),
                Some("Checking for new announcements; these are the ones saved last time.".to_string()),
            ),
        };
        self.news_modal = Some(NewsModal { items, history: true, note, scroll: 0 });
    }

    /// Keys while an announcement pop-up or the history is open: ↑↓/PgUp/PgDn/Home scroll, `c`
    /// copies a pop-up's link, Enter/Esc/q close it. Closing a pop-up records it as shown.
    pub fn handle_news_modal_key(&mut self, key: KeyEvent) {
        let Some(m) = self.news_modal.as_mut() else { return };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => m.scroll = m.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => m.scroll = m.scroll.saturating_add(1),
            KeyCode::PageUp => m.scroll = m.scroll.saturating_sub(10),
            KeyCode::PageDown => m.scroll = m.scroll.saturating_add(10),
            KeyCode::Home => m.scroll = 0,
            KeyCode::Char('c') | KeyCode::Char('y') if !m.history => {
                if let Some(link) = m.items.first().and_then(|a| a.link.clone()) {
                    self.clipboard = Some(link);
                    self.status_message = Some("Link copied to clipboard".to_string());
                }
            }
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                let m = self.news_modal.take().expect("checked above");
                if !m.history {
                    for a in &m.items {
                        crate::commands::news::mark_seen(&a.id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle a key while the update prompt is open. Returns what the user chose.
    pub fn handle_update_modal_key(&mut self, key: KeyEvent) -> UpdateChoice {
        match key.code {
            KeyCode::Enter | KeyCode::Char('u') | KeyCode::Char('U') => {
                self.update_modal = None;
                self.should_update = true;
                UpdateChoice::Update
            }
            KeyCode::Esc | KeyCode::Char('l') | KeyCode::Char('L') => {
                self.update_modal = None;
                UpdateChoice::Later
            }
            _ => UpdateChoice::Ignored,
        }
    }

    /// Is a text field taking keystrokes? Then `?` is typed rather than opening a tip.
    pub(super) fn is_typing(&self) -> bool {
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

    /// Open the tip pop-up on the next tip, whether or not startup tips are on.
    pub fn open_tip(&mut self) {
        self.status_message = None;
        self.error_message = None;
        self.tip_modal = Some(TipModal::new(super::tips::next_tip(), super::tips::enabled()));
    }

    /// Keys while the tip pop-up is open: ←/→/Tab choose a button, Enter/Space press it,
    /// `n` shows the next tip, Esc/q close it.
    pub fn handle_tip_modal_key(&mut self, key: KeyEvent) {
        let Some(m) = self.tip_modal.as_mut() else { return };
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => m.cursor = 0,
            KeyCode::Right | KeyCode::Char('l') => m.cursor = 1,
            KeyCode::Tab | KeyCode::BackTab => m.cursor = 1 - m.cursor,
            KeyCode::Char('n') => m.tip = super::tips::next_tip(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let m = self.tip_modal.take().expect("checked above");
                if m.cursor == 1 {
                    super::tips::set_enabled(!m.tips_enabled);
                    self.status_message = Some(if m.tips_enabled {
                        "Tips turned off. Press ? to see a tip or to turn them back on".to_string()
                    } else {
                        "Tips will show when QSMxT opens".to_string()
                    });
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.tip_modal = None,
            _ => {}
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
                self.set_mask_input(section, m.cursor);
            }
            AlgoModalTarget::MaskGenerator(section) => {
                self.set_mask_generator(section, m.cursor);
            }
            AlgoModalTarget::MaskThresholdMethod(section) => {
                self.set_mask_threshold_method(section, m.cursor);
            }
            AlgoModalTarget::InputMode => self.set_input_mode(m.cursor),
            AlgoModalTarget::MaskCombine(section) => {
                use crate::pipeline::config::MaskCombine;
                let mode = if m.cursor == 0 { MaskCombine::Or } else { MaskCombine::And };
                self.set_combine(section, mode);
            }
            AlgoModalTarget::MaskHdBetInference(section) => self.set_hd_bet_inference(section, m.cursor),
            AlgoModalTarget::MaskHdBetStep(section) => {
                let (values, _) = self.mask().hd_bet_step_choices(section);
                if let Some(&v) = values.get(m.cursor) { self.set_hd_bet_tile_step(section, v); }
            }
            AlgoModalTarget::MaskBetVoxelScale(section) => {
                let (values, _) = self.mask().bet_voxel_scale_choices(section);
                if let Some(&v) = values.get(m.cursor) { self.set_bet_voxel_scale(section, v); }
            }
            AlgoModalTarget::MaskRefinementThresholdMethod { section, index } => {
                self.set_refinement_threshold_method(section, index, m.cursor);
            }
            AlgoModalTarget::MaskAddStep(section) => {
                if let Some(kind) = MaskRecipe::available_op_types().get(m.cursor) {
                    self.add_mask_step(section, kind);
                }
            }
            AlgoModalTarget::DicomSeriesType => {
                if let Some(&t) = dicom::SeriesType::ALL.get(m.cursor) { self.set_focused_unique_series_type(t); }
            }
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

    /// Bring the state a frame shows up to date, before drawing it: the header's pipeline status,
    /// and on a row-based tab the pipeline form's page and context ([`Self::sync_pipeline_mode`];
    /// a page that stopped applying sends focus back to the top). Drawing ([`super::ui::draw`])
    /// then only reads the form, so the next key and the config see the state this left, whether
    /// or not a frame was drawn in between.
    pub fn prepare_frame(&mut self) {
        // Built once for the frame: the header, the command, the methods, the rows' context and
        // Basic mode's choice of rows all read it, many times a frame.
        self.config = Some(super::command::config_from_app(self).map_err(|e| e.to_string()));
        self.in_frame = true;
        // In the order drawing used to do these, so each frame shows what it did.
        self.pipeline_status();
        if Self::is_rows_tab(self.active_tab) {
            self.sync_pipeline_mode();
        }
        self.in_frame = false;
    }

    /// The config the form builds, or why it builds none: the frame's ([`Self::config`]) while a
    /// frame is prepared or drawn, else built now, as a key handler that has just changed the
    /// form needs it.
    pub fn current_config(&self) -> std::borrow::Cow<'_, Result<crate::pipeline::config::PipelineConfig, String>> {
        match &self.config {
            Some(c) if self.in_frame => std::borrow::Cow::Borrowed(c),
            _ => std::borrow::Cow::Owned(super::command::config_from_app(self).map_err(|e| e.to_string())),
        }
    }

    /// Point the pipeline form at the active page, with its context fresh. When the page
    /// changes under it (a tab or sub-tab switch, or a sub-tab that stopped applying), focus goes
    /// back to the top. Called by key handling and before each frame ([`Self::prepare_frame`]) so
    /// the display is consistent.
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
        let config = self.current_config();
        self.row_context_with(config.as_ref().as_ref().ok())
    }

    /// [`Self::row_context`], for the config the form builds now.
    fn row_context_with(&self, config: Option<&crate::pipeline::config::PipelineConfig>) -> RowContext {
        let ps = &self.pipeline_state;
        let group = self.filter_state.orientation_group_by();
        let algorithm = self.field_select("orientation_algorithm");
        // Whether QSM runs is the resolved config's answer — the same enforcement the run applies.
        // The config as the form gives it, and as the run takes it: QSM another output needs is
        // a required change the run makes itself when it is left off.
        let qsm_forced = config.as_ref().filter(|_| !ps.do_qsm)
            .and_then(|c| c.normalised().1.into_iter().find(|a| a.setting == "pipeline.do_qsm"));
        RowContext {
            qsm_forced_by: qsm_forced.map(|a| match a.because {
                "pipeline.do_smwi" => "SMWI weights the magnitude by the χ map (QSM → SMWI)",
                _ => "the per-structure statistics need a map to summarise (Other outputs → Segmentation)",
            }),
            orientation_group: group.is_some(),
            orientation_cosmos: algorithm == 0,
            num_echoes: self.filter_state.num_echoes.trim().parse().ok(),
            inputs: self.input_relevance(),
            axial_forced_by: config.as_ref().and_then(|c| {
                let names = qsmxt_engine::bridge::axial_only_algorithms(
                    &c.normalised().0, qsmxt_engine::bridge::RunOrientationFacts { has_measured_r2: false });
                (!names.is_empty()).then(|| names.join(", "))
            }),
            multi_coil: self.multi_coil(),
        }
    }

    /// Whether a selected run is uncombined multi-coil data (true until runs are listed).
    pub fn multi_coil(&self) -> bool {
        self.filter_state.tree.as_ref().is_none_or(|t| {
            let mut any = false;
            t.for_each_run(|leaf| any |= leaf.selected && leaf.multi_coil);
            any
        })
    }

    /// Which bring-your-own inputs the run can use, from what the form asks for (see
    /// [`InputRelevance`]). Decides both whether their rows show and whether they are passed on.
    pub fn input_relevance(&self) -> InputRelevance {
        let f = &self.form;
        let mut c = qsmxt_engine::PipelineConfig::default();
        let st = &f.settings;
        (c.pipeline.do_chi_separation, c.pipeline.do_analysis, c.pipeline.do_smwi) =
            (st.toggle("pipeline.do_chi_separation"), st.toggle("pipeline.do_analysis"), st.toggle("pipeline.do_smwi"));
        (c.pipeline.do_r2map, c.pipeline.do_r2primemap) = (st.toggle("pipeline.do_r2map"), st.toggle("pipeline.do_r2primemap"));
        if let Ok(sep) = serde_json::from_value(st.get("separation.algorithm")) {
            c.separation.algorithm = sep;
        }
        c.separation.custom_r2prime_tool = Some(st.text("separation.custom_r2prime_tool").trim().to_string()).filter(|t| !t.is_empty());
        // R2' is measured as R2* − R2 unless it is always predicted (R2PRIMEnet).
        if st.get("separation.r2prime_strategy") == "r2primenet" {
            c.separation.r2prime_strategy = qsmxt_engine::R2PrimeStrategy::R2primenet;
        }
        InputRelevance::of(&c)
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
        // Editing a mask row's number (a threshold value, BET's fractional intensity, a
        // refinement step's value, a signal-erode setting): Enter keeps it when it is valid,
        // Esc leaves the setting as it was.
        if ps.mask_value_editing {
            let mut cursor = ps.cursor;
            match key.code {
                KeyCode::Esc => {
                    ps.mask_value_editing = false;
                    return;
                }
                KeyCode::Enter => {
                    let text = ps.mask_value_buf.clone();
                    let result = match self.focused_row() {
                        Some(row) => self.set_mask_value(&row, &text),
                        None => Ok(()),
                    };
                    match result {
                        Ok(()) => self.pipeline_state.mask_value_editing = false,
                        // Still editing, to correct it.
                        Err(e) => self.error_message = Some(e),
                    }
                    return;
                }
                KeyCode::Char(c) if c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E') => {
                    ps.mask_value_buf.insert(cursor, c);
                    cursor += 1;
                }
                KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                    let new_pos = word_boundary_left(&ps.mask_value_buf, cursor);
                    ps.mask_value_buf.drain(new_pos..cursor);
                    cursor = new_pos;
                }
                KeyCode::Backspace if cursor > 0 => {
                    cursor -= 1;
                    ps.mask_value_buf.remove(cursor);
                }
                KeyCode::Delete if cursor < ps.mask_value_buf.len() => {
                    ps.mask_value_buf.remove(cursor);
                }
                KeyCode::Left => cursor = cursor.saturating_sub(1),
                KeyCode::Right if cursor < ps.mask_value_buf.len() => cursor += 1,
                KeyCode::Home => cursor = 0,
                KeyCode::End => cursor = ps.mask_value_buf.len(),
                _ => {}
            }
            ps.cursor = cursor;
            return;
        }

        // In the sidebar: ↑/↓ choose the section, → or Enter go into its rows, ↑ from the first
        // goes up to the tab bar. ← does nothing: it is how the rows come back here.
        if self.on_subtabs() && self.sidebar() {
            let pages = self.pages(self.active_tab);
            let cur = self.page().and_then(|p| pages.iter().position(|&q| q == p)).unwrap_or(0);
            match key.code {
                KeyCode::Up | KeyCode::Char('k') if cur == 0 => {
                    self.tab_focus = true;
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') => return self.goto_page_index(cur - 1),
                KeyCode::Down | KeyCode::Char('j') => return self.goto_page_index(cur + 1),
                KeyCode::Home => return self.goto_page_index(0),
                KeyCode::End => return self.goto_page_index(pages.len() - 1),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                    self.subtab_focus = false;
                    self.set_focus(0);
                    return;
                }
                KeyCode::Char('q' | '[' | ']' | 'R') | KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab | KeyCode::F(5) => {}
                KeyCode::Char(c) if c.is_ascii_digit() => {}
                _ => return,
            }
        }

        // On the sub-tab strip: ←/→ choose the sub-tab, ↓ or Enter go into its rows, ↑ goes up
        // to the tab bar. Tab switching, `[`/`]`, `R`, F5 and quitting work as on the rows;
        // nothing else applies.
        if self.on_subtabs() && !self.sidebar() {
            match key.code {
                KeyCode::Left => return self.cycle_page(-1),
                KeyCode::Right => return self.cycle_page(1),
                KeyCode::Up | KeyCode::Char('k') => {
                    self.tab_focus = true;
                    return;
                }
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
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}
            // `[` and `]`: the sub-tabs.
            KeyCode::Char('[') => self.cycle_page(-1),
            KeyCode::Char(']') => self.cycle_page(1),

            // ← from any row goes back to the sidebar (or the narrow terminal's section strip), on
            // the same section. No row changes its value with ←/→: a choice opens a list (Enter),
            // a number is typed (Enter), a switch flips (Space or Enter).
            KeyCode::Left | KeyCode::Char('h') => {
                if self.has_subtabs() {
                    self.subtab_focus = true;
                }
            }

            // Reorder mask ops with Ctrl+Up/Down (must be before regular Up/Down)
            KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Some(PipelineRow::MaskOpEntry { section: si, index: oi }) = self.focused_row() {
                    if self.move_mask_step(si, oi, true) {
                        let f = self.focus();
                        self.set_focus(f.saturating_sub(1));
                    }
                }
            }
            KeyCode::Down if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Some(PipelineRow::MaskOpEntry { section: si, index: oi }) = self.focused_row() {
                    if self.move_mask_step(si, oi, false) {
                        let max = self.focusable_rows().len().saturating_sub(1);
                        let f = self.focus();
                        if f < max { self.set_focus(f + 1); }
                    }
                }
            }

            // Navigation; up from the first row goes back to the sub-tab strip, or to the tab
            // bar on a tab without sub-tabs.
            KeyCode::Up | KeyCode::Char('k') => {
                let f = self.focus();
                if f == 0 {
                    if self.has_subtabs() {
                        self.subtab_focus = true;
                    } else {
                        self.tab_focus = true;
                    }
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
            KeyCode::Enter | KeyCode::Char(' ') if matches!(self.focused_row(), Some(PipelineRow::AdvancedFooter { .. })) => self.toggle_advanced(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let row = self.focused_row();
                // A choice opens a pop-up listing every option, the current one highlighted.
                if let Some(m) = row.as_ref().and_then(|r| self.selector_modal(r)) {
                    self.algo_modal = Some(m);
                    return;
                }
                // A mask row's number is typed.
                if let Some(text) = row.as_ref().and_then(|r| self.mask().mask_value_text(r)) {
                    let ps = &mut self.pipeline_state;
                    ps.cursor = text.len();
                    ps.mask_value_buf = text;
                    ps.mask_value_editing = true;
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
                    // MindGrab's one setting, its test-time augmentation, is a switch.
                    Some(PipelineRow::MaskOpGeneratorParam { section }) => self.toggle_mindgrab_tta(section),
                    Some(PipelineRow::MaskOpAddSection { two_pass }) => self.add_mask_section(two_pass),
                    _ => {}
                }
            }

            // Delete a refinement step (on its rows), or the section a row belongs to (the
            // "Combined with" row before it, or its input, generator and generator settings) when
            // its mask has another.
            KeyCode::Char('d') | KeyCode::Delete => {
                match self.focused_row() {
                    Some(PipelineRow::MaskOpEntry { section: si, index: oi } | PipelineRow::MaskOpSignalErodeParam { section: si, index: oi, .. }) => {
                        if self.remove_mask_step(si, oi) {
                            let max = self.focusable_rows().len().saturating_sub(1);
                            if self.focus() > max { self.set_focus(max); }
                        }
                    }
                    Some(row) => {
                        if let Some(si) = MaskRecipe::removable_section(&row) {
                            self.remove_mask_section(si);
                        }
                    }
                    None => {}
                }
            }

            // `+` / `-` nudge a masking number by its step (typing it, with Enter, is the other way).
            KeyCode::Char('+' | '=' | '-') => {
                let delta = if key.code == KeyCode::Char('-') { -1 } else { 1 };
                if let Some(row) = self.focused_row() {
                    self.nudge_mask_value(&row, delta);
                }
            }

            // Reset focused field to default
            // `a` on the Export section: tick every map.
            KeyCode::Char('a') if self.page() == Some(Page::Export) => self.form.dicom_unticked.clear(),
            KeyCode::Char('r') => self.reset_row_field(),
            // Reset every setting of the tab to its default
            KeyCode::Char('R') => self.reset_current_tab(),

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    /// The pop-up a row's choice is made in, when the row is a choice: every option listed, the
    /// current one highlighted. Enter (or Space) opens it; nothing is chosen until it is
    /// confirmed, and Esc leaves the setting as it was.
    pub fn selector_modal(&self, row: &PipelineRow) -> Option<AlgoModal> {
        use crate::pipeline::config::{MaskCombine, MaskOp};
        let ps = self.mask();
        let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let modal = |target, title: &str, options: Vec<String>, help: Vec<String>, cursor| Some(AlgoModal {
            target, title: title.to_string(), options, help, cursor,
            offset: Default::default(),
        });
        let generator = |section: usize| ps.mask_section(section).map(|s| &s.generator);
        match *row {
            PipelineRow::AlgoSelect { label, field, options, help } => modal(
                AlgoModalTarget::Select(field.to_string()), label.trim(), strings(options), strings(help), self.field_select(field)),
            PipelineRow::MaskOpInput { section } => modal(
                AlgoModalTarget::MaskInput(section), "Masking input",
                MaskRecipe::MASK_INPUT_SOURCES.iter().map(|s| format!("{}", s)).collect(),
                mask_schema().inputs.iter().map(|i| format!("{}: {}", i.label, i.help)).collect(),
                ps.mask_input_index(section)),
            PipelineRow::MaskOpGenerator { section } => modal(
                AlgoModalTarget::MaskGenerator(section), "Mask algorithm",
                strings(MASK_GENERATOR_TYPES),
                // A threshold's method is chosen on its own row.
                MASK_GENERATOR_TYPES.iter().map(|t| match *t {
                    "threshold" => "Keep the voxels of the section's image above a threshold: Otsu's, a value, a percentile, or STI Suite's".to_string(),
                    t => mask_step(t).help.to_string(),
                }).collect(),
                ps.mask_generator_index(section)),
            PipelineRow::MaskOpGeneratorParam { section } => match generator(section)? {
                MaskOp::Threshold { .. } => modal(
                    AlgoModalTarget::MaskThresholdMethod(section), "Threshold method",
                    strings(&["otsu", "fixed", "percentile", "sti-gui"]), threshold_method_help(),
                    ps.mask_threshold_method_index(section)),
                MaskOp::HdBet { .. } => {
                    let choices = &mask_schema().hd_bet_inference;
                    modal(AlgoModalTarget::MaskHdBetInference(section), "HD-BET inference",
                        choices.iter().map(|c| c.name.to_lowercase()).collect(),
                        choices.iter().map(|c| c.help.to_string()).collect(),
                        ps.hd_bet_inference_index(section))
                }
                _ => None,
            },
            PipelineRow::MaskOpHdBetStep { section } => {
                let (values, at) = ps.hd_bet_step_choices(section);
                let help = mask_param("hd-bet", "tile_step").help.to_string();
                modal(AlgoModalTarget::MaskHdBetStep(section), "HD-BET patch step",
                    values.iter().map(|v| format!("{v:.3}")).collect(), vec![help; values.len()], at)
            }
            PipelineRow::MaskOpBetVoxelScale { section } => {
                let (values, at) = ps.bet_voxel_scale_choices(section);
                let help = mask_param("bet", "voxel_scale").help.to_string();
                modal(AlgoModalTarget::MaskBetVoxelScale(section), "BET voxel scale",
                    values.iter().map(|v| format!("{v}x")).collect(), vec![help; values.len()], at)
            }
            PipelineRow::MaskCombineOp { section } => modal(
                AlgoModalTarget::MaskCombine(section), "How the sections combine",
                strings(&["OR", "AND"]),
                strings(&["Union: a voxel is in the mask when any section has it",
                          "Intersection: a voxel is in the mask only when every section has it"]),
                usize::from(ps.combine_mode(section) == MaskCombine::And)),
            PipelineRow::MaskOpEntry { section, index } => match ps.mask_refinements(section)?.get(index)? {
                MaskOp::Threshold { .. } => {
                    let cursor = match ps.mask_refinements(section)?.get(index)? {
                        MaskOp::Threshold { method, .. } => MaskRecipe::MASK_THRESHOLD_METHODS.iter().position(|m| m == method).unwrap_or(0),
                        _ => 0,
                    };
                    modal(AlgoModalTarget::MaskRefinementThresholdMethod { section, index }, "Threshold method",
                        strings(&["otsu", "fixed", "percentile", "sti-gui"]), threshold_method_help(), cursor)
                }
                _ => None,
            },
            PipelineRow::MaskOpAddStep { section } => {
                let available = MaskRecipe::available_op_types();
                modal(AlgoModalTarget::MaskAddStep(section), "Step to add",
                    available.iter().map(|t| t.to_string()).collect(),
                    available.iter().map(|t| mask_schema().steps.iter().find(|s| s.label == *t).map_or("", |s| s.help).to_string()).collect(), 0)
            }
            _ => None,
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
    /// The run's own rule (`requirements::implied_by` on the resolved config), so what the run
    /// makes, the refusal to untick it and the required-change pop-up cannot disagree: a supplied map
    /// (custom dseg, R2 or R2') satisfies the need and forces nothing.
    fn forced_by(&self, out: crate::pipeline::requirements::Output) -> Option<String> {
        // On the config as the run takes it: an output needed by one that is itself needed (R2
        // for an R2′ that χ-separation needs) counts.
        let config = super::command::config_from_app(self).ok()?.normalised().0;
        crate::pipeline::requirements::implied_by(&config, out)
    }

    /// [`Self::forced_by`], naming the setting by its label rather than its flag ("Reference =
    /// ventricles").
    fn forced_by_label(&self, out: crate::pipeline::requirements::Output) -> Option<String> {
        let config = super::command::config_from_app(self).ok()?.normalised().0;
        crate::pipeline::requirements::implied_by_label(&config, out)
    }

    // ─── Row-based tabs: pages, rows, focus and named field access ───

    /// Whether a tab is drawn and driven as named rows ([`PipelineRow`]).
    pub fn is_rows_tab(tab: usize) -> bool {
        matches!(tab, TAB_RECONSTRUCTION | TAB_EXECUTION)
    }

    /// The pipeline form as it decides its rows now: its context fresh from the rest of the form.
    fn synced_pipeline(&self, page: Page, ctx: &RowContext) -> PipelineFormState {
        let mut ps = self.pipeline_state.clone();
        ps.page = page;
        ps.ctx = ctx.clone();
        ps
    }

    /// A tab's pages. The Reconstruction tab lists every section whatever the settings, so the
    /// sidebar keeps its shape; one that does not apply says why ([`App::section_state`]).
    pub fn pages(&self, tab: usize) -> Vec<Page> {
        match tab {
            TAB_RECONSTRUCTION => Page::RECONSTRUCTION.to_vec(),
            TAB_EXECUTION => vec![Page::Execution],
            TAB_METHODS => vec![Page::MethodsText, Page::Pipeline],
            _ => Vec::new(),
        }
    }

    /// The active sub-tab: the one last chosen on this tab while it still applies, else the
    /// tab's default ([`App::default_page`]), else the first.
    pub fn page(&self) -> Option<Page> {
        let pages = self.pages(self.active_tab);
        let chosen = self.sub_page.get(self.active_tab).copied().flatten();
        chosen.or_else(|| Self::default_page(self.active_tab)).filter(|p| pages.contains(p))
            .or_else(|| pages.first().copied())
    }

    /// The page a tab opens on before one is chosen, when it is not the first: Reconstruction
    /// opens on Masking, the section most runs need to look at.
    fn default_page(tab: usize) -> Option<Page> {
        (tab == TAB_RECONSTRUCTION).then_some(Page::Masking)
    }

    /// Whether a Reconstruction section's stage or output is part of the run now, for the
    /// sidebar: on, off (its output is not made), or not applicable to the current settings.
    pub fn section_state(&self, page: Page) -> SectionState {
        let on = |b: bool| if b { SectionState::On } else { SectionState::Off };
        match page {
            Page::ComputeQsm => on(self.qsm_on()),
            Page::FieldMap | Page::Background | Page::Inversion | Page::MultiOrientation | Page::Reference => {
                let ps = self.synced_pipeline(page, &self.row_context());
                if !ps.qsm_runs() {
                    SectionState::Off
                } else if (page == Page::FieldMap && !ps.field_mapping_runs().0)
                    || (page == Page::Background && !ps.inversion_needs_bg_removal())
                    || (page == Page::MultiOrientation && !ps.ctx.orientation_group)
                {
                    SectionState::NotApplicable
                } else {
                    SectionState::On
                }
            }
            Page::Separation => on(self.form.settings.toggle("pipeline.do_chi_separation")),
            Page::Smwi => on(self.form.settings.toggle("pipeline.do_smwi")),
            Page::Magnitude => on(self.form.settings.toggle("pipeline.do_magnitude")),
            Page::Swi => on(self.form.settings.toggle("pipeline.do_swi")),
            Page::Relaxometry => on(["do_t2starmap", "do_r2starmap", "do_r2map", "do_r2primemap"].iter().any(|f| self.output_made(f))),
            Page::Segmentation => on(self.output_made("do_segmentation") || self.form.settings.toggle("pipeline.do_analysis")),
            Page::Export => on(self.form.export_dicom),
            _ => SectionState::On,
        }
    }

    /// Whether QSM is reconstructed now (Compute QSM, or another output needs it).
    pub fn qsm_on(&self) -> bool {
        self.synced_pipeline(Page::Inversion, &self.row_context()).qsm_runs()
    }

    /// The badge a Reconstruction section carries for the rules' findings on its rows: `Some(true)`
    /// when one is offered as a change (⚑), `Some(false)` for information only (ⓘ).
    pub fn section_marker(&self, view: &[qsmxt_engine::recommendations::Recommendation], page: Page) -> Option<bool> {
        let mut any = None;
        for row in self.page_rows(page) {
            if let Some(m) = App::row_marker(view, &row) {
                if m.actionable { return Some(true); }
                any = Some(false);
            }
        }
        any
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

    /// Whether the active tab shows a sub-tab strip (a row-based tab with more than one page, or
    /// the Methods tab).
    pub fn has_subtabs(&self) -> bool {
        (Self::is_rows_tab(self.active_tab) || self.active_tab == TAB_METHODS) && self.pages(self.active_tab).len() > 1
    }

    /// Whether the active tab's pages are a sidebar (the Reconstruction tab on a wide terminal).
    pub fn sidebar(&self) -> bool {
        matches!(self.active_tab, TAB_RECONSTRUCTION | TAB_METHODS) && self.sidebar_layout.get()
    }

    /// Open the active tab's page `i` (clamped), keeping focus where it is.
    fn goto_page_index(&mut self, i: usize) {
        let pages = self.pages(self.active_tab);
        let Some(&page) = pages.get(i.min(pages.len().saturating_sub(1))) else { return };
        let on_strip = self.on_subtabs();
        self.sub_page[self.active_tab] = Some(page);
        self.reset_view();
        self.sync_pipeline_mode();
        self.subtab_focus = on_strip;
    }

    /// Whether focus is on the sub-tab strip or the sidebar rather than on a row (or the tab bar).
    pub fn on_subtabs(&self) -> bool {
        self.subtab_focus && self.has_subtabs() && !self.tab_focus
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
        ps.mask_value_editing = false;
    }

    /// The active page's rows.
    pub fn rows(&self) -> Vec<PipelineRow> {
        self.page().map(|p| self.page_rows(p)).unwrap_or_default()
    }

    /// The settings Basic mode shows ([`qsmxt_engine::schema::regular_view`]), compared with the
    /// chosen pipeline's values or QSMxT's defaults; cached until the config changes.
    fn basic_paths(&self) -> BasicPaths {
        let config = self.current_config();
        let Ok(config) = config.as_ref() else { return Default::default() };
        let facts = self.view_facts();
        let key = format!("{}{facts:?}", serde_json::to_string(&config).unwrap_or_default());
        if let Some((k, paths)) = &*self.basic_cache.borrow() {
            if *k == key {
                return paths.clone();
            }
        }
        let base = qsmxt_engine::schema::baseline(config, &facts, &qsmxt_engine::PipelineConfig::default());
        let paths = std::rc::Rc::new(qsmxt_engine::schema::regular_view(config, &base, &facts));
        *self.basic_cache.borrow_mut() = Some((key, paths.clone()));
        paths
    }

    /// A Reconstruction section's rows in Basic mode: the settings [`Self::basic_paths`] lists
    /// (and the rows that edit no setting), the mask recipe as one line unless it is edited by
    /// hand, and a last row saying how many settings are hidden.
    fn basic_rows(&self, rows: Vec<PipelineRow>) -> Vec<PipelineRow> {
        use PipelineRow::*;
        let shown = self.basic_paths();
        let collapse_recipe = self.pipeline_state.mask_preset != *MASK_PRESET_CUSTOM;
        let main_recipe = |section: usize| !is_two_pass_section(section);
        let mut hidden = 0;
        // Each row, and whether it shows.
        let mut kept: Vec<(PipelineRow, bool)> = Vec::new();
        for row in rows {
            let keep = match &row {
                AlgoSelect { field, .. } | Param { field, .. } | Toggle { field, .. } => {
                    field_path(field).is_none_or(|p| shown.contains(p))
                }
                MaskCombineOp { section } | MaskOpInput { section } | MaskOpGenerator { section }
                | MaskOpGeneratorParam { section } | MaskOpThresholdValue { section } | MaskOpHdBetStep { section }
                | MaskOpBetVoxelScale { section } | MaskOpEntry { section, .. } | MaskOpSignalErodeParam { section, .. }
                | MaskOpAddStep { section } | MaskSectionHeader { section } => !(collapse_recipe && main_recipe(*section)),
                MaskOpAddSection { two_pass } => !(collapse_recipe && !*two_pass),
                MaskCombinedHeader => !collapse_recipe,
                _ => true,
            };
            if !keep && row_field(&row).is_some() {
                hidden += 1;
            }
            let preset = matches!(&row, AlgoSelect { field: "mask_preset", .. });
            kept.push((row, keep));
            if preset && collapse_recipe {
                hidden += 1; // the recipe's editor
                for text in self.mask().mask_recipe_summary() {
                    kept.push((Summary { text }, true));
                }
            }
        }
        // A note that heads a block of rows goes with them when they are all hidden.
        for i in 0..kept.len() {
            if matches!(kept[i].0, Note { .. }) {
                let block: Vec<bool> = kept[i + 1..].iter()
                    .take_while(|(r, _)| !matches!(r, Note { .. } | Separator))
                    .map(|(_, k)| *k)
                    .collect();
                if !block.is_empty() && block.iter().all(|k| !k) {
                    kept[i].1 = false;
                }
            }
        }
        let mut out: Vec<PipelineRow> = Vec::new();
        for (row, keep) in kept {
            let blank = matches!(row, Separator);
            if keep && !(blank && (out.is_empty() || matches!(out.last(), Some(Separator)))) {
                out.push(row);
            }
        }
        while matches!(out.last(), Some(Separator)) {
            out.pop();
        }
        if hidden > 0 {
            out.push(Separator);
            out.push(AdvancedFooter { hidden });
        }
        out
    }

    /// Switch between Basic and Advanced mode (`A`), keeping focus on the same setting where it
    /// still shows, and remember the choice.
    pub fn toggle_advanced(&mut self) {
        let field = self.focused_row().as_ref().and_then(row_field);
        self.advanced = !self.advanced;
        super::prefs::set_advanced(self.advanced);
        if !(field.is_some_and(|f| self.focus_field(f))) {
            let n = self.focusable_rows().len();
            if self.focus() >= n {
                self.set_focus(n.saturating_sub(1));
            }
        }
    }

    /// Any page's rows, as they would show now.
    pub fn page_rows(&self, page: Page) -> Vec<PipelineRow> {
        let config = self.current_config();
        let config = config.as_ref().as_ref().ok();
        self.page_rows_in(page, &self.row_context_with(config), config)
    }

    /// Every row of the pipeline form's pages that shows now, page after page: for tests that ask
    /// whether a setting is reachable, not where.
    #[cfg(test)]
    pub fn pipeline_rows(&self) -> Vec<PipelineRow> {
        Page::RECONSTRUCTION.iter().filter(|p| p.is_pipeline()).flat_map(|&p| self.page_rows(p)).collect()
    }

    /// Every page of every tab with its rows, as they apply now (the config and context worked
    /// out once).
    pub fn all_pages(&self) -> Vec<(usize, Page, Vec<PipelineRow>)> {
        let config = self.current_config();
        let config = config.as_ref().as_ref().ok();
        let ctx = self.row_context_with(config);
        (0..TAB_NAMES.len())
            .flat_map(|t| self.pages(t).into_iter().map(move |p| (t, p)))
            .map(|(t, p)| (t, p, self.page_rows_in(p, &ctx, config)))
            .collect()
    }

    fn page_rows_in(&self, page: Page, ctx: &RowContext, config: Option<&crate::pipeline::config::PipelineConfig>) -> Vec<PipelineRow> {
        let rows = self.all_page_rows_in(page, ctx, config);
        let rows = if self.advanced || !Page::RECONSTRUCTION.contains(&page) { rows } else { self.basic_rows(rows) };
        wrap_notes(rows, self.note_width.get().saturating_sub(2) as usize)
    }

    /// A page's rows with every setting shown (Advanced mode).
    fn all_page_rows_in(&self, page: Page, ctx: &RowContext, config: Option<&crate::pipeline::config::PipelineConfig>) -> Vec<PipelineRow> {
        if page.is_pipeline() {
            // A stage that does not run says why, whoever builds its rows.
            let ps = self.synced_pipeline(page, ctx);
            if let Some(notes) = ps.page_notes() {
                return notes;
            }
            return self.registry_rows(page, ctx, config).unwrap_or_else(|| ps.visible_rows());
        }
        if let Some(rows) = self.registry_rows(page, ctx, config) {
            return rows;
        }
        match page {
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

    pub(super) fn set_focus(&mut self, focus: usize) {
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
        if let Some(path) = stored_path(field) {
            return self.form.settings.select(path);
        }
        if let Some(&c) = self.form.choice(field) {
            return c;
        }
        self.pipeline_state.get_select(field)
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
        if let Some(path) = stored_path(field) {
            return self.form.settings.set_select(path, val);
        }
        if let Some(c) = self.form.choice_mut(field) {
            *c = val;
            return;
        }
        if field == "mask_preset" {
            return self.choose_mask_preset(val);
        }
        self.pipeline_state.set_select(field, val)
    }

    /// Whether the run makes an output: its checkbox is ticked, or another setting needs it (a
    /// required change the run makes itself when it is left off, [`App::forced_by`]).
    pub fn output_made(&self, field: &str) -> bool {
        if field == "do_segmentation" {
            return self.field_toggle(field)
                || self.forced_by(crate::pipeline::requirements::Output::Segmentation).is_some();
        }
        if field == "do_qsm" && !self.pipeline_state.do_qsm {
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
        if field == "do_segmentation" {
            let st = &self.form.settings;
            return st.toggle("pipeline.do_segmentation") || !st.text(DSEG).trim().is_empty();
        }
        if let Some(path) = stored_path(field) {
            return self.form.settings.toggle(path);
        }
        match self.form.flag(field) {
            Some(&b) => b,
            None => self.pipeline_state.get_toggle(field),
        }
    }

    /// Flip a checkbox. Turning off an output another setting needs is refused, with the reason.
    pub fn toggle_field(&mut self, field: &str) {
        if let Some(m) = dicom_map(field) {
            return self.toggle_dicom_map(m);
        }
        if field == "do_qsm" && self.pipeline_state.do_qsm {
            // QSM another output needs: refuse turning it off, as for the other maps.
            self.pipeline_state.do_qsm = false;
            let why = self.row_context().qsm_forced_by;
            self.pipeline_state.do_qsm = true;
            if let Some(why) = why {
                self.error_message = Some(format!("QSM is required: {why} — change or turn that off first."));
                return;
            }
        }
        if field == "do_segmentation" {
            if !self.field_toggle(field) {
                self.form.settings.set("pipeline.do_segmentation", true);
                return;
            }
            // What needs a segmentation whichever its source: asked with no dseg supplied.
            let kept = self.form.settings.clone();
            self.form.settings.reset(DSEG, false);
            let why = self.forced_by_label(crate::pipeline::requirements::Output::Segmentation);
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
            "orientation_reference" => "first by name",
            _ if stored_path(field).is_some() => stored_placeholder(stored_path(field).unwrap_or_default()),
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
    pub(crate) fn reset_field(&mut self, field: &str) {
        let (form, ps) = (RunForm::default(), PipelineFormState::default());
        if let Some(m) = dicom_map(field) {
            return self.form.dicom_unticked.retain(|u| !u.eq_ignore_ascii_case(m.suffix));
        }
        match field {
            "segmentation_source" | "do_segmentation" => {
                self.form.settings.reset(DSEG, false);
                self.form.settings.reset("pipeline.do_segmentation", false);
                return;
            }
            _ => {}
        }
        if let Some(path) = stored_path(field) {
            let value_row = super::settings_rows::is_value_row(field, path);
            return self.form.settings.reset(path, value_row);
        }
        if let (Some(d), Some(v)) = (form.text(field), self.form.text_mut(field)) {
            *v = d.clone();
        } else if let (Some(d), Some(v)) = (form.flag(field), self.form.flag_mut(field)) {
            *v = *d;
        } else if let (Some(d), Some(v)) = (form.choice(field), self.form.choice_mut(field)) {
            *v = *d;
        } else if ps.get_toggle(field) != self.pipeline_state.get_toggle(field) {
            self.pipeline_state.toggle(field);
        } else {
            self.set_field_select(field, ps.get_select(field));
        }
    }

    /// Reset every setting of the active sub-tab: each field it shows, again for the rows a
    /// reset reveals, until none are new (and the mask recipe, on the Masking page).
    fn reset_current_page(&mut self) {
        let Some(page) = self.page() else { return };
        self.reset_page(page);
        self.reset_view();
    }

    /// Every setting on `page` back to its default (`R`, and a pipeline's stages).
    pub(super) fn reset_page(&mut self, page: Page) {
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

    /// Export → DICOM: the switch, the source DICOM, and a tick per map the settings make.
    fn export_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{Note, Param, Toggle};
        let mut rows = vec![Toggle { label: "Export DICOM", field: "export_dicom", help: "Also write final maps as DICOM series into each subject's extra_files/ folder" }];
        if self.form.export_dicom {
            rows.push(Param { label: "  Source DICOM dir", field: "source_dicom", help: "Original DICOM directory: the exported series inherit its patient/study identity (empty = none)" });
            rows.push(PipelineRow::Separator);
            rows.push(Note { text: "Maps to export".into() });
            let made: Vec<_> = DICOM_MAPS.iter().filter(|m| self.dicom_map_made(m)).collect();
            if made.is_empty() {
                rows.push(Note { text: "  None: these settings make no exportable map.".into() });
            }
            for m in made {
                rows.push(Toggle { label: m.label, field: m.field,
                    help: if m.field == "dicom_out_dseg" {
                        "The segmentation as one DICOM Segmentation object, its structures coded and coloured, drawn on an exported series on its grid: the magnitude, else χ, else another map"
                    } else {
                        "Space ticks or unticks the map. While every map is ticked all are exported, and a map turned on later is ticked too; a ticks them all"
                    } });
            }
            // The SEG is drawn on another exported series on its grid (the magnitude, χ, another
            // map): the run refuses a selection that leaves it none, and the checklist says so.
            let seg = dicom_map("dicom_out_dseg").expect("in the table");
            let reference_ticked = DICOM_MAPS.iter().any(|m| self.dicom_map_made(m) && self.field_toggle(m.field)
                && crate::dicom::export::SEG_REFERENCES.iter().any(|r| r.eq_ignore_ascii_case(m.suffix)));
            if self.dicom_map_made(seg) && self.field_toggle(seg.field) && !reference_ticked {
                rows.push(Note { text: "The segmentation needs a map to be drawn on: tick the".into() });
                rows.push(Note { text: "magnitude, χ or another map, or the run is refused.".into() });
            }
        }
        rows
    }

    /// Whether the current settings make an exportable map.
    fn dicom_map_made(&self, m: &DicomMap) -> bool {
        match m.group {
            // The combined magnitude, when it is published.
            "magnitude" => self.form.settings.toggle("pipeline.do_magnitude"),
            "χ" => self.qsm_on(),
            "SWI" => self.form.settings.toggle("pipeline.do_swi"),
            "SWI phase" => self.form.settings.toggle("pipeline.do_swi") && self.form.settings.toggle("swi.filtered_phase"),
            "SMWI" => self.form.settings.toggle("pipeline.do_smwi"),
            "T2*" => self.output_made("do_t2starmap"),
            "R2*" => self.output_made("do_r2starmap"),
            "R2" => self.output_made("do_r2map"),
            "R2′" => self.output_made("do_r2primemap"),
            "χ-separation" => self.form.settings.toggle("pipeline.do_chi_separation"),
            "segmentation" => self.output_made("do_segmentation"),
            _ => false,
        }
    }

    /// `--dicom-outputs`: the maps made and ticked, when any made map is unticked; `None`
    /// exports every map made.
    pub fn dicom_outputs(&self) -> Option<Vec<String>> {
        let made: Vec<&DicomMap> = DICOM_MAPS.iter().filter(|m| self.dicom_map_made(m)).collect();
        let unticked = |m: &&DicomMap| self.form.dicom_unticked.iter().any(|u| u.eq_ignore_ascii_case(m.suffix));
        if !made.iter().any(unticked) {
            return None;
        }
        Some(made.into_iter().filter(|m| !unticked(m)).map(|m| m.suffix.to_string()).collect())
    }

    /// Tick or untick an Export map. The last ticked map is not unticked: that would export
    /// nothing, which Export DICOM itself says.
    fn toggle_dicom_map(&mut self, m: &DicomMap) {
        let pos = self.form.dicom_unticked.iter().position(|u| u.eq_ignore_ascii_case(m.suffix));
        if let Some(i) = pos {
            self.form.dicom_unticked.remove(i);
            return;
        }
        let ticked = DICOM_MAPS.iter().filter(|d| self.dicom_map_made(d) && self.field_toggle(d.field)).count();
        if ticked <= 1 {
            self.error_message = Some("That is the only map left to export: untick Export DICOM instead.".to_string());
            return;
        }
        self.form.dicom_unticked.push(m.suffix.to_lowercase());
    }

    /// The Execution tab: local or SLURM, each mode's own settings, then what applies to both.
    fn execution_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{AlgoSelect, Param, Toggle};
        let local = self.form.execution_mode == 0;
        let mut rows = vec![AlgoSelect { label: "Execution mode", field: "execution_mode", options: EXECUTION_MODE_OPTIONS, help: EXECUTION_MODE_HELP }];
        if local {
            rows.push(Toggle { label: "  Dry run", field: "dry_run", help: "Print the processing plan, pipeline graph and methods text without running anything" });
            rows.push(Param { label: "  Num processes", field: "n_procs", help: "Number of parallel threads (empty = auto)" });
            // The memory limit schedules concurrent local runs.
            rows.push(Param { label: "  Memory limit (GB)", field: "mem_limit_gb", help: "Memory budget for scheduling concurrent runs: auto (empty) detects it, a number of GB, or none for no limit" });
        } else {
            rows.push(Param { label: "  SLURM account", field: "slurm_account", help: "SLURM account name (required for SLURM mode)" });
            rows.push(Param { label: "  SLURM partition", field: "slurm_partition", help: "SLURM partition (optional)" });
            rows.push(Param { label: "  SLURM time limit", field: "slurm_time", help: "Wall time limit per job (e.g. 02:00:00)" });
            rows.push(Param { label: "  SLURM memory (GB)", field: "slurm_mem", help: "Memory per job in GB (empty = estimated per run)" });
            rows.push(Param { label: "  SLURM CPUs/Task", field: "slurm_cpus", help: "CPUs per SLURM task" });
            rows.push(Toggle { label: "  Auto-submit", field: "slurm_submit", help: "Automatically submit generated scripts via sbatch" });
        }
        rows.push(PipelineRow::Separator);
        rows.push(Toggle { label: "Debug logging", field: "debug", help: "Enable verbose debug log output" });
        rows.push(Toggle { label: "Force re-run", field: "force", help: "Ignore cached pipeline state and recompute everything" });
        rows.push(Toggle { label: "Clean intermediates", field: "clean_intermediates", help: "Remove intermediate files after the run, keeping only final outputs" });
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
    const SYNTHSEG_ROWS: [&str; 6] = ["synthseg_version", "synthseg_crop", "synthseg_patch", "synthseg_flip_averaging",
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
        assert_eq!(app.example_state.cursor, ExampleState::BUTTON);
        let picked = app.example_state.selected_examples();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].id, crate::example::DEFAULT_ID);
        assert_eq!(app.example_state.selected_bytes(), picked[0].bytes);
    }

    #[test]
    fn example_picker_navigates_and_ticks() {
        let mut app = example_app();

        // Down from the button walks into the list, one row at a time; Space ticks a second
        // acquisition.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.example_state.acquisition(), Some(1));
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.example_state.selected_examples().len(), 2);

        // Space again unticks it.
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.example_state.selected_examples().len(), 1);

        // Down cannot walk past the last acquisition.
        for _ in 0..=ExampleState::last_row() {
            app.handle_key(key(KeyCode::Down));
        }
        assert_eq!(app.example_state.cursor, ExampleState::last_row());
    }

    /// From the IO fields, Down reaches the button, and the next Down the first acquisition.
    #[test]
    fn example_picker_is_walked_in_the_order_it_is_drawn() {
        let mut app = example_app();
        app.active_field = App::INPUT_IO_FIELDS - 1;
        app.handle_key(key(KeyCode::Down));
        assert_eq!((app.active_field, app.example_state.cursor), (App::INPUT_IO_FIELDS, ExampleState::BUTTON));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.example_state.acquisition(), Some(0));
        assert_eq!(crate::example::EXAMPLES[0].id, "prisma-bridge-run1");
    }

    #[test]
    fn example_picker_returns_to_the_io_fields_from_the_top() {
        let mut app = example_app();
        app.example_state.cursor = ExampleState::BUTTON;
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
        app.example_state.cursor = 1;
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.example_state.output_dir.is_empty());
        assert_eq!(app.example_state.cursor, ExampleState::BUTTON);
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
                step(&mut app, 1);
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
    fn update_prompt_esc_defers_to_the_next_start() {
        let mut app = update_app();
        assert_eq!(app.handle_update_modal_key(key(KeyCode::Esc)), UpdateChoice::Later);
        assert!(!app.should_update);
        assert!(app.update_modal.is_none());
    }

    /// There is no way to skip a release: the prompt comes back at every start until the
    /// user updates, so `d` does nothing.
    #[test]
    fn update_prompt_has_no_skip() {
        let mut app = update_app();
        assert_eq!(app.handle_update_modal_key(key(KeyCode::Char('d'))), UpdateChoice::Ignored);
        assert!(app.update_modal.is_some());
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

    /// A dev build must not spawn a network check just by opening the TUI.
    #[test]
    fn a_dev_build_starts_no_background_check() {
        let mut state = UpdateState::start();
        assert!(state.poll().is_none());
    }

    /// An app whose launch checks have come back: `update` the newer release found (if any),
    /// `news` the announcements fetched.
    fn launched(update: Option<&str>, news: Vec<crate::commands::news::Announcement>) -> App {
        let mut app = App::new();
        app.update_state = UpdateState::ready(update.map(|tag| crate::commands::update::check::UpdateInfo {
            tag: tag.into(),
            html_url: "https://example.com/r".into(),
        }));
        app.news_state = NewsState::ready(crate::commands::news::Loaded { items: news, error: None });
        app
    }

    use crate::commands::news::tests::item;

    /// The pop-ups' order: critical announcements, the update prompt, then ordinary ones
    /// (newest first), one at a time; closing one records it as shown.
    #[test]
    fn launch_pop_ups_come_one_at_a_time_in_order() {
        let t = crate::commands::news::TestDir::new();
        let mut app = launched(Some("v99.0.0"), vec![
            item("old", "2026-11-01", false, None),
            item("new", "2026-11-05", false, None),
            item("crit", "2026-10-01", true, None),
        ]);
        app.poll_launch_checks();
        let shown = |app: &App| app.news_modal.as_ref().map(|m| m.items[0].id.clone());
        assert_eq!(shown(&app).as_deref(), Some("crit"), "critical first");
        assert!(app.update_modal.is_none(), "no pop-up opens over another");

        app.handle_key(key(KeyCode::Enter));
        app.poll_launch_checks();
        assert!(app.update_modal.is_some(), "then the update prompt");
        assert!(app.news_modal.is_none());

        app.handle_key(key(KeyCode::Esc));
        app.poll_launch_checks();
        assert_eq!(shown(&app).as_deref(), Some("new"), "then ordinary news, newest first");
        app.handle_key(key(KeyCode::Esc));
        app.poll_launch_checks();
        assert_eq!(shown(&app).as_deref(), Some("old"));
        app.handle_key(key(KeyCode::Char('q')));
        app.poll_launch_checks();
        assert!(app.news_modal.is_none() && app.update_modal.is_none());

        // Next start: only the critical one (and the update) come back.
        let mut app = launched(Some("v99.0.0"), vec![
            item("old", "2026-11-01", false, None),
            item("new", "2026-11-05", false, None),
            item("crit", "2026-10-01", true, None),
        ]);
        app.poll_launch_checks();
        assert_eq!(shown(&app).as_deref(), Some("crit"));
        app.handle_key(key(KeyCode::Enter));
        app.poll_launch_checks();
        assert!(app.update_modal.is_some());
        app.handle_key(key(KeyCode::Esc));
        app.poll_launch_checks();
        assert!(app.news_modal.is_none(), "ordinary news pops up once");
        drop(t);
    }

    /// Announcements wait while another pop-up (the startup tip, an algorithm list) is open.
    #[test]
    fn announcements_wait_for_other_pop_ups() {
        let _t = crate::commands::news::TestDir::new();
        let mut app = launched(None, vec![item("crit", "2026-10-01", true, None), item("a", "2026-11-01", false, None)]);
        app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
        app.poll_launch_checks();
        assert!(app.news_modal.is_none(), "not over the tip");
        app.tip_modal = None;
        app.poll_launch_checks();
        assert_eq!(app.news_modal.as_ref().unwrap().items[0].id, "crit");
    }

    /// The update prompt waits for the announcements (a critical one goes first), and ordinary
    /// announcements wait for the release check (the prompt goes first).
    #[test]
    fn each_pop_up_waits_for_the_check_that_goes_before_it() {
        let _t = crate::commands::news::TestDir::new();
        let mut app = launched(Some("v99.0.0"), Vec::new());
        let (_tx, rx) = std::sync::mpsc::channel();
        app.news_state = NewsState { receiver: Some(rx), done: false };
        app.poll_launch_checks();
        assert!(app.update_modal.is_none(), "the announcements are still being fetched");

        let mut app = launched(None, vec![item("a", "2026-11-01", false, None)]);
        let (_tx2, rx2) = std::sync::mpsc::channel();
        app.update_state = UpdateState { receiver: Some(rx2), done: false };
        app.poll_launch_checks();
        assert!(app.news_modal.is_none(), "the release check is still running");
    }

    /// An announcement for other versions does not pop up; nor does a ranged one on a dev build.
    #[test]
    fn announcements_for_other_versions_do_not_pop_up() {
        let _t = crate::commands::news::TestDir::new();
        let mut app = launched(None, vec![item("a", "2026-11-01", true, Some(("1.0.0", "2.0.0")))]);
        app.poll_launch_checks();
        assert!(app.news_modal.is_none());
        assert!(app.news_queue.is_empty());
    }

    /// `n` on the tab bar opens every announcement, newest first, including those already
    /// shown; it closes without recording anything.
    #[test]
    fn n_on_the_tab_bar_opens_the_history() {
        let _t = crate::commands::news::TestDir::new();
        let mut app = launched(None, vec![item("new", "2026-11-05", false, None), item("old", "2026-11-01", false, Some(("1.0.0", "2.0.0")))]);
        app.poll_launch_checks();
        app.news_modal = None;
        app.news_queue.clear();
        app.tab_focus = true;
        app.handle_key(key(KeyCode::Char('n')));
        let m = app.news_modal.as_ref().expect("the history should open");
        assert!(m.history);
        assert_eq!(m.items.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["new", "old"]);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.news_modal.as_ref().unwrap().scroll, 1, "↓ scrolls");
        app.handle_key(key(KeyCode::Esc));
        assert!(app.news_modal.is_none());
        assert!(!app.should_quit, "Esc closes the history, not the TUI");
        assert!(crate::commands::news::load_seen().is_empty(), "reading the history records nothing");
    }

    /// Offline, the history is the saved copy, and says so.
    #[test]
    fn the_history_offline_is_the_saved_copy() {
        let _t = crate::commands::news::TestDir::new();
        crate::commands::news::load(|| Ok("[[announcement]]\nid=\"a\"\ndate=\"2026-11-01\"\ntitle=\"Saved\"\nbody=\"b\"\n".into()));
        let mut app = App::new();
        app.receive_news(crate::commands::news::load(|| Err("offline".into())));
        app.open_news_history();
        let m = app.news_modal.as_ref().unwrap();
        assert_eq!(m.items[0].title, "Saved");
        assert!(m.note.as_deref().is_some_and(|n| n.contains("could not be read")));
    }

    /// `c` in a pop-up copies its link.
    #[test]
    fn c_copies_an_announcements_link() {
        let _t = crate::commands::news::TestDir::new();
        let mut a = item("a", "2026-11-01", false, None);
        a.link = Some("https://example.com/a".into());
        let mut app = launched(None, vec![a]);
        app.poll_launch_checks();
        app.handle_key(key(KeyCode::Char('c')));
        assert_eq!(app.clipboard.as_deref(), Some("https://example.com/a"));
        assert!(app.news_modal.is_some(), "copying does not close it");
    }

    #[test]
    fn test_algo_modal_opens_on_enter_over_algoselect() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        // Move down the section to a selector and open it.
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
            title: "QSM inversion".to_string(),
            options: QSM_ALGO_OPTIONS.iter().map(|s| s.to_string()).collect(),
            help: vec![String::new(); QSM_ALGO_OPTIONS.len()],
            cursor: 0,
            offset: Default::default(),
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
            title: "QSM inversion".to_string(),
            options: vec!["a".to_string(), "b".to_string()],
            help: vec![String::new(), String::new()],
            cursor: 1,
            offset: Default::default(),
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
            app.mask().sections[0].input,
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
        assert!(app.field_toggle("phase_offset_removal"));
        app.toggle_field("phase_offset_removal");
        assert!(!app.field_toggle("phase_offset_removal"));
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
        assert!(app.error_message.as_deref() == Some("SLURM account is required"));
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
        app.active_tab = TAB_RECONSTRUCTION;
        // Should not crash — filter handler takes over
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('n')));
    }

    #[test]
    fn test_pipeline_get_param() {
        let mut app = App::new();
        *app.field_text_mut("bet_smoothness").unwrap() = "0.2".to_string();
        assert_eq!(app.field_text("bet_smoothness"), "0.2");
    }


    #[test]
    fn test_text_value_unknown_returns_empty() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION; // Algorithms tab - no text fields
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
        assert_eq!(app.text_value(), "");
    }

    // --- get_ accessors (used by UI rendering) ---

    /// Every row on every page names a field the accessors resolve, with the kind it edits.
    #[test]
    fn rows_name_real_fields() {
        let mut app = App::new();
        app.form.settings.set("pipeline.do_swi", true);
        app.form.settings.set("pipeline.do_smwi", true);
        app.form.settings.set("pipeline.do_segmentation", true);
        app.form.export_dicom = true;
        app.form.settings.set("pipeline.do_analysis", true);
        app.form.settings.set("pipeline.do_r2primemap", true);
        app.form.settings.set("motion_correction.enabled", true);
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
        assert!(!form.settings.toggle("pipeline.do_swi"));
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
        let mut app = App::new();
        assert!(!app.field_text("bet_smoothness").is_empty()); // has QSM.rs default
        if let Some(s) = app.field_text_mut("bet_smoothness") {
            *s = "0.25".to_string();
        }
        assert_eq!(app.field_text("bet_smoothness"), "0.25");
        let mut ps = super::PipelineFormState::default();

        // Test select (qsm_algorithm)
        ps.set_select("qsm_algorithm", 2);
        assert_eq!(ps.get_select("qsm_algorithm"), 2);
    }

    #[test]
    fn test_pipeline_visible_rows_change_with_algorithm() {
        let mut app = App::new();
        app.pipeline_state.qsm_algorithm = 0; // RTS
        let rows_rts = app.pipeline_rows().len();
        app.pipeline_state.qsm_algorithm = 2; // TKD (fewer params)
        let rows_tkd = app.pipeline_rows().len();
        assert!(rows_tkd < rows_rts, "TKD should have fewer rows than RTS");

        app.pipeline_state.qsm_algorithm = 4; // TGV (hides background removal)
        let rows_tgv = app.pipeline_rows().len();
        assert!(rows_tgv < rows_rts, "TGV should hide background removal");
    }

    /// `[` and `]` walk the sub-tabs of the active tab, wrapping, and put focus back on top.
    #[test]
    fn brackets_switch_sub_tabs_and_reset_focus() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.sync_pipeline_mode();
        // Masking, the second section, is the one Reconstruction opens on.
        assert_eq!(app.pages(TAB_RECONSTRUCTION)[..3], [Page::Geometry, Page::Masking, Page::Motion]);
        assert_eq!(app.page(), Some(Page::Masking));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focus(), 1);
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.page(), Some(Page::Motion));
        assert_eq!(app.focus(), 0, "a sub-tab switch starts at the top");
        app.handle_key(key(KeyCode::Char('[')));
        app.handle_key(key(KeyCode::Char('[')));
        assert_eq!(app.page(), Some(Page::Geometry));
        app.handle_key(key(KeyCode::Char('[')));
        assert_eq!(app.page(), Some(Page::Export), "wraps");
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.page(), Some(Page::Geometry));
        // The section is remembered per tab; a main-tab switch resets focus too.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.active_tab, TAB_EXECUTION);
        assert_eq!(app.focus(), 0);
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.page(), Some(Page::Geometry));
    }

    /// Every QSM section stays in the sidebar; one whose stage the settings skip is marked not
    /// applicable and says why, and with QSM off each shows the switch alone.
    #[test]
    fn qsm_sections_stay_and_say_whether_their_stage_runs() {
        let mut app = App::new();
        let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name).unwrap();
        assert_eq!(app.pages(TAB_RECONSTRUCTION), Page::RECONSTRUCTION.to_vec());
        let state = |app: &App, p| app.section_state(p);
        assert_eq!(state(&app, Page::FieldMap), SectionState::On);
        assert_eq!(state(&app, Page::Background), SectionState::On);
        app.pipeline_state.qsm_algorithm = idx("iqsm");
        assert_eq!(app.pages(TAB_RECONSTRUCTION), Page::RECONSTRUCTION.to_vec(), "the list keeps its shape");
        assert_eq!(state(&app, Page::FieldMap), SectionState::NotApplicable);
        assert_eq!(state(&app, Page::Background), SectionState::NotApplicable);
        let rows = |app: &App, p| app.page_rows(p);
        assert!(rows(&app, Page::Background).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("No background removal"))));
        assert!(!rows(&app, Page::Background).iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "bf_algorithm", .. })));
        assert!(rows(&app, Page::FieldMap).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("No field mapping"))));
        // Without a group, COSMOS/STI says where groups are chosen; with one, it has its settings.
        assert_eq!(state(&app, Page::MultiOrientation), SectionState::NotApplicable);
        assert!(!rows(&app, Page::MultiOrientation).iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "orientation_algorithm", .. })));
        assert!(rows(&app, Page::MultiOrientation).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("1 Input → Orientations"))));
        app.filter_state.orientation_group = "acq".into();
        assert_eq!(state(&app, Page::MultiOrientation), SectionState::On);
        assert!(rows(&app, Page::MultiOrientation).iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "orientation_algorithm", .. })));
        // QSM's switch is a section of its own, and nowhere else.
        assert_eq!(state(&app, Page::ComputeQsm), SectionState::On);
        for p in Page::RECONSTRUCTION {
            let has = app.page_rows(p).iter().any(|r| row_field(r) == Some("do_qsm"));
            assert_eq!(has, p == Page::ComputeQsm, "{p:?}");
        }
        // QSM off: every QSM stage is off and says where QSM is turned on; its section says
        // what off means.
        app.pipeline_state.do_qsm = false;
        assert_eq!(state(&app, Page::ComputeQsm), SectionState::Off);
        for p in [Page::FieldMap, Page::Background, Page::Inversion, Page::MultiOrientation, Page::Reference] {
            assert_eq!(state(&app, p), SectionState::Off, "{p:?}");
            assert_eq!(rows(&app, p), vec![PipelineRow::Note { text: QSM_OFF_NOTE.into() }], "{p:?}");
        }
        assert!(rows(&app, Page::ComputeQsm).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("Off: no field map"))));
        // A forced QSM keeps the stages, and its section says why.
        app.form.settings.set("pipeline.do_smwi", true);
        assert_eq!(state(&app, Page::Inversion), SectionState::On);
        assert_eq!(state(&app, Page::ComputeQsm), SectionState::On);
        assert!(rows(&app, Page::ComputeQsm).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("QSM → SMWI"))));
    }

    /// The QSM group holds QSM's stages and the outputs made from χ (χ-separation, SMWI); SWI,
    /// from the phase alone, is an other output.
    #[test]
    fn the_qsm_group_holds_what_needs_qsm() {
        let group = |g: &str| Page::RECONSTRUCTION.iter().copied().filter(|p| p.group() == g).collect::<Vec<_>>();
        assert_eq!(group("QSM"), [Page::ComputeQsm, Page::FieldMap, Page::Background, Page::Inversion,
            Page::MultiOrientation, Page::Reference, Page::Separation, Page::Smwi]);
        assert_eq!(group("Other outputs"), [Page::Magnitude, Page::Swi, Page::Relaxometry, Page::Segmentation]);
        let app = App::new();
        let fields = |p| app.page_rows(p).iter().filter_map(row_field).collect::<Vec<_>>();
        assert!(fields(Page::Swi).contains(&"do_swi") && !fields(Page::Swi).contains(&"do_smwi"));
        assert!(fields(Page::Smwi).contains(&"do_smwi") && !fields(Page::Smwi).contains(&"do_swi"));
        assert_eq!(app.section_state(Page::Smwi), SectionState::Off);
        assert_eq!(app.section_state(Page::Swi), SectionState::Off);
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

    /// ←/→ on a run walks it through no group, each group of its session as a member and as
    /// the reference, and a new group; the groups are kept as a `runs:` pattern. A typed
    /// pattern's groups are where it starts.
    #[test]
    fn arrows_give_runs_their_orientation_groups() {
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_multi_orientation_bids(dir.path());
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.form.bids_dir = dir.path().to_string_lossy().into_owned();
        app.filter_state.maybe_rescan(&app.form.bids_dir.clone());
        app.active_field = App::INPUT_IO_FIELDS;
        let rows = app.filter_state.visible_rows();
        let runs: Vec<usize> = (0..rows.len()).filter(|&i| matches!(rows[i], TreeRow::Run { .. })).collect();
        let press = |app: &mut App, i: usize, code: KeyCode| {
            app.filter_state.focus = FilterFocus::TreeNode(i);
            app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        let slot = |app: &mut App, i: usize| {
            app.filter_state.focus = FilterFocus::TreeNode(i);
            let (slots, now) = app.filter_state.orientation_slots("").unwrap();
            slots[now]
        };
        // → puts the first run in a new group; the next run's → joins it, then is its reference.
        press(&mut app, runs[0], KeyCode::Right);
        assert_eq!(slot(&mut app, runs[0]), OrientationSlot::New);
        press(&mut app, runs[1], KeyCode::Right);
        assert_eq!(slot(&mut app, runs[1]), OrientationSlot::Member(0));
        let preview = app.filter_state.orientation_preview("");
        assert_eq!(preview.groups.len(), 1);
        press(&mut app, runs[1], KeyCode::Right);
        assert_eq!(slot(&mut app, runs[1]), OrientationSlot::Reference(0));
        assert_eq!(app.filter_state.orientation_groups("")[0][0], app.filter_state.orientation_preview("").order[runs_order(&app, runs[1])]);
        // → again: a second group of its own; ← back.
        press(&mut app, runs[1], KeyCode::Right);
        assert_eq!(slot(&mut app, runs[1]), OrientationSlot::New);
        press(&mut app, runs[1], KeyCode::Left);
        assert_eq!(slot(&mut app, runs[1]), OrientationSlot::Reference(0));
        // ← all the way: out of every group, and with nothing grouped, off.
        for run in [runs[1], runs[0]] {
            for _ in 0..4 {
                press(&mut app, run, KeyCode::Left);
            }
            assert_eq!(slot(&mut app, run), OrientationSlot::None);
        }
        assert!(app.filter_state.orientation_group.is_empty(), "{}", app.filter_state.orientation_group);

        // From a typed pattern, the arrows start with what it grouped.
        app.filter_state.orientation_group = "*acq-dir*".into();
        let grouped = app.filter_state.orientation_preview("").member_of.len();
        let member = runs.iter().copied().find(|&i| slot(&mut app, i) != OrientationSlot::None).unwrap();
        press(&mut app, member, KeyCode::Left);
        assert!(app.filter_state.orientation_group.starts_with("runs:"));
        let listed: usize = app.filter_state.orientation_groups("").iter().map(Vec::len).sum();
        assert_eq!(listed, grouped);
        assert!(crate::bids::orientation::parse_pattern(&app.filter_state.orientation_group).unwrap().is_some());

        // ←/→ on a subject or session still collapses and expands it.
        let subject = (0..rows.len()).find(|&i| matches!(rows[i], TreeRow::Subject(_))).unwrap();
        let before = app.filter_state.visible_rows().len();
        press(&mut app, subject, KeyCode::Left);
        assert!(app.filter_state.visible_rows().len() < before);
    }

    /// The position of the run at tree row `i` among the selected runs.
    fn runs_order(app: &App, i: usize) -> usize {
        let rows = app.filter_state.visible_rows();
        let TreeRow::Run { sub, ses, run } = rows[i] else { panic!() };
        let tree = app.filter_state.tree.as_ref().unwrap();
        let key = match ses {
            Some(sei) => &tree.subjects[sub].sessions[sei].runs[run].key_string,
            None => &tree.subjects[sub].runs[run].key_string,
        };
        app.filter_state.orientation_preview("").order.iter().position(|k| k == key).unwrap()
    }

    /// The Input tab keeps only the pattern: its settings are on QSM → COSMOS/STI, so
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
        let preview = fs.orientation_preview("");
        assert!(preview.error.unwrap().contains("chunk"));

        fs.orientation_group = "re:(a)(b)".into();
        assert!(fs.orientation_preview("").error.is_some());
    }

    #[test]
    fn orientation_preview_is_empty_when_off() {
        let fs = super::FilterTreeState::default();
        let preview = fs.orientation_preview("");
        assert!(preview.error.is_none());
        assert!(preview.groups.is_empty());
    }

    #[test]
    fn the_orientation_grouping_is_none_until_a_pattern_is_set() {
        let mut fs = super::FilterTreeState::default();
        assert_eq!(fs.orientation_group_by(), None);
        fs.orientation_group = "  acq  ".into();
        assert_eq!(fs.orientation_group_by().as_deref(), Some("acq"), "the pattern is trimmed");
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
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "do_swi"));
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "do_r2starmap"));
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
        step(&mut app, 1);
        assert_eq!(app.input_mode, InputMode::NIfTI);
    }

    #[test]
    fn test_nifti_focus_navigation_into_nifti_section() {
        let mut app = App::new();
        app.active_field = 0;
        step(&mut app, 1); // switch to NIfTI mode
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
        step(&mut app, 1); // switch to NIfTI
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
        step(&mut app, 1); // switch to NIfTI
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
        step(&mut app, 1); // switch to NIfTI
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
        step(&mut app, 1);
        step(&mut app, 1);
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
        step(&mut app, 1); // switch to NIfTI
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
        step(&mut app, 1); // switch to NIfTI
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
        app.active_tab = TAB_RECONSTRUCTION;
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
        app.active_tab = TAB_RECONSTRUCTION;
        app.sync_pipeline_mode();
        let max = app.focusable_rows().len().saturating_sub(1);
        app.pipeline_state.focus = max;
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.pipeline_state.focus, max);
    }

    #[test]
    fn test_pipeline_tab_switch_from_pipeline() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_EXECUTION);
        app.active_tab = TAB_RECONSTRUCTION;
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.active_tab, TAB_PIPELINE);
    }

    #[test]
    fn test_pipeline_tab_number_switch() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, 2);
    }

    #[test]
    fn test_switch_to_separation_resets_focus_to_first_row() {
        let mut app = App::new();
        // Sit deep in the Field map section, past the number of rows χ-separation will have.
        app.active_tab = TAB_RECONSTRUCTION;
        app.sub_page[TAB_RECONSTRUCTION] = Some(Page::FieldMap);
        app.sync_pipeline_mode();
        let qsm_max = app.focusable_rows().len().saturating_sub(1);
        app.pipeline_state.focus = qsm_max;
        // Switch to χ-separation; the page changes, so focus must reset to the first row.
        for _ in 0..5 { app.handle_key(key(KeyCode::Char(']'))); }
        assert_eq!(app.pipeline_state.page, Page::Separation);
        assert_eq!(app.pipeline_state.focus, 0);
        // And it must be in range for the Separation row set.
        let sep_len = app.focusable_rows().len();
        assert!(app.pipeline_state.focus < sep_len.max(1));
    }

    #[test]
    fn test_pipeline_left_right_algo_select() {
        let mut app = App::new();
        assert!(app.goto_field("qsm_algorithm"));
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
        step(&mut app, 1);
        assert_eq!(QSM_ALGO_OPTIONS[app.pipeline_state.qsm_algorithm], QSM_ALGO_OPTIONS[ilsqr + 1]);
        step(&mut app, -1);
        assert_eq!(app.pipeline_state.qsm_algorithm, ilsqr);
    }

    #[test]
    fn test_pipeline_enter_opens_the_unwrapping_picker() {
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
        assert_eq!(app.field_select("unwrapping_algorithm"), 0); // romeo

        // Enter opens the selection modal for this field (does not cycle).
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            app.algo_modal.as_ref().unwrap().target,
            AlgoModalTarget::Select(ref f) if f == "unwrapping_algorithm"
        ));
        assert_eq!(app.field_select("unwrapping_algorithm"), 0); // still unchanged
        // Pick the next option (laplacian) and confirm.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.field_select("unwrapping_algorithm"), 1); // laplacian

        // Focus is restored to the selector after confirming: → changes nothing, Enter reopens it.
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.field_select("unwrapping_algorithm"), 1);
        assert!(app.algo_modal.is_none());
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            app.algo_modal.as_ref().unwrap().target,
            AlgoModalTarget::Select(ref f) if f == "unwrapping_algorithm"
        ));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.field_select("unwrapping_algorithm"), 0);
    }

    #[test]
    fn test_pipeline_edit_text_param() {
        let mut app = App::new();
        app.form.settings.set("pipeline.axial_resampling", true);
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
        for _ in 0..app.field_text("obliquity_threshold").len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        app.handle_key(key(KeyCode::Char('5')));
        assert_eq!(app.field_text("obliquity_threshold"), "5");
        // Escape exits editing
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.editing);
    }

    #[test]
    fn test_pipeline_toggle_do_qsm() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.sub_page[TAB_RECONSTRUCTION] = Some(Page::ComputeQsm);
        app.sync_pipeline_mode();
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
        assert!(app.field_toggle("inhomogeneity_correction"));
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.field_toggle("inhomogeneity_correction"));
    }

    #[test]
    fn test_pipeline_quit_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_pipeline_esc_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn test_pipeline_f5_from_pipeline_tab() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
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
        app.pipeline_state.set_select("bf_algorithm", 3);
        *app.field_text_mut("tikhonov_lambda").unwrap() = "9".into();
        assert!(app.goto_field("qsm_algorithm"));
        app.handle_key(key(KeyCode::Char('R')));
        // The page's settings, the ones a reset reveals among them, and nothing on other pages.
        assert_eq!(QSM_ALGO_OPTIONS[app.pipeline_state.qsm_algorithm], "ilsqr");
        assert_eq!(app.field_text("tikhonov_lambda"), App::new().field_text("tikhonov_lambda"));
        assert_eq!(app.pipeline_state.bf_algorithm, 3, "Background is another page");
    }

    #[test]
    fn test_pipeline_visible_rows_change_on_toggle() {
        let mut app = App::new();
        let rows_with_qsm = app.pipeline_rows().len();
        app.pipeline_state.do_qsm = false;
        let rows_without_qsm = app.pipeline_rows().len();
        assert!(rows_without_qsm < rows_with_qsm);
    }

    #[test]
    fn test_pipeline_editing_left_right_cursor() {
        let mut app = App::new();
        app.form.settings.set("pipeline.axial_resampling", true);
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
        let len = app.field_text("obliquity_threshold").len();
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
        app.active_tab = TAB_RECONSTRUCTION;
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
        assert!(!app.form.settings.toggle("pipeline.do_swi"));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_swi"));
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!app.form.settings.toggle("pipeline.do_swi"));
    }

    #[test]
    fn test_supplementary_toggle_t2star() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_t2starmap");
        assert!(!app.form.settings.toggle("pipeline.do_t2starmap"));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_t2starmap"));
    }

    #[test]
    fn test_supplementary_toggle_r2star() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_r2starmap");
        assert!(!app.form.settings.toggle("pipeline.do_r2starmap"));
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(app.form.settings.toggle("pipeline.do_r2starmap"));
    }

    #[test]
    fn test_supplementary_swi_scaling_select() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.form.settings.set("pipeline.do_swi", true); // make SWI fields visible
        focus_on(&mut app, TAB_RECONSTRUCTION, "swi_scaling");
        let scaling = |app: &App| app.form.settings.get("swi.scaling");
        assert_eq!(scaling(&app), "tanh");
        step(&mut app, 1);
        assert_eq!(scaling(&app), "negative-tanh");
        step(&mut app, -1);
        assert_eq!(scaling(&app), "tanh");
    }

    #[test]
    fn test_supplementary_edit_swi_strength() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.form.settings.set("pipeline.do_swi", true);
        focus_on(&mut app, TAB_RECONSTRUCTION, "swi_strength");
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
        app.active_tab = TAB_RECONSTRUCTION;
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
        // SWI fields are hidden when do_swi is false: Compute SWI is the section's only row.
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field_name().as_deref(), Some("do_swi"));
        app.form.settings.set("pipeline.do_swi", true);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field_name().as_deref(), Some("swi_unwrap"));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field_name().as_deref(), Some("do_swi"));
    }

    #[test]
    fn test_supplementary_swi_visible_when_enabled() {
        let mut app = App::new();
        app.form.settings.set("pipeline.do_swi", true);
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_scaling")); // SWI Scaling
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_strength")); // SWI Strength
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_mip_window")); // SWI MIP Window
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_filtered_phase"));
    }

    /// The filtered phase is ticked under SWI, and then listed for DICOM export.
    #[test]
    fn swi_filtered_phase_is_an_swi_output_and_exportable() {
        let mut app = App::new();
        let phase = dicom_map("dicom_out_swi_phase").unwrap();
        app.form.settings.set("pipeline.do_swi", true);
        assert!(!app.form.settings.toggle("swi.filtered_phase") && !app.dicom_map_made(phase), "off by default");
        app.form.settings.set("swi.filtered_phase", true);
        assert!(app.dicom_map_made(phase));
        app.form.export_dicom = true;
        let export: Vec<&str> = app.page_rows(Page::Export).iter().filter_map(row_field).collect();
        assert!(export.contains(&"dicom_out_swi_phase"), "listed in Maps to export: {export:?}");
        app.form.dicom_unticked = vec!["desc-swi_phase".into()];
        assert!(!app.dicom_outputs().unwrap().iter().any(|m| m == "desc-swi_phase"));
        // Without SWI there is no phase, and the row is hidden.
        app.form.settings.set("pipeline.do_swi", false);
        assert!(!app.dicom_map_made(phase));
        assert!(!shows(&mut app, TAB_RECONSTRUCTION, "swi_filtered_phase"));
    }

    #[test]
    fn test_supplementary_swi_hidden_when_disabled() {
        let mut app = App::new();
        assert!(!shows(&mut app, TAB_RECONSTRUCTION, "swi_scaling"));
        assert!(!shows(&mut app, TAB_RECONSTRUCTION, "swi_mip_window"));
    }

    #[test]
    fn test_supplementary_tab_switch() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_EXECUTION);
    }

    #[test]
    fn test_supplementary_reset_field() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
        app.form.settings.set("pipeline.do_swi", true);
        app.handle_key(key(KeyCode::Char('r')));
        assert!(!app.form.settings.toggle("pipeline.do_swi")); // reset to default
    }

    #[test]
    fn test_supplementary_reset_tab() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.form.settings.set("pipeline.do_swi", true);
        app.form.settings.set("pipeline.do_t2starmap", true);
        app.form.settings.set("pipeline.do_r2starmap", true);
        app.form.settings.set("swi.scaling", "negative");
        app.sub_page[TAB_RECONSTRUCTION] = Some(Page::Swi);
        app.sync_pipeline_mode();
        app.handle_key(key(KeyCode::Char('R')));
        assert!(!app.form.settings.toggle("pipeline.do_swi"));
        assert_eq!(app.form.settings.get("swi.scaling"), "tanh");
        assert!(app.form.settings.toggle("pipeline.do_t2starmap") && app.form.settings.toggle("pipeline.do_r2starmap"), "Relaxometry is another page");
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_t2starmap");
        app.handle_key(key(KeyCode::Char('R')));
        assert!(!app.form.settings.toggle("pipeline.do_t2starmap") && !app.form.settings.toggle("pipeline.do_r2starmap"));
    }

    // ========== Execution tab handler tests ==========

    #[test]
    fn test_execution_mode_left_right_change_nothing() {
        let mut app = App::new();
        app.active_tab = TAB_EXECUTION;
        focus_on(&mut app, TAB_EXECUTION, "execution_mode");
        assert_eq!(app.form.execution_mode, 0); // Local
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.form.execution_mode, 0, "←/→ change no value");
        assert!(app.algo_modal.is_none() && !app.on_subtabs() && !app.tab_focus, "and Execution has no sections to go back to");
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
        // Esc closes it again with nothing changed.
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Esc));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.form.execution_mode, 1); // still SLURM
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
        assert!(app.on_subtabs(), "↑ at the top goes up to the sub-tab strip");
    }

    /// The Methods tab opens on its sidebar with the methods text chosen; the pipeline graph is
    /// the second page, reached with ↓, `]` or `[`, and → goes into it.
    #[test]
    fn the_pipeline_is_the_methods_tabs_second_sub_tab() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('5')));
        assert_eq!(app.active_tab, TAB_METHODS);
        assert_eq!(app.pages(TAB_METHODS), vec![Page::MethodsText, Page::Pipeline]);
        assert_eq!(app.page(), Some(Page::MethodsText));
        assert!(app.on_subtabs() && app.sidebar());
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.page(), Some(Page::Pipeline));
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::Right));
        assert!(!app.on_subtabs());
        app.handle_key(key(KeyCode::Down));
        assert_eq!((app.pipeline_view.scroll, app.methods_scroll_offset), (1, 0), "↓ scrolls the graph");
        app.handle_key(key(KeyCode::Char('[')));
        assert_eq!(app.page(), Some(Page::MethodsText));
        app.handle_key(key(KeyCode::Char(']')));
        assert_eq!(app.page(), Some(Page::Pipeline));
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert!(app.on_subtabs());
        // Up the sidebar to the methods text, then to the tab bar.
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.page(), Some(Page::MethodsText));
        app.handle_key(key(KeyCode::Up));
        assert!(app.tab_focus);
        assert!(!TAB_NAMES.contains(&"Graph"), "the graph is a Methods sub-tab");
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
        // No session, so focus_next from Series(0) goes to the keep-UIDs toggle, then the button
        ds.focus_next();
        assert_eq!(ds.focus, DicomFocus::KeepUids);
        ds.focus_next();
        assert_eq!(ds.focus, DicomFocus::ConvertButton);
        // Back up to the toggle; with no session there is no series above it
        ds.focus_prev();
        assert_eq!(ds.focus, DicomFocus::KeepUids);
        ds.focus_prev();
        assert_eq!(ds.focus, DicomFocus::KeepUids);
    }

    #[test]
    fn test_dicom_navigate_down() {
        let mut app = App::new();
        app.input_mode = InputMode::DicomToBids;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        // No session, so ↓ goes to the keep-UIDs toggle, which Space flips, then the button
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.dicom_state.focus, DicomFocus::KeepUids);
        assert!(!app.dicom_state.keep_uids, "off by default: a fully de-identified dataset");
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(app.dicom_state.keep_uids);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.dicom_state.focus, DicomFocus::ConvertButton);
    }

    /// Enter on a selector row opens its picker; move its cursor `by` and confirm.
    fn step(app: &mut App, by: isize) {
        app.handle_key(key(KeyCode::Enter));
        let m = app.algo_modal.as_mut().expect("Enter opens the picker");
        let n = m.options.len() as isize;
        m.cursor = (m.cursor as isize + by).rem_euclid(n) as usize;
        app.handle_key(key(KeyCode::Enter));
    }

    /// Enter on a mask row's number, the text replaced by `text`, Enter.
    fn type_mask_value(app: &mut App, text: &str) {
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.mask_value_editing, "Enter edits the number");
        app.handle_key(key(KeyCode::End));
        while !app.pipeline_state.mask_value_buf.is_empty() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
    }

    // ========== IO select (input mode picker) ==========

    /// The input mode is chosen from its picker (Enter); ←/→ neither cycle it nor open it.
    #[test]
    fn test_left_right_leave_the_io_mode_alone() {
        let mut app = App::new();
        app.active_tab = TAB_INPUT;
        app.active_field = 0;
        assert_eq!(app.input_mode, InputMode::Bids);
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.input_mode, InputMode::Bids, "not cycled");
        assert!(app.algo_modal.is_none());
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
        // Esc closes it with nothing changed.
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Esc));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.input_mode, InputMode::NIfTI);
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
        assert!(app.error_message.as_deref().unwrap().contains("BIDS directory"));
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
        assert!(app.error_message.as_deref().unwrap().contains("SLURM account"));
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
        app.active_tab = TAB_RECONSTRUCTION;
        app.form.settings.set("pipeline.do_swi", true);
        focus_on(&mut app, TAB_RECONSTRUCTION, "swi_strength");
        *app.field_text_mut("swi_strength").unwrap() = "999".to_string();
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.field_text("swi_strength"), RunForm::default().settings.text("swi.strength"));
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
        step(&mut app, 1);
        assert_eq!(app.pipeline_state.mask_preset, 1); // BET
        step(&mut app, 1);
        assert_eq!(app.pipeline_state.mask_preset, 2); // HD-BET + signal-gated erosion
        assert_eq!(app.mask().sections, crate::pipeline::config::hd_bet_mask_sections());
        step(&mut app, 1);
        assert_eq!(app.pipeline_state.mask_preset, 3); // BET AND phase quality
        let recipe = crate::pipeline::config::bet_and_phase_mask_recipe();
        assert_eq!(app.mask().sections, recipe.sections);
        assert_eq!(app.mask().combine, recipe.combine);
        assert_eq!(app.mask().refinements, recipe.refinements);
        step(&mut app, 1);
        assert_eq!(app.pipeline_state.mask_preset, 4); // RS2-Net (rodent)
        assert_eq!(app.mask().sections, crate::pipeline::config::rs2_net_mask_sections());
        // Leaving the two-section preset resets the combine mode and the combined-mask steps.
        assert_eq!(app.mask().combine, crate::pipeline::config::MaskCombine::Or);
        assert!(app.mask().refinements.is_empty());
        step(&mut app, 1);
        assert_eq!(app.pipeline_state.mask_preset, 5); // MindGrab
        assert_eq!(app.mask().sections, crate::pipeline::config::mindgrab_mask_sections());
        step(&mut app, 1);
        assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM, "the picker lists 'custom' last");
    }

    #[test]
    fn test_mask_threshold_method_modal_opens_and_sets() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.sync_pipeline_mode();
        // Default preset is a threshold generator, so the param row is the method select.
        assert!(matches!(
            app.mask().sections[0].generator,
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
        assert_eq!(modal.options.len(), 4);
        // Pick "percentile" (index 2) and confirm.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert!(matches!(
            app.mask().sections[0].generator,
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
        app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
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
        assert_eq!(&MASK_PRESET_OPTIONS[..*MASK_PRESET_CUSTOM], &names[..]);
        assert_eq!(MASK_PRESET_OPTIONS[*MASK_PRESET_CUSTOM], "custom");
        assert_eq!(MASK_PRESET_OPTIONS.len(), mask_preset_help().len());

        let cli_names: Vec<String> = crate::cli::MaskPresetArg::value_variants().iter()
            .filter_map(|v| v.to_possible_value()).filter(|v| !v.is_hide_set())
            .map(|v| v.get_name().to_string()).collect();
        assert_eq!(cli_names, names, "--mask-preset values drifted from mask_presets()");
    }

    /// Every signal-erode parameter is reachable and typed (Enter) on the pipeline form,
    /// and what the CLI preview says is what the rows show.
    #[test]
    fn test_pipeline_signal_erode_params_are_editable() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.apply_mask_preset(2); // hd-bet + signal-erode

        let focus_param = |app: &mut App, param: usize| {
            let rows = app.rows();
            app.pipeline_state.focus = app.focusable_rows().iter()
                .position(|&ri| matches!(rows[ri], PipelineRow::MaskOpSignalErodeParam { param: p, .. } if p == param))
                .unwrap_or_else(|| panic!("param {param} row not focusable"));
        };
        let op = |app: &App| app.mask().sections[0].refinements[0].clone();
        let MaskOp::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } = op(&app)
            else { panic!("preset should end in signal-erode") };

        // The step row itself is the threshold; the four params each have their own row.
        let rows = app.rows();
        app.pipeline_state.focus = app.focusable_rows().iter()
            .position(|&ri| matches!(rows[ri], PipelineRow::MaskOpEntry { .. })).expect("step row");
        type_mask_value(&mut app, "0.85");
        assert!(matches!(op(&app), MaskOp::SignalErode { threshold: t, .. } if t == 0.85 && t != threshold));

        for (param, typed, expect) in [
            (0usize, "7", "depth cap"), (1, "4", "plain erosions"), (2, "9.5", "bias sigma"), (3, "321", "min component"),
        ] {
            focus_param(&mut app, param);
            let before = op(&app);
            app.handle_key(key(KeyCode::Right));
            assert_eq!(op(&app), before, "→ changes nothing");
            type_mask_value(&mut app, typed);
            let MaskOp::SignalErode { depth_cap: d, global_erosions: g, bias_sigma: b, min_component: m, .. } = op(&app)
                else { panic!("still signal-erode") };
            let now = match param { 0 => d.to_string(), 1 => g.to_string(), 2 => b.to_string(), _ => m.to_string() };
            assert_eq!(now, typed, "{expect}");
            // And back.
            let back = match param { 0 => depth_cap.to_string(), 1 => global_erosions.to_string(), 2 => bias_sigma.to_string(), _ => min_component.to_string() };
            type_mask_value(&mut app, &back);
        }

        let MaskOp::SignalErode { depth_cap: d, global_erosions: g, bias_sigma: b, min_component: m, .. } = op(&app)
            else { panic!("still signal-erode") };
        assert_eq!((d, g, b, m), (depth_cap, global_erosions, bias_sigma, min_component), "edits round-trip");

        // A count is a whole number of at least zero: anything else is refused and kept for
        // correcting, and Esc leaves the setting as it was.
        focus_param(&mut app, 0);
        type_mask_value(&mut app, "-1");
        assert!(app.pipeline_state.mask_value_editing && app.error_message.is_some(), "refused, still editing");
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.mask_value_editing);
        assert!(matches!(op(&app), MaskOp::SignalErode { depth_cap: d, .. } if d == depth_cap));
        type_mask_value(&mut app, "0");
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
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "qsm_reference"));
        let copies = app.all_pages().iter().filter(|(_, _, rows)| rows.iter().any(|r| row_field(r) == Some("qsm_reference"))).count();
        assert_eq!(copies, 1, "one copy only");
        focus_on(&mut app, TAB_RECONSTRUCTION, "qsm_reference");
        app.set_field_select("qsm_reference", thalamus);
        // A region needs segmentation: the run makes one, so its settings show, but the checkbox
        // reads as the user left it until the required change is applied.
        assert!(!app.field_toggle("do_segmentation") && app.output_made("do_segmentation"));
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "synthseg_version"));
    }

    #[test]
    fn a_region_reference_needs_segmentation_visibly() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        assert!(!app.field_toggle("do_segmentation"), "off by default");

        let region = qsm_ref_options().iter().position(|o| *o == "ventricles").unwrap();
        app.set_field_select("qsm_reference", region);
        assert!(!app.field_toggle("do_segmentation"), "the checkbox reads as the user left it");
        assert!(app.output_made("do_segmentation"), "a region reference needs SynthSeg, which the run makes");
        for f in SYNTHSEG_ROWS {
            assert!(shows(&mut app, TAB_RECONSTRUCTION, f),
                    "{f} shapes the parcellation the reference is measured on");
        }

        // Ticking it is the required change; unticking it again is refused, and says why.
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_segmentation");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_segmentation"), "{:?}", app.error_message);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_segmentation"), "turning it off is refused");
        let msg = app.error_message.clone().expect("a refusal should explain itself");
        assert!(msg.contains("Reference = ventricles") && !msg.contains("--"), "the refusal names the setting: {msg}");

        // `mean` and `none` need nothing.
        app.form.settings.set("pipeline.do_segmentation", false);
        for idx in [0, 1] {
            app.set_field_select("qsm_reference", idx);
            assert!(!app.output_made("do_segmentation"), "option {idx} needs nothing");
        }
    }

    /// The generated command carries the region as the reference value itself.
    #[test]
    fn a_region_reference_reaches_the_command() {
        let mut app = App::new();
        let idx = qsm_ref_options().iter().position(|o| *o == "left-thalamus").unwrap();
        app.set_field_select("qsm_reference", idx);
        let cmd = crate::tui::command::build_command_string(&app);
        assert!(cmd.contains("--qsm-reference left-thalamus"), "cmd: {cmd}");

        // The default stays off the command line.
        app.set_field_select("qsm_reference", 0);
        assert!(!crate::tui::command::build_command_string(&app).contains("--qsm-reference"));
    }

    /// The per-structure statistics force segmentation on, so the checkbox has to read as checked
    /// and its settings has to be reachable — otherwise the run segments while the TUI says it will
    /// not, and the SynthSeg parameters that shape that segmentation cannot be seen or edited.
    #[test]
    fn statistics_force_segmentation_visibly() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;

        // Off by default: unchecked, and its settings hidden.
        assert!(!app.field_toggle("do_segmentation"));
        for f in SYNTHSEG_ROWS {
            assert!(!shows(&mut app, TAB_RECONSTRUCTION, f), "{f} should be hidden");
        }
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "do_analysis"), "the statistics are reachable on their own");

        // A config or CLI run can arrive with the statistics already on; segmentation is then in
        // force, so the block must be visible rather than hiding a setting that is taking effect.
        app.form.settings.set("pipeline.do_analysis", true);
        assert!(!app.field_toggle("do_segmentation") && app.output_made("do_segmentation"),
                "segmentation is needed: made by the run, but not shown as ticked");
        for f in SYNTHSEG_ROWS {
            assert!(shows(&mut app, TAB_RECONSTRUCTION, f),
                    "{f} shapes the segmentation the run makes and must be reachable");
        }

        // Ticking it is the required change; it then cannot be turned off from under them.
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_segmentation");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_segmentation"));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_segmentation"), "turning it off is refused");
        let msg = app.error_message.clone().expect("a refusal should explain itself");
        assert!(msg.contains("per-structure statistics"), "unhelpful message: {msg}");

        // With the statistics off again it is an ordinary toggle.
        app.form.settings.set("pipeline.do_analysis", false);
        app.error_message = None;
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.form.settings.toggle("pipeline.do_segmentation"), "{:?}", app.error_message);
    }

    /// Selecting HEIDI shows both parameter groups (its seed is an LSQR solve); selecting plain
    /// LSQR shows only its own. The nested rows appear and disappear with their switches.
    #[test]
    fn heidi_shows_the_lsqr_group_and_its_own() {
        let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name)
            .unwrap_or_else(|| panic!("{name} missing from QSM_ALGO_OPTIONS"));
        let fields = |app: &App| -> Vec<String> {
            app.page_rows(Page::Inversion).iter().filter_map(|r| match r {
                PipelineRow::Param { field, .. } | PipelineRow::Toggle { field, .. } => Some(field.to_string()),
                _ => None,
            }).collect()
        };
        let with = |alg: &str, heidi: bool| {
            let mut app = App::new();
            (app.pipeline_state.qsm_algorithm, app.pipeline_state.heidi_enabled) = (idx(alg), heidi);
            app
        };

        let mut app = with("lsqr", false);
        let f = fields(&app);
        assert!(f.contains(&"lsqr_tol".to_string()), "{f:?}");
        assert!(f.contains(&"lsqr_fit_global_offset".to_string()), "{f:?}");
        assert!(f.contains(&"heidi_enabled".to_string()), "the HEIDI checkbox: {f:?}");
        assert!(!f.iter().any(|x| x.starts_with("heidi_") && x != "heidi_enabled"),
            "plain LSQR must not show HEIDI's settings: {f:?}");

        // Ticked after another inversion, HEIDI's settings follow.
        assert!(fields(&with("tkd", true)).contains(&"heidi_cone_threshold".to_string()));
        assert!(!fields(&with("tgv", false)).contains(&"heidi_enabled".to_string()), "not offered without a local field");

        // LSQR+HEIDI: LSQR with the checkbox ticked.
        app.pipeline_state.heidi_enabled = true;
        let f = fields(&app);
        assert!(f.contains(&"lsqr_tol".to_string()), "HEIDI seeds with LSQR: {f:?}");
        assert!(f.contains(&"heidi_cone_threshold".to_string()), "{f:?}");
        assert!(f.contains(&"heidi_continuation_steps".to_string()), "{f:?}");

        // The sub-parameters follow their switches.
        assert!(f.contains(&"heidi_denoise_iterations".to_string()), "denoise is on by default: {f:?}");
        assert!(f.contains(&"heidi_laplacian_threshold".to_string()), "{f:?}");
        app.toggle_field("heidi_denoise");
        app.toggle_field("heidi_apply_laplacian_correction");
        let f = fields(&app);
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
            // Background removal's and field mapping's rows come from the registry; their pages
            // say when they do not run.
            let page = match id { "bgremoval" => Page::Background, "fieldmap" => Page::FieldMap, _ => unreachable!("{id}") };
            PipelineFormState { page, ..ps.clone() }.page_notes().is_none()
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
        // LSQR (and LSQR+HEIDI) is an ordinary local-field inversion.
        ps.qsm_algorithm = idx("lsqr");
        ps.heidi_enabled = true;
        assert!(has_section(&ps, "bgremoval"), "LSQR inverts the local field");
        assert!(has_section(&ps, "fieldmap"), "LSQR needs a field map");
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
        // The QSM sections stay listed but hold only a note.
        for gone in ["unwrapping_algorithm", "bf_algorithm", "qsm_algorithm", "qsm_reference", "crop_to_mask", "fft_padding"] {
            assert!(!f.contains(&gone.to_string()), "{gone} shown with QSM off: {f:?}");
        }
        // Motion correction's own settings follow it, not QSM.
        app.toggle_field("motion_correction");
        let f = fields(&mut app);
        assert!(f.contains(&"motion_reference".to_string()) && f.contains(&"reg_levels".to_string()), "{f:?}");

        // SMWI needs a χ map: QSM runs regardless, and its settings are on show, with the reason.
        app.form.settings.set("pipeline.do_smwi", true);
        let f = fields(&mut app);
        assert!(f.contains(&"qsm_algorithm".to_string()) && f.contains(&"unwrapping_algorithm".to_string()), "{f:?}");
        assert!(app.pipeline_rows().iter().any(|r|
            matches!(r, PipelineRow::Note { text } if text.contains("SMWI"))), "the forced QSM says why");
    }

    /// Bipolar correction is part of HIP phase offset removal, which runs before any unwrapper.
    #[test]
    fn bipolar_correction_needs_offset_removal() {
        let mut app = App::new();
        let shows = |app: &App| app.pipeline_rows().iter().any(|r|
            matches!(r, PipelineRow::Toggle { field: "bipolar_correction", .. }));
        app.toggle_field("bipolar_correction");
        let passed = |app: &App| super::super::command::config_from_app(app).unwrap().field_mapping.bipolar_correction;
        assert!(app.field_toggle("phase_offset_removal") && app.field_select("unwrapping_algorithm") == 0);
        assert!(shows(&app) && passed(&app));
        app.toggle_field("phase_offset_removal");
        assert!(!shows(&app) && !passed(&app), "no offset removal, no bipolar correction");
        app.toggle_field("phase_offset_removal");
        app.set_field_select("unwrapping_algorithm", 1);
        assert!(shows(&app) && passed(&app), "Laplacian unwrapping removes offsets first too");
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

        app.form.settings.set("pipeline.do_r2primemap", true);
        let f = fields(&mut app);
        assert!(has(&f, "custom_r2prime_tool") && has(&f, "custom_r2_tool") && has(&f, "r2prime_strategy"), "{f:?}");
        assert!(!has(&f, "custom_qsm_tool"), "{f:?}");
        // R2PRIMEnet predicts R2' from R2*: no R2 is read.
        app.set_field_select("r2prime_strategy", 2);
        assert!(!has(&fields(&mut app), "custom_r2_tool"));
        // A supplied R2' replaces the computation: its source no longer applies, its row stays.
        app.set_field_select("r2prime_strategy", 0);
        *app.field_text_mut("custom_r2prime_tool").unwrap() = "*".into();
        let f = fields(&mut app);
        assert!(has(&f, "custom_r2prime_tool") && !has(&f, "r2prime_strategy") && !has(&f, "custom_r2_tool"), "{f:?}");

        let mut app = App::new();
        app.form.settings.set("pipeline.do_analysis", true);
        assert!(has(&fields(&mut app), "custom_qsm_tool"), "the statistics can read a supplied χ map");
        let mut app = App::new();
        app.form.settings.set("pipeline.do_r2map", true);
        let f = fields(&mut app);
        assert!(has(&f, "custom_r2_tool") && !has(&f, "custom_r2prime_tool"), "{f:?}");
    }

    /// A supplied segmentation satisfies the statistics and a region reference: SynthSeg is not
    /// forced, so its checkbox reads as the user left it and can be toggled.
    #[test]
    fn a_custom_dseg_is_the_segmentation_source() {
        let mut app = App::new();
        app.form.settings.set("pipeline.do_analysis", true);
        assert!(app.output_made("do_segmentation"), "needed without a supplied dseg");
        assert_eq!(app.field_select("segmentation_source"), 0, "SynthSeg");
        *app.field_text_mut("custom_dseg_tool").unwrap() = "freesurfer".into();
        assert!(app.field_toggle("do_segmentation"), "a supplied segmentation is a segmentation");
        assert_eq!(app.field_select("segmentation_source"), 1);
        assert!(!shows(&mut app, TAB_RECONSTRUCTION, "synthseg_version"), "no SynthSeg settings to show");
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "custom_dseg_tool"));
        assert!(shows(&mut app, TAB_RECONSTRUCTION, "do_analysis"), "the statistics stay reachable");
        // The statistics need one, whichever the source: turning it off is refused.
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_segmentation");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.error_message.is_some() && app.field_text("custom_dseg_tool") == "freesurfer");
        // Back to SynthSeg: the supplied one is dropped, SynthSeg asked for.
        app.set_field_select("segmentation_source", 0);
        assert!(app.field_text("custom_dseg_tool").is_empty() && app.form.settings.toggle("pipeline.do_segmentation"));
        app.set_field_select("segmentation_source", 1);
        assert_eq!(app.field_text("custom_dseg_tool"), "*");
        assert!(!app.form.settings.toggle("pipeline.do_segmentation"), "one representation per choice");
        // Unforced: off clears whichever source.
        app.form.settings.set("pipeline.do_analysis", false);
        app.toggle_field("do_segmentation");
        assert!(!app.field_toggle("do_segmentation") && app.field_text("custom_dseg_tool").is_empty());
    }

    /// R2* is needed by what reads it (R2*-QSM, a computed R2'): made by the run, shown as the
    /// user left it, and once ticked it cannot be turned off from under them.
    #[test]
    fn r2star_is_made_when_needed_and_reads_as_set() {
        let mut app = App::new();
        assert!(!app.field_toggle("do_r2starmap"));
        app.form.settings.set("pipeline.do_r2primemap", true);
        assert!(!app.field_toggle("do_r2starmap") && app.output_made("do_r2starmap"), "R2' = R2* - R2");
        app.active_tab = TAB_RECONSTRUCTION;
        focus_on(&mut app, TAB_RECONSTRUCTION, "do_r2starmap");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_r2starmap"), "{:?}", app.error_message);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.form.settings.toggle("pipeline.do_r2starmap") && app.error_message.as_deref().is_some_and(|m| m.contains("R2′ map")),
                "{:?}", app.error_message);
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
        assert!(app.field_toggle("two_pass"));
        assert!(reliable_rows(&app) > 0, "the reliable mask editor appears with the toggle");

        // It starts as the default recipe: an unfilled Otsu threshold on phase quality.
        let sections = &app.mask().two_pass_sections;
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
        app.form.settings.set("masking.two_pass", true);
        let main_before = app.mask().sections.clone();

        let rows = app.rows();
        let focus = app.focusable_rows().iter()
            .position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpInput { section } if is_two_pass_section(*section)))
            .expect("reliable input row not focusable");
        app.pipeline_state.focus = focus;
        step(&mut app, 1);

        assert_ne!(app.mask().two_pass_sections[0].input, MaskingInput::PhaseQuality,
                   "the reliable mask's input should have changed");
        assert_eq!(app.mask().sections, main_before, "the main mask must be untouched");
        assert_eq!(app.pipeline_state.mask_preset, 0, "the main mask still follows its preset");

        // Edited away from the default, so the command now spells the reliable mask out.
        let cmd = crate::tui::command::build_command_string(&app);
        assert!(cmd.contains("--two-pass-mask"), "cmd: {cmd}");
    }

    /// Under QSMART the default mask stays as it is, with no warning of QSMxT's own: QSMART's
    /// reference mask is the rules' to point out (the mask row's ⓘ). Resetting the preset brings
    /// the default back.
    #[test]
    fn qsmart_keeps_the_mask() {
        let qsmart = QSM_PICKER_OPTIONS.iter().position(|a| *a == "qsmart").unwrap();
        let mut app = App::new();
        let before = app.mask().sections.clone();
        app.pipeline_state.set_select("qsm_algorithm", qsmart);
        assert_eq!(app.mask().sections, before, "choosing QSMART changes no mask");
        assert!(!app.pipeline_rows().iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("brain mask"))));
        app.set_field_select("mask_preset", 1);
        app.reset_field("mask_preset");
        assert_eq!(app.mask().sections, before);
    }

    /// Turning two-pass off hides the editor but keeps what was typed into it, so toggling back
    /// on does not silently discard the recipe.
    #[test]
    fn two_pass_remembers_its_mask_while_off() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let mut app = App::new();
        app.form.settings.set("masking.two_pass", true);
        app.mask_mut().two_pass_sections[0].generator =
            MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(40.0) };

        app.toggle_field("two_pass");
        assert!(!app.field_toggle("two_pass"));
        // Off, so it contributes nothing to the config...
        let config = crate::tui::command::config_from_app(&app).unwrap();
        assert!(!config.masking.two_pass);
        assert!(config.masking.two_pass_sections.is_none());
        // ...but the edit survives.
        assert!(matches!(app.mask().two_pass_sections[0].generator,
                         MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. }));
    }

    /// The combine row only appears once there is more than one section, and its OR / AND is
    /// chosen from a list.
    #[test]
    fn test_pipeline_mask_combine_row_toggles() {
        use crate::pipeline::config::MaskCombine;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);

        let has_combine_row = |app: &App| app.rows().iter()
            .any(|r| matches!(r, PipelineRow::MaskCombineOp { .. }));
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

        app.pipeline_state.focus = focus_on(&app, |r| matches!(r, PipelineRow::MaskCombineOp { .. }))
            .expect("MaskCombineOp not focusable");
        assert_eq!(app.mask().combine, MaskCombine::Or);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.mask().combine, MaskCombine::Or, "→ changes nothing");
        // Enter opens OR / AND with OR highlighted; Esc keeps it.
        app.handle_key(key(KeyCode::Enter));
        let m = app.algo_modal.as_ref().expect("the combine list");
        assert_eq!((m.options.clone(), m.cursor), (vec!["OR".to_string(), "AND".to_string()], 0));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mask().combine, MaskCombine::Or);
        step(&mut app, 1);
        assert_eq!(app.mask().combine, MaskCombine::And);
        assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM, "editing the recipe marks it custom");
        step(&mut app, 1);
        assert_eq!(app.mask().combine, MaskCombine::Or);
    }

    /// With two-pass on, the reliable mask's "Combined with" row is its own control: ←/→ toggle
    /// its mode, which starts as the main mask's and follows it until set, and leaves the main
    /// mask (and its preset) alone. The run gets it as `--two-pass-mask-combine`.
    #[test]
    fn the_reliable_mask_has_its_own_combine_mode() {
        use crate::pipeline::config::MaskCombine;
        let mut app = App::new();
        app.form.bids_dir = "/bids".into();
        goto_page(&mut app, Page::Masking);
        app.form.settings.set("masking.two_pass", true);
        let extra = app.mask().two_pass_sections[0].clone();
        app.mask_mut().two_pass_sections.push(extra);
        let main = app.mask().sections[0].clone();
        app.mask_mut().sections.push(main);
        let rows = app.rows();
        let reliable = MASK_TWO_PASS_BASE + 1;
        assert!(rows.contains(&PipelineRow::MaskCombineOp { section: 1 }));
        assert!(rows.contains(&PipelineRow::MaskCombineOp { section: reliable }));
        assert!(!rows.iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("combined with"))));

        app.set_combine(1, MaskCombine::And);
        let m = app.mask();
        assert_eq!((m.combine, m.two_pass_combine_mode(), m.two_pass_combine), (MaskCombine::And, MaskCombine::And, None),
                   "unset, the reliable mask follows the main mask");
        let preset = app.pipeline_state.mask_preset;
        app.pipeline_state.focus = app.focusable_rows().iter()
            .position(|&ri| app.rows()[ri] == PipelineRow::MaskCombineOp { section: reliable }).unwrap();
        step(&mut app, -1);
        let m = app.mask();
        assert_eq!((m.combine, m.two_pass_combine), (MaskCombine::And, Some(MaskCombine::Or)));
        assert_eq!(app.pipeline_state.mask_preset, preset, "the reliable mask is not the main mask's preset");
        let config = crate::tui::command::config_from_app(&app).unwrap();
        assert_eq!((config.masking.combine, config.masking.resolved_two_pass_combine()), (MaskCombine::And, MaskCombine::Or));
        assert!(crate::tui::command::build_command_string(&app).contains("--two-pass-mask-combine or"));
        step(&mut app, 1);
        assert_eq!(app.mask().two_pass_combine, Some(MaskCombine::And));
        app.handle_key(key(KeyCode::Char('R')));
        assert_eq!(app.mask().two_pass_combine, None, "R resets it with the rest of the recipe");
    }

    /// `d` removes the section a row belongs to — from its "Combined with" row (the section after
    /// it) or any of its input and generator rows — in the main and the reliable mask alike, and
    /// says so in the footer. A refinement step's rows remove only the step, and a mask's only
    /// section stays.
    #[test]
    fn d_removes_the_section_a_row_belongs_to() {
        use crate::pipeline::config::{MaskOp, MaskingInput};
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.form.settings.set("masking.two_pass", true);
        for two_pass in [false, true] {
            let base = if two_pass { MASK_TWO_PASS_BASE } else { 0 };
            let sections = |app: &App| if two_pass { app.mask().two_pass_sections.clone() } else { app.mask().sections.clone() };
            let set = |app: &mut App, s: Vec<crate::pipeline::config::MaskSection>| {
                if two_pass { app.mask_mut().two_pass_sections = s } else { app.mask_mut().sections = s }
            };
            let press_d_on = |app: &mut App, row: PipelineRow| {
                let rows = app.rows();
                app.pipeline_state.focus = app.focusable_rows().iter().position(|&ri| rows[ri] == row)
                    .unwrap_or_else(|| panic!("{row:?} not shown"));
                app.handle_key(key(KeyCode::Char('d')));
            };
            let mut first = sections(&app)[0].clone();
            first.refinements = vec![MaskOp::Erode { iterations: 1 }];
            let mut second = first.clone();
            second.input = MaskingInput::MagnitudeLast;
            let mut third = first.clone();
            third.input = MaskingInput::MagnitudeFirst;

            // The "Combined with" row removes the section after it.
            set(&mut app, vec![first.clone(), second.clone(), third.clone()]);
            assert!(app.mask().row_removes_section(&PipelineRow::MaskCombineOp { section: base + 1 }));
            press_d_on(&mut app, PipelineRow::MaskCombineOp { section: base + 1 });
            assert_eq!(sections(&app), vec![first.clone(), third.clone()], "two_pass {two_pass}");
            // A generator-setting row removes its section.
            set(&mut app, vec![first.clone(), second.clone(), third.clone()]);
            press_d_on(&mut app, PipelineRow::MaskOpGeneratorParam { section: base + 2 });
            assert_eq!(sections(&app), vec![first.clone(), second.clone()]);
            // A refinement step's row removes the step only.
            press_d_on(&mut app, PipelineRow::MaskOpEntry { section: base + 1, index: 0 });
            let mut bare = second.clone();
            bare.refinements.clear();
            assert_eq!(sections(&app), vec![first.clone(), bare]);
            // The footer offers it where it applies.
            let rows = app.rows();
            app.pipeline_state.focus = app.focusable_rows().iter()
                .position(|&ri| rows[ri] == PipelineRow::MaskOpInput { section: base + 1 }).unwrap();
            let footer = crate::tui::ui::tests::footer(&mut app);
            assert!(footer.contains("d:Remove section"), "{footer}");
            press_d_on(&mut app, PipelineRow::MaskOpInput { section: base + 1 });
            // The only section stays, and the footer does not offer it.
            assert_eq!(sections(&app), vec![first.clone()]);
            assert!(!app.mask().row_removes_section(&PipelineRow::MaskOpGenerator { section: base }));
            press_d_on(&mut app, PipelineRow::MaskOpGenerator { section: base });
            assert_eq!(sections(&app), vec![first.clone()]);
            assert!(!crate::tui::ui::tests::footer(&mut app).contains("Remove section"));
        }
    }

    /// Steps added under "On the combined mask" land in the combined list, not in a section.
    #[test]
    fn test_pipeline_mask_combined_refinement_steps() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.apply_mask_preset(3); // bet-and-phase: two sections
        let recipe = crate::pipeline::config::bet_and_phase_mask_recipe();
        assert_eq!(app.mask().refinements, recipe.refinements);

        let section_refinements: usize = app.mask().sections.iter().map(|s| s.refinements.len()).sum();
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| {
            matches!(rows[ri], PipelineRow::MaskOpAddStep { section: MASK_COMBINED_SECTION })
        }).expect("combined-mask add-step row not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Enter)); // open the type selector
        app.handle_key(key(KeyCode::Enter)); // add the selected type
        assert_eq!(app.mask().refinements.len(), recipe.refinements.len() + 1);
        assert_eq!(app.mask().sections.iter().map(|s| s.refinements.len()).sum::<usize>(),
                   section_refinements, "the step did not land in a section");

        // ...and `d` deletes from the same list.
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| {
            matches!(rows[ri], PipelineRow::MaskOpEntry { section: MASK_COMBINED_SECTION, index: 0 })
        }).expect("combined-mask step row not found");
        app.pipeline_state.focus = fi;
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.mask().refinements.len(), recipe.refinements.len());
    }

    #[test]
    fn test_pipeline_mask_add_section() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        let initial_sections = app.mask().sections.len();
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
        assert_eq!(app.mask().sections.len(), initial_sections + 1);
    }

    #[test]
    fn test_pipeline_mask_add_step() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        let initial_refs = app.mask().sections[0].refinements.len();
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
        // Enter opens the list of steps; Enter adds the highlighted one.
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.algo_modal.as_ref().map(|m| &m.target), Some(AlgoModalTarget::MaskAddStep(0))));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.mask().sections[0].refinements.len(), initial_refs + 1);
    }

    #[test]
    fn test_pipeline_mask_add_step_chooses_from_a_list() {
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
        let available = MaskRecipe::available_op_types();
        let before = app.mask().sections[0].refinements.clone();
        // Esc closes the list and adds nothing.
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Esc));
        assert!(app.algo_modal.is_none());
        assert_eq!(app.mask().sections[0].refinements, before);
        // The second type, chosen.
        app.handle_key(key(KeyCode::Enter));
        let m = app.algo_modal.as_ref().unwrap();
        assert_eq!(m.options, available.iter().map(|t| t.to_string()).collect::<Vec<_>>());
        assert_eq!(m.cursor, 0);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        let mask = app.mask();
        let added = mask.sections[0].refinements.last().unwrap();
        assert_eq!(MaskRecipe::mask_op_label_value(added).0, available[1]);
        assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM);
    }

    #[test]
    fn test_pipeline_mask_delete_refinement() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        let initial_refs = app.mask().sections[0].refinements.len();
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
        assert_eq!(app.mask().sections[0].refinements.len(), initial_refs - 1);
    }

    #[test]
    fn test_pipeline_mask_op_left_right_change_nothing() {
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
        let before = app.mask().sections[0].refinements.clone();
        // →: nothing changes; ←: back to the sidebar.
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.mask().sections[0].refinements, before);
        app.handle_key(key(KeyCode::Left));
        assert!(app.subtab_focus, "← goes back to the sections");
        assert_eq!(app.mask().sections[0].refinements, before);
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
        step(&mut app, 1);
        assert!(matches!(
            app.mask().sections[0].generator,
            crate::pipeline::config::MaskOp::Bet { .. }
        ));
        step(&mut app, 1);
        assert!(matches!(
            app.mask().sections[0].generator,
            crate::pipeline::config::MaskOp::HdBet { .. }
        ));
        step(&mut app, 1);
        assert!(matches!(
            app.mask().sections[0].generator,
            crate::pipeline::config::MaskOp::MindGrab { tta: false }
        ));
        step(&mut app, 1);
        assert!(matches!(
            app.mask().sections[0].generator,
            crate::pipeline::config::MaskOp::Rs2Net { .. }
        ));
        // Wraps back round to the first generator.
        step(&mut app, 1);
        assert!(matches!(
            app.mask().sections[0].generator,
            crate::pipeline::config::MaskOp::Threshold { .. }
        ));
        step(&mut app, -1);
        assert!(matches!(
            app.mask().sections[0].generator,
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
        // The threshold method, chosen from its list.
        step(&mut app, 1);
        // Verify the method changed (Otsu -> Fixed)
        if let crate::pipeline::config::MaskOp::Threshold { method, .. } = &app.mask().sections[0].generator {
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
        let before = app.mask().sections[0].input;
        step(&mut app, 1);
        // Should have cycled to a different input
        assert_ne!(app.mask().sections[0].input, before);
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
        app.form.settings.set("pipeline.axial_resampling", true);
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
        *app.field_text_mut("obliquity_threshold").unwrap() = "999".to_string();
        // Reset
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.field_text("obliquity_threshold"), App::new().field_text("obliquity_threshold"));
    }

    #[test]
    fn test_reset_pipeline_toggle_field() {
        let mut app = App::new();
        app.active_tab = TAB_RECONSTRUCTION;
        app.sub_page[TAB_RECONSTRUCTION] = Some(Page::ComputeQsm);
        app.sync_pipeline_mode();
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
        app.form.settings.set("pipeline.do_swi", true);
        // One row for the three axes.
        focus_on(&mut app, TAB_RECONSTRUCTION, "swi_hp_sigma");
        let sigma = |app: &App| super::super::command::config_from_app(app).unwrap().swi.hp_sigma;
        assert_eq!(sigma(&app), qsmxt_engine::SwiConfig::default().hp_sigma, "the default parses");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        for _ in 0..20 { app.handle_key(key(KeyCode::Backspace)); }
        for c in "4, 4 0".chars() { app.handle_key(key(KeyCode::Char(c))); }
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.editing);
        assert_eq!(sigma(&app), [4.0, 4.0, 0.0]);
        *app.field_text_mut("swi_hp_sigma").unwrap() = "3 3".into();
        assert!(super::super::command::config_from_app(&app).is_err(), "three values or none: two are refused");
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
        let mut app = App::new();
        // Switch through EVERY QSM algorithm (incl. the deep-learning ones: single-step
        // NeXtQSM/AutoQSM hide BG removal; end-to-end iQSM/iQSM+ hide field mapping too).
        for (algo, name) in QSM_ALGO_OPTIONS.iter().enumerate() {
            app.pipeline_state.qsm_algorithm = algo;
            let rows = app.page_rows(Page::Inversion);
            assert!(!focusable_indices(&rows).is_empty(), "{name}");
        }
    }

    #[test]
    fn test_pipeline_separation_algorithm_all_params() {
        let mut app = App::new();
        app.form.settings.set("pipeline.do_chi_separation", true);
        // Every separation method incl. SUSEP-Net / χ-sepnet.
        for algo in 0..super::super::settings_rows::options("separation.algorithm").0.len() {
            app.set_field_select("separation_algorithm", algo);
            assert!(app.page_rows(Page::Separation).len() > 3);
        }
    }

    #[test]
    fn test_pipeline_medi_smv_shows_bg_removal() {
        let ps = PipelineFormState {
            qsm_algorithm: 7,
            medi_smv: true,
            ..Default::default()
        };
        // MEDI takes the local field with every SMV option, so background removal runs.
        assert!(PipelineFormState { page: Page::Background, ..ps }.page_notes().is_none(),
                "MEDI+SMV takes the local field from background removal");
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
        app.mask_mut().sections[0].generator = crate::pipeline::config::MaskOp::Threshold {
            method: crate::pipeline::config::MaskThresholdMethod::Fixed,
            value: Some(0.5),
        };
        app.mark_mask_custom(0);

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
        assert!(app.pipeline_state.mask_value_editing);
        // Type a new value -- clear via backspace from end
        let buf_len = app.pipeline_state.mask_value_buf.len();
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
        assert!(!app.pipeline_state.mask_value_editing);
        if let crate::pipeline::config::MaskOp::Threshold { value, .. } = &app.mask().sections[0].generator {
            assert!((value.unwrap() - 0.7).abs() < 0.001);
        }
    }

    /// The reliable mask's sections are addressed from `MASK_TWO_PASS_BASE`, so starting an edit
    /// there must not index the main mask's sections (it used to panic).
    #[test]
    fn test_reliable_mask_threshold_value_editing() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.form.settings.set("masking.two_pass", true);
        app.mask_mut().two_pass_sections[0].generator =
            MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.5) };
        app.mark_mask_custom(MASK_TWO_PASS_BASE);
        let main_before = app.mask().sections.clone();

        let rows = app.rows();
        let focus = app.focusable_rows().iter()
            .position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpThresholdValue { section } if is_two_pass_section(*section)))
            .expect("reliable threshold value row not focusable");
        app.pipeline_state.focus = focus;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.mask_value_editing);
        assert_eq!(app.pipeline_state.mask_value_buf, "0.5");

        for _ in 0..app.pipeline_state.mask_value_buf.len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "0.7".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.pipeline_state.mask_value_editing);
        let MaskOp::Threshold { value, .. } = &app.mask().two_pass_sections[0].generator else {
            panic!("the reliable mask's generator should still be a threshold");
        };
        assert!((value.unwrap() - 0.7).abs() < 0.001);
        assert_eq!(app.mask().sections, main_before, "the main mask must be untouched");
    }

    #[test]
    fn test_pipeline_bet_voxel_scale_row() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
        app.mark_mask_custom(0);

        let find_scale_row = |app: &App| {
            let rows = app.rows();
            let focusable = app.focusable_rows();
            focusable.iter().position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpBetVoxelScale { .. }))
        };
        let fi = find_scale_row(&app).expect("BET voxel-scale row should be shown and focusable");
        app.pipeline_state.focus = fi;

        let scale_of = |app: &App| match &app.mask().sections[0].generator {
            MaskOp::Bet { voxel_scale, .. } => *voxel_scale,
            other => panic!("expected bet, got {other:?}"),
        };
        assert_eq!(scale_of(&app), crate::pipeline::config::bet_default_voxel_scale());

        // Enter lists the scales with the current one highlighted; the mouse setting is the last.
        app.handle_key(key(KeyCode::Enter));
        let m = app.algo_modal.as_ref().expect("the scale list");
        assert_eq!((m.options.clone(), m.cursor), (vec!["1x".into(), "2x".into(), "5x".into(), "10x".into()], 0));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(scale_of(&app), 1.0, "Esc changes nothing");
        step(&mut app, -1);
        assert_eq!(scale_of(&app), 10.0);
        // A scale from a loaded config is listed too, in order.
        app.set_bet_voxel_scale(0, 3.0);
        app.handle_key(key(KeyCode::Enter));
        let m = app.algo_modal.take().unwrap();
        assert_eq!((m.options[2].as_str(), m.cursor), ("3x", 2));

        // The row belongs to BET only.
        app.mask_mut().sections[0].generator = MaskOp::hd_bet_default();
        assert!(find_scale_row(&app).is_none(), "scale row should not show for HD-BET");
    }

    /// Every BET setting sits in one block under the BET step: the step's own fractional
    /// intensity and voxel scale, then the run-wide surface settings.
    #[test]
    fn bet_settings_are_listed_together_under_the_bet_step() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
        app.mark_mask_custom(0);
        let rows = app.rows();
        let start = rows.iter().position(|r| matches!(r, PipelineRow::MaskOpGenerator { section: 0 }))
            .expect("generator row");
        let block: Vec<String> = rows[start..start + 8].iter().map(|r| match r {
            PipelineRow::MaskOpGenerator { .. } => "generator".to_string(),
            PipelineRow::MaskOpGeneratorParam { .. } => "frac".to_string(),
            PipelineRow::MaskOpBetVoxelScale { .. } => "scale".to_string(),
            PipelineRow::Param { field, .. } | PipelineRow::AlgoSelect { field, .. } => field.to_string(),
            other => format!("{other:?}"),
        }).collect();
        assert_eq!(block, ["generator", "frac", "scale", "bet_smoothness", "bet_gradient_threshold",
            "bet_iterations", "bet_subdivisions", "bet_orientation"]);
        // Listed once, however many BET steps share them.
        let count = |app: &App| app.rows().iter()
            .filter(|r| matches!(r, PipelineRow::Param { field: "bet_smoothness", .. })).count();
        assert_eq!(count(&app), 1);
        let bet_and_phase = MASK_PRESET_OPTIONS.iter().position(|o| *o == "bet-and-phase").unwrap();
        app.apply_mask_preset(bet_and_phase);
        assert!(app.mask().sections.len() > 1);
        assert_eq!(count(&app), 1);
        // Not shown without BET.
        app.mask_mut().sections = vec![crate::pipeline::config::MaskSection {
            input: crate::pipeline::config::MaskingInput::Magnitude,
            generator: MaskOp::hd_bet_default(),
            refinements: vec![],
        }];
        app.mark_mask_custom(0);
        assert_eq!(count(&app), 0);
    }

    #[test]
    fn test_pipeline_hd_bet_step_row() {
        use crate::pipeline::config::MaskOp;
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.mask_mut().sections[0].generator = MaskOp::hd_bet_default();
        app.mark_mask_custom(0);
        let rows = app.rows();
        assert!(!rows.iter().any(|r| matches!(r, PipelineRow::MaskOpHdBetStep { .. })),
                "whole-volume HD-BET has no window step");
        app.mask_mut().sections[0].generator =
            MaskOp::HdBet { patch: Some(MaskOp::hd_bet_default_patch()), tta: false,
                            tile_step: crate::pipeline::config::hd_bet_default_tile_step() };

        let find_step_row = |app: &App| {
            let rows = app.rows();
            let focusable = app.focusable_rows();
            focusable.iter().position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpHdBetStep { .. }))
        };
        let fi = find_step_row(&app).expect("HD-BET step row should be shown and focusable");
        app.pipeline_state.focus = fi;

        let step_of = |app: &App| match &app.mask().sections[0].generator {
            MaskOp::HdBet { tile_step, .. } => *tile_step,
            other => panic!("expected hd-bet, got {other:?}"),
        };
        assert_eq!(step_of(&app), crate::pipeline::config::hd_bet_default_tile_step());

        // Chosen from a list: a coarser stride, then back.
        app.handle_key(key(KeyCode::Right));
        assert_eq!(step_of(&app), 0.5, "→ changes nothing");
        step(&mut app, 1);
        assert_eq!(step_of(&app), 0.625);
        step(&mut app, -1);
        assert_eq!(step_of(&app), 0.5);

        // The row belongs to HD-BET only.
        app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
        assert!(find_step_row(&app).is_none(), "step row should not show for BET");
    }

    #[test]
    fn test_pipeline_threshold_value_esc_cancels() {
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.mask_mut().sections[0].generator = crate::pipeline::config::MaskOp::Threshold {
            method: crate::pipeline::config::MaskThresholdMethod::Fixed,
            value: Some(0.5),
        };
        app.mark_mask_custom(0);
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
        assert!(app.pipeline_state.mask_value_editing);
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.pipeline_state.mask_value_editing);
        // Value should be unchanged
        if let crate::pipeline::config::MaskOp::Threshold { value, .. } = &app.mask().sections[0].generator {
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

    // ─── Export → DICOM: the checklist of maps ───

    /// The checklist lists the maps the settings make, all ticked; unticking one passes the
    /// rest to --dicom-outputs; a map turned on later is ticked; the last one stays ticked.
    #[test]
    fn the_dicom_checklist_follows_what_the_settings_make() {
        let mut app = App::new();
        app.form.export_dicom = true;
        app.form.settings.set("pipeline.do_swi", true);
        let fields: Vec<&str> = app.page_rows(Page::Export).iter().filter_map(row_field).collect();
        assert!(["dicom_out_chimap", "dicom_out_swi", "dicom_out_minip"].iter().all(|f| fields.contains(f)), "{fields:?}");
        assert!(!fields.contains(&"dicom_out_t2starmap"), "T2* is not made: {fields:?}");
        let notes: Vec<String> = app.page_rows(Page::Export).iter()
            .filter_map(|r| match r { PipelineRow::Note { text } => Some(text.to_string()), _ => None }).collect();
        assert_eq!(notes, ["Maps to export"], "a heading and the maps made, nothing about the maps not made");
        assert!(!fields.contains(&"dicom_out_swi_phase"), "SWI on, its filtered phase off: {fields:?}");
        assert_eq!(app.dicom_outputs(), None, "all ticked: no --dicom-outputs");

        app.toggle_field("dicom_out_minip");
        assert!(!app.field_toggle("dicom_out_minip"));
        let s = |v: &[&str]| Some(v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(app.dicom_outputs(), s(&["magnitude", "Chimap", "swi"]));
        // A map turned on later is exported too.
        app.form.settings.set("pipeline.do_r2starmap", true);
        assert_eq!(app.dicom_outputs(), s(&["magnitude", "Chimap", "swi", "R2starmap"]));
        // The last ticked map stays ticked.
        app.toggle_field("dicom_out_magnitude");
        app.toggle_field("dicom_out_swi");
        app.toggle_field("dicom_out_r2starmap");
        app.toggle_field("dicom_out_chimap");
        assert!(app.field_toggle("dicom_out_chimap"), "the last map is not unticked");
        assert!(app.error_message.as_deref().is_some_and(|e| e.contains("untick Export DICOM")));
        // `a` ticks them all; `r` resets one.
        goto_page(&mut app, Page::Export);
        app.handle_key(key(KeyCode::Char('a')));
        assert_eq!(app.dicom_outputs(), None);
        app.toggle_field("dicom_out_swi");
        app.reset_field("dicom_out_swi");
        assert!(app.field_toggle("dicom_out_swi"));
        // Only while exporting.
        app.toggle_field("dicom_out_swi");
        app.form.bids_dir = "/b".into();
        assert!(crate::tui::command::build_run_args(&app).unwrap().dicom_outputs.is_some());
        app.form.export_dicom = false;
        assert!(crate::tui::command::build_run_args(&app).unwrap().dicom_outputs.is_none());
    }

    /// The segmentation is listed when one is made, and the checklist says its SEG needs χ.
    #[test]
    fn the_segmentation_needs_a_map_to_be_drawn_on() {
        let mut app = App::new();
        app.form.export_dicom = true;
        let fields = |app: &App| app.page_rows(Page::Export).iter().filter_map(row_field).collect::<Vec<_>>();
        assert!(!fields(&app).contains(&"dicom_out_dseg"), "no segmentation, no SEG");
        app.form.settings.set("pipeline.do_segmentation", true);
        assert!(fields(&app).contains(&"dicom_out_dseg") && fields(&app).contains(&"dicom_out_magnitude"));
        let note = |app: &App| app.page_rows(Page::Export).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("needs a map to be drawn on")));
        assert!(!note(&app));
        // χ alone unticked: the magnitude is still there to draw it on.
        app.toggle_field("dicom_out_chimap");
        assert!(!note(&app), "the magnitude is reference enough");
        app.toggle_field("dicom_out_magnitude");
        assert!(note(&app), "no map left for the SEG");
        assert_eq!(app.dicom_outputs(), Some(vec!["dseg".to_string()]));
    }

    /// The checklist's maps are the ones `--dicom-outputs` accepts.
    #[test]
    fn the_checklist_and_the_cli_name_the_same_maps() {
        let mut ours: Vec<String> = DICOM_MAPS.iter().map(|m| m.suffix.to_lowercase()).collect();
        let mut cli: Vec<String> = crate::cli::DICOM_OUTPUT_TOKENS.iter().map(|t| t.to_string()).collect();
        ours.sort();
        cli.sort();
        assert_eq!(ours, cli);
    }

    /// `--dicom-outputs` takes one map per use or a comma list, and refuses unknown names.
    #[test]
    fn dicom_outputs_repeat_or_comma_separate() {
        use clap::Parser;
        let parse = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "run", "bids", "--export-dicom"];
            argv.extend_from_slice(extra);
            crate::cli::Cli::try_parse_from(argv)
        };
        for args in [&["--dicom-outputs", "chimap,swi"][..], &["--dicom-outputs", "Chimap", "--dicom-outputs", "SWI"][..]] {
            let crate::cli::Command::Run(r) = parse(args).unwrap().command else { panic!() };
            let got: Vec<String> = r.dicom_outputs.unwrap().iter().map(|s| s.to_lowercase()).collect();
            assert_eq!(got, ["chimap", "swi"], "{args:?}");
        }
        assert!(parse(&["--dicom-outputs", "chimapp"]).is_err(), "an unknown map is refused");
        // A space-separated list no longer swallows the BIDS directory.
        let argv = ["qsmxt", "run", "--export-dicom", "--dicom-outputs", "chimap", "bids"];
        let crate::cli::Command::Run(r) = crate::cli::Cli::try_parse_from(argv).unwrap().command else { panic!() };
        assert_eq!(r.bids_dir, std::path::PathBuf::from("bids"));
    }

    // ─── The Reconstruction sidebar (and its pill strip when narrow): focus, arrows, back ───

    #[test]
    fn reconstruction_opens_on_its_sidebar_at_masking() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
        assert!(app.on_subtabs() && app.sidebar());
        assert_eq!(app.page(), Some(Page::Masking), "Reconstruction opens on Masking");
        // ↓/↑ move through the sections, across groups, without wrapping; focus stays.
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.page(), Some(Page::Motion));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.page(), Some(Page::ComputeQsm), "into the next group");
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.page(), Some(Page::FieldMap));
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::End));
        assert_eq!(app.page(), Some(Page::Export));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.page(), Some(Page::Export), "no wrap at the end");
        app.handle_key(key(KeyCode::Home));
        assert_eq!(app.page(), Some(Page::Geometry));
        // `[`/`]` switch too, wrapping.
        app.handle_key(key(KeyCode::Char('[')));
        assert_eq!(app.page(), Some(Page::Export));
        assert!(app.on_subtabs());
        // ← does nothing in the sidebar.
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.page(), Some(Page::Export));
        assert!(app.on_subtabs());
    }

    #[test]
    fn right_enters_the_section_and_up_from_its_first_row_returns() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('3'))); // Masking, the default
        assert!(app.focusable_rows().len() > 1);
        app.handle_key(key(KeyCode::Right));
        assert!(!app.on_subtabs());
        assert_eq!(app.focus(), 0);
        assert_eq!(app.page(), Some(Page::Masking));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focus(), 1);
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert!(app.on_subtabs(), "↑ from the first row goes back to the sidebar");
        assert_eq!(app.page(), Some(Page::Masking));
        // Enter goes in too; from the first section ↑ goes to the tab bar and ↓ comes back.
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.on_subtabs());
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Home));
        assert_eq!(app.page(), Some(Page::Geometry));
        app.handle_key(key(KeyCode::Up));
        assert!(app.tab_focus);
        app.handle_key(key(KeyCode::Down));
        assert!(!app.tab_focus && app.on_subtabs());
        assert_eq!(app.page(), Some(Page::Geometry));
    }

    /// → on a row changes nothing (no row cycles); ← goes back to the sidebar on the same
    /// section, and → comes back in.
    #[test]
    fn right_on_a_row_changes_nothing_and_left_returns_to_the_sidebar() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('3')));
        app.handle_key(key(KeyCode::Char(']'))); // Motion
        app.handle_key(key(KeyCode::Char(']'))); // Compute QSM
        app.handle_key(key(KeyCode::Char(']'))); // Field map
        assert_eq!(app.page(), Some(Page::FieldMap));
        app.handle_key(key(KeyCode::Right));
        let field = app.focused_field_name();
        let before = crate::tui::command::build_command_string(&app);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.page(), Some(Page::FieldMap), "→ on a row switched the section");
        assert!(!app.on_subtabs());
        assert_eq!(app.focused_field_name(), field);
        assert_eq!(crate::tui::command::build_command_string(&app), before, "→ changed a value");
        app.handle_key(key(KeyCode::Left));
        assert!(app.on_subtabs() && app.sidebar());
        assert_eq!(app.page(), Some(Page::FieldMap));
        app.handle_key(key(KeyCode::Left));
        assert!(app.on_subtabs(), "← on the sidebar stays there");
        assert_eq!(app.page(), Some(Page::FieldMap));
        app.handle_key(key(KeyCode::Right));
        assert!(!app.on_subtabs());
    }

    /// Everything on, so every section has its full rows (and the mask has every kind of row).
    fn app_with_every_row() -> App {
        use crate::pipeline::config::{MaskOp, MaskSection, MaskingInput};
        let mut app = App::new();
        app.form.settings.set("pipeline.do_swi", true);
        app.form.settings.set("pipeline.do_smwi", true);
        app.form.export_dicom = true;
        app.form.settings.set("pipeline.do_t2starmap", true);
        app.form.settings.set("pipeline.do_r2starmap", true);
        app.form.settings.set("pipeline.do_chi_separation", true);
        app.apply_mask_preset(2); // hd-bet + signal-erode
        let mut m = app.mask_mut();
        m.sections[0].generator = MaskOp::HdBet { patch: Some(MaskOp::hd_bet_default_patch()), tta: false,
            tile_step: crate::pipeline::config::hd_bet_default_tile_step() };
        m.sections.push(MaskSection { input: MaskingInput::Magnitude, generator: MaskOp::bet(0.5), refinements: vec![
            MaskOp::Erode { iterations: 1 }, MaskOp::FillHoles { max_size: 0 }, MaskOp::GaussianSmooth { sigma_mm: 1.0 },
            MaskOp::MinIntensity { value: 100.0 },
        ] });
        m.sections.push(MaskSection { input: MaskingInput::Magnitude, generator: MaskOp::MindGrab { tta: false }, refinements: vec![] });
        drop(m);
        app.pipeline_state.mask_preset = *MASK_PRESET_CUSTOM;
        app
    }

    /// ← from every row of every section goes back to the sections (the sidebar, or the pill
    /// strip on a narrow terminal) on that section, and changes nothing.
    #[test]
    fn left_from_every_row_of_every_section_returns_to_the_sections() {
        for wide in [true, false] {
            let mut app = app_with_every_row();
            app.sidebar_layout.set(wide);
            app.handle_key(key(KeyCode::Char('3')));
            for page in app.pages(TAB_RECONSTRUCTION) {
                goto_page(&mut app, page);
                let n = app.focusable_rows().len();
                if n == 0 {
                    // A section with only a note (COSMOS/STI without groups): in and back out.
                    app.subtab_focus = false;
                    app.handle_key(key(KeyCode::Left));
                    assert!(app.on_subtabs() && app.page() == Some(page), "{page:?}");
                }
                for i in 0..n {
                    app.subtab_focus = false;
                    app.set_focus(i);
                    let before = crate::tui::command::build_command_string(&app);
                    let row = app.focused_row();
                    app.handle_key(key(KeyCode::Left));
                    assert!(app.on_subtabs(), "{page:?} row {i} {row:?}: ← did not go back (wide: {wide})");
                    assert_eq!(app.sidebar(), wide);
                    assert_eq!(app.page(), Some(page), "{page:?} row {i}: ← changed the section");
                    assert!(app.algo_modal.is_none() && !app.pipeline_state.mask_value_editing);
                    assert_eq!(crate::tui::command::build_command_string(&app), before, "{page:?} row {i} {row:?}: ← changed a value");
                    // → comes back in (onto the first row), and changes nothing either.
                    app.handle_key(key(if wide { KeyCode::Right } else { KeyCode::Down }));
                    assert!(!app.on_subtabs());
                    app.set_focus(i);
                    app.handle_key(key(KeyCode::Right));
                    assert!(!app.on_subtabs() && app.page() == Some(page) && app.algo_modal.is_none(), "{page:?} row {i}: →");
                    assert_eq!(crate::tui::command::build_command_string(&app), before, "{page:?} row {i} {row:?}: → changed a value");
                }
            }
        }
    }

    /// While a number is typed, ←/→ move the cursor; ← goes back to the sections only after.
    #[test]
    fn left_while_typing_moves_the_cursor() {
        let mut app = App::new();
        goto_page(&mut app, Page::Inversion);
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| matches!(rows[ri], PipelineRow::Param { .. })).expect("a value row");
        app.set_focus(fi);
        app.subtab_focus = false;
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.editing);
        let at = app.pipeline_state.cursor;
        app.handle_key(key(KeyCode::Left));
        assert!(app.pipeline_state.editing && !app.on_subtabs());
        assert_eq!(app.pipeline_state.cursor, at.saturating_sub(1));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Left));
        assert!(app.on_subtabs());

        // The same for a mask row's number.
        let mut app = app_with_every_row();
        goto_page(&mut app, Page::Masking);
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| matches!(rows[ri], PipelineRow::MaskOpEntry { .. })).unwrap();
        app.set_focus(fi);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.pipeline_state.mask_value_editing);
        let at = app.pipeline_state.cursor;
        app.handle_key(key(KeyCode::Left));
        assert!(app.pipeline_state.mask_value_editing && !app.on_subtabs());
        assert_eq!(app.pipeline_state.cursor, at - 1);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Left));
        assert!(app.on_subtabs());
    }

    /// Every choice on the tab opens a list (Enter, or Space) with the current option
    /// highlighted; Esc closes it with nothing changed, and Enter on another option chooses it.
    #[test]
    fn every_choice_opens_a_list_that_commits_or_cancels() {
        let mut app = app_with_every_row();
        app.handle_key(key(KeyCode::Char('3')));
        let mut kinds = HashSet::new();
        for page in app.pages(TAB_RECONSTRUCTION) {
            goto_page(&mut app, page);
            let mut i = 0;
            while i < app.focusable_rows().len() {
                app.set_focus(i);
                let row = app.focused_row().unwrap();
                i += 1;
                let Some(expected) = app.selector_modal(&row) else { continue };
                if expected.options.len() < 2 { continue; }
                kinds.insert(std::mem::discriminant(&expected.target));
                let before = crate::tui::command::build_command_string(&app);
                for open in [KeyCode::Enter, KeyCode::Char(' ')] {
                    app.handle_key(key(open));
                    let m = app.algo_modal.as_ref().unwrap_or_else(|| panic!("{page:?} {row:?}: {open:?} opened no list"));
                    assert_eq!((m.options.clone(), m.cursor), (expected.options.clone(), expected.cursor), "{row:?}");
                    app.handle_key(key(KeyCode::Down));
                    app.handle_key(key(KeyCode::Esc));
                    assert!(app.algo_modal.is_none());
                    assert_eq!(crate::tui::command::build_command_string(&app), before, "{row:?}: Esc changed it");
                }
                // A selector the rules or the run treat specially is left to its own tests.
                if matches!(row, PipelineRow::AlgoSelect { .. } | PipelineRow::MaskOpGenerator { .. } | PipelineRow::MaskOpAddStep { .. }) { continue; }
                app.handle_key(key(KeyCode::Enter));
                let other = (expected.cursor + 1) % expected.options.len();
                app.algo_modal.as_mut().unwrap().cursor = other;
                app.handle_key(key(KeyCode::Enter));
                assert!(app.algo_modal.is_none());
                assert_eq!(app.selector_modal(&row).map(|m| m.cursor), Some(other), "{row:?}: not chosen");
                // And back.
                app.handle_key(key(KeyCode::Enter));
                app.algo_modal.as_mut().unwrap().cursor = expected.cursor;
                app.handle_key(key(KeyCode::Enter));
                assert_eq!(crate::tui::command::build_command_string(&app), before, "{row:?}: not restored");
            }
        }
        use AlgoModalTarget as T;
        for target in [T::Select(String::new()), T::MaskInput(0), T::MaskGenerator(0), T::MaskThresholdMethod(0),
                       T::MaskCombine(0), T::MaskHdBetInference(0), T::MaskHdBetStep(0), T::MaskBetVoxelScale(0), T::MaskAddStep(0)] {
            if matches!(target, T::MaskThresholdMethod(_)) { continue; } // no threshold generator here: below
            assert!(kinds.contains(&std::mem::discriminant(&target)), "no row opened {target:?}");
        }
    }

    /// `+` / `-` nudge a masking number by its old step, within its range; they do nothing on
    /// any other row.
    #[test]
    fn plus_and_minus_nudge_mask_numbers() {
        use crate::pipeline::config::MaskOp;
        let mut app = app_with_every_row();
        goto_page(&mut app, Page::Masking);
        let focus = |app: &mut App, want: PipelineRow| {
            let rows = app.rows();
            let fi = app.focusable_rows().iter().position(|&ri| rows[ri] == want).expect("row");
            app.set_focus(fi);
        };
        focus(&mut app, PipelineRow::MaskOpGeneratorParam { section: 1 });
        app.handle_key(key(KeyCode::Char('+')));
        assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.55));
        app.handle_key(key(KeyCode::Char('-')));
        app.handle_key(key(KeyCode::Char('-')));
        assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.45));
        focus(&mut app, PipelineRow::MaskOpEntry { section: 1, index: 0 });
        app.handle_key(key(KeyCode::Char('-')));
        assert_eq!(app.mask().sections[1].refinements[0], MaskOp::Erode { iterations: 1 }, "stays at least 1");
        app.handle_key(key(KeyCode::Char('=')));
        assert_eq!(app.mask().sections[1].refinements[0], MaskOp::Erode { iterations: 2 }, "= is + unshifted");
        focus(&mut app, PipelineRow::MaskOpEntry { section: 1, index: 3 });
        app.handle_key(key(KeyCode::Char('+')));
        assert_eq!(app.mask().sections[1].refinements[3], MaskOp::MinIntensity { value: 110.0 });
        let MaskOp::SignalErode { depth_cap, .. } = app.mask().sections[0].refinements[0].clone() else { panic!() };
        focus(&mut app, PipelineRow::MaskOpSignalErodeParam { section: 0, index: 0, param: 0 });
        app.handle_key(key(KeyCode::Char('+')));
        assert!(matches!(app.mask().sections[0].refinements[0], MaskOp::SignalErode { depth_cap: d, .. } if d == depth_cap + 1));
        // Not on a choice.
        let before = crate::tui::command::build_command_string(&app);
        focus(&mut app, PipelineRow::MaskOpHdBetStep { section: 0 });
        app.handle_key(key(KeyCode::Char('+')));
        assert_eq!(crate::tui::command::build_command_string(&app), before);
    }

    /// A DICOM series' type is chosen from a list (Enter), current type highlighted; ←/→ leave it.
    #[test]
    fn a_dicom_series_type_is_chosen_from_a_list() {
        use crate::dicom::{DicomAcquisition, DicomSeries, DicomSession, DicomStudy, DicomSubject, SeriesType};
        let mut app = App::new();
        app.input_mode = InputMode::DicomToBids;
        app.active_tab = TAB_INPUT;
        app.active_field = App::INPUT_IO_FIELDS;
        let series = DicomSeries { series_uid: "1".into(), description: "gre".into(), series_number: 1,
            series_type: SeriesType::Magnitude, auto_type: SeriesType::Magnitude, ..Default::default() };
        app.dicom_state.session = Some(DicomSession { subjects: vec![DicomSubject { patient_id: "p".into(), studies: vec![DicomStudy {
            study_date: "20240101".into(), acquisitions: vec![DicomAcquisition { name: "gre".into(), run_number: 1, series: vec![series] }] }] }],
            ..Default::default() });
        app.dicom_state.focus = DicomFocus::Series(0);
        let type_now = |app: &App| app.dicom_state.session.as_ref().unwrap().subjects[0].studies[0].acquisitions[0].series[0].series_type;
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(type_now(&app), SeriesType::Magnitude, "←/→ change nothing");
        assert!(app.algo_modal.is_none());
        app.handle_key(key(KeyCode::Enter));
        let m = app.algo_modal.as_ref().expect("the series-type list");
        assert_eq!((m.options.len(), m.cursor), (SeriesType::ALL.len(), 0));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(type_now(&app), SeriesType::Magnitude, "Esc changes nothing");
        app.handle_key(key(KeyCode::Char(' ')));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(type_now(&app), SeriesType::Phase);
    }

    /// A threshold's method (the generator's, or a refinement's from a loaded config) is chosen
    /// from a list.
    #[test]
    fn a_threshold_method_is_chosen_from_a_list() {
        use crate::pipeline::config::{MaskOp, MaskThresholdMethod};
        let mut app = App::new();
        goto_page(&mut app, Page::Masking);
        app.mask_mut().sections[0].refinements.push(MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None });
        let rows = app.rows();
        let gen = app.focusable_rows().iter().position(|&ri| matches!(rows[ri], PipelineRow::MaskOpGeneratorParam { .. })).unwrap();
        app.set_focus(gen);
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.algo_modal.as_ref().map(|m| &m.target), Some(AlgoModalTarget::MaskThresholdMethod(0))));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.mask().sections[0].generator, MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. }));
        // (The rows moved: a percentile has a value row.)
        let rows = app.rows();
        let refinement = app.focusable_rows().iter().rposition(|&ri| matches!(rows[ri], PipelineRow::MaskOpEntry { .. })).unwrap();
        app.set_focus(refinement);
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.algo_modal.as_ref().map(|m| &m.target), Some(AlgoModalTarget::MaskRefinementThresholdMethod { section: 0, .. })));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.mask().sections[0].refinements.last(), Some(MaskOp::Threshold { method: MaskThresholdMethod::Fixed, .. })));
    }

    /// The masking numbers are typed: BET's fractional intensity and each refinement step's
    /// value, checked on Enter (refused out of range, still editing), Esc leaving them; MindGrab's
    /// augmentation is a switch.
    #[test]
    fn mask_numbers_are_typed_and_mindgrab_tta_is_a_switch() {
        use crate::pipeline::config::MaskOp;
        let mut app = app_with_every_row();
        goto_page(&mut app, Page::Masking);
        let focus = |app: &mut App, pred: &dyn Fn(&PipelineRow, &App) -> bool| {
            let rows = app.rows();
            let fi = app.focusable_rows().iter().position(|&ri| pred(&rows[ri], app)).expect("row");
            app.set_focus(fi);
        };
        // BET's fractional intensity (section 1).
        focus(&mut app, &|r, _| *r == PipelineRow::MaskOpGeneratorParam { section: 1 });
        type_mask_value(&mut app, "0.35");
        assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.35));
        type_mask_value(&mut app, "1.5");
        assert!(app.pipeline_state.mask_value_editing && app.error_message.as_deref().is_some_and(|e| e.contains("at most 1")));
        app.handle_key(key(KeyCode::Esc));
        assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.35));
        // Each refinement of section 1.
        for (index, typed, check) in [
            (0usize, "3", (|op: &MaskOp| *op == MaskOp::Erode { iterations: 3 }) as fn(&MaskOp) -> bool),
            (1, "500", |op| *op == MaskOp::FillHoles { max_size: 500 }),
            (2, "2.5", |op| *op == MaskOp::GaussianSmooth { sigma_mm: 2.5 }),
            (3, "0.25", |op| *op == MaskOp::MinIntensity { value: 0.25 }),
        ] {
            focus(&mut app, &|r, _| *r == PipelineRow::MaskOpEntry { section: 1, index });
            type_mask_value(&mut app, typed);
            assert!(!app.pipeline_state.mask_value_editing, "{typed}");
            assert!(check(&app.mask().sections[1].refinements[index]), "{:?}", app.mask().sections[1].refinements[index]);
        }
        focus(&mut app, &|r, _| *r == PipelineRow::MaskOpEntry { section: 1, index: 0 });
        type_mask_value(&mut app, "0");
        assert!(app.error_message.as_deref().is_some_and(|e| e.contains("at least 1")), "an erosion of 0 is refused");
        app.handle_key(key(KeyCode::Esc));
        // MindGrab (section 2): Space or Enter switch its augmentation.
        focus(&mut app, &|r, _| *r == PipelineRow::MaskOpGeneratorParam { section: 2 });
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.mask().sections[2].generator, MaskOp::MindGrab { tta: true });
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mask().sections[2].generator, MaskOp::MindGrab { tta: false });
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.mask().sections[2].generator, MaskOp::MindGrab { tta: false }, "→ changes nothing");
    }

    /// Too narrow for the sidebar, the sections are a pill strip: ←/→ move along it, wrapping,
    /// and ↓ enters the rows.
    #[test]
    fn a_narrow_terminal_keeps_the_pill_strip_keys() {
        let mut app = App::new();
        app.sidebar_layout.set(false);
        app.handle_key(key(KeyCode::Char('3')));
        assert!(app.on_subtabs() && !app.sidebar());
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.page(), Some(Page::Motion));
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.page(), Some(Page::Export), "wraps");
        app.handle_key(key(KeyCode::Down));
        assert!(!app.on_subtabs());
        assert_eq!(app.focus(), 0);
    }

    /// The Execution tab has no pages to choose: focus starts on its first row.
    #[test]
    fn a_tab_without_sections_opens_on_its_first_row() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char(char::from_digit(TAB_EXECUTION as u32 + 1, 10).unwrap())));
        assert_eq!(app.active_tab, TAB_EXECUTION);
        assert!(!app.on_subtabs());
        assert_eq!(app.focus(), 0);
        app.handle_key(key(KeyCode::Up));
        assert!(app.tab_focus, "↑ from the first row goes to the tab bar");
        app.handle_key(key(KeyCode::Down));
        assert!(!app.tab_focus);
    }

    /// Tab, Shift-Tab and the digits land on the sidebar; `q` still quits from it.
    #[test]
    fn tab_switching_and_quitting_work_from_the_sidebar() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_PIPELINE);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
        assert!(app.on_subtabs());
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_tab, TAB_EXECUTION);
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
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
        assert_eq!(app.error_message.as_deref(), Some("BIDS directory is required"));

        let mut app = App::new();
        app.form.bids_dir = "/tmp/bids".to_string();
        app.form.execution_mode = 1;
        focus_run_button(&mut app);
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(!app.should_run);
        assert_eq!(app.error_message.as_deref(), Some("SLURM account is required"));
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

    #[test]
    fn tip_modal_ok_closes_and_leaves_tips_on() {
        let _s = crate::tui::tips::TestSettings::new();
        let mut app = App::new();
        app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.tip_modal.is_none());
        assert!(crate::tui::tips::enabled());
        assert!(!app.should_quit);
    }

    /// Turning tips off takes a deliberate move to the second button; `?` brings the
    /// pop-up back with a button that turns them on again.
    #[test]
    fn tips_can_be_turned_off_and_back_on_from_the_tui() {
        let _s = crate::tui::tips::TestSettings::new();
        let mut app = App::new();
        app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.tip_modal.is_none());
        assert!(!crate::tui::tips::enabled());
        assert!(crate::tui::tips::startup_tip().is_none());
        assert!(app.status_message.as_deref().unwrap().contains("Press ?"));

        app.handle_key(key(KeyCode::Char('?')));
        let m = app.tip_modal.as_ref().expect("? opens a tip");
        assert_eq!(m.buttons()[1], "Show tips at startup");
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Enter));
        assert!(crate::tui::tips::enabled());
    }

    #[test]
    fn tip_modal_esc_and_q_close_it_without_quitting() {
        let _s = crate::tui::tips::TestSettings::new();
        for code in [KeyCode::Esc, KeyCode::Char('q')] {
            let mut app = App::new();
            app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
            app.handle_key(key(KeyCode::Right));
            app.handle_key(key(code));
            assert!(app.tip_modal.is_none());
            assert!(!app.should_quit);
            assert!(crate::tui::tips::enabled(), "closing must not turn tips off");
        }
    }

    #[test]
    fn n_shows_the_next_tip() {
        let _s = crate::tui::tips::TestSettings::new();
        let mut app = App::new();
        app.open_tip();
        let first = app.tip_modal.as_ref().unwrap().tip;
        app.handle_key(key(KeyCode::Char('n')));
        assert_ne!(app.tip_modal.as_ref().unwrap().tip, first);
    }

    /// `?` typed into a text field is text, not a request for a tip.
    #[test]
    fn question_mark_is_typed_while_editing() {
        let _s = crate::tui::tips::TestSettings::new();
        let mut app = App::new();
        app.active_field = 1;
        app.editing = true;
        app.handle_key(key(KeyCode::Char('?')));
        assert!(app.tip_modal.is_none());
    }

    // ─── The main tab bar ───

    /// ↑ from the top of the Input tab reaches the tab bar; ←/→ there walk the tabs, wrapping,
    /// and the focus stays on the bar until ↓.
    #[test]
    fn arrows_walk_the_main_tabs_from_the_tab_bar() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Up));
        assert!(app.tab_focus);
        assert_eq!(app.active_tab, TAB_INPUT);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.active_tab, TAB_PIPELINE);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
        assert!(app.tab_focus);
        assert!(!app.on_subtabs(), "the sidebar must not look focused under the tab bar");
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.active_tab, TAB_EXECUTION);
        for _ in 0..TAB_EXECUTION + 1 {
            app.handle_key(key(KeyCode::Left));
        }
        assert_eq!(app.active_tab, TAB_NAMES.len() - 1, "← from the first tab wraps to the last");
        // Digits still work and keep the bar focused.
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
        assert!(app.tab_focus);
        // ↓ goes into the tab: onto its sidebar, here.
        app.handle_key(key(KeyCode::Down));
        assert!(!app.tab_focus);
        assert!(app.on_subtabs());
    }

    /// Enter on the bar opens a tab without sub-tabs at its first row; ↑ from the Methods tab's
    /// sub-tabs reaches the strip once they are scrolled to the top, and then the bar.
    #[test]
    fn the_tab_bar_is_reachable_from_every_tab() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('7')));
        app.handle_key(key(KeyCode::Up));
        assert!(app.tab_focus);
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.tab_focus);
        assert_eq!(app.focus(), 0);

        for page in [Page::MethodsText, Page::Pipeline] {
            app.handle_key(key(KeyCode::Char(char::from_digit(TAB_METHODS as u32 + 1, 10).unwrap())));
            app.sub_page[TAB_METHODS] = Some(page);
            app.handle_key(key(KeyCode::Right));
            app.handle_key(key(KeyCode::Down));
            app.handle_key(key(KeyCode::Up));
            assert!(!app.tab_focus && !app.on_subtabs(), "the first ↑ scrolls back up");
            app.handle_key(key(KeyCode::Up));
            assert!(app.on_subtabs());
            // Up the sidebar, then to the bar.
            for _ in 0..2 {
                if !app.tab_focus { app.handle_key(key(KeyCode::Up)); }
            }
            assert!(app.tab_focus);
            app.handle_key(key(KeyCode::Down));
            assert!(!app.tab_focus);
        }
    }

    /// Keys that act on a setting do nothing while the tab bar has focus; `q` still quits.
    #[test]
    fn setting_keys_do_nothing_on_the_tab_bar() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('4')));
        app.handle_key(key(KeyCode::Up));
        assert!(app.tab_focus);
        let before = crate::tui::command::build_command_string(&app);
        for k in [KeyCode::Char(' '), KeyCode::Char('r'), KeyCode::Char('R'), KeyCode::Char('[')] {
            app.handle_key(key(k));
            assert!(app.tab_focus, "{k:?} left the tab bar");
        }
        assert_eq!(crate::tui::command::build_command_string(&app), before);
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    // ========== Basic and Advanced mode ==========

    fn basic_medi() -> App {
        let mut app = App::new();
        app.advanced = false;
        let medi = QSM_PICKER_OPTIONS.iter().position(|o| *o == "medi").unwrap();
        app.pipeline_state.set_select("qsm_algorithm", medi);
        goto_page(&mut app, Page::Inversion);
        app.subtab_focus = false;
        app
    }

    fn fields(rows: &[PipelineRow]) -> Vec<&'static str> {
        rows.iter().filter_map(row_field).collect()
    }

    #[test]
    fn basic_mode_shows_the_choices_and_counts_the_rest() {
        let app = basic_medi();
        let rows = app.rows();
        assert_eq!(fields(&rows), ["qsm_algorithm", "medi_smv", "medi_msmv", "medi_csf", "heidi_enabled"]);
        assert_eq!(rows.last(), Some(&PipelineRow::AdvancedFooter { hidden: 15 }));
    }

    #[test]
    fn a_changed_advanced_setting_shows_in_basic_mode() {
        let mut app = basic_medi();
        *app.field_text_mut("medi_lambda").unwrap() = "2000".into();
        let rows = app.rows();
        assert!(fields(&rows).contains(&"medi_lambda"));
        assert_eq!(rows.last(), Some(&PipelineRow::AdvancedFooter { hidden: 14 }));
    }

    #[test]
    fn a_shows_every_setting_and_is_remembered() {
        let _s = crate::tui::tips::TestSettings::new();
        let mut app = basic_medi();
        app.handle_key(key(KeyCode::Char('A')));
        assert!(app.advanced && crate::tui::prefs::advanced());
        let rows = app.rows();
        assert!(fields(&rows).contains(&"medi_lambda"));
        assert!(!rows.iter().any(|r| matches!(r, PipelineRow::AdvancedFooter { .. })));
        app.handle_key(key(KeyCode::Char('A')));
        assert!(!app.advanced && !crate::tui::prefs::advanced());
    }

    #[test]
    fn enter_on_the_footer_shows_every_setting_and_keeps_the_section() {
        let _s = crate::tui::tips::TestSettings::new();
        let mut app = basic_medi();
        let last = app.focusable_rows().len() - 1;
        app.set_focus(last);
        assert!(matches!(app.focused_row(), Some(PipelineRow::AdvancedFooter { .. })));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.advanced);
        assert_eq!(app.page(), Some(Page::Inversion));
    }

    #[test]
    fn a_keeps_focus_on_the_same_setting() {
        let _s = crate::tui::tips::TestSettings::new();
        let mut app = basic_medi();
        assert!(app.focus_field("medi_csf"));
        app.handle_key(key(KeyCode::Char('A')));
        assert_eq!(app.focused_row().as_ref().and_then(row_field), Some("medi_csf"));
    }

    #[test]
    fn basic_mode_shows_the_mask_recipe_as_a_line_until_it_is_edited() {
        let mut app = App::new();
        app.advanced = false;
        goto_page(&mut app, Page::Masking);
        let rows = app.rows();
        let summary: Vec<&str> = rows.iter().filter_map(|r| match r { PipelineRow::Summary { text } => Some(text.as_str()), _ => None }).collect();
        assert_eq!(summary.join(" "), "phase-quality › otsu threshold › dilate 1 › fill-holes › erode 1");
        assert!(!rows.iter().any(|r| matches!(r, PipelineRow::MaskOpEntry { .. } | PipelineRow::MaskOpAddSection { .. })));
        app.pipeline_state.mask_preset = *MASK_PRESET_CUSTOM;
        assert!(app.rows().iter().any(|r| matches!(r, PipelineRow::MaskOpEntry { .. })));
    }

    #[test]
    fn the_mask_recipe_line_fits_beside_the_sidebar_at_80_columns() {
        let mut app = App::new();
        for (preset, name) in MASK_PRESET_OPTIONS.iter().enumerate().take(*MASK_PRESET_CUSTOM) {
            app.set_field_select("mask_preset", preset);
            for line in app.mask().mask_recipe_summary() {
                assert!(line.chars().count() <= 46, "{name}: {line}");
            }
        }
    }
}
