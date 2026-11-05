//! What a config departs from in each chosen method's reference pipeline, from QSM.rs's pipeline
//! rules (`qsm_core::pipeline::rules`), in QSMxT's settings.
//!
//! QSMxT has no rules of its own: it converts its config to QSM.rs's
//! ([`crate::bridge::to_qsm_pipeline_config`]), evaluates QSM.rs's rules on it, and says what they
//! find in its own settings' terms ([`Recommendation`]). Every front end gets the same findings
//! from here: the TUI and qsmbly through [`crate::schema::settings_view`], `qsmxt run` at plan
//! time, the run record and the methods text's Note section from the run.
//!
//! - **Nothing is changed silently.** A finding is shown; [`apply`] (the TUI's or qsmbly's
//!   Apply, `qsmxt run --accept-recommendations` or `--accept-reference-settings`) changes a
//!   setting only when asked.
//! - **Two kinds** ([`Kind`], QSM.rs's): a rule is either **the original authors' setting**
//!   (`reference`: their code, paper or README does it, with no reason given why it is better)
//!   or **a reasoned recommendation** (`reasoned`: required, recommended or discouraged, for the
//!   reason it gives). Front ends label the two differently ([`Kind::label`]); a reasoned
//!   finding is preselected and applied by `--accept-recommendations`; an authors' setting is
//!   shown but never preselected, and applied only when picked or with
//!   `--accept-reference-settings` ([`accept`]).
//! - **Strength** ([`Strength`]) is QSM.rs's. A `required` finding stops the run as configured
//!   (a plan-time error, [`crate::requirements::scan_check`]); `recommended` and `discouraged`
//!   findings are offered (a modal when a change creates one, a marker afterwards); a
//!   `reference` finding is the authors' setting: its Note line is the softer "differs from the
//!   reference pipeline".
//! - **By method** ([`by_method`]): a method's `reference` rules are its reference pipeline. A
//!   front end offers it when the method is chosen ([`chosen_methods`] before and after a change)
//!   and the config differs from it ([`reference_differences`]): Apply all ([`apply_all`]), per
//!   setting ([`apply`]), or Keep mine ([`keep`], dismissals, so it is not offered again for the
//!   same values). A setting's [`marker`] names whose rule it departs from and what that uses.
//! - **Findings chosen together** ([`settle`]) get one answer on every host: on each setting the
//!   strongest takes it, so a reasoned finding outranks an authors' setting, and two as strong
//!   that disagree leave the setting for the user (#493).
//! - **Every rule that applies is shown**, departed from or not ([`reference_values`]), so a
//!   setting covered by a rule always shows its reference value and source next to its own.
//! - **Dismissals** ([`crate::config::Dismissal`]) are kept in the config, with the value they
//!   were dismissed for: changing the setting brings the finding back.
//! - **Scan facts** (echo count, field strength, voxel size, matrix size) may be unknown; a rule
//!   that needs an unknown one stays silent until the scan is known ([`Recommendation::needs_scan`]).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use qsm_core::pipeline::rules::{self as core, Condition, Current, Rule, RuleInput, Test};

use crate::config::{Dismissal, PipelineConfig};
use crate::enums::*;
use crate::masking::MaskOp;

/// How strongly a rule applies (QSM.rs's `Strength`). Ordered `Reference < Discouraged <
/// Recommended < Required`: on one setting, the strongest finding takes precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    /// What the method's reference pipeline used, where its authors never said it is needed or
    /// better. Information only.
    Reference,
    /// A combination known to fail.
    Discouraged,
    /// What the authors recommend, or what QSM.rs measured to be better.
    Recommended,
    /// The method cannot run correctly otherwise.
    Required,
}

impl Strength {
    pub fn label(self) -> &'static str {
        match self {
            Strength::Reference => "reference",
            Strength::Discouraged => "discouraged",
            Strength::Recommended => "recommended",
            Strength::Required => "required",
        }
    }

    /// Whether a finding of this strength is offered as a change with the recommended value
    /// preselected (a modal, `--accept-recommendations`): the reasoned ones, every strength but
    /// `reference`.
    pub fn actionable(self) -> bool {
        self != Strength::Reference
    }

    /// Whether a finding of this strength can be dismissed ("Keep mine", `--dismiss-recommendation`):
    /// `recommended` and `discouraged`. A `required` one must be followed, and a `reference` one is
    /// information only: never applied, and in the methods Note either way.
    pub fn dismissable(self) -> bool {
        matches!(self, Strength::Recommended | Strength::Discouraged)
    }
}

impl From<core::Strength> for Strength {
    fn from(s: core::Strength) -> Self {
        match s {
            core::Strength::Reference => Strength::Reference,
            core::Strength::Discouraged => Strength::Discouraged,
            core::Strength::Recommended => Strength::Recommended,
            core::Strength::Required => Strength::Required,
        }
    }
}

/// Whether a rule is the original authors' setting or a reasoned recommendation (QSM.rs's
/// `Kind`; the `reference` strength is the authors' setting, every other strength is reasoned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// What the method's authors did, with no reason given why it is better: shown, never
    /// preselected, applied only when picked or with `--accept-reference-settings`.
    Reference,
    /// Needed or better for the reason the rule gives: preselected, and applied by
    /// `--accept-recommendations`.
    #[default]
    Reasoned,
}

impl Kind {
    /// How front ends label it: "Original authors' setting" or "Recommended".
    pub fn label(self) -> &'static str {
        match self {
            Kind::Reference => "Original authors' setting",
            Kind::Reasoned => "Recommended",
        }
    }
}

impl From<core::Kind> for Kind {
    fn from(k: core::Kind) -> Self {
        match k {
            core::Kind::Reference => Kind::Reference,
            core::Kind::Reasoned => Kind::Reasoned,
        }
    }
}

impl Strength {
    /// The kind of a finding of this strength.
    pub fn kind(self) -> Kind {
        if self == Strength::Reference { Kind::Reference } else { Kind::Reasoned }
    }
}

/// What a rule rests on (QSM.rs's `Basis`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// The authors say so: their paper, README or a comment in their code (for an authors'
    /// setting, without saying why).
    Stated,
    /// QSM.rs measured it, and recorded the measurement in the method's docs.
    Measured,
    /// The authors' code or paper does it, without saying it is needed or better.
    ReferenceChoice,
    /// What QSM.rs's implementation does (e.g. a setting it ignores in some combination).
    Implementation,
}

impl Basis {
    pub fn label(self) -> &'static str {
        match self {
            Basis::Stated => "stated by the authors",
            Basis::Measured => "measured in QSM.rs",
            Basis::ReferenceChoice => "the reference pipeline's choice",
            Basis::Implementation => "QSM.rs's implementation",
        }
    }
}

impl From<core::Basis> for Basis {
    fn from(b: core::Basis) -> Self {
        match b {
            core::Basis::Stated => Basis::Stated,
            core::Basis::Measured => Basis::Measured,
            core::Basis::ReferenceChoice => Basis::ReferenceChoice,
            core::Basis::Implementation => Basis::Implementation,
        }
    }
}

