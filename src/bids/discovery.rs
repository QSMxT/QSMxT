use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use glob::glob;
use log::{debug, warn};

use crate::bids::entities::{self, AcquisitionKey, BidsEntities, Part};
use crate::bids::sidecar;
use crate::error::QsmxtError;

/// Files for a single echo in a BIDS acquisition.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EchoFiles {
    pub echo_number: u32,
    pub phase_nifti: PathBuf,
    pub phase_json: PathBuf,
    pub magnitude_nifti: Option<PathBuf>,
    pub magnitude_json: Option<PathBuf>,
}

/// Per-echo files of one receive-coil channel in an uncombined (per-coil) acquisition.
#[derive(Debug, Clone)]
pub struct CoilFiles {
    pub coil_number: u32,
    pub echoes: Vec<EchoFiles>,
}

/// A complete QSM acquisition run with all echoes.
#[derive(Debug, Clone)]
pub struct QsmRun {
    pub key: AcquisitionKey,
    /// Per-echo files. For an uncombined multi-coil run these are the first coil's files:
    /// they define the grid and sidecar metadata, and the pipeline replaces them with the
    /// MCPC-3D-S combination of `coils` in its first stage.
    pub echoes: Vec<EchoFiles>,
    /// Uncombined receive-coil channels (`coil-NN` entity), when the acquisition was exported
    /// per coil. `None` for scanner-combined data.
    pub coils: Option<Vec<CoilFiles>>,
    pub magnetic_field_strength: f64,
    pub echo_times: Vec<f64>,
    /// B0 direction in voxel coordinates, when a sidecar declares `B0_dir`. `None` means the
    /// pipeline derives it from the NIfTI affine (or gets `(0,0,1)` after axial resampling).
    pub b0_dir: Option<(f64, f64, f64)>,
    /// Volume dimensions (nx, ny, nz) from the first phase NIfTI header.
    pub dims: (usize, usize, usize),
    /// Whether magnitude files are available for this run.
    pub has_magnitude: bool,
    /// Matching multi-echo spin-echo (`MESE`) acquisition, if present in the dataset
    /// (same subject/session). Used to compute R2 (EPG) → R2' for chi-separation.
    pub mese: Option<MeseRun>,
}

/// A multi-echo spin-echo (`MESE`) acquisition for T2/R2 mapping (BIDS suffix `MESE`).
#[derive(Debug, Clone)]
pub struct MeseRun {
    pub key: AcquisitionKey,
    /// Per-echo magnitude NIfTI paths, ordered by echo number.
    pub magnitude_niftis: Vec<PathBuf>,
    /// Echo times in seconds, ordered by echo number.
    pub echo_times: Vec<f64>,
}

/// Filters for BIDS discovery.
#[derive(Debug, Clone, Default)]
pub struct DiscoveryFilter {
    /// Glob patterns — include runs whose key matches at least one pattern
    pub include: Option<Vec<String>>,
    /// Glob patterns — exclude runs whose key matches any pattern
    pub exclude: Option<Vec<String>>,
    pub num_echoes: Option<usize>,
}

/// Convert a glob pattern to a case-insensitive regex string.
/// Supports `*` (any chars), `?` (single char). Anchored to full string.
pub fn glob_to_regex(pattern: &str) -> String {
    let mut re = String::from("(?i)^");
    for ch in pattern.chars() {
        match ch {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '.' => re.push_str("\\."),
            '(' | ')' | '[' | ']' | '{' | '}' | '+' | '^' | '$' | '|' | '\\' => {
                re.push('\\');
                re.push(ch);
            }
            _ => re.push(ch),
        }
    }
    re.push('$');
    re
}

/// Check if a key matches a glob pattern.
pub fn matches_glob(key: &str, pattern: &str) -> bool {
    let re_str = glob_to_regex(pattern);
    regex::Regex::new(&re_str).map(|re| re.is_match(key)).unwrap_or(false)
}

/// Check if a key passes include/exclude filters.
pub fn passes_include_exclude(key: &str, include: &Option<Vec<String>>, exclude: &Option<Vec<String>>) -> bool {
    // Include: must match at least one pattern (if specified)
    if let Some(ref patterns) = include {
        if !patterns.iter().any(|p| matches_glob(key, p)) {
            return false;
        }
    }
    // Exclude: must not match any pattern
    if let Some(ref patterns) = exclude {
        if patterns.iter().any(|p| matches_glob(key, p)) {
            return false;
        }
    }
    true
}

