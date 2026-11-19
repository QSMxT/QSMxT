//! The row-based tabs: the settings, their sidebar and help beside them.

use super::*;

/// The most rows a setting's reference values take under its help.
const MAX_REFERENCE_ROWS: usize = 3;


/// A picker's value drawn as a button, `[ MEDI ▾ ]`: Enter opens the list.
/// A value's style: cyan on the focused row, grey elsewhere.
fn value_style(focused: bool) -> Style {
    Style::default().fg(if focused { Color::Cyan } else { Color::Gray })
}

/// A mask row's number: the text being typed while it is edited, else `value`.
fn mask_number(app: &App, value: String, focused: bool) -> Span<'static> {
    let text = if focused && app.pipeline_state.mask_value_editing { app.pipeline_state.mask_value_buf.clone() } else { value };
    Span::styled(text, value_style(focused))
}

/// The sidebar on the left of a panel (`area`, `inner` inside its border), its divider meeting
/// the border; returns the room beside it.
pub(super) fn draw_sidebar_beside(f: &mut Frame, app: &App, area: Rect, inner: Rect) -> Rect {
    let sw = crate::tui::tabs::SIDEBAR_WIDTH.min(inner.width);
    crate::tui::tabs::draw_sidebar(f, app, Rect { width: sw, ..inner });
    let x = inner.x + sw - 1;
    // (Not where the title is written over the top border.)
    let buf = f.buffer_mut();
    let faint = buf[(area.x, area.y)].fg;
    if buf[(x, area.y)].symbol() == "─" { buf[(x, area.y)].set_symbol("┬").set_fg(faint); }
    if buf[(x, area.bottom() - 1)].symbol() == "─" { buf[(x, area.bottom() - 1)].set_symbol("┴").set_fg(faint); }
    Rect { x: inner.x + sw + 1, width: inner.width.saturating_sub(sw + 1), ..inner }
}

/// Columns the settings keep before their help moves beside them: the widest label column (34),
/// a choice's button, and a short rule marker.
const SETTINGS_COLUMNS: u16 = 76;
/// The narrowest help column worth having.
const HELP_COLUMNS_MIN: u16 = 36;
/// The widest help column; more room goes to the settings.
const HELP_COLUMNS_MAX: u16 = 64;

/// Where the help goes beside the settings in `inner`, when it is wide enough for both.
fn side_help(inner: Rect) -> Option<Rect> {
    let spare = inner.width.checked_sub(SETTINGS_COLUMNS + 1)?;
    (spare >= HELP_COLUMNS_MIN).then(|| {
        let w = spare.min(HELP_COLUMNS_MAX);
        Rect { x: inner.right() - w, width: w, ..inner }
    })
}

