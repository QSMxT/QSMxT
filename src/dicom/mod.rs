//! Reading a folder of DICOM files for `qsmxt dicom-convert` and the TUI.
//!
//! This module does the I/O: it walks the folder, reads each file's header with dicom-rs, and
//! compares pixel data when two series might be the same images exported twice. Grouping,
//! classification and naming are qsmxt-engine's ([`qsmxt_engine::dicom`]), so every front end
//! decides the same way; [`convert`] runs dcm2niix and moves its outputs into BIDS.

pub mod convert;
pub mod export;
#[cfg(test)]
pub mod synth;
#[cfg(test)]
mod tests;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dicom::core::Tag;
use dicom::dictionary_std::tags;
use dicom::object::{InMemDicomObject, OpenFileOptions};
use rayon::prelude::*;

pub use qsmxt_engine::dicom::*;

// ─── Reading values ───

fn element_str(obj: &InMemDicomObject, tag: Tag) -> String {
    obj.element_opt(tag)
        .ok()
        .flatten()
        .and_then(|e| e.to_str().ok().map(|s| s.trim().trim_end_matches('\0').trim().to_string()))
        .unwrap_or_default()
}

fn element_strs(obj: &InMemDicomObject, tag: Tag) -> Vec<String> {
    let s = element_str(obj, tag);
    if s.is_empty() {
        return Vec::new();
    }
    s.split('\\').map(|v| v.trim().to_uppercase()).collect()
}

fn element_f64(obj: &InMemDicomObject, tag: Tag) -> Option<f64> {
    element_str(obj, tag).split('\\').next().and_then(|s| s.trim().parse::<f64>().ok())
}

fn element_f64s(obj: &InMemDicomObject, tag: Tag) -> Vec<f64> {
    element_str(obj, tag).split('\\').filter_map(|s| s.trim().parse::<f64>().ok()).collect()
}

fn element_i32(obj: &InMemDicomObject, tag: Tag) -> Option<i32> {
    let e = obj.element_opt(tag).ok().flatten()?;
    if let Some(v) = e.to_str().ok().and_then(|s| s.trim().trim_end_matches('\0').trim().parse::<i32>().ok()) {
        return Some(v);
    }
    // A private element read without its VR (implicit-VR files): two little-endian bytes.
    let b = e.to_bytes().ok()?;
    (b.len() == 2).then(|| i16::from_le_bytes([b[0], b[1]]) as i32)
}

fn first_item(obj: &InMemDicomObject, tag: Tag) -> Option<&InMemDicomObject> {
    obj.element_opt(tag).ok().flatten()?.items()?.first()
}

