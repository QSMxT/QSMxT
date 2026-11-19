//! The Input tab: the example dataset, the BIDS run tree, NIfTI and DICOM, the input mode.

use super::*;

// ─── Example-dataset input mode ───

/// An app parked in Example mode with focus on the picker.
fn example_app() -> App {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.input_mode = InputMode::Example;
    app.active_field = App::INPUT_IO_FIELDS;
    app
}

#[test]
fn example_state_starts_on_the_button_with_the_default_ticked() {
    let app = example_app();
    assert_eq!(app.example_state.cursor, ExampleState::BUTTON);
    let picked = app.example_state.selected_examples();
    assert_eq!(picked.len(), 1);
    assert_eq!(picked[0].id, crate::example::DEFAULT_ID);
    assert_eq!(app.example_state.selected_bytes(), picked[0].bytes);
}

#[test]
fn example_picker_navigates_and_ticks() {
    let mut app = example_app();

    // Down from the button walks into the list, one row at a time; Space ticks a second
    // acquisition.
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.example_state.acquisition(), Some(1));
    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.example_state.selected_examples().len(), 2);

    // Space again unticks it.
    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.example_state.selected_examples().len(), 1);

    // Down cannot walk past the last acquisition.
    for _ in 0..=ExampleState::last_row() {
        app.handle_key(key(KeyCode::Down));
    }
    assert_eq!(app.example_state.cursor, ExampleState::last_row());
}

/// From the IO fields, Down reaches the button, and the next Down the first acquisition.
#[test]
fn example_picker_is_walked_in_the_order_it_is_drawn() {
    let mut app = example_app();
    app.active_field = App::INPUT_IO_FIELDS - 1;
    app.handle_key(key(KeyCode::Down));
    assert_eq!((app.active_field, app.example_state.cursor), (App::INPUT_IO_FIELDS, ExampleState::BUTTON));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.example_state.acquisition(), Some(0));
    assert_eq!(crate::example::EXAMPLES[0].id, "prisma-bridge-run1");
}

#[test]
fn example_picker_returns_to_the_io_fields_from_the_top() {
    let mut app = example_app();
    app.example_state.cursor = ExampleState::BUTTON;
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
}

#[test]
fn example_download_refuses_an_empty_selection() {
    let mut app = example_app();
    app.example_state.selected = vec![false; crate::example::EXAMPLES.len()];
    app.handle_key(key(KeyCode::Enter)); // cursor starts on the button
    assert_eq!(app.example_state.status, ConvertStatus::Idle);
    assert!(
        app.error_message.as_deref().unwrap_or("").contains("Select at least one"),
        "{:?}",
        app.error_message
    );
}

#[test]
fn example_reset_is_blocked_while_a_download_runs() {
    // Resetting mid-fetch would orphan the worker thread mid-write.
    let mut app = example_app();
    app.example_state.status = ConvertStatus::Converting;
    app.example_state.output_dir = "somewhere".to_string();
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.example_state.output_dir, "somewhere", "state must survive");
    assert!(app.error_message.is_some());
}

#[test]
fn example_reset_clears_state_when_idle() {
    let mut app = example_app();
    app.example_state.output_dir = "somewhere".to_string();
    app.example_state.cursor = 1;
    app.handle_key(key(KeyCode::Char('r')));
    assert!(app.example_state.output_dir.is_empty());
    assert_eq!(app.example_state.cursor, ExampleState::BUTTON);
}

#[test]
fn example_dataset_dir_defaults_and_expands_tilde() {
    let mut app = example_app();
    assert_eq!(app.example_state.resolved_dir(), std::path::PathBuf::from(EXAMPLE_DEFAULT_DIR));

    app.example_state.output_dir = "  ~/data/example  ".to_string();
    if let Some(home) = std::env::var_os("HOME") {
        assert_eq!(
            app.example_state.resolved_dir(),
            std::path::Path::new(&home).join("data/example")
        );
    }

    app.example_state.output_dir = "/abs/path".to_string();
    assert_eq!(app.example_state.resolved_dir(), std::path::PathBuf::from("/abs/path"));
}

#[test]
fn example_poll_is_inert_without_a_worker() {
    let mut app = example_app();
    assert!(app.example_state.poll().is_none());
    assert!(app.example_state.progress().is_none());
}

#[test]
fn example_mode_blocks_a_run_until_the_dataset_exists() {
    let mut app = example_app();
    app.form.bids_dir = "/some/dir".to_string();
    app.try_run();
    assert!(!app.should_run);
    assert!(
        app.error_message.as_deref().unwrap_or("").contains("Download the example dataset"),
        "{:?}",
        app.error_message
    );
}

