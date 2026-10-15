//! Masking: the brain mask from its sections, a supplied mask brought onto the working grid, and
//! the reliable-phase mask of a two-pass run.

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::masking::{MaskCombine, MaskSection};
use crate::scan::{scan_metadata, RunMetadata};
use crate::stages::magnitude::homogeneity_correct;

/// The `mask` step's parameters.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    serde_json::json!({
        "sections": config.masking.sections.iter().map(|s| format!("{}", s)).collect::<Vec<_>>(),
        "combine": format!("{}", config.masking.combine),
        "refinements": config.masking.refinements.iter().map(|o| format!("{}", o)).collect::<Vec<_>>(),
        "custom_mask_tool": config.masking.custom_mask_tool,
    })
}

/// The reliable-phase mask step's parameters (`mask` + [`crate::cache::RELIABLE_SUFFIX`]).
pub fn reliable_params(config: &PipelineConfig) -> serde_json::Value {
    let sections = config.masking.resolved_two_pass_sections();
    serde_json::json!({
        "sections": sections.iter().map(|s| format!("{}", s)).collect::<Vec<_>>(),
        "combine": format!("{}", config.masking.combine),
        "refinements": config.masking.refinements.iter().map(|o| format!("{}", o)).collect::<Vec<_>>(),
    })
}

/// The deep-learning models (HD-BET, RS2-Net, MindGrab) `sections` run, by model id: the weights
/// to have ready before [`build_mask`].
pub fn model_ids(sections: &[MaskSection], config: &PipelineConfig) -> Vec<&'static str> {
    crate::bridge::to_mask_sections(sections, &config.bet).iter()
        .flat_map(|s| s.all_ops())
        .filter_map(|op| op.dl_model_id())
        .collect()
}

/// Build a mask from `sections`, folded with the configured combine mode and refinements. The
/// main mask and the two-pass reliable mask are both built here, so both read the same images
/// and follow the same folding rules.
///
/// `phases` are every echo's phase in radians (for phase-based inputs such as phase quality).
/// `magnitudes` are every echo's magnitude, or none for a phase-only run. Each echo is corrected
/// for inhomogeneity on its own when `--inhomogeneity-correction` is on, and each section builds
/// the magnitude it asks for (echo 1, the RSS, the last echo) from these in qsm-core, so all
/// sections see the same, consistently corrected echoes (QSM.rs#173).
pub fn build_mask(
    sections: &[MaskSection],
    config: &PipelineConfig,
    phases: &[&[f64]],
    magnitudes: Vec<Vec<f64>>,
    meta: &RunMetadata,
) -> Result<Vec<u8>, EngineError> {
    let magnitudes = if config.masking.inhomogeneity_correction {
        magnitudes.into_iter().enumerate()
            .map(|(i, m)| homogeneity_correct(&m, meta, &config.homogeneity, &format!("echo {}'s magnitude", i + 1)))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        magnitudes
    };
    let core_sections = crate::bridge::to_mask_sections(sections, &config.bet);
    let core_refinements = crate::bridge::to_mask_ops(&config.masking.refinements, &config.bet);
    let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
    combine_sections(
        &core_sections, config.masking.combine, &core_refinements, phases, &mag_refs,
        &scan_metadata(meta),
    ).map_err(|e| EngineError::Config(format!("masking: {}", e)))
}

