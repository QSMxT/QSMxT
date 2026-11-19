//! The Input tab: the input mode, the I/O fields and the mode's own section.

use super::*;

mod dicom_to_bids;
mod example;
mod nifti_to_bids;
mod orientation;

use dicom_to_bids::*;
use example::*;
use nifti_to_bids::*;
use orientation::*;

pub(super) fn draw_input_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let block = crate::tabs::panel(app);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let io_field_count = crate::app::App::INPUT_IO_FIELDS;
    let in_io = app.active_field < io_field_count;
    let is_bids = app.input_mode == crate::app::InputMode::Bids;
    let is_nifti = app.input_mode == crate::app::InputMode::NIfTI;
    let is_example = app.input_mode == crate::app::InputMode::Example;

    // Help text, wrapped, under the scrollable content: at least two rows so the form does not
    // jump between short ones, at most half the panel.
    let help_text = if in_io {
        match app.active_field {
            0 => "Enter: choose input mode (BIDS / NIfTI / DICOM / Example)",
            1 => match app.input_mode {
                crate::app::InputMode::Bids => "Path to BIDS-formatted dataset directory",
                crate::app::InputMode::NIfTI => "Directory with NIfTI files + JSON sidecars (optional, for auto-scan)",
                crate::app::InputMode::DicomToBids => "Path to directory containing DICOM files",
                crate::app::InputMode::Example => "Directory to create the example dataset in (added to if it exists)",
            },
            2 => if is_bids || is_example { "Output directory (defaults to BIDS directory)" } else { "Output BIDS directory (empty = auto-generate)" },
            3 => "Optional pipeline configuration file (TOML)",
            _ => "",
        }
    } else if is_nifti {
        match &app.nifti_state.focus {
            crate::app::NiftiFocus::AddMagnitude | crate::app::NiftiFocus::AddPhase => "Enter: type glob/path to add files",
            crate::app::NiftiFocus::MagFile(_) | crate::app::NiftiFocus::PhaseFile(_) => "Shift+J/K: reorder, d: remove",
            crate::app::NiftiFocus::EchoTimes => "Enter: edit echo times (comma-separated, in ms)",
            crate::app::NiftiFocus::FieldStrength => "Enter: edit field strength (in Tesla)",
            crate::app::NiftiFocus::B0Direction => "Enter: edit B0 direction (comma-separated x,y,z)",
            crate::app::NiftiFocus::ConvertButton => "Enter: convert NIfTI files to BIDS directory",
        }
    } else if is_bids {
        match app.filter_state.focus {
            crate::app::FilterFocus::Include => "Glob patterns to include (space-separated, e.g. sub-1* *ses-pre*)",
            crate::app::FilterFocus::Exclude => "Glob patterns to exclude (space-separated, e.g. *mygrea*)",
            crate::app::FilterFocus::OrientationGroup => {
                "Runs that are the same object at a different angle: an entity (acq, run, rec, inv), a glob (*acq-dir*), or re:<regex>. Empty = off. Or give runs their groups with ←/→ in the list below. How they combine: QSM → COSMOS/STI"
            }
            crate::app::FilterFocus::TreeNode(_) => "Space/Enter: tick. ←/→: a run's orientation group and reference; a subject or session: collapse/expand",
            crate::app::FilterFocus::NumEchoes => "Limit number of echoes to process",
        }
    } else if is_example {
        if app.example_state.status == crate::app::ConvertStatus::Converting {
            // The button and the download log say what is happening.
            ""
        } else if app.example_state.cursor == crate::app::ExampleState::BUTTON {
            "Enter: download the ticked acquisitions and build the dataset"
        } else {
            "Space/Enter: tick an acquisition (several can share one dataset), r: reset"
        }
    } else {
        match app.dicom_state.focus {
            crate::app::DicomFocus::Series(_) => "Enter: choose the series type",
            crate::app::DicomFocus::KeepUids => "Space/Enter: keep the study's DICOM UIDs and dates in the sidecars, so a DICOM export files its series into the original study. Patient identity is still removed, but the UIDs link the dataset to that study: don't share it outside your site",
            crate::app::DicomFocus::ConvertButton => "Enter: run dcm2niix conversion",
        }
    };
    let help = help_lines(help_text, inner.width, Color::DarkGray);
    let help_rows = (help.len() as u16).clamp(2, (inner.height / 2).max(2));
    let [content_area, divider, help_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(1), Constraint::Length(help_rows)])
        .areas(inner);
    help_divider(f, divider);

    // Build lines for IO fields
    let mut lines: Vec<Line> = Vec::new();
    // Where the DICOM section drew its toggle and Convert button.
    let mut dicom_lines: Option<(usize, usize)> = None;

    // Field 0: Mode selector
    let mode_focused = in_io && app.active_field == 0 && !app.tab_focus;
    let mode_label_style = if mode_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let mode_text = match app.input_mode {
        crate::app::InputMode::Bids => "BIDS",
        crate::app::InputMode::NIfTI => "NIfTI -> BIDS",
        crate::app::InputMode::DicomToBids => "DICOM -> BIDS",
        crate::app::InputMode::Example => "Example dataset",
    };
    // The example dataset lands in a plain BIDS tree, so it carries no experimental caveat.
    let is_experimental = !matches!(app.input_mode, crate::app::InputMode::Bids | crate::app::InputMode::Example);
    let experimental_span = Span::styled(" (experimental)", Style::default().fg(Color::Yellow));
    let mut spans = vec![Span::styled(format!("  {:22}", "Input mode:"), mode_label_style)];
    spans.extend(picker_button(mode_text, mode_focused));
    if is_experimental { spans.push(experimental_span); }
    lines.push(Line::from(spans));

    // Fields 1-3: directory/config fields (labels change based on mode)
    let io_labels = match app.input_mode {
        crate::app::InputMode::Bids => ["BIDS directory", "Output directory", "Config file"],
        crate::app::InputMode::NIfTI => ["Input directory", "Output BIDS dir", "Config file"],
        crate::app::InputMode::DicomToBids => ["DICOM directory", "Output BIDS dir", "Config file"],
        crate::app::InputMode::Example => ["Dataset directory", "Output directory", "Config file"],
    };

    // Fields 1-3 are the text IO fields
    for (label_idx, io_label) in io_labels.iter().enumerate() {
        let field_idx = label_idx + 1; // field 0 is mode selector
        let focused = in_io && field_idx == app.active_field && !app.tab_focus;
        let label_style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        // Text field - pull value from the right source based on mode
        let value: &str = match (label_idx, app.input_mode) {
            (0, crate::app::InputMode::Bids) => &app.form.bids_dir,
            (0, crate::app::InputMode::NIfTI) => &app.nifti_state.input_dir,
            (0, crate::app::InputMode::DicomToBids) => &app.dicom_state.dicom_dir,
            (0, crate::app::InputMode::Example) => &app.example_state.output_dir,
            (1, crate::app::InputMode::Bids) => &app.form.output_dir,
            (1, crate::app::InputMode::NIfTI) => &app.nifti_state.output_dir,
            (1, crate::app::InputMode::DicomToBids) => &app.dicom_state.output_dir,
            (1, crate::app::InputMode::Example) => &app.form.output_dir,
            (2, _) => &app.form.config_file,
            _ => "",
        };
        let val_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) };
        // Placeholder logic
        let primary_dir = match app.input_mode {
            crate::app::InputMode::Bids => &app.form.bids_dir,
            crate::app::InputMode::NIfTI => &app.nifti_state.input_dir,
            crate::app::InputMode::DicomToBids => &app.dicom_state.dicom_dir,
            crate::app::InputMode::Example => &app.example_state.output_dir,
        };
        let display_val = if label_idx == 0 && is_example && value.is_empty() && !(focused && app.editing) {
            Span::styled(
                format!("(default: {}/)", crate::app::EXAMPLE_DEFAULT_DIR),
                Style::default().fg(Color::DarkGray),
            )
        } else if label_idx == 1 && is_example && value.is_empty() && !(focused && app.editing) {
            Span::styled(
                "(defaults to the dataset directory)".to_string(),
                Style::default().fg(Color::DarkGray),
            )
        } else if label_idx == 1 && value.is_empty() && !primary_dir.is_empty() && !(focused && app.editing) {
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

                let include_focused = !in_io && app.filter_state.focus == crate::app::FilterFocus::Include;
                let include_label = Span::styled(
                    "  Include:  ",
                    if include_focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                    else { Style::default().fg(Color::White) },
                );
                let include_val = Span::styled(&app.filter_state.include_pattern, Style::default().fg(Color::Cyan));
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
                    Span::styled(&app.filter_state.exclude_pattern, Style::default().fg(Color::Cyan))
                };
                lines.push(Line::from(vec![exclude_label, exclude_val]));

                lines.extend(orientation_block(app, in_io));
                lines.push(Line::from(""));

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
    } else if is_example {
        // ─── Example mode: drawn below, with the list in a region of its own ───
    } else {
        // ─── DICOM mode: series classification ───
        dicom_lines = draw_dicom_series_section(&app.dicom_state, &mut lines, in_io);
    }

    // Where each focusable thing is drawn, for scrolling to the focused one and for the mouse.
    // IO fields: mode(1) + dir fields(3) = 4 lines total (no blank between mode and dirs)
    use crate::mouse::InputTarget;
    // The orientation block sits between Exclude and the tree, so it pushes everything
    // below it down. One helper renders it and reports its height, so the two cannot drift.
    let ori = if is_bids { orientation_block(app, in_io).len() } else { 0 };
    let line_of = |target: InputTarget| -> Option<usize> {
        match target {
            InputTarget::Io(i) => Some(i),
            InputTarget::Filter(focus) if is_bids => {
                // Filter area starts after IO fields + separator + header
                let offset = io_field_count + 2; // +2 for blank + header
                Some(match focus {
                    crate::app::FilterFocus::Include => offset,
                    crate::app::FilterFocus::Exclude => offset + 1,
                    crate::app::FilterFocus::OrientationGroup => offset + 2,
                    crate::app::FilterFocus::TreeNode(i) => offset + ori + i + 3,
                    crate::app::FilterFocus::NumEchoes => offset + ori + app.filter_state.visible_rows().len() + 4,
                })
            }
            InputTarget::Nifti(focus) if is_nifti => {
                // NIfTI section: compute line position from focus
                // Layout: blank, (optional scan_info), mag_header, AddMag, mag files...,
                // blank, phase_header, AddPhase, phase files..., blank, params_header, EchoTimes, FieldStrength, B0Direction
                let has_scan_info = !app.nifti_state.magnitude_files.is_empty()
                    || !app.nifti_state.phase_files.is_empty()
                    || !app.nifti_state.scan_log.is_empty();
                let offset = io_field_count + 1 + (has_scan_info as usize) + 1; // blank + (scan_info?) + header
                let items = app.nifti_state.focusable_items();
                let pos = items.iter().position(|f| *f == focus)?;
                let mut line = offset;
                for item in &items[..pos] {
                    match item {
                        crate::app::NiftiFocus::AddMagnitude => line += 1,
                        crate::app::NiftiFocus::MagFile(_) => line += 1,
                        crate::app::NiftiFocus::AddPhase => line += 3,      // line + blank + header + add line
                        crate::app::NiftiFocus::PhaseFile(_) => line += 1,
                        crate::app::NiftiFocus::EchoTimes => line += 3,     // line + blank + header + param
                        crate::app::NiftiFocus::FieldStrength => line += 1,
                        crate::app::NiftiFocus::B0Direction => line += 1,
                        crate::app::NiftiFocus::ConvertButton => line += 2, // blank + button
                    }
                }
                Some(line)
            }
            InputTarget::Dicom(focus) if app.input_mode == crate::app::InputMode::DicomToBids => {
                // DICOM series area
                let offset = io_field_count + 2; // blank + header
                match focus {
                    crate::app::DicomFocus::Series(i) => {
                        // Mirror the unique-series render layout: an acq header line whenever
                        // the acquisition name changes, then one line per unique series.
                        let session = app.dicom_state.session.as_ref()?;
                        let mut line_offset = offset;
                        let mut current_acq: Option<String> = None;
                        for (current_flat, g) in session.unique_series().iter().enumerate() {
                            if current_acq.as_deref() != Some(g.acq_name.as_str()) {
                                current_acq = Some(g.acq_name.clone());
                                line_offset += 1; // acq header
                            }
                            if current_flat == i {
                                return Some(line_offset);
                            }
                            line_offset += 1;
                        }
                        None
                    }
                    crate::app::DicomFocus::KeepUids => dicom_lines.map(|(keep, _)| keep),
                    crate::app::DicomFocus::ConvertButton => dicom_lines.map(|(_, convert)| convert),
                }
            }
            _ => None,
        }
    };
    let focused_line = if in_io || !is_example { line_of(app.input_target()) } else { None };
    // Everything a click can focus, with its line.
    let mut targets: Vec<InputTarget> = (0..io_field_count).map(InputTarget::Io).collect();
    if is_bids && app.filter_state.tree.as_ref().is_some_and(|t| !t.subjects.is_empty()) && !app.form.bids_dir.trim().is_empty() {
        use crate::app::FilterFocus as F;
        targets.extend([F::Include, F::Exclude, F::OrientationGroup].map(InputTarget::Filter));
        targets.extend((0..app.filter_state.visible_rows().len()).map(|i| InputTarget::Filter(F::TreeNode(i))));
        targets.push(InputTarget::Filter(F::NumEchoes));
    } else if is_nifti {
        targets.extend(app.nifti_state.focusable_items().into_iter().map(InputTarget::Nifti));
    } else if let (crate::app::InputMode::DicomToBids, Some(session)) = (app.input_mode, &app.dicom_state.session) {
        targets.extend((0..session.unique_series().len()).map(|i| InputTarget::Dicom(crate::app::DicomFocus::Series(i))));
        targets.push(InputTarget::Dicom(crate::app::DicomFocus::KeepUids));
        targets.push(InputTarget::Dicom(crate::app::DicomFocus::ConvertButton));
    }
    let target_lines: Vec<(usize, InputTarget)> = targets.into_iter().filter_map(|t| Some((line_of(t)?, t))).collect();

    // How far the IO fields are scrolled: the example list scrolls on its own, under them.
    let io_scroll = match app.input_mode {
        crate::app::InputMode::Bids => {
            render_scrollable(f, content_area, lines, &mut app.form_scroll_offset, focused_line);
            app.form_scroll_offset
        }
        crate::app::InputMode::NIfTI => {
            render_scrollable(f, content_area, lines, &mut app.nifti_state.scroll_offset, focused_line);
            app.nifti_state.scroll_offset
        }
        crate::app::InputMode::DicomToBids => {
            render_scrollable(f, content_area, lines, &mut app.dicom_state.scroll_offset, focused_line);
            app.dicom_state.scroll_offset
        }
        crate::app::InputMode::Example => {
            let focused = in_io.then_some(app.active_field);
            draw_example_picker(f, &mut app.example_state, &app.hits, content_area, lines, in_io, focused)
        }
    };
    // The example picker records its own lines (its list scrolls apart from the IO fields).
    if !is_example {
        add_line_hits(&app.hits, content_area, io_scroll, target_lines);
    }

    f.render_widget(Paragraph::new(help), help_area);

    // Set cursor if editing IO text field
    if app.editing && in_io && app.active_field > 0 {
        let scroll = io_scroll;
        let line = app.active_field;
        if line >= scroll && line < scroll + content_area.height as usize {
            let y = content_area.y + (line - scroll) as u16;
            let x = content_area.x + 24 + app.cursor_pos as u16;
            f.set_cursor_position((x, y));
        }
    }
    // Set cursor if editing a filter text field (BIDS mode only): on the focused line, after
    // its label.
    let fs = &app.filter_state;
    let editing_at = if fs.include_editing {
        Some(("  Include:  ".len(), fs.include_cursor))
    } else if fs.exclude_editing {
        Some(("  Exclude:  ".len(), fs.exclude_cursor))
    } else if fs.orientation_editing {
        Some((ORIENTATIONS_LABEL.len(), fs.orientation_cursor))
    } else if fs.num_echoes_editing {
        Some((14, fs.num_echoes_cursor))
    } else {
        None
    };
    if let (true, Some((label, cursor)), Some(line)) = (is_bids, editing_at, focused_line) {
        let scroll = app.form_scroll_offset;
        if line >= scroll && line < scroll + content_area.height as usize {
            let y = content_area.y + (line - scroll) as u16;
            let x = content_area.x + (label + cursor) as u16;
            f.set_cursor_position((x, y));
        }
    }

    // Set cursor if editing NIfTI parameter fields (EchoTimes, FieldStrength, B0Direction)
    if is_nifti && app.nifti_state.editing && !in_io {
        let param_focused = matches!(
            app.nifti_state.focus,
            crate::app::NiftiFocus::EchoTimes
            | crate::app::NiftiFocus::FieldStrength
            | crate::app::NiftiFocus::B0Direction
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
