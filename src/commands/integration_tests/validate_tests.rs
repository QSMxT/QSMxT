use super::*;

// --- Validate ---

#[test]
fn test_validate_single_echo() {
    let dir = tempfile::tempdir().unwrap();
    testutils::create_single_echo_bids(dir.path());
    super::validate::execute(ValidateArgs {
        bids_dir: dir.path().to_path_buf(), filter: Default::default(),
    }).unwrap();
}

#[test]
fn test_validate_multi_echo() {
    let dir = tempfile::tempdir().unwrap();
    testutils::create_multi_echo_bids(dir.path());
    super::validate::execute(ValidateArgs {
        bids_dir: dir.path().to_path_buf(), filter: Default::default(),
    }).unwrap();
}

/// No runs is an error (exit 1), so a script does not take it for success.
#[test]
fn test_validate_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let err = super::validate::execute(ValidateArgs {
        bids_dir: dir.path().to_path_buf(), filter: Default::default(),
    }).unwrap_err().to_string();
    assert!(err.contains("no QSM-compatible runs found"), "{err}");
}

#[test]
fn test_validate_multi_session() {
    let dir = tempfile::tempdir().unwrap();
    testutils::create_multi_session_bids(dir.path());
    super::validate::execute(ValidateArgs {
        bids_dir: dir.path().to_path_buf(), filter: Default::default(),
    }).unwrap();
}
