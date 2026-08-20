use log::{info, warn};
use super::common::{load_nifti, load_mask, save_nifti};
use crate::cli::{InvertCommand, InvertCommonArgs};

/// Run a deep-learning dipole inversion through qsm-core's pipeline dispatcher (the DL models
/// are not exposed as standalone functions). Weights are downloaded on first use. AutoQSM is
/// single-step and expects the TOTAL field as input; the others take a local (tissue) field.
fn run_dl_inversion(
    c: InvertCommonArgs, field_strength: f64,
    algorithm: qsm_core::pipeline::InversionAlgorithm, name: &str,
) -> crate::Result<(InvertCommonArgs, (Vec<f64>, qsm_core::io::NiftiData))> {
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let (nx, ny, nz) = field_nifti.dims;
    info!("Dipole inversion ({}, {}x{}x{})", name, nx, ny, nz);
    let metadata = qsm_core::pipeline::ScanMetadata {
        dims: field_nifti.dims,
        voxel_size: field_nifti.voxel_size,
        echo_times: vec![],
        field_strength,
        b0_direction: (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]),
    };
    let config = qsm_core::pipeline::InversionConfig { algorithm, ..Default::default() };
    let chi = qsm_core::pipeline::run_dipole_inversion(
        &field_nifti.data, &mask, &metadata, &config, None, &mut |_, _| {},
    ).map_err(|e| crate::error::QsmxtError::Config(format!("{}: {}", name, e)))?;
    Ok((c, (chi, field_nifti)))
}

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
        InvertCommand::Ndi(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (NDI, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::NdiParams::default();
            let params = qsm_core::inversion::NdiParams {
                tau: args.tau.unwrap_or(d.tau),
                alpha: args.alpha.unwrap_or(d.alpha),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                phase_scale: args.phase_scale.unwrap_or(d.phase_scale),
            };
            let chi = qsm_core::inversion::ndi(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Fansi(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (FANSI nlTV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::FansiParams::default();
            let params = qsm_core::inversion::FansiParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.unwrap_or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                alpha0: args.alpha0.unwrap_or(d.alpha0),
                mu0: args.mu0.unwrap_or(d.mu0),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                phase_scale: args.phase_scale.unwrap_or(d.phase_scale),
                is_tgv: false,
            };
            let chi = qsm_core::inversion::fansi(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::FansiTgv(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (FANSI nlTGV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::FansiParams::default();
            let params = qsm_core::inversion::FansiParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.unwrap_or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                alpha0: args.alpha0.unwrap_or(d.alpha0),
                mu0: args.mu0.unwrap_or(d.mu0),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                phase_scale: args.phase_scale.unwrap_or(d.phase_scale),
                is_tgv: true,
            };
            let chi = qsm_core::inversion::fansi(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::L1qsm(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (L1-QSM, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::L1QsmParams::default();
            let params = qsm_core::inversion::L1QsmParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.unwrap_or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                mu3: args.mu3.unwrap_or(d.mu3),
                lambda: args.lambda.unwrap_or(d.lambda),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                phase_scale: args.phase_scale.unwrap_or(d.phase_scale),
            };
            let chi = qsm_core::inversion::l1qsm(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Whqsm(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (WH-QSM, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::WhQsmParams::default();
            let params = qsm_core::inversion::WhQsmParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.unwrap_or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                beta: args.beta.unwrap_or(d.beta),
                muh: args.muh.unwrap_or(d.muh),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                phase_scale: args.phase_scale.unwrap_or(d.phase_scale),
            };
            let chi = qsm_core::inversion::whqsm(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Hdqsm(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (HD-QSM, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::inversion::HdQsmParams::default();
            let params = qsm_core::inversion::HdQsmParams {
                alpha_l2: args.alpha_l2.unwrap_or(d.alpha_l2),
                mu1_l2: args.mu1_l2.unwrap_or(d.mu1_l2),
                mu2: args.mu2.unwrap_or(d.mu2),
                max_iter_l1: args.max_iter_l1.unwrap_or(d.max_iter_l1),
                max_iter_l2: args.max_iter_l2.unwrap_or(d.max_iter_l2),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
            };
            let chi = qsm_core::inversion::hdqsm(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::AmpPe(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (AMP-PE, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            // AMP-PE takes the local field in ppm (like NDI). Magnitude, when provided, is the
            // data-fidelity weight + morphology mask; a multi-echo (4D) magnitude is RSS-combined.
            // χ comes back in ppm.
            let n_voxels = field_nifti.data.len();
            let magnitude: Option<Vec<f64>> = args.magnitude.as_ref()
                .map(|mag_path| super::common::load_magnitude_rss(mag_path, n_voxels))
                .transpose()?;
            if magnitude.is_none() {
                warn!("No --magnitude provided for AMP-PE; using uniform weights with no morphology mask");
            }

            let d = qsm_core::inversion::AmpPeParams::default();
            let params = qsm_core::inversion::AmpPeParams {
                wave_order: args.wave_order.unwrap_or(d.wave_order),
                nlevel: args.nlevel.unwrap_or(d.nlevel),
                wave_pec: args.wave_pec.unwrap_or(d.wave_pec),
                simulated_te: args.simulated_te.unwrap_or(d.simulated_te),
                max_linearization_ite: args.max_linearization_ite.unwrap_or(d.max_linearization_ite),
                b0: args.b0,
                gyro_ratio: d.gyro_ratio,
                damp_rate_sig: args.damp_rate_sig.unwrap_or(d.damp_rate_sig),
                damp_rate_par: args.damp_rate_par.unwrap_or(d.damp_rate_par),
                max_pe_spar_ite: args.max_pe_spar_ite.unwrap_or(d.max_pe_spar_ite),
                max_pe_est_ite: args.max_pe_est_ite.unwrap_or(d.max_pe_est_ite),
                cvg_thd: args.cvg_thd.unwrap_or(d.cvg_thd),
                tikhonov_beta: args.tikhonov_beta.unwrap_or(d.tikhonov_beta),
            };
            let chi = qsm_core::inversion::amp_pe(
                &field_nifti.data, &mask, magnitude.as_deref(), &grid, bdir, &params, |_, _| {},
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
                // Multi-echo (4D) or multiple magnitudes are RSS-combined to a single weighting volume.
                let mag = super::common::load_magnitude_rss(mag_path, n_voxels)?;
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
        InvertCommand::Tfi(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (TFI, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            // TFI takes the TOTAL field in ppm (same convention as NDI and the other inversions —
            // NOT MEDI's radians). No conversion: scaling the large total field to radians would
            // wrap it in the exp(i·field) data term. χ comes back in ppm.
            let d = qsm_core::inversion::TfiParams::default();
            let n_voxels = field_nifti.data.len();
            let (n_std, magnitude) = if let Some(ref mag_path) = args.magnitude {
                // Multi-echo (4D) or multiple magnitudes are RSS-combined to a single weighting volume.
                let mag = super::common::load_magnitude_rss(mag_path, n_voxels)?;
                (vec![1.0f64; n_voxels], mag)
            } else {
                warn!("No --magnitude provided for TFI; using uniform magnitude (results may be suboptimal)");
                (vec![1.0f64; n_voxels], vec![1.0f64; n_voxels])
            };
            let params = qsm_core::inversion::TfiParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                precond: args.precond.unwrap_or(d.precond),
                merit: args.merit.unwrap_or(d.merit),
                data_weighting: args.data_weighting.unwrap_or(d.data_weighting),
                percentage: args.percentage.unwrap_or(d.percentage),
                cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
                cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
            };
            let chi = qsm_core::inversion::tfi(
                &field_nifti.data, &n_std, &magnitude, &mask, &grid, bdir, &params, |_, _| {},
            );
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
        InvertCommand::Xqsm(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::Xqsm, "xQSM")?,
        InvertCommand::Qsmnet(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::Qsmnet, "QSMnet")?,
        InvertCommand::QsmnetPlus(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::QsmnetPlus, "QSMnet+")?,
        InvertCommand::Autoqsm(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::Autoqsm, "AutoQSM")?,
        InvertCommand::Qsmgan(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::Qsmgan, "QSMGAN")?,
        InvertCommand::Ir2qsm(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::Ir2qsm, "IR2QSM")?,
        InvertCommand::Lpcnn(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::Lpcnn, "LPCNN")?,
        InvertCommand::ModlQsm(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::ModlQsm, "MoDL-QSM")?,
        InvertCommand::Nextqsm(args) =>
            run_dl_inversion(args.common, args.field_strength, qsm_core::pipeline::InversionAlgorithm::Nextqsm, "NeXtQSM")?,
    };

    let (chi_data, field_nifti) = chi;
    save_nifti(&common.output, &chi_data, &field_nifti)?;
    info!("Susceptibility map saved to {}", common.output.display());
    Ok(())
}
