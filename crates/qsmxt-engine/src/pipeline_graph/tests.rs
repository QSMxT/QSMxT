use super::*;
use crate::observe::{Observer, StageEvent, VolumeData};
use crate::plan::tests::facts;
use crate::store::{Completed, Data, Header, MemoryStore, Mese, StageStore, Supplied};

fn graph(config: &PipelineConfig, facts: &ScanFacts) -> PipelineGraph {
    pipeline_graph(&crate::plan::plan(config, facts), config, facts, None)
}

fn ids(g: &PipelineGraph) -> Vec<&str> {
    g.nodes.iter().map(|n| n.id.as_str()).collect()
}

/// The grid of a run on an identity affine, resampled or not.
fn grid(resampled: bool) -> RunMetadata {
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    let f = facts();
    RunMetadata {
        dims: f.dims, voxel_size: (1.0, 1.0, 1.0), affine, n_echoes: f.n_echoes, echo_times: f.echo_times,
        b0_direction: (0.0, 0.0, 1.0), field_strength: 3.0, has_magnitude: true,
        source_geometry: resampled.then_some(((16, 16, 8), affine)), slice_geometry: None,
    }
}

/// Every arrow is a data dependency the cache's invalidation table also has: a step reading
/// another's output is stale when that output changes. The load step stands for the scan, whose
/// supplied inputs are keyed by the steps that read them rather than by the load step.
fn assert_consistent(g: &PipelineGraph) {
    for e in &g.edges {
        assert!(g.node(&e.from).is_some() && g.node(&e.to).is_some(), "{e:?}");
        // The return to the acquired grid is redone whenever any step is (the store puts the
        // working-grid outputs back first), so it is in no step's invalidation set.
        // A region reference is keyed by the segmentation's own key instead, so that
        // segmenting again only re-references a run whose reference is measured on it.
        let keyed = e.from == "segmentation" && e.to.starts_with("reference");
        if e.from != "load" && e.to != "output_space" && !keyed {
            assert!(crate::graph::downstream_of(&e.from).contains(&e.to),
                    "{} reads {:?} from {}, but {} is not invalidated by it", e.to, e.items, e.from, e.to);
        }
        let from = g.nodes.iter().position(|n| n.id == e.from).unwrap();
        let to = g.nodes.iter().position(|n| n.id == e.to).unwrap();
        assert!(from < to, "{e:?} points backwards");
    }
    for n in &g.nodes {
        for key in n.reads.iter().chain(&n.writes) {
            assert!(g.item(key).is_some(), "{key} is not in the item table");
        }
        // Everything a step reads was written by an earlier one (the load step writes the scan).
        for key in &n.reads {
            assert!(g.edges.iter().any(|e| e.to == n.id && e.items.contains(key)), "{}: {key} comes from nowhere", n.id);
        }
    }
}

