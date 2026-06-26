use log::info;
use super::common::{load_nifti, save_nifti};
use crate::cli::HomogeneityArgs;

pub fn execute(args: HomogeneityArgs) -> crate::Result<()> {
    let nifti = load_nifti(&args.input)?;
    let grid = super::common::nifti_grid(&nifti);

    info!(
        "Applying inhomogeneity correction to {} ({}x{}x{}, sigma={:.1}mm)",
        args.input.display(), grid.nx(), grid.ny(), grid.nz(), args.sigma
    );

    let corrected = qsm_core::utils::makehomogeneous(
        &nifti.data, &grid, args.sigma, args.nbox,
    );

    save_nifti(&args.output, &corrected, &nifti)?;
    info!("Corrected magnitude saved to {}", args.output.display());
    Ok(())
}
