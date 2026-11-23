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
//! - **A rule never changes a hidden setting** ([`rule_may_change`], decision 194): one that does
//!   not apply in the config (`sti.unwrap-laplacian` under iQFM, which maps no field) is left as it
//!   is by [`apply`], [`apply_all`] and [`accept`]. It would do nothing for the run, and would come
//!   back changed when the setting applies again. The finding is still shown. A pipeline's own
//!   choices are not rules, and apply whole ([`crate::pipelines::choose`]).
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
//! - **Chosen methods' authors who disagree** on a setting ([`disagreeing`]: PDF's complex fit,
//!   AMP-PE's average of φ/TE) leave it as it is, whichever of them (or neither) it follows: the
//!   finding is a [`Recommendation::conflict`], which [`settle`], and so
//!   `--accept-reference-settings`, never applies, so the setting cannot flip between the two
//!   from one run to the next. A front end shows the authors' values as one choice for the user.
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

mod apply;
mod by_method;
mod evaluate;
mod mapping;

pub use apply::*;
pub use by_method::*;
pub use evaluate::*;
pub use mapping::*;

/// How strongly a rule applies (QSM.rs's `Strength`). Ordered `Reference < Suggested < Discouraged <
/// Recommended < Required`: on one setting, the strongest finding takes precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    /// What the method's reference pipeline used, where its authors never said it is needed or
    /// better. Information only.
    Reference,
    /// What QSM.rs measured to be better on some data but not all (in vivo, not on the simulated
    /// head): offered with its reason, but Keep mine is preselected and `--accept-recommendations`
    /// leaves it ([`Strength::preselected`]).
    Suggested,
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
            Strength::Suggested => "suggested",
            Strength::Discouraged => "discouraged",
            Strength::Recommended => "recommended",
            Strength::Required => "required",
        }
    }

    /// Whether a finding of this strength is offered as a change (a modal, a badge): the reasoned
    /// ones, every strength but `reference`.
    pub fn actionable(self) -> bool {
        self != Strength::Reference
    }

    /// Whether the change is preselected and applied by `--accept-recommendations`: every reasoned
    /// strength but `suggested` (QSM.rs's `Strength::preselected`).
    pub fn preselected(self) -> bool {
        matches!(self, Strength::Discouraged | Strength::Recommended | Strength::Required)
    }

    /// Whether a finding of this strength can be dismissed ("Keep mine", `--dismiss-recommendation`):
    /// `recommended`, `suggested` and `discouraged`. A `required` one must be followed, and a `reference` one is
    /// information only: never applied, and in the methods Note either way.
    pub fn dismissable(self) -> bool {
        matches!(self, Strength::Recommended | Strength::Suggested | Strength::Discouraged)
    }
}