#[test]
fn example_mode_edits_the_dataset_dir_not_the_bids_dir() {
    // Field 1 in Example mode is the dataset directory; the pipeline's own output
    // directory stays on field 2, unlike the DICOM/NIfTI modes.
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.input_mode = InputMode::Example;
    app.active_field = 1;
    app.handle_key(key(KeyCode::Enter));
    for c in "mydata".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    assert_eq!(app.example_state.output_dir, "mydata");
    assert!(app.form.bids_dir.is_empty(), "the BIDS dir must not be touched");
}

#[test]
fn input_mode_cycles_through_all_four_modes() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 0;
    let seen: Vec<InputMode> = (0..4)
        .map(|_| {
            step(&mut app, 1);
            app.input_mode
        })
        .collect();
    assert_eq!(
        seen,
        vec![InputMode::NIfTI, InputMode::DicomToBids, InputMode::Example, InputMode::Bids]
    );
}

// --- Filter tree tests ---

#[test]
fn test_filter_state_default() {
    let fs = super::FilterTreeState::default();
    assert!(fs.tree.is_none());
    assert_eq!(fs.include_pattern, "*");
    assert!(fs.exclude_pattern.is_empty());
    assert_eq!(fs.focus, super::FilterFocus::Include);
}

#[test]
fn test_filter_scan_with_bids() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_multi_echo_bids(dir.path());
    let mut fs = super::FilterTreeState::default();
    fs.maybe_rescan(dir.path().to_str().unwrap());
    assert!(fs.tree.is_some());
    let tree = fs.tree.as_ref().unwrap();
    assert_eq!(tree.subjects.len(), 1);
    assert_eq!(tree.subjects[0].name, "1");
}

#[test]
fn test_filter_scan_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let mut fs = super::FilterTreeState::default();
    fs.maybe_rescan(dir.path().to_str().unwrap());
    // Tree may be Some with empty subjects, or None
    if let Some(ref tree) = fs.tree {
        assert!(tree.subjects.is_empty());
    }
}

#[test]
fn test_filter_scan_caches() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut fs = super::FilterTreeState::default();
    let path = dir.path().to_str().unwrap();
    fs.maybe_rescan(path);
    assert!(fs.tree.is_some());
    // Second call should not rescan
    fs.maybe_rescan(path);
    assert_eq!(fs.scanned_bids_dir.as_deref(), Some(path));
}

#[test]
fn test_filter_navigation() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut fs = super::FilterTreeState::default();
    fs.maybe_rescan(dir.path().to_str().unwrap());

    assert_eq!(fs.focus, super::FilterFocus::Include);
    fs.focus_next(); // -> Exclude
    assert_eq!(fs.focus, super::FilterFocus::Exclude);
    fs.focus_next(); // -> orientation group pattern
    assert_eq!(fs.focus, super::FilterFocus::OrientationGroup);
    // With no pattern set, its settings rows are skipped entirely — the single-orientation
    // majority should not have to tab past a feature they are not using.
    fs.focus_next(); // -> tree node 0 (subject)
    assert!(matches!(fs.focus, super::FilterFocus::TreeNode(0)));
    fs.focus_next(); // -> tree node 1 (run)
    fs.focus_next(); // -> NumEchoes
    assert_eq!(fs.focus, super::FilterFocus::NumEchoes);
    fs.focus_next(); // stays at NumEchoes
    assert_eq!(fs.focus, super::FilterFocus::NumEchoes);
    fs.focus_prev(); // back up
    assert!(matches!(fs.focus, super::FilterFocus::TreeNode(_)));
}

#[test]
fn test_filter_toggle_run() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut fs = super::FilterTreeState::default();
    fs.maybe_rescan(dir.path().to_str().unwrap());

    // Navigate to the run leaf
    fs.focus_next(); // orientation group pattern
    fs.focus_next(); // subject
    fs.focus_next(); // run leaf
    let tree = fs.tree.as_ref().unwrap();
    assert!(tree.subjects[0].runs[0].selected);
    fs.toggle_focused();
    let tree = fs.tree.as_ref().unwrap();
    assert!(!tree.subjects[0].runs[0].selected);
}

