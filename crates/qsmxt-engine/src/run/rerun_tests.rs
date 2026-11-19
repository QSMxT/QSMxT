//! Running again over an earlier run's outputs: what is reused, what is computed again, and that
//! the store then holds what a fresh run's does.

use super::tests::*;
use super::*;
use crate::enums::{QsmAlgorithm, SeparationAlgorithm};
use crate::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
use crate::observe::VolumeData;
use crate::store::{Header, MemoryStore};

/// The whole plan, run on the phantom in memory: every engine step computed once, a final
/// referenced map over its support, and a second run with nothing to do. Changing the
/// inversion then re-runs the inversion and what reads it, and nothing before it.
#[test]
fn a_plan_runs_end_to_end_in_memory_and_reruns_only_what_changed() {
    let (facts, mut store) = phantom();
    let mut config = config();
    let first = run_once(&config, &facts, &mut store);
    assert_eq!(first.computed, ["load", "scale_phase", "magnitude", "mask", "unwrap", "bgremove",
                                "invert", "reference"]);
    assert!(first.outputs.contains(&Item::Chi) && first.outputs.contains(&Item::ChiSupport));
    let (Some(VolumeData::Real(chi)), Some(VolumeData::Mask(support))) =
        (store.get(&Item::Chi), store.get(&Item::ChiSupport)) else { panic!("no final map") };
    assert_eq!(chi.len(), N * N * N);
    let n_support = support.iter().filter(|&&s| s != 0).count();
    assert!(n_support > 100, "the support is the sphere less what LBV left undefined: {n_support}");
    assert!(chi.iter().zip(support).all(|(&c, &s)| s != 0 || c == 0.0), "χ is 0 outside its support");
    assert!(chi.iter().all(|c| c.is_finite()));
    assert!(store.annotations(&Item::Chi).is_some_and(|a| a.contains_key("QsmReferenceOffsetPpm")));
    // The magnitude of a run that is not resampled is kept as it was given.
    assert_eq!(store.get(&Item::Magnitude(2)), store.get(&Item::AcquiredMagnitude(2)));

    let again = run_once(&config, &facts, &mut store);
    assert!(again.computed.is_empty(), "nothing changed, nothing to compute: {:?}", again.computed);
    assert_eq!(again.cached.len(), first.computed.len());

    config.inversion.tkd.threshold *= 2.0;
    let changed = run_once(&config, &facts, &mut store);
    assert_eq!(changed.computed, ["invert", "reference"]);

    // New input data recomputes everything.
    store.set_identity(json!("phantom-2"));
    assert_eq!(run_once(&config, &facts, &mut store).computed, first.computed);
}

/// An axial-only inversion (xQSM) on axial data changes nothing about the grid, so switching
/// to it from a classical one reuses every step before the inversion (#355).
#[test]
fn an_axial_only_inversion_on_axial_data_keeps_the_steps_before_it() {
    let (facts, mut store) = phantom();
    let mut config = config();
    run_once(&config, &facts, &mut store);
    config.inversion.algorithm = QsmAlgorithm::Xqsm;
    let events = run_until_done_or_failed(&config, &facts, &mut store);
    assert_eq!(events.cached, ["load", "scale_phase", "magnitude", "mask", "unwrap", "bgremove"]);
    assert_eq!(events.computed.first().map(String::as_str), Some("invert"), "{:?}", events.computed);
}

/// On oblique data the same switch moves the scan to an axial grid, so nothing computed on
/// the acquired grid can be reused: the load step and everything after it are computed again.
#[test]
fn an_axial_only_inversion_on_oblique_data_recomputes_from_the_load_step() {
    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0];
    let (facts, mut store) = phantom_on(affine);
    let mut config = config();
    let first = run_once(&config, &facts, &mut store);
    assert!(store.grid().is_some_and(|m| m.source_geometry.is_none()), "TKD takes the tilt as it is");
    config.inversion.algorithm = QsmAlgorithm::Xqsm;
    let events = run_until_done_or_failed(&config, &facts, &mut store);
    assert!(events.cached.is_empty(), "{:?}", events.cached);
    // The same steps, with the echoes resampled after phase scaling.
    let mut resampled = first.computed.clone();
    resampled.insert(2, "resample_axial".to_string());
    assert_eq!(first.computed[1], "scale_phase");
    assert_eq!(events.computed[..resampled.len() - 2], resampled[..resampled.len() - 2]);
    assert!(store.grid().is_some_and(|m| m.source_geometry.is_some()), "resampled for xQSM");
}

