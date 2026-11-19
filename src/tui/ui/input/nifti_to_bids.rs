//! The NIfTI to BIDS section: input files and acquisition details.

use super::*;

pub(super) fn draw_nifti_section(
    ns: &crate::tui::app::NiftiState,
    in_io: bool,
    lines: &mut Vec<Line<'_>>,
) {
    let focused = |f: &crate::tui::app::NiftiFocus| -> bool { !in_io && ns.focus == *f };

    let label_style = |f: &crate::tui::app::NiftiFocus| -> Style {
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
        format!("  -- Magnitude files ({}) --", ns.magnitude_files.len()),
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));

    // Add magnitude button
    let add_mag_focus = crate::tui::app::NiftiFocus::AddMagnitude;
    if ns.editing && ns.adding_to == Some(qsmxt_io::nifti::convert::NiftiPartType::Magnitude) {
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
        let f = crate::tui::app::NiftiFocus::MagFile(i);
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
        format!("  -- Phase files ({}) --", ns.phase_files.len()),
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));

    // Add phase button
    let add_phase_focus = crate::tui::app::NiftiFocus::AddPhase;
    if ns.editing && ns.adding_to == Some(qsmxt_io::nifti::convert::NiftiPartType::Phase) {
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
        let f = crate::tui::app::NiftiFocus::PhaseFile(i);
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
    let et_focus = crate::tui::app::NiftiFocus::EchoTimes;
    let et_val = if ns.echo_times.is_empty() && !(ns.editing && focused(&et_focus)) {
        Span::styled("(required)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
    } else {
        Span::styled(ns.echo_times.clone(), Style::default().fg(Color::Cyan))
    };
    lines.push(Line::from(vec![
        Span::styled(format!("  {:22}", "Echo times (ms):"), label_style(&et_focus)),
        et_val,
    ]));

    // Field Strength
    let fs_focus = crate::tui::app::NiftiFocus::FieldStrength;
    let fs_val = if ns.field_strength.is_empty() && !(ns.editing && focused(&fs_focus)) {
        Span::styled("(required)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
    } else {
        Span::styled(ns.field_strength.clone(), Style::default().fg(Color::Cyan))
    };
    lines.push(Line::from(vec![
        Span::styled(format!("  {:22}", "Field strength (T):"), label_style(&fs_focus)),
        fs_val,
    ]));

    // B0 Direction
    let b0_focus = crate::tui::app::NiftiFocus::B0Direction;
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
    let convert_focus = crate::tui::app::NiftiFocus::ConvertButton;
    let btn_style = if focused(&convert_focus) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        match ns.convert_status {
            crate::tui::app::ConvertStatus::Done => Style::default().fg(Color::Green),
            crate::tui::app::ConvertStatus::Error => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::White),
        }
    };
    let btn_label = match ns.convert_status {
        crate::tui::app::ConvertStatus::Idle => "  [ Convert to BIDS ]",
        crate::tui::app::ConvertStatus::Converting => "  [ Converting... ]",
        crate::tui::app::ConvertStatus::Done => "  [ Done! ]",
        crate::tui::app::ConvertStatus::Error => "  [ Error ]",
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
