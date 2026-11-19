//! The other tabs: SLURM and execution, other outputs, Methods, run checks, resets, DICOM export.

use super::*;

// --- SLURM field visibility ---

#[test]
fn test_field_visibility_local_mode() {
    let mut app = App::new();
    app.form.execution_mode = 0; // Local
    // Local-only fields visible
    assert!(shows(&mut app, TAB_EXECUTION, "execution_mode")); // Execution Mode
    assert!(shows(&mut app, TAB_EXECUTION, "dry_run")); // Dry Run
    assert!(shows(&mut app, TAB_EXECUTION, "debug")); // Debug
    assert!(shows(&mut app, TAB_EXECUTION, "n_procs")); // Num Processes
    // SLURM fields hidden
    assert!(!shows(&mut app, TAB_EXECUTION, "slurm_account")); // SLURM Account
    assert!(!shows(&mut app, TAB_EXECUTION, "slurm_partition")); // SLURM Partition
    assert!(!shows(&mut app, TAB_EXECUTION, "slurm_time")); // SLURM Time
    assert!(!shows(&mut app, TAB_EXECUTION, "slurm_mem")); // SLURM Mem
    assert!(!shows(&mut app, TAB_EXECUTION, "slurm_cpus")); // SLURM CPUs
    assert!(!shows(&mut app, TAB_EXECUTION, "slurm_submit")); // Auto-Submit
}

#[test]
fn test_field_visibility_slurm_mode() {
    let mut app = App::new();
    app.form.execution_mode = 1; // SLURM
    // Execution Mode always visible
    assert!(shows(&mut app, TAB_EXECUTION, "execution_mode"));
    // Dry Run and Num Processes hidden in SLURM mode
    assert!(!shows(&mut app, TAB_EXECUTION, "dry_run")); // Dry Run
    assert!(!shows(&mut app, TAB_EXECUTION, "n_procs")); // Num Processes
    // Debug visible in both modes
    assert!(shows(&mut app, TAB_EXECUTION, "debug"));
    // SLURM fields visible
    assert!(shows(&mut app, TAB_EXECUTION, "slurm_account"));
    assert!(shows(&mut app, TAB_EXECUTION, "slurm_partition"));
    assert!(shows(&mut app, TAB_EXECUTION, "slurm_time"));
    assert!(shows(&mut app, TAB_EXECUTION, "slurm_mem"));
    assert!(shows(&mut app, TAB_EXECUTION, "slurm_cpus"));
    assert!(shows(&mut app, TAB_EXECUTION, "slurm_submit"));
}

#[test]
fn test_field_visibility_other_tabs() {
    let mut app = App::new();
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "do_swi"));
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "do_r2starmap"));
}

// --- SLURM form fields ---

#[test]
fn test_slurm_form_defaults() {
    let form = RunForm::default();
    assert_eq!(form.execution_mode, 0);
    assert!(form.slurm_account.is_empty());
    assert!(form.slurm_partition.is_empty());
    assert_eq!(form.slurm_time, "02:00:00");
    assert_eq!(form.slurm_mem, "", "empty: sized per run from the memory estimate");
    assert_eq!(form.slurm_cpus, "4");
    assert!(!form.slurm_submit);
}

// --- Execution mode select ---

#[test]
fn test_execution_mode_select() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    assert_eq!(app.field_select("execution_mode"), 0); // Local
    app.form.execution_mode = 1;
    assert_eq!(app.field_select("execution_mode"), 1); // SLURM
}

// --- SLURM field accessors ---

#[test]
fn test_slurm_text_values() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.slurm_account = "myacct".to_string();
    app.form.slurm_partition = "gpu".to_string();
    app.form.slurm_time = "04:00:00".to_string();
    app.form.slurm_mem = "64".to_string();
    app.form.slurm_cpus = "8".to_string();
    assert_eq!(app.field_text("slurm_account"), "myacct");
    assert_eq!(app.field_text("slurm_partition"), "gpu");
    assert_eq!(app.field_text("slurm_time"), "04:00:00");
    assert_eq!(app.field_text("slurm_mem"), "64");
    assert_eq!(app.field_text("slurm_cpus"), "8");
}

