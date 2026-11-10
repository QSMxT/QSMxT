//! What `qsmxt run` and `qsmxt slurm` say about the pipeline rules' findings
//! ([`qsmxt_engine::recommendations`]): at plan time, in `--dry`, applied with
//! `--accept-recommendations` (the reasoned recommendations) or `--accept-reference-settings`
//! (the original authors' settings), and in the end-of-run summary. No setting changes without
//! one of them. The two kinds are said apart everywhere: "Recommended" (with its reason) or
//! "Original authors' setting".

use std::collections::BTreeMap;

use log::{info, warn};
use qsmxt_engine::recommendations::{self as rec, Accept, Facts, Kind, Recommendation, Strength};

use crate::bids::discovery::QsmRun;
use crate::pipeline::config::PipelineConfig;

/// What `--accept-recommendations` and `--accept-reference-settings` ask to apply.
pub(crate) fn which(recommendations: bool, reference: bool) -> Accept {
    Accept { recommendations, reference }
}

/// A finding over the dataset: found for these runs (all of them when it does not depend on the
/// scan).
pub(crate) struct Found {
    pub finding: Recommendation,
    pub runs: Vec<String>,
}

/// Each run's facts for the rules, by run key.
fn facts(runs: &[QsmRun], config: &PipelineConfig) -> Vec<(String, Facts)> {
    runs.iter().map(|r| (r.key.to_string(), Facts::of_scan(&crate::pipeline::requirements::scan_facts(r, config)))).collect()
}

/// Every finding on `config` across the dataset's runs (each run's `--pipeline` completed on its
/// scan, as it will run), each once (by rule and current value), with the runs it was found for.
pub(crate) fn found(config: &PipelineConfig, runs: &[QsmRun]) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for (run, (key, f)) in runs.iter().zip(facts(runs, config)) {
        // Each run's config as it will run: a `--pipeline` completed on its scan.
        let completed = crate::pipeline::requirements::run_config(run, config).map(|(c, _)| c);
        for r in rec::recommendations(completed.as_ref().unwrap_or(config), &f) {
            match out.iter_mut().find(|o| o.finding.rule == r.rule && o.finding.current == r.current) {
                Some(o) => o.runs.push(key.clone()),
                None => out.push(Found { finding: r, runs: vec![key.clone()] }),
            }
        }
    }
    out
}

/// What `--accept-recommendations` / `--accept-reference-settings` did.
#[derive(Default)]
pub(crate) struct Accepted {
    /// The findings applied, as they were before.
    pub applied: Vec<Recommendation>,
    /// The findings applied that each run had, by run key: what its run record keeps.
    pub per_run: std::collections::HashMap<String, Vec<Recommendation>>,
}

/// The findings `which` applies, applied to `config` until none is left: a value applied can
/// choose a method whose own rules then apply (the authors' complex fit for MEDI, then the
/// unwrapper the authors pair with it). Returns the findings applied, each once, and those left
/// because they conflict.
fn accept_all(config: &mut PipelineConfig, runs: &[QsmRun], which: Accept) -> (Vec<Recommendation>, Vec<Recommendation>) {
    let mut applied: Vec<Recommendation> = Vec::new();
    let mut conflicting = Vec::new();
    // A few rounds at most: each round only applies values that rules ask for, and a rule once
    // followed no longer fires.
    for _ in 0..4 {
        let all: Vec<Recommendation> = found(config, runs).into_iter().map(|f| f.finding).collect();
        let (now, left) = rec::accept(config, &all, which);
        conflicting = left;
        let new: Vec<Recommendation> = now.into_iter().filter(|r| !applied.iter().any(|a| a.rule == r.rule)).collect();
        if new.is_empty() {
            break;
        }
        applied.extend(new);
    }
    (applied, conflicting)
}

/// Apply every finding `--accept-recommendations` and `--accept-reference-settings` take
/// (`which`; QSMxT's engine decides which findings: [`rec::accept`]), over every run's scan, and
/// say what changed.
pub(crate) fn accept(config: &mut PipelineConfig, runs: &[QsmRun], which: Accept) -> Accepted {
    let before = config.clone();
    let (applied, conflicting) = accept_all(config, runs, which);
    let mut per_run = std::collections::HashMap::new();
    for (key, f) in facts(runs, &before) {
        let had: Vec<Recommendation> = rec::recommendations(&before, &f).into_iter()
            .filter(|r| applied.iter().any(|a| a.rule == r.rule)).collect();
        per_run.insert(key, had);
    }
    for r in &applied {
        info!("Applied ({}): {}: {} → {} [{}]", applied_label(r), r.label, r.current_text(), r.recommended.name, r.rule);
    }
    for r in &conflicting {
        warn!("Not applied: {} ({}), which another as strong suggestion disagrees with: {}", r.rule, applied_label(r), r.line());
    }
    Accepted { applied, per_run }
}

