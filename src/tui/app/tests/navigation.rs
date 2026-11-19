//! The Reconstruction sidebar and sub-tabs, the Execution tab's Run button, the main tab bar.

use super::*;

// ─── The Reconstruction sidebar (and its pill strip when narrow): focus, arrows, back ───

#[test]
fn reconstruction_opens_on_its_sidebar_at_masking() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('3')));
    assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
    assert!(app.on_subtabs() && app.sidebar());
    assert_eq!(app.page(), Some(Page::Masking), "Reconstruction opens on Masking");
    // ↓/↑ move through the sections, across groups, without wrapping; focus stays.
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.page(), Some(Page::Motion));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.page(), Some(Page::ComputeQsm), "into the next group");
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.page(), Some(Page::FieldMap));
    assert!(app.on_subtabs());
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.page(), Some(Page::Export));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.page(), Some(Page::Export), "no wrap at the end");
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.page(), Some(Page::Geometry));
    // `[`/`]` switch too, wrapping.
    app.handle_key(key(KeyCode::Char('[')));
    assert_eq!(app.page(), Some(Page::Export));
    assert!(app.on_subtabs());
    // ← does nothing in the sidebar.
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.page(), Some(Page::Export));
    assert!(app.on_subtabs());
}

#[test]
fn right_enters_the_section_and_up_from_its_first_row_returns() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('3'))); // Masking, the default
    assert!(app.focusable_rows().len() > 1);
    app.handle_key(key(KeyCode::Right));
    assert!(!app.on_subtabs());
    assert_eq!(app.focus(), 0);
    assert_eq!(app.page(), Some(Page::Masking));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.focus(), 1);
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Up));
    assert!(app.on_subtabs(), "↑ from the first row goes back to the sidebar");
    assert_eq!(app.page(), Some(Page::Masking));
    // Enter goes in too; from the first section ↑ goes to the tab bar and ↓ comes back.
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.on_subtabs());
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.page(), Some(Page::Geometry));
    app.handle_key(key(KeyCode::Up));
    assert!(app.tab_focus);
    app.handle_key(key(KeyCode::Down));
    assert!(!app.tab_focus && app.on_subtabs());
    assert_eq!(app.page(), Some(Page::Geometry));
}

/// → on a row changes nothing (no row cycles); ← goes back to the sidebar on the same
/// section, and → comes back in.
#[test]
fn right_on_a_row_changes_nothing_and_left_returns_to_the_sidebar() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('3')));
    app.handle_key(key(KeyCode::Char(']'))); // Motion
    app.handle_key(key(KeyCode::Char(']'))); // Compute QSM
    app.handle_key(key(KeyCode::Char(']'))); // Field map
    assert_eq!(app.page(), Some(Page::FieldMap));
    app.handle_key(key(KeyCode::Right));
    let field = app.focused_field_name();
    let before = crate::tui::command::build_command_string(&app);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.page(), Some(Page::FieldMap), "→ on a row switched the section");
    assert!(!app.on_subtabs());
    assert_eq!(app.focused_field_name(), field);
    assert_eq!(crate::tui::command::build_command_string(&app), before, "→ changed a value");
    app.handle_key(key(KeyCode::Left));
    assert!(app.on_subtabs() && app.sidebar());
    assert_eq!(app.page(), Some(Page::FieldMap));
    app.handle_key(key(KeyCode::Left));
    assert!(app.on_subtabs(), "← on the sidebar stays there");
    assert_eq!(app.page(), Some(Page::FieldMap));
    app.handle_key(key(KeyCode::Right));
    assert!(!app.on_subtabs());
}

