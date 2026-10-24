//! Background field removal: the total field → the local field, and the mask it is defined on.

use crate::config::PipelineConfig;
use crate::enums::BfAlgorithm;
use crate::error::EngineError;
use crate::observe::Observer;
use crate::scan::{scan_metadata, scan_metadata_in, RunMetadata};
use crate::stages::crop::crop_box_for;

/// The background-removal step's parameters (the algorithm is recorded beside them): the method's
/// settings serialised whole, so that a setting added to its config is recorded (and keyed)
/// without anyone listing it here.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    fn all<T: serde::Serialize>(settings: &T) -> serde_json::Value {
        serde_json::to_value(settings).unwrap_or_default()
    }
    let bg = &config.bg_removal;
    match bg.algorithm {
        BfAlgorithm::Vsharp => all(&bg.vsharp),
        BfAlgorithm::Pdf => all(&bg.pdf),
        BfAlgorithm::Lbv => all(&bg.lbv),
        BfAlgorithm::Ismv => all(&bg.ismv),
        BfAlgorithm::Sharp => all(&bg.sharp),
        BfAlgorithm::Resharp => all(&bg.resharp),
        BfAlgorithm::Harperella => all(&bg.harperella),
        BfAlgorithm::Iharperella => all(&bg.iharperella),
        // BFRnet / iQFM (deep learning) have no network parameters; a tiled run is a different
        // result from a whole-volume one, so the tiling is in the key.
        BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => match config.dl_tile() {
            Some((core, halo)) => serde_json::json!({ "tile": [core, halo] }),
            None => serde_json::json!({}),
        },
    }
}

/// Whether the configured background removal takes every echo's magnitude: PDF, whose data
/// weight is the noise map qsm-core makes from them (MEDI toolbox `Fit_ppm_complex`).
pub fn takes_echo_magnitudes(config: &PipelineConfig) -> bool {
    config.bg_removal.algorithm == BfAlgorithm::Pdf
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
/// `magnitudes` is every echo's magnitude, stacked with the echo slowest, for the method that
/// takes them ([`takes_echo_magnitudes`]), and `None` otherwise.
///
/// Runs in the reconstruction box ([`crop_box_for`]) and returns to the full grid: the local
/// field and the eroded mask are both undefined outside the box, so zero is the right fill for
/// each.
pub fn remove_background(
    field_ppm: &[f64], mask: &[u8], magnitudes: Option<&[f64]>, meta: &RunMetadata,
    config: &PipelineConfig, observer: &mut dyn Observer,
) -> Result<LocalField, EngineError> {
    let (_, bg_config, _, _) = crate::bridge::to_pipeline_stages(config);
    let cb = crop_box_for(config, meta, mask, "Background removal");
    let magnitudes = magnitudes.map(|m| crate::stages::inversion::crop_stack(m, mask.len(), &cb));
    let result = qsm_core::pipeline::run_bg_removal(
        &qsm_core::crop::crop_volume(field_ppm, &cb),
        &qsm_core::crop::crop_volume(mask, &cb),
        &scan_metadata_in(meta, &cb), &bg_config, magnitudes.as_deref(),
        &mut |done, total| observer.progress(done, total),
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
    config: &PipelineConfig,
) -> Result<LocalField, EngineError> {
    log::info!("Background removal (iQFM, deep-learning joint unwrap+BFR)");
    let local_field_ppm = qsm_core::pipeline::run_iqfm(phases, magnitudes, mask, &scan_metadata(meta), config.dl_tile())
        .map_err(|e| EngineError::Config(format!("iQFM: {}", e)))?;
    Ok(LocalField { local_field_ppm, mask: mask.to_vec() })
}
