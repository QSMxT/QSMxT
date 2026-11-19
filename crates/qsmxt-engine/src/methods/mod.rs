use crate::config::*;
use crate::enums::*;
use crate::masking::*;
use crate::plan::{Pass, Plan, Stage};
use crate::record::{Choice, DatasetRecord, RunRecord, Topic};
use serde::{Deserialize, Serialize};

// The framework (the Draft, its ordering, the Note, the "only non-defaults" helpers) is here;
// each stage's prose is in its own file, and `draft` puts the paragraph together in run order.
mod background;
mod citations;
mod draft;
mod field_map;
mod grid;
mod inversion;
mod masking;
mod qsmart;
mod separation;
mod two_pass;

use background::*;
use citations::*;
use draft::*;
use field_map::*;
use grid::*;
use inversion::*;
use masking::*;
use qsmart::*;
use separation::*;
use two_pass::*;

pub(crate) use inversion::tiled_phase_network;
pub(crate) use separation::separation_name;

/// The voxels a susceptibility map is defined on (`referencing::defined_support` in the main
/// crate), named once in full and as "the reconstruction mask" after that.
const RECON_MASK: &str =
    "the reconstruction mask (the brain mask less any rim eroded by background field removal)";
const RECON_MASK_AGAIN: &str = "the reconstruction mask";

// ─── Settings that differ from the defaults ───
//
// A methods sentence names its method and cites it. A parameter value is stated only when the
// run's setting differs from QSMxT's default for it: the defaults are what the citation and the
// QSMxT version already pin down, and listing them buries the choices the user actually made.

