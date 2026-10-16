//! The run record ([`crate::record`]) of runs in memory: what each step did, what was decided
//! and why, kept for a run that reuses the steps; the graph after the run, and the methods text.

use super::tests::{config, phantom, with_mese};
use super::*;
use crate::methods::{generate_methods_for, methods_for_run, methods_structured};
use crate::pipeline_graph::{pipeline_graph, pipeline_graph_after};
use crate::record::{Choice, StepState, Topic};
use crate::store::MemoryStore;

fn run_recorded(config: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore) -> (Plan, RunRecord) {
    let plan = crate::plan::plan(config, facts);
    let record = run(&plan, config, facts, store, &mut ()).unwrap();
    (plan, record)
}

fn states(record: &RunRecord) -> Vec<(&str, StepState)> {
    record.steps.iter().map(|s| (s.id.as_str(), s.state)).collect()
}

/// A run that decides nothing records every step computed, then reused; its methods text is the
/// configuration's, and its graph after the run is the planned one with the states on its boxes.
#[test]
fn a_run_that_decides_nothing_records_its_steps_computed_then_reused() {
    let (facts, mut store) = phantom();
    let config = config();
    let (plan, first) = run_recorded(&config, &facts, &mut store);
    let ids: Vec<&str> = first.steps.iter().map(|s| s.id.as_str()).collect();
    let planned: Vec<String> = plan.steps.iter().map(|s| s.name.clone()).collect();
    assert_eq!(ids, planned, "one record per step of the plan, in its order");
    assert_eq!(states(&first), [
        ("load", StepState::Computed), ("scale_phase", StepState::Computed), ("magnitude", StepState::Computed),
        ("mask", StepState::Computed), ("motion", StepState::Skipped), ("unwrap", StepState::Computed),
        ("bgremove", StepState::Computed), ("invert", StepState::Computed), ("reference", StepState::Computed),
        ("output_space", StepState::Skipped),
    ]);
    // Motion correction is off and the run is not resampled: the plan already said so.
    assert!(!first.decided() || first.decisions().all(|(s, _)| s.id == "output_space"), "{first:?}");
    assert_eq!(first.resampled, Some(false));

    let (_, again) = run_recorded(&config, &facts, &mut store);
    assert!(again.steps.iter().all(|s| s.state != StepState::Computed), "{:?}", states(&again));
    assert_eq!(again.step("invert").unwrap().state, StepState::Cached);

    for record in [&first, &again] {
        assert_eq!(methods_for_run(&config, &plan, record, "qsmxt").to_markdown(), generate_methods_for(&config, "qsmxt"));
        let after = pipeline_graph_after(&plan, &config, &facts, record);
        let before = pipeline_graph(&plan, &config, &facts, store.grid());
        assert!(!after.differs_from(&before), "nothing differed from the plan");
        assert!(after.nodes.iter().all(|n| n.run.is_some()));
    }
}

/// A supplied mask that cannot be brought onto the working grid is computed instead: the record
/// says so and why, the graph after the run draws the mask computed from the echoes, and the
/// methods text describes the recipe without "where available".
#[test]
fn a_supplied_mask_that_cannot_be_used_is_recorded_computed() {
    let (mut facts, mut store) = phantom();
    let mut config = config();
    config.masking.custom_mask_tool = Some("bet".into());
    config.masking.two_pass = true;
    facts.custom_mask = true;
    // A mask on a grid that shares nothing with the scan's.
    let mut far = [0.0; 16];
    for i in 0..4 { far[5 * i] = 1.0; }
    far[3] = 1000.0;
    store.supply_mask(Supplied::Found { origin: "sub-1_mask.nii".into(), data: vec![1.0; 8], dims: (2, 2, 2), affine: far });
    let (plan, record) = run_recorded(&config, &facts, &mut store);
    assert!(!plan.two_pass, "the plan drops two-pass for a supplied mask");
    let mask = record.step("mask").unwrap();
    assert_eq!(mask.state, StepState::Computed);
    let source = mask.decision(Topic::MaskSource).unwrap();
    assert_eq!(source.choice, Choice::Computed);
    assert_eq!(source.detail.as_deref(), Some("sub-1_mask.nii"));
    assert_eq!(mask.decision(Topic::TwoPass).unwrap().choice, Choice::NotApplied);

    let before = pipeline_graph(&plan, &config, &facts, store.grid());
    let after = pipeline_graph_after(&plan, &config, &facts, &record);
    assert!(after.differs_from(&before));
    let node = after.node("mask").unwrap();
    assert!(node.reads.iter().any(|r| r == "Phase(1)") && !node.reads.iter().any(|r| r == "SuppliedMask"), "{:?}", node.reads);
    assert!(!before.node("load").unwrap().writes.is_empty());
    assert!(!after.node("load").unwrap().writes.iter().any(|w| w == "SuppliedMask"));

    let text = methods_for_run(&config, &plan, &record, "qsmxt").to_markdown();
    assert_ne!(text, generate_methods_for(&config, "qsmxt"));
    assert!(!text.contains("Where available") && text.contains("A brain mask was generated"), "{text}");
    assert!(!text.contains("Two-pass"), "{text}");

    // Reused, the step still says what it decided when it was computed.
    let (_, again) = run_recorded(&config, &facts, &mut store);
    let mask = again.step("mask").unwrap();
    assert_eq!(mask.state, StepState::Cached);
    assert_eq!(mask.decision(Topic::MaskSource).unwrap().choice, Choice::Computed);
    // ...and it survives a store restored over the same volumes (qsmbly's page).
    let recorded: crate::store::Recorded = serde_json::from_value(serde_json::to_value(store.recorded()).unwrap()).unwrap();
    let mut restored = MemoryStore::new(store.header().unwrap(), json!("phantom-1"));
    restored.restore(recorded);
    // (Kept with the step: what was decided computing it; two-pass is decided by every run.)
    let computing: Vec<_> = mask.decisions.iter().filter(|d| d.topic != Topic::TwoPass).cloned().collect();
    assert_eq!(StageStore::decisions(&mut restored, "mask"), computing);
}

