//! Tiny synthetic DICOM series for tests: 8×8 slices, no patient data, written with dicom-rs.
//!
//! Each fixture in [`FIXTURES`] reproduces one export pattern the conversion has to handle (a
//! scanner's SWI and QSM next to the magnitude, a duplicate export, a repeat with other echoes, a
//! headerless file, …). The tests scan and convert them; CI also runs the BIDS validator over
//! what `qsmxt dicom-convert` writes from them.

use std::path::{Path, PathBuf};

use dicom::core::value::PrimitiveValue;
use dicom::core::{DataElement, Tag, VR};
use dicom::dictionary_std::{tags, uids};
use dicom::object::meta::FileMetaTableBuilder;
use dicom::object::InMemDicomObject;

/// One synthetic series.
#[derive(Debug, Clone)]
pub struct Series {
    pub uid: String,
    pub number: i32,
    pub description: String,
    pub protocol: String,
    pub image_type: Vec<String>,
    /// Echo times (ms); one set of slices per echo.
    pub tes: Vec<f64>,
    /// Whether to write EchoNumbers (Bruker exports leave it out).
    pub echo_numbers: bool,
    pub slices: usize,
    pub patient: String,
    pub study_uid: String,
    /// AcquisitionTime, HHMMSS.
    pub time: String,
    pub manufacturer: String,
    pub scanning_sequence: String,
    pub ge_image_type: Option<i16>,
    pub coil: Option<String>,
    pub inversion_time: Option<f64>,
    pub sop_class: String,
    /// Pixel values start here; series with the same seed have the same pixels.
    pub seed: u16,
    /// Write the data set without the Part-10 preamble and file meta group.
    pub headerless: bool,
    /// Instance number of the first file, less one.
    pub first_instance: usize,
    /// Write every echo with the first echo's EchoTime and EchoNumbers (tags that do not tell
    /// the echoes apart, as some scanner reconstructions export).
    pub same_echo_tags: bool,
    /// RescaleSlope and RescaleIntercept.
    pub rescale: Option<(f64, f64)>,
}

impl Series {
    pub fn new(uid: &str, number: i32, description: &str, image_type: &[&str]) -> Self {
        Series {
            uid: format!("1.2.826.0.1.3680043.10.1234.{uid}"),
            number,
            description: description.into(),
            protocol: description.into(),
            image_type: image_type.iter().map(|s| s.to_string()).collect(),
            tes: vec![20.0],
            echo_numbers: true,
            slices: 4,
            patient: "synth01".into(),
            study_uid: "1.2.826.0.1.3680043.10.1234.1".into(),
            time: "100000".into(),
            manufacturer: "SIEMENS".into(),
            scanning_sequence: "GR".into(),
            ge_image_type: None,
            coil: None,
            inversion_time: None,
            sop_class: uids::MR_IMAGE_STORAGE.into(),
            seed: (number as u16).wrapping_mul(7),
            headerless: false,
            first_instance: 0,
            same_echo_tags: false,
            rescale: None,
        }
    }
    pub fn protocol(mut self, p: &str) -> Self { self.protocol = p.into(); self }
    pub fn tes(mut self, t: &[f64]) -> Self { self.tes = t.to_vec(); self }
    pub fn time(mut self, t: &str) -> Self { self.time = t.into(); self }
    pub fn seed(mut self, s: u16) -> Self { self.seed = s; self }
    pub fn manufacturer(mut self, m: &str) -> Self { self.manufacturer = m.into(); self }
    pub fn scanning(mut self, s: &str) -> Self { self.scanning_sequence = s.into(); self }
    pub fn ge(mut self, t: i16) -> Self { self.ge_image_type = Some(t); self }
    pub fn coil(mut self, c: &str) -> Self { self.coil = Some(c.into()); self }
    pub fn inversion(mut self, ti: f64) -> Self { self.inversion_time = Some(ti); self }
    pub fn headerless(mut self) -> Self { self.headerless = true; self }
    pub fn no_echo_numbers(mut self) -> Self { self.echo_numbers = false; self }
    pub fn first_instance(mut self, n: usize) -> Self { self.first_instance = n; self }
    pub fn patient(mut self, p: &str) -> Self { self.patient = p.into(); self }
    pub fn same_echo_tags(mut self) -> Self { self.same_echo_tags = true; self }
    pub fn rescale(mut self, slope: f64, intercept: f64) -> Self { self.rescale = Some((slope, intercept)); self }
}

