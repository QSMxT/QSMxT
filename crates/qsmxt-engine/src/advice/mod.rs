//! Advice: what a front end offers when the pipeline rules find something
//! ([`crate::recommendations`]), decided once for every front end (the TUI, qsmbly, `qsmxt run
//! --accept-*`).
//!
//! - **[`Offer`]**: an advice pop-up's content: the changes on offer as rows, each "setting
//!   current → new (kind)" with its reason, grouped so that the values of one setting exclude
//!   each other, and what starts ticked. A front end draws it and lets the user tick rows; what
//!   the rows mean is decided here.
//! - **[`edit`]**: after a change, which offer opens (a required change first, then a change
//!   against a flag that is on, then the recommendations and the chosen methods' reference
//!   pipelines).
//! - **[`decide`]**: what applying an offer's picks does (what is applied, kept, left marked,
//!   and offered next).
//!
//! **What starts ticked** ([`Preselect`], per group) follows the kind of advice:
//! - a recommendation (or anything required or discouraged) preselects the change;
//! - an original authors' setting (a `reference` rule) preselects **Keep mine**;
//! - a suggestion is offered with its reason and preselects Keep mine;
//! - nothing is preselected only when equally strong advice on a setting disagrees.
//!
//! **The setting just changed stands** ([`Offer::changed`]): a change that makes a setting depart
//! from a rule scoped to a flag that is on (choosing QSMART with HEIDI on) preselects turning
//! that flag off, and changing the setting back is not offered. A rule scoped to a method still
//! offers the rule's value: nothing says which other method to pick.
//!
//! **Required changes** ([`Offer::required`]): outputs another setting needs, every one ticked,
//! with "Undo my change" when a change just made needs them; left as they are, a run turns them
//! on itself and says so (no setting changes silently).

pub mod decide;
pub mod edit;

use serde::Serialize;
use serde_json::Value;

use crate::recommendations::{self as rec, Kind, Recommendation, Strength, Trigger};

/// What ticking a row of an advice pop-up does.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowAction {
    /// Set the finding's setting to `value` (the rule's recommended one when `None`), which reads
    /// as `name`.
    Apply { value: Option<Value>, name: String },
    /// Turn off the flag the finding is scoped to (`setting`, its config path) instead, keeping
    /// the setting as it is.
    TurnOff { setting: String },
}

/// One row of an advice pop-up: a setting as it is and what ticking the row changes it to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdviceRow {
    pub finding: Recommendation,
    /// The other methods' rules that want the same value of the same setting: decided with it.
    pub also: Vec<Recommendation>,
    pub action: RowAction,
    pub checked: bool,
    /// A value the rule allows besides the one it gives.
    pub alternative: bool,
    /// One of the values chosen methods' authors disagree on ([`rec::disagreeing`]): tagged
    /// with whose it is, one choice with the other authors' values.
    pub authors: bool,
    /// The setting already has this row's value (another chosen method's authors' setting it
    /// follows): choosing the row keeps it.
    pub keeps: bool,
    /// Rows of one group exclude each other: the values of one part of a setting, and turning
    /// off the flag that part depends on instead.
    pub group: String,
}

