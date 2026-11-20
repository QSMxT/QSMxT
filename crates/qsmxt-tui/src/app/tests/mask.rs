//! The mask editor: presets, steps, sections and the two-pass reliable mask.

use super::*;

// ========== Pipeline mask operations ==========

#[test]
fn test_pipeline_mask_preset_switch() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    // Find mask_preset focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut mp_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::AlgoSelect { field: "mask_preset", .. } = &rows[ri] {
            mp_focus = Some(fi);
            break;
        }
    }
    let fi = mp_focus.expect("mask_preset not found");
    app.pipeline_state.focus = fi;
    assert_eq!(app.pipeline_state.mask_preset, 0); // robust threshold
    step(&mut app, 1);
    assert_eq!(app.pipeline_state.mask_preset, 1); // BET
    step(&mut app, 1);
    assert_eq!(app.pipeline_state.mask_preset, 2); // HD-BET + signal-gated erosion
    assert_eq!(app.mask().sections, qsmxt_engine::hd_bet_mask_sections());
    step(&mut app, 1);
    assert_eq!(app.pipeline_state.mask_preset, 3); // BET AND phase quality
    let recipe = qsmxt_engine::bet_and_phase_mask_recipe();
    assert_eq!(app.mask().sections, recipe.sections);
    assert_eq!(app.mask().combine, recipe.combine);
    assert_eq!(app.mask().refinements, recipe.refinements);
    step(&mut app, 1);
    assert_eq!(app.pipeline_state.mask_preset, 4); // RS2-Net (rodent)
    assert_eq!(app.mask().sections, qsmxt_engine::rs2_net_mask_sections());
    // Leaving the two-section preset resets the combine mode and the combined-mask steps.
    assert_eq!(app.mask().combine, qsmxt_engine::MaskCombine::Or);
    assert!(app.mask().refinements.is_empty());
    step(&mut app, 1);
    assert_eq!(app.pipeline_state.mask_preset, 5); // MindGrab
    assert_eq!(app.mask().sections, qsmxt_engine::mindgrab_mask_sections());
    step(&mut app, 1);
    assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM, "the picker lists 'custom' last");
}

#[test]
fn test_mask_threshold_method_modal_opens_and_sets() {
    use qsmxt_engine::{MaskOp, MaskThresholdMethod};
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.sync_pipeline_mode();
    // Default preset is a threshold generator, so the param row is the method select.
    assert!(matches!(
        app.mask().sections[0].generator,
        MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }
    ));
    let rows = app.rows();
    let target = app.focusable_rows().iter().position(|&ri| {
        matches!(rows.get(ri), Some(PipelineRow::MaskOpGeneratorParam { .. }))
    }).expect("MaskOpGeneratorParam row not focusable");
    app.pipeline_state.focus = target;
    app.handle_key(key(KeyCode::Enter));
    let modal = app.algo_modal.as_ref().expect("modal should open over threshold Method");
    assert!(matches!(modal.target, AlgoModalTarget::MaskThresholdMethod(_)));
    assert_eq!(modal.options.len(), 4);
    // Pick "percentile" (index 2) and confirm.
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_none());
    assert!(matches!(
        app.mask().sections[0].generator,
        MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. }
    ));
}

#[test]
fn test_mask_bet_frac_intensity_opens_no_modal() {
    use qsmxt_engine::MaskOp;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.sync_pipeline_mode();
    // Switch the generator to BET so the param row is a numeric frac-intensity slider.
    app.mask_mut().sections[0].generator = MaskOp::bet(0.5);
    let rows = app.rows();
    let target = app.focusable_rows().iter().position(|&ri| {
        matches!(rows.get(ri), Some(PipelineRow::MaskOpGeneratorParam { .. }))
    }).expect("MaskOpGeneratorParam row not focusable");
    app.pipeline_state.focus = target;
    app.handle_key(key(KeyCode::Enter));
    // BET frac. intensity is a slider, not a discrete select — no modal.
    assert!(app.algo_modal.is_none());
}

/// The preset row and `mask_presets()` are two views of one list (the CLI's `--mask-preset`
/// values are the third, checked in the binary); `apply_mask_preset` indexes straight into it,
/// so they have to stay aligned.
#[test]
fn test_mask_preset_options_match_the_preset_list() {
    let names: Vec<&str> = qsmxt_engine::mask_presets().iter().map(|(n, _)| *n).collect();
    assert_eq!(&MASK_PRESET_OPTIONS[..*MASK_PRESET_CUSTOM], &names[..]);
    assert_eq!(MASK_PRESET_OPTIONS[*MASK_PRESET_CUSTOM], "custom");
    assert_eq!(MASK_PRESET_OPTIONS.len(), mask_preset_help().len());
}

