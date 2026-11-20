use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use super::*;

mod least_squares;
mod regularised;

pub use least_squares::*;
pub use regularised::*;

// ── Invert ──

crate::flag_set! {
    /// The deep-learning inversions' overlap-tiling, under its `qsmxt run` names.
    TileFlags = ["inversion.tile_size", "inversion.tile_halo"], short = false;
    RtsFlags = ["inversion.rts."];
    TvFlags = ["inversion.tv."];
    TkdFlags = ["inversion.tkd."];
    TsvdFlags = ["inversion.tsvd."];
    TgvFlags = ["inversion.tgv."];
    TikhonovFlags = ["inversion.tikhonov."];
    NltvFlags = ["inversion.nltv."], aliases = [("nltv-max-iter-newton", "newton-iter")];
    MediFlags = ["inversion.medi."];
    TfiFlags = ["inversion.tfi."];
    IlsqrFlags = ["inversion.ilsqr."];
    StarFlags = ["inversion.star."];
    NdiFlags = ["inversion.ndi."];
    /// FANSI's, for nlTV and nlTGV.
    FansiFlags = ["inversion.fansi."];
    L1qsmFlags = ["inversion.l1qsm."];
    WhqsmFlags = ["inversion.whqsm."];
    HdqsmFlags = ["inversion.hdqsm."];
    AmpPeFlags = ["inversion.amp_pe."];
    /// LSQR's, which have always had their `--lsqr-` names here.
    LsqrFlags = ["inversion.lsqr."], short = false;
    /// HEIDI's and its LSQR seed's (HEIDI is on: `--heidi` is not one).
    HeidiFlags = ["inversion.lsqr.", "inversion.heidi.", "!inversion.heidi.enabled"], short = false;
}

#[derive(Args, Debug, Clone)]
pub struct InvertCommonArgs {
    /// Input local field NIfTI file
    pub input: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output susceptibility map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// Deep-learning overlap-tiling (opt-in; ignored by classical algorithms).
    #[command(flatten)]
    pub tiling: crate::cli_settings::SettingsOf<TileFlags>,
}

/// The scan parameters a standalone step takes from its acquisition: what `qsmxt run` reads from
/// the sidecars. The methods that work in radians convert the field with them, at the echo time
/// their reference picks.
#[derive(Args, Debug, Clone)]
pub struct ScanParamArgs {
    /// B0 field strength in Tesla
    #[arg(long)]
    pub field_strength: f64,
    /// Echo times in seconds, every echo's, in order
    #[arg(long, num_args = 1.., required = true, alias = "echo-time")]
    pub echo_times: Vec<f64>,
}


#[derive(Subcommand, Debug)]
pub enum InvertCommand {
    /// Rapid Two-Step (RTS)
    Rts(InvertRtsArgs),
    /// Total Variation (ADMM)
    Tv(InvertTvArgs),
    /// Truncated K-space Division
    Tkd(InvertTkdArgs),
    /// Truncated Singular Value Decomposition
    Tsvd(InvertTsvdArgs),
    /// Total Generalized Variation
    Tgv(InvertTgvArgs),
    /// Tikhonov regularization
    Tikhonov(InvertTikhonovArgs),
    /// Nonlinear Total Variation
    Nltv(InvertNltvArgs),
    /// Morphology Enabled Dipole Inversion
    Medi(InvertMediArgs),
    /// Total Field Inversion (preconditioned). Input is the TOTAL field (not a local field).
    Tfi(InvertTfiArgs),
    /// Iterative Least Squares QR
    Ilsqr(InvertIlsqrArgs),
    /// STAR-QSM (Streaking Artifact Reduction): sparse inversion from a k-space initial guess
    Star(InvertStarArgs),
    /// Minimally regularised LSQR
    Lsqr(InvertLsqrArgs),
    /// LSQR+HEIDI (Homogeneity Enabled Incremental Dipole Inversion), or HEIDI on another
    /// inversion's map (`--seed`)
    Heidi(InvertHeidiArgs),
    /// Nonlinear Dipole Inversion
    Ndi(InvertNdiArgs),
    /// FANSI Nonlinear Total Variation
    Fansi(InvertFansiArgs),
    /// FANSI Nonlinear Total Generalized Variation
    #[command(name = "fansi-tgv")]
    FansiTgv(InvertFansiTgvArgs),
    /// L1 Data-Fidelity QSM
    L1qsm(InvertL1qsmArgs),
    /// Weak-Harmonic QSM
    Whqsm(InvertWhqsmArgs),
    /// Hybrid Data-Fidelity QSM
    Hdqsm(InvertHdqsmArgs),
    /// AMP-PE (Approximate Message Passing with Parameter Estimation)
    AmpPe(InvertAmpPeArgs),
    /// xQSM deep-learning dipole inversion (weights downloaded on first use)
    Xqsm(InvertDlArgs),
    /// QSMnet deep-learning dipole inversion (weights downloaded on first use)
    Qsmnet(InvertDlArgs),
    /// QSMnet+ deep-learning dipole inversion (weights downloaded on first use)
    QsmnetPlus(InvertDlArgs),
    /// QSMnetME deep-learning dipole inversion, the χ-separation toolbox's QSM network (weights downloaded on first use)
    Qsmnetme(InvertDlArgs),
    /// AutoQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use)
    Autoqsm(InvertDlArgs),
    /// QSMGAN deep-learning dipole inversion (weights downloaded on first use)
    Qsmgan(InvertDlArgs),
    /// IR2QSM deep-learning dipole inversion (weights downloaded on first use)
    Ir2qsm(InvertDlArgs),
    /// LPCNN deep-learning dipole inversion (weights downloaded on first use)
    Lpcnn(InvertDlArgs),
    /// MoDL-QSM deep-learning dipole inversion → χ33/STI component (weights downloaded on first use)
    ModlQsm(InvertDlArgs),
    /// NeXtQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use)
    Nextqsm(InvertDlArgs),
    /// iQSM single-step reconstruction — input is every echo's WRAPPED phase (weights downloaded
    /// on first use)
    Iqsm(IqsmArgs),
    /// iQSM+ orientation-adaptive single-step reconstruction — input is every echo's WRAPPED phase;
    /// echoes are combined with (magnitude × TE)² weights (weights downloaded on first use)
    IqsmPlus(PhaseNetArgs),
    /// COSMOS — multi-orientation least squares over 2+ co-registered orientations
    Cosmos(InvertCosmosArgs),
    /// STI (susceptibility tensor imaging) — 6+ co-registered orientations to a rank-2 tensor
    Sti(InvertStiArgs),
}

