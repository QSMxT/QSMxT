//! Pop-ups: tips, the advice checklist, announcements, the update prompt, the option picker.

use super::*;

/// The "Did you know?" pop-up: a tip, its link, and the OK / turn-tips-off buttons.
pub(super) fn draw_tip_modal(f: &mut Frame, modal: &TipModal, area: Rect, hits: &crate::tui::mouse::Hits) {
    let popup_w = 68.min(area.width);
    let text_w = popup_w.saturating_sub(6).max(10) as usize;

    let mut lines = vec![Line::from("")];
    lines.extend(tip_lines(modal.tip.text, text_w));
    if let Some(link) = modal.tip.link {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            link,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
        )));
    }
    lines.push(Line::from(""));

    let mut buttons = Vec::new();
    for (i, label) in modal.buttons().into_iter().enumerate() {
        if i > 0 {
            buttons.push(Span::raw("   "));
        }
        let style = if i == modal.cursor {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        buttons.push(Span::styled(format!(" {label} "), style));
    }
    lines.push(Line::from(buttons).alignment(Alignment::Center));
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "\u{2190}\u{2192} choose   Enter select   n next tip   Esc close",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
    );

    let popup = centered_rect(popup_w, lines.len() as u16 + 2, area);
    hits.add(popup, crate::tui::mouse::Hit::Popup);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(" Did you know? ", Style::default().add_modifier(Modifier::BOLD)));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let text_area = Rect { x: inner.x + 2, width: inner.width.saturating_sub(4), ..inner };
    // The buttons sit three lines from the bottom: buttons, a blank, the hint.
    let at = lines.len().saturating_sub(3);
    hits.add_buttons(Rect { y: text_area.y + at as u16, height: 1, ..text_area }, &lines[at..at + 1]);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);
}

