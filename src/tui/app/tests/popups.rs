//! The launch-time pop-ups (update, announcements) and the option picker.

use super::*;

fn update_app() -> App {
    let mut app = App::new();
    app.update_state = UpdateState::disabled();
    app.update_modal = Some(UpdateModal {
        tag: "v9.23.0".into(),
        html_url: "https://example.com/r".into(),
    });
    app
}

#[test]
fn update_prompt_enter_hands_over_to_the_installer() {
    let mut app = update_app();
    assert_eq!(app.handle_update_modal_key(key(KeyCode::Enter)), UpdateChoice::Update);
    assert!(app.should_update, "the main loop needs this to run the update");
    assert!(app.update_modal.is_none(), "the prompt should close");
}

#[test]
fn update_prompt_esc_defers_to_the_next_start() {
    let mut app = update_app();
    assert_eq!(app.handle_update_modal_key(key(KeyCode::Esc)), UpdateChoice::Later);
    assert!(!app.should_update);
    assert!(app.update_modal.is_none());
}

/// There is no way to skip a release: the prompt comes back at every start until the
/// user updates, so `d` does nothing.
#[test]
fn update_prompt_has_no_skip() {
    let mut app = update_app();
    assert_eq!(app.handle_update_modal_key(key(KeyCode::Char('d'))), UpdateChoice::Ignored);
    assert!(app.update_modal.is_some());
}

/// An unrelated keystroke must not dismiss the prompt by accident — it is modal,
/// and a stray arrow key should leave it exactly as it was.
#[test]
fn update_prompt_ignores_keys_it_does_not_bind() {
    let mut app = update_app();
    assert_eq!(app.handle_update_modal_key(key(KeyCode::Down)), UpdateChoice::Ignored);
    assert!(app.update_modal.is_some());
    assert!(!app.should_update);
}

/// While the prompt is up it owns the keyboard: keys must not also reach the form
/// underneath, or answering it would edit whatever field happened to be focused.
#[test]
fn update_prompt_captures_input_from_the_form() {
    let mut app = update_app();
    app.active_tab = TAB_EXECUTION;
    let before = app.active_field;
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.active_field, before, "the form must not move behind the prompt");
    assert!(app.update_modal.is_some());
}

/// A dev build must not spawn a network check just by opening the TUI.
#[test]
fn a_dev_build_starts_no_background_check() {
    let mut state = UpdateState::start();
    assert!(state.poll().is_none());
}

/// An app whose launch checks have come back: `update` the newer release found (if any),
/// `news` the announcements fetched.
fn launched(update: Option<&str>, news: Vec<qsmxt_io::news::Announcement>) -> App {
    let mut app = App::new();
    app.update_state = UpdateState::ready(update.map(|tag| qsmxt_io::release::check::UpdateInfo {
        tag: tag.into(),
        html_url: "https://example.com/r".into(),
    }));
    app.news_state = NewsState::ready(qsmxt_io::news::Loaded { items: news, error: None });
    app
}

use qsmxt_io::news::item;

/// The pop-ups' order: critical announcements, the update prompt, then ordinary ones
/// (newest first), one at a time; closing one records it as shown.
#[test]
fn launch_pop_ups_come_one_at_a_time_in_order() {
    let t = qsmxt_io::news::TestDir::new();
    let mut app = launched(Some("v99.0.0"), vec![
        item("old", "2026-11-01", false, None),
        item("new", "2026-11-05", false, None),
        item("crit", "2026-10-01", true, None),
    ]);
    app.poll_launch_checks();
    let shown = |app: &App| app.news_modal.as_ref().map(|m| m.items[0].id.clone());
    assert_eq!(shown(&app).as_deref(), Some("crit"), "critical first");
    assert!(app.update_modal.is_none(), "no pop-up opens over another");

    app.handle_key(key(KeyCode::Enter));
    app.poll_launch_checks();
    assert!(app.update_modal.is_some(), "then the update prompt");
    assert!(app.news_modal.is_none());

    app.handle_key(key(KeyCode::Esc));
    app.poll_launch_checks();
    assert_eq!(shown(&app).as_deref(), Some("new"), "then ordinary news, newest first");
    app.handle_key(key(KeyCode::Esc));
    app.poll_launch_checks();
    assert_eq!(shown(&app).as_deref(), Some("old"));
    app.handle_key(key(KeyCode::Char('q')));
    app.poll_launch_checks();
    assert!(app.news_modal.is_none() && app.update_modal.is_none());

    // Next start: only the critical one (and the update) come back.
    let mut app = launched(Some("v99.0.0"), vec![
        item("old", "2026-11-01", false, None),
        item("new", "2026-11-05", false, None),
        item("crit", "2026-10-01", true, None),
    ]);
    app.poll_launch_checks();
    assert_eq!(shown(&app).as_deref(), Some("crit"));
    app.handle_key(key(KeyCode::Enter));
    app.poll_launch_checks();
    assert!(app.update_modal.is_some());
    app.handle_key(key(KeyCode::Esc));
    app.poll_launch_checks();
    assert!(app.news_modal.is_none(), "ordinary news pops up once");
    drop(t);
}

