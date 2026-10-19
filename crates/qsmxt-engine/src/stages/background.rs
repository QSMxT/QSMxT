//! Background field removal: the total field → the local field, and the mask it is defined on.

use crate::config::PipelineConfig;
use crate::enums::BfAlgorithm;
use crate::error::EngineError;
use crate::observe::Observer;
use crate::scan::{scan_metadata, scan_metadata_in, RunMetadata};
use crate::stages::crop::crop_box_for;

/// The background-removal step's parameters (the algorithm is recorded beside them).
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    let bg = &config.bg_removal;
    match bg.algorithm {
        BfAlgorithm::Vsharp => serde_json::json!({
            "deconvolve": bg.vsharp.deconvolve,
            "max_radius": bg.vsharp.max_radius,
            "min_radius": bg.vsharp.min_radius,
            "threshold": bg.vsharp.threshold,
            "grid": bg.vsharp.grid.to_string(),
            "grid_spacing": bg.vsharp.grid_spacing,
        }),
        BfAlgorithm::Pdf => serde_json::json!({ "tol": bg.pdf.tol }),
        BfAlgorithm::Lbv => serde_json::json!({ "tol": bg.lbv.tol }),
        BfAlgorithm::Ismv => serde_json::json!({
            "radius": bg.ismv.radius,
            "tol": bg.ismv.tol,
            "max_iter": bg.ismv.max_iter,
        }),
        BfAlgorithm::Sharp => serde_json::json!({
            "radius": bg.sharp.radius,
            "threshold": bg.sharp.threshold,
        }),
        BfAlgorithm::Resharp => serde_json::json!({
            "radius": bg.resharp.radius,
            "tik_reg": bg.resharp.tik_reg,
            "tol": bg.resharp.tol,
            "max_iter": bg.resharp.max_iter,
        }),
        BfAlgorithm::Harperella => serde_json::json!({
            "radius": bg.harperella.radius,
            "max_iter": bg.harperella.max_iter,
            "tol": bg.harperella.tol,
        }),
        BfAlgorithm::Iharperella => serde_json::json!({
            "max_iter": bg.iharperella.max_iter,
            "tol": bg.iharperella.tol,
        }),
        // BFRnet / iQFM (deep learning) have no user-tunable parameters; cache key is the weights.
        BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => serde_json::json!({}),
    }
}

/// A local field and the mask it is defined on.
pub struct LocalField {
    /// The local (tissue) field in ppm, zero outside `mask`.
    pub local_field_ppm: Vec<f64>,
    /// Where the local field is defined: the mask background removal eroded (by its kernel
    /// radius, for the SMV methods), or the brain mask for a method that keeps the edge.
    pub mask: Vec<u8>,
}

/// Remove the background field from the total field `field_ppm` (ppm) inside the brain `mask`,
/// with the configured algorithm (any but iQFM, which starts from phase: see [`iqfm`]).
///
/// Runs in the reconstruction box ([`crop_box_for`]) and returns to the full grid: the local
/// field and the eroded mask are both undefined outside the box, so zero is the right fill for
/// each.
pub fn remove_background(
    field_ppm: &[f64], mask: &[u8], meta: &RunMetadata, config: &PipelineConfig,
    observer: &mut dyn Observer,
) -> Result<LocalField, EngineError> {
    let (_, bg_config, _, _) = crate::bridge::to_pipeline_stages(config);
    let cb = crop_box_for(config, meta, mask, "Background removal");
    let result = qsm_core::pipeline::run_bg_removal(
        &qsm_core::crop::crop_volume(field_ppm, &cb),
        &qsm_core::crop::crop_volume(mask, &cb),
        &scan_metadata_in(meta, &cb), &bg_config, &mut |done, total| observer.progress(done, total),
    ).map_err(|e| EngineError::Config(format!("bg removal: {}", e)))?;
    Ok(LocalField {
        local_field_ppm: qsm_core::crop::uncrop_volume(&result.local_field_ppm, &cb, 0.0),
        mask: qsm_core::crop::uncrop_volume(&result.eroded_mask, &cb, 0u8),
    })
}

/// iQFM: the local field straight from every echo's wrapped phase (joint unwrapping and
/// background removal in one network), inside the brain `mask`. `magnitudes` may be empty
/// (uniform weights). iQFM keeps the brain edge, so the local field's mask is the brain mask.
pub fn iqfm(
    phases: &[&[f64]], magnitudes: &[&[f64]], mask: &[u8], meta: &RunMetadata,
) -> Result<LocalField, EngineError> {
    log::info!("Background removal (iQFM, deep-learning joint unwrap+BFR)");
    let local_field_ppm = qsm_core::pipeline::run_iqfm(phases, magnitudes, mask, &scan_metadata(meta), None)
        .map_err(|e| EngineError::Config(format!("iQFM: {}", e)))?;
    Ok(LocalField { local_field_ppm, mask: mask.to_vec() })
}