/// ←/→ on a run walks it through no group, each group of its session as a member and as
/// the reference, and a new group; the groups are kept as a `runs:` pattern. A typed
/// pattern's groups are where it starts.
#[test]
fn arrows_give_runs_their_orientation_groups() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_multi_orientation_bids(dir.path());
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.form.bids_dir = dir.path().to_string_lossy().into_owned();
    app.filter_state.maybe_rescan(&app.form.bids_dir.clone());
    app.active_field = App::INPUT_IO_FIELDS;
    let rows = app.filter_state.visible_rows();
    let runs: Vec<usize> = (0..rows.len()).filter(|&i| matches!(rows[i], TreeRow::Run { .. })).collect();
    let press = |app: &mut App, i: usize, code: KeyCode| {
        app.filter_state.focus = FilterFocus::TreeNode(i);
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    };
    let slot = |app: &mut App, i: usize| {
        app.filter_state.focus = FilterFocus::TreeNode(i);
        let (slots, now) = app.filter_state.orientation_slots("").unwrap();
        slots[now]
    };
    // → puts the first run in a new group; the next run's → joins it, then is its reference.
    press(&mut app, runs[0], KeyCode::Right);
    assert_eq!(slot(&mut app, runs[0]), OrientationSlot::New);
    press(&mut app, runs[1], KeyCode::Right);
    assert_eq!(slot(&mut app, runs[1]), OrientationSlot::Member(0));
    let preview = app.filter_state.orientation_preview("");
    assert_eq!(preview.groups.len(), 1);
    press(&mut app, runs[1], KeyCode::Right);
    assert_eq!(slot(&mut app, runs[1]), OrientationSlot::Reference(0));
    assert_eq!(app.filter_state.orientation_groups("")[0][0], app.filter_state.orientation_preview("").order[runs_order(&app, runs[1])]);
    // → again: a second group of its own; ← back.
    press(&mut app, runs[1], KeyCode::Right);
    assert_eq!(slot(&mut app, runs[1]), OrientationSlot::New);
    press(&mut app, runs[1], KeyCode::Left);
    assert_eq!(slot(&mut app, runs[1]), OrientationSlot::Reference(0));
    // ← all the way: out of every group, and with nothing grouped, off.
    for run in [runs[1], runs[0]] {
        for _ in 0..4 {
            press(&mut app, run, KeyCode::Left);
        }
        assert_eq!(slot(&mut app, run), OrientationSlot::None);
    }
    assert!(app.filter_state.orientation_group.is_empty(), "{}", app.filter_state.orientation_group);

    // From a typed pattern, the arrows start with what it grouped.
    app.filter_state.orientation_group = "*acq-dir*".into();
    let grouped = app.filter_state.orientation_preview("").member_of.len();
    let member = runs.iter().copied().find(|&i| slot(&mut app, i) != OrientationSlot::None).unwrap();
    press(&mut app, member, KeyCode::Left);
    assert!(app.filter_state.orientation_group.starts_with("runs:"));
    let listed: usize = app.filter_state.orientation_groups("").iter().map(Vec::len).sum();
    assert_eq!(listed, grouped);
    assert!(crate::bids::orientation::parse_pattern(&app.filter_state.orientation_group).unwrap().is_some());

    // ←/→ on a subject or session still collapses and expands it.
    let subject = (0..rows.len()).find(|&i| matches!(rows[i], TreeRow::Subject(_))).unwrap();
    let before = app.filter_state.visible_rows().len();
    press(&mut app, subject, KeyCode::Left);
    assert!(app.filter_state.visible_rows().len() < before);
}

/// The position of the run at tree row `i` among the selected runs.
fn runs_order(app: &App, i: usize) -> usize {
    let rows = app.filter_state.visible_rows();
    let TreeRow::Run { sub, ses, run } = rows[i] else { panic!() };
    let tree = app.filter_state.tree.as_ref().unwrap();
    let key = match ses {
        Some(sei) => &tree.subjects[sub].sessions[sei].runs[run].key_string,
        None => &tree.subjects[sub].runs[run].key_string,
    };
    app.filter_state.orientation_preview("").order.iter().position(|k| k == key).unwrap()
}

/// The Input tab keeps only the pattern: its settings are on QSM → COSMOS/STI, so
/// focus goes from the pattern straight to the tree, set or not.
#[test]
fn the_input_tab_has_only_the_orientation_pattern() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut fs = super::FilterTreeState::default();
    fs.maybe_rescan(dir.path().to_str().unwrap());
    for pattern in ["*acq-dir*", ""] {
        fs.orientation_group = pattern.into();
        fs.focus = super::FilterFocus::OrientationGroup;
        fs.focus_next();
        assert!(matches!(fs.focus, super::FilterFocus::TreeNode(0)));
        fs.focus_prev();
        assert_eq!(fs.focus, super::FilterFocus::OrientationGroup);
    }
}

/// The preview is the safety feature: it has to report a bad pattern rather than
/// silently matching nothing.
#[test]
fn orientation_preview_reports_a_bad_pattern() {
    let mut fs = super::FilterTreeState {
        orientation_group: "chunk".into(),
        ..Default::default()
    };
    let preview = fs.orientation_preview("");
    assert!(preview.error.unwrap().contains("chunk"));

    fs.orientation_group = "re:(a)(b)".into();
    assert!(fs.orientation_preview("").error.is_some());
}

#[test]
fn orientation_preview_is_empty_when_off() {
    let fs = super::FilterTreeState::default();
    let preview = fs.orientation_preview("");
    assert!(preview.error.is_none());
    assert!(preview.groups.is_empty());
}

