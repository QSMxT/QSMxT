//! What a run will do, decided before it starts: its steps in the order they run, the
//! deep-learning models they need, the memory it will take, and what its data cannot produce.
//!
//! [`plan`] reads only the configuration and the scan's [`ScanFacts`], so a front end can show all
//! of it before anything is computed (qsmbly: the weights to download, the memory estimate, the
//! requirement warnings). `qsmxt run` runs the steps in this order, so the plan a front end shows
//! is the run that happens.

use serde::Serialize;

use crate::bridge::{field_mapping_runs, skips_bgremove};
use crate::config::PipelineConfig;
use crate::enums::*;
use crate::requirements::{self, Check, Output};
use crate::memory::MemoryProfile;
use crate::scan::ScanFacts;
use crate::store::Item;

/// Which of a two-pass run's reconstructions a step belongs to. A single-pass run has only the
/// main one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Pass {
    /// The ordinary reconstruction, on the brain mask: the only one of a single-pass run, and the
    /// "filled mask" pass of a two-pass run.
    Main,
    /// The second pass of a two-pass run, on the reliable-phase mask.
    Reliable,
}

impl Pass {
    /// Appended to the pass's step names: nothing for the main pass, `-reliable` for the other.
    pub fn suffix(self) -> &'static str {
        match self {
            Pass::Main => "",
            Pass::Reliable => crate::cache::RELIABLE_SUFFIX,
        }
    }
}

/// A step of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Stage {
    /// The grid the run is processed on, from the first echo's header.
    Load,
    /// Every echo's phase in radians on the working grid (and, for uncombined coils or
    /// real/imaginary input, their combination or conversion first).
    ScalePhase,
    /// The combined magnitude.
    Magnitude,
    /// The brain mask (main pass), or the reliable-phase mask of a two-pass run.
    Mask(Pass),
    /// Retrospective motion correction of the echoes (run, or its earlier output forgotten when
    /// it is off).
    Motion,
    Swi,
    Segmentation,
    /// T2* and R2* (`t2star_r2star`).
    Relaxometry,
    /// R2 and R2′ (`r2_r2prime`).
    R2,
    /// The total field (`unwrap`).
    FieldMap,
    /// Background-field removal, iQFM included (`bgremove`).
    BackgroundRemoval(Pass),
    /// The dipole inversion of the local field, or iQSM/iQSM+ from phase (`invert`).
    Inversion(Pass),
    /// TGV-QSM, from the total field or echo 1's phase (`tgv`).
    Tgv(Pass),
    /// QSMART, from the total field (`qsmart`).
    Qsmart(Pass),
    /// The two passes' maps combined into one (`twopass`).
    TwoPassCombination,
    /// The final susceptibility map: the raw map referenced over its support.
    Reference,
    /// A two-pass run's single-pass map, referenced over its own support.
    SinglePassReference,
    Smwi,
    ChiSeparation,
    /// Per-structure statistics over the segmentation.
    Analysis,
    /// The outputs returned to the acquired grid, after axial resampling.
    OutputSpace,
}

impl Stage {
    /// The step's name in a front end's cache and in provenance (`mask`, `invert-reliable`, ...).
    pub fn name(&self) -> String {
        let (base, pass) = match *self {
            Stage::Load => ("load", Pass::Main),
            Stage::ScalePhase => ("scale_phase", Pass::Main),
            Stage::Magnitude => ("magnitude", Pass::Main),
            Stage::Mask(p) => ("mask", p),
            Stage::Motion => ("motion", Pass::Main),
            Stage::Swi => ("swi", Pass::Main),
            Stage::Segmentation => ("segmentation", Pass::Main),
            Stage::Relaxometry => ("t2star_r2star", Pass::Main),
            Stage::R2 => ("r2_r2prime", Pass::Main),
            Stage::FieldMap => ("unwrap", Pass::Main),
            Stage::BackgroundRemoval(p) => ("bgremove", p),
            Stage::Inversion(p) => ("invert", p),
            Stage::Tgv(p) => ("tgv", p),
            Stage::Qsmart(p) => ("qsmart", p),
            Stage::TwoPassCombination => ("twopass", Pass::Main),
            Stage::Reference => ("reference", Pass::Main),
            Stage::SinglePassReference => ("reference-singlepass", Pass::Main),
            Stage::Smwi => ("smwi", Pass::Main),
            Stage::ChiSeparation => ("chi_separation", Pass::Main),
            Stage::Analysis => ("analysis", Pass::Main),
            Stage::OutputSpace => ("output_space", Pass::Main),
        };
        format!("{base}{}", pass.suffix())
    }
}

