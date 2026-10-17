//! Where each box and arrow of a [`PipelineGraph`] goes when it is drawn: one box per step, top
//! to bottom in run order, with every arrow routed between the boxes. [`PipelineGraph::to_svg`]
//! draws it; a front end can draw the same [`Layout`] (it serialises to JSON).
//!
//! The graph is small (10–25 steps) and its steps are in run order, so a layered layout does the
//! job without a layout library:
//!
//! - **Rows.** A step sits one row below the lowest step it reads from (longest path).
//! - **Columns.** The steps every run takes form the spine (column 0). A step an output switch
//!   asked for (`enabled_by` = `pipeline.*`) goes to the first free column to its right, as does a
//!   spine step whose row is taken. In a two-pass run each pass has a column of its own, so each
//!   pass's box holds its steps and nothing else: main pass, then the shared spine, then reliable.
//! - **Arrows** run only between boxes: down from the source into the gap below it, along a
//!   horizontal track in that gap, down a gutter between columns, along a track in the gap above
//!   the target, and down into it. Each arrow has its own track and gutter lane, so no arrow
//!   crosses a box and no two share a segment; its label sits on its last track.
//! - The many arrows into the return to the acquired grid are drawn as one (the data is unchanged).
//!
//! Text is measured by an estimate of its width, so the layout is a pure function of the graph.
//! The constants and rules are those of qsmbly's Pipeline tab, so the two draw the same picture.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use super::{Branch, Edge, Node, PipelineGraph};
use crate::record::StepState;

/// The step whose arrows are drawn as one.
pub const RETURN_STEP: &str = "output_space";

/// A box's width.
pub const NODE_W: f64 = 210.0;
const PAD: f64 = 8.0;
const TRACK: f64 = 15.0; // between horizontal tracks in a gap
const GAP: f64 = 14.0; // a gap's margin above and below its tracks
const LANE: f64 = 7.0; // between vertical lanes in a gutter
const GUTTER: f64 = 28.0; // a gutter's width with no lanes
const GROUP_PAD: f64 = 9.0;
const GROUP_TITLE: f64 = 18.0;
/// The share of a pass's first box, from the left, its title sits above.
const TITLE_CLEAR: f64 = 0.55;
const MARGIN: f64 = 12.0;
const LABEL_MAX: usize = 34; // characters
/// An edge label's average glyph width.
const LABEL_CHAR: f64 = 5.4;

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

/// A label's box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// An arrow, routed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaidEdge {
    pub from: String,
    pub to: String,
    /// What flows along it, by item key.
    pub items: Vec<String>,
    /// What flows along it, as drawn (clipped).
    pub label: String,
    /// The label in full.
    pub title: String,
    /// The route, from the source's bottom to the target's top.
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

/// A graph, laid out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<LaidNode>,
    pub edges: Vec<LaidEdge>,
    pub groups: Vec<Group>,
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

/// `text`, cut to `max` characters with an ellipsis.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut s: String = text.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        text.to_string()
    }
}

/// What a run's step did, as its box says it.
pub fn run_state_text(state: StepState) -> &'static str {
    match state {
        StepState::Computed => "Computed",
        StepState::Cached => "Reused from the cache",
        StepState::Skipped => "Skipped",
    }
}

/// `text` on at most two lines of `cls`, the second clipped.
fn two_lines(text: &str, cls: LineClass, width: usize) -> Vec<TextLine> {
    let ls = wrap(text, width);
    let shown = if ls.len() > 2 {
        vec![ls[0].clone(), clip(&ls[1..].join(" "), width)]
    } else {
        ls
    };
    shown
        .into_iter()
        .map(|text| TextLine { text, cls })
        .collect()
}

