//! A [`PipelineGraph`] as text, for a terminal: each step in run order, numbered, with its
//! algorithm, its pass in a two-pass run, its settings, when it applies, the models it runs, and
//! a line for each step it reads from, naming what flows along that arrow. `qsmxt run --dry`
//! prints it ([`PipelineGraph::to_text`]); the TUI's Pipeline tab styles it by [`Part`]
//! ([`PipelineGraph::text_lines`]).

use super::{layout, Branch, Node, PipelineGraph};
use crate::record::Decision;

/// What a piece of a line says, which sets how a terminal styles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// A step's number, in run order.
    Number,
    /// A step's label.
    Step,
    /// The algorithm a step runs.
    Algorithm,
    /// A two-pass run's pass, after the step it belongs to.
    Pass,
    /// One of the algorithm's settings.
    Setting,
    /// When the step does anything at all.
    When,
    /// The deep-learning models it runs.
    Models,
    /// The step an arrow comes from (its number and label).
    From,
    /// What flows along that arrow.
    Items,
    /// What a run did in the step, or what a dataset's runs did.
    Run,
    /// What it decided, and why.
    Note,
}

/// One line of the graph as text: its indent and its pieces, each with what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The node the line belongs to.
    pub step: String,
    pub indent: usize,
    pub parts: Vec<(Part, String)>,
}

impl Line {
    /// The line's text, unstyled, without its indent.
    pub fn text(&self) -> String {
        self.parts.iter().map(|(_, t)| t.as_str()).collect()
    }
}

/// Where a step's details start, under its label.
const DETAIL: usize = 5;

/// A model's name, from QSM.rs's registry.
pub fn model_name(id: &str) -> &str {
    qsm_core::models::find_model(id).map_or(id, |m| m.name)
}

fn number_width(n: usize) -> usize {
    n.to_string().len()
}

impl PipelineGraph {
    /// The graph as lines of text, `width` characters wide at most (a long line continues,
    /// further indented, on the next; `None` for no limit). See the module docs.
    pub fn text_lines(&self, width: Option<usize>) -> Vec<Line> {
        let digits = number_width(self.nodes.len());
        let number = |id: &str| self.nodes.iter().position(|n| n.id == id).map(|i| i + 1);
        let mut out: Vec<Line> = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            let mut push = |indent: usize, parts: Vec<(Part, String)>| {
                let line = Line { step: n.id.clone(), indent, parts };
                match width {
                    Some(w) => out.extend(wrap_line(line, w)),
                    None => out.push(line),
                }
            };
            let mut head = vec![(Part::Number, format!("{:>digits$} ", i + 1)), (Part::Step, n.label.clone())];
            if let Some(alg) = &n.algorithm_label {
                head.push((Part::Algorithm, format!(" · {alg}")));
            }
            // A two-pass run's per-pass steps say which pass, unless their label already does.
            let pass = match n.branch {
                Some(Branch::Main) => Some("main pass"),
                Some(Branch::Reliable) => Some("reliable pass"),
                None => None,
            };
            if let Some(pass) = pass.filter(|p| !n.label.contains(p)) {
                head.push((Part::Pass, format!("  [{pass}]")));
            }
            push(1, head);
            for s in detail_lines(n) {
                push(DETAIL, vec![s]);
            }
            // The arrows into the step, one per step it reads from, in the order it reads them.
            let mut from: Vec<(&str, Vec<String>)> = Vec::new();
            for e in self.edges.iter().filter(|e| e.to == n.id) {
                match from.iter_mut().find(|(f, _)| *f == e.from) {
                    Some((_, items)) => items.extend(e.items.iter().cloned()),
                    None => from.push((&e.from, e.items.clone())),
                }
            }
            for (f, items) in from {
                let Some(src) = self.node(f) else { continue };
                let num = number(f).map_or(String::new(), |k| format!("{k} "));
                let labels = layout::item_labels(self, &items);
                push(DETAIL, vec![
                    (Part::From, format!("← {num}{}:", src.label)),
                    (Part::Items, format!(" {}", labels.join(", "))),
                ]);
            }
        }
        out
    }

    /// The graph as plain text ([`PipelineGraph::text_lines`], no width limit), a line each.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for l in self.text_lines(None) {
            out.push_str(&" ".repeat(l.indent));
            out.push_str(&l.text());
            out.push('\n');
        }
        out
    }
}

