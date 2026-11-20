//! Keys and editing on the form: navigation, text editing, running, and the form's basics.

use super::*;

// --- App::new ---



// --- Navigation ---

#[test]
fn test_quit_on_q() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_quit_on_esc() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Esc));
    assert!(app.should_quit);
}

#[test]
fn test_tab_switching_numbers() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('3')));
    assert_eq!(app.active_tab, 2);
    app.handle_key(key(KeyCode::Char('1')));
    assert_eq!(app.active_tab, 0);
    app.handle_key(key(KeyCode::Char('4')));
    assert_eq!(app.active_tab, 3);
}



#[test]
fn test_tab_switch_resets_field() {
    let mut app = App::new();
    app.active_field = 2;
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_field, 0);
}

#[test]
fn test_field_navigation_down() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.active_field, 1);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.active_field, 2);
}

#[test]
fn test_field_navigation_up() {
    let mut app = App::new();
    app.active_field = 2;
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.active_field, 1);
    app.handle_key(key(KeyCode::Char('k')));
    assert_eq!(app.active_field, 0);
}


// --- Text editing ---

#[test]
fn test_enter_editing_text_field() {
    let mut app = App::new();
    // Tab 0, field 1 is BIDS Directory (Text) — field 0 is mode selector
    app.active_field = 1;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.editing);
}

#[test]
fn test_type_characters() {
    let mut app = App::new();
    app.active_field = 1; // BIDS Directory
    app.handle_key(key(KeyCode::Enter)); // enter editing
    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('d')));
    app.handle_key(key(KeyCode::Char('a')));
    app.handle_key(key(KeyCode::Char('t')));
    app.handle_key(key(KeyCode::Char('a')));
    assert_eq!(app.form.bids_dir, "/data");
    assert_eq!(app.cursor_pos, 5);
}

#[test]
fn test_backspace() {
    let mut app = App::new();
    app.active_field = 1;
    app.form.bids_dir = "abc".to_string();
    app.handle_key(key(KeyCode::Enter)); // enter editing, cursor at end (3)
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(app.form.bids_dir, "ab");
    assert_eq!(app.cursor_pos, 2);
}

#[test]
fn test_backspace_at_start_does_nothing() {
    let mut app = App::new();
    app.form.bids_dir = "x".to_string();
    app.editing = true;
    app.cursor_pos = 0;
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(app.form.bids_dir, "x");
}

#[test]
fn test_delete_key() {
    let mut app = App::new();
    app.active_field = 1;
    app.form.bids_dir = "abc".to_string();
    app.editing = true;
    app.cursor_pos = 0;
    app.handle_key(key(KeyCode::Delete));
    assert_eq!(app.form.bids_dir, "bc");
}

#[test]
fn test_cursor_left_right() {
    let mut app = App::new();
    app.active_field = 1;
    app.form.bids_dir = "abc".to_string();
    app.editing = true;
    app.cursor_pos = 2;
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.cursor_pos, 1);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.cursor_pos, 2);
}

#[test]
fn test_cursor_left_at_zero() {
    let mut app = App::new();
    app.editing = true;
    app.cursor_pos = 0;
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.cursor_pos, 0);
}

#[test]
fn test_home_end_keys() {
    let mut app = App::new();
    app.active_field = 1;
    app.form.bids_dir = "abcdef".to_string();
    app.editing = true;
    app.cursor_pos = 3;
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.cursor_pos, 0);
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.cursor_pos, 6);
}

#[test]
fn test_esc_exits_editing() {
    let mut app = App::new();
    app.active_field = 1;
    app.editing = true;
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.editing);
    // Should NOT trigger quit while editing
    assert!(!app.should_quit);
}

#[test]
fn test_enter_exits_editing() {
    let mut app = App::new();
    app.active_field = 1;
    app.editing = true;
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.editing);
}

// --- Checkbox fields ---



#[test]
fn test_pipeline_phase_offset_removal() {
    let mut app = App::new();
    assert!(app.field_toggle("phase_offset_removal"));
    app.toggle_field("phase_offset_removal");
    assert!(!app.field_toggle("phase_offset_removal"));
}

