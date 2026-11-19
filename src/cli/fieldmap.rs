use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use super::*;

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
    /// `--b0-estimation qsmart-echofit`: how the fit residual at which a voxel is marked
    /// unreliable is set: a percentile of the blurred residual over the volume
    /// (`--qsmart-fit-percentile`), or a fixed residual (`--qsmart-fit-threshold`; default, QSMART v1.0's 40).
    /// Without it, giving one of those two selects its mode
    #[arg(long, value_enum)]
    pub qsmart_fit_threshold_mode: Option<QsmartFitThresholdModeArg>,
    /// `--b0-estimation qsmart-echofit`: the percentile of the blurred fit residual, in (0, 100],
    /// above which a voxel is marked unreliable (default 99)
    #[arg(long)]
    pub qsmart_fit_percentile: Option<f64>,
    /// `--b0-estimation qsmart-echofit`: a fixed blurred fit residual above which a voxel is marked
    /// unreliable, instead of a percentile (40 when the mode is `absolute` and none is given)
    #[arg(long)]
    pub qsmart_fit_threshold: Option<f64>,
    /// `--b0-estimation qsmart-echofit`: also save its reliability map (1 = reliable) here, for
    /// `qsmxt qsmart --reliability`
    #[arg(long)]
    pub reliability_output: Option<PathBuf>,
    /// Also save the mask the field map is defined in here: the mask, less the voxels where
    /// phase offset removal could not estimate the phase offset (the field is 0 there). Pass it
    /// to `qsmxt bgremove` and the inversions in place of the mask
    #[arg(long)]
    pub field_mask_output: Option<PathBuf>,
    /// B0 weighted-averaging weight type. `assumed-decay` gives each echo's phase a fixed weight
    /// TE·exp(−TE/T2*) from the echo times and an assumed T2* (no T2* map, no magnitude; Wang et
    /// al. 2022). `phase-snr` is generally preferable with magnitude
    #[arg(long, value_enum, default_value = "phase-snr")]
    pub b0_weight_type: B0WeightTypeArg,
    /// T2* in ms assumed by `--b0-weight-type assumed-decay` (no T2* map is used)
    #[arg(long, value_name = "MS", default_value_t = 40.0)]
    pub b0_weight_assumed_t2star: f64,
    /// Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs
    /// 4 echoes with it, 3 without
    #[arg(long)]
    pub linear_fit_estimate_offset: Option<bool>,
    /// Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights.
    /// `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects
    #[arg(long, value_enum)]
    pub robust_fit_loss: Option<RobustLossArg>,
    /// Motion-robust fit: tuning radius in robust-scale units (default: the loss's
    /// conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive
    #[arg(long)]
    pub robust_fit_tuning: Option<f64>,
    /// Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's)
    #[arg(long)]
    pub robust_fit_iterations: Option<usize>,
    /// Motion-robust fit (reporting only): a robust weight below this counts as down-weighted
    /// (default 0.75; in (0, 1])
    #[arg(long)]
    pub robust_fit_downweight_level: Option<f64>,
    /// Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many
    /// times the best echo's (default 3; at least 1)
    #[arg(long)]
    pub robust_fit_flag_ratio: Option<f64>,
    /// Complex fit (`--b0-estimation complex-fit`): stop once the update is this fraction of the
    /// first estimate, over the whole volume (default 1e-4)
    #[arg(long)]
    pub complex_fit_reltol: Option<f64>,
    /// Complex fit: Gauss-Newton iterations at most (default 30)
    #[arg(long)]
    pub complex_fit_max_iter: Option<usize>,
    /// `--b0-estimation complex-fit`: also save the fit itself here, before spatial unwrapping:
    /// the phase accrued per echo spacing (radians, wrapped to [-π, π]; MEDI's `iFreq_raw`)
    #[arg(long, value_name = "FILE")]
    pub complex_fit_output: Option<PathBuf>,
    /// `--b0-estimation complex-fit`: also save the fit's noise map here: the standard deviation
    /// of the phase per echo spacing, in radians (MEDI's `N_std`)
    #[arg(long, value_name = "FILE")]
    pub complex_fit_noise_output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum FieldmapCommand {
    /// ROMEO-based multi-echo field mapping (phase offset removal + unwrap + B0 fit)
    Romeo(FieldmapRomeoArgs),
    /// Laplacian-based multi-echo field mapping (per-echo unwrap + echo combination)
    Laplacian(FieldmapLaplacianArgs),
    /// PRELUDE-based multi-echo field mapping (phase offset removal + per-echo unwrap + B0 fit)
    Prelude(FieldmapPreludeArgs),
    /// MEDI region-growing field mapping (phase offset removal + per-echo unwrap + B0 fit; with
    /// `--b0-estimation complex-fit`, MEDI's field map). Needs `--magnitude`.
    MediRegionGrowing(FieldmapMediRegionGrowingArgs),
}

