//! The Pipeline tab: start from QSMxT's defaults or an original toolbox's pipeline, then adjust
//! any step on the tabs that follow ([`qsmxt_engine::pipelines`]).
//!
//! - **The list** is the engine's ([`pipelines::pipelines`]): QSMxT's defaults, then the
//!   toolboxes. Beside it, the step-by-step comparison of the highlighted pipeline with the form's
//!   settings ([`pipelines::changes`]).
//! - **Choosing** (Enter) opens a preview of every change ([`PreviewModal`]): Apply (preselected:
//!   choosing the pipeline is the request) or Cancel. Nothing changes before Apply.
//! - **Applying** resets every reconstruction page, shown settings and hidden ones
//!   ([`pipelines::covers`]; not the Input page or the Outputs and Execution tabs), then sets each
//!   change on its row, as the recommendation pop-up's Apply does, until the form's config follows
//!   the pipeline. The choice is remembered in the form (and so in the config:
//!   `reference_pipeline`). The reasoned recommendations its choices trigger (PDF's iteration
//!   count), which are not part of it, then open their pop-up as after any change.
//! - **The status** ([`pipelines::status`]: "MEDI toolbox", "MEDI toolbox · 1 departure",
//!   "Custom") sits in the header's rule on every tab, cached by the form's fingerprint. `P` on a
//!   settings page comes here.

use crossterm::event::{KeyCode, KeyEvent};
use qsmxt_engine::pipelines::{self, Change, Pipeline, Status};
use qsmxt_engine::schema::{Section, ViewFacts};
use serde_json::Value;

use super::app::{App, Page, PipelineRow};

/// The Pipeline tab's state.
#[derive(Debug, Clone, Default)]
pub struct PipelinesState {
    /// The highlighted pipeline, an index into [`pipelines::pipelines`].
    pub cursor: usize,
    /// The preview of the changes choosing it makes, while open.
    pub preview: Option<PreviewModal>,
    /// The status, and the fingerprint of the form it was worked out for.
    cached: Option<(String, Status)>,
    /// The comparison beside the list, for the highlighted pipeline, with its fingerprint.
    compared: Option<(String, Result<Compared, String>)>,
}

/// What choosing a pipeline does to the form's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Compared {
    pub changes: Vec<Change>,
    /// The stages it resets to the defaults, by name.
    pub stages: Vec<&'static str>,
    /// Reasoned recommendations on its settings, which are not part of it: "PDF CG iterations:
    /// 300 (reason)".
    pub advice: Vec<String>,
}

/// The preview of the changes choosing a pipeline makes.
#[derive(Debug, Clone)]
pub struct PreviewModal {
    pub pipeline: Pipeline,
    pub compared: Compared,
    /// Apply (0) or Cancel (1).
    pub button: usize,
    pub scroll: u16,
}

/// The page a stage's settings are on.
fn page_of(section: Section) -> Option<Page> {
    Some(match section {
        Section::Geometry => Page::Geometry,
        Section::Masking => Page::Masking,
        Section::Motion => Page::Motion,
        Section::MultiOrientation => Page::MultiOrientation,
        Section::FieldMap => Page::FieldMap,
        Section::Background => Page::Background,
        Section::Inversion => Page::Inversion,
        Section::Reference => Page::Reference,
        Section::Separation => Page::Separation,
        _ => return None,
    })
}

/// The texts a parameter row may take for `value`, the shortest first: a list of equal values as
/// one, then written out.
fn field_texts_of(value: &Value) -> Vec<String> {
    let plain = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    match value {
        Value::Array(xs) => {
            let all = xs.iter().map(plain).collect::<Vec<_>>().join(" ");
            if xs.len() > 1 && xs.iter().all(|x| Some(x) == xs.first()) {
                vec![plain(&xs[0]), all]
            } else {
                vec![all]
            }
        }
        other => vec![plain(other)],
    }
}

impl App {
    /// What the settings view knows about the data: the echo count, field strength, manufacturer.
    pub fn view_facts(&self) -> ViewFacts {
        let f = self.rule_facts();
        ViewFacts {
            num_echoes: f.echo_count,
            field_strength: f.field_strength,
            manufacturer: f.manufacturer,
            ..Default::default()
        }
    }