/// A table of configurations: the steps, their branches and their arrows.
#[test]
fn graphs_of_a_table_of_configurations() {
    // The defaults: one box per step that does something.
    let g = graph(&PipelineConfig::default(), &facts());
    assert_eq!(ids(&g), ["load", "scale_phase", "magnitude", "mask", "unwrap", "bgremove", "invert",
                         "reference", "output_space"]);
    assert!(!g.two_pass && g.nodes.iter().all(|n| n.branch.is_none() && n.enabled_by.is_none()));
    for (from, to) in [("load", "scale_phase"), ("scale_phase", "mask"), ("mask", "unwrap"),
                       ("unwrap", "bgremove"), ("bgremove", "invert"), ("invert", "reference")] {
        assert!(g.has_edge(from, to), "{from} -> {to}: {:?}", g.edges);
    }
    assert!(!g.has_edge("unwrap", "invert"), "a local-field inversion reads the local field, not the total");
    let bg = g.node("bgremove").unwrap();
    assert_eq!((bg.algorithm.as_deref(), bg.algorithm_label.as_deref()), (Some("vsharp"), Some("V-SHARP")));
    assert_eq!(bg.settings, ["SMV kernel radii from 12 to 1 mm"]);
    assert_eq!(g.node("invert").unwrap().algorithm_label.as_deref(), Some("iLSQR"));
    // Whether the outputs go back to the acquired grid depends on the header.
    assert_eq!(g.node("output_space").unwrap().only_when.as_deref(), Some("the run is resampled"));
    assert_consistent(&g);

    // Two-pass: two branches joining at the combination, the field map shared.
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    let g = graph(&config, &facts());
    assert!(g.two_pass);
    let branch = |id: &str| g.node(id).unwrap().branch;
    for id in ["mask", "bgremove", "invert"] {
        assert_eq!(branch(id), Some(Branch::Main), "{id}");
        assert_eq!(branch(&format!("{id}-reliable")), Some(Branch::Reliable), "{id}");
        assert_eq!(g.node(&format!("{id}-reliable")).unwrap().enabled_by.as_deref(), Some("masking.two_pass"));
    }
    assert_eq!(branch("unwrap"), None);
    assert!(g.has_edge("unwrap", "bgremove") && g.has_edge("unwrap", "bgremove-reliable"));
    assert!(g.has_edge("mask", "mask-reliable"), "the reliable mask is restricted to the brain mask");
    assert!(g.has_edge("invert", "twopass") && g.has_edge("invert-reliable", "twopass"));
    assert!(g.has_edge("twopass", "reference") && g.has_edge("invert", "reference-singlepass"));
    assert!(!g.has_edge("invert", "reference"), "the final map is the combination's");
    assert_consistent(&g);

    // Two-pass with TGV: the passes are a mask and a TGV each.
    config.inversion.algorithm = QsmAlgorithm::Tgv;
    let g = graph(&config, &facts());
    assert_eq!(ids(&g)[4..], ["unwrap", "tgv", "mask-reliable", "tgv-reliable", "twopass", "reference-singlepass",
                              "reference", "output_space"]);
    assert!(g.has_edge("unwrap", "tgv") && g.has_edge("unwrap", "tgv-reliable"));
    assert_consistent(&g);

    // SWI and R2* on: optional steps, named by the setting that asked for them.
    let mut config = PipelineConfig::default();
    config.pipeline.do_swi = true;
    config.pipeline.do_r2starmap = true;
    let g = graph(&config, &facts());
    assert_eq!(g.node("swi").unwrap().enabled_by.as_deref(), Some("pipeline.do_swi"));
    assert_eq!(g.node("t2star_r2star").unwrap().enabled_by.as_deref(), Some("pipeline.do_r2starmap"));
    assert!(g.has_edge("scale_phase", "swi") && g.has_edge("mask", "t2star_r2star"));
    assert!(g.node("mask").unwrap().enabled_by.is_none(), "the mask is core");
    assert_consistent(&g);

    // χ-separation with a MESE: R2 and R2′, separation reading the map, R2* and R2′.
    let mut config = PipelineConfig::default();
    config.pipeline.do_r2starmap = true;
    config.pipeline.do_r2map = true;
    config.pipeline.do_r2primemap = true;
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
    let mut f = facts();
    f.mese_echo_times = Some(vec![0.01, 0.02, 0.03, 0.04]);
    let g = graph(&config, &f);
    for from in ["reference", "t2star_r2star", "r2_r2prime", "bgremove", "load"] {
        assert!(g.has_edge(from, "chi_separation"), "{from}: {:?}", g.edges);
    }
    assert!(g.has_edge("t2star_r2star", "r2_r2prime") && g.has_edge("load", "r2_r2prime"));
    assert_eq!(g.node("chi_separation").unwrap().algorithm_label.as_deref(), Some("χ-separation"));
    assert!(g.node("r2_r2prime").unwrap().writes.contains(&item_key(&Item::R2primeCoverage)));
    assert_consistent(&g);

    // QSMART: the total field and the combined magnitude in, five intermediates out.
    let mut config = PipelineConfig::default();
    config.inversion.algorithm = QsmAlgorithm::Qsmart;
    let g = graph(&config, &facts());
    assert!(g.node("bgremove").is_none());
    assert!(g.has_edge("unwrap", "qsmart") && g.has_edge("magnitude", "qsmart") && g.has_edge("qsmart", "reference"));
    let q = g.node("qsmart").unwrap();
    assert_eq!(q.writes.len(), 6);
    assert_eq!(q.settings.first().map(String::as_str), Some("iLSQR inversion"));
    assert_consistent(&g);

    // A magnitude-weighted inversion, and iQSM straight from the echoes.
    config.inversion.algorithm = QsmAlgorithm::Medi;
    let g = graph(&config, &facts());
    assert!(g.has_edge("magnitude", "invert"));
    assert_consistent(&g);
    config.inversion.algorithm = QsmAlgorithm::Iqsm;
    let g = graph(&config, &facts());
    assert!(g.node("unwrap").is_none() && g.has_edge("scale_phase", "invert"));
    assert_eq!(g.node("invert").unwrap().models, ["iqsm"]);
    assert_consistent(&g);

    // Motion correction: the steps after it read its echoes and its coverage mask.
    let mut config = PipelineConfig::default();
    config.motion_correction.enabled = true;
    let g = graph(&config, &facts());
    assert_eq!(g.node("motion").unwrap().enabled_by.as_deref(), Some("motion_correction.enabled"));
    assert!(g.has_edge("motion", "unwrap") && g.has_edge("motion", "bgremove") && !g.has_edge("mask", "unwrap"));
    assert_consistent(&g);

    // Uncombined coils: the combination's algorithm on the phase-scaling box.
    let mut f = facts();
    f.n_coils = 4;
    let g = graph(&PipelineConfig::default(), &f);
    assert_eq!(g.node("scale_phase").unwrap().algorithm.as_deref(), Some("mcpc3ds"));
    assert_eq!(g.node("load").unwrap().writes.len(), 4 * 5 * 2);
    assert_consistent(&g);

    // The grid known: a resampled run returns its published outputs; one that is not has no
    // such step.
    let config = PipelineConfig::default();
    let plan = crate::plan::plan(&config, &facts());
    let resampled = pipeline_graph(&plan, &config, &facts(), Some(&grid(true)));
    assert!(resampled.node("output_space").unwrap().only_when.is_none());
    for from in ["mask", "magnitude", "unwrap", "reference"] {
        assert!(resampled.has_edge(from, "output_space"), "{from}");
    }
    assert_consistent(&resampled);
    let acquired = pipeline_graph(&plan, &config, &facts(), Some(&grid(false)));
    assert!(acquired.node("output_space").is_none());

    // Segmentation, a region reference on it and the statistics.
    let mut config = PipelineConfig::default();
    config.pipeline.do_segmentation = true;
    config.pipeline.do_analysis = true;
    config.pipeline.do_r2starmap = true;
    config.qsm.reference = QsmReference::Region;
    config.qsm.reference_region = Some("thalamus".into());
    config.segmentation.custom_dseg_tool = Some("mine".into());
    let mut f = facts();
    f.custom_dseg = true;
    let g = graph(&config, &f);
    assert_eq!(g.node("segmentation").unwrap().reads, ["SuppliedSegmentation"]);
    assert_eq!(g.node("segmentation").unwrap().enabled_by.as_deref(), Some("pipeline.do_segmentation"));
    for from in ["segmentation", "reference", "t2star_r2star"] {
        assert!(g.has_edge(from, "analysis"), "{from}");
    }
    assert!(g.has_edge("segmentation", "reference"));
    assert_consistent(&g);
}

