//! What each step records reading: the plan's inputs, everything it read, and nothing else.

use super::tests::*;
use super::*;
use crate::enums::{BfAlgorithm, QsmAlgorithm, SeparationAlgorithm};
use crate::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
use crate::store::MemoryStore;

/// Every step a dry run of `config` on the scan in `store` would compute, with what it would
/// record reading: computing nothing.
fn dry_recorded(config: &PipelineConfig, facts: &ScanFacts, store: &MemoryStore) -> (Plan, Vec<(String, Vec<Item>)>) {
    // As every entry point takes it ([`PipelineConfig::normalised`]).
    let plan = crate::plan::plan(config, facts);
    let config = &plan.config;
    let mut recorder = Recorder::new(store.shadow());
    dry_steps(&plan, config, facts, &mut recorder).unwrap();
    (plan, recorder.completed.into_iter().map(|(step, inputs, _)| (step, inputs)).collect())
}

/// What a run records each step reading (what `qsmxt` writes into its provenance) is what the
/// plan says it reads (what the pipeline graph draws): for every step, every inversion,
/// single- and multi-echo, one pass and two. Found by dry runs, so nothing is computed.
/// Single-echo TGV reads echo 1's wrapped phase, not the field in ppm, which no step makes in
/// that run.
#[test]
fn every_step_records_reading_what_the_plan_says_it_reads() {
    let check = |config: &PipelineConfig, facts: &ScanFacts, store: &MemoryStore| {
        let (plan, recorded) = dry_recorded(config, facts, store);
        let computed: Vec<&str> = recorded.iter().map(|(s, _)| s.as_str()).collect();
        // The phantom is axial: the run is not resampled, and does not take the steps that
        // would resample it and return it to the acquired grid.
        let skipped = |s: &Stage| matches!(s, Stage::Load | Stage::Motion | Stage::ResampleAxial | Stage::OutputSpace);
        for step in plan.steps.iter().filter(|s| !skipped(&s.stage)) {
            assert!(computed.contains(&step.name.as_str()), "{}: not computed in the dry run: {computed:?}", step.name);
        }
        let unresampled = crate::pipeline_graph::step_details(&plan, &plan.config, facts, Some(false));
        for (step, inputs) in &recorded {
            let at = plan.steps.iter().position(|s| &s.name == step).unwrap();
            assert_eq!(inputs, &unresampled[at].reads, "{step} ({}, {} echo(es), two-pass {})",
                       config.inversion.algorithm, facts.n_echoes, plan.two_pass);
            if !plan.has(Stage::ResampleAxial) {
                assert_eq!(inputs, &plan.steps[at].reads, "{step}: what the plan says, where it cannot resample");
            }
        }
    };
    let one_echo = |facts: &mut ScanFacts| {
        facts.n_echoes = 1;
        facts.echo_times.truncate(1);
    };

    let reliable = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }];
    for algorithm in QsmAlgorithm::ALL {
        for single in [false, true] {
            for two_pass in [false, true] {
                let (mut facts, store) = phantom();
                if single {
                    one_echo(&mut facts);
                }
                let mut c = config();
                c.inversion.algorithm = algorithm;
                c.masking.two_pass = two_pass;
                c.masking.two_pass_sections = Some(reliable.clone());
                check(&c, &facts, &store);
            }
        }
    }

    // The single-echo TGV the plan's table says: echo 1's phase and the mask, nothing else.
    let (mut facts, store) = phantom();
    one_echo(&mut facts);
    let mut c = config();
    c.inversion.algorithm = QsmAlgorithm::Tgv;
    let (_, recorded) = dry_recorded(&c, &facts, &store);
    let tgv = recorded.iter().find(|(s, _)| s == "tgv").unwrap();
    assert_eq!(tgv.1, [Item::BrainMask, Item::Phase(1)]);

    // iQFM, motion correction, and every other output: SWI, SMWI, R2*, R2, R2′,
    // χ-separation, the segmentation and the statistics over it.
    for single in [false, true] {
        let (mut facts, mut store) = phantom();
        with_mese(&mut facts, &mut store);
        if single {
            one_echo(&mut facts);
        }
        let mut c = config();
        c.bg_removal.algorithm = BfAlgorithm::Iqfm;
        let p = &mut c.pipeline;
        (p.do_swi, p.do_smwi, p.do_r2starmap, p.do_r2map, p.do_r2primemap) = (true, true, true, true, true);
        (p.do_chi_separation, p.do_segmentation, p.do_analysis) = (true, true, true);
        c.motion_correction.enabled = !single;
        check(&c, &facts, &store);
    }

    // A supplied mask and segmentation, with the reference measured on a region of it.
    let (mut facts, mut store) = phantom();
    (facts.custom_mask, facts.custom_dseg) = (true, true);
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    let grid = crate::store::Geometry { dims: facts.dims, voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
    store.insert_on_grid(Item::SuppliedSegmentation, vec![10.0; N * N * N], grid);
    let mut c = config();
    c.masking.custom_mask_tool = Some("bet".into());
    c.segmentation.custom_dseg_tool = Some("synthseg".into());
    c.qsm.reference = crate::enums::QsmReference::Region;
    c.qsm.reference_region = Some("thalamus".into());
    c.pipeline.do_analysis = true;
    check(&c, &facts, &store);
}

