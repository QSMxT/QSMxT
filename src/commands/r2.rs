use log::info;
use super::common::{load_nifti, load_mask, save_nifti, nifti_grid, load_multiecho_voxel_major};
use crate::cli::R2Args;
use crate::error::QsmxtError;

pub fn execute(args: R2Args) -> crate::Result<()> {
    let reference = load_nifti(&args.inputs[0])?;
    let (nx, ny, nz) = reference.dims;
    let n = nx * ny * nz;
    let (multi, ne) = load_multiecho_voxel_major(&args.inputs, n)?;
    if ne != args.echo_times.len() {
        return Err(QsmxtError::Config(format!(
            "{} spin-echo images but {} echo times", ne, args.echo_times.len())));
    }
    info!("Computing R2 (EPG) from {} spin echoes", ne);
    let (mask, _) = load_mask(&args.mask)?;
    let grid = nifti_grid(&reference);
    let b1 = args.b1_map.as_ref().map(|p| load_nifti(p)).transpose()?;
    let d = qsm_core::relaxometry::R2EpgParams::default();
    let params = qsm_core::relaxometry::R2EpgParams { t1: args.t1.unwrap_or(d.t1), ..d };
    let (r2_map, _b1_fit) = qsm_core::relaxometry::r2_epg(
        &multi, &mask, &args.echo_times, &grid, &params,
        b1.as_ref().map(|n| n.data.as_slice()),
    );
    save_nifti(&args.output, &r2_map, &reference)?;
    info!("R2 map saved to {}", args.output.display());
    Ok(())
}
