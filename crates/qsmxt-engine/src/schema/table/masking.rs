//! The Masking section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("masking.sections", Masking, "Mask", "The mask recipe: one or more sections, each an input image, a generator and its refinements, combined with OR or AND", MaskSections).basic(),
        d("masking.combine", Masking, "Combined with", "How the sections are combined: OR keeps a voxel any section keeps, AND only one every section keeps", Enum(vec![
            o("or", "OR", "OR", "A voxel is in the mask when any section keeps it"),
            o("and", "AND", "AND", "A voxel is in the mask when every section keeps it"),
        ])).parent("masking.sections")
        .when(|x| x.c.masking.sections.len() > 1),
        d("masking.refinements", Masking, "Combined mask steps", "Refinement steps run on the combined mask, after the sections are combined", MaskOps).parent("masking.sections")
        .when(|x| x.c.masking.sections.len() > 1),
        d("masking.custom_mask_tool", Masking, "Use custom masks", "BYO mask from BIDS derivatives: empty=off, * =any tool, or a tool name (e.g. bet); falls back if absent", Text).basic().optional().flag("--use-custom-masks")
        .supplied_file(Item::SuppliedMask, "A brain mask to use instead of computing one from the mask recipe").full("Custom mask")
        .gui("Use a mask from <bids>/derivatives/<tool>/ instead of computing one: * for any tool, or a tool name (e.g. bet); empty to always compute it. Where none is found, the mask is computed"),
        d("masking.two_pass", Masking, "Two-pass artefact reduction", "Reconstruct again on a mask with holes at strong susceptibility sources and merge the two — keeps streaking from those sources out of the rest of the brain, at roughly double the reconstruction time (doi:10.1002/mrm.29048)", Bool).basic().flag("--two-pass"),
        // QSM.rs bet/evolution.rs: f = 6/(1/rmin - 1/rmax) is NaN at 0.
        d("bet.smoothness", Masking, "Smoothness", "BET surface smoothness factor (shared by every BET step)", Float).above(0.0).flag("--bet-smoothness").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET"),
        d("bet.gradient_threshold", Masking, "Gradient threshold", "BET gradient threshold, -1 to 1 (shared by every BET step)", Float).flag("--bet-gradient-threshold").parent("masking.sections")
        .when(|x| x.uses_bet)
        .gui("BET gradient threshold, from -1 to 1 (shared by every BET step)").context("BET"),
        d("bet.iterations", Masking, "Iterations", "BET surface evolution iterations (shared by every BET step)", Int).flag("--bet-iterations").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET"),
        d("bet.subdivisions", Masking, "Subdivisions", "BET icosphere subdivision level (shared by every BET step)", Int).flag("--bet-subdivisions").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET"),
        d("bet.orientation", Masking, "Orientation", "", Enum(vec![
            o("fsl", "fsl", "As FSL reads it", "Left-right reversed for a neurologically stored image, as FSL's bet reads a NIfTI file (default)"),
            o("as-stored", "as-stored", "As stored", "The voxels in the order they are stored, as MEDI's BET.m runs bet2"),
        ])).flag("--bet-orientation").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET")
        .gui("Which way round BET sees the left-right axis (shared by every BET step)"),
        d("masking.two_pass_sections", Masking, "Reliable mask", "The mask the second pass reconstructs on: its holes are left unfilled. Empty = the main mask without its hole filling", MaskSections).optional().parent("masking.two_pass")
        .when(|x| x.c.masking.two_pass),
        d("masking.two_pass_combine", Masking, "Combined with", "How the reliable mask's sections are combined: OR keeps a voxel any section keeps, AND only one every section keeps. Empty = as the main mask's sections are", Enum(vec![
            o("or", "OR", "OR", "A voxel is in the reliable mask when any section keeps it"),
            o("and", "AND", "AND", "A voxel is in the reliable mask when every section keeps it"),
        ])).optional().flag("--two-pass-mask-combine").parent("masking.two_pass_sections").context("Reliable mask")
        .when(|x| x.c.masking.two_pass && x.c.masking.resolved_two_pass_sections().len() > 1),
        d("masking.two_pass_mode", Masking, "Two-pass mode", "", Enum(vec![
            o("local", "local", "Local", "Holes only where the reliable mask has a core of very low phase quality (strong sources, not veins or the pallidum); the second pass is used within 6 mm of them, blended into the first out to 12 mm (default)"),
            o("original", "original", "Original", "Every voxel outside the reliable mask is a hole, and the second pass is used wherever it is defined (the published algorithm)"),
        ])).basic().flag("--two-pass-mode").parent("masking.two_pass")
        .when(|x| x.c.masking.two_pass)
        .gui("How the second pass is made and merged: local (holes only at strong sources, second pass used near them) or original (the published algorithm)"),
        d("masking.inhomogeneity_correction", Masking, "Magnitude correction", "Correct the magnitude for receive-coil inhomogeneity: the echoes masking reads, and the combined magnitude", Bool).basic().flag("--no-inhomogeneity-correction").full("Inhomogeneity correction"),
        d("masking.inhomogeneity_method", Masking, "Method", "", Enum(vec![
            o("makehomogeneous", "makehomogeneous", "makehomogeneous", "Divide by a smoothed estimate of the receive sensitivity (MriResearchTools' makehomogeneous) (default)"),
            o("n4", "n4", "N4", "N4 bias field correction, as ANTs' N4BiasFieldCorrection: a B-spline bias field fitted to sharpen the intensity histogram — https://doi.org/10.1109/TMI.2010.2046908"),
        ])).basic().flag("--inhomogeneity-method").parent("masking.inhomogeneity_correction")
        .when(|x| x.c.masking.inhomogeneity_correction).full("Inhomogeneity correction method")
        .gui("How the magnitude is corrected for inhomogeneity"),
        d("homogeneity.sigma_mm", Masking, "Sigma (mm)", "Smoothing sigma of the bias-field estimate, in mm", Float).units("mm").flag("--homogeneity-sigma-mm").parent("masking.inhomogeneity_method")
        .when(|x| x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::Makehomogeneous).full("Bias field sigma")
        .gui("Smoothing sigma of the bias-field estimate"),
        // QSM.rs utils/bias_correction.rs: nx.div_ceil(nbox) divides by zero at 0.
        d("homogeneity.nbox", Masking, "Segmentation boxes", "Boxes per dimension for the tissue segmentation the receive sensitivity is estimated from", Int).min(1.0).flag("--homogeneity-nbox").parent("masking.inhomogeneity_method")
        .when(|x| x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::Makehomogeneous).full("Box filter passes"),
        // N4's settings: QSM.rs utils/n4.rs asserts each range below (a shrink factor, mesh and
        // spline order it can use, at least two histogram bins); FWHM 0 divides by zero.
        d("n4.shrink_factor", Masking, "Shrink factor", "Subsample the image by this factor before fitting the bias field", Int).min(1.0).flag("--n4-shrink-factor").parent("masking.inhomogeneity_method")
        .when(n4_on).context("N4"),
        d("n4.max_iterations", Masking, "Iterations", "Maximum iterations at each fitting level, one number a level (e.g. 50 50 50 50); the B-spline mesh doubles from one level to the next", IntList).flag("--n4-max-iterations").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Iterations per level").context("N4"),
        d("n4.convergence_threshold", Masking, "Convergence", "Stop a level once the change in the bias field (its coefficient of variation) falls to this; 0 runs every iteration", Float).min(0.0).flag("--n4-convergence-threshold").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Convergence threshold").context("N4"),
        d("n4.mesh", Masking, "Mesh", "B-spline mesh elements per axis at the first level (x y z)", IntVec3).min(1.0).flag("--n4-mesh").parent("masking.inhomogeneity_method")
        .when(n4_on).full("B-spline mesh").context("N4"),
        d("n4.spline_order", Masking, "Spline order", "Order of the B-spline bias field: 1, 2 or 3", Int).min(1.0).max(3.0).flag("--n4-spline-order").parent("masking.inhomogeneity_method")
        .when(n4_on).full("B-spline order").context("N4"),
        d("n4.bias_field_fwhm", Masking, "Bias FWHM", "FWHM of the Gaussian modelling the bias field's blurring of the log-intensity histogram", Float).above(0.0).flag("--n4-bias-field-fwhm").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Bias field FWHM").context("N4"),
        d("n4.wiener_noise", Masking, "Wiener noise", "Noise term of the Wiener deconvolution of the histogram", Float).min(0.0).flag("--n4-wiener-noise").parent("masking.inhomogeneity_method")
        .when(n4_on).context("N4"),
        d("n4.histogram_bins", Masking, "Histogram bins", "Bins of the log-intensity histogram", Int).min(2.0).flag("--n4-histogram-bins").parent("masking.inhomogeneity_method")
        .when(n4_on).context("N4"),
        d("masking.n4_geometry", Masking, "Geometry", "", Enum(vec![
            o("scanner", "scanner", "Scanner", "The scan's header, as ANTs reads the acquired NIfTI (default)"),
            o("make-nii", "make-nii", "make_nii", "The axis-aligned header MATLAB's make_nii writes, as QSMART runs N4"),
        ])).flag("--n4-geometry").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Image geometry").context("N4")
        .gui("The image geometry N4 is given, which decides where its shrunk grid falls"),
    ]
}

/// N4 inhomogeneity correction is on: its settings apply.
fn n4_on(x: &crate::schema::view::Ctx) -> bool {
    x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::N4
}
