//! Where each box and arrow of a [`PipelineGraph`] goes when it is drawn: one box per step, top
//! to bottom in run order, with every arrow routed between the boxes and labelled with what flows
//! along it. [`PipelineGraph::to_svg`] draws it; a front end can draw the same [`Layout`] (it
//! serialises to JSON). The layout is a pure function of the graph.
//!
//! The graph is small (10–25 steps) and its steps are in run order, so a layered layout does the
//! job without a layout library:
//!
//! - **Rows.** A step sits one row below the lowest step it reads from (longest path).
//! - **Columns.** The steps every run takes form the spine (column 0). A step an output switch
//!   asked for (`enabled_by` = `pipeline.*`) goes to the first free column to its right, as does a
//!   spine step whose row is taken. In a two-pass run each pass has a column of its own, so each
//!   pass's box holds its steps and nothing else: main pass, then the shared spine, then reliable.
//! - **Arrows** run only between boxes, and every arrow enters its target from the top:
//!   - to the next row: down from the source's bottom, along a track in the gap above the
//!     target, and down into it (straight down where the two ends line up);
//!   - further down: out of the source's side into a lane of the channel beside it, straight down
//!     the lane, along a track in the gap above the target, and down into it.
//!
//!   Each box side has a port per arrow, ordered by where the arrow's other end is, so arrows
//!   leaving or entering a box do not cross each other there. Each arrow has its own track and
//!   lane. Two arrows from one box to another are one arrow carrying both.
//! - **Labels** list what flows along an arrow, one item per line, never cut short. A label sits
//!   beside its arrow's last drop into the target, below the arrow's track. The gap above a row
//!   is tall enough for the labels of the arrows entering it, the leftmost drop's track highest,
//!   so a label to the right of its drop does not lie across a later drop. A label that would
//!   still touch a box, a line or another label moves to the other side of its drop or beside
//!   another of its arrow's vertical runs, and failing that, its gap grows.
//! - The many arrows into the return to the acquired grid are drawn as one (the data is unchanged).
//! - A **key** below the graph explains the styles the graph uses, and only those.
//!
//! Text is measured by an estimate of its width.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::{Branch, Edge, Node, PipelineGraph};
use crate::record::StepState;

/// The step whose arrows are drawn as one.
pub const RETURN_STEP: &str = "output_space";

/// A box's width.
pub const NODE_W: f64 = 210.0;
const PAD: f64 = 8.0;
/// The least distance between two tracks in a gap.
const TRACK: f64 = 10.0;
/// A gap's margin above its tracks.
const GAP: f64 = 14.0;
/// Between the last label of a gap and the boxes below it.
const ARROW_ROOM: f64 = 6.0;
const LANE: f64 = 7.0; // between vertical lanes in a channel
const GUTTER: f64 = 28.0; // a channel's width with no lanes
const GROUP_PAD: f64 = 9.0;
const GROUP_TITLE: f64 = 18.0;
/// The share of a pass's first box, from the left, its title sits above.
const TITLE_CLEAR: f64 = 0.55;
const MARGIN: f64 = 12.0;
/// A label's average glyph width and line height.
const LABEL_CHAR: f64 = 6.0;
/// A label's line height.
pub const LABEL_LINE: f64 = 12.0;
/// The longest line of a label before an item's name wraps.
const LABEL_WRAP: usize = 32;
/// Between a label and the line it labels.
const LABEL_GAP: f64 = 4.0;
/// The key's row height.
const KEY_H: f64 = 22.0;
/// A key entry's sample width.
pub const KEY_SAMPLE_W: f64 = 26.0;
const KEY_CHAR: f64 = 5.9;
/// How often the gaps may grow to make room for labels.
const ROUNDS: usize = 6;

/// What a line of a box's text is, which sets its size and style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineClass {
    Title,
    Algorithm,
    Setting,
    When,
    Run,
    Note,
}

impl LineClass {
    /// The line's height.
    pub fn height(self) -> f64 {
        match self {
            LineClass::Title => 17.0,
            LineClass::Algorithm => 15.0,
            LineClass::Setting | LineClass::When | LineClass::Run => 14.0,
            LineClass::Note => 13.0,
        }
    }

    /// The average glyph width at the size the line is drawn.
    fn char_width(self) -> f64 {
        match self {
            LineClass::Title => 7.7,
            LineClass::Algorithm => 6.4,
            LineClass::Setting | LineClass::When => 5.9,
            LineClass::Run => 6.2,
            LineClass::Note => 5.6,
        }
    }

    /// The characters a box's line of this class holds.
    fn room(self) -> usize {
        ((NODE_W - 2.0 * PAD) / self.char_width()).floor() as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            LineClass::Title => "title",
            LineClass::Algorithm => "algorithm",
            LineClass::Setting => "setting",
            LineClass::When => "when",
            LineClass::Run => "run",
            LineClass::Note => "note",
        }
    }
}

