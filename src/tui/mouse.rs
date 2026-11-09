//! The mouse: clicking tabs, sub-tabs, settings, list entries and pop-up buttons, and the wheel.
//!
//! - **Where things are** is recorded as each frame is drawn ([`Hits`]): every drawer that puts
//!   something clickable on the screen records its area and what it is ([`Hit`]). A click acts
//!   on the last area recorded under it, so a pop-up (drawn last) wins over what is under it.
//! - **A click does what its key does**, through the same handlers: it focuses the row or entry
//!   and then sends the key that acts on it. Nothing happens on a click that a key could not do.
//!   - A tab or sub-tab: switches to it.
//!   - A checkbox: ticks it. A picker button (`[ … ▾ ]`) or a value to edit: opens it. A short
//!     choice (`◀ … ▶`): the first click focuses it, the next takes the next value. Any other row
//!     (Add section, Run): the first click focuses it, the next presses it.
//!   - A pipeline, or a picker's entry: the first click highlights it, the next chooses it.
//!   - A pop-up's button: presses it.
//! - **While a pop-up is open**, only its own entries and buttons answer, as only its keys do.
//! - **The wheel** moves as `↑`/`↓` do, but never out of the page onto the sub-tab strip or the
//!   tab bar.
//! - **Selecting text**: with the mouse captured, a plain drag no longer selects text in most
//!   terminals; holding Shift (Option in iTerm2) does. `QSMXT_NO_MOUSE` leaves the mouse to the
//!   terminal.

use std::cell::RefCell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;

use super::app::{focusable_indices, App, PipelineRow};

/// Set (to anything non-empty) to leave the mouse to the terminal.
pub const OPT_OUT_ENV: &str = "QSMXT_NO_MOUSE";

/// Whether the TUI captures the mouse.
pub fn enabled() -> bool {
    std::env::var(OPT_OUT_ENV).map_or(true, |v| v.is_empty())
}

/// What a click lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A main tab, by index.
    Tab(usize),
    /// A sub-tab pill, by index into the active tab's pages.
    SubTab(usize),
    /// The settings rows: the row is `scroll` plus the line clicked.
    Rows { scroll: usize },
    /// The Pipeline tab's list: the entry is the line clicked, less the heading after the first.
    Pipelines,
    /// A picker's list: the entry is `offset` plus the line clicked.
    Picker { offset: usize },
    /// The open pop-up's button, by index.
    Button(usize),
}

impl Hit {
    /// Whether it belongs to a pop-up (and so answers while one is open).
    fn in_popup(self) -> bool {
        matches!(self, Hit::Picker { .. } | Hit::Button(_))
    }
}

/// The clickable areas of the frame being drawn, in drawing order.
#[derive(Debug, Default)]
pub struct Hits(RefCell<Vec<(Rect, Hit)>>);

impl Hits {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    pub fn add(&self, area: Rect, hit: Hit) {
        if area.width > 0 && area.height > 0 {
            self.0.borrow_mut().push((area, hit));
        }
    }

    /// The buttons of lines drawn in `area` from its top: each styled, non-blank span is a button
    /// (the separators between them are plain), numbered in order across the lines.
    pub fn add_buttons(&self, area: Rect, lines: &[Line]) {
        let mut n = 0;
        for (dy, line) in lines.iter().enumerate() {
            if dy as u16 >= area.height {
                break;
            }
            let width = line.width() as u16;
            let mut x = area.x + match line.alignment {
                Some(Alignment::Center) => area.width.saturating_sub(width) / 2,
                Some(Alignment::Right) => area.width.saturating_sub(width),
                _ => 0,
            };
            for span in &line.spans {
                let w = span.width() as u16;
                if span.style != ratatui::style::Style::default() && !span.content.trim().is_empty() {
                    self.add(Rect { x, y: area.y + dy as u16, width: w, height: 1 }, Hit::Button(n));
                    n += 1;
                }
                x += w;
            }
        }
    }

