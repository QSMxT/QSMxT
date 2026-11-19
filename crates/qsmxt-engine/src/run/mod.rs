//! Running a plan: every step in order, each computed only when its earlier output is no longer
//! valid, its inputs read from the store as it starts and its outputs handed back as they are
//! produced.
//!
//! A plan is one run. Reconstructions from several runs (multi-orientation COSMOS and STI) are
//! not steps of any plan: `qsmxt run` makes them after every run's plan has run, from those runs'
//! outputs. They join the engine as a plan over several runs' [`ScanFacts`], whose steps read the
//! single-run plans' outputs through the same store.

use log::Level;
use serde_json::{json, Value};

use crate::cache::step_cache_key;
use crate::config::PipelineConfig;
use crate::observe::{Observer, StageEvent};
use crate::plan::{Pass, Plan, Stage, Step};
use crate::record::{Choice, Decision, RunRecord, StepRecord, StepState, Topic};
use crate::scan::{RunMetadata, ScanFacts};
use crate::stages;
use crate::store::{Completed, Data, Item, StageStore, Supplied};

/// Run `plan` (made by [`crate::plan::plan`] from `config` and `facts`) on the scan in `store`,
/// reporting to `observer`.
///
/// `config` is normalised as [`crate::plan::plan`] normalises it ([`PipelineConfig::normalised`]),
/// so the configuration the plan was made from can be passed as it is, and the steps run with,
/// and are cached under, what `qsmxt run` gives them.
///
/// What the plan says the data cannot produce ([`Plan::check`]) is the front end's to refuse
/// before calling this: `qsmxt` checks every run before the first starts.
///
/// Returns what the run did ([`RunRecord`]): every step of the plan, computed, reused or skipped,
/// with what was decided in it.
pub fn run<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, store: &mut S, observer: &mut dyn Observer,
) -> Result<RunRecord, S::Error> {
    let config = &config.normalised_as_planned(plan);
    // Every front end's config is held to the settings' ranges (qsmxt checks them earlier too).
    crate::schema::check_ranges(config)?;
    let (meta, load_record, _) = load(config, facts, store, observer)?;
    check_supplied_grids(plan, &meta, store)?;
    store.begin(&meta)?;
    let mut record = RunRecord {
        steps: vec![load_record],
        resampled: Some(meta.source_geometry.is_some()),
        recommendations: crate::recommendations::recorded(
            config, &crate::recommendations::Facts::of_run(&meta).with_manufacturer(facts.manufacturer.clone())),
        dicom_exported: false,
    };
    if !plan.processes() {
        observer.log(Level::Info, "No outputs enabled — nothing to process");
        store.finish()?;
        return Ok(record);
    }
    if !config.pipeline.do_qsm {
        observer.log(Level::Info, "QSM processing disabled — skipping reconstruction");
    }
    let mut r = Run::new(plan, config, facts, meta, store, observer);
    for step in plan.steps.iter().filter(|s| s.stage != Stage::Load) {
        record.steps.push(r.take(step)?);
    }
    r.store.finish()?;
    Ok(record)
}

/// What a run of a plan would do with one of its steps, found before running it ([`dry_run`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Expected {
    /// The step's name ([`crate::plan::Stage::name`]).
    pub id: String,
    pub state: ExpectedState,
    /// The key it would be cached under ([`crate::cache::step_cache_key`]); none for a step that
    /// would do nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// What a run would do with a step, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedState {
    /// Its earlier output is still valid: reused.
    Cached,
    /// Computed: it has not been computed before.
    WillRun,
    /// Computed: it was computed under another key, so its own parameters or the configuration it
    /// reads have changed.
    Changed,
    /// Computed with the parameters it was computed with before: a step it reads is computed
    /// again (or its outputs are no longer held).
    Stale,
    /// It would do nothing (motion correction off, an unresampled run's resampling to an axial
    /// grid and return to the acquired grid).
    Skipped,
}

impl ExpectedState {
    /// Whether the step would be computed.
    pub fn computes(self) -> bool {
        matches!(self, Self::WillRun | Self::Changed | Self::Stale)
    }
}

/// What [`run`] would do with each step of `plan` on the scan in `store`, in the plan's order,
/// computing nothing and leaving `store` as it is: the run is taken on a [`crate::store::MemoryStore::shadow`]
/// of it, by the same code, with each step that would be computed recorded there under its key
/// instead. The steps it says are cached are exactly those `run` then reuses.
///
/// What the plan says the data cannot produce ([`Plan::check`]) is not looked at, as for [`run`].
pub fn dry_run<V: crate::store::Volumes>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, store: &crate::store::MemoryStore<V>,
) -> Result<Vec<Expected>, crate::EngineError> {
    let config = &config.normalised_as_planned(plan);
    crate::schema::check_ranges(config)?;
    let before = store.recorded_keys();
    let mut shadow = store.shadow();
    let records = dry_steps(plan, config, facts, &mut shadow)?;
    let after = shadow.recorded_keys();
    Ok(records.into_iter().map(|record| {
        let key = after.get(&record.id).cloned();
        let state = match record.state {
            StepState::Cached => ExpectedState::Cached,
            StepState::Skipped => ExpectedState::Skipped,
            StepState::Computed => match before.get(&record.id) {
                None => ExpectedState::WillRun,
                Some(k) if Some(k) == key.as_ref() => ExpectedState::Stale,
                Some(_) => ExpectedState::Changed,
            },
        };
        let key = if state == ExpectedState::Skipped { None } else { key };
        Expected { id: record.id, state, key }
    }).collect())
}