/// Every signal-erode parameter is reachable and typed (Enter) on the pipeline form,
/// and what the CLI preview says is what the rows show.
#[test]
fn test_pipeline_signal_erode_params_are_editable() {
    use qsmxt_engine::MaskOp;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.apply_mask_preset(2); // hd-bet + signal-erode

    let focus_param = |app: &mut App, param: usize| {
        let rows = app.rows();
        app.pipeline_state.focus = app.focusable_rows().iter()
            .position(|&ri| matches!(rows[ri], PipelineRow::MaskOpSignalErodeParam { param: p, .. } if p == param))
            .unwrap_or_else(|| panic!("param {param} row not focusable"));
    };
    let op = |app: &App| app.mask().sections[0].refinements[0].clone();
    let MaskOp::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } = op(&app)
        else { panic!("preset should end in signal-erode") };

    // The step row itself is the threshold; the four params each have their own row.
    let rows = app.rows();
    app.pipeline_state.focus = app.focusable_rows().iter()
        .position(|&ri| matches!(rows[ri], PipelineRow::MaskOpEntry { .. })).expect("step row");
    type_mask_value(&mut app, "0.85");
    assert!(matches!(op(&app), MaskOp::SignalErode { threshold: t, .. } if t == 0.85 && t != threshold));

    for (param, typed, expect) in [
        (0usize, "7", "depth cap"), (1, "4", "plain erosions"), (2, "9.5", "bias sigma"), (3, "321", "min component"),
    ] {
        focus_param(&mut app, param);
        let before = op(&app);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(op(&app), before, "→ changes nothing");
        type_mask_value(&mut app, typed);
        let MaskOp::SignalErode { depth_cap: d, global_erosions: g, bias_sigma: b, min_component: m, .. } = op(&app)
            else { panic!("still signal-erode") };
        let now = match param { 0 => d.to_string(), 1 => g.to_string(), 2 => b.to_string(), _ => m.to_string() };
        assert_eq!(now, typed, "{expect}");
        // And back.
        let back = match param { 0 => depth_cap.to_string(), 1 => global_erosions.to_string(), 2 => bias_sigma.to_string(), _ => min_component.to_string() };
        type_mask_value(&mut app, &back);
    }

    let MaskOp::SignalErode { depth_cap: d, global_erosions: g, bias_sigma: b, min_component: m, .. } = op(&app)
        else { panic!("still signal-erode") };
    assert_eq!((d, g, b, m), (depth_cap, global_erosions, bias_sigma, min_component), "edits round-trip");

    // A count is a whole number of at least zero: anything else is refused and kept for
    // correcting, and Esc leaves the setting as it was.
    focus_param(&mut app, 0);
    type_mask_value(&mut app, "-1");
    assert!(app.pipeline_state.mask_value_editing && app.error_message.is_some(), "refused, still editing");
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.pipeline_state.mask_value_editing);
    assert!(matches!(op(&app), MaskOp::SignalErode { depth_cap: d, .. } if d == depth_cap));
    type_mask_value(&mut app, "0");
    assert!(matches!(op(&app), MaskOp::SignalErode { depth_cap: 0, .. }));

    // The generated command carries the edited values.
    // Edited away from the preset, so the command spells the recipe out — with the
    // parameters that are still at their default left off.
    let cmd = crate::command::build_command_string(&app);
    assert!(cmd.contains("--mask magnitude,hd-bet,signal-erode:0.85:0"), "cmd: {cmd}");
}

/// The reference has one row (QSM → Reference); a region there turns segmentation on where
/// segmentation is set up.
#[test]
fn the_reference_has_one_row_on_the_qsm_tab() {
    let mut app = App::new();
    let thalamus = qsm_ref_options().iter().position(|o| *o == "thalamus")
        .expect("thalamus should be offered");
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "qsm.reference"));
    let copies = app.all_pages().iter().filter(|(_, _, rows)| rows.iter().any(|r| row_field(r) == Some("qsm.reference"))).count();
    assert_eq!(copies, 1, "one copy only");
    focus_on(&mut app, TAB_RECONSTRUCTION, "qsm.reference");
    app.set_field_select("qsm.reference", thalamus);
    // A region needs segmentation: the run makes one, so its settings show, but the checkbox
    // reads as the user left it until the required change is applied.
    assert!(!app.field_toggle("pipeline.do_segmentation") && app.output_made("pipeline.do_segmentation"));
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "segmentation.version"));
}

#[test]
fn a_region_reference_needs_segmentation_visibly() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    assert!(!app.field_toggle("pipeline.do_segmentation"), "off by default");

    let region = qsm_ref_options().iter().position(|o| *o == "ventricles").unwrap();
    app.set_field_select("qsm.reference", region);
    assert!(!app.field_toggle("pipeline.do_segmentation"), "the checkbox reads as the user left it");
    assert!(app.output_made("pipeline.do_segmentation"), "a region reference needs SynthSeg, which the run makes");
    for f in SYNTHSEG_ROWS {
        assert!(shows(&mut app, TAB_RECONSTRUCTION, f),
                "{f} shapes the parcellation the reference is measured on");
    }

    // Ticking it is the required change; unticking it again is refused, and says why.
    focus_on(&mut app, TAB_RECONSTRUCTION, "pipeline.do_segmentation");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_segmentation"), "{:?}", app.error_message);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_segmentation"), "turning it off is refused");
    let msg = app.error_message.clone().expect("a refusal should explain itself");
    assert!(msg.contains("Reference = ventricles") && !msg.contains("--"), "the refusal names the setting: {msg}");

    // `mean` and `none` need nothing.
    app.form.settings.set("pipeline.do_segmentation", false);
    for idx in [0, 1] {
        app.set_field_select("qsm.reference", idx);
        assert!(!app.output_made("pipeline.do_segmentation"), "option {idx} needs nothing");
    }
}

/// The generated command carries the region as the reference value itself.
#[test]
fn a_region_reference_reaches_the_command() {
    let mut app = App::new();
    let idx = qsm_ref_options().iter().position(|o| *o == "left-thalamus").unwrap();
    app.set_field_select("qsm.reference", idx);
    let cmd = crate::command::build_command_string(&app);
    assert!(cmd.contains("--qsm-reference left-thalamus"), "cmd: {cmd}");

    // The default stays off the command line.
    app.set_field_select("qsm.reference", 0);
    assert!(!crate::command::build_command_string(&app).contains("--qsm-reference"));
}

/// The per-structure statistics force segmentation on, so the checkbox has to read as checked
/// and its settings has to be reachable — otherwise the run segments while the TUI says it will
/// not, and the SynthSeg parameters that shape that segmentation cannot be seen or edited.
#[test]
fn statistics_force_segmentation_visibly() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;

    // Off by default: unchecked, and its settings hidden.
    assert!(!app.field_toggle("pipeline.do_segmentation"));
    for f in SYNTHSEG_ROWS {
        assert!(!shows(&mut app, TAB_RECONSTRUCTION, f), "{f} should be hidden");
    }
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "pipeline.do_analysis"), "the statistics are reachable on their own");

    // A config or CLI run can arrive with the statistics already on; segmentation is then in
    // force, so the block must be visible rather than hiding a setting that is taking effect.
    app.form.settings.set("pipeline.do_analysis", true);
    assert!(!app.field_toggle("pipeline.do_segmentation") && app.output_made("pipeline.do_segmentation"),
            "segmentation is needed: made by the run, but not shown as ticked");
    for f in SYNTHSEG_ROWS {
        assert!(shows(&mut app, TAB_RECONSTRUCTION, f),
                "{f} shapes the segmentation the run makes and must be reachable");
    }

    // Ticking it is the required change; it then cannot be turned off from under them.
    focus_on(&mut app, TAB_RECONSTRUCTION, "pipeline.do_segmentation");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_segmentation"));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_segmentation"), "turning it off is refused");
    let msg = app.error_message.clone().expect("a refusal should explain itself");
    assert!(msg.contains("per-structure statistics"), "unhelpful message: {msg}");

    // With the statistics off again it is an ordinary toggle.
    app.form.settings.set("pipeline.do_analysis", false);
    app.error_message = None;
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.form.settings.toggle("pipeline.do_segmentation"), "{:?}", app.error_message);
}