/// A value of a setting, with its display name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named {
    /// The value: as the config writes it, or, for a setting QSM.rs describes by kind rather than
    /// by value (the mask's generator, HD-BET's and SynthSeg's inference, the Laplacian solver,
    /// V-SHARP's grid), QSM.rs's id for it.
    pub value: Value,
    /// Its display name ("V-SHARP", "BET", "12 mm").
    pub name: String,
}

/// What a rule expects of its setting, in the setting's terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "test", rename_all = "snake_case")]
pub enum Allowed {
    /// One of these (the first is the recommended one).
    OneOf { values: Vec<Named> },
    /// None of these.
    NoneOf { values: Vec<Named> },
    /// A number within the inclusive range (either bound may be open).
    Range {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<Value>,
    },
}

/// The choice a rule is scoped to: the setting and value that trigger it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trigger {
    /// QSMxT config path.
    pub setting: String,
    pub value: Value,
    /// The value's display name ("MEDI").
    pub name: String,
}

/// One rule of QSM.rs's pipeline rules that applies to a config, in QSMxT's settings: its
/// trigger is chosen and its setting is in play. [`Recommendation::departs`] says whether the
/// setting departs from what the rule expects (a finding).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    /// QSM.rs's rule id (`medi.bg-pdf`).
    pub rule: String,
    /// The original authors' setting, or a reasoned recommendation. Follows `strength`; a run
    /// record written before findings carried it reads as reasoned until its reader sets it
    /// from the strength.
    #[serde(default)]
    pub kind: Kind,
    pub strength: Strength,
    pub basis: Basis,
    /// The QSMxT config path of the setting the rule is about (`bg_removal.algorithm`; the mask
    /// recipe's rules are about `masking.sections`).
    pub setting: String,
    /// The setting's display name ("Background removal").
    pub label: String,
    pub trigger: Trigger,
    /// The setting's value now (one per mask section for the mask recipe's rules).
    pub current: Vec<Named>,
    /// What the rule applies to resolve the finding.
    pub recommended: Named,
    /// What the rule expects of the setting.
    pub allowed: Allowed,
    /// One line from QSM.rs: why.
    pub reason: String,
    /// The authors' code (file, line) or paper (section), or the QSM.rs docs recording a
    /// measurement.
    pub source: String,
    /// The rule can only fire once the scan is known (it needs the echo count, field strength,
    /// voxel or matrix size).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_scan: bool,
    /// The setting departs from what the rule expects: a finding.
    pub departs: bool,
    /// The finding was dismissed for this value of the setting ([`Dismissal`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dismissed: bool,
    /// Another finding on the same setting, as strong as this one, recommends something else:
    /// show them together, with neither preselected, and leave the choice to the user.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub conflict: bool,
}

impl Recommendation {
    /// The current value(s), named: "V-SHARP", or "Threshold, BET" for a recipe of two sections.
    pub fn current_text(&self) -> String {
        let mut names: Vec<&str> = Vec::new();
        for n in &self.current {
            if !names.contains(&n.name.as_str()) {
                names.push(&n.name);
            }
        }
        names.join(", ")
    }

    /// Offered as a change, preselected: a departure from a reasoned rule.
    pub fn actionable(&self) -> bool {
        self.departs && self.strength.actionable()
    }

    /// A departure from the original authors' setting: shown, applied only when picked or with
    /// `--accept-reference-settings`.
    pub fn authors_setting(&self) -> bool {
        self.departs && self.kind == Kind::Reference
    }

    /// The other values the rule allows besides the recommended one (for a choice).
    pub fn alternatives(&self) -> Vec<&Named> {
        match &self.allowed {
            Allowed::OneOf { values } => values.iter().filter(|v| v.value != self.recommended.value).collect(),
            _ => vec![],
        }
    }

    /// The method the rule is scoped to, when it is one ("MEDI"); `None` when the trigger is
    /// another setting's value, or the setting itself.
    fn method(&self) -> Option<&str> {
        (METHOD_SETTINGS.contains(&self.trigger.setting.as_str()) && self.trigger.setting != self.setting).then_some(&self.trigger.name)
    }

    /// The setting is a flag.
    fn flag(&self) -> bool {
        self.recommended.value.is_boolean()
    }

    /// What the departure is from, after the current value: "; MEDI recommends PDF: why",
    /// ", which is discouraged with NDI: why", "; this differs from the MEDI reference pipeline,
    /// which uses BET". With `why` off, the rule's reason is left out: what the methods text says.
    fn departure(&self, why: bool) -> String {
        let rec = if self.flag() { format!("turning it {}", self.recommended.name) } else { self.recommended.name.clone() };
        let uses = if self.flag() { format!("has it {}", self.recommended.name) } else { format!("uses {}", self.recommended.name) };
        let verb = if self.strength == Strength::Required { "requires" } else { "recommends" };
        let reason = if why { format!(": {}", self.reason) } else { String::new() };
        match (self.strength, self.method()) {
            (Strength::Discouraged, Some(m)) => format!(", which is discouraged with {m}{reason}"),
            (Strength::Discouraged, None) => format!(", which is discouraged{reason}"),
            (Strength::Reference, Some(m)) => format!("; this differs from the {m} reference pipeline, which {uses}"),
            (Strength::Reference, None) if why => format!("; the reference pipeline {uses}{reason}"),
            (Strength::Reference, None) => format!("; this differs from the reference pipeline, which {uses}"),
            (_, Some(m)) => format!("; {m} {verb} {rec}{reason}"),
            (_, None) => format!("; {rec} is {verb}d{reason}").replace("recommendsd", "recommended").replace("requiresd", "required"),
        }
    }

    /// One line for a terminal or a log: "Background removal is V-SHARP; MEDI recommends
    /// PDF: why [source]".
    pub fn line(&self) -> String {
        format!("{} is {}{} [{}]", self.label, self.current_text(), self.departure(true), self.source)
    }

    /// The methods text's Note line for this departure, without its full stop: what was done, and
    /// what the method's reference uses ("Background removal used LBV; this differs from the RTS
    /// reference pipeline, which uses V-SHARP"). The rule's reason and source are for the log and
    /// the TUI, not a methods section.
    pub fn note(&self) -> String {
        let verb = if self.current.iter().all(|n| n.value.is_string()) { "used" } else { "was" };
        format!("{} {verb} {}{}", self.label, self.current_text(), self.departure(false))
    }
}

/// The settings whose values are methods (a rule triggered by one is that method's).
const METHOD_SETTINGS: [&str; 4] = ["inversion.algorithm", "bg_removal.algorithm", "masking.sections", "separation.algorithm"];

// ─── What is known about the scan ───

