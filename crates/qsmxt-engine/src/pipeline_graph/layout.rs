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

mod arrows;
mod grid;
mod place;

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
    for key in &n.publishes {
        push_wrapped(&mut lines, &super::output_text(key), LineClass::Setting, LineClass::Setting.room());
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

/// Where each box goes ([`grid::grid`]): its row and column, and how tall it and its row are.
struct Grid {
    /// A two-pass run, with a column of its own for each pass.
    two: bool,
    /// The passes' columns.
    reserved: Vec<usize>,
    row: Vec<usize>,
    col: Vec<usize>,
    n_rows: usize,
    n_cols: usize,
    /// Each box's text and height.
    lines: Vec<Vec<TextLine>>,
    heights: Vec<f64>,
    /// Each row's height: its tallest box's.
    row_h: Vec<f64>,
    /// Each pass's first step, and the rows they are in.
    first_of_group: HashSet<usize>,
    group_rows: HashSet<usize>,
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

    let grid = grid::grid(graph, &index, &drawn);
    let mut arrows = arrows::arrows(graph, drawn, &index, &grid);
    arrows::ports(&mut arrows, &grid);
    let entering = arrows::entering(&arrows, grid.n_rows);
    place::place(nodes, &grid, &arrows, &entering)
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
