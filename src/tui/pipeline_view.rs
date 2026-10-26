//! The Pipeline tab: the graph of the run the current settings plan ([`qsmxt_engine::plan`] and
//! [`qsmxt_engine::pipeline_graph`], as `qsmxt run --dry` prints it), with the plan's peak memory,
//! the deep-learning models it needs and their download sizes, and what the scan cannot produce.
//! It is drawn from the settings each time, so it follows every change. A key writes the graph as
//! SVG ([`PipelineGraph::to_svg`]) and opens it in the system's viewer, or, where there is none to
//! open it with (over SSH, or with no display), says where it was written.
//!
//! The plan needs the scan's facts: those of the first selected run of a BIDS dataset, read once
//! for each run chosen; with none chosen, a typical scan's, and the view says so.

use std::path::{Path, PathBuf};

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use qsmxt_engine::pipeline_graph::text::{model_name, wrap_parts, Part};
use qsmxt_engine::pipeline_graph::{pipeline_graph, PipelineGraph};
use qsmxt_engine::plan::Plan;
use qsmxt_engine::scan::ScanFacts;

use super::app::{App, InputMode};

/// The facts of the scan the view plans for when no run is chosen: a 5-echo, 3 T whole-brain
/// scan with magnitude.
pub fn typical_scan() -> ScanFacts {
    ScanFacts {
        dims: (256, 256, 176),
        voxel_size: None,
        n_echoes: 5,
        echo_times: vec![0.004, 0.008, 0.012, 0.016, 0.020],
        has_magnitude: true,
        n_coils: 1,
        real_imaginary: false,
        field_strength: 3.0,
        b0_dir: None,
        mese_echo_times: None,
        custom_mask: false,
        custom_dseg: false,
        custom_qsm: false,
        custom_r2: false,
        custom_r2prime: false,
    }
}

/// Where the view gets the scan's facts from.
#[derive(Debug, Clone)]
pub enum ScanSource {
    /// A run of the dataset, by its key, and how many runs are selected.
    Run { key: String, selected: usize },
    /// No run is chosen (or its files cannot be read: why).
    Typical { why: Option<String> },
}

/// What [`open`] did with the SVG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// Handed to the system's viewer.
    Viewer,
    /// Not opened: there is nothing here to show it with (why).
    NoViewer(&'static str),
}

/// A run of a dataset: the dataset's folder, the run's key and the echo-count filter.
type RunId = (String, String, String);

/// The Pipeline tab's state.
pub struct PipelineView {
    pub scroll: usize,
    /// The last run read, by dataset and run key, kept so the files are read once per run chosen.
    run: Option<(RunId, Result<crate::bids::discovery::QsmRun, String>)>,
    /// Where the SVG is written.
    pub svg_dir: PathBuf,
    /// How the SVG is opened ([`open`]; tests put a stub here).
    pub opener: fn(&Path) -> Result<Opened, String>,
}

impl Default for PipelineView {
    fn default() -> Self {
        PipelineView { scroll: 0, run: None, svg_dir: std::env::temp_dir(), opener: open }
    }
}

/// Everything the tab shows, for the current settings.
pub struct Planned {
    pub source: ScanSource,
    pub facts: ScanFacts,
    pub plan: Plan,
    pub graph: PipelineGraph,
    /// With motion correction on, how many of the selected runs it runs for and which it is
    /// skipped for ([`qsmxt_engine::requirements::motion_correction_summary`]).
    pub motion: Option<String>,
}

/// Each selected run of a scanned BIDS dataset with motion correction on: its key, echo count (as
/// the echo limit leaves it) and whether it has magnitude, from its file names.
fn motion_runs(app: &App, config: &crate::pipeline::config::PipelineConfig) -> Vec<(String, usize, bool)> {
    let mut runs = Vec::new();
    let Some(tree) = app.filter_state.tree.as_ref() else { return runs };
    if app.input_mode != InputMode::Bids || !config.motion_correction.enabled {
        return runs;
    }
    let limit: Option<usize> = app.filter_state.num_echoes.trim().parse().ok();
    tree.for_each_run(|r| {
        if r.selected {
            runs.push((r.key_string.clone(), limit.map_or(r.n_echoes, |l| r.n_echoes.min(l)), r.has_magnitude));
        }
    });
    runs
}