/// Discover all QSM runs in a BIDS directory.
pub fn discover_runs(bids_dir: &Path, filter: &DiscoveryFilter) -> crate::Result<Vec<QsmRun>> {
    let patterns = [
        format!("{}/sub-*/anat/*_part-phase_*.nii*", bids_dir.display()),
        format!(
            "{}/sub-*/ses-*/anat/*_part-phase_*.nii*",
            bids_dir.display()
        ),
    ];

    // Collect all phase files
    let mut phase_files: Vec<(PathBuf, BidsEntities)> = Vec::new();

    for pattern in &patterns {
        for entry in glob(pattern).map_err(|e| QsmxtError::BidsDiscovery(e.to_string()))? {
            let path = entry.map_err(|e| QsmxtError::BidsDiscovery(e.to_string()))?;
            let filename = path
                .file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| QsmxtError::BidsDiscovery("Invalid filename".to_string()))?;

            if let Some(ent) = entities::parse_entities(filename) {
                if ent.part != Some(Part::Phase) {
                    continue;
                }

                debug!("Found phase file: {}", path.display());
                phase_files.push((path, ent));
            }
        }
    }

    // Group by AcquisitionKey
    let mut groups: HashMap<AcquisitionKey, Vec<(PathBuf, BidsEntities)>> = HashMap::new();
    for (path, ent) in phase_files {
        let key = ent.acquisition_key();
        groups.entry(key).or_default().push((path, ent));
    }

    // Apply include/exclude filters on grouped run keys
    if filter.include.is_some() || filter.exclude.is_some() {
        groups.retain(|key, _| {
            passes_include_exclude(&key.to_string(), &filter.include, &filter.exclude)
        });
    }

    // Build QsmRun for each group
    let mut runs: Vec<QsmRun> = Vec::new();

    for (key, files) in groups {
        // Split per-coil (uncombined) channels from combined files.
        let mut by_coil: BTreeMap<Option<u32>, Vec<(PathBuf, BidsEntities)>> = BTreeMap::new();
        for f in files {
            by_coil.entry(f.1.coil).or_default().push(f);
        }
        let combined = by_coil.remove(&None);

        let (echoes, echo_times, b0_tesla, b0_dir, coils) = if by_coil.len() >= 2 {
            if combined.is_some() {
                warn!(
                    "{}: both per-coil and combined files present in one run; using the {} per-coil channels",
                    key, by_coil.len()
                );
            }
            let mut coils: Vec<CoilFiles> = Vec::with_capacity(by_coil.len());
            let mut tes: Option<Vec<f64>> = None;
            let mut b0 = 0.0;
            let mut dir: Option<(f64, f64, f64)> = None;
            for (coil_number, cfiles) in by_coil {
                let (e, t, b, d) = build_echoes(cfiles, filter.num_echoes)?;
                if e.is_empty() {
                    continue;
                }
                if e.iter().any(|x| x.magnitude_nifti.is_none()) {
                    return Err(QsmxtError::BidsDiscovery(format!(
                        "{}: coil {} is missing a magnitude file — MCPC-3D-S coil combination needs \
                         magnitude and phase for every coil and echo",
                        key, coil_number.unwrap_or(0)
                    )));
                }
                match &tes {
                    None => { tes = Some(t); b0 = b; dir = d; }
                    Some(t0) => {
                        if t0.len() != t.len() || t0.iter().zip(&t).any(|(a, b)| (a - b).abs() > 1e-9) {
                            return Err(QsmxtError::BidsDiscovery(format!(
                                "{}: coil {} has echo times {:?} but coil {} has {:?} — all coils of a run must share the same echoes",
                                key, coil_number.unwrap_or(0), t, coils[0].coil_number, t0
                            )));
                        }
                    }
                }
                coils.push(CoilFiles { coil_number: coil_number.unwrap_or(0), echoes: e });
            }
            if coils.is_empty() {
                continue;
            }
            (coils[0].echoes.clone(), tes.unwrap_or_default(), b0, dir, Some(coils))
        } else {
            // Combined data, or a single coil (treated as combined).
            let files = match combined {
                Some(c) => c,
                None => match by_coil.into_values().next() {
                    Some(c) => c,
                    None => continue,
                },
            };
            let (e, t, b, d) = build_echoes(files, filter.num_echoes)?;
            (e, t, b, d, None)
        };

        if echoes.is_empty() {
            continue;
        }

        // Read volume dimensions from the first phase NIfTI header (fast, header-only)
        let dims = qsm_core::io::read_nifti_dims(&echoes[0].phase_nifti)
            .map_err(QsmxtError::NiftiIo)?;
        let has_magnitude = echoes[0].magnitude_nifti.is_some();

        runs.push(QsmRun {
            key,
            echoes,
            coils,
            magnetic_field_strength: b0_tesla,
            echo_times,
            b0_dir,
            dims,
            has_magnitude,
            mese: None,
        });
    }

    // When an acquisition was exported both scanner-combined and per coil (Siemens SWI with
    // "save uncombined"), prefer the per-coil channels: the scanner's phase combination is not
    // suitable for QSM, and MCPC-3D-S on the raw coils is. `--exclude "*rec-uncombined*"` keeps
    // the scanner-combined run instead.
    let coil_keys: Vec<AcquisitionKey> = runs.iter()
        .filter(|r| r.coils.is_some())
        .map(|r| r.key.clone())
        .collect();
    runs.retain(|r| {
        if r.coils.is_some() || r.key.reconstruction.is_some() {
            return true;
        }
        let shadowed = coil_keys.iter().any(|k| {
            k.reconstruction.as_deref() == Some("uncombined")
                && k.subject == r.key.subject && k.session == r.key.session
                && k.acquisition == r.key.acquisition && k.run == r.key.run
                && k.suffix == r.key.suffix
        });
        if shadowed {
            log::info!(
                "{}: skipping the scanner-combined run in favour of its uncombined coils \
                 (MCPC-3D-S); pass --exclude \"*rec-uncombined*\" to process the combined data instead",
                r.key
            );
        }
        !shadowed
    });

    // Attach matching MESE (multi-echo spin-echo) acquisitions for R2/R2' computation.
    attach_mese(&mut runs, bids_dir);

    // Sort by key for deterministic ordering
    runs.sort_by_key(|a| a.key.to_string());

    Ok(runs)
}

