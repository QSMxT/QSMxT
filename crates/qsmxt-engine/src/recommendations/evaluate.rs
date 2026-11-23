//! Evaluating QSM.rs's rules on a config, in QSMxT's settings.

use super::*;

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

/// Whether `rule` holds only for some choice of background removal (`harperella.bg-iharperella`,
/// PDF's settings): a condition of it names that choice.
fn on_the_background_choice(rule: &core::Rule) -> bool {
    rule.when.iter().any(|c| matches!(c, Condition::Setting { setting: "bg_removal.algorithm", .. }))
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
    // iQFM is no background removal of QSM.rs's (it works from phase): the rules see a placeholder
    // in its place (`bridge::to_qsm_pipeline_config`), so none that the choice of background
    // removal triggers is about the run.
    let iqfm = c.bg_removal.algorithm == crate::enums::BfAlgorithm::Iqfm;
    let mut out: Vec<Recommendation> = core::RULES.iter().zip(applying()).filter_map(|(rule, probe)| {
        let Condition::Setting { setting: trigger, .. } = rule.trigger() else { return None };
        if !c.pipeline.do_qsm && (reconstructs(trigger) || reconstructs(rule.setting)) {
            return None;
        }
        if iqfm && on_the_background_choice(rule) {
            return None;
        }
        let applies = core::check(probe, &input, &scan)?;
        let departs = core::check(rule, &input, &scan).is_some();
        Some(recommendation(&json, rule, &input, &scan, &applies.current, departs))
    }).collect();
    out.extend(qsmxt_findings(c, facts));
    out.extend(bipolar_signature_finding(c, facts));
    // The outputs the config as given leaves off that another setting needs: on the config as
    // given, since normalising it turns them on.
    out.extend(crate::normalise::required_findings(config));
    // What the scan's acquisition needs (a bipolar readout's bipolar correction).
    out.extend(crate::normalise::scan_required_findings(config, facts));
    mark(config, &mut out);
    out
}

/// QSMxT's own recommendations, beside QSM.rs's rules ([`Basis::Qsmxt`]).
///
/// `qsmxt.chi-sep-predict-r2prime`: the χ-separation toolbox's iLSQR and MEDI methods take a
/// measured R2′, and their toolbox pipelines measure it only (`r2prime_strategy = mese`; decided
/// 2026-11-10). On a scan known to have none (no MESE, no supplied map), the run cannot go ahead
/// as configured; QSMxT can predict R2′ from R2* with R2PRIMEnet instead, which this offers,
/// labelled as QSMxT's and not the toolbox's. A recommendation, so a front end preselects it.
fn qsmxt_findings(c: &PipelineConfig, facts: &Facts) -> Vec<Recommendation> {
    use crate::enums::{R2PrimeStrategy, SeparationAlgorithm as S};
    let s = &c.separation;
    let iterative = matches!(s.algorithm, S::ChiSepIlsqr | S::ChiSepMedi);
    if !(c.pipeline.do_chi_separation && iterative && s.r2prime_strategy == R2PrimeStrategy::Mese
        && s.custom_r2prime_tool.is_none() && facts.measured_r2prime == Some(false))
    {
        return vec![];
    }
    let named = |v: &str| {
        let value = Value::String(v.into());
        Named { name: name_of("separation.r2prime_strategy", Mapping::Same, &value), value }
    };
    let method = serde_json::to_value(s.algorithm).unwrap_or(Value::Null);
    vec![Recommendation {
        rule: "qsmxt.chi-sep-predict-r2prime".into(),
        kind: Kind::Reasoned,
        strength: Strength::Recommended,
        basis: Basis::Qsmxt,
        setting: "separation.r2prime_strategy".into(),
        label: label_of("separation.r2prime_strategy"),
        trigger: Trigger {
            setting: "separation.algorithm".into(),
            name: name_of("separation.algorithm", Mapping::Same, &method),
            value: method,
        },
        current: vec![named("mese")],
        recommended: named("auto"),
        allowed: Allowed::OneOf { values: vec![named("auto"), named("r2primenet")] },
        reason: "This scan has no measured R2' (no multi-echo spin-echo acquisition and no supplied \
                 map), and the χ-separation toolbox only takes a measured one. QSMxT can instead \
                 predict R2' from R2* with R2PRIMEnet (Kim et al., 2025): a QSMxT recommendation, \
                 not part of the toolbox's pipeline".into(),
        source: "QSMxT; Kim et al. 2025 (R2PRIMEnet)".into(),
        needs_scan: true,
        departs: true,
        dismissed: false,
        conflict: false,
    }]
}

