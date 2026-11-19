use clap::Parser;
use std::path::PathBuf;

use super::*;

#[derive(Parser, Debug)]
pub struct InvertTkdArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Threshold on |D|; larger is smoother and underestimates χ more (default: 0.25)
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Zero-padding before the FFTs in voxels, X Y Z: at least this many, rounded up to an
    /// FFT-efficient size; -1 leaves an axis unpadded (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"], allow_negative_numbers = true)]
    pub pad: Option<Vec<i64>>,
}

#[derive(Parser, Debug)]
pub struct InvertTsvdArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Threshold on |D| (default: 0.15)
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Zero-padding before the FFTs in voxels, X Y Z: at least this many, rounded up to an
    /// FFT-efficient size; -1 leaves an axis unpadded (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"], allow_negative_numbers = true)]
    pub pad: Option<Vec<i64>>,
}

#[derive(Parser, Debug)]
pub struct InvertIlsqrArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations of each LSQR solve (default: 100)
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Zero-padding in mm per side
    #[arg(long, value_name = "MM")]
    pub pad_mm: Option<f64>,
    /// Arithmetic of the two LSQR solves: double, or single as the original implementation
    /// (default: double)
    #[arg(long, value_name = "PRECISION", value_parser = parse_ilsqr_precision)]
    pub precision: Option<qsmxt_engine::IlsqrPrecision>,
}

/// Minimally regularised LSQR. The residual-field weight (`--lsqr-residual-weighting`) is the only
/// thing regularising the solve, so on its own this streaks; it exists mainly as HEIDI's seed.
/// Flags carry the `--lsqr-` prefix so they read the same here as under `qsmxt run`.
#[derive(Parser, Debug)]
pub struct InvertLsqrArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Magnitude NIfTI, LSQR's row weight as the HEIDI package's (its scale does not change the
    /// solution): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// B0 field strength in Tesla — sets the default residual weighting
    #[arg(long, default_value_t = 3.0)]
    pub b0: f64,
    #[command(flatten)]
    pub lsqr_params: LsqrParamArgs,
}

/// LSQR+HEIDI, HEIDI as published: it runs its own LSQR seed, so the `--lsqr-*` flags apply here
/// too. With `--seed`, HEIDI instead reduces the streaking of a map another inversion made of the
/// same field (`qsmxt run --heidi`'s step). Its result depends on the field's scale in phase units,
/// so it takes the echo times and field strength.
#[derive(Parser, Debug)]
pub struct InvertHeidiArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, the row weight of the LSQR seed as the HEIDI package's (not needed with
    /// --seed): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined
    #[arg(long, num_args = 1.., required_unless_present = "seed")]
    pub magnitude: Vec<PathBuf>,
    /// A χ map (ppm) another inversion made of this field, to fill the cone of instead of
    /// running LSQR (`--lsqr-*` and `--magnitude` then do nothing). HEIDI suits a seed with little
    /// or no smoothing (TKD and TSVD with a threshold no larger than the cone threshold, small-λ
    /// Tikhonov, NDI, RTS); a method that smooths its result also changes the parts of the data
    /// HEIDI keeps as they are, and HEIDI cannot undo that
    #[arg(long)]
    pub seed: Option<PathBuf>,
    #[command(flatten)]
    pub lsqr_params: LsqrParamArgs,
    #[command(flatten)]
    pub heidi_params: HeidiParamArgs,
}

#[derive(Parser, Debug)]
pub struct InvertNdiArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Gradient-descent step size (tau)
    #[arg(long)]
    pub tau: Option<f64>,
    /// L2 regularization weight (alpha)
    #[arg(long)]
    pub alpha: Option<f64>,
    /// Max iterations (the maximum, with --auto-stop)
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto)
    #[arg(long)]
    pub auto_stop: Option<bool>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: first)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
    /// Magnitude-weight normaliser: global-max (the magnitude's maximum, as FANSI; default) or
    /// mask-p999 (its 99.9th percentile inside the mask; more consistent in vivo)
    #[arg(long, value_name = "NORM", value_parser = parse_ndi_weight_norm)]
    pub weight_norm: Option<qsmxt_engine::NdiWeightNorm>,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo;
    /// several echoes are RSS-combined
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
}
