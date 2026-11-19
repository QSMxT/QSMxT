//! The boxes, arrows, labels, passes and key placed ([`super::layout`]): rounds of placing, each
//! making more room where a label did not fit, for at most [`ROUNDS`].

use super::{
    group_title, label_lines, Arrow, Grid, Group, KeyEntry, KeyStyle, LaidEdge, LaidNode, Layout, Rect, Side,
    ARROW_ROOM, GAP, GROUP_PAD, GROUP_TITLE, GUTTER, KEY_CHAR, KEY_H, KEY_SAMPLE_W, LABEL_GAP, LABEL_LINE, LABEL_WRAP,
    LANE, MARGIN, NODE_W, ROUNDS, TRACK,
};
use crate::pipeline_graph::{Branch, Node};
use crate::record::StepState;

/// What grows from round to round to make room for the labels.
struct Room {
    /// Below each arrow's label.
    extra: Vec<f64>,
    /// The width each arrow's label wraps at.
    wrap_at: Vec<usize>,
    /// Room kept free at the left of each channel for labels that reach into it from the column
    /// on its left; its lanes run to the right of it.
    reserve: Vec<f64>,
}

/// The boxes, arrows, passes and key of a graph of `nodes`, its boxes in `grid` and its `arrows`
/// into each row `entering` it.
pub(super) fn place(nodes: &[Node], grid: &Grid, arrows: &[Arrow], entering: &[Vec<usize>]) -> Layout {
    let mut room = Room {
        extra: vec![0.0f64; arrows.len()],
        wrap_at: vec![LABEL_WRAP; arrows.len()],
        reserve: vec![0.0f64; grid.n_cols + 1],
    };
    let mut round = 0;
    loop {
        let labels: Vec<(Vec<String>, f64, f64)> = arrows
            .iter()
            .zip(&room.wrap_at)
            .map(|(a, &w)| label_lines(&a.names, a.collapsed, w))
            .collect();
        let ys = Ys::new(grid, arrows, entering, &labels, &room.extra);
        let (lane_pos, lane_count) = lanes(arrows, grid.n_cols, &ys);
        let (gutter_w, col_x, mut width) = columns(grid, &lane_count, &room.reserve);
        let reserve = &room.reserve;
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

        let laid = laid_nodes(nodes, grid, &col_x, &ys);
        let routes = routes(arrows, &laid, &ys, lane_x);
        let groups = groups(nodes, &laid, grid.two);
        let (obstacles, segments) = (obstacles(&laid, &groups), segments(&routes));
        let scene = Scene { arrows, labels: &labels, routes: &routes, obstacles: &obstacles, segments: &segments, col_x: &col_x, ys: &ys };
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
            let chosen = place_label(&scene, k, &placed, &mut room, round, &mut grew);
            width = width.max(chosen.x + chosen.w + MARGIN);
            placed.push(chosen);
            boxes[k] = chosen;
        }
        round += 1;
        if grew && round <= ROUNDS {
            continue;
        }

        let (key, height) = key(nodes, arrows, ys.graph_bottom, width);

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

/// Where each row, gap and track is, top to bottom.
struct Ys<'a> {
    arrows: &'a [Arrow],
    row: &'a [usize],
    heights: &'a [f64],
    track_off: Vec<f64>,
    row_y: Vec<f64>,
    gap_y: Vec<f64>,
    graph_bottom: f64,
}

impl<'a> Ys<'a> {
    /// The gaps above each row grown to hold the `labels` of the arrows `entering` it, with
    /// `extra` room below each.
    fn new(
        grid: &'a Grid, arrows: &'a [Arrow], entering: &[Vec<usize>], labels: &[(Vec<String>, f64, f64)], extra: &[f64],
    ) -> Self {
        let (two, n_rows, group_rows, row_h) = (grid.two, grid.n_rows, &grid.group_rows, &grid.row_h);
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
        Ys { arrows, row: &grid.row, heights: &grid.heights, track_off, row_y, gap_y, graph_bottom }
    }

    /// Arrow `k`'s track.
    fn track_y(&self, k: usize) -> f64 {
        self.gap_y[self.arrows[k].t.0] + self.track_off[k]
    }

    /// Box `i`'s top.
    fn box_y(&self, i: usize) -> f64 {
        self.row_y[self.row[i]]
    }

    /// Where arrow `k` leaves its source.
    fn exit_y(&self, k: usize) -> f64 {
        let arrows = self.arrows;
        (self.box_y(arrows[k].s.2) + self.heights[arrows[k].s.2] * arrows[k].out).round()
    }
}

