use super::*;

// --- Unwrap ---

#[test]
fn test_unwrap_laplacian() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("unwrapped.nii");
    testutils::write_phase(&phase);
    testutils::write_mask(&mask);

    super::unwrap::execute(UnwrapCommand::Laplacian(UnwrapLaplacianArgs {
        common: common_unwrap(phase, mask, output.clone()),
    })).unwrap();
    assert!(output.exists());
}

/// `qsmxt unwrap medi-region-growing` unwraps one 3D phase in order of its magnitude, with no
/// mask, and writes the result; a magnitude on another grid is refused.
#[test]
fn test_unwrap_medi_region_growing() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let mag = dir.path().join("mag.nii");
    let output = dir.path().join("unwrapped.nii");
    testutils::write_phase(&phase);
    testutils::write_magnitude(&mag);
    super::unwrap::execute(UnwrapCommand::MediRegionGrowing(crate::cli::UnwrapMediRegionGrowingArgs {
        input: phase.clone(), magnitude: mag, output: output.clone(),
    })).unwrap();
    assert!(output.exists());
    let other = dir.path().join("other.nii");
    testutils::write_mismatched_volume(&other, 1.0);
    assert!(super::unwrap::execute(UnwrapCommand::MediRegionGrowing(crate::cli::UnwrapMediRegionGrowingArgs {
        input: phase, magnitude: other, output,
    })).is_err());
}

/// `qsmxt unwrap prelude` unwraps one 3D phase with its settings and writes the result.
#[test]
fn test_unwrap_prelude() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let mask = dir.path().join("mask.nii");
    let output = dir.path().join("unwrapped.nii");
    testutils::write_phase(&phase);
    testutils::write_mask(&mask);

    super::unwrap::execute(UnwrapCommand::Prelude(crate::cli::UnwrapPreludeArgs {
        common: common_unwrap(phase, mask, output.clone()),
        settings: SettingsOf::from_flags(&["--prelude-labelling", "volume", "--prelude-partitions", "6"]),
    })).unwrap();
    assert!(output.exists());
}

#[test]
fn test_unwrap_romeo() {
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("phase.nii");
    let mask = dir.path().join("mask.nii");
    let mag = dir.path().join("mag.nii");
    let output = dir.path().join("unwrapped.nii");
    testutils::write_phase(&phase);
    testutils::write_mask(&mask);
    testutils::write_magnitude(&mag);

    super::unwrap::execute(UnwrapCommand::Romeo(UnwrapRomeoArgs {
        common: common_unwrap(phase, mask, output.clone()),
        magnitude: Some(mag.clone()),
        settings: Default::default(),
    })).unwrap();
    assert!(output.exists());

    // A multi-echo setting cannot apply to one 3D phase, so it is refused rather than ignored.
    let err = super::unwrap::execute(UnwrapCommand::Romeo(UnwrapRomeoArgs {
        common: common_unwrap(dir.path().join("phase.nii"), dir.path().join("mask.nii"), output.clone()),
        magnitude: Some(mag),
        settings: SettingsOf::from_flags(&["--romeo-template", "2"]),
    })).unwrap_err();
    assert!(format!("{err}").contains("--romeo-template"), "{err}");
}
