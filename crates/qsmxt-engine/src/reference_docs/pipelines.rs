//! The toolbox pipeline pages: one generated page per toolbox pipeline
//! (`toolbox-pipelines/<id>.md`) and the table of them on the overview page, from QSM.rs's
//! catalogue (`qsm_core::pipeline::reference_pipelines`) and the settings each pipeline resolves
//! to in QSMxT ([`crate::pipelines`]), so a page can never disagree with what `--pipeline` sets.
//!
//! - **Settings are what `--pipeline` sets, relative to QSMxT's defaults** ([`crate::pipelines::changes`]
//!   from a default config), each with the reason of the QSM.rs rule that sets it. They are
//!   resolved twice: with no scan (what the command line sets before it reads the data) and on a
//!   typical scan ([`SCAN`]), and a setting only the scan brings in is shown with the scan
//!   conditions of its rule ("for scans with two or more echoes").
//! - **Parity is QSM-CI's measured result**, shown as its badge for the pipeline's pair
//!   (`ReferencePipeline::qsmci_pair`). The pages make no claim that a pipeline gives the
//!   toolbox's results, and list no "not reproduced" items (decision 217): only the deliberate
//!   differences QSM.rs records, with their reasons, and its known gaps.

use qsm_core::pipeline::reference_pipelines::{ReferencePipeline, DIFFERENCES, REFERENCE_PIPELINES};
use qsm_core::pipeline::rules::{self as core, Condition, Fact, Test};
use serde_json::Value;

use super::{cell, code, sentence, table};
use crate::config::PipelineConfig;
use crate::methods::citations::{reference_text, CITE_QSMXT};
use crate::pipelines::{self, Change};
use crate::recommendations::{self as rec, Facts, Strength};
use crate::schema::{self, ViewFacts};

/// The toolbox pipelines' pages, relative to the repository root: the overview (`index.md`) and
/// one `<id>.md` per pipeline.
pub const DIR: &str = "docs/src/content/docs/toolbox-pipelines";
/// The overview page, which holds the generated table of pipelines.
pub const OVERVIEW: &str = "docs/src/content/docs/toolbox-pipelines/index.md";

/// The site's base path, for links between pages.
const BASE: &str = "/QSMxT";
const PARITY_PAGE: &str = "https://qsmxt.github.io/QSM-CI/parity.html";

const REGENERATE: &str = "QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs";

/// The scan the pages resolve scan-dependent settings on: multi-echo 3 T data from a vendor no
/// rule singles out.
fn scan() -> ViewFacts {
    ViewFacts {
        num_echoes: Some(4),
        field_strength: Some(3.0),
        manufacturer: Some("Siemens".into()),
        ..Default::default()
    }
}

/// The issues behind what a page links to that QSM.rs records only in its comments: a known
/// gap's issue (`ReferencePipeline::note`), and the QSM-CI issue for a pipeline with no parity
/// pair yet. `(pipeline id, repository, issue number)`.
const GAP_ISSUES: &[(&str, &str, u32)] = &[
    ("medi-toolbox", "QSMxT/QSMxT", 564),
    ("sti-suite-star", "astewartau/QSM.rs", 493),
    ("uk-biobank", "astewartau/QSM.rs", 491),
];
const PARITY_ISSUES: &[(&str, &str, u32)] = &[
    ("sti-suite-star", "QSMxT/QSM-CI", 469),
    ("chi-sepnet", "QSMxT/QSM-CI", 470),
];