/// An advice pop-up ([`crate::tui::recommend::AdviceModal`]): the changes on offer as a checklist
/// under the sections they are on, as on the Pipeline tab ("setting  current → new  (kind)"),
/// then a rule and the highlighted row's reason, then Apply and Keep mine.
pub(super) fn draw_checklist_modal(f: &mut Frame, modal: &mut crate::tui::recommend::AdviceModal, area: Rect, hits: &crate::tui::mouse::Hits) {
    use crate::tui::recommend::{AdviceKind, Strength};
    let popup_w = advice_modal_width(area);
    let text_w = popup_w.saturating_sub(6).max(10) as usize;
    let muted = Style::default().fg(Color::DarkGray);
    modal.cursor = modal.cursor.min(modal.rows.len().saturating_sub(1));
    let label_w = modal.rows.iter().map(|r| r.setting().1.chars().count()).max().unwrap_or(0).min(text_w * 2 / 5) + 2;

    // The list: a heading for each section, then its rows, one line each (two when narrow).
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut row_lines: Vec<(usize, usize)> = Vec::new();
    let mut section = None;
    for (i, row) in modal.rows.iter().enumerate() {
        let (path, label) = row.setting();
        let sec = qsmxt_engine::schema::setting(path).map(|s| s.section);
        if let Some(s) = sec.filter(|_| sec != section) {
            section = sec;
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(format!("{} › {}", s.tab().label(), s.label()), muted.add_modifier(Modifier::BOLD))));
        }
        let focused = i == modal.cursor && modal.button.is_none();
        let (from, to) = row.change();
        let tag_color = match (row.alternative, row.finding.strength) {
            (true, _) => Color::DarkGray,
            (_, Strength::Reference) => Color::Cyan,
            (_, Strength::Suggested) => Color::LightYellow,
            _ => Color::Yellow,
        };
        let label_style = if focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().add_modifier(Modifier::BOLD) };
        let label: String = label.chars().take(label_w - 2).collect();
        let head = vec![
            Span::styled(if focused { "\u{25b8} " } else { "  " }, label_style),
            // One choice of several ways to resolve a setting reads as a radio button.
            Span::styled(match (modal.radio(i), row.checked) {
                (true, true) => "(\u{2022}) ",
                (true, false) => "( ) ",
                (false, true) => "[x] ",
                (false, false) => "[ ] ",
            }, if row.checked { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) }),
            Span::styled(format!("{label:label_w$}"), label_style),
        ];
        let chosen = if row.checked { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) };
        // A row that keeps the value the setting has (another chosen method's authors'): no arrow.
        let change = if row.keeps {
            vec![
                Span::styled(to, chosen),
                Span::styled(" (as set)", muted),
                Span::styled(format!("  ({})", modal.tag(i)), Style::default().fg(tag_color)),
            ]
        } else {
            vec![
                Span::styled(from, Style::default().fg(Color::Gray)),
                Span::styled(" \u{2192} ", muted),
                Span::styled(to, chosen),
                Span::styled(format!("  ({})", modal.tag(i)), Style::default().fg(tag_color)),
            ]
        };
        let first = lines.len();
        let width: usize = head.iter().chain(&change).map(|s| s.width()).sum();
        if width <= text_w {
            lines.push(Line::from([head, change].concat()));
        } else {
            lines.push(Line::from(head));
            let mut indented = vec![Span::raw(" ".repeat(6))];
            indented.extend(change);
            lines.push(Line::from(indented));
        }
        row_lines.push((first, lines.len()));
    }

    // The highlighted row's reason, in a panel of a fixed height: the tallest any row needs.
    let explain = |i: usize| -> Vec<Line<'static>> {
        tip_lines(&modal.explanation(i), text_w).into_iter()
            .map(|l| Line::from(l.spans.into_iter().map(|s| Span::styled(s.content, s.style.patch(Style::default().fg(Color::Gray)))).collect::<Vec<_>>()))
            .collect()
    };
    let panel_h = (0..modal.rows.len()).map(|i| explain(i).len()).max().unwrap_or(1).clamp(2, 6) as u16;
    let panel = if modal.cursor < modal.rows.len() { explain(modal.cursor) } else { vec![] };

    // Apply (dimmed while nothing is ticked) and Keep mine, then the keys.
    // The focused button (↓ past the last row, or Tab) is drawn reversed in cyan.
    let ticked = modal.ticked();
    let focus_style = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
    let apply_style = match (modal.button, ticked) {
        (Some(0), _) => focus_style,
        (_, 0) => Style::default().fg(Color::DarkGray).bg(Color::Black),
        _ => Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
    };
    let labels = modal.button_labels();
    let mut buttons = vec![Span::styled(format!(" {} ", labels[0]), apply_style)];
    if let Some(second) = labels.get(1) {
        buttons.push(Span::raw("   "));
        let keep_style = if modal.button == Some(1) { focus_style } else { Style::default().fg(Color::Gray).bg(Color::Black) };
        buttons.push(Span::styled(format!(" {second} "), keep_style));
    }
    let buttons = Line::from(buttons).alignment(Alignment::Center);
    let required = modal.kind == AdviceKind::Required;
    let esc = if matches!(modal.kind, AdviceKind::Change | AdviceKind::Changed | AdviceKind::Required) { "later" } else { "close" };
    let keep = match (required, modal.keepable()) {
        (true, true) => "   U undo my change",
        (false, true) => "   K keep mine",
        _ => "",
    };
    let short_keep = match (required, modal.keepable()) {
        (true, true) => " · U undo",
        (false, true) => " · K keep",
        _ => "",
    };
    let tick = if required { "" } else { "Space/Enter tick   " };
    let hint = [
        format!("\u{2191}\u{2193} move   {tick}A apply{keep}   Esc {esc}"),
        format!("{tick}A apply{keep}   Esc {esc}"),
        format!("A apply{short_keep} · Esc {esc}"),
    ].into_iter().find(|h| h.chars().count() <= text_w).unwrap_or_else(|| format!("A apply · Esc {esc}"));
    let hint = Line::from(Span::styled(hint, muted)).alignment(Alignment::Center);

    // Height: a blank line, the list, a blank line, the rule, the panel, a blank line, the
    // buttons and the keys, inside the border.
    let chrome = 1 + 1 + 1 + panel_h + 1 + 2 + 2;
    let list_len = lines.len() as u16;
    let popup = centered_rect(popup_w, list_len + chrome, area);
    hits.add(popup, crate::tui::mouse::Hit::Popup);
    let color = if modal.findings.iter().any(|r| r.strength != Strength::Reference) { Color::Yellow } else { Color::Cyan };
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(modal.title(), Style::default().add_modifier(Modifier::BOLD)));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    // On a short terminal the panel gives way first, then the blank lines.
    let panel_h = panel_h.min(inner.height.saturating_sub(4)).max(1);
    let gap = u16::from(inner.height >= list_len.min(3) + 1 + panel_h + 2 + 3);
    let [_, list, _, rule, panel_area, _, foot] = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(gap), Constraint::Min(1), Constraint::Length(gap), Constraint::Length(1),
            Constraint::Length(panel_h), Constraint::Length(gap), Constraint::Length(2)])
        .areas(inner);
    let list = Rect { x: list.x + 2, width: list.width.saturating_sub(4), ..list };

    // The list scrolls to keep the highlighted row in view.
    let max_scroll = (lines.len() as u16).saturating_sub(list.height);
    if let Some(&(start, end)) = row_lines.get(modal.cursor) {
        let (start, end) = (start as u16, end as u16);
        // The section heading above the first row of a section stays with it.
        let start = if modal.cursor == 0 { 0 } else { start };
        if start < modal.scroll {
            modal.scroll = start;
        } else if end > modal.scroll + list.height {
            modal.scroll = end.saturating_sub(list.height);
        }
    }
    modal.scroll = modal.scroll.min(max_scroll);
    f.render_widget(Paragraph::new(lines).scroll((modal.scroll, 0)), list);
    for (i, &(start, end)) in row_lines.iter().enumerate() {
        for l in start..end {
            let Some(y) = (l as u16).checked_sub(modal.scroll).filter(|&y| y < list.height) else { continue };
            hits.add(Rect { x: list.x, y: list.y + y, width: list.width, height: 1 }, crate::tui::mouse::Hit::AdviceRow(i));
        }
    }
    if max_scroll > 0 {
        let bar = Rect { x: popup.x + popup.width - 1, y: list.y, width: 1, height: list.height };
        let mut state = ScrollbarState::new(max_scroll as usize).viewport_content_length(list.height as usize).position(modal.scroll as usize);
        f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None)
            .thumb_style(Style::default().fg(color)), bar, &mut state);
    }
    help_divider(f, rule);
    let panel_area = Rect { x: panel_area.x + 2, width: panel_area.width.saturating_sub(4), ..panel_area };
    f.render_widget(Paragraph::new(panel), panel_area);
    let foot_lines = vec![buttons, hint];
    f.render_widget(Paragraph::new(foot_lines.clone()), foot);
    hits.add_buttons(foot, &foot_lines[..1]);
}

