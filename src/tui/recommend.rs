//! The TUI's side of the pipeline rules' findings ([`qsmxt_engine::recommendations`]): the pop-up a
//! change opens when it creates a recommendation, the pop-up choosing a method opens when the
//! config differs from that method's reference pipeline, the marker on a setting that departs
//! from a rule, its details pop-up, and the reference values in the help line. The findings are
//! the engine's, for the config the form builds ([`super::command::config_from_app`]); the TUI
//! holds no rules.
//!
//! - **Two kinds, labelled apart** ([`rec::Kind`]): a reasoned finding (required, recommended,
//!   discouraged) is labelled by its strength ("Recommended") with its reason, and its value is
//!   preselected; an original authors' setting (a `reference` rule: what the method's authors
//!   did, with no reason given that it is better) is labelled "Original authors' setting", and
//!   its value is never preselected: Keep mine is.
//! - **The recommendation pop-up** opens only when a change creates a `required`, `recommended`
//!   or `discouraged` finding that is not dismissed. It gives the reason (the source behind `w`,
//!   "why?"), **Apply** (the recommended value, preselected), the other values the rule allows,
//!   and **Keep mine** (dismiss: kept in the form, and so in the run's config; only a
//!   `recommended` or `discouraged` finding has one). Two equally
//!   strong findings that disagree on a setting are shown together, with nothing preselected.
//!   Esc closes it and leaves the finding open, shown by its marker.
//! - **The setting just changed is respected** ([`RecModal::changed`]): when a change makes a
//!   setting depart from a rule scoped to a flag that is on ([`Recommendation::flag_trigger`]),
//!   such as choosing QSMART with HEIDI ticked, the pop-up preselects turning that flag off, and
//!   changing the setting back is the other choice. It comes before any reference pipeline pop-up.
//! - **The reference pipeline pop-up** ([`RefModal`]) opens when a method with a reference
//!   pipeline is chosen ([`rec::chosen_methods`]) and the config differs from it
//!   ([`rec::reference_differences`]): every difference from the authors' settings, and first
//!   any recommendations the same change created. Its buttons ([`RefButton`]): **Apply
//!   recommended** (preselected when there are recommendations: applies them and keeps every
//!   authors' setting as it is), **Apply all** / **Apply the authors' settings**, **Choose per
//!   setting** (recommendations start on their value, authors' settings on "keep") and **Keep
//!   mine** (closes it: an authors' setting has nothing to dismiss, so nothing is recorded, and
//!   it is offered again only when the method is chosen again). With only authors' settings,
//!   Keep mine is preselected. Nothing changes unless applied. What is applied together follows
//!   [`rec::settle`], as on every host: a reasoned finding outranks an authors' setting, and two
//!   authors' settings that disagree leave the setting as it is, marked, to choose with `!`.
//! - **Pickers** ([`picker_noun`]): every selector is chosen from a picker (Enter), never
//!   cycled, so no method is chosen on the way to another.
//! - **The marker** ([`rec::marker`]) names whose rule a setting departs from and what it uses:
//!   ⚑ for a finding offered as a change, ⓘ for information only. `!` on a marked setting opens
//!   its details: whose rule, what it uses, why, the source, and Apply.
//! - **Apply** sets the form so the config it builds takes the value: each value of the setting's
//!   row is tried until the engine no longer finds the departure.

use crossterm::event::{KeyCode, KeyEvent};
pub use qsmxt_engine::recommendations::{Kind, Strength};
use qsmxt_engine::recommendations::{self as rec, Facts, Marker, Recommendation, Trigger};
use serde_json::Value;

use super::app::{App, PipelineRow};

/// One choice of the pop-up.
#[derive(Debug, Clone, PartialEq)]
pub enum RecChoice {
    /// Set the setting to `value` (the rule's recommended one when `None`), resolving `rule`.
    Apply { rule: String, value: Option<Value>, label: String },
    /// Turn off the flag the finding is scoped to (`setting`, its config path).
    TurnOff { setting: String, label: String },
    /// Keep the setting as it is: dismiss the findings, for its current value.
    Keep,
}

/// The recommendation pop-up, and the details pop-up of a marked setting (`!`).
#[derive(Debug, Clone)]
pub struct RecModal {
    /// What it is about: one finding, or the findings on one setting.
    pub findings: Vec<Recommendation>,
    pub choices: Vec<RecChoice>,
    /// The highlighted choice; `None` until one is picked when the findings disagree.
    pub cursor: Option<usize>,
    /// Show the sources ("why?").
    pub why: bool,
    /// How far the text above the choices is scrolled (PgUp/PgDn), when it does not all fit.
    pub scroll: u16,
    /// The details of a marked setting, opened with `!`: every rule the setting departs from,
    /// `reference` ones included, with their sources.
    pub details: bool,
    /// The finding's setting is the one just changed ([`Self::changed`]).
    pub changed: bool,
}

impl RecModal {
    pub fn new(findings: Vec<Recommendation>) -> Self {
        let conflict = findings.len() > 1;
        let choices = choices(&findings);
        // A suggestion (not preselected) starts on Keep mine.
        let cursor = match conflict {
            true => None,
            false if findings.iter().all(|r| r.strength.preselected()) => Some(0),
            false => choices.iter().position(|c| *c == RecChoice::Keep),
        };
        RecModal { findings, choices, cursor, why: false, scroll: 0, details: false, changed: false }
    }

