use crate::config::*;
use crate::enums::*;
use crate::masking::*;
use crate::plan::{Pass, Plan, Stage};
use crate::record::{Choice, DatasetRecord, RunRecord, Topic};
use serde::{Deserialize, Serialize};

mod citations;
use citations::*;
mod inversion;
use inversion::*;
pub(crate) use inversion::tiled_phase_network;
mod background;
use background::*;
mod two_pass;
use two_pass::*;
mod masking;
use masking::*;
mod field_map;
use field_map::*;

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

/// QSMART's settings that differ from their defaults, its inner inversion and vessel magnitude
/// aside (said apart). The vessel sizes carry their units.
fn qsmart_changes(config: &PipelineConfig) -> Changed {
    let (q, d) = (&config.inversion.qsmart, QsmartConfig::default());
    let mut c = Changed::default();
    let mut skip = vec!["inversion", "vessel_magnitude", "vessel_size_units", "offset_grid", "frangi_c_mode", "frangi_c_factor"];
    skip.extend(QSMART_VESSEL_SIZE_KEYS);
    // A relative c replaces the absolute one, which then plays no part.
    let relative_c = q.frangi_c_mode == crate::enums::QsmartFrangiCMode::Relative;
    if relative_c {
        skip.push("frangi_c");
    }
    c.add_fields(q, &d, &skip);
    c.add(relative_c, || format!(
        "Frangi's c set to {} × the median of the vessel magnitude inside the brain mask", q.frangi_c_factor));
    c.add(q.offset_grid == crate::enums::QsmartOffsetGrid::QsmartReference,
        || "the stages' offset adjusted on the dipole grid of QSMART's adjust_offset.m".to_string());
    // In other units than the default, the sizes are said whatever they are: the default numbers
    // mean something else.
    let u = q.vessel_size_units;
    let other_units = u != d.vessel_size_units;
    c.add(other_units || q.vasc_sphere_radius != d.vasc_sphere_radius,
        || format!("a bottom-hat sphere radius of {} {u}", q.vasc_sphere_radius));
    c.add(other_units || differs(q.frangi_scale_min, d.frangi_scale_min) || differs(q.frangi_scale_max, d.frangi_scale_max)
            || differs(q.frangi_scale_ratio, d.frangi_scale_ratio),
        || format!("Frangi scales from {} to {} {u} in steps of {} {u}", q.frangi_scale_min, q.frangi_scale_max, q.frangi_scale_ratio));
    c
}

/// The non-default rigid-registration settings, shared by motion correction and
/// multi-orientation co-registration.
fn registration_changes(config: &PipelineConfig) -> Changed {
    let mut c = Changed::default();
    c.add_fields(&config.registration, &crate::motion::RegistrationConfig::default(), &[]);
    c
}

/// The non-default settings of the selected susceptibility-source separation.
fn separation_changes(config: &PipelineConfig) -> Changed {
    let (s, d) = (&config.separation, SeparationConfig::default());
    let mut c = Changed::default();
    match s.algorithm {
        SeparationAlgorithm::R2starQsm => c.add_fields(&s.r2star_qsm, &d.r2star_qsm, &[]),
        SeparationAlgorithm::Decompose => {
            // Unwrapping and a χ bound from the echo spacing have sentences of their own.
            c.add_fields(&s.decompose, &d.decompose, &["lsqcurvefit", "unwrap_phase", "chi_bound", "fit"]);
            c.add(s.decompose.fit == crate::enums::DecomposeFit::Paper, || "the paper's fit (the \
                compartment amplitudes constrained to sum to one and R2*₀ fitted by linear least squares, \
                χ± by trust-region-reflective least squares, each voxel's magnitude scaled to its signal \
                at TE = 0, extrapolated from the first two echoes)".to_string());
            if let ChiBoundChoice::Ppm(x) = s.decompose.chi_bound {
                c.add(s.decompose.chi_bound != d.decompose.chi_bound, || format!("χ bound = {x} ppm"));
            }
            c.add_fields(&s.decompose.lsqcurvefit, &d.decompose.lsqcurvefit, &[]);
        }
        // Its QSM's iLSQR is worded with that QSM.
        SeparationAlgorithm::ChiSepIlsqr => c.add_fields(&s.chi_sep_ilsqr, &d.chi_sep_ilsqr, &["qsm_ilsqr"]),
        SeparationAlgorithm::ChiSepMedi => c.add_fields(&s.chi_sep_medi, &d.chi_sep_medi, &["qsm_ilsqr", "csf_r2star"]),
        SeparationAlgorithm::WaveSep => c.add_fields(&s.wavesep, &d.wavesep, &[]),
        SeparationAlgorithm::HcChisep => c.add_fields(&s.hc_chisep, &d.hc_chisep, &[]),
        SeparationAlgorithm::ChiSepNet => match s.chi_sepnet.route {
            ChiSepNetRoute::Toolbox => {
                c.add(true, || "the inputs resampled to 1 mm with B0 along z and the susceptibility \
                    channel computed by QSMnetME, as in the χ-separation toolbox, and the maps resampled back".to_string());
                if let Some([x, y, z]) = s.chi_sepnet.frame_patch {
                    c.add(true, || format!("the networks applied as a sliding window of {x}×{y}×{z}-voxel patches"));
                }
            }
            ChiSepNetRoute::Native => {
                c.add(true, || "the network applied on the acquired grid to this run's QSM".to_string());
                let [x, y, z] = s.chi_sepnet.patch;
                c.add(s.chi_sepnet.patch != d.chi_sepnet.patch, || format!("{x}×{y}×{z}-voxel sliding-window patches"));
            }
        },
        SeparationAlgorithm::SusepNet => {
            if let Some([x, y, z]) = s.susep_net.patch {
                c.add(true, || format!("the network applied as a sliding window of {x}×{y}×{z}-voxel patches"));
            }
        }
    }
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
    references: Vec<&'static Citation>,
    cited: Vec<&'static str>,
}