/// One line of a box's text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextLine {
    pub text: String,
    pub cls: LineClass,
}

/// A box, placed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaidNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub row: usize,
    pub col: usize,
    pub lines: Vec<TextLine>,
    /// An output switch or the two-pass setting asked for it.
    pub optional: bool,
    /// It may do nothing ([`Node::only_when`]).
    pub conditional: bool,
}

/// A rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// The area this and `o` share.
    pub fn overlap(&self, o: &Rect) -> f64 {
        let w = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let h = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        if w > 0.0 && h > 0.0 {
            w * h
        } else {
            0.0
        }
    }
}

/// An arrow, routed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaidEdge {
    pub from: String,
    pub to: String,
    /// What flows along it, by item key.
    pub items: Vec<String>,
    /// What flows along it, as drawn: one item per line.
    pub label: Vec<String>,
    /// What flows along it, on one line.
    pub title: String,
    /// The route, from the source to the target's top.
    pub points: Vec<(f64, f64)>,
    pub label_box: Rect,
    /// How many arrows it stands for (1 for an ordinary one).
    pub collapsed: usize,
}

/// A two-pass run's pass, drawn as a box around its steps.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub branch: Branch,
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A style the key explains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStyle {
    /// A dashed box: a setting asked for the step.
    Optional,
    /// A dotted box: the step does something only in some runs.
    Conditional,
    /// A faded dotted box: the run skipped the step.
    Skipped,
    /// The model mark: the step runs a deep-learning model.
    Model,
    /// A thick arrow: several arrows drawn as one.
    Collapsed,
}

/// One entry of the key, placed: its sample at (`x`, `y`), its text after it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeyEntry {
    pub style: KeyStyle,
    pub text: String,
    pub x: f64,
    pub y: f64,
}

/// A graph, laid out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<LaidNode>,
    pub edges: Vec<LaidEdge>,
    pub groups: Vec<Group>,
    /// The styles the graph uses; empty when it uses none.
    pub key: Vec<KeyEntry>,
}

/// `text` in lines of at most `max` characters, broken between words, and a word longer than a
/// line broken where it reaches the end.
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        } else if line.is_empty() {
            line = word.to_string();
        } else {
            line.push(' ');
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out.into_iter()
        .flat_map(|l| {
            let chars: Vec<char> = l.chars().collect();
            if chars.len() <= max || max == 0 {
                vec![l]
            } else {
                chars.chunks(max).map(|c| c.iter().collect()).collect()
            }
        })
        .collect()
}

/// What a run's step did, as its box says it.
pub fn run_state_text(state: StepState) -> &'static str {
    match state {
        StepState::Computed => "Computed",
        StepState::Cached => "Reused from the cache",
        StepState::Skipped => "Skipped",
    }
}

fn push_wrapped(lines: &mut Vec<TextLine>, text: &str, cls: LineClass, width: usize) {
    lines.extend(
        wrap(text, width)
            .into_iter()
            .map(|text| TextLine { text, cls }),
    );
}

/// A box's text, in full: the step, its algorithm, its settings and when it applies; after a run,
/// what the step did and what it decided; for a dataset's runs, each way they set the step up,
/// how many took it, and what they decided.
pub fn node_lines(n: &Node) -> Vec<TextLine> {
    let mut lines = Vec::new();
    let title = LineClass::Title.room() - if n.models.is_empty() { 0 } else { 7 };
    push_wrapped(&mut lines, &n.label, LineClass::Title, title);
    if let Some(alg) = &n.algorithm_label {
        push_wrapped(
            &mut lines,
            alg,
            LineClass::Algorithm,
            LineClass::Algorithm.room(),
        );
    }
    for s in &n.settings {
        push_wrapped(&mut lines, s, LineClass::Setting, LineClass::Setting.room());
    }
    if let Some(d) = &n.dataset {
        for a in &d.alternatives {
            push_wrapped(
                &mut lines,
                &d.alternative_text(a),
                LineClass::Setting,
                LineClass::Setting.room(),
            );
        }
    }
    if let Some(when) = &n.only_when {
        push_wrapped(
            &mut lines,
            &format!("Only when {when}"),
            LineClass::When,
            LineClass::When.room(),
        );
    }
    if let Some(run) = &n.run {
        lines.push(TextLine {
            text: run_state_text(run.state).to_string(),
            cls: LineClass::Run,
        });
        for d in &run.decisions {
            push_wrapped(
                &mut lines,
                &d.reason,
                LineClass::Note,
                LineClass::Note.room(),
            );
        }
    }
    if let Some(d) = &n.dataset {
        for text in d.summary() {
            push_wrapped(&mut lines, &text, LineClass::Run, LineClass::Run.room());
        }
        for c in &d.decisions {
            push_wrapped(
                &mut lines,
                &d.decision_note(c),
                LineClass::Note,
                LineClass::Note.room(),
            );
        }
    }
    lines
}