/// Everything on, so every section has its full rows (and the mask has every kind of row).
fn app_with_every_row() -> App {
    use qsmxt_engine::{MaskOp, MaskSection, MaskingInput};
    let mut app = App::new();
    app.form.settings.set("pipeline.do_swi", true);
    app.form.settings.set("pipeline.do_smwi", true);
    app.form.export_dicom = true;
    app.form.settings.set("pipeline.do_t2starmap", true);
    app.form.settings.set("pipeline.do_r2starmap", true);
    app.form.settings.set("pipeline.do_chi_separation", true);
    app.apply_mask_preset(2); // hd-bet + signal-erode
    let mut m = app.mask_mut();
    m.sections[0].generator = MaskOp::HdBet { patch: Some(MaskOp::hd_bet_default_patch()), tta: false,
        tile_step: qsmxt_engine::hd_bet_default_tile_step() };
    m.sections.push(MaskSection { input: MaskingInput::Magnitude, generator: MaskOp::bet(0.5), refinements: vec![
        MaskOp::Erode { iterations: 1 }, MaskOp::FillHoles { max_size: 0 }, MaskOp::GaussianSmooth { sigma_mm: 1.0 },
        MaskOp::MinIntensity { value: 100.0 },
    ] });
    m.sections.push(MaskSection { input: MaskingInput::Magnitude, generator: MaskOp::MindGrab { tta: false }, refinements: vec![] });
    drop(m);
    app.pipeline_state.mask_preset = *MASK_PRESET_CUSTOM;
    app
}

/// ← from every row of every section goes back to the sections (the sidebar, or the pill
/// strip on a narrow terminal) on that section, and changes nothing.
#[test]
fn left_from_every_row_of_every_section_returns_to_the_sections() {
    for wide in [true, false] {
        let mut app = app_with_every_row();
        app.sidebar_layout.set(wide);
        app.handle_key(key(KeyCode::Char('3')));
        for page in app.pages(TAB_RECONSTRUCTION) {
            goto_page(&mut app, page);
            let n = app.focusable_rows().len();
            if n == 0 {
                // A section with only a note (COSMOS/STI without groups): in and back out.
                app.subtab_focus = false;
                app.handle_key(key(KeyCode::Left));
                assert!(app.on_subtabs() && app.page() == Some(page), "{page:?}");
            }
            for i in 0..n {
                app.subtab_focus = false;
                app.set_focus(i);
                let before = crate::tui::command::build_command_string(&app);
                let row = app.focused_row();
                app.handle_key(key(KeyCode::Left));
                assert!(app.on_subtabs(), "{page:?} row {i} {row:?}: ← did not go back (wide: {wide})");
                assert_eq!(app.sidebar(), wide);
                assert_eq!(app.page(), Some(page), "{page:?} row {i}: ← changed the section");
                assert!(app.algo_modal.is_none() && !app.pipeline_state.mask_value_editing);
                assert_eq!(crate::tui::command::build_command_string(&app), before, "{page:?} row {i} {row:?}: ← changed a value");
                // → comes back in (onto the first row), and changes nothing either.
                app.handle_key(key(if wide { KeyCode::Right } else { KeyCode::Down }));
                assert!(!app.on_subtabs());
                app.set_focus(i);
                app.handle_key(key(KeyCode::Right));
                assert!(!app.on_subtabs() && app.page() == Some(page) && app.algo_modal.is_none(), "{page:?} row {i}: →");
                assert_eq!(crate::tui::command::build_command_string(&app), before, "{page:?} row {i} {row:?}: → changed a value");
            }
        }
    }
}

/// While a number is typed, ←/→ move the cursor; ← goes back to the sections only after.
#[test]
fn left_while_typing_moves_the_cursor() {
    let mut app = App::new();
    goto_page(&mut app, Page::Inversion);
    let rows = app.rows();
    let fi = app.focusable_rows().iter().position(|&ri| matches!(rows[ri], PipelineRow::Param { .. })).expect("a value row");
    app.set_focus(fi);
    app.subtab_focus = false;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.editing);
    let at = app.pipeline_state.cursor;
    app.handle_key(key(KeyCode::Left));
    assert!(app.pipeline_state.editing && !app.on_subtabs());
    assert_eq!(app.pipeline_state.cursor, at.saturating_sub(1));
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(key(KeyCode::Left));
    assert!(app.on_subtabs());

    // The same for a mask row's number.
    let mut app = app_with_every_row();
    goto_page(&mut app, Page::Masking);
    let rows = app.rows();
    let fi = app.focusable_rows().iter().position(|&ri| matches!(rows[ri], PipelineRow::MaskOpEntry { .. })).unwrap();
    app.set_focus(fi);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.mask_value_editing);
    let at = app.pipeline_state.cursor;
    app.handle_key(key(KeyCode::Left));
    assert!(app.pipeline_state.mask_value_editing && !app.on_subtabs());
    assert_eq!(app.pipeline_state.cursor, at - 1);
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(key(KeyCode::Left));
    assert!(app.on_subtabs());
}