/// Selecting HEIDI shows both parameter groups (its seed is an LSQR solve); selecting plain
/// LSQR shows only its own. The nested rows appear and disappear with their switches.
#[test]
fn heidi_shows_the_lsqr_group_and_its_own() {
    let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name)
        .unwrap_or_else(|| panic!("{name} missing from QSM_ALGO_OPTIONS"));
    let fields = |app: &App| -> Vec<String> {
        app.page_rows(Page::Inversion).iter().filter_map(|r| match r {
            PipelineRow::Param { field, .. } | PipelineRow::Toggle { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect()
    };
    let with = |alg: &str, heidi: bool| {
        let mut app = App::new();
        app.set_qsm_algorithm(idx(alg));
        app.form.settings.set("inversion.heidi.enabled", heidi);
        app
    };

    let mut app = with("lsqr", false);
    let f = fields(&app);
    assert!(f.contains(&"inversion.lsqr.tol".to_string()), "{f:?}");
    assert!(f.contains(&"inversion.lsqr.fit_global_offset".to_string()), "{f:?}");
    assert!(f.contains(&"inversion.heidi.enabled".to_string()), "the HEIDI checkbox: {f:?}");
    assert!(!f.iter().any(|x| x.starts_with("inversion.heidi.") && x != "inversion.heidi.enabled"),
        "plain LSQR must not show HEIDI's settings: {f:?}");

    // Ticked after another inversion, HEIDI's settings follow.
    assert!(fields(&with("tkd", true)).contains(&"inversion.heidi.cone_threshold".to_string()));
    assert!(!fields(&with("tgv", false)).contains(&"inversion.heidi.enabled".to_string()), "not offered without a local field");

    // LSQR+HEIDI: LSQR with the checkbox ticked.
    app.form.settings.set("inversion.heidi.enabled", true);
    let f = fields(&app);
    assert!(f.contains(&"inversion.lsqr.tol".to_string()), "HEIDI seeds with LSQR: {f:?}");
    assert!(f.contains(&"inversion.heidi.cone_threshold".to_string()), "{f:?}");
    assert!(f.contains(&"inversion.heidi.continuation_steps".to_string()), "{f:?}");

    // The sub-parameters follow their switches.
    assert!(f.contains(&"inversion.heidi.denoise_iterations".to_string()), "denoise is on by default: {f:?}");
    assert!(f.contains(&"inversion.heidi.laplacian_threshold".to_string()), "{f:?}");
    app.toggle_field("inversion.heidi.denoise");
    app.toggle_field("inversion.heidi.apply_laplacian_correction");
    let f = fields(&app);
    assert!(!f.contains(&"inversion.heidi.denoise_iterations".to_string()), "{f:?}");
    assert!(!f.contains(&"inversion.heidi.laplacian_threshold".to_string()), "{f:?}");
}

/// Inserting an algorithm mid-list used to shift the hardcoded indices that decide which
/// stages are shown. They are compared by name now; this pins the behaviour for the algorithms
/// that hide a stage, at whatever index they end up on.
#[test]
fn stage_visibility_follows_the_algorithm_not_its_index() {
    let idx = |name: &str| QSM_ALGO_OPTIONS.iter().position(|o| *o == name).expect(name);
    let has_section = |app: &App, id: &str| {
        // Background removal's and field mapping's rows come from the registry; their pages
        // say when they do not run.
        let page = match id { "bgremoval" => Page::Background, "fieldmap" => Page::FieldMap, _ => unreachable!("{id}") };
        app.page_notes(page, &app.pipeline_state.ctx).is_none()
    };

    let mut app = App::new();
    app.set_qsm_algorithm(idx("rts"));
    assert!(has_section(&app, "bgremoval"), "RTS needs background removal");
    assert!(has_section(&app, "fieldmap"), "RTS needs a field map");

    // Single-step: own background removal, but still unwraps.
    for alg in ["nextqsm", "autoqsm"] {
        app.set_qsm_algorithm(idx(alg));
        assert!(!has_section(&app, "bgremoval"), "{alg} does its own background removal");
        assert!(has_section(&app, "fieldmap"), "{alg} still needs a field map");
    }
    // End-to-end from wrapped phase: neither.
    for alg in ["iqsm", "iqsm-plus"] {
        app.set_qsm_algorithm(idx(alg));
        assert!(!has_section(&app, "bgremoval"), "{alg} takes wrapped phase");
        assert!(!has_section(&app, "fieldmap"), "{alg} takes wrapped phase");
    }
    // Total-field methods with their own background removal still map the field: QSMART,
    // TFI, and TGV on multi-echo data (QSM.rs `InversionConfig::input`).
    for alg in ["qsmart", "tfi", "tgv"] {
        app.set_qsm_algorithm(idx(alg));
        assert!(!has_section(&app, "bgremoval"), "{alg} removes the background itself");
        assert!(has_section(&app, "fieldmap"), "{alg} reads the total field");
    }
    // TGV on a single echo reconstructs from its wrapped phase.
    app.set_qsm_algorithm(idx("tgv"));
    app.pipeline_state.ctx.num_echoes = Some(1);
    assert!(!has_section(&app, "fieldmap"), "single-echo TGV takes wrapped phase");
    app.pipeline_state.ctx.num_echoes = Some(4);
    assert!(has_section(&app, "fieldmap"), "multi-echo TGV maps the field");
    app.pipeline_state.ctx.num_echoes = None;
    // iQFM makes the local field from phase: no field mapping in front of it.
    app.set_qsm_algorithm(idx("rts"));
    app.set_field_select("bg_removal.algorithm", BF_OPTIONS.iter().position(|o| *o == "iqfm").unwrap());
    assert!(!has_section(&app, "fieldmap"), "iQFM unwraps and removes the background itself");
    app.set_field_select("bg_removal.algorithm", 0);
    // LSQR (and LSQR+HEIDI) is an ordinary local-field inversion.
    app.set_qsm_algorithm(idx("lsqr"));
    app.form.settings.set("inversion.heidi.enabled", true);
    assert!(has_section(&app, "bgremoval"), "LSQR inverts the local field");
    assert!(has_section(&app, "fieldmap"), "LSQR needs a field map");
}

/// Compute QSM off hides only the reconstruction: the geometry, motion correction, coil
/// combination, masking and supplied segmentation serve SWI, T2*/R2*, R2' and the
/// segmentation as much as QSM. An output that forces QSM keeps the reconstruction on show.
#[test]
fn qsm_off_keeps_the_preprocessing_rows() {
    let fields = all_fields;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.form.settings.set("pipeline.do_qsm", false);
    let f = fields(&mut app);
    for kept in ["pipeline.axial_resampling", "masking.inhomogeneity_correction", "pipeline.ignore_slice_gap",
                 "motion_correction.enabled", "field_mapping.coil_combination_sigma",
                 "mask_preset", "masking.custom_mask_tool", "masking.two_pass"] {
        assert!(f.contains(&kept.to_string()), "{kept} hidden with QSM off: {f:?}");
    }
    // The QSM sections stay listed but hold only a note.
    for gone in ["field_mapping.unwrapping_algorithm", "bg_removal.algorithm", "inversion.algorithm", "qsm.reference", "pipeline.crop_to_mask", "pipeline.fft_padding"] {
        assert!(!f.contains(&gone.to_string()), "{gone} shown with QSM off: {f:?}");
    }
    // Motion correction's own settings follow it, not QSM.
    app.toggle_field("motion_correction.enabled");
    let f = fields(&mut app);
    assert!(f.contains(&"motion_correction.reference".to_string()) && f.contains(&"registration.levels".to_string()), "{f:?}");

    // SMWI needs a χ map: QSM runs regardless, and its settings are on show, with the reason.
    app.form.settings.set("pipeline.do_smwi", true);
    let f = fields(&mut app);
    assert!(f.contains(&"inversion.algorithm".to_string()) && f.contains(&"field_mapping.unwrapping_algorithm".to_string()), "{f:?}");
    assert!(app.pipeline_rows().iter().any(|r|
        matches!(r, PipelineRow::Note { text } if text.contains("SMWI"))), "the forced QSM says why");
}

/// Bipolar correction is part of MCPC-3D-S phase offset removal, which runs before any unwrapper.
#[test]
fn bipolar_correction_needs_offset_removal() {
    let mut app = App::new();
    let shows = |app: &App| app.pipeline_rows().iter().any(|r|
        matches!(r, PipelineRow::Toggle { field: "field_mapping.bipolar_correction", .. }));
    app.toggle_field("field_mapping.bipolar_correction");
    let passed = |app: &App| super::super::super::command::config_from_app(app).unwrap().field_mapping.bipolar_correction;
    assert!(app.field_toggle("field_mapping.phase_offset_removal") && app.field_select("field_mapping.unwrapping_algorithm") == 0);
    assert!(shows(&app) && passed(&app));
    app.toggle_field("field_mapping.phase_offset_removal");
    assert!(!shows(&app) && !passed(&app), "no offset removal, no bipolar correction");
    app.toggle_field("field_mapping.phase_offset_removal");
    app.set_field_select("field_mapping.unwrapping_algorithm", 1);
    assert!(shows(&app) && passed(&app), "Laplacian unwrapping removes offsets first too");
}

/// The bring-your-own inputs and the R2' source are reachable wherever a map they feed is
/// made, not only with χ-separation on.
#[test]
fn supplied_inputs_follow_the_maps_that_read_them() {
    let fields = all_fields;
    let has = |f: &[String], x: &str| f.iter().any(|y| y == x);
    let mut app = App::new();
    let f = fields(&mut app);
    for x in ["separation.custom_qsm_tool", "separation.custom_r2_tool", "separation.custom_r2prime_tool", "separation.r2prime_strategy"] {
        assert!(!has(&f, x), "nothing reads a supplied map: {x}");
    }

    app.form.settings.set("pipeline.do_r2primemap", true);
    let f = fields(&mut app);
    assert!(has(&f, "separation.custom_r2prime_tool") && has(&f, "separation.custom_r2_tool") && has(&f, "separation.r2prime_strategy"), "{f:?}");
    assert!(!has(&f, "separation.custom_qsm_tool"), "{f:?}");
    // R2PRIMEnet predicts R2' from R2*: no R2 is read.
    app.set_field_select("separation.r2prime_strategy", 2);
    assert!(!has(&fields(&mut app), "separation.custom_r2_tool"));
    // A supplied R2' replaces the computation: its source no longer applies, its row stays.
    app.set_field_select("separation.r2prime_strategy", 0);
    *app.field_text_mut("separation.custom_r2prime_tool").unwrap() = "*".into();
    let f = fields(&mut app);
    assert!(has(&f, "separation.custom_r2prime_tool") && !has(&f, "separation.r2prime_strategy") && !has(&f, "separation.custom_r2_tool"), "{f:?}");

    let mut app = App::new();
    app.form.settings.set("pipeline.do_analysis", true);
    assert!(has(&fields(&mut app), "separation.custom_qsm_tool"), "the statistics can read a supplied χ map");
    let mut app = App::new();
    app.form.settings.set("pipeline.do_r2map", true);
    let f = fields(&mut app);
    assert!(has(&f, "separation.custom_r2_tool") && !has(&f, "separation.custom_r2prime_tool"), "{f:?}");
}

/// A supplied segmentation satisfies the statistics and a region reference: SynthSeg is not
/// forced, so its checkbox reads as the user left it and can be toggled.
#[test]
fn a_custom_dseg_is_the_segmentation_source() {
    let mut app = App::new();
    app.form.settings.set("pipeline.do_analysis", true);
    assert!(app.output_made("pipeline.do_segmentation"), "needed without a supplied dseg");
    assert_eq!(app.field_select("segmentation_source"), 0, "SynthSeg");
    *app.field_text_mut("segmentation.custom_dseg_tool").unwrap() = "freesurfer".into();
    assert!(app.field_toggle("pipeline.do_segmentation"), "a supplied segmentation is a segmentation");
    assert_eq!(app.field_select("segmentation_source"), 1);
    assert!(!shows(&mut app, TAB_RECONSTRUCTION, "segmentation.version"), "no SynthSeg settings to show");
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "segmentation.custom_dseg_tool"));
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "pipeline.do_analysis"), "the statistics stay reachable");
    // The statistics need one, whichever the source: turning it off is refused.
    focus_on(&mut app, TAB_RECONSTRUCTION, "pipeline.do_segmentation");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.error_message.is_some() && app.field_text("segmentation.custom_dseg_tool") == "freesurfer");
    // Back to SynthSeg: the supplied one is dropped, SynthSeg asked for.
    app.set_field_select("segmentation_source", 0);
    assert!(app.field_text("segmentation.custom_dseg_tool").is_empty() && app.form.settings.toggle("pipeline.do_segmentation"));
    app.set_field_select("segmentation_source", 1);
    assert_eq!(app.field_text("segmentation.custom_dseg_tool"), "*");
    assert!(!app.form.settings.toggle("pipeline.do_segmentation"), "one representation per choice");
    // Unforced: off clears whichever source.
    app.form.settings.set("pipeline.do_analysis", false);
    app.toggle_field("pipeline.do_segmentation");
    assert!(!app.field_toggle("pipeline.do_segmentation") && app.field_text("segmentation.custom_dseg_tool").is_empty());
}

