use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

use super::{clean_bids_label, find_collisions, parse_dcm2niix_suffix, plan_session, CoilType, DicomSession, SeriesType, Target};

/// Message sent from the conversion thread to the UI.
pub enum ConvertMessage {
    Log(String),
    Error(String),
    Done { bids_dir: PathBuf },
}

/// dcm2niix version bundled with release builds (see .github/workflows/release.yml).
/// Kept in sync with the `DCM2NIIX_VERSION` pin in the release workflow.
pub const DCM2NIIX_BUNDLED_VERSION: &str = "v1.0.20260724";

/// Bare executable name for the current platform.
fn dcm2niix_exe_name() -> &'static str {
    if cfg!(target_os = "windows") { "dcm2niix.exe" } else { "dcm2niix" }
}

/// Directory where the bundled dcm2niix is installed (`~/.qsmxt/bin`).
/// Matches the install scripts and `qsmxt update`.
pub fn qsmxt_bin_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".qsmxt").join("bin"))
}

/// Resolve the dcm2niix executable, preferring the bundled copy over PATH.
///
/// Resolution order:
/// 1. `QSMXT_DCM2NIIX` env override (explicit path)
/// 2. bundled copy in `~/.qsmxt/bin`
/// 3. sibling of the current executable
/// 4. PATH (`which`/`where`)
///
/// Returns `None` if dcm2niix cannot be found anywhere.
pub fn find_dcm2niix() -> Option<PathBuf> {
    let exe = dcm2niix_exe_name();

    // 1. Explicit override.
    if let Some(p) = std::env::var_os("QSMXT_DCM2NIIX") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. Bundled copy in ~/.qsmxt/bin.
    if let Some(dir) = qsmxt_bin_dir() {
        let candidate = dir.join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // 3. Sibling of the current executable.
    if let Ok(cur) = std::env::current_exe() {
        if let Some(parent) = cur.parent() {
            let candidate = parent.join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // 4. PATH lookup.
    find_dcm2niix_on_path()
}

/// Look up dcm2niix on PATH via `which` (Unix) or `where` (Windows).
fn find_dcm2niix_on_path() -> Option<PathBuf> {
    let output = Command::new("which").arg("dcm2niix").output().ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() { None } else { Some(PathBuf::from(path)) }
    } else {
        let output = Command::new("where").arg("dcm2niix").output().ok()?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).lines().next()?.trim().to_string();
            if path.is_empty() { None } else { Some(PathBuf::from(path)) }
        } else {
            None
        }
    }
}

// ─── 4D file detection ───

/// Check if a NIfTI file is 4D by reading the header only.
/// Returns the 4th dimension size if > 1.
pub fn nifti_4d_size(path: &Path) -> Option<usize> {
    use nifti::NiftiObject;
    let obj = nifti::ReaderOptions::new().read_file(path).ok()?;
    let header = obj.header();
    if header.dim[0] >= 4 && header.dim[4] > 1 {
        Some(header.dim[4] as usize)
    } else {
        None
    }
}

// ─── JSON sidecar post-processing ───

/// Convert a .nii.gz path to its .json sidecar path.
pub fn nii_to_json_path(nii_path: &Path) -> PathBuf {
    let s = nii_path.to_string_lossy();
    let base = s.strip_suffix(".nii.gz")
        .or_else(|| s.strip_suffix(".nii"))
        .unwrap_or(&s);
    PathBuf::from(format!("{}.json", base))
}

// ─── GE detection ───

/// Whether a series' `Manufacturer` (0008,0070) names GE.
///
/// GE writes `GE MEDICAL SYSTEMS` (older systems) or `GE HEALTHCARE`; `GENERAL ELECTRIC` and a
/// bare `GE` turn up in anonymised or re-exported data. Matched as a whole word at the start of
/// the string, so a manufacturer that merely contains the letters (`...IMAGE...`,
/// `...MANAGEMENT...`) is not GE.
fn is_ge_manufacturer(manufacturer: &str) -> bool {
    let m = manufacturer.trim().to_uppercase();
    m == "GE" || m.starts_with("GE ") || m.starts_with("GE_") || m.starts_with("GENERAL ELECTRIC")
}

