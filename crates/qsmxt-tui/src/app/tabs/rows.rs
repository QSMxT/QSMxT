//! Keys on the row-based tabs: moving, choosing, ticking and editing a row.

use super::*;

impl App {
    /// Keys on a row-based tab (all but Input and Methods): every row is a named
    /// field ([`PipelineRow`]) read and written through the `field_*` accessors, so one handler
    /// serves them all, whichever form a field lives in.
    pub(super) fn handle_rows_key(&mut self, key: KeyEvent) {
        if self.pipeline_state.editing {
            // Text editing mode for a parameter
            return self.handle_param_edit_key(key);
        }

        // Editing a mask row's number (a threshold value, BET's fractional intensity, a
        // refinement step's value, a signal-erode setting): Enter keeps it when it is valid,
        // Esc leaves the setting as it was.
        if self.pipeline_state.mask_value_editing {
            return self.handle_mask_value_key(key);
        }

        // In the sidebar: ↑/↓ choose the section, → or Enter go into its rows, ↑ from the first
        // goes up to the tab bar. ← does nothing: it is how the rows come back here.
        if self.on_subtabs() && self.sidebar() {
            let pages = self.pages(self.active_tab);
            let cur = self.page().and_then(|p| pages.iter().position(|&q| q == p)).unwrap_or(0);
            match key.code {
                KeyCode::Up | KeyCode::Char('k') if cur == 0 => {
                    self.tab_focus = true;
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') => return self.goto_page_index(cur - 1),
                KeyCode::Down | KeyCode::Char('j') => return self.goto_page_index(cur + 1),
                KeyCode::Home => return self.goto_page_index(0),
                KeyCode::End => return self.goto_page_index(pages.len() - 1),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                    self.subtab_focus = false;
                    self.set_focus(0);
                    return;
                }
                KeyCode::Char('q' | '[' | ']' | 'R') | KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab | KeyCode::F(5) => {}
                KeyCode::Char(c) if c.is_ascii_digit() => {}
                _ => return,
            }
        }

        // On the sub-tab strip: ←/→ choose the sub-tab, ↓ or Enter go into its rows, ↑ goes up
        // to the tab bar. Tab switching, `[`/`]`, `R`, F5 and quitting work as on the rows;
        // nothing else applies.
        if self.on_subtabs() && !self.sidebar() {
            match key.code {
                KeyCode::Left => return self.cycle_page(-1),
                KeyCode::Right => return self.cycle_page(1),
                KeyCode::Up | KeyCode::Char('k') => {
                    self.tab_focus = true;
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                    self.subtab_focus = false;
                    self.set_focus(0);
                    return;
                }
                KeyCode::Char('q' | '[' | ']' | 'R') | KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab | KeyCode::F(5) => {}
                KeyCode::Char(c) if c.is_ascii_digit() => {}
                _ => return,
            }
        }

        self.handle_row_key(key);
    }

