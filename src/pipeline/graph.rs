use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bids::entities::AcquisitionKey;
use crate::error::QsmxtError;
use crate::pipeline::config::PipelineConfig;
use qsmxt_engine::cache::config_hash;
pub use qsmxt_engine::cache::{step_params_hash, RELIABLE_SUFFIX};
pub use qsmxt_engine::graph::downstream_of;

pub use qsmxt_engine::scan::RunMetadata;

/// Record of a completed step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub outputs: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Hash of the step's algorithm + parameters for cache invalidation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params_hash: Option<String>,
    /// `--clean-intermediates` deleted this step's workflow outputs after the run. The step still
    /// counts as done while the configuration is the one it ran with (see
    /// [`PipelineState::config_unchanged`]); otherwise it is recomputed when it is reached.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outputs_removed: bool,
}

/// Persistent pipeline state, serialised to JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineState {
    pub version: String,
    pub config_hash: String,
    pub run_key: String,
    pub status: String,
    #[serde(default)]
    pub current_step: Option<String>,
    pub completed_steps: HashMap<String, StepRecord>,
    #[serde(default)]
    pub run_metadata: Option<RunMetadata>,
    /// The configuration is the one the last completed run used, and every output it recorded
    /// (other than those `--clean-intermediates` removed) is still on disk: nothing will be
    /// recomputed, so steps whose intermediates were cleaned can be trusted as they are.
    #[serde(skip)]
    pub config_unchanged: bool,
    /// The `space-` label the published outputs carry (`None`: the acquired grid). Read back by
    /// [`DerivativeOutputs::published`](crate::bids::derivatives::DerivativeOutputs::published).
    #[serde(default)]
    pub space: Option<String>,
    /// The output naming scheme the recorded outputs were written under ([`NAMING_VERSION`]).
    #[serde(default)]
    pub naming: u32,
}

/// The version of the output names (`bids::naming`). A state recorded under another one names
/// files the pipeline no longer writes or reads, so it is discarded and the run recomputed once.
/// 0: before #275; 1: #275's BIDS names.
pub const NAMING_VERSION: u32 = 1;

