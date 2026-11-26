use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use super::*;

// ── Bgremove ──

crate::flag_set! {
    VsharpFlags = ["bg_removal.vsharp."];
    PdfFlags = ["bg_removal.pdf."];
    LbvFlags = ["bg_removal.lbv."];
    IsmvFlags = ["bg_removal.ismv."];
    SharpFlags = ["bg_removal.sharp."];
    ResharpFlags = ["bg_removal.resharp."];
    HarperellaFlags = ["bg_removal.harperella."];
    IharperellaFlags = ["bg_removal.iharperella."];
}

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
    /// SMV ball: `default` (the voxels within r + 1/6 mm, centre excluded; a ball fits where all
    /// of it is in the mask) or `qsmjl` (QSM.jl's: within r, centre included)
    #[arg(long, value_enum)]
    pub ball: Option<VsharpBallArg>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<VsharpFlags>,
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
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<PdfFlags>,
}

#[derive(Parser, Debug)]
pub struct BgremoveLbvArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<LbvFlags>,
}

#[derive(Parser, Debug)]
pub struct BgremoveIsmvArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<IsmvFlags>,
}

#[derive(Parser, Debug)]
pub struct BgremoveSharpArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    /// Implementation: sepia (SEPIA's SHARP.m, the default) or qsmjl (QSM.jl's `sharp`, with its
    /// own defaults)
    #[arg(long, value_enum)]
    pub variant: Option<SharpVariantArg>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<SharpFlags>,
}

#[derive(Parser, Debug)]
pub struct BgremoveResharpArgs {
    #[command(flatten)]
    pub common: BgremoveCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<ResharpFlags>,
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
    /// Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped
    /// echo in rad (the published method); if both are omitted, it is an unwrapped total field in ppm.
    /// The local field is written in ppm either way
    #[arg(long, requires = "field_strength")]
    pub te: Option<f64>,
    /// B0 field strength (T) of a wrapped-phase input — see --te for when both are omitted
    #[arg(long, requires = "te")]
    pub field_strength: Option<f64>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<HarperellaFlags>,
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
    /// Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped
    /// echo in rad, unwrapped by HARPERELLA's Laplacian; if both are omitted, it is an unwrapped total
    /// field in ppm, which iHARPERELLA takes as it is. The local field is written in ppm either
    /// way
    #[arg(long, requires = "field_strength")]
    pub te: Option<f64>,
    /// B0 field strength (T) of a wrapped-phase input — see --te for when both are omitted
    #[arg(long, requires = "te")]
    pub field_strength: Option<f64>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<IharperellaFlags>,
}
