use log::info;
use super::common::{load_nifti, save_nifti};
use crate::cli::QualityMapArgs;

pub fn execute(args: QualityMapArgs) -> crate::Result<()> {
    let phase_nifti = load_nifti(&args.phase)?;
    let (nx, ny, nz) = phase_nifti.dims;
    let n_voxels = nx * ny * nz;

    let mag = if let Some(ref mag_path) = args.magnitude {
        load_nifti(mag_path)?.data
    } else {
        vec![1.0f64; n_voxels]
    };

    let phase2 = if let Some(ref p2_path) = args.phase2 {
        Some(load_nifti(p2_path)?.data)
    } else {
        None
    };

    let all_ones = vec![1u8; n_voxels];

    info!("Computing ROMEO quality map ({}x{}x{})", nx, ny, nz);

    let grid = super::common::nifti_grid(&phase_nifti);
    let quality = qsm_core::unwrap::voxel_quality_romeo(
        &phase_nifti.data, &mag, phase2.as_deref(),
        args.te1, args.te2, &all_ones, &grid,
    );

    save_nifti(&args.output, &quality, &phase_nifti)?;

    let max_q = quality.iter().cloned().fold(0.0f64, f64::max);
    let mean_q: f64 = quality.iter().sum::<f64>() / quality.len() as f64;
    info!(
        "Quality map saved to {} (mean={:.1}, max={:.1})",
        args.output.display(), mean_q, max_q
    );
    Ok(())
}