/// Echo files, echo times, field strength and B0 direction for one set of phase files
/// (one coil, or the combined data), sorted by echo number.
type EchoSet = (Vec<EchoFiles>, Vec<f64>, f64, Option<(f64, f64, f64)>);

fn build_echoes(mut files: Vec<(PathBuf, BidsEntities)>, num_echoes: Option<usize>) -> crate::Result<EchoSet> {
    // Sort by echo number
    files.sort_by_key(|(_, ent)| ent.echo.unwrap_or(1));

    // Apply echo limit
    if let Some(max_echoes) = num_echoes {
        files.truncate(max_echoes);
    }

    let mut echoes = Vec::new();
    let mut echo_times = Vec::new();
    let mut b0_tesla = 0.0f64;
    let mut b0_dir: Option<(f64, f64, f64)> = None;

    for (phase_path, ent) in &files {
        let echo_num = ent.echo.unwrap_or(1);

        // Find corresponding files
        let json_path = entities::sidecar_path(phase_path).ok_or_else(|| {
            QsmxtError::BidsDiscovery(format!(
                "Cannot determine sidecar path for non-NIfTI file: {}",
                phase_path.display()
            ))
        })?;
        let mag_path = entities::phase_to_magnitude_path(phase_path);

        // Read sidecar
        if !json_path.exists() {
            return Err(QsmxtError::BidsDiscovery(format!(
                "JSON sidecar not found: {}",
                json_path.display()
            )));
        }
        let sc = sidecar::read_sidecar(&json_path)?;
        echo_times.push(sc.echo_time);
        b0_tesla = sc.magnetic_field_strength;

        if let Some(ref dir) = sc.b0_dir {
            if dir.len() == 3 {
                b0_dir = Some((dir[0], dir[1], dir[2]));
            } else {
                warn!(
                    "B0 direction has {} components (expected 3), defaulting to (0,0,1): {}",
                    dir.len(), json_path.display()
                );
            }
        }

        let mag_nifti = if mag_path.exists() {
            Some(mag_path.clone())
        } else {
            warn!(
                "Magnitude file not found (will proceed without): {}",
                mag_path.display()
            );
            None
        };

        let mag_json = mag_nifti.as_ref().and_then(|p| entities::sidecar_path(p));

        echoes.push(EchoFiles {
            echo_number: echo_num,
            phase_nifti: phase_path.clone(),
            phase_json: json_path,
            magnitude_nifti: mag_nifti,
            magnitude_json: mag_json,
        });
    }

    Ok((echoes, echo_times, b0_tesla, b0_dir))
}