/// What a GE series needs that QSMxT does not do yet, said once per series (#274).
///
/// GE phase needs its handedness flipped and its slice-alternating offset ("chopping") removed
/// before it is QSM-ready. Real/imaginary series get both from the part-real/part-imag path; a
/// phase-only GE series does not yet.
fn ge_note(series_type: SeriesType) -> Option<&'static str> {
    match series_type {
        SeriesType::Phase => Some(
            "  Warning: GE phase is converted as-is. QSMxT does not yet correct GE phase \
             handedness or slice chopping, so χ from it may have the wrong sign and slice-wise \
             wraps."),
        // Real/imaginary series go to part-real/part-imag, which `qsmxt run` converts (chopping
        // corrected, GE phase negated) — nothing to warn about.
        _ => None,
    }
}

// ─── Main conversion ───

/// `.bidsignore` entries of a converted dataset: the series kept unconverted in `extra_data/`,
/// per-coil images (`coil-` is a QSMxT extension of BIDS), and MP2RAGE inversion images (BIDS
/// requires sidecar fields for them, such as `NumberShots`, that DICOM headers do not carry).
pub const RAW_BIDSIGNORE: &[&str] = &["extra_data", "*_coil-*", "*_MP2RAGE.*"];

const README_EXTRA_NOTE: &str = "Series that QSMxT does not use (scouts, reformats, other contrasts, \
other scanner maps) are kept in sub-*/[ses-*/]extra_data/ as dcm2niix named them. They are not \
converted to BIDS and are listed in .bidsignore.";

/// Write the dataset-level files of a converted dataset: `dataset_description.json` (unless one
/// is there), `.bidsignore` (adding QSMxT's entries to any already there) and a README.
fn write_dataset_files(output_dir: &Path, has_extra: bool) -> std::io::Result<()> {
    let desc = output_dir.join("dataset_description.json");
    if !desc.exists() {
        let json = serde_json::json!({
            "Name": "QSMxT DICOM conversion",
            "BIDSVersion": "1.10.0",
            "DatasetType": "raw",
            "GeneratedBy": [
                {"Name": "qsmxt", "Version": crate::commands::update::describe_version(), "CodeURL": "https://github.com/QSMxT/qsmxt"},
                {"Name": "dcm2niix", "CodeURL": "https://github.com/rordenlab/dcm2niix"}
            ]
        });
        fs::write(&desc, serde_json::to_string_pretty(&json).unwrap_or_default())?;
    }

    let ignore = output_dir.join(".bidsignore");
    let mut text = fs::read_to_string(&ignore).unwrap_or_default();
    for entry in RAW_BIDSIGNORE {
        if !text.lines().any(|l| l.trim() == *entry) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(entry);
            text.push('\n');
        }
    }
    fs::write(&ignore, text)?;

    let readme = output_dir.join("README");
    let mut text = fs::read_to_string(&readme)
        .unwrap_or_else(|_| "BIDS dataset converted from DICOM by qsmxt dicom-convert (dcm2niix).\n".to_string());
    if has_extra && !text.contains("extra_data/") {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
        text.push_str(README_EXTRA_NOTE);
        text.push('\n');
    }
    fs::write(&readme, text)
}

/// The extension of a dcm2niix output that goes into the dataset, and the name without it.
fn split_extension(file_name: &str) -> Option<(&str, &'static str)> {
    for ext in [".nii.gz", ".nii", ".json", ".bval", ".bvec"] {
        if let Some(stem) = file_name.strip_suffix(ext) {
            return Some((stem, ext));
        }
    }
    None
}

/// What QSMxT knows about a dcm2niix output that dcm2niix's name may not say: the echo time of
/// the files it was run on, or which volume of a 4D image it is.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Known {
    echo_te: Option<f64>,
    volume: Option<usize>,
}

fn is_nifti(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.ends_with(".nii.gz") || s.ends_with(".nii")
}

