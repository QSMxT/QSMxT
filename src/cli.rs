use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Whether this build includes deep-learning (ONNX) support — shown in `--version` so a
/// non-`dl` build (e.g. the Windows/ARM64 binary) advertises the missing models up front.
#[cfg(feature = "dl")]
const DL_STATUS: &str = "deep-learning models: enabled";
#[cfg(not(feature = "dl"))]
const DL_STATUS: &str = "deep-learning models: DISABLED in this build (classical algorithms only)";

#[derive(Parser, Debug)]
#[command(
    name = "qsmxt",
    version,
    long_version = const_format::formatcp!(
        "{}\nqsm-core: {} ({})\n{}",
        env!("CARGO_PKG_VERSION"),
        env!("QSM_CORE_VERSION"),
        env!("QSM_CORE_GIT_HASH"),
        DL_STATUS,
    ),
    about = "QSMxT: Quantitative Susceptibility Mapping tool (Rust)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Run the full QSM pipeline on a BIDS dataset
    Run(RunArgs),
    /// Generate a pipeline configuration file
    Init(InitArgs),
    /// Validate BIDS dataset structure for QSM processing
    Validate(ValidateArgs),
    /// Convert a DICOM directory to BIDS using automatic series classification
    #[command(name = "dicom-convert")]
    DicomConvert(DicomConvertArgs),
    /// Generate SLURM job scripts for HPC execution
    Slurm(SlurmArgs),
    /// Masking operations
    Mask {
        #[command(subcommand)]
        command: MaskCommand,
    },
    /// Phase unwrapping
    Unwrap {
        #[command(subcommand)]
        command: UnwrapCommand,
    },
    /// B0 field mapping from multi-echo phase (NIfTI in, ppm out)
    Fieldmap {
        #[command(subcommand)]
        command: FieldmapCommand,
    },
    /// Background field removal
    Bgremove {
        #[command(subcommand)]
        command: BgremoveCommand,
    },
    /// Dipole inversion
    Invert {
        #[command(subcommand)]
        command: InvertCommand,
    },
    /// Susceptibility source separation
    Separate {
        #[command(subcommand)]
        command: SeparateCommand,
    },
    /// QSMART vessel-aware reconstruction: total field -> susceptibility
    Qsmart(QsmartArgs),
    /// Susceptibility-weighted imaging
    Swi(SwiArgs),
    /// R2* mapping from multi-echo magnitude data
    R2star(R2starArgs),
    /// T2* mapping from multi-echo magnitude data
    T2star(T2starArgs),
    /// R2 mapping from multi-echo spin-echo (MESE) magnitude data (EPG)
    R2(R2Args),
    /// R2' mapping (R2' = R2* - R2)
    R2prime(R2primeArgs),
    /// Inhomogeneity correction on magnitude data
    Homogeneity(HomogeneityArgs),
    /// Resample oblique volume to axial orientation
    Resample(ResampleArgs),
    /// Compute ROMEO phase quality map
    #[command(name = "quality-map")]
    QualityMap(QualityMapArgs),
    /// Launch interactive TUI for pipeline configuration
    Tui,
    /// Check for updates and optionally install the latest version
    Update(UpdateArgs),
}

#[derive(Parser, Debug)]
pub struct UpdateArgs {
    /// Update without prompting for confirmation
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Parser, Debug)]
pub struct DicomConvertArgs {
    /// Input DICOM directory (searched recursively)
    pub dicom_dir: PathBuf,

    /// Output BIDS directory
    pub output_dir: PathBuf,

    /// Print the auto-detected series classification and exit without converting
    #[arg(long)]
    pub dry_run: bool,
}

// ─── Shared algorithm parameter groups (prefixed, used by RunArgs) ───

