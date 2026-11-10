//! Export final QSM maps as DICOM series.
//!
//! When `--export-dicom` is enabled, each completed run's final NIfTI maps are
//! additionally written as a DICOM series (one instance per axial slice) under
//! `sub-X/[ses-Y/]extra_files/<basename>_<suffix>_dicoms/`.
//!
//! The DICOMs are written as *MR Image Storage* instances, synthesized from the
//! output volume plus the run's geometry and acquisition metadata. Floating-point
//! map values are linearly rescaled into 16-bit integers with
//! `RescaleSlope`/`RescaleIntercept` so a viewer recovers the true values (e.g.
//! susceptibility in ppm).
//!
//! When `--source-dicom <dir>` is given, patient/study identity (PatientID,
//! StudyInstanceUID, FrameOfReferenceUID, etc.) is inherited from a representative
//! original DICOM so the derived series slot alongside the source study in a PACS.
//! Otherwise that identity is synthesized (patient fields left empty; a stable
//! StudyInstanceUID is derived from the subject/session). New Series/SOP UIDs are
//! always generated — these are distinct derived series.
//!
//! The run's segmentation goes out as a DICOM Segmentation object referencing the χ series
//! ([`seg`]).

mod seg;

use std::hash::{Hash, Hasher};
use std::path::Path;

use dicom::core::value::PrimitiveValue;
use dicom::core::{DataElement, VR};
use dicom::dictionary_std::{tags, uids};
use dicom::object::meta::FileMetaTableBuilder;
use dicom::object::{open_file, InMemDicomObject};
use log::{info, warn};

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::error::QsmxtError;
use qsm_core::io;

/// Root OID prefix for synthesized UIDs. `2.25` is the DICOM-sanctioned arc for
/// UUID-derived UIDs; we substitute a stable hash of the run identity so repeat
/// exports of the same run are reproducible.
const UID_ROOT: &str = "2.25";

/// Options controlling DICOM export for a run.
pub struct DicomExportOptions<'a> {
    /// Export the SWI and minIP maps if present.
    pub export_swi: bool,
    /// Export the SMWI maps and their minIPs (paramagnetic and diamagnetic) if present.
    pub export_smwi: bool,
    /// Export the T2* map if present.
    pub export_t2star: bool,
    /// Export the R2* map if present.
    pub export_r2star: bool,
    /// Export the R2 map (from MESE) if present.
    pub export_r2: bool,
    /// Export the R2' map if present.
    pub export_r2prime: bool,
    /// Export the chi-separation maps (paramagnetic / diamagnetic / total) if present.
    pub export_chisep: bool,
    /// Optional source DICOM directory to inherit patient/study identity from.
    pub source_dicom: Option<&'a Path>,
    /// Optional subset of map suffixes to export (case-insensitive tokens like
    /// `chimap`, `swi`, `minip`, `desc-paramagnetic_smwi`, `desc-paramagneticsmwi_minip` (and
    /// the diamagnetic pair), `t2starmap`, `r2starmap`, `r2map`, `r2primemap`,
    /// `desc-paramagnetic_chimap`, `desc-diamagnetic_chimap`, `desc-total_chimap`, `dseg`).
    /// `None` = all produced.
    pub outputs_filter: Option<&'a [String]>,
    /// Export the segmentation as a DICOM SEG, when the run makes or reads one.
    pub segmentation: Option<SegmentationExport>,
}

/// How the run's segmentation is to be described in its DICOM SEG.
pub struct SegmentationExport {
    /// The labels are SynthSeg's FreeSurfer ids, which have standard codes; otherwise (a supplied
    /// segmentation) each label is coded generically and named from the `dseg.tsv`.
    pub synthseg: bool,
    /// What made it: "SynthSeg 1.0", or the derivatives tool a supplied one came from.
    pub algorithm: String,
}

/// The SMWI series' suffixes, (SMWI, its minIP), paramagnetic then diamagnetic: the published
/// names' tails ([`qsmxt_engine::bids::naming::smwi`], `smwi_minip`).
const SMWI_SUFFIXES: [(&str, &str); 2] = [
    ("desc-paramagnetic_smwi", "desc-paramagneticsmwi_minIP"),
    ("desc-diamagnetic_smwi", "desc-diamagneticsmwi_minIP"),
];

/// A final map eligible for DICOM export.
struct MapExport {
    /// BIDS suffix (also used in the output folder name), e.g. "Chimap".
    suffix: &'static str,
    /// Path to the final NIfTI, if it exists.
    path: std::path::PathBuf,
    /// Human-readable series description.
    description: String,
}

/// Identity/provenance fields shared by every instance of an exported run.
///
/// Either inherited from `--source-dicom` or synthesized. Series/SOP UIDs are
/// *not* here — those are generated per map/slice.
#[derive(Default, Clone)]
struct DicomIdentity {
    study_uid: String,
    frame_of_reference_uid: String,
    patient_name: String,
    patient_id: String,
    patient_birth_date: String,
    patient_sex: String,
    study_date: String,
    study_time: String,
    study_id: String,
    accession_number: String,
    referring_physician: String,
    manufacturer: String,
    // MR attributes a viewer may read the series by (Type 2/2C): the source's, else empty.
    patient_position: String,
    laterality: String,
    body_part: String,
    repetition_time: String,
    echo_train_length: String,
}