/// The load step hands N4 the first phase image's header orientation while the run works on
/// that image's grid, and not once the run is resampled onto another one.
#[test]
fn the_header_orientation_reaches_the_run_only_on_the_acquired_grid() {
    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0];
    let (facts, _) = phantom_on(affine);
    let orientation = crate::scan::HeaderOrientation {
        pixdim: [1.0; 4], qform_code: 1, quatern: [0.17, 0.0, 0.0, 1.0, 2.0, 3.0],
        sform_code: 0, srow: [[0.0; 4]; 3],
    };
    let header = Header { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: Some(orientation), sidecar: None };
    let mut config = config();
    let loaded = |config: &PipelineConfig| {
        let mut store = MemoryStore::new(header.clone(), json!("oriented"));
        load(config, &facts, &mut store, &mut ()).unwrap().0
    };
    let meta = loaded(&config);
    assert!(meta.source_geometry.is_none());
    assert_eq!(meta.orientation, Some(orientation));
    config.inversion.algorithm = QsmAlgorithm::Xqsm;
    let meta = loaded(&config);
    assert!(meta.source_geometry.is_some(), "resampled for xQSM");
    assert_eq!(meta.orientation, None);
}

/// The load step's key on data where no axial-only algorithm is configured is the one earlier
/// releases wrote (an empty list), so their caches stay valid.
#[test]
fn the_load_key_of_a_run_without_axial_only_algorithms_is_unchanged() {
    let (facts, mut store) = phantom();
    let config = config();
    run_once(&config, &facts, &mut store);
    let geometry = json!({
        "axial_resampling": config.pipeline.axial_resampling,
        "obliquity_threshold": config.pipeline.obliquity_threshold,
        "ignore_slice_gap": config.pipeline.ignore_slice_gap,
        "motion_correction": config.motion_correction,
        "registration": None::<()>,
        "axial_only": Vec::<String>::new(),
        "inputs": json!("phantom-1"),
    });
    let key = crate::cache::step_params_hash(None, &geometry);
    assert!(store.cached_scan(&key).is_some(), "the load step is recorded under the key earlier releases used");
}

/// A store with every step of `config`'s plan recorded as a run records it (under its key,
/// with what it read and wrote), computing nothing: a dry run's shadow of `store`. `None`
/// when the run would stop at a step.
fn recorded_as_run<'a>(config: &PipelineConfig, facts: &ScanFacts, store: &'a MemoryStore)
    -> Option<(Plan, MemoryStore<crate::store::Shadow<'a, crate::store::InMemory>>)> {
    let plan = crate::plan::plan(config, facts);
    let mut done = store.shadow();
    dry_steps(&plan, &plan.config, facts, &mut done).ok()?;
    Some((plan, done))
}

/// Asking for the R2′ map after a χ-sepnet toolbox run that estimated its own R2′ (#502):
/// only the map is computed. χ-separation does not read it, so it stays cached; it used to
/// be recomputed (15 minutes in the browser) because every R2′ step was taken to feed it.
#[test]
fn asking_for_the_r2prime_map_after_a_toolbox_run_computes_only_the_map() {
    let (facts, store) = phantom();
    let mut c = config();
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
    let (plan, done) = recorded_as_run(&c, &facts, &store).unwrap();
    assert!(!plan.has(Stage::R2) && plan.has(Stage::ChiSeparation), "{:?}", plan.steps);
    c.pipeline.do_r2primemap = true;
    let plan = crate::plan::plan(&c, &facts);
    let e = dry_run(&plan, &c, &facts, &done).unwrap();
    let computes: Vec<(&str, ExpectedState)> = e.iter().filter(|e| e.state.computes()).map(|e| (e.id.as_str(), e.state)).collect();
    assert_eq!(computes, [("r2_r2prime", ExpectedState::WillRun)], "{e:?}");
}