impl Stage {
    /// What the step is called where a front end lists the steps (qsmbly's plan view), the pass
    /// named for a two-pass run's second one.
    pub fn label(&self) -> String {
        let (base, pass) = match *self {
            Stage::Load => ("Load", Pass::Main),
            Stage::ScalePhase => ("Phase scaling", Pass::Main),
            Stage::Magnitude => ("Combined magnitude", Pass::Main),
            Stage::Mask(Pass::Main) => ("Brain mask", Pass::Main),
            Stage::Mask(Pass::Reliable) => ("Reliable-phase mask", Pass::Main),
            Stage::Motion => ("Motion correction", Pass::Main),
            Stage::Swi => ("SWI", Pass::Main),
            Stage::Segmentation => ("Segmentation", Pass::Main),
            Stage::Relaxometry => ("T2* and R2*", Pass::Main),
            Stage::R2 => ("R2 and R2′", Pass::Main),
            Stage::FieldMap => ("Field map", Pass::Main),
            Stage::BackgroundRemoval(p) => ("Background-field removal", p),
            Stage::Inversion(p) => ("Dipole inversion", p),
            Stage::Tgv(p) => ("TGV-QSM", p),
            Stage::Qsmart(p) => ("QSMART", p),
            Stage::TwoPassCombination => ("Two-pass combination", Pass::Main),
            Stage::Reference => ("Referencing", Pass::Main),
            Stage::SinglePassReference => ("Single-pass referencing", Pass::Main),
            Stage::Smwi => ("SMWI", Pass::Main),
            Stage::ChiSeparation => ("χ-separation", Pass::Main),
            Stage::Analysis => ("Region statistics", Pass::Main),
            Stage::OutputSpace => ("Return to the acquired grid", Pass::Main),
        };
        match pass {
            Pass::Main => base.to_string(),
            Pass::Reliable => format!("{base} (reliable pass)"),
        }
    }
}

/// One step of a [`Plan`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub stage: Stage,
    /// The deep-learning models the step runs, by QSM.rs model id: the weights to have ready
    /// before it is computed.
    pub models: Vec<&'static str>,
    /// [`Stage::name`]: what the observer's events and the cache call the step.
    pub name: String,
    /// [`Stage::label`]: what a front end calls it.
    pub label: String,
    /// The algorithm it runs, as the observer reports it (`vsharp`, `rts`), for a step with a
    /// choice of one.
    pub algorithm: Option<String>,
    /// That algorithm's name as the methods text gives it ("V-SHARP", "RTS").
    pub algorithm_label: Option<String>,
    /// The items it reads ([`crate::pipeline_graph`] draws an edge from each one's writer).
    pub reads: Vec<Item>,
    /// The items it writes: its outputs, each described by [`Item::meta`].
    pub outputs: Vec<Item>,
    /// The memory it takes at its peak, in bytes, on the host and store the plan was made for
    /// ([`Plan::memory_profile`]; [`crate::memory::estimate`]).
    pub memory_bytes: u64,
}

impl Step {
    /// A step of `stage` running `models`, its name and label filled in; what it runs, reads
    /// and writes is filled in by [`plan`], which knows the rest of the run.
    pub fn new(stage: Stage, models: Vec<&'static str>) -> Self {
        Step {
            stage, models, name: stage.name(), label: stage.label(),
            algorithm: None, algorithm_label: None, reads: Vec::new(), outputs: Vec::new(), memory_bytes: 0,
        }
    }
}

