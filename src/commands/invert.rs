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
    // Fetch the model's ONNX weights with a download bar before inference (cached afterwards).
    if let Some(id) = algorithm.dl_model_id() {
        crate::pipeline::runner::prefetch_weights(id, name)?;
    }
    info!("Dipole inversion ({}, {}x{}x{})", name, nx, ny, nz);
    let metadata = qsm_core::pipeline::ScanMetadata {
        dims: field_nifti.dims,
        voxel_size: field_nifti.voxel_size,
        echo_times: vec![],
        field_strength,
        b0_direction: (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]),
    };
    // Opt-in overlap-tiling: presence of --tile-size enables it; halo defaults to 8.
    let tile = c.tiling_params.tile_size.map(|core| (core, c.tiling_params.tile_halo.unwrap_or(8)));
    let config = qsm_core::pipeline::InversionConfig { algorithm, tile, ..Default::default() };
    let chi = qsm_core::pipeline::run_dipole_inversion(
        &field_nifti.data, &mask, &metadata, &config, None, &mut |_, _| {},
    ).map_err(|e| crate::error::QsmxtError::Config(format!("{}: {}", name, e)))?;
    Ok((c, (chi, field_nifti)))
}

/// Load the optional magnitude used by LSQR (and so by HEIDI's seed) as an SNR row weight.
/// A multi-echo (4D) magnitude is RSS-combined; qsm-core renormalises it to unit mean inside
/// the mask, so the units it arrives in do not matter.
fn load_snr_weight(path: Option<&std::path::Path>, n_voxels: usize) -> crate::Result<Option<Vec<f64>>> {
    match path {
        Some(p) => Ok(Some(super::common::load_magnitude_rss(p, n_voxels)?)),
        None => {
            warn!("No --magnitude provided; the LSQR solve uses uniform row weights");
            Ok(None)
        }
    }
}

/// Build `LsqrQsmParams` from the shared `--lsqr-*` group.
///
/// Defaults come from [`LsqrConfig`] — the same struct `qsmxt run` starts from — so the two
/// surfaces cannot drift apart. `mask_output` is not a user flag: a standalone LSQR map is masked,
/// while HEIDI's seed must not be (HEIDI low-passes χ through the dipole cone, and a hard mask
/// edge rings).
fn lsqr_params(
    args: &crate::cli::LsqrParamArgs, b0: f64, mask_output: bool,
) -> qsm_core::inversion::LsqrQsmParams {
    let d = crate::pipeline::config::LsqrConfig::default();
    qsm_core::inversion::LsqrQsmParams {
        b0,
        // `None` here is meaningful — it selects qsm-core's field-strength-scaled default — so an
        // unset flag keeps the `None` rather than collapsing to a concrete 3T number.
        residual_weighting: args.lsqr_residual_weighting.or(d.residual_weighting),
        fit_global_offset: d.fit_global_offset && !args.no_lsqr_global_offset,
        tol: args.lsqr_tol.unwrap_or(d.tol),
        max_iter: args.lsqr_max_iter.unwrap_or(d.max_iter),
        mask_output,
    }
}