/// Whatever a step reads, it is stale once the step that wrote it is computed again under
/// another key. For every step of the plans of a table of configurations (every inversion,
/// one echo and several, one pass and two; every χ-separation with and without a MESE, with
/// and without motion correction, from one QSM or one per echo; supplied maps), recorded as
/// a run records them: each step recorded again under a new key leaves no step whose plan
/// reads one of its outputs cached. On χ-sepnet's toolbox route with nothing to measure R2′
/// from, R2′ is not one of χ-separation's inputs, and a new R2′ leaves it cached; on every
/// other route that reads R2′ it does not.
#[test]
fn a_step_computed_again_makes_every_step_that_reads_its_outputs_stale() {
    let reliable = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }];
    let mut table: Vec<(String, PipelineConfig, ScanFacts, MemoryStore)> = Vec::new();
    for algorithm in QsmAlgorithm::ALL {
        for single in [false, true] {
            for two_pass in [false, true] {
                let (mut facts, store) = phantom();
                if single {
                    facts.n_echoes = 1;
                    facts.echo_times.truncate(1);
                }
                let mut c = config();
                c.inversion.algorithm = algorithm;
                c.masking.two_pass = two_pass;
                c.masking.two_pass_sections = Some(reliable.clone());
                table.push((format!("{algorithm}, {} echo(es), two-pass {two_pass}", facts.n_echoes), c, facts, store));
            }
        }
    }
    let separations = [SeparationAlgorithm::ChiSepIlsqr, SeparationAlgorithm::ChiSepMedi, SeparationAlgorithm::ChiSepNet,
                       SeparationAlgorithm::SusepNet, SeparationAlgorithm::Decompose, SeparationAlgorithm::R2starQsm];
    for separation in separations {
        for (mese, motion, per_echo, native) in [(false, false, false, false), (true, false, false, false), (false, true, false, false),
                                                 (true, true, false, false), (false, false, true, false), (false, false, false, true),
                                                 (true, false, false, true)] {
            let (mut facts, mut store) = phantom();
            if mese {
                with_mese(&mut facts, &mut store);
            }
            let mut c = config();
            let p = &mut c.pipeline;
            (p.do_swi, p.do_smwi, p.do_t2starmap, p.do_r2starmap, p.do_r2map, p.do_r2primemap) = (true, true, true, true, mese, true);
            (p.do_chi_separation, p.do_analysis) = (true, true);
            c.separation.algorithm = separation;
            c.motion_correction.enabled = motion;
            c.qsm.per_echo = per_echo;
            if native {
                c.separation.chi_sepnet.route = crate::ChiSepNetRoute::Native;
            }
            table.push((format!("{separation}, MESE {mese}, motion {motion}, per echo {per_echo}, native {native}"), c, facts, store));
        }
    }
    // Supplied R2′ and χ, a supplied mask, and a region reference on a supplied segmentation.
    for separation in [SeparationAlgorithm::ChiSepNet, SeparationAlgorithm::ChiSepIlsqr] {
        let (mut facts, mut store) = phantom();
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        let grid = crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
        for (item, value) in [(Item::SuppliedR2prime, 5.0), (Item::SuppliedChi, 0.01), (Item::SuppliedSegmentation, 10.0), (Item::SuppliedMask, 1.0)] {
            store.insert(item, VolumeData::Real(vec![value; N * N * N]));
            store.supply(item, grid.clone(), format!("{item:?}"));
        }
        (facts.custom_r2prime, facts.custom_qsm, facts.custom_dseg, facts.custom_mask) = (true, true, true, true);
        let mut c = config();
        c.separation.custom_r2prime_tool = Some("mine".into());
        c.separation.custom_qsm_tool = Some("mine".into());
        c.segmentation.custom_dseg_tool = Some("mine".into());
        c.masking.custom_mask_tool = Some("mine".into());
        c.qsm.reference = crate::enums::QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".into());
        let p = &mut c.pipeline;
        (p.do_r2starmap, p.do_r2primemap, p.do_chi_separation, p.do_segmentation, p.do_analysis) = (true, true, true, true, true);
        c.separation.algorithm = separation;
        table.push((format!("{separation}, everything supplied"), c, facts, store));
    }

    // Each configuration on a thread of its own: (reads checked, χ-separations left cached by a
    // new R2 step, made stale by one).
    let check = |(name, config, facts, store): &(String, PipelineConfig, ScanFacts, MemoryStore)| {
        let (mut checked, mut kept_on_the_toolbox_route, mut stale_elsewhere) = (0, 0, 0);
        let Some((plan, done)) = recorded_as_run(config, facts, store) else { return (0, 0, 0) };
        for x in plan.steps.iter().filter(|s| !matches!(s.stage, Stage::Load | Stage::OutputSpace)) {
            let Some(key) = done.recorded_keys().get(&x.name).cloned() else { continue };
            let mut again = done.shadow();
            again.record(&Completed {
                step: &x.name, algorithm: None, params: &json!({}), outcome: None, key: &format!("{key}, again"),
                inputs: &x.reads, outputs: &x.outputs, decisions: &[],
            }).unwrap();
            let left = again.recorded_keys();
            for y in plan.steps.iter().filter(|y| y.name != x.name && done.recorded_keys().contains_key(&y.name)) {
                if y.reads.iter().any(|i| x.outputs.contains(i)) {
                    assert!(!left.contains_key(&y.name), "{name}: {} reads {:?} from {} and is still cached after it",
                            y.name, y.reads.iter().filter(|i| x.outputs.contains(i)).collect::<Vec<_>>(), x.name);
                    checked += 1;
                    if x.stage == Stage::R2 && y.stage == Stage::ChiSeparation {
                        stale_elsewhere += 1;
                    }
                } else if x.stage == Stage::R2 && y.stage == Stage::ChiSeparation {
                    assert!(left.contains_key(&y.name), "{name}: χ-separation reads no R2 step output: {:?}", y.reads);
                    kept_on_the_toolbox_route += 1;
                }
            }
        }
        (checked, kept_on_the_toolbox_route, stale_elsewhere)
    };
    let (checked, kept_on_the_toolbox_route, stale_elsewhere) = std::thread::scope(|scope| {
        let handles: Vec<_> = table.chunks(8).map(|chunk| scope.spawn(move || chunk.iter().map(check)
            .fold((0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2)))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).fold((0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2))
    });
    assert!(checked > 1000, "too few reads checked: {checked}");
    assert!(kept_on_the_toolbox_route > 0 && stale_elsewhere > 0, "{kept_on_the_toolbox_route} {stale_elsewhere}");
}

