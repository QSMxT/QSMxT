//! The screens' tests: what each tab, section and pop-up draws.

use super::*;
use ratatui::{backend::TestBackend, Terminal};

/// The Inversion section with MEDI, focus on its settings.
fn medi_app() -> App {
    let mut app = App::new();
    let medi = crate::app::QSM_PICKER_OPTIONS.iter().position(|o| *o == "medi").unwrap();
    app.set_field_select("qsm_algorithm", medi);
    app.active_tab = crate::app::TAB_RECONSTRUCTION;
    app.sub_page[crate::app::TAB_RECONSTRUCTION] = Some(crate::app::Page::Inversion);
    app.sync_pipeline_mode();
    app.subtab_focus = false;
    app
}

/// Whether the help is drawn beside the settings: a second rule joins the panel's top border.
fn help_beside(width: u16) -> bool {
    let t = render_app_sized(&mut medi_app(), width, 40);
    rules(&t).len() == 2
}

/// The columns of the panel's inner rules, where they meet its bottom border.
fn rules(t: &Terminal<TestBackend>) -> Vec<u16> {
    let buf = t.backend().buffer();
    let bottom = (0..buf.area.height).find(|&y| buf[(0, y)].symbol() == "╰").unwrap();
    (0..buf.area.width).filter(|&x| buf[(x, bottom)].symbol() == "┴").collect()
}

#[test]
fn the_help_moves_beside_the_settings_from_138_columns() {
    assert!(!help_beside(120));
    assert!(!help_beside(137));
    assert!(help_beside(138));
    assert!(help_beside(200));
}

#[test]
fn beside_the_settings_the_help_is_wrapped_to_its_column_and_the_settings_keep_76() {
    let mut app = medi_app();
    let t = render_app_sized(&mut app, 160, 40);
    let text = screen_text(&t);
    assert!(text.contains("Morphology Enabled Dipole"), "{text}");
    let buf = t.backend().buffer();
    let rules = rules(&t);
    assert!(rules[1] - rules[0] > 76, "{rules:?}");
    // Nothing of the help reaches into the settings' column.
    for y in 3..30 {
        let left: String = (rules[0] + 1..rules[1]).map(|x| buf[(x, y)].symbol()).collect();
        assert!(!left.contains("Morphology"), "{left}");
    }
}

#[test]
fn notes_wrap_to_the_settings_width() {
    use crate::app::{wrap_notes, PipelineRow};
    let note = |t: &str| PipelineRow::Note { text: t.to_string().into() };
    let rows = vec![note("  No background removal with these settings: the inversion takes the total field"), note("  or the wrapped phase itself (see Inversion).")];
    let out = wrap_notes(rows.clone(), 40);
    let texts: Vec<String> = out.iter().map(|r| match r { PipelineRow::Note { text } => text.to_string(), _ => unreachable!() }).collect();
    assert!(texts.iter().all(|t| t.chars().count() <= 40 && t.starts_with("  ")), "{texts:?}");
    assert_eq!(texts.iter().map(|t| t.trim()).collect::<Vec<_>>().join(" "),
        "No background removal with these settings: the inversion takes the total field or the wrapped phase itself (see Inversion).");
    assert_eq!(wrap_notes(rows.clone(), 0), rows);
}

/// The footer (key hints) as drawn on a wide terminal.
pub(crate) fn footer(app: &mut App) -> String {
    let (w, h) = (200, 40);
    let t = render_app_sized(app, w, h);
    let buf = t.backend().buffer();
    (0..w).map(|x| buf[(x, h - 1)].symbol().to_string()).collect()
}

/// Render at a given size, for tabs that are taller than the default 30-row terminal.
pub(super) fn render_app_sized(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    app.prepare_frame();
    terminal.draw(|f| draw(f, app)).unwrap();
    terminal
}

pub(super) fn render_app(app: &mut App) -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    app.prepare_frame();
    terminal.draw(|f| draw(f, app)).unwrap();
    terminal
}