    /// Which pipeline the form follows, cached until the form changes.
    pub fn pipeline_status(&mut self) -> Status {
        let print = self.rec_fingerprint();
        if let Some((p, s)) = &self.pipelines.cached {
            if *p == print {
                return s.clone();
            }
        }
        let s = super::command::config_from_app(self)
            .map(|c| pipelines::status(&c, &self.view_facts()))
            .unwrap_or(Status { pipeline: None, departures: vec![] });
        self.pipelines.cached = Some((print, s.clone()));
        s
    }

    /// The status the header shows, from the cache ([`Self::pipeline_status`] fills it each frame).
    pub fn pipeline_status_text(&self) -> Option<String> {
        self.pipelines.cached.as_ref().map(|(_, s)| s.text())
    }

    /// The highlighted pipeline.
    pub fn highlighted_pipeline(&self) -> Pipeline {
        let list = pipelines::pipelines();
        list[self.pipelines.cursor.min(list.len() - 1)].clone()
    }

    /// What choosing the highlighted pipeline does to the form's settings, cached until either
    /// changes.
    pub fn highlighted_changes(&mut self) -> Result<Compared, String> {
        let id = self.highlighted_pipeline().id;
        let print = format!("{id}{}", self.rec_fingerprint());
        if let Some((p, c)) = &self.pipelines.compared {
            if *p == print {
                return c.clone();
            }
        }
        let c = self.compare(id);
        self.pipelines.compared = Some((print, c.clone()));
        c
    }

    fn compare(&self, id: &str) -> Result<Compared, String> {
        let facts = self.view_facts();
        let config = super::command::config_from_app(self).map_err(|e| e.to_string())?;
        let changes = pipelines::changes(&config, id, &facts)?;
        let (stages, _) = pipelines::covers(&config, id, &facts)?;
        let target = pipelines::target(&config, id, &facts)?;
        let advice = qsmxt_engine::recommendations::recommendations(&target, &self.rule_facts()).into_iter()
            .filter(|r| r.kind == qsmxt_engine::recommendations::Kind::Reasoned && !r.dismissed)
            .map(|r| format!("{} {}", r.label, r.recommended.name))
            .collect();
        Ok(Compared { changes, stages: stages.iter().map(|s| s.label()).collect(), advice })
    }

