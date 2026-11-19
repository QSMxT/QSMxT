//! The example dataset picker, its download button and log.

use super::*;

pub(super) fn draw_example_picker(
    f: &mut Frame,
    state: &mut crate::tui::app::ExampleState,
    hits: &crate::tui::mouse::Hits,
    area: Rect,
    mut lines: Vec<Line<'_>>,
    in_io: bool,
    focused_io: Option<usize>,
) -> usize {
    /// The list's borders and one row.
    const MIN_LIST: usize = 3;
    let button_line = lines.len();
    draw_example_button(state, in_io, &mut lines);
    // Once the download starts, the list gives way to its log, in a small box of its own; the
    // last lines show, as they may be the error.
    if state.status != crate::tui::app::ConvertStatus::Idle {
        let head_h = (lines.len() as u16 + 1).min(area.height);
        let room = (area.height.saturating_sub(head_h + 2) as usize).clamp(1, 8);
        let log = example_log_lines(state, room);
        let [head, rest] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(head_h), Constraint::Min(0)])
            .areas(area);
        f.render_widget(Paragraph::new(lines), head);
        add_line_hits(hits, head, 0, example_targets(button_line, 0).take(crate::tui::app::App::INPUT_IO_FIELDS + 1));
        // Inset from the panel's border, as the acquisitions' box is (no doubled border).
        let boxed = Rect { x: rest.x + 1, width: rest.width.saturating_sub(2), height: (log.len().max(1) as u16 + 2).min(rest.height), ..rest };
        // Styled as the panels are: rounded, a faint border, the title in the text colour.
        let block = Block::default().borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Download log ").border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(boxed);
        f.render_widget(block, boxed);
        f.render_widget(Paragraph::new(log), inner);
        return 0;
    }
    let rows = example_rows(state, in_io);
    let focused_row = if in_io { None } else { state.acquisition() };

    const TITLE: &str = " Example acquisitions (one in-vivo subject, Siemens 3T) ";
    if area.height as usize >= lines.len() + MIN_LIST {
        let [head, list] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(lines.len() as u16), Constraint::Min(0)])
            .areas(area);
        f.render_widget(Paragraph::new(lines), head);
        let border = if focused_row.is_some() { Color::Yellow } else { Color::DarkGray };
        // Inset from the panel's border, so the two borders do not sit side by side.
        let list = Rect { x: list.x + 1, width: list.width.saturating_sub(2), ..list };
        let block = Block::default().borders(Borders::ALL).title(TITLE).border_style(Style::default().fg(border));
        let inner = block.inner(list);
        f.render_widget(block, list);
        render_scrollable(f, inner, rows, &mut state.scroll_offset, focused_row);
        add_line_hits(hits, head, 0, example_targets(button_line, 0).take(crate::tui::app::App::INPUT_IO_FIELDS + 1));
        add_line_hits(hits, inner, state.scroll_offset, example_targets(0, 0).skip(crate::tui::app::App::INPUT_IO_FIELDS + 1));
        0
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(format!(" {TITLE}"), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD))));
        let first_row = lines.len();
        let focused = focused_io.or(Some(focused_row.map_or(button_line, |r| first_row + r)));
        lines.extend(rows);
        render_scrollable(f, area, lines, &mut state.scroll_offset, focused);
        add_line_hits(hits, area, state.scroll_offset, example_targets(button_line, first_row));
        state.scroll_offset
    }
}

/// The example page's click targets with their lines: the IO fields, the Download button on
/// `button_line`, then the acquisitions from `first_row`.
fn example_targets(button_line: usize, first_row: usize) -> impl Iterator<Item = (usize, crate::tui::mouse::InputTarget)> {
    use crate::tui::{app::ExampleState, mouse::InputTarget};
    let io = (0..crate::tui::app::App::INPUT_IO_FIELDS).map(|i| (i, InputTarget::Io(i)));
    let rows = (0..crate::example::EXAMPLES.len()).map(move |r| (first_row + r, InputTarget::Example(r + 1)));
    io.chain(std::iter::once((button_line, InputTarget::Example(ExampleState::BUTTON)))).chain(rows)
}

/// One row for each acquisition, ticked or not.
fn example_rows(state: &crate::tui::app::ExampleState, in_io: bool) -> Vec<Line<'static>> {
    crate::example::EXAMPLES.iter().enumerate().map(|(i, example)| {
        let ticked = state.selected.get(i).copied().unwrap_or(false);
        let focused = !in_io && state.acquisition() == Some(i);
        let name_style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else if ticked {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };
        Line::from(vec![
            Span::styled(
                format!(" [{}] ", if ticked { "x" } else { " " }),
                if ticked { Style::default().fg(Color::Green) } else { Style::default().fg(Color::DarkGray) },
            ),
            Span::styled(format!("{:<28}", example.id), name_style),
            Span::styled(example.describe(), Style::default().fg(Color::DarkGray)),
        ])
    }).collect()
}

/// The Download button, carrying the current status.
fn draw_example_button(
    state: &crate::tui::app::ExampleState,
    in_io: bool,
    lines: &mut Vec<Line<'_>>,
) {
    use crate::tui::app::ConvertStatus;

    let focused = !in_io && state.cursor == crate::tui::app::ExampleState::BUTTON;
    let (label, style) = match state.status {
        ConvertStatus::Converting => (
            match state.progress() {
                Some((done, total)) => format!(
                    "  Downloading... {:>3}%  ({:.0}/{:.0} MB)",
                    done * 100 / total.max(1),
                    done as f64 / 1e6,
                    total as f64 / 1e6
                ),
                None => "  Writing dataset...".to_string(),
            },
            Style::default().fg(Color::Yellow),
        ),
        ConvertStatus::Done => (
            "  [ Download ]  (dataset ready)".to_string(),
            Style::default().fg(Color::Green),
        ),
        ConvertStatus::Error => (
            "  [ Download ]  (failed - see below)".to_string(),
            Style::default().fg(Color::Red),
        ),
        ConvertStatus::Idle => {
            let n = state.selected_examples().len();
            (
                format!(
                    "  [ Download ]  {n} selected, {:.0} MB",
                    state.selected_bytes() as f64 / 1e6
                ),
                Style::default().fg(Color::White),
            )
        }
    };
    let style = if focused && state.status != ConvertStatus::Converting {
        style.add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        style
    };
    lines.push(Line::from(Span::styled(label, style)));

}

/// The download log's last `room` lines, errors in red.
fn example_log_lines(state: &crate::tui::app::ExampleState, room: usize) -> Vec<Line<'static>> {
    state.log.iter().rev().take(room).rev().map(|line| {
        let style = if line.starts_with("Error:") { Style::default().fg(Color::Red) } else { Style::default().fg(Color::DarkGray) };
        Line::from(Span::styled(format!(" {line}"), style))
    }).collect()
}