/// Run dcm2niix once on `files` in `work`; its outputs (`<base>…`), sorted.
fn dcm2niix_once(dcm2niix: &Path, files: &[PathBuf], work: &Path, base: &str, log: &mut Vec<String>) -> Result<Vec<PathBuf>, String> {
    let input = work.join("in");
    let out = work.join("out");
    fs::create_dir_all(&input).map_err(|e| format!("cannot create {}: {e}", input.display()))?;
    fs::create_dir_all(&out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    for (i, src) in files.iter().enumerate() {
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("dcm");
        let dst = input.join(format!("{i:06}.{ext}"));
        if fs::hard_link(src, &dst).is_err() {
            fs::copy(src, &dst).map_err(|e| format!("cannot copy {}: {e}", src.display()))?;
        }
    }
    let output = Command::new(dcm2niix)
        .args(["-o", &out.to_string_lossy(), "-f", base, "-z", "y", "-m", "o"])
        .arg(&input)
        .output()
        .map_err(|e| format!("cannot run dcm2niix: {e}"))?;
    let mut last = String::new();
    for line in String::from_utf8_lossy(&output.stdout).lines().chain(String::from_utf8_lossy(&output.stderr).lines()) {
        if !line.trim().is_empty() {
            log.push(format!("  dcm2niix: {line}"));
            last = line.trim().to_string();
        }
    }
    if !output.status.success() {
        return Err(format!("exit code {:?}{}", output.status.code(), if last.is_empty() { String::new() } else { format!(": {last}") }));
    }
    let mut outputs: Vec<PathBuf> = fs::read_dir(&out)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(base)))
        .collect();
    outputs.sort();
    Ok(outputs)
}

/// Run dcm2niix over groups of files, one working folder each; every output carries its group's
/// `Known`. `None` when some group still gives a 4D image.
fn dcm2niix_groups(
    dcm2niix: &Path, groups: Vec<(Known, Vec<PathBuf>)>, work: &Path, base: &str, log: &mut Vec<String>,
) -> Result<Option<Vec<(PathBuf, Known)>>, String> {
    let mut all = Vec::new();
    for (k, (known, files)) in groups.into_iter().enumerate() {
        let outputs = dcm2niix_once(dcm2niix, &files, &work.join(format!("g{k}")), base, log)?;
        if outputs.iter().any(|p| is_nifti(p) && nifti_4d_size(p).is_some()) {
            return Ok(None);
        }
        all.extend(outputs.into_iter().map(|p| (p, known)));
    }
    Ok(Some(all))
}

/// Convert a series' files with dcm2niix in a fresh working folder.
///
/// When dcm2niix merges images into one 4D file (echoes it cannot tell apart from their tags),
/// QSMxT separates them itself: it runs dcm2niix once per echo time, and when every file has the
/// same echo time, once per volume (the n-th file at each slice position is volume n). Every echo
/// or volume then becomes its own file. If that does not separate them either, the 4D image is
/// kept whole. An unsuccessful dcm2niix exit is an error: nothing of the series is used.
fn run_dcm2niix(
    dcm2niix: &Path, files: &[PathBuf], info: &HashMap<PathBuf, (Option<f64>, String)>, work: &Path, base: &str, log: &mut Vec<String>,
) -> Result<Vec<(PathBuf, Known)>, String> {
    let first = dcm2niix_once(dcm2niix, files, &work.join("all"), base, log)?;
    let Some(n) = first.iter().filter(|p| is_nifti(p)).find_map(|p| nifti_4d_size(p)) else {
        return Ok(first.into_iter().map(|p| (p, Known::default())).collect());
    };

    // By echo time.
    let mut by_te: Vec<(Option<f64>, Vec<PathBuf>)> = Vec::new();
    for f in files {
        let te = info.get(f).and_then(|i| i.0);
        match by_te.iter_mut().find(|(t, _)| match (t, te) { (Some(a), Some(b)) => (a - b).abs() < 0.01, (None, None) => true, _ => false }) {
            Some((_, v)) => v.push(f.clone()),
            None => by_te.push((te, vec![f.clone()])),
        }
    }
    if by_te.len() > 1 {
        log.push(format!("  dcm2niix merged {n} volumes into one 4D image; converting each of the {} echo times separately", by_te.len()));
        let groups = by_te.into_iter().map(|(te, v)| (Known { echo_te: te, volume: None }, v)).collect();
        if let Some(out) = dcm2niix_groups(dcm2niix, groups, &work.join("by-te"), base, log)? {
            return Ok(out);
        }
    } else {
        // By volume: the n-th file at each slice position (files are in instance order).
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut volumes: Vec<Vec<PathBuf>> = Vec::new();
        for f in files {
            let pos = info.get(f).map(|i| i.1.clone()).unwrap_or_default();
            let k = seen.entry(pos).or_insert(0);
            if volumes.len() <= *k {
                volumes.push(Vec::new());
            }
            volumes[*k].push(f.clone());
            *k += 1;
        }
        if volumes.len() > 1 && volumes.iter().all(|v| v.len() == volumes[0].len()) {
            log.push(format!(
                "  dcm2niix merged {n} volumes into one 4D image, and their echo tags do not tell them apart; \
                 converting each of the {} volumes separately (rec-…volN)", volumes.len()
            ));
            let groups = volumes.into_iter().enumerate().map(|(k, v)| (Known { echo_te: None, volume: Some(k + 1) }, v)).collect();
            if let Some(out) = dcm2niix_groups(dcm2niix, groups, &work.join("by-volume"), base, log)? {
                return Ok(out);
            }
        }
    }
    log.push(format!("  Warning: dcm2niix merged {n} volumes into one 4D image that QSMxT could not separate; keeping it whole"));
    Ok(first.into_iter().map(|p| (p, Known::default())).collect())
}