#[test]
fn the_default_graph_in_mermaid() {
    let g = graph(&PipelineConfig::default(), &facts());
    let expected = r#"flowchart TD
    load["Load"]
    scale_phase["Phase scaling"]
    magnitude["Combined magnitude<br/><i>root sum of squares</i><br/><small>inhomogeneity correction</small>"]
    mask["Brain mask<br/><small>phase quality: Otsu threshold, dilation, hole-filling, erosion</small>"]
    unwrap["Field map<br/><i>ROMEO</i><br/><small>phase offsets removed</small><br/><small>B0 by weighted average</small>"]
    bgremove["Background-field removal<br/><i>V-SHARP</i><br/><small>SMV kernel radii from 12 to 1 mm</small>"]
    invert["Dipole inversion<br/><i>iLSQR</i>"]
    reference["Referencing<br/><i>mean</i>"]
    output_space["Return to the acquired grid<br/><small>only when the run is resampled</small>"]
    load -->|"Phase (as acquired), Magnitude (as acquired)"| scale_phase
    scale_phase -->|"Magnitude"| magnitude
    scale_phase -->|"Phase, Magnitude"| mask
    scale_phase -->|"Phase, Magnitude"| unwrap
    mask -->|"Brain mask"| unwrap
    unwrap -->|"Total field"| bgremove
    mask -->|"Brain mask"| bgremove
    bgremove -->|"Local field, Local field mask"| invert
    invert -->|"Unreferenced susceptibility"| reference
    bgremove -->|"Local field mask"| reference
    reference -->|"Susceptibility, Susceptibility support"| output_space
    mask -->|"Brain mask"| output_space
    magnitude -->|"Combined magnitude"| output_space
    unwrap -->|"Total field, Field map magnitude"| output_space
    classDef conditional stroke-dasharray: 2 3
    class output_space conditional
"#;
    assert_eq!(g.to_mermaid(), expected);
}

