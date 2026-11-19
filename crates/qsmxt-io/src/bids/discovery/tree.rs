//! A lightweight scan of a BIDS tree, for the TUI's filters.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use glob::glob;

use crate::bids::entities::{self, AcquisitionKey, Part};
use crate::error::QsmxtError;

use super::signal_patterns;

/// A run leaf in the BIDS tree (one QSM acquisition).
#[derive(Debug, Clone)]
pub struct BidsRunLeaf {
    /// Display string: the distinguishing part (e.g. "acq-gre_run-1_MEGRE")
    pub display: String,
    /// Full AcquisitionKey as string for pattern matching
    pub key_string: String,
    /// The parsed key, so the TUI can group orientations without re-parsing filenames.
    pub key: AcquisitionKey,
    /// Whether this run is selected for processing
    pub selected: bool,
    /// Whether the run is uncombined multi-coil data (`coil-` files), which coil combination reads.
    pub multi_coil: bool,
    /// Its number of echoes, from the file names.
    pub n_echoes: usize,
    /// Whether it has magnitude images (or real/imaginary parts, which give them), from the file
    /// names: motion correction needs them.
    pub has_magnitude: bool,
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
    let patterns = signal_patterns(bids_dir);

    // Subjects/sessions that also have a matching MESE acquisition (for the "(+MESE)" annotation).
    let mese_present = mese_subject_sessions(bids_dir);

    // Collect unique AcquisitionKeys grouped by subject and session
    // subject -> (session -> [AcquisitionKey])
    let mut tree_map: BTreeMap<String, BTreeMap<Option<String>, Vec<AcquisitionKey>>> = BTreeMap::new();
    let mut seen_keys = std::collections::HashSet::new();
    let mut coil_keys = std::collections::HashSet::new();
    // Each key's echoes and whether it has magnitude, for what motion correction can run on.
    let mut echoes: HashMap<String, (std::collections::BTreeSet<u32>, bool)> = HashMap::new();

    for pattern in &patterns {
        for entry in glob(pattern).map_err(|e| QsmxtError::BidsDiscovery(e.to_string()))? {
            let path = entry.map_err(|e| QsmxtError::BidsDiscovery(e.to_string()))?;
            let filename = match path.file_name().and_then(|f| f.to_str()) {
                Some(f) => f,
                None => continue,
            };

            if let Some(ent) = entities::parse_entities(filename) {
                if !matches!(ent.part, Some(Part::Phase | Part::Real)) {
                    continue;
                }

                let key = ent.acquisition_key();
                let key_str = key.to_string();
                if ent.coil.is_some() {
                    coil_keys.insert(key_str.clone());
                }
                let entry = echoes.entry(key_str.clone()).or_default();
                entry.0.insert(ent.echo.unwrap_or(1));
                entry.1 |= ent.part == Some(Part::Real) || entities::phase_to_magnitude_path(&path).exists();
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
                let multi_coil = coil_keys.contains(&key_string);
                let (n_echoes, has_magnitude) = echoes.get(&key_string).map_or((1, false), |(e, m)| (e.len(), *m));
                BidsRunLeaf { display, key_string, key, selected: true, multi_coil, n_echoes, has_magnitude }
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
            if matches!(ent.part, Some(Part::Phase | Part::Real | Part::Imaginary)) {
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
