//! The run's segmentation as a DICOM Segmentation object (SEG, Segmentation Storage
//! 1.2.840.10008.5.1.4.1.1.66.4), next to the exported maps.
//!
//! One multi-frame instance holds every structure as a BINARY segment: one frame per slice that
//! has any voxel of it, bit-packed. Each frame points at the χ series' slice it was cut from, so
//! the SEG needs the χ map exported in the same run, on the same grid (the published `dseg` and
//! `Chimap` share it). A viewer overlays it on that series: OHIF, Weasis, and 3D Slicer with the
//! QuantitativeReporting extension.
//!
//! SynthSeg's labels are coded (category, type, laterality) from DICOM PS3.16 (see
//! [`synthseg_code`]); a supplied segmentation is not known to use FreeSurfer's numbering, so its
//! labels are coded generically and named from its `dseg.tsv`.

use std::collections::BTreeMap;
use std::path::Path;

use dicom::core::value::{DataSetSequence, PrimitiveValue};
use dicom::core::{DataElement, Tag, VR};
use dicom::dictionary_std::{tags, uids};
use dicom::object::meta::FileMetaTableBuilder;
use dicom::object::InMemDicomObject;

use super::{fmt_ds, make_uid, DicomIdentity, SliceGeometry};
use crate::error::QsmxtError;
use qsm_core::io;

/// A coded concept: (code value, coding scheme, code meaning).
type Code = (&'static str, &'static str, &'static str);

/// Segmented Property Category for every segment: CID 7150.
const ANATOMICAL_STRUCTURE: Code = ("91723000", "SCT", "Anatomical Structure");
const LEFT: Code = ("7771000", "SCT", "Left");
const RIGHT: Code = ("24028007", "SCT", "Right");
/// A label of a supplied segmentation, whose meaning is unknown (CID 7140).
const INTRACRANIAL_STRUCTURE: Code = ("128319008", "SCT", "Intracranial structure");

/// Segmented Property Type and laterality of a SynthSeg (FreeSurfer) label.
///
/// Types are from DICOM PS3.16 2026d, CID 7153 "CNS Segmentation Type" (the group CID 7151, the
/// Segmented Property Type's baseline, takes them from). Three FreeSurfer structures are not in
/// it and come from CID 7140 "Brain Structure for Volumetric Measurement", the same edition:
/// cerebellar cortex, the inferior horn of the lateral ventricle and the ventral diencephalon.
/// Laterality is a type modifier (CID 244). `None` for background and unknown ids.
pub(super) fn synthseg_code(id: i64) -> Option<(Code, Option<Code>)> {
    let ty: Code = match id {
        2 | 41 => ("68523003", "SCT", "Cerebral White Matter"),
        3 | 42 => ("40146001", "SCT", "Cerebral cortex"),
        4 | 43 => ("66720007", "SCT", "Lateral ventricle"),
        5 | 44 => ("53118009", "SCT", "Inferior Horn of Lateral Ventricle"),
        7 | 46 => ("33060004", "SCT", "Cerebellar white matter"),
        8 | 47 => ("25991003", "SCT", "Cerebellar Cortex"),
        10 | 49 => ("42695009", "SCT", "Thalamus"),
        11 | 50 => ("11000004", "SCT", "Caudate nucleus"),
        12 | 51 => ("89278009", "SCT", "Putamen"),
        13 | 52 => ("14738005", "SCT", "Globus pallidus"),
        14 => ("49841001", "SCT", "Third ventricle"),
        15 => ("35918002", "SCT", "Fourth ventricle"),
        16 => ("119238007", "SCT", "Brain stem"),
        17 | 53 => ("5366008", "SCT", "Hippocampus"),
        18 | 54 => ("4958002", "SCT", "Amygdala"),
        24 => ("65216001", "SCT", "Cerebrospinal Fluid"),
        26 | 58 => ("427667007", "SCT", "Nucleus accumbens"),
        28 | 60 => ("110700", "DCM", "Ventral Diencephalon"),
        _ => return None,
    };
    let side = match id {
        2..=13 | 17 | 18 | 26 | 28 => Some(LEFT),
        41..=54 | 58 | 60 => Some(RIGHT),
        _ => None,
    };
    Some((ty, side))
}

/// The segmentation to export and how its labels are to be read.
pub(super) struct SegSource<'a> {
    pub dseg: &'a Path,
    /// The BIDS `index`/`name` table that goes with it, when there is one.
    pub lookup: Option<&'a Path>,
    /// The labels are SynthSeg's FreeSurfer ids, coded by [`synthseg_code`].
    pub synthseg: bool,
    /// Segment Algorithm Name: "SynthSeg 1.0", or the derivatives tool a supplied one came from.
    pub algorithm: &'a str,
}