/// Whether two floats differ by more than rounding.
fn differs(a: f64, b: f64) -> bool {
    (a - b).abs() > 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// The non-default settings of one sentence, as prose fragments ("deconvolution enabled").
#[derive(Default)]
struct Changed(Vec<String>);

impl Changed {
    /// Add `text` when `differs` holds.
    fn add(&mut self, differs: bool, text: impl FnOnce() -> String) {
        if differs {
            self.0.push(text());
        }
    }

    /// Add every field of `cfg` that differs from `default` (see [`changed_fields`]).
    fn add_fields<T: serde::Serialize>(&mut self, cfg: &T, default: &T, skip: &[&str]) {
        self.0.extend(changed_fields(cfg, default, skip));
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `" with a, b and c"`, or nothing.
    fn with(&self) -> String {
        self.joined(" with ")
    }

    /// `", with a, b and c"`, or nothing — for the end of a sentence that already has commas.
    fn comma_with(&self) -> String {
        self.joined(", with ")
    }

    /// `"; a, b and c"`, or nothing — for inside a citation's parentheses.
    fn semi(&self) -> String {
        self.joined("; ")
    }

    /// `" (a, b and c)"`, or nothing.
    fn paren(&self) -> String {
        if self.0.is_empty() { String::new() } else { format!(" ({})", join_list(&self.0)) }
    }

    fn joined(&self, lead: &str) -> String {
        if self.0.is_empty() { String::new() } else { format!("{lead}{}", join_list(&self.0)) }
    }
}

/// How a parameter reads in prose.
enum Label {
    /// `"λ = 0.001"`.
    Eq(&'static str),
    /// `"λ = 0.001"` plus a unit: `"radius = 5 mm"`.
    Unit(&'static str, &'static str),
    /// `"500 iterations"`.
    Count(&'static str),
    /// `"at most 500 iterations"`.
    AtMost(&'static str),
    /// `"merit enabled"` / `"merit disabled"`.
    Toggle(&'static str),
}

/// The prose label of an algorithm parameter, by its config field name. Anything not listed reads
/// as its field name with spaces, which is still unambiguous next to the algorithm it belongs to.
fn param_label(field: &str) -> Label {
    use Label::*;
    match field {
        "tol" | "tolerance" => Eq("tolerance"),
        "cg_tol" => Eq("conjugate-gradient tolerance"),
        "tol_update" => Eq("update tolerance"),
        "tol_delta" => Eq("convergence tolerance"),
        "max_iter" | "max_iterations" => AtMost("iterations"),
        "cg_max_iter" => AtMost("conjugate-gradient iterations"),
        "max_iter_l1" => AtMost("L1 iterations"),
        "max_iter_l2" => AtMost("L2 iterations"),
        "lsmr_iter" => Count("LSMR iterations"),
        "max_iter_newton" => AtMost("Newton iterations"),
        "n_cg" => AtMost("conjugate-gradient iterations"),
        "iterations" => Count("iterations"),
        "erosions" => Count("mask erosions"),
        "continuation_steps" => Count("continuation steps"),
        "inner_iterations" => Count("inner iterations"),
        "denoise_iterations" => Count("denoising iterations"),
        "max_evaluations" => AtMost("evaluations"),
        "max_samples" => AtMost("samples"),
        "levels" => Count("pyramid levels"),
        "chain_levels" => Count("pyramid levels for chained registrations"),
        "chain" => Toggle("chained registration"),
        "lambda" => Eq("λ"),
        "lambda1" => Eq("λ1"),
        "lambda_csf" => Eq("λ (CSF)"),
        "cg_max_iter_lsqr" => AtMost("LSQR conjugate-gradient iterations"),
        "cg_tol_lsqr" => Eq("LSQR conjugate-gradient tolerance"),
        "cg_max_iter_sa" => AtMost("streaking-removal conjugate-gradient iterations"),
        "cg_tol_sa" => Eq("streaking-removal conjugate-gradient tolerance"),
        "d2_thr" => Eq("streaking cone threshold"),
        "prc_w_i_min" => Eq("lower Laplacian-weight percentile"),
        "prc_w_i_max" => Eq("upper Laplacian-weight percentile"),
        "prc_w_g_min" => Eq("lower gradient-weight percentile"),
        "prc_w_g_max" => Eq("upper gradient-weight percentile"),
        "unwrap_phase" => Toggle("echo-phase unwrapping"),
        "max_function_evaluations" => AtMost("function evaluations"),
        "function_tolerance" => Eq("function tolerance"),
        "step_tolerance" => Eq("step tolerance"),
        "optimality_tolerance" => Eq("optimality tolerance"),
        "finite_difference_step" => Eq("finite-difference step"),
        "typical_x" => Eq("typical x"),
        "n_inner" => Count("alternating passes"),
        "chi_bound" => Unit("χ bound", "ppm"),
        "alpha" => Eq("α"),
        "alpha0" => Eq("α0"),
        "alpha1" => Eq("α1"),
        "alpha_l2" => Eq("α (L2)"),
        "beta" => Eq("β"),
        "delta" => Eq("δ"),
        "mu" => Eq("μ"),
        "mu0" => Eq("μ0"),
        "mu1" => Eq("μ1"),
        "mu2" => Eq("μ2"),
        "mu3" => Eq("μ3"),
        "mu1_l2" => Eq("μ1 (L2)"),
        "muh" => Eq("μ (harmonic)"),
        "mu_min" => Eq("minimum μ"),
        "rho" => Eq("ρ"),
        "tau" => Eq("τ"),
        "gamma" => Eq("γ (ρ increase factor)"),
        "tol_newton" => Eq("Newton tolerance"),
        "weight" => Eq("data weight"),
        "space" => Eq("dipole kernel"),
        "n_pad" => Unit("zero-padding", "voxels"),
        "pad" => Unit("zero-padding", "voxels per side"),
        "threshold" => Eq("threshold"),
        "radius" | "smv_radius" => Unit("SMV radius", "mm"),
        "tik_reg" => Eq("Tikhonov weight"),
        "smv" => Toggle("SMV preprocessing"),
        "smv_shrink_mask" => Toggle("SMV mask shrinking"),
        "merit" => Toggle("MERIT"),
        "linear" => Toggle("the linear data term"),
        "zeropad" => Unit("zero-padding", "voxels at the end of each axis"),
        "reltol" => Eq("relative tolerance"),
        "percentage" => Eq("gradient-mask percentage"),
        "data_weighting" => Eq("data weighting mode"),
        "precond" => Eq("preconditioner weight"),
        "step_size" => Eq("step size"),
        "reg" => Eq("regularization operator"),
        "pad_mm" | "ilsqr_pad_mm" => Unit("zero-padding", "mm per side"),
        "dr_pos" => Eq("Dr+"),
        "dr_neg" => Eq("Dr−"),
        "dr_pos_3t" => Eq("Dr+ at 3 T"),
        "r_const_3t" => Eq("relaxometric constant at 3 T"),
        "sigma_mm" => Unit("σ", "mm"),
        "nbox" => Count("segmentation boxes per dimension"),
        "shrink_factor" => Eq("shrink factor"),
        "convergence_threshold" => Eq("convergence threshold"),
        "mesh" => Eq("initial B-spline mesh"),
        "spline_order" => Eq("B-spline order"),
        "bias_field_fwhm" => Eq("bias field FWHM"),
        "wiener_noise" => Eq("Wiener filter noise"),
        "histogram_bins" => Count("histogram bins"),
        "estimate_offset" => Toggle("phase-offset estimation"),
        "denoise" => Toggle("anisotropic-diffusion denoising"),
        "rotation_step_deg" => Unit("initial rotation step", "°"),
        "translation_step_mm" => Unit("initial translation step", "mm"),
        "min_overlap" => Eq("minimum overlap"),
        "crop" => Unit("centre crop", "voxels"),
        "topology_cleanup" => Toggle("topology cleanup"),
        "sigma_smoothing" => Unit("posterior smoothing σ", "voxels"),
        "patch" => Unit("patch", "voxels"),
        _ => Eq(""),
    }
}

/// A JSON value as prose: numbers as written, arrays as `a × b × c`, `null` as "automatic".
fn json_prose(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "automatic".to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(json_prose).collect::<Vec<_>>().join(" × "),
        serde_json::Value::Number(n) if !n.is_i64() && !n.is_u64() => {
            n.as_f64().map_or_else(|| n.to_string(), |f| format!("{f}"))
        }
        other => other.to_string(),
    }
}

fn json_differs(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => differs(x, y),
        _ => a != b,
    }
}

/// The fields of `cfg` that differ from `default`, as prose (`"λ = 0.002"`, `"at most 50
/// iterations"`). Works on any serializable parameter struct, so every algorithm's settings are
/// covered without a sentence of its own; `skip` names fields the caller words itself.
fn changed_fields<T: serde::Serialize>(cfg: &T, default: &T, skip: &[&str]) -> Vec<String> {
    let (Ok(serde_json::Value::Object(c)), Ok(serde_json::Value::Object(d))) =
        (serde_json::to_value(cfg), serde_json::to_value(default))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, v) in &c {
        if skip.contains(&k.as_str()) || d.get(k).is_some_and(|dv| !json_differs(v, dv)) {
            continue;
        }
        let value = json_prose(v);
        out.push(match param_label(k) {
            Label::Eq("") if v.is_boolean() => format!("{} {}", k.replace('_', " "),
                if v.as_bool() == Some(true) { "enabled" } else { "disabled" }),
            Label::Eq("") => format!("{} = {value}", k.replace('_', " ")),
            Label::Eq(l) => format!("{l} = {value}"),
            Label::Unit(l, "°") => format!("{l} = {value}°"),
            Label::Unit(l, u) => format!("{l} = {value} {u}"),
            Label::Count(noun) => format!("{value} {noun}"),
            Label::AtMost(noun) => format!("at most {value} {noun}"),
            Label::Toggle(l) => format!("{l} {}", if v.as_bool() == Some(true) { "enabled" } else { "disabled" }),
        });
    }
    out
}

/// The non-default rigid-registration settings, shared by motion correction and
/// multi-orientation co-registration.
fn registration_changes(config: &PipelineConfig) -> Changed {
    let mut c = Changed::default();
    c.add_fields(&config.registration, &crate::motion::RegistrationConfig::default(), &[]);
    c
}

/// One sentence of the methods paragraph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sentence {
    pub text: String,
    /// The steps it describes, by the ids [`crate::pipeline_graph::pipeline_graph`] gives their
    /// boxes (`mask`, `invert-reliable`): several where one method does the work of several
    /// steps (TGV removes the background and inverts the field), none for a sentence about the
    /// run as a whole.
    pub steps: Vec<String>,
    /// The works it cites, by [`Reference::key`].
    pub citations: Vec<String>,
}

/// A work the methods text cites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    /// A short name for it (`dymerska2021`).
    pub key: String,
    /// The reference as the list gives it, in Markdown.
    pub text: String,
}

/// The methods text sentence by sentence, each with the steps it describes: what a front end
/// draws beside the pipeline graph, so that a box and the sentences about it can point at each
/// other. [`Methods::to_markdown`] is the text [`generate_methods_for`] writes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Methods {
    pub sentences: Vec<Sentence>,
    /// In the order the paragraph first cites them.
    pub references: Vec<Reference>,
    /// What the run departed from in its methods' reference pipelines, said before the references.
    #[serde(default, skip_serializing_if = "Note::is_empty")]
    pub note: Note,
}

