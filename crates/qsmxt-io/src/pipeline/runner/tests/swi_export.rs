use crate::pipeline::runner::*;

const IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

fn key() -> crate::bids::entities::AcquisitionKey {
    crate::bids::entities::AcquisitionKey {
        subject: "01".into(), session: None, acquisition: None,
        reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
    }
}

fn run_for(key: crate::bids::entities::AcquisitionKey) -> crate::bids::discovery::QsmRun {
    crate::bids::discovery::QsmRun {
        key,
        coils: None,
        echoes: vec![crate::bids::discovery::EchoFiles {
            echo_number: 1,
            phase_json: PathBuf::from("p.json"),
            phase_nifti: PathBuf::from("p.nii"),
            magnitude_json: None,
            imaginary_nifti: None,
            magnitude_nifti: None,
        }],
        magnetic_field_strength: 3.0,
        echo_times: vec![0.004],
        b0_dir: None,
        dims: (0, 0, 0),
        has_magnitude: true,
        mese: None,
    }
}

fn meta_for(dims: (usize, usize, usize), affine: [f64; 16]) -> RunMetadata {
    RunMetadata {
        dims,
        voxel_size: qsm_core::geometry::voxel_sizes_from_affine(&affine),
        affine,
        n_echoes: 1,
        echo_times: vec![0.004],
        b0_direction: (0.0, 0.0, 1.0),
        field_strength: 3.0,
        has_magnitude: true,
        source_geometry: None, slice_geometry: None, orientation: None,
    }
}

/// Two echoes where the head moved 2 mm along x between them: the stage reports echo 1 as the
/// reference and echo 2 as moved, writes echo 2 corrected (closer to echo 1 than it came in),
/// and the shrunk mask. Off, or without magnitude, it does what it says. How well the shift
/// is recovered is QSM.rs's to test (`motion::a_known_transform_is_recovered_and_b0_follows_it`).
#[test]
fn motion_correction_stage() {
    let dims = (24, 24, 16);
    let (nx, ny, nz) = dims;
    let (cx, cy, cz) = ((nx - 1) as f64 / 2.0, (ny - 1) as f64 / 2.0, (nz - 1) as f64 / 2.0);
    let blob = |shift: f64| -> Vec<f64> {
        let mut v = vec![0.0; nx * ny * nz];
        for k in 0..nz { for j in 0..ny { for i in 0..nx {
            let (x, y, z) = (i as f64 - cx - shift, j as f64 - cy, k as f64 - cz);
            v[i + j * nx + k * nx * ny] = 100.0 * (-(x * x / 30.0 + y * y / 20.0 + z * z / 10.0)).exp()
                + 30.0 * (-((x - 3.0).powi(2) + (y + 2.0).powi(2) + z * z) / 5.0).exp();
        }}}
        v
    };
    let dir = tempfile::tempdir().unwrap();
    let k = key();
    let mut run = run_for(k.clone());
    run.echoes.push(run.echoes[0].clone());
    run.echo_times = vec![0.004, 0.008];
    let output = DerivativeOutputs::new(dir.path());
    let mut meta = meta_for(dims, IDENTITY);
    meta.n_echoes = 2;
    meta.echo_times = vec![0.004, 0.008];
    let mask_path = output.mask_path(&k);
    let vs = meta.voxel_size;
    for (e, shift) in [(1, 0.0), (2, 2.0)] {
        write_volume(&output.mag_path(&k, e), &blob(shift), dims, vs, &IDENTITY).unwrap();
        write_volume(&output.phase_scaled_path(&k, e), &vec![0.3 * e as f64; nx * ny * nz], dims, vs, &IDENTITY).unwrap();
    }
    let brain: Vec<f64> = blob(0.0).iter().map(|&v| (v > 5.0) as u8 as f64).collect();
    write_volume(&mask_path, &brain, dims, vs, &IDENTITY).unwrap();

    let mut config = PipelineConfig::default();
    run_engine_step(&run, &config, &output, &meta, Stage::Motion).unwrap();
    assert!(!output.motion_coverage_path(&k).exists(), "off: no coverage mask");

    config.motion_correction.enabled = true;
    run_engine_step(&run, &config, &output, &meta, Stage::Motion).unwrap();
    let shrunk = output.motion_coverage_path(&k);
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(output.motion_report_path(&k)).unwrap()).unwrap();
    let t = report["echoes"][1]["translation_mm"].as_f64().unwrap();
    assert!(t > 1.0, "echo 2 moved 2 mm and is reported at {t} mm: {report}");
    assert_eq!(report["echoes"][0]["translation_mm"].as_f64().unwrap(), 0.0);
    // Echo 2 is written corrected: nearer echo 1 inside the brain than the echo that came in.
    let (m1, m2) = (load_volume(&output.motion_mag_path(&k, 1)).unwrap(), load_volume(&output.motion_mag_path(&k, 2)).unwrap());
    let mask = load_mask(&shrunk).unwrap();
    let worst = |a: &[f64], b: &[f64]| a.iter().zip(b).zip(&mask).filter(|(_, &m)| m != 0)
        .map(|((a, b), _)| (a - b).abs()).fold(0.0, f64::max);
    assert!(worst(&m1, &m2) < worst(&m1, &blob(2.0)), "echo 2 was not corrected towards echo 1");

    // Off again: the coverage mask it left is not this run's.
    config.motion_correction.enabled = false;
    run_engine_step(&run, &config, &output, &meta, Stage::Motion).unwrap();
    assert!(!shrunk.exists(), "a stale coverage mask stays behind");

    // On for a run without magnitude: the plan's check warned, and the run goes ahead
    // without it (#397).
    config.motion_correction.enabled = true;
    let mut no_mag = run.clone();
    no_mag.has_magnitude = false;
    run_engine_step(&no_mag, &config, &output, &meta, Stage::Motion).unwrap();
    assert!(!shrunk.exists(), "skipped: no coverage mask");
}

