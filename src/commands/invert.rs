use log::{info, warn};
use super::common::{load_nifti, load_mask, save_nifti};
use crate::cli::{InvertCommand, InvertCommonArgs};
use crate::error::QsmxtError;

/// The magnitude an inversion is given, as qsm-core's dispatcher asks for it.
enum Magnitude<'a> {
    /// None: the method takes no magnitude, or it is optional and was not given.
    None,
    /// The combined magnitude (root-sum-of-squares over the echoes given): the data weight.
    Combined(&'a [std::path::PathBuf]),
    /// Every echo's magnitude, stacked with the echo slowest (MEDI and TFI).
    Echoes(&'a [std::path::PathBuf]),
}

/// Invert `c.input` inside `c.mask` with `config` through qsm-core's pipeline dispatcher
/// ([`qsm_core::pipeline::run_dipole_inversion`]), as `qsmxt run` does: the field in ppm and the
/// scan's field strength and echo times go in, and each method does its own unit conversion and
/// derives its own weights. `field_strength` and `echo_times` are the scan's (empty for a method
/// that uses neither). Weights of a deep-learning method are fetched (with a download bar) first.
fn invert_with(
    c: &InvertCommonArgs, field_strength: f64, echo_times: &[f64], magnitude: Magnitude,
    config: qsm_core::pipeline::InversionConfig, name: &str,
) -> crate::Result<(Vec<f64>, qsm_core::io::NiftiData)> {
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let (nx, ny, nz) = field_nifti.dims;
    let n_voxels = field_nifti.data.len();
    if let Some(id) = config.algorithm.dl_model_id() {
        crate::pipeline::runner::prefetch_weights(id, name)?;
    }
    let magnitude = match magnitude {
        Magnitude::None => None,
        Magnitude::Combined(paths) => super::common::load_combined_magnitude(paths, n_voxels)?,
        Magnitude::Echoes(paths) => {
            let (stack, n_echoes) = super::common::load_echo_stack(paths, n_voxels)?;
            if n_echoes != echo_times.len() {
                return Err(QsmxtError::Config(format!(
                    "{name} needs the magnitude of every echo: got {n_echoes} magnitude volume(s) for {} echo time(s)",
                    echo_times.len())));
            }
            Some(stack)
        }
    };
    info!("Dipole inversion ({}, {}x{}x{})", name, nx, ny, nz);
    let metadata = qsm_core::pipeline::ScanMetadata {
        dims: field_nifti.dims,
        voxel_size: field_nifti.voxel_size,
        echo_times: echo_times.to_vec(),
        field_strength,
        b0_direction: (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]),
        slice_geometry: None,
        affine: Some(field_nifti.affine),
    };
    let chi = qsm_core::pipeline::run_dipole_inversion(
        &field_nifti.data, &mask, &metadata, &config, magnitude.as_deref(), &mut |_, _| {},
    ).map_err(|e| QsmxtError::Config(format!("{}: {}", name, e)))?;
    Ok((chi, field_nifti))
}

/// An inversion config for `algorithm`, with qsm-core's settings for every other method.
fn config_for(algorithm: qsm_core::pipeline::InversionAlgorithm) -> qsm_core::pipeline::InversionConfig {
    qsm_core::pipeline::InversionConfig { algorithm, ..Default::default() }
}

/// Run a deep-learning dipole inversion (the DL models are not exposed as standalone functions).
/// Weights are downloaded on first use. AutoQSM and NeXtQSM are single-step and expect the TOTAL
/// field as input; the others take a local (tissue) field.
fn run_dl_inversion(
    c: &InvertCommonArgs, field_strength: f64,
    algorithm: qsm_core::pipeline::InversionAlgorithm, name: &str,
) -> crate::Result<(Vec<f64>, qsm_core::io::NiftiData)> {
    // Opt-in overlap-tiling: presence of --tile-size enables it; halo defaults to 8.
    let tile = c.tiling_params.tile_size.map(|core| (core, c.tiling_params.tile_halo.unwrap_or(8)));
    let config = qsm_core::pipeline::InversionConfig { algorithm, tile, ..Default::default() };
    invert_with(c, field_strength, &[], Magnitude::None, config, name)
}