#[test]
fn a_two_pass_graph_in_mermaid() {
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    config.inversion.algorithm = QsmAlgorithm::Tgv;
    let f = facts();
    let g = pipeline_graph(&crate::plan::plan(&config, &f), &config, &f, Some(&grid(false)));
    let text = g.to_mermaid();
    assert!(text.contains("    subgraph main_pass[\"Main pass\"]\n        mask[\"Brain mask"), "{text}");
    assert!(text.contains("    subgraph reliable_pass[\"Reliable pass\"]\n        mask_reliable[\"Reliable-phase mask"), "{text}");
    assert!(text.contains("    tgv_reliable -->|\"Unreferenced susceptibility (reliable pass)\"| twopass\n"), "{text}");
    assert!(text.contains("class mask_reliable,tgv_reliable,twopass,reference_singlepass optional"), "{text}");
    assert!(!text.contains("output_space"), "{text}");
}

#[test]
fn the_graph_round_trips_through_json() {
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    config.pipeline.do_swi = true;
    let g = graph(&config, &facts());
    let json = serde_json::to_string(&g).unwrap();
    let back: PipelineGraph = serde_json::from_str(&json).unwrap();
    assert_eq!(back, g);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["nodes"][0]["id"], "load");
    assert_eq!(v["nodes"].as_array().unwrap().iter().find(|n| n["id"] == "invert-reliable").unwrap()["branch"], "reliable");
    let chi = v["items"].as_array().unwrap().iter().find(|i| i["key"] == "Chi").unwrap();
    assert_eq!((&chi["units"], &chi["kind"], &chi["bids_name"], &chi["item"]),
               (&serde_json::json!("ppm"), &serde_json::json!("chi"), &serde_json::json!("Chimap"), &serde_json::json!("Chi")));
}

/// A [`MemoryStore`] that also keeps what each computed step recorded reading and writing.
struct Recorder {
    inner: MemoryStore,
    /// Each computed step, what it read and what it wrote.
    completed: Vec<Recorded>,
}

type Recorded = (String, Vec<Item>, Vec<Item>);