/// Phase units for a phase image's sidecar, which BIDS requires: "rad" when dcm2niix wrote it
/// already scaled to radians (non-integer values within ±π after the NIfTI scaling, as Philips
/// phase is), "arbitrary" for integer-coded phase (Siemens, GE).
fn phase_units(nifti: &Path) -> &'static str {
    let Ok(data) = qsm_core::io::read_nifti_file(nifti) else { return "arbitrary" };
    let limit = std::f64::consts::PI + 0.05;
    let mut fractional = false;
    for &v in data.data.iter().filter(|v| v.is_finite()) {
        if v.abs() > limit {
            return "arbitrary";
        }
        fractional |= v.fract().abs() > 1e-6;
    }
    if fractional { "rad" } else { "arbitrary" }
}

/// One converted file waiting to be moved into the dataset.
struct Staged {
    from: PathBuf,
    /// Path relative to the dataset root.
    rel: String,
    owner: String,
}

/// Fix up a sidecar dcm2niix wrote: EchoTime in seconds, and the `Units` BIDS requires for a
/// phase image (see [`phase_units`]). Run after its image is in place.
fn fix_sidecar(path: &Path) {
    let Ok(content) = fs::read_to_string(path) else { return };
    let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content) else { return };
    let mut changed = false;
    if let Some(et) = json.get("EchoTime").and_then(|v| v.as_f64()) {
        if et > 1.0 {
            json["EchoTime"] = serde_json::json!(et / 1000.0);
            changed = true;
        }
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.contains("_part-phase_") && json.get("Units").is_none() {
        let stem = path.with_extension("");
        let image = [stem.with_extension("nii.gz"), stem.with_extension("nii")].into_iter().find(|p| p.exists());
        json["Units"] = serde_json::json!(image.map(|p| phase_units(&p)).unwrap_or("arbitrary"));
        changed = true;
    }
    if changed {
        if let Ok(out) = serde_json::to_string_pretty(&json) {
            let _ = fs::write(path, out);
        }
    }
}