/// What is known about the scans a config runs on, for the rules that depend on them. Every fact
/// may be unknown; [`Target`](core::Target) is this build's (a browser build is `wasm`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub echo_count: Option<usize>,
    pub field_strength: Option<f64>,
    /// mm.
    pub voxel_size: Option<(f64, f64, f64)>,
    pub dims: Option<(usize, usize, usize)>,
    /// The sidecar's `Manufacturer`, as it gives it.
    pub manufacturer: Option<String>,
}

impl Facts {
    /// A scan's facts ([`crate::scan::ScanFacts`]); the voxel size only when the front end read it.
    pub fn of_scan(f: &crate::scan::ScanFacts) -> Self {
        Facts {
            echo_count: Some(f.n_echoes),
            field_strength: Some(f.field_strength),
            voxel_size: f.voxel_size,
            dims: Some(f.dims),
            manufacturer: f.manufacturer.clone(),
        }
    }

    /// A loaded run's facts: all those of its metadata. The metadata does not carry the
    /// manufacturer; [`Self::with_manufacturer`] adds the scan's.
    pub fn of_run(m: &crate::scan::RunMetadata) -> Self {
        Facts {
            echo_count: Some(m.n_echoes),
            field_strength: Some(m.field_strength),
            voxel_size: Some(m.voxel_size),
            dims: Some(m.dims),
            manufacturer: None,
        }
    }

    /// These facts with the scan's manufacturer.
    pub fn with_manufacturer(self, manufacturer: Option<String>) -> Self {
        Facts { manufacturer, ..self }
    }

    /// What a settings view knows ([`crate::schema::ViewFacts`]).
    pub fn of_view(v: &crate::schema::ViewFacts) -> Self {
        Facts {
            echo_count: v.num_echoes,
            field_strength: v.field_strength,
            voxel_size: v.voxel_size.map(|[x, y, z]| (x, y, z)),
            dims: v.dims.map(|[x, y, z]| (x, y, z)),
            manufacturer: v.manufacturer.clone(),
        }
    }

    fn core(&self) -> core::ScanFacts {
        core::ScanFacts {
            field_strength: self.field_strength,
            echo_count: self.echo_count,
            voxel_size: self.voxel_size,
            dims: self.dims,
            manufacturer: self.manufacturer.as_deref().map(core::Manufacturer::from_name),
            target: core::Target::current(),
        }
    }
}

// ─── QSM.rs's settings in QSMxT's terms ───

/// How a QSM.rs setting is one of QSMxT's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mapping {
    /// The same value at `qsmxt` (choices by the same names; numbers, flags and vectors as they
    /// are).
    Same,
    /// Each mask section's generator (`MaskOp::id`, the step's `op`).
    MaskGenerator,
    /// Each HD-BET step's inference (`full-volume`, `sliding-window`, `low-memory`,
    /// `custom-window`), its `patch` in QSMxT.
    HdBetInference,
    /// `dct`, `fft-12`, `fft` or `mirror`: the solver and, for FFT, its padding.
    LaplacianSolver,
    /// `native`, `isotropic-finest`, `isotropic-1mm` or `isotropic`: the grid and its spacing.
    VsharpGrid,
    /// `full-volume`, `low-memory` or `sliding-window`: SynthSeg's `patch`.
    SynthSegInference,
    /// SynthSeg's window edge: its `patch`.
    SynthSegPatch,
}

/// Each of QSM.rs's rule settings ([`core::SETTINGS`]), as a QSMxT setting.
const SETTINGS: &[(&str, &str, Mapping)] = &[
    ("masking.generator", "masking.sections", Mapping::MaskGenerator),
    ("masking.hd_bet.inference", "masking.sections", Mapping::HdBetInference),
    ("field_mapping.unwrapping_algorithm", "field_mapping.unwrapping_algorithm", Mapping::Same),
    ("field_mapping.b0_estimation", "field_mapping.b0_estimation", Mapping::Same),
    ("field_mapping.b0_estimation.weighting", "field_mapping.b0_weight_type", Mapping::Same),
    ("field_mapping.phase_offset_removal", "field_mapping.phase_offset_removal", Mapping::Same),
    ("field_mapping.phase_offset_sigma_mm", "field_mapping.phase_offset_sigma_mm", Mapping::Same),
    ("field_mapping.bipolar_correction", "field_mapping.bipolar_correction", Mapping::Same),
    ("field_mapping.laplacian_solver", "field_mapping.laplacian_solver", Mapping::LaplacianSolver),
    ("field_mapping.laplacian_unmasked", "field_mapping.laplacian_unmasked", Mapping::Same),
    ("bg_removal.algorithm", "bg_removal.algorithm", Mapping::Same),
    ("bg_removal.vsharp.deconvolve", "bg_removal.vsharp.deconvolve", Mapping::Same),
    ("bg_removal.vsharp.max_radius", "bg_removal.vsharp.max_radius", Mapping::Same),
    ("bg_removal.vsharp.min_radius", "bg_removal.vsharp.min_radius", Mapping::Same),
    ("bg_removal.vsharp.radius_step", "bg_removal.vsharp.radius_step", Mapping::Same),
    ("bg_removal.vsharp.grid", "bg_removal.vsharp.grid", Mapping::VsharpGrid),
    ("bg_removal.ismv.variant", "bg_removal.ismv.variant", Mapping::Same),
    ("bg_removal.ismv.tol", "bg_removal.ismv.tol", Mapping::Same),
    ("bg_removal.ismv.max_iter", "bg_removal.ismv.max_iter", Mapping::Same),
    ("bg_removal.resharp.radius", "bg_removal.resharp.radius", Mapping::Same),
    ("bg_removal.resharp.max_iter", "bg_removal.resharp.max_iter", Mapping::Same),
    ("inversion.algorithm", "inversion.algorithm", Mapping::Same),
    ("inversion.star.pad", "inversion.star.pad", Mapping::Same),
    ("inversion.ilsqr.max_iter", "inversion.ilsqr.max_iter", Mapping::Same),
    ("inversion.ilsqr.pad_mm", "inversion.ilsqr.pad_mm", Mapping::Same),
    ("inversion.whqsm.weight", "inversion.whqsm.weight", Mapping::Same),
    ("inversion.star.te", "inversion.star.te", Mapping::Same),
    ("inversion.fansi.te", "inversion.fansi.te", Mapping::Same),
    ("inversion.ndi.te", "inversion.ndi.te", Mapping::Same),
    ("inversion.l1qsm.te", "inversion.l1qsm.te", Mapping::Same),
    ("inversion.whqsm.te", "inversion.whqsm.te", Mapping::Same),
    ("inversion.medi.msmv", "inversion.medi.msmv", Mapping::Same),
    ("inversion.medi.msmv.prefilter", "inversion.medi.msmv_prefilter", Mapping::Same),
    ("inversion.medi.te", "inversion.medi.te", Mapping::Same),
    ("inversion.tfi.te", "inversion.tfi.te", Mapping::Same),
    ("inversion.apply_heidi", "inversion.heidi.enabled", Mapping::Same),
    ("inversion.heidi.te", "inversion.heidi.te", Mapping::Same),
    ("inversion.qsmart.inversion", "inversion.qsmart.inversion", Mapping::Same),
    ("inversion.qsmart.ilsqr_max_iter", "inversion.qsmart.ilsqr_max_iter", Mapping::Same),
    ("inversion.qsmart.ilsqr_pad_mm", "inversion.qsmart.ilsqr_pad_mm", Mapping::Same),
    ("reference", "qsm.reference", Mapping::Same),
    ("qsm_per_echo", "qsm.per_echo", Mapping::Same),
    ("tukey_window", "pipeline.tukey_window", Mapping::Same),
    ("separation.algorithm", "separation.algorithm", Mapping::Same),
    ("separation.decompose.unwrap_phase", "separation.decompose.unwrap_phase", Mapping::Same),
    ("segmentation.inference", "segmentation.patch", Mapping::SynthSegInference),
    ("segmentation.inference.patch", "segmentation.patch", Mapping::SynthSegPatch),
];