#[derive(Parser, Debug)]
pub struct FieldmapMediRegionGrowingArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    /// Enable phase offset removal for multi-echo data (default: true)
    #[arg(long)]
    pub phase_offset_removal: Option<bool>,
    /// Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command
    /// line's)
    #[arg(long, num_args = 3)]
    pub phase_offset_sigma_mm: Option<Vec<f64>>,
    /// Enable bipolar gradient correction (requires >= 3 echoes)
    #[arg(long)]
    pub bipolar_correction: bool,
}

#[derive(Parser, Debug)]
pub struct FieldmapRomeoArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    /// Enable phase offset removal for multi-echo data (default: true)
    #[arg(long)]
    pub phase_offset_removal: Option<bool>,
    /// Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command
    /// line's)
    #[arg(long, num_args = 3)]
    pub phase_offset_sigma_mm: Option<Vec<f64>>,
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
    /// Poisson solver: `dct` (unweighted least squares, Neumann/DCT solve), `fft` (Schofield &
    /// Zhu, FFT solve on a zero-padded volume), `mirror` (the same on the volume mirrored along
    /// each axis, as QSMART unwraps) or `mgpcg` (QSM.jl's MGPCG: Laplacian unwrapping with LBV
    /// background removal in one step; QSM.jl follows it with V-SHARP to remove remaining
    /// background)
    #[arg(long, value_enum, default_value = "dct", alias = "laplacian-kernel")]
    pub laplacian_solver: LaplacianSolverArg,
    /// Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or
    /// three (x y z)
    #[arg(long, value_name = "VOXELS", num_args = 1..=3, default_values_t = [64])]
    pub laplacian_fft_pad: Vec<usize>,
    /// Unwrap the raw phase over the whole volume, instead of the phase set to zero outside the
    /// mask, as QSMART does. With `--b0-estimation qsmart-echofit` the fit then covers the whole
    /// volume (the saved field map is still set to zero outside the mask)
    #[arg(long)]
    pub laplacian_unmasked: bool,
    /// Enable phase offset removal for multi-echo data (default: true)
    #[arg(long)]
    pub phase_offset_removal: Option<bool>,
    /// Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command
    /// line's)
    #[arg(long, num_args = 3)]
    pub phase_offset_sigma_mm: Option<Vec<f64>>,
    /// Enable bipolar gradient correction (requires >= 3 echoes)
    #[arg(long)]
    pub bipolar_correction: bool,
}

#[derive(Parser, Debug)]
pub struct FieldmapPreludeArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    /// Enable phase offset removal for multi-echo data (default: true)
    #[arg(long)]
    pub phase_offset_removal: Option<bool>,
    /// Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command
    /// line's)
    #[arg(long, num_args = 3)]
    pub phase_offset_sigma_mm: Option<Vec<f64>>,
    /// Enable bipolar gradient correction (requires >= 3 echoes)
    #[arg(long)]
    pub bipolar_correction: bool,
    #[command(flatten)]
    pub prelude_params: PreludeParamArgs,
}