#[derive(Args, Debug, Default, Clone)]
pub struct RtsParamArgs {
    /// RTS delta parameter
    #[arg(long)]
    pub rts_delta: Option<f64>,
    /// RTS mu parameter
    #[arg(long)]
    pub rts_mu: Option<f64>,
    /// RTS tolerance
    #[arg(long)]
    pub rts_tol: Option<f64>,
    /// RTS rho (ADMM penalty)
    #[arg(long)]
    pub rts_rho: Option<f64>,
    /// RTS max iterations
    #[arg(long)]
    pub rts_max_iter: Option<usize>,
    /// RTS LSMR iterations
    #[arg(long)]
    pub rts_lsmr_iter: Option<usize>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct TvParamArgs {
    /// TV lambda parameter
    #[arg(long)]
    pub tv_lambda: Option<f64>,
    /// TV rho (ADMM penalty)
    #[arg(long)]
    pub tv_rho: Option<f64>,
    /// TV tolerance
    #[arg(long)]
    pub tv_tol: Option<f64>,
    /// TV max iterations
    #[arg(long)]
    pub tv_max_iter: Option<usize>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct TkdParamArgs {
    /// TKD threshold
    #[arg(long)]
    pub tkd_threshold: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct TsvdParamArgs {
    /// TSVD threshold
    #[arg(long)]
    pub tsvd_threshold: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct TgvParamArgs {
    /// TGV iterations
    #[arg(long)]
    pub tgv_iterations: Option<usize>,
    /// TGV erosions
    #[arg(long)]
    pub tgv_erosions: Option<usize>,
    /// TGV alpha1 (first-order weight)
    #[arg(long)]
    pub tgv_alpha1: Option<f64>,
    /// TGV alpha0 (second-order weight)
    #[arg(long)]
    pub tgv_alpha0: Option<f64>,
    /// TGV primal step size multiplier
    #[arg(long)]
    pub tgv_step_size: Option<f64>,
    /// TGV convergence tolerance
    #[arg(long)]
    pub tgv_tol: Option<f64>,
}

/// Overlap-tiling for deep-learning inversions (bounded memory). Passing `--tile-size` enables
/// tiling (the net runs patch-by-patch); omit it to run whole-volume. `--tile-halo` sets the
/// context margin (defaults to 8 voxels). Tiling is an approximation of the whole-volume network.
#[derive(Args, Debug, Default, Clone)]
pub struct TilingParamArgs {
    /// DL tiling: output core size per patch, in voxels (enables tiling)
    #[arg(long)]
    pub tile_size: Option<usize>,
    /// DL tiling: context margin per side, in voxels (default 8)
    #[arg(long)]
    pub tile_halo: Option<usize>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct TikhonovParamArgs {
    /// Tikhonov lambda
    #[arg(long)]
    pub tikhonov_lambda: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct NltvParamArgs {
    /// NLTV lambda
    #[arg(long)]
    pub nltv_lambda: Option<f64>,
    /// NLTV mu (penalty parameter)
    #[arg(long)]
    pub nltv_mu: Option<f64>,
    /// NLTV tolerance
    #[arg(long)]
    pub nltv_tol: Option<f64>,
    /// NLTV max iterations
    #[arg(long)]
    pub nltv_max_iter: Option<usize>,
    /// NLTV Newton iterations
    #[arg(long)]
    pub nltv_newton_iter: Option<usize>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct NdiParamArgs {
    /// NDI gradient-descent step size (tau)
    #[arg(long)]
    pub ndi_tau: Option<f64>,
    /// NDI L2 regularization weight (alpha)
    #[arg(long)]
    pub ndi_alpha: Option<f64>,
    /// NDI max iterations
    #[arg(long)]
    pub ndi_max_iter: Option<usize>,
    /// NDI phase scale (ppm -> working scale)
    #[arg(long)]
    pub ndi_phase_scale: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct FansiParamArgs {
    /// FANSI first-order (TV/TGV) L1 penalty weight (alpha1)
    #[arg(long)]
    pub fansi_alpha1: Option<f64>,
    /// FANSI gradient-consistency ADMM weight (mu1)
    #[arg(long)]
    pub fansi_mu1: Option<f64>,
    /// FANSI fidelity-consistency ADMM weight (mu2)
    #[arg(long)]
    pub fansi_mu2: Option<f64>,
    /// FANSI second-order L1 penalty weight (alpha0, nlTGV only)
    #[arg(long)]
    pub fansi_alpha0: Option<f64>,
    /// FANSI second-order consistency ADMM weight (mu0, nlTGV only)
    #[arg(long)]
    pub fansi_mu0: Option<f64>,
    /// FANSI outer ADMM iterations
    #[arg(long)]
    pub fansi_max_iter: Option<usize>,
    /// FANSI percent-update convergence tolerance
    #[arg(long)]
    pub fansi_tol_update: Option<f64>,
    /// FANSI inner Newton convergence tolerance
    #[arg(long)]
    pub fansi_tol_delta: Option<f64>,
    /// FANSI phase scale (ppm -> working scale)
    #[arg(long)]
    pub fansi_phase_scale: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct L1qsmParamArgs {
    /// L1-QSM gradient (TV) L1 penalty weight (alpha1)
    #[arg(long)]
    pub l1qsm_alpha1: Option<f64>,
    /// L1-QSM gradient-consistency ADMM weight (mu1)
    #[arg(long)]
    pub l1qsm_mu1: Option<f64>,
    /// L1-QSM fidelity-consistency ADMM weight (mu2)
    #[arg(long)]
    pub l1qsm_mu2: Option<f64>,
    /// L1-QSM L1 proximal ADMM weight (mu3)
    #[arg(long)]
    pub l1qsm_mu3: Option<f64>,
    /// L1-QSM L1 fidelity strength (lambda)
    #[arg(long)]
    pub l1qsm_lambda: Option<f64>,
    /// L1-QSM outer ADMM iterations
    #[arg(long)]
    pub l1qsm_max_iter: Option<usize>,
    /// L1-QSM percent-update convergence tolerance
    #[arg(long)]
    pub l1qsm_tol_update: Option<f64>,
    /// L1-QSM inner Newton convergence tolerance
    #[arg(long)]
    pub l1qsm_tol_delta: Option<f64>,
    /// L1-QSM phase scale (ppm -> working scale)
    #[arg(long)]
    pub l1qsm_phase_scale: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct WhqsmParamArgs {
    /// WH-QSM TV regularization weight (alpha1)
    #[arg(long)]
    pub whqsm_alpha1: Option<f64>,
    /// WH-QSM ADMM penalty for TV splitting (mu1)
    #[arg(long)]
    pub whqsm_mu1: Option<f64>,
    /// WH-QSM ADMM penalty for data-fidelity splitting (mu2)
    #[arg(long)]
    pub whqsm_mu2: Option<f64>,
    /// WH-QSM weak-harmonic ROI penalty (beta)
    #[arg(long)]
    pub whqsm_beta: Option<f64>,
    /// WH-QSM ADMM penalty for harmonic-field splitting (muh)
    #[arg(long)]
    pub whqsm_muh: Option<f64>,
    /// WH-QSM max outer iterations
    #[arg(long)]
    pub whqsm_max_iter: Option<usize>,
    /// WH-QSM percent-update convergence tolerance
    #[arg(long)]
    pub whqsm_tol_update: Option<f64>,
    /// WH-QSM inner Newton convergence tolerance
    #[arg(long)]
    pub whqsm_tol_delta: Option<f64>,
    /// WH-QSM phase scale (ppm -> working scale)
    #[arg(long)]
    pub whqsm_phase_scale: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct HdqsmParamArgs {
    /// HD-QSM L2-stage TV weight (alpha_l2)
    #[arg(long)]
    pub hdqsm_alpha_l2: Option<f64>,
    /// HD-QSM L2-stage gradient-consistency ADMM weight (mu1_l2)
    #[arg(long)]
    pub hdqsm_mu1_l2: Option<f64>,
    /// HD-QSM fidelity consistency weight (mu2)
    #[arg(long)]
    pub hdqsm_mu2: Option<f64>,
    /// HD-QSM stage-1 (L1) iterations (max_iter_l1)
    #[arg(long)]
    pub hdqsm_max_iter_l1: Option<usize>,
    /// HD-QSM stage-2 (L2) iterations (max_iter_l2)
    #[arg(long)]
    pub hdqsm_max_iter_l2: Option<usize>,
    /// HD-QSM stage-2 percent-update convergence tolerance
    #[arg(long)]
    pub hdqsm_tol_update: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct AmpPeParamArgs {
    /// AMP-PE Daubechies wavelet order (1=db1, 2=db2)
    #[arg(long)]
    pub amp_pe_wave_order: Option<usize>,
    /// AMP-PE wavelet decomposition levels
    #[arg(long)]
    pub amp_pe_nlevel: Option<usize>,
    /// AMP-PE morphology-mask energy retention fraction (0.0-1.0)
    #[arg(long)]
    pub amp_pe_wave_pec: Option<f64>,
    /// AMP-PE simulated echo time (s) used to turn the field into phase
    #[arg(long)]
    pub amp_pe_simulated_te: Option<f64>,
    /// AMP-PE linearization iterations per stage
    #[arg(long)]
    pub amp_pe_max_linearization_ite: Option<usize>,
    /// AMP-PE GAMP signal-update damping rate
    #[arg(long)]
    pub amp_pe_damp_rate_sig: Option<f64>,
    /// AMP-PE parameter-estimation learning rate (kappa)
    #[arg(long)]
    pub amp_pe_damp_rate_par: Option<f64>,
    /// AMP-PE inner sparse-reconstruction iterations
    #[arg(long)]
    pub amp_pe_max_pe_spar_ite: Option<usize>,
    /// AMP-PE inner parameter-estimation iterations
    #[arg(long)]
    pub amp_pe_max_pe_est_ite: Option<usize>,
    /// AMP-PE GAMP inner convergence threshold
    #[arg(long)]
    pub amp_pe_cvg_thd: Option<f64>,
    /// AMP-PE L2-seed Tikhonov weight
    #[arg(long)]
    pub amp_pe_tikhonov_beta: Option<f64>,
}

/// Per-method chi-separation parameters (defaults come from qsm-core's `Default` impls).
#[derive(Args, Debug, Default, Clone)]
pub struct SeparationParamArgs {
    // R2*-QSM (Dimov)
    /// R2*-QSM relaxometric constant at 3T (Hz/ppm)
    #[arg(long)]
    pub r2star_qsm_r_const_3t: Option<f64>,
    // DECOMPOSE (Chen)
    /// DECOMPOSE inner alternating passes per voxel
    #[arg(long)]
    pub decompose_n_inner: Option<usize>,
    /// DECOMPOSE upper bound on |χ| in the fit (ppm)
    #[arg(long)]
    pub decompose_chi_bound: Option<f64>,
    /// DECOMPOSE Levenberg–Marquardt max iterations
    #[arg(long)]
    pub decompose_max_lm_iter: Option<usize>,
    // χ-sep iLSQR (Shin)
    /// χ-sep iLSQR paramagnetic relaxometric constant (Hz/ppm)
    #[arg(long)]
    pub chi_sep_ilsqr_dr_pos: Option<f64>,
    /// χ-sep iLSQR diamagnetic relaxometric constant (Hz/ppm)
    #[arg(long)]
    pub chi_sep_ilsqr_dr_neg: Option<f64>,
    /// χ-sep iLSQR L1 edge-masked TV weight
    #[arg(long)]
    pub chi_sep_ilsqr_lambda1: Option<f64>,
    /// χ-sep iLSQR edge-mask keep fraction (0-1)
    #[arg(long)]
    pub chi_sep_ilsqr_percentage: Option<f64>,
    /// χ-sep iLSQR R2' reliability window lower (Hz)
    #[arg(long)]
    pub chi_sep_ilsqr_r2p_min: Option<f64>,
    /// χ-sep iLSQR R2' reliability window upper (Hz)
    #[arg(long)]
    pub chi_sep_ilsqr_r2p_max: Option<f64>,
    /// χ-sep iLSQR outer Gauss-Newton iterations
    #[arg(long)]
    pub chi_sep_ilsqr_max_iter: Option<usize>,
    /// χ-sep iLSQR outer relative-change tolerance
    #[arg(long)]
    pub chi_sep_ilsqr_tol: Option<f64>,
    /// χ-sep iLSQR inner CG max iterations
    #[arg(long)]
    pub chi_sep_ilsqr_cg_max_iter: Option<usize>,
    /// χ-sep iLSQR inner CG relative tolerance
    #[arg(long)]
    pub chi_sep_ilsqr_cg_tol: Option<f64>,
    // χ-sep MEDI
    /// χ-sep MEDI paramagnetic L1 weight
    #[arg(long)]
    pub chi_sep_medi_lambda_para: Option<f64>,
    /// χ-sep MEDI diamagnetic L1 weight
    #[arg(long)]
    pub chi_sep_medi_lambda_dia: Option<f64>,
    /// χ-sep MEDI field/R2' coupling weight
    #[arg(long)]
    pub chi_sep_medi_lambda_cpl: Option<f64>,
    /// χ-sep MEDI paramagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub chi_sep_medi_dr_pos: Option<f64>,
    /// χ-sep MEDI diamagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub chi_sep_medi_dr_neg: Option<f64>,
    /// χ-sep MEDI edge-mask percentage (0-1)
    #[arg(long)]
    pub chi_sep_medi_percentage: Option<f64>,
    /// χ-sep MEDI inner CG tolerance
    #[arg(long)]
    pub chi_sep_medi_cg_tol: Option<f64>,
    /// χ-sep MEDI inner CG max iterations
    #[arg(long)]
    pub chi_sep_medi_cg_max_iter: Option<usize>,
    /// χ-sep MEDI outer Gauss-Newton iterations
    #[arg(long)]
    pub chi_sep_medi_max_iter: Option<usize>,
    /// χ-sep MEDI outer convergence tolerance
    #[arg(long)]
    pub chi_sep_medi_tol: Option<f64>,
    // WaveSep (Fang)
    /// WaveSep paramagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub wavesep_dr_pos: Option<f64>,
    /// WaveSep diamagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub wavesep_dr_neg: Option<f64>,
    /// WaveSep proximal-gradient step size
    #[arg(long)]
    pub wavesep_alpha: Option<f64>,
    /// WaveSep wavelet L1 soft-threshold weight
    #[arg(long)]
    pub wavesep_lambda: Option<f64>,
    /// WaveSep Daubechies wavelet order
    #[arg(long)]
    pub wavesep_wavelet_order: Option<usize>,
    /// WaveSep ISTA max iterations
    #[arg(long)]
    pub wavesep_max_iter: Option<usize>,
    /// WaveSep relative-change stop tolerance
    #[arg(long)]
    pub wavesep_tol: Option<f64>,
    // HC-ChiSep (Stewart 2026)
    /// HC-ChiSep paramagnetic relaxivity at 3T (Hz/ppm)
    #[arg(long)]
    pub hc_chisep_dr_pos_3t: Option<f64>,
    /// HC-ChiSep R2' bin width for the anchored grid search (Hz)
    #[arg(long)]
    pub hc_chisep_bin_hz: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct MediParamArgs {
    /// MEDI lambda
    #[arg(long)]
    pub medi_lambda: Option<f64>,
    /// MEDI: enable MERIT weighting
    #[arg(long)]
    pub medi_merit: Option<bool>,
    /// MEDI: enable SMV deconvolution
    #[arg(long)]
    pub medi_smv: bool,
    /// MEDI SMV radius in mm
    #[arg(long)]
    pub medi_smv_radius: Option<f64>,
    /// MEDI: data weighting mode (0=uniform, 1=SNR)
    #[arg(long)]
    pub medi_data_weighting: Option<i32>,
    /// MEDI edge percentage (0.0-1.0)
    #[arg(long)]
    pub medi_percentage: Option<f64>,
    /// MEDI CG tolerance
    #[arg(long)]
    pub medi_cg_tol: Option<f64>,
    /// MEDI CG max iterations
    #[arg(long)]
    pub medi_cg_max_iter: Option<usize>,
    /// MEDI max outer iterations
    #[arg(long)]
    pub medi_max_iter: Option<usize>,
    /// MEDI outer tolerance
    #[arg(long)]
    pub medi_tol: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct TfiParamArgs {
    /// TFI lambda
    #[arg(long)]
    pub tfi_lambda: Option<f64>,
    /// TFI preconditioner value (susceptibility scaling outside the mask)
    #[arg(long)]
    pub tfi_precond: Option<f64>,
    /// TFI: enable MERIT weighting
    #[arg(long)]
    pub tfi_merit: Option<bool>,
    /// TFI: data weighting mode (0=uniform, 1=SNR)
    #[arg(long)]
    pub tfi_data_weighting: Option<i32>,
    /// TFI edge percentage (0.0-1.0)
    #[arg(long)]
    pub tfi_percentage: Option<f64>,
    /// TFI CG tolerance
    #[arg(long)]
    pub tfi_cg_tol: Option<f64>,
    /// TFI CG max iterations
    #[arg(long)]
    pub tfi_cg_max_iter: Option<usize>,
    /// TFI max outer iterations
    #[arg(long)]
    pub tfi_max_iter: Option<usize>,
    /// TFI outer tolerance
    #[arg(long)]
    pub tfi_tol: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct IlsqrParamArgs {
    /// iLSQR tolerance
    #[arg(long)]
    pub ilsqr_tol: Option<f64>,
    /// iLSQR max iterations
    #[arg(long)]
    pub ilsqr_max_iter: Option<usize>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct QsmartParamArgs {
    /// QSMART iLSQR tolerance
    #[arg(long)]
    pub qsmart_ilsqr_tol: Option<f64>,
    /// QSMART iLSQR max iterations
    #[arg(long)]
    pub qsmart_ilsqr_max_iter: Option<usize>,
    /// QSMART vasculature detection sphere radius
    #[arg(long)]
    pub qsmart_vasc_sphere_radius: Option<i32>,
    /// QSMART SDF spatial radius
    #[arg(long)]
    pub qsmart_sdf_spatial_radius: Option<i32>,
    /// QSMART inner dipole inversion algorithm (default: ilsqr)
    #[arg(long)]
    pub qsmart_inversion: Option<QsmAlgorithmArg>,
    /// QSMART SDF sigma1, stage 1 (voxels)
    #[arg(long)]
    pub qsmart_sdf_sigma1_stage1: Option<f64>,
    /// QSMART SDF sigma2, stage 1 (voxels)
    #[arg(long)]
    pub qsmart_sdf_sigma2_stage1: Option<f64>,
    /// QSMART SDF sigma1, stage 2 (voxels)
    #[arg(long)]
    pub qsmart_sdf_sigma1_stage2: Option<f64>,
    /// QSMART SDF sigma2, stage 2 (voxels)
    #[arg(long)]
    pub qsmart_sdf_sigma2_stage2: Option<f64>,
    /// QSMART SDF proximity lower limit
    #[arg(long)]
    pub qsmart_sdf_lower_lim: Option<f64>,
    /// QSMART SDF curvature constant
    #[arg(long)]
    pub qsmart_sdf_curv_constant: Option<f64>,
    /// QSMART Frangi min vessel radius (mm)
    #[arg(long)]
    pub qsmart_frangi_scale_min: Option<f64>,
    /// QSMART Frangi max vessel radius (mm)
    #[arg(long)]
    pub qsmart_frangi_scale_max: Option<f64>,
    /// QSMART Frangi scale step (mm)
    #[arg(long)]
    pub qsmart_frangi_scale_ratio: Option<f64>,
    /// QSMART Frangi C noise threshold
    #[arg(long)]
    pub qsmart_frangi_c: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct VsharpParamArgs {
    /// V-SHARP deconvolution threshold
    #[arg(long)]
    pub vsharp_threshold: Option<f64>,
    /// V-SHARP max kernel radius in mm
    #[arg(long)]
    pub vsharp_max_radius: Option<f64>,
    /// V-SHARP min kernel radius in mm
    #[arg(long)]
    pub vsharp_min_radius: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct PdfParamArgs {
    /// PDF tolerance
    #[arg(long)]
    pub pdf_tol: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct LbvParamArgs {
    /// LBV tolerance
    #[arg(long)]
    pub lbv_tol: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct IsmvParamArgs {
    /// iSMV tolerance
    #[arg(long)]
    pub ismv_tol: Option<f64>,
    /// iSMV max iterations
    #[arg(long)]
    pub ismv_max_iter: Option<usize>,
    /// iSMV kernel radius in mm
    #[arg(long)]
    pub ismv_radius: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct SharpParamArgs {
    /// SHARP threshold
    #[arg(long)]
    pub sharp_threshold: Option<f64>,
    /// SHARP kernel radius in mm
    #[arg(long)]
    pub sharp_radius: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct ResharpParamArgs {
    /// RESHARP SMV kernel radius in mm
    #[arg(long)]
    pub resharp_radius: Option<f64>,
    /// RESHARP Tikhonov regularization parameter
    #[arg(long)]
    pub resharp_tik_reg: Option<f64>,
    /// RESHARP CG convergence tolerance
    #[arg(long)]
    pub resharp_tol: Option<f64>,
    /// RESHARP maximum CG iterations
    #[arg(long)]
    pub resharp_max_iter: Option<usize>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct HarperellaParamArgs {
    /// HARPERELLA SMV kernel radius in mm
    #[arg(long)]
    pub harperella_radius: Option<f64>,
    /// HARPERELLA maximum iterations
    #[arg(long)]
    pub harperella_max_iter: Option<usize>,
    /// HARPERELLA convergence tolerance
    #[arg(long)]
    pub harperella_tol: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct IharperellaParamArgs {
    /// iHARPERELLA SMV kernel radius in mm
    #[arg(long)]
    pub iharperella_radius: Option<f64>,
    /// iHARPERELLA maximum iterations
    #[arg(long)]
    pub iharperella_max_iter: Option<usize>,
    /// iHARPERELLA convergence tolerance
    #[arg(long)]
    pub iharperella_tol: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct MsmvParamArgs {
    /// Apply mSMV boundary-shadow refinement (Roberts 2024) on top of the primary BFR
    #[arg(long)]
    pub msmv_refine: bool,
    /// mSMV SMV kernel radius in mm
    #[arg(long)]
    pub msmv_radius: Option<f64>,
    /// mSMV maximum boundary-correction iterations
    #[arg(long)]
    pub msmv_maxk: Option<usize>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct RomeoParamArgs {
    /// ROMEO: disable phase coherence weights (default: on)
    #[arg(long)]
    pub no_romeo_phase_coherence: bool,
    /// ROMEO: disable phase gradient coherence weights (default: on)
    #[arg(long)]
    pub no_romeo_phase_gradient_coherence: bool,
    /// ROMEO: disable phase linearity weights (default: on)
    #[arg(long)]
    pub no_romeo_phase_linearity: bool,
    /// ROMEO: disable magnitude coherence weights (default: on)
    #[arg(long)]
    pub no_romeo_mag_coherence: bool,
    /// ROMEO: enable magnitude weighting (default: off)
    #[arg(long)]
    pub romeo_mag_weight: bool,
    /// ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility)
    #[arg(long)]
    pub no_romeo_mag_weight: bool,
    /// ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off)
    #[arg(long)]
    pub romeo_mag_weight2: bool,
    /// ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off)
    #[arg(long)]
    pub romeo_bestpath: bool,
    /// ROMEO: template echo index (1-indexed, only for template mode)
    #[arg(long)]
    pub romeo_template: Option<usize>,
    /// ROMEO: use individual per-echo unwrapping (default)
    #[arg(long)]
    pub romeo_individual: bool,
    /// ROMEO: use template-based temporal unwrapping (disables individual mode)
    #[arg(long)]
    pub no_romeo_individual: bool,
    /// ROMEO: enable inter-echo 2π offset correction (default: off)
    #[arg(long)]
    pub romeo_correct_global: bool,
    /// ROMEO: disable inter-echo 2π offset correction
    #[arg(long)]
    pub no_romeo_correct_global: bool,
    /// ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0.5)
    #[arg(long)]
    pub romeo_temporal_uncertain_unwrapping: Option<f64>,
    /// ROMEO: maximum number of seed regions (default: 1)
    #[arg(long)]
    pub romeo_max_seeds: Option<u8>,
    /// ROMEO: merge neighboring regions after unwrapping (default: off)
    #[arg(long)]
    pub romeo_merge_regions: bool,
    /// ROMEO: correct each region's median to nearest 0 (default: off)
    #[arg(long)]
    pub romeo_correct_regions: bool,
    /// ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0)
    #[arg(long)]
    pub romeo_wrap_addition: Option<f64>,
}

impl RomeoParamArgs {
    /// Build `qsm_core::unwrap::RomeoParams` from these CLI flags, starting from the
    /// library defaults and applying overrides. `--romeo-template` is 1-indexed on the
    /// CLI and converted to the 0-indexed value the library expects.
    pub fn to_romeo_params(&self) -> qsm_core::unwrap::RomeoParams {
        let mut p = qsm_core::unwrap::RomeoParams::default();
        // Weight component flags (default-true → disabled via --no-*)
        if self.no_romeo_phase_coherence { p.phase_coherence = false; }
        if self.no_romeo_phase_gradient_coherence { p.phase_gradient_coherence = false; }
        if self.no_romeo_phase_linearity { p.phase_linearity = false; }
        if self.no_romeo_mag_coherence { p.mag_coherence = false; }
        // Weight component flags (default-false → enabled via positive flag)
        if self.romeo_mag_weight { p.mag_weight = true; }
        if self.no_romeo_mag_weight { p.mag_weight = false; }
        if self.romeo_mag_weight2 { p.mag_weight2 = true; }
        if self.romeo_bestpath { p.bestpath = true; }
        // Multi-echo options
        if let Some(t) = self.romeo_template { p.template = t.saturating_sub(1); }
        if self.romeo_individual { p.individual = true; }
        if self.no_romeo_individual { p.individual = false; }
        if self.romeo_correct_global { p.correct_global = true; }
        if self.no_romeo_correct_global { p.correct_global = false; }
        if let Some(v) = self.romeo_temporal_uncertain_unwrapping { p.temporal_uncertain_unwrapping = v; }
        if let Some(v) = self.romeo_max_seeds { p.max_seeds = v; }
        if self.romeo_merge_regions { p.merge_regions = true; }
        if self.romeo_correct_regions { p.correct_regions = true; }
        if let Some(v) = self.romeo_wrap_addition { p.wrap_addition = v; }
        p
    }
}

#[derive(Args, Debug, Default, Clone)]
pub struct SwiParamArgs {
    /// SWI high-pass filter sigma (3 values, in voxels)
    #[arg(long, num_args = 3)]
    pub swi_hp_sigma: Option<Vec<f64>>,
    /// SWI phase scaling type (tanh, negative-tanh, positive, negative, triangular)
    #[arg(long)]
    pub swi_scaling: Option<String>,
    /// SWI phase scaling strength
    #[arg(long)]
    pub swi_strength: Option<f64>,
    /// SWI MIP window size in slices
    #[arg(long)]
    pub swi_mip_window: Option<usize>,
}

// ─── Pipeline commands ───

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt/)
    pub output_dir: Option<PathBuf>,

    /// Pipeline configuration file (TOML)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Include only runs matching these glob patterns (e.g. "sub-1*" "*ses-pre*")
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns (e.g. "*mygrea*")
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,

    /// Limit number of echoes to process
    #[arg(long)]
    pub num_echoes: Option<usize>,

    /// QSM algorithm
    #[arg(long, value_enum)]
    pub qsm_algorithm: Option<QsmAlgorithmArg>,

    /// Unwrapping algorithm
    #[arg(long, value_enum)]
    pub unwrapping_algorithm: Option<UnwrapAlgorithmArg>,

    /// Background field removal algorithm
    #[arg(long, value_enum)]
    pub bf_algorithm: Option<BfAlgorithmArg>,

    /// Masking input image (overrides the input of the configured mask sections;
    /// combine with --mask-preset, e.g. --mask-preset robust-threshold --masking-input magnitude)
    #[arg(long, value_enum)]
    pub masking_input: Option<MaskInputArg>,

    /// Enable phase offset removal for multi-echo data (default: true)
    #[arg(long)]
    pub phase_offset_removal: Option<bool>,

    /// Phase offset removal smoothing sigma (3 values, in voxels)
    #[arg(long, num_args = 3)]
    pub phase_offset_sigma: Option<Vec<f64>>,

    /// Enable bipolar gradient correction (requires >= 3 echoes)
    #[arg(long)]
    pub bipolar_correction: bool,

    /// B0 field estimation method
    #[arg(long, value_enum)]
    pub b0_estimation: Option<B0EstimationArg>,

    /// B0 weighted averaging weight type
    #[arg(long, value_enum)]
    pub b0_weight_type: Option<B0WeightTypeArg>,

    /// BET fractional intensity (0.0-1.0)
    #[arg(long)]
    pub bet_fractional_intensity: Option<f64>,

    /// BET surface smoothness factor
    #[arg(long)]
    pub bet_smoothness: Option<f64>,

    /// BET gradient threshold (-1 to 1)
    #[arg(long)]
    pub bet_gradient_threshold: Option<f64>,

    /// BET surface evolution iterations
    #[arg(long)]
    pub bet_iterations: Option<usize>,

    /// BET icosphere subdivision level
    #[arg(long)]
    pub bet_subdivisions: Option<usize>,

    /// QSM reference method (mean or none)
    #[arg(long, value_enum)]
    pub qsm_reference: Option<QsmReferenceArg>,

    #[command(flatten)]
    pub rts_params: RtsParamArgs,
    #[command(flatten)]
    pub tv_params: TvParamArgs,
    #[command(flatten)]
    pub tkd_params: TkdParamArgs,
    #[command(flatten)]
    pub tsvd_params: TsvdParamArgs,
    #[command(flatten)]
    pub tgv_params: TgvParamArgs,
    #[command(flatten)]
    pub tikhonov_params: TikhonovParamArgs,
    #[command(flatten)]
    pub nltv_params: NltvParamArgs,
    #[command(flatten)]
    pub medi_params: MediParamArgs,
    #[command(flatten)]
    pub tfi_params: TfiParamArgs,
    #[command(flatten)]
    pub ilsqr_params: IlsqrParamArgs,
    #[command(flatten)]
    pub qsmart_params: QsmartParamArgs,
    #[command(flatten)]
    pub ndi_params: NdiParamArgs,
    #[command(flatten)]
    pub fansi_params: FansiParamArgs,
    #[command(flatten)]
    pub l1qsm_params: L1qsmParamArgs,
    #[command(flatten)]
    pub whqsm_params: WhqsmParamArgs,
    #[command(flatten)]
    pub hdqsm_params: HdqsmParamArgs,
    #[command(flatten)]
    pub amp_pe_params: AmpPeParamArgs,
    #[command(flatten)]
    pub separation_params: SeparationParamArgs,
    #[command(flatten)]
    pub vsharp_params: VsharpParamArgs,
    #[command(flatten)]
    pub pdf_params: PdfParamArgs,
    #[command(flatten)]
    pub lbv_params: LbvParamArgs,
    #[command(flatten)]
    pub ismv_params: IsmvParamArgs,
    #[command(flatten)]
    pub sharp_params: SharpParamArgs,
    #[command(flatten)]
    pub resharp_params: ResharpParamArgs,
    #[command(flatten)]
    pub harperella_params: HarperellaParamArgs,
    #[command(flatten)]
    pub iharperella_params: IharperellaParamArgs,
    #[command(flatten)]
    pub msmv_params: MsmvParamArgs,
    #[command(flatten)]
    pub romeo_params: RomeoParamArgs,
    #[command(flatten)]
    pub swi_params: SwiParamArgs,
    #[command(flatten)]
    pub tiling_params: TilingParamArgs,


    /// Number of parallel threads
    #[arg(long)]
    pub n_procs: Option<usize>,

    /// Inhomogeneity correction smoothing sigma in mm
    #[arg(long)]
    pub homogeneity_sigma_mm: Option<f64>,

    /// Inhomogeneity correction box filter passes
    #[arg(long)]
    pub homogeneity_nbox: Option<usize>,

    /// Linear fit reliability threshold percentile (degrees)
    #[arg(long)]
    pub linear_fit_reliability_threshold: Option<f64>,

    /// Linear fit: estimate a phase offset term (default: true)
    #[arg(long)]
    pub linear_fit_estimate_offset: Option<bool>,

    /// Skip QSM processing (only run supplementary outputs like SWI, T2*, R2*)
    #[arg(long)]
    pub no_qsm: bool,

    /// Also compute SWI
    #[arg(long)]
    pub do_swi: bool,

    /// Compute T2* relaxation map from multi-echo magnitude data
    #[arg(long)]
    pub do_t2starmap: bool,

    /// Compute R2* decay rate map from multi-echo magnitude data
    #[arg(long)]
    pub do_r2starmap: bool,

    /// Compute R2 map from a multi-echo spin-echo (MESE) acquisition (EPG)
    #[arg(long)]
    pub do_r2map: bool,

    /// Compute R2' map (= R2* − R2; needs GRE magnitude + a MESE acquisition)
    #[arg(long)]
    pub do_r2primemap: bool,

    /// Compute chi-separation (paramagnetic/diamagnetic susceptibility maps)
    #[arg(long = "do-chisep")]
    pub do_chi_separation: bool,

    /// Chi-separation method (default: r2star-qsm)
    #[arg(long = "chisep", value_enum)]
    pub chi_separation_algorithm: Option<SeparationAlgorithmArg>,

    /// Use a bring-your-own QSM (Chimap) from <bids>/derivatives/<TOOL>/ for chi-separation
    #[arg(long, value_name = "TOOL")]
    pub use_custom_qsm: Option<String>,

    /// Use a bring-your-own R2 map from <bids>/derivatives/<TOOL>/ for chi-separation
    #[arg(long, value_name = "TOOL")]
    pub use_custom_r2: Option<String>,

    /// Use a bring-your-own R2' map from <bids>/derivatives/<TOOL>/ for chi-separation
    #[arg(long, value_name = "TOOL")]
    pub use_custom_r2prime: Option<String>,

    /// Also export final maps as DICOM series into each subject's extra_files/ folder
    #[arg(long)]
    pub export_dicom: bool,

    /// Optional source DICOM directory; inherit patient/study identity from the
    /// original DICOMs when exporting (used with --export-dicom)
    #[arg(long)]
    pub source_dicom: Option<PathBuf>,

    /// Restrict DICOM export to these maps (default: all produced). Values:
    /// chimap, swi, minip, t2starmap, r2starmap, r2map, r2primemap,
    /// desc-paramagnetic_chimap, desc-diamagnetic_chimap, desc-total_chimap
    /// (used with --export-dicom)
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    pub dicom_outputs: Option<Vec<String>>,

    /// Apply inhomogeneity correction to magnitude before masking
    #[arg(long)]
    pub inhomogeneity_correction: bool,

    /// Disable inhomogeneity correction
    #[arg(long)]
    pub no_inhomogeneity_correction: bool,

    /// Resample oblique acquisitions to axial if obliquity exceeds threshold (degrees, -1 to disable)
    #[arg(long)]
    pub obliquity_threshold: Option<f64>,

    /// Mask preset (robust-threshold or bet)
    #[arg(long, value_enum)]
    pub mask_preset: Option<MaskPresetArg>,

    /// Prefer a bring-your-own mask from BIDS derivatives when present (else compute it).
    /// Bare flag = first matching derivatives tool (alphabetical); give a name (e.g. `bet`)
    /// to restrict to `derivatives/<tool>/sub-*/anat/*_mask.nii*`. Always falls back.
    #[arg(long, num_args = 0..=1, default_missing_value = "*")]
    pub use_custom_masks: Option<String>,

    /// Define a mask section (repeatable, multiple sections are OR'd together).
    /// Format: <input>,<generator>,<refinement1>,<refinement2>,...
    /// Example: phase-quality,threshold:otsu,dilate:2,fill-holes:0,erode:2
    /// Example: magnitude,bet:0.5,erode:2
    #[arg(long = "mask", num_args = 1)]
    pub mask_sections_cli: Option<Vec<String>>,

    /// Print processing plan without executing
    #[arg(long)]
    pub dry: bool,

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,

    /// Memory limit in GB for concurrent run scheduling (auto-detected if not specified)
    #[arg(long)]
    pub mem_limit_gb: Option<f64>,

    /// Disable memory-aware concurrency limiting
    #[arg(long)]
    pub no_mem_limit: bool,

    /// Force re-run, ignoring cached pipeline state
    #[arg(long)]
    pub force: bool,

    /// Remove intermediate files after pipeline completes (keep only final outputs)
    #[arg(long)]
    pub clean_intermediates: bool,
}

#[derive(Parser, Debug)]
pub struct InitArgs {
    /// Output file path (prints to stdout if not specified)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct ValidateArgs {
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    /// Include only runs matching these glob patterns
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,
}

#[derive(Parser, Debug)]
pub struct SlurmArgs {
    /// Input BIDS directory
    pub bids_dir: PathBuf,

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt/)
    pub output_dir: Option<PathBuf>,

    /// SLURM account name
    #[arg(long)]
    pub account: String,

    /// SLURM partition
    #[arg(long)]
    pub partition: Option<String>,

    /// Pipeline configuration file (TOML)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Wall time limit (e.g., 02:00:00)
    #[arg(long, default_value = "02:00:00")]
    pub time: String,

    /// Memory per job in GB
    #[arg(long, default_value_t = 32)]
    pub mem: usize,

    /// CPUs per task
    #[arg(long, default_value_t = 4)]
    pub cpus_per_task: usize,

    /// Auto-submit scripts via sbatch
    #[arg(long)]
    pub submit: bool,

    /// Include only runs matching these glob patterns (e.g. "sub-1*" "*ses-pre*")
    #[arg(long, num_args = 1..)]
    pub include: Option<Vec<String>>,

    /// Exclude runs matching these glob patterns (e.g. "*mygrea*")
    #[arg(long, num_args = 1..)]
    pub exclude: Option<Vec<String>>,

    /// Limit number of echoes to process
    #[arg(long)]
    pub num_echoes: Option<usize>,
}

// ─── Standalone algorithm commands (subcommand-per-algorithm) ───

// ── Mask ──

#[derive(Args, Debug, Clone)]
pub struct MaskCommonArgs {
    /// Input NIfTI file
    pub input: PathBuf,
    /// Output mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Refinement operation (repeatable, applied in order).
    /// Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0
    #[arg(long = "op")]
    pub ops: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum MaskCommand {
    /// Otsu automatic thresholding
    Otsu(MaskOtsuArgs),
    /// Fixed value thresholding
    Value(MaskValueArgs),
    /// Percentile thresholding
    Percentile(MaskPercentileArgs),
    /// Brain extraction (BET)
    Bet(MaskBetArgs),
    /// Robust threshold (Otsu + dilate:1 + fill-holes:0 + erode:1)
    Robust(MaskRobustArgs),
    /// Erode a binary mask
    Erode(MaskErodeArgs),
    /// Dilate a binary mask
    Dilate(MaskDilateArgs),
    /// Morphological closing on a binary mask
    Close(MaskCloseArgs),
    /// Fill holes in a binary mask
    FillHoles(MaskFillHolesArgs),
    /// Gaussian smooth a binary mask (re-thresholds at 0.5)
    Smooth(MaskSmoothArgs),
}

#[derive(Parser, Debug)]
pub struct MaskOtsuArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
}

#[derive(Parser, Debug)]
pub struct MaskValueArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Threshold value
    #[arg(long)]
    pub threshold: f64,
}

#[derive(Parser, Debug)]
pub struct MaskPercentileArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Percentile value (0-100)
    #[arg(long)]
    pub percentile: f64,
}

#[derive(Parser, Debug)]
pub struct MaskBetArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Fractional intensity (0.0-1.0, smaller = larger brain)
    #[arg(long, default_value_t = 0.5)]
    pub fractional_intensity: f64,
}

#[derive(Parser, Debug)]
pub struct MaskRobustArgs {
    /// Input NIfTI file
    pub input: PathBuf,
    /// Output mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
}

// ── Unwrap ──

#[derive(Args, Debug, Clone)]
pub struct UnwrapCommonArgs {
    /// Input wrapped phase NIfTI file
    pub input: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output unwrapped phase NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
}

#[derive(Subcommand, Debug)]
pub enum UnwrapCommand {
    /// ROMEO region-growing unwrapping
    Romeo(UnwrapRomeoArgs),
    /// Laplacian-based unwrapping
    Laplacian(UnwrapLaplacianArgs),
}

#[derive(Parser, Debug)]
pub struct UnwrapRomeoArgs {
    #[command(flatten)]
    pub common: UnwrapCommonArgs,
    /// Magnitude image (improves quality)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    /// Disable phase gradient coherence weights
    #[arg(long)]
    pub no_phase_gradient_coherence: bool,
    /// Disable magnitude coherence weights
    #[arg(long)]
    pub no_mag_coherence: bool,
    /// Disable magnitude weighting
    #[arg(long)]
    pub no_mag_weight: bool,
}

#[derive(Parser, Debug)]
pub struct UnwrapLaplacianArgs {
    #[command(flatten)]
    pub common: UnwrapCommonArgs,
}

// ── Fieldmap ──

#[derive(Args, Debug, Clone)]
pub struct FieldmapCommonArgs {
    /// Input multi-echo wrapped phase NIfTI file (4D: x,y,z,echo)
    pub input: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output B0 field map NIfTI file (in ppm)
    #[arg(short, long)]
    pub output: PathBuf,
    /// Multi-echo magnitude NIfTI file (4D; improves weighting)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    /// Echo times in seconds (space-separated, one per echo)
    #[arg(long, num_args = 1..)]
    pub tes: Option<Vec<f64>>,
    /// Field strength in Tesla (alias: --field-strength)
    #[arg(long, alias = "field-strength")]
    pub b0: Option<f64>,
    /// JSON params file: {"TE":[..seconds], "B0":<tesla>, "voxel_size":[..], "B0_dir":[..]}.
    /// --tes / --b0 override values from this file when both are present.
    #[arg(long)]
    pub params: Option<PathBuf>,
    /// B0 field estimation method
    #[arg(long, value_enum, default_value = "weighted-avg")]
    pub b0_estimation: B0EstimationArg,
    /// B0 weighted-averaging weight type
    #[arg(long, value_enum, default_value = "phase-snr")]
    pub b0_weight_type: B0WeightTypeArg,
    /// Linear fit reliability threshold percentile (degrees)
    #[arg(long)]
    pub linear_fit_reliability_threshold: Option<f64>,
    /// Linear fit: estimate a phase offset term (default: true)
    #[arg(long)]
    pub linear_fit_estimate_offset: Option<bool>,
}

#[derive(Subcommand, Debug)]
pub enum FieldmapCommand {
    /// ROMEO-based multi-echo field mapping (phase offset removal + unwrap + B0 fit)
    Romeo(FieldmapRomeoArgs),
    /// Laplacian-based multi-echo field mapping
    Laplacian(FieldmapLaplacianArgs),
}

#[derive(Parser, Debug)]
pub struct FieldmapRomeoArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    /// Enable phase offset removal for multi-echo data (default: true)
    #[arg(long)]
    pub phase_offset_removal: Option<bool>,
    /// Phase offset removal smoothing sigma (3 values, in voxels)
    #[arg(long, num_args = 3)]
    pub phase_offset_sigma: Option<Vec<f64>>,
    /// Enable bipolar gradient correction (requires >= 3 echoes)
    #[arg(long)]
    pub bipolar_correction: bool,
    #[command(flatten)]
    pub romeo_params: RomeoParamArgs,
}

#[derive(Parser, Debug)]
pub struct FieldmapLaplacianArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    // Note: phase offset removal is inert for Laplacian (skipped by the engine),
    // so it is intentionally not exposed here. b0-estimation / b0-weight-type /
    // linear-fit-* on `common` still apply.
}

// ── Bgremove ──

#[derive(Args, Debug, Clone)]
pub struct BgremoveCommonArgs {
    /// Input total field NIfTI file
    pub input: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output local field NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// Output eroded mask (for algorithms that erode)
    #[arg(long)]
    pub output_mask: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum BgremoveCommand {
    /// Variable radius SHARP (V-SHARP)
    Vsharp(BgremoveVsharpArgs),
    /// Projection onto Dipole Fields
    Pdf(BgremovePdfArgs),
    /// Laplacian Boundary Value
    Lbv(BgremoveLbvArgs),
    /// Iterative Spherical Mean Value
    Ismv(BgremoveIsmvArgs),
    /// Spherical Harmonic Array Reconstruction Procedure
    Sharp(BgremoveSharpArgs),
    /// Regularized SHARP (RESHARP) with Tikhonov regularization
    Resharp(BgremoveResharpArgs),
    /// HARPERELLA integrated phase unwrapping + background removal
    Harperella(BgremoveHarperellaArgs),
    /// Improved HARPERELLA (iHARPERELLA)
    Iharperella(BgremoveIharperellaArgs),
    /// Maximum Spherical Mean Value (mSMV) boundary-shadow removal
    Msmv(BgremoveMsmvArgs),
    /// BFRnet deep-learning background removal (weights downloaded on first use)
    Bfrnet(BgremoveBfrnetArgs),
}

#[derive(Parser, Debug)]
pub struct BgremoveBfrnetArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
}

#[derive(Parser, Debug)]
pub struct BgremoveVsharpArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Deconvolution threshold
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Max radius factor (multiplied by min voxel size)
    #[arg(long)]
    pub max_radius: Option<f64>,
    /// Min radius factor (multiplied by max voxel size)
    #[arg(long)]
    pub min_radius: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct BgremovePdfArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct BgremoveLbvArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct BgremoveIsmvArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Radius factor (multiplied by max voxel size)
    #[arg(long)]
    pub radius: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct BgremoveSharpArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Threshold
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Radius factor (multiplied by min voxel size)
    #[arg(long)]
    pub radius: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct BgremoveResharpArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// SMV kernel radius in mm
    #[arg(long)]
    pub radius: Option<f64>,
    /// Tikhonov regularization parameter
    #[arg(long)]
    pub tik_reg: Option<f64>,
    /// CG convergence tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Maximum CG iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct BgremoveMsmvArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// SMV prefilter kernel radius in mm
    #[arg(long)]
    pub radius: Option<f64>,
    /// Maximum boundary-correction iterations
    #[arg(long)]
    pub maxk: Option<usize>,
    /// B0 field strength (T) — sets the radian shadow-threshold cap
    #[arg(long)]
    pub field_strength: Option<f64>,
    /// Echo time (s) for the ppm↔radian conversion
    #[arg(long)]
    pub te: Option<f64>,
    /// Refinement mode: skip the SMV prefilter and treat the input as an already-local
    /// field from a primary BFR (only the boundary-shadow correction is applied)
    #[arg(long)]
    pub refine: bool,
}

#[derive(Parser, Debug)]
pub struct BgremoveHarperellaArgs {
    /// Input wrapped phase NIfTI file
    pub input: std::path::PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: std::path::PathBuf,
    /// Output local field NIfTI file
    #[arg(short, long)]
    pub output: std::path::PathBuf,
    /// Output eroded mask (for algorithms that erode)
    #[arg(long)]
    pub output_mask: Option<std::path::PathBuf>,
    /// SMV kernel radius in mm
    #[arg(long)]
    pub radius: Option<f64>,
    /// Maximum iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Convergence tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Echo time (s). With --field-strength, converts the radian tissue-phase output to a
    /// ppm field (`phase / (2·π·γ·B0·TE)`); without both, the output stays in radians.
    #[arg(long)]
    pub te: Option<f64>,
    /// B0 field strength (T) — see --te.
    #[arg(long)]
    pub field_strength: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct BgremoveIharperellaArgs {
    /// Input wrapped phase NIfTI file
    pub input: std::path::PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: std::path::PathBuf,
    /// Output local field NIfTI file
    #[arg(short, long)]
    pub output: std::path::PathBuf,
    /// Output eroded mask (for algorithms that erode)
    #[arg(long)]
    pub output_mask: Option<std::path::PathBuf>,
    /// SMV kernel radius in mm
    #[arg(long)]
    pub radius: Option<f64>,
    /// Maximum iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Convergence tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Echo time (s). With --field-strength, converts the radian tissue-phase output to a
    /// ppm field (`phase / (2·π·γ·B0·TE)`); without both, the output stays in radians.
    #[arg(long)]
    pub te: Option<f64>,
    /// B0 field strength (T) — see --te.
    #[arg(long)]
    pub field_strength: Option<f64>,
}

// ── Invert ──

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
    #[arg(long, num_args = 3, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// Deep-learning overlap-tiling (opt-in; ignored by classical algorithms).
    #[command(flatten)]
    pub tiling_params: TilingParamArgs,
}

#[derive(Subcommand, Debug)]
pub enum InvertCommand {
    /// Regularized Total Strength (RTS)
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
    /// Nonlocal Total Variation
    Nltv(InvertNltvArgs),
    /// Morphology Enabled Dipole Inversion
    Medi(InvertMediArgs),
    /// Total Field Inversion (preconditioned). Input is the TOTAL field (not a local field).
    Tfi(InvertTfiArgs),
    /// Iterative Least Squares QR
    Ilsqr(InvertIlsqrArgs),
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
}

#[derive(Parser, Debug)]
pub struct InvertTvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Lambda parameter
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Rho (ADMM penalty)
    #[arg(long)]
    pub rho: Option<f64>,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct InvertTkdArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Threshold
    #[arg(long)]
    pub threshold: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertTsvdArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Threshold
    #[arg(long)]
    pub threshold: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertTgvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// B0 field strength in Tesla
    #[arg(long)]
    pub field_strength: f64,
    /// Echo time in seconds
    #[arg(long)]
    pub echo_time: f64,
    /// Iterations
    #[arg(long)]
    pub iterations: Option<usize>,
    /// Erosions
    #[arg(long)]
    pub erosions: Option<usize>,
    /// Alpha1 (first-order weight)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Alpha0 (second-order weight)
    #[arg(long)]
    pub alpha0: Option<f64>,
    /// Primal step size multiplier
    #[arg(long)]
    pub step_size: Option<f64>,
    /// Convergence tolerance
    #[arg(long)]
    pub tol: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertTikhonovArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Lambda
    #[arg(long)]
    pub lambda: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertNltvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Lambda
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Mu (penalty parameter)
    #[arg(long)]
    pub mu: Option<f64>,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Newton iterations
    #[arg(long)]
    pub newton_iter: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct InvertMediArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// B0 field strength in Tesla (MEDI works in radians; used to convert the ppm field)
    #[arg(long)]
    pub field_strength: f64,
    /// Echo time in seconds (used to convert the ppm field to radians for MEDI)
    #[arg(long)]
    pub echo_time: f64,
    /// Magnitude NIfTI file (recommended)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    /// Lambda
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Enable MERIT weighting
    #[arg(long)]
    pub merit: Option<bool>,
    /// Enable SMV deconvolution (MEDI's own background removal). Set false when the input is
    /// already a local field. [default: from qsm-core]
    #[arg(long)]
    pub smv: Option<bool>,
    /// SMV radius in mm
    #[arg(long)]
    pub smv_radius: Option<f64>,
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
}

#[derive(Parser, Debug)]
pub struct InvertTfiArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Magnitude NIfTI file (recommended)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    /// Lambda
    #[arg(long)]
    pub lambda: Option<f64>,
    /// Preconditioner value (susceptibility scaling outside the mask)
    #[arg(long)]
    pub precond: Option<f64>,
    /// Enable MERIT weighting
    #[arg(long)]
    pub merit: Option<bool>,
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
}

#[derive(Parser, Debug)]
pub struct InvertIlsqrArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
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
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Phase scale (ppm -> working scale)
    #[arg(long)]
    pub phase_scale: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertFansiArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// First-order (TV/TGV) L1 penalty weight (alpha1)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Gradient-consistency ADMM weight (mu1)
    #[arg(long)]
    pub mu1: Option<f64>,
    /// Fidelity-consistency ADMM weight (mu2)
    #[arg(long)]
    pub mu2: Option<f64>,
    /// Second-order L1 penalty weight (alpha0, nlTGV only)
    #[arg(long)]
    pub alpha0: Option<f64>,
    /// Second-order consistency ADMM weight (mu0, nlTGV only)
    #[arg(long)]
    pub mu0: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Percent-update convergence tolerance
    #[arg(long)]
    pub tol_update: Option<f64>,
    /// Inner Newton convergence tolerance
    #[arg(long)]
    pub tol_delta: Option<f64>,
    /// Phase scale (ppm -> working scale)
    #[arg(long)]
    pub phase_scale: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertFansiTgvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// First-order (TGV gradient) L1 penalty weight (alpha1)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Gradient-consistency ADMM weight (mu1)
    #[arg(long)]
    pub mu1: Option<f64>,
    /// Fidelity-consistency ADMM weight (mu2)
    #[arg(long)]
    pub mu2: Option<f64>,
    /// Second-order L1 penalty weight (alpha0)
    #[arg(long)]
    pub alpha0: Option<f64>,
    /// Second-order consistency ADMM weight (mu0)
    #[arg(long)]
    pub mu0: Option<f64>,
    /// Max iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Percent-update convergence tolerance
    #[arg(long)]
    pub tol_update: Option<f64>,
    /// Inner Newton convergence tolerance
    #[arg(long)]
    pub tol_delta: Option<f64>,
    /// Phase scale (ppm -> working scale)
    #[arg(long)]
    pub phase_scale: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertL1qsmArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Gradient (TV) L1 penalty weight (alpha1)
    #[arg(long)]
    pub alpha1: Option<f64>,
    /// Gradient-consistency ADMM weight (mu1)
    #[arg(long)]
    pub mu1: Option<f64>,
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
    /// Phase scale (ppm -> working scale)
    #[arg(long)]
    pub phase_scale: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertWhqsmArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
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
    /// Phase scale (ppm -> working scale)
    #[arg(long)]
    pub phase_scale: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct InvertHdqsmArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// L2-stage TV weight (alpha_l2)
    #[arg(long)]
    pub alpha_l2: Option<f64>,
    /// L2-stage gradient-consistency ADMM weight (mu1_l2)
    #[arg(long)]
    pub mu1_l2: Option<f64>,
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
    /// Magnitude NIfTI file (used as data-fidelity weight + morphology mask)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
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

// ── Chi-separation ──

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum SeparationAlgorithmArg {
    /// R2*-QSM closed-form (Dimov 2022) — QSM + R2* (GRE-only)
    R2starQsm,
    /// DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude (GRE-only)
    Decompose,
    /// χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM
    ChiSepIlsqr,
    /// χ-separation MEDI — local field + R2' + magnitude
    ChiSepMedi,
    /// WaveSep wavelet-L1 (Fang 2023) — QSM + R2'
    Wavesep,
    /// Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude
    HcChisep,
    /// SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use)
    SusepNet,
    /// χ-sepnet deep-learning separation — QSM + R2' + local field (weights downloaded on first use)
    #[value(name = "chi-sepnet")]
    ChiSepNet,
}

/// Inputs shared by every chi-separation method.
#[derive(Args, Debug, Clone)]
pub struct SeparateCommonArgs {
    /// Conventional QSM (χ_total) NIfTI in ppm
    #[arg(long)]
    pub qsm: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output prefix; writes {prefix}_paramagnetic.nii, {prefix}_diamagnetic.nii, {prefix}_total.nii
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 field strength in Tesla
    #[arg(long, default_value_t = 3.0)]
    pub field_strength: f64,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
}

#[derive(Subcommand, Debug)]
pub enum SeparateCommand {
    /// R2*-QSM closed-form (Dimov 2022) — QSM + R2* (GRE-only)
    R2starQsm(SeparateR2starQsmArgs),
    /// DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude (GRE-only)
    Decompose(SeparateDecomposeArgs),
    /// χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM
    ChiSepIlsqr(SeparateChiSepIlsqrArgs),
    /// χ-separation MEDI — local field + R2' + magnitude + QSM
    ChiSepMedi(SeparateChiSepMediArgs),
    /// WaveSep wavelet-L1 (Fang 2023) — QSM + R2'
    Wavesep(SeparateWavesepArgs),
    /// Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude
    HcChisep(SeparateHcChisepArgs),
    /// SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use)
    SusepNet(SeparateSusepNetArgs),
    /// χ-sepnet deep-learning separation — QSM + R2' + local field (weights downloaded on first use)
    #[command(name = "chi-sepnet")]
    ChiSepNet(SeparateSusepNetArgs),
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
    /// GRE echo times in seconds (space- or comma-separated)
    #[arg(long, num_args = 1.., value_delimiter = ',', required = true)]
    pub echo_times: Vec<f64>,
    /// Inner alternating passes per voxel
    #[arg(long)]
    pub n_inner: Option<usize>,
    /// Upper bound on |χ| in the fit (ppm)
    #[arg(long)]
    pub chi_bound: Option<f64>,
    /// Levenberg–Marquardt max iterations
    #[arg(long)]
    pub max_lm_iter: Option<usize>,
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
    /// Magnitude for SNR weighting + edge mask: one 4D file or several 3D files (RSS-combined)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Paramagnetic relaxometric constant (Hz/ppm)
    #[arg(long)]
    pub dr_pos: Option<f64>,
    /// Diamagnetic relaxometric constant (Hz/ppm)
    #[arg(long)]
    pub dr_neg: Option<f64>,
    /// L1 edge-masked TV weight
    #[arg(long)]
    pub lambda1: Option<f64>,
    /// Edge-mask keep fraction (0-1)
    #[arg(long)]
    pub percentage: Option<f64>,
    /// R2' reliability window lower (Hz)
    #[arg(long)]
    pub r2p_min: Option<f64>,
    /// R2' reliability window upper (Hz)
    #[arg(long)]
    pub r2p_max: Option<f64>,
    /// Outer Gauss-Newton iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Outer relative-change tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Inner CG max iterations
    #[arg(long)]
    pub cg_max_iter: Option<usize>,
    /// Inner CG relative tolerance
    #[arg(long)]
    pub cg_tol: Option<f64>,
}

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
    /// Magnitude for edge weighting: one 4D file or several 3D files (RSS-combined)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Paramagnetic L1 weight
    #[arg(long)]
    pub lambda_para: Option<f64>,
    /// Diamagnetic L1 weight
    #[arg(long)]
    pub lambda_dia: Option<f64>,
    /// Field/R2' coupling weight
    #[arg(long)]
    pub lambda_cpl: Option<f64>,
    /// Paramagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub dr_pos: Option<f64>,
    /// Diamagnetic relaxivity (Hz/ppm)
    #[arg(long)]
    pub dr_neg: Option<f64>,
    /// Edge-mask percentage (0-1)
    #[arg(long)]
    pub percentage: Option<f64>,
    /// Inner CG tolerance
    #[arg(long)]
    pub cg_tol: Option<f64>,
    /// Inner CG max iterations
    #[arg(long)]
    pub cg_max_iter: Option<usize>,
    /// Outer Gauss-Newton iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Outer convergence tolerance
    #[arg(long)]
    pub tol: Option<f64>,
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

// ── SWI ──

#[derive(Parser, Debug)]
pub struct QsmartArgs {
    /// Input total field NIfTI file (ppm)
    pub input: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output susceptibility map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// B0 field strength in Tesla
    #[arg(long)]
    pub field_strength: f64,
    /// Echo time in seconds
    #[arg(long)]
    pub echo_time: f64,
    /// Combined magnitude NIfTI (optional; drives vasculature detection)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    #[command(flatten)]
    pub qsmart_params: QsmartParamArgs,
}

#[derive(Parser, Debug)]
pub struct SwiArgs {
    /// Input phase NIfTI file
    pub phase: PathBuf,
    /// Input magnitude NIfTI file
    pub magnitude: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output SWI NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Also compute minimum intensity projection
    #[arg(long)]
    pub mip: bool,
    /// Output path for MIP
    #[arg(long)]
    pub mip_output: Option<PathBuf>,
    #[command(flatten)]
    pub swi_params: SwiParamArgs,
}

// ── Other simple commands ──

#[derive(Parser, Debug)]
pub struct R2starArgs {
    /// Multi-echo magnitude: one 4D file or several 3D files (3+ echoes; space-separated)
    #[arg(required = true, num_args = 1..)]
    pub inputs: Vec<PathBuf>,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output R2* map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Echo times in seconds (must match number of inputs)
    #[arg(long, required = true, num_args = 3..)]
    pub echo_times: Vec<f64>,
}

#[derive(Parser, Debug)]
pub struct T2starArgs {
    /// Multi-echo magnitude: one 4D file or several 3D files (3+ echoes; space-separated)
    #[arg(required = true, num_args = 1..)]
    pub inputs: Vec<PathBuf>,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output T2* map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Echo times in seconds (must match number of inputs)
    #[arg(long, required = true, num_args = 3..)]
    pub echo_times: Vec<f64>,
}

#[derive(Parser, Debug)]
pub struct R2Args {
    /// Multi-echo spin-echo (MESE) magnitude: one 4D file or several 3D files (space-separated)
    #[arg(required = true, num_args = 1..)]
    pub inputs: Vec<PathBuf>,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output R2 map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Spin-echo times in seconds (space- or comma-separated)
    #[arg(long, required = true, num_args = 1.., value_delimiter = ',')]
    pub echo_times: Vec<f64>,
    /// Assumed T1 in seconds (EPG dictionary)
    #[arg(long)]
    pub t1: Option<f64>,
    /// Optional B1 map NIfTI (refocusing efficiency); fitted from the dictionary if absent
    #[arg(long)]
    pub b1_map: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct R2primeArgs {
    /// R2* map NIfTI in Hz
    #[arg(long)]
    pub r2star: PathBuf,
    /// R2 map NIfTI in Hz
    #[arg(long)]
    pub r2: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output R2' map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
}

#[derive(Parser, Debug)]
pub struct HomogeneityArgs {
    /// Input magnitude NIfTI file
    pub input: PathBuf,
    /// Output corrected magnitude NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Smoothing sigma in mm (default: 7.0)
    #[arg(long, default_value_t = 7.0)]
    pub sigma: f64,
    /// Number of box filter passes for Gaussian approximation (default: 3)
    #[arg(long, default_value_t = 3)]
    pub nbox: usize,
}

#[derive(Parser, Debug)]
pub struct MaskErodeArgs {
    /// Input binary mask NIfTI file
    pub input: PathBuf,
    /// Output eroded mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Number of erosion iterations
    #[arg(long, default_value_t = 1)]
    pub iterations: usize,
}

#[derive(Parser, Debug)]
pub struct MaskDilateArgs {
    /// Input binary mask NIfTI file
    pub input: PathBuf,
    /// Output dilated mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Number of dilation iterations
    #[arg(long, default_value_t = 1)]
    pub iterations: usize,
}

#[derive(Parser, Debug)]
pub struct MaskCloseArgs {
    /// Input binary mask NIfTI file
    pub input: PathBuf,
    /// Output closed mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Closing radius
    #[arg(long, default_value_t = 1)]
    pub radius: usize,
}

#[derive(Parser, Debug)]
pub struct MaskFillHolesArgs {
    /// Input binary mask NIfTI file
    pub input: PathBuf,
    /// Output filled mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Maximum hole size in voxels
    #[arg(long, default_value_t = 1000)]
    pub max_size: usize,
}

#[derive(Parser, Debug)]
pub struct MaskSmoothArgs {
    /// Input binary mask NIfTI file
    pub input: PathBuf,
    /// Output smoothed mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Gaussian sigma in mm
    #[arg(long, default_value_t = 4.0)]
    pub sigma: f64,
}

#[derive(Parser, Debug)]
pub struct ResampleArgs {
    /// Input NIfTI file
    pub input: PathBuf,
    /// Output resampled NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
}

#[derive(Parser, Debug)]
pub struct QualityMapArgs {
    /// Input phase NIfTI file (first echo)
    pub phase: PathBuf,
    /// Output quality map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Magnitude image (improves quality estimation)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    /// Second echo phase image (improves quality estimation)
    #[arg(long)]
    pub phase2: Option<PathBuf>,
    /// Echo time of first phase in seconds
    #[arg(long, default_value_t = 0.02)]
    pub te1: f64,
    /// Echo time of second phase in seconds (if --phase2 provided)
    #[arg(long, default_value_t = 0.04)]
    pub te2: f64,
}

// ─── Shared enums (used by RunArgs) ───

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum QsmAlgorithmArg {
    Rts, Tv, Tkd, Tsvd, Tgv, Tikhonov, Nltv, Medi, Tfi, Ilsqr, Qsmart,
    Ndi, Fansi, FansiTgv, L1qsm, Whqsm, Hdqsm, AmpPe,
    // Deep-learning dipole inversions (weights downloaded on first use).
    Xqsm, Qsmnet, QsmnetPlus, Autoqsm, Qsmgan, Ir2qsm, Lpcnn, ModlQsm, Nextqsm,
    // End-to-end DL reconstructions from wrapped phase (no separate unwrap/BFR).
    Iqsm, IqsmPlus,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum UnwrapAlgorithmArg {
    Romeo, Laplacian,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum BfAlgorithmArg {
    Vsharp, Pdf, Lbv, Ismv, Sharp, Resharp, Harperella, Iharperella,
    /// BFRnet deep-learning background removal (weights downloaded on first use)
    Bfrnet,
    /// iQFM deep-learning joint unwrapping + background removal from phase (weights downloaded on first use)
    Iqfm,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum B0EstimationArg {
    WeightedAvg, LinearFit,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum B0WeightTypeArg {
    PhaseSNR, PhaseVar, Average, TEs, Mag,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum MaskInputArg {
    MagnitudeFirst, Magnitude, MagnitudeLast, PhaseQuality,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum QsmReferenceArg {
    Mean, None,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum MaskPresetArg {
    RobustThreshold, Bet,
}