/// R2* is needed by what reads it (R2*-QSM, a computed R2'): made by the run, shown as the
/// user left it, and once ticked it cannot be turned off from under them.
#[test]
fn r2star_is_made_when_needed_and_reads_as_set() {
    let mut app = App::new();
    assert!(!app.field_toggle("pipeline.do_r2starmap"));
    app.form.settings.set("pipeline.do_r2primemap", true);
    assert!(!app.field_toggle("pipeline.do_r2starmap") && app.output_made("pipeline.do_r2starmap"), "R2' = R2* - R2");
    app.active_tab = TAB_RECONSTRUCTION;
    focus_on(&mut app, TAB_RECONSTRUCTION, "pipeline.do_r2starmap");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_r2starmap"), "{:?}", app.error_message);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_r2starmap") && app.error_message.as_deref().is_some_and(|m| m.contains("R2′ map")),
            "{:?}", app.error_message);
}

/// The help bar's tab hint and the digit keys both follow TAB_NAMES.
#[test]
fn every_tab_has_a_digit_and_nothing_else_does() {
    let mut app = App::new();
    for (i, _) in TAB_NAMES.iter().enumerate() {
        app.active_tab = 0;
        assert!(app.switch_tab(key(KeyCode::Char(char::from_digit(i as u32 + 1, 10).unwrap()))));
        assert_eq!(app.active_tab, i);
    }
    let past = char::from_digit(TAB_NAMES.len() as u32 + 1, 10).unwrap();
    assert!(!app.switch_tab(key(KeyCode::Char(past))));
    assert!(!app.switch_tab(key(KeyCode::Char('0'))));
}

