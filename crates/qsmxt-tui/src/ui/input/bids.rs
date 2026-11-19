//! The BIDS section: the filters and the tree of runs.

use super::*;

/// The BIDS section's filters and tree of runs. Its lines borrow only the filter state `fs`.
pub(super) fn draw_bids_section<'a>(app: &App, fs: &'a crate::app::FilterTreeState, in_io: bool, lines: &mut Vec<Line<'a>>) {
    lines.push(Line::from(Span::styled(
        "  -- Filters --",
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));

    if app.form.bids_dir.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "  Set BIDS directory above first",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let has_runs = app.filter_state.tree.as_ref().is_some_and(|t| !t.subjects.is_empty());
        if !has_runs {
            lines.push(Line::from(Span::styled(
                "  No QSM-compatible runs found",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            let tree = app.filter_state.tree.as_ref().unwrap();

            let include_focused = !in_io && app.filter_state.focus == crate::app::FilterFocus::Include;
            let include_label = Span::styled(
                "  Include:  ",
                if include_focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                else { Style::default().fg(Color::White) },
            );
            let include_val = Span::styled(&fs.include_pattern, Style::default().fg(Color::Cyan));
            lines.push(Line::from(vec![include_label, include_val]));

            let exclude_focused = !in_io && app.filter_state.focus == crate::app::FilterFocus::Exclude;
            let exclude_label = Span::styled(
                "  Exclude:  ",
                if exclude_focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                else { Style::default().fg(Color::White) },
            );
            let exclude_val = if app.filter_state.exclude_pattern.is_empty() && !app.filter_state.exclude_editing {
                Span::styled("(empty)", Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(&fs.exclude_pattern, Style::default().fg(Color::Cyan))
            };
            lines.push(Line::from(vec![exclude_label, exclude_val]));

            lines.extend(orientation_block(app, in_io));
            lines.push(Line::from(""));

            tree_lines(app, tree, in_io, lines);

            lines.push(Line::from(""));
            let ne_focused = !in_io && app.filter_state.focus == crate::app::FilterFocus::NumEchoes;
            let ne_label = Span::styled(
                "  Num echoes: ",
                if ne_focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                else { Style::default().fg(Color::White) },
            );
            let ne_val = if app.filter_state.num_echoes.is_empty() && !app.filter_state.num_echoes_editing {
                Span::styled("(all)", Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(&fs.num_echoes, Style::default().fg(Color::Cyan))
            };
            lines.push(Line::from(vec![ne_label, ne_val]));

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {} run(s), {} selected", tree.total_runs(), tree.selected_runs()),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
}

/// The tree's lines: a subject, a session or a run, with its orientation group.
fn tree_lines(app: &App, tree: &qsmxt_io::bids::discovery::BidsTree, in_io: bool, lines: &mut Vec<Line<'_>>) {
    let visible = app.filter_state.visible_rows();
    let reference = app.field_text("orientation_reference");
    let preview = app.filter_state.orientation_preview(&reference);
    let groups = app.filter_state.orientation_groups(&reference);
    let need = orientations_needed(app.field_select("orientation_algorithm"));
    for (i, row) in visible.iter().enumerate() {
        let focused = !in_io && app.filter_state.focus == crate::app::FilterFocus::TreeNode(i);
        let line = match row {
            crate::app::TreeRow::Subject(si) => {
                let sub = &tree.subjects[*si];
                let collapsed = app.filter_state.collapsed.contains(&format!("sub-{}", sub.name));
                let arrow = if collapsed { "▶" } else { "▼" };
                let sel = sub.selected_runs();
                let total = sub.total_runs();
                let sel_info = if sel == total { "all selected".to_string() } else { format!("{}/{} selected", sel, total) };
                let style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                };
                Line::from(Span::styled(
                    format!("  {} sub-{} ({} run{}, {})", arrow, sub.name, total, if total == 1 { "" } else { "s" }, sel_info),
                    style,
                ))
            }
            crate::app::TreeRow::Session(si, sei) => {
                let sub = &tree.subjects[*si];
                let ses = &sub.sessions[*sei];
                let collapsed = app.filter_state.collapsed.contains(&format!("sub-{}/ses-{}", sub.name, ses.name));
                let arrow = if collapsed { "▶" } else { "▼" };
                let style = if focused {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!("    {} ses-{}", arrow, ses.name), style))
            }
            crate::app::TreeRow::Run { sub, ses, run } => {
                let leaf = match ses {
                    Some(sei) => &tree.subjects[*sub].sessions[*sei].runs[*run],
                    None => &tree.subjects[*sub].runs[*run],
                };
                let indent = if ses.is_some() { "      " } else { "    " };
                let (marker, color) = if leaf.selected { ("[x]", Color::Green) } else { ("[ ]", Color::Gray) };
                let style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(color)
                };
                let mut spans = vec![Span::styled(format!("{}{} {}", indent, marker, leaf.display), style)];
                // Its orientation group, marked where the group is too small to run.
                // Its orientation group: ◀ ▶ around it on the focused run, which ←/→
                // changes; marked where the group is too small to run.
                if let Some(tag) = groups.iter().enumerate().find_map(|(g, members)| {
                    let at = members.iter().position(|k| *k == leaf.key_string)?;
                    let size = members.iter().filter(|k| preview.order.contains(k)).count();
                    let mut tag = format!("group {}", g + 1);
                    if at == 0 && size > 1 {
                        tag.push_str("; reference");
                    }
                    if size < 2 {
                        tag.push_str("; needs another run");
                    } else if size < need {
                        tag.push_str(&format!("; {size} of the {need} needed"));
                    }
                    Some((tag, size < need))
                }).or_else(|| focused.then(|| ("no group".to_string(), false))) {
                    let (tag, short) = tag;
                    let color = if short { Color::Red } else if focused { Color::Yellow } else { Color::Cyan };
                    let text = if focused { format!("  ◀ {tag} ▶") } else { format!("  ({tag})") };
                    spans.push(Span::styled(text, Style::default().fg(color)));
                }
                Line::from(spans)
            }
        };
        lines.push(line);
    }
}