    /// The last area under (`x`, `y`), and where in it.
    fn at(&self, x: u16, y: u16) -> Option<(Rect, Hit)> {
        self.0.borrow().iter().rev().find(|(r, _)| x >= r.x && x < r.right() && y >= r.y && y < r.bottom()).copied()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

impl App {
    /// Whether a pop-up has the keys.
    fn popup_open(&self) -> bool {
        self.update_modal.is_some() || self.news_modal.is_some() || self.tip_modal.is_some()
            || self.algo_modal.is_some() || self.rec_modal.is_some() || self.refs.modal.is_some()
            || self.pipelines.preview.is_some()
    }

    pub fn handle_mouse(&mut self, ev: MouseEvent) {
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click(ev.column, ev.row),
            MouseEventKind::ScrollUp => self.wheel(KeyCode::Up),
            MouseEventKind::ScrollDown => self.wheel(KeyCode::Down),
            _ => {}
        }
    }

    /// The wheel: `↑`/`↓`, but never off the page onto the strip or the tab bar.
    fn wheel(&mut self, code: KeyCode) {
        if self.is_typing() {
            return;
        }
        let (tab, strip) = (self.tab_focus, self.subtab_focus);
        self.handle_key(key(code));
        if !self.popup_open() && ((self.tab_focus && !tab) || (self.subtab_focus && !strip)) {
            self.tab_focus = tab;
            self.subtab_focus = strip;
        }
    }

    fn click(&mut self, x: u16, y: u16) {
        let Some((area, hit)) = self.hits.at(x, y) else { return };
        if self.popup_open() != hit.in_popup() {
            return;
        }
        let line = (y - area.y) as usize;
        // A click elsewhere ends typing, as Enter would; the text is kept.
        if self.pipeline_state.editing && !matches!(hit, Hit::Rows { .. }) {
            self.pipeline_state.editing = false;
        }
        match hit {
            Hit::Tab(i) => {
                self.pipeline_state.editing = false;
                self.tab_focus = false;
                if let Some(d) = char::from_digit(i as u32 + 1, 10) {
                    self.switch_tab(key(KeyCode::Char(d)));
                }
                // Into the tab, as ↓ from the tab bar goes: onto its strip, or its first row.
                self.subtab_focus = self.has_subtabs();
            }
            Hit::SubTab(i) => {
                self.tab_focus = false;
                let pages = self.pages(self.active_tab);
                if let Some(&page) = pages.get(i) {
                    self.sub_page[self.active_tab] = Some(page);
                    self.reset_view();
                    self.sync_pipeline_mode();
                }
                self.subtab_focus = true;
            }
            Hit::Rows { scroll } => self.click_row(scroll + line),
            Hit::Pipelines => {
                // The list's lines: the first entry, a heading, then the others.
                let i = match line { 0 => 0, 1 => return, n => n - 1 };
                if i >= qsmxt_engine::pipelines::pipelines().len() {
                    return;
                }
                let again = self.pipelines.cursor == i && !self.tab_focus;
                self.tab_focus = false;
                self.pipelines.cursor = i;
                if again {
                    self.handle_key(key(KeyCode::Enter));
                }
            }
            Hit::Picker { offset } => {
                let i = offset + line;
                let Some(m) = self.algo_modal.as_mut() else { return };
                if i >= m.options.len() {
                    return;
                }
                let again = m.cursor == i;
                m.cursor = i;
                if again {
                    self.handle_key(key(KeyCode::Enter));
                }
            }
            Hit::Button(i) => self.press_button(i),
        }
    }

    /// Press button `i` of the pop-up that has the keys: select it, then Enter.
    fn press_button(&mut self, i: usize) {
        if self.update_modal.is_some() || self.news_modal.is_some() {
            return;
        }
        if let Some(m) = self.tip_modal.as_mut() {
            m.cursor = i;
        } else if let Some(m) = self.rec_modal.as_mut() {
            m.cursor = Some(i);
        } else if let Some(m) = self.refs.modal.as_mut() {
            m.cursor = Some(i);
        } else if let Some(m) = self.pipelines.preview.as_mut() {
            m.button = i;
        } else {
            return;
        }
        self.handle_key(key(KeyCode::Enter));
    }

    /// A click on settings row `row`: focus it, then act as its key does.
    fn click_row(&mut self, row: usize) {
        let rows = self.rows();
        let Some(pos) = focusable_indices(&rows).iter().position(|&r| r == row) else { return };
        let Some(r) = rows.get(row).cloned() else { return };
        if self.pipeline_state.editing {
            if pos == self.focus() {
                return;
            }
            self.pipeline_state.editing = false;
        }
        let again = pos == self.focus() && !self.tab_focus && !self.subtab_focus;
        self.tab_focus = false;
        self.subtab_focus = false;
        self.set_focus(pos);
        let code = match &r {
            PipelineRow::Toggle { .. } => Some(KeyCode::Char(' ')),
            PipelineRow::AlgoSelect { .. } if super::recommend::picker_noun(&r).is_some() => Some(KeyCode::Enter),
            PipelineRow::AlgoSelect { .. } => again.then_some(KeyCode::Right),
            PipelineRow::Param { .. } => Some(KeyCode::Enter),
            _ => again.then_some(KeyCode::Enter),
        };
        if let Some(code) = code {
            self.handle_key(key(code));
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Page, TAB_PIPELINE, TAB_PREPROCESSING, TAB_QSM};
    use ratatui::{backend::TestBackend, Terminal};

    /// Draw a frame (recording where things are) and return its text, one string per row.
    fn draw(app: &mut App) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(140, 42)).unwrap();
        t.draw(|f| super::super::ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer();
        (0..buf.area.height).map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()).collect()
    }

