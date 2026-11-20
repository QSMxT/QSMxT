//! The Reconstruction tab's rows: algorithms, parameters, resets, Basic and Advanced mode.

use super::*;

// ========== Pipeline tab handler tests ==========

#[test]
fn test_pipeline_tab_navigate_down_up() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    assert_eq!(app.pipeline_state.focus, 0);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.pipeline_state.focus, 1);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.pipeline_state.focus, 2);
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.pipeline_state.focus, 1);
    app.handle_key(key(KeyCode::Char('k')));
    assert_eq!(app.pipeline_state.focus, 0);
    // Up at 0 stays at 0
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.pipeline_state.focus, 0);
}

#[test]
fn test_pipeline_tab_navigate_clamp_bottom() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.sync_pipeline_mode();
    let max = app.focusable_rows().len().saturating_sub(1);
    app.pipeline_state.focus = max;
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.pipeline_state.focus, max);
}

#[test]
fn test_pipeline_tab_switch_from_pipeline() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, TAB_EXECUTION);
    app.active_tab = TAB_RECONSTRUCTION;
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(app.active_tab, TAB_PIPELINE);
}

#[test]
fn test_pipeline_tab_number_switch() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.handle_key(key(KeyCode::Char('3')));
    assert_eq!(app.active_tab, 2);
}

#[test]
fn test_switch_to_separation_resets_focus_to_first_row() {
    let mut app = App::new();
    // Sit deep in the Field map section, past the number of rows χ-separation will have.
    app.active_tab = TAB_RECONSTRUCTION;
    app.sub_page[TAB_RECONSTRUCTION] = Some(Page::FieldMap);
    app.sync_pipeline_mode();
    let qsm_max = app.focusable_rows().len().saturating_sub(1);
    app.pipeline_state.focus = qsm_max;
    // Switch to χ-separation; the page changes, so focus must reset to the first row.
    for _ in 0..5 { app.handle_key(key(KeyCode::Char(']'))); }
    assert_eq!(app.pipeline_state.page, Page::Separation);
    assert_eq!(app.pipeline_state.focus, 0);
    // And it must be in range for the Separation row set.
    let sep_len = app.focusable_rows().len();
    assert!(app.pipeline_state.focus < sep_len.max(1));
}

#[test]
fn test_pipeline_left_right_algo_select() {
    let mut app = App::new();
    assert!(app.goto_field("inversion.algorithm"));
    // Find the focus index for the QSM Inversion AlgoSelect
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut qsm_inv_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::AlgoSelect { field: "inversion.algorithm", .. } = &rows[ri] {
            qsm_inv_focus = Some(fi);
            break;
        }
    }
    let fi = qsm_inv_focus.expect("qsm_algorithm row not found");
    app.pipeline_state.focus = fi;
    let ilsqr = QSM_ALGO_OPTIONS.iter().position(|o| *o == "ilsqr").unwrap();
    assert_eq!(app.qsm_algorithm(), ilsqr); // the default inversion
    step(&mut app, 1);
    assert_eq!(QSM_ALGO_OPTIONS[app.qsm_algorithm()], QSM_ALGO_OPTIONS[ilsqr + 1]);
    step(&mut app, -1);
    assert_eq!(app.qsm_algorithm(), ilsqr);
}

#[test]
fn test_pipeline_enter_opens_the_unwrapping_picker() {
    let mut app = App::new();
    assert!(app.goto_field("field_mapping.unwrapping_algorithm"));
    // Navigate to unwrapping_algorithm AlgoSelect
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut uw_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::AlgoSelect { field: "field_mapping.unwrapping_algorithm", .. } = &rows[ri] {
            uw_focus = Some(fi);
            break;
        }
    }
    let fi = uw_focus.expect("unwrapping_algorithm row not found");
    app.pipeline_state.focus = fi;
    assert_eq!(app.field_select("field_mapping.unwrapping_algorithm"), 0); // romeo

    // Enter opens the selection modal for this field (does not cycle).
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(
        app.algo_modal.as_ref().unwrap().target,
        AlgoModalTarget::Select(ref f) if f == "field_mapping.unwrapping_algorithm"
    ));
    assert_eq!(app.field_select("field_mapping.unwrapping_algorithm"), 0); // still unchanged
    // Pick the next option (laplacian) and confirm.
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.field_select("field_mapping.unwrapping_algorithm"), 1); // laplacian

    // Focus is restored to the selector after confirming: → changes nothing, Enter reopens it.
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.field_select("field_mapping.unwrapping_algorithm"), 1);
    assert!(app.algo_modal.is_none());
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(
        app.algo_modal.as_ref().unwrap().target,
        AlgoModalTarget::Select(ref f) if f == "field_mapping.unwrapping_algorithm"
    ));
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.field_select("field_mapping.unwrapping_algorithm"), 0);
}

