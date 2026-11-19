//! The patient, study and acquisition identity every instance of an exported run shares.

use std::path::Path;

use dicom::dictionary_std::tags;
use dicom::object::{open_file, InMemDicomObject};
use log::{info, warn};

use super::{fmt_ds, make_uid};
use crate::bids::discovery::QsmRun;

/// The log target these messages had before they moved into this module.
const LOG_TARGET: &str = "qsmxt_io::dicom::export";

/// Identity/provenance fields shared by every instance of an exported run.
///
/// Either inherited from `--source-dicom` or synthesized. Series/SOP UIDs are
/// *not* here — those are generated per map/slice.
#[derive(Default, Clone)]
pub(super) struct DicomIdentity {
    pub(super) study_uid: String,
    pub(super) frame_of_reference_uid: String,
    pub(super) patient_name: String,
    pub(super) patient_id: String,
    pub(super) patient_birth_date: String,
    pub(super) patient_sex: String,
    pub(super) study_date: String,
    pub(super) study_time: String,
    pub(super) study_id: String,
    pub(super) accession_number: String,
    pub(super) referring_physician: String,
    pub(super) manufacturer: String,
    // MR attributes a viewer may read the series by (Type 2/2C): the source's, else empty.
    pub(super) patient_position: String,
    pub(super) laterality: String,
    pub(super) body_part: String,
    pub(super) repetition_time: String,
    pub(super) echo_train_length: String,
}

/// Build the run's shared DICOM identity, inheriting from `source_dicom` when set.
pub(super) fn build_identity(run: &QsmRun, source_dicom: Option<&Path>) -> DicomIdentity {
    let key = &run.key;
    // Synthesized defaults.
    let mut id = DicomIdentity {
        study_uid: make_uid(&[&key.subject, key.session.as_deref().unwrap_or("")]),
        frame_of_reference_uid: make_uid(&[&key.subject, key.session.as_deref().unwrap_or(""), "frame"]),
        manufacturer: "QSMxT".to_string(),
        ..Default::default()
    };

    // The acquisition fields a converter keeps in the BIDS sidecar (dcm2niix does by default),
    // for when there is no source DICOM; a source DICOM's own values win over them.
    // A dataset converted with `qsmxt dicom-convert --keep-dicom-uids` (or dcm2niix `-ba o`/`n`)
    // also keeps the study's UIDs there, so the series file into the original study.
    if let Some(sc) = run.echoes.first().and_then(|e| read_sidecar_acquisition(&e.phase_json)) {
        if !sc.study_uid.is_empty() { id.study_uid = sc.study_uid.clone(); }
        if !sc.frame_of_reference_uid.is_empty() { id.frame_of_reference_uid = sc.frame_of_reference_uid.clone(); }
        if !sc.study_id.is_empty() { id.study_id = sc.study_id.clone(); }
        id.apply_acquisition(sc);
    }

    if let Some(dir) = source_dicom {
        match read_source_template(dir) {
            Some(src) => {
                info!(target: LOG_TARGET, "DICOM export: inheriting patient/study identity from {}", dir.display());
                id.apply_acquisition(src.clone());
                if !src.study_uid.is_empty() { id.study_uid = src.study_uid; }
                if !src.frame_of_reference_uid.is_empty() { id.frame_of_reference_uid = src.frame_of_reference_uid; }
                id.patient_name = src.patient_name;
                id.patient_id = src.patient_id;
                id.patient_birth_date = src.patient_birth_date;
                id.patient_sex = src.patient_sex;
                id.study_date = src.study_date;
                id.study_time = src.study_time;
                if !src.study_id.is_empty() { id.study_id = src.study_id; }
                id.accession_number = src.accession_number;
                id.referring_physician = src.referring_physician;
            }
            None => warn!(
                target: LOG_TARGET,
                "DICOM export: no readable DICOM found under --source-dicom {}; synthesizing identity",
                dir.display()
            ),
        }
    }

    id
}

