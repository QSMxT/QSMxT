use clap::Args;

use super::*;

// ─── Shared algorithm parameter groups (prefixed, used by RunArgs) ───

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
