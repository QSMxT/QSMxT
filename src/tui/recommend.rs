//! The TUI's side of the pipeline rules' findings ([`qsmxt_engine::recommendations`]): the pop-up a
//! change opens when it creates a recommendation, the pop-up choosing a method opens when the
//! config differs from that method's reference pipeline, the marker on a setting that departs
//! from a rule, its details pop-up, and the reference values in the help line. The findings are
//! the engine's, for the config the form builds ([`super::command::config_from_app`]); the TUI
//! holds no rules.
//!
//! - **The recommendation pop-up** opens only when a change creates a `required`, `recommended`
//!   or `discouraged` finding that is not dismissed. It gives the reason (the source behind `w`,
//!   "why?"), **Apply** (the recommended value, preselected), the other values the rule allows,
//!   and **Keep mine** (dismiss: kept in the form, and so in the run's config; only a
//!   `recommended` or `discouraged` finding has one). Two equally
//!   strong findings that disagree on a setting are shown together, with nothing preselected.
//!   Esc closes it and leaves the finding open, shown by its marker.
//! - **The reference pipeline pop-up** ([`RefModal`]) opens when a method with a reference
//!   pipeline is chosen ([`rec::chosen_methods`]) and the config differs from it
//!   ([`rec::reference_differences`]): every difference, with **Apply all**, **Choose per
//!   setting** and **Keep mine** (closes it: a reference difference is information only, so
//!   nothing is recorded, and it is offered again only when the method is chosen again). Nothing
//!   changes unless applied. When the same change
//!   also creates recommendations, they lead the same pop-up.
//! - **Pickers** ([`picker_noun`]): a selector whose choice can open that pop-up, or that has
//!   more than three options, is chosen from a picker (Enter, or ←/→), never cycled, so no
//!   method is chosen on the way to another.
//! - **The marker** ([`rec::marker`]) names whose rule a setting departs from and what it uses:
//!   ⚑ for a finding offered as a change, ⓘ for information only. `!` on a marked setting opens
//!   its details: whose rule, what it uses, why, the source, and Apply.
//! - **Apply** sets the form so the config it builds takes the value: each value of the setting's
//!   row is tried until the engine no longer finds the departure.

use crossterm::event::{KeyCode, KeyEvent};
pub use qsmxt_engine::recommendations::Strength;
use qsmxt_engine::recommendations::{self as rec, Facts, Marker, Recommendation, Trigger};
use serde_json::Value;

use super::app::{App, PipelineRow};

/// One choice of the pop-up.
#[derive(Debug, Clone, PartialEq)]
pub enum RecChoice {
    /// Set the setting to `value` (the rule's recommended one when `None`), resolving `rule`.
    Apply { rule: String, value: Option<Value>, label: String },
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
    /// The details of a marked setting, opened with `!`: every rule the setting departs from,
    /// `reference` ones included, with their sources.
    pub details: bool,
}

impl RecModal {
    pub fn new(findings: Vec<Recommendation>) -> Self {
        let conflict = findings.len() > 1;
        let choices = choices(&findings);
        RecModal { findings, choices, cursor: (!conflict).then_some(0), why: false, details: false }
    }

