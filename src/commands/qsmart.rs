//! Standalone QSMART reconstruction: total field (ppm) -> susceptibility (ppm).
//!
//! QSMART is a two-stage, vessel-aware method that performs its own background field removal, so it
//! consumes the TOTAL field rather than a local field. It is not a plain `invert`/`bgremove` step,
//! hence its own top-level command. This mirrors the `qsmxt run` QSMART stage on NIfTI in/out.

use log::info;

use super::common::{load_mask, load_nifti, load_nifti_4d, nifti_grid, save_nifti};
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
    // The vessel sizes in voxels, converted from mm when given in mm, as `qsmxt run` does.
    let inv_config = qsmxt_engine::stages::qsmart::vessel_sizes_in_voxels(&cfg, voxel_size);

    let scan_meta =
        config::to_scan_metadata(dims, voxel_size, &[args.echo_time], args.field_strength, b0, Some(field.affine));

    let magnitude: Option<Vec<f64>> = match (&args.magnitude, cfg.inversion.qsmart.vessel_magnitude) {
        (Some(p), config::QsmartVesselMagnitude::MeanN4) => {
            let echoes = load_nifti_4d(p)?.echoes;
            let refs: Vec<&[f64]> = echoes.iter().map(|e| e.as_slice()).collect();
            Some(qsm_core::utils::qsmart_vessel_magnitude(&refs, &scan_meta.grid())
                .map_err(|e| QsmxtError::Config(format!("--magnitude: {e}")))?)
        }
        (Some(p), config::QsmartVesselMagnitude::Combined) => Some(load_nifti(p)?.data),
        (None, _) => None,
    };
    let reliability = match &args.reliability {
        Some(p) => Some(load_mask(p)?.0),
        None => None,
    };

    let mut progress = |_: usize, _: usize| {};
    let chi = qsm_core::pipeline::run_qsmart(
        &field.data,
        &mask,
        reliability.as_deref(),
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