/// A step's lines under its label: settings, when it applies, models, and what a run (or a
/// dataset's runs) did in it.
fn detail_lines(n: &Node) -> Vec<(Part, String)> {
    let mut out: Vec<(Part, String)> = n.settings.iter().map(|s| (Part::Setting, s.clone())).collect();
    if let Some(d) = &n.dataset {
        out.extend(d.alternatives.iter().map(|a| (Part::Setting, d.alternative_text(a))));
    }
    if let Some(when) = &n.only_when {
        out.push((Part::When, format!("only when {when}")));
    }
    if !n.models.is_empty() {
        let names: Vec<&str> = n.models.iter().map(|m| model_name(m)).collect();
        let what = if names.len() == 1 { "model" } else { "models" };
        out.push((Part::Models, format!("{what}: {}", names.join(", "))));
    }
    if let Some(run) = &n.run {
        out.push((Part::Run, layout::run_state_text(run.state).to_string()));
        out.extend(run.decisions.iter().map(|d: &Decision| (Part::Note, d.note())));
    }
    if let Some(d) = &n.dataset {
        out.extend(d.summary().into_iter().map(|t| (Part::Run, t)));
        out.extend(d.decisions.iter().map(|c| (Part::Note, d.decision_note(c))));
    }
    out
}

/// `line` broken to `width` ([`wrap_parts`]), its continuations indented two more.
fn wrap_line(line: Line, width: usize) -> Vec<Line> {
    wrap_parts(line.indent, line.indent + 2, &line.parts, width)
        .into_iter()
        .map(|(indent, parts)| Line { step: line.step.clone(), indent, parts })
        .collect()
}

/// A line of `parts` (each a piece of text and what it says, `P`), indented `indent`, broken
/// between words into lines of at most `width` characters, indent included; the lines after the
/// first are indented `hang`. A word longer than a line is cut where it reaches the end. Each
/// piece keeps what it says. Returns each line's indent and pieces.
pub fn wrap_parts<P: Copy + PartialEq>(indent: usize, hang: usize, parts: &[(P, String)], width: usize) -> Vec<(usize, Vec<(P, String)>)> {
    let total = indent + parts.iter().map(|(_, t)| t.chars().count()).sum::<usize>();
    if total <= width {
        return vec![(indent, parts.to_vec())];
    }
    // The line's words, each with the spaces before it, and what it says.
    let mut words: Vec<(P, String)> = Vec::new();
    for (part, text) in parts {
        let mut word = String::new();
        let mut in_space = true;
        for ch in text.chars() {
            if ch == ' ' && !in_space {
                words.push((*part, std::mem::take(&mut word)));
                in_space = true;
            } else if ch != ' ' {
                in_space = false;
            }
            word.push(ch);
        }
        if !word.is_empty() {
            words.push((*part, word));
        }
    }
    let mut out: Vec<(usize, Vec<(P, String)>)> = Vec::new();
    let mut cur: (usize, Vec<(P, String)>) = (indent, Vec::new());
    let mut used = indent;
    for (part, mut word) in words {
        if cur.1.is_empty() && !out.is_empty() {
            word = word.trim_start().to_string();
        }
        if !cur.1.is_empty() && used + word.chars().count() > width {
            out.push(std::mem::replace(&mut cur, (hang, Vec::new())));
            used = hang;
            word = word.trim_start().to_string();
        }
        // A word longer than a whole line: cut it.
        let room = |indent: usize| width.saturating_sub(indent).max(1);
        let mut chars: Vec<char> = word.chars().collect();
        while chars.len() > room(cur.0) && cur.1.is_empty() {
            let rest = chars.split_off(room(cur.0));
            cur.1.push((part, chars.into_iter().collect()));
            out.push(std::mem::replace(&mut cur, (hang, Vec::new())));
            used = hang;
            chars = rest;
        }
        let word: String = chars.into_iter().collect();
        used += word.chars().count();
        match cur.1.last_mut() {
            Some((p, t)) if *p == part => t.push_str(&word),
            _ => cur.1.push((part, word)),
        }
    }
    if !cur.1.is_empty() {
        out.push(cur);
    }
    out
}
