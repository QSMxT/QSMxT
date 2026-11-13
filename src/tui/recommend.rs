//! The TUI's side of the pipeline rules' findings ([`qsmxt_engine::recommendations`]): the pop-up a
//! change opens when it creates a recommendation, the pop-up choosing a method opens when the
//! config differs from that method's reference pipeline, the marker on a setting that departs
//! from a rule, its details pop-up, and the reference values in the help line. The findings are
//! the engine's, for the config the form builds ([`super::command::config_from_app`]); the TUI
//! holds no rules.
//!
//! - **One design for every advice pop-up** ([`AdviceModal`]): the changes on offer as a
//!   checklist under the sections they are on, each "setting  current → new  (kind)", with the
//!   highlighted row's reason in a panel under a rule. **Apply** applies the ticked rows (nothing
//!   while none is), **Keep mine** keeps every setting as it is (a `recommended`, `suggested` or
//!   `discouraged` finding is dismissed for its current value; a `reference` one records nothing;
//!   a `required` one cannot be kept), and Esc closes the pop-up, leaving the findings open.
//! - **What starts ticked** follows the kind: a recommendation, or anything required or
//!   discouraged, is ticked; a suggestion or an original authors' setting (a `reference` rule:
//!   what the method's authors did, with no reason given that it is better) is not; and nothing
//!   is when equally strong advice on a setting disagrees. The values of one setting exclude each
//!   other. An unticked required change says the run will refuse to start.
//! - **The pop-ups**: a change that creates a reasoned finding opens [`AdviceModal::new`]; `!` on
//!   a marked setting opens [`AdviceModal::details`] (every rule it departs from); choosing a
//!   method with a reference pipeline ([`rec::chosen_methods`]) whose settings differ
//!   ([`rec::reference_differences`]) opens [`AdviceModal::reference`], led by the
//!   recommendations the same change created. What is applied together follows [`rec::settle`],
//!   as on every host: a reasoned finding outranks an authors' setting, and two authors' settings
//!   that disagree leave the setting as it is, marked, to choose with `!`.
//! - **The setting just changed is respected** ([`AdviceModal::changed`]): when a change makes a
//!   setting depart from a rule scoped to a flag that is on ([`Recommendation::flag_trigger`]),
//!   such as choosing QSMART with HEIDI ticked, turning that flag off starts ticked, and changing
//!   the setting back is offered unticked. It comes before any reference pipeline pop-up.
//! - **Pickers** ([`picker_noun`]): every selector is chosen from a picker (Enter), never
//!   cycled, so no method is chosen on the way to another.
//! - **The marker** ([`rec::marker`]) names whose rule a setting departs from and what it uses:
//!   ⚑ for a finding offered as a change, ⓘ for information only. `!` on a marked setting opens
//!   its details.
//! - **Apply** sets the form so the config it builds takes the value: each value of the setting's
//!   row is tried until the engine no longer finds the departure.

use crossterm::event::{KeyCode, KeyEvent};
pub use qsmxt_engine::recommendations::{Kind, Strength};
use qsmxt_engine::recommendations::{self as rec, Facts, Marker, Recommendation, Trigger};
use serde_json::Value;

use super::app::{App, PipelineRow};

/// What ticking a row of an advice pop-up does.
#[derive(Debug, Clone, PartialEq)]
pub enum RowAction {
    /// Set the finding's setting to `value` (the rule's recommended one when `None`), which reads
    /// as `name`.
    Apply { value: Option<Value>, name: String },
    /// Turn off the flag the finding is scoped to (`setting`, its config path) instead, keeping
    /// the setting as it is.
    TurnOff { setting: String },
}

/// One row of an advice pop-up: a setting as it is and what ticking the row changes it to.
#[derive(Debug, Clone, PartialEq)]
pub struct AdviceRow {
    pub finding: Recommendation,
    /// The other methods' rules that want the same value of the same setting: decided with it.
    pub also: Vec<Recommendation>,
    pub action: RowAction,
    pub checked: bool,
    /// A value the rule allows besides the one it gives.
    pub alternative: bool,
    /// Rows of one group exclude each other: the values of one part of a setting, and turning
    /// off the flag that part depends on instead.
    group: String,
}

impl AdviceRow {
    fn apply(finding: Recommendation, value: Option<Value>, name: String, alternative: bool) -> Self {
        let group = rec::rule_setting(&finding).to_string();
        AdviceRow { finding, also: vec![], action: RowAction::Apply { value, name }, checked: false, alternative, group }
    }

    /// The setting the row changes, and its label: the flag's for turning it off.
    pub fn setting(&self) -> (&str, String) {
        match &self.action {
            RowAction::TurnOff { setting } => (setting, capitalised(self.finding.flag_name())),
            RowAction::Apply { .. } => (&self.finding.setting, part_label(&self.finding)),
        }
    }

    /// The value as it is, and as ticking the row makes it.
    pub fn change(&self) -> (String, String) {
        match &self.action {
            RowAction::TurnOff { .. } => ("on".into(), "off".into()),
            RowAction::Apply { name, .. } => (self.finding.current_text(), name.clone()),
        }
    }

    /// The row's tag: what kind of advice it is.
    pub fn tag(&self) -> &'static str {
        if self.alternative {
            return "allowed";
        }
        self.finding.strength.label()
    }

    /// The methods whose rules the row is: "MEDI, PDF".
    pub fn methods(&self) -> String {
        std::iter::once(&self.finding).chain(&self.also).map(|r| r.trigger.name.as_str()).collect::<Vec<_>>().join(", ")
    }

    /// The plain explanation of the row, for the panel under the list. `group_unticked`: no row
    /// of its group is ticked, so a required change unticked leaves the run refused.
    pub fn explanation(&self, several: bool, group_unticked: bool) -> String {
        let r = &self.finding;
        let whose = if several || !self.also.is_empty() { format!(" ({})", self.methods()) } else { String::new() };
        let verb = match r.strength {
            Strength::Required => "requires",
            Strength::Discouraged | Strength::Recommended => "recommends",
            Strength::Suggested => "suggests",
            Strength::Reference => "uses",
        };
        let mut text = match (&self.action, r.strength) {
            (RowAction::TurnOff { .. }, _) => format!("Turn off {} and keep {} as {}. With it on, {} {verb} {}: {}",
                r.flag_name(), r.label, r.current_text(), r.trigger.name, r.recommended.name, sentence(&r.reason)),
            (RowAction::Apply { name, .. }, _) if self.alternative => format!("{} also allows {name}{whose}.", r.trigger.name),
            (_, Strength::Required) => format!("{} requires {}{whose}: {}", r.trigger.name, r.recommended.name, sentence(&r.reason)),
            (_, Strength::Discouraged) => format!("{} is discouraged with {}{whose}: {}", r.current_text(), r.trigger.name, sentence(&r.reason)),
            (_, Strength::Suggested) => format!("{} suggests {}{whose}: {}", r.trigger.name, r.recommended.name, sentence(&r.reason)),
            (_, Strength::Recommended) => format!("{} recommends {}{whose}: {}", r.trigger.name, r.recommended.name, sentence(&r.reason)),
            (_, Strength::Reference) => format!("{} uses {}{whose}: {}. The authors' setting, not a recommendation.",
                rec::reference_title(&r.trigger), r.recommended.name, r.reason.trim_end_matches('.')),
        };
        if r.strength == Strength::Required && group_unticked && matches!(self.action, RowAction::Apply { .. }) && !self.alternative {
            text += &format!(" Unticked, the run will refuse to start: {} cannot run with {}.", r.trigger.name, r.current_text());
        }
        if r.dismissed {
            text += &format!(" You kept {} before.", r.current_text());
        }
        text
    }
}

/// A reason as a sentence: with its full stop.
fn sentence(s: &str) -> String {
    let s = s.trim();
    if s.ends_with(['.', '!', '?']) { s.to_string() } else { format!("{s}.") }
}

/// A row's setting as the checklist names it: the parts of the mask recipe (one QSMxT setting,
/// "Mask") by the part.
fn part_label(r: &Recommendation) -> String {
    match rec::rule_setting(r) {
        "masking.input" => "Mask input",
        "masking.threshold_method" => "Mask threshold",
        "masking.largest_component" => "Mask components",
        "masking.imfill" => "Mask holes",
        "masking.erosion" => "Mask erosion",
        "masking.min_intensity" => "Mask intensity threshold",
        "masking.majority" => "Mask majority filter",
        "masking.close_radius" => "Mask closing",
        "masking.bet.fractional_intensity" => "BET fractional intensity",
        "masking.bet.orientation" => "BET orientation",
        _ => return r.label.clone(),
    }.to_string()
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Which pop-up an advice pop-up is.
#[derive(Debug, Clone, PartialEq)]
pub enum AdviceKind {
    /// A change created a reasoned finding ([`AdviceModal::new`]).
    Change,
    /// The finding is on the setting just changed, against a flag that is on
    /// ([`AdviceModal::changed`]).
    Changed,
    /// The details of a marked setting, opened with `!` ([`AdviceModal::details`]).
    Details,
    /// The differences from the reference pipelines of the methods a change chose
    /// ([`AdviceModal::reference`]).
    Reference(Vec<Trigger>),
}

/// An advice pop-up: the changes on offer as a checklist under the sections they are on, each
/// "setting: current → new" with its kind, and the highlighted one's reason in a panel under it.
/// One Apply applies the ticked rows; Keep mine keeps every setting as it is. What starts ticked
/// follows the kind of advice: a recommendation (or anything required or discouraged) is
/// ticked, an authors' setting or a suggestion is not, and nothing is when equal advice
/// disagrees. Turning off the flag a change went against starts ticked when that change was
/// just made, so the change itself is kept.
#[derive(Debug, Clone)]
pub struct AdviceModal {
    pub kind: AdviceKind,
    /// The findings it is about, in order.
    pub findings: Vec<Recommendation>,
    pub rows: Vec<AdviceRow>,
    /// The highlighted row.
    pub cursor: usize,
    /// How far the list is scrolled; drawing keeps the highlighted row in view.
    pub scroll: u16,
    /// The button that has the focus instead of a row: 0 Apply, 1 Keep mine ([`Self::buttons`]).
    pub button: Option<usize>,
}

/// The pop-up a change, or `!`, opens ([`AdviceModal::new`], [`AdviceModal::changed`],
/// [`AdviceModal::details`]).
pub type RecModal = AdviceModal;
/// The reference pipeline pop-up ([`AdviceModal::reference`]).
pub type RefModal = AdviceModal;

impl AdviceModal {
    /// Rows for `findings`: each value once (the methods that agree on it together), with the
    /// values each rule also allows when `alternatives`.
    fn build(kind: AdviceKind, findings: Vec<Recommendation>, alternatives: bool) -> Self {
        let mut rows: Vec<AdviceRow> = Vec::new();
        for r in &findings {
            let same = |row: &&mut AdviceRow| !row.alternative && row.group == rec::rule_setting(r)
                && matches!(&row.action, RowAction::Apply { value: None, .. }) && row.finding.recommended.value == r.recommended.value;
            match rows.iter_mut().find(|row| same(row)) {
                Some(row) => row.also.push(r.clone()),
                None => rows.push(AdviceRow::apply(r.clone(), None, r.recommended.name.clone(), false)),
            }
        }
        if alternatives {
            let primaries: Vec<Recommendation> = rows.iter().map(|r| r.finding.clone()).collect();
            for r in &primaries {
                for a in r.alternatives() {
                    let taken = rows.iter().any(|row| row.group == rec::rule_setting(r)
                        && matches!(&row.action, RowAction::Apply { value, .. } if value.as_ref().unwrap_or(&row.finding.recommended.value) == &a.value));
                    if !taken {
                        rows.push(AdviceRow::apply(r.clone(), Some(a.value.clone()), a.name.clone(), true));
                    }
                }
            }
        }
        // In each group, the strongest row starts ticked when its kind is preselected and no
        // other row as strong disagrees with it (a recommendation outranks an authors' setting).
        let mut groups: Vec<String> = rows.iter().map(|r| r.group.clone()).collect();
        groups.dedup();
        for g in &groups {
            let here: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.group == *g && !r.alternative).map(|(i, _)| i).collect();
            let Some(&best) = here.iter().max_by_key(|&&i| (rows[i].finding.kind == Kind::Reasoned, rows[i].finding.strength)) else { continue };
            let r = &rows[best].finding;
            let disagree = here.iter().any(|&i| i != best && rows[i].finding.strength == r.strength && rows[i].finding.kind == r.kind);
            rows[best].checked = !disagree && !r.conflict && !r.dismissed && r.kind == Kind::Reasoned && r.strength.preselected();
        }
        AdviceModal { kind, findings, rows, cursor: 0, scroll: 0, button: None }
    }

    pub fn new(findings: Vec<Recommendation>) -> Self {
        Self::build(AdviceKind::Change, findings, true)
    }

    /// `findings` are on the setting just changed, and scoped to a flag that is on: turning the
    /// flag off is offered, ticked, so the change itself stands. Changing the setting back (or to
    /// another value the rule allows) is not offered: it would undo the change just made, and
    /// both resolve the one conflict, so only one of them belongs in the pop-up.
    pub fn changed(findings: Vec<Recommendation>) -> Self {
        let mut m = Self::build(AdviceKind::Changed, findings, true);
        let mut off = turn_off_rows(&m.findings);
        m.rows.retain(|row| !off.iter().any(|o| o.group == row.group));
        for o in &mut off {
            o.checked = true;
        }
        m.rows.splice(0..0, off);
        m
    }

    /// The details of a setting: `findings` are the rules it departs from, offered ones first.
    pub fn details(findings: Vec<Recommendation>) -> Self {
        let mut m = Self::build(AdviceKind::Details, findings, true);
        m.rows.extend(turn_off_rows(&m.findings));
        m
    }

    /// Where the config differs from the reference pipelines of the methods a change chose,
    /// led by the recommendations the same change created.
    pub fn reference(methods: Vec<Trigger>, findings: Vec<Recommendation>) -> Self {
        Self::build(AdviceKind::Reference(methods), findings, false)
    }

    /// Several methods' advice: each row says whose it is.
    pub fn several(&self) -> bool {
        let mut methods: Vec<&str> = Vec::new();
        for f in &self.findings {
            if !methods.contains(&f.trigger.name.as_str()) {
                methods.push(&f.trigger.name);
            }
        }
        methods.len() > 1
    }

    /// The methods whose reference pipelines a reference pop-up is about.
    pub fn methods(&self) -> &[Trigger] {
        match &self.kind {
            AdviceKind::Reference(methods) => methods,
            _ => &[],
        }
    }

    /// Whether Keep mine is offered: not when every finding is required.
    pub fn keepable(&self) -> bool {
        self.findings.iter().any(|r| r.strength != Strength::Required)
    }

    /// The ticked rows.
    pub fn ticked(&self) -> usize {
        self.rows.iter().filter(|r| r.checked).count()
    }

    /// Row `i` is one of several ways to resolve one setting (the values of one setting, or
    /// turning off the flag it depends on instead): drawn as a radio button, as one choice.
    pub fn radio(&self, i: usize) -> bool {
        self.rows.get(i).is_some_and(|row| self.rows.iter().filter(|r| r.group == row.group).count() > 1)
    }

    /// Row `i` is an alternative to a required row before it in its radio group: tagged "or"
    /// rather than "required" a second time, as the group is one requirement.
    pub fn or_row(&self, i: usize) -> bool {
        let Some(row) = self.rows.get(i) else { return false };
        !row.alternative && row.finding.strength == Strength::Required
            && self.rows[..i].iter().any(|r| r.group == row.group && r.finding.strength == Strength::Required)
    }

    /// No row of row `i`'s group is ticked: a required one then leaves the run refused.
    fn group_unticked(&self, i: usize) -> bool {
        self.rows.get(i).is_some_and(|row| !self.rows.iter().any(|r| r.group == row.group && r.checked))
    }

    /// Row `i`'s explanation, for the panel under the list.
    pub fn explanation(&self, i: usize) -> String {
        self.rows.get(i).map_or_else(String::new, |r| r.explanation(self.several(), self.group_unticked(i)))
    }

    /// Row `i`'s tag: its kind, or "or" for a second way to meet one requirement.
    pub fn tag(&self, i: usize) -> &'static str {
        match self.rows.get(i) {
            Some(_) if self.or_row(i) => "or",
            Some(r) => r.tag(),
            None => "",
        }
    }

    /// The buttons under the list: Apply, and Keep mine when it is offered.
    pub fn buttons(&self) -> usize {
        if self.keepable() { 2 } else { 1 }
    }

    /// Tick or untick row `i`: ticking it unticks the others of its group. A required radio
    /// group keeps one row ticked: choosing a way to meet the requirement is choosing another
    /// row of it.
    pub fn toggle(&mut self, i: usize) {
        let Some(row) = self.rows.get(i) else { return };
        let (group, on) = (row.group.clone(), !row.checked);
        if !on && row.finding.strength == Strength::Required && self.radio(i) {
            return;
        }
        if on {
            for r in self.rows.iter_mut().filter(|r| r.group == group) {
                r.checked = false;
            }
        }
        self.rows[i].checked = on;
    }

    pub fn title(&self) -> String {
        let r = &self.findings[0];
        let several = self.several();
        match &self.kind {
            AdviceKind::Reference(_) => format!(" {} ", reference_whose(self.methods())),
            _ if r.strength == Strength::Reference && !several => format!(" {}: {} ", Kind::Reference.label(), rec::reference_title(&r.trigger)),
            _ if r.strength == Strength::Reference => format!(" {}s: {} ", Kind::Reference.label(), r.label),
            AdviceKind::Details if self.findings.len() > 1 => format!(" {} ", r.label),
            AdviceKind::Changed if r.strength == Strength::Required => format!(" {} requires a change ", r.current_text()),
            _ => match r.strength {
                Strength::Required => format!(" {} requires a change ", r.trigger.name),
                Strength::Discouraged => format!(" Discouraged with {} ", r.trigger.name),
                Strength::Suggested if self.findings.len() == 1 => format!(" {} suggests a change ", r.trigger.name),
                _ if self.findings.len() > 1 => " Recommendations disagree ".to_string(),
                _ => format!(" {} recommends a change ", r.trigger.name),
            },
        }
    }
}