#[test]
fn test_pipeline_edit_text_param() {
    let mut app = App::new();
    app.form.settings.set("pipeline.axial_resampling", true);
    assert!(app.goto_field("pipeline.obliquity_threshold"));
    // Find the obliquity_threshold Param row
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut obl_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::Param { field: "pipeline.obliquity_threshold", .. } = &rows[ri] {
            obl_focus = Some(fi);
            break;
        }
    }
    let fi = obl_focus.expect("obliquity_threshold row not found");
    app.pipeline_state.focus = fi;
    // Enter editing
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.editing);
    // Type some chars
    app.handle_key(key(KeyCode::Home));
    app.handle_key(key(KeyCode::End));
    // Backspace to clear
    for _ in 0..app.field_text("pipeline.obliquity_threshold").len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    app.handle_key(key(KeyCode::Char('5')));
    assert_eq!(app.field_text("pipeline.obliquity_threshold"), "5");
    // Escape exits editing
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.pipeline_state.editing);
}

#[test]
fn test_pipeline_toggle_do_qsm() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.sub_page[TAB_RECONSTRUCTION] = Some(Page::ComputeQsm);
    app.sync_pipeline_mode();
    // Focus 0 should be the do_qsm toggle
    app.pipeline_state.focus = 0;
    assert!(app.form.settings.toggle("pipeline.do_qsm"));
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.form.settings.toggle("pipeline.do_qsm"));
    // Toggling back
    app.handle_key(key(KeyCode::Char(' ')));
    assert!(app.form.settings.toggle("pipeline.do_qsm"));
}

#[test]
fn test_pipeline_toggle_inhomogeneity_correction() {
    let mut app = App::new();
    assert!(app.goto_field("masking.inhomogeneity_correction"));
    // Find inhomogeneity_correction toggle
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut ic_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::Toggle { field: "masking.inhomogeneity_correction", .. } = &rows[ri] {
            ic_focus = Some(fi);
            break;
        }
    }
    let fi = ic_focus.expect("inhomogeneity_correction not found");
    app.pipeline_state.focus = fi;
    assert!(app.field_toggle("masking.inhomogeneity_correction"));
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.field_toggle("masking.inhomogeneity_correction"));
}

#[test]
fn test_pipeline_quit_from_pipeline_tab() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_pipeline_esc_from_pipeline_tab() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.handle_key(key(KeyCode::Esc));
    assert!(app.should_quit);
}

#[test]
fn test_pipeline_f5_from_pipeline_tab() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.form.bids_dir = "/tmp/bids".to_string();
    app.handle_key(key(KeyCode::F(5)));
    assert!(app.should_run);
}

#[test]
fn test_pipeline_reset_field() {
    let mut app = App::new();
    assert!(app.goto_field("inversion.algorithm"));
    app.set_qsm_algorithm(5);
    // Find qsm_algorithm focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut fi_found = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::AlgoSelect { field: "inversion.algorithm", .. } = &rows[ri] {
            fi_found = Some(fi);
            break;
        }
    }
    let fi = fi_found.expect("qsm_algorithm not found");
    app.pipeline_state.focus = fi;
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(QSM_ALGO_OPTIONS[app.qsm_algorithm()], "ilsqr"); // default
}

#[test]
fn test_pipeline_reset_tab() {
    let mut app = App::new();
    app.set_qsm_algorithm(5);
    app.set_field_select("bg_removal.algorithm", 3);
    *app.field_text_mut("inversion.tikhonov.lambda").unwrap() = "9".into();
    assert!(app.goto_field("inversion.algorithm"));
    app.handle_key(key(KeyCode::Char('R')));
    // The page's settings, the ones a reset reveals among them, and nothing on other pages.
    assert_eq!(QSM_ALGO_OPTIONS[app.qsm_algorithm()], "ilsqr");
    assert_eq!(app.field_text("inversion.tikhonov.lambda"), App::new().field_text("inversion.tikhonov.lambda"));
    assert_eq!(app.field_select("bg_removal.algorithm"), 3, "Background is another page");
}

