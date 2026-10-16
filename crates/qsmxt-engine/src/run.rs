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
/// What the plan says the data cannot produce ([`Plan::check`]) is the front end's to refuse
/// before calling this: `qsmxt` checks every run before the first starts.
///
/// Returns what the run did ([`RunRecord`]): every step of the plan, computed, reused or skipped,
/// with what was decided in it.
pub fn run<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, store: &mut S, observer: &mut dyn Observer,
) -> Result<RunRecord, S::Error> {
    // Every front end's config is held to the settings' ranges (qsmxt checks them earlier too).
    crate::schema::check_ranges(config)?;
    let (meta, load_record) = load(config, facts, store, observer)?;
    store.begin(&meta)?;
    let mut record = RunRecord { steps: vec![load_record], resampled: Some(meta.source_geometry.is_some()) };
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

/// Run one of `plan`'s engine steps on its own, on a run whose grid (from its load step) is
/// `meta`: what a front end re-running a single step uses (a mask preview, say), and tests.
/// Returns what the step did.
pub fn run_step<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, meta: &RunMetadata, step: &Step, store: &mut S,
    observer: &mut dyn Observer,
) -> Result<StepRecord, S::Error> {
    Run::new(plan, config, facts, meta.clone(), store, observer).take(step)
}

/// The grid the run is processed on: the cached load step's, or decided from the first echo's
/// header ([`crate::scan::resolve_geometry`]) and recorded; and what the load step did.
fn load<S: StageStore + ?Sized>(
    config: &PipelineConfig, facts: &ScanFacts, store: &mut S, observer: &mut dyn Observer,
) -> Result<(RunMetadata, StepRecord), S::Error> {
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
        "axial_only": crate::bridge::axial_only_algorithms(
            config, crate::scan::orientation_facts(facts, config)),
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
        return Ok((meta, record));
    }
    observer.stage("load", None, StageEvent::Started("Loading NIfTI metadata"));
    store.start("load");
    let header = store.header()?;
    let mut meta = crate::scan::resolve_geometry(header.dims, header.voxel_size, header.affine, facts, config);
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
    Ok((meta, record))
}

/// Every echo's phase, and every echo's magnitude (or none).
type Echoes = (Vec<Vec<f64>>, Vec<Vec<f64>>);

/// One run of a plan, between its steps.
struct Run<'r, S: StageStore + ?Sized> {
    plan: &'r Plan,
    config: &'r PipelineConfig,
    facts: &'r ScanFacts,
    meta: RunMetadata,
    store: &'r mut S,
    observer: &'r mut dyn Observer,
    /// What became of the step being taken: `None` until it is computed or found cached, and
    /// skipped if neither.
    state: Option<StepState>,
    /// What it decided so far.
    decisions: Vec<Decision>,
    /// Where the decisions made while computing it start: those are kept with it in the store,
    /// for a run that reuses it. The ones before are made every run.
    computing_from: usize,
}

impl<'r, S: StageStore + ?Sized> Run<'r, S> {
    fn new(
        plan: &'r Plan, config: &'r PipelineConfig, facts: &'r ScanFacts, meta: RunMetadata, store: &'r mut S,
        observer: &'r mut dyn Observer,
    ) -> Self {
        Run { plan, config, facts, meta, store, observer, state: None, decisions: Vec::new(), computing_from: 0 }
    }

    /// Take `step`, and say what it did.
    fn take(&mut self, step: &Step) -> Result<StepRecord, S::Error> {
        (self.state, self.computing_from) = (None, 0);
        self.decisions.clear();
        self.step(step)?;
        Ok(StepRecord {
            id: step.name.clone(),
            label: step.label.clone(),
            state: self.state.take().unwrap_or(StepState::Skipped),
            decisions: std::mem::take(&mut self.decisions),
        })
    }

    /// Note a decision of the step being taken.
    fn decide(&mut self, decision: Decision) {
        self.decisions.push(decision);
    }

    /// Note that the step did nothing, and why.
    fn skip(&mut self, reason: &str) {
        self.decide(Decision::new(Topic::Step, Choice::Skipped, reason));
    }

    /// The step's earlier output is reused: what it decided when it was computed holds.
    fn reuse(&mut self, step: &str) {
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

    /// The key `step` is cached under, unless its earlier output is still valid (then `None`, with
    /// `skipping` logged). Otherwise the step starts, `doing` being what it does.
    fn begin(&mut self, step: &str, algorithm: Option<&str>, params: &Value, skipping: &str, doing: &str) -> Option<String> {
        let key = step_cache_key(step, algorithm, params, self.config);
        if self.store.cached(step, &key) {
            self.observer.log(Level::Info, skipping);
            self.observer.stage(step, algorithm, StageEvent::Cached);
            self.reuse(step);
            return None;
        }
        self.observer.stage(step, algorithm, StageEvent::Started(doing));
        self.store.start(step);
        self.computing_from = self.decisions.len();
        Some(key)
    }

    /// Keep an output, and show it. A volume on a grid of its own (a projection) is kept but not
    /// shown: the observer's volumes are on the run's grid.
    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), S::Error> {
        self.store.put(item, data)?;
        match data {
            Data::Real(v) => self.observer.output(&item, qsm_core::pipeline::VolumeRef::Real(v)),
            Data::Mask(m) => self.observer.output(&item, qsm_core::pipeline::VolumeRef::Mask(m)),
            Data::OnGrid(..) | Data::Json(_) | Data::Text(_) | Data::Same(_) => {}
        }
        Ok(())
    }

    /// Record a computed step, with what it decided while computed; `label` names it in the log.
    fn complete(&mut self, c: Completed<'_>, label: &str) -> Result<(), S::Error> {
        let (step, algorithm) = (c.step, c.algorithm);
        self.record(c)?;
        self.observer.stage(step, algorithm, StageEvent::Finished(label));
        Ok(())
    }

    /// Keep a computed step in the store, with what it decided while computed.
    fn record(&mut self, c: Completed<'_>) -> Result<(), S::Error> {
        let decisions = self.decisions[self.computing_from.min(self.decisions.len())..].to_vec();
        self.store.record(&Completed { decisions: &decisions, ..c })?;
        self.state = Some(StepState::Computed);
        Ok(())
    }

    fn log(&mut self, level: Level, message: &str) {
        self.observer.log(level, message);
    }

    /// The box the FFT-based stages reconstruct in around `mask` ([`stages::crop::reconstruction_box`]),
    /// when the run crops or pads.
    fn reconstruction_box(&self, mask: &[u8]) -> Option<Decision> {
        let p = &self.config.pipeline;
        if !p.crop_to_mask && !p.fft_padding {
            return None;
        }
        let b = stages::crop::reconstruction_box(p.crop_to_mask, p.fft_padding, mask, self.meta.dims,
                                                 self.meta.voxel_size, p.crop_margin_mm);
        let (d, f) = (b.dims, b.full_dims);
        let size = format!("{}x{}x{} voxels, of {}x{}x{}", d.0, d.1, d.2, f.0, f.1, f.2);
        Some(if d == f {
            Decision::new(Topic::ReconstructionBox, Choice::Full,
                          "Reconstructed on the whole grid, which the box would not have changed.")
        } else if p.crop_to_mask {
            Decision::new(Topic::ReconstructionBox, Choice::Cropped, "Reconstructed in a box around the mask.")
        } else {
            Decision::new(Topic::ReconstructionBox, Choice::Padded,
                          "Reconstructed on the grid zero-padded to an FFT-efficient size.")
        }.with_detail(size))
    }

    fn n_echoes(&self) -> usize {
        self.meta.n_echoes
    }

    /// The mask the stages after motion correction work in: the brain mask, or with motion
    /// correction the part of it every echo still covers.
    fn mask_item(&self) -> Item {
        if self.plan.motion_corrected { Item::CoverageMask } else { Item::BrainMask }
    }

    /// The mask a reconstruction pass runs on.
    fn pass_mask(&self, pass: Pass) -> Item {
        match pass {
            Pass::Main => self.mask_item(),
            Pass::Reliable => Item::ReliableMask,
        }
    }

    /// Where a pass's susceptibility map is defined: the mask background removal left, or the
    /// pass's mask when the inversion did its own background removal.
    fn support(&self, pass: Pass) -> Item {
        if crate::bridge::skips_bgremove(self.config) { self.pass_mask(pass) } else { Item::LocalFieldMask(pass) }
    }

    /// Echo `e`'s phase as the stages after motion correction read it.
    fn echo_phase(&self, e: usize) -> Item {
        if self.plan.motion_corrected { Item::CorrectedPhase(e) } else { Item::Phase(e) }
    }

    fn echo_magnitude(&self, e: usize) -> Item {
        if self.plan.motion_corrected { Item::CorrectedMagnitude(e) } else { Item::Magnitude(e) }
    }

    /// Every echo's phase and, when there are any, magnitude, as the stages after motion
    /// correction read them: what the phase-domain networks (iQSM, iQFM) take.
    fn phase_echoes(&mut self) -> Result<Echoes, S::Error> {
        let n = self.n_echoes();
        let phases = (1..=n).map(|e| { let i = self.echo_phase(e); self.store.real(&i) }).collect::<Result<Vec<_>, _>>()?;
        let mut magnitudes = Vec::new();
        for e in 1..=n {
            let item = self.echo_magnitude(e);
            if self.store.has(&item) {
                magnitudes.push(self.store.real(&item)?);
            }
        }
        Ok((phases, magnitudes))
    }