#[test]
fn the_orientation_grouping_is_none_until_a_pattern_is_set() {
    let mut fs = super::FilterTreeState::default();
    assert_eq!(fs.orientation_group_by(), None);
    fs.orientation_group = "  acq  ".into();
    assert_eq!(fs.orientation_group_by().as_deref(), Some("acq"), "the pattern is trimmed");
}

#[test]
fn test_filter_select_all_none() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_multi_echo_bids(dir.path());
    let mut fs = super::FilterTreeState::default();
    fs.maybe_rescan(dir.path().to_str().unwrap());

    let tree = fs.tree.as_mut().unwrap();
    tree.set_all(false);
    assert_eq!(tree.selected_runs(), 0);
    tree.set_all(true);
    assert_eq!(tree.selected_runs(), tree.total_runs());
}



#[test]
fn test_filter_collapse() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut fs = super::FilterTreeState::default();
    fs.maybe_rescan(dir.path().to_str().unwrap());

    let rows_before = fs.visible_rows().len();
    fs.focus = super::FilterFocus::TreeNode(0); // subject node
    fs.toggle_collapse();
    let rows_after = fs.visible_rows().len();
    assert!(rows_after < rows_before, "Collapsing should hide children");

    // Expand again
    fs.toggle_collapse();
    assert_eq!(fs.visible_rows().len(), rows_before);
}

// --- Right arrow on non-select does nothing ---

#[test]
fn test_left_right_on_text_field_does_nothing() {
    let mut app = App::new();
    app.active_field = 0; // Text field
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Right));
    // No crash, no state change
    assert_eq!(app.active_field, 0);
}

// --- Cursor doesn't go below Config File when no BIDS tree ---

#[test]
fn test_input_tab_no_tree_cursor_stays() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 3; // Config File (field 0=mode, 1=bids_dir, 2=output_dir, 3=config)
    // No BIDS tree loaded
    assert!(app.filter_state.tree.is_none());
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.active_field, 3); // Should stay on Config File
}

// --- NIfTI mode tests ---

#[test]
fn test_nifti_mode_switching() {
    let mut app = App::new();
    assert_eq!(app.input_mode, InputMode::Bids);
    // Field 0 is the mode selector; Right arrow cycles Bids -> NIfTI
    app.active_field = 0;
    step(&mut app, 1);
    assert_eq!(app.input_mode, InputMode::NIfTI);
}

#[test]
fn test_nifti_focus_navigation_into_nifti_section() {
    let mut app = App::new();
    app.active_field = 0;
    step(&mut app, 1); // switch to NIfTI mode
    assert_eq!(app.input_mode, InputMode::NIfTI);
    // Navigate down past IO fields (0..3) into NIfTI section
    app.active_field = 3;
    app.handle_key(key(KeyCode::Down));
    // NIfTI mode always has content below, so active_field should be INPUT_IO_FIELDS (4)
    assert_eq!(app.active_field, App::INPUT_IO_FIELDS);
    // Default NIfTI focus is AddMagnitude
    assert_eq!(app.nifti_state.focus, NiftiFocus::AddMagnitude);
}

#[test]
fn test_nifti_state_focus_next_and_prev() {
    let mut ns = NiftiState::default();
    assert_eq!(ns.focus, NiftiFocus::AddMagnitude);
    // focus_next cycles through items (no mag/phase files, so: AddMagnitude -> AddPhase -> EchoTimes -> ...)
    ns.focus_next();
    assert_eq!(ns.focus, NiftiFocus::AddPhase);
    ns.focus_next();
    assert_eq!(ns.focus, NiftiFocus::EchoTimes);
    ns.focus_next();
    assert_eq!(ns.focus, NiftiFocus::FieldStrength);
    ns.focus_next();
    assert_eq!(ns.focus, NiftiFocus::B0Direction);
    ns.focus_next();
    assert_eq!(ns.focus, NiftiFocus::ConvertButton);

    // focus_prev returns true when it moves
    assert!(ns.focus_prev());
    assert_eq!(ns.focus, NiftiFocus::B0Direction);

    // Go back to the top
    ns.focus = NiftiFocus::AddMagnitude;
    // focus_prev at the top returns false
    assert!(!ns.focus_prev());
}

#[test]
fn test_nifti_editing_echo_times() {
    let mut app = App::new();
    app.active_field = 0;
    step(&mut app, 1); // switch to NIfTI
    // Navigate into NIfTI section
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.focus = NiftiFocus::EchoTimes;
    // Press Enter to start editing
    app.handle_key(key(KeyCode::Enter));
    assert!(app.nifti_state.editing);
    // Type characters
    app.handle_key(key(KeyCode::Char('4')));
    app.handle_key(key(KeyCode::Char(',')));
    app.handle_key(key(KeyCode::Char(' ')));
    app.handle_key(key(KeyCode::Char('8')));
    assert_eq!(app.nifti_state.echo_times, "4, 8");
}

