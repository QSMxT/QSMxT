use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "qsmxt",
    version,
    long_version = const_format::formatcp!(
        "{}\nqsm-core: {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("QSM_CORE_VERSION"),
        env!("QSM_CORE_GIT_HASH"),
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
    /// Masking operations (NIfTI in/out)
    Mask {
        #[command(subcommand)]
        command: MaskCommand,
    },
    /// Phase unwrapping (NIfTI in/out)
    Unwrap {
        #[command(subcommand)]
        command: UnwrapCommand,
    },
    /// Background field removal (NIfTI in/out)
    Bgremove {
        #[command(subcommand)]
        command: BgremoveCommand,
    },
    /// Dipole inversion (NIfTI in/out)
    Invert {
        #[command(subcommand)]
        command: InvertCommand,
    },
    /// QSMART vessel-aware reconstruction: total field -> susceptibility (NIfTI in/out)
    Qsmart(QsmartArgs),
    /// Susceptibility-weighted imaging (NIfTI in/out)
    Swi(SwiArgs),
    /// R2* mapping from multi-echo magnitude data (NIfTI in/out)
    R2star(R2starArgs),
    /// T2* mapping from multi-echo magnitude data (NIfTI in/out)
    T2star(T2starArgs),
    /// Inhomogeneity correction on magnitude data (NIfTI in/out)
    Homogeneity(HomogeneityArgs),
    /// Resample oblique volume to axial orientation (NIfTI in/out)
    Resample(ResampleArgs),
    /// Compute ROMEO phase quality map (NIfTI in/out)
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
    /// V-SHARP max radius factor (multiplied by min voxel size)
    #[arg(long)]
    pub vsharp_max_radius_factor: Option<f64>,
    /// V-SHARP min radius factor (multiplied by max voxel size)
    #[arg(long)]
    pub vsharp_min_radius_factor: Option<f64>,
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
    /// iSMV radius factor (multiplied by max voxel size)
    #[arg(long)]
    pub ismv_radius_factor: Option<f64>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct SharpParamArgs {
    /// SHARP threshold
    #[arg(long)]
    pub sharp_threshold: Option<f64>,
    /// SHARP radius factor (multiplied by min voxel size)
    #[arg(long)]
    pub sharp_radius_factor: Option<f64>,
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
pub struct RomeoParamArgs {
    /// ROMEO: disable phase gradient coherence weights
    #[arg(long)]
    pub no_romeo_phase_gradient_coherence: bool,
    /// ROMEO: disable magnitude coherence weights
    #[arg(long)]
    pub no_romeo_mag_coherence: bool,
    /// ROMEO: disable magnitude weighting
    #[arg(long)]
    pub no_romeo_mag_weight: bool,
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

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt.rs/)
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

    /// Masking algorithm
    #[arg(long, value_enum)]
    pub masking_algorithm: Option<MaskAlgorithmArg>,

    /// Masking input type
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

    /// ROMEO: use individual per-echo unwrapping (default)
    #[arg(long)]
    pub romeo_individual: bool,

    /// ROMEO: use template-based temporal unwrapping
    #[arg(long)]
    pub no_romeo_individual: bool,

    /// ROMEO: disable inter-echo 2π offset correction
    #[arg(long)]
    pub no_romeo_correct_global: bool,

    /// ROMEO: template echo index (1-indexed, only for template mode)
    #[arg(long)]
    pub romeo_template: Option<usize>,

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

    /// Mask erosion iterations
    #[arg(long, num_args = 1..)]
    pub mask_erosions: Option<Vec<usize>>,

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
    pub ilsqr_params: IlsqrParamArgs,
    #[command(flatten)]
    pub qsmart_params: QsmartParamArgs,
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
    pub romeo_params: RomeoParamArgs,
    #[command(flatten)]
    pub swi_params: SwiParamArgs,


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

    /// Also export final maps as DICOM series into each subject's extra_files/ folder
    #[arg(long)]
    pub export_dicom: bool,

    /// Optional source DICOM directory; inherit patient/study identity from the
    /// original DICOMs when exporting (used with --export-dicom)
    #[arg(long)]
    pub source_dicom: Option<PathBuf>,

    /// Restrict DICOM export to these maps (default: all produced). Values:
    /// chimap, swi, minip, t2starmap, r2starmap (used with --export-dicom)
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

    /// Output directory (defaults to bids_dir; outputs go into <dir>/derivatives/qsmxt.rs/)
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
    pub max_radius_factor: Option<f64>,
    /// Min radius factor (multiplied by max voxel size)
    #[arg(long)]
    pub min_radius_factor: Option<f64>,
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
    pub radius_factor: Option<f64>,
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
    pub radius_factor: Option<f64>,
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
    /// Iterative Least Squares QR
    Ilsqr(InvertIlsqrArgs),
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
    /// Input multi-echo magnitude NIfTI files (3+ echoes required)
    #[arg(required = true, num_args = 3..)]
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
    /// Input multi-echo magnitude NIfTI files (3+ echoes required)
    #[arg(required = true, num_args = 3..)]
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
    Rts, Tv, Tkd, Tsvd, Tgv, Tikhonov, Nltv, Medi, Ilsqr, Qsmart,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum UnwrapAlgorithmArg {
    Romeo, Laplacian,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum BfAlgorithmArg {
    Vsharp, Pdf, Lbv, Ismv, Sharp, Resharp, Harperella, Iharperella,
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
pub enum MaskAlgorithmArg {
    Bet, Threshold,
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
