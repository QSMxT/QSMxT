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

use qsm_core::pipeline::reference_pipelines::{ReferencePipeline, REFERENCE_PIPELINES};
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

/// A link to an issue given as `owner/repo#n`.
fn issue(reference: &str) -> String {
    let (repo, n) = reference.split_once('#').unwrap_or_else(|| panic!("{reference:?} is not owner/repo#n"));
    let short = repo.rsplit('/').next().unwrap_or(repo);
    format!("[{short}#{n}](https://github.com/{repo}/issues/{n})")
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

/// Why `p` sets `path` when it is one of the pipeline's own choices (QSM.rs's `choose_reasons`),
/// or one QSMxT sets with them.
fn choice_reason(p: &ReferencePipeline, path: &str) -> Option<&'static str> {
    if let Some(i) = p.choose.iter().position(|(s, _)| rec::qsmxt_path(s) == Some(path)) {
        return p.choose_reasons.get(i).copied();
    }
    match path {
        "pipeline.do_chi_separation" => Some(CHOICE_REASONS[0]),
        "separation.r2prime_strategy" => Some(CHOICE_REASONS[1]),
        _ => None,
    }
}

/// [`choice_reason`]'s sentences.
const CHOICE_REASONS: [&str; 2] = [
    "A χ-separation method runs with χ-separation on.",
    "The χ-separation toolbox takes a measured R2′, from a multi-echo spin-echo scan or a supplied map.",
];

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
        // A pipeline's own choice gives QSM.rs's reason for it (`choose_reasons`); any other
        // setting, the reasons of the rules that set it.
        let mut why: Vec<String> = choice_reason(p, change.path).map(sentence).into_iter().collect();
        if why.is_empty() {
            why = rules.iter().map(|r| sentence(&r.reason)).collect();
        }
        Step { change, on, why }
    }).collect()
}

/// The sentences of `text`, each ended as a sentence.
fn sentences(text: &str) -> Vec<String> {
    let text = sentence(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    let mut out: Vec<String> = Vec::new();
    let mut rest = text.as_str();
    // A sentence ends at ". " before a capital letter (not "e.g. x" or "2.89 T").
    while let Some(i) = rest.char_indices().find(|&(i, c)| {
        c == '.' && rest[i + 1..].starts_with(' ') && rest[i + 2..].starts_with(|c: char| c.is_uppercase())
    }).map(|(i, _)| i) {
        out.push(rest[..=i].to_string());
        rest = &rest[i + 2..];
    }
    if !rest.is_empty() {
        out.push(rest.to_string());
    }
    out
}

/// Nouns that end a sentence's subject ("The MEDI toolbox", "DECOMPOSE's authors").
const SUBJECT_NOUNS: [&str; 5] = ["toolbox", "package", "GUI", "authors", "pipeline"];

/// The subject a sentence opens with: its shortest opening that is a name or ends in one of
/// [`SUBJECT_NOUNS`] and is followed by a verb ("QSMART runs", "STI Suite's GUI keeps").
fn subject(sentence: &str) -> Option<String> {
    let words: Vec<&str> = sentence.split(' ').collect();
    (1..words.len()).find_map(|n| {
        let last = words[n - 1];
        let next = words[n];
        let names = last.starts_with(|c: char| c.is_uppercase()) || SUBJECT_NOUNS.contains(&last);
        let verb_next = next.starts_with(|c: char| c.is_lowercase()) && !SUBJECT_NOUNS.contains(&next);
        let article = n == 1 && matches!(last, "The" | "A" | "An");
        (names && verb_next && !article && !last.ends_with("'s")).then(|| words[..n].join(" "))
    })
}

/// Sentences joined, each that opens with the subject of the one before it saying "It" or "They"
/// instead of naming the toolbox again.
fn join_sentences(sentences: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    for (k, x) in sentences.iter().enumerate() {
        let before = k.checked_sub(1).and_then(|j| subject(&sentences[j]));
        match before.filter(|s| subject(x).as_deref() == Some(s.as_str())) {
            Some(s) => {
                let pronoun = if s.ends_with("authors") { "They" } else { "It" };
                out.push(format!("{pronoun}{}", &x[s.len()..]));
            }
            None => out.push(x.clone()),
        }
    }
    out.join(" ")
}

/// The most a table cell says itself; a longer reason goes to a note under the table.
const CELL_SENTENCES: usize = 2;

/// The steps table, and the notes under it. Each reason is given once on a page: a row set by a
/// rule already given above says so, and a row whose reasons run past [`CELL_SENTENCES`] gives
/// its first sentence and the rest in a numbered note.
fn steps_table(steps: &[Step]) -> (String, Vec<String>) {
    let mut stage = "";
    // Each reason given so far, by its first sentence, with the row it was given on.
    let mut given: Vec<(String, &'static str)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
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
        // The sentences no earlier row gave, once each.
        let mut fresh: Vec<String> = Vec::new();
        let mut earlier: Vec<&'static str> = Vec::new();
        for why in &s.why {
            for x in sentences(why) {
                // QSMxT's own wording for a pipeline's choices is the same on every such row.
                if CHOICE_REASONS.contains(&x.as_str()) {
                    if !fresh.contains(&x) { fresh.push(x) }
                    continue;
                }
                match given.iter().find(|(g, _)| *g == x) {
                    Some((_, row)) => { if !earlier.contains(row) { earlier.push(row) } }
                    None if !fresh.contains(&x) => fresh.push(x),
                    None => {}
                }
            }
        }
        given.extend(fresh.iter().map(|x| (x.clone(), label)));
        let why = if fresh.is_empty() {
            let rows: Vec<String> = earlier.iter().map(|r| format!("*{}*", cell(r))).collect();
            format!("The same reason as {} above.", rows.join(" and "))
        } else if fresh.len() <= CELL_SENTENCES {
            cell(&join_sentences(&fresh))
        } else {
            notes.push(join_sentences(&fresh[1..]));
            format!("{} (note {})", cell(&fresh[0]), notes.len())
        };
        vec![stage_cell, format!("{}{flag}", cell(label)), value, value_text(c.path, &c.from), why]
    }).collect();
    (table(&["Stage", "Setting", "This pipeline", "QSMxT default", "Why"], &rows), notes)
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
            p.qsmci_pair_issue.map(|i| format!(" ({})", issue(i))).unwrap_or_default()
        ),
    }
}