/// Discover `MESE` acquisitions and attach each to QSM runs of the same subject/session.
///
/// MESE files are magnitude-only (`sub-XX[_ses-][_acq-][_run-]_echo-N_MESE.nii(.gz)`) so they
/// are not picked up by the phase-based [`discover_runs`] glob. A run keeps `mese = None` when
/// no matching spin-echo acquisition exists.
fn attach_mese(runs: &mut [QsmRun], bids_dir: &Path) {
    let patterns = [
        format!("{}/sub-*/anat/*_MESE.nii*", bids_dir.display()),
        format!("{}/sub-*/ses-*/anat/*_MESE.nii*", bids_dir.display()),
    ];

    // Group MESE magnitude files by acquisition key.
    let mut groups: HashMap<AcquisitionKey, Vec<(PathBuf, BidsEntities)>> = HashMap::new();
    for pattern in &patterns {
        let entries = match glob(pattern) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let Some(filename) = entry.file_name().and_then(|f| f.to_str()) else { continue };
            let Some(ent) = entities::parse_entities(filename) else { continue };
            // MESE magnitude images carry no phase part.
            if ent.part == Some(Part::Phase) {
                continue;
            }
            groups.entry(ent.acquisition_key()).or_default().push((entry.clone(), ent));
        }
    }

    // Build a MeseRun per acquisition key (sorted by echo, echo times from sidecars).
    let mut mese_runs: Vec<MeseRun> = Vec::new();
    for (key, mut files) in groups {
        files.sort_by_key(|(_, ent)| ent.echo.unwrap_or(1));
        let mut magnitude_niftis = Vec::new();
        let mut echo_times = Vec::new();
        for (path, _) in &files {
            let Some(json_path) = entities::sidecar_path(path) else { continue };
            match sidecar::read_sidecar(&json_path) {
                Ok(sc) => {
                    magnitude_niftis.push(path.clone());
                    echo_times.push(sc.echo_time);
                }
                Err(e) => warn!("Skipping MESE echo (bad sidecar {}): {}", json_path.display(), e),
            }
        }
        if magnitude_niftis.len() >= 3 {
            mese_runs.push(MeseRun { key, magnitude_niftis, echo_times });
        } else if !magnitude_niftis.is_empty() {
            warn!("Ignoring MESE acquisition with <3 usable echoes: {}", key);
        }
    }

    // Attach by subject/session (ignoring acq/run differences between GRE and MESE).
    for run in runs.iter_mut() {
        run.mese = mese_runs.iter()
            .find(|m| m.key.subject == run.key.subject && m.key.session == run.key.session)
            .cloned();
        if run.mese.is_some() {
            debug!("Attached MESE acquisition to run {}", run.key);
        }
    }
}

// ─── Lightweight BIDS tree scanner (for TUI filters) ───

/// A run leaf in the BIDS tree (one QSM acquisition).
#[derive(Debug, Clone)]
pub struct BidsRunLeaf {
    /// Display string: the distinguishing part (e.g. "acq-gre_run-1_MEGRE")
    pub display: String,
    /// Full AcquisitionKey as string for pattern matching
    pub key_string: String,
    /// Whether this run is selected for processing
    pub selected: bool,
}

/// A session node containing runs.
#[derive(Debug, Clone)]
pub struct BidsSessionNode {
    pub name: String,
    pub runs: Vec<BidsRunLeaf>,
}

/// A subject node containing optional sessions and/or direct runs.
#[derive(Debug, Clone)]
pub struct BidsSubjectNode {
    pub name: String,
    /// Sessions under this subject (empty if no sessions)
    pub sessions: Vec<BidsSessionNode>,
    /// Runs directly under this subject (no session)
    pub runs: Vec<BidsRunLeaf>,
}