/// What a run will do.
/// Serialises (as front ends pass it on, e.g. to JavaScript) with each step as its stage and
/// models.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    /// Every step, in the order the run takes them.
    pub steps: Vec<Step>,
    /// Every model any step needs, each once, in the order they are first needed.
    pub models: Vec<&'static str>,
    /// The run's estimated peak memory: the largest of its steps' [`Step::memory_bytes`].
    pub peak_memory_bytes: u64,
    /// The name of the step that sets [`Self::peak_memory_bytes`], when the run computes anything.
    pub peak_memory_step: Option<String>,
    /// The host and store the memory figures are for.
    pub memory_profile: MemoryProfile,
    /// The most the run can take on that host, when there is a hard limit (a browser's WASM heap).
    pub memory_budget_bytes: Option<u64>,
    /// The step whose estimate exceeds the budget first, when one does. The same is among
    /// [`Self::check`]'s warnings.
    pub over_budget: Option<OverBudget>,
    /// What the scan's data cannot produce of what was enabled ([`requirements::scan_check`]):
    /// errors for outputs asked for, warnings for outputs another option implied (the run goes
    /// ahead without those).
    pub check: Check,
    /// Where the run departs from the configuration for this scan, for the log: an option this
    /// scan's inputs rule out.
    pub notes: Vec<String>,
    /// What normalising the configuration changed ([`PipelineConfig::normalised`]): the outputs
    /// switched on because another output needs them, each with its reason.
    pub adjustments: Vec<crate::normalise::Adjustment>,
    /// The configuration the plan was made from, normalised: what the run takes.
    pub config: PipelineConfig,
    /// Two-pass artefact reduction runs.
    pub two_pass: bool,
    /// The echoes the stages after masking read are motion-corrected, and so is the mask they work
    /// in (the voxels every echo still covers).
    pub motion_corrected: bool,
}

/// A step estimated to need more memory than the host has ([`Plan::over_budget`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverBudget {
    /// The step's name ([`Stage::name`]).
    pub step: String,
    pub bytes: u64,
    pub budget_bytes: u64,
    /// The settings that bring its memory down, by config path
    /// ([`crate::memory::reducing_settings`]).
    pub settings: Vec<&'static str>,
}

impl Plan {
    /// Whether the run computes anything after loading the scan.
    pub fn processes(&self) -> bool {
        self.steps.iter().any(|s| s.stage != Stage::Load)
    }

    /// Whether the plan has `stage`.
    pub fn has(&self, stage: Stage) -> bool {
        self.steps.iter().any(|s| s.stage == stage)
    }
}

/// `id`, if it names a model in QSM.rs's registry (a deep-learning algorithm): every algorithm
/// option is named after its model, and the classical ones are not in the registry.
fn model(id: &str) -> Option<&'static str> {
    qsm_core::models::find_model(id).map(|m| m.id)
}

/// The steps of one reconstruction pass: background removal and dipole inversion, by whichever
/// route the chosen algorithm takes. Both passes of a two-pass run take the same route.
fn reconstruction(config: &PipelineConfig, pass: Pass) -> Vec<Step> {
    let inversion = &config.inversion.algorithm;
    let single = |stage: Stage, models: Vec<&'static str>| vec![Step::new(stage, models)];
    match inversion {
        QsmAlgorithm::Tgv => single(Stage::Tgv(pass), vec![]),
        QsmAlgorithm::Qsmart => single(Stage::Qsmart(pass), vec![]),
        QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => single(
            Stage::Inversion(pass),
            crate::stages::inversion::iqsm_algorithm(config).and_then(model).into_iter().collect(),
        ),
        _ => {
            let mut steps = Vec::new();
            // The total-field methods (TFI, AutoQSM, NeXtQSM) remove or model the
            // background themselves, so there is no separate step for it.
            if !skips_bgremove(config) {
                let bf = config.bg_removal.algorithm;
                let id = if bf == BfAlgorithm::Iqfm { model("iqfm") } else { model(&bf.to_string()) };
                steps.push(Step::new(Stage::BackgroundRemoval(pass), id.into_iter().collect()));
            }
            steps.push(Step::new(Stage::Inversion(pass), model(&inversion.to_string()).into_iter().collect()));
            steps
        }
    }
}