impl AdviceRow {
    fn apply(finding: Recommendation, value: Option<Value>, name: String, alternative: bool) -> Self {
        let group = rec::rule_setting(&finding).to_string();
        AdviceRow { finding, also: vec![], action: RowAction::Apply { value, name }, checked: false, alternative, authors: false, keeps: false, group }
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

    /// The row's tag: what kind of advice it is, or, for one of the values chosen methods'
    /// authors disagree on, whose it is ("PDF's authors").
    pub fn tag(&self) -> String {
        if self.alternative {
            return "allowed".into();
        }
        if self.authors {
            let names: Vec<String> = std::iter::once(&self.finding).chain(&self.also).map(|r| format!("{}'s", r.trigger.name)).collect();
            return format!("{} authors", names.join(" and "));
        }
        self.finding.strength.label().into()
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
pub fn part_label(r: &Recommendation) -> String {
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

/// `s` with its first letter in capitals, if it is a Latin one ("χ-separation" stays as it is).
fn ascii_capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| std::iter::once(f.to_ascii_uppercase()).chain(c).collect())
}

fn lowercase_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_lowercase().chain(c).collect())
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Which pop-up an advice pop-up is.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdviceKind {
    /// A change created a reasoned finding ([`Offer::new`]).
    Change,
    /// The finding is on the setting just changed, against a flag that is on
    /// ([`Offer::changed`]).
    Changed,
    /// The details of a marked setting ([`Offer::details`]).
    Details,
    /// The differences from the reference pipelines of the methods a change chose
    /// ([`Offer::reference`]).
    Reference(Vec<Trigger>),
    /// Outputs another setting needs ([`Offer::required`]).
    Required,
}

/// What an offer starts with for one group of rows (one setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Preselect {
    /// A change is ticked: a recommendation, or anything required or discouraged, or turning off
    /// the flag a change just made went against.
    Change,
    /// Nothing is ticked, and Keep mine is what the offer starts with: an original authors'
    /// setting, or a suggestion.
    KeepMine,
    /// Nothing is ticked and nothing is preselected: equally strong advice on the setting
    /// disagrees.
    Nothing,
}

/// An advice pop-up's content: the changes on offer, each "setting: current → new" with its
/// kind, grouped by setting. One Apply applies the ticked rows; Keep mine keeps every setting as
/// it is. What starts ticked is [`Self::preselected`], per group.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Offer {
    pub kind: AdviceKind,
    /// The findings it is about, in order.
    pub findings: Vec<Recommendation>,
    pub rows: Vec<AdviceRow>,
    /// What each group (by [`AdviceRow::group`], in row order) starts with.
    pub preselected: Vec<(String, Preselect)>,
    /// What "Undo my change" sets back, in words ("QSM reference back to mean"), in a
    /// required-change offer opened by a change.
    pub undo: Option<String>,
}

