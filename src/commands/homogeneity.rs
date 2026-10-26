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
            let core = n4.core();
            let dims = (grid.nx(), grid.ny(), grid.nz());
            core.validate()?;
            if let Err(e) = core.check_grid(dims) {
                return Err(QsmxtError::Config(format!(
                    "{e}. Lower --n4-shrink-factor, or use --method makehomogeneous")));
            }
            info!(
                "Applying N4 inhomogeneity correction to {} ({}x{}x{})",
                args.input.display(), dims.0, dims.1, dims.2
            );
            qsm_core::utils::n4_bias_correction(&nifti.data, None, &grid, &core, |_, _| {})?.corrected
        }
    };

    save_nifti(&args.output, &corrected, &nifti)?;
    info!("Corrected magnitude saved to {}", args.output.display());
    Ok(())
}
