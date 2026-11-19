//! The pipeline a run will take, as a graph: one box per step (its label, the algorithm it runs
//! and that algorithm's settings), and an arrow wherever one step reads what another wrote.
//!
//! [`pipeline_graph`] reads only the [`Plan`], the configuration and the scan's facts, so a front
//! end can draw it before anything runs, and since the plan is what `run` takes, the picture
//! cannot disagree with what runs. qsmbly draws it from [`PipelineGraph`]'s JSON; `qsmxt` writes
//! it as Mermaid ([`PipelineGraph::to_mermaid`]) and SVG ([`PipelineGraph::to_svg`]), and shows it
//! in a terminal as text ([`text`]).
//!
//! The arrows are data flow: an item a step reads, from the step that last wrote it before it.
//! The load step stands for the scan: the acquired echoes and anything supplied with the scan (a
//! brain mask, a segmentation, a MESE) enter the pipeline there. Which steps a change makes stale
//! follows the same reads: a step a run recorded reading an item is stale once the step that wrote
//! it is computed again ([`crate::graph::readers`]). Every arrow here is also an edge of the fixed
//! table a step recorded without what it read falls back to ([`crate::graph::downstream_of`]).

use serde::{Deserialize, Serialize};

use crate::bridge::skips_bgremove;
use crate::config::PipelineConfig;
use crate::items::ItemKind;
use crate::plan::{Pass, Plan, Stage};
use crate::record::{Choice, DatasetRecord, Decision, DecisionCount, RunRecord, StepState, Topic};
use crate::scan::{RunMetadata, ScanFacts};
use crate::store::Item;

/// Which reconstruction of a two-pass run a step belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Branch {
    /// The reconstruction on the brain mask.
    Main,
    /// The reconstruction on the reliable-phase mask.
    Reliable,
}

/// One step of the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// The step's name, as the observer and the cache name it (`mask`, `invert-reliable`).
    pub id: String,
    /// What a front end calls it ("Background-field removal").
    pub label: String,
    /// The algorithm the step runs, as the observer reports it (`vsharp`, `rts`), when it has one.
    pub algorithm: Option<String>,
    /// The algorithm's name as the methods text gives it ("V-SHARP", "RTS").
    pub algorithm_label: Option<String>,
    /// The algorithm's settings worth showing beside it, in the methods text's words: what
    /// differs from the defaults, and what names the variant that ran (V-SHARP's radii).
    pub settings: Vec<String>,
    /// For a two-pass run's per-pass steps, which pass; none for a step both passes share.
    pub branch: Option<Branch>,
    /// The setting that asked for this step (`pipeline.do_swi`, `masking.two_pass`), by its path
    /// in the configuration; none for a step every run of the enabled outputs takes.
    pub enabled_by: Option<String>,
    /// When the step does anything at all, for a step whose need depends on the scan's header,
    /// which a graph drawn from the scan's facts alone does not know.
    pub only_when: Option<String>,
    /// The deep-learning models it runs, by QSM.rs model id.
    pub models: Vec<String>,
    /// The items it reads, by [`item_key`].
    pub reads: Vec<String>,
    /// The items it writes, by [`item_key`].
    pub writes: Vec<String>,
    /// What of those the run publishes ([`Item::published_in`]), by [`item_key`]: the graph's
    /// leaves. The return to the acquired grid only moves them, so its box has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes: Vec<String>,
    /// What the step did in the run, on the graph after a run ([`pipeline_graph_after`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<NodeRun>,
    /// What a dataset's runs did in the step, on a dataset's graph ([`pipeline_graph_dataset`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<NodeDataset>,
}

/// A published output as a graph's leaf says it: "→ Combined magnitude (part-mag_T2starw)".
pub fn output_text(key: &str) -> String {
    match Item::from_key(key).map(|i| i.meta()) {
        Some(m) => match m.bids_name {
            Some(b) => format!("→ {} ({b})", m.label),
            None => format!("→ {}", m.label),
        },
        None => format!("→ {key}"),
    }
}

