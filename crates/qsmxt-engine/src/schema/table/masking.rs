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
        d("masking.custom_mask_tool", Masking, "Use custom masks", "BYO mask from BIDS derivatives: empty=off, * =any tool, or a tool name (e.g. bet); falls back if absent", Text).basic().optional().flag("--use-custom-masks").if_omitted("the mask is computed")
        .supplied_file(Item::SuppliedMask, "A brain mask to use instead of computing one from the mask recipe").full("Custom mask")
        .gui("Use a mask from <bids>/derivatives/<tool>/ instead of computing one: * for any tool, or a tool name (e.g. bet); empty to always compute it. Where none is found, the mask is computed")
        .cli("Prefer a bring-your-own mask from BIDS derivatives when present (else compute it). Bare flag = first matching derivatives tool (alphabetical); give a name (e.g. `bet`) to restrict to `derivatives/<tool>/sub-*/anat/*_mask.nii*`. Always falls back")
        .default_missing("*"),
        d("masking.two_pass", Masking, "Two-pass artefact reduction", "Reconstruct again on a mask with holes at strong susceptibility sources and merge the two — keeps streaking from those sources out of the rest of the brain, at roughly double the reconstruction time (doi:10.1002/mrm.29048)", Bool).basic().flag("--two-pass")
        .cli("Two-pass artefact reduction: reconstruct a second time against a mask with holes at strong susceptibility sources, and merge the two reconstructions (how: --two-pass-mode). Strong sources (haemorrhage, calcification, implants) make the phase around them unrecoverable; leaving those voxels out keeps their streaking from spreading, while the ordinary mask still supplies values inside the holes. Roughly doubles background removal and inversion time, and is not free of downsides — where the holes were not artefactual it can be worse than a single pass. Based on doi:10.1002/mrm.29048 (Stewart et al.). Off by default. The conventional single-pass map is kept alongside as `_desc-singlepass_Chimap.nii`"),
        // QSM.rs bet/evolution.rs: f = 6/(1/rmin - 1/rmax) is NaN at 0.
        d("bet.smoothness", Masking, "Smoothness", "BET surface smoothness factor (shared by every BET step)", Float).above(0.0).flag("--bet-smoothness").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET")
        .cli("BET surface smoothness factor. Applies to every BET step in the masking recipe"),
        d("bet.gradient_threshold", Masking, "Gradient threshold", "BET gradient threshold, -1 to 1 (shared by every BET step)", Float).flag("--bet-gradient-threshold").parent("masking.sections")
        .when(|x| x.uses_bet)
        .gui("BET gradient threshold, from -1 to 1 (shared by every BET step)").context("BET")
        .cli("BET gradient threshold (-1 to 1). Applies to every BET step in the masking recipe")
        .attrs(&["allow_hyphen_values"]),
        d("bet.iterations", Masking, "Iterations", "BET surface evolution iterations (shared by every BET step)", Int).flag("--bet-iterations").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET")
        .cli("BET surface evolution iterations. Applies to every BET step in the masking recipe"),
        d("bet.subdivisions", Masking, "Subdivisions", "BET icosphere subdivision level (shared by every BET step)", Int).flag("--bet-subdivisions").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET")
        .cli("BET icosphere subdivision level. Applies to every BET step in the masking recipe"),
        d("bet.orientation", Masking, "Orientation", "", Enum(vec![
            o("fsl", "fsl", "As FSL reads it", "Left-right reversed for a neurologically stored image, as FSL's bet reads a NIfTI file (default)"),
            o("as-stored", "as-stored", "As stored", "The voxels in the order they are stored, as MEDI's BET.m runs bet2"),
        ])).flag("--bet-orientation").parent("masking.sections")
        .when(|x| x.uses_bet).context("BET")
        .gui("Which way round BET sees the left-right axis (shared by every BET step)")
        .cli("Which way round BET sees the left-right axis: `fsl` (the default; reversed for a neurologically stored image, as FSL's bet reads a NIfTI file) or `as-stored` (the voxels in the order they are stored, as MEDI's BET.m runs bet2). Applies to every BET step in the masking recipe")
        .parse(P::Named).value_name("ORIENTATION"),
        d("masking.two_pass_sections", Masking, "Reliable mask", "The mask the second pass reconstructs on: its holes are left unfilled. Empty = the main mask without its hole filling", MaskSections).optional().parent("masking.two_pass")
        .when(|x| x.c.masking.two_pass),
        d("masking.two_pass_combine", Masking, "Combined with", "How the reliable mask's sections are combined: OR keeps a voxel any section keeps, AND only one every section keeps. Empty = as the main mask's sections are", Enum(vec![
            o("or", "OR", "OR", "A voxel is in the reliable mask when any section keeps it"),
            o("and", "AND", "AND", "A voxel is in the reliable mask when every section keeps it"),
        ])).optional().flag("--two-pass-mask-combine").parent("masking.two_pass_sections").context("Reliable mask")
        .when(|x| x.c.masking.two_pass && x.c.masking.resolved_two_pass_sections().len() > 1)
        .cli("How to combine multiple --two-pass-mask sections: `or` (union) or `and` (intersection). Defaults to the main mask's --mask-combine. Turns on --two-pass")
        .value_help("or", "Union — keep a voxel any section keeps (default)").value_help("and", "Intersection — keep only voxels every section keeps"),
        d("masking.two_pass_mode", Masking, "Two-pass mode", "", Enum(vec![
            o("local", "local", "Local", "Holes only where the reliable mask has a core of very low phase quality (strong sources, not veins or the pallidum); the second pass is used within 6 mm of them, blended into the first out to 12 mm (default)"),
            o("original", "original", "Original", "Every voxel outside the reliable mask is a hole, and the second pass is used wherever it is defined (the published algorithm)"),
        ])).basic().flag("--two-pass-mode").parent("masking.two_pass")
        .when(|x| x.c.masking.two_pass)
        .gui("How the second pass is made and merged: local (holes only at strong sources, second pass used near them) or original (the published algorithm)")
        .cli("How --two-pass makes and merges its second pass. `local` (default): only holes with a core of very low phase quality are kept (strong sources, not veins or the pallidum), background removal leaves no eroded rim around them, and the second pass is used within 6 mm of them, blended into the first out to 12 mm. `original`: the published algorithm (doi:10.1002/mrm.29048), every voxel outside the reliable mask is a hole and the second pass is used wherever it is defined. Turns on --two-pass")
        .value_help("local", "Holes only at strong sources; second pass used near them (default)").value_help("original", "The published algorithm"),
        d("masking.inhomogeneity_correction", Masking, "Magnitude correction", "Correct the magnitude for receive-coil inhomogeneity: the echoes masking reads, and the combined magnitude", Bool).basic().flag("--no-inhomogeneity-correction").full("Inhomogeneity correction")
        .cli("Disable inhomogeneity correction")
        .negated("--inhomogeneity-correction", "Apply inhomogeneity correction to magnitude before masking"),
        d("masking.inhomogeneity_method", Masking, "Method", "", Enum(vec![
            o("makehomogeneous", "makehomogeneous", "makehomogeneous", "Divide by a smoothed estimate of the receive sensitivity (MriResearchTools' makehomogeneous) (default)"),
            o("n4", "n4", "N4", "N4 bias field correction, as ANTs' N4BiasFieldCorrection: a B-spline bias field fitted to sharpen the intensity histogram — https://doi.org/10.1109/TMI.2010.2046908"),
        ])).basic().flag("--inhomogeneity-method").parent("masking.inhomogeneity_correction")
        .when(|x| x.c.masking.inhomogeneity_correction).full("Inhomogeneity correction method")
        .gui("How the magnitude is corrected for inhomogeneity")
        .cli("Inhomogeneity correction method: `makehomogeneous` (default; --homogeneity-*) or `n4` (N4 bias field correction, as ANTs' N4BiasFieldCorrection; --n4-*)")
        .value_help("makehomogeneous", "Divide by a smoothed estimate of the receive sensitivity (MriResearchTools' makehomogeneous)").value_help("n4", "N4 bias field correction, as ANTs' N4BiasFieldCorrection"),
        d("homogeneity.sigma_mm", Masking, "Sigma (mm)", "Smoothing sigma of the bias-field estimate, in mm", Float).units("mm").flag("--homogeneity-sigma-mm").parent("masking.inhomogeneity_method")
        .when(|x| x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::Makehomogeneous).full("Bias field sigma")
        .gui("Smoothing sigma of the bias-field estimate")
        .cli("Inhomogeneity correction smoothing sigma in mm (makehomogeneous)"),
        // QSM.rs utils/bias_correction.rs: nx.div_ceil(nbox) divides by zero at 0.
        d("homogeneity.nbox", Masking, "Segmentation boxes", "Boxes per dimension for the tissue segmentation the receive sensitivity is estimated from", Int).min(1.0).flag("--homogeneity-nbox").parent("masking.inhomogeneity_method")
        .when(|x| x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::Makehomogeneous).full("Box filter passes")
        .cli("makehomogeneous: boxes per dimension for the tissue segmentation the receive sensitivity is estimated from"),
        // N4's settings: QSM.rs utils/n4.rs asserts each range below (a shrink factor, mesh and
        // spline order it can use, at least two histogram bins); FWHM 0 divides by zero.
        d("n4.shrink_factor", Masking, "Shrink factor", "Subsample the image by this factor before fitting the bias field", Int).min(1.0).flag("--n4-shrink-factor").parent("masking.inhomogeneity_method")
        .when(n4_on).context("N4"),
        d("n4.max_iterations", Masking, "Iterations", "Maximum iterations at each fitting level, one number a level (e.g. 50 50 50 50); the B-spline mesh doubles from one level to the next", IntList).flag("--n4-max-iterations").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Iterations per level").context("N4")
        .cli("N4: maximum iterations at each fitting level, one number a level (default 50 50 50 50); the B-spline mesh doubles from one level to the next")
        .value_name("ITERATIONS"),
        d("n4.convergence_threshold", Masking, "Convergence", "Stop a level once the change in the bias field (its coefficient of variation) falls to this; 0 runs every iteration", Float).min(0.0).flag("--n4-convergence-threshold").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Convergence threshold").context("N4")
        .cli("N4: stop a level once the change in the bias field (its coefficient of variation) falls to this (default 0: every iteration runs)"),
        d("n4.mesh", Masking, "Mesh", "B-spline mesh elements per axis at the first level (x y z)", IntVec3).min(1.0).flag("--n4-mesh").parent("masking.inhomogeneity_method")
        .when(n4_on).full("B-spline mesh").context("N4"),
        d("n4.spline_order", Masking, "Spline order", "Order of the B-spline bias field: 1, 2 or 3", Int).min(1.0).max(3.0).flag("--n4-spline-order").parent("masking.inhomogeneity_method")
        .when(n4_on).full("B-spline order").context("N4")
        .cli("N4: order of the B-spline bias field, 1 to 3"),
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
        .gui("The image geometry N4 is given, which decides where its shrunk grid falls")
        .cli("N4: the image geometry it is given: `scanner` (the scan's header, as ANTs reads the acquired NIfTI; the default) or `make-nii` (the axis-aligned header MATLAB's make_nii writes, as QSMART runs N4). N4's shrunk grid falls where the header puts it")
        .parse(P::Named).value_name("GEOMETRY"),
    ]
}

/// N4 inhomogeneity correction is on: its settings apply.
fn n4_on(x: &crate::schema::view::Ctx) -> bool {
    x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::N4
}
