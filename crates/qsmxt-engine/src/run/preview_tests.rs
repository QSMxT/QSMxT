//! Mask previews ([`preview_mask`]): the mask a recipe makes, computed by the run's own masking
//! without touching the store's mask or anything downstream of it, from inputs kept between
//! previews.

use super::tests::{config, phantom};
use super::*;
use crate::masking::{
    bet_and_phase_mask_recipe, default_mask_sections, MaskCombine, MaskOp, MaskSection, MaskThresholdMethod,
    MaskingInput,
};
use crate::stages::mask::{MaskInputs, Upto};
use crate::store::MemoryStore;
use crate::observe::VolumeData;

/// Steps started, by name.
#[derive(Default)]
struct Started(Vec<String>);

impl Observer for Started {
    fn stage(&mut self, step: &str, _algorithm: Option<&str>, event: StageEvent<'_>) {
        if let StageEvent::Started(_) = event {
            self.0.push(step.to_string());
        }
    }
}

fn preview_of(
    config: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore, inputs: &mut MaskInputs, upto: Upto,
) -> (stages::mask::MaskPreview, Vec<String>) {
    let plan = crate::plan::plan(config, facts);
    let mut started = Started::default();
    let p = preview_mask(&plan, config, facts, store, &mut started, inputs, &config.masking.sections, upto).unwrap();
    (p, started.0)
}

fn brain_mask(store: &MemoryStore) -> Vec<u8> {
    match store.get(&Item::BrainMask) {
        Some(VolumeData::Mask(m)) => m.clone(),
        other => panic!("no brain mask: {other:?}"),
    }
}

fn threshold(method: MaskThresholdMethod, value: Option<f64>) -> MaskOp {
    MaskOp::Threshold { method, value }
}

/// Recipes the run's mask step makes differently: each input, each threshold method, sections
/// combined both ways, refinements after combining, and inhomogeneity correction on.
fn recipes() -> Vec<(&'static str, PipelineConfig)> {
    let with = |f: &dyn Fn(&mut PipelineConfig)| {
        let mut c = config();
        f(&mut c);
        c
    };
    let section = |input, generator, refinements| MaskSection { input, generator, refinements };
    vec![
        ("magnitude otsu", config()),
        ("default recipe, corrected", with(&|c| {
            c.masking.sections = default_mask_sections();
            c.masking.inhomogeneity_correction = true;
        })),
        ("phase quality, percentile", with(&|c| {
            c.masking.sections = vec![section(MaskingInput::PhaseQuality, threshold(MaskThresholdMethod::Percentile, Some(40.0)),
                vec![MaskOp::Dilate { iterations: 2 }, MaskOp::Close { radius: 1 }])];
        })),
        ("first and last echo, AND, refined", with(&|c| {
            c.masking.sections = vec![
                section(MaskingInput::MagnitudeFirst, threshold(MaskThresholdMethod::Fixed, Some(20.0)), vec![]),
                section(MaskingInput::MagnitudeLast, threshold(MaskThresholdMethod::Otsu, None), vec![MaskOp::Dilate { iterations: 1 }]),
            ];
            c.masking.combine = MaskCombine::And;
            c.masking.refinements = vec![MaskOp::FillHoles { max_size: 0 }, MaskOp::Erode { iterations: 1 }];
        })),
        ("BET and phase quality", with(&|c| {
            let r = bet_and_phase_mask_recipe();
            c.masking.sections = r.sections;
            c.masking.combine = r.combine;
            c.masking.refinements = r.refinements;
        })),
    ]
}

/// A preview of the whole recipe is the mask the run then makes, bit for bit; and the run reuses
/// the steps the preview took.
#[test]
fn a_preview_of_the_recipe_is_the_runs_mask() {
    for (name, config) in recipes() {
        let (facts, mut store) = phantom();
        let mut inputs = MaskInputs::default();
        let (preview, started) = preview_of(&config, &facts, &mut store, &mut inputs, Upto::Recipe);
        assert!(!started.contains(&"mask".to_string()), "{name}: the preview is not the mask step: {started:?}");
        assert!(!store.has(&Item::BrainMask), "{name}: the preview leaves the brain mask alone");
        assert_eq!(preview.input, config.masking.sections[0].input);
        assert_eq!(preview.voxels, preview.mask.iter().filter(|&&v| v != 0).count());
        assert!(preview.voxels > 0, "{name}: an empty mask tests nothing");

        let plan = crate::plan::plan(&config, &facts);
        let mut ran = Started::default();
        run(&plan, &config, &facts, &mut store, &mut ran).unwrap();
        assert_eq!(brain_mask(&store), preview.mask, "{name}: the run's mask is the preview");
        assert!(!ran.0.iter().any(|s| started.contains(s)), "{name}: the run reuses what the preview computed: {:?}", ran.0);
    }
}