// --- F5 triggers run ---

#[test]
fn test_f5_requires_bids_dir() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::F(5)));
    assert!(!app.should_run);
    assert!(app.error_message.is_some());
}

#[test]
fn test_f5_triggers_run_with_bids() {
    let mut app = App::new();
    app.form.bids_dir = "/tmp/bids".to_string();
    app.handle_key(key(KeyCode::F(5)));
    assert!(app.should_run);
    assert!(app.error_message.is_none());
}

#[test]
fn test_f5_requires_slurm_account() {
    let mut app = App::new();
    app.form.bids_dir = "/tmp/bids".to_string();
    app.form.execution_mode = 1; // SLURM
    app.handle_key(key(KeyCode::F(5)));
    assert!(!app.should_run);
    assert!(app.error_message.as_deref() == Some("SLURM account is required"));
    assert_eq!(app.active_tab, TAB_EXECUTION); // navigated to Execution tab
}

// --- Pipeline state selects ---

#[test]
fn test_pipeline_algorithm_selects() {
    let mut app = App::new();
    assert_eq!(QSM_ALGO_OPTIONS[app.qsm_algorithm()], "ilsqr"); // the default inversion
    app.set_field_select("qsm_algorithm", 1);
    assert_eq!(app.qsm_algorithm(), 1); // tv
    app.set_field_select("qsm_algorithm", 3);
    assert_eq!(app.qsm_algorithm(), 3); // tgv
}

// --- Text value accessors for different tabs ---

#[test]
fn test_pipeline_get_param() {
    let mut app = App::new();
    *app.field_text_mut("bet_smoothness").unwrap() = "0.2".to_string();
    assert_eq!(app.field_text("bet_smoothness"), "0.2");
}


#[test]
fn test_text_value_unknown_returns_empty() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION; // Algorithms tab - no text fields
    focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
    assert_eq!(app.text_value(), "");
}

// --- get_ accessors (used by UI rendering) ---

/// Every row on every page names a field the accessors resolve, with the kind it edits.
#[test]
fn rows_name_real_fields() {
    let mut app = App::new();
    app.form.settings.set("pipeline.do_swi", true);
    app.form.settings.set("pipeline.do_smwi", true);
    app.form.settings.set("pipeline.do_segmentation", true);
    app.form.export_dicom = true;
    app.form.settings.set("pipeline.do_analysis", true);
    app.form.settings.set("pipeline.do_r2primemap", true);
    app.form.settings.set("motion_correction.enabled", true);
    app.filter_state.orientation_group = "acq".into();
    for mode in [0, 1] {
        app.form.execution_mode = mode;
        for page in Page::ALL {
            for r in app.page_rows(page) {
                match r {
                    PipelineRow::Param { field, .. } => assert!(app.field_text_mut(field).is_some(), "{field}"),
                    PipelineRow::AlgoSelect { field, options, .. } => assert!(app.field_select(field) < options.len(), "{field}"),
                    _ => {}
                }
            }
        }
    }
    // Unknown names resolve to nothing in the run form.
    assert!(app.form.text("nope").is_none() && app.form.flag("nope").is_none() && app.form.choice("nope").is_none());
}

// --- checkbox_value (private, exercised for coverage) ---


// --- RunForm default ---

#[test]
fn test_run_form_default() {
    let form = RunForm::default();
    assert!(form.bids_dir.is_empty());
    assert!(form.output_dir.is_empty());
    assert!(!form.settings.toggle("pipeline.do_swi"));
}


// --- Editing output_dir ---

#[test]
fn test_edit_output_dir() {
    let mut app = App::new();
    app.active_field = 2; // output_dir (field 0=mode, 1=bids_dir, 2=output_dir)
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('o')));
    app.handle_key(key(KeyCode::Char('u')));
    app.handle_key(key(KeyCode::Char('t')));
    assert_eq!(app.form.output_dir, "/out");
}

// --- Editing config_file ---

#[test]
fn test_edit_config_file() {
    let mut app = App::new();
    app.active_field = 3; // config_file (field 0=mode, 1=bids_dir, 2=output_dir, 3=config)
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Char('c')));
    assert_eq!(app.form.config_file, "c");
}