/// An arrow as drawn, with how many it stands for.
struct Drawn {
    from: String,
    to: String,
    items: Vec<String>,
    collapsed: usize,
}

/// The arrows as drawn: two from one box to another are one, and every arrow into
/// [`RETURN_STEP`] becomes one, from the last step that feeds it, carrying all their items.
fn drawn_edges(graph: &PipelineGraph) -> Vec<Drawn> {
    let order = |id: &str| graph.nodes.iter().position(|n| n.id == id).unwrap_or(0);
    let mut out: Vec<Drawn> = Vec::new();
    let add =
        |out: &mut Vec<Drawn>, from: &str, to: &str, items: &[String], collapsed: usize| match out
            .iter_mut()
            .find(|d| d.from == from && d.to == to)
        {
            Some(d) => {
                for i in items {
                    if !d.items.contains(i) {
                        d.items.push(i.clone());
                    }
                }
                d.collapsed = d.collapsed.max(collapsed);
            }
            None => out.push(Drawn {
                from: from.into(),
                to: to.into(),
                items: items.to_vec(),
                collapsed,
            }),
        };
    let into: Vec<&Edge> = graph.edges.iter().filter(|e| e.to == RETURN_STEP).collect();
    for e in graph.edges.iter().filter(|e| e.to != RETURN_STEP) {
        add(&mut out, &e.from, &e.to, &e.items, 1);
    }
    if into.len() <= 1 {
        for e in into {
            add(&mut out, &e.from, &e.to, &e.items, 1);
        }
        return out;
    }
    let mut last = into[0];
    for e in &into[1..] {
        if order(&e.from) > order(&last.from) {
            last = e;
        }
    }
    let mut items: Vec<String> = Vec::new();
    for i in into.iter().flat_map(|e| &e.items) {
        if !items.contains(i) {
            items.push(i.clone());
        }
    }
    out.push(Drawn {
        from: last.from.clone(),
        to: RETURN_STEP.into(),
        items,
        collapsed: into.len(),
    });
    out
}