/// Whether R2′ may come from R2PRIMEnet on this scan: computed rather than supplied, not
/// certain to be measured, and allowed at this field strength. Asked for by name it is predicted;
/// under `auto` it is when there is no R2 to subtract, and also when the R2 is to come from a MESE,
/// which can still prove unusable when it is read (an unreadable echo, a grid that shares no
/// space with the run's): the step then predicts R2′ instead, so its weights are listed.
fn may_predict_r2prime(facts: &ScanFacts, config: &PipelineConfig) -> bool {
    let s = &config.separation;
    if s.custom_r2prime_tool.is_some() || s.r2prime_strategy == R2PrimeStrategy::Mese {
        return false;
    }
    let certainly_measured = s.custom_r2_tool.is_some() && requirements::r2_problem(facts, config).is_none();
    let predicts = s.r2prime_strategy == R2PrimeStrategy::R2primenet || !certainly_measured;
    predicts && requirements::r2primenet_off_field(
        facts.field_strength, s.r2prime_strategy, s.r2primenet.ignore_field_mismatch).is_none()
}

/// Plan one run of `config` on the scan `facts` describes, as `qsmxt run` runs it: its memory
/// figures are native, with the volumes in files ([`MemoryProfile::NATIVE`]).
///
/// `config` is normalised first ([`PipelineConfig::normalised`]), so it may be given as the user
/// left it: the plan is that of the configuration `qsmxt run` would take, which it holds
/// ([`Plan::config`]), with what was changed ([`Plan::adjustments`]).
pub fn plan(config: &PipelineConfig, facts: &ScanFacts) -> Plan {
    plan_for(config, facts, &MemoryProfile::NATIVE)
}