fn ds(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn strs(v: &[String]) -> PrimitiveValue {
    PrimitiveValue::Strs(v.iter().cloned().collect())
}

/// The data set of one slice of one echo.
fn slice(s: &Series, echo: usize, k: usize, instance: usize) -> InMemDicomObject {
    let mut o = InMemDicomObject::new_empty();
    let sop_uid = format!("{}.{}", s.uid, instance);
    let mut put = |tag: Tag, vr: VR, v: PrimitiveValue| {
        o.put(DataElement::new(tag, vr, v));
    };
    put(tags::SPECIFIC_CHARACTER_SET, VR::CS, "ISO_IR 100".into());
    put(tags::IMAGE_TYPE, VR::CS, strs(&s.image_type));
    put(tags::SOP_CLASS_UID, VR::UI, s.sop_class.as_str().into());
    put(tags::SOP_INSTANCE_UID, VR::UI, sop_uid.as_str().into());
    put(tags::STUDY_DATE, VR::DA, "20240101".into());
    put(tags::SERIES_DATE, VR::DA, "20240101".into());
    put(tags::ACQUISITION_DATE, VR::DA, "20240101".into());
    put(tags::STUDY_TIME, VR::TM, "095500".into());
    put(tags::SERIES_TIME, VR::TM, s.time.as_str().into());
    put(tags::ACQUISITION_TIME, VR::TM, s.time.as_str().into());
    put(tags::MODALITY, VR::CS, "MR".into());
    put(tags::MANUFACTURER, VR::LO, s.manufacturer.as_str().into());
    put(tags::SERIES_DESCRIPTION, VR::LO, s.description.as_str().into());
    put(tags::PATIENT_NAME, VR::PN, s.patient.as_str().into());
    put(tags::PATIENT_ID, VR::LO, s.patient.as_str().into());
    put(tags::SCANNING_SEQUENCE, VR::CS, strs(&s.scanning_sequence.split('\\').map(String::from).collect::<Vec<_>>()));
    put(tags::SEQUENCE_VARIANT, VR::CS, "SP".into());
    put(tags::MR_ACQUISITION_TYPE, VR::CS, "3D".into());
    put(tags::SLICE_THICKNESS, VR::DS, "1".into());
    put(tags::REPETITION_TIME, VR::DS, "30".into());
    let tag_echo = if s.same_echo_tags { 0 } else { echo };
    put(tags::ECHO_TIME, VR::DS, ds(s.tes[tag_echo]).as_str().into());
    if let Some(ti) = s.inversion_time {
        put(tags::INVERSION_TIME, VR::DS, ds(ti).as_str().into());
    }
    put(tags::IMAGING_FREQUENCY, VR::DS, "123.2".into());
    if s.echo_numbers {
        put(tags::ECHO_NUMBERS, VR::IS, (tag_echo + 1).to_string().as_str().into());
    }
    put(tags::MAGNETIC_FIELD_STRENGTH, VR::DS, "3".into());
    put(tags::FLIP_ANGLE, VR::DS, "15".into());
    put(tags::PROTOCOL_NAME, VR::LO, s.protocol.as_str().into());
    put(tags::PATIENT_POSITION, VR::CS, "HFS".into());
    put(tags::STUDY_INSTANCE_UID, VR::UI, s.study_uid.as_str().into());
    put(tags::SERIES_INSTANCE_UID, VR::UI, s.uid.as_str().into());
    put(tags::SERIES_NUMBER, VR::IS, s.number.to_string().as_str().into());
    put(tags::ACQUISITION_NUMBER, VR::IS, "1".into());
    put(tags::INSTANCE_NUMBER, VR::IS, instance.to_string().as_str().into());
    put(tags::IMAGE_POSITION_PATIENT, VR::DS, strs(&["-4".into(), "-4".into(), ds(k as f64)]));
    put(tags::IMAGE_ORIENTATION_PATIENT, VR::DS, strs(&["1", "0", "0", "0", "1", "0"].map(String::from)));
    put(tags::FRAME_OF_REFERENCE_UID, VR::UI, format!("{}.9", s.study_uid).as_str().into());
    put(tags::SLICE_LOCATION, VR::DS, ds(k as f64).as_str().into());
    put(tags::SAMPLES_PER_PIXEL, VR::US, PrimitiveValue::from(1u16));
    put(tags::PHOTOMETRIC_INTERPRETATION, VR::CS, "MONOCHROME2".into());
    put(tags::ROWS, VR::US, PrimitiveValue::from(8u16));
    put(tags::COLUMNS, VR::US, PrimitiveValue::from(8u16));
    put(tags::PIXEL_SPACING, VR::DS, strs(&["1".into(), "1".into()]));
    put(tags::BITS_ALLOCATED, VR::US, PrimitiveValue::from(16u16));
    put(tags::BITS_STORED, VR::US, PrimitiveValue::from(12u16));
    put(tags::HIGH_BIT, VR::US, PrimitiveValue::from(11u16));
    put(tags::PIXEL_REPRESENTATION, VR::US, PrimitiveValue::from(0u16));
    if let Some((slope, intercept)) = s.rescale {
        put(tags::RESCALE_SLOPE, VR::DS, format!("{slope:.10}").as_str().into());
        put(tags::RESCALE_INTERCEPT, VR::DS, format!("{intercept:.8}").as_str().into());
        put(tags::RESCALE_TYPE, VR::LO, "normalized".into());
    }
    if let Some(c) = &s.coil {
        put(Tag(0x0051, 0x0010), VR::LO, "SIEMENS MR HEADER".into());
        put(Tag(0x0051, 0x100F), VR::LO, c.as_str().into());
    }
    if let Some(t) = s.ge_image_type {
        put(Tag(0x0043, 0x0010), VR::LO, "GEMS_PARM_01".into());
        put(Tag(0x0043, 0x102F), VR::SS, PrimitiveValue::from(t));
    }
    let pixels: Vec<u16> = (0..64u16).map(|i| s.seed.wrapping_add(i * 3 + (k as u16) * 11 + (echo as u16) * 5) % 4000).collect();
    put(tags::PIXEL_DATA, VR::OW, PrimitiveValue::U16(pixels.into()));
    o
}

fn write_object(obj: InMemDicomObject, path: &Path, s: &Series, instance: usize) {
    if s.headerless {
        let mut f = std::fs::File::create(path).unwrap();
        let ts = dicom::transfer_syntax::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased();
        obj.write_dataset_with_ts(&mut f, &ts).unwrap();
    } else {
        let sop_uid = format!("{}.{}", s.uid, instance);
        obj.with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid(s.sop_class.as_str())
                .media_storage_sop_instance_uid(sop_uid.as_str()),
        )
        .unwrap()
        .write_to_file(path)
        .unwrap();
    }
}