/// Every choice on the tab opens a list (Enter, or Space) with the current option
/// highlighted; Esc closes it with nothing changed, and Enter on another option chooses it.
#[test]
fn every_choice_opens_a_list_that_commits_or_cancels() {
    let mut app = app_with_every_row();
    app.handle_key(key(KeyCode::Char('3')));
    let mut kinds = HashSet::new();
    for page in app.pages(TAB_RECONSTRUCTION) {
        goto_page(&mut app, page);
        let mut i = 0;
        while i < app.focusable_rows().len() {
            app.set_focus(i);
            let row = app.focused_row().unwrap();
            i += 1;
            let Some(expected) = app.selector_modal(&row) else { continue };
            if expected.options.len() < 2 { continue; }
            kinds.insert(std::mem::discriminant(&expected.target));
            let before = crate::tui::command::build_command_string(&app);
            for open in [KeyCode::Enter, KeyCode::Char(' ')] {
                app.handle_key(key(open));
                let m = app.algo_modal.as_ref().unwrap_or_else(|| panic!("{page:?} {row:?}: {open:?} opened no list"));
                assert_eq!((m.options.clone(), m.cursor), (expected.options.clone(), expected.cursor), "{row:?}");
                app.handle_key(key(KeyCode::Down));
                app.handle_key(key(KeyCode::Esc));
                assert!(app.algo_modal.is_none());
                assert_eq!(crate::tui::command::build_command_string(&app), before, "{row:?}: Esc changed it");
            }
            // A selector the rules or the run treat specially is left to its own tests.
            if matches!(row, PipelineRow::AlgoSelect { .. } | PipelineRow::MaskOpGenerator { .. } | PipelineRow::MaskOpAddStep { .. }) { continue; }
            app.handle_key(key(KeyCode::Enter));
            let other = (expected.cursor + 1) % expected.options.len();
            app.algo_modal.as_mut().unwrap().cursor = other;
            app.handle_key(key(KeyCode::Enter));
            assert!(app.algo_modal.is_none());
            assert_eq!(app.selector_modal(&row).map(|m| m.cursor), Some(other), "{row:?}: not chosen");
            // And back.
            app.handle_key(key(KeyCode::Enter));
            app.algo_modal.as_mut().unwrap().cursor = expected.cursor;
            app.handle_key(key(KeyCode::Enter));
            assert_eq!(crate::tui::command::build_command_string(&app), before, "{row:?}: not restored");
        }
    }
    use AlgoModalTarget as T;
    for target in [T::Select(String::new()), T::MaskInput(0), T::MaskGenerator(0), T::MaskThresholdMethod(0),
                   T::MaskCombine(0), T::MaskHdBetInference(0), T::MaskHdBetStep(0), T::MaskBetVoxelScale(0), T::MaskAddStep(0)] {
        if matches!(target, T::MaskThresholdMethod(_)) { continue; } // no threshold generator here: below
        assert!(kinds.contains(&std::mem::discriminant(&target)), "no row opened {target:?}");
    }
}