/// A QSM.rs rule setting as QSMxT's: listed in [`SETTINGS`], or else the QSMxT setting at the same
/// path, taken as it is ([`Mapping::Same`]). So a rule QSM.rs adds on a setting QSMxT already has
/// (`separation.decompose.chi_bound`, ...) is offered without a change here.
fn setting_of(core_path: &str) -> Option<(&'static str, Mapping)> {
    SETTINGS.iter().find(|(c, _, _)| *c == core_path).map(|&(_, q, k)| (q, k))
        .or_else(|| setting(core_path).map(|s| (s.path, Mapping::Same)))
}

/// Whether `id` names one of QSM.rs's rules.
pub fn is_rule(id: &str) -> bool {
    core::RULES.iter().any(|r| r.id == id)
}

/// Whether `id` is a reference-pipeline rule (`reference` strength): it is never applied by
/// `--accept-recommendations` and the methods Note lists its differences either way, so it has no
/// dismissal ([`keep`], [`dismiss`]).
pub fn is_reference_rule(id: &str) -> bool {
    core::RULES.iter().any(|r| r.id == id && r.strength == core::Strength::Reference)
}

/// The QSMxT config path of a QSM.rs rule setting (`reference` → `qsm.reference`).
pub fn qsmxt_path(core_path: &str) -> Option<&'static str> {
    setting_of(core_path).map(|(q, _)| q)
}

/// A rule's value (or a QSM.rs current value) as JSON, as QSM.rs serialises it.
fn core_json(v: &core::Value) -> Value {
    match *v {
        core::Value::Bool(b) => json!(b),
        core::Value::Number(x) => number(x),
        core::Value::Choice(c) => json!(c),
        core::Value::Vector(xs) => Value::Array(xs.iter().map(|&x| number(x)).collect()),
    }
}

fn current_json(v: &Current) -> Value {
    match v {
        Current::Bool(b) => json!(b),
        Current::Number(x) => number(*x),
        Current::Choice(c) => json!(c),
        Current::Vector(xs) => Value::Array(xs.iter().map(|&x| number(x)).collect()),
    }
}

/// A whole number as an integer (so an integer setting accepts it), anything else as a float.
fn number(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 { json!(x as i64) } else { json!(x) }
}

/// A schema setting, borrowed (looked up often: [`crate::schema::setting`] clones).
fn setting(path: &str) -> Option<&'static crate::schema::Setting> {
    crate::schema::settings().iter().find(|s| s.path == path)
}

/// The mask recipe's step types, built once.
fn mask_steps() -> &'static [crate::schema::mask::MaskStep] {
    static STEPS: std::sync::LazyLock<Vec<crate::schema::mask::MaskStep>> =
        std::sync::LazyLock::new(|| crate::schema::mask_schema().steps);
    &STEPS
}

/// A number as it is said (`2`, not `2.0`); anything else as JSON.
fn num_text(v: &Value) -> String {
    v.as_f64().map_or_else(|| v.to_string(), |x| x.to_string())
}

/// A value's display name for the QSMxT setting at `path` (of `kind`).
fn name_of(path: &str, kind: Mapping, value: &Value) -> String {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::Null => "none".into(),
        Value::Array(xs) => xs.iter().map(num_text).collect::<Vec<_>>().join(" × "),
        other => num_text(other),
    };
    match kind {
        Mapping::MaskGenerator => {
            let op = value.as_str().unwrap_or_default();
            mask_steps().iter().find(|s| s.op == op)
                .map_or_else(|| op.to_string(), |s| s.name.to_string())
        }
        Mapping::HdBetInference => match value.as_str().unwrap_or_default() {
            "full-volume" => "whole volume".into(),
            "sliding-window" => "sliding window".into(),
            "low-memory" => "low-memory sliding window".into(),
            other => other.replace('-', " "),
        },
        Mapping::LaplacianSolver => match value.as_str().unwrap_or_default() {
            "dct" => "DCT".into(),
            "fft-12" => "FFT, 12-voxel padding".into(),
            "fft" => "FFT".into(),
            "mirror" => "mirror".into(),
            other => other.into(),
        },
        Mapping::VsharpGrid => match value.as_str().unwrap_or_default() {
            "native" => "acquired grid".into(),
            "isotropic-finest" => "isotropic at the finest voxel size".into(),
            "isotropic-1mm" => "1 mm isotropic grid".into(),
            "isotropic" => "isotropic grid".into(),
            other => other.into(),
        },
        Mapping::SynthSegInference => match value.as_str().unwrap_or_default() {
            "full-volume" => "whole volume".into(),
            "low-memory" => "low-memory sliding window".into(),
            "sliding-window" => "sliding window".into(),
            other => other.into(),
        },
        Mapping::SynthSegPatch => format!("{}-voxel window", text(value)),
        Mapping::Same => {
            let Some(s) = setting(path) else { return text(value) };
            if let Some(o) = s.value_type.options().into_iter().find(|o| o.value == *value) {
                return o.name.clone();
            }
            match (s.units, value) {
                (Some(u), Value::Number(_) | Value::Array(_)) => format!("{} {u}", text(value)),
                _ => text(value),
            }
        }
    }
}

/// The current value(s) of a QSM.rs setting in QSMxT's terms: for a [`Mapping::Same`] setting, the
/// config's own value (QSMxT's region referencing is `none` to QSM.rs, but `region` here).
fn current_named(config: &Value, core_path: &str, core_current: &[Current]) -> Vec<Named> {
    let Some((path, kind)) = setting_of(core_path) else { return vec![] };
    match kind {
        Mapping::Same => {
            let v = path.split('.').try_fold(config, |v, k| v.get(k)).cloned().unwrap_or(Value::Null);
            vec![Named { name: name_of(path, kind, &v), value: v }]
        }
        _ => core_current.iter().map(|c| {
            let v = current_json(c);
            Named { name: name_of(path, kind, &v), value: v }
        }).collect(),
    }
}

