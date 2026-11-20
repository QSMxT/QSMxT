mod swi_export;
mod oblique_run;
mod real_imag_input;
mod reference_support;
mod chisep_support;

/// A copy is compressed as its name says, so other readers can open it (nibabel refuses
/// gzipped data under a `.nii` name).
#[test]
fn a_nifti_copy_is_compressed_as_its_name_says() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let gz = dir.path().join("a.nii.gz");
    let bytes = b"not really a NIfTI, the copy does not read it".to_vec();
    let mut e = flate2::write::GzEncoder::new(std::fs::File::create(&gz).unwrap(), flate2::Compression::default());
    e.write_all(&bytes).unwrap();
    e.finish().unwrap();
    let is_gz = |p: &std::path::Path| std::fs::read(p).unwrap().starts_with(&[0x1f, 0x8b]);

    let plain = dir.path().join("plain.nii");
    super::copy_nifti(&gz, &plain).unwrap();
    assert_eq!(std::fs::read(&plain).unwrap(), bytes);
    let zipped = dir.path().join("zipped.nii.gz");
    super::copy_nifti(&plain, &zipped).unwrap();
    assert!(is_gz(&zipped));
    let same = dir.path().join("same.nii.gz");
    super::copy_nifti(&gz, &same).unwrap();
    assert_eq!(std::fs::read(&same).unwrap(), std::fs::read(&gz).unwrap());
}

/// The two passes must not share a single output path. If they did, the second pass would
/// overwrite the first's local field, eroded mask or raw map — and the combination would be
/// one reconstruction blended with itself.
#[test]
fn the_two_passes_write_to_different_places() {
    use crate::bids::derivatives::DerivativeOutputs;
    use crate::pipeline::graph::PipelineState;
    use qsmxt_engine::plan::{Pass, Stage};
    use qsmxt_engine::store::Item;
    use std::path::PathBuf;

    let run = run_with_echoes(vec![(PathBuf::from("/in/sub-1/anat/p.nii"), None)]);
    let config = qsmxt_engine::PipelineConfig::default();
    let store = super::FileStore {
        run: &run, config: &config, output: DerivativeOutputs::new(std::path::Path::new("/out")),
        meta: None, state: PipelineState::new(&config, &run.key), state_path: PathBuf::from("/out/s.json"),
        started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(),
    metadata: None, working_grid_kept: false,
    };
    for (a, b) in [
        (Item::BrainMask, Item::ReliableMask),
        (Item::LocalField(Pass::Main), Item::LocalField(Pass::Reliable)),
        (Item::LocalFieldMask(Pass::Main), Item::LocalFieldMask(Pass::Reliable)),
        (Item::RawChi(Pass::Main), Item::RawChi(Pass::Reliable)),
    ] {
        assert_ne!(store.path(&a), store.path(&b), "the two passes share an output path");
    }

    // And their steps are cached separately, or one pass's cache hit would skip the other.
    for stage in [Stage::Mask, Stage::BackgroundRemoval, Stage::Inversion, Stage::Tgv, Stage::Qsmart] {
        assert_ne!(stage(Pass::Main).name(), stage(Pass::Reliable).name());
    }
    assert_eq!(Stage::Inversion(Pass::Main).name(), "invert", "the main pass keeps the unsuffixed step names");
}

/// The header a run starts from carries the first phase image's own orientation fields, so
/// N4 gets the image geometry ITK reads from that file (an oblique one here).
#[test]
fn the_header_carries_the_first_phase_images_orientation() {
    use crate::bids::derivatives::DerivativeOutputs;
    use crate::pipeline::graph::PipelineState;
    use qsmxt_engine::store::StageStore;

    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("p.nii");
    let (c, s) = (0.3f64.cos(), 0.3f64.sin());
    let affine = [1.2 * c, 0.0, -2.0 * s, -90.5, 0.0, 1.1, 0.0, -110.25, 1.2 * s, 0.0, 2.0 * c, -60.0, 0.0, 0.0, 0.0, 1.0];
    let bytes = qsm_core::io::save_nifti(&[0.0; 120], (6, 5, 4), (1.2, 1.1, 2.0), &affine).unwrap();
    std::fs::write(&phase, &bytes).unwrap();
    let run = run_with_echoes(vec![(phase.clone(), None)]);
    let config = qsmxt_engine::PipelineConfig::default();
    let mut store = super::FileStore {
        run: &run, config: &config, output: DerivativeOutputs::new(dir.path()),
        meta: None, state: PipelineState::new(&config, &run.key), state_path: dir.path().join("s.json"),
        started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(),
        metadata: None, working_grid_kept: false,
    };
    let header = store.header().unwrap();
    let orientation = header.orientation.expect("read from the file");
    assert_eq!(orientation, qsmxt_engine::scan::HeaderOrientation::read(&bytes).unwrap());
    let meta = qsmxt_engine::scan::RunMetadata {
        dims: header.dims, voxel_size: header.voxel_size, affine: header.affine, n_echoes: 1,
        echo_times: vec![0.004], b0_direction: (0.0, 0.0, 1.0), field_strength: 3.0,
        has_magnitude: false, source_geometry: None, slice_geometry: None, orientation: Some(orientation),
    };
    assert_eq!(qsmxt_engine::scan::itk_geometry(&meta).unwrap(), qsm_core::io::read_itk_geometry_file(&phase).unwrap());
}