    /// `findings` are on the setting just changed, and scoped to a flag that is on: turning the
    /// flag off comes first and is preselected, then changing the setting back, then the rest.
    pub fn changed(findings: Vec<Recommendation>) -> Self {
        let mut choices = choices(&findings);
        for c in &mut choices {
            if let RecChoice::Apply { value: None, label, .. } = c {
                let name = &findings[0].recommended.name;
                *label = format!("Use {name} instead");
            }
        }
        let off = turn_off_choices(&findings);
        choices.splice(0..0, off);
        RecModal { findings, choices, cursor: Some(0), why: false, scroll: 0, details: false, changed: true }
    }

    /// The details of a setting: `findings` are the rules it departs from, offered ones first.
    /// Apply is preselected for a recommendation, Keep mine for an authors' setting, and nothing
    /// when recommendations disagree.
    pub fn details(findings: Vec<Recommendation>) -> Self {
        let conflict = findings.iter().any(|r| r.conflict && open(r));
        let reasoned = findings.first().is_some_and(|r| r.kind == Kind::Reasoned && r.strength.preselected());
        let mut choices = choices(&findings);
        // Before Keep mine: the flag's own row may be hidden, as HEIDI's is with QSMART.
        let keep = choices.iter().position(|c| *c == RecChoice::Keep).unwrap_or(choices.len());
        choices.splice(keep..keep, turn_off_choices(&findings));
        let cursor = match (conflict, reasoned) {
            (true, _) => None,
            (false, true) => Some(0),
            (false, false) => choices.iter().position(|c| *c == RecChoice::Keep),
        };
        RecModal { findings, choices, cursor, why: true, scroll: 0, details: true, changed: false }
    }

    pub fn title(&self) -> String {
        let r = &self.findings[0];
        let mut methods: Vec<&str> = Vec::new();
        for f in &self.findings {
            if !methods.contains(&f.trigger.name.as_str()) {
                methods.push(&f.trigger.name);
            }
        }
        match r.strength {
            Strength::Reference if methods.len() == 1 => format!(" {}: {} ", Kind::Reference.label(), rec::reference_title(&r.trigger)),
            Strength::Reference => format!(" {}s: {} of {} ", Kind::Reference.label(), r.label, methods.join(", ")),
            _ if self.details && self.findings.len() > 1 => format!(" {} ", r.label),
            Strength::Required if self.changed => format!(" {} requires a change ", r.current_text()),
            Strength::Required => format!(" {} requires a change ", r.trigger.name),
            Strength::Discouraged => format!(" Discouraged with {} ", r.trigger.name),
            Strength::Suggested if self.findings.len() == 1 => format!(" {} suggests a change ", r.trigger.name),
            _ if self.findings.len() > 1 => " Recommendations disagree ".to_string(),
            _ => format!(" {} recommends a change ", r.trigger.name),
        }
    }

    pub fn choice_label(c: &RecChoice) -> &str {
        match c {
            RecChoice::Apply { label, .. } | RecChoice::TurnOff { label, .. } => label,
            RecChoice::Keep => "Keep mine",
        }
    }
}

/// The choices for `findings` on one setting: each value they recommend or allow once, with the
/// methods whose rule has it when there are several, then Keep mine unless one is `required`.
fn choices(findings: &[Recommendation]) -> Vec<RecChoice> {
    let mut methods_of: Vec<(Value, String, Vec<String>, String, bool)> = Vec::new();
    for r in findings {
        let values = std::iter::once((&r.recommended, true)).chain(r.alternatives().into_iter().map(|a| (a, false)));
        for (v, recommended) in values {
            match methods_of.iter_mut().find(|(value, ..)| *value == v.value) {
                Some((.., who, _, _)) if who.contains(&r.trigger.name) => {}
                Some((.., who, _, _)) => who.push(r.trigger.name.clone()),
                None => methods_of.push((v.value.clone(), v.name.clone(), vec![r.trigger.name.clone()], r.rule.clone(), recommended)),
            }
        }
    }
    let several = findings.len() > 1;
    // An authors' setting is offered as theirs, not as advice.
    let authors = findings.iter().all(|r| r.kind == Kind::Reference);
    let mut out: Vec<RecChoice> = methods_of.into_iter().map(|(value, name, who, rule, recommended)| {
        let who = if several { format!(" ({})", who.join(", ")) } else { String::new() };
        let (value, label) = match (recommended, authors) {
            (true, false) => (None, format!("Apply: {name}{who}")),
            (true, true) => (None, format!("Use the authors' setting: {name}{who}")),
            (false, _) => (Some(value), format!("Use {name} instead{who}")),
        };
        RecChoice::Apply { rule, value, label }
    }).collect();
    // A required one cannot be dismissed: the method cannot run otherwise.
    if findings.iter().all(|r| r.strength != Strength::Required) {
        out.push(RecChoice::Keep);
    }
    out
}

/// "Turn off HEIDI streaking reduction", once for each flag `findings` are scoped to that has a
/// row to turn off.
fn turn_off_choices(findings: &[Recommendation]) -> Vec<RecChoice> {
    let mut out: Vec<RecChoice> = Vec::new();
    for r in findings {
        let Some(setting) = r.flag_trigger() else { continue };
        let choice = RecChoice::TurnOff { setting: setting.to_string(), label: format!("Turn off {}", r.flag_name()) };
        if super::app::field_of_path(setting).is_some() && !out.contains(&choice) {
            out.push(choice);
        }
    }
    out
}