/// Write a series into `dir` (one file per slice and echo); returns the files.
pub fn write_series(dir: &Path, s: &Series) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    let mut out = Vec::new();
    let mut instance = s.first_instance;
    for echo in 0..s.tes.len() {
        for k in 0..s.slices {
            instance += 1;
            let path = dir.join(format!("s{:03}_{:04}.dcm", s.number, instance));
            write_object(slice(s, echo, k, instance), &path, s, instance);
            out.push(path);
        }
    }
    out
}

/// A structured-report file (no pixels), like Siemens' PhoenixZIPReport.
pub fn write_report(dir: &Path, number: i32) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let uid = format!("1.2.826.0.1.3680043.10.1234.{number}");
    let mut o = InMemDicomObject::new_empty();
    o.put(DataElement::new(tags::SOP_CLASS_UID, VR::UI, uids::ENHANCED_SR_STORAGE));
    o.put(DataElement::new(tags::SOP_INSTANCE_UID, VR::UI, format!("{uid}.1")));
    o.put(DataElement::new(tags::MODALITY, VR::CS, "SR"));
    o.put(DataElement::new(tags::IMAGE_TYPE, VR::CS, strs(&["ORIGINAL", "PRIMARY", "OTHER", "CSA REPORT"].map(String::from))));
    o.put(DataElement::new(tags::SERIES_DESCRIPTION, VR::LO, "PhoenixZIPReport"));
    o.put(DataElement::new(tags::PATIENT_ID, VR::LO, "synth01"));
    o.put(DataElement::new(tags::STUDY_DATE, VR::DA, "20240101"));
    o.put(DataElement::new(tags::STUDY_INSTANCE_UID, VR::UI, "1.2.826.0.1.3680043.10.1234.1"));
    o.put(DataElement::new(tags::SERIES_INSTANCE_UID, VR::UI, uid.as_str()));
    o.put(DataElement::new(tags::SERIES_NUMBER, VR::IS, number.to_string()));
    let path = dir.join(format!("sr{number}.dcm"));
    o.with_meta(
        FileMetaTableBuilder::new()
            .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
            .media_storage_sop_class_uid(uids::ENHANCED_SR_STORAGE)
            .media_storage_sop_instance_uid(format!("{uid}.1")),
    )
    .unwrap()
    .write_to_file(&path)
    .unwrap();
    path
}

