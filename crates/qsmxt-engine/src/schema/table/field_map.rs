//! The Field map section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("field_mapping.unwrapping_algorithm", FieldMap, "Unwrapping", "", Enum(vec![
            o("romeo", "romeo", "ROMEO", "ROMEO region-growing unwrapping — https://doi.org/10.1002/mrm.28563"),
            o("laplacian", "laplacian", "Laplacian", "Laplacian phase unwrapping (FFT-based) — https://doi.org/10.1364/OL.28.001194"),
            o("prelude", "prelude", "PRELUDE", "PRELUDE region-merging unwrapping, each echo on its own — https://doi.org/10.1002/mrm.10354"),
            o("medi-region-growing", "medi-region-growing", "MEDI region growing", "MEDI's magnitude-guided region-growing unwrapping; needs magnitude images — https://doi.org/10.1002/mrm.24272"),
        ])).basic().flag("--unwrapping-algorithm").full("Phase unwrapping")
        .gui("How the wrapped phase is unwrapped"),
        d("field_mapping.prelude.labelling", FieldMap, "Labelling", "", Enum(vec![
            o("auto", "auto", "Auto", "As FSL prelude chooses: slice by slice above 64 voxels in-plane, else over the whole volume (default)"),
            o("slices", "slices", "Slices", "Label regions slice by slice (FSL prelude --labelslices)"),
            o("volume", "volume", "Volume", "Label regions over the whole volume (FSL prelude -f)"),
        ])).flag("--prelude-labelling").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.prelude).full("PRELUDE labelling")
        .gui("How PRELUDE labels its initial regions"),
        d("field_mapping.prelude.num_phase_partitions", FieldMap, "Phase partitions", "Number of phase bins the initial regions are formed from (default 8)", Int).min(1.0).flag("--prelude-partitions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.prelude).full("PRELUDE phase partitions")
        .gui("Number of phase bins the initial regions are formed from"),
        d("field_mapping.laplacian_solver", FieldMap, "Solver", "", Enum(vec![
            o("dct", "dct", "DCT", "Unweighted least squares, Neumann-boundary DCT solve (Ghiglia & Romero 1994; QSM.jl :dct) (default)"),
            o("fft", "fft", "FFT", "Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume"),
            o("mirror", "mirror", "Mirror", "Schofield & Zhu sin/cos Laplacian, FFT solve on the volume mirrored along each axis (QSMART's reference)"),
            o("mgpcg", "mgpcg", "MGPCG", "MGPCG (QSM.jl): Laplacian unwrapping with LBV background removal in one step; QSM.jl follows it with V-SHARP to remove remaining background"),
        ])).flag("--laplacian-solver").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian).full("Laplacian solver")
        .gui("How Laplacian unwrapping's Poisson equation is solved"),
        d("field_mapping.laplacian_fft_pad", FieldMap, "FFT pad (voxels)", "Zero-padding per side: one value for every axis or three (x y z). Default: 64", IntVec3).units("voxels").flag("--laplacian-fft-pad").parent("field_mapping.laplacian_solver")
        .when(|x| x.laplacian && x.c.field_mapping.laplacian_solver == LaplacianSolver::Fft).full("Laplacian FFT padding")
        .gui("Zero-padding per side: one value for every axis, or three (x y z)"),
        d("field_mapping.laplacian_unmasked", FieldMap, "Unmasked phase", "Unwrap the raw phase over the whole volume, as QSMART does, instead of the phase set to zero outside the brain mask", Bool).flag("--laplacian-unmasked").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian).full("Laplacian unwrapping of the raw phase"),
        d("field_mapping.laplacian_keep_outside", FieldMap, "Keep field outside", "Keep the field over the whole volume, not only inside the brain mask, as UK Biobank's pipeline hands it to V-SHARP (weighted-average combination)", Bool).flag("--laplacian-keep-outside").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian && !x.c.field_mapping.laplacian_unmasked && x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg)
        .full("Laplacian field kept outside the mask"),
        // MEDI's complex fit fits its own phase offset: QSM.rs does not remove them first.
        d("field_mapping.phase_offset_removal", FieldMap, "Phase offset removal", "Remove spatially-varying phase offset using HIP (Eckstein et al., 2018); multi-echo only", Bool).basic().flag("--phase-offset-removal").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit),
        d("field_mapping.phase_offset_sigma_mm", FieldMap, "Sigma (mm)", "Gaussian smoothing sigma in mm (x y z)", FloatVec3).units("mm").flag("--phase-offset-sigma-mm").parent("field_mapping.phase_offset_removal")
        .when(|x| x.combines && x.c.field_mapping.phase_offset_removal && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit).full("Phase offset sigma")
        .gui("Gaussian smoothing sigma (x y z)"),
        d("field_mapping.bipolar_correction", FieldMap, "Bipolar correction", "Remove the linear phase artefact of bipolar readout gradients (needs at least 3 echoes; fewer: skipped)", Bool).basic().flag("--bipolar-correction").parent("field_mapping.phase_offset_removal")
        .when(|x| x.combines && x.c.field_mapping.phase_offset_removal && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit),
        d("field_mapping.romeo.individual", FieldMap, "Individual mode", "Unwrap each echo on its own (for motion or time series). Off = the template echo is unwrapped spatially and the others temporally from it.", Bool).flag("--romeo-individual").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Unwrap echoes individually"),
        // The config counts echoes from 0; the TUI and --romeo-template from 1.
        d("field_mapping.romeo.template", FieldMap, "Template echo", "Echo used for spatial unwrapping, counted from 0 (--romeo-template counts from 1)", Int).flag("--romeo-template").parent("field_mapping.romeo.individual")
        .when(|x| x.romeo && !x.c.field_mapping.romeo.individual)
        .gui("Echo used for spatial unwrapping, counted from 0"),
        d("field_mapping.romeo.correct_global", FieldMap, "Correct global", "Correct inter-echo 2π offsets after unwrapping", Bool).flag("--romeo-correct-global").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Correct global offsets"),
        d("field_mapping.romeo.phase_coherence", FieldMap, "Phase coherence", "Weight edges by neighbour phase coherence", Bool).flag("--no-romeo-phase-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo),
        d("field_mapping.romeo.phase_gradient_coherence", FieldMap, "Phase gradient coherence", "Weight edges by inter-echo phase-gradient consistency (multi-echo)", Bool).flag("--no-romeo-phase-gradient-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo),
        d("field_mapping.romeo.phase_linearity", FieldMap, "Phase linearity", "Weight edges by second-derivative smoothness", Bool).flag("--no-romeo-phase-linearity").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo),
        d("field_mapping.romeo.mag_coherence", FieldMap, "Mag coherence", "Weight edges by neighbor magnitude coherence", Bool).flag("--no-romeo-mag-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Magnitude coherence")
        .gui("Weight edges by neighbour magnitude coherence"),
        d("field_mapping.romeo.mag_weight", FieldMap, "Mag weight", "Penalise low-signal voxels (off by default)", Bool).flag("--romeo-mag-weight").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Magnitude weight")
        .gui("Penalise low-signal voxels"),
        d("field_mapping.romeo.mag_weight2", FieldMap, "Mag weight 2", "Penalise abnormally high signal / flow artefacts (off by default)", Bool).flag("--romeo-mag-weight2").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Magnitude weight 2")
        .gui("Penalise abnormally high signal and flow artefacts"),
        d("field_mapping.romeo.bestpath", FieldMap, "Best-path weights", "Use Abdul-Rahman best-path weights instead of ROMEO weights", Bool).flag("--romeo-bestpath").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo),
        d("field_mapping.romeo.temporal_uncertain_unwrapping", FieldMap, "Temporal uncertain", "Re-unwrap voxels whose temporal unwrapping is uncertain", Float)
        .value("Quality threshold", "Quality threshold (0, 1] below which a voxel is re-unwrapped").off(json!(0.0)).flag("--romeo-temporal-uncertain-unwrapping").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Temporal uncertain unwrapping"),
        d("field_mapping.romeo.max_seeds", FieldMap, "Max seeds", "Maximum number of seed regions (default 1)", Int).flag("--romeo-max-seeds").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Maximum seeds")
        .gui("Maximum number of seed regions"),
        d("field_mapping.romeo.merge_regions", FieldMap, "Merge regions", "Merge neighboring regions after unwrapping", Bool).flag("--romeo-merge-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .gui("Merge neighbouring regions after unwrapping"),
        d("field_mapping.romeo.correct_regions", FieldMap, "Correct regions", "Shift each region's median to nearest 0 by n·2π", Bool).flag("--romeo-correct-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .gui("Shift each region by a multiple of 2π to bring its median nearest 0"),
        d("field_mapping.romeo.wrap_addition", FieldMap, "Wrap addition", "Extra phase tolerance beyond π for neighbor differences [0,π]", Float).flag("--romeo-wrap-addition").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .gui("Extra phase tolerance beyond π for neighbour differences, from 0 to π"),
        // A per-echo QSM runs the field map, background removal, inversion and referencing on
        // each echo on its own: the settings that combine or compare echoes are out of play.
        d("qsm.per_echo", FieldMap, "Per-echo QSM", "Reconstruct a QSM from each echo on its own (each echo's field map, background removal, inversion and referencing) and report their mean as the QSM, with each echo's map, instead of one QSM from the echo-combined field map. Multi-echo only", Bool).basic().flag("--qsm-per-echo")
        .gui("Reconstruct a QSM from each echo on its own and report their mean, instead of one QSM from the echo-combined field map"),
        d("field_mapping.b0_estimation", FieldMap, "B0 estimation", "", Enum(vec![
            o("weighted-avg", "weighted-avg", "Weighted average", "Weighted average of phase/TE across echoes (default)"),
            o("linear-fit", "linear-fit", "Linear fit", "Magnitude-weighted linear fit of phase vs TE"),
            o("robust-fit", "robust-fit", "Robust fit", "Motion-robust linear fit (IRLS): down-weights echoes departing from a straight line, per voxel. Needs 4 echoes (3 without offset)"),
            o("qsmart-echofit", "qsmart-echofit", "QSMART echo fit", "QSMART's: magnitude-weighted fit of phase vs TE through the origin; marks the voxels QSMART leaves out"),
            o("complex-fit", "complex-fit", "Complex fit", "MEDI's field map (Fit_ppm_complex): a magnitude-weighted fit of the complex echoes, then unwrapping of the fitted phase. Needs the magnitude and evenly spaced echoes; no phase offset removal or bipolar correction"),
        ])).basic().flag("--b0-estimation")
        .gui("How the unwrapped echoes are combined into one field map")
        .when(|x| x.combines),
        d("field_mapping.qsmart_fit_threshold_mode", FieldMap, "Fit threshold", "", Enum(vec![
            o("percentile", "percentile", "Percentile", "A percentile of the blurred fit residual over the volume, as the authors' October 2020 demo runs it"),
            o("absolute", "absolute", "Fixed", "A fixed fit residual, as QSMART v1.0 runs it (default)"),
        ])).flag("--qsmart-fit-threshold-mode").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit).full("QSMART fit threshold")
        .gui("How the fit residual at which a voxel is left out of QSMART's reconstruction is set"),
        // QSM.rs utils/qsmart.rs QsmartFitThreshold::validate refuses a percentile outside (0, 100].
        d("field_mapping.qsmart_fit_percentile", FieldMap, "Fit percentile", "Percentile of the blurred fit residual above which a voxel is left out of QSMART's reconstruction (default: 99)", Float).above(0.0).max(100.0).flag("--qsmart-fit-percentile").parent("field_mapping.qsmart_fit_threshold_mode")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit && x.c.field_mapping.qsmart_fit_threshold_mode == QsmartFitThresholdMode::Percentile).full("QSMART fit percentile")
        .gui("Percentile of the blurred fit residual above which a voxel is left out of QSMART's reconstruction"),
        // QSM.rs utils/qsmart.rs QsmartFitThreshold::validate refuses a threshold that is not > 0.
        d("field_mapping.qsmart_fit_threshold", FieldMap, "Fit residual", "Blurred fit residual above which a voxel is left out of QSMART's reconstruction (default: 40)", Float).above(0.0).flag("--qsmart-fit-threshold").parent("field_mapping.qsmart_fit_threshold_mode")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit && x.c.field_mapping.qsmart_fit_threshold_mode == QsmartFitThresholdMode::Absolute).full("QSMART fit residual")
        .gui("Blurred fit residual above which a voxel is left out of QSMART's reconstruction"),
        d("field_mapping.b0_weight_type", FieldMap, "Weight type", "", Enum(vec![
            o("phase-snr", "phase-snr", "Phase SNR", "mag × TE — optimal for phase SNR (default)"),
            o("phase-var", "phase-var", "Phase variance", "mag² × TE² — based on phase variance"),
            o("average", "average", "Uniform", "Uniform weights (unweighted average)"),
            o("tes", "tes", "Echo time", "TE only"),
            o("mag", "mag", "Magnitude", "Magnitude only"),
            o("assumed-decay", "assumed-decay", "Assumed decay", "Fixed TE·exp(−TE/T2*) per echo from an assumed T2* (no T2* map, no magnitude); phase-snr is generally better"),
        ])).flag("--b0-weight-type").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg).full("Echo weighting")
        .gui("How each echo is weighted in the average"),
        // QSMxT fieldmap.rs and QSM.rs pipeline/field_mapping.rs refuse a T2* that is not > 0.
        d("field_mapping.b0_weight_assumed_t2star_ms", FieldMap, "Assumed T2* (ms)", "T2* assumed to compute the fixed echo weights; no T2* map is used (default: 40 ms)", Float).above(0.0).units("ms").flag("--b0-weight-assumed-t2star").parent("field_mapping.b0_weight_type")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg && x.c.field_mapping.b0_weight_type == B0WeightType::AssumedDecay).full("Assumed T2*")
        .gui("T2* assumed to compute the fixed echo weights; no T2* map is used"),
        d("field_mapping.linear_fit.estimate_offset", FieldMap, "Estimate offset", "Fit a phase-offset term in the echo-time fit (robust fit: needs 4 echoes with it, 3 without)", Bool).flag("--linear-fit-estimate-offset").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && matches!(x.c.field_mapping.b0_estimation, B0Estimation::LinearFit | B0Estimation::RobustFit)).context("Echo-time fit"),
        d("field_mapping.robust_fit.loss", FieldMap, "Loss", "", Enum(vec![
            o("tukey", "tukey", "Tukey", "Tukey biweight: rejects an echo at a voxel past the tuning radius (default)"),
            o("huber", "huber", "Huber", "Huber: caps an outlying echo's influence but never rejects it"),
        ])).flag("--robust-fit-loss").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Loss function")
        .gui("The loss that down-weights an echo departing from the straight line through the others"),
        // QSM.rs RobustFitParams::validate: tuning must be > 0.
        d("field_mapping.robust_fit.tuning", FieldMap, "Tuning", "Tuning radius in robust-scale units; blank = conventional (Tukey 4.685, Huber 1.345)", Float).above(0.0).optional().flag("--robust-fit-tuning").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Tuning radius")
        .gui("Tuning radius in robust-scale units; empty for the conventional value (Tukey 4.685, Huber 1.345)"),
        d("field_mapping.robust_fit.iterations", FieldMap, "Iterations", "IRLS passes (0 = report only: the field map is the linear fit's)", Int).flag("--robust-fit-iterations").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Reweighting passes")
        .gui("Iteratively reweighted passes (0 reports only: the field map is the linear fit's)"),
        // QSM.rs RobustFitParams::validate: downweight level in (0, 1].
        d("field_mapping.robust_fit.downweight_level", FieldMap, "Downweight level", "Reporting only: a weight below this counts as down-weighted (0, 1]", Float).above(0.0).max(1.0).flag("--robust-fit-downweight-level").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit)
        .gui("Reporting only: a weight below this counts as down-weighted"),
        // QSM.rs RobustFitParams::validate: flag ratio >= 1.
        d("field_mapping.robust_fit.flag_ratio", FieldMap, "Flag ratio", "Reporting only: flag an echo whose outlier score exceeds this x the best echo's (>= 1)", Float).min(1.0).flag("--robust-fit-flag-ratio").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit)
        .gui("Reporting only: flag an echo whose outlier score exceeds this multiple of the best echo's"),
        // QSM.rs fit_ppm_complex (utils/multi_echo.rs) refuses a reltol that is not >= 0.
        d("field_mapping.complex_fit.reltol", FieldMap, "Rel tolerance", "Stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4)", Float).min(0.0).flag("--complex-fit-reltol").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::ComplexFit).full("Relative tolerance")
        .gui("The fit stops once its update is this fraction of the first estimate, over the whole volume"),
        // 0 keeps the fit's starting estimate (a linear fit to the first three echoes' phase).
        d("field_mapping.complex_fit.max_iter", FieldMap, "Max iter", "Gauss-Newton iterations at most (default 30)", Int).flag("--complex-fit-max-iter").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::ComplexFit).full("Maximum iterations")
        .gui("Gauss-Newton iterations at most"),
    ]
}
