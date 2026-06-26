use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bids::entities::AcquisitionKey;
use crate::error::QsmxtError;
use crate::pipeline::config::PipelineConfig;

/// Metadata about the run, extracted during the load step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub dims: (usize, usize, usize),
    pub voxel_size: (f64, f64, f64),
    pub affine: [f64; 16],
    pub n_echoes: usize,
    pub echo_times: Vec<f64>,
    pub b0_direction: (f64, f64, f64),
    pub field_strength: f64,
    pub has_magnitude: bool,
}

/// Record of a completed step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub outputs: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Hash of the step's algorithm + parameters for cache invalidation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params_hash: Option<String>,
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
}

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
            if let Ok(state) = serde_json::from_str::<PipelineState>(&text) {
                log::info!("Resuming from cached pipeline state");
                return state;
            } else {
                log::warn!("Could not parse pipeline state file. Starting fresh.");
            }
        }

        Self::new(config, run_key)
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
            // Verify all output files still exist
            if !record.outputs.iter().all(|p| p.exists()) {
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
            },
        );
        self.current_step = None;
    }

    /// Mark the entire run as complete.
    pub fn mark_run_complete(&mut self) {
        self.status = "complete".to_string();
        self.current_step = None;
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
        // Also invalidate downstream steps
        let downstream = downstream_steps(step_name);
        for ds in downstream {
            self.completed_steps.remove(*ds);
        }
    }

    /// Get all completed step names.
    #[allow(dead_code)]
    pub fn completed_step_names(&self) -> HashSet<&str> {
        self.completed_steps.keys().map(|s| s.as_str()).collect()
    }
}

/// Compute a hash of the pipeline config for change detection.
fn config_hash(config: &PipelineConfig) -> String {
    let toml = config.to_toml().unwrap_or_default();
    format!("{:x}", md5_simple(&toml))
}

/// Compute a hash of step parameters (algorithm + params JSON) for per-step cache invalidation.
pub fn step_params_hash(algorithm: Option<&str>, params: &serde_json::Value) -> String {
    let s = format!("{}:{}", algorithm.unwrap_or(""), params);
    format!("{:x}", md5_simple(&s))
}

/// Simple hash (not cryptographic, just for change detection).
fn md5_simple(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Return step names that depend on the given step (for invalidation).
#[allow(dead_code)]
fn downstream_steps(step_name: &str) -> &'static [&'static str] {
    match step_name {
        "load" => &[
            "resample",
            "scale_phase",
            "inhomog",
            "magnitude",
            "mask",
            "swi",
            "t2star_r2star",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        "resample" => &[
            "scale_phase",
            "inhomog",
            "magnitude",
            "mask",
            "swi",
            "t2star_r2star",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        "scale_phase" => &[
            "mask",
            "swi",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        "inhomog" => &[
            "magnitude",
            "mask",
            "swi",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        // Combined magnitude feeds masking, SWI, MEDI weighting, and QSMART vasculature.
        "magnitude" => &[
            "mask",
            "swi",
            "invert",
            "qsmart",
            "reference",
        ],
        "mask" => &[
            "swi",
            "t2star_r2star",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        "swi" => &[],
        "t2star_r2star" => &[],
        "unwrap" => &["bgremove", "invert", "tgv", "qsmart", "reference"],
        "bgremove" => &["invert", "reference"],
        "invert" | "tgv" | "qsmart" => &["reference"],
        "reference" => &[],
        _ => &[],
    }
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

/// Remove intermediate files (workflow dir), keeping only final outputs.
pub fn clean_intermediates(state: &PipelineState, output_dir: &Path, key: &AcquisitionKey) {
    let final_steps: HashSet<&str> =
        ["mask", "magnitude", "reference", "swi", "t2star_r2star"].iter().copied().collect();

    for (step_name, record) in &state.completed_steps {
        if !final_steps.contains(step_name.as_str()) {
            for path in &record.outputs {
                if path.exists() {
                    log::info!("Cleaning intermediate: {}", path.display());
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    // Remove state file itself
    let sf = state_file_path(output_dir, key);
    let _ = std::fs::remove_file(sf);
}

#[cfg(test)]
mod tests {
    use super::*;
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
        state.mark_run_complete();
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
    fn test_downstream_steps_load() {
        let ds = downstream_steps("load");
        assert!(ds.contains(&"mask"));
        assert!(ds.contains(&"unwrap"));
        assert!(ds.contains(&"reference"));
    }

    #[test]
    fn test_downstream_steps_unknown() {
        let ds = downstream_steps("nonexistent");
        assert!(ds.is_empty());
    }

    #[test]
    fn test_downstream_steps_leaf() {
        assert!(downstream_steps("swi").is_empty());
        assert!(downstream_steps("reference").is_empty());
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
        let intermediate = dir.path().join("unwrapped.nii");
        std::fs::write(&intermediate, "fake").unwrap();
        state.mark_completed("unwrap", vec![intermediate.clone()], None);

        // Create a fake final output
        let final_output = dir.path().join("qsm.nii");
        std::fs::write(&final_output, "fake").unwrap();
        state.mark_completed("reference", vec![final_output.clone()], None);

        clean_intermediates(&state, dir.path(), &key);

        // Intermediate should be removed, final kept
        assert!(!intermediate.exists());
        assert!(final_output.exists());
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
