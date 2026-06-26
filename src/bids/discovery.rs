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

/// A complete QSM acquisition run with all echoes.
#[derive(Debug, Clone)]
pub struct QsmRun {
    pub key: AcquisitionKey,
    pub echoes: Vec<EchoFiles>,
    pub magnetic_field_strength: f64,
    pub echo_times: Vec<f64>,
    pub b0_dir: (f64, f64, f64),
    /// Volume dimensions (nx, ny, nz) from the first phase NIfTI header.
    pub dims: (usize, usize, usize),
    /// Whether magnitude files are available for this run.
    pub has_magnitude: bool,
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

    for (key, mut files) in groups {
        // Sort by echo number
        files.sort_by_key(|(_, ent)| ent.echo.unwrap_or(1));

        // Apply echo limit
        if let Some(max_echoes) = filter.num_echoes {
            files.truncate(max_echoes);
        }

        let mut echoes = Vec::new();
        let mut echo_times = Vec::new();
        let mut b0_tesla = 0.0f64;
        let mut b0_dir = (0.0, 0.0, 1.0);

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
                    b0_dir = (dir[0], dir[1], dir[2]);
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

        if echoes.is_empty() {
            continue;
        }

        // Read volume dimensions from the first phase NIfTI header (fast, header-only)
        let dims = qsm_core::nifti_io::read_nifti_dims(&echoes[0].phase_nifti)
            .map_err(QsmxtError::NiftiIo)?;
        let has_magnitude = echoes[0].magnitude_nifti.is_some();

        runs.push(QsmRun {
            key,
            echoes,
            magnetic_field_strength: b0_tesla,
            echo_times,
            b0_dir,
            dims,
            has_magnitude,
        });
    }

    // Sort by key for deterministic ordering
    runs.sort_by_key(|a| a.key.to_string());

    Ok(runs)
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
            let run_leaves: Vec<BidsRunLeaf> = keys.into_iter().map(|key| {
                let key_string = key.to_string();
                // Build display: everything after sub-XX[_ses-YY]_
                let display = build_run_display(&key);
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
