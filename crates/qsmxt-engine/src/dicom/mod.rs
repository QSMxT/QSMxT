//! DICOM → BIDS without file I/O: how per-file DICOM metadata is grouped into subjects, sessions,
//! acquisitions, runs and reconstructions ([`group`]), what each series is ([`classify`]), and the
//! BIDS names its images get ([`naming`]).
//!
//! The qsmxt binary reads the headers (dicom-rs), runs dcm2niix and moves files; everything that
//! decides *what goes where* lives here, so the CLI, the TUI and a browser front end (through
//! WebAssembly) classify and name the same series the same way.
//!
//! QSMxT is not a general-purpose BIDS converter. It names the series QSM needs (gradient-echo
//! magnitude, phase, real and imaginary images, a T1-weighted structural, and the scanner's own
//! QSM-related reconstructions) and keeps every other image series, unrenamed, in
//! `sub-*/[ses-*/]extra_data/`, which `.bidsignore` excludes from validation.

pub mod classify;
pub mod group;
pub mod naming;

use std::path::PathBuf;

pub use classify::{classify_series, file_component, recon_tokens, Component, SeriesFacts};
pub use group::build_session;
pub use naming::{
    find_collisions, parse_dcm2niix_suffix, plan_session, BidsStem, Dcm2niixSuffix, SeriesPlan, Target,
};

/// What a DICOM series is, and so where `qsmxt dicom-convert` puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum SeriesType {
    /// Gradient-echo magnitude (`part-mag`).
    Magnitude,
    /// Gradient-echo phase (`part-phase`).
    Phase,
    /// Real part of a complex gradient-echo image (`part-real`).
    Real,
    /// Imaginary part (`part-imag`).
    Imaginary,
    /// T1-weighted structural (`T1w`).
    T1w,
    /// An MP2RAGE inversion image (`inv-N_MP2RAGE`).
    Inversion,
    /// The scanner's T1 map (`T1map`).
    T1map,
    /// The scanner's susceptibility-weighted image (`rec-swi_part-mag_T2starw`).
    Swi,
    /// The scanner's minimum-intensity projection (`rec-mip_T2starw`).
    MinIp,
    /// The scanner's own susceptibility map (`rec-scanner_Chimap`).
    ScannerQsm,
    /// The scanner's T2* map (`rec-scanner_T2starmap`).
    T2starMap,
    /// The scanner's R2* map (`rec-scanner_R2starmap`).
    R2starMap,
    /// Kept as dcm2niix writes it, in `extra_data/`.
    Extra,
    /// Not converted.
    #[default]
    Skip,
}