/// Build each mask section, fold them together with `combine`, then apply `refinements` to the
/// result.
///
/// `combine` only matters with more than one section: `or` keeps a voxel any section keeps
/// (the union qsm-core's `run_masking` produces), `and` keeps only voxels every section keeps.
/// The refinements run on the combined mask, which is where an intersection's holes get filled —
/// they are checked to be non-generators before we get here, so the image they see does not
/// matter; the magnitude is passed for `signal-erode`.
pub fn combine_sections(
    sections: &[qsm_core::pipeline::config::MaskSection],
    combine: MaskCombine,
    refinements: &[qsm_core::pipeline::config::MaskOp],
    phases: &[&[f64]],
    magnitudes: &[&[f64]],
    meta: &qsm_core::pipeline::config::ScanMetadata,
) -> Result<Vec<u8>, qsm_core::pipeline::PipelineError> {
    use qsm_core::pipeline::PipelineError;

    let mut combined: Option<Vec<u8>> = None;
    for section in sections {
        let mask = qsm_core::pipeline::build_mask_section(section, phases, magnitudes, meta)?;
        combined = Some(match combined {
            None => mask,
            Some(mut acc) => { combine.accumulate(&mut acc, &mask); acc }
        });
    }
    let combined = combined.ok_or_else(|| PipelineError::InvalidConfig("no mask sections configured".into()))?;

    if refinements.is_empty() {
        return Ok(combined);
    }
    // The refinements run on the combined mask, so they see the RSS magnitude (what
    // `signal-erode` gates on); they are not generators, so the input image does not matter.
    let magnitude = qsm_core::pipeline::section_magnitude(
        qsm_core::pipeline::config::MaskingInput::Magnitude, magnitudes, meta)?;
    qsm_core::pipeline::apply_mask_ops(
        combined, refinements, magnitude.as_deref().unwrap_or(&[]), magnitude.as_deref(), meta)
}

/// A supplied mask (`data` on its own grid, `dims` and `affine`) on the run's working grid:
/// binarised at 0.5 and, when its grid is not the working grid, resampled onto the axial grid by
/// nearest neighbour. `None` when it still does not fit the working grid.
pub fn conform_custom_mask(
    data: &[f64], dims: (usize, usize, usize), affine: &[f64; 16], meta: &RunMetadata,
) -> Option<Vec<u8>> {
    let raw: Vec<u8> = data.iter().map(|&v| if v > 0.5 { 1u8 } else { 0u8 }).collect();
    let mask = if dims == meta.dims {
        raw
    } else {
        qsm_core::geometry::resample_mask_to_axial(&raw, dims.0, dims.1, dims.2, affine)
    };
    let (nx, ny, nz) = meta.dims;
    (mask.len() == nx * ny * nz).then_some(mask)
}

/// The reliable-phase mask of a two-pass run (`reliable`, built from its own sections by
/// [`build_mask`]), restricted to the main mask.
///
/// The restriction is not optional: the reliable mask decides where its pass's values are
/// preferred, so a section reaching past the brain mask would promote a reconstruction over a
/// region the run excluded outright. Logs how much of the brain it keeps, and warns when that
/// leaves two-pass with nothing useful to do ([`coverage_advice`]).
pub fn reliable_mask(reliable: &[u8], main: &[u8]) -> Result<Vec<u8>, EngineError> {
    let restricted = qsm_core::pipeline::restrict_reliable_mask(reliable, main)
        .map_err(|e| EngineError::Config(format!("reliable mask: {}", e)))?;

    let (kept, brain) = (
        restricted.iter().filter(|&&v| v != 0).count(),
        main.iter().filter(|&&v| v != 0).count(),
    );
    log::info!("Reliable-phase mask: {kept} of {brain} brain voxels ({:.1}%)",
               100.0 * kept as f64 / brain.max(1) as f64);
    if let Some(advice) = coverage_advice(kept, brain) {
        log::warn!("{advice}");
    }
    Ok(restricted)
}