fn named(core_path: &str, v: &core::Value) -> Named {
    let (path, kind) = setting_of(core_path).unwrap_or(("", Mapping::Same));
    let v = core_json(v);
    Named { name: name_of(path, kind, &v), value: v }
}

fn label_of(core_path: &str) -> String {
    match core_path {
        "masking.generator" => "Brain mask".into(),
        "masking.hd_bet.inference" => "HD-BET inference".into(),
        "segmentation.inference" => "SynthSeg inference".into(),
        _ => {
            let path = qsmxt_path(core_path).unwrap_or(core_path);
            setting(path).map_or_else(|| path.to_string(), |s| match s.context {
                Some(ctx) if !s.full_label.contains(ctx) => format!("{} ({ctx})", s.full_label),
                _ => s.full_label.to_string(),
            })
        }
    }
}

// ─── Evaluating ───

/// The rules with an expectation nothing passes: a rule's copy that fires whenever the rule
/// applies (its conditions hold and its setting is in play), so [`core::check`] says which rules
/// apply, departed from or not, with QSM.rs's own semantics.
fn applying() -> &'static [Rule] {
    static RULES: std::sync::OnceLock<Vec<Rule>> = std::sync::OnceLock::new();
    RULES.get_or_init(|| core::RULES.iter().map(|r| Rule { expect: Test::OneOf { values: &[] }, ..*r }).collect())
}

/// The rule settings that belong to the QSM reconstruction: not in play when QSM is not
/// reconstructed.
fn reconstructs(core_path: &str) -> bool {
    ["field_mapping.", "bg_removal.", "inversion.", "reference", "qsm_per_echo"].iter().any(|p| core_path.starts_with(p))
}

/// Every rule that applies to `config` on a scan with `facts`, in QSM.rs's order: its trigger is
/// chosen and its setting is in play, whether or not the setting departs from it. The always-on
/// reference values a front end shows next to a setting.
pub fn reference_values(config: &PipelineConfig, facts: &Facts) -> Vec<Recommendation> {
    let c = &config.normalised().0;
    let pipeline = crate::bridge::to_qsm_pipeline_config(c);
    let separation = crate::bridge::to_separation_config(c);
    let segmentation = crate::bridge::to_synthseg_params(&c.segmentation);
    let input = RuleInput {
        pipeline: &pipeline,
        separation: c.pipeline.do_chi_separation.then_some(&separation),
        segmentation: c.pipeline.do_segmentation.then_some(&segmentation),
    };
    let scan = facts.core();
    let json = serde_json::to_value(c).unwrap_or(Value::Null);
    let mut out: Vec<Recommendation> = core::RULES.iter().zip(applying()).filter_map(|(rule, probe)| {
        let Condition::Setting { setting: trigger, .. } = rule.trigger() else { return None };
        if !c.pipeline.do_qsm && (reconstructs(trigger) || reconstructs(rule.setting)) {
            return None;
        }
        let applies = core::check(probe, &input, &scan)?;
        let departs = core::check(rule, &input, &scan).is_some();
        Some(recommendation(&json, rule, &input, &scan, &applies.current, departs))
    }).collect();
    mark(config, &mut out);
    out
}

/// The findings on `config` on a scan with `facts`: every rule that applies and is departed from.
pub fn recommendations(config: &PipelineConfig, facts: &Facts) -> Vec<Recommendation> {
    reference_values(config, facts).into_iter().filter(|r| r.departs).collect()
}

fn recommendation(
    c: &Value, rule: &'static Rule, input: &RuleInput, scan: &core::ScanFacts, current: &[Current], departs: bool,
) -> Recommendation {
    let trigger = match rule.trigger() {
        Condition::Setting { setting, is } => {
            let path = qsmxt_path(setting).unwrap_or(setting);
            let values = core::current(input, scan, setting).unwrap_or_default();
            let named = current_named(c, setting, &values);
            // The value that triggered it: the first one the condition holds for.
            let hit = values.iter().position(|v| match is {
                Test::OneOf { values } => values.iter().any(|x| current_json(v) == core_json(x)),
                _ => true,
            }).and_then(|i| named.get(i).or(named.first())).or(named.first());
            let hit = hit.cloned().unwrap_or(Named { value: Value::Null, name: String::new() });
            Trigger { setting: path.to_string(), value: hit.value, name: trigger_name(setting, &hit.name) }
        }
        Condition::Fact { .. } => Trigger { setting: String::new(), value: Value::Null, name: String::new() },
    };
    let names = |values: &[core::Value]| values.iter().map(|v| named(rule.setting, v)).collect();
    let allowed = match rule.expect {
        Test::OneOf { values } => Allowed::OneOf { values: names(values) },
        Test::NoneOf { values } => Allowed::NoneOf { values: names(values) },
        Test::Range { min, max } => Allowed::Range { min: min.map(number), max: max.map(number) },
    };
    Recommendation {
        rule: rule.id.to_string(),
        kind: rule.kind().into(),
        strength: rule.strength.into(),
        basis: rule.basis.into(),
        setting: qsmxt_path(rule.setting).unwrap_or(rule.setting).to_string(),
        label: label_of(rule.setting),
        trigger,
        current: current_named(c, rule.setting, current),
        recommended: named(rule.setting, &rule.default),
        allowed,
        reason: rule.reason.to_string(),
        source: rule.source.to_string(),
        needs_scan: rule.needs_scan(),
        departs,
        dismissed: false,
        conflict: false,
    }
}

/// How a trigger is named: the method ("MEDI"), with what it is for a setting that is not a
/// method ("bipolar correction").
fn trigger_name(core_path: &str, value_name: &str) -> String {
    match core_path {
        "inversion.algorithm" | "bg_removal.algorithm" | "masking.generator" => value_name.to_string(),
        "separation.algorithm" => value_name.to_string(),
        // The echo combination by its method's name: "Complex fit" (MEDI's Fit_ppm_complex).
        "field_mapping.b0_estimation" => value_name.to_string(),
        _ => format!("{} {}", label_of(core_path).to_lowercase(), value_name),
    }
}

/// Mark each finding dismissed (by `config`'s dismissals) or in conflict with another.
fn mark(config: &PipelineConfig, out: &mut [Recommendation]) {
    for r in out.iter_mut() {
        r.dismissed = r.departs && r.strength.dismissable() && config.dismissed_recommendations.iter()
            .any(|d| d.rule == r.rule && d.chosen == chosen_text(&r.current));
    }
    let snapshot: Vec<(String, Strength, Value, bool)> =
        out.iter().map(|r| (r.setting.clone(), r.strength, r.recommended.value.clone(), r.departs)).collect();
    for (i, r) in out.iter_mut().enumerate() {
        r.conflict = r.departs && snapshot.iter().enumerate().any(|(j, (s, st, v, d))| {
            j != i && *d && *s == r.setting && *st == r.strength && *v != r.recommended.value
        });
    }
}

