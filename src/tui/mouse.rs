//! The mouse: clicking tabs, sub-tabs, settings, list entries and pop-up buttons, and the wheel.
//!
//! - **Where things are** is recorded as each frame is drawn ([`Hits`]): every drawer that puts
//!   something clickable on the screen records its area and what it is ([`Hit`]). A click acts
//!   on the last area recorded under it, so a pop-up (drawn last) wins over what is under it.
//! - **A click does what its key does**, through the same handlers: it focuses the row or entry
//!   and then sends the key that acts on it. Nothing happens on a click that a key could not do.
//!   - A tab or sub-tab: switches to it.
//!   - A checkbox: ticks it. A picker button (`[ … ▾ ]`) or a value to edit: opens it. Any other
//!     row (Add section, Run): the first click focuses it, the next presses it.
//!   - On the Input page: a field to type in, or the input-mode picker, opens; a run or an example
//!     acquisition ticks; anything else (a subject, a series, a button) is focused by the first
//!     click and pressed by the next.
//!   - A pipeline, or a picker's entry: the first click highlights it, the next chooses it.
//!   - A pop-up's button: presses it.
//!   - The Methods tab's button (Copy methods text, Open graph): presses it, and focus goes into
//!     the page. A click anywhere else on the page goes into it too.
//! - **While a pop-up is open**, only its own entries and buttons answer, as only its keys do.
//! - **The wheel** scrolls what is under the mouse. Over the Methods tab's text or graph it
//!   scrolls that, wherever focus is. Over a tab's settings while focus is on its sections or
//!   sub-tabs it scrolls the settings, leaving focus there. Otherwise it moves as `↑`/`↓` do, but
//!   never out of the page onto the sub-tab strip or the tab bar.
//! - **Selecting text**: with the mouse captured, a plain drag no longer selects text in most
//!   terminals; holding Shift (Option in iTerm2) does. `QSMXT_NO_MOUSE` leaves the mouse to the
//!   terminal.

use std::cell::RefCell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;

use super::app::{focusable_indices, App, DicomFocus, ExampleState, FilterFocus, InputMode, NiftiFocus, PipelineRow, TreeRow};

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
    /// A row of the open advice pop-up's checklist, by index: a click ticks it.
    AdviceRow(usize),
    /// A line of the Input page.
    Input(InputTarget),
    /// The open pop-up, anywhere in it: a click there does nothing, a click outside it closes it.
    Popup,
    /// The Methods tab's page (its text or graph): the wheel scrolls it, a click goes into it.
    Page,
    /// The Methods tab's button (Copy methods text, Open graph): a click presses it.
    PageButton,
}

/// What a line of the Input page focuses: the input-mode picker and the directory/config fields
/// above, then whatever the input mode puts below them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    /// The input-mode picker (0) or a directory/config field (1-3).
    Io(usize),
    Filter(FilterFocus),
    Nifti(NiftiFocus),
    Dicom(DicomFocus),
    /// The example picker's cursor: the Download button or an acquisition.
    Example(usize),
}