/// Settings are sub-fields of their checkbox and have to read as such: without the indent
/// they sit flush against it and look like independent outputs.
#[test]
fn settings_are_indented_under_their_checkbox() {
    let indent_of = |text: &str, label: &str| -> usize {
        let line = text.lines().find(|l| l.contains(label))
            .unwrap_or_else(|| panic!("{label} not on screen:\n{text}"));
        // In columns: the sidebar beside the rows draws multi-byte characters (χ, │).
        line[..line.find(label).expect("just matched")].chars().count()
    };
    let mut app = App::new();
    app.form.settings.set("pipeline.do_swi", true);
    app.form.settings.set("pipeline.do_smwi", true);
    app.form.settings.set("pipeline.do_segmentation", true);
    assert!(app.goto_field("do_smwi"));
    let text = screen_text(&render_app_sized(&mut app, 120, 60));
    let smwi = indent_of(&text, "Compute SMWI");
    for child in ["Threshold", "Power"] {
        assert!(indent_of(&text, child) > smwi, "{child} should be indented:\n{text}");
    }
    assert!(app.goto_field("do_swi"));
    let text = screen_text(&render_app_sized(&mut app, 120, 60));
    let swi = indent_of(&text, "Compute SWI");
    for child in ["Strength", "HP sigma (x y z)"] {
        assert!(indent_of(&text, child) > swi, "{child} should be indented:\n{text}");
    }
    assert!(app.goto_field("synthseg_version"));
    let text = screen_text(&render_app_sized(&mut app, 120, 60));
    let seg = indent_of(&text, "Segmentation:");
    let source = indent_of(&text, "Source");
    assert!(source > seg, "{text}");
    for child in ["SynthSeg version", "Crop", "Flip averaging", "Topology cleanup", "Posterior smoothing"] {
        assert!(indent_of(&text, child) > source, "{child} belongs under the source:\n{text}");
    }
    // The sidebar is drawn beside them.
    assert!(text.contains("SMWI") && text.contains("Relaxometry") && text.contains("DICOM"), "{text}");
}

#[test]
fn test_draw_default_app_no_panic() {
    let mut app = App::new();
    let _ = render_app(&mut app);
}

/// The rendered screen as one string, for substring assertions.
pub(super) fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn example_app() -> App {
    let mut app = App::new();
    app.input_mode = crate::app::InputMode::Example;
    app
}

#[test]
fn example_mode_renders_the_picker() {
    // Focus on the IO fields: the top of the panel is in view.
    let mut app = example_app();
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Example dataset"), "mode label missing:\n{text}");
    assert!(text.contains("Dataset directory"), "IO label missing:\n{text}");
    assert!(text.contains("prisma-bridge-run1"), "default acquisition missing:\n{text}");
    assert!(text.contains("[x]"), "default acquisition not ticked:\n{text}");
}

#[test]
fn example_mode_opens_on_the_download_button() {
    // Focus enters the picker on the button, so that fetching the default acquisition
    // is a single Enter.
    let mut app = example_app();
    app.active_field = App::INPUT_IO_FIELDS;
    assert_eq!(
        app.example_state.cursor,
        crate::app::ExampleState::BUTTON
    );
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Download"), "button missing:\n{text}");
    assert!(text.contains("1 selected"), "selection summary missing:\n{text}");
}

#[test]
fn example_mode_shows_the_default_dataset_directory() {
    let mut app = example_app();
    let text = screen_text(&render_app(&mut app));
    assert!(
        text.contains(crate::app::EXAMPLE_DEFAULT_DIR),
        "default dir placeholder missing:\n{text}"
    );
}

