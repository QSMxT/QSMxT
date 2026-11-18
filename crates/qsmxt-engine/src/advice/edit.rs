//! Which offer a change opens, and when ([`State::check`]).
//!
//! After every change a front end asks once. In order:
//! - **Required changes first**: the outputs the change needs (statistics, a region reference,
//!   SMWI, χ-separation's maps) in one [`Offer::required`], with Undo my change.
//! - **A change against a flag that is on** ([`Offer::changed`]): the setting just changed stands,
//!   and turning the flag off is preselected.
//! - **The findings the change created**, one setting (with what disagrees on it) per offer, the
//!   rest queued for the offers after it; merged with the chosen methods' reference pipelines
//!   into one [`Offer::reference`] when nothing among them is required or disagrees.
//! - **A chosen method's reference pipeline** ([`Offer::reference`]) once nothing else is open.
//!
//! The first check takes the config as it opened: only a change opens an offer.

use serde_json::Value;

use super::{changed_settings, group, open, undo_words, Offer};
use crate::config::PipelineConfig;
use crate::recommendations::{self as rec, Facts, Recommendation, Strength, Trigger};

/// Where an offer opens: a front end shows the two kinds in their own place (the TUI keeps a
/// reference pipeline offer apart from a change's, so one can wait for the other).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// A change's findings, required changes, details.
    Change,
    /// A chosen method's reference pipeline (led by the change's own recommendations).
    Reference,
}

/// An offer to open, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub slot: Slot,
    pub offer: Offer,
}

/// What the offers after a change are worked out from, kept between changes by the front end.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// The open findings at the last change, each with what its setting was: a finding not
    /// among them was created by the change after.
    pub seen: Vec<(String, String)>,
    /// The rules of findings a change created that wait for the offer before them to close.
    pub queue: Vec<String>,
    /// The config at the last change, as JSON: what a change is measured against.
    pub before: Option<Value>,
    /// Whether the config has been checked once (the first check opens nothing).
    pub started: bool,
    /// The choices with a reference pipeline at the last check ([`rec::chosen_methods`]).
    pub methods: Vec<Trigger>,
    /// Methods just chosen whose reference pipeline offer has not been shown, each with the
    /// settings (config paths) or rules (ids) it leaves out: those the offer that chose it had
    /// just decided.
    pub pending: Vec<(Trigger, Vec<String>)>,
}

impl State {
    /// Every rule that applies to `config`, departed from or not.
    fn view(config: Option<&PipelineConfig>, facts: &Facts) -> Vec<Recommendation> {
        config.map(|c| rec::reference_values(c, facts)).unwrap_or_default()
    }

