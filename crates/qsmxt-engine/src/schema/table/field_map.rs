//! The Field map section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    [
        unwrapping(),
        phase_offsets(),
        romeo(),
        echo_combination(),
        echo_fits(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn unwrapping() -> Vec<Def> {
    vec![
        d("field_mapping.unwrapping_algorithm", FieldMap, "Unwrapping", "", Enum(vec![
            o("romeo", "romeo", "ROMEO", "ROMEO region-growing unwrapping — https://doi.org/10.1002/mrm.28563"),
            o("laplacian", "laplacian", "Laplacian", "Laplacian phase unwrapping (FFT-based) — https://doi.org/10.1364/OL.28.001194"),
            o("prelude", "prelude", "PRELUDE", "PRELUDE region-merging unwrapping, each echo on its own — https://doi.org/10.1002/mrm.10354"),
            o("medi-region-growing", "medi-region-growing", "MEDI region growing", "MEDI's magnitude-guided region-growing unwrapping; needs magnitude images — https://doi.org/10.1002/mrm.24272"),
        ])).basic().flag("--unwrapping-algorithm").full("Phase unwrapping")
        .gui("How the wrapped phase is unwrapped")
        .cli("Unwrapping algorithm"),
        d("field_mapping.prelude.labelling", FieldMap, "Labelling", "", Enum(vec![
            o("auto", "auto", "Auto", "As FSL prelude chooses: slice by slice above 64 voxels in-plane, else over the whole volume (default)"),
            o("slices", "slices", "Slices", "Label regions slice by slice (FSL prelude --labelslices)"),
            o("volume", "volume", "Volume", "Label regions over the whole volume (FSL prelude -f)"),
        ])).flag("--prelude-labelling").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.prelude).full("PRELUDE labelling")
        .gui("How PRELUDE labels its initial regions")
        .cli("PRELUDE: how the initial regions are labelled")
        .value_help("auto", "FSL prelude's default: slice by slice above 64 voxels in-plane, else over the whole volume").value_help("slices", "Label regions slice by slice (FSL prelude --labelslices)").value_help("volume", "Label regions over the whole volume (FSL prelude -f)"),
        d("field_mapping.prelude.num_phase_partitions", FieldMap, "Phase partitions", "Number of phase bins the initial regions are formed from (default 8)", Int).min(1.0).flag("--prelude-partitions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.prelude).full("PRELUDE phase partitions")
        .gui("Number of phase bins the initial regions are formed from")
        .cli("PRELUDE: number of phase partitions the initial regions are formed from")
        .parse(P::PositiveU64).value_name("N"),
        d("field_mapping.laplacian_solver", FieldMap, "Solver", "", Enum(vec![
            o("dct", "dct", "DCT", "Unweighted least squares, Neumann-boundary DCT solve (Ghiglia & Romero 1994; QSM.jl :dct) (default)"),
            o("fft", "fft", "FFT", "Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume"),
            o("mirror", "mirror", "Mirror", "Schofield & Zhu sin/cos Laplacian, FFT solve on the volume mirrored along each axis (QSMART's reference)"),
            o("mgpcg", "mgpcg", "MGPCG", "MGPCG (QSM.jl): Laplacian unwrapping with LBV background removal in one step; QSM.jl follows it with V-SHARP to remove remaining background"),
        ])).flag("--laplacian-solver").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian).full("Laplacian solver")
        .gui("How Laplacian unwrapping's Poisson equation is solved")
        .cli("Poisson solver for `--unwrapping-algorithm laplacian`: `dct` (unweighted least squares, Neumann/DCT solve; default), `fft` (Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume), `mirror` (the same on the volume mirrored along each axis, as QSMART unwraps) or `mgpcg` (QSM.jl's MGPCG: Laplacian unwrapping with LBV background removal in one step; QSM.jl follows it with V-SHARP to remove remaining background)")
        .value_alias("fft", &["sti"]).flag_aliases(&["laplacian-kernel"])
        .value_help("mirror", "FFT solve on the volume mirrored along each axis, as QSMART unwraps").value_help("mgpcg", "QSM.jl's MGPCG: Laplacian unwrapping with LBV background removal in one step"),
        d("field_mapping.laplacian_fft_pad", FieldMap, "FFT pad (voxels)", "Zero-padding per side: one value for every axis or three (x y z). Default: 64", IntVec3).units("voxels").flag("--laplacian-fft-pad").parent("field_mapping.laplacian_solver")
        .when(|x| x.laplacian && x.c.field_mapping.laplacian_solver == LaplacianSolver::Fft).full("Laplacian FFT padding")
        .gui("Zero-padding per side: one value for every axis, or three (x y z)")
        .cli("Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or three (x y z). Default 64")
        .num_args(1, 3).value_name("VOXELS"),
        d("field_mapping.laplacian_unmasked", FieldMap, "Unmasked phase", "Unwrap the raw phase over the whole volume, as QSMART does, instead of the phase set to zero outside the brain mask", Bool).flag("--laplacian-unmasked").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian).full("Laplacian unwrapping of the raw phase")
        .cli("`--unwrapping-algorithm laplacian`: unwrap the raw phase over the whole volume, instead of the phase set to zero outside the brain mask, as QSMART does. With `--b0-estimation qsmart-echofit` the field map is then kept over the whole volume")
        .negated("--no-laplacian-unmasked", "`--unwrapping-algorithm laplacian`: unwrap the phase set to zero outside the brain mask (the default), after a `--pipeline` that unwraps the whole volume").negated_conflicts(),
        d("field_mapping.laplacian_keep_outside", FieldMap, "Keep field outside", "Keep the field over the whole volume, not only inside the brain mask, as UK Biobank's pipeline hands it to V-SHARP (weighted-average combination)", Bool).flag("--laplacian-keep-outside").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.laplacian && !x.c.field_mapping.laplacian_unmasked && x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg)
        .full("Laplacian field kept outside the mask")
        .cli("`--unwrapping-algorithm laplacian` with a weighted-average echo combination: keep the field over the whole volume, not only inside the brain mask, as UK Biobank's pipeline hands it to V-SHARP")
        .negated("--no-laplacian-keep-outside", "Keep the Laplacian field inside the brain mask only (the default), after a `--pipeline` that keeps it over the whole volume").negated_conflicts(),
    ]
}