#[test]
fn example_picker_tracks_the_cursor_without_panicking() {
    // Walk from the button to the last acquisition: the focused row stays in view and the
    // IO fields stay above it, at any height, and scrolling never covers the button. A
    // terminal too short for the separate list still shows the focused row.
    let mut app = example_app();
    app.active_field = App::INPUT_IO_FIELDS;
    for height in [16, 24, 30, 60] {
        app.example_state.scroll_offset = 0;
        for cursor in 0..=crate::app::ExampleState::last_row() {
            app.example_state.cursor = cursor;
            let text = screen_text(&render_app_sized(&mut app, 120, height));
            assert!(text.contains("Output directory") || height < 24, "{height}, {cursor}:\n{text}");
            assert!(text.contains("[ Download ]") || height < 24, "{height}, {cursor}:\n{text}");
            if let Some(i) = app.example_state.acquisition() {
                let id = qsmxt_io::example::EXAMPLES[i].id;
                assert!(text.contains(id), "{height}, {id}:\n{text}");
            }
        }
    }
}

/// The button is above the list.
#[test]
fn example_download_button_is_above_the_list() {
    let mut app = example_app();
    let text = screen_text(&render_app(&mut app));
    assert!(text.find("[ Download ]").unwrap() < text.find("prisma-bridge-run1").unwrap(), "{text}");
}

/// A help text longer than a line is wrapped, on the Input tab as on the settings tabs.
#[test]
fn long_help_wraps() {
    let mut app = App::new();
    app.input_mode = crate::app::InputMode::Bids;
    app.form.bids_dir = "/some/dir".to_string();
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.focus = crate::app::FilterFocus::OrientationGroup;
    let text = screen_text(&render_app_sized(&mut app, 60, 30));
    assert!(text.contains("Runs that are the same object"), "{text}");
    assert!(text.contains("COSMOS/STI"), "the end of the text is cut off:\n{text}");
}

/// Once the download starts, the acquisitions give way to the download log, in a box.
#[test]
fn example_mode_renders_download_progress() {
    let mut app = example_app();
    app.active_field = App::INPUT_IO_FIELDS;
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Example acquisitions") && !text.contains("Download log"), "{text}");
    app.example_state.status = crate::app::ConvertStatus::Converting;
    app.example_state.log.push("Downloading prisma-bridge-run1".to_string());
    let text = screen_text(&render_app(&mut app));
    // No progress counters yet, so the button shows the post-download phase.
    assert!(text.contains("Writing dataset"), "status missing:\n{text}");
    assert!(text.contains("Download log") && text.contains("Downloading prisma-bridge-run1"), "log missing:\n{text}");
    assert!(!text.contains("Example acquisitions"), "the list is hidden:\n{text}");
    // While the archive downloads, the button says how far, and nothing offers a key.
    app.example_state.set_progress(173_000_000, 412_000_000);
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Downloading...  41%"), "{text}");
    assert!(!text.contains("download the ticked") && !text.contains("Enter:Download") && !text.contains("Space:Tick"), "{text}");
    // The keys stay on the button.
    app.handle_key(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Down));
    assert_eq!(app.example_state.cursor, crate::app::ExampleState::BUTTON);
}

/// A long list scrolls only when the cursor would leave the window: ↑ from the bottom row
/// moves the cursor, not the list, until it reaches the top row.
#[test]
fn a_list_scrolls_only_when_the_cursor_leaves_it() {
    use crate::app::AlgoModalTarget;
    use crossterm::event::{KeyCode, KeyEvent};
    let mut app = App::new();
    let options: Vec<String> = (0..30).map(|i| format!("option-{i:02}")).collect();
    app.algo_modal = Some(AlgoModal {
        target: AlgoModalTarget::Select("qsm_algorithm".into()),
        title: "QSM inversion".into(),
        help: options.iter().map(|o| format!("help for {o}")).collect(),
        options,
        cursor: 0,
        offset: Default::default(),
    });
    let offset = |app: &App| app.algo_modal.as_ref().unwrap().offset.get();
    for _ in 0..20 {
        app.handle_key(KeyEvent::from(KeyCode::Down));
        render_app_sized(&mut app, 120, 40);
    }
    let low = offset(&app);
    assert!(low > 0, "the list scrolled down with the cursor");
    app.handle_key(KeyEvent::from(KeyCode::Up));
    render_app_sized(&mut app, 120, 40);
    assert_eq!(offset(&app), low, "↑ from the bottom row does not scroll");
    // Up to the top row, then once more: now it scrolls.
    while app.algo_modal.as_ref().unwrap().cursor > low {
        app.handle_key(KeyEvent::from(KeyCode::Up));
        render_app_sized(&mut app, 120, 40);
        assert_eq!(offset(&app), low);
    }
    app.handle_key(KeyEvent::from(KeyCode::Up));
    render_app_sized(&mut app, 120, 40);
    assert_eq!(offset(&app), low - 1);
}