impl Offer {
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
        let mut offer = Offer { kind, findings, rows, preselected: vec![], undo: None };
        offer.note_preselected();
        offer
    }

    /// Record what each group starts with, from the rows as they are ticked: a ticked row is a
    /// change; an untouched group is Keep mine, unless equally strong advice in it disagrees.
    fn note_preselected(&mut self) {
        let mut groups: Vec<String> = Vec::new();
        for r in &self.rows {
            if !groups.contains(&r.group) {
                groups.push(r.group.clone());
            }
        }
        self.preselected = groups.into_iter().map(|g| {
            let here: Vec<&AdviceRow> = self.rows.iter().filter(|r| r.group == g).collect();
            let p = if here.iter().any(|r| r.checked) {
                Preselect::Change
            } else {
                let primaries: Vec<&&AdviceRow> = here.iter().filter(|r| !r.alternative).collect();
                let disagree = primaries.iter().enumerate().any(|(i, a)| primaries.iter().skip(i + 1).any(|b| {
                    a.finding.strength == b.finding.strength && a.finding.kind == b.finding.kind
                        && a.finding.recommended.value != b.finding.recommended.value
                }));
                let conflict = primaries.iter().any(|r| r.finding.conflict || r.authors);
                if disagree || conflict { Preselect::Nothing } else { Preselect::KeepMine }
            };
            (g, p)
        }).collect();
    }

    /// What `group` starts with ([`Preselect`]).
    pub fn preselection(&self, group: &str) -> Option<Preselect> {
        self.preselected.iter().find(|(g, _)| g == group).map(|(_, p)| *p)
    }

    /// The outputs other settings need that the config leaves off (`findings`, each
    /// [`rec::Recommendation::applied_by_run`]): every one ticked, as one change, with "Undo my
    /// change" when a change just made needs them (`undo`, what it sets back).
    pub fn required(findings: Vec<Recommendation>, undo: Option<String>) -> Self {
        let rows = findings.iter().map(|r| {
            let mut row = AdviceRow::apply(r.clone(), None, r.recommended.name.clone(), false);
            row.checked = true;
            row
        }).collect();
        let mut offer = Offer { kind: AdviceKind::Required, findings, rows, preselected: vec![], undo };
        offer.note_preselected();
        offer
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
        m.note_preselected();
        m
    }

    /// The details of a setting: `findings` are the rules it departs from, offered ones first.
    pub fn details(findings: Vec<Recommendation>) -> Self {
        let mut m = Self::build(AdviceKind::Details, findings, true);
        m.rows.extend(turn_off_rows(&m.findings));
        m.note_preselected();
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

    /// The methods whose reference pipelines a reference offer is about.
    pub fn methods(&self) -> &[Trigger] {
        match &self.kind {
            AdviceKind::Reference(methods) => methods,
            _ => &[],
        }
    }

    /// Whether Keep mine is offered: not when every finding is required. In a required-change
    /// offer, its place is Undo my change, when there is a change to undo.
    pub fn keepable(&self) -> bool {
        if self.kind == AdviceKind::Required {
            return self.undo.is_some();
        }
        self.findings.iter().any(|r| r.strength != Strength::Required)
    }

    /// The buttons' labels, in order.
    pub fn button_labels(&self) -> Vec<String> {
        let mut out = match self.kind {
            AdviceKind::Required => vec!["Apply required change".to_string()],
            _ => vec![format!("Apply ({})", self.ticked())],
        };
        if self.keepable() {
            out.push(if self.kind == AdviceKind::Required { "Undo my change" } else { "Keep mine" }.to_string());
        }
        out
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

    /// Row `i`'s explanation, for the panel under the list. For one of the values chosen
    /// methods' authors disagree on: whose it is and why, what the others use, and that the
    /// choice is the user's.
    pub fn explanation(&self, i: usize) -> String {
        let Some(row) = self.rows.get(i) else { return String::new() };
        if row.finding.applied_by_run() {
            let r = &row.finding;
            let mut text = format!("{} requires {}: {}", ascii_capitalised(&r.trigger.name), r.label, sentence(&lowercase_first(&r.reason)));
            if let Some(what) = &self.undo {
                text += &format!(" Undo my change sets {what} instead.");
            } else {
                text += " Left off, the run turns it on itself and says so in its log.";
            }
            return text;
        }
        if !row.authors {
            return row.explanation(self.several(), self.group_unticked(i));
        }
        let r = &row.finding;
        let whose = row.tag();
        let as_set = if row.keeps { " (as set)" } else { "" };
        let others: Vec<String> = self.rows.iter().filter(|o| o.authors && o.group == row.group && o.finding.rule != r.rule)
            .map(|o| format!("{} use {}", o.tag(), o.finding.recommended.name)).collect();
        format!("{whose} use {}{as_set}: {} {}, so QSMxT does not choose between them: pick one, or Keep mine to leave {} as {}.",
            r.recommended.name, sentence(&r.reason), others.join(" and "), r.label, r.current_text())
    }

    /// Show the settings chosen methods' authors disagree on ([`rec::disagreeing`] in `view`)
    /// as one choice between the authors' values, each tagged with whose it is: the other
    /// authors' values join the group of each such row, after it (so the method the pop-up is
    /// about comes first), the one the setting already has as "as set". Nothing is preselected:
    /// they are authors' settings, and two disagree.
    pub fn with_authors(mut self, view: &[Recommendation]) -> Self {
        let mut i = 0;
        while i < self.rows.len() {
            let row = &self.rows[i];
            let others: Vec<Recommendation> = if row.alternative || !matches!(row.action, RowAction::Apply { value: None, .. }) {
                vec![]
            } else {
                rec::disagreeing(view, &row.finding).into_iter().cloned().collect()
            };
            if others.is_empty() {
                i += 1;
                continue;
            }
            self.rows[i].authors = true;
            self.rows[i].checked = false;
            let group = self.rows[i].group.clone();
            let mut at = self.rows.iter().rposition(|r| r.group == group).unwrap_or(i) + 1;
            for o in others {
                // A value already offered (both departed, both in the pop-up): tag it.
                if let Some(existing) = self.rows.iter_mut().find(|r| r.group == group && !r.alternative
                    && matches!(&r.action, RowAction::Apply { value: None, .. }) && r.finding.recommended.value == o.recommended.value)
                {
                    existing.authors = true;
                    existing.checked = false;
                    if existing.finding.rule != o.rule && !existing.also.iter().any(|a| a.rule == o.rule) {
                        existing.also.push(o);
                    }
                    continue;
                }
                let mut row = AdviceRow::apply(o.clone(), None, o.recommended.name.clone(), false);
                row.authors = true;
                row.keeps = !o.departs;
                self.rows.insert(at, row);
                at += 1;
            }
            i += 1;
        }
        // The alternatives an authors' row allows are not offered beside the authors' values.
        let authors: Vec<String> = self.rows.iter().filter(|r| r.authors).map(|r| r.group.clone()).collect();
        self.rows.retain(|r| !(r.alternative && authors.contains(&r.group)));
        self.note_preselected();
        self
    }

    /// Row `i`'s tag: its kind, "or" for a second way to meet one requirement, or whose
    /// authors' value it is.
    pub fn tag(&self, i: usize) -> String {
        match self.rows.get(i) {
            Some(_) if self.or_row(i) => "or".into(),
            Some(r) => r.tag(),
            None => String::new(),
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
        // A required change is one change: its rows stay ticked.
        if self.kind == AdviceKind::Required {
            return;
        }
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
            AdviceKind::Required => " Required change ".to_string(),
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
/// is a setting of the config (one a front end shows), each in the group of the setting its
/// finding is on.
fn turn_off_rows(findings: &[Recommendation]) -> Vec<AdviceRow> {
    let mut out: Vec<AdviceRow> = Vec::new();
    for r in findings {
        let Some(setting) = r.flag_trigger() else { continue };
        if crate::schema::setting(setting).is_none() || out.iter().any(|o| o.action == RowAction::TurnOff { setting: setting.to_string() }) {
            continue;
        }
        out.push(AdviceRow {
            finding: r.clone(), also: vec![], action: RowAction::TurnOff { setting: setting.to_string() },
            checked: false, alternative: false, authors: false, keeps: false, group: rec::rule_setting(r).to_string(),
        });
    }
    out
}

/// A finding offered as a change and not dismissed: what can open an offer or show a badge.
pub fn open(r: &Recommendation) -> bool {
    r.actionable() && !r.dismissed
}

/// `first`, with the findings as strong on its setting that disagree with it.
pub fn group(view: &[Recommendation], first: &Recommendation) -> Vec<Recommendation> {
    if !first.conflict {
        return vec![first.clone()];
    }
    view.iter().filter(|r| r.setting == first.setting && r.strength == first.strength && open(r)).cloned().collect()
}

/// The settings that differ between two configs (as JSON), by path, with their new values.
pub fn changed_settings(before: &Value, after: &Value, path: String, out: &mut Vec<(String, Value)>) {
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

/// The change just made, as "Undo my change" says it sets it back: the first changed setting the
/// settings schema names, with its value before ("QSM reference back to mean").
pub fn undo_words(before: Option<&Value>, changed: &[(String, Value)]) -> Option<String> {
    let before = before?;
    changed.iter().find_map(|(path, _)| {
        let setting = crate::schema::setting(path)?;
        let was = path.split('.').try_fold(before, |v, k| v.get(k))?;
        let was = match was {
            Value::Bool(true) => "on".to_string(),
            Value::Bool(false) => "off".to_string(),
            Value::Null => "none".to_string(),
            Value::String(s) if s.is_empty() => "none".to_string(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        Some(format!("{} back to {was}", setting.label))
    })
}

#[cfg(test)]
mod tests;
