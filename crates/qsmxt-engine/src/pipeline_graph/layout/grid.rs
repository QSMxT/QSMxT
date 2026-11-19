//! Rows and columns: where each box goes ([`super::layout`]).

use std::collections::{HashMap, HashSet};

use super::{node_lines, side_step, Drawn, Grid, TextLine, PAD};
use crate::pipeline_graph::{Branch, PipelineGraph};

/// The rows and columns of `graph`'s boxes, in run order, its `drawn` arrows read from above; `index`
/// gives each box's place in the graph by its id.
pub(super) fn grid(graph: &PipelineGraph, index: &HashMap<&str, usize>, drawn: &[Drawn]) -> Grid {
    let nodes = &graph.nodes;
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
    Grid { two, reserved, row, col, n_rows, n_cols, lines, heights, row_h, first_of_group, group_rows }
}
