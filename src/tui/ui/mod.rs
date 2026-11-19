//! Drawing: a frame of the [`App`], by part: the tabs (Input, the row-based tabs, Methods), the
//! footer, and the pop-ups.

mod help_bar;
mod input;
mod methods;
mod modals;
mod rows;
mod widgets;
#[cfg(test)]
pub(super) mod tests;
#[cfg(test)]
mod orientation_block_tests;

use help_bar::*;
use input::*;
use methods::*;
use modals::*;
use rows::*;
use widgets::*;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use super::app::{AlgoModal, App, NewsModal, PipelineRow, TipModal, UpdateModal, TAB_NAMES, TAB_INPUT, TAB_METHODS, TAB_PIPELINE};
use super::command;

/// Draw a frame of `app`, brought up to date first by [`App::prepare_frame`].
///
/// Drawing changes nothing the config or the form's settings depend on. What it does write is
/// what the frame turned out to be, which the input that follows it is matched against: where
/// each clickable thing is (`hits`, for the mouse), whether the sections went in a sidebar
/// (`sidebar_layout`) and how wide the notes are (`note_width`), both from the terminal's size,
/// and the scroll offsets that keep the focused row on screen (and a pop-up's list offset). The
/// keys follow what is on screen, so these are worked out where the screen is.
///
/// The config it shows is the one [`App::prepare_frame`] built ([`App::config`]).
pub fn draw(f: &mut Frame, app: &mut App) {
    // A frame drawn from a config the form no longer builds would show settings that are not the
    // form's: the form must not change between preparing a frame and drawing it.
    debug_assert_eq!(app.config, Some(command::config_from_app(app).map_err(|e| e.to_string())),
                     "the frame was not prepared for this form (App::prepare_frame)");
    app.in_frame = true;
    draw_frame(f, app);
    app.in_frame = false;
}

fn draw_frame(f: &mut Frame, app: &mut App) {
    // Each drawer records where its clickable things are.
    app.hits.clear();
    // The Reconstruction tab's sections are a sidebar when there is room beside the rows.
    app.sidebar_layout.set(f.area().width >= super::tabs::SIDEBAR_MIN_WIDTH);
    // Compute command preview height dynamically
    let cmd = command::build_command_string(app);
    let available_width = f.area().width.saturating_sub(4) as usize; // borders + padding
    let cmd_lines = cmd.len()
        .checked_div(available_width)
        .map(|v| (v + 1).clamp(1, 6))
        .unwrap_or(1);
    let preview_height = cmd_lines as u16 + 2; // +2 for borders

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(super::tabs::header_height()), // tab bar
            Constraint::Min(8),               // form
            Constraint::Length(preview_height), // command preview (dynamic)
            Constraint::Length(1),             // help bar
        ])
        .split(f.area());

    super::tabs::draw_header(f, app, chunks[0]);
    draw_form(f, app, chunks[1]);
    draw_command_preview_with(f, &cmd, chunks[2]);
    draw_help_bar(f, app, chunks[3]);

    if let Some(ref modal) = app.algo_modal {
        draw_algo_modal(f, modal, f.area(), &app.hits);
    }

    if let Some(ref mut modal) = app.rec_modal {
        draw_checklist_modal(f, modal, f.area(), &app.hits);
    }

    if let Some(ref mut modal) = app.refs.modal {
        draw_checklist_modal(f, modal, f.area(), &app.hits);
    }

    if let Some(ref mut modal) = app.pipelines.preview {
        super::pipelines_tab::draw_preview(f, modal, f.area(), &app.hits);
    }

    if let Some(ref modal) = app.tip_modal {
        draw_tip_modal(f, modal, f.area(), &app.hits);
    }

    if let Some(ref mut modal) = app.news_modal {
        draw_news_modal(f, modal, f.area(), &app.hits);
    }

    // Drawn last so it sits above the other pop-ups, matching the input routing.
    if let Some(ref modal) = app.update_modal {
        draw_update_modal(f, modal, f.area(), &app.hits);
    }
}

fn draw_form(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    if app.active_tab == TAB_INPUT {
        draw_input_tab(f, app, area);
        return;
    }
    if app.active_tab == TAB_PIPELINE {
        super::pipelines_tab::draw(f, app, area);
        return;
    }
    if App::is_rows_tab(app.active_tab) {
        draw_rows_tab(f, app, area);
        return;
    }
    if app.active_tab == TAB_METHODS {
        draw_methods_tab(f, app, area);
    }
}