/// After a run, previewing another recipe changes nothing the run made: the next run of the same
/// config has nothing to do.
#[test]
fn a_preview_leaves_the_runs_outputs_alone() {
    let (facts, mut store) = phantom();
    let config = config();
    let plan = crate::plan::plan(&config, &facts);
    run(&plan, &config, &facts, &mut store, &mut ()).unwrap();
    let mask = brain_mask(&store);

    let mut other = config.clone();
    other.masking.sections[0].refinements = vec![MaskOp::Erode { iterations: 2 }];
    let mut inputs = MaskInputs::default();
    let (preview, started) = preview_of(&other, &facts, &mut store, &mut inputs, Upto::Recipe);
    assert!(started.is_empty(), "every step before the mask is cached: {started:?}");
    assert_ne!(preview.mask, mask, "the eroded recipe makes another mask");
    assert_eq!(brain_mask(&store), mask);

    let mut again = Started::default();
    run(&plan, &config, &facts, &mut store, &mut again).unwrap();
    assert!(again.0.is_empty(), "nothing went stale: {:?}", again.0);
}

/// Part of a recipe: one section after its first steps is its method, then its steps in order,
/// on its input's image, with Otsu's threshold of that image.
#[test]
fn a_preview_shows_a_section_step_by_step() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.masking.sections = vec![
        MaskSection { input: MaskingInput::Magnitude, generator: threshold(MaskThresholdMethod::Fixed, Some(1e9)), refinements: vec![] },
        MaskSection {
            input: MaskingInput::PhaseQuality, generator: threshold(MaskThresholdMethod::Otsu, None),
            refinements: vec![MaskOp::Dilate { iterations: 1 }, MaskOp::Erode { iterations: 3 }],
        },
    ];
    let mut inputs = MaskInputs::default();
    let at = |inputs: &mut MaskInputs, store: &mut MemoryStore, ops| {
        preview_of(&config, &facts, store, inputs, Upto::Section { index: 1, ops }).0
    };
    let generated = at(&mut inputs, &mut store, 1);
    assert_eq!(generated.input, MaskingInput::PhaseQuality);
    assert_eq!(generated.otsu, qsm_core::utils::otsu_threshold(&generated.image, 256));
    let thresholded: Vec<u8> = generated.image.iter().map(|&v| u8::from(v > generated.otsu)).collect();
    assert_eq!(generated.mask, thresholded, "its method alone: Otsu's threshold of its image");
    let dilated = at(&mut inputs, &mut store, 2);
    assert!(dilated.voxels > generated.voxels);
    let all = at(&mut inputs, &mut store, 3);
    assert!(all.voxels < dilated.voxels);
    assert_eq!(at(&mut inputs, &mut store, 99).mask, all.mask, "more steps than it has is all of them");
    assert_eq!(at(&mut inputs, &mut store, 0).mask, generated.mask, "its method is always applied");

    // The first section keeps nothing, so OR makes the second's mask.
    let recipe = preview_of(&config, &facts, &mut store, &mut inputs, Upto::Recipe).0;
    assert_eq!(recipe.mask, all.mask);
    assert_eq!(recipe.input, MaskingInput::Magnitude, "the whole recipe shows its first section's input");

    let err = preview_mask(&crate::plan::plan(&config, &facts), &config, &facts, &mut store, &mut (), &mut inputs,
                           &config.masking.sections, Upto::Section { index: 2, ops: 1 }).unwrap_err();
    assert!(err.to_string().contains("no section 3"), "{err}");
}