/// The first selected run's key and how many runs are selected, in a scanned BIDS dataset.
fn first_selected(app: &App) -> Option<(String, usize)> {
    if app.input_mode != InputMode::Bids {
        return None;
    }
    let tree = app.filter_state.tree.as_ref()?;
    let mut first = None;
    tree.for_each_run(|r| {
        if r.selected && first.is_none() {
            first = Some(r.key_string.clone());
        }
    });
    first.map(|k| (k, tree.selected_runs()))
}

impl App {
    /// The plan and graph of the current settings, on the first selected run's scan (or a
    /// typical one), or why the settings are invalid.
    pub fn planned(&mut self) -> Result<Planned, String> {
        let config = super::command::config_from_app(self).map_err(|e| e.to_string())?;
        let (source, facts) = match first_selected(self) {
            Some((key, selected)) => {
                let id = (self.form.bids_dir.clone(), key.clone(), self.filter_state.num_echoes.clone());
                if self.pipeline_view.run.as_ref().is_none_or(|(k, _)| *k != id) {
                    let read = read_run(&id.0, &key, &id.2);
                    self.pipeline_view.run = Some((id, read));
                }
                match &self.pipeline_view.run.as_ref().expect("just read").1 {
                    Ok(run) => (ScanSource::Run { key, selected }, crate::pipeline::requirements::scan_facts(run, &config)),
                    Err(e) => (ScanSource::Typical { why: Some(format!("{key} cannot be read: {e}")) }, typical_scan()),
                }
            }
            None => (ScanSource::Typical { why: None }, typical_scan()),
        };
        let plan = qsmxt_engine::plan::plan(&config, &facts);
        let graph = pipeline_graph(&plan, &config, &facts, None);
        let motion = qsmxt_engine::requirements::motion_correction_summary(&motion_runs(self, &config));
        Ok(Planned { source, facts, plan, graph, motion })
    }

    /// Write the graph of the current settings as SVG and open it; the footer says what happened.
    pub fn open_pipeline_svg(&mut self) {
        let planned = match self.planned() {
            Ok(p) => p,
            Err(e) => {
                self.error_message = Some(format!("No graph: {e}"));
                return;
            }
        };
        let path = self.pipeline_view.svg_dir.join(format!("qsmxt-pipeline-{}.svg", std::process::id()));
        if let Err(e) = std::fs::write(&path, planned.graph.to_svg()) {
            self.error_message = Some(format!("Could not write {}: {e}", path.display()));
            return;
        }
        self.status_message = Some(match (self.pipeline_view.opener)(&path) {
            Ok(Opened::Viewer) => format!("Pipeline graph saved to {} and opened", path.display()),
            Ok(Opened::NoViewer(why)) => format!("Pipeline graph saved to {} ({why})", path.display()),
            Err(e) => format!("Pipeline graph saved to {} (not opened: {e})", path.display()),
        });
    }
}

/// The run `key` of the dataset in `bids_dir`, its headers and sidecars read.
fn read_run(bids_dir: &str, key: &str, num_echoes: &str) -> Result<crate::bids::discovery::QsmRun, String> {
    let filter = crate::bids::discovery::DiscoveryFilter {
        include: Some(vec![key.to_string()]),
        exclude: None,
        num_echoes: num_echoes.trim().parse().ok(),
    };
    crate::bids::discovery::discover_runs(&super::command::expand_tilde(bids_dir), &filter)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.key.to_string() == key)
        .ok_or_else(|| "its files were not found".to_string())
}

