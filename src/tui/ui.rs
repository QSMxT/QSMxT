use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs},
    Frame,
};

use super::app::{App, FieldKind, PipelineRow, TAB_NAMES};
use super::command;

pub fn draw(f: &mut Frame, app: &mut App) {
    // Compute command preview height dynamically
    let cmd = command::build_command_string(app);
    let available_width = f.area().width.saturating_sub(4) as usize; // borders + padding
    let cmd_lines = cmd.len()
        .checked_div(available_width)
        .map(|v| (v + 1).clamp(1, 6))
        .unwrap_or(1);
    let preview_height = cmd_lines as u16 + 2; // +2 for borders

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),             // tab bar
            Constraint::Min(8),               // form
            Constraint::Length(preview_height), // command preview (dynamic)
            Constraint::Length(1),             // help bar
        ])
        .split(f.area());

    draw_tabs(f, app, chunks[0]);
    draw_form(f, app, chunks[1]);
    draw_command_preview_with(f, &cmd, chunks[2]);
    draw_help_bar(f, app, chunks[3]);
}

/// Render a scrollable paragraph with a scrollbar when content exceeds visible height.
/// Updates `scroll_offset` in place so it persists between frames.
fn render_scrollable(
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

fn draw_tabs(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let titles: Vec<Line> = TAB_NAMES
        .iter()
        .enumerate()
        .map(|(i, t)| Line::from(format!(" {}:{} ", i + 1, t)))
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " QSMxT.rs ({}) / QSM.rs ({}) ",
            env!("CARGO_PKG_VERSION"), env!("QSM_CORE_VERSION")
        )))
        .select(app.active_tab)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|");

    f.render_widget(tabs, area);
}

