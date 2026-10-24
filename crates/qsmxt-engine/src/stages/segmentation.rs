//! Whole-brain parcellation of the combined magnitude by SynthSeg.
//!
//! SynthSeg is trained on synthetic images with randomised contrast, which is what lets one set of
//! weights segment a GRE magnitude — so susceptibility can be reported per structure without
//! acquiring and registering a separate T1w. It takes no brain mask: it finds the brain itself.

use crate::config::{PipelineConfig, SegmentationConfig};
use crate::enums::SynthSegVersion;
use crate::error::EngineError;
use crate::scan::RunMetadata;

/// The `segmentation` step's parameters. A sliding window's `patch` is there only when one is
/// set, so a whole-volume run keeps the cache key it had before the setting existed.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    let c = &config.segmentation;
    let mut params = serde_json::json!({
        "version": format!("{}", c.version),
        "crop": c.crop,
        "flip_averaging": c.flip_averaging,
        "topology_cleanup": c.topology_cleanup,
        "sigma_smoothing": c.sigma_smoothing,
        "custom_dseg_tool": c.custom_dseg_tool,
    });
    if let Some(patch) = c.patch {
        params["patch"] = patch.into();
    }
    params
}

/// Whether `version`'s weights can be had (published in QSM.rs's registry, or present locally),
/// or why not. A run is refused up front rather than per scan, or running one version's weights
/// through another's label table (#264).
pub fn weights_available(version: SynthSegVersion) -> Result<(), EngineError> {
    let id = version.weights_id();
    let spec = qsm_core::models::find_model(id).ok_or_else(|| EngineError::Config(format!(
        "SynthSeg {version}: QSM.rs has no model '{id}'")))?;
    if spec.is_available() || spec.files.iter().all(qsm_core::models::weight_file_present) {
        return Ok(());
    }
    Err(EngineError::Config(format!(
        "SynthSeg {version} is not available yet: its weights ({}) are not published in QSM.rs's \
         model registry. Use --synthseg-version v1, or put the exported file in $QSM_MODEL_DIR",
        spec.files.iter().map(|f| f.name).collect::<Vec<_>>().join(", "))))
}

/// The BIDS lookup table pairing each of `version`'s label integers with its name.
pub fn lookup_table(version: SynthSegVersion) -> String {
    let labels = version.to_core().labels();
    let mut out = String::from("index\tname\n");
    for (id, name) in labels.ids.iter().zip(labels.names) {
        out.push_str(&format!("{id}\t{name}\n"));
    }
    out
}

/// A parcellation: FreeSurfer label ids per voxel, and SynthSeg's per-label volumes (mm³, from
/// its posteriors, in label-table order).
pub struct Segmentation {
    pub labels: Vec<f64>,
    pub volumes: Vec<f64>,
}

/// Segment the combined `magnitude` with SynthSeg, whose weights must be ready.
#[cfg(feature = "onnx")]
pub fn synthseg(
    magnitude: &[f64], meta: &RunMetadata, config: &SegmentationConfig,
    observer: &mut dyn crate::observe::Observer,
) -> Result<Segmentation, EngineError> {
    let ((nx, ny, nz), (vx, vy, vz)) = (meta.dims, meta.voxel_size);
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    let params = qsm_core::segment::SynthSegParams {
        version: config.version.to_core(),
        crop: config.crop,
        flip_averaging: config.flip_averaging,
        topology_cleanup: config.topology_cleanup,
        sigma_smoothing: config.sigma_smoothing,
        inference: crate::config::synthseg_inference(config.patch),
        // The head crop and the posteriors' precision are not QSMxT settings yet: qsm-core's
        // (no crop, single precision).
        ..Default::default()
    };
    let weights = qsm_core::models::primary_weight(config.version.weights_id())
        .map_err(|e| EngineError::Config(format!("synthseg weights: {}", e)))?;
    let out = qsm_core::segment::synthseg(magnitude, &grid, &meta.affine, &weights, &params,
                                          |done, total| observer.progress(done, total))
        .map_err(|e| EngineError::Config(format!("synthseg: {}", e)))?;
    // FreeSurfer ids are integers; they travel as f64 exactly (all are < 2^53).
    Ok(Segmentation { labels: out.labels.iter().map(|&l| l as f64).collect(), volumes: out.volumes })
}

/// Without the `onnx` feature there is no network to run.
#[cfg(not(feature = "onnx"))]
pub fn synthseg(
    _magnitude: &[f64], _meta: &RunMetadata, _config: &SegmentationConfig,
    _observer: &mut dyn crate::observe::Observer,
) -> Result<Segmentation, EngineError> {
    Err(EngineError::Config(
        "segmentation needs a deep-learning build: SynthSeg runs an ONNX network, which this \
         binary was compiled without (--no-default-features)".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookup_table_names_every_label() {
        let table = lookup_table(SynthSegVersion::V2);
        let labels = SynthSegVersion::V2.to_core().labels();
        assert_eq!(table.lines().count(), labels.ids.len() + 1);
        assert!(table.starts_with("index\tname\n0\t"), "{table}");
    }
}