#[test]
fn test_slurm_checkbox_value() {
    let mut app = App::new();
    assert!(!app.field_toggle("slurm_submit")); // slurm_submit default
    app.form.slurm_submit = true;
    assert!(app.field_toggle("slurm_submit"));
}

// --- Reset SLURM fields ---

#[test]
fn test_reset_slurm_fields() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    app.form.slurm_account = "changed".to_string();
    app.form.slurm_time = "99:00:00".to_string();
    // Reset entire tab
    app.reset_current_tab();
    assert_eq!(app.form.execution_mode, 0);
    assert!(app.form.slurm_account.is_empty());
    assert_eq!(app.form.slurm_time, "02:00:00");
}

#[test]
fn test_reset_single_slurm_field() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    focus_on(&mut app, TAB_EXECUTION, "slurm_account");
    app.form.slurm_account = "changed".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.form.slurm_account.is_empty());
}

// --- Navigation skips hidden fields ---

#[test]
fn test_navigation_skips_hidden_slurm_fields() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1; // SLURM — hides Dry Run and Num Processes
    focus_on(&mut app, TAB_EXECUTION, "execution_mode");
    // Down from the mode skips the hidden Dry Run and lands on SLURM Account
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.focused_field_name().as_deref(), Some("slurm_account"));
}

// ========== Other outputs tab handler tests ==========

#[test]
fn test_supplementary_toggle_swi() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
    assert!(!app.form.settings.toggle("pipeline.do_swi"));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_swi"));
    app.handle_key(key(KeyCode::Char(' ')));
    assert!(!app.form.settings.toggle("pipeline.do_swi"));
}

#[test]
fn test_supplementary_toggle_t2star() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    focus_on(&mut app, TAB_RECONSTRUCTION, "do_t2starmap");
    assert!(!app.form.settings.toggle("pipeline.do_t2starmap"));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.settings.toggle("pipeline.do_t2starmap"));
}

#[test]
fn test_supplementary_toggle_r2star() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    focus_on(&mut app, TAB_RECONSTRUCTION, "do_r2starmap");
    assert!(!app.form.settings.toggle("pipeline.do_r2starmap"));
    app.handle_key(key(KeyCode::Char(' ')));
    assert!(app.form.settings.toggle("pipeline.do_r2starmap"));
}

#[test]
fn test_supplementary_swi_scaling_select() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.form.settings.set("pipeline.do_swi", true); // make SWI fields visible
    focus_on(&mut app, TAB_RECONSTRUCTION, "swi_scaling");
    let scaling = |app: &App| app.form.settings.get("swi.scaling");
    assert_eq!(scaling(&app), "tanh");
    step(&mut app, 1);
    assert_eq!(scaling(&app), "negative-tanh");
    step(&mut app, -1);
    assert_eq!(scaling(&app), "tanh");
}

#[test]
fn test_supplementary_edit_swi_strength() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.form.settings.set("pipeline.do_swi", true);
    focus_on(&mut app, TAB_RECONSTRUCTION, "swi_strength");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.editing);
    // Type something
    app.handle_key(key(KeyCode::End));
    app.handle_key(key(KeyCode::Char('0')));
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.pipeline_state.editing);
}

#[test]
fn test_supplementary_navigate_fields() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
    // SWI fields are hidden when do_swi is false: Compute SWI is the section's only row.
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.focused_field_name().as_deref(), Some("do_swi"));
    app.form.settings.set("pipeline.do_swi", true);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.focused_field_name().as_deref(), Some("swi_unwrap"));
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.focused_field_name().as_deref(), Some("do_swi"));
}

#[test]
fn test_supplementary_swi_visible_when_enabled() {
    let mut app = App::new();
    app.form.settings.set("pipeline.do_swi", true);
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_scaling")); // SWI Scaling
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_strength")); // SWI Strength
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_mip_window")); // SWI MIP Window
    assert!(shows(&mut app, TAB_RECONSTRUCTION, "swi_filtered_phase"));
}

