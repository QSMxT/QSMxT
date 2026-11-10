use super::*;
use crate::observe::{Observer, StageEvent, VolumeData};
use crate::plan::tests::facts;
use crate::run::tests::Recorder;
use crate::store::{MemoryStore, StageStore};

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
        source_geometry: resampled.then_some(((16, 16, 8), affine)), slice_geometry: None, orientation: None,
    }
}

/// Every arrow is a data dependency the fixed invalidation table also has, so a step recorded
/// without what it read ([`crate::graph::readers`]'s fallback) is stale whenever one it reads from
/// changes. The load step stands for the scan, whose
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
    // The defaults keep the acquired grid whatever the header says: nothing resamples, and nothing
    // returns to the acquired grid.
    assert_eq!(ids(&g), ["load", "scale_phase", "magnitude", "mask", "unwrap", "bgremove", "invert",
                         "reference"]);
    assert!(!g.two_pass && g.nodes.iter().all(|n| n.branch.is_none() && n.enabled_by.is_none()));
    // Phase scaling makes the phase; the magnitude is the scan's, as it was given.
    assert_eq!(g.node("scale_phase").unwrap().writes, (1..=5).map(|e| item_key(&Item::Phase(e))).collect::<Vec<_>>());
    assert!(g.has_edge("load", "magnitude") && !g.has_edge("scale_phase", "magnitude"));
    for (from, to) in [("load", "scale_phase"), ("scale_phase", "mask"), ("load", "mask"), ("mask", "unwrap"),
                       ("unwrap", "bgremove"), ("bgremove", "invert"), ("invert", "reference")] {
        assert!(g.has_edge(from, to), "{from} -> {to}: {:?}", g.edges);
    }
    assert!(!g.has_edge("unwrap", "invert"), "a local-field inversion reads the local field, not the total");
    assert!(g.node("unwrap").unwrap().writes.contains(&item_key(&Item::FieldMask)));
    let bg = g.node("bgremove").unwrap();
    assert!(bg.reads.contains(&item_key(&Item::FieldMask)) && !bg.reads.contains(&item_key(&Item::BrainMask)),
            "background removal works where the field is defined: {:?}", bg.reads);
    assert_eq!((bg.algorithm.as_deref(), bg.algorithm_label.as_deref()), (Some("vsharp"), Some("V-SHARP")));
    assert_eq!(bg.settings, ["SMV kernel radii from 12 to 2 mm in 1 mm steps"]);
    assert_eq!(g.node("invert").unwrap().algorithm_label.as_deref(), Some("iLSQR"));
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
    assert!(g.has_edge("unwrap", "mask-reliable"), "the reliable mask is restricted to the mask of the total field");
    assert!(g.has_edge("invert", "twopass") && g.has_edge("invert-reliable", "twopass"));
    assert!(g.has_edge("twopass", "reference") && g.has_edge("invert", "reference-singlepass"));
    assert!(!g.has_edge("invert", "reference"), "the final map is the combination's");
    assert_consistent(&g);

    // Two-pass with TGV: the passes are a mask and a TGV each.
    config.inversion.algorithm = QsmAlgorithm::Tgv;
    let g = graph(&config, &facts());
    assert_eq!(ids(&g)[4..], ["unwrap", "tgv", "mask-reliable", "tgv-reliable", "twopass", "reference-singlepass",
                              "reference"]);
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
    for from in ["t2star_r2star", "r2_r2prime", "bgremove", "load"] {
        assert!(g.has_edge(from, "chi_separation"), "{from}: {:?}", g.edges);
    }
    // χ-sep iLSQR and χ-sep MEDI make their QSM from the local field; DECOMPOSE separates the run's.
    assert!(!g.has_edge("reference", "chi_separation"), "{:?}", g.edges);
    config.separation.algorithm = SeparationAlgorithm::ChiSepMedi;
    assert!(!graph(&config, &f).has_edge("reference", "chi_separation"));
    config.separation.algorithm = SeparationAlgorithm::Decompose;
    assert!(graph(&config, &f).has_edge("reference", "chi_separation"));
    config.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
    assert!(g.has_edge("t2star_r2star", "r2_r2prime") && g.has_edge("load", "r2_r2prime"));
    assert_eq!(g.node("chi_separation").unwrap().algorithm_label.as_deref(), Some("χ-separation"));
    assert!(g.node("r2_r2prime").unwrap().writes.contains(&item_key(&Item::R2primeCoverage)));
    assert_consistent(&g);

    // QSMART, its inputs prepared as its own (set here; nothing sets them for it): the total field and the echo fit's reliability map
    // from QSMART's Laplacian unwrapping and echo fit, every echo's magnitude for its own vessel
    // magnitude (kept), and five intermediates of its own out.
    let mut config = PipelineConfig::default();
    config.inversion.algorithm = QsmAlgorithm::Qsmart;
    config.field_mapping.unwrapping_algorithm = crate::enums::UnwrappingAlgorithm::Laplacian;
    config.field_mapping.laplacian_solver = crate::enums::LaplacianSolver::Mirror;
    config.field_mapping.b0_estimation = crate::enums::B0Estimation::QsmartEchofit;
    let g = graph(&config, &facts());
    assert!(g.node("bgremove").is_none());
    assert!(g.has_edge("unwrap", "qsmart") && g.has_edge("load", "qsmart") && g.has_edge("qsmart", "reference"));
    assert!(!g.has_edge("magnitude", "qsmart"));
    let u = g.node("unwrap").unwrap();
    assert_eq!(u.algorithm_label.as_deref(), Some("Laplacian"));
    assert!(u.writes.contains(&item_key(&Item::FieldReliability)), "{:?}", u.writes);
    assert!(u.settings.contains(&"mirror solver".to_string()) && u.settings.contains(&"B0 by QSMART's echo fit".to_string()),
            "{:?}", u.settings);
    assert!(!u.settings.iter().any(|s| s.contains("raw phase")), "{:?}", u.settings);
    config.field_mapping.laplacian_unmasked = true;
    let g = graph(&config, &facts());
    let u = g.node("unwrap").unwrap();
    assert!(u.settings.contains(&"raw phase over the whole volume".to_string()), "{:?}", u.settings);
    let q = g.node("qsmart").unwrap();
    assert_eq!(q.writes.len(), 7);
    assert!(q.reads.contains(&item_key(&Item::FieldReliability)));
    assert!(q.writes.contains(&item_key(&Item::QsmartIntermediate(Pass::Main, crate::stages::qsmart::VESSEL_MAGNITUDE))));
    assert_eq!(q.settings.first().map(String::as_str), Some("iLSQR inversion"));
    assert_consistent(&g);
    // The combined magnitude, and the general field mapping: as before.
    config.inversion.qsmart.vessel_magnitude = QsmartVesselMagnitude::Combined;
    config.field_mapping = Default::default();
    let g = graph(&config, &facts());
    assert!(g.has_edge("magnitude", "qsmart"));
    let q = g.node("qsmart").unwrap();
    assert_eq!(q.writes.len(), 6);
    assert!(!q.reads.contains(&item_key(&Item::FieldReliability)));
    assert!(q.settings.contains(&"vessels on the combined magnitude".to_string()), "{:?}", q.settings);
    assert!(!g.node("unwrap").unwrap().writes.contains(&item_key(&Item::FieldReliability)));
    assert_consistent(&g);
    let mut config = PipelineConfig::default();

    // A magnitude-weighted inversion, one weighted by every echo's magnitude, and iQSM straight
    // from the echoes.
    config.inversion.algorithm = QsmAlgorithm::Ndi;
    let g = graph(&config, &facts());
    assert!(g.has_edge("magnitude", "invert"));
    assert_consistent(&g);
    config.inversion.algorithm = QsmAlgorithm::Medi;
    let g = graph(&config, &facts());
    assert!(g.has_edge("load", "invert") && !g.has_edge("magnitude", "invert"), "every echo's magnitude, as given");
    assert_consistent(&g);
    config.inversion.algorithm = QsmAlgorithm::Iqsm;
    let g = graph(&config, &facts());
    // iQSM only understands axial data, so the echoes it reads may be resampled first.
    assert!(g.node("unwrap").is_none() && g.has_edge("resample_axial", "invert"));
    assert_eq!(g.node("resample_axial").unwrap().enabled_by, None);
    assert_eq!(g.node("invert").unwrap().models, ["iqsm"]);
    assert_consistent(&g);

    // Motion correction: the steps after it read its echoes and its coverage mask.
    let mut config = PipelineConfig::default();
    config.motion_correction.enabled = true;
    let g = graph(&config, &facts());
    assert_eq!(g.node("motion").unwrap().enabled_by.as_deref(), Some("motion_correction.enabled"));
    assert!(g.has_edge("motion", "unwrap") && g.has_edge("unwrap", "bgremove") && !g.has_edge("mask", "unwrap"));
    assert_consistent(&g);

    // Uncombined coils: the combination's algorithm on the phase-scaling box.
    let mut f = facts();
    f.n_coils = 4;
    let g = graph(&PipelineConfig::default(), &f);
    assert_eq!(g.node("scale_phase").unwrap().algorithm.as_deref(), Some("mcpc3ds"));
    assert_eq!(g.node("load").unwrap().writes.len(), 4 * 5 * 2);
    assert!(g.has_edge("scale_phase", "magnitude"), "the combination makes the magnitude");
    assert!(g.node("scale_phase").unwrap().writes.contains(&item_key(&Item::CombinedEchoPhase(1))));
    assert_consistent(&g);
    // Resampled, the combined echoes are published as they are on the working grid.
    let mut config = PipelineConfig::default();
    config.pipeline.axial_resampling = true;
    let g = graph(&config, &f);
    assert!(g.node("resample_axial").unwrap().writes.contains(&item_key(&Item::CombinedEchoPhase(1))));
    assert!(g.node("scale_phase").unwrap().writes.contains(&item_key(&Item::ScaledMagnitude(1))));
    assert!(g.has_edge("resample_axial", "output_space"));
    assert_consistent(&g);

    // Axial resampling on: whether the run is resampled, and so whether its outputs go back to
    // the acquired grid, depends on the header. Unknown, both steps are drawn as conditional, the
    // echoes resampled after phase scaling and before anything reads them.
    let mut config = PipelineConfig::default();
    config.pipeline.axial_resampling = true;
    let plan = crate::plan::plan(&config, &facts());
    let unknown = pipeline_graph(&plan, &config, &facts(), None);
    assert_eq!(ids(&unknown), ["load", "scale_phase", "resample_axial", "magnitude", "mask", "unwrap", "bgremove",
                               "invert", "reference", "output_space"]);
    for id in ["resample_axial", "output_space"] {
        assert_eq!(unknown.node(id).unwrap().only_when.as_deref(), Some("the run is resampled"), "{id}");
    }
    assert_eq!(unknown.node("resample_axial").unwrap().enabled_by.as_deref(), Some("pipeline.axial_resampling"));
    assert!(unknown.has_edge("scale_phase", "resample_axial") && unknown.has_edge("load", "resample_axial"));
    for to in ["magnitude", "mask", "unwrap"] {
        assert!(unknown.has_edge("resample_axial", to) && !unknown.has_edge("scale_phase", to), "{to}");
    }
    assert_consistent(&unknown);
    // The grid known: a resampled run resamples, and returns its published outputs; one that is
    // not has neither step, and its phase comes from phase scaling.
    let resampled = pipeline_graph(&plan, &config, &facts(), Some(&grid(true)));
    for id in ["resample_axial", "output_space"] {
        assert!(resampled.node(id).unwrap().only_when.is_none(), "{id}");
    }
    for from in ["mask", "magnitude", "unwrap", "reference"] {
        assert!(resampled.has_edge(from, "output_space"), "{from}");
    }
    assert_consistent(&resampled);
    let acquired = pipeline_graph(&plan, &config, &facts(), Some(&grid(false)));
    assert!(acquired.node("output_space").is_none() && acquired.node("resample_axial").is_none());
    assert_eq!(ids(&acquired), ids(&graph(&PipelineConfig::default(), &facts())));
    assert!(acquired.has_edge("scale_phase", "mask") && acquired.has_edge("load", "mask"));
    assert_consistent(&acquired);
    // Kept on its working grid, a resampled run has no return step.
    config.pipeline.output_space = crate::OutputSpace::Working;
    let working = pipeline_graph(&plan, &config, &facts(), None);
    assert!(working.node("resample_axial").is_some() && working.node("output_space").is_none());

    // The MEDI toolbox's pipeline cannot resample: neither step, whatever the header.
    let mut config = PipelineConfig::default();
    let view = crate::schema::ViewFacts { num_echoes: Some(5), field_strength: Some(3.0), ..Default::default() };
    crate::pipelines::choose(&mut config, "medi-toolbox", &view).unwrap();
    let g = graph(&config, &facts());
    assert!(g.node("resample_axial").is_none() && g.node("output_space").is_none(), "{:?}", ids(&g));
    assert_consistent(&g);

    // The Tukey window: on the acquired grid, between phase scaling and the resampling, and the
    // magnitude it windows is the one the later steps read.
    let mut config = PipelineConfig::default();
    config.pipeline.tukey_window = 0.3;
    let g = graph(&config, &facts());
    assert_eq!(ids(&g)[..4], ["load", "scale_phase", "tukey_window", "magnitude"]);
    let tukey = g.node("tukey_window").unwrap();
    assert_eq!((tukey.enabled_by.as_deref(), tukey.settings.clone()), (Some("pipeline.tukey_window"), vec!["taper ratio 0.3".to_string()]));
    for (from, to) in [("scale_phase", "tukey_window"), ("load", "tukey_window"), ("tukey_window", "magnitude"),
                       ("tukey_window", "mask")] {
        assert!(g.has_edge(from, to), "{from} -> {to}: {:?}", g.edges);
    }
    assert!(!g.has_edge("load", "mask"));
    assert_consistent(&g);
    config.pipeline.axial_resampling = true;
    let g = graph(&config, &facts());
    assert_eq!(ids(&g)[..5], ["load", "scale_phase", "tukey_window", "resample_axial", "magnitude"]);
    assert!(g.has_edge("tukey_window", "resample_axial") && !g.has_edge("scale_phase", "resample_axial")
            && !g.has_edge("load", "resample_axial"));
    assert_consistent(&g);

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
    bgremove["Background-field removal<br/><i>V-SHARP</i><br/><small>SMV kernel radii from 12 to 2 mm in 1 mm steps</small>"]
    invert["Dipole inversion<br/><i>iLSQR</i>"]
    reference["Referencing<br/><i>mean</i>"]
    load -->|"Phase (as acquired)"| scale_phase
    load -->|"Magnitude"| magnitude
    scale_phase -->|"Phase"| mask
    load -->|"Magnitude"| mask
    scale_phase -->|"Phase"| unwrap
    load -->|"Magnitude"| unwrap
    mask -->|"Brain mask"| unwrap
    unwrap -->|"Total field, Field mask"| bgremove
    bgremove -->|"Local field, Local field mask"| invert
    invert -->|"Unreferenced susceptibility"| reference
    bgremove -->|"Local field mask"| reference
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
    let mut store = Recorder::new(store);
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
    // What the plan says each step reads and writes, for a run on this grid.
    let details = step_details(&plan, &plan.config, facts, Some(meta.source_geometry.is_some()));
    for (step, inputs, outputs) in &store.completed {
        let at = plan.steps.iter().position(|s| &s.name == step).unwrap();
        let planned = &details[at];
        assert!(outputs.iter().all(|o| planned.writes.contains(o)), "{step}: {outputs:?} vs {:?}", planned.writes);
        assert_eq!(inputs, &planned.reads, "{step}: what the run recorded reading is what the plan says it reads");
        let node = g.node(step).unwrap();
        let inputs: Vec<String> = inputs.iter().map(item_key).collect();
        assert_eq!(inputs, node.reads, "{step}");
        for item in outputs {
            // A magnitude phase scaling keeps as it was given is drawn from the load step.
            let copied = step == "scale_phase" && matches!(item, Item::Magnitude(_))
                && g.edges.iter().any(|e| e.from == "load" && e.items.contains(&item_key(item)));
            assert!(copied || node.writes.contains(&item_key(item)), "{step} wrote {item:?}; the graph says {:?}", node.writes);
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

    // A per-echo QSM, separated by DECOMPOSE from the echoes' maps.
    let (f, s) = phantom();
    let mut c = config();
    c.qsm.per_echo = true;
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::Decompose;
    c.separation.decompose.n_inner = 1;
    c.separation.decompose.lsqcurvefit.max_iterations = 2;
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
    s.insert_on_grid(Item::SuppliedSegmentation, dseg, crate::store::Geometry { dims: f.dims, voxel_size: (1.0, 1.0, 1.0), affine, orientation: None });
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
    assert_eq!(bg.reads, [Item::FieldPpm, Item::FieldMask]);
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
        if let Some(dir) = &out_dir {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join(format!("{name}.svg")), g.to_svg()).unwrap();
            std::fs::write(dir.join(format!("{name}.graph.json")), serde_json::to_string(&g).unwrap()).unwrap();
            std::fs::write(dir.join(format!("{name}.layout.json")), serde_json::to_string(&l).unwrap()).unwrap();
        }
        for (i, a) in l.nodes.iter().enumerate() {
            assert!(a.x >= 0.0 && a.y >= 0.0 && a.x + a.w <= l.width && a.y + a.h <= l.height, "{name}: {} outside", a.id);
            for b in &l.nodes[i + 1..] {
                let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{name}: {} overlaps {}", a.id, b.id);
            }
        }
        let mut on_lines: Vec<String> = Vec::new();
        for e in &l.edges {
            let from = l.nodes.iter().find(|n| n.id == e.from).unwrap();
            let to = l.nodes.iter().find(|n| n.id == e.to).unwrap();
            let (first, last) = (e.points[0], *e.points.last().unwrap());
            // Out of the source's bottom, or (skipping rows) its side.
            let bottom = first.1 == from.y + from.h && first.0 >= from.x && first.0 <= from.x + from.w;
            let side = (first.0 == from.x || first.0 == from.x + from.w) && first.1 > from.y && first.1 < from.y + from.h;
            assert!(bottom || side, "{name}: {}->{} starts at {first:?}", e.from, e.to);
            assert!(e.points.len() <= 5, "{name}: {}->{} bends {} times", e.from, e.to, e.points.len() - 2);
            assert_eq!(last.1, to.y, "{name}: {}->{}", e.from, e.to);
            assert!(last.0 >= to.x && last.0 <= to.x + to.w);
            for w in e.points.windows(2) {
                assert!(w[0].0 == w[1].0 || w[0].1 == w[1].1, "{name}: a diagonal in {}->{}", e.from, e.to);
            }
            let b = e.label_box;
            assert!(b.x >= 0.0 && b.x + b.w <= l.width, "{name}: label of {}->{} outside", e.from, e.to);
            // A label touches no box, no other label and no line.
            for n in &l.nodes {
                let r = layout::Rect { x: n.x, y: n.y, w: n.w, h: n.h };
                assert_eq!(b.overlap(&r), 0.0, "{name}: label of {}->{} on {}", e.from, e.to, n.id);
            }
            for f in &l.edges {
                if f.from != e.from || f.to != e.to {
                    assert_eq!(b.overlap(&f.label_box), 0.0, "{name}: labels of {}->{} and {}->{} overlap", e.from, e.to, f.from, f.to);
                }
                for w in f.points.windows(2) {
                    let seg = layout::Rect { x: w[0].0.min(w[1].0) - 0.5, y: w[0].1.min(w[1].1) - 0.5,
                                             w: (w[0].0 - w[1].0).abs() + 1.0, h: (w[0].1 - w[1].1).abs() + 1.0 };
                    if b.overlap(&seg) > 0.0 {
                        on_lines.push(format!("{}->{} on {}->{}", e.from, e.to, f.from, f.to));
                    }
                }
            }
        }
        eprintln!("{name}: labels on lines: {on_lines:?}");
        assert!(on_lines.is_empty(), "{name}: labels on lines: {on_lines:?}");
        // Nothing is cut short, and the key has the styles the graph uses.
        let svg_text = g.to_svg();
        assert!(!svg_text.contains('…'), "{name}");
        assert_eq!(l.key.iter().any(|k| k.style == layout::KeyStyle::Optional), g.nodes.iter().any(|n| n.enabled_by.is_some()), "{name}");
        assert_eq!(l.key.iter().any(|k| k.style == layout::KeyStyle::Conditional), g.nodes.iter().any(|n| n.only_when.is_some()), "{name}");
        if g.two_pass {
            assert_eq!(l.groups.len(), 2, "{name}");
        }
        let svg = g.to_svg();
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\"") && svg.ends_with("</svg>\n"));
        assert_eq!(svg.matches(" data-step=").count(), g.nodes.len(), "{name}");
        assert_eq!(svg.matches(" data-from=").count(), l.edges.len(), "{name}");
        assert_eq!(svg.matches("<g").count(), svg.matches("</g>").count(), "{name}");
        assert!(!svg.contains("NaN") && !svg.contains("inf"), "{name}");
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

