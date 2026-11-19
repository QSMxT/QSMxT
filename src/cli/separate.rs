use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use super::*;

// ── Chi-separation ──

/// Inputs shared by every chi-separation method.
#[derive(Args, Debug, Clone)]
pub struct SeparateCommonArgs {
    /// Conventional QSM (χ_total) NIfTI in ppm
    #[arg(long)]
    pub qsm: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii
    /// and {prefix}_desc-total_Chimap.nii — the pipeline's names
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 field strength in Tesla
    #[arg(long, default_value_t = 3.0)]
    pub field_strength: f64,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
}

#[derive(Subcommand, Debug)]
pub enum SeparateCommand {
    /// R2*-QSM closed-form (Dimov 2022) — QSM + R2* (GRE-only)
    R2starQsm(SeparateR2starQsmArgs),
    /// DECOMPOSE-QSM signal-domain fit (Chen 2021) — a QSM per echo + multi-echo magnitude (GRE-only)
    Decompose(SeparateDecomposeArgs),
    /// χ-separation iLSQR (Shin 2021) — local field + R2' + QSM
    ChiSepIlsqr(SeparateChiSepIlsqrArgs),
    /// χ-separation MEDI — local field + R2' + multi-echo magnitude + QSM
    ChiSepMedi(SeparateChiSepMediArgs),
    /// WaveSep wavelet-L1 (Fang 2023) — QSM + R2'
    Wavesep(SeparateWavesepArgs),
    /// Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude
    HcChisep(SeparateHcChisepArgs),
    /// SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use)
    SusepNet(SeparateSusepNetArgs),
    /// χ-sepnet deep-learning separation — local field + R2' or R2* (weights downloaded on first use)
    #[command(name = "chi-sepnet")]
    ChiSepNet(SeparateChiSepNetArgs),
}

#[derive(Parser, Debug)]
pub struct SeparateSusepNetArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    /// Sliding-window patch in voxels, D H W, each a multiple of 8 (default: the whole volume at once)
    #[arg(long, num_args = 3, value_names = ["D", "H", "W"])]
    pub patch: Option<Vec<usize>>,
}

