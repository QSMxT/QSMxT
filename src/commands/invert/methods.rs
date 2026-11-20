use log::{info, warn};
use super::common::{load_nifti, load_mask};
use crate::cli::{
    InvertAmpPeArgs, InvertCommonArgs, InvertFansiArgs, InvertFansiTgvArgs, InvertHdqsmArgs,
    InvertHeidiArgs, InvertIlsqrArgs, InvertL1qsmArgs, InvertLsqrArgs, InvertMediArgs,
    InvertNdiArgs, InvertNltvArgs, InvertRtsArgs, InvertStarArgs, InvertTfiArgs, InvertTgvArgs,
    InvertTikhonovArgs, InvertTkdArgs, InvertTsvdArgs, InvertTvArgs, InvertWhqsmArgs,
};
use qsmxt_io::error::QsmxtError;
use qsm_core::inversion as inv;
use qsm_core::pipeline::InversionAlgorithm as A;

use super::{config_for, heidi_on_seed, heidi_params, invert_with, lsqr_params, pad3, require_magnitude, star_params, Magnitude};

/// What an inversion gives back: its common args, and the χ map with the field it was made from.
pub(super) type Inverted = (InvertCommonArgs, (Vec<f64>, qsm_core::io::NiftiData));

pub(super) fn rts(args: InvertRtsArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::RtsParams::default();
    let rts = inv::RtsParams {
        delta: args.delta.unwrap_or(d.delta),
        mu: args.mu.unwrap_or(d.mu),
        rho: args.rho.unwrap_or(d.rho),
        tol: args.tol.unwrap_or(d.tol),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        lsmr_iter: args.lsmr_iter.unwrap_or(d.lsmr_iter),
        tau: args.tau.unwrap_or(d.tau),
        gamma: args.gamma.unwrap_or(d.gamma),
        pad: pad3(args.pad.as_deref(), d.pad),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { rts, ..config_for(A::Rts) };
    let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "RTS")?;
    Ok((args.common, out))
}

pub(super) fn tv(args: InvertTvArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::TvParams::default();
    let tv = inv::TvParams {
        lambda: args.lambda.unwrap_or(d.lambda),
        rho: args.rho.map_or(d.rho, crate::cli::OrAuto::value),
        tol: args.tol.unwrap_or(d.tol),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        pad: pad3(args.pad.as_deref(), d.pad),
    };
    let config = qsm_core::pipeline::InversionConfig { tv, ..config_for(A::Tv) };
    let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "TV")?;
    Ok((args.common, out))
}

pub(super) fn tkd(args: InvertTkdArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::TkdParams::default();
    let tkd = inv::TkdParams { threshold: args.threshold.unwrap_or(d.threshold), pad: pad3(args.pad.as_deref(), d.pad) };
    let config = qsm_core::pipeline::InversionConfig { tkd, ..config_for(A::Tkd) };
    let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "TKD")?;
    Ok((args.common, out))
}

pub(super) fn tsvd(args: InvertTsvdArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::TsvdParams::default();
    let tsvd = inv::TsvdParams { threshold: args.threshold.unwrap_or(d.threshold), pad: pad3(args.pad.as_deref(), d.pad) };
    let config = qsm_core::pipeline::InversionConfig { tsvd, ..config_for(A::Tsvd) };
    let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "TSVD")?;
    Ok((args.common, out))
}

pub(super) fn ilsqr(args: InvertIlsqrArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::IlsqrParams::default();
    let ilsqr = inv::IlsqrParams {
        tol: args.tol.unwrap_or(d.tol),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        pad_mm: args.pad_mm.unwrap_or(d.pad_mm),
        precision: args.precision.map_or(d.precision, Into::into),
    };
    ilsqr.validate().map_err(|e| QsmxtError::Config(format!("--pad-mm: {e}")))?;
    let config = qsm_core::pipeline::InversionConfig { ilsqr, ..config_for(A::Ilsqr) };
    let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "iLSQR")?;
    Ok((args.common, out))
}