/// `qsmxt.bipolar-signature`: the echoes' phase shows a bipolar readout's odd/even pattern
/// (QSM.rs's `bipolar_signature`, measured by a front end that holds the data:
/// [`Facts::bipolar_signature`]) while the sidecar does not say the readout polarity. Bipolar
/// correction is then suggested, not preselected: the signature is measured on the data, and a
/// monopolar scan with an unusual odd/even phase (or data corrected already) could show it too.
fn bipolar_signature_finding(c: &PipelineConfig, facts: &Facts) -> Option<Recommendation> {
    if facts.bipolar_signature != Some(true) || facts.readout_polarity.is_some() || c.field_mapping.bipolar_correction {
        return None;
    }
    // In play as for a bipolar readout: what `scan_adjustments` would turn on.
    let mut probe = c.clone();
    crate::normalise::scan_adjustments(&mut probe, Some(crate::scan::ReadoutPolarity::Bipolar), facts.echo_count)
        .iter().find(|a| a.rule == crate::normalise::BIPOLAR_READOUT_RULE)?;
    let named = |on: bool| Named { value: Value::Bool(on), name: if on { "on" } else { "off" }.into() };
    Some(Recommendation {
        rule: BIPOLAR_SIGNATURE_RULE.into(),
        kind: Kind::Reasoned,
        strength: Strength::Suggested,
        basis: Basis::Measured,
        setting: "field_mapping.bipolar_correction".into(),
        label: label_of("field_mapping.bipolar_correction"),
        trigger: Trigger {
            setting: crate::normalise::READOUT_FACT.into(),
            value: Value::String("bipolar-signature".into()),
            name: "the echoes' bipolar readout signature".into(),
        },
        current: vec![named(false)],
        recommended: named(true),
        allowed: Allowed::OneOf { values: vec![named(true)] },
        reason: "The echoes' phase alternates between odd and even echoes as a bipolar (alternating-polarity) \
                 readout makes it, and the sidecar does not say the readout polarity: if the scan's readout \
                 is bipolar, its field map needs bipolar correction".into(),
        source: "QSM.rs utils::bipolar_signature; QSMxT/QSMxT#564".into(),
        needs_scan: true,
        departs: true,
        dismissed: false,
        conflict: false,
    })
}

/// The id of QSMxT's suggestion from the echoes' bipolar signature ([`bipolar_signature_finding`]).
pub const BIPOLAR_SIGNATURE_RULE: &str = "qsmxt.bipolar-signature";

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
        // A trigger is a choice (QSM.rs's rules), so these do not occur.
        Condition::Fact { .. } | Condition::Unless { .. } => Trigger { setting: String::new(), value: Value::Null, name: String::new() },
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
        "inversion.apply_heidi" => "HEIDI streaking reduction".to_string(),
        _ => format!("{} {}", in_sentence(&label_of(core_path)), value_name),
    }
}

/// A setting's label inside a sentence: lower case, unless it opens with a name in capitals
/// ("UK Biobank mask trim").
fn in_sentence(label: &str) -> String {
    let mut c = label.chars();
    match (c.next(), c.next()) {
        (Some(a), Some(b)) if a.is_uppercase() && b.is_uppercase() => label.to_string(),
        _ => label.to_lowercase(),
    }
}

/// Mark each finding dismissed (by `config`'s dismissals) or in conflict with another.
fn mark(config: &PipelineConfig, out: &mut [Recommendation]) {
    for r in out.iter_mut() {
        r.dismissed = r.departs && r.strength.dismissable() && config.dismissed_recommendations.iter()
            .any(|d| d.rule == r.rule && d.chosen == chosen_text(&r.current));
    }
    // By QSM.rs's setting, as [`settle`] groups them.
    let snapshot: Vec<(String, Strength, Value, bool)> =
        out.iter().map(|r| (rule_setting(r).to_string(), r.strength, r.recommended.value.clone(), r.departs)).collect();
    let all = out.to_vec();
    for (i, r) in out.iter_mut().enumerate() {
        let key = rule_setting(r).to_string();
        r.conflict = r.departs && (snapshot.iter().enumerate().any(|(j, (s, st, v, d))| {
            j != i && *d && *s == key && *st == r.strength && *v != r.recommended.value
        }) || !disagreeing(&all, r).is_empty());
    }
}

/// `other` is another chosen method's authors' setting that `r`'s, on the same setting, disagrees
/// with: both are `reference` rules of different choices, they want different values, and
/// `other` already departs or would depart once `r`'s value is applied. PDF's authors use the
/// complex fit, AMP-PE's average φ/TE over the echoes: with both chosen, following one departs
/// from the other.
fn disagrees(other: &Recommendation, r: &Recommendation) -> bool {
    other.rule != r.rule && other.strength == Strength::Reference && r.strength == Strength::Reference
        && other.trigger != r.trigger && rule_setting(other) == rule_setting(r)
        && other.recommended.value != r.recommended.value
        && (other.departs || !allows(&other.allowed, &r.recommended.value))
}

/// `value` passes what a rule expects ([`Allowed`]).
fn allows(allowed: &Allowed, value: &Value) -> bool {
    match allowed {
        Allowed::OneOf { values } => values.iter().any(|v| v.value == *value),
        Allowed::NoneOf { values } => !values.iter().any(|v| v.value == *value),
        Allowed::Range { min, max } => value.as_f64().is_some_and(|x| {
            min.as_ref().and_then(Value::as_f64).is_none_or(|m| x >= m) && max.as_ref().and_then(Value::as_f64).is_none_or(|m| x <= m)
        }),
    }
}

/// The other chosen methods' authors' settings in `view` ([`reference_values`]) that disagree
/// with `r`'s on its setting, followed or not: what a front end offers beside `r`, as one choice
/// between the authors' values. Empty for a reasoned finding, or when no other chosen method's
/// authors say otherwise.
pub fn disagreeing<'a>(view: &'a [Recommendation], r: &Recommendation) -> Vec<&'a Recommendation> {
    view.iter().filter(|o| disagrees(o, r)).collect()
}

/// Whose authors' settings disagree with `r`'s, and on what, in a line: "PDF's authors use
/// Complex fit; AMP-PE's authors use Weighted average". Empty when none does ([`disagreeing`]).
pub fn disagreement(view: &[Recommendation], r: &Recommendation) -> String {
    let others = disagreeing(view, r);
    if others.is_empty() {
        return String::new();
    }
    std::iter::once(r).chain(others).map(|o| format!("{}'s authors use {}", o.trigger.name, o.recommended.name))
        .collect::<Vec<_>>().join("; ")
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