/// Export all enabled final maps for a completed run as DICOM series.
///
/// Geometry (dims, voxel size, affine) is read from each output NIfTI so it
/// reflects any resampling applied during processing; acquisition metadata
/// (echo time, field strength) comes from the run.
///
/// Returns how many series were written: none when no map it was set to export exists. Errors
/// from a single map are propagated; callers may choose to treat DICOM export as best-effort (see
/// the runner hook).
pub fn export_run_dicoms(
    run: &QsmRun,
    output: &DerivativeOutputs,
    opts: &DicomExportOptions,
) -> crate::Result<usize> {
    let key = &run.key;
    // The names the run published under (its working space, when it stayed there).
    let output = &output.published(key);
    let mut maps: Vec<MapExport> = vec![MapExport {
        suffix: "Chimap",
        path: output.qsm_path(key),
        description: "QSMxT Chimap".to_string(),
    }];
    if opts.export_swi {
        maps.push(MapExport { suffix: "swi", path: output.swi_path(key), description: "QSMxT SWI".to_string() });
        maps.push(MapExport { suffix: "minIP", path: output.swi_mip_path(key), description: "QSMxT SWI minIP".to_string() });
    }
    if opts.export_smwi {
        use qsmxt_engine::store::Contrast;
        for (contrast, name) in [(Contrast::Paramagnetic, "paramagnetic"), (Contrast::Diamagnetic, "diamagnetic")] {
            let (smwi, minip) = SMWI_SUFFIXES[usize::from(contrast == Contrast::Diamagnetic)];
            maps.push(MapExport { suffix: smwi, path: output.smwi_path(key, contrast), description: format!("QSMxT {name} SMWI") });
            maps.push(MapExport { suffix: minip, path: output.smwi_mip_path(key, contrast), description: format!("QSMxT {name} SMWI minIP") });
        }
    }
    if opts.export_t2star {
        maps.push(MapExport { suffix: "T2starmap", path: output.t2star_path(key), description: "QSMxT T2* map".to_string() });
    }
    if opts.export_r2star {
        maps.push(MapExport { suffix: "R2starmap", path: output.r2star_path(key), description: "QSMxT R2* map".to_string() });
    }
    if opts.export_r2 {
        maps.push(MapExport { suffix: "R2map", path: output.r2_path(key), description: "QSMxT R2 map".to_string() });
    }
    if opts.export_r2prime {
        maps.push(MapExport { suffix: "R2primemap", path: output.r2prime_path(key), description: "QSMxT R2' map".to_string() });
    }
    if opts.export_chisep {
        maps.push(MapExport { suffix: "desc-paramagnetic_Chimap", path: output.chi_para_path(key), description: "QSMxT paramagnetic chi".to_string() });
        maps.push(MapExport { suffix: "desc-diamagnetic_Chimap", path: output.chi_dia_path(key), description: "QSMxT diamagnetic chi".to_string() });
        maps.push(MapExport { suffix: "desc-total_Chimap", path: output.chi_sep_total_path(key), description: "QSMxT total chi".to_string() });
    }

    // Apply the optional output subset filter.
    if let Some(filter) = opts.outputs_filter {
        maps.retain(|m| suffix_selected(m.suffix, filter));
    }

    // Build the shared identity — inherited from source DICOM or synthesized.
    let identity = build_identity(run, opts.source_dicom);
    let field_strength = run.magnetic_field_strength;
    let first_te = run.echo_times.first().copied();

    let mut written = 0;
    let mut chi_exported = false;
    for map in &maps {
        if !map.path.exists() {
            continue;
        }
        let dir = output.dicom_dir(key, map.suffix);
        let n = export_map(ExportMap {
            nifti_path: &map.path,
            out_dir: &dir,
            description: &map.description,
            suffix: map.suffix,
            identity: &identity,
            field_strength,
            first_te,
            key,
        })?;
        info!("{}: -> {} ({} slices)", key, dir.display(), n);
        written += 1;
        chi_exported |= map.suffix == "Chimap";
    }

    if let Some(seg) = &opts.segmentation {
        let selected = opts.outputs_filter.is_none_or(|f| suffix_selected("dseg", f));
        let dseg = output.dseg_path(key);
        if selected && dseg.exists() && export_segmentation(key, output, seg, &dseg, &identity, chi_exported) {
            written += 1;
        }
    }

    Ok(written)
}

/// The segmentation as a DICOM SEG on the exported χ series; whether one was written. Best-effort:
/// the maps are out already, so a problem here is a warning, not a failed export.
fn export_segmentation(
    key: &crate::bids::entities::AcquisitionKey, output: &DerivativeOutputs, seg: &SegmentationExport,
    dseg: &Path, identity: &DicomIdentity, chi_exported: bool,
) -> bool {
    let chi = output.qsm_path(key);
    if !chi_exported {
        warn!("{key}: the segmentation is not exported as DICOM: its SEG references the χ series, which this run does not export");
        return false;
    }
    let reference = match io::read_nifti_file(&chi) {
        Ok(v) => v,
        Err(e) => {
            warn!("{key}: DICOM SEG not written: {}: {e}", chi.display());
            return false;
        }
    };
    let lookup = output.dseg_lookup_path(key);
    let out_dir = output.dicom_dir(key, "dseg");
    let result = seg::export_segmentation(seg::SegExport {
        source: seg::SegSource {
            dseg,
            lookup: lookup.exists().then_some(lookup.as_path()),
            synthseg: seg.synthseg,
            algorithm: &seg.algorithm,
        },
        reference: seg::Reference {
            series_uid: &make_uid(&[&key.basename(), "Chimap"]),
            dims: reference.dims,
            affine: &reference.affine,
        },
        identity,
        key,
        out_dir: &out_dir,
    });
    match result {
        Ok(0) => {
            warn!("{key}: DICOM SEG not written: the segmentation has no labelled voxel");
            false
        }
        Ok(n) => {
            info!("{}: -> {} ({} segments)", key, out_dir.display(), n);
            true
        }
        Err(e) => {
            warn!("{key}: DICOM SEG not written: {e}");
            false
        }
    }
}