/// Lanes: in each channel, the arrows leaving the column on its left and those leaving the
/// column on its right, each set nested (the one starting highest outermost); arrows whose
/// runs overlap are apart.
fn lanes(arrows: &[Arrow], n_cols: usize, ys: &Ys) -> (Vec<usize>, Vec<usize>) {
    let (track_y, exit_y) = (|k: usize| ys.track_y(k), |k: usize| ys.exit_y(k));
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
    (lane_pos, lane_count)
}

/// Each channel's width, each column's left edge, and the graph's width.
fn columns(grid: &Grid, lane_count: &[usize], reserve: &[f64]) -> (Vec<f64>, Vec<f64>, f64) {
    let (two, n_cols, reserved) = (grid.two, grid.n_cols, &grid.reserved);
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
    let width = x + gutter_w[n_cols] + MARGIN;
    (gutter_w, col_x, width)
}

/// The boxes, placed.
fn laid_nodes(nodes: &[Node], grid: &Grid, col_x: &[f64], ys: &Ys) -> Vec<LaidNode> {
    let (row, col, lines, heights) = (&grid.row, &grid.col, &grid.lines, &grid.heights);
    let box_y = |i: usize| ys.box_y(i);
    nodes
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
        .collect()
}

/// Each arrow's route, from its source to its target's top.
fn routes(arrows: &[Arrow], laid: &[LaidNode], ys: &Ys, lane_x: impl Fn(usize) -> f64) -> Vec<Vec<(f64, f64)>> {
    let (track_y, exit_y) = (|k: usize| ys.track_y(k), |k: usize| ys.exit_y(k));
    (0..arrows.len())
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
        .collect()
}

/// A two-pass run's passes, each a box around its steps.
fn groups(nodes: &[Node], laid: &[LaidNode], two: bool) -> Vec<Group> {
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
    groups
}

/// What a label must not lie on: the boxes, and the passes' titles.
fn obstacles(laid: &[LaidNode], groups: &[Group]) -> Vec<Rect> {
    let mut obstacles: Vec<Rect> = laid
        .iter()
        .map(|n| Rect {
            x: n.x - 2.0,
            y: n.y - 2.0,
            w: n.w + 4.0,
            h: n.h + 4.0,
        })
        .collect();
    for g in groups {
        obstacles.push(Rect {
            x: g.x,
            y: g.y,
            w: 100.0,
            h: GROUP_TITLE,
        });
    }
    obstacles
}

/// Each arrow's runs, as thin rectangles.
fn segments(routes: &[Vec<(f64, f64)>]) -> Vec<Vec<Rect>> {
    routes
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
        .collect()
}

/// What a label is placed among.
struct Scene<'a> {
    arrows: &'a [Arrow],
    labels: &'a [(Vec<String>, f64, f64)],
    routes: &'a [Vec<(f64, f64)>],
    obstacles: &'a [Rect],
    segments: &'a [Vec<Rect>],
    col_x: &'a [f64],
    ys: &'a Ys<'a>,
}

/// Labels: beside the last drop, else wherever along the arrow is free, else the gap grows.
///
/// Arrow `k`'s label, among those `placed` so far. Where none of its places is free, `room` grows
/// (and `grew` is set) for the next round, if there is one.
fn place_label(s: &Scene, k: usize, placed: &[Rect], room: &mut Room, round: usize, grew: &mut bool) -> Rect {
    let (arrows, labels, routes, obstacles, segments, col_x) = (s.arrows, s.labels, s.routes, s.obstacles, s.segments, s.col_x);
    let track_y = |k: usize| s.ys.track_y(k);
    let Room { extra, wrap_at, reserve } = room;
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
    let chosen = match candidates.iter().find(|r| cost(r, placed) == 0.0) {
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
                        *grew = true;
                    } else {
                        // No wrap clears it: move the lanes of the channel to the right
                        // of the target's column clear of the label.
                        let c = a.t.1;
                        let edge = col_x[c] + NODE_W;
                        let need = tx + LABEL_GAP + lw + 4.0 - edge;
                        if need > reserve[c + 1] {
                            reserve[c + 1] = need;
                            *grew = true;
                        }
                    }
                } else {
                    extra[k] += lh.min(2.0 * LABEL_LINE);
                    *grew = true;
                }
            }
            candidates
                .iter()
                .copied()
                .min_by(|p, q| cost(p, placed).total_cmp(&cost(q, placed)))
                .unwrap_or(candidates[0])
        }
    };
    chosen
}

/// The key, below the graph: the styles the graph uses; and the graph's height with it.
fn key(nodes: &[Node], arrows: &[Arrow], graph_bottom: f64, width: f64) -> (Vec<KeyEntry>, f64) {
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
    (key, height)
}