/// Build `LsqrQsmParams` from the shared `--lsqr-*` group.
///
/// Defaults come from [`LsqrConfig`] — the same struct `qsmxt run` starts from — so the two
/// surfaces cannot drift apart. `b0` and `mask_output` are the dispatcher's (as in `qsmxt run`).
fn lsqr_params(args: &crate::cli::LsqrParamArgs) -> qsm_core::inversion::LsqrQsmParams {
    let d = crate::pipeline::config::LsqrConfig::default();
    qsm_core::inversion::LsqrQsmParams {
        // `None` here is meaningful — it selects qsm-core's field-strength-scaled default — so an
        // unset flag keeps the `None` rather than collapsing to a concrete 3T number.
        residual_weighting: args.lsqr_residual_weighting.or(d.residual_weighting),
        fit_global_offset: d.fit_global_offset && !args.no_lsqr_global_offset,
        tol: args.lsqr_tol.unwrap_or(d.tol),
        max_iter: args.lsqr_max_iter.unwrap_or(d.max_iter),
        ..Default::default()
    }
}

/// Build `HeidiParams` from the `--heidi-*` group.
///
/// [`HeidiConfig`] is the pipeline's flattening of qsm-core's optional denoise struct into a
/// switch plus three values; taking the defaults from it keeps `invert heidi` and
/// `run --qsm-algorithm heidi` on one set of numbers.
/// STAR-QSM's settings from `qsmxt invert star`'s flags. The field strength and echo times are
/// the scan's; qsm-core's dispatcher fills them in.
fn star_params(args: &crate::cli::InvertStarArgs) -> qsm_core::inversion::StarParams {
    let d = qsm_core::inversion::StarParams::default();
    qsm_core::inversion::StarParams {
        pad: args.pad.as_deref().map_or(d.pad, |p| [p[0], p[1], p[2]]),
        te: args.te.map_or(d.te, Into::into),
        ..d
    }
}

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
        // Field strength and echo times are the scan's; the dispatcher fills them in.
        ..Default::default()
    }
}

