//! The DICOM to BIDS section: the series tree and conversion.

use super::*;

/// Render the DICOM series classification section (used within the unified input tab).
/// Returns the lines of the keep-UIDs toggle and the Convert button, when they are drawn.
pub(super) fn draw_dicom_series_section(
    ds: &crate::app::DicomConvertState,
    lines: &mut Vec<Line<'_>>,
    in_io: bool,
) -> Option<(usize, usize)> {
    if ds.dicom_dir.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "  Set DICOM directory above first",
            Style::default().fg(Color::DarkGray),
        )));
        return None;
    }

    if ds.scan_status == crate::app::ScanStatus::Scanning {
        let n = ds.scan_files_examined();
        lines.push(Line::from(Span::styled(
            format!("  Scanning DICOM directory... ({} files examined)", n),
            Style::default().fg(Color::Yellow),
        )));
        return None;
    }

    if let Some(ref err) = ds.scan_error {
        lines.push(Line::from(Span::styled(
            format!("  Scan error: {}", err),
            Style::default().fg(Color::Red),
        )));
        return None;
    }

    let Some(ref session) = ds.session else {
        lines.push(Line::from(Span::styled(
            "  No DICOM files found in directory",
            Style::default().fg(Color::DarkGray),
        )));
        return None;
    };

    lines.push(Line::from(Span::styled(
        "  -- Series classification --",
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
    )));
    series_lines(ds, session, in_io, lines);

    // What the scan found besides the series: problems, and what it left out.
    let report = &session.report;
    if !report.warnings.is_empty() {
        lines.push(Line::from(""));
        for w in &report.warnings {
            lines.push(Line::from(Span::styled(format!("  ⚠ {w}"), Style::default().fg(Color::Yellow))));
        }
    }
    if !report.notes.is_empty() || !report.skipped.is_empty() || report.not_dicom > 0 {
        lines.push(Line::from(""));
        for n in &report.notes {
            lines.push(Line::from(Span::styled(format!("  {n}"), Style::default().fg(Color::DarkGray))));
        }
        for s in &report.skipped {
            lines.push(Line::from(Span::styled(format!("  Not an image, not converted: {s}"), Style::default().fg(Color::DarkGray))));
        }
        if report.not_dicom > 0 {
            lines.push(Line::from(Span::styled(
                format!("  {} file(s) in the folder are not DICOM and are ignored", report.not_dicom),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    let (keep_line, convert_line) = conversion_lines(ds, in_io, lines);
    Some((keep_line, convert_line))
}

/// The series of `session`, one row per unique series under a header per acquisition.
fn series_lines(ds: &crate::app::DicomConvertState, session: &qsmxt_engine::dicom::DicomSession, in_io: bool, lines: &mut Vec<Line<'_>>) {
    // One row per UNIQUE series (shared across subjects), not per per-subject instance.
    let groups = session.unique_series();
    let plans = qsmxt_engine::dicom::plan_session(session);
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
        let series_focused = !in_io && ds.focus == crate::app::DicomFocus::Series(flat_idx);
        let type_label = series.series_type.label();
        let echo_info = match g.echo_times.as_slice() {
            [] => String::new(),
            [te] => format!(" TE={:.1}ms", te),
            tes => format!(" {}×TEs=[{:.1}…{:.1}]ms", tes.len(), tes[0], tes[tes.len() - 1]),
        };
        let echo_info = if series.coil_type == qsmxt_engine::dicom::CoilType::Uncombined {
            format!("{} [uncombined ×{}]", echo_info, series.coil_groups.len())
        } else {
            echo_info
        };
        let target = plans
            .iter()
            .find(|p| p.r == g.refs[0])
            .map(|p| format!("  → {}", qsmxt_io::dicom::unique_target_label(g, &p.target)))
            .unwrap_or_default();
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
            qsmxt_engine::dicom::SeriesType::Skip => Style::default().fg(Color::DarkGray),
            qsmxt_engine::dicom::SeriesType::Phase => Style::default().fg(Color::Cyan),
            qsmxt_engine::dicom::SeriesType::Magnitude => Style::default().fg(Color::Green),
            qsmxt_engine::dicom::SeriesType::T1w | qsmxt_engine::dicom::SeriesType::Inversion | qsmxt_engine::dicom::SeriesType::T1map => Style::default().fg(Color::Magenta),
            qsmxt_engine::dicom::SeriesType::Real | qsmxt_engine::dicom::SeriesType::Imaginary => Style::default().fg(Color::Cyan),
            qsmxt_engine::dicom::SeriesType::Extra => Style::default().fg(Color::DarkGray),
            _ => Style::default().fg(Color::Blue),
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
                Span::styled(target, Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    format!("    {:30}", format!("{}{}", series.description, echo_info)),
                    style,
                ),
                Span::styled(format!("[{}]", type_label), type_style),
                Span::styled(files_label, Style::default().fg(Color::DarkGray)),
                Span::styled(target, Style::default().fg(Color::DarkGray)),
            ])
        };
        lines.push(line);
        if series_focused {
            if let Some(note) = &series.note {
                lines.push(Line::from(Span::styled(format!("      {note}"), Style::default().fg(Color::DarkGray))));
            }
        }
    }
}

