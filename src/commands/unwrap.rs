use log::{info, debug};
use super::common::{load_nifti, load_mask, save_nifti};
use crate::cli::UnwrapCommand;
use crate::pipeline::phase;

pub fn execute(cmd: UnwrapCommand) -> crate::Result<()> {
    match cmd {
        UnwrapCommand::Laplacian(args) => {
            let phase_nifti = load_nifti(&args.common.input)?;
            let (mask, _) = load_mask(&args.common.mask)?;
            let grid = super::common::nifti_grid(&phase_nifti);

            let mut phase_data = phase_nifti.data.clone();
            phase::scale_phase_to_pi(&mut phase_data);
            info!("Unwrapping phase (Laplacian, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let unwrapped = qsm_core::unwrap::laplacian_unwrap(&phase_data, &mask, &grid, qsm_core::unwrap::LaplacianSolver::Dct);

            save_nifti(&args.common.output, &unwrapped, &phase_nifti)?;
            info!("Unwrapped phase saved to {}", args.common.output.display());
        }
        UnwrapCommand::Romeo(args) => {
            let phase_nifti = load_nifti(&args.common.input)?;
            let (mask, _) = load_mask(&args.common.mask)?;
            let grid = super::common::nifti_grid(&phase_nifti);
            let mut phase_data = phase_nifti.data.clone();
            phase::scale_phase_to_pi(&mut phase_data);
            info!("Unwrapping phase (ROMEO, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let multi = args.romeo_params.multi_echo_flags();
            if !multi.is_empty() {
                return Err(crate::error::QsmxtError::Config(format!(
                    "{}: only meaningful when several echoes are unwrapped together — use \
                     `qsmxt fieldmap romeo` or `qsmxt run`; `unwrap romeo` unwraps one 3D phase",
                    multi.join(", "))));
            }

            let mag = if let Some(ref mag_path) = args.magnitude {
                load_nifti(mag_path)?.data
            } else {
                vec![1.0f64; phase_data.len()]
            };

            let params = args.romeo_params.to_romeo_params();
            debug!("ROMEO params: {:?}", params);
            let unwrapped = qsm_core::unwrap::unwrap_romeo(
                &phase_data, &mag, None, 0.0, 0.0, &mask, &params, &grid,
            );

            save_nifti(&args.common.output, &unwrapped, &phase_nifti)?;
            info!("Unwrapped phase saved to {}", args.common.output.display());
        }
    }
    Ok(())
}
