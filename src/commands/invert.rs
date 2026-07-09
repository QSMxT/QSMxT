use log::{info, warn};
use super::common::{load_nifti, load_mask, save_nifti};
use crate::cli::InvertCommand;

pub fn execute(cmd: InvertCommand) -> crate::Result<()> {
    let (common, chi) = match cmd {
        InvertCommand::Rts(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (RTS, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::RtsParams::default();
            let params = qsm_core::inversion::RtsParams {
                delta: args.delta.unwrap_or(d.delta),
                mu: args.mu.unwrap_or(d.mu),
                rho: args.rho.unwrap_or(d.rho),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                lsmr_iter: args.lsmr_iter.unwrap_or(d.lsmr_iter),
            };
            let chi = qsm_core::inversion::rts(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Tv(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (TV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::TvParams::default();
            let params = qsm_core::inversion::TvParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                rho: args.rho.unwrap_or(d.rho),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
            };
            let chi = qsm_core::inversion::tv_admm(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Tkd(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (TKD, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::TkdParams::default();
            let chi = qsm_core::inversion::tkd(
                &field_nifti.data, &mask, &grid, bdir,
                &qsm_core::inversion::TkdParams { threshold: args.threshold.unwrap_or(d.threshold) },
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Tsvd(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (TSVD, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::TkdParams::default();
            let chi = qsm_core::inversion::tsvd(
                &field_nifti.data, &mask, &grid, bdir,
                &qsm_core::inversion::TkdParams { threshold: args.threshold.unwrap_or(d.threshold) },
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Ilsqr(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (iLSQR, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::IlsqrParams::default();
            let params = qsm_core::inversion::IlsqrParams {
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
            };
            let (chi, _, _, _) = qsm_core::inversion::ilsqr(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Tikhonov(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (Tikhonov, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::TikhonovParams::default();
            let params = qsm_core::inversion::TikhonovParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                ..d
            };
            let chi = qsm_core::inversion::tikhonov(
                &field_nifti.data, &mask, &grid, bdir, &params,
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Nltv(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (NLTV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::NltvParams::default();
            let params = qsm_core::inversion::NltvParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                mu: args.mu.unwrap_or(d.mu),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                newton_iter: args.newton_iter.unwrap_or(d.newton_iter),
            };
            let chi = qsm_core::inversion::nltv(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Medi(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (MEDI, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            // MEDI treats the field as a phase (exp(i·field)), so it must be in RADIANS.
            // Convert the ppm field with the field strength and echo time, and convert χ back.
            let gamma_hz = 42.576e6;
            let ppm_to_rad =
                2.0 * std::f64::consts::PI * gamma_hz * args.field_strength * args.echo_time * 1e-6;
            let field_rad: Vec<f64> = field_nifti.data.iter().map(|&v| v * ppm_to_rad).collect();

            let d = qsm_core::inversion::MediParams::default();
            let n_voxels = field_nifti.data.len();
            let (n_std, magnitude) = if let Some(ref mag_path) = args.magnitude {
                let mag_nifti = load_nifti(mag_path)?;
                // A multi-echo magnitude arrives 4D; MEDI uses a single 3D volume (first echo).
                let mag = if mag_nifti.data.len() > n_voxels {
                    mag_nifti.data[..n_voxels].to_vec()
                } else {
                    mag_nifti.data
                };
                (vec![1.0f64; n_voxels], mag)
            } else {
                warn!("No --magnitude provided for MEDI; using uniform magnitude (results may be suboptimal)");
                (vec![1.0f64; n_voxels], vec![1.0f64; n_voxels])
            };
            let params = qsm_core::inversion::MediParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                merit: args.merit.unwrap_or(d.merit),
                smv: args.smv.unwrap_or(d.smv),
                smv_radius: args.smv_radius.unwrap_or(d.smv_radius),
                data_weighting: args.data_weighting.unwrap_or(d.data_weighting),
                percentage: args.percentage.unwrap_or(d.percentage),
                cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
                cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
            };
            let chi_rad = qsm_core::inversion::medi(
                &field_rad, &n_std, &magnitude, &mask, &grid, bdir, &params, |_, _| {},
            );
            let chi: Vec<f64> = chi_rad.iter().map(|&v| v / ppm_to_rad).collect();
            (c, (chi, field_nifti))
        }
        InvertCommand::Tgv(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (TGV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::TgvParams::default();
            let params = qsm_core::inversion::TgvParams {
                iterations: args.iterations.unwrap_or(d.iterations),
                erosions: args.erosions.unwrap_or(d.erosions),
                alpha1: args.alpha1.unwrap_or(d.alpha1 as f64) as f32,
                alpha0: args.alpha0.unwrap_or(d.alpha0 as f64) as f32,
                step_size: args.step_size.unwrap_or(d.step_size as f64) as f32,
                tol: args.tol.unwrap_or(d.tol as f64) as f32,
                fieldstrength: args.field_strength as f32,
                te: args.echo_time as f32,
            };
            let chi = qsm_core::inversion::tgv_qsm(
                &field_nifti.data, &mask, &grid, &params, bdir, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
    };

    let (chi_data, field_nifti) = chi;
    save_nifti(&common.output, &chi_data, &field_nifti)?;
    info!("Susceptibility map saved to {}", common.output.display());
    Ok(())
}
