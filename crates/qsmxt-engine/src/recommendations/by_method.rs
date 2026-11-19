//! Findings by method: each chosen method's reference pipeline, and a setting's marker.

use super::*;

// ─── By method ───

/// One method's reference pipeline as far as it covers a config: the `reference` rules scoped to
/// that method (their trigger), departed from or not. What a front end opens when the method is
/// chosen ([`chosen_methods`], [`reference_differences`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodReference {
    /// The choice the rules are scoped to ("MEDI").
    pub method: Trigger,
    /// "MEDI's reference pipeline".
    pub title: String,
    pub rules: Vec<Recommendation>,
}

impl MethodReference {
    /// The rules the config departs from and has not kept ([`keep`]).
    pub fn differences(&self) -> Vec<Recommendation> {
        self.rules.iter().filter(|r| r.departs && !r.dismissed).cloned().collect()
    }
}

/// What a pop-up about `method`'s reference pipeline is called: "MEDI's reference pipeline" for a
/// method, "The reference for phase offset removal on" for another setting's value.
pub fn reference_title(method: &Trigger) -> String {
    if METHOD_SETTINGS.contains(&method.setting.as_str()) {
        format!("{}'s reference pipeline", method.name)
    } else {
        format!("The reference for {}", method.name)
    }
}

/// The `reference` rules of `view` ([`reference_values`]) grouped by the choice they are scoped
/// to, in the order the choices first appear.
pub fn by_method(view: &[Recommendation]) -> Vec<MethodReference> {
    let mut out: Vec<MethodReference> = Vec::new();
    for r in view.iter().filter(|r| r.strength == Strength::Reference && !r.trigger.setting.is_empty()) {
        match out.iter_mut().find(|m| m.method == r.trigger) {
            Some(m) => m.rules.push(r.clone()),
            None => out.push(MethodReference { method: r.trigger.clone(), title: reference_title(&r.trigger), rules: vec![r.clone()] }),
        }
    }
    out
}