pub fn execute(cmd: InvertCommand) -> crate::Result<()> {
    use qsm_core::inversion as inv;
    use qsm_core::pipeline::InversionAlgorithm as A;
    // COSMOS and STI take N inputs (and STI writes many outputs), so they do not fit the
    // single-input/single-output shape the rest of this match shares.
    let cmd = match cmd {
        InvertCommand::Cosmos(args) => return run_cosmos(args),
        InvertCommand::Sti(args) => return run_sti(args),
        other => other,
    };

    let (common, (chi_data, field_nifti)) = match cmd {
        InvertCommand::Rts(args) => {
            let d = inv::RtsParams::default();
            let rts = inv::RtsParams {
                delta: args.delta.unwrap_or(d.delta),
                mu: args.mu.unwrap_or(d.mu),
                rho: args.rho.unwrap_or(d.rho),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                lsmr_iter: args.lsmr_iter.unwrap_or(d.lsmr_iter),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { rts, ..config_for(A::Rts) };
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "RTS")?;
            (args.common, out)
        }
        InvertCommand::Tv(args) => {
            let d = inv::TvParams::default();
            let tv = inv::TvParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                rho: args.rho.or(d.rho),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
            };
            let config = qsm_core::pipeline::InversionConfig { tv, ..config_for(A::Tv) };
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "TV")?;
            (args.common, out)
        }
        InvertCommand::Tkd(args) => {
            let d = inv::TkdParams::default();
            let tkd = inv::TkdParams { threshold: args.threshold.unwrap_or(d.threshold) };
            let config = qsm_core::pipeline::InversionConfig { tkd, ..config_for(A::Tkd) };
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "TKD")?;
            (args.common, out)
        }
        InvertCommand::Tsvd(args) => {
            let d = inv::TkdParams::default();
            let tsvd = inv::TkdParams { threshold: args.threshold.unwrap_or(d.threshold) };
            let config = qsm_core::pipeline::InversionConfig { tsvd, ..config_for(A::Tsvd) };
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "TSVD")?;
            (args.common, out)
        }
        InvertCommand::Ilsqr(args) => {
            let d = inv::IlsqrParams::default();
            let ilsqr = inv::IlsqrParams {
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                pad_mm: args.pad_mm.unwrap_or(d.pad_mm),
            };
            ilsqr.validate().map_err(|e| QsmxtError::Config(format!("--pad-mm: {e}")))?;
            let config = qsm_core::pipeline::InversionConfig { ilsqr, ..config_for(A::Ilsqr) };
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "iLSQR")?;
            (args.common, out)
        }
        InvertCommand::Star(args) => {
            let config = qsm_core::pipeline::InversionConfig { star: star_params(&args), ..config_for(A::Star) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::None, config, "STAR-QSM")?;
            (args.common, out)
        }
        InvertCommand::Lsqr(args) => {
            let config = qsm_core::pipeline::InversionConfig {
                lsqr: lsqr_params(&args.lsqr_params), ..config_for(A::Lsqr)
            };
            let out = invert_with(&args.common, args.b0, &[], Magnitude::Combined(&args.magnitude), config, "LSQR")?;
            (args.common, out)
        }
        InvertCommand::Heidi(args) => {
            // HEIDI runs its own LSQR seed (qsm_heidi's), with the --lsqr-* settings.
            let config = qsm_core::pipeline::InversionConfig {
                lsqr: lsqr_params(&args.lsqr_params),
                heidi: heidi_params(&args.heidi_params),
                ..config_for(A::Heidi)
            };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Combined(&args.magnitude), config, "HEIDI")?;
            (args.common, out)
        }
        InvertCommand::Tikhonov(args) => {
            let d = inv::TikhonovParams::default();
            let tikhonov = inv::TikhonovParams { lambda: args.lambda.unwrap_or(d.lambda), ..d };
            let config = qsm_core::pipeline::InversionConfig { tikhonov, ..config_for(A::Tikhonov) };
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "Tikhonov")?;
            (args.common, out)
        }
        InvertCommand::Nltv(args) => {
            let d = inv::NltvParams::default();
            let nltv = inv::NltvParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                mu: args.mu.unwrap_or(d.mu),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                max_iter_newton: args.newton_iter.unwrap_or(d.max_iter_newton),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { nltv, ..config_for(A::Nltv) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Combined(&args.magnitude), config, "NLTV")?;
            (args.common, out)
        }
        InvertCommand::Ndi(args) => {
            // With --auto-stop, unset parameters take FANSI ndi_auto's defaults, else ndi's.
            let auto_stop = args.auto_stop.unwrap_or(false);
            let d = if auto_stop { inv::NdiParams::auto() } else { inv::NdiParams::default() };
            let ndi = inv::NdiParams {
                tau: args.tau.unwrap_or(d.tau),
                alpha: args.alpha.unwrap_or(d.alpha),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                auto_stop,
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { ndi, ..config_for(A::Ndi) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Combined(&args.magnitude), config, "NDI")?;
            (args.common, out)
        }
        InvertCommand::Fansi(args) => {
            let d = inv::FansiParams::default();
            let fansi = inv::FansiParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                alpha0: args.alpha0.or(d.alpha0),
                mu0: args.mu0.or(d.mu0),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { fansi, ..config_for(A::Fansi) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Combined(&args.magnitude), config, "FANSI nlTV")?;
            (args.common, out)
        }
        InvertCommand::FansiTgv(args) => {
            let d = inv::FansiParams::default();
            let fansi = inv::FansiParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                alpha0: args.alpha0.or(d.alpha0),
                mu0: args.mu0.or(d.mu0),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { fansi, ..config_for(A::FansiTgv) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Combined(&args.magnitude), config, "FANSI nlTGV")?;
            (args.common, out)
        }
        InvertCommand::L1qsm(args) => {
            let d = inv::L1QsmParams::default();
            let l1qsm = inv::L1QsmParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                mu3: args.mu3.unwrap_or(d.mu3),
                lambda: args.lambda.unwrap_or(d.lambda),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { l1qsm, ..config_for(A::L1qsm) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Combined(&args.magnitude), config, "L1-QSM")?;
            (args.common, out)
        }
        InvertCommand::Whqsm(args) => {
            let d = inv::WhQsmParams::default();
            let whqsm = inv::WhQsmParams {
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                mu1: args.mu1.unwrap_or(d.mu1),
                mu2: args.mu2.unwrap_or(d.mu2),
                beta: args.beta.unwrap_or(d.beta),
                muh: args.muh.unwrap_or(d.muh),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
                tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
                ..d
            };
            // WH_nlTV's default weight is flat: no magnitude.
            let config = qsm_core::pipeline::InversionConfig { whqsm, ..config_for(A::Whqsm) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::None, config, "WH-QSM")?;
            (args.common, out)
        }
        InvertCommand::Hdqsm(args) => {
            let d = inv::HdQsmParams::default();
            let hdqsm = inv::HdQsmParams {
                alpha_l2: args.alpha_l2.unwrap_or(d.alpha_l2),
                mu1_l2: args.mu1_l2.or(d.mu1_l2),
                mu2: args.mu2.unwrap_or(d.mu2),
                max_iter_l1: args.max_iter_l1.unwrap_or(d.max_iter_l1),
                max_iter_l2: args.max_iter_l2.unwrap_or(d.max_iter_l2),
                tol_update: args.tol_update.unwrap_or(d.tol_update),
            };
            let config = qsm_core::pipeline::InversionConfig { hdqsm, ..config_for(A::Hdqsm) };
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::Combined(&args.magnitude), config, "HD-QSM")?;
            (args.common, out)
        }
        InvertCommand::AmpPe(args) => {
            if args.magnitude.is_empty() {
                warn!("No --magnitude provided for AMP-PE; using uniform weights with no morphology mask");
            }
            let d = inv::AmpPeParams::default();
            let amp_pe = inv::AmpPeParams {
                wave_order: args.wave_order.unwrap_or(d.wave_order),
                nlevel: args.nlevel.unwrap_or(d.nlevel),
                wave_pec: args.wave_pec.unwrap_or(d.wave_pec),
                simulated_te: args.simulated_te.unwrap_or(d.simulated_te),
                max_linearization_ite: args.max_linearization_ite.unwrap_or(d.max_linearization_ite),
                damp_rate_sig: args.damp_rate_sig.unwrap_or(d.damp_rate_sig),
                damp_rate_par: args.damp_rate_par.unwrap_or(d.damp_rate_par),
                max_pe_spar_ite: args.max_pe_spar_ite.unwrap_or(d.max_pe_spar_ite),
                max_pe_est_ite: args.max_pe_est_ite.unwrap_or(d.max_pe_est_ite),
                cvg_thd: args.cvg_thd.unwrap_or(d.cvg_thd),
                tikhonov_beta: args.tikhonov_beta.unwrap_or(d.tikhonov_beta),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { amp_pe, ..config_for(A::AmpPe) };
            let out = invert_with(&args.common, args.b0, &[], Magnitude::Combined(&args.magnitude), config, "AMP-PE")?;
            (args.common, out)
        }
        InvertCommand::Medi(args) => {
            let d = inv::MediParams::default();
            let msmv_default = inv::MsmvParams::default();
            let medi = inv::MediParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                merit: args.merit.unwrap_or(d.merit),
                smv: args.smv.unwrap_or(d.smv),
                smv_radius: args.smv_radius.unwrap_or(d.smv_radius),
                smv_shrink_mask: args.smv_shrink_mask.unwrap_or(d.smv_shrink_mask),
                msmv: args.msmv.then(|| inv::MsmvParams {
                    prefilter: !args.no_msmv_prefilter,
                    maxk: args.msmv_maxk.unwrap_or(msmv_default.maxk),
                    vessel_radius: args.msmv_vessel_radius.or(msmv_default.vessel_radius),
                }),
                data_weighting: args.data_weighting.unwrap_or(d.data_weighting),
                percentage: args.percentage.unwrap_or(d.percentage),
                cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
                cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { medi, ..config_for(A::Medi) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Echoes(&args.magnitude), config, "MEDI")?;
            (args.common, out)
        }
        InvertCommand::Tfi(args) => {
            let d = inv::TfiParams::default();
            let tfi = inv::TfiParams {
                lambda: args.lambda.unwrap_or(d.lambda),
                precond: args.precond.unwrap_or(d.precond),
                data_weighting: args.data_weighting.unwrap_or(d.data_weighting),
                percentage: args.percentage.unwrap_or(d.percentage),
                cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
                cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
                ..d
            };
            let config = qsm_core::pipeline::InversionConfig { tfi, ..config_for(A::Tfi) };
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Echoes(&args.magnitude), config, "TFI")?;
            (args.common, out)
        }
        InvertCommand::Tgv(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let grid = super::common::nifti_grid(&field_nifti);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Dipole inversion (TGV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());
            let d = qsmxt_engine::TgvConfig::default();
            let params = qsmxt_engine::stages::inversion::tgv_params_from_config(&qsmxt_engine::TgvConfig {
                iterations: args.iterations.or(d.iterations),
                erosions: args.erosions.unwrap_or(d.erosions),
                alpha1: args.alpha1.unwrap_or(d.alpha1),
                alpha0: args.alpha0.unwrap_or(d.alpha0),
                step_size: args.step_size.unwrap_or(d.step_size),
            });
            // A field map in ppm, which TGV expresses as phase at the first echo time and this
            // field strength itself (qsm-core has no pipeline entry point for a field-map input).
            let chi = qsm_core::inversion::tgv_qsm(
                qsm_core::inversion::TgvInput::FieldPpm {
                    field: &field_nifti.data, echo_times: &args.scan.echo_times,
                    b0_t: args.scan.field_strength,
                },
                &mask, &grid, &params, bdir, |_, _| {},
            )?;
            (c, (chi, field_nifti))
        }
        InvertCommand::Xqsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Xqsm, "xQSM")?; (args.common, out) }
        InvertCommand::Qsmnet(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Qsmnet, "QSMnet")?; (args.common, out) }
        InvertCommand::QsmnetPlus(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::QsmnetPlus, "QSMnet+")?; (args.common, out) }
        InvertCommand::Autoqsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Autoqsm, "AutoQSM")?; (args.common, out) }
        InvertCommand::Qsmgan(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Qsmgan, "QSMGAN")?; (args.common, out) }
        InvertCommand::Ir2qsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Ir2qsm, "IR2QSM")?; (args.common, out) }
        InvertCommand::Lpcnn(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Lpcnn, "LPCNN")?; (args.common, out) }
        InvertCommand::ModlQsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::ModlQsm, "MoDL-QSM")?; (args.common, out) }
        InvertCommand::Nextqsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Nextqsm, "NeXtQSM")?; (args.common, out) }
        // Returned above; repeated here only to keep the match exhaustive.
        InvertCommand::Cosmos(_) | InvertCommand::Sti(_) => unreachable!("dispatched above"),
    };

    save_nifti(&common.output, &chi_data, &field_nifti)?;
    info!("Susceptibility map saved to {}", common.output.display());
    Ok(())
}