/// What flows along an arrow, by item label, each once ("Phase", "Magnitude" for every echo's).
pub fn item_labels(graph: &PipelineGraph, keys: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for key in keys {
        let label = graph
            .item(key)
            .map_or_else(|| key.clone(), |i| i.label.clone());
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

/// A step an output switch asked for, drawn beside the spine.
fn side_step(n: &Node) -> bool {
    n.branch.is_none()
        && n.enabled_by
            .as_deref()
            .is_some_and(|s| s.starts_with("pipeline."))
}

/// A pass's title.
fn group_title(b: Branch) -> &'static str {
    match b {
        Branch::Main => "Main pass",
        Branch::Reliable => "Reliable pass",
    }
}

/// Which side of its source a long arrow leaves by.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

/// An arrow, with where its ends are and how it runs.
struct Arrow {
    from: String,
    to: String,
    items: Vec<String>,
    collapsed: usize,
    /// Source and target: row, column, index.
    s: (usize, usize, usize),
    t: (usize, usize, usize),
    /// It skips a row: out of the source's side, down a lane.
    long: bool,
    /// For a long arrow, its channel (channel g is left of column g) and the source's side.
    gutter: usize,
    side: Side,
    /// Its ports: a share of the source's width (bottom) or height (side), and of the target's width.
    out: f64,
    inn: f64,
    /// What flows along it, one item each.
    names: Vec<String>,
    title: String,
}

/// A label's lines, width and height, its items' names wrapped at `wrap` characters.
fn label_lines(names: &[String], collapsed: usize, wrap_at: usize) -> (Vec<String>, f64, f64) {
    let lines: Vec<String> = if collapsed > 1 {
        vec![format!("{} outputs", names.len())]
    } else {
        names.iter().flat_map(|n| wrap(n, wrap_at)).collect()
    };
    let lw = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * LABEL_CHAR + 8.0;
    let lh = lines.len() as f64 * LABEL_LINE + 3.0;
    (lines, lw, lh)
}

/// `graph`, laid out.
pub fn layout(graph: &PipelineGraph) -> Layout {
    let nodes = &graph.nodes;
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let drawn: Vec<Drawn> = drawn_edges(graph)
        .into_iter()
        .filter(|e| matches!((index.get(e.from.as_str()), index.get(e.to.as_str())), (Some(a), Some(b)) if a < b))
        .collect();

    // Columns: in a two-pass run, the main pass, the spine and the reliable pass have their own.
    let two = graph.two_pass && nodes.iter().any(|n| n.branch.is_some());
    let spine = usize::from(two);
    let dedicated = |b: Branch| -> Option<usize> {
        two.then_some(match b {
            Branch::Main => 0,
            Branch::Reliable => 2,
        })
    };
    let reserved: Vec<usize> = if two { vec![0, 2] } else { vec![] };

    // Rows and columns, in run order.
    let n_nodes = nodes.len();
    let mut row = vec![0usize; n_nodes];
    let mut col = vec![0usize; n_nodes];
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    for (i, n) in nodes.iter().enumerate() {
        let mut r = 0;
        for e in drawn.iter().filter(|e| e.to == n.id) {
            r = r.max(row[index[e.from.as_str()]] + 1);
        }
        let c = match n.branch.and_then(dedicated) {
            Some(c) => {
                while taken.contains(&(r, c)) {
                    r += 1;
                }
                c
            }
            None => {
                // A pass's column is free for other steps below its last step.
                let free = |c: usize, row: &[usize], taken: &HashSet<(usize, usize)>| {
                    if taken.contains(&(r, c)) {
                        return false;
                    }
                    let Some(b) = [Branch::Main, Branch::Reliable]
                        .into_iter()
                        .find(|&b| dedicated(b) == Some(c))
                    else {
                        return true;
                    };
                    nodes
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.branch == Some(b))
                        .all(|(j, _)| j < i && row[j] < r)
                };
                // The first free column to the right, else the first free one at all.
                let mut k = if side_step(n) { spine + 1 } else { spine };
                while k < n_nodes + 3 && !free(k, &row, &taken) {
                    k += 1;
                }
                if k >= n_nodes + 3 {
                    k = 0;
                }
                while !free(k, &row, &taken) {
                    k += 1;
                }
                k
            }
        };
        row[i] = r;
        col[i] = c;
        taken.insert((r, c));
    }
    let n_rows = row.iter().copied().max().unwrap_or(0) + 1;
    let n_cols = col.iter().copied().max().unwrap_or(0) + 1;
    let lines: Vec<Vec<TextLine>> = nodes.iter().map(node_lines).collect();
    let heights: Vec<f64> = lines
        .iter()
        .map(|ls| 2.0 * PAD + ls.iter().map(|l| l.cls.height()).sum::<f64>())
        .collect();
    let row_h: Vec<f64> = (0..n_rows)
        .map(|r| {
            (0..n_nodes)
                .filter(|&i| row[i] == r)
                .map(|i| heights[i])
                .fold(0.0, f64::max)
        })
        .collect();
    let mut first_of_group: HashSet<usize> = HashSet::new();
    let mut group_rows: HashSet<usize> = HashSet::new();
    if two {
        for b in [Branch::Main, Branch::Reliable] {
            let mut members: Vec<usize> = (0..n_nodes)
                .filter(|&i| nodes[i].branch == Some(b))
                .collect();
            members.sort_by_key(|&i| row[i]);
            if let Some(&first) = members.first() {
                first_of_group.insert(first);
                group_rows.insert(row[first]);
            }
        }
    }

    // The arrows: which run long, by which channel, and their labels.
    let rc = |id: &str| {
        let i = index[id];
        (row[i], col[i], i)
    };
    let mut arrows: Vec<Arrow> = drawn
        .into_iter()
        .map(|e| {
            let (s, t) = (rc(&e.from), rc(&e.to));
            // The channel beside the source on the target's side; the outer left one for a
            // target straight below in the first column.
            let (gutter, side) = if t.1 > s.1 || (t.1 == s.1 && s.1 > 0) {
                (s.1 + 1, Side::Right)
            } else {
                (s.1, Side::Left)
            };
            let names = item_labels(graph, &e.items);
            let title = if e.collapsed > 1 {
                format!("{} outputs: {}", names.len(), names.join(", "))
            } else {
                names.join(", ")
            };
            Arrow {
                from: e.from,
                to: e.to,
                items: e.items,
                collapsed: e.collapsed,
                s,
                t,
                long: t.0 > s.0 + 1,
                gutter,
                side,
                out: 0.5,
                inn: 0.5,
                names,
                title,
            }
        })
        .collect();

    // Ports. Out of a box's bottom, each arrow to the next row, ordered by where it heads; out of
    // each side, each long arrow, the one going furthest highest (so the lanes nest). Into a
    // box's top, each arrow, ordered by where it comes from.
    for i in 0..n_nodes {
        let mut bottom: Vec<usize> = (0..arrows.len())
            .filter(|&k| arrows[k].s.2 == i && !arrows[k].long)
            .collect();
        bottom.sort_by_key(|&k| (arrows[k].t.1, arrows[k].t.2));
        let n = bottom.len() as f64;
        for (j, &k) in bottom.iter().enumerate() {
            arrows[k].out = (j as f64 + 1.0) / (n + 1.0);
        }
        for side in [Side::Left, Side::Right] {
            let mut ks: Vec<usize> = (0..arrows.len())
                .filter(|&k| arrows[k].s.2 == i && arrows[k].long && arrows[k].side == side)
                .collect();
            ks.sort_by_key(|&k| (std::cmp::Reverse(arrows[k].t.0), arrows[k].t.2));
            let n = ks.len() as f64;
            for (j, &k) in ks.iter().enumerate() {
                arrows[k].out = (j as f64 + 1.0) / (n + 1.0);
            }
        }
    }
    for i in 0..n_nodes {
        // Where each arrow comes from, left to right: a column and a place in it.
        // The arrow from the box straight above comes in first: leftmost, with the highest track,
        // so it crosses no other arrow's label.
        let approach = |a: &Arrow| {
            if a.long {
                a.gutter as f64 - 0.5
            } else if a.s.1 == a.t.1 {
                -1.0
            } else {
                a.s.1 as f64 + a.out
            }
        };
        let mut top: Vec<usize> = (0..arrows.len()).filter(|&k| arrows[k].t.2 == i).collect();
        top.sort_by(|&a, &b| {
            approach(&arrows[a])
                .total_cmp(&approach(&arrows[b]))
                .then(arrows[a].s.2.cmp(&arrows[b].s.2))
        });
        // A pass's first step has the pass's title above its left half: its arrows come in on the right.
        let (lo, span) = if first_of_group.contains(&i) {
            (TITLE_CLEAR, 1.0 - TITLE_CLEAR)
        } else {
            (0.0, 1.0)
        };
        let n = top.len() as f64;
        for (j, &k) in top.iter().enumerate() {
            arrows[k].inn = lo + span * (j as f64 + 1.0) / (n + 1.0);
        }
    }
    // A short arrow whose ends are close in one column runs straight down when neither end has
    // another arrow on that side.
    for k in 0..arrows.len() {
        let a = &arrows[k];
        if a.long || a.s.1 != a.t.1 {
            continue;
        }
        let alone_out = !arrows
            .iter()
            .enumerate()
            .any(|(j, b)| j != k && b.s.2 == a.s.2 && !b.long);
        let alone_in = !arrows
            .iter()
            .enumerate()
            .any(|(j, b)| j != k && b.t.2 == a.t.2);
        if alone_out {
            let inn = arrows[k].inn;
            arrows[k].out = inn;
        } else if alone_in && !first_of_group.contains(&a.t.2) {
            let out = arrows[k].out;
            arrows[k].inn = out;
        }
    }

    // The arrows into each row, left to right: their tracks run top to bottom in that order.
    let entering: Vec<Vec<usize>> = (0..n_rows)
        .map(|r| {
            let mut ks: Vec<usize> = (0..arrows.len()).filter(|&k| arrows[k].t.0 == r).collect();
            ks.sort_by(|&a, &b| {
                (arrows[a].t.1 as f64 + arrows[a].inn)
                    .total_cmp(&(arrows[b].t.1 as f64 + arrows[b].inn))
            });
            ks
        })
        .collect();

    let mut extra = vec![0.0f64; arrows.len()];
    let mut wrap_at = vec![LABEL_WRAP; arrows.len()];
    // Room kept free at the left of each channel for labels that reach into it from the column
    // on its left; its lanes run to the right of it.
    let mut reserve = vec![0.0f64; n_cols + 1];
    let mut round = 0;
    loop {
        let labels: Vec<(Vec<String>, f64, f64)> = arrows
            .iter()
            .zip(&wrap_at)
            .map(|(a, &w)| label_lines(&a.names, a.collapsed, w))
            .collect();
        // Gap r is above row r: a margin, then each entering arrow's track with its label below
        // it, then room for the arrowheads, and a pass's title.
        let mut track_off = vec![0.0f64; arrows.len()];
        let gap_h: Vec<f64> = (0..=n_rows)
            .map(|r| {
                let top =
                    (if r == 0 { 0.0 } else { GAP }) + if two && r > 0 { GROUP_PAD } else { 0.0 };
                let title = if group_rows.contains(&r) {
                    GROUP_TITLE
                } else {
                    0.0
                };
                let ks = entering.get(r).map(Vec::as_slice).unwrap_or(&[]);
                let mut y = 0.0;
                for (j, &k) in ks.iter().enumerate() {
                    track_off[k] = top + y;
                    let room = labels[k].2 + 6.0 + extra[k];
                    y += if j + 1 < ks.len() {
                        TRACK.max(room)
                    } else {
                        room
                    };
                }
                top + y + if ks.is_empty() { 0.0 } else { ARROW_ROOM } + title
            })
            .collect();
        let mut row_y = Vec::with_capacity(n_rows);
        let mut gap_y = Vec::with_capacity(n_rows + 1);
        let mut y = MARGIN;
        for r in 0..n_rows {
            gap_y.push(y);
            y += gap_h[r];
            row_y.push(y);
            y += row_h[r];
        }
        gap_y.push(y);
        let graph_bottom = y + if two { GROUP_PAD } else { 0.0 };
        let track_y = |k: usize| gap_y[arrows[k].t.0] + track_off[k];
        let box_y = |i: usize| row_y[row[i]];
        let exit_y =
            |k: usize| (box_y(arrows[k].s.2) + heights[arrows[k].s.2] * arrows[k].out).round();

        // Lanes: in each channel, the arrows leaving the column on its left and those leaving the
        // column on its right, each set nested (the one starting highest outermost); arrows whose
        // runs overlap are apart.
        let mut lane_pos = vec![0usize; arrows.len()];
        let mut lane_count = vec![0usize; n_cols + 1];
        for (g, count) in lane_count.iter_mut().enumerate() {
            let mut sets: Vec<Vec<(usize, usize)>> = Vec::new();
            for side in [Side::Right, Side::Left] {
                let mut ks: Vec<usize> = (0..arrows.len())
                    .filter(|&k| arrows[k].long && arrows[k].gutter == g && arrows[k].side == side)
                    .collect();
                ks.sort_by(|&a, &b| {
                    exit_y(a)
                        .total_cmp(&exit_y(b))
                        .then(track_y(b).total_cmp(&track_y(a)))
                });
                let mut lanes: Vec<Vec<(f64, f64)>> = Vec::new();
                let mut got = Vec::new();
                for k in ks {
                    let (lo, hi) = (exit_y(k), track_y(k));
                    let l = (0..lanes.len())
                        .find(|&l| lanes[l].iter().all(|&(a, b)| hi + 2.0 < a || lo > b + 2.0))
                        .unwrap_or(lanes.len());
                    if lanes.len() <= l {
                        lanes.push(Vec::new());
                    }
                    lanes[l].push((lo, hi));
                    got.push((k, l));
                }
                sets.push(got);
            }
            let n_right = sets[0].iter().map(|&(_, l)| l + 1).max().unwrap_or(0);
            let n_left = sets[1].iter().map(|&(_, l)| l + 1).max().unwrap_or(0);
            // Left to right: the right-side set, its outermost furthest right, then the left-side
            // set, its outermost furthest left.
            for &(k, l) in &sets[0] {
                lane_pos[k] = n_right - 1 - l;
            }
            for &(k, l) in &sets[1] {
                lane_pos[k] = n_right + l;
            }
            *count = n_right + n_left;
        }
        let gutter_w: Vec<f64> = (0..=n_cols)
            .map(|g| {
                let pads =
                    if two && (reserved.contains(&g) || (g > 0 && reserved.contains(&(g - 1)))) {
                        2.0 * GROUP_PAD
                    } else {
                        0.0
                    };
                (if g == 0 || g == n_cols {
                    GUTTER / 2.0
                } else {
                    GUTTER
                }) + pads
                    + lane_count[g] as f64 * LANE
                    + reserve[g]
            })
            .collect();
        let mut col_x = Vec::with_capacity(n_cols);
        let mut x = MARGIN;
        for g in gutter_w.iter().take(n_cols) {
            x += g;
            col_x.push(x);
            x += NODE_W;
        }
        let mut width = x + gutter_w[n_cols] + MARGIN;
        let lane_x = |k: usize| {
            let g = arrows[k].gutter;
            let left = if g == 0 {
                col_x[0] - gutter_w[0]
            } else {
                col_x[g - 1] + NODE_W
            };
            (left
                + reserve[g]
                + (gutter_w[g] - reserve[g] - lane_count[g] as f64 * LANE) / 2.0
                + (lane_pos[k] as f64 + 0.5) * LANE)
                .round()
        };

        let laid: Vec<LaidNode> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| LaidNode {
                id: n.id.clone(),
                x: col_x[col[i]],
                y: box_y(i),
                w: NODE_W,
                h: heights[i],
                row: row[i],
                col: col[i],
                lines: lines[i].clone(),
                optional: n.enabled_by.is_some(),
                conditional: n.only_when.is_some(),
            })
            .collect();

        let routes: Vec<Vec<(f64, f64)>> = (0..arrows.len())
            .map(|k| {
                let a = &arrows[k];
                let (src, dst) = (&laid[a.s.2], &laid[a.t.2]);
                let tx = (dst.x + dst.w * a.inn).round();
                let ty = track_y(k);
                if a.long {
                    let sx = if a.side == Side::Left {
                        src.x
                    } else {
                        src.x + src.w
                    };
                    let (sy, lx) = (exit_y(k), lane_x(k));
                    vec![(sx, sy), (lx, sy), (lx, ty), (tx, ty), (tx, dst.y)]
                } else {
                    let sx = (src.x + src.w * a.out).round();
                    if (sx - tx).abs() < 1.0 {
                        vec![(tx, src.y + src.h), (tx, dst.y)]
                    } else {
                        vec![(sx, src.y + src.h), (sx, ty), (tx, ty), (tx, dst.y)]
                    }
                }
            })
            .collect();

        let mut groups = Vec::new();
        if two {
            for b in [Branch::Main, Branch::Reliable] {
                let members: Vec<&LaidNode> = laid
                    .iter()
                    .zip(nodes)
                    .filter(|(_, n)| n.branch == Some(b))
                    .map(|(l, _)| l)
                    .collect();
                let Some(first) = members.first() else {
                    continue;
                };
                let top = members.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
                let bottom = members
                    .iter()
                    .map(|n| n.y + n.h)
                    .fold(f64::NEG_INFINITY, f64::max);
                groups.push(Group {
                    branch: b,
                    title: group_title(b).to_string(),
                    x: first.x - GROUP_PAD,
                    y: top - GROUP_TITLE - 2.0,
                    w: NODE_W + 2.0 * GROUP_PAD,
                    h: bottom - top + GROUP_TITLE + 2.0 + GROUP_PAD,
                });
            }
        }

        // Labels: beside the last drop, else wherever along the arrow is free, else the gap grows.
        let mut obstacles: Vec<Rect> = laid
            .iter()
            .map(|n| Rect {
                x: n.x - 2.0,
                y: n.y - 2.0,
                w: n.w + 4.0,
                h: n.h + 4.0,
            })
            .collect();
        for g in &groups {
            obstacles.push(Rect {
                x: g.x,
                y: g.y,
                w: 100.0,
                h: GROUP_TITLE,
            });
        }
        let segments: Vec<Vec<Rect>> = routes
            .iter()
            .map(|pts| {
                pts.windows(2)
                    .map(|w| {
                        let (x0, x1) = (w[0].0.min(w[1].0), w[0].0.max(w[1].0));
                        let (y0, y1) = (w[0].1.min(w[1].1), w[0].1.max(w[1].1));
                        Rect {
                            x: x0 - 1.5,
                            y: y0 - 1.5,
                            w: x1 - x0 + 3.0,
                            h: y1 - y0 + 3.0,
                        }
                    })
                    .collect()
            })
            .collect();
        let mut placed: Vec<Rect> = Vec::new();
        let mut boxes = vec![
            Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0
            };
            arrows.len()
        ];
        let mut grew = false;
        for k in entering.iter().flatten().copied() {
            let a = &arrows[k];
            let pts = &routes[k];
            let (lw, lh) = (labels[k].1, labels[k].2);
            let cost = |r: &Rect, placed: &[Rect]| -> f64 {
                let mut c: f64 = obstacles.iter().chain(placed).map(|o| r.overlap(o)).sum();
                c += segments
                    .iter()
                    .flatten()
                    .map(|seg| r.overlap(seg))
                    .sum::<f64>();
                if r.x < MARGIN / 2.0 {
                    c += 1e6;
                }
                c
            };
            // What a wider gap cannot cure: a vertical line through the label's place.
            let crossed = |r: &Rect| {
                r.x < MARGIN / 2.0
                    || segments
                        .iter()
                        .flatten()
                        .any(|seg| seg.w <= 3.5 && r.overlap(seg) > 0.0)
            };
            // Beside the drop into the target, below the track: right, then left.
            let (tx, _) = pts[pts.len() - 2];
            let below_track = track_y(k) + 3.0;
            let candidates = [
                Rect {
                    x: tx + LABEL_GAP,
                    y: below_track,
                    w: lw,
                    h: lh,
                },
                Rect {
                    x: tx - LABEL_GAP - lw,
                    y: below_track,
                    w: lw,
                    h: lh,
                },
            ];
            let chosen = match candidates.iter().find(|r| cost(r, &placed) == 0.0) {
                Some(r) => *r,
                None => {
                    if round < ROUNDS {
                        if candidates.iter().all(&crossed) {
                            // A line runs through both places: wrap the label narrower, to the
                            // widest that clears it on either side.
                            // Never inside a word.
                            let longest = a
                                .names
                                .iter()
                                .flat_map(|n| n.split_whitespace())
                                .map(|w| w.chars().count())
                                .max()
                                .unwrap_or(1);
                            let fits = (longest..wrap_at[k]).rev().find(|&w| {
                                let (_, w2, h2) = label_lines(&a.names, a.collapsed, w);
                                [
                                    Rect {
                                        x: tx + LABEL_GAP,
                                        y: below_track,
                                        w: w2,
                                        h: h2,
                                    },
                                    Rect {
                                        x: tx - LABEL_GAP - w2,
                                        y: below_track,
                                        w: w2,
                                        h: h2,
                                    },
                                ]
                                .iter()
                                .any(|r| !crossed(r))
                            });
                            if let Some(w) = fits {
                                wrap_at[k] = w;
                                grew = true;
                            } else {
                                // No wrap clears it: move the lanes of the channel to the right
                                // of the target's column clear of the label.
                                let c = a.t.1;
                                let edge = col_x[c] + NODE_W;
                                let need = tx + LABEL_GAP + lw + 4.0 - edge;
                                if need > reserve[c + 1] {
                                    reserve[c + 1] = need;
                                    grew = true;
                                }
                            }
                        } else {
                            extra[k] += lh.min(2.0 * LABEL_LINE);
                            grew = true;
                        }
                    }
                    candidates
                        .iter()
                        .copied()
                        .min_by(|p, q| cost(p, &placed).total_cmp(&cost(q, &placed)))
                        .unwrap_or(candidates[0])
                }
            };
            width = width.max(chosen.x + chosen.w + MARGIN);
            placed.push(chosen);
            boxes[k] = chosen;
        }
        round += 1;
        if grew && round <= ROUNDS {
            continue;
        }

        // The key, below the graph: the styles the graph uses.
        let mut styles: Vec<(KeyStyle, &str)> = Vec::new();
        if nodes.iter().any(|n| n.enabled_by.is_some()) {
            styles.push((KeyStyle::Optional, "optional"));
        }
        if nodes.iter().any(|n| n.only_when.is_some()) {
            styles.push((KeyStyle::Conditional, "only in some runs"));
        }
        if nodes.iter().any(|n| {
            n.run
                .as_ref()
                .is_some_and(|r| r.state == StepState::Skipped)
        }) {
            styles.push((KeyStyle::Skipped, "skipped"));
        }
        if nodes.iter().any(|n| !n.models.is_empty()) {
            styles.push((KeyStyle::Model, "runs a deep-learning model"));
        }
        if arrows.iter().any(|a| a.collapsed > 1) {
            styles.push((KeyStyle::Collapsed, "several arrows drawn as one"));
        }
        let mut key = Vec::new();
        let mut height = graph_bottom + MARGIN;
        if !styles.is_empty() {
            let (mut kx, mut ky) = (MARGIN, graph_bottom + MARGIN);
            for (style, text) in styles {
                let w = KEY_SAMPLE_W + 10.0 + text.chars().count() as f64 * KEY_CHAR + 16.0;
                if kx > MARGIN && kx + w > width - MARGIN {
                    kx = MARGIN;
                    ky += KEY_H;
                }
                key.push(KeyEntry {
                    style,
                    text: text.to_string(),
                    x: kx,
                    y: ky,
                });
                kx += w;
            }
            height = ky + KEY_H + MARGIN;
        }

        let edges = arrows
            .iter()
            .enumerate()
            .map(|(k, a)| LaidEdge {
                from: a.from.clone(),
                to: a.to.clone(),
                items: a.items.clone(),
                label: labels[k].0.clone(),
                title: a.title.clone(),
                points: routes[k].clone(),
                label_box: boxes[k],
                collapsed: a.collapsed,
            })
            .collect();
        return Layout {
            width,
            height,
            nodes: laid,
            edges,
            groups,
            key,
        };
    }
}