/// Build the run's shared DICOM identity, inheriting from `source_dicom` when set.
fn build_identity(run: &QsmRun, source_dicom: Option<&Path>) -> DicomIdentity {
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
    if let Some(sc) = run.echoes.first().and_then(|e| read_sidecar_acquisition(&e.phase_json)) {
        id.apply_acquisition(sc);
    }

    if let Some(dir) = source_dicom {
        match read_source_template(dir) {
            Some(src) => {
                info!("DICOM export: inheriting patient/study identity from {}", dir.display());
                id.apply_acquisition(src.clone());
                if !src.study_uid.is_empty() { id.study_uid = src.study_uid; }
                if !src.frame_of_reference_uid.is_empty() { id.frame_of_reference_uid = src.frame_of_reference_uid; }
                id.patient_name = src.patient_name;
                id.patient_id = src.patient_id;
                id.patient_birth_date = src.patient_birth_date;
                id.patient_sex = src.patient_sex;
                id.study_date = src.study_date;
                id.study_time = src.study_time;
                id.study_id = src.study_id;
                id.accession_number = src.accession_number;
                id.referring_physician = src.referring_physician;
            }
            None => warn!(
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
/// `Manufacturer`. `None` when the file cannot be read.
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

struct ExportMap<'a> {
    nifti_path: &'a Path,
    out_dir: &'a Path,
    description: &'a str,
    suffix: &'a str,
    identity: &'a DicomIdentity,
    field_strength: f64,
    first_te: Option<f64>,
    key: &'a crate::bids::entities::AcquisitionKey,
}

/// Export one map (one NIfTI volume) as a DICOM series of axial slices.
/// Returns the number of slices written.
fn export_map(m: ExportMap) -> crate::Result<usize> {
    let volume = io::read_nifti_file(m.nifti_path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", m.nifti_path.display(), e)))?;

    let (nx, ny, nz) = volume.dims;
    let slice_len = nx * ny;
    if slice_len == 0 || volume.data.len() < slice_len * nz {
        return Err(QsmxtError::Dicom(format!(
            "{}: volume has {} voxels, expected at least {}",
            m.nifti_path.display(),
            volume.data.len(),
            slice_len * nz
        )));
    }

    std::fs::create_dir_all(m.out_dir)?;

    // Per-volume linear rescale of f64 -> u16 [0, 65535].
    let (slope, intercept) = rescale_params(&volume.data);

    let geometry = SliceGeometry::from_affine(&volume.affine, volume.voxel_size);
    let series_uid = make_uid(&[&m.key.basename(), m.suffix]);
    let series_number = series_number_for(m.suffix);

    for k in 0..nz {
        let start = k * slice_len;
        let slice = &volume.data[start..start + slice_len];
        let pixels: Vec<u16> = slice
            .iter()
            .map(|&v| (((v - intercept) / slope).round()).clamp(0.0, 65535.0) as u16)
            .collect();

        let sop_uid = seg::slice_uid(&series_uid, k);
        let obj = build_slice(BuildSlice {
            pixels,
            nx,
            ny,
            k,
            slope,
            intercept,
            description: m.description,
            suffix: m.suffix,
            series_uid: &series_uid,
            sop_uid: &sop_uid,
            identity: m.identity,
            series_number,
            geometry: &geometry,
            field_strength: m.field_strength,
            first_te: m.first_te,
        });

        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid(uids::MR_IMAGE_STORAGE)
                    .media_storage_sop_instance_uid(sop_uid.as_str()),
            )
            .map_err(|e| QsmxtError::Dicom(format!("build file meta: {}", e)))?;

        let path = m.out_dir.join(format!("{}_{}_{:04}.dcm", m.key.basename(), m.suffix, k + 1));
        file_obj
            .write_to_file(&path)
            .map_err(|e| QsmxtError::Dicom(format!("{}: {}", path.display(), e)))?;
    }

    Ok(nz)
}

struct BuildSlice<'a> {
    pixels: Vec<u16>,
    nx: usize,
    ny: usize,
    k: usize,
    slope: f64,
    intercept: f64,
    description: &'a str,
    suffix: &'a str,
    series_uid: &'a str,
    sop_uid: &'a str,
    identity: &'a DicomIdentity,
    series_number: i32,
    geometry: &'a SliceGeometry,
    field_strength: f64,
    first_te: Option<f64>,
}

/// Build a single MR Image Storage slice as an in-memory DICOM object.
fn build_slice(s: BuildSlice) -> InMemDicomObject {
    let id = s.identity;
    let mut obj = InMemDicomObject::new_empty();

    // SOP Common
    obj.put(DataElement::new(tags::SOP_CLASS_UID, VR::UI, uids::MR_IMAGE_STORAGE));
    obj.put(DataElement::new(tags::SOP_INSTANCE_UID, VR::UI, s.sop_uid));
    // DERIVED (computed), SECONDARY (not the original acquisition), and the MR image flavour: a
    // minimum-intensity projection is a PROJECTION IMAGE; no other defined term fits a QSMxT map
    // (T2 MAP is T2, not T2*), so OTHER.
    let flavour = if s.suffix.ends_with("minIP") { "PROJECTION IMAGE" } else { "OTHER" };
    obj.put(DataElement::new(tags::IMAGE_TYPE, VR::CS, PrimitiveValue::Strs(
        ["DERIVED", "SECONDARY", flavour].into_iter().map(str::to_string).collect())));

    // Patient module (Type 2 — present, possibly empty).
    obj.put(DataElement::new(tags::PATIENT_NAME, VR::PN, id.patient_name.as_str()));
    obj.put(DataElement::new(tags::PATIENT_ID, VR::LO, id.patient_id.as_str()));
    obj.put(DataElement::new(tags::PATIENT_BIRTH_DATE, VR::DA, id.patient_birth_date.as_str()));
    obj.put(DataElement::new(tags::PATIENT_SEX, VR::CS, id.patient_sex.as_str()));

    // General Study module
    obj.put(DataElement::new(tags::STUDY_INSTANCE_UID, VR::UI, id.study_uid.as_str()));
    obj.put(DataElement::new(tags::STUDY_DATE, VR::DA, id.study_date.as_str()));
    obj.put(DataElement::new(tags::STUDY_TIME, VR::TM, id.study_time.as_str()));
    obj.put(DataElement::new(tags::REFERRING_PHYSICIAN_NAME, VR::PN, id.referring_physician.as_str()));
    obj.put(DataElement::new(tags::STUDY_ID, VR::SH, id.study_id.as_str()));
    obj.put(DataElement::new(tags::ACCESSION_NUMBER, VR::SH, id.accession_number.as_str()));

    // General Series / Frame of Reference / Equipment
    obj.put(DataElement::new(tags::SERIES_INSTANCE_UID, VR::UI, s.series_uid));
    obj.put(DataElement::new(tags::MODALITY, VR::CS, "MR"));
    obj.put(DataElement::new(tags::SERIES_DESCRIPTION, VR::LO, s.description));
    obj.put(DataElement::new(tags::SERIES_NUMBER, VR::IS, s.series_number.to_string()));
    // Type 2C. Laterality is for a paired body part: the source's when it has one; absent when the
    // source names an (unpaired) body part, which is then copied; else present and empty, as
    // QSMxT does not know the body part. The patient position is the source's or empty.
    if !id.laterality.is_empty() || id.body_part.is_empty() {
        obj.put(DataElement::new(tags::LATERALITY, VR::CS, id.laterality.as_str()));
    }
    if !id.body_part.is_empty() {
        obj.put(DataElement::new(tags::BODY_PART_EXAMINED, VR::CS, id.body_part.as_str()));
    }
    obj.put(DataElement::new(tags::PATIENT_POSITION, VR::CS, id.patient_position.as_str()));
    obj.put(DataElement::new(tags::FRAME_OF_REFERENCE_UID, VR::UI, id.frame_of_reference_uid.as_str()));
    obj.put(DataElement::new(tags::POSITION_REFERENCE_INDICATOR, VR::LO, ""));
    obj.put(DataElement::new(tags::MANUFACTURER, VR::LO, id.manufacturer.as_str()));
    obj.put(DataElement::new(tags::INSTANCE_NUMBER, VR::IS, (s.k + 1).to_string()));

    // MR Image module (Type 1 basics for a derived image).
    obj.put(DataElement::new(tags::SCANNING_SEQUENCE, VR::CS, "RM")); // Research Mode
    obj.put(DataElement::new(tags::SEQUENCE_VARIANT, VR::CS, "NONE"));
    obj.put(DataElement::new(tags::SCAN_OPTIONS, VR::CS, ""));
    obj.put(DataElement::new(tags::MR_ACQUISITION_TYPE, VR::CS, "3D"));
    // Type 2C/2: the acquisition's, when --source-dicom gives them; QSMxT does not know them
    // otherwise (BIDS sidecars need not carry them), so empty rather than guessed.
    obj.put(DataElement::new(tags::REPETITION_TIME, VR::DS, id.repetition_time.as_str()));
    obj.put(DataElement::new(tags::ECHO_TRAIN_LENGTH, VR::IS, id.echo_train_length.as_str()));

    // Acquisition metadata (best-effort).
    if s.field_strength > 0.0 {
        obj.put(DataElement::new(tags::MAGNETIC_FIELD_STRENGTH, VR::DS, fmt_ds(s.field_strength)));
    }
    if let Some(te) = s.first_te {
        // BIDS/qsmxt echo times are seconds; DICOM EchoTime is milliseconds.
        obj.put(DataElement::new(tags::ECHO_TIME, VR::DS, fmt_ds(te * 1000.0)));
    }

    // Geometry
    obj.put(DataElement::new(
        tags::IMAGE_ORIENTATION_PATIENT,
        VR::DS,
        PrimitiveValue::Strs(s.geometry.orientation.iter().map(|v| fmt_ds(*v)).collect()),
    ));
    obj.put(DataElement::new(
        tags::IMAGE_POSITION_PATIENT,
        VR::DS,
        PrimitiveValue::Strs(s.geometry.position(s.k).iter().map(|v| fmt_ds(*v)).collect()),
    ));
    obj.put(DataElement::new(
        tags::PIXEL_SPACING,
        VR::DS,
        PrimitiveValue::Strs([fmt_ds(s.geometry.row_spacing), fmt_ds(s.geometry.col_spacing)].into_iter().collect()),
    ));
    obj.put(DataElement::new(tags::SLICE_THICKNESS, VR::DS, fmt_ds(s.geometry.slice_thickness)));
    obj.put(DataElement::new(tags::SLICE_LOCATION, VR::DS, fmt_ds(s.geometry.slice_location(s.k))));

    // Pixel module
    obj.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, PrimitiveValue::U16([1].as_ref().into())));
    obj.put(DataElement::new(tags::PHOTOMETRIC_INTERPRETATION, VR::CS, "MONOCHROME2"));
    obj.put(DataElement::new(tags::ROWS, VR::US, PrimitiveValue::U16([s.ny as u16].as_ref().into())));
    obj.put(DataElement::new(tags::COLUMNS, VR::US, PrimitiveValue::U16([s.nx as u16].as_ref().into())));
    obj.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, PrimitiveValue::U16([16].as_ref().into())));
    obj.put(DataElement::new(tags::BITS_STORED, VR::US, PrimitiveValue::U16([16].as_ref().into())));
    obj.put(DataElement::new(tags::HIGH_BIT, VR::US, PrimitiveValue::U16([15].as_ref().into())));
    obj.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, PrimitiveValue::U16([0].as_ref().into())));

    // Rescale so viewers recover the true map values.
    obj.put(DataElement::new(tags::RESCALE_INTERCEPT, VR::DS, fmt_ds(s.intercept)));
    obj.put(DataElement::new(tags::RESCALE_SLOPE, VR::DS, fmt_ds(s.slope)));

    obj.put(DataElement::new(tags::PIXEL_DATA, VR::OW, PrimitiveValue::U16(s.pixels.into())));

    obj
}

