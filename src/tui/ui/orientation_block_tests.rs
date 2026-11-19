//! The Input tab's multi-orientation block, drawn.

// `super::tests::*` re-exports the render helpers this module needs; `super::*` on top of it
// brings in nothing further.
use super::tests::*;
use crate::tui::app::{App, TAB_INPUT};

/// Build an app sitting on the Input tab with a scanned three-orientation dataset.
fn app_with_orientations() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_multi_orientation_bids(dir.path());
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.form.bids_dir = dir.path().to_string_lossy().into_owned();
    app.filter_state.maybe_rescan(&app.form.bids_dir.clone());
    (app, dir)
}

/// Off by default, and taking up exactly one line, so the single-orientation majority
/// sees a field rather than a feature.
#[test]
fn the_block_is_one_line_when_off() {
    let (mut app, _d) = app_with_orientations();
    let text = screen_text(&render_app_sized(&mut app, 140, 60));
    assert!(text.contains("Orientations:"), "{text}");
    assert!(text.contains("(off: type a pattern"), "{text}");
    assert!(!text.contains("Combine as:"), "settings should stay hidden:\n{text}");
}

/// With a pattern, the groups and their members are on screen — this is the preview the
/// whole pattern field depends on.
#[test]
fn a_matching_pattern_lists_its_groups() {
    let (mut app, _d) = app_with_orientations();
    app.filter_state.orientation_group = "*acq-dir*".into();
    let text = screen_text(&render_app_sized(&mut app, 140, 60));
    assert!(text.contains("1 group of 3 orientations"), "{text}");
    assert!(text.contains("COSMOS on 3 Reconstruction › COSMOS/STI"), "{text}");
    assert_eq!(text.matches("(group 1").count(), 3, "each member is marked:\n{text}");
    // The unrelated acquisition is reported as left out, not silently swallowed.
    assert!(text.contains("matched no group"), "{text}");
}

/// A pattern that matches nothing has to say so — the failure it prevents is a run that
/// quietly processes every orientation separately.
#[test]
fn a_pattern_that_matches_nothing_says_so() {
    let (mut app, _d) = app_with_orientations();
    app.filter_state.orientation_group = "*acq-nothinghere*".into();
    let text = screen_text(&render_app_sized(&mut app, 140, 60));
    assert!(text.contains("no orientation groups"), "{text}");
}

/// STI needs six orientations; a three-orientation group is flagged before the run.
#[test]
fn a_group_too_small_for_sti_is_flagged() {
    let (mut app, _d) = app_with_orientations();
    app.filter_state.orientation_group = "*acq-dir*".into();
    app.set_field_select("orientation_algorithm", 1); // sti
    let text = screen_text(&render_app_sized(&mut app, 140, 60));
    assert!(text.contains("fewer than the 6 STI needs"), "{text}");
    assert!(text.contains("3 of the 6 needed"), "{text}");
}

/// The reference orientation is listed first and marked; one that matches no member is
/// reported where the groups would be, as the run would refuse it.
#[test]
fn the_reference_orientation_is_marked_and_checked() {
    let (mut app, _d) = app_with_orientations();
    app.filter_state.orientation_group = "*acq-dir*".into();
    *app.field_text_mut("orientation_reference").unwrap() = "dir3".into();
    let text = screen_text(&render_app_sized(&mut app, 200, 60));
    let line = text.lines().find(|l| l.contains("(group 1; reference)")).unwrap_or_else(|| panic!("{text}"));
    assert!(line.contains("acq-dir3_MEGRE"), "{line}");
    assert_eq!(text.matches("reference)").count(), 1, "{text}");
    *app.field_text_mut("orientation_reference").unwrap() = "dir9".into();
    let text = screen_text(&render_app_sized(&mut app, 200, 60));
    assert!(text.contains("matches none"), "{text}");
}