impl Node {
    /// Everything the box says, in full, a line each: the step, its algorithm, settings, models,
    /// and what the run or the dataset's runs did.
    pub fn tooltip(&self) -> String {
        let mut lines: Vec<String> = vec![self.label.clone()];
        lines.extend(self.algorithm_label.clone());
        lines.extend(self.settings.iter().cloned());
        lines.extend(self.only_when.as_ref().map(|w| format!("Only when {w}")));
        if !self.models.is_empty() {
            lines.push(format!("Models: {}", self.models.join(", ")));
        }
        lines.extend(self.publishes.iter().map(|k| output_text(k)));
        if let Some(run) = &self.run {
            lines.push(layout::run_state_text(run.state).to_string());
            lines.extend(run.decisions.iter().map(Decision::note));
        }
        if let Some(d) = &self.dataset {
            lines.extend(d.alternatives.iter().map(|a| d.alternative_text(a)));
            lines.extend(d.summary());
            lines.extend(d.decisions.iter().map(|c| d.decision_note(c)));
        }
        lines.join("\n")
    }
}

/// What a step did in a run ([`crate::record::StepRecord`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRun {
    pub state: StepState,
    /// What it decided, in the order it decided it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
}

/// What a dataset's runs did in a step ([`crate::record::DatasetStep`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDataset {
    /// The dataset's runs.
    pub of: usize,
    /// The runs whose plan had the step.
    pub runs: usize,
    pub computed: usize,
    pub cached: usize,
    pub skipped: usize,
    /// What the runs decided in it, each with how many runs decided it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<DecisionCount>,
    /// Where the runs set the step up differently (a mask supplied for some, computed for the
    /// others): each way, with how many runs took it. Empty when they all did the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<Alternative>,
}

/// One way some of a dataset's runs set a step up ([`NodeDataset::alternatives`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alternative {
    pub runs: usize,
    /// The step's settings in that setup, on one line ("phase quality (Otsu threshold, …)").
    pub text: String,
}

/// "1 run", "all 4 runs", "3 of 4 runs".
fn runs_text(k: usize, n: usize) -> String {
    match (k, n) {
        (1, 1) => "1 run".to_string(),
        _ if k == n => format!("all {n} runs"),
        _ => format!("{k} of {n} runs"),
    }
}

impl NodeDataset {
    /// What the box says about the runs, a line each: how many runs took the step, when not all
    /// did, and how many skipped it. Reusing the cache is not a difference, so it is not counted.
    pub fn summary(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.runs < self.of {
            out.push(format!("In {}", runs_text(self.runs, self.of)));
        }
        if self.skipped > 0 {
            out.push(format!("Skipped in {}", runs_text(self.skipped, self.runs)));
        }
        out
    }

    /// An alternative, after how many of the step's runs took it: "2 of 3: supplied (premask)".
    pub fn alternative_text(&self, a: &Alternative) -> String {
        format!("{} of {}: {}", a.runs, self.runs, a.text)
    }

    /// A decision, after how many of the step's runs made it (first, so a box cut to its width
    /// keeps it): "2 of 3 runs: The brain mask was the one supplied with the scan."
    pub fn decision_note(&self, c: &DecisionCount) -> String {
        let runs = runs_text(c.runs, self.runs);
        let mut chars = runs.chars();
        let runs: String = chars.next().map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default();
        format!("{runs}: {}", c.reason)
    }
}

/// One step reading what another wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// What flows along it, by [`item_key`].
    pub items: Vec<String>,
}

/// An item of the graph, and what it is ([`Item::meta`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemInfo {
    /// [`item_key`].
    pub key: String,
    /// The item as the observer reports it (its serialised [`Item`]).
    pub item: serde_json::Value,
    pub label: String,
    pub units: Option<String>,
    pub kind: ItemKind,
    pub bids_name: Option<String>,
    pub published: bool,
    pub echo: Option<usize>,
    pub coil: Option<usize>,
}

/// A planned run as a graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineGraph {
    /// Every step that does something with the data, in the order the run takes them.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Every item a step reads or writes, in the order they first appear.
    pub items: Vec<ItemInfo>,
    /// Two-pass artefact reduction runs: the steps with a [`Branch`] are its two
    /// reconstructions, joining at the two-pass combination.
    pub two_pass: bool,
}

/// A stable name for an item, used in the graph's JSON: its variant and arguments, as the
/// in-memory store also names it (`Phase(2)`, `LocalField(Reliable)`).
pub fn item_key(item: &Item) -> String {
    format!("{item:?}")
}

/// What the graph is drawn from.
struct Ctx<'a> {
    plan: &'a Plan,
    config: &'a PipelineConfig,
    facts: &'a ScanFacts,
    /// Whether the run is resampled, when that is known.
    resampled: Option<bool>,
}

