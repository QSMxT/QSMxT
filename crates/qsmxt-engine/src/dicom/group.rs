//! Grouping DICOM files into subject → session → acquisition → run → reconstruction.
//!
//! - **Series first.** Files are grouped by SeriesInstanceUID before anything else, so a series is
//!   never split across subjects. When the files of one series disagree on the patient (an
//!   inconsistent de-identification, say), the series is filed under the identity most of its
//!   files carry, with a warning naming the odd files.
//! - **Reconstructions inside one UID.** Some scanners (Philips, GE) put magnitude, phase, real
//!   and imaginary images under one SeriesInstanceUID; those are split into one series each.
//! - **Session**: the subject's study date.
//! - **Acquisition**: the protocol name (the series description when it is empty, then the
//!   sequence name).
//! - **Run**: series of one acquisition acquired within a minute of each other are one run. Runs
//!   are then compared on their settings (echo times, TR, flip angle, matrix, slice thickness,
//!   voxel size, receive coil), as dicompare does: a run with different settings is a different
//!   acquisition (`acq-<name>2`), and runs with the same settings are `run-1`, `run-2`, ….
//! - **Reconstruction**: series in one run were acquired together and reconstructed several ways.
//!   Two ORIGINAL series of the same part and echoes with no marker telling them apart are the
//!   same images exported twice (the second is skipped) or, when their pixels differ, an extra
//!   reconstruction kept in `extra_data/`. [`super::naming`] gives the others `rec-` labels.

use std::collections::BTreeMap;

use super::classify::{classify_series, file_component, recon_tokens, SeriesFacts};
use super::{
    clean_bids_label, normalize_series_description, push_te, same_te, CoilGroup, CoilType, DicomAcquisition,
    DicomSeries, DicomSession, DicomStudy, DicomSubject, FileMeta, ScanReport, SeriesType,
};

/// Storage SOP classes that are images QSMxT converts: MR Image, Enhanced MR Image and Legacy
/// Converted Enhanced MR Image. An empty SOP class (some headerless files) is let through.
fn is_image_sop(uid: &str) -> bool {
    matches!(
        uid.trim(),
        "" | "1.2.840.10008.5.1.4.1.1.4" | "1.2.840.10008.5.1.4.1.1.4.1" | "1.2.840.10008.5.1.4.1.1.4.4"
    )
}

/// A readable name for a SOP class the scan skips.
fn sop_name(uid: &str) -> String {
    let name = match uid.trim() {
        "1.2.840.10008.5.1.4.1.1.7" => "secondary capture",
        "1.2.840.10008.5.1.4.1.1.11.1" => "presentation state",
        "1.2.840.10008.5.1.4.1.1.66" => "raw data",
        "1.2.840.10008.5.1.4.1.1.88.11" | "1.2.840.10008.5.1.4.1.1.88.22" | "1.2.840.10008.5.1.4.1.1.88.33" => "structured report",
        "1.2.840.10008.5.1.4.1.1.104.1" => "encapsulated PDF",
        "1.2.840.10008.5.1.4.1.1.4.2" => "MR spectroscopy",
        u if u.starts_with("1.3.46.670589.") => "Philips private data",
        u if u.starts_with("1.3.12.2.1107.") => "Siemens private data",
        _ => "",
    };
    if name.is_empty() { format!("SOP class {}", uid.trim()) } else { name.to_string() }
}

/// The value most items have; ties go to the one that sorts first.
fn majority<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for v in values {
        *counts.entry(v).or_default() += 1;
    }
    let max = counts.values().copied().max().unwrap_or(0);
    counts.into_iter().find(|(_, c)| *c == max).map(|(v, _)| v.to_string()).unwrap_or_default()
}

/// Whether a Siemens coil string names one coil element (`H15`, `A32`) rather than a combination
/// (`HEA;HEP`, `HC1-7`, `HE1,2`) or a coil without element numbers.
fn single_element(coil: &str) -> bool {
    let c = coil.trim();
    if c.is_empty() || c.contains(';') || c.contains(',') {
        return false;
    }
    if !c.chars().any(|ch| ch.is_ascii_digit()) {
        return false;
    }
    if let Some((l, r)) = c.split_once('-') {
        if l.chars().any(|ch| ch.is_ascii_digit()) && r.chars().any(|ch| ch.is_ascii_digit()) {
            return false;
        }
    }
    true
}