impl SeriesType {
    pub const ALL: &[SeriesType] = &[
        SeriesType::Magnitude,
        SeriesType::Phase,
        SeriesType::Real,
        SeriesType::Imaginary,
        SeriesType::T1w,
        SeriesType::Inversion,
        SeriesType::T1map,
        SeriesType::Swi,
        SeriesType::MinIp,
        SeriesType::ScannerQsm,
        SeriesType::T2starMap,
        SeriesType::R2starMap,
        SeriesType::Extra,
        SeriesType::Skip,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SeriesType::Magnitude => "Magnitude",
            SeriesType::Phase => "Phase",
            SeriesType::Real => "Real",
            SeriesType::Imaginary => "Imaginary",
            SeriesType::T1w => "T1w",
            SeriesType::Inversion => "MP2RAGE inversion",
            SeriesType::T1map => "T1 map",
            SeriesType::Swi => "SWI",
            SeriesType::MinIp => "mIP",
            SeriesType::ScannerQsm => "Scanner QSM",
            SeriesType::T2starMap => "T2* map",
            SeriesType::R2starMap => "R2* map",
            SeriesType::Extra => "Extra",
            SeriesType::Skip => "Skip",
        }
    }

    pub fn next(self) -> SeriesType {
        let all = Self::ALL;
        let idx = all.iter().position(|&t| t == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(self) -> SeriesType {
        let all = Self::ALL;
        let idx = all.iter().position(|&t| t == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }

    /// Magnitude, phase, real or imaginary: one part of a gradient-echo image.
    pub fn is_complex_part(self) -> bool {
        matches!(self, SeriesType::Magnitude | SeriesType::Phase | SeriesType::Real | SeriesType::Imaginary)
    }

    /// Whether the series gets a BIDS name (rather than `extra_data/` or nothing).
    pub fn is_bids(self) -> bool {
        !matches!(self, SeriesType::Extra | SeriesType::Skip)
    }
}

/// The metadata of one DICOM file that grouping, classification and naming use. The qsmxt binary
/// reads it with dicom-rs; a browser front end can fill it from its own reader.
#[derive(Debug, Clone, Default)]
pub struct FileMeta {
    pub path: PathBuf,
    /// Cleaned PatientID, else the cleaned PatientName, else `unknown`.
    pub patient_id: String,
    pub study_uid: String,
    pub study_date: String,
    pub series_uid: String,
    pub series_description: String,
    pub protocol_name: String,
    pub sequence_name: String,
    pub series_number: i32,
    pub instance_number: Option<i32>,
    /// Echo times in ms: one for a classic file, every frame's for a multi-frame one.
    pub echo_times: Vec<f64>,
    /// ImageType values, upper-case.
    pub image_type: Vec<String>,
    /// GE's private image type (0043,102F): 0 magnitude, 1 phase, 2 real, 3 imaginary.
    pub ge_image_type: Option<i32>,
    /// ComplexImageComponent, from the file or its first frame (MAGNITUDE, PHASE, REAL, IMAGINARY, MIXED).
    pub complex_component: String,
    /// ScanningSequence values, upper-case (GR, SE, IR, EP, RM, …).
    pub scanning_sequence: Vec<String>,
    pub inversion_time: Option<f64>,
    pub repetition_time: Option<f64>,
    pub flip_angle: Option<f64>,
    pub rows: Option<u32>,
    pub columns: Option<u32>,
    pub slice_thickness: Option<f64>,
    pub pixel_spacing: Vec<f64>,
    /// ReceiveCoilName (0018,1250).
    pub receive_coil: String,
    /// Siemens coil element string (0051,100F), or an Enhanced file's single coil element name.
    pub coil_string: String,
    /// Number of coil elements an Enhanced file's MultiCoilDefinitionSequence lists, if any.
    pub coil_elements: Option<u32>,
    pub manufacturer: String,
    pub magnetic_field_strength: Option<f64>,
    /// Seconds since midnight.
    pub acquisition_time: Option<f64>,
    pub sop_class_uid: String,
    pub is_enhanced: bool,
    /// Frames in the file (1 for a classic single-frame file).
    pub frames: u32,
}

/// A group of DICOM files that form a single series.
#[derive(Debug, Clone, Default)]
pub struct DicomSeries {
    /// SeriesInstanceUID, with `#<part>` appended when one UID held several reconstructions.
    pub series_uid: String,
    /// SeriesDescription, `_RR` repeats removed.
    pub description: String,
    pub protocol_name: String,
    pub series_number: i32,
    pub image_type: Vec<String>,
    pub echo_time: Option<f64>,
    pub magnetic_field_strength: Option<f64>,
    pub num_files: usize,
    /// Images in the series: files, or frames of multi-frame files.
    pub num_images: usize,
    pub series_type: SeriesType,
    /// The type the series' metadata gave it, before any relabelling.
    pub auto_type: SeriesType,
    /// Distinct echo times (ms), sorted.
    pub echo_times: Vec<f64>,
    pub files: Vec<PathBuf>,
    pub manufacturer: String,
    pub coil_type: CoilType,
    /// Per-coil file grouping for uncombined acquisitions; empty otherwise.
    pub coil_groups: Vec<CoilGroup>,
    /// ImageType starts with ORIGINAL.
    pub original: bool,
    /// Distinct inversion times (ms), sorted.
    pub inversion_times: Vec<f64>,
    /// Median acquisition time, seconds since midnight.
    pub acquisition_time: Option<f64>,
    /// Acquisition settings (`field=value`), compared between runs of one protocol.
    pub settings: Vec<String>,
    /// Why the series got its type, when that is not obvious (a duplicate, say).
    pub note: Option<String>,
}

impl DicomSeries {
    /// `series 9 'gre_qsm'`: how messages name a series.
    pub fn display_name(&self) -> String {
        let name = if self.description.is_empty() { &self.protocol_name } else { &self.description };
        format!("series {} '{}'", self.series_number, name)
    }
}

/// One coil element's files within an uncombined acquisition. `coil_num` is the element number
/// parsed from the coil string, e.g. `H15` → 15 → `coil-15`; `None` when there is no number.
#[derive(Debug, Clone)]
pub struct CoilGroup {
    pub coil_num: Option<u32>,
    pub files: Vec<PathBuf>,
}

/// Whether coil data is combined or uncombined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoilType {
    Combined,
    Uncombined,
    #[default]
    Unknown,
}

impl CoilType {
    /// Discriminator string for series-identity keys.
    pub fn label(&self) -> &'static str {
        match self {
            CoilType::Combined => "combined",
            CoilType::Uncombined => "uncombined",
            CoilType::Unknown => "unknown",
        }
    }
}