/// χ-sepnet's toolbox route on a dry run: the steps that run, what its step records reading,
/// and the R2′ it records using. Nothing is computed, so no network runs.
#[test]
fn the_toolbox_route_estimates_r2prime_unless_it_is_measured() {
    let toolbox = |edit: fn(&mut PipelineConfig)| {
        let mut c = config();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepNet;
        edit(&mut c);
        c
    };
    let dry = |config: &PipelineConfig, facts: &ScanFacts, store: &MemoryStore| {
        let plan = crate::plan::plan(config, facts);
        let mut shadow = store.shadow();
        let records = dry_steps(&plan, &plan.config, facts, &mut shadow).unwrap();
        let chi_sep = records.iter().find(|r| r.id == "chi_separation").unwrap().clone();
        (plan, records.into_iter().map(|r| r.id).collect::<Vec<_>>(), chi_sep)
    };
    let reads_r2prime = |plan: &Plan| plan.steps.iter().find(|s| s.stage == Stage::ChiSeparation)
        .unwrap().reads.contains(&Item::R2prime);
    let (facts, store) = phantom();

    // Nothing asked for: no R2 step, R2* read, the weights fetched for the route.
    let (plan, steps, chi_sep) = dry(&toolbox(|_| {}), &facts, &store);
    assert!(!steps.iter().any(|s| s == "r2_r2prime"), "{steps:?}");
    assert!(steps.iter().any(|s| s == "t2star_r2star"), "{steps:?}");
    assert!(plan.models.contains(&"r2primenet"), "{:?}", plan.models);
    assert!(!reads_r2prime(&plan));
    assert_eq!(chi_sep.decision(crate::record::Topic::ChiSeparationR2prime).unwrap().choice,
               crate::record::Choice::Predicted);

    // Asked for: it runs, for the user, and is not what the route reads.
    let (plan, steps, chi_sep) = dry(&toolbox(|c| c.pipeline.do_r2primemap = true), &facts, &store);
    assert!(steps.iter().any(|s| s == "r2_r2prime"), "{steps:?}");
    assert!(!reads_r2prime(&plan));
    assert_eq!(chi_sep.decision(crate::record::Topic::ChiSeparationR2prime).unwrap().choice,
               crate::record::Choice::Predicted);

    // With a MESE: measured and given to the route, and published, without being asked for.
    let (mut mese_facts, mut mese_store) = phantom();
    with_mese(&mut mese_facts, &mut mese_store);
    let (plan, steps, chi_sep) = dry(&toolbox(|_| {}), &mese_facts, &mese_store);
    assert!(steps.iter().any(|s| s == "r2_r2prime"), "{steps:?}");
    assert!(reads_r2prime(&plan));
    assert!(plan.config.pipeline.do_r2primemap && plan.config.pipeline.do_r2map);
    assert_eq!(chi_sep.decision(crate::record::Topic::ChiSeparationR2prime).unwrap().choice,
               crate::record::Choice::Measured);

    // A MESE R2′ is told not to be measured from: the route's own, and no R2 step.
    let (plan, steps, chi_sep) = dry(
        &toolbox(|c| c.separation.r2prime_strategy = crate::R2PrimeStrategy::R2primenet), &mese_facts, &mese_store);
    assert!(!steps.iter().any(|s| s == "r2_r2prime"), "{steps:?}");
    assert!(!reads_r2prime(&plan));
    assert_eq!(chi_sep.decision(crate::record::Topic::ChiSeparationR2prime).unwrap().choice,
               crate::record::Choice::Predicted);

    // Off 3 T, told to go ahead: the estimate is recorded as uncalibrated with no R2 step.
    let (mut off_facts, off_store) = phantom();
    off_facts.field_strength = 7.0;
    let (_, steps, chi_sep) = dry(
        &toolbox(|c| c.separation.r2primenet.ignore_field_mismatch = true), &off_facts, &off_store);
    assert!(!steps.iter().any(|s| s == "r2_r2prime"), "{steps:?}");
    assert_eq!(chi_sep.decision(crate::record::Topic::R2primenetCalibration).unwrap().choice,
               crate::record::Choice::Uncalibrated);

    // The network alone is given the R2 step's R2′, as before.
    let (plan, steps, chi_sep) = dry(
        &toolbox(|c| c.separation.chi_sepnet.route = crate::ChiSepNetRoute::Native), &facts, &store);
    assert!(steps.iter().any(|s| s == "r2_r2prime"), "{steps:?}");
    assert!(reads_r2prime(&plan));
    assert!(chi_sep.decision(crate::record::Topic::ChiSeparationR2prime).is_none());
}

