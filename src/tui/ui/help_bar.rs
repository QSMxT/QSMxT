//! The footer: the keys that apply to what has focus.

use super::*;

pub(super) fn draw_help_bar(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    // Show error message if present
    if let Some(ref err) = app.error_message {
        let error_line = Line::from(vec![
            Span::styled(" Error: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(err.as_str(), Style::default().fg(Color::Red)),
        ]);
        f.render_widget(Paragraph::new(error_line), area);
        return;
    }
    if let Some(ref msg) = app.status_message {
        let line = Line::from(Span::styled(format!(" ✓ {msg}"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let tabs = format!("1-{}", TAB_NAMES.len());
    let keys: Vec<(&str, &str)> = if app.editing || app.pipeline_state.editing || app.pipeline_state.mask_value_editing {
        vec![("Esc", "Cancel"), ("Enter", "Confirm")]
    } else if app.tab_focus {
        vec![("\u{2190}\u{2192}", "Tabs"), ("\u{2193}/Enter", "Open tab"), ("F5", "Run"), ("?", "Tip"), ("n", "News"), ("q", "Quit")]
    } else if app.active_tab == TAB_PIPELINE && app.pipelines.preview.is_some() {
        vec![("\u{2190}\u{2192}", "Choose"), ("Enter", "Select"), ("\u{2191}\u{2193}", "Scroll"), ("Esc", "Close (nothing changes)")]
    } else if app.active_tab == TAB_PIPELINE && app.about_highlighted() {
        vec![(&tabs, "Tabs"), ("\u{2191}\u{2193}", "Choose a pipeline"), ("Enter", "Copy link"), ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]
    } else if app.active_tab == TAB_PIPELINE {
        vec![(&tabs, "Tabs"), ("\u{2191}\u{2193}", "Choose a pipeline"), ("Enter", "Preview and apply"), ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]
    } else if app.active_tab == TAB_METHODS && app.on_subtabs() && app.sidebar() {
        vec![(&tabs, "Tabs"), ("\u{2191}\u{2193}/[ ]", "Pages"), ("\u{2192}/Enter", "Open"), ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]
    } else if app.active_tab == TAB_METHODS && app.on_subtabs() {
        vec![(&tabs, "Tabs"), ("\u{2190}\u{2192}/[ ]", "Sub-tabs"), ("\u{2193}/Enter", "Open"), ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]
    } else if app.active_tab == TAB_METHODS {
        let (back, pages) = if app.sidebar() { ("\u{2190}", "Sidebar") } else { ("[ ]", "Sub-tabs") };
        let open = if app.page() == Some(crate::tui::app::Page::Pipeline) { ("Enter/s", "Open graph") } else { ("Enter/c", "Copy methods text") };
        vec![(&tabs, "Tabs"), (back, pages), ("\u{2191}\u{2193}", "Scroll"), open, ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]
    } else if app.on_subtabs() && app.sidebar() {
        let mode = if app.advanced { "Basic" } else { "Advanced" };
        vec![(&tabs, "Tabs"), ("\u{2191}\u{2193}/[ ]", "Sections"), ("\u{2192}/Enter", "Settings"), ("A", mode), ("R", "Reset section"),
             ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]
    } else if app.on_subtabs() {
        let recon = app.active_tab == crate::tui::app::TAB_RECONSTRUCTION;
        let what = if recon { "Sections" } else { "Sub-tabs" };
        let mut keys = vec![(tabs.as_str(), "Tabs"), ("\u{2190}\u{2192}/[ ]", what), ("\u{2193}/Enter", "Settings")];
        if recon {
            keys.push(("A", if app.advanced { "Basic" } else { "Advanced" }));
        }
        keys.extend([("R", "Reset page"), ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]);
        keys
    } else {
        // The row's own keys: Enter for a choice (it opens a list), Enter to edit a number or
        // text, Space for a switch. ←/→ never change a value; on the Reconstruction tab ← goes
        // back to the sections.
        let rows_tab = App::is_rows_tab(app.active_tab);
        let row = if rows_tab { app.focused_row() } else { None };
        let on_series = app.active_tab == TAB_INPUT && app.input_mode == crate::tui::app::InputMode::DicomToBids
            && app.active_field >= App::INPUT_IO_FIELDS && matches!(app.dicom_state.focus, crate::tui::app::DicomFocus::Series(_));
        let choose = match &row {
            Some(r) => crate::tui::recommend::picker_noun(r)
                .or_else(|| app.selector_modal(r).map(|m| m.title.to_lowercase()))
                .map(|noun| format!("Choose {noun}")),
            None if app.active_tab == TAB_INPUT && app.active_field == 0 => Some("Choose input mode".to_string()),
            None if on_series => Some("Choose series type".to_string()),
            None => None,
        };
        let number = row.as_ref().is_some_and(|r| app.mask().mask_value_text(r).is_some());
        let in_picker = app.active_tab == TAB_INPUT && app.input_mode == crate::tui::app::InputMode::Example
            && app.active_field >= App::INPUT_IO_FIELDS;
        let on_run = app.active_tab == TAB_INPUT && app.input_mode == crate::tui::app::InputMode::Bids
            && app.active_field >= App::INPUT_IO_FIELDS
            && matches!(app.filter_state.focus, crate::tui::app::FilterFocus::TreeNode(i)
                if matches!(app.filter_state.visible_rows().get(i), Some(crate::tui::app::TreeRow::Run { .. })));
        let recon = app.active_tab == crate::tui::app::TAB_RECONSTRUCTION;
        let mut keys = vec![(tabs.as_str(), "Tabs"), ("[ ]", if recon { "Sections" } else { "Sub-tabs" }), ("\u{2191}\u{2193}", "Navigate")];
        if rows_tab && app.has_subtabs() {
            keys.push(("\u{2190}", if app.sidebar() { "Sidebar" } else { "Sections" }));
        }
        match (&choose, &row) {
            (Some(c), _) => keys.push(("Enter", c.as_str())),
            // While a download runs there is nothing to press or tick.
            (None, None) if in_picker && app.example_state.status == crate::tui::app::ConvertStatus::Converting => {}
            (None, None) if in_picker && app.example_state.cursor == crate::tui::app::ExampleState::BUTTON => keys.push(("Enter", "Download")),
            (None, None) if in_picker => keys.push(("Space", "Tick")),
            (None, None) if on_run => keys.extend([("Space", "Tick"), ("\u{2190}\u{2192}", "Orientation group")]),
            (None, None) if !rows_tab => keys.extend([("Space", "Change"), ("Enter", "Edit")]),
            (None, Some(PipelineRow::Param { .. })) => keys.push(("Enter", "Edit")),
            (None, Some(PipelineRow::MaskOpThresholdValue { .. })) => keys.push(("Enter", "Edit")),
            (None, Some(_)) if number => keys.extend([("Enter", "Edit"), ("+/-", "Nudge")]),
            (None, Some(PipelineRow::Toggle { .. })) => keys.push(("Space", "Toggle")),
            (None, Some(PipelineRow::MaskOpGeneratorParam { .. })) => keys.push(("Space", "Switch")),
            (None, Some(PipelineRow::MaskOpAddSection { .. })) => keys.push(("Enter", "Add")),
            _ => {}
        }
        if row.as_ref().is_some_and(|r| App::row_marker(&app.rule_view(), r).is_some()) {
            keys.push(("!", "Advice"));
        }
        if row.as_ref().is_some_and(|r| app.mask().row_removes_section(r)) {
            keys.push(("d", "Remove section"));
        }
        if recon {
            keys.push(("A", if app.advanced { "Basic" } else { "Advanced" }));
        }
        if rows_tab {
            keys.push(("P", "Pipeline"));
        }
        keys.extend([("r", "Reset"), ("R", if recon { "Reset section" } else { "Reset page" }), ("F5", "Run"), ("?", "Tip"), ("q", "Quit")]);
        draw_keys(f, keys, area);
        return;
    };
    draw_keys(f, keys, area);
}

/// The help bar's key hints: each key, then what it does. Hints that every screen has (the tip,
/// moving up and down, resets) give way first on a narrow terminal, so the row's own keys stay.
fn draw_keys(f: &mut Frame, mut keys: Vec<(&str, &str)>, area: ratatui::layout::Rect) {
    let width = |keys: &[(&str, &str)]| 1 + keys.iter().map(|(k, w)| k.chars().count() + 1 + w.chars().count()).sum::<usize>()
        + 2 * keys.len().saturating_sub(1);
    for key in ["?", "\u{2191}\u{2193}", "r", "R", "P", "[ ]"] {
        if width(&keys) <= area.width as usize {
            break;
        }
        keys.retain(|(k, _)| *k != key);
    }
    let mut help = vec![Span::raw(" ")];
    for (i, (key, what)) in keys.into_iter().enumerate() {
        if i > 0 {
            help.push(Span::raw("  "));
        }
        help.push(Span::styled(key.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
        help.push(Span::styled(format!(":{what}"), Style::default().fg(Color::DarkGray)));
    }

    f.render_widget(Paragraph::new(Line::from(help)), area);
}