#[test]
fn test_pipeline_visible_rows_change_on_toggle() {
    let mut app = App::new();
    let rows_with_qsm = app.pipeline_rows().len();
    app.form.settings.set("pipeline.do_qsm", false);
    let rows_without_qsm = app.pipeline_rows().len();
    assert!(rows_without_qsm < rows_with_qsm);
}

#[test]
fn test_pipeline_editing_left_right_cursor() {
    let mut app = App::new();
    app.form.settings.set("pipeline.axial_resampling", true);
    assert!(app.goto_field("pipeline.obliquity_threshold"));
    // Find obliquity_threshold param and edit it
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut fi = 0;
    for (f, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::Param { field: "pipeline.obliquity_threshold", .. } = &rows[ri] {
            fi = f;
            break;
        }
    }
    app.pipeline_state.focus = fi;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.editing);
    // Move cursor left and right
    let len = app.field_text("pipeline.obliquity_threshold").len();
    assert_eq!(app.pipeline_state.cursor, len);
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.pipeline_state.cursor, len - 1);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.pipeline_state.cursor, len);
    // Enter exits editing
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.pipeline_state.editing);
}

// ========== Pipeline reset_pipeline_field for Param and Toggle ==========

#[test]
fn test_reset_pipeline_param_field() {
    let mut app = App::new();
    app.form.settings.set("pipeline.axial_resampling", true);
    assert!(app.goto_field("pipeline.obliquity_threshold"));
    // Find obliquity_threshold param
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut fi = 0;
    for (f, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::Param { field: "pipeline.obliquity_threshold", .. } = &rows[ri] {
            fi = f;
            break;
        }
    }
    app.pipeline_state.focus = fi;
    // Modify the value
    *app.field_text_mut("pipeline.obliquity_threshold").unwrap() = "999".to_string();
    // Reset
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.field_text("pipeline.obliquity_threshold"), App::new().field_text("pipeline.obliquity_threshold"));
}

#[test]
fn test_reset_pipeline_toggle_field() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.sub_page[TAB_RECONSTRUCTION] = Some(Page::ComputeQsm);
    app.sync_pipeline_mode();
    // Focus 0 is do_qsm toggle
    app.pipeline_state.focus = 0;
    app.form.settings.set("pipeline.do_qsm", false);
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.form.settings.toggle("pipeline.do_qsm")); // default is true
}

// ========== Pipeline: switch algorithms and verify row changes ==========

/// The tests name the background-removal select's indices by [`BF_OPTIONS`]: it lists the
/// setting's options in the select's order.
#[test]
fn bf_options_are_the_selects() {
    let options: Vec<_> = crate::settings_rows::setting("bg_removal.algorithm").value_type.options()
        .iter().map(|o| o.value.clone()).collect();
    assert_eq!(options, BF_OPTIONS.iter().map(|o| serde_json::json!(o)).collect::<Vec<_>>());
}

#[test]
fn test_pipeline_bf_algorithm_params() {
    let mut app = App::new();
    // Switch through every BG removal algorithm (incl. BFRnet/iQFM) and verify rows change.
    for algo in 0..BF_OPTIONS.len() {
        app.set_field_select("bg_removal.algorithm", algo);
        let rows = app.page_rows(Page::Background);
        assert!(!rows.is_empty());
    }
}

#[test]
fn test_pipeline_qsm_algorithm_all_params() {
    let mut app = App::new();
    // Switch through EVERY QSM algorithm (incl. the deep-learning ones: single-step
    // NeXtQSM/AutoQSM hide BG removal; end-to-end iQSM/iQSM+ hide field mapping too).
    for (algo, name) in QSM_ALGO_OPTIONS.iter().enumerate() {
        app.set_qsm_algorithm(algo);
        let rows = app.page_rows(Page::Inversion);
        assert!(!focusable_indices(&rows).is_empty(), "{name}");
    }
}

#[test]
fn test_pipeline_separation_algorithm_all_params() {
    let mut app = App::new();
    app.form.settings.set("pipeline.do_chi_separation", true);
    // Every separation method incl. SUSEP-Net / χ-sepnet.
    for algo in 0..super::super::super::settings_rows::options("separation.algorithm").0.len() {
        app.set_field_select("separation.algorithm", algo);
        assert!(app.page_rows(Page::Separation).len() > 3);
    }
}

#[test]
fn test_pipeline_medi_smv_shows_bg_removal() {
    let mut app = App::new();
    app.set_qsm_algorithm(7);
    app.form.settings.set("inversion.medi.smv", true);
    // MEDI takes the local field with every SMV option, so background removal runs.
    assert!(app.page_notes(Page::Background, &app.pipeline_state.ctx).is_none(),
            "MEDI+SMV takes the local field from background removal");
}