/// A file supplied with the scan, replaced between two runs over one store (#383): the second
/// run recomputes the steps that read it, and nothing before them. Supplied again unchanged,
/// nothing is recomputed.
#[test]
fn a_replaced_supplied_file_recomputes_only_the_steps_that_read_it() {
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    let grid = crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
    let sphere = |store: &MemoryStore, inside: f64| -> Vec<f64> {
        let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)) else { panic!() };
        m.iter().map(|&v| if v > 50.0 { inside } else { 0.0 }).collect()
    };
    let supply = |store: &mut MemoryStore, item: Item, version: u32| {
        let data = sphere(store, match item { Item::SuppliedSegmentation => 10.0, _ => 0.01 * version as f64 });
        store.insert(item, VolumeData::Real(data));
        store.supply(item, grid.clone(), format!("{item:?} v{version}"));
    };
    let base = || {
        let mut c = config();
        c.pipeline.do_r2starmap = true;
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepIlsqr;
        c
    };

    // χ: χ-separation reads it; the field map and everything else before it are kept.
    let (mut facts, mut store) = phantom();
    facts.custom_qsm = true;
    let mut c = base();
    c.pipeline.do_r2primemap = true;
    c.separation.custom_r2prime_tool = Some("mine".into());
    c.separation.custom_qsm_tool = Some("mine".into());
    facts.custom_r2prime = true;
    supply(&mut store, Item::SuppliedChi, 1);
    supply(&mut store, Item::SuppliedR2prime, 1);
    let first = run_once(&c, &facts, &mut store);
    assert!(first.computed.iter().any(|s| s == "unwrap") && first.computed.iter().any(|s| s == "chi_separation"),
            "{:?}", first.computed);
    supply(&mut store, Item::SuppliedChi, 2);
    assert_eq!(run_once(&c, &facts, &mut store).computed, ["chi_separation"]);
    supply(&mut store, Item::SuppliedChi, 2);
    assert!(run_once(&c, &facts, &mut store).computed.is_empty());
    // R2′: read by the R2/R2′ step and by χ-separation.
    supply(&mut store, Item::SuppliedR2prime, 2);
    assert_eq!(run_once(&c, &facts, &mut store).computed, ["r2_r2prime", "chi_separation"]);

    // R2: read by the R2/R2′ step.
    let (mut facts, mut store) = phantom();
    facts.custom_r2 = true;
    let mut c = config();
    c.pipeline.do_r2map = true;
    c.separation.custom_r2_tool = Some("mine".into());
    supply(&mut store, Item::SuppliedR2, 1);
    assert!(run_once(&c, &facts, &mut store).computed.iter().any(|s| s == "r2_r2prime"));
    supply(&mut store, Item::SuppliedR2, 2);
    assert_eq!(run_once(&c, &facts, &mut store).computed, ["r2_r2prime"]);

    // A segmentation: read by the segmentation step, and the statistics over it after.
    let (mut facts, mut store) = phantom();
    facts.custom_dseg = true;
    let mut c = config();
    c.pipeline.do_segmentation = true;
    c.pipeline.do_analysis = true;
    c.segmentation.custom_dseg_tool = Some("mine".into());
    supply(&mut store, Item::SuppliedSegmentation, 1);
    assert!(run_once(&c, &facts, &mut store).computed.iter().any(|s| s == "segmentation"));
    supply(&mut store, Item::SuppliedSegmentation, 2);
    assert_eq!(run_once(&c, &facts, &mut store).computed, ["segmentation", "analysis"]);

    // A brain mask: the mask step and what reads the mask; the steps before it are kept.
    let (mut facts, mut store) = phantom();
    facts.custom_mask = true;
    let mut c = config();
    c.masking.custom_mask_tool = Some("mine".into());
    let mask = |store: &MemoryStore| Supplied::Found { origin: "mask.nii".into(), data: sphere(store, 1.0), dims: (N, N, N), affine };
    store.supply_mask(mask(&store), "mask.nii v1");
    let first = run_once(&c, &facts, &mut store);
    store.supply_mask(mask(&store), "mask.nii v2");
    let second = run_once(&c, &facts, &mut store);
    assert_eq!(second.computed, first.computed.iter().skip_while(|s| *s != "mask").cloned().collect::<Vec<_>>());
    assert!(second.computed.len() < first.computed.len(), "{:?}", second.computed);
}

