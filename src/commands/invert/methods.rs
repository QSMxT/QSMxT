//! The single-input inversions: each method's settings flags (`qsmxt run`'s) applied to QSMxT's
//! configuration, and the inversion config `qsmxt run` would build from it
//! ([`qsmxt_engine::to_pipeline_stages`]), so a standalone inversion runs as the same step of a
//! run does.

use log::{info, warn};
use super::common::{load_nifti, load_mask};
use crate::cli::{
    InvertAmpPeArgs, InvertCommonArgs, InvertFansiArgs, InvertFansiTgvArgs, InvertHdqsmArgs,
    InvertHeidiArgs, InvertIlsqrArgs, InvertL1qsmArgs, InvertLsqrArgs, InvertMediArgs,
    InvertNdiArgs, InvertNltvArgs, InvertRtsArgs, InvertStarArgs, InvertTfiArgs, InvertTgvArgs,
    InvertTikhonovArgs, InvertTkdArgs, InvertTsvdArgs, InvertTvArgs, InvertWhqsmArgs,
};
use qsmxt_engine::QsmAlgorithm as Q;
use qsmxt_io::error::QsmxtError;

use super::{config, heidi_on_seed, invert_with, require_magnitude, Magnitude};

/// What an inversion gives back: its common args, and the χ map with the field it was made from.
pub(super) type Inverted = (InvertCommonArgs, (Vec<f64>, qsm_core::io::NiftiData));

/// The inversion config `qsmxt run` builds from `cfg`.
fn inversion(cfg: &qsmxt_engine::PipelineConfig) -> qsm_core::pipeline::InversionConfig {
    qsmxt_engine::to_pipeline_stages(cfg).2
}

/// A method that takes the local field alone.
macro_rules! field_only {
    ($($name:ident($args:ty) = $alg:ident, $label:literal;)*) => {$(
        pub(super) fn $name(args: $args) -> qsmxt_io::Result<Inverted> {
            let cfg = config(&args.common, Q::$alg, &args.settings.0)?;
            let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, inversion(&cfg), $label)?;
            Ok((args.common, out))
        }
    )*};
}

field_only! {
    rts(InvertRtsArgs) = Rts, "RTS";
    tv(InvertTvArgs) = Tv, "TV";
    tkd(InvertTkdArgs) = Tkd, "TKD";
    tsvd(InvertTsvdArgs) = Tsvd, "TSVD";
    tikhonov(InvertTikhonovArgs) = Tikhonov, "Tikhonov";
}

pub(super) fn ilsqr(args: InvertIlsqrArgs) -> qsmxt_io::Result<Inverted> {
    let cfg = config(&args.common, Q::Ilsqr, &args.settings.0)?;
    let config = inversion(&cfg);
    config.ilsqr.validate().map_err(|e| QsmxtError::Config(format!("--ilsqr-pad-mm: {e}")))?;
    let out = invert_with(&args.common, f64::NAN, &[], Magnitude::None, config, "iLSQR")?;
    Ok((args.common, out))
}

pub(super) fn star(args: InvertStarArgs) -> qsmxt_io::Result<Inverted> {
    let cfg = config(&args.common, Q::Star, &args.settings.0)?;
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::None, inversion(&cfg), "STAR-QSM")?;
    Ok((args.common, out))
}

pub(super) fn lsqr(args: InvertLsqrArgs) -> qsmxt_io::Result<Inverted> {
    require_magnitude(&args.magnitude, "LSQR")?;
    let cfg = config(&args.common, Q::Lsqr, &args.settings.0)?;
    let out = invert_with(&args.common, args.b0, &[], Magnitude::Combined(&args.magnitude), inversion(&cfg), "LSQR")?;
    Ok((args.common, out))
}

pub(super) fn heidi(args: InvertHeidiArgs) -> qsmxt_io::Result<Inverted> {
    // LSQR+HEIDI: HEIDI runs its own LSQR seed (qsm_heidi's), with the --lsqr-*
    // settings; or, with --seed, fills the cone of a map another inversion made.
    let mut cfg = config(&args.common, Q::Lsqr, &args.settings.0)?;
    cfg.inversion.heidi.enabled = true;
    let config = inversion(&cfg);
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

/// A method that takes the local field, the scan's field strength and echo times, and the
/// combined magnitude as its data weight.
macro_rules! weighted {
    ($($name:ident($args:ty) = $alg:ident, $label:literal;)*) => {$(
        pub(super) fn $name(args: $args) -> qsmxt_io::Result<Inverted> {
            let cfg = config(&args.common, Q::$alg, &args.settings.0)?;
            let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
                Magnitude::Combined(&args.magnitude), inversion(&cfg), $label)?;
            Ok((args.common, out))
        }
    )*};
}