/// Convert a DICOM session to BIDS with dcm2niix, streaming log and error messages through `tx`,
/// then `Done`.
///
/// Every series of a session is converted into a working folder first, and every name is
/// decided before anything moves into the dataset: a name two series would share is an error
/// naming both, and neither is written there. A series dcm2niix fails on leaves nothing behind.
pub fn convert_session_streaming(
    session: &DicomSession,
    output_dir: &Path,
    tx: &mpsc::Sender<ConvertMessage>,
) {
    let send = |m: ConvertMessage| {
        let _ = tx.send(m);
    };
    let dcm2niix = match find_dcm2niix() {
        Some(p) => p,
        None => {
            send(ConvertMessage::Error(
                "dcm2niix not found: no bundled copy in ~/.qsmxt/bin and not on PATH. \
                 Reinstall qsmxt or install dcm2niix manually."
                    .to_string(),
            ));
            send(ConvertMessage::Done { bids_dir: output_dir.to_path_buf() });
            return;
        }
    };
    if let Err(e) = fs::create_dir_all(output_dir) {
        send(ConvertMessage::Error(format!("Failed to create output directory: {}", e)));
        send(ConvertMessage::Done { bids_dir: output_dir.to_path_buf() });
        return;
    }
    send(ConvertMessage::Log(format!("Output BIDS directory: {}", output_dir.display())));
    send(ConvertMessage::Log(format!("Using dcm2niix: {}", dcm2niix.display())));

    let plans = plan_session(session);
    let has_extra = plans.iter().any(|p| matches!(p.target, Target::Extra { .. }));
    if let Err(e) = write_dataset_files(output_dir, has_extra) {
        send(ConvertMessage::Error(format!("Failed to write the dataset files: {e}")));
    }
    for w in &session.report.warnings {
        send(ConvertMessage::Log(format!("Warning: {w}")));
    }
    for n in &session.report.notes {
        send(ConvertMessage::Log(format!("Note: {n}")));
    }
    for s in &session.report.skipped {
        send(ConvertMessage::Log(format!("Skipped (not an image): {s}")));
    }

    let work_root = output_dir.join(".qsmxt_dcm2niix");
    let _ = fs::remove_dir_all(&work_root);
    let mut counter = 0usize;

    for (si, subject) in session.subjects.iter().enumerate() {
        for (sti, _study) in subject.studies.iter().enumerate() {
            let mut staged: Vec<Staged> = Vec::new();
            let mut failed: Vec<String> = Vec::new();
            for plan in plans.iter().filter(|p| p.r.sub == si && p.r.study == sti) {
                let series = session.series_ref(&plan.r);
                let owner = series.display_name();
                let info: HashMap<PathBuf, (Option<f64>, String)> = series
                    .files
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (f.clone(), (series.file_echo_times.get(i).copied().flatten(), series.file_positions.get(i).cloned().unwrap_or_default())))
                    .collect();
                let mut log = Vec::new();
                match &plan.target {
                    Target::Skip => {
                        let why = series.note.as_ref().map(|n| format!(" ({n})")).unwrap_or_default();
                        send(ConvertMessage::Log(format!("Skipping {owner}{why}")));
                    }
                    Target::Extra { dir, prefix } => {
                        counter += 1;
                        send(ConvertMessage::Log(format!("Keeping {owner} in extra_data ({} files)", series.num_files)));
                        match run_dcm2niix(&dcm2niix, &series.files, &info, &work_root.join(counter.to_string()), prefix, &mut log) {
                            Ok(outputs) => {
                                for (out, known) in outputs {
                                    let Some(name) = out.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
                                    if split_extension(&name).is_none() {
                                        continue;
                                    }
                                    // Volumes QSMxT separated get their number in the name.
                                    let name = match known.volume.or(known.echo_te.map(|_| 0)) {
                                        Some(_) => {
                                            let tag = match (known.volume, known.echo_te) {
                                                (Some(v), _) => format!("_vol{v}"),
                                                (None, Some(te)) => format!("_te{}", clean_bids_label(&format!("{te}"))),
                                                _ => String::new(),
                                            };
                                            name.replacen(prefix.as_str(), &format!("{prefix}{tag}"), 1)
                                        }
                                        None => name,
                                    };
                                    staged.push(Staged { from: out, rel: format!("{}/{}", dir.join("/"), name), owner: owner.clone() });
                                }
                            }
                            Err(e) => log.push(format!("  Warning: dcm2niix could not convert {owner} ({e}); it is left out")),
                        }
                    }
                    Target::Bids(stem) => {
                        send(ConvertMessage::Log(format!("Converting: {owner} ({} files) -> {}", series.num_files, stem.preview())));
                        if let Some(note) = is_ge_manufacturer(&series.manufacturer).then(|| ge_note(series.series_type)).flatten() {
                            log.push(note.to_string());
                        }
                        let groups: Vec<(Option<u32>, &[PathBuf])> = if series.coil_type == CoilType::Uncombined && !series.coil_groups.is_empty() {
                            series.coil_groups.iter().map(|g| (g.coil_num, g.files.as_slice())).collect()
                        } else {
                            vec![(None, series.files.as_slice())]
                        };
                        let base = "qsmxt";
                        for (coil, files) in groups {
                            counter += 1;
                            match run_dcm2niix(&dcm2niix, files, &info, &work_root.join(counter.to_string()), base, &mut log) {
                                Ok(outputs) => {
                                    let images = outputs.iter().filter(|(p, _)| is_nifti(p)).count();
                                    for (out, known) in outputs {
                                        let Some(name) = out.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
                                        let Some((stem_part, ext)) = split_extension(&name) else { continue };
                                        let mut sfx = parse_dcm2niix_suffix(stem_part.strip_prefix(base).unwrap_or(""));
                                        if let Some(te) = known.echo_te {
                                            sfx.echo_te = Some(te);
                                            sfx.echo_num = None;
                                        }
                                        if let Some(v) = known.volume {
                                            // The echo tags are what could not tell the volumes apart.
                                            sfx.echo_num = None;
                                            sfx.extra.push(format!("vol{v}"));
                                        }
                                        // A lone map computed at a late echo: nothing to tell apart.
                                        if images == 1 && !stem.allows_echo {
                                            sfx.echo_num = None;
                                            sfx.echo_te = None;
                                        }
                                        let bids_name = stem.file_name(&sfx, coil, ext);
                                        staged.push(Staged { from: out.clone(), rel: format!("{}/{}", stem.dir().join("/"), bids_name), owner: owner.clone() });
                                    }
                                }
                                Err(e) => {
                                    let coil = coil.map(|c| format!(" (coil {c})")).unwrap_or_default();
                                    send(ConvertMessage::Error(format!("dcm2niix failed for {owner}{coil}: {e}; nothing of this series was converted")));
                                    failed.push(owner.clone());
                                }
                            }
                        }
                    }
                }
                for line in log {
                    send(ConvertMessage::Log(line));
                }
            }

            staged.retain(|s| !failed.contains(&s.owner));
            let entries: Vec<(String, String)> = staged.iter().map(|s| (s.rel.clone(), s.owner.clone())).collect();
            let clashes = find_collisions(&entries);
            for (name, a, b) in &clashes {
                let msg = if a == b {
                    format!("{name}: two images of {a} would both be written there; neither was")
                } else {
                    format!("{name}: {a} and {b} would both be written there; neither was")
                };
                send(ConvertMessage::Error(msg));
            }
            let clashing: Vec<&String> = clashes.iter().map(|c| &c.0).collect();
            for s in &staged {
                if clashing.contains(&&s.rel) {
                    continue;
                }
                let dest = output_dir.join(&s.rel);
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::rename(&s.from, &dest).is_err() {
                    if let Err(e) = fs::copy(&s.from, &dest) {
                        send(ConvertMessage::Error(format!("Failed to write {}: {e}", dest.display())));
                        continue;
                    }
                }
                if !s.rel.ends_with(".json") {
                    send(ConvertMessage::Log(format!("  -> {}", s.rel)));
                }
            }
            // Sidecars last: a phase sidecar's Units depends on its image.
            for s in staged.iter().filter(|s| s.rel.ends_with(".json") && !clashing.contains(&&s.rel)) {
                fix_sidecar(&output_dir.join(&s.rel));
            }
        }
    }

    let _ = fs::remove_dir_all(&work_root);
    send(ConvertMessage::Done { bids_dir: output_dir.to_path_buf() });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-wide environment variables.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_dcm2niix_exe_name() {
        let name = dcm2niix_exe_name();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "dcm2niix.exe");
        } else {
            assert_eq!(name, "dcm2niix");
        }
    }

    #[test]
    fn test_qsmxt_bin_dir_ends_with_qsmxt_bin() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev_home = std::env::var_os("HOME");
        let prev_userprofile = std::env::var_os("USERPROFILE");
        std::env::set_var("HOME", "/tmp/fake-home");
        std::env::remove_var("USERPROFILE");

        let dir = qsmxt_bin_dir().expect("bin dir");
        assert_eq!(dir, PathBuf::from("/tmp/fake-home/.qsmxt/bin"));

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        if let Some(v) = prev_userprofile {
            std::env::set_var("USERPROFILE", v);
        }
    }

    #[test]
    fn test_find_dcm2niix_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Point the override at a file that exists (this test binary itself).
        let real_file = std::env::current_exe().expect("current exe");
        let prev = std::env::var_os("QSMXT_DCM2NIIX");
        std::env::set_var("QSMXT_DCM2NIIX", &real_file);

        assert_eq!(find_dcm2niix(), Some(real_file));

        match prev {
            Some(v) => std::env::set_var("QSMXT_DCM2NIIX", v),
            None => std::env::remove_var("QSMXT_DCM2NIIX"),
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_find_dcm2niix_prefers_bundled_in_home() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev_home = std::env::var_os("HOME");
        let prev_userprofile = std::env::var_os("USERPROFILE");
        let prev_override = std::env::var_os("QSMXT_DCM2NIIX");
        std::env::remove_var("QSMXT_DCM2NIIX");

        // Fake HOME with a bundled dcm2niix at ~/.qsmxt/bin/dcm2niix.
        let home = tempfile::tempdir().expect("temp home");
        let bin = home.path().join(".qsmxt").join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        let bundled = bin.join("dcm2niix");
        std::fs::write(&bundled, b"#!/bin/sh\n").expect("write stub");
        std::env::set_var("HOME", home.path());
        std::env::remove_var("USERPROFILE");

        // Resolver must return the bundled copy (step 2), ahead of any PATH copy.
        assert_eq!(find_dcm2niix(), Some(bundled));

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        if let Some(v) = prev_userprofile {
            std::env::set_var("USERPROFILE", v);
        }
        if let Some(v) = prev_override {
            std::env::set_var("QSMXT_DCM2NIIX", v);
        }
    }

    #[test]
    fn test_find_dcm2niix_env_override_nonexistent_is_ignored() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("QSMXT_DCM2NIIX");
        std::env::set_var("QSMXT_DCM2NIIX", "/nonexistent/path/to/dcm2niix");

        // A bogus override must not be returned; resolution falls through.
        assert_ne!(
            find_dcm2niix(),
            Some(PathBuf::from("/nonexistent/path/to/dcm2niix"))
        );

        match prev {
            Some(v) => std::env::set_var("QSMXT_DCM2NIIX", v),
            None => std::env::remove_var("QSMXT_DCM2NIIX"),
        }
    }


























    // ─── nii_to_json_path tests ───

    #[test]
    fn test_nii_to_json_path_nii_gz() {
        let p = nii_to_json_path(Path::new("/data/sub-01_T1w.nii.gz"));
        assert_eq!(p, PathBuf::from("/data/sub-01_T1w.json"));
    }

    #[test]
    fn test_nii_to_json_path_nii() {
        let p = nii_to_json_path(Path::new("/data/sub-01_T1w.nii"));
        assert_eq!(p, PathBuf::from("/data/sub-01_T1w.json"));
    }

    #[test]
    fn test_nii_to_json_path_other_extension() {
        let p = nii_to_json_path(Path::new("/data/sub-01_T1w.bval"));
        assert_eq!(p, PathBuf::from("/data/sub-01_T1w.bval.json"));
    }

    // ─── nifti_4d_size tests ───

    #[test]
    fn test_nifti_4d_size_3d_returns_none() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let nii_path = dir.path().join("mag.nii");
        crate::testutils::write_magnitude(&nii_path);
        assert_eq!(nifti_4d_size(&nii_path), None);
    }

    // ─── is_ge_manufacturer tests ───

    #[test]
    fn test_is_ge_manufacturer_lowercase() {
        assert!(is_ge_manufacturer("ge medical systems"));
    }

    #[test]
    fn test_is_ge_manufacturer_mixed_case() {
        assert!(is_ge_manufacturer("General Electric"));
    }

    #[test]
    fn test_is_ge_manufacturer_empty() {
        assert!(!is_ge_manufacturer(""));
    }

    #[test]
    fn test_is_ge_manufacturer_other_vendors() {
        assert!(!is_ge_manufacturer("SIEMENS HEALTHINEERS"));
        assert!(!is_ge_manufacturer("Philips Medical"));
        assert!(!is_ge_manufacturer("Philips"));
        assert!(!is_ge_manufacturer("Canon"));
        assert!(!is_ge_manufacturer("Canon Medical Systems"));
    }

    /// The old substring test took any manufacturer containing the letters "GE" for GE.
    #[test]
    fn test_is_ge_manufacturer_rejects_strings_merely_containing_ge() {
        assert!(!is_ge_manufacturer("ACME IMAGE MANAGEMENT"));
        assert!(!is_ge_manufacturer("GEMINI SCANNERS"));
        assert!(!is_ge_manufacturer("HITACHI MEDICAL SYSTEMS GERMANY"));
    }

    #[test]
    fn test_is_ge_manufacturer_ge_forms() {
        assert!(is_ge_manufacturer("GE HEALTHCARE"));
        assert!(is_ge_manufacturer(" GE MEDICAL SYSTEMS "));
        assert!(is_ge_manufacturer("GE"));
    }

    /// No log line for something that does not happen: GE phase is not inverted, so nothing
    /// says it is; what is said is what QSMxT does not do (#274).
    #[test]
    fn ge_notes_say_what_is_not_done() {
        let phase = ge_note(SeriesType::Phase).unwrap();
        assert!(phase.contains("does not yet correct"), "{phase}");
        assert!(!phase.contains("will be applied"), "{phase}");
        assert!(ge_note(SeriesType::Real).is_none(), "real/imaginary is converted by `qsmxt run`");
        assert!(ge_note(SeriesType::Imaginary).is_none());
        assert!(ge_note(SeriesType::Magnitude).is_none());
    }



}