/// `qsmxt run` keeps volumes in float32, except the magnitude echoes QSMxT computes before
/// masking (`Item::kept_in_double`), which a later step reads back exactly as computed, on the
/// working grid and on the acquired grid.
#[test]
fn computed_magnitude_echoes_are_kept_in_double_precision() {
    use crate::bids::derivatives::DerivativeOutputs;
    use crate::pipeline::graph::PipelineState;
    use qsmxt_engine::store::{Data, Item, StageStore};

    let dir = tempfile::tempdir().unwrap();
    let run = run_with_echoes(vec![(dir.path().join("p.nii"), Some(dir.path().join("m.nii")))]);
    let config = qsmxt_engine::PipelineConfig::default();
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let meta = qsmxt_engine::scan::RunMetadata {
        dims: (4, 3, 2), voxel_size: (1.0, 1.0, 1.0), affine, n_echoes: 1,
        echo_times: vec![0.004], b0_direction: (0.0, 0.0, 1.0), field_strength: 3.0,
        has_magnitude: true, source_geometry: None, slice_geometry: None, orientation: None,
    };
    let mut store = super::FileStore {
        run: &run, config: &config, output: DerivativeOutputs::new(dir.path()),
        meta: Some(meta), state: PipelineState::new(&config, &run.key), state_path: dir.path().join("s.json"),
        started: None, step: Some("tukey_window".into()), supplied_mask: None, extra_outputs: Vec::new(),
        metadata: None, working_grid_kept: false,
    };
    let data: Vec<f64> = (0..24).map(|i| 100.0 / 3.0 + i as f64 * 1e-10).collect();
    let single: Vec<f64> = data.iter().map(|&v| v as f32 as f64).collect();
    assert_ne!(single, data);
    let grid = qsmxt_engine::store::Geometry { dims: (4, 3, 2), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
    store.put(Item::Magnitude(1), Data::Real(&data)).unwrap();
    store.put(Item::WindowedMagnitude(1), Data::OnGrid(&data, &grid)).unwrap();
    store.put(Item::Phase(1), Data::Real(&data)).unwrap();
    assert_eq!(store.real(&Item::Magnitude(1)).unwrap(), data);
    assert_eq!(store.real(&Item::WindowedMagnitude(1)).unwrap(), data);
    assert_eq!(store.real(&Item::Phase(1)).unwrap(), single);
}

/// The BIDS name the engine gives each item ([`qsmxt_engine::store::Item::meta`]) is the one
/// its file is written under, and what it calls published is what lands among the run's
/// outputs rather than in a workflow directory.
#[test]
fn item_metadata_names_the_files_qsmxt_writes() {
    use crate::bids::derivatives::DerivativeOutputs;
    use crate::pipeline::graph::PipelineState;
    use qsmxt_engine::store::Item;
    use std::path::PathBuf;

    let run = run_with_echoes(vec![(PathBuf::from("/in/sub-1/anat/p.nii"), None)]);
    let config = qsmxt_engine::PipelineConfig::default();
    let store = super::FileStore {
        run: &run, config: &config, output: DerivativeOutputs::new(std::path::Path::new("/out")),
        meta: None, state: PipelineState::new(&config, &run.key), state_path: PathBuf::from("/out/s.json"),
        started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(),
        metadata: None, working_grid_kept: false,
    };
    for item in Item::one_of_each() {
        let meta = item.meta();
        let Some(bids) = meta.bids_name else { continue };
        let path = store.path(&item).unwrap_or_else(|| panic!("{item:?} has a BIDS name and no file"));
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let stem = name.split('.').next().unwrap();
        assert!(stem.ends_with(&format!("_{bids}")), "{item:?}: {name} is not named {bids}");
        assert_eq!(meta.published, !path.starts_with("/out/workflow"), "{item:?}: {}", path.display());
    }
}

/// Single-step and end-to-end algorithms produce a map over the brain mask, with no separate
/// background-removal stage to erode it.
#[test]
fn single_step_algorithms_skip_background_removal() {
    use qsmxt_engine::{PipelineConfig, QsmAlgorithm};
    let mut config = PipelineConfig::default();

    for alg in [QsmAlgorithm::Rts, QsmAlgorithm::Tkd, QsmAlgorithm::Ilsqr] {
        config.inversion.algorithm = alg;
        assert!(!qsmxt_engine::bridge::skips_bgremove(&config), "{alg} runs its own background removal stage");
    }
    for alg in [QsmAlgorithm::Nextqsm, QsmAlgorithm::Autoqsm, QsmAlgorithm::Iqsm,
                QsmAlgorithm::IqsmPlus, QsmAlgorithm::Tgv, QsmAlgorithm::Qsmart, QsmAlgorithm::Tfi] {
        config.inversion.algorithm = alg;
        assert!(qsmxt_engine::bridge::skips_bgremove(&config), "{alg} has no separate background removal");
    }

    // MEDI takes the local field with every SMV option.
    config.inversion.algorithm = QsmAlgorithm::Medi;
    for smv in [false, true] {
        config.inversion.medi.smv = smv;
        assert!(!qsmxt_engine::bridge::skips_bgremove(&config));
    }
}

/// A bring-your-own mask has no recipe to leave unfilled, so two-pass has nothing to build a
/// reliable mask from. Without this gate the run would reconstruct twice from the same mask
/// and "combine" a map with itself — twice the time, and an output indistinguishable from the
/// single-pass one.
#[test]
fn a_custom_mask_turns_two_pass_off() {
    use qsmxt_engine::PipelineConfig;
    let two_pass = |config: &PipelineConfig, run: &crate::bids::discovery::QsmRun| {
        qsmxt_engine::plan::plan(config, &crate::pipeline::requirements::scan_facts(run, config)).two_pass
    };

    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path();
    let anat = bids.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();
    let phase = anat.join("sub-1_echo-1_part-phase_MEGRE.nii");
    write_vol(&phase, (2, 2, 2));
    let run = run_with_echoes(vec![(phase, None)]);

    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    assert!(two_pass(&config, &run), "computed mask: two-pass applies");

    // Asking for a BYO mask that is actually there turns it off.
    let deriv_anat = bids.join("derivatives/bet/sub-1/anat");
    std::fs::create_dir_all(&deriv_anat).unwrap();
    write_vol(&deriv_anat.join("sub-1_MEGRE_mask.nii"), (2, 2, 2));
    config.masking.custom_mask_tool = Some("bet".to_string());
    assert!(!two_pass(&config, &run));

    // --use-custom-masks falls back to computing the mask when nothing matches, and then
    // there *is* a recipe — so two-pass applies after all.
    config.masking.custom_mask_tool = Some("nonexistent".to_string());
    assert!(two_pass(&config, &run), "a BYO mask that isn't there is not a BYO mask");

    // And it is off whenever QSM is, or whenever it was not asked for.
    config.masking.custom_mask_tool = None;
    config.pipeline.do_qsm = false;
    assert!(!two_pass(&config, &run));
    config.pipeline.do_qsm = true;
    config.masking.two_pass = false;
    assert!(!two_pass(&config, &run));
}

/// A mask a front end supplies for a run, written where `qsmxt_engine::supplied` says, is the
/// one `--use-custom-masks <tool>` finds for that run, over the subject's other runs' masks.
#[test]
fn a_supplied_mask_written_for_a_run_is_found_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path();
    let anat = bids.join("sub-1/ses-a/anat");
    std::fs::create_dir_all(&anat).unwrap();
    let mut runs = Vec::new();
    for r in ["1", "2"] {
        let phase = anat.join(format!("sub-1_ses-a_run-{r}_echo-1_part-phase_MEGRE.nii"));
        write_vol(&phase, (2, 2, 2));
        let mut run = run_with_echoes(vec![(phase, None)]);
        run.key.session = Some("a".into());
        run.key.run = Some(r.into());
        runs.push(run);
    }
    let write = |prefix: &str| {
        let path = bids.join(qsmxt_engine::supplied::supplied_mask_path("qsmbly", prefix).unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_vol(&path, (2, 2, 2));
        path
    };
    let second = write("sub-1_ses-a_run-2");
    assert_eq!(super::find_custom_mask(&runs[1], "qsmbly"), Some(second.clone()));
    let first = write("sub-1_ses-a_run-1");
    assert_eq!(super::find_custom_mask(&runs[0], "qsmbly"), Some(first));
    assert_eq!(super::find_custom_mask(&runs[1], "qsmbly"), Some(second.clone()), "its own, not the first by name");
    assert_eq!(super::find_custom_mask(&runs[1], "*"), Some(second));
}

/// A bring-your-own mask must be the brain mask, not a two-pass reliable mask that happens
/// to sit next to it. `desc-reliable` sorts before the plain `_mask` file, so the first
/// alphabetical match is the wrong one — and reconstructing inside a holey mask while calling
/// it the brain produces a plausible-looking map with no error anywhere.
#[test]
fn a_custom_mask_lookup_ignores_the_reliable_mask() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path();
    let anat = bids.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();
    let phase = anat.join("sub-1_echo-1_part-phase_MEGRE.nii");
    write_vol(&phase, (2, 2, 2));

    let deriv_anat = bids.join("derivatives/qsmxt/sub-1/anat");
    std::fs::create_dir_all(&deriv_anat).unwrap();
    let brain = deriv_anat.join("sub-1_MEGRE_mask.nii");
    let reliable = deriv_anat.join("sub-1_MEGRE_desc-reliable_mask.nii");
    write_vol(&brain, (2, 2, 2));
    write_vol(&reliable, (2, 2, 2));
    assert!(reliable < brain, "the reliable mask sorts first — that is the trap");

    let run = run_with_echoes(vec![(phase, None)]);
    assert_eq!(super::find_custom_mask(&run, "qsmxt"), Some(brain.clone()));
    assert_eq!(super::find_custom_mask(&run, "*"), Some(brain.clone()));

    // Only *our* reliable mask is excluded: `desc-brain`/`desc-bet` is how most tools name a
    // brain mask, and those must still be found — including when they are the only candidate.
    std::fs::remove_file(&brain).unwrap();
    let desc_brain = deriv_anat.join("sub-1_MEGRE_desc-brain_mask.nii");
    write_vol(&desc_brain, (2, 2, 2));
    assert_eq!(super::find_custom_mask(&run, "qsmxt"), Some(desc_brain));
}