/// The plain-text graph numbers each step in run order, with its algorithm, settings and when it
/// applies, and a line for each step it reads from naming what flows along that arrow.
#[test]
fn the_graph_as_text() {
    let text = graph(&PipelineConfig::default(), &facts()).to_text();
    assert!(text.starts_with(" 1 Load\n 2 Phase scaling\n     ← 1 Load: Phase (as acquired)\n"), "{text}");
    assert!(text.contains(concat!(
        " 6 Background-field removal · V-SHARP\n",
        "     SMV kernel radii from 12 to 2 mm in 1 mm steps\n",
        "     ← 5 Field map: Total field, Field mask\n")), "{text}");
    let mut resampling = PipelineConfig::default();
    resampling.pipeline.axial_resampling = true;
    let text = graph(&resampling, &facts()).to_text();
    assert!(text.contains(concat!(
        " 3 Resample to an axial grid\n",
        "     only when the run is resampled\n",
        "     ← 2 Phase scaling: Phase (acquired grid)\n",
        "     ← 1 Load: Magnitude (as acquired)\n")), "{text}");
    // Every arrow is a line.
    let g = graph(&PipelineConfig::default(), &facts());
    let from_lines = g.text_lines(None).iter().filter(|l| l.parts[0].0 == text::Part::From).count();
    let pairs: std::collections::BTreeSet<(&str, &str)> = g.edges.iter().map(|e| (e.from.as_str(), e.to.as_str())).collect();
    assert_eq!(from_lines, pairs.len());
}

