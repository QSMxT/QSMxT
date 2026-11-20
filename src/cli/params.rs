use clap::Args;

use super::*;

// ─── Shared algorithm parameter groups (prefixed, used by RunArgs) ───

/// Overlap-tiling for deep-learning inversions (bounded memory). Passing `--tile-size` enables
/// tiling (the net runs patch-by-patch); omit it to run whole-volume. `--tile-halo` sets the
/// context margin (defaults to 8 voxels). Tiling is an approximation of the whole-volume network.
#[derive(Args, Debug, Default, Clone)]
pub struct TilingParamArgs {
    /// DL tiling: output core size per patch, in voxels (enables tiling)
    #[arg(long)]
    pub tile_size: Option<usize>,
    /// DL tiling: context margin per side, in voxels (default 8)
    #[arg(long)]
    pub tile_halo: Option<usize>,
}


#[derive(Args, Debug, Default, Clone)]
pub struct LsqrParamArgs {
    /// LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use
    /// qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly
    #[arg(long)]
    pub lsqr_residual_weighting: Option<f64>,
    /// Do not fit a global field offset alongside χ
    #[arg(long)]
    pub no_lsqr_global_offset: bool,
    /// LSQR convergence tolerance
    #[arg(long)]
    pub lsqr_tol: Option<f64>,
    /// LSQR max iterations
    #[arg(long)]
    pub lsqr_max_iter: Option<usize>,
}