#[test]
fn test_nifti_editing_field_strength() {
    let mut app = App::new();
    app.active_field = 0;
    step(&mut app, 1); // switch to NIfTI
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.focus = NiftiFocus::FieldStrength;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.nifti_state.editing);
    app.handle_key(key(KeyCode::Char('3')));
    app.handle_key(key(KeyCode::Char('.')));
    app.handle_key(key(KeyCode::Char('0')));
    // Escape exits editing
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.nifti_state.editing);
    assert_eq!(app.nifti_state.field_strength, "3.0");
}

#[test]
fn test_nifti_editing_b0_direction() {
    let mut app = App::new();
    app.active_field = 0;
    step(&mut app, 1); // switch to NIfTI
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.focus = NiftiFocus::B0Direction;
    // Clear default value first
    app.nifti_state.b0_direction.clear();
    app.handle_key(key(KeyCode::Enter));
    assert!(app.nifti_state.editing);
    app.handle_key(key(KeyCode::Char('1')));
    app.handle_key(key(KeyCode::Char(',')));
    app.handle_key(key(KeyCode::Char('0')));
    app.handle_key(key(KeyCode::Char(',')));
    app.handle_key(key(KeyCode::Char('0')));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.nifti_state.b0_direction, "1,0,0");
}

#[test]
fn test_dicom_mode_switching() {
    let mut app = App::new();
    assert_eq!(app.input_mode, InputMode::Bids);
    app.active_field = 0;
    // Right twice: Bids -> NIfTI -> DicomToBids
    step(&mut app, 1);
    step(&mut app, 1);
    assert_eq!(app.input_mode, InputMode::DicomToBids);
}

#[test]
fn test_dicom_state_default_focus() {
    let ds = DicomConvertState::default();
    assert_eq!(ds.focus, DicomFocus::Series(0));
}

#[test]
fn test_nifti_scan_input_directory_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let mut ns = NiftiState {
        input_dir: dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };
    ns.scan_input_directory();
    assert!(ns.magnitude_files.is_empty());
    assert!(ns.phase_files.is_empty());
}

#[test]
fn test_nifti_add_magnitude_editing() {
    let mut app = App::new();
    app.active_field = 0;
    step(&mut app, 1); // switch to NIfTI
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.focus = NiftiFocus::AddMagnitude;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.nifti_state.editing);
    assert_eq!(
        app.nifti_state.adding_to,
        Some(crate::nifti::convert::NiftiPartType::Magnitude)
    );
}

#[test]
fn test_nifti_add_phase_editing() {
    let mut app = App::new();
    app.active_field = 0;
    step(&mut app, 1); // switch to NIfTI
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.focus = NiftiFocus::AddPhase;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.nifti_state.editing);
    assert_eq!(
        app.nifti_state.adding_to,
        Some(crate::nifti::convert::NiftiPartType::Phase)
    );
}

#[test]
fn test_nifti_focus_wrapping_at_convert_button() {
    let mut ns = NiftiState {
        focus: NiftiFocus::ConvertButton,
        ..Default::default()
    };
    // focus_next at ConvertButton should stay at ConvertButton
    ns.focus_next();
    assert_eq!(ns.focus, NiftiFocus::ConvertButton);
}

// ========== Filter tree key handler tests ==========

#[test]
fn test_filter_enter_include_editing() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS; // in filter tree area
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.focus = FilterFocus::Include;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.filter_state.include_editing);
}

#[test]
fn test_filter_enter_exclude_editing() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.focus = FilterFocus::Exclude;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.filter_state.exclude_editing);
}

#[test]
fn test_filter_type_in_include() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.focus = FilterFocus::Include;
    app.filter_state.include_editing = true;
    app.filter_state.include_pattern.clear();
    app.filter_state.include_cursor = 0;
    app.handle_key(key(KeyCode::Char('s')));
    app.handle_key(key(KeyCode::Char('u')));
    app.handle_key(key(KeyCode::Char('b')));
    assert_eq!(app.filter_state.include_pattern, "sub");
    assert_eq!(app.filter_state.include_cursor, 3);
}

#[test]
fn test_filter_esc_stops_include_editing() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.include_editing = true;
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.filter_state.include_editing);
    assert!(!app.should_quit);
}

#[test]
fn test_filter_enter_stops_include_editing_and_applies() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.include_editing = true;
    app.filter_state.include_pattern = "sub-1*".to_string();
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.filter_state.include_editing);
}

