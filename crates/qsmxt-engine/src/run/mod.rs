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
use crate::observe::{Observer, StageEvent, VolumeData};
use crate::plan::{Pass, Plan, Stage, Step};
use crate::record::{Choice, Decision, RunRecord, StepRecord, StepState, Topic};
use crate::scan::{RunMetadata, ScanFacts};
use crate::stages;
use crate::store::{Completed, Contrast, Data, Item, StageStore, Supplied};

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

    /// The MESE's echoes, voxel-major on the working grid, and where its FOV reaches
    /// ([`stages::r2::mese_on_grid`]); why not, without a usable MESE.
    fn mese_on_grid(&mut self, mese: &crate::store::Mese) -> Result<Result<stages::r2::MeseOnGrid, String>, S::Error> {
        let store = &mut *self.store;
        Ok(stages::r2::mese_on_grid(mese, |n| store.supplied(&Item::MeseMagnitude(n)), self.meta.dims, &self.meta.affine)?)
    }

    /// The magnitude the configured inversion takes
    /// ([`stages::inversion::magnitude_input`]).
    fn inversion_magnitude(&mut self) -> Result<Option<Vec<f64>>, S::Error> {
        match stages::inversion::magnitude_input(self.config) {
            Some(stages::inversion::MagnitudeInput::Combined) => self.combined_magnitude(),
            Some(stages::inversion::MagnitudeInput::Rss) => {
                let (nx, ny, nz) = self.meta.dims;
                let n = nx * ny * nz;
                Ok(self.echo_magnitude_stack()?.map(|stack| {
                    let echoes: Vec<&[f64]> = stack.chunks(n).collect();
                    qsm_core::pipeline::rss_combine(&echoes).unwrap_or_default()
                }))
            }
            Some(stages::inversion::MagnitudeInput::Echoes) => self.echo_magnitude_stack(),
            None => Ok(None),
        }
    }

    /// Background-field removal for one pass, from the total field — or, for iQFM, straight from
    /// the echoes' wrapped phase (joint unwrapping and removal).
    fn background_removal(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let name = step.stage.name();
        let label = pass_label(pass);
        let iqfm = self.config.bg_removal.algorithm == crate::enums::BfAlgorithm::Iqfm;
        let algorithm = format!("{}", self.config.bg_removal.algorithm);
        let mut params = stages::background::params(self.config);
        if self.per_echo() {
            params["per_echo"] = json!(true);
        }
        let (skipping, doing) = if iqfm {
            ("Skipping iQFM field preparation (cached)", format!("iQFM field preparation{label}"))
        } else {
            ("Skipping bgremove (cached)", format!("Background field removal{label}"))
        };
        let Some(key) = self.begin(&name, Some(&algorithm), &params, skipping, &doing) else { return Ok(()) };
        let mask_item = self.pass_mask(pass);
        let outputs = [Item::LocalField(pass), Item::LocalFieldMask(pass)];
        // UK Biobank's mask trimming reads reliability maps of the first and last echo's phase
        // over the untrimmed mask (bg_removal.ukb_mask_trim).
        let reliability = if self.config.bg_removal.ukb_mask_trim && !iqfm {
            let (phases, _) = self.phase_echoes()?;
            let mask = self.store.mask(&mask_item)?;
            let refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
            stages::background::ukb_phase_reliability(&refs, &mask, &self.meta, self.config)?
        } else {
            None
        };
        if self.per_echo() {
            // Each echo's field on its own (iQFM, from every echo's phase, has no per-echo form).
            let mask = self.store.mask(&mask_item)?;
            self.store.models(&step.models)?;
            if let Some(d) = self.reconstruction_box(&mask) {
                self.decide(d);
            }
            let n = self.n_echoes();
            let mut outputs = Vec::with_capacity(2 * n);
            for e in 1..=n {
                let field = self.store.real(&Item::EchoField(e))?;
                let magnitude = if stages::background::takes_echo_magnitudes(self.config) { self.one_echo_magnitude(e)? } else { None };
                self.log(Level::Info, &format!("Background removal ({algorithm}), echo {e} of {n}"));
                let local = stages::background::remove_background(&field, &mask, magnitude.as_deref(), reliability.as_ref(), &self.echo_meta(e),
                                                                  self.config, self.observer)?;
                self.put(Item::EchoLocalField(e), Data::Real(&local.local_field_ppm))?;
                self.put(Item::EchoLocalFieldMask(e), Data::Mask(&local.mask))?;
                outputs.extend([Item::EchoLocalField(e), Item::EchoLocalFieldMask(e)]);
            }
            return self.complete(Done {
                step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, outputs: &outputs,
            }, &format!("Background removal ({algorithm}), each echo"));
        }
        let local = if iqfm {
            let (phases, magnitudes) = self.phase_echoes()?;
            let mask = self.store.mask(&mask_item)?;
            let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
            let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
            self.store.models(&step.models)?;
            stages::background::iqfm(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?
        } else {
            let field = self.store.real(&Item::FieldPpm)?;
            let mask = self.store.mask(&mask_item)?;
            self.store.models(&step.models)?;
            if let Some(d) = self.reconstruction_box(&mask) {
                self.decide(d);
            }
            // PDF's data weight is a noise map made from every echo's magnitude.
            let magnitudes = if stages::background::takes_echo_magnitudes(self.config) {
                self.echo_magnitude_stack()?
            } else {
                None
            };
            self.log(Level::Info, &format!("Background removal ({algorithm}){label}"));
            let local = stages::background::remove_background(&field, &mask, magnitudes.as_deref(), reliability.as_ref(), &self.meta, self.config, self.observer)?;
            if pass == Pass::Reliable && self.config.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
                // No eroded rim around the holes: LBV fills it (qsm-core `fill_eroded_rim`).
                let support = self.store.mask(&Item::LocalFieldMask(Pass::Main))?;
                let before = local.mask.iter().filter(|&&m| m != 0).count();
                let (local_field_ppm, filled) = stages::background::fill_eroded_rim(
                    &field, &mask, &local.local_field_ppm, &local.mask, &support, &self.meta)?;
                let after = filled.iter().filter(|&&m| m != 0).count();
                self.log(Level::Info, &format!(
                    "Two-pass (local): {} voxels of the rim background removal left around the holes filled by LBV",
                    after.saturating_sub(before)));
                stages::background::LocalField { local_field_ppm, mask: filled }
            } else {
                local
            }
        };
        self.put(Item::LocalField(pass), Data::Real(&local.local_field_ppm))?;
        self.put(Item::LocalFieldMask(pass), Data::Mask(&local.mask))?;
        let label = if iqfm { format!("Background removal (iQFM){label}") } else { format!("Background removal ({algorithm}){label}") };
        self.complete(Done {
            step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, &label)
    }

    /// Dipole inversion for one pass, of the local field background removal left — or, for the
    /// total-field methods (TFI, AutoQSM, NeXtQSM), which remove or model the
    /// background themselves, of the total field inside the pass's mask.
    fn inversion(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let name = step.stage.name();
        let label = pass_label(pass);
        // A two-pass run inverts each pass without HEIDI, and fills the combined map once
        // ([`Self::two_pass_combination`]).
        let config = stages::inversion::pass_config(self.config, self.plan.two_pass);
        let config: &PipelineConfig = &config;
        let skip_bgremove = crate::bridge::skips_bgremove(self.config);
        if skip_bgremove {
            self.log(Level::Info, "Skipping background removal (single-step inversion handles it internally)");
        }
        let algorithm = format!("{}", self.config.inversion.algorithm);
        let per_echo = self.per_echo();
        // A per-echo QSM inverts each echo as a single-echo scan's.
        if let Some(d) = medi_csf_decision(self.config, if per_echo { 1 } else { self.n_echoes() }) {
            self.decide(d);
        }
        let mut params = stages::inversion::params(config);
        // The echo time the field is converted to radians at, as the scan's echo times resolve it
        // (each echo's own, for a per-echo QSM).
        if per_echo {
            let tes: Vec<Option<f64>> = (1..=self.n_echoes())
                .map(|e| stages::inversion::resolved_te(self.config, &self.echo_meta(e))).collect();
            params["per_echo"] = json!(true);
            if tes.iter().any(Option::is_some) {
                params["te_s"] = json!(tes);
            }
        } else if let Some(te) = stages::inversion::resolved_te(self.config, &self.meta) {
            params["te_s"] = serde_json::json!(te);
        }
        if !per_echo {
            if let Some(te) = stages::inversion::resolved_heidi_te(config, &self.meta) {
                params["heidi_te_s"] = serde_json::json!(te);
            }
        }
        let Some(key) = self.begin(&name, Some(&algorithm), &params, "Skipping invert (cached)",
                                   &format!("Dipole inversion{label}")) else { return Ok(()) };
        if per_echo {
            // Read only by the total-field methods: the others each take their echo's own mask.
            let mask = if skip_bgremove { Some(self.store.mask(&self.pass_mask(pass))?) } else { None };
            self.store.models(&step.models)?;
            let n = self.n_echoes();
            let mut outputs = Vec::with_capacity(n);
            for e in 1..=n {
                // The total-field methods take the echo's field inside the mask, the others its
                // local field inside the mask background removal left.
                let (field, mask) = if let Some(mask) = &mask {
                    (self.store.real(&Item::EchoField(e))?, mask.clone())
                } else {
                    (self.store.real(&Item::EchoLocalField(e))?, self.store.mask(&Item::EchoLocalFieldMask(e))?)
                };
                let magnitude = match stages::inversion::magnitude_input(self.config) {
                    Some(_) => self.one_echo_magnitude(e)?,
                    None => None,
                };
                if e == 1 {
                    if let Some(d) = self.reconstruction_box(&mask) {
                        self.decide(d);
                    }
                }
                self.log(Level::Info, &format!("Dipole inversion ({algorithm}), echo {e} of {n}"));
                let chi = stages::inversion::invert(&field, &mask, magnitude.as_deref(), &self.echo_meta(e), config, self.observer)?;
                self.put(Item::EchoRawChi(e), Data::Real(&chi))?;
                outputs.push(Item::EchoRawChi(e));
            }
            return self.complete(Done {
                step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, outputs: &outputs,
            }, &format!("Dipole inversion ({algorithm}), each echo"));
        }
        let (field_item, mask_item) = if skip_bgremove {
            (Item::FieldPpm, self.pass_mask(pass))
        } else {
            (Item::LocalField(pass), Item::LocalFieldMask(pass))
        };
        let field = self.store.real(&field_item)?;
        let mask = self.store.mask(&mask_item)?;
        // The combined magnitude, or every echo's for MEDI and TFI.
        let magnitude = self.inversion_magnitude()?;
        self.store.models(&step.models)?;
        if let Some(d) = self.reconstruction_box(&mask) {
            self.decide(d);
        }
        self.log(Level::Info, &format!("Dipole inversion ({algorithm}){label}"));
        let chi = stages::inversion::invert(&field, &mask, magnitude.as_deref(), &self.meta, config, self.observer)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        self.complete(Done {
            step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key,
            outputs: &[Item::RawChi(pass)],
        }, &format!("Dipole inversion ({algorithm}){label}"))
    }

    /// iQSM / iQSM+: the raw map straight from every echo's wrapped phase.
    fn iqsm(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let name = step.stage.name();
        let label = pass_label(pass);
        let algorithm = stages::inversion::iqsm_algorithm(self.config).unwrap_or_default();
        let params = stages::inversion::iqsm_params(&self.meta);
        let Some(key) = self.begin(&name, Some(algorithm), &params, &format!("Skipping {algorithm} (cached)"),
                                   &format!("iQSM reconstruction{label}")) else { return Ok(()) };
        let (phases, magnitudes) = self.phase_echoes()?;
        let mask_item = self.pass_mask(pass);
        let mask = self.store.mask(&mask_item)?;
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
        self.store.models(&step.models)?;
        let chi = stages::inversion::iqsm(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        let net = if algorithm == "iqsm-plus" { "iQSM+" } else { "iQSM" };
        self.complete(Done {
            step: &name, algorithm: Some(algorithm), params: &params, outcome: None, key: &key,
            outputs: &[Item::RawChi(pass)],
        }, &format!("{net}{label}"))
    }

    /// TGV-QSM, from the total field (several echoes) or echo 1's wrapped phase (one).
    fn tgv(&mut self, pass: Pass) -> Result<(), S::Error> {
        use stages::inversion::TgvSource;
        let name = Stage::Tgv(pass).name();
        let label = pass_label(pass);
        let params = stages::inversion::tgv_params(self.config, &self.meta);
        let Some(key) = self.begin(&name, Some("tgv"), &params, "Skipping tgv (cached)",
                                   &format!("TGV-QSM reconstruction{label}")) else { return Ok(()) };
        let mask_item = self.pass_mask(pass);
        let mask = self.store.mask(&mask_item)?;
        let data = match stages::inversion::tgv_source(&self.meta) {
            TgvSource::FieldPpm => self.store.real(&Item::FieldPpm)?,
            TgvSource::WrappedPhase => { let i = self.echo_phase(1); self.store.real(&i)? }
        };
        let chi = stages::inversion::tgv(&data, &mask, &self.meta, self.config)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        self.complete(Done {
            step: &name, algorithm: Some("tgv"), params: &params, outcome: None, key: &key,
            outputs: &[Item::RawChi(pass)],
        }, &format!("TGV-QSM{label}"))
    }

    /// QSMART, from the total field, keeping its intermediates.
    fn qsmart(&mut self, pass: Pass) -> Result<(), S::Error> {
        let name = Stage::Qsmart(pass).name();
        let params = stages::qsmart::params(self.config);
        let Some(key) = self.begin(&name, Some("qsmart"), &params, "Skipping qsmart (cached)",
                                   "QSMART reconstruction") else { return Ok(()) };
        let field = self.store.real(&Item::FieldPpm)?;
        let mask_item = self.pass_mask(pass);
        let mask = self.store.mask(&mask_item)?;
        // QSMART's echo fit's reliability map, when the field came from it: both stages run
        // inside it.
        let reliability = if self.config.field_mapping.b0_estimation == crate::enums::B0Estimation::QsmartEchofit {
            Some(self.store.mask(&Item::FieldReliability)?)
        } else {
            None
        };
        // The magnitude vessels are detected on (and MEDI's edge weighting): every echo's, for
        // QSMART's own, or the combined one.
        let magnitude = match self.config.inversion.qsmart.vessel_magnitude {
            // The inhomogeneity correction made it, for both passes.
            crate::enums::QsmartVesselMagnitude::MeanN4 if self.plan.homogeneity.vessel => {
                let m = if self.store.has(&Item::VesselMagnitude) { Some(self.store.real(&Item::VesselMagnitude)?) } else { None };
                stages::qsmart::Magnitude::Vessel(m)
            }
            crate::enums::QsmartVesselMagnitude::MeanN4 => {
                // The mean is over every echo: a run with magnitude must have each echo's. A run
                // without any detects vessels on a uniform magnitude.
                let n = self.n_echoes();
                let items: Vec<Item> = (1..=n).map(|e| self.echo_magnitude(e)).collect();
                let missing: Vec<usize> = (1..=n).filter(|&e| !self.store.has(&items[e - 1])).collect();
                if !missing.is_empty() && missing.len() < n {
                    let list = missing.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                    return Err(crate::EngineError::Config(format!(
                        "qsmart: no magnitude for echo {list} of {n}: QSMART's vessel magnitude is the \
                         mean over every echo. Supply every echo's magnitude, or use \
                         --qsmart-vessel-magnitude combined")).into());
                }
                let mut echoes = Vec::with_capacity(n);
                for item in items.iter().filter(|_| missing.is_empty()) {
                    echoes.push(self.store.real(item)?);
                }
                stages::qsmart::Magnitude::Echoes(echoes)
            }
            crate::enums::QsmartVesselMagnitude::Combined => stages::qsmart::Magnitude::Combined(self.combined_magnitude()?),
        };
        let stages::qsmart::Qsmart { chi, intermediates } = stages::qsmart::qsmart(
            &field, &mask, reliability.as_deref(), magnitude, &self.meta, self.config, self.observer)?;
        drop((field, mask, reliability));
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        let mut outputs = vec![Item::RawChi(pass)];
        for (intermediate, volume) in &intermediates {
            let item = Item::QsmartIntermediate(pass, intermediate);
            self.put(item, match volume {
                VolumeData::Real(v) => Data::Real(v),
                VolumeData::Mask(m) => Data::Mask(m),
            })?;
            outputs.push(item);
        }
        self.complete(Done {
            step: &name, algorithm: Some("qsmart"), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, &format!("QSMART{}", pass_label(pass)))
    }

    /// The two passes' raw maps combined into one ([`stages::inversion::combine_two_pass`]), and
    /// HEIDI's cone fill on it when HEIDI is on ([`stages::inversion::heidi_cone_fill`], with the
    /// main pass's local field and mask).
    fn two_pass_combination(&mut self) -> Result<(), S::Error> {
        let support = self.support(Pass::Reliable);
        let mut params = json!({ "support": self.store.identity(&support) });
        if self.config.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
            params["merge"] = json!("second pass within 6 mm of its holes, feathered to 12 mm");
        }
        let heidi = stages::inversion::heidi_params(self.config);
        if let Some(h) = &heidi {
            params["heidi"] = h.clone();
            if let Some(te) = stages::inversion::resolved_heidi_te(self.config, &self.meta) {
                params["heidi"]["te_s"] = json!(te);
            }
        }
        let doing = if heidi.is_some() { "Combining two-pass reconstructions, then HEIDI" } else { "Combining two-pass reconstructions" };
        let Some(key) = self.begin("twopass", None, &params, "Skipping twopass (cached)", doing) else { return Ok(()) };
        let chi_reliable = self.store.real(&Item::RawChi(Pass::Reliable))?;
        let chi_main = self.store.real(&Item::RawChi(Pass::Main))?;
        let declared = self.store.mask(&support)?;
        let brain_item = self.pass_mask(Pass::Main);
        let brain = self.store.mask(&brain_item)?;
        let mut combined = if self.config.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
            // The second pass only near its holes, feathered into the first.
            let reliable_mask = self.store.mask(&Item::ReliableMask)?;
            let main_support_item = self.support(Pass::Main);
            let main_support = self.store.mask(&main_support_item)?;
            stages::inversion::combine_two_pass_local(&chi_reliable, &chi_main, &declared, &reliable_mask, &brain,
                                                      &main_support, &self.meta)?
        } else {
            stages::inversion::combine_two_pass(&chi_reliable, &chi_main, &declared, &brain)?
        };
        drop((chi_reliable, chi_main, declared, brain));
        if heidi.is_some() {
            let (field, mask) = (self.store.real(&Item::LocalField(Pass::Main))?, self.store.mask(&Item::LocalFieldMask(Pass::Main))?);
            self.log(Level::Info, "HEIDI on the combined two-pass map");
            combined = stages::inversion::heidi_cone_fill(&combined, &field, &mask, &self.meta, self.config, self.observer)?;
        }
        self.put(Item::CombinedRawChi, Data::Real(&combined))?;
        self.complete(Done {
            step: "twopass", algorithm: heidi.is_some().then_some("heidi"), params: &params, outcome: None, key: &key,
            outputs: &[Item::CombinedRawChi],
        }, if heidi.is_some() { "Two-pass combination and HEIDI" } else { "Two-pass combination" })
    }

    /// Reference a raw map over the voxels it is defined on ([`crate::referencing`]): the final
    /// map, or (`single_pass`) a two-pass run's single-pass map. The single-pass map's support is
    /// the main pass's; the combined map takes each voxel from one pass or the other, so its
    /// support is the union of both.
    fn reference(&mut self, single_pass: bool) -> Result<(), S::Error> {
        let stage = if single_pass { Stage::SinglePassReference } else { Stage::Reference };
        let name = stage.name();
        let method = format!("{}", self.config.qsm.reference);
        let segmentation = self.store.recorded_key("segmentation");
        let params = crate::referencing::params(self.config, segmentation.as_deref())?;
        let region = crate::referencing::reference_region(self.config)?;
        let Some(key) = self.begin(&name, Some(&method), &params, &format!("Skipping {name} (cached)"),
                                   "Referencing QSM") else { return Ok(()) };
        self.log(Level::Info, &format!("QSM referencing ({})", self.config.qsm.reference));
        let dseg = match region {
            Some((spec, _)) => {
                let Some(segmentation) = self.segmentation_item() else {
                    return Err(crate::EngineError::Config(format!(
                        "--qsm-reference {spec}: no segmentation to measure the region on. Enable \
                         --do-segmentation, or supply one with --use-custom-dseg")).into());
                };
                Some(self.map(&segmentation)?)
            }
            None => None,
        };
        if !single_pass && self.per_echo() {
            return self.reference_per_echo(&name, &method, &params, &key, dseg.as_deref());
        }
        let (raw, supports, out) = if single_pass {
            (Item::RawChi(Pass::Main), vec![self.support(Pass::Main)], Item::SinglePassChi)
        } else if self.plan.two_pass {
            (Item::CombinedRawChi, vec![self.support(Pass::Main), self.support(Pass::Reliable)], Item::Chi)
        } else {
            (Item::RawChi(Pass::Main), vec![self.support(Pass::Main)], Item::Chi)
        };
        let chi = self.store.real(&raw)?;
        let declared = supports.iter().map(|s| self.store.mask(s)).collect::<Result<Vec<_>, _>>()?;
        let declared_refs: Vec<&[u8]> = declared.iter().map(|m| m.as_slice()).collect();
        let crate::referencing::Referenced { chi, support, offset } =
            crate::referencing::reference_chi(self.config, &chi, &declared_refs, dseg.as_deref())?;
        self.put(out, Data::Real(&chi))?;
        let mut outputs = vec![out];
        if !single_pass {
            self.put(Item::ChiSupport, Data::Mask(&support))?;
            outputs.push(Item::ChiSupport);
        }
        // What the map was referenced to, recorded with the map itself: each map is referenced
        // over its own support, so each has its own offset.
        self.store.annotate(&out, crate::referencing::sidecar_fields(self.config, offset.as_ref()))?;
        self.complete(Done {
            step: &name, algorithm: Some(&method), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "QSM referencing")
    }

    /// A per-echo QSM's referencing: each echo's map over the voxels it is defined on (published
    /// with its echo time), and the QSM, the mean of the echoes' maps (qsm-core
    /// `per_echo::mean_across_echoes`, equal weights) where every echo's is defined, referenced
    /// as a QSM is. Referencing commutes with the mean over one support, so the QSM is also the
    /// mean of the referenced echoes' maps there.
    fn reference_per_echo(
        &mut self, name: &str, method: &str, params: &Value, key: &str, dseg: Option<&[f64]>,
    ) -> Result<(), S::Error> {
        let n = self.n_echoes();
        let skip_bgremove = crate::bridge::skips_bgremove(self.config);
        let mut raws = Vec::with_capacity(n);
        let mut common: Option<Vec<u8>> = None;
        let mut outputs = Vec::with_capacity(n + 2);
        for e in 1..=n {
            let raw = self.store.real(&Item::EchoRawChi(e))?;
            let declared_item = if skip_bgremove { self.pass_mask(Pass::Main) } else { Item::EchoLocalFieldMask(e) };
            let declared = self.store.mask(&declared_item)?;
            let crate::referencing::Referenced { chi, support, offset } =
                crate::referencing::reference_chi(self.config, &raw, &[&declared], dseg)?;
            self.put(Item::EchoChi(e), Data::Real(&chi))?;
            let mut fields = crate::referencing::sidecar_fields(self.config, offset.as_ref());
            fields.insert("EchoTime".into(), json!(self.meta.echo_times[e - 1]));
            self.store.annotate(&Item::EchoChi(e), fields)?;
            outputs.push(Item::EchoChi(e));
            common = Some(match common {
                None => support,
                Some(c) => c.iter().zip(&support).map(|(&a, &b)| u8::from(a != 0 && b != 0)).collect(),
            });
            raws.push(raw);
        }
        let common = common.unwrap_or_default();
        let mut mean = qsm_core::pipeline::per_echo::mean_across_echoes(&raws);
        drop(raws);
        for (m, &c) in mean.iter_mut().zip(&common) {
            if c == 0 {
                *m = 0.0;
            }
        }
        self.log(Level::Info, &format!("QSM: the mean of {n} echoes' maps"));
        let crate::referencing::Referenced { chi, support, offset } =
            crate::referencing::reference_chi(self.config, &mean, &[&common], dseg)?;
        self.put(Item::Chi, Data::Real(&chi))?;
        self.put(Item::ChiSupport, Data::Mask(&support))?;
        self.store.annotate(&Item::Chi, crate::referencing::sidecar_fields(self.config, offset.as_ref()))?;
        outputs.extend([Item::Chi, Item::ChiSupport]);
        self.complete(Done {
            step: name, algorithm: Some(method), params, outcome: None, key, outputs: &outputs,
        }, "QSM referencing")
    }

    /// CLEAR-SWI over every echo, and its minIP.
    fn swi(&mut self) -> Result<(), S::Error> {
        let params = stages::swi::swi_params(self.config, &self.meta)?;
        let Some(key) = self.begin("swi", Some("clear-swi"), &params, "Skipping swi (cached)", "Computing SWI") else { return Ok(()) };
        let n = self.n_echoes();
        self.log(Level::Info, &format!("Computing CLEAR-SWI ({} echo{})", n, if n == 1 { "" } else { "es" }));
        let phase_items: Vec<Item> = (1..=n).map(|e| self.echo_phase(e)).collect();
        let mag_items: Vec<Item> = (1..=n).map(|e| self.echo_magnitude(e)).collect();
        let phases = phase_items.iter().map(|i| self.store.real(i)).collect::<Result<Vec<_>, _>>()?;
        let magnitudes = mag_items.iter().map(|i| self.store.real(i)).collect::<Result<Vec<_>, _>>()?;
        let stages::swi::Swi { projected: swi, filtered_phase } =
            stages::swi::swi(&phases, &magnitudes, &self.meta, self.config)?;
        drop((phases, magnitudes));
        self.put(Item::Swi, Data::Real(&swi.image))?;
        self.put(Item::SwiMip, Data::OnGrid(&swi.mip, &swi.mip_geometry))?;
        let mut outputs = vec![Item::Swi, Item::SwiMip];
        if let Some(phase) = filtered_phase {
            self.put(Item::SwiPhase, Data::Real(&phase))?;
            self.store.annotate(&Item::SwiPhase, crate::bids::outputs::swi_phase_fields(n))?;
            outputs.push(Item::SwiPhase);
        }
        self.complete(Done {
            step: "swi", algorithm: Some("clear-swi"), params: &params, outcome: None, key: &key,
            outputs: &outputs,
        }, "SWI")
    }

    /// SMWI, weighted by the final, referenced susceptibility map over the voxels it is defined on
    /// (outside them χ is 0 by construction, not by measurement).
    fn smwi(&mut self) -> Result<(), S::Error> {
        let params = stages::swi::smwi_params(self.config, &self.meta)?;
        if !self.store.has(&Item::Chi) {
            self.log(Level::Warn, "Skipping SMWI: no susceptibility map");
            self.skip("There was no susceptibility map to weight by.");
            return Ok(());
        }
        let Some(key) = self.begin("smwi", Some("smwi"), &params, "Skipping smwi (cached)", "Computing SMWI") else { return Ok(()) };
        let s = &self.config.smwi;
        self.log(Level::Info, &format!("Computing SMWI (threshold {} ppm, power {}, mIP over {} slices)",
                                       s.threshold_ppm, s.power, s.mip_window));
        let mask_item = if self.store.has(&Item::ChiSupport) { Item::ChiSupport } else { self.mask_item() };
        let chi = self.store.real(&Item::Chi)?;
        let magnitude = self.store.real(&Item::CombinedMagnitude)?;
        let mask = self.store.mask(&mask_item)?;
        let weighted = stages::swi::smwi(&magnitude, &chi, &mask, &self.meta, self.config)?;
        drop((chi, magnitude, mask));
        let mut outputs = Vec::new();
        for (contrast, image) in &weighted {
            self.put(Item::Smwi(*contrast), Data::Real(&image.image))?;
            self.put(Item::SmwiMip(*contrast), Data::OnGrid(&image.mip, &image.mip_geometry))?;
            outputs.extend([Item::Smwi(*contrast), Item::SmwiMip(*contrast)]);
        }
        self.complete(Done {
            step: "smwi", algorithm: Some("smwi"), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "SMWI")
    }

    /// R2* by ARLO from every echo's magnitude, and T2* from it.
    fn relaxometry(&mut self) -> Result<(), S::Error> {
        let params = stages::relaxometry::t2star_r2star_params(&self.meta);
        let Some(key) = self.begin("t2star_r2star", Some("arlo"), &params, "Skipping t2star_r2star (cached)",
                                   "Computing R2*/T2* maps") else { return Ok(()) };
        let n = self.n_echoes();
        self.log(Level::Info, &format!("Computing R2*/T2* maps (ARLO, {n} echoes)"));
        let mask_item = self.mask_item();
        let mask = self.store.mask(&mask_item)?;
        let mut magnitudes = Vec::with_capacity(mask.len() * n);
        for e in 1..=n {
            let item = self.working_magnitude(e);
            let echo = self.store.real(&item)?;
            if echo.len() != mask.len() {
                return Err(crate::error::EngineError::Config(format!(
                    "mag echo {e} has {} voxels, but this run is processed on a grid of {}", echo.len(), mask.len())).into());
            }
            magnitudes.extend(echo);
        }
        let (r2star, t2star) = stages::relaxometry::t2star_r2star(&magnitudes, &mask, &self.meta)?;
        drop(magnitudes);
        self.put(Item::R2star, Data::Real(&r2star))?;
        self.put(Item::T2star, Data::Real(&t2star))?;
        self.complete(Done {
            step: "t2star_r2star", algorithm: Some("arlo"), params: &params, outcome: None, key: &key,
            outputs: &[Item::R2star, Item::T2star],
        }, "T2*/R2* mapping")
    }

    /// R2 from the MESE (or a supplied map) and R2′, measured as R2* − R2 where R2 was measured
    /// and otherwise, as the strategy allows, predicted from R2* by R2PRIMEnet (or supplied). An
    /// input that is not there skips its map with a warning rather than failing the run.
    fn r2(&mut self, step: &Step) -> Result<(), S::Error> {
        use crate::enums::R2PrimeStrategy;
        use crate::requirements::{r2primenet_field_message, r2primenet_field_policy, R2primenetFieldPolicy};
        let (want_r2, want_r2p) = (self.config.pipeline.do_r2map, self.config.pipeline.do_r2primemap);
        if !want_r2 && !want_r2p {
            return Ok(());
        }
        let mese = self.store.mese();
        let sep = &self.config.separation;
        let supplied: Vec<Item> = [(sep.custom_r2_tool.is_some(), Item::SuppliedR2),
                                   (sep.custom_r2prime_tool.is_some(), Item::SuppliedR2prime)]
            .into_iter().filter_map(|(read, item)| read.then_some(item)).collect();
        let params = self.with_supplied(stages::r2::params(self.config, &self.meta, mese.as_ref()), &supplied);
        let Some(key) = self.begin("r2_r2prime", Some("epg"), &params, "Skipping r2_r2prime (cached)",
                                   "Computing R2/R2' maps") else { return Ok(()) };
        let config = self.config;
        let sep = &config.separation;
        let mask_item = self.mask_item();
        let mask = self.store.mask(&mask_item)?;
        let mut outputs = Vec::new();
        // The mask R2 was actually measured over. Narrower than the brain mask when the MESE's FOV
        // does not reach all of it; `None` means R2 came from somewhere other than a MESE.
        let mut r2_mask: Option<Vec<u8>> = None;

        // ── R2 map (Hz) ──
        let r2: Option<Vec<f64>> = if let Some(tool) = &sep.custom_r2_tool {
            if self.store.has(&Item::SuppliedR2) {
                let from = self.store.identity(&Item::SuppliedR2);
                self.log(Level::Info, &format!("Using custom R2 map from {}", from.as_str().unwrap_or_default()));
                self.decide(Decision::new(Topic::R2Source, Choice::Supplied, "The R2 map was the one supplied with the scan.")
                    .with_detail(from.as_str().unwrap_or_default()));
                Some(self.map(&Item::SuppliedR2)?)
            } else {
                self.log(Level::Warn, &format!("no custom R2 map under derivatives (tool: {tool}) — skipping R2"));
                self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                    "No supplied R2 map was found, so there is no R2 map.").with_detail(tool.clone()));
                None
            }
        } else if let Some(mese) = &mese {
            if let Some(why) = crate::requirements::mese_problem(self.facts) {
                // Refused up front when R2 was asked for; here only when something else implied it.
                self.log(Level::Warn, &format!("Not measuring R2 from the MESE ({why})"));
                self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                    "R2 was not measured: the multi-echo spin-echo acquisition cannot be used.").with_detail(why));
                None
            } else {
                self.log(Level::Info, "Computing R2 (EPG) from MESE");
                match self.mese_on_grid(mese)? {
                    Ok(stages::r2::MeseOnGrid { echoes: se, coverage }) => {
                        // R2 is only measured where the MESE's FOV reaches. Fitting outside it
                        // would put a number on interpolated zeros, and R2' = R2* - R2 would then
                        // quietly take that number as a measurement.
                        let fit_mask = stages::r2::mese_measured_mask(&mask, &coverage);
                        let (in_mask, in_both) = (stages::r2::count_set(&mask), stages::r2::count_set(&fit_mask));
                        if in_mask > 0 && in_both < in_mask {
                            self.log(Level::Warn, &format!(
                                "The MESE covers {:.0}% of the brain mask; R2 (and so a measured R2') \
                                 is left at zero over the remaining {} voxels.",
                                100.0 * in_both as f64 / in_mask as f64, in_mask - in_both));
                            self.decide(Decision::new(Topic::R2Coverage, Choice::Partial,
                                "R2 was measured only where the multi-echo spin-echo acquisition covers the brain mask, \
                                 and left at zero elsewhere.")
                                .with_detail(format!("{:.0}% of the brain mask", 100.0 * in_both as f64 / in_mask as f64)));
                        }
                        self.decide(Decision::new(Topic::R2Source, Choice::Measured,
                            "R2 was fitted from the multi-echo spin-echo acquisition.").with_detail(mese.name.clone()));
                        let r2 = stages::r2::r2_epg(&se, &fit_mask, mese, &self.meta)?;
                        r2_mask = Some(fit_mask);
                        Some(r2)
                    }
                    Err(why) => {
                        self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                            "R2 was not measured: the multi-echo spin-echo acquisition cannot be used.").with_detail(why));
                        None
                    }
                }
            }
        } else {
            self.log(Level::Info, "No MESE acquisition and no custom R2 map — R2/R2' unavailable for this run");
            self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                "There is no R2 map: the scan has no multi-echo spin-echo acquisition and none was supplied."));
            None
        };
        if let Some(r2) = &r2 {
            self.put(Item::R2, Data::Real(r2))?;
            outputs.push(Item::R2);
        }

        // ── R2' map (Hz) = R2* − R2 ──
        if want_r2p {
            // Where this R2' came from, recorded with it (#236): a predicted R2' looks exactly like
            // a measured one, and only that record can tell a downstream reader which it is.
            let mut source = serde_json::Map::new();
            let r2p: Option<Vec<f64>> = if let Some(tool) = &sep.custom_r2prime_tool {
                if self.store.has(&Item::SuppliedR2prime) {
                    let from = self.store.identity(&Item::SuppliedR2prime);
                    let from = from.as_str().unwrap_or_default();
                    self.log(Level::Info, &format!("Using custom R2' map from {from}"));
                    self.decide(Decision::new(Topic::R2primeSource, Choice::Supplied,
                        "The R2′ map was the one supplied with the scan.").with_detail(from));
                    source.insert("R2primeSource".into(), json!("supplied"));
                    source.insert("R2primeSuppliedFrom".into(), json!(from));
                    Some(self.map(&Item::SuppliedR2prime)?)
                } else {
                    self.log(Level::Warn, &format!("no custom R2' map under derivatives (tool: {tool}) — skipping R2'"));
                    self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                        "No supplied R2′ map was found, so there is no R2′ map.").with_detail(tool.clone()));
                    None
                }
            } else {
                let strategy = sep.r2prime_strategy;
                let has_r2star = self.store.has(&Item::R2star);
                // Measuring wins wherever it is possible and allowed: R2' = R2* - R2 is a
                // measurement, and R2PRIMEnet's is an estimate standing in for one.
                if let (Some(r2), true) = (&r2, strategy != R2PrimeStrategy::R2primenet) {
                    if has_r2star {
                        self.log(Level::Info, "R2' measured as R2* - R2");
                        let r2star = self.store.real(&Item::R2star)?;
                        // Subtract only where R2 was measured: outside the MESE's FOV, R2 is zero
                        // because nothing was acquired there, and R2' = R2* - 0 would report the
                        // whole of R2* as reversible.
                        let (r2p, coverage) = stages::r2::measured_r2prime(&r2star, r2, &mask, r2_mask.as_deref())?;
                        // Kept, so a 0 in the R2' map can be told apart from R2* = R2.
                        self.put(Item::R2primeCoverage, Data::Mask(&coverage))?;
                        outputs.push(Item::R2primeCoverage);
                        let (in_mask, measured) = (stages::r2::count_set(&mask), stages::r2::count_set(&coverage));
                        if in_mask > 0 && measured < in_mask {
                            self.log(Level::Warn, &format!(
                                "R2' is a measurement over {:.0}% of the brain mask; the remaining {} \
                                 voxels are left at zero because no R2 was measured there. Maps \
                                 derived from this R2' inherit that gap.",
                                100.0 * measured as f64 / in_mask as f64, in_mask - measured));
                            self.decide(Decision::new(Topic::R2primeCoverage, Choice::Partial,
                                "R2′ was measured only where R2 was, and left at zero elsewhere.")
                                .with_detail(format!("{:.0}% of the brain mask", 100.0 * measured as f64 / in_mask as f64)));
                        }
                        self.decide(Decision::new(Topic::R2primeSource, Choice::Measured, "R2′ was computed as R2* − R2."));
                        source.insert("R2primeSource".into(), json!("measured"));
                        source.insert("R2primeMethod".into(), json!(match &sep.custom_r2_tool {
                            Some(tool) => format!("R2* - R2 (R2 supplied by {tool})"),
                            None => "R2* - R2 (R2 by EPG fit of the MESE)".to_string(),
                        }));
                        Some(r2p)
                    } else {
                        self.log(Level::Warn, "R2' needs both R2* and R2 - one is missing; skipping R2'");
                        self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                            "There is no R2′ map: R2* was not computed to subtract R2 from."));
                        None
                    }
                } else if strategy == R2PrimeStrategy::Mese {
                    self.log(Level::Warn,
                        "Skipping R2': no R2 to subtract (no MESE acquisition and no custom R2 map), \
                         and --r2prime-strategy mese does not estimate one. Use `auto` or \
                         `r2primenet` to predict R2' from R2* instead.");
                    self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                        "There is no R2′ map: there was no R2 to subtract, and R2′ was only to be measured."));
                    None
                } else if !has_r2star {
                    self.log(Level::Warn, "Skipping R2': R2PRIMEnet predicts it from R2*, which was not computed");
                    self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                        "There is no R2′ map: R2* was not computed to predict it from."));
                    None
                } else {
                    // Auto with no R2 to subtract, or r2primenet outright.
                    let field = self.meta.field_strength;
                    match r2primenet_field_policy(field, strategy, sep.r2primenet.ignore_field_mismatch) {
                        R2primenetFieldPolicy::Skip => {
                            self.log(Level::Warn, &format!(
                                "Skipping R2': there is no R2 to subtract, and {}. Pass \
                                 --r2primenet-ignore-field-mismatch to predict it anyway (uncalibrated).",
                                r2primenet_field_message(field)));
                            self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                                "There is no R2′ map: there was no R2 to subtract, and R2PRIMEnet is not run \
                                 off the field strength it was trained at.").with_detail(format!("{field} T")));
                            None
                        }
                        R2primenetFieldPolicy::Refuse => {
                            return Err(crate::EngineError::Config(format!(
                                "--r2prime-strategy r2primenet: {}; or pass \
                                 --r2primenet-ignore-field-mismatch to run it anyway and treat the R2' \
                                 (and any chi-separation built on it) as uncalibrated.",
                                r2primenet_field_message(field))).into());
                        }
                        policy => {
                            if policy == R2primenetFieldPolicy::RunUncalibrated {
                                self.log(Level::Warn, &format!(
                                    "{}. Running anyway (--r2primenet-ignore-field-mismatch): treat this \
                                     R2', and any chi-separation built on it, as uncalibrated.",
                                    r2primenet_field_message(field)));
                                self.decide(Decision::new(Topic::R2primenetCalibration, Choice::Uncalibrated,
                                    "R2PRIMEnet was run off the field strength it was trained at, so its R2′ is uncalibrated.")
                                    .with_detail(format!("{field} T")));
                            }
                            if strategy == R2PrimeStrategy::Auto {
                                self.log(Level::Info, "No R2 to subtract - estimating R2' from R2* with R2PRIMEnet");
                            }
                            if let Some(tilt) = stages::r2::r2primenet_grid_tilt(self.meta.source_geometry.is_some(), &self.meta.affine) {
                                self.log(Level::Warn, &format!(
                                    "R2PRIMEnet assumes B0 lies along +z, but this run is reconstructing on its \
                                     acquired grid with B0 {tilt:.1}° from the slice normal, so its R2' is \
                                     off-distribution. The run was not resampled because a measured R2 was \
                                     expected and did not materialise. Pass --axial-resampling to resample, \
                                     or --r2prime-strategy mese to not estimate R2' at all."));
                                self.decide(Decision::new(Topic::R2primenetGrid, Choice::OffDistribution,
                                    "R2PRIMEnet ran on a grid whose B0 is not along the slice normal, so its R2′ is \
                                     off its training distribution.").with_detail(format!("{tilt:.1}°")));
                            }
                            self.store.models(&step.models)?;
                            let r2star = self.store.real(&Item::R2star)?;
                            let predicted = stages::r2::r2primenet(&r2star, &mask, &self.meta, &sep.r2primenet, self.observer)?;
                            self.log(Level::Info, "R2' estimated by R2PRIMEnet (an estimate, not a measurement)");
                            self.decide(Decision::new(Topic::R2primeSource, Choice::Predicted,
                                "R2′ was estimated from R2* by R2PRIMEnet."));
                            source.insert("R2primeSource".into(), json!("predicted"));
                            source.insert("R2primeMethod".into(), json!("R2PRIMEnet (from R2*)"));
                            source.insert("R2primenetTrainedFieldStrength".into(),
                                          json!(qsm_core::relaxometry::TRAINED_B0_T));
                            source.insert("R2primenetCalibrated".into(), json!(policy == R2primenetFieldPolicy::Run));
                            // χ-sepnet's toolbox route makes its own, in its training frame.
                            if crate::config::chisepnet_route_can_estimate_r2prime(config) {
                                source.insert("R2primeUsedByChiSeparation".into(), json!(false));
                            }
                            Some(predicted)
                        }
                    }
                }
            };
            if let Some(r2p) = &r2p {
                self.put(Item::R2prime, Data::Real(r2p))?;
                self.store.annotate(&Item::R2prime, source)?;
                outputs.push(Item::R2prime);
            }
        }

        if outputs.is_empty() {
            return Ok(());
        }
        self.complete(Done {
            step: "r2_r2prime", algorithm: Some("epg"), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "R2/R2' mapping")
    }

    /// χ-separation of the susceptibility map (the run's, or one supplied) into its paramagnetic
    /// and diamagnetic sources, from what the earlier steps made (local field, R2*, R2′, the
    /// echoes' magnitude, the MESE). Fitted over, and 0 outside, χ's support
    /// ([`stages::separation::separation_support`]). Only ever asked for, never implied, so an
    /// input it lacks fails the run: `qsmxt run` has already refused the cases the scan decides,
    /// and what is left (the local field the reconstruction route did not write, an R2′ that did
    /// not materialise) must not be a successful run with the maps missing.
    fn chi_separation(&mut self, step: &Step) -> Result<(), S::Error> {
        use stages::separation as sep;
        let alg = self.config.separation.algorithm;
        let alg_name = format!("{alg}");
        // Every method needs a conventional QSM (χ_total): the run's, a supplied one, or (χ-sep
        // iLSQR) one this step reconstructs from the local field.
        let own_qsm = sep::reconstructs_qsm(self.config);
        let qsm_item = match &self.config.separation.custom_qsm_tool {
            None if own_qsm => {
                self.decide(Decision::new(Topic::QsmSource, Choice::Computed,
                    "The susceptibility map separated was reconstructed from the local field by iLSQR, \
                     as the χ-separation toolbox does."));
                None
            }
            Some(tool) => {
                if !self.store.has(&Item::SuppliedChi) {
                    return Err(crate::EngineError::Config(format!(
                        "--do-chisep ({alg_name}) needs a QSM, and --use-custom-qsm {tool} names no Chimap \
                         for this run under derivatives/{tool}")).into());
                }
                let from = self.store.identity(&Item::SuppliedChi);
                self.log(Level::Info, &format!("Using custom QSM from {}", from.as_str().unwrap_or_default()));
                self.decide(Decision::new(Topic::QsmSource, Choice::Supplied,
                    "The susceptibility map separated was the one supplied with the scan.")
                    .with_detail(from.as_str().unwrap_or_default()));
                Some(Item::SuppliedChi)
            }
            None => {
                if !self.store.has(&Item::Chi) {
                    return Err(crate::EngineError::Config(format!(
                        "--do-chisep ({alg_name}) needs a QSM, and none was made")).into());
                }
                Some(Item::Chi)
            }
        };
        let mese = self.store.mese();
        let s = &self.config.separation;
        let supplied: Vec<Item> = [(s.custom_qsm_tool.is_some(), Item::SuppliedChi),
                                   (s.custom_r2prime_tool.is_some(), Item::SuppliedR2prime)]
            .into_iter().filter_map(|(read, item)| read.then_some(item)).collect();
        if let Some(d) = chi_bound_decision(self.config, self.meta.field_strength, &self.meta.echo_times) {
            self.decide(d);
        }
        let route_estimates_r2prime = self.chisepnet_route_estimates_r2prime();
        self.decide_chisepnet_r2prime(route_estimates_r2prime);
        let params = self.with_supplied(sep::params(self.config, &self.meta, mese.is_some()), &supplied);
        let Some(key) = self.begin("chi_separation", Some(&alg_name), &params, "Skipping chi_separation (cached)",
                                   "Chi-separation") else { return Ok(()) };
        self.log(Level::Info, &format!("Chi-separation ({alg_name})"));
        let (nx, ny, nz) = self.meta.dims;
        let n_voxels = nx * ny * nz;
        let mask_item = self.mask_item();
        let brain = self.store.mask(&mask_item)?;

        // Multi-echo magnitude (voxel-major) and its RSS over echoes, when there is magnitude;
        // χ-sep MEDI also takes the echoes one volume after another, for its noise map and CSF mask.
        let chi_sep_medi = alg == crate::SeparationAlgorithm::ChiSepMedi;
        let mut echo_major = Vec::new();
        let (magnitude_multi, magnitude_rss) = if self.meta.has_magnitude {
            let n = self.n_echoes();
            let mut multi = stages::relaxometry::VoxelMajor::new(n_voxels, n);
            let mut rss = vec![0.0f64; n_voxels];
            for e in 1..=n {
                let item = self.working_magnitude(e);
                let echo = self.store.real(&item)?;
                multi.set(e - 1, &echo, &format!("mag echo {e}"))?;
                if chi_sep_medi {
                    echo_major.extend_from_slice(&echo);
                }
                for (r, m) in rss.iter_mut().zip(&echo) {
                    *r += m * m;
                }
            }
            for v in rss.iter_mut() { *v = v.sqrt(); }
            (Some(multi.data), Some(rss))
        } else {
            (None, None)
        };
        // R2* and R2′, from the relaxometry and R2 steps, or a supplied R2′.
        let mut r2star = if self.store.has(&Item::R2star) { Some(self.store.real(&Item::R2star)?) } else { None };
        let r2prime_item = if self.config.separation.custom_r2prime_tool.is_some() { Item::SuppliedR2prime } else { Item::R2prime };
        let mut r2prime = if self.store.has(&r2prime_item) { Some(self.map(&r2prime_item)?) } else { None };
        // χ-sepnet's toolbox route predicts R2' from R2* itself, with R2PRIMEnet in its training
        // frame, as the authors do; it takes an R2' only when that is measured or supplied.
        if route_estimates_r2prime {
            if r2star.is_some() {
                self.log(Level::Info, "Chi-separation (chi-sepnet): R2' predicted from R2* inside the network's route");
            }
            r2prime = None;
        }
        // The MESE's echoes on this run's grid, for HC-ChiSep. Voxels outside its FOV are zero,
        // the same as voxels outside the brain.
        let se_multi = match &mese {
            Some(m) => self.mese_on_grid(m)?.ok().map(|m| m.echoes),
            None => None,
        };
        // The local field (ppm), for the field-based methods.
        let field_item = Item::LocalField(Pass::Main);
        let mut local_field = if self.store.has(&field_item) { self.store.real(&field_item)? } else { Vec::new() };
        let mut qsm = match &qsm_item {
            Some(item) => self.map(item)?,
            None if local_field.is_empty() => Vec::new(),
            None => {
                let field_mask = self.store.mask(&Item::LocalFieldMask(Pass::Main))?;
                self.log(Level::Info, "Chi-separation: reconstructing its QSM from the local field (iLSQR)");
                sep::initial_qsm(&local_field, &field_mask, &self.meta, self.config, self.observer)?
            }
        };

        // χ-sep MEDI's noise map and CSF mask are made below, on the support, from the magnitude:
        // here they count as present when the magnitude is.
        let makes_medi_inputs = chi_sep_medi && !echo_major.is_empty();
        let (no_map, no_mask): (&[f64], &[u8]) = (&[], &[]);
        let missing = sep::missing_inputs(alg, &qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &local_field, qsm: &qsm, qsm_per_echo: None, mask: &brain, r2prime: r2prime.as_deref(),
            r2star: r2star.as_deref(), magnitude_rss: magnitude_rss.as_deref(),
            magnitude_multi: magnitude_multi.as_deref(), se_magnitude_multi: se_multi.as_deref(),
            n_std: makes_medi_inputs.then_some(no_map), mask_csf: makes_medi_inputs.then_some(no_mask),
        });
        if !missing.is_empty() {
            return Err(crate::EngineError::Config(format!(
                "--do-chisep ({alg_name}) cannot run: missing {}", missing.join(", "))).into());
        }

        // Fit, and write, only where χ is defined. The run's χ is 0 on the rim background-field
        // removal eroded; fitting the brain mask would treat that rim as measured χ = 0. Its
        // support is the one referencing kept; a supplied χ has none, so it is the brain mask less
        // the voxels that χ leaves undefined. The field-based methods also need the local field
        // there, so they are further limited to where it exists.
        let published = if qsm_item == Some(Item::Chi) && self.store.has(&Item::ChiSupport) {
            Some(self.store.mask(&Item::ChiSupport)?)
        } else {
            None
        };
        let uses_field = sep::uses_field(alg);
        for (what, len) in [("brain mask", brain.len()), ("QSM", qsm.len())]
            .into_iter()
            .chain(published.as_ref().map(|p| ("QSM support", p.len())))
            .chain(uses_field.then_some(("local field", local_field.len())))
            .chain(r2prime.as_ref().map(|m| ("a relaxation map", m.len())))
            .chain(r2star.as_ref().map(|m| ("a relaxation map", m.len())))
        {
            if len != n_voxels {
                return Err(crate::EngineError::DimensionMismatch(format!(
                    "chi-separation: {what} has {len} voxels but this run is processed on {n_voxels}")).into());
            }
        }
        let support = sep::separation_support(&brain, published.as_deref(), &qsm, uses_field.then_some(local_field.as_slice()));
        self.log(Level::Info, &format!("Chi-separation over the χ support: {} of {} brain-mask voxels",
            stages::r2::count_set(&support), stages::r2::count_set(&brain)));
        // The maps combined with χ voxel by voxel take the same support, so no method — the
        // spatial and deep-learning ones included — sees values on the rim that χ does not have.
        // The χ-separation toolbox's methods take R2′ and R2* as its script passes them, unmasked
        // ([`sep::takes_relaxation_unmasked`]); what they write is still 0 outside the support.
        sep::zero_outside(&support, &mut qsm);
        if uses_field {
            sep::zero_outside(&support, &mut local_field);
        }
        for map in [r2prime.as_mut(), r2star.as_mut()].into_iter().flatten() {
            sep::restrict_relaxation(self.config, &support, map);
        }
        // DECOMPOSE takes a QSM per echo (its authors run STAR-QSM on each echo): a per-echo QSM's
        // echoes' maps, or else the one QSM given at every echo (said in the methods text).
        let qsm_per_echo = if alg != crate::SeparationAlgorithm::Decompose {
            None
        } else if qsm_item == Some(Item::Chi) && self.per_echo() {
            let mut maps = Vec::with_capacity(self.n_echoes());
            for e in 1..=self.n_echoes() {
                let mut map = self.store.real(&Item::EchoChi(e))?;
                sep::zero_outside(&support, &mut map);
                maps.push(map);
            }
            Some(qsm_core::pipeline::per_echo::voxel_major(&maps))
        } else {
            Some(sep::repeat_per_echo(&qsm, self.n_echoes()))
        };
        let (n_std, mask_csf) = if chi_sep_medi {
            let (n_std, csf) = sep::chi_sep_medi_inputs(
                &echo_major, &self.meta.echo_times, &support, self.meta.dims, self.meta.voxel_size,
                self.config.separation.chi_sep_medi.csf_r2star)?;
            self.log(Level::Info, &format!("Chi-separation (MEDI): CSF mask of {} voxels from {} R2*",
                stages::r2::count_set(&csf), self.config.separation.chi_sep_medi.csf_r2star.label()));
            (Some(n_std), Some(csf))
        } else {
            (None, None)
        };
        let inputs = qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &local_field, qsm: &qsm, qsm_per_echo: qsm_per_echo.as_deref(),
            mask: &support, r2prime: r2prime.as_deref(),
            r2star: r2star.as_deref(), magnitude_rss: magnitude_rss.as_deref(),
            magnitude_multi: magnitude_multi.as_deref(), se_magnitude_multi: se_multi.as_deref(),
            n_std: n_std.as_deref(), mask_csf: mask_csf.as_deref(),
        };
        self.store.models(&step.models)?;
        let separated = sep::separate(inputs, mese.as_ref().map(|m| m.echo_times.as_slice()), &self.meta,
                                      self.config, self.observer)?;
        self.put(Item::SeparatedChi(Contrast::Paramagnetic), Data::Real(&separated.paramagnetic))?;
        self.put(Item::SeparatedChi(Contrast::Diamagnetic), Data::Real(&separated.diamagnetic))?;
        self.put(Item::SeparatedTotalChi, Data::Real(&separated.total))?;
        // What χ-sepnet's toolbox route used, recorded with what it made: R2′ is no output of it.
        if crate::config::chisepnet_route_can_estimate_r2prime(self.config) {
            let mut used = serde_json::Map::new();
            used.insert("R2primeSource".into(), json!(if route_estimates_r2prime { "predicted" } else { "measured" }));
            used.insert("R2primeMethod".into(), json!(if route_estimates_r2prime {
                "R2PRIMEnet (from R2*, within chi-separation)"
            } else {
                "R2* - R2"
            }));
            for item in [Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                         Item::SeparatedTotalChi] {
                self.store.annotate(&item, used.clone())?;
            }
        }
        self.complete(Done {
            step: "chi_separation", algorithm: Some(&alg_name), params: &params, outcome: None, key: &key,
            outputs: &[Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                       Item::SeparatedTotalChi],
        }, &format!("Chi-separation ({alg_name})"))
    }

    /// Whether χ-sepnet's toolbox route makes its own R2′ on this run: it has none measured to take.
    fn chisepnet_route_estimates_r2prime(&mut self) -> bool {
        crate::config::chisepnet_route_can_estimate_r2prime(self.config)
            && !(self.config.separation.r2prime_strategy != crate::R2PrimeStrategy::R2primenet
                && self.store.has(&Item::R2) && self.store.has(&Item::R2prime))
    }

    /// Where χ-sepnet's toolbox route got its R2′ on this run.
    fn decide_chisepnet_r2prime(&mut self, estimates: bool) {
        use crate::requirements::{r2primenet_field_message, r2primenet_field_policy, R2primenetFieldPolicy};
        if !crate::config::chisepnet_route_can_estimate_r2prime(self.config) {
            return;
        }
        if !estimates {
            self.decide(Decision::new(Topic::ChiSeparationR2prime, Choice::Measured,
                "χ-separation used the R2′ measured as R2* − R2."));
            return;
        }
        self.decide(Decision::new(Topic::ChiSeparationR2prime, Choice::Predicted,
            "χ-separation used its own R2′, estimated from R2* by R2PRIMEnet in the network's training frame."));
        let sep = &self.config.separation;
        let field = self.meta.field_strength;
        if r2primenet_field_policy(field, sep.r2prime_strategy, sep.r2primenet.ignore_field_mismatch)
            == R2primenetFieldPolicy::RunUncalibrated
        {
            self.log(Level::Warn, &format!(
                "{}. Running anyway (--r2primenet-ignore-field-mismatch): treat this chi-separation, built on \
                 its R2', as uncalibrated.", r2primenet_field_message(field)));
            self.decide(Decision::new(Topic::R2primenetCalibration, Choice::Uncalibrated,
                "R2PRIMEnet was run off the field strength it was trained at, so its R2′ is uncalibrated.")
                .with_detail(format!("{field} T")));
        }
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