impl Ctx<'_> {
    /// Where the phase steps read and write each echo: a run that may be resampled is drawn as
    /// one that is, the resampling step marked as conditional.
    fn chain(&self) -> crate::stages::phase::Chain {
        let f = self.facts;
        crate::stages::phase::Chain {
            makes_magnitude: f.n_coils > 1 || f.real_imaginary,
            has_magnitude: f.has_magnitude || f.n_coils > 1 || f.real_imaginary,
            windowed: self.plan.has(Stage::TukeyWindow),
            resampled: self.plan.has(Stage::ResampleAxial) && self.resampled != Some(false),
            coils: f.n_coils > 1,
        }
    }

    /// What a phase step writes for echo `e` beside its phase and magnitude: when it writes the
    /// working echoes of a run of uncombined coils, the combined echoes it publishes.
    fn combined_echo(&self, e: usize, working: bool) -> Vec<Item> {
        if working && self.facts.n_coils > 1 { vec![Item::CombinedEchoPhase(e), Item::CombinedEchoMagnitude(e)] } else { vec![] }
    }

    /// The items `stage` writes as copies of the scan's (`(source, written)`): the graph draws
    /// them from where they came, and not as the step's. Phase scaling with nothing after it keeps
    /// the magnitude given as it is.
    fn copies(&self, stage: Stage) -> Vec<(Item, Item)> {
        match stage {
            Stage::ScalePhase if self.chain().copies_magnitude() =>
                (1..=self.n()).map(|e| (Item::AcquiredMagnitude(e), Item::Magnitude(e))).collect(),
            _ => Vec::new(),
        }
    }

    fn n(&self) -> usize {
        self.facts.n_echoes
    }

    fn echoes(&self, item: fn(usize) -> Item) -> Vec<Item> {
        (1..=self.n()).map(item).collect()
    }

    /// Every echo's magnitude on the working grid, when the scan has magnitude.
    fn magnitudes(&self) -> Vec<Item> {
        if self.facts.has_magnitude { self.echoes(Item::Magnitude) } else { Vec::new() }
    }

    /// The magnitudes of `echoes` as the steps after the inhomogeneity correction read them:
    /// corrected where the correction corrects them, as they are otherwise.
    fn corrected_magnitudes(&self, echoes: &[usize]) -> Vec<Item> {
        if !self.facts.has_magnitude {
            return Vec::new();
        }
        let corrected = &self.plan.homogeneity.echoes;
        echoes.iter().map(|&e| if corrected.contains(&e) { Item::HomogeneousMagnitude(e) } else { Item::Magnitude(e) }).collect()
    }

    /// The magnitudes a mask of `sections` reads: with the correction on, the echoes it reads
    /// ([`crate::stages::magnitude::mask_echoes`]), corrected; every echo's otherwise.
    fn mask_magnitudes(&self, sections: &[crate::masking::MaskSection]) -> Vec<Item> {
        let m = &self.config.masking;
        if m.inhomogeneity_correction {
            self.corrected_magnitudes(&crate::stages::magnitude::mask_echoes(sections, &m.refinements, self.n()))
        } else {
            self.magnitudes()
        }
    }

    fn mask_item(&self) -> Item {
        if self.plan.motion_corrected { Item::CoverageMask } else { Item::BrainMask }
    }

    fn pass_mask(&self, pass: Pass) -> Item {
        match pass {
            Pass::Main if crate::run::field_mask_written(self.plan, self.config, self.n()) => Item::FieldMask,
            Pass::Main => self.mask_item(),
            Pass::Reliable => Item::ReliableMask,
        }
    }

    fn support(&self, pass: Pass) -> Item {
        if skips_bgremove(self.config) { self.pass_mask(pass) } else { Item::LocalFieldMask(pass) }
    }

    fn echo_phases(&self) -> Vec<Item> {
        if self.plan.motion_corrected { self.echoes(Item::CorrectedPhase) } else { self.echoes(Item::Phase) }
    }

    fn echo_magnitudes(&self) -> Vec<Item> {
        match (self.facts.has_magnitude, self.plan.motion_corrected) {
            (false, _) => Vec::new(),
            (true, true) => self.echoes(Item::CorrectedMagnitude),
            (true, false) => self.echoes(Item::Magnitude),
        }
    }

    fn supplied_mask(&self) -> bool {
        self.config.masking.custom_mask_tool.is_some() && self.facts.custom_mask
    }

    fn supplied_dseg(&self) -> bool {
        self.config.segmentation.custom_dseg_tool.is_some() && self.facts.custom_dseg
    }

    /// The segmentation a region is measured on: the run's own, or the one supplied.
    fn segmentation(&self) -> Option<Item> {
        if self.plan.has(Stage::Segmentation) {
            Some(Item::Segmentation)
        } else if self.facts.custom_dseg && self.config.segmentation.custom_dseg_tool.is_some() {
            Some(Item::SuppliedSegmentation)
        } else {
            None
        }
    }

    fn mese(&self) -> Vec<Item> {
        let k = self.facts.mese_echo_times.as_ref().map_or(0, Vec::len);
        (1..=k).map(Item::MeseMagnitude).collect()
    }

    /// Whether the run reconstructs a QSM per echo ([`crate::bridge::per_echo`]).
    fn per_echo(&self) -> bool {
        crate::bridge::per_echo(self.config, self.n())
    }
}