/// The Methods tab's pages are a sidebar, as the Reconstruction tab's sections are; the
/// Pipeline tab's list is styled the same, with no explanation above it and no "Now:" line.
#[test]
fn the_methods_and_pipeline_tabs_have_sidebars() {
    use crossterm::event::{KeyCode, KeyEvent};
    let mut app = App::new();
    app.handle_key(KeyEvent::from(KeyCode::Char('5')));
    let text = screen_text(&render_app_sized(&mut app, 120, 40));
    assert!(text.contains("▸ Methods text") && text.contains("   Graph"), "{text}");
    assert!(!text.contains("sub-tab"), "no pills beside a sidebar:\n{text}");
    app.handle_key(KeyEvent::from(KeyCode::Down));
    assert_eq!(app.page(), Some(crate::app::Page::Pipeline));
    app.handle_key(KeyEvent::from(KeyCode::Right));
    assert!(!app.on_subtabs());
    app.handle_key(KeyEvent::from(KeyCode::Left));
    assert!(app.on_subtabs(), "← returns to the sidebar");

    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    let text = screen_text(&render_app_sized(&mut app, 120, 40));
    assert!(text.contains("TOOLBOX PIPELINES") && text.contains("▸ QSMxT defaults"), "{text}");
    for gone in ["Now:", "Start from a pipeline", "start from QSMxT's defaults"] {
        assert!(!text.contains(gone), "{gone}:\n{text}");
    }
}

#[test]
fn example_mode_renders_an_error_from_the_worker() {
    let mut app = example_app();
    app.active_field = App::INPUT_IO_FIELDS;
    app.example_state.status = crate::app::ConvertStatus::Error;
    app.example_state.log.push("Error: checksum mismatch".to_string());
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("failed"), "failed marker missing:\n{text}");
    assert!(text.contains("Error: checksum mismatch"), "error text missing:\n{text}");
}

/// The prompt has to name both versions and every key it accepts, or it is just an
/// interruption the user cannot act on.
#[test]
fn update_modal_shows_both_versions_and_its_choices() {
    use crate::app::UpdateModal;
    let mut app = App::new();
    app.update_modal = Some(UpdateModal {
        tag: "v9.23.0".into(),
        html_url: "https://github.com/QSMxT/QSMxT/releases/tag/v9.23.0".into(),
    });

    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Update available"), "missing title:\n{text}");
    assert!(text.contains("v9.23.0"), "missing the new version:\n{text}");
    assert!(
        text.contains(qsmxt_io::release::current_version()),
        "missing the running version:\n{text}"
    );
    assert!(text.contains("update now"), "missing the accept hint:\n{text}");
    assert!(text.contains("later"), "missing the defer hint:\n{text}");
    assert!(!text.contains("skip"), "a release cannot be skipped:\n{text}");
}

/// An announcement pop-up shows its date, title, wrapped body, range and link; a critical
/// one says it comes back until the update.
#[test]
fn an_announcement_pop_up_shows_everything_in_it() {
    use crate::app::NewsModal;
    let mut a = qsmxt_io::news::item("a", "2026-11-06", true, Some(("10.0.0", "11.0.1")));
    a.title = "Wrong results from the thing".into();
    a.body = format!("{}\n\nSecond paragraph.", "word ".repeat(40));
    a.link = Some("https://example.com/a".into());
    let mut app = App::new();
    app.news_modal = Some(NewsModal { items: vec![a], history: false, note: None, scroll: 0 });
    let text = screen_text(&render_app_sized(&mut app, 120, 40));
    for want in ["Important announcement", "2026-11-06", "Important", "Wrong results from the thing",
                 "Second paragraph.", "Affects versions from 10.0.0; fixed in 11.0.1.",
                 "until you update to 11.0.1", "https://example.com/a", "c copy link", "Enter close"] {
        assert!(text.contains(want), "missing {want:?}:\n{text}");
    }
    assert!(text.lines().all(|l| l.matches("word").count() < 40), "the body is wrapped:\n{text}");
}