/// `+` / `-` nudge a masking number by its old step, within its range; they do nothing on
/// any other row.
#[test]
fn plus_and_minus_nudge_mask_numbers() {
    use qsmxt_engine::MaskOp;
    let mut app = app_with_every_row();
    goto_page(&mut app, Page::Masking);
    let focus = |app: &mut App, want: PipelineRow| {
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| rows[ri] == want).expect("row");
        app.set_focus(fi);
    };
    focus(&mut app, PipelineRow::MaskOpGeneratorParam { section: 1 });
    app.handle_key(key(KeyCode::Char('+')));
    assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.55));
    app.handle_key(key(KeyCode::Char('-')));
    app.handle_key(key(KeyCode::Char('-')));
    assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.45));
    focus(&mut app, PipelineRow::MaskOpEntry { section: 1, index: 0 });
    app.handle_key(key(KeyCode::Char('-')));
    assert_eq!(app.mask().sections[1].refinements[0], MaskOp::Erode { iterations: 1 }, "stays at least 1");
    app.handle_key(key(KeyCode::Char('=')));
    assert_eq!(app.mask().sections[1].refinements[0], MaskOp::Erode { iterations: 2 }, "= is + unshifted");
    focus(&mut app, PipelineRow::MaskOpEntry { section: 1, index: 3 });
    app.handle_key(key(KeyCode::Char('+')));
    assert_eq!(app.mask().sections[1].refinements[3], MaskOp::MinIntensity { value: 110.0 });
    let MaskOp::SignalErode { depth_cap, .. } = app.mask().sections[0].refinements[0].clone() else { panic!() };
    focus(&mut app, PipelineRow::MaskOpSignalErodeParam { section: 0, index: 0, param: 0 });
    app.handle_key(key(KeyCode::Char('+')));
    assert!(matches!(app.mask().sections[0].refinements[0], MaskOp::SignalErode { depth_cap: d, .. } if d == depth_cap + 1));
    // Not on a choice.
    let before = crate::tui::command::build_command_string(&app);
    focus(&mut app, PipelineRow::MaskOpHdBetStep { section: 0 });
    app.handle_key(key(KeyCode::Char('+')));
    assert_eq!(crate::tui::command::build_command_string(&app), before);
}

/// A DICOM series' type is chosen from a list (Enter), current type highlighted; ←/→ leave it.
#[test]
fn a_dicom_series_type_is_chosen_from_a_list() {
    use qsmxt_io::dicom::{DicomAcquisition, DicomSeries, DicomSession, DicomStudy, DicomSubject, SeriesType};
    let mut app = App::new();
    app.input_mode = InputMode::DicomToBids;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    let series = DicomSeries { series_uid: "1".into(), description: "gre".into(), series_number: 1,
        series_type: SeriesType::Magnitude, auto_type: SeriesType::Magnitude, ..Default::default() };
    app.dicom_state.session = Some(DicomSession { subjects: vec![DicomSubject { patient_id: "p".into(), studies: vec![DicomStudy {
        study_date: "20240101".into(), acquisitions: vec![DicomAcquisition { name: "gre".into(), run_number: 1, series: vec![series] }] }] }],
        ..Default::default() });
    app.dicom_state.focus = DicomFocus::Series(0);
    let type_now = |app: &App| app.dicom_state.session.as_ref().unwrap().subjects[0].studies[0].acquisitions[0].series[0].series_type;
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Left));
    assert_eq!(type_now(&app), SeriesType::Magnitude, "←/→ change nothing");
    assert!(app.algo_modal.is_none());
    app.handle_key(key(KeyCode::Enter));
    let m = app.algo_modal.as_ref().expect("the series-type list");
    assert_eq!((m.options.len(), m.cursor), (SeriesType::ALL.len(), 0));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(type_now(&app), SeriesType::Magnitude, "Esc changes nothing");
    app.handle_key(key(KeyCode::Char(' ')));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(type_now(&app), SeriesType::Phase);
}

/// A threshold's method (the generator's, or a refinement's from a loaded config) is chosen
/// from a list.
#[test]
fn a_threshold_method_is_chosen_from_a_list() {
    use qsmxt_engine::{MaskOp, MaskThresholdMethod};
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.mask_mut().sections[0].refinements.push(MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None });
    let rows = app.rows();
    let gen = app.focusable_rows().iter().position(|&ri| matches!(rows[ri], PipelineRow::MaskOpGeneratorParam { .. })).unwrap();
    app.set_focus(gen);
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.algo_modal.as_ref().map(|m| &m.target), Some(AlgoModalTarget::MaskThresholdMethod(0))));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.mask().sections[0].generator, MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. }));
    // (The rows moved: a percentile has a value row.)
    let rows = app.rows();
    let refinement = app.focusable_rows().iter().rposition(|&ri| matches!(rows[ri], PipelineRow::MaskOpEntry { .. })).unwrap();
    app.set_focus(refinement);
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.algo_modal.as_ref().map(|m| &m.target), Some(AlgoModalTarget::MaskRefinementThresholdMethod { section: 0, .. })));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.mask().sections[0].refinements.last(), Some(MaskOp::Threshold { method: MaskThresholdMethod::Fixed, .. })));
}