impl Hit {
    /// Whether it belongs to a pop-up (and so answers while one is open).
    fn in_popup(self) -> bool {
        matches!(self, Hit::Picker { .. } | Hit::Button(_) | Hit::AdviceRow(_) | Hit::Popup)
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
            MouseEventKind::ScrollUp => self.wheel(KeyCode::Up, ev.column, ev.row),
            MouseEventKind::ScrollDown => self.wheel(KeyCode::Down, ev.column, ev.row),
            _ => {}
        }
    }

    /// The wheel: it scrolls the Methods page under the mouse, or the settings under it while
    /// focus is on the sections; otherwise `↑`/`↓`, but never off the page onto the strip or the
    /// tab bar.
    fn wheel(&mut self, code: KeyCode, x: u16, y: u16) {
        if self.is_typing() {
            return;
        }
        let up = code == KeyCode::Up;
        let step = |s: &mut usize| *s = if up { s.saturating_sub(1) } else { *s + 1 };
        if !self.popup_open() {
            match self.hits.at(x, y).map(|(_, h)| h) {
                Some(Hit::Page | Hit::PageButton) => {
                    self.status_message = None;
                    return step(self.methods_page_scroll());
                }
                // Focus stays on the sections; the settings scroll under it (the drawer keeps
                // the view where it is while no setting has focus).
                Some(Hit::Rows { .. }) if self.on_subtabs() => {
                    self.status_message = None;
                    return step(&mut self.pipeline_state.scroll_offset);
                }
                _ => {}
            }
        }
        let (tab, strip) = (self.tab_focus, self.subtab_focus);
        self.handle_key(key(code));
        if !self.popup_open() && ((self.tab_focus && !tab) || (self.subtab_focus && !strip)) {
            self.tab_focus = tab;
            self.subtab_focus = strip;
        }
    }

    fn click(&mut self, x: u16, y: u16) {
        let found = self.hits.at(x, y);
        // A click outside an open pop-up closes it, as Esc does.
        if self.popup_open() && !found.is_some_and(|(_, h)| h.in_popup()) {
            self.handle_key(key(KeyCode::Esc));
            return;
        }
        let Some((area, hit)) = found else { return };
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
                let Some(i) = super::pipelines_tab::list_entry(line) else { return };
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
            Hit::AdviceRow(i) => {
                if let Some(m) = self.rec_modal.as_mut().or(self.refs.modal.as_mut()) {
                    m.cursor = i;
                    m.button = None;
                    m.toggle(i);
                }
            }
            Hit::Popup => {}
            Hit::Input(target) => self.click_input(target),
            Hit::Page => (self.tab_focus, self.subtab_focus) = (false, false),
            Hit::PageButton => {
                (self.tab_focus, self.subtab_focus) = (false, false);
                self.press_page_button();
            }
        }
    }

    /// What the Input page has focused.
    pub(super) fn input_target(&self) -> InputTarget {
        if self.active_field < Self::INPUT_IO_FIELDS {
            return InputTarget::Io(self.active_field);
        }
        match self.input_mode {
            InputMode::Bids => InputTarget::Filter(self.filter_state.focus),
            InputMode::NIfTI => InputTarget::Nifti(self.nifti_state.focus),
            InputMode::DicomToBids => InputTarget::Dicom(self.dicom_state.focus),
            InputMode::Example => InputTarget::Example(self.example_state.cursor),
        }
    }

    /// A click on a line of the Input page: focus it, then act as its key does.
    fn click_input(&mut self, target: InputTarget) {
        let current = self.input_target();
        if self.is_typing() {
            if current == target {
                return;
            }
            // Typing elsewhere ends as Enter ends it, keeping (and applying) the text.
            self.handle_key(key(KeyCode::Enter));
        }
        let again = current == target && !self.tab_focus;
        self.tab_focus = false;
        let below = Self::INPUT_IO_FIELDS;
        match target {
            InputTarget::Io(i) => self.active_field = i,
            InputTarget::Filter(f) => (self.active_field, self.filter_state.focus) = (below, f),
            InputTarget::Nifti(f) => (self.active_field, self.nifti_state.focus) = (below, f),
            InputTarget::Dicom(f) => (self.active_field, self.dicom_state.focus) = (below, f),
            InputTarget::Example(c) => (self.active_field, self.example_state.cursor) = (below, c),
        }
        let code = match target {
            InputTarget::Io(_) => Some(KeyCode::Enter),
            InputTarget::Filter(FilterFocus::TreeNode(i)) if matches!(self.filter_state.visible_rows().get(i), Some(TreeRow::Run { .. })) => {
                Some(KeyCode::Char(' '))
            }
            InputTarget::Filter(FilterFocus::TreeNode(_)) => again.then_some(KeyCode::Enter),
            InputTarget::Filter(_) => Some(KeyCode::Enter),
            InputTarget::Nifti(NiftiFocus::MagFile(_) | NiftiFocus::PhaseFile(_)) => None,
            InputTarget::Nifti(NiftiFocus::ConvertButton) => again.then_some(KeyCode::Enter),
            InputTarget::Nifti(_) => Some(KeyCode::Enter),
            // A tick box ticks on the first click, as the example acquisitions do.
            InputTarget::Dicom(super::app::DicomFocus::KeepUids) => Some(KeyCode::Char(' ')),
            InputTarget::Dicom(_) => again.then_some(KeyCode::Enter),
            InputTarget::Example(c) if c != ExampleState::BUTTON => Some(KeyCode::Char(' ')),
            InputTarget::Example(_) => again.then_some(KeyCode::Enter),
        };
        if let Some(code) = code {
            self.handle_key(key(code));
        }
    }

    /// Press button `i` of the pop-up that has the keys: select it, then Enter.
    fn press_button(&mut self, i: usize) {
        if self.update_modal.is_some() || self.news_modal.is_some() {
            return;
        }
        if let Some(m) = self.tip_modal.as_mut() {
            m.cursor = i;
        } else if self.rec_modal.is_some() || self.refs.modal.is_some() {
            // Apply, then Keep mine.
            self.handle_key(key(KeyCode::Char(if i == 0 { 'a' } else { 'k' })));
            return;
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
        if self.pipeline_state.mask_value_editing {
            if pos == self.focus() {
                return;
            }
            // A click on another row leaves the number being typed as it was.
            self.pipeline_state.mask_value_editing = false;
        }
        let again = pos == self.focus() && !self.tab_focus && !self.subtab_focus;
        self.tab_focus = false;
        self.subtab_focus = false;
        self.set_focus(pos);
        // A click does what Enter or Space does: a switch flips, a choice opens its list, a
        // number or text starts editing. Anything else (adding a step or a mask, the run button)
        // needs a second click, on the row already focused.
        let code = match &r {
            PipelineRow::Toggle { .. } => Some(KeyCode::Char(' ')),
            PipelineRow::Param { .. } | PipelineRow::AdvancedFooter { .. } => Some(KeyCode::Enter),
            _ if self.selector_modal(&r).is_some() || self.pipeline_state.mask_value_text(&r).is_some() => Some(KeyCode::Enter),
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
    use crate::tui::app::{Page, TAB_PIPELINE, TAB_RECONSTRUCTION};
    use ratatui::{backend::TestBackend, Terminal};

    /// Draw a frame (recording where things are) and return its text, one string per row.
    fn draw(app: &mut App) -> Vec<String> {
        draw_sized(app, 140, 42)
    }

    fn draw_sized(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
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
    fn tabs_and_sections_switch_on_a_click() {
        let mut app = App::new_for_trial();
        click(&mut app, "3 Reconstruction");
        assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
        click(&mut app, "   Motion ");
        assert_eq!(app.page(), Some(Page::Motion));
        assert!(app.on_subtabs(), "a click on a section focuses the sidebar");
        // A group's heading opens its first section.
        click(&mut app, " OTHER OUTPUTS");
        assert_eq!(app.page(), Some(Page::Magnitude));
        click(&mut app, "4 Execution");
        assert_eq!(app.active_tab, crate::tui::app::TAB_EXECUTION);
    }

    /// A picker button opens its list; an entry is highlighted by one click and chosen by the
    /// next; a pop-up's button is pressed by a click, and while it is open the tabs do not answer.
    #[test]
    fn pickers_entries_and_pop_up_buttons() {
        let mut app = App::new_for_trial();
        app.check_recommendations();
        click(&mut app, "3 Reconstruction");
        click(&mut app, "   Inversion ");
        click(&mut app, "QSM inversion");
        assert!(app.algo_modal.is_some(), "the picker opens");
        click(&mut app, "  tkd");
        assert!(app.algo_modal.is_some(), "one click highlights");
        click(&mut app, "▶ tkd");
        assert!(app.algo_modal.is_none(), "the second chooses");
        assert_eq!(app.field_select("qsm_algorithm"), crate::tui::app::QSM_PICKER_OPTIONS.iter().position(|o| *o == "tkd").unwrap());
        // HEIDI with TKD opens the cone threshold it requires, then HEIDI's reference pop-up
        // (LSQR as the authors' seed).
        click(&mut app, "HEIDI");
        app.check_recommendations();
        assert!(app.rec_modal.is_some(), "ticking HEIDI on TKD requires the cone threshold");
        click(&mut app, " Apply (1) ");
        assert!(app.rec_modal.is_none(), "the button applies it");
        app.check_recommendations();
        assert!(app.refs.modal.is_some(), "then LSQR as the authors' seed");
        click(&mut app, " Keep mine ");
        assert!(app.refs.modal.is_none(), "the button closes it");
    }

    /// A click outside an open pop-up closes it, as Esc does, and does nothing else: the tab
    /// clicked is not opened. A click inside it, away from its entries, leaves it open.
    #[test]
    fn a_click_outside_a_pop_up_closes_it() {
        let mut app = App::new_for_trial();
        click(&mut app, "3 Reconstruction");
        click(&mut app, "   Inversion ");
        let before = app.field_select("qsm_algorithm");
        click(&mut app, "QSM inversion");
        assert!(app.algo_modal.is_some());
        click(&mut app, "Esc cancel");
        assert!(app.algo_modal.is_some(), "a click inside it keeps it open");
        click(&mut app, "1 Input");
        assert!(app.algo_modal.is_none(), "a click outside closes it");
        assert_eq!(app.active_tab, TAB_RECONSTRUCTION, "and does nothing else");
        assert_eq!(app.field_select("qsm_algorithm"), before, "nothing is chosen");
    }

    #[test]
    fn a_checkbox_ticks_and_a_value_opens_for_editing() {
        let mut app = App::new_for_trial();
        click(&mut app, "3 Reconstruction");
        click(&mut app, "   Inversion ");
        let before = app.pipeline_state.heidi_enabled;
        click(&mut app, "HEIDI");
        assert_ne!(app.pipeline_state.heidi_enabled, before);
        click(&mut app, "Max iter");
        assert!(app.pipeline_state.editing, "a value opens for editing");
        click(&mut app, "Tolerance");
        assert!(app.pipeline_state.editing, "another value: that one");
        click(&mut app, "3 Reconstruction");
        assert!(!app.pipeline_state.editing, "a click elsewhere ends editing");
    }

    #[test]
    fn a_pipeline_is_previewed_and_applied_by_clicks() {
        let mut app = App::new_for_trial();
        click(&mut app, "2 Pipeline");
        assert_eq!(app.active_tab, TAB_PIPELINE);
        click(&mut app, "MEDI toolbox");
        assert!(app.pipelines.preview.is_none(), "one click highlights");
        click(&mut app, "▸ MEDI toolbox");
        assert!(app.pipelines.preview.is_some(), "the second opens the preview");
        click(&mut app, "Apply MEDI toolbox");
        assert!(app.pipelines.preview.is_none());
        assert_eq!(app.pipeline_status().text(), "MEDI toolbox");
    }

    /// The wheel moves through the rows, and stops at the first instead of leaving the page.
    #[test]
    fn the_wheel_stays_on_the_page() {
        let mut app = App::new_for_trial();
        click(&mut app, "3 Reconstruction");
        click(&mut app, "   Inversion ");
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

    /// The wheel over `text` (the first place it is drawn).
    fn wheel_over(app: &mut App, kind: MouseEventKind, text: &str) {
        let screen = draw(app);
        let (x, y) = screen.iter().enumerate().find_map(|(y, row)| {
            row.find(text).map(|b| (row[..b].chars().count() as u16, y as u16))
        }).unwrap_or_else(|| panic!("{text:?} is not on the screen:\n{}", screen.join("\n")));
        app.handle_mouse(MouseEvent { kind, column: x, row: y, modifiers: KeyModifiers::NONE });
    }

    /// The Methods tab: with focus on its sidebar, the wheel over the text scrolls the text and
    /// leaves focus on the sidebar; the button above it copies the text and says so; `c` copies
    /// from the sidebar too. On the Graph page the button opens the graph.
    #[test]
    fn the_methods_page_scrolls_under_the_wheel_and_its_buttons_press() {
        use crate::tui::app::TAB_METHODS;
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new_for_trial();
        app.pipeline_view.svg_dir = dir.path().to_path_buf();
        app.pipeline_view.opener = |_| Ok(super::super::pipeline_view::Opened::Viewer);
        click(&mut app, "5 Methods");
        assert_eq!(app.active_tab, TAB_METHODS);
        assert!(app.on_subtabs() && app.sidebar(), "focus on the sidebar");
        assert!(draw(&mut app).iter().any(|r| r.contains(" Copy methods text   c")), "the button and its key");
        wheel_over(&mut app, MouseEventKind::ScrollDown, "processing was performed");
        assert_eq!(app.methods_scroll_offset, 1, "the wheel over the text scrolls it");
        assert!(app.on_subtabs() && app.page() == Some(Page::MethodsText), "focus stays on the sidebar");
        wheel_over(&mut app, MouseEventKind::ScrollUp, "Copy methods text");
        wheel_over(&mut app, MouseEventKind::ScrollUp, "Copy methods text");
        assert_eq!(app.methods_scroll_offset, 0);
        assert!(app.on_subtabs(), "the wheel at the top does not move focus");

        click(&mut app, "Copy methods text");
        assert_eq!(app.clipboard.take(), Some(crate::tui::command::methods_text(&app).unwrap()));
        assert!(!app.on_subtabs(), "a click on the button goes into the page");
        let screen = draw(&mut app);
        assert!(screen.iter().any(|r| r.contains(" Copy methods text   ✓ Copied")), "{}", screen.join("\n"));
        app.handle_key(KeyEvent::from(KeyCode::Left));
        assert!(app.on_subtabs());
        assert!(draw(&mut app).iter().any(|r| r.contains(" Copy methods text   c")), "the next key clears it");
        app.handle_key(KeyEvent::from(KeyCode::Char('c')));
        assert!(app.clipboard.take().is_some() && app.on_subtabs(), "c copies from the sidebar");

        click(&mut app, "Graph");
        assert_eq!(app.page(), Some(Page::Pipeline));
        assert!(draw(&mut app).iter().any(|r| r.contains(" Open graph   s")));
        click(&mut app, "Open graph");
        assert!(app.status_message.as_deref().unwrap().ends_with("and opened"));
        assert!(dir.path().read_dir().unwrap().next().is_some(), "the SVG is written");
    }

    /// With focus on the Reconstruction tab's sections, the wheel over the settings scrolls them
    /// and focus stays on the sections.
    #[test]
    fn the_settings_scroll_under_the_wheel_from_the_sections() {
        let mut app = App::new_for_trial();
        click(&mut app, "3 Reconstruction");
        click(&mut app, "   Inversion ");
        app.handle_key(KeyEvent::from(KeyCode::Left));
        assert!(app.on_subtabs());
        let section = app.page();
        // Short enough that the settings overflow.
        for _ in 0..3 {
            draw_sized(&mut app, 80, 16);
            app.handle_mouse(MouseEvent { kind: MouseEventKind::ScrollDown, column: 40, row: 6, modifiers: KeyModifiers::NONE });
        }
        draw_sized(&mut app, 80, 16);
        assert!(app.on_subtabs() && app.page() == section, "focus and section stay");
        assert!(app.pipeline_state.scroll_offset > 0, "the settings scrolled:\n{}", draw_sized(&mut app, 80, 16).join("\n"));
    }

    /// The Input page: the mode picker and the directories open on a click; a click elsewhere
    /// ends typing; a run ticks; Include opens for typing.
    #[test]
    fn the_input_page_answers_clicks() {
        use crate::tui::app::{FilterFocus, InputMode, TreeRow, TAB_INPUT};
        let dir = tempfile::tempdir().unwrap();
        crate::testutils::create_multi_orientation_bids(dir.path());
        let mut app = App::new_for_trial();
        assert_eq!(app.active_tab, TAB_INPUT);
        click(&mut app, "Input mode:");
        assert!(app.algo_modal.is_some(), "the mode picker opens");
        app.algo_modal = None;
        click(&mut app, "BIDS directory:");
        assert!(app.editing && app.active_field == 1, "the directory opens for typing");
        app.form.bids_dir = dir.path().to_string_lossy().into_owned();
        app.cursor_pos = app.form.bids_dir.len();
        click(&mut app, "Config file:");
        assert_eq!(app.active_field, 3, "a click elsewhere ends typing and opens that field");
        click(&mut app, "Output directory:");
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.editing);
        app.filter_state.maybe_rescan(&app.form.bids_dir.clone());
        let rows = app.filter_state.visible_rows();
        let run = rows.iter().position(|r| matches!(r, TreeRow::Run { .. })).expect("a run");
        let name = {
            let tree = app.filter_state.tree.as_ref().unwrap();
            let TreeRow::Run { sub, ses, run: r } = rows[run] else { unreachable!() };
            match ses {
                Some(sei) => tree.subjects[sub].sessions[sei].runs[r].display.clone(),
                None => tree.subjects[sub].runs[r].display.clone(),
            }
        };
        let before = app.filter_state.tree.as_ref().unwrap().selected_runs();
        click(&mut app, &name);
        assert_eq!(app.filter_state.focus, FilterFocus::TreeNode(run));
        assert_ne!(app.filter_state.tree.as_ref().unwrap().selected_runs(), before, "a click ticks a run");
        click(&mut app, "Include:");
        assert!(app.filter_state.include_editing, "Include opens for typing");
        // The example page: an acquisition ticks; the Download button is focused, not pressed.
        app.filter_state.include_editing = false;
        app.input_mode = InputMode::Example;
        let example = crate::example::EXAMPLES[1].id;
        let ticked = app.example_state.selected[1];
        click(&mut app, example);
        assert_eq!(app.example_state.selected[1], !ticked);
        click(&mut app, "[ Download ]");
        assert_eq!(app.example_state.cursor, crate::tui::app::ExampleState::BUTTON);
        assert_eq!(app.example_state.status, crate::tui::app::ConvertStatus::Idle, "one click only focuses it");
    }

    #[test]
    fn the_mouse_can_be_left_to_the_terminal() {
        assert_eq!(OPT_OUT_ENV, "QSMXT_NO_MOUSE");
    }
}