/// Tree structure of a BIDS dataset for the TUI filter view.
#[derive(Debug, Clone)]
pub struct BidsTree {
    pub subjects: Vec<BidsSubjectNode>,
}

impl BidsTree {
    /// Total number of run leaves in the tree.
    pub fn total_runs(&self) -> usize {
        self.subjects.iter().map(|s| {
            s.runs.len() + s.sessions.iter().map(|ses| ses.runs.len()).sum::<usize>()
        }).sum()
    }

    /// Number of selected run leaves.
    pub fn selected_runs(&self) -> usize {
        self.subjects.iter().map(|s| {
            s.runs.iter().filter(|r| r.selected).count()
                + s.sessions.iter().map(|ses| ses.runs.iter().filter(|r| r.selected).count()).sum::<usize>()
        }).sum()
    }

    /// Iterate over all run leaves mutably.
    pub fn for_each_run(&self, mut f: impl FnMut(&BidsRunLeaf)) {
        for sub in &self.subjects {
            for run in &sub.runs {
                f(run);
            }
            for ses in &sub.sessions {
                for run in &ses.runs {
                    f(run);
                }
            }
        }
    }

    pub fn for_each_run_mut(&mut self, mut f: impl FnMut(&mut BidsRunLeaf)) {
        for sub in &mut self.subjects {
            for run in &mut sub.runs {
                f(run);
            }
            for ses in &mut sub.sessions {
                for run in &mut ses.runs {
                    f(run);
                }
            }
        }
    }

    /// Set all runs selected or deselected.
    pub fn set_all(&mut self, selected: bool) {
        self.for_each_run_mut(|r| r.selected = selected);
    }
}

impl BidsSubjectNode {
    /// Total runs under this subject (direct + all sessions).
    pub fn total_runs(&self) -> usize {
        self.runs.len() + self.sessions.iter().map(|s| s.runs.len()).sum::<usize>()
    }

    /// Selected runs under this subject.
    pub fn selected_runs(&self) -> usize {
        self.runs.iter().filter(|r| r.selected).count()
            + self.sessions.iter().map(|s| s.runs.iter().filter(|r| r.selected).count()).sum::<usize>()
    }

    /// Set all runs under this subject.
    pub fn set_all(&mut self, selected: bool) {
        for r in &mut self.runs { r.selected = selected; }
        for ses in &mut self.sessions {
            for r in &mut ses.runs { r.selected = selected; }
        }
    }
}

impl BidsSessionNode {
    /// Set all runs under this session.
    pub fn set_all(&mut self, selected: bool) {
        for r in &mut self.runs { r.selected = selected; }
    }
}

/// Scan a BIDS directory and build a tree of subjects/sessions/runs.
///
/// This is lightweight: only globs filenames and parses entities.
/// No JSON sidecars or NIfTI headers are read.
pub fn scan_bids_tree(bids_dir: &Path) -> crate::Result<BidsTree> {
    let patterns = [
        format!("{}/sub-*/anat/*_part-phase_*.nii*", bids_dir.display()),
        format!("{}/sub-*/ses-*/anat/*_part-phase_*.nii*", bids_dir.display()),
    ];

    // Subjects/sessions that also have a matching MESE acquisition (for the "(+MESE)" annotation).
    let mese_present = mese_subject_sessions(bids_dir);

    // Collect unique AcquisitionKeys grouped by subject and session
    // subject -> (session -> [AcquisitionKey])
    let mut tree_map: BTreeMap<String, BTreeMap<Option<String>, Vec<AcquisitionKey>>> = BTreeMap::new();
    let mut seen_keys = std::collections::HashSet::new();

    for pattern in &patterns {
        for entry in glob(pattern).map_err(|e| QsmxtError::BidsDiscovery(e.to_string()))? {
            let path = entry.map_err(|e| QsmxtError::BidsDiscovery(e.to_string()))?;
            let filename = match path.file_name().and_then(|f| f.to_str()) {
                Some(f) => f,
                None => continue,
            };

            if let Some(ent) = entities::parse_entities(filename) {
                if ent.part != Some(Part::Phase) {
                    continue;
                }

                let key = ent.acquisition_key();
                let key_str = key.to_string();
                if seen_keys.contains(&key_str) {
                    continue;
                }
                seen_keys.insert(key_str);

                tree_map
                    .entry(ent.subject.clone())
                    .or_default()
                    .entry(ent.session.clone())
                    .or_default()
                    .push(key);
            }
        }
    }

    // Build tree from map
    let mut subjects = Vec::new();
    for (subject, session_map) in tree_map {
        let mut direct_runs = Vec::new();
        let mut sessions = Vec::new();

        for (session, keys) in session_map {
            // A MESE for this subject/session is auto-used (R2 → R2') for every run under it.
            let has_mese = mese_present.contains(&(subject.clone(), session.clone()));
            let run_leaves: Vec<BidsRunLeaf> = keys.into_iter().map(|key| {
                let key_string = key.to_string();
                // Build display: everything after sub-XX[_ses-YY]_ (annotated when MESE is present).
                let mut display = build_run_display(&key);
                if has_mese {
                    display.push_str(" (+MESE)");
                }
                BidsRunLeaf { display, key_string, selected: true }
            }).collect();

            match session {
                Some(ses) => sessions.push(BidsSessionNode { name: ses, runs: run_leaves }),
                None => direct_runs = run_leaves,
            }
        }

        sessions.sort_by(|a, b| a.name.cmp(&b.name));
        subjects.push(BidsSubjectNode {
            name: subject,
            sessions,
            runs: direct_runs,
        });
    }

    Ok(BidsTree { subjects })
}

