use clap::Parser;
use std::path::PathBuf;

use super::*;

#[derive(Parser, Debug)]
pub struct InvertRtsArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<RtsFlags>,
}

#[derive(Parser, Debug)]
pub struct InvertTvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<TvFlags>,
}

/// TGV of a field map in ppm, which TGV expresses as phase at the echo time `--te` picks.
#[derive(Parser, Debug)]
pub struct InvertTgvArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
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
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<TgvFlags>,
}

#[derive(Parser, Debug)]
pub struct InvertTikhonovArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<TikhonovFlags>,
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
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<NltvFlags>,
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
    /// Hidden: `--smv true|false`, the earlier spelling of `--medi-smv` / `--no-medi-smv`
    #[arg(long, hide = true, value_name = "BOOL")]
    pub smv: Option<bool>,
    /// Hidden: `--smv-shrink-mask true|false`, the earlier spelling of `--medi-smv-shrink-mask`
    /// / `--no-medi-smv-shrink-mask`
    #[arg(long, hide = true, value_name = "BOOL")]
    pub smv_shrink_mask: Option<bool>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<MediFlags>,
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
    /// Dipole kernel on a grid with an odd number of voxels along an axis: default samples the
    /// dipole on the Fourier grid; original samples it half a frequency step off, as the original
    /// implementation does, for comparison with it. Identical on even grids (default: default)
    #[arg(long, value_enum, value_name = "KERNEL")]
    pub kernel: Option<TfiKernelArg>,
    /// Arithmetic: double, or single as the original implementation runs, for comparison with
    /// it (default: double)
    #[arg(long, value_enum, value_name = "PRECISION")]
    pub precision: Option<TfiPrecisionArg>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<TfiFlags>,
}

/// STAR-QSM. Its result depends on the scale of the field in radians, so it takes the scan's
/// field strength and echo times and converts the field to phase at the echo time `--te` picks.
#[derive(Parser, Debug)]
pub struct InvertStarArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<StarFlags>,
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
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<FansiFlags>,
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
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<FansiFlags>,
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
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<L1qsmFlags>,
}

#[derive(Parser, Debug)]
pub struct InvertWhqsmArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, for a magnitude data weight (--weight): one 3D volume, a 4D one, or one
    /// per echo; several echoes are RSS-combined
    #[arg(long, num_args = 1..)]
    pub magnitude: Vec<PathBuf>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<WhqsmFlags>,
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
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<HdqsmFlags>,
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
    #[arg(long)]
    pub b0: f64,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<AmpPeFlags>,
}