/// An acquisition: series of one protocol (one settings group) acquired together, i.e. one run.
#[derive(Debug, Clone)]
pub struct DicomAcquisition {
    /// The BIDS `acq-` label: the cleaned protocol name, with `2`, `3`, … appended when the
    /// protocol was run again with different settings.
    pub name: String,
    pub run_number: u32,
    pub series: Vec<DicomSeries>,
}

/// A study (session) groups acquisitions from a single scan date.
#[derive(Debug, Clone)]
pub struct DicomStudy {
    pub study_date: String,
    pub acquisitions: Vec<DicomAcquisition>,
}

/// A subject groups studies from a single patient.
#[derive(Debug, Clone)]
pub struct DicomSubject {
    pub patient_id: String,
    pub studies: Vec<DicomStudy>,
}

/// What a scan found besides the series: problems to warn about, and what it left out.
#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    /// Problems the user should know about before converting.
    pub warnings: Vec<String>,
    /// What the scan decided that is worth saying (duplicates left out, protocols split by settings).
    pub notes: Vec<String>,
    /// Series that are not images (reports, presentation states, raw data) and were left out.
    pub skipped: Vec<String>,
    /// Files that could not be read as DICOM.
    pub not_dicom: usize,
    /// Empty (zero-byte) files.
    pub empty_files: Vec<PathBuf>,
}

/// A complete DICOM session loaded from a directory.
#[derive(Debug, Clone, Default)]
pub struct DicomSession {
    pub subjects: Vec<DicomSubject>,
    pub report: ScanReport,
}

impl DicomSession {
    /// Total number of series across all subjects/studies/acquisitions.
    pub fn total_series(&self) -> usize {
        self.subjects.iter().flat_map(|s| &s.studies)
            .flat_map(|st| &st.acquisitions)
            .map(|a| a.series.len())
            .sum()
    }

    /// Flatten all series into a list with indices for navigation.
    pub fn flat_series(&self) -> Vec<FlatSeriesRef> {
        let mut result = Vec::new();
        for (si, sub) in self.subjects.iter().enumerate() {
            for (sti, study) in sub.studies.iter().enumerate() {
                for (ai, acq) in study.acquisitions.iter().enumerate() {
                    for sei in 0..acq.series.len() {
                        result.push(FlatSeriesRef { sub: si, study: sti, acq: ai, series: sei });
                    }
                }
            }
        }
        result
    }

    pub fn series_mut(&mut self, r: &FlatSeriesRef) -> &mut DicomSeries {
        &mut self.subjects[r.sub].studies[r.study].acquisitions[r.acq].series[r.series]
    }

    pub fn series_ref(&self, r: &FlatSeriesRef) -> &DicomSeries {
        &self.subjects[r.sub].studies[r.study].acquisitions[r.acq].series[r.series]
    }

