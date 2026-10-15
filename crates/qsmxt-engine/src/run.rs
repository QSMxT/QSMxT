//! Running a plan: every step in order, each computed only when its earlier output is no longer
//! valid, its inputs read from the store as it starts and its outputs handed back as they are
//! produced.
//!
//! The steps the engine computes are here; the ones it does not yet ([`Step::host`]) are handed
//! to the store. A step moves into the engine by gaining a function here, its `host` flag
//! dropping in [`crate::plan`], and its arm in the store's `host` going away.
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
use crate::scan::{RunMetadata, ScanFacts};
use crate::stages;
use crate::store::{Completed, Data, Item, StageStore, Supplied};

/// Run `plan` (made by [`crate::plan::plan`] from `config` and `facts`) on the scan in `store`,
/// reporting to `observer`.
///
/// What the plan says the data cannot produce ([`Plan::check`]) is the front end's to refuse
/// before calling this: `qsmxt` checks every run before the first starts.
pub fn run<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, store: &mut S, observer: &mut dyn Observer,
) -> Result<(), S::Error> {
    let meta = load(config, facts, store, observer)?;
    store.begin(&meta)?;
    if !plan.processes() {
        observer.log(Level::Info, "No outputs enabled — nothing to process");
        return store.finish();
    }
    if !config.pipeline.do_qsm {
        observer.log(Level::Info, "QSM processing disabled — skipping reconstruction");
    }
    let mut r = Run { plan, config, meta, store, observer };
    for step in &plan.steps {
        if step.host {
            r.store.host(step, plan, r.observer)?;
        } else {
            r.step(step)?;
        }
    }
    r.store.finish()
}

/// Run one of `plan`'s engine steps on its own, on a run whose grid (from its load step) is
/// `meta`: what a front end re-running a single step uses (a mask preview, say), and tests.
pub fn run_step<S: StageStore + ?Sized>(
    plan: &Plan, config: &PipelineConfig, meta: &RunMetadata, step: &Step, store: &mut S,
    observer: &mut dyn Observer,
) -> Result<(), S::Error> {
    Run { plan, config, meta: meta.clone(), store, observer }.step(step)
}

/// The grid the run is processed on: the cached load step's, or decided from the first echo's
/// header ([`crate::scan::resolve_geometry`]) and recorded.
fn load<S: StageStore + ?Sized>(
    config: &PipelineConfig, facts: &ScanFacts, store: &mut S, observer: &mut dyn Observer,
) -> Result<RunMetadata, S::Error> {
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
    if let Some(meta) = store.cached_scan(&key) {
        observer.log(Level::Info, "Skipping load (cached)");
        observer.stage("load", None, StageEvent::Cached);
        return Ok(meta);
    }
    observer.stage("load", None, StageEvent::Started("Loading NIfTI metadata"));
    store.start("load");
    let header = store.header()?;
    let mut meta = crate::scan::resolve_geometry(header.dims, header.voxel_size, header.affine, facts, config);
    meta.slice_geometry = match &header.sidecar {
        Some(sc) => crate::scan::check_slice_gap(sc, header.dims, header.voxel_size, &meta, config)?,
        None => None,
    };
    let (d, v) = (header.dims, header.voxel_size);
    observer.log(Level::Info, &format!(
        "Volume: {}x{}x{}, {:.2}x{:.2}x{:.2}mm, {} echoes{}, B0={:.1}T, TEs={:?}s",
        d.0, d.1, d.2, v.0, v.1, v.2, meta.n_echoes,
        if facts.n_coils > 1 { format!(" x {} uncombined coils", facts.n_coils) } else { String::new() },
        meta.field_strength, meta.echo_times,
    ));
    store.record_scan(&key, &meta)?;
    observer.stage("load", None, StageEvent::Finished("Load"));
    Ok(meta)
}

/// Every echo's phase, and every echo's magnitude (or none).
type Echoes = (Vec<Vec<f64>>, Vec<Vec<f64>>);

/// One run of a plan, between its steps.
struct Run<'r, S: StageStore + ?Sized> {
    plan: &'r Plan,
    config: &'r PipelineConfig,
    meta: RunMetadata,
    store: &'r mut S,
    observer: &'r mut dyn Observer,
}