/// [`dry_run`]'s run, on `store` (a shadow of the scan's): what it did with each step, each that
/// would be computed recorded there under its key, with what it reads and writes, instead.
fn dry_steps<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, store: &mut S,
) -> Result<Vec<StepRecord>, S::Error> {
    let mut quiet = ();
    let (meta, load_record, _) = load(config, facts, store, &mut quiet)?;
    check_supplied_grids(plan, &meta, store)?;
    store.begin(&meta)?;
    let mut records = vec![load_record];
    if plan.processes() {
        let mut r = Run::new(plan, config, facts, meta, store, &mut quiet);
        r.dry = true;
        for step in plan.steps.iter().filter(|s| s.stage != Stage::Load) {
            records.push(r.take(step)?);
        }
    }
    Ok(records)
}

/// Run one of `plan`'s engine steps on its own, on a run whose grid (from its load step) is
/// `meta`: what a front end re-running a single step uses (a mask preview, say), and tests.
/// Returns what the step did.
pub fn run_step<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, meta: &RunMetadata, step: &Step, store: &mut S,
    observer: &mut dyn Observer,
) -> Result<StepRecord, S::Error> {
    Run::new(plan, &config.normalised_as_planned(plan), facts, meta.clone(), store, observer).take(step)
}

/// A mask `sections` make (folded with `combine`, then `config`'s refinements after combining), or
/// part of it (`upto`), on the scan in `store`, as the run's mask step would make it: what a front end
/// shows while the recipe is edited ([`stages::mask::preview`]).
///
/// The steps the plan's mask step reads from are taken first, each only computed when its earlier
/// output is no longer valid, as a run takes them (so the run that follows reuses them). Nothing
/// else is written to the store: not the brain mask, and nothing downstream of it goes stale.
/// The images the sections work from are kept in `inputs` between previews, and built again only
/// when what they were built from changes ([`stages::mask::MaskInputs::key`]): a threshold or
/// morphology change reuses them. The weights of any deep-learning method in `sections` must be
/// ready ([`StageStore::models`]).
#[allow(clippy::too_many_arguments)]
pub fn preview_mask<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, store: &mut S, observer: &mut dyn Observer,
    inputs: &mut stages::mask::MaskInputs, sections: &[crate::masking::MaskSection],
    combine: crate::masking::MaskCombine, upto: stages::mask::Upto,
) -> Result<stages::mask::MaskPreview, S::Error> {
    let config = &config.normalised_as_planned(plan);
    crate::schema::check_ranges(config)?;
    let Some(at) = plan.steps.iter().position(|s| s.stage == Stage::Mask(Pass::Main)) else {
        return Err(crate::EngineError::Config("this run makes no brain mask to preview".into()).into());
    };
    let (meta, _, load_key) = load(config, facts, store, observer)?;
    store.begin(&meta)?;
    let mut upstream = vec![load_key];
    let mut r = Run::new(plan, config, facts, meta, store, observer);
    for step in mask_reads_from(plan, at) {
        r.take(step)?;
        upstream.push(format!("{}={}", step.name, r.store.recorded_key(&step.name).unwrap_or_default()));
    }
    let key = stages::mask::MaskInputs::key(config, &upstream.join(";"));
    let meta = r.meta.clone();
    inputs.fill(sections, config, key, &meta, || -> Result<_, S::Error> {
        let (phases, magnitudes) = r.echoes()?;
        Ok((phases, stages::magnitude::masking_magnitudes(sections, config, magnitudes, &meta)?))
    })?;
    r.store.models(&stages::mask::model_ids(sections, config))?;
    let preview = stages::mask::preview(sections, combine, config, inputs, &meta, upto)?;
    r.store.finish()?;
    Ok(preview)
}

/// The steps before the plan's step `at` whose outputs it reads, directly or through another of
/// them, in plan order. The inhomogeneity correction is not among them: a preview corrects the
/// echoes the recipe it shows reads itself ([`stages::magnitude::masking_magnitudes`]), from
/// the echoes the correction would read.
fn mask_reads_from(plan: &Plan, at: usize) -> Vec<&Step> {
    let mut wanted: Vec<Item> = plan.steps[at].reads.clone();
    let mut taken = vec![false; at];
    for i in (0..at).rev() {
        let step = &plan.steps[i];
        if step.stage != Stage::Load && step.outputs.iter().any(|o| wanted.contains(o)) {
            taken[i] = step.stage != Stage::InhomogeneityCorrection;
            wanted.extend(step.reads.iter().copied());
        }
    }
    (0..at).filter(|&i| taken[i]).map(|i| &plan.steps[i]).collect()
}