/// "MEDI's reference pipeline", or "MEDI's and PDF's reference pipelines".
fn reference_whose(methods: &[Trigger]) -> String {
    match methods {
        [one] => rec::reference_title(one),
        many => format!("{}'s reference pipelines", many.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join("'s and ")),
    }
}

/// "Turn off HEIDI streaking reduction" rows, once for each flag `findings` are scoped to that
/// has a row in the form, each in the group of the setting its finding is on.
fn turn_off_rows(findings: &[Recommendation]) -> Vec<AdviceRow> {
    let mut out: Vec<AdviceRow> = Vec::new();
    for r in findings {
        let Some(setting) = r.flag_trigger() else { continue };
        if super::app::field_of_path(setting).is_none() || out.iter().any(|o| o.action == RowAction::TurnOff { setting: setting.to_string() }) {
            continue;
        }
        out.push(AdviceRow {
            finding: r.clone(), also: vec![], action: RowAction::TurnOff { setting: setting.to_string() },
            checked: false, alternative: false, group: rec::rule_setting(r).to_string(),
        });
    }
    out
}

/// One setting as the reference pipeline pop-up decides it, for [`App::decide`].
#[derive(Debug, Clone, PartialEq)]
struct RefItem {
    finding: Recommendation,
    also: Vec<Recommendation>,
    /// The value to apply and its name, or `None` to keep the current one.
    pick: Option<(Option<Value>, String)>,
}

impl RefItem {
    /// The item's findings as picked, each wanting the picked value: none when keeping the
    /// current one.
    fn picked(&self) -> Vec<Recommendation> {
        let Some((value, _)) = &self.pick else { return vec![] };
        std::iter::once(&self.finding).chain(&self.also).cloned().map(|mut r| {
            if let Some(v) = value {
                r.recommended.value = v.clone();
            }
            r
        }).collect()
    }
}

/// What the reference pipeline pop-ups are worked out from.
#[derive(Debug, Clone, Default)]
pub struct RefState {
    pub modal: Option<RefModal>,
    /// The choices with a reference pipeline at the last check ([`rec::chosen_methods`]).
    pub methods: Vec<Trigger>,
    /// Methods just chosen whose pop-up has not been shown, each with the settings (config
    /// paths) or rules (ids) it leaves out: those the pop-up that chose it had just decided.
    pub pending: Vec<(Trigger, Vec<String>)>,
}

/// A finding offered as a change and not dismissed: what can open the pop-up or show a badge.
fn open(r: &Recommendation) -> bool {
    r.actionable() && !r.dismissed
}

impl App {
    /// What the rules know about the data from the form: the echo count it is limited to, the
    /// field strength (typed for NIfTI input; the first selected run's, once the Pipeline view has
    /// read it, for a BIDS dataset) and the manufacturer (the first phase image's sidecar; the
    /// first selected run's). A rule on an unknown fact stays silent.
    pub(super) fn rule_facts(&self) -> Facts {
        let (field_strength, manufacturer) = match self.input_mode {
            super::app::InputMode::NIfTI => (
                self.nifti_state.field_strength.trim().parse().ok(),
                self.nifti_state.phase_files.first()
                    .and_then(|p| super::pipeline_view::sidecar_manufacturer(&crate::dicom::convert::nii_to_json_path(p))),
            ),
            super::app::InputMode::Bids => (self.pipeline_view.read_field_strength(), self.pipeline_view.read_manufacturer()),
            _ => (None, None),
        };
        // A measured R2′ only from a BIDS run's MESE (a supplied map is a setting, which the rule
        // reads itself); unknown otherwise, so QSMxT's offer to predict R2′ waits for the scan.
        let measured_r2prime = match self.input_mode {
            super::app::InputMode::Bids => self.pipeline_view.read_has_mese(),
            _ => None,
        };
        Facts { echo_count: self.filter_state.num_echoes.trim().parse().ok(), field_strength, manufacturer,
                measured_r2prime, ..Default::default() }
    }

    /// Every rule that applies to the form's config, departed from or not.
    pub fn rule_view(&self) -> Vec<Recommendation> {
        super::command::config_from_app(self)
            .map(|c| rec::reference_values(&c, &self.rule_facts()))
            .unwrap_or_default()
    }

    /// What the findings are worked out from: the form, the echo limit and the field strength.
    pub(super) fn rec_fingerprint(&self) -> String {
        format!("{:?}{:?}{}{:?}", self.form, self.pipeline_state, self.filter_state.num_echoes, self.rule_facts().field_strength)
    }

    /// After a change: drop the dismissals whose setting has changed, open the pop-up for the
    /// findings the change created, and note the methods it chose. A chosen method's reference
    /// pipeline pop-up opens once nothing else is open. Nothing
    /// while a value is being typed.
    pub fn check_recommendations(&mut self) {
        self.check_recommendations_with(true);
    }

    /// After a pipeline is applied: the reasoned recommendations its choices trigger (PDF's
    /// iteration count) open their pop-up as after any change, but no reference pipeline pop-up
    /// does: the pipeline is the authors' settings.
    pub(super) fn check_recommendations_after_pipeline(&mut self) {
        self.rec_queue.clear();
        self.check_recommendations_with(false);
    }

    fn check_recommendations_with(&mut self, offer_reference: bool) {
        if self.is_typing() || self.rec_modal.is_some() || self.refs.modal.is_some() {
            return;
        }
        let form = self.rec_fingerprint();
        // The next finding the last change created, once the pop-up before it is closed.
        if form == self.rec_form && self.refs.pending.is_empty() && !self.rec_queue.is_empty() {
            let view = self.rule_view();
            while !self.rec_queue.is_empty() {
                let rule = self.rec_queue.remove(0);
                if let Some(r) = view.iter().find(|r| r.rule == rule && open(r)) {
                    self.rec_modal = Some(RecModal::new(group(&view, r)));
                    return;
                }
            }
        }
        let mut created: Vec<Recommendation> = Vec::new();
        let mut view: Option<Vec<Recommendation>> = None;
        let mut just_changed: Vec<String> = Vec::new();
        if form != self.rec_form {
            // The first check takes the form as it opened: only a change opens a pop-up.
            let opening = self.rec_form.is_empty();
            self.rec_form = form;
            if let Ok(mut c) = super::command::config_from_app(self) {
                let before = c.dismissed_recommendations.len();
                rec::prune_dismissals(&mut c, &self.rule_facts());
                if c.dismissed_recommendations.len() != before {
                    self.pipeline_state.dismissed = c.dismissed_recommendations;
                }
            }
            let config = super::command::config_from_app(self).ok().and_then(|c| serde_json::to_value(c).ok());
            let mut changed = Vec::new();
            if let (Some(before), Some(after)) = (&self.rec_config, &config) {
                changed_settings(before, after, String::new(), &mut changed);
            }
            self.rec_config = config;
            let v = self.rule_view();
            let now: Vec<(String, String)> =
                v.iter().filter(|r| open(r)).map(|r| (r.rule.clone(), rec::chosen_text(&r.current))).collect();
            created = v.iter().filter(|r| {
                open(r) && !self.rec_seen.contains(&(r.rule.clone(), rec::chosen_text(&r.current)))
            }).cloned().collect();
            self.rec_seen = now;
            just_changed = changed.into_iter().map(|(path, _)| path).collect();
            let chosen = self.note_methods(&v, &[]);
            if opening {
                self.refs.pending.clear();
                return;
            }
            // A method no longer chosen is not offered.
            self.refs.pending.retain(|(m, _)| chosen.contains(m) && offer_reference);
            view = Some(v);
        }
        let view = view.unwrap_or_else(|| if self.refs.pending.is_empty() { vec![] } else { self.rule_view() });
        // Every pending method's differences from its reference: one pop-up for one change.
        let mut reference: Option<(Vec<Trigger>, Vec<Recommendation>)> = None;
        if !self.refs.pending.is_empty() {
            let groups = rec::by_method(&view);
            let mut methods = Vec::new();
            let mut diffs: Vec<Recommendation> = Vec::new();
            for (method, skip) in std::mem::take(&mut self.refs.pending) {
                let here: Vec<Recommendation> = groups.iter().find(|m| m.method == method)
                    .map(|m| m.differences()).unwrap_or_default()
                    .into_iter().filter(|r| !skip.contains(&r.setting) && !skip.contains(&r.rule)).collect();
                if !here.is_empty() {
                    methods.push(method);
                    diffs.extend(here);
                }
            }
            if !diffs.is_empty() {
                reference = Some((methods, diffs));
            }
        }
        let Some(first) = created.first() else {
            if let Some((methods, diffs)) = reference {
                self.refs.modal = Some(AdviceModal::reference(methods, diffs));
            }
            return;
        };
        // A setting just changed, against a flag that is on and was not: offer the flag first.
        let touched = |path: &str| just_changed.iter().any(|c| c == path || c.starts_with(&format!("{path}.")));
        let against_flag = created.iter().find(|r| {
            r.flag_trigger().is_some_and(|flag| !touched(flag) && super::app::field_of_path(flag).is_some()) && touched(&r.setting)
        });
        if let Some(r) = against_flag {
            let group = group(&view, r);
            self.rec_queue = created.iter().filter(|c| !group.iter().any(|g| g.setting == c.setting))
                .map(|c| c.rule.clone()).collect();
            if let Some((methods, _)) = reference {
                self.refs.pending = methods.into_iter().map(|m| (m, vec![])).collect();
            }
            self.rec_modal = Some(RecModal::changed(group));
            return;
        }
        let group = group(&view, first);
        self.rec_queue.clear();
        match reference {
            // One pop-up: the recommendations first, then the reference differences.
            Some((methods, diffs)) if created.iter().all(|r| r.strength != Strength::Required && !r.conflict) => {
                let mut items = created.clone();
                items.extend(diffs.into_iter().filter(|d| !created.iter().any(|c| c.rule == d.rule)));
                self.refs.modal = Some(AdviceModal::reference(methods, items));
            }
            // A required change or a disagreement first; the reference pipeline after it.
            Some((methods, _)) => {
                self.refs.pending = methods.into_iter().map(|m| (m, vec![])).collect();
                self.rec_modal = Some(RecModal::new(group));
            }
            None => {
                self.rec_queue = created.iter().filter(|r| !group.iter().any(|g| g.setting == r.setting))
                    .map(|r| r.rule.clone()).collect();
                self.rec_modal = Some(RecModal::new(group));
            }
        }
    }

    /// Note the methods `view` chooses: those not chosen before are queued for their reference
    /// pipeline pop-up, leaving out the settings in `skip`. Returns the methods chosen.
    fn note_methods(&mut self, view: &[Recommendation], skip: &[String]) -> Vec<Trigger> {
        let chosen: Vec<Trigger> = rec::by_method(view).into_iter().map(|m| m.method).collect();
        for m in chosen.iter().filter(|m| !self.refs.methods.contains(m)) {
            if !self.refs.pending.iter().any(|(p, _)| p == m) {
                self.refs.pending.push((m.clone(), skip.to_vec()));
            }
        }
        self.refs.methods = chosen.clone();
        chosen
    }