/// Seconds since midnight from a DICOM TM (`HHMMSS.frac`) or DT (`YYYYMMDDHHMMSS.frac`) value.
fn parse_time_to_seconds(s: &str) -> Option<f64> {
    let s = s.trim();
    // A DT value starts with the date, YYYYMMDD; a TM value is at most 13 characters.
    let s = if s.len() >= 14 && s.is_char_boundary(8) && s[..8].chars().all(|c| c.is_ascii_digit()) { &s[8..] } else { s };
    if s.len() < 6 || !s.is_char_boundary(6) {
        return None;
    }
    let hours: f64 = s[..2].parse().ok()?;
    let minutes: f64 = s[2..4].parse().ok()?;
    let seconds: f64 = s[4..].split(['+', '-']).next()?.parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// The metadata grouping, classification and naming use, from a parsed header. `None` without a
/// SeriesInstanceUID.
fn file_meta(obj: &InMemDicomObject, path: &Path) -> Option<FileMeta> {
    let series_uid = element_str(obj, tags::SERIES_INSTANCE_UID);
    if series_uid.is_empty() {
        return None;
    }
    let clean_id = clean_bids_label(&element_str(obj, tags::PATIENT_ID));
    let clean_name = clean_bids_label(&element_str(obj, tags::PATIENT_NAME));
    let patient_id = if !clean_id.is_empty() {
        clean_id
    } else if !clean_name.is_empty() {
        clean_name
    } else {
        "unknown".to_string()
    };

    let mut m = FileMeta {
        path: path.to_path_buf(),
        patient_id,
        study_uid: element_str(obj, tags::STUDY_INSTANCE_UID),
        study_date: element_str(obj, tags::STUDY_DATE),
        series_uid,
        series_description: element_str(obj, tags::SERIES_DESCRIPTION),
        protocol_name: element_str(obj, tags::PROTOCOL_NAME),
        sequence_name: element_str(obj, tags::SEQUENCE_NAME),
        series_number: element_i32(obj, tags::SERIES_NUMBER).unwrap_or(0),
        instance_number: element_i32(obj, tags::INSTANCE_NUMBER),
        echo_times: element_f64(obj, tags::ECHO_TIME).into_iter().collect(),
        image_type: element_strs(obj, tags::IMAGE_TYPE),
        ge_image_type: element_i32(obj, Tag(0x0043, 0x102F)),
        complex_component: element_str(obj, tags::COMPLEX_IMAGE_COMPONENT),
        scanning_sequence: element_strs(obj, tags::SCANNING_SEQUENCE),
        inversion_time: element_f64(obj, tags::INVERSION_TIME),
        repetition_time: element_f64(obj, tags::REPETITION_TIME),
        flip_angle: element_f64(obj, tags::FLIP_ANGLE),
        rows: element_i32(obj, tags::ROWS).map(|v| v as u32),
        columns: element_i32(obj, tags::COLUMNS).map(|v| v as u32),
        slice_thickness: element_f64(obj, tags::SLICE_THICKNESS),
        pixel_spacing: element_f64s(obj, tags::PIXEL_SPACING),
        receive_coil: element_str(obj, tags::RECEIVE_COIL_NAME),
        coil_string: element_str(obj, Tag(0x0051, 0x100F)),
        coil_elements: None,
        manufacturer: element_str(obj, tags::MANUFACTURER),
        magnetic_field_strength: element_f64(obj, tags::MAGNETIC_FIELD_STRENGTH),
        acquisition_time: None,
        sop_class_uid: element_str(obj, tags::SOP_CLASS_UID),
        is_enhanced: false,
        frames: element_i32(obj, tags::NUMBER_OF_FRAMES).unwrap_or(1).max(1) as u32,
    };
    m.acquisition_time = [tags::ACQUISITION_TIME, tags::ACQUISITION_DATE_TIME, tags::SERIES_TIME, tags::CONTENT_TIME]
        .into_iter()
        .find_map(|t| parse_time_to_seconds(&element_str(obj, t)));

    // Enhanced (multi-frame) MR: what the frames say.
    let per_frame = obj.element_opt(tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE).ok().flatten().and_then(|e| e.items());
    let shared = first_item(obj, tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE);
    if let Some(frames) = per_frame {
        m.is_enhanced = true;
        let mut components: Vec<String> = Vec::new();
        let mut frame_types: Vec<Vec<String>> = Vec::new();
        for fr in frames {
            let ft = first_item(fr, tags::MR_IMAGE_FRAME_TYPE_SEQUENCE).or_else(|| shared.and_then(|s| first_item(s, tags::MR_IMAGE_FRAME_TYPE_SEQUENCE)));
            if let Some(ft) = ft {
                let c = element_str(ft, tags::COMPLEX_IMAGE_COMPONENT);
                if !c.is_empty() && !components.contains(&c) {
                    components.push(c);
                }
                let t = element_strs(ft, tags::FRAME_TYPE);
                if !t.is_empty() && !frame_types.contains(&t) {
                    frame_types.push(t);
                }
            }
            let echo = first_item(fr, tags::MR_ECHO_SEQUENCE).or_else(|| shared.and_then(|s| first_item(s, tags::MR_ECHO_SEQUENCE)));
            if let Some(te) = echo.and_then(|e| element_f64(e, tags::EFFECTIVE_ECHO_TIME)) {
                if !m.echo_times.iter().any(|&t| (t - te).abs() < 0.01) {
                    m.echo_times.push(te);
                }
            }
            if m.acquisition_time.is_none() {
                m.acquisition_time = first_item(fr, tags::FRAME_CONTENT_SEQUENCE)
                    .and_then(|c| parse_time_to_seconds(&element_str(c, tags::FRAME_ACQUISITION_DATE_TIME)));
            }
        }
        m.echo_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if m.complex_component.is_empty() {
            m.complex_component = if components.len() == 1 { components[0].clone() } else if components.len() > 1 { "MIXED".into() } else { String::new() };
        }
        if m.image_type.is_empty() {
            if let Some(t) = frame_types.first() {
                m.image_type = t.clone();
            }
        }
        let first = frames.first();
        let group = |tag: Tag| first.and_then(|f| first_item(f, tag)).or_else(|| shared.and_then(|s| first_item(s, tag)));
        if let Some(t) = group(tags::MR_TIMING_AND_RELATED_PARAMETERS_SEQUENCE) {
            m.repetition_time = m.repetition_time.or_else(|| element_f64(t, tags::REPETITION_TIME));
            m.flip_angle = m.flip_angle.or_else(|| element_f64(t, tags::FLIP_ANGLE));
        }
        if let Some(px) = group(tags::PIXEL_MEASURES_SEQUENCE) {
            if m.pixel_spacing.is_empty() {
                m.pixel_spacing = element_f64s(px, tags::PIXEL_SPACING);
            }
            m.slice_thickness = m.slice_thickness.or_else(|| element_f64(px, tags::SLICE_THICKNESS));
        }
        if let Some(coil) = group(tags::MR_RECEIVE_COIL_SEQUENCE) {
            if m.receive_coil.is_empty() {
                m.receive_coil = element_str(coil, tags::RECEIVE_COIL_NAME);
            }
            if let Some(items) = coil.element_opt(tags::MULTI_COIL_DEFINITION_SEQUENCE).ok().flatten().and_then(|e| e.items()) {
                m.coil_elements = Some(items.len() as u32);
                if items.len() == 1 && m.coil_string.is_empty() {
                    m.coil_string = element_str(&items[0], tags::MULTI_COIL_ELEMENT_NAME);
                }
            }
        }
    }
    Some(m)
}

/// The first bytes of a DICOM data set written without the Part-10 preamble and file meta group
/// (older Siemens and ACR-NEMA exports), and which VR encoding they use: `Some(true)` explicit,
/// `Some(false)` implicit, `None` when they do not look like a data set.
fn headerless_encoding(head: &[u8]) -> Option<bool> {
    if head.len() < 8 {
        return None;
    }
    let group = u16::from_le_bytes([head[0], head[1]]);
    let element = u16::from_le_bytes([head[2], head[3]]);
    // Data sets start with a low group (identifying 0008, or 0002/0004/0005 in some exports).
    if !(0x0002..=0x0010).contains(&group) || element > 0x1000 {
        return None;
    }
    let explicit = head[4].is_ascii_uppercase() && head[5].is_ascii_uppercase();
    if !explicit {
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        if len > 1 << 16 {
            return None;
        }
    }
    Some(explicit)
}

/// Read a data set with no Part-10 header, up to its Pixel Data.
fn read_headerless(path: &Path, full: bool) -> Option<InMemDicomObject> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 8];
    f.read_exact(&mut head).ok()?;
    let explicit = headerless_encoding(&head)?;
    let mut bytes = head.to_vec();
    f.read_to_end(&mut bytes).ok()?;
    if !full {
        // Stop before Pixel Data (7FE0,0010): the header is all a scan needs.
        if let Some(pos) = bytes.windows(4).position(|w| w == [0xE0, 0x7F, 0x10, 0x00]) {
            bytes.truncate(pos);
        }
    }
    let implicit_ts = dicom::transfer_syntax::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased();
    let explicit_ts = dicom::transfer_syntax::entries::EXPLICIT_VR_LITTLE_ENDIAN.erased();
    let order = if explicit { [&explicit_ts, &implicit_ts] } else { [&implicit_ts, &explicit_ts] };
    let found = order.into_iter().find_map(|ts| {
        InMemDicomObject::read_dataset_with_ts(bytes.as_slice(), ts)
            .ok()
            .filter(|o| !element_str(o, tags::SERIES_INSTANCE_UID).is_empty())
    });
    found
}