/// The exported χ series the segmentation was made on.
pub(super) struct Reference<'a> {
    pub series_uid: &'a str,
    pub dims: (usize, usize, usize),
    pub affine: &'a [f64; 16],
}

pub(super) struct SegExport<'a> {
    pub source: SegSource<'a>,
    pub reference: Reference<'a>,
    pub identity: &'a DicomIdentity,
    pub key: &'a crate::bids::entities::AcquisitionKey,
    pub out_dir: &'a Path,
}

/// A segment: its number in the SEG, the label it is cut from, and how it is described.
struct Segment {
    label: i64,
    name: String,
    ty: Code,
    side: Option<Code>,
}

/// SOP Instance UID of slice `k` (0-based) of an exported map series ([`super::export_map`]).
pub(super) fn slice_uid(series_uid: &str, k: usize) -> String {
    format!("{}.{}", series_uid, k + 1)
}

/// Write the SEG. Returns how many segments it holds (0, writing nothing, when the segmentation
/// has no labelled voxel).
pub(super) fn export_segmentation(s: SegExport) -> crate::Result<usize> {
    let src = &s.source;
    let vol = io::read_nifti_file(src.dseg)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", src.dseg.display(), e)))?;
    let (nx, ny, nz) = vol.dims;
    let same_affine = vol.affine.iter().zip(s.reference.affine).all(|(a, b)| (a - b).abs() < 1e-4);
    if vol.dims != s.reference.dims || !same_affine {
        return Err(QsmxtError::Dicom(format!(
            "{}: the segmentation is not on the χ map's grid, so a SEG cannot reference it", src.dseg.display())));
    }
    let npix = nx * ny;
    let labels: Vec<i64> = vol.data.iter().map(|&v| if v.is_finite() { v.round() as i64 } else { 0 }).collect();

    let names = src.lookup.map(read_lookup).unwrap_or_default();
    let present: std::collections::BTreeSet<i64> = labels.iter().copied().filter(|&l| l > 0).collect();
    let segments: Vec<Segment> = present.into_iter().map(|label| {
        let name = names.get(&label).cloned().unwrap_or_else(|| format!("label {label}"));
        let (ty, side) = if src.synthseg { synthseg_code(label) } else { None }
            .unwrap_or((INTRACRANIAL_STRUCTURE, None));
        Segment { label, name, ty, side }
    }).collect();
    if segments.is_empty() {
        return Ok(0);
    }

    // Frames: per segment, every slice it is on; the bits of all frames follow each other.
    let mut frames: Vec<(usize, usize)> = Vec::new(); // (segment index, slice)
    let mut bits: Vec<u8> = Vec::new();
    for (si, seg) in segments.iter().enumerate() {
        for k in 0..nz {
            let slice = &labels[k * npix..(k + 1) * npix];
            if !slice.contains(&seg.label) {
                continue;
            }
            let base = frames.len() * npix;
            bits.resize((base + npix).div_ceil(8), 0);
            for (p, _) in slice.iter().enumerate().filter(|(_, &l)| l == seg.label) {
                let b = base + p;
                bits[b / 8] |= 1 << (b % 8);
            }
            frames.push((si, k));
        }
    }
    if bits.len() % 2 == 1 {
        bits.push(0);
    }

    let geometry = SliceGeometry::from_affine(&vol.affine, vol.voxel_size);
    let series_uid = make_uid(&[&s.key.basename(), "dseg"]);
    let sop_uid = format!("{series_uid}.1");
    let obj = build_seg(&s, &segments, &frames, bits, &geometry, &series_uid, &sop_uid, (nx, ny));

    std::fs::create_dir_all(s.out_dir)?;
    let file_obj = obj
        .with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid(uids::SEGMENTATION_STORAGE)
                .media_storage_sop_instance_uid(sop_uid.as_str()),
        )
        .map_err(|e| QsmxtError::Dicom(format!("build file meta: {}", e)))?;
    let path = s.out_dir.join(format!("{}_dseg.dcm", s.key.basename()));
    file_obj.write_to_file(&path).map_err(|e| QsmxtError::Dicom(format!("{}: {}", path.display(), e)))?;
    Ok(segments.len())
}

