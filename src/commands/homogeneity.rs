use log::info;
use super::common::{load_nifti, save_nifti};
use crate::cli::{HomogeneityArgs, InhomogeneityMethodArg};
use crate::error::QsmxtError;

pub fn execute(args: HomogeneityArgs) -> crate::Result<()> {
    let nifti = load_nifti(&args.input)?;
    let grid = super::common::nifti_grid(&nifti);

    let corrected = match args.method {
        InhomogeneityMethodArg::Makehomogeneous => {
            info!(
                "Applying inhomogeneity correction (makehomogeneous) to {} ({}x{}x{}, sigma={:.1}mm)",
                args.input.display(), grid.nx(), grid.ny(), grid.nz(), args.sigma
            );
            qsm_core::utils::makehomogeneous(&nifti.data, &grid, args.sigma, args.nbox)?
        }
        InhomogeneityMethodArg::N4 => {
            let mut n4 = qsmxt_engine::N4Config::default();
            args.n4_params.apply_to(&mut n4);
            // qsm-core panics on these; say what is wrong instead.
            let dims = (grid.nx(), grid.ny(), grid.nz());
            if let Some(why) = n4.validate().err().or_else(|| n4.grid_problem(dims)) {
                return Err(QsmxtError::Config(why.replace("--inhomogeneity-method", "--method")));
            }
            info!(
                "Applying N4 inhomogeneity correction to {} ({}x{}x{})",
                args.input.display(), dims.0, dims.1, dims.2
            );
            qsm_core::utils::n4_bias_correction(&nifti.data, None, &grid, &n4.core(), |_, _| {}).corrected
        }
    };

    save_nifti(&args.output, &corrected, &nifti)?;
    info!("Corrected magnitude saved to {}", args.output.display());
    Ok(())
}