/// The step's pass, for the steps two-pass runs once per pass.
fn branch(stage: Stage, two_pass: bool) -> Option<Branch> {
    let pass = match stage {
        Stage::Mask(p) | Stage::BackgroundRemoval(p) | Stage::Inversion(p) | Stage::Tgv(p) | Stage::Qsmart(p) => p,
        _ => return None,
    };
    two_pass.then_some(match pass {
        Pass::Main => Branch::Main,
        Pass::Reliable => Branch::Reliable,
    })
}

/// Whether the graph leaves out `stage`, a step of `plan` that does nothing with the data:
/// motion correction when it is off (the step only forgets an earlier run's output); the
/// resampling to an axial grid when `resampled` is known (from the run's grid) to be false; and
/// the return to the acquired grid of a run that keeps its working grid, cannot be resampled (its
/// plan has no resampling step) or, when `resampled` is known, is not resampled.
pub(crate) fn left_out(plan: &Plan, config: &PipelineConfig, stage: Stage, resampled: Option<bool>) -> bool {
    match stage {
        Stage::Motion => !plan.motion_corrected,
        Stage::ResampleAxial => resampled == Some(false),
        Stage::OutputSpace => config.pipeline.output_space != crate::OutputSpace::Acquired
            || !plan.has(Stage::ResampleAxial) || resampled == Some(false),
        _ => false,
    }
}

/// The graph of a run of `plan` (made by [`crate::plan::plan`] from `config` and `facts`).
///
/// `grid` is the run's grid, when it is known ([`crate::scan::resolve_geometry`] on the first
/// echo's header): it decides whether the run is resampled, and so whether its echoes are
/// resampled to an axial grid and its outputs returned to the acquired grid. Without it, those
/// steps are drawn with [`Node::only_when`] set, where the configuration can resample at all.
///
/// A step the plan lists that does nothing with the data is left out ([`left_out`]): motion
/// correction when it is off (the step only forgets an earlier run's output), and the resampling
/// and the return to the acquired grid of a run that is not resampled (the return also of one
/// that keeps its working grid).
pub fn pipeline_graph(plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, grid: Option<&RunMetadata>) -> PipelineGraph {
    pipeline_graph_on(plan, config, facts, grid.map(|g| g.source_geometry.is_some()))
}

/// The graph after a run of `plan` that did what `record` says ([`crate::run::run`]'s record):
/// each box carries what its step did ([`Node::run`]: computed, reused or skipped, and what it
/// decided), and the boxes are drawn as the run took them where it decided something the plan
/// could not know: a supplied mask that could not be used is drawn computed from the echoes, a
/// segmentation, R2 or R2′ map read from what was supplied only where one was.
///
/// [`PipelineGraph::differs_from`] says whether it differs from the planned graph.
pub fn pipeline_graph_after(plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, record: &RunRecord) -> PipelineGraph {
    let config = &config.normalised_as_planned(plan);
    let ran = as_decided(facts, record.decisions().map(|(_, d)| d));
    let mut graph = pipeline_graph_on(plan, config, &ran, record.resampled);
    for node in &mut graph.nodes {
        // A mask computed in place of one the plan expected to read runs its recipe's models.
        if node.id == Stage::Mask(Pass::Main).name() && facts.custom_mask && !ran.custom_mask {
            node.models = crate::stages::mask::model_ids(&config.masking.sections, config)
                .into_iter().map(str::to_string).collect();
        }
        node.run = record.step(&node.id).map(|s| NodeRun { state: s.state, decisions: s.decisions.clone() });
    }
    graph
}