pub(super) fn star(args: InvertStarArgs) -> qsmxt_io::Result<Inverted> {
    let config = qsm_core::pipeline::InversionConfig { star: star_params(&args), ..config_for(A::Star) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::None, config, "STAR-QSM")?;
    Ok((args.common, out))
}

pub(super) fn lsqr(args: InvertLsqrArgs) -> qsmxt_io::Result<Inverted> {
    require_magnitude(&args.magnitude, "LSQR")?;
    let config = qsm_core::pipeline::InversionConfig {
        lsqr: lsqr_params(&args.lsqr_params), ..config_for(A::Lsqr)
    };
    let out = invert_with(&args.common, args.b0, &[], Magnitude::Combined(&args.magnitude), config, "LSQR")?;
    Ok((args.common, out))
}

pub(super) fn heidi(args: InvertHeidiArgs) -> qsmxt_io::Result<Inverted> {
    // LSQR+HEIDI: HEIDI runs its own LSQR seed (qsm_heidi's), with the --lsqr-*
    // settings; or, with --seed, fills the cone of a map another inversion made.
    let config = qsm_core::pipeline::InversionConfig {
        lsqr: lsqr_params(&args.lsqr_params),
        apply_heidi: true,
        heidi: heidi_params(&args.heidi_params),
        ..config_for(A::Lsqr)
    };
    if args.seed.is_none() {
        require_magnitude(&args.magnitude, "HEIDI's LSQR seed")?;
    }
    let out = match &args.seed {
        None => invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
            Magnitude::Combined(&args.magnitude), config, "HEIDI")?,
        Some(seed) => heidi_on_seed(&args.common, seed, args.scan.field_strength, &args.scan.echo_times, &config)?,
    };
    Ok((args.common, out))
}

pub(super) fn tikhonov(args: InvertTikhonovArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::TikhonovParams::default();
    let tikhonov = inv::TikhonovParams {
        lambda: args.lambda.unwrap_or(d.lambda), pad: pad3(args.pad.as_deref(), d.pad), ..d
    };
    let config = qsm_core::pipeline::InversionConfig { tikhonov, ..config_for(A::Tikhonov) };
    let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "Tikhonov")?;
    Ok((args.common, out))
}

