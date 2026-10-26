//! The TUI's side of the pipeline rules' findings ([`qsmxt_engine::recommendations`]): the pop-up a
//! change opens when it creates a recommendation, the badge on a setting that has one, and the
//! reference values in the help line. The findings are the engine's, for the config the form
//! builds ([`super::command::config_from_app`]); the TUI holds no rules.
//!
//! - **The pop-up** opens only when a change creates a `required`, `recommended` or
//!   `discouraged` finding that is not dismissed. It gives the reason (the source behind `w`,
//!   "why?"), **Apply** (the recommended value, preselected), the other values the rule allows,
//!   and **Ignore** (dismiss: kept in the form, and so in the run's config). Two equally strong
//!   findings that disagree on a setting are shown together, with nothing preselected. Esc closes
//!   it and leaves the finding open, shown by its badge; `!` on the setting opens it again.
//! - **A `reference` finding** never opens the pop-up: the setting shows an information marker,
//!   and its help line says what the reference pipeline uses.
//! - **Apply** sets the form so the config it builds takes the value: each value of the setting's
//!   row is tried until the engine no longer finds the departure.

use crossterm::event::{KeyCode, KeyEvent};
pub use qsmxt_engine::recommendations::Strength;
use qsmxt_engine::recommendations::{self as rec, Facts, Recommendation};
use serde_json::Value;

use super::app::{App, PipelineRow};

/// One choice of the pop-up.
#[derive(Debug, Clone, PartialEq)]
pub enum RecChoice {
    /// Set the setting to `value` (the rule's recommended one when `None`), resolving `rule`.
    Apply { rule: String, value: Option<Value>, label: String },
    /// Dismiss the findings, for the setting's current value.
    Ignore,
}

/// The recommendation pop-up.
#[derive(Debug, Clone)]
pub struct RecModal {
    /// What it is about: one finding, or equally strong ones on one setting that disagree.
    pub findings: Vec<Recommendation>,
    pub choices: Vec<RecChoice>,
    /// The highlighted choice; `None` until one is picked when the findings disagree.
    pub cursor: Option<usize>,
    /// Show the sources ("why?").
    pub why: bool,
}

impl RecModal {
    pub fn new(findings: Vec<Recommendation>) -> Self {
        let mut choices = Vec::new();
        for r in &findings {
            let who = if findings.len() > 1 { format!(" ({})", r.trigger.name) } else { String::new() };
            choices.push(RecChoice::Apply { rule: r.rule.clone(), value: None, label: format!("Apply: {}{who}", r.recommended.name) });
            for alt in r.alternatives() {
                choices.push(RecChoice::Apply {
                    rule: r.rule.clone(), value: Some(alt.value.clone()), label: format!("Use {} instead{who}", alt.name),
                });
            }
        }
        // A required one cannot be dismissed: the method cannot run otherwise.
        if findings.iter().all(|r| r.strength != Strength::Required) {
            choices.push(RecChoice::Ignore);
        }
        let conflict = findings.len() > 1;
        RecModal { findings, choices, cursor: (!conflict).then_some(0), why: false }
    }

    pub fn title(&self) -> String {
        let r = &self.findings[0];
        match r.strength {
            Strength::Required => format!(" {} requires a change ", r.trigger.name),
            Strength::Discouraged => format!(" Discouraged with {} ", r.trigger.name),
            _ if self.findings.len() > 1 => " Recommendations disagree ".to_string(),
            _ => format!(" {} recommends a change ", r.trigger.name),
        }
    }

    pub fn choice_label(c: &RecChoice) -> &str {
        match c {
            RecChoice::Apply { label, .. } => label,
            RecChoice::Ignore => "Ignore (keep my choice)",
        }
    }
}

/// A finding offered as a change and not dismissed: what can open the pop-up or show a badge.
fn open(r: &Recommendation) -> bool {
    r.actionable() && !r.dismissed
}

impl App {
    /// What the rules know about the data from the form: the echo count it is limited to.
    fn rule_facts(&self) -> Facts {
        Facts { echo_count: self.filter_state.num_echoes.trim().parse().ok(), ..Default::default() }
    }

    /// Every rule that applies to the form's config, departed from or not.
    pub fn rule_view(&self) -> Vec<Recommendation> {
        super::command::config_from_app(self)
            .map(|c| rec::reference_values(&c, &self.rule_facts()))
            .unwrap_or_default()
    }

    /// What the findings are worked out from: the form and the echo limit.
    pub(super) fn rec_fingerprint(&self) -> String {
        format!("{:?}{:?}{}", self.form, self.pipeline_state, self.filter_state.num_echoes)
    }

    /// The open findings (offered and not dismissed), by rule and value.
    pub fn open_recommendations(&self) -> Vec<(String, String)> {
        self.rule_view().into_iter().filter(open).map(|r| (r.rule.clone(), rec::chosen_text(&r.current))).collect()
    }