/// `facts` as a run's `decisions` left them, where it decided something the plan could not know:
/// whether the mask, segmentation, R2 or R2′ map supplied with the scan was the one used. The
/// first decision on each counts. What a step reads ([`crate::plan::Step::reads`]) under these
/// facts is what it read in that run: the graph after the run draws it, and the run records it.
pub(crate) fn as_decided<'d>(facts: &ScanFacts, decisions: impl Iterator<Item = &'d Decision>) -> ScanFacts {
    let mut ran = facts.clone();
    let mut seen: Vec<Topic> = Vec::new();
    for d in decisions {
        if seen.contains(&d.topic) {
            continue;
        }
        let supplied = d.choice == Choice::Supplied;
        match d.topic {
            Topic::MaskSource => ran.custom_mask = supplied,
            Topic::SegmentationSource => ran.custom_dseg = supplied,
            Topic::R2Source => ran.custom_r2 = supplied,
            Topic::R2primeSource => ran.custom_r2prime = supplied,
            _ => continue,
        }
        seen.push(d.topic);
    }
    ran
}

/// A dataset's graph as its runs took it: the graphs after its runs ([`pipeline_graph_after`]),
/// merged ([`PipelineGraph::merged`]), each box carrying what the runs did in it
/// ([`Node::dataset`]), from their merged record ([`crate::record::merge`]). `None` for no runs.
///
/// Where the runs set a step up differently, its box lists each way with how many runs took it
/// ([`NodeDataset::alternatives`]) in place of its settings, and leaves out the decisions about
/// where its input came from, which those lines already say.
pub fn pipeline_graph_dataset(after: &[PipelineGraph], record: &DatasetRecord) -> Option<PipelineGraph> {
    let bare: Vec<PipelineGraph> = after.iter().map(PipelineGraph::without_runs).collect();
    let mut graph = PipelineGraph::merged(&bare)?;
    for node in &mut graph.nodes {
        // Each way the runs set the step up, in the order first seen.
        let mut ways: Vec<(Vec<String>, usize)> = Vec::new();
        for g in &bare {
            let Some(n) = g.node(&node.id) else { continue };
            match ways.iter_mut().find(|(s, _)| *s == n.settings) {
                Some((_, k)) => *k += 1,
                None => ways.push((n.settings.clone(), 1)),
            }
        }
        let alternatives: Vec<Alternative> = if ways.len() > 1 {
            ways.into_iter().map(|(settings, runs)| Alternative { runs, text: setup_text(&settings, node) }).collect()
        } else {
            Vec::new()
        };
        if !alternatives.is_empty() {
            node.settings.clear();
        }
        node.dataset = record.step(&node.id).map(|s| NodeDataset {
            of: record.runs, runs: s.runs, computed: s.computed, cached: s.cached, skipped: s.skipped,
            decisions: s.decisions.iter()
                .filter(|d| alternatives.is_empty() || !matches!(d.topic,
                    Topic::MaskSource | Topic::SegmentationSource | Topic::R2Source | Topic::R2primeSource | Topic::QsmSource))
                .cloned().collect(),
            alternatives,
        });
    }
    Some(graph)
}

/// A step's settings on one line: "phase quality: Otsu threshold, erosion" as "phase quality
/// (Otsu threshold, erosion)", several joined by "; ". With none, its algorithm, else "computed".
fn setup_text(settings: &[String], node: &Node) -> String {
    if settings.is_empty() {
        return node.algorithm_label.clone().unwrap_or_else(|| "computed".into());
    }
    settings.iter().map(|s| match s.split_once(": ") {
        Some((what, how)) => format!("{what} ({how})"),
        None => s.clone(),
    }).collect::<Vec<_>>().join("; ")
}

