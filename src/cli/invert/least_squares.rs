use clap::Parser;
use std::path::PathBuf;

use super::*;

#[derive(Parser, Debug)]
pub struct InvertTkdArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<TkdFlags>,
}

#[derive(Parser, Debug)]
pub struct InvertTsvdArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<TsvdFlags>,
}

#[derive(Parser, Debug)]
pub struct InvertIlsqrArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<IlsqrFlags>,
}

/// Minimally regularised LSQR. The residual-field weight (`--lsqr-residual-weighting`) is the only
/// thing regularising the solve, so on its own this streaks; it exists mainly as HEIDI's seed.
/// Flags carry the `--lsqr-` prefix so they read the same here as under `qsmxt run`.
#[derive(Parser, Debug)]
pub struct InvertLsqrArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    /// Magnitude NIfTI, LSQR's row weight as the HEIDI package's (its scale does not change the
    /// solution): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// B0 field strength in Tesla — sets the default residual weighting
    #[arg(long)]
    pub b0: f64,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<LsqrFlags>,
}

/// LSQR+HEIDI, HEIDI as published: it runs its own LSQR seed, so the `--lsqr-*` flags apply here
/// too. With `--seed`, HEIDI instead reduces the streaking of a map another inversion made of the
/// same field (`qsmxt run --heidi`'s step). Its result depends on the field's scale in phase units,
/// so it takes the echo times and field strength.
#[derive(Parser, Debug)]
pub struct InvertHeidiArgs {
    #[command(flatten)]
    pub common: InvertCommonArgs,
    #[command(flatten)]
    pub scan: ScanParamArgs,
    /// Magnitude NIfTI, the row weight of the LSQR seed as the HEIDI package's (not needed with
    /// --seed): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined
    #[arg(long, num_args = 1.., required_unless_present = "seed")]
    pub magnitude: Vec<PathBuf>,
    /// A χ map (ppm) another inversion made of this field, to fill the cone of instead of
    /// running LSQR (`--lsqr-*` and `--magnitude` then do nothing). HEIDI suits a seed with little
    /// or no smoothing (TKD and TSVD with a threshold no larger than the cone threshold, small-λ
    /// Tikhonov, NDI, RTS); a method that smooths its result also changes the parts of the data
    /// HEIDI keeps as they are, and HEIDI cannot undo that
    #[arg(long)]
    pub seed: Option<PathBuf>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<HeidiFlags>,
}

#[derive(Parser, Debug)]
pub struct InvertNdiArgs {
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
    pub settings: crate::cli_settings::SettingsOf<NdiFlags>,
}
