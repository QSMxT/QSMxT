//! The generated parts of the reference docs, from their single sources: the settings reference
//! in `configuration.md` from the settings registry ([`crate::schema::settings`]), and the method
//! tables in `algorithms.md` and the README from QSM.rs's method catalogue
//! ([`qsm_core::catalogue::METHODS`]), with each method's citations written as the methods text
//! writes them.
//!
//! Only the factual blocks are generated; the prose around them is hand-written. Each block sits
//! between two markers in its page,
//!
//! ```text
//! <!-- generated:NAME start (…) -->
//! …
//! <!-- generated:NAME end -->
//! ```
//!
//! and [`render_page`] replaces what is between them (and the start marker's note). The tests
//! fail when a committed page differs from what renders; regenerate every page with
//!
//! ```sh
//! QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs
//! ```
//!
//! (`qsmxt`'s CLI reference is regenerated the same way: `cargo test --bin qsmxt cli_reference`.)

use qsm_core::catalogue::{self, Citation, Method, Stage};
use serde_json::Value;

use crate::methods::citations::PageCitations;
use crate::schema::{self, EchoKind, EnumOption, Section, Setting, ValueType};

/// The settings reference, relative to the repository root.
pub const CONFIGURATION: &str = "docs/src/content/docs/reference/configuration.md";
/// The algorithms reference.
pub const ALGORITHMS: &str = "docs/src/content/docs/reference/algorithms.md";
/// The README.
pub const README: &str = "README.md";

const REGENERATE: &str = "regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs";