/// Announcements wait while another pop-up (the startup tip, an algorithm list) is open.
#[test]
fn announcements_wait_for_other_pop_ups() {
    let _t = qsmxt_io::news::TestDir::new();
    let mut app = launched(None, vec![item("crit", "2026-10-01", true, None), item("a", "2026-11-01", false, None)]);
    app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
    app.poll_launch_checks();
    assert!(app.news_modal.is_none(), "not over the tip");
    app.tip_modal = None;
    app.poll_launch_checks();
    assert_eq!(app.news_modal.as_ref().unwrap().items[0].id, "crit");
}

/// The update prompt waits for the announcements (a critical one goes first), and ordinary
/// announcements wait for the release check (the prompt goes first).
#[test]
fn each_pop_up_waits_for_the_check_that_goes_before_it() {
    let _t = qsmxt_io::news::TestDir::new();
    let mut app = launched(Some("v99.0.0"), Vec::new());
    let (_tx, rx) = std::sync::mpsc::channel();
    app.news_state = NewsState { receiver: Some(rx), done: false };
    app.poll_launch_checks();
    assert!(app.update_modal.is_none(), "the announcements are still being fetched");

    let mut app = launched(None, vec![item("a", "2026-11-01", false, None)]);
    let (_tx2, rx2) = std::sync::mpsc::channel();
    app.update_state = UpdateState { receiver: Some(rx2), done: false };
    app.poll_launch_checks();
    assert!(app.news_modal.is_none(), "the release check is still running");
}

/// An announcement for other versions does not pop up; nor does a ranged one on a dev build.
#[test]
fn announcements_for_other_versions_do_not_pop_up() {
    let _t = qsmxt_io::news::TestDir::new();
    let mut app = launched(None, vec![item("a", "2026-11-01", true, Some(("1.0.0", "2.0.0")))]);
    app.poll_launch_checks();
    assert!(app.news_modal.is_none());
    assert!(app.news_queue.is_empty());
}

/// `n` on the tab bar opens every announcement, newest first, including those already
/// shown; it closes without recording anything.
#[test]
fn n_on_the_tab_bar_opens_the_history() {
    let _t = qsmxt_io::news::TestDir::new();
    let mut app = launched(None, vec![item("new", "2026-11-05", false, None), item("old", "2026-11-01", false, Some(("1.0.0", "2.0.0")))]);
    app.poll_launch_checks();
    app.news_modal = None;
    app.news_queue.clear();
    app.tab_focus = true;
    app.handle_key(key(KeyCode::Char('n')));
    let m = app.news_modal.as_ref().expect("the history should open");
    assert!(m.history);
    assert_eq!(m.items.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["new", "old"]);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.news_modal.as_ref().unwrap().scroll, 1, "↓ scrolls");
    app.handle_key(key(KeyCode::Esc));
    assert!(app.news_modal.is_none());
    assert!(!app.should_quit, "Esc closes the history, not the TUI");
    assert!(qsmxt_io::news::load_seen().is_empty(), "reading the history records nothing");
}

/// Offline, the history is the saved copy, and says so.
#[test]
fn the_history_offline_is_the_saved_copy() {
    let _t = qsmxt_io::news::TestDir::new();
    qsmxt_io::news::load(|| Ok("[[announcement]]\nid=\"a\"\ndate=\"2026-11-01\"\ntitle=\"Saved\"\nbody=\"b\"\n".into()));
    let mut app = App::new();
    app.receive_news(qsmxt_io::news::load(|| Err("offline".into())));
    app.open_news_history();
    let m = app.news_modal.as_ref().unwrap();
    assert_eq!(m.items[0].title, "Saved");
    assert!(m.note.as_deref().is_some_and(|n| n.contains("could not be read")));
}