/// The filtered phase is ticked under SWI, and then listed for DICOM export.
#[test]
fn swi_filtered_phase_is_an_swi_output_and_exportable() {
    let mut app = App::new();
    let phase = dicom_map("dicom_out_swi_phase").unwrap();
    app.form.settings.set("pipeline.do_swi", true);
    assert!(!app.form.settings.toggle("swi.filtered_phase") && !app.dicom_map_made(phase), "off by default");
    app.form.settings.set("swi.filtered_phase", true);
    assert!(app.dicom_map_made(phase));
    app.form.export_dicom = true;
    let export: Vec<&str> = app.page_rows(Page::Export).iter().filter_map(row_field).collect();
    assert!(export.contains(&"dicom_out_swi_phase"), "listed in Maps to export: {export:?}");
    app.form.dicom_unticked = vec!["desc-swi_phase".into()];
    assert!(!app.dicom_outputs().unwrap().iter().any(|m| m == "desc-swi_phase"));
    // Without SWI there is no phase, and the row is hidden.
    app.form.settings.set("pipeline.do_swi", false);
    assert!(!app.dicom_map_made(phase));
    assert!(!shows(&mut app, TAB_RECONSTRUCTION, "swi_filtered_phase"));
}

#[test]
fn test_supplementary_swi_hidden_when_disabled() {
    let mut app = App::new();
    assert!(!shows(&mut app, TAB_RECONSTRUCTION, "swi_scaling"));
    assert!(!shows(&mut app, TAB_RECONSTRUCTION, "swi_mip_window"));
}

#[test]
fn test_supplementary_tab_switch() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, TAB_EXECUTION);
}

#[test]
fn test_supplementary_reset_field() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    focus_on(&mut app, TAB_RECONSTRUCTION, "do_swi");
    app.form.settings.set("pipeline.do_swi", true);
    app.handle_key(key(KeyCode::Char('r')));
    assert!(!app.form.settings.toggle("pipeline.do_swi")); // reset to default
}

#[test]
fn test_supplementary_reset_tab() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.form.settings.set("pipeline.do_swi", true);
    app.form.settings.set("pipeline.do_t2starmap", true);
    app.form.settings.set("pipeline.do_r2starmap", true);
    app.form.settings.set("swi.scaling", "negative");
    app.sub_page[TAB_RECONSTRUCTION] = Some(Page::Swi);
    app.sync_pipeline_mode();
    app.handle_key(key(KeyCode::Char('R')));
    assert!(!app.form.settings.toggle("pipeline.do_swi"));
    assert_eq!(app.form.settings.get("swi.scaling"), "tanh");
    assert!(app.form.settings.toggle("pipeline.do_t2starmap") && app.form.settings.toggle("pipeline.do_r2starmap"), "Relaxometry is another page");
    focus_on(&mut app, TAB_RECONSTRUCTION, "do_t2starmap");
    app.handle_key(key(KeyCode::Char('R')));
    assert!(!app.form.settings.toggle("pipeline.do_t2starmap") && !app.form.settings.toggle("pipeline.do_r2starmap"));
}

// ========== Execution tab handler tests ==========

#[test]
fn test_execution_mode_left_right_change_nothing() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    focus_on(&mut app, TAB_EXECUTION, "execution_mode");
    assert_eq!(app.form.execution_mode, 0); // Local
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.form.execution_mode, 0, "←/→ change no value");
    assert!(app.algo_modal.is_none() && !app.on_subtabs() && !app.tab_focus, "and Execution has no sections to go back to");
}

#[test]
fn test_execution_mode_enter_opens_modal() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    focus_on(&mut app, TAB_EXECUTION, "execution_mode");
    // Enter opens the modal (rather than cycling); selecting SLURM applies it.
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_some());
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.form.execution_mode, 1); // SLURM
    // Esc closes it again with nothing changed.
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Esc));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.form.execution_mode, 1); // still SLURM
}

#[test]
fn test_execution_toggle_dry_run() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    focus_on(&mut app, TAB_EXECUTION, "dry_run");
    assert!(!app.form.dry_run);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.dry_run);
}

