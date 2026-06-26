use std::path::{Path, PathBuf};

use crate::bids::entities::AcquisitionKey;

/// Manages BIDS derivative output paths.
///
/// Final outputs (QSM, mask, magnitude, SWI, T2*, R2*) go to `output_dir/sub-XX/anat/`.
/// Intermediates go to `output_dir/workflow/sub-XX/anat/<step>/`.
/// Each step's workflow folder also contains a `provenance.json`.
pub struct DerivativeOutputs {
    pub output_dir: PathBuf,
}

impl DerivativeOutputs {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_owned(),
        }
    }

    /// Build the subject/session anat directory for final outputs.
    fn anat_dir(&self, key: &AcquisitionKey) -> PathBuf {
        let mut dir = self.output_dir.join(format!("sub-{}", key.subject));
        if let Some(ref ses) = key.session {
            dir = dir.join(format!("ses-{}", ses));
        }
        dir.join("anat")
    }

    /// Build the per-run workflow directory.
    ///
    /// Structure: `workflow/sub-{subject}/[ses-{session}/][acq-{acq}[_run-{run}]/]`
    fn workflow_run_dir(&self, key: &AcquisitionKey) -> PathBuf {
        let mut dir = self.output_dir.join("workflow").join(format!("sub-{}", key.subject));
        if let Some(ref ses) = key.session {
            dir = dir.join(format!("ses-{}", ses));
        }
        // Build a combined directory name from remaining entities
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
        dir
    }

    /// Build the workflow step directory for a given step.
    fn workflow_step_dir(&self, key: &AcquisitionKey, step: &str) -> PathBuf {
        self.workflow_run_dir(key).join(step)
    }

    /// Build a NIfTI output path with the given suffix (final outputs).
    fn nifti_path(&self, key: &AcquisitionKey, suffix: &str) -> PathBuf {
        self.anat_dir(key).join(format!("{}_{}.nii", key.basename(), suffix))
    }

    /// Build a NIfTI output path in a workflow step directory.
    fn workflow_nifti_path(&self, key: &AcquisitionKey, step: &str, suffix: &str) -> PathBuf {
        self.workflow_step_dir(key, step).join(format!("{}_{}.nii", key.basename(), suffix))
    }

    /// Path to provenance.json for a given step.
    pub fn provenance_path(&self, key: &AcquisitionKey, step: &str) -> PathBuf {
        self.workflow_step_dir(key, step).join("provenance.json")
    }

    // Final outputs
    pub fn qsm_path(&self, key: &AcquisitionKey) -> PathBuf { self.nifti_path(key, "Chimap") }
    pub fn mask_path(&self, key: &AcquisitionKey) -> PathBuf { self.nifti_path(key, "mask") }
    pub fn magnitude_path(&self, key: &AcquisitionKey) -> PathBuf { self.nifti_path(key, "magnitude") }
    pub fn swi_path(&self, key: &AcquisitionKey) -> PathBuf { self.nifti_path(key, "swi") }
    pub fn swi_mip_path(&self, key: &AcquisitionKey) -> PathBuf { self.nifti_path(key, "minIP") }
    pub fn t2star_path(&self, key: &AcquisitionKey) -> PathBuf { self.nifti_path(key, "T2starmap") }
    pub fn r2star_path(&self, key: &AcquisitionKey) -> PathBuf { self.nifti_path(key, "R2starmap") }

    // Intermediate outputs (in workflow step directories)
    pub fn field_ppm_path(&self, key: &AcquisitionKey) -> PathBuf { self.workflow_nifti_path(key, "unwrap", "field-ppm") }
    pub fn local_field_path(&self, key: &AcquisitionKey) -> PathBuf { self.workflow_nifti_path(key, "bgremove", "localfield") }
    pub fn bg_mask_path(&self, key: &AcquisitionKey) -> PathBuf { self.workflow_nifti_path(key, "bgremove", "bgmask") }
    pub fn chi_raw_path(&self, key: &AcquisitionKey) -> PathBuf { self.workflow_nifti_path(key, "invert", "Chimap-raw") }

    // Per-echo intermediates (in scale_phase step directory)
    pub fn phase_scaled_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.workflow_step_dir(key, "scale_phase").join(format!("{}_echo-{}_phase-scaled.nii", key.basename(), echo))
    }
    pub fn mag_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.workflow_step_dir(key, "scale_phase").join(format!("{}_echo-{}_mag.nii", key.basename(), echo))
    }

    // Pipeline state (per-run, in workflow directory)
    pub fn state_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_run_dir(key).join(".pipeline_state.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_no_session() -> AcquisitionKey {
        AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        }
    }

    fn key_with_session() -> AcquisitionKey {
        AcquisitionKey {
            subject: "01".to_string(),
            session: Some("pre".to_string()),
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        }
    }

    fn output() -> DerivativeOutputs {
        DerivativeOutputs::new(Path::new("/out"))
    }

    // --- Final output paths (not under workflow/) ---

    #[test]
    fn test_qsm_path_no_session() {
        let path = output().qsm_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/anat/sub-01_Chimap.nii"));
    }

    #[test]
    fn test_qsm_path_with_session() {
        let path = output().qsm_path(&key_with_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/ses-pre/anat/sub-01_ses-pre_Chimap.nii"));
    }

    #[test]
    fn test_mask_path() {
        let path = output().mask_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/anat/sub-01_mask.nii"));
    }

    #[test]
    fn test_magnitude_path() {
        let path = output().magnitude_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/anat/sub-01_magnitude.nii"));
    }

    #[test]
    fn test_swi_and_mip_paths() {
        let o = output();
        let key = key_no_session();
        assert!(!o.swi_path(&key).to_str().unwrap().contains("workflow"));
        assert!(!o.swi_mip_path(&key).to_str().unwrap().contains("workflow"));
    }

    #[test]
    fn test_t2star_and_r2star_paths() {
        let o = output();
        let key = key_no_session();
        assert!(!o.t2star_path(&key).to_str().unwrap().contains("workflow"));
        assert!(!o.r2star_path(&key).to_str().unwrap().contains("workflow"));
    }

    // --- Workflow step paths ---

    #[test]
    fn test_field_ppm_path() {
        let path = output().field_ppm_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/unwrap/sub-01_field-ppm.nii"));
    }

    #[test]
    fn test_local_field_and_bgmask_paths() {
        let o = output();
        let key = key_no_session();
        assert_eq!(o.local_field_path(&key), PathBuf::from("/out/workflow/sub-01/bgremove/sub-01_localfield.nii"));
        assert_eq!(o.bg_mask_path(&key), PathBuf::from("/out/workflow/sub-01/bgremove/sub-01_bgmask.nii"));
    }

    #[test]
    fn test_chi_raw_path() {
        let path = output().chi_raw_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/invert/sub-01_Chimap-raw.nii"));
    }

    #[test]
    fn test_phase_scaled_path() {
        let path = output().phase_scaled_path(&key_no_session(), 2);
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/scale_phase/sub-01_echo-2_phase-scaled.nii"));
    }

    #[test]
    fn test_mag_path() {
        let path = output().mag_path(&key_no_session(), 1);
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/scale_phase/sub-01_echo-1_mag.nii"));
    }

    #[test]
    fn test_state_path() {
        let path = output().state_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/.pipeline_state.json"));
    }

    #[test]
    fn test_provenance_path() {
        let path = output().provenance_path(&key_no_session(), "bgremove");
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/bgremove/provenance.json"));
    }

    // --- Session paths ---

    #[test]
    fn test_workflow_path_with_session() {
        let o = output();
        let key = key_with_session();
        assert_eq!(
            o.field_ppm_path(&key),
            PathBuf::from("/out/workflow/sub-01/ses-pre/unwrap/sub-01_ses-pre_field-ppm.nii")
        );
        assert_eq!(
            o.provenance_path(&key, "unwrap"),
            PathBuf::from("/out/workflow/sub-01/ses-pre/unwrap/provenance.json")
        );
    }

    #[test]
    fn test_workflow_run_dir_with_acq_run() {
        let key = AcquisitionKey {
            subject: "05".to_string(),
            session: None,
            acquisition: Some("mygrea".to_string()),
            reconstruction: None,
            inversion: None,
            run: Some("1".to_string()),
            suffix: "MEGRE".to_string(),
        };
        let o = output();
        assert_eq!(
            o.field_ppm_path(&key),
            PathBuf::from("/out/workflow/sub-05/acq-mygrea_run-1/unwrap/sub-05_acq-mygrea_run-1_field-ppm.nii")
        );
        assert_eq!(
            o.state_path(&key),
            PathBuf::from("/out/workflow/sub-05/acq-mygrea_run-1/.pipeline_state.json")
        );
    }

    #[test]
    fn test_path_with_all_entities() {
        let key = AcquisitionKey {
            subject: "02".to_string(),
            session: Some("post".to_string()),
            acquisition: Some("gre".to_string()),
            reconstruction: None,
            inversion: None,
            run: Some("1".to_string()),
            suffix: "MEGRE".to_string(),
        };
        let path = output().qsm_path(&key);
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.contains("sub-02"));
        assert!(name.contains("ses-post"));
        assert!(name.contains("acq-gre"));
        assert!(name.contains("run-1"));
        assert!(name.ends_with("_Chimap.nii"));
    }
}