    /// The details of a setting: `findings` are the rules it departs from, offered ones first.
    pub fn details(findings: Vec<Recommendation>) -> Self {
        let conflict = findings.iter().any(|r| r.conflict && open(r));
        let choices = choices(&findings);
        RecModal { findings, choices, cursor: (!conflict).then_some(0), why: true, details: true }
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
            Strength::Reference if methods.len() == 1 => format!(" {} ", rec::reference_title(&r.trigger)),
            Strength::Reference => format!(" {}: reference pipelines of {} ", r.label, methods.join(", ")),
            _ if self.details && self.findings.len() > 1 => format!(" {} ", r.label),
            Strength::Required => format!(" {} requires a change ", r.trigger.name),
            Strength::Discouraged => format!(" Discouraged with {} ", r.trigger.name),
            _ if self.findings.len() > 1 => " Recommendations disagree ".to_string(),
            _ => format!(" {} recommends a change ", r.trigger.name),
        }
    }

    pub fn choice_label(c: &RecChoice) -> &str {
        match c {
            RecChoice::Apply { label, .. } => label,
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
    let mut out: Vec<RecChoice> = methods_of.into_iter().map(|(value, name, who, rule, recommended)| {
        let who = if several { format!(" ({})", who.join(", ")) } else { String::new() };
        let (value, label) = if recommended { (None, format!("Apply: {name}{who}")) } else { (Some(value), format!("Use {name} instead{who}")) };
        RecChoice::Apply { rule, value, label }
    }).collect();
    // A required one cannot be dismissed: the method cannot run otherwise.
    if findings.iter().all(|r| r.strength != Strength::Required) {
        out.push(RecChoice::Keep);
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
    fn new(finding: Recommendation) -> Self {
        let mut options = vec![Some((None, finding.recommended.name.clone()))];
        options.extend(finding.alternatives().into_iter().map(|a| Some((Some(a.value.clone()), a.name.clone()))));
        options.push(None);
        RefItem { finding, also: vec![], options, pick: 0 }
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

/// The buttons of the reference pipeline pop-up.
pub const REF_BUTTONS: [&str; 3] = ["Apply all", "Choose per setting", "Keep mine"];

/// The reference pipeline pop-up: where the config differs from the reference pipelines of the
/// methods a change chose, led by the recommendations the same change created.
#[derive(Debug, Clone)]
pub struct RefModal {
    /// The methods chosen, the first the one the title names when there is one.
    pub methods: Vec<Trigger>,
    /// The recommendations the change created (`recommended` and `discouraged` ones), then the
    /// differences from the reference pipelines, one per setting and value.
    pub items: Vec<RefItem>,
    /// The highlighted button ([`REF_BUTTONS`]).
    pub cursor: usize,
    /// Choosing per setting: the highlighted setting.
    pub row: Option<usize>,
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
        RefModal { methods, items, cursor: 0, row: None }
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

/// A finding offered as a change and not dismissed: what can open the pop-up or show a badge.
fn open(r: &Recommendation) -> bool {
    r.actionable() && !r.dismissed
}

impl App {
    /// What the rules know about the data from the form: the echo count it is limited to, and the
    /// field strength (typed for NIfTI input; the first selected run's, once the Pipeline view has
    /// read it, for a BIDS dataset). A rule on an unknown fact stays silent.
    fn rule_facts(&self) -> Facts {
        let field_strength = match self.input_mode {
            super::app::InputMode::NIfTI => self.nifti_state.field_strength.trim().parse().ok(),
            super::app::InputMode::Bids => self.pipeline_view.read_field_strength(),
            _ => None,
        };
        Facts { echo_count: self.filter_state.num_echoes.trim().parse().ok(), field_strength, ..Default::default() }
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
            let v = self.rule_view();
            let now: Vec<(String, String)> =
                v.iter().filter(|r| open(r)).map(|r| (r.rule.clone(), rec::chosen_text(&r.current))).collect();
            created = v.iter().filter(|r| {
                open(r) && !self.rec_seen.contains(&(r.rule.clone(), rec::chosen_text(&r.current)))
            }).cloned().collect();
            self.rec_seen = now;
            let chosen = self.note_methods(&v, &[]);
            if opening {
                self.refs.pending.clear();
                return;
            }
            // A method no longer chosen is not offered.
            self.refs.pending.retain(|(m, _)| chosen.contains(m));
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
        match (key.code, modal.row) {
            (KeyCode::Esc, Some(_)) => modal.row = None,
            (KeyCode::Esc, None) => self.refs.modal = None,
            (KeyCode::Left | KeyCode::Char('h'), None) => modal.cursor = (modal.cursor + REF_BUTTONS.len() - 1) % REF_BUTTONS.len(),
            (KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab, None) => modal.cursor = (modal.cursor + 1) % REF_BUTTONS.len(),
            (KeyCode::Up | KeyCode::Char('k'), Some(i)) => modal.row = Some((i + modal.items.len() - 1) % modal.items.len()),
            (KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab, Some(i)) => modal.row = Some((i + 1) % modal.items.len()),
            (KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') | KeyCode::Char(' '), Some(i)) => {
                let item = &mut modal.items[i];
                let n = item.options.len();
                item.pick = if key.code == KeyCode::Left || key.code == KeyCode::Char('h') { (item.pick + n - 1) % n } else { (item.pick + 1) % n };
            }
            (KeyCode::Enter, None) if modal.cursor == 1 => modal.row = Some(0),
            (KeyCode::Enter, _) => {
                let mut items = modal.items.clone();
                if !per_setting {
                    // Apply all: each item's recommended value; Keep mine: none.
                    let pick = if modal.cursor == 0 { 0 } else { usize::MAX };
                    for item in &mut items {
                        item.pick = if pick == 0 { 0 } else { item.options.len() - 1 };
                    }
                }
                self.refs.modal = None;
                self.decide(&items);
            }
            _ => {}
        }
    }

    /// Carry out the reference pipeline pop-up's choices: apply each picked value, keep the
    /// rest. A method an applied value chooses is offered without the settings just decided.
    fn decide(&mut self, items: &[RefItem]) {
        let mut errors = Vec::new();
        let mut kept = Vec::new();
        let mut applied: Vec<&str> = Vec::new();
        for item in items {
            match &item.options[item.pick] {
                // Two methods that want different values of one setting: the first applied wins.
                Some(_) if applied.contains(&item.finding.setting.as_str()) => {}
                Some((value, _)) => {
                    applied.push(&item.finding.setting);
                    if let Err(e) = self.apply_recommendation(&item.finding.rule, value.as_ref()) {
                        errors.push(e);
                    }
                }
                None => {
                    kept.push(item.finding.clone());
                    kept.extend(item.also.iter().cloned());
                }
            }
        }
        self.keep_mine(&kept);
        let kept_items = items.iter().filter(|i| i.options[i.pick].is_none()).count();
        self.status_message = Some(match (items.len() - kept_items, kept_items) {
            (0, _) => "Kept your settings".to_string(),
            (n, 0) => format!("Applied {n} setting{}", if n == 1 { "" } else { "s" }),
            (n, k) => format!("Applied {n}, kept {k}"),
        });
        if !errors.is_empty() {
            self.error_message = Some(errors.join("; "));
        }
        let skip: Vec<String> = items.iter().map(|i| i.finding.setting.clone()).collect();
        let view = self.rule_view();
        self.note_methods(&view, &skip);
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
                            let mut app = self.clone_form();
                            app.set_field_select(field, i);
                            candidates.push(app);
                        }
                    }
                    PipelineRow::Toggle { .. } => {
                        let mut app = self.clone_form();
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
                            let mut app = self.clone_form();
                            if let Some(s) = app.field_text_mut(field) {
                                *s = t;
                            }
                            candidates.push(app);
                        }
                    }
                }
            }
        }
        match candidates.into_iter().find(resolved) {
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

    /// A copy of the form, for trying a value on.
    fn clone_form(&self) -> App {
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
        let parts: Vec<String> = view.iter().filter(|r| r.setting == path).map(|r| {
            let what = match (r.departs, r.strength, r.dismissed) {
                (false, _, _) => "Reference",
                (true, Strength::Reference, _) => "Differs from the reference",
                (true, _, true) => "Dismissed",
                (true, Strength::Required, _) => "Required",
                (true, Strength::Discouraged, _) => "Discouraged",
                (true, Strength::Recommended, _) => "Recommended",
            };
            let value = if r.strength == Strength::Discouraged && r.departs {
                format!("not {}", r.current_text())
            } else {
                r.recommended.name.clone()
            };
            format!("{what}: {value} ({}; {})", r.trigger.name, r.source)
        }).collect();
        if parts.is_empty() {
            return None;
        }
        let marked = view.iter().any(|r| r.setting == path && r.departs);
        Some(format!("{}{}", parts.join(" · "), if marked { " — ! for details" } else { "" }))
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

/// The most options a selector cycles through with ←/→; one with more is a picker.
const MAX_CYCLED: usize = 3;

/// For a selector row chosen from a picker (Enter; ←/→ open it too) rather than cycled: what it
/// chooses, for "Enter: choose method". A selector is a picker when it has more than
/// [`MAX_CYCLED`] options, or when choosing it can open a reference pipeline pop-up
/// ([`rec::reference_trigger_settings`]): then no method is chosen on the way to another.
pub fn picker_noun(row: &PipelineRow) -> Option<String> {
    static TRIGGERS: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(rec::reference_trigger_settings);
    let (label, options) = match row {
        PipelineRow::AlgoSelect { label, field, options, .. } => {
            let n = if *field == "mask_preset" { super::app::MASK_PRESET_CUSTOM } else { options.len() };
            (label.trim(), n)
        }
        PipelineRow::MaskOpInput { .. } => ("Input", super::app::PipelineFormState::MASK_INPUT_SOURCES.len()),
        PipelineRow::MaskOpGenerator { .. } => ("Algorithm", super::app::MASK_GENERATOR_TYPES.len()),
        _ => return None,
    };
    let method = row_path(row).is_some_and(|p| TRIGGERS.contains(&p));
    if method {
        Some("method".into())
    } else if options > MAX_CYCLED {
        Some(label.to_lowercase())
    } else {
        None
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
    use super::super::app::QSM_ALGO_OPTIONS;

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
        select(&mut app, "qsm_algorithm", "medi", QSM_ALGO_OPTIONS);
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
        select(&mut app, "qsm_algorithm", "rts", QSM_ALGO_OPTIONS);
        let row = PipelineRow::AlgoSelect { label: "", field: "bf_algorithm", options: &[], help: &[] };
        let text = App::row_reference(&app.rule_view(), &row).unwrap();
        assert!(text.starts_with("Reference: V-SHARP (RTS; QSM.jl README.md"), "{text}");
        select(&mut app, "bf_algorithm", "pdf", super::super::app::BF_OPTIONS);
        assert!(app.rec_modal.is_none(), "information only");
        assert!(app.refs.modal.as_ref().is_none_or(|m| m.methods[0].name == "PDF"), "only a method just chosen opens its reference");
        assert_eq!(App::row_marker(&app.rule_view(), &row).map(|m| (m.actionable, m.text)), Some((false, "RTS: V-SHARP".into())));
        assert!(App::row_reference(&app.rule_view(), &row).unwrap().starts_with("Differs from the reference: V-SHARP"));
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
        select(&mut app, "qsm_algorithm", "medi", QSM_ALGO_OPTIONS);
        app
    }

    fn settings(modal: &RefModal) -> Vec<&str> {
        modal.items.iter().map(|i| i.finding.setting.as_str()).collect()
    }

    #[test]
    fn choosing_medi_opens_its_reference_pipeline_and_apply_all_takes_it() {
        let mut app = medi_chosen();
        let modal = app.refs.modal.as_ref().expect("MEDI's reference pipeline pop-up");
        assert_eq!(modal.title(), " MEDI's reference pipeline ");
        let s = settings(modal);
        assert!(s.contains(&"bg_removal.algorithm") && s.contains(&"masking.sections"), "{s:?}");
        let text = screen(&mut app, "MEDI chosen");
        assert!(text.contains("V-SHARP → PDF") && text.contains("threshold → BET") && text.contains("README.m:124"), "{text}");
        assert!(text.contains("Apply all") && text.contains("Choose per setting") && text.contains("Keep mine"));
        // Apply all (preselected).
        key(&mut app, KeyCode::Enter);
        assert!(app.refs.modal.is_none(), "PDF's reference adds nothing the pop-up did not decide");
        // PDF at its default 30 CG iterations: the measured suggestion of 300 is offered, not taken.
        let modal = app.rec_modal.as_ref().expect("PDF's iteration suggestion");
        assert_eq!(modal.findings[0].rule, "pdf.cg-iterations");
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::enums::BfAlgorithm::Pdf);
        assert_eq!(c.bg_removal.pdf.n_cg, qsm_core::bgremove::PdfParams::default().n_cg, "nothing rewritten yet");
        app.handle_rec_modal_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.rec_modal.is_none());
        let c = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(c.bg_removal.pdf.n_cg, 300, "Apply (preselected) takes the suggestion");
        let medi = rec::chosen_methods(&c, &Facts::default()).into_iter().find(|m| m.name == "MEDI").unwrap();
        assert!(rec::reference_differences(&c, &Facts::default(), &medi).is_empty());
        let row = PipelineRow::AlgoSelect { label: "", field: "mask_preset", options: &[], help: &[] };
        assert_eq!(App::row_marker(&app.rule_view(), &row), None, "the mask is BET now");
    }

    #[test]
    fn keep_mine_is_not_offered_again_until_the_method_is_chosen_again() {
        let mut app = medi_chosen();
        app.refs.modal.as_mut().unwrap().cursor = 2;
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
        select(&mut app, "qsm_algorithm", "rts", QSM_ALGO_OPTIONS);
        assert!(app.refs.modal.is_none());
        select(&mut app, "qsm_algorithm", "medi", QSM_ALGO_OPTIONS);
        assert!(app.refs.modal.as_ref().is_some_and(|m| m.methods[0].name == "MEDI"));
    }

    #[test]
    fn choosing_per_setting_applies_some_and_keeps_the_rest() {
        let mut app = medi_chosen();
        app.refs.modal.as_mut().unwrap().cursor = 1;
        key(&mut app, KeyCode::Enter);
        let modal = app.refs.modal.as_ref().unwrap();
        assert_eq!(modal.row, Some(0));
        let mask = settings(modal).iter().position(|s| *s == "masking.sections").unwrap();
        let bg = settings(modal).iter().position(|s| *s == "bg_removal.algorithm").unwrap();
        // Keep the mask (its last choice); everything else takes the reference.
        let keep = modal.items[mask].options.len() - 1;
        app.refs.modal.as_mut().unwrap().items[mask].pick = keep;
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
    /// `!` opens its details with Apply.
    #[test]
    fn a_reference_marker_names_the_method_and_bang_opens_its_details() {
        let mut app = app();
        app.set_field_select("mask_preset", 1);
        app.check_recommendations();
        select(&mut app, "qsm_algorithm", "qsmart", QSM_ALGO_OPTIONS);
        app.refs.modal = None;
        app.set_field_select("mask_preset", 0);
        app.check_recommendations();
        assert!(app.refs.modal.is_none() && app.rec_modal.is_none(), "a mask change chooses no method");
        let row = PipelineRow::AlgoSelect { label: "", field: "mask_preset", options: &[], help: &[] };
        let m = App::row_marker(&app.rule_view(), &row).unwrap();
        assert_eq!((m.actionable, m.text.as_str()), (false, "QSMART: BET"));
        assert!(app.goto_field("mask_preset"));
        let text = screen(&mut app, "QSMART with the threshold mask");
        assert!(text.contains("ⓘ QSMART: BET"), "{text}");
        key(&mut app, KeyCode::Char('!'));
        let modal = app.rec_modal.as_ref().expect("details");
        assert!(modal.details);
        assert_eq!(modal.title(), " QSMART's reference pipeline ");
        assert_eq!(modal.choices[0], RecChoice::Apply { rule: "qsmart.mask-bet".into(), value: None, label: "Apply: BET".into() });
        let text = screen(&mut app, "QSMART mask details (!)");
        assert!(text.contains("QSMART's reference pipeline uses BET") && text.contains("Source"), "{text}");
        key(&mut app, KeyCode::Enter);
        assert_eq!(App::row_marker(&app.rule_view(), &row), None);
    }

    /// A change that creates a recommendation and chooses a method opens one pop-up, the
    /// recommendation first.
    #[test]
    fn one_change_one_pop_up() {
        let mut app = app();
        // MEDI with mSMV and the prefilter off is discouraged; choosing MEDI with them set
        // creates that and chooses MEDI at once.
        select(&mut app, "qsm_algorithm", "medi", QSM_ALGO_OPTIONS);
        app.refs.modal = None;
        app.toggle_field("medi_msmv");
        app.toggle_field("medi_msmv_prefilter");
        app.check_recommendations();
        app.rec_modal = None;
        select(&mut app, "qsm_algorithm", "rts", QSM_ALGO_OPTIONS);
        select(&mut app, "qsm_algorithm", "medi", QSM_ALGO_OPTIONS);
        assert!(app.rec_modal.is_none());
        let modal = app.refs.modal.as_ref().expect("one pop-up");
        assert_eq!(modal.items[0].finding.rule, "medi-msmv.prefilter", "the recommendation first");
        assert!(modal.items[1..].iter().all(|i| i.finding.strength == Strength::Reference));
        screen(&mut app, "MEDI chosen with a recommendation");
    }

    /// No row that cycles with ←/→ can open a reference pipeline pop-up: each selector whose
    /// choice can is a picker, as is each with more than three options.
    #[test]
    fn method_and_long_selectors_are_pickers() {
        let triggers = rec::reference_trigger_settings();
        let mut app = app();
        app.form.do_swi = true;
        for (_, _, rows) in app.all_pages() {
            for row in &rows {
                let picker = picker_noun(row).is_some();
                if let PipelineRow::AlgoSelect { field, options, .. } = row {
                    if row_path(row).is_some_and(|p| triggers.contains(&p)) {
                        assert_eq!(picker_noun(row).as_deref(), Some("method"), "{field}");
                    } else {
                        assert_eq!(picker, options.len() > MAX_CYCLED, "{field}");
                    }
                }
            }
        }
        let row = |field: &'static str| PipelineRow::AlgoSelect { label: "  Unwrapping", field, options: &["romeo", "laplacian"], help: &[] };
        assert_eq!(picker_noun(&row("unwrapping_algorithm")), None, "two options: cycled");
        assert_eq!(picker_noun(&row("qsm_algorithm")).as_deref(), Some("method"), "a method, however few options");
    }

    /// ←/→ on a picker row open the picker (no cycling), and the row and footer say so.
    #[test]
    fn arrows_on_a_method_row_open_its_picker() {
        let mut app = app();
        assert!(app.goto_field("qsm_algorithm"));
        let before = app.field_select("qsm_algorithm");
        app.handle_key(KeyEvent::from(KeyCode::Right));
        assert_eq!(app.field_select("qsm_algorithm"), before, "not cycled");
        assert!(app.algo_modal.is_some(), "the picker opened");
        app.algo_modal = None;
        select(&mut app, "qsm_algorithm", "medi", QSM_ALGO_OPTIONS);
        app.refs.modal = None;
        assert!(app.goto_field("qsm_algorithm"));
        let text = screen(&mut app, "A method picker row and its footer");
        assert!(text.contains("[ medi ▾ ]") && text.contains("Enter: choose method") && text.contains("Enter:Choose method"), "{text}");
        assert!(app.goto_field("unwrapping_algorithm"));
        let text = screen(&mut app, "A cycled row and its footer");
        assert!(text.contains("◀ ") && text.contains("←→:Change") && !text.contains("Enter:Choose"), "{text}");
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
}