/// Editing the pattern shows a cursor in it, after what is typed.
#[test]
fn editing_the_pattern_shows_a_cursor() {
    let (mut app, _d) = app_with_orientations();
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.focus = crate::tui::app::FilterFocus::OrientationGroup;
    app.filter_state.orientation_group = "acq".into();
    app.filter_state.orientation_editing = true;
    app.filter_state.orientation_cursor = 3;
    let mut t = render_app_sized(&mut app, 140, 60);
    let at = t.get_cursor_position().unwrap();
    let text = screen_text(&t);
    let row = text.lines().nth(at.y as usize).unwrap();
    assert!(row.contains("Orientations: acq"), "{row}");
    let col = row[..row.find("acq").unwrap()].chars().count() + 3;
    assert_eq!(at.x as usize, col, "{row}");
}

#[test]
fn an_invalid_pattern_is_reported_inline() {
    let (mut app, _d) = app_with_orientations();
    app.filter_state.orientation_group = "chunk".into();
    let text = screen_text(&render_app_sized(&mut app, 140, 60));
    assert!(text.contains("does not tell"), "{text}");
}

/// The grouping's settings are rows of QSM → COSMOS/STI, with the algorithms' help.
#[test]
fn orientation_settings_are_on_the_qsm_tab() {
    let (mut app, _d) = app_with_orientations();
    app.filter_state.orientation_group = "*acq-dir*".into();
    app.active_tab = crate::tui::app::TAB_RECONSTRUCTION;
    assert!(app.goto_field("orientation_algorithm"));
    let text = screen_text(&render_app_sized(&mut app, 140, 60));
    assert!(text.contains("COSMOS/STI") && text.contains("Combine as"), "{text}");
    assert!(text.contains("COSMOS — scalar susceptibility"), "help follows the choice:\n{text}");
    app.set_field_select("orientation_algorithm", 1);
    let text = screen_text(&render_app_sized(&mut app, 140, 60));
    assert!(text.contains("STI — rank-2"), "{text}");
}

/// Long numbers read rounded; short ones and non-numbers as they are.
#[test]
fn long_numbers_read_rounded() {
    assert_eq!(super::compact_number("0.003924908970520748").as_deref(), Some("0.003925"));
    assert_eq!(super::compact_number("0.05943433583931419").as_deref(), Some("0.05943"));
    assert_eq!(super::compact_number("0.0000000000000002220446049250313").as_deref(), Some("2.22e-16"));
    assert_eq!(super::compact_number("1234.56789012345").as_deref(), Some("1235"));
    for short in ["0.1", "0.00105", "500", "12 12 12", "not.a.number.at.all", "12345678901234"] {
        assert_eq!(super::compact_number(short), None, "{short}");
    }
}

/// HEIDI's rows show the rounded defaults, but the form keeps every digit: the defaults are
/// not sent as changes, and the help line gives the exact value.
#[test]
fn heidi_defaults_read_rounded_and_stay_exact() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = crate::tui::app::App::new();
    app.set_field_select("qsm_algorithm", crate::tui::app::QSM_PICKER_OPTIONS.iter().position(|o| *o == crate::tui::app::LSQR_HEIDI).unwrap());
    assert!(app.goto_field("heidi_gradient_threshold"));
    let mut t = Terminal::new(TestBackend::new(200, 60)).unwrap();
    app.prepare_frame();
    t.draw(|f| super::draw(f, &mut app)).unwrap();
    let buf = t.backend().buffer();
    let text = (0..buf.area.height).map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>()).collect::<Vec<_>>().join("\n");
    assert!(text.contains("0.003925") && text.contains("2.22e-16") && text.contains("0.05943"), "{text}");
    assert!(text.contains("exact value: 0.003924908970520748"), "{text}");
    assert!(!text.contains("0.0000000000000002220446049250313"), "{text}");
    let c = crate::tui::command::config_from_app(&app).unwrap();
    assert_eq!(c.inversion.heidi, qsmxt_engine::HeidiConfig { enabled: true, ..Default::default() });
}