/// The grid the run is processed on: the cached load step's, or decided from the first echo's
/// header ([`crate::scan::resolve_geometry`]) and recorded; what the load step did; and its cache
/// key (which a mask preview keys the images it keeps on).
fn load<S: StageStore + ?Sized>(
    config: &PipelineConfig, facts: &ScanFacts, store: &mut S, observer: &mut dyn Observer,
) -> Result<(RunMetadata, StepRecord, String), S::Error> {
    // The algorithms that assume B0 along the slice normal decide the grid only when this scan's
    // B0 is tilted from it: on axial data they change nothing, and switching to or from one leaves
    // the cache alone. The header is read for that only when there are any.
    let mut header = None;
    let mut axial_only = crate::bridge::axial_only_algorithms(config, crate::scan::orientation_facts(facts, config));
    if !axial_only.is_empty() {
        let h = store.header()?;
        if !crate::scan::tilted_for_axial_only(&h.affine) {
            axial_only.clear();
        }
        header = Some(h);
    }
    // The grid every other step works on is decided here, so a setting that changes it leaves
    // nothing cached valid.
    let geometry = json!({
        "axial_resampling": config.pipeline.axial_resampling,
        "obliquity_threshold": config.pipeline.obliquity_threshold,
        "ignore_slice_gap": config.pipeline.ignore_slice_gap,
        // Everything after masking reads the motion-corrected echoes, and no later step's cache
        // key says so; changing these recomputes the run, as a geometry change does.
        "motion_correction": config.motion_correction,
        "registration": if config.motion_correction.enabled { Some(&config.registration) } else { None },
        // Empty unless they force resampling, so a cache from before this rule stays valid for
        // every run they did not resample.
        "axial_only": axial_only,
        // Replacing an input in place (a re-export, a corrected sidecar) changes everything
        // computed from it; nothing else would notice.
        "inputs": store.input_identity(),
    });
    let key = crate::cache::step_params_hash(None, &geometry);
    let done = |meta: &RunMetadata, state: StepState, mut decisions: Vec<Decision>| {
        decisions.extend(crate::scan::grid_decision(meta, facts, config));
        StepRecord { id: Stage::Load.name(), label: Stage::Load.label(), state, decisions }
    };
    if let Some(meta) = store.cached_scan(&key) {
        observer.log(Level::Info, "Skipping load (cached)");
        observer.stage("load", None, StageEvent::Cached);
        let record = done(&meta, StepState::Cached, store.decisions("load"));
        return Ok((meta, record, key));
    }
    observer.stage("load", None, StageEvent::Started("Loading NIfTI metadata"));
    store.start("load");
    let header = match header {
        Some(h) => h,
        None => store.header()?,
    };
    let mut meta = crate::scan::resolve_geometry(header.dims, header.voxel_size, header.affine, facts, config);
    if meta.source_geometry.is_none() {
        meta.orientation = header.orientation;
    }
    let mut decisions = Vec::new();
    meta.slice_geometry = match &header.sidecar {
        Some(sc) => {
            let (geometry, gap) = crate::scan::check_slice_gap(sc, header.dims, header.voxel_size, &meta, config)?;
            decisions.extend(gap);
            geometry
        }
        None => None,
    };
    let (d, v) = (header.dims, header.voxel_size);
    observer.log(Level::Info, &format!(
        "Volume: {}x{}x{}, {:.2}x{:.2}x{:.2}mm, {} echoes{}, B0={:.1}T, TEs={:?}s",
        d.0, d.1, d.2, v.0, v.1, v.2, meta.n_echoes,
        if facts.n_coils > 1 { format!(" x {} uncombined coils", facts.n_coils) } else { String::new() },
        meta.field_strength, meta.echo_times,
    ));
    store.record_scan(&key, &meta, &decisions)?;
    observer.stage("load", None, StageEvent::Finished("Load"));
    let record = done(&meta, StepState::Computed, decisions);
    Ok((meta, record, key))
}