/// How a dismissal records the value(s) it was dismissed for: each value as the config writes it
/// (a vector's numbers joined by commas), the values of a recipe's sections joined by `/`. Written
/// in characters a shell leaves alone, so `--dismiss-recommendation rule=chosen` needs no quoting.
pub fn chosen_text(current: &[Named]) -> String {
    let one = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Array(xs) => xs.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","),
        other => other.to_string(),
    };
    current.iter().map(|n| one(&n.value)).collect::<Vec<_>>().join("/")
}

// ─── By method ───

/// One method's reference pipeline as far as it covers a config: the `reference` rules scoped to
/// that method (their trigger), departed from or not. What a front end opens when the method is
/// chosen ([`chosen_methods`], [`reference_differences`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodReference {
    /// The choice the rules are scoped to ("MEDI").
    pub method: Trigger,
    /// "MEDI's reference pipeline".
    pub title: String,
    pub rules: Vec<Recommendation>,
}

impl MethodReference {
    /// The rules the config departs from and has not kept ([`keep`]).
    pub fn differences(&self) -> Vec<Recommendation> {
        self.rules.iter().filter(|r| r.departs && !r.dismissed).cloned().collect()
    }
}

/// What a pop-up about `method`'s reference pipeline is called: "MEDI's reference pipeline" for a
/// method, "The reference for phase offset removal on" for another setting's value.
pub fn reference_title(method: &Trigger) -> String {
    if METHOD_SETTINGS.contains(&method.setting.as_str()) {
        format!("{}'s reference pipeline", method.name)
    } else {
        format!("The reference for {}", method.name)
    }
}

/// The `reference` rules of `view` ([`reference_values`]) grouped by the choice they are scoped
/// to, in the order the choices first appear.
pub fn by_method(view: &[Recommendation]) -> Vec<MethodReference> {
    let mut out: Vec<MethodReference> = Vec::new();
    for r in view.iter().filter(|r| r.strength == Strength::Reference && !r.trigger.setting.is_empty()) {
        match out.iter_mut().find(|m| m.method == r.trigger) {
            Some(m) => m.rules.push(r.clone()),
            None => out.push(MethodReference { method: r.trigger.clone(), title: reference_title(&r.trigger), rules: vec![r.clone()] }),
        }
    }
    out
}

