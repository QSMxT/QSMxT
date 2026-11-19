use clap::Parser;
use std::path::PathBuf;

use super::*;

#[derive(Parser, Debug)]
pub struct InvertRtsArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Delta parameter
    #[arg(long)]
    pub delta: Option<f64>,
    /// Mu parameter
    #[arg(long)]
    pub mu: Option<f64>,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Rho (ADMM penalty)
    #[arg(long)]
    pub rho: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// LSMR iterations
    #[arg(long)]
    pub lsmr_iter: Option<usize>,
    /// Rho is raised when the primal residual falls by less than this factor (tau)
    #[arg(long)]
    pub tau: Option<f64>,
    /// The factor rho is raised by (gamma)
    #[arg(long)]
    pub gamma: Option<f64>,
    /// Zero-padding before the FFTs in voxels, X Y Z: at least this many, rounded up to an
    /// FFT-efficient size; -1 leaves an axis unpadded (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"], allow_negative_numbers = true)]
    pub pad: Option<Vec<i64>>,
}

#[derive(Parser, Debug)]
pub struct InvertTvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Lambda parameter (default: 0.0003)
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Rho (ADMM penalty), or auto for 100 × lambda (default: auto)
    #[arg(long, value_name = "RHO", value_parser = parse_or_auto)]
    pub rho: Option<OrAuto>,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Zero-padding before the FFTs in voxels, X Y Z: at least this many, rounded up to an
    /// FFT-efficient size; -1 leaves an axis unpadded (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"], allow_negative_numbers = true)]
    pub pad: Option<Vec<i64>>,
}

/// TGV of a field map in ppm, which TGV expresses as phase at the echo time `--te` picks.
#[derive(Parser, Debug)]
pub struct InvertTgvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Iterations (default: from the voxel size, as the reference does)
    #[arg(long)]
    pub iterations: Option<usize>,
    /// Erosions
    #[arg(long)]
    pub erosions: Option<usize>,
    /// Alpha1 (first-order weight), in radians at the echo time (--te) and the field strength
    /// (default: 0.00212; QuantitativeSusceptibilityMappingTGV.jl's regularization 2 is 0.003)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Alpha0 (second-order weight), in radians at the echo time (--te) and the field strength
    /// (default: 0.00636; QuantitativeSusceptibilityMappingTGV.jl's regularization 2 is 0.002)
    #[arg(long)]
    pub alpha0: Option<f64>,
    /// Primal step size multiplier
    #[arg(long)]
    pub step_size: Option<f64>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: first)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
    /// Use QuantitativeSusceptibilityMappingTGV.jl's divergence in the w update rather than the
    /// exact adjoint of the symmetric gradient. Changes the result only on anisotropic voxels;
    /// with --reference-edge-erosion and --reference-output-box it reproduces that package, for
    /// comparison with it
    #[arg(long)]
    pub reference_divergence: bool,
    /// Erode the mask as QuantitativeSusceptibilityMappingTGV.jl does, not from the array's edge.
    /// Changes the result only where the mask reaches an array face
    #[arg(long)]
    pub reference_edge_erosion: bool,
    /// Return χ over QuantitativeSusceptibilityMappingTGV.jl's whole output box, not only inside
    /// the eroded mask: adds the rim of up to two voxels just outside it, where no data constrain χ
    #[arg(long)]
    pub reference_output_box: bool,
}

#[derive(Parser, Debug)]
pub struct InvertTikhonovArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Lambda
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Zero-padding before the FFTs in voxels, X Y Z: at least this many, rounded up to an
    /// FFT-efficient size; -1 leaves an axis unpadded (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"], allow_negative_numbers = true)]
    pub pad: Option<Vec<i64>>,
}

#[derive(Parser, Debug)]
pub struct InvertNltvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo;
    /// several echoes are RSS-combined
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Lambda
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Rho (ADMM penalty), or auto for 100 × lambda (default: auto)
    #[arg(long, value_name = "RHO", value_parser = parse_or_auto)]
    pub rho: Option<OrAuto>,
    /// Mu (penalty parameter)
    #[arg(long)]
    pub mu: Option<f64>,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Convergence tolerance of the Newton iterations of the data-term subproblem
    #[arg(long)]
    pub tol_newton: Option<f64>,
    /// Maximum Newton iterations of the data-term subproblem
    #[arg(long)]
    pub newton_iter: Option<usize>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: first)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
    /// Zero-padding before the FFTs in voxels, X Y Z: at least this many, rounded up to an
    /// FFT-efficient size; -1 leaves an axis unpadded (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"], allow_negative_numbers = true)]
    pub pad: Option<Vec<i64>>,
}