/// [`pipeline_graph`], knowing only whether the run is `resampled` (when it is known) rather than
/// its whole grid: the planned graph a run's graph after it ([`pipeline_graph_after`]) is compared
/// with.
pub fn pipeline_graph_on(plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, resampled: Option<bool>) -> PipelineGraph {
    let config = &config.normalised_as_planned(plan);
    let ctx = Ctx { plan, config, facts, resampled };
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    // Every item written so far, with the step that last wrote it.
    let mut writer: Vec<(Item, String)> = Vec::new();
    for step in &plan.steps {
        let stage = step.stage;
        if left_out(plan, config, stage, resampled) {
            continue;
        }
        let only_when = (matches!(stage, Stage::ResampleAxial | Stage::OutputSpace) && resampled.is_none())
            .then(|| "the run is resampled".to_string());
        let written: Vec<Item> = writer.iter().map(|(i, _)| *i).collect();
        let (reads, mut writes) = ctx.io(stage, &written);
        let id = stage.name();
        // What the step only copies is drawn from where it came, not through the step.
        for (from, to) in ctx.copies(stage) {
            writes.retain(|i| *i != to);
            if let Some(source) = writer.iter().find(|(i, _)| *i == from).map(|(_, s)| s.clone()) {
                match writer.iter_mut().find(|(i, _)| *i == to) {
                    Some(slot) => slot.1 = source,
                    None => writer.push((to, source)),
                }
            }
        }
        let mut incoming: Vec<Edge> = Vec::new();
        for item in &reads {
            let Some((_, from)) = writer.iter().find(|(i, _)| i == item) else { continue };
            match incoming.iter_mut().find(|e| &e.from == from) {
                Some(e) => e.items.push(item_key(item)),
                None => incoming.push(Edge { from: from.clone(), to: id.clone(), items: vec![item_key(item)] }),
            }
        }
        edges.extend(incoming);
        for item in reads.iter().chain(&writes) {
            if !items.contains(item) {
                items.push(*item);
            }
        }
        for item in &writes {
            match writer.iter_mut().find(|(i, _)| i == item) {
                Some(slot) => slot.1 = id.clone(),
                None => writer.push((*item, id.clone())),
            }
        }
        let (algorithm, algorithm_label, settings) = ctx.algorithm(stage);
        nodes.push(Node {
            label: stage.label(),
            algorithm, algorithm_label, settings,
            branch: branch(stage, plan.two_pass),
            enabled_by: ctx.enabled_by(stage),
            only_when,
            models: step.models.iter().map(|m| m.to_string()).collect(),
            reads: reads.iter().map(item_key).collect(),
            publishes: if stage == Stage::OutputSpace { Vec::new() } else {
                writes.iter().filter(|i| i.published_in(config)).map(item_key).collect()
            },
            writes: writes.iter().map(item_key).collect(),
            id,
            run: None,
            dataset: None,
        });
    }
    let items = items.iter().map(|item| {
        let m = item.meta();
        ItemInfo {
            key: item_key(item), item: serde_json::to_value(item).unwrap_or_default(),
            label: m.label, units: m.units.map(str::to_string), kind: m.kind, bids_name: m.bids_name,
            published: m.published, echo: m.echo, coil: m.coil,
        }
    }).collect();
    PipelineGraph { nodes, edges, items, two_pass: plan.two_pass }
}

/// What one of a plan's steps runs, reads and writes: what [`crate::plan::Step`] carries
/// beside its stage.
pub(crate) struct StepDetails {
    pub algorithm: Option<String>,
    pub algorithm_label: Option<String>,
    pub reads: Vec<Item>,
    pub writes: Vec<Item>,
}

/// [`StepDetails`] for every step of `plan`, in its order, for a run that is `resampled` (when
/// that is known; a plan's are drawn as for a run that is, where it can be). A step that does
/// nothing with the data on this scan (motion correction when it is off, the resampling of a run
/// that is not resampled) reads and writes nothing.
pub(crate) fn step_details(plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, resampled: Option<bool>) -> Vec<StepDetails> {
    let ctx = Ctx { plan, config, facts, resampled };
    let mut written: Vec<Item> = Vec::new();
    plan.steps.iter().map(|step| {
        let (algorithm, algorithm_label, _) = ctx.algorithm(step.stage);
        let (reads, writes) = if step.stage == Stage::Motion && !plan.motion_corrected {
            (Vec::new(), Vec::new())
        } else {
            ctx.io(step.stage, &written)
        };
        written.extend(writes.iter().copied().filter(|i| !written.contains(i)).collect::<Vec<_>>());
        StepDetails { algorithm, algorithm_label, reads, writes }
    }).collect()
}

