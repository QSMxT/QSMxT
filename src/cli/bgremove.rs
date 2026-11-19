use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use super::*;

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
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
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
    /// BFRnet deep-learning background removal (weights downloaded on first use)
    Bfrnet(BgremoveBfrnetArgs),
    /// iQFM deep-learning joint unwrapping and background removal — input is every echo's
    /// WRAPPED phase (weights downloaded on first use)
    Iqfm(IqsmArgs),
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
    /// Undo the SMV high-pass with a truncated-SVD deconvolution (default: false)
    #[arg(long, value_name = "BOOL")]
    pub deconvolve: Option<bool>,
    /// Deconvolution (TSVD) threshold; only used with --deconvolve true
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Largest SMV kernel radius, in mm
    #[arg(long)]
    pub max_radius: Option<f64>,
    /// Smallest SMV kernel radius, in mm
    #[arg(long)]
    pub min_radius: Option<f64>,
    /// Step between SMV kernel radii, in mm (default: 1); the radii run from --max-radius down to
    /// --min-radius
    #[arg(long, value_name = "MM")]
    pub radius_step: Option<f64>,
    /// Grid the background is estimated on: `isotropic` (default; spacing from --grid-spacing),
    /// `native` (the acquired grid), or `finest` (isotropic at the smallest voxel dimension,
    /// e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened). The local field keeps native
    /// resolution
    #[arg(long, value_enum)]
    pub grid: Option<VsharpGridArg>,
    /// Isotropic working-grid spacing in mm (default 1); used with --grid isotropic
    #[arg(long, value_name = "MM")]
    pub grid_spacing: Option<f64>,
    /// SMV ball: `default` (the voxels within r + 1/6 mm, centre excluded; a ball fits where all
    /// of it is in the mask) or `qsmjl` (QSM.jl's: within r, centre included)
    #[arg(long, value_enum)]
    pub ball: Option<VsharpBallArg>,
}

/// PDF weights the fit by a noise map it makes from every echo's magnitude (MEDI toolbox
/// `Fit_ppm_complex`), so it takes the echoes' magnitudes, echo times and field strength.
#[derive(Parser, Debug)]
pub struct BgremovePdfArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude of every echo: one 4D NIfTI, or one 3D NIfTI per echo in echo order
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Tolerance, relative to the noise level
    #[arg(long)]
    pub tol: Option<f64>,
    /// Maximum conjugate-gradient iterations (default: 30)
    #[arg(long)]
    pub n_cg: Option<usize>,
    /// Dipole kernel: image (the FFT of the dipole sampled on the voxel grid) or kspace (the
    /// analytic k-space kernel) (default: image)
    #[arg(long, value_name = "SPACE", value_parser = parse_pdf_space)]
    pub space: Option<qsmxt_engine::PdfSpace>,
    /// Zero-padding around the mask's bounding box, in voxels; 0 runs on the full volume
    /// (default: 40)
    #[arg(long, value_name = "VOXELS")]
    pub n_pad: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct BgremoveLbvArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Tolerance: stop the coarsest grid's relaxation when its summed absolute update falls below
    /// this, in radians over the echo spacing (default: 0.01)
    #[arg(long)]
    pub tol: Option<f64>,
    /// Coarsest multigrid level, at most 8 (default: log2 of the smallest dimension minus 3)
    #[arg(long, value_name = "LEVEL")]
    pub depth: Option<usize>,
    /// Boundary layers removed from the mask beyond the first (default: 0)
    #[arg(long, value_name = "LAYERS")]
    pub peel: Option<usize>,
    /// Relaxations on each grid before the coarser grid (default: 30)
    #[arg(long)]
    pub n1: Option<usize>,
    /// Relaxations on each grid after the coarser grid (default: 100)
    #[arg(long)]
    pub n2: Option<usize>,
    /// Relaxations on the finest grid at the end (default: 100)
    #[arg(long)]
    pub n3: Option<usize>,
    /// Arithmetic: double, or single as the original implementation (default: double)
    #[arg(long, value_name = "PRECISION", value_parser = parse_lbv_precision)]
    pub precision: Option<qsmxt_engine::LbvPrecision>,
}

