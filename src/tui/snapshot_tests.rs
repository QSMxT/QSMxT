//! Snapshots of the TUI's screens (under `tests/snapshots/tui/`): every tab, every input mode,
//! and every Reconstruction section in Basic and Advanced mode, on a fresh form, as text.
//!
//! What the screens show must not change in a refactor. Regenerate with
//! `QSMXT_UPDATE_SNAPSHOTS=1 cargo test --bin qsmxt tui::snapshot_tests` only for an intended
//! change, and say why in the PR.

use crate::snapshot;

use ratatui::{backend::TestBackend, Terminal};

use super::app::{App, InputMode, Page, TAB_NAMES};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/tui");

/// Tall enough that the longest form fits.
const WIDTH: u16 = 120;
const HEIGHT: u16 = 100;

/// The screen as text, one line per row, trailing spaces trimmed. This machine's memory is
/// `<memory>`, and the build's versions in the tab rule `<version>` (with the rule's padding before
/// the label shortened, as its length follows theirs).
fn screen(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    app.prepare_frame();
    t.draw(|f| super::ui::draw(f, app)).unwrap();
    let buf = t.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect();
        out.push_str(row.trim_end());
        out.push('\n');
    }
    let memory = regex::Regex::new(r"\([0-9.]+ [KMGT]?B available here\) *").unwrap();
    let out = memory.replace_all(&out, "(<memory> available here) ");
    let versions = regex::Regex::new(r"QSMxT \S+ · QSM\.rs \S+").unwrap();
    let out = versions.replace_all(&out, "QSMxT <version> · QSM.rs <version>");
    let padding = regex::Regex::new(r"─+ Pipeline:").unwrap();
    padding.replace_all(&out, "── Pipeline:").into_owned()
}

/// A fresh form on `tab`, at `page` when given.
fn app_on(tab: usize, page: Option<Page>, advanced: bool) -> App {
    let mut app = App::new_for_trial();
    app.advanced = advanced;
    app.active_tab = tab;
    if page.is_some() {
        app.sub_page[tab] = page;
    }
    app.sync_pipeline_mode();
    app
}

fn name(s: &str) -> String {
    s.to_lowercase().replace(['/', ' '], "_")
}

#[test]
fn every_tab_and_section() {
    for advanced in [false, true] {
        let mode = if advanced { "advanced" } else { "basic" };
        for (tab, tab_name) in TAB_NAMES.iter().enumerate() {
            let pages = app_on(tab, None, advanced).pages(tab);
            if pages.is_empty() {
                let mut app = app_on(tab, None, advanced);
                snapshot::check(DIR, &format!("{mode}/{}.txt", name(tab_name)), &screen(&mut app));
            }
            for page in pages {
                let mut app = app_on(tab, Some(page), advanced);
                snapshot::check(DIR, &format!("{mode}/{}_{}.txt", name(tab_name), name(page.label())), &screen(&mut app));
            }
        }
    }
}

#[test]
fn every_input_mode() {
    for (mode, label) in [(InputMode::Bids, "bids"), (InputMode::NIfTI, "nifti"),
                          (InputMode::DicomToBids, "dicom"), (InputMode::Example, "example")] {
        let mut app = app_on(0, None, false);
        app.input_mode = mode;
        snapshot::check(DIR, &format!("input_{label}.txt"), &screen(&mut app));
    }
}

/// What the next key and the config depend on: the settings, the context and page of the pipeline
/// form, where focus is, and the config built.
fn form_state(app: &App) -> String {
    let config = super::command::config_from_app(app).map(|c| serde_json::to_string(&c).unwrap());
    format!("{} {:?} {:?} {} {} {} {} {config:?}", app.rec_fingerprint(), app.active_tab, app.sub_page,
            app.active_field, app.subtab_focus, app.tab_focus, app.editing)
}

/// Drawing only shows the state ([`App::prepare_frame`] brings it up to date first): a frame
/// leaves the form, its focus and its config as they were, on every tab and section.
#[test]
fn drawing_changes_nothing_the_form_depends_on() {
    for advanced in [false, true] {
        for tab in 0..TAB_NAMES.len() {
            let pages = app_on(tab, None, advanced).pages(tab);
            let pages: Vec<Option<Page>> = if pages.is_empty() { vec![None] } else { pages.into_iter().map(Some).collect() };
            for page in pages {
                let mut app = app_on(tab, page, advanced);
                app.prepare_frame();
                let before = form_state(&app);
                let mut t = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
                t.draw(|f| super::ui::draw(f, &mut app)).unwrap();
                assert_eq!(before, form_state(&app), "drawing tab {tab} {page:?} changed the form");
            }
        }
    }
}

/// A frame shows the config the form builds: [`App::prepare_frame`] builds it, and outside a
/// frame a change to the form is seen at once.
#[test]
fn the_frames_config_is_the_forms() {
    let fresh = |app: &App| Some(super::command::config_from_app(app).map_err(|e| e.to_string()));
    let mut app = App::new_for_trial();
    app.prepare_frame();
    assert_eq!(app.config, fresh(&app));
    app.set_field_select("qsm_algorithm", 7);
    assert_eq!(app.current_config().as_ref().as_ref().unwrap().inversion.algorithm, qsmxt_engine::QsmAlgorithm::Medi,
               "a key handler sees its own change");
    app.prepare_frame();
    assert_eq!(app.config, fresh(&app));
}
