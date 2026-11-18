//! What an offer starts with, by kind of advice: a recommendation (or anything required or
//! discouraged) preselects the change; an authors' setting or a suggestion preselects Keep mine;
//! equally strong advice that disagrees preselects nothing; a change against a flag that is on
//! starts with the flag turned off.

use serde_json::{json, Value};

use super::*;
use crate::recommendations::{Allowed, Basis, Named};

fn named(value: Value) -> Named {
    Named { name: value.to_string(), value }
}

/// A finding on `setting` wanting `value`, scoped to `trigger` (a method, or a flag when its value
/// is `true`). Rule ids are made up, so the group is the finding's own setting.
fn finding(kind: Kind, strength: Strength, setting: &str, value: Value, trigger: (&str, Value)) -> Recommendation {
    Recommendation {
        rule: format!("test.{setting}.{value}"), kind, strength, basis: Basis::Stated,
        setting: setting.into(), label: setting.into(),
        trigger: Trigger { setting: trigger.0.into(), name: "Method".into(), value: trigger.1 },
        current: vec![named(json!("mine"))], recommended: named(value.clone()),
        allowed: Allowed::OneOf { values: vec![named(value)] },
        reason: String::new(), source: String::new(),
        needs_scan: false, departs: true, dismissed: false, conflict: false,
    }
}

fn method(kind: Kind, strength: Strength, value: Value) -> Recommendation {
    finding(kind, strength, "bg_removal.algorithm", value, ("inversion.algorithm", json!("medi")))
}

const GROUP: &str = "bg_removal.algorithm";

#[test]
fn a_recommendation_preselects_the_change() {
    for strength in [Strength::Recommended, Strength::Required, Strength::Discouraged] {
        let offer = Offer::new(vec![method(Kind::Reasoned, strength, json!("pdf"))]);
        assert_eq!(offer.preselection(GROUP), Some(Preselect::Change), "{strength:?}");
        assert!(offer.rows[0].checked, "{strength:?}");
    }
}

#[test]
fn an_authors_setting_or_a_suggestion_preselects_keep_mine() {
    for (kind, strength) in [(Kind::Reference, Strength::Reference), (Kind::Reasoned, Strength::Suggested)] {
        for offer in [Offer::new(vec![method(kind, strength, json!("pdf"))]),
                      Offer::reference(vec![], vec![method(kind, strength, json!("pdf"))])] {
            assert_eq!(offer.preselection(GROUP), Some(Preselect::KeepMine), "{strength:?}");
            assert_eq!(offer.ticked(), 0, "{strength:?}");
        }
    }
}

#[test]
fn a_recommendation_outranks_an_authors_setting_on_the_same_setting() {
    let offer = Offer::new(vec![method(Kind::Reference, Strength::Reference, json!("vsharp")),
                                method(Kind::Reasoned, Strength::Recommended, json!("pdf"))]);
    assert_eq!(offer.preselection(GROUP), Some(Preselect::Change));
    let ticked: Vec<&AdviceRow> = offer.rows.iter().filter(|r| r.checked).collect();
    assert_eq!(ticked.len(), 1);
    assert_eq!(ticked[0].finding.recommended.value, json!("pdf"));
}

#[test]
fn equally_strong_disagreement_preselects_nothing() {
    for (kind, strength) in [(Kind::Reasoned, Strength::Recommended), (Kind::Reference, Strength::Reference)] {
        let offer = Offer::new(vec![method(kind, strength, json!("pdf")), method(kind, strength, json!("vsharp"))]);
        assert_eq!(offer.preselection(GROUP), Some(Preselect::Nothing), "{strength:?}");
        assert_eq!(offer.ticked(), 0, "{strength:?}");
    }
}

#[test]
fn a_conflicting_finding_preselects_nothing() {
    let mut f = method(Kind::Reasoned, Strength::Recommended, json!("pdf"));
    f.conflict = true;
    assert_eq!(Offer::new(vec![f]).preselection(GROUP), Some(Preselect::Nothing));
}

#[test]
fn the_setting_just_changed_stands_and_the_flag_is_turned_off() {
    let heidi = ("inversion.heidi.enabled", json!(true));
    for strength in [Strength::Suggested, Strength::Recommended, Strength::Required] {
        let f = finding(Kind::Reasoned, strength, "inversion.algorithm", json!("lsqr"), heidi.clone());
        let offer = Offer::changed(vec![f]);
        assert_eq!(offer.rows.len(), 1, "{strength:?}: only turning HEIDI off, not changing back: {:?}", offer.rows);
        assert_eq!(offer.rows[0].action, RowAction::TurnOff { setting: "inversion.heidi.enabled".into() });
        assert!(offer.rows[0].checked);
        assert_eq!(offer.preselection("inversion.algorithm"), Some(Preselect::Change));
    }
}

#[test]
fn a_required_output_is_ticked_with_undo() {
    let mut f = method(Kind::Reasoned, Strength::Required, json!(true));
    f.basis = Basis::Wiring;
    let offer = Offer::required(vec![f], Some("QSM reference back to mean".into()));
    assert!(offer.rows.iter().all(|r| r.checked));
    assert_eq!(offer.preselection(GROUP), Some(Preselect::Change));
    assert_eq!(offer.undo.as_deref(), Some("QSM reference back to mean"));
}