/// The QSMxT settings a choice of which can open a reference pipeline pop-up: those that trigger
/// a `reference` rule (the inversion, background removal, χ-separation, phase offset removal),
/// from QSM.rs's rules. A front end shows them as a picker
/// rather than cycling through their values, so no method is chosen on the way to another.
pub fn reference_trigger_settings() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for r in core::RULES.iter().filter(|r| r.strength == core::Strength::Reference) {
        let Condition::Setting { setting, .. } = r.trigger() else { continue };
        if let Some(path) = qsmxt_path(setting) {
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// The choices in `config` that have a reference pipeline: the triggers of the `reference` rules
/// that apply. A front end compares them before and after a change; a choice that appears is a
/// method just chosen, and its [`reference_differences`] are what to offer.
pub fn chosen_methods(config: &PipelineConfig, facts: &Facts) -> Vec<Trigger> {
    by_method(&reference_values(config, facts)).into_iter().map(|m| m.method).collect()
}

/// Where `config` differs from `method`'s reference pipeline and has not kept its own value
/// ([`keep`]): each setting's current value, the reference value, why and the source. Empty when
/// it does not differ, or `method` is not chosen.
pub fn reference_differences(config: &PipelineConfig, facts: &Facts, method: &Trigger) -> Vec<Recommendation> {
    by_method(&reference_values(config, facts)).into_iter()
        .find(|m| m.method == *method)
        .map(|m| m.differences())
        .unwrap_or_default()
}

/// The marker a setting shows when it departs from a rule ([`marker`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// A finding offered as a change and not dismissed (⚑); otherwise information only (ⓘ): a
    /// `reference` difference, or a finding kept.
    pub actionable: bool,
    /// Whose rule and what it uses: "MEDI: BET", "MEDI, PDF: BET", "MEDI: PDF · TFI: LBV".
    pub text: String,
}

/// The marker for the setting at `path`, from the rules of `view` ([`reference_values`]) that it
/// departs from: `None` when it departs from none. Offered findings are named first.
pub fn marker(view: &[Recommendation], path: &str) -> Option<Marker> {
    let mut here: Vec<&Recommendation> = view.iter().filter(|r| r.setting == path && r.departs).collect();
    if here.is_empty() {
        return None;
    }
    let open = |r: &Recommendation| r.actionable() && !r.dismissed;
    here.sort_by_key(|r| !open(r));
    // Each value, with every method whose rule has it.
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for r in &here {
        let value = if r.strength == Strength::Discouraged { format!("not {}", r.current_text()) } else { r.recommended.name.clone() };
        let who = if r.trigger.name.is_empty() { "Reference".to_string() } else { r.trigger.name.clone() };
        match groups.iter_mut().find(|(v, _)| *v == value) {
            Some((_, names)) if names.contains(&who) => {}
            Some((_, names)) => names.push(who),
            None => groups.push((value, vec![who])),
        }
    }
    let text = groups.iter().map(|(v, names)| format!("{}: {v}", names.join(", "))).collect::<Vec<_>>().join(" · ");
    Some(Marker { actionable: here.iter().any(|r| open(r)), text })
}

/// Keep the setting of each finding at its current value: dismiss each for that value
/// ([`Dismissal`], into a config's `dismissed_recommendations`), so it is not offered again while
/// the setting keeps it. A `required` finding cannot be kept and is left; a `reference` one is
/// information only and records nothing. What "Keep mine" does.
pub fn keep(dismissals: &mut Vec<Dismissal>, findings: &[Recommendation]) {
    for r in findings.iter().filter(|r| r.strength.dismissable()) {
        dismissals.retain(|d| d.rule != r.rule);
        dismissals.push(Dismissal { rule: r.rule.clone(), chosen: chosen_text(&r.current) });
    }
}

/// Apply the findings the user chose together: what "Apply all" does, with the precedence
/// [`settle`] gives (the same as [`accept`]'s), so the order of `findings` does not matter. A
/// finding left by that precedence changes nothing and is not an error: the returned [`Settled`]
/// says which findings were applied and which were left, and the setting keeps its value and goes
/// on showing them. Every finding `settle` takes is tried; `Err` lists those that could not be
/// applied.
pub fn apply_all(config: &mut PipelineConfig, findings: &[Recommendation]) -> Result<Settled, String> {
    let settled = settle(findings);
    let errors: Vec<String> = settled.apply.iter().filter_map(|r| apply(config, &r.rule, None).err()).collect();
    if errors.is_empty() { Ok(settled) } else { Err(errors.join("; ")) }
}

// ─── Dismissing ───

/// Dismiss the finding of `rule` on `config` for the setting's current value: it is no longer
/// offered while the setting keeps that value. `Err` when the rule is a reference-pipeline note
/// ([`reference_rule_refusal`]), has no finding on `config`, or is `required` (the method cannot
/// run otherwise).
pub fn dismiss(config: &mut PipelineConfig, rule: &str, facts: &Facts) -> Result<(), String> {
    if is_reference_rule(rule) {
        return Err(reference_rule_refusal(rule));
    }
    let r = recommendations(config, facts).into_iter().find(|r| r.rule == rule)
        .ok_or_else(|| format!("no recommendation {rule} applies to this configuration"))?;
    if r.strength == Strength::Required {
        return Err(format!("{rule} is required: the method cannot run otherwise"));
    }
    let chosen = chosen_text(&r.current);
    config.dismissed_recommendations.retain(|d| d.rule != rule);
    config.dismissed_recommendations.push(Dismissal { rule: rule.to_string(), chosen });
    Ok(())
}

/// Why a reference-pipeline rule cannot be dismissed, as front ends say it.
pub fn reference_rule_refusal(rule: &str) -> String {
    format!("{rule} is a reference-pipeline note, not a recommendation; there is nothing to dismiss")
}

/// Take back the dismissal of `rule`.
pub fn undismiss(config: &mut PipelineConfig, rule: &str) {
    config.dismissed_recommendations.retain(|d| d.rule != rule);
}

/// `config`'s dismissals less those that no longer match a finding (the setting has changed, or
/// the rule no longer applies): what a front end keeps after a change, so going back to a value
/// brings its finding back. Scan-dependent findings are kept while the scan is unknown.
pub fn prune_dismissals(config: &mut PipelineConfig, facts: &Facts) {
    let all = reference_values(config, facts);
    config.dismissed_recommendations.retain(|d| {
        let rule = core::RULES.iter().find(|r| r.id == d.rule);
        let reference = rule.is_some_and(|r| r.strength == core::Strength::Reference);
        let silent_until_scan = rule.is_some_and(|r| r.needs_scan()) && facts.echo_count.is_none();
        !reference && (silent_until_scan || all.iter().any(|r| r.rule == d.rule && r.departs && r.dismissed))
    });
}

// ─── Applying ───

/// Change `config` so the finding of `rule` is resolved: to the rule's recommended value, or, with
/// `pick`, to another value it allows (one of [`Allowed::OneOf`]'s, or a number in its
/// [`Allowed::Range`]). Any dismissal of the rule is dropped.
pub fn apply(config: &mut PipelineConfig, rule: &str, pick: Option<&Value>) -> Result<(), String> {
    let r = core::RULES.iter().find(|r| r.id == rule).ok_or_else(|| format!("no rule {rule}"))?;
    let value = match (pick, r.expect) {
        (None, _) => core_json(&r.default),
        (Some(p), Test::OneOf { values }) => values.iter().map(core_json).find(|v| v == p)
            .ok_or_else(|| format!("{rule} does not allow {p}"))?,
        (Some(p), Test::Range { min, max }) => {
            let x = p.as_f64().ok_or_else(|| format!("{rule} takes a number, not {p}"))?;
            if min.is_some_and(|m| x < m) || max.is_some_and(|m| x > m) {
                return Err(format!("{x} is outside what {rule} allows"));
            }
            number(x)
        }
        (Some(p), Test::NoneOf { values }) => {
            if values.iter().map(core_json).any(|v| v == *p) {
                return Err(format!("{rule} discourages {p}"));
            }
            p.clone()
        }
    };
    set(config, r.setting, &value)?;
    undismiss(config, rule);
    Ok(())
}

/// Set the QSM.rs setting `core_path` to `value` (QSM.rs's terms) in `config`.
fn set(config: &mut PipelineConfig, core_path: &str, value: &Value) -> Result<(), String> {
    let (path, kind) = setting_of(core_path).ok_or_else(|| format!("QSMxT has no setting for {core_path}"))?;
    let id = value.as_str().unwrap_or_default();
    match kind {
        Mapping::Same => {
            crate::schema::set(config, path, value.clone())?;
        }
        Mapping::MaskGenerator => {
            // The preset whose sections all start with that generator (`bet` → BET on the
            // magnitude, then erode).
            let (_, recipe) = crate::masking::mask_presets().into_iter()
                .find(|(_, r)| !r.sections.is_empty() && r.sections.iter().all(|s| op_id(&s.generator) == id))
                .ok_or_else(|| format!("no mask preset starts with {id}"))?;
            config.masking.sections = recipe.sections;
            config.masking.combine = recipe.combine;
            config.masking.refinements = recipe.refinements;
        }
        Mapping::HdBetInference => {
            let patch = match id {
                "full-volume" => None,
                "sliding-window" => Some(MaskOp::hd_bet_default_patch()),
                "low-memory" => Some(crate::masking::hd_bet_low_memory_patch()),
                other => return Err(format!("HD-BET inference {other} is not one QSMxT can set")),
            };
            for s in &mut config.masking.sections {
                for op in std::iter::once(&mut s.generator).chain(s.refinements.iter_mut()) {
                    if let MaskOp::HdBet { patch: p, .. } = op {
                        *p = patch;
                    }
                }
            }
        }
        Mapping::LaplacianSolver => {
            let fm = &mut config.field_mapping;
            fm.laplacian_solver = match id {
                "dct" => LaplacianSolver::Dct,
                "mirror" => LaplacianSolver::Mirror,
                "fft" => LaplacianSolver::Fft,
                "fft-12" => {
                    let qsm_core::unwrap::LaplacianSolver::Fft { pad } = qsm_core::unwrap::LaplacianSolver::FFT_DEFAULT_PAD
                    else { unreachable!("FFT_DEFAULT_PAD is an FFT solver") };
                    fm.laplacian_fft_pad = pad;
                    LaplacianSolver::Fft
                }
                other => return Err(format!("unknown Laplacian solver {other}")),
            };
        }
        Mapping::VsharpGrid => {
            let v = &mut config.bg_removal.vsharp;
            v.grid = match id {
                "native" => VsharpGrid::Native,
                "isotropic-finest" => VsharpGrid::Finest,
                "isotropic-1mm" => {
                    v.grid_spacing = 1.0;
                    VsharpGrid::Isotropic
                }
                "isotropic" => VsharpGrid::Isotropic,
                other => return Err(format!("unknown V-SHARP grid {other}")),
            };
        }
        Mapping::SynthSegInference => {
            config.segmentation.patch = crate::config::synthseg_patch(match id {
                "full-volume" => qsm_core::segment::SynthSegInference::FullVolume,
                "low-memory" => qsm_core::segment::SynthSegInference::LOW_MEMORY,
                other => return Err(format!("SynthSeg inference {other} needs a window size")),
            });
        }
        Mapping::SynthSegPatch => {
            config.segmentation.patch = Some(value.as_u64().ok_or_else(|| format!("{value} is not a window size"))? as usize);
        }
    }
    Ok(())
}

/// QSM.rs's id of a mask op (`MaskOp::id`): the step's `op`.
fn op_id(op: &MaskOp) -> &'static str {
    let kind = crate::schema::mask::step_kind(op);
    mask_steps().iter().find(|s| s.kind == kind).map_or("", |s| s.op)
}