fn draw_form(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    if app.active_tab == 0 {
        draw_input_tab(f, app, area);
        return;
    }
    if app.active_tab == 1 {
        draw_pipeline_tab(f, app, area);
        return;
    }
    if app.active_tab == 4 {
        draw_methods_tab(f, app, area);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", TAB_NAMES[app.active_tab]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into scrollable form area + help text area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);
    let form_area = chunks[0];
    let help_area = chunks[1];

    let fields = &app.tab_fields[app.active_tab];

    let mut lines: Vec<Line> = Vec::new();
    let mut field_to_line: Vec<usize> = Vec::new(); // maps field index -> line index
    for (i, field) in fields.iter().enumerate() {
        if !app.is_field_visible(app.active_tab, i) {
            field_to_line.push(0); // placeholder, won't be used
            continue;
        }
        field_to_line.push(lines.len());
        let focused = i == app.active_field;
        let editing = focused && app.editing;

        // Indent sub-fields under a parent checkbox
        let indent = match (app.active_tab, i) {
            (2, 1..=6) => true,   // SWI settings under "Compute SWI"
            (3, 4..=9) => true,   // SLURM settings under "Execution Mode"
            _ => false,
        };
        let prefix = if indent { "    " } else { "  " };
        let label_width: usize = if indent { 20 } else { 22 };

        let line = match &field.kind {
            FieldKind::Text => {
                let value = app.get_text_value(app.active_tab, i).to_string();
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let val_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) };
                let is_required = app.active_tab == 3 && i == 4 && app.form.execution_mode == 1;
                let display_val = if value.is_empty() && !editing && is_required {
                    Span::styled("(required)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
                } else if value.is_empty() && !editing {
                    Span::styled("(empty)", Style::default().fg(Color::DarkGray))
                } else {
                    Span::styled(value, val_style)
                };
                Line::from(vec![
                    Span::styled(format!("{}{:w$}", prefix, format!("{}:", field.label), w = label_width), label_style),
                    display_val,
                ])
            }
            FieldKind::Select { options } => {
                let selected = app.get_select_value(app.active_tab, i);
                let val = options.get(selected).unwrap_or(&"?");
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let label = format!("{}{:w$}", prefix, format!("{}:", field.label), w = label_width);
                if focused {
                    Line::from(vec![
                        Span::styled(label, label_style),
                        Span::styled("◀ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(*val, Style::default().fg(Color::Cyan)),
                        Span::styled(" ▶", Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(label, label_style),
                        Span::styled(*val, Style::default().fg(Color::Gray)),
                    ])
                }
            }
            FieldKind::Checkbox => {
                let checked = app.get_checkbox_value(app.active_tab, i);
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let (marker, color) = if checked { ("[x]", Color::Green) } else { ("[ ]", Color::Gray) };
                Line::from(vec![
                    Span::styled(format!("{}{:w$}", prefix, format!("{}:", field.label), w = label_width), label_style),
                    Span::styled(marker, Style::default().fg(color)),
                ])
            }
        };
        lines.push(line);
    }

    let focused_line = field_to_line.get(app.active_field).copied();
    render_scrollable(f, form_area, lines, &mut app.form_scroll_offset, focused_line);

    // Help text for focused field
    if app.active_field < fields.len() {
        let help = fields[app.active_field].help;
        if !help.is_empty() {
            let help_para = Paragraph::new(Line::from(Span::styled(
                format!("  {}", help),
                Style::default().fg(Color::DarkGray),
            )));
            f.render_widget(help_para, help_area);
        }
    }

    // Set cursor if editing
    if app.editing {
        if let Some(line_idx) = focused_line {
            let scroll = app.form_scroll_offset;
            if line_idx >= scroll && line_idx < scroll + form_area.height as usize {
                let y = form_area.y + (line_idx - scroll) as u16;
                let x = form_area.x + 24 + app.cursor_pos as u16;
                f.set_cursor_position((x, y));
            }
        }
    }
}

fn draw_input_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Input ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into scrollable content + help text area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);
    let content_area = chunks[0];
    let help_area = chunks[1];

    let io_field_count = super::app::App::INPUT_IO_FIELDS;
    let in_io = app.active_field < io_field_count;
    let is_bids = app.input_mode == super::app::InputMode::Bids;
    let is_nifti = app.input_mode == super::app::InputMode::NIfTI;

    // Build lines for IO fields
    let mut lines: Vec<Line> = Vec::new();

    // Field 0: Mode selector
    let mode_focused = in_io && app.active_field == 0;
    let mode_label_style = if mode_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let mode_text = match app.input_mode {
        super::app::InputMode::Bids => "BIDS",
        super::app::InputMode::NIfTI => "NIfTI -> BIDS",
        super::app::InputMode::DicomToBids => "DICOM -> BIDS",
    };
    let is_experimental = !matches!(app.input_mode, super::app::InputMode::Bids);
    let mode_style = Style::default().fg(Color::Cyan);
    let experimental_span = Span::styled(" (experimental)", Style::default().fg(Color::Yellow));
    if mode_focused {
        let mut spans = vec![
            Span::styled(format!("  {:22}", "Input Mode:"), mode_label_style),
            Span::styled("< ", Style::default().fg(Color::DarkGray)),
            Span::styled(mode_text, mode_style.add_modifier(Modifier::BOLD)),
            Span::styled(" >", Style::default().fg(Color::DarkGray)),
        ];
        if is_experimental { spans.push(experimental_span); }
        lines.push(Line::from(spans));
    } else {
        let mut spans = vec![
            Span::styled(format!("  {:22}", "Input Mode:"), mode_label_style),
            Span::styled(mode_text, mode_style),
        ];
        if is_experimental { spans.push(experimental_span); }
        lines.push(Line::from(spans));
    }

    // Fields 1-3: directory/config fields (labels change based on mode)
    let io_labels = match app.input_mode {
        super::app::InputMode::Bids => ["BIDS Directory", "Output Directory", "Config File"],
        super::app::InputMode::NIfTI => ["Input Directory", "Output BIDS Dir", "Config File"],
        super::app::InputMode::DicomToBids => ["DICOM Directory", "Output BIDS Dir", "Config File"],
    };

    // Fields 1-3 are the text IO fields
    for (label_idx, io_label) in io_labels.iter().enumerate() {
        let field_idx = label_idx + 1; // field 0 is mode selector
        let focused = in_io && field_idx == app.active_field;
        let label_style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        // Text field - pull value from the right source based on mode
        let value: &str = match (label_idx, app.input_mode) {
            (0, super::app::InputMode::Bids) => &app.form.bids_dir,
            (0, super::app::InputMode::NIfTI) => &app.nifti_state.input_dir,
            (0, super::app::InputMode::DicomToBids) => &app.dicom_state.dicom_dir,
            (1, super::app::InputMode::Bids) => &app.form.output_dir,
            (1, super::app::InputMode::NIfTI) => &app.nifti_state.output_dir,
            (1, super::app::InputMode::DicomToBids) => &app.dicom_state.output_dir,
            (2, _) => &app.form.config_file,
            _ => "",
        };
        let val_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) };
        // Placeholder logic
        let primary_dir = match app.input_mode {
            super::app::InputMode::Bids => &app.form.bids_dir,
            super::app::InputMode::NIfTI => &app.nifti_state.input_dir,
            super::app::InputMode::DicomToBids => &app.dicom_state.dicom_dir,
        };
        let display_val = if label_idx == 1 && value.is_empty() && !primary_dir.is_empty() && !(focused && app.editing) {
            if is_bids {
                Span::styled(primary_dir.to_string(), Style::default().fg(Color::DarkGray))
            } else {
                Span::styled("(auto: bids_output/)", Style::default().fg(Color::DarkGray))
            }
        } else if label_idx == 0 && value.is_empty() && !(focused && app.editing) {
            if is_nifti {
                Span::styled("(optional, for auto-scan)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            } else {
                Span::styled("(required)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            }
        } else if value.is_empty() && !(focused && app.editing) {
            Span::styled("(empty)", Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(value.to_string(), val_style)
        };
        let line = Line::from(vec![
            Span::styled(format!("  {:22}", format!("{}:", io_label)), label_style),
            display_val,
        ]);
        lines.push(line);
    }

    // Separator + mode-specific content below IO fields
    lines.push(Line::from(""));

    if is_nifti {
        // ─── NIfTI mode ───
        draw_nifti_section(&app.nifti_state, in_io, &mut lines);
    } else if is_bids {
        // ─── BIDS mode: filters + tree ───
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

                let include_focused = !in_io && app.filter_state.focus == super::app::FilterFocus::Include;
                let include_label = Span::styled(
                    "  Include:  ",
                    if include_focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                    else { Style::default().fg(Color::White) },
                );
                let include_val = Span::styled(&app.filter_state.include_pattern, Style::default().fg(Color::Cyan));
                lines.push(Line::from(vec![include_label, include_val]));

                let exclude_focused = !in_io && app.filter_state.focus == super::app::FilterFocus::Exclude;
                let exclude_label = Span::styled(
                    "  Exclude:  ",
                    if exclude_focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                    else { Style::default().fg(Color::White) },
                );
                let exclude_val = if app.filter_state.exclude_pattern.is_empty() && !app.filter_state.exclude_editing {
                    Span::styled("(empty)", Style::default().fg(Color::DarkGray))
                } else {
                    Span::styled(&app.filter_state.exclude_pattern, Style::default().fg(Color::Cyan))
                };
                lines.push(Line::from(vec![exclude_label, exclude_val]));
                lines.push(Line::from(""));

                let visible = app.filter_state.visible_rows();
                for (i, row) in visible.iter().enumerate() {
                    let focused = !in_io && app.filter_state.focus == super::app::FilterFocus::TreeNode(i);
                    let line = match row {
                        super::app::TreeRow::Subject(si) => {
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
                        super::app::TreeRow::Session(si, sei) => {
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
                        super::app::TreeRow::Run { sub, ses, run } => {
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
                            Line::from(Span::styled(format!("{}{} {}", indent, marker, leaf.display), style))
                        }
                    };
                    lines.push(line);
                }

                lines.push(Line::from(""));
                let ne_focused = !in_io && app.filter_state.focus == super::app::FilterFocus::NumEchoes;
                let ne_label = Span::styled(
                    "  Num Echoes: ",
                    if ne_focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                    else { Style::default().fg(Color::White) },
                );
                let ne_val = if app.filter_state.num_echoes.is_empty() && !app.filter_state.num_echoes_editing {
                    Span::styled("(all)", Style::default().fg(Color::DarkGray))
                } else {
                    Span::styled(&app.filter_state.num_echoes, Style::default().fg(Color::Cyan))
                };
                lines.push(Line::from(vec![ne_label, ne_val]));

                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  {} run(s), {} selected", tree.total_runs(), tree.selected_runs()),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    } else {
        // ─── DICOM mode: series classification ───
        draw_dicom_series_section(&app.dicom_state, &mut lines, in_io);
    }

    // IO fields: mode(1) + dir fields(3) = 4 lines total (no blank between mode and dirs)
    let focused_line = if in_io {
        Some(app.active_field) // field 0 = mode line, 1-3 = dirs
    } else if is_bids {
        // Filter area starts after IO fields + separator + header
        let offset = io_field_count + 2; // +2 for blank + header
        match app.filter_state.focus {
            super::app::FilterFocus::Include => Some(offset),
            super::app::FilterFocus::Exclude => Some(offset + 1),
            super::app::FilterFocus::TreeNode(i) => Some(offset + i + 3),
            super::app::FilterFocus::NumEchoes => {
                let vis_len = app.filter_state.visible_rows().len();
                Some(offset + vis_len + 4)
            }
        }
    } else if is_nifti {
        // NIfTI section: compute line position from focus
        // Layout: blank, (optional scan_info), mag_header, AddMag, mag files...,
        // blank, phase_header, AddPhase, phase files..., blank, params_header, EchoTimes, FieldStrength, B0Direction
        let has_scan_info = !app.nifti_state.magnitude_files.is_empty()
            || !app.nifti_state.phase_files.is_empty()
            || !app.nifti_state.scan_log.is_empty();
        let offset = io_field_count + 1 + (has_scan_info as usize) + 1; // blank + (scan_info?) + header
        let items = app.nifti_state.focusable_items();
        if let Some(pos) = items.iter().position(|f| f == &app.nifti_state.focus) {
            let mut line = offset;
            for (idx, item) in items.iter().enumerate() {
                if idx == pos { break; }
                match item {
                    super::app::NiftiFocus::AddMagnitude => line += 1,
                    super::app::NiftiFocus::MagFile(_) => line += 1,
                    super::app::NiftiFocus::AddPhase => line += 3,      // line + blank + header + add line
                    super::app::NiftiFocus::PhaseFile(_) => line += 1,
                    super::app::NiftiFocus::EchoTimes => line += 3,     // line + blank + header + param
                    super::app::NiftiFocus::FieldStrength => line += 1,
                    super::app::NiftiFocus::B0Direction => line += 1,
                    super::app::NiftiFocus::ConvertButton => line += 2, // blank + button
                }
            }
            Some(line)
        } else {
            None
        }
    } else {
        // DICOM series area
        let offset = io_field_count + 2; // blank + header
        match app.dicom_state.focus {
            super::app::DicomFocus::Series(i) => {
                // Mirror the unique-series render layout: an acq header line whenever
                // the acquisition name changes, then one line per unique series.
                if let Some(ref session) = app.dicom_state.session {
                    let mut line_offset = offset;
                    let mut current_acq: Option<String> = None;
                    let mut found = None;
                    for (current_flat, g) in session.unique_series().iter().enumerate() {
                        if current_acq.as_deref() != Some(g.acq_name.as_str()) {
                            current_acq = Some(g.acq_name.clone());
                            line_offset += 1; // acq header
                        }
                        if current_flat == i {
                            found = Some(line_offset);
                            break;
                        }
                        line_offset += 1;
                    }
                    found
                } else {
                    None
                }
            }
            super::app::DicomFocus::ConvertButton => Some(lines.len().saturating_sub(1)),
        }
    };

    let scroll_offset = match app.input_mode {
        super::app::InputMode::Bids => &mut app.form_scroll_offset,
        super::app::InputMode::NIfTI => &mut app.nifti_state.scroll_offset,
        super::app::InputMode::DicomToBids => &mut app.dicom_state.scroll_offset,
    };
    render_scrollable(f, content_area, lines, scroll_offset, focused_line);

    // Help text
    let help_text = if in_io {
        match app.active_field {
            0 => "Left/Right to switch input mode (BIDS / NIfTI / DICOM)",
            1 => match app.input_mode {
                super::app::InputMode::Bids => "Path to BIDS-formatted dataset directory",
                super::app::InputMode::NIfTI => "Directory with NIfTI files + JSON sidecars (optional, for auto-scan)",
                super::app::InputMode::DicomToBids => "Path to directory containing DICOM files",
            },
            2 => if is_bids { "Output directory (defaults to BIDS directory)" } else { "Output BIDS directory (empty = auto-generate)" },
            3 => "Optional pipeline configuration file (TOML)",
            _ => "",
        }
    } else if is_nifti {
        match &app.nifti_state.focus {
            super::app::NiftiFocus::AddMagnitude | super::app::NiftiFocus::AddPhase => "Enter: type glob/path to add files",
            super::app::NiftiFocus::MagFile(_) | super::app::NiftiFocus::PhaseFile(_) => "Shift+J/K: reorder, d: remove",
            super::app::NiftiFocus::EchoTimes => "Enter: edit echo times (comma-separated, in ms)",
            super::app::NiftiFocus::FieldStrength => "Enter: edit field strength (in Tesla)",
            super::app::NiftiFocus::B0Direction => "Enter: edit B0 direction (comma-separated x,y,z)",
            super::app::NiftiFocus::ConvertButton => "Enter: convert NIfTI files to BIDS directory",
        }
    } else if is_bids {
        match app.filter_state.focus {
            super::app::FilterFocus::Include => "Glob patterns to include (space-separated, e.g. sub-1* *ses-pre*)",
            super::app::FilterFocus::Exclude => "Glob patterns to exclude (space-separated, e.g. *mygrea*)",
            super::app::FilterFocus::TreeNode(_) => "Space: toggle, Enter: expand/collapse",
            super::app::FilterFocus::NumEchoes => "Limit number of echoes to process",
        }
    } else {
        match app.dicom_state.focus {
            super::app::DicomFocus::Series(_) => "Left/Right: change type, Enter: cycle type",
            super::app::DicomFocus::ConvertButton => "Enter: run dcm2niix conversion",
        }
    };
    if !help_text.is_empty() {
        let help_para = Paragraph::new(Line::from(Span::styled(
            format!("  {}", help_text),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(help_para, help_area);
    }

    // Set cursor if editing IO text field
    if app.editing && in_io && app.active_field > 0 {
        let scroll = match app.input_mode {
            super::app::InputMode::Bids => app.form_scroll_offset,
            super::app::InputMode::NIfTI => app.nifti_state.scroll_offset,
            super::app::InputMode::DicomToBids => app.dicom_state.scroll_offset,
        };
        let line = app.active_field;
        if line >= scroll && line < scroll + content_area.height as usize {
            let y = content_area.y + (line - scroll) as u16;
            let x = content_area.x + 24 + app.cursor_pos as u16;
            f.set_cursor_position((x, y));
        }
    }
    // Set cursor if editing filter include/exclude/num_echoes (BIDS mode only)
    if is_bids && app.filter_state.include_editing {
        let offset = io_field_count + 2;
        let scroll = app.form_scroll_offset;
        if offset >= scroll && offset < scroll + content_area.height as usize {
            let y = content_area.y + (offset - scroll) as u16;
            let x = content_area.x + 12 + app.filter_state.include_cursor as u16;
            f.set_cursor_position((x, y));
        }
    } else if is_bids && app.filter_state.exclude_editing {
        let offset = io_field_count + 2 + 1;
        let scroll = app.form_scroll_offset;
        if offset >= scroll && offset < scroll + content_area.height as usize {
            let y = content_area.y + (offset - scroll) as u16;
            let x = content_area.x + 12 + app.filter_state.exclude_cursor as u16;
            f.set_cursor_position((x, y));
        }
    } else if is_bids && app.filter_state.num_echoes_editing {
        let offset = io_field_count + 2 + 1;
        let vis_len = app.filter_state.visible_rows().len();
        let line = offset + vis_len + 2;
        let scroll = app.form_scroll_offset;
        if line >= scroll && line < scroll + content_area.height as usize {
            let y = content_area.y + (line - scroll) as u16;
            let x = content_area.x + 14 + app.filter_state.num_echoes_cursor as u16;
            f.set_cursor_position((x, y));
        }
    }

    // Set cursor if editing NIfTI parameter fields (EchoTimes, FieldStrength, B0Direction)
    if is_nifti && app.nifti_state.editing && !in_io {
        let param_focused = matches!(
            app.nifti_state.focus,
            super::app::NiftiFocus::EchoTimes
            | super::app::NiftiFocus::FieldStrength
            | super::app::NiftiFocus::B0Direction
        );
        if param_focused {
            if let Some(line) = focused_line {
                let scroll = app.nifti_state.scroll_offset;
                if line >= scroll && line < scroll + content_area.height as usize {
                    let y = content_area.y + (line - scroll) as u16;
                    let x = content_area.x + 24 + app.nifti_state.cursor as u16;
                    f.set_cursor_position((x, y));
                }
            }
        }
    }
}

/// Render the NIfTI configuration section (used within the unified input tab).
fn draw_nifti_section(
    ns: &super::app::NiftiState,
    in_io: bool,
    lines: &mut Vec<Line<'_>>,
) {
    let focused = |f: &super::app::NiftiFocus| -> bool { !in_io && ns.focus == *f };

    let label_style = |f: &super::app::NiftiFocus| -> Style {
        if focused(f) {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    };

    // Input Dir scan status
    let scan_info = if !ns.magnitude_files.is_empty() || !ns.phase_files.is_empty() {
        format!("  (found {} mag, {} phase)", ns.magnitude_files.len(), ns.phase_files.len())
    } else if !ns.scan_log.is_empty() {
        format!("  ({} unclassified)", ns.scan_log.len())
    } else {
        String::new()
    };
    if !scan_info.is_empty() {
        lines.push(Line::from(Span::styled(
            scan_info,
            Style::default().fg(Color::DarkGray),
        )));
    }

    // ── Magnitude section ──
    lines.push(Line::from(Span::styled(
        format!("  -- Magnitude Files ({}) --", ns.magnitude_files.len()),
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));

    // Add magnitude button
    let add_mag_focus = super::app::NiftiFocus::AddMagnitude;
    if ns.editing && ns.adding_to == Some(crate::nifti::convert::NiftiPartType::Magnitude) {
        lines.push(Line::from(vec![
            Span::styled("  [+] ", label_style(&add_mag_focus)),
            Span::styled(ns.add_pattern.clone(), Style::default().fg(Color::Cyan)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "  [+] Add files...",
            label_style(&add_mag_focus),
        )));
    }

    // Magnitude file list
    for (i, path) in ns.magnitude_files.iter().enumerate() {
        let f = super::app::NiftiFocus::MagFile(i);
        let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let style = if focused(&f) {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Green)
        };
        lines.push(Line::from(Span::styled(
            format!("    {}. {}", i + 1, basename),
            style,
        )));
    }

    lines.push(Line::from(""));

    // ── Phase section ──
    lines.push(Line::from(Span::styled(
        format!("  -- Phase Files ({}) --", ns.phase_files.len()),
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));

    // Add phase button
    let add_phase_focus = super::app::NiftiFocus::AddPhase;
    if ns.editing && ns.adding_to == Some(crate::nifti::convert::NiftiPartType::Phase) {
        lines.push(Line::from(vec![
            Span::styled("  [+] ", label_style(&add_phase_focus)),
            Span::styled(ns.add_pattern.clone(), Style::default().fg(Color::Cyan)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "  [+] Add files...",
            label_style(&add_phase_focus),
        )));
    }

    // Phase file list
    for (i, path) in ns.phase_files.iter().enumerate() {
        let f = super::app::NiftiFocus::PhaseFile(i);
        let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let style = if focused(&f) {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Cyan)
        };
        lines.push(Line::from(Span::styled(
            format!("    {}. {}", i + 1, basename),
            style,
        )));
    }

    lines.push(Line::from(""));

    // ── Parameters section ──
    lines.push(Line::from(Span::styled(
        "  -- Parameters --",
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));

    // Echo Times
    let et_focus = super::app::NiftiFocus::EchoTimes;
    let et_val = if ns.echo_times.is_empty() && !(ns.editing && focused(&et_focus)) {
        Span::styled("(required)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
    } else {
        Span::styled(ns.echo_times.clone(), Style::default().fg(Color::Cyan))
    };
    lines.push(Line::from(vec![
        Span::styled(format!("  {:22}", "Echo Times (ms):"), label_style(&et_focus)),
        et_val,
    ]));

    // Field Strength
    let fs_focus = super::app::NiftiFocus::FieldStrength;
    let fs_val = if ns.field_strength.is_empty() && !(ns.editing && focused(&fs_focus)) {
        Span::styled("(required)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
    } else {
        Span::styled(ns.field_strength.clone(), Style::default().fg(Color::Cyan))
    };
    lines.push(Line::from(vec![
        Span::styled(format!("  {:22}", "Field Strength (T):"), label_style(&fs_focus)),
        fs_val,
    ]));

    // B0 Direction
    let b0_focus = super::app::NiftiFocus::B0Direction;
    lines.push(Line::from(vec![
        Span::styled(format!("  {:22}", "B0 Direction:"), label_style(&b0_focus)),
        Span::styled(ns.b0_direction.clone(), Style::default().fg(Color::Cyan)),
    ]));

    // Validation warnings
    if !ns.magnitude_files.is_empty() && !ns.phase_files.is_empty()
        && ns.magnitude_files.len() != ns.phase_files.len()
    {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Warning: {} magnitude vs {} phase files", ns.magnitude_files.len(), ns.phase_files.len()),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(""));

    // Convert button + status
    let convert_focus = super::app::NiftiFocus::ConvertButton;
    let btn_style = if focused(&convert_focus) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        match ns.convert_status {
            super::app::ConvertStatus::Done => Style::default().fg(Color::Green),
            super::app::ConvertStatus::Error => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::White),
        }
    };
    let btn_label = match ns.convert_status {
        super::app::ConvertStatus::Idle => "  [ Convert to BIDS ]",
        super::app::ConvertStatus::Converting => "  [ Converting... ]",
        super::app::ConvertStatus::Done => "  [ Done! ]",
        super::app::ConvertStatus::Error => "  [ Error ]",
    };
    lines.push(Line::from(Span::styled(btn_label, btn_style)));

    // Show conversion log
    for msg in &ns.convert_log {
        lines.push(Line::from(Span::styled(
            format!("  {}", msg),
            Style::default().fg(Color::DarkGray),
        )));
    }
}

/// Render the DICOM series classification section (used within the unified input tab).
fn draw_dicom_series_section(
    ds: &super::app::DicomConvertState,
    lines: &mut Vec<Line<'_>>,
    in_io: bool,
) {
    if ds.dicom_dir.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "  Set DICOM directory above first",
            Style::default().fg(Color::DarkGray),
        )));
        return;
    }

    if ds.scan_status == super::app::ScanStatus::Scanning {
        let n = ds.scan_files_examined();
        lines.push(Line::from(Span::styled(
            format!("  Scanning DICOM directory... ({} files examined)", n),
            Style::default().fg(Color::Yellow),
        )));
        return;
    }

    if let Some(ref err) = ds.scan_error {
        lines.push(Line::from(Span::styled(
            format!("  Scan error: {}", err),
            Style::default().fg(Color::Red),
        )));
        return;
    }

    let Some(ref session) = ds.session else {
        lines.push(Line::from(Span::styled(
            "  No DICOM files found in directory",
            Style::default().fg(Color::DarkGray),
        )));
        return;
    };

    lines.push(Line::from(Span::styled(
        "  -- Series Classification --",
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));

    // One row per UNIQUE series (shared across subjects), not per per-subject instance.
    let groups = session.unique_series();
    let mut acq_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for g in &groups {
        *acq_counts.entry(g.acq_name.as_str()).or_default() += 1;
    }

    let mut current_acq: Option<&str> = None;
    for (flat_idx, g) in groups.iter().enumerate() {
        if current_acq != Some(g.acq_name.as_str()) {
            current_acq = Some(g.acq_name.as_str());
            let run_label = if g.run_number > 1 {
                format!(" (run {})", g.run_number)
            } else {
                String::new()
            };
            let count = acq_counts.get(g.acq_name.as_str()).copied().unwrap_or(0);
            lines.push(Line::from(Span::styled(
                format!("  acq-{}{} ({} series)", g.acq_name, run_label, count),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )));
        }

        let series = session.series_ref(&g.refs[0]);
        let n_subjects = g.subject_count();
        let series_focused = !in_io && ds.focus == super::app::DicomFocus::Series(flat_idx);
        let type_label = series.series_type.label();
        let echo_info = match series.echo_times.as_slice() {
            [] => String::new(),
            [te] => format!(" TE={:.1}ms", te),
            tes => format!(" {}×TEs=[{:.1}…{:.1}]ms", tes.len(), tes[0], tes[tes.len() - 1]),
        };
        let echo_info = if series.coil_type == crate::dicom::CoilType::Uncombined {
            format!("{} [uncombined ×{}]", echo_info, series.coil_groups.len())
        } else {
            echo_info
        };
        let echo_info = match crate::dicom::recon_desc(&series.image_type) {
            Some(d) => format!("{} [{}]", echo_info, d),
            None => echo_info,
        };
        let files_label = if n_subjects > 1 {
            format!("  ({} files × {} subjects)", series.num_files, n_subjects)
        } else {
            format!("  ({} files)", series.num_files)
        };

        let style = if series_focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let type_style = match series.series_type {
            crate::dicom::SeriesType::Skip => Style::default().fg(Color::DarkGray),
            crate::dicom::SeriesType::Phase => Style::default().fg(Color::Cyan),
            crate::dicom::SeriesType::Magnitude => Style::default().fg(Color::Green),
            crate::dicom::SeriesType::T1w => Style::default().fg(Color::Magenta),
            _ => Style::default().fg(Color::White),
        };

        let line = if series_focused {
            Line::from(vec![
                Span::styled(
                    format!("    {:30}", format!("{}{}", series.description, echo_info)),
                    style,
                ),
                Span::styled("< ", Style::default().fg(Color::DarkGray)),
                Span::styled(type_label, type_style.add_modifier(Modifier::BOLD)),
                Span::styled(" >", Style::default().fg(Color::DarkGray)),
                Span::styled(files_label, Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    format!("    {:30}", format!("{}{}", series.description, echo_info)),
                    style,
                ),
                Span::styled(format!("[{}]", type_label), type_style),
                Span::styled(files_label, Style::default().fg(Color::DarkGray)),
            ])
        };
        lines.push(line);
    }

    // dcm2niix availability indicator (uses the cached resolution)
    lines.push(Line::from(""));
    match ds.dcm2niix_path() {
        Some(path) => {
            let is_bundled = crate::dicom::convert::qsmxt_bin_dir()
                .map(|dir| path.starts_with(&dir))
                .unwrap_or(false);
            let (label, color) = if is_bundled {
                (
                    format!("  dcm2niix: bundled {}", crate::dicom::convert::DCM2NIIX_BUNDLED_VERSION),
                    Color::Green,
                )
            } else {
                (format!("  dcm2niix: {}", path.display()), Color::Green)
            };
            lines.push(Line::from(Span::styled(label, Style::default().fg(color))));
        }
        None => {
            lines.push(Line::from(Span::styled(
                "  dcm2niix not found — DICOM conversion unavailable (reinstall qsmxt or install dcm2niix)",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
        }
    }

    // Convert button
    lines.push(Line::from(""));
    let convert_focused = !in_io && ds.focus == super::app::DicomFocus::ConvertButton;
    let convert_style = if convert_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let status_span = match ds.convert_status {
        super::app::ConvertStatus::Idle => Span::styled("  Ready", Style::default().fg(Color::DarkGray)),
        super::app::ConvertStatus::Converting => Span::styled("  Converting...", Style::default().fg(Color::Yellow)),
        super::app::ConvertStatus::Done => Span::styled("  Done!", Style::default().fg(Color::Green)),
        super::app::ConvertStatus::Error => Span::styled("  Error (see log)", Style::default().fg(Color::Red)),
    };
    lines.push(Line::from(vec![
        Span::styled("  [ Convert to BIDS ]", convert_style),
        status_span,
    ]));

    // Conversion log
    if !ds.convert_log.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  -- Conversion Log --",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        )));
        for log_line in &ds.convert_log {
            let style = if log_line.starts_with("ERROR:") {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(Span::styled(format!("  {}", log_line), style)));
        }
    }
}

fn draw_pipeline_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pipeline ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into form area + help text area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);
    let form_area = chunks[0];
    let help_area = chunks[1];

    // Build lines and collect state from pipeline_state before mutable borrow
    let rows = app.pipeline_state.visible_rows();
    let focusable = app.pipeline_state.focusable_rows();
    let ps_focus = app.pipeline_state.focus;
    let ps_editing = app.pipeline_state.editing;
    let ps_cursor = app.pipeline_state.cursor;

    let mut lines: Vec<Line> = Vec::new();
    let mut focused_help: Option<String> = None;

    let mut focusable_idx = 0;
    for (i, row) in rows.iter().enumerate() {
        let is_focusable = focusable.contains(&i);
        let focused = is_focusable && focusable_idx == ps_focus;
        if is_focusable {
            focusable_idx += 1;
        }

        let line = match row {
            PipelineRow::AlgoSelect { label, field, options, help } => {
                let selected = app.pipeline_state.get_select(field);
                let val = options.get(selected).unwrap_or(&"?");
                if focused {
                    focused_help = help.get(selected).map(|s| s.to_string());
                }
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                if focused {
                    Line::from(vec![
                        Span::styled(format!("  {:22}", format!("{}:", label)), label_style),
                        Span::styled("◀ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(*val, Style::default().fg(Color::Cyan)),
                        Span::styled(" ▶", Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(format!("  {:22}", format!("{}:", label)), label_style),
                        Span::styled(*val, Style::default().fg(Color::Gray)),
                    ])
                }
            }
            PipelineRow::Param { label, field, help } => {
                let val = app.pipeline_state.get_param(field).to_string();
                if focused {
                    focused_help = Some(help.to_string());
                }
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let val_style = if focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let display_val = if val.is_empty() && !(focused && ps_editing) {
                    Span::styled("(default)", Style::default().fg(Color::DarkGray))
                } else {
                    Span::styled(val, val_style)
                };
                Line::from(vec![
                    Span::styled(format!("  {:22}", format!("{}:", label)), label_style),
                    display_val,
                ])
            }
            PipelineRow::Toggle { label, field, help } => {
                let checked = app.pipeline_state.get_toggle(field);
                if focused {
                    focused_help = Some(help.to_string());
                }
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let (marker, color) = if checked {
                    ("[x]", Color::Green)
                } else {
                    ("[ ]", Color::Gray)
                };
                Line::from(vec![
                    Span::styled(format!("  {:22}", format!("{}:", label)), label_style),
                    Span::styled(marker, Style::default().fg(color)),
                ])
            }
            PipelineRow::Separator => Line::from(""),
            PipelineRow::Note { text } => Line::from(Span::styled(
                format!("  {}", text),
                Style::default().fg(Color::Yellow),
            )),
            PipelineRow::MaskSectionHeader { section } => {
                Line::from(Span::styled(
                    format!("  ── Mask {} ──", section + 1),
                    Style::default().fg(Color::DarkGray),
                ))
            }
            PipelineRow::MaskOrSeparator => {
                Line::from(Span::styled(
                    "  ── COMBINED WITH ──",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
                ))
            }
            PipelineRow::MaskOpInput { section } => {
                let input = &app.pipeline_state.mask_sections[*section].input;
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                if focused {
                    if focused_help.is_none() {
                        focused_help = Some("Masking input source (←/→ to change)".to_string());
                    }
                    Line::from(vec![
                        Span::styled(format!("  {:22}", "Input:"), label_style),
                        Span::styled("◀ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{}", input), Style::default().fg(Color::Cyan)),
                        Span::styled(" ▶", Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(format!("  {:22}", "Input:"), label_style),
                        Span::styled(format!("{}", input), Style::default().fg(Color::Gray)),
                    ])
                }
            }
            PipelineRow::MaskOpGenerator { section } => {
                let gen = &app.pipeline_state.mask_sections[*section].generator;
                let algo_name = match gen {
                    crate::pipeline::config::MaskOp::Threshold { .. } => "threshold",
                    crate::pipeline::config::MaskOp::Bet { .. } => "bet",
                    _ => "?",
                };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                if focused {
                    if focused_help.is_none() {
                        focused_help = Some("Mask algorithm (←/→ to switch between threshold and BET)".to_string());
                    }
                    Line::from(vec![
                        Span::styled(format!("  {:22}", "Algorithm:"), label_style),
                        Span::styled("◀ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(algo_name, Style::default().fg(Color::Cyan)),
                        Span::styled(" ▶", Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(format!("  {:22}", "Algorithm:"), label_style),
                        Span::styled(algo_name, Style::default().fg(Color::Gray)),
                    ])
                }
            }
            PipelineRow::MaskOpGeneratorParam { section } => {
                let gen = &app.pipeline_state.mask_sections[*section].generator;
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let (label, val, help) = match gen {
                    crate::pipeline::config::MaskOp::Threshold { method, .. } => {
                        let method_name = match method {
                            crate::pipeline::config::MaskThresholdMethod::Otsu => "otsu",
                            crate::pipeline::config::MaskThresholdMethod::Fixed => "fixed",
                            crate::pipeline::config::MaskThresholdMethod::Percentile => "percentile",
                        };
                        ("Method:", method_name.to_string(), "Threshold method (←/→ to change)")
                    }
                    crate::pipeline::config::MaskOp::Bet { fractional_intensity } => {
                        ("Frac. Intensity:", format!("{:.2}", fractional_intensity), "BET fractional intensity 0.0-1.0, smaller = larger brain (←/→ to adjust)")
                    }
                    _ => ("?:", "?".to_string(), ""),
                };
                if focused {
                    if focused_help.is_none() {
                        focused_help = Some(help.to_string());
                    }
                    Line::from(vec![
                        Span::styled(format!("  {:22}", label), label_style),
                        Span::styled("◀ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(val, Style::default().fg(Color::Cyan)),
                        Span::styled(" ▶", Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(format!("  {:22}", label), label_style),
                        Span::styled(val, Style::default().fg(Color::Gray)),
                    ])
                }
            }
            PipelineRow::MaskOpThresholdValue { section } => {
                let gen = &app.pipeline_state.mask_sections[*section].generator;
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let (label, val) = match gen {
                    crate::pipeline::config::MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Fixed, value } =>
                        ("Value:", value.map(|v| format!("{}", v)).unwrap_or("0.5".to_string())),
                    crate::pipeline::config::MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Percentile, value } =>
                        ("Percentile:", value.map(|v| format!("{}", v)).unwrap_or("75".to_string())),
                    _ => ("Value:", "?".to_string()),
                };
                let display_val = if app.pipeline_state.mask_threshold_editing && focused {
                    app.pipeline_state.mask_threshold_value_buf.clone()
                } else {
                    val
                };
                if focused
                    && focused_help.is_none() {
                        focused_help = Some("Enter to edit value, Esc to cancel".to_string());
                    }
                let val_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) };
                Line::from(vec![
                    Span::styled(format!("  {:22}", label), label_style),
                    Span::styled(display_val, val_style),
                ])
            }
            PipelineRow::MaskOpEntry { section, index } => {
                let op = &app.pipeline_state.mask_sections[*section].refinements[*index];
                let (op_type, op_val) = super::app::PipelineFormState::mask_op_label_value(op);
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let val_style = if focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Gray)
                };
                if focused {
                    if focused_help.is_none() {
                        focused_help = Some(super::app::PipelineFormState::mask_op_help(op).to_string());
                    }
                    Line::from(vec![
                        Span::styled(format!("  {:3}", format!("{}.", index + 1)), label_style),
                        Span::styled(format!("{:19}", format!("{}:", op_type)), label_style),
                        Span::styled("◀ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(op_val.clone(), val_style),
                        Span::styled(" ▶", Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(format!("  {:3}", format!("{}.", index + 1)), Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{:19}", format!("{}:", op_type)), label_style),
                        Span::styled(op_val, val_style),
                    ])
                }
            }
            PipelineRow::MaskOpAddStep { section } => {
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                if app.pipeline_state.mask_ops_adding && focused {
                    let available = app.pipeline_state.available_op_types(*section);
                    let type_name = available.get(app.pipeline_state.mask_ops_add_idx).copied().unwrap_or("?");
                    if focused_help.is_none() {
                        focused_help = Some("←/→ to select type, Enter to add, Esc to cancel".to_string());
                    }
                    Line::from(vec![
                        Span::styled("  +   ", label_style),
                        Span::styled("◀ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(type_name, Style::default().fg(Color::Cyan)),
                        Span::styled(" ▶", Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    if focused && focused_help.is_none() {
                        focused_help = Some("Enter to add step, d to delete, Ctrl+↑/↓ to reorder".to_string());
                    }
                    Line::from(Span::styled("  + Add step...", label_style))
                }
            }
            PipelineRow::MaskOpAddSection => {
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                if focused && focused_help.is_none() {
                    focused_help = Some("Enter to add a new OR'd mask section".to_string());
                }
                Line::from(Span::styled("  + Add mask...", label_style))
            }
        };
        lines.push(line);
    }

    // Determine which line is focused for auto-scroll
    let focused_line = focusable.get(ps_focus).copied();

    render_scrollable(f, form_area, lines, &mut app.pipeline_state.scroll_offset, focused_line);
    let scroll = app.pipeline_state.scroll_offset;

    // Render help text
    if let Some(help) = focused_help {
        let help_para = Paragraph::new(Line::from(Span::styled(
            format!("  {}", help),
            Style::default().fg(Color::DarkGray),
        ))).wrap(ratatui::widgets::Wrap { trim: false });
        f.render_widget(help_para, help_area);
    }

    // Set cursor if editing a param or threshold value
    if ps_editing || app.pipeline_state.mask_threshold_editing {
        if let Some(&row_idx) = focusable.get(ps_focus) {
            if row_idx >= scroll && row_idx < scroll + form_area.height as usize {
                let y = form_area.y + (row_idx - scroll) as u16;
                let label_width = 24;
                let x = form_area.x + label_width + ps_cursor as u16;
                f.set_cursor_position((x, y));
            }
        }
    }
}

fn draw_methods_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let config = command::config_from_app(app);
    let methods_text = crate::pipeline::methods::generate_methods(&config);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Methods ");
    let inner = block.inner(area);
    f.render_widget(block, area);

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
            let width = inner.width as usize;
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
            let width = inner.width as usize;
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

fn draw_command_preview_with(f: &mut Frame, cmd: &str, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Command Preview ");
    let para = Paragraph::new(Line::from(Span::styled(
        cmd.to_string(),
        Style::default().fg(Color::Green),
    )))
    .block(block)
    .wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(para, area);
}

fn draw_help_bar(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    // Show error message if present
    if let Some(ref err) = app.error_message {
        let error_line = Line::from(vec![
            Span::styled(" Error: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(err.as_str(), Style::default().fg(Color::Red)),
        ]);
        f.render_widget(Paragraph::new(error_line), area);
        return;
    }

    let help = if app.editing {
        vec![
            Span::styled(" Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Cancel  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Confirm", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled(" 1-4", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Tabs  ", Style::default().fg(Color::DarkGray)),
            Span::styled("\u{2191}\u{2193}", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("\u{2190}\u{2192}/Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Change  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Edit  ", Style::default().fg(Color::DarkGray)),
            Span::styled("r", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Reset  ", Style::default().fg(Color::DarkGray)),
            Span::styled("R", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Reset All  ", Style::default().fg(Color::DarkGray)),
            Span::styled("F5", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Run  ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Quit", Style::default().fg(Color::DarkGray)),
        ]
    };

    f.render_widget(Paragraph::new(Line::from(help)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render_app(app: &mut App) -> Terminal<TestBackend> {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
    }

    #[test]
    fn test_draw_default_app_no_panic() {
        let mut app = App::new();
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_all_tabs() {
        let mut app = App::new();
        for tab in 0..4 {
            app.active_tab = tab;
            app.active_field = 0;
            let _ = render_app(&mut app);
        }
    }

    #[test]
    fn test_draw_editing_mode() {
        let mut app = App::new();
        app.editing = true;
        app.form.bids_dir = "/some/path".to_string();
        app.cursor_pos = 5;
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_with_form_data() {
        let mut app = App::new();
        app.form.bids_dir = "/data/bids".to_string();
        app.form.output_dir = "/data/out".to_string();
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_algorithms_tab() {
        let mut app = App::new();
        app.active_tab = 2;
        app.active_field = 0;
        let _ = render_app(&mut app);
        // Move through fields
        app.active_field = 4;
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_parameters_tab() {
        let mut app = App::new();
        app.active_tab = 2; // Pipeline tab
        let _ = render_app(&mut app);
        // Change algorithm
        app.pipeline_state.qsm_algorithm = 3; // TGV
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_execution_tab_with_flags() {
        let mut app = App::new();
        app.active_tab = 3;
        app.form.do_swi = true;
        app.form.do_t2starmap = true;
        app.form.dry_run = true;
        app.form.debug = true;
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_non_focused_fields() {
        let mut app = App::new();
        app.active_tab = 0;
        app.active_field = 3; // Last field focused, others not
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_select_not_focused() {
        let mut app = App::new();
        app.active_tab = 2;
        app.active_field = 1; // field 0 (select) not focused
        let _ = render_app(&mut app);
    }

    #[test]
    fn test_draw_empty_text_not_editing() {
        let mut app = App::new();
        // All fields empty, not editing — shows "(empty)"
        app.active_tab = 0;
        app.active_field = 0;
        let _ = render_app(&mut app);
    }
}