/// Warn when the reliable mask leaves two-pass with nothing useful to do.
///
/// The holes are what the method works with, so a reliable mask that has (almost) none is a
/// second reconstruction spent reproducing the first, and one with no voxels at all produces a
/// combined map identical to the single-pass one. Neither is an error — the user may be
/// exploring — but neither is worth the run time in silence. The 99.9% tolerance is there because
/// a slightly-too-loose threshold leaves a handful of voxels rather than exactly zero.
pub fn coverage_advice(kept: usize, brain: usize) -> Option<&'static str> {
    if brain == 0 {
        return None;
    }
    if kept == 0 {
        return Some("The reliable-phase mask is empty, so the two-pass output will be identical \
                     to the single-pass one. Loosen --two-pass-mask, or turn two-pass off.");
    }
    if kept as f64 >= 0.999 * brain as f64 {
        return Some("The reliable-phase mask covers essentially the whole brain mask, so \
                     two-pass has almost nothing to reduce — it will spend a second \
                     reconstruction reproducing the first. Tighten --two-pass-mask (and drop any \
                     hole-filling from it), or turn two-pass off.");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::masking::{MaskOp, MaskThresholdMethod, MaskingInput};

    /// A reliable mask with no holes, or no voxels, makes the second reconstruction pointless —
    /// and both are easy to produce by mistake with a slightly-wrong threshold.
    #[test]
    fn two_pass_coverage_advice_flags_a_useless_reliable_mask() {
        // A mask with real holes is the working case: no advice.
        assert!(coverage_advice(800, 1000).is_none());
        assert!(coverage_advice(997, 1000).is_none());

        // Essentially the whole brain — including the handful-of-voxels case that is not
        // exactly equal but is just as useless.
        assert!(coverage_advice(1000, 1000).unwrap().contains("almost nothing"));
        assert!(coverage_advice(999, 1000).unwrap().contains("almost nothing"));

        assert!(coverage_advice(0, 1000).unwrap().contains("empty"));
        // No brain mask at all is someone else's error to report.
        assert!(coverage_advice(0, 0).is_none());
    }

    /// The chain the warning exists for, end to end on real masking: a reliable recipe that fills
    /// its holes produces a mask covering the whole brain, and the advice fires on it — while the
    /// default recipe keeps its holes and stays silent.
    ///
    /// The pieces were tested separately before; this is the join between them, which is where the
    /// feature would quietly become a no-op that costs a second reconstruction.
    #[test]
    fn hole_filling_collapses_the_reliable_mask_onto_the_brain() {
        // A 12³ block of bright signal with a dark 3³ void inside it — a strong susceptibility
        // source as a quality map sees it.
        let dims = (12usize, 12, 12);
        let (nx, ny, nz) = dims;
        let mut quality = vec![1.0f64; nx * ny * nz];
        for z in 4..7 { for y in 4..7 { for x in 4..7 {
            quality[z * nx * ny + y * nx + x] = 0.0;
        }}}
        let meta = crate::bridge::to_scan_metadata(dims, (1.0, 1.0, 1.0), &[0.005], 3.0, (0.0, 0.0, 1.0), None);

        let recipe = |refinements: Vec<MaskOp>| crate::bridge::to_mask_sections(&[MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.5) },
            refinements,
        }], &Default::default());
        let build = |refinements: Vec<MaskOp>| {
            combine_sections(&recipe(refinements), MaskCombine::Or, &[], &[], &[&quality], &meta).unwrap()
        };
        let count = |m: &[u8]| m.iter().filter(|&&v| v != 0).count();

        // The brain mask, standing in for the main pass: holes filled.
        let brain = build(vec![MaskOp::FillHoles { max_size: 0 }]);
        assert_eq!(count(&brain), nx * ny * nz, "the filled mask should have swallowed the void");

        // The default reliable recipe keeps the void, so the two passes reconstruct different
        // regions and the advice stays quiet.
        let unfilled = build(vec![]);
        assert_eq!(count(&unfilled), nx * ny * nz - 27, "the void is what the reliable pass excludes");
        assert!(coverage_advice(count(&unfilled), count(&brain)).is_none());

        // Fill the holes in the *reliable* mask and it becomes the brain mask — a second
        // reconstruction of the same region.
        let filled = build(vec![MaskOp::FillHoles { max_size: 0 }]);
        assert_eq!(count(&filled), count(&brain));
        let advice = coverage_advice(count(&filled), count(&brain))
            .expect("a reliable mask with no holes must be flagged");
        assert!(advice.contains("almost nothing to reduce"), "{advice}");
    }

    /// Two magnitude sections with fixed thresholds, so the section masks are known exactly:
    /// OR is their union, AND their intersection, and the post-combine refinements run once on
    /// the result rather than per section.
    #[test]
    fn test_combine_mask_sections_or_and_and() {
        let dims = (4usize, 4, 4);
        let n = dims.0 * dims.1 * dims.2;
        // A ramp: voxel i has value i, so `threshold:fixed:t` keeps exactly the voxels above t.
        let mag: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let meta = crate::bridge::to_scan_metadata(dims, (1.0, 1.0, 1.0), &[0.005], 3.0, (0.0, 0.0, 1.0), None);

        let section = |t: f64| MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(t) },
            refinements: vec![],
        };
        // > 10 keeps 53 voxels, > 30 keeps 33; the second is a strict subset of the first.
        let sections = crate::bridge::to_mask_sections(&[section(10.0), section(30.0)], &Default::default());
        let count = |m: &[u8]| m.iter().filter(|&&v| v == 1).count();

        let or = combine_sections(&sections, MaskCombine::Or, &[], &[], &[&mag], &meta).unwrap();
        assert_eq!(count(&or), n - 11, "union is the looser threshold");

        let and = combine_sections(&sections, MaskCombine::And, &[], &[], &[&mag], &meta).unwrap();
        assert_eq!(count(&and), n - 31, "intersection is the tighter threshold");
        assert!(or.iter().zip(&and).all(|(o, a)| o >= a), "AND ⊆ OR");

        // One section: the combine mode makes no difference.
        let one = crate::bridge::to_mask_sections(&[section(10.0)], &Default::default());
        for mode in [MaskCombine::Or, MaskCombine::And] {
            assert_eq!(combine_sections(&one, mode, &[], &[], &[&mag], &meta).unwrap(), or);
        }

        // Post-combine refinements run on the combined mask.
        let refinements = crate::bridge::to_mask_ops(&[MaskOp::Erode { iterations: 1 }], &Default::default());
        let eroded = combine_sections(&sections, MaskCombine::And, &refinements, &[], &[&mag], &meta).unwrap();
        assert!(count(&eroded) < count(&and), "erosion shrank the combined mask");

        // No sections is a configuration error, not an empty mask.
        assert!(combine_sections(&[], MaskCombine::Or, &[], &[], &[&mag], &meta).is_err());
    }

    /// `build_mask` corrects each echo on its own before the sections see it, and words a
    /// mis-sized echo as the dimension error it is.
    #[test]
    fn build_mask_corrects_each_echo_and_names_a_missized_one() {
        let meta = crate::stages::magnitude::tests::meta_4x4x4();
        let mut config = PipelineConfig::default();
        let sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.0) },
            refinements: vec![],
        }];
        config.masking.inhomogeneity_correction = true;
        let err = build_mask(&sections, &config, &[], vec![vec![1.0; 48]], &meta).unwrap_err();
        assert!(matches!(err, EngineError::DimensionMismatch(ref m) if m.contains("echo 1's magnitude")), "{err}");

        config.masking.inhomogeneity_correction = false;
        let mask = build_mask(&sections, &config, &[], vec![vec![1.0; 64]], &meta).unwrap();
        assert_eq!(mask.len(), 64);
    }

    #[test]
    fn a_custom_mask_on_the_working_grid_is_binarised_as_it_is() {
        let meta = crate::stages::magnitude::tests::meta_4x4x4();
        let data: Vec<f64> = (0..64).map(|i| if i % 2 == 0 { 0.9 } else { 0.1 }).collect();
        let m = conform_custom_mask(&data, meta.dims, &meta.affine, &meta).unwrap();
        assert_eq!(m.iter().filter(|&&v| v == 1).count(), 32);
    }
}