/// [`plan`], with the memory figures for `profile`'s host and store (qsmbly:
/// [`MemoryProfile::browser`]). Where the host has a limit and a step's estimate exceeds it, the
/// plan names the step ([`Plan::over_budget`]) and warns of it in [`Plan::check`].
pub fn plan_for(config: &PipelineConfig, facts: &ScanFacts, profile: &MemoryProfile) -> Plan {
    let (normalised, adjustments) = config.normalised();
    let config = &normalised;
    let p = &config.pipeline;
    let wanted = |o: Output| requirements::wanted(facts, config, o);
    let relaxometry = wanted(Output::T2star) || wanted(Output::R2star);
    let needs_mask = p.do_qsm || p.do_swi || p.do_smwi || relaxometry
        || p.do_r2map || p.do_r2primemap || p.do_chi_separation;
    let needs_phase = needs_mask || p.do_qsm || p.do_segmentation || p.do_analysis;

    let mut notes = Vec::new();
    // Two-pass derives its reliable mask from a mask recipe, so a supplied mask rules it out: a
    // mask read off disk has no recipe to leave unfilled, and two-pass would reconstruct twice
    // from the same mask and combine a map with itself. Without one found, the mask is computed
    // and two-pass is fine.
    let supplied_mask = config.masking.custom_mask_tool.is_some() && facts.custom_mask;
    let two_pass = p.do_qsm && config.masking.two_pass && !supplied_mask;
    if p.do_qsm && config.masking.two_pass && supplied_mask {
        notes.push(format!(
            "Ignoring two-pass artefact reduction: the mask is supplied (--use-custom-masks {}), \
             so there is no recipe to derive a reliable-phase mask from",
            config.masking.custom_mask_tool.as_deref().unwrap_or_default()));
    }
    let motion_corrected = config.motion_correction.enabled && facts.n_echoes > 1 && facts.has_magnitude;

    let mut steps = vec![Step::new(Stage::Load, vec![])];
    let mut add = |stage: Stage, models: Vec<&'static str>| steps.push(Step::new(stage, models));
    if needs_phase {
        add(Stage::ScalePhase, vec![]);
        if facts.has_magnitude {
            add(Stage::Magnitude, vec![]);
        }
        if needs_mask {
            let models = if supplied_mask {
                vec![]
            } else {
                crate::stages::mask::model_ids(&config.masking.sections, config)
            };
            add(Stage::Mask(Pass::Main), models);
            add(Stage::Motion, vec![]);
        }
        if wanted(Output::Swi) {
            add(Stage::Swi, vec![]);
        }
        // Before reconstruction: a region reference is measured on the parcellation.
        if wanted(Output::Segmentation) {
            let models = if facts.custom_dseg { vec![] } else {
                model(config.segmentation.version.weights_id()).into_iter().collect()
            };
            add(Stage::Segmentation, models);
        }
        if relaxometry {
            add(Stage::Relaxometry, vec![]);
        }
        if p.do_r2map || p.do_r2primemap {
            let predicts = p.do_r2primemap && may_predict_r2prime(facts, config);
            add(Stage::R2, if predicts { model("r2primenet").into_iter().collect() } else { vec![] });
        }
        if p.do_qsm {
            // The field map is computed once, on the brain mask, and shared by both passes.
            if field_mapping_runs(config, facts.n_echoes) {
                add(Stage::FieldMap, vec![]);
            }
            for step in reconstruction(config, Pass::Main) {
                add(step.stage, step.models);
            }
            if two_pass {
                let sections = config.masking.resolved_two_pass_sections();
                add(Stage::Mask(Pass::Reliable), crate::stages::mask::model_ids(&sections, config));
                for step in reconstruction(config, Pass::Reliable) {
                    add(step.stage, step.models);
                }
                add(Stage::TwoPassCombination, vec![]);
                add(Stage::SinglePassReference, vec![]);
            }
            add(Stage::Reference, vec![]);
        }
        // After referencing: SMWI weights by the final, referenced map.
        if wanted(Output::Smwi) {
            add(Stage::Smwi, vec![]);
        }
        if p.do_chi_separation {
            add(Stage::ChiSeparation, model(&config.separation.algorithm.to_string()).into_iter().collect());
        }
        // Last of the map-producing stages: it summarises every map the run made.
        if p.do_analysis {
            add(Stage::Analysis, vec![]);
        }
        add(Stage::OutputSpace, vec![]);
    }

    let mut models: Vec<&'static str> = Vec::new();
    for id in steps.iter().flat_map(|s| s.models.iter()) {
        if !models.contains(id) {
            models.push(id);
        }
    }
    let mut plan = Plan {
        steps,
        models,
        peak_memory_bytes: 0,
        peak_memory_step: None,
        memory_profile: *profile,
        memory_budget_bytes: profile.budget_bytes(),
        over_budget: None,
        check: requirements::scan_check(facts, config),
        notes,
        adjustments,
        config: config.clone(),
        two_pass,
        motion_corrected,
    };
    let details = crate::pipeline_graph::step_details(&plan, config, facts);
    for (step, d) in plan.steps.iter_mut().zip(details) {
        (step.algorithm, step.algorithm_label, step.reads, step.outputs) = (d.algorithm, d.algorithm_label, d.reads, d.writes);
    }
    estimate_memory(&mut plan, facts, profile);
    plan
}

