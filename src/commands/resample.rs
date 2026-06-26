use log::info;
use super::common::{load_nifti, save_nifti};
use crate::cli::ResampleArgs;
use crate::pipeline::phase;

pub fn execute(args: ResampleArgs) -> crate::Result<()> {
    let nifti = load_nifti(&args.input)?;
    let (nx, ny, nz) = nifti.dims;
    let obliquity = phase::obliquity_from_affine(&nifti.affine);

    info!(
        "Resampling {} to axial ({}x{}x{}, obliquity={:.1}°)",
        args.input.display(), nx, ny, nz, obliquity
    );

    let resampled = phase::resample_to_axial(&nifti.data, nx, ny, nz, &nifti.affine);

    info!(
        "New dimensions: {}x{}x{}",
        resampled.dims.0, resampled.dims.1, resampled.dims.2
    );

    // Save using resampled geometry
    let ref_nifti = qsm_core::nifti_io::NiftiData {
        data: vec![],
        dims: resampled.dims,
        voxel_size: resampled.voxel_size,
        affine: resampled.affine,
        scl_slope: 1.0,
        scl_inter: 0.0,
    };
    save_nifti(&args.output, &resampled.data, &ref_nifti)?;
    info!("Resampled volume saved to {}", args.output.display());
    Ok(())
}
