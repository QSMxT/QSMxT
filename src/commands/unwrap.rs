use log::{info, debug};
use super::common::{load_nifti, load_mask, save_nifti};
use crate::cli::UnwrapCommand;

/// QSMxT's field-mapping settings with these flags applied, as `qsmxt run` applies them: ROMEO's
/// and PRELUDE's start from QSMxT's defaults, the ones the run unwraps with.
fn config(settings: &[qsmxt_engine::schema::cli_apply::Given]) -> qsmxt_io::Result<qsmxt_engine::FieldMappingConfig> {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    qsmxt_io::settings::apply(&mut cfg, settings)?;
    Ok(cfg.field_mapping)
}

/// The ROMEO flags that only mean something when several echoes are unwrapped together.
const MULTI_ECHO_ROMEO_FLAGS: &[&str] = &[
    "romeo-template", "romeo-individual", "no-romeo-individual", "romeo-correct-global", "no-romeo-correct-global",
    "romeo-temporal-uncertain-unwrapping",
    // Phase gradient coherence compares two echoes; with one it is never computed.
    "no-romeo-phase-gradient-coherence",
];

pub fn execute(cmd: UnwrapCommand) -> qsmxt_io::Result<()> {
    match cmd {
        UnwrapCommand::Laplacian(args) => {
            let phase_nifti = load_nifti(&args.common.input)?;
            let (mask, _) = load_mask(&args.common.mask)?;
            let grid = super::common::nifti_grid(&phase_nifti);

            let mut phase_data = phase_nifti.data.clone();
            qsm_core::pipeline::scale_phase_to_pi(&mut phase_data);
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
            qsm_core::pipeline::scale_phase_to_pi(&mut phase_data);
            info!("Unwrapping phase (ROMEO, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let multi: Vec<String> = args.settings.0.iter().filter(|g| MULTI_ECHO_ROMEO_FLAGS.contains(&g.flag.long))
                .map(|g| format!("--{}", g.flag.long)).collect();
            if !multi.is_empty() {
                return Err(qsmxt_io::error::QsmxtError::Config(format!(
                    "{}: only meaningful when several echoes are unwrapped together — use \
                     `qsmxt fieldmap romeo` or `qsmxt run`; `unwrap romeo` unwraps one 3D phase",
                    multi.join(", "))));
            }

            let mag = if let Some(ref mag_path) = args.magnitude {
                load_nifti(mag_path)?.data
            } else {
                vec![1.0f64; phase_data.len()]
            };

            let params: qsm_core::unwrap::RomeoParams = (&config(&args.settings.0)?.romeo).into();
            debug!("ROMEO params: {:?}", params);
            let unwrapped = qsm_core::unwrap::unwrap_romeo(
                &phase_data, &mag, None, 0.0, 0.0, &mask, &params, &grid,
            );

            save_nifti(&args.common.output, &unwrapped, &phase_nifti)?;
            info!("Unwrapped phase saved to {}", args.common.output.display());
        }
        UnwrapCommand::MediRegionGrowing(args) => {
            let phase_nifti = load_nifti(&args.input)?;
            let grid = super::common::nifti_grid(&phase_nifti);
            let mut phase_data = phase_nifti.data.clone();
            qsm_core::pipeline::scale_phase_to_pi(&mut phase_data);
            let mag = load_nifti(&args.magnitude)?.data;
            if mag.len() != phase_data.len() {
                return Err(qsmxt_io::error::QsmxtError::Config(format!(
                    "--magnitude has {} voxels and the phase {}: give one 3D magnitude on the phase's grid",
                    mag.len(), phase_data.len())));
            }
            info!("Unwrapping phase (MEDI region growing, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());
            let unwrapped = qsm_core::unwrap::unwrap_medi_region_growing(&phase_data, &mag, &grid);
            save_nifti(&args.output, &unwrapped, &phase_nifti)?;
            info!("Unwrapped phase saved to {}", args.output.display());
        }
        UnwrapCommand::Prelude(args) => {
            let phase_nifti = load_nifti(&args.common.input)?;
            let (mask, _) = load_mask(&args.common.mask)?;
            let grid = super::common::nifti_grid(&phase_nifti);
            let mut phase_data = phase_nifti.data.clone();
            qsm_core::pipeline::scale_phase_to_pi(&mut phase_data);
            info!("Unwrapping phase (PRELUDE, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let params: qsm_core::unwrap::PreludeParams = (&config(&args.settings.0)?.prelude).into();
            debug!("PRELUDE params: {:?}", params);
            let unwrapped = qsm_core::unwrap::unwrap_prelude(&phase_data, &mask, &grid, &params);

            save_nifti(&args.common.output, &unwrapped, &phase_nifti)?;
            info!("Unwrapped phase saved to {}", args.common.output.display());
        }
    }
    Ok(())
}