/// The `index` → `name` pairs of a BIDS `dseg.tsv`; empty when it cannot be read.
fn read_lookup(path: &Path) -> BTreeMap<i64, String> {
    let Ok(text) = std::fs::read_to_string(path) else { return BTreeMap::new() };
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let (Some(ix), Some(nx)) = (header.iter().position(|h| *h == "index"), header.iter().position(|h| *h == "name")) else {
        return BTreeMap::new();
    };
    lines.filter_map(|l| {
        let cols: Vec<&str> = l.split('\t').collect();
        Some((cols.get(ix)?.trim().parse().ok()?, cols.get(nx)?.trim().to_string()))
    }).collect()
}

fn el(tag: Tag, vr: VR, v: impl Into<PrimitiveValue>) -> DataElement<InMemDicomObject> {
    DataElement::new(tag, vr, v.into())
}

fn us(tag: Tag, v: u16) -> DataElement<InMemDicomObject> {
    DataElement::new(tag, VR::US, PrimitiveValue::U16([v].as_ref().into()))
}

fn ds3(tag: Tag, v: &[f64]) -> DataElement<InMemDicomObject> {
    DataElement::new(tag, VR::DS, PrimitiveValue::Strs(v.iter().map(|x| fmt_ds(*x)).collect()))
}

fn seq(tag: Tag, items: Vec<InMemDicomObject>) -> DataElement<InMemDicomObject> {
    DataElement::new(tag, VR::SQ, DataSetSequence::from(items))
}

fn item(elements: impl IntoIterator<Item = DataElement<InMemDicomObject>>) -> InMemDicomObject {
    InMemDicomObject::from_element_iter(elements)
}

fn code_item((value, scheme, meaning): Code) -> InMemDicomObject {
    item([
        el(tags::CODE_VALUE, VR::SH, value),
        el(tags::CODING_SCHEME_DESIGNATOR, VR::SH, scheme),
        el(tags::CODE_MEANING, VR::LO, meaning),
    ])
}