/// Something with structure in every direction, so a projection is not trivially constant.
fn textured(dims: (usize, usize, usize)) -> Vec<f64> {
    let (nx, ny, nz) = dims;
    let mut data = vec![0.0; nx * ny * nz];
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let (x, y, z) = (i as f64, j as f64, k as f64);
                data[i + j * nx + k * nx * ny] =
                    100.0 + 20.0 * (x * 0.7).sin() + 15.0 * (y * 0.4).cos() + 10.0 * (z * 0.9).sin();
            }
        }
    }
    data
}

/// Run the SWI step end to end over synthetic inputs on the given grid.
fn run_swi_stage(
    dir: &Path,
    dims: (usize, usize, usize),
    affine: [f64; 16],
    window: usize,
) -> (crate::Result<()>, DerivativeOutputs, crate::bids::entities::AcquisitionKey) {
    let k = key();
    let run = run_for(k.clone());
    let output = DerivativeOutputs::new(dir);
    // Two echoes: CLEAR-SWI reads every one.
    let mut meta = meta_for(dims, affine);
    meta.n_echoes = 2;
    meta.echo_times = vec![0.004, 0.008];

    let mut config = PipelineConfig::default();
    config.swi.mip_window = window;

    let phase: Vec<f64> = textured(dims).iter().map(|v| (v * 0.01).sin()).collect();
    for e in 1..=meta.n_echoes {
        write_volume(&output.phase_scaled_path(&k, e), &phase, dims, meta.voxel_size, &affine).unwrap();
        write_volume(&output.mag_path(&k, e), &textured(dims), dims, meta.voxel_size, &affine).unwrap();
    }

    let result = run_engine_step(&run, &config, &output, &meta, Stage::Swi);
    (result, output, k)
}

/// The projection a viewer should see: a sliding minimum over `window` slices.
fn expected_mip(swi: &[f64], dims: (usize, usize, usize), window: usize) -> Vec<f64> {
    let (nx, ny, nz) = dims;
    let nxy = nx * ny;
    let mut out = Vec::with_capacity(nxy * (nz - window + 1));
    for k in 0..=(nz - window) {
        for v in 0..nxy {
            out.push((0..window).map(|w| swi[v + (k + w) * nxy]).fold(f64::INFINITY, f64::min));
        }
    }
    out
}

/// The whole export path, not just `create_mip`: a header-only load hid this (issue #211).
#[test]
fn minip_header_matches_its_payload() {
    let dir = tempfile::tempdir().unwrap();
    let dims = (12, 10, 32);
    let (result, output, k) = run_swi_stage(dir.path(), dims, IDENTITY, 7);
    result.unwrap();

    let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
    assert_eq!(mip.dims, (12, 10, 26));
    // Every voxel the header promises is really there.
    assert_eq!(mip.data.len(), 12 * 10 * 26);

    // The file on disk holds exactly the payload, not the full-volume length.
    let bytes = std::fs::metadata(output.swi_mip_path(&k)).unwrap().len();
    assert_eq!(bytes, 352 + (12 * 10 * 26 * 4) as u64);

    // And it is the projection of the SWI that was saved beside it, zeros included.
    let swi = qsm_core::io::read_nifti_file(&output.swi_path(&k)).unwrap();
    assert_eq!(swi.dims, dims);
    let expected = expected_mip(&swi.data, dims, 7);
    for (i, (got, want)) in mip.data.iter().zip(expected.iter()).enumerate() {
        assert!((got - want).abs() < 1e-3, "voxel {i}: {got} != {want}");
    }
}

