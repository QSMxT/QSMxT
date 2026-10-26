//! The combined magnitude, and the inhomogeneity (bias field) correction masking also uses.

use crate::config::PipelineConfig;
use crate::enums::InhomogeneityMethod;
use crate::error::EngineError;
use crate::scan::RunMetadata;

/// The `magnitude` step's parameters: the correction's method and that method's settings.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    match config.masking.inhomogeneity_method {
        InhomogeneityMethod::Makehomogeneous => serde_json::json!({
            "inhomogeneity_correction": config.masking.inhomogeneity_correction,
            "homogeneity_sigma_mm": config.homogeneity.sigma_mm,
            "homogeneity_nbox": config.homogeneity.nbox,
        }),
        InhomogeneityMethod::N4 => serde_json::json!({
            "inhomogeneity_correction": config.masking.inhomogeneity_correction,
            "inhomogeneity_method": "n4",
            "n4": config.n4,
        }),
    }
}

/// The run's combined magnitude: the root sum of squares of every echo's, corrected for
/// inhomogeneity when `--inhomogeneity-correction` is on.
pub fn combined_magnitude(
    echoes: &[&[f64]], meta: &RunMetadata, config: &PipelineConfig,
) -> Result<Vec<f64>, EngineError> {
    let combined = qsm_core::pipeline::rss_combine(echoes)?;
    if !config.masking.inhomogeneity_correction {
        return Ok(combined);
    }
    homogeneity_correct(&combined, meta, config, "the RSS-combined magnitude")
}

/// Apply inhomogeneity (bias field) correction on the run's working grid, with the configured
/// method (`masking.inhomogeneity_method`). `source` names the volume in the error.
///
/// qsm-core indexes the volume as `x + y*nx + z*nx*ny` over the grid dimensions, so a volume
/// with fewer voxels than the grid panics with an out-of-bounds index inside the box filter
/// rather than reporting which input is wrong (issue #184). `qsmxt` checks every input's size at
/// load time for a fresh run; this guard also covers runs resumed from a cached state.
pub fn homogeneity_correct(
    data: &[f64],
    meta: &RunMetadata,
    config: &PipelineConfig,
    source: &str,
) -> Result<Vec<f64>, EngineError> {
    let (nx, ny, nz) = meta.dims;
    let (vsx, vsy, vsz) = meta.voxel_size;
    if data.len() != nx * ny * nz {
        return Err(EngineError::DimensionMismatch(format!(
            concat!(
                "inhomogeneity correction: {} has {} voxels but this run is processed on a ",
                "{}x{}x{} grid ({} voxels) taken from its first phase echo. Check that the ",
                "magnitude and phase come from the same acquisition, or pass ",
                "--no-inhomogeneity-correction to skip this step",
            ),
            source, data.len(), nx, ny, nz, nx * ny * nz,
        )));
    }
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    match config.masking.inhomogeneity_method {
        InhomogeneityMethod::Makehomogeneous => {
            let h = &config.homogeneity;
            Ok(qsm_core::utils::makehomogeneous(data, &grid, h.sigma_mm, h.nbox)?)
        }
        InhomogeneityMethod::N4 => {
            // The plan refuses settings and grids N4 cannot use up front; this covers runs
            // resumed from a cached state. No mask: the corrected magnitude is what masking is
            // made from.
            qsm_core::utils::n4_bias_correction(data, None, &grid, &config.n4.core(), |_, _| {})
                .map(|out| out.corrected)
                .map_err(|e| EngineError::Config(format!("inhomogeneity correction: {e}")))
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn meta_4x4x4() -> RunMetadata {
        RunMetadata {
            dims: (4, 4, 4), voxel_size: (1.0, 1.0, 1.0),
            affine: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            n_echoes: 1, echo_times: vec![0.004], b0_direction: (0.0, 0.0, 1.0),
            field_strength: 3.0, has_magnitude: true,
            source_geometry: None, slice_geometry: None,
        }
    }

    #[test]
    fn test_homogeneity_correct_rejects_size_mismatch() {
        // The guard must report the mismatch rather than let qsm-core index past the volume.
        let cfg = PipelineConfig::default();
        let short = vec![1.0f64; 4 * 4 * 3];
        let err = homogeneity_correct(&short, &meta_4x4x4(), &cfg, "the magnitude").unwrap_err();
        assert!(matches!(err, EngineError::DimensionMismatch(_)));
        let msg = format!("{}", err);
        assert!(msg.contains("48 voxels"), "got: {}", msg);
        assert!(msg.contains("4x4x4"), "got: {}", msg);
        assert!(msg.contains("--no-inhomogeneity-correction"), "got: {}", msg);
    }

    #[test]
    fn test_homogeneity_correct_runs_on_matching_volume() {
        let cfg = PipelineConfig::default();
        let data: Vec<f64> = (0..4 * 4 * 4).map(|i| 100.0 + i as f64).collect();
        let out = homogeneity_correct(&data, &meta_4x4x4(), &cfg, "the magnitude").unwrap();
        assert_eq!(out.len(), data.len());
        assert!(out.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn the_combined_magnitude_is_corrected_only_when_asked() {
        let meta = meta_4x4x4();
        let a: Vec<f64> = (0..64).map(|i| 100.0 + i as f64).collect();
        let b: Vec<f64> = (0..64).map(|i| 50.0 + i as f64).collect();
        let rss = qsm_core::pipeline::rss_combine(&[&a, &b]).unwrap();

        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_correction = false;
        assert_eq!(combined_magnitude(&[&a, &b], &meta, &config).unwrap(), rss);

        config.masking.inhomogeneity_correction = true;
        let corrected = homogeneity_correct(&rss, &meta, &config, "x").unwrap();
        assert_eq!(combined_magnitude(&[&a, &b], &meta, &config).unwrap(), corrected);
    }

    /// The method decides which correction runs, and N4's own settings reach it.
    #[test]
    fn the_method_chooses_the_correction() {
        let mut meta = meta_4x4x4();
        meta.dims = (8, 8, 8);
        let data: Vec<f64> = (0..512).map(|i| 100.0 + (i % 37) as f64).collect();
        let mut config = PipelineConfig::default();
        let made = homogeneity_correct(&data, &meta, &config, "x").unwrap();
        config.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        config.n4.max_iterations = vec![2];
        let grid = qsm_core::Grid::new(8, 8, 8, 1.0, 1.0, 1.0);
        let n4 = qsm_core::utils::n4_bias_correction(&data, None, &grid, &config.n4.core(), |_, _| {}).unwrap().corrected;
        assert_eq!(homogeneity_correct(&data, &meta, &config, "x").unwrap(), n4);
        assert_ne!(n4, made);
        assert_eq!(params(&config)["inhomogeneity_method"], "n4");
        assert_eq!(params(&config)["n4"]["max_iterations"], serde_json::json!([2]));
    }

    /// A grid N4 cannot shrink, and no fitting level, are errors rather than qsm-core panics.
    #[test]
    fn n4_refuses_what_qsm_core_would_panic_on() {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        let data = vec![1.0f64; 64];
        let err = homogeneity_correct(&data, &meta_4x4x4(), &config, "x").unwrap_err().to_string();
        assert!(err.contains("shrink_factor = 4") && err.contains("4x4x4"), "{err}");
        config.n4.shrink_factor = 2;
        assert!(homogeneity_correct(&data, &meta_4x4x4(), &config, "x").is_ok());
        config.n4.max_iterations.clear();
        let err = homogeneity_correct(&data, &meta_4x4x4(), &config, "x").unwrap_err().to_string();
        assert!(err.contains("N4 max_iterations = []"), "{err}");
    }
}