/// Refuse a map supplied with the scan that the run reads and that cannot be brought onto its
/// grid ([`stages::supplied::grid_problem`]), once the load step has decided the grid and before
/// anything is computed from the scan (#384). A segmentation the segmentation step reads is left
/// to it: like a mask that cannot be used, it segments the scan instead.
fn check_supplied_grids<S: StageStore + ?Sized>(plan: &Plan, meta: &RunMetadata, store: &mut S) -> Result<(), S::Error> {
    let mut problems = Vec::new();
    for item in SUPPLIED_MAPS {
        let read = plan.steps.iter().any(|s| {
            s.reads.contains(&item) && !(item == Item::SuppliedSegmentation && s.stage == Stage::Segmentation)
        });
        if let Some(g) = read.then(|| store.supplied_grid(&item)).flatten() {
            problems.extend(stages::supplied::grid_problem(&item, &g.origin, g.dims, &g.affine, meta));
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(crate::EngineError::DimensionMismatch(problems.join(" ")).into())
}

/// The maps a scan can be supplied with, which the steps read onto the working grid
/// ([`Run::map`]).
const SUPPLIED_MAPS: [Item; 4] = [Item::SuppliedChi, Item::SuppliedR2, Item::SuppliedR2prime, Item::SuppliedSegmentation];

/// Every echo's phase, and every echo's magnitude (or none).
pub(crate) type Echoes = (Vec<Vec<f64>>, Vec<Vec<f64>>);

/// One run of a plan, between its steps.
pub(crate) struct Run<'r, S: StageStore + ?Sized> {
    pub(crate) plan: &'r Plan,
    pub(crate) config: &'r PipelineConfig,
    pub(crate) facts: &'r ScanFacts,
    pub(crate) meta: RunMetadata,
    pub(crate) store: &'r mut S,
    pub(crate) observer: &'r mut dyn Observer,
    /// What became of the step being taken: `None` until it is computed or found cached, and
    /// skipped if neither.
    pub(crate) state: Option<StepState>,
    /// What it decided so far.
    pub(crate) decisions: Vec<Decision>,
    /// Where the decisions made while computing it start: those are kept with it in the store,
    /// for a run that reuses it. The ones before are made every run.
    pub(crate) computing_from: usize,
    /// A dry run ([`dry_run`]): a step that would be computed is recorded under its key, its
    /// outputs held, instead.
    pub(crate) dry: bool,
    /// What the steps taken so far decided.
    pub(crate) decided: Vec<Decision>,
}

/// A computed step, as a step of the run hands it to [`Run::record`]: [`Completed`] without what
/// it read, which the run takes from the plan ([`Run::reads`]), and without its decisions, which
/// the run kept as it made them.
pub(crate) struct Done<'a> {
    pub(crate) step: &'a str,
    pub(crate) algorithm: Option<&'a str>,
    pub(crate) params: &'a Value,
    pub(crate) outcome: Option<(&'static str, Value)>,
    pub(crate) key: &'a str,
    pub(crate) outputs: &'a [Item],
}

impl<'r, S: StageStore + ?Sized> Run<'r, S> {
    fn new(
        plan: &'r Plan, config: &'r PipelineConfig, facts: &'r ScanFacts, meta: RunMetadata, store: &'r mut S,
        observer: &'r mut dyn Observer,
    ) -> Self {
        Run { plan, config, facts, meta, store, observer, state: None, decisions: Vec::new(), computing_from: 0, dry: false,
              decided: Vec::new() }
    }

    /// Take `step`, and say what it did.
    fn take(&mut self, step: &Step) -> Result<StepRecord, S::Error> {
        (self.state, self.computing_from) = (None, 0);
        self.decisions.clear();
        self.step(step)?;
        self.decided.extend(self.decisions.iter().cloned());
        Ok(StepRecord {
            id: step.name.clone(),
            label: step.label.clone(),
            state: self.state.take().unwrap_or(StepState::Skipped),
            decisions: std::mem::take(&mut self.decisions),
        })
    }

    /// Note a decision of the step being taken.
    pub(crate) fn decide(&mut self, decision: Decision) {
        self.decisions.push(decision);
    }

    /// Note that the step did nothing, and why.
    pub(crate) fn skip(&mut self, reason: &str) {
        self.decide(Decision::new(Topic::Step, Choice::Skipped, reason));
    }

    /// The step's earlier output is reused: what it decided when it was computed holds.
    pub(crate) fn reuse(&mut self, step: &str) {
        self.state = Some(StepState::Cached);
        let earlier = self.store.decisions(step);
        self.decisions.extend(earlier);
    }

    fn step(&mut self, step: &Step) -> Result<(), S::Error> {
        match step.stage {
            Stage::Load => Ok(()),
            Stage::ScalePhase if self.facts.n_coils > 1 => self.combine_coils(),
            Stage::ScalePhase if self.facts.real_imaginary => self.scale_real_imaginary(),
            Stage::ScalePhase => self.scale_phase(),
            Stage::TukeyWindow => self.tukey_window(),
            Stage::ResampleAxial => self.resample_axial(),
            Stage::InhomogeneityCorrection => self.homogeneity(),
            Stage::Magnitude => self.magnitude(),
            Stage::Mask(Pass::Main) => self.mask(step),
            Stage::Mask(Pass::Reliable) => self.reliable_mask(step),
            Stage::FieldMap => self.field_map(),
            Stage::BackgroundRemoval(pass) => self.background_removal(step, pass),
            Stage::Inversion(pass) if stages::inversion::iqsm_algorithm(self.config).is_some() => self.iqsm(step, pass),
            Stage::Inversion(pass) => self.inversion(step, pass),
            Stage::Tgv(pass) => self.tgv(pass),
            Stage::Qsmart(pass) => self.qsmart(pass),
            Stage::TwoPassCombination => self.two_pass_combination(),
            Stage::Reference => self.reference(false),
            Stage::SinglePassReference => self.reference(true),
            Stage::Swi => self.swi(),
            Stage::Smwi => self.smwi(),
            Stage::Relaxometry => self.relaxometry(),
            Stage::R2 => self.r2(step),
            Stage::ChiSeparation => self.chi_separation(step),
            Stage::Motion => self.motion(),
            Stage::Segmentation => self.segmentation(step),
            Stage::Analysis => self.analysis(),
            Stage::OutputSpace => self.output_space(),
        }
    }

    /// `params` with the identity ([`StageStore::identity`]) of each file supplied with the scan
    /// that the step reads, under `supplied`: replacing one of those files recomputes the steps
    /// that read it and nothing else (#383). `params` is returned as it is when the step reads none.
    pub(crate) fn with_supplied(&mut self, mut params: Value, items: &[Item]) -> Value {
        if !items.is_empty() {
            let identities: serde_json::Map<String, Value> =
                items.iter().map(|item| (item.key(), self.store.identity(item))).collect();
            params["supplied"] = Value::Object(identities);
        }
        params
    }

    /// `item` as a step reads it: a map supplied with the scan on the working grid
    /// ([`stages::supplied::conform`]), anything else as the store holds it.
    pub(crate) fn map(&mut self, item: &Item) -> Result<Vec<f64>, S::Error> {
        if SUPPLIED_MAPS.contains(item) {
            if let Supplied::Found { origin, data, dims, affine } = self.store.supplied(item) {
                return Ok(stages::supplied::conform(item, &origin, data, dims, &affine, &self.meta)?);
            }
        }
        self.store.real(item)
    }

    /// The key `step` is cached under, unless its earlier output is still valid (then `None`, with
    /// `skipping` logged). Otherwise the step starts, `doing` being what it does.
    pub(crate) fn begin(&mut self, step: &str, algorithm: Option<&str>, params: &Value, skipping: &str, doing: &str) -> Option<String> {
        let key = step_cache_key(step, algorithm, params, self.config);
        if self.store.cached(step, &key) {
            self.observer.log(Level::Info, skipping);
            self.observer.stage(step, algorithm, StageEvent::Cached);
            self.reuse(step);
            return None;
        }
        if self.dry {
            self.pretend(step, algorithm, params, &key);
            return None;
        }
        self.observer.stage(step, algorithm, StageEvent::Started(doing));
        self.store.start(step);
        self.computing_from = self.decisions.len();
        Some(key)
    }

    /// In a dry run, record `step` as computed under `key`, with the outputs the plan says it
    /// writes, so the steps after it see what they would.
    fn pretend(&mut self, step: &str, algorithm: Option<&str>, params: &Value, key: &str) {
        let outputs = self.details(step).map(|d| d.writes).unwrap_or_default();
        // A dry run's store is a shadow ([`crate::store::MemoryStore::shadow`]), which takes these without fail.
        for item in &outputs {
            let _ = self.store.put(*item, Data::Mask(&[]));
        }
        let _ = self.record(Done { step, algorithm, params, outcome: None, key, outputs: &outputs });
    }

    /// Keep an output, and show it. A volume on a grid of its own (a projection) is kept but not
    /// shown: the observer's volumes are on the run's grid.
    pub(crate) fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), S::Error> {
        self.store.put(item, data)?;
        match data {
            Data::Real(v) => self.observer.output(&item, qsm_core::pipeline::VolumeRef::Real(v)),
            Data::Mask(m) => self.observer.output(&item, qsm_core::pipeline::VolumeRef::Mask(m)),
            Data::OnGrid(..) | Data::Json(_) | Data::Text(_) | Data::Same(_) => {}
        }
        Ok(())
    }

    /// Record a computed step, with what it decided while computed; `label` names it in the log.
    pub(crate) fn complete(&mut self, c: Done<'_>, label: &str) -> Result<(), S::Error> {
        let (step, algorithm) = (c.step, c.algorithm);
        self.record(c)?;
        self.observer.stage(step, algorithm, StageEvent::Finished(label));
        Ok(())
    }

    /// Keep a computed step in the store, with what it decided while computed and what it read:
    /// the plan's [`Step::reads`] for it, as this run's decisions so far left them
    /// ([`Self::reads`]), never a list of its own.
    pub(crate) fn record(&mut self, c: Done<'_>) -> Result<(), S::Error> {
        let decisions = self.decisions[self.computing_from.min(self.decisions.len())..].to_vec();
        let inputs = self.reads(c.step);
        let Done { step, algorithm, params, outcome, key, outputs } = c;
        self.store.record(&Completed { step, algorithm, params, outcome, key, inputs: &inputs, outputs, decisions: &decisions })?;
        self.state = Some(StepState::Computed);
        Ok(())
    }

    /// What `step` reads: the plan's [`Step::reads`], drawn again from the facts as the run's
    /// decisions so far left them ([`crate::pipeline_graph::as_decided`]) — a supplied mask that
    /// could not be used is computed from the echoes, and reads them. The same items the graph
    /// after the run ([`crate::pipeline_graph::pipeline_graph_after`]) draws the step reading.
    fn reads(&self, step: &str) -> Vec<Item> {
        self.details(step).map(|d| d.reads).unwrap_or_default()
    }

    /// What `step` reads and writes in this run: the plan's, drawn again from the facts as the
    /// run's decisions so far left them, and knowing whether the run is resampled (a run that is
    /// not writes its working echoes before the resampling step, which it skips).
    fn details(&self, step: &str) -> Option<crate::pipeline_graph::StepDetails> {
        let decided = self.decided.iter().chain(&self.decisions);
        let ran = crate::pipeline_graph::as_decided(self.facts, decided);
        let resampled = Some(self.meta.source_geometry.is_some());
        crate::pipeline_graph::step_details(self.plan, self.config, &ran, resampled).into_iter().zip(&self.plan.steps)
            .find(|(_, s)| s.name == step).map(|(d, _)| d)
    }

    pub(crate) fn log(&mut self, level: Level, message: &str) {
        self.observer.log(level, message);
    }

    /// Retrospective motion correction: every echo co-registered onto the reference echo and the
    /// mask cut to where every echo still has data. Off (or impossible: one echo, or no
    /// magnitude), what an earlier run of it left is stale: its coverage mask must not be
    /// published as this run's, and every step that read the corrected echoes or that mask has to
    /// run again on the uncorrected ones.
    fn motion(&mut self) -> Result<(), S::Error> {
        let m = &self.config.motion_correction;
        if !m.enabled {
            return self.store.discard("motion", &[Item::CoverageMask]);
        }
        // Decided per run by the plan, which said so before the run started.
        self.decide(motion_decision(self.plan.motion_corrected));
        if !self.plan.motion_corrected {
            return self.store.discard("motion", &[Item::CoverageMask]);
        }
        let n = self.n_echoes();
        let params = stages::motion::params(self.config, &self.meta);
        let Some(key) = self.begin("motion", None, &params, "Skipping motion correction (cached)",
                                   "Motion correction (rigid co-registration of the echoes)") else { return Ok(()) };
        if m.reference != crate::MotionReference::First
            && self.config.masking.sections.iter().any(|s| s.input == crate::MaskingInput::MagnitudeFirst)
        {
            self.log(Level::Warn, &format!(
                "Motion correction aligns the echoes onto echo {} but the mask was made on the first \
                 echo's magnitude; if the head moved, the mask is offset from the corrected echoes", m.reference));
        }
        let (mut phases, mut magnitudes) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for e in 1..=n {
            phases.push(self.store.real(&Item::Phase(e))?);
            magnitudes.push(self.store.real(&Item::Magnitude(e))?);
        }
        let mask = self.store.mask(&Item::BrainMask)?;
        let phase_refs: Vec<&[f64]> = phases.iter().map(|v| v.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|v| v.as_slice()).collect();
        let r = stages::motion::correct(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?;
        drop((phases, magnitudes));
        let mut outputs = Vec::with_capacity(2 * n + 2);
        for e in 1..=n {
            self.put(Item::CorrectedPhase(e), Data::Real(&r.phases[e - 1]))?;
            self.put(Item::CorrectedMagnitude(e), Data::Real(&r.magnitudes[e - 1]))?;
            outputs.extend([Item::CorrectedPhase(e), Item::CorrectedMagnitude(e)]);
        }
        self.put(Item::CoverageMask, Data::Mask(&r.mask))?;
        self.put(Item::MotionReport, Data::Json(&stages::motion::report(&r)))?;
        outputs.extend([Item::CoverageMask, Item::MotionReport]);
        for (level, line) in stages::motion::summary(&r, &mask) {
            self.log(level, &line);
        }
        self.complete(Done {
            step: "motion", algorithm: None, params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "Motion correction")
    }

    /// Whole-brain parcellation of the combined magnitude by SynthSeg, with its lookup table — or
    /// a segmentation supplied with the scan, read as FreeSurfer ids, when one is asked for and
    /// fits the working grid.
    fn segmentation(&mut self, step: &Step) -> Result<(), S::Error> {
        let config = self.config;
        let c = &config.segmentation;
        let supplied: &[Item] = if c.custom_dseg_tool.is_some() { &[Item::SuppliedSegmentation] } else { &[] };
        let params = self.with_supplied(stages::segmentation::params(config), supplied);
        let Some(key) = self.begin("segmentation", Some("synthseg"), &params, "Skipping segmentation (cached)",
                                   "Segmenting") else { return Ok(()) };
        let lookup = stages::segmentation::lookup_table(c.version);
        // Prefer a bring-your-own segmentation, as masking does.
        if let Some(tool) = &c.custom_dseg_tool {
            match self.store.supplied(&Item::SuppliedSegmentation) {
                Supplied::Found { origin, data, dims, affine }
                    if stages::supplied::fit(dims, &affine, &self.meta) != stages::supplied::Fit::Refused =>
                {
                    let item = Item::SuppliedSegmentation;
                    let data = stages::supplied::conform(&item, &origin, data, dims, &affine, &self.meta)?;
                    self.log(Level::Info, &format!("Using custom segmentation from {origin}"));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Supplied,
                        "The segmentation was the one supplied with the scan.").with_detail(origin));
                    self.put(Item::Segmentation, Data::Real(&data))?;
                    self.put(Item::SegmentationLabels, Data::Text(&lookup))?;
                    return self.complete(Done {
                        step: "segmentation", algorithm: Some("synthseg"), params: &params, outcome: None, key: &key,
                        outputs: &[Item::Segmentation, Item::SegmentationLabels],
                    }, "Segmentation (custom)");
                }
                Supplied::Found { origin, dims, .. } => {
                    self.log(Level::Warn, &format!(
                        "custom segmentation {origin} is {}x{}x{}, not on the scan's grid (the run's is {}x{}x{}) — computing instead",
                        dims.0, dims.1, dims.2, self.meta.dims.0, self.meta.dims.1, self.meta.dims.2));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Computed,
                        "The supplied segmentation is not on the run's grid, so the scan was segmented.")
                        .with_detail(format!("{origin}: {}x{}x{}", dims.0, dims.1, dims.2)));
                }
                Supplied::Unreadable { origin, error } => {
                    self.log(Level::Warn, &format!(
                        "could not read custom segmentation {origin} ({error}) — computing instead"));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Computed,
                        "The supplied segmentation could not be read, so the scan was segmented.")
                        .with_detail(format!("{origin}: {error}")));
                }
                Supplied::Missing { searched } => {
                    self.log(Level::Info, &format!(
                        "no custom segmentation under derivatives (tool: {tool}) — computing"));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Computed,
                        "No supplied segmentation was found, so the scan was segmented.").with_detail(searched));
                }
            }
        }
        if !self.store.has(&Item::CombinedMagnitude) {
            self.log(Level::Warn, "Skipping segmentation: no combined magnitude");
            self.skip("There was no combined magnitude to segment.");
            return Ok(());
        }
        stages::segmentation::weights_available(c.version)?;
        self.store.models(&step.models)?;
        let labels = c.version.to_core().labels();
        self.log(Level::Info, &format!("Segmentation (SynthSeg {}, {} labels{}{})",
            c.version, labels.ids.len(),
            if c.flip_averaging { ", flip-averaged" } else { "" },
            if c.topology_cleanup { ", topology cleanup" } else { "" }));
        let magnitude = self.store.real(&Item::CombinedMagnitude)?;
        let seg = stages::segmentation::synthseg(&magnitude, &self.meta, c, self.observer)?;
        drop(magnitude);
        self.put(Item::Segmentation, Data::Real(&seg.labels))?;
        self.put(Item::SegmentationLabels, Data::Text(&lookup))?;
        self.put(Item::SegmentationVolumes, Data::Json(&json!(seg.volumes)))?;
        let present = labels.ids.iter().skip(1).zip(seg.volumes.iter().skip(1)).filter(|(_, &v)| v > 0.0).count();
        self.log(Level::Info, &format!("Segmented {} of {} structures", present, labels.ids.len() - 1));
        self.complete(Done {
            step: "segmentation", algorithm: Some("synthseg"), params: &params, outcome: None, key: &key,
            outputs: &[Item::Segmentation, Item::SegmentationLabels, Item::SegmentationVolumes],
        }, "Segmentation")
    }

    /// Per-structure statistics of every quantitative map the run made (or was given) over the
    /// segmentation ([`stages::analysis`]). The maps are the ones there, not the ones the
    /// configuration implies, so a supplied map is summarised the same as one this run computed,
    /// and a map that was enabled but could not be made simply contributes no rows. Last of the
    /// map-producing steps, so it summarises all of them.
    fn analysis(&mut self) -> Result<(), S::Error> {
        use stages::analysis::{self as a, MAPS};
        let Some(seg_item) = self.segmentation_item() else {
            self.log(Level::Warn, "Skipping analysis: no segmentation");
            self.skip("There was no segmentation to summarise the maps over.");
            return Ok(());
        };
        let mut maps: Vec<(Item, &str, &str)> = MAPS.into_iter().filter(|(i, ..)| self.store.has(i)).collect();
        // A supplied χ is summarised where the run made none.
        if self.config.separation.custom_qsm_tool.is_some() && !maps.iter().any(|(i, ..)| *i == Item::Chi)
            && self.store.has(&Item::SuppliedChi)
        {
            maps.insert(0, (Item::SuppliedChi, a::CHIMAP, "ppm"));
        }
        let identities: Vec<(&str, Value)> = maps.iter().map(|(i, name, _)| (*name, self.store.identity(i))).collect();
        let params = a::params(self.config.segmentation.version, self.store.identity(&seg_item), &identities);
        let key = step_cache_key("analysis", None, &params, self.config);
        if self.store.cached("analysis", &key) {
            self.log(Level::Info, "Skipping analysis (cached)");
            self.observer.stage("analysis", None, StageEvent::Cached);
            self.reuse("analysis");
            return Ok(());
        }
        if maps.is_empty() {
            self.log(Level::Warn, "Skipping analysis: the run produced no quantitative map to summarise");
            self.skip("The run made no quantitative map to summarise.");
            return Ok(());
        }
        if self.dry {
            self.pretend("analysis", None, &params, &key);
            return Ok(());
        }
        self.observer.stage("analysis", None, StageEvent::Started("Summarising maps per structure"));
        self.store.start("analysis");
        let seg = self.map(&seg_item)?;
        let labels = self.config.segmentation.version.to_core().labels();
        // SynthSeg's per-label volumes, when this run segmented the scan itself.
        let volumes: Option<Vec<f64>> = self.store.report(&Item::SegmentationVolumes)
            .and_then(|v| serde_json::from_value(v).ok());
        let mut table = String::from(a::HEADER);
        for (item, name, unit) in &maps {
            let map = self.map(item)?;
            if map.len() != seg.len() {
                let what = self.store.identity(item);
                return Err(crate::EngineError::DimensionMismatch(format!(
                    "segmentation has {} voxels but {} has {}", seg.len(), what.as_str().unwrap_or_default(), map.len())).into());
            }
            // What this map was referenced to, as referencing recorded it, so the table says what
            // its zero means. A map that was never referenced (the χ-separation maps) has no such
            // record, and its columns stay empty.
            let reference = a::ReferenceInfo::from_fields(&self.store.annotation(item));
            table.push_str(&a::map_rows(name, unit, &map, &seg, labels, volumes.as_deref(), reference.as_ref()));
        }
        let names: Vec<&str> = maps.iter().map(|(_, name, _)| *name).collect();
        self.log(Level::Info, &format!("Per-structure statistics for {}", names.join(", ")));
        self.decide(Decision::new(Topic::AnalysisMaps, Choice::Summarised, "The maps summarised per structure.")
            .with_detail(names.join(", ")));
        self.put(Item::RegionStats, Data::Text(&table))?;
        self.complete(Done {
            step: "analysis", algorithm: None, params: &params, outcome: None, key: &key, outputs: &[Item::RegionStats],
        }, "Per-structure analysis")
    }

    /// The outputs of a run resampled to axial, back on the grid it was acquired on
    /// ([`stages::output_space`]) — or, with `--output-space working`, left where they are.
    fn output_space(&mut self) -> Result<(), S::Error> {
        use stages::output_space::{self as o, Kind};
        // Not resampled: the outputs are on the acquired grid already (the record's `resampled`).
        let Some((dst_dims, dst_affine)) = self.meta.source_geometry else { return Ok(()) };
        if self.config.pipeline.output_space != crate::OutputSpace::Acquired {
            self.log(Level::Info, "Leaving outputs on the resampled grid (--output-space working)");
            return self.store.keep_working_grid();
        }
        let params = o::params(dst_dims);
        let Some(key) = self.begin("output_space", None, &params, "Skipping output_space (cached)",
                                   "Resampling outputs to the acquired grid") else { return Ok(()) };
        let (src_dims, src_affine) = (self.meta.dims, self.meta.affine);
        self.log(Level::Info, &format!(
            "Returning outputs to the acquired grid: {}x{}x{} -> {}x{}x{}",
            src_dims.0, src_dims.1, src_dims.2, dst_dims.0, dst_dims.1, dst_dims.2));
        let published: Vec<(Item, Kind)> = o::published(self.n_echoes()).into_iter()
            .filter(|(i, _)| i.published_in(self.config) && self.store.has(i)).collect();
        let dst = o::acquired(dst_dims, &dst_affine);
        let src_voxels = src_dims.0 * src_dims.1 * src_dims.2;
        let mut done: Vec<Item> = Vec::new();
        // Phase is only meaningful alongside its magnitude, so those pairs go first, together.
        for &(phase_item, kind) in &published {
            let Kind::Phase(mag_item) = kind else { continue };
            if !published.iter().any(|(i, _)| *i == mag_item) {
                let what = self.store.identity(&phase_item);
                self.log(Level::Warn, &format!(
                    "Leaving {} on the working grid: wrapped phase can only be resampled with its \
                     magnitude, and there is none beside it", what.as_str().unwrap_or_default()));
                self.decide(Decision::new(Topic::OutputGrid, Choice::Kept,
                    "Phase without a magnitude beside it was left on the working grid.")
                    .with_detail(what.as_str().unwrap_or_default()));
                done.push(phase_item);
                continue;
            }
            let (phase, magnitude) = (self.store.real(&phase_item)?, self.store.real(&mag_item)?);
            if let Some((m, p)) = qsm_core::geometry::resample_complex_onto(
                &magnitude, &phase, src_dims, &src_affine, dst_dims, &dst_affine,
            ) {
                self.put(phase_item, Data::OnGrid(&p, &dst))?;
                self.put(mag_item, Data::OnGrid(&m, &dst))?;
                done.extend([phase_item, mag_item]);
            }
        }
        for &(item, kind) in &published {
            if done.contains(&item) || matches!(kind, Kind::Projection(_)) {
                continue;
            }
            let data = self.store.real(&item)?;
            // Resampling indexes the source by the working grid, so anything on a different grid
            // would be read out of bounds. Leave it where it is rather than corrupt it.
            if data.len() != src_voxels {
                let what = self.store.identity(&item);
                self.log(Level::Warn, &format!(
                    "Leaving {} on its own grid: {} voxels, not the {} of the working grid",
                    what.as_str().unwrap_or_default(), data.len(), src_voxels));
                self.decide(Decision::new(Topic::OutputGrid, Choice::Kept,
                    "An output not on the working grid was left on its own grid.")
                    .with_detail(what.as_str().unwrap_or_default()));
                continue;
            }
            if let Some(out) = o::resample(&data, kind, (&src_dims, &src_affine), &dst)? {
                self.put(item, Data::OnGrid(&out, &dst))?;
            }
        }
        // A projection cannot be interpolated onto the acquired grid as if it were a volume — its
        // slices are slabs, not samples — so each is rebuilt from its source, now on that grid.
        let dst_grid = qsm_core::Grid::new(dst_dims.0, dst_dims.1, dst_dims.2, dst.voxel_size.0, dst.voxel_size.1, dst.voxel_size.2);
        for &(item, kind) in &published {
            let Kind::Projection(source) = kind else { continue };
            if !self.store.has(&source) {
                continue;
            }
            let window = if source == Item::Swi { self.config.swi.mip_window } else { self.config.smwi.mip_window };
            let volume = self.store.real(&source)?;
            let (mip, geometry) = stages::swi::project_on(&volume, &dst_grid, &dst_affine, window)?;
            self.put(item, Data::OnGrid(&mip, &geometry))?;
        }
        // Read: the working-grid versions each output was returned from.
        let items: Vec<Item> = published.iter().map(|(i, _)| *i).collect();
        self.complete(Done {
            step: "output_space", algorithm: None, params: &params, outcome: None, key: &key, outputs: &items,
        }, "Output space")
    }
}