impl DicomIdentity {
    /// Take `other`'s manufacturer and acquisition fields where it has them, keeping ours where
    /// it does not.
    fn apply_acquisition(&mut self, other: DicomIdentity) {
        let take = |mine: &mut String, theirs: String| if !theirs.is_empty() { *mine = theirs };
        take(&mut self.manufacturer, other.manufacturer);
        take(&mut self.patient_position, other.patient_position);
        take(&mut self.laterality, other.laterality);
        take(&mut self.body_part, other.body_part);
        take(&mut self.repetition_time, other.repetition_time);
        take(&mut self.echo_train_length, other.echo_train_length);
    }
}

/// The acquisition fields of a BIDS sidecar, under the names BIDS and dcm2niix give them:
/// `RepetitionTime` (BIDS, seconds; DICOM wants ms), `EchoTrainLength`, `PatientPosition` (the
/// DICOM keywords, as dcm2niix writes them), `BodyPart` (BIDS; DICOM's Body Part Examined) and
/// `Manufacturer`; and the study's `StudyInstanceUID`, `FrameOfReferenceUID` and `StudyID` when
/// the conversion kept them. Never patient identity. `None` when the file cannot be read.
fn read_sidecar_acquisition(path: &Path) -> Option<DicomIdentity> {
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let text = |k: &str| json.get(k).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).unwrap_or_default();
    let number = |k: &str| json.get(k).and_then(|v| v.as_f64());
    Some(DicomIdentity {
        manufacturer: text("Manufacturer"),
        patient_position: text("PatientPosition"),
        body_part: text("BodyPart"),
        repetition_time: number("RepetitionTime").map(|s| fmt_ds(s * 1000.0)).unwrap_or_default(),
        echo_train_length: number("EchoTrainLength").map(|n| format!("{}", n.round() as i64)).unwrap_or_default(),
        study_uid: text("StudyInstanceUID"),
        frame_of_reference_uid: text("FrameOfReferenceUID"),
        study_id: text("StudyID"),
        ..Default::default()
    })
}

/// Read identity tags from the first readable DICOM under `dir` (recursive).
fn read_source_template(dir: &Path) -> Option<DicomIdentity> {
    let path = find_first_dicom(dir)?;
    let obj = open_file(&path).ok()?;
    let get = |tag| str_tag(&obj, tag);
    Some(DicomIdentity {
        study_uid: get(tags::STUDY_INSTANCE_UID),
        frame_of_reference_uid: get(tags::FRAME_OF_REFERENCE_UID),
        patient_name: get(tags::PATIENT_NAME),
        patient_id: get(tags::PATIENT_ID),
        patient_birth_date: get(tags::PATIENT_BIRTH_DATE),
        patient_sex: get(tags::PATIENT_SEX),
        study_date: get(tags::STUDY_DATE),
        study_time: get(tags::STUDY_TIME),
        study_id: get(tags::STUDY_ID),
        accession_number: get(tags::ACCESSION_NUMBER),
        referring_physician: get(tags::REFERRING_PHYSICIAN_NAME),
        manufacturer: get(tags::MANUFACTURER),
        patient_position: get(tags::PATIENT_POSITION),
        laterality: get(tags::LATERALITY),
        body_part: get(tags::BODY_PART_EXAMINED),
        repetition_time: get(tags::REPETITION_TIME),
        echo_train_length: get(tags::ECHO_TRAIN_LENGTH),
    })
}

/// Read a string element, returning "" if absent or unreadable.
fn str_tag(obj: &dicom::object::FileDicomObject<InMemDicomObject>, tag: dicom::core::Tag) -> String {
    obj.element(tag)
        .ok()
        .and_then(|e| e.to_str().ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Recursively find the first file that parses as a DICOM object.
fn find_first_dicom(dir: &Path) -> Option<std::path::PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).ok()?;
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                files.push(p);
            }
        }
        files.sort();
        for f in files {
            if open_file(&f).is_ok() {
                return Some(f);
            }
        }
    }
    None
}