// ========== Pipeline threshold value editing ==========

#[test]
fn test_pipeline_threshold_value_editing() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    // Set generator to Fixed threshold to make MaskOpThresholdValue visible
    app.mask_mut().sections[0].generator = qsmxt_engine::MaskOp::Threshold {
        method: qsmxt_engine::MaskThresholdMethod::Fixed,
        value: Some(0.5),
    };
    app.mark_mask_custom(0);

    // Find MaskOpThresholdValue focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut tv_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if matches!(&rows[ri], PipelineRow::MaskOpThresholdValue { .. }) {
            tv_focus = Some(fi);
            break;
        }
    }
    let fi = tv_focus.expect("MaskOpThresholdValue not found");
    app.pipeline_state.focus = fi;
    // Enter starts editing
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.mask_value_editing);
    // Type a new value -- clear via backspace from end
    let buf_len = app.pipeline_state.mask_value_buf.len();
    for _ in 0..buf_len {
        app.handle_key(key(KeyCode::Backspace));
    }
    app.handle_key(key(KeyCode::Char('0')));
    app.handle_key(key(KeyCode::Char('.')));
    app.handle_key(key(KeyCode::Char('7')));
    // Cursor movement
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Right));
    // Enter saves
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.pipeline_state.mask_value_editing);
    if let qsmxt_engine::MaskOp::Threshold { value, .. } = &app.mask().sections[0].generator {
        assert!((value.unwrap() - 0.7).abs() < 0.001);
    }
}

/// The reliable mask's sections are addressed from `MASK_TWO_PASS_BASE`, so starting an edit
/// there must not index the main mask's sections (it used to panic).
#[test]
fn test_reliable_mask_threshold_value_editing() {
    use qsmxt_engine::{MaskOp, MaskThresholdMethod};
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.form.settings.set("masking.two_pass", true);
    app.mask_mut().two_pass_sections[0].generator =
        MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.5) };
    app.mark_mask_custom(MASK_TWO_PASS_BASE);
    let main_before = app.mask().sections.clone();

    let rows = app.rows();
    let focus = app.focusable_rows().iter()
        .position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpThresholdValue { section } if is_two_pass_section(*section)))
        .expect("reliable threshold value row not focusable");
    app.pipeline_state.focus = focus;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.mask_value_editing);
    assert_eq!(app.pipeline_state.mask_value_buf, "0.5");

    for _ in 0..app.pipeline_state.mask_value_buf.len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "0.7".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.pipeline_state.mask_value_editing);
    let MaskOp::Threshold { value, .. } = &app.mask().two_pass_sections[0].generator else {
        panic!("the reliable mask's generator should still be a threshold");
    };
    assert!((value.unwrap() - 0.7).abs() < 0.001);
    assert_eq!(app.mask().sections, main_before, "the main mask must be untouched");
}

#[test]
fn test_pipeline_bet_voxel_scale_row() {
    use qsmxt_engine::MaskOp;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
    app.mark_mask_custom(0);

    let find_scale_row = |app: &App| {
        let rows = app.rows();
        let focusable = app.focusable_rows();
        focusable.iter().position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpBetVoxelScale { .. }))
    };
    let fi = find_scale_row(&app).expect("BET voxel-scale row should be shown and focusable");
    app.pipeline_state.focus = fi;

    let scale_of = |app: &App| match &app.mask().sections[0].generator {
        MaskOp::Bet { voxel_scale, .. } => *voxel_scale,
        other => panic!("expected bet, got {other:?}"),
    };
    assert_eq!(scale_of(&app), qsmxt_engine::bet_default_voxel_scale());

    // Enter lists the scales with the current one highlighted; the mouse setting is the last.
    app.handle_key(key(KeyCode::Enter));
    let m = app.algo_modal.as_ref().expect("the scale list");
    assert_eq!((m.options.clone(), m.cursor), (vec!["1x".into(), "2x".into(), "5x".into(), "10x".into()], 0));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(scale_of(&app), 1.0, "Esc changes nothing");
    step(&mut app, -1);
    assert_eq!(scale_of(&app), 10.0);
    // A scale from a loaded config is listed too, in order.
    app.set_bet_voxel_scale(0, 3.0);
    app.handle_key(key(KeyCode::Enter));
    let m = app.algo_modal.take().unwrap();
    assert_eq!((m.options[2].as_str(), m.cursor), ("3x", 2));

    // The row belongs to BET only.
    app.mask_mut().sections[0].generator = MaskOp::hd_bet_default();
    assert!(find_scale_row(&app).is_none(), "scale row should not show for HD-BET");
}

