//! The settings table: one entry per user-facing [`PipelineConfig`](crate::PipelineConfig) setting,
//! in the TUI's order, with its labels and help.
//!
//! Labels and help are the TUI's own wording (`src/tui/app.rs`); the TUI's tests check that the
//! two agree, so edit both together.

use serde_json::{json, Value};

use super::{d, o, Def, Section::*, Ty::*};
use crate::enums::*;

pub(super) fn table() -> Vec<Def> {
    vec![
        d("multi_orientation.group_by", Input, "Orientations", "Runs that are the same object at a different angle: an entity (acq, run, rec, inv), a glob (*acq-dir*), or re:<regex>. Empty = off. How they combine: QSM → Multi-orientation", Text).basic().off(json!("")).flag("--orientation-group"),
        d("pipeline.axial_resampling", Geometry, "Axial Resampling", "Resample oblique acquisitions onto an axial grid when they are tilted more than the threshold. Off: reconstruct on the acquired grid with the true B0 direction", Bool).basic().flag("--axial-resampling"),
        d("pipeline.obliquity_threshold", Geometry, "Obliquity Threshold (°)", "Resample only data more oblique than this, in degrees", Float).units("deg").min(0.0).flag("--obliquity-threshold").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling),
        d("pipeline.output_space", Geometry, "Output Space", "", Enum(vec![
            o("acquired", "acquired", "Write derivatives back on the acquired grid when the run was resampled to axial (default)"),
            o("working", "working", "Leave derivatives on the resampled (axial) working grid, avoiding a second interpolation"),
        ])).flag("--output-space").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling || x.axial_forced),
        d("masking.inhomogeneity_correction", Geometry, "Inhomog. Correction", "Apply B1 field correction to magnitude (improves masking, ROMEO weights, MEDI edges, SWI)", Bool).basic().flag("--no-inhomogeneity-correction"),
        d("homogeneity.sigma_mm", Geometry, "Sigma (mm)", "Smoothing sigma of the bias-field estimate, in mm", Float).units("mm").min(0.0).flag("--homogeneity-sigma-mm").parent("masking.inhomogeneity_correction")
        .when(|x| x.c.masking.inhomogeneity_correction),
        d("homogeneity.nbox", Geometry, "Box Passes", "Box-filter passes approximating the Gaussian", Int).min(0.0).flag("--homogeneity-nbox").parent("masking.inhomogeneity_correction")
        .when(|x| x.c.masking.inhomogeneity_correction),
        d("pipeline.ignore_slice_gap", Geometry, "Ignore Slice Gap", "Reconstruct even if the sidecar's SliceThickness is thinner than the slice pitch (gapped 2D data gives a wrong map)", Bool).flag("--ignore-slice-gap"),
        d("pipeline.crop_to_mask", Geometry, "Crop to Mask", "Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid). Off by default", Bool).flag("--crop-to-mask")
        .when(|x| x.qsm),
        d("pipeline.crop_margin_mm", Geometry, "Crop Margin (mm)", "Margin around the mask; below ~32 mm results start to differ, and it must clear the largest SMV kernel", Float).units("mm").min(0.0).flag("--crop-margin-mm").parent("pipeline.crop_to_mask")
        .when(|x| x.qsm && x.c.pipeline.crop_to_mask),
        d("pipeline.fft_padding", Geometry, "FFT Padding", "Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly). Off by default", Bool).flag("--fft-padding")
        .when(|x| x.qsm),
        d("field_mapping.coil_combination_sigma", Geometry, "Coil Comb. Sigma", "MCPC-3D-S smoothing of per-coil phase offsets, voxels (x y z); only for uncombined multi-coil data", FloatVec3).units("voxels").min(0.0).flag("--coil-combination-sigma")
        .when(|x| x.multi_coil),
        d("masking.sections", Masking, "Mask", "The mask recipe: one or more sections, each an input image, a generator and its refinements, combined with OR or AND", MaskSections).basic(),
        d("masking.combine", Masking, "Combined With", "How the sections are combined: OR keeps a voxel any section keeps, AND only one every section keeps", Enum(vec![
            o("or", "OR", "A voxel is in the mask when any section keeps it"),
            o("and", "AND", "A voxel is in the mask when every section keeps it"),
        ])).parent("masking.sections")
        .when(|x| x.c.masking.sections.len() > 1),
        d("masking.refinements", Masking, "Combined Mask Steps", "Refinement steps run on the combined mask, after the sections are combined", MaskOps).parent("masking.sections")
        .when(|x| x.c.masking.sections.len() > 1),
        d("masking.custom_mask_tool", Masking, "Use custom masks", "BYO mask from BIDS derivatives: empty=off, * =any tool, or a tool name (e.g. bet); falls back if absent", Text).optional().flag("--use-custom-masks").supplied_file(),
        d("masking.two_pass", Masking, "Two-pass Artefact Reduction", "Reconstruct again on a mask whose holes are left unfilled and keep that result where it is defined — keeps streaking from strong sources out of the rest of the brain, at roughly double the reconstruction time (doi:10.1002/mrm.29048)", Bool).basic().flag("--two-pass"),
        d("bet.smoothness", Masking, "Smoothness", "BET surface smoothness factor (shared by every BET step)", Float).min(0.0).flag("--bet-smoothness").parent("masking.sections")
        .when(|x| x.uses_bet),
        d("bet.gradient_threshold", Masking, "Gradient Threshold", "BET gradient threshold, -1 to 1 (shared by every BET step)", Float).min(-1.0).max(1.0).flag("--bet-gradient-threshold").parent("masking.sections")
        .when(|x| x.uses_bet),
        d("bet.iterations", Masking, "Iterations", "BET surface evolution iterations (shared by every BET step)", Int).min(0.0).flag("--bet-iterations").parent("masking.sections")
        .when(|x| x.uses_bet),
        d("bet.subdivisions", Masking, "Subdivisions", "BET icosphere subdivision level (shared by every BET step)", Int).min(0.0).flag("--bet-subdivisions").parent("masking.sections")
        .when(|x| x.uses_bet),
        d("masking.two_pass_sections", Masking, "Reliable Mask", "The mask the second pass reconstructs on: its holes are left unfilled. Empty = the main mask without its hole filling", MaskSections).optional().parent("masking.two_pass")
        .when(|x| x.c.masking.two_pass),
        d("motion_correction.enabled", Motion, "Motion Correction", "Rigidly co-register every echo onto a reference echo before any other stage (multi-echo, needs magnitude). Off by default", Bool).basic().flag("--motion-correction"),
        d("motion_correction.reference", Motion, "Reference", "", EchoReference(vec![
            o("first", "first", "Align every echo onto the first (best SNR; the published procedure) (default)"),
            o("last", "last", "Align every echo onto the last"),
            o("1", "echo", "Align every echo onto a given echo (Reference Echo)"),
        ]))
        .value("Reference Echo", "1-based echo number the others are aligned onto").flag("--motion-reference").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled),
        d("motion_correction.chain", Motion, "Chain", "Seed each echo's registration from its neighbour's solution", Bool).flag("--no-motion-chain").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled),
        d("motion_correction.chain_levels", Motion, "Chain Levels", "Pyramid levels for a seeded registration (default 3)", Int).min(0.0).flag("--motion-chain-levels").parent("motion_correction.chain")
        .when(|x| x.c.motion_correction.enabled && x.c.motion_correction.chain),
        d("motion_correction.min_ncc", Motion, "Min NCC", "NCC below which a registration is reported as suspect (0.3 for echoes; ~0.9 for repeats)", Float).min(0.0).flag("--motion-min-ncc").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled),
        d("registration.levels", Motion, "Levels", "Pyramid levels: the capture range; do not lower (default 5)", Int).min(0.0).flag("--registration-levels").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("registration.max_samples", Motion, "Max Samples", "Voxels sampled per metric evaluation, per level", Int).min(0.0).flag("--registration-max-samples").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("registration.max_iterations", Motion, "Max Iterations", "Powell cycles per level", Int).min(0.0).flag("--registration-max-iterations").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("registration.max_evaluations", Motion, "Max Evaluations", "Metric evaluations across all levels (hard stop)", Int).min(0.0).flag("--registration-max-evaluations").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("registration.tolerance", Motion, "Tolerance", "Relative cost improvement per cycle that counts as converged", Float).min(0.0).flag("--registration-tolerance").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("registration.rotation_step_deg", Motion, "Rotation Step (deg)", "Initial rotation step at the finest level", Float).units("deg").min(0.0).flag("--registration-rotation-step-deg").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("registration.translation_step_mm", Motion, "Translation Step (mm)", "Initial translation step at the finest level", Float).units("mm").min(0.0).flag("--registration-translation-step-mm").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("registration.min_overlap", Motion, "Min Overlap", "Smallest fraction of sampled voxels that must overlap", Float).min(0.0).max(1.0).flag("--registration-min-overlap").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)),
        d("field_mapping.unwrapping_algorithm", FieldMap, "Unwrapping", "", Enum(vec![
            o("romeo", "romeo", "ROMEO region-growing unwrapping — https://doi.org/10.1002/mrm.28563"),
            o("laplacian", "laplacian", "Laplacian phase unwrapping (FFT-based) — https://doi.org/10.1364/OL.28.001194"),
        ])).basic().flag("--unwrapping-algorithm"),
        d("field_mapping.laplacian_solver", FieldMap, "Solver", "", Enum(vec![
            o("dct", "dct", "Unweighted least squares, Neumann-boundary DCT solve (Ghiglia & Romero 1994; QSM.jl :dct) (default)"),
            o("fft", "fft", "Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume"),
        ])).flag("--laplacian-solver").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian),
        d("field_mapping.laplacian_fft_pad", FieldMap, "FFT Pad (voxels)", "Zero-padding per side: one value for every axis or three (x y z). Default: 64", IntVec3).units("voxels").min(0.0).flag("--laplacian-fft-pad").parent("field_mapping.laplacian_solver")
        .when(|x| x.laplacian && x.c.field_mapping.laplacian_solver == LaplacianSolver::Fft),
        d("field_mapping.phase_offset_removal", FieldMap, "Phase Offset Removal", "Remove spatially-varying phase offset using HIP (Eckstein et al., 2018); multi-echo only", Bool).flag("--phase-offset-removal").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.phase_offset_sigma", FieldMap, "Sigma", "Gaussian smoothing sigma in voxels (x y z)", FloatVec3).units("voxels").min(0.0).flag("--phase-offset-sigma").parent("field_mapping.phase_offset_removal")
        .when(|x| !x.laplacian && x.c.field_mapping.phase_offset_removal),
        d("field_mapping.bipolar_correction", FieldMap, "Bipolar Correction", "Remove the linear phase artefact of bipolar readout gradients (needs at least 3 echoes; fewer: skipped)", Bool).flag("--bipolar-correction").parent("field_mapping.phase_offset_removal")
        .when(|x| !x.laplacian && x.c.field_mapping.phase_offset_removal),
        d("field_mapping.romeo.individual", FieldMap, "Individual Mode", "Unwrap each echo independently (recommended). Off = template-based temporal unwrapping.", Bool).flag("--no-romeo-individual").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        // The config counts echoes from 0; the TUI and --romeo-template from 1.
        d("field_mapping.romeo.template", FieldMap, "Template Echo", "Echo used for spatial unwrapping, counted from 0 (--romeo-template counts from 1)", Int).min(0.0).flag("--romeo-template").parent("field_mapping.romeo.individual")
        .when(|x| !x.laplacian && !x.c.field_mapping.romeo.individual),
        d("field_mapping.romeo.correct_global", FieldMap, "Correct Global", "Correct inter-echo 2π offsets after unwrapping", Bool).flag("--no-romeo-correct-global").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.phase_coherence", FieldMap, "Phase Coherence", "Weight edges by neighbour phase coherence", Bool).flag("--no-romeo-phase-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.phase_gradient_coherence", FieldMap, "Phase Gradient Coherence", "Weight edges by inter-echo phase-gradient consistency (multi-echo)", Bool).flag("--no-romeo-phase-gradient-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.phase_linearity", FieldMap, "Phase Linearity", "Weight edges by second-derivative smoothness", Bool).flag("--no-romeo-phase-linearity").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.mag_coherence", FieldMap, "Mag Coherence", "Weight edges by neighbor magnitude coherence", Bool).flag("--no-romeo-mag-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.mag_weight", FieldMap, "Mag Weight", "Penalise low-signal voxels (off by default)", Bool).flag("--romeo-mag-weight").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.mag_weight2", FieldMap, "Mag Weight 2", "Penalise abnormally high signal / flow artefacts (off by default)", Bool).flag("--romeo-mag-weight2").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.bestpath", FieldMap, "Best-path Weights", "Use Abdul-Rahman best-path weights instead of ROMEO weights", Bool).flag("--romeo-bestpath").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.temporal_uncertain_unwrapping", FieldMap, "Temporal Uncertain", "Re-unwrap voxels whose temporal unwrapping is uncertain", Float)
        .value("Quality Threshold", "Quality threshold (0, 1] below which a voxel is re-unwrapped").min(0.0).max(1.0).off(json!(0.0)).flag("--romeo-temporal-uncertain-unwrapping").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.max_seeds", FieldMap, "Max Seeds", "Maximum number of seed regions (default 1)", Int).min(0.0).flag("--romeo-max-seeds").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.merge_regions", FieldMap, "Merge Regions", "Merge neighboring regions after unwrapping", Bool).flag("--romeo-merge-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.correct_regions", FieldMap, "Correct Regions", "Shift each region's median to nearest 0 by n·2π", Bool).flag("--romeo-correct-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.romeo.wrap_addition", FieldMap, "Wrap Addition", "Extra phase tolerance beyond π for neighbor differences [0,π]", Float).min(0.0).max(std::f64::consts::PI).flag("--romeo-wrap-addition").parent("field_mapping.unwrapping_algorithm")
        .when(|x| !x.laplacian),
        d("field_mapping.b0_estimation", FieldMap, "B0 Estimation", "", Enum(vec![
            o("weighted-avg", "weighted-avg", "Weighted average of phase/TE across echoes (default)"),
            o("linear-fit", "linear-fit", "Magnitude-weighted linear fit of phase vs TE"),
            o("robust-fit", "robust-fit", "Motion-robust linear fit (IRLS): down-weights echoes departing from a straight line, per voxel. Needs 4 echoes (3 without offset)"),
        ])).basic().flag("--b0-estimation"),
        d("field_mapping.b0_weight_type", FieldMap, "Weight Type", "", Enum(vec![
            o("phase-snr", "phase-snr", "mag × TE — optimal for phase SNR (default)"),
            o("phase-var", "phase-var", "mag² × TE² — based on phase variance"),
            o("average", "average", "Uniform weights (unweighted average)"),
            o("tes", "tes", "TE only"),
            o("mag", "mag", "Magnitude only"),
            o("assumed-decay", "assumed-decay", "Fixed TE·exp(−TE/T2*) per echo from an assumed T2* (no T2* map, no magnitude); phase-snr is generally better"),
        ])).flag("--b0-weight-type").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg),
        d("field_mapping.b0_weight_assumed_t2star_ms", FieldMap, "Assumed T2* (ms)", "T2* assumed to compute the fixed echo weights; no T2* map is used (default: 40 ms)", Float).units("ms").min(0.0).flag("--b0-weight-assumed-t2star").parent("field_mapping.b0_weight_type")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg && x.c.field_mapping.b0_weight_type == B0WeightType::AssumedDecay),
        d("field_mapping.linear_fit.estimate_offset", FieldMap, "Estimate Offset", "Fit a phase-offset term in the echo-time fit (robust fit: needs 4 echoes with it, 3 without)", Bool).flag("--linear-fit-estimate-offset").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation != B0Estimation::WeightedAvg),
        d("field_mapping.linear_fit.reliability_threshold_percentile", FieldMap, "Reliability Mask", "Report the fit's reliability mask; does not change the field map", Float)
        .value("Percentile", "Percentile of the fit residual above which a voxel counts as unreliable").min(0.0).max(100.0).off(json!(0.0)).flag("--linear-fit-reliability-threshold").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation != B0Estimation::WeightedAvg),
        d("field_mapping.robust_fit.loss", FieldMap, "Loss", "", Enum(vec![
            o("tukey", "tukey", "Tukey biweight: rejects an echo at a voxel past the tuning radius (default)"),
            o("huber", "huber", "Huber: caps an outlying echo's influence but never rejects it"),
        ])).flag("--robust-fit-loss").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit),
        d("field_mapping.robust_fit.tuning", FieldMap, "Tuning", "Tuning radius in robust-scale units; blank = conventional (Tukey 4.685, Huber 1.345)", Float).min(0.0).optional().flag("--robust-fit-tuning").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit),
        d("field_mapping.robust_fit.iterations", FieldMap, "Iterations", "IRLS passes (0 = report only: the field map is the linear fit's)", Int).min(0.0).flag("--robust-fit-iterations").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit),
        d("field_mapping.robust_fit.downweight_level", FieldMap, "Downweight Level", "Reporting only: a weight below this counts as down-weighted (0, 1]", Float).min(0.0).max(1.0).flag("--robust-fit-downweight-level").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit),
        d("field_mapping.robust_fit.flag_ratio", FieldMap, "Flag Ratio", "Reporting only: flag an echo whose outlier score exceeds this x the best echo's (>= 1)", Float).min(1.0).flag("--robust-fit-flag-ratio").parent("field_mapping.b0_estimation")
        .when(|x| x.c.field_mapping.b0_estimation == B0Estimation::RobustFit),
        d("bg_removal.algorithm", Background, "BG Removal", "", Enum(vec![
            o("vsharp", "vsharp", "Variable-kernel SHARP (V-SHARP) — https://doi.org/10.1002/mrm.23000"),
            o("pdf", "pdf", "Projection onto Dipole Fields (PDF) — https://doi.org/10.1002/nbm.1670"),
            o("lbv", "lbv", "Laplacian Boundary Value (LBV) — https://doi.org/10.1002/nbm.3064"),
            o("ismv", "ismv", "Iterative Spherical Mean Value (iSMV) — https://doi.org/10.1002/mrm.24998"),
            o("sharp", "sharp", "SHARP (Sophisticated Harmonic Artifact Reduction) — https://doi.org/10.1016/j.neuroimage.2010.10.070"),
            o("resharp", "resharp", "Regularized SHARP (RESHARP) with Tikhonov — https://doi.org/10.1002/mrm.25032"),
            o("harperella", "harperella", "HARPERELLA integrated unwrap+BFR — https://doi.org/10.1002/nbm.3056"),
            o("iharperella", "iharperella", "Improved HARPERELLA (iHARPERELLA) — Li et al., Proc. ISMRM 2015, p.3313"),
            o("bfrnet", "bfrnet", "BFRnet deep-learning background removal (weights auto-downloaded) — https://github.com/sunhongfu/BFRnet"),
            o("iqfm", "iqfm", "iQFM deep-learning joint unwrapping + background removal from phase (weights auto-downloaded) — https://doi.org/10.1016/j.neuroimage.2022.119410"),
        ])).basic().flag("--bf-algorithm"),
        d("bg_removal.vsharp.deconvolve", Background, "Deconvolve", "Undo the SMV high-pass with a truncated-SVD deconvolution (default: off)", Bool).flag("--vsharp-deconvolve").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")),
        d("bg_removal.vsharp.threshold", Background, "Threshold", "Deconvolution (TSVD) threshold", Float).min(0.0).flag("--vsharp-threshold").parent("bg_removal.vsharp.deconvolve")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.deconvolve),
        d("bg_removal.vsharp.max_radius", Background, "Max Radius", "Largest SMV kernel radius (mm)", Float).units("mm").min(0.0).flag("--vsharp-max-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")),
        d("bg_removal.vsharp.min_radius", Background, "Min Radius", "Smallest SMV kernel radius (mm)", Float).units("mm").min(0.0).flag("--vsharp-min-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")),
        d("bg_removal.vsharp.grid", Background, "Working Grid", "", Enum(vec![
            o("native", "native", "The acquired voxel grid (default)"),
            o("finest", "finest", "An isotropic grid at the smallest voxel dimension, so no axis is coarsened; isotropic data stays on the acquired grid"),
            o("isotropic", "isotropic", "An isotropic grid of Grid Spacing mm; the background is interpolated back, the local field keeps native resolution"),
        ])).flag("--vsharp-grid").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")),
        d("bg_removal.vsharp.grid_spacing", Background, "Grid Spacing", "Isotropic working-grid spacing (mm)", Float).units("mm").min(0.0).flag("--vsharp-grid-spacing").parent("bg_removal.vsharp.grid")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.grid == VsharpGrid::Isotropic),
        d("bg_removal.pdf.tol", Background, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--pdf-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")),
        d("bg_removal.lbv.tol", Background, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--lbv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")),
        d("bg_removal.ismv.tol", Background, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--ismv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")),
        d("bg_removal.ismv.max_iter", Background, "Max Iter", "Maximum iterations", Int).min(0.0).flag("--ismv-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")),
        d("bg_removal.ismv.radius", Background, "Radius", "SMV kernel radius (mm)", Float).units("mm").min(0.0).flag("--ismv-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")),
        d("bg_removal.sharp.threshold", Background, "Threshold", "Deconvolution threshold", Float).min(0.0).flag("--sharp-threshold").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp")),
        d("bg_removal.sharp.radius", Background, "Radius", "SMV kernel radius (mm)", Float).units("mm").min(0.0).flag("--sharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp")),
        d("bg_removal.resharp.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").min(0.0).flag("--resharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")),
        d("bg_removal.resharp.tik_reg", Background, "Tik Reg", "Tikhonov regularization parameter", Float).min(0.0).flag("--resharp-tik-reg").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")),
        d("bg_removal.resharp.tol", Background, "Tolerance", "CG convergence tolerance", Float).min(0.0).flag("--resharp-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")),
        d("bg_removal.resharp.max_iter", Background, "Max Iter", "Maximum CG iterations", Int).min(0.0).flag("--resharp-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")),
        d("bg_removal.harperella.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").min(0.0).flag("--harperella-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")),
        d("bg_removal.harperella.max_iter", Background, "Max Iter", "Maximum iterations", Int).min(0.0).flag("--harperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")),
        d("bg_removal.harperella.tol", Background, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--harperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")),
        d("bg_removal.iharperella.max_iter", Background, "Max Iter", "Maximum iterations", Int).min(0.0).flag("--iharperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")),
        d("bg_removal.iharperella.tol", Background, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--iharperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")),
        d("bg_removal.msmv_refine", Background, "mSMV refinement", "Remove residual boundary-field shadows with mSMV after the primary BFR (Roberts 2024)", Bool).flag("--msmv-refine"),
        d("bg_removal.msmv.radius", Background, "mSMV Radius", "SMV prefilter kernel radius in mm", Float).units("mm").min(0.0).flag("--msmv-radius").parent("bg_removal.msmv_refine")
        .when(|x| x.c.bg_removal.msmv_refine),
        d("bg_removal.msmv.maxk", Background, "mSMV Max Iter", "Maximum boundary-correction iterations", Int).min(0.0).flag("--msmv-maxk").parent("bg_removal.msmv_refine")
        .when(|x| x.c.bg_removal.msmv_refine),
        d("pipeline.do_qsm", Inversion, "Compute QSM", "Enable QSM reconstruction (disable to only run the other outputs)", Bool).basic().flag("--no-qsm").also(&[FieldMap, Background, MultiOrientation, Reference]),
        d("inversion.algorithm", Inversion, "QSM Inversion", "", Enum(vec![
            o("rts", "rts", "RTS (Rapid Two-Step) — fast two-step k-space inversion with streaking removal. https://doi.org/10.1016/j.neuroimage.2017.11.018"),
            o("tv", "tv", "TV-ADMM (Total Variation) — ADMM inversion with a total-variation prior. https://doi.org/10.1002/mrm.25029"),
            o("tkd", "tkd", "TKD (Thresholded K-space Division) — direct k-space division with thresholding. https://doi.org/10.1002/mrm.22135"),
            o("tsvd", "tsvd", "TSVD (Truncated SVD) — zeroes the small singular values of the dipole kernel. https://doi.org/10.1002/mrm.22135"),
            o("tgv", "tgv", "TGV (Total Generalized Variation) — single-step: unwrapping, background removal and inversion together. https://doi.org/10.1016/j.neuroimage.2015.02.041"),
            o("tikhonov", "tikhonov", "Tikhonov — closed-form L2-regularized inversion. https://doi.org/10.1002/jmri.24365"),
            o("nltv", "nltv", "NLTV (Nonlinear Total Variation) — nonlinear data fidelity with a TV prior. https://doi.org/10.1002/mrm.27073"),
            o("medi", "medi", "MEDI (Morphology Enabled Dipole Inversion) — magnitude-guided edge weighting. https://doi.org/10.1002/mrm.22816"),
            o("tfi", "tfi", "TFI (Total Field Inversion) — inverts the total field directly (its own background removal). https://doi.org/10.1002/mrm.26331"),
            o("ilsqr", "ilsqr", "iLSQR (default) — iterative LSQR with streaking-artifact correction. https://doi.org/10.1016/j.neuroimage.2014.12.043"),
            o("lsqr", "lsqr", "LSQR (minimally regularised) — one least-squares solve for χ plus a residual field; the residual weight is the only regularisation. https://doi.org/10.1118/1.3481505"),
            o("heidi", "heidi", "HEIDI (Homogeneity Enabled Incremental Dipole Inversion) — keeps the well-conditioned k-space of an LSQR solve and re-derives the dipole cone under a homogeneity prior. https://doi.org/10.1016/j.neuroimage.2012.05.067"),
            o("qsmart", "qsmart", "QSMART — two-stage spatially-dependent filtering plus inversion. https://doi.org/10.1016/j.neuroimage.2020.117701"),
            o("ndi", "ndi", "NDI (Nonlinear Dipole Inversion) — gradient-descent nonlinear fit. https://doi.org/10.1002/nbm.4271"),
            o("fansi", "fansi", "FANSI (Nonlinear TV) — fast nonlinear susceptibility inversion with a TV prior. https://doi.org/10.1002/mrm.27073"),
            o("fansi-tgv", "fansi-tgv", "FANSI (Nonlinear TGV) — FANSI with a total-generalized-variation prior. https://doi.org/10.1002/mrm.27073"),
            o("l1qsm", "l1qsm", "L1-QSM (L1 Data Fidelity) — L1 fidelity for streaking suppression. https://doi.org/10.1002/mrm.29057"),
            o("whqsm", "whqsm", "WH-QSM (Weak-Harmonic QSM) — jointly fits a residual harmonic background field. https://doi.org/10.1002/mrm.27483"),
            o("hdqsm", "hdqsm", "HD-QSM (Hybrid Data Fidelity) — combined L1/L2 data fidelity. https://doi.org/10.1002/mrm.29296"),
            o("amp-pe", "amp-pe", "AMP-PE (Approximate Message Passing with Parameter Estimation) — self-tuning Bayesian inversion. https://doi.org/10.1002/mrm.29725"),
            o("xqsm", "xqsm", "xQSM (deep learning; weights auto-downloaded) — octave-convolution U-net dipole inversion. https://doi.org/10.1002/nbm.4461"),
            o("qsmnet", "qsmnet", "QSMnet (deep learning; weights auto-downloaded) — 3D U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2018.06.030"),
            o("qsmnet-plus", "qsmnet-plus", "QSMnet+ (deep learning; weights auto-downloaded) — QSMnet trained for a wider susceptibility range. https://doi.org/10.1016/j.neuroimage.2020.116579"),
            o("autoqsm", "autoqsm", "AutoQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2019.116064"),
            o("qsmgan", "qsmgan", "QSMGAN (deep learning; weights auto-downloaded) — GAN-refined U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2019.116389"),
            o("ir2qsm", "ir2qsm", "IR2QSM (deep learning; weights auto-downloaded) — iterative unrolled U-net inversion. https://doi.org/10.1002/mp.17747"),
            o("lpcnn", "lpcnn", "LPCNN (deep learning; weights auto-downloaded) — learned-proximal unrolled inversion. https://doi.org/10.1007/978-3-030-59713-9_13"),
            o("modl-qsm", "modl-qsm", "MoDL-QSM (deep learning; weights auto-downloaded) — model-based unrolled inversion (χ33/STI). https://doi.org/10.1016/j.neuroimage.2021.118376"),
            o("nextqsm", "nextqsm", "NeXtQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2022.119729"),
            o("iqsm", "iqsm", "iQSM (deep learning; weights auto-downloaded) — end-to-end reconstruction from wrapped phase. https://doi.org/10.1016/j.neuroimage.2022.119410"),
            o("iqsm-plus", "iqsm-plus", "iQSM+ (deep learning; weights auto-downloaded) — orientation-adaptive end-to-end reconstruction. https://doi.org/10.1016/j.media.2024.103160"),
        ])).basic().flag("--qsm-algorithm"),
        d("inversion.rts.delta", Inversion, "Delta", "Threshold for ill-conditioned k-space region", Float).min(0.0).flag("--rts-delta").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.mu", Inversion, "Mu", "Regularization parameter for well-conditioned region", Float).min(0.0).flag("--rts-mu").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.rho", Inversion, "Rho", "ADMM penalty parameter", Float).min(0.0).flag("--rts-rho").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.tol", Inversion, "Tolerance", "Convergence tolerance (relative change)", Float).min(0.0).flag("--rts-tol").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.max_iter", Inversion, "Max Iter", "Maximum ADMM iterations", Int).min(0.0).flag("--rts-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.lsmr_iter", Inversion, "LSMR Iter", "LSMR iterations for step 1 (well-conditioned solve)", Int).min(0.0).flag("--rts-lsmr-iter").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.tv.lambda", Inversion, "Lambda", "L1 regularization weight (smaller = smoother)", Float).min(0.0).flag("--tv-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        d("inversion.tv.rho", Inversion, "Rho", "ADMM penalty parameter (typically 100×lambda)", Float).min(0.0).flag("--tv-rho").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        d("inversion.tv.tol", Inversion, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--tv-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        d("inversion.tv.max_iter", Inversion, "Max Iter", "Maximum ADMM iterations", Int).min(0.0).flag("--tv-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        d("inversion.tkd.threshold", Inversion, "Threshold", "Truncation threshold for k-space division (0.1-0.2)", Float).min(0.0).flag("--tkd-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("tkd") || x.qsmart_inner("tkd")),
        d("inversion.tsvd.threshold", Inversion, "Threshold", "Truncation threshold for SVD (0.1-0.2)", Float).min(0.0).flag("--tsvd-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("tsvd") || x.qsmart_inner("tsvd")),
        d("inversion.tgv.iterations", Inversion, "Iterations", "Primal-dual iterations", Int).min(0.0).flag("--tgv-iterations").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.erosions", Inversion, "Erosions", "Mask erosions before TGV solve", Int).min(0.0).flag("--tgv-erosions").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.alpha1", Inversion, "Alpha1", "First-order TGV weight (gradient term), in ppm", Float).units("ppm").min(0.0).flag("--tgv-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.alpha0", Inversion, "Alpha0", "Second-order TGV weight (symmetric gradient term), in ppm", Float).units("ppm").min(0.0).flag("--tgv-alpha0").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.step_size", Inversion, "Step Size", "Primal step-size multiplier", Float).min(0.0).flag("--tgv-step-size").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.tol", Inversion, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--tgv-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tikhonov.lambda", Inversion, "Lambda", "L2 regularization weight", Float).min(0.0).flag("--tikhonov-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tikhonov") || x.qsmart_inner("tikhonov")),
        d("inversion.nltv.lambda", Inversion, "Lambda", "Regularization parameter", Float).min(0.0).flag("--nltv-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.mu", Inversion, "Mu", "Penalty parameter", Float).min(0.0).flag("--nltv-mu").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.tol", Inversion, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--nltv-tol").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.max_iter", Inversion, "Max Iter", "Maximum ADMM iterations", Int).min(0.0).flag("--nltv-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.newton_iter", Inversion, "Newton Iter", "Newton iterations for weight update", Int).min(0.0).flag("--nltv-newton-iter").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.medi.smv", Inversion, "SMV Mode", "MEDI handles background removal internally using spherical mean value preprocessing (skips the BG removal step)", Bool).flag("--no-medi-smv").parent("inversion.algorithm")
        .when(|x| x.inv("medi")),
        d("inversion.medi.smv_radius", Inversion, "SMV Radius", "SMV preprocessing radius in mm", Float).units("mm").min(0.0).flag("--medi-smv-radius").parent("inversion.algorithm")
        .when(|x| x.inv("medi")),
        d("inversion.medi.lambda", Inversion, "Lambda", "Regularization weight", Float).min(0.0).flag("--medi-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.percentage", Inversion, "Percentage", "Fraction of voxels considered edges (0.0-1.0)", Float).min(0.0).max(1.0).flag("--medi-percentage").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.max_iter", Inversion, "Max Iter", "Maximum outer iterations", Int).min(0.0).flag("--medi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.cg_max_iter", Inversion, "CG Max Iter", "Maximum conjugate gradient iterations", Int).min(0.0).flag("--medi-cg-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.cg_tol", Inversion, "CG Tolerance", "CG convergence tolerance", Float).min(0.0).flag("--medi-cg-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.tol", Inversion, "Tolerance", "Outer convergence tolerance", Float).min(0.0).flag("--medi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.merit", Inversion, "MERIT", "Iteratively down-weight voxels whose residual stays large", Bool).flag("--medi-merit").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.data_weighting", Inversion, "Data Weighting", "", Enum(vec![
            o(0, "uniform", "Every voxel of the field weighs the same in the data term"),
            o(1, "magnitude SNR", "Weight the data term by the magnitude SNR, so noisy voxels count for less"),
        ])).flag("--medi-data-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.tfi.lambda", Inversion, "Lambda", "Regularization weight", Float).min(0.0).flag("--tfi-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.precond", Inversion, "Precond", "Preconditioner value (susceptibility scaling outside the mask)", Float).min(0.0).flag("--tfi-precond").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.percentage", Inversion, "Percentage", "Fraction of voxels considered edges (0.0-1.0)", Float).min(0.0).max(1.0).flag("--tfi-percentage").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.max_iter", Inversion, "Max Iter", "Maximum outer iterations", Int).min(0.0).flag("--tfi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.cg_max_iter", Inversion, "CG Max Iter", "Maximum conjugate gradient iterations", Int).min(0.0).flag("--tfi-cg-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.cg_tol", Inversion, "CG Tolerance", "CG convergence tolerance", Float).min(0.0).flag("--tfi-cg-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.tol", Inversion, "Tolerance", "Outer convergence tolerance", Float).min(0.0).flag("--tfi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.merit", Inversion, "MERIT", "Iteratively down-weight voxels whose residual stays large", Bool).flag("--tfi-merit").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.data_weighting", Inversion, "Data Weighting", "", Enum(vec![
            o(0, "uniform", "Every voxel of the field weighs the same in the data term"),
            o(1, "magnitude SNR", "Weight the data term by the magnitude SNR, so noisy voxels count for less"),
        ])).flag("--tfi-data-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.ilsqr.tol", Inversion, "Tolerance", "Convergence tolerance", Float).min(0.0).flag("--ilsqr-tol").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr") || x.qsmart_inner("ilsqr")),
        d("inversion.ilsqr.max_iter", Inversion, "Max Iter", "Maximum iterations", Int).min(0.0).flag("--ilsqr-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr")),
        d("inversion.ilsqr.pad_mm", Inversion, "Pad (mm)", "Zero-padding per side, mm. Larger costs time and memory; below 16 mm thin volumes lose accuracy", Float).units("mm").min(0.0).flag("--ilsqr-pad-mm").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr")),
        d("inversion.lsqr.residual_weighting", Inversion, "Residual Weight", "Weight w on the residual field — the only regularisation. Blank = scaled from B0; 0 drops the term and streaks badly", Float).min(0.0).optional().flag("--lsqr-residual-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi")),
        d("inversion.lsqr.fit_global_offset", Inversion, "Fit Global Offset", "Fit a single global field offset alongside χ", Bool).flag("--no-lsqr-global-offset").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi")),
        d("inversion.lsqr.tol", Inversion, "LSQR Tolerance", "LSQR convergence tolerance", Float).min(0.0).flag("--lsqr-tol").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi")),
        d("inversion.lsqr.max_iter", Inversion, "LSQR Max Iter", "Maximum LSQR iterations", Int).min(0.0).flag("--lsqr-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr") || x.inv("heidi")),
        d("inversion.heidi.cone_threshold", Inversion, "Cone Threshold", "|D(k)| above which a coefficient is taken from the LSQR seed unchanged", Float).min(0.0).flag("--heidi-cone-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.gradient_threshold", Inversion, "Gradient Threshold", "Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous", Float).units("ppm/mm").min(0.0).flag("--heidi-gradient-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.apply_laplacian_correction", Inversion, "Laplacian Correction", "Open the homogeneity masks where the field's Laplacian is large", Bool).flag("--no-heidi-laplacian-correction").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.laplacian_threshold", Inversion, "Laplacian Threshold", "Field-Laplacian threshold (ppm per voxel²)", Float).min(0.0).flag("--heidi-laplacian-threshold").parent("inversion.heidi.apply_laplacian_correction")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.apply_laplacian_correction),
        d("inversion.heidi.gradient_mask_floor", Inversion, "Gradient Mask Floor", "TV weight kept where every direction reads as an edge (0 = upstream's binarised masks)", Float).min(0.0).flag("--heidi-gradient-mask-floor").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.continuation_steps", Inversion, "Continuation Steps", "NESTA continuation steps on μ. The dominant quality knob, and more is not better — worth sweeping", Int).min(0.0).flag("--heidi-continuation-steps").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.inner_iterations", Inversion, "Inner Iterations", "Accelerated-gradient iterations per continuation step", Int).min(0.0).flag("--heidi-inner-iterations").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.mu_min", Inversion, "Mu Min", "Final (smallest) smoothing parameter μ", Float).min(0.0).flag("--heidi-mu-min").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.tol", Inversion, "HEIDI Tolerance", "Relative objective change that ends an inner loop early", Float).min(0.0).flag("--heidi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.denoise", Inversion, "Denoise Field", "Anisotropic-diffusion smoothing of the field before its gradients are taken", Bool).flag("--no-heidi-denoise").parent("inversion.algorithm")
        .when(|x| x.inv("heidi")),
        d("inversion.heidi.denoise_iterations", Inversion, "Denoise Iter", "Anisotropic-diffusion iterations", Int).min(0.0).flag("--heidi-denoise-iterations").parent("inversion.heidi.denoise")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.denoise),
        d("inversion.heidi.denoise_time_step", Inversion, "Time Step", "Anisotropic-diffusion time step", Float).min(0.0).flag("--heidi-denoise-time-step").parent("inversion.heidi.denoise")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.denoise),
        d("inversion.heidi.denoise_conductance", Inversion, "Conductance", "Anisotropic-diffusion conductance", Float).min(0.0).flag("--heidi-denoise-conductance").parent("inversion.heidi.denoise")
        .when(|x| x.inv("heidi") && x.c.inversion.heidi.denoise),
        d("inversion.qsmart.inversion", Inversion, "Inner Inversion", "", Enum(vec![
            o("ilsqr", "ilsqr", "iLSQR (default) — QSMART's original inner inversion"),
            o("rts", "rts", "Rapid Two-Step (RTS)"),
            o("tv", "tv", "Total Variation ADMM (TV)"),
            o("tkd", "tkd", "Truncated K-space Division (TKD)"),
            o("tsvd", "tsvd", "Truncated SVD (TSVD)"),
            o("tikhonov", "tikhonov", "Tikhonov L2 regularization"),
            o("nltv", "nltv", "Nonlinear Total Variation (NLTV)"),
            o("medi", "medi", "Morphology Enabled Dipole Inversion (MEDI)"),
        ])).flag("--qsmart-inversion").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.ilsqr_max_iter", Inversion, "iLSQR Max Iter", "Maximum iLSQR iterations per stage (tolerance: iLSQR's)", Int).min(0.0).flag("--qsmart-ilsqr-max-iter").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")),
        d("inversion.qsmart.ilsqr_pad_mm", Inversion, "iLSQR Pad (mm)", "Zero-padding of the inner iLSQR solves, mm per side (default: 6)", Float).units("mm").min(0.0).flag("--qsmart-ilsqr-pad-mm").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")),
        d("inversion.qsmart.sdf_sigma1_stage1", Inversion, "SDF Sigma1 (s1)", "Stage 1 SDF kernel sigma 1 (voxels)", Float).units("voxels").min(0.0).flag("--qsmart-sdf-sigma1-stage1").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.sdf_sigma2_stage1", Inversion, "SDF Sigma2 (s1)", "Stage 1 SDF kernel sigma 2 (voxels)", Float).units("voxels").min(0.0).flag("--qsmart-sdf-sigma2-stage1").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.sdf_sigma1_stage2", Inversion, "SDF Sigma1 (s2)", "Stage 2 SDF kernel sigma 1 (voxels)", Float).units("voxels").min(0.0).flag("--qsmart-sdf-sigma1-stage2").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.sdf_sigma2_stage2", Inversion, "SDF Sigma2 (s2)", "Stage 2 SDF kernel sigma 2 (voxels)", Float).units("voxels").min(0.0).flag("--qsmart-sdf-sigma2-stage2").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.sdf_spatial_radius", Inversion, "SDF Radius", "SDF spatial filtering radius (voxels)", Int).units("voxels").min(0.0).flag("--qsmart-sdf-spatial-radius").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.sdf_lower_lim", Inversion, "SDF Lower Lim", "SDF proximity lower limit", Float).min(0.0).flag("--qsmart-sdf-lower-lim").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.sdf_curv_constant", Inversion, "SDF Curv Const", "SDF curvature constant", Float).min(0.0).flag("--qsmart-sdf-curv-constant").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.vasc_sphere_radius", Inversion, "Vasc Radius (mm)", "Bottom-hat sphere radius for vasculature detection (mm)", Int).units("mm").min(0.0).flag("--qsmart-vasc-sphere-radius").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.frangi_scale_min", Inversion, "Frangi Min (mm)", "Frangi minimum vessel radius (mm)", Float).units("mm").min(0.0).flag("--qsmart-frangi-scale-min").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.frangi_scale_max", Inversion, "Frangi Max (mm)", "Frangi maximum vessel radius (mm)", Float).units("mm").min(0.0).flag("--qsmart-frangi-scale-max").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.frangi_scale_ratio", Inversion, "Frangi Step (mm)", "Frangi scale step (mm)", Float).units("mm").min(0.0).flag("--qsmart-frangi-scale-ratio").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.qsmart.frangi_c", Inversion, "Frangi C", "Frangi C noise threshold", Float).min(0.0).flag("--qsmart-frangi-c").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")),
        d("inversion.ndi.tau", Inversion, "Tau", "Gradient-descent step size", Float).min(0.0).flag("--ndi-tau").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.ndi.alpha", Inversion, "Alpha", "L2 regularization weight", Float).min(0.0).flag("--ndi-alpha").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.ndi.max_iter", Inversion, "Max Iter", "Number of iterations", Int).min(0.0).flag("--ndi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.ndi.phase_scale", Inversion, "Phase Scale", "ppm -> working-scale multiplier", Float).min(0.0).flag("--ndi-phase-scale").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.fansi.alpha1", Inversion, "Alpha1", "First-order (TV/TGV) L1 penalty weight", Float).min(0.0).flag("--fansi-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.mu1", Inversion, "Mu1", "Gradient-consistency ADMM weight", Float).min(0.0).flag("--fansi-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.mu2", Inversion, "Mu2", "Fidelity-consistency ADMM weight", Float).min(0.0).flag("--fansi-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.alpha0", Inversion, "Alpha0", "Second-order L1 penalty weight (nlTGV only)", Float).min(0.0).flag("--fansi-alpha0").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.mu0", Inversion, "Mu0", "Second-order consistency ADMM weight (nlTGV only)", Float).min(0.0).flag("--fansi-mu0").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.max_iter", Inversion, "Max Iter", "Number of outer ADMM iterations", Int).min(0.0).flag("--fansi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.tol_update", Inversion, "Tol Update", "Percent-update stopping tolerance", Float).min(0.0).flag("--fansi-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.tol_delta", Inversion, "Tol Delta", "Inner Newton convergence tolerance", Float).min(0.0).flag("--fansi-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.fansi.phase_scale", Inversion, "Phase Scale", "ppm -> working-scale multiplier", Float).min(0.0).flag("--fansi-phase-scale").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")),
        d("inversion.l1qsm.alpha1", Inversion, "Alpha1", "Gradient (TV) L1 penalty weight", Float).min(0.0).flag("--l1qsm-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu1", Inversion, "Mu1", "Gradient-consistency ADMM weight", Float).min(0.0).flag("--l1qsm-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu2", Inversion, "Mu2", "Fidelity-consistency ADMM weight", Float).min(0.0).flag("--l1qsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu3", Inversion, "Mu3", "L1 proximal ADMM weight", Float).min(0.0).flag("--l1qsm-mu3").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.lambda", Inversion, "Lambda", "L1 fidelity strength", Float).min(0.0).flag("--l1qsm-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.max_iter", Inversion, "Max Iter", "Number of outer ADMM iterations", Int).min(0.0).flag("--l1qsm-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.tol_update", Inversion, "Tol Update", "Percent-update stopping tolerance", Float).min(0.0).flag("--l1qsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.tol_delta", Inversion, "Tol Delta", "Inner Newton convergence tolerance", Float).min(0.0).flag("--l1qsm-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.phase_scale", Inversion, "Phase Scale", "ppm -> working-scale multiplier", Float).min(0.0).flag("--l1qsm-phase-scale").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.whqsm.alpha1", Inversion, "Alpha1", "TV regularization weight", Float).min(0.0).flag("--whqsm-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.mu1", Inversion, "Mu1", "ADMM penalty for TV splitting", Float).min(0.0).flag("--whqsm-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.mu2", Inversion, "Mu2", "ADMM penalty for data-fidelity splitting", Float).min(0.0).flag("--whqsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.beta", Inversion, "Beta", "Weak-harmonic ROI penalty", Float).min(0.0).flag("--whqsm-beta").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.muh", Inversion, "Muh", "ADMM penalty for harmonic-field splitting", Float).min(0.0).flag("--whqsm-muh").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.max_iter", Inversion, "Max Iter", "Maximum outer iterations", Int).min(0.0).flag("--whqsm-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.tol_update", Inversion, "Tol Update", "Percent-update stopping tolerance", Float).min(0.0).flag("--whqsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.tol_delta", Inversion, "Tol Delta", "Inner Newton stopping tolerance", Float).min(0.0).flag("--whqsm-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.phase_scale", Inversion, "Phase Scale", "ppm -> working-scale multiplier", Float).min(0.0).flag("--whqsm-phase-scale").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.hdqsm.alpha_l2", Inversion, "Alpha L2", "L2-stage TV weight", Float).min(0.0).flag("--hdqsm-alpha-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.mu1_l2", Inversion, "Mu1 L2", "L2-stage gradient-consistency ADMM weight", Float).min(0.0).flag("--hdqsm-mu1-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.mu2", Inversion, "Mu2", "Fidelity consistency weight", Float).min(0.0).flag("--hdqsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.max_iter_l1", Inversion, "Max Iter L1", "Stage-1 (L1) iterations", Int).min(0.0).flag("--hdqsm-max-iter-l1").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.max_iter_l2", Inversion, "Max Iter L2", "Stage-2 (L2) iterations", Int).min(0.0).flag("--hdqsm-max-iter-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.tol_update", Inversion, "Tol Update", "Stage-2 percent-update stopping tolerance", Float).min(0.0).flag("--hdqsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.amp_pe.wave_order", Inversion, "Wave Order", "Daubechies wavelet order (1=db1, 2=db2)", Int).min(0.0).flag("--amp-pe-wave-order").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.nlevel", Inversion, "N Level", "Wavelet decomposition levels", Int).min(0.0).flag("--amp-pe-nlevel").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.wave_pec", Inversion, "Wave Pec", "Morphology-mask energy retention fraction (0.0-1.0)", Float).min(0.0).max(1.0).flag("--amp-pe-wave-pec").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.simulated_te", Inversion, "Simulated TE", "Simulated echo time (s) used to turn the field into phase", Float).units("s").min(0.0).flag("--amp-pe-simulated-te").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.max_linearization_ite", Inversion, "Max Linearization Ite", "Linearization iterations per stage", Int).min(0.0).flag("--amp-pe-max-linearization-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.tikhonov_beta", Inversion, "Tikhonov Beta", "L2-seed Tikhonov weight", Float).min(0.0).flag("--amp-pe-tikhonov-beta").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.damp_rate_sig", Inversion, "Damp Rate (signal)", "GAMP signal-update damping rate", Float).min(0.0).flag("--amp-pe-damp-rate-sig").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.damp_rate_par", Inversion, "Damp Rate (params)", "Parameter-estimation learning rate (kappa)", Float).min(0.0).flag("--amp-pe-damp-rate-par").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.max_pe_spar_ite", Inversion, "Sparse Recon Ite", "Inner sparse-reconstruction iterations", Int).min(0.0).flag("--amp-pe-max-pe-spar-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.max_pe_est_ite", Inversion, "Param Est Ite", "Inner parameter-estimation iterations", Int).min(0.0).flag("--amp-pe-max-pe-est-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.cvg_thd", Inversion, "Convergence", "GAMP inner convergence threshold", Float).min(0.0).flag("--amp-pe-cvg-thd").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.tile_size", Inversion, "Tiled Inference", "Run the network on overlapping tiles: bounded memory, approximate. Off = whole volume", Int)
        .value("Tile Size", "Overlap-tiling core size in voxels").units("voxels").min(0.0).off(Value::Null).flag("--tile-size").parent("inversion.algorithm")
        .when(|x| x.tileable),
        d("inversion.tile_halo", Inversion, "Tile Halo", "Overlap-tiling context margin in voxels (blank = 8)", Int).units("voxels").min(0.0).optional().flag("--tile-halo").parent("inversion.tile_size")
        .when(|x| x.tileable && x.c.inversion.tile_size.is_some()),
        d("multi_orientation.algorithm", MultiOrientation, "Combine as", "", Enum(vec![
            o("cosmos", "cosmos", "COSMOS — scalar susceptibility from 2+ orientations, co-registered if needed, closed form in k-space"),
            o("sti", "sti", "STI — rank-2 susceptibility tensor; needs 6+ orientations that are not coplanar"),
        ])).basic().flag("--multi-orientation-algorithm").parent("multi_orientation.group_by"),
        d("multi_orientation.lambda", MultiOrientation, "Lambda", "Regularization (blank = 0, plain least squares). Raise for 2 orientations or single-axis rotations", Float).min(0.0).flag("--multi-orientation-lambda").parent("multi_orientation.group_by"),
        d("multi_orientation.magnitude_weighting", MultiOrientation, "COSMOS Magnitude Weighting", "COSMOS groups: weight each orientation by its combined magnitude (conjugate-gradient solve). Off = closed-form least squares", Bool).flag("--cosmos-magnitude-weighting").parent("multi_orientation.algorithm")
        .when(|x| x.cosmos),
        d("multi_orientation.cosmos_tol", MultiOrientation, "CG Tolerance", "Conjugate-gradient tolerance of weighted COSMOS", Float).min(0.0).flag("--cosmos-tol").parent("multi_orientation.magnitude_weighting")
        .when(|x| x.cosmos && x.c.multi_orientation.magnitude_weighting),
        d("multi_orientation.cosmos_max_iter", MultiOrientation, "CG Max Iterations", "Conjugate-gradient iteration cap of weighted COSMOS", Int).min(0.0).flag("--cosmos-max-iter").parent("multi_orientation.magnitude_weighting")
        .when(|x| x.cosmos && x.c.multi_orientation.magnitude_weighting),
        d("multi_orientation.register", MultiOrientation, "Co-register", "Co-register orientations that need it, with the registration settings below. Off = refuse such groups", Bool).flag("--no-orientation-registration").parent("multi_orientation.group_by"),
        d("multi_orientation.force", MultiOrientation, "Force", "Reconstruct even when the B0 directions look degenerate (wrong metadata gives a plausible wrong map)", Bool).flag("--multi-orientation-force").parent("multi_orientation.group_by"),
        d("qsm.reference", Reference, "QSM Reference", "", Enum(vec![
            o("mean", "mean", "Subtract the mean susceptibility over the voxels the map is defined on (recommended)"),
            o("none", "none", "No referencing (raw susceptibility values)"),
            o("region", "region", "Reference χ to the mean of a SynthSeg region (Reference Region). Enables SynthSeg"),
        ])).basic().flag("--qsm-reference"),
        d("qsm.reference_region", Reference, "Reference Region", "The SynthSeg region whose mean susceptibility is zero. Enables SynthSeg", Regions).basic().optional().flag("--qsm-reference").parent("qsm.reference")
        .when(|x| x.c.qsm.reference == QsmReference::Region),
        d("pipeline.do_chi_separation", Separation, "Compute χ-separation", "Split QSM into paramagnetic (χ+, iron) and diamagnetic (χ−, myelin) maps", Bool).basic().flag("--do-chisep"),
        d("separation.algorithm", Separation, "Method", "", Enum(vec![
            o("r2star-qsm", "r2star-qsm", "R2*-QSM closed-form (Dimov 2022) — QSM + R2*, GRE-only"),
            o("decompose", "decompose", "DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude, GRE-only"),
            o("chi-sep-ilsqr", "chi-sep-ilsqr", "χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM"),
            o("chi-sep-medi", "chi-sep-medi", "χ-separation MEDI — local field + R2' + magnitude"),
            o("wavesep", "wavesep", "WaveSep wavelet-L1 (Fang 2023) — QSM + R2'"),
            o("hc-chisep", "hc-chisep", "Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude"),
            o("susep-net", "susep-net", "SUSEP-Net deep-learning separation — QSM + R2' + local field (weights auto-downloaded)"),
            o("chi-sepnet", "chi-sepnet", "χ-sepnet deep-learning separation — QSM + R2' + local field (weights auto-downloaded)"),
        ])).basic().flag("--chisep").parent("pipeline.do_chi_separation")
        .when(|x| x.c.pipeline.do_chi_separation),
        d("separation.r2star_qsm.r_const_3t", Separation, "Relaxometric const @3T", "Hz/ppm at 3T (scales with B0)", Float).units("Hz/ppm").min(0.0).flag("--r2star-qsm-r-const-3t").parent("separation.algorithm")
        .when(|x| x.sep("r2star-qsm")),
        d("separation.decompose.n_inner", Separation, "Inner passes", "Alternating 3-stage fit passes per voxel", Int).min(0.0).flag("--decompose-n-inner").parent("separation.algorithm")
        .when(|x| x.sep("decompose")),
        d("separation.decompose.chi_bound", Separation, "Chi bound (ppm)", "Upper bound on |χ| in the fit", Float).units("ppm").min(0.0).flag("--decompose-chi-bound").parent("separation.algorithm")
        .when(|x| x.sep("decompose")),
        d("separation.decompose.max_lm_iter", Separation, "Max LM iter", "Levenberg–Marquardt max iterations", Int).min(0.0).flag("--decompose-max-lm-iter").parent("separation.algorithm")
        .when(|x| x.sep("decompose")),
        d("separation.chi_sep_ilsqr.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxometric constant", Float).units("Hz/ppm").min(0.0).flag("--chi-sep-ilsqr-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxometric constant", Float).units("Hz/ppm").min(0.0).flag("--chi-sep-ilsqr-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.lambda1", Separation, "Lambda1", "L1 edge-masked TV weight", Float).min(0.0).flag("--chi-sep-ilsqr-lambda1").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.percentage", Separation, "Percentage", "Edge-mask keep fraction (0-1)", Float).min(0.0).max(1.0).flag("--chi-sep-ilsqr-percentage").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.r2p_min", Separation, "R2' min (Hz)", "R2' reliability window lower", Float).units("Hz").min(0.0).flag("--chi-sep-ilsqr-r2p-min").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.r2p_max", Separation, "R2' max (Hz)", "R2' reliability window upper", Float).units("Hz").min(0.0).flag("--chi-sep-ilsqr-r2p-max").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.max_iter", Separation, "Max Iter", "Outer Gauss-Newton iterations", Int).min(0.0).flag("--chi-sep-ilsqr-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.tol", Separation, "Tolerance", "Outer relative-change tolerance", Float).min(0.0).flag("--chi-sep-ilsqr-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.cg_max_iter", Separation, "CG Max Iter", "Inner CG max iterations", Int).min(0.0).flag("--chi-sep-ilsqr-cg-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_ilsqr.cg_tol", Separation, "CG Tolerance", "Inner CG relative tolerance", Float).min(0.0).flag("--chi-sep-ilsqr-cg-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")),
        d("separation.chi_sep_medi.lambda_para", Separation, "Lambda para", "Paramagnetic L1 weight", Float).min(0.0).flag("--chi-sep-medi-lambda-para").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.lambda_dia", Separation, "Lambda dia", "Diamagnetic L1 weight", Float).min(0.0).flag("--chi-sep-medi-lambda-dia").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.lambda_cpl", Separation, "Lambda cpl", "Field/R2' coupling weight", Float).min(0.0).flag("--chi-sep-medi-lambda-cpl").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxivity", Float).units("Hz/ppm").min(0.0).flag("--chi-sep-medi-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxivity", Float).units("Hz/ppm").min(0.0).flag("--chi-sep-medi-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.percentage", Separation, "Percentage", "Edge-mask percentage (0-1)", Float).min(0.0).max(1.0).flag("--chi-sep-medi-percentage").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.cg_tol", Separation, "CG Tolerance", "Inner CG tolerance", Float).min(0.0).flag("--chi-sep-medi-cg-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.cg_max_iter", Separation, "CG Max Iter", "Inner CG max iterations", Int).min(0.0).flag("--chi-sep-medi-cg-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.max_iter", Separation, "Max Iter", "Outer Gauss-Newton iterations", Int).min(0.0).flag("--chi-sep-medi-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.chi_sep_medi.tol", Separation, "Tolerance", "Outer convergence tolerance", Float).min(0.0).flag("--chi-sep-medi-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")),
        d("separation.wavesep.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxivity", Float).units("Hz/ppm").min(0.0).flag("--wavesep-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.wavesep.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxivity", Float).units("Hz/ppm").min(0.0).flag("--wavesep-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.wavesep.alpha", Separation, "Alpha", "Proximal-gradient step size", Float).min(0.0).flag("--wavesep-alpha").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.wavesep.lambda", Separation, "Lambda", "Wavelet L1 soft-threshold weight", Float).min(0.0).flag("--wavesep-lambda").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.wavesep.wavelet_order", Separation, "Wavelet order", "Daubechies order", Int).min(0.0).flag("--wavesep-wavelet-order").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.wavesep.max_iter", Separation, "Max Iter", "ISTA max iterations", Int).min(0.0).flag("--wavesep-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.wavesep.tol", Separation, "Tolerance", "Relative-change stop tolerance", Float).min(0.0).flag("--wavesep-tol").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")),
        d("separation.hc_chisep.dr_pos_3t", Separation, "Dr+ @3T (Hz/ppm)", "Paramagnetic relaxivity at 3T", Float).units("Hz/ppm").min(0.0).flag("--hc-chisep-dr-pos-3t").parent("separation.algorithm")
        .when(|x| x.sep("hc-chisep")),
        d("separation.hc_chisep.bin_hz", Separation, "R2' bin (Hz)", "R2' bin width for anchored grid search", Float).units("Hz").min(0.0).flag("--hc-chisep-bin-hz").parent("separation.algorithm")
        .when(|x| x.sep("hc-chisep")),
        d("separation.custom_qsm_tool", Separation, "Custom QSM tool", "Use a Chimap from <bids>/derivatives/<tool>/ instead of recomputing QSM (χ-separation, statistics, SMWI); blank = compute, * = any tool", Text).optional().flag("--use-custom-qsm").also(&[SwiSmwi, Segmentation]).supplied_file()
        .when(|x| x.reads_qsm),
        d("pipeline.do_swi", SwiSmwi, "Compute SWI", "Also compute susceptibility-weighted images with CLEAR-SWI, from every echo", Bool).basic().flag("--do-swi"),
        d("swi.unwrap", SwiSmwi, "Unwrapping", "", Enum(vec![
            o("laplacian", "laplacian", "Each echo unwrapped with the Laplacian method and high-pass filtered, then combined"),
            o("laplacian-slice", "laplacian-slice", "Laplacian unwrapping slice by slice, for 2D multi-slice acquisitions"),
            o("romeo", "romeo", "All echoes unwrapped together with ROMEO, combined, then high-pass filtered"),
        ])).flag("--swi-unwrapping").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi),
        d("swi.scaling", SwiSmwi, "Scaling", "", Enum(vec![
            o("tanh", "tanh", "Phase scaling type for SWI"),
            o("negative-tanh", "negative-tanh", "Phase scaling type for SWI"),
            o("positive", "positive", "Phase scaling type for SWI"),
            o("negative", "negative", "Phase scaling type for SWI"),
            o("triangular", "triangular", "Phase scaling type for SWI"),
        ])).flag("--swi-scaling").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi),
        d("swi.strength", SwiSmwi, "Strength", "Phase scaling strength (higher = stronger phase contrast)", Float).min(0.0).flag("--swi-strength").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi),
        d("swi.hp_sigma", SwiSmwi, "HP Sigma (x y z)", "High-pass filter sigma per axis, in voxels (x y z): background phase removal. Set z to 0 for thin axial slices", FloatVec3).units("voxels").min(0.0).flag("--swi-hp-sigma").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi),
        d("swi.mip_window", SwiSmwi, "mIP Window", "Minimum intensity projection window size in slices", Int).units("slices").min(0.0).flag("--swi-mip-window").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi),
        d("pipeline.do_smwi", SwiSmwi, "Compute SMWI", "Susceptibility map-weighted imaging — weights the magnitude by a mask built from χ, not from filtered phase. Needs QSM", Bool).basic().flag("--do-smwi"),
        d("smwi.threshold_ppm", SwiSmwi, "Threshold", "|χ| in ppm at which the weighting mask reaches zero. The 1 ppm default targets strong sources; brain tissue wants far less", Float).units("ppm").min(0.0).flag("--smwi-threshold").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi),
        d("smwi.power", SwiSmwi, "Power", "Power the mask is raised to (contrast strength)", Float).min(0.0).flag("--smwi-power").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi),
        d("smwi.mip_window", SwiSmwi, "mIP Window", "Minimum-intensity-projection window in slices", Int).units("slices").min(0.0).flag("--smwi-mip-window").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi),
        d("pipeline.do_t2starmap", Relaxometry, "Compute T2* Map", "Compute T2* relaxation map (requires 3+ echoes with magnitude)", Bool).basic().flag("--do-t2starmap"),
        d("pipeline.do_r2starmap", Relaxometry, "Compute R2* Map", "Compute R2* decay rate map (requires 3+ echoes with magnitude)", Bool).basic().flag("--do-r2starmap"),
        d("pipeline.do_r2map", Relaxometry, "Compute R2 Map", "R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG. Forced on when chi-separation needs it", Bool).basic().flag("--do-r2map"),
        d("separation.custom_r2_tool", Relaxometry, "Custom R2 tool", "Use an R2 map from <bids>/derivatives/<tool>/ instead of computing it from MESE (blank = compute, * = any tool)", Text).optional().flag("--use-custom-r2").parent("pipeline.do_r2map").supplied_file()
        .when(|x| x.reads_r2),
        d("pipeline.do_r2primemap", Relaxometry, "Compute R2' Map", "R2' = R2* − R2 (Hz); needs GRE magnitude + a MESE acquisition, or R2PRIMEnet. Forced on when chi-separation needs it", Bool).basic().flag("--do-r2primemap"),
        d("separation.custom_r2prime_tool", Relaxometry, "Custom R2' tool", "Use an R2' map from <bids>/derivatives/<tool>/ instead of computing it (blank = compute, * = any tool)", Text).optional().flag("--use-custom-r2prime").parent("pipeline.do_r2primemap").supplied_file()
        .when(|x| x.reads_r2prime),
        d("separation.r2prime_strategy", Relaxometry, "R2' Source", "", Enum(vec![
            o("auto", "auto", "Measure R2' = R2* - R2 from a MESE acquisition when one is present, else predict it from R2* with R2PRIMEnet"),
            o("mese", "mese", "Only ever measure R2' from a MESE acquisition - produces no R2' without spin-echo data"),
            o("r2primenet", "r2primenet", "Always predict R2' from R2* with R2PRIMEnet, even when a MESE acquisition is available (an estimate, not a measurement)"),
        ])).flag("--r2prime-strategy").parent("pipeline.do_r2primemap")
        .when(|x| x.computes_r2prime),
        d("separation.r2primenet.ignore_field_mismatch", Relaxometry, "R2PRIMEnet off 3 T", "Run R2PRIMEnet on non-3 T data (trained at 3 T only; the R2' is uncalibrated). Off: auto skips R2', r2primenet errors", Bool).flag("--r2primenet-ignore-field-mismatch").parent("separation.r2prime_strategy")
        .when(|x| x.computes_r2prime && x.c.separation.r2prime_strategy != R2PrimeStrategy::Mese),
        d("separation.r2primenet.patch", Relaxometry, "R2PRIMEnet Patch", "Sliding-window patch D H W in voxels, each a multiple of 16 (default 192 192 128)", IntVec3).units("voxels").min(0.0).flag("--r2primenet-patch").parent("separation.r2prime_strategy")
        .when(|x| x.computes_r2prime && x.c.separation.r2prime_strategy != R2PrimeStrategy::Mese),
        d("pipeline.do_segmentation", Segmentation, "Segmentation", "Whole-brain parcellation: SynthSeg straight off the GRE magnitude, or one supplied in derivatives. Forced on by the statistics and a region reference", Bool).basic().flag("--do-segmentation"),
        d("segmentation.custom_dseg_tool", Segmentation, "Derivatives tool", "The dseg from <bids>/derivatives/<tool>/: * = any tool, or a tool name; SynthSeg runs where none is found", Text).optional().flag("--use-custom-dseg").parent("pipeline.do_segmentation").supplied_file()
        .when(|x| x.segmentation_on),
        d("segmentation.version", Segmentation, "SynthSeg Version", "", Enum(vec![
            o("v1", "v1", "v1: 32 labels, weights ship with SynthSeg. v2: 33 labels (adds CSF); its weights are not published yet, so a run with v2 is refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR"),
            o("v2", "v2", "v1: 32 labels, weights ship with SynthSeg. v2: 33 labels (adds CSF); its weights are not published yet, so a run with v2 is refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR"),
        ])).flag("--synthseg-version").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()),
        d("segmentation.crop", Segmentation, "Crop", "Cubic centre-crop in voxels at 1 mm before inference; empty processes the whole volume", Int).units("voxels").min(0.0).optional().flag("--synthseg-crop").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()),
        d("segmentation.flip_averaging", Segmentation, "Flip Averaging", "Average with a left-right flipped second pass (SynthSeg's default). Doubles inference time", Bool).flag("--no-synthseg-flip-averaging").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()),
        d("segmentation.topology_cleanup", Segmentation, "Topology Cleanup", "Reset each topological class to its largest connected component", Bool).flag("--no-synthseg-topology-cleanup").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()),
        d("segmentation.sigma_smoothing", Segmentation, "Posterior Smoothing", "Gaussian blur on the posteriors before the labels are taken", Float)
        .value("Sigma (voxels)", "Gaussian sigma of the posterior blur, in voxels").units("voxels").min(0.0).off(json!(0.0)).flag("--synthseg-sigma").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()),
        d("pipeline.do_analysis", Segmentation, "Per-structure statistics", "Summarise every map the run makes (χ, χ+/χ−, T2*, R2*, R2, R2') per structure into a TSV: mean, SD, median, min, max, 5th/95th percentiles. Implies segmentation; implies QSM only if no other map is enabled", Bool).basic().flag("--do-analysis"),
        // DICOM export is not in the browser yet (astewartau/qsmbly#164).
        d("pipeline.export_dicom", Export, "Export DICOM", "Also write final maps as DICOM series into each subject's extra_files/ folder", Bool).basic().flag("--export-dicom").unavailable_in_browser(),
    ]
}