/// dcm2niix's availability, the keep-UIDs toggle, the Convert button and the conversion log.
/// Returns the lines of the toggle and the button.
fn conversion_lines(ds: &crate::app::DicomConvertState, in_io: bool, lines: &mut Vec<Line<'_>>) -> (usize, usize) {
    // dcm2niix availability indicator (uses the cached resolution)
    lines.push(Line::from(""));
    match ds.dcm2niix_path() {
        Some(path) => {
            let is_bundled = qsmxt_io::dicom::convert::qsmxt_bin_dir()
                .map(|dir| path.starts_with(&dir))
                .unwrap_or(false);
            let (label, color) = if is_bundled {
                (
                    format!("  dcm2niix: bundled {}", qsmxt_io::dicom::convert::DCM2NIIX_BUNDLED_VERSION),
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

    // Whether the study's UIDs stay in the sidecars (dcm2niix -ba o).
    lines.push(Line::from(""));
    let keep_line = lines.len();
    let keep_focused = !in_io && ds.focus == crate::app::DicomFocus::KeepUids;
    lines.push(Line::from(vec![
        Span::styled("  Keep DICOM UIDs: ", if keep_focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }),
        if ds.keep_uids { Span::styled("[x]", Style::default().fg(Color::Green)) } else { Span::styled("[ ]", Style::default().fg(Color::Gray)) },
        Span::styled(if ds.keep_uids { "  links the dataset to the original study: don't share it outside your site" } else { "" },
            Style::default().fg(Color::Yellow)),
    ]));

    // Convert button
    lines.push(Line::from(""));
    let convert_line = lines.len();
    let convert_focused = !in_io && ds.focus == crate::app::DicomFocus::ConvertButton;
    let convert_style = if convert_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let status_span = match ds.convert_status {
        crate::app::ConvertStatus::Idle => Span::styled("  Ready", Style::default().fg(Color::DarkGray)),
        crate::app::ConvertStatus::Converting => Span::styled("  Converting...", Style::default().fg(Color::Yellow)),
        crate::app::ConvertStatus::Done => Span::styled("  Done!", Style::default().fg(Color::Green)),
        crate::app::ConvertStatus::Error => Span::styled("  Error (see log)", Style::default().fg(Color::Red)),
    };
    lines.push(Line::from(vec![
        Span::styled("  [ Convert to BIDS ]", convert_style),
        status_span,
    ]));

    // Conversion log
    if !ds.convert_log.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  -- Conversion log --",
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
    (keep_line, convert_line)
}