// --- Editing all parameter text fields ---

#[test]
fn test_pipeline_param_mutation() {
    let mut app = App::new();
    assert!(!app.field_text("bet_smoothness").is_empty()); // has QSM.rs default
    if let Some(s) = app.field_text_mut("bet_smoothness") {
        *s = "0.25".to_string();
    }
    assert_eq!(app.field_text("bet_smoothness"), "0.25");

    // Test select (qsm_algorithm)
    app.set_field_select("qsm_algorithm", 2);
    assert_eq!(app.field_select("qsm_algorithm"), 2);
}

#[test]
fn test_pipeline_visible_rows_change_with_algorithm() {
    let mut app = App::new();
    app.set_qsm_algorithm(0); // RTS
    let rows_rts = app.pipeline_rows().len();
    app.set_qsm_algorithm(2); // TKD (fewer params)
    let rows_tkd = app.pipeline_rows().len();
    assert!(rows_tkd < rows_rts, "TKD should have fewer rows than RTS");

    app.set_qsm_algorithm(4); // TGV (hides background removal)
    let rows_tgv = app.pipeline_rows().len();
    assert!(rows_tgv < rows_rts, "TGV should hide background removal");
}

/// `[` and `]` walk the sub-tabs of the active tab, wrapping, and put focus back on top.
#[test]
fn brackets_switch_sub_tabs_and_reset_focus() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.sync_pipeline_mode();
    // Masking, the second section, is the one Reconstruction opens on.
    assert_eq!(app.pages(TAB_RECONSTRUCTION)[..3], [Page::Geometry, Page::Masking, Page::Motion]);
    assert_eq!(app.page(), Some(Page::Masking));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.focus(), 1);
    app.handle_key(key(KeyCode::Char(']')));
    assert_eq!(app.page(), Some(Page::Motion));
    assert_eq!(app.focus(), 0, "a sub-tab switch starts at the top");
    app.handle_key(key(KeyCode::Char('[')));
    app.handle_key(key(KeyCode::Char('[')));
    assert_eq!(app.page(), Some(Page::Geometry));
    app.handle_key(key(KeyCode::Char('[')));
    assert_eq!(app.page(), Some(Page::Export), "wraps");
    app.handle_key(key(KeyCode::Char(']')));
    assert_eq!(app.page(), Some(Page::Geometry));
    // The section is remembered per tab; a main-tab switch resets focus too.
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Char('4')));
    assert_eq!(app.active_tab, TAB_EXECUTION);
    assert_eq!(app.focus(), 0);
    app.handle_key(key(KeyCode::Char('3')));
    assert_eq!(app.page(), Some(Page::Geometry));
}