impl Draft {
    /// Cite `cite` in the next sentence, and list it if it is not listed yet.
    fn cite(&mut self, cite: &'static Citation) {
        if !self.references.iter().any(|c| c.key == cite.key) {
            self.references.push(cite);
        }
        if !self.cited.contains(&cite.key) {
            self.cited.push(cite.key);
        }
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

/// DECOMPOSE's χ bound from the echo spacing: in ppm where every run that recorded it
/// ([`Topic::ChiBound`]) had the same, and otherwise as where it came from.
fn chi_bound_sentence(ran: Ran) -> String {
    let reasons: std::collections::BTreeSet<&str> = ran.0.and_then(|r| r.step("chi_separation"))
        .map(|s| s.decisions.iter().filter(|d| d.topic == Topic::ChiBound).map(|d| d.reason.as_str()).collect())
        .unwrap_or_default();
    match reasons.iter().next() {
        Some(one) if reasons.len() == 1 => one.to_string(),
        Some(_) => "The χ search was bounded from each scan's field strength and echo spacing.".to_string(),
        None => "The χ search was bounded from the field strength and echo spacing.".to_string(),
    }
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

/// Where χ-sepnet's toolbox route got its R2′: its own estimate unless it is given a measured one.
fn chisepnet_r2prime_sentences(doc: &mut Draft, config: &PipelineConfig, ran: Ran) {
    let (measured, estimated) = (ran.count(Topic::ChiSeparationR2prime, Choice::Measured),
                                 ran.count(Topic::ChiSeparationR2prime, Choice::Predicted));
    if measured > 0 && estimated == 0 {
        doc.say(&[Stage::ChiSeparation], "χ-separation used the measured R2'.".to_string());
        return;
    }
    doc.cite(&CITE_R2PRIMENET);
    let unknown = measured == 0 && estimated == 0;
    let always = config.separation.r2prime_strategy == R2PrimeStrategy::R2primenet;
    let estimate = "estimated from R2* with R2PRIMEnet (Kim et al., 2025) in the network's training frame";
    doc.say(&[Stage::ChiSeparation], if measured > 0 || (unknown && !always) {
        format!("χ-separation used R2' measured as R2* - R2 where a multi-echo spin-echo acquisition \
                 was available, and otherwise its own R2', {estimate}.")
    } else {
        format!("χ-separation used its own R2', {estimate}.")
    });
    // Only a map a run did make, and that was estimated: a measured one is what the route used.
    if config.pipeline.do_r2primemap && (unknown || ran.count(Topic::R2primeSource, Choice::Predicted) > 0) {
        doc.say(&[Stage::ChiSeparation], if measured > 0 || (unknown && !always) {
            "Where R2' was estimated, the R2' map in the outputs was not an input to χ-separation.".to_string()
        } else {
            "The R2' map in the outputs was not an input to χ-separation.".to_string()
        });
    }
    // The R2′ sentences say this where a map was made; the route's own estimate is off 3 T too.
    let off_field = ran.count(Topic::R2primenetCalibration, Choice::Uncalibrated) > 0 || unknown;
    if config.separation.r2primenet.ignore_field_mismatch && off_field && !config.pipeline.do_r2primemap {
        doc.say(&[Stage::ChiSeparation],
            "R2PRIMEnet, whose weights were trained at 3 T, was also applied to data acquired at other \
             field strengths.".to_string());
    }
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

fn draft(config: &PipelineConfig, tool: &str, ran: Ran) -> Draft {
    let mut doc = Draft::default();

    let (cite, author_year) = match tool {
        "QSMbly" | "qsmbly" => (&CITE_QSMBLY, "Stewart, 2026"),
        _ => (&CITE_QSMXT, "Stewart, 2026"),
    };

    doc.cite(cite);
    if config.pipeline.do_qsm {
        doc.say(&[], format!("QSM processing was performed using {} ({}).", tool, author_year));
    } else {
        doc.say(&[], format!("MRI processing was performed using {} ({}).", tool, author_year));
    }
    // The toolbox pipeline the settings were chosen from, while its choices hold; the settings
    // below say where they depart from it.
    if let Some(p) = config.reference_pipeline.as_deref()
        .filter(|id| crate::pipelines::chosen(config, id))
        .and_then(qsm_core::pipeline::reference_pipelines::reference_pipeline) {
        doc.say(&[], format!("Processing followed the {} pipeline.", p.toolbox));
        // UK Biobank's brain mask comes from its T1-weighted images, which QSMxT does not use.
        if p.id == "uk-biobank" {
            doc.say(&[], "The brain mask was made from the gradient-echo images as described below, \
                in place of the UK Biobank pipeline's mask from its T1-weighted images.".to_string());
        }
    }

    // The k-space Tukey window comes before every other step.
    if config.pipeline.tukey_window > 0.0 {
        doc.say(&[Stage::TukeyWindow], format!(
            "Before any other processing, each echo's complex image was multiplied in k-space by a Tukey \
             window with a taper ratio of {}.", config.pipeline.tukey_window));
    }

    // Masking (inhomogeneity correction is described inline)
    describe_masking(config, ran, &mut doc);

    // QSM-specific pipeline
    if config.pipeline.do_qsm {
        describe_reconstruction_grid(config, ran, &mut doc);
        match config.inversion.algorithm {
            QsmAlgorithm::Tgv => {
                // Routed by QSM.rs's `InversionInput`: one echo goes in as wrapped phase (TGV
                // unwraps it itself), several are field-mapped first and go in as the total field.
                // Neither route runs a separate background-removal stage.
                doc.cite(&CITE_TGV);
                doc.say(&both(Stage::Tgv), format!(
                    "QSM was computed using the Total Generalized Variation (TGV) algorithm \
                     (Langkammer et al., 2015), with background field removal and dipole inversion \
                     performed jointly. For single-echo data TGV was applied to the wrapped phase of \
                     the echo and also performed the phase unwrapping; for multi-echo data TGV was \
                     applied to the B0 field map, computed as described below, expressed as phase at \
                     {}. The regularisation weights are in radians at that echo time \
                     and the field strength, and the susceptibility was converted to ppm with them.",
                    echo_time_prose(config.inversion.tgv.te)));
                let changed = inversion_changes(config, QsmAlgorithm::Tgv);
                if !changed.is_empty() {
                    doc.say(&both(Stage::Tgv), format!("TGV was run{}.", changed.with()));
                }
                describe_field_mapping(config, ran, &mut doc);
            }
            QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => {
                // `InversionInput::WrappedPhase`: no field mapping and no background removal ran.
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                doc.cite(cite);
                doc.say(&both(Stage::Inversion), format!(
                    "QSM was reconstructed with {} ({}) from the wrapped phase of each echo, with \
                     phase unwrapping, background field removal and dipole inversion performed by the \
                     network.",
                    name, cite_inline(cite),
                ));
                say_tiling(config, Stage::Inversion, &mut doc);
            }
            QsmAlgorithm::Qsmart => {
                // QSMART consumes a pre-computed total field, so the field-mapping
                // stage (phase offset, unwrapping, B0 estimation) still runs and must
                // be described.
                describe_field_mapping(config, ran, &mut doc);
                let (inv_name, inv_cite) = inversion_name_cite(config.inversion.qsmart.inversion);
                doc.cite(&CITE_QSMART);
                doc.cite(inv_cite);
                doc.say(&both(Stage::Qsmart), format!(
                    "QSM was computed using QSMART (Yaghmaie et al., 2021), with spatially dependent \
                     filtering for background field removal, {} dipole inversion ({}), and Frangi \
                     vesselness-based tissue/vasculature separation{}.",
                    inv_name, cite_inline(inv_cite), qsmart_changes(config).comma_with(),
                ));
                // The magnitude vessels were detected on.
                match config.inversion.qsmart.vessel_magnitude {
                    QsmartVesselMagnitude::MeanN4 => {
                        doc.cite(&CITE_N4);
                        doc.say(&both(Stage::Qsmart), "Vessels were detected on the mean magnitude over echoes, \
                            bias-corrected with N4 (Tustison et al., 2010).".to_string());
                    }
                    QsmartVesselMagnitude::Combined => {
                        doc.say(&both(Stage::Qsmart), format!("Vessels were detected on the {}RSS-combined magnitude.",
                            if config.masking.inhomogeneity_correction { "inhomogeneity-corrected " } else { "" }));
                    }
                }
            }
            _ => {
                describe_field_mapping(config, ran, &mut doc);

                // Background removal — only in front of a local-field inversion (QSM.rs
                // `InversionInput`); the total-field methods remove or model it themselves.
                let total_field = !crate::bridge::inversion_input(config, 2).needs_bg_removal();
                if !total_field {
                    let alg = &config.bg_removal.algorithm;
                    let (name, cite) = bg_removal_name_cite(*alg);
                    doc.cite(cite);
                    let erode = config.bg_removal.erode_voxels;
                    if erode > 0 {
                        doc.say(&both(Stage::BackgroundRemoval), format!(
                            "Before background field removal, the brain mask was eroded by {erode} voxel{}.",
                            if erode == 1 { "" } else { "s" }));
                    }
                    if config.bg_removal.ukb_mask_trim {
                        doc.cite(&CITE_UKB_QSM);
                        doc.say(&both(Stage::BackgroundRemoval), format!(
                            "Before and after background field removal, the mask was trimmed of voxels with \
                             unreliable phase as in the UK Biobank pipeline ({}): phase-reliability maps of the \
                             first and last echoes (the magnitude of the unit phasor averaged over a 2 mm ball) \
                             were thresholded at 0.6 and 0.5 before background removal and at 0.7 and 0.6 after \
                             it, then in each slice components smaller than 300 (after: 200) voxels were removed \
                             and holes smaller than 50 (after: 30) voxels filled, and enclosed holes were filled.",
                            cite_inline(&CITE_UKB_QSM)));
                    }
                    if config.bg_removal.polish_mask {
                        doc.say(&both(Stage::BackgroundRemoval),
                            "After background field removal, the mask was reduced to its largest connected \
                             component with its holes filled, and the local field was set to zero outside it."
                                .to_string());
                    }
                    if *alg == BfAlgorithm::Vsharp {
                        let (v, d) = (&config.bg_removal.vsharp, VsharpConfig::default());
                        let mut changed = Changed::default();
                        changed.add(differs(v.max_radius, d.max_radius) || differs(v.min_radius, d.min_radius)
                            || differs(v.radius_step, d.radius_step),
                            || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
                        changed.add(v.deconvolve != d.deconvolve, || format!(
                            "truncated-SVD deconvolution {}",
                            if v.deconvolve { "enabled" } else { "disabled" }));
                        // The threshold only exists for the deconvolution.
                        changed.add(v.deconvolve && differs(v.threshold, d.threshold),
                            || format!("a deconvolution threshold of {}", v.threshold));
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), changed.with()));
                        let resample = "the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution";
                        match v.grid {
                            VsharpGrid::Finest => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "For anisotropic voxels the V-SHARP background was estimated on an isotropic grid at the smallest acquired voxel dimension: {resample}. Isotropic acquisitions were processed on their acquired grid.")),
                            VsharpGrid::Isotropic if !differs(v.grid_spacing, d.grid_spacing) => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on an isotropic grid: {resample}.")),
                            VsharpGrid::Isotropic => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on a {} mm isotropic grid: {resample}.", v.grid_spacing)),
                            VsharpGrid::Native => {}
                        }
                    } else {
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), bg_removal_changes(config).with()));
                    }

                }