/// HEIDI's own knobs, for `--heidi` (and `--qsm-algorithm heidi`, whose seed is an LSQR solve, so
/// `--lsqr-*` applies there as well).
#[derive(Args, Debug, Default, Clone)]
pub struct HeidiParamArgs {
    /// |D(k)| above which a coefficient is well-conditioned and kept from the seed map
    #[arg(long)]
    pub heidi_cone_threshold: Option<f64>,
    /// Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous
    #[arg(long)]
    pub heidi_gradient_threshold: Option<f64>,
    /// Do not open the homogeneity masks where the field's Laplacian is large
    #[arg(long)]
    pub no_heidi_laplacian_correction: bool,
    /// Field-Laplacian threshold (ppm per voxel²)
    #[arg(long)]
    pub heidi_laplacian_threshold: Option<f64>,
    /// TV weight kept where every direction reads as an edge (0 reproduces upstream's shipped
    /// binarised masks)
    #[arg(long)]
    pub heidi_gradient_mask_floor: Option<f64>,
    /// NESTA continuation steps on μ
    #[arg(long)]
    pub heidi_continuation_steps: Option<usize>,
    /// Accelerated-gradient iterations per continuation step
    #[arg(long)]
    pub heidi_inner_iterations: Option<usize>,
    /// Final (smallest) smoothing parameter μ
    #[arg(long)]
    pub heidi_mu_min: Option<f64>,
    /// Relative objective change that ends an inner loop early (the default runs every
    /// iteration)
    #[arg(long)]
    pub heidi_tol: Option<f64>,
    /// Skip the anisotropic-diffusion smoothing of the field before its gradients are taken
    #[arg(long)]
    pub no_heidi_denoise: bool,
    /// Anisotropic-diffusion iterations
    #[arg(long)]
    pub heidi_denoise_iterations: Option<usize>,
    /// Anisotropic-diffusion time step
    #[arg(long)]
    pub heidi_denoise_time_step: Option<f64>,
    /// Anisotropic-diffusion conductance
    #[arg(long)]
    pub heidi_denoise_conductance: Option<f64>,
    /// ITK 3.20's float arithmetic for the denoising, as the authors' code runs it (for comparison with it; the default computes its conductance scale in double precision)
    #[arg(long, conflicts_with = "no_heidi_denoise_reference_itk")]
    pub heidi_denoise_reference_itk: bool,
    /// The denoising in double precision, after a pipeline that turns ITK's float arithmetic on
    /// (the HEIDI pipeline)
    #[arg(long)]
    pub no_heidi_denoise_reference_itk: bool,
    /// HEIDI: the echo time the field stands for the phase of, in the conversion to HEIDI's
    /// units: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms
    /// (default: last)
    #[arg(long, value_name = "TE", value_parser = parse_echo_time)]
    pub heidi_te: Option<qsmxt_engine::EchoTimeChoice>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct RomeoParamArgs {
    /// ROMEO: disable phase coherence weights (default: on)
    #[arg(long)]
    pub no_romeo_phase_coherence: bool,
    /// ROMEO: disable phase gradient coherence weights (default: on)
    #[arg(long)]
    pub no_romeo_phase_gradient_coherence: bool,
    /// ROMEO: disable phase linearity weights (default: on; the romeo command line's romeo3 weights)
    #[arg(long)]
    pub no_romeo_phase_linearity: bool,
    /// ROMEO: enable phase linearity weights (default: on; for a --pipeline that turns them off)
    #[arg(long, conflicts_with = "no_romeo_phase_linearity")]
    pub romeo_phase_linearity: bool,
    /// ROMEO: disable magnitude coherence weights (default: on)
    #[arg(long)]
    pub no_romeo_mag_coherence: bool,
    /// ROMEO: enable magnitude weighting (default: off)
    #[arg(long)]
    pub romeo_mag_weight: bool,
    /// ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility)
    #[arg(long)]
    pub no_romeo_mag_weight: bool,
    /// ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off)
    #[arg(long)]
    pub romeo_mag_weight2: bool,
    /// ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off)
    #[arg(long)]
    pub romeo_bestpath: bool,
    /// ROMEO: template echo index (1-indexed, only for template mode)
    #[arg(long)]
    pub romeo_template: Option<usize>,
    /// ROMEO: unwrap each echo on its own (individual mode, for motion or time series)
    #[arg(long)]
    pub romeo_individual: bool,
    /// ROMEO: unwrap the template echo spatially and the others temporally from it (default)
    #[arg(long)]
    pub no_romeo_individual: bool,
    /// ROMEO: correct global 2π offsets, of each echo and between echoes (default: off)
    #[arg(long)]
    pub romeo_correct_global: bool,
    /// ROMEO: do not correct global 2π offsets (default)
    #[arg(long)]
    pub no_romeo_correct_global: bool,
    /// ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0)
    #[arg(long)]
    pub romeo_temporal_uncertain_unwrapping: Option<f64>,
    /// ROMEO: maximum number of seed regions (default: 1)
    #[arg(long)]
    pub romeo_max_seeds: Option<u8>,
    /// ROMEO: merge neighboring regions after unwrapping (default: off)
    #[arg(long)]
    pub romeo_merge_regions: bool,
    /// ROMEO: correct each region's median to nearest 0 (default: off)
    #[arg(long)]
    pub romeo_correct_regions: bool,
    /// ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0)
    #[arg(long)]
    pub romeo_wrap_addition: Option<f64>,
}

impl RomeoParamArgs {
    /// Apply these flags onto a ROMEO configuration. The one place they are read, so `qsmxt run`
    /// and `qsmxt fieldmap romeo` / `unwrap romeo` cannot disagree about what a flag means.
    /// `--romeo-template` is 1-indexed on the CLI and 0-indexed in the library.
    pub fn apply_to(&self, r: &mut qsmxt_engine::RomeoConfig) {
        // Weight component flags (default-true → disabled via --no-*)
        if self.no_romeo_phase_coherence { r.phase_coherence = false; }
        if self.no_romeo_phase_gradient_coherence { r.phase_gradient_coherence = false; }
        if self.no_romeo_phase_linearity { r.phase_linearity = false; }
        if self.romeo_phase_linearity { r.phase_linearity = true; }
        if self.no_romeo_mag_coherence { r.mag_coherence = false; }
        // Weight component flags (default-false → enabled via positive flag)
        if self.romeo_mag_weight { r.mag_weight = true; }
        if self.no_romeo_mag_weight { r.mag_weight = false; }
        if self.romeo_mag_weight2 { r.mag_weight2 = true; }
        if self.romeo_bestpath { r.bestpath = true; }
        // Multi-echo options
        if let Some(t) = self.romeo_template { r.template = t.saturating_sub(1); }
        if self.romeo_individual { r.individual = true; }
        if self.no_romeo_individual { r.individual = false; }
        if self.romeo_correct_global { r.correct_global = true; }
        if self.no_romeo_correct_global { r.correct_global = false; }
        if let Some(v) = self.romeo_temporal_uncertain_unwrapping { r.temporal_uncertain_unwrapping = v; }
        if let Some(v) = self.romeo_max_seeds { r.max_seeds = v; }
        if self.romeo_merge_regions { r.merge_regions = true; }
        if self.romeo_correct_regions { r.correct_regions = true; }
        if let Some(v) = self.romeo_wrap_addition { r.wrap_addition = v; }
    }

