use super::*;
use super::super::command::config_from_app;
use super::super::app::QSM_ALGO_OPTIONS;

/// A text row shows what was typed; the run takes it when it reads as the setting's value,
/// the default when it is blank, and refuses it when it reads as no value (see
/// [`unreadable`]).
#[test]
fn typed_text_stands_for_its_value() {
    let mut app = App::new_for_trial();
    app.form.settings.set("pipeline.do_swi", true);
    *app.field_text_mut("swi_strength").unwrap() = "2.5".into();
    assert_eq!(config_from_app(&app).unwrap().swi.strength, 2.5);
    *app.field_text_mut("swi_strength").unwrap() = "2.".into();
    assert_eq!(app.field_text("swi_strength"), "2.");
    assert_eq!(config_from_app(&app).unwrap().swi.strength, 2.0);
    *app.field_text_mut("swi_strength").unwrap() = " ".into();
    assert_eq!(config_from_app(&app).unwrap().swi.strength, qsmxt_engine::SwiConfig::default().strength);
    *app.field_text_mut("swi_strength").unwrap() = "x".into();
    assert!(config_from_app(&app).is_err(), "unreadable text is refused, not run as the default");
}

/// Type `text` on `field`'s row and check that the config is refused with `message`, as the
/// command line refuses such a value, rather than run with the setting's default.
fn unreadable(app: &mut App, field: &str, text: &str, message: &str) {
    *app.field_text_mut(field).unwrap() = text.into();
    let err = config_from_app(app).expect_err(&format!("{field} = {text:?} is refused")).to_string();
    assert!(err.contains(message), "{field}: {err}");
}

#[test]
fn unreadable_geometry_value_is_refused() {
    let mut app = App::new_for_trial();
    unreadable(&mut app, "tukey_window", "abc", "--tukey-window abc: not a number");
}

#[test]
fn unreadable_swi_and_smwi_values_are_refused() {
    let mut app = App::new_for_trial();
    app.form.settings.set("pipeline.do_swi", true);
    unreadable(&mut app, "swi_strength", "1.2.3", "--swi-strength 1.2.3: not a number");
    *app.field_text_mut("swi_strength").unwrap() = String::new();
    app.form.settings.set("pipeline.do_smwi", true);
    unreadable(&mut app, "smwi_power", "abc", "--smwi-power abc: not a number");
}

#[test]
fn unreadable_motion_value_is_refused() {
    let mut app = App::new_for_trial();
    app.toggle_field("motion_correction");
    unreadable(&mut app, "motion_min_ncc", "abc", "--motion-min-ncc abc: not a number");
}

#[test]
fn unreadable_background_value_is_refused() {
    let mut app = App::new_for_trial();
    unreadable(&mut app, "vsharp_max_radius", "abc", "--vsharp-max-radius abc: not a number");
}

/// The Reference page has no typed row: a method and a region, both chosen from lists.
#[test]
fn the_reference_page_has_nothing_typed() {
    let typed: Vec<_> = schema::settings().iter()
        .filter(|s| s.section == Section::Reference && s.off.is_none())
        .filter(|s| matches!(s.value_type, ValueType::Int | ValueType::Float | ValueType::IntVec3
            | ValueType::FloatVec3 | ValueType::IntList | ValueType::Text))
        .map(|s| s.path).collect();
    assert!(typed.is_empty(), "test an unreadable value of {typed:?}");
}

#[test]
fn unreadable_relaxometry_value_is_refused() {
    let mut app = App::new_for_trial();
    app.form.settings.set("pipeline.do_r2primemap", true);
    unreadable(&mut app, "r2primenet_patch", "128 128", "--r2primenet-patch 128 128: not three whole numbers");
}

#[test]
fn unreadable_segmentation_value_is_refused() {
    let mut app = App::new_for_trial();
    app.toggle_field("do_segmentation");
    unreadable(&mut app, "synthseg_sigma", "abc", "--synthseg-sigma abc: not a number");
}