/// The `(subject, session)` pairs that have a `MESE` (multi-echo spin-echo) acquisition, used to
/// annotate the Input tree — a MESE is auto-matched to same-subject/session runs at processing time.
fn mese_subject_sessions(bids_dir: &Path) -> std::collections::HashSet<(String, Option<String>)> {
    let mut set = std::collections::HashSet::new();
    let patterns = [
        format!("{}/sub-*/anat/*_MESE.nii*", bids_dir.display()),
        format!("{}/sub-*/ses-*/anat/*_MESE.nii*", bids_dir.display()),
    ];
    for pattern in &patterns {
        let entries = match glob(pattern) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let Some(filename) = entry.file_name().and_then(|f| f.to_str()) else { continue };
            let Some(ent) = entities::parse_entities(filename) else { continue };
            if ent.part == Some(Part::Phase) {
                continue;
            }
            set.insert((ent.subject.clone(), ent.session.clone()));
        }
    }
    set
}

/// Build the display string for a run (the part after subject/session).
fn build_run_display(key: &AcquisitionKey) -> String {
    let mut parts = Vec::new();
    if let Some(ref acq) = key.acquisition {
        parts.push(format!("acq-{}", acq));
    }
    if let Some(ref rec) = key.reconstruction {
        parts.push(format!("rec-{}", rec));
    }
    if let Some(ref inv) = key.inversion {
        parts.push(format!("inv-{}", inv));
    }
    if let Some(ref run) = key.run {
        parts.push(format!("run-{}", run));
    }
    parts.push(key.suffix.clone());
    parts.join("_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutils;

    #[test]
    fn test_discover_single_echo() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_single_echo_bids(dir.path());
        let runs = discover_runs(dir.path(), &DiscoveryFilter::default()).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].key.subject, "1");
        assert_eq!(runs[0].echoes.len(), 1);
        assert!(runs[0].has_magnitude);
        assert!((runs[0].echo_times[0] - 0.02).abs() < 1e-6);
    }

    #[test]
    fn test_discover_multi_coil_prefers_uncombined() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_coil_bids(dir.path(), 3);
        let runs = discover_runs(dir.path(), &DiscoveryFilter::default()).unwrap();
        assert_eq!(runs.len(), 1, "scanner-combined duplicate must be dropped: {:?}", runs.iter().map(|r| r.key.to_string()).collect::<Vec<_>>());
        let run = &runs[0];
        assert_eq!(run.key.reconstruction.as_deref(), Some("uncombined"));
        let coils = run.coils.as_ref().expect("coils");
        assert_eq!(coils.len(), 3);
        assert_eq!(coils.iter().map(|c| c.coil_number).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(coils.iter().all(|c| c.echoes.len() == 2));
        assert_eq!(run.echoes.len(), 2, "placeholder echoes come from the first coil");
        assert_eq!(run.echo_times.len(), 2);
        assert!(run.has_magnitude);
    }

    #[test]
    fn test_discover_multi_coil_exclude_keeps_combined() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_coil_bids(dir.path(), 2);
        let filter = DiscoveryFilter {
            exclude: Some(vec!["*rec-uncombined*".to_string()]),
            ..Default::default()
        };
        let runs = discover_runs(dir.path(), &filter).unwrap();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].coils.is_none());
        assert_eq!(runs[0].key.reconstruction, None);
        assert_eq!(runs[0].echoes.len(), 2);
    }

    #[test]
    fn test_discover_multi_echo() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_echo_bids(dir.path());
        let runs = discover_runs(dir.path(), &DiscoveryFilter::default()).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].echoes.len(), 3);
        assert_eq!(runs[0].echo_times.len(), 3);
    }

    #[test]
    fn test_discover_with_include_filter() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_single_echo_bids(dir.path());
        let filter = DiscoveryFilter {
            include: Some(vec!["sub-99*".to_string()]),
            ..Default::default()
        };
        let runs = discover_runs(dir.path(), &filter).unwrap();
        assert_eq!(runs.len(), 0, "Filter should exclude sub-1");
    }

    #[test]
    fn test_discover_with_exclude_filter() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_single_echo_bids(dir.path());
        let filter = DiscoveryFilter {
            exclude: Some(vec!["*sub-1*".to_string()]),
            ..Default::default()
        };
        let runs = discover_runs(dir.path(), &filter).unwrap();
        assert_eq!(runs.len(), 0, "Exclude should remove sub-1");
    }

    #[test]
    fn test_discover_multi_session() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_session_bids(dir.path());
        let runs = discover_runs(dir.path(), &DiscoveryFilter::default()).unwrap();
        assert_eq!(runs.len(), 2);
        let sessions: Vec<_> = runs.iter().map(|r| r.key.session.as_deref().unwrap_or("")).collect();
        assert!(sessions.contains(&"pre"));
        assert!(sessions.contains(&"post"));
    }

    #[test]
    fn test_discover_with_session_include() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_session_bids(dir.path());
        let filter = DiscoveryFilter {
            include: Some(vec!["*ses-pre*".to_string()]),
            ..Default::default()
        };
        let runs = discover_runs(dir.path(), &filter).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].key.session.as_deref(), Some("pre"));
    }

    #[test]
    fn test_scan_bids_tree() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_session_bids(dir.path());
        let tree = scan_bids_tree(dir.path()).unwrap();
        assert_eq!(tree.subjects.len(), 1);
        assert_eq!(tree.subjects[0].name, "1");
        assert_eq!(tree.subjects[0].sessions.len(), 2);
        assert_eq!(tree.total_runs(), 2);
        assert_eq!(tree.selected_runs(), 2);
    }

    #[test]
    fn test_scan_bids_tree_annotates_mese() {
        let dir = tempfile::tempdir().unwrap();
        let anat = dir.path().join("sub-1/anat");
        std::fs::create_dir_all(&anat).unwrap();
        // A MEGRE (phase) run and a same-subject MESE magnitude acquisition.
        for echo in 1..=3 {
            testutils::write_phase(&anat.join(format!("sub-1_echo-{}_part-phase_MEGRE.nii", echo)));
            testutils::write_sidecar(&anat.join(format!("sub-1_echo-{}_part-phase_MEGRE.json", echo)), 0.004 * echo as f64, 3.0);
        }
        for echo in 1..=4 {
            testutils::write_magnitude(&anat.join(format!("sub-1_echo-{}_MESE.nii", echo)));
        }
        let tree = scan_bids_tree(dir.path()).unwrap();
        let display = &tree.subjects[0].runs[0].display;
        assert!(display.contains("MEGRE"), "display: {}", display);
        assert!(display.contains("(+MESE)"), "expected MESE annotation, got: {}", display);
        // The key_string (used for include/exclude filtering) must NOT carry the annotation.
        assert!(!tree.subjects[0].runs[0].key_string.contains("+MESE"));
    }

    #[test]
    fn test_discover_num_echoes_filter() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_echo_bids(dir.path());
        let filter = DiscoveryFilter {
            num_echoes: Some(2),
            ..Default::default()
        };
        let runs = discover_runs(dir.path(), &filter).unwrap();
        assert_eq!(runs[0].echoes.len(), 2, "Should truncate to 2 echoes");
    }
}