/// What [`accept`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Accept {
    /// The reasoned findings (`--accept-recommendations`).
    pub recommendations: bool,
    /// The original authors' settings (`--accept-reference-settings`).
    pub reference: bool,
}

impl Accept {
    pub fn any(self) -> bool {
        self.recommendations || self.reference
    }
}

/// What findings chosen together come to, setting by setting ([`settle`]). Every host decides this
/// here (#493): `qsmxt run` through [`accept`], qsmbly through [`apply_all`], the TUI through
/// [`settle`] on its pop-up's picks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settled {
    /// The findings whose value the setting takes: the strongest on each setting, with any weaker
    /// one that wants the same value.
    pub apply: Vec<Recommendation>,
    /// The findings left because they are as strong as each other on one setting and want
    /// different values. The setting keeps its value for the user to choose.
    pub conflicting: Vec<Recommendation>,
    /// The findings left because a stronger one on the setting wants another value.
    pub outranked: Vec<Recommendation>,
}

/// The precedence of findings chosen together, in any order and with nothing changed: on each
/// setting, the strongest finding that is open (departed from and not dismissed) takes it, so a
/// reasoned finding outranks an authors' setting, unless two as strong disagree, when none is
/// taken. Findings on other settings are unaffected.
pub fn settle(findings: &[Recommendation]) -> Settled {
    settle_where(findings, |_| true)
}

/// [`settle`] over the findings `takes` lets in.
fn settle_where(findings: &[Recommendation], takes: impl Fn(&Recommendation) -> bool) -> Settled {
    let mut settled = Settled::default();
    let mut settings: Vec<&str> = Vec::new();
    for r in findings {
        if !settings.contains(&r.setting.as_str()) {
            settings.push(&r.setting);
        }
    }
    for s in settings {
        let open: Vec<&Recommendation> = findings.iter()
            .filter(|r| r.setting == s && r.departs && !r.dismissed && takes(r)).collect();
        let Some(top) = open.iter().map(|r| r.strength).max() else { continue };
        let strongest: Vec<&Recommendation> = open.iter().copied().filter(|r| r.strength == top).collect();
        let value = &strongest[0].recommended.value;
        if strongest.iter().any(|r| r.recommended.value != *value) {
            let (level, weaker): (Vec<_>, Vec<_>) = open.into_iter().partition(|r| r.strength == top);
            settled.conflicting.extend(level.into_iter().cloned());
            settled.outranked.extend(weaker.into_iter().cloned());
            continue;
        }
        let (same, other): (Vec<_>, Vec<_>) = open.into_iter().partition(|r| r.recommended.value == *value);
        settled.apply.extend(same.into_iter().cloned());
        settled.outranked.extend(other.into_iter().cloned());
    }
    settled
}

/// What `--accept-recommendations` and `--accept-reference-settings` change: the findings of the
/// kinds `which` takes, as [`settle`] decides among them. Returns the findings applied (as they
/// were before the change) and those left because they conflict.
pub fn accept(config: &mut PipelineConfig, findings: &[Recommendation], which: Accept) -> (Vec<Recommendation>, Vec<Recommendation>) {
    let settled = settle_where(findings, |r| match r.kind {
        Kind::Reasoned => which.recommendations && r.actionable(),
        Kind::Reference => which.reference,
    });
    let mut applied = Vec::new();
    for r in settled.apply {
        match apply(config, &r.rule, None) {
            Ok(()) => applied.push(r),
            Err(e) => log::warn!("could not apply {}: {e}", r.rule),
        }
    }
    (applied, settled.conflicting)
}

/// Every recommendation for a config across the scans it runs on: those that do not depend on
/// the scan once, and each scan-dependent one once for each different finding, in QSM.rs's order.
pub fn across_scans(config: &PipelineConfig, scans: &[Facts]) -> Vec<Recommendation> {
    let mut out: Vec<Recommendation> = Vec::new();
    let unknown = [Facts::default()];
    for facts in if scans.is_empty() { &unknown[..] } else { scans } {
        for r in recommendations(config, facts) {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    let order = |r: &Recommendation| core::RULES.iter().position(|x| x.id == r.rule).unwrap_or(usize::MAX);
    out.sort_by_key(order);
    out
}

// ─── What a run kept ───

/// What became of a finding in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Applied before the run (`--accept-recommendations`): the run took the recommended value.
    Applied,
    /// Dismissed: the run kept the chosen value.
    Dismissed,
    /// Neither applied nor dismissed: the run kept the chosen value.
    Kept,
}

/// A finding as a run record keeps it ([`crate::record::RunRecord::recommendations`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorded {
    #[serde(flatten)]
    pub finding: Recommendation,
    pub outcome: Outcome,
}

/// The findings a run of `config` on a scan with `facts` keeps: every departure, dismissed or not.
pub fn recorded(config: &PipelineConfig, facts: &Facts) -> Vec<Recorded> {
    recommendations(config, facts).into_iter().map(|r| {
        let outcome = if r.dismissed { Outcome::Dismissed } else { Outcome::Kept };
        Recorded { finding: r, outcome }
    }).collect()
}

/// The methods text's Note for the findings a dataset's `runs` kept (one run's: `runs` 1): the
/// departures from recommendations, then the differences from the reference pipelines, each once,
/// with how many runs it was found in when not all. Applied findings are not departures, and a
/// `required` one cannot have run.
pub fn note(findings: &[crate::record::RecommendationCount], runs: usize) -> crate::methods::Note {
    let lines = |strengths: &[Strength]| -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for c in findings.iter().filter(|c| c.recorded.outcome != Outcome::Applied && strengths.contains(&c.recorded.finding.strength)) {
            // Found in several runs as different records (one dismissed, one not): counted together.
            let n: usize = findings.iter().filter(|o| o.recorded.finding.rule == c.recorded.finding.rule
                && o.recorded.finding.current == c.recorded.finding.current && o.recorded.outcome != Outcome::Applied)
                .map(|o| o.runs).sum();
            let mut line = c.recorded.finding.note();
            if n < runs {
                line.push_str(&format!("; in {n} of {runs} runs"));
            }
            line.push('.');
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
        lines
    };
    crate::methods::Note {
        departures: lines(&[Strength::Recommended, Strength::Discouraged]),
        differences: lines(&[Strength::Reference]),
    }
}

/// The Note for a config before any scan is known: the findings that do not depend on the scan.
pub fn config_note(config: &PipelineConfig) -> crate::methods::Note {
    let found: Vec<crate::record::RecommendationCount> = recorded(config, &Facts::default()).into_iter()
        .map(|recorded| crate::record::RecommendationCount { recorded, runs: 1 }).collect();
    note(&found, 1)
}

#[cfg(test)]
mod tests;