#[test]
fn test_filter_type_in_exclude() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.focus = FilterFocus::Exclude;
    app.filter_state.exclude_editing = true;
    app.filter_state.exclude_cursor = 0;
    app.handle_key(key(KeyCode::Char('x')));
    assert_eq!(app.filter_state.exclude_pattern, "x");
}

#[test]
fn test_filter_text_backspace() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.include_editing = true;
    app.filter_state.include_pattern = "abc".to_string();
    app.filter_state.include_cursor = 3;
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(app.filter_state.include_pattern, "ab");
    assert_eq!(app.filter_state.include_cursor, 2);
}

#[test]
fn test_filter_text_delete() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.include_editing = true;
    app.filter_state.include_pattern = "abc".to_string();
    app.filter_state.include_cursor = 0;
    app.handle_key(key(KeyCode::Delete));
    assert_eq!(app.filter_state.include_pattern, "bc");
}

#[test]
fn test_filter_text_home_end() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.include_editing = true;
    app.filter_state.include_pattern = "hello".to_string();
    app.filter_state.include_cursor = 3;
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.filter_state.include_cursor, 0);
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.filter_state.include_cursor, 5);
}

#[test]
fn test_filter_text_left_right() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.include_editing = true;
    app.filter_state.include_pattern = "abc".to_string();
    app.filter_state.include_cursor = 2;
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.filter_state.include_cursor, 1);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.filter_state.include_cursor, 2);
}

#[test]
fn test_filter_num_echoes_editing() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.focus = FilterFocus::NumEchoes;
    app.handle_key(key(KeyCode::Enter));
    assert!(app.filter_state.num_echoes_editing);
    app.handle_key(key(KeyCode::Char('2')));
    assert_eq!(app.filter_state.num_echoes, "2");
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(app.filter_state.num_echoes, "");
    app.handle_key(key(KeyCode::Char('4')));
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.filter_state.num_echoes_cursor, 0);
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.filter_state.num_echoes_cursor, 1);
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.filter_state.num_echoes_cursor, 0);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.filter_state.num_echoes_cursor, 1);
    // Delete at cursor
    app.handle_key(key(KeyCode::Home));
    app.handle_key(key(KeyCode::Delete));
    assert_eq!(app.filter_state.num_echoes, "");
    // Enter exits editing
    app.handle_key(key(KeyCode::Enter));
    assert!(!app.filter_state.num_echoes_editing);
}

#[test]
fn test_filter_select_all_via_key() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut app = App::new();
    app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.focus = FilterFocus::TreeNode(0);
    // Select none
    app.handle_key(key(KeyCode::Char('n')));
    assert!(app.filter_state.manual_override);
    let tree = app.filter_state.tree.as_ref().unwrap();
    assert_eq!(tree.selected_runs(), 0);
    // Select all
    app.handle_key(key(KeyCode::Char('a')));
    let tree = app.filter_state.tree.as_ref().unwrap();
    assert_eq!(tree.selected_runs(), tree.total_runs());
}

#[test]
fn test_filter_space_toggles_node() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut app = App::new();
    app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    // Navigate to a run leaf
    app.filter_state.focus = FilterFocus::Exclude;
    app.filter_state.focus_next(); // TreeNode(0) subject
    app.filter_state.focus_next(); // TreeNode(1) run
    let tree = app.filter_state.tree.as_ref().unwrap();
    let was_selected = tree.subjects[0].runs[0].selected;
    app.handle_key(key(KeyCode::Char(' ')));
    let tree = app.filter_state.tree.as_ref().unwrap();
    assert_ne!(tree.subjects[0].runs[0].selected, was_selected);
    assert!(app.filter_state.manual_override);
}

#[test]
fn test_filter_left_right_collapse() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut app = App::new();
    app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.focus = FilterFocus::TreeNode(0); // subject
    let rows_before = app.filter_state.visible_rows().len();
    app.handle_key(key(KeyCode::Left));
    let rows_after = app.filter_state.visible_rows().len();
    assert!(rows_after < rows_before);
    // Expand with Right
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.filter_state.visible_rows().len(), rows_before);
}

#[test]
fn test_filter_up_at_include_goes_to_io() {
    let dir = tempfile::tempdir().unwrap();
    crate::testutils::create_single_echo_bids(dir.path());
    let mut app = App::new();
    app.filter_state.maybe_rescan(dir.path().to_str().unwrap());
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.focus = FilterFocus::Include;
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
}

#[test]
fn test_filter_tab_switch() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.focus = FilterFocus::Exclude;
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, 1);
}

#[test]
fn test_filter_f5() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.filter_state.tree = Some(crate::bids::discovery::BidsTree { subjects: vec![] });
    app.filter_state.focus = FilterFocus::Include;
    app.form.bids_dir = "/tmp/bids".to_string();
    app.handle_key(key(KeyCode::F(5)));
    assert!(app.should_run);
}