/// Args for the networks that work from every echo's wrapped phase: iQSM and iQSM+ (`qsmxt
/// invert`, χ out) and iQFM (`qsmxt bgremove`, the local field out). They run as `qsmxt run`
/// runs them, with no tunable parameters ([`IqsmArgs`] adds iQSM's and iQFM's echo weighting);
/// weights are fetched on first use.
#[derive(Parser, Debug)]
pub struct PhaseNetArgs {
    /// Wrapped phase NIfTI file in radians: 4D (x,y,z,echo), or 3D for one echo
    pub input: PathBuf,
    /// Binary brain mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output NIfTI file in ppm (susceptibility for iqsm/iqsm-plus, the local field for iqfm)
    #[arg(short, long)]
    pub output: PathBuf,
    /// Magnitude NIfTI file with the same echoes as the phase (4D), to weight the echo
    /// combination by magnitude; without it the echoes are weighted by TE² alone
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// Deep-learning overlap-tiling (opt-in).
    #[command(flatten)]
    pub tiling: crate::cli_settings::SettingsOf<TileFlags>,
}

/// Args for iQSM (`qsmxt invert iqsm`) and iQFM (`qsmxt bgremove iqfm`): [`PhaseNetArgs`] and
/// the echo weighting. The weighting is a standalone option only; `qsmxt run` uses the default.
#[derive(Parser, Debug)]
pub struct IqsmArgs {
    #[command(flatten)]
    pub net: PhaseNetArgs,
    /// Echo weighting of the multi-echo combination: `mag2-te2` weights each echo by
    /// (magnitude × TE)², the inverse of its noise variance, as in the authors' Python code;
    /// `mag-te2` by magnitude × TE², as in the authors' MATLAB code
    #[arg(long, value_enum, default_value_t = IqsmEchoWeightingArg::Mag2Te2)]
    pub echo_weighting: IqsmEchoWeightingArg,
}

/// `--echo-weighting` of `qsmxt invert iqsm` and `qsmxt bgremove iqfm`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum IqsmEchoWeightingArg {
    /// magnitude × TE²
    #[value(name = "mag-te2")]
    MagTe2,
    /// (magnitude × TE)²
    #[value(name = "mag2-te2")]
    Mag2Te2,
}

impl From<IqsmEchoWeightingArg> for qsm_core::inversion::IqsmEchoWeighting {
    fn from(w: IqsmEchoWeightingArg) -> Self {
        match w {
            IqsmEchoWeightingArg::MagTe2 => Self::MagnitudeTeSquared,
            IqsmEchoWeightingArg::Mag2Te2 => Self::MagnitudeSquaredTeSquared,
        }
    }
}

/// Args for the deep-learning dipole inversions. They have no tunable parameters;
/// weights are fetched on first use. `--field-strength` feeds the scan metadata.
#[derive(Parser, Debug)]
pub struct InvertDlArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// B0 field strength in Tesla (scan metadata)
    #[arg(long, default_value_t = 3.0)]
    pub field_strength: f64,
}