impl<S: StageStore + ?Sized> Run<'_, S> {
    fn step(&mut self, step: &Step) -> Result<(), S::Error> {
        match step.stage {
            Stage::Load => Ok(()),
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
            stage => Err(crate::EngineError::Config(format!(
                "the {} step is not in the engine yet", stage.name())).into()),
        }
    }

    /// The key `step` is cached under, unless its earlier output is still valid (then `None`, with
    /// `skipping` logged). Otherwise the step starts, `doing` being what it does.
    fn begin(&mut self, step: &str, algorithm: Option<&str>, params: &Value, skipping: &str, doing: &str) -> Option<String> {
        let key = step_cache_key(step, algorithm, params, self.config);
        if self.store.cached(step, &key) {
            self.observer.log(Level::Info, skipping);
            self.observer.stage(step, algorithm, StageEvent::Cached);
            return None;
        }
        self.observer.stage(step, algorithm, StageEvent::Started(doing));
        self.store.start(step);
        Some(key)
    }

    /// Keep an output, and show it. A volume on a grid of its own (a projection) is kept but not
    /// shown: the observer's volumes are on the run's grid.
    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), S::Error> {
        self.store.put(item, data)?;
        match data {
            Data::Real(v) => self.observer.output(&item, qsm_core::pipeline::VolumeRef::Real(v)),
            Data::Mask(m) => self.observer.output(&item, qsm_core::pipeline::VolumeRef::Mask(m)),
            Data::OnGrid(..) | Data::Json(_) | Data::Same(_) => {}
        }
        Ok(())
    }

    /// Record a computed step; `label` names it in the log.
    fn complete(&mut self, c: Completed<'_>, label: &str) -> Result<(), S::Error> {
        self.store.record(&c)?;
        self.observer.stage(c.step, c.algorithm, StageEvent::Finished(label));
        Ok(())
    }

    fn log(&mut self, level: Level, message: &str) {
        self.observer.log(level, message);
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
            step: "scale_phase", algorithm: None, params: &params, outcome: None, key: &key,
            inputs: &inputs, outputs: &outputs,
        }, if resampled { "Resample + rescale phase" } else { "Rescale phase" })
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
            self.store.record(&Completed {
                step: "magnitude", algorithm: Some("rss"), params: &params, outcome: None, key: &key,
                inputs: &inputs, outputs: &[Item::CombinedMagnitude],
            })?;
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
                        self.put(Item::BrainMask, Data::Mask(&mask))?;
                        return self.complete(Completed {
                            step: "mask", algorithm: None, params: &params, outcome: None, key: &key,
                            inputs: &[Item::SuppliedMask], outputs: &[Item::BrainMask],
                        }, "Mask (custom)");
                    }
                    self.log(Level::Warn, &format!(
                        "custom mask {origin} could not be conformed to the working grid — computing instead"));
                    models = stages::mask::model_ids(&self.config.masking.sections, self.config);
                }
                Supplied::Unreadable { origin, error } => {
                    self.log(Level::Warn, &format!("could not read custom mask {origin} ({error}) — computing instead"));
                    models = stages::mask::model_ids(&self.config.masking.sections, self.config);
                }
                Supplied::Missing { searched } => self.log(Level::Info, &format!(
                    "no custom mask under derivatives (tool: {tool}) for {searched} — computing")),
            }
        }
        self.log(Level::Info, &format!("Creating mask ({} section(s), combined with {})",
            self.config.masking.sections.len(), self.config.masking.combine));
        let mask = self.build_mask(&self.config.masking.sections.clone(), &models)?;
        self.put(Item::BrainMask, Data::Mask(&mask))?;
        let n = self.n_echoes();
        let inputs: Vec<Item> = (1..=n).map(Item::Magnitude).filter(|i| self.store.has(i)).collect();
        self.complete(Completed {
            step: "mask", algorithm: None, params: &params, outcome: None, key: &key,
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
            step: &name, algorithm: None, params: &params, outcome: None, key: &key,
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
            step: "unwrap", algorithm: Some(&algorithm), params: &params, outcome: None, key: &key,
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
            self.log(Level::Info, &format!("Background removal ({algorithm}){label}"));
            let local = stages::background::remove_background(&field, &mask, &self.meta, self.config, self.observer)?;
            (local, vec![Item::FieldPpm, mask_item])
        };
        self.put(Item::LocalField(pass), Data::Real(&local.local_field_ppm))?;
        self.put(Item::LocalFieldMask(pass), Data::Mask(&local.mask))?;
        let label = if iqfm { format!("Background removal (iQFM){label}") } else { format!("Background removal ({algorithm}){label}") };
        self.complete(Completed {
            step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key,
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
        self.log(Level::Info, &format!("Dipole inversion ({algorithm}){label}"));
        let chi = stages::inversion::invert(&field, &mask, magnitude.as_deref(), &self.meta, self.config, self.observer)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        self.complete(Completed {
            step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key,
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
            step: &name, algorithm: Some(algorithm), params: &params, outcome: None, key: &key,
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
            step: &name, algorithm: Some("tgv"), params: &params, outcome: None, key: &key,
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
            step: &name, algorithm: Some("qsmart"), params: &params, outcome: None, key: &key,
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
            step: "twopass", algorithm: None, params: &params, outcome: None, key: &key,
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
                if !self.store.has(&Item::Segmentation) {
                    return Err(crate::EngineError::Config(format!(
                        "--qsm-reference {spec}: no segmentation to measure the region on. Enable \
                         --do-segmentation, or supply one with --use-custom-dseg")).into());
                }
                inputs.push(Item::Segmentation);
                Some(self.store.real(&Item::Segmentation)?)
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
            step: &name, algorithm: Some(&method), params: &params, outcome: None, key: &key,
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
            step: "swi", algorithm: Some("clear-swi"), params: &params, outcome: None, key: &key,
            inputs: &inputs, outputs: &[Item::Swi, Item::SwiMip],
        }, "SWI")
    }

    /// SMWI, weighted by the final, referenced susceptibility map over the voxels it is defined on
    /// (outside them χ is 0 by construction, not by measurement).
    fn smwi(&mut self) -> Result<(), S::Error> {
        let params = stages::swi::smwi_params(self.config, &self.meta)?;
        if !self.store.has(&Item::Chi) {
            self.log(Level::Warn, "Skipping SMWI: no susceptibility map");
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
            step: "smwi", algorithm: Some("smwi"), params: &params, outcome: None, key: &key,
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
            step: "t2star_r2star", algorithm: Some("arlo"), params: &params, outcome: None, key: &key,
            inputs: &[mask_item], outputs: &[Item::R2star, Item::T2star],
        }, "T2*/R2* mapping")
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
mod tests {
    use super::*;
    use crate::enums::{BfAlgorithm, QsmAlgorithm};
    use crate::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
    use crate::store::{Contrast, Header, MemoryStore};

    const N: usize = 16;
    const TES: [f64; 3] = [0.005, 0.010, 0.015];

    /// A sphere of tissue in a 16³ box at 1 mm, three echoes: a smooth local field inside it
    /// (wrapped into the phase at each echo time) and a magnitude that is bright inside, dark out.
    fn phantom() -> (ScanFacts, MemoryStore) {
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
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
    fn config() -> PipelineConfig {
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
        let r = Run { plan: &crate::plan::plan(&config, &facts), config: &config,
                          meta: crate::stages::magnitude::tests::meta_4x4x4(), store: &mut store, observer: &mut () };
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

    /// A step the engine does not run yet is handed to the store, which says so if it cannot.
    #[test]
    fn host_steps_are_the_stores() {
        let (facts, mut store) = phantom();
        let mut config = config();
        config.pipeline.do_r2map = true;
        let plan = crate::plan::plan(&config, &facts);
        assert!(plan.steps.iter().any(|s| s.stage == Stage::R2 && s.host));
        // As a front end hands it on: each step's stage, models and who runs it.
        let json = serde_json::to_value(&plan).unwrap();
        assert!(json["steps"].as_array().unwrap().iter().any(|s| s["stage"] == "R2" && s["host"] == true), "{json}");
        let err = run(&plan, &config, &facts, &mut store, &mut ()).unwrap_err();
        assert!(format!("{err}").contains("r2_r2prime"), "{err}");
    }
}