#[test]
fn unreadable_multi_orientation_value_is_refused() {
    let mut app = App::new_for_trial();
    app.filter_state.orientation_group = "acq".into();
    unreadable(&mut app, "orientation_lambda", "abc", "--multi-orientation-lambda abc: not a number");
    // Blank is no regularisation, as its placeholder says.
    *app.field_text_mut("orientation_lambda").unwrap() = String::new();
    assert!(config_from_app(&app).is_ok());
}

#[test]
fn unreadable_field_map_values_are_refused() {
    let mut app = App::new_for_trial();
    unreadable(&mut app, "romeo_template", "abc", "--romeo-template abc: not an echo number");
    *app.field_text_mut("romeo_template").unwrap() = "1".into();
    app.set_field_select("unwrapping_algorithm", 1);
    app.set_field_select("laplacian_solver", 1);
    unreadable(&mut app, "laplacian_fft_pad", "32 32", "--laplacian-fft-pad 32 32: not one whole number, or three");
}

#[test]
fn unreadable_inversion_values_are_refused() {
    let mut app = App::new_for_trial();
    app.set_qsm_algorithm(QSM_ALGO_OPTIONS.iter().position(|a| *a == "nltv").unwrap());
    unreadable(&mut app, "nltv_lambda", "abc", "--nltv-lambda abc: not a number");
    *app.field_text_mut("nltv_lambda").unwrap() = String::new();
    let ms = options("inversion.nltv.te").0.iter().position(|o| *o == "ms").unwrap();
    app.set_field_select("nltv_te", ms);
    unreadable(&mut app, "nltv_te_ms", "abc", "--nltv-te abc: not a time in ms");
}

#[test]
fn unreadable_separation_value_is_refused() {
    let mut app = App::new_for_trial();
    app.form.settings.set("pipeline.do_chi_separation", true);
    app.form.settings.set("separation.algorithm", "decompose");
    unreadable(&mut app, "sep_decompose_chi_bound", "abc", "--decompose-chi-bound abc: not a number");
}

/// A typed number outside its setting's range, or one the config cannot hold, is refused as
/// the run refuses it, rather than run as the setting's default: a number, a count, a triple.
#[test]
fn out_of_range_values_are_refused() {
    let mut app = App::new_for_trial();
    app.set_field_select("mask_preset", 1); // BET
    unreadable(&mut app, "bet_smoothness", "-1", "--bet-smoothness -1: must be greater than 0");
    *app.field_text_mut("bet_smoothness").unwrap() = String::new();
    unreadable(&mut app, "bet_iterations", "-1", "--bet-iterations -1: must be at least 0");
    *app.field_text_mut("bet_iterations").unwrap() = String::new();
    unreadable(&mut app, "homogeneity_nbox", "0", "--homogeneity-nbox 0: must be at least 1");
    *app.field_text_mut("homogeneity_nbox").unwrap() = String::new();
    app.set_field_select("inhomogeneity_method", 1);
    unreadable(&mut app, "n4_mesh", "1 0 1", "--n4-mesh 1 0 1: must be at least 1");
    *app.field_text_mut("n4_mesh").unwrap() = "2 2 2".into();
    assert_eq!(config_from_app(&app).unwrap().n4.mesh, [2, 2, 2]);
}

/// An out-of-range number in a row that does not apply does not block the run.
#[test]
fn an_out_of_range_hidden_value_does_not_block() {
    let mut app = App::new_for_trial();
    unreadable(&mut app, "homogeneity_nbox", "0", "--homogeneity-nbox 0");
    app.toggle_field("inhomogeneity_correction");
    assert!(config_from_app(&app).is_ok(), "the correction off: its boxes do not apply");
}