#[derive(Parser, Debug)]
pub struct BgremoveIsmvArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Implementation: wen2014 (the authors' paper and code) or qsmjl (QSM.jl's) (default: wen2014)
    #[arg(long, value_name = "VARIANT", value_parser = parse_ismv_variant)]
    pub variant: Option<qsmxt_engine::IsmvVariant>,
    /// Tolerance: the relative change between iterates it stops at (qsmjl: the residual relative
    /// to the first) (default: 5e-5)
    #[arg(long)]
    pub tol: Option<f64>,
    /// Max iterations (default: no limit)
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Kernel radius in mm (default: 6 × the smallest voxel dimension; qsmjl: 2 × the largest)
    #[arg(long)]
    pub radius: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct BgremoveSharpArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Deconvolution (TSVD) threshold (default: 0.03; qsmjl: 0.05)
    #[arg(long)]
    pub threshold: Option<f64>,
    /// Sphere radius in mm (default: 4; qsmjl: 18 × the smallest voxel dimension)
    #[arg(long)]
    pub radius: Option<f64>,
    /// Implementation: sepia (SEPIA's SHARP.m, the default) or qsmjl (QSM.jl's `sharp`, with its
    /// own defaults)
    #[arg(long, value_enum)]
    pub variant: Option<SharpVariantArg>,
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
    /// Conjugate gradients squared (CGS) relative tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Maximum CGS iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct BgremoveHarperellaArgs {
    /// Input NIfTI: a wrapped echo in rad (with --te and --field-strength) or a total field in ppm
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
    /// Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped
    /// echo in rad (the published method); without both, it is an unwrapped total field in ppm.
    /// The local field is written in ppm either way
    #[arg(long, requires = "field_strength")]
    pub te: Option<f64>,
    /// B0 field strength (T) of a wrapped-phase input — see --te
    #[arg(long, requires = "te")]
    pub field_strength: Option<f64>,
    /// Zero-padding in voxels per side, X Y Z (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"])]
    pub pad: Option<Vec<usize>>,
    /// Boundary rim: whole (zero every Laplacian value in the rim; default) or sti (zero the
    /// 20 % largest, as STI Suite does)
    #[arg(long, value_name = "RIM", value_parser = parse_harperella_rim)]
    pub rim: Option<qsmxt_engine::HarperellaRim>,
}

#[derive(Parser, Debug)]
pub struct BgremoveIharperellaArgs {
    /// Input NIfTI: a wrapped echo in rad (with --te and --field-strength) or a total field in ppm
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
    /// Maximum iterations
    #[arg(long)]
    pub max_iter: Option<usize>,
    /// Convergence tolerance
    #[arg(long)]
    pub tol: Option<f64>,
    /// Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped
    /// echo in rad, unwrapped by HARPERELLA's Laplacian; without both, it is an unwrapped total
    /// field in ppm, which iHARPERELLA takes as it is. The local field is written in ppm either
    /// way
    #[arg(long, requires = "field_strength")]
    pub te: Option<f64>,
    /// B0 field strength (T) of a wrapped-phase input — see --te
    #[arg(long, requires = "te")]
    pub field_strength: Option<f64>,
    /// Zero-padding in voxels per side, X Y Z (default: 0 0 0)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"])]
    pub pad: Option<Vec<usize>>,
    /// Boundary rim: whole (zero every Laplacian value in the rim) or sti (zero the 20 %
    /// largest, as STI Suite does; default)
    #[arg(long, value_name = "RIM", value_parser = parse_harperella_rim)]
    pub rim: Option<qsmxt_engine::HarperellaRim>,
}