    /// After a key: drop the dismissals whose setting has changed, and open the pop-up for the
    /// findings the change created. Nothing while a value is being typed.
    pub fn check_recommendations(&mut self) {
        if self.is_typing() || self.rec_modal.is_some() {
            return;
        }
        let form = self.rec_fingerprint();
        if form == self.rec_form {
            return;
        }
        self.rec_form = form;
        if let Ok(mut c) = super::command::config_from_app(self) {
            let before = c.dismissed_recommendations.len();
            rec::prune_dismissals(&mut c, &self.rule_facts());
            if c.dismissed_recommendations.len() != before {
                self.pipeline_state.dismissed = c.dismissed_recommendations;
            }
        }
        let view = self.rule_view();
        let now: Vec<(String, String)> =
            view.iter().filter(|r| open(r)).map(|r| (r.rule.clone(), rec::chosen_text(&r.current))).collect();
        let created: Vec<&Recommendation> = view.iter().filter(|r| {
            open(r) && !self.rec_seen.contains(&(r.rule.clone(), rec::chosen_text(&r.current)))
        }).collect();
        self.rec_seen = now;
        if let Some(first) = created.first() {
            self.rec_modal = Some(RecModal::new(group(&view, first)));
        }
    }

    /// Open the pop-up for the open findings on the setting of the focused row (`!`).
    pub fn review_recommendations(&mut self) {
        let Some(path) = self.focused_row().and_then(|r| row_path(&r)) else { return };
        let view = self.rule_view();
        if let Some(first) = view.iter().find(|r| r.setting == path && r.actionable()) {
            self.rec_modal = Some(RecModal::new(group(&view, first)));
        }
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
                    RecChoice::Ignore => {
                        for r in &findings {
                            self.pipeline_state.dismissed.retain(|d| d.rule != r.rule);
                            self.pipeline_state.dismissed.push(qsmxt_engine::Dismissal {
                                rule: r.rule.clone(), chosen: rec::chosen_text(&r.current),
                            });
                        }
                    }
                }
                self.rec_seen = self.open_recommendations();
                self.rec_form.clear();
                // Whatever the choice created (a value another rule departs from) opens next.
                self.check_recommendations();
            }
            _ => {}
        }
    }

    /// Set the form so its config resolves `rule`: to `value` (the rule's recommended one when
    /// `None`). Each value of the setting's row is tried until the engine no longer finds the
    /// departure and, for a chosen value, the setting has it.
    pub fn apply_recommendation(&mut self, rule: &str, value: Option<&Value>) -> Result<(), String> {
        let Some(r) = self.rule_view().into_iter().find(|r| r.rule == rule) else { return Ok(()) };
        let resolved = |app: &App| {
            app.rule_view().iter().find(|x| x.rule == rule).is_none_or(|x| match value {
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

    /// The badge for a row: `Some(true)` for an open recommendation, `Some(false)` for a
    /// departure that is information only (a reference one, or one dismissed).
    pub fn row_badge(view: &[Recommendation], row: &PipelineRow) -> Option<bool> {
        let path = row_path(row)?;
        let here: Vec<&Recommendation> = view.iter().filter(|r| r.setting == path && r.departs).collect();
        if here.iter().any(|r| open(r)) {
            Some(true)
        } else if here.is_empty() {
            None
        } else {
            Some(false)
        }
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
        let open_here = view.iter().any(|r| r.setting == path && open(r));
        Some(format!("{}{}", parts.join(" · "), if open_here { " — ! to review" } else { "" }))
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
        app.rec_seen = app.open_recommendations();
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
        assert!(modal.choices.contains(&RecChoice::Ignore));
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
        assert_eq!(App::row_badge(&app.rule_view(), &row), Some(true));
        assert!(App::row_reference(&app.rule_view(), &row).is_some_and(|t| t.starts_with("Discouraged: not off") && t.ends_with("! to review")));
        app.check_recommendations();
        assert!(app.rec_modal.is_none(), "not opened again by a change that creates nothing");
        app.rec_modal = Some(RecModal::new(app.rule_view().into_iter().filter(|r| r.rule == "medi-msmv.prefilter").collect()));
        enter(&mut app, &RecChoice::Ignore);
        assert_eq!(app.pipeline_state.dismissed.len(), 1);
        let config = super::super::command::config_from_app(&app).unwrap();
        assert_eq!(config.dismissed_recommendations[0].rule, "medi-msmv.prefilter", "the dismissal reaches the config");
        assert!(super::super::command::build_command_string(&app).contains("--dismiss-recommendation medi-msmv.prefilter=false"));
        assert_eq!(App::row_badge(&app.rule_view(), &row), Some(false), "dismissed: information only");
        // A change to the setting drops the dismissal; going back offers it again.
        app.toggle_field("medi_msmv_prefilter");
        app.check_recommendations();
        assert!(app.pipeline_state.dismissed.is_empty());
        app.toggle_field("medi_msmv_prefilter");
        app.check_recommendations();
        assert!(app.rec_modal.is_some());
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
        assert_eq!(App::row_badge(&app.rule_view(), &row), Some(false));
        assert!(App::row_reference(&app.rule_view(), &row).unwrap().starts_with("Differs from the reference: V-SHARP"));
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