/// The reliable-mask editor only exists once two-pass is on, and it is the ordinary mask
/// editor — input, generator and refinement rows — addressed at the two-pass index.
#[test]
fn two_pass_toggle_reveals_the_reliable_mask_editor() {
    use qsmxt_engine::{MaskOp, MaskThresholdMethod, MaskingInput};
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);

    let reliable_rows = |app: &App| app.rows().into_iter()
        .filter(|r| match r {
            PipelineRow::MaskOpInput { section } | PipelineRow::MaskOpGenerator { section }
            | PipelineRow::MaskOpAddStep { section } => is_two_pass_section(*section),
            PipelineRow::MaskTwoPassHeader => true,
            _ => false,
        })
        .count();
    assert_eq!(reliable_rows(&app), 0, "no reliable mask while two-pass is off");

    let focus_on = |app: &App, pred: fn(&PipelineRow) -> bool| {
        let rows = app.rows();
        app.focusable_rows().iter().position(|&ri| pred(&rows[ri]))
    };
    app.pipeline_state.focus =
        focus_on(&app, |r| matches!(r, PipelineRow::Toggle { field: "masking.two_pass", .. }))
            .expect("two-pass toggle not focusable");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.field_toggle("masking.two_pass"));
    assert!(reliable_rows(&app) > 0, "the reliable mask editor appears with the toggle");

    // It starts as the default recipe: an unfilled Otsu threshold on phase quality.
    let sections = &app.mask().two_pass_sections;
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].input, MaskingInput::PhaseQuality);
    assert!(matches!(sections[0].generator,
                     MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }));
    assert!(sections[0].refinements.is_empty());

    // Default recipe → a bare --two-pass in the generated command.
    let cmd = crate::command::build_command_string(&app);
    assert!(cmd.contains("--two-pass"), "cmd: {cmd}");
    assert!(!cmd.contains("--two-pass-mask"), "cmd: {cmd}");
}