/// A box's text: the step, its algorithm, its settings and when it applies; after a run, what the
/// step did and what it decided; for a dataset's runs, how many took it and what they decided.
/// Long notes are cut to two lines; [`Node::tooltip`] has them in full.
pub fn node_lines(n: &Node) -> Vec<TextLine> {
    let width = LineClass::Title.room() - if n.models.is_empty() { 0 } else { 7 };
    let mut lines = two_lines(&n.label, LineClass::Title, width);
    if let Some(alg) = &n.algorithm_label {
        lines.push(TextLine {
            text: clip(alg, LineClass::Algorithm.room()),
            cls: LineClass::Algorithm,
        });
    }
    for s in &n.settings {
        for text in wrap(s, LineClass::Setting.room()) {
            lines.push(TextLine {
                text,
                cls: LineClass::Setting,
            });
        }
    }
    if let Some(when) = &n.only_when {
        for text in wrap(&format!("Only when {when}"), LineClass::When.room()) {
            lines.push(TextLine {
                text,
                cls: LineClass::When,
            });
        }
    }
    if let Some(run) = &n.run {
        lines.push(TextLine {
            text: run_state_text(run.state).to_string(),
            cls: LineClass::Run,
        });
        for d in &run.decisions {
            lines.extend(two_lines(
                &d.reason,
                LineClass::Note,
                LineClass::Note.room(),
            ));
        }
    }
    if let Some(d) = &n.dataset {
        for text in d.summary() {
            lines.push(TextLine {
                text,
                cls: LineClass::Run,
            });
        }
        for c in &d.decisions {
            lines.extend(two_lines(
                &d.decision_note(c),
                LineClass::Note,
                LineClass::Note.room(),
            ));
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

/// The arrows as drawn: every arrow into [`RETURN_STEP`] becomes one, from the last step that
/// feeds it, carrying all their items.
fn drawn_edges(graph: &PipelineGraph) -> Vec<Drawn> {
    let order = |id: &str| graph.nodes.iter().position(|n| n.id == id).unwrap_or(0);
    let one = |e: &Edge| Drawn {
        from: e.from.clone(),
        to: e.to.clone(),
        items: e.items.clone(),
        collapsed: 1,
    };
    let into: Vec<&Edge> = graph.edges.iter().filter(|e| e.to == RETURN_STEP).collect();
    let mut out: Vec<Drawn> = graph
        .edges
        .iter()
        .filter(|e| e.to != RETURN_STEP)
        .map(one)
        .collect();
    if into.len() <= 1 {
        out.extend(into.into_iter().map(one));
        return out;
    }
    let mut last = into[0];
    for e in &into[1..] {
        if order(&e.from) > order(&last.from) {
            last = e;
        }
    }
    let mut items: Vec<String> = Vec::new();
    for key in into.iter().flat_map(|e| &e.items) {
        if !items.contains(key) {
            items.push(key.clone());
        }
    }
    out.push(Drawn {
        from: last.from.clone(),
        to: RETURN_STEP.to_string(),
        items,
        collapsed: into.len(),
    });
    out
}

/// What flows along an arrow, by item label, each once ("Phase, Magnitude" for every echo's).
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

/// Where an arrow runs: its ends' rows and columns, and the tracks and lane it takes.
#[derive(Clone, Copy)]
struct Route {
    s: (usize, usize, usize), // row, column, index
    t: (usize, usize, usize),
    long: bool,
    gutter: usize,
    lane: usize,
    exit_track: usize,
    entry_track: usize,
}

/// A pass's title.
fn group_title(b: Branch) -> &'static str {
    match b {
        Branch::Main => "Main pass",
        Branch::Reliable => "Reliable pass",
    }
}

/// `graph`, laid out.
pub fn layout(graph: &PipelineGraph) -> Layout {
    let nodes = &graph.nodes;
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let edges: Vec<Drawn> = drawn_edges(graph)
        .into_iter()
        .filter(
            |e| match (index.get(e.from.as_str()), index.get(e.to.as_str())) {
                (Some(a), Some(b)) => a < b,
                _ => false,
            },
        )
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
        for e in edges.iter().filter(|e| e.to == n.id) {
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

    let rc = |id: &str| {
        let i = index[id];
        (row[i], col[i], i)
    };
    let is_long = |e: &Drawn| rc(&e.to).0 > rc(&e.from).0 + 1;
    let gutter_of = |e: &Drawn| {
        let (s, t) = (rc(&e.from), rc(&e.to));
        if s.1 < t.1 {
            s.1 + 1
        } else if s.1 > t.1 {
            s.1
        } else if s.1 == 0 {
            0 // the left edge for the first column
        } else {
            s.1 + 1
        }
    };
    let mut first_of_group: HashSet<&str> = HashSet::new();
    if two {
        for b in [Branch::Main, Branch::Reliable] {
            let mut members: Vec<usize> = (0..n_nodes)
                .filter(|&i| nodes[i].branch == Some(b))
                .collect();
            members.sort_by_key(|&i| row[i]);
            if let Some(&first) = members.first() {
                first_of_group.insert(&nodes[first].id);
            }
        }
    }

    // Ports, as fractions of a box's width: each arrow into a step enters at its own point, in
    // the order of the columns it comes from. Out of a step, each arrow to the next row leaves at
    // its own point, and the arrows that go further leave together (and share their way down a
    // gutter until they part).
    let mut port_out = vec![0.5; edges.len()];
    let mut port_in = vec![0.5; edges.len()];
    for n in nodes {
        let outs: Vec<usize> = (0..edges.len())
            .filter(|&k| edges[k].from == n.id)
            .collect();
        // Slots: each short arrow, and one for all the long ones, by where they head.
        let mut slots: Vec<(f64, isize, Vec<usize>)> = outs
            .iter()
            .filter(|&&k| !is_long(&edges[k]))
            .map(|&k| {
                let t = rc(&edges[k].to);
                (t.1 as f64, t.2 as isize, vec![k])
            })
            .collect();
        let longs: Vec<usize> = outs
            .iter()
            .copied()
            .filter(|&k| is_long(&edges[k]))
            .collect();
        if !longs.is_empty() {
            let at = longs
                .iter()
                .map(|&k| gutter_of(&edges[k]))
                .min()
                .unwrap_or(0) as f64
                - 0.5;
            slots.push((at, -1, longs));
        }
        slots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let n_slots = slots.len() as f64;
        for (k, slot) in slots.iter().enumerate() {
            for &e in &slot.2 {
                port_out[e] = (k as f64 + 1.0) / (n_slots + 1.0);
            }
        }
        let mut ins: Vec<usize> = (0..edges.len()).filter(|&k| edges[k].to == n.id).collect();
        ins.sort_by_key(|&k| {
            let s = rc(&edges[k].from);
            (s.1, s.2)
        });
        // A pass's first step has the pass's title above its left half: its arrows come in on the right.
        let (lo, span) = if first_of_group.contains(n.id.as_str()) {
            (TITLE_CLEAR, 1.0 - TITLE_CLEAR)
        } else {
            (0.0, 1.0)
        };
        let n_ins = ins.len() as f64;
        for (k, &e) in ins.iter().enumerate() {
            port_in[e] = lo + span * (k as f64 + 1.0) / (n_ins + 1.0);
        }
    }

    // Each arrow's tracks (gap g is the one above row g) and, for a long one, its gutter (gutter
    // g is left of column g) and lane in it.
    let mut routes: Vec<Route> = edges
        .iter()
        .map(|e| {
            let long = is_long(e);
            Route {
                s: rc(&e.from),
                t: rc(&e.to),
                long,
                gutter: if long { gutter_of(e) } else { 0 },
                lane: 0,
                exit_track: 0,
                entry_track: 0,
            }
        })
        .collect();
    let mut gap_exit: Vec<Vec<&str>> = vec![Vec::new(); n_rows + 1];
    let mut gap_entry: Vec<Vec<usize>> = vec![Vec::new(); n_rows + 1];
    // Entry tracks: per gap, the arrows into that row's steps, left to right.
    for (r, entry) in gap_entry.iter_mut().enumerate().take(n_rows) {
        let mut list: Vec<usize> = (0..edges.len()).filter(|&k| routes[k].t.0 == r).collect();
        list.sort_by(|&a, &b| {
            routes[a]
                .t
                .1
                .cmp(&routes[b].t.1)
                .then(port_in[a].total_cmp(&port_in[b]))
        });
        *entry = list;
    }
    // Exit tracks: one per step with long arrows, in the gap below it.
    for (k, e) in edges.iter().enumerate() {
        let g = &mut gap_exit[routes[k].s.0 + 1];
        if routes[k].long && !g.contains(&e.from.as_str()) {
            g.push(&e.from);
        }
    }
    for (r, entry) in gap_entry.iter().enumerate() {
        for (k, &e) in entry.iter().enumerate() {
            routes[e].entry_track = gap_exit[r].len() + k;
        }
    }
    for (k, e) in edges.iter().enumerate() {
        if routes[k].long {
            let g = &gap_exit[routes[k].s.0 + 1];
            routes[k].exit_track = g.iter().position(|id| *id == e.from).unwrap_or(0);
        }
    }
    // Gutter lanes: one per step and gutter, from below the step to its furthest target there;
    // lanes whose rows overlap are apart.
    let mut lanes: Vec<BTreeMap<usize, Vec<(usize, usize)>>> = vec![BTreeMap::new(); n_cols + 1];
    struct Bundle {
        gutter: usize,
        from: usize,
        to: usize,
        list: Vec<usize>,
    }
    let mut bundles: Vec<(usize, String, Bundle)> = Vec::new();
    for (k, e) in edges.iter().enumerate().filter(|(k, _)| routes[*k].long) {
        let ro = routes[k];
        match bundles
            .iter_mut()
            .find(|(g, from, _)| *g == ro.gutter && *from == e.from)
        {
            Some((_, _, b)) => {
                b.to = b.to.max(ro.t.0);
                b.list.push(k);
            }
            None => bundles.push((
                ro.gutter,
                e.from.clone(),
                Bundle {
                    gutter: ro.gutter,
                    from: ro.s.0 + 1,
                    to: ro.t.0,
                    list: vec![k],
                },
            )),
        }
    }
    let mut bundles: Vec<Bundle> = bundles.into_iter().map(|(_, _, b)| b).collect();
    bundles.sort_by_key(|b| (b.from, b.to));
    for b in &bundles {
        let used = &mut lanes[b.gutter];
        let mut k = 0;
        while used
            .get(&k)
            .is_some_and(|spans| spans.iter().any(|&(lo, hi)| !(b.to < lo || b.from > hi)))
        {
            k += 1;
        }
        used.entry(k).or_default().push((b.from, b.to));
        for &e in &b.list {
            routes[e].lane = k;
        }
    }

    // Sizes and positions.
    let mut group_rows: HashSet<usize> = HashSet::new();
    if two {
        for b in [Branch::Main, Branch::Reliable] {
            if let Some(r) = (0..n_nodes)
                .filter(|&i| nodes[i].branch == Some(b))
                .map(|i| row[i])
                .min()
            {
                group_rows.insert(r);
            }
        }
    }
    let gap_h: Vec<f64> = (0..=n_rows)
        .map(|r| {
            let tracks = (gap_exit[r].len() + gap_entry[r].len()) as f64;
            let title = if group_rows.contains(&r) {
                GROUP_TITLE
            } else {
                0.0
            };
            let group_bottom = if two && r > 0 { GROUP_PAD } else { 0.0 };
            (if r == 0 { 0.0 } else { GAP })
                + tracks * TRACK
                + if tracks > 0.0 { GAP - 4.0 } else { 0.0 }
                + title
                + group_bottom
        })
        .collect();
    let gutter_w: Vec<f64> = (0..=n_cols)
        .map(|g| {
            let pads = if two && (reserved.contains(&g) || (g > 0 && reserved.contains(&(g - 1)))) {
                2.0 * GROUP_PAD
            } else {
                0.0
            };
            (if g == 0 || g == n_cols {
                GUTTER / 2.0
            } else {
                GUTTER
            }) + pads
                + lanes[g].len() as f64 * LANE
        })
        .collect();
    let mut col_x = Vec::with_capacity(n_cols);
    let mut x = MARGIN;
    for g in gutter_w.iter().take(n_cols) {
        x += g;
        col_x.push(x);
        x += NODE_W;
    }
    let width = x + gutter_w[n_cols] + MARGIN;
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
    let height = y + MARGIN + if two { GROUP_PAD } else { 0.0 };

    let laid: Vec<LaidNode> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| LaidNode {
            id: n.id.clone(),
            x: col_x[col[i]],
            y: row_y[row[i]],
            w: NODE_W,
            h: heights[i],
            row: row[i],
            col: col[i],
            lines: lines[i].clone(),
            optional: n.enabled_by.is_some(),
            conditional: n.only_when.is_some(),
        })
        .collect();
    // Track k of gap r: below the margin (and below a pass's bottom edge).
    let track_y = |r: usize, k: usize| {
        gap_y[r]
            + (if r == 0 { 0.0 } else { GAP })
            + (if two && r > 0 { GROUP_PAD } else { 0.0 })
            + k as f64 * TRACK
            + 4.0
    };
    let lane_x = |g: usize, k: usize| {
        let left = if g == 0 {
            col_x[0] - gutter_w[0]
        } else {
            col_x[g - 1] + NODE_W
        };
        left + gutter_w[g] / 2.0 + (k as f64 - (lanes[g].len() as f64 - 1.0) / 2.0) * LANE
    };

    let laid_edges: Vec<LaidEdge> = edges
        .iter()
        .enumerate()
        .map(|(k, e)| {
            let ro = routes[k];
            let (a, b) = (&laid[ro.s.2], &laid[ro.t.2]);
            let sx = (a.x + a.w * port_out[k]).round();
            let tx = (b.x + b.w * port_in[k]).round();
            let ty = track_y(ro.t.0, ro.entry_track);
            let points = if ro.long {
                let ey = track_y(ro.s.0 + 1, ro.exit_track);
                let gx = lane_x(ro.gutter, ro.lane).round();
                vec![
                    (sx, a.y + a.h),
                    (sx, ey),
                    (gx, ey),
                    (gx, ty),
                    (tx, ty),
                    (tx, b.y),
                ]
            } else {
                vec![(sx, a.y + a.h), (sx, ty), (tx, ty), (tx, b.y)]
            };
            let names = item_labels(graph, &e.items);
            let title = if e.collapsed > 1 {
                format!("{} outputs: {}", names.len(), names.join(", "))
            } else {
                names.join(", ")
            };
            let label = clip(
                &if e.collapsed > 1 {
                    format!("{} outputs", names.len())
                } else {
                    title.clone()
                },
                LABEL_MAX,
            );
            let lw = (label.chars().count() as f64 * LABEL_CHAR).ceil() + 8.0;
            // On its last track, beside the drop into the target: to the right, or to the left
            // where it would run off the drawing. A gap holds no boxes and each track one label,
            // so it overlaps neither.
            let lx = if tx + 3.0 + lw <= width - MARGIN / 2.0 {
                tx + 3.0
            } else {
                (MARGIN / 2.0).max(tx - 3.0 - lw)
            };
            LaidEdge {
                from: e.from.clone(),
                to: e.to.clone(),
                items: e.items.clone(),
                label,
                title,
                points,
                label_box: Rect {
                    x: lx,
                    y: ty + 1.0,
                    w: lw,
                    h: TRACK - 2.0,
                },
                collapsed: e.collapsed,
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
    Layout {
        width,
        height,
        nodes: laid,
        edges: laid_edges,
        groups,
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