fn phase_offsets() -> Vec<Def> {
    vec![
        // MEDI's complex fit fits its own phase offset: QSM.rs does not remove them first.
        d("field_mapping.phase_offset_removal", FieldMap, "Phase offset removal", "Remove the echoes' spatially varying phase offset with MCPC-3D-S (Eckstein et al., 2018) before unwrapping; multi-echo only", Bool).basic().flag("--phase-offset-removal").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit)
        .cli("Remove the phase offset of multi-echo data with MCPC-3D-S (Eckstein et al., 2018) before unwrapping")
        .bool_value(),
        d("field_mapping.phase_offset_sigma_mm", FieldMap, "Sigma (mm)", "Gaussian smoothing sigma in mm (x y z)", FloatVec3).units("mm").flag("--phase-offset-sigma-mm").parent("field_mapping.phase_offset_removal")
        .when(|x| x.combines && x.c.field_mapping.phase_offset_removal && x.c.field_mapping.b0_estimation != B0Estimation::ComplexFit).full("Phase offset sigma")
        .gui("Gaussian smoothing sigma (x y z)")
        .cli("Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's)"),
        d("field_mapping.bipolar_correction", FieldMap, "Bipolar correction", "Remove the phase artefact of bipolar (alternating-polarity) readout gradients (needs at least 3 echoes; fewer: skipped). With the complex fit, MEDI's bipolar echo fit is used instead", Bool).basic().flag("--bipolar-correction").parent("field_mapping.unwrapping_algorithm")
        // With the complex fit (no offset removal to sit under) it is MEDI's bipolar fit.
        .when(|x| x.combines && (x.c.field_mapping.phase_offset_removal || x.c.field_mapping.b0_estimation == B0Estimation::ComplexFit))
        .cli("Enable bipolar gradient correction (requires >= 3 echoes)"),
    ]
}