fn issue(table: &[(&str, &'static str, u32)], id: &str) -> Option<String> {
    table.iter().find(|(p, ..)| *p == id).map(|(_, repo, n)| {
        let short = repo.rsplit('/').next().unwrap_or(repo);
        format!("[{short}#{n}](https://github.com/{repo}/issues/{n})")
    })
}

/// The file a pipeline's page is in, relative to the repository root.
pub fn page_path(p: &ReferencePipeline) -> String {
    format!("{DIR}/{}.md", p.id)
}

fn page_url(p: &ReferencePipeline) -> String {
    format!("{BASE}/toolbox-pipelines/{}/", p.id)
}

/// The QSM-CI parity badge of a pair, linked to its row on the parity page.
fn badge(pair: &str) -> String {
    let json = format!("https://qsmxt.github.io/QSM-CI/parity/badges/{pair}.json");
    let encoded = json.replace(':', "%3A").replace('/', "%2F");
    format!("[![parity](https://img.shields.io/endpoint?url={encoded})]({PARITY_PAGE}#pair-{pair})")
}

// ─── Settings ───

/// One setting a pipeline sets differently from QSMxT's defaults, and why.
struct Step {
    change: Change,
    /// The scan conditions it is set on, where only a scan brings it in ("for scans with two or
    /// more echoes").
    on: Option<String>,
    /// The reasons of the rules that set it, or the pipeline's choice.
    why: Vec<String>,
}

/// A setting's value as a page shows it: a mask recipe as `--mask` writes it, a number with its
/// units.
fn value_text(path: &str, value: &Value) -> String {
    if path == "masking.sections" {
        if let Ok(sections) = serde_json::from_value::<Vec<crate::MaskSection>>(value.clone()) {
            if !sections.is_empty() {
                return sections.iter().map(|s| code(&s.compact_spec())).collect::<Vec<_>>().join(" and ");
            }
        }
    }
    // A whole number as a whole number (64, not 64.0).
    let whole = |v: &Value| match v.as_f64() {
        Some(x) if v.is_f64() && x.fract() == 0.0 && x.abs() < 1e15 => Value::from(x as i64),
        _ => v.clone(),
    };
    let value = &match value {
        Value::Array(xs) => Value::Array(xs.iter().map(whole).collect()),
        v => whole(v),
    };
    let text = cell(&pipelines::value_text(path, value));
    match (schema::setting(path).and_then(|s| s.units), value) {
        (Some(units), Value::Number(_) | Value::Array(_)) => format!("{text} {units}"),
        _ => text,
    }
}

/// What a scan condition of a rule asks, in words; `None` for a fact that is not the scan's.
fn fact_text(c: &Condition) -> Option<String> {
    let Condition::Fact { fact, is } = c else { return None };
    let names = |values: &[core::Value]| -> Vec<String> {
        values.iter().map(|v| match v {
            core::Value::Choice(id) => match *id {
                "ge" => "GE".to_string(),
                id => { let mut c = id.chars(); c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default() }
            },
            other => format!("{other:?}"),
        }).collect()
    };
    let number = |x: f64| if x == 2.0 { "two".to_string() } else { x.to_string() };
    match (fact, is) {
        (Fact::EchoCount, Test::Range { min: Some(m), max: None }) => Some(format!("with {} or more echoes", number(*m))),
        (Fact::EchoCount, Test::Range { min: None, max: Some(m) }) => Some(format!("with at most {m} echoes")),
        (Fact::FieldStrength, Test::Range { min: Some(m), max: None }) => Some(format!("at {m} T or more")),
        (Fact::FieldStrength, Test::Range { min: None, max: Some(m) }) => Some(format!("at up to {m} T")),
        (Fact::Manufacturer, Test::OneOf { values }) => Some(format!("from {}", names(values).join(" or "))),
        (Fact::Manufacturer, Test::NoneOf { values }) => Some(format!("not from {}", names(values).join(" or "))),
        _ => None,
    }
}

/// The scan conditions of the rules `ids`, in words ("for scans with two or more echoes").
fn scan_conditions(ids: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for rule in core::RULES.iter().filter(|r| ids.iter().any(|id| id == r.id)) {
        for t in rule.when.iter().filter_map(fact_text) {
            if !parts.contains(&t) {
                parts.push(t);
            }
        }
    }
    if parts.is_empty() { "depending on the scan".into() } else { format!("for scans {}", parts.join(", ")) }
}

/// The rules that give `p`'s value of the setting `path` in `target`: the authors' settings and
/// the reasoned rules the pipeline follows, that `target` follows.
fn rules_for(p: &ReferencePipeline, target: &PipelineConfig, facts: &ViewFacts, path: &str) -> Vec<rec::Recommendation> {
    let mut out: Vec<rec::Recommendation> = Vec::new();
    for r in rec::reference_values(target, &Facts::of_view(facts)) {
        let followed = r.strength == Strength::Reference || p.reasoned.contains(&r.rule.as_str());
        if r.setting == path && !r.departs && followed && !out.iter().any(|o| o.reason == r.reason) {
            out.push(r);
        }
    }
    out
}

/// Why `p` sets `path`, where no rule says: its own choices, and what QSMxT does with them.
fn choice_reason(p: &ReferencePipeline, path: &str) -> Option<&'static str> {
    if p.choose.iter().any(|(s, _)| rec::qsmxt_path(s) == Some(path)) {
        return Some("One of the choices that start this pipeline.");
    }
    match path {
        "pipeline.do_chi_separation" => Some("A χ-separation method runs with χ-separation on."),
        "separation.r2prime_strategy" => Some("The χ-separation toolbox takes a measured R2′, from a multi-echo spin-echo scan or a supplied map."),
        _ => None,
    }
}

/// The settings `p` sets differently from QSMxT's defaults, in settings order, each with why.
fn steps(p: &ReferencePipeline) -> Vec<Step> {
    let defaults = PipelineConfig::default();
    let blind = ViewFacts::default();
    let scan = scan();
    let before = pipelines::changes(&defaults, p.id, &blind).unwrap_or_else(|e| panic!("{}: {e}", p.id));
    let target = pipelines::target(&defaults, p.id, &scan).unwrap_or_else(|e| panic!("{}: {e}", p.id));
    pipelines::changes(&defaults, p.id, &scan).unwrap_or_else(|e| panic!("{}: {e}", p.id)).into_iter().map(|change| {
        // A setting that is part of another's value (the Laplacian solver's padding: `fft-12`)
        // takes that setting's rules.
        let mut rules = rules_for(p, &target, &scan, change.path);
        if rules.is_empty() {
            if let Some(parent) = schema::setting(change.path).and_then(|s| s.parent) {
                rules = rules_for(p, &target, &scan, parent);
            }
        }
        let scan_only = !before.iter().any(|b| b.path == change.path && b.to == change.to);
        let on = scan_only.then(|| scan_conditions(&rules.iter().map(|r| r.rule.clone()).collect::<Vec<_>>()));
        let mut why: Vec<String> = rules.iter().map(|r| sentence(&r.reason)).collect();
        if why.is_empty() {
            why.extend(choice_reason(p, change.path).map(str::to_string));
        }
        Step { change, on, why }
    }).collect()
}

fn steps_table(steps: &[Step]) -> String {
    let mut stage = "";
    let rows: Vec<Vec<String>> = steps.iter().map(|s| {
        let c = &s.change;
        let label = schema::setting(c.path).map_or(c.label, |x| x.full_label);
        let flag = schema::setting(c.path).and_then(|x| x.cli_flag).map(|f| format!(" ({})", code(f))).unwrap_or_default();
        let mut value = value_text(c.path, &c.to);
        if let Some(on) = &s.on {
            value.push_str(&format!(", {}", cell(on)));
        }
        let this_stage = c.section.label();
        let stage_cell = if this_stage == stage { String::new() } else { this_stage.to_string() };
        stage = this_stage;
        vec![stage_cell, format!("{}{flag}", cell(label)), value, value_text(c.path, &c.from), cell(&s.why.join(" "))]
    }).collect();
    table(&["Stage", "Setting", "This pipeline", "QSMxT default", "Why"], &rows)
}

// ─── Pages ───

fn citations(p: &ReferencePipeline) -> String {
    let mut out: Vec<String> = Vec::new();
    for c in p.citations {
        let text = format!("- {}", reference_text(c));
        if !out.contains(&text) {
            out.push(text);
        }
    }
    out.push(format!("- {}", CITE_QSMXT.text));
    out.join("\n")
}

fn parity(p: &ReferencePipeline) -> String {
    match p.qsmci_pair {
        Some(pair) => format!(
            "{} QSM-CI's parity check against the original toolbox, for the latest QSMxT release \
             ([how parity is checked]({BASE}/toolbox-pipelines/#parity)).",
            badge(pair)
        ),
        None => format!(
            "No whole-pipeline parity test yet{}.",
            issue(PARITY_ISSUES, p.id).map(|i| format!(" ({i})")).unwrap_or_default()
        ),
    }
}

/// A toolbox pipeline's page.
pub fn pipeline_page(p: &ReferencePipeline) -> String {
    let order = REFERENCE_PIPELINES.iter().position(|q| q.id == p.id).map_or(0, |i| i + 1);
    let mut out = format!(
        "---\ntitle: \"{name}\"\ndescription: \"{toolbox}'s pipeline in QSMxT: qsmxt run --pipeline {id}.\"\nsidebar:\n  order: {order}\n---\n\n\
         <!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with {REGENERATE}) -->\n\n\
         ```sh\nqsmxt run study/bids --pipeline {id}\n```\n\n\
         In the TUI: the Pipeline tab, **{name}**. Taken from {toolbox} ({source}).\n\n\
         **Parity:** {parity}\n\n\
         ## Use it when\n\n{use_when}\n\n\
         ## Steps and settings\n\n\
         Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. \
         Every other setting keeps [its default]({BASE}/reference/configuration/).\n\n{steps}\n",
        name = p.name, toolbox = p.toolbox, id = p.id, source = p.source.trim_end_matches('.'),
        parity = parity(p), use_when = p.use_when, steps = steps_table(&steps(p)).trim_end(),
    );
    out.push_str("\n## Intentional differences from the original\n\n");
    for d in p.differences.iter().chain(DIFFERENCES) {
        out.push_str(&format!("- **{}** {}\n", d.what, d.why));
    }
    out.push_str("\n## Known gaps\n\n");
    match p.note {
        Some(note) => {
            let link = issue(GAP_ISSUES, p.id).map(|i| format!(" ({i})")).unwrap_or_default();
            out.push_str(&format!("{}{link}.\n", note.trim_end_matches('.')));
        }
        None => out.push_str("None known.\n"),
    }
    out.push_str(&format!("\n## Citations\n\n{}\n", citations(p)));
    out
}

/// The overview's table: every toolbox pipeline, its toolbox, its `--pipeline` value and its
/// parity.
pub fn pipelines_table() -> String {
    let rows: Vec<Vec<String>> = REFERENCE_PIPELINES.iter().map(|p| vec![
        format!("[{}]({})", cell(p.name), page_url(p)),
        cell(p.toolbox),
        code(p.id),
        p.qsmci_pair.map(badge).unwrap_or_else(|| "no whole-pipeline test yet".into()),
    ]).collect();
    table(&["Pipeline", "Taken from", "`--pipeline`", "Parity"], &rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every toolbox pipeline's page says why it sets each setting.
    #[test]
    fn every_setting_a_pipeline_sets_has_a_reason() {
        for p in REFERENCE_PIPELINES {
            for s in steps(p) {
                assert!(!s.why.is_empty(), "{}: no reason for {}", p.id, s.change.path);
            }
        }
    }

    /// A known gap links its issue, and a pipeline with no QSM-CI pair links the issue that adds
    /// one; the tables list only pipelines that exist.
    #[test]
    fn gaps_and_missing_pairs_link_their_issues() {
        for p in REFERENCE_PIPELINES {
            assert_eq!(p.note.is_some(), issue(GAP_ISSUES, p.id).is_some(), "{}: gap issue", p.id);
            assert_eq!(p.qsmci_pair.is_none(), issue(PARITY_ISSUES, p.id).is_some(), "{}: parity issue", p.id);
        }
    }

    /// The scan conditions read as words.
    #[test]
    fn scan_conditions_read_as_words() {
        assert_eq!(scan_conditions(&["chi-sep.romeo-template".into()]), "for scans with two or more echoes");
        assert_eq!(scan_conditions(&["chi-sep.tukey-not-philips".into()]), "for scans not from Philips");
        assert_eq!(scan_conditions(&[]), "depending on the scan");
    }
}
