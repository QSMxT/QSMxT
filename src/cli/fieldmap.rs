use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;


// ── Fieldmap ──

crate::flag_set! {
    /// How the echoes become one field map, and the phase corrections before: the methods' common
    /// settings, under their `qsmxt run` names, which they have always had here.
    FieldmapFlags = [
        "field_mapping.b0_estimation", "field_mapping.qsmart_fit_threshold_mode", "field_mapping.qsmart_fit_percentile",
        "field_mapping.qsmart_fit_threshold", "field_mapping.b0_weight_type", "field_mapping.b0_weight_assumed_t2star_ms",
        "field_mapping.linear_fit.", "field_mapping.robust_fit.", "field_mapping.complex_fit.",
        "field_mapping.phase_offset_removal", "field_mapping.phase_offset_sigma_mm", "field_mapping.bipolar_correction",
    ], short = false;
    /// ROMEO's, as `qsmxt run` names them (`--romeo-*`).
    RomeoFlags = ["field_mapping.romeo."], short = false;
    /// PRELUDE's, as `qsmxt run` names them (`--prelude-*`).
    PreludeFlags = ["field_mapping.prelude."], short = false;
    LaplacianFlags = [
        "field_mapping.laplacian_solver", "field_mapping.laplacian_fft_pad", "field_mapping.laplacian_unmasked",
        "field_mapping.laplacian_keep_outside",
    ], short = false, aliases = [("laplacian-solver", "laplacian-kernel")];
}

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
    /// `--b0-estimation qsmart-echofit`: also save its reliability map (1 = reliable) here, for
    /// `qsmxt qsmart --reliability`
    #[arg(long)]
    pub reliability_output: Option<PathBuf>,
    /// Also save the mask the field map is defined in here: the mask, less the voxels where
    /// phase offset removal could not estimate the phase offset (the field is 0 there). Pass it
    /// to `qsmxt bgremove` and the inversions in place of the mask
    #[arg(long)]
    pub field_mask_output: Option<PathBuf>,
    /// `--b0-estimation complex-fit`: also save the fit itself here, before spatial unwrapping:
    /// the phase accrued per echo spacing (radians, wrapped to [-π, π]; MEDI's `iFreq_raw`)
    #[arg(long, value_name = "FILE")]
    pub complex_fit_output: Option<PathBuf>,
    /// `--b0-estimation complex-fit`: also save the fit's noise map here: the standard deviation
    /// of the phase per echo spacing, in radians (MEDI's `N_std`)
    #[arg(long, value_name = "FILE")]
    pub complex_fit_noise_output: Option<PathBuf>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<FieldmapFlags>,
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
}

#[derive(Parser, Debug)]
pub struct FieldmapRomeoArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<RomeoFlags>,
}

#[derive(Parser, Debug)]
pub struct FieldmapLaplacianArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<LaplacianFlags>,
}

#[derive(Parser, Debug)]
pub struct FieldmapPreludeArgs {
    #[command(flatten)]
    pub common: FieldmapCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<PreludeFlags>,
}