/// What each step asks its store for while it is computed, of what another step writes, is
/// among what it records reading: a store invalidates a step by what it recorded reading
/// ([`crate::graph::readers`]), so an item read and not recorded would leave the step cached
/// when that item changed. That is the direction that must never be weak, so the steps are
/// computed here, not dry-run: every inversion that runs without a network, single- and
/// multi-echo, one pass and two; every other output, with and without a MESE; motion
/// correction; a supplied mask and segmentation with a region reference; a per-echo QSM; and
/// a resampled run's return to the acquired grid.
#[test]
fn every_step_records_reading_everything_it_reads() {
    fn check(config: &PipelineConfig, facts: &ScanFacts, store: MemoryStore) -> usize {
        let plan = crate::plan::plan(config, facts);
        if !plan.models.is_empty() {
            return 0;
        }
        // What some step other than loading writes: what a store can see change.
        let written: Vec<Item> = plan.steps.iter()
            .filter(|s| !matches!(s.stage, Stage::Load | Stage::OutputSpace))
            .flat_map(|s| s.outputs.iter().copied()).collect();
        let mut recorder = Recorder::new(store);
        // A step that cannot run (χ-separation of one echo has no R2*) stops the run; what
        // was computed before it is checked.
        let _ = run(&plan, config, facts, &mut recorder, &mut ());
        for (step, inputs, outputs) in &recorder.completed {
            let unrecorded: Vec<&Item> = recorder.asked.get(step).into_iter().flatten()
                .filter(|i| written.contains(i) && !inputs.contains(i) && !outputs.contains(i)).collect();
            assert!(unrecorded.is_empty(), "{step} read {unrecorded:?} without recording it ({}, {} echo(es), two-pass {}): it recorded {inputs:?}",
                    config.inversion.algorithm, facts.n_echoes, plan.two_pass);
        }
        let outside: Vec<&Item> = recorder.asked.get("").into_iter().flatten().filter(|i| written.contains(i)).collect();
        assert!(outside.is_empty(), "read outside any step: {outside:?}");
        recorder.completed.len()
    }
    let one_echo = |facts: &mut ScanFacts| {
        facts.n_echoes = 1;
        facts.echo_times.truncate(1);
    };
    let reliable = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }];
    let mut runs: Vec<(PipelineConfig, ScanFacts, MemoryStore)> = Vec::new();
    // One inversion of each kind: those whose plans read and write alike are wired alike, so
    // the first of them stands for the rest (the iterative ones cost seconds here).
    let mut kinds = Vec::new();
    for algorithm in QsmAlgorithm::ALL {
        for single in [false, true] {
            for two_pass in [false, true] {
                let (mut facts, store) = phantom();
                if single {
                    one_echo(&mut facts);
                }
                let mut c = config();
                c.inversion.algorithm = algorithm;
                c.inversion.tgv.iterations = Some(2);
                c.masking.two_pass = two_pass;
                c.masking.two_pass_sections = Some(reliable.clone());
                let plan = crate::plan::plan(&c, &facts);
                let kind: Vec<_> = plan.steps.iter().map(|s| (s.name.clone(), s.reads.clone(), s.outputs.clone())).collect();
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                    runs.push((c, facts, store));
                }
            }
        }
    }

    // Every other output, the statistics over a supplied segmentation: R2′ measured from a
    // MESE, or supplied; multi- and single-echo; with motion correction and without.
    let supplied = |store: &mut MemoryStore, item: Item, inside: f64| {
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
        let data: Vec<f64> = m.iter().enumerate()
            .map(|(i, &v)| if v > 50.0 { if i % N < N / 2 { inside } else { 49.0 } } else { 0.0 }).collect();
        store.insert(item, VolumeData::Real(data));
        let grid = crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
        store.supply(item, grid, format!("{item:?}"));
    };
    for (single, mese, separation) in [(false, true, SeparationAlgorithm::ChiSepIlsqr), (true, true, SeparationAlgorithm::ChiSepIlsqr),
                                        (false, false, SeparationAlgorithm::ChiSepIlsqr), (false, true, SeparationAlgorithm::Decompose)] {
        let (mut facts, mut store) = phantom();
        let mut c = config();
        if mese {
            with_mese(&mut facts, &mut store);
            c.separation.r2prime_strategy = crate::R2PrimeStrategy::Mese;
        } else {
            facts.custom_r2prime = true;
            c.separation.custom_r2prime_tool = Some("mine".into());
            supplied(&mut store, Item::SuppliedR2prime, 5.0);
        }
        if single {
            one_echo(&mut facts);
        }
        facts.custom_dseg = true;
        supplied(&mut store, Item::SuppliedSegmentation, 10.0);
        c.segmentation.custom_dseg_tool = Some("mine".into());
        let p = &mut c.pipeline;
        (p.do_swi, p.do_smwi, p.do_t2starmap, p.do_r2starmap, p.do_r2map, p.do_r2primemap) = (true, true, true, true, mese, true);
        (p.do_chi_separation, p.do_segmentation, p.do_analysis) = (true, true, true);
        c.swi.mip_window = 4;
        c.smwi.mip_window = 4;
        c.separation.algorithm = separation;
        c.separation.decompose.n_inner = 1;
        c.separation.decompose.lsqcurvefit.max_iterations = 2;
        c.motion_correction.enabled = !single;
        runs.push((c, facts, store));
    }

    // A supplied mask, with the reference measured on a region of the supplied segmentation.
    let (mut facts, mut store) = phantom();
    (facts.custom_mask, facts.custom_dseg) = (true, true);
    supplied(&mut store, Item::SuppliedSegmentation, 10.0);
    supplied(&mut store, Item::SuppliedMask, 1.0);
    let mut c = config();
    c.masking.custom_mask_tool = Some("bet".into());
    c.segmentation.custom_dseg_tool = Some("synthseg".into());
    c.qsm.reference = crate::enums::QsmReference::Region;
    c.qsm.reference_region = Some("thalamus".into());
    (c.pipeline.do_segmentation, c.pipeline.do_analysis, c.pipeline.do_r2starmap) = (true, true, true);
    runs.push((c, facts, store));

    // A per-echo QSM, with χ-separation from it.
    let (facts, store) = phantom();
    let mut c = config();
    c.qsm.per_echo = true;
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::Decompose;
    c.separation.decompose.n_inner = 1;
    c.separation.decompose.lsqcurvefit.max_iterations = 2;
    runs.push((c, facts, store));

    // An oblique scan resampled to axial and returned to the acquired grid.
    let (c20, s20) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let (facts, store) = phantom_on([1.0, 0.0, 0.0, 0.0, 0.0, c20, -s20, 0.0, 0.0, s20, c20, 0.0, 0.0, 0.0, 0.0, 1.0]);
    let mut c = config();
    c.pipeline.axial_resampling = true;
    c.pipeline.obliquity_threshold = 5.0;
    c.pipeline.do_swi = true;
    c.swi.mip_window = 4;
    runs.push((c, facts, store));
    // Each run on a thread of its own: they share nothing.
    let computed: usize = std::thread::scope(|scope| {
        let handles: Vec<_> = runs.iter_mut()
            .map(|(c, facts, store)| {
                let store = std::mem::take(store);
                scope.spawn(move || check(c, facts, store))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert!(computed > 100, "too few steps computed to say anything: {computed}");
}

/// A step's key covers everything that changes what it computes besides what it reads. A
/// step is made stale only by a step that wrote what it read ([`crate::graph::readers`]), so
/// a setting it takes that is in no key of its own (only in an upstream step's) would leave it
/// cached, computed under the old setting. One run of a configuration with every output
/// that runs without a network, then one run of each edit of a setting, each on a fresh
/// store: every step that has the same key in both and read the same data in both wrote the
/// same data in both.
#[test]
fn a_step_with_its_key_and_inputs_unchanged_computes_the_same() {
    use crate::enums::{InhomogeneityMethod, LaplacianSolver, QsmReference};
    type Edit = fn(&mut PipelineConfig);
    let base = || {
        let (mut facts, mut store) = phantom();
        with_mese(&mut facts, &mut store);
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
        let dseg: Vec<f64> = m.iter().enumerate()
            .map(|(i, &v)| if v > 50.0 { if i % N < N / 2 { 10.0 } else { 49.0 } } else { 0.0 }).collect();
        store.insert(Item::SuppliedSegmentation, VolumeData::Real(dseg));
        store.supply(Item::SuppliedSegmentation,
                     crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None }, "dseg");
        facts.custom_dseg = true;
        let mut c = config();
        c.segmentation.custom_dseg_tool = Some("mine".into());
        c.masking.inhomogeneity_correction = true;
        c.masking.two_pass = true;
        c.masking.two_pass_sections = Some(vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
            refinements: vec![],
        }]);
        let p = &mut c.pipeline;
        (p.do_swi, p.do_smwi, p.do_t2starmap, p.do_r2starmap, p.do_r2map, p.do_r2primemap) = (true, true, true, true, true, true);
        (p.do_chi_separation, p.do_segmentation, p.do_analysis) = (true, true, true);
        c.swi.mip_window = 4;
        c.smwi.mip_window = 4;
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        c.separation.r2prime_strategy = crate::R2PrimeStrategy::Mese;
        (c, facts, store)
    };
    // What a run computed: each step's key and inputs, and the store it left.
    let computed = |edit: Edit| {
        let (mut c, facts, store) = base();
        edit(&mut c);
        let plan = crate::plan::plan(&c, &facts);
        let mut recorder = Recorder::new(store);
        run(&plan, &c, &facts, &mut recorder, &mut ()).unwrap();
        let keys = recorder.inner.recorded_keys();
        (recorder.completed, keys, recorder.inner)
    };
    // HEIDI is left out: its settings key the two-pass combination explicitly, and it takes
    // a minute here.
    let edits: [(&str, Edit); 16] = [
        ("homogeneity.sigma_mm", |c| c.homogeneity.sigma_mm *= 2.0),
        ("masking.inhomogeneity_method", |c| c.masking.inhomogeneity_method = InhomogeneityMethod::N4),
        ("masking.inhomogeneity_correction", |c| c.masking.inhomogeneity_correction = false),
        ("masking.refinements", |c| c.masking.refinements.push(MaskOp::Erode { iterations: 1 })),
        ("masking.two_pass_sections", |c| c.masking.two_pass_sections.as_mut().unwrap()[0].generator =
            MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(20.0) }),
        ("field_mapping.laplacian_solver", |c| c.field_mapping.laplacian_solver = LaplacianSolver::Fft),
        ("bg_removal.lbv.tol", |c| c.bg_removal.lbv.tol *= 100.0),
        ("inversion.tkd.threshold", |c| c.inversion.tkd.threshold *= 2.0),
        ("qsm.reference", |c| c.qsm.reference = QsmReference::None),
        ("swi.strength", |c| c.swi.strength *= 2.0),
        ("smwi.power", |c| c.smwi.power *= 2.0),
        ("separation.chi_sep_ilsqr", |c| c.separation.chi_sep_ilsqr.cg_max_iter_lsqr += 1),
        ("separation.chi_sep_ilsqr.qsm_ilsqr", |c| c.separation.chi_sep_ilsqr.qsm_ilsqr.max_iter += 1),
        ("pipeline.crop_to_mask", |c| c.pipeline.crop_to_mask = !c.pipeline.crop_to_mask),
        ("pipeline.fft_padding", |c| c.pipeline.fft_padding = !c.pipeline.fft_padding),
        ("pipeline.tukey_window", |c| c.pipeline.tukey_window = 0.2),
    ];
    fn same(a: Option<&VolumeData>, b: Option<&VolumeData>) -> bool {
        match (a, b) {
            (Some(VolumeData::Real(a)), Some(VolumeData::Real(b))) => {
                let scale = a.iter().fold(1.0f64, |m, v| m.max(v.abs()));
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-6 * scale)
            }
            (Some(VolumeData::Mask(a)), Some(VolumeData::Mask(b))) => a == b,
            (None, None) => true,
            _ => false,
        }
    }
    let held = |store: &MemoryStore, item: &Item| (store.get(item).cloned(), store.report(item).cloned());
    let (before, before_keys, before_store) = computed(|_| {});
    assert!(before.len() > 15, "{:?}", before.iter().map(|c| &c.0).collect::<Vec<_>>());
    let mut checked = 0;
    std::thread::scope(|scope| {
        let runs: Vec<_> = edits.iter().map(|&(name, edit)| (name, scope.spawn(move || computed(edit)))).collect();
        for (name, handle) in runs {
            let (after, after_keys, after_store) = handle.join().unwrap();
            for (step, inputs, outputs) in &after {
                let Some((_, read_before, _)) = before.iter().find(|(s, ..)| s == step) else { continue };
                if before_keys.get(step) != after_keys.get(step) || read_before != inputs {
                    continue;
                }
                // Bitwise: an iterative step (χ-separation's LSQR) can turn a rounding-level
                // change in its input (1e-12 relative) into a visible one, so "the same inputs"
                // has to mean identical ones for the outputs to be held to the tolerance.
                let read_same = inputs.iter().all(|i| held(&before_store, i) == held(&after_store, i));
                if !read_same {
                    continue;
                }
                checked += 1;
                for o in outputs {
                    let ((a, ra), (b, rb)) = (held(&before_store, o), held(&after_store, o));
                    assert!(same(a.as_ref(), b.as_ref()) && ra == rb,
                            "{step}: same key and inputs, but {o:?} differs when {name} changes: its key misses it");
                }
            }
        }
    });
    assert!(checked > 50, "too few steps compared: {checked}");
}