/// Whether the field-mapping step writes [`Item::FieldMask`]: when it maps one total field from
/// every echo (not a per-echo QSM's echo fields, nor a route without a field map).
pub(crate) fn field_mask_written(plan: &Plan, config: &PipelineConfig, n_echoes: usize) -> bool {
    plan.has(Stage::FieldMap) && !crate::bridge::per_echo(config, n_echoes)
}

/// How a pass is named in progress messages and the log: nothing for a single-pass run's (only)
/// pass, so its log reads as it always has.
pub(crate) fn pass_label(pass: Pass) -> &'static str {
    match pass {
        Pass::Main => "",
        Pass::Reliable => " (reliable pass)",
    }
}

/// Whether motion correction, switched on, was applied to the run ([`Topic::MotionCorrection`]):
/// the plan decides it per run ([`crate::plan::Plan::motion_corrected`]), so the methods text can
/// say which runs it was applied to.
pub(crate) fn motion_decision(applied: bool) -> Decision {
    if applied {
        Decision::new(Topic::MotionCorrection, Choice::Applied, "Motion correction was applied.")
    } else {
        Decision::new(Topic::MotionCorrection, Choice::NotApplied,
            "Motion correction was not applied: it needs at least two echoes with magnitude images.")
    }
}

/// The χ bound DECOMPOSE's search takes from the field strength `b0` (T) and `echo_times` (s),
/// when it takes it from the echo spacing ([`Topic::ChiBound`]), in ppm, so the methods text can
/// give it. `None` for another method, a fixed bound, or echo times that give no bound (qsm-core
/// refuses the run).
pub(crate) fn chi_bound_decision(config: &PipelineConfig, b0: f64, echo_times: &[f64]) -> Option<Decision> {
    let s = &config.separation;
    if s.algorithm != crate::SeparationAlgorithm::Decompose || s.decompose.chi_bound != crate::ChiBoundChoice::EchoSpacing {
        return None;
    }
    let bound = s.decompose.chi_bound.resolve(b0, echo_times)?;
    Some(Decision::new(Topic::ChiBound, Choice::Computed, format!(
        "The χ search was bounded at {bound:.2} ppm, from the field strength and echo spacing.")))
}