/// What a run left in a store: its records, and every output a step recorded.
struct Snapshot {
    recorded: crate::store::Recorded,
    outputs: Vec<(Item, Option<VolumeData>)>,
}

fn snapshot(store: &MemoryStore) -> Snapshot {
    let recorded = store.recorded();
    let outputs = recorded.steps.values().flat_map(|(_, outputs)| outputs)
        .map(|k| Item::from_key(k).unwrap()).map(|item| (item, store.get(&item).cloned())).collect();
    Snapshot { recorded, outputs }
}

/// `store`, re-run with `config`, holds what a fresh run with it (on `fresh`, a new store of
/// the same scan) does.
fn assert_as_if_fresh(config: &PipelineConfig, facts: &ScanFacts, store: &MemoryStore, mut fresh: MemoryStore) {
    run_once(config, facts, &mut fresh);
    assert_holds(config, facts, store, &snapshot(&fresh));
}

/// `store` holds what `want` does: the same steps under the same keys, and the same outputs.
/// Any other step it still records is one `config`'s plan does not have, left by an earlier
/// configuration.
fn assert_holds(config: &PipelineConfig, facts: &ScanFacts, store: &MemoryStore, want: &Snapshot) {
    assert_records_hold(config, facts, store.recorded(), &want.recorded);
    for (item, volume) in &want.outputs {
        assert!(store.get(item) == volume.as_ref(), "{item:?} is not what a fresh run makes");
    }
}

/// `got` records what `want` does: the same steps under the same keys and outputs, and the
/// same reports and annotations. Any other step `got` records is one `config`'s plan does not
/// have, left by an earlier configuration.
fn assert_records_hold(config: &PipelineConfig, facts: &ScanFacts, mut got: crate::store::Recorded,
                       want: &crate::store::Recorded) {
    let plan: Vec<String> = crate::plan::plan(config, facts).steps.iter().map(|s| s.stage.name()).collect();
    got.steps.retain(|step, _| want.steps.contains_key(step) || {
        assert!(!plan.contains(step), "{step} is in the plan but a fresh run does not record it");
        false
    });
    assert_eq!(got.steps, want.steps);
    assert_eq!(got.reports, want.reports);
    assert_eq!(got.annotations, want.annotations);
}