#[test]
fn test_execution_toggle_debug() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    focus_on(&mut app, TAB_EXECUTION, "debug");
    assert!(!app.form.debug);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.debug);
}

#[test]
fn test_execution_edit_n_procs() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    focus_on(&mut app, TAB_EXECUTION, "n_procs");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.editing);
    app.handle_key(key(KeyCode::Char('4')));
    assert_eq!(app.form.n_procs, "4");
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.pipeline_state.editing);
}

#[test]
fn test_execution_edit_slurm_account() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1; // SLURM mode
    focus_on(&mut app, TAB_EXECUTION, "slurm_account");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.editing);
    app.handle_key(key(KeyCode::Char('a')));
    app.handle_key(key(KeyCode::Char('c')));
    app.handle_key(key(KeyCode::Char('t')));
    assert_eq!(app.form.slurm_account, "act");
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.pipeline_state.editing);
}

#[test]
fn test_execution_edit_slurm_partition() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    focus_on(&mut app, TAB_EXECUTION, "slurm_partition");
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Char('g')));
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('u')));
    assert_eq!(app.form.slurm_partition, "gpu");
}

#[test]
fn test_execution_edit_slurm_time() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    focus_on(&mut app, TAB_EXECUTION, "slurm_time");
    app.handle_key(key(KeyCode::Enter));
    // Clear existing "02:00:00"
    for _ in 0..8 {
        app.handle_key(key(KeyCode::Backspace));
    }
    app.handle_key(key(KeyCode::Char('1')));
    assert_eq!(app.form.slurm_time, "1");
}

#[test]
fn test_execution_toggle_auto_submit() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    focus_on(&mut app, TAB_EXECUTION, "slurm_submit");
    assert!(!app.form.slurm_submit);
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.slurm_submit);
}

#[test]
fn test_execution_reset_field() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    focus_on(&mut app, TAB_EXECUTION, "slurm_time");
    app.form.slurm_time = "99:99:99".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.form.slurm_time, "02:00:00");
}

#[test]
fn test_execution_reset_tab() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    app.form.dry_run = true;
    app.form.debug = true;
    app.form.slurm_account = "x".to_string();
    app.handle_key(key(KeyCode::Char('R')));
    assert_eq!(app.form.execution_mode, 0);
    assert!(!app.form.dry_run);
    assert!(!app.form.debug);
    assert!(app.form.slurm_account.is_empty());
}

// ========== Methods tab handler tests ==========

#[test]
fn test_methods_tab_scroll_down() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    assert_eq!(app.methods_scroll_offset, 0);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.methods_scroll_offset, 1);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.methods_scroll_offset, 2);
}

#[test]
fn test_methods_tab_scroll_up() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.methods_scroll_offset = 3;
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.methods_scroll_offset, 2);
    app.handle_key(key(KeyCode::Char('k')));
    assert_eq!(app.methods_scroll_offset, 1);
}

#[test]
fn test_methods_tab_scroll_up_at_zero() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.methods_scroll_offset, 0);
    assert!(app.on_subtabs(), "↑ at the top goes up to the sub-tab strip");
}

/// The Methods tab opens on its sidebar with the methods text chosen; the pipeline graph is
/// the second page, reached with ↓, `]` or `[`, and → goes into it.
#[test]
fn the_pipeline_is_the_methods_tabs_second_sub_tab() {
    let mut app = App::new();
    app.handle_key(key(KeyCode::Char('5')));
    assert_eq!(app.active_tab, TAB_METHODS);
    assert_eq!(app.pages(TAB_METHODS), vec![Page::MethodsText, Page::Pipeline]);
    assert_eq!(app.page(), Some(Page::MethodsText));
    assert!(app.on_subtabs() && app.sidebar());
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.page(), Some(Page::Pipeline));
    assert!(app.on_subtabs());
    app.handle_key(key(KeyCode::Right));
    assert!(!app.on_subtabs());
    app.handle_key(key(KeyCode::Down));
    assert_eq!((app.pipeline_view.scroll, app.methods_scroll_offset), (1, 0), "↓ scrolls the graph");
    app.handle_key(key(KeyCode::Char('[')));
    assert_eq!(app.page(), Some(Page::MethodsText));
    app.handle_key(key(KeyCode::Char(']')));
    assert_eq!(app.page(), Some(Page::Pipeline));
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Up));
    assert!(app.on_subtabs());
    // Up the sidebar to the methods text, then to the tab bar.
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.page(), Some(Page::MethodsText));
    app.handle_key(key(KeyCode::Up));
    assert!(app.tab_focus);
    assert!(!TAB_NAMES.contains(&"Graph"), "the graph is a Methods sub-tab");
}

