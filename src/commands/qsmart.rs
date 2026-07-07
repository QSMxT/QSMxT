//! Standalone QSMART reconstruction: total field (ppm) -> susceptibility (ppm).
//!
//! QSMART is a two-stage, vessel-aware method that performs its own background field removal, so it
//! consumes the TOTAL field rather than a local field. It is not a plain `invert`/`bgremove` step,
//! hence its own top-level command. This mirrors the `qsmxt run` QSMART stage on NIfTI in/out.

use log::info;

use super::common::{load_mask, load_nifti, nifti_grid, save_nifti};
use crate::cli::QsmartArgs;
use crate::error::QsmxtError;
use crate::pipeline::config;

pub fn execute(args: QsmartArgs) -> crate::Result<()> {
    let field = load_nifti(&args.input)?;
    let (mask, _) = load_mask(&args.mask)?;
    let grid = nifti_grid(&field);
    let dims = (grid.nx(), grid.ny(), grid.nz());
    let voxel_size = (grid.vsx(), grid.vsy(), grid.vsz());
    let b0 = (args.b0_direction[0], args.b0_direction[1], args.b0_direction[2]);
    info!("QSMART ({}x{}x{})", dims.0, dims.1, dims.2);

    // Lower a QSMART pipeline config to the qsm-core inversion config, exactly like `qsmxt run`.
    let mut cfg = config::PipelineConfig::default();
    cfg.inversion.algorithm = config::QsmAlgorithm::Qsmart;
    config::apply_qsmart_overrides(&mut cfg, &args.qsmart_params);
    let (_, _, mut inv_config, _) = config::to_pipeline_stages(&cfg);

    // Vasculature sphere radius + Frangi vessel scales are configured in mm; convert to voxels
    // using the dataset voxel size (matches the pipeline runner).
    {
        let (vsx, vsy, vsz) = voxel_size;
        let avg = (vsx + vsy + vsz) / 3.0;
        let q = &mut inv_config.qsmart;
        q.vasc_sphere_radius = (((q.vasc_sphere_radius as f64) / avg).round() as i32).max(2);
        q.frangi_scale_range = [q.frangi_scale_range[0] / avg, q.frangi_scale_range[1] / avg];
        q.frangi_scale_ratio = (q.frangi_scale_ratio / avg).max(0.1);
    }

    let scan_meta =
        config::to_scan_metadata(dims, voxel_size, &[args.echo_time], args.field_strength, b0);

    let magnitude: Option<Vec<f64>> = match &args.magnitude {
        Some(p) => Some(load_nifti(p)?.data),
        None => None,
    };

    let mut progress = |_: usize, _: usize| {};
    let chi = qsm_core::pipeline::run_qsmart(
        &field.data,
        &mask,
        magnitude.as_deref(),
        &scan_meta,
        &inv_config,
        qsm_core::pipeline::QsmReference::None,
        &mut progress,
    )
    .map_err(|e| QsmxtError::Config(format!("qsmart: {}", e)))?;

    save_nifti(&args.output, &chi, &field)?;
    info!("Susceptibility map saved to {}", args.output.display());
    Ok(())
}