    /// `qsm_core::unwrap::RomeoParams` for these flags, starting from QSMxT's ROMEO defaults —
    /// the ones `qsmxt run` uses (no re-unwrapping of uncertain voxels), not the library's.
    pub fn to_romeo_params(&self) -> qsm_core::unwrap::RomeoParams {
        let mut r = qsmxt_engine::RomeoConfig::default();
        self.apply_to(&mut r);
        (&r).into()
    }

    /// The flags that only mean something when several echoes are unwrapped together.
    pub fn multi_echo_flags(&self) -> Vec<&'static str> {
        [
            ("--romeo-template", self.romeo_template.is_some()),
            ("--romeo-individual", self.romeo_individual),
            ("--no-romeo-individual", self.no_romeo_individual),
            ("--romeo-correct-global", self.romeo_correct_global),
            ("--no-romeo-correct-global", self.no_romeo_correct_global),
            ("--romeo-temporal-uncertain-unwrapping", self.romeo_temporal_uncertain_unwrapping.is_some()),
            // Phase gradient coherence compares two echoes; with one it is never computed.
            ("--no-romeo-phase-gradient-coherence", self.no_romeo_phase_gradient_coherence),
        ].iter().filter(|(_, set)| *set).map(|(f, _)| *f).collect()
    }
}

/// PRELUDE's settings (`--unwrapping-algorithm prelude`), shared by `qsmxt run`, `qsmxt unwrap
/// prelude` and `qsmxt fieldmap prelude`.
#[derive(Args, Debug, Default, Clone)]
pub struct PreludeParamArgs {
    /// PRELUDE: how the initial regions are labelled (default: auto)
    #[arg(long, value_enum)]
    pub prelude_labelling: Option<PreludeLabellingArg>,
    /// PRELUDE: number of phase partitions the initial regions are formed from (default: 8)
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u64).range(1..))]
    pub prelude_partitions: Option<u64>,
}

impl PreludeParamArgs {
    /// Apply these flags onto a PRELUDE configuration: the one place they are read.
    pub fn apply_to(&self, p: &mut qsmxt_engine::PreludeConfig) {
        if let Some(l) = self.prelude_labelling { p.labelling = l.into(); }
        if let Some(n) = self.prelude_partitions { p.num_phase_partitions = n as usize; }
    }

    /// `qsm_core::unwrap::PreludeParams` for these flags, from the defaults.
    pub fn to_prelude_params(&self) -> qsm_core::unwrap::PreludeParams {
        let mut p = qsmxt_engine::PreludeConfig::default();
        self.apply_to(&mut p);
        (&p).into()
    }
}

/// N4's settings (`--inhomogeneity-method n4`), shared by `qsmxt run` and `qsmxt homogeneity`.
#[derive(Args, Debug, Default, Clone)]
pub struct N4ParamArgs {
    /// N4: subsample the image by this factor before fitting the bias field (default 4)
    #[arg(long)]
    pub n4_shrink_factor: Option<usize>,
    /// N4: maximum iterations at each fitting level, one number a level (default 50 50 50 50);
    /// the B-spline mesh doubles from one level to the next
    #[arg(long, num_args = 1.., value_name = "ITERATIONS")]
    pub n4_max_iterations: Option<Vec<usize>>,
    /// N4: stop a level once the change in the bias field (its coefficient of variation) falls
    /// to this (default 0: every iteration runs)
    #[arg(long)]
    pub n4_convergence_threshold: Option<f64>,
    /// N4: B-spline mesh elements per axis at the first level, X Y Z (default 1 1 1)
    #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"])]
    pub n4_mesh: Option<Vec<usize>>,
    /// N4: order of the B-spline bias field, 1 to 3 (default 3)
    #[arg(long)]
    pub n4_spline_order: Option<usize>,
    /// N4: FWHM of the Gaussian modelling the bias field's blurring of the log-intensity
    /// histogram (default 0.15)
    #[arg(long)]
    pub n4_bias_field_fwhm: Option<f64>,
    /// N4: noise term of the Wiener deconvolution of the histogram (default 0.01)
    #[arg(long)]
    pub n4_wiener_noise: Option<f64>,
    /// N4: bins of the log-intensity histogram (default 200)
    #[arg(long)]
    pub n4_histogram_bins: Option<usize>,
    /// N4: the image geometry it is given: `scanner` (the scan's header, as ANTs reads the
    /// acquired NIfTI; the default) or `make-nii` (the axis-aligned header MATLAB's make_nii
    /// writes, as QSMART runs N4). N4's shrunk grid falls where the header puts it
    #[arg(long, value_name = "GEOMETRY", value_parser = parse_n4_geometry)]
    pub n4_geometry: Option<qsmxt_engine::N4Geometry>,
}

