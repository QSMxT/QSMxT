//! What carrying out an offer does: which values are applied, which findings are kept (dismissed
//! for their value), which stay marked, and what is offered next.
//!
//! A front end turns the offer's rows into [`Item`]s ([`items`]), applies [`Plan::apply`] in
//! order (each rule's value, as [`rec::apply`] sets it), applies once more what still departs (a
//! mask preset brings its erosion back), sets back the kept mask parts ([`restore_kept`]), keeps
//! [`Plan::kept`] ([`rec::keep`]), and says [`Plan::status`]. The methods the applied values
//! chose are offered next ([`follow_ups`]).

use serde_json::Value;

use super::edit::State;
use super::{Offer, RowAction};
use crate::config::PipelineConfig;
use crate::recommendations::{self as rec, Facts, Recommendation};

/// One setting as an offer decides it.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub finding: Recommendation,
    pub also: Vec<Recommendation>,
    /// The value to apply and its name, or `None` to keep the current one.
    pub pick: Option<(Option<Value>, String)>,
}

impl Item {
    /// The item's findings as picked, each wanting the picked value: none when keeping the
    /// current one.
    pub fn picked(&self) -> Vec<Recommendation> {
        let Some((value, _)) = &self.pick else { return vec![] };
        std::iter::once(&self.finding).chain(&self.also).cloned().map(|mut r| {
            if let Some(v) = value {
                r.recommended.value = v.clone();
            }
            // Picked between authors who disagree: the user's choice, which [`rec::settle`] takes.
            r.conflict = false;
            r
        }).collect()
    }
}

/// An offer carried out: the flags ticked off (each a config path, turned off before anything
/// else), and every other finding as an [`Item`], applied when its row is ticked and kept
/// otherwise. With `keep_all` (Keep mine), every one is kept.
pub fn items(offer: &Offer, keep_all: bool) -> (Vec<(String, Recommendation)>, Vec<Item>) {
    let off: Vec<(String, Recommendation)> = if keep_all { vec![] } else {
        offer.rows.iter().filter(|r| r.checked).filter_map(|r| match &r.action {
            RowAction::TurnOff { setting } => Some((setting.clone(), r.finding.clone())),
            RowAction::Apply { .. } => None,
        }).collect()
    };
    let mut items: Vec<Item> = Vec::new();
    // A row that keeps the value the setting has (another chosen method's authors') is no
    // finding: choosing it leaves the others of its group kept.
    for row in offer.rows.iter().filter(|r| !r.alternative && !r.keeps && matches!(r.action, RowAction::Apply { .. })) {
        let rule = &row.finding.rule;
        let chosen = if keep_all { None } else {
            offer.rows.iter().find(|x| x.finding.rule == *rule && x.checked && matches!(x.action, RowAction::Apply { .. }))
        };
        if chosen.is_none() && off.iter().any(|(_, f)| f.rule == *rule) {
            continue;
        }
        let pick = chosen.and_then(|c| match &c.action {
            RowAction::Apply { value, name } => Some((value.clone(), name.clone())),
            RowAction::TurnOff { .. } => None,
        });
        items.push(Item { finding: row.finding.clone(), also: row.also.clone(), pick });
    }
    (off, items)
}

/// What carrying out `items` does ([`plan`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The rules to apply, in order, each with the value picked (the rule's own when `None`).
    pub apply: Vec<(String, Option<Value>)>,
    /// The findings kept as they are.
    pub kept: Vec<Recommendation>,
    /// Picks not applied because a stronger finding wants another value.
    pub outranked: usize,
    /// Picks left because the advice disagrees.
    pub disagreeing: usize,
    /// The settings those disagreeing picks are on, by label.
    pub disagree_on: Vec<String>,
    /// The items kept.
    pub kept_items: usize,
    /// The items.
    pub items: usize,
}

/// What applying `items` together does: the picks [`rec::settle`] takes are applied (one per
/// setting), the items kept are kept, and a pick it leaves changes nothing and stays marked.
pub fn plan(items: &[Item]) -> Plan {
    let picked: Vec<Recommendation> = items.iter().flat_map(Item::picked).collect();
    let settled = rec::settle(&picked);
    let mut out = Plan { items: items.len(), ..Default::default() };
    let mut applied: Vec<&str> = Vec::new();
    for item in items {
        let finding = &item.finding;
        match &item.pick {
            Some(_) if settled.conflicting.iter().any(|r| r.rule == finding.rule) => {
                out.disagreeing += 1;
                if !out.disagree_on.contains(&finding.label) {
                    out.disagree_on.push(finding.label.clone());
                }
            }
            Some(_) if !settled.apply.iter().any(|r| r.rule == finding.rule) => out.outranked += 1,
            // Agrees with one already applied on the same setting (by QSM.rs's path: the mask
            // recipe's generator, input and erosion are parts of one QSMxT setting).
            Some(_) if applied.contains(&rec::rule_setting(finding)) => {}
            Some((value, _)) => {
                applied.push(rec::rule_setting(finding));
                out.apply.push((finding.rule.clone(), value.clone()));
            }
            None => {
                out.kept.push(finding.clone());
                out.kept.extend(item.also.iter().cloned());
                out.kept_items += 1;
            }
        }
    }
    out
}

