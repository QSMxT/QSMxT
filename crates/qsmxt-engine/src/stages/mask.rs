//! Masking: the brain mask from its sections, a supplied mask brought onto the working grid, and
//! the reliable-phase mask of a two-pass run.

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::masking::{MaskCombine, MaskSection, MaskingInput};
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
        "combine": format!("{}", config.masking.resolved_two_pass_combine()),
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

/// Build a mask from `sections`, folded with `combine` and then the configured refinements. The
/// main mask (`masking.combine`) and the two-pass reliable mask
/// ([`resolved_two_pass_combine`](crate::config::MaskingConfig::resolved_two_pass_combine)) are
/// both built here, so both read the same images and follow the same folding rules.
///
/// `phases` are every echo's phase in radians (for phase-based inputs such as phase quality).
/// `magnitudes` are every echo's magnitude, or none for a phase-only run. Each echo is corrected
/// for inhomogeneity on its own when `--inhomogeneity-correction` is on, and each section builds
/// the magnitude it asks for (echo 1, the RSS, the last echo) from these in qsm-core, so all
/// sections see the same, consistently corrected echoes (QSM.rs#173).
///
/// This is [`preview`] of the whole recipe on freshly built [`MaskInputs`], so a preview and
/// the run's mask are the same computation.
pub fn build_mask(
    sections: &[MaskSection],
    combine: MaskCombine,
    config: &PipelineConfig,
    phases: &[&[f64]],
    magnitudes: Vec<Vec<f64>>,
    meta: &RunMetadata,
) -> Result<Vec<u8>, EngineError> {
    let mut inputs = MaskInputs::default();
    inputs.fill::<EngineError>(sections, config, String::new(), meta, || Ok((
        phases.iter().map(|p| p.to_vec()).collect(), magnitudes,
    )))?;
    Ok(preview(sections, combine, config, &inputs, meta, Upto::Recipe)?.mask)
}

/// The images mask sections work from, built once and kept for previews: for each input a
/// section names (phase quality, the RSS magnitude, the first or last echo), the image its method
/// thresholds and the magnitude its BET, HD-BET, RS2-Net, MindGrab and signal-gated erosion see
/// (qsm-core's `resolve_masking_input` and `section_magnitude`), from echoes corrected for
/// inhomogeneity when that is on.
///
/// They are kept under a key ([`MaskInputs::key`]): the settings they depend on and what the
/// echoes were computed from. Threshold and morphology changes reuse them; a new scan, or a change
/// to how the echoes or their correction are computed, builds them again.
///
/// They are kept in double precision, as the run's mask step computes them, so a preview's mask is
/// bit for bit the run's. A front end shows `image` from the preview it gets back.
#[derive(Debug, Default)]
pub struct MaskInputs {
    key: Option<String>,
    /// The image each input's sections threshold.
    images: std::collections::HashMap<MaskingInput, std::sync::Arc<Vec<f64>>>,
    /// The magnitude each input's sections (and the refinements after combining, which see the
    /// RSS) work with; `None` for a phase-only run.
    magnitudes: std::collections::HashMap<MaskingInput, Option<std::sync::Arc<Vec<f64>>>>,
}

/// Every echo's phase, and every echo's magnitude (none for a phase-only run).
pub type Echoes = (Vec<Vec<f64>>, Vec<Vec<f64>>);

impl MaskInputs {
    /// What the inputs depend on besides the echoes themselves: the inhomogeneity correction and
    /// its settings, and `upstream`, which names what the echoes were computed from (the run's
    /// load and earlier steps' cache keys, [`crate::run::preview_mask`]).
    pub fn key(config: &PipelineConfig, upstream: &str) -> String {
        let correction = config.masking.inhomogeneity_correction.then(|| crate::stages::magnitude::params(config));
        serde_json::json!({ "upstream": upstream, "inhomogeneity_correction": correction }).to_string()
    }