/// Build `HeidiParams` from the `--heidi-*` group.
///
/// [`HeidiConfig`] is the pipeline's flattening of qsm-core's optional denoise struct into a
/// switch plus three values; taking the defaults from it keeps `invert heidi` and
/// `run --qsm-algorithm heidi` on one set of numbers.
fn heidi_params(args: &crate::cli::HeidiParamArgs) -> qsm_core::inversion::HeidiParams {
    let d = crate::pipeline::config::HeidiConfig::default();
    qsm_core::inversion::HeidiParams {
        cone_threshold: args.heidi_cone_threshold.unwrap_or(d.cone_threshold),
        gradient_threshold: args.heidi_gradient_threshold.unwrap_or(d.gradient_threshold),
        apply_laplacian_correction: d.apply_laplacian_correction
            && !args.no_heidi_laplacian_correction,
        laplacian_threshold: args.heidi_laplacian_threshold.unwrap_or(d.laplacian_threshold),
        gradient_mask_floor: args.heidi_gradient_mask_floor.unwrap_or(d.gradient_mask_floor),
        continuation_steps: args.heidi_continuation_steps.unwrap_or(d.continuation_steps),
        inner_iterations: args.heidi_inner_iterations.unwrap_or(d.inner_iterations),
        mu_min: args.heidi_mu_min.unwrap_or(d.mu_min),
        tol: args.heidi_tol.unwrap_or(d.tol),
        denoise: (d.denoise && !args.no_heidi_denoise).then(|| {
            qsm_core::utils::AnisotropicDiffusionParams {
                iterations: args.heidi_denoise_iterations.unwrap_or(d.denoise_iterations),
                time_step: args.heidi_denoise_time_step.unwrap_or(d.denoise_time_step),
                conductance: args.heidi_denoise_conductance.unwrap_or(d.denoise_conductance),
            }
        }),
    }
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
        InvertCommand::Lsqr(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (LSQR, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let magnitude = load_snr_weight(args.magnitude.as_deref(), field_nifti.data.len())?;
            let params = lsqr_params(&args.lsqr_params, args.b0, true);
            let chi = qsm_core::inversion::lsqr_qsm(
                &field_nifti.data, &mask, magnitude.as_deref(), &grid, bdir, &params, |_, _| {},
            );
            (c, (chi, field_nifti))
        }
        InvertCommand::Heidi(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (HEIDI, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let magnitude = load_snr_weight(args.magnitude.as_deref(), field_nifti.data.len())?;
            // HEIDI is incremental: it keeps the well-conditioned k-space of a seed map and
            // re-derives the cone. The seed is the minimally regularised LSQR solution, left
            // unmasked so the cone projection does not ring off a hard mask edge — the same
            // arrangement the pipeline dispatcher uses for `--qsm-algorithm heidi`.
            let seed = lsqr_params(&args.lsqr_params, args.b0, false);
            let params = heidi_params(&args.heidi_params);
            let chi_init = qsm_core::inversion::lsqr_qsm(
                &field_nifti.data, &mask, magnitude.as_deref(), &grid, bdir, &seed, |_, _| {},
            );
            let chi = qsm_core::inversion::heidi(
                &field_nifti.data, &mask, &chi_init, &grid, bdir, &params, |_, _| {},
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

#[cfg(test)]
mod tests {
    use super::{heidi_params, lsqr_params};
    use crate::cli::{HeidiParamArgs, LsqrParamArgs};

    /// With no flags set, the `invert` surface must reproduce qsm-core's own defaults — the same
    /// parameters `qsmxt run --qsm-algorithm lsqr` would build. A hand-written `unwrap_or` chain
    /// is exactly where a default silently drifts from the library's.
    #[test]
    fn unset_flags_give_the_qsm_core_defaults() {
        let d = qsm_core::inversion::LsqrQsmParams::default();
        let p = lsqr_params(&LsqrParamArgs::default(), d.b0, d.mask_output);
        assert_eq!(p.residual_weighting, d.residual_weighting);
        assert_eq!(p.fit_global_offset, d.fit_global_offset);
        assert_eq!(p.tol, d.tol);
        assert_eq!(p.max_iter, d.max_iter);

        let h = qsm_core::inversion::HeidiParams::default();
        let p = heidi_params(&HeidiParamArgs::default());
        assert_eq!(p.cone_threshold, h.cone_threshold);
        assert_eq!(p.gradient_threshold, h.gradient_threshold);
        assert_eq!(p.apply_laplacian_correction, h.apply_laplacian_correction);
        assert_eq!(p.laplacian_threshold, h.laplacian_threshold);
        assert_eq!(p.gradient_mask_floor, h.gradient_mask_floor);
        assert_eq!(p.continuation_steps, h.continuation_steps);
        assert_eq!(p.inner_iterations, h.inner_iterations);
        assert_eq!(p.mu_min, h.mu_min);
        assert_eq!(p.tol, h.tol);
        let (a, b) = (p.denoise.unwrap(), h.denoise.unwrap());
        assert_eq!((a.iterations, a.time_step, a.conductance), (b.iterations, b.time_step, b.conductance));
    }

    /// `--lsqr-residual-weighting` is `Option<f64>` in qsm-core because `None` means
    /// "scale with field strength" — collapsing it to a concrete number would silently pin the
    /// weight to 3T behaviour on every other scanner.
    #[test]
    fn an_unset_residual_weighting_stays_none() {
        assert_eq!(lsqr_params(&LsqrParamArgs::default(), 7.0, true).residual_weighting, None);
        let explicit = LsqrParamArgs { lsqr_residual_weighting: Some(0.0), ..Default::default() };
        assert_eq!(lsqr_params(&explicit, 7.0, true).residual_weighting, Some(0.0));
    }

    /// HEIDI low-passes its seed through the dipole cone, so a hard mask edge rings. The seed
    /// must therefore be unmasked while a standalone `invert lsqr` map is masked — and neither is
    /// a user flag, so only a test pins it.
    #[test]
    fn the_heidi_seed_is_unmasked_and_standalone_lsqr_is_not() {
        assert!(!lsqr_params(&LsqrParamArgs::default(), 3.0, false).mask_output);
        assert!(lsqr_params(&LsqrParamArgs::default(), 3.0, true).mask_output);
    }

    /// The denoise switch is a negative flag over a `Some(..)` default: `--no-heidi-denoise` has
    /// to clear the whole struct, not just zero its fields.
    #[test]
    fn no_denoise_clears_the_diffusion_params() {
        let off = HeidiParamArgs { no_heidi_denoise: true, ..Default::default() };
        assert!(heidi_params(&off).denoise.is_none());
        let tuned = HeidiParamArgs { heidi_denoise_iterations: Some(9), ..Default::default() };
        assert_eq!(heidi_params(&tuned).denoise.unwrap().iterations, 9);
    }

    /// `--b0` feeds the seed's field-strength-scaled residual weight; passing it through the
    /// wrong argument would leave every solve at the 3T default.
    #[test]
    fn b0_reaches_the_effective_residual_weighting() {
        let at3 = lsqr_params(&LsqrParamArgs::default(), 3.0, true).effective_residual_weighting();
        let at7 = lsqr_params(&LsqrParamArgs::default(), 7.0, true).effective_residual_weighting();
        assert!(at7 > at3, "7T weight {at7} should exceed 3T {at3}");
    }
}