    /// The Pipeline tab's keys.
    pub fn handle_pipeline_tab_key(&mut self, key: KeyEvent) {
        if self.pipelines.preview.is_some() {
            return self.handle_pipeline_preview_key(key);
        }
        let n = pipelines::pipelines().len();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ if self.switch_tab(key) => {}
            KeyCode::F(5) => self.try_run(),
            KeyCode::Up | KeyCode::Char('k') if self.pipelines.cursor == 0 => self.tab_focus = true,
            KeyCode::Up | KeyCode::Char('k') => self.pipelines.cursor -= 1,
            KeyCode::Down | KeyCode::Char('j') => self.pipelines.cursor = (self.pipelines.cursor + 1).min(n - 1),
            KeyCode::Home => self.pipelines.cursor = 0,
            KeyCode::End => self.pipelines.cursor = n - 1,
            KeyCode::Enter => self.open_pipeline_preview(),
            _ => {}
        }
    }

    /// Open the preview of choosing the highlighted pipeline. A pipeline the form already
    /// follows has nothing to change, and says so.
    pub fn open_pipeline_preview(&mut self) {
        let pipeline = self.highlighted_pipeline();
        match self.highlighted_changes() {
            Err(e) => self.error_message = Some(e),
            Ok(c) if c.changes.is_empty() && self.pipeline_status().pipeline.as_ref() == Some(&pipeline) => {
                self.status_message = Some(format!("The settings already follow {}", pipeline.name));
            }
            Ok(compared) => self.pipelines.preview = Some(PreviewModal { pipeline, compared, button: 0, scroll: 0 }),
        }
    }

    fn handle_pipeline_preview_key(&mut self, key: KeyEvent) {
        let Some(m) = self.pipelines.preview.as_mut() else { return };
        match key.code {
            KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => m.button = 0,
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => m.button = 1,
            KeyCode::Up | KeyCode::Char('k') => m.scroll = m.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => m.scroll += 1,
            KeyCode::Esc => self.pipelines.preview = None,
            KeyCode::Enter => {
                let Some(m) = self.pipelines.preview.take() else { return };
                if m.button == 0 {
                    match self.choose_pipeline(m.pipeline.id) {
                        Ok(()) => self.status_message = Some(format!("Pipeline: {}", m.pipeline.name)),
                        Err(e) => self.error_message = Some(e),
                    }
                }
            }
            _ => {}
        }
    }

    /// Choose pipeline `id`: reset the pages it covers, then set each of its settings on its row,
    /// until the form's config follows it. The form is left as it was if that cannot be done.
    pub fn choose_pipeline(&mut self, id: &str) -> Result<(), String> {
        let facts = self.view_facts();
        let config = super::command::config_from_app(self).map_err(|e| e.to_string())?;
        let (stages, _) = pipelines::covers(&config, id, &facts)?;
        let mut trial = self.clone_form();
        for page in stages.iter().filter_map(|s| page_of(*s)) {
            trial.reset_page(page);
        }
        // And the settings out of view (under an option that is off), which would otherwise come
        // back with their old values when it is turned on.
        for s in qsmxt_engine::schema::settings().iter().filter(|s| stages.contains(&s.section)) {
            if let Some(field) = super::app::field_of_path(s.path) {
                trial.reset_field(field);
            }
        }
        trial.pipeline_state.reference_pipeline = (id != pipelines::QSMXT_DEFAULTS).then(|| id.to_string());
        // A change can bring rows into view (PDF's settings once PDF is chosen): set what can be
        // set, then look again.
        let remaining = |app: &App| -> Result<Vec<Change>, String> {
            let c = super::command::config_from_app(app).map_err(|e| e.to_string())?;
            pipelines::changes(&c, id, &facts)
        };
        let mut left = remaining(&trial)?;
        while !left.is_empty() {
            let before = left.len();
            for ch in &left {
                trial.set_setting(ch.path, &ch.to);
            }
            left = remaining(&trial)?;
            if left.len() >= before {
                break;
            }
        }
        if !left.is_empty() {
            let names: Vec<&str> = left.iter().map(|c| c.label).collect();
            return Err(format!("{} cannot be set here: {}", pipelines::pipeline(id).map_or(id, |p| p.name), names.join(", ")));
        }
        self.form = trial.form;
        self.pipeline_state = trial.pipeline_state;
        self.filter_state = trial.filter_state;
        self.pipelines.cached = None;
        // The recommendations its choices trigger, which are not part of it, are offered now.
        self.check_recommendations_after_pipeline();
        Ok(())
    }

    /// Set the setting at config `path` to `value` on the row that edits it: each value of a
    /// selector is tried until the config takes it. Returns whether it did.
    fn set_setting(&mut self, path: &str, value: &Value) -> bool {
        let takes = |app: &App| super::command::config_from_app(app).ok()
            .and_then(|c| qsmxt_engine::schema::get(&c, path)).as_ref() == Some(value);
        if takes(self) {
            return true;
        }
        // A mask recipe that no preset gives (the toolboxes' BET without erosion, QSMART's first
        // echo): set as the hand-edited recipe.
        if path == "masking.sections" {
            if let Ok(sections) = serde_json::from_value::<Vec<crate::pipeline::config::MaskSection>>(value.clone()) {
                let mut app = self.clone_form();
                app.pipeline_state.mask_preset = super::app::MASK_PRESET_CUSTOM;
                app.pipeline_state.mask_sections = sections;
                if takes(&app) {
                    self.form = app.form;
                    self.pipeline_state = app.pipeline_state;
                    self.filter_state = app.filter_state;
                    return true;
                }
            }
        }
        for (_, _, rows) in self.all_pages() {
            for row in rows {
                let field = match &row {
                    PipelineRow::AlgoSelect { field, .. } | PipelineRow::Toggle { field, .. } | PipelineRow::Param { field, .. } => *field,
                    _ => continue,
                };
                let row_path = if field == "mask_preset" { Some("masking.sections") } else { super::app::field_path(field) };
                if row_path != Some(path) {
                    continue;
                }
                let mut candidates: Vec<App> = Vec::new();
                match &row {
                    PipelineRow::AlgoSelect { options, .. } => {
                        for i in 0..options.len() {
                            let mut app = self.clone_form();
                            // A value of this setting only: the inversion's LSQR entries also say
                            // whether HEIDI is on.
                            let heidi = app.pipeline_state.heidi_enabled;
                            app.set_field_select(field, i);
                            app.pipeline_state.heidi_enabled = heidi;
                            candidates.push(app);
                        }
                    }
                    PipelineRow::Toggle { .. } => {
                        let mut app = self.clone_form();
                        app.toggle_field(field);
                        candidates.push(app);
                    }
                    _ => {
                        // A list of equal values reads as one where the row takes one for every
                        // axis, but a row that wants all three (STAR-QSM's pad, `0 0 0` for
                        // DECOMPOSE) takes them written out.
                        for text in field_texts_of(value) {
                            let mut app = self.clone_form();
                            if let Some(s) = app.field_text_mut(field) {
                                *s = text;
                            }
                            candidates.push(app);
                        }
                    }
                }
                if let Some(app) = candidates.into_iter().find(|a| takes(a)) {
                    self.form = app.form;
                    self.pipeline_state = app.pipeline_state;
                    self.filter_state = app.filter_state;
                    return true;
                }
            }
        }
        false
    }
}