/// The masking numbers are typed: BET's fractional intensity and each refinement step's
/// value, checked on Enter (refused out of range, still editing), Esc leaving them; MindGrab's
/// augmentation is a switch.
#[test]
fn mask_numbers_are_typed_and_mindgrab_tta_is_a_switch() {
    use qsmxt_engine::MaskOp;
    let mut app = app_with_every_row();
    goto_page(&mut app, Page::Masking);
    let focus = |app: &mut App, pred: &dyn Fn(&PipelineRow, &App) -> bool| {
        let rows = app.rows();
        let fi = app.focusable_rows().iter().position(|&ri| pred(&rows[ri], app)).expect("row");
        app.set_focus(fi);
    };
    // BET's fractional intensity (section 1).
    focus(&mut app, &|r, _| *r == PipelineRow::MaskOpGeneratorParam { section: 1 });
    type_mask_value(&mut app, "0.35");
    assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.35));
    type_mask_value(&mut app, "1.5");
    assert!(app.pipeline_state.mask_value_editing && app.error_message.as_deref().is_some_and(|e| e.contains("at most 1")));
    app.handle_key(key(KeyCode::Esc));
    assert!(matches!(app.mask().sections[1].generator, MaskOp::Bet { fractional_intensity: f, .. } if f == 0.35));
    // Each refinement of section 1.
    for (index, typed, check) in [
        (0usize, "3", (|op: &MaskOp| *op == MaskOp::Erode { iterations: 3 }) as fn(&MaskOp) -> bool),
        (1, "500", |op| *op == MaskOp::FillHoles { max_size: 500 }),
        (2, "2.5", |op| *op == MaskOp::GaussianSmooth { sigma_mm: 2.5 }),
        (3, "0.25", |op| *op == MaskOp::MinIntensity { value: 0.25 }),
    ] {
        focus(&mut app, &|r, _| *r == PipelineRow::MaskOpEntry { section: 1, index });
        type_mask_value(&mut app, typed);
        assert!(!app.pipeline_state.mask_value_editing, "{typed}");
        assert!(check(&app.mask().sections[1].refinements[index]), "{:?}", app.mask().sections[1].refinements[index]);
    }
    focus(&mut app, &|r, _| *r == PipelineRow::MaskOpEntry { section: 1, index: 0 });
    type_mask_value(&mut app, "0");
    assert!(app.error_message.as_deref().is_some_and(|e| e.contains("at least 1")), "an erosion of 0 is refused");
    app.handle_key(key(KeyCode::Esc));
    // MindGrab (section 2): Space or Enter switch its augmentation.
    focus(&mut app, &|r, _| *r == PipelineRow::MaskOpGeneratorParam { section: 2 });
    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.mask().sections[2].generator, MaskOp::MindGrab { tta: true });
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mask().sections[2].generator, MaskOp::MindGrab { tta: false });
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.mask().sections[2].generator, MaskOp::MindGrab { tta: false }, "→ changes nothing");
}

/// Too narrow for the sidebar, the sections are a pill strip: ←/→ move along it, wrapping,
/// and ↓ enters the rows.
#[test]
fn a_narrow_terminal_keeps_the_pill_strip_keys() {
    let mut app = App::new();
    app.sidebar_layout.set(false);
    app.handle_key(key(KeyCode::Char('3')));
    assert!(app.on_subtabs() && !app.sidebar());
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.page(), Some(Page::Motion));
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.page(), Some(Page::Export), "wraps");
    app.handle_key(key(KeyCode::Down));
    assert!(!app.on_subtabs());
    assert_eq!(app.focus(), 0);
}

