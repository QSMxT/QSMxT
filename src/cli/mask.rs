use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use super::*;

// ─── Standalone algorithm commands (subcommand-per-algorithm) ───

// ── Mask ──

crate::flag_set! {
    /// BET's settings for every BET step, as `qsmxt run` names them (`--bet-orientation`).
    BetFlags = ["bet."];
}

#[derive(Args, Debug, Clone)]
pub struct MaskCommonArgs {
    /// Input NIfTI file (an image for the generating subcommands, a mask for the rest)
    pub input: PathBuf,
    /// Output mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Further refinement, applied in order after the subcommand's own operation (repeatable).
    /// Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority
    /// filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image
    /// input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude
    /// input)
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
    /// STI Suite QSM_GUI's thresholding: the level its thresh_tool starts at (Otsu's threshold of
    /// the middle slice, scaled to that slice's maximum)
    StiGui(MaskOtsuArgs),
    /// Brain extraction (BET)
    Bet(MaskBetArgs),
    /// Deep-learning brain extraction (HD-BET; downloads weights on first use)
    HdBet(MaskHdBetArgs),
    /// Deep-learning rodent brain extraction (RS2-Net; downloads weights on first use)
    Rs2Net(MaskRs2NetArgs),
    /// Deep-learning brain extraction for any contrast (MindGrab; downloads weights on first use)
    Mindgrab(MaskMindGrabArgs),
    /// Run a `--mask-preset` recipe on an image (robust-threshold, bet, hd-bet, bet-and-phase,
    /// rs2-net, mindgrab)
    Preset(MaskPresetArgs),
    /// Alias for `preset robust-threshold`
    Robust(MaskRobustArgs),
    /// Erode a binary mask
    Erode(MaskErodeArgs),
    /// Dilate a binary mask
    Dilate(MaskDilateArgs),
    /// Morphological closing on a binary mask
    Close(MaskCloseArgs),
    /// 3x3x3 majority filter on a binary mask (MATLAB's bwmorph3 'majority')
    Majority(MaskMajorityArgs),
    /// Keep only the largest 26-connected component of a binary mask (MATLAB's bwlabeln)
    LargestComponent(MaskMajorityArgs),
    /// Fill every hole of a binary mask, whatever its size (MATLAB's imfill 'holes')
    Imfill(MaskMajorityArgs),
    /// Fill holes in a binary mask
    FillHoles(MaskFillHolesArgs),
    /// Gaussian smooth a binary mask (re-thresholds at 0.5)
    Smooth(MaskSmoothArgs),
    /// Intersect binary masks: keep voxels every input keeps (the `run --mask-combine and`)
    And(MaskCombineCliArgs),
    /// Union binary masks: keep voxels any input keeps (the `run --mask-combine or`)
    Or(MaskCombineCliArgs),
}

#[derive(Parser, Debug)]
pub struct MaskCombineCliArgs {
    /// Input mask NIfTI files (two or more, on the same grid)
    #[arg(num_args = 2..)]
    pub inputs: Vec<PathBuf>,
    /// Output mask NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// Refinement operation applied to the combined mask (repeatable, applied in order) —
    /// the `run --mask-refine` of this command. Examples: fill-holes:0, erode:1, close:1
    #[arg(long = "op")]
    pub ops: Vec<String>,
    /// Magnitude image, needed only by `--op signal-erode`
    #[arg(long)]
    pub magnitude: Option<PathBuf>,
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
    #[arg(long, default_value_t = qsmxt_engine::BetConfig::default().fractional_intensity)]
    pub fractional_intensity: f64,
    /// Factor applied to the voxel sizes before BET runs — 10 for mouse data, 1 for the
    /// acquisition's own geometry (see --bet-voxel-scale). Prefer `mask rs2-net`, which masks
    /// rodent brains without the trick
    #[arg(long, value_name = "FACTOR")]
    pub voxel_scale: Option<f64>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<BetFlags>,
}

#[derive(Parser, Debug)]
pub struct MaskHdBetArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Run in 192x192x96 sliding-window patches (about 3.6–4.5 GB measured) instead of on the whole
    /// volume (the default; about 6–8 GB measured, and about twice as fast)
    #[arg(long)]
    pub window: bool,
    /// Use 128x128x64 sliding-window patches (about 1.5–1.9 GB measured)
    #[arg(long)]
    pub low_memory: bool,
    /// Sliding-window patch size XxYxZ in voxels at 1 mm (multiples of 32x32x16)
    #[arg(long)]
    pub patch: Option<String>,
    /// 8-fold mirroring test-time augmentation (about 8x slower)
    #[arg(long)]
    pub tta: bool,
    /// Sliding-window step as a fraction of the patch, in (0, 1]. Larger steps mean fewer
    /// patches and a shorter run: 0.5 (default) is HD-BET's own, 1.0 abuts the patches.
    /// Implies --window.
    #[arg(long)]
    pub tile_step: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct MaskRs2NetArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// 8-fold mirroring test-time augmentation (about 8x slower)
    #[arg(long)]
    pub tta: bool,
    /// Sliding-window step as a fraction of the patch, in (0, 1]. Larger steps mean fewer
    /// patches and a shorter run: 0.5 (default) is nnU-Net's own, 1.0 abuts the patches.
    #[arg(long)]
    pub tile_step: Option<f64>,
}

#[derive(Parser, Debug)]
pub struct MaskMindGrabArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Left-right flip test-time augmentation (about 2x slower)
    #[arg(long)]
    pub tta: bool,
}

#[derive(Parser, Debug)]
pub struct MaskRobustArgs {
    #[command(flatten)]
    pub common: MaskCommonArgs,
}

#[derive(Parser, Debug)]
pub struct MaskPresetArgs {
    /// Which recipe — the same names as `run --mask-preset`
    #[arg(value_enum)]
    pub preset: MaskPresetArg,
    #[command(flatten)]
    pub common: MaskCommonArgs,
    /// Phase-quality map (from `qsmxt quality-map`) for the sections that read phase quality.
    /// Without it those sections read the input image instead, as `run --masking-input` would.
    #[arg(long)]
    pub quality: Option<PathBuf>,
    /// Phase image (3-D, or 4-D multi-echo): the mask leaves out voxels where the scanner recorded
    /// no phase (a constant value in every echo), as `qsmxt run` does for every recipe. A
    /// phase-quality map rates that flat phase as reliable, so give this with `--quality`
    #[arg(long)]
    pub phase: Option<PathBuf>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<BetFlags>,
}