/// A toolbox pipeline's page.
pub fn pipeline_page(p: &ReferencePipeline) -> String {
    let (table, notes) = steps_table(&steps(p));
    let order = REFERENCE_PIPELINES.iter().position(|q| q.id == p.id).map_or(0, |i| i + 1);
    let mut out = format!(
        "---\ntitle: \"{name}\"\ndescription: \"{toolbox}'s pipeline in QSMxT: qsmxt run --pipeline {id}.\"\nsidebar:\n  order: {order}\n---\n\n\
         <!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with {REGENERATE}) -->\n\n\
         ```sh\nqsmxt run study/bids --pipeline {id}\n```\n\n\
         In the TUI: the Pipeline tab, **{name}**.\n\n\
         **Based on:** {based_on}.\n\n\
         **Parity:** {parity}\n\n\
         ## Use it when\n\n{use_when}\n\n\
         ## Steps and settings\n\n\
         Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. \
         Every other setting keeps [its default]({BASE}/reference/configuration/).\n\n{steps}\n",
        name = p.name, toolbox = p.toolbox, id = p.id, based_on = p.based_on.trim_end_matches('.'),
        parity = parity(p), use_when = p.use_when, steps = table.trim_end(),
    );
    if !notes.is_empty() {
        out.push_str("\nNotes on the table:\n\n");
        for (k, n) in notes.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", k + 1, n));
        }
    }
    out.push_str("\n## Intentional differences from the original\n\n");
    let differences = p.all_differences();
    if differences.is_empty() {
        out.push_str("None.\n");
    }
    for d in differences {
        out.push_str(&format!("- **{}** {}\n", d.what, d.why));
    }
    out.push_str("\n## Known gaps\n\n");
    match p.note {
        Some(note) => {
            let link = p.note_issue.map(|i| format!(" ({})", issue(i))).unwrap_or_default();
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
            assert_eq!(p.choose_reasons.len(), p.choose.len(), "{}: a reason for each choice", p.id);
            for s in steps(p) {
                assert!(!s.why.is_empty(), "{}: no reason for {}", p.id, s.change.path);
            }
        }
    }

    /// A reason's sentences split where a sentence ends, not inside a number or abbreviation.
    #[test]
    fn reasons_split_into_sentences() {
        assert_eq!(sentences("One at 2.89 T. Two, e.g. this"), ["One at 2.89 T.", "Two, e.g. this."]);
    }

    /// A note names its toolbox once: a sentence with the same subject as the one before says "It".
    #[test]
    fn joined_sentences_name_the_subject_once() {
        let s = |x: &[&str]| x.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(join_sentences(&s(&["QSMART runs it.", "QSMART smooths it.", "QSMART closes it."])),
            "QSMART runs it. It smooths it. It closes it.");
        assert_eq!(join_sentences(&s(&["The MEDI toolbox runs it.", "The MEDI toolbox uses it."])),
            "The MEDI toolbox runs it. It uses it.");
        assert_eq!(join_sentences(&s(&["DECOMPOSE's authors run it.", "DECOMPOSE's authors describe it."])),
            "DECOMPOSE's authors run it. They describe it.");
        assert_eq!(join_sentences(&s(&["MEDI's PDF runs it.", "The MEDI toolbox runs it."])),
            "MEDI's PDF runs it. The MEDI toolbox runs it.");
    }

    /// No steps table repeats a reason, and no cell runs past two sentences.
    #[test]
    fn steps_tables_give_each_reason_once() {
        for p in REFERENCE_PIPELINES {
            let (table, _) = steps_table(&steps(p));
            let mut seen: Vec<String> = Vec::new();
            for row in table.lines().skip(2) {
                let why = row.trim_end_matches(" |").rsplit(" | ").next().unwrap().to_string();
                for x in sentences(&why) {
                    assert!(x.starts_with("The same reason as") || CHOICE_REASONS.contains(&x.as_str()) || !seen.contains(&x), "{}: {x:?} twice", p.id);
                    seen.push(x);
                }
                assert!(sentences(&why).len() <= CELL_SENTENCES, "{}: {why:?}", p.id);
            }
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