/// Every BET setting sits in one block under the BET step: the step's own fractional
/// intensity and voxel scale, then the run-wide surface settings.
#[test]
fn bet_settings_are_listed_together_under_the_bet_step() {
    use qsmxt_engine::MaskOp;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
    app.mark_mask_custom(0);
    let rows = app.rows();
    let start = rows.iter().position(|r| matches!(r, PipelineRow::MaskOpGenerator { section: 0 }))
        .expect("generator row");
    let block: Vec<String> = rows[start..start + 8].iter().map(|r| match r {
        PipelineRow::MaskOpGenerator { .. } => "generator".to_string(),
        PipelineRow::MaskOpGeneratorParam { .. } => "frac".to_string(),
        PipelineRow::MaskOpBetVoxelScale { .. } => "scale".to_string(),
        PipelineRow::Param { field, .. } | PipelineRow::AlgoSelect { field, .. } => field.to_string(),
        other => format!("{other:?}"),
    }).collect();
    assert_eq!(block, ["generator", "frac", "scale", "bet.smoothness", "bet.gradient_threshold",
        "bet.iterations", "bet.subdivisions", "bet.orientation"]);
    // Listed once, however many BET steps share them.
    let count = |app: &App| app.rows().iter()
        .filter(|r| matches!(r, PipelineRow::Param { field: "bet.smoothness", .. })).count();
    assert_eq!(count(&app), 1);
    let bet_and_phase = MASK_PRESET_OPTIONS.iter().position(|o| *o == "bet-and-phase").unwrap();
    app.apply_mask_preset(bet_and_phase);
    assert!(app.mask().sections.len() > 1);
    assert_eq!(count(&app), 1);
    // Not shown without BET.
    app.mask_mut().sections = vec![qsmxt_engine::MaskSection {
        input: qsmxt_engine::MaskingInput::Magnitude,
        generator: MaskOp::hd_bet_default(),
        refinements: vec![],
    }];
    app.mark_mask_custom(0);
    assert_eq!(count(&app), 0);
}

#[test]
fn test_pipeline_hd_bet_step_row() {
    use qsmxt_engine::MaskOp;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.mask_mut().sections[0].generator = MaskOp::hd_bet_default();
    app.mark_mask_custom(0);
    let rows = app.rows();
    assert!(!rows.iter().any(|r| matches!(r, PipelineRow::MaskOpHdBetStep { .. })),
            "whole-volume HD-BET has no window step");
    app.mask_mut().sections[0].generator =
        MaskOp::HdBet { patch: Some(MaskOp::hd_bet_default_patch()), tta: false,
                        tile_step: qsmxt_engine::hd_bet_default_tile_step() };

    let find_step_row = |app: &App| {
        let rows = app.rows();
        let focusable = app.focusable_rows();
        focusable.iter().position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpHdBetStep { .. }))
    };
    let fi = find_step_row(&app).expect("HD-BET step row should be shown and focusable");
    app.pipeline_state.focus = fi;

    let step_of = |app: &App| match &app.mask().sections[0].generator {
        MaskOp::HdBet { tile_step, .. } => *tile_step,
        other => panic!("expected hd-bet, got {other:?}"),
    };
    assert_eq!(step_of(&app), qsmxt_engine::hd_bet_default_tile_step());

    // Chosen from a list: a coarser stride, then back.
    app.handle_key(key(KeyCode::Right));
    assert_eq!(step_of(&app), 0.5, "→ changes nothing");
    step(&mut app, 1);
    assert_eq!(step_of(&app), 0.625);
    step(&mut app, -1);
    assert_eq!(step_of(&app), 0.5);

    // The row belongs to HD-BET only.
    app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
    assert!(find_step_row(&app).is_none(), "step row should not show for BET");
}

#[test]
fn test_pipeline_threshold_value_esc_cancels() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.mask_mut().sections[0].generator = qsmxt_engine::MaskOp::Threshold {
        method: qsmxt_engine::MaskThresholdMethod::Fixed,
        value: Some(0.5),
    };
    app.mark_mask_custom(0);
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut tv_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if matches!(&rows[ri], PipelineRow::MaskOpThresholdValue { .. }) {
            tv_focus = Some(fi);
            break;
        }
    }
    let fi = tv_focus.expect("MaskOpThresholdValue not found");
    app.pipeline_state.focus = fi;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.mask_value_editing);
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.pipeline_state.mask_value_editing);
    // Value should be unchanged
    if let qsmxt_engine::MaskOp::Threshold { value, .. } = &app.mask().sections[0].generator {
        assert!((value.unwrap() - 0.5).abs() < 0.001);
    }
}