/// What reading one file found.
enum Read1 {
    Dicom(Box<FileMeta>),
    NotDicom,
    Empty,
}

/// Read one file's header: a Part-10 file up to its Pixel Data, or a data set without the Part-10
/// header (which dcm2niix converts too).
fn read_dicom_file(path: &Path) -> Read1 {
    match std::fs::metadata(path) {
        Ok(m) if m.len() == 0 => return Read1::Empty,
        Err(_) => return Read1::NotDicom,
        _ => {}
    }
    if let Ok(obj) = OpenFileOptions::new().read_until(tags::PIXEL_DATA).open_file(path) {
        return match file_meta(&obj, path) {
            Some(m) => Read1::Dicom(Box::new(m)),
            None => Read1::NotDicom,
        };
    }
    match read_headerless(path, false).and_then(|o| file_meta(&o, path)) {
        Some(m) => Read1::Dicom(Box::new(m)),
        None => Read1::NotDicom,
    }
}

/// A file's pixel data, for telling a re-exported series from a different reconstruction.
fn pixel_bytes(path: &Path) -> Option<Vec<u8>> {
    let obj: InMemDicomObject = match dicom::object::open_file(path) {
        Ok(o) => o.into_inner(),
        Err(_) => read_headerless(path, true)?,
    };
    obj.element_opt(tags::PIXEL_DATA).ok().flatten()?.to_bytes().ok().map(|b| b.into_owned())
}