/// The coil-element number in a coil string, e.g. `H15` → 15.
pub fn extract_coil_num(coil_str: &str) -> Option<u32> {
    let digits: String = coil_str
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn fmt_num(v: f64) -> String {
    let s = format!("{:.3}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The `field=value` settings of a group of files, sorted and distinct.
fn settings_of(files: &[&FileMeta], coil_type: CoilType) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for f in files {
        for &te in &f.echo_times {
            v.push(format!("te={}", fmt_num((te * 100.0).round() / 100.0)));
        }
        if let Some(x) = f.repetition_time { v.push(format!("tr={}", fmt_num(x))); }
        if let Some(x) = f.flip_angle { v.push(format!("flip={}", fmt_num(x))); }
        if let (Some(r), Some(c)) = (f.rows, f.columns) { v.push(format!("matrix={r}x{c}")); }
        if let Some(x) = f.slice_thickness { v.push(format!("thickness={}", fmt_num(x))); }
        if !f.pixel_spacing.is_empty() {
            v.push(format!("voxel={}", f.pixel_spacing.iter().map(|p| fmt_num(*p)).collect::<Vec<_>>().join("x")));
        }
        if !f.receive_coil.trim().is_empty() { v.push(format!("coil={}", f.receive_coil.trim())); }
    }
    v.push(format!("coiltype={}", coil_type.label()));
    v.sort();
    v.dedup();
    v
}

/// A series built from files, with the name its acquisition is grouped by.
struct Built {
    series: DicomSeries,
    acq_base: String,
    referenced_series: Vec<String>,
    source_images: Vec<String>,
    sop_uids: std::collections::HashSet<String>,
}

/// The series a derived series without a protocol name (a Bruker parametric map) was made from,
/// and whether the link is firm enough for the map to follow that series into `extra_data/`.
///
/// In order: the series it references; the series holding an image it names as its source; for
/// Bruker's numbering (scan x 10000 + reconstruction), the latest earlier reconstruction of the
/// same scan, so 90002 is made from 90001 and 90004 from 90003; the series acquired at the same
/// time, the nearest before it in number. All reconstructions of a Bruker scan share an
/// acquisition time, so the numbering has to come before the time.
///
/// A series classified as not for QSM is no source by image or by time: ReferencedImageSequence
/// usually names the localizer the slices were planned on, and reformats share the time of the
/// scan they reformat. Several series at the same time do not say which one it came from, so that
/// link only files the map with the acquisition.
fn parent_of(built: &[Built], i: usize) -> Option<(usize, bool)> {
    let b = &built[i];
    let base_uid = |s: &DicomSeries| s.series_uid.split('#').next().unwrap_or("").to_string();
    let candidates = || built.iter().enumerate().filter(move |(j, c)| *j != i && !c.series.protocol_name.trim().is_empty());
    if let Some((j, _)) = candidates().find(|(_, c)| b.referenced_series.contains(&base_uid(&c.series))) {
        return Some((j, true));
    }
    let for_qsm = |c: &Built| c.series.series_type != SeriesType::Extra;
    if let Some((j, _)) = candidates().find(|(_, c)| for_qsm(c) && b.source_images.iter().any(|u| c.sop_uids.contains(u))) {
        return Some((j, true));
    }
    let n = b.series.series_number;
    if n >= 10000 {
        let earlier = candidates()
            .filter(|(_, c)| c.series.series_number < n && c.series.series_number / 10000 == n / 10000)
            .max_by_key(|(_, c)| c.series.series_number);
        if let Some((j, _)) = earlier {
            return Some((j, true));
        }
    }
    let same_time: Vec<(usize, i32)> = candidates()
        .filter(|(_, c)| for_qsm(c) && matches!((b.series.acquisition_time, c.series.acquisition_time), (Some(x), Some(y)) if (x - y).abs() < 1.0))
        .map(|(j, c)| (j, c.series.series_number))
        .collect();
    let nearest = same_time.iter().filter(|(_, m)| *m < n).max_by_key(|(_, m)| *m).or_else(|| same_time.iter().min_by_key(|(_, m)| *m))?;
    Some((nearest.0, same_time.len() == 1))
}

/// One series from a set of files of one SeriesInstanceUID.
fn build_series(uid: String, files: &[&FileMeta]) -> Built {
    // The series' names are what most of its files say (one odd file must not rename it).
    let protocol = majority(files.iter().map(|f| f.protocol_name.as_str()));
    let raw_description = majority(files.iter().map(|f| f.series_description.as_str()));
    let first = files
        .iter()
        .copied()
        .find(|f| f.protocol_name == protocol && f.series_description == raw_description)
        .unwrap_or(files[0]);
    let description = normalize_series_description(&raw_description);
    let component = file_component(first);

    let mut echo_times = Vec::new();
    let mut inversion_times = Vec::new();
    for f in files {
        for &te in &f.echo_times {
            push_te(&mut echo_times, te);
        }
        if let Some(ti) = f.inversion_time {
            push_te(&mut inversion_times, ti);
        }
    }

    let mut times: Vec<f64> = files.iter().filter_map(|f| f.acquisition_time).collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let acquisition_time = times.get(times.len() / 2).copied();

    // Coil type: per-coil (uncombined) only when the series holds at least two different single
    // coil elements, or an Enhanced file says each of its images comes from one element. One
    // element name on its own (a 7T `A32`) is the scanner's combined image.
    let mut coils: Vec<&str> = files.iter().map(|f| f.coil_string.trim()).filter(|c| !c.is_empty()).collect();
    coils.sort_unstable();
    coils.dedup();
    let enhanced_single = files.iter().any(|f| f.coil_elements == Some(1));
    let coil_type = if (coils.len() >= 2 && coils.iter().all(|c| single_element(c))) || (enhanced_single && !coils.is_empty()) {
        CoilType::Uncombined
    } else if !coils.is_empty() {
        CoilType::Combined
    } else {
        CoilType::Unknown
    };
    let coil_groups = if coil_type == CoilType::Uncombined {
        let mut groups: Vec<CoilGroup> = Vec::new();
        for f in files {
            let num = extract_coil_num(&f.coil_string);
            match groups.iter_mut().find(|g| g.coil_num == num) {
                Some(g) => g.files.push(f.path.clone()),
                None => groups.push(CoilGroup { coil_num: num, files: vec![f.path.clone()] }),
            }
        }
        groups.sort_by_key(|g| g.coil_num);
        groups
    } else {
        Vec::new()
    };

    let facts = SeriesFacts {
        image_type: &first.image_type,
        component,
        description: &first.series_description,
        protocol: &first.protocol_name,
        scanning_sequence: &first.scanning_sequence,
        inversion_time: first.inversion_time,
    };
    let series_type = classify_series(&facts);

    let acq_base = [first.protocol_name.as_str(), description.as_str(), first.sequence_name.as_str()]
        .iter()
        .map(|s| clean_bids_label(s))
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    Built {
        acq_base,
        series: DicomSeries {
            series_uid: uid,
            description,
            protocol_name: first.protocol_name.clone(),
            series_number: first.series_number,
            image_type: first.image_type.clone(),
            echo_time: echo_times.first().copied(),
            magnetic_field_strength: first.magnetic_field_strength,
            num_files: files.len(),
            num_images: files.iter().map(|f| f.frames.max(1) as usize).sum(),
            series_type,
            auto_type: series_type,
            echo_times,
            files: files.iter().map(|f| f.path.clone()).collect(),
            manufacturer: first.manufacturer.clone(),
            coil_type,
            coil_groups,
            original: first.image_type.first().is_some_and(|t| t == "ORIGINAL"),
            inversion_times,
            acquisition_time,
            settings: settings_of(files, coil_type),
            note: None,
            file_echo_times: files.iter().map(|f| f.echo_times.first().copied()).collect(),
            file_positions: files.iter().map(|f| f.image_position.clone()).collect(),
        },
        referenced_series: files.iter().flat_map(|f| f.referenced_series.iter().cloned()).collect(),
        source_images: files.iter().flat_map(|f| f.source_images.iter().cloned()).take(16).collect(),
        sop_uids: files.iter().map(|f| f.sop_instance_uid.clone()).filter(|u| !u.is_empty()).collect(),
    }
}

/// Split one SeriesInstanceUID's files into series: one per reconstruction when the UID holds
/// several (Philips and GE put magnitude and phase, or real and imaginary, under one UID). A lone
/// odd file that is not a gradient-echo part stays with the rest.
fn split_uid(uid: &str, files: &[FileMeta]) -> Vec<Built> {
    type Key = (SeriesType, Vec<String>);
    let mut groups: Vec<(Key, Vec<&FileMeta>)> = Vec::new();
    for f in files {
        let facts = SeriesFacts {
            image_type: &f.image_type,
            component: file_component(f),
            description: &f.series_description,
            protocol: &f.protocol_name,
            scanning_sequence: &f.scanning_sequence,
            inversion_time: f.inversion_time,
        };
        let key: Key = (classify_series(&facts), recon_tokens(&f.image_type));
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(f),
            None => groups.push((key, vec![f])),
        }
    }
    if groups.len() > 1 {
        // Fold single odd files that are not gradient-echo parts into the largest group.
        let largest = groups.iter().enumerate().max_by_key(|(_, (_, g))| g.len()).map(|(i, _)| i).unwrap_or(0);
        let mut kept: Vec<(Key, Vec<&FileMeta>)> = Vec::new();
        let mut folded: Vec<&FileMeta> = Vec::new();
        for (i, (k, g)) in groups.into_iter().enumerate() {
            if i != largest && g.len() < 2 && !k.0.is_complex_part() {
                folded.extend(g);
            } else {
                kept.push((k, g));
            }
        }
        if let Some((_, g)) = kept.iter_mut().max_by_key(|(_, g)| g.len()) {
            g.extend(folded);
        }
        groups = kept;
    }
    if groups.len() == 1 {
        let all: Vec<&FileMeta> = files.iter().collect();
        return vec![build_series(uid.to_string(), &all)];
    }
    groups
        .into_iter()
        .map(|((t, tokens), g)| {
            let suffix = if tokens.is_empty() { t.label().replace(' ', "") } else { format!("{}{}", t.label().replace(' ', ""), tokens.join("")) };
            build_series(format!("{uid}#{suffix}"), &g)
        })
        .collect()
}

/// Group series into runs: series whose median acquisition times lie within 60 s of the first
/// series of a cluster are one run. Series without a time join the run whose series number is
/// closest.
pub fn assign_runs_temporal(series: &[DicomSeries]) -> Vec<u32> {
    let gap_threshold = 60.0;
    let mut timed: Vec<(usize, f64)> = Vec::new();
    let mut untimed: Vec<usize> = Vec::new();
    for (i, s) in series.iter().enumerate() {
        match s.acquisition_time {
            Some(t) => timed.push((i, t)),
            None => untimed.push(i),
        }
    }
    let mut runs = vec![1u32; series.len()];
    if timed.is_empty() {
        return runs;
    }
    timed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut current = 1u32;
    let mut start = timed[0].1;
    for &(idx, t) in &timed {
        if t - start > gap_threshold {
            current += 1;
            start = t;
        }
        runs[idx] = current;
    }
    for &idx in &untimed {
        let n = series[idx].series_number;
        runs[idx] = timed
            .iter()
            .min_by_key(|&&(ti, _)| (series[ti].series_number - n).unsigned_abs())
            .map(|&(ti, _)| runs[ti])
            .unwrap_or(1);
    }
    runs
}

/// The settings a run is compared on: those of its gradient-echo parts and structurals (the
/// acquisition itself), or of all its series when it has none.
fn run_signature(run: &[DicomSeries]) -> Vec<String> {
    let primary: Vec<&DicomSeries> = run
        .iter()
        .filter(|s| s.auto_type.is_complex_part() || matches!(s.auto_type, SeriesType::T1w | SeriesType::Inversion))
        .collect();
    let chosen: Vec<&DicomSeries> = if primary.is_empty() { run.iter().collect() } else { primary };
    let mut v: Vec<String> = chosen.iter().flat_map(|s| s.settings.iter().cloned()).collect();
    v.sort();
    v.dedup();
    v
}

/// Whether two series hold the same echoes (or neither has echo times).
fn tes_overlap(a: &DicomSeries, b: &DicomSeries) -> bool {
    if a.echo_times.is_empty() || b.echo_times.is_empty() {
        return a.echo_times.is_empty() && b.echo_times.is_empty();
    }
    a.echo_times.iter().any(|&x| b.echo_times.iter().any(|&y| same_te(x, y)))
}

fn same_coils(a: &DicomSeries, b: &DicomSeries) -> bool {
    a.coil_type == b.coil_type
        && a.coil_groups.iter().map(|g| g.coil_num).collect::<Vec<_>>() == b.coil_groups.iter().map(|g| g.coil_num).collect::<Vec<_>>()
}

/// Whether a series is a plain ORIGINAL image of its kind: nothing in its ImageType marks it as a
/// particular reconstruction.
pub fn is_plain_original(s: &DicomSeries) -> bool {
    s.original && recon_tokens(&s.image_type).is_empty()
}

/// Two plain ORIGINAL series of the same part, echoes and coils in one run: the same images
/// exported twice (skip the second), or a second reconstruction nothing marks (keep it in
/// `extra_data/`).
fn resolve_duplicates<F>(run_label: &str, series: &mut [DicomSeries], report: &mut ScanReport, identical: &mut F)
where
    F: FnMut(&DicomSeries, &DicomSeries) -> bool,
{
    for j in 1..series.len() {
        for i in 0..j {
            let (left, right) = series.split_at_mut(j);
            let (a, b) = (&left[i], &mut right[0]);
            let kind = a.series_type;
            if kind != b.series_type
                || !(kind.is_complex_part() || matches!(kind, SeriesType::T1w | SeriesType::Inversion))
                || !is_plain_original(a)
                || !is_plain_original(b)
                || !same_coils(a, b)
                || !tes_overlap(a, b)
                || a.inversion_times != b.inversion_times
            {
                continue;
            }
            if identical(a, b) {
                b.note = Some(format!("same images as {}; not converted again", a.display_name()));
                report.notes.push(format!(
                    "{run_label}: {} has the same images as {}; converting {} only",
                    b.display_name(), a.display_name(), a.display_name()
                ));
                b.series_type = SeriesType::Skip;
            } else {
                b.note = Some(format!("a second {} reconstruction next to {}; kept in extra_data", kind.label().to_lowercase(), a.display_name()));
                report.warnings.push(format!(
                    "{run_label}: {} and {} are both original {} images of the same acquisition but differ; \
                     {} is used, {} is kept in extra_data",
                    a.display_name(), b.display_name(), kind.label().to_lowercase(), a.display_name(), b.display_name()
                ));
                b.series_type = SeriesType::Extra;
                b.auto_type = SeriesType::Extra;
            }
        }
    }
}

/// Magnitude and phase (or real and imaginary) of one run must have as many images; a difference
/// means an incomplete export, which `qsmxt run` would otherwise stop on with a size mismatch.
fn check_part_counts(run_label: &str, series: &[DicomSeries], report: &mut ScanReport) {
    for coil in [CoilType::Combined, CoilType::Uncombined, CoilType::Unknown] {
        let count = |t: SeriesType| -> usize {
            series
                .iter()
                .filter(|s| s.series_type == t && s.coil_type == coil && recon_tokens(&s.image_type).is_empty())
                .map(|s| s.num_images)
                .sum()
        };
        for (a, b) in [(SeriesType::Magnitude, SeriesType::Phase), (SeriesType::Real, SeriesType::Imaginary)] {
            let (na, nb) = (count(a), count(b));
            if na > 0 && nb > 0 && na != nb {
                report.warnings.push(format!(
                    "{run_label}: the {} has {na} images but the {} has {nb}; the export is probably incomplete",
                    a.label().to_lowercase(), b.label().to_lowercase()
                ));
            }
        }
    }
}

/// Instance numbers of a single-frame series with gaps (some images missing from the export).
fn check_instance_gaps(files: &[FileMeta], report: &mut ScanReport) {
    // Multi-frame files, and UIDs holding several kinds of image (Philips numbers those its own
    // way), have no reliable count.
    if files.iter().any(|f| f.frames > 1) || files.iter().any(|f| f.image_type != files[0].image_type) {
        return;
    }
    let mut nums: Vec<i32> = match files.iter().map(|f| f.instance_number).collect::<Option<Vec<_>>>() {
        Some(v) if v.len() >= 2 => v,
        _ => return,
    };
    nums.sort_unstable();
    let n = nums.len();
    nums.dedup();
    if nums.len() != n {
        return; // repeated instance numbers: no reliable count
    }
    let (lo, hi) = (nums[0], nums[n - 1]);
    let missing = (hi - lo + 1) as usize - n;
    if missing > 0 && missing < n {
        let f = &files[0];
        report.warnings.push(format!(
            "series {} '{}': {} images with instance numbers {lo}–{hi}, {missing} missing; the export is probably incomplete",
            f.series_number, normalize_series_description(&f.series_description), n
        ));
    }
}

/// Group a scan's files into a session. `identical` tells whether two series hold the same pixel
/// data; it is asked only for two plain ORIGINAL series of the same part and echoes in one run.
pub fn build_session<F>(files: Vec<FileMeta>, mut identical: F) -> DicomSession
where
    F: FnMut(&DicomSeries, &DicomSeries) -> bool,
{
    let mut report = ScanReport::default();

    let mut by_uid: BTreeMap<String, Vec<FileMeta>> = BTreeMap::new();
    for f in files {
        by_uid.entry(f.series_uid.clone()).or_default().push(f);
    }

    // subject → date → series (with the acquisition name each is grouped by)
    let mut tree: BTreeMap<String, BTreeMap<String, Vec<Built>>> = BTreeMap::new();
    for (uid, mut files) in by_uid {
        files.sort_by(|a, b| (a.instance_number, &a.path).cmp(&(b.instance_number, &b.path)));
        let (images, other): (Vec<FileMeta>, Vec<FileMeta>) = files.into_iter().partition(|f| is_image_sop(&f.sop_class_uid));
        if let Some(f) = other.first() {
            report.skipped.push(format!(
                "series {} '{}' ({}, {} file{})",
                f.series_number, normalize_series_description(&f.series_description), sop_name(&f.sop_class_uid),
                other.len(), if other.len() == 1 { "" } else { "s" }
            ));
        }
        if images.is_empty() {
            continue;
        }

        let subject = majority(images.iter().map(|f| f.patient_id.as_str()));
        let odd: Vec<&FileMeta> = images.iter().filter(|f| f.patient_id != subject).collect();
        if !odd.is_empty() {
            let f = images.iter().find(|f| f.patient_id == subject).unwrap_or(&images[0]);
            let shown: Vec<String> = odd.iter().take(3).map(|o| format!("{} ({})", o.path.display(), o.patient_id)).collect();
            report.warnings.push(format!(
                "series {} '{}': {} of its {} files name a different patient: {}{}; the whole series is filed under sub-{} like its other files",
                f.series_number, normalize_series_description(&f.series_description), odd.len(), images.len(),
                shown.join(", "), if odd.len() > 3 { ", …" } else { "" }, subject
            ));
        }
        let date = majority(images.iter().map(|f| f.study_date.as_str()));
        check_instance_gaps(&images, &mut report);

        tree.entry(subject).or_default().entry(date).or_default().extend(split_uid(&uid, &images));
    }

    let mut subjects = Vec::new();
    for (patient_id, dates) in tree {
        let mut studies = Vec::new();
        for (study_date, mut built) in dates {
            let mut made_from: BTreeMap<String, String> = BTreeMap::new();
            // A derived series without a protocol name joins the acquisition it was made from.
            for i in 0..built.len() {
                if !built[i].series.protocol_name.trim().is_empty() || built[i].series.original {
                    continue;
                }
                if let Some((j, firm)) = parent_of(&built, i) {
                    let (acq, time, name) = (built[j].acq_base.clone(), built[j].series.acquisition_time, built[j].series.display_name());
                    built[i].acq_base = acq;
                    built[i].series.acquisition_time = time;
                    built[i].series.note = Some(format!("made from {name}"));
                    if firm {
                        made_from.insert(built[i].series.series_uid.clone(), built[j].series.series_uid.clone());
                    }
                }
            }
            let mut acquisitions = form_acquisitions(&patient_id, built, &mut report, &mut identical);
            follow_into_extra(&patient_id, &mut acquisitions, &made_from, &mut report);
            studies.push(DicomStudy { study_date, acquisitions });
        }
        subjects.push(DicomSubject { patient_id, studies });
    }
    DicomSession { subjects, report }
}

/// A derived series made from a series that went to `extra_data/` goes there too. Its own row in
/// the listing comes from `auto_type`, as this is classification rather than a relabelling.
fn follow_into_extra(subject: &str, acquisitions: &mut [DicomAcquisition], made_from: &BTreeMap<String, String>, report: &mut ScanReport) {
    let mut extra: BTreeMap<String, String> = BTreeMap::new();
    for s in acquisitions.iter().flat_map(|a| a.series.iter()) {
        if s.series_type == SeriesType::Extra {
            extra.insert(s.series_uid.clone(), s.display_name());
        }
    }
    for s in acquisitions.iter_mut().flat_map(|a| a.series.iter_mut()) {
        let Some(source) = made_from.get(&s.series_uid).and_then(|p| extra.get(p)) else { continue };
        if s.series_type.is_bids() {
            s.series_type = SeriesType::Extra;
            s.auto_type = SeriesType::Extra;
            s.note = Some(format!("made from {source}, which is kept in extra_data"));
            report.notes.push(format!("sub-{subject}: {} was made from {source}, which is kept in extra_data; it is kept there too", s.display_name()));
        }
    }
}

/// Acquisitions (one per run) of one session.
fn form_acquisitions<F>(subject: &str, built: Vec<Built>, report: &mut ScanReport, identical: &mut F) -> Vec<DicomAcquisition>
where
    F: FnMut(&DicomSeries, &DicomSeries) -> bool,
{
    let mut by_name: BTreeMap<String, Vec<DicomSeries>> = BTreeMap::new();
    for b in built {
        by_name.entry(b.acq_base).or_default().push(b.series);
    }
    let mut taken: Vec<String> = by_name.keys().cloned().collect();

    let mut out: Vec<DicomAcquisition> = Vec::new();
    for (name, mut list) in by_name {
        list.sort_by_key(|s| s.series_number);
        let runs = assign_runs_temporal(&list);
        let max_run = runs.iter().copied().max().unwrap_or(1);
        let mut buckets: Vec<Vec<DicomSeries>> = (0..max_run).map(|_| Vec::new()).collect();
        for (s, r) in list.into_iter().zip(runs) {
            buckets[(r - 1) as usize].push(s);
        }
        buckets.retain(|b| !b.is_empty());

        // Runs with the same settings are repeats; different settings make another acquisition.
        let mut groups: Vec<(Vec<String>, Vec<Vec<DicomSeries>>)> = Vec::new();
        for run in buckets {
            let sig = run_signature(&run);
            match groups.iter_mut().find(|(g, _)| *g == sig) {
                Some((_, runs)) => runs.push(run),
                None => groups.push((sig, vec![run])),
            }
        }
        let n_groups = groups.len();
        for (gi, (_, runs)) in groups.into_iter().enumerate() {
            let acq_name = if gi == 0 {
                name.clone()
            } else {
                let mut k = gi + 1;
                let mut candidate = format!("{name}{k}");
                while taken.contains(&candidate) {
                    k += 1;
                    candidate = format!("{name}{k}");
                }
                taken.push(candidate.clone());
                candidate
            };
            if gi > 0 {
                report.notes.push(format!(
                    "sub-{subject}: protocol {name} was run with different settings ({n_groups} variants); \
                     series {} are acq-{acq_name}",
                    {
                        let mut nums: Vec<i32> = runs.iter().flatten().map(|s| s.series_number).collect();
                        nums.sort_unstable();
                        nums.dedup();
                        nums.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
                    }
                ));
            }
            let n_runs = runs.len();
            for (ri, mut run) in runs.into_iter().enumerate() {
                let label = if n_runs > 1 { format!("sub-{subject} acq-{acq_name} run-{}", ri + 1) } else { format!("sub-{subject} acq-{acq_name}") };
                resolve_duplicates(&label, &mut run, report, identical);
                check_part_counts(&label, &run, report);
                out.push(DicomAcquisition { name: acq_name.clone(), run_number: (ri + 1) as u32, series: run });
            }
        }
    }
    out.sort_by_key(|a| (a.series.iter().map(|s| s.series_number).min().unwrap_or(0), a.name.clone(), a.run_number));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coil_strings() {
        assert!(single_element("H1"));
        assert!(single_element("A32"));
        assert!(!single_element("HEA;HEP"));
        assert!(!single_element("HC1-7"));
        assert!(!single_element("HE1,2"));
        assert!(!single_element("HEA"));
        assert!(!single_element(""));
        assert_eq!(extract_coil_num("H15"), Some(15));
        assert_eq!(extract_coil_num("HEA;HEP"), None);
    }

    #[test]
    fn majority_prefers_the_most_common_then_the_first() {
        assert_eq!(majority(["b", "a", "b"].into_iter()), "b");
        assert_eq!(majority(["b", "a"].into_iter()), "a");
    }
}