/// Unreadable text in a row that does not apply does not block the run: its setting is left
/// at its default, as for any value of a hidden setting.
#[test]
fn unreadable_text_of_a_hidden_setting_does_not_block() {
    let mut app = App::new_for_trial();
    app.form.settings.set("pipeline.do_swi", true);
    unreadable(&mut app, "swi_strength", "abc", "--swi-strength abc");
    app.toggle_field("do_swi");
    assert!(config_from_app(&app).is_ok(), "SWI off: its strength does not apply");
    // A time typed under a named echo time, and a method not chosen.
    app.set_qsm_algorithm(QSM_ALGO_OPTIONS.iter().position(|a| *a == "nltv").unwrap());
    let ms = options("inversion.nltv.te").0.iter().position(|o| *o == "ms").unwrap();
    app.set_field_select("nltv_te", ms);
    unreadable(&mut app, "nltv_te_ms", "abc", "--nltv-te abc");
    app.set_field_select("nltv_te", 1);
    assert!(config_from_app(&app).is_ok(), "a named echo time: the time typed does not apply");
    app.set_field_select("nltv_te", ms);
    assert!(config_from_app(&app).is_err());
    app.set_qsm_algorithm(0);
    assert!(config_from_app(&app).is_ok(), "another method: NLTV's echo time does not apply");
}

/// A stored setting that no longer applies is left at its default, though the form keeps it.
#[test]
fn a_hidden_setting_is_left_at_its_default() {
    let mut app = App::new_for_trial();
    app.form.settings.set("pipeline.do_swi", true);
    app.set_field_select("swi_scaling", 3);
    assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Negative);
    app.toggle_field("do_swi");
    assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Tanh);
    app.toggle_field("do_swi");
    assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Negative);
}

/// An echo number the run cannot take is refused, as the command line refuses it, rather
/// than run with another reference; with motion correction off it does not apply.
#[test]
fn an_unreadable_echo_number_is_an_error() {
    let mut app = App::new_for_trial();
    app.toggle_field("motion_correction");
    app.set_field_select("motion_reference", 2);
    *app.field_text_mut("motion_reference_echo").unwrap() = "x".into();
    let err = config_from_app(&app).expect_err("an unreadable echo number");
    assert!(err.to_string().contains("--motion-reference x"), "{err}");
    *app.field_text_mut("motion_reference_echo").unwrap() = "0".into();
    assert!(config_from_app(&app).is_err(), "echoes count from 1");
    app.toggle_field("motion_correction");
    assert!(config_from_app(&app).is_ok(), "motion correction off: the number does not apply");
}

/// The echo number typed stays while a named echo is chosen, and is the reference again once
/// the echo number is.
#[test]
fn the_echo_number_survives_a_named_choice() {
    use qsmxt_engine::MotionReference;
    let mut app = App::new_for_trial();
    app.toggle_field("motion_correction");
    app.set_field_select("motion_reference", 2);
    *app.field_text_mut("motion_reference_echo").unwrap() = "3".into();
    let reference = |app: &App| config_from_app(app).unwrap().motion_correction.reference;
    assert_eq!(reference(&app), MotionReference::Echo(3));
    app.set_field_select("motion_reference", 1);
    assert_eq!(reference(&app), MotionReference::Last);
    app.set_field_select("motion_reference", 2);
    assert_eq!(app.field_text("motion_reference_echo"), "3");
    assert_eq!(reference(&app), MotionReference::Echo(3));
}

/// ROMEO's template echo is typed from 1, as the command line takes it, and the config counts
/// from 0; the FFT pad is one value for every axis, or three.
#[test]
fn rows_type_values_as_the_command_line_does() {
    let mut app = App::new_for_trial();
    assert_eq!(app.field_text("romeo_template"), "1");
    *app.field_text_mut("romeo_template").unwrap() = "3".into();
    assert_eq!(config_from_app(&app).unwrap().field_mapping.romeo.template, 2);
    app.set_field_select("unwrapping_algorithm", 1);
    app.set_field_select("laplacian_solver", 1);
    assert_eq!(app.field_text("laplacian_fft_pad"), "64");
    *app.field_text_mut("laplacian_fft_pad").unwrap() = "32".into();
    assert_eq!(config_from_app(&app).unwrap().field_mapping.laplacian_fft_pad, [32; 3]);
}