    /// The inputs `sections` and the refinements after combining need, of those not kept under
    /// the same key: built from the echoes `load` gives, which is called only if one is missing.
    /// Inputs kept under another key are dropped first.
    pub fn fill<E: From<EngineError>>(
        &mut self,
        sections: &[MaskSection],
        config: &PipelineConfig,
        key: String,
        meta: &RunMetadata,
        load: impl FnOnce() -> Result<Echoes, E>,
    ) -> Result<(), E> {
        if self.key.as_deref() != Some(key.as_str()) {
            self.images.clear();
            self.magnitudes.clear();
            self.key = Some(key);
        }
        let wanted_images: Vec<MaskingInput> = sections.iter().map(|s| s.input).collect();
        // The refinements after combining see the RSS magnitude.
        let wanted_magnitudes: Vec<MaskingInput> =
            wanted_images.iter().copied().chain(std::iter::once(MaskingInput::Magnitude)).collect();
        if wanted_images.iter().all(|i| self.images.contains_key(i))
            && wanted_magnitudes.iter().all(|i| self.magnitudes.contains_key(i))
        {
            return Ok(());
        }
        let (phases, magnitudes) = load()?;
        let magnitudes = if config.masking.inhomogeneity_correction {
            magnitudes.into_iter().enumerate()
                .map(|(i, m)| homogeneity_correct(&m, meta, config, &format!("echo {}'s magnitude", i + 1)))
                .collect::<Result<Vec<_>, EngineError>>()?
        } else {
            magnitudes
        };
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
        let core_meta = scan_metadata(meta);
        let failed = |e: qsm_core::pipeline::PipelineError| EngineError::Config(format!("masking: {}", e));
        for input in wanted_magnitudes {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.magnitudes.entry(input) {
                let m = qsm_core::pipeline::section_magnitude(core_input(input), &mag_refs, &core_meta).map_err(failed)?;
                slot.insert(m.map(std::sync::Arc::new));
            }
        }
        for input in wanted_images {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.images.entry(input) {
                let image = qsm_core::pipeline::resolve_masking_input(core_input(input), &phase_refs, &mag_refs, &core_meta)
                    .map_err(failed)?;
                slot.insert(std::sync::Arc::new(image));
            }
        }
        Ok(())
    }

    /// The key the inputs are kept under.
    #[cfg(test)]
    pub(crate) fn held_key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// The image sections with `input` threshold, when it is kept.
    pub fn image(&self, input: MaskingInput) -> Option<&std::sync::Arc<Vec<f64>>> {
        self.images.get(&input)
    }

    fn magnitude(&self, input: MaskingInput) -> Option<&[f64]> {
        self.magnitudes.get(&input).and_then(|m| m.as_deref()).map(|m| m.as_slice())
    }

    /// Bytes held.
    pub fn bytes(&self) -> usize {
        let mut seen: Vec<*const Vec<f64>> = Vec::new();
        let mut total = 0;
        for a in self.images.values().chain(self.magnitudes.values().flatten()) {
            let p = std::sync::Arc::as_ptr(a);
            if !seen.contains(&p) {
                seen.push(p);
                total += a.len() * std::mem::size_of::<f64>();
            }
        }
        total
    }
}

fn core_input(input: MaskingInput) -> qsm_core::pipeline::config::MaskingInput {
    use qsm_core::pipeline::config::MaskingInput as P;
    match input {
        MaskingInput::MagnitudeFirst => P::MagnitudeFirst,
        MaskingInput::Magnitude => P::Magnitude,
        MaskingInput::MagnitudeLast => P::MagnitudeLast,
        MaskingInput::PhaseQuality => P::PhaseQuality,
    }
}

/// How much of a recipe a preview shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Upto {
    /// The whole recipe: every section, combined, then the refinements after combining. The mask
    /// the run makes.
    Recipe,
    /// One section (0-based) on its own, after its first `ops` steps (its method is the first;
    /// more than it has is all of them).
    Section { index: usize, ops: usize },
}

/// A mask the recipe makes, and what it was made from.
#[derive(Debug, Clone)]
pub struct MaskPreview {
    pub mask: Vec<u8>,
    /// The input of the section shown (the first section's for the whole recipe).
    pub input: MaskingInput,
    /// That input's image: what its method thresholds.
    pub image: std::sync::Arc<Vec<f64>>,
    /// Otsu's threshold of that image, as a `threshold:otsu` method computes it.
    pub otsu: f64,
    /// The voxels in the mask.
    pub voxels: usize,
}