impl From<core::Strength> for Strength {
    fn from(s: core::Strength) -> Self {
        match s {
            core::Strength::Reference => Strength::Reference,
            core::Strength::Suggested => Strength::Suggested,
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
    /// QSMxT's own recommendation, outside the reference pipeline: something the original
    /// toolbox does not offer, which QSMxT can do instead ([`qsmxt_findings`]).
    Qsmxt,
    /// QSMxT's wiring: an output another setting reads, left off
    /// ([`crate::normalise::required_findings`]). Always `required`, and unlike QSM.rs's
    /// `required` rules, never a refusal: a run applies it itself, says so in its log and records
    /// it ([`Recommendation::applied_by_run`]).
    Wiring,
    /// What the scan's acquisition needs: bipolar correction for a bipolar readout
    /// ([`crate::normalise::scan_adjustments`]). Always `required`, and, like [`Basis::Wiring`],
    /// applied by a run that is left without it, logged and recorded.
    Acquisition,
}

impl Basis {
    pub fn label(self) -> &'static str {
        match self {
            Basis::Stated => "stated by the authors",
            Basis::Measured => "measured in QSM.rs",
            Basis::ReferenceChoice => "the reference pipeline's choice",
            Basis::Implementation => "QSM.rs's implementation",
            Basis::Qsmxt => "QSMxT's recommendation, not offered by the reference toolbox",
            Basis::Wiring => "an output another setting needs",
            Basis::Acquisition => "what the scan's acquisition needs",
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
    /// show them together, with neither preselected, and leave the choice to the user. For an
    /// authors' setting, that includes another chosen method's authors' setting the config
    /// already follows ([`disagreeing`]): applying this one would only make that one depart.
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

    /// A required change the run makes itself when it is left unresolved: an output another
    /// setting needs ([`Basis::Wiring`]). A front end offers it ("Required change") when that
    /// setting is chosen; a run turns the output on, logs it and records it
    /// ([`PipelineConfig::required_changes`]). QSM.rs's `required` rules are not: a run refuses
    /// them ([`crate::requirements`]).
    pub fn applied_by_run(&self) -> bool {
        matches!(self.basis, Basis::Wiring | Basis::Acquisition)
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

    /// The rule is scoped to a flag that is on ("HEIDI streaking reduction"), so the finding also
    /// resolves by turning that flag off. Returns its config path. A method has no such way out:
    /// nothing says which other method to choose.
    pub fn flag_trigger(&self) -> Option<&str> {
        (self.kind == Kind::Reasoned && !self.applied_by_run() && self.trigger.value == Value::Bool(true)
            && self.trigger.setting != self.setting)
            .then_some(self.trigger.setting.as_str())
    }

    /// The flag of [`Self::flag_trigger`], as "turn off" names it: "HEIDI streaking reduction",
    /// "bipolar correction".
    pub fn flag_name(&self) -> &str {
        self.trigger.name.strip_suffix(" on").unwrap_or(&self.trigger.name)
    }

    /// The choice the rule is scoped to ("MEDI", "HEIDI streaking reduction"); `None` when its
    /// trigger is the setting itself, or a fact about the scan.
    fn scope(&self) -> Option<&str> {
        (!self.trigger.name.is_empty() && self.trigger.setting != self.setting).then_some(&self.trigger.name)
    }

    /// [`Self::scope`], when that choice is a method.
    fn method(&self) -> Option<&str> {
        self.scope().filter(|_| METHOD_SETTINGS.contains(&self.trigger.setting.as_str()))
    }

    /// The setting is a flag.
    fn flag(&self) -> bool {
        // A part of the mask recipe (its majority filter) is named by its value, like the others.
        self.recommended.value.is_boolean() && self.setting != "masking.sections"
    }

    /// What the departure is from, after the current value: "; MEDI recommends PDF: why",
    /// ", which is discouraged with NDI: why", "; this differs from the MEDI reference pipeline,
    /// which uses BET". Each names the choice the rule is scoped to, so the reader can tell whose
    /// it is. With `why` off, the rule's reason is left out, but for a discouraged value, which
    /// the sentence alone would not explain: what the methods text says.
    fn departure(&self, why: bool) -> String {
        let rec = if self.flag() { format!("turning it {}", self.recommended.name) } else { self.recommended.name.clone() };
        let uses = if self.flag() { format!("has it {}", self.recommended.name) } else { format!("uses {}", self.recommended.name) };
        // A suggestion is said as one, not as a recommendation (decided 2026-11-12).
        let (verb, needed) = match self.strength {
            Strength::Required => ("requires", "required"),
            Strength::Suggested => ("suggests", "suggested"),
            _ => ("recommends", "recommended"),
        };
        let reason = if why || self.strength == Strength::Discouraged { format!(": {}", self.reason) } else { String::new() };
        match (self.strength, self.method(), self.scope()) {
            (Strength::Discouraged, _, Some(s)) => format!(", which is discouraged with {s}{reason}"),
            (Strength::Discouraged, _, None) => format!(", which is discouraged{reason}"),
            (Strength::Reference, Some(m), _) => format!("; this differs from the {m} reference pipeline, which {uses}"),
            (Strength::Reference, None, Some(s)) => format!("; this differs from the reference pipeline for {s}, which {uses}"),
            (Strength::Reference, None, None) if why => format!("; the reference pipeline {uses}{reason}"),
            (Strength::Reference, None, None) => format!("; this differs from the reference pipeline, which {uses}"),
            (_, Some(m), _) => format!("; {m} {verb} {rec}{reason}"),
            (_, None, Some(s)) => format!("; {rec} is {needed} with {s}{reason}"),
            (_, None, None) => format!("; {rec} is {needed}{reason}"),
        }
    }

    /// One line for a terminal: "Background removal is V-SHARP; MEDI recommends PDF: why".
    /// The rule's source is for the run log only ([`Self::log_line`]; decided 2026-11-12).
    pub fn line(&self) -> String {
        format!("{} is {}{}", self.label, self.current_text(), self.departure(true))
    }

    /// [`Self::line`] for the run log, with where the rule comes from: "... : why [source]".
    pub fn log_line(&self) -> String {
        format!("{} [{}]", self.line(), self.source)
    }

    /// The methods text's Note line for this departure, without its full stop: what was done, and
    /// what the method's reference uses ("Background removal used LBV; this differs from the RTS
    /// reference pipeline, which uses V-SHARP"). The source, and the reason unless the value is
    /// discouraged, are for the log and the TUI, not a methods section.
    pub fn note(&self) -> String {
        // The mask recipe's parts are named as what it used ("no erosion", "majority filter").
        let used = self.setting == "masking.sections" || self.current.iter().all(|n| n.value.is_string());
        let verb = if used { "used" } else { "was" };
        format!("{} {verb} {}{}", self.label, self.current_text(), self.departure(false))
    }
}

/// The settings whose values are methods (a rule triggered by one is that method's).
const METHOD_SETTINGS: [&str; 5] = ["inversion.algorithm", "bg_removal.algorithm", "masking.sections", "separation.algorithm", "field_mapping.b0_estimation"];

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
    /// Whether the scan has a measured R2′ for χ-separation (a MESE acquisition, or a supplied
    /// map): QSMxT's own `qsmxt.chi-sep-predict-r2prime` fires only when it is known to have none.
    pub measured_r2prime: Option<bool>,
    /// The readout polarity of the echoes, when the sidecar says it: a bipolar readout makes bipolar
    /// correction a required change (`qsmxt.bipolar-readout`, [`crate::normalise`]).
    pub readout_polarity: Option<crate::scan::ReadoutPolarity>,
    /// Whether the echoes' phase shows a bipolar readout's odd/even pattern, when a front end that
    /// holds the data measured it (`qsm_core::utils::bipolar_signature`): with the polarity not
    /// known, bipolar correction is then suggested (`qsmxt.bipolar-signature`).
    pub bipolar_signature: Option<bool>,
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
            measured_r2prime: Some(f.mese_echo_times.is_some() || f.custom_r2prime),
            readout_polarity: f.readout_polarity,
            bipolar_signature: None,
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
            measured_r2prime: None,
            readout_polarity: None,
            bipolar_signature: None,
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
            measured_r2prime: v.measured_r2prime,
            readout_polarity: v.readout_polarity,
            bipolar_signature: None,
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
    // A config that follows QSMxT's defaults exactly made QSMxT's choices, not departures from a
    // toolbox the user picked: no differences from reference pipelines are recorded for it (Ashley,
    // 2026-11-10). Against a chosen toolbox pipeline, or once the defaults are departed from, they
    // are.
    let defaults = follows_defaults(config);
    recommendations(config, facts).into_iter().filter(|r| !(defaults && r.strength == Strength::Reference)).map(|r| {
        let outcome = if r.dismissed { Outcome::Dismissed } else { Outcome::Kept };
        Recorded { finding: r, outcome }
    }).collect()
}

/// Whether `config` is the QSMxT-defaults pipeline as it stands: every reconstruction setting at
/// its default (the same whatever the scan, so judged on the config alone), and no toolbox
/// pipeline remembered. Combining orientations (COSMOS, STI) is not part of it: it combines the
/// reconstructions the reference rules are about, and changes none of their settings.
fn follows_defaults(config: &PipelineConfig) -> bool {
    config.reference_pipeline.is_none()
        && crate::pipelines::target(config, crate::pipelines::QSMXT_DEFAULTS, &crate::schema::ViewFacts::default())
            .is_ok_and(|mut t| {
                t.multi_orientation = config.multi_orientation.clone();
                serde_json::to_value(&t).ok() == serde_json::to_value(config).ok()
            })
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
        required: vec![],
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