// ========== NIfTI handler tests (delete, reorder) ==========

#[test]
fn test_nifti_delete_magnitude_file() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.magnitude_files = vec![
        std::path::PathBuf::from("/a.nii"),
        std::path::PathBuf::from("/b.nii"),
    ];
    app.nifti_state.focus = NiftiFocus::MagFile(0);
    app.handle_key(key(KeyCode::Delete));
    assert_eq!(app.nifti_state.magnitude_files.len(), 1);
    assert_eq!(app.nifti_state.magnitude_files[0], std::path::PathBuf::from("/b.nii"));
}

#[test]
fn test_nifti_delete_last_magnitude_resets_focus() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.magnitude_files = vec![std::path::PathBuf::from("/a.nii")];
    app.nifti_state.focus = NiftiFocus::MagFile(0);
    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.nifti_state.magnitude_files.is_empty());
    assert_eq!(app.nifti_state.focus, NiftiFocus::AddMagnitude);
}

#[test]
fn test_nifti_delete_phase_file() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.phase_files = vec![
        std::path::PathBuf::from("/p1.nii"),
        std::path::PathBuf::from("/p2.nii"),
    ];
    app.nifti_state.focus = NiftiFocus::PhaseFile(1);
    app.handle_key(key(KeyCode::Delete));
    assert_eq!(app.nifti_state.phase_files.len(), 1);
    // Focus should clamp to 0 since we deleted index 1
    assert_eq!(app.nifti_state.focus, NiftiFocus::PhaseFile(0));
}

#[test]
fn test_nifti_delete_last_phase_resets_focus() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.phase_files = vec![std::path::PathBuf::from("/p.nii")];
    app.nifti_state.focus = NiftiFocus::PhaseFile(0);
    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.nifti_state.phase_files.is_empty());
    assert_eq!(app.nifti_state.focus, NiftiFocus::AddPhase);
}

#[test]
fn test_nifti_reorder_magnitude_down() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.magnitude_files = vec![
        std::path::PathBuf::from("/a.nii"),
        std::path::PathBuf::from("/b.nii"),
    ];
    app.nifti_state.focus = NiftiFocus::MagFile(0);
    app.handle_key(key(KeyCode::Char('J'))); // Shift+J = move down
    assert_eq!(app.nifti_state.magnitude_files[0], std::path::PathBuf::from("/b.nii"));
    assert_eq!(app.nifti_state.magnitude_files[1], std::path::PathBuf::from("/a.nii"));
    assert_eq!(app.nifti_state.focus, NiftiFocus::MagFile(1));
}

#[test]
fn test_nifti_reorder_magnitude_up() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.magnitude_files = vec![
        std::path::PathBuf::from("/a.nii"),
        std::path::PathBuf::from("/b.nii"),
    ];
    app.nifti_state.focus = NiftiFocus::MagFile(1);
    app.handle_key(key(KeyCode::Char('K'))); // Shift+K = move up
    assert_eq!(app.nifti_state.magnitude_files[0], std::path::PathBuf::from("/b.nii"));
    assert_eq!(app.nifti_state.magnitude_files[1], std::path::PathBuf::from("/a.nii"));
    assert_eq!(app.nifti_state.focus, NiftiFocus::MagFile(0));
}

#[test]
fn test_nifti_reorder_phase_down() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.phase_files = vec![
        std::path::PathBuf::from("/p1.nii"),
        std::path::PathBuf::from("/p2.nii"),
    ];
    app.nifti_state.focus = NiftiFocus::PhaseFile(0);
    app.handle_key(key(KeyCode::Char('J')));
    assert_eq!(app.nifti_state.phase_files[0], std::path::PathBuf::from("/p2.nii"));
    assert_eq!(app.nifti_state.focus, NiftiFocus::PhaseFile(1));
}

#[test]
fn test_nifti_reorder_phase_up() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.phase_files = vec![
        std::path::PathBuf::from("/p1.nii"),
        std::path::PathBuf::from("/p2.nii"),
    ];
    app.nifti_state.focus = NiftiFocus::PhaseFile(1);
    app.handle_key(key(KeyCode::Char('K')));
    assert_eq!(app.nifti_state.phase_files[0], std::path::PathBuf::from("/p2.nii"));
    assert_eq!(app.nifti_state.focus, NiftiFocus::PhaseFile(0));
}

#[test]
fn test_nifti_up_at_top_goes_to_io() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.focus = NiftiFocus::AddMagnitude;
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
}

#[test]
fn test_nifti_tab_switch() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, 1);
}