/// One setting of the reference pipeline pop-up, and what is picked for it.
#[derive(Debug, Clone, PartialEq)]
pub struct RefItem {
    pub finding: Recommendation,
    /// The other methods' rules that want the same value of the same setting: decided with it.
    pub also: Vec<Recommendation>,
    /// The values to choose from: the rule's (recommended first), then `None`, keeping the
    /// current value.
    pub options: Vec<Option<(Option<Value>, String)>>,
    pub pick: usize,
}

impl RefItem {
    /// A recommendation starts on its value; an authors' setting on keeping the current value.
    fn new(finding: Recommendation) -> Self {
        let mut options = vec![Some((None, finding.recommended.name.clone()))];
        options.extend(finding.alternatives().into_iter().map(|a| Some((Some(a.value.clone()), a.name.clone()))));
        options.push(None);
        let pick = if finding.kind == Kind::Reasoned && finding.strength.preselected() { 0 } else { options.len() - 1 };
        RefItem { finding, also: vec![], options, pick }
    }

    /// The pick for a button: the recommended value, or keeping the current one.
    fn take(&mut self, apply: bool) {
        self.pick = if apply { 0 } else { self.options.len() - 1 };
    }

    /// The item's findings as picked, each wanting the picked value: none when keeping the
    /// current one.
    fn picked(&self) -> Vec<Recommendation> {
        let Some((value, _)) = &self.options[self.pick] else { return vec![] };
        std::iter::once(&self.finding).chain(&self.also).cloned().map(|mut r| {
            if let Some(v) = value {
                r.recommended.value = v.clone();
            }
            r
        }).collect()
    }

    /// What is picked, as the per-setting view says it: "PDF", "keep V-SHARP".
    pub fn pick_text(&self) -> String {
        match &self.options[self.pick] {
            Some((_, name)) => name.clone(),
            None => format!("keep {}", self.finding.current_text()),
        }
    }

    /// The methods whose rules the item is: "MEDI, PDF".
    pub fn methods(&self) -> String {
        std::iter::once(&self.finding).chain(&self.also).map(|r| r.trigger.name.as_str()).collect::<Vec<_>>().join(", ")
    }
}

/// A button of the reference pipeline pop-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefButton {
    /// Apply the recommendations; keep every authors' setting as it is.
    ApplyRecommended,
    /// Apply everything, the authors' settings too.
    ApplyAll,
    ChoosePerSetting,
    KeepMine,
}

/// The reference pipeline pop-up: where the config differs from the reference pipelines of the
/// methods a change chose, led by the recommendations the same change created.
#[derive(Debug, Clone)]
pub struct RefModal {
    /// The methods chosen, the first the one the title names when there is one.
    pub methods: Vec<Trigger>,
    /// The recommendations the change created (`recommended` and `discouraged` ones), then the
    /// differences from the reference pipelines, one per setting and value.
    pub items: Vec<RefItem>,
    /// The buttons, for what the pop-up holds ([`RefModal::new`]).
    pub buttons: Vec<RefButton>,
    /// The highlighted button: the first ("Apply recommended") when there are recommendations,
    /// Keep mine when it holds only authors' settings, whose values are never preselected.
    pub cursor: Option<usize>,
    /// Choosing per setting: the highlighted setting.
    pub row: Option<usize>,
    /// How far the list above the buttons is scrolled, when it does not all fit.
    pub scroll: u16,
}

impl RefModal {
    pub fn new(methods: Vec<Trigger>, findings: Vec<Recommendation>) -> Self {
        let mut items: Vec<RefItem> = Vec::new();
        for r in findings {
            let same = |i: &&mut RefItem| i.finding.setting == r.setting && i.finding.recommended == r.recommended
                && i.finding.strength == Strength::Reference && r.strength == Strength::Reference;
            match items.iter_mut().find(|i| same(i)) {
                Some(item) => item.also.push(r),
                None => items.push(RefItem::new(r)),
            }
        }
        let reasoned = items.iter().any(|i| i.finding.kind == Kind::Reasoned);
        let authors = items.iter().any(|i| i.finding.kind == Kind::Reference);
        let mut buttons = Vec::new();
        if reasoned {
            buttons.push(RefButton::ApplyRecommended);
        }
        if authors {
            buttons.push(RefButton::ApplyAll);
        }
        buttons.extend([RefButton::ChoosePerSetting, RefButton::KeepMine]);
        let cursor = if reasoned { 0 } else { buttons.len() - 1 };
        RefModal { methods, items, buttons, cursor: Some(cursor), row: None, scroll: 0 }
    }

    /// A button's label, for what the pop-up holds.
    pub fn button_label(&self, b: RefButton) -> &'static str {
        let reasoned = self.buttons.contains(&RefButton::ApplyRecommended);
        let authors = self.buttons.contains(&RefButton::ApplyAll);
        match b {
            RefButton::ApplyRecommended if authors => "Apply recommended",
            RefButton::ApplyRecommended => "Apply",
            RefButton::ApplyAll if reasoned => "Apply all, with the authors' settings",
            RefButton::ApplyAll => "Apply the authors' settings",
            RefButton::ChoosePerSetting => "Choose per setting",
            RefButton::KeepMine => "Keep mine",
        }
    }

    pub fn title(&self) -> String {
        format!(" {} ", self.whose())
    }

    /// "MEDI's reference pipeline", or "MEDI's and PDF's reference pipelines".
    pub fn whose(&self) -> String {
        match self.methods.as_slice() {
            [one] => rec::reference_title(one),
            many => format!("{}'s reference pipelines", many.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join("'s and ")),
        }
    }

    /// The pop-up says which method each difference is from when it covers several.
    pub fn several(&self) -> bool {
        self.methods.len() > 1
    }
}