    /// Click the first place `text` is drawn, after drawing a frame.
    fn click(app: &mut App, text: &str) {
        let screen = draw(app);
        let (x, y) = screen.iter().enumerate().find_map(|(y, row)| {
            row.find(text).map(|b| (row[..b].chars().count() as u16 + 1, y as u16))
        }).unwrap_or_else(|| panic!("{text:?} is not on the screen:\n{}", screen.join("\n")));
        app.handle_mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: x, row: y, modifiers: KeyModifiers::NONE });
    }

    fn wheel(app: &mut App, kind: MouseEventKind) {
        draw(app);
        app.handle_mouse(MouseEvent { kind, column: 60, row: 10, modifiers: KeyModifiers::NONE });
    }

    #[test]
    fn tabs_and_sub_tabs_switch_on_a_click() {
        let mut app = App::new_for_trial();
        click(&mut app, "Preprocessing");
        assert_eq!(app.active_tab, TAB_PREPROCESSING);
        click(&mut app, " Motion ");
        assert_eq!(app.page(), Some(Page::Motion));
        click(&mut app, "4 QSM");
        assert_eq!(app.active_tab, TAB_QSM);
    }

    /// A picker button opens its list; an entry is highlighted by one click and chosen by the
    /// next; a pop-up's button is pressed by a click, and while it is open the tabs do not answer.
    #[test]
    fn pickers_entries_and_pop_up_buttons() {
        let mut app = App::new_for_trial();
        app.check_recommendations();
        click(&mut app, "4 QSM");
        click(&mut app, " Inversion ");
        click(&mut app, "QSM inversion");
        assert!(app.algo_modal.is_some(), "the picker opens");
        click(&mut app, "  tkd");
        assert!(app.algo_modal.is_some(), "one click highlights");
        click(&mut app, "▶ tkd");
        assert!(app.algo_modal.is_none(), "the second chooses");
        assert_eq!(app.field_select("qsm_algorithm"), crate::tui::app::QSM_PICKER_OPTIONS.iter().position(|o| *o == "tkd").unwrap());
        // HEIDI with TKD opens its recommendation pop-up.
        click(&mut app, "HEIDI");
        app.check_recommendations();
        assert!(app.rec_modal.is_some(), "ticking HEIDI on TKD recommends LSQR");
        click(&mut app, "1 Input");
        assert_eq!(app.active_tab, TAB_QSM, "the tabs do not answer under a pop-up");
        click(&mut app, " Keep mine ");
        assert!(app.rec_modal.is_none(), "the button closes it");
    }

    #[test]
    fn a_checkbox_ticks_and_a_value_opens_for_editing() {
        let mut app = App::new_for_trial();
        click(&mut app, "4 QSM");
        click(&mut app, " Inversion ");
        let before = app.pipeline_state.heidi_enabled;
        click(&mut app, "HEIDI");
        assert_ne!(app.pipeline_state.heidi_enabled, before);
        click(&mut app, "Max iter");
        assert!(app.pipeline_state.editing, "a value opens for editing");
        click(&mut app, "Tolerance");
        assert!(app.pipeline_state.editing, "another value: that one");
        click(&mut app, "4 QSM");
        assert!(!app.pipeline_state.editing, "a click elsewhere ends editing");
    }

    #[test]
    fn a_pipeline_is_previewed_and_applied_by_clicks() {
        let mut app = App::new_for_trial();
        click(&mut app, "2 Pipeline");
        assert_eq!(app.active_tab, TAB_PIPELINE);
        click(&mut app, "MEDI toolbox");
        assert!(app.pipelines.preview.is_none(), "one click highlights");
        click(&mut app, "▶ MEDI toolbox");
        assert!(app.pipelines.preview.is_some(), "the second opens the preview");
        click(&mut app, "Apply MEDI toolbox");
        assert!(app.pipelines.preview.is_none());
        assert_eq!(app.pipeline_status().text(), "MEDI toolbox");
    }

    /// The wheel moves through the rows, and stops at the first instead of leaving the page.
    #[test]
    fn the_wheel_stays_on_the_page() {
        let mut app = App::new_for_trial();
        click(&mut app, "4 QSM");
        click(&mut app, " Inversion ");
        click(&mut app, "QSM inversion");
        app.algo_modal = None;
        let at = app.focus();
        wheel(&mut app, MouseEventKind::ScrollDown);
        assert_eq!(app.focus(), at + 1);
        for _ in 0..10 {
            wheel(&mut app, MouseEventKind::ScrollUp);
        }
        assert_eq!(app.focus(), 0);
        assert!(!app.tab_focus && !app.on_subtabs(), "the wheel does not leave the page");
    }

    #[test]
    fn the_mouse_can_be_left_to_the_terminal() {
        assert_eq!(OPT_OUT_ENV, "QSMXT_NO_MOUSE");
    }
}