/// How this system opens a file in its default viewer, given its environment (`var`): `None` where
/// there is nothing to show it with (why).
pub fn open_command(os: &str, var: impl Fn(&str) -> Option<String>, path: &Path) -> Result<(String, Vec<String>), &'static str> {
    let set = |name: &str| var(name).is_some_and(|v| !v.is_empty());
    if set("SSH_CONNECTION") || set("SSH_CLIENT") || set("SSH_TTY") {
        return Err("over SSH, not opened");
    }
    let p = path.display().to_string();
    match os {
        "macos" => Ok(("open".into(), vec![p])),
        "windows" => Ok(("cmd".into(), vec!["/C".into(), "start".into(), String::new(), p])),
        _ if set("DISPLAY") || set("WAYLAND_DISPLAY") => Ok(("xdg-open".into(), vec![p])),
        _ => Err("no display, not opened"),
    }
}

/// Open `path` in the system's default viewer, without waiting for it and without letting it
/// write to the terminal.
pub fn open(path: &Path) -> Result<Opened, String> {
    let (program, args) = match open_command(std::env::consts::OS, |n| std::env::var(n).ok(), path) {
        Ok(c) => c,
        Err(why) => return Ok(Opened::NoViewer(why)),
    };
    let mut child = std::process::Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    // Reap it when it exits, so it does not linger as a zombie while the TUI runs.
    std::thread::spawn(move || child.wait());
    Ok(Opened::Viewer)
}

/// The width of the summary's headings ("Memory"), after a space.
const HEAD: usize = 9;

/// What the run needs before it starts, a line each: the scan it is planned on, its peak memory,
/// its models, and what the scan cannot produce.
pub fn summary_lines(p: &Planned) -> Vec<Line<'static>> {
    let head = |s: &str| Span::styled(format!(" {s:<w$}", w = HEAD), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let dim = Style::default().fg(Color::DarkGray);
    let f = &p.facts;
    let scan = format!(
        "{} echo{}, {} T, {} × {} × {} voxels{}",
        f.n_echoes, if f.n_echoes == 1 { "" } else { "es" }, f.field_strength, f.dims.0, f.dims.1, f.dims.2,
        if f.has_magnitude { "" } else { ", phase only" },
    );
    let mut lines = vec![match &p.source {
        ScanSource::Run { key, selected } => {
            let of = if *selected > 1 { format!(" (the first of {selected} selected runs)") } else { String::new() };
            Line::from(vec![head("Scan"), Span::raw(format!("{key}{of}: {scan}"))])
        }
        ScanSource::Typical { why } => Line::from(vec![
            head("Scan"),
            Span::raw(format!("a typical scan: {scan}")),
            Span::styled(match why {
                Some(why) => format!(" ({why})"),
                None => " (choose a run on the Input tab to plan for it)".to_string(),
            }, dim),
        ]),
    }];
    lines.push(Line::from(vec![
        head("Memory"),
        Span::raw(format!("peak about {}", qsmxt_engine::memory::format_bytes(p.plan.peak_memory_bytes))),
        Span::styled(
            format!(" ({} available here)", qsmxt_engine::memory::format_bytes(crate::pipeline::memory::available_memory_bytes())),
            dim,
        ),
    ]));
    let mut models: Vec<Span> = vec![head("Models")];
    if p.plan.models.is_empty() {
        models.push(Span::raw("none"));
    }
    for (i, id) in p.plan.models.iter().enumerate() {
        if i > 0 {
            models.push(Span::raw(", "));
        }
        let spec = qsm_core::models::find_model(id);
        let bytes: u64 = spec.map_or(0, |m| m.files.iter().map(|f| f.bytes).sum());
        let present = spec.is_some_and(|m| m.files.iter().all(qsm_core::models::weight_file_present));
        models.push(Span::styled(model_name(id).to_string(), Style::default().fg(Color::Magenta)));
        models.push(Span::raw(format!(" {}", qsmxt_engine::memory::format_bytes(bytes))));
        models.push(Span::styled(if present { " (downloaded)" } else { " (to download)" }, dim));
    }
    lines.push(Line::from(models));
    for e in &p.plan.check.errors {
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<w$}", "Error", w = HEAD), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(e.clone(), Style::default().fg(Color::Red)),
        ]));
    }
    for w in &p.plan.check.warnings {
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<w$}", "Warning", w = HEAD), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(w.clone()),
        ]));
    }
    for n in &p.plan.notes {
        lines.push(Line::from(vec![head("Note"), Span::raw(n.clone())]));
    }
    if let Some(m) = &p.motion {
        lines.push(Line::from(vec![head("Motion"), Span::raw(m.clone())]));
    }
    lines
}

