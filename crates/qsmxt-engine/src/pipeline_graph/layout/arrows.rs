//! The arrows: how each runs, and the ports it leaves and enters its boxes by ([`super::layout`]).

use std::collections::HashMap;

use super::{item_labels, Arrow, Drawn, Grid, Side, TITLE_CLEAR};
use crate::pipeline_graph::PipelineGraph;

/// The arrows: which run long, by which channel, and their labels.
pub(super) fn arrows(graph: &PipelineGraph, drawn: Vec<Drawn>, index: &HashMap<&str, usize>, grid: &Grid) -> Vec<Arrow> {
    let (row, col) = (&grid.row, &grid.col);
    let rc = |id: &str| {
        let i = index[id];
        (row[i], col[i], i)
    };
    drawn
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
        .collect()
}

/// Ports. Out of a box's bottom, each arrow to the next row, ordered by where it heads; out of
/// each side, each long arrow, the one going furthest highest (so the lanes nest). Into a
/// box's top, each arrow, ordered by where it comes from.
pub(super) fn ports(arrows: &mut [Arrow], grid: &Grid) {
    let (n_nodes, first_of_group) = (grid.row.len(), &grid.first_of_group);
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
}

/// The arrows into each row, left to right: their tracks run top to bottom in that order.
pub(super) fn entering(arrows: &[Arrow], n_rows: usize) -> Vec<Vec<usize>> {
    (0..n_rows)
        .map(|r| {
            let mut ks: Vec<usize> = (0..arrows.len()).filter(|&k| arrows[k].t.0 == r).collect();
            ks.sort_by(|&a, &b| {
                (arrows[a].t.1 as f64 + arrows[a].inn)
                    .total_cmp(&(arrows[b].t.1 as f64 + arrows[b].inn))
            });
            ks
        })
        .collect()
}