                // Dipole inversion
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                let target = if total_field { " on the total field within the brain mask" } else { "" };
                doc.cite(cite);
                let msmv = medi_msmv(config, &mut doc) + &medi_smv(config);
                let changes = inversion_changes(config, config.inversion.algorithm);
                doc.say(&both(Stage::Inversion), format!("Dipole inversion was performed{} using {} ({}){msmv}{}.",
                    target, name, cite_inline(cite),
                    if msmv.is_empty() { changes.with() } else { changes.comma_with() }));
                describe_medi_csf(config, ran, &mut doc);
                // Deep-learning tiling (memory-bounded, approximate) is noted when enabled — only
                // for the tileable networks (a stray --tile-size on a classical algorithm is ignored).
                if config.inversion.algorithm.tileable() {
                    say_tiling(config, Stage::Inversion, &mut doc);
                } else if config.runs_a_tileable_network() {
                    say_tiling(config, Stage::BackgroundRemoval, &mut doc);
                }
            }
        }

        // Two-pass wraps background removal and inversion, so it is described after both and
        // before referencing, which runs on the combined map.
        describe_two_pass(config, ran, &mut doc);
        // HEIDI after the inversion, and after two-pass on the combined map.
        describe_heidi(config, ran, &mut doc);

        // Referencing
        match config.qsm.reference {
            QsmReference::Mean => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility within \
                     {RECON_MASK}."));
            }
            QsmReference::None => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], "No susceptibility referencing was applied.".to_string());
            }
            QsmReference::Region => {
                let region = config.qsm.reference_region.as_deref().unwrap_or("");
                let pretty = region.replace('-', " ").replace(',', ", ");
                let plural = if region.contains(',') { "s" } else { "" };
                let labels = match config.segmentation.custom_dseg_tool.as_deref() {
                    Some(tool) => format!("'{tool}' segmentation label{plural}"),
                    None => format!("SynthSeg label{plural}"),
                };
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility in the \
                     {pretty} ({labels}), computed within {RECON_MASK}."
                ));
            }
        }
        if says_per_echo(config) {
            doc.say(&[Stage::Reference],
                "For multi-echo data, the susceptibility map reported is the mean across echoes of the \
                 echoes' maps, over the voxels where every echo's map was defined; each echo's map is \
                 reported too.".to_string());
        }
    }

    // A supplied segmentation, every run's or some runs'.
    let dseg_tool = config.segmentation.custom_dseg_tool.as_deref();
    let dseg_supplied = ran.count(Topic::SegmentationSource, Choice::Supplied) > 0;
    let dseg_all_supplied = ran.only(Topic::SegmentationSource, &[Choice::Supplied]);
    if config.pipeline.do_segmentation && dseg_supplied {
        let from = match dseg_tool { Some(t) if t != "*" => format!("'{t}' "), _ => String::new() };
        doc.say(&[Stage::Segmentation], if dseg_all_supplied {
            format!("Segmentations were sourced from the {from}BIDS derivatives.")
        } else {
            format!("Where available, segmentations were sourced from the {from}BIDS derivatives; \
                     otherwise they were generated as follows.")
        });
    }
    if config.pipeline.do_segmentation && !dseg_all_supplied && !ran.none_ran(Stage::Segmentation) {
        doc.cite(&CITE_SYNTHSEG);
        let v2 = config.segmentation.version == SynthSegVersion::V2;
        if v2 {
            doc.cite(&CITE_SYNTHSEG2);
        }
        // The label table counts the background.
        let structures = config.segmentation.version.to_core().n_labels() - 1;
        let (s, d) = (&config.segmentation, SegmentationConfig::default());
        let mut changed = Changed::default();
        changed.add(s.flip_averaging != d.flip_averaging, || format!(
            "left-right flip averaging of the posteriors {}",
            if s.flip_averaging { "enabled" } else { "disabled" }));
        // The version names the weights (and the label count says what they produce); the
        // bring-your-own tool is not a SynthSeg setting.
        if let Some(patch) = s.patch {
            changed.add(true, || format!(
                "the network applied as a sliding window of {p}×{p}×{p}-voxel patches overlapping by at \
                 least half a patch, their posteriors Gaussian-weighted", p = patch.div_ceil(16) * 16));
        }
        changed.add_fields(s, &d, &["version", "flip_averaging", "custom_dseg_tool", "patch"]);
        doc.say(&[Stage::Segmentation], format!(
            "The brain was parcellated from the GRE magnitude using SynthSeg {} ({}), yielding {} \
             FreeSurfer-labelled structures{}.",
            config.segmentation.version,
            if v2 { "Billot, Greve, et al., 2023; Billot, Magdamo, et al., 2023" } else { "Billot et al., 2023" },
            structures, changed.comma_with(),
        ));
    }
    if config.pipeline.do_analysis && !ran.none_ran(Stage::Analysis) {
        doc.say(&[Stage::Analysis], 
            "Each quantitative map was then summarised within every segmented structure (mean, \
             standard deviation, median, minimum, maximum and 5th/95th percentiles over the \
             structure's voxels), excluding voxels the map does not cover.".to_string());
    }

    if config.pipeline.do_smwi && !ran.none_ran(Stage::Smwi) {
        doc.cite(&CITE_SMWI);
        let (s, d) = (&config.smwi, SmwiConfig::default());
        let mut mask = Changed::default();
        mask.add(differs(s.threshold_ppm, d.threshold_ppm), || format!("|χ| threshold {:.2} ppm", s.threshold_ppm));
        mask.add(differs(s.power, d.power), || format!("mask raised to the power {}", s.power));
        let slab = if s.mip_window != d.mip_window { format!(" over {} slices", s.mip_window) } else { String::new() };
        doc.say(&[Stage::Smwi], format!(
            "Susceptibility map-weighted images were computed (Gho et al., 2014) by weighting the \
             magnitude with a mask derived from the susceptibility map{}, separately for \
             paramagnetic and diamagnetic sources, each followed by a minimum-intensity \
             projection{slab}.",
            mask.paren(),
        ));
    }

    // SWI
    if config.pipeline.do_swi {
        let (swi, d) = (&config.swi, SwiConfig::default());
        let mut changed: Vec<String> = Vec::new();
        match swi.unwrap {
            _ if swi.unwrap == d.unwrap => {}
            SwiUnwrap::Romeo => {
                changed.push("ROMEO phase unwrapping (Dymerska et al., 2021)".into());
                doc.cite(&CITE_ROMEO);
            }
            SwiUnwrap::LaplacianSlice => changed.push("slice-wise Laplacian phase unwrapping".into()),
            SwiUnwrap::Laplacian => changed.push("Laplacian phase unwrapping".into()),
        }
        if swi.hp_sigma.iter().zip(d.hp_sigma).any(|(a, b)| differs(*a, b)) {
            changed.push(format!(
                "a high-pass filter σ of {} voxels",
                swi.hp_sigma.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" × "),
            ));
        }
        if swi.scaling != d.scaling {
            changed.push(format!("{} phase-mask scaling", swi.scaling));
        }
        if differs(swi.strength, d.strength) {
            changed.push(format!("phase-mask strength {}", swi.strength));
        }
        let mip = if swi.mip_window != d.mip_window {
            format!("Minimum-intensity projections were taken over {}-slice slabs.", swi.mip_window)
        } else {
            "Minimum-intensity projections of these images were also computed.".to_string()
        };
        let using = if changed.is_empty() { String::new() } else { format!(" using {}", join_list(&changed)) };
        // ROMEO and slice-wise unwrapping filter in-plane whatever the σ; Laplacian does so when
        // σ has no slice component (the default).
        let phase = if swi.filtered_phase {
            let in_plane = swi.unwrap != SwiUnwrap::Laplacian || swi.hp_sigma[2] == 0.0;
            format!(" The filtered phase that weights these images, CLEAR-SWI's phase after {} Gaussian \
                     high-pass filter within a head mask from the first echo's magnitude, was also saved, in Hz \
                     for multi-echo data and in radians for single-echo data.",
                    if in_plane { "an in-plane" } else { "a" })
        } else {
            String::new()
        };
        doc.cite(&CITE_CLEARSWI);
        doc.say(&[Stage::Swi], format!(
            "Susceptibility-weighted images were computed with CLEAR-SWI (Eckstein et al., 2021){using}. {mip}{phase}"
        ));
    }

    // T2*/R2*
    if config.pipeline.do_t2starmap && config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* and R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_t2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    }

    // R2 / R2'. Worth stating how R2' was obtained, not just that it was: a measured R2' and an
    // estimated one support different claims, and only one of them is a measurement.
    if config.pipeline.do_r2map || config.pipeline.do_r2primemap {
        if config.pipeline.do_r2map && ran.only(Topic::R2Source, &[Choice::Supplied]) {
            doc.say(&[Stage::R2], "R2 maps were sourced from the supplied BIDS derivatives.".to_string());
        } else if config.pipeline.do_r2map && !ran.only(Topic::R2Source, &[Choice::Unavailable]) {
            doc.cite(&CITE_EPG);
            doc.say(&[Stage::R2], 
                "R2 maps were fitted from multi-echo spin-echo magnitude data using an Extended \
                 Phase Graph dictionary (Weigel, 2015).".to_string());
        }
        if config.pipeline.do_r2primemap {
            // R2PRIMEnet's own settings; the field-strength override gets a sentence below.
            let mut r2pn = Changed::default();
            r2pn.add_fields(&config.separation.r2primenet, &R2primenetConfig::default(),
                &["ignore_field_mismatch"]);
            let r2pn = r2pn.semi();
            // What the runs that made an R2′ did, where they all did the same.
            let measured = ran.count(Topic::R2primeSource, Choice::Measured) > 0;
            let predicted = ran.count(Topic::R2primeSource, Choice::Predicted) > 0;
            let none_made = ran.only(Topic::R2primeSource, &[Choice::Unavailable]);
            match config.separation.custom_r2prime_tool.as_deref() {
                _ if none_made => {}
                // `auto` that only measured, or only predicted.
                None if config.separation.r2prime_strategy == R2PrimeStrategy::Auto && measured != predicted => {
                    if measured {
                        doc.say(&[Stage::R2], "R2' was computed as R2* - R2.".to_string());
                    } else {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., 2025{r2pn}).",
                        ));
                    }
                }
                Some(_) => doc.say(&[Stage::R2], 
                    "R2' maps were taken from the supplied BIDS derivatives.".to_string()),
                None => match config.separation.r2prime_strategy {
                    R2PrimeStrategy::Mese => doc.say(&[Stage::R2], 
                        "R2' was computed as R2* - R2.".to_string()),
                    R2PrimeStrategy::R2primenet => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., \
                             2025{r2pn}).",
                        ));
                    }
                    R2PrimeStrategy::Auto => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], if config.separation.r2primenet.ignore_field_mismatch {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and estimated from R2* with R2PRIMEnet \
                             (Kim et al., 2025{r2pn}) otherwise.")
                        } else {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and otherwise, at 3 T only, estimated \
                             from R2* with R2PRIMEnet (Kim et al., 2025{r2pn}).")
                        });
                    }
                },
            }
            // Both ways QSMxT computes an R2′ (R2* − R2 and R2PRIMEnet) set negative values to zero.
            if config.separation.custom_r2prime_tool.is_none() && !none_made {
                doc.say(&[Stage::R2], "Negative R2' values were set to zero.".to_string());
            }
            // Said where a run applied it off 3 T, or nothing is known of the runs.
            let off_field = ran.count(Topic::R2primenetCalibration, Choice::Uncalibrated) > 0
                || ran.0.is_none_or(|r| r.split(Topic::R2primeSource).is_empty());
            if config.separation.custom_r2prime_tool.is_none()
                && config.separation.r2prime_strategy != R2PrimeStrategy::Mese
                && config.separation.r2primenet.ignore_field_mismatch
                && off_field && !none_made
            {
                doc.say(&[Stage::R2], 
                    "R2PRIMEnet, whose weights were trained at 3 T, was also applied to data \
                     acquired at other field strengths."
                        .to_string());
            }
        }
    }

    // Chi-separation (susceptibility source separation)
    if config.pipeline.do_chi_separation {
        let (sep_name, sep_cite) = separation_name_cite(config.separation.algorithm);
        doc.cite(sep_cite);
        doc.say(&[Stage::ChiSeparation], format!(
            "Paramagnetic and diamagnetic susceptibility maps were computed using {} ({}) within {}{}.",
            sep_name, cite_inline(sep_cite),
            // Named in full by the referencing sentence, unless the map was left unreferenced.
            if config.pipeline.do_qsm && config.qsm.reference != QsmReference::None { RECON_MASK_AGAIN } else { RECON_MASK },
            separation_changes(config).comma_with(),
        ));
        // DECOMPOSE's signal phase comes from a QSM per echo: a per-echo QSM's, or the one QSM.
        if config.separation.algorithm == SeparationAlgorithm::Decompose {
            doc.say(&[Stage::ChiSeparation], if says_per_echo(config) && config.separation.custom_qsm_tool.is_none() {
                "Each echo's susceptibility map was used for the signal phase at that echo.".to_string()
            } else {
                "The same susceptibility map was used for the signal phase at every echo.".to_string()
            });
            if config.separation.decompose.unwrap_phase {
                doc.say(&[Stage::ChiSeparation], "The phase was unwrapped along the echoes.".to_string());
            }
            if config.separation.decompose.chi_bound == ChiBoundChoice::EchoSpacing {
                doc.say(&[Stage::ChiSeparation], chi_bound_sentence(ran));
            }
        }
        match config.separation.algorithm {
            SeparationAlgorithm::ChiSepIlsqr => doc.say(&[Stage::ChiSeparation],
                "The two sources were initialised from the susceptibility map and solved by LSQR, \
                 streaking removal and a non-negative refinement.".to_string()),
            SeparationAlgorithm::ChiSepMedi => {
                let r2star = match config.separation.chi_sep_medi.csf_r2star {
                    ChiSepCsfR2star::Arlo => {
                        doc.cite(&CITE_ARLO);
                        "ARLO R2* (Pei et al., 2015)"
                    }
                    ChiSepCsfR2star::Nnls => "R2* from a non-negative least-squares mono-exponential fit",
                };
                doc.say(&[Stage::ChiSeparation], format!(
                    "The two sources were initialised from the susceptibility map, the data terms were \
                     weighted by a noise map from the echo magnitudes, and the susceptibility within a \
                     CSF mask from {r2star} was regularised towards uniformity."));
            }
            _ => {}
        }
        // The networks' outputs as QSM.rs returns them.
        if matches!(config.separation.algorithm, SeparationAlgorithm::ChiSepNet | SeparationAlgorithm::SusepNet) {
            doc.say(&[Stage::ChiSeparation], "Negative χ+ and |χ−| values were set to zero.".to_string());
        }
        if crate::config::chisepnet_route_can_estimate_r2prime(config) {
            chisepnet_r2prime_sentences(&mut doc, config, ran);
        }
        if crate::stages::separation::reconstructs_qsm(config) {
            let mut q = Changed::default();
            let defaults = SeparationConfig::default();
            let d = match config.separation.algorithm {
                SeparationAlgorithm::ChiSepMedi => &defaults.chi_sep_medi.qsm_ilsqr,
                _ => &defaults.chi_sep_ilsqr.qsm_ilsqr,
            };
            q.add_fields(crate::stages::separation::initial_qsm_ilsqr(config), d, &[]);
            doc.cite(&CITE_ILSQR);
            doc.say(&[Stage::ChiSeparation], format!(
                "The susceptibility map separated was reconstructed from the local field by iLSQR ({}){}.",
                cite_inline(&CITE_ILSQR), q.with()));
        }
    }

    // Multi-orientation. Described after the per-orientation pipeline because that is the
    // order it runs in: each orientation is unwrapped and background-corrected on its own,
    // and only the local fields are combined. Whether a group forms depends on the runs, so the
    // setting alone says nothing happened: the record does ([`Topic::OrientationGroup`]). Before
    // any run, or where only some runs were combined, the wording is conditional (#496).
    let combined = ran.count(Topic::OrientationGroup, Choice::Applied);
    let every_run = combined > 0 && combined == ran.runs();
    if config.multi_orientation.enabled() && (ran.0.is_none() || combined > 0) {
        let fields = if every_run {
            "Local field maps from multiple head orientations"
        } else {
            "Where a session held several head orientations of the same object, local field maps \
             from those orientations"
        };
        match config.multi_orientation.algorithm {
            MultiOrientAlgorithm::Cosmos => {
                doc.cite(&CITE_COSMOS);
                doc.say(&[], format!(
                    "{fields}, co-registered onto a common \
                     grid, were combined by Calculation Of Susceptibility through Multiple \
                     Orientation Sampling (COSMOS; Liu et al., 2009), using each orientation's B0 \
                     direction expressed in that common frame."),
                );
            }
            MultiOrientAlgorithm::Sti => {
                doc.cite(&CITE_STI);
                doc.say(&[], format!(
                    "{fields}, co-registered onto a common \
                     grid, were combined by susceptibility tensor imaging (STI; Liu, 2010), \
                     yielding the six independent components of the symmetric susceptibility \
                     tensor along with mean magnetic susceptibility (MMS), magnetic susceptibility \
                     anisotropy (MSA) and the principal eigenvector."),
                );
            }
        }
        // How the orientations got onto one grid is part of the method, not an implementation
        // detail: it determines where the B0 directions came from, which is half the input.
        if config.multi_orientation.register {
            let reference = config.multi_orientation.reference.trim();
            let target = if reference.is_empty() {
                "the first orientation's grid".to_string()
            } else {
                format!("the grid of the reference orientation ({reference})")
            };
            doc.say(&[], format!(
                "Orientations that were not already on a common grid were rigidly co-registered \
                 (6 degrees of freedom, normalized cross-correlation over a multi-resolution \
                 pyramid{}) onto {target} using the magnitude images, and each \
                 orientation's B0 direction was derived from the recovered rotation.",
                registration_changes(config).semi(),
            ));
        } else {
            doc.say(&[], 
                "The orientations were taken to be already co-registered onto a common grid."
                    .to_string(),
            );
        }
        if differs(config.multi_orientation.lambda, MultiOrientationConfig::default().lambda) {
            doc.say(&[], format!(
                "The multi-orientation inversion was regularized with lambda = {}.",
                config.multi_orientation.lambda
            ));
        }
        let m = &config.multi_orientation;
        if m.algorithm == MultiOrientAlgorithm::Cosmos {
            if m.magnitude_weighting {
                let d = MultiOrientationConfig::default();
                let mut cg = Changed::default();
                cg.add(differs(m.cosmos_tol, d.cosmos_tol), || format!("tolerance {}", m.cosmos_tol));
                cg.add(m.cosmos_max_iter != d.cosmos_max_iter, || format!("at most {} iterations", m.cosmos_max_iter));
                doc.say(&[], format!(
                    "Each orientation's field was weighted by its magnitude, and the weighted \
                     least-squares problem solved by conjugate gradient{}.", cg.paren()));
            }
            doc.say(&[], 
                "The COSMOS map was referenced in the same way as the single-orientation maps, \
                 over the voxels covered by every orientation's background-field-removal mask."
                    .to_string(),
            );
        }
    }

    // DICOM export. It is the front end's, after the run, and can fail without failing the run, so
    // the setting alone says nothing happened: the record does ([`RunRecord::dicom_exported`]).
    // Before any run the setting is described, as a native run exports; a browser run never does
    // (the browser profile clears the setting).
    let exported = ran.dicom_exported();
    if config.pipeline.export_dicom && (ran.0.is_none() || exported > 0) {
        doc.say(&[], if ran.0.is_none() || exported == ran.runs() {
            "Final maps were additionally exported as DICOM series.".to_string()
        } else {
            "For some of the runs, final maps were additionally exported as DICOM series.".to_string()
        });
    }

    doc
}