/// A row-based tab (all but Input and Methods): its sub-tabs, then one named row per setting.
pub(super) fn draw_rows_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let block = crate::tui::tabs::panel(app);
    let mut inner = block.inner(area);
    f.render_widget(block, area);
    if app.sidebar() {
        inner = draw_sidebar_beside(f, app, area, inner);
    }
    // Wide enough: the help in a column of its own on the right, beside the settings.
    let side = side_help(inner);
    if let Some(help) = side {
        inner = Rect { width: inner.width - help.width - 1, ..inner };
        // A rule like the sidebar's, joined to the panel's border.
        let x = help.x - 1;
        let buf = f.buffer_mut();
        let faint = buf[(area.x, area.y)].fg;
        for y in inner.y..inner.bottom() {
            buf[(x, y)].set_symbol("│").set_fg(faint);
        }
        if buf[(x, area.y)].symbol() == "─" { buf[(x, area.y)].set_symbol("┬").set_fg(faint); }
        if buf[(x, area.bottom() - 1)].symbol() == "─" { buf[(x, area.bottom() - 1)].set_symbol("┴").set_fg(faint); }
    }

    // Notes wrap to the settings' width.
    app.note_width.set(inner.width);
    // Build lines and collect state from pipeline_state before mutable borrow
    let rows = app.rows();
    let focusable = crate::tui::app::focusable_indices(&rows);
    let ps_focus = app.focus();
    let ps_editing = app.pipeline_state.editing;
    // The mask recipes the mask builder's rows show, read once for the frame.
    let mask = app.mask();
    let ps_cursor = app.pipeline_state.cursor;
    // With focus on the sub-tab strip no row is focused; the help line says how to get in.
    let on_strip = app.on_subtabs();
    // The pipeline rules' findings for the form's config: a badge on each setting with one, and
    // the focused setting's reference values under its help.
    let view = app.rule_view();
    let reference = (!on_strip && !app.tab_focus)
        .then(|| focusable.get(ps_focus).and_then(|&i| rows.get(i)))
        .flatten()
        .and_then(|row| App::row_reference(&view, row));

    // The sub-tabs (second tab row), when the tab has more than one; then the form and its help.
    let bar = crate::tui::tabs::subtab_rows(app);

    // One label column per page, wide enough for its longest label (within reason).
    let lw = rows.iter().filter_map(|r| match r {
        PipelineRow::AlgoSelect { label, .. } | PipelineRow::Param { label, .. }
        | PipelineRow::Toggle { label, .. } => Some(label.chars().count() + 2),
        _ => None,
    }).max().unwrap_or(22).clamp(22, 34);

    let mut lines: Vec<Line> = Vec::new();
    let mut focused_help: Option<String> = None;

    let mut focusable_idx = 0;
    for (i, row) in rows.iter().enumerate() {
        let is_focusable = focusable.contains(&i);
        let focused = is_focusable && focusable_idx == ps_focus && !on_strip && !app.tab_focus;
        if is_focusable {
            focusable_idx += 1;
        }

        let line = match row {
            PipelineRow::AlgoSelect { label, field, options, help } => {
                let selected = app.field_select(field);
                let val = options.get(selected).unwrap_or(&"?");
                let picker = crate::tui::recommend::picker_noun(row);
                if focused {
                    focused_help = help.get(selected).map(|s| match &picker {
                        Some(noun) => format!("{s} — Enter: choose {noun}"),
                        None => s.to_string(),
                    });
                }
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let mut spans = vec![Span::styled(format!("  {:lw$}", format!("{}:", label)), label_style)];
                spans.extend(picker_button(val, focused));
                Line::from(spans)
            }
            PipelineRow::Param { label, field, help } => {
                let exact = app.field_text(field).to_string();
                // A long number reads rounded; the field keeps every digit, and the help line and
                // editing show them.
                let rounded = if focused && ps_editing { None } else { compact_number(&exact) };
                if focused {
                    focused_help = Some(match &rounded {
                        Some(_) => format!("{help} (exact value: {exact})"),
                        None => help.to_string(),
                    });
                }
                let val = rounded.unwrap_or(exact);
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
                    let placeholder = app.field_placeholder(field);
                    let style = Style::default().fg(Color::DarkGray);
                    Span::styled(placeholder, if placeholder == "(required)" { style.add_modifier(Modifier::ITALIC) } else { style })
                } else {
                    Span::styled(val, val_style)
                };
                Line::from(vec![
                    Span::styled(format!("  {:lw$}", format!("{}:", label)), label_style),
                    display_val,
                ])
            }
            PipelineRow::Toggle { label, field, help } => {
                let checked = app.field_toggle(field);
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
                    Span::styled(format!("  {:lw$}", format!("{}:", label)), label_style),
                    Span::styled(marker, Style::default().fg(color)),
                ])
            }
            PipelineRow::Separator => Line::from(""),
            PipelineRow::Note { text } => Line::from(Span::styled(
                format!("  {}", text),
                Style::default().fg(Color::Yellow),
            )),
            PipelineRow::MaskSectionHeader { section } => {
                let (two_pass, i) = crate::tui::mask_builder::MaskRecipe::mask_section_slot(*section);
                Line::from(Span::styled(
                    format!("  ── {} {} ──", if two_pass { "Reliable mask" } else { "Mask" }, i + 1),
                    Style::default().fg(Color::DarkGray),
                ))
            }
            PipelineRow::MaskCombineOp { section } => {
                let mode = format!("{}", mask.combine_mode(*section)).to_uppercase();
                let sep = Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD);
                if focused {
                    if focused_help.is_none() {
                        let which = if crate::tui::app::is_two_pass_section(*section) { "reliable mask's" } else { "mask" };
                        focused_help = Some(format!(
                            "How the {which} sections combine: OR = union, AND = intersection — Enter: choose (d removes the next section)"));
                    }
                    let mut spans = vec![Span::styled("  ── COMBINED WITH ", sep)];
                    spans.extend(picker_button(&mode, true));
                    spans.push(Span::styled(" ──", sep));
                    Line::from(spans)
                } else {
                    Line::from(Span::styled(format!("  ── COMBINED WITH {} ──", mode), sep))
                }
            }
            PipelineRow::MaskCombinedHeader => {
                Line::from(Span::styled(
                    "  ── On the combined mask ──",
                    Style::default().fg(Color::DarkGray),
                ))
            }
            PipelineRow::MaskTwoPassHeader => {
                Line::from(Span::styled(
                    "  ── Reliable-phase mask (holes left unfilled) ──",
                    Style::default().fg(Color::DarkGray),
                ))
            }
            PipelineRow::MaskOpInput { section } => {
                let Some(input) = mask.mask_section(*section).map(|s| &s.input) else { continue };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                if focused && focused_help.is_none() {
                    let i = crate::tui::app::mask_schema().inputs.iter().find(|i| i.value == *input);
                    focused_help = Some(format!("{} — Enter: choose input", i.map_or("Masking input source", |i| i.help)));
                }
                let mut spans = vec![Span::styled(format!("  {:lw$}", "Input:"), label_style)];
                spans.extend(picker_button(&input.to_string(), focused));
                Line::from(spans)
            }
            PipelineRow::MaskOpGenerator { section } => {
                let Some(gen) = mask.mask_section(*section).map(|s| &s.generator) else { continue };
                let algo_name = match gen {
                    qsmxt_engine::MaskOp::Threshold { .. } => "threshold",
                    qsmxt_engine::MaskOp::Bet { .. } => "bet",
                    qsmxt_engine::MaskOp::HdBet { .. } => "hd-bet",
                    qsmxt_engine::MaskOp::Rs2Net { .. } => "rs2-net",
                    qsmxt_engine::MaskOp::MindGrab { .. } => "mindgrab",
                    _ => "?",
                };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                if focused && focused_help.is_none() {
                    focused_help = Some(format!("{} — Enter: choose algorithm (threshold, BET, HD-BET, MindGrab or RS2-Net)",
                        crate::tui::app::mask_step_of(gen).help));
                }
                let mut spans = vec![Span::styled(format!("  {:lw$}", "Algorithm:"), label_style)];
                spans.extend(picker_button(algo_name, focused));
                Line::from(spans)
            }
            PipelineRow::MaskOpGeneratorParam { section } => {
                let Some(gen) = mask.mask_section(*section).map(|s| &s.generator) else { continue };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                // How the value is set: chosen from a list, typed, switched, or fixed.
                enum Kind { Choice, Number, Switch, Fixed }
                let (label, val, help, kind) = match gen {
                    qsmxt_engine::MaskOp::Threshold { method, .. } => {
                        let method_name = match method {
                            qsmxt_engine::MaskThresholdMethod::Otsu => "otsu",
                            qsmxt_engine::MaskThresholdMethod::Fixed => "fixed",
                            qsmxt_engine::MaskThresholdMethod::Percentile => "percentile",
                            qsmxt_engine::MaskThresholdMethod::StiGui => "sti-gui",
                        };
                        ("Method:", method_name.to_string(), format!("{} — Enter: choose method", crate::tui::app::mask_step_of(gen).help), Kind::Choice)
                    }
                    qsmxt_engine::MaskOp::Bet { fractional_intensity, .. } => {
                        let p = crate::tui::app::mask_param("bet", "fractional_intensity");
                        ("Frac. Intensity:", format!("{:.2}", fractional_intensity), format!("{}: {}, from 0 to 1 (Enter to edit, +/- to nudge)", p.label, p.help), Kind::Number)
                    }
                    qsmxt_engine::MaskOp::HdBet { patch, .. } => {
                        let val = match patch {
                            None => "whole volume".to_string(),
                            Some(p) => format!("{}x{}x{} window{}", p[0], p[1], p[2],
                                if *p == qsmxt_engine::hd_bet_low_memory_patch() { " (low memory)" } else { "" }),
                        };
                        ("Inference:", val, hd_bet_inference_help(), Kind::Choice)
                    }
                    // RS2-Net's graph is traced at one fixed patch, so there is nothing to choose.
                    qsmxt_engine::MaskOp::Rs2Net { tta, .. } => {
                        ("Augmentation:", if *tta { "mirroring TTA".to_string() } else { "none".to_string() },
                         crate::tui::app::mask_step("rs2-net").help.to_string(), Kind::Fixed)
                    }
                    qsmxt_engine::MaskOp::MindGrab { tta } => {
                        ("Augmentation:", format!("[{}] left-right flip TTA", if *tta { "x" } else { " " }),
                         format!("{} (Space or Enter to switch)", crate::tui::app::mask_param("mindgrab", "tta").help), Kind::Switch)
                    }
                    _ => ("?:", "?".to_string(), String::new(), Kind::Fixed),
                };
                if focused && focused_help.is_none() {
                    focused_help = Some(help);
                }
                let mut spans = vec![Span::styled(format!("  {:lw$}", label), label_style)];
                match kind {
                    Kind::Choice => spans.extend(picker_button(&val, focused)),
                    Kind::Number => spans.push(mask_number(app, val, focused)),
                    Kind::Switch | Kind::Fixed => spans.push(Span::styled(val, value_style(focused))),
                }
                Line::from(spans)
            }
            PipelineRow::MaskOpThresholdValue { section } => {
                let Some(gen) = mask.mask_section(*section).map(|s| &s.generator) else { continue };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let (label, val) = match gen {
                    qsmxt_engine::MaskOp::Threshold { method: qsmxt_engine::MaskThresholdMethod::Fixed, value } =>
                        ("Value:", value.map(|v| format!("{}", v)).unwrap_or("0.5".to_string())),
                    qsmxt_engine::MaskOp::Threshold { method: qsmxt_engine::MaskThresholdMethod::Percentile, value } =>
                        ("Percentile:", value.map(|v| format!("{}", v)).unwrap_or("75".to_string())),
                    _ => ("Value:", "?".to_string()),
                };
                if focused && focused_help.is_none() {
                    let kind = crate::tui::app::mask_step_of(gen).kind;
                    let help = crate::tui::app::mask_step(kind).params.first().map_or("", |p| p.help);
                    focused_help = Some(format!("{help} (Enter to edit, Esc to cancel)"));
                }
                Line::from(vec![
                    Span::styled(format!("  {:lw$}", label), label_style),
                    mask_number(app, val, focused),
                ])
            }
            PipelineRow::MaskOpHdBetStep { section } => {
                let Some(gen) = mask.mask_section(*section).map(|s| &s.generator) else { continue };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let step = match gen {
                    qsmxt_engine::MaskOp::HdBet { tile_step, .. } => *tile_step,
                    _ => qsmxt_engine::hd_bet_default_tile_step(),
                };
                // Named and valued as the stride throughout — same word and same number as
                // qsm-core's `tile_step`, the `step=` in the op string, and nnU-Net's
                // `tile_step_size`. No translation for a reader to undo.
                let val = format!("{:.3}{}", step,
                    if (step - qsmxt_engine::hd_bet_default_tile_step()).abs() < f64::EPSILON { " (default)" } else { "" });
                if focused && focused_help.is_none() {
                    focused_help = Some(format!("{} — Enter: choose", crate::tui::app::mask_param("hd-bet", "tile_step").help));
                }
                let mut spans = vec![Span::styled(format!("  {:lw$}", "Patch step:"), label_style)];
                spans.extend(picker_button(&val, focused));
                Line::from(spans)
            }
            PipelineRow::MaskOpBetVoxelScale { section } => {
                let Some(gen) = mask.mask_section(*section).map(|s| &s.generator) else { continue };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let scale = match gen {
                    qsmxt_engine::MaskOp::Bet { voxel_scale, .. } => *voxel_scale,
                    _ => qsmxt_engine::bet_default_voxel_scale(),
                };
                let default = (scale - qsmxt_engine::bet_default_voxel_scale()).abs() < f64::EPSILON;
                let val = format!("{}x{}", scale, if default { " (acquired geometry)" } else { "" });
                if focused && focused_help.is_none() {
                    focused_help = Some(format!("{} — Enter: choose", crate::tui::app::mask_param("bet", "voxel_scale").help));
                }
                let mut spans = vec![Span::styled(format!("  {:lw$}", "Voxel scale:"), label_style)];
                spans.extend(picker_button(&val, focused));
                Line::from(spans)
            }
            PipelineRow::MaskOpEntry { section, index } => {
                let Some(op) = mask.mask_refinements(*section).and_then(|r| r.get(*index)) else {
                    lines.push(Line::from(""));
                    continue;
                };
                let (op_type, op_val) = crate::tui::mask_builder::MaskRecipe::mask_op_label_value(op);
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                if focused && focused_help.is_none() {
                    focused_help = Some(crate::tui::mask_builder::MaskRecipe::mask_op_help(op));
                }
                let number_style = if focused { label_style } else { Style::default().fg(Color::DarkGray) };
                let mut spans = vec![
                    Span::styled(format!("  {:3}", format!("{}.", index + 1)), number_style),
                    Span::styled(format!("{:19}", format!("{}:", op_type)), label_style),
                ];
                if matches!(op, qsmxt_engine::MaskOp::Threshold { .. }) {
                    spans.extend(picker_button(&op_val, focused));
                } else {
                    spans.push(mask_number(app, op_val, focused));
                }
                Line::from(spans)
            }
            PipelineRow::MaskOpSignalErodeParam { section, index, param } => {
                let Some((label, value)) = mask.signal_erode_param(*section, *index, *param) else {
                    lines.push(Line::from(""));
                    continue;
                };
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                // Indented one level past the step row, so it reads as that step's parameter.
                let label = format!("      {:18}", format!("{label}:"));
                if focused && focused_help.is_none() {
                    focused_help = Some(format!("{} (Enter to edit, +/- to nudge)", crate::tui::app::signal_erode_params()[*param].1));
                }
                Line::from(vec![Span::styled(label, label_style), mask_number(app, value, focused)])
            }
            PipelineRow::MaskOpAddStep { .. } => {
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                if focused && focused_help.is_none() {
                    focused_help = Some("Enter: choose a step to add (on a step: d deletes it, Ctrl+↑/↓ reorder)".to_string());
                }
                Line::from(Span::styled("  + Add step...", label_style))
            }
            PipelineRow::MaskOpAddSection { two_pass } => {
                let label_style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                if focused && focused_help.is_none() {
                    focused_help = Some(if *two_pass {
                        "Enter to add a section to the reliable-phase mask".to_string()
                    } else {
                        "Enter to add a new OR'd mask section".to_string()
                    });
                }
                Line::from(Span::styled(
                    if *two_pass { "  + Add reliable mask..." } else { "  + Add mask..." },
                    label_style,
                ))
            }
            PipelineRow::Summary { text } => Line::from(Span::styled(
                format!("      {text}"),
                Style::default().fg(Color::DarkGray),
            )),
            PipelineRow::AdvancedFooter { hidden } => {
                if focused {
                    focused_help = Some("Show every setting (Enter or A); A again shows the basic ones".to_string());
                }
                let style = if focused {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(Span::styled(
                    format!("  + {hidden} advanced setting{} · A", if *hidden == 1 { "" } else { "s" }),
                    style,
                ))
            }
            PipelineRow::RunButton => {
                let (label, help) = app.run_action();
                let label = format!(" ▶ {label} ");
                let key = Style::default().fg(Color::DarkGray);
                if focused {
                    focused_help = Some(help.to_string());
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(label, Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled("  Enter or F5", key),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(label, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD | Modifier::REVERSED)),
                        Span::styled("  F5", key),
                    ])
                }
            }
        };
        let line = match App::row_marker(&view, row) {
            Some(m) => {
                let mut line = line;
                let (mark, color) = if m.actionable { ("⚑", Color::Yellow) } else { ("ⓘ", Color::DarkGray) };
                // A few words beside the setting (whose rule, and its value while short); `!`
                // shows the rest. On a narrow form they give way (…), and the `!` hint shows
                // only where it fits after them (the key bar has `!:Advice` too). One column is
                // left for the form's scrollbar.
                let hint = " (! for advice)";
                let room = (inner.width as usize).saturating_sub(line.width() + 3 + 1);
                let short_len = m.short.chars().count();
                let text = if short_len <= room {
                    m.short.clone()
                } else if room < 2 {
                    String::new() // the mark alone
                } else {
                    let mut t: String = m.short.chars().take(room.saturating_sub(1)).collect();
                    t.push('…');
                    t
                };
                let marked = if text.is_empty() { format!(" {mark}") } else { format!(" {mark} {text}") };
                line.spans.push(Span::styled(marked, Style::default().fg(color)));
                if focused && short_len + hint.chars().count() <= room {
                    line.spans.push(Span::styled(hint, Style::default().fg(Color::Yellow)));
                }
                line
            }
            None => line,
        };
        lines.push(line);
    }
    if on_strip && app.sidebar() {
        focused_help = Some("↑/↓ choose a section; → or Enter to go to its settings (← comes back here); ↑ from the first to the tabs".to_string());
    } else if on_strip {
        focused_help = Some("←/→ choose a section; ↓ or Enter to go to its settings (← comes back here); ↑ to the tabs".to_string());
    }

    // Determine which line is focused for auto-scroll. While the sections have focus, none: the
    // view stays where it is (at the top after a change of section, or where the wheel put it).
    let focused_line = if on_strip { None } else { focusable.get(ps_focus).copied() };

    // The help, wrapped, and under it the reference values of the setting: at least two rows of
    // help so the form does not jump between short ones, at most half the panel together.
    let help_width = side.map_or(inner.width, |r| r.width - 1);
    let help = focused_help.as_deref().map_or_else(Vec::new, |h| help_lines(h, help_width, Color::DarkGray));
    let mut refs = reference.as_deref().map_or_else(Vec::new, |r| help_lines(r, help_width, Color::Cyan));
    // Sources can run long, and `!` shows them in full.
    if refs.len() > MAX_REFERENCE_ROWS {
        refs.truncate(MAX_REFERENCE_ROWS);
        refs[MAX_REFERENCE_ROWS - 1].spans.push(Span::styled(" …", Style::default().fg(Color::Cyan)));
    }
    let help_rows = (help.len().max(2) + refs.len()) as u16;
    let help_rows = if side.is_some() { 0 } else { help_rows.min((inner.height.saturating_sub(bar) / 2).max(2)) };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(bar), Constraint::Min(4), Constraint::Length(u16::from(side.is_none())), Constraint::Length(help_rows)])
        .split(inner);
    if bar > 0 {
        crate::tui::tabs::draw_subtabs(f, app, chunks[0]);
    }
    let form_area = chunks[1];
    help_divider(f, chunks[2]);
    let help_area = chunks[3];

    let scroll_offset = &mut app.pipeline_state.scroll_offset;
    render_scrollable(f, form_area, lines, scroll_offset, focused_line);
    let scroll = *scroll_offset;
    app.hits.add(form_area, crate::tui::mouse::Hit::Rows { scroll });

    // Render help text, and under it the reference values of the setting
    let mut text = help;
    if text.len() < 2 && !refs.is_empty() {
        text.resize(2, Line::from(""));
    }
    text.extend(refs);
    match side {
        Some(r) => f.render_widget(Paragraph::new(text), Rect { x: r.x + 1, width: r.width - 1, ..r }),
        None => f.render_widget(Paragraph::new(text), help_area),
    }

    // Set cursor if editing a param or threshold value
    if ps_editing || app.pipeline_state.mask_value_editing {
        if let Some(&row_idx) = focusable.get(ps_focus) {
            if row_idx >= scroll && row_idx < scroll + form_area.height as usize {
                let y = form_area.y + (row_idx - scroll) as u16;
                // The value starts after the 2-char row indent + the label field, which the render
                // left-pads to a *minimum* of 22 columns (`format!("  {:lw$}", "label:")`) but lets
                // grow for longer labels — so a wide label (e.g. the relaxometry constants) pushes
                // the value right. Match that here instead of assuming a fixed 24.
                let label_cols = match rows.get(row_idx) {
                    Some(PipelineRow::Param { label, .. }) => (label.chars().count() + 1).max(lw),
                    // A refinement step's number column and its settings' are fixed (`  NN.` + 19,
                    // or 6 + 18).
                    Some(PipelineRow::MaskOpEntry { .. } | PipelineRow::MaskOpSignalErodeParam { .. }) => 22,
                    _ => lw,
                };
                let x = form_area.x + 2 + label_cols as u16 + ps_cursor as u16;
                f.set_cursor_position((x, y));
            }
        }
    }
}

/// A number with too many digits to read (a default converted between units, or machine epsilon),
/// to four significant figures, or `None` when it is short enough as it is.
pub(super) fn compact_number(text: &str) -> Option<String> {
    const SHORT: usize = 10;
    if text.len() <= SHORT || !text.contains('.') {
        return None;
    }
    let x: f64 = text.parse().ok()?;
    if x == 0.0 || !x.is_finite() {
        return None;
    }
    if (1e-3..1e6).contains(&x.abs()) {
        let decimals = (3 - x.abs().log10().floor() as i32).max(0) as usize;
        let fixed = format!("{x:.decimals$}");
        Some(if fixed.contains('.') { fixed.trim_end_matches('0').trim_end_matches('.').to_string() } else { fixed })
    } else {
        Some(format!("{x:.2e}"))
    }
}

/// HD-BET's inference choices and what each costs in memory, from the mask schema.
fn hd_bet_inference_help() -> String {
    let choices: Vec<String> = crate::tui::app::mask_schema().hd_bet_inference.iter()
        .map(|c| format!("{}: {}", c.name.to_lowercase(), c.help))
        .collect();
    format!("HD-BET inference — Enter: choose. {}", choices.join("; "))
}