fn romeo() -> Vec<Def> {
    vec![
        d("field_mapping.romeo.individual", FieldMap, "Individual mode", "Unwrap each echo on its own (for motion or time series). Off = the template echo is unwrapped spatially and the others temporally from it.", Bool).flag("--romeo-individual").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Unwrap echoes individually")
        .cli("ROMEO: unwrap each echo on its own (individual mode, for motion or time series)")
        .negated("--no-romeo-individual", "ROMEO: unwrap the template echo spatially and the others temporally from it (default)"),
        // The config counts echoes from 0; the TUI and --romeo-template from 1.
        d("field_mapping.romeo.template", FieldMap, "Template echo", "Echo used for spatial unwrapping, counted from 0 (--romeo-template counts from 1)", Int).flag("--romeo-template").parent("field_mapping.romeo.individual")
        .when(|x| x.romeo && !x.c.field_mapping.romeo.individual)
        .gui("Echo used for spatial unwrapping, counted from 0")
        .cli("ROMEO: template echo index (1-indexed, only for template mode)").cli_default("1"),
        d("field_mapping.romeo.correct_global", FieldMap, "Correct global", "Correct inter-echo 2π offsets after unwrapping", Bool).flag("--romeo-correct-global").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Correct global offsets")
        .cli("ROMEO: correct global 2π offsets, of each echo and between echoes (default: off)")
        .negated("--no-romeo-correct-global", "ROMEO: do not correct global 2π offsets (default)"),
        d("field_mapping.romeo.phase_coherence", FieldMap, "Phase coherence", "Weight edges by neighbour phase coherence", Bool).flag("--no-romeo-phase-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .cli("ROMEO: disable phase coherence weights (default: on)"),
        d("field_mapping.romeo.phase_gradient_coherence", FieldMap, "Phase gradient coherence", "Weight edges by inter-echo phase-gradient consistency (multi-echo)", Bool).flag("--no-romeo-phase-gradient-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .cli("ROMEO: disable phase gradient coherence weights (default: on)"),
        d("field_mapping.romeo.phase_linearity", FieldMap, "Phase linearity", "Weight edges by second-derivative smoothness", Bool).flag("--no-romeo-phase-linearity").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .cli("ROMEO: disable phase linearity weights (default: on; the romeo command line's romeo3 weights)")
        .negated("--romeo-phase-linearity", "ROMEO: enable phase linearity weights (default: on; for a --pipeline that turns them off)").negated_conflicts(),
        d("field_mapping.romeo.mag_coherence", FieldMap, "Mag coherence", "Weight edges by neighbor magnitude coherence", Bool).flag("--no-romeo-mag-coherence").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Magnitude coherence")
        .gui("Weight edges by neighbour magnitude coherence")
        .cli("ROMEO: disable magnitude coherence weights (default: on)"),
        d("field_mapping.romeo.mag_weight", FieldMap, "Mag weight", "Penalise low-signal voxels (off by default)", Bool).flag("--romeo-mag-weight").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Magnitude weight")
        .gui("Penalise low-signal voxels")
        .cli("ROMEO: enable magnitude weighting (default: off)")
        .negated("--no-romeo-mag-weight", "ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility)"),
        d("field_mapping.romeo.mag_weight2", FieldMap, "Mag weight 2", "Penalise abnormally high signal / flow artefacts (off by default)", Bool).flag("--romeo-mag-weight2").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Magnitude weight 2")
        .gui("Penalise abnormally high signal and flow artefacts")
        .cli("ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off)"),
        d("field_mapping.romeo.bestpath", FieldMap, "Best-path weights", "Use Abdul-Rahman best-path weights instead of ROMEO weights", Bool).flag("--romeo-bestpath").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .cli("ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off)"),
        d("field_mapping.romeo.temporal_uncertain_unwrapping", FieldMap, "Temporal uncertain", "Re-unwrap voxels whose temporal unwrapping is uncertain", Float)
        .value("Quality threshold", "Quality threshold (0, 1] below which a voxel is re-unwrapped").off(json!(0.0)).flag("--romeo-temporal-uncertain-unwrapping").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Temporal uncertain unwrapping")
        .cli("ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0.5)"),
        d("field_mapping.romeo.max_seeds", FieldMap, "Max seeds", "Maximum number of seed regions (default 1)", Int).flag("--romeo-max-seeds").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo).full("Maximum seeds")
        .gui("Maximum number of seed regions")
        .parse(P::U8),
        d("field_mapping.romeo.merge_regions", FieldMap, "Merge regions", "Merge neighboring regions after unwrapping", Bool).flag("--romeo-merge-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .gui("Merge neighbouring regions after unwrapping")
        .cli("ROMEO: merge neighboring regions after unwrapping (default: off)"),
        d("field_mapping.romeo.correct_regions", FieldMap, "Correct regions", "Shift each region's median to nearest 0 by n·2π", Bool).flag("--romeo-correct-regions").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .gui("Shift each region by a multiple of 2π to bring its median nearest 0")
        .cli("ROMEO: correct each region's median to nearest 0 (default: off)"),
        d("field_mapping.romeo.wrap_addition", FieldMap, "Wrap addition", "Extra phase tolerance beyond π for neighbor differences [0,π]", Float).flag("--romeo-wrap-addition").parent("field_mapping.unwrapping_algorithm")
        .when(|x| x.romeo)
        .gui("Extra phase tolerance beyond π for neighbour differences, from 0 to π")
        .cli("ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0)"),
    ]
}

fn echo_combination() -> Vec<Def> {
    vec![
        // A per-echo QSM runs the field map, background removal, inversion and referencing on
        // each echo on its own: the settings that combine or compare echoes are out of play.
        d("qsm.per_echo", FieldMap, "Per-echo QSM", "Reconstruct a QSM from each echo on its own (each echo's field map, background removal, inversion and referencing) and report their mean as the QSM, with each echo's map, instead of one QSM from the echo-combined field map. Multi-echo only", Bool).basic().flag("--qsm-per-echo")
        .gui("Reconstruct a QSM from each echo on its own and report their mean, instead of one QSM from the echo-combined field map")
        .cli("Reconstruct a QSM from each echo on its own (each echo's phase unwrapped and divided by its echo time, then background removal, dipole inversion and referencing per echo) and publish each echo's map (`desc-echo<n>_Chimap`) and their mean as the QSM (`Chimap`), instead of one QSM from the echo-combined field map. The echo combination (--b0-estimation), phase-offset removal and bipolar correction do not run. DECOMPOSE takes the echoes' maps. Multi-echo only; not with TGV, QSMART, iQSM, iQFM, two-pass, or χ-separation methods that fit the local field")
        .bool_value().value_name("BOOL"),
        d("field_mapping.b0_estimation", FieldMap, "B0 estimation", "", Enum(vec![
            o("weighted-avg", "weighted-avg", "Weighted average", "Weighted average of phase/TE across echoes (default)"),
            o("linear-fit", "linear-fit", "Linear fit", "Magnitude-weighted linear fit of phase vs TE"),
            o("robust-fit", "robust-fit", "Robust fit", "Motion-robust linear fit (IRLS): down-weights echoes departing from a straight line, per voxel. Needs 4 echoes (3 without offset)"),
            o("qsmart-echofit", "qsmart-echofit", "QSMART echo fit", "QSMART's: magnitude-weighted fit of phase vs TE through the origin; marks the voxels QSMART leaves out"),
            o("complex-fit", "complex-fit", "Complex fit", "MEDI's field map (Fit_ppm_complex): a magnitude-weighted fit of the complex echoes, then unwrapping of the fitted phase. Needs the magnitude and evenly spaced echoes; no phase offset removal or bipolar correction"),
        ])).basic().flag("--b0-estimation")
        .gui("How the unwrapped echoes are combined into one field map")
        .when(|x| x.combines)
        .cli("B0 field estimation method")
        .value_help("robust-fit", "Motion-robust (IRLS) linear fit: down-weights echoes that depart from a straight line in TE, voxel by voxel. Needs 4 echoes (3 with --linear-fit-estimate-offset false)").value_help("qsmart-echofit", "QSMART's echo fit: a magnitude-weighted fit of the unwrapped phase vs TE through the origin, which also marks the voxels QSMART leaves out").value_help("complex-fit", "MEDI's field map (Fit_ppm_complex): a magnitude-weighted fit of the complex echoes, then unwrapping of the fitted phase per echo spacing. Needs the magnitude and evenly spaced echoes; no phase offset removal is run first, and bipolar correction is refused"),
        d("field_mapping.qsmart_fit_threshold_mode", FieldMap, "Fit threshold", "", Enum(vec![
            o("percentile", "percentile", "Percentile", "A percentile of the blurred fit residual over the volume, as the authors' October 2020 demo runs it"),
            o("absolute", "absolute", "Fixed", "A fixed fit residual, as QSMART v1.0 runs it (default)"),
        ])).flag("--qsmart-fit-threshold-mode").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit).full("QSMART fit threshold")
        .gui("How the fit residual at which a voxel is left out of QSMART's reconstruction is set")
        .cli("`--b0-estimation qsmart-echofit`: how the fit residual at which a voxel is left out of QSMART's reconstruction is set: a percentile of the blurred residual over the volume (`--qsmart-fit-percentile`), or a fixed residual (`--qsmart-fit-threshold`; default, QSMART v1.0's 40). Without it, giving one of those two selects its mode"),
        // QSM.rs utils/qsmart.rs QsmartFitThreshold::validate refuses a percentile outside (0, 100].
        d("field_mapping.qsmart_fit_percentile", FieldMap, "Fit percentile", "Percentile of the blurred fit residual above which a voxel is left out of QSMART's reconstruction (default: 99)", Float).above(0.0).max(100.0).flag("--qsmart-fit-percentile").parent("field_mapping.qsmart_fit_threshold_mode")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit && x.c.field_mapping.qsmart_fit_threshold_mode == QsmartFitThresholdMode::Percentile).full("QSMART fit percentile")
        .gui("Percentile of the blurred fit residual above which a voxel is left out of QSMART's reconstruction")
        .cli("`--b0-estimation qsmart-echofit`: the percentile of the blurred fit residual, in (0, 100], above which a voxel is left out of QSMART's reconstruction"),
        // QSM.rs utils/qsmart.rs QsmartFitThreshold::validate refuses a threshold that is not > 0.
        d("field_mapping.qsmart_fit_threshold", FieldMap, "Fit residual", "Blurred fit residual above which a voxel is left out of QSMART's reconstruction (default: 40)", Float).above(0.0).flag("--qsmart-fit-threshold").parent("field_mapping.qsmart_fit_threshold_mode")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit && x.c.field_mapping.qsmart_fit_threshold_mode == QsmartFitThresholdMode::Absolute).full("QSMART fit residual")
        .gui("Blurred fit residual above which a voxel is left out of QSMART's reconstruction")
        .cli("`--b0-estimation qsmart-echofit`: a fixed blurred fit residual above which a voxel is left out of QSMART's reconstruction, instead of a percentile (40 when the mode is `absolute` and none is given)"),
        d("field_mapping.b0_weight_type", FieldMap, "Weight type", "", Enum(vec![
            o("phase-snr", "phase-snr", "Phase SNR", "mag × TE — optimal for phase SNR (default)"),
            o("phase-var", "phase-var", "Phase variance", "mag² × TE² — based on phase variance"),
            o("average", "average", "Uniform", "Uniform weights (unweighted average)"),
            o("tes", "tes", "Echo time", "TE only"),
            o("mag", "mag", "Magnitude", "Magnitude only"),
            o("assumed-decay", "assumed-decay", "Assumed decay", "Fixed TE·exp(−TE/T2*) per echo from an assumed T2* (no T2* map, no magnitude); phase-snr is generally better"),
        ])).flag("--b0-weight-type").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg).full("Echo weighting")
        .gui("How each echo is weighted in the average")
        .cli("B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2*) computed from the echo times and an assumed T2* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO"),
        // QSMxT fieldmap.rs and QSM.rs pipeline/field_mapping.rs refuse a T2* that is not > 0.
        d("field_mapping.b0_weight_assumed_t2star_ms", FieldMap, "Assumed T2* (ms)", "T2* assumed to compute the fixed echo weights; no T2* map is used (default: 40 ms)", Float).above(0.0).units("ms").flag("--b0-weight-assumed-t2star").parent("field_mapping.b0_weight_type")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::WeightedAvg && x.c.field_mapping.b0_weight_type == B0WeightType::AssumedDecay).full("Assumed T2*")
        .gui("T2* assumed to compute the fixed echo weights; no T2* map is used")
        .cli("T2* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2* map is used)")
        .value_name("MS"),
    ]
}

