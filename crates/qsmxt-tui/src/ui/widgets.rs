//! Pieces the screens share: help text, picker buttons, dividers, scrolling, centring.

use super::*;

/// A help text, word-wrapped to `width` columns and indented like the rows above it.
pub(super) fn help_lines(text: &str, width: u16, color: Color) -> Vec<Line<'static>> {
    tip_lines(text, width.saturating_sub(3).max(10) as usize).into_iter().map(|l| {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(l.spans.into_iter().map(|s| Span::styled(s.content, s.style.fg(color))));
        Line::from(spans)
    }).collect()
}

/// Word-wrap `text` to `width` columns, drawing `backticked` parts as commands.
pub(super) fn tip_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let code = Style::default().fg(Color::Yellow);
    // Each word as its pieces, with the backticks dropped: "`--config`." is a command then a dot.
    let mut in_code = false;
    let words: Vec<Vec<Span<'static>>> = text
        .split_whitespace()
        .map(|word| {
            let mut pieces = Vec::new();
            for (i, piece) in word.split('`').enumerate() {
                if i > 0 {
                    in_code = !in_code;
                }
                if !piece.is_empty() {
                    let style = if in_code { code } else { Style::default() };
                    pieces.push(Span::styled(piece.to_string(), style));
                }
            }
            pieces
        })
        .collect();
    let mut lines = Vec::new();
    let mut line: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for word in words {
        let w: usize = word.iter().map(|p| p.content.chars().count()).sum();
        if used > 0 && used + 1 + w > width {
            lines.push(Line::from(std::mem::take(&mut line)));
            used = 0;
        }
        if used > 0 {
            line.push(Span::raw(" "));
            used += 1;
        }
        line.extend(word);
        used += w;
    }
    if !line.is_empty() {
        lines.push(Line::from(line));
    }
    lines
}

pub(super) fn picker_button(value: &str, focused: bool) -> Vec<Span<'static>> {
    let (frame, text) = if focused {
        (Style::default().fg(Color::DarkGray), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        (Style::default().fg(Color::DarkGray), Style::default().fg(Color::Gray))
    };
    vec![Span::styled("[ ", frame), Span::styled(value.to_string(), text), Span::styled(" \u{25be} ]", frame)]
}

/// A horizontally+vertically centered rectangle of at most `w`×`h`, clamped to `area`.
pub(super) fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

/// Render a scrollable paragraph with a scrollbar when content exceeds visible height.
/// Updates `scroll_offset` in place so it persists between frames.
/// The rule between a panel's settings and the help under them: a line across `row`, joined to
/// the vertical rules on either side (the panel's border, or the sidebar's), in their colour. The
/// settings' scrollbar ends above it.
pub(super) fn help_divider(f: &mut Frame, row: Rect) {
    if row.height == 0 || row.width == 0 {
        return;
    }
    let buf = f.buffer_mut();
    let y = row.y;
    // Beside the sidebar, the form starts a blank column after the sidebar's rule: span it.
    let row = match row.x.checked_sub(2) {
        Some(x) if buf[(x + 1, y)].symbol() == " " && buf[(x, y)].symbol() == "│" => Rect { x: x + 1, width: row.width + 1, ..row },
        _ => row,
    };
    let left = row.x.checked_sub(1).filter(|&x| buf.area.contains((x, y).into()));
    let right = Some(row.right()).filter(|&x| buf.area.contains((x, y).into()));
    let faint = left.or(right).map_or(Color::DarkGray, |x| buf[(x, y)].fg);
    for x in row.x..row.right() {
        buf[(x, y)].set_symbol("─").set_fg(faint);
    }
    if let Some(x) = left.filter(|&x| buf[(x, y)].symbol() == "│") {
        buf[(x, y)].set_symbol("├");
    }
    if let Some(x) = right.filter(|&x| buf[(x, y)].symbol() == "│") {
        buf[(x, y)].set_symbol("┤");
    }
}

pub(super) fn render_scrollable(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_offset: &mut usize,
    focused_line: Option<usize>,
) {
    let visible_height = area.height as usize;
    let total_lines = lines.len();

    if total_lines <= visible_height {
        *scroll_offset = 0;
        let para = Paragraph::new(lines);
        f.render_widget(para, area);
        return;
    }

    // Only scroll when focused line goes beyond the visible edges.
    // Otherwise the viewport stays put — this means the cursor can
    // freely move within the visible area without the view jumping.
    if let Some(fl) = focused_line {
        if fl < *scroll_offset {
            *scroll_offset = fl;
        } else if fl >= *scroll_offset + visible_height {
            *scroll_offset = fl - visible_height + 1;
        }
    }
    *scroll_offset = (*scroll_offset).min(total_lines.saturating_sub(visible_height));

    let scroll = *scroll_offset;
    let para = Paragraph::new(lines).scroll((scroll as u16, 0));
    f.render_widget(para, area);

    // Render scrollbar
    let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(visible_height))
        .position(scroll);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some("↑"))
        .end_symbol(Some("↓"));
    f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}

/// Render the NIfTI configuration section (used within the unified input tab).
/// Draw the example-dataset picker under the IO fields in `lines`: the Download button with its
/// status and progress log stay in view, and the acquisitions scroll in a region of their own.
/// A terminal too short for that scrolls it all as one. Returns how far the IO fields scrolled.
/// Record a click target for each of `targets` (by line) that is in view in `area`, scrolled by
/// `scroll` lines.
pub(super) fn add_line_hits(hits: &crate::mouse::Hits, area: Rect, scroll: usize, targets: impl IntoIterator<Item = (usize, crate::mouse::InputTarget)>) {
    for (line, target) in targets {
        if line >= scroll && line < scroll + area.height as usize {
            hits.add(Rect { y: area.y + (line - scroll) as u16, height: 1, ..area }, crate::mouse::Hit::Input(target));
        }
    }
}