pub(super) fn nltv(args: InvertNltvArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::NltvParams::default();
    let nltv = inv::NltvParams {
        lambda: args.lambda.unwrap_or(d.lambda),
        rho: args.rho.map_or(d.rho, crate::cli::OrAuto::value),
        mu: args.mu.unwrap_or(d.mu),
        tol: args.tol.unwrap_or(d.tol),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        tol_newton: args.tol_newton.unwrap_or(d.tol_newton),
        max_iter_newton: args.newton_iter.unwrap_or(d.max_iter_newton),
        te: args.te.map_or(d.te, Into::into),
        pad: pad3(args.pad.as_deref(), d.pad),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { nltv, ..config_for(A::Nltv) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Combined(&args.magnitude), config, "NLTV")?;
    Ok((args.common, out))
}

pub(super) fn ndi(args: InvertNdiArgs) -> qsmxt_io::Result<Inverted> {
    // With --auto-stop, unset parameters take FANSI ndi_auto's defaults, else ndi's.
    let auto_stop = args.auto_stop.unwrap_or(false);
    let d = if auto_stop { inv::NdiParams::auto() } else { inv::NdiParams::default() };
    let ndi = inv::NdiParams {
        tau: args.tau.unwrap_or(d.tau),
        alpha: args.alpha.unwrap_or(d.alpha),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        auto_stop,
        te: args.te.map_or(d.te, Into::into),
        weight_norm: args.weight_norm.map_or(d.weight_norm, Into::into),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { ndi, ..config_for(A::Ndi) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Combined(&args.magnitude), config, "NDI")?;
    Ok((args.common, out))
}

pub(super) fn fansi(args: InvertFansiArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::FansiParams::default();
    let fansi = inv::FansiParams {
        alpha1: args.alpha1.unwrap_or(d.alpha1),
        mu1: args.mu1.map_or(d.mu1, crate::cli::OrAuto::value),
        mu2: args.mu2.unwrap_or(d.mu2),
        alpha0: args.alpha0.map_or(d.alpha0, crate::cli::OrAuto::value),
        mu0: args.mu0.map_or(d.mu0, crate::cli::OrAuto::value),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        tol_update: args.tol_update.unwrap_or(d.tol_update),
        tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
        te: args.te.map_or(d.te, Into::into),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { fansi, ..config_for(A::Fansi) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Combined(&args.magnitude), config, "FANSI nlTV")?;
    Ok((args.common, out))
}

pub(super) fn fansi_tgv(args: InvertFansiTgvArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::FansiParams::default();
    let fansi = inv::FansiParams {
        alpha1: args.alpha1.unwrap_or(d.alpha1),
        mu1: args.mu1.map_or(d.mu1, crate::cli::OrAuto::value),
        mu2: args.mu2.unwrap_or(d.mu2),
        alpha0: args.alpha0.map_or(d.alpha0, crate::cli::OrAuto::value),
        mu0: args.mu0.map_or(d.mu0, crate::cli::OrAuto::value),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        tol_update: args.tol_update.unwrap_or(d.tol_update),
        tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
        te: args.te.map_or(d.te, Into::into),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { fansi, ..config_for(A::FansiTgv) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Combined(&args.magnitude), config, "FANSI nlTGV")?;
    Ok((args.common, out))
}

pub(super) fn l1qsm(args: InvertL1qsmArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::L1QsmParams::default();
    let l1qsm = inv::L1QsmParams {
        alpha1: args.alpha1.unwrap_or(d.alpha1),
        mu1: args.mu1.map_or(d.mu1, crate::cli::OrAuto::value),
        mu2: args.mu2.unwrap_or(d.mu2),
        mu3: args.mu3.unwrap_or(d.mu3),
        lambda: args.lambda.unwrap_or(d.lambda),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        tol_update: args.tol_update.unwrap_or(d.tol_update),
        tol_delta: args.tol_delta.unwrap_or(d.tol_delta),
        te: args.te.map_or(d.te, Into::into),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { l1qsm, ..config_for(A::L1qsm) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Combined(&args.magnitude), config, "L1-QSM")?;
    Ok((args.common, out))
}

pub(super) fn whqsm(args: InvertWhqsmArgs) -> qsmxt_io::Result<Inverted> {
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
        weight: args.weight.map_or(d.weight, Into::into),
        te: args.te.map_or(d.te, Into::into),
        ..d
    };
    // WH_nlTV's default weight is flat: no magnitude. A magnitude weight needs one.
    let magnitude = if whqsm.weight.needs_magnitude() {
        if args.magnitude.is_empty() {
            return Err(QsmxtError::Config(format!(
                "WH-QSM: --weight {} needs --magnitude", args.weight.unwrap_or_default())));
        }
        Magnitude::Combined(&args.magnitude)
    } else {
        Magnitude::None
    };
    let config = qsm_core::pipeline::InversionConfig { whqsm, ..config_for(A::Whqsm) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        magnitude, config, "WH-QSM")?;
    Ok((args.common, out))
}

pub(super) fn hdqsm(args: InvertHdqsmArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::HdQsmParams::default();
    let hdqsm = inv::HdQsmParams {
        alpha_l2: args.alpha_l2.unwrap_or(d.alpha_l2),
        mu1_l2: args.mu1_l2.map_or(d.mu1_l2, crate::cli::OrAuto::value),
        mu2: args.mu2.unwrap_or(d.mu2),
        max_iter_l1: args.max_iter_l1.unwrap_or(d.max_iter_l1),
        max_iter_l2: args.max_iter_l2.unwrap_or(d.max_iter_l2),
        tol_update: args.tol_update.unwrap_or(d.tol_update),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { hdqsm, ..config_for(A::Hdqsm) };
    let out = invert_with(&args.common, f64::NAN, &args.echo_times, Magnitude::Echoes(&args.magnitude), config, "HD-QSM")?;
    Ok((args.common, out))
}

pub(super) fn amp_pe(args: InvertAmpPeArgs) -> qsmxt_io::Result<Inverted> {
    if args.magnitude.is_empty() {
        warn!(target: super::LOG_TARGET, "No --magnitude provided for AMP-PE; using uniform weights with no morphology mask");
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
    Ok((args.common, out))
}

pub(super) fn medi(args: InvertMediArgs) -> qsmxt_io::Result<Inverted> {
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
        csf: args.csf.unwrap_or(d.csf),
        lambda_csf: args.lambda_csf.unwrap_or(d.lambda_csf),
        csf_r2s_threshold: args.csf_r2s_threshold.unwrap_or(d.csf_r2s_threshold),
        csf_erode: args.csf_erode.unwrap_or(d.csf_erode),
        zeropad: args.zeropad.as_deref().map_or(d.zeropad, |z| (z[0], z[1], z[2])),
        cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
        cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        tol: args.tol.unwrap_or(d.tol),
        te: args.te.map_or(d.te, Into::into),
        ..d
    };
    if medi.csf && args.scan.echo_times.len() < 3 {
        warn!(target: super::LOG_TARGET, "MEDI's CSF zero reference (MEDI+0) needs at least 3 echoes (it finds the CSF from R2*); \
               with {} MEDI runs without it", args.scan.echo_times.len());
    }
    let config = qsm_core::pipeline::InversionConfig { medi, ..config_for(A::Medi) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Echoes(&args.magnitude), config, "MEDI")?;
    Ok((args.common, out))
}

pub(super) fn tfi(args: InvertTfiArgs) -> qsmxt_io::Result<Inverted> {
    let d = inv::TfiParams::default();
    let tfi = inv::TfiParams {
        lambda: args.lambda.unwrap_or(d.lambda),
        precond: args.precond.unwrap_or(d.precond),
        merit: args.merit.unwrap_or(d.merit),
        linear: args.linear.unwrap_or(d.linear),
        data_weighting: args.data_weighting.unwrap_or(d.data_weighting),
        percentage: args.percentage.unwrap_or(d.percentage),
        cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
        cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
        tol: args.tol.unwrap_or(d.tol),
        te: args.te.map_or(d.te, Into::into),
        kernel: args.kernel.map_or(d.kernel, Into::into),
        precision: args.precision.map_or(d.precision, Into::into),
        ..d
    };
    let config = qsm_core::pipeline::InversionConfig { tfi, ..config_for(A::Tfi) };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Echoes(&args.magnitude), config, "TFI")?;
    Ok((args.common, out))
}

pub(super) fn tgv(args: InvertTgvArgs) -> qsmxt_io::Result<Inverted> {
    let c = args.common;
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let grid = super::common::nifti_grid(&field_nifti);
    let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
    info!(target: super::LOG_TARGET, "Dipole inversion (TGV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());
    let d = qsmxt_engine::TgvConfig::default();
    let params = qsmxt_engine::stages::inversion::tgv_params_from_config(&qsmxt_engine::TgvConfig {
        iterations: args.iterations.or(d.iterations),
        erosions: args.erosions.unwrap_or(d.erosions),
        alpha1: args.alpha1.unwrap_or(d.alpha1),
        alpha0: args.alpha0.unwrap_or(d.alpha0),
        step_size: args.step_size.unwrap_or(d.step_size),
        te: args.te.unwrap_or(d.te),
    });
    let params = qsm_core::inversion::TgvParams {
        reference_divergence: args.reference_divergence,
        reference_edge_erosion: args.reference_edge_erosion,
        reference_output_box: args.reference_output_box,
        ..params
    };
    // A field map in ppm, which TGV expresses as phase at the echo time --te picks and
    // this field strength itself (qsm-core has no pipeline entry point for a field-map input).
    let chi = qsm_core::inversion::tgv_qsm(
        qsm_core::inversion::TgvInput::FieldPpm {
            field: &field_nifti.data, echo_times: &args.scan.echo_times,
            b0_t: args.scan.field_strength,
        },
        &mask, &grid, &params, bdir, |_, _| {},
    )?;
    Ok((c, (chi, field_nifti)))
}