#[test]
fn test_prefetch_weights_noop_and_download_bar() {
    // Classical algorithm ids aren't registry models → prefetch is a no-op (in both the
    // `dl` and non-`dl` builds), no network.
    assert!(super::prefetch_weights("rts", "test").is_ok());
    assert!(super::prefetch_weights("vsharp", "test").is_ok());
    // The download bar renders without touching the network (only exists in `dl` builds).
    #[cfg(feature = "dl")]
    {
        let pb = super::create_download_bar("test ↓ x.onnx", 1000);
        pb.set_position(500);
        pb.finish_and_clear();
    }
}

#[cfg(not(feature = "dl"))]
#[test]
fn test_prefetch_weights_errors_for_dl_without_dl_feature() {
    // Selecting a DL model in a non-DL build must fail with a clear message.
    let err = super::prefetch_weights("qsmnet", "test").unwrap_err();
    assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
}

#[cfg(not(feature = "dl"))]
#[test]
fn test_hd_bet_mask_needs_dl_feature() {
    // The mask stage prefetches every DL op's weights; HD-BET must hit the same clear error.
    let sections = qsmxt_engine::to_mask_sections(&qsmxt_engine::hd_bet_mask_sections(), &Default::default());
    let ids: Vec<&str> = sections.iter().flat_map(|s| s.all_ops()).filter_map(|op| op.dl_model_id()).collect();
    assert_eq!(ids, ["hd-bet"]);
    let err = super::prefetch_weights(ids[0], "test").unwrap_err();
    assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
}