impl StageStore for Recorder {
    type Error = crate::error::EngineError;
    fn input_identity(&mut self) -> serde_json::Value { self.inner.input_identity() }
    fn header(&mut self) -> Result<Header, Self::Error> { self.inner.header() }
    fn sidecar(&mut self) -> Result<Option<crate::scan::QsmSidecar>, Self::Error> { self.inner.sidecar() }
    fn coils(&mut self) -> Vec<u32> { self.inner.coils() }
    fn cached_scan(&mut self, key: &str) -> Option<RunMetadata> { self.inner.cached_scan(key) }
    fn record_scan(&mut self, key: &str, meta: &RunMetadata, decisions: &[crate::record::Decision]) -> Result<(), Self::Error> {
        self.inner.record_scan(key, meta, decisions)
    }
    fn decisions(&mut self, step: &str) -> Vec<crate::record::Decision> { self.inner.decisions(step) }
    fn has(&mut self, item: &Item) -> bool { self.inner.has(item) }
    fn real(&mut self, item: &Item) -> Result<Vec<f64>, Self::Error> { self.inner.real(item) }
    fn mask(&mut self, item: &Item) -> Result<Vec<u8>, Self::Error> { self.inner.mask(item) }
    fn supplied_mask(&mut self) -> Supplied { self.inner.supplied_mask() }
    fn mese(&mut self) -> Option<Mese> { self.inner.mese() }
    fn supplied(&mut self, item: &Item) -> Supplied { self.inner.supplied(item) }
    fn identity(&mut self, item: &Item) -> serde_json::Value { self.inner.identity(item) }
    fn recorded_key(&mut self, step: &str) -> Option<String> { self.inner.recorded_key(step) }
    fn cached(&mut self, step: &str, key: &str) -> bool { self.inner.cached(step, key) }
    fn start(&mut self, step: &str) { self.inner.start(step) }
    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), Self::Error> { self.inner.put(item, data) }
    fn annotate(&mut self, item: &Item, fields: serde_json::Map<String, serde_json::Value>) -> Result<(), Self::Error> {
        self.inner.annotate(item, fields)
    }
    fn report(&mut self, item: &Item) -> Option<serde_json::Value> { StageStore::report(&mut self.inner, item) }
    fn annotation(&mut self, item: &Item) -> serde_json::Map<String, serde_json::Value> { self.inner.annotation(item) }
    fn discard(&mut self, step: &str, outputs: &[Item]) -> Result<(), Self::Error> { self.inner.discard(step, outputs) }
    fn record(&mut self, c: &Completed<'_>) -> Result<(), Self::Error> {
        self.completed.push((c.step.to_string(), c.inputs.to_vec(), c.outputs.to_vec()));
        self.inner.record(c)
    }
}

/// The algorithm each step reported starting with.
#[derive(Default)]
struct Algorithms(Vec<(String, Option<String>)>);

impl Observer for Algorithms {
    fn stage(&mut self, step: &str, algorithm: Option<&str>, event: StageEvent<'_>) {
        if matches!(event, StageEvent::Started(_)) {
            self.0.push((step.to_string(), algorithm.map(str::to_string)));
        }
    }
}

/// Run `config` on the phantom and check the graph against what the run did: every step it
/// computed is a box reporting the box's algorithm, and read and wrote only what the box says;
/// every box (bar one that may do nothing) was computed.
fn check_against_a_run(config: &PipelineConfig, facts: &ScanFacts, store: MemoryStore) {
    let plan = crate::plan::plan(config, facts);
    let mut store = Recorder { inner: store, completed: Vec::new() };
    let header = store.header().unwrap();
    let meta = crate::scan::resolve_geometry(header.dims, header.voxel_size, header.affine, facts, config);
    let mut observer = Algorithms::default();
    crate::run::run(&plan, config, facts, &mut store, &mut observer).unwrap();
    let g = pipeline_graph(&plan, config, facts, Some(&meta));
    assert_consistent(&g);
    for (step, algorithm) in observer.0.iter().filter(|(s, _)| s != "load") {
        let node = g.node(step).unwrap_or_else(|| panic!("{step} ran, and is not in the graph {:?}", ids(&g)));
        assert_eq!(&node.algorithm, algorithm, "{step}");
        // The plan's step carries the observer's name for it, and the same algorithm.
        let planned = plan.steps.iter().find(|s| &s.name == step).unwrap_or_else(|| panic!("{step} is not a plan step"));
        assert_eq!(&planned.algorithm, algorithm, "{step}");
    }
    for (step, inputs, outputs) in &store.completed {
        let planned = plan.steps.iter().find(|s| &s.name == step).unwrap();
        assert!(outputs.iter().all(|o| planned.outputs.contains(o)), "{step}: {outputs:?} vs {:?}", planned.outputs);
        let node = g.node(step).unwrap();
        for item in inputs {
            assert!(node.reads.contains(&item_key(item)), "{step} read {item:?}; the graph says {:?}", node.reads);
        }
        for item in outputs {
            assert!(node.writes.contains(&item_key(item)), "{step} wrote {item:?}; the graph says {:?}", node.writes);
        }
    }
    let ran: Vec<&str> = store.completed.iter().map(|(s, ..)| s.as_str()).collect();
    for n in g.nodes.iter().filter(|n| n.id != "load" && n.only_when.is_none()) {
        assert!(ran.contains(&n.id.as_str()), "{} is drawn, and did not run: {ran:?}", n.id);
    }
}