    /// Open the details of the focused row's setting (`!`): every rule it departs from, offered
    /// ones first.
    pub fn review_recommendations(&mut self) {
        let Some(path) = self.focused_row().and_then(|r| row_path(&r)) else { return };
        let mut here: Vec<Recommendation> = self.rule_view().into_iter().filter(|r| r.setting == path && r.departs).collect();
        if here.is_empty() {
            return;
        }
        here.sort_by_key(|r| (!open(r), std::cmp::Reverse(r.strength)));
        self.rec_modal = Some(RecModal::details(here));
    }

    pub fn handle_rec_modal_key(&mut self, key: KeyEvent) {
        self.handle_advice_key(key, false);
    }

    /// An advice pop-up's keys: ↑/↓ move between the rows and on to the buttons under them (Tab
    /// goes to the buttons, ←/→ between them), Space or Enter ticks a row, Enter on a button
    /// presses it, `A` applies the ticked rows (nothing while none is), `K` keeps every setting
    /// as it is (closes the pop-up, changing nothing, where Keep mine is not offered), and Esc
    /// closes it, changing nothing. Letters are read either case.
    fn handle_advice_key(&mut self, key: KeyEvent, reference: bool) {
        enum Act { Apply, Keep, Close }
        let slot = if reference { &mut self.refs.modal } else { &mut self.rec_modal };
        let Some(modal) = slot.as_mut() else { return };
        let n = modal.rows.len().max(1);
        let buttons = modal.buttons();
        let code = match key.code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        };
        let mut act = None;
        match (code, modal.button) {
            (KeyCode::Up, None) if modal.cursor == 0 => modal.button = Some(buttons - 1),
            (KeyCode::Up, None) => modal.cursor -= 1,
            (KeyCode::Up, Some(_)) => {
                modal.button = None;
                modal.cursor = n - 1;
            }
            (KeyCode::Down, None) if modal.cursor + 1 >= n => modal.button = Some(0),
            (KeyCode::Down, None) => modal.cursor += 1,
            (KeyCode::Down, Some(_)) => {
                modal.button = None;
                modal.cursor = 0;
            }
            (KeyCode::Tab, None) => modal.button = Some(0),
            (KeyCode::Tab, Some(b)) => modal.button = (b + 1 < buttons).then_some(b + 1),
            (KeyCode::BackTab, None) => modal.button = Some(buttons - 1),
            (KeyCode::BackTab, Some(b)) => modal.button = b.checked_sub(1),
            (KeyCode::Left, Some(b)) => modal.button = Some(b.saturating_sub(1)),
            (KeyCode::Right, Some(b)) => modal.button = Some((b + 1).min(buttons - 1)),
            (KeyCode::PageUp, _) => {
                modal.button = None;
                modal.cursor = modal.cursor.saturating_sub(10);
            }
            (KeyCode::PageDown, _) => {
                modal.button = None;
                modal.cursor = (modal.cursor + 10).min(n - 1);
            }
            (KeyCode::Home, _) => {
                modal.button = None;
                modal.cursor = 0;
            }
            (KeyCode::End, _) => {
                modal.button = None;
                modal.cursor = n - 1;
            }
            (KeyCode::Char(' ') | KeyCode::Char('x') | KeyCode::Enter, None) => {
                let c = modal.cursor;
                modal.toggle(c);
            }
            (KeyCode::Enter, Some(0)) | (KeyCode::Char('a'), _) => act = Some(Act::Apply),
            (KeyCode::Enter, Some(_)) => act = Some(Act::Keep),
            (KeyCode::Char('k'), _) => act = Some(if modal.keepable() { Act::Keep } else { Act::Close }),
            (KeyCode::Esc, _) => act = Some(Act::Close),
            _ => {}
        }
        match act {
            Some(Act::Apply) if modal.ticked() == 0 => {
                self.status_message = Some("Nothing ticked: Space or Enter ticks a row".to_string());
            }
            Some(Act::Apply) => {
                let modal = slot.take().expect("open");
                self.settle_advice(&modal, false);
            }
            Some(Act::Keep) => {
                let modal = slot.take().expect("open");
                self.settle_advice(&modal, true);
            }
            Some(Act::Close) => *slot = None,
            None => {}
        }
    }

    /// Carry out an advice pop-up: the ticked rows applied, flags ticked off turned off, and
    /// every other finding kept as it is ([`Self::decide`]); with `keep_all`, every one kept.
    fn settle_advice(&mut self, modal: &AdviceModal, keep_all: bool) {
        let off: Vec<&AdviceRow> = if keep_all { vec![] } else {
            modal.rows.iter().filter(|r| r.checked && matches!(r.action, RowAction::TurnOff { .. })).collect()
        };
        for row in &off {
            if let RowAction::TurnOff { setting } = &row.action {
                if let Some(field) = super::app::field_of_path(setting) {
                    self.set_companion(field, &Value::Bool(false));
                }
            }
        }
        let mut items: Vec<RefItem> = Vec::new();
        for row in modal.rows.iter().filter(|r| !r.alternative && matches!(r.action, RowAction::Apply { .. })) {
            let rule = &row.finding.rule;
            let chosen = if keep_all { None } else {
                modal.rows.iter().find(|x| x.finding.rule == *rule && x.checked && matches!(x.action, RowAction::Apply { .. }))
            };
            if chosen.is_none() && off.iter().any(|o| o.finding.rule == *rule) {
                continue;
            }
            let pick = chosen.and_then(|c| match &c.action {
                RowAction::Apply { value, name } => Some((value.clone(), name.clone())),
                RowAction::TurnOff { .. } => None,
            });
            items.push(RefItem { finding: row.finding.clone(), also: row.also.clone(), pick });
        }
        self.decide(&items, matches!(modal.kind, AdviceKind::Reference(_)));
        if !off.is_empty() {
            let names: Vec<&str> = off.iter().map(|r| r.finding.flag_name()).collect();
            let rest = self.status_message.take().filter(|s| s != "Kept your settings" && !items.is_empty());
            self.status_message = Some(match rest {
                Some(rest) => format!("Turned off {}; {rest}", names.join(", ")),
                None => format!("Turned off {}", names.join(", ")),
            });
        }
    }

    /// Set back each of `kept` (the mask recipe's parts) that the values just applied changed,
    /// to the value it had when it was kept.
    fn restore_kept(&mut self, kept: &[Recommendation]) {
        for k in kept.iter().filter(|k| k.setting == "masking.sections") {
            let Some(was) = k.current.first().map(|n| n.value.clone()) else { continue };
            if k.current.iter().any(|n| n.value != was) {
                continue;
            }
            let now = self.rule_view().into_iter().find(|r| r.rule == k.rule);
            if now.is_none_or(|r| r.current.iter().all(|n| n.value == was)) {
                continue;
            }
            let Ok(mut c) = super::command::config_from_app(self) else { continue };
            if rec::restore(&mut c, &k.rule, &was).is_ok() {
                self.pipeline_state.mask_preset = super::app::MASK_PRESET_CUSTOM;
                self.pipeline_state.mask_sections = c.masking.sections;
            }
        }
    }

    /// Keep the settings of `findings` as they are: each `recommended` or `discouraged` one
    /// dismissed for its current value, in the form and so in the run's config ([`rec::keep`]). A
    /// `reference` one records nothing, and a `required` one cannot be kept.
    fn keep_mine(&mut self, findings: &[Recommendation]) {
        rec::keep(&mut self.pipeline_state.dismissed, findings);
    }

    pub fn handle_ref_modal_key(&mut self, key: KeyEvent) {
        self.handle_advice_key(key, true);
    }

    /// Carry out the reference pipeline pop-up's choices: the picks [`rec::settle`] takes are
    /// applied, the items kept are kept, and a pick it leaves changes nothing and stays marked. A
    /// method an applied value chooses is offered without the settings just decided.
    /// With `offer_new` off (a recommendation's pop-up), a method's differences the applied
    /// values bring into play are not offered after it.
    fn decide(&mut self, items: &[RefItem], offer_new: bool) {
        // What departed before, each with its value: a follow-up offers only what the applied
        // values bring into play, never a row already kept (or applied) again.
        let before: Vec<(String, String)> = self.rule_view().iter().filter(|r| r.departs)
            .map(|r| (r.rule.clone(), rec::chosen_text(&r.current))).collect();
        let picked: Vec<Recommendation> = items.iter().flat_map(RefItem::picked).collect();
        let settled = rec::settle(&picked);
        let mut errors = Vec::new();
        let mut kept = Vec::new();
        let mut applied: Vec<&str> = Vec::new();
        let mut outranked = 0;
        let mut disagreeing = 0;
        let mut disagree_on: Vec<&str> = Vec::new();
        for item in items {
            let finding = &item.finding;
            match &item.pick {
                Some(_) if settled.conflicting.iter().any(|r| r.rule == finding.rule) => {
                    disagreeing += 1;
                    if !disagree_on.contains(&finding.label.as_str()) {
                        disagree_on.push(&finding.label);
                    }
                }
                Some(_) if !settled.apply.iter().any(|r| r.rule == finding.rule) => outranked += 1,
                // Agrees with one already applied on the same setting (by QSM.rs's path: the mask
                // recipe's generator, input and erosion are parts of one QSMxT setting).
                Some(_) if applied.contains(&rec::rule_setting(finding)) => {}
                Some((value, _)) => {
                    applied.push(rec::rule_setting(finding));
                    if let Err(e) = self.apply_recommendation(&finding.rule, value.as_ref()) {
                        errors.push(e);
                    }
                }
                None => {
                    kept.push(finding.clone());
                    kept.extend(item.also.iter().cloned());
                }
            }
        }
        // A value applied later can undo an earlier one on another part of the same setting (a
        // mask preset brings its erosion back): apply what still departs once more.
        let facts = self.rule_facts();
        for item in items {
            let (finding, Some((value, _))) = (&item.finding, &item.pick) else { continue };
            if !settled.apply.iter().any(|r| r.rule == finding.rule) {
                continue;
            }
            let still = super::command::config_from_app(self).ok()
                .map(|c| rec::reference_values(&c, &facts))
                .is_some_and(|v| v.iter().any(|x| x.rule == finding.rule && x.departs));
            if still {
                let _ = self.apply_recommendation(&finding.rule, value.as_ref());
            }
        }
        // A value applied to one part of the mask recipe can change a part kept (the generator
        // brings its preset's input and erosion): set the kept parts back.
        self.restore_kept(&kept);
        self.keep_mine(&kept);
        let kept_items = items.iter().filter(|i| i.pick.is_none()).count();
        let mut status = match (items.len() - kept_items - outranked - disagreeing, kept_items) {
            (0, _) if outranked + disagreeing == 0 => "Kept your settings".to_string(),
            (0, 0) => "Applied nothing".to_string(),
            (0, k) => format!("Kept {k}"),
            (n, 0) => format!("Applied {n} setting{}", if n == 1 { "" } else { "s" }),
            (n, k) => format!("Applied {n}, kept {k}"),
        };
        if outranked > 0 {
            status += &format!("; {outranked} not applied: a stronger suggestion wants another value");
        }
        if !disagree_on.is_empty() {
            status += &format!("; left {} as set: the suggestions disagree (! to choose)", disagree_on.join(", "));
        }
        self.status_message = Some(status);
        if !errors.is_empty() {
            self.error_message = Some(errors.join("; "));
        }
        let skip: Vec<String> = items.iter().map(|i| i.finding.setting.clone()).collect();
        let view = self.rule_view();
        self.note_methods(&view, &skip);
        // A method's difference that only comes into play with a value just applied (BET's
        // orientation, once the mask is BET; the Laplacian's solver, once it unwraps) is offered
        // next, as the pop-up did not show it: only those, by rule, so the rows kept here (or
        // in an earlier pop-up) are not offered again and applying reaches a fixed point.
        let presented: Vec<&str> = items.iter().map(|i| i.finding.rule.as_str()).collect();
        for m in rec::by_method(&view).into_iter().filter(|_| offer_new) {
            if !items.iter().any(|i| i.finding.trigger == m.method) || self.refs.pending.iter().any(|(p, _)| *p == m.method) {
                continue;
            }
            let differences = m.differences();
            let new = |r: &Recommendation| !presented.contains(&r.rule.as_str())
                && !before.contains(&(r.rule.clone(), rec::chosen_text(&r.current)));
            if differences.iter().any(new) {
                let skip = differences.iter().filter(|r| !new(r)).map(|r| r.rule.clone()).collect();
                self.refs.pending.push((m.method, skip));
            }
        }
        self.check_recommendations();
    }

    /// Set the form so its config resolves `rule`: to `value` (the rule's recommended one when
    /// `None`). Each value of the setting's row is tried until the engine no longer finds the
    /// departure and, for a chosen value, the setting has it.
    pub fn apply_recommendation(&mut self, rule: &str, value: Option<&Value>) -> Result<(), String> {
        let Some(r) = self.rule_view().into_iter().find(|r| r.rule == rule) else { return Ok(()) };
        // Each value is tried on this scan: a trial form has no scan of its own, and a rule on the
        // field strength (DECOMPOSE's at 7 T) would not apply to it at all.
        let facts = self.rule_facts();
        let resolved = |app: &App| {
            let view = super::command::config_from_app(app).map(|c| rec::reference_values(&c, &facts)).unwrap_or_default();
            view.iter().find(|x| x.rule == rule).is_none_or(|x| match value {
                None => !x.departs,
                Some(v) => x.current.iter().all(|n| n.value == *v),
            })
        };
        // A rule's value can fix more than its setting: "FFT, 12-voxel padding" is a solver and its
        // padding, which has a row of its own. Each candidate takes those too.
        let companions = self.rule_companions(rule, value, &r.setting);
        let clone_form = || {
            let mut app = self.clone_form();
            for (field, v) in &companions {
                app.set_companion(field, v);
            }
            app
        };
        let mut candidates: Vec<App> = Vec::new();
        for (_, _, rows) in self.all_pages() {
            for row in rows {
                let (field, path) = match &row {
                    PipelineRow::AlgoSelect { field, .. } | PipelineRow::Toggle { field, .. } | PipelineRow::Param { field, .. }
                        if *field == "mask_preset" => (*field, "masking.sections"),
                    PipelineRow::AlgoSelect { field, .. } | PipelineRow::Toggle { field, .. } | PipelineRow::Param { field, .. } => {
                        match super::app::field_path(field) { Some(p) => (*field, p), None => continue }
                    }
                    _ => continue,
                };
                if path != r.setting {
                    continue;
                }
                match &row {
                    PipelineRow::AlgoSelect { options, .. } => {
                        for i in 0..options.len() {
                            let mut app = clone_form();
                            // A value of this setting only: the inversion's LSQR entries also say
                            // whether HEIDI is on, and a rule on HEIDI's seed must leave it on.
                            let heidi = app.pipeline_state.heidi_enabled;
                            app.set_field_select(field, i);
                            app.pipeline_state.heidi_enabled = heidi;
                            candidates.push(app);
                        }
                    }
                    PipelineRow::Toggle { .. } => {
                        let mut app = clone_form();
                        app.toggle_field(field);
                        candidates.push(app);
                    }
                    _ => {
                        let target = value.unwrap_or(&r.recommended.value);
                        let texts = match target {
                            Value::Array(xs) => {
                                let parts: Vec<String> = xs.iter().map(|x| x.to_string()).collect();
                                vec![parts.join(" "), parts.join(",")]
                            }
                            Value::String(s) => vec![s.clone()],
                            other => vec![other.to_string()],
                        };
                        for t in texts {
                            let mut app = clone_form();
                            if let Some(s) = app.field_text_mut(field) {
                                *s = t;
                            }
                            candidates.push(app);
                        }
                    }
                }
            }
        }
        // Settings the rows cannot set value by value: a mask recipe no preset gives (the
        // toolboxes' BET without erosion, QSMART's first echo), set as the hand-edited recipe; and
        // the inhomogeneity correction's three ways (none, makehomogeneous, N4), a toggle and its
        // method. Each is the form the engine's own apply gives.
        if let Ok(mut after) = super::command::config_from_app(self) {
            if rec::apply(&mut after, rule, value).is_ok() {
                match r.setting.as_str() {
                    "masking.sections" => {
                        let mut app = clone_form();
                        app.pipeline_state.mask_preset = super::app::MASK_PRESET_CUSTOM;
                        app.pipeline_state.mask_sections = after.masking.sections.clone();
                        // First: the engine's own result, before any preset that also resolves the rule
                        // (HD-BET's recipe has no erosion step either).
                        candidates.insert(0, app);
                    }
                    "masking.inhomogeneity_correction" => {
                        let mut app = clone_form();
                        if app.field_toggle("inhomogeneity_correction") != after.masking.inhomogeneity_correction {
                            app.toggle_field("inhomogeneity_correction");
                        }
                        app.set_field_select("inhomogeneity_method",
                            usize::from(after.masking.inhomogeneity_method == qsmxt_engine::enums::InhomogeneityMethod::N4));
                        // First: the engine's own result, before any preset that also resolves the rule
                        // (HD-BET's recipe has no erosion step either).
                        candidates.insert(0, app);
                    }
                    _ => {}
                }
            }
        }
        // Apply means the recommended value: a rule that names values to avoid (heidi.seed-*,
        // `discouraged` ones) is also resolved by others, so prefer the form that takes the
        // recommended value, and only then any other that resolves it.
        let takes_recommended = |app: &App| value.is_some() || super::command::config_from_app(app)
            .map(|c| rec::reference_values(&c, &facts))
            .unwrap_or_default()
            .iter()
            .find(|x| x.rule == rule)
            .is_none_or(|x| x.current.iter().all(|n| n.value == r.recommended.value));
        let pick = candidates.iter().position(|a| resolved(a) && takes_recommended(a))
            .or_else(|| candidates.iter().position(resolved));
        match pick.map(|i| candidates.swap_remove(i)) {
            Some(app) => {
                self.form = app.form;
                self.pipeline_state = app.pipeline_state;
                self.filter_state = app.filter_state;
                self.pipeline_state.dismissed.retain(|d| d.rule != rule);
                self.status_message = Some(format!("{}: {}", r.label, value.and_then(|v| {
                    r.alternatives().into_iter().find(|a| a.value == *v).map(|a| a.name.clone())
                }).unwrap_or(r.recommended.name.clone())));
                Ok(())
            }
            None => Err(format!("{} cannot be set to {} here: change it on its row", r.label, r.recommended.name)),
        }
    }

    /// The other form fields applying `rule` sets, and their values: the engine's [`rec::apply`]
    /// on the form's config, read back from what it changed beyond `setting`.
    fn rule_companions(&self, rule: &str, value: Option<&Value>, setting: &str) -> Vec<(&'static str, Value)> {
        let Ok(before) = super::command::config_from_app(self) else { return vec![] };
        let mut after = before.clone();
        if rec::apply(&mut after, rule, value).is_err() {
            return vec![];
        }
        let (Ok(before), Ok(after)) = (serde_json::to_value(&before), serde_json::to_value(&after)) else { return vec![] };
        let mut changed = Vec::new();
        changed_settings(&before, &after, String::new(), &mut changed);
        changed.into_iter()
            .filter(|(path, _)| path != setting)
            .filter_map(|(path, v)| super::app::field_of_path(&path).map(|field| (field, v)))
            .collect()
    }

    /// Set a form field to a value of its setting, as a companion of an applied value.
    fn set_companion(&mut self, field: &str, value: &Value) {
        if let Value::Bool(on) = value {
            if self.field_toggle(field) != *on {
                self.toggle_field(field);
            }
        } else if let Some(text) = self.field_text_mut(field) {
            *text = field_text_of(value);
        }
    }

    /// A copy of the form, for trying a value on.
    pub(super) fn clone_form(&self) -> App {
        let mut app = App::new_for_trial();
        app.form = self.form.clone();
        app.pipeline_state = self.pipeline_state.clone();
        app.filter_state = self.filter_state.clone();
        app.input_mode = self.input_mode;
        app
    }

    /// The marker for a row ([`rec::marker`]): whose rule its setting departs from, what that
    /// rule uses, and whether it is offered as a change.
    pub fn row_marker(view: &[Recommendation], row: &PipelineRow) -> Option<Marker> {
        rec::marker(view, row_path(row)?)
    }

    /// The help line's reference values for a row: what each rule that covers the setting has
    /// as its reference, and whose rule it is, marked where the current value differs ("Authors'
    /// setting (RTS): V-SHARP"). Plain words only: the rules' sources are evidence for QSM.rs's
    /// maintainers, not text for the form. Each value is said once, by the strongest advice that
    /// gives it ("Required (HEIDI streaking reduction): LSQR", not also its authors' setting and
    /// a suggestion of the same LSQR); `!` lists every rule.
    pub fn row_reference(view: &[Recommendation], row: &PipelineRow) -> Option<String> {
        let path = row_path(row)?;
        // One entry for what several rules say alike: each HEIDI seed rule recommends LSQR.
        // Strongest first (the rank orders the groups), so a value goes to the strongest advice.
        let mut said: Vec<(u8, &str, String, String)> = view.iter().filter(|r| r.setting == path).map(|r| {
            let (rank, what) = match (r.departs, r.strength, r.dismissed) {
                (true, _, true) => (5, "Dismissed"),
                (true, Strength::Required, _) => (0, "Required"),
                (true, Strength::Discouraged, _) => (1, "Discouraged"),
                (true, Strength::Recommended, _) => (2, "Recommended"),
                (true, Strength::Suggested, _) => (3, "Suggested"),
                (true, Strength::Reference, _) => (4, "Differs from the authors' setting"),
                (false, Strength::Reference, _) => (7, "Authors' setting"),
                (false, Strength::Suggested, _) => (8, "Suggested"),
                (false, _, _) => (6, "Recommended"),
            };
            let value = if r.strength == Strength::Discouraged && r.departs {
                format!("not {}", r.current_text())
            } else {
                r.recommended.name.clone()
            };
            (rank, what, value, r.trigger.name.to_string())
        }).collect();
        said.sort_by_key(|(rank, ..)| *rank);
        let mut groups: Vec<(&str, Vec<String>, Vec<String>)> = Vec::new();
        let mut values_said: Vec<String> = Vec::new();
        for (_, what, value, who) in said {
            if values_said.contains(&value) {
                continue;
            }
            values_said.push(value.clone());
            match groups.iter_mut().find(|(w, _, _)| *w == what) {
                Some((_, values, names)) => {
                    values.push(value);
                    if !who.is_empty() && !names.contains(&who) { names.push(who) }
                }
                None => groups.push((what, vec![value], if who.is_empty() { vec![] } else { vec![who] })),
            }
        }
        // A few words each: several values read as a count, which `!` lists.
        let parts: Vec<String> = groups.into_iter().map(|(what, values, names)| {
            let whose = if names.is_empty() { String::new() } else { format!(" ({})", names.join(", ")) };
            let joined = values.join(", ");
            let said = if values.len() == 1 || joined.chars().count() <= REFERENCE_VALUES_MAX { joined } else { format!("{} values", values.len()) };
            format!("{what}{whose}: {said}")
        }).collect();
        if parts.is_empty() {
            return None;
        }
        let marked = view.iter().any(|r| r.setting == path && r.departs);
        Some(format!("{}{}", parts.join(" · "), if marked { " — ! for details" } else { "" }))
    }
}