/// `text` inside a Mermaid quoted label: quotes and angle brackets as entities.
fn mermaid_text(text: &str) -> String {
    text.replace('&', "#amp;").replace('"', "#quot;").replace('<', "#lt;").replace('>', "#gt;")
}

/// A step name as a Mermaid node id (no `-`, which Mermaid reads as part of an arrow).
fn mermaid_id(id: &str) -> String {
    id.replace('-', "_")
}

impl PipelineGraph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn item(&self, key: &str) -> Option<&ItemInfo> {
        self.items.iter().find(|i| i.key == key)
    }

    /// The graph without what a run or a dataset's runs did in each step: what was drawn.
    pub fn without_runs(&self) -> PipelineGraph {
        let mut g = self.clone();
        for n in &mut g.nodes {
            n.run = None;
            n.dataset = None;
        }
        g
    }

    /// Whether this graph, of a run ([`pipeline_graph_after`]), differs from `planned`, the graph
    /// of its plan: a box or an arrow the run drew differently (what a step read, wrote, ran or
    /// how it is labelled), or a box whose step did nothing. A step reused from the cache, and what
    /// a step noted without changing what it did (the box it reconstructed in), are not differences.
    pub fn differs_from(&self, planned: &PipelineGraph) -> bool {
        let bare = |n: &Node| Node { run: None, dataset: None, ..n.clone() };
        self.nodes.len() != planned.nodes.len()
            || self.nodes.iter().zip(&planned.nodes).any(|(a, b)| bare(a) != bare(b))
            || self.edges != planned.edges
            || self.nodes.iter().any(|n| n.run.as_ref().is_some_and(|r| r.state == StepState::Skipped))
    }

    /// A dataset's runs' graphs as one: the graph they share, or where they differ, every box and
    /// arrow any of them has, each once. A box only some runs have is placed after the box it
    /// follows in the first graph that has it, and drawn as conditional ([`Node::only_when`]). A
    /// box the graphs draw differently is the first graph's, with every graph's settings and models.
    /// How many runs took each box is the merged record's ([`crate::record::merge`]). `None` for
    /// no graphs.
    pub fn merged(graphs: &[PipelineGraph]) -> Option<PipelineGraph> {
        let first = graphs.first()?;
        if graphs.iter().all(|g| g == first) {
            return Some(first.clone());
        }
        let mut out = PipelineGraph { nodes: Vec::new(), edges: Vec::new(), items: Vec::new(), two_pass: false };
        let mut seen: Vec<(String, usize)> = Vec::new();
        for g in graphs {
            let mut at = 0;
            for n in &g.nodes {
                match out.nodes.iter().position(|m| m.id == n.id) {
                    Some(i) => {
                        seen.iter_mut().find(|(id, _)| *id == n.id).expect("counted").1 += 1;
                        // A step the runs take differently (a mask supplied for some runs and
                        // computed for others) lists every way it was set up and the models any run ran.
                        let m = &mut out.nodes[i];
                        for (mine, theirs) in [(&mut m.settings, &n.settings), (&mut m.models, &n.models)] {
                            for x in theirs {
                                if !mine.contains(x) {
                                    mine.push(x.clone());
                                }
                            }
                        }
                        at = i + 1;
                    }
                    None => {
                        out.nodes.insert(at, n.clone());
                        seen.push((n.id.clone(), 1));
                        at += 1;
                    }
                }
            }
            for e in &g.edges {
                if !out.edges.contains(e) {
                    out.edges.push(e.clone());
                }
            }
            for i in &g.items {
                if !out.items.iter().any(|j| j.key == i.key) {
                    out.items.push(i.clone());
                }
            }
            out.two_pass |= g.two_pass;
        }
        for n in &mut out.nodes {
            let runs = seen.iter().find(|(id, _)| *id == n.id).map_or(0, |(_, k)| *k);
            if runs < graphs.len() && n.only_when.is_none() {
                n.only_when = Some("the run's scan needs it".into());
            }
        }
        Some(out)
    }

    /// Whether `from` feeds `to` directly.
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        self.edges.iter().any(|e| e.from == from && e.to == to)
    }

    /// The graph as a Mermaid flowchart: one box per step (label, algorithm, settings), each
    /// arrow labelled with what flows along it (an echo's items once, for all echoes). A two-pass
    /// run's passes are subgraphs; a step a setting asked for is dashed, and one that may do
    /// nothing is dotted.
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("flowchart TD\n");
        let node_line = |n: &Node| {
            let mut text = mermaid_text(&n.label);
            if let Some(alg) = &n.algorithm_label {
                text.push_str(&format!("<br/><i>{}</i>", mermaid_text(alg)));
            }
            for s in &n.settings {
                text.push_str(&format!("<br/><small>{}</small>", mermaid_text(s)));
            }
            if let Some(when) = &n.only_when {
                text.push_str(&format!("<br/><small>only when {}</small>", mermaid_text(when)));
            }
            if let Some(run) = &n.run {
                let state = match run.state {
                    StepState::Computed => "computed",
                    StepState::Cached => "reused from the cache",
                    StepState::Skipped => "skipped",
                };
                text.push_str(&format!("<br/><small><b>{state}</b></small>"));
                for d in &run.decisions {
                    text.push_str(&format!("<br/><small>{}</small>", mermaid_text(&d.note())));
                }
            }
            if let Some(d) = &n.dataset {
                for a in &d.alternatives {
                    text.push_str(&format!("<br/><small>{}</small>", mermaid_text(&d.alternative_text(a))));
                }
                for line in d.summary() {
                    text.push_str(&format!("<br/><small><b>{}</b></small>", mermaid_text(&line)));
                }
                for c in &d.decisions {
                    text.push_str(&format!("<br/><small>{}</small>", mermaid_text(&d.decision_note(c))));
                }
            }
            format!("{}[\"{text}\"]", mermaid_id(&n.id))
        };
        for n in self.nodes.iter().filter(|n| n.branch.is_none()) {
            out.push_str(&format!("    {}\n", node_line(n)));
        }
        for (branch, id, title) in [(Branch::Main, "main_pass", "Main pass"), (Branch::Reliable, "reliable_pass", "Reliable pass")] {
            let members: Vec<&Node> = self.nodes.iter().filter(|n| n.branch == Some(branch)).collect();
            if members.is_empty() {
                continue;
            }
            out.push_str(&format!("    subgraph {id}[\"{title}\"]\n"));
            for n in members {
                out.push_str(&format!("        {}\n", node_line(n)));
            }
            out.push_str("    end\n");
        }
        for e in &self.edges {
            let mut labels: Vec<String> = Vec::new();
            for key in &e.items {
                let label = self.item(key).map_or_else(|| key.clone(), |i| i.label.clone());
                if !labels.contains(&label) {
                    labels.push(label);
                }
            }
            out.push_str(&format!("    {} -->|\"{}\"| {}\n", mermaid_id(&e.from), mermaid_text(&labels.join(", ")), mermaid_id(&e.to)));
        }
        // The published outputs, as leaves.
        for n in &self.nodes {
            for (k, key) in n.publishes.iter().enumerate() {
                let leaf = format!("{}_out{}", mermaid_id(&n.id), k + 1);
                let text = output_text(key);
                out.push_str(&format!("    {leaf}([\"{}\"])\n", mermaid_text(text.trim_start_matches("→ "))));
                out.push_str(&format!("    {} --> {leaf}\n", mermaid_id(&n.id)));
            }
        }
        let optional: Vec<String> = self.nodes.iter().filter(|n| n.enabled_by.is_some()).map(|n| mermaid_id(&n.id)).collect();
        let conditional: Vec<String> = self.nodes.iter().filter(|n| n.only_when.is_some()).map(|n| mermaid_id(&n.id)).collect();
        if !optional.is_empty() {
            out.push_str("    classDef optional stroke-dasharray: 6 4\n");
            out.push_str(&format!("    class {} optional\n", optional.join(",")));
        }
        if !conditional.is_empty() {
            out.push_str("    classDef conditional stroke-dasharray: 2 3\n");
            out.push_str(&format!("    class {} conditional\n", conditional.join(",")));
        }
        let skipped: Vec<String> = self.nodes.iter()
            .filter(|n| n.run.as_ref().is_some_and(|r| r.state == StepState::Skipped))
            .map(|n| mermaid_id(&n.id)).collect();
        if !skipped.is_empty() {
            out.push_str("    classDef skipped fill:#eeeeee,color:#777777\n");
            out.push_str(&format!("    class {} skipped\n", skipped.join(",")));
        }
        out
    }
}

mod algorithm;
mod io;
pub mod layout;
mod svg;
pub mod text;

#[cfg(test)]
mod tests;