#[test]
fn the_graph_is_what_a_run_does() {
    use crate::run::tests::{config, phantom, phantom_on, with_mese};
    let (f, s) = phantom();
    check_against_a_run(&config(), &f, s);

    let (f, s) = phantom();
    let mut c = config();
    c.masking.two_pass = true;
    c.masking.two_pass_sections = Some(vec![crate::masking::MaskSection {
        input: crate::masking::MaskingInput::Magnitude,
        generator: crate::masking::MaskOp::Threshold { method: crate::masking::MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }]);
    check_against_a_run(&c, &f, s);

    let (mut f, mut s) = phantom();
    with_mese(&mut f, &mut s);
    let mut c = config();
    c.pipeline.do_swi = true;
    c.pipeline.do_smwi = true;
    c.pipeline.do_r2starmap = true;
    c.pipeline.do_r2map = true;
    c.pipeline.do_r2primemap = true;
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
    c.swi.mip_window = 4;
    c.smwi.mip_window = 4;
    check_against_a_run(&c, &f, s);

    let (f, s) = phantom();
    let mut c = config();
    c.motion_correction.enabled = true;
    check_against_a_run(&c, &f, s);

    let (f, s) = phantom();
    let mut c = config();
    c.inversion.algorithm = QsmAlgorithm::Tgv;
    check_against_a_run(&c, &f, s);

    // Oblique, resampled to axial and returned.
    let (cos, sin) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, cos, -sin, 0.0, 0.0, sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0];
    let (f, s) = phantom_on(affine);
    let mut c = config();
    c.pipeline.axial_resampling = true;
    c.pipeline.obliquity_threshold = 5.0;
    check_against_a_run(&c, &f, s);

    // A supplied segmentation, measured on by referencing, and the statistics over it.
    let (mut f, mut s) = phantom();
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    let Some(VolumeData::Real(m)) = s.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
    let n = f.dims.0;
    let dseg: Vec<f64> = m.iter().enumerate().map(|(i, &v)| if v > 50.0 { if i % n < n / 2 { 10.0 } else { 49.0 } } else { 0.0 }).collect();
    s.insert_on_grid(Item::SuppliedSegmentation, dseg, crate::store::Geometry { dims: f.dims, voxel_size: (1.0, 1.0, 1.0), affine });
    f.custom_dseg = true;
    let mut c = config();
    c.pipeline.do_segmentation = true;
    c.pipeline.do_analysis = true;
    c.pipeline.do_r2starmap = true;
    c.segmentation.custom_dseg_tool = Some("mine".into());
    c.qsm.reference = QsmReference::Region;
    c.qsm.reference_region = Some("thalamus".into());
    check_against_a_run(&c, &f, s);
}