/// Whether two series hold the same pixel data, file by file in instance order.
fn same_pixels(a: &DicomSeries, b: &DicomSeries) -> bool {
    a.files.len() == b.files.len()
        && a.files.par_iter().zip(b.files.par_iter()).all(|(fa, fb)| match (pixel_bytes(fa), pixel_bytes(fb)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        })
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Scan a directory for DICOM files and build a structured session. `progress` is incremented
/// for each file examined (DICOM or not).
pub fn scan_dicom_directory(dir: &Path, progress: Arc<AtomicUsize>) -> Result<DicomSession, String> {
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", dir.display()));
    }
    let mut paths = Vec::new();
    collect_files(dir, &mut paths);
    paths.sort();

    let results: Vec<(PathBuf, Read1)> = paths
        .into_par_iter()
        .map(|p| {
            let r = read_dicom_file(&p);
            progress.fetch_add(1, Ordering::Relaxed);
            (p, r)
        })
        .collect();

    let mut files = Vec::new();
    let mut not_dicom = 0;
    let mut empty = Vec::new();
    for (p, r) in results {
        match r {
            Read1::Dicom(m) => files.push(*m),
            Read1::NotDicom => not_dicom += 1,
            Read1::Empty => empty.push(p),
        }
    }
    if files.is_empty() {
        let mut msg = "No DICOM files found".to_string();
        if not_dicom > 0 || !empty.is_empty() {
            msg.push_str(&format!(" ({} files were not DICOM, {} were empty)", not_dicom, empty.len()));
        }
        return Err(msg);
    }

    let mut session = build_session(files, same_pixels);
    if !empty.is_empty() {
        session.report.warnings.insert(0, format!(
            "{} empty (0-byte) file{} found, e.g. {}; the series {} belonged to may be incomplete",
            empty.len(), if empty.len() == 1 { " was" } else { "s were" }, empty[0].display(),
            if empty.len() == 1 { "it" } else { "they" }
        ));
    }
    session.report.not_dicom = not_dicom;
    session.report.empty_files = empty;
    Ok(session)
}

#[cfg(test)]
mod read_tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time_to_seconds("120000.000"), Some(43200.0));
        assert_eq!(parse_time_to_seconds("000100"), Some(60.0));
        assert_eq!(parse_time_to_seconds("20240101000100.5"), Some(60.5));
        assert_eq!(parse_time_to_seconds(""), None);
    }

    #[test]
    fn headerless_detection() {
        // (0008,0005) CS explicit / implicit, and something that is not DICOM.
        assert_eq!(headerless_encoding(&[0x08, 0x00, 0x05, 0x00, b'C', b'S', 0x0A, 0x00]), Some(true));
        assert_eq!(headerless_encoding(&[0x08, 0x00, 0x05, 0x00, 0x0A, 0x00, 0x00, 0x00]), Some(false));
        assert_eq!(headerless_encoding(b"\x1f\x8b\x08\x00\x00\x00\x00\x00"), None);
        assert_eq!(headerless_encoding(b"{\"Echo"), None);
    }
}