/// MEDI of a local field in ppm. It weights the data by a noise map and its edges by a magnitude it
/// makes from every echo's magnitude, so it takes the echoes' magnitudes and echo times (evenly
/// spaced).
#[derive(Parser, Debug)]
pub struct InvertMediArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude of every echo: one 4D NIfTI, or one 3D NIfTI per echo in echo order
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Lambda: the data-fidelity weight (larger fits the field more closely)
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Enable MERIT weighting
    #[arg(long)]
    pub merit: Option<bool>,
    /// MEDI+0 (Liu et al. 2018): zero-reference χ to the ventricular CSF, found from R2* of the
    /// echo magnitudes, and keep it uniform there (default: true). Needs at least 3 echoes; with
    /// fewer, MEDI runs without it
    #[arg(long)]
    pub csf: Option<bool>,
    /// MEDI+0: weight of the CSF uniformity term (default: 100)
    #[arg(long)]
    pub lambda_csf: Option<f64>,
    /// MEDI+0: R2* in Hz below which a voxel is a CSF candidate (default: 5)
    #[arg(long)]
    pub csf_r2s_threshold: Option<f64>,
    /// MEDI+0: erode the mask by a 10 mm sphere before finding the CSF (default: true)
    #[arg(long)]
    pub csf_erode: Option<bool>,
    /// Voxels of zeros appended at the end of x, y and z before the inversion; the result is
    /// cropped back (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"])]
    pub zeropad: Option<Vec<usize>>,
    /// High-pass the local field with a spherical mean value filter, removing residual
    /// background near the brain edge (MEDI-SMV) [default: on]
    #[arg(long)]
    pub smv: Option<bool>,
    /// SMV radius in mm (also mSMV's)
    #[arg(long)]
    pub smv_radius: Option<f64>,
    /// With SMV, shrink the mask to where the SMV of the mask exceeds 0.999; false keeps the
    /// full mask, high-passing the field over it [default: on]
    #[arg(long)]
    pub smv_shrink_mask: Option<bool>,
    /// MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value
    /// first, and fit it with the SMV-filtered dipole kernel. Needs at least 3 echoes
    #[arg(long)]
    pub msmv: bool,
    /// MEDI-mSMV: the boundary correction makes at most this many passes, less one
    #[arg(long, requires = "msmv")]
    pub msmv_maxk: Option<usize>,
    /// MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 ×
    /// the largest voxel dimension if that is more)
    #[arg(long, requires = "msmv")]
    pub msmv_vessel_radius: Option<f64>,
    /// MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed
    /// by the same SMV; after PDF, V-SHARP or LBV it diverges (expert)
    #[arg(long, requires = "msmv")]
    pub no_msmv_prefilter: bool,
    /// Data weighting mode (0=uniform, 1=SNR)
    #[arg(long)]
    pub data_weighting: Option<i32>,
    /// Edge percentage (0.0-1.0)
    #[arg(long)]
    pub percentage: Option<f64>,
    /// CG tolerance
    #[arg(long)]
    pub cg_tol: Option<f64>,
    /// CG max iterations
    #[arg(long)]
    pub cg_max_iter: Option<usize>,
    /// Max outer iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Outer tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: spacing)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
}

/// TFI of the total field in ppm. Like MEDI it takes every echo's magnitude and the echo times
/// (evenly spaced).
#[derive(Parser, Debug)]
pub struct InvertTfiArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude of every echo: one 4D NIfTI, or one 3D NIfTI per echo in echo order
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Lambda: the data-fidelity weight (larger fits the field more closely)
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Preconditioner value (susceptibility scaling outside the mask)
    #[arg(long)]
    pub precond: Option<f64>,
    /// Enable MERIT weighting (default: true)
    #[arg(long)]
    pub merit: Option<bool>,
    /// Fit the field linearly, as the TFI paper does (default: true); false uses MEDI's
    /// exponential data term
    #[arg(long)]
    pub linear: Option<bool>,
    /// Data weighting mode (0=uniform, 1=SNR)
    #[arg(long)]
    pub data_weighting: Option<i32>,
    /// Edge percentage (0.0-1.0)
    #[arg(long)]
    pub percentage: Option<f64>,
    /// CG tolerance
    #[arg(long)]
    pub cg_tol: Option<f64>,
    /// CG max iterations
    #[arg(long)]
    pub cg_max_iter: Option<usize>,
    /// Max outer iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Outer tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: spacing)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
    /// Dipole kernel on a grid with an odd number of voxels along an axis: default samples the
    /// dipole on the Fourier grid; original samples it half a frequency step off, as the original
    /// implementation does, for comparison with it. Identical on even grids (default: default)
    #[arg(long, value_enum, value_name = "KERNEL")]
    pub kernel: Option<TfiKernelArg>,
    /// Arithmetic: double, or single as the original implementation runs, for comparison with
    /// it (default: double)
    #[arg(long, value_enum, value_name = "PRECISION")]
    pub precision: Option<TfiPrecisionArg>,
}

