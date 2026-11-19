use clap::Parser;
use std::path::PathBuf;

use super::*;

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
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// B0 field strength in Tesla
    #[arg(long)]
    pub field_strength: f64,
    /// Echo time in seconds
    #[arg(long)]
    pub echo_time: f64,
    /// Magnitude NIfTI (optional; drives vasculature detection). With --qsmart-vessel-magnitude
    /// mean-n4 (the default) every echo's magnitude (4D), averaged and N4 bias-corrected; with
    /// `combined`, a combined magnitude (3D), used as it is
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    /// QSMART's reliability map from its echo fit (`qsmxt fieldmap laplacian --b0-estimation
    /// qsmart-echofit --reliability-output`): both stages are reconstructed inside it
    #[arg(long)]
    pub reliability: Option<PathBuf>,
    /// Also write QSMART's intermediate maps into this directory, as <name>.nii:
    /// vessel_mask (1 = vessel), localfield_stage1, chimap_stage1, localfield_stage2 and
    /// chimap_stage2 (ppm), and vessel_magnitude when QSMART makes its own (mean-n4)
    #[arg(long, value_name = "DIR")]
    pub intermediates: Option<PathBuf>,
    /// Arithmetic of the inner iLSQR's two LSQR solves (`--qsmart-inversion ilsqr`): double, or
    /// single as STI Suite's QSM_iLSQR, which QSMART calls (default: double; `qsmxt run
    /// --ilsqr-precision`)
    #[arg(long, value_name = "PRECISION", value_parser = parse_ilsqr_precision)]
    pub ilsqr_precision: Option<qsmxt_engine::IlsqrPrecision>,
    #[command(flatten)]
    pub qsmart_params: QsmartParamArgs,
}

#[derive(Parser, Debug)]
pub struct SegmentArgs {
    /// Input magnitude NIfTI file. Not a susceptibility map: SynthSeg reads anatomy, and χ has
    /// none of the tissue contrast it was trained to recognise
    pub magnitude: PathBuf,
    /// Output label NIfTI file (FreeSurfer label ids)
    #[arg(short, long)]
    pub output: PathBuf,
    /// Also write the BIDS label lookup table (index/name) to this path
    #[arg(long)]
    pub lookup: Option<PathBuf>,
    #[command(flatten)]
    pub params: SegmentationParamArgs,
}

#[derive(Parser, Debug)]
pub struct SwiArgs {
    /// Wrapped phase NIfTI files, one per echo, in echo order
    #[arg(long, num_args = 1.., required = true)]
    pub phase: Vec<PathBuf>,
    /// Magnitude NIfTI files, one per echo, in the same order as --phase
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Echo times in seconds, one per echo (read from the phase JSON sidecars when omitted;
    /// not needed for a single echo)
    #[arg(long, num_args = 1..)]
    pub tes: Option<Vec<f64>>,
    /// Mask the phase is filtered within (default: CLEAR-SWI's own, from the first echo's
    /// magnitude)
    #[arg(short, long)]
    pub mask: Option<PathBuf>,
    /// Output SWI NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Also compute minimum intensity projection
    #[arg(long)]
    pub mip: bool,
    /// Output path for MIP (default: the --output name with its `swi` suffix replaced by
    /// `minIP`, or `_minIP` appended — `sub-01_swi.nii` → `sub-01_minIP.nii`, as the pipeline
    /// names them)
    #[arg(long)]
    pub mip_output: Option<PathBuf>,
    /// Also write CLEAR-SWI's high-pass filtered phase, the phase the SWI is weighted by: Hz for
    /// several echoes, radians for one, 0 outside the mask (`qsmxt run --swi-filtered-phase`)
    #[arg(long)]
    pub filtered_phase: bool,
    /// Output path for the filtered phase (default: the --output name with its `swi` suffix
    /// replaced by `desc-swi_phase`, or `_desc-swi_phase` appended — `sub-01_swi.nii` →
    /// `sub-01_desc-swi_phase.nii`, as the pipeline names it). Implies --filtered-phase
    #[arg(long)]
    pub filtered_phase_output: Option<PathBuf>,
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
    /// R2 map NIfTI in Hz: R2' is measured, as R2* - R2
    #[arg(long, required_unless_present = "r2primenet", conflicts_with = "r2primenet")]
    pub r2: Option<PathBuf>,
    /// Estimate R2' from the R2* map with R2PRIMEnet (deep learning, trained at 3 T; weights
    /// downloaded on first use) instead of measuring it from an R2 map. Needs --field-strength
    #[arg(long, requires = "field_strength")]
    pub r2primenet: bool,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Binary mask of where the R2 acquisition reached, if it does not cover the whole mask
    /// (a MESE slab). Without it, coverage is inferred from R2 > 0
    #[arg(long, conflicts_with = "r2primenet")]
    pub r2_coverage: Option<PathBuf>,
    /// Output R2' map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Also write the mask of voxels where R2' is a measurement
    #[arg(long, conflicts_with = "r2primenet")]
    pub output_coverage: Option<PathBuf>,
    /// B0 field strength in Tesla (--r2primenet: R2PRIMEnet refuses a field strength other than
    /// 3 T unless --r2primenet-ignore-field-mismatch is given)
    #[arg(long, conflicts_with = "r2")]
    pub field_strength: Option<f64>,
    /// Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its
    /// weights were trained at. The R2' is then uncalibrated
    #[arg(long, conflicts_with = "r2")]
    pub r2primenet_ignore_field_mismatch: bool,
    /// R2PRIMEnet sliding-window patch in voxels, D H W, each a multiple of 16
    /// (default: 192 192 128)
    #[arg(long, num_args = 3, value_names = ["D", "H", "W"], conflicts_with = "r2")]
    pub r2primenet_patch: Option<Vec<usize>>,
}

#[derive(Parser, Debug)]
pub struct HomogeneityArgs {
    /// Input magnitude NIfTI file
    pub input: PathBuf,
    /// Output corrected magnitude NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Correction method: `makehomogeneous` (--sigma, --nbox) or `n4` (N4 bias field correction,
    /// as ANTs' N4BiasFieldCorrection; --n4-*)
    #[arg(long, value_enum, default_value_t = InhomogeneityMethodArg::Makehomogeneous)]
    pub method: InhomogeneityMethodArg,
    /// makehomogeneous: smoothing sigma in mm
    #[arg(long, default_value_t = qsmxt_engine::HomogeneityConfig::default().sigma_mm)]
    pub sigma: f64,
    /// makehomogeneous: boxes per dimension for the tissue segmentation the receive sensitivity
    /// is estimated from
    #[arg(long, default_value_t = qsmxt_engine::HomogeneityConfig::default().nbox)]
    pub nbox: usize,
    #[command(flatten)]
    pub n4_params: N4ParamArgs,
}

#[derive(Parser, Debug)]
pub struct MaskErodeArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Number of erosion iterations
    #[arg(long, default_value_t = 1)]
    pub iterations: usize,
}

