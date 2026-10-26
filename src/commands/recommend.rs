//! What `qsmxt run` and `qsmxt slurm` say about the pipeline rules' findings
//! ([`qsmxt_engine::recommendations`]): at plan time, in `--dry`, applied with
//! `--accept-recommendations`, and in the end-of-run summary. No setting changes without
//! `--accept-recommendations`.

use std::collections::BTreeMap;

use log::{info, warn};
use qsmxt_engine::recommendations::{self as rec, Facts, Recommendation, Strength};

use crate::bids::discovery::QsmRun;
use crate::pipeline::config::PipelineConfig;

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

/// Every finding on `config` across the dataset's runs, each once (by rule and current value),
/// with the runs it was found for.
pub(crate) fn found(config: &PipelineConfig, runs: &[QsmRun]) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for (key, f) in facts(runs, config) {
        for r in rec::recommendations(config, &f) {
            match out.iter_mut().find(|o| o.finding.rule == r.rule && o.finding.current == r.current) {
                Some(o) => o.runs.push(key.clone()),
                None => out.push(Found { finding: r, runs: vec![key.clone()] }),
            }
        }
    }
    out
}

/// What `--accept-recommendations` did.
#[derive(Default)]
pub(crate) struct Accepted {
    /// The findings applied, as they were before.
    pub applied: Vec<Recommendation>,
    /// The findings applied that each run had, by run key: what its run record keeps.
    pub per_run: std::collections::HashMap<String, Vec<Recommendation>>,
}

/// Apply every finding `--accept-recommendations` takes (QSMxT's engine decides which:
/// [`rec::accept`]), over every run's scan, and say what changed.
pub(crate) fn accept(config: &mut PipelineConfig, runs: &[QsmRun]) -> Accepted {
    let before = config.clone();
    let all: Vec<Recommendation> = found(config, runs).into_iter().map(|f| f.finding).collect();
    let (applied, conflicting) = rec::accept(config, &all);
    let mut per_run = std::collections::HashMap::new();
    for (key, f) in facts(runs, &before) {
        let had: Vec<Recommendation> = rec::recommendations(&before, &f).into_iter()
            .filter(|r| applied.iter().any(|a| a.rule == r.rule)).collect();
        per_run.insert(key, had);
    }
    for r in &applied {
        info!("Applied ({}): {}: {} → {} [{}]", r.strength.label(), r.label, r.current_text(), r.recommended.name, r.rule);
    }
    for r in &conflicting {
        warn!("Not applied: {} ({}), which another as strong recommendation disagrees with: {}", r.rule, r.strength.label(), r.line());
    }
    Accepted { applied, per_run }
}

/// How many of the runs, when not all.
fn of_runs(f: &Found, total: usize) -> String {
    if f.runs.len() < total { format!(" ({} of {total} runs)", f.runs.len()) } else { String::new() }
}

/// The flag that dismisses a finding.
fn dismiss_flag(r: &Recommendation) -> String {
    format!("--dismiss-recommendation {}={}", r.rule, rec::chosen_text(&r.current))
}

/// The plan-time lines of `qsmxt run`: each finding not dismissed (a warning when it is offered
/// as a change, information for a reference one), and how many were dismissed. A required one is
/// refused by the run's checks ([`crate::pipeline::requirements::check_runs`]).
pub(crate) fn log_found(config: &PipelineConfig, runs: &[QsmRun]) {
    let found = found(config, runs);
    let mut dismissed = 0;
    for f in &found {
        let r = &f.finding;
        let at = of_runs(f, runs.len());
        match (r.dismissed, r.strength) {
            (true, _) => dismissed += 1,
            (false, Strength::Required) => {}
            (false, Strength::Reference) => info!("Differs from the reference pipeline{at}: {} ({})", r.line(), r.rule),
            (false, s) => warn!("{}{at}: {} ({}; --accept-recommendations applies it, {} keeps the choice)",
                                capitalised(s.label()), r.line(), r.rule, dismiss_flag(r)),
        }
    }
    if dismissed > 0 {
        info!("{dismissed} dismissed recommendation(s) not shown (qsmxt run --dry lists them)");
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// What `qsmxt run --dry` prints: every finding for the config and dataset, grouped by strength,
/// the dismissed ones apart; each covered setting's reference value; and with
/// `--accept-recommendations`, the settings that would change and the resulting command.
pub(crate) fn print_dry(config: &PipelineConfig, runs: &[QsmRun], accept: bool) {
    print!("{}", dry_text(config, runs, accept));
}

/// [`print_dry`]'s text.
pub(crate) fn dry_text(config: &PipelineConfig, runs: &[QsmRun], accept: bool) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let found = found(config, runs);
    out.push('\n');
    if found.is_empty() {
        let _ = writeln!(out, "Recommendations: none (the configuration departs from nothing the chosen methods' rules cover)");
    } else {
        let _ = writeln!(out, "Recommendations (QSM.rs's pipeline rules for the chosen methods):");
        let groups: [(&str, Box<dyn Fn(&Recommendation) -> bool>); 5] = [
            ("required (the run is refused as configured)", Box::new(|r| r.strength == Strength::Required)),
            ("recommended", Box::new(|r| !r.dismissed && r.strength == Strength::Recommended)),
            ("discouraged", Box::new(|r| !r.dismissed && r.strength == Strength::Discouraged)),
            ("reference (information only)", Box::new(|r| !r.dismissed && r.strength == Strength::Reference)),
            ("dismissed", Box::new(|r| r.dismissed && r.strength != Strength::Required)),
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
                if r.strength.actionable() && r.strength != Strength::Required && !r.dismissed {
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
    if accept {
        let mut changed = config.clone();
        let all: Vec<Recommendation> = found.iter().map(|f| f.finding.clone()).collect();
        let (applied, conflicting) = rec::accept(&mut changed, &all);
        out.push('\n');
        if applied.is_empty() {
            let _ = writeln!(out, "--accept-recommendations would change nothing.");
        } else {
            let _ = writeln!(out, "With --accept-recommendations, the run would use:");
            for r in &applied {
                let _ = writeln!(out, "  {}: {} → {} ({})", r.label, r.current_text(), r.recommended.name, r.rule);
            }
            let _ = writeln!(out, "  i.e. {}", qsmxt_engine::generate_command(&changed));
        }
        for r in &conflicting {
            let _ = writeln!(out, "  not applied, as another equally strong recommendation disagrees: {} ({})", r.line(), r.rule);
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
        out.push(format!("  applied ({}): {}: {} (was {}) [{}]", r.strength.label(), r.label, r.recommended.name, r.current_text(), r.rule));
    }
    let wants = |r: &Recommendation| match r.strength {
        Strength::Reference => format!("the {} reference pipeline uses {}", r.trigger.name, r.recommended.name),
        Strength::Discouraged => format!("which is discouraged with {}", r.trigger.name),
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
                         r.strength.label(), r.label, r.current_text(), wants(r), of_runs(f, runs.len()), r.rule));
    }
    out
}