/// STAR-QSM. Its result depends on the scale of the field in radians, so it takes the scan's
/// field strength and echo times and converts the field to phase at the echo time `--te` picks.
#[derive(Parser, Debug)]
pub struct InvertStarArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Zero-padding in voxels per side, X Y Z (default: 8 8 20)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"])]
    pub pad: Option<Vec<usize>>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: sum)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
}

#[derive(Parser, Debug)]
pub struct InvertFansiArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo;
    /// several echoes are RSS-combined
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// First-order (TV/TGV) L1 penalty weight (alpha1)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto)
    #[arg(long, value_name = "MU1", value_parser = parse_or_auto)]
    pub mu1: Option<OrAuto>,
    /// Fidelity-consistency ADMM weight (mu2)
    #[arg(long)]
    pub mu2: Option<f64>,
    /// Second-order L1 penalty weight (alpha0, nlTGV only), or auto for 2 × alpha1 (default: auto)
    #[arg(long, value_name = "ALPHA0", value_parser = parse_or_auto)]
    pub alpha0: Option<OrAuto>,
    /// Second-order consistency ADMM weight (mu0, nlTGV only), or auto for 2 × mu1 (default:
    /// auto)
    #[arg(long, value_name = "MU0", value_parser = parse_or_auto)]
    pub mu0: Option<OrAuto>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Percent-update convergence tolerance
    #[arg(long)]
    pub tol_update: Option<f64>,
    /// Inner Newton convergence tolerance
    #[arg(long)]
    pub tol_delta: Option<f64>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: spacing)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
}

#[derive(Parser, Debug)]
pub struct InvertFansiTgvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo;
    /// several echoes are RSS-combined
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// First-order (TGV gradient) L1 penalty weight (alpha1)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto)
    #[arg(long, value_name = "MU1", value_parser = parse_or_auto)]
    pub mu1: Option<OrAuto>,
    /// Fidelity-consistency ADMM weight (mu2)
    #[arg(long)]
    pub mu2: Option<f64>,
    /// Second-order L1 penalty weight (alpha0), or auto for 2 × alpha1 (default: auto)
    #[arg(long, value_name = "ALPHA0", value_parser = parse_or_auto)]
    pub alpha0: Option<OrAuto>,
    /// Second-order consistency ADMM weight (mu0), or auto for 2 × mu1 (default: auto)
    #[arg(long, value_name = "MU0", value_parser = parse_or_auto)]
    pub mu0: Option<OrAuto>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Percent-update convergence tolerance
    #[arg(long)]
    pub tol_update: Option<f64>,
    /// Inner Newton convergence tolerance
    #[arg(long)]
    pub tol_delta: Option<f64>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: spacing)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
}

#[derive(Parser, Debug)]
pub struct InvertL1qsmArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo;
    /// several echoes are RSS-combined
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Gradient (TV) L1 penalty weight (alpha1)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto)
    #[arg(long, value_name = "MU1", value_parser = parse_or_auto)]
    pub mu1: Option<OrAuto>,
    /// Fidelity-consistency ADMM weight (mu2)
    #[arg(long)]
    pub mu2: Option<f64>,
    /// L1 proximal ADMM weight (mu3)
    #[arg(long)]
    pub mu3: Option<f64>,
    /// L1 fidelity strength (lambda)
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Percent-update convergence tolerance
    #[arg(long)]
    pub tol_update: Option<f64>,
    /// Inner Newton convergence tolerance
    #[arg(long)]
    pub tol_delta: Option<f64>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: first)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
}