#[cfg(not(feature = "dl"))]
#[test]
fn test_rs2_net_mask_needs_dl_feature() {
    // RS2-Net is a DL generator like HD-BET, so it must reach the same prefetch error
    // rather than failing later with something about ONNX.
    let sections = qsmxt_engine::to_mask_sections(&qsmxt_engine::rs2_net_mask_sections(), &Default::default());
    let ids: Vec<&str> = sections.iter().flat_map(|s| s.all_ops()).filter_map(|op| op.dl_model_id()).collect();
    assert_eq!(ids, ["rs2-net"]);
    let err = super::prefetch_weights(ids[0], "test").unwrap_err();
    assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
}

#[cfg(not(feature = "dl"))]
#[test]
fn test_mindgrab_mask_needs_dl_feature() {
    let sections = qsmxt_engine::to_mask_sections(&qsmxt_engine::mindgrab_mask_sections(), &Default::default());
    let ids: Vec<&str> = sections.iter().flat_map(|s| s.all_ops()).filter_map(|op| op.dl_model_id()).collect();
    assert_eq!(ids, ["mindgrab"]);
    let err = super::prefetch_weights(ids[0], "test").unwrap_err();
    assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
}

#[test]
fn test_find_custom_mask() {
    use crate::bids::discovery::{EchoFiles, QsmRun};
    use crate::bids::entities::AcquisitionKey;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let anat = root.join("sub-1").join("anat");
    std::fs::create_dir_all(&anat).unwrap();
    let phase = anat.join("sub-1_part-phase_MEGRE.nii");
    std::fs::write(&phase, b"x").unwrap();
    let bet_anat = root.join("derivatives").join("bet").join("sub-1").join("anat");
    std::fs::create_dir_all(&bet_anat).unwrap();
    let mask = bet_anat.join("sub-1_desc-bet_mask.nii");
    std::fs::write(&mask, b"x").unwrap();

    let run = QsmRun {
        key: AcquisitionKey {
            subject: "1".into(), session: None, acquisition: None,
            reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
        },
        coils: None,
        echoes: vec![EchoFiles {
            echo_number: 1, phase_nifti: phase.clone(), phase_json: phase.clone(),
            magnitude_nifti: None, magnitude_json: None, imaginary_nifti: None,
        }],
        magnetic_field_strength: 3.0, echo_times: vec![0.004], b0_dir: None,
        dims: (2, 2, 2), has_magnitude: false,
        mese: None,
    };
    assert_eq!(super::find_custom_mask(&run, "bet"), Some(mask.clone()));
    assert_eq!(super::find_custom_mask(&run, "*"), Some(mask.clone())); // first tool alphabetically
    assert_eq!(super::find_custom_mask(&run, "nonexistent"), None);     // falls back (None)
}