weighted! {
    nltv(InvertNltvArgs) = Nltv, "NLTV";
    ndi(InvertNdiArgs) = Ndi, "NDI";
    fansi(InvertFansiArgs) = Fansi, "FANSI nlTV";
    fansi_tgv(InvertFansiTgvArgs) = FansiTgv, "FANSI nlTGV";
    l1qsm(InvertL1qsmArgs) = L1qsm, "L1-QSM";
}

pub(super) fn whqsm(args: InvertWhqsmArgs) -> qsmxt_io::Result<Inverted> {
    let cfg = config(&args.common, Q::Whqsm, &args.settings.0)?;
    let config = inversion(&cfg);
    // WH_nlTV's default weight is flat: no magnitude. A magnitude weight needs one.
    let magnitude = if config.whqsm.weight.needs_magnitude() {
        if args.magnitude.is_empty() {
            return Err(QsmxtError::Config(format!(
                "WH-QSM: --whqsm-weight {} needs --magnitude", cfg.inversion.whqsm.weight)));
        }
        Magnitude::Combined(&args.magnitude)
    } else {
        Magnitude::None
    };
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        magnitude, config, "WH-QSM")?;
    Ok((args.common, out))
}

pub(super) fn hdqsm(args: InvertHdqsmArgs) -> qsmxt_io::Result<Inverted> {
    let cfg = config(&args.common, Q::Hdqsm, &args.settings.0)?;
    let out = invert_with(&args.common, f64::NAN, &args.echo_times, Magnitude::Echoes(&args.magnitude), inversion(&cfg), "HD-QSM")?;
    Ok((args.common, out))
}

pub(super) fn amp_pe(args: InvertAmpPeArgs) -> qsmxt_io::Result<Inverted> {
    if args.magnitude.is_empty() {
        warn!(target: super::LOG_TARGET, "No --magnitude provided for AMP-PE; using uniform weights with no morphology mask");
    }
    let cfg = config(&args.common, Q::AmpPe, &args.settings.0)?;
    let out = invert_with(&args.common, args.b0, &[], Magnitude::Combined(&args.magnitude), inversion(&cfg), "AMP-PE")?;
    Ok((args.common, out))
}

pub(super) fn medi(args: InvertMediArgs) -> qsmxt_io::Result<Inverted> {
    let cfg = config(&args.common, Q::Medi, &args.settings.0)?;
    if cfg.inversion.medi.csf && args.scan.echo_times.len() < 3 {
        warn!(target: super::LOG_TARGET, "MEDI's CSF zero reference (MEDI+0) needs at least 3 echoes (it finds the CSF from R2*); \
               with {} MEDI runs without it", args.scan.echo_times.len());
    }
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Echoes(&args.magnitude), inversion(&cfg), "MEDI")?;
    Ok((args.common, out))
}

pub(super) fn tfi(args: InvertTfiArgs) -> qsmxt_io::Result<Inverted> {
    let cfg = config(&args.common, Q::Tfi, &args.settings.0)?;
    let mut config = inversion(&cfg);
    // The original implementation's kernel and arithmetic, for comparison with it: not settings.
    if let Some(k) = args.kernel { config.tfi.kernel = k.into(); }
    if let Some(p) = args.precision { config.tfi.precision = p.into(); }
    let out = invert_with(&args.common, args.scan.field_strength, &args.scan.echo_times,
        Magnitude::Echoes(&args.magnitude), config, "TFI")?;
    Ok((args.common, out))
}

pub(super) fn tgv(args: InvertTgvArgs) -> qsmxt_io::Result<Inverted> {
    let cfg = config(&args.common, Q::Tgv, &args.settings.0)?;
    let c = args.common;
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let grid = super::common::nifti_grid(&field_nifti);
    let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
    info!(target: super::LOG_TARGET, "Dipole inversion (TGV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());
    let params = qsm_core::inversion::TgvParams {
        // QuantitativeSusceptibilityMappingTGV.jl's choices, for comparison with it: not settings.
        reference_divergence: args.reference_divergence,
        reference_edge_erosion: args.reference_edge_erosion,
        reference_output_box: args.reference_output_box,
        ..qsmxt_engine::stages::inversion::tgv_params_from_config(&cfg.inversion.tgv)
    };
    // A field map in ppm, which TGV expresses as phase at the echo time --tgv-te picks and
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