    /// The config has changed since the last check (`changed`) or not: the offer to open now, if
    /// any. `config` is the config as the front end has it (`None` when it cannot build one),
    /// with dismissals of settings that changed already pruned ([`rec::prune_dismissals`]).
    /// With `offer_reference` off (after a pipeline is chosen: it is the authors' settings), no
    /// reference pipeline offer opens.
    pub fn check(&mut self, config: Option<&PipelineConfig>, facts: &Facts, changed: bool, offer_reference: bool) -> Option<Opened> {
        // The next finding the last change created, once the offer before it is closed.
        if !changed && self.pending.is_empty() && !self.queue.is_empty() {
            let view = Self::view(config, facts);
            while !self.queue.is_empty() {
                let rule = self.queue.remove(0);
                if let Some(r) = view.iter().find(|r| r.rule == rule && open(r)) {
                    return Some(Opened { slot: Slot::Change, offer: Offer::new(group(&view, r)) });
                }
            }
        }
        let mut created: Vec<Recommendation> = Vec::new();
        let mut view: Option<Vec<Recommendation>> = None;
        let mut just_changed: Vec<String> = Vec::new();
        let mut undo_what: Option<String> = None;
        if changed {
            let opening = !self.started;
            self.started = true;
            let json = config.and_then(|c| serde_json::to_value(c).ok());
            let mut diff = Vec::new();
            if let (Some(before), Some(after)) = (&self.before, &json) {
                changed_settings(before, after, String::new(), &mut diff);
            }
            undo_what = undo_words(self.before.as_ref(), &diff);
            self.before = json;
            let v = Self::view(config, facts);
            let now: Vec<(String, String)> =
                v.iter().filter(|r| open(r)).map(|r| (r.rule.clone(), rec::chosen_text(&r.current))).collect();
            created = v.iter().filter(|r| {
                open(r) && !self.seen.contains(&(r.rule.clone(), rec::chosen_text(&r.current)))
            }).cloned().collect();
            self.seen = now;
            just_changed = diff.into_iter().map(|(path, _)| path).collect();
            let chosen = self.note_methods(&v, &[]);
            if opening {
                self.pending.clear();
                return None;
            }
            // A method no longer chosen is not offered.
            self.pending.retain(|(m, _)| chosen.contains(m) && offer_reference);
            view = Some(v);
        }
        let view = view.unwrap_or_else(|| if self.pending.is_empty() { vec![] } else { Self::view(config, facts) });
        // Every pending method's differences from its reference: one offer for one change.
        let mut reference: Option<(Vec<Trigger>, Vec<Recommendation>)> = None;
        if !self.pending.is_empty() {
            let groups = rec::by_method(&view);
            let mut methods = Vec::new();
            let mut diffs: Vec<Recommendation> = Vec::new();
            for (method, skip) in std::mem::take(&mut self.pending) {
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
            return reference.map(|(methods, diffs)| Opened { slot: Slot::Reference, offer: Offer::reference(methods, diffs).with_authors(&view) });
        };
        // Outputs the change needs: one required-change offer for them all, before anything else.
        let required: Vec<Recommendation> = created.iter().filter(|r| r.applied_by_run()).cloned().collect();
        if !required.is_empty() {
            self.queue = created.iter().filter(|r| !r.applied_by_run()).map(|r| r.rule.clone()).collect();
            if let Some((methods, _)) = reference {
                self.pending = methods.into_iter().map(|m| (m, vec![])).collect();
            }
            return Some(Opened { slot: Slot::Change, offer: Offer::required(required, undo_what) });
        }
        // A setting just changed, against a flag that is on and was not: offer the flag first.
        let touched = |path: &str| just_changed.iter().any(|c| c == path || c.starts_with(&format!("{path}.")));
        let against_flag = created.iter().find(|r| {
            r.flag_trigger().is_some_and(|flag| !touched(flag) && crate::schema::setting(flag).is_some()) && touched(&r.setting)
        });
        if let Some(r) = against_flag {
            let group = group(&view, r);
            self.queue = created.iter().filter(|c| !group.iter().any(|g| g.setting == c.setting))
                .map(|c| c.rule.clone()).collect();
            if let Some((methods, _)) = reference {
                self.pending = methods.into_iter().map(|m| (m, vec![])).collect();
            }
            return Some(Opened { slot: Slot::Change, offer: Offer::changed(group) });
        }
        let group = group(&view, first);
        self.queue.clear();
        match reference {
            // One offer: the recommendations first, then the reference differences.
            Some((methods, diffs)) if created.iter().all(|r| r.strength != Strength::Required && !r.conflict) => {
                let mut items = created.clone();
                items.extend(diffs.into_iter().filter(|d| !created.iter().any(|c| c.rule == d.rule)));
                Some(Opened { slot: Slot::Reference, offer: Offer::reference(methods, items).with_authors(&view) })
            }
            // A required change or a disagreement first; the reference pipeline after it.
            Some((methods, _)) => {
                self.pending = methods.into_iter().map(|m| (m, vec![])).collect();
                Some(Opened { slot: Slot::Change, offer: Offer::new(group) })
            }
            None => {
                self.queue = created.iter().filter(|r| !group.iter().any(|g| g.setting == r.setting))
                    .map(|r| r.rule.clone()).collect();
                Some(Opened { slot: Slot::Change, offer: Offer::new(group) })
            }
        }
    }

    /// Note the methods `view` chooses: those not chosen before are queued for their reference
    /// pipeline offer, leaving out the settings in `skip`. Returns the methods chosen.
    pub fn note_methods(&mut self, view: &[Recommendation], skip: &[String]) -> Vec<Trigger> {
        let chosen: Vec<Trigger> = rec::by_method(view).into_iter().map(|m| m.method).collect();
        for m in chosen.iter().filter(|m| !self.methods.contains(m)) {
            if !self.pending.iter().any(|(p, _)| p == m) {
                self.pending.push((m.clone(), skip.to_vec()));
            }
        }
        self.methods = chosen.clone();
        chosen
    }
}

/// The details of `setting` (a front end's `!` on a marked setting): every rule it departs
/// from in `view`, offered ones first; `None` when it departs from none.
pub fn details(view: &[Recommendation], setting: &str) -> Option<Offer> {
    let mut here: Vec<Recommendation> = view.iter().filter(|r| r.setting == setting && r.departs).cloned().collect();
    if here.is_empty() {
        return None;
    }
    here.sort_by_key(|r| (!open(r), std::cmp::Reverse(r.strength)));
    if here.iter().all(|r| r.applied_by_run()) {
        return Some(Offer::required(here, None));
    }
    Some(Offer::details(here).with_authors(view))
}