#[test]
fn test_nifti_quit() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_nifti_editing_backspace_home_end_left_right() {
    let mut app = App::new();
    app.input_mode = InputMode::NIfTI;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.nifti_state.focus = NiftiFocus::EchoTimes;
    app.nifti_state.echo_times = "12, 24".to_string();
    app.handle_key(key(KeyCode::Enter));
    assert!(app.nifti_state.editing);
    // cursor at end = 6
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.nifti_state.cursor, 0);
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.nifti_state.cursor, 6);
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.nifti_state.cursor, 5);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.nifti_state.cursor, 6);
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(app.nifti_state.echo_times, "12, 2");
}

// ========== DICOM tab handler tests ==========

#[test]
fn test_dicom_tab_navigate_up_to_io() {
    let mut app = App::new();
    app.input_mode = InputMode::DicomToBids;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.dicom_state.focus = DicomFocus::Series(0);
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.active_field, App::INPUT_IO_FIELDS - 1);
}

#[test]
fn test_dicom_tab_switch() {
    let mut app = App::new();
    app.input_mode = InputMode::DicomToBids;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.active_tab, 1);
}

#[test]
fn test_dicom_tab_quit() {
    let mut app = App::new();
    app.input_mode = InputMode::DicomToBids;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_dicom_focus_next_prev_no_session() {
    let mut ds = DicomConvertState::default();
    // No session, so focus_next from Series(0) goes to the keep-UIDs toggle, then the button
    ds.focus_next();
    assert_eq!(ds.focus, DicomFocus::KeepUids);
    ds.focus_next();
    assert_eq!(ds.focus, DicomFocus::ConvertButton);
    // Back up to the toggle; with no session there is no series above it
    ds.focus_prev();
    assert_eq!(ds.focus, DicomFocus::KeepUids);
    ds.focus_prev();
    assert_eq!(ds.focus, DicomFocus::KeepUids);
}

#[test]
fn test_dicom_navigate_down() {
    let mut app = App::new();
    app.input_mode = InputMode::DicomToBids;
    app.active_tab = TAB_INPUT;
    app.active_field = App::INPUT_IO_FIELDS;
    // No session, so ↓ goes to the keep-UIDs toggle, which Space flips, then the button
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.dicom_state.focus, DicomFocus::KeepUids);
    assert!(!app.dicom_state.keep_uids, "off by default: a fully de-identified dataset");
    app.handle_key(key(KeyCode::Char(' ')));
    assert!(app.dicom_state.keep_uids);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.dicom_state.focus, DicomFocus::ConvertButton);
}

// ========== IO select (input mode picker) ==========

/// The input mode is chosen from its picker (Enter); ←/→ neither cycle it nor open it.
#[test]
fn test_left_right_leave_the_io_mode_alone() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 0;
    assert_eq!(app.input_mode, InputMode::Bids);
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.input_mode, InputMode::Bids, "not cycled");
    assert!(app.algo_modal.is_none());
}

#[test]
fn test_enter_on_io_mode_opens_modal() {
    let mut app = App::new();
    app.active_tab = TAB_INPUT;
    app.active_field = 0;
    assert_eq!(app.input_mode, InputMode::Bids);
    // Enter opens the input-mode modal; picking NIfTI applies it.
    app.handle_key(key(KeyCode::Enter));
    assert!(app.algo_modal.is_some());
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.input_mode, InputMode::NIfTI);
    // Esc closes it with nothing changed.
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Esc));
    assert!(app.algo_modal.is_none());
    assert_eq!(app.input_mode, InputMode::NIfTI);
}

// ========== NIfTI: focusable_items with files ==========

#[test]
fn test_nifti_focusable_items_with_files() {
    let ns = NiftiState {
        magnitude_files: vec![
            std::path::PathBuf::from("/a.nii"),
            std::path::PathBuf::from("/b.nii"),
        ],
        phase_files: vec![
            std::path::PathBuf::from("/p.nii"),
        ],
        ..Default::default()
    };
    let items = ns.focusable_items();
    // AddMagnitude, MagFile(0), MagFile(1), AddPhase, PhaseFile(0),
    // EchoTimes, FieldStrength, B0Direction, ConvertButton = 9
    assert_eq!(items.len(), 9);
}

// ========== get_include_exclude tests ==========

#[test]
fn test_filter_get_include_exclude_pattern_mode() {
    let fs = FilterTreeState {
        include_pattern: "sub-1*".to_string(),
        exclude_pattern: "sub-2*".to_string(),
        ..Default::default()
    };
    let (inc, exc) = fs.get_include_exclude();
    assert_eq!(inc, Some(vec!["sub-1*".to_string()]));
    assert_eq!(exc, Some(vec!["sub-2*".to_string()]));
}

#[test]
fn test_filter_get_include_exclude_star_only() {
    let fs = FilterTreeState::default(); // include_pattern = "*"
    let (inc, exc) = fs.get_include_exclude();
    assert!(inc.is_none());
    assert!(exc.is_none());
}