/// The methods text's Note section: the departures from the chosen methods' recommendations and
/// reference pipelines that a run kept ([`crate::recommendations::note`]), one sentence each.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// From what the methods' authors recommend, or QSM.rs measured to be better (or a combination
    /// known to fail).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub departures: Vec<String>,
    /// From what the methods' reference pipelines used, where their authors did not say it is
    /// needed or better.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<String>,
    /// The required changes the run made itself: outputs another setting needs, turned on because
    /// the configuration left them off ([`crate::config::PipelineConfig::required_changes`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

impl Note {
    pub fn is_empty(&self) -> bool {
        self.departures.is_empty() && self.differences.is_empty() && self.required.is_empty()
    }

    /// [`Self::required`] from `config`'s record of them, each a sentence.
    fn with_required(mut self, config: &crate::config::PipelineConfig) -> Self {
        self.required = config.required_changes.iter().map(|m| format!("{m}.")).collect();
        self
    }
}

impl Methods {
    /// The paragraph: the sentences, joined by spaces.
    pub fn paragraph(&self) -> String {
        self.sentences.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    /// The paragraph under a `# Methods` heading, then the Note, if there is one, under `## Note`,
    /// then the references under `## References`.
    pub fn to_markdown(&self) -> String {
        let mut out = format!("# Methods\n\n{}\n\n", self.paragraph());
        let n = &self.note;
        if !n.is_empty() {
            out.push_str("## Note\n\n");
            for (heading, lines) in [
                ("Departures from the methods' recommendations:", &n.departures),
                ("Differences from the methods' reference pipelines:", &n.differences),
                ("Outputs the run turned on because other settings need them:", &n.required),
            ] {
                if lines.is_empty() {
                    continue;
                }
                out.push_str(&format!("{heading}\n\n"));
                for l in lines {
                    out.push_str(&format!("- {l}\n"));
                }
                out.push('\n');
            }
        }
        if !self.references.is_empty() {
            out.push_str("## References\n\n");
            for r in &self.references {
                out.push_str(&format!("- {}\n", r.text));
            }
        }
        out.truncate(out.trim_end().len());
        out.push('\n');
        out
    }
}

/// The methods text as it is written: each sentence with the steps it describes and the works
/// cited since the one before (a work is cited before the sentence that names it), and the
/// reference list.
#[derive(Default)]
struct Draft {
    sentences: Vec<(String, Vec<Stage>, Vec<&'static str>)>,
    references: Vec<Citation>,
    cited: Vec<&'static str>,
}

impl Draft {
    /// Cite `cite` in the next sentence, and list it if it is not listed yet.
    fn cite(&mut self, cite: &Citation) {
        if !self.references.iter().any(|c| c.key == cite.key) {
            self.references.push(cite.clone());
        }
        if !self.cited.contains(&cite.key) {
            self.cited.push(cite.key);
        }
    }