/// What the reference pipeline pop-ups are worked out from.
#[derive(Debug, Clone, Default)]
pub struct RefState {
    pub modal: Option<RefModal>,
    /// The choices with a reference pipeline at the last check ([`rec::chosen_methods`]).
    pub methods: Vec<Trigger>,
    /// Methods just chosen whose pop-up has not been shown, each with the settings it leaves
    /// out (those the pop-up that chose it had just decided).
    pub pending: Vec<(Trigger, Vec<String>)>,
}

/// A pop-up's scroll position after a scrolling key. Drawing clamps it to what the text needs.
fn scrolled(scroll: u16, key: KeyCode) -> u16 {
    const PAGE: u16 = 10;
    match key {
        KeyCode::Up | KeyCode::Char('k') => scroll.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => scroll.saturating_add(1),
        KeyCode::PageUp => scroll.saturating_sub(PAGE),
        KeyCode::PageDown => scroll.saturating_add(PAGE),
        KeyCode::Home => 0,
        KeyCode::End => u16::MAX,
        _ => scroll,
    }
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
                    .into_iter().filter(|r| !skip.contains(&r.setting)).collect();
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
                self.refs.modal = Some(RefModal::new(methods, diffs));
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
                self.refs.modal = Some(RefModal::new(methods, items));
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
        let Some(modal) = self.rec_modal.as_mut() else { return };
        let n = modal.choices.len();
        match key.code {
            KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End => modal.scroll = scrolled(modal.scroll, key.code),
            KeyCode::Esc => self.rec_modal = None,
            KeyCode::Char('w') | KeyCode::Char('?') => modal.why = !modal.why,
            KeyCode::Up | KeyCode::Left | KeyCode::Char('k') => {
                modal.cursor = Some(modal.cursor.map_or(n - 1, |c| (c + n - 1) % n));
            }
            KeyCode::Down | KeyCode::Right | KeyCode::Char('j') | KeyCode::Tab => {
                modal.cursor = Some(modal.cursor.map_or(0, |c| (c + 1) % n));
            }
            KeyCode::Enter => {
                let Some(choice) = modal.cursor.map(|c| modal.choices[c].clone()) else { return };
                let findings = modal.findings.clone();
                self.rec_modal = None;
                match choice {
                    RecChoice::Apply { rule, value, .. } => {
                        if let Err(e) = self.apply_recommendation(&rule, value.as_ref()) {
                            self.error_message = Some(e);
                        }
                    }
                    RecChoice::TurnOff { setting, .. } => {
                        if let Some(field) = super::app::field_of_path(&setting) {
                            self.set_companion(field, &Value::Bool(false));
                        }
                    }
                    RecChoice::Keep => self.keep_mine(&findings),
                }
                // A method the choice chose is offered without the setting just decided.
                let skip: Vec<String> = findings.iter().map(|r| r.setting.clone()).collect();
                let view = self.rule_view();
                self.note_methods(&view, &skip);
                // Whatever the choice created (a value another rule departs from) opens next.
                self.check_recommendations();
            }
            _ => {}
        }
    }

    /// Keep the settings of `findings` as they are: each `recommended` or `discouraged` one
    /// dismissed for its current value, in the form and so in the run's config ([`rec::keep`]). A
    /// `reference` one records nothing, and a `required` one cannot be kept.
    fn keep_mine(&mut self, findings: &[Recommendation]) {
        rec::keep(&mut self.pipeline_state.dismissed, findings);
    }

    pub fn handle_ref_modal_key(&mut self, key: KeyEvent) {
        let Some(modal) = self.refs.modal.as_mut() else { return };
        let per_setting = modal.row.is_some();
        let n = modal.buttons.len();
        match (key.code, modal.row) {
            (KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End, _)
            | (KeyCode::Up | KeyCode::Down | KeyCode::Char('k') | KeyCode::Char('j'), None) => modal.scroll = scrolled(modal.scroll, key.code),
            (KeyCode::Esc, Some(_)) => modal.row = None,
            (KeyCode::Esc, None) => self.refs.modal = None,
            (KeyCode::Left | KeyCode::Char('h'), None) => modal.cursor = Some(modal.cursor.map_or(n - 1, |c| (c + n - 1) % n)),
            (KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab, None) => modal.cursor = Some(modal.cursor.map_or(0, |c| (c + 1) % n)),
            (KeyCode::Up | KeyCode::Char('k'), Some(i)) => modal.row = Some((i + modal.items.len() - 1) % modal.items.len()),
            (KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab, Some(i)) => modal.row = Some((i + 1) % modal.items.len()),
            (KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') | KeyCode::Char(' '), Some(i)) => {
                let item = &mut modal.items[i];
                let n = item.options.len();
                item.pick = if key.code == KeyCode::Left || key.code == KeyCode::Char('h') { (item.pick + n - 1) % n } else { (item.pick + 1) % n };
            }
            // Nothing preselected: Enter waits for a choice.
            (KeyCode::Enter, None) if modal.cursor.is_none() => {}
            (KeyCode::Enter, None) if modal.cursor.map(|c| modal.buttons[c]) == Some(RefButton::ChoosePerSetting) => modal.row = Some(0),
            (KeyCode::Enter, _) => {
                let mut items = modal.items.clone();
                if !per_setting {
                    let button = modal.cursor.map(|c| modal.buttons[c]);
                    for item in &mut items {
                        item.take(match button {
                            Some(RefButton::ApplyRecommended) => item.finding.kind == Kind::Reasoned && item.finding.strength.preselected(),
                            Some(RefButton::ApplyAll) => true,
                            _ => false,
                        });
                    }
                }
                self.refs.modal = None;
                self.decide(&items);
            }
            _ => {}
        }
    }

    /// Carry out the reference pipeline pop-up's choices: the picks [`rec::settle`] takes are
    /// applied, the items kept are kept, and a pick it leaves changes nothing and stays marked. A
    /// method an applied value chooses is offered without the settings just decided.
    fn decide(&mut self, items: &[RefItem]) {
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
            match &item.options[item.pick] {
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
            let (finding, Some((value, _))) = (&item.finding, &item.options[item.pick]) else { continue };
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
        self.keep_mine(&kept);
        let kept_items = items.iter().filter(|i| i.options[i.pick].is_none()).count();
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
        // orientation, once the mask is BET) is offered next, as the pop-up did not show it.
        let presented: Vec<&str> = items.iter().map(|i| i.finding.rule.as_str()).collect();
        for m in rec::by_method(&view) {
            if !items.iter().any(|i| i.finding.trigger == m.method) || self.refs.pending.iter().any(|(p, _)| *p == m.method) {
                continue;
            }
            let new: Vec<String> = m.differences().into_iter()
                .filter(|r| !presented.contains(&r.rule.as_str())).map(|r| r.setting).collect();
            if !new.is_empty() {
                let skip = skip.iter().filter(|s| !new.contains(s)).cloned().collect();
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
    /// as its reference, marked where the current value differs.
    pub fn row_reference(view: &[Recommendation], row: &PipelineRow) -> Option<String> {
        let path = row_path(row)?;
        // One entry for what several rules say alike: each HEIDI seed rule recommends LSQR.
        let mut parts: Vec<(String, Vec<&str>)> = Vec::new();
        for r in view.iter().filter(|r| r.setting == path) {
            let what = match (r.departs, r.strength, r.dismissed) {
                (false, Strength::Reference, _) => "Authors' setting",
                (false, _, _) => "Recommended",
                (true, Strength::Reference, _) => "Differs from the authors' setting",
                (true, _, true) => "Dismissed",
                (true, Strength::Required, _) => "Required",
                (true, Strength::Discouraged, _) => "Discouraged",
                (true, Strength::Recommended, _) => "Recommended",
                (true, Strength::Suggested, _) => "Suggested",
            };
            let value = if r.strength == Strength::Discouraged && r.departs {
                format!("not {}", r.current_text())
            } else {
                r.recommended.name.clone()
            };
            let head = format!("{what}: {value} ({}", r.trigger.name);
            match parts.iter_mut().find(|(h, _)| *h == head) {
                Some((_, sources)) if sources.contains(&r.source.as_str()) => {}
                Some((_, sources)) => sources.push(&r.source),
                None => parts.push((head, vec![&r.source])),
            }
        }
        let parts: Vec<String> = parts.into_iter().map(|(head, sources)| format!("{head}; {})", sources.join("; "))).collect();
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

    fn enter(app: &mut App, choice: &RecChoice) {
        let modal = app.rec_modal.as_ref().unwrap();
        let i = modal.choices.iter().position(|c| c == choice).unwrap();
        app.rec_modal.as_mut().unwrap().cursor = Some(i);
        app.handle_rec_modal_key(KeyEvent::from(KeyCode::Enter));
    }

    #[test]
    fn a_change_that_creates_a_recommendation_opens_the_modal_and_apply_resolves_it() {
        let mut app = medi_msmv_without_prefilter();
        let modal = app.rec_modal.as_ref().expect("the change opened the pop-up");
        assert_eq!(modal.findings[0].rule, "medi-msmv.prefilter");
        assert_eq!(modal.cursor, Some(0), "Apply is preselected");
        assert!(modal.choices.contains(&RecChoice::Keep));
        app.handle_rec_modal_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.rec_modal.is_none());
        assert!(app.field_toggle("medi_msmv_prefilter"), "applied through the form");
        let config = super::super::command::config_from_app(&app).unwrap();
        assert!(config.inversion.medi.msmv_prefilter);
        assert!(!app.rule_view().iter().any(|r| r.rule == "medi-msmv.prefilter" && r.departs));
    }

    /// A suggestion (not preselected) opens on Keep mine, and a reference pop-up item starts on
    /// keeping the current value; iHARPERELLA's whole rim (a recommendation) starts on Apply.
    #[test]
    fn a_suggestion_starts_on_keep_mine() {
        let mut a = app();
        select(&mut a, "qsm_algorithm", "ndi", &QSM_PICKER_OPTIONS);
        let ndi: Vec<_> = a.rule_view().into_iter().filter(|r| r.rule == "ndi.weight-norm-in-vivo" && r.departs).collect();
        assert_eq!(ndi.len(), 1, "{ndi:?}");
        let modal = RecModal::new(ndi.clone());
        assert_eq!(modal.cursor, modal.choices.iter().position(|c| *c == RecChoice::Keep));
        let item = RefItem::new(ndi[0].clone());
        assert_eq!(item.pick, item.options.len() - 1);
        let mut b = app();
        b.pipeline_state.set_select("bf_algorithm", super::super::app::BF_OPTIONS.iter().position(|o| *o == "iharperella").unwrap());
        let rim: Vec<_> = b.rule_view().into_iter().filter(|r| r.rule == "iharperella.whole-rim" && r.departs).collect();
        assert_eq!(rim.len(), 1);
        assert_eq!(RecModal::new(rim).cursor, Some(0));
    }

    #[test]
    fn ignore_dismisses_until_the_setting_changes_and_esc_leaves_a_badge() {
        let mut app = medi_msmv_without_prefilter();
        // Esc: no change, the finding stays open with its badge; `!` brings the pop-up back.
        app.handle_rec_modal_key(KeyEvent::from(KeyCode::Esc));
        let row = PipelineRow::Toggle { label: "", field: "medi_msmv_prefilter", help: "" };
        assert!(App::row_marker(&app.rule_view(), &row).is_some_and(|m| m.actionable && m.text == "MEDI: not off"));
        assert!(App::row_reference(&app.rule_view(), &row).is_some_and(|t| t.starts_with("Discouraged: not off") && t.ends_with("! for details")));
        app.check_recommendations();
        assert!(app.rec_modal.is_none(), "not opened again by a change that creates nothing");
        app.rec_modal = Some(RecModal::new(app.rule_view().into_iter().filter(|r| r.rule == "medi-msmv.prefilter").collect()));
        enter(&mut app, &RecChoice::Keep);
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

    /// Ticking HEIDI after any other inversion opens the pop-up recommending LSQR, Apply
    /// preselected; nothing changes until it is applied. After LSQR nothing opens.
    #[test]
    fn ticking_heidi_recommends_lsqr() {
        let lsqr = QSM_ALGO_OPTIONS.iter().position(|o| *o == "lsqr").unwrap();
        for (alg, rule) in [("rts", "heidi.seed-rts"), ("tikhonov", "heidi.seed-lsqr"), ("medi", "heidi.seed-regularised")] {
            let mut app = app();
            select(&mut app, "qsm_algorithm", alg, &QSM_PICKER_OPTIONS);
            // The method's own reference pipeline pop-up, if any: closed, nothing changes.
            app.handle_ref_modal_key(KeyEvent::from(KeyCode::Esc));
            app.rec_modal = None;
            app.check_recommendations();
            let chosen = app.pipeline_state.qsm_algorithm;
            app.toggle_field("heidi_enabled");
            app.check_recommendations();
            let modal = app.rec_modal.as_ref().unwrap_or_else(|| panic!("{alg}: ticking HEIDI opened no pop-up"));
            assert_eq!(modal.findings[0].rule, rule, "{alg}");
            assert_eq!(modal.findings[0].recommended.value, serde_json::json!("lsqr"), "{alg}");
            assert_eq!(modal.cursor, Some(0), "{alg}: Apply is preselected");
            assert_eq!(app.pipeline_state.qsm_algorithm, chosen, "{alg}: nothing changes before Apply");
            assert!(app.pipeline_state.heidi_enabled);
            app.handle_rec_modal_key(KeyEvent::from(KeyCode::Enter));
            assert_eq!(app.pipeline_state.qsm_algorithm, lsqr, "{alg}: Apply chooses LSQR");
            assert!(app.pipeline_state.heidi_enabled, "{alg}: HEIDI stays ticked");
            let config = super::super::command::config_from_app(&app).unwrap();
            assert_eq!((config.inversion.algorithm, config.inversion.heidi.enabled), (crate::pipeline::config::QsmAlgorithm::Lsqr, true));
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
        let finding = &modal.items.iter().find(|i| i.finding.rule == "decompose.qsm-per-echo").expect("per-echo QSM is offered").finding;
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
            .or_else(|| app.refs.modal.take().map(|m| m.items.into_iter().map(|i| i.finding).collect()))
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
        assert_eq!(RecModal::choice_label(&modal.choices[0]), "Apply: From the echo spacing");
        assert_eq!(modal.choices.last(), Some(&RecChoice::Keep));
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
        assert!(text.starts_with("Authors' setting: V-SHARP (RTS; QSM.jl README.md"), "{text}");
        select(&mut app, "bf_algorithm", "pdf", super::super::app::BF_OPTIONS);
        assert!(app.rec_modal.is_none(), "information only");
        assert!(app.refs.modal.as_ref().is_none_or(|m| m.methods[0].name == "PDF"), "only a method just chosen opens its reference");
        assert_eq!(App::row_marker(&app.rule_view(), &row).map(|m| (m.actionable, m.text)), Some((false, "RTS: V-SHARP".into())));
        assert!(App::row_reference(&app.rule_view(), &row).unwrap().starts_with("Differs from the authors' setting: V-SHARP"));
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
        modal.items.iter().map(|i| i.finding.setting.as_str()).collect()
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
        let mut modal = RefModal::new(vec![found[0].trigger.clone()], found);
        modal.items.iter_mut().for_each(|i| i.take(true));
        (app, modal.items)
    }

    /// Apply all leaves a setting the authors disagree on as it is, says so, and records nothing;
    /// the other settings are applied.
    #[test]
    fn apply_all_leaves_authors_settings_that_disagree() {
        let (mut app, items) = disagreeing_bg_removal();
        let bg: Vec<&str> = items.iter().filter(|i| i.finding.setting == "bg_removal.algorithm").map(|i| i.finding.rule.as_str()).collect();
        assert!(bg.len() >= 2, "{bg:?}");
        assert!(items.iter().any(|i| i.finding.setting == "masking.sections"));
        app.decide(&items);
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
        items[first].take(false);
        app.decide(&items);
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
        let mut modal = RefModal::new(vec![found[0].trigger.clone()], found);
        modal.items.reverse();
        modal.items.iter_mut().for_each(|i| i.take(true));
        let items = modal.items;
        app.decide(&items);
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.unwrapping_algorithm, qsmxt_engine::enums::UnwrappingAlgorithm::Romeo);
        assert!(app.status_message.as_deref().unwrap().contains("a stronger suggestion"));
    }

    #[test]
    fn choosing_medi_opens_its_reference_pipeline_and_apply_all_takes_it() {
        let mut app = medi_chosen();
        let modal = app.refs.modal.as_ref().expect("MEDI's reference pipeline pop-up");
        assert_eq!(modal.title(), " MEDI's reference pipeline ");
        // Only the authors' settings: nothing preselected, so Enter alone changes nothing.
        assert_eq!(modal.buttons[0], RefButton::ApplyAll);
        assert_eq!(modal.cursor.map(|c| modal.buttons[c]), Some(RefButton::KeepMine), "Keep mine is preselected");
        assert!(modal.items.iter().all(|i| i.finding.kind == Kind::Reference && i.options[i.pick].is_none()));
        let s = settings(modal);
        assert!(s.contains(&"bg_removal.algorithm") && s.contains(&"masking.sections"), "{s:?}");
        let text = screen(&mut app, "MEDI chosen");
        assert!(text.contains("V-SHARP → PDF") && text.contains("threshold → BET") && text.contains("README.m:124"), "{text}");
        assert!(text.contains("Original authors' settings") && text.contains("none is applied unless you choose it"), "{text}");
        assert!(text.contains("Apply the authors' settings") && text.contains("Choose per setting") && text.contains("Keep mine"));
        // Apply the authors' settings, chosen.
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Enter);
        // The complex fit is now chosen, and PDF: one pop-up, PDF's measured suggestion of 300 CG
        // iterations first (preselected), then the complex fit's authors' unwrapper (not).
        let modal = app.refs.modal.as_ref().expect("the complex fit's authors' setting, with PDF's suggestion");
        assert_eq!(modal.title(), " Complex fit's and MEDI's reference pipelines ");
        let rules: Vec<&str> = modal.items.iter().map(|i| i.finding.rule.as_str()).collect();
        // MEDI's BET orientation came into play with the BET mask just applied: offered with them.
        assert_eq!(rules, ["pdf.cg-iterations", "complex-fit.unwrap-region-growing", "medi.bet-as-stored"]);
        assert_eq!((modal.cursor, modal.buttons[0]), (Some(0), RefButton::ApplyRecommended));
        let text = screen(&mut app, "MEDI's settings applied: PDF's suggestion and the complex fit's unwrapper");
        assert!(text.contains("Recommended, each for the reason given") && text.contains("Apply recommended"), "{text}");
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::enums::BfAlgorithm::Pdf);
        assert_eq!(c.bg_removal.pdf.n_cg, qsm_core::bgremove::PdfParams::default().n_cg, "nothing rewritten yet");
        key(&mut app, KeyCode::Enter);
        assert!(app.refs.modal.is_none() && app.rec_modal.is_none());
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.pdf.n_cg, 300, "Apply recommended (preselected) takes the suggestion");
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
        let m = app.refs.modal.as_mut().unwrap();
        m.cursor = m.buttons.iter().position(|b| *b == RefButton::KeepMine);
        key(&mut app, KeyCode::Enter);
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
        assert!(app.refs.modal.as_ref().is_some_and(|m| m.methods[0].name == "MEDI"));
    }

    #[test]
    fn choosing_per_setting_applies_some_and_keeps_the_rest() {
        let mut app = medi_chosen();
        let m = app.refs.modal.as_mut().unwrap();
        m.cursor = m.buttons.iter().position(|b| *b == RefButton::ChoosePerSetting);
        key(&mut app, KeyCode::Enter);
        let modal = app.refs.modal.as_ref().unwrap();
        assert_eq!(modal.row, Some(0));
        let mask = settings(modal).iter().position(|s| *s == "masking.sections").unwrap();
        let bg = settings(modal).iter().position(|s| *s == "bg_removal.algorithm").unwrap();
        // The authors' settings start on "keep", none preselected.
        assert!(modal.items.iter().all(|i| i.finding.kind == Kind::Reasoned || i.options[i.pick].is_none()));
        // Keep the mask (its last choice); take the background removal.
        let keep = modal.items[mask].options.len() - 1;
        app.refs.modal.as_mut().unwrap().items[mask].pick = keep;
        app.refs.modal.as_mut().unwrap().items[bg].pick = 0;
        app.refs.modal.as_mut().unwrap().row = Some(mask);
        screen(&mut app, "MEDI, choosing per setting");
        key(&mut app, KeyCode::Enter);
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::enums::BfAlgorithm::Pdf, "item {bg} applied");
        assert!(!c.masking.sections.iter().any(|s| matches!(s.generator, qsmxt_engine::masking::MaskOp::Bet { .. })), "the mask kept");
        assert!(c.dismissed_recommendations.is_empty(), "keeping a reference difference records nothing");
        assert!(app.refs.modal.is_none(), "PDF's reference wants the mask just kept: not asked again");
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
            assert_eq!(modal.cursor, Some(0), "{method}");
            assert_eq!(RecModal::choice_label(&modal.choices[0]), "Turn off HEIDI streaking reduction", "{method}");
            assert_eq!(RecModal::choice_label(&modal.choices[1]), "Use LSQR instead", "{method}");
            assert_eq!(modal.choices.len(), 2, "{method}: a required finding has no Keep mine");

            key(&mut app, KeyCode::Enter);
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
        assert_eq!(RecModal::choice_label(&modal.choices[0]), "Turn off HEIDI streaking reduction");
        assert!(modal.choices.contains(&RecChoice::Keep));
        key(&mut app, KeyCode::Enter);
        assert!(!app.pipeline_state.heidi_enabled);
        assert_eq!(app.pipeline_state.qsm_algorithm, medi);
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
        assert!(modal.choices.iter().any(|c| matches!(c, RecChoice::TurnOff { .. })), "{:?}", modal.choices);
    }

    /// Rules that say the same thing on a setting read as one entry, naming each source once.
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
        assert_eq!(text.matches("Recommended: LSQR").count(), 1, "{text}");
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
        assert!(text.contains("ⓘ QSMART: BET"), "{text}");
        key(&mut app, KeyCode::Char('!'));
        let modal = app.rec_modal.as_ref().expect("details");
        assert!(modal.details);
        assert_eq!(modal.title(), " Original authors' setting: QSMART's reference pipeline ");
        assert_eq!(modal.choices[0], RecChoice::Apply { rule: "qsmart.mask-bet".into(), value: None, label: "Use the authors' setting: BET (QSMART)".into() });
        assert_eq!(modal.cursor.map(|c| &modal.choices[c]), Some(&RecChoice::Keep), "an authors' setting: Keep mine is preselected");
        let text = screen(&mut app, "QSMART mask details (!)");
        assert!(text.contains("Original authors' setting (not a recommendation)")
            && text.contains("QSMART's reference pipeline uses BET") && text.contains("Source"), "{text}");
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        // BET now; the recipe's other parts (QSMART's first echo, f 0.2, no erosion) are their
        // own rules, still marked.
        assert!(App::row_marker(&app.rule_view(), &row).is_none_or(|m| !m.text.contains("BET")));
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
        assert_eq!(modal.items[0].finding.rule, "medi-msmv.prefilter", "the recommendation first");
        assert!(modal.items[1..].iter().all(|i| i.finding.strength == Strength::Reference));
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

    /// `!` preselects by kind: a recommendation's value, Keep mine for an authors' setting.
    #[test]
    fn details_preselect_by_kind() {
        let mut app = medi_msmv_without_prefilter();
        app.rec_modal = None;
        app.refs.modal = None;
        assert!(app.goto_field("medi_msmv_prefilter"));
        key(&mut app, KeyCode::Char('!'));
        let modal = app.rec_modal.as_ref().expect("details");
        assert!(matches!(modal.cursor.map(|c| &modal.choices[c]), Some(RecChoice::Apply { .. })), "a recommendation: the change");
    }

    /// The buttons all show, on any width, and the text scrolls above them with a scrollbar.
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
            assert!(text.contains("Keep mine") && text.contains("Choose per setting"), "{w}x{h}:\n{text}");
            assert!(text.contains("Esc close"), "the hint stays too, {w}x{h}:\n{text}");
        }
        // Scrolling moves the text under the buttons, and stops at its end.
        let top = draw(&mut app, 145, 12);
        assert!(top.contains('█') || top.contains('║'), "a scrollbar:\n{top}");
        key(&mut app, KeyCode::End);
        let bottom = draw(&mut app, 145, 12);
        assert_ne!(top, bottom);
        let at_end = app.refs.modal.as_ref().unwrap().scroll;
        assert!(at_end > 0 && at_end < u16::MAX, "drawing clamps it: {at_end}");
        key(&mut app, KeyCode::Up);
        assert_eq!(app.refs.modal.as_ref().unwrap().scroll, at_end - 1);
        assert!(bottom.contains("Keep mine"));

        // The details pop-up: its choices stay in view however long the text.
        let mut app = medi_msmv_without_prefilter();
        app.rec_modal = None;
        app.refs.modal = None;
        assert!(app.goto_field("medi_msmv_prefilter"));
        key(&mut app, KeyCode::Char('!'));
        let text = draw(&mut app, 80, 10);
        assert!(text.contains("Keep mine"), "{text}");
        key(&mut app, KeyCode::PageDown);
        assert!(app.rec_modal.as_ref().unwrap().scroll > 0 || !text.contains("PgUp/PgDn"));
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
                options: vec![], help: vec![], cursor: at(name),
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