/// How wide the advice pop-ups are: wide enough to keep a long reason short, on any terminal.
fn advice_modal_width(area: Rect) -> u16 {
    const MAX_WIDTH: u16 = 110;
    MAX_WIDTH.min(area.width.saturating_sub(4)).max(area.width.min(40))
}

/// An advice pop-up: its text scrolls, with a scrollbar when it does not all fit, above a footer
/// (the choices and `hint`) that always stays in view.
#[allow(clippy::too_many_arguments)]
fn draw_advice_modal(
    f: &mut Frame, area: Rect, popup_w: u16, title: &str, color: Color,
    lines: Vec<Line<'static>>, mut footer: Vec<Line<'static>>, hint: &str, scroll_hint: &str, scroll: &mut u16,
    hits: &crate::tui::mouse::Hits,
) {
    let buttons = footer.clone();
    let chrome = footer.len() as u16 + 1 + 2;
    let list_len = lines.len() as u16;
    let popup = centered_rect(popup_w, list_len + chrome, area);
    hits.add(popup, crate::tui::mouse::Hit::Popup);
    let body_h = popup.height.saturating_sub(chrome);
    let max_scroll = (lines.len() as u16).saturating_sub(body_h);
    *scroll = (*scroll).min(max_scroll);
    let hint = if max_scroll > 0 { format!("{scroll_hint}   {hint}") } else { hint.to_string() };
    footer.push(Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))).alignment(Alignment::Center));

    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(title.to_string(), Style::default().add_modifier(Modifier::BOLD)));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let text_area = Rect { x: inner.x + 2, width: inner.width.saturating_sub(4), ..inner };
    let [body, foot] = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer.len() as u16)]).areas(text_area);
    f.render_widget(Paragraph::new(lines).scroll((*scroll, 0)), body);
    f.render_widget(Paragraph::new(footer), foot);
    hits.add_buttons(foot, &buttons);
    if max_scroll > 0 {
        // On the pop-up's right border, beside the text.
        let bar = Rect { x: popup.x + popup.width - 1, y: body.y, width: 1, height: body.height };
        let mut state = ScrollbarState::new(max_scroll as usize)
            .viewport_content_length(body.height as usize)
            .position(*scroll as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(color));
        f.render_stateful_widget(scrollbar, bar, &mut state);
    }
}