impl Plan {
    /// What was done, for the status line: "Applied 2, kept 1; …".
    pub fn status(&self) -> String {
        let (outranked, disagreeing) = (self.outranked, self.disagreeing);
        let mut status = match (self.items - self.kept_items - outranked - disagreeing, self.kept_items) {
            (0, _) if outranked + disagreeing == 0 => "Kept your settings".to_string(),
            (0, 0) => "Applied nothing".to_string(),
            (0, k) => format!("Kept {k}"),
            (n, 0) => format!("Applied {n} setting{}", if n == 1 { "" } else { "s" }),
            (n, k) => format!("Applied {n}, kept {k}"),
        };
        if outranked > 0 {
            status += &format!("; {outranked} not applied: a stronger suggestion wants another value");
        }
        if !self.disagree_on.is_empty() {
            status += &format!("; left {} as set: the suggestions disagree (! to choose)", self.disagree_on.join(", "));
        }
        status
    }
}

/// The rules of [`Plan::apply`] that `config` still departs from: applied once more, as a value
/// applied later can undo an earlier one on another part of the same setting (a mask preset
/// brings its erosion back).
pub fn still_departing(plan: &Plan, config: &PipelineConfig, facts: &Facts) -> Vec<(String, Option<Value>)> {
    let view = rec::reference_values(config, facts);
    plan.apply.iter().filter(|(rule, _)| view.iter().any(|x| x.rule == *rule && x.departs)).cloned().collect()
}

/// Set back each of `kept` (the mask recipe's parts) that the values just applied changed, to
/// the value it had when it was kept: a value applied to one part of the mask recipe can change
/// a part kept (the generator brings its preset's input and erosion). Returns whether `config`
/// changed.
pub fn restore_kept(config: &mut PipelineConfig, kept: &[Recommendation], facts: &Facts) -> bool {
    let mut changed = false;
    for k in kept.iter().filter(|k| k.setting == "masking.sections") {
        let Some(was) = k.current.first().map(|n| n.value.clone()) else { continue };
        if k.current.iter().any(|n| n.value != was) {
            continue;
        }
        let now = rec::reference_values(config, facts).into_iter().find(|r| r.rule == k.rule);
        if now.is_none_or(|r| r.current.iter().all(|n| n.value == was)) {
            continue;
        }
        let mut c = config.clone();
        if rec::restore(&mut c, &k.rule, &was).is_ok() {
            *config = c;
            changed = true;
        }
    }
    changed
}

/// After `items` are carried out (`before`: what departed before, each with its value; `view`:
/// the rules as they are now), note the methods the applied values chose, and offer next
/// a method's differences that only come into play with a value just applied (BET's
/// orientation, once the mask is BET; the Laplacian's solver, once it unwraps): only those, by
/// rule, so the rows kept here (or in an earlier offer) are not offered again and applying
/// reaches a fixed point. With `offer_new` off (a recommendation's offer, not a reference
/// pipeline's), no new method's differences are offered after it.
pub fn follow_ups(state: &mut State, items: &[Item], before: &[(String, String)], view: &[Recommendation], offer_new: bool) {
    let skip: Vec<String> = items.iter().map(|i| i.finding.setting.clone()).collect();
    state.note_methods(view, &skip);
    if !offer_new {
        return;
    }
    let presented: Vec<&str> = items.iter().map(|i| i.finding.rule.as_str()).collect();
    for m in rec::by_method(view) {
        if !items.iter().any(|i| i.finding.trigger == m.method) || state.pending.iter().any(|(p, _)| *p == m.method) {
            continue;
        }
        let differences = m.differences();
        let new = |r: &Recommendation| !presented.contains(&r.rule.as_str())
            && !before.contains(&(r.rule.clone(), rec::chosen_text(&r.current)));
        if differences.iter().any(new) {
            let skip = differences.iter().filter(|r| !new(r)).map(|r| r.rule.clone()).collect();
            state.pending.push((m.method, skip));
        }
    }
}

/// What departs in `view`, each with its value: what [`follow_ups`] measures against.
pub fn departing(view: &[Recommendation]) -> Vec<(String, String)> {
    view.iter().filter(|r| r.departs).map(|r| (r.rule.clone(), rec::chosen_text(&r.current))).collect()
}