fn echo_fits() -> Vec<Def> {
    vec![
        d("field_mapping.linear_fit.estimate_offset", FieldMap, "Estimate offset", "Fit a phase-offset term in the echo-time fit (robust fit: needs 4 echoes with it, 3 without)", Bool).flag("--linear-fit-estimate-offset").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && matches!(x.c.field_mapping.b0_estimation, B0Estimation::LinearFit | B0Estimation::RobustFit)).context("Echo-time fit")
        .cli("Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without")
        .bool_value(),
        d("field_mapping.robust_fit.loss", FieldMap, "Loss", "", Enum(vec![
            o("tukey", "tukey", "Tukey", "Tukey biweight: rejects an echo at a voxel past the tuning radius (default)"),
            o("huber", "huber", "Huber", "Huber: caps an outlying echo's influence but never rejects it"),
        ])).flag("--robust-fit-loss").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Loss function")
        .gui("The loss that down-weights an echo departing from the straight line through the others")
        .cli("Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects"),
        // QSM.rs RobustFitParams::validate: tuning must be > 0.
        d("field_mapping.robust_fit.tuning", FieldMap, "Tuning", "Tuning radius in robust-scale units; blank = conventional (Tukey 4.685, Huber 1.345)", Float).above(0.0).optional().flag("--robust-fit-tuning").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Tuning radius")
        .gui("Tuning radius in robust-scale units; empty for the conventional value (Tukey 4.685, Huber 1.345)")
        .cli("Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive"),
        d("field_mapping.robust_fit.iterations", FieldMap, "Iterations", "IRLS passes (0 = report only: the field map is the linear fit's)", Int).flag("--robust-fit-iterations").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit).full("Reweighting passes")
        .gui("Iteratively reweighted passes (0 reports only: the field map is the linear fit's)")
        .cli("Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's)"),
        // QSM.rs RobustFitParams::validate: downweight level in (0, 1].
        d("field_mapping.robust_fit.downweight_level", FieldMap, "Downweight level", "Reporting only: a weight below this counts as down-weighted (0, 1]", Float).above(0.0).max(1.0).flag("--robust-fit-downweight-level").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit)
        .gui("Reporting only: a weight below this counts as down-weighted")
        .cli("Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1])"),
        // QSM.rs RobustFitParams::validate: flag ratio >= 1.
        d("field_mapping.robust_fit.flag_ratio", FieldMap, "Flag ratio", "Reporting only: flag an echo whose outlier score exceeds this x the best echo's (>= 1)", Float).min(1.0).flag("--robust-fit-flag-ratio").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::RobustFit)
        .gui("Reporting only: flag an echo whose outlier score exceeds this multiple of the best echo's")
        .cli("Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1)"),
        // QSM.rs fit_ppm_complex (utils/multi_echo.rs) refuses a reltol that is not >= 0.
        d("field_mapping.complex_fit.reltol", FieldMap, "Rel tolerance", "Stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4)", Float).min(0.0).flag("--complex-fit-reltol").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::ComplexFit).full("Relative tolerance")
        .gui("The fit stops once its update is this fraction of the first estimate, over the whole volume")
        .cli("Complex fit (`--b0-estimation complex-fit`): stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4)"),
        // 0 keeps the fit's starting estimate (a linear fit to the first three echoes' phase).
        d("field_mapping.complex_fit.max_iter", FieldMap, "Max iter", "Gauss-Newton iterations at most (default 30)", Int).flag("--complex-fit-max-iter").parent("field_mapping.b0_estimation")
        .when(|x| x.combines && x.c.field_mapping.b0_estimation == B0Estimation::ComplexFit).full("Maximum iterations")
        .gui("Gauss-Newton iterations at most"),
    ]
}
