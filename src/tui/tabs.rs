//! The stage tab bar, the sub-tab bar and the panel that holds a tab's content.
//!
//! The header is a cyan "QSMxT" chip followed by quiet stage labels; the active stage is bold
//! and underlined by a cyan `━` bar set into a dim rule, which also carries the versions. A
//! stage with sub-tabs shows them as pills along the top of its (dim, rounded) panel: the active
//! one filled cyan, the others on dark grey, with a `[ ] sub-tab  n/m` hint on the right. While
//! focus is on the strip itself (←/→ move along it), the active pill is yellow, the colour of a
//! focused row, and the hint reads `← → sub-tab`.
//!
//! On a narrow terminal the stage labels shorten (inactive tabs first to short names, then to
//! their digits; the active tab keeps its full name), and the sub-tabs drop the hint and then
//! show a window around the active one with `‹`/`›` marking the ones cut off.

use std::sync::OnceLock;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
    Frame,
};

use super::app::{App, TAB_NAMES};

/// Short names, used for inactive tabs when the full names do not fit the width.
const SHORT: [&str; TAB_NAMES.len()] = ["Input", "Prep", "QSM", "χ-sep", "Outputs", "Exec", "Methods"];

/// The greys the design is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Palette {
    /// Inactive labels.
    text: Color,
    /// Digits of inactive tabs, hints, versions, cut-off markers.
    muted: Color,
    /// The rule under the tabs and the panel border.
    faint: Color,
    /// Inactive pills' background; `None` draws them on the terminal's own background.
    pill_bg: Option<Color>,
}

impl Palette {
    /// 256-colour greys.
    pub(super) const EXTENDED: Palette = Palette {
        text: Color::Indexed(250),
        muted: Color::Indexed(244),
        faint: Color::Indexed(240),
        pill_bg: Some(Color::Indexed(237)),
    };
    /// The 16 basic colours only. Indexed greys would be mapped to whatever the terminal's
    /// nearest colour is (often not grey at all), so these use the named greys and no pill fill.
    pub(super) const BASIC: Palette = Palette {
        text: Color::Gray,
        muted: Color::DarkGray,
        faint: Color::DarkGray,
        pill_bg: None,
    };

    /// 256 colours unless the environment says the terminal has only the basic 16.
    fn detect(term: Option<&str>, colorterm: Option<&str>) -> Palette {
        if colorterm.is_some_and(|c| !c.is_empty()) {
            return Palette::EXTENDED;
        }
        match term {
            Some(t) if t.contains("256") || t.contains("direct") || t.contains("truecolor") => Palette::EXTENDED,
            // The Linux console and old hardware terminals.
            Some("linux" | "vt100" | "vt102" | "vt220" | "ansi" | "cons25" | "dumb") => Palette::BASIC,
            Some(t) if t.ends_with("-16color") || t.ends_with("-color") || t.ends_with("-8color") => Palette::BASIC,
            // Anything else (plain `xterm`, `screen`, `tmux`, unset): every terminal emulator in
            // use does 256 colours, whatever its TERM says.
            _ => Palette::EXTENDED,
        }
    }
}

fn palette() -> Palette {
    #[cfg(test)]
    if let Some(p) = tests::PALETTE.with(|c| c.get()) {
        return p;
    }
    static P: OnceLock<Palette> = OnceLock::new();
    *P.get_or_init(|| {
        Palette::detect(std::env::var("TERM").ok().as_deref(), std::env::var("COLORTERM").ok().as_deref())
    })
}

fn version() -> String {
    // Development builds describe QSM.rs by a full commit hash: keep the line short.
    let core = env!("QSM_CORE_DESCRIBE");
    let core = if core.chars().count() > 16 { format!("{}…", core.chars().take(15).collect::<String>()) } else { core.to_string() };
    format!("QSMxT {} · QSM.rs {core}", env!("CARGO_PKG_VERSION"))
}

fn w(s: &str) -> usize {
    s.chars().count()
}

/// Write `s` at (`x`, `y`), clipped to `area`; returns the column after it.
fn put(buf: &mut Buffer, area: Rect, x: u16, y: u16, s: &str, style: Style) -> u16 {
    if x >= area.right() || y >= area.bottom() {
        return x + w(s) as u16;
    }
    let max = (area.right() - x) as usize;
    buf.set_stringn(x, y, s, max, style);
    x + w(s) as u16
}