#[test]
fn test_methods_tab_quit() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_methods_tab_esc() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.handle_key(key(KeyCode::Esc));
    assert!(app.should_quit);
}

#[test]
fn test_methods_tab_switch() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, 0);
}

#[test]
fn test_methods_tab_backtab() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(app.active_tab, TAB_EXECUTION);
}

#[test]
fn test_methods_tab_number_switch() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.handle_key(key(KeyCode::Char('2')));
    assert_eq!(app.active_tab, 1);
}

#[test]
fn test_methods_tab_f5_triggers_run() {
    let mut app = App::new();
    app.active_tab = TAB_METHODS;
    app.form.bids_dir = "/tmp/bids".to_string();
    app.handle_key(key(KeyCode::F(5)));
    assert!(app.should_run);
}

/// Enter, `c` and `y` copy the methods text (F5 still runs); the next key clears the message.
#[test]
fn methods_tab_enter_copies_the_methods_text() {
    for k in [KeyCode::Enter, KeyCode::Char('c'), KeyCode::Char('y')] {
        let mut app = App::new();
        app.active_tab = TAB_METHODS;
        app.form.bids_dir = "/tmp/bids".to_string();
        app.handle_key(key(k));
        assert!(!app.should_run, "{k:?} ran the pipeline");
        let copied = app.clipboard.take().unwrap_or_else(|| panic!("{k:?} copied nothing"));
        assert_eq!(copied, crate::tui::command::methods_text(&app).unwrap());
        assert!(copied.contains("processing was performed using qsmxt"), "{copied}");
        assert_eq!(app.status_message.as_deref(), Some("Methods copied to clipboard"));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.status_message, None);
    }
}

// ========== try_run validation tests ==========

#[test]
fn test_try_run_dicom_not_converted() {
    let mut app = App::new();
    app.input_mode = InputMode::DicomToBids;
    app.form.bids_dir = "/tmp/bids".to_string();
    app.try_run();
    assert!(!app.should_run);
    assert!(app.error_message.as_deref().unwrap().contains("Convert DICOM"));
}

#[test]
fn test_try_run_nifti_not_converted() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.form.bids_dir = "/tmp/bids".to_string();
    app.try_run();
    assert!(!app.should_run);
    assert!(app.error_message.as_deref().unwrap().contains("Convert NIfTI"));
}

#[test]
fn test_try_run_empty_bids_dir() {
    let mut app = App::new();
    app.try_run();
    assert!(!app.should_run);
    assert!(app.error_message.as_deref().unwrap().contains("BIDS directory"));
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.active_field, 0);
}

#[test]
fn test_try_run_slurm_no_account() {
    let mut app = App::new();
    app.form.bids_dir = "/tmp/bids".to_string();
    app.form.execution_mode = 1;
    app.try_run();
    assert!(!app.should_run);
    assert!(app.error_message.as_deref().unwrap().contains("SLURM account"));
    assert_eq!(app.active_tab, TAB_EXECUTION);
    assert_eq!(app.focused_field_name().as_deref(), Some("slurm_account"));
}

#[test]
fn test_try_run_slurm_with_account() {
    let mut app = App::new();
    app.form.bids_dir = "/tmp/bids".to_string();
    app.form.execution_mode = 1;
    app.form.slurm_account = "myacct".to_string();
    app.try_run();
    assert!(app.should_run);
}

// ========== Reset tests on various tabs ==========

