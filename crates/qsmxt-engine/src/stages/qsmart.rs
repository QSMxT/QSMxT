//! QSMART: the two-stage reconstruction from the total field, with its intermediates.

use crate::config::PipelineConfig;
use crate::enums::{B0Estimation, VesselSizeUnits};
use crate::error::EngineError;
use crate::observe::{Core, Observer, VolumeData};
use crate::scan::{scan_metadata, RunMetadata};

/// The intermediates [`qsmart`] returns, by name, in the order they are produced: the vessel
/// mask, then the stage-1 and stage-2 local fields and susceptibility maps. See
/// [`qsm_core::pipeline::qsmart::outputs`] for what each holds.
pub use qsm_core::pipeline::qsmart::outputs;

/// QSMxT's name for the magnitude QSMART detected vessels on, kept beside QSMART's own
/// intermediates ([`outputs`]) when it is QSMART's vessel magnitude rather than the combined one.
pub const VESSEL_MAGNITUDE: &str = "vessel_magnitude";

/// The QSMART step's parameters.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    let q = &config.inversion.qsmart;
    serde_json::json!({
        "inversion": format!("{}", q.inversion),
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
        "vessel_size_units": format!("{}", q.vessel_size_units),
        "vessel_magnitude": format!("{}", q.vessel_magnitude),
        // Both stages run inside the echo fit's reliability map when the field came from it.
        "reliability_map": config.field_mapping.b0_estimation == B0Estimation::QsmartEchofit,
    })
}

/// qsm-core's inversion configuration for QSMART on this run.
///
/// qsm-core takes the vasculature sphere radius and the Frangi vessel scales in voxels. Given in
/// voxels (`vessel_size_units`, the default) they are passed as they are; given in mm they are
/// converted with the mean voxel size, the sphere radius rounded and kept at 2 voxels or more, the
/// scale step at 0.1 voxels or more.
pub fn inversion_config(config: &PipelineConfig, meta: &RunMetadata) -> qsm_core::pipeline::InversionConfig {
    vessel_sizes_in_voxels(config, meta.voxel_size)
}

/// [`inversion_config`] for a grid of `voxel_size` (mm): what `qsmxt qsmart` takes too.
pub fn vessel_sizes_in_voxels(config: &PipelineConfig, voxel_size: (f64, f64, f64)) -> qsm_core::pipeline::InversionConfig {
    let (_, _, mut inv_config, _) = crate::bridge::to_pipeline_stages(config);
    if config.inversion.qsmart.vessel_size_units == VesselSizeUnits::Mm {
        let (vsx, vsy, vsz) = voxel_size;
        let avg = (vsx + vsy + vsz) / 3.0;
        let q = &mut inv_config.qsmart;
        q.vasc_sphere_radius = (((q.vasc_sphere_radius as f64) / avg).round() as i32).max(2);
        q.frangi_scale_range = [q.frangi_scale_range[0] / avg, q.frangi_scale_range[1] / avg];
        q.frangi_scale_ratio = (q.frangi_scale_ratio / avg).max(0.1);
    }
    inv_config
}

/// The magnitude QSMART is given to detect vessels on (`inversion.qsmart.vessel_magnitude`).
pub enum Magnitude {
    /// Every echo's magnitude (none for a phase-only run), from which QSMART's own vessel
    /// magnitude is made ([`qsm_core::utils::qsmart_vessel_magnitude`]).
    Echoes(Vec<Vec<f64>>),
    /// The run's combined magnitude, when there is one.
    Combined(Option<Vec<f64>>),
}