/// Every QSM section stays in the sidebar; one whose stage the settings skip is marked not
/// applicable and says why, and with QSM off each shows the switch alone.
#[test]
fn qsm_sections_stay_and_say_whether_their_stage_runs() {
    let mut app = App::new();
    let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name).unwrap();
    assert_eq!(app.pages(TAB_RECONSTRUCTION), Page::RECONSTRUCTION.to_vec());
    let state = |app: &App, p| app.section_state(p);
    assert_eq!(state(&app, Page::FieldMap), SectionState::On);
    assert_eq!(state(&app, Page::Background), SectionState::On);
    app.set_qsm_algorithm(idx("iqsm"));
    assert_eq!(app.pages(TAB_RECONSTRUCTION), Page::RECONSTRUCTION.to_vec(), "the list keeps its shape");
    assert_eq!(state(&app, Page::FieldMap), SectionState::NotApplicable);
    assert_eq!(state(&app, Page::Background), SectionState::NotApplicable);
    let rows = |app: &App, p| app.page_rows(p);
    assert!(rows(&app, Page::Background).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("No background removal"))));
    assert!(!rows(&app, Page::Background).iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "bf_algorithm", .. })));
    assert!(rows(&app, Page::FieldMap).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("No field mapping"))));
    // Without a group, COSMOS/STI says where groups are chosen; with one, it has its settings.
    assert_eq!(state(&app, Page::MultiOrientation), SectionState::NotApplicable);
    assert!(!rows(&app, Page::MultiOrientation).iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "orientation_algorithm", .. })));
    assert!(rows(&app, Page::MultiOrientation).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("1 Input → Orientations"))));
    app.filter_state.orientation_group = "acq".into();
    assert_eq!(state(&app, Page::MultiOrientation), SectionState::On);
    assert!(rows(&app, Page::MultiOrientation).iter().any(|r| matches!(r, PipelineRow::AlgoSelect { field: "orientation_algorithm", .. })));
    // QSM's switch is a section of its own, and nowhere else.
    assert_eq!(state(&app, Page::ComputeQsm), SectionState::On);
    for p in Page::RECONSTRUCTION {
        let has = app.page_rows(p).iter().any(|r| row_field(r) == Some("do_qsm"));
        assert_eq!(has, p == Page::ComputeQsm, "{p:?}");
    }
    // QSM off: every QSM stage is off and says where QSM is turned on; its section says
    // what off means.
    app.form.settings.set("pipeline.do_qsm", false);
    assert_eq!(state(&app, Page::ComputeQsm), SectionState::Off);
    for p in [Page::FieldMap, Page::Background, Page::Inversion, Page::MultiOrientation, Page::Reference] {
        assert_eq!(state(&app, p), SectionState::Off, "{p:?}");
        assert_eq!(rows(&app, p), vec![PipelineRow::Note { text: QSM_OFF_NOTE.into() }], "{p:?}");
    }
    assert!(rows(&app, Page::ComputeQsm).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("Off: no field map"))));
    // A forced QSM keeps the stages, and its section says why.
    app.form.settings.set("pipeline.do_smwi", true);
    assert_eq!(state(&app, Page::Inversion), SectionState::On);
    assert_eq!(state(&app, Page::ComputeQsm), SectionState::On);
    assert!(rows(&app, Page::ComputeQsm).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("QSM → SMWI"))));
}

/// The QSM group holds QSM's stages and the outputs made from χ (χ-separation, SMWI); SWI,
/// from the phase alone, is an other output.
#[test]
fn the_qsm_group_holds_what_needs_qsm() {
    let group = |g: &str| Page::RECONSTRUCTION.iter().copied().filter(|p| p.group() == g).collect::<Vec<_>>();
    assert_eq!(group("QSM"), [Page::ComputeQsm, Page::FieldMap, Page::Background, Page::Inversion,
        Page::MultiOrientation, Page::Reference, Page::Separation, Page::Smwi]);
    assert_eq!(group("Other outputs"), [Page::Magnitude, Page::Swi, Page::Relaxometry, Page::Segmentation]);
    let app = App::new();
    let fields = |p| app.page_rows(p).iter().filter_map(row_field).collect::<Vec<_>>();
    assert!(fields(Page::Swi).contains(&"do_swi") && !fields(Page::Swi).contains(&"do_smwi"));
    assert!(fields(Page::Smwi).contains(&"do_smwi") && !fields(Page::Smwi).contains(&"do_swi"));
    assert_eq!(app.section_state(Page::Smwi), SectionState::Off);
    assert_eq!(app.section_state(Page::Swi), SectionState::Off);
}

// ========== word_boundary_left tests ==========

#[test]
fn test_word_boundary_left() {
    assert_eq!(word_boundary_left("hello world", 11), 6);
    assert_eq!(word_boundary_left("hello world", 5), 0);
    assert_eq!(word_boundary_left("a/b/c", 5), 4);
    assert_eq!(word_boundary_left("", 0), 0);
    assert_eq!(word_boundary_left("abc", 0), 0);
}

// ========== Error message clears on non-F5 key ==========

#[test]
fn test_error_clears_on_keypress() {
    let mut app = App::new();
    app.error_message = Some("test error".to_string());
    app.handle_key(key(KeyCode::Down));
    assert!(app.error_message.is_none());
}

#[test]
fn test_error_does_not_clear_on_f5() {
    let mut app = App::new();
    // F5 with no bids_dir sets an error
    app.handle_key(key(KeyCode::F(5)));
    assert!(app.error_message.is_some());
}