/// A two-pass run's per-pass steps say their pass, once; a deep-learning step names its models.
#[test]
fn the_text_names_passes_and_models() {
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    config.inversion.algorithm = QsmAlgorithm::Tgv;
    let text = graph(&config, &facts()).to_text();
    assert!(text.contains(" 4 Brain mask  [main pass]\n"), "{text}");
    assert!(text.contains("Reliable-phase mask  [reliable pass]\n"), "{text}");
    assert!(text.contains("TGV-QSM (reliable pass) · TGV\n"), "{text}");
    assert!(text.contains("Two-pass combination\n"), "{text}");

    let mut config = PipelineConfig::default();
    config.inversion.algorithm = QsmAlgorithm::Xqsm;
    let g = graph(&config, &facts());
    let node = g.nodes.iter().find(|n| !n.models.is_empty()).expect("a step runs a model");
    let name = text::model_name(&node.models[0]);
    assert_ne!(name, node.models[0], "the registry names it");
    assert!(g.to_text().contains(&format!("model: {name}\n")), "{}", g.to_text());
}

/// A line too long for the width continues on the next, further indented, and keeps what each
/// piece of it says; nothing is lost.
#[test]
fn text_lines_wrap_to_a_width() {
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    let g = graph(&config, &facts());
    let words = |lines: &[text::Line]| lines.iter().flat_map(|l| l.text().split_whitespace().map(str::to_string).collect::<Vec<_>>()).collect::<Vec<_>>();
    let full = g.text_lines(None);
    for width in [30, 44, 60] {
        let narrow = g.text_lines(Some(width));
        assert!(narrow.len() > full.len(), "{width}");
        for l in &narrow {
            assert!(l.indent + l.text().chars().count() <= width, "{width}: {l:?}");
            assert!(!l.parts.is_empty());
        }
        assert_eq!(words(&narrow), words(&full), "{width}");
        // A continued arrow line is still the arrow's: its items stay items.
        let items = |lines: &[text::Line]| lines.iter().flat_map(|l| &l.parts).filter(|(p, _)| *p == text::Part::Items)
            .map(|(_, t)| t.split_whitespace().count()).sum::<usize>();
        assert_eq!(items(&narrow), items(&full), "{width}");
    }
    // A word longer than the line is cut, not dropped.
    let mut g = graph(&PipelineConfig::default(), &facts());
    g.nodes[0].settings.push("x".repeat(40));
    let lines = g.text_lines(Some(20));
    assert_eq!(lines.iter().filter(|l| l.step == g.nodes[0].id).map(|l| l.text().trim().to_string()).collect::<String>().matches('x').count(), 40);
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

/// A graph that uses no special style has no key; one that does lists only what it uses.
#[test]
fn the_key_lists_only_the_styles_drawn() {
    let mut config = PipelineConfig::default();
    config.pipeline.axial_resampling = true;
    let f = facts();
    let plain = pipeline_graph_on(&crate::plan::plan(&config, &f), &config, &f, Some(false));
    assert!(plain.layout().key.is_empty());
    assert!(!plain.to_svg().contains("class=\"g-key\""));
    let planned = graph(&config, &f);
    let styles: Vec<layout::KeyStyle> = planned.layout().key.iter().map(|k| k.style).collect();
    assert_eq!(styles, [layout::KeyStyle::Optional, layout::KeyStyle::Conditional, layout::KeyStyle::Collapsed]);
}

/// Two arrows from one box to another are drawn as one carrying both, and a label lists one item
/// per line, in full.
#[test]
fn arrows_between_the_same_boxes_are_one_and_labels_list_items_in_full() {
    let mut g = graph(&PipelineConfig::default(), &facts());
    let first = g.edges[0].clone();
    g.edges.push(Edge { items: vec!["Mask".into()], ..first.clone() });
    let l = g.layout();
    let drawn: Vec<_> = l.edges.iter().filter(|e| e.from == first.from && e.to == first.to).collect();
    assert_eq!(drawn.len(), 1);
    // An item's name may wrap to the arrow's room, and is never cut.
    assert_eq!(drawn[0].label.join(" "), "Phase (as acquired) Mask");
    assert_eq!(drawn[0].label.last().map(String::as_str), Some("Mask"));
}

/// A two-pass run's HEIDI fills the combined map: the combination reads the main pass's local
/// field (an arrow from its background removal) and is keyed on HEIDI's settings, and the passes
/// are inverted without it.
#[test]
fn two_pass_heidi_runs_on_the_combined_map() {
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    config.inversion.algorithm = QsmAlgorithm::Tkd;
    let off = graph(&config, &facts());
    assert!(!off.has_edge("bgremove", "twopass"));
    config.inversion.heidi.enabled = true;
    let g = graph(&config, &facts());
    assert!(g.has_edge("bgremove", "twopass"), "HEIDI reads the main pass's local field");
    assert_consistent(&g);
    let key = |c: &PipelineConfig| crate::cache::step_cache_key("twopass", None, &serde_json::json!({}), c);
    let mut changed = config.clone();
    changed.inversion.heidi.cone_threshold = 0.05;
    assert_ne!(key(&config), key(&changed), "HEIDI's settings key the combination");
    let pass = crate::stages::inversion::pass_config(&config, true);
    assert!(!pass.inversion.heidi.enabled, "the passes are inverted without HEIDI");
    assert!(crate::stages::inversion::pass_config(&config, false).inversion.heidi.enabled);
    assert!(crate::stages::inversion::params(&pass).get("heidi").is_none());
    assert!(crate::stages::inversion::params(&config).get("heidi").is_some());
}

/// The graph draws a run's own steps. A group is the dataset's, so a run's graph says nothing
/// about one from the setting alone, before or after the run; the fan-in's decision only notes the
/// box whose output it read (#496).
#[test]
fn a_group_setting_alone_draws_nothing() {
    let f = facts();
    let mut grouped = PipelineConfig::default();
    grouped.multi_orientation.group_by = "acq".into();
    let plan = crate::plan::plan(&grouped, &f);
    let planned = pipeline_graph(&plan, &grouped, &f, None);
    assert_eq!(planned, pipeline_graph(&crate::plan::plan(&PipelineConfig::default(), &f), &PipelineConfig::default(), &f, None));
    assert!(!planned.to_text().contains("COSMOS") && !planned.to_text().contains("rientation"), "{}", planned.to_text());

    let steps = plan.steps.iter().map(|s| crate::record::StepRecord {
        id: s.stage.name(), label: s.stage.label(), state: crate::record::StepState::Computed, decisions: vec![],
    }).collect();
    let mut record = crate::record::RunRecord { steps, ..Default::default() };
    assert!(!pipeline_graph_after(&plan, &grouped, &f, &record).to_text().contains("rientation"));
    record.set_orientation_group(Some("sub-1_ses-1"));
    let after = pipeline_graph_after(&plan, &grouped, &f, &record);
    assert!(!after.differs_from(&planned), "the same steps ran");
}

/// χ-sepnet's toolbox route is drawn as it runs: an R2′ the user asked for is no input to it, and
/// a measured one is.
#[test]
fn the_toolbox_route_is_drawn_reading_only_what_it_uses() {
    let mut config = PipelineConfig::default();
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = SeparationAlgorithm::ChiSepNet;
    let g = graph(&config, &facts());
    assert!(!ids(&g).contains(&"r2_r2prime"), "{:?}", ids(&g));
    assert!(g.has_edge("t2star_r2star", "chi_separation"), "{:?}", g.edges);

    config.pipeline.do_r2primemap = true;
    let g = graph(&config, &facts());
    assert!(ids(&g).contains(&"r2_r2prime"), "{:?}", ids(&g));
    assert!(!g.has_edge("r2_r2prime", "chi_separation"), "{:?}", g.edges);
    config.pipeline.do_r2primemap = false;

    let mut f = facts();
    f.mese_echo_times = Some(vec![0.01, 0.02, 0.03, 0.04]);
    let g = graph(&config, &f);
    assert!(g.has_edge("r2_r2prime", "chi_separation"), "{:?}", g.edges);
}

/// An oblique scan on χ-sepnet's toolbox route stays on the acquired grid (#497): no resampling
/// step and no return, the B0 direction comes from the header, and the route itself maps its inputs
/// into the networks' frame (checked in QSM.rs's `tests/chisep_orientation.rs`). Wiring only.
#[test]
fn an_oblique_scan_on_the_chisepnet_toolbox_route_stays_on_the_acquired_grid() {
    let mut config = PipelineConfig::default();
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = SeparationAlgorithm::ChiSepNet;
    assert_eq!(config.separation.chi_sepnet.route, crate::ChiSepNetRoute::Toolbox);
    let f = facts();
    let g = graph(&config, &f);
    assert!(g.node("resample_axial").is_none() && g.node("output_space").is_none(), "{:?}", ids(&g));
    assert!(!crate::scan::can_resample(&f, &config));
    assert_consistent(&g);

    // A 20° oblique header: kept on its grid, B0 rotated into it.
    let (cos, sin) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, cos, -sin, 0.0, 0.0, sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0];
    let meta = crate::scan::resolve_geometry(f.dims, (1.0, 1.0, 1.0), affine, &f, &config);
    assert!(meta.source_geometry.is_none(), "not resampled");
    assert_eq!(meta.dims, f.dims);
    assert!((meta.b0_direction.2 - 1.0).abs() > 1e-3, "B0 from the header: {:?}", meta.b0_direction);
    let g = pipeline_graph(&crate::plan::plan(&config, &f), &config, &f, Some(&meta));
    assert!(g.node("resample_axial").is_none() && g.node("output_space").is_none(), "{:?}", ids(&g));
}