#[test]
fn test_reset_input_mode_field() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 0;
    app.input_mode = InputMode::DicomToBids;
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.input_mode, InputMode::Bids);
}

#[test]
fn test_reset_bids_dir_field() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 1;
    app.form.bids_dir = "/something".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.form.bids_dir.is_empty());
}

#[test]
fn test_reset_output_dir_field() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 2;
    app.form.output_dir = "/out".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.form.output_dir.is_empty());
}

#[test]
fn test_reset_config_file_field() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 3;
    app.form.config_file = "cfg.toml".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.form.config_file.is_empty());
}

#[test]
fn test_reset_input_tab() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.input_mode = InputMode::DicomToBids;
    app.form.bids_dir = "/x".to_string();
    app.form.output_dir = "/y".to_string();
    app.handle_key(key(KeyCode::Char('R')));
    assert_eq!(app.input_mode, InputMode::Bids);
    assert!(app.form.bids_dir.is_empty());
    assert!(app.form.output_dir.is_empty());
}

#[test]
fn test_reset_nifti_input_dir() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 1;
    app.input_mode = InputMode::NIfTI;
    app.nifti_state.input_dir = "/data".to_string();
    app.nifti_state.magnitude_files.push(std::path::PathBuf::from("/a.nii"));
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.nifti_state.input_dir.is_empty());
    assert!(app.nifti_state.magnitude_files.is_empty());
}

#[test]
fn test_reset_nifti_output_dir() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 2;
    app.input_mode = InputMode::NIfTI;
    app.nifti_state.output_dir = "/out".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.nifti_state.output_dir.is_empty());
}

#[test]
fn test_reset_dicom_dir() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 1;
    app.input_mode = InputMode::DicomToBids;
    app.dicom_state.dicom_dir = "/dcm".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.dicom_state.dicom_dir.is_empty());
    assert!(app.dicom_state.scanned_dir.is_none());
}

#[test]
fn test_reset_dicom_output_dir() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 2;
    app.input_mode = InputMode::DicomToBids;
    app.dicom_state.output_dir = "/out".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.dicom_state.output_dir.is_empty());
}

#[test]
fn test_reset_supplementary_fields() {
    let mut app = App::new();
    app.active_tab = TAB_RECONSTRUCTION;
    app.form.settings.set("pipeline.do_swi", true);
    focus_on(&mut app, TAB_RECONSTRUCTION, "swi_strength");
    *app.field_text_mut("swi_strength").unwrap() = "999".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.field_text("swi_strength"), RunForm::default().settings.text("swi.strength"));
}

#[test]
fn test_reset_execution_fields() {
    let mut app = App::new();
    app.active_tab = TAB_EXECUTION;
    app.form.execution_mode = 1;
    focus_on(&mut app, TAB_EXECUTION, "slurm_mem");
    app.form.slurm_mem = "999".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.form.slurm_mem, "");
}

// ========== Other outputs: edit SWI fields ==========

#[test]
fn test_supplementary_edit_swi_hp_sigma() {
    let mut app = App::new();
    app.form.settings.set("pipeline.do_swi", true);
    // One row for the three axes.
    focus_on(&mut app, TAB_RECONSTRUCTION, "swi_hp_sigma");
    let sigma = |app: &App| super::super::super::command::config_from_app(app).unwrap().swi.hp_sigma;
    assert_eq!(sigma(&app), qsmxt_engine::SwiConfig::default().hp_sigma, "the default parses");
    app.handle_key(key(KeyCode::Enter));
    assert!(app.pipeline_state.editing);
    for _ in 0..20 { app.handle_key(key(KeyCode::Backspace)); }
    for c in "4, 4 0".chars() { app.handle_key(key(KeyCode::Char(c))); }
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.pipeline_state.editing);
    assert_eq!(sigma(&app), [4.0, 4.0, 0.0]);
    *app.field_text_mut("swi_hp_sigma").unwrap() = "3 3".into();
    assert!(super::super::super::command::config_from_app(&app).is_err(), "three values or none: two are refused");
}

// ─── Export → DICOM: the checklist of maps ───