    /// Cite each of `cites` in the next sentence, in order.
    fn cite_all(&mut self, cites: &[Citation]) {
        for c in cites {
            self.cite(c);
        }
    }

    /// Cite the works of method `id` in `stage` ([`method`]) in the next sentence, and return
    /// them, for the sentence's in-text citation.
    fn cite_method(&mut self, stage: MethodStage, id: &str) -> Vec<Citation> {
        let cites = method(stage, id);
        self.cite_all(&cites);
        cites
    }

    /// A sentence about `steps`, citing what was cited since the last one.
    fn say(&mut self, steps: &[Stage], text: impl Into<String>) {
        self.sentences.push((text.into(), steps.to_vec(), std::mem::take(&mut self.cited)));
    }

    /// The steps each sentence names: those `plan` has and its graph draws, or none without a
    /// plan.
    fn finish(self, config: &PipelineConfig, plan: Option<&Plan>) -> Methods {
        self.finish_drawn(|stage| plan.is_some_and(|p| {
            p.has(*stage) && !crate::pipeline_graph::left_out(p, config, *stage, None)
        }))
    }

    /// The steps each sentence names: those `drawn` says the graph draws.
    fn finish_drawn(self, drawn: impl Fn(&Stage) -> bool) -> Methods {
        let sentences = self.sentences.into_iter().map(|(text, stages, cites)| {
            let mut steps: Vec<String> = Vec::new();
            for name in stages.iter().filter(|s| drawn(s)).map(Stage::name) {
                if !steps.contains(&name) {
                    steps.push(name);
                }
            }
            Sentence { text, steps, citations: cites.into_iter().map(str::to_string).collect() }
        }).collect();
        let references = self.references.iter()
            .map(|c| Reference { key: c.key.to_string(), text: c.text.to_string() })
            .collect();
        Methods { sentences, references, note: Note::default() }
    }
}

/// A step two-pass runs once per pass, in both passes.
fn both(step: fn(Pass) -> Stage) -> [Stage; 2] {
    [step(Pass::Main), step(Pass::Reliable)]
}

/// The steps that reconstruct the susceptibility map, in both passes: the ones that run in the
/// reconstruction box on the padded grid.
fn reconstruction() -> Vec<Stage> {
    [both(Stage::BackgroundRemoval), both(Stage::Inversion), both(Stage::Tgv), both(Stage::Qsmart)].concat()
}

/// The steps two-pass adds: the reliable-phase mask, the reconstruction on it, and the
/// combination of the two maps.
fn two_pass_steps() -> Vec<Stage> {
    vec![Stage::Mask(Pass::Reliable), Stage::BackgroundRemoval(Pass::Reliable), Stage::Inversion(Pass::Reliable),
         Stage::Tgv(Pass::Reliable), Stage::Qsmart(Pass::Reliable), Stage::TwoPassCombination]
}

/// Generate a methods description and citation list from a pipeline configuration.
/// Uses "qsmxt" as the tool name by default.
pub fn generate_methods(config: &PipelineConfig) -> String {
    generate_methods_for(config, "qsmxt")
}

/// Generate methods for a specific tool (e.g. "qsmxt" or "QSMbly"): [`Methods::to_markdown`].
pub fn generate_methods_for(config: &PipelineConfig, tool: &str) -> String {
    let config = &config.normalised().0;
    let mut methods = draft(config, tool, Ran::default()).finish(config, None);
    methods.note = crate::recommendations::config_note(config).with_required(config);
    methods.to_markdown()
}

/// The methods text for a run of `plan` (made by [`crate::plan::plan`] from `config` and the
/// scan's facts), sentence by sentence, each naming the steps of `plan`'s pipeline graph it
/// describes. Its [`Methods::to_markdown`] is [`generate_methods_for`]'s text.
pub fn methods_structured(config: &PipelineConfig, plan: &Plan, tool: &str) -> Methods {
    let config = &config.normalised_as_planned(plan);
    let mut methods = draft(config, tool, Ran::default()).finish(config, Some(plan));
    methods.note = crate::recommendations::config_note(config).with_required(config);
    methods
}

/// The methods text of a run of `plan` that did what `record` says ([`crate::run::run`]'s
/// record): it says what was done, as [`methods_structured`] does for the plan, where the run
/// decided something the configuration does not (a supplied mask used, or not found and computed
/// instead; R2′ measured rather than predicted; a step that had nothing to work on). A run that
/// decided nothing gets [`methods_structured`]'s text, less multi-orientation and DICOM export,
/// which are said only of a run whose record shows its group was combined or its maps exported.
pub fn methods_for_run(config: &PipelineConfig, plan: &Plan, record: &RunRecord, tool: &str) -> Methods {
    let config = &config.normalised_as_planned(plan);
    let merged = DatasetRecord::from(record);
    let mut methods = draft(config, tool, Ran(Some(&merged))).finish_drawn(|stage| {
        plan.has(*stage) && !crate::pipeline_graph::left_out(plan, config, *stage, record.resampled)
    });
    methods.note = crate::recommendations::note(&merged.recommendations, 1).with_required(config);
    methods
}

/// The methods text of a dataset's runs of `config`, merged into `record` ([`crate::record::merge`]):
/// a choice every run made is said as for one run; where the runs differ, the configuration's
/// wording ("where available") stands. Each sentence names the steps some run did something in.
pub fn methods_for_dataset(config: &PipelineConfig, record: &DatasetRecord, tool: &str) -> Methods {
    let mut config = config.normalised().0;
    // χ-sepnet's toolbox route: the R2′ maps the scans that could measure one made.
    if record.count(Topic::ChiSeparationR2prime, Choice::Measured) > 0 {
        config.pipeline.do_r2primemap = true;
        crate::config::enforce_separation_dependencies(&mut config);
    }
    let config = &config;
    let mut methods = draft(config, tool, Ran(Some(record)))
        .finish_drawn(|stage| record.step(&stage.name()).is_some_and(|s| s.ran() > 0));
    methods.note = crate::recommendations::note(&record.recommendations, record.runs).with_required(config);
    methods
}

/// What the runs the text describes did ([`DatasetRecord`]; one run's is a dataset of one). With
/// nothing known, the text says what the configuration asks for.
#[derive(Clone, Copy, Default)]
struct Ran<'a>(Option<&'a DatasetRecord>);

impl Ran<'_> {
    /// Some run decided about `topic`, and every one that did chose one of `choices`.
    fn only(&self, topic: Topic, choices: &[Choice]) -> bool {
        self.0.is_some_and(|r| r.only(topic, choices))
    }

