use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;


// ── Chi-separation ──

crate::flag_set! {
    R2starQsmFlags = ["separation.r2star_qsm."];
    DecomposeFlags = ["separation.decompose."];
    /// The QSM is given (`--qsm`), so the settings of the iLSQR that would make it are not flags.
    ChiSepIlsqrFlags = ["separation.chi_sep_ilsqr.", "!separation.chi_sep_ilsqr.qsm_ilsqr."];
    ChiSepMediFlags = ["separation.chi_sep_medi.", "!separation.chi_sep_medi.qsm_ilsqr."];
    WavesepFlags = ["separation.wavesep."];
    HcChisepFlags = ["separation.hc_chisep."];
    SusepNetFlags = ["separation.susep_net."];
    /// With R2PRIMEnet's field-strength rule, which the toolbox route runs it under.
    ChiSepNetFlags = ["separation.chi_sepnet.", "separation.r2primenet.ignore_field_mismatch"];
}

/// Inputs shared by every chi-separation method.
#[derive(Args, Debug, Clone)]
pub struct SeparateCommonArgs {
    /// Conventional QSM (χ_total) NIfTI in ppm
    #[arg(long)]
    pub qsm: PathBuf,
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii
    /// and {prefix}_desc-total_Chimap.nii — the pipeline's names
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 field strength in Tesla
    #[arg(long, default_value_t = 3.0)]
    pub field_strength: f64,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
}

#[derive(Subcommand, Debug)]
pub enum SeparateCommand {
    /// R2*-QSM closed-form (Dimov 2022) — QSM + R2* (GRE-only)
    R2starQsm(SeparateR2starQsmArgs),
    /// DECOMPOSE-QSM signal-domain fit (Chen 2021) — a QSM per echo + multi-echo magnitude (GRE-only)
    Decompose(SeparateDecomposeArgs),
    /// χ-separation iLSQR (Shin 2021) — local field + R2' + QSM
    ChiSepIlsqr(SeparateChiSepIlsqrArgs),
    /// χ-separation MEDI — local field + R2' + multi-echo magnitude + QSM
    ChiSepMedi(SeparateChiSepMediArgs),
    /// WaveSep wavelet-L1 (Fang 2023) — QSM + R2'
    Wavesep(SeparateWavesepArgs),
    /// Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude
    HcChisep(SeparateHcChisepArgs),
    /// SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use)
    SusepNet(SeparateSusepNetArgs),
    /// χ-sepnet deep-learning separation — local field + R2' or R2* (weights downloaded on first use)
    #[command(name = "chi-sepnet")]
    ChiSepNet(SeparateChiSepNetArgs),
}

#[derive(Parser, Debug)]
pub struct SeparateSusepNetArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<SusepNetFlags>,
}

#[derive(Parser, Debug)]
#[command(group(clap::ArgGroup::new("chisepnet_r2").required(true).args(["r2prime", "r2star"])))]
pub struct SeparateChiSepNetArgs {
    /// Binary mask NIfTI file
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii
    /// and {prefix}_desc-total_Chimap.nii — the pipeline's names
    #[arg(short, long)]
    pub output: PathBuf,
    /// B0 field strength in Tesla
    #[arg(long, default_value_t = 3.0)]
    pub field_strength: f64,
    /// B0 direction (3 values)
    #[arg(long, num_args = 3, allow_negative_numbers = true, default_values_t = [0.0, 0.0, 1.0])]
    pub b0_direction: Vec<f64>,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz (χ-sepnet-R2')
    #[arg(long)]
    pub r2prime: Option<PathBuf>,
    /// R2* map NIfTI in Hz (χ-sepnet-R2*: R2' predicted by R2PRIMEnet; `--route toolbox` only)
    #[arg(long)]
    pub r2star: Option<PathBuf>,
    /// Conventional QSM (χ_total) NIfTI in ppm; `--route native` only (the toolbox route makes its
    /// own with QSMnetME)
    #[arg(long)]
    pub qsm: Option<PathBuf>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<ChiSepNetFlags>,
}

#[derive(Parser, Debug)]
#[command(group(clap::ArgGroup::new("r2star_source").required(true).args(["r2star", "magnitude"])))]
pub struct SeparateR2starQsmArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// R2* map NIfTI in Hz (else fit from --magnitude + --echo-times)
    #[arg(long)]
    pub r2star: Option<PathBuf>,
    /// Multi-echo magnitude: one 4D file or several 3D files (space-separated). Fits R2* if --r2star absent.
    #[arg(long, num_args = 1.., requires = "echo_times")]
    pub magnitude: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated), required with --magnitude
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    pub echo_times: Vec<f64>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<R2starQsmFlags>,
}

#[derive(Parser, Debug)]
pub struct SeparateDecomposeArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Multi-echo magnitude: one 4D file or several 3D files (space-separated)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// QSM of each echo (ppm), which sets each echo's phase: one 4D file or one 3D file per echo
    /// (space-separated; the same file may be given for every echo)
    #[arg(long, num_args = 1.., required = true)]
    pub qsm_per_echo: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated)
    #[arg(long, num_args = 1.., value_delimiter = ',', required = true)]
    pub echo_times: Vec<f64>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<DecomposeFlags>,
}

#[derive(Parser, Debug)]
pub struct SeparateChiSepIlsqrArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<ChiSepIlsqrFlags>,
}

/// χ-separation MEDI. Its data terms are weighted by a noise map made from every echo's
/// magnitude, and its CSF term reads a CSF mask made from R2* of the echoes (ARLO, or the
/// χ-separation toolbox's NNLS fit), so it takes the echoes' magnitudes and echo times.
#[derive(Parser, Debug)]
pub struct SeparateChiSepMediArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// Local (tissue) field NIfTI in ppm
    #[arg(long)]
    pub local_field: PathBuf,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    /// Multi-echo magnitude: one 4D file or one 3D file per echo (space-separated)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated)
    #[arg(long, num_args = 1.., value_delimiter = ',', required = true)]
    pub echo_times: Vec<f64>,
    /// CSF mask NIfTI for the CSF-uniformity term, or `none` for no CSF term (the toolbox's GUI
    /// without a CSF mask) (default: the CSF found from R2* of the echoes, as the toolbox's
    /// script does; see --csf-r2star)
    #[arg(long, value_name = "FILE|none")]
    pub csf_mask: Option<PathBuf>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<ChiSepMediFlags>,
}

#[derive(Parser, Debug)]
pub struct SeparateWavesepArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<WavesepFlags>,
}

#[derive(Parser, Debug)]
pub struct SeparateHcChisepArgs {
    #[command(flatten)]
    pub common: SeparateCommonArgs,
    /// R2' map NIfTI in Hz
    #[arg(long)]
    pub r2prime: PathBuf,
    /// Multi-echo magnitude: one 4D file or several 3D files (space-separated)
    #[arg(long, num_args = 1.., required = true)]
    pub magnitude: Vec<PathBuf>,
    /// GRE echo times in seconds (space- or comma-separated)
    #[arg(long, num_args = 1.., value_delimiter = ',', required = true)]
    pub echo_times: Vec<f64>,
    /// Multi-echo spin-echo magnitude: one 4D file or several 3D files (optional)
    #[arg(long, num_args = 1..)]
    pub se_magnitude: Vec<PathBuf>,
    /// Spin-echo times in seconds (comma-separated) for --se-magnitude
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    pub se_echo_times: Vec<f64>,
    // The settings flags (`qsmxt run`'s, from the flag table).
    #[command(flatten)]
    pub settings: crate::cli_settings::SettingsOf<HcChisepFlags>,
}