/// The reliable-phase mask is restricted to the brain mask, so a new brain mask redoes it and
/// the reliable pass after it: re-run over the old outputs, the run is a fresh one.
#[test]
fn a_new_brain_mask_redoes_the_reliable_pass_as_a_fresh_run_would() {
    let (facts, mut store) = phantom();
    let mut config = two_pass_config();
    run_once(&config, &facts, &mut store);
    erode_the_brain_mask(&mut config);
    let rerun = run_once(&config, &facts, &mut store);
    for step in ["mask", "mask-reliable", "bgremove-reliable", "invert-reliable", "twopass"] {
        assert!(rerun.computed.iter().any(|c| c == step), "{step}: {:?}", rerun.computed);
    }
    let (Some(VolumeData::Mask(brain)), Some(VolumeData::Mask(reliable))) =
        (store.get(&Item::BrainMask), store.get(&Item::ReliableMask)) else { panic!("no masks") };
    assert!(reliable.iter().zip(brain).all(|(&r, &b)| r == 0 || b != 0), "the reliable mask is in the new brain mask");
    assert_as_if_fresh(&config, &facts, &store, phantom().1);
}

/// R2 is fitted inside the brain mask, so a new mask redoes it — on its own too, with no R2*
/// map in the run whose recomputation would.
#[test]
fn a_new_brain_mask_redoes_r2_as_a_fresh_run_would() {
    let scan = || {
        let (mut facts, mut store) = phantom();
        with_mese(&mut facts, &mut store);
        (facts, store)
    };
    let (facts, mut store) = scan();
    let mut config = config();
    config.pipeline.do_r2map = true;
    run_once(&config, &facts, &mut store);
    erode_the_brain_mask(&mut config);
    let rerun = run_once(&config, &facts, &mut store);
    assert!(rerun.computed.iter().any(|c| c == "r2_r2prime"), "{:?}", rerun.computed);
    assert_as_if_fresh(&config, &facts, &store, scan().1);
}

/// The combination reads the main pass's map as well as the reliable pass's, so a new
/// inversion redoes it.
#[test]
fn a_new_inversion_redoes_the_two_pass_combination_as_a_fresh_run_would() {
    let (facts, mut store) = phantom();
    let mut config = two_pass_config();
    run_once(&config, &facts, &mut store);
    config.inversion.tkd.threshold *= 2.0;
    let rerun = run_once(&config, &facts, &mut store);
    assert_eq!(rerun.computed, ["invert", "invert-reliable", "twopass", "reference-singlepass", "reference"]);
    assert_as_if_fresh(&config, &facts, &store, phantom().1);
}

/// What a run of `config` would do, taken on `store` itself as [`dry_run`] takes it on a
/// shadow: each step that would be computed is recorded under its key, with the outputs the
/// plan says it writes, and nothing is computed. One after another on a
/// [`MemoryStore::shadow`] of a scan, these are what a sequence of runs records, with no
/// algorithm run. Returns the steps that would be computed, in order.
fn pretend_run<V: crate::store::Volumes>(config: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore<V>) -> Vec<String> {
    let plan = crate::plan::plan(config, facts);
    let mut quiet = ();
    let (meta, loaded, _) = load(config, facts, store, &mut quiet).unwrap();
    store.begin(&meta).unwrap();
    let mut computed: Vec<String> = Some(loaded).filter(|l| l.state == StepState::Computed).map(|l| l.id).into_iter().collect();
    let mut r = Run::new(&plan, config, facts, meta, store, &mut quiet);
    r.dry = true;
    for step in plan.steps.iter().filter(|s| s.stage != Stage::Load) {
        let record = r.take(step).unwrap();
        if record.state == StepState::Computed {
            computed.push(record.id);
        }
    }
    computed
}

/// The steps of `config`'s plan that write a pass's raw map: its inversion steps.
fn inversion_steps(config: &PipelineConfig, facts: &ScanFacts) -> Vec<String> {
    crate::plan::plan(config, facts).steps.iter()
        .filter(|s| s.outputs.iter().any(|o| matches!(o, Item::RawChi(_))))
        .map(|s| s.name.clone()).collect()
}