/// The settings that differ between two configs (as JSON), by path, with their new values.
fn changed_settings(before: &Value, after: &Value, path: String, out: &mut Vec<(String, Value)>) {
    match (before, after) {
        (Value::Object(b), Value::Object(a)) => {
            for (k, v) in a {
                let path = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                changed_settings(b.get(k).unwrap_or(&Value::Null), v, path, out);
            }
        }
        _ if before != after => out.push((path, after.clone())),
        _ => {}
    }
}

/// A value as a text row holds it: one number for a vector with the same value on every axis.
fn field_text_of(value: &Value) -> String {
    let plain = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    match value {
        Value::Array(xs) if xs.iter().all(|x| Some(x) == xs.first()) && !xs.is_empty() => plain(&xs[0]),
        Value::Array(xs) => xs.iter().map(plain).collect::<Vec<_>>().join(" "),
        other => plain(other),
    }
}

/// The most characters of values the help line lists for one kind of reference before it
/// counts them instead ([`App::row_reference`]).
const REFERENCE_VALUES_MAX: usize = 40;

/// The config setting a row edits, for the rules (the mask recipe's rows edit `masking.sections`).
fn row_path(row: &PipelineRow) -> Option<&'static str> {
    match row {
        PipelineRow::AlgoSelect { field, .. } | PipelineRow::Toggle { field, .. } | PipelineRow::Param { field, .. } => {
            if *field == "mask_preset" { Some("masking.sections") } else { super::app::field_path(field) }
        }
        PipelineRow::MaskOpGenerator { .. } => Some("masking.sections"),
        _ => None,
    }
}

/// What a selector row chooses, for "Enter: choose method": "method" for a selector whose
/// choice can open a reference pipeline pop-up ([`rec::reference_trigger_settings`]), else its
/// label. Every selector is chosen from a picker (Enter), never cycled, so no method is chosen
/// on the way to another. `None` for a row that is not a selector of a setting.
pub fn picker_noun(row: &PipelineRow) -> Option<String> {
    static TRIGGERS: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(rec::reference_trigger_settings);
    let label = match row {
        PipelineRow::AlgoSelect { label, .. } => label.trim(),
        PipelineRow::MaskOpInput { .. } => "Input",
        PipelineRow::MaskOpGenerator { .. } => "Algorithm",
        _ => return None,
    };
    if row_path(row).is_some_and(|p| TRIGGERS.contains(&p)) {
        Some("method".into())
    } else {
        Some(label.to_lowercase())
    }
}