/// The history scrolls when it does not fit, and an empty one says so.
#[test]
fn the_announcement_history_scrolls_and_says_when_empty() {
    use crate::app::NewsModal;
    let items: Vec<_> = (0..12).map(|i| qsmxt_io::news::item(&format!("n{i}"), &format!("2026-11-{:02}", 20 - i), false, None)).collect();
    let mut app = App::new();
    app.news_modal = Some(NewsModal { items, history: true, note: None, scroll: 0 });
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Announcements") && text.contains("Title n0"), "{text}");
    assert!(text.contains("scroll"), "a long history scrolls:\n{text}");
    assert!(!text.contains("Title n11"), "{text}");
    app.news_modal.as_mut().unwrap().scroll = 500;
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Title n11"), "scrolled to the end:\n{text}");

    app.news_modal = Some(NewsModal { items: Vec::new(), history: true, note: None, scroll: 0 });
    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("There are no announcements."), "{text}");
}

/// The tab bar's key hints include the history.
#[test]
fn the_tab_bar_key_bar_offers_the_news() {
    let mut app = App::new();
    app.tab_focus = true;
    assert!(footer(&mut app).contains("n:News"), "{}", footer(&mut app));
}

/// The prompt is drawn after the algorithm modal so it is answered first — the key
/// handler routes to it unconditionally, so it must not render underneath.
#[test]
fn update_modal_draws_on_top_of_an_algorithm_modal() {
    use crate::app::{AlgoModalTarget, UpdateModal};
    let mut app = App::new();
    app.algo_modal = Some(AlgoModal {
        target: AlgoModalTarget::Select("qsm_algorithm".into()),
        title: "QSM inversion".into(),
        options: (0..20).map(|i| format!("algo-{i}")).collect(),
        help: (0..20).map(|i| format!("help {i}")).collect(),
        cursor: 0,
        offset: Default::default(),
    });
    app.update_modal = Some(UpdateModal {
        tag: "v9.23.0".into(),
        html_url: "https://example.com/r".into(),
    });

    let text = screen_text(&render_app(&mut app));
    assert!(text.contains("Update available"), "the update prompt must win:\n{text}");
}

/// A narrow terminal must not panic or clip the prompt out of existence.
#[test]
fn update_modal_survives_a_small_terminal() {
    use crate::app::UpdateModal;
    let mut app = App::new();
    app.update_modal = Some(UpdateModal {
        tag: "v9.23.0".into(),
        html_url: "https://github.com/QSMxT/QSMxT/releases/tag/v9.23.0".into(),
    });
    let text = screen_text(&render_app_sized(&mut app, 40, 12));
    assert!(text.contains("Update"), "prompt vanished on a narrow screen:\n{text}");
}

#[test]
fn test_algo_modal_scrollbar_and_legend() {
    use crate::app::AlgoModalTarget;
    let mut app = App::new();
    // A long list with deep-learning entries at the end (like the QSM inversion selector).
    let mut options: Vec<String> = (0..18).map(|i| format!("classical-{i}")).collect();
    options.extend(["xqsm", "qsmnet", "autoqsm", "nextqsm"].iter().map(|s| s.to_string()));
    let help: Vec<String> = options.iter().map(|o| format!("help for {o}")).collect();
    app.algo_modal = Some(AlgoModal {
        target: AlgoModalTarget::Select("qsm_algorithm".into()),
        title: "QSM inversion".into(),
        options,
        help,
        cursor: 20, // inside the DL group, past the capped viewport → list must scroll
        offset: Default::default(),
    });
    let term = render_app(&mut app);
    let buf = term.backend().buffer();
    let mut text = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            text.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(text.contains('█'), "expected a scrollbar thumb in the overflowing modal");
    assert!(text.contains("deep learning"), "expected the deep-learning legend in the footer");
    assert!(text.contains("select") && text.contains("cancel"), "expected the hotkey footer");
}