/// The main tabs' labels at the least compression that fits: full names, then short names for
/// the inactive tabs, then their digits alone. The active tab always keeps its full name.
fn fit_labels(active: usize, avail: usize, total: impl Fn(&[String]) -> usize) -> Vec<String> {
    let mut last = Vec::new();
    for level in 0..3 {
        last = (0..TAB_NAMES.len())
            .map(|i| match (i == active, level) {
                (true, _) | (_, 0) => format!("{} {}", i + 1, TAB_NAMES[i]),
                (_, 1) => format!("{} {}", i + 1, SHORT[i]),
                _ => format!("{}", i + 1),
            })
            .collect();
        if total(&last) <= avail {
            break;
        }
    }
    last
}

/// A contiguous run of items around `active` that fits `avail` columns, counting `gap` between
/// items and one column for each `‹`/`›` marker shown when items are cut off. Returns `lo..hi`.
fn window(widths: &[usize], gap: usize, active: usize, avail: usize) -> (usize, usize) {
    let n = widths.len();
    let total = |lo: usize, hi: usize| {
        widths[lo..hi].iter().sum::<usize>() + gap * (hi - lo).saturating_sub(1) + usize::from(lo > 0) + usize::from(hi < n)
    };
    if total(0, n) <= avail {
        return (0, n);
    }
    let (mut lo, mut hi) = (active, active + 1);
    loop {
        let mut grew = false;
        if hi < n && total(lo, hi + 1) <= avail {
            hi += 1;
            grew = true;
        }
        if lo > 0 && total(lo - 1, hi) <= avail {
            lo -= 1;
            grew = true;
        }
        if !grew {
            return (lo, hi);
        }
    }
}

/// The active tab's sub-tabs and the index of the active one, when it has more than one.
fn subtabs(app: &App) -> Option<(Vec<&'static str>, usize)> {
    let pages = app.pages(app.active_tab);
    if pages.len() < 2 {
        return None;
    }
    let cur = app.page();
    let active = pages.iter().position(|&p| Some(p) == cur).unwrap_or(0);
    Some((pages.iter().map(|p| p.label()).collect(), active))
}

// ─── Header (main tab bar) ───

/// Rows the header takes: the labels, and the rule they sit on.
pub fn header_height() -> u16 {
    2
}