/// The reconstruction-grid switches, all off by default, so each is said only when on.
fn describe_reconstruction_grid(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let p = &config.pipeline;
    if p.ignore_slice_gap {
        doc.say(&[Stage::Load], "Acquisitions whose slices were separated by a gap were reconstructed as if the slices were contiguous.".to_string());
    }
    let space = match p.output_space {
        OutputSpace::Acquired => "",
        OutputSpace::Working => ", and the derivatives were written on the resampled grid",
    };
    // Resampled only because an algorithm needs axial data: the switches do not say so.
    if !p.axial_resampling && ran.count(Topic::Grid, Choice::ResampledForAlgorithm) > 0 {
        doc.say(&[Stage::Load, Stage::ResampleAxial, Stage::OutputSpace], format!(
            "Acquisitions whose B0 direction was more than {}° from the slice normal were resampled onto an \
             axial grid, with B0 along the slice normal, before reconstruction, as the algorithms used assume \
             it{space}.", crate::scan::AXIAL_ONLY_TOLERANCE_DEG));
    }
    // None of the runs was resampled: there is nothing to say.
    if p.axial_resampling && !ran.only(Topic::Grid, &[Choice::Acquired]) {
        let which = if differs(p.obliquity_threshold, DEFAULT_OBLIQUITY_THRESHOLD) {
            format!("Acquisitions more than {}° oblique", p.obliquity_threshold)
        } else {
            "Oblique acquisitions".to_string()
        };
        doc.say(&[Stage::Load, Stage::ResampleAxial, Stage::OutputSpace], format!(
            "{which} were resampled onto an axial grid, with B0 along the slice normal, before reconstruction{space}."));
    }
    if p.crop_to_mask {
        let margin = if differs(p.crop_margin_mm, PipelineToggles::default().crop_margin_mm) {
            format!(" with a {} mm margin", p.crop_margin_mm)
        } else {
            String::new()
        };
        doc.say(&reconstruction(), format!("Reconstruction was performed within a box around the brain mask{margin}."));
    }
    if p.fft_padding {
        doc.say(&reconstruction(), "Before the FFT-based stages, the grid was zero-padded to an FFT-efficient size.".to_string());
    }
}