/// `c` in a pop-up copies its link.
#[test]
fn c_copies_an_announcements_link() {
    let _t = qsmxt_io::news::TestDir::new();
    let mut a = item("a", "2026-11-01", false, None);
    a.link = Some("https://example.com/a".into());
    let mut app = launched(None, vec![a]);
    app.poll_launch_checks();
    app.handle_key(key(KeyCode::Char('c')));
    assert_eq!(app.clipboard.as_deref(), Some("https://example.com/a"));
    assert!(app.news_modal.is_some(), "copying does not close it");
}

#[test]
fn test_algo_modal_opens_on_enter_over_algoselect() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    // Move down the section to a selector and open it.
    let opened = (0..40).any(|_| {
        app.handle_key(key(KeyCode::Enter)); // opens modal iff focused row is an AlgoSelect
        if app.algo_modal.is_some() { return true; }
        app.handle_key(key(KeyCode::Down));
        false
    });
    assert!(opened, "expected to open an AlgoSelect modal while navigating the QSM tab");
    assert!(!app.algo_modal.as_ref().unwrap().options.is_empty());
}

#[test]
fn test_algo_modal_selects_option() {
    let mut app = App::new();
    app.algo_modal = Some(AlgoModal {
        target: AlgoModalTarget::Select("qsm_algorithm".to_string()),
        title: "QSM inversion".to_string(),
        options: QSM_ALGO_OPTIONS.iter().map(|s| s.to_string()).collect(),
        help: vec![String::new(); QSM_ALGO_OPTIONS.len()],
        cursor: 0,
        offset: Default::default(),
    });
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.algo_modal.as_ref().unwrap().cursor, 2);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.field_select("qsm_algorithm"), 2);
}

#[test]
fn test_algo_modal_cancel_leaves_value() {
    let mut app = App::new();
    let before = app.field_select("qsm_algorithm");
    app.algo_modal = Some(AlgoModal {
        target: AlgoModalTarget::Select("qsm_algorithm".to_string()),
        title: "QSM inversion".to_string(),
        options: vec!["a".to_string(), "b".to_string()],
        help: vec![String::new(), String::new()],
        cursor: 1,
        offset: Default::default(),
    });
    app.handle_key(key(KeyCode::Esc));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.field_select("qsm_algorithm"), before);
}

#[test]
fn test_mask_input_modal_opens_and_sets() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.sync_pipeline_mode();
    // Focus the mask Input row.
    let rows = app.rows();
    let target = app.focusable_rows().iter().position(|&ri| {
        matches!(rows.get(ri), Some(PipelineRow::MaskOpInput { .. }))
    }).expect("MaskOpInput row not focusable");
    app.pipeline_state.focus = target;
    app.handle_key(key(KeyCode::Enter));
    let modal = app.algo_modal.as_ref().expect("modal should open over mask Input");
    assert!(matches!(modal.target, AlgoModalTarget::MaskInput(_)));
    assert_eq!(modal.options.len(), 4);
    // Pick the last option (phase-quality) and confirm.
    while app.algo_modal.as_ref().unwrap().cursor < 3 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_none());
    assert_eq!(
        app.mask().sections[0].input,
        qsmxt_engine::MaskingInput::PhaseQuality
    );
}

#[test]
fn test_input_mode_modal_opens_and_sets() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 0;
    app.handle_key(key(KeyCode::Enter));
    let modal = app.algo_modal.as_ref().expect("modal should open over Input Mode");
    assert!(matches!(modal.target, AlgoModalTarget::InputMode));
    assert_eq!(modal.options.len(), 4);
    app.handle_key(key(KeyCode::Down)); // NIfTI
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.input_mode, InputMode::NIfTI);
}

#[test]
fn test_execution_mode_modal_opens_and_sets() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    focus_on(&mut app, TAB_EXECUTION, "execution_mode");
    app.handle_key(key(KeyCode::Enter));
    let modal = app.algo_modal.as_ref().expect("modal should open over Execution Mode");
    assert!(matches!(&modal.target, AlgoModalTarget::Select(f) if f == "execution_mode"));
    app.handle_key(key(KeyCode::Down)); // SLURM
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.form.execution_mode, 1);
}