impl PipelineState {
    /// Create a fresh state for a new run.
    pub fn new(config: &PipelineConfig, run_key: &AcquisitionKey) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_hash: config_hash(config),
            run_key: format!("{}", run_key),
            status: "pending".to_string(),
            current_step: None,
            completed_steps: HashMap::new(),
            run_metadata: None,
            config_unchanged: false,
            space: None,
            naming: NAMING_VERSION,
        }
    }

    /// Load existing state from disk, or create new if missing/incompatible.
    pub fn load_or_create(
        state_path: &Path,
        config: &PipelineConfig,
        run_key: &AcquisitionKey,
        force: bool,
    ) -> Self {
        if force {
            return Self::new(config, run_key);
        }

        if let Ok(text) = std::fs::read_to_string(state_path) {
            if let Ok(mut state) = serde_json::from_str::<PipelineState>(&text) {
                if state.naming != NAMING_VERSION {
                    log::info!(
                        "{run_key}: outputs were written under an earlier naming scheme; removing \
                         them and recomputing the run once"
                    );
                    state.remove_recorded_outputs(state_path);
                    return Self::new(config, run_key);
                }
                log::info!("Resuming from cached pipeline state");
                state.config_unchanged = state.config_hash == config_hash(config)
                    && state.completed_steps.values()
                        .all(|r| r.outputs_removed || r.outputs.iter().all(|p| p.exists()));
                return state;
            } else {
                log::warn!("Could not parse pipeline state file. Starting fresh.");
            }
        }

        Self::new(config, run_key)
    }

    /// Delete every file this state recorded writing, and the run's working-grid copies, so files
    /// under names the pipeline no longer writes do not linger next to the renamed ones.
    ///
    /// Only files under the derivatives directory that holds this state (`<deriv>/workflow/...`)
    /// are touched: a recorded output is never an input, but this keeps it that way.
    pub fn remove_recorded_outputs(&self, state_path: &Path) {
        let Some(deriv) = state_path.ancestors().find(|a| a.file_name().is_some_and(|n| n == "workflow"))
            .and_then(|w| w.parent()) else { return };
        for rec in self.completed_steps.values() {
            for p in &rec.outputs {
                if p.starts_with(deriv) && p.is_file() {
                    if let Err(e) = std::fs::remove_file(p) {
                        log::warn!("could not remove {}: {e}", p.display());
                    }
                    // ...and the sidecar provenance gave it.
                    if let Some(js) = crate::bids::entities::sidecar_path(p).filter(|j| j.is_file()) {
                        let _ = std::fs::remove_file(js);
                    }
                }
            }
        }
        if let Some(run_dir) = state_path.parent() {
            let stash = run_dir.join("working-grid");
            if stash.is_dir() {
                let _ = std::fs::remove_dir_all(stash);
            }
        }
    }

    /// Save state to disk.
    pub fn save(&self, state_path: &Path) -> crate::Result<()> {
        if let Some(parent) = state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| QsmxtError::Config(format!("Failed to serialize state: {}", e)))?;
        std::fs::write(state_path, json)?;
        Ok(())
    }

    /// Check if a step is completed, outputs exist, and params haven't changed.
    ///
    /// If `expected_params_hash` is provided, the cached step must match it.
    /// When a parameter mismatch is detected, the step AND all downstream steps
    /// are invalidated so they will be re-executed.
    pub fn is_step_cached_with_hash(&mut self, step_name: &str, expected_params_hash: Option<&str>) -> bool {
        if let Some(record) = self.completed_steps.get(step_name) {
            // Cleaned intermediates are trusted only while nothing can need them again; once the
            // configuration has changed the step is recomputed — with the same parameters, which
            // leaves what is downstream of it cached (see the runner's `complete_step`).
            if record.outputs_removed {
                if !self.config_unchanged {
                    return false;
                }
            } else if !record.outputs.iter().all(|p| p.exists()) {
                // Verify all output files still exist
                return false;
            }
            // If caller provides an expected hash, the stored hash must exist and match
            if let Some(expected) = expected_params_hash {
                match &record.params_hash {
                    Some(stored) if stored == expected => {}
                    Some(_) => {
                        log::info!("Step '{}' parameters changed — invalidating cache", step_name);
                        self.invalidate(step_name);
                        return false;
                    }
                    None => {
                        log::info!("Step '{}' missing params hash (legacy cache) — invalidating", step_name);
                        self.invalidate(step_name);
                        return false;
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// Check if a step is completed and its outputs still exist (no param check).
    pub fn is_step_cached(&mut self, step_name: &str) -> bool {
        self.is_step_cached_with_hash(step_name, None)
    }

    /// Mark a step as the current one being processed.
    #[allow(dead_code)]
    pub fn set_current(&mut self, step_name: &str) {
        self.status = "in_progress".to_string();
        self.current_step = Some(step_name.to_string());
    }

    /// Mark a step as completed with its output paths and parameter hash.
    pub fn mark_completed(&mut self, step_name: &str, outputs: Vec<PathBuf>, params_hash: Option<String>) {
        self.completed_steps.insert(
            step_name.to_string(),
            StepRecord {
                outputs,
                metadata: None,
                params_hash,
                outputs_removed: false,
            },
        );
        self.current_step = None;
    }

    /// Like [`Self::mark_completed`], but keeping a step's own metadata in the state file.
    ///
    /// Used for figures a later stage needs and cannot recompute — SynthSeg's per-label volumes
    /// come from the soft posteriors, which are gone by the time the label volume is on disk.
    pub fn mark_completed_with_metadata(
        &mut self, step_name: &str, outputs: Vec<PathBuf>, params_hash: Option<String>,
        metadata: Option<serde_json::Value>,
    ) {
        self.completed_steps.insert(
            step_name.to_string(),
            StepRecord { outputs, metadata, params_hash, outputs_removed: false },
        );
        self.current_step = None;
    }

    /// Mark the entire run as complete, under the configuration it ran with.
    pub fn mark_run_complete(&mut self, config: &PipelineConfig) {
        self.status = "complete".to_string();
        self.current_step = None;
        self.config_hash = config_hash(config);
    }

    /// Get output paths for a completed step.
    #[allow(dead_code)]
    pub fn step_outputs(&self, step_name: &str) -> Option<&[PathBuf]> {
        self.completed_steps
            .get(step_name)
            .map(|r| r.outputs.as_slice())
    }

    /// Invalidate a step and all steps that depend on it.
    #[allow(dead_code)]
    pub fn invalidate(&mut self, step_name: &str) {
        self.completed_steps.remove(step_name);
        for ds in downstream_of(step_name) {
            self.completed_steps.remove(&ds);
        }
    }

    /// Get all completed step names.
    #[allow(dead_code)]
    pub fn completed_step_names(&self) -> HashSet<&str> {
        self.completed_steps.keys().map(|s| s.as_str()).collect()
    }
}

/// Has `key`'s per-run pipeline completed under `config`? `qsmxt aggregate` asks this of every run
/// before the dataset-level steps, in place of the per-run results a local run has in hand. The
/// error says why not, for the message naming the run.
pub fn run_completed_under(output_dir: &Path, key: &AcquisitionKey, config: &PipelineConfig) -> Result<(), String> {
    let path = state_file_path(output_dir, key);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("has not been processed (no {})", path.display()))?;
    let state: PipelineState = serde_json::from_str(&text)
        .map_err(|e| format!("has an unreadable pipeline state {}: {e}", path.display()))?;
    if state.status != "complete" {
        return Err(match state.current_step {
            Some(step) => format!("did not complete (stopped in step '{step}')"),
            None => format!("did not complete (status '{}')", state.status),
        });
    }
    if state.config_hash != config_hash(config) {
        return Err("was processed with a different configuration".to_string());
    }
    Ok(())
}

/// The path to the pipeline state file for a given run (in workflow dir).
pub fn state_file_path(output_dir: &Path, key: &AcquisitionKey) -> PathBuf {
    let mut dir = output_dir.join("workflow").join(format!("sub-{}", key.subject));
    if let Some(ref ses) = key.session {
        dir = dir.join(format!("ses-{}", ses));
    }
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
    if !parts.is_empty() {
        dir = dir.join(parts.join("_"));
    }
    dir.join(".pipeline_state.json")
}

/// Remove a completed run's intermediate files, keeping every published output and the state.
///
/// Only files under `workflow/` are ever deleted, whichever step wrote them: a step may publish
/// into `anat/` (coil combination's `rec-mcpc3ds` echoes, motion correction's coverage mask),
/// and a published output is never an intermediate. Each step's `provenance.json` stays, so a
/// later run can still assemble BIDS-Prov from steps it does not recompute, and so does the
/// state file, with the cleaned steps marked: an immediate re-run with the same configuration
/// recomputes nothing, and one with a changed configuration recomputes the intermediates it needs.
///
/// Run after everything that reads intermediates across runs (the multi-orientation fan-in reads
/// each member's local field), not at the end of each run.
pub fn clean_intermediates(output_dir: &Path, key: &AcquisitionKey) -> crate::Result<()> {
    let state_path = state_file_path(output_dir, key);
    let text = std::fs::read_to_string(&state_path)?;
    let mut state: PipelineState = serde_json::from_str(&text)
        .map_err(|e| QsmxtError::Config(format!("{}: {e}", state_path.display())))?;
    let workflow = output_dir.join("workflow");
    for record in state.completed_steps.values_mut() {
        let mut removed = false;
        for path in record.outputs.iter().filter(|p| p.starts_with(&workflow)) {
            if path.exists() {
                log::info!("Cleaning intermediate: {}", path.display());
                std::fs::remove_file(path)?;
            }
            removed = true;
        }
        record.outputs_removed |= removed;
    }
    state.save(&state_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsmxt_engine::graph::downstream_steps;

    /// Every step the runner records has an entry in the invalidation table (#272: `motion` and
    /// `reference-singlepass` had none, so nothing they fed was ever invalidated through them).
    #[test]
    fn every_recorded_step_has_an_invalidation_entry() {
        for step in crate::bids::bidsprov::STEPS {
            let base = step.strip_suffix(RELIABLE_SUFFIX).unwrap_or(step);
            assert!(downstream_steps(base).is_some() || *step == "twopass",
                    "{step} is recorded but not in the invalidation table");
        }
        // The edges the issue found missing.
        let sp = downstream_of("scale_phase");
        for step in ["magnitude", "motion", "t2star_r2star", "segmentation", "reference-singlepass"] {
            assert!(sp.contains(&step.to_string()) || downstream_of("magnitude").contains(&step.to_string()),
                    "scale_phase should (transitively) invalidate {step}: {sp:?}");
        }
        assert!(downstream_of("invert").contains(&"reference-singlepass".to_string()));
        assert!(downstream_of("reference-singlepass").contains(&"analysis".to_string()));
    }

    #[test]
    fn test_new_state() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let state = PipelineState::new(&config, &key);
        assert_eq!(state.status, "pending");
        assert!(state.completed_steps.is_empty());
        assert!(!state.config_hash.is_empty());
    }

    #[test]
    fn test_mark_completed_and_cached() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);

        // Not cached yet
        assert!(!state.is_step_cached("mask"));

        // Mark completed with no file paths (metadata-only step)
        state.mark_completed("load", vec![], None);
        assert!(state.is_step_cached("load"));

        // Mark completed with a file that doesn't exist — should not be cached
        state.mark_completed("mask", vec![PathBuf::from("/nonexistent/mask.nii")], None);
        assert!(!state.is_step_cached("mask"));
    }

    #[test]
    fn test_invalidate_downstream() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);
        state.mark_completed("load", vec![], None);
        state.mark_completed("mask", vec![], None);
        state.mark_completed("unwrap", vec![], None);
        state.mark_completed("bgremove", vec![], None);

        // Invalidating mask should also remove unwrap, bgremove
        state.invalidate("mask");
        assert!(state.completed_steps.contains_key("load"));
        assert!(!state.completed_steps.contains_key("mask"));
        assert!(!state.completed_steps.contains_key("unwrap"));
        assert!(!state.completed_steps.contains_key("bgremove"));
    }

    #[test]
    fn test_invalidate_qsmart_cascades_to_reference() {
        // Regression: changing a QSMART param must re-reference the final map.
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);
        state.mark_completed("qsmart", vec![], None);
        state.mark_completed("reference", vec![], None);

        state.invalidate("qsmart");
        assert!(!state.completed_steps.contains_key("qsmart"));
        assert!(!state.completed_steps.contains_key("reference"),
            "reference must be invalidated when qsmart changes");
    }

    #[test]
    fn test_qsmart_invalidated_by_upstream_steps() {
        // QSMART reads the mask, the field (unwrap) and the magnitude, so a change to
        // any of them — or anything further upstream — must re-run it.
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        for upstream in ["load", "scale_phase", "magnitude", "mask", "unwrap"] {
            let mut state = PipelineState::new(&config, &key);
            state.mark_completed("qsmart", vec![], None);
            state.invalidate(upstream);
            assert!(!state.completed_steps.contains_key("qsmart"),
                "changing '{}' must invalidate qsmart", upstream);
        }
    }

    #[test]
    fn test_state_json_roundtrip() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);
        state.mark_completed("load", vec![], None);
        state.mark_completed("mask", vec![PathBuf::from("/tmp/mask.nii")], None);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        state.save(&path).unwrap();

        let loaded = PipelineState::load_or_create(&path, &config, &key, false);
        assert_eq!(loaded.config_hash, state.config_hash);
        assert!(loaded.completed_steps.contains_key("load"));
        assert!(loaded.completed_steps.contains_key("mask"));
    }

    #[test]
    fn test_config_change_preserves_state_with_per_step_invalidation() {
        let config1 = PipelineConfig::default();
        let mut config2 = PipelineConfig::default();
        config2.inversion.algorithm = crate::pipeline::config::QsmAlgorithm::Tkd;

        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };

        let mut state = PipelineState::new(&config1, &key);
        let hash = step_params_hash(None, &serde_json::json!({}));
        state.mark_completed("load", vec![], Some(hash.clone()));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        state.save(&path).unwrap();

        // Loading with different config should preserve existing steps
        let mut loaded = PipelineState::load_or_create(&path, &config2, &key, false);
        assert!(loaded.completed_steps.contains_key("load"));

        // Step with matching params_hash is still cached
        assert!(loaded.is_step_cached_with_hash("load", Some(&hash)));

        // Step with different params_hash is not cached
        let different_hash = step_params_hash(Some("tkd"), &serde_json::json!({"threshold": 0.1}));
        assert!(!loaded.is_step_cached_with_hash("load", Some(&different_hash)));
    }

    #[test]
    fn test_set_current() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);
        state.set_current("mask");
        assert_eq!(state.status, "in_progress");
        assert_eq!(state.current_step, Some("mask".to_string()));
    }

    #[test]
    fn test_step_outputs() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);
        assert!(state.step_outputs("mask").is_none());
        state.mark_completed("mask", vec![PathBuf::from("/tmp/mask.nii")], None);
        let outputs = state.step_outputs("mask").unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0], PathBuf::from("/tmp/mask.nii"));
    }

    #[test]
    fn test_completed_step_names() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);
        state.mark_completed("load", vec![], None);
        state.mark_completed("mask", vec![], None);
        let names = state.completed_step_names();
        assert!(names.contains("load"));
        assert!(names.contains("mask"));
        assert!(!names.contains("unwrap"));
    }

    #[test]
    fn test_mark_run_complete() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let mut state = PipelineState::new(&config, &key);
        state.mark_run_complete(&config);
        assert_eq!(state.status, "complete");
        assert!(state.current_step.is_none());
    }

    #[test]
    fn test_state_file_path_with_session() {
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: Some("pre".to_string()),
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let path = state_file_path(Path::new("/out"), &key);
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/ses-pre/.pipeline_state.json"));
    }

    #[test]
    fn test_clean_intermediates() {
        let dir = tempfile::tempdir().unwrap();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let config = PipelineConfig::default();
        let mut state = PipelineState::new(&config, &key);

        // Create a fake intermediate file
        std::fs::create_dir_all(dir.path().join("workflow")).unwrap();
        let intermediate = dir.path().join("workflow/unwrapped.nii");
        std::fs::write(&intermediate, "fake").unwrap();
        // ...and a published output of a step that is otherwise intermediate.
        let published = dir.path().join("desc-motioncoverage_mask.nii");
        std::fs::write(&published, "fake").unwrap();
        state.mark_completed("unwrap", vec![intermediate.clone()], None);
        state.mark_completed("motion", vec![published.clone()], None);

        // Create a fake final output
        let final_output = dir.path().join("qsm.nii");
        std::fs::write(&final_output, "fake").unwrap();
        state.mark_completed("reference", vec![final_output.clone()], None);
        state.mark_run_complete(&config);
        let state_path = state_file_path(dir.path(), &key);
        state.save(&state_path).unwrap();

        clean_intermediates(dir.path(), &key).unwrap();

        // Intermediate should be removed, final kept
        assert!(!intermediate.exists());
        assert!(final_output.exists());
        assert!(published.exists(), "a published output outside workflow/ was cleaned");
        // The state survives, with the cleaned step marked: trusted while the configuration is
        // unchanged, recomputed once it changes.
        let mut back = PipelineState::load_or_create(&state_path, &config, &key, false);
        assert!(back.completed_steps["unwrap"].outputs_removed);
        assert!(!back.completed_steps["reference"].outputs_removed);
        assert!(back.is_step_cached("unwrap"));
        let mut other = config.clone();
        other.inversion.algorithm = crate::pipeline::config::QsmAlgorithm::Tkd;
        let mut back = PipelineState::load_or_create(&state_path, &other, &key, false);
        assert!(!back.is_step_cached("unwrap"));
        assert!(back.is_step_cached("reference"), "a final step's own cache is not touched");
    }

    #[test]
    fn test_load_or_create_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, "not valid json").unwrap();
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };
        let state = PipelineState::load_or_create(&path, &config, &key, false);
        // Should create fresh state
        assert_eq!(state.status, "pending");
        assert!(state.completed_steps.is_empty());
    }

    #[test]
    fn test_force_ignores_cache() {
        let config = PipelineConfig::default();
        let key = AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        };

        let mut state = PipelineState::new(&config, &key);
        state.mark_completed("load", vec![], None);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        state.save(&path).unwrap();

        let loaded = PipelineState::load_or_create(&path, &config, &key, true);
        assert!(loaded.completed_steps.is_empty());
    }
}