/// Direction cosines, spacings, and per-slice position derived from the affine.
struct SliceGeometry {
    /// ImageOrientationPatient: [row(x) cosines (i-dir), col(y) cosines (j-dir)], LPS.
    orientation: [f64; 6],
    /// Position of voxel (0,0,0) in LPS mm.
    origin: [f64; 3],
    /// Per-slice step vector (k-direction) in LPS mm.
    slice_dir: [f64; 3],
    row_spacing: f64,
    col_spacing: f64,
    slice_thickness: f64,
}

impl SliceGeometry {
    /// Derive geometry from a NIfTI (RAS) row-major 4x4 affine.
    ///
    /// NIfTI world coordinates are RAS; DICOM patient coordinates are LPS, so the
    /// x and y components are negated.
    fn from_affine(affine: &[f64; 16], voxel: (f64, f64, f64)) -> Self {
        // Columns of the 3x3 direction/scale matrix (voxel-step vectors), RAS.
        let i_col = [affine[0], affine[4], affine[8]];
        let j_col = [affine[1], affine[5], affine[9]];
        let k_col = [affine[2], affine[6], affine[10]];
        let origin_ras = [affine[3], affine[7], affine[11]];

        let col_spacing = norm(&i_col).max(voxel.0);
        let row_spacing = norm(&j_col).max(voxel.1);
        let slice_thickness = norm(&k_col).max(voxel.2);

        let i_unit = to_lps(&unit(&i_col));
        let j_unit = to_lps(&unit(&j_col));
        let slice_dir = to_lps(&k_col);
        let origin = to_lps(&origin_ras);

        Self {
            orientation: [i_unit[0], i_unit[1], i_unit[2], j_unit[0], j_unit[1], j_unit[2]],
            origin,
            slice_dir,
            row_spacing,
            col_spacing,
            slice_thickness,
        }
    }

    fn position(&self, k: usize) -> [f64; 3] {
        [
            self.origin[0] + self.slice_dir[0] * k as f64,
            self.origin[1] + self.slice_dir[1] * k as f64,
            self.origin[2] + self.slice_dir[2] * k as f64,
        ]
    }

    fn slice_location(&self, k: usize) -> f64 {
        // Projection of the slice position onto the slice normal.
        let pos = self.position(k);
        dot(&pos, &unit(&self.slice_dir))
    }
}

fn to_lps(v: &[f64; 3]) -> [f64; 3] {
    [-v[0], -v[1], v[2]]
}