#[allow(clippy::too_many_arguments)]
fn build_seg(
    s: &SegExport, segments: &[Segment], frames: &[(usize, usize)], bits: Vec<u8>, g: &SliceGeometry,
    series_uid: &str, sop_uid: &str, (nx, ny): (usize, usize),
) -> InMemDicomObject {
    let id = s.identity;
    let now = chrono::Local::now();
    let (date, time) = (now.format("%Y%m%d").to_string(), now.format("%H%M%S").to_string());
    let mut obj = InMemDicomObject::new_empty();
    let mut put = |e| { obj.put(e); };

    // SOP Common; UTF-8, since a supplied segmentation's names can be anything.
    put(el(tags::SPECIFIC_CHARACTER_SET, VR::CS, "ISO_IR 192"));
    put(el(tags::SOP_CLASS_UID, VR::UI, uids::SEGMENTATION_STORAGE));
    put(el(tags::SOP_INSTANCE_UID, VR::UI, sop_uid));

    // Patient, General Study: as the maps.
    put(el(tags::PATIENT_NAME, VR::PN, id.patient_name.as_str()));
    put(el(tags::PATIENT_ID, VR::LO, id.patient_id.as_str()));
    put(el(tags::PATIENT_BIRTH_DATE, VR::DA, id.patient_birth_date.as_str()));
    put(el(tags::PATIENT_SEX, VR::CS, id.patient_sex.as_str()));
    put(el(tags::STUDY_INSTANCE_UID, VR::UI, id.study_uid.as_str()));
    put(el(tags::STUDY_DATE, VR::DA, id.study_date.as_str()));
    put(el(tags::STUDY_TIME, VR::TM, id.study_time.as_str()));
    put(el(tags::REFERRING_PHYSICIAN_NAME, VR::PN, id.referring_physician.as_str()));
    put(el(tags::STUDY_ID, VR::SH, id.study_id.as_str()));
    put(el(tags::ACCESSION_NUMBER, VR::SH, id.accession_number.as_str()));

    // General Series, Segmentation Series, Frame of Reference.
    put(el(tags::MODALITY, VR::CS, "SEG"));
    put(el(tags::SERIES_INSTANCE_UID, VR::UI, series_uid));
    put(el(tags::SERIES_NUMBER, VR::IS, super::series_number_for("dseg").to_string()));
    put(el(tags::SERIES_DESCRIPTION, VR::LO, "QSMxT segmentation"));
    put(el(tags::FRAME_OF_REFERENCE_UID, VR::UI, id.frame_of_reference_uid.as_str()));
    put(el(tags::POSITION_REFERENCE_INDICATOR, VR::LO, ""));

    // General and Enhanced General Equipment: QSMxT made this object.
    let version = crate::commands::update::describe_version().to_string();
    put(el(tags::MANUFACTURER, VR::LO, "QSMxT"));
    put(el(tags::MANUFACTURER_MODEL_NAME, VR::LO, "QSMxT"));
    put(el(tags::DEVICE_SERIAL_NUMBER, VR::LO, "0"));
    put(el(tags::SOFTWARE_VERSIONS, VR::LO, version.as_str()));

    // General Image, Segmentation Image.
    put(el(tags::INSTANCE_NUMBER, VR::IS, "1"));
    put(el(tags::CONTENT_DATE, VR::DA, date.as_str()));
    put(el(tags::CONTENT_TIME, VR::TM, time.as_str()));
    put(el(tags::IMAGE_TYPE, VR::CS, PrimitiveValue::Strs(["DERIVED".to_string(), "PRIMARY".to_string()].into_iter().collect())));
    put(el(tags::CONTENT_LABEL, VR::CS, "SEGMENTATION"));
    put(el(tags::CONTENT_DESCRIPTION, VR::LO, s.source.algorithm));
    put(el(tags::CONTENT_CREATOR_NAME, VR::PN, ""));
    put(el(tags::SEGMENTATION_TYPE, VR::CS, "BINARY"));
    put(el(tags::SEGMENTS_OVERLAP, VR::CS, "NO"));
    put(seq(tags::SEGMENT_SEQUENCE, segments.iter().enumerate().map(|(i, seg)| {
        let mut ty = code_item(seg.ty);
        if let Some(side) = seg.side {
            ty.put(seq(tags::SEGMENTED_PROPERTY_TYPE_MODIFIER_CODE_SEQUENCE, vec![code_item(side)]));
        }
        item([
            us(tags::SEGMENT_NUMBER, (i + 1) as u16),
            el(tags::SEGMENT_LABEL, VR::LO, seg.name.as_str()),
            el(tags::SEGMENT_ALGORITHM_TYPE, VR::CS, "AUTOMATIC"),
            el(tags::SEGMENT_ALGORITHM_NAME, VR::LO, s.source.algorithm),
            seq(tags::SEGMENTED_PROPERTY_CATEGORY_CODE_SEQUENCE, vec![code_item(ANATOMICAL_STRUCTURE)]),
            seq(tags::SEGMENTED_PROPERTY_TYPE_CODE_SEQUENCE, vec![ty]),
        ])
    }).collect()));

    // Image Pixel: 1-bit frames, packed with no padding between them.
    put(us(tags::SAMPLES_PER_PIXEL, 1));
    put(el(tags::PHOTOMETRIC_INTERPRETATION, VR::CS, "MONOCHROME2"));
    put(us(tags::ROWS, ny as u16));
    put(us(tags::COLUMNS, nx as u16));
    put(us(tags::BITS_ALLOCATED, 1));
    put(us(tags::BITS_STORED, 1));
    put(us(tags::HIGH_BIT, 0));
    put(us(tags::PIXEL_REPRESENTATION, 0));
    put(el(tags::LOSSY_IMAGE_COMPRESSION, VR::CS, "00"));
    put(el(tags::NUMBER_OF_FRAMES, VR::IS, frames.len().to_string()));
    put(DataElement::new(tags::PIXEL_DATA, VR::OB, PrimitiveValue::U8(bits.into())));

    // Multi-frame Dimension: frames are indexed by segment, then by position.
    let org = make_uid(&[series_uid, "dimensions"]);
    put(seq(tags::DIMENSION_ORGANIZATION_SEQUENCE, vec![item([el(tags::DIMENSION_ORGANIZATION_UID, VR::UI, org.as_str())])]));
    put(el(tags::DIMENSION_ORGANIZATION_TYPE, VR::CS, "3D"));
    let dim = |pointer: Tag, group: Tag, label: &str| item([
        el(tags::DIMENSION_ORGANIZATION_UID, VR::UI, org.as_str()),
        DataElement::new(tags::DIMENSION_INDEX_POINTER, VR::AT, PrimitiveValue::Tags([pointer].as_ref().into())),
        DataElement::new(tags::FUNCTIONAL_GROUP_POINTER, VR::AT, PrimitiveValue::Tags([group].as_ref().into())),
        el(tags::DIMENSION_DESCRIPTION_LABEL, VR::LO, label),
    ]);
    put(seq(tags::DIMENSION_INDEX_SEQUENCE, vec![
        dim(tags::REFERENCED_SEGMENT_NUMBER, tags::SEGMENT_IDENTIFICATION_SEQUENCE, "ReferencedSegmentNumber"),
        dim(tags::IMAGE_POSITION_PATIENT, tags::PLANE_POSITION_SEQUENCE, "ImagePositionPatient"),
    ]));

    // Multi-frame Functional Groups. Shared: the grid. Per frame: where it is, which segment,
    // and the χ slice it was derived from.
    put(seq(tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE, vec![item([
        seq(tags::PIXEL_MEASURES_SEQUENCE, vec![item([
            ds3(tags::PIXEL_SPACING, &[g.row_spacing, g.col_spacing]),
            el(tags::SLICE_THICKNESS, VR::DS, fmt_ds(g.slice_thickness)),
            el(tags::SPACING_BETWEEN_SLICES, VR::DS, fmt_ds(g.slice_thickness)),
        ])]),
        seq(tags::PLANE_ORIENTATION_SEQUENCE, vec![item([ds3(tags::IMAGE_ORIENTATION_PATIENT, &g.orientation)])]),
    ])]));
    let source_purpose = code_item(("121322", "DCM", "Source image for image processing operation"));
    let derivation = code_item(("113076", "DCM", "Segmentation"));
    put(seq(tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE, frames.iter().map(|&(si, k)| {
        let source = item([
            el(tags::REFERENCED_SOP_CLASS_UID, VR::UI, uids::MR_IMAGE_STORAGE),
            el(tags::REFERENCED_SOP_INSTANCE_UID, VR::UI, slice_uid(s.reference.series_uid, k).as_str()),
            seq(tags::PURPOSE_OF_REFERENCE_CODE_SEQUENCE, vec![source_purpose.clone()]),
            // Same grid as the χ slice ([`export_segmentation`] refuses any other).
            el(tags::SPATIAL_LOCATIONS_PRESERVED, VR::CS, "YES"),
        ]);
        item([
            seq(tags::FRAME_CONTENT_SEQUENCE, vec![item([DataElement::new(tags::DIMENSION_INDEX_VALUES, VR::UL,
                PrimitiveValue::U32([(si + 1) as u32, (k + 1) as u32].as_ref().into()))])]),
            seq(tags::PLANE_POSITION_SEQUENCE, vec![item([ds3(tags::IMAGE_POSITION_PATIENT, &g.position(k))])]),
            seq(tags::SEGMENT_IDENTIFICATION_SEQUENCE, vec![item([us(tags::REFERENCED_SEGMENT_NUMBER, (si + 1) as u16)])]),
            seq(tags::DERIVATION_IMAGE_SEQUENCE, vec![item([
                seq(tags::DERIVATION_CODE_SEQUENCE, vec![derivation.clone()]),
                seq(tags::SOURCE_IMAGE_SEQUENCE, vec![source]),
            ])]),
        ])
    }).collect()));

    // Common Instance Reference: the χ series, every slice.
    let (_, _, rz) = s.reference.dims;
    put(seq(tags::REFERENCED_SERIES_SEQUENCE, vec![item([
        el(tags::SERIES_INSTANCE_UID, VR::UI, s.reference.series_uid),
        seq(tags::REFERENCED_INSTANCE_SEQUENCE, (0..rz).map(|k| item([
            el(tags::REFERENCED_SOP_CLASS_UID, VR::UI, uids::MR_IMAGE_STORAGE),
            el(tags::REFERENCED_SOP_INSTANCE_UID, VR::UI, slice_uid(s.reference.series_uid, k).as_str()),
        ])).collect()),
    ])]));
    obj
}