/// Editing the reliable mask must not claim the main mask stopped following its preset — and
/// it must edit the reliable mask, not the main one.
#[test]
fn editing_the_reliable_mask_leaves_the_main_preset_alone() {
    use qsmxt_engine::MaskingInput;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.form.settings.set("masking.two_pass", true);
    let main_before = app.mask().sections.clone();

    let rows = app.rows();
    let focus = app.focusable_rows().iter()
        .position(|&ri| matches!(&rows[ri], PipelineRow::MaskOpInput { section } if is_two_pass_section(*section)))
        .expect("reliable input row not focusable");
    app.pipeline_state.focus = focus;
    step(&mut app, 1);

    assert_ne!(app.mask().two_pass_sections[0].input, MaskingInput::PhaseQuality,
               "the reliable mask's input should have changed");
    assert_eq!(app.mask().sections, main_before, "the main mask must be untouched");
    assert_eq!(app.pipeline_state.mask_preset, 0, "the main mask still follows its preset");

    // Edited away from the default, so the command now spells the reliable mask out.
    let cmd = crate::command::build_command_string(&app);
    assert!(cmd.contains("--two-pass-mask"), "cmd: {cmd}");
}

/// Under QSMART the default mask stays as it is, with no warning of QSMxT's own: QSMART's
/// reference mask is the rules' to point out (the mask row's ⓘ). Resetting the preset brings
/// the default back.
#[test]
fn qsmart_keeps_the_mask() {
    let qsmart = QSM_PICKER_OPTIONS.iter().position(|a| *a == "qsmart").unwrap();
    let mut app = App::new();
    let before = app.mask().sections.clone();
    app.set_field_select("inversion.algorithm", qsmart);
    assert_eq!(app.mask().sections, before, "choosing QSMART changes no mask");
    assert!(!app.pipeline_rows().iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("brain mask"))));
    app.set_field_select("mask_preset", 1);
    app.reset_field("mask_preset");
    assert_eq!(app.mask().sections, before);
}

/// Turning two-pass off hides the editor but keeps what was typed into it, so toggling back
/// on does not silently discard the recipe.
#[test]
fn two_pass_remembers_its_mask_while_off() {
    use qsmxt_engine::{MaskOp, MaskThresholdMethod};
    let mut app = App::new();
    app.form.settings.set("masking.two_pass", true);
    app.mask_mut().two_pass_sections[0].generator =
        MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(40.0) };

    app.toggle_field("masking.two_pass");
    assert!(!app.field_toggle("masking.two_pass"));
    // Off, so it contributes nothing to the config...
    let config = crate::command::config_from_app(&app).unwrap();
    assert!(!config.masking.two_pass);
    assert!(config.masking.two_pass_sections.is_none());
    // ...but the edit survives.
    assert!(matches!(app.mask().two_pass_sections[0].generator,
                     MaskOp::Threshold { method: MaskThresholdMethod::Percentile, .. }));
}

/// The combine row only appears once there is more than one section, and its OR / AND is
/// chosen from a list.
#[test]
fn test_pipeline_mask_combine_row_toggles() {
    use qsmxt_engine::MaskCombine;
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);

    let has_combine_row = |app: &App| app.rows().iter()
        .any(|r| matches!(r, PipelineRow::MaskCombineOp { .. }));
    assert!(!has_combine_row(&app), "a single section has nothing to combine with");

    // Add a second section via the "+ Add mask..." row.
    let focus_on = |app: &App, pred: fn(&PipelineRow) -> bool| {
        let rows = app.rows();
        app.focusable_rows().iter().position(|&ri| pred(&rows[ri]))
    };
    app.pipeline_state.focus = focus_on(&app, |r| matches!(r, PipelineRow::MaskOpAddSection { .. }))
        .expect("MaskOpAddSection not found");
    app.handle_key(key(KeyCode::Enter));
    assert!(has_combine_row(&app), "two sections show how they combine");

    app.pipeline_state.focus = focus_on(&app, |r| matches!(r, PipelineRow::MaskCombineOp { .. }))
        .expect("MaskCombineOp not focusable");
    assert_eq!(app.mask().combine, MaskCombine::Or);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.mask().combine, MaskCombine::Or, "→ changes nothing");
    // Enter opens OR / AND with OR highlighted; Esc keeps it.
    app.handle_key(key(KeyCode::Enter));
    let m = app.algo_modal.as_ref().expect("the combine list");
    assert_eq!((m.options.clone(), m.cursor), (vec!["OR".to_string(), "AND".to_string()], 0));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mask().combine, MaskCombine::Or);
    step(&mut app, 1);
    assert_eq!(app.mask().combine, MaskCombine::And);
    assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM, "editing the recipe marks it custom");
    step(&mut app, 1);
    assert_eq!(app.mask().combine, MaskCombine::Or);
}

/// With two-pass on, the reliable mask's "Combined with" row is its own control: ←/→ toggle
/// its mode, which starts as the main mask's and follows it until set, and leaves the main
/// mask (and its preset) alone. The run gets it as `--two-pass-mask-combine`.
#[test]
fn the_reliable_mask_has_its_own_combine_mode() {
    use qsmxt_engine::MaskCombine;
    let mut app = App::new();
    app.form.bids_dir = "/bids".into();
    goto_page(&mut app, Page::Masking);
    app.form.settings.set("masking.two_pass", true);
    let extra = app.mask().two_pass_sections[0].clone();
    app.mask_mut().two_pass_sections.push(extra);
    let main = app.mask().sections[0].clone();
    app.mask_mut().sections.push(main);
    let rows = app.rows();
    let reliable = MASK_TWO_PASS_BASE + 1;
    assert!(rows.contains(&PipelineRow::MaskCombineOp { section: 1 }));
    assert!(rows.contains(&PipelineRow::MaskCombineOp { section: reliable }));
    assert!(!rows.iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("combined with"))));

    app.set_combine(1, MaskCombine::And);
    let m = app.mask();
    assert_eq!((m.combine, m.two_pass_combine_mode(), m.two_pass_combine), (MaskCombine::And, MaskCombine::And, None),
               "unset, the reliable mask follows the main mask");
    let preset = app.pipeline_state.mask_preset;
    app.pipeline_state.focus = app.focusable_rows().iter()
        .position(|&ri| app.rows()[ri] == PipelineRow::MaskCombineOp { section: reliable }).unwrap();
    step(&mut app, -1);
    let m = app.mask();
    assert_eq!((m.combine, m.two_pass_combine), (MaskCombine::And, Some(MaskCombine::Or)));
    assert_eq!(app.pipeline_state.mask_preset, preset, "the reliable mask is not the main mask's preset");
    let config = crate::command::config_from_app(&app).unwrap();
    assert_eq!((config.masking.combine, config.masking.resolved_two_pass_combine()), (MaskCombine::And, MaskCombine::Or));
    assert!(crate::command::build_command_string(&app).contains("--two-pass-mask-combine or"));
    step(&mut app, 1);
    assert_eq!(app.mask().two_pass_combine, Some(MaskCombine::And));
    app.handle_key(key(KeyCode::Char('R')));
    assert_eq!(app.mask().two_pass_combine, None, "R resets it with the rest of the recipe");
}