/// "64" for a uniform pad, "64×64×32" otherwise.
fn describe_pad(p: [usize; 3]) -> String {
    if p[0] == p[1] && p[1] == p[2] { p[0].to_string() } else { format!("{}×{}×{}", p[0], p[1], p[2]) }
}

/// What the pipeline graph ([`crate::pipeline_graph`]) says about a step's algorithm, in the
/// methods text's words: so a box and the sentence describing the same step cannot disagree.
pub(crate) mod words {
    use super::*;

    /// Which of `n` echoes `echoes` (1-based, ascending) are: "each echo", "the first echo", "the
    /// last echo", or "echoes 1 and 5".
    pub(crate) fn echoes(echoes: &[usize], n: usize) -> String {
        match echoes {
            e if e.len() == n && n > 1 => "each echo".to_string(),
            [1] => "the first echo".to_string(),
            [e] if *e == n => "the last echo".to_string(),
            [e] => format!("echo {e}"),
            [rest @ .., last] => format!("echoes {} and {last}",
                rest.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")),
            [] => String::new(),
        }
    }

    /// The name the methods text gives an algorithm, without what follows it in parentheses:
    /// "RTS (Rapid Two-Step)" is "RTS" in a box.
    fn short(name: &'static str) -> String {
        name.split(" (").next().unwrap_or(name).to_string()
    }

    pub(crate) fn inversion(alg: QsmAlgorithm) -> String {
        short(inversion_name_cite(alg).0)
    }

    pub(crate) fn bg_removal(alg: BfAlgorithm) -> String {
        short(bg_removal_name_cite(alg).0)
    }

    pub(crate) fn separation(alg: SeparationAlgorithm) -> String {
        short(separation_name_cite(alg).0)
    }

    /// The selected inversion's settings that differ from their defaults (`"λ = 0.002"`).
    pub(crate) fn inversion_settings(config: &PipelineConfig, alg: QsmAlgorithm) -> Vec<String> {
        inversion_changes(config, alg).0
    }

    /// The selected background removal's settings: V-SHARP's kernel radii always (they are what
    /// says which V-SHARP ran), and otherwise what differs from the defaults.
    pub(crate) fn bg_removal_settings(config: &PipelineConfig) -> Vec<String> {
        let b = &config.bg_removal;
        if b.algorithm != BfAlgorithm::Vsharp {
            return bg_removal_changes(config).0;
        }
        let (v, d) = (&b.vsharp, VsharpConfig::default());
        let mut c = Changed::default();
        c.add(true, || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
        c.add(v.deconvolve != d.deconvolve, || format!(
            "truncated-SVD deconvolution {}", if v.deconvolve { "enabled" } else { "disabled" }));
        c.add(v.deconvolve && differs(v.threshold, d.threshold), || format!("deconvolution threshold = {}", v.threshold));
        c.0
    }

    pub(crate) fn separation_settings(config: &PipelineConfig) -> Vec<String> {
        separation_changes(config).0
    }

    pub(crate) fn registration_settings(config: &PipelineConfig) -> Vec<String> {
        registration_changes(config).0
    }

    /// ROMEO's settings that differ from their defaults; Laplacian unwrapping's solver, when it
    /// is not the default one, and the raw phase, when it is unwrapped unmasked; PRELUDE's
    /// partitions and labelling, when they are not the defaults.
    pub(crate) fn unwrapping_settings(config: &PipelineConfig) -> Vec<String> {
        let (fm, d) = (&config.field_mapping, FieldMappingConfig::default());
        let mut c = Changed::default();
        match fm.unwrapping_algorithm {
            UnwrappingAlgorithm::Romeo => c.add_fields(&fm.romeo, &d.romeo, &[]),
            UnwrappingAlgorithm::Laplacian => {
                c.add(fm.laplacian_solver != d.laplacian_solver, || format!("{} solver", fm.laplacian_solver));
                c.add(fm.laplacian_unmasked != d.laplacian_unmasked, || "raw phase over the whole volume".to_string());
                c.add(fm.laplacian_keep_outside != d.laplacian_keep_outside, || "field kept over the whole volume".to_string());
            }
            UnwrappingAlgorithm::Prelude => c = prelude_changes(fm, &d),
            UnwrappingAlgorithm::MediRegionGrowing => {}
        }
        c.0
    }

    /// Each mask section as one line: its input, how the mask is made from it and how it is
    /// refined ("phase quality: Otsu threshold, dilation, hole-filling, erosion").
    pub(crate) fn mask_sections(sections: &[MaskSection]) -> Vec<String> {
        sections.iter().map(|section| {
            let input = match section.input {
                MaskingInput::PhaseQuality => "phase quality",
                MaskingInput::Magnitude => "magnitude",
                MaskingInput::MagnitudeFirst => "first-echo magnitude",
                MaskingInput::MagnitudeLast => "last-echo magnitude",
            };
            let generator = match &section.generator {
                MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => "Otsu threshold".to_string(),
                MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => format!("threshold {}", value.unwrap_or(0.5)),
                MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } =>
                    format!("{}th-percentile threshold", value.unwrap_or(75.0)),
                MaskOp::Threshold { method: MaskThresholdMethod::StiGui, .. } => "STI Suite threshold".to_string(),
                MaskOp::Bet { .. } => "BET".to_string(),
                MaskOp::HdBet { .. } => "HD-BET".to_string(),
                MaskOp::Rs2Net { .. } => "RS2-Net".to_string(),
                MaskOp::MindGrab { .. } => "MindGrab".to_string(),
                other => describe_refinements(std::slice::from_ref(other), &mut Draft::default()).join(", "),
            };
            let steps: Vec<String> = std::iter::once(generator)
                .chain(describe_refinements(&section.refinements, &mut Draft::default()))
                .collect();
            format!("{input}: {}", steps.join(", "))
        }).collect()
    }

    /// QSMART's settings that differ from their defaults (its inner inversion is said apart).
    pub(crate) fn qsmart_settings(config: &PipelineConfig) -> Vec<String> {
        let mut c = qsmart_changes(config);
        c.add(config.inversion.qsmart.vessel_magnitude == QsmartVesselMagnitude::Combined,
            || "vessels on the combined magnitude".to_string());
        c.0
    }
}

/// The χ-separation method's name as the methods text gives it.
pub(crate) fn separation_name(alg: SeparationAlgorithm) -> &'static str {
    separation_name_cite(alg).0
}

fn separation_name_cite(alg: SeparationAlgorithm) -> (&'static str, &'static Citation) {
    match alg {
        SeparationAlgorithm::R2starQsm => ("R2*-QSM susceptibility source separation", &CITE_R2STARQSM),
        SeparationAlgorithm::Decompose => ("DECOMPOSE-QSM", &CITE_DECOMPOSE),
        SeparationAlgorithm::ChiSepIlsqr => ("χ-separation (iLSQR)", &CITE_CHISEP),
        SeparationAlgorithm::ChiSepMedi => ("χ-separation (MEDI)", &CITE_CHISEP),
        SeparationAlgorithm::WaveSep => ("WaveSep", &CITE_WAVESEP),
        SeparationAlgorithm::HcChisep => ("hollow-cylinder χ-separation (HC-ChiSep)", &CITE_HCCHISEP),
        SeparationAlgorithm::SusepNet => ("SUSEP-Net (deep-learning source separation)", &CITE_SUSEPNET),
        SeparationAlgorithm::ChiSepNet => ("χ-sepnet (deep-learning source separation)", &CITE_CHISEPNET),
    }
}

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
