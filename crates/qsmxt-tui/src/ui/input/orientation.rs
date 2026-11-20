//! The Input tab's multi-orientation block: the pattern, its settings and a live preview.

use super::*;

/// The multi-orientation block of the Input tab: the pattern, its settings, and a live
/// preview of what it currently matches.
///
/// One function produces the lines and, by its length, their count — the focused-line
/// arithmetic below the block depends on that height, and computing it twice is how such
/// things drift out of sync.
///
/// The preview is the reason this is a live field rather than a config key. A grouping
/// pattern that matches the wrong runs does not error: it silently reconstructs the wrong
/// set, or matches nothing and falls back to ordinary single-orientation processing. Showing
/// the groups as they are typed is what makes that visible.
/// The orientation pattern's label, which its cursor follows.
pub(super) const ORIENTATIONS_LABEL: &str = "  Orientations: ";

/// How many orientations a group needs for the chosen reconstruction: STI six, COSMOS two.
pub(super) fn orientations_needed(algorithm: usize) -> usize {
    if algorithm == 1 { 6 } else { 2 }
}

pub(super) fn orientation_block(app: &App, in_io: bool) -> Vec<Line<'static>> {
    use crate::app::FilterFocus;

    let fs = &app.filter_state;
    let focused = |f: FilterFocus| !in_io && fs.focus == f;
    let label_style = |f: FilterFocus| {
        if focused(f) {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    };

    let mut lines = Vec::new();

    let listed = qsmxt_io::bids::orientation::listed_groups(&fs.orientation_group).filter(|_| !fs.orientation_editing);
    let value = if fs.orientation_group.is_empty() && !fs.orientation_editing {
        Span::styled("(off: type a pattern, or ←/→ on runs below)", Style::default().fg(Color::DarkGray))
    } else if listed.is_some() {
        Span::styled("chosen with ←/→ in the runs below", Style::default().fg(Color::Cyan))
    } else {
        Span::styled(fs.orientation_group.clone(), Style::default().fg(Color::Cyan))
    };
    lines.push(Line::from(vec![
        Span::styled(ORIENTATIONS_LABEL, label_style(FilterFocus::OrientationGroup)),
        value,
    ]));

    if !fs.multi_orientation_on() {
        return lines;
    }

    let algorithm = app.field_select("multi_orientation.algorithm");
    let algo = app.form.settings.get("multi_orientation.algorithm").as_str().unwrap_or_default().to_uppercase();

    let preview = fs.orientation_preview(&app.field_text("multi_orientation.reference"));
    if let Some(err) = preview.error {
        lines.push(Line::from(Span::styled(
            format!("    ✗ {err}"),
            Style::default().fg(Color::Red),
        )));
        return lines;
    }

    if preview.groups.is_empty() {
        lines.push(Line::from(Span::styled(
            if listed.is_some() {
                "    ✗ no orientation group yet — give at least 2 runs of one session the same group"
            } else {
                "    ✗ no orientation groups — the pattern matches fewer than 2 runs per session"
            },
            Style::default().fg(Color::Red),
        )));
        return lines;
    }

    // One line for all the groups: each run's group is marked against it in the tree. STI needs
    // six orientations and COSMOS two; flagging a short group here saves a run that would only
    // fail once the field maps had been computed.
    let need = orientations_needed(algorithm);
    let sizes: Vec<usize> = preview.groups.iter().map(|(_, m)| m.len()).collect();
    let (least, most) = (sizes.iter().min().copied().unwrap_or(0), sizes.iter().max().copied().unwrap_or(0));
    let n = sizes.len();
    let of = if least == most { format!("{most}") } else { format!("{least}–{most}") };
    let short = sizes.iter().filter(|&&k| k < need).count();
    let (marker, style) = if short > 0 { ("✗", Style::default().fg(Color::Red)) } else { ("◉", Style::default().fg(Color::Green)) };
    let mut summary = format!("    {marker} {n} group{} of {of} orientations, marked in the runs below; {algo} on 3 Reconstruction › COSMOS/STI",
        if n == 1 { "" } else { "s" });
    if short > 0 {
        summary.push_str(&format!(" — {short} {} fewer than the {need} {algo} needs", if short == 1 { "has" } else { "have" }));
    }
    lines.push(Line::from(Span::styled(summary, style)));
    if preview.ungrouped > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "    {} selected run(s) matched no group — processed one orientation at a time",
                preview.ungrouped
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(Span::styled(
        "    B0 directions are checked at run time — `qsmxt run --dry` prints them per group",
        Style::default().fg(Color::DarkGray),
    )));

    lines
}