#[derive(Parser, Debug)]
#[command(group(clap::ArgGroup::new("chisepnet_r2").required(true).args(["r2prime", "r2star"])))]
pub struct SeparateChiSepNetArgs {
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii
    /// and {prefix}_desc-total_Chimap.nii — the pipeline's names
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 field strength in Tesla
    #[arg(long, default_value_t = 3.0)]
    pub field_strength: f64,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz (χ-sepnet-R2')
    #[arg(long)]
    pub r2prime: Option<PathBuf>,
    /// R2* map NIfTI in Hz (χ-sepnet-R2*: R2' predicted by R2PRIMEnet; `--route toolbox` only)
    #[arg(long)]
    pub r2star: Option<PathBuf>,
    /// Conventional QSM (χ_total) NIfTI in ppm; `--route native` only (the toolbox route makes its
    /// own with QSMnetME)
    #[arg(long)]
    pub qsm: Option<PathBuf>,
    /// Input route: `toolbox` (default; the authors': training frame, QSMnetME susceptibility
    /// channel) or `native` (the network alone on the acquired grid, given --qsm)
    #[arg(long, value_enum, default_value = "toolbox")]
    pub route: ChiSepNetRouteArg,
    /// `--route native`: sliding-window patch in voxels, D H W, each a multiple of 16 (default:
    /// 192 192 128, the authors')
    #[arg(long, num_args = 3, value_names = ["D", "H", "W"])]
    pub patch: Option<Vec<usize>>,
    /// `--route toolbox`: run the networks as a sliding window of this patch over the 1 mm frame,
    /// D H W, each a multiple of 16 (default: the whole frame at once, as the authors do)
    #[arg(long, num_args = 3, value_names = ["D", "H", "W"])]
    pub frame_patch: Option<Vec<usize>>,
    /// With --r2star: run R2PRIMEnet even when the field strength is not 3 T (uncalibrated)
    #[arg(long)]
    pub ignore_field_mismatch: bool,
}

#[derive(Parser, Debug)]
#[command(group(clap::ArgGroup::new("r2star_source").required(true).args(["r2star", "magnitude"])))]
pub struct SeparateR2starQsmArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// R2* map NIfTI in Hz (else fit from --magnitude + --echo-times)
    #[arg(long)]
    pub r2star: Option<PathBuf>,
    /// Multi-echo magnitude: one 4D file or several 3D files (space-separated). Fits R2* if --r2star absent.
    #[arg(long, num_args = 1.., requires = "echo_times")]
    pub magnitude: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated), required with --magnitude
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    pub echo_times: Vec<f64>,
    /// Relaxometric constant at 3T (Hz/ppm)
    #[arg(long)]
    pub r_const_3t: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct SeparateDecomposeArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Multi-echo magnitude: one 4D file or several 3D files (space-separated)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// QSM of each echo (ppm), which sets each echo's phase: one 4D file or one 3D file per echo
    /// (space-separated; the same file may be given for every echo)
    #[arg(long, num_args = 1.., required = true)]
    pub qsm_per_echo: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated)
    #[arg(long, num_args = 1.., value_delimiter = ',', required = true)]
    pub echo_times: Vec<f64>,
    /// Inner alternating passes per voxel
    #[arg(long)]
    pub n_inner: Option<usize>,
    /// Bound on |χ| in the fit: a number in ppm (default: 0.5), or echo-spacing to compute it
    /// from the field strength and echo spacing
    #[arg(long, value_name = "BOUND", value_parser = parse_chi_bound)]
    pub chi_bound: Option<qsmxt_engine::ChiBoundChoice>,
    /// Unwrap the phase along the echoes (default: false, as in the authors' code)
    #[arg(long, value_name = "BOOL")]
    pub unwrap_phase: Option<bool>,
    /// The fit: `reference-code` (Tim Ho's script; default) or `paper` (Chen et al. 2021:
    /// amplitudes summing to one, per-voxel S(0) = 1 scale)
    #[arg(long, value_enum)]
    pub fit: Option<DecomposeFitArg>,
    /// Maximum solver iterations of each fit (lsqcurvefit MaxIterations; default: 400)
    #[arg(long)]
    pub max_iterations: Option<usize>,
    /// Maximum function evaluations of each fit (lsqcurvefit MaxFunctionEvaluations; default:
    /// 100 × the fit's parameters)
    #[arg(long)]
    pub max_function_evaluations: Option<usize>,
    /// Relative cost change each fit stops at (lsqcurvefit FunctionTolerance; default: 1e-6)
    #[arg(long)]
    pub function_tolerance: Option<f64>,
    /// Relative step each fit stops at (lsqcurvefit StepTolerance; default: 1e-6)
    #[arg(long)]
    pub step_tolerance: Option<f64>,
    /// First-order optimality each fit stops at (lsqcurvefit OptimalityTolerance; default: 1e-6)
    #[arg(long)]
    pub optimality_tolerance: Option<f64>,
    /// Relative finite-difference step of the Jacobian (lsqcurvefit FiniteDifferenceStepSize;
    /// default: √eps)
    #[arg(long)]
    pub finite_difference_step: Option<f64>,
    /// Typical magnitude of each fitted value (lsqcurvefit TypicalX; default: 1)
    #[arg(long)]
    pub typical_x: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct SeparateChiSepIlsqrArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    /// Paramagnetic relaxometric constant (Hz/ppm) (default: 137 × B0 / 2.9 T)
    #[arg(long)]
    pub dr_pos: Option<f64>,
    /// Diamagnetic relaxometric constant (Hz/ppm) (default: 137 × B0 / 2.9 T)
    #[arg(long)]
    pub dr_neg: Option<f64>,
    /// Zero-padding at both ends of each axis in voxels, X Y Z (default: 16 16 16)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"])]
    pub pad: Option<Vec<usize>>,
    /// Maximum CG iterations of the LSQR step (default: 50)
    #[arg(long)]
    pub cg_max_iter_lsqr: Option<usize>,
    /// CG relative tolerance of the LSQR step (default: 0.02)
    #[arg(long)]
    pub cg_tol_lsqr: Option<f64>,
    /// Maximum CG iterations of the streaking-removal step (default: 50)
    #[arg(long)]
    pub cg_max_iter_sa: Option<usize>,
    /// CG relative tolerance of the streaking-removal step (default: 0.02)
    #[arg(long)]
    pub cg_tol_sa: Option<f64>,
    /// The streaking cone is where |D| is below this (default: 0.11)
    #[arg(long)]
    pub d2_thr: Option<f64>,
    /// Percentile of the field's Laplacian at which the LSQR weight is 1 (default: 60)
    #[arg(long)]
    pub prc_w_i_min: Option<f64>,
    /// Percentile of the field's Laplacian at which the LSQR weight is 0 (default: 99.8)
    #[arg(long)]
    pub prc_w_i_max: Option<f64>,
    /// Percentile of each gradient's magnitude at which the streaking-removal weight is 1
    /// (default: 50)
    #[arg(long)]
    pub prc_w_g_min: Option<f64>,
    /// Percentile of each gradient's magnitude at which the streaking-removal weight is 0
    /// (default: 70)
    #[arg(long)]
    pub prc_w_g_max: Option<f64>,
}