/// The serialised plan names each step as the observer does, labels it, says which algorithm
/// it runs and what it reads and writes (#348).
#[test]
fn plan_steps_carry_their_name_label_algorithm_and_outputs() {
    let plan = crate::plan::plan(&PipelineConfig::default(), &facts());
    let bg = plan.steps.iter().find(|s| s.stage == Stage::BackgroundRemoval(Pass::Main)).unwrap();
    assert_eq!((bg.name.as_str(), bg.label.as_str()), ("bgremove", "Background-field removal"));
    assert_eq!((bg.algorithm.as_deref(), bg.algorithm_label.as_deref()), (Some("vsharp"), Some("V-SHARP")));
    assert_eq!(bg.reads, [Item::FieldPpm, Item::BrainMask]);
    assert_eq!(bg.outputs, [Item::LocalField(Pass::Main), Item::LocalFieldMask(Pass::Main)]);
    let mask = plan.steps.iter().find(|s| s.name == "mask").unwrap();
    assert_eq!((mask.algorithm.as_deref(), mask.outputs.as_slice()), (None, [Item::BrainMask].as_slice()));
    // Motion correction is off: in the plan (its earlier output is forgotten), touching nothing.
    let motion = plan.steps.iter().find(|s| s.name == "motion").unwrap();
    assert!(motion.reads.is_empty() && motion.outputs.is_empty());
    let v = serde_json::to_value(&plan).unwrap();
    let step = &v["steps"][5];
    assert_eq!((&step["name"], &step["label"], &step["algorithm"], &step["algorithm_label"]),
               (&serde_json::json!("unwrap"), &serde_json::json!("Field map"), &serde_json::json!("phase_offset_removal"),
                &serde_json::json!("ROMEO")));
    assert_eq!(step["outputs"][0], serde_json::json!("FieldPpm"));
}

/// A dataset's graphs merged: identical graphs give that graph; where runs differ, every box once,
/// one only some runs have placed where it runs and drawn as conditional.
#[test]
fn a_datasets_graphs_merge_into_one() {
    let config = PipelineConfig::default();
    let multi = graph(&config, &facts());
    assert_eq!(PipelineGraph::merged(&[multi.clone(), multi.clone()]), Some(multi.clone()));
    assert_eq!(PipelineGraph::merged(&[]), None);
    let mut swi = config.clone();
    swi.pipeline.do_swi = true;
    let mut no_mag = facts();
    no_mag.has_magnitude = false;
    let with_swi = graph(&swi, &facts());
    let without = graph(&swi, &no_mag);
    let merged = PipelineGraph::merged(&[without.clone(), with_swi.clone()]).unwrap();
    let ids: Vec<&str> = merged.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut want: Vec<&str> = with_swi.nodes.iter().map(|n| n.id.as_str()).collect();
    for n in &without.nodes {
        if !want.contains(&n.id.as_str()) { want.push(&n.id); }
    }
    assert_eq!(ids.len(), want.len(), "{ids:?} vs {want:?}");
    assert!(merged.node("swi").unwrap().only_when.is_some());
    assert!(merged.node("invert").unwrap().only_when.is_none());
    for e in with_swi.edges.iter().chain(&without.edges) {
        assert!(merged.edges.contains(e));
    }
}