/// An announcement pop-up, or the history of them all ([`NewsModal`]): each announcement's date,
/// title, wrapped body, the versions it affects and its link.
pub(super) fn draw_news_modal(f: &mut Frame, modal: &mut NewsModal, area: Rect, hits: &crate::tui::mouse::Hits) {
    let popup_w = advice_modal_width(area).min(84);
    let text_w = popup_w.saturating_sub(6).max(10) as usize;
    let dim = Style::default().fg(Color::DarkGray);
    let restyle = |lines: Vec<Line<'static>>, style: Style| -> Vec<Line<'static>> {
        lines.into_iter()
            .map(|l| Line::from(l.spans.into_iter().map(|s| Span::styled(s.content, s.style.patch(style))).collect::<Vec<_>>()))
            .collect()
    };
    let mut lines = vec![Line::from("")];
    if let Some(note) = &modal.note {
        lines.extend(restyle(tip_lines(note, text_w), dim));
        lines.push(Line::from(""));
    }
    if modal.items.is_empty() {
        lines.push(Line::from("There are no announcements."));
        lines.push(Line::from(""));
    }
    for (i, a) in modal.items.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(Span::styled("\u{2500}".repeat(text_w), dim)));
            lines.push(Line::from(""));
        }
        let mut head = vec![Span::styled(a.date.clone(), dim)];
        if a.critical {
            head.push(Span::styled("  Important", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
        }
        lines.push(Line::from(head));
        lines.extend(restyle(tip_lines(&a.title, text_w), Style::default().add_modifier(Modifier::BOLD)));
        for p in a.paragraphs() {
            lines.push(Line::from(""));
            lines.extend(tip_lines(&p, text_w));
        }
        let mut notes = a.affects_text().unwrap_or_default();
        if a.critical && !modal.history {
            let until = match a.affects.as_ref().and_then(|r| r.fixed_in.as_deref()) {
                Some(fixed) => format!(" This shows each time QSMxT starts until you update to {fixed} or later (`qsmxt update`)."),
                None => " This shows each time QSMxT starts.".to_string(),
            };
            notes.push_str(&until);
        }
        if !notes.is_empty() {
            lines.push(Line::from(""));
            lines.extend(restyle(tip_lines(notes.trim(), text_w), dim));
        }
        if let Some(link) = &a.link {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(link.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED))));
        }
        lines.push(Line::from(""));
    }
    let critical = !modal.history && modal.items.iter().any(|a| a.critical);
    let (title, color) = match (modal.history, critical) {
        (true, _) => (" Announcements ", Color::Cyan),
        (false, true) => (" Important announcement ", Color::Red),
        (false, false) => (" Announcement ", Color::Cyan),
    };
    let has_link = !modal.history && modal.items.iter().any(|a| a.link.is_some());
    let hint = match (modal.history, has_link) {
        (true, _) => "Esc close",
        (false, true) => "c copy link   Enter close",
        (false, false) => "Enter close",
    };
    draw_advice_modal(f, area, popup_w, title, color, lines, Vec::new(), hint, "\u{2191}\u{2193} scroll", &mut modal.scroll, hits);
}

