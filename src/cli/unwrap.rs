use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use super::*;

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

// ── Combine ──

#[derive(Subcommand, Debug)]
pub enum CombineCommand {
    /// MCPC-3D-S (ASPIRE) coil combination from per-coil multi-echo phase + magnitude
    Mcpc3ds(CombineMcpc3dsArgs),
}

#[derive(Parser, Debug)]
pub struct CombineMcpc3dsArgs {
    /// Per-coil, per-echo wrapped phase NIfTI files (3D each). Files carrying BIDS `coil-NN`
    /// and `echo-N` entities are ordered by those; otherwise they are taken coil-major in the
    /// given order and `--num-echoes` is required.
    #[arg(long, num_args = 1.., required = true)]
    pub phase: Vec<PathBuf>,
    /// Per-coil, per-echo magnitude NIfTI files, same layout as --phase
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// Output prefix: writes <prefix>_rec-mcpc3ds_echo-N_part-phase_<suffix>.nii,
    /// <prefix>_rec-mcpc3ds_echo-N_part-mag_<suffix>.nii and <prefix>_desc-mcpc3ds_mask.nii — the
    /// pipeline's names. <suffix> is the inputs' BIDS suffix (MEGRE when they carry none)
    #[arg(short, long)]
    pub output: PathBuf,
    /// Echo times in seconds, one per echo (read from the phase JSON sidecars when omitted)
    #[arg(long, num_args = 1..)]
    pub tes: Option<Vec<f64>>,
    /// Number of echoes when the file names carry no `echo-N` entity
    #[arg(long)]
    pub num_echoes: Option<usize>,
    /// Smoothing sigma (voxels) for the per-coil phase offsets (default 10 10 5)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"])]
    pub sigma: Option<Vec<f64>>,
    /// Unwrapping used on the coil-summed HIP phase
    #[arg(long, value_enum, default_value = "romeo")]
    pub hip_unwrapping: UnwrapAlgorithmArg,
    /// PRELUDE's settings, with `--hip-unwrapping prelude`
    #[command(flatten)]
    pub prelude_params: PreludeParamArgs,
}

#[derive(Subcommand, Debug)]
pub enum UnwrapCommand {
    /// ROMEO region-growing unwrapping
    Romeo(UnwrapRomeoArgs),
    /// Laplacian-based unwrapping
    Laplacian(UnwrapLaplacianArgs),
    /// PRELUDE region-merging unwrapping
    Prelude(UnwrapPreludeArgs),
    /// MEDI's magnitude-guided region-growing unwrapping (takes no mask)
    MediRegionGrowing(UnwrapMediRegionGrowingArgs),
}

/// `qsmxt unwrap medi-region-growing`: no mask, as MEDI's unwrapper grows over the whole volume.
#[derive(Parser, Debug)]
pub struct UnwrapMediRegionGrowingArgs {
    /// Input wrapped phase NIfTI file
    pub input: PathBuf,
    /// Magnitude NIfTI file (3D): the order in which the volume is unwrapped. Required.
    #[arg(long)]
    pub magnitude: PathBuf,
    /// Output unwrapped phase NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
}

#[derive(Parser, Debug)]
pub struct UnwrapRomeoArgs {
    #[command(flatten)]
    pub common: UnwrapCommonArgs,
    /// Magnitude image (improves quality)
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
    /// The ROMEO settings `qsmxt run` takes. The multi-echo ones are refused here: this unwraps
    /// a single 3D phase.
    #[command(flatten)]
    pub romeo_params: RomeoParamArgs,
}

#[derive(Parser, Debug)]
pub struct UnwrapLaplacianArgs {
    #[command(flatten)]
    pub common: UnwrapCommonArgs,
}

#[derive(Parser, Debug)]
pub struct UnwrapPreludeArgs {
    #[command(flatten)]
    pub common: UnwrapCommonArgs,
    #[command(flatten)]
    pub prelude_params: PreludeParamArgs,
}