/// `page` (the text of the file `path`, one of [`CONFIGURATION`], [`ALGORITHMS`] and
/// [`README`]) with its generated blocks rendered afresh. An error names a block whose markers
/// are missing or out of order, or a file that has no generated blocks.
pub fn render_page(path: &str, page: &str) -> Result<String, String> {
    let blocks = match path {
        CONFIGURATION => vec![("settings", "the settings registry", settings_reference())],
        ALGORITHMS => algorithm_blocks(),
        README => vec![("algorithms", "QSM.rs's method catalogue", readme_algorithms())],
        _ => return Err(format!("{path} has no generated blocks")),
    };
    let mut out = page.to_string();
    for (name, source, content) in blocks {
        out = splice(&out, name, source, &content).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(out)
}

/// `page` with the block `name` replaced by `content`.
fn splice(page: &str, name: &str, source: &str, content: &str) -> Result<String, String> {
    let start = format!("<!-- generated:{name} start");
    let end = format!("<!-- generated:{name} end -->");
    let s = page.find(&start).ok_or_else(|| format!("no `{start}` marker"))?;
    let e = page[s..].find(&end).map(|i| s + i).ok_or_else(|| format!("no `{end}` after its start marker"))?;
    let mut out = page[..s].to_string();
    out.push_str(&format!("{start} (from {source}: do not edit; {REGENERATE}) -->\n\n"));
    out.push_str(content.trim_end());
    out.push_str("\n\n");
    out.push_str(&page[e..]);
    Ok(out)
}

// ─── Markdown ───

/// Text that is safe in a Markdown table cell: `|` escaped everywhere, and outside `code`, `<`/`>`
/// written as entities (not read as HTML) and `*` escaped (not read as emphasis).
fn cell(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.split('`').enumerate().map(|(k, part)| {
        let part = part.replace('|', "\\|");
        if k % 2 == 1 { part } else { part.replace('<', "&lt;").replace('>', "&gt;").replace('*', "\\*") }
    }).collect::<Vec<_>>().join("`")
}

/// `text` ended as a sentence.
fn sentence(text: &str) -> String {
    let t = text.trim_end();
    if t.is_empty() || t.ends_with(['.', '!', '?']) { t.to_string() } else { format!("{t}.") }
}

/// `text` as inline code, with a longer fence where it holds a backtick.
fn code(text: &str) -> String {
    if text.contains('`') { format!("`` {text} ``") } else { format!("`{text}`") }
}

fn table(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = format!("| {} |\n|{}\n", header.join(" | "), " --- |".repeat(header.len()));
    for row in rows {
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out
}

// ─── The settings reference ───

/// Every registry setting, grouped by the TUI's tabs and sections: its config path, type, units
/// and range, default, `qsmxt run` option and front-end help (`help`, which is worded for any
/// front end and leaves the default and units to their own columns, rather than the TUI's
/// `tui_help`).
pub fn settings_reference() -> String {
    let settings = schema::settings();
    let mut out = String::from(
        "## All settings\n\n\
         Every setting a configuration file can hold, grouped as the TUI's tabs show them: where it\n\
         goes in the file (`a.b.c` is `c` under `[a.b]`), what it takes, its default, and the\n\
         `qsmxt run` option that sets it. A setting applies only where its method or step runs.\n",
    );
    for section in Section::ALL {
        let in_section: Vec<&Setting> = settings.iter().filter(|s| s.section == section).collect();
        if in_section.is_empty() {
            continue;
        }
        let tab = section.tab().label();
        let title = if tab == section.label() { tab.to_string() } else { format!("{tab}: {}", section.label()) };
        out.push_str(&format!("\n### {title}\n\n"));
        let rows: Vec<Vec<String>> = in_section.iter().map(|s| vec![
            code(s.path),
            cell(&type_text(s)),
            default_text(s),
            s.cli_flag.map(code).unwrap_or_else(|| "—".into()),
            cell(&description(s)),
        ]).collect();
        out.push_str(&table(&["Setting", "Type", "Default", "Option", "Description"], &rows));
    }
    out
}

fn type_text(s: &Setting) -> String {
    let mut t = match &s.value_type {
        ValueType::Bool => "true or false",
        ValueType::Int => "whole number",
        ValueType::Float => "number",
        ValueType::IntVec3 => "three whole numbers",
        ValueType::FloatVec3 => "three numbers",
        ValueType::IntList => "list of whole numbers",
        ValueType::Text => "text",
        ValueType::Enum { .. } => "choice",
        ValueType::EchoReference { .. } => "echo",
        ValueType::EchoTime { .. } => "echo time",
        ValueType::NumberOrChoice { .. } => "number or choice",
        ValueType::MaskSections => "mask sections",
        ValueType::MaskOps => "mask steps",
    }.to_string();
    if let Some(u) = s.units {
        t.push_str(&format!(", {u}"));
    }
    if s.min.is_some() || s.max.is_some() {
        t.push_str(&format!(", {}", s.range_text()));
    }
    t
}

/// A config value as TOML writes it inline.
fn toml(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{s}\""),
        Value::Array(a) => format!("[{}]", a.iter().map(toml).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => format!("{{ {} }}", o.iter().map(|(k, v)| format!("{k} = {}", toml(v))).collect::<Vec<_>>().join(", ")),
    }
}

fn default_text(s: &Setting) -> String {
    match (&s.value_type, &s.default) {
        (_, Value::Null) => "unset".into(),
        // A mask recipe's TOML is an array of tables: give it as `--mask` and `--mask-refine` write it.
        (ValueType::MaskSections, v) => match serde_json::from_value::<Vec<crate::MaskSection>>(v.clone()) {
            Ok(sections) if sections.is_empty() => "none".into(),
            Ok(sections) => sections.iter().map(|x| code(&x.compact_spec())).collect::<Vec<_>>().join(" and "),
            Err(_) => code(&toml(v)),
        },
        (ValueType::MaskOps, v) => match serde_json::from_value::<Vec<crate::MaskOp>>(v.clone()) {
            Ok(ops) if ops.is_empty() => "none".into(),
            Ok(ops) => ops.iter().map(|x| code(&x.compact_spec())).collect::<Vec<_>>().join(", "),
            Err(_) => code(&toml(v)),
        },
        (_, v) => code(&toml(v)).replace('|', "\\|"),
    }
}

fn description(s: &Setting) -> String {
    let mut parts = vec![sentence(if s.help.is_empty() { s.full_label } else { s.help })];
    // A setting shown as a checkbox with a value under it: what the value is, when it is on.
    if let (Some(_), Some(h)) = (&s.off, s.value_help.filter(|h| !h.is_empty())) {
        parts.push(sentence(&format!("{}: {h}", s.value_label.unwrap_or("Value"))));
    }
    let values = values_text(&s.value_type);
    if !values.is_empty() {
        parts.push(format!("Values: {values}."));
    }
    if let Some(off) = &s.off {
        parts.push(if off.is_null() { "Unset turns it off.".into() } else { format!("{} turns it off.", code(&toml(off))) });
    } else if s.optional {
        parts.push("May be left unset.".into());
    }
    parts.join(" ")
}

/// The values a choice takes, as the config writes them.
fn values_text(t: &ValueType) -> String {
    let named = |o: &EnumOption| code(&o.value.as_str().map(str::to_string).unwrap_or_else(|| toml(&o.value)));
    match t {
        ValueType::Enum { options } => options.iter().map(named).collect::<Vec<_>>().join(", "),
        ValueType::NumberOrChoice { options } => {
            format!("a number, or {}", options.iter().map(named).collect::<Vec<_>>().join(", "))
        }
        ValueType::EchoReference { options } | ValueType::EchoTime { options } => options.iter().map(|o| match o.kind {
            EchoKind::Named => named(&o.option),
            EchoKind::EchoNumber { min } => format!("an echo number from {min}, as text (`\"{min}\"`)"),
            EchoKind::Time => "a time in seconds, `{ seconds = … }`".into(),
        }).collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

// ─── Method tables ───

/// The methods a table lists.
#[derive(Clone, Copy)]
enum Rows {
    /// A registry choice's options, each the catalogue's method of `stage` with the option's id.
    Setting { path: &'static str, stage: Stage, family: Family },
    /// Every catalogue method of a stage, in catalogue order.
    Stage(Stage),
    /// The mask recipe's steps ([`schema::mask_schema`]): thresholds by their method.
    MaskSteps,
}

/// Which of a choice's methods a table lists.
#[derive(Clone, Copy, PartialEq)]
enum Family {
    All,
    /// Methods that run no network.
    Classical,
    /// Methods that run a network of QSM.rs's model registry.
    DeepLearning,
}

/// A table's row: the value that chooses it (for a registry choice), its catalogue entry, and the
/// registry's help for the option.
struct Row {
    value: Option<String>,
    method: &'static Method,
    help: Option<String>,
    default: bool,
}

fn rows(rows: Rows) -> Vec<Row> {
    match rows {
        Rows::Setting { path, stage, family } => {
            let s = schema::settings().iter().find(|s| s.path == path)
                .unwrap_or_else(|| panic!("the settings registry has no {path}"));
            s.value_type.options().into_iter().map(|o| {
                let id = o.value.as_str().unwrap_or_else(|| panic!("{path}: an option that is not a name"));
                // Every option is a method the catalogue knows, or the table would drop it.
                let method = catalogue::find(stage, id)
                    .unwrap_or_else(|| panic!("{path} offers {id:?}, which QSM.rs's catalogue has no {stage:?} method for"));
                Row { value: Some(id.to_string()), method, help: Some(o.help.clone()), default: o.value == s.default }
            }).filter(|r| match family {
                Family::All => true,
                Family::Classical => r.method.models.is_empty(),
                Family::DeepLearning => !r.method.models.is_empty(),
            }).collect()
        }
        Rows::Stage(stage) => catalogue::in_stage(stage)
            .map(|method| Row { value: None, method, help: None, default: false })
            .collect(),
        Rows::MaskSteps => schema::mask_schema().steps.iter().map(|step| {
            let method = match step.kind.strip_prefix("threshold-") {
                Some(t) => catalogue::find(Stage::MaskThreshold, t),
                None => catalogue::find(Stage::Masking, step.op),
            }.unwrap_or_else(|| panic!("mask step {} has no QSM.rs catalogue entry", step.kind));
            Row { value: None, method, help: None, default: false }
        }).collect(),
    }
}

/// A row's citations, each linked to its DOI (or its URL, for a work with none).
fn citations(cites: &PageCitations, works: &[Citation]) -> String {
    let linked: Vec<String> = works.iter().map(|w| {
        let label = cites.inline(w);
        match (w.doi, w.url) {
            (Some(doi), _) => format!("[{label}](https://doi.org/{doi})"),
            (None, Some(url)) => format!("[{label}]({url})"),
            (None, None) => label,
        }
    }).collect();
    let mut once: Vec<String> = Vec::new();
    for l in linked {
        if !once.contains(&l) {
            once.push(l);
        }
    }
    if once.is_empty() { "—".into() } else { once.join("; ") }
}

/// The citations of a page whose tables list `tables`' rows, in that order.
fn page_citations(tables: &[Rows]) -> PageCitations {
    PageCitations::new(tables.iter().flat_map(|t| rows(*t)).flat_map(|r| r.method.citations.iter()))
}

// ─── The algorithms page ───

const INVERSION: &str = "inversion.algorithm";

/// The algorithms page's blocks: each stage's choice of method, its id, catalogue name, the
/// registry's help for it, and its papers.
const ALGORITHM_TABLES: [(&str, Rows); 6] = [
    ("inhomogeneity-methods", Rows::Setting { path: "masking.inhomogeneity_method", stage: Stage::InhomogeneityCorrection, family: Family::All }),
    ("unwrapping-methods", Rows::Setting { path: "field_mapping.unwrapping_algorithm", stage: Stage::Unwrapping, family: Family::All }),
    ("background-methods", Rows::Setting { path: "bg_removal.algorithm", stage: Stage::BackgroundRemoval, family: Family::All }),
    ("inversion-methods", Rows::Setting { path: INVERSION, stage: Stage::Inversion, family: Family::Classical }),
    ("deep-learning-inversion-methods", Rows::Setting { path: INVERSION, stage: Stage::Inversion, family: Family::DeepLearning }),
    ("separation-methods", Rows::Setting { path: "separation.algorithm", stage: Stage::Separation, family: Family::All }),
];

fn algorithm_blocks() -> Vec<(&'static str, &'static str, String)> {
    let cites = page_citations(&ALGORITHM_TABLES.map(|(_, t)| t));
    ALGORITHM_TABLES.iter().map(|(name, t)| {
        let deep_learning_inversions = matches!(t, Rows::Setting { path: INVERSION, family: Family::DeepLearning, .. });
        let rows: Vec<Vec<String>> = rows(*t).into_iter().map(|r| {
            let value = r.value.expect("an algorithms-page row is a registry option");
            let mut row = vec![
                code(&value),
                cell(&format!("{}{}", r.method.name, if r.default { " (default)" } else { "" })),
                cell(&sentence(r.help.as_deref().unwrap_or_default())),
                citations(&cites, r.method.citations),
            ];
            if deep_learning_inversions {
                let alg: crate::QsmAlgorithm = serde_json::from_value(Value::from(value.as_str()))
                    .unwrap_or_else(|_| panic!("{value} is not a QsmAlgorithm"));
                row.push(if alg.tileable() { "✓".into() } else { String::new() });
            }
            row
        }).collect();
        let mut header = vec!["Value", "Method", "Description", "Citation"];
        if deep_learning_inversions {
            header.push("Tileable");
        }
        (*name, "QSM.rs's method catalogue and the settings registry", table(&header, &rows))
    }).collect()
}

// ─── The README ───

/// The README's tables: every method QSMxT offers, by stage.
const README_TABLES: [(&str, Rows); 14] = [
    ("Masking", Rows::MaskSteps),
    ("Inhomogeneity correction", Rows::Setting { path: "masking.inhomogeneity_method", stage: Stage::InhomogeneityCorrection, family: Family::All }),
    ("Phase correction", Rows::Stage(Stage::PhaseCorrection)),
    ("Phase unwrapping", Rows::Setting { path: "field_mapping.unwrapping_algorithm", stage: Stage::Unwrapping, family: Family::All }),
    ("Echo combination", Rows::Setting { path: "field_mapping.b0_estimation", stage: Stage::EchoCombination, family: Family::All }),
    ("Background field removal", Rows::Setting { path: "bg_removal.algorithm", stage: Stage::BackgroundRemoval, family: Family::All }),
    ("Dipole inversion", Rows::Setting { path: INVERSION, stage: Stage::Inversion, family: Family::Classical }),
    ("Dipole inversion: deep learning", Rows::Setting { path: INVERSION, stage: Stage::Inversion, family: Family::DeepLearning }),
    ("Additions to the inversion", Rows::Stage(Stage::InversionAddition)),
    ("Multi-orientation", Rows::Setting { path: "multi_orientation.algorithm", stage: Stage::MultiOrientation, family: Family::All }),
    ("Susceptibility source separation", Rows::Setting { path: "separation.algorithm", stage: Stage::Separation, family: Family::All }),
    ("Relaxometry", Rows::Stage(Stage::Relaxometry)),
    ("Susceptibility-weighted imaging", Rows::Stage(Stage::Swi)),
    ("Segmentation", Rows::Stage(Stage::Segmentation)),
];

/// The README's algorithm tables: each stage's methods by name, with their papers.
pub fn readme_algorithms() -> String {
    let cites = page_citations(&README_TABLES.map(|(_, t)| t));
    let mut out = String::from(
        "## Algorithms\n\n\
         The methods QSMxT offers at each stage, all implemented in\n\
         [QSM.rs](https://github.com/astewartau/QSM.rs), and the papers to cite for them.\n",
    );
    for (title, t) in README_TABLES {
        out.push_str(&format!("\n### {title}\n\n"));
        let rows: Vec<Vec<String>> = rows(t).into_iter()
            .map(|r| vec![cell(r.method.name), citations(&cites, r.method.citations)])
            .collect();
        out.push_str(&table(&["Method", "Citation"], &rows));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The committed page is what renders. `QSMXT_UPDATE_DOCS=1` writes it instead.
    fn check(path: &str) {
        let file = repo().join(path);
        let committed = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{path}: {e}"));
        let rendered = render_page(path, &committed).unwrap();
        if std::env::var_os("QSMXT_UPDATE_DOCS").is_some() {
            std::fs::write(&file, &rendered).unwrap();
            return;
        }
        assert!(committed == rendered,
            "{path}'s generated blocks are out of date: run `QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine \
             reference_docs` and commit the result");
    }

    #[test]
    fn configuration_reference_is_current() {
        check(CONFIGURATION);
    }

    #[test]
    fn algorithms_reference_is_current() {
        check(ALGORITHMS);
    }

    #[test]
    fn readme_algorithms_are_current() {
        check(README);
    }

    /// The settings reference lists every registry setting once.
    #[test]
    fn every_setting_is_in_the_settings_reference() {
        let text = settings_reference();
        for s in schema::settings() {
            assert_eq!(text.matches(&format!("| `{}` |", s.path)).count(), 1, "{}", s.path);
        }
    }

    /// Splicing replaces only what is between a block's markers, and refuses a page without them.
    #[test]
    fn splice_replaces_only_the_block() {
        let page = "before\n<!-- generated:x start (old) -->\nold\n<!-- generated:x end -->\nafter\n";
        let out = splice(page, "x", "src", "new").unwrap();
        assert!(out.starts_with("before\n<!-- generated:x start (from src: do not edit; "), "{out}");
        assert!(out.ends_with(" -->\n\nnew\n\n<!-- generated:x end -->\nafter\n"), "{out}");
        assert_eq!(splice(&out, "x", "src", "new").unwrap(), out);
        assert!(splice("no markers", "x", "src", "new").is_err());
    }
}
