use clap::Parser;
use std::path::PathBuf;

/// Args shared by the multi-orientation reconstructions (COSMOS, STI).
///
/// These take N field maps rather than one, and they carry the direction table that makes the
/// reconstruction meaningful. The inputs must **already be co-registered onto one common
/// grid**: these subcommands are the bare inversions, and silently reconstructing from
/// unregistered orientations would produce a map that looks fine and is wrong. They have no
/// magnitude to register on, and a local field cannot stand in — its contrast is the dipole
/// response, which changes with orientation by construction.
///
/// The pipeline path does register: `qsmxt run --orientation-group` co-registers a group's
/// orientations on their magnitudes and recovers the B0 directions from the rotations. Use that
/// unless the fields were produced elsewhere.
#[derive(Parser, Debug)]
pub struct MultiOrientCommonArgs {
    /// Local field NIfTI in ppm — repeat once per orientation, all on one common grid
    #[arg(short = 'i', long = "input", required = true, action = clap::ArgAction::Append)]
    pub inputs: Vec<PathBuf>,
    /// B0 direction for one orientation in the common grid's voxel frame (3 values) — repeat
    /// once per --input, in the same order. Omitted, each volume's own affine is used instead,
    /// which is only valid when the orientations were separately prescribed.
    #[arg(long, num_args = 3, allow_negative_numbers = true, action = clap::ArgAction::Append)]
    pub b0_direction: Vec<f64>,
    /// Text file of B0 directions, one "x y z" per line, in --input order
    #[arg(long, conflicts_with = "b0_direction")]
    pub b0_dirs: Option<PathBuf>,
    /// Binary mask NIfTI file, on the common grid
    #[arg(short, long)]
    pub mask: PathBuf,
    /// Reconstruct even when the B0 directions look degenerate. A degenerate set yields a
    /// plausible-looking map that is not the method you asked for; prefer fixing the metadata
    #[arg(long)]
    pub force: bool,
}

#[derive(Parser, Debug)]
pub struct InvertCosmosArgs {
    #[command(flatten)]
    pub common: MultiOrientCommonArgs,
    /// Output susceptibility map NIfTI file
    #[arg(short, long)]
    pub output: PathBuf,
    /// L2 regularization on the k-space denominator. 0 is plain least squares and is right for
    /// 3+ covering orientations; 1e-3..1e-2 helps with 2, or single-axis rotations
    #[arg(long, default_value_t = qsm_core::inversion::CosmosParams::default().lambda)]
    pub lambda: f64,
    /// Magnitude NIfTI for one orientation — repeat once per --input to use the
    /// magnitude-weighted formulation (conjugate gradient) instead of the closed form
    #[arg(long = "magnitude", action = clap::ArgAction::Append)]
    pub magnitudes: Vec<PathBuf>,
    /// Conjugate-gradient tolerance (weighted formulation only)
    #[arg(long, default_value_t = qsm_core::inversion::CosmosParams::default().tol)]
    pub tol: f64,
    /// Maximum conjugate-gradient iterations (weighted formulation only)
    #[arg(long, default_value_t = qsm_core::inversion::CosmosParams::default().max_iter)]
    pub max_iter: usize,
}

#[derive(Parser, Debug)]
pub struct InvertStiArgs {
    #[command(flatten)]
    pub common: MultiOrientCommonArgs,
    /// Output prefix; writes {prefix}_desc-sti_Chimap.nii (the 4D tensor, components 11 12 13 22
    /// 23 33), {prefix}_desc-mms_Chimap.nii, {prefix}_desc-msa_Chimap.nii and
    /// {prefix}_desc-sti_evec.nii (the 4D principal eigenvector) — the pipeline's names
    #[arg(short, long)]
    pub output: PathBuf,
    /// Ridge term on the 6x6 normal equations at each k-point. Raise when orientations are
    /// few, clustered or near-coplanar
    #[arg(long, default_value_t = qsm_core::inversion::StiParams::default().lambda)]
    pub lambda: f64,
}