/// The Execution tab has no pages to choose: focus starts on its first row.
#[test]
fn a_tab_without_sections_opens_on_its_first_row() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char(char::from_digit(TAB_EXECUTION as u32 + 1, 10).unwrap())));
    assert_eq!(app.active_tab, TAB_EXECUTION);
    assert!(!app.on_subtabs());
    assert_eq!(app.focus(), 0);
    app.handle_key(key(KeyCode::Up));
    assert!(app.tab_focus, "↑ from the first row goes to the tab bar");
    app.handle_key(key(KeyCode::Down));
    assert!(!app.tab_focus);
}

/// Tab, Shift-Tab and the digits land on the sidebar; `q` still quits from it.
#[test]
fn tab_switching_and_quitting_work_from_the_sidebar() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, TAB_PIPELINE);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
    assert!(app.on_subtabs());
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, TAB_EXECUTION);
    app.handle_key(key(KeyCode::Char('3')));
    assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
    assert!(app.on_subtabs());
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

// ─── The Execution tab's Run button ───

fn focus_run_button(app: &mut App) {
    app.handle_key(key(KeyCode::Char(char::from_digit(TAB_EXECUTION as u32 + 1, 10).unwrap())));
    for _ in 0..40 {
        app.handle_key(key(KeyCode::Down));
    }
    assert!(matches!(app.focused_row(), Some(PipelineRow::RunButton)), "{:?}", app.focused_row());
}

#[test]
fn the_run_button_is_the_last_row_and_enter_runs_like_f5() {
    let mut app = App::new();
    app.form.bids_dir = "/tmp/bids".to_string();
    focus_run_button(&mut app);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.should_run);
    assert!(app.error_message.is_none());
}

/// The button goes through the same checks as F5: SLURM without an account is refused.
#[test]
fn the_run_button_validates_like_f5() {
    let mut app = App::new();
    focus_run_button(&mut app);
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.should_run);
    assert_eq!(app.error_message.as_deref(), Some("BIDS directory is required"));

    let mut app = App::new();
    app.form.bids_dir = "/tmp/bids".to_string();
    app.form.execution_mode = 1;
    focus_run_button(&mut app);
    app.handle_key(key(KeyCode::Char(' ')));
    assert!(!app.should_run);
    assert_eq!(app.error_message.as_deref(), Some("SLURM account is required"));
}

#[test]
fn the_run_button_says_what_f5_will_do() {
    let mut app = App::new();
    assert_eq!(app.run_action().0, "Run");
    app.form.dry_run = true;
    assert_eq!(app.run_action().0, "Dry run");
    app.form.execution_mode = 1;
    assert_eq!(app.run_action().0, "Write SLURM scripts");
    app.form.slurm_submit = true;
    assert_eq!(app.run_action().0, "Submit SLURM jobs");
}

#[test]
fn tip_modal_ok_closes_and_leaves_tips_on() {
    let _s = crate::tui::tips::TestSettings::new();
    let mut app = App::new();
    app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.tip_modal.is_none());
    assert!(crate::tui::tips::enabled());
    assert!(!app.should_quit);
}

/// Turning tips off takes a deliberate move to the second button; `?` brings the
/// pop-up back with a button that turns them on again.
#[test]
fn tips_can_be_turned_off_and_back_on_from_the_tui() {
    let _s = crate::tui::tips::TestSettings::new();
    let mut app = App::new();
    app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.tip_modal.is_none());
    assert!(!crate::tui::tips::enabled());
    assert!(crate::tui::tips::startup_tip().is_none());
    assert!(app.status_message.as_deref().unwrap().contains("Press ?"));

    app.handle_key(key(KeyCode::Char('?')));
    let m = app.tip_modal.as_ref().expect("? opens a tip");
    assert_eq!(m.buttons()[1], "Show tips at startup");
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(key(KeyCode::Enter));
    assert!(crate::tui::tips::enabled());
}