/// The inversion steps are named for their algorithm (`invert`, `tgv`, `qsmart`), and every one
/// writes the pass's raw map. Switching from `a` to another and back in the same store (#373)
/// would leave `a`'s record naming a map the other has since replaced: the other's step forgets
/// it, the switch back recomputes it and every step that reads a raw map, and the store
/// records what `a`'s first (fresh) run did — single- and two-pass, after each of the others.
/// Taken as dry runs one after another on a shadow of the scan ([`pretend_run`]), so nothing
/// is reconstructed: that a dry run says what the run then does is
/// [`switching_inversions_does_what_the_dry_run_says`]'s to check.
fn switch_away_and_back(a: QsmAlgorithm) {
    let algorithms = [QsmAlgorithm::Tkd, QsmAlgorithm::Tgv, QsmAlgorithm::Qsmart];
    for two_pass in [false, true] {
        let mut config = if two_pass { two_pass_config() } else { config() };
        config.inversion.algorithm = a;
        let (facts, scan) = phantom();
        let mut store = scan.shadow();
        assert!(pretend_run(&config, &facts, &mut store).contains(&"reference".to_string()));
        let fresh = store.recorded();
        let steps = inversion_steps(&config, &facts);
        let readers: Vec<String> = crate::plan::plan(&config, &facts).steps.iter()
            .filter(|s| s.reads.iter().any(|r| matches!(r, Item::RawChi(_))))
            .map(|s| s.name.clone()).collect();
        assert!(!readers.is_empty());
        for b in algorithms.into_iter().filter(|&b| b != a) {
            config.inversion.algorithm = b;
            let away = pretend_run(&config, &facts, &mut store);
            for step in inversion_steps(&config, &facts) {
                assert!(away.contains(&step), "{a} → {b}, two-pass {two_pass}: {away:?}");
            }
            let recorded = store.recorded().steps;
            assert!(steps.iter().all(|s| !recorded.contains_key(s)), "{a} → {b}, two-pass {two_pass}: {a}'s map was replaced");
            config.inversion.algorithm = a;
            let back = pretend_run(&config, &facts, &mut store);
            for step in steps.iter().chain(&readers) {
                assert!(back.contains(step), "{a} → {b} → {a}, two-pass {two_pass}: {step} in {back:?}");
            }
            assert_records_hold(&config, &facts, store.recorded(), &fresh);
            assert!(pretend_run(&config, &facts, &mut store).is_empty(), "{a} → {b} → {a}, two-pass {two_pass}");
        }
    }
}

#[test]
fn switching_from_tkd_and_back_gives_what_a_fresh_run_does() {
    switch_away_and_back(QsmAlgorithm::Tkd);
}

#[test]
fn switching_from_tgv_and_back_gives_what_a_fresh_run_does() {
    switch_away_and_back(QsmAlgorithm::Tgv);
}

#[test]
fn switching_from_qsmart_and_back_gives_what_a_fresh_run_does() {
    switch_away_and_back(QsmAlgorithm::Qsmart);
}

/// TKD to QSMART and back, single- and two-pass, run on the phantom: at each switch the dry
/// run says what the run then does, each switch leaves what a fresh run of the new algorithm
/// does, maps and all, and back on TKD nothing is left to do. (QSMART is the cheap one of the
/// steps that share the inversion's output.)
#[test]
fn switching_inversions_does_what_the_dry_run_says() {
    for two_pass in [false, true] {
        let mut config = if two_pass { two_pass_config() } else { config() };
        let (facts, mut store) = phantom();
        expect_then_run(&config, &facts, &mut store);
        let tkd = snapshot(&store);
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        expect_then_run(&config, &facts, &mut store);
        assert_as_if_fresh(&config, &facts, &store, phantom().1);
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        let back = expect_then_run(&config, &facts, &mut store);
        assert!(back.iter().any(|(id, s)| id == "invert" && s.computes()), "two-pass {two_pass}: {back:?}");
        assert_holds(&config, &facts, &store, &tkd);
        let again = expect_then_run(&config, &facts, &mut store);
        assert!(again.iter().all(|(_, s)| matches!(s, ExpectedState::Cached | ExpectedState::Skipped)), "{again:?}");
    }
}