/// The brand chip and quiet labels; the active one bold, underlined by an accent bar on a rule.
pub fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let buf = f.buffer_mut();
    let pal = palette();
    let width = area.width as usize;
    let active = app.active_tab;
    let (y0, y1) = (area.y, area.y + 1);
    let brand = " QSMxT ";
    let lead = 1 + w(brand) + 2;
    // Three columns between labels, two when that keeps the inactive tabs' (short) names.
    let total = |gap: usize| move |l: &[String]| lead + l.iter().map(|s| w(s)).sum::<usize>() + gap * (l.len() - 1) + 1;
    let mut gap = 3;
    let mut labels = fit_labels(active, width, total(3));
    if labels.iter().any(|l| w(l) == 1) {
        gap = 2;
        labels = fit_labels(active, width, total(2));
    }

    put(buf, area, area.x, y1, &"─".repeat(width), Style::default().fg(pal.faint));
    let mut x = put(buf, area, area.x + 1, y0, brand, Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD));
    x += 2;
    let mut underline = (0u16, 0u16);
    for (i, l) in labels.iter().enumerate() {
        if i > 0 {
            x += gap;
        }
        let (digit, name) = l.split_at(1);
        if i == active {
            underline = (x.saturating_sub(1), x + w(l) as u16 + 1);
            x = put(buf, area, x, y0, digit, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
            x = put(buf, area, x, y0, name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
        } else {
            x = put(buf, area, x, y0, digit, Style::default().fg(pal.muted));
            x = put(buf, area, x, y0, name, Style::default().fg(pal.text));
        }
    }
    let (ux0, ux1) = underline;
    put(buf, area, ux0, y1, &"━".repeat((ux1 - ux0) as usize), Style::default().fg(Color::Cyan));

    // The versions sit in the rule, right-aligned, unless they would cross the underline.
    let ver = format!(" {} ", version());
    let vx = area.right().saturating_sub(w(&ver) as u16 + 2);
    if vx > ux1 + 1 && vx > area.x {
        put(buf, area, vx, y1, &ver, Style::default().fg(pal.muted));
    }
}

// ─── Panel ───

/// The dim, rounded panel a tab's content sits in, titled `Tab › Sub-tab`.
pub fn panel(app: &App) -> Block<'static> {
    let pal = palette();
    let tab = TAB_NAMES[app.active_tab];
    let sub = subtabs(app).map(|(names, cur)| names[cur]);
    let mut spans = vec![Span::raw(" "), Span::styled(tab, Style::default().fg(pal.text))];
    if let Some(s) = sub {
        spans.push(Span::styled(" › ", Style::default().fg(pal.muted)));
        spans.push(Span::styled(s, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
    }
    spans.push(Span::raw(" "));
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(pal.faint))
        .title(Line::from(spans))
}

// ─── Sub-tabs (inside the panel) ───

/// Rows the sub-tab bar takes at the top of the panel: the pills and a blank row (0 when the
/// tab has one page).
pub fn subtab_rows(app: &App) -> u16 {
    if subtabs(app).is_some() { 2 } else { 0 }
}

/// Pills, the active one filled with the accent (yellow while the strip has focus); a key hint
/// and counter on the right.
pub fn draw_subtabs(f: &mut Frame, app: &App, area: Rect) {
    let Some((names, cur)) = subtabs(app) else { return };
    let buf = f.buffer_mut();
    let pal = palette();
    let width = area.width as usize;
    let items: Vec<String> = names.iter().map(|n| format!(" {n} ")).collect();
    let widths: Vec<usize> = items.iter().map(|s| w(s)).collect();
    let focused = app.on_subtabs();
    let counter = format!("{}/{}", cur + 1, names.len());
    // The two hints are the same width, so focusing the strip never moves the pills.
    let (k0, k1) = if focused { ("←", "→") } else { ("[", "]") };
    let hint_w = w("[ ] sub-tab  ") + w(&counter) + 1;
    let all = widths.iter().sum::<usize>() + widths.len() - 1 + 1;
    let show_hint = all + hint_w + 2 <= width;
    let avail = if show_hint { width - hint_w - 2 } else { width.saturating_sub(1) };
    let (lo, hi) = window(&widths, 1, cur, avail);

    let y = area.y;
    let mut x = area.x + 1;
    if lo > 0 {
        x = put(buf, area, x, y, "‹", Style::default().fg(pal.muted));
    }
    for (i, item) in items.iter().enumerate().take(hi).skip(lo) {
        if i > lo {
            x += 1;
        }
        let st = if i == cur {
            let bg = if focused { Color::Yellow } else { Color::Cyan };
            Style::default().fg(Color::Black).bg(bg).add_modifier(Modifier::BOLD)
        } else {
            let st = Style::default().fg(pal.text);
            match pal.pill_bg { Some(bg) => st.bg(bg), None => st }
        };
        x = put(buf, area, x, y, item, st);
    }
    if hi < items.len() {
        put(buf, area, x, y, "›", Style::default().fg(pal.muted));
    }
    if show_hint {
        let key = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(pal.muted);
        let mut x = area.right() - hint_w as u16;
        x = put(buf, area, x, y, k0, key);
        x = put(buf, area, x, y, " ", dim);
        x = put(buf, area, x, y, k1, key);
        x = put(buf, area, x, y, " sub-tab  ", dim);
        put(buf, area, x, y, &counter, Style::default().fg(pal.text));
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::tui::app::Page;
    use ratatui::{backend::TestBackend, Terminal};

    thread_local! {
        /// A test's palette, in place of the one detected from the environment.
        pub(super) static PALETTE: Cell<Option<Palette>> = const { Cell::new(None) };
    }

    fn render(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        t.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
        t
    }

    fn row(t: &Terminal<TestBackend>, y: u16) -> String {
        let buf = t.backend().buffer();
        (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect()
    }

    fn goto(app: &mut App, tab: usize, page: Option<Page>) {
        app.active_tab = tab;
        if page.is_some() {
            app.sub_page[tab] = page;
        }
        app.sync_pipeline_mode();
    }

    #[test]
    fn the_active_stage_is_underlined_and_every_stage_is_on_screen() {
        for (width, height) in [(120, 40), (80, 24)] {
            for (tab, name) in TAB_NAMES.iter().enumerate() {
                let mut app = App::new();
                goto(&mut app, tab, None);
                let t = render(&mut app, width, height);
                let (labels, rule) = (row(&t, 0), row(&t, 1));
                assert!(labels.contains(" QSMxT "), "{width}x{height}: brand chip:\n{labels}");
                // The active tab is named in full; every tab's digit is shown.
                assert!(labels.contains(&format!("{} {name}", tab + 1)), "{width}x{height}:\n{labels}");
                for i in 1..=TAB_NAMES.len() {
                    assert!(labels.contains(&i.to_string()), "{width}x{height}: tab {i} missing:\n{labels}");
                }
                // The underline sits under the active label.
                let start = labels.chars().collect::<Vec<_>>();
                let col = labels.find(&format!("{} {name}", tab + 1)).unwrap();
                let col = labels[..col].chars().count();
                assert_eq!(rule.chars().nth(col), Some('━'), "{width}x{height} tab {tab}:\n{labels}\n{rule}");
                assert_eq!(start.len(), width as usize);
            }
        }
    }

    #[test]
    fn the_versions_sit_in_the_rule_when_there_is_room() {
        let mut app = App::new();
        let t = render(&mut app, 120, 40);
        assert!(row(&t, 1).contains(&format!("QSMxT {}", env!("CARGO_PKG_VERSION"))), "{}", row(&t, 1));
    }

    #[test]
    fn sub_tabs_are_pills_with_a_hint_and_a_counter() {
        let mut app = App::new();
        goto(&mut app, 1, Some(Page::Masking));
        let t = render(&mut app, 120, 40);
        // Row 2 is the panel's top border with its breadcrumb title; row 3 the pills.
        assert!(row(&t, 2).contains("Preprocessing › Masking"), "{}", row(&t, 2));
        let pills = row(&t, 3);
        for name in [" Geometry ", " Masking ", " Motion "] {
            assert!(pills.contains(name), "{pills}");
        }
        assert!(pills.contains("[ ] sub-tab  2/3"), "{pills}");
        // The active pill is filled with the accent, the others are not.
        let buf = t.backend().buffer();
        let at = |s: &str| pills.find(s).map(|b| pills[..b].chars().count() as u16 + 1).unwrap();
        assert_eq!(buf[(at(" Masking "), 3)].bg, Color::Cyan);
        assert_ne!(buf[(at(" Geometry "), 3)].bg, Color::Cyan);
    }

    #[test]
    fn a_tab_without_sub_tabs_has_no_pill_row() {
        let mut app = App::new();
        goto(&mut app, 0, None);
        let t = render(&mut app, 120, 40);
        assert!(!row(&t, 3).contains("sub-tab"), "{}", row(&t, 3));
        assert!(row(&t, 2).contains(" Input "), "{}", row(&t, 2));
    }

    /// At 80 columns every QSM sub-tab still fits; squeezed further, a window around the active
    /// one is shown with markers for the rest, and the active one is never cut off.
    #[test]
    fn sub_tabs_fit_or_window_on_narrow_terminals() {
        let mut app = App::new();
        app.filter_state.orientation_group = "acq".into();
        let pages = app.pages(2);
        let last = *pages.last().unwrap();
        goto(&mut app, 2, Some(last));
        let t = render(&mut app, 80, 24);
        let pills = row(&t, 3);
        for p in &pages {
            assert!(pills.contains(p.label()), "{} missing at 80 columns:\n{pills}", p.label());
        }
        let t = render(&mut app, 40, 24);
        let pills = row(&t, 3);
        assert!(pills.contains(last.label()), "active sub-tab cut off:\n{pills}");
        assert!(pills.contains('‹'), "cut-off sub-tabs should be marked:\n{pills}");
    }

    #[test]
    fn the_basic_palette_uses_only_the_16_named_colours() {
        PALETTE.with(|c| c.set(Some(Palette::BASIC)));
        let mut app = App::new();
        goto(&mut app, 1, Some(Page::Masking));
        let t = render(&mut app, 120, 40);
        PALETTE.with(|c| c.set(None));
        let buf = t.backend().buffer();
        for y in 0..5 {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                for c in [cell.fg, cell.bg] {
                    assert!(!matches!(c, Color::Indexed(_) | Color::Rgb(..)), "({x},{y}) uses {c:?}");
                }
            }
        }
    }

    #[test]
    fn the_palette_follows_the_terminal() {
        assert_eq!(Palette::detect(Some("xterm-256color"), None), Palette::EXTENDED);
        assert_eq!(Palette::detect(Some("xterm"), Some("truecolor")), Palette::EXTENDED);
        assert_eq!(Palette::detect(Some("linux"), None), Palette::BASIC);
        assert_eq!(Palette::detect(Some("xterm"), None), Palette::EXTENDED);
        assert_eq!(Palette::detect(Some("rxvt-16color"), None), Palette::BASIC);
        assert_eq!(Palette::detect(None, None), Palette::EXTENDED);
    }

    fn key(app: &mut App, code: crossterm::event::KeyCode) {
        app.handle_key(crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE));
    }

    fn screen(t: &Terminal<TestBackend>) -> String {
        (0..t.backend().buffer().area.height).map(|y| row(t, y)).collect::<Vec<_>>().join("\n")
    }

    /// With focus on the strip the active pill turns yellow (the focus colour) and the hint
    /// shows the arrows; going down into the rows turns it back to cyan.
    #[test]
    fn a_focused_sub_tab_strip_looks_focused() {
        for (width, height) in [(120, 40), (80, 24)] {
            let mut app = App::new();
            key(&mut app, crossterm::event::KeyCode::Char('2'));
            key(&mut app, crossterm::event::KeyCode::Right);
            let t = render(&mut app, width, height);
            let pills = row(&t, 3);
            let at = |s: &str| pills.find(s).map(|b| pills[..b].chars().count() as u16 + 1).unwrap();
            assert_eq!(t.backend().buffer()[(at(" Masking "), 3)].bg, Color::Yellow, "{width}x{height}:\n{pills}");
            assert!(pills.contains("← → sub-tab  2/3"), "{width}x{height}:\n{pills}");
            let text = screen(&t);
            assert!(text.contains("↓ or Enter to go to its settings"), "{width}x{height}:\n{text}");
            assert!(row(&t, height - 1).contains("Sub-tabs"), "{width}x{height}:\n{}", row(&t, height - 1));

            key(&mut app, crossterm::event::KeyCode::Down);
            let t = render(&mut app, width, height);
            let pills = row(&t, 3);
            let at = |s: &str| pills.find(s).map(|b| pills[..b].chars().count() as u16 + 1).unwrap();
            assert_eq!(t.backend().buffer()[(at(" Masking "), 3)].bg, Color::Cyan);
            assert!(pills.contains("[ ] sub-tab  2/3"), "{pills}");
        }
    }

    #[test]
    fn the_chi_separation_tab_is_named_in_full_and_fits() {
        for (width, height) in [(120, 40), (80, 24)] {
            let mut app = App::new();
            goto(&mut app, crate::tui::app::TAB_SEPARATION, None);
            let t = render(&mut app, width, height);
            assert!(row(&t, 0).contains("4 χ-separation"), "{width}x{height}:\n{}", row(&t, 0));
            assert!(row(&t, 2).contains(" χ-separation "), "{width}x{height}:\n{}", row(&t, 2));
        }
    }

    /// The Execution tab shows its Run button without scrolling, labelled with what F5 does.
    #[test]
    fn the_run_button_is_on_screen() {
        for (width, height) in [(120, 40), (80, 24)] {
            for (mode, dry, submit, label) in [(0, false, false, "▶ Run "), (0, true, false, "▶ Dry run "),
                                               (1, false, false, "▶ Write SLURM scripts "), (1, false, true, "▶ Submit SLURM jobs ")] {
                let mut app = App::new();
                app.form.execution_mode = mode;
                app.form.dry_run = dry;
                app.form.slurm_submit = submit;
                goto(&mut app, crate::tui::app::TAB_EXECUTION, None);
                let t = render(&mut app, width, height);
                let text = screen(&t);
                assert!(text.contains(label), "{width}x{height} {label}:\n{text}");
            }
        }
    }

    /// The Methods tab's footer says how to copy, and the confirmation replaces it once copied.
    #[test]
    fn the_methods_tab_offers_copying() {
        for (width, height) in [(120, 40), (80, 24)] {
            let mut app = App::new();
            goto(&mut app, crate::tui::app::TAB_METHODS, None);
            let t = render(&mut app, width, height);
            assert!(row(&t, height - 1).contains("Enter/c:Copy to clipboard"), "{}", row(&t, height - 1));
            key(&mut app, crossterm::event::KeyCode::Enter);
            let t = render(&mut app, width, height);
            assert!(row(&t, height - 1).contains("Methods copied to clipboard"), "{}", row(&t, height - 1));
        }
    }
}