/// Build a `QsmRun` whose echoes point at the given (phase, magnitude) file pairs.
pub(super) fn run_with_echoes(echoes: Vec<(std::path::PathBuf, Option<std::path::PathBuf>)>) -> crate::bids::discovery::QsmRun {
    use crate::bids::discovery::{EchoFiles, QsmRun};
    use crate::bids::entities::AcquisitionKey;
    let n = echoes.len();
    QsmRun {
        key: AcquisitionKey {
            subject: "1".into(), session: None, acquisition: None,
            reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
        },
        coils: None,
        echoes: echoes.into_iter().enumerate().map(|(i, (phase, mag))| EchoFiles {
            echo_number: i as u32 + 1, phase_json: phase.clone(), phase_nifti: phase,
            magnitude_json: None, magnitude_nifti: mag, imaginary_nifti: None,
        }).collect(),
        magnetic_field_strength: 3.0,
        echo_times: (0..n).map(|i| 0.004 + i as f64 * 0.004).collect(),
        b0_dir: None,
        dims: (4, 4, 4), has_magnitude: true, mese: None,
    }
}

/// Write a NIfTI of the given dimensions filled with 1.0.
fn write_vol(path: &std::path::Path, dims: (usize, usize, usize)) {
    let data = vec![1.0f64; dims.0 * dims.1 * dims.2];
    qsm_core::io::save_nifti_to_file(path, &data, dims, (1.0, 1.0, 1.0), &[
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]).unwrap();
}

#[test]
fn test_validate_run_dims_accepts_matching_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let mut echoes = Vec::new();
    for e in 1..=2 {
        let phase = dir.path().join(format!("echo{}_phase.nii", e));
        let mag = dir.path().join(format!("echo{}_mag.nii", e));
        write_vol(&phase, (4, 4, 4));
        write_vol(&mag, (4, 4, 4));
        echoes.push((phase, Some(mag)));
    }
    let run = run_with_echoes(echoes);
    let reference = qsm_core::io::read_nifti_file(&run.echoes[0].phase_nifti).unwrap();
    super::validate_run_dims(&run, &reference).unwrap();
}

#[test]
fn test_validate_run_dims_rejects_mismatched_magnitude() {
    // A magnitude smaller than the phase used to reach qsm-core and panic with an
    // out-of-bounds index during inhomogeneity correction (issue #184).
    let dir = tempfile::tempdir().unwrap();
    let phase = dir.path().join("echo1_phase.nii");
    let mag = dir.path().join("echo1_mag.nii");
    write_vol(&phase, (4, 4, 4));
    write_vol(&mag, (4, 4, 3));
    let run = run_with_echoes(vec![(phase, Some(mag.clone()))]);
    let reference = qsm_core::io::read_nifti_file(&run.echoes[0].phase_nifti).unwrap();
    let err = super::validate_run_dims(&run, &reference).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("Dimension mismatch"), "got: {}", msg);
    assert!(msg.contains("magnitude"), "got: {}", msg);
    assert!(msg.contains(&mag.display().to_string()), "got: {}", msg);
    assert!(msg.contains("4x4x3"), "got: {}", msg);
    assert!(msg.contains("4x4x4"), "got: {}", msg);
}

#[test]
fn test_validate_run_dims_rejects_mismatched_later_echo() {
    let dir = tempfile::tempdir().unwrap();
    let phase1 = dir.path().join("echo1_phase.nii");
    let phase2 = dir.path().join("echo2_phase.nii");
    write_vol(&phase1, (4, 4, 4));
    write_vol(&phase2, (5, 4, 4));
    let run = run_with_echoes(vec![(phase1, None), (phase2.clone(), None)]);
    let reference = qsm_core::io::read_nifti_file(&run.echoes[0].phase_nifti).unwrap();
    let err = super::validate_run_dims(&run, &reference).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("echo 2 phase"), "got: {}", msg);
    assert!(msg.contains(&phase2.display().to_string()), "got: {}", msg);
}