/// What QSMART produced.
pub struct Qsmart {
    /// The raw (unreferenced) susceptibility map in ppm: referencing is left to the referencing
    /// stage.
    pub chi: Vec<f64>,
    /// Its intermediates, by their names in [`outputs`] (and [`VESSEL_MAGNITUDE`]), in the order
    /// they were produced.
    pub intermediates: Vec<(&'static str, VolumeData)>,
}

/// QSMART's two-stage reconstruction of the total field `field_ppm` (ppm) inside the brain `mask`
/// ([`qsm_core::pipeline::run_qsmart`]), and inside `reliability`, the echo fit's reliability map,
/// when the field came from QSMART's echo fit. `magnitude` drives vasculature detection (and
/// MEDI's edge weighting when that is the inner inversion); uniform when there is none.
///
/// `observer` gets progress (step of 6). The intermediates come back with the map rather than
/// through it, so that keeping them is the caller's, and a failure to keep one is the caller's
/// error to return.
pub fn qsmart(
    field_ppm: &[f64], mask: &[u8], reliability: Option<&[u8]>, magnitude: Magnitude, meta: &RunMetadata,
    config: &PipelineConfig, observer: &mut dyn Observer,
) -> Result<Qsmart, EngineError> {
    let q = &config.inversion.qsmart;
    log::info!(
        "QSMART (inversion={}, ilsqr tol={:.0e}, max_iter={}, pad={} mm, vasc_radius={} {}, vessel magnitude={}{})",
        q.inversion, config.inversion.ilsqr.tol, q.ilsqr_max_iter, q.ilsqr_pad_mm,
        q.vasc_sphere_radius, q.vessel_size_units, q.vessel_magnitude,
        if reliability.is_some() { ", inside the echo fit's reliability map" } else { "" },
    );
    let metadata = scan_metadata(meta);
    let (magnitude, own) = match magnitude {
        Magnitude::Echoes(echoes) if !echoes.is_empty() => {
            let refs: Vec<&[f64]> = echoes.iter().map(|e| e.as_slice()).collect();
            let m = qsm_core::utils::qsmart_vessel_magnitude(&refs, &metadata.grid())
                .map_err(|e| EngineError::Config(format!("qsmart: {e}")))?;
            (Some(m), true)
        }
        Magnitude::Echoes(_) => (None, false),
        Magnitude::Combined(m) => (m, false),
    };
    let mut core = Core::new(observer);
    let chi = qsm_core::pipeline::run_qsmart(
        field_ppm, mask, reliability, magnitude.as_deref(), &metadata, &inversion_config(config, meta),
        qsm_core::pipeline::QsmReference::None, &mut core,
    ).map_err(|e| EngineError::Config(format!("qsmart: {}", e)))?;
    let mut intermediates = core.outputs;
    // QSMART's own vessel magnitude is made first, so it comes first.
    if let (true, Some(m)) = (own, magnitude) {
        intermediates.insert(0, (VESSEL_MAGNITUDE, VolumeData::Real(m)));
    }
    Ok(Qsmart { chi, intermediates })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vessel_sizes_are_converted_from_mm_to_voxels() {
        let mut meta = crate::stages::magnitude::tests::meta_4x4x4();
        meta.voxel_size = (0.5, 0.5, 0.5);
        let mut config = PipelineConfig::default();
        config.inversion.qsmart.vessel_size_units = VesselSizeUnits::Mm;
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

    /// In voxels (the default) the sizes reach qsm-core as they are, whatever the voxel size.
    #[test]
    fn vessel_sizes_in_voxels_are_passed_as_they_are() {
        let mut meta = crate::stages::magnitude::tests::meta_4x4x4();
        meta.voxel_size = (0.5, 0.7, 2.0);
        let config = PipelineConfig::default();
        let q = inversion_config(&config, &meta).qsmart;
        let d = qsm_core::utils::QsmartParams::default();
        assert_eq!((q.vasc_sphere_radius, q.frangi_scale_range, q.frangi_scale_ratio),
                   (d.vasc_sphere_radius, d.frangi_scale_range, d.frangi_scale_ratio));
        assert_eq!(params(&config)["vessel_size_units"], "voxels");
        assert_eq!(params(&config)["vessel_magnitude"], "mean-n4");
    }
}