    /// Group series across subjects/studies into UNIQUE series for classification: the same
    /// series (same acquisition, description, reconstruction) appears once per subject, but its
    /// type is a property of the series, so it is classified once and applied to every instance.
    /// Series exported one per echo (same description) form one row. Returns groups in first-seen
    /// order; `refs[0]` is the representative.
    pub fn unique_series(&self) -> Vec<UniqueSeries> {
        type Key = (String, String, SeriesType, &'static str, Vec<String>, bool);
        let mut order: Vec<Key> = Vec::new();
        let mut groups: std::collections::HashMap<Key, UniqueSeries> = std::collections::HashMap::new();
        for (si, sub) in self.subjects.iter().enumerate() {
            for (sti, study) in sub.studies.iter().enumerate() {
                for (ai, acq) in study.acquisitions.iter().enumerate() {
                    for (sei, series) in acq.series.iter().enumerate() {
                        // The auto-classified type (not the current one) keeps a relabelled series
                        // in its row; the recon tokens keep e.g. a filtered magnitude apart.
                        let key: Key = (
                            acq.name.clone(),
                            series.description.clone(),
                            series.auto_type,
                            series.coil_type.label(),
                            recon_tokens(&series.image_type),
                            series.original,
                        );
                        let r = FlatSeriesRef { sub: si, study: sti, acq: ai, series: sei };
                        match groups.get_mut(&key) {
                            Some(g) => {
                                g.refs.push(r);
                                for &te in &series.echo_times {
                                    push_te(&mut g.echo_times, te);
                                }
                            }
                            None => {
                                order.push(key.clone());
                                let mut echo_times = Vec::new();
                                for &te in &series.echo_times {
                                    push_te(&mut echo_times, te);
                                }
                                groups.insert(key, UniqueSeries {
                                    acq_name: acq.name.clone(),
                                    run_number: acq.run_number,
                                    refs: vec![r],
                                    echo_times,
                                });
                            }
                        }
                    }
                }
            }
        }
        order.into_iter().filter_map(|k| groups.remove(&k)).collect()
    }

    /// Number of unique series — navigation bound for the classification list.
    pub fn unique_series_count(&self) -> usize {
        self.unique_series().len()
    }

    /// Apply a classification type to every instance in `refs`.
    pub fn set_type_for_refs(&mut self, refs: &[FlatSeriesRef], t: SeriesType) {
        for r in refs {
            self.series_mut(r).series_type = t;
        }
    }
}

/// Add an echo time (ms) to a sorted list of distinct ones.
pub(crate) fn push_te(tes: &mut Vec<f64>, te: f64) {
    if !tes.iter().any(|&t| same_te(t, te)) {
        tes.push(te);
        tes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    }
}

/// Echo times (ms) that are the same echo: equal to 0.01 ms.
pub(crate) fn same_te(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

/// Index into the flattened series list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatSeriesRef {
    pub sub: usize,
    pub study: usize,
    pub acq: usize,
    pub series: usize,
}

/// A unique series shared across subjects: one classification applies to all `refs`.
#[derive(Debug, Clone)]
pub struct UniqueSeries {
    pub acq_name: String,
    pub run_number: u32,
    pub refs: Vec<FlatSeriesRef>,
    /// Every echo time (ms) of the series' instances, sorted: a series exported one echo per
    /// series shows all its echoes.
    pub echo_times: Vec<f64>,
}

impl UniqueSeries {
    /// Number of distinct subjects this classification spans.
    pub fn subject_count(&self) -> usize {
        let mut subs: Vec<usize> = self.refs.iter().map(|r| r.sub).collect();
        subs.sort_unstable();
        subs.dedup();
        subs.len()
    }
}

/// Clean a string for use as a BIDS label (alphanumeric only).
pub fn clean_bids_label(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// Strip trailing Siemens `_RR` suffixes (repeated reconstructions) from a series description.
pub fn normalize_series_description(desc: &str) -> String {
    let mut s = desc.trim().to_string();
    while s.ends_with("_RR") {
        s.truncate(s.len() - 3);
    }
    s
}

#[cfg(test)]
mod tests;