    /// Typing in a parameter's value.
    fn handle_param_edit_key(&mut self, key: KeyEvent) {
        let field = match self.focused_row() {
            Some(PipelineRow::Param { field, .. }) => field,
            _ => {
                self.pipeline_state.editing = false;
                return;
            }
        };
        let mut cursor = self.pipeline_state.cursor;
        let len = self.field_text(field).len();
        match key.code {
            KeyCode::Esc | KeyCode::Enter => { self.pipeline_state.editing = false; return; }
            KeyCode::Char(c) => {
                if let Some(s) = self.field_text_mut(field) {
                    s.insert(cursor, c);
                    cursor += 1;
                }
            }
            KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                if let Some(s) = self.field_text_mut(field) {
                    let new_pos = word_boundary_left(s, cursor);
                    s.drain(new_pos..cursor);
                    cursor = new_pos;
                }
            }
            KeyCode::Backspace if cursor > 0 => {
                cursor -= 1;
                if let Some(s) = self.field_text_mut(field) {
                    s.remove(cursor);
                }
            }
            KeyCode::Delete if cursor < len => {
                if let Some(s) = self.field_text_mut(field) {
                    s.remove(cursor);
                }
            }
            KeyCode::Left => cursor = cursor.saturating_sub(1),
            KeyCode::Right if cursor < len => cursor += 1,
            KeyCode::Home => cursor = 0,
            KeyCode::End => cursor = len,
            _ => {}
        }
        self.pipeline_state.cursor = cursor;
    }

    /// Typing in a mask row's number.
    fn handle_mask_value_key(&mut self, key: KeyEvent) {
        let ps = &mut self.pipeline_state;
        let mut cursor = ps.cursor;
        match key.code {
            KeyCode::Esc => {
                ps.mask_value_editing = false;
                return;
            }
            KeyCode::Enter => {
                let text = ps.mask_value_buf.clone();
                let result = match self.focused_row() {
                    Some(row) => self.set_mask_value(&row, &text),
                    None => Ok(()),
                };
                match result {
                    Ok(()) => self.pipeline_state.mask_value_editing = false,
                    // Still editing, to correct it.
                    Err(e) => self.error_message = Some(e),
                }
                return;
            }
            KeyCode::Char(c) if c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E') => {
                ps.mask_value_buf.insert(cursor, c);
                cursor += 1;
            }
            KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                let new_pos = word_boundary_left(&ps.mask_value_buf, cursor);
                ps.mask_value_buf.drain(new_pos..cursor);
                cursor = new_pos;
            }
            KeyCode::Backspace if cursor > 0 => {
                cursor -= 1;
                ps.mask_value_buf.remove(cursor);
            }
            KeyCode::Delete if cursor < ps.mask_value_buf.len() => {
                ps.mask_value_buf.remove(cursor);
            }
            KeyCode::Left => cursor = cursor.saturating_sub(1),
            KeyCode::Right if cursor < ps.mask_value_buf.len() => cursor += 1,
            KeyCode::Home => cursor = 0,
            KeyCode::End => cursor = ps.mask_value_buf.len(),
            _ => {}
        }
        ps.cursor = cursor;
    }

    /// Keys on a row of the form (not the sub-tabs).
    fn handle_row_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}
            // `[` and `]`: the sub-tabs.
            KeyCode::Char('[') => self.cycle_page(-1),
            KeyCode::Char(']') => self.cycle_page(1),

            // ← from any row goes back to the sidebar (or the narrow terminal's section strip), on
            // the same section. No row changes its value with ←/→: a choice opens a list (Enter),
            // a number is typed (Enter), a switch flips (Space or Enter).
            KeyCode::Left | KeyCode::Char('h') => {
                if self.has_subtabs() {
                    self.subtab_focus = true;
                }
            }

            // Reorder mask ops with Ctrl+Up/Down (must be before regular Up/Down)
            KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Some(PipelineRow::MaskOpEntry { section: si, index: oi }) = self.focused_row() {
                    if self.move_mask_step(si, oi, true) {
                        let f = self.focus();
                        self.set_focus(f.saturating_sub(1));
                    }
                }
            }
            KeyCode::Down if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Some(PipelineRow::MaskOpEntry { section: si, index: oi }) = self.focused_row() {
                    if self.move_mask_step(si, oi, false) {
                        let max = self.focusable_rows().len().saturating_sub(1);
                        let f = self.focus();
                        if f < max { self.set_focus(f + 1); }
                    }
                }
            }

            // Navigation; up from the first row goes back to the sub-tab strip, or to the tab
            // bar on a tab without sub-tabs.
            KeyCode::Up | KeyCode::Char('k') => {
                let f = self.focus();
                if f == 0 {
                    if self.has_subtabs() {
                        self.subtab_focus = true;
                    } else {
                        self.tab_focus = true;
                    }
                } else {
                    self.set_focus(f.saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.focusable_rows().len().saturating_sub(1);
                let f = self.focus();
                if f < max {
                    self.set_focus(f + 1);
                }
            }

            // Interact
            KeyCode::Enter | KeyCode::Char(' ') if matches!(self.focused_row(), Some(PipelineRow::RunButton)) => self.try_run(),
            KeyCode::Enter | KeyCode::Char(' ') if matches!(self.focused_row(), Some(PipelineRow::AdvancedFooter { .. })) => self.toggle_advanced(),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_row(),

            // Delete a refinement step (on its rows), or the section a row belongs to (the
            // "Combined with" row before it, or its input, generator and generator settings) when
            // its mask has another.
            KeyCode::Char('d') | KeyCode::Delete => {
                match self.focused_row() {
                    Some(PipelineRow::MaskOpEntry { section: si, index: oi } | PipelineRow::MaskOpSignalErodeParam { section: si, index: oi, .. }) => {
                        if self.remove_mask_step(si, oi) {
                            let max = self.focusable_rows().len().saturating_sub(1);
                            if self.focus() > max { self.set_focus(max); }
                        }
                    }
                    Some(row) => {
                        if let Some(si) = MaskRecipe::removable_section(&row) {
                            self.remove_mask_section(si);
                        }
                    }
                    None => {}
                }
            }

            // `+` / `-` nudge a masking number by its step (typing it, with Enter, is the other way).
            KeyCode::Char('+' | '=' | '-') => {
                let delta = if key.code == KeyCode::Char('-') { -1 } else { 1 };
                if let Some(row) = self.focused_row() {
                    self.nudge_mask_value(&row, delta);
                }
            }

            // Reset focused field to default
            // `a` on the Export section: tick every map.
            KeyCode::Char('a') if self.page() == Some(Page::Export) => self.form.dicom_unticked.clear(),
            KeyCode::Char('r') => self.reset_row_field(),
            // Reset every setting of the tab to its default
            KeyCode::Char('R') => self.reset_current_tab(),

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    /// Enter or Space on a row: open its choices, type its value, or flip it.
    fn activate_row(&mut self) {
        let row = self.focused_row();
        // A choice opens a pop-up listing every option, the current one highlighted.
        if let Some(m) = row.as_ref().and_then(|r| self.selector_modal(r)) {
            self.algo_modal = Some(m);
            return;
        }
        // A mask row's number is typed.
        if let Some(text) = row.as_ref().and_then(|r| self.mask().mask_value_text(r)) {
            let ps = &mut self.pipeline_state;
            ps.cursor = text.len();
            ps.mask_value_buf = text;
            ps.mask_value_editing = true;
            return;
        }
        match row {
            Some(PipelineRow::Param { field, .. }) => {
                self.pipeline_state.editing = true;
                self.pipeline_state.cursor = self.field_text(field).len();
            }
            Some(PipelineRow::Toggle { field, .. }) => {
                let focused_field = self.focused_field_name();
                self.toggle_field(field);
                self.restore_focus(&focused_field);
            }
            // MindGrab's one setting, its test-time augmentation, is a switch.
            Some(PipelineRow::MaskOpGeneratorParam { section }) => self.toggle_mindgrab_tta(section),
            Some(PipelineRow::MaskOpAddSection { two_pass }) => self.add_mask_section(two_pass),
            _ => {}
        }
    }


    pub(crate) fn handle_editing_key(&mut self, key: KeyEvent) {
        let mut cursor = self.cursor_pos;

        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.editing = false;
                return;
            }
            KeyCode::Char(c) => {
                self.text_value_mut().insert(cursor, c);
                cursor += 1;
            }
            KeyCode::Backspace
                if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && cursor > 0 => {
                    let new_pos = word_boundary_left(self.text_value(), cursor);
                    self.text_value_mut().drain(new_pos..cursor);
                    cursor = new_pos;
                }
            KeyCode::Backspace
                if cursor > 0 => {
                    cursor -= 1;
                    self.text_value_mut().remove(cursor);
                }
            KeyCode::Delete => {
                let len = self.text_value().len();
                if cursor < len {
                    self.text_value_mut().remove(cursor);
                }
            }
            KeyCode::Left => {
                cursor = cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                let len = self.text_value().len();
                if cursor < len {
                    cursor += 1;
                }
            }
            KeyCode::Home => cursor = 0,
            KeyCode::End => cursor = self.text_value().len(),
            _ => {}
        }

        self.cursor_pos = cursor;
    }
}
