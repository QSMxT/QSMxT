//! The settings table: one entry per user-facing [`PipelineConfig`](crate::PipelineConfig) setting,
//! in the TUI's order, with its labels and help.
//!
//! An entry's label and help (and `.value(..)`) are the TUI's own wording (`src/tui/app.rs`); the
//! TUI's tests check that the two agree, so edit both together. `.full(..)`, `.gui(..)` and
//! `.gui_value(..)` give the wording for other front ends where the TUI's is abbreviated, names
//! the default, repeats the units, or is missing.

use serde_json::{json, Value};

use super::{d, o, Def, Section::*, Ty::*};
use crate::enums::*;
use crate::store::Item;
use qsm_core::utils::wavelet::SUPPORTED_ORDERS as WAVELET_ORDERS;

pub(super) fn table() -> Vec<Def> {
    vec![
        d("multi_orientation.group_by", Input, "Orientations", "Runs that are the same object at a different angle: an entity (acq, run, rec, inv), a glob (*acq-dir*), or re:<regex>. Empty = off. How they combine: QSM → Multi-orientation", Text).basic().off(json!("")).flag("--orientation-group").full("Orientation grouping")
        .gui("Runs that are the same object at a different angle, matched by an entity (acq, run, rec, inv), a glob (*acq-dir*) or re:<regex>; empty turns grouping off. How the orientations are combined is set under Multi-orientation").unavailable_in_browser(),
        d("pipeline.axial_resampling", Geometry, "Axial resampling", "Resample oblique acquisitions onto an axial grid when they are tilted more than the threshold. Off: reconstruct on the acquired grid with the true B0 direction", Bool).basic().flag("--axial-resampling"),
        // QSMxT: --obliquity-threshold and the config file refuse a negative threshold (src/pipeline/config.rs, PipelineConfig::from_toml_migrating).
        d("pipeline.obliquity_threshold", Geometry, "Obliquity threshold (°)", "Resample only data more oblique than this, in degrees", Float).min(0.0).units("deg").flag("--obliquity-threshold").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling).full("Obliquity threshold")
        .gui("Resample only data more oblique than this"),
        d("pipeline.output_space", Geometry, "Output space", "", Enum(vec![
            o("acquired", "acquired", "Acquired grid", "Write derivatives back on the acquired grid when the run was resampled to axial (default)"),
            o("working", "working", "Working grid", "Leave derivatives on the resampled (axial) working grid, avoiding a second interpolation"),
        ])).flag("--output-space").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling || x.axial_forced)
        .gui("The grid derivatives are written on, when an oblique run was resampled to axial"),
        d("masking.inhomogeneity_correction", Geometry, "Inhomog. correction", "Apply B1 field correction to magnitude (improves masking, ROMEO weights, SWI)", Bool).basic().flag("--no-inhomogeneity-correction").full("Inhomogeneity correction"),
        d("masking.inhomogeneity_method", Geometry, "Method", "", Enum(vec![
            o("makehomogeneous", "makehomogeneous", "makehomogeneous", "Divide by a smoothed estimate of the receive sensitivity (MriResearchTools' makehomogeneous) (default)"),
            o("n4", "n4", "N4", "N4 bias field correction, as ANTs' N4BiasFieldCorrection: a B-spline bias field fitted to sharpen the intensity histogram — https://doi.org/10.1109/TMI.2010.2046908"),
        ])).flag("--inhomogeneity-method").parent("masking.inhomogeneity_correction")
        .when(|x| x.c.masking.inhomogeneity_correction).full("Inhomogeneity correction method")
        .gui("How the magnitude is corrected for inhomogeneity"),
        d("homogeneity.sigma_mm", Geometry, "Sigma (mm)", "Smoothing sigma of the bias-field estimate, in mm", Float).units("mm").flag("--homogeneity-sigma-mm").parent("masking.inhomogeneity_method")
        .when(|x| x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::Makehomogeneous).full("Bias field sigma")
        .gui("Smoothing sigma of the bias-field estimate"),
        // QSM.rs utils/bias_correction.rs: nx.div_ceil(nbox) divides by zero at 0.
        d("homogeneity.nbox", Geometry, "Box passes", "Box-filter passes approximating the Gaussian", Int).min(1.0).flag("--homogeneity-nbox").parent("masking.inhomogeneity_method")
        .when(|x| x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::Makehomogeneous).full("Box filter passes"),
        // N4's settings: QSM.rs utils/n4.rs asserts each range below (a shrink factor, mesh and
        // spline order it can use, at least two histogram bins); FWHM 0 divides by zero.
        d("n4.shrink_factor", Geometry, "Shrink factor", "Subsample the image by this factor before fitting the bias field", Int).min(1.0).flag("--n4-shrink-factor").parent("masking.inhomogeneity_method")
        .when(n4_on).context("N4"),
        d("n4.max_iterations", Geometry, "Iterations", "Maximum iterations at each fitting level, one number a level (e.g. 50 50 50 50); the B-spline mesh doubles from one level to the next", IntList).flag("--n4-max-iterations").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Iterations per level").context("N4"),
        d("n4.convergence_threshold", Geometry, "Convergence", "Stop a level once the change in the bias field (its coefficient of variation) falls to this; 0 runs every iteration", Float).min(0.0).flag("--n4-convergence-threshold").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Convergence threshold").context("N4"),
        d("n4.mesh", Geometry, "Mesh", "B-spline mesh elements per axis at the first level (x y z)", IntVec3).min(1.0).flag("--n4-mesh").parent("masking.inhomogeneity_method")
        .when(n4_on).full("B-spline mesh").context("N4"),
        d("n4.spline_order", Geometry, "Spline order", "Order of the B-spline bias field: 1, 2 or 3", Int).min(1.0).max(3.0).flag("--n4-spline-order").parent("masking.inhomogeneity_method")
        .when(n4_on).full("B-spline order").context("N4"),
        d("n4.bias_field_fwhm", Geometry, "Bias FWHM", "FWHM of the Gaussian modelling the bias field's blurring of the log-intensity histogram", Float).above(0.0).flag("--n4-bias-field-fwhm").parent("masking.inhomogeneity_method")
        .when(n4_on).full("Bias field FWHM").context("N4"),
        d("n4.wiener_noise", Geometry, "Wiener noise", "Noise term of the Wiener deconvolution of the histogram", Float).min(0.0).flag("--n4-wiener-noise").parent("masking.inhomogeneity_method")
        .when(n4_on).context("N4"),
        d("n4.histogram_bins", Geometry, "Histogram bins", "Bins of the log-intensity histogram", Int).min(2.0).flag("--n4-histogram-bins").parent("masking.inhomogeneity_method")
        .when(n4_on).context("N4"),
        d("pipeline.ignore_slice_gap", Geometry, "Ignore slice gap", "Reconstruct even if the sidecar's SliceThickness is thinner than the slice pitch (gapped 2D data gives a wrong map)", Bool).flag("--ignore-slice-gap"),
        d("pipeline.crop_to_mask", Geometry, "Crop to mask", "Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid). Off by default", Bool).flag("--crop-to-mask")
        .when(|x| x.qsm)
        .gui("Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid)"),
        d("pipeline.crop_margin_mm", Geometry, "Crop margin (mm)", "Margin around the mask; below ~32 mm results start to differ, and it must clear the largest SMV kernel", Float).units("mm").flag("--crop-margin-mm").parent("pipeline.crop_to_mask")
        .when(|x| x.qsm && x.c.pipeline.crop_to_mask).full("Crop margin")
        .gui("Margin around the mask; below about 32 mm results start to differ, and it must clear the largest SMV kernel"),
        d("pipeline.fft_padding", Geometry, "FFT padding", "Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly). Off by default", Bool).flag("--fft-padding")
        .when(|x| x.qsm)
        .gui("Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly)"),
        d("field_mapping.coil_combination_sigma", Geometry, "Coil comb. sigma", "MCPC-3D-S smoothing of per-coil phase offsets, voxels (x y z); only for uncombined multi-coil data", FloatVec3).units("voxels").flag("--coil-combination-sigma")
        .when(|x| x.multi_coil).full("Coil combination sigma")
        .gui("MCPC-3D-S smoothing of the per-coil phase offsets (x y z); only for uncombined multi-coil data"),
        d("masking.sections", Masking, "Mask", "The mask recipe: one or more sections, each an input image, a generator and its refinements, combined with OR or AND", MaskSections).basic(),
        d("masking.combine", Masking, "Combined with", "How the sections are combined: OR keeps a voxel any section keeps, AND only one every section keeps", Enum(vec![
            o("or", "OR", "OR", "A voxel is in the mask when any section keeps it"),
            o("and", "AND", "AND", "A voxel is in the mask when every section keeps it"),
        ])).parent("masking.sections")
        .when(|x| x.c.masking.sections.len() > 1),
        d("masking.refinements", Masking, "Combined mask steps", "Refinement steps run on the combined mask, after the sections are combined", MaskOps).parent("masking.sections")
        .when(|x| x.c.masking.sections.len() > 1),
        d("masking.custom_mask_tool", Masking, "Use custom masks", "BYO mask from BIDS derivatives: empty=off, * =any tool, or a tool name (e.g. bet); falls back if absent", Text).optional().flag("--use-custom-masks")
        .supplied_file(Item::SuppliedMask, "A brain mask to use instead of computing one from the mask recipe").full("Custom mask")
        .gui("Use a mask from <bids>/derivatives/<tool>/ instead of computing one: * for any tool, or a tool name (e.g. bet); empty to always compute it. Where none is found, the mask is computed"),
        d("masking.two_pass", Masking, "Two-pass artefact reduction", "Reconstruct again on a mask whose holes are left unfilled and keep that result where it is defined — keeps streaking from strong sources out of the rest of the brain, at roughly double the reconstruction time (doi:10.1002/mrm.29048)", Bool).basic().flag("--two-pass"),
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
        d("masking.two_pass_sections", Masking, "Reliable mask", "The mask the second pass reconstructs on: its holes are left unfilled. Empty = the main mask without its hole filling", MaskSections).optional().parent("masking.two_pass")
        .when(|x| x.c.masking.two_pass),
        d("motion_correction.enabled", Motion, "Motion correction", "Rigidly co-register every echo onto a reference echo before any other stage (multi-echo, needs magnitude). Off by default", Bool).basic().flag("--motion-correction")
        .gui("Rigidly co-register every echo onto a reference echo before any other stage (multi-echo, needs magnitude)"),
        d("motion_correction.reference", Motion, "Reference", "", EchoReference(vec![
            o("first", "first", "First echo", "Align every echo onto the first (best SNR; the published procedure) (default)"),
            o("last", "last", "Last echo", "Align every echo onto the last"),
            o("1", "echo", "Echo number", "Align every echo onto a given echo (Reference echo)"),
        ]))
        .value("Reference echo", "1-based echo number the others are aligned onto").flag("--motion-reference").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled).full("Reference echo")
        .gui("The echo every other echo is aligned onto").gui_value("Echo number", "The echo the others are aligned onto, counted from 1"),
        d("motion_correction.chain", Motion, "Chain", "Seed each echo's registration from its neighbour's solution", Bool).flag("--no-motion-chain").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled).full("Chain registrations"),
        d("motion_correction.chain_levels", Motion, "Chain levels", "Pyramid levels for a seeded registration (default 3)", Int).flag("--motion-chain-levels").parent("motion_correction.chain")
        .when(|x| x.c.motion_correction.enabled && x.c.motion_correction.chain)
        .gui("Pyramid levels for a seeded registration"),
        d("motion_correction.min_ncc", Motion, "Min NCC", "NCC below which a registration is reported as suspect (0.3 for echoes; ~0.9 for repeats)", Float).flag("--motion-min-ncc").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled).full("Minimum NCC")
        .gui("Normalised cross-correlation below which a registration is reported as suspect (0.3 for echoes; about 0.9 for repeats)"),
        d("registration.levels", Motion, "Levels", "Pyramid levels: the capture range; do not lower (default 5)", Int).flag("--registration-levels").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Pyramid levels")
        .gui("Pyramid levels: the capture range; do not lower").context("Registration"),
        d("registration.max_samples", Motion, "Max samples", "Voxels sampled per metric evaluation, per level", Int).flag("--registration-max-samples").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Maximum samples").context("Registration"),
        d("registration.max_iterations", Motion, "Max iterations", "Powell cycles per level", Int).flag("--registration-max-iterations").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Maximum iterations").context("Registration"),
        d("registration.max_evaluations", Motion, "Max evaluations", "Metric evaluations across all levels (hard stop)", Int).flag("--registration-max-evaluations").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Maximum evaluations").context("Registration"),
        d("registration.tolerance", Motion, "Tolerance", "Relative cost improvement per cycle that counts as converged", Float).flag("--registration-tolerance").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).context("Registration"),
        d("registration.rotation_step_deg", Motion, "Rotation step (deg)", "Initial rotation step at the finest level", Float).units("deg").flag("--registration-rotation-step-deg").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Rotation step").context("Registration"),
        d("registration.translation_step_mm", Motion, "Translation step (mm)", "Initial translation step at the finest level", Float).units("mm").flag("--registration-translation-step-mm").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Translation step").context("Registration"),
        d("registration.min_overlap", Motion, "Min overlap", "Smallest fraction of sampled voxels that must overlap", Float).flag("--registration-min-overlap").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Minimum overlap").context("Registration"),
        d("field_mapping.unwrapping_algorithm", FieldMap, "Unwrapping", "", Enum(vec![
            o("romeo", "romeo", "ROMEO", "ROMEO region-growing unwrapping — https://doi.org/10.1002/mrm.28563"),
            o("laplacian", "laplacian", "Laplacian", "Laplacian phase unwrapping (FFT-based) — https://doi.org/10.1364/OL.28.001194"),
        ])).basic().flag("--unwrapping-algorithm").full("Phase unwrapping")
        .gui("How the wrapped phase is unwrapped"),
        d("field_mapping.laplacian_solver", FieldMap, "Solver", "", Enum(vec![
            o("dct", "dct", "DCT", "Unweighted least squares, Neumann-boundary DCT solve (Ghiglia & Romero 1994; QSM.jl :dct) (default)"),
            o("fft", "fft", "FFT", "Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume"),
            o("mirror", "mirror", "Mirror", "Schofield & Zhu sin/cos Laplacian, FFT solve on the volume mirrored along each axis (QSMART's reference)"),
        ])).flag("--laplacian-solver").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian).full("Laplacian solver")
        .gui("How Laplacian unwrapping's Poisson equation is solved"),
        d("field_mapping.laplacian_fft_pad", FieldMap, "FFT pad (voxels)", "Zero-padding per side: one value for every axis or three (x y z). Default: 64", IntVec3).units("voxels").flag("--laplacian-fft-pad").parent("field_mapping.laplacian_solver")
        .when(|x| x.laplacian && x.c.field_mapping.laplacian_solver == LaplacianSolver::Fft).full("Laplacian FFT padding")
        .gui("Zero-padding per side: one value for every axis, or three (x y z)"),
        // MEDI's complex fit fits its own phase offset: QSM.rs does not remove them first.
        d("field_mapping.phase_offset_removal", FieldMap, "Phase offset removal", "Remove spatially-varying phase offset using HIP (Eckstein et al., 2018); multi-echo only", Bool).flag("--phase-offset-removal").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit),
        d("field_mapping.phase_offset_sigma_mm", FieldMap, "Sigma (mm)", "Gaussian smoothing sigma in mm (x y z)", FloatVec3).units("mm").flag("--phase-offset-sigma-mm").parent("field_mapping.phase_offset_removal")
        .when(|x| !x.laplacian && x.c.field_mapping.phase_offset_removal && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit).full("Phase offset sigma")
        .gui("Gaussian smoothing sigma (x y z)"),
        d("field_mapping.bipolar_correction", FieldMap, "Bipolar correction", "Remove the linear phase artefact of bipolar readout gradients (needs at least 3 echoes; fewer: skipped)", Bool).flag("--bipolar-correction").parent("field_mapping.phase_offset_removal")
        .when(|x| !x.laplacian && x.c.field_mapping.phase_offset_removal && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit),
        d("field_mapping.romeo.individual", FieldMap, "Individual mode", "Unwrap each echo independently (recommended). Off = template-based temporal unwrapping.", Bool).flag("--no-romeo-individual").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian).full("Unwrap echoes individually"),
        // The config counts echoes from 0; the TUI and --romeo-template from 1.
        d("field_mapping.romeo.template", FieldMap, "Template echo", "Echo used for spatial unwrapping, counted from 0 (--romeo-template counts from 1)", Int).flag("--romeo-template").parent("field_mapping.romeo.individual")
        .when(|x| !x.laplacian && !x.c.field_mapping.romeo.individual)
        .gui("Echo used for spatial unwrapping, counted from 0"),
        d("field_mapping.romeo.correct_global", FieldMap, "Correct global", "Correct inter-echo 2π offsets after unwrapping", Bool).flag("--no-romeo-correct-global").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian).full("Correct global offsets"),
        d("field_mapping.romeo.phase_coherence", FieldMap, "Phase coherence", "Weight edges by neighbour phase coherence", Bool).flag("--no-romeo-phase-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.phase_gradient_coherence", FieldMap, "Phase gradient coherence", "Weight edges by inter-echo phase-gradient consistency (multi-echo)", Bool).flag("--no-romeo-phase-gradient-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.phase_linearity", FieldMap, "Phase linearity", "Weight edges by second-derivative smoothness", Bool).flag("--no-romeo-phase-linearity").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.mag_coherence", FieldMap, "Mag coherence", "Weight edges by neighbor magnitude coherence", Bool).flag("--no-romeo-mag-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian).full("Magnitude coherence")
        .gui("Weight edges by neighbour magnitude coherence"),
        d("field_mapping.romeo.mag_weight", FieldMap, "Mag weight", "Penalise low-signal voxels (off by default)", Bool).flag("--romeo-mag-weight").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian).full("Magnitude weight")
        .gui("Penalise low-signal voxels"),
        d("field_mapping.romeo.mag_weight2", FieldMap, "Mag weight 2", "Penalise abnormally high signal / flow artefacts (off by default)", Bool).flag("--romeo-mag-weight2").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian).full("Magnitude weight 2")
        .gui("Penalise abnormally high signal and flow artefacts"),
        d("field_mapping.romeo.bestpath", FieldMap, "Best-path weights", "Use Abdul-Rahman best-path weights instead of ROMEO weights", Bool).flag("--romeo-bestpath").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.temporal_uncertain_unwrapping", FieldMap, "Temporal uncertain", "Re-unwrap voxels whose temporal unwrapping is uncertain", Float)
        .value("Quality threshold", "Quality threshold (0, 1] below which a voxel is re-unwrapped").off(json!(0.0)).flag("--romeo-temporal-uncertain-unwrapping").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian).full("Temporal uncertain unwrapping"),
        d("field_mapping.romeo.max_seeds", FieldMap, "Max seeds", "Maximum number of seed regions (default 1)", Int).flag("--romeo-max-seeds").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian).full("Maximum seeds")
        .gui("Maximum number of seed regions"),
        d("field_mapping.romeo.merge_regions", FieldMap, "Merge regions", "Merge neighboring regions after unwrapping", Bool).flag("--romeo-merge-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian)
        .gui("Merge neighbouring regions after unwrapping"),
        d("field_mapping.romeo.correct_regions", FieldMap, "Correct regions", "Shift each region's median to nearest 0 by n·2π", Bool).flag("--romeo-correct-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian)
        .gui("Shift each region by a multiple of 2π to bring its median nearest 0"),
        d("field_mapping.romeo.wrap_addition", FieldMap, "Wrap addition", "Extra phase tolerance beyond π for neighbor differences [0,π]", Float).flag("--romeo-wrap-addition").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian)
        .gui("Extra phase tolerance beyond π for neighbour differences, from 0 to π"),
        d("field_mapping.b0_estimation", FieldMap, "B0 estimation", "", Enum(vec![
            o("weighted-avg", "weighted-avg", "Weighted average", "Weighted average of phase/TE across echoes (default)"),
            o("linear-fit", "linear-fit", "Linear fit", "Magnitude-weighted linear fit of phase vs TE"),
            o("robust-fit", "robust-fit", "Robust fit", "Motion-robust linear fit (IRLS): down-weights echoes departing from a straight line, per voxel. Needs 4 echoes (3 without offset)"),
            o("qsmart-echofit", "qsmart-echofit", "QSMART echo fit", "QSMART's: magnitude-weighted fit of phase vs TE through the origin, each echo unwrapped over the whole volume; marks the voxels QSMART leaves out. Needs Laplacian unwrapping"),
            o("complex-fit", "complex-fit", "Complex fit", "MEDI's field map (Fit_ppm_complex): a magnitude-weighted fit of the complex echoes, then unwrapping of the fitted phase. Needs the magnitude and evenly spaced echoes; no phase offset removal or bipolar correction"),
        ])).basic().flag("--b0-estimation")
        .gui("How the unwrapped echoes are combined into one field map"),
        d("field_mapping.qsmart_fit_threshold_mode", FieldMap, "Fit threshold", "", Enum(vec![
            o("percentile", "percentile", "Percentile", "A percentile of the blurred fit residual over the volume, as QSMART's authors run it (default)"),
            o("absolute", "absolute", "Fixed", "A fixed fit residual"),
        ])).flag("--qsmart-fit-threshold-mode").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit).full("QSMART fit threshold")
        .gui("How the fit residual at which a voxel is left out of QSMART's reconstruction is set"),
        // QSM.rs utils/qsmart.rs QsmartFitThreshold::validate refuses a percentile outside (0, 100].
        d("field_mapping.qsmart_fit_percentile", FieldMap, "Fit percentile", "Percentile of the blurred fit residual above which a voxel is left out of QSMART's reconstruction (default: 99)", Float).above(0.0).max(100.0).flag("--qsmart-fit-percentile").parent("field_mapping.qsmart_fit_threshold_mode")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit && x.c.field_mapping.qsmart_fit_threshold_mode == QsmartFitThresholdMode::Percentile).full("QSMART fit percentile")
        .gui("Percentile of the blurred fit residual above which a voxel is left out of QSMART's reconstruction"),
        // QSM.rs utils/qsmart.rs QsmartFitThreshold::validate refuses a threshold that is not > 0.
        d("field_mapping.qsmart_fit_threshold", FieldMap, "Fit residual", "Blurred fit residual above which a voxel is left out of QSMART's reconstruction (default: 40)", Float).above(0.0).flag("--qsmart-fit-threshold").parent("field_mapping.qsmart_fit_threshold_mode")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit && x.c.field_mapping.qsmart_fit_threshold_mode == QsmartFitThresholdMode::Absolute).full("QSMART fit residual")
        .gui("Blurred fit residual above which a voxel is left out of QSMART's reconstruction"),
        d("field_mapping.b0_weight_type", FieldMap, "Weight type", "", Enum(vec![
            o("phase-snr", "phase-snr", "Phase SNR", "mag × TE — optimal for phase SNR (default)"),
            o("phase-var", "phase-var", "Phase variance", "mag² × TE² — based on phase variance"),
            o("average", "average", "Uniform", "Uniform weights (unweighted average)"),
            o("tes", "tes", "Echo time", "TE only"),
            o("mag", "mag", "Magnitude", "Magnitude only"),
            o("assumed-decay", "assumed-decay", "Assumed decay", "Fixed TE·exp(−TE/T2*) per echo from an assumed T2* (no T2* map, no magnitude); phase-snr is generally better"),
        ])).flag("--b0-weight-type").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg).full("Echo weighting")
        .gui("How each echo is weighted in the average"),
        // QSMxT fieldmap.rs and QSM.rs pipeline/field_mapping.rs refuse a T2* that is not > 0.
        d("field_mapping.b0_weight_assumed_t2star_ms", FieldMap, "Assumed T2* (ms)", "T2* assumed to compute the fixed echo weights; no T2* map is used (default: 40 ms)", Float).above(0.0).units("ms").flag("--b0-weight-assumed-t2star").parent("field_mapping.b0_weight_type")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg && x.c.field_mapping.b0_weight_type == B0WeightType::AssumedDecay).full("Assumed T2*")
        .gui("T2* assumed to compute the fixed echo weights; no T2* map is used"),
        d("field_mapping.linear_fit.estimate_offset", FieldMap, "Estimate offset", "Fit a phase-offset term in the echo-time fit (robust fit: needs 4 echoes with it, 3 without)", Bool).flag("--linear-fit-estimate-offset").parent("field_mapping.b0_estimation")
        .when(|x| matches!(x.c.field_mapping.b0_estimation, B0Estimation::LinearFit | B0Estimation::RobustFit)).context("Echo-time fit"),
        // QSM.rs RobustFitParams::validate (utils/multi_echo.rs) refuses a percentile outside [0, 100].
        d("field_mapping.linear_fit.reliability_threshold_percentile", FieldMap, "Reliability mask", "Report the fit's reliability mask; does not change the field map", Float).min(0.0).max(100.0)
        .value("Percentile", "Percentile of the fit residual above which a voxel counts as unreliable").off(json!(0.0)).flag("--linear-fit-reliability-threshold").parent("field_mapping.b0_estimation")
        .when(|x| matches!(x.c.field_mapping.b0_estimation, B0Estimation::LinearFit | B0Estimation::RobustFit)).context("Echo-time fit"),
        d("field_mapping.robust_fit.loss", FieldMap, "Loss", "", Enum(vec![
            o("tukey", "tukey", "Tukey", "Tukey biweight: rejects an echo at a voxel past the tuning radius (default)"),
            o("huber", "huber", "Huber", "Huber: caps an outlying echo's influence but never rejects it"),
        ])).flag("--robust-fit-loss").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Loss function")
        .gui("The loss that down-weights an echo departing from the straight line through the others"),
        // QSM.rs RobustFitParams::validate: tuning must be > 0.
        d("field_mapping.robust_fit.tuning", FieldMap, "Tuning", "Tuning radius in robust-scale units; blank = conventional (Tukey 4.685, Huber 1.345)", Float).above(0.0).optional().flag("--robust-fit-tuning").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Tuning radius")
        .gui("Tuning radius in robust-scale units; empty for the conventional value (Tukey 4.685, Huber 1.345)"),
        d("field_mapping.robust_fit.iterations", FieldMap, "Iterations", "IRLS passes (0 = report only: the field map is the linear fit's)", Int).flag("--robust-fit-iterations").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Reweighting passes")
        .gui("Iteratively reweighted passes (0 reports only: the field map is the linear fit's)"),
        // QSM.rs RobustFitParams::validate: downweight level in (0, 1].
        d("field_mapping.robust_fit.downweight_level", FieldMap, "Downweight level", "Reporting only: a weight below this counts as down-weighted (0, 1]", Float).above(0.0).max(1.0).flag("--robust-fit-downweight-level").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit)
        .gui("Reporting only: a weight below this counts as down-weighted"),
        // QSM.rs RobustFitParams::validate: flag ratio >= 1.
        d("field_mapping.robust_fit.flag_ratio", FieldMap, "Flag ratio", "Reporting only: flag an echo whose outlier score exceeds this x the best echo's (>= 1)", Float).min(1.0).flag("--robust-fit-flag-ratio").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit)
        .gui("Reporting only: flag an echo whose outlier score exceeds this multiple of the best echo's"),
        // QSM.rs fit_ppm_complex (utils/multi_echo.rs) refuses a reltol that is not >= 0.
        d("field_mapping.complex_fit.reltol", FieldMap, "Rel tolerance", "Stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4)", Float).min(0.0).flag("--complex-fit-reltol").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::ComplexFit).full("Relative tolerance")
        .gui("The fit stops once its update is this fraction of the first estimate, over the whole volume"),
        // 0 keeps the fit's starting estimate (a linear fit to the first three echoes' phase).
        d("field_mapping.complex_fit.max_iter", FieldMap, "Max iter", "Gauss-Newton iterations at most (default 30)", Int).flag("--complex-fit-max-iter").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::ComplexFit).full("Maximum iterations")
        .gui("Gauss-Newton iterations at most"),
        d("bg_removal.algorithm", Background, "BG removal", "", Enum(vec![
            o("vsharp", "vsharp", "V-SHARP", "Variable-kernel SHARP (V-SHARP) — https://doi.org/10.1002/mrm.23000"),
            o("pdf", "pdf", "PDF", "Projection onto Dipole Fields (PDF) — https://doi.org/10.1002/nbm.1670"),
            o("lbv", "lbv", "LBV", "Laplacian Boundary Value (LBV) — https://doi.org/10.1002/nbm.3064"),
            o("ismv", "ismv", "iSMV", "Iterative Spherical Mean Value (iSMV) — https://doi.org/10.1002/mrm.24998"),
            o("sharp", "sharp", "SHARP", "SHARP (Sophisticated Harmonic Artifact Reduction) — https://doi.org/10.1016/j.neuroimage.2010.10.070"),
            o("resharp", "resharp", "RESHARP", "Regularized SHARP (RESHARP) with Tikhonov — https://doi.org/10.1002/mrm.25032"),
            o("harperella", "harperella", "HARPERELLA", "HARPERELLA integrated unwrap+BFR — https://doi.org/10.1002/nbm.3056"),
            o("iharperella", "iharperella", "iHARPERELLA", "Improved HARPERELLA (iHARPERELLA) — Li et al., Proc. ISMRM 2015, p.3313"),
            o("bfrnet", "bfrnet", "BFRnet", "BFRnet deep-learning background removal (weights auto-downloaded) — https://github.com/sunhongfu/BFRnet"),
            o("iqfm", "iqfm", "iQFM", "iQFM deep-learning joint unwrapping + background removal from phase (weights auto-downloaded) — https://doi.org/10.1016/j.neuroimage.2022.119410"),
        ])).basic().flag("--bf-algorithm").full("Background removal")
        .gui("How the background field, from sources outside the mask, is removed to leave the local field"),
        d("bg_removal.vsharp.deconvolve", Background, "Deconvolve", "Undo the SMV high-pass with a truncated-SVD deconvolution (default: off)", Bool).flag("--vsharp-deconvolve").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("Undo the SMV high-pass with a truncated-SVD deconvolution"),
        // QSM.rs bgremove/vsharp.rs: at <= 0 the DC bin (1 - S = 0) is inverted, giving inf/NaN.
        d("bg_removal.vsharp.threshold", Background, "Threshold", "Deconvolution (TSVD) threshold", Float).above(0.0).flag("--vsharp-threshold").parent("bg_removal.vsharp.deconvolve")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.deconvolve),
        d("bg_removal.vsharp.max_radius", Background, "Max radius", "Largest SMV kernel radius (mm)", Float).units("mm").flag("--vsharp-max-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")).full("Maximum radius")
        .gui("Largest SMV kernel radius"),
        // QSM.rs VsharpParams::validate: min_radius and radius_step must be > 0.
        d("bg_removal.vsharp.min_radius", Background, "Min radius", "Smallest SMV kernel radius (mm)", Float).above(0.0).units("mm").flag("--vsharp-min-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")).full("Minimum radius")
        .gui("Smallest SMV kernel radius"),
        d("bg_removal.vsharp.radius_step", Background, "Radius step", "Step between SMV kernel radii (mm)", Float).above(0.0).units("mm").flag("--vsharp-radius-step").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("Step between SMV kernel radii, from the largest down to the smallest"),
        d("bg_removal.vsharp.grid", Background, "Working grid", "", Enum(vec![
            o("native", "native", "Native", "The acquired voxel grid"),
            o("finest", "finest", "Finest", "An isotropic grid at the smallest voxel dimension, so no axis is coarsened; isotropic data stays on the acquired grid"),
            o("isotropic", "isotropic", "Isotropic", "An isotropic grid of Grid spacing mm; the background is interpolated back, the local field keeps native resolution"),
        ])).flag("--vsharp-grid").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("The voxel grid V-SHARP runs on"),
        // QSM.rs pipeline/bg_removal.rs refuses an isotropic spacing that is not > 0.
        d("bg_removal.vsharp.grid_spacing", Background, "Grid spacing", "Isotropic working-grid spacing (mm)", Float).above(0.0).units("mm").flag("--vsharp-grid-spacing").parent("bg_removal.vsharp.grid")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.grid == VsharpGrid::Isotropic)
        .gui("Isotropic working-grid spacing"),
        d("bg_removal.pdf.tol", Background, "Tolerance", "Convergence tolerance, relative to the noise level", Float).flag("--pdf-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")),
        d("bg_removal.pdf.n_cg", Background, "CG iter", "Maximum conjugate-gradient iterations", Int).min(1.0).flag("--pdf-n-cg").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")).full("CG iterations"),
        d("bg_removal.pdf.space", Background, "Kernel", "", Enum(vec![
            o("image", "image", "Image space", "The FFT of the dipole field sampled on the voxel grid (default)"),
            o("kspace", "kspace", "k-space", "The analytic k-space dipole kernel"),
        ])).flag("--pdf-space").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf"))
        .gui("The dipole kernel the background is fitted with"),
        // QSM.rs bgremove/pdf.rs refuses 1 (PdfConfig::validate).
        d("bg_removal.pdf.n_pad", Background, "Pad (voxels)", "Zero-padding around the mask's bounding box, voxels; 0 = the full volume", Int).min(0.0).units("voxels").flag("--pdf-n-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")).full("Padding")
        .gui("Zero-padding around the mask's bounding box; 0 runs on the full volume"),
        d("bg_removal.lbv.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--lbv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")),
        d("bg_removal.ismv.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--ismv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")),
        // QSM.rs bgremove/ismv.rs: 0 iterations give an all-zero local field.
        d("bg_removal.ismv.max_iter", Background, "Max iter", "Maximum iterations", Int).min(1.0).flag("--ismv-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")).full("Maximum iterations"),
        d("bg_removal.ismv.radius", Background, "Radius", "SMV kernel radius (mm)", Float).units("mm").flag("--ismv-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv"))
        .gui("SMV kernel radius"),
        // QSM.rs bgremove/sharp.rs: at <= 0 the DC bin is divided by ~0, giving inf/NaN.
        d("bg_removal.sharp.threshold", Background, "Threshold", "Deconvolution threshold", Float).above(0.0).flag("--sharp-threshold").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp")),
        d("bg_removal.sharp.radius", Background, "Radius", "SMV kernel radius (mm)", Float).units("mm").flag("--sharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp"))
        .gui("SMV kernel radius"),
        d("bg_removal.resharp.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--resharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp"))
        .gui("SMV kernel radius"),
        d("bg_removal.resharp.tik_reg", Background, "Tik reg", "Tikhonov regularization parameter", Float).flag("--resharp-tik-reg").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")).full("Tikhonov regularisation")
        .gui("Tikhonov regularisation parameter"),
        d("bg_removal.resharp.tol", Background, "Tolerance", "CG convergence tolerance", Float).flag("--resharp-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")),
        // QSM.rs solvers/cg.rs: 0 iterations return x = 0, an all-zero local field.
        d("bg_removal.resharp.max_iter", Background, "Max iter", "Maximum CG iterations", Int).min(1.0).flag("--resharp-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")).full("Maximum iterations"),
        d("bg_removal.harperella.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--harperella-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella"))
        .gui("SMV kernel radius"),
        d("bg_removal.harperella.max_iter", Background, "Max iter", "Maximum iterations", Int).flag("--harperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")).full("Maximum iterations"),
        d("bg_removal.harperella.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--harperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")),
        d("bg_removal.harperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--harperella-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory"),
        d("bg_removal.iharperella.max_iter", Background, "Max iter", "Maximum iterations", Int).flag("--iharperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")).full("Maximum iterations"),
        d("bg_removal.iharperella.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--iharperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")),
        d("bg_removal.iharperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--iharperella-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory"),
        d("pipeline.do_qsm", Inversion, "Compute QSM", "Enable QSM reconstruction (disable to only run the other outputs)", Bool).basic().flag("--no-qsm").also(&[FieldMap, Background, MultiOrientation, Reference]),
        d("inversion.algorithm", Inversion, "QSM inversion", "", Enum(vec![
            o("rts", "rts", "RTS", "RTS (Rapid Two-Step) — fast two-step k-space inversion with streaking removal. https://doi.org/10.1016/j.neuroimage.2017.11.018"),
            o("tv", "tv", "TV-ADMM", "TV-ADMM (Total Variation) — ADMM inversion with a total-variation prior. https://doi.org/10.1002/mrm.25029"),
            o("tkd", "tkd", "TKD", "TKD (Thresholded K-space Division) — direct k-space division with thresholding. https://doi.org/10.1002/mrm.22135"),
            o("tsvd", "tsvd", "TSVD", "TSVD (Truncated SVD) — zeroes the small singular values of the dipole kernel. https://doi.org/10.1002/mrm.22135"),
            o("tgv", "tgv", "TGV", "TGV (Total Generalized Variation) — single-step: unwrapping, background removal and inversion together. https://doi.org/10.1016/j.neuroimage.2015.02.041"),
            o("tikhonov", "tikhonov", "Tikhonov", "Tikhonov — closed-form L2-regularized inversion. https://doi.org/10.1002/jmri.24365"),
            o("nltv", "nltv", "NLTV", "NLTV (Nonlinear Total Variation) — nonlinear data fidelity with a TV prior. https://doi.org/10.1002/mrm.27073"),
            o("medi", "medi", "MEDI", "MEDI (Morphology Enabled Dipole Inversion) — magnitude-guided edge weighting. https://doi.org/10.1002/mrm.22816"),
            o("tfi", "tfi", "TFI", "TFI (Total Field Inversion) — inverts the total field directly (its own background removal). https://doi.org/10.1002/mrm.26331"),
            o("ilsqr", "ilsqr", "iLSQR", "iLSQR (default) — iterative LSQR with streaking-artifact correction. https://doi.org/10.1016/j.neuroimage.2014.12.043"),
            o("star", "star", "STAR-QSM", "STAR-QSM (Streaking Artifact Reduction) — sparse (L1) inversion from a k-space initial guess, for sources with a large susceptibility range. https://doi.org/10.1002/nbm.3383"),
            o("lsqr", "lsqr", "LSQR", "LSQR (minimally regularised) — one least-squares solve for χ plus a residual field; the residual weight is the only regularisation. https://doi.org/10.1118/1.3481505"),
            o("heidi", "heidi", "HEIDI", "HEIDI (Homogeneity Enabled Incremental Dipole Inversion) — keeps the well-conditioned k-space of an LSQR solve and re-derives the dipole cone under a homogeneity prior. https://doi.org/10.1016/j.neuroimage.2012.05.067"),
            o("qsmart", "qsmart", "QSMART", "QSMART — two-stage spatially-dependent filtering plus inversion. https://doi.org/10.1016/j.neuroimage.2020.117701"),
            o("ndi", "ndi", "NDI", "NDI (Nonlinear Dipole Inversion) — gradient-descent nonlinear fit. https://doi.org/10.1002/nbm.4271"),
            o("fansi", "fansi", "FANSI (TV)", "FANSI (Nonlinear TV) — fast nonlinear susceptibility inversion with a TV prior. https://doi.org/10.1002/mrm.27073"),
            o("fansi-tgv", "fansi-tgv", "FANSI (TGV)", "FANSI (Nonlinear TGV) — FANSI with a total-generalized-variation prior. https://doi.org/10.1002/mrm.27073"),
            o("l1qsm", "l1qsm", "L1-QSM", "L1-QSM (L1 Data Fidelity) — L1 fidelity for streaking suppression. https://doi.org/10.1002/mrm.29057"),
            o("whqsm", "whqsm", "WH-QSM", "WH-QSM (Weak-Harmonic QSM) — jointly fits a residual harmonic background field. https://doi.org/10.1002/mrm.27483"),
            o("hdqsm", "hdqsm", "HD-QSM", "HD-QSM (Hybrid Data Fidelity) — combined L1/L2 data fidelity. https://doi.org/10.1002/mrm.29296"),
            o("amp-pe", "amp-pe", "AMP-PE", "AMP-PE (Approximate Message Passing with Parameter Estimation) — self-tuning Bayesian inversion. https://doi.org/10.1002/mrm.29725"),
            o("xqsm", "xqsm", "xQSM", "xQSM (deep learning; weights auto-downloaded) — octave-convolution U-net dipole inversion. https://doi.org/10.1002/nbm.4461"),
            o("qsmnet", "qsmnet", "QSMnet", "QSMnet (deep learning; weights auto-downloaded) — 3D U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2018.06.030"),
            o("qsmnet-plus", "qsmnet-plus", "QSMnet+", "QSMnet+ (deep learning; weights auto-downloaded) — QSMnet trained for a wider susceptibility range. https://doi.org/10.1016/j.neuroimage.2020.116579"),
            o("autoqsm", "autoqsm", "AutoQSM", "AutoQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2019.116064"),
            o("qsmgan", "qsmgan", "QSMGAN", "QSMGAN (deep learning; weights auto-downloaded) — GAN-refined U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2019.116389"),
            o("ir2qsm", "ir2qsm", "IR2QSM", "IR2QSM (deep learning; weights auto-downloaded) — iterative unrolled U-net inversion. https://doi.org/10.1002/mp.17747"),
            o("lpcnn", "lpcnn", "LPCNN", "LPCNN (deep learning; weights auto-downloaded) — learned-proximal unrolled inversion. https://doi.org/10.1007/978-3-030-59713-9_13"),
            o("modl-qsm", "modl-qsm", "MoDL-QSM", "MoDL-QSM (deep learning; weights auto-downloaded) — model-based unrolled inversion (χ33/STI). https://doi.org/10.1016/j.neuroimage.2021.118376"),
            o("nextqsm", "nextqsm", "NeXtQSM", "NeXtQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2022.119729"),
            o("iqsm", "iqsm", "iQSM", "iQSM (deep learning; weights auto-downloaded) — end-to-end reconstruction from wrapped phase. https://doi.org/10.1016/j.neuroimage.2022.119410"),
            o("iqsm-plus", "iqsm-plus", "iQSM+", "iQSM+ (deep learning; weights auto-downloaded) — orientation-adaptive end-to-end reconstruction. https://doi.org/10.1016/j.media.2024.103160"),
        ])).basic().flag("--qsm-algorithm")
        .gui("How the field is inverted into a susceptibility map"),
        // QSM.rs inversion/rts.rs: no |D| exceeds 2/3, so delta >= 2/3 empties the mask and the map is all zero.
        d("inversion.rts.delta", Inversion, "Delta", "Threshold for ill-conditioned k-space region", Float).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--rts-delta").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.mu", Inversion, "Mu", "Regularization parameter for well-conditioned region", Float).flag("--rts-mu").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.rho", Inversion, "Rho", "ADMM penalty parameter", Float).flag("--rts-rho").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.tol", Inversion, "Tolerance", "Convergence tolerance (relative change)", Float).flag("--rts-tol").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.max_iter", Inversion, "Max iter", "Maximum ADMM iterations", Int).flag("--rts-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")).full("Maximum iterations"),
        d("inversion.rts.lsmr_iter", Inversion, "LSMR iter", "LSMR iterations for step 1 (well-conditioned solve)", Int).flag("--rts-lsmr-iter").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")).full("LSMR iterations"),
        d("inversion.rts.tau", Inversion, "Tau", "Rho is raised when the primal residual falls by less than this factor", Float).flag("--rts-tau").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.gamma", Inversion, "Gamma", "Factor rho is raised by", Float).flag("--rts-gamma").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.tv.lambda", Inversion, "Lambda", "L1 regularization weight (larger = smoother)", Float).flag("--tv-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv"))
        .gui("L1 regularisation weight (larger = smoother)"),
        d("inversion.tv.rho", Inversion, "Rho", "ADMM penalty parameter. Blank = 100×lambda", Float).above(0.0).optional().flag("--tv-rho").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        d("inversion.tv.tol", Inversion, "Tolerance", "Convergence tolerance", Float).flag("--tv-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        // QSM.rs inversion/tv.rs: 0 iterations give an all-zero map.
        d("inversion.tv.max_iter", Inversion, "Max iter", "Maximum ADMM iterations", Int).min(1.0).flag("--tv-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")).full("Maximum iterations"),
        // QSM.rs inversion/tkd.rs: 1/threshold on the DC bin (D = 0) gives inf/NaN at <= 0.
        d("inversion.tkd.threshold", Inversion, "Threshold", "Truncation threshold for k-space division (0.1-0.2)", Float).above(0.0).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--tkd-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("tkd") || x.qsmart_inner("tkd")),
        // QSM.rs inversion/tkd.rs: < 0 inverts D = 0 (NaN); >= 2/3 zeroes every bin.
        d("inversion.tsvd.threshold", Inversion, "Threshold", "Truncation threshold for SVD (0.1-0.2)", Float).min(0.0).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--tsvd-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("tsvd") || x.qsmart_inner("tsvd")),
        // QSM.rs inversion/tgv.rs: 0 iterations give an all-zero map.
        d("inversion.tgv.iterations", Inversion, "Iterations", "Primal-dual iterations. Blank = from the voxel size, as the reference does", Int).min(1.0).optional().flag("--tgv-iterations").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.erosions", Inversion, "Erosions", "Mask erosions before TGV solve", Int).flag("--tgv-erosions").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        // QSM.rs inversion/tgv.rs project_linf3: a negative weight divides by a zero norm (NaN).
        d("inversion.tgv.alpha1", Inversion, "Alpha1", "First-order TGV weight (gradient term), in radians at the echo time and field strength", Float).min(0.0).units("rad").flag("--tgv-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("tgv"))
        .gui("First-order TGV weight (gradient term), at the echo time and field strength"),
        // QSM.rs inversion/tgv.rs project_linf6: a negative weight divides by a zero norm (NaN).
        d("inversion.tgv.alpha0", Inversion, "Alpha0", "Second-order TGV weight (symmetric gradient term), in radians at the echo time and field strength", Float).min(0.0).units("rad").flag("--tgv-alpha0").parent("inversion.algorithm")
        .when(|x| x.inv("tgv"))
        .gui("Second-order TGV weight (symmetric gradient term), at the echo time and field strength"),
        d("inversion.tgv.step_size", Inversion, "Step size", "Primal step-size multiplier", Float).flag("--tgv-step-size").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--tgv-te").parent("inversion.algorithm")
        .when(|x| x.inv("tgv"))
        .gui("The echo time the field is converted to phase at (several echoes; a single echo is at its own); the weights are in radians at it").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.tikhonov.lambda", Inversion, "Lambda", "L2 regularization weight", Float).flag("--tikhonov-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tikhonov") || x.qsmart_inner("tikhonov"))
        .gui("L2 regularisation weight"),
        d("inversion.nltv.lambda", Inversion, "Lambda", "Regularization parameter", Float).flag("--nltv-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv"))
        .gui("Regularisation parameter"),
        d("inversion.nltv.rho", Inversion, "Rho", "ADMM penalty parameter. Blank = 100×lambda", Float).above(0.0).optional().flag("--nltv-rho").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.mu", Inversion, "Mu", "Penalty parameter", Float).flag("--nltv-mu").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.tol", Inversion, "Tolerance", "Convergence tolerance", Float).flag("--nltv-tol").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        // QSM.rs inversion/nltv.rs: 0 iterations give an all-zero map.
        d("inversion.nltv.max_iter", Inversion, "Max iter", "Maximum ADMM iterations", Int).min(1.0).flag("--nltv-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")).full("Maximum iterations"),
        d("inversion.nltv.tol_newton", Inversion, "Newton tol", "Convergence tolerance of the Newton iterations of the data-term subproblem", Float).flag("--nltv-tol-newton").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")).full("Newton tolerance"),
        // QSM.rs inversion/nltv.rs: 0 Newton iterations give an all-zero map.
        d("inversion.nltv.max_iter_newton", Inversion, "Newton iter", "Maximum Newton iterations of the data-term subproblem", Int).min(1.0).flag("--nltv-newton-iter").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")).full("Newton iterations"),
        d("inversion.nltv.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--nltv-te").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.medi.smv", Inversion, "SMV mode", "High-pass the local field with a spherical mean value (SMV) filter, removing residual background near the brain edge (MEDI-SMV)", Bool).flag("--no-medi-smv").parent("inversion.algorithm")
        .when(|x| x.inv("medi")),
        d("inversion.medi.smv_radius", Inversion, "SMV radius", "SMV radius in mm (also mSMV's)", Float).units("mm").flag("--medi-smv-radius").parent("inversion.algorithm")
        .when(|x| x.inv("medi"))
        .gui("SMV radius (also mSMV's)"),
        d("inversion.medi.smv_shrink_mask", Inversion, "Shrink mask", "With SMV, shrink the mask to where the SMV of the mask exceeds 0.999, dropping the edge band whose SMV reaches outside it. Off keeps the full mask", Bool).flag("--no-medi-smv-shrink-mask").parent("inversion.medi.smv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.smv && !x.c.inversion.medi.msmv).full("SMV mask shrinking"),
        d("inversion.medi.msmv", Inversion, "mSMV", "Filter the local field by maximum spherical mean value (MEDI-mSMV, Roberts 2024) before MEDI; needs at least 3 echoes", Bool).flag("--medi-msmv").parent("inversion.algorithm")
        .when(|x| x.inv("medi")).full("MEDI-mSMV")
        .gui("Filter the local field by maximum spherical mean value (mSMV) before MEDI, which removes residual background at the brain edge; needs at least three echoes"),
        // QSM.rs inversion/msmv.rs: the boundary correction makes at most maxk − 1 passes.
        d("inversion.medi.msmv_maxk", Inversion, "mSMV max K", "mSMV's boundary correction makes at most this many passes, less one", Int).min(1.0).flag("--medi-msmv-maxk").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv).full("mSMV maximum k"),
        d("inversion.medi.msmv_vessel_radius", Inversion, "mSMV vessel radius", "Largest vessel thickness of mSMV's vessel mask, in voxels. Blank = max(15, 8 × the largest voxel dimension)", Float).above(0.0).optional().units("voxels").flag("--medi-msmv-vessel-radius").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv)
        .gui("Largest vessel thickness of mSMV's vessel mask. Empty: 15 voxels, or 8 times the largest voxel dimension if that is more"),
        d("inversion.medi.msmv_prefilter", Inversion, "mSMV prefilter", "SMV-filter the field before mSMV, as the authors do. Off diverges after PDF, V-SHARP or LBV (expert)", Bool).flag("--no-medi-msmv-prefilter").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv),
        d("inversion.medi.lambda", Inversion, "Lambda", "Data-fidelity weight (larger fits the field more closely)", Float).flag("--medi-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("Data-fidelity weight (larger fits the field more closely)"),
        d("inversion.medi.percentage", Inversion, "Percentage", "Fraction of voxels considered edges (0.0-1.0)", Float).flag("--medi-percentage").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        // QSM.rs inversion/medi.rs: 0 iterations give an all-zero map.
        d("inversion.medi.max_iter", Inversion, "Max iter", "Maximum outer iterations", Int).min(1.0).flag("--medi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("Maximum iterations"),
        // QSM.rs inversion/medi.rs: 0 CG iterations give an all-zero map.
        d("inversion.medi.cg_max_iter", Inversion, "CG max iter", "Maximum conjugate gradient iterations", Int).min(1.0).flag("--medi-cg-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("CG maximum iterations"),
        d("inversion.medi.cg_tol", Inversion, "CG tolerance", "CG convergence tolerance", Float).flag("--medi-cg-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.tol", Inversion, "Tolerance", "Outer convergence tolerance", Float).flag("--medi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.merit", Inversion, "MERIT", "Iteratively down-weight voxels whose residual stays large", Bool).flag("--medi-merit").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        // QSM.rs inversion/medi.rs: the CSF term needs R2* from at least 3 echoes; with fewer, MEDI
        // runs without it (and the run says so, requirements::medi_csf_note).
        d("inversion.medi.csf", Inversion, "CSF reference", "Zero-reference χ to the ventricular CSF and keep it uniform there (MEDI+0, Liu 2018); needs at least 3 echoes, with fewer MEDI runs without it", Bool).flag("--medi-csf").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("CSF zero reference (MEDI+0)")
        .gui("Zero-reference the susceptibility to the ventricular CSF, found from R2*, and keep it uniform there (MEDI+0); needs at least three echoes, with fewer MEDI runs without it"),
        d("inversion.medi.lambda_csf", Inversion, "Lambda CSF", "Weight of the CSF uniformity term", Float).min(0.0).flag("--medi-lambda-csf").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf).full("CSF weight"),
        d("inversion.medi.csf_r2s_threshold", Inversion, "CSF R2* threshold", "Voxels below this R2* are CSF candidates", Float).above(0.0).units("Hz").flag("--medi-csf-r2s-threshold").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf),
        d("inversion.medi.csf_erode", Inversion, "CSF erode", "Erode the mask by a 10 mm sphere before finding the CSF", Bool).flag("--medi-csf-erode").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf).full("Erode before finding CSF"),
        d("inversion.medi.zeropad", Inversion, "Zero padding", "Voxels of zeros appended at the end of x, y and z before the inversion; the result is cropped back (default 0 0 0)", IntVec3).units("voxels").flag("--medi-zeropad").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("Voxels of zeros appended at the end of each axis before the inversion; the result is cropped back"),
        d("inversion.medi.data_weighting", Inversion, "Data weighting", "", Enum(vec![
            o(0, "uniform", "Uniform", "Every voxel of the field weighs the same in the data term"),
            o(1, "magnitude SNR", "Magnitude SNR", "Weight the data term by the magnitude SNR, so noisy voxels count for less"),
        ])).flag("--medi-data-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("How the voxels of the field are weighted in the data term"),
        d("inversion.medi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--medi-te").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.tfi.lambda", Inversion, "Lambda", "Data-fidelity weight (larger fits the field more closely)", Float).flag("--tfi-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("Data-fidelity weight (larger fits the field more closely)"),
        d("inversion.tfi.precond", Inversion, "Precond", "Preconditioner value (susceptibility scaling outside the mask)", Float).flag("--tfi-precond").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")).full("Preconditioner"),
        d("inversion.tfi.percentage", Inversion, "Percentage", "Fraction of voxels considered edges (0.0-1.0)", Float).flag("--tfi-percentage").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        // QSM.rs inversion/tfi.rs: 0 iterations give an all-zero map.
        d("inversion.tfi.max_iter", Inversion, "Max iter", "Maximum outer iterations", Int).min(1.0).flag("--tfi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")).full("Maximum iterations"),
        // QSM.rs inversion/tfi.rs: 0 CG iterations give an all-zero map.
        d("inversion.tfi.cg_max_iter", Inversion, "CG max iter", "Maximum conjugate gradient iterations", Int).min(1.0).flag("--tfi-cg-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")).full("CG maximum iterations"),
        d("inversion.tfi.cg_tol", Inversion, "CG tolerance", "CG convergence tolerance", Float).flag("--tfi-cg-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.tol", Inversion, "Tolerance", "Outer convergence tolerance", Float).flag("--tfi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.merit", Inversion, "MERIT", "Iteratively down-weight voxels whose residual stays large", Bool).flag("--tfi-merit").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.linear", Inversion, "Linear data term", "Fit the field linearly, as the TFI paper does (default); off uses MEDI's exponential data term", Bool).flag("--tfi-linear").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("Fit the field linearly, as the TFI paper does; off uses MEDI's exponential data term"),
        d("inversion.tfi.data_weighting", Inversion, "Data weighting", "", Enum(vec![
            o(0, "uniform", "Uniform", "Every voxel of the field weighs the same in the data term"),
            o(1, "magnitude SNR", "Magnitude SNR", "Weight the data term by the magnitude SNR, so noisy voxels count for less"),
        ])).flag("--tfi-data-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("How the voxels of the field are weighted in the data term"),
        d("inversion.tfi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--tfi-te").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.ilsqr.tol", Inversion, "Tolerance", "Convergence tolerance", Float).flag("--ilsqr-tol").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr") || x.qsmart_inner("ilsqr")),
        d("inversion.ilsqr.max_iter", Inversion, "Max iter", "Maximum iterations", Int).flag("--ilsqr-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr")).full("Maximum iterations"),
        // QSM.rs IlsqrParams::validate (inversion/ilsqr.rs): pad must be >= 0.
        d("inversion.ilsqr.pad_mm", Inversion, "Pad (mm)", "Zero-padding per side, mm. Larger costs time and memory", Float).min(0.0).units("mm").flag("--ilsqr-pad-mm").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr")).full("Padding")
        .gui("Zero-padding per side. Larger costs time and memory"),
        d("inversion.star.pad", Inversion, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 8 8 20)", IntVec3).units("voxels").flag("--star-pad").parent("inversion.algorithm")
        .when(|x| x.inv("star")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory"),
        d("inversion.star.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--star-te").parent("inversion.algorithm")
        .when(|x| x.inv("star"))
        .gui("The echo time the field is converted to phase at; STAR-QSM's result depends on it").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.lsqr.residual_weighting", Inversion, "Residual weight", "Weight w on the residual field — the only regularisation. Blank = scaled from B0; 0 drops the term and streaks badly", Float).optional().flag("--lsqr-residual-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi"))
        .gui("Weight w on the residual field, the only regularisation. Empty: scaled from B0; 0 drops the term and streaks badly"),
        d("inversion.lsqr.fit_global_offset", Inversion, "Fit global offset", "Fit a single global field offset alongside χ", Bool).flag("--no-lsqr-global-offset").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi")),
        d("inversion.lsqr.tol", Inversion, "LSQR tolerance", "LSQR convergence tolerance", Float).flag("--lsqr-tol").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi")),
        // QSM.rs inversion/ilsqr.rs lsqr: 0 iterations return x = 0, an all-zero map.
        d("inversion.lsqr.max_iter", Inversion, "LSQR max iter", "Maximum LSQR iterations", Int).min(1.0).flag("--lsqr-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi")).full("LSQR maximum iterations"),
        // QSM.rs inversion/heidi.rs: no |D| exceeds 2/3, so the cone is empty and the map is all zero.
        d("inversion.heidi.cone_threshold", Inversion, "Cone threshold", "|D(k)| above which a coefficient is taken from the LSQR seed unchanged", Float).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--heidi-cone-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.gradient_threshold", Inversion, "Gradient threshold", "Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous", Float).units("ppm/mm").flag("--heidi-gradient-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("heidi"))
        .gui("Field-gradient threshold; below it a direction reads as homogeneous"),
        d("inversion.heidi.apply_laplacian_correction", Inversion, "Laplacian correction", "Open the homogeneity masks where the field's Laplacian is large", Bool).flag("--no-heidi-laplacian-correction").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.laplacian_threshold", Inversion, "Laplacian threshold", "Field-Laplacian threshold (ppm per voxel²)", Float).flag("--heidi-laplacian-threshold").parent("inversion.heidi.apply_laplacian_correction")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.apply_laplacian_correction),
        d("inversion.heidi.gradient_mask_floor", Inversion, "Gradient mask floor", "TV weight kept where every direction reads as an edge (0 = upstream's binarised masks)", Float).flag("--heidi-gradient-mask-floor").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.continuation_steps", Inversion, "Continuation steps", "NESTA continuation steps on μ", Int).flag("--heidi-continuation-steps").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.inner_iterations", Inversion, "Inner iterations", "Accelerated-gradient iterations per continuation step", Int).flag("--heidi-inner-iterations").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        // QSM.rs inversion/heidi.rs: (mu_min/mu0)^(1/steps) is 0 or NaN at <= 0, so the TV fill never runs.
        d("inversion.heidi.mu_min", Inversion, "Mu min", "Final (smallest) smoothing parameter μ", Float).above(0.0).flag("--heidi-mu-min").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")).full("Final mu"),
        d("inversion.heidi.tol", Inversion, "HEIDI tolerance", "Relative objective change that ends an inner loop early (the default runs every iteration)", Float).flag("--heidi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--heidi-te").parent("inversion.algorithm")
        .when(|x| x.inv("heidi"))
        .gui("The echo time the field is converted to phase at, in the conversion to HEIDI's units").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.heidi.denoise", Inversion, "Denoise field", "Anisotropic-diffusion smoothing of the field before its gradients are taken", Bool).flag("--no-heidi-denoise").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.denoise_iterations", Inversion, "Denoise iter", "Anisotropic-diffusion iterations", Int).flag("--heidi-denoise-iterations").parent("inversion.heidi.denoise")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.denoise).full("Denoise iterations"),
        d("inversion.heidi.denoise_time_step", Inversion, "Time step", "Anisotropic-diffusion time step", Float).flag("--heidi-denoise-time-step").parent("inversion.heidi.denoise")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.denoise),
        d("inversion.heidi.denoise_conductance", Inversion, "Conductance", "Anisotropic-diffusion conductance", Float).flag("--heidi-denoise-conductance").parent("inversion.heidi.denoise")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.denoise),
        d("inversion.qsmart.inversion", Inversion, "Inner inversion", "", Enum(vec![
            o("ilsqr", "ilsqr", "iLSQR", "iLSQR (default) — QSMART's original inner inversion"),
            o("rts", "rts", "RTS", "Rapid Two-Step (RTS)"),
            o("tv", "tv", "TV-ADMM", "Total Variation ADMM (TV)"),
            o("tkd", "tkd", "TKD", "Truncated K-space Division (TKD)"),
            o("tsvd", "tsvd", "TSVD", "Truncated SVD (TSVD)"),
            o("tikhonov", "tikhonov", "Tikhonov", "Tikhonov L2 regularization"),
            o("nltv", "nltv", "NLTV", "Nonlinear Total Variation (NLTV)"),
            o("medi", "medi", "MEDI", "Morphology Enabled Dipole Inversion (MEDI)"),
        ])).flag("--qsmart-inversion").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart"))
        .gui("The dipole inversion QSMART runs in each of its two stages"),
        // QSM.rs inversion/ilsqr.rs: 0 iterations give an all-zero map.
        d("inversion.qsmart.ilsqr_max_iter", Inversion, "iLSQR max iter", "Maximum iLSQR iterations per stage (tolerance: iLSQR's)", Int).min(1.0).flag("--qsmart-ilsqr-max-iter").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")).full("iLSQR maximum iterations"),
        // QSM.rs IlsqrParams::validate (pipeline/qsmart.rs): pad must be >= 0.
        d("inversion.qsmart.ilsqr_pad_mm", Inversion, "iLSQR pad (mm)", "Zero-padding of the inner iLSQR solves, mm per side (default: 6)", Float).min(0.0).units("mm").flag("--qsmart-ilsqr-pad-mm").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")).full("iLSQR padding")
        .gui("Zero-padding of the inner iLSQR solves, per side"),
        d("inversion.qsmart.sdf_sigma1_stage1", Inversion, "SDF sigma1 (s1)", "Stage 1 SDF kernel sigma 1 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma1-stage1").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 1, stage 1")
        .gui("Stage 1 SDF kernel sigma 1"),
        d("inversion.qsmart.sdf_sigma2_stage1", Inversion, "SDF sigma2 (s1)", "Stage 1 SDF kernel sigma 2 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma2-stage1").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 2, stage 1")
        .gui("Stage 1 SDF kernel sigma 2"),
        d("inversion.qsmart.sdf_sigma1_stage2", Inversion, "SDF sigma1 (s2)", "Stage 2 SDF kernel sigma 1 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma1-stage2").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 1, stage 2")
        .gui("Stage 2 SDF kernel sigma 1"),
        d("inversion.qsmart.sdf_sigma2_stage2", Inversion, "SDF sigma2 (s2)", "Stage 2 SDF kernel sigma 2 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma2-stage2").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 2, stage 2")
        .gui("Stage 2 SDF kernel sigma 2"),
        d("inversion.qsmart.sdf_lower_lim", Inversion, "SDF lower lim", "SDF proximity lower limit", Float).flag("--qsmart-sdf-lower-lim").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF lower limit"),
        d("inversion.qsmart.sdf_curv_constant", Inversion, "SDF curv const", "SDF curvature constant", Float).flag("--qsmart-sdf-curv-constant").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF curvature constant"),
        d("inversion.qsmart.vessel_magnitude", Inversion, "Vessel magnitude", "", Enum(vec![
            o("mean-n4", "mean-n4", "Mean + N4", "Mean over echoes, N4 bias-corrected, as QSMART prepares it (default)"),
            o("combined", "combined", "Combined", "The combined magnitude: root sum of squares over echoes, inhomogeneity-corrected when that is on"),
        ])).flag("--qsmart-vessel-magnitude").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Vessel detection magnitude")
        .gui("The magnitude vessels are detected on"),
        d("inversion.qsmart.vessel_size_units", Inversion, "Vessel size units", "", Enum(vec![
            o("voxels", "voxels", "Voxels", "Vessel radius and scales in voxels, as QSMART gives them (default)"),
            o("mm", "mm", "mm", "Vessel radius and scales in mm, converted to voxels with the mean voxel size"),
        ])).flag("--qsmart-vessel-units").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart"))
        .gui("The units of the vasculature sphere radius and the Frangi scales"),
        d("inversion.qsmart.vasc_sphere_radius", Inversion, "Vasc radius", "Bottom-hat sphere radius for vasculature detection, in the vessel size units", Int).flag("--qsmart-vasc-sphere-radius").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Vasculature sphere radius"),
        d("inversion.qsmart.frangi_scale_min", Inversion, "Frangi min", "Frangi minimum vessel radius, in the vessel size units", Float).flag("--qsmart-frangi-scale-min").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Frangi minimum scale"),
        d("inversion.qsmart.frangi_scale_max", Inversion, "Frangi max", "Frangi maximum vessel radius, in the vessel size units", Float).flag("--qsmart-frangi-scale-max").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Frangi maximum scale"),
        d("inversion.qsmart.frangi_scale_ratio", Inversion, "Frangi step", "Frangi scale step, in the vessel size units", Float).above(0.0).flag("--qsmart-frangi-scale-ratio").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Frangi scale step"),
        d("inversion.qsmart.frangi_c", Inversion, "Frangi C", "Frangi C noise threshold", Float).flag("--qsmart-frangi-c").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.ndi.tau", Inversion, "Tau", "Gradient-descent step size", Float).flag("--ndi-tau").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.ndi.alpha", Inversion, "Alpha", "L2 regularization weight", Float).flag("--ndi-alpha").parent("inversion.algorithm")
        .when(|x| x.inv("ndi"))
        .gui("L2 regularisation weight"),
        // QSM.rs inversion/ndi.rs: 0 iterations give an all-zero map.
        d("inversion.ndi.max_iter", Inversion, "Max iter", "Number of iterations", Int).min(1.0).flag("--ndi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")).full("Iterations"),
        d("inversion.ndi.auto_stop", Inversion, "Auto stop", "Stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto)", Bool).flag("--ndi-auto-stop").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.ndi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--ndi-te").parent("inversion.algorithm")
        .when(|x| x.inv("ndi"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.fansi.alpha1", Inversion, "Alpha1", "First-order (TV/TGV) L1 penalty weight", Float).flag("--fansi-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.mu1", Inversion, "Mu1", "Gradient-consistency ADMM weight. Blank = 100×alpha1", Float).above(0.0).optional().flag("--fansi-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.mu2", Inversion, "Mu2", "Fidelity-consistency ADMM weight", Float).flag("--fansi-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.alpha0", Inversion, "Alpha0", "Second-order L1 penalty weight (nlTGV only). Blank = 2×alpha1", Float).min(0.0).optional().flag("--fansi-alpha0").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.mu0", Inversion, "Mu0", "Second-order consistency ADMM weight (nlTGV only). Blank = 2×mu1", Float).above(0.0).optional().flag("--fansi-mu0").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        // QSM.rs inversion/fansi.rs: 0 iterations give an all-zero map.
        d("inversion.fansi.max_iter", Inversion, "Max iter", "Number of outer ADMM iterations", Int).min(1.0).flag("--fansi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).full("Iterations").context("FANSI"),
        d("inversion.fansi.tol_update", Inversion, "Tol update", "Percent-update stopping tolerance", Float).flag("--fansi-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).full("Update tolerance").context("FANSI"),
        d("inversion.fansi.tol_delta", Inversion, "Tol delta", "Inner Newton convergence tolerance", Float).flag("--fansi-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).full("Newton tolerance").context("FANSI"),
        d("inversion.fansi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--fansi-te").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI")
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.l1qsm.alpha1", Inversion, "Alpha1", "Gradient (TV) L1 penalty weight", Float).flag("--l1qsm-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu1", Inversion, "Mu1", "Gradient-consistency ADMM weight. Blank = 100×alpha1", Float).above(0.0).optional().flag("--l1qsm-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu2", Inversion, "Mu2", "Fidelity-consistency ADMM weight", Float).flag("--l1qsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu3", Inversion, "Mu3", "L1 proximal ADMM weight", Float).flag("--l1qsm-mu3").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.lambda", Inversion, "Lambda", "L1 fidelity strength", Float).flag("--l1qsm-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        // QSM.rs inversion/l1qsm.rs: 0 iterations give an all-zero map.
        d("inversion.l1qsm.max_iter", Inversion, "Max iter", "Number of outer ADMM iterations", Int).min(1.0).flag("--l1qsm-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")).full("Iterations"),
        d("inversion.l1qsm.tol_update", Inversion, "Tol update", "Percent-update stopping tolerance", Float).flag("--l1qsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")).full("Update tolerance"),
        d("inversion.l1qsm.tol_delta", Inversion, "Tol delta", "Inner Newton convergence tolerance", Float).flag("--l1qsm-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")).full("Newton tolerance"),
        d("inversion.l1qsm.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--l1qsm-te").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.whqsm.alpha1", Inversion, "Alpha1", "TV regularization weight", Float).flag("--whqsm-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm"))
        .gui("TV regularisation weight"),
        d("inversion.whqsm.mu1", Inversion, "Mu1", "ADMM penalty for TV splitting", Float).flag("--whqsm-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.mu2", Inversion, "Mu2", "ADMM penalty for data-fidelity splitting", Float).flag("--whqsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.beta", Inversion, "Beta", "Weak-harmonic ROI penalty", Float).flag("--whqsm-beta").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.muh", Inversion, "Muh", "ADMM penalty for harmonic-field splitting", Float).flag("--whqsm-muh").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Mu H"),
        d("inversion.whqsm.max_iter", Inversion, "Max iter", "Maximum outer iterations", Int).flag("--whqsm-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Maximum iterations"),
        d("inversion.whqsm.tol_update", Inversion, "Tol update", "Percent-update stopping tolerance", Float).flag("--whqsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Update tolerance"),
        d("inversion.whqsm.tol_delta", Inversion, "Tol delta", "Inner Newton stopping tolerance", Float).flag("--whqsm-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Newton tolerance"),
        d("inversion.whqsm.weight", Inversion, "Data weight", "", Enum(vec![
            o("flat", "flat", "Flat", "1 in the mask, no magnitude (default)"),
            o("max-normalised", "max-normalised", "Max-normalised", "The magnitude over its maximum in the mask; needs magnitude images"),
            o("in-vivo", "in-vivo", "In vivo", "min(2 × magnitude / maximum, 1) in the mask, as NDI weights; needs magnitude images"),
        ])).flag("--whqsm-weight").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm"))
        .gui("How the voxels of the field are weighted in the data term"),
        d("inversion.whqsm.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--whqsm-te").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        // QSM.rs inversion/hdqsm.rs: sqrt(alpha_l2) is NaN below 0.
        d("inversion.hdqsm.alpha_l2", Inversion, "Alpha L2", "L2-stage TV weight", Float).min(0.0).flag("--hdqsm-alpha-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        // QSM.rs inversion/hdqsm.rs: sqrt(mu1_l2) is NaN below 0 and alpha/mu1 infinite at 0.
        d("inversion.hdqsm.mu1_l2", Inversion, "Mu1 L2", "L2-stage gradient-consistency ADMM weight. Blank = 100×alpha L2", Float).above(0.0).optional().flag("--hdqsm-mu1-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.mu2", Inversion, "Mu2", "Fidelity consistency weight", Float).flag("--hdqsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.max_iter_l1", Inversion, "Max iter L1", "Stage-1 (L1) iterations", Int).flag("--hdqsm-max-iter-l1").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")).full("L1 iterations"),
        d("inversion.hdqsm.max_iter_l2", Inversion, "Max iter L2", "Stage-2 (L2) iterations", Int).flag("--hdqsm-max-iter-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")).full("L2 iterations"),
        d("inversion.hdqsm.tol_update", Inversion, "Tol update", "Stage-2 percent-update stopping tolerance", Float).flag("--hdqsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")).full("Update tolerance"),
        // QSM.rs utils/wavelet.rs: only the SUPPORTED_ORDERS (db1–db4) exist.
        d("inversion.amp_pe.wave_order", Inversion, "Wave order", "Daubechies wavelet order (1=db1, 2=db2)", Int).min(*WAVELET_ORDERS.start() as f64).max(*WAVELET_ORDERS.end() as f64).flag("--amp-pe-wave-order").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Wavelet order"),
        // QSM.rs inversion/amp_pe.rs: 1 << nlevel overflows at 64.
        d("inversion.amp_pe.nlevel", Inversion, "N level", "Wavelet decomposition levels", Int).max(qsm_core::inversion::amp_pe::MAX_NLEVEL as f64).flag("--amp-pe-nlevel").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Wavelet levels"),
        d("inversion.amp_pe.wave_pec", Inversion, "Wave pec", "Morphology-mask energy retention fraction (0.0-1.0)", Float).flag("--amp-pe-wave-pec").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Energy retention"),
        d("inversion.amp_pe.simulated_te", Inversion, "Simulated TE", "Simulated echo time (s) used to turn the field into phase", Float).units("s").flag("--amp-pe-simulated-te").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe"))
        .gui("Simulated echo time used to turn the field into phase"),
        // QSM.rs inversion/amp_pe.rs: 0 iterations give an all-zero map.
        d("inversion.amp_pe.max_linearization_ite", Inversion, "Max linearization ite", "Linearization iterations per stage", Int).min(1.0).flag("--amp-pe-max-linearization-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Linearisation iterations")
        .gui("Linearisation iterations per stage"),
        d("inversion.amp_pe.tikhonov_beta", Inversion, "Tikhonov beta", "L2-seed Tikhonov weight", Float).flag("--amp-pe-tikhonov-beta").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.damp_rate_sig", Inversion, "Damp rate (signal)", "GAMP signal-update damping rate", Float).flag("--amp-pe-damp-rate-sig").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Signal damping rate"),
        d("inversion.amp_pe.damp_rate_par", Inversion, "Damp rate (params)", "Parameter-estimation learning rate (kappa)", Float).flag("--amp-pe-damp-rate-par").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Parameter damping rate"),
        d("inversion.amp_pe.max_pe_spar_ite", Inversion, "Sparse recon ite", "Inner sparse-reconstruction iterations", Int).flag("--amp-pe-max-pe-spar-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Sparse reconstruction iterations"),
        d("inversion.amp_pe.max_pe_est_ite", Inversion, "Param est ite", "Inner parameter-estimation iterations", Int).flag("--amp-pe-max-pe-est-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Parameter estimation iterations"),
        d("inversion.amp_pe.cvg_thd", Inversion, "Convergence", "GAMP inner convergence threshold", Float).flag("--amp-pe-cvg-thd").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Convergence threshold"),
        d("inversion.tile_size", Inversion, "Tiled inference", "Run the network on overlapping tiles: bounded memory, approximate. Off = whole volume", Int)
        .value("Tile size", "Overlap-tiling core size in voxels").units("voxels").off(Value::Null).flag("--tile-size").parent("inversion.algorithm")
        .when(|x| x.tileable).gui_value("Tile size", "Core size of each overlapping tile"),
        d("inversion.tile_halo", Inversion, "Tile halo", "Overlap-tiling context margin in voxels (blank = 8)", Int).units("voxels").optional().flag("--tile-halo").parent("inversion.tile_size")
        .when(|x| x.tileable && x.c.inversion.tile_size.is_some())
        .gui("Context margin around each tile (empty = 8)"),
        d("multi_orientation.algorithm", MultiOrientation, "Combine as", "", Enum(vec![
            o("cosmos", "cosmos", "COSMOS", "COSMOS — scalar susceptibility from 2+ orientations, co-registered if needed, closed form in k-space"),
            o("sti", "sti", "STI", "STI — rank-2 susceptibility tensor; needs 6+ orientations that are not coplanar"),
        ])).basic().flag("--multi-orientation-algorithm").parent("multi_orientation.group_by").full("Combine as")
        .gui("How the orientations of a group are combined into one map"),
        d("multi_orientation.lambda", MultiOrientation, "Lambda", "Regularization (blank = 0, plain least squares). Raise for 2 orientations or single-axis rotations", Float).flag("--multi-orientation-lambda").parent("multi_orientation.group_by")
        .gui("Regularisation weight; 0 is plain least squares. Raise it for 2 orientations or single-axis rotations"),
        d("multi_orientation.magnitude_weighting", MultiOrientation, "COSMOS magnitude weighting", "COSMOS groups: weight each orientation by its combined magnitude (conjugate-gradient solve). Off = closed-form least squares", Bool).flag("--cosmos-magnitude-weighting").parent("multi_orientation.algorithm")
        .when(|x| x.cosmos),
        // QSMxT MultiOrientationConfig::validate: tolerance must be > 0.
        d("multi_orientation.cosmos_tol", MultiOrientation, "CG tolerance", "Conjugate-gradient tolerance of weighted COSMOS", Float).above(0.0).flag("--cosmos-tol").parent("multi_orientation.magnitude_weighting")
        .when(|x| x.cosmos && x.c.multi_orientation.magnitude_weighting),
        // QSMxT MultiOrientationConfig::validate: at least one iteration.
        d("multi_orientation.cosmos_max_iter", MultiOrientation, "CG max iterations", "Conjugate-gradient iteration cap of weighted COSMOS", Int).min(1.0).flag("--cosmos-max-iter").parent("multi_orientation.magnitude_weighting")
        .when(|x| x.cosmos && x.c.multi_orientation.magnitude_weighting).full("CG maximum iterations"),
        d("multi_orientation.register", MultiOrientation, "Co-register", "Co-register orientations that need it, with the registration settings below. Off = refuse such groups", Bool).flag("--no-orientation-registration").parent("multi_orientation.group_by"),
        d("multi_orientation.force", MultiOrientation, "Force", "Reconstruct even when the B0 directions look degenerate (wrong metadata gives a plausible wrong map)", Bool).flag("--multi-orientation-force").parent("multi_orientation.group_by").full("Force reconstruction"),
        d("qsm.reference", Reference, "QSM reference", "", Enum(vec![
            o("mean", "mean", "Mean", "Subtract the mean susceptibility over the voxels the map is defined on (recommended)"),
            o("none", "none", "None", "No referencing (raw susceptibility values)"),
            o("region", "region", "Region", "Reference χ to the mean of a SynthSeg region (Reference region). Enables SynthSeg"),
        ])).basic().flag("--qsm-reference")
        .gui("What the susceptibility map is referenced to: the value that reads as zero"),
        d("qsm.reference_region", Reference, "Reference region", "The SynthSeg region whose mean susceptibility is zero. Enables SynthSeg", Regions).basic().optional().flag("--qsm-reference").parent("qsm.reference")
        .when(|x| x.c.qsm.reference == QsmReference::Region),
        d("pipeline.do_chi_separation", Separation, "Compute χ-separation", "Split QSM into paramagnetic (χ+, iron) and diamagnetic (χ−, myelin) maps", Bool).basic().flag("--do-chisep"),
        d("separation.algorithm", Separation, "Method", "", Enum(vec![
            o("r2star-qsm", "r2star-qsm", "R2*-QSM", "R2*-QSM closed-form (Dimov 2022) — QSM + R2*, GRE-only"),
            o("decompose", "decompose", "DECOMPOSE-QSM", "DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude, GRE-only"),
            o("chi-sep-ilsqr", "chi-sep-ilsqr", "χ-separation (iLSQR)", "χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM"),
            o("chi-sep-medi", "chi-sep-medi", "χ-separation (MEDI)", "χ-separation MEDI — local field + R2' + magnitude"),
            o("wavesep", "wavesep", "WaveSep", "WaveSep wavelet-L1 (Fang 2023) — QSM + R2'"),
            o("hc-chisep", "hc-chisep", "Hollow-cylinder χ-separation", "Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude"),
            o("susep-net", "susep-net", "SUSEP-Net", "SUSEP-Net deep-learning separation — QSM + R2' + local field (weights auto-downloaded)"),
            o("chi-sepnet", "chi-sepnet", "χ-sepnet", "χ-sepnet deep-learning separation — QSM + R2' + local field (weights auto-downloaded)"),
        ])).basic().flag("--chisep").parent("pipeline.do_chi_separation")
        .when(|x| x.c.pipeline.do_chi_separation).full("χ-separation method")
        .gui("The χ-separation method, and the maps it reads"),
        d("separation.r2star_qsm.r_const_3t", Separation, "Relaxometric const @3T", "Hz/ppm at 3T (scales with B0)", Float).units("Hz/ppm").flag("--r2star-qsm-r-const-3t").parent("separation.algorithm")
        .when(|x| x.sep("r2star-qsm")).full("Relaxometric constant at 3 T")
        .gui("Relaxometric constant at 3 T (scales with B0)"),
        d("separation.decompose.n_inner", Separation, "Inner passes", "Alternating 3-stage fit passes per voxel", Int).flag("--decompose-n-inner").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Inner passes"),
        // QSM.rs separation/decompose.rs: a negative bound makes lb > ub and f64::clamp panics.
        d("separation.decompose.chi_bound", Separation, "Chi bound (ppm)", "Upper bound on |χ| in the fit", Float).min(0.0).units("ppm").flag("--decompose-chi-bound").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("χ bound"),
        d("separation.decompose.max_lm_iter", Separation, "Max LM iter", "Levenberg–Marquardt max iterations", Int).flag("--decompose-max-lm-iter").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Maximum LM iterations")
        .gui("Maximum Levenberg–Marquardt iterations"),
        d("separation.chi_sep_ilsqr.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxometric constant", Float).units("Hz/ppm").flag("--chi-sep-ilsqr-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Paramagnetic constant (Dr+)"),
        d("separation.chi_sep_ilsqr.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxometric constant", Float).units("Hz/ppm").flag("--chi-sep-ilsqr-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Diamagnetic constant (Dr−)"),
        d("separation.chi_sep_ilsqr.lambda1", Separation, "Lambda1", "L1 edge-masked TV weight", Float).flag("--chi-sep-ilsqr-lambda1").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.percentage", Separation, "Percentage", "Edge-mask keep fraction (0-1)", Float).flag("--chi-sep-ilsqr-percentage").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.r2p_min", Separation, "R2' min (Hz)", "R2' reliability window lower", Float).units("Hz").flag("--chi-sep-ilsqr-r2p-min").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("R2' minimum")
        .gui("Lower end of the R2' reliability window"),
        d("separation.chi_sep_ilsqr.r2p_max", Separation, "R2' max (Hz)", "R2' reliability window upper", Float).units("Hz").flag("--chi-sep-ilsqr-r2p-max").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("R2' maximum")
        .gui("Upper end of the R2' reliability window"),
        d("separation.chi_sep_ilsqr.max_iter", Separation, "Max iter", "Outer Gauss-Newton iterations", Int).flag("--chi-sep-ilsqr-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Maximum iterations"),
        d("separation.chi_sep_ilsqr.tol", Separation, "Tolerance", "Outer relative-change tolerance", Float).flag("--chi-sep-ilsqr-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.cg_max_iter", Separation, "CG max iter", "Inner CG max iterations", Int).flag("--chi-sep-ilsqr-cg-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("CG maximum iterations"),
        d("separation.chi_sep_ilsqr.cg_tol", Separation, "CG tolerance", "Inner CG relative tolerance", Float).flag("--chi-sep-ilsqr-cg-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        // The QSM χ-sep iLSQR is initialised with, reconstructed unless one is supplied. QSM.rs
        // inversion/ilsqr.rs: 0 iterations give an all-zero map; IlsqrParams::validate: pad >= 0.
        d("separation.chi_sep_ilsqr.qsm_ilsqr.max_iter", Separation, "QSM max iter", "Maximum iterations of the iLSQR that reconstructs the QSM separated", Int).min(1.0).flag("--chi-sep-ilsqr-qsm-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr") && x.c.separation.custom_qsm_tool.is_none()).full("QSM maximum iterations"),
        d("separation.chi_sep_ilsqr.qsm_ilsqr.tol", Separation, "QSM tolerance", "Tolerance of the iLSQR that reconstructs the QSM separated", Float).flag("--chi-sep-ilsqr-qsm-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr") && x.c.separation.custom_qsm_tool.is_none()),
        d("separation.chi_sep_ilsqr.qsm_ilsqr.pad_mm", Separation, "QSM pad (mm)", "Zero-padding per side of the iLSQR that reconstructs the QSM separated", Float).min(0.0).units("mm").flag("--chi-sep-ilsqr-qsm-pad-mm").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr") && x.c.separation.custom_qsm_tool.is_none()).full("QSM padding"),
        d("separation.chi_sep_medi.lambda_para", Separation, "Lambda para", "Paramagnetic L1 weight", Float).flag("--chi-sep-medi-lambda-para").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Paramagnetic lambda"),
        d("separation.chi_sep_medi.lambda_dia", Separation, "Lambda dia", "Diamagnetic L1 weight", Float).flag("--chi-sep-medi-lambda-dia").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Diamagnetic lambda"),
        // QSM.rs separation/chi_sep_medi.rs: lambda_cpl weights both data terms; at 0 they drop out
        // and the solution stays zero, and below 0 the problem is no longer convex.
        d("separation.chi_sep_medi.lambda_cpl", Separation, "Lambda cpl", "Field/R2' coupling weight", Float).above(0.0).flag("--chi-sep-medi-lambda-cpl").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Coupling lambda"),
        d("separation.chi_sep_medi.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxivity", Float).units("Hz/ppm").flag("--chi-sep-medi-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Paramagnetic constant (Dr+)"),
        d("separation.chi_sep_medi.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxivity", Float).units("Hz/ppm").flag("--chi-sep-medi-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Diamagnetic constant (Dr−)"),
        d("separation.chi_sep_medi.percentage", Separation, "Percentage", "Edge-mask percentage (0-1)", Float).flag("--chi-sep-medi-percentage").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.cg_tol", Separation, "CG tolerance", "Inner CG tolerance", Float).flag("--chi-sep-medi-cg-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.cg_max_iter", Separation, "CG max iter", "Inner CG max iterations", Int).flag("--chi-sep-medi-cg-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("CG maximum iterations"),
        d("separation.chi_sep_medi.max_iter", Separation, "Max iter", "Outer Gauss-Newton iterations", Int).flag("--chi-sep-medi-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Maximum iterations"),
        d("separation.chi_sep_medi.tol", Separation, "Tolerance", "Outer convergence tolerance", Float).flag("--chi-sep-medi-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.wavesep.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxivity", Float).units("Hz/ppm").flag("--wavesep-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Paramagnetic constant (Dr+)"),
        d("separation.wavesep.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxivity", Float).units("Hz/ppm").flag("--wavesep-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Diamagnetic constant (Dr−)"),
        d("separation.wavesep.alpha", Separation, "Alpha", "Proximal-gradient step size", Float).flag("--wavesep-alpha").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.wavesep.lambda", Separation, "Lambda", "Wavelet L1 soft-threshold weight", Float).flag("--wavesep-lambda").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        // QSM.rs utils/wavelet.rs: only the SUPPORTED_ORDERS (db1–db4) exist.
        d("separation.wavesep.wavelet_order", Separation, "Wavelet order", "Daubechies order", Int).min(*WAVELET_ORDERS.start() as f64).max(*WAVELET_ORDERS.end() as f64).flag("--wavesep-wavelet-order").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Wavelet order"),
        d("separation.wavesep.max_iter", Separation, "Max iter", "ISTA max iterations", Int).flag("--wavesep-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Maximum iterations"),
        d("separation.wavesep.tol", Separation, "Tolerance", "Relative-change stop tolerance", Float).flag("--wavesep-tol").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        // QSM.rs separation/hc_chisep.rs: r2prime/dr_pos is inf at 0; below 0 the calibration window is empty.
        d("separation.hc_chisep.dr_pos_3t", Separation, "Dr+ @3T (Hz/ppm)", "Paramagnetic relaxivity at 3T", Float).above(0.0).units("Hz/ppm").flag("--hc-chisep-dr-pos-3t").parent("separation.algorithm")
        .when(|x| x.sep("hc-chisep")).full("Paramagnetic constant at 3 T (Dr+)"),
        d("separation.hc_chisep.bin_hz", Separation, "R2' bin (Hz)", "R2' bin width for anchored grid search", Float).units("Hz").flag("--hc-chisep-bin-hz").parent("separation.algorithm")
        .when(|x| x.sep("hc-chisep")).full("R2' bin width")
        .gui("R2' bin width for the anchored grid search"),
        // QSM.rs ChiSepNetParams::validate: each side a positive multiple of chisepnet::POOL_MULTIPLE
        // (16). The texts' "16" is pinned to it by a test.
        d("separation.chi_sepnet.patch", Separation, "Patch", "Sliding-window patch D H W in voxels, each a multiple of 16 (default 192 192 128)", IntVec3).min(qsm_core::separation::chisepnet::POOL_MULTIPLE as f64).units("voxels").flag("--chi-sepnet-patch").parent("separation.algorithm")
        .when(|x| x.sep("chi-sepnet")).full("χ-sepnet patch")
        .gui("Sliding-window patch D H W, each a multiple of 16"),
        // QSM.rs SusepNetParams::validate: each side a positive multiple of susep_net::POOL_MULTIPLE
        // (8). The texts' "8" is pinned to it by a test.
        d("separation.susep_net.patch", Separation, "Sliding window", "Run the network on patches D H W in voxels, each a multiple of 8, so its memory follows the patch instead of the volume; empty runs the whole volume at once", IntVec3).min(qsm_core::separation::susep_net::POOL_MULTIPLE as f64).units("voxels").optional().flag("--susep-net-patch").parent("separation.algorithm")
        .when(|x| x.sep("susep-net")).full("SUSEP-Net sliding window")
        .gui("Patch size for running SUSEP-Net piece by piece, which needs less memory; empty runs the whole volume at once"),
        d("separation.custom_qsm_tool", Separation, "Custom QSM tool", "Use a Chimap from <bids>/derivatives/<tool>/ instead of recomputing QSM (χ-separation, statistics, SMWI); blank = compute, * = any tool", Text).optional().flag("--use-custom-qsm").also(&[SwiSmwi, Segmentation])
        .supplied_file(Item::SuppliedChi, "A susceptibility (χ) map to use instead of computing QSM, for χ-separation, the statistics and SMWI")
        .when(|x| x.reads_qsm).full("Custom QSM")
        .gui("Use a χ map (Chimap) from <bids>/derivatives/<tool>/ instead of computing QSM, for χ-separation, the statistics and SMWI: * for any tool, or a tool name; empty to compute it"),
        d("pipeline.do_swi", SwiSmwi, "Compute SWI", "Also compute susceptibility-weighted images with CLEAR-SWI, from every echo", Bool).basic().flag("--do-swi"),
        d("swi.unwrap", SwiSmwi, "Unwrapping", "", Enum(vec![
            o("laplacian", "laplacian", "Laplacian", "Each echo unwrapped with the Laplacian method and high-pass filtered, then combined"),
            o("laplacian-slice", "laplacian-slice", "Laplacian per slice", "Laplacian unwrapping slice by slice, for 2D multi-slice acquisitions"),
            o("romeo", "romeo", "ROMEO", "All echoes unwrapped together with ROMEO, combined, then high-pass filtered"),
        ])).flag("--swi-unwrapping").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).full("SWI unwrapping")
        .gui("How the phase is unwrapped and high-pass filtered for SWI").context("SWI"),
        d("swi.scaling", SwiSmwi, "Scaling", "", Enum(vec![
            o("tanh", "tanh", "Tanh", "Phase scaling type for SWI"),
            o("negative-tanh", "negative-tanh", "Negative tanh", "Phase scaling type for SWI"),
            o("positive", "positive", "Positive", "Phase scaling type for SWI"),
            o("negative", "negative", "Negative", "Phase scaling type for SWI"),
            o("triangular", "triangular", "Triangular", "Phase scaling type for SWI"),
        ])).flag("--swi-scaling").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).full("Phase scaling")
        .gui("How the filtered phase is turned into the mask the magnitude is multiplied by").context("SWI"),
        d("swi.strength", SwiSmwi, "Strength", "Phase scaling strength (higher = stronger phase contrast)", Float).flag("--swi-strength").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).context("SWI"),
        d("swi.hp_sigma", SwiSmwi, "HP sigma (x y z)", "High-pass filter sigma per axis, in voxels (x y z): background phase removal. Set z to 0 for thin axial slices", FloatVec3).units("voxels").flag("--swi-hp-sigma").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).full("High-pass sigma")
        .gui("High-pass filter sigma per axis (x y z): background phase removal. Set z to 0 for thin axial slices").context("SWI"),
        // QSM.rs swi.rs mip_geometry refuses a window of 0 (and one wider than the volume).
        d("swi.mip_window", SwiSmwi, "mIP window", "Minimum intensity projection window size in slices", Int).min(1.0).units("slices").flag("--swi-mip-window").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi)
        .gui("Minimum-intensity-projection window").context("SWI"),
        d("pipeline.do_smwi", SwiSmwi, "Compute SMWI", "Susceptibility map-weighted imaging — weights the magnitude by a mask built from χ, not from filtered phase. Needs QSM", Bool).basic().flag("--do-smwi"),
        d("smwi.threshold_ppm", SwiSmwi, "Threshold", "|χ| in ppm at which the weighting mask reaches zero. The 1 ppm default targets strong sources; brain tissue wants far less", Float).units("ppm").flag("--smwi-threshold").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi)
        .gui("|χ| at which the weighting mask reaches zero. 1 ppm targets strong sources; brain tissue wants far less").context("SMWI"),
        // QSM.rs swi.rs: w^power with w = 0 is inf for a negative power.
        d("smwi.power", SwiSmwi, "Power", "Power the mask is raised to (contrast strength)", Float).min(0.0).flag("--smwi-power").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi).context("SMWI"),
        // QSM.rs swi.rs mip_geometry refuses a window of 0 (and one wider than the volume).
        d("smwi.mip_window", SwiSmwi, "mIP window", "Minimum-intensity-projection window in slices", Int).min(1.0).units("slices").flag("--smwi-mip-window").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi)
        .gui("Minimum-intensity-projection window").context("SMWI"),
        d("pipeline.do_t2starmap", Relaxometry, "Compute T2* map", "Compute T2* relaxation map (requires 3+ echoes with magnitude)", Bool).basic().flag("--do-t2starmap"),
        d("pipeline.do_r2starmap", Relaxometry, "Compute R2* map", "Compute R2* decay rate map (requires 3+ echoes with magnitude)", Bool).basic().flag("--do-r2starmap"),
        d("pipeline.do_r2map", Relaxometry, "Compute R2 map", "R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG. Forced on when chi-separation needs it", Bool).basic().flag("--do-r2map")
        .gui("R2 map from a multi-echo spin-echo (MESE) acquisition via EPG. Forced on when χ-separation needs it"),
        d("separation.custom_r2_tool", Relaxometry, "Custom R2 tool", "Use an R2 map from <bids>/derivatives/<tool>/ instead of computing it from MESE (blank = compute, * = any tool)", Text).optional().flag("--use-custom-r2").parent("pipeline.do_r2map")
        .supplied_file(Item::SuppliedR2, "An R2 map to use instead of computing one from a MESE acquisition")
        .when(|x| x.reads_r2).full("Custom R2")
        .gui("Use an R2 map from <bids>/derivatives/<tool>/ instead of computing it from MESE: * for any tool, or a tool name; empty to compute it"),
        d("pipeline.do_r2primemap", Relaxometry, "Compute R2' map", "R2' = R2* − R2 (Hz); needs GRE magnitude + a MESE acquisition, or R2PRIMEnet. Forced on when chi-separation needs it", Bool).basic().flag("--do-r2primemap")
        .gui("R2' = R2* − R2; needs GRE magnitude and a MESE acquisition, or R2PRIMEnet. Forced on when χ-separation needs it"),
        d("separation.custom_r2prime_tool", Relaxometry, "Custom R2' tool", "Use an R2' map from <bids>/derivatives/<tool>/ instead of computing it (blank = compute, * = any tool)", Text).optional().flag("--use-custom-r2prime").parent("pipeline.do_r2primemap")
        .supplied_file(Item::SuppliedR2prime, "An R2' map to use instead of computing one")
        .when(|x| x.reads_r2prime).full("Custom R2'")
        .gui("Use an R2' map from <bids>/derivatives/<tool>/ instead of computing it: * for any tool, or a tool name; empty to compute it"),
        d("separation.r2prime_strategy", Relaxometry, "R2' source", "", Enum(vec![
            o("auto", "auto", "Automatic", "Measure R2' = R2* - R2 from a MESE acquisition when one is present, else predict it from R2* with R2PRIMEnet"),
            o("mese", "mese", "MESE", "Only ever measure R2' from a MESE acquisition - produces no R2' without spin-echo data"),
            o("r2primenet", "r2primenet", "R2PRIMEnet", "Always predict R2' from R2* with R2PRIMEnet, even when a MESE acquisition is available (an estimate, not a measurement)"),
        ])).flag("--r2prime-strategy").parent("pipeline.do_r2primemap")
        .when(|x| x.computes_r2prime)
        .gui("How R2' is obtained when no R2' map is supplied"),
        d("separation.r2primenet.ignore_field_mismatch", Relaxometry, "R2PRIMEnet off 3 T", "Run R2PRIMEnet on non-3 T data (trained at 3 T only; the R2' is uncalibrated). Off: auto skips R2', r2primenet errors", Bool).flag("--r2primenet-ignore-field-mismatch").parent("separation.r2prime_strategy")
        .when(|x| x.computes_r2prime && x.c.separation.r2prime_strategy != R2PrimeStrategy::Mese).full("R2PRIMEnet away from 3 T"),
        // QSMxT R2primenetConfig::validate: each side a positive multiple of QSM.rs's
        // relaxometry::POOL_MULTIPLE (16; QSM.rs utils/r2primenet.rs asserts it). The texts' "16" is pinned to it by a test.
        d("separation.r2primenet.patch", Relaxometry, "R2PRIMEnet patch", "Sliding-window patch D H W in voxels, each a multiple of 16 (default 192 192 128)", IntVec3).min(qsm_core::relaxometry::POOL_MULTIPLE as f64).units("voxels").flag("--r2primenet-patch").parent("separation.r2prime_strategy")
        .when(|x| x.computes_r2prime && x.c.separation.r2prime_strategy != R2PrimeStrategy::Mese)
        .gui("Sliding-window patch D H W, each a multiple of 16"),
        d("pipeline.do_segmentation", Segmentation, "Segmentation", "Whole-brain parcellation: SynthSeg straight off the GRE magnitude, or one supplied in derivatives. Forced on by the statistics and a region reference", Bool).basic().flag("--do-segmentation"),
        d("segmentation.custom_dseg_tool", Segmentation, "Derivatives tool", "The dseg from <bids>/derivatives/<tool>/: * = any tool, or a tool name; SynthSeg runs where none is found", Text).optional().flag("--use-custom-dseg").parent("pipeline.do_segmentation")
        .supplied_file(Item::SuppliedSegmentation, "A segmentation to use instead of running SynthSeg")
        .when(|x| x.segmentation_on).full("Custom segmentation")
        .gui("Use a segmentation (dseg) from <bids>/derivatives/<tool>/ instead of running SynthSeg: * for any tool, or a tool name. SynthSeg runs where none is found"),
        d("segmentation.version", Segmentation, "SynthSeg version", "", Enum(vec![
            o("v1", "v1", "SynthSeg 1", "v1: 32 labels, weights ship with SynthSeg. v2: 33 labels (adds CSF); its weights are not published yet, so a run with v2 is refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR"),
            o("v2", "v2", "SynthSeg 2", "v1: 32 labels, weights ship with SynthSeg. v2: 33 labels (adds CSF); its weights are not published yet, so a run with v2 is refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR"),
        ])).flag("--synthseg-version").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .gui("Which SynthSeg model segments the brain"),
        // QSM.rs segment/synthseg.rs: a crop of 0 gives the network an empty input.
        d("segmentation.crop", Segmentation, "Crop", "Cubic centre-crop in voxels at 1 mm before inference; empty processes the whole volume", Int).min(1.0).units("voxels").optional().flag("--synthseg-crop").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .gui("Cubic centre crop at 1 mm before inference; empty processes the whole volume"),
        // QSM.rs segment/synthseg.rs refuses a patch under 64 and rounds it up to a multiple of 32.
        d("segmentation.patch", Segmentation, "Sliding window", "Run the network on half-overlapping cubes of this many voxels at 1 mm (rounded up to a multiple of 32), so its memory follows the patch instead of the volume; empty runs the whole volume at once", Int).min(64.0).units("voxels").optional().flag("--synthseg-patch").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .gui("Patch size for running SynthSeg piece by piece, which needs less memory; empty runs the whole volume at once"),
        d("segmentation.flip_averaging", Segmentation, "Flip averaging", "Average with a left-right flipped second pass (SynthSeg's default). Doubles inference time", Bool).flag("--no-synthseg-flip-averaging").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()),
        d("segmentation.topology_cleanup", Segmentation, "Topology cleanup", "Reset each topological class to its largest connected component", Bool).flag("--no-synthseg-topology-cleanup").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()),
        // QSM.rs segment/synthseg.rs refuses a negative sigma.
        d("segmentation.sigma_smoothing", Segmentation, "Posterior smoothing", "Gaussian blur on the posteriors before the labels are taken", Float).min(0.0)
        .value("Sigma (voxels)", "Gaussian sigma of the posterior blur, in voxels").units("voxels").off(json!(0.0)).flag("--synthseg-sigma").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()).gui_value("Sigma", "Gaussian sigma of the posterior blur"),
        d("pipeline.do_analysis", Segmentation, "Per-structure statistics", "Summarise every map the run makes (χ, χ+/χ−, T2*, R2*, R2, R2') per structure into a TSV: mean, SD, median, min, max, 5th/95th percentiles. Implies segmentation; implies QSM only if no other map is enabled", Bool).basic().flag("--do-analysis").full("Per-structure statistics"),
        // DICOM export is not in the browser yet (astewartau/qsmbly#164).
        d("pipeline.export_dicom", Export, "Export DICOM", "Also write final maps as DICOM series into each subject's extra_files/ folder", Bool).basic().flag("--export-dicom").unavailable_in_browser(),
    ]
}

/// N4 inhomogeneity correction is on: its settings apply.
fn n4_on(x: &super::view::Ctx) -> bool {
    x.c.masking.inhomogeneity_correction && x.c.masking.inhomogeneity_method == InhomogeneityMethod::N4
}