/// A run that windows and may be resampled: phase scaling, the Tukey window and the resampling
/// in data order, the resampling and the return conditional on the header, and the magnitude
/// windowed with the phase.
#[test]
fn a_windowed_resampled_graph_in_mermaid() {
    let mut config = PipelineConfig::default();
    config.pipeline.tukey_window = 0.25;
    config.pipeline.axial_resampling = true;
    let text = graph(&config, &facts()).to_mermaid();
    println!("{text}");
    for line in [
        "    tukey_window[\"Tukey window<br/><small>taper ratio 0.25</small>\"]\n",
        "    resample_axial[\"Resample to an axial grid<br/><small>only when the run is resampled</small>\"]\n",
        "    load -->|\"Phase (as acquired)\"| scale_phase\n",
        "    scale_phase -->|\"Phase (acquired grid)\"| tukey_window\n",
        "    load -->|\"Magnitude (as acquired)\"| tukey_window\n",
        "    tukey_window -->|\"Windowed phase (acquired grid), Windowed magnitude (acquired grid)\"| resample_axial\n",
        "    resample_axial -->|\"Magnitude\"| magnitude\n",
        "    resample_axial -->|\"Phase, Magnitude\"| mask\n",
        "    class tukey_window,resample_axial optional\n",
        "    class resample_axial,output_space conditional\n",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
}