/// An echo time is a select with a time in ms under it while a given time is chosen: the row
/// starts at the method's default time, and the time typed waits under a named echo time.
#[test]
fn an_echo_time_keeps_its_time_under_a_named_one() {
    use qsmxt_engine::EchoTimeChoice;
    let mut app = App::new_for_trial();
    app.set_qsm_algorithm(QSM_ALGO_OPTIONS.iter().position(|a| *a == "nltv").unwrap());
    let te = |app: &App| config_from_app(app).unwrap().inversion.nltv.te;
    let ms = options("inversion.nltv.te").0.iter().position(|o| *o == "ms").unwrap();
    assert!(options("inversion.nltv.te").1[0].ends_with("(default)"), "NLTV's default is the first echo time");
    app.set_field_select("nltv_te", ms);
    assert_eq!(app.field_text("nltv_te_ms"), "20");
    assert_eq!(te(&app), EchoTimeChoice::Seconds(0.02));
    *app.field_text_mut("nltv_te_ms").unwrap() = "7".into();
    assert_eq!(te(&app), EchoTimeChoice::Seconds(0.007));
    app.set_field_select("nltv_te", 1);
    assert_eq!(te(&app), EchoTimeChoice::Last);
    app.set_field_select("nltv_te", ms);
    assert_eq!((app.field_text("nltv_te_ms").as_ref(), te(&app)), ("7", EchoTimeChoice::Seconds(0.007)));
}

/// DECOMPOSE's χ bound is a number or a named choice: the select offers the number first, its
/// row under it while it is chosen, and the number typed waits under the named choice.
#[test]
fn a_number_or_choice_keeps_its_number_under_a_choice() {
    use qsmxt_engine::ChiBoundChoice;
    let mut app = App::new_for_trial();
    app.form.settings.set("pipeline.do_chi_separation", true);
    app.form.settings.set("separation.algorithm", "decompose");
    let bound = |app: &App| config_from_app(app).unwrap().separation.decompose.chi_bound;
    assert_eq!(options("separation.decompose.chi_bound").0, ["ppm", "echo-spacing"]);
    assert_eq!(app.field_text("sep_decompose_chi_bound"), "0.5");
    *app.field_text_mut("sep_decompose_chi_bound").unwrap() = "0.3".into();
    assert_eq!(bound(&app), ChiBoundChoice::Ppm(0.3));
    app.set_field_select("sep_decompose_chi_bound_mode", 1);
    assert_eq!(bound(&app), ChiBoundChoice::EchoSpacing);
    app.set_field_select("sep_decompose_chi_bound_mode", 0);
    assert_eq!((app.field_text("sep_decompose_chi_bound").as_ref(), bound(&app)), ("0.3", ChiBoundChoice::Ppm(0.3)));
}

/// Posterior smoothing is a checkbox with its sigma under it: off is a sigma of 0, and the
/// sigma typed is kept while it is off and taken again when it is back on.
#[test]
fn a_checkbox_with_a_value_keeps_its_value_while_off() {
    let mut app = App::new_for_trial();
    app.toggle_field("do_segmentation");
    let sigma = |app: &App| config_from_app(app).unwrap().segmentation.sigma_smoothing;
    assert!(app.field_toggle("synthseg_smoothing"));
    *app.field_text_mut("synthseg_sigma").unwrap() = "1.5".into();
    assert_eq!(sigma(&app), 1.5);
    app.toggle_field("synthseg_smoothing");
    assert!(!app.field_toggle("synthseg_smoothing"));
    assert_eq!(sigma(&app), 0.0);
    app.toggle_field("synthseg_smoothing");
    assert_eq!((app.field_text("synthseg_sigma").as_ref(), sigma(&app)), ("1.5", 1.5));
}