/// `d` removes the section a row belongs to — from its "Combined with" row (the section after
/// it) or any of its input and generator rows — in the main and the reliable mask alike, and
/// says so in the footer. A refinement step's rows remove only the step, and a mask's only
/// section stays.
#[test]
fn d_removes_the_section_a_row_belongs_to() {
    use qsmxt_engine::{MaskOp, MaskingInput};
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.form.settings.set("masking.two_pass", true);
    for two_pass in [false, true] {
        let base = if two_pass { MASK_TWO_PASS_BASE } else { 0 };
        let sections = |app: &App| if two_pass { app.mask().two_pass_sections.clone() } else { app.mask().sections.clone() };
        let set = |app: &mut App, s: Vec<qsmxt_engine::MaskSection>| {
            if two_pass { app.mask_mut().two_pass_sections = s } else { app.mask_mut().sections = s }
        };
        let press_d_on = |app: &mut App, row: PipelineRow| {
            let rows = app.rows();
            app.pipeline_state.focus = app.focusable_rows().iter().position(|&ri| rows[ri] == row)
                .unwrap_or_else(|| panic!("{row:?} not shown"));
            app.handle_key(key(KeyCode::Char('d')));
        };
        let mut first = sections(&app)[0].clone();
        first.refinements = vec![MaskOp::Erode { iterations: 1 }];
        let mut second = first.clone();
        second.input = MaskingInput::MagnitudeLast;
        let mut third = first.clone();
        third.input = MaskingInput::MagnitudeFirst;

        // The "Combined with" row removes the section after it.
        set(&mut app, vec![first.clone(), second.clone(), third.clone()]);
        assert!(app.mask().row_removes_section(&PipelineRow::MaskCombineOp { section: base + 1 }));
        press_d_on(&mut app, PipelineRow::MaskCombineOp { section: base + 1 });
        assert_eq!(sections(&app), vec![first.clone(), third.clone()], "two_pass {two_pass}");
        // A generator-setting row removes its section.
        set(&mut app, vec![first.clone(), second.clone(), third.clone()]);
        press_d_on(&mut app, PipelineRow::MaskOpGeneratorParam { section: base + 2 });
        assert_eq!(sections(&app), vec![first.clone(), second.clone()]);
        // A refinement step's row removes the step only.
        press_d_on(&mut app, PipelineRow::MaskOpEntry { section: base + 1, index: 0 });
        let mut bare = second.clone();
        bare.refinements.clear();
        assert_eq!(sections(&app), vec![first.clone(), bare]);
        // The footer offers it where it applies.
        let rows = app.rows();
        app.pipeline_state.focus = app.focusable_rows().iter()
            .position(|&ri| rows[ri] == PipelineRow::MaskOpInput { section: base + 1 }).unwrap();
        let footer = crate::ui::tests::footer(&mut app);
        assert!(footer.contains("d:Remove section"), "{footer}");
        press_d_on(&mut app, PipelineRow::MaskOpInput { section: base + 1 });
        // The only section stays, and the footer does not offer it.
        assert_eq!(sections(&app), vec![first.clone()]);
        assert!(!app.mask().row_removes_section(&PipelineRow::MaskOpGenerator { section: base }));
        press_d_on(&mut app, PipelineRow::MaskOpGenerator { section: base });
        assert_eq!(sections(&app), vec![first.clone()]);
        assert!(!crate::ui::tests::footer(&mut app).contains("Remove section"));
    }
}

/// Steps added under "On the combined mask" land in the combined list, not in a section.
#[test]
fn test_pipeline_mask_combined_refinement_steps() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    app.apply_mask_preset(3); // bet-and-phase: two sections
    let recipe = qsmxt_engine::bet_and_phase_mask_recipe();
    assert_eq!(app.mask().refinements, recipe.refinements);

    let section_refinements: usize = app.mask().sections.iter().map(|s| s.refinements.len()).sum();
    let rows = app.rows();
    let fi = app.focusable_rows().iter().position(|&ri| {
        matches!(rows[ri], PipelineRow::MaskOpAddStep { section: MASK_COMBINED_SECTION })
    }).expect("combined-mask add-step row not found");
    app.pipeline_state.focus = fi;
    app.handle_key(key(KeyCode::Enter)); // open the type selector
    app.handle_key(key(KeyCode::Enter)); // add the selected type
    assert_eq!(app.mask().refinements.len(), recipe.refinements.len() + 1);
    assert_eq!(app.mask().sections.iter().map(|s| s.refinements.len()).sum::<usize>(),
               section_refinements, "the step did not land in a section");

    // ...and `d` deletes from the same list.
    let rows = app.rows();
    let fi = app.focusable_rows().iter().position(|&ri| {
        matches!(rows[ri], PipelineRow::MaskOpEntry { section: MASK_COMBINED_SECTION, index: 0 })
    }).expect("combined-mask step row not found");
    app.pipeline_state.focus = fi;
    app.handle_key(key(KeyCode::Char('d')));
    assert_eq!(app.mask().refinements.len(), recipe.refinements.len());
}

#[test]
fn test_pipeline_mask_add_section() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    let initial_sections = app.mask().sections.len();
    // Find MaskOpAddSection focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut add_sec_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if matches!(&rows[ri], PipelineRow::MaskOpAddSection { .. }) {
            add_sec_focus = Some(fi);
            break;
        }
    }
    let fi = add_sec_focus.expect("MaskOpAddSection not found");
    app.pipeline_state.focus = fi;
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mask().sections.len(), initial_sections + 1);
}