/// The QSMxT settings a choice of which can open a reference pipeline pop-up: those that trigger
/// a `reference` rule (the inversion, background removal, χ-separation, phase offset removal),
/// from QSM.rs's rules. A front end shows them as a picker
/// rather than cycling through their values, so no method is chosen on the way to another.
pub fn reference_trigger_settings() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for r in core::RULES.iter().filter(|r| r.strength == core::Strength::Reference) {
        let Condition::Setting { setting, .. } = r.trigger() else { continue };
        if let Some(path) = qsmxt_path(setting) {
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// The choices in `config` that have a reference pipeline: the triggers of the `reference` rules
/// that apply. A front end compares them before and after a change; a choice that appears is a
/// method just chosen, and its [`reference_differences`] are what to offer.
pub fn chosen_methods(config: &PipelineConfig, facts: &Facts) -> Vec<Trigger> {
    by_method(&reference_values(config, facts)).into_iter().map(|m| m.method).collect()
}

/// Where `config` differs from `method`'s reference pipeline and has not kept its own value
/// ([`keep`]): each setting's current value, the reference value, why and the source. Empty when
/// it does not differ, or `method` is not chosen.
pub fn reference_differences(config: &PipelineConfig, facts: &Facts, method: &Trigger) -> Vec<Recommendation> {
    by_method(&reference_values(config, facts)).into_iter()
        .find(|m| m.method == *method)
        .map(|m| m.differences())
        .unwrap_or_default()
}

/// The marker a setting shows when it departs from a rule ([`marker`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// A finding offered as a change and not dismissed (⚑); otherwise information only (ⓘ): a
    /// `reference` difference, or a finding kept.
    pub actionable: bool,
    /// Whose rule and what it uses: "MEDI: BET", "MEDI, PDF: BET", "MEDI: PDF · TFI: LBV".
    pub text: String,
    /// What a front end shows beside the setting: [`Self::text`] while it is a few words
    /// ([`MARKER_SHORT_MAX`] characters), else only whose rules they are ("iLSQR", "MEDI, PDF");
    /// the details pop-up says the rest.
    pub short: String,
}

/// The most characters a marker's [`Marker::short`] text takes beside its setting.
pub const MARKER_SHORT_MAX: usize = 24;

/// The marker for the setting at `path`, from the rules of `view` ([`reference_values`]) that it
/// departs from: `None` when it departs from none. Offered findings are named first.
pub fn marker(view: &[Recommendation], path: &str) -> Option<Marker> {
    let mut here: Vec<&Recommendation> = view.iter().filter(|r| r.setting == path && r.departs).collect();
    if here.is_empty() {
        return None;
    }
    let open = |r: &Recommendation| r.actionable() && !r.dismissed;
    here.sort_by_key(|r| !open(r));
    // Each value, with every method whose rule has it.
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for r in &here {
        let value = if r.strength == Strength::Discouraged { format!("not {}", r.current_text()) } else { r.recommended.name.clone() };
        let who = if r.trigger.name.is_empty() { "Reference".to_string() } else { r.trigger.name.clone() };
        match groups.iter_mut().find(|(v, _)| *v == value) {
            Some((_, names)) if names.contains(&who) => {}
            Some((_, names)) => names.push(who),
            None => groups.push((value, vec![who])),
        }
    }
    // The values of one set of methods together: a mask recipe's parts ("QSMART: BET,
    // magnitude-first, no erosion") read as one.
    let mut by_methods: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for (value, names) in groups {
        match by_methods.iter_mut().find(|(n, _)| *n == names) {
            Some((_, values)) => values.push(value),
            None => by_methods.push((names, vec![value])),
        }
    }
    let text = by_methods.iter().map(|(names, values)| format!("{}: {}", names.join(", "), values.join(", ")))
        .collect::<Vec<_>>().join(" · ");
    let short = if text.chars().count() <= MARKER_SHORT_MAX {
        text.clone()
    } else {
        let mut names: Vec<&str> = Vec::new();
        for n in by_methods.iter().flat_map(|(n, _)| n) {
            if !names.contains(&n.as_str()) {
                names.push(n);
            }
        }
        let all = names.join(", ");
        if all.chars().count() <= MARKER_SHORT_MAX || names.len() == 1 { all } else { format!("{} +{}", names[0], names.len() - 1) }
    };
    Some(Marker { actionable: here.iter().any(|r| open(r)), text, short })
}

/// Keep the setting of each finding at its current value: dismiss each for that value
/// ([`Dismissal`], into a config's `dismissed_recommendations`), so it is not offered again while
/// the setting keeps it. A `required` finding cannot be kept and is left; a `reference` one is
/// information only and records nothing. What "Keep mine" does.
pub fn keep(dismissals: &mut Vec<Dismissal>, findings: &[Recommendation]) {
    for r in findings.iter().filter(|r| r.strength.dismissable()) {
        dismissals.retain(|d| d.rule != r.rule);
        dismissals.push(Dismissal { rule: r.rule.clone(), chosen: chosen_text(&r.current) });
    }
}

/// Apply the findings the user chose together: what "Apply all" does, with the precedence
/// [`settle`] gives (the same as [`accept`]'s), so the order of `findings` does not matter. A
/// finding left by that precedence changes nothing and is not an error: the returned [`Settled`]
/// says which findings were applied and which were left, and the setting keeps its value and goes
/// on showing them. A finding on a setting that does not apply on a scan with `facts`
/// ([`rule_may_change`]) is not taken, nor listed as left. Every finding `settle` takes is tried;
/// `Err` lists those that could not be applied.
pub fn apply_all(config: &mut PipelineConfig, findings: &[Recommendation], facts: &Facts) -> Result<Settled, String> {
    let shown = |r: &Recommendation| rule_may_change(config, &r.setting, facts);
    let settled = settle_where(findings, shown);
    let errors: Vec<String> = settled.apply.iter().filter_map(|r| apply_rule(config, &r.rule, None).err()).collect();
    if errors.is_empty() { Ok(settled) } else { Err(errors.join("; ")) }
}