// ─── Drawing ───

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

fn muted() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// The changes, under the sub-tab each is on, as current → pipeline value.
fn change_lines(changes: &[Change]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut section = None;
    let label_w = changes.iter().map(|c| c.label.chars().count()).max().unwrap_or(0) + 2;
    for c in changes {
        if section != Some(c.section) {
            section = Some(c.section);
            lines.push(Line::from(Span::styled(format!("{} › {}", c.section.tab().label(), c.section.label()), muted().add_modifier(Modifier::BOLD))));
        }
        lines.push(Line::from(vec![
            Span::styled(format!("  {:label_w$}", c.label), Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(c.from_text.clone(), Style::default().fg(Color::Gray)),
            Span::styled(" → ", muted()),
            Span::styled(c.to_text.clone(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]));
    }
    lines
}

/// What the comparison says beyond the changes: what is reset, and the recommendations that are
/// not part of the pipeline.
fn summary_lines(c: &Compared) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !c.stages.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("{} start from QSMxT's defaults; every other setting stays as it is.", c.stages.join(", ")), muted())));
    }
    for a in &c.advice {
        lines.push(Line::from(vec![
            Span::styled("ⓘ ", Style::default().fg(Color::Cyan)),
            Span::styled(format!("Recommended, not part of the pipeline: {a} (press ! on the setting for the reason)"), Style::default().fg(Color::Gray)),
        ]));
    }
    lines
}