impl N4ParamArgs {
    /// Apply these flags onto an N4 configuration: the one place they are read, so `qsmxt run`
    /// and `qsmxt homogeneity` cannot disagree about what a flag means.
    pub fn apply_to(&self, n: &mut qsmxt_engine::N4Config) {
        if let Some(v) = self.n4_shrink_factor { n.shrink_factor = v; }
        if let Some(v) = &self.n4_max_iterations { n.max_iterations = v.clone(); }
        if let Some(v) = self.n4_convergence_threshold { n.convergence_threshold = v; }
        if let Some(v) = &self.n4_mesh {
            if let [x, y, z] = v[..] { n.mesh = [x, y, z]; }
        }
        if let Some(v) = self.n4_spline_order { n.spline_order = v; }
        if let Some(v) = self.n4_bias_field_fwhm { n.bias_field_fwhm = v; }
        if let Some(v) = self.n4_wiener_noise { n.wiener_noise = v; }
        if let Some(v) = self.n4_histogram_bins { n.histogram_bins = v; }
    }
}

#[derive(Args, Debug, Default, Clone)]
pub struct SegmentationParamArgs {
    /// SynthSeg weights generation: v1 (SynthSeg 1.0, 32 labels; the default) or v2 (SynthSeg 2.0,
    /// 33 labels, adds a general CSF class; Billot et al., PNAS 2023)
    #[arg(long, value_name = "VERSION")]
    pub synthseg_version: Option<String>,
    /// Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32).
    /// Omit to process the whole volume, as the SynthSeg CLI does
    #[arg(long)]
    pub synthseg_crop: Option<usize>,
    /// Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at
    /// 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the
    /// volume. Omit to run the whole volume at once, as the SynthSeg CLI does
    #[arg(long, value_name = "VOXELS")]
    pub synthseg_patch: Option<usize>,
    /// Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does
    #[arg(long)]
    pub no_synthseg_flip_averaging: bool,
    /// Skip resetting each topological class to its largest connected component
    #[arg(long)]
    pub no_synthseg_topology_cleanup: bool,
    /// Gaussian blur on the posteriors, in voxels (0 disables it)
    #[arg(long)]
    pub synthseg_sigma: Option<f64>,
    /// Prefer a bring-your-own segmentation from <bids>/derivatives/<TOOL>/ when present.
    /// Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg
    #[arg(long, num_args = 0..=1, default_missing_value = "*", value_name = "TOOL")]
    pub use_custom_dseg: Option<String>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct SwiParamArgs {
    /// SWI phase unwrapping before the echoes are combined [default: laplacian]
    #[arg(long, value_enum)]
    pub swi_unwrapping: Option<SwiUnwrapArg>,
    /// SWI high-pass filter sigma (3 values, in voxels)
    #[arg(long, num_args = 3)]
    pub swi_hp_sigma: Option<Vec<f64>>,
    /// SWI phase-mask scaling
    #[arg(long, value_enum)]
    pub swi_scaling: Option<SwiScalingArg>,
    /// SWI phase scaling strength
    #[arg(long)]
    pub swi_strength: Option<f64>,
    /// SWI MIP window size in slices
    #[arg(long)]
    pub swi_mip_window: Option<usize>,
}