const M: &[&str] = &["ORIGINAL", "PRIMARY", "M", "ND", "NORM"];
const P: &[&str] = &["ORIGINAL", "PRIMARY", "P", "ND"];

/// The export patterns, by name.
pub const FIXTURES: &[&str] = &[
    "swi_qsm_and_magnitude",
    "duplicate_identical",
    "duplicate_different",
    "repeat_same_settings",
    "repeat_other_echoes",
    "clinical_series",
    "bruker_te_suffix",
    "headerless",
    "incomplete_export",
    "phoenix_report",
    "split_patient",
    "ge_private_type",
    "mp2rage",
    "uncombined_coils",
    "same_echo_tags",
    "philips_radian_phase",
];

/// Write fixture `name` into `dir`.
pub fn write_fixture(name: &str, dir: &Path) {
    let gre = "gre_qsm";
    match name {
        "swi_qsm_and_magnitude" => {
            let p = "t2star_SWIandQSM";
            write_series(dir, &Series::new("9", 9, p, M));
            write_series(dir, &Series::new("10", 10, p, P));
            write_series(dir, &Series::new("13", 13, "t2star_SWIandQSM_QSM", &["ORIGINAL", "PRIMARY", "QSM", "ND"]).protocol(p));
            write_series(dir, &Series::new("14", 14, "mIP_Images(SW)", &["ORIGINAL", "PRIMARY", "MNIP", "ND", "NORM"]).protocol(p));
            write_series(dir, &Series::new("15", 15, "SWI_Images", &["ORIGINAL", "PRIMARY", "M", "SWI", "ND", "NORM"]).protocol(p));
        }
        "duplicate_identical" | "duplicate_different" => {
            write_series(dir, &Series::new("9", 9, gre, M));
            write_series(dir, &Series::new("10", 10, gre, P));
            let seed = if name == "duplicate_identical" { 9 * 7 } else { 500 };
            write_series(dir, &Series::new("11", 11, "Mag_Images", M).protocol(gre).seed(seed));
        }
        "repeat_same_settings" => {
            write_series(dir, &Series::new("2", 2, gre, M).tes(&[5.0, 10.0]));
            write_series(dir, &Series::new("3", 3, gre, P).tes(&[5.0, 10.0]));
            write_series(dir, &Series::new("4", 4, gre, M).tes(&[5.0, 10.0]).time("101500"));
            write_series(dir, &Series::new("5", 5, gre, P).tes(&[5.0, 10.0]).time("101500"));
        }
        "repeat_other_echoes" => {
            write_series(dir, &Series::new("2", 2, gre, M));
            write_series(dir, &Series::new("3", 3, gre, P));
            write_series(dir, &Series::new("4", 4, gre, M).tes(&[4.0, 8.0, 12.0]).time("102000"));
            write_series(dir, &Series::new("5", 5, gre, P).tes(&[4.0, 8.0, 12.0]).time("102000"));
        }
        "clinical_series" => {
            write_series(dir, &Series::new("1", 1, "AAHead_Scout", M));
            write_series(dir, &Series::new("2", 2, "AAHead_Scout_MPR_sag", &["DERIVED", "PRIMARY", "MPR", "ND", "NORM"]).protocol("AAHead_Scout"));
            write_series(dir, &Series::new("5", 5, "t2_tirm_tra_dark-fluid", M).scanning("SE\\IR").time("100500"));
            write_series(dir, &Series::new("6", 6, "resolve_TRACEW", &["DERIVED", "PRIMARY", "DIFFUSION", "TRACEW", "ND"]).protocol("resolve").time("101000"));
            write_series(dir, &Series::new("8", 8, "t1_mprage_sag", M).scanning("GR\\IR").time("101500"));
            write_series(dir, &Series::new("9", 9, gre, M).time("102000"));
            write_series(dir, &Series::new("10", 10, gre, P).time("102000"));
        }
        "bruker_te_suffix" => {
            let mk = |uid: &str, n: i32, d: &str, it: &[&str]| {
                Series::new(uid, n, d, it).manufacturer("Bruker BioSpin MRI GmbH").scanning("RM").no_echo_numbers()
            };
            write_series(dir, &mk("90001", 90001, "", &["ORIGINAL", "PRIMARY", "OTHER"]).protocol("T2star_map_MGE").tes(&[3.3, 6.6, 9.9]));
            write_series(dir, &mk("90002", 90002, "Parametric image for T2* relaxation", &["DERIVED", "SECONDARY", "OTHER"]).protocol("").tes(&[3.3]));
        }
        "headerless" => {
            write_series(dir, &Series::new("9", 9, gre, M));
            write_series(dir, &Series::new("10", 10, gre, P).headerless());
        }
        "incomplete_export" => {
            write_series(dir, &Series::new("9", 9, gre, M).tes(&[5.0, 10.0]));
            let files = write_series(dir, &Series::new("10", 10, gre, P).tes(&[5.0, 10.0]));
            std::fs::write(&files[5], b"").unwrap();
        }
        "phoenix_report" => {
            write_series(dir, &Series::new("9", 9, gre, M));
            write_series(dir, &Series::new("10", 10, gre, P));
            write_report(dir, 99);
        }
        "split_patient" => {
            write_series(dir, &Series::new("4", 4, gre, M));
            let files = write_series(dir, &Series::new("5", 5, gre, P));
            // One file of the phase series names another patient (and description).
            let odd = Series::new("5", 5, "Other name", P).protocol(gre).patient("other01");
            write_object(slice(&odd, 0, 2, 3), &files[2], &odd, 3);
        }
        "ge_private_type" => {
            for (t, seed) in [(0i16, 10u16), (2, 300), (3, 600)] {
                let s = Series::new("8", 8, "Ax SWAN", &["ORIGINAL", "PRIMARY", "OTHER"])
                    .protocol("SWAN").manufacturer("GE MEDICAL SYSTEMS").ge(t).seed(seed).tes(&[10.0, 20.0])
                    .first_instance(t as usize * 100);
                write_series(&dir.join(format!("part{t}")), &s);
            }
        }
        "mp2rage" => {
            let p = "MP2RAGE_0p75mm";
            let mk = |uid: &str, n: i32, d: &str, it: &[&str], ti: f64| Series::new(uid, n, d, it).protocol(p).scanning("GR\\IR").inversion(ti).tes(&[2.0]);
            write_series(dir, &mk("27", 27, "MP2RAGE_0p75mm_INV1", &["ORIGINAL", "PRIMARY", "M", "DIS3D"], 840.0));
            write_series(dir, &mk("28", 28, "MP2RAGE_0p75mm_INV2", &["ORIGINAL", "PRIMARY", "M", "DIS3D"], 2370.0));
            write_series(dir, &mk("29", 29, "MP2RAGE_0p75mm_T1_Images", &["DERIVED", "PRIMARY", "T1 MAP", "DIS3D"], 0.0));
            write_series(dir, &mk("31", 31, "MP2RAGE_0p75mm_UNI_Images", &["DERIVED", "PRIMARY", "M", "UNI", "DIS3D"], 0.0));
            write_series(dir, &mk("32", 32, "MP2RAGE_0p75mm_UNI-DEN", &["DERIVED", "PRIMARY", "M", "UNI", "DIS3D"], 0.0));
        }
        "uncombined_coils" => {
            let p = "SWI_3mm";
            for (uid, n, it) in [("15", 15, M), ("18", 18, P)] {
                for c in 1..=2u16 {
                    let s = Series::new(uid, n, p, it).coil(&format!("H{c}")).seed(c * 100 + n as u16).tes(&[9.4, 20.0])
                        .first_instance(c as usize * 100);
                    write_series(&dir.join(format!("s{n}c{c}")), &s);
                }
            }
            write_series(dir, &Series::new("16", 16, p, &["ORIGINAL", "PRIMARY", "M", "ND"]).coil("HEA;HEP").tes(&[9.4, 20.0]));
            write_series(dir, &Series::new("17", 17, p, &["ORIGINAL", "PRIMARY", "M", "ND", "NORM", "FM", "FIL"]).coil("HEA;HEP").tes(&[9.4, 20.0]));
            write_series(dir, &Series::new("19", 19, p, P).coil("HEA;HEP").tes(&[9.4, 20.0]));
        }
        "same_echo_tags" => {
            // A scanner QSM series whose echoes all carry the same echo time and number.
            let p = "fl3d_vibe_QSM";
            write_series(dir, &Series::new("20", 20, p, M).tes(&[4.0, 8.0, 12.0]));
            write_series(dir, &Series::new("21", 21, p, P).tes(&[4.0, 8.0, 12.0]));
            write_series(dir, &Series::new("22", 22, "fl3d_vibe_QSM_QSM", &["ORIGINAL", "PRIMARY", "QSM", "ND"]).protocol(p)
                .tes(&[4.0, 8.0, 12.0]).same_echo_tags());
        }
        "philips_radian_phase" => {
            let p = "WIP QSM 2e";
            let mk = |uid: &str, it: &[&str]| Series::new(uid, 301, "QSM 2e", it).protocol(p).manufacturer("Philips").tes(&[5.0, 10.0]);
            write_series(dir, &mk("301m", &["ORIGINAL", "PRIMARY", "M_FFE", "M", "FFE"]));
            // Philips scales its phase to radians: values 0…4000 → −π…3.0.
            write_series(dir, &mk("301p", &["ORIGINAL", "PRIMARY", "PHASE MAP", "P", "FFE"]).rescale(std::f64::consts::TAU / 4096.0, -std::f64::consts::PI).first_instance(100));
        }
        other => panic!("no fixture {other}"),
    }
}