// ─── Multi-orientation reconstructions (COSMOS, STI) ───

/// Read a whitespace-separated N x 3 direction table (`--b0-dirs`).
///
/// Deliberately forgiving about the separator — people produce these with `awk`, a
/// spreadsheet export, or by hand — but strict about the shape, because a row that is not
/// three numbers means the file is not what the user thinks it is.
fn read_b0_dirs_file(path: &std::path::Path) -> crate::Result<Vec<(f64, f64, f64)>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| QsmxtError::Config(format!("{}: {}", path.display(), e)))?;
    let mut dirs = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let vals: Vec<&str> = line.split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .collect();
        if vals.len() != 3 {
            return Err(QsmxtError::Config(format!(
                "{}:{}: expected 3 numbers per line, found {}",
                path.display(), lineno + 1, vals.len()
            )));
        }
        let mut xyz = [0.0f64; 3];
        for (i, v) in vals.iter().enumerate() {
            xyz[i] = v.parse().map_err(|_| QsmxtError::Config(format!(
                "{}:{}: '{}' is not a number", path.display(), lineno + 1, v
            )))?;
        }
        dirs.push((xyz[0], xyz[1], xyz[2]));
    }
    Ok(dirs)
}

/// Everything a multi-orientation reconstruction needs, loaded and validated.
struct MultiOrientInputs {
    fields: Vec<Vec<f64>>,
    mask: Vec<u8>,
    grid: qsm_core::Grid,
    reference: qsm_core::io::NiftiData,
    orientations: Vec<crate::multiorient::Orientation>,
}