/// χ-separation MEDI. Its data terms are weighted by a noise map made from every echo's
/// magnitude, and its CSF term reads a CSF mask made from R2* of the echoes (ARLO, or the
/// χ-separation toolbox's NNLS fit), so it takes the echoes' magnitudes and echo times.
#[derive(Parser, Debug)]
pub struct SeparateChiSepMediArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    /// Multi-echo magnitude: one 4D file or one 3D file per echo (space-separated)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated)
    #[arg(long, num_args = 1.., value_delimiter = ',', required = true)]
    pub echo_times: Vec<f64>,
    /// CSF mask NIfTI for the CSF-uniformity term, or `none` for no CSF term (the toolbox's GUI
    /// without a CSF mask) (default: the CSF found from R2* of the echoes, as the toolbox's
    /// script does; see --csf-r2star)
    #[arg(long, value_name = "FILE|none")]
    pub csf_mask: Option<PathBuf>,
    /// The R2* the CSF mask is found from when no --csf-mask is given: arlo (MEDI's ARLO; the
    /// default) or nnls (the χ-separation toolbox's r2star_nnls, which its script runs)
    #[arg(long, value_name = "METHOD", value_parser = parse_chi_sep_csf_r2star)]
    pub csf_r2star: Option<qsmxt_engine::ChiSepCsfR2star>,
    /// Weight of the data terms against the L1 terms (default: 1)
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Weight of the CSF-uniformity term (default: 1)
    #[arg(long)]
    pub lambda_csf: Option<f64>,
    /// Paramagnetic relaxometric constant (Hz/ppm) (default: 137 × B0 / 2.9 T)
    #[arg(long)]
    pub dr_pos: Option<f64>,
    /// Diamagnetic relaxometric constant (Hz/ppm) (default: 137 × B0 / 2.9 T)
    #[arg(long)]
    pub dr_neg: Option<f64>,
    /// Zero-padding at both ends of the third axis, in voxels (default: 5)
    #[arg(long)]
    pub pad: Option<usize>,
    /// Stop when the relative change falls below this (default: 0.03)
    #[arg(long)]
    pub tol: Option<f64>,
    /// Maximum Gauss-Newton iterations (default: 30)
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Inner CG tolerance (default: 0.05)
    #[arg(long)]
    pub cg_tol: Option<f64>,
    /// Inner CG max iterations (default: 100)
    #[arg(long)]
    pub cg_max_iter: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct SeparateWavesepArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    /// Paramagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub dr_pos: Option<f64>,
    /// Diamagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub dr_neg: Option<f64>,
    /// Proximal-gradient step size
    #[arg(long)]
    pub alpha: Option<f64>,
    /// Wavelet L1 soft-threshold weight
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Daubechies wavelet order
    #[arg(long)]
    pub wavelet_order: Option<usize>,
    /// ISTA max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Relative-change stop tolerance
    #[arg(long)]
    pub tol: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct SeparateHcChisepArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    /// Multi-echo magnitude: one 4D file or several 3D files (space-separated)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated)
    #[arg(long, num_args = 1.., value_delimiter = ',', required = true)]
    pub echo_times: Vec<f64>,
    /// Multi-echo spin-echo magnitude: one 4D file or several 3D files (optional)
    #[arg(long, num_args = 1..)]
    pub se_magnitude: Vec<PathBuf>,
    /// Spin-echo times in seconds (comma-separated) for --se-magnitude
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    pub se_echo_times: Vec<f64>,
    /// Paramagnetic relaxivity at 3T (Hz/ppm)
    #[arg(long)]
    pub dr_pos_3t: Option<f64>,
    /// R2' bin width for the anchored grid search (Hz)
    #[arg(long)]
    pub bin_hz: Option<f64>,
}