/// The Pipeline tab: the list on the left, the highlighted pipeline's comparison on the right.
pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let block = super::tabs::panel(app);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)]).split(inner);
    f.render_widget(Paragraph::new(Line::from(Span::styled(
        " Start from a pipeline, then adjust any step on the tabs that follow. Choosing one shows every change first.", muted()))), rows[0]);
    let cols = Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Length(40), Constraint::Min(20)]).split(rows[1]);

    let status = app.pipeline_status();
    let current = status.pipeline.as_ref().map(|p| p.id);
    let cursor = app.pipelines.cursor;
    let mut list = Vec::new();
    for (i, p) in pipelines::pipelines().into_iter().enumerate() {
        if i == 1 {
            list.push(Line::from(Span::styled(" Toolbox pipelines", muted().add_modifier(Modifier::BOLD))));
        }
        let sel = i == cursor && !app.tab_focus;
        let style = if sel { Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD) } else { Style::default() };
        let mut spans = vec![
            Span::styled(if sel { " ▶ " } else { "   " }, style.fg(Color::Cyan)),
            Span::styled(p.name.to_string(), style),
        ];
        if current == Some(p.id) {
            spans.push(Span::styled(" ●", style.fg(Color::Green)));
        }
        list.push(Line::from(spans));
    }
    list.push(Line::from(""));
    list.push(Line::from(vec![
        Span::raw("   "),
        Span::styled("Now: ", muted()),
        Span::styled(status.text(), Style::default().fg(if current.is_some() { Color::Green } else { Color::Gray })),
    ]));
    // The entries and the heading between them, for clicks.
    let entries = pipelines::pipelines().len() as u16 + 1;
    app.hits.add(Rect { height: entries.min(cols[0].height), ..cols[0] }, super::mouse::Hit::Pipelines);
    f.render_widget(Paragraph::new(list), cols[0]);

    let p = app.highlighted_pipeline();
    let mut right = vec![Line::from(Span::styled(p.toolbox.unwrap_or(p.name).to_string(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))];
    if let Some(source) = p.source {
        right.push(Line::from(Span::styled(source.to_string(), muted())));
    }
    right.push(Line::from(""));
    match app.highlighted_changes() {
        Err(e) => right.push(Line::from(Span::styled(e, Style::default().fg(Color::Red)))),
        Ok(c) => {
            if c.changes.is_empty() {
                right.push(Line::from(Span::styled("Your settings already follow it.", Style::default().fg(Color::Green))));
            } else {
                let n = c.changes.len();
                right.push(Line::from(Span::styled(
                    format!("Choosing it changes {n} setting{}:", if n == 1 { "" } else { "s" }), Style::default().add_modifier(Modifier::BOLD))));
                right.extend(change_lines(&c.changes));
            }
            right.push(Line::from(""));
            right.extend(summary_lines(&c));
        }
    }
    let side = Block::default().borders(Borders::LEFT).border_style(muted());
    let text = side.inner(cols[1]);
    f.render_widget(side, cols[1]);
    let pad = Rect { x: text.x + 1, width: text.width.saturating_sub(1), ..text };
    f.render_widget(Paragraph::new(right).wrap(Wrap { trim: false }), pad);
}

/// The preview: every change, what is reset, then Apply / Cancel.
pub fn draw_preview(f: &mut Frame, m: &mut PreviewModal, area: Rect, hits: &super::mouse::Hits) {
    let w = area.width.saturating_sub(8).min(110);
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Choosing {} changes these settings:", p_name(&m.pipeline)),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(""),
    ];
    lines.extend(change_lines(&m.compared.changes));
    lines.push(Line::from(""));
    lines.extend(summary_lines(&m.compared));
    if let Some(s) = m.pipeline.source {
        lines.push(Line::from(Span::styled(format!("Source: {s}"), muted())));
    }
    // Lines as they wrap in the text area.
    let text_w = w.saturating_sub(6).max(10) as usize;
    let body = lines.iter().map(|l| l.width().max(1).div_ceil(text_w) as u16).sum::<u16>();
    let h = (body + 6).min(area.height.saturating_sub(2));
    let popup = Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height.saturating_sub(h)) / 2, width: w, height: h };
    f.render_widget(Clear, popup);
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(format!(" {} ", m.pipeline.name), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let parts = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)]).split(inner);
    let max_scroll = body.saturating_sub(parts[0].height);
    m.scroll = m.scroll.min(max_scroll);
    let text = Rect { x: parts[0].x + 2, width: parts[0].width.saturating_sub(4), ..parts[0] };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((m.scroll, 0)), text);
    let button = |label: String, on: bool| if on {
        Span::styled(format!(" {label} "), Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(format!(" {label} "), Style::default().fg(Color::Gray))
    };
    let buttons = Line::from(vec![
        button(format!("Apply {}", m.pipeline.name), m.button == 0), Span::raw("    "), button("Cancel".into(), m.button == 1),
    ]).alignment(ratatui::layout::Alignment::Center);
    hits.add_buttons(parts[1], std::slice::from_ref(&buttons));
    f.render_widget(Paragraph::new(buttons), parts[1]);
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("←→", Style::default().fg(Color::Yellow)), Span::styled(" choose   ", muted()),
        Span::styled("Enter", Style::default().fg(Color::Yellow)), Span::styled(" select   ", muted()),
        Span::styled("Esc", Style::default().fg(Color::Yellow)), Span::styled(" close (nothing changes)", muted()),
    ])).alignment(ratatui::layout::Alignment::Center), parts[3]);
}