/// "A new version is available" pop-up, raised by the launch-time release check.
pub(super) fn draw_update_modal(f: &mut Frame, modal: &UpdateModal, area: Rect, hits: &crate::tui::mouse::Hits) {
    let running = crate::commands::update::current_version();
    let lines = vec![
        Line::from(vec![
            Span::raw("  You are running "),
            Span::styled(format!("v{running}"), Style::default().fg(Color::Yellow)),
            Span::raw("  →  "),
            Span::styled(
                modal.tag.clone(),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", modal.html_url),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" update now   "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" later"),
        ]),
    ];

    // Wide enough for the release URL, which is the longest line.
    let content_w = lines
        .iter()
        .map(|l| l.width())
        .max()
        .unwrap_or(40) as u16;
    let popup = centered_rect(content_w + 4, lines.len() as u16 + 2, area);
    hits.add(popup, crate::tui::mouse::Hit::Popup);

    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Update available ");
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// Pop-up list to choose one algorithm option, showing every option at once (scrolls if needed)
/// plus the highlighted option's help.
pub(super) fn draw_algo_modal(f: &mut Frame, modal: &AlgoModal, area: Rect, hits: &crate::tui::mouse::Hits) {
    // Cap the visible list rather than growing to fit every option — a long list (e.g. the 29 QSM
    // algorithms) then scrolls (with a scrollbar) instead of overflowing the screen and pushing
    // the tooltip/hotkey footer off the bottom. Reserve room for a 3-line wrapped tooltip + a
    // 1-line footer, and go wide so those lines fit.
    const MAX_LIST_ROWS: u16 = 14;
    const HELP_ROWS: u16 = 3;
    const FOOTER_ROWS: u16 = 1;
    let chrome = 2 + HELP_ROWS + FOOTER_ROWS; // borders + tooltip + footer

    let content_w = modal.options.iter().map(|o| o.len()).max().unwrap_or(0)
        .max(modal.title.len()) as u16 + 8;
    // Wide enough for the footer legend and long tooltips (which still wrap); clamp to screen.
    let width = content_w.max(72).clamp(72, 100).min(area.width.saturating_sub(4)).max(30);
    let list_h = (modal.options.len() as u16)
        .min(MAX_LIST_ROWS)
        .min(area.height.saturating_sub(chrome + 2)) // keep the whole modal on-screen
        .max(1);
    let height = (list_h + chrome).min(area.height.saturating_sub(2));
    let popup = centered_rect(width, height, area);
    hits.add(popup, crate::tui::mouse::Hit::Popup);

    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} ", modal.title));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(HELP_ROWS),
            Constraint::Length(FOOTER_ROWS),
        ])
        .split(inner);

    // Colour deep-learning options distinctly so they read as a group (they are listed
    // contiguously at the end of each algorithm modal).
    let dl_color = Color::LightMagenta;
    let has_dl = modal.options.iter().any(|o| is_deep_learning_algo(o));
    let items: Vec<ListItem> = modal.options.iter().map(|o| {
        let item = ListItem::new(o.clone());
        if is_deep_learning_algo(o) { item.style(Style::default().fg(dl_color)) } else { item }
    }).collect();

    // Reserve a 1-column scrollbar gutter when the list overflows its viewport.
    let needs_scroll = modal.options.len() > parts[0].height as usize;
    let (list_area, sb_area) = if needs_scroll {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(parts[0]);
        (cols[0], Some(cols[1]))
    } else {
        (parts[0], None)
    };

    let list = List::new(items)
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    // From where the list was last drawn: ratatui moves the window only as far as keeps the
    // cursor in it, so the view scrolls only when the cursor would leave it.
    let mut state = ListState::default().with_offset(modal.offset.get());
    state.select(Some(modal.cursor));
    f.render_stateful_widget(list, list_area, &mut state);
    modal.offset.set(state.offset());
    hits.add(list_area, crate::tui::mouse::Hit::Picker { offset: state.offset() });

    if let Some(sb_area) = sb_area {
        let mut sb_state = ScrollbarState::new(modal.options.len())
            .viewport_content_length(list_area.height as usize)
            .position(modal.cursor);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(Color::Cyan));
        f.render_stateful_widget(sb, sb_area, &mut sb_state);
    }

    let help = modal.help.get(modal.cursor).cloned().unwrap_or_default();
    f.render_widget(
        Paragraph::new(help)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        parts[1],
    );
    let mut footer = vec![
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::styled(" select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::styled(" choose  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ];
    if has_dl {
        footer.push(Span::styled("   ◆", Style::default().fg(dl_color)));
        footer.push(Span::styled(" deep learning", Style::default().fg(Color::DarkGray)));
    }
    f.render_widget(Paragraph::new(Line::from(footer)), parts[2]);
}

/// Deep-learning algorithm ids across the inversion / background-removal / separation modals.
/// Used to colour them as a group in the selector.
fn is_deep_learning_algo(name: &str) -> bool {
    matches!(
        name,
        "xqsm" | "qsmnet" | "qsmnet-plus" | "qsmnetme" | "autoqsm" | "qsmgan" | "ir2qsm" | "lpcnn"
            | "modl-qsm" | "nextqsm" | "iqsm" | "iqsm-plus" | "bfrnet" | "iqfm"
            | "susep-net" | "chi-sepnet" | "hd-bet" | "rs2-net" | "mindgrab"
    )
}