    /// Echo `e`'s magnitude as the stages after motion correction read it, falling back to the
    /// acquired echo when there is no working-grid copy (only for a run that was not resampled,
    /// where the two are the same grid).
    fn working_magnitude(&mut self, e: usize) -> Item {
        let item = self.echo_magnitude(e);
        if !self.store.has(&item) && self.store.has(&Item::AcquiredMagnitude(e)) { Item::AcquiredMagnitude(e) } else { item }
    }

    /// The MESE's echoes, voxel-major on the working grid, and where its FOV reaches
    /// ([`stages::r2::mese_on_grid`]); why not, without a usable MESE.
    fn mese_on_grid(&mut self, mese: &crate::store::Mese) -> Result<Result<stages::r2::MeseOnGrid, String>, S::Error> {
        let store = &mut *self.store;
        Ok(stages::r2::mese_on_grid(mese, |n| store.supplied(&Item::MeseMagnitude(n)), self.meta.dims, &self.meta.affine)?)
    }

    /// The segmentation regions are measured on: the run's own, or the one supplied with the scan
    /// (`--use-custom-dseg` without segmenting).
    fn segmentation_item(&mut self) -> Option<Item> {
        [Item::Segmentation, Item::SuppliedSegmentation].into_iter().find(|i| self.store.has(i))
    }

    fn combined_magnitude(&mut self) -> Result<Option<Vec<f64>>, S::Error> {
        if self.store.has(&Item::CombinedMagnitude) { self.store.real(&Item::CombinedMagnitude).map(Some) } else { Ok(None) }
    }

    /// Every echo's phase in radians, on the working grid. Each echo is read, brought onto the
    /// working grid and kept before the next is read.
    fn scale_phase(&mut self) -> Result<(), S::Error> {
        let params = stages::phase::params(&self.meta, None);
        let resampled = self.meta.source_geometry.is_some();
        let doing = if resampled { "Resampling to axial + rescaling phase" } else { "Rescaling phase to radians" };
        let Some(key) = self.begin("scale_phase", None, &params, "Skipping scale_phase (cached)", doing) else { return Ok(()) };
        let n = self.n_echoes();
        let (mut phases, mut magnitudes) = (Vec::new(), Vec::new());
        for e in 1..=n {
            let phase = self.store.real(&Item::AcquiredPhase(e))?;
            // Only resampling reads the magnitude: otherwise it is kept as it is.
            let magnitude = if resampled && self.store.has(&Item::AcquiredMagnitude(e)) {
                Some(self.store.real(&Item::AcquiredMagnitude(e))?)
            } else {
                None
            };
            let out = stages::phase::working_echo(phase, magnitude, false, &self.meta);
            self.put(Item::Phase(e), Data::Real(&out.phase))?;
            phases.push(Item::Phase(e));
            // A resampled run keeps the magnitude it resampled with the phase.
            if let (Some(mag), true) = (&out.magnitude, resampled && self.meta.has_magnitude) {
                self.put(Item::Magnitude(e), Data::Real(mag))?;
                magnitudes.push(Item::Magnitude(e));
            }
        }
        // Raw (uncorrected) per-echo magnitudes as they are, for MCPC-3D-S, linear fit and ROMEO.
        if !resampled && self.meta.has_magnitude {
            for e in 1..=n {
                if self.store.has(&Item::AcquiredMagnitude(e)) {
                    self.put(Item::Magnitude(e), Data::Same(Item::AcquiredMagnitude(e)))?;
                    magnitudes.push(Item::Magnitude(e));
                }
            }
        }
        let inputs: Vec<Item> = (1..=n).map(Item::AcquiredPhase).collect();
        let outputs: Vec<Item> = phases.into_iter().chain(magnitudes).collect();
        self.complete(Completed {
            step: "scale_phase", algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &outputs,
        }, if resampled { "Resample + rescale phase" } else { "Rescale phase" })
    }