/// The inputs are built once and kept: a threshold or morphology change reuses them; another
/// input adds its image; a change to the inhomogeneity correction or new input data builds them
/// again.
#[test]
fn the_section_inputs_are_kept_between_previews() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.masking.inhomogeneity_correction = true;
    let mut inputs = MaskInputs::default();
    preview_of(&config, &facts, &mut store, &mut inputs, Upto::Recipe);
    let key = inputs.held_key().unwrap().to_string();
    let image = inputs.image(MaskingInput::Magnitude).unwrap().clone();

    config.masking.sections[0].generator = threshold(MaskThresholdMethod::Percentile, Some(60.0));
    config.masking.sections[0].refinements = vec![MaskOp::Dilate { iterations: 1 }];
    preview_of(&config, &facts, &mut store, &mut inputs, Upto::Recipe);
    assert_eq!(inputs.held_key(), Some(key.as_str()));
    assert!(std::sync::Arc::ptr_eq(inputs.image(MaskingInput::Magnitude).unwrap(), &image), "the same image, not rebuilt");

    config.masking.sections[0].input = MaskingInput::PhaseQuality;
    preview_of(&config, &facts, &mut store, &mut inputs, Upto::Recipe);
    assert!(std::sync::Arc::ptr_eq(inputs.image(MaskingInput::Magnitude).unwrap(), &image), "kept beside the new one");
    assert!(inputs.image(MaskingInput::PhaseQuality).is_some());

    config.homogeneity.sigma_mm *= 2.0;
    preview_of(&config, &facts, &mut store, &mut inputs, Upto::Recipe);
    assert_ne!(inputs.held_key(), Some(key.as_str()), "corrected otherwise, built again");
    assert!(inputs.image(MaskingInput::Magnitude).is_none(), "only what this recipe reads is built");

    let before = inputs.held_key().unwrap().to_string();
    store.set_identity(serde_json::json!("phantom-2"));
    preview_of(&config, &facts, &mut store, &mut inputs, Upto::Recipe);
    assert_ne!(inputs.held_key(), Some(before.as_str()), "new input data, built again");

}

/// A run with no mask step has nothing to preview.
#[test]
fn a_run_without_a_mask_has_no_preview() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.do_qsm = false;
    let plan = crate::plan::plan(&config, &facts);
    if plan.has(Stage::Mask(Pass::Main)) {
        return;
    }
    let err = preview_mask(&plan, &config, &facts, &mut store, &mut (), &mut MaskInputs::default(),
                           &config.masking.sections, Upto::Recipe).unwrap_err();
    assert!(err.to_string().contains("no brain mask"), "{err}");
}

/// Threshold and morphology previews of a 6 M voxel volume, timed: `cargo test --release --
/// --ignored --nocapture preview_speed`. The first preview builds the inputs; the rest reuse them.
#[test]
#[ignore]
fn preview_speed() {
    let dims = (176usize, 240, 150);
    let n = dims.0 * dims.1 * dims.2;
    let meta = RunMetadata { dims, n_echoes: 2, echo_times: vec![0.004, 0.012], ..crate::stages::magnitude::tests::meta_4x4x4() };
    let (cx, cy, cz) = (dims.0 as f64 / 2.0, dims.1 as f64 / 2.0, dims.2 as f64 / 2.0);
    let mut magnitude = vec![0.0; n];
    let mut phase = vec![0.0; n];
    for z in 0..dims.2 { for y in 0..dims.1 { for x in 0..dims.0 {
        let i = x + y * dims.0 + z * dims.0 * dims.1;
        let r = ((x as f64 - cx) / 70.0).powi(2) + ((y as f64 - cy) / 95.0).powi(2) + ((z as f64 - cz) / 60.0).powi(2);
        magnitude[i] = if r < 1.0 { 100.0 + (i % 7) as f64 } else { (i % 5) as f64 };
        phase[i] = ((x * 7 + y * 3 + z) % 628) as f64 / 100.0 - std::f64::consts::PI;
    }}}
    let mut config = PipelineConfig::default();
    config.masking.inhomogeneity_correction = false;
    for input in [MaskingInput::Magnitude, MaskingInput::PhaseQuality] {
        config.masking.sections = vec![MaskSection {
            input, generator: threshold(MaskThresholdMethod::Otsu, None),
            refinements: vec![MaskOp::Dilate { iterations: 1 }, MaskOp::FillHoles { max_size: 0 }, MaskOp::Erode { iterations: 1 }],
        }];
        let mut inputs = MaskInputs::default();
        let t = std::time::Instant::now();
        inputs.fill::<crate::EngineError>(&config.masking.sections, &config, "k".into(), &meta,
            || Ok((vec![phase.clone(), phase.clone()], vec![magnitude.clone(), magnitude.clone()]))).unwrap();
        let built = t.elapsed();
        for (label, refinements) in [
            ("threshold only", vec![]),
            ("dilate, fill holes, erode", config.masking.sections[0].refinements.clone()),
        ] {
            let mut c = config.clone();
            c.masking.sections[0].refinements = refinements;
            let t = std::time::Instant::now();
            let p = stages::mask::preview(&c.masking.sections, &c, &inputs, &meta, Upto::Recipe).unwrap();
            eprintln!("{input} ({n} voxels): inputs built in {built:?}; {label}: {:?} ({} voxels)", t.elapsed(), p.voxels);
        }
    }
}
