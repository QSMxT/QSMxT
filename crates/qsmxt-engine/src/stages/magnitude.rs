//! The combined magnitude, and the inhomogeneity (bias field) correction masking also uses.

use crate::config::{HomogeneityConfig, PipelineConfig};
use crate::error::EngineError;
use crate::scan::RunMetadata;

/// The `magnitude` step's parameters.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    serde_json::json!({
        "inhomogeneity_correction": config.masking.inhomogeneity_correction,
        "homogeneity_sigma_mm": config.homogeneity.sigma_mm,
        "homogeneity_nbox": config.homogeneity.nbox,
    })
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
    homogeneity_correct(&combined, meta, &config.homogeneity, "the RSS-combined magnitude")
}

/// Apply inhomogeneity (bias field) correction on the run's working grid. `source` names the
/// volume in the error.
///
/// qsm-core indexes the volume as `x + y*nx + z*nx*ny` over the grid dimensions, so a volume
/// with fewer voxels than the grid panics with an out-of-bounds index inside the box filter
/// rather than reporting which input is wrong (issue #184). `qsmxt` checks every input's size at
/// load time for a fresh run; this guard also covers runs resumed from a cached state.
pub fn homogeneity_correct(
    data: &[f64],
    meta: &RunMetadata,
    homogeneity: &HomogeneityConfig,
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
    Ok(qsm_core::utils::makehomogeneous(data, &grid, homogeneity.sigma_mm, homogeneity.nbox)?)
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
        let cfg = HomogeneityConfig::default();
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
        let cfg = HomogeneityConfig::default();
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
        let corrected = homogeneity_correct(&rss, &meta, &config.homogeneity, "x").unwrap();
        assert_eq!(combined_magnitude(&[&a, &b], &meta, &config).unwrap(), corrected);
    }
}