    /// `scale_phase` for a scan given as real and imaginary parts: they become magnitude and
    /// phase ([`stages::phase::real_imaginary`]) before anything else sees them, and each converted
    /// echo (already in radians) is brought onto the working grid as for any other scan. The sign
    /// convention follows from the scanner's manufacturer, so it is a parameter (and keys the
    /// cache).
    fn scale_real_imaginary(&mut self) -> Result<(), S::Error> {
        let manufacturer = self.store.sidecar()?.and_then(|s| s.manufacturer);
        let negate_phase = stages::phase::negates_real_imaginary_phase(manufacturer.as_deref());
        let params = stages::phase::params(&self.meta, Some(negate_phase));
        let Some(key) = self.begin("scale_phase", None, &params, "Skipping scale_phase (cached)",
                                   "Converting real/imaginary to magnitude/phase") else { return Ok(()) };
        let n = self.n_echoes();
        let acquired = stages::phase::acquired_grid(&self.meta).n_total();
        let (mut real, mut imaginary, mut inputs) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::new());
        for e in 1..=n {
            for (item, out) in [(Item::AcquiredPhase(e), &mut real), (Item::AcquiredImaginary(e), &mut imaginary)] {
                if !self.store.has(&item) {
                    return Err(crate::EngineError::Config(format!(
                        "echo {e}: real/imaginary run without a part-imag file")).into());
                }
                let v = self.store.real(&item)?;
                if v.len() != acquired {
                    let what = self.store.identity(&item);
                    return Err(crate::EngineError::DimensionMismatch(format!(
                        "{} has {} voxels, expected {acquired}", what.as_str().unwrap_or_default(), v.len())).into());
                }
                out.push(v);
                inputs.push(item);
            }
        }
        let (converted, lines) = stages::phase::real_imaginary(&real, &imaginary, &self.meta, negate_phase);
        drop((real, imaginary));
        for (level, line) in lines {
            self.log(level, &line);
        }
        if negate_phase {
            self.decide(Decision::new(Topic::PhaseSign, Choice::Negated,
                "The phase from the real and imaginary parts was negated, as the scanner's manufacturer \
                 records it with the opposite sign.").with_detail(manufacturer.clone().unwrap_or_default()));
        }
        let chopped: Vec<&str> = ["x", "y", "z"].into_iter().zip(converted.chopping.corrected)
            .filter(|(_, on)| *on).map(|(a, _)| a).collect();
        if !chopped.is_empty() {
            self.decide(Decision::new(Topic::SliceChopping, Choice::Corrected,
                "Phase alternating by π between neighbouring slices was corrected.")
                .with_detail(format!("along {}", chopped.join(", "))));
        }
        let resampled = self.meta.source_geometry.is_some();
        let outcome = Some(("real_imaginary_conversion", stages::phase::slice_chopping_json(&converted.chopping)));
        let (mut phases, mut magnitudes) = (Vec::new(), Vec::new());
        for (i, (phase, mag)) in converted.phases.into_iter().zip(converted.magnitudes).enumerate() {
            let out = stages::phase::working_echo(phase, Some(mag), true, &self.meta);
            self.put(Item::Phase(i + 1), Data::Real(&out.phase))?;
            phases.push(Item::Phase(i + 1));
            // The magnitude it converted, resampled with the phase when the run is resampled.
            if let (Some(mag), true) = (&out.magnitude, !resampled || self.meta.has_magnitude) {
                self.put(Item::Magnitude(i + 1), Data::Real(mag))?;
                magnitudes.push(Item::Magnitude(i + 1));
            }
        }
        let outputs: Vec<Item> = phases.into_iter().chain(magnitudes).collect();
        self.complete(Completed {
            step: "scale_phase", algorithm: None, params: &params, outcome, key: &key, decisions: &[],
            inputs: &inputs, outputs: &outputs,
        }, if resampled { "Resample + rescale phase" } else { "Rescale phase" })
    }

    /// `scale_phase` for uncombined coils: every coil's echoes combined by MCPC-3D-S
    /// ([`stages::phase::combine_coils`]) into the per-echo phase (radians, coil offsets removed)
    /// and magnitude every later step reads, which are also kept as outputs of their own.
    fn combine_coils(&mut self) -> Result<(), S::Error> {
        let coils = self.store.coils();
        let n = self.n_echoes();
        // MCPC-3D-S estimates each coil's phase offset from the difference of two echoes.
        // `qsmxt run` refuses such a run up front; this keeps a direct caller from QSM.rs's assertion.
        if n < 2 {
            return Err(crate::EngineError::Config(format!(
                "MCPC-3D-S coil combination needs at least 2 echoes, and this run has {n} echo with {} \
                 uncombined coils. Combine the coils first (on the scanner, or with another tool).",
                coils.len())).into());
        }
        let params = stages::phase::coil_params(self.config, &self.meta, &coils);
        let Some(key) = self.begin("scale_phase", Some("mcpc3ds"), &params, "Skipping coil combination (cached)",
                                   "MCPC-3D-S coil combination") else { return Ok(()) };
        self.log(Level::Info, &format!("Coil combination (MCPC-3D-S): {} coils x {n} echoes", coils.len()));
        let acquired = stages::phase::acquired_grid(&self.meta).n_total();
        let (mut phases, mut magnitudes, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
        for (c, number) in coils.iter().enumerate().map(|(i, &number)| (i + 1, number)) {
            let (mut cp, mut cm) = (Vec::with_capacity(n), Vec::with_capacity(n));
            for e in 1..=n {
                let (phase_item, mag_item) = (Item::CoilPhase(c, e), Item::CoilMagnitude(c, e));
                let mut p = self.store.real(&phase_item)?;
                if p.len() != acquired {
                    let what = self.store.identity(&phase_item);
                    return Err(crate::EngineError::DimensionMismatch(format!(
                        "{} has {} voxels, expected {acquired}", what.as_str().unwrap_or_default(), p.len())).into());
                }
                qsm_core::pipeline::scale_phase_to_pi(&mut p);
                if !self.store.has(&mag_item) {
                    return Err(crate::EngineError::Config(format!(
                        "coil {number} echo {e}: magnitude required for MCPC-3D-S")).into());
                }
                let m = self.store.real(&mag_item)?;
                if m.len() != acquired {
                    let what = self.store.identity(&mag_item);
                    return Err(crate::EngineError::DimensionMismatch(format!(
                        "{} has {} voxels, expected {acquired}", what.as_str().unwrap_or_default(), m.len())).into());
                }
                inputs.extend([phase_item, mag_item]);
                cp.push(p);
                cm.push(m);
            }
            phases.push(cp);
            magnitudes.push(cm);
        }
        let combined = stages::phase::combine_coils(&phases, &magnitudes, &self.meta, self.config)?;
        drop((phases, magnitudes));
        let mut outputs = Vec::new();
        for e in 1..=n {
            let echo = stages::phase::combined_echo(&combined.magnitudes[e - 1], &combined.phases[e - 1], &self.meta);
            let magnitude = echo.magnitude.unwrap_or_default();
            self.put(Item::Phase(e), Data::Real(&echo.phase))?;
            self.put(Item::Magnitude(e), Data::Real(&magnitude))?;
            self.put(Item::CombinedEchoPhase(e), Data::Real(&echo.phase))?;
            self.put(Item::CombinedEchoMagnitude(e), Data::Real(&magnitude))?;
            outputs.extend([Item::Phase(e), Item::Magnitude(e), Item::CombinedEchoPhase(e), Item::CombinedEchoMagnitude(e)]);
        }
        let mask = stages::phase::combination_mask(&combined.mask, &self.meta);
        self.put(Item::CoilCombinationMask, Data::Mask(&mask))?;
        outputs.push(Item::CoilCombinationMask);
        self.complete(Completed {
            step: "scale_phase", algorithm: Some("mcpc3ds"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &outputs,
        }, "Coil combination")
    }

    fn magnitude(&mut self) -> Result<(), S::Error> {
        let params = stages::magnitude::params(self.config);
        let doing = if self.config.masking.inhomogeneity_correction {
            "Computing RSS-combined magnitude with inhomogeneity correction"
        } else {
            "Computing RSS-combined magnitude"
        };
        let Some(key) = self.begin("magnitude", Some("rss"), &params, "Skipping magnitude (cached)", doing) else { return Ok(()) };
        let n = self.n_echoes();
        let mut echoes = Vec::with_capacity(n);
        for e in 1..=n {
            if self.store.has(&Item::Magnitude(e)) {
                echoes.push(self.store.real(&Item::Magnitude(e))?);
            } else if self.store.has(&Item::AcquiredMagnitude(e)) {
                echoes.push(self.store.real(&Item::AcquiredMagnitude(e))?);
            }
        }
        if !echoes.is_empty() {
            let refs: Vec<&[f64]> = echoes.iter().map(|v| v.as_slice()).collect();
            let combined = stages::magnitude::combined_magnitude(&refs, &self.meta, self.config)?;
            drop(echoes);
            self.put(Item::CombinedMagnitude, Data::Real(&combined))?;
            let inputs: Vec<Item> = (1..=n).map(Item::Magnitude).collect();
            self.record(Completed {
                step: "magnitude", algorithm: Some("rss"), params: &params, outcome: None, key: &key, decisions: &[],
                inputs: &inputs, outputs: &[Item::CombinedMagnitude],
            })?;
        } else {
            self.skip("There was no magnitude to combine.");
        }
        self.observer.stage("magnitude", Some("rss"), StageEvent::Finished("RSS magnitude"));
        Ok(())
    }

    /// Build a mask from `sections`, with the weights of the deep-learning `models` it runs ready
    /// first: every echo's phase, and every echo's magnitude (none for a phase-only run).
    fn build_mask(&mut self, sections: &[crate::masking::MaskSection], models: &[&'static str]) -> Result<Vec<u8>, S::Error> {
        let n = self.n_echoes();
        let mut phases = Vec::with_capacity(n);
        for e in 1..=n {
            if self.store.has(&Item::Phase(e)) {
                phases.push(self.store.real(&Item::Phase(e))?);
            }
        }
        let mut magnitudes = Vec::with_capacity(n);
        for e in 1..=n {
            if !self.store.has(&Item::Magnitude(e)) {
                magnitudes.clear();
                break;
            }
            magnitudes.push(self.store.real(&Item::Magnitude(e))?);
        }
        self.store.models(models)?;
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        Ok(stages::mask::build_mask(sections, self.config, &phase_refs, magnitudes, &self.meta)?)
    }

    fn mask(&mut self, step: &Step) -> Result<(), S::Error> {
        // The plan leaves two-pass out when a mask is supplied ([`crate::plan::plan`]).
        if self.config.pipeline.do_qsm && self.config.masking.two_pass && !self.plan.two_pass {
            self.decide(Decision::new(Topic::TwoPass, Choice::NotApplied,
                "Two-pass artefact reduction was not applied: the mask was supplied, so there was no recipe to \
                 derive a reliable-phase mask from."));
        }
        let params = stages::mask::params(self.config);
        let Some(key) = self.begin("mask", None, &params, "Skipping mask (cached)", "Creating mask") else { return Ok(()) };
        // A supplied mask, when one is asked for and found. One that cannot be used falls back to
        // the recipe, whose models the plan did not list.
        let mut models = step.models.clone();
        if let Some(tool) = self.config.masking.custom_mask_tool.clone() {
            match self.store.supplied_mask() {
                Supplied::Found { origin, data, dims, affine } => {
                    if let Some(mask) = stages::mask::conform_custom_mask(&data, dims, &affine, &self.meta) {
                        self.log(Level::Info, &format!("Using custom mask from {origin}"));
                        self.decide(Decision::new(Topic::MaskSource, Choice::Supplied,
                            "The brain mask was the one supplied with the scan.").with_detail(origin));
                        self.put(Item::BrainMask, Data::Mask(&mask))?;
                        return self.complete(Completed {
                            step: "mask", algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
                            inputs: &[Item::SuppliedMask], outputs: &[Item::BrainMask],
                        }, "Mask (custom)");
                    }
                    self.log(Level::Warn, &format!(
                        "custom mask {origin} could not be conformed to the working grid — computing instead"));
                    self.decide(Decision::new(Topic::MaskSource, Choice::Computed,
                        "The supplied mask could not be brought onto the working grid, so the mask was computed.")
                        .with_detail(origin));
                    models = stages::mask::model_ids(&self.config.masking.sections, self.config);
                }
                Supplied::Unreadable { origin, error } => {
                    self.log(Level::Warn, &format!("could not read custom mask {origin} ({error}) — computing instead"));
                    self.decide(Decision::new(Topic::MaskSource, Choice::Computed,
                        "The supplied mask could not be read, so the mask was computed.")
                        .with_detail(format!("{origin}: {error}")));
                    models = stages::mask::model_ids(&self.config.masking.sections, self.config);
                }
                Supplied::Missing { searched } => {
                    self.log(Level::Info, &format!(
                        "no custom mask under derivatives (tool: {tool}) for {searched} — computing"));
                    self.decide(Decision::new(Topic::MaskSource, Choice::Computed,
                        "No supplied mask was found, so the mask was computed.").with_detail(searched));
                }
            }
        }
        self.log(Level::Info, &format!("Creating mask ({} section(s), combined with {})",
            self.config.masking.sections.len(), self.config.masking.combine));
        let mask = self.build_mask(&self.config.masking.sections.clone(), &models)?;
        self.put(Item::BrainMask, Data::Mask(&mask))?;
        let n = self.n_echoes();
        let inputs: Vec<Item> = (1..=n).map(Item::Magnitude).filter(|i| self.store.has(i)).collect();
        self.complete(Completed {
            step: "mask", algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &[Item::BrainMask],
        }, "Mask creation")
    }

    /// The reliable-phase mask of a two-pass run, restricted to the main mask.
    fn reliable_mask(&mut self, step: &Step) -> Result<(), S::Error> {
        let sections = self.config.masking.resolved_two_pass_sections();
        let params = stages::mask::reliable_params(self.config);
        let name = Stage::Mask(Pass::Reliable).name();
        let Some(key) = self.begin(&name, None, &params, "Skipping reliable-phase mask (cached)",
                                   "Creating the reliable-phase mask") else { return Ok(()) };
        self.log(Level::Info, &format!("Creating the reliable-phase mask ({} section(s))", sections.len()));
        let reliable = self.build_mask(&sections, &step.models)?;
        let main_item = self.mask_item();
        let main = self.store.mask(&main_item)?;
        let restricted = stages::mask::reliable_mask(&reliable, &main)?;
        self.put(Item::ReliableMask, Data::Mask(&restricted))?;
        self.complete(Completed {
            step: &name, algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[main_item], outputs: &[Item::ReliableMask],
        }, "Reliable-phase mask")
    }

    /// The total field, computed once on the run's mask and shared by both passes of a two-pass
    /// run: the reliable mask is a subset of it.
    fn field_map(&mut self) -> Result<(), S::Error> {
        let algorithm = stages::field::algorithm(self.config, &self.meta);
        let params = stages::field::params(self.config, &self.meta);
        let Some(key) = self.begin("unwrap", Some(&algorithm), &params, "Skipping unwrap (cached)",
                                   "Phase unwrapping / echo combination") else { return Ok(()) };
        let n = self.n_echoes();
        let phase_items: Vec<Item> = (1..=n).map(|e| self.echo_phase(e)).collect();
        let phases = phase_items.iter().map(|i| self.store.real(i)).collect::<Result<Vec<_>, _>>()?;
        let mask_item = self.mask_item();
        let mask = self.store.mask(&mask_item)?;
        let mut magnitudes = Vec::with_capacity(n);
        for e in 1..=n {
            let item = self.echo_magnitude(e);
            if self.store.has(&item) {
                magnitudes.push(self.store.real(&item)?);
            }
        }
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
        let field = stages::field::map_field(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?;
        drop(phases);

        self.put(Item::FieldPpm, Data::Real(&field.b0_field_ppm))?;
        self.put(Item::TotalFieldHz, Data::Real(&field.total_field_hz))?;
        let mut outputs = vec![Item::FieldPpm, Item::TotalFieldHz];
        // A published field map is paired with a magnitude for anatomical reference: the first
        // echo's, the one with the most signal.
        if let Some(first) = magnitudes.first() {
            self.put(Item::TotalFieldMagnitude, Data::Real(first))?;
            outputs.push(Item::TotalFieldMagnitude);
        }
        if let Some(report) = &field.robust_fit_report {
            self.put(Item::RobustFitReport, Data::Json(report))?;
            outputs.push(Item::RobustFitReport);
        }
        let inputs: Vec<Item> = phase_items.into_iter().chain(std::iter::once(mask_item)).collect();
        self.complete(Completed {
            step: "unwrap", algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &outputs,
        }, "Phase unwrapping")
    }

    /// Background-field removal for one pass, from the total field — or, for iQFM, straight from
    /// the echoes' wrapped phase (joint unwrapping and removal).
    fn background_removal(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let name = step.stage.name();
        let label = pass_label(pass);
        let iqfm = self.config.bg_removal.algorithm == crate::enums::BfAlgorithm::Iqfm;
        let algorithm = format!("{}", self.config.bg_removal.algorithm);
        let params = stages::background::params(self.config);
        let (skipping, doing) = if iqfm {
            ("Skipping iQFM field preparation (cached)", format!("iQFM field preparation{label}"))
        } else {
            ("Skipping bgremove (cached)", format!("Background field removal{label}"))
        };
        let Some(key) = self.begin(&name, Some(&algorithm), &params, skipping, &doing) else { return Ok(()) };
        let mask_item = self.pass_mask(pass);
        let outputs = [Item::LocalField(pass), Item::LocalFieldMask(pass)];
        let (local, inputs) = if iqfm {
            let (phases, magnitudes) = self.phase_echoes()?;
            let mask = self.store.mask(&mask_item)?;
            let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
            let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
            self.store.models(&step.models)?;
            let local = stages::background::iqfm(&phase_refs, &mag_refs, &mask, &self.meta)?;
            let inputs: Vec<Item> = (1..=self.n_echoes()).map(|e| self.echo_phase(e))
                .chain(std::iter::once(mask_item)).collect();
            (local, inputs)
        } else {
            let field = self.store.real(&Item::FieldPpm)?;
            let mask = self.store.mask(&mask_item)?;
            self.store.models(&step.models)?;
            if let Some(d) = self.reconstruction_box(&mask) {
                self.decide(d);
            }
            self.log(Level::Info, &format!("Background removal ({algorithm}){label}"));
            let local = stages::background::remove_background(&field, &mask, &self.meta, self.config, self.observer)?;
            (local, vec![Item::FieldPpm, mask_item])
        };
        self.put(Item::LocalField(pass), Data::Real(&local.local_field_ppm))?;
        self.put(Item::LocalFieldMask(pass), Data::Mask(&local.mask))?;
        let label = if iqfm { format!("Background removal (iQFM){label}") } else { format!("Background removal ({algorithm}){label}") };
        self.complete(Completed {
            step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &outputs,
        }, &label)
    }

    /// Dipole inversion for one pass, of the local field background removal left — or, for the
    /// total-field methods (TFI, MEDI+SMV, AutoQSM, NeXtQSM), which remove or model the
    /// background themselves, of the total field inside the pass's mask.
    fn inversion(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let name = step.stage.name();
        let label = pass_label(pass);
        let skip_bgremove = crate::bridge::skips_bgremove(self.config);
        if skip_bgremove {
            self.log(Level::Info, "Skipping background removal (single-step inversion handles it internally)");
        }
        let algorithm = format!("{}", self.config.inversion.algorithm);
        let params = stages::inversion::params(self.config);
        let Some(key) = self.begin(&name, Some(&algorithm), &params, "Skipping invert (cached)",
                                   &format!("Dipole inversion{label}")) else { return Ok(()) };
        let (field_item, mask_item) = if skip_bgremove {
            (Item::FieldPpm, self.pass_mask(pass))
        } else {
            (Item::LocalField(pass), Item::LocalFieldMask(pass))
        };
        let field = self.store.real(&field_item)?;
        let mask = self.store.mask(&mask_item)?;
        // The combined magnitude, for MEDI's edge weighting.
        let magnitude = self.combined_magnitude()?;
        self.store.models(&step.models)?;
        if let Some(d) = self.reconstruction_box(&mask) {
            self.decide(d);
        }
        self.log(Level::Info, &format!("Dipole inversion ({algorithm}){label}"));
        let chi = stages::inversion::invert(&field, &mask, magnitude.as_deref(), &self.meta, self.config, self.observer)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        self.complete(Completed {
            step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[field_item, mask_item], outputs: &[Item::RawChi(pass)],
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
        let inputs: Vec<Item> = (1..=self.n_echoes()).map(|e| self.echo_phase(e))
            .chain(std::iter::once(mask_item)).collect();
        let net = if algorithm == "iqsm-plus" { "iQSM+" } else { "iQSM" };
        self.complete(Completed {
            step: &name, algorithm: Some(algorithm), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &[Item::RawChi(pass)],
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
        self.complete(Completed {
            step: &name, algorithm: Some("tgv"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[mask_item, Item::FieldPpm], outputs: &[Item::RawChi(pass)],
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
        // The combined magnitude drives vasculature detection (and MEDI's edge weighting).
        let magnitude = self.combined_magnitude()?;
        let stages::qsmart::Qsmart { chi, intermediates } = stages::qsmart::qsmart(
            &field, &mask, magnitude.as_deref(), &self.meta, self.config, self.observer)?;
        drop((field, mask, magnitude));
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
        self.complete(Completed {
            step: &name, algorithm: Some("qsmart"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[mask_item, Item::FieldPpm], outputs: &outputs,
        }, &format!("QSMART{}", pass_label(pass)))
    }

    /// The two passes' raw maps combined into one ([`stages::inversion::combine_two_pass`]).
    fn two_pass_combination(&mut self) -> Result<(), S::Error> {
        let support = self.support(Pass::Reliable);
        let params = json!({ "support": self.store.identity(&support) });
        let Some(key) = self.begin("twopass", None, &params, "Skipping twopass (cached)",
                                   "Combining two-pass reconstructions") else { return Ok(()) };
        let chi_reliable = self.store.real(&Item::RawChi(Pass::Reliable))?;
        let chi_main = self.store.real(&Item::RawChi(Pass::Main))?;
        let declared = self.store.mask(&support)?;
        let brain_item = self.pass_mask(Pass::Main);
        let brain = self.store.mask(&brain_item)?;
        let combined = stages::inversion::combine_two_pass(&chi_reliable, &chi_main, &declared, &brain)?;
        self.put(Item::CombinedRawChi, Data::Real(&combined))?;
        self.complete(Completed {
            step: "twopass", algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[Item::RawChi(Pass::Reliable), Item::RawChi(Pass::Main), support],
            outputs: &[Item::CombinedRawChi],
        }, "Two-pass combination")
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
        let mut inputs: Vec<Item> = std::iter::once(raw).chain(supports.iter().copied()).collect();
        let dseg = match region {
            Some((spec, _)) => {
                let Some(segmentation) = self.segmentation_item() else {
                    return Err(crate::EngineError::Config(format!(
                        "--qsm-reference {spec}: no segmentation to measure the region on. Enable \
                         --do-segmentation, or supply one with --use-custom-dseg")).into());
                };
                inputs.push(segmentation);
                Some(self.store.real(&segmentation)?)
            }
            None => None,
        };
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
        self.complete(Completed {
            step: &name, algorithm: Some(&method), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &outputs,
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
        let swi = stages::swi::swi(&phases, &magnitudes, &self.meta, self.config)?;
        drop((phases, magnitudes));
        self.put(Item::Swi, Data::Real(&swi.image))?;
        self.put(Item::SwiMip, Data::OnGrid(&swi.mip, &swi.mip_geometry))?;
        let inputs: Vec<Item> = phase_items.into_iter().chain(mag_items).collect();
        self.complete(Completed {
            step: "swi", algorithm: Some("clear-swi"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &[Item::Swi, Item::SwiMip],
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
        self.complete(Completed {
            step: "smwi", algorithm: Some("smwi"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[Item::Chi, Item::CombinedMagnitude, mask_item], outputs: &outputs,
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
        let (nx, ny, nz) = self.meta.dims;
        let mut magnitudes = stages::relaxometry::VoxelMajor::new(nx * ny * nz, n);
        for e in 1..=n {
            let item = self.working_magnitude(e);
            let echo = self.store.real(&item)?;
            magnitudes.set(e - 1, &echo, &format!("mag echo {e}"))?;
        }
        let (r2star, t2star) = stages::relaxometry::t2star_r2star(&magnitudes.data, &mask, &self.meta)?;
        drop(magnitudes);
        self.put(Item::R2star, Data::Real(&r2star))?;
        self.put(Item::T2star, Data::Real(&t2star))?;
        self.complete(Completed {
            step: "t2star_r2star", algorithm: Some("arlo"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[mask_item], outputs: &[Item::R2star, Item::T2star],
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
        let params = stages::r2::params(self.config, &self.meta, mese.as_ref());
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
                Some(self.store.real(&Item::SuppliedR2)?)
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
                    Some(self.store.real(&Item::SuppliedR2prime)?)
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
                                          json!(crate::requirements::R2PRIMENET_TRAINED_B0_T));
                            source.insert("R2primenetCalibrated".into(), json!(policy == R2primenetFieldPolicy::Run));
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
        self.complete(Completed {
            step: "r2_r2prime", algorithm: Some("epg"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[mask_item], outputs: &outputs,
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
        // Every method needs a conventional QSM (χ_total): the run's, or a supplied one.
        let qsm_item = match &self.config.separation.custom_qsm_tool {
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
                Item::SuppliedChi
            }
            None => {
                if !self.store.has(&Item::Chi) {
                    return Err(crate::EngineError::Config(format!(
                        "--do-chisep ({alg_name}) needs a QSM, and none was made")).into());
                }
                Item::Chi
            }
        };
        let mese = self.store.mese();
        let params = sep::params(self.config, &self.meta, mese.is_some());
        let Some(key) = self.begin("chi_separation", Some(&alg_name), &params, "Skipping chi_separation (cached)",
                                   "Chi-separation") else { return Ok(()) };
        self.log(Level::Info, &format!("Chi-separation ({alg_name})"));
        let (nx, ny, nz) = self.meta.dims;
        let n_voxels = nx * ny * nz;
        let mask_item = self.mask_item();
        let brain = self.store.mask(&mask_item)?;
        let mut qsm = self.store.real(&qsm_item)?;

        // Multi-echo magnitude (voxel-major) and its RSS over echoes, when there is magnitude.
        let (magnitude_multi, magnitude_rss) = if self.meta.has_magnitude {
            let n = self.n_echoes();
            let mut multi = stages::relaxometry::VoxelMajor::new(n_voxels, n);
            let mut rss = vec![0.0f64; n_voxels];
            for e in 1..=n {
                let item = self.working_magnitude(e);
                let echo = self.store.real(&item)?;
                multi.set(e - 1, &echo, &format!("mag echo {e}"))?;
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
        let mut r2prime = if self.store.has(&r2prime_item) { Some(self.store.real(&r2prime_item)?) } else { None };
        // The MESE's echoes on this run's grid, for HC-ChiSep. Voxels outside its FOV are zero,
        // the same as voxels outside the brain.
        let se_multi = match &mese {
            Some(m) => self.mese_on_grid(m)?.ok().map(|m| m.echoes),
            None => None,
        };
        // The local field (ppm), for the field-based methods.
        let field_item = Item::LocalField(Pass::Main);
        let mut local_field = if self.store.has(&field_item) { self.store.real(&field_item)? } else { Vec::new() };

        let missing = sep::missing_inputs(alg, &qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &local_field, qsm: &qsm, mask: &brain, r2prime: r2prime.as_deref(),
            r2star: r2star.as_deref(), magnitude_rss: magnitude_rss.as_deref(),
            magnitude_multi: magnitude_multi.as_deref(), se_magnitude_multi: se_multi.as_deref(),
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
        let published = if qsm_item == Item::Chi && self.store.has(&Item::ChiSupport) {
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
        sep::zero_outside(&support, &mut qsm);
        if uses_field {
            sep::zero_outside(&support, &mut local_field);
        }
        for map in [r2prime.as_mut(), r2star.as_mut()].into_iter().flatten() {
            sep::zero_outside(&support, map);
        }
        let inputs = qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &local_field, qsm: &qsm, mask: &support, r2prime: r2prime.as_deref(),
            r2star: r2star.as_deref(), magnitude_rss: magnitude_rss.as_deref(),
            magnitude_multi: magnitude_multi.as_deref(), se_magnitude_multi: se_multi.as_deref(),
        };
        self.store.models(&step.models)?;
        let separated = sep::separate(inputs, mese.as_ref().map(|m| m.echo_times.as_slice()), &self.meta,
                                      self.config, self.observer)?;
        self.put(Item::SeparatedChi(Contrast::Paramagnetic), Data::Real(&separated.paramagnetic))?;
        self.put(Item::SeparatedChi(Contrast::Diamagnetic), Data::Real(&separated.diamagnetic))?;
        self.put(Item::SeparatedTotalChi, Data::Real(&separated.total))?;
        let mut inputs = vec![mask_item, qsm_item];
        if published.is_some() {
            inputs.push(Item::ChiSupport);
        }
        self.complete(Completed {
            step: "chi_separation", algorithm: Some(&alg_name), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs,
            outputs: &[Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                       Item::SeparatedTotalChi],
        }, &format!("Chi-separation ({alg_name})"))
    }

    /// Retrospective motion correction: every echo co-registered onto the reference echo and the
    /// mask cut to where every echo still has data. Off (or with one echo), what an earlier run
    /// of it left is stale: its coverage mask must not be published as this run's, and every
    /// step that read the corrected echoes or that mask has to run again on the uncorrected ones.
    fn motion(&mut self) -> Result<(), S::Error> {
        let m = &self.config.motion_correction;
        if !m.enabled {
            return self.store.discard("motion", &[Item::CoverageMask]);
        }
        if !self.meta.has_magnitude {
            return Err(crate::EngineError::Config(
                "--motion-correction needs magnitude images: the registration is estimated on magnitude, \
                 and this run has none".into()).into());
        }
        let n = self.n_echoes();
        if n < 2 {
            self.log(Level::Warn, "Motion correction needs at least two echoes; this run has one, so it is skipped");
            self.skip("Motion correction needs at least two echoes, and this run has one.");
            return self.store.discard("motion", &[Item::CoverageMask]);
        }
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
        let (mut phases, mut magnitudes, mut inputs) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::new());
        for e in 1..=n {
            phases.push(self.store.real(&Item::Phase(e))?);
            magnitudes.push(self.store.real(&Item::Magnitude(e))?);
            inputs.extend([Item::Phase(e), Item::Magnitude(e)]);
        }
        let mask = self.store.mask(&Item::BrainMask)?;
        inputs.push(Item::BrainMask);
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
        self.complete(Completed {
            step: "motion", algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &outputs,
        }, "Motion correction")
    }

    /// Whole-brain parcellation of the combined magnitude by SynthSeg, with its lookup table — or
    /// a segmentation supplied with the scan, read as FreeSurfer ids, when one is asked for and
    /// fits the working grid.
    fn segmentation(&mut self, step: &Step) -> Result<(), S::Error> {
        let config = self.config;
        let c = &config.segmentation;
        let params = stages::segmentation::params(config);
        let Some(key) = self.begin("segmentation", Some("synthseg"), &params, "Skipping segmentation (cached)",
                                   "Segmenting") else { return Ok(()) };
        let lookup = stages::segmentation::lookup_table(c.version);
        // Prefer a bring-your-own segmentation, as masking does.
        if let Some(tool) = &c.custom_dseg_tool {
            match self.store.supplied(&Item::SuppliedSegmentation) {
                Supplied::Found { origin, data, dims, .. } if dims == self.meta.dims => {
                    self.log(Level::Info, &format!("Using custom segmentation from {origin}"));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Supplied,
                        "The segmentation was the one supplied with the scan.").with_detail(origin));
                    self.put(Item::Segmentation, Data::Real(&data))?;
                    self.put(Item::SegmentationLabels, Data::Text(&lookup))?;
                    return self.complete(Completed {
                        step: "segmentation", algorithm: Some("synthseg"), params: &params, outcome: None, key: &key, decisions: &[],
                        inputs: &[Item::SuppliedSegmentation], outputs: &[Item::Segmentation, Item::SegmentationLabels],
                    }, "Segmentation (custom)");
                }
                Supplied::Found { origin, dims, .. } => {
                    self.log(Level::Warn, &format!(
                        "custom segmentation {origin} is {}x{}x{}, not the run's {}x{}x{} — computing instead",
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
        self.complete(Completed {
            step: "segmentation", algorithm: Some("synthseg"), params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &[Item::CombinedMagnitude],
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
        self.observer.stage("analysis", None, StageEvent::Started("Summarising maps per structure"));
        self.store.start("analysis");
        let seg = self.store.real(&seg_item)?;
        let labels = self.config.segmentation.version.to_core().labels();
        // SynthSeg's per-label volumes, when this run segmented the scan itself.
        let volumes: Option<Vec<f64>> = self.store.report(&Item::SegmentationVolumes)
            .and_then(|v| serde_json::from_value(v).ok());
        let mut table = String::from(a::HEADER);
        for (item, name, unit) in &maps {
            let map = self.store.real(item)?;
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
        let inputs: Vec<Item> = std::iter::once(seg_item).chain(maps.iter().map(|(i, ..)| *i)).collect();
        self.complete(Completed {
            step: "analysis", algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &inputs, outputs: &[Item::RegionStats],
        }, "Per-structure analysis")
    }

    /// The outputs of a run resampled to axial, back on the grid it was acquired on
    /// ([`stages::output_space`]) — or, with `--output-space working`, left where they are.
    fn output_space(&mut self) -> Result<(), S::Error> {
        use stages::output_space::{self as o, Kind};
        let Some((dst_dims, dst_affine)) = self.meta.source_geometry else {
            self.skip("The run was not resampled, so its outputs are on the acquired grid.");
            return Ok(());
        };
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
        let published: Vec<(Item, Kind)> = o::published(self.n_echoes()).into_iter().filter(|(i, _)| self.store.has(i)).collect();
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
        self.complete(Completed {
            step: "output_space", algorithm: None, params: &params, outcome: None, key: &key, decisions: &[],
            inputs: &items, outputs: &items,
        }, "Output space")
    }
}

/// How a pass is named in progress messages and the log: nothing for a single-pass run's (only)
/// pass, so its log reads as it always has.
fn pass_label(pass: Pass) -> &'static str {
    match pass {
        Pass::Main => "",
        Pass::Reliable => " (reliable pass)",
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::enums::{BfAlgorithm, QsmAlgorithm};
    use crate::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
    use crate::store::{Header, MemoryStore};

    const N: usize = 16;
    const TES: [f64; 3] = [0.005, 0.010, 0.015];

    /// A sphere of tissue in a 16³ box at 1 mm, three echoes: a smooth local field inside it
    /// (wrapped into the phase at each echo time) and a magnitude that is bright inside, dark out.
    pub(crate) fn phantom() -> (ScanFacts, MemoryStore) {
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        phantom_on(affine)
    }

    /// The phantom, its voxels placed in the world by `affine`.
    pub(crate) fn phantom_on(affine: [f64; 16]) -> (ScanFacts, MemoryStore) {
        let header = Header { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, sidecar: None };
        let mut store = MemoryStore::new(header, json!("phantom-1"));
        let c = (N as f64 - 1.0) / 2.0;
        let inside = |x: usize, y: usize, z: usize| {
            let (dx, dy, dz) = (x as f64 - c, y as f64 - c, z as f64 - c);
            dx * dx + dy * dy + dz * dz < 36.0
        };
        let (mut field_hz, mut magnitude) = (vec![0.0; N * N * N], vec![0.0; N * N * N]);
        for z in 0..N { for y in 0..N { for x in 0..N {
            let i = x + y * N + z * N * N;
            if inside(x, y, z) {
                field_hz[i] = 4.0 * ((x as f64 - c) / 6.0) + 2.0 * ((z as f64 - c) / 6.0).powi(2);
                magnitude[i] = 100.0;
            } else {
                magnitude[i] = 1.0;
            }
        }}}
        for (e, te) in TES.iter().enumerate() {
            let phase: Vec<f64> = field_hz.iter()
                .map(|f| (2.0 * std::f64::consts::PI * f * te + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI)
                    - std::f64::consts::PI)
                .collect();
            let mag: Vec<f64> = magnitude.iter().map(|m| m * (-te / 0.04).exp()).collect();
            store.insert(Item::AcquiredPhase(e + 1), VolumeData::Real(phase));
            store.insert(Item::AcquiredMagnitude(e + 1), VolumeData::Real(mag));
        }
        let facts = ScanFacts {
            dims: (N, N, N), n_echoes: TES.len(), echo_times: TES.to_vec(), has_magnitude: true,
            n_coils: 1, real_imaginary: false, field_strength: 3.0, b0_dir: None,
            mese_echo_times: None, custom_mask: false, custom_dseg: false, custom_qsm: false,
            custom_r2: false, custom_r2prime: false,
        };
        (facts, store)
    }

    /// A quick configuration for a tiny phantom: a magnitude threshold for the mask, PDF (which
    /// does not erode a small brain away) and TKD.
    pub(crate) fn config() -> PipelineConfig {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        config.masking.refinements = vec![];
        config.masking.inhomogeneity_correction = false;
        config.bg_removal.algorithm = BfAlgorithm::Pdf;
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        config
    }

    /// Which steps were computed and which were cached, in order.
    #[derive(Default)]
    struct Events {
        computed: Vec<String>,
        cached: Vec<String>,
        outputs: Vec<Item>,
    }

    impl Observer for Events {
        fn stage(&mut self, step: &str, _algorithm: Option<&str>, event: StageEvent<'_>) {
            match event {
                StageEvent::Started(_) => self.computed.push(step.to_string()),
                StageEvent::Cached => self.cached.push(step.to_string()),
                StageEvent::Finished(_) => {}
            }
        }

        fn output(&mut self, item: &Item, _volume: qsm_core::pipeline::VolumeRef<'_>) {
            self.outputs.push(*item);
        }
    }

    fn run_once(config: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore) -> Events {
        let plan = crate::plan::plan(config, facts);
        let mut events = Events::default();
        run(&plan, config, facts, store, &mut events).unwrap();
        events
    }

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
        assert!(n_support > 100, "the support is the sphere less what PDF left undefined: {n_support}");
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

    /// Two-pass reconstructs twice and references both maps; each pass's map is defined where its
    /// background removal left a field.
    #[test]
    fn two_pass_runs_both_passes_in_memory() {
        let (facts, mut store) = phantom();
        let mut config = config();
        config.masking.two_pass = true;
        config.masking.two_pass_sections = Some(vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
            refinements: vec![],
        }]);
        let events = run_once(&config, &facts, &mut store);
        assert_eq!(events.computed[4..], ["unwrap", "bgremove", "invert", "mask-reliable", "bgremove-reliable",
                                          "invert-reliable", "twopass", "reference-singlepass", "reference"]);
        for item in [Item::CombinedRawChi, Item::SinglePassChi, Item::Chi, Item::RawChi(Pass::Reliable)] {
            assert!(store.get(&item).is_some(), "{item:?}");
        }
        // The combination is keyed by which mask its reliable pass is defined on.
        let plan = crate::plan::plan(&config, &facts);
        let mut quiet = ();
        let r = Run::new(&plan, &config, &facts, crate::stages::magnitude::tests::meta_4x4x4(), &mut store, &mut quiet);
        assert_eq!(r.support(Pass::Reliable), Item::LocalFieldMask(Pass::Reliable));
        assert_eq!(r.store.identity(&r.support(Pass::Reliable)), json!("LocalFieldMask(Reliable)"));
    }

    /// SWI and its minIP, T2*/R2*, and SMWI from the final map, all in memory: each projection on
    /// its own shorter grid, T2* the inverse of R2* inside the mask.
    #[test]
    fn swi_smwi_and_relaxometry_run_in_memory() {
        let (facts, mut store) = phantom();
        let mut config = config();
        config.pipeline.do_swi = true;
        config.pipeline.do_smwi = true;
        config.pipeline.do_t2starmap = true;
        config.pipeline.do_r2starmap = true;
        config.swi.mip_window = 4;
        config.smwi.mip_window = 4;
        let events = run_once(&config, &facts, &mut store);
        for step in ["swi", "t2star_r2star", "smwi"] {
            assert!(events.computed.iter().any(|c| c == step), "{step}: {:?}", events.computed);
        }
        for item in [Item::SwiMip, Item::SmwiMip(Contrast::Paramagnetic), Item::SmwiMip(Contrast::Diamagnetic)] {
            assert_eq!(store.geometry(&item).map(|g| g.dims), Some((N, N, N - 3)), "{item:?}");
            assert!(!events.outputs.contains(&item), "a projection is not shown on the run's grid");
        }
        for item in [Item::Swi, Item::Smwi(Contrast::Paramagnetic), Item::Smwi(Contrast::Diamagnetic)] {
            assert!(events.outputs.contains(&item) && store.geometry(&item).is_none(), "{item:?}");
        }
        let (Some(VolumeData::Real(r2star)), Some(VolumeData::Real(t2star)), Some(VolumeData::Mask(mask))) =
            (store.get(&Item::R2star), store.get(&Item::T2star), store.get(&Item::BrainMask)) else { panic!("no maps") };
        // The phantom's magnitude decays with T2* = 40 ms.
        let inside: Vec<f64> = r2star.iter().zip(mask).filter(|(_, &m)| m != 0).map(|(&r, _)| r).collect();
        assert!(inside.iter().all(|r| (r - 25.0).abs() < 1.0), "R2* {:?}", &inside[..4]);
        assert!(r2star.iter().zip(t2star).all(|(&r, &t)| if r > 0.0 { (t * r - 1.0).abs() < 1e-12 } else { t == 0.0 }));
        assert!(run_once(&config, &facts, &mut store).computed.is_empty());

        // SMWI reads the referenced map, so a new inversion redoes it; SWI and R2* read the echoes.
        config.inversion.tkd.threshold *= 2.0;
        assert_eq!(run_once(&config, &facts, &mut store).computed, ["invert", "reference", "smwi"]);
    }

    /// The phantom with a MESE on its own grid (the run's): spin echoes at n·10 ms decaying with
    /// R2 = 10 Hz inside the sphere.
    pub(crate) fn with_mese(facts: &mut ScanFacts, store: &mut MemoryStore) {
        let tes: Vec<f64> = vec![0.01, 0.02, 0.03, 0.04];
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        let geometry = crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine };
        let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
        for (e, te) in tes.iter().enumerate() {
            let echo: Vec<f64> = m.iter().map(|&v| if v > 50.0 { 1000.0 * (-10.0f64 * te).exp() } else { 1.0 }).collect();
            store.insert_on_grid(Item::MeseMagnitude(e + 1), echo, geometry.clone());
        }
        store.supply_mese(crate::store::Mese { name: "MESE".into(), dims: (N, N, N), affine, echo_times: tes.clone() });
        facts.mese_echo_times = Some(tes);
    }

    /// R2 from the MESE, R2′ measured as R2* − R2 (and recorded as measured), and χ-separation
    /// over the final map's support, in memory.
    #[test]
    fn r2_r2prime_and_chi_separation_run_in_memory() {
        let (mut facts, mut store) = phantom();
        with_mese(&mut facts, &mut store);
        let mut config = config();
        config.pipeline.do_r2starmap = true;
        config.pipeline.do_r2map = true;
        config.pipeline.do_r2primemap = true;
        config.pipeline.do_chi_separation = true;
        config.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepIlsqr;
        let events = run_once(&config, &facts, &mut store);
        for step in ["t2star_r2star", "r2_r2prime", "chi_separation"] {
            assert!(events.computed.iter().any(|c| c == step), "{step}: {:?}", events.computed);
        }
        let real = |item: Item| match store.get(&item) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!("no {item:?}") };
        let (r2, mask) = (real(Item::R2), store.get(&Item::BrainMask).cloned().unwrap());
        let VolumeData::Mask(mask) = mask else { panic!() };
        let inside: Vec<f64> = r2.iter().zip(&mask).filter(|(_, &m)| m != 0).map(|(&r, _)| r).collect();
        assert!(inside.iter().all(|r| (r - 10.0).abs() < 1.0), "R2 {:?}", &inside[..4]);
        assert_eq!(store.annotations(&Item::R2prime).and_then(|a| a.get("R2primeSource")), Some(&json!("measured")));
        assert!(store.get(&Item::R2primeCoverage).is_some());
        let support = match store.get(&Item::ChiSupport) { Some(VolumeData::Mask(m)) => m.clone(), _ => panic!() };
        for item in [Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic), Item::SeparatedTotalChi] {
            let map = real(item);
            assert!(map.iter().zip(&support).all(|(&v, &s)| s != 0 || v == 0.0), "{item:?} outside the support");
            assert!(map.iter().all(|v| v.is_finite()));
        }
        assert!(real(Item::SeparatedChi(Contrast::Diamagnetic)).iter().all(|&v| v >= 0.0), "|χ−|");
        assert!(run_once(&config, &facts, &mut store).computed.is_empty());
    }

    /// Without a MESE, R2′ is predicted by R2PRIMEnet (whose weights the plan lists), and a
    /// network separates the sources. Minutes in a debug build, so run with `--ignored`; skipped
    /// where the weights have not been downloaded.
    #[test]
    #[ignore]
    fn r2primenet_and_a_separation_network_run_in_memory() {
        let present = |id: &str| qsm_core::models::find_model(id)
            .is_some_and(|m| m.files.iter().all(qsm_core::models::weight_file_present));
        if !cfg!(feature = "onnx") || !present("r2primenet") || !present("chi-sepnet") {
            eprintln!("skipped: R2PRIMEnet or χ-sepnet weights are not available here");
            return;
        }
        let (facts, mut store) = phantom();
        let mut config = config();
        config.pipeline.do_r2starmap = true;
        config.pipeline.do_r2primemap = true;
        config.pipeline.do_chi_separation = true;
        config.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepNet;
        config.separation.r2primenet.patch = [16, 16, 16];
        let plan = crate::plan::plan(&config, &facts);
        assert_eq!(plan.models, ["r2primenet", "chi-sepnet"]);
        run(&plan, &config, &facts, &mut store, &mut ()).unwrap();
        assert_eq!(store.annotations(&Item::R2prime).and_then(|a| a.get("R2primeSource")), Some(&json!("predicted")));
        assert!(store.get(&Item::SeparatedTotalChi).is_some());
    }

    /// A supplied segmentation, a region reference measured on it, and the statistics of every map
    /// over it, all in memory.
    #[test]
    fn segmentation_and_analysis_run_in_memory() {
        let (mut facts, mut store) = phantom();
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        // Two structures: the sphere's left and right halves.
        let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
        let dseg: Vec<f64> = m.iter().enumerate()
            .map(|(i, &v)| if v > 50.0 { if i % N < N / 2 { 10.0 } else { 49.0 } } else { 0.0 }).collect();
        store.insert_on_grid(Item::SuppliedSegmentation, dseg.clone(),
                             crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine });
        facts.custom_dseg = true;
        let mut config = config();
        config.pipeline.do_segmentation = true;
        config.pipeline.do_analysis = true;
        config.pipeline.do_r2starmap = true;
        config.segmentation.custom_dseg_tool = Some("mine".into());
        config.qsm.reference = crate::enums::QsmReference::Region;
        config.qsm.reference_region = Some("thalamus".into());
        let events = run_once(&config, &facts, &mut store);
        for step in ["segmentation", "analysis"] {
            assert!(events.computed.iter().any(|c| c == step), "{step}: {:?}", events.computed);
        }
        assert_eq!(store.get(&Item::Segmentation), Some(&VolumeData::Real(dseg)));
        let lookup = store.report(&Item::SegmentationLabels).unwrap();
        assert!(lookup.as_str().unwrap().starts_with("index\tname\n"));
        assert!(store.report(&Item::SegmentationVolumes).is_none(), "a supplied segmentation has no posteriors");
        let table = store.report(&Item::RegionStats).unwrap();
        let rows: Vec<&str> = table.as_str().unwrap().lines().skip(1).collect();
        for map in ["Chimap", "R2starmap"] {
            assert_eq!(rows.iter().filter(|r| r.starts_with(&format!("{map}\t"))).count(), 2, "{map}: {rows:?}");
        }
        assert!(rows.iter().filter(|r| r.starts_with("Chimap\t")).all(|r| r.contains("\tthalamus\t")), "{rows:?}");
        assert!(run_once(&config, &facts, &mut store).computed.is_empty());
    }

    /// Motion correction in memory: the corrected echoes and the mask every echo covers, which
    /// the steps after it read. Turned off again, what it left goes, and the run is redone (the
    /// load step's key carries the setting, as for a change of grid).
    #[test]
    fn motion_correction_runs_in_memory() {
        let (facts, mut store) = phantom();
        let mut config = config();
        config.motion_correction.enabled = true;
        let events = run_once(&config, &facts, &mut store);
        assert_eq!(events.computed[..5], ["load", "scale_phase", "magnitude", "mask", "motion"]);
        assert!(store.get(&Item::CoverageMask).is_some() && store.get(&Item::CorrectedPhase(3)).is_some());
        let report = store.report(&Item::MotionReport).unwrap();
        assert_eq!(report["echoes"].as_array().map(Vec::len), Some(3));
        assert!(run_once(&config, &facts, &mut store).computed.is_empty());

        config.motion_correction.enabled = false;
        let off = run_once(&config, &facts, &mut store);
        assert!(store.get(&Item::CoverageMask).is_none(), "the coverage mask is not this run's");
        assert_eq!(off.computed, ["load", "scale_phase", "magnitude", "mask", "unwrap", "bgremove", "invert", "reference"]);
    }

    /// Two-pass on the phantom, its reliable-phase mask a fixed threshold.
    fn two_pass_config() -> PipelineConfig {
        let mut config = config();
        config.masking.two_pass = true;
        config.masking.two_pass_sections = Some(vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
            refinements: vec![],
        }]);
        config
    }

    /// Another brain mask for the phantom: the usual one, eroded.
    fn erode_the_brain_mask(config: &mut PipelineConfig) {
        config.masking.sections[0].refinements = vec![MaskOp::Erode { iterations: 1 }];
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
        let mut got = store.recorded();
        let plan: Vec<String> = crate::plan::plan(config, facts).steps.iter().map(|s| s.stage.name()).collect();
        got.steps.retain(|step, _| want.recorded.steps.contains_key(step) || {
            assert!(!plan.contains(step), "{step} is in the plan but a fresh run does not record it");
            false
        });
        assert_eq!(got.steps, want.recorded.steps);
        for (item, volume) in &want.outputs {
            assert!(store.get(item) == volume.as_ref(), "{item:?} is not what a fresh run makes");
        }
        assert_eq!(got.reports, want.recorded.reports);
        assert_eq!(got.annotations, want.recorded.annotations);
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

    /// The inversion steps are named for their algorithm (`invert`, `tgv`, `qsmart`), and every one
    /// writes the pass's raw map. Switching from `a` to another and back in the same store (#373)
    /// would leave `a`'s record naming a map the other has since replaced: the switch back
    /// recomputes it, and what reads it, and the store holds what `a`'s first (fresh) run left —
    /// single- and two-pass, after each of the others.
    fn switch_away_and_back(a: QsmAlgorithm) {
        let algorithms = [QsmAlgorithm::Tkd, QsmAlgorithm::Tgv, QsmAlgorithm::Qsmart];
        for two_pass in [false, true] {
            let mut config = if two_pass { two_pass_config() } else { config() };
            config.inversion.qsmart.ilsqr_max_iter = 5;
            config.inversion.algorithm = a;
            let (facts, mut store) = phantom();
            run_once(&config, &facts, &mut store);
            let fresh = snapshot(&store);
            let step = crate::plan::plan(&config, &facts).steps.iter()
                .find(|s| matches!(s.stage, Stage::Inversion(Pass::Main) | Stage::Tgv(Pass::Main) | Stage::Qsmart(Pass::Main)))
                .map(|s| s.stage.name()).unwrap();
            for b in algorithms.into_iter().filter(|&b| b != a) {
                config.inversion.algorithm = b;
                run_once(&config, &facts, &mut store);
                config.inversion.algorithm = a;
                let back = run_once(&config, &facts, &mut store);
                assert!(back.computed.contains(&step), "{a} → {b} → {a}, two-pass {two_pass}: {:?}", back.computed);
                assert_holds(&config, &facts, &store, &fresh);
                assert!(run_once(&config, &facts, &mut store).computed.is_empty(), "{a} → {b} → {a}, two-pass {two_pass}");
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

    /// SynthSeg in memory. Minutes in a debug build, so run with `--ignored`; skipped where the
    /// weights have not been downloaded.
    #[test]
    #[ignore]
    fn synthseg_runs_in_memory() {
        let present = qsm_core::models::find_model("synthseg")
            .is_some_and(|m| m.files.iter().all(qsm_core::models::weight_file_present));
        if !cfg!(feature = "onnx") || !present {
            eprintln!("skipped: SynthSeg's weights are not available here");
            return;
        }
        let (facts, mut store) = phantom();
        let mut config = config();
        config.pipeline.do_qsm = false;
        config.pipeline.do_segmentation = true;
        let events = run_once(&config, &facts, &mut store);
        assert!(events.computed.iter().any(|c| c == "segmentation"), "{:?}", events.computed);
        let Some(VolumeData::Real(labels)) = store.get(&Item::Segmentation) else { panic!("no segmentation") };
        assert_eq!(labels.len(), N * N * N);
        let volumes = store.report(&Item::SegmentationVolumes).expect("SynthSeg's volumes");
        assert_eq!(volumes.as_array().unwrap().len(), config.segmentation.version.to_core().labels().ids.len());
    }

    /// An oblique run resampled to axial, reconstructed there and returned to the grid it was
    /// acquired on: the masks still binary, the minIP rebuilt on that grid, and a re-run that
    /// recomputes a step reads the working-grid versions again.
    #[test]
    fn a_resampled_run_returns_to_the_acquired_grid_in_memory() {
        let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
        let affine = [1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0];
        let (facts, mut store) = phantom_on(affine);
        let mut config = config();
        config.pipeline.axial_resampling = true;
        config.pipeline.obliquity_threshold = 5.0;
        config.pipeline.do_swi = true;
        config.swi.mip_window = 4;
        let events = run_once(&config, &facts, &mut store);
        assert_eq!(events.computed.last().map(String::as_str), Some("output_space"), "{:?}", events.computed);
        for item in [Item::Chi, Item::ChiSupport, Item::BrainMask, Item::CombinedMagnitude, Item::Swi] {
            let g = store.geometry(&item).unwrap_or_else(|| panic!("{item:?} left on the working grid"));
            assert_eq!((g.dims, g.affine), ((N, N, N), affine), "{item:?}");
        }
        let Some(VolumeData::Real(mask)) = store.get(&Item::BrainMask) else { panic!() };
        assert!(mask.iter().all(|&v| v == 0.0 || v == 1.0), "a mask travels by nearest neighbour");
        let mip = store.geometry(&Item::SwiMip).unwrap();
        assert_eq!(mip.dims, (N, N, N - 3), "rebuilt from the returned SWI");
        assert!(run_once(&config, &facts, &mut store).computed.is_empty());

        // A new inversion reads the working-grid field again, and returns its map once more.
        config.inversion.tkd.threshold *= 2.0;
        let again = run_once(&config, &facts, &mut store);
        assert_eq!(again.computed, ["invert", "reference", "output_space"]);
        assert!(store.geometry(&Item::Chi).is_some_and(|g| g.affine == affine));
        assert!(store.geometry(&Item::LocalField(Pass::Main)).is_none(), "intermediates stay on the working grid");
    }

    /// Uncombined coils and real/imaginary parts, in memory: the coils combined by MCPC-3D-S,
    /// the parts converted (GE's phase sign undone), and either one reconstructed from there.
    #[test]
    fn coils_and_real_imaginary_parts_run_in_memory() {
        let (facts, store) = phantom();
        let echo = |item: Item| match store.get(&item) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!() };
        let (phases, mags): (Vec<_>, Vec<_>) = (1..=TES.len()).map(|e| (echo(Item::AcquiredPhase(e)), echo(Item::AcquiredMagnitude(e)))).unzip();
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        let header = |manufacturer: &str| {
            let sidecar: crate::scan::QsmSidecar = serde_json::from_value(json!({
                "EchoTime": 0.005, "MagneticFieldStrength": 3.0, "Manufacturer": manufacturer })).unwrap();
            Header { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, sidecar: Some(sidecar) }
        };

        // Two coils, each seeing the scan with its own smooth sensitivity and phase offset.
        let mut coils = MemoryStore::new(header("Siemens"), json!("coils"));
        for (c, offset) in [(1, 0.4), (2, -0.9)] {
            for e in 1..=TES.len() {
                let (p, m) = (&phases[e - 1], &mags[e - 1]);
                let weight = |i: usize| 0.5 + (i % N) as f64 / N as f64 * if c == 1 { 1.0 } else { -1.0 } * 0.4;
                let phase: Vec<f64> = p.iter().map(|&v| (v + offset + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).collect();
                coils.insert(Item::CoilPhase(c, e), VolumeData::Real(phase));
                coils.insert(Item::CoilMagnitude(c, e), VolumeData::Real(m.iter().enumerate().map(|(i, &v)| v * weight(i)).collect()));
            }
        }
        let mut f = facts.clone();
        f.n_coils = 2;
        let config = config();
        let events = run_once(&config, &f, &mut coils);
        assert!(events.computed.contains(&"scale_phase".to_string()) && events.computed.contains(&"reference".to_string()));
        for e in 1..=TES.len() {
            assert!(coils.get(&Item::CombinedEchoPhase(e)).is_some() && coils.get(&Item::Phase(e)).is_some());
        }
        assert!(coils.get(&Item::CoilCombinationMask).is_some());
        assert!(run_once(&config, &f, &mut coils).computed.is_empty());

        // Real and imaginary parts from a GE scanner, whose phase has the opposite sign.
        let mut parts = MemoryStore::new(header("GE MEDICAL SYSTEMS"), json!("parts"));
        for e in 1..=TES.len() {
            let (p, m) = (&phases[e - 1], &mags[e - 1]);
            parts.insert(Item::AcquiredPhase(e), VolumeData::Real(p.iter().zip(m).map(|(&p, &m)| m * p.cos()).collect()));
            parts.insert(Item::AcquiredImaginary(e), VolumeData::Real(p.iter().zip(m).map(|(&p, &m)| -m * p.sin()).collect()));
        }
        let mut f = facts.clone();
        f.real_imaginary = true;
        run_once(&config, &f, &mut parts);
        let (Some(VolumeData::Real(converted)), Some(VolumeData::Real(magnitude))) =
            (parts.get(&Item::Phase(2)), parts.get(&Item::Magnitude(2))) else { panic!("not converted") };
        // The phase comes back as given: wrapped, so compared on the circle.
        assert!(converted.iter().zip(&phases[1]).all(|(&a, &b)| (a - b).sin().abs() < 1e-9 && (a - b).cos() > 0.0));
        assert!(magnitude.iter().zip(&mags[1]).all(|(a, b)| (a - b).abs() < 1e-9));
        assert!(parts.get(&Item::Chi).is_some());
    }

}

#[cfg(test)]
mod record_tests;