// ========== Basic and Advanced mode ==========

fn basic_medi() -> App {
    let mut app = App::new();
    app.advanced = false;
    let medi = QSM_PICKER_OPTIONS.iter().position(|o| *o == "medi").unwrap();
    app.set_field_select("inversion.algorithm", medi);
    goto_page(&mut app, Page::Inversion);
    app.subtab_focus = false;
    app
}

fn fields(rows: &[PipelineRow]) -> Vec<&'static str> {
    rows.iter().filter_map(row_field).collect()
}

#[test]
fn basic_mode_shows_the_choices_and_counts_the_rest() {
    let app = basic_medi();
    let rows = app.rows();
    assert_eq!(fields(&rows), ["inversion.algorithm", "inversion.medi.smv", "inversion.medi.msmv", "inversion.medi.csf", "inversion.heidi.enabled"]);
    assert_eq!(rows.last(), Some(&PipelineRow::AdvancedFooter { hidden: 15 }));
}

#[test]
fn a_changed_advanced_setting_shows_in_basic_mode() {
    let mut app = basic_medi();
    *app.field_text_mut("inversion.medi.lambda").unwrap() = "2000".into();
    let rows = app.rows();
    assert!(fields(&rows).contains(&"inversion.medi.lambda"));
    assert_eq!(rows.last(), Some(&PipelineRow::AdvancedFooter { hidden: 14 }));
}

#[test]
fn a_shows_every_setting_and_is_remembered() {
    let _s = crate::tips::TestSettings::new();
    let mut app = basic_medi();
    app.handle_key(key(KeyCode::Char('A')));
    assert!(app.advanced && crate::prefs::advanced());
    let rows = app.rows();
    assert!(fields(&rows).contains(&"inversion.medi.lambda"));
    assert!(!rows.iter().any(|r| matches!(r, PipelineRow::AdvancedFooter { .. })));
    app.handle_key(key(KeyCode::Char('A')));
    assert!(!app.advanced && !crate::prefs::advanced());
}

#[test]
fn enter_on_the_footer_shows_every_setting_and_keeps_the_section() {
    let _s = crate::tips::TestSettings::new();
    let mut app = basic_medi();
    let last = app.focusable_rows().len() - 1;
    app.set_focus(last);
    assert!(matches!(app.focused_row(), Some(PipelineRow::AdvancedFooter { .. })));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.advanced);
    assert_eq!(app.page(), Some(Page::Inversion));
}

#[test]
fn a_keeps_focus_on_the_same_setting() {
    let _s = crate::tips::TestSettings::new();
    let mut app = basic_medi();
    assert!(app.focus_field("inversion.medi.csf"));
    app.handle_key(key(KeyCode::Char('A')));
    assert_eq!(app.focused_row().as_ref().and_then(row_field), Some("inversion.medi.csf"));
}

#[test]
fn basic_mode_shows_the_mask_recipe_as_a_line_until_it_is_edited() {
    let mut app = App::new();
    app.advanced = false;
    goto_page(&mut app, Page::Masking);
    let rows = app.rows();
    let summary: Vec<&str> = rows.iter().filter_map(|r| match r { PipelineRow::Summary { text } => Some(text.as_str()), _ => None }).collect();
    assert_eq!(summary.join(" "), "phase-quality › otsu threshold › dilate 1 › fill-holes › erode 1");
    assert!(!rows.iter().any(|r| matches!(r, PipelineRow::MaskOpEntry { .. } | PipelineRow::MaskOpAddSection { .. })));
    app.pipeline_state.mask_preset = *MASK_PRESET_CUSTOM;
    assert!(app.rows().iter().any(|r| matches!(r, PipelineRow::MaskOpEntry { .. })));
}

#[test]
fn the_mask_recipe_line_fits_beside_the_sidebar_at_80_columns() {
    let mut app = App::new();
    for (preset, name) in MASK_PRESET_OPTIONS.iter().enumerate().take(*MASK_PRESET_CUSTOM) {
        app.set_field_select("mask_preset", preset);
        for line in app.mask().mask_recipe_summary() {
            assert!(line.chars().count() <= 46, "{name}: {line}");
        }
    }
}