/// The run's phase is converted to ppm with the scanner's `ImagingFrequency` when the scan gives
/// one (decided 2026-11-10: every run, as the MEDI and χ-separation toolboxes do), else with γ̄
/// times the nominal field strength. Only the fallback is recorded ([`Topic::CentreFrequency`]),
/// so the methods text can say it.
pub(crate) fn centre_frequency_decision(facts: &ScanFacts) -> Option<Decision> {
    facts.imaging_frequency.is_none().then(|| {
        Decision::new(Topic::CentreFrequency, Choice::Unavailable,
            "Phase was converted to ppm with the nominal field strength: the data gave no centre \
             frequency (ImagingFrequency).")
            .with_detail(format!("{} T", facts.field_strength))
    })
}

/// Whether MEDI+0's CSF term is applied to a MEDI run of `n_echoes` echoes ([`Topic::MediCsf`]):
/// QSM.rs applies it on three or more and runs MEDI without it on fewer (the plan warns), so the
/// methods text can say what was done. `None` for another inversion or with the term off.
pub(crate) fn medi_csf_decision(config: &PipelineConfig, n_echoes: usize) -> Option<Decision> {
    (config.inversion.algorithm == crate::QsmAlgorithm::Medi && config.inversion.medi.csf).then(|| {
        if n_echoes >= 3 {
            Decision::new(Topic::MediCsf, Choice::Applied,
                "MEDI's zero reference to the ventricular CSF (MEDI+0) was applied.")
        } else {
            Decision::new(Topic::MediCsf, Choice::NotApplied,
                "MEDI's zero reference to the ventricular CSF (MEDI+0) was not applied: it needs at least three echoes.")
        }
    })
}

mod volumes;

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod reads_tests;

#[cfg(test)]
mod dry_tests;

#[cfg(test)]
mod stage_tests;

#[cfg(test)]
mod rerun_tests;

#[cfg(test)]
mod record_tests;

#[cfg(test)]
mod preview_tests;
