use super::*;
use super::super::command::config_from_app;

/// The recipe the editor edits is the config's: an edit reaches the config, marks the preset
/// custom, and a fresh form set to that config holds the same recipe.
#[test]
fn the_recipe_is_the_configs() {
    let mut app = App::new_for_trial();
    app.set_field_select("mask_preset", 3); // BET and phase quality: two sections, AND
    app.add_mask_step(MASK_COMBINED_SECTION, "erode");
    assert_eq!(app.pipeline_state.mask_preset, *MASK_PRESET_CUSTOM);
    let m = app.mask();
    let c = config_from_app(&app).unwrap();
    assert_eq!((&c.masking.sections, c.masking.combine, &c.masking.refinements), (&m.sections, m.combine, &m.refinements));
    assert_eq!(c.masking.refinements.last(), Some(&MaskOp::Erode { iterations: 1 }));
    let mut fresh = App::new_for_trial();
    fresh.set_form_from_config(&c);
    assert_eq!(fresh.mask(), m);
}

/// The combine mode and the combined mask's steps apply only with more than one section: with
/// one, the form keeps them but the run does not take them.
#[test]
fn a_single_sections_combine_settings_are_kept_but_not_run() {
    let mut app = App::new_for_trial();
    app.set_field_select("mask_preset", 3);
    app.add_mask_step(MASK_COMBINED_SECTION, "erode");
    assert!(app.remove_mask_section(1));
    assert_eq!(app.mask().refinements.last(), Some(&MaskOp::Erode { iterations: 1 }), "kept");
    let c = config_from_app(&app).unwrap();
    assert!(c.masking.refinements.is_empty(), "not run");
}

/// The run takes the recipe as the command line spells it, as it always has from the form: a
/// fixed threshold without a value is the default one, BET's fractional intensity two places.
#[test]
fn the_run_takes_the_recipe_as_the_command_line_spells_it() {
    let mut app = App::new_for_trial();
    app.set_mask_threshold_method(0, 1);
    assert!(matches!(app.mask().sections[0].generator, MaskOp::Threshold { value: None, .. }));
    let generator = |app: &App| config_from_app(app).unwrap().masking.sections[0].generator.clone();
    assert_eq!(generator(&app), MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.5) });
    app.set_mask_generator(0, MASK_GENERATOR_TYPES.iter().position(|t| *t == "bet").unwrap());
    app.set_mask_value(&PipelineRow::MaskOpGeneratorParam { section: 0 }, "0.123").unwrap();
    assert_eq!(app.mask().mask_value_text(&PipelineRow::MaskOpGeneratorParam { section: 0 }).as_deref(), Some("0.123"));
    assert!(matches!(generator(&app), MaskOp::Bet { fractional_intensity, .. } if fractional_intensity == 0.12));
}

/// The Masking page's settings rows refuse unreadable text while they apply, as the
/// registry's rows elsewhere do.
#[test]
fn unreadable_masking_values_are_refused() {
    let mut app = App::new_for_trial();
    *app.field_text_mut("custom_mask_tool").unwrap() = " bet ".into();
    assert_eq!(config_from_app(&app).unwrap().masking.custom_mask_tool.as_deref(), Some("bet"));
    *app.field_text_mut("homogeneity_sigma_mm").unwrap() = "wide".into();
    let err = config_from_app(&app).expect_err("refused").to_string();
    assert!(err.contains("--homogeneity-sigma-mm wide: not a number"), "{err}");
}
