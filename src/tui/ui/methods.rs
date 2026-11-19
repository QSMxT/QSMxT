//! The Methods tab: the methods text and the command preview.

use super::*;

/// The Methods tab: its sub-tab strip, then the methods text or the pipeline graph.
pub(super) fn draw_methods_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let block = crate::tui::tabs::panel(app);
    let mut inner = block.inner(area);
    f.render_widget(block, area);
    if app.sidebar() {
        inner = draw_sidebar_beside(f, app, area, inner);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(crate::tui::tabs::subtab_rows(app)), Constraint::Min(1)])
        .split(inner);
    crate::tui::tabs::draw_subtabs(f, app, chunks[0]);
    // The page's button on its first line (a blank under it where there is room), then the page.
    let page = chunks[1];
    let gap = u16::from(page.height > 6);
    let button = Rect { height: 1.min(page.height), ..page };
    let body = Rect { y: page.y + 1 + gap, height: page.height.saturating_sub(1 + gap), ..page };
    app.hits.add(page, crate::tui::mouse::Hit::Page);
    let line = methods_button(app);
    let width = line.spans.iter().take(2).map(|s| s.width() as u16).sum::<u16>();
    let first = line.spans.first().map_or(0, |s| s.width() as u16);
    f.render_widget(Paragraph::new(line), button);
    app.hits.add(Rect { x: button.x + first, width: width.saturating_sub(first), ..button }, crate::tui::mouse::Hit::PageButton);
    if app.page() == Some(crate::tui::app::Page::Pipeline) {
        crate::tui::pipeline_view::draw(f, app, body);
    } else {
        draw_methods_text(f, app, body);
    }
}

/// The Methods tab's button, styled as the Execution tab's Run button: Copy methods text above
/// the text, Open graph above the graph. It looks pressed-ready (filled) while focus is in the
/// page, where Enter presses it; its own key works from the sidebar too.
fn methods_button(app: &App) -> Line<'static> {
    let graph = app.page() == Some(crate::tui::app::Page::Pipeline);
    let (label, key) = if graph { (" Open graph ", "s") } else { (" Copy methods text ", "c") };
    let focused = !app.tab_focus && !app.on_subtabs();
    let (style, hint) = if focused {
        (Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD), format!("  Enter or {key}"))
    } else {
        (Style::default().fg(Color::Green).add_modifier(Modifier::BOLD | Modifier::REVERSED), format!("  {key}"))
    };
    let mut spans = vec![Span::raw(" "), Span::styled(label, style)];
    if !graph && app.methods_copied() {
        spans.push(Span::styled("  ✓ Copied", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
    } else {
        spans.push(Span::styled(hint, Style::default().fg(Color::DarkGray)));
    }
    Line::from(spans)
}

fn draw_methods_text(f: &mut Frame, app: &mut App, inner: ratatui::layout::Rect) {
    let methods_text = command::methods_text(app).unwrap_or_else(|e| {
        format!("# Invalid setting\n\n{e}\n\nThe run would refuse this configuration.")
    });

    let mut lines: Vec<Line> = Vec::new();
    for raw_line in methods_text.lines() {
        if raw_line.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
        } else if raw_line.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(content) = raw_line.strip_prefix("- ") {
            let prefix = "  - ";
            let width = inner.width.saturating_sub(1) as usize; // a column for the scrollbar
            let content_width = width.saturating_sub(prefix.len());
            if content_width > 0 && content.len() > content_width {
                let words: Vec<&str> = content.split_whitespace().collect();
                let mut current_line = String::new();
                let mut first = true;
                for word in words {
                    if current_line.is_empty() {
                        current_line = word.to_string();
                    } else if current_line.len() + 1 + word.len() <= content_width {
                        current_line.push(' ');
                        current_line.push_str(word);
                    } else {
                        if first {
                            lines.push(Line::from(vec![
                                Span::styled(prefix, Style::default().fg(Color::Green)),
                                Span::raw(current_line),
                            ]));
                            first = false;
                        } else {
                            lines.push(Line::from(format!("{}{}", " ".repeat(prefix.len()), current_line)));
                        }
                        current_line = word.to_string();
                    }
                }
                if !current_line.is_empty() {
                    if first {
                        lines.push(Line::from(vec![
                            Span::styled(prefix, Style::default().fg(Color::Green)),
                            Span::raw(current_line),
                        ]));
                    } else {
                        lines.push(Line::from(format!("{}{}", " ".repeat(prefix.len()), current_line)));
                    }
                }
            } else {
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Green)),
                    Span::raw(content.to_string()),
                ]));
            }
        } else if raw_line.is_empty() {
            lines.push(Line::from(""));
        } else {
            // Wrap long paragraph text to inner width
            let width = inner.width.saturating_sub(1) as usize; // a column for the scrollbar
            if width > 0 && raw_line.len() > width {
                let words: Vec<&str> = raw_line.split_whitespace().collect();
                let mut current_line = String::new();
                for word in words {
                    if current_line.is_empty() {
                        current_line = word.to_string();
                    } else if current_line.len() + 1 + word.len() <= width {
                        current_line.push(' ');
                        current_line.push_str(word);
                    } else {
                        lines.push(Line::from(current_line));
                        current_line = word.to_string();
                    }
                }
                if !current_line.is_empty() {
                    lines.push(Line::from(current_line));
                }
            } else {
                lines.push(Line::from(raw_line.to_string()));
            }
        }
    }

    render_scrollable(f, inner, lines, &mut app.methods_scroll_offset, None);
}

pub(super) fn draw_command_preview_with(f: &mut Frame, cmd: &str, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Command preview ");
    let para = Paragraph::new(Line::from(Span::styled(
        cmd.to_string(),
        Style::default().fg(Color::Green),
    )))
    .block(block)
    .wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(para, area);
}
