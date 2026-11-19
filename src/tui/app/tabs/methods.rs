//! The Methods tab: copying the methods text, and its keys.

use super::*;

/// The footer's confirmation after the methods text is copied.
pub const METHODS_COPIED: &str = "Methods copied to clipboard";

impl App {
    /// Put the methods text for the current settings on the clipboard (written out by the main
    /// loop as an OSC 52 sequence, which reaches the local clipboard through SSH).
    pub fn copy_methods(&mut self) {
        match crate::tui::command::methods_text(self) {
            Ok(text) => {
                self.clipboard = Some(text);
                self.status_message = Some(METHODS_COPIED.to_string());
            }
            Err(e) => self.error_message = Some(format!("Nothing copied: {e}")),
        }
    }

    /// Whether the methods text was just copied (the button says so until the next key).
    pub fn methods_copied(&self) -> bool {
        self.status_message.as_deref() == Some(METHODS_COPIED)
    }

    /// The Methods tab's scroll position on the page shown (the text or the graph).
    pub(crate) fn methods_page_scroll(&mut self) -> &mut usize {
        if self.page() == Some(Page::Pipeline) { &mut self.pipeline_view.scroll } else { &mut self.methods_scroll_offset }
    }

    /// The Methods tab's button: copy the methods text, or open the graph.
    pub(crate) fn press_page_button(&mut self) {
        if self.page() == Some(Page::Pipeline) { self.open_pipeline_svg() } else { self.copy_methods() }
    }

    /// The Methods tab: its sub-tab strip, then the methods text (scrolling, copying) or the
    /// pipeline graph (scrolling, the graph as SVG). Both are read-only.
    pub(super) fn handle_methods_tab_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return self.should_quit = true,
            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => return,
            KeyCode::Char('[') => return self.cycle_page(-1),
            KeyCode::Char(']') => return self.cycle_page(1),
            KeyCode::F(5) => return self.try_run(),
            // The page's button works from the sidebar too: `c` copies the text, `s` opens the
            // graph (Enter as well, inside the page).
            KeyCode::Char('c') | KeyCode::Char('y') if self.page() != Some(Page::Pipeline) => return self.copy_methods(),
            KeyCode::Char('s') if self.page() == Some(Page::Pipeline) => return self.open_pipeline_svg(),
            _ => {}
        }
        // The sidebar, as the Reconstruction tab's: ↑/↓ choose the page, → or Enter go into it,
        // ↑ from the first goes up to the tab bar.
        if self.on_subtabs() && self.sidebar() {
            let first = self.page() != Some(Page::Pipeline);
            match key.code {
                KeyCode::Up | KeyCode::Char('k') if first => self.tab_focus = true,
                KeyCode::Up | KeyCode::Char('k') => self.cycle_page(-1),
                KeyCode::Down | KeyCode::Char('j') if first => self.cycle_page(1),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.subtab_focus = false,
                _ => {}
            }
            return;
        }
        if self.on_subtabs() {
            match key.code {
                KeyCode::Left => self.cycle_page(-1),
                KeyCode::Right => self.cycle_page(1),
                KeyCode::Up | KeyCode::Char('k') => self.tab_focus = true,
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => self.subtab_focus = false,
                _ => {}
            }
            return;
        }
        let pipeline = self.page() == Some(Page::Pipeline);
        let sidebar = self.sidebar();
        let scroll = if pipeline { &mut self.pipeline_view.scroll } else { &mut self.methods_scroll_offset };
        match key.code {
            // ← goes back to the sidebar, and so does ↑ from the top.
            KeyCode::Left | KeyCode::Char('h') if sidebar => self.subtab_focus = true,
            KeyCode::Up | KeyCode::Char('k') if *scroll == 0 => self.subtab_focus = true,
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *scroll += 1,
            KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
            KeyCode::PageDown => *scroll += 10,
            KeyCode::Home => *scroll = 0,
            KeyCode::Enter => self.press_page_button(),
            _ => {}
        }
    }
}