// --- resolve_geometry: how an oblique acquisition is handled ---

/// A real UK Biobank SWI affine: 0.8 x 0.8 x 3 mm, header "Tra>Cor(-22.9)".
fn oblique_affine() -> [f64; 16] {
    [
        0.7976, -0.0273, -0.1075, -101.6,
        0.0141, 0.7358, -1.1647, -87.4,
        0.0370, 0.3092, 2.7626, -60.2,
        0.0, 0.0, 0.0, 1.0,
    ]
}

fn nifti_with(affine: [f64; 16], dims: (usize, usize, usize)) -> qsm_core::io::NiftiData {
    qsm_core::io::NiftiData {
        data: vec![0.0; dims.0 * dims.1 * dims.2],
        dims,
        voxel_size: qsm_core::geometry::voxel_sizes_from_affine(&affine),
        affine,
        scl_slope: 1.0,
        scl_inter: 0.0,
    }
}

fn run_for_geometry(b0_dir: Option<(f64, f64, f64)>) -> crate::bids::discovery::QsmRun {
    let mut run = run_with_echoes(vec![(std::path::PathBuf::from("p.nii"), None)]);
    run.b0_dir = b0_dir;
    run.echo_times = vec![0.004];
    run
}

#[test]
fn a_gapped_2d_acquisition_is_refused_unless_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("p.json");
    let mut run = run_for_geometry(None);
    run.echoes[0].phase_json = json.clone();
    let affine = [0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let phase = nifti_with(affine, (4, 4, 4));
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    let meta = super::resolve_run_geometry(&run, &phase, &cfg).unwrap();
    let write = |t: f64| std::fs::write(&json, format!(
        r#"{{"EchoTime": 0.004, "MagneticFieldStrength": 3, "SliceThickness": {t}, "MRAcquisitionType": "2D"}}"#)).unwrap();

    write(3.0); // contiguous: passed on to qsm-core
    assert_eq!(super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap(), Some((3.0, 2)));
    write(2.0); // 1 mm gap
    let err = super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap_err().to_string();
    assert!(err.contains("--ignore-slice-gap"), "{err}");
    cfg.pipeline.ignore_slice_gap = true;
    assert_eq!(super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap(), None);
    cfg.pipeline.ignore_slice_gap = false;
    cfg.pipeline.do_qsm = false; // nothing that needs a contiguous grid
    assert_eq!(super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap(), None);
    // And qsm-core refuses it too, given the geometry.
    let mut gapped = meta.clone();
    gapped.slice_geometry = Some((2.0, 2));
    assert!(qsmxt_engine::scan::scan_metadata(&gapped).require_contiguous_slices().is_err());
}

#[test]
fn geometry_axial_run_is_untouched() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.pipeline.axial_resampling = true;
    cfg.pipeline.obliquity_threshold = 10.0;
    let affine = [
        0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(affine, (4, 4, 4)), &cfg).unwrap();
    assert!(meta.source_geometry.is_none(), "an axial run must not be resampled");
    assert_eq!(meta.dims, (4, 4, 4));
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
}

#[test]
fn geometry_oblique_without_threshold_rotates_the_kernel() {
    let cfg = qsmxt_engine::PipelineConfig::default(); // threshold -1 (disabled)
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_none(), "resampling is off by default");
    assert_eq!(meta.dims, (16, 16, 8), "grid is left alone");
    // B0 comes from the affine rather than being assumed along z.
    let (bx, by, bz) = meta.b0_direction;
    assert!((bx - 0.0463).abs() < 1e-3 && (by - 0.3871).abs() < 1e-3 && (bz - 0.9209).abs() < 1e-3,
            "B0 should follow the affine, got ({bx}, {by}, {bz})");
}

#[test]
fn geometry_oblique_over_threshold_resamples_and_b0_becomes_z() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.pipeline.axial_resampling = true;
    cfg.pipeline.obliquity_threshold = 10.0; // obliquity here is ~32.5 degrees
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    let (src_dims, _) = meta.source_geometry.expect("should resample");
    assert_eq!(src_dims, (16, 16, 8), "source dims are recorded for scale_phase");
    assert!(meta.dims.0 >= 16 && meta.dims.1 >= 16 && meta.dims.2 >= 8,
            "the cardinal box is at least as large, got {:?}", meta.dims);
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0), "resampled grid puts B0 along z");
    assert!(qsm_core::geometry::obliquity_from_affine(&meta.affine) < 1e-9);
    // The stages (masking among them) run on the resampled grid, so that is the affine
    // qsm-core is given — not the oblique one the files were acquired on.
    assert_eq!(qsmxt_engine::scan::scan_metadata(&meta).affine, Some(meta.affine));
    assert_ne!(meta.affine, oblique_affine());
}