/// An SVG path through `points` with rounded corners.
pub fn rounded_path(points: &[(f64, f64)], radius: f64) -> String {
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for &p in points {
        if pts.last() != Some(&p) {
            pts.push(p);
        }
    }
    let Some(&(x0, y0)) = pts.first() else {
        return String::new();
    };
    let mut d = format!("M{},{}", num(x0), num(y0));
    for i in 1..pts.len() {
        let (x, y) = pts[i];
        let Some(&(nx, ny)) = pts.get(i + 1) else {
            d.push_str(&format!(" L{},{}", num(x), num(y)));
            break;
        };
        let (px, py) = pts[i - 1];
        let r = radius
            .min((x - px).hypot(y - py) / 2.0)
            .min((nx - x).hypot(ny - y) / 2.0);
        let sign = |v: f64| {
            if v > 0.0 {
                1.0
            } else if v < 0.0 {
                -1.0
            } else {
                0.0
            }
        };
        let (ux, uy, vx, vy) = (sign(x - px), sign(y - py), sign(nx - x), sign(ny - y));
        d.push_str(&format!(
            " L{},{} Q{},{} {},{}",
            num(x - ux * r),
            num(y - uy * r),
            num(x),
            num(y),
            num(x + vx * r),
            num(y + vy * r)
        ));
    }
    d
}

/// A coordinate as SVG gives it: at most two decimals, no trailing zeros.
pub(crate) fn num(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}