#[test]
fn test_draw_all_tabs() {
    // Every tab, with its sections open and with the settings that reveal the most rows:
    // QSM off and forced back on by SMWI, a grouping, motion correction, every output.
    let mut app = App::new();
    for setup in [|_: &mut App| {}, |a: &mut App| {
        a.form.settings.set("pipeline.do_qsm", false);
        a.form.settings.set("pipeline.do_smwi", true);
        a.form.settings.set("pipeline.do_swi", true);
        a.form.settings.set("pipeline.do_analysis", true);
        a.form.settings.set("pipeline.do_r2primemap", true);
        a.form.export_dicom = true;
        a.filter_state.orientation_group = "acq".into();
        a.form.settings.set("motion_correction.enabled", true);
        a.form.settings.set("pipeline.do_chi_separation", true);
    }] {
        setup(&mut app);
        for (tab, name) in TAB_NAMES.iter().enumerate() {
            app.active_tab = tab;
            app.active_field = 0;
            let text = screen_text(&render_app(&mut app));
            assert!(text.contains(&format!("{} {name}", tab + 1)), "tab bar on tab {tab}:\n{text}");
            assert!(text.contains(&format!("1-{}", TAB_NAMES.len())), "help bar names every tab digit");
        }
    }
}

#[test]
fn test_draw_editing_mode() {
    let mut app = App::new();
    app.editing = true;
    app.form.bids_dir = "/some/path".to_string();
    app.cursor_pos = 5;
    let _ = render_app(&mut app);
}

#[test]
fn test_draw_with_form_data() {
    let mut app = App::new();
    app.form.bids_dir = "/data/bids".to_string();
    app.form.output_dir = "/data/out".to_string();
    let _ = render_app(&mut app);
}

#[test]
fn test_draw_algorithms_tab() {
    let mut app = App::new();
    app.active_tab = super::super::app::TAB_RECONSTRUCTION;
    app.active_field = 0;
    let _ = render_app(&mut app);
    // Move through fields
    app.active_field = 4;
    let _ = render_app(&mut app);
}

/// Every signal-erode parameter gets its own labelled row under the step.
#[test]
fn test_draw_signal_erode_param_rows() {
    use crate::app::{signal_erode_params, PipelineRow};
    let mut app = App::new();
    app.apply_mask_preset(2); // hd-bet + signal-erode
    assert!(app.goto_field("mask_preset"));

    let rows = app.rows();
    let params: Vec<usize> = rows.iter().filter_map(|r| match r {
        PipelineRow::MaskOpSignalErodeParam { param, .. } => Some(*param),
        _ => None,
    }).collect();
    assert_eq!(params, (0..signal_erode_params().len()).collect::<Vec<_>>());

    // Focus the last one, so every param row is scrolled into view together.
    app.pipeline_state.focus = app.focusable_rows().iter()
        .position(|&ri| matches!(rows[ri], PipelineRow::MaskOpSignalErodeParam { param: 3, .. }))
        .expect("min component row not focusable");
    let terminal = render_app(&mut app);
    let text: String = terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect();
    for (label, _) in signal_erode_params() {
        assert!(text.contains(label), "{label} row not drawn");
    }
    assert!(text.contains("connected component"), "focused param's help not drawn");
}

