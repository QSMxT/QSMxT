use log::info;
use super::common::{load_nifti, save_mask, run_mask_operation};
use crate::cli::{MaskCommand, MaskCommonArgs};
use crate::pipeline::config::{parse_mask_op, MaskOp};

fn apply_ops(mut mask: Vec<u8>, ops: &[String], grid: &qsm_core::Grid) -> crate::Result<Vec<u8>> {
    for op_str in ops {
        let op = parse_mask_op(op_str)?;
        mask = apply_mask_op(mask, &op, grid);
    }
    Ok(mask)
}

fn apply_mask_op(mut mask: Vec<u8>, op: &MaskOp, grid: &qsm_core::Grid) -> Vec<u8> {
    match op {
        MaskOp::Erode { iterations } => {
            mask = qsm_core::utils::erode_mask(&mask, grid, *iterations);
        }
        MaskOp::Dilate { iterations } => {
            mask = qsm_core::utils::dilate_mask(&mask, grid, *iterations);
        }
        MaskOp::Close { radius } => {
            mask = qsm_core::utils::morphological_close(&mask, grid, *radius as i32);
        }
        MaskOp::FillHoles { max_size } => {
            let effective_size = if *max_size == 0 { grid.n_total() / 20 } else { *max_size };
            mask = qsm_core::utils::fill_holes(&mask, grid, effective_size);
        }
        MaskOp::GaussianSmooth { sigma_mm } => {
            let mask_f64: Vec<f64> = mask.iter().map(|&v| v as f64).collect();
            let smoothed = qsm_core::utils::gaussian_smooth_3d(
                &mask_f64, [*sigma_mm, *sigma_mm, *sigma_mm], None, None, 3, grid,
            );
            mask = smoothed.iter().map(|&v| if v > 0.5 { 1u8 } else { 0u8 }).collect();
        }
        _ => {} // Threshold/Bet are generators, not refinements
    }
    mask
}

pub fn execute(cmd: MaskCommand) -> crate::Result<()> {
    match cmd {
        MaskCommand::Otsu(args) => {
            let nifti = load_nifti(&args.common.input)?;
            let grid = super::common::nifti_grid(&nifti);
            let t = qsm_core::utils::otsu_threshold(&nifti.data, 256);
            info!("Otsu threshold: {:.4}", t);
            let mask: Vec<u8> = nifti.data.iter().map(|&v| if v > t { 1u8 } else { 0u8 }).collect();
            let mask = apply_ops(mask, &args.common.ops, &grid)?;
            save_and_log(&args.common, &mask, &nifti)
        }
        MaskCommand::Value(args) => {
            let nifti = load_nifti(&args.common.input)?;
            let grid = super::common::nifti_grid(&nifti);
            let mask: Vec<u8> = nifti.data.iter().map(|&v| if v > args.threshold { 1u8 } else { 0u8 }).collect();
            let mask = apply_ops(mask, &args.common.ops, &grid)?;
            save_and_log(&args.common, &mask, &nifti)
        }
        MaskCommand::Percentile(args) => {
            let nifti = load_nifti(&args.common.input)?;
            let grid = super::common::nifti_grid(&nifti);
            let mut sorted: Vec<f64> = nifti.data.iter().copied().filter(|v| v.is_finite()).collect();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let idx = ((args.percentile / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
            let t = sorted[idx.min(sorted.len() - 1)];
            info!("Percentile {:.1}% threshold: {:.4}", args.percentile, t);
            let mask: Vec<u8> = nifti.data.iter().map(|&v| if v > t { 1u8 } else { 0u8 }).collect();
            let mask = apply_ops(mask, &args.common.ops, &grid)?;
            save_and_log(&args.common, &mask, &nifti)
        }
        MaskCommand::Bet(args) => {
            let nifti = load_nifti(&args.common.input)?;
            let grid = super::common::nifti_grid(&nifti);
            info!("Running BET (fractional_intensity={:.2})", args.fractional_intensity);
            let mask = qsm_core::bet::run_bet(
                &nifti.data, &grid,
                args.fractional_intensity, 1.0, 0.0, 1000, 4, |_, _| {},
            );
            let mask = apply_ops(mask, &args.common.ops, &grid)?;
            save_and_log(&args.common, &mask, &nifti)
        }
        MaskCommand::Robust(args) => {
            let nifti = load_nifti(&args.input)?;
            let grid = super::common::nifti_grid(&nifti);
            let t = qsm_core::utils::otsu_threshold(&nifti.data, 256);
            info!("Robust mask (Otsu threshold: {:.4}, dilate:1, fill-holes:auto, erode:1)", t);
            let mut mask: Vec<u8> = nifti.data.iter().map(|&v| if v > t { 1u8 } else { 0u8 }).collect();
            mask = qsm_core::utils::dilate_mask(&mask, &grid, 1);
            mask = qsm_core::utils::fill_holes(&mask, &grid, grid.n_total() / 20);
            mask = qsm_core::utils::erode_mask(&mask, &grid, 1);

            save_mask(&args.output, &mask, &nifti)?;
            let count: usize = mask.iter().map(|&m| m as usize).sum();
            info!(
                "Mask saved to {} ({} voxels, {:.1}%)",
                args.output.display(), count, 100.0 * count as f64 / mask.len() as f64
            );
            Ok(())
        }
        MaskCommand::Erode(args) => {
            let iters = args.iterations;
            run_mask_operation(&args.input, &args.output, "Eroding mask", |mask, grid| {
                qsm_core::utils::erode_mask(mask, grid, iters)
            })
        }
        MaskCommand::Dilate(args) => {
            let iters = args.iterations;
            run_mask_operation(&args.input, &args.output, "Dilating mask", |mask, grid| {
                qsm_core::utils::dilate_mask(mask, grid, iters)
            })
        }
        MaskCommand::Close(args) => {
            let radius = args.radius;
            run_mask_operation(&args.input, &args.output, "Morphological close", |mask, grid| {
                qsm_core::utils::morphological_close(mask, grid, radius as i32)
            })
        }
        MaskCommand::FillHoles(args) => {
            let max_size = args.max_size;
            run_mask_operation(&args.input, &args.output, "Filling holes", |mask, grid| {
                qsm_core::utils::fill_holes(mask, grid, max_size)
            })
        }
        MaskCommand::Smooth(args) => {
            let nifti = load_nifti(&args.input)?;
            let grid = super::common::nifti_grid(&nifti);
            info!("Gaussian smoothing mask ({}x{}x{}, sigma={:.1}mm)", grid.nx(), grid.ny(), grid.nz(), args.sigma);
            let mask_f64: Vec<f64> = nifti.data.iter().map(|&v| if v > 0.0 { 1.0 } else { 0.0 }).collect();
            let smoothed = qsm_core::utils::gaussian_smooth_3d(
                &mask_f64, [args.sigma, args.sigma, args.sigma], None, None, 3, &grid,
            );
            let result: Vec<f64> = smoothed.iter().map(|&v| if v > 0.5 { 1.0 } else { 0.0 }).collect();
            super::common::save_nifti(&args.output, &result, &nifti)?;
            info!("Smoothed mask saved to {}", args.output.display());
            Ok(())
        }
    }
}

fn save_and_log(common: &MaskCommonArgs, mask: &[u8], nifti: &qsm_core::nifti_io::NiftiData) -> crate::Result<()> {
    save_mask(&common.output, mask, nifti)?;
    let count: usize = mask.iter().map(|&m| m as usize).sum();
    info!(
        "Mask saved to {} ({} voxels, {:.1}%)",
        common.output.display(), count, 100.0 * count as f64 / mask.len() as f64
    );
    Ok(())
}
