use log::info;
use super::common::{load_nifti, save_nifti};
use crate::cli::HomogeneityArgs;
use qsmxt_io::error::QsmxtError;

pub fn execute(args: HomogeneityArgs) -> qsmxt_io::Result<()> {
    let nifti = load_nifti(&args.input)?;
    let grid = super::common::nifti_grid(&nifti);

    // QSMxT's settings with the flags applied, as `qsmxt run` applies them.
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    qsmxt_io::settings::apply(&mut cfg, &args.settings.0)?;
    let corrected = match cfg.masking.inhomogeneity_method {
        qsmxt_engine::InhomogeneityMethod::Makehomogeneous => {
            let h = &cfg.homogeneity;
            info!(
                "Applying inhomogeneity correction (makehomogeneous) to {} ({}x{}x{}, sigma={:.1}mm)",
                args.input.display(), grid.nx(), grid.ny(), grid.nz(), h.sigma_mm
            );
            qsm_core::utils::makehomogeneous(&nifti.data, &grid, h.sigma_mm, h.nbox)?
        }
        qsmxt_engine::InhomogeneityMethod::N4 => {
            let core = cfg.n4.core();
            let dims = (grid.nx(), grid.ny(), grid.nz());
            core.validate()?;
            if let Err(e) = core.check_grid(dims) {
                return Err(QsmxtError::Config(format!(
                    "{e}. Lower --n4-shrink-factor, or use --inhomogeneity-method makehomogeneous")));
            }
            info!(
                "Applying N4 inhomogeneity correction to {} ({}x{}x{})",
                args.input.display(), dims.0, dims.1, dims.2
            );
            // N4's shrink offsets depend on the image's physical geometry, which ITK reads from the
            // header (its sform/qform choice, LPS origin and directions): read it the same way, or
            // take make_nii's axis-aligned header for the file's voxel size (`--n4-geometry`).
            let geometry = match cfg.masking.n4_geometry {
                qsmxt_engine::N4Geometry::Scanner => qsm_core::io::read_itk_geometry_file(&args.input)
                    .map_err(|e| QsmxtError::NiftiIo(format!("{}: {e}", args.input.display())))?,
                qsmxt_engine::N4Geometry::MakeNii =>
                    qsm_core::utils::ItkGeometry::make_nii((grid.vsx(), grid.vsy(), grid.vsz())),
            };
            qsm_core::utils::n4_bias_correction(&nifti.data, None, &grid, &geometry, &core, |_, _| {})?.corrected
        }
    };

    save_nifti(&args.output, &corrected, &nifti)?;
    info!("Corrected magnitude saved to {}", args.output.display());
    Ok(())
}