/// Load N field maps, resolve their B0 directions, and refuse a set that cannot support the
/// requested reconstruction.
///
/// The grid check is not a formality. COSMOS and STI combine orientations voxel-for-voxel in
/// one k-space, so volumes that merely have the same dimensions but a different affine are
/// being silently treated as aligned when they are not.
fn load_multiorient(
    c: crate::cli::MultiOrientCommonArgs,
    kind: crate::multiorient::MultiOrientKind,
) -> crate::Result<MultiOrientInputs> {
    use crate::multiorient::{check_directions, direction_table, resolve_directions, DirectionSource};

    let n = c.inputs.len();

    // Explicit directions, from either spelling, before anything is loaded — a typo here
    // should not cost the user a minute of I/O.
    let explicit: Option<Vec<(f64, f64, f64)>> = if let Some(ref f) = c.b0_dirs {
        Some(read_b0_dirs_file(f)?)
    } else if !c.b0_direction.is_empty() {
        // clap flattens repeated `--b0-direction x y z` into one 3N-long vector.
        Some(c.b0_direction.chunks(3).map(|d| (d[0], d[1], d[2])).collect())
    } else {
        None
    };
    if let Some(ref dirs) = explicit {
        if dirs.len() != n {
            return Err(QsmxtError::Config(format!(
                "got {} B0 direction(s) for {} input(s) — supply one per --input, in the same order",
                dirs.len(), n
            )));
        }
    }

    let (mask, mask_nifti) = load_mask(&c.mask)?;
    let mut fields = Vec::with_capacity(n);
    let mut affines = Vec::with_capacity(n);
    let mut labels = Vec::with_capacity(n);
    let mut reference: Option<qsm_core::io::NiftiData> = None;

    for path in &c.inputs {
        let nifti = load_nifti(path)?;
        match &reference {
            // Geometry only — the first volume's samples are kept in `fields` like every
            // other orientation's, not duplicated here.
            None => reference = Some(qsm_core::io::NiftiData {
                data: Vec::new(),
                dims: nifti.dims,
                voxel_size: nifti.voxel_size,
                affine: nifti.affine,
                scl_slope: 1.0,
                scl_inter: 0.0,
            }),
            Some(r) => {
                if nifti.dims != r.dims {
                    return Err(QsmxtError::DimensionMismatch(format!(
                        "{} is {:?} but {:?} was expected — every orientation must sit on one common grid",
                        path.display(), nifti.dims, r.dims
                    )));
                }
                // Same dims, different affine: the volumes occupy different physical space,
                // so combining them voxel-for-voxel is meaningless.
                let drift = nifti.affine.iter().zip(r.affine.iter())
                    .map(|(a, b)| (a - b).abs()).fold(0.0f64, f64::max);
                if drift > 1e-3 {
                    return Err(QsmxtError::DimensionMismatch(format!(
                        "{} has a different affine from the first input (max element difference \
                         {drift:.3}) — the orientations are not on a common grid. Co-register \
                         them first; QSMxT does not do that yet",
                        path.display()
                    )));
                }
            }
        }
        affines.push(nifti.affine);
        labels.push(
            path.file_name().map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
        );
        fields.push(nifti.data);
    }
    let reference = reference.expect("clap requires at least one --input");

    if mask_nifti.dims != reference.dims {
        return Err(QsmxtError::DimensionMismatch(format!(
            "mask is {:?} but the orientations are {:?}", mask_nifti.dims, reference.dims
        )));
    }

    let declared: Vec<Option<(f64, f64, f64)>> = match &explicit {
        Some(dirs) => dirs.iter().map(|&d| Some(d)).collect(),
        None => vec![None; n],
    };
    let orientations =
        resolve_directions(&labels, &declared, &affines, DirectionSource::Explicit);

    info!("{} over {} orientation(s):", kind, n);
    for row in direction_table(&orientations) {
        info!("  {row}");
    }

    let check = check_directions(&orientations, kind);
    match (&check.verdict, c.force) {
        (Ok(()), _) => info!("  {}", check.summary()),
        (Err(why), false) => return Err(QsmxtError::Config(why.to_string())),
        (Err(why), true) => warn!("--force: reconstructing anyway, but {why}"),
    }

    let grid = super::common::nifti_grid(&reference);
    Ok(MultiOrientInputs { fields, mask, grid, reference, orientations })
}

