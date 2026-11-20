//! The App's tests, by subject, and the helpers they share.

mod input;
mod popups;
mod keys;
mod navigation;
mod pipeline;
mod other_tabs;
mod mask;

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Whether `field` has a row on `tab` now (switches to it, syncing the pipeline rows).
/// SynthSeg's own settings, under the segmentation checkbox.
const SYNTHSEG_ROWS: [&str; 6] = ["segmentation.version", "segmentation.crop", "segmentation.patch", "segmentation.flip_averaging",
                                  "segmentation.topology_cleanup", "segmentation.sigma_smoothing#value"];

fn shows(app: &mut App, tab: usize, field: &str) -> bool {
    app.active_tab = tab;
    app.sync_pipeline_mode();
    app.pages(tab).into_iter().any(|p| app.page_rows(p).iter().any(|r| row_field(r) == Some(field)))
}

/// Switch to `tab`, on the sub-tab that shows `field`, and focus its row.
fn focus_on(app: &mut App, tab: usize, field: &str) {
    app.active_tab = tab;
    let page = app.pages(tab).into_iter()
        .find(|&p| app.page_rows(p).iter().any(|r| row_field(r) == Some(field)));
    let page = page.unwrap_or_else(|| panic!("{field} has no row on tab {tab}"));
    goto_page(app, page);
    assert!(app.focus_field(field), "{field} has no row on tab {tab}");
}

/// Every field with a row anywhere now, with a `#Page` marker for each sub-tab shown.
fn all_fields(app: &mut App) -> Vec<String> {
    app.sync_pipeline_mode();
    let mut out = Vec::new();
    for (_, page, rows) in app.all_pages() {
        out.push(format!("#{page:?}"));
        out.extend(rows.iter().filter_map(row_field).map(str::to_string));
    }
    out
}

/// Open `page` (on whichever tab has it now).
fn goto_page(app: &mut App, page: Page) {
    let tab = (0..TAB_NAMES.len()).find(|&t| app.pages(t).contains(&page))
        .unwrap_or_else(|| panic!("{page:?} is not shown now"));
    app.active_tab = tab;
    app.sub_page[tab] = Some(page);
    app.sync_pipeline_mode();
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Enter on a selector row opens its picker; move its cursor `by` and confirm.
fn step(app: &mut App, by: isize) {
    app.handle_key(key(KeyCode::Enter));
    let m = app.algo_modal.as_mut().expect("Enter opens the picker");
    let n = m.options.len() as isize;
    m.cursor = (m.cursor as isize + by).rem_euclid(n) as usize;
    app.handle_key(key(KeyCode::Enter));
}

/// Enter on a mask row's number, the text replaced by `text`, Enter.
fn type_mask_value(app: &mut App, text: &str) {
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.mask_value_editing, "Enter edits the number");
    app.handle_key(key(KeyCode::End));
    while !app.pipeline_state.mask_value_buf.is_empty() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in text.chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
}