/// A crop of the working grid keeps its world position: voxel (0,0,0) of the box is voxel
/// `origin` of the grid, so the box's affine maps it to the same point.
#[test]
fn scan_metadata_in_a_box_moves_the_affine_to_the_box() {
    let cfg = qsmxt_engine::PipelineConfig::default();
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    let cb = qsm_core::crop::CropBox { origin: (2, -1, 3), dims: (8, 10, 4), full_dims: meta.dims };
    let m = qsmxt_engine::scan::scan_metadata_in(&meta, &cb);
    assert_eq!(m.dims, (8, 10, 4));
    let (a, b) = (meta.affine, m.affine.unwrap());
    let world = |aff: &[f64; 16], v: [f64; 3]| -> [f64; 3] {
        std::array::from_fn(|r| aff[4 * r] * v[0] + aff[4 * r + 1] * v[1] + aff[4 * r + 2] * v[2] + aff[4 * r + 3])
    };
    for (vb, vg) in [([0.0, 0.0, 0.0], [2.0, -1.0, 3.0]), ([1.0, 2.0, 3.0], [3.0, 1.0, 6.0])] {
        let (pb, pg) = (world(&b, vb), world(&a, vg));
        assert!(pb.iter().zip(pg).all(|(x, y)| (x - y).abs() < 1e-9), "{pb:?} vs {pg:?}");
    }
    assert_eq!(qsmxt_engine::scan::scan_metadata(&meta).affine, Some(meta.affine));
}

#[test]
fn geometry_threshold_above_obliquity_does_not_resample() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.pipeline.axial_resampling = true;
    cfg.pipeline.obliquity_threshold = 45.0; // above this run's ~32.5 degrees
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_none());
    assert!(meta.b0_direction.2 < 0.99, "still uses the affine direction");
}

#[test]
fn geometry_sidecar_b0_wins_over_resampling() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.pipeline.axial_resampling = true;
    cfg.pipeline.obliquity_threshold = 10.0;
    let run = run_for_geometry(Some((0.0, 0.5, 0.866)));
    let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_none(), "an explicit B0_dir means the grid is left alone");
    assert_eq!(meta.b0_direction, (0.0, 0.5, 0.866));
}

// --- reconciling a MESE's geometry with the grid the run reconstructs on ---











#[test]
fn robust_fit_report_sits_next_to_the_field_map() {
    let p = std::path::Path::new("/w/unwrap/sub-1_field-ppm.nii");
    assert_eq!(super::robust_fit_report_path(p), std::path::PathBuf::from("/w/unwrap/sub-1_field-ppm_robustfit.json"));
    let gz = std::path::Path::new("/w/unwrap/sub-1_field-ppm.nii.gz");
    assert_eq!(super::robust_fit_report_path(gz), std::path::PathBuf::from("/w/unwrap/sub-1_field-ppm_robustfit.json"));
}



/// A MESE attached to a run, on whatever grid is given.
fn mese_on(dims: (usize, usize, usize), affine: [f64; 16]) -> crate::bids::discovery::MeseRun {
    crate::bids::discovery::MeseRun {
        key: crate::bids::entities::AcquisitionKey {
            subject: "1".to_string(), session: None, acquisition: None,
            reconstruction: None, inversion: None, run: None,
            suffix: "MESE".to_string(),
        },
        magnitude_niftis: vec![std::path::PathBuf::from("mese.nii")],
        echo_times: vec![0.01],
        dims, affine,
    }
}

/// Used to assert the opposite: a MESE made the oblique run keep its grid, because resampling
/// the GRE alone would have stranded the MESE. The MESE is now resampled onto whatever grid
/// this picks, so it no longer has a vote.
#[test]
fn geometry_mese_run_is_resampled_like_any_other() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.pipeline.axial_resampling = true;
    cfg.pipeline.obliquity_threshold = 10.0;
    let mut run = run_for_geometry(None);
    run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
    let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_some(),
            "a MESE no longer suppresses a resample the threshold asked for");
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
}

// --- an algorithm that only understands axial data outranks everything else ---

/// A deep-learning inversion: no B0 direction to set, an axial prior baked into the weights.
fn cfg_with_axial_only_inversion() -> qsmxt_engine::PipelineConfig {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.inversion.algorithm = qsmxt_engine::QsmAlgorithm::Qsmnet;
    cfg
}