fn run_cosmos(args: crate::cli::InvertCosmosArgs) -> crate::Result<()> {
    use crate::multiorient::MultiOrientKind;

    let out = args.output.clone();
    let magnitudes = args.magnitudes.clone();
    let inputs = load_multiorient(args.common, MultiOrientKind::Cosmos)?;
    let bdirs: Vec<_> = inputs.orientations.iter().map(|o| o.b0).collect();

    let d = qsm_core::inversion::CosmosParams::default();
    let params = qsm_core::inversion::CosmosParams {
        lambda: args.lambda.unwrap_or(d.lambda),
        tol: args.tol.unwrap_or(d.tol),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
    };

    let chi = if magnitudes.is_empty() {
        qsm_core::inversion::cosmos(&inputs.fields, &bdirs, &inputs.mask, &inputs.grid, &params)
    } else {
        if magnitudes.len() != inputs.fields.len() {
            return Err(QsmxtError::Config(format!(
                "got {} --magnitude file(s) for {} orientation(s) — supply one per --input, or none",
                magnitudes.len(), inputs.fields.len()
            )));
        }
        let mut weights = Vec::with_capacity(magnitudes.len());
        for path in &magnitudes {
            let m = load_nifti(path)?;
            if m.dims != inputs.reference.dims {
                return Err(QsmxtError::DimensionMismatch(format!(
                    "magnitude {} is {:?} but the orientations are {:?}",
                    path.display(), m.dims, inputs.reference.dims
                )));
            }
            weights.push(m.data);
        }
        info!("Magnitude-weighted COSMOS (conjugate gradient, up to {} iterations)", params.max_iter);
        qsm_core::inversion::cosmos_weighted(
            &inputs.fields, &weights, &bdirs, &inputs.mask, &inputs.grid, &params,
        )
    };

    save_nifti(&out, &chi, &inputs.reference)?;
    info!("Susceptibility map saved to {}", out.display());
    Ok(())
}