/// What kind of finding was applied: "recommended", "discouraged", "required" or "original
/// authors' setting".
fn applied_label(r: &Recommendation) -> String {
    match r.kind {
        Kind::Reference => Kind::Reference.label().to_lowercase(),
        Kind::Reasoned => r.strength.label().to_string(),
    }
}

/// How many of the runs, when not all.
fn of_runs(f: &Found, total: usize) -> String {
    if f.runs.len() < total { format!(" ({} of {total} runs)", f.runs.len()) } else { String::new() }
}

/// The flag that dismisses a finding.
fn dismiss_flag(r: &Recommendation) -> String {
    format!("--dismiss-recommendation {}={}", r.rule, rec::chosen_text(&r.current))
}

/// The plan-time lines of `qsmxt run`: every departure the run keeps, a warning for a
/// recommended or discouraged one (dismissed or not) and information for a difference from the
/// original authors' setting. A required one is refused by the run's checks
/// ([`crate::pipeline::requirements::check_runs`]).
pub(crate) fn log_found(config: &PipelineConfig, runs: &[QsmRun]) {
    for line in found_lines(config, runs) {
        match line {
            (true, l) => warn!("{l}"),
            (false, l) => info!("{l}"),
        }
    }
}

/// [`log_found`]'s lines, each with whether it is a warning.
pub(crate) fn found_lines(config: &PipelineConfig, runs: &[QsmRun]) -> Vec<(bool, String)> {
    found(config, runs).iter().filter_map(|f| {
        let r = &f.finding;
        let at = of_runs(f, runs.len());
        match (r.strength, r.dismissed) {
            (Strength::Required, _) => None,
            (Strength::Reference, _) => Some((false, format!("Differs from the original authors' setting{at}: {} ({}; --accept-reference-settings applies it)", r.line(), r.rule))),
            (s, true) => Some((true, format!("{}{at}: {} ({}; dismissed, kept)", capitalised(s.label()), r.line(), r.rule))),
            (s, false) => Some((true, format!("{}{at}: {} ({}; --accept-recommendations applies it)",
                                             capitalised(s.label()), r.line(), r.rule))),
        }
    }).collect()
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// What `qsmxt run --dry` prints: every finding for the config and dataset, grouped by strength,
/// the dismissed ones apart; each covered setting's reference value; and with
/// `--accept-recommendations` or `--accept-reference-settings`, the settings that would change
/// and the resulting command.
pub(crate) fn print_dry(config: &PipelineConfig, runs: &[QsmRun], accept: Accept) {
    print!("{}", dry_text(config, runs, accept));
}

/// The flags `which` stands for, as the dry run names them.
fn flags(which: Accept) -> String {
    let mut f = Vec::new();
    if which.recommendations {
        f.push("--accept-recommendations");
    }
    if which.reference {
        f.push("--accept-reference-settings");
    }
    f.join(" ")
}

/// [`print_dry`]'s text.
pub(crate) fn dry_text(config: &PipelineConfig, runs: &[QsmRun], accept: Accept) -> String {
    use std::fmt::Write;
    // As the runs will run: a `--pipeline` completed on their scans.
    let config = &crate::pipeline::requirements::planned_config(config, runs);
    let mut out = String::new();
    let found = found(config, runs);
    out.push('\n');
    if found.is_empty() {
        let _ = writeln!(out, "Recommendations: none (the configuration departs from nothing the chosen methods' rules cover)");
    } else {
        let _ = writeln!(out, "Recommendations (QSM.rs's pipeline rules for the chosen methods):");
        type Belongs = Box<dyn Fn(&Recommendation) -> bool>;
        let groups: [(&str, Belongs); 6] = [
            ("required (the run is refused as configured)", Box::new(|r| r.strength == Strength::Required)),
            ("recommended (--accept-recommendations applies them)", Box::new(|r| !r.dismissed && r.strength == Strength::Recommended)),
            ("discouraged (--accept-recommendations applies the alternative)", Box::new(|r| !r.dismissed && r.strength == Strength::Discouraged)),
            ("suggested (better on some data, e.g. in vivo; --accept-recommendations leaves them)", Box::new(|r| !r.dismissed && r.strength == Strength::Suggested)),
            ("original authors' settings (not recommendations; --accept-reference-settings applies them)", Box::new(|r| r.kind == Kind::Reference)),
            ("dismissed (kept)", Box::new(|r| r.dismissed && r.strength.dismissable())),
        ];
        for (heading, belongs) in &groups {
            let members: Vec<&Found> = found.iter().filter(|f| belongs(&f.finding)).collect();
            if members.is_empty() {
                continue;
            }
            let _ = writeln!(out, "  {heading}:");
            for f in members {
                let r = &f.finding;
                let _ = writeln!(out, "    - {}{} ({}, {})", r.line(), of_runs(f, runs.len()), r.rule, r.basis.label());
                if r.strength.dismissable() && !r.dismissed {
                    let _ = writeln!(out, "      keep the choice with {}", dismiss_flag(r));
                }
            }
        }
    }
    // Every covered setting's reference value, departed from or not.
    let mut references: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let first = facts(runs, config).into_iter().next().map(|(_, f)| f).unwrap_or_default();
    for r in rec::reference_values(config, &first) {
        let mark = if r.departs { " (differs)" } else { "" };
        let line = format!("{}: {} — {} uses {}{mark} [{}]", r.label, r.current_text(), r.trigger.name, r.recommended.name, r.source);
        let lines = references.entry(r.setting.clone()).or_default();
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    if !references.is_empty() {
        out.push('\n');
        let _ = writeln!(out, "Reference values of the chosen methods (for the first run's scan):");
        for line in references.values().flatten() {
            let _ = writeln!(out, "  {line}");
        }
    }
    if accept.any() {
        let mut changed = config.clone();
        let (applied, conflicting) = accept_all(&mut changed, runs, accept);
        let flags = flags(accept);
        out.push('\n');
        if applied.is_empty() {
            let _ = writeln!(out, "{flags} would change nothing.");
        } else {
            let _ = writeln!(out, "With {flags}, the run would use:");
            for r in &applied {
                let _ = writeln!(out, "  {}: {} → {} ({}; {})", r.label, r.current_text(), r.recommended.name, applied_label(r), r.rule);
            }
            let _ = writeln!(out, "  i.e. {}", qsmxt_engine::generate_command(&changed));
        }
        for r in &conflicting {
            let _ = writeln!(out, "  not applied, as another equally strong suggestion disagrees: {} ({})", r.line(), r.rule);
        }
    }
    out
}

/// The end-of-run summary: what was applied, what was dismissed, and the departures from the
/// reference pipelines that were kept. Nothing when there were no findings.
pub(crate) fn log_summary(config: &PipelineConfig, runs: &[QsmRun], accepted: &Accepted) {
    for line in summary(config, runs, accepted) {
        info!("{line}");
    }
}

/// [`log_summary`]'s lines.
pub(crate) fn summary(config: &PipelineConfig, runs: &[QsmRun], accepted: &Accepted) -> Vec<String> {
    let found = found(config, runs);
    if accepted.applied.is_empty() && found.is_empty() {
        return vec![];
    }
    let mut out = vec!["Recommendations:".to_string()];
    for r in &accepted.applied {
        out.push(format!("  applied ({}): {}: {} (was {}) [{}]", applied_label(r), r.label, r.recommended.name, r.current_text(), r.rule));
    }
    let wants = |r: &Recommendation| match r.strength {
        Strength::Reference => format!("the original authors' setting ({}) is {}", r.trigger.name, r.recommended.name),
        Strength::Discouraged => format!("which is discouraged with {}", r.trigger.name),
        Strength::Suggested => format!("{} suggests {}", r.trigger.name, r.recommended.name),
        _ => format!("{} recommends {}", r.trigger.name, r.recommended.name),
    };
    for f in found.iter().filter(|f| f.finding.dismissed) {
        let r = &f.finding;
        out.push(format!("  dismissed ({}): {}: kept {}; {}{} [{}]",
                         r.strength.label(), r.label, r.current_text(), wants(r), of_runs(f, runs.len()), r.rule));
    }
    for f in found.iter().filter(|f| !f.finding.dismissed) {
        let r = &f.finding;
        out.push(format!("  kept ({}): {}: {}; {}{} [{}]",
                         applied_label(r), r.label, r.current_text(), wants(r), of_runs(f, runs.len()), r.rule));
    }
    out
}