#[test]
fn tip_modal_esc_and_q_close_it_without_quitting() {
    let _s = crate::tui::tips::TestSettings::new();
    for code in [KeyCode::Esc, KeyCode::Char('q')] {
        let mut app = App::new();
        app.tip_modal = Some(TipModal::new(crate::tui::tips::TIPS[0], true));
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(code));
        assert!(app.tip_modal.is_none());
        assert!(!app.should_quit);
        assert!(crate::tui::tips::enabled(), "closing must not turn tips off");
    }
}

#[test]
fn n_shows_the_next_tip() {
    let _s = crate::tui::tips::TestSettings::new();
    let mut app = App::new();
    app.open_tip();
    let first = app.tip_modal.as_ref().unwrap().tip;
    app.handle_key(key(KeyCode::Char('n')));
    assert_ne!(app.tip_modal.as_ref().unwrap().tip, first);
}

/// `?` typed into a text field is text, not a request for a tip.
#[test]
fn question_mark_is_typed_while_editing() {
    let _s = crate::tui::tips::TestSettings::new();
    let mut app = App::new();
    app.active_field = 1;
    app.editing = true;
    app.handle_key(key(KeyCode::Char('?')));
    assert!(app.tip_modal.is_none());
}

// ─── The main tab bar ───

/// ↑ from the top of the Input tab reaches the tab bar; ←/→ there walk the tabs, wrapping,
/// and the focus stays on the bar until ↓.
#[test]
fn arrows_walk_the_main_tabs_from_the_tab_bar() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Up));
    assert!(app.tab_focus);
    assert_eq!(app.active_tab, TAB_INPUT);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.active_tab, TAB_PIPELINE);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
    assert!(app.tab_focus);
    assert!(!app.on_subtabs(), "the sidebar must not look focused under the tab bar");
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.active_tab, TAB_EXECUTION);
    for _ in 0..TAB_EXECUTION + 1 {
        app.handle_key(key(KeyCode::Left));
    }
    assert_eq!(app.active_tab, TAB_NAMES.len() - 1, "← from the first tab wraps to the last");
    // Digits still work and keep the bar focused.
    app.handle_key(key(KeyCode::Char('3')));
    assert_eq!(app.active_tab, TAB_RECONSTRUCTION);
    assert!(app.tab_focus);
    // ↓ goes into the tab: onto its sidebar, here.
    app.handle_key(key(KeyCode::Down));
    assert!(!app.tab_focus);
    assert!(app.on_subtabs());
}

/// Enter on the bar opens a tab without sub-tabs at its first row; ↑ from the Methods tab's
/// sub-tabs reaches the strip once they are scrolled to the top, and then the bar.
#[test]
fn the_tab_bar_is_reachable_from_every_tab() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('7')));
    app.handle_key(key(KeyCode::Up));
    assert!(app.tab_focus);
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.tab_focus);
    assert_eq!(app.focus(), 0);

    for page in [Page::MethodsText, Page::Pipeline] {
        app.handle_key(key(KeyCode::Char(char::from_digit(TAB_METHODS as u32 + 1, 10).unwrap())));
        app.sub_page[TAB_METHODS] = Some(page);
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Up));
        assert!(!app.tab_focus && !app.on_subtabs(), "the first ↑ scrolls back up");
        app.handle_key(key(KeyCode::Up));
        assert!(app.on_subtabs());
        // Up the sidebar, then to the bar.
        for _ in 0..2 {
            if !app.tab_focus { app.handle_key(key(KeyCode::Up)); }
        }
        assert!(app.tab_focus);
        app.handle_key(key(KeyCode::Down));
        assert!(!app.tab_focus);
    }
}

/// Keys that act on a setting do nothing while the tab bar has focus; `q` still quits.
#[test]
fn setting_keys_do_nothing_on_the_tab_bar() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('4')));
    app.handle_key(key(KeyCode::Up));
    assert!(app.tab_focus);
    let before = crate::tui::command::build_command_string(&app);
    for k in [KeyCode::Char(' '), KeyCode::Char('r'), KeyCode::Char('R'), KeyCode::Char('[')] {
        app.handle_key(key(k));
        assert!(app.tab_focus, "{k:?} left the tab bar");
    }
    assert_eq!(crate::tui::command::build_command_string(&app), before);
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}