#[derive(Parser, Debug)]
pub struct InvertWhqsmArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// TV regularization weight (alpha1)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// ADMM penalty for TV splitting (mu1)
    #[arg(long)]
    pub mu1: Option<f64>,
    /// ADMM penalty for data-fidelity splitting (mu2)
    #[arg(long)]
    pub mu2: Option<f64>,
    /// Weak-harmonic ROI penalty (beta)
    #[arg(long)]
    pub beta: Option<f64>,
    /// ADMM penalty for harmonic-field splitting (muh)
    #[arg(long)]
    pub muh: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Percent-update convergence tolerance
    #[arg(long)]
    pub tol_update: Option<f64>,
    /// Inner Newton convergence tolerance
    #[arg(long)]
    pub tol_delta: Option<f64>,
    /// Data weight: flat (1 over the whole volume, WH_nlTV.m's default), flat-in-mask (1 inside
    /// the mask only), max-normalised (the magnitude over its maximum) or in-vivo (min(2 ×
    /// magnitude / maximum, 1)); the last two need --magnitude (default: flat)
    #[arg(long, value_name = "WEIGHT", value_parser = parse_whqsm_weight)]
    pub weight: Option<qsmxt_engine::WhqsmWeight>,
    /// Magnitude NIfTI, for a magnitude data weight (--weight): one 3D volume, a 4D one, or one
    /// per echo; several echoes are RSS-combined
    #[arg(long, num_args = 1..)]
    pub magnitude: Vec<PathBuf>,
    /// Echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of
    /// the echo times), or a time in ms (default: first)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub te: Option<qsmxt_engine::EchoTimeChoice>,
}

#[derive(Parser, Debug)]
pub struct InvertHdqsmArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Echo times in seconds, every echo's, in order
    #[arg(long, num_args = 1.., required = true, alias = "echo-time")]
    pub echo_times: Vec<f64>,
    /// Magnitude of every echo (the data weight is their echo-time-weighted combination): one 4D
    /// NIfTI, or one 3D NIfTI per echo in echo order
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// L2-stage TV weight (alpha_l2)
    #[arg(long)]
    pub alpha_l2: Option<f64>,
    /// L2-stage gradient-consistency ADMM weight (mu1_l2), or auto for 100 × alpha_l2 (default:
    /// auto)
    #[arg(long, value_name = "MU1_L2", value_parser = parse_or_auto)]
    pub mu1_l2: Option<OrAuto>,
    /// Fidelity consistency weight (mu2)
    #[arg(long)]
    pub mu2: Option<f64>,
    /// Stage-1 (L1) iterations (max_iter_l1)
    #[arg(long)]
    pub max_iter_l1: Option<usize>,
    /// Stage-2 (L2) iterations (max_iter_l2)
    #[arg(long)]
    pub max_iter_l2: Option<usize>,
    /// Stage-2 percent-update convergence tolerance
    #[arg(long)]
    pub tol_update: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertAmpPeArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Magnitude NIfTI (data weight and morphology image): one 3D volume, a 4D one, or one per
    /// echo; several echoes are RSS-combined
    #[arg(long, num_args = 1..)]
    pub magnitude: Vec<PathBuf>,
    /// B0 field strength in Tesla (scales the simulated phase)
    #[arg(long, default_value_t = 3.0)]
    pub b0: f64,
    /// Daubechies wavelet order (1=db1, 2=db2)
    #[arg(long)]
    pub wave_order: Option<usize>,
    /// Wavelet decomposition levels
    #[arg(long)]
    pub nlevel: Option<usize>,
    /// Morphology-mask energy retention fraction (0.0-1.0)
    #[arg(long)]
    pub wave_pec: Option<f64>,
    /// Simulated echo time (s) used to turn the field into phase
    #[arg(long)]
    pub simulated_te: Option<f64>,
    /// Linearization iterations per stage
    #[arg(long)]
    pub max_linearization_ite: Option<usize>,
    /// GAMP signal-update damping rate
    #[arg(long)]
    pub damp_rate_sig: Option<f64>,
    /// Parameter-estimation learning rate (kappa)
    #[arg(long)]
    pub damp_rate_par: Option<f64>,
    /// Inner sparse-reconstruction iterations
    #[arg(long)]
    pub max_pe_spar_ite: Option<usize>,
    /// Inner parameter-estimation iterations
    #[arg(long)]
    pub max_pe_est_ite: Option<usize>,
    /// GAMP inner convergence threshold
    #[arg(long)]
    pub cvg_thd: Option<f64>,
    /// L2-seed Tikhonov weight
    #[arg(long)]
    pub tikhonov_beta: Option<f64>,
}