/// The mask `sections` make (folded with `combine`, then the configured refinements),
/// or part of it (`upto`), from `inputs` (filled for `sections` by [`MaskInputs::fill`]). It reads
/// and writes nothing else: a front end previews a recipe while editing it, and the run's mask
/// step is this function on the whole recipe ([`build_mask`]).
///
/// The weights of any deep-learning method the shown part runs must have been provided.
pub fn preview(
    sections: &[MaskSection],
    combine: MaskCombine,
    config: &PipelineConfig,
    inputs: &MaskInputs,
    meta: &RunMetadata,
    upto: Upto,
) -> Result<MaskPreview, EngineError> {
    use qsm_core::pipeline::PipelineError;
    let core_meta = scan_metadata(meta);
    let n = meta.dims.0 * meta.dims.1 * meta.dims.2;
    let failed = |e: PipelineError| EngineError::Config(format!("masking: {}", e));
    let missing = |input: MaskingInput| EngineError::Config(format!("masking: the {input} image was not built"));
    if sections.is_empty() {
        return Err(failed(PipelineError::InvalidConfig("no mask sections configured".into())));
    }
    let core_sections = crate::bridge::to_mask_sections(sections, &config.bet);
    // One section, from an all-ones mask, as qsm-core's `build_mask_section` does.
    let section = |i: usize, ops: usize| -> Result<Vec<u8>, EngineError> {
        let input = sections[i].input;
        let image = inputs.image(input).ok_or_else(|| missing(input))?;
        let all = core_sections[i].all_ops();
        let ops = &all[..ops.clamp(1, all.len())];
        qsm_core::pipeline::apply_mask_ops(vec![1u8; n], ops, image, inputs.magnitude(input), &core_meta).map_err(failed)
    };
    let (mask, shown) = match upto {
        Upto::Section { index, ops } => {
            if index >= sections.len() {
                return Err(EngineError::Config(format!(
                    "masking: there is no section {} (the recipe has {})", index + 1, sections.len())));
            }
            (section(index, ops)?, index)
        }
        Upto::Recipe => {
            let mut combined: Option<Vec<u8>> = None;
            for i in 0..sections.len() {
                let mask = section(i, usize::MAX)?;
                combined = Some(match combined {
                    None => mask,
                    Some(mut acc) => { combine.accumulate(&mut acc, &mask); acc }
                });
            }
            let mut mask = combined.unwrap_or_default();
            let refinements = crate::bridge::to_mask_ops(&config.masking.refinements, &config.bet);
            if !refinements.is_empty() {
                // The refinements run on the combined mask, so they see the RSS magnitude (what
                // `signal-erode` gates on); they are not generators, so the input image does not
                // matter.
                let magnitude = inputs.magnitude(MaskingInput::Magnitude);
                mask = qsm_core::pipeline::apply_mask_ops(
                    mask, &refinements, magnitude.unwrap_or(&[]), magnitude, &core_meta).map_err(failed)?;
            }
            (mask, 0)
        }
    };
    let input = sections[shown].input;
    let image = inputs.image(input).ok_or_else(|| missing(input))?.clone();
    let otsu = qsm_core::utils::otsu_threshold(&image, 256);
    let voxels = mask.iter().filter(|&&v| v != 0).count();
    Ok(MaskPreview { mask, input, image, otsu, voxels })
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

/// Where a run's brain mask came from, for [`require_voxels`]'s message.
pub enum MaskSource<'a> {
    /// Computed from the run's mask recipe.
    Recipe(&'a crate::config::MaskingConfig),
    /// Read from the file at this path.
    Supplied(&'a str),
}

/// Refuse a brain mask with no voxels (#251) at the mask stage, so the run stops there, naming
/// the cause, rather than several steps later with an empty map. Only the final mask counts: a
/// section that adds nothing to a union is a valid recipe, and an empty reliable-phase mask only
/// makes two-pass redundant ([`coverage_advice`]).
pub fn require_voxels(mask: &[u8], source: MaskSource) -> Result<(), EngineError> {
    if mask.iter().any(|&v| v != 0) {
        return Ok(());
    }
    Err(EngineError::Config(match source {
        MaskSource::Recipe(masking) => {
            let joiner = format!(" {} ", masking.combine.to_string().to_uppercase());
            let mut recipe = masking.sections.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(&joiner);
            for op in &masking.refinements {
                recipe.push_str(&format!(" → {op}"));
            }
            format!(
                "the brain mask is empty: masking ({recipe}) left no voxels. Check that the \
                 threshold or BET settings suit the contrast of the image each section is built \
                 from, and that erosions do not remove the whole brain (--mask, --mask-refine).")
        }
        MaskSource::Supplied(origin) => format!(
            "the supplied brain mask {origin} is empty: no voxel is above 0.5. Supply a mask with \
             the brain in it, or drop --use-custom-masks to compute one."),
    }))
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
    use crate::masking::{MaskOp, MaskThresholdMethod};

    /// A grid of `dims` with 1 mm voxels and one echo.
    fn meta(dims: (usize, usize, usize)) -> RunMetadata {
        RunMetadata { dims, ..crate::stages::magnitude::tests::meta_4x4x4() }
    }

    /// The mask `sections` make (combined with `combine`, then `refinements`) from one echo whose
    /// magnitude is `magnitude`, with no inhomogeneity correction.
    fn masked(sections: &[MaskSection], combine: MaskCombine, refinements: Vec<MaskOp>, magnitude: &[f64],
              dims: (usize, usize, usize)) -> Result<Vec<u8>, EngineError> {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_correction = false;
        config.masking.refinements = refinements;
        build_mask(sections, combine, &config, &[], vec![magnitude.to_vec()], &meta(dims))
    }

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
        let recipe = |refinements: Vec<MaskOp>| vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.5) },
            refinements,
        }];
        let build = |refinements: Vec<MaskOp>| {
            masked(&recipe(refinements), MaskCombine::Or, vec![], &quality, dims).unwrap()
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

        let section = |t: f64| MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(t) },
            refinements: vec![],
        };
        // > 10 keeps 53 voxels, > 30 keeps 33; the second is a strict subset of the first.
        let sections = [section(10.0), section(30.0)];
        let count = |m: &[u8]| m.iter().filter(|&&v| v == 1).count();

        let or = masked(&sections, MaskCombine::Or, vec![], &mag, dims).unwrap();
        assert_eq!(count(&or), n - 11, "union is the looser threshold");

        let and = masked(&sections, MaskCombine::And, vec![], &mag, dims).unwrap();
        assert_eq!(count(&and), n - 31, "intersection is the tighter threshold");
        assert!(or.iter().zip(&and).all(|(o, a)| o >= a), "AND ⊆ OR");

        // One section: the combine mode makes no difference.
        let one = [section(10.0)];
        for mode in [MaskCombine::Or, MaskCombine::And] {
            assert_eq!(masked(&one, mode, vec![], &mag, dims).unwrap(), or);
        }

        // Post-combine refinements run on the combined mask.
        let eroded = masked(&sections, MaskCombine::And, vec![MaskOp::Erode { iterations: 1 }], &mag, dims).unwrap();
        assert!(count(&eroded) < count(&and), "erosion shrank the combined mask");

        // No sections is a configuration error, not an empty mask.
        assert!(masked(&[], MaskCombine::Or, vec![], &mag, dims).is_err());
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
        let err = build_mask(&sections, MaskCombine::Or, &config, &[], vec![vec![1.0; 48]], &meta).unwrap_err();
        assert!(matches!(err, EngineError::DimensionMismatch(ref m) if m.contains("echo 1's magnitude")), "{err}");

        config.masking.inhomogeneity_correction = false;
        let mask = build_mask(&sections, MaskCombine::Or, &config, &[], vec![vec![1.0; 64]], &meta).unwrap();
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