/// A two-section recipe renders its combine mode and the steps that run on the combined mask.
#[test]
fn test_draw_mask_combine_rows() {
    use crate::app::{PipelineRow, MASK_COMBINED_SECTION};
    let mut app = App::new();
    app.apply_mask_preset(3); // bet-and-phase: two sections, AND
    assert!(app.goto_field("mask_preset"));

    // Focus the combine row so the focused branch renders too.
    let rows = app.rows();
    assert!(rows.iter().any(|r| matches!(r, PipelineRow::MaskCombinedHeader)));
    assert!(rows.iter().any(|r| matches!(r, PipelineRow::MaskOpEntry { section: MASK_COMBINED_SECTION, .. })));
    app.pipeline_state.focus = app.focusable_rows().iter()
        .position(|&ri| matches!(rows[ri], PipelineRow::MaskCombineOp { .. }))
        .expect("MaskCombineOp not focusable");

    let terminal = render_app(&mut app);
    let text: String = terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect();
    assert!(text.contains("COMBINED WITH"), "combine row not drawn");
    assert!(text.contains("AND"), "combine mode not drawn");
}

#[test]
fn test_draw_parameters_tab() {
    let mut app = App::new();
    app.active_tab = super::super::app::TAB_RECONSTRUCTION;
    let _ = render_app(&mut app);
    // Change algorithm
    app.set_qsm_algorithm(3); // TSVD
    let _ = render_app(&mut app);
}

#[test]
fn test_draw_execution_tab_with_flags() {
    let mut app = App::new();
    app.active_tab = super::super::app::TAB_RECONSTRUCTION;
    app.form.settings.set("pipeline.do_swi", true);
    app.form.settings.set("pipeline.do_t2starmap", true);
    app.form.dry_run = true;
    app.form.debug = true;
    let _ = render_app(&mut app);
}

#[test]
fn test_draw_non_focused_fields() {
    let mut app = App::new();
    app.active_tab = 0;
    app.active_field = 3; // Last field focused, others not
    let _ = render_app(&mut app);
}

#[test]
fn test_draw_select_not_focused() {
    let mut app = App::new();
    app.active_tab = super::super::app::TAB_RECONSTRUCTION;
    app.active_field = 1; // field 0 (select) not focused
    let _ = render_app(&mut app);
}

#[test]
fn test_draw_empty_text_not_editing() {
    let mut app = App::new();
    // All fields empty, not editing — shows "(empty)"
    app.active_tab = 0;
    app.active_field = 0;
    let _ = render_app(&mut app);
}

fn tip_app(tips_enabled: bool) -> App {
    let mut app = App::new();
    app.tip_modal = Some(TipModal::new(crate::tips::TIPS[0], tips_enabled));
    app
}

/// The pop-up shows the tip, its link and both buttons.
#[test]
fn tip_modal_shows_the_tip_its_link_and_buttons() {
    let text = screen_text(&render_app(&mut tip_app(true)));
    assert!(text.contains("Did you know?"), "missing title:\n{text}");
    assert!(text.contains("QSMxT can run on the MRI scanner"), "missing tip:\n{text}");
    assert!(text.contains("https://qsmxt.github.io/recon/"), "missing link:\n{text}");
    assert!(text.contains(" OK "), "missing OK:\n{text}");
    assert!(text.contains("Don't show tips"), "missing opt-out:\n{text}");
}

/// Opened with `?` while tips are off, the second button turns them back on.
#[test]
fn tip_modal_offers_to_turn_tips_back_on() {
    let text = screen_text(&render_app(&mut tip_app(false)));
    assert!(text.contains("Show tips at startup"), "{text}");
    assert!(!text.contains("Don't show tips"), "{text}");
}

/// Every tip wraps without losing words, and a narrow terminal does not panic.
#[test]
fn every_tip_wraps_without_losing_words() {
    for tip in crate::tips::TIPS {
        let lines = tip_lines(tip.text, 62);
        let joined: String = lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(" ");
        assert_eq!(joined, tip.text.replace('`', ""));
        assert!(lines.iter().all(|l| l.width() <= 62), "{}", tip.text);
    }
    let mut app = tip_app(true);
    let text = screen_text(&render_app_sized(&mut app, 40, 12));
    assert!(text.contains("Did you know"), "{text}");
}