/// The style of each part of the graph's text.
fn style(part: Part) -> Style {
    match part {
        Part::Number => Style::default().fg(Color::DarkGray),
        Part::Step => Style::default().add_modifier(Modifier::BOLD),
        Part::Algorithm => Style::default().fg(Color::Cyan),
        Part::Pass => Style::default().fg(Color::Yellow),
        Part::Setting => Style::default(),
        Part::When | Part::Note => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        Part::Models => Style::default().fg(Color::Magenta),
        Part::From => Style::default().fg(Color::Green),
        Part::Items => Style::default().fg(Color::DarkGray),
        Part::Run => Style::default().fg(Color::Blue),
    }
}

/// `line` broken to `width`, its continuations under its text, past the heading.
fn wrapped(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    let parts: Vec<(Style, String)> = line.spans.iter().map(|s| (s.style, s.content.to_string())).collect();
    wrap_parts(0, HEAD + 1, &parts, width)
        .into_iter()
        .map(|(indent, parts)| {
            let mut spans = vec![Span::raw(" ".repeat(indent))];
            spans.extend(parts.into_iter().map(|(style, text)| Span::styled(text, style)));
            Line::from(spans)
        })
        .collect()
}

/// The tab's lines at `width`: the summary, then the graph.
pub fn lines(p: &Planned, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = summary_lines(p).into_iter().flat_map(|l| wrapped(l, width)).collect();
    lines.push(Line::from(""));
    let title = if p.graph.two_pass { " Steps, in run order (two passes)" } else { " Steps, in run order" };
    lines.push(Line::from(Span::styled(title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
    for l in p.graph.text_lines(Some(width)) {
        let mut spans = vec![Span::raw(" ".repeat(l.indent))];
        spans.extend(l.parts.into_iter().map(|(part, text)| Span::styled(text, style(part))));
        lines.push(Line::from(spans));
    }
    lines
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let block = super::tabs::panel(app);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = match app.planned() {
        Ok(p) => lines(&p, inner.width.saturating_sub(1) as usize),
        Err(e) => vec![
            Line::from(Span::styled(" Invalid setting", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(format!(" {e}")),
            Line::from(""),
            Line::from(" The run would refuse this configuration."),
        ],
    };
    let total = lines.len();
    let visible = inner.height as usize;
    let max_scroll = total.saturating_sub(visible);
    app.pipeline_view.scroll = app.pipeline_view.scroll.min(max_scroll);
    let para = Paragraph::new(lines).scroll((app.pipeline_view.scroll as u16, 0));
    f.render_widget(para, inner);
    if total > visible {
        let mut state = ScrollbarState::new(max_scroll).position(app.pipeline_view.scroll);
        f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight), inner, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{QSM_ALGO_OPTIONS, TAB_PIPELINE};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};

    /// The app on the Pipeline tab, its SVG kept in `dir` and never opened.
    fn app(dir: &Path) -> App {
        let mut app = App::new();
        app.active_tab = TAB_PIPELINE;
        app.pipeline_view.svg_dir = dir.to_path_buf();
        app.pipeline_view.opener = |_| Ok(Opened::NoViewer("not in a test"));
        app
    }

    /// The screen as text, a row a line, drawn `height` rows tall.
    fn screen(app: &mut App, height: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(110, height)).unwrap();
        t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer();
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn algorithm(app: &mut App, name: &str) {
        app.pipeline_state.qsm_algorithm = QSM_ALGO_OPTIONS.iter().position(|a| *a == name).unwrap();
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// The default (classical) pipeline: every step, the arrows into them, the peak memory, no
    /// models, and the typical scan it is planned on.
    #[test]
    fn the_default_pipeline_shows_its_steps_arrows_memory_and_models() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        let s = screen(&mut app, 80);
        let p = app.planned().unwrap();
        for n in &p.graph.nodes {
            assert!(s.contains(&n.label), "{} missing:\n{s}", n.label);
        }
        assert!(s.contains("Dipole inversion · iLSQR"), "{s}");
        assert!(s.contains("← 6 Background-field removal: Local field, Local field mask"), "{s}");
        assert!(s.contains("only when the run is resampled"), "{s}");
        let peak = qsmxt_engine::memory::format_bytes(p.plan.peak_memory_bytes);
        assert!(s.contains(&format!("Memory   peak about {peak}")), "{s}");
        assert!(s.contains("Models   none"), "{s}");
        assert!(s.contains("Scan     a typical scan: 5 echoes, 3 T, 256 × 256 × 176 voxels"), "{s}");
        assert!(s.contains("Enter/s:Open as SVG"), "{s}");
    }

    /// A two-pass run draws both passes and their combination.
    #[test]
    fn a_two_pass_pipeline_shows_both_passes() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        app.pipeline_state.two_pass = true;
        let s = screen(&mut app, 90);
        assert!(s.contains("Steps, in run order (two passes)"), "{s}");
        assert!(s.contains("Brain mask  [main pass]"), "{s}");
        assert!(s.contains("Reliable-phase mask  [reliable pass]"), "{s}");
        assert!(s.contains("Two-pass combination"), "{s}");
        assert!(s.contains("Dipole inversion (reliable pass)"), "{s}");
    }

    /// A deep-learning inversion lists its model with its download size, on the step and in
    /// the summary.
    #[test]
    fn a_deep_learning_inversion_lists_its_model_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        algorithm(&mut app, "xqsm");
        let s = screen(&mut app, 80);
        let p = app.planned().unwrap();
        assert_eq!(p.plan.models.len(), 1, "{:?}", p.plan.models);
        let spec = qsm_core::models::find_model(p.plan.models[0]).unwrap();
        let size = qsmxt_engine::memory::format_bytes(spec.files.iter().map(|f| f.bytes).sum());
        assert!(s.contains(&format!("Models   {} {size} (", spec.name)), "{s}");
        assert!(s.contains(&format!("model: {}", spec.name)), "{s}");
    }

    /// The view follows the settings: another inversion, another graph and memory estimate.
    #[test]
    fn changing_a_setting_changes_the_view() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        let before = screen(&mut app, 80);
        algorithm(&mut app, "tgv");
        let after = screen(&mut app, 80);
        assert!(before.contains("Background-field removal") && before.contains("iLSQR"), "{before}");
        assert!(!after.contains("Background-field removal") && after.contains("TGV-QSM"), "{after}");
        // A setting on another tab (SWI, an output) adds its step.
        assert!(!after.contains("SWI"), "{after}");
        app.form.do_swi = true;
        let swi = screen(&mut app, 80);
        assert!(swi.contains("SWI"), "{swi}");
    }

    /// With a dataset selected, the plan is the first selected run's: its echoes and grid.
    #[test]
    fn a_selected_run_is_planned_on_its_own_scan() {
        let dir = tempfile::tempdir().unwrap();
        let bids = tempfile::tempdir().unwrap();
        crate::testutils::create_multi_echo_bids(bids.path());
        let mut app = app(dir.path());
        app.form.bids_dir = bids.path().to_string_lossy().to_string();
        app.filter_state.maybe_rescan(&app.form.bids_dir.clone());
        assert!(app.filter_state.tree.is_some());
        let s = screen(&mut app, 60);
        assert!(s.contains("Scan     sub-1_MEGRE: 3 echoes, 3 T"), "{s}");
        let p = app.planned().unwrap();
        assert_eq!(p.facts.n_echoes, 3);
        let typical = typical_scan();
        let f = qsmxt_engine::memory::estimate_peak_memory_bytes;
        assert_eq!(p.plan.peak_memory_bytes, f(p.facts.dims.0, p.facts.dims.1, p.facts.dims.2, 3, 1, true,
            &crate::tui::command::config_from_app(&app).unwrap()));
        assert_ne!(p.facts.dims, typical.dims);
        // Deselecting every run goes back to the typical scan.
        app.filter_state.tree.as_mut().unwrap().set_all(false);
        assert!(screen(&mut app, 60).contains("Scan     a typical scan"));
    }

    /// Motion correction is decided per run: the view counts the selected runs it runs for and
    /// names the ones it is skipped for (#397).
    #[test]
    fn motion_correction_is_counted_over_the_selected_runs() {
        let dir = tempfile::tempdir().unwrap();
        let bids = tempfile::tempdir().unwrap();
        crate::testutils::create_multi_echo_bids(bids.path());
        crate::testutils::create_single_echo_bids(bids.path());
        let mut app = app(dir.path());
        app.form.bids_dir = bids.path().to_string_lossy().to_string();
        app.filter_state.maybe_rescan(&app.form.bids_dir.clone());
        assert!(app.planned().unwrap().motion.is_none(), "off: nothing to count");
        app.pipeline_state.motion_correction = true;
        let s = screen(&mut app, 60);
        assert!(s.contains("Motion   Motion correction will run for 1 of 2 run(s); skipped for sub-1_T2starw (one echo)"), "{s}");
        app.filter_state.num_echoes = "1".into();
        assert!(app.planned().unwrap().motion.unwrap().starts_with("Motion correction will run for 0 of 2"));
    }

    /// Enter (or `s`) writes the graph as SVG and says where; scrolling stays on the tab.
    #[test]
    fn the_svg_key_writes_the_graph_and_says_where() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        app.pipeline_state.two_pass = true;
        press(&mut app, KeyCode::Char('s'));
        let path = dir.path().join(format!("qsmxt-pipeline-{}.svg", std::process::id()));
        let svg = std::fs::read_to_string(&path).unwrap();
        assert_eq!(svg, app.planned().unwrap().graph.to_svg());
        let msg = app.status_message.clone().unwrap();
        assert!(msg.contains(&path.display().to_string()) && msg.contains("not in a test"), "{msg}");
        let s = screen(&mut app, 40);
        assert!(s.contains("Pipeline graph saved to"), "{s}");

        app.pipeline_view.opener = |_| Ok(Opened::Viewer);
        press(&mut app, KeyCode::Enter);
        assert!(app.status_message.as_deref().unwrap().ends_with("and opened"));
        app.pipeline_view.opener = |_| Err("xdg-open: not found".into());
        press(&mut app, KeyCode::Enter);
        assert!(app.status_message.as_deref().unwrap().contains("not opened: xdg-open: not found"));

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.pipeline_view.scroll, 2);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.pipeline_view.scroll, 1);
        assert_eq!(app.active_tab, TAB_PIPELINE);
    }

    /// The system's opener, or none over SSH or with no display.
    #[test]
    fn the_opener_follows_the_platform_and_stays_quiet_over_ssh() {
        let path = Path::new("/tmp/g.svg");
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |n: &str| vars.iter().find(|(k, _)| *k == n).map(|(_, v)| v.to_string())
        };
        assert_eq!(open_command("macos", env(&[]), path).unwrap().0, "open");
        assert_eq!(open_command("windows", env(&[]), path).unwrap(), ("cmd".to_string(),
            vec!["/C".to_string(), "start".to_string(), String::new(), "/tmp/g.svg".to_string()]));
        assert_eq!(open_command("linux", env(&[("DISPLAY", ":0")]), path).unwrap().0, "xdg-open");
        assert_eq!(open_command("linux", env(&[("WAYLAND_DISPLAY", "wayland-0")]), path).unwrap().0, "xdg-open");
        assert!(open_command("linux", env(&[]), path).unwrap_err().contains("no display"));
        assert!(open_command("linux", env(&[("DISPLAY", "")]), path).is_err());
        for ssh in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
            let vars: &'static [(&'static str, &'static str)] = match ssh {
                "SSH_CONNECTION" => &[("SSH_CONNECTION", "1 2 3 4"), ("DISPLAY", "localhost:10")],
                "SSH_CLIENT" => &[("SSH_CLIENT", "1 2 3")],
                _ => &[("SSH_TTY", "/dev/pts/1")],
            };
            assert!(open_command("macos", env(vars), path).unwrap_err().contains("SSH"), "{ssh}");
        }
    }
}