#[derive(Parser, Debug)]
pub struct MaskDilateArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Number of dilation iterations
    #[arg(long, default_value_t = 1)]
    pub iterations: usize,
}

#[derive(Parser, Debug)]
pub struct MaskCloseArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Closing radius
    #[arg(long, default_value_t = 1)]
    pub radius: usize,
}

#[derive(Parser, Debug)]
pub struct MaskMajorityArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
}

#[derive(Parser, Debug)]
pub struct MaskFillHolesArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Maximum hole size in voxels (0 = automatic, 5% of the volume — what every preset uses)
    #[arg(long, default_value_t = 0)]
    pub max_size: usize,
}

#[derive(Parser, Debug)]
pub struct MaskSmoothArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Gaussian sigma in mm
    #[arg(long, default_value_t = 4.0)]
    pub sigma: f64,
}

#[derive(Parser, Debug)]
pub struct ResampleArgs {
    /// Input NIfTI file. Treated as continuous data (magnitude, an unwrapped field map, chi).
    /// For wrapped phase pass --phase instead, with its magnitude, so the interpolation can go
    /// through the complex domain and survive the wraps.
    pub input: Option<PathBuf>,
    /// Output resampled NIfTI file (for the positional input)
    #[arg(short, long, conflicts_with = "phase")]
    pub output: Option<PathBuf>,
    /// Wrapped phase to resample together with --magnitude (complex-domain interpolation)
    #[arg(long, requires = "magnitude", conflicts_with = "input")]
    pub phase: Option<PathBuf>,
    /// Magnitude paired with --phase
    #[arg(long, requires = "phase")]
    pub magnitude: Option<PathBuf>,
    /// Where to write the resampled phase (default: <phase stem>_axial.nii)
    #[arg(long, requires = "phase")]
    pub phase_out: Option<PathBuf>,
    /// Where to write the resampled magnitude (default: <magnitude stem>_axial.nii)
    #[arg(long, requires = "phase")]
    pub magnitude_out: Option<PathBuf>,
    /// Fill the empty corners of the new grid with seeded noise, as QSMxT 8.x did. Only the
    /// --phase / --magnitude pair can be noise-filled; by default the corners are zero
    #[arg(long, requires = "phase")]
    pub noise_fill: bool,
    /// No longer has any effect: the corners are zero unless --noise-fill is given. Kept so
    /// existing scripts keep running
    #[arg(long, requires = "phase", conflicts_with = "noise_fill", hide = true)]
    pub no_noise_fill: bool,
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