/// The checklist lists the maps the settings make, all ticked; unticking one passes the
/// rest to --dicom-outputs; a map turned on later is ticked; the last one stays ticked.
#[test]
fn the_dicom_checklist_follows_what_the_settings_make() {
    let mut app = App::new();
    app.form.export_dicom = true;
    app.form.settings.set("pipeline.do_swi", true);
    let fields: Vec<&str> = app.page_rows(Page::Export).iter().filter_map(row_field).collect();
    assert!(["dicom_out_chimap", "dicom_out_swi", "dicom_out_minip"].iter().all(|f| fields.contains(f)), "{fields:?}");
    assert!(!fields.contains(&"dicom_out_t2starmap"), "T2* is not made: {fields:?}");
    let notes: Vec<String> = app.page_rows(Page::Export).iter()
        .filter_map(|r| match r { PipelineRow::Note { text } => Some(text.to_string()), _ => None }).collect();
    assert_eq!(notes, ["Maps to export"], "a heading and the maps made, nothing about the maps not made");
    assert!(!fields.contains(&"dicom_out_swi_phase"), "SWI on, its filtered phase off: {fields:?}");
    assert_eq!(app.dicom_outputs(), None, "all ticked: no --dicom-outputs");

    app.toggle_field("dicom_out_minip");
    assert!(!app.field_toggle("dicom_out_minip"));
    let s = |v: &[&str]| Some(v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    assert_eq!(app.dicom_outputs(), s(&["magnitude", "Chimap", "swi"]));
    // A map turned on later is exported too.
    app.form.settings.set("pipeline.do_r2starmap", true);
    assert_eq!(app.dicom_outputs(), s(&["magnitude", "Chimap", "swi", "R2starmap"]));
    // The last ticked map stays ticked.
    app.toggle_field("dicom_out_magnitude");
    app.toggle_field("dicom_out_swi");
    app.toggle_field("dicom_out_r2starmap");
    app.toggle_field("dicom_out_chimap");
    assert!(app.field_toggle("dicom_out_chimap"), "the last map is not unticked");
    assert!(app.error_message.as_deref().is_some_and(|e| e.contains("untick Export DICOM")));
    // `a` ticks them all; `r` resets one.
    goto_page(&mut app, Page::Export);
    app.handle_key(key(KeyCode::Char('a')));
    assert_eq!(app.dicom_outputs(), None);
    app.toggle_field("dicom_out_swi");
    app.reset_field("dicom_out_swi");
    assert!(app.field_toggle("dicom_out_swi"));
    // Only while exporting.
    app.toggle_field("dicom_out_swi");
    app.form.bids_dir = "/b".into();
    assert!(crate::tui::command::build_run_args(&app).unwrap().dicom_outputs.is_some());
    app.form.export_dicom = false;
    assert!(crate::tui::command::build_run_args(&app).unwrap().dicom_outputs.is_none());
}

/// The segmentation is listed when one is made, and the checklist says its SEG needs χ.
#[test]
fn the_segmentation_needs_a_map_to_be_drawn_on() {
    let mut app = App::new();
    app.form.export_dicom = true;
    let fields = |app: &App| app.page_rows(Page::Export).iter().filter_map(row_field).collect::<Vec<_>>();
    assert!(!fields(&app).contains(&"dicom_out_dseg"), "no segmentation, no SEG");
    app.form.settings.set("pipeline.do_segmentation", true);
    assert!(fields(&app).contains(&"dicom_out_dseg") && fields(&app).contains(&"dicom_out_magnitude"));
    let note = |app: &App| app.page_rows(Page::Export).iter().any(|r| matches!(r, PipelineRow::Note { text } if text.contains("needs a map to be drawn on")));
    assert!(!note(&app));
    // χ alone unticked: the magnitude is still there to draw it on.
    app.toggle_field("dicom_out_chimap");
    assert!(!note(&app), "the magnitude is reference enough");
    app.toggle_field("dicom_out_magnitude");
    assert!(note(&app), "no map left for the SEG");
    assert_eq!(app.dicom_outputs(), Some(vec!["dseg".to_string()]));
}