/// `first`, with the findings as strong on its setting that disagree with it.
fn group(view: &[Recommendation], first: &Recommendation) -> Vec<Recommendation> {
    if !first.conflict {
        return vec![first.clone()];
    }
    view.iter().filter(|r| r.setting == first.setting && r.strength == first.strength && open(r)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::app::{QSM_ALGO_OPTIONS, QSM_PICKER_OPTIONS, LSQR_HEIDI};

    fn app() -> App {
        let mut app = App::new_for_trial();
        app.check_recommendations();
        app
    }

    fn select(app: &mut App, field: &str, value: &str, options: &[&str]) {
        app.set_field_select(field, options.iter().position(|o| *o == value).unwrap());
        app.check_recommendations();
    }

    /// MEDI with mSMV and its prefilter off: discouraged (`medi-msmv.prefilter`).
    fn medi_msmv_without_prefilter() -> App {
        let mut app = app();
        select(&mut app, "qsm_algorithm", "medi", &QSM_PICKER_OPTIONS);
        assert!(app.rec_modal.is_none(), "MEDI at its defaults departs from nothing it is offered");
        // Its reference pipeline pop-up: closed, nothing changes.
        app.handle_ref_modal_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.refs.modal.is_none());
        app.toggle_field("medi_msmv");
        app.check_recommendations();
        assert!(app.rec_modal.is_none());
        app.toggle_field("medi_msmv_prefilter");
        app.check_recommendations();
        app
    }

    /// The pop-up's rows as decisions for [`App::decide`]: each value taken, or each kept.
    fn items_of(m: &AdviceModal, take: bool) -> Vec<RefItem> {
        m.rows.iter().filter(|r| !r.alternative).map(|r| RefItem {
            finding: r.finding.clone(), also: r.also.clone(),
            pick: take.then(|| (None, r.finding.recommended.name.clone())),
        }).collect()
    }

    #[test]
    fn a_change_that_creates_a_recommendation_opens_the_modal_and_apply_resolves_it() {
        let mut app = medi_msmv_without_prefilter();
        let modal = app.rec_modal.as_ref().expect("the change opened the pop-up");
        assert_eq!(modal.findings[0].rule, "medi-msmv.prefilter");
        assert!(modal.rows[0].checked, "the change starts ticked");
        assert!(modal.keepable());
        app.handle_rec_modal_key(KeyEvent::from(KeyCode::Char('a')));
        assert!(app.rec_modal.is_none());
        assert!(app.field_toggle("medi_msmv_prefilter"), "applied through the form");
        let config = super::super::command::config_from_app(&app).unwrap();
        assert!(config.inversion.medi.msmv_prefilter);
        assert!(!app.rule_view().iter().any(|r| r.rule == "medi-msmv.prefilter" && r.departs));
    }

    /// A suggestion (not preselected) starts unticked, in either pop-up; iHARPERELLA's whole rim
    /// (a recommendation) starts ticked.
    #[test]
    fn a_suggestion_starts_unticked() {
        let mut a = app();
        select(&mut a, "qsm_algorithm", "ndi", &QSM_PICKER_OPTIONS);
        let ndi: Vec<_> = a.rule_view().into_iter().filter(|r| r.rule == "ndi.weight-norm-in-vivo" && r.departs).collect();
        assert_eq!(ndi.len(), 1, "{ndi:?}");
        assert_eq!(RecModal::new(ndi.clone()).ticked(), 0);
        assert_eq!(AdviceModal::reference(vec![ndi[0].trigger.clone()], ndi.clone()).ticked(), 0);
        let mut b = app();
        b.pipeline_state.set_select("bf_algorithm", super::super::app::BF_OPTIONS.iter().position(|o| *o == "iharperella").unwrap());
        let rim: Vec<_> = b.rule_view().into_iter().filter(|r| r.rule == "iharperella.whole-rim" && r.departs).collect();
        assert_eq!(rim.len(), 1);
        assert!(RecModal::new(rim).rows[0].checked);
    }

    /// Choosing WH-QSM offers its in-mask flat weight as a recommendation, ticked; Apply takes it.
    /// The authors' flat weight stays the default until then.
    #[test]
    fn choosing_whqsm_ticks_its_in_mask_weight() {
        let mut app = app();
        select(&mut app, "qsm_algorithm", "whqsm", &QSM_PICKER_OPTIONS);
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.inversion.whqsm.weight, qsmxt_engine::enums::WhqsmWeight::Flat, "nothing rewritten");
        let modal = app.rec_modal.as_ref().expect("the recommendation's pop-up");
        let row = modal.rows.iter().find(|r| r.finding.rule == "whqsm.flat-in-mask").expect("its row");
        assert!(row.checked && row.finding.strength == Strength::Recommended);
        let text = screen(&mut app, "WH-QSM chosen");
        assert!(text.contains("Data weight (WH-QSM)  Flat → Flat in mask") && text.contains("(recommended)"), "{text}");
        key(&mut app, KeyCode::Char('a'));
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.inversion.whqsm.weight, qsmxt_engine::enums::WhqsmWeight::FlatInMask);
    }

    #[test]
    fn ignore_dismisses_until_the_setting_changes_and_esc_leaves_a_badge() {
        let mut app = medi_msmv_without_prefilter();
        // Esc: no change, the finding stays open with its badge; `!` brings the pop-up back.
        app.handle_rec_modal_key(KeyEvent::from(KeyCode::Esc));
        let row = PipelineRow::Toggle { label: "", field: "medi_msmv_prefilter", help: "" };
        assert!(App::row_marker(&app.rule_view(), &row).is_some_and(|m| m.actionable && m.text == "MEDI: not off"));
        assert!(App::row_reference(&app.rule_view(), &row).is_some_and(|t| t.starts_with("Discouraged (MEDI): not off") && t.ends_with("! for details")));
        app.check_recommendations();
        assert!(app.rec_modal.is_none(), "not opened again by a change that creates nothing");
        app.rec_modal = Some(RecModal::new(app.rule_view().into_iter().filter(|r| r.rule == "medi-msmv.prefilter").collect()));
        app.handle_rec_modal_key(KeyEvent::from(KeyCode::Char('K')));
        assert_eq!(app.pipeline_state.dismissed.len(), 1);
        let config = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(config.dismissed_recommendations[0].rule, "medi-msmv.prefilter", "the dismissal reaches the config");
        assert!(super::super::command::build_command_string(&app).contains("--dismiss-recommendation medi-msmv.prefilter=false"));
        assert!(App::row_marker(&app.rule_view(), &row).is_some_and(|m| !m.actionable), "dismissed: information only");
        // A change to the setting drops the dismissal; going back offers it again.
        app.toggle_field("medi_msmv_prefilter");
        app.check_recommendations();
        assert!(app.pipeline_state.dismissed.is_empty());
        app.toggle_field("medi_msmv_prefilter");
        app.check_recommendations();
        assert!(app.rec_modal.is_some());
    }

    /// Ticking HEIDI after a minimally regularised inversion (RTS, TKD, Tikhonov) opens HEIDI's
    /// reference pipeline pop-up: LSQR as the authors' seed, unticked, so Keep mine changes
    /// nothing and ticking it then Apply chooses LSQR. After a regularised one (MEDI) it opens
    /// the suggestion of LSQR, unticked too. HEIDI stays ticked either way. After LSQR no seed is
    /// offered.
    #[test]
    fn ticking_heidi_offers_lsqr() {
        let lsqr = QSM_ALGO_OPTIONS.iter().position(|o| *o == "lsqr").unwrap();
        let ticked = |alg: &str| {
            let mut app = app();
            select(&mut app, "qsm_algorithm", alg, &QSM_PICKER_OPTIONS);
            // The method's own reference pipeline pop-up, if any: closed, nothing changes.
            app.handle_ref_modal_key(KeyEvent::from(KeyCode::Esc));
            app.rec_modal = None;
            app.check_recommendations();
            let chosen = app.pipeline_state.qsm_algorithm;
            app.toggle_field("heidi_enabled");
            app.check_recommendations();
            if alg == "tkd" {
                // TKD's threshold at HEIDI's cone first (required), then the seed.
                let modal = app.rec_modal.as_ref().expect("tkd: the cone threshold");
                assert_eq!(modal.findings[0].rule, "heidi.tkd-cone");
                app.handle_rec_modal_key(KeyEvent::from(KeyCode::Char('a')));
                app.check_recommendations();
            }
            (app, chosen)
        };
        for alg in ["rts", "tkd", "tikhonov"] {
            for apply in [false, true] {
                let (mut app, chosen) = ticked(alg);
                assert!(app.rec_modal.is_none(), "{alg}: {:?}", app.rec_modal.as_ref().map(|m| &m.findings[0].rule));
                let modal = app.refs.modal.as_ref().unwrap_or_else(|| panic!("{alg}: ticking HEIDI opened no pop-up"));
                let rules: Vec<_> = modal.rows.iter().map(|i| (i.finding.rule.as_str(), i.finding.strength)).collect();
                assert_eq!(rules, [("heidi.seed-lsqr", Strength::Reference)], "{alg}");
                assert_eq!(modal.rows[0].finding.recommended.value, serde_json::json!("lsqr"), "{alg}");
                assert_eq!(modal.ticked(), 0, "{alg}: the authors' setting starts unticked");
                if alg == "tkd" && !apply {
                    let text = screen(&mut app, "TKD, then HEIDI ticked");
                    assert!(text.contains("(reference)") && text.contains("LSQR") && text.contains("Keep mine"), "{text}");
                }
                if apply {
                    app.refs.modal.as_mut().unwrap().toggle(0);
                }
                app.handle_ref_modal_key(KeyEvent::from(if apply { KeyCode::Char('a') } else { KeyCode::Char('K') }));
                let want = if apply { lsqr } else { chosen };
                assert_eq!(app.pipeline_state.qsm_algorithm, want, "{alg}: apply {apply}");
                assert!(app.pipeline_state.heidi_enabled, "{alg}: HEIDI stays ticked");
            }
        }
        for apply in [false, true] {
            let (mut app, chosen) = ticked("medi");
            assert!(app.refs.modal.is_none());
            let modal = app.rec_modal.as_ref().expect("medi: ticking HEIDI opened no pop-up");
            let rules: Vec<_> = modal.findings.iter().map(|f| (f.rule.as_str(), f.strength)).collect();
            assert_eq!(rules, [("heidi.seed-regularised", Strength::Suggested)]);
            assert_eq!(modal.findings[0].recommended.value, serde_json::json!("lsqr"));
            assert_eq!(modal.ticked(), 0, "a suggestion starts unticked");
            if !apply {
                let text = screen(&mut app, "MEDI, then HEIDI ticked");
                assert!(text.contains("LSQR") && text.contains("(suggested)") && text.contains("Keep mine"), "{text}");
            }
            if apply {
                app.rec_modal.as_mut().unwrap().toggle(0);
            }
            app.handle_rec_modal_key(KeyEvent::from(if apply { KeyCode::Char('a') } else { KeyCode::Char('K') }));
            assert_eq!(app.pipeline_state.qsm_algorithm, if apply { lsqr } else { chosen }, "medi: apply {apply}");
            assert!(app.pipeline_state.heidi_enabled, "medi: HEIDI stays ticked");
            let config = super::super::command::config_from_app(&app).unwrap();
            assert!(config.inversion.heidi.enabled);
        }
        // LSQR+HEIDI is the reference: nothing to offer.
        let mut app = app();
        select(&mut app, "qsm_algorithm", "lsqr", &QSM_PICKER_OPTIONS);
        app.handle_ref_modal_key(KeyEvent::from(KeyCode::Esc));
        app.rec_modal = None;
        app.check_recommendations();
        app.toggle_field("heidi_enabled");
        app.check_recommendations();
        assert!(app.rec_modal.is_none(), "{:?}", app.rec_modal.as_ref().map(|m| &m.findings[0].rule));
        assert!(!app.refs.modal.as_ref().is_some_and(|m| m.rows.iter().any(|i| i.finding.rule.starts_with("heidi.seed"))));
    }

    /// Choosing DECOMPOSE offers a per-echo QSM (its authors' input); applying it switches the
    /// form to per-echo, which hides the echo combination.
    #[test]
    fn choosing_decompose_offers_a_per_echo_qsm() {
        let mut app = app();
        app.toggle_field("do_chi_separation");
        app.check_recommendations();
        // The default method's reference pipeline pop-up: closed.
        app.refs.modal = None;
        select(&mut app, "separation_algorithm", "decompose", super::super::app::SEP_ALGO_OPTIONS);
        // One pop-up: the recommendation first, then DECOMPOSE's reference pipeline.
        let modal = app.refs.modal.take().expect("the choice opened the pop-up");
        let finding = &modal.rows.iter().find(|i| i.finding.rule == "decompose.qsm-per-echo").expect("per-echo QSM is offered").finding;
        assert_eq!((finding.setting.as_str(), finding.strength), ("qsm.per_echo", Strength::Recommended));
        assert!(!app.pipeline_state.qsm_per_echo, "offered, not applied");
        app.apply_recommendation("decompose.qsm-per-echo", None).unwrap();
        assert!(app.pipeline_state.qsm_per_echo);
        assert!(super::super::command::config_from_app(&app).unwrap().qsm.per_echo);
        let mut rows = Vec::new();
        app.pipeline_state.field_map_rows(&mut rows);
        assert!(!rows.iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "b0_estimation", .. })));
        assert!(rows.iter().any(|r| matches!(r, PipelineRow::Toggle { field: "qsm_per_echo", .. })));
    }

    /// DECOMPOSE at 7 T suggests its echo-phase unwrapping once the field strength is known; at
    /// 3 T, or before it is known, nothing.
    #[test]
    fn decompose_at_7t_suggests_unwrapping_the_phase() {
        let mut app = app();
        app.toggle_field("do_chi_separation");
        app.set_field_select("separation_algorithm", super::super::app::SEP_ALGO_OPTIONS.iter().position(|o| *o == "decompose").unwrap());
        let fires = |app: &App| app.rule_view().iter().any(|r| r.rule == "decompose.unwrap-7t" && r.departs);
        assert!(!fires(&app), "the field strength is not known");
        app.input_mode = super::super::app::InputMode::NIfTI;
        app.nifti_state.field_strength = "3".into();
        assert!(!fires(&app));
        app.nifti_state.field_strength = "7".into();
        let r = app.rule_view().into_iter().find(|r| r.rule == "decompose.unwrap-7t").unwrap();
        assert!(r.departs && r.setting == "separation.decompose.unwrap_phase" && r.reason.contains("3 T"), "{r:?}");
        app.toggle_field("sep_decompose_unwrap_phase");
        assert!(!fires(&app));
    }

    /// DECOMPOSE at 7 T offers both of its suggestions, unwrapping and the χ bound from the echo
    /// spacing, each shown plainly; nothing changes until they are applied.
    #[test]
    fn decompose_at_7t_offers_unwrapping_and_the_echo_spacing_bound() {
        let mut app = app();
        app.toggle_field("do_chi_separation");
        app.set_field_select("separation_algorithm", super::super::app::SEP_ALGO_OPTIONS.iter().position(|o| *o == "decompose").unwrap());
        app.check_recommendations();
        app.rec_modal = None;
        app.refs.modal = None;
        app.input_mode = super::super::app::InputMode::NIfTI;
        app.nifti_state.field_strength = "7".into();
        // Knowing the field strength opens the pop-up.
        app.check_recommendations();
        let shown: Vec<String> = app.rec_modal.take().map(|m| m.findings)
            .or_else(|| app.refs.modal.take().map(|m| m.rows.into_iter().map(|i| i.finding).collect()))
            .expect("7 T opened the pop-up").into_iter().map(|r| r.rule).collect();
        assert!(shown.iter().any(|r| r.ends_with("-7t")), "{shown:?}");
        // Closed, the other 7 T suggestion opens next: both are offered.
        app.check_recommendations();
        let next: Vec<String> = app.rec_modal.take().expect("the second 7 T suggestion opened").findings
            .into_iter().map(|r| r.rule).collect();
        let mut both: Vec<String> = shown.into_iter().chain(next).collect();
        both.sort();
        assert_eq!(both, ["decompose.chi-bound-7t", "decompose.unwrap-7t"]);
        app.check_recommendations();
        assert!(app.rec_modal.is_none());
        let view = app.rule_view();
        let bound = view.iter().find(|r| r.rule == "decompose.chi-bound-7t").expect("the χ bound is offered");
        assert!(bound.departs && bound.setting == "separation.decompose.chi_bound", "{bound:?}");
        assert_eq!((bound.current_text(), bound.recommended.name.as_str()), ("0.5 ppm".to_string(), "From the echo spacing"));
        assert!(view.iter().any(|r| r.rule == "decompose.unwrap-7t" && r.departs));
        // The pop-up offers the value by its name.
        let modal = RecModal::new(vec![bound.clone()]);
        assert_eq!(modal.rows[0].change().1, "From the echo spacing");
        assert!(modal.keepable());
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!((c.separation.decompose.chi_bound, c.separation.decompose.unwrap_phase),
                   (qsmxt_engine::ChiBoundChoice::Ppm(0.5), false), "offered, not applied");
        app.apply_recommendation("decompose.chi-bound-7t", None).unwrap();
        app.apply_recommendation("decompose.unwrap-7t", None).unwrap();
        let left: Vec<_> = app.rule_view().into_iter().filter(|r| r.departs && r.rule.starts_with("decompose.") && r.rule.ends_with("-7t")).collect();
        assert!(left.is_empty(), "{left:?}");
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!((c.separation.decompose.chi_bound, c.separation.decompose.unwrap_phase),
                   (qsmxt_engine::ChiBoundChoice::EchoSpacing, true));
        // The bound's own row is gone: it is computed.
        let mut rows = Vec::new();
        app.pipeline_state.separation_rows(&mut rows);
        assert!(!rows.iter().any(|r| matches!(r, PipelineRow::Param { field: "sep_decompose_chi_bound", .. })));
    }

    /// A reference departure never opens the pop-up; its setting shows the reference value.
    #[test]
    fn reference_departures_are_information_only() {
        let mut app = app();
        select(&mut app, "qsm_algorithm", "rts", &QSM_PICKER_OPTIONS);
        let row = PipelineRow::AlgoSelect { label: "", field: "bf_algorithm", options: &[], help: &[] };
        let text = App::row_reference(&app.rule_view(), &row).unwrap();
        assert!(text.starts_with("Authors' setting (RTS): V-SHARP") && !text.contains("README"), "{text}");
        select(&mut app, "bf_algorithm", "pdf", super::super::app::BF_OPTIONS);
        assert!(app.rec_modal.is_none(), "information only");
        assert!(app.refs.modal.as_ref().is_none_or(|m| m.methods()[0].name == "PDF"), "only a method just chosen opens its reference");
        assert_eq!(App::row_marker(&app.rule_view(), &row).map(|m| (m.actionable, m.text)), Some((false, "RTS: V-SHARP".into())));
        assert!(App::row_reference(&app.rule_view(), &row).unwrap().starts_with("Differs from the authors' setting (RTS): V-SHARP"));
    }

    /// The screen with its pop-ups, as text (the PR's renders: `--nocapture` prints them).
    fn screen(app: &mut App, name: &str) -> String {
        use ratatui::{backend::TestBackend, Terminal};
        let mut t = Terminal::new(TestBackend::new(110, 40)).unwrap();
        t.draw(|f| super::super::ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer();
        let text = (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>().join("\n");
        println!("──── {name} ────\n{text}\n");
        text
    }

    fn key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::from(code));
        app.check_recommendations();
    }

    /// MEDI chosen with V-SHARP and the threshold mask (QSMxT's defaults): its reference
    /// pipeline pop-up lists both.
    fn medi_chosen() -> App {
        let mut app = app();
        assert_eq!(super::super::app::MASK_PRESET_OPTIONS[app.pipeline_state.mask_preset], "robust-threshold");
        assert!(app.refs.modal.is_none(), "nothing at start-up");
        select(&mut app, "qsm_algorithm", "medi", &QSM_PICKER_OPTIONS);
        app
    }

    fn settings(modal: &RefModal) -> Vec<&str> {
        modal.rows.iter().map(|i| i.finding.setting.as_str()).collect()
    }

    /// A form whose reference differences include two authors' settings that disagree: MEDI's PDF
    /// and χ-separation's V-SHARP, with LBV chosen. The pop-up for them, on Apply all.
    fn disagreeing_bg_removal() -> (App, Vec<RefItem>) {
        let mut app = medi_chosen();
        app.refs.modal = None;
        app.toggle_field("do_chi_separation");
        let at = |o: &[&str], v: &str| o.iter().position(|x| *x == v).unwrap();
        app.set_field_select("separation_algorithm", at(super::super::app::SEP_ALGO_OPTIONS, "chi-sep-ilsqr"));
        app.set_field_select("bf_algorithm", at(super::super::app::BF_OPTIONS, "lbv"));
        let found: Vec<Recommendation> = app.rule_view().into_iter().filter(|r| r.authors_setting()).collect();
        let modal = AdviceModal::reference(vec![found[0].trigger.clone()], found);
        (app, items_of(&modal, true))
    }

    /// Apply all leaves a setting the authors disagree on as it is, says so, and records nothing;
    /// the other settings are applied.
    #[test]
    fn apply_all_leaves_authors_settings_that_disagree() {
        let (mut app, items) = disagreeing_bg_removal();
        let bg: Vec<&str> = items.iter().filter(|i| i.finding.setting == "bg_removal.algorithm").map(|i| i.finding.rule.as_str()).collect();
        assert!(bg.len() >= 2, "{bg:?}");
        assert!(items.iter().any(|i| i.finding.setting == "masking.sections"));
        app.decide(&items, true);
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::enums::BfAlgorithm::Lbv, "the user's value stays");
        assert!(c.dismissed_recommendations.is_empty(), "nothing is recorded for a setting left");
        // The mask is the toolboxes' recipe: BET on the magnitude, not eroded (MEDI's and the
        // χ-separation toolbox's mask rules agree on it).
        assert!(c.masking.sections.iter().all(|s| matches!(s.generator, qsmxt_engine::masking::MaskOp::Bet { .. })
            && !s.refinements.iter().any(|op| matches!(op, qsmxt_engine::masking::MaskOp::Erode { .. }))), "{:?}", c.masking.sections);
        let status = app.status_message.clone().unwrap();
        assert!(status.contains("left Background removal as set") && status.contains("! to choose"), "{status}");
    }

    /// Choosing per setting, one of the two is kept: the other is then the only one picked, and
    /// applied.
    #[test]
    fn keeping_one_of_two_disagreeing_authors_settings_applies_the_other() {
        let (mut app, mut items) = disagreeing_bg_removal();
        let first = items.iter().position(|i| i.finding.setting == "bg_removal.algorithm").unwrap();
        let wanted = items.iter().rfind(|i| i.finding.setting == "bg_removal.algorithm").unwrap().finding.recommended.value.clone();
        items[first].pick = None;
        app.decide(&items, true);
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(serde_json::to_value(c.bg_removal.algorithm).unwrap(), wanted);
    }

    /// A reasoned finding outranks an authors' setting on Apply all, whichever the pop-up lists
    /// first: FANSI's suggested ROMEO over the complex fit's authors' unwrapper.
    #[test]
    fn apply_all_lets_a_recommendation_outrank_an_authors_setting() {
        let mut app = app();
        select(&mut app, "qsm_algorithm", "fansi", QSM_ALGO_OPTIONS);
        let at = |o: &[&str], v: &str| o.iter().position(|x| *x == v).unwrap();
        app.set_field_select("b0_estimation", at(super::super::app::B0_ESTIMATION_OPTIONS, "complex-fit"));
        app.set_field_select("unwrapping_algorithm", at(super::super::app::UNWRAP_OPTIONS, "laplacian"));
        app.filter_state.num_echoes = "4".into();
        let found: Vec<Recommendation> = app.rule_view().into_iter()
            .filter(|r| r.departs && r.setting == "field_mapping.unwrapping_algorithm").collect();
        assert!(found.iter().any(|r| r.kind == Kind::Reasoned) && found.iter().any(|r| r.kind == Kind::Reference), "{found:?}");
        let modal = AdviceModal::reference(vec![found[0].trigger.clone()], found);
        let mut items = items_of(&modal, true);
        items.reverse();
        app.decide(&items, true);
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.unwrapping_algorithm, qsmxt_engine::enums::UnwrappingAlgorithm::Romeo);
        assert!(app.status_message.as_deref().unwrap().contains("a stronger suggestion"));
    }

    #[test]
    fn choosing_medi_opens_its_reference_pipeline_and_apply_all_takes_it() {
        let mut app = medi_chosen();
        let modal = app.refs.modal.as_ref().expect("MEDI's reference pipeline pop-up");
        assert_eq!(modal.title(), " MEDI's reference pipeline ");
        // Only the authors' settings: none ticked, so Enter alone changes nothing.
        assert!(modal.rows.iter().all(|i| i.finding.kind == Kind::Reference && !i.checked));
        let s = settings(modal);
        assert!(s.contains(&"bg_removal.algorithm") && s.contains(&"masking.sections"), "{s:?}");
        let text = screen(&mut app, "MEDI chosen");
        assert!(text.contains("V-SHARP → PDF") && text.contains("threshold → BET") && !text.contains("README.m"), "{text}");
        assert!(text.contains("(reference)") && text.contains("Apply (0)") && text.contains("Keep mine"), "{text}");
        assert!(text.contains("Preprocessing › Masking") && text.contains("QSM › Background"), "grouped by section:\n{text}");
        key(&mut app, KeyCode::Char('a'));
        assert!(app.refs.modal.is_some(), "nothing ticked: Apply does nothing");
        // Every authors' setting ticked, then applied.
        let m = app.refs.modal.as_mut().unwrap();
        for i in 0..m.rows.len() {
            if !m.rows[i].checked {
                m.toggle(i);
            }
        }
        key(&mut app, KeyCode::Char('a'));
        // The complex fit is now chosen, and PDF: one pop-up, PDF's measured suggestion of 300 CG
        // iterations first (preselected), then the complex fit's authors' unwrapper (not).
        let modal = app.refs.modal.as_ref().expect("the complex fit's authors' setting, with PDF's suggestion");
        assert_eq!(modal.title(), " Complex fit's and MEDI's reference pipelines ");
        let rules: Vec<&str> = modal.rows.iter().map(|i| i.finding.rule.as_str()).collect();
        // MEDI's BET orientation came into play with the BET mask just applied: offered with them.
        assert_eq!(rules, ["pdf.cg-iterations", "complex-fit.unwrap-region-growing", "medi.bet-as-stored"]);
        assert_eq!(modal.rows.iter().map(|r| r.checked).collect::<Vec<_>>(), [true, false, false], "the recommendation ticked");
        let text = screen(&mut app, "MEDI's settings applied: PDF's suggestion and the complex fit's unwrapper");
        assert!(text.contains("(recommended)") && text.contains("Apply (1)"), "{text}");
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::enums::BfAlgorithm::Pdf);
        assert_eq!(c.bg_removal.pdf.n_cg, qsm_core::bgremove::PdfParams::default().n_cg, "nothing rewritten yet");
        key(&mut app, KeyCode::Char('a'));
        assert!(app.refs.modal.is_none() && app.rec_modal.is_none());
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.pdf.n_cg, 300, "Apply takes the ticked recommendation");
        assert_eq!(c.field_mapping.unwrapping_algorithm, qsmxt_engine::enums::UnwrappingAlgorithm::Romeo, "and keeps ROMEO");
        let medi = rec::chosen_methods(&c, &Facts::default()).into_iter().find(|m| m.name == "MEDI").unwrap();
        let left: Vec<String> = rec::reference_differences(&c, &Facts::default(), &medi).into_iter().map(|r| r.rule).collect();
        // The orientation was offered, not taken: an authors' setting is applied only when chosen.
        assert_eq!(left, ["medi.bet-as-stored"]);
        let row = PipelineRow::AlgoSelect { label: "", field: "mask_preset", options: &[], help: &[] };
        assert_eq!(App::row_marker(&app.rule_view(), &row), None, "the mask is BET now");
    }

    #[test]
    fn keep_mine_is_not_offered_again_until_the_method_is_chosen_again() {
        let mut app = medi_chosen();
        key(&mut app, KeyCode::Char('K'));
        assert!(app.refs.modal.is_none());
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::enums::BfAlgorithm::Vsharp, "nothing changed");
        // A reference difference is information only: nothing recorded, in the config or the command.
        assert!(c.dismissed_recommendations.is_empty(), "{:?}", c.dismissed_recommendations);
        assert!(!super::super::command::build_command_string(&app).contains("--dismiss-recommendation"));
        // The background removal row keeps its ⓘ marker.
        let bg = PipelineRow::AlgoSelect { label: "", field: "bf_algorithm", options: &[], help: &[] };
        assert!(App::row_marker(&app.rule_view(), &bg).is_some_and(|m| !m.actionable && m.text.contains("MEDI")));
        // Another change, MEDI still chosen: no pop-up.
        app.toggle_field("medi_merit");
        app.check_recommendations();
        assert!(app.refs.modal.is_none() && app.rec_modal.is_none());
        app.toggle_field("medi_merit");
        app.check_recommendations();
        assert!(app.refs.modal.is_none() && app.rec_modal.is_none());
        // Away and back: offered again.
        select(&mut app, "qsm_algorithm", "rts", &QSM_PICKER_OPTIONS);
        assert!(app.refs.modal.is_none());
        select(&mut app, "qsm_algorithm", "medi", &QSM_PICKER_OPTIONS);
        assert!(app.refs.modal.as_ref().is_some_and(|m| m.methods()[0].name == "MEDI"));
    }

    /// Ticking some rows applies them and keeps the rest.
    #[test]
    fn ticking_some_rows_applies_them_and_keeps_the_rest() {
        let mut app = medi_chosen();
        let modal = app.refs.modal.as_mut().unwrap();
        assert!(modal.rows.iter().all(|r| !r.checked), "the authors' settings start unticked");
        let bg = modal.rows.iter().position(|r| r.finding.setting == "bg_removal.algorithm").unwrap();
        modal.cursor = bg;
        app.handle_ref_modal_key(KeyEvent::from(KeyCode::Char(' ')));
        assert_eq!(app.refs.modal.as_ref().unwrap().ticked(), 1);
        screen(&mut app, "MEDI, the background removal ticked");
        key(&mut app, KeyCode::Char('a'));
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::enums::BfAlgorithm::Pdf, "row {bg} applied");
        assert!(!c.masking.sections.iter().any(|s| matches!(s.generator, qsmxt_engine::masking::MaskOp::Bet { .. })), "the mask kept");
        assert!(c.dismissed_recommendations.is_empty(), "keeping a reference difference records nothing");
        assert!(app.refs.modal.is_none(), "PDF's reference wants the mask just kept: not asked again");
    }

    /// Ticking a row unticks the others of its setting: two values of one setting cannot both be
    /// applied.
    #[test]
    fn a_settings_values_exclude_each_other() {
        let mut app = medi_msmv_without_prefilter();
        let m = app.rec_modal.as_mut().unwrap();
        let n = m.rows.len();
        assert!(n >= 1);
        for i in 0..n {
            m.toggle(i);
            let group: Vec<bool> = m.rows.iter().map(|r| r.checked).collect();
            assert!(group.iter().filter(|c| **c).count() <= 1, "{group:?}");
        }
    }

    /// QSMART, then the threshold mask: the mask row is marked ⓘ with QSMART's reference, and
    /// An app with HEIDI ticked and its pop-ups closed, then `method` chosen.
    fn method_chosen_with_heidi(method: &str) -> App {
        let chosen = QSM_PICKER_OPTIONS.iter().position(|o| *o == method).unwrap();
        let mut app = app();
        app.toggle_field("heidi_enabled");
        app.check_recommendations();
        while app.rec_modal.is_some() || app.refs.modal.is_some() {
            key(&mut app, KeyCode::Esc);
            app.check_recommendations();
        }
        app.pipeline_state.set_select("qsm_algorithm", chosen);
        app.check_recommendations();
        app
    }

    /// HEIDI ticked, then a method it cannot follow chosen: the method stays chosen and the pop-up
    /// offers to turn HEIDI off, ahead of the method's reference pipeline. Ticking HEIDI on a
    /// method is the other way round: the method is what is offered to change.
    #[test]
    fn choosing_a_method_against_heidi_offers_to_turn_heidi_off() {
        for method in ["tfi", "tgv", "qsmart", "autoqsm", "nextqsm", "iqsm", "iqsm-plus"] {
            let mut app = method_chosen_with_heidi(method);
            let chosen = app.pipeline_state.qsm_algorithm;
            let modal = app.rec_modal.as_ref().expect("the pop-up opens");
            assert_eq!(modal.title(), format!(" {} requires a change ", modal.findings[0].current_text()), "{method}");
            assert!(matches!(modal.rows[0].action, RowAction::TurnOff { .. }) && modal.rows[0].checked, "{method}: turning HEIDI off starts ticked");
            assert_eq!(modal.rows[0].change(), ("on".to_string(), "off".to_string()), "{method}");
            // Changing the method back is the other way to resolve the one conflict: it would
            // undo the change just made, so it is not offered.
            assert_eq!(modal.rows.len(), 1, "{method}: only turning HEIDI off: {:?}", modal.rows);
            assert!(!modal.keepable(), "{method}: a required finding has no Keep mine");
            let text = screen(&mut app, &format!("{method} chosen with HEIDI on"));
            assert!(!text.contains("→ LSQR"), "{method}: {text}");

            key(&mut app, KeyCode::Char('a'));
            assert!(!app.pipeline_state.heidi_enabled, "{method}: HEIDI is off");
            assert_eq!(app.pipeline_state.qsm_algorithm, chosen, "{method} stays chosen");
        }
    }

    /// The same for a method HEIDI only discourages, where the reference pipeline pop-up would
    /// otherwise have offered to change the method back along with the authors' settings.
    #[test]
    fn a_discouraged_method_just_chosen_is_not_changed_back_by_default() {
        let mut app = method_chosen_with_heidi("medi");
        let medi = app.pipeline_state.qsm_algorithm;
        let modal = app.rec_modal.as_ref().expect("the pop-up opens");
        assert!(matches!(modal.rows[0].action, RowAction::TurnOff { .. }) && modal.rows[0].checked);
        assert!(!modal.rows.iter().any(|r| r.finding.setting == "inversion.algorithm" && matches!(r.action, RowAction::Apply { .. })),
            "MEDI is not offered to be changed back: {:?}", modal.rows);
        assert!(modal.keepable());
        key(&mut app, KeyCode::Char('a'));
        assert!(!app.pipeline_state.heidi_enabled);
        assert_eq!(app.pipeline_state.qsm_algorithm, medi);
    }

    /// `!` on a method HEIDI cannot follow: turning HEIDI off and choosing LSQR are two ways to
    /// meet one requirement, so they are one choice: radio buttons, one ticked, the second
    /// tagged "or", and ticking one unticks the other without ever leaving none.
    #[test]
    fn ways_to_meet_one_requirement_are_one_choice() {
        let chosen = |name: &str| QSM_PICKER_OPTIONS.iter().position(|o| *o == name).unwrap();
        let mut app = app();
        app.pipeline_state.heidi_enabled = true;
        app.pipeline_state.set_select("qsm_algorithm", chosen("qsmart"));
        let here: Vec<_> = app.rule_view().into_iter()
            .filter(|r| r.setting == "inversion.algorithm" && r.departs && r.strength == Strength::Required).collect();
        let mut modal = RecModal::details(here);
        assert_eq!(modal.rows.len(), 2, "{:?}", modal.rows);
        assert!(modal.radio(0) && modal.radio(1));
        assert_eq!((modal.tag(0), modal.tag(1)), ("required", "or"));
        assert_eq!(modal.ticked(), 1);
        let ticked = modal.rows.iter().position(|r| r.checked).unwrap();
        modal.toggle(ticked);
        assert_eq!(modal.ticked(), 1, "a required choice keeps one ticked");
        modal.toggle(1 - ticked);
        assert!(modal.rows[1 - ticked].checked && !modal.rows[ticked].checked);
        // A ticked way out: the panel does not say the run will be refused.
        for i in 0..2 {
            assert!(!modal.explanation(i).contains("refuse"), "{}", modal.explanation(i));
        }
        app.rec_modal = Some(modal);
        let text = screen(&mut app, "details: two ways to meet one requirement");
        assert!(text.contains("(\u{2022})") && text.contains("( )") && text.contains("(or)"), "{text}");
    }

    /// Changing a value HEIDI requires (TKD's threshold above HEIDI's cone) offers only turning
    /// HEIDI off, ticked; ticking HEIDI on TKD offers only the threshold, not turning HEIDI back
    /// off: the change just made stands either way.
    #[test]
    fn every_required_conflict_respects_the_change_just_made() {
        let mut app = app();
        select(&mut app, "qsm_algorithm", "tkd", &QSM_PICKER_OPTIONS);
        app.refs.modal = None;
        app.rec_modal = None;
        app.toggle_field("heidi_enabled");
        app.check_recommendations();
        let m = app.rec_modal.as_ref().expect("HEIDI on TKD needs the cone threshold");
        assert!(m.rows.iter().all(|r| matches!(r.action, RowAction::Apply { .. })), "{:?}", m.rows);
        key(&mut app, KeyCode::Char('a'));
        while app.rec_modal.is_some() || app.refs.modal.is_some() {
            key(&mut app, KeyCode::Esc);
        }
        assert!(app.pipeline_state.heidi_enabled);
        // Now the threshold, back above the cone.
        let field = super::super::app::field_of_path("inversion.tkd.threshold").expect("the TKD threshold row");
        *app.field_text_mut(field).unwrap() = "0.25".into();
        app.check_recommendations();
        let m = app.rec_modal.as_ref().expect("the threshold goes against HEIDI");
        assert_eq!(m.kind, AdviceKind::Changed);
        assert_eq!(m.rows.len(), 1, "{:?}", m.rows);
        assert!(matches!(m.rows[0].action, RowAction::TurnOff { .. }) && m.rows[0].checked);
    }

    /// The keys: Enter ticks the highlighted row like Space; ↓ past the last row (or Tab) moves
    /// to the buttons, where Enter presses the one focused; `A` applies from anywhere; `K` (or
    /// `k`) keeps every setting and closes the pop-up, and does not move the highlight.
    #[test]
    fn enter_ticks_and_the_buttons_apply_or_keep() {
        let mut app = medi_chosen();
        let n = app.refs.modal.as_ref().unwrap().rows.len();
        assert_eq!(app.refs.modal.as_ref().unwrap().ticked(), 0);
        key(&mut app, KeyCode::Enter);
        let m = app.refs.modal.as_ref().expect("Enter on a row does not close the pop-up");
        assert!(m.rows[0].checked, "Enter ticks the row");
        key(&mut app, KeyCode::Enter);
        assert!(!app.refs.modal.as_ref().unwrap().rows[0].checked, "and unticks it");
        key(&mut app, KeyCode::Char(' '));
        for _ in 0..n {
            key(&mut app, KeyCode::Down);
        }
        assert_eq!(app.refs.modal.as_ref().unwrap().button, Some(0), "↓ past the last row: Apply");
        let text = screen(&mut app, "MEDI: Apply focused");
        assert!(text.contains("Space/Enter tick") && text.contains("A apply") && text.contains("K keep mine"), "{text}");
        key(&mut app, KeyCode::Up);
        assert_eq!(app.refs.modal.as_ref().unwrap().button, None);
        key(&mut app, KeyCode::Tab);
        let before = super::super::command::config_from_app(&app).unwrap();
        let rows = app.refs.modal.as_ref().unwrap().rows.clone();
        key(&mut app, KeyCode::Enter);
        assert!(app.refs.modal.as_ref().is_none_or(|m| m.rows != rows), "Enter on Apply applies (a follow-up may open)");
        assert_ne!(super::super::command::config_from_app(&app).unwrap(), before, "the ticked row applied");

        // Keep mine: Tab, →, Enter; and K, either case, from a row.
        for keys in [&[KeyCode::Tab, KeyCode::Right, KeyCode::Enter][..], &[KeyCode::Char('K')], &[KeyCode::Char('k')]] {
            let mut app = medi_chosen();
            let before = super::super::command::config_from_app(&app).unwrap();
            for k in keys {
                key(&mut app, *k);
            }
            assert!(app.refs.modal.is_none(), "{keys:?} closes the pop-up");
            assert_eq!(super::super::command::config_from_app(&app).unwrap(), before, "{keys:?} keeps every setting");
        }
        // Where Keep mine is not offered (every finding required), K closes it all the same.
        let mut app = method_chosen_with_heidi("qsmart");
        assert!(!app.rec_modal.as_ref().unwrap().keepable());
        key(&mut app, KeyCode::Char('k'));
        assert!(app.rec_modal.is_none() && app.pipeline_state.heidi_enabled);
    }

    /// Ticking every row of a method's reference pipeline pop-up and applying reaches a fixed
    /// point, for every toolbox pipeline's method and every other method: a follow-up offers
    /// only what the applied values bring into play (BET's fractional intensity once the mask is
    /// BET; PDF's own authors' settings once HD-QSM's PDF is applied), nothing is offered twice,
    /// and then none of the method's authors' settings is left. Ticking only some rows also
    /// converges: the rows kept are not offered again.
    #[test]
    fn applying_a_reference_pipeline_reaches_a_fixed_point() {
        use qsmxt_engine::pipelines;
        let picker = |c: &qsmxt_engine::config::PipelineConfig| {
            let name = serde_json::to_value(c.inversion.algorithm).unwrap().as_str().unwrap().to_string();
            if name == "lsqr" && c.inversion.heidi.enabled { LSQR_HEIDI.to_string() } else { name }
        };
        let mut methods: Vec<String> = Vec::new();
        for p in pipelines::pipelines().into_iter().filter(|p| p.toolbox.is_some()) {
            let mut app = App::new_for_trial();
            app.filter_state.num_echoes = "4".into();
            let c = super::super::command::config_from_app(&app).unwrap();
            let target = pipelines::target(&c, p.id, &app.view_facts()).unwrap();
            let m = picker(&target);
            assert!(QSM_PICKER_OPTIONS.contains(&m.as_str()), "{}: {m}", p.id);
            if !methods.contains(&m) {
                methods.push(m);
            }
        }
        for m in QSM_PICKER_OPTIONS.iter() {
            if !methods.iter().any(|x| x == m) {
                methods.push(m.to_string());
            }
        }
        for partial in [false, true] {
            for method in &methods {
                let mut app = App::new_for_trial();
                app.filter_state.num_echoes = "4".into();
                app.check_recommendations();
                // From another method, so that choosing QSMxT's default (iLSQR) is a change too.
                let other = if method == "tkd" { "rts" } else { "tkd" };
                app.set_field_select("qsm_algorithm", QSM_PICKER_OPTIONS.iter().position(|o| *o == other).unwrap());
                app.check_recommendations();
                while app.rec_modal.is_some() || app.refs.modal.is_some() {
                    key(&mut app, KeyCode::Esc);
                }
                app.set_field_select("qsm_algorithm", QSM_PICKER_OPTIONS.iter().position(|o| o == method).unwrap());
                app.check_recommendations();
                let mut pop_ups = 0;
                let mut reference_pop_ups = 0;
                let mut offered: Vec<(String, (String, String))> = Vec::new();
                while app.rec_modal.is_some() || app.refs.modal.is_some() {
                    pop_ups += 1;
                    assert!(pop_ups <= 8, "{method}: the pop-ups do not stop");
                    let reference = app.rec_modal.is_none();
                    reference_pop_ups += usize::from(reference);
                    let m = if reference { app.refs.modal.as_mut() } else { app.rec_modal.as_mut() }.unwrap();
                    for r in m.rows.iter().filter(|_| reference) {
                        let offer = (r.finding.rule.clone(), r.change());
                        assert!(!offered.contains(&offer), "{method}: {offer:?} offered again ({})", m.title());
                        offered.push(offer);
                    }
                    for i in 0..m.rows.len() {
                        let group = m.rows[i].group.clone();
                        let skip = partial && reference_pop_ups == 1 && i % 2 == 1;
                        if !skip && !m.rows[i].alternative && !m.rows.iter().any(|r| r.group == group && r.checked) {
                            m.toggle(i);
                        }
                    }
                    let k = if m.ticked() > 0 { KeyCode::Char('a') } else { KeyCode::Char('k') };
                    key(&mut app, k);
                }
                if partial {
                    continue;
                }
                // Nothing of the method's reference pipeline left, but where another chosen
                // method's authors disagree and the setting follows theirs (AMP-PE's echo
                // combination, once PDF's complex fit is taken).
                let c = super::super::command::config_from_app(&app).unwrap();
                let facts = app.rule_facts();
                let view = app.rule_view();
                let other_authors = |r: &Recommendation| view.iter().any(|o| o.trigger != r.trigger
                    && o.kind == Kind::Reference && !o.departs && rec::rule_setting(o) == rec::rule_setting(r));
                for m in rec::chosen_methods(&c, &facts).into_iter().filter(|m| m.setting == "inversion.algorithm") {
                    let left: Vec<String> = rec::reference_differences(&c, &facts, &m).into_iter()
                        .filter(|r| !r.conflict && !other_authors(r)).map(|r| format!("{} ({})", r.rule, r.current_text())).collect();
                    assert!(left.is_empty(), "{method}: {} left: {left:?}", m.name);
                }
            }
        }
    }

    /// With HEIDI's row hidden, `!` on the inversion is the way to turn it off later.
    #[test]
    fn details_of_a_setting_against_a_flag_offer_to_turn_it_off() {
        let chosen = |name: &str| QSM_PICKER_OPTIONS.iter().position(|o| *o == name).unwrap();
        let mut app = app();
        app.pipeline_state.heidi_enabled = true;
        app.pipeline_state.set_select("qsm_algorithm", chosen("qsmart"));
        let here: Vec<_> = app.rule_view().into_iter().filter(|r| r.setting == "inversion.algorithm" && r.departs).collect();
        let modal = RecModal::details(here);
        assert!(modal.rows.iter().any(|r| matches!(r.action, RowAction::TurnOff { .. })), "{:?}", modal.rows);
    }

    /// Rules that say the same thing on a setting read as one entry, naming each method once.
    #[test]
    fn a_rows_reference_says_each_thing_once() {
        let mut app = app();
        app.pipeline_state.heidi_enabled = true;
        app.pipeline_state.set_select("qsm_algorithm", QSM_PICKER_OPTIONS.iter().position(|o| *o == "lsqr").unwrap());
        app.pipeline_state.heidi_enabled = true;
        let view = app.rule_view();
        let row = app.all_pages().into_iter().flat_map(|(_, _, rows)| rows)
            .find(|r| matches!(r, PipelineRow::AlgoSelect { field: "qsm_algorithm", .. })).unwrap();
        let text = App::row_reference(&view, &row).unwrap();
        assert_eq!(text.matches("Recommended (HEIDI streaking reduction): LSQR").count(), 1, "{text}");
        assert_eq!(text.matches("LSQR").count(), 1, "the authors' setting and the suggestion agree: {text}");
    }

    /// Each value is said once, by the strongest advice that gives it: QSMART with HEIDI on
    /// shows HEIDI's requirement, not also its authors' setting and suggestion of the same LSQR.
    #[test]
    fn a_rows_reference_gives_each_value_to_its_strongest_advice() {
        let mut app = app();
        app.pipeline_state.heidi_enabled = true;
        app.pipeline_state.set_select("qsm_algorithm", QSM_PICKER_OPTIONS.iter().position(|o| *o == "qsmart").unwrap());
        let row = PipelineRow::AlgoSelect { label: "", field: "qsm_algorithm", options: &[], help: &[] };
        assert_eq!(App::row_reference(&app.rule_view(), &row).as_deref(),
                   Some("Required (HEIDI streaking reduction): LSQR — ! for details"));
    }

    /// `!` opens its details with Apply.
    #[test]
    fn a_reference_marker_names_the_method_and_bang_opens_its_details() {
        let mut app = app();
        app.set_field_select("mask_preset", 1);
        app.check_recommendations();
        select(&mut app, "qsm_algorithm", "qsmart", &QSM_PICKER_OPTIONS);
        app.refs.modal = None;
        app.set_field_select("mask_preset", 0);
        app.check_recommendations();
        assert!(app.refs.modal.is_none() && app.rec_modal.is_none(), "a mask change chooses no method");
        let row = PipelineRow::AlgoSelect { label: "", field: "mask_preset", options: &[], help: &[] };
        let m = App::row_marker(&app.rule_view(), &row).unwrap();
        assert_eq!((m.actionable, m.text.as_str()), (false, "QSMART: BET, first echo, intensity threshold 100, majority filter, closing radius 2, no erosion"));
        assert!(app.goto_field("mask_preset"));
        let text = screen(&mut app, "QSMART with the threshold mask");
        // Beside the setting, only whose rule it is: the recipe is too long for the row.
        assert!(text.contains("ⓘ QSMART") && !text.contains("first echo, intensity"), "{text}");
        key(&mut app, KeyCode::Char('!'));
        let modal = app.rec_modal.as_ref().expect("details");
        assert_eq!(modal.kind, AdviceKind::Details);
        assert_eq!(modal.title(), " Original authors' setting: QSMART's reference pipeline ");
        let bet = modal.rows.iter().position(|r| r.finding.rule == "qsmart.mask-bet").expect("BET offered");
        assert_eq!(modal.rows[bet].change().1, "BET");
        assert_eq!(modal.ticked(), 0, "an authors' setting starts unticked");
        app.rec_modal.as_mut().unwrap().cursor = bet;
        let text = screen(&mut app, "QSMART mask details (!)");
        assert!(text.contains("(reference)") && text.contains("QSMART's reference pipeline uses BET")
            && text.contains("not a recommendation") && !text.contains("Source"), "{text}");
        key(&mut app, KeyCode::Char(' '));
        key(&mut app, KeyCode::Char('a'));
        // BET now; the recipe's other parts (QSMART's first echo, f 0.2, no erosion) are their
        // own rules, still marked.
        assert!(App::row_marker(&app.rule_view(), &row).is_none_or(|m| !m.text.contains("BET")));
    }

    /// The defaults' mask beside iLSQR's reference recipe: a few words on the row, the recipe in
    /// the details.
    #[test]
    fn markers_beside_a_setting_are_a_few_words() {
        let mut app = app();
        app.check_recommendations();
        assert!(app.goto_field("mask_preset"));
        let text = screen(&mut app, "the defaults' mask");
        let row = text.lines().find(|l| l.contains("Mask preset:")).unwrap();
        assert!(row.contains("ⓘ iLSQR (! for advice)") && !row.contains("thresh_tool"), "{row}");
        key(&mut app, KeyCode::Char('!'));
        let text = screen(&mut app, "its details");
        assert!(text.contains("thresh_tool"), "{text}");
    }

    /// A change that creates a recommendation and chooses a method opens one pop-up, the
    /// recommendation first.
    #[test]
    fn one_change_one_pop_up() {
        let mut app = app();
        // MEDI with mSMV and the prefilter off is discouraged; choosing MEDI with them set
        // creates that and chooses MEDI at once.
        select(&mut app, "qsm_algorithm", "medi", &QSM_PICKER_OPTIONS);
        app.refs.modal = None;
        app.toggle_field("medi_msmv");
        app.toggle_field("medi_msmv_prefilter");
        app.check_recommendations();
        app.rec_modal = None;
        select(&mut app, "qsm_algorithm", "rts", &QSM_PICKER_OPTIONS);
        select(&mut app, "qsm_algorithm", "medi", &QSM_PICKER_OPTIONS);
        assert!(app.rec_modal.is_none());
        let modal = app.refs.modal.as_ref().expect("one pop-up");
        assert_eq!(modal.rows[0].finding.rule, "medi-msmv.prefilter", "the recommendation first");
        assert!(modal.rows[1..].iter().all(|i| i.finding.strength == Strength::Reference));
        screen(&mut app, "MEDI chosen with a recommendation");
    }

    /// Every selector is a picker (nothing is cycled), and a selector whose choice can open a
    /// reference pipeline pop-up is a method picker.
    #[test]
    fn every_selector_is_a_picker() {
        let triggers = rec::reference_trigger_settings();
        let mut app = app();
        app.form.do_swi = true;
        for (_, _, rows) in app.all_pages() {
            for row in &rows {
                if let PipelineRow::AlgoSelect { field, .. } = row {
                    let noun = picker_noun(row);
                    assert!(noun.is_some(), "{field}");
                    if row_path(row).is_some_and(|p| triggers.contains(&p)) {
                        assert_eq!(noun.as_deref(), Some("method"), "{field}");
                    }
                }
            }
        }
        let row = |field: &'static str| PipelineRow::AlgoSelect { label: "  Unwrapping", field, options: &["romeo", "laplacian"], help: &[] };
        assert_eq!(picker_noun(&row("unwrapping_algorithm")).as_deref(), Some("unwrapping"), "two options: a picker too");
        assert_eq!(picker_noun(&row("qsm_algorithm")).as_deref(), Some("method"), "a method, however few options");
    }

    /// ←/→ on a picker row neither cycle it nor open the picker (← goes back to the sections);
    /// Enter opens it, and the row and footer say so.
    #[test]
    fn arrows_on_a_method_row_leave_it_alone() {
        let mut app = app();
        assert!(app.goto_field("qsm_algorithm"));
        let before = app.field_select("qsm_algorithm");
        app.handle_key(KeyEvent::from(KeyCode::Right));
        assert_eq!(app.field_select("qsm_algorithm"), before, "not cycled");
        assert!(app.algo_modal.is_none(), "→ opens nothing");
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.algo_modal.is_some(), "Enter opens the picker");
        app.algo_modal = None;
        select(&mut app, "qsm_algorithm", "medi", &QSM_PICKER_OPTIONS);
        app.refs.modal = None;
        assert!(app.goto_field("qsm_algorithm"));
        let text = screen(&mut app, "A method picker row and its footer");
        assert!(text.contains("[ medi ▾ ]") && text.contains("Enter: choose method") && text.contains("Enter:Choose method"), "{text}");
        app.pipeline_state.unwrapping_algorithm = 1; // laplacian: three solvers, a picker too
        assert!(app.goto_field("laplacian_solver"));
        let text = screen(&mut app, "A short selector row and its footer");
        assert!(!text.contains("◀ ") && text.contains("▾ ]") && text.contains("Enter:Choose") && !text.contains("←→:Change"), "{text}");
        app.handle_key(KeyEvent::from(KeyCode::Left));
        assert!(app.on_subtabs(), "← goes back to the sections");
    }

    /// Every setting a rule covers has a TUI row to carry its badge and reference values.
    #[test]
    fn every_rule_setting_has_a_row() {
        for (core_path, _) in qsmxt_engine::qsm_core::pipeline::rules::SETTINGS {
            let path = rec::qsmxt_path(core_path).unwrap();
            let row = path == "masking.sections" || super::super::app::FIELD_PATHS.iter().any(|(_, p)| *p == path);
            assert!(row, "{core_path} ({path}) has no TUI row");
        }
    }

    /// iLSQR's reference Laplacian solver is a solver and its padding, which has a row of its own.
    #[test]
    fn apply_sets_every_field_a_rules_value_needs() {
        let mut app = app();
        select(&mut app, "unwrapping_algorithm", "laplacian", super::super::app::UNWRAP_OPTIONS);
        app.refs.modal = None;
        for solver in ["dct", "fft", "mirror"] {
            app.set_field_select("laplacian_solver", super::super::app::LAPLACIAN_SOLVER_OPTIONS.iter().position(|o| *o == solver).unwrap());
            let row = PipelineRow::AlgoSelect { label: "", field: "laplacian_solver", options: &[], help: &[] };
            assert_eq!(App::row_marker(&app.rule_view(), &row).map(|m| m.text), Some("iLSQR: FFT, 12-voxel padding".into()), "{solver}");
        }
        app.apply_recommendation("sti.laplacian-solver", None).unwrap();
        assert_eq!(app.pipeline_state.laplacian_solver, 1, "FFT");
        assert_eq!(app.pipeline_state.laplacian_fft_pad, "12");
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.laplacian_fft_pad, [12; 3]);
        let row = PipelineRow::AlgoSelect { label: "", field: "laplacian_solver", options: &[], help: &[] };
        assert_eq!(App::row_marker(&app.rule_view(), &row), None, "the marker goes");
    }

    /// `!` on a marked setting is announced, on the row and in the key bar.
    #[test]
    fn a_marked_row_says_bang_opens_its_advice() {
        let mut app = app();
        select(&mut app, "qsm_algorithm", "qsmart", &QSM_PICKER_OPTIONS);
        app.refs.modal = None;
        app.set_field_select("mask_preset", 0);
        app.check_recommendations();
        assert!(app.goto_field("mask_preset"));
        let text = screen(&mut app, "a marked row");
        assert!(text.contains("(! for advice)") && text.contains("!:Advice"), "{text}");
        assert!(app.goto_field("qsm_algorithm"));
        let text = screen(&mut app, "an unmarked row");
        assert!(!text.contains("(! for advice)") && !text.contains("!:Advice"), "{text}");
    }

    /// `!` ticks by kind: a recommendation's value; an authors' setting is left unticked.
    #[test]
    fn details_preselect_by_kind() {
        let mut app = medi_msmv_without_prefilter();
        app.rec_modal = None;
        app.refs.modal = None;
        assert!(app.goto_field("medi_msmv_prefilter"));
        key(&mut app, KeyCode::Char('!'));
        let modal = app.rec_modal.as_ref().expect("details");
        assert!(modal.rows[0].checked && matches!(modal.rows[0].action, RowAction::Apply { .. }), "a recommendation: the change");
    }

    /// The buttons all show, on any width, and the list scrolls above them with a scrollbar,
    /// following the highlighted row.
    #[test]
    fn long_pop_ups_keep_their_buttons_and_scroll() {
        use ratatui::{backend::TestBackend, Terminal};
        let draw = |app: &mut App, w, h| {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| super::super::ui::draw(f, app)).unwrap();
            let buf = t.backend().buffer();
            (0..buf.area.height).map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>()).collect::<Vec<_>>().join("\n")
        };
        let mut app = app();
        select(&mut app, "qsm_algorithm", "medi", &QSM_PICKER_OPTIONS);
        assert!(app.refs.modal.is_some());
        for (w, h) in [(145, 12), (80, 12), (60, 16)] {
            let text = draw(&mut app, w, h);
            assert!(text.contains("Keep mine") && text.contains("Apply (0)"), "{w}x{h}:\n{text}");
            assert!(text.contains("Esc close"), "the hint stays too, {w}x{h}:\n{text}");
        }
        // Moving down scrolls the list under the buttons, to its end.
        let top = draw(&mut app, 145, 12);
        assert!(top.contains('█') || top.contains('║'), "a scrollbar:\n{top}");
        key(&mut app, KeyCode::End);
        let bottom = draw(&mut app, 145, 12);
        assert_ne!(top, bottom);
        assert!(app.refs.modal.as_ref().unwrap().scroll > 0);
        key(&mut app, KeyCode::Home);
        draw(&mut app, 145, 12);
        assert_eq!(app.refs.modal.as_ref().unwrap().scroll, 0);
        assert!(bottom.contains("Keep mine"));

        // The details pop-up: its choices stay in view however long the text.
        let mut app = medi_msmv_without_prefilter();
        app.rec_modal = None;
        app.refs.modal = None;
        assert!(app.goto_field("medi_msmv_prefilter"));
        key(&mut app, KeyCode::Char('!'));
        let text = draw(&mut app, 80, 10);
        assert!(text.contains("Keep mine"), "{text}");
    }

    /// LSQR+HEIDI in the picker is LSQR with HEIDI on: one representation, and the box and the
    /// picker agree whichever is used.
    #[test]
    fn lsqr_heidi_is_an_alias_for_lsqr_with_the_box_ticked() {
        use super::super::app::{AlgoModal, AlgoModalTarget};
        let at = |name: &str| QSM_PICKER_OPTIONS.iter().position(|o| *o == name).unwrap();
        let pick = |app: &mut App, name: &str| {
            app.algo_modal = Some(AlgoModal {
                target: AlgoModalTarget::Select("qsm_algorithm".into()), title: String::new(),
                options: vec![], help: vec![], cursor: at(name), offset: Default::default(),
            });
            key(app, KeyCode::Enter);
        };
        let lsqr = QSM_ALGO_OPTIONS.iter().position(|o| *o == "lsqr").unwrap();
        let mut app = app();
        pick(&mut app, LSQR_HEIDI);
        assert_eq!((app.pipeline_state.qsm_algorithm, app.pipeline_state.heidi_enabled), (lsqr, true));
        assert_eq!(app.field_select("qsm_algorithm"), at(LSQR_HEIDI), "shown as the alias");
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.inversion.algorithm, qsmxt_engine::QsmAlgorithm::Lsqr);
        assert!(c.inversion.heidi.enabled);
        assert!(app.rec_modal.is_none(), "HEIDI's reference seed: no recommendation");
        app.refs.modal = None; // LSQR's own reference pipeline, as for plain LSQR.

        pick(&mut app, "lsqr");
        assert_eq!((app.pipeline_state.qsm_algorithm, app.pipeline_state.heidi_enabled), (lsqr, false), "plain LSQR means no HEIDI");
        assert_eq!(app.field_select("qsm_algorithm"), at("lsqr"));
        app.toggle_field("heidi_enabled");
        assert_eq!(app.field_select("qsm_algorithm"), at(LSQR_HEIDI), "ticking the box on LSQR is the same choice");

        // A method after it keeps the box, as the box does for any method, and says so.
        pick(&mut app, "medi");
        assert!(app.pipeline_state.heidi_enabled);
        assert_eq!(app.field_select("qsm_algorithm"), at("medi"));
    }
}

