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

/// The pipeline form's state on screen: the page shown, what decides its rows, focus and editing,
/// and where the mask recipe came from. Its settings are in [`RunForm::settings`].
#[derive(Debug, Clone)]
pub struct PipelineFormState {
    /// Which page's rows to render — set by the App (from the active tab and sub-tab) before
    /// routing input or drawing.
    pub page: Page,
    /// What the rest of the form decides about these rows — set by the App with `page`.
    pub ctx: RowContext,
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
        Self {
            page: Page::Geometry,
            ctx: RowContext::default(),
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

impl App {
    /// QSM → Compute QSM: the QSM group's switch, with what turning it off does, or why QSM is
    /// reconstructed anyway.
    fn compute_qsm_rows(&self, ctx: &RowContext, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Toggle {
            label: "Compute QSM", field: "do_qsm",
            help: "Enable QSM reconstruction (disable to only run the other outputs)",
        });
        if !self.form.settings.toggle("pipeline.do_qsm") {
            match ctx.qsm_forced_by {
                Some(why) => rows.push(PipelineRow::Note { text: format!("  Required: {why}. Left off, the run turns it on").into() }),
                None => {
                    rows.push(PipelineRow::Note { text: "  Off: no field map, background removal, inversion or reference.".into() });
                    rows.push(PipelineRow::Note { text: "  χ-separation and SMWI need QSM, or a supplied one.".into() });
                }
            }
        }
    }

    /// The notes a page of a stage that does not run shows instead of its settings: a QSM section
    /// says where QSM is turned on while it is off, and why when the settings skip its stage (the
    /// sidebar lists every section whatever the settings). `None` while the stage runs.
    pub fn page_notes(&self, page: Page, ctx: &RowContext) -> Option<Vec<PipelineRow>> {
        let notes = |lines: &[&'static str]| Some(lines.iter().map(|&l| PipelineRow::Note { text: l.into() }).collect());
        match page {
            Page::FieldMap | Page::Background | Page::Inversion | Page::MultiOrientation | Page::Reference
                if !self.qsm_runs(ctx) => notes(&[QSM_OFF_NOTE]),
            Page::FieldMap if !self.field_mapping_runs_for(ctx.num_echoes).0 => notes(&[
                "No field mapping with these settings: the inversion takes the wrapped phase",
                "itself (iQSM, iQSM+, single-echo TGV), or iQFM makes the field from it.",
            ]),
            Page::Background if !self.inversion_needs_bg_removal() => notes(&[
                "No background removal with these settings: the inversion takes the total field",
                "or the wrapped phase itself (see Inversion).",
            ]),
            Page::MultiOrientation if !ctx.orientation_group => notes(&[
                "No orientation groups: runs of one head at different angles, reconstructed together.",
                "Choose them on 1 Input → Orientations, and these settings apply.",
            ]),
            _ => None,
        }
    }

    /// Whether the selected inversion reads a background-removed local field (QSM.rs
    /// `InversionInput::LocalField`), and so whether background removal is part of the run.
    pub fn inversion_needs_bg_removal(&self) -> bool {
        !qsmxt_engine::bridge::skips_bgremove(&self.stage_config())
    }

    /// The settings that decide which reconstruction stages run — the inversion (and MEDI's SMV
    /// mode) and the background removal — on an otherwise default config.
    fn stage_config(&self) -> crate::pipeline::config::PipelineConfig {
        let st = &self.form.settings;
        let mut config = crate::pipeline::config::PipelineConfig::default();
        if let Ok(a) = serde_json::from_value(st.get("inversion.algorithm")) {
            config.inversion.algorithm = a;
        }
        config.inversion.medi.smv = st.toggle("inversion.medi.smv");
        if let Ok(b) = serde_json::from_value(st.get("bg_removal.algorithm")) {
            config.bg_removal.algorithm = b;
        }
        config
    }

    /// Whether field mapping runs in front of the inversion, for a run limited to `num_echoes`
    /// echoes (`None`: unknown): `(for some runs, for every run)`.
    ///
    /// The runner's own rule ([`crate::pipeline::runner::field_mapping_runs`]), asked for the
    /// echo count the run is limited to, or for one and for several echoes when that is unknown —
    /// TGV maps the field of multi-echo data but reconstructs a single echo from its wrapped phase.
    pub fn field_mapping_runs_for(&self, num_echoes: Option<usize>) -> (bool, bool) {
        let config = self.stage_config();
        let counts = match num_echoes {
            Some(n) => vec![n],
            None => vec![1, 2],
        };
        let runs: Vec<bool> = counts.iter()
            .map(|&n| qsmxt_engine::bridge::field_mapping_runs(&config, n)).collect();
        (runs.iter().any(|&r| r), runs.iter().all(|&r| r))
    }

    /// Whether QSM is reconstructed: Compute QSM is on, or another output forces it.
    pub fn qsm_runs(&self, ctx: &RowContext) -> bool {
        self.form.settings.toggle("pipeline.do_qsm") || ctx.qsm_forced_by.is_some()
    }
}

#[cfg(test)]
impl App {
    /// The chosen inversion, as its position in [`QSM_ALGO_OPTIONS`].
    pub fn qsm_algorithm(&self) -> usize {
        let algorithm = self.form.settings.get("inversion.algorithm");
        QSM_ALGO_OPTIONS.iter().position(|a| algorithm == *a).expect("an inversion of QSM_ALGO_OPTIONS")
    }

    /// Choose the inversion at position `i` in [`QSM_ALGO_OPTIONS`], leaving HEIDI as it is.
    pub fn set_qsm_algorithm(&mut self, i: usize) {
        self.form.settings.set("inversion.algorithm", QSM_ALGO_OPTIONS[i]);
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
    /// The recommendations dismissed (Keep mine on a `recommended` or `discouraged` finding), each
    /// for the value its setting had then: `--dismiss-recommendation`, and so the run's config.
    pub dismissed: Vec<qsmxt_engine::Dismissal>,
    /// The toolbox pipeline chosen on the Pipeline tab: the config's `reference_pipeline`, while
    /// the form still makes its choices ([`super::pipelines_tab`]).
    pub reference_pipeline: Option<String>,

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
            dismissed: Vec::new(),
            reference_pipeline: None,
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
        let group = self.filter_state.orientation_group_by();
        let algorithm = self.field_select("orientation_algorithm");
        // Whether QSM runs is the resolved config's answer — the same enforcement the run applies.
        // The config as the form gives it, and as the run takes it: QSM another output needs is
        // a required change the run makes itself when it is left off.
        let qsm_forced = config.as_ref().filter(|_| !self.form.settings.toggle("pipeline.do_qsm"))
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
                let ctx = self.row_context();
                if !self.qsm_runs(&ctx) {
                    SectionState::Off
                } else if (page == Page::FieldMap && !self.field_mapping_runs_for(ctx.num_echoes).0)
                    || (page == Page::Background && !self.inversion_needs_bg_removal())
                    || (page == Page::MultiOrientation && !ctx.orientation_group)
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
        self.qsm_runs(&self.row_context())
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
            if let Some(notes) = self.page_notes(page, ctx) {
                return notes;
            }
            return self.registry_rows(page, ctx, config).unwrap_or_else(|| {
                // Compute QSM is the one page built by hand.
                let mut rows = Vec::new();
                if page == Page::ComputeQsm {
                    self.compute_qsm_rows(ctx, &mut rows);
                }
                rows
            });
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
        if field == "qsm_algorithm" {
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
        if field == "qsm_algorithm" {
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
        if field == "do_segmentation" {
            return self.field_toggle(field)
                || self.forced_by(crate::pipeline::requirements::Output::Segmentation).is_some();
        }
        if field == "do_qsm" && !self.form.settings.toggle("pipeline.do_qsm") {
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
        self.form.flag(field).is_some_and(|&b| b)
    }

    /// Flip a checkbox. Turning off an output another setting needs is refused, with the reason.
    pub fn toggle_field(&mut self, field: &str) {
        if let Some(m) = dicom_map(field) {
            return self.toggle_dicom_map(m);
        }
        if field == "do_qsm" && self.form.settings.toggle("pipeline.do_qsm") {
            // QSM another output needs: refuse turning it off, as for the other maps.
            self.form.settings.set("pipeline.do_qsm", false);
            let why = self.row_context().qsm_forced_by;
            self.form.settings.set("pipeline.do_qsm", true);
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
        if let Some(b) = self.form.flag_mut(field) {
            *b = !*b;
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
        let form = RunForm::default();
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
        } else if field == "mask_preset" {
            self.set_field_select(field, PipelineFormState::default().mask_preset);
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
mod tests;