/// The pipeline's name as the preview's sentence starts.
fn p_name(p: &Pipeline) -> String {
    match p.toolbox {
        Some(t) if p.is_toolbox() => format!("the {} pipeline ({t})", p.name),
        _ => "QSMxT's defaults".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn screen(app: &mut App) -> String {
        use ratatui::{backend::TestBackend, Terminal};
        let mut t = Terminal::new(TestBackend::new(140, 42)).unwrap();
        t.draw(|f| super::super::ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer();
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>().join("\n")
    }

    fn four_echoes() -> App {
        let mut app = App::new_for_trial();
        app.filter_state.num_echoes = "4".into();
        app
    }

    /// Every pipeline can be set on the form's rows, and the form then follows it with no
    /// departures; QSMxT's defaults bring it back.
    #[test]
    fn every_pipeline_applies_through_the_form() {
        for p in pipelines::pipelines() {
            let mut app = four_echoes();
            app.choose_pipeline(p.id).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            let s = app.pipeline_status();
            assert_eq!(s.pipeline.as_ref().map(|x| x.id), Some(p.id), "{}: {}", p.id, s.text());
            assert!(s.departures.is_empty(), "{}: {:?}", p.id, s.departures);
            app.choose_pipeline(pipelines::QSMXT_DEFAULTS).unwrap();
            assert_eq!(app.pipeline_status().text(), "QSMxT defaults", "after {}", p.id);
        }
    }

    /// The recommendations a pipeline's choices trigger are not part of it, but open their
    /// pop-up once it is applied: PDF's iterations under the MEDI toolbox. DECOMPOSE's per-echo
    /// QSM is what its authors ran, so it is part of its pipeline and offers nothing.
    #[test]
    fn applying_offers_the_recommendations_it_does_not_include() {
        let mut app = four_echoes();
        app.check_recommendations();
        app.choose_pipeline("medi-toolbox").unwrap();
        let m = app.rec_modal.as_ref().expect("a recommendation pop-up");
        assert!(m.findings.iter().any(|r| r.setting == "bg_removal.pdf.n_cg"), "{:?}", m.findings);
        assert_eq!(m.cursor, Some(0), "its Apply is preselected");
        assert!(app.refs.modal.is_none() && app.refs.pending.is_empty(), "no reference pipeline pop-up");
        app.rec_modal = None;
        app.choose_pipeline("decompose").unwrap();
        assert!(super::super::command::config_from_app(&app).unwrap().qsm.per_echo, "DECOMPOSE's QSM is per echo");
        assert!(!app.rule_view().iter().any(|r| r.rule == "decompose.qsm-per-echo" && r.departs));
    }

    /// Nothing carries over from a pipeline chosen before, shown or hidden: the form's config is
    /// the same as choosing the pipeline on a fresh form, and QSMxT's defaults are the defaults.
    #[test]
    fn nothing_carries_over_between_pipelines() {
        let config = |app: &App| serde_json::to_value(super::super::command::config_from_app(app).unwrap()).unwrap();
        let fresh = |id: &str| { let mut app = four_echoes(); app.choose_pipeline(id).unwrap(); config(&app) };
        let ids = pipelines::ids();
        let mut app = four_echoes();
        for id in ids.iter().chain(ids.iter().rev()) {
            app.choose_pipeline(id).unwrap();
            assert_eq!(config(&app), fresh(id), "{id}");
        }
        app.choose_pipeline("chi-sep-ilsqr").unwrap();
        app.choose_pipeline(pipelines::QSMXT_DEFAULTS).unwrap();
        assert_eq!(config(&app), config(&four_echoes()), "QSMxT defaults are the defaults");
    }

    /// A value changed on a covered page goes back to the pipeline's; changing a setting after
    /// choosing is a departure; the command says the pipeline and the departure.
    #[test]
    fn departures_show_in_the_header_and_the_command() {
        let mut app = four_echoes();
        app.choose_pipeline("medi-toolbox").unwrap();
        assert!(super::super::command::build_command_string(&app).contains("--pipeline medi-toolbox"));
        *app.field_text_mut("medi_lambda").expect("MEDI's lambda row") = "500".into();
        assert_eq!(app.pipeline_status().text(), "MEDI toolbox · 1 departure");
        let header = screen(&mut app).lines().nth(1).unwrap_or_default().to_string();
        assert!(header.contains("Pipeline: MEDI toolbox · 1 departure"), "{header}");
        let cmd = super::super::command::build_command_string(&app);
        assert!(cmd.contains("--pipeline medi-toolbox --medi-lambda 500"), "{cmd}");
        app.choose_pipeline("medi-toolbox").unwrap();
        assert_eq!(app.pipeline_status().text(), "MEDI toolbox");
    }

    /// The tab: the list, the comparison, Enter's preview, and Apply.
    #[test]
    fn the_tab_previews_then_applies() {
        let mut app = four_echoes();
        key(&mut app, KeyCode::Char('2'));
        assert_eq!(app.active_tab, super::super::app::TAB_PIPELINE);
        let text = screen(&mut app);
        assert!(text.contains("Toolbox pipelines") && text.contains("Pipeline: QSMxT defaults"), "{text}");
        key(&mut app, KeyCode::Down);
        let text = screen(&mut app);
        assert!(text.contains("MEDI toolbox 2024.11.26") && text.contains("Choosing it changes"), "{text}");
        key(&mut app, KeyCode::Enter);
        let text = screen(&mut app);
        assert!(text.contains("Apply MEDI toolbox"), "{text}");
        assert_eq!(app.pipeline_status().text(), "QSMxT defaults", "nothing changes before Apply");
        key(&mut app, KeyCode::Enter);
        assert!(app.pipelines.preview.is_none());
        assert_eq!(app.pipeline_status().text(), "MEDI toolbox");
        // Esc on a preview changes nothing.
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.pipeline_status().text(), "MEDI toolbox");
        // P from a settings page comes back here.
        app.check_recommendations();
        let _ = screen(&mut app);
        key(&mut app, KeyCode::Char('3'));
        assert_eq!(app.active_tab, super::super::app::TAB_RECONSTRUCTION, "rec {:?} ref {:?}", app.rec_modal.is_some(), app.refs.modal.is_some());
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char('P'));
        assert_eq!(app.active_tab, super::super::app::TAB_PIPELINE);
    }
}