    /// The runs the text describes (none when nothing is known of them).
    fn runs(&self) -> usize {
        self.0.map_or(0, |r| r.runs)
    }

    /// The runs that chose `choice` about `topic`.
    fn count(&self, topic: Topic, choice: Choice) -> usize {
        self.0.map_or(0, |r| r.count(topic, choice))
    }

    /// The runs whose final maps were exported as DICOM series.
    fn dicom_exported(&self) -> usize {
        self.0.map_or(0, |r| r.dicom_exported)
    }

    /// The plan of some run had `stage`, and no run did anything in it.
    fn none_ran(&self, stage: Stage) -> bool {
        self.0.is_some_and(|r| r.none_ran(&stage.name()))
    }

    /// The runs that did something in `stage`.
    fn ran(&self, stage: Stage) -> usize {
        self.0.and_then(|r| r.step(&stage.name())).map_or(0, |s| s.ran())
    }
}

/// What the pipeline graph ([`crate::pipeline_graph`]) says about a step's algorithm, in the
/// methods text's words: so a box and the sentence describing the same step cannot disagree.
pub(crate) mod words;

fn join_list(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}, and {}", rest.join(", "), last)
        }
    }
}

/// `text` (a [`Methods::to_markdown`]) without its `## Note` section.
#[cfg(test)]
fn without_note(text: &str) -> String {
    match text.find("## Note\n") {
        Some(start) => {
            let end = text[start..].find("## References").map_or(text.len(), |e| start + e);
            format!("{}{}", &text[..start], &text[end..])
        }
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod masking_tests;

#[cfg(test)]
mod multiorient_methods_tests;

/// Parameter values are stated only when they differ from QSMxT's defaults.
#[cfg(test)]
mod defaults_only_tests;

#[cfg(test)]
mod structured_tests;