/// Graphs drawn from a spread of configurations: no two boxes overlap, every arrow runs from its
/// source's bottom to its target's top in horizontal and vertical segments, every label is inside
/// the drawing, and the SVG has a box per step and an arrow per drawn edge. With
/// `QSMXT_WRITE_GRAPH_SVG=<dir>` set, each is written there to look at (and each layout as JSON).
#[test]
fn graphs_lay_out_and_draw_as_svg() {
    let mut cases: Vec<(&str, PipelineConfig, ScanFacts)> = vec![("default", PipelineConfig::default(), facts())];
    let mut c = PipelineConfig::default();
    c.masking.two_pass = true;
    c.inversion.algorithm = QsmAlgorithm::Tgv;
    cases.push(("two-pass", c, facts()));
    let mut c = PipelineConfig::default();
    c.pipeline.do_swi = true;
    c.pipeline.do_t2starmap = true;
    c.pipeline.do_r2starmap = true;
    c.pipeline.do_segmentation = true;
    c.pipeline.do_analysis = true;
    c.masking.two_pass = true;
    cases.push(("many-outputs", c, facts()));
    let mut c = PipelineConfig::default();
    c.pipeline.do_swi = true;
    c.pipeline.do_smwi = true;
    let mut f = facts();
    f.has_magnitude = false;
    cases.push(("no-magnitude", c, f));
    let out_dir = std::env::var_os("QSMXT_WRITE_GRAPH_SVG").map(std::path::PathBuf::from);
    for (name, config, f) in cases {
        let g = graph(&config, &f);
        let l = g.layout();
        for (i, a) in l.nodes.iter().enumerate() {
            assert!(a.x >= 0.0 && a.y >= 0.0 && a.x + a.w <= l.width && a.y + a.h <= l.height, "{name}: {} outside", a.id);
            for b in &l.nodes[i + 1..] {
                let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{name}: {} overlaps {}", a.id, b.id);
            }
        }
        for e in &l.edges {
            let from = l.nodes.iter().find(|n| n.id == e.from).unwrap();
            let to = l.nodes.iter().find(|n| n.id == e.to).unwrap();
            let (first, last) = (e.points[0], *e.points.last().unwrap());
            assert_eq!(first.1, from.y + from.h, "{name}: {}->{}", e.from, e.to);
            assert!(first.0 >= from.x && first.0 <= from.x + from.w);
            assert_eq!(last.1, to.y, "{name}: {}->{}", e.from, e.to);
            assert!(last.0 >= to.x && last.0 <= to.x + to.w);
            for w in e.points.windows(2) {
                assert!(w[0].0 == w[1].0 || w[0].1 == w[1].1, "{name}: a diagonal in {}->{}", e.from, e.to);
            }
            let b = e.label_box;
            assert!(b.x >= 0.0 && b.x + b.w <= l.width, "{name}: label of {}->{} outside", e.from, e.to);
        }
        if g.two_pass {
            assert_eq!(l.groups.len(), 2, "{name}");
        }
        let svg = g.to_svg();
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\"") && svg.ends_with("</svg>\n"));
        assert_eq!(svg.matches("<g class=\"g-node").count(), g.nodes.len(), "{name}");
        assert_eq!(svg.matches("<g class=\"g-edge").count(), l.edges.len(), "{name}");
        assert_eq!(svg.matches("<g").count(), svg.matches("</g>").count(), "{name}");
        assert!(!svg.contains("NaN") && !svg.contains("inf"), "{name}");
        if let Some(dir) = &out_dir {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join(format!("{name}.svg")), &svg).unwrap();
            std::fs::write(dir.join(format!("{name}.graph.json")), serde_json::to_string(&g).unwrap()).unwrap();
            std::fs::write(dir.join(format!("{name}.layout.json")), serde_json::to_string(&l).unwrap()).unwrap();
        }
    }
}

/// Text in a box is cut to its width, a long word too, and XML in a label is escaped.
#[test]
fn box_text_wraps_and_svg_escapes() {
    assert_eq!(layout::wrap("one two three", 7), ["one two", "three"]);
    assert_eq!(layout::wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    let mut g = graph(&PipelineConfig::default(), &facts());
    g.nodes[0].settings.push("a < b & \"c\"".into());
    let svg = g.to_svg();
    assert!(svg.contains("a &lt; b &amp; &quot;c&quot;"), "{svg}");
}

/// The plain-text graph names each step, its algorithm, settings and the steps it follows.
#[test]
fn the_graph_as_text() {
    let text = graph(&PipelineConfig::default(), &facts()).to_text();
    assert!(text.starts_with("  Load\n  Phase scaling\n      after: Load\n"), "{text}");
    assert!(text.contains("  Background-field removal: V-SHARP\n      SMV kernel radii from 12 to 1 mm\n      after: Field map, Brain mask\n"), "{text}");
    assert!(text.contains("      only when the run is resampled\n"), "{text}");
}

/// Runs that set a step up differently (a mask supplied for one, computed for another): the
/// merged box lists both ways.
#[test]
fn a_box_the_runs_set_up_differently_lists_each_way() {
    let mut config = PipelineConfig::default();
    config.masking.custom_mask_tool = Some("bet".into());
    let mut supplied = facts();
    supplied.custom_mask = true;
    let a = graph(&config, &supplied);
    let b = graph(&config, &facts());
    let merged = PipelineGraph::merged(&[a.clone(), b.clone()]).unwrap();
    let mask = merged.node("mask").unwrap();
    assert_eq!(mask.settings[0], "supplied (bet)");
    assert!(b.node("mask").unwrap().settings.iter().all(|s| mask.settings.contains(s)), "{:?}", mask.settings);
}
