//! QSMART: the two-stage reconstruction from the total field, with its intermediates.

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::observe::{Core, Observer, VolumeData};
use crate::scan::{scan_metadata, RunMetadata};

/// The intermediates [`qsmart`] returns, by name, in the order they are produced: the vessel
/// mask, then the stage-1 and stage-2 local fields and susceptibility maps. See
/// [`qsm_core::pipeline::qsmart::outputs`] for what each holds.
pub use qsm_core::pipeline::qsmart::outputs;

/// The QSMART step's parameters.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    let q = &config.inversion.qsmart;
    serde_json::json!({
        "inversion": format!("{}", q.inversion),
        "sdf_spatial_radius": q.sdf_spatial_radius,
        "vasc_sphere_radius": q.vasc_sphere_radius,
        // The inner iLSQR's tolerance is the iLSQR stage's.
        "ilsqr_tol": config.inversion.ilsqr.tol,
        "ilsqr_max_iter": q.ilsqr_max_iter,
        "ilsqr_pad_mm": q.ilsqr_pad_mm,
        "sdf_sigma1_stage1": q.sdf_sigma1_stage1,
        "sdf_sigma2_stage1": q.sdf_sigma2_stage1,
        "sdf_sigma1_stage2": q.sdf_sigma1_stage2,
        "sdf_sigma2_stage2": q.sdf_sigma2_stage2,
        "sdf_lower_lim": q.sdf_lower_lim,
        "sdf_curv_constant": q.sdf_curv_constant,
        "frangi_scale_min": q.frangi_scale_min,
        "frangi_scale_max": q.frangi_scale_max,
        "frangi_scale_ratio": q.frangi_scale_ratio,
        "frangi_c": q.frangi_c,
    })
}

/// qsm-core's inversion configuration for QSMART on this run.
///
/// The vasculature sphere radius and the Frangi vessel scales are configured in mm (as qsmbly
/// shows them); qsm-core takes voxels, so they are converted with the mean voxel size. The sphere
/// radius is rounded and kept at 2 voxels or more, the scale ratio at 0.1 voxels or more.
pub fn inversion_config(config: &PipelineConfig, meta: &RunMetadata) -> qsm_core::pipeline::InversionConfig {
    let (_, _, mut inv_config, _) = crate::bridge::to_pipeline_stages(config);
    let (vsx, vsy, vsz) = meta.voxel_size;
    let avg = (vsx + vsy + vsz) / 3.0;
    let q = &mut inv_config.qsmart;
    q.vasc_sphere_radius = (((q.vasc_sphere_radius as f64) / avg).round() as i32).max(2);
    q.frangi_scale_range = [q.frangi_scale_range[0] / avg, q.frangi_scale_range[1] / avg];
    q.frangi_scale_ratio = (q.frangi_scale_ratio / avg).max(0.1);
    inv_config
}

/// What QSMART produced.
pub struct Qsmart {
    /// The raw (unreferenced) susceptibility map in ppm: referencing is left to the referencing
    /// stage.
    pub chi: Vec<f64>,
    /// Its intermediates, by their names in [`outputs`], in the order they were produced.
    pub intermediates: Vec<(&'static str, VolumeData)>,
}

/// QSMART's two-stage reconstruction of the total field `field_ppm` (ppm) inside the brain `mask`
/// ([`qsm_core::pipeline::run_qsmart`]). `magnitude` is the combined magnitude, which drives
/// vasculature detection (and MEDI's edge weighting when that is the inner inversion); uniform
/// when `None`.
///
/// `observer` gets progress (step of 6). The intermediates come back with the map rather than
/// through it, so that keeping them is the caller's, and a failure to keep one is the caller's
/// error to return.
pub fn qsmart(
    field_ppm: &[f64], mask: &[u8], magnitude: Option<&[f64]>, meta: &RunMetadata,
    config: &PipelineConfig, observer: &mut dyn Observer,
) -> Result<Qsmart, EngineError> {
    let q = &config.inversion.qsmart;
    log::info!(
        "QSMART (inversion={}, ilsqr tol={:.0e}, max_iter={}, pad={} mm, vasc_radius={}, sdf_radius={})",
        q.inversion, config.inversion.ilsqr.tol, q.ilsqr_max_iter, q.ilsqr_pad_mm,
        q.vasc_sphere_radius, q.sdf_spatial_radius,
    );
    let mut core = Core::new(observer);
    let chi = qsm_core::pipeline::run_qsmart(
        field_ppm, mask, magnitude, &scan_metadata(meta), &inversion_config(config, meta),
        qsm_core::pipeline::QsmReference::None, &mut core,
    ).map_err(|e| EngineError::Config(format!("qsmart: {}", e)))?;
    Ok(Qsmart { chi, intermediates: core.outputs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vessel_sizes_are_converted_from_mm_to_voxels() {
        let mut meta = crate::stages::magnitude::tests::meta_4x4x4();
        meta.voxel_size = (0.5, 0.5, 0.5);
        let mut config = PipelineConfig::default();
        config.inversion.qsmart.vasc_sphere_radius = 4;
        config.inversion.qsmart.frangi_scale_min = 0.5;
        config.inversion.qsmart.frangi_scale_max = 6.0;
        config.inversion.qsmart.frangi_scale_ratio = 0.5;
        let q = inversion_config(&config, &meta).qsmart;
        assert_eq!(q.vasc_sphere_radius, 8, "4 mm at 0.5 mm voxels");
        assert_eq!(q.frangi_scale_range, [1.0, 12.0]);
        assert_eq!(q.frangi_scale_ratio, 1.0);

        // Large voxels: the radius and ratio keep their floors.
        meta.voxel_size = (10.0, 10.0, 10.0);
        let q = inversion_config(&config, &meta).qsmart;
        assert_eq!(q.vasc_sphere_radius, 2);
        assert_eq!(q.frangi_scale_ratio, 0.1);
    }
}