#[cfg(test)]
mod live {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    /// As the event loop runs it (a check and a draw before each key): after Apply the
    /// recommendation the pipeline does not include (PDF's iterations) opens its pop-up, with
    /// Apply preselected, and no reference pipeline pop-up follows it.
    #[test]
    fn keys_after_apply_like_the_event_loop() {
        let mut app = App::new();
        let step = |app: &mut App, code: KeyCode| {
            app.check_recommendations();
            let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 42)).unwrap();
            t.draw(|f| super::super::ui::draw(f, app)).unwrap();
            app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        for k in [KeyCode::Esc, KeyCode::Char('2'), KeyCode::Down, KeyCode::Enter, KeyCode::Enter] {
            step(&mut app, k);
        }
        assert_eq!(app.pipeline_status().text(), "MEDI toolbox");
        let m = app.rec_modal.as_ref().expect("Apply offers PDF's iteration count");
        assert!(m.findings.iter().any(|r| r.setting == "bg_removal.pdf.n_cg"), "{:?}", m.findings);
        assert_eq!(m.cursor, Some(0), "its Apply is preselected");
        step(&mut app, KeyCode::Enter);
        assert_eq!(super::super::command::config_from_app(&app).unwrap().bg_removal.pdf.n_cg, 300);
        step(&mut app, KeyCode::Char('3'));
        assert!(app.rec_modal.is_none() && app.refs.modal.is_none(), "no reference pipeline pop-up: the pipeline is the authors'");
        assert_eq!(app.active_tab, super::super::app::TAB_RECONSTRUCTION, "a digit after the pop-up switches tab");
    }
}