#[test]
fn geometry_axial_only_algorithm_resamples_without_a_threshold() {
    let cfg = cfg_with_axial_only_inversion(); // threshold stays -1 (disabled)
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_some(),
            "a network that assumes B0 is +z must be given axial data, threshold or not");
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
}

#[test]
fn geometry_axial_only_algorithm_outranks_a_sidecar_b0() {
    let cfg = cfg_with_axial_only_inversion();
    let run = run_for_geometry(Some((0.0, 0.5, 0.866)));
    let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_some(),
            "the sidecar describes the acquisition, but the network cannot be told about it");
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
}

#[test]
fn geometry_axial_only_algorithm_leaves_axial_data_alone() {
    let cfg = cfg_with_axial_only_inversion();
    let affine = [
        0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(affine, (4, 4, 4)), &cfg).unwrap();
    assert!(meta.source_geometry.is_none(), "already axial: nothing to fix");
    assert_eq!(meta.dims, (4, 4, 4));
}

/// Used to be refused outright: resampling would have stranded the MESE, not resampling would
/// have fed the network oblique data. The third option it skipped — resample both — is what
/// happens now, so the refusal is gone and the network still gets axial data.
#[test]
fn geometry_axial_only_algorithm_with_a_mese_resamples_both() {
    let cfg = cfg_with_axial_only_inversion();
    let mut run = run_for_geometry(None);
    run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
    let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg)
        .expect("resampling both is defensible, so this no longer refuses");
    assert!(meta.source_geometry.is_some(), "the network must still be given axial data");
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    assert!(qsm_core::geometry::obliquity_from_affine(&meta.affine) < 1e-9);
}

/// The MESE being on a grid of its own — the usual case for a 2D spin-echo companion to a 3D
/// GRE — changes nothing about the decision here. It used to mean the MESE was dropped
/// entirely at load time, so R2' was never computed.
#[test]
fn geometry_mese_on_its_own_grid_does_not_change_the_decision() {
    let cfg = cfg_with_axial_only_inversion();
    let mut run = run_for_geometry(None);
    // Half the matrix, four times the slice thickness, and an axial affine of its own.
    run.mese = Some(mese_on((8, 8, 2), [
        1.6, 0.0, 0.0, 0.0, 0.0, 1.6, 0.0, 0.0, 0.0, 0.0, 12.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]));
    let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg)
        .expect("the MESE's own grid is reconciled at load time, not here");
    assert!(meta.source_geometry.is_some());
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
}

/// #243: R2PRIMEnet was outside the axial-only gate, so an oblique run whose other stages all
/// take B0 as a parameter was reconstructed on the acquired grid and the network was handed it.
#[test]
fn geometry_r2primenet_on_an_oblique_grid_forces_a_resample() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.pipeline.do_chi_separation = true;
    // χ-sep-iLSQR takes B0 as a parameter, so nothing else in this run wants axial data.
    cfg.separation.algorithm = qsmxt_engine::SeparationAlgorithm::ChiSepIlsqr;
    cfg = cfg.normalised().0;
    // No MESE, so `--r2prime-strategy auto` predicts R2′ with R2PRIMEnet.
    let run = run_for_geometry(None);
    let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_some(),
            "R2PRIMEnet assumes B0 is +z and takes no direction, so the data must move");
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
}

/// The same config with a MESE attached: `auto` measures R2′ = R2* − R2 instead, no network
/// runs, and the run keeps its grid. A resample here would cost an interpolation for nothing.
#[test]
fn geometry_a_mese_removes_the_r2primenet_reason_to_resample() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.pipeline.do_chi_separation = true;
    cfg.separation.algorithm = qsmxt_engine::SeparationAlgorithm::ChiSepIlsqr;
    cfg = cfg.normalised().0;
    let mut run = run_for_geometry(None);
    run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
    let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_none(),
            "nothing in this run assumes B0 is +z, so the kernel is rotated instead");
    assert!(meta.b0_direction.2 < 0.99, "B0 comes from the affine");
}

/// iQFM is iQSM's local-field head, and `map_bf_alg` has no iQFM to map to — it answers for a
/// placeholder. The gate asks about iQSM instead, so an oblique run is resampled.
#[test]
fn geometry_iqfm_on_an_oblique_grid_forces_a_resample() {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.bg_removal.algorithm = qsmxt_engine::BfAlgorithm::Iqfm;
    let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
    assert!(meta.source_geometry.is_some(),
            "iQFM learned the same axial prior as iQSM and takes no B0 direction");
    assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
}

#[test]
fn geometry_classical_algorithm_needs_no_resampling() {
    let cfg = qsmxt_engine::PipelineConfig::default(); // iLSQR by default
    assert!(qsmxt_engine::bridge::axial_only_algorithms(&cfg, Default::default()).is_empty(),
            "the default pipeline takes B0 as a parameter throughout");
}