fn run_sti(args: crate::cli::InvertStiArgs) -> crate::Result<()> {
    use crate::multiorient::MultiOrientKind;

    let prefix = args.output.clone();
    let inputs = load_multiorient(args.common, MultiOrientKind::Sti)?;
    let bdirs: Vec<_> = inputs.orientations.iter().map(|o| o.b0).collect();

    let d = qsm_core::inversion::StiParams::default();
    let params = qsm_core::inversion::StiParams { lambda: args.lambda.unwrap_or(d.lambda) };

    let tensor = qsm_core::inversion::sti(
        &inputs.fields, &bdirs, &inputs.mask, &inputs.grid, &params,
    );
    let maps = qsm_core::inversion::tensor_maps(&tensor, &inputs.mask);

    // The pipeline's names and layout (`bids::naming`): one 4D tensor with its component order in
    // the sidecar, the scalar MMS and MSA maps, and one 4D eigenvector.
    use crate::bids::naming;
    let r = &inputs.reference;
    let write_4d = |tail: &naming::Tail, volumes: &[Vec<f64>], sidecar: serde_json::Value| -> crate::Result<()> {
        let path = naming::nifti_beside(&prefix, tail);
        let vols: Vec<&[f64]> = volumes.iter().map(|v| v.as_slice()).collect();
        crate::nifti::write::write_volumes_4d(&path, &vols, r.dims, r.voxel_size, &r.affine)?;
        if let Some(js) = crate::bids::entities::sidecar_path(&path) {
            std::fs::write(js, serde_json::to_string_pretty(&sidecar).unwrap_or_default())?;
        }
        Ok(())
    };
    write_4d(&naming::sti_tensor(), &tensor.components,
             serde_json::Value::Object(crate::pipeline::multiorient::tensor_sidecar_fields()))?;
    save_nifti(&naming::nifti_beside(&prefix, &naming::sti_mms()), &maps.mms, r)?;
    save_nifti(&naming::nifti_beside(&prefix, &naming::sti_msa()), &maps.msa, r)?;
    write_4d(&naming::sti_eigenvector(), &maps.pev, crate::pipeline::multiorient::eigenvector_sidecar())?;
    info!(
        "Tensor, MMS, MSA and principal eigenvector saved with prefix {}",
        prefix.display()
    );
    info!("MMS is the orientation-independent counterpart of a scalar QSM map; the eigenvector is in voxel coordinates of the common grid");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{heidi_params, lsqr_params, star_params};
    use crate::cli::{HeidiParamArgs, LsqrParamArgs};

    /// With no flags set, the `invert` surface must reproduce qsm-core's own defaults — the same
    /// parameters `qsmxt run --qsm-algorithm lsqr` would build. A hand-written `unwrap_or` chain
    /// is exactly where a default silently drifts from the library's.
    #[test]
    fn unset_flags_give_the_qsm_core_defaults() {
        let d = qsm_core::inversion::LsqrQsmParams::default();
        let p = lsqr_params(&LsqrParamArgs::default());
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
        assert_eq!(p.te, h.te);
        let (a, b) = (p.denoise.unwrap(), h.denoise.unwrap());
        assert_eq!((a.iterations, a.time_step, a.conductance), (b.iterations, b.time_step, b.conductance));
    }

    /// `qsmxt invert star` with no flags gives qsm-core's STAR-QSM defaults; `--pad` and `--te`
    /// reach its parameters.
    #[test]
    fn star_flags_map_to_its_params() {
        use clap::Parser;
        let parse = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "invert", "star", "f.nii", "-m", "m.nii", "-o", "o.nii",
                                "--field-strength", "3", "--echo-times", "0.01", "0.02"];
            argv.extend(extra);
            match crate::cli::Cli::try_parse_from(argv).unwrap().command {
                crate::cli::Command::Invert { command: crate::cli::InvertCommand::Star(a) } => a,
                _ => panic!("expected invert star"),
            }
        };
        let (p, d) = (star_params(&parse(&[])), qsm_core::inversion::StarParams::default());
        assert_eq!((p.pad, p.te), (d.pad, d.te));
        let p = star_params(&parse(&["--pad", "2", "4", "6", "--te", "spacing"]));
        assert_eq!((p.pad, p.te), ([2, 4, 6], qsm_core::EchoTime::Spacing));
    }

    /// `--lsqr-residual-weighting` is `Option<f64>` in qsm-core because `None` means
    /// "scale with field strength" — collapsing it to a concrete number would silently pin the
    /// weight to 3T behaviour on every other scanner.
    #[test]
    fn an_unset_residual_weighting_stays_none() {
        assert_eq!(lsqr_params(&LsqrParamArgs::default()).residual_weighting, None);
        let explicit = LsqrParamArgs { lsqr_residual_weighting: Some(0.0), ..Default::default() };
        assert_eq!(lsqr_params(&explicit).residual_weighting, Some(0.0));
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
}