/// A supplied mask that is used: the record and the methods text say it was taken from the
/// derivatives, and nothing differs from the plan.
#[test]
fn a_supplied_mask_that_is_used_is_recorded_supplied() {
    let (mut facts, mut store) = phantom();
    let mut config = config();
    config.masking.custom_mask_tool = Some("bet".into());
    facts.custom_mask = true;
    let header = store.header().unwrap();
    let ones = vec![1.0; header.dims.0 * header.dims.1 * header.dims.2];
    store.supply_mask(Supplied::Found { origin: "sub-1_mask.nii".into(), data: ones, dims: header.dims, affine: header.affine });
    let (plan, record) = run_recorded(&config, &facts, &mut store);
    assert_eq!(record.step("mask").unwrap().decision(Topic::MaskSource).unwrap().choice, Choice::Supplied);
    let text = methods_for_run(&config, &plan, &record, "qsmxt").to_markdown();
    assert!(text.contains("Brain masks were sourced from the 'bet' BIDS derivatives."), "{text}");
    assert!(!text.contains("A brain mask was generated"), "{text}");
    let before = pipeline_graph(&plan, &config, &facts, store.grid());
    assert!(!pipeline_graph_after(&plan, &config, &facts, &record).differs_from(&before));
}

/// With `--use-custom-masks` and no mask found, the mask is computed and the text drops "where
/// available".
#[test]
fn a_supplied_mask_not_found_is_recorded_computed() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.masking.custom_mask_tool = Some("bet".into());
    let (plan, record) = run_recorded(&config, &facts, &mut store);
    let d = record.step("mask").unwrap().decision(Topic::MaskSource).unwrap().clone();
    assert_eq!((d.choice, d.detail.as_deref()), (Choice::Computed, Some("the loaded files")));
    let text = methods_for_run(&config, &plan, &record, "qsmxt").to_markdown();
    let expected = generate_methods_for(&config, "qsmxt").replace(
        "Where available, brain masks were sourced from the 'bet' BIDS derivatives; otherwise they were generated as follows. ", "");
    assert_eq!(text, expected);
}

/// Cropping to the mask records the box each reconstruction step ran in, without changing the
/// text or the graph's structure; reused, the boxes are still reported.
#[test]
fn the_reconstruction_box_is_recorded_from_the_mask() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.crop_to_mask = true;
    config.pipeline.crop_margin_mm = 1.0;
    let (plan, record) = run_recorded(&config, &facts, &mut store);
    for step in ["bgremove", "invert"] {
        let d = record.step(step).unwrap().decision(Topic::ReconstructionBox).unwrap();
        assert_eq!(d.choice, Choice::Cropped, "{step}: {d:?}");
        assert!(d.detail.as_deref().is_some_and(|s| s.ends_with("of 16x16x16")), "{d:?}");
    }
    assert_eq!(methods_for_run(&config, &plan, &record, "qsmxt"), methods_structured(&config, &plan, "qsmxt"));
    let before = pipeline_graph(&plan, &config, &facts, store.grid());
    assert!(!pipeline_graph_after(&plan, &config, &facts, &record).differs_from(&before));
    let (_, again) = run_recorded(&config, &facts, &mut store);
    assert_eq!(again.step("invert").unwrap().decisions, record.step("invert").unwrap().decisions);
}

/// R2 from the MESE and R2′ measured: under `auto`, the text says R2′ was measured, not "where
/// available ... otherwise estimated".
#[test]
fn r2prime_measured_under_auto_is_said_measured() {
    let (mut facts, mut store) = phantom();
    with_mese(&mut facts, &mut store);
    let mut config = config();
    config.pipeline.do_r2starmap = true;
    config.pipeline.do_r2map = true;
    config.pipeline.do_r2primemap = true;
    let (plan, record) = run_recorded(&config, &facts, &mut store);
    let r2 = record.step("r2_r2prime").unwrap();
    assert_eq!(r2.decision(Topic::R2Source).unwrap().choice, Choice::Measured);
    assert_eq!(r2.decision(Topic::R2primeSource).unwrap().choice, Choice::Measured);
    let text = methods_for_run(&config, &plan, &record, "qsmxt").to_markdown();
    assert!(text.contains("R2' was computed as R2* - R2.") && !text.contains("R2PRIMEnet"), "{text}");
    assert!(generate_methods_for(&config, "qsmxt").contains("R2PRIMEnet"));
}

/// SMWI with no susceptibility map to weight by is skipped, with the reason.
#[test]
fn a_step_with_nothing_to_work_on_is_recorded_skipped() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.do_smwi = true;
    let mut swi_only = super::tests::config();
    swi_only.pipeline.do_qsm = false;
    swi_only.pipeline.do_swi = true;
    run_recorded(&swi_only, &facts, &mut store);
    let meta = store.grid().unwrap().clone();
    let plan = crate::plan::plan(&config, &facts);
    let step = plan.steps.iter().find(|s| s.stage == Stage::Smwi).unwrap();
    let r = run_step(&plan, &config, &facts, &meta, step, &mut store, &mut ()).unwrap();
    assert_eq!(r.state, StepState::Skipped);
    assert_eq!(r.decision(Topic::Step).unwrap().reason, "There was no susceptibility map to weight by.");
}