/// Fill in the plan's memory figures, and warn of the first step that exceeds the host's budget.
fn estimate_memory(plan: &mut Plan, facts: &ScanFacts, profile: &MemoryProfile) {
    let estimate = crate::memory::estimate(plan, facts, profile);
    for (step, bytes) in plan.steps.iter_mut().zip(&estimate.steps) {
        step.memory_bytes = *bytes;
    }
    plan.peak_memory_bytes = estimate.peak_bytes;
    plan.peak_memory_step = estimate.peak_step.map(|i| plan.steps[i].name.clone());
    let Some(budget) = plan.memory_budget_bytes else { return };
    let Some(step) = plan.steps.iter().find(|s| s.memory_bytes > budget) else { return };
    let settings = crate::memory::reducing_settings(step, &plan.config);
    let fmt = crate::memory::format_bytes;
    let message = format!("{} is estimated to need {} of memory, more than the {} a browser run has",
                          step.label, fmt(step.memory_bytes), fmt(budget));
    plan.check.warnings.push(message.clone());
    // Not one setting's problem: the whole sentence is the reason, and the settings that bring
    // the step down are listed.
    plan.check.problems.push(requirements::Problem {
        severity: requirements::Severity::Warning, setting: None, implied_by: None,
        settings: settings.clone(), reason: message.clone(), message,
    });
    plan.over_budget = Some(OverBudget { step: step.name.clone(), bytes: step.memory_bytes, budget_bytes: budget, settings });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 5-echo, 3 T scan with magnitude: what the example data looks like.
    pub(crate) fn facts() -> ScanFacts {
        ScanFacts {
            dims: (16, 16, 8),
            voxel_size: None,
            n_echoes: 5,
            echo_times: vec![0.004, 0.008, 0.012, 0.016, 0.020],
            has_magnitude: true,
            n_coils: 1,
            real_imaginary: false,
            field_strength: 3.0,
            b0_dir: None,
            mese_echo_times: None,
            custom_mask: false, custom_dseg: false, custom_qsm: false, custom_r2: false,
            custom_r2prime: false,
        }
    }

    fn names(plan: &Plan) -> Vec<String> {
        plan.steps.iter().map(|s| s.stage.name()).collect()
    }

    #[test]
    fn the_default_run_maps_the_field_removes_the_background_and_inverts() {
        let plan = plan(&PipelineConfig::default(), &facts());
        assert_eq!(names(&plan), ["load", "scale_phase", "magnitude", "mask", "motion", "unwrap",
                                  "bgremove", "invert", "reference", "output_space"]);
        assert!(plan.models.is_empty(), "the defaults are classical: {:?}", plan.models);
        assert!(plan.check.errors.is_empty() && plan.check.warnings.is_empty());
        assert!(plan.peak_memory_bytes > 0);
        assert!(!plan.two_pass && !plan.motion_corrected);
    }

    /// MEDI's background removal is PDF, its reference pipeline's, unless one is chosen; a chosen
    /// one is kept, the general default included.
    #[test]
    fn medi_removes_the_background_with_pdf_unless_another_is_chosen() {
        use crate::enums::BfAlgorithm;
        let bfr = |c: &PipelineConfig| {
            let p = plan(c, &facts());
            p.steps.iter().find(|s| s.name == "bgremove").and_then(|s| s.algorithm.clone())
        };
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert_eq!(bfr(&config).as_deref(), Some("pdf"));
        config.inversion.medi.smv = false;
        assert_eq!(bfr(&config).as_deref(), Some("pdf"));
        config.bg_removal.algorithm_chosen = true;
        assert_eq!(bfr(&config).as_deref(), Some("vsharp"), "the general default, chosen");
        config.bg_removal.algorithm = BfAlgorithm::Lbv;
        config.bg_removal.algorithm_chosen = false;
        assert_eq!(bfr(&config).as_deref(), Some("lbv"));
        // Other inversions keep the general default.
        assert_eq!(bfr(&PipelineConfig::default()).as_deref(), Some("vsharp"));
    }

    #[test]
    fn two_pass_repeats_the_reconstruction_and_references_both_maps() {
        let mut config = PipelineConfig::default();
        config.masking.two_pass = true;
        config.inversion.algorithm = QsmAlgorithm::Tgv;
        let p = plan(&config, &facts());
        assert_eq!(p.steps[8].stage.label(), "TGV-QSM (reliable pass)");
        assert_eq!(names(&p)[5..], ["unwrap", "tgv", "mask-reliable", "tgv-reliable", "twopass",
                                    "reference-singlepass", "reference", "output_space"]);
        // Every step name is one the invalidation table knows.
        for step in &p.steps {
            let name = step.stage.name();
            let base = name.strip_suffix(crate::cache::RELIABLE_SUFFIX).unwrap_or(&name);
            assert!(crate::graph::downstream_steps(base).is_some() || base == "twopass", "{name}");
        }

        // A supplied mask has no recipe for the reliable pass.
        config.masking.custom_mask_tool = Some("bet".into());
        let mut f = facts();
        f.custom_mask = true;
        let p = plan(&config, &f);
        assert!(!p.two_pass && !p.has(Stage::Mask(Pass::Reliable)));
        assert_eq!(p.notes.len(), 1, "{:?}", p.notes);
        // ...unless none was found, when the mask is computed after all.
        f.custom_mask = false;
        assert!(plan(&config, &f).two_pass);
    }

    #[test]
    fn deep_learning_steps_name_their_models() {
        let mut config = PipelineConfig::default();
        config.bg_removal.algorithm = BfAlgorithm::Iqfm;
        config.inversion.algorithm = QsmAlgorithm::Rts;
        let p = plan(&config, &facts());
        let bfr = p.steps.iter().find(|s| s.stage == Stage::BackgroundRemoval(Pass::Main)).unwrap();
        assert_eq!(bfr.models, ["iqfm"]);
        assert_eq!(p.models, ["iqfm"]);

        config.inversion.algorithm = QsmAlgorithm::Iqsm;
        let p = plan(&config, &facts());
        assert!(!p.has(Stage::BackgroundRemoval(Pass::Main)) && !p.has(Stage::FieldMap));
        assert_eq!(p.models, ["iqsm"]);

        config.inversion.algorithm = QsmAlgorithm::Xqsm;
        config.bg_removal.algorithm = BfAlgorithm::Vsharp;
        config.pipeline.do_segmentation = true;
        let p = plan(&config, &facts());
        assert_eq!(p.models, [config.segmentation.version.weights_id(), "xqsm"],
                   "in the order the run needs them");
    }

    /// A requested output the scan cannot produce is the plan's error, worded as `qsmxt run`
    /// words it; nothing about it reaches the steps.
    #[test]
    fn what_the_data_cannot_produce_is_reported_not_planned() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_swi = true;
        let mut f = facts();
        f.has_magnitude = false;
        let p = plan(&config, &f);
        assert!(!p.has(Stage::Swi) && !p.has(Stage::Magnitude));
        assert_eq!(p.check.errors, ["--do-swi needs magnitude images, and this run has none"]);
        // The same problem as data, naming the setting rather than the flag.
        assert_eq!(p.check.problems, [requirements::Problem {
            severity: requirements::Severity::Error, setting: Some("pipeline.do_swi"), implied_by: None,
            settings: vec!["pipeline.do_swi"],
            reason: "needs magnitude images, and this run has none".into(),
            message: "Compute SWI needs magnitude images, and this run has none".into(),
        }]);
    }

    /// R2PRIMEnet is listed wherever the R2 step may run it: R2′ asked for with no R2 to subtract,
    /// or with R2 to come from a MESE that may prove unusable, but not when R2 is supplied or the
    /// strategy only measures.
    #[test]
    fn r2primenet_is_listed_wherever_r2prime_may_be_predicted() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_r2primemap = true;
        let r2_models = |config: &PipelineConfig, f: &ScanFacts| {
            plan(config, f).steps.iter().find(|s| s.stage == Stage::R2).unwrap().models.clone()
        };
        let mut f = facts();
        assert_eq!(r2_models(&config, &f), ["r2primenet"], "no R2 to subtract");
        f.mese_echo_times = Some(vec![0.01, 0.02, 0.03, 0.04]);
        assert_eq!(r2_models(&config, &f), ["r2primenet"], "the MESE may yet give no R2");
        config.separation.custom_r2_tool = Some("mine".into());
        f.custom_r2 = true;
        assert!(r2_models(&config, &f).is_empty(), "a supplied R2 is there to subtract");
        config.separation.custom_r2_tool = None;
        config.separation.r2prime_strategy = R2PrimeStrategy::Mese;
        assert!(r2_models(&config, &f).is_empty(), "measured or nothing");
        config.separation.r2prime_strategy = R2PrimeStrategy::Auto;
        f.field_strength = 7.0;
        assert!(r2_models(&config, &f).is_empty(), "not run off its training field strength");
    }

    /// A configuration as a front end holds it (only the outputs asked for) is planned as `qsmxt
    /// run` plans it: the outputs those need are switched on, the plan says so, and it holds the
    /// configuration it planned. Planning that configuration again plans the same run.
    #[test]
    fn the_outputs_an_output_needs_are_planned_and_said() {
        let step = |p: &Plan, name: &str| p.steps.iter().find(|s| s.name == name).cloned();
        let planned = |edit: fn(&mut PipelineConfig)| {
            let mut c = PipelineConfig::default();
            edit(&mut c);
            let p = plan(&c, &facts());
            let again = plan(&p.config, &facts());
            assert_eq!(again.steps, p.steps, "the plan's own configuration plans the same run");
            assert_eq!(again.config, p.config);
            assert!(again.adjustments.is_empty(), "{:?}", again.adjustments);
            p
        };

        // R2′ alone: R2* is fitted for it and R2′ is written and returned to the acquired grid.
        let p = planned(|c| c.pipeline.do_r2primemap = true);
        assert!(p.has(Stage::Relaxometry), "{:?}", names(&p));
        assert!(step(&p, "r2_r2prime").unwrap().outputs.contains(&Item::R2prime));
        assert!(step(&p, "r2_r2prime").unwrap().reads.contains(&Item::R2star));
        assert!(step(&p, "output_space").unwrap().reads.contains(&Item::R2prime));
        assert!(p.config.pipeline.do_r2starmap && p.config.pipeline.do_r2map);
        assert_eq!(p.adjustments.iter().map(|a| a.setting).collect::<Vec<_>>(),
                   ["pipeline.do_r2starmap", "pipeline.do_r2map"]);

        // The statistics alone: a segmentation to measure them over.
        let p = planned(|c| c.pipeline.do_analysis = true);
        assert!(p.has(Stage::Segmentation) && p.has(Stage::Analysis), "{:?}", names(&p));
        assert_eq!(p.adjustments[0].because, "pipeline.do_analysis");

        // ...and, with QSM off and nothing else to summarise, QSM.
        let p = planned(|c| { c.pipeline.do_qsm = false; c.pipeline.do_analysis = true; });
        assert!(p.has(Stage::Inversion(Pass::Main)) && p.has(Stage::Segmentation), "{:?}", names(&p));

        // SMWI with QSM off: the susceptibility map it weights by is reconstructed.
        let p = planned(|c| { c.pipeline.do_qsm = false; c.pipeline.do_smwi = true; });
        assert!(p.has(Stage::Inversion(Pass::Main)) && p.has(Stage::Reference) && p.has(Stage::Smwi),
                "{:?}", names(&p));
        assert_eq!(p.adjustments[0].setting, "pipeline.do_qsm");

        // χ-separation: the maps its method reads.
        let p = planned(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::R2starQsm; });
        assert!(p.has(Stage::Relaxometry) && !p.has(Stage::R2), "{:?}", names(&p));
        let p = planned(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr; });
        assert!(p.has(Stage::Relaxometry) && p.has(Stage::R2) && p.has(Stage::ChiSeparation), "{:?}", names(&p));
        assert!(step(&p, "chi_separation").unwrap().reads.contains(&Item::R2prime));

        // A region reference: the segmentation it is measured on, before the reference.
        let p = planned(|c| { c.qsm.reference = QsmReference::Region; c.qsm.reference_region = Some("thalamus".into()); });
        let at = |name: &str| p.steps.iter().position(|s| s.name == name).unwrap();
        assert!(at("segmentation") < at("reference"), "{:?}", names(&p));
        assert_eq!(p.adjustments[0].because, "qsm.reference");
    }

    #[test]
    fn nothing_enabled_loads_the_scan_and_stops() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_qsm = false;
        let p = plan(&config, &facts());
        assert!(!p.processes());
        assert_eq!(names(&p), ["load"]);
    }
}