fn norm(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn unit(v: &[f64; 3]) -> [f64; 3] {
    let n = norm(v);
    if n <= f64::EPSILON {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Compute a linear map value -> integer: `pixel = (value - intercept) / slope`,
/// spanning the data range across [0, 65535].
fn rescale_params(data: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in data {
        if v.is_finite() {
            if v < min { min = v; }
            if v > max { max = v; }
        }
    }
    if !min.is_finite() || !max.is_finite() || (max - min) <= 0.0 {
        // Constant or empty volume: unit slope, intercept at the constant value
        // (so every voxel maps to 0), avoiding divide-by-zero.
        let intercept = if min.is_finite() { min } else { 0.0 };
        return (1.0, intercept);
    }
    let slope = (max - min) / 65535.0;
    (slope, min)
}

/// Format an f64 as a DICOM DS (decimal string), max 16 chars.
fn fmt_ds(v: f64) -> String {
    let mut s = format!("{:.6}", v);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.len() > 16 {
        s = format!("{:.6e}", v);
    }
    s
}

/// Whether a map suffix is selected by a `--dicom-outputs` filter (case-insensitive).
fn suffix_selected(suffix: &str, filter: &[String]) -> bool {
    filter.iter().any(|f| f.eq_ignore_ascii_case(suffix))
}

/// Stable series numbers per map so viewers order them predictably.
fn series_number_for(suffix: &str) -> i32 {
    match suffix {
        "Chimap" => 1,
        "swi" => 2,
        "minIP" => 3,
        "T2starmap" => 4,
        "R2starmap" => 5,
        "R2map" => 6,
        "R2primemap" => 7,
        "desc-paramagnetic_Chimap" => 8,
        "desc-diamagnetic_Chimap" => 9,
        "desc-total_Chimap" => 10,
        "desc-paramagnetic_smwi" => 11,
        "desc-diamagnetic_smwi" => 12,
        "desc-paramagneticsmwi_minIP" => 13,
        "desc-diamagneticsmwi_minIP" => 14,
        "dseg" => 15,
        _ => 99,
    }
}

/// Build a valid, reproducible UID under `UID_ROOT` from string components.
fn make_uid(parts: &[&str]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut hasher);
        0u8.hash(&mut hasher); // separator to avoid collisions
    }
    let h = hasher.finish();
    format!("{}.{}", UID_ROOT, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_ds_trims_zeros() {
        assert_eq!(fmt_ds(1.5), "1.5");
        assert_eq!(fmt_ds(3.0), "3");
        assert_eq!(fmt_ds(0.001), "0.001");
    }

    #[test]
    fn test_fmt_ds_len_bounded() {
        assert!(fmt_ds(123456789.123456).len() <= 16);
        assert!(fmt_ds(-0.000000123456).len() <= 16);
    }

    #[test]
    fn test_rescale_spans_range() {
        let data = vec![-0.1, 0.0, 0.1];
        let (slope, intercept) = rescale_params(&data);
        assert!((intercept - (-0.1)).abs() < 1e-12);
        // top of range maps to ~65535
        let top = ((0.1 - intercept) / slope).round();
        assert!((top - 65535.0).abs() < 1.0);
    }

    #[test]
    fn test_rescale_constant_volume() {
        let (slope, _) = rescale_params(&[2.0, 2.0, 2.0]);
        assert!(slope != 0.0);
    }

    #[test]
    fn test_make_uid_valid_and_stable() {
        let a = make_uid(&["01", "pre"]);
        let b = make_uid(&["01", "pre"]);
        assert_eq!(a, b);
        assert!(a.starts_with("2.25."));
        assert!(a.chars().all(|c| c.is_ascii_digit() || c == '.'));
        assert!(a.len() <= 64);
        // Different inputs differ.
        assert_ne!(a, make_uid(&["01", "post"]));
    }

    #[test]
    fn test_export_map_roundtrip() {
        use crate::bids::entities::AcquisitionKey;

        let tmp = tempfile::tempdir().unwrap();
        let nii = tmp.path().join("in.nii");
        let (nx, ny, nz) = (3usize, 4usize, 2usize);
        // Ramp data so the volume has a real range.
        let data: Vec<f64> = (0..nx * ny * nz).map(|i| i as f64 * 0.01 - 0.1).collect();
        let affine = [
            1.0, 0.0, 0.0, 5.0,
            0.0, 1.0, 0.0, 6.0,
            0.0, 0.0, 2.0, 7.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        io::save_nifti_to_file(&nii, &data, (nx, ny, nz), (1.0, 1.0, 2.0), &affine).unwrap();

        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let out_dir = tmp.path().join("dicoms");
        let identity = DicomIdentity {
            study_uid: "2.25.123".to_string(),
            frame_of_reference_uid: "2.25.456".to_string(),
            manufacturer: "QSMxT".to_string(),
            ..Default::default()
        };
        let n = export_map(ExportMap {
            nifti_path: &nii,
            out_dir: &out_dir,
            description: "QSMxT Chimap",
            suffix: "Chimap",
            identity: &identity,
            field_strength: 3.0,
            first_te: Some(0.02),
            key: &key,
        })
        .unwrap();
        assert_eq!(n, nz);

        // One .dcm per slice.
        let dcms: Vec<_> = std::fs::read_dir(&out_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "dcm"))
            .collect();
        assert_eq!(dcms.len(), nz);

        // Read one back and validate key attributes.
        let first = out_dir.join(format!("{}_Chimap_0001.dcm", key.basename()));
        let obj = open_file(&first).expect("valid DICOM");
        let rows = obj.element(tags::ROWS).unwrap().to_int::<u16>().unwrap();
        let cols = obj.element(tags::COLUMNS).unwrap().to_int::<u16>().unwrap();
        assert_eq!(rows, ny as u16);
        assert_eq!(cols, nx as u16);
        assert_eq!(obj.element(tags::MODALITY).unwrap().to_str().unwrap(), "MR");
        // Written as MR Image Storage.
        assert_eq!(obj.element(tags::SOP_CLASS_UID).unwrap().to_str().unwrap(), uids::MR_IMAGE_STORAGE);
        assert_eq!(obj.meta().media_storage_sop_class_uid.trim_end_matches('\0'), uids::MR_IMAGE_STORAGE);
        // Identity threaded through.
        assert_eq!(obj.element(tags::STUDY_INSTANCE_UID).unwrap().to_str().unwrap(), "2.25.123");
        assert_eq!(obj.element(tags::FRAME_OF_REFERENCE_UID).unwrap().to_str().unwrap(), "2.25.456");
        // Pixel data present and correctly sized (u16 => 2 bytes per voxel).
        let px = obj.element(tags::PIXEL_DATA).unwrap().to_bytes().unwrap();
        assert_eq!(px.len(), nx * ny * 2);
        // Rescale present.
        assert!(obj.element(tags::RESCALE_SLOPE).is_ok());
        // MR Image Type has its third value, and the Type 2/2C attributes are present, empty
        // without a source DICOM.
        let image_type: Vec<String> = obj.element(tags::IMAGE_TYPE).unwrap().to_multi_str().unwrap().to_vec();
        assert_eq!(image_type, ["DERIVED", "SECONDARY", "OTHER"]);
        for tag in [tags::LATERALITY, tags::PATIENT_POSITION, tags::REPETITION_TIME, tags::ECHO_TRAIN_LENGTH] {
            assert_eq!(obj.element(tag).unwrap().to_str().unwrap().trim(), "", "{tag}");
        }
    }

    #[test]
    fn test_source_dicom_inheritance() {
        use crate::bids::entities::AcquisitionKey;

        // Hand-build a minimal source DICOM carrying patient/study identity.
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let mut src = InMemDicomObject::new_empty();
        src.put(DataElement::new(tags::SOP_CLASS_UID, VR::UI, uids::MR_IMAGE_STORAGE));
        src.put(DataElement::new(tags::SOP_INSTANCE_UID, VR::UI, "2.25.999"));
        src.put(DataElement::new(tags::PATIENT_ID, VR::LO, "P123"));
        src.put(DataElement::new(tags::PATIENT_NAME, VR::PN, "Doe^Jane"));
        src.put(DataElement::new(tags::STUDY_INSTANCE_UID, VR::UI, "1.2.3.4"));
        src.put(DataElement::new(tags::FRAME_OF_REFERENCE_UID, VR::UI, "5.6.7.8"));
        src.put(DataElement::new(tags::MANUFACTURER, VR::LO, "ACME"));
        src.put(DataElement::new(tags::PATIENT_POSITION, VR::CS, "HFS"));
        src.put(DataElement::new(tags::REPETITION_TIME, VR::DS, "28"));
        src.put(DataElement::new(tags::ECHO_TRAIN_LENGTH, VR::IS, "4"));
        src.put(DataElement::new(tags::BODY_PART_EXAMINED, VR::CS, "HEAD"));
        src.with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid(uids::MR_IMAGE_STORAGE)
                .media_storage_sop_instance_uid("2.25.999"),
        )
        .unwrap()
        .write_to_file(src_dir.join("template.dcm"))
        .unwrap();

        // A minimal run.
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let run = crate::bids::discovery::QsmRun {
            key,
            echoes: vec![],
            coils: None,
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02],
            b0_dir: None,
            dims: (2, 2, 2),
            has_magnitude: true,
            mese: None,
        };

        let id = build_identity(&run, Some(&src_dir));
        assert_eq!(id.patient_id, "P123");
        assert_eq!(id.patient_name, "Doe^Jane");
        assert_eq!(id.study_uid, "1.2.3.4");
        assert_eq!(id.frame_of_reference_uid, "5.6.7.8");
        assert_eq!(id.manufacturer, "ACME");
        assert_eq!((id.patient_position.as_str(), id.repetition_time.as_str(), id.echo_train_length.as_str()), ("HFS", "28", "4"));
        assert!(id.laterality.is_empty(), "absent from the source: empty");
        assert_eq!(id.body_part, "HEAD");

        // Without a source, identity is synthesized (empty patient, non-empty UIDs).
        let synth = build_identity(&run, None);
        assert!(synth.patient_id.is_empty());
        assert!(synth.study_uid.starts_with("2.25."));
        assert_eq!(synth.manufacturer, "QSMxT");
    }

    /// Without a source DICOM the acquisition fields come from the run's sidecar (dcm2niix's
    /// names, TR in seconds); a source DICOM's values win where it has them.
    #[test]
    fn acquisition_fields_come_from_the_sidecar_unless_a_source_dicom_has_them() {
        use crate::bids::entities::AcquisitionKey;
        let tmp = tempfile::tempdir().unwrap();
        let json = tmp.path().join("echo-1_part-phase_MEGRE.json");
        std::fs::write(&json, r#"{"RepetitionTime": 0.028, "EchoTrainLength": 4, "PatientPosition": "HFS",
            "BodyPart": "BRAIN", "Manufacturer": "Siemens", "EchoTime": 0.004}"#).unwrap();
        let run = crate::bids::discovery::QsmRun {
            key: AcquisitionKey { subject: "01".into(), session: None, acquisition: None, reconstruction: None,
                                  inversion: None, run: None, suffix: "MEGRE".into() },
            echoes: vec![crate::bids::discovery::EchoFiles {
                echo_number: 1, phase_nifti: tmp.path().join("p.nii"), phase_json: json,
                magnitude_nifti: None, magnitude_json: None, imaginary_nifti: None,
            }],
            coils: None, magnetic_field_strength: 3.0, echo_times: vec![0.004], b0_dir: None,
            dims: (2, 2, 2), has_magnitude: false, mese: None,
        };
        let id = build_identity(&run, None);
        assert_eq!((id.repetition_time.as_str(), id.echo_train_length.as_str()), ("28", "4"));
        assert_eq!((id.patient_position.as_str(), id.body_part.as_str(), id.manufacturer.as_str()), ("HFS", "BRAIN", "Siemens"));
        assert!(id.patient_id.is_empty(), "no identity from a sidecar");

        // A source DICOM with only some of them: its values win, the sidecar fills the rest.
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let mut src = InMemDicomObject::new_empty();
        src.put(DataElement::new(tags::SOP_CLASS_UID, VR::UI, uids::MR_IMAGE_STORAGE));
        src.put(DataElement::new(tags::SOP_INSTANCE_UID, VR::UI, "2.25.999"));
        src.put(DataElement::new(tags::REPETITION_TIME, VR::DS, "30"));
        src.with_meta(FileMetaTableBuilder::new().transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
            .media_storage_sop_class_uid(uids::MR_IMAGE_STORAGE).media_storage_sop_instance_uid("2.25.999"))
            .unwrap().write_to_file(src_dir.join("t.dcm")).unwrap();
        let id = build_identity(&run, Some(&src_dir));
        assert_eq!((id.repetition_time.as_str(), id.echo_train_length.as_str(), id.body_part.as_str()), ("30", "4", "BRAIN"));
    }

    #[test]
    fn test_suffix_selected() {
        let filter = vec!["chimap".to_string(), "T2starmap".to_string()];
        assert!(suffix_selected("Chimap", &filter));
        assert!(suffix_selected("T2starmap", &filter));
        assert!(!suffix_selected("swi", &filter));
        assert!(!suffix_selected("R2starmap", &filter));
    }

    #[test]
    fn test_series_numbers_unique_for_all_maps() {
        // Every exportable suffix maps to a distinct, non-fallback series number.
        let suffixes = [
            "Chimap", "swi", "minIP", "T2starmap", "R2starmap", "R2map", "R2primemap",
            "desc-paramagnetic_Chimap", "desc-diamagnetic_Chimap", "desc-total_Chimap",
            "desc-paramagnetic_smwi", "desc-diamagnetic_smwi", "desc-paramagneticsmwi_minIP",
            "desc-diamagneticsmwi_minIP", "dseg",
        ];
        let mut nums: Vec<i32> = suffixes.iter().map(|s| series_number_for(s)).collect();
        assert!(nums.iter().all(|&n| n != 99), "a known suffix fell through to the fallback");
        nums.sort();
        nums.dedup();
        assert_eq!(nums.len(), suffixes.len(), "series numbers must be unique");
    }

    #[test]
    fn test_export_run_dicoms_covers_chisep_r2_r2prime_smwi() {
        use crate::bids::entities::AcquisitionKey;

        let tmp = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(tmp.path());
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };

        // Write a small volume to every final-map path the exporter should pick up.
        let (nx, ny, nz) = (2usize, 2usize, 2usize);
        let data: Vec<f64> = (0..nx * ny * nz).map(|i| i as f64 * 0.01).collect();
        let affine = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let write = |p: std::path::PathBuf| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            io::save_nifti_to_file(&p, &data, (nx, ny, nz), (1.0, 1.0, 1.0), &affine).unwrap();
        };
        write(output.qsm_path(&key));
        write(output.r2star_path(&key));
        write(output.r2_path(&key));
        write(output.r2prime_path(&key));
        write(output.chi_para_path(&key));
        write(output.chi_dia_path(&key));
        write(output.chi_sep_total_path(&key));
        use qsmxt_engine::store::Contrast;
        for c in [Contrast::Paramagnetic, Contrast::Diamagnetic] {
            write(output.smwi_path(&key, c));
            write(output.smwi_mip_path(&key, c));
        }
        // The series are named after the published files' tails.
        for (c, (smwi, minip)) in [Contrast::Paramagnetic, Contrast::Diamagnetic].into_iter().zip(SMWI_SUFFIXES) {
            assert!(output.smwi_path(&key, c).to_string_lossy().ends_with(&format!("_{smwi}.nii")), "{smwi}");
            assert!(output.smwi_mip_path(&key, c).to_string_lossy().ends_with(&format!("_{minip}.nii")), "{minip}");
        }

        let run = crate::bids::discovery::QsmRun {
            key: key.clone(),
            echoes: vec![],
            coils: None,
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02],
            b0_dir: None,
            dims: (nx, ny, nz),
            has_magnitude: true,
            mese: None,
        };

        let opts = DicomExportOptions {
            export_swi: false,
            export_smwi: true,
            export_t2star: false,
            export_r2star: true,
            export_r2: true,
            export_r2prime: true,
            export_chisep: true,
            source_dicom: None,
            outputs_filter: None,
            segmentation: None,
        };
        assert_eq!(export_run_dicoms(&run, &output, &opts).unwrap(), 11);
        // Nothing to export is no series, which the run's record then does not claim.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(export_run_dicoms(&run, &DerivativeOutputs::new(empty.path()), &opts).unwrap(), 0);

        // Each enabled map produced its own DICOM series folder with nz slices.
        for suffix in [
            "Chimap", "R2starmap", "R2map", "R2primemap",
            "desc-paramagnetic_Chimap", "desc-diamagnetic_Chimap", "desc-total_Chimap",
            "desc-paramagnetic_smwi", "desc-diamagnetic_smwi", "desc-paramagneticsmwi_minIP",
            "desc-diamagneticsmwi_minIP",
        ] {
            let dir = output.dicom_dir(&key, suffix);
            assert!(dir.is_dir(), "missing DICOM dir for {suffix}");
            let n = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|x| x == "dcm"))
                .count();
            assert_eq!(n, nz, "expected {nz} slices for {suffix}");
        }
    }

    #[test]
    fn test_geometry_ras_to_lps() {
        // Identity-ish RAS affine, 1mm iso, origin at (10,20,30) RAS.
        let affine = [
            1.0, 0.0, 0.0, 10.0,
            0.0, 1.0, 0.0, 20.0,
            0.0, 0.0, 1.0, 30.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let g = SliceGeometry::from_affine(&affine, (1.0, 1.0, 1.0));
        // RAS origin (10,20,30) -> LPS (-10,-20,30)
        assert_eq!(g.position(0), [-10.0, -20.0, 30.0]);
        // slice 2 steps +2 in z
        assert_eq!(g.position(2), [-10.0, -20.0, 32.0]);
        assert!((g.row_spacing - 1.0).abs() < 1e-12);
        assert!((g.col_spacing - 1.0).abs() < 1e-12);
    }

    /// A run with χ and a segmentation on a 3x4x2 grid: left thalamus (10) on both slices, right
    /// thalamus (49) and CSF (24) on the second only.
    fn seg_fixture(tsv: Option<&str>) -> (tempfile::TempDir, DerivativeOutputs, crate::bids::discovery::QsmRun) {
        use crate::bids::entities::AcquisitionKey;
        let tmp = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(tmp.path());
        let key = AcquisitionKey {
            subject: "01".to_string(), session: None, acquisition: None, reconstruction: None,
            inversion: None, run: None, suffix: "MEGRE".to_string(),
        };
        let (dims, vox) = ((3usize, 4usize, 2usize), (1.0, 1.0, 2.0));
        let affine = [1.0, 0.0, 0.0, 5.0, 0.0, 1.0, 0.0, 6.0, 0.0, 0.0, 2.0, 7.0, 0.0, 0.0, 0.0, 1.0];
        let mut labels = vec![0.0; 24];
        labels[0] = 10.0;
        labels[12] = 10.0;
        labels[13] = 49.0;
        labels[23] = 24.0;
        let chi: Vec<f64> = (0..24).map(|i| i as f64 * 0.01).collect();
        for (p, d) in [(output.qsm_path(&key), &chi), (output.dseg_path(&key), &labels)] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            io::save_nifti_to_file(&p, d, dims, vox, &affine).unwrap();
        }
        if let Some(t) = tsv {
            std::fs::write(output.dseg_lookup_path(&key), t).unwrap();
        }
        let run = crate::bids::discovery::QsmRun {
            key, echoes: vec![], coils: None, magnetic_field_strength: 3.0, echo_times: vec![0.02],
            b0_dir: None, dims, has_magnitude: true, mese: None,
        };
        (tmp, output, run)
    }

    fn seg_opts(synthseg: bool, filter: Option<&[String]>) -> DicomExportOptions<'_> {
        DicomExportOptions {
            export_swi: false, export_t2star: false, export_r2star: false, export_r2: false,
            export_r2prime: false, export_chisep: false, source_dicom: None, outputs_filter: filter,
            segmentation: Some(SegmentationExport { synthseg, algorithm: "SynthSeg 1.0".into() }),
        }
    }

    fn items(obj: &InMemDicomObject, tag: dicom::core::Tag) -> Vec<InMemDicomObject> {
        obj.element(tag).unwrap().items().unwrap().to_vec()
    }

    fn text(obj: &InMemDicomObject, tag: dicom::core::Tag) -> String {
        obj.element(tag).unwrap().to_str().unwrap().trim().to_string()
    }

    #[test]
    fn the_segmentation_is_a_seg_on_the_chi_series() {
        let (_tmp, output, run) = seg_fixture(Some("index\tname\n10\tleft thalamus\n49\tright thalamus\n24\tcsf\n"));
        assert_eq!(export_run_dicoms(&run, &output, &seg_opts(true, None)).unwrap(), 2, "χ and the SEG");
        let key = &run.key;
        let file = output.dicom_dir(key, "dseg").join(format!("{}_dseg.dcm", key.basename()));
        let obj = open_file(&file).expect("a DICOM SEG");
        assert_eq!(text(&obj, tags::SOP_CLASS_UID), uids::SEGMENTATION_STORAGE);
        assert_eq!(text(&obj, tags::MODALITY), "SEG");
        assert_eq!(text(&obj, tags::SEGMENTATION_TYPE), "BINARY");
        // Segments in label order (CSF 24, left thalamus 10 → 10 first), one frame per slice each is on.
        let segs = items(&obj, tags::SEGMENT_SEQUENCE);
        let names: Vec<String> = segs.iter().map(|s| text(s, tags::SEGMENT_LABEL)).collect();
        assert_eq!(names, ["left thalamus", "csf", "right thalamus"]);
        assert_eq!(text(&obj, tags::NUMBER_OF_FRAMES), "4");
        assert_eq!(obj.element(tags::PIXEL_DATA).unwrap().to_bytes().unwrap().len(), 6, "4 frames x 12 bits, even");
        let ty = &items(&segs[0], tags::SEGMENTED_PROPERTY_TYPE_CODE_SEQUENCE)[0];
        assert_eq!(text(ty, tags::CODE_VALUE), "42695009");
        assert_eq!(text(&items(ty, tags::SEGMENTED_PROPERTY_TYPE_MODIFIER_CODE_SEQUENCE)[0], tags::CODE_VALUE), "7771000");
        // It references the χ series that was exported with it, slice by slice.
        let chi_series = make_uid(&[&key.basename(), "Chimap"]);
        let refs = items(&obj, tags::REFERENCED_SERIES_SEQUENCE);
        assert_eq!(text(&refs[0], tags::SERIES_INSTANCE_UID), chi_series);
        assert_eq!(items(&refs[0], tags::REFERENCED_INSTANCE_SEQUENCE).len(), 2);
        let chi_slice = open_file(output.dicom_dir(key, "Chimap").join(format!("{}_Chimap_0002.dcm", key.basename()))).unwrap();
        let frame = &items(&obj, tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE)[1];
        let source = &items(&items(frame, tags::DERIVATION_IMAGE_SEQUENCE)[0], tags::SOURCE_IMAGE_SEQUENCE)[0];
        assert_eq!(text(source, tags::REFERENCED_SOP_INSTANCE_UID), text(&chi_slice, tags::SOP_INSTANCE_UID));
        assert_eq!(text(&obj, tags::FRAME_OF_REFERENCE_UID), text(&chi_slice, tags::FRAME_OF_REFERENCE_UID));
    }

    #[test]
    fn without_the_chi_series_there_is_no_seg() {
        let (_tmp, output, run) = seg_fixture(None);
        let only_seg = vec!["dseg".to_string()];
        assert_eq!(export_run_dicoms(&run, &output, &seg_opts(true, Some(&only_seg))).unwrap(), 0);
        assert!(!output.dicom_dir(&run.key, "dseg").exists());
        // Filtered out, it is not written either.
        let only_chi = vec!["chimap".to_string()];
        assert_eq!(export_run_dicoms(&run, &output, &seg_opts(true, Some(&only_chi))).unwrap(), 1);
        assert!(!output.dicom_dir(&run.key, "dseg").exists());
    }

    #[test]
    fn a_supplied_segmentation_is_coded_generically_and_named_from_its_table() {
        let (_tmp, output, run) = seg_fixture(Some("index\tname\n10\tlesion\n"));
        assert_eq!(export_run_dicoms(&run, &output, &seg_opts(false, None)).unwrap(), 2);
        let file = output.dicom_dir(&run.key, "dseg").join(format!("{}_dseg.dcm", run.key.basename()));
        let segs = items(&open_file(&file).unwrap(), tags::SEGMENT_SEQUENCE);
        let names: Vec<String> = segs.iter().map(|s| text(s, tags::SEGMENT_LABEL)).collect();
        assert_eq!(names, ["lesion", "label 24", "label 49"]);
        let ty = &items(&segs[0], tags::SEGMENTED_PROPERTY_TYPE_CODE_SEQUENCE)[0];
        assert_eq!(text(ty, tags::CODE_VALUE), "128319008");
        assert!(ty.element(tags::SEGMENTED_PROPERTY_TYPE_MODIFIER_CODE_SEQUENCE).is_err());
    }

    #[test]
    fn every_synthseg_label_has_a_code() {
        for version in [qsm_core::segment::SynthSegVersion::V1, qsm_core::segment::SynthSegVersion::V2] {
            for &id in version.labels().ids.iter().filter(|&&id| id != 0) {
                assert!(seg::synthseg_code(id as i64).is_some(), "SynthSeg label {id} has no DICOM code");
            }
        }
    }
}