#[test]
fn test_pipeline_mask_add_step() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    let initial_refs = app.mask().sections[0].refinements.len();
    // Find MaskOpAddStep for section 0
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut add_step_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::MaskOpAddStep { section: 0 } = &rows[ri] {
            add_step_focus = Some(fi);
            break;
        }
    }
    let fi = add_step_focus.expect("MaskOpAddStep not found");
    app.pipeline_state.focus = fi;
    // Enter opens the list of steps; Enter adds the highlighted one.
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.algo_modal.as_ref().map(|m| &m.target), Some(AlgoModalTarget::MaskAddStep(0))));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.mask().sections[0].refinements.len(), initial_refs + 1);
}

#[test]
fn test_pipeline_mask_add_step_chooses_from_a_list() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut add_step_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::MaskOpAddStep { section: 0 } = &rows[ri] {
            add_step_focus = Some(fi);
            break;
        }
    }
    let fi = add_step_focus.expect("MaskOpAddStep not found");
    app.pipeline_state.focus = fi;
    let available = MaskRecipe::available_op_types();
    let before = app.mask().sections[0].refinements.clone();
    // Esc closes the list and adds nothing.
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Esc));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.mask().sections[0].refinements, before);
    // The second type, chosen.
    app.handle_key(key(KeyCode::Enter));
    let m = app.algo_modal.as_ref().unwrap();
    assert_eq!(m.options, available.iter().map(|t| t.to_string()).collect::<Vec<_>>());
    assert_eq!(m.cursor, 0);
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    let mask = app.mask();
    let added = mask.sections[0].refinements.last().unwrap();
    assert_eq!(MaskRecipe::mask_op_label_value(added).0, available[1]);
    assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM);
}

#[test]
fn test_pipeline_mask_delete_refinement() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    let initial_refs = app.mask().sections[0].refinements.len();
    assert!(initial_refs > 0);
    // Find first MaskOpEntry focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut entry_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::MaskOpEntry { section: 0, .. } = &rows[ri] {
            entry_focus = Some(fi);
            break;
        }
    }
    let fi = entry_focus.expect("MaskOpEntry not found");
    app.pipeline_state.focus = fi;
    app.handle_key(key(KeyCode::Char('d')));
    assert_eq!(app.mask().sections[0].refinements.len(), initial_refs - 1);
}

#[test]
fn test_pipeline_mask_op_left_right_change_nothing() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    // Find MaskOpEntry focus and adjust with Left/Right
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut entry_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::MaskOpEntry { section: 0, .. } = &rows[ri] {
            entry_focus = Some(fi);
            break;
        }
    }
    let fi = entry_focus.expect("MaskOpEntry not found");
    app.pipeline_state.focus = fi;
    let before = app.mask().sections[0].refinements.clone();
    // →: nothing changes; ←: back to the sidebar.
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.mask().sections[0].refinements, before);
    app.handle_key(key(KeyCode::Left));
    assert!(app.subtab_focus, "← goes back to the sections");
    assert_eq!(app.mask().sections[0].refinements, before);
}

#[test]
fn test_pipeline_mask_generator_adjust() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    // Find MaskOpGenerator focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut gen_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::MaskOpGenerator { section: 0 } = &rows[ri] {
            gen_focus = Some(fi);
            break;
        }
    }
    let fi = gen_focus.expect("MaskOpGenerator not found");
    app.pipeline_state.focus = fi;
    // Cycle generator threshold -> bet -> hd-bet -> threshold (and back with Left)
    step(&mut app, 1);
    assert!(matches!(
        app.mask().sections[0].generator,
        qsmxt_engine::MaskOp::Bet { .. }
    ));
    step(&mut app, 1);
    assert!(matches!(
        app.mask().sections[0].generator,
        qsmxt_engine::MaskOp::HdBet { .. }
    ));
    step(&mut app, 1);
    assert!(matches!(
        app.mask().sections[0].generator,
        qsmxt_engine::MaskOp::MindGrab { tta: false }
    ));
    step(&mut app, 1);
    assert!(matches!(
        app.mask().sections[0].generator,
        qsmxt_engine::MaskOp::Rs2Net { .. }
    ));
    // Wraps back round to the first generator.
    step(&mut app, 1);
    assert!(matches!(
        app.mask().sections[0].generator,
        qsmxt_engine::MaskOp::Threshold { .. }
    ));
    step(&mut app, -1);
    assert!(matches!(
        app.mask().sections[0].generator,
        qsmxt_engine::MaskOp::Rs2Net { .. }
    ));
}

#[test]
fn test_pipeline_mask_generator_param_adjust() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    // Find MaskOpGeneratorParam focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut gp_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::MaskOpGeneratorParam { section: 0 } = &rows[ri] {
            gp_focus = Some(fi);
            break;
        }
    }
    let fi = gp_focus.expect("MaskOpGeneratorParam not found");
    app.pipeline_state.focus = fi;
    // The threshold method, chosen from its list.
    step(&mut app, 1);
    // Verify the method changed (Otsu -> Fixed)
    if let qsmxt_engine::MaskOp::Threshold { method, .. } = &app.mask().sections[0].generator {
        assert_eq!(*method, qsmxt_engine::MaskThresholdMethod::Fixed);
    }
}

#[test]
fn test_pipeline_mask_input_adjust() {
    let mut app = App::new();
    goto_page(&mut app, Page::Masking);
    // Find MaskOpInput focus
    let rows = app.rows();
    let focusable = app.focusable_rows();
    let mut inp_focus = None;
    for (fi, &ri) in focusable.iter().enumerate() {
        if let PipelineRow::MaskOpInput { section: 0 } = &rows[ri] {
            inp_focus = Some(fi);
            break;
        }
    }
    let fi = inp_focus.expect("MaskOpInput not found");
    app.pipeline_state.focus = fi;
    let before = app.mask().sections[0].input;
    step(&mut app, 1);
    // Should have cycled to a different input
    assert_ne!(app.mask().sections[0].input, before);
}