#[test]
fn minip_origin_sits_at_the_slab_centre() {
    let dir = tempfile::tempdir().unwrap();
    let mut affine = IDENTITY;
    affine[10] = 3.0; // 3 mm slices
    affine[11] = -60.0;
    let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 24), affine, 7);
    result.unwrap();

    let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
    assert_eq!(mip.dims, (8, 8, 18));
    // Three 3 mm slices along k from the SWI origin.
    assert!((mip.affine[11] - (-51.0)).abs() < 1e-3, "{:?}", mip.affine);
    // The SWI itself keeps the acquisition geometry.
    let swi = qsm_core::io::read_nifti_file(&output.swi_path(&k)).unwrap();
    assert!((swi.affine[11] - (-60.0)).abs() < 1e-3, "{:?}", swi.affine);
}

#[test]
fn a_window_of_one_keeps_the_full_depth() {
    let dir = tempfile::tempdir().unwrap();
    let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 1);
    result.unwrap();
    let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
    let swi = qsm_core::io::read_nifti_file(&output.swi_path(&k)).unwrap();
    assert_eq!(mip.dims, (8, 8, 10));
    assert_eq!(mip.data, swi.data);
}

#[test]
fn a_window_of_the_full_depth_leaves_one_slice() {
    let dir = tempfile::tempdir().unwrap();
    let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 10);
    result.unwrap();
    let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
    assert_eq!(mip.dims, (8, 8, 1));
    assert_eq!(mip.data.len(), 64);
}

#[test]
fn a_window_deeper_than_the_volume_is_refused_before_any_work() {
    let dir = tempfile::tempdir().unwrap();
    let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 11);
    let err = result.unwrap_err().to_string();
    assert!(err.contains("deeper than the 10-slice volume"), "{err}");
    // create_mip would have returned an empty vector here; nothing must reach disk.
    assert!(!output.swi_mip_path(&k).exists());
    assert!(!output.swi_path(&k).exists());
}

#[test]
fn a_zero_window_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (result, _, _) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 0);
    let err = result.unwrap_err().to_string();
    assert!(err.contains("at least 1 slice"), "{err}");
}

/// An SWI cached when the stage used the first echo only must be recomputed with every echo.
#[test]
fn a_first_echo_swi_cache_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 16), IDENTITY, 7);
    result.unwrap();
    let d = PipelineConfig::default().swi;
    let first_echo_key = serde_json::json!({
        "scaling": d.scaling, "strength": d.strength, "hp_sigma": d.hp_sigma,
        "mip_window": 7, "mip_dims": [8, 8, 10],
    });
    let first_echo_hash = crate::pipeline::graph::step_params_hash(Some("clear-swi"), &first_echo_key);
    let state: PipelineState = serde_json::from_str(
        &std::fs::read_to_string(output.state_path(&k)).unwrap()).unwrap();
    assert_ne!(state.completed_steps.get("swi").unwrap().params_hash.as_deref(), Some(first_echo_hash.as_str()));
}

/// A minIP written with full-volume dimensions must not survive as a cache hit.
#[test]
fn a_stale_minip_cache_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let dims = (8, 8, 16);
    let (result, output, k) = run_swi_stage(dir.path(), dims, IDENTITY, 7);
    result.unwrap();

    // Replay the pre-fix parameter set: same window, no minIP dimensions recorded.
    let stale = serde_json::json!({
        "scaling": PipelineConfig::default().swi.scaling,
        "strength": PipelineConfig::default().swi.strength,
        "hp_sigma": PipelineConfig::default().swi.hp_sigma,
        "mip_window": 7,
    });
    let stale_hash = crate::pipeline::graph::step_params_hash(Some("clear-swi"), &stale);
    let mut state: PipelineState =
        serde_json::from_str(&std::fs::read_to_string(output.state_path(&k)).unwrap()).unwrap();
    assert_ne!(
        state.completed_steps.get("swi").unwrap().params_hash.as_deref(),
        Some(stale_hash.as_str()),
        "the fix must change the swi cache key so old minIPs are regenerated",
    );

    // With the old hash stored, the step is not treated as cached.
    state.completed_steps.get_mut("swi").unwrap().params_hash = Some(stale_hash);
    assert!(!state.is_step_cached_with_hash(
        "swi",
        Some(&crate::pipeline::graph::step_params_hash(
            Some("clear-swi"),
            &serde_json::json!({
                "scaling": PipelineConfig::default().swi.scaling,
                "strength": PipelineConfig::default().swi.strength,
                "hp_sigma": PipelineConfig::default().swi.hp_sigma,
                "mip_window": 7,
                "mip_dims": [8, 8, 10],
            }),
        )),
    ));
}
