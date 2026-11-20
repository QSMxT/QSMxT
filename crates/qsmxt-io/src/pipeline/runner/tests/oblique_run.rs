//! End-to-end runs of an oblique acquisition resampled to axial (issue #223).

use crate::pipeline::runner::*;
use crate::bids::entities::AcquisitionKey;

/// As small as the grid logic allows: the obliquity is in the affine, not the matrix size.
/// Each axis differs, so a swapped axis shows, and ten slices leave room for the SMWI minIP.
const DIMS: (usize, usize, usize) = (12, 14, 10);

fn key(acq: &str) -> AcquisitionKey {
    AcquisitionKey {
        subject: "1".into(), session: None, acquisition: Some(acq.into()),
        reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
    }
}

/// A small multi-echo GRE of a sphere, 1 mm isotropic and tilted 30 degrees about x.
fn oblique_run(dir: &Path, acq: &str, n_echoes: usize) -> QsmRun {
    let (nx, ny, nz) = DIMS;
    let (cx, cy, cz) = ((nx / 2) as f64, (ny / 2) as f64, (nz / 2) as f64);
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let affine = [
        1.0, 0.0, 0.0, -cx,
        0.0, c, -s, -cy,
        0.0, s, c, -cz,
        0.0, 0.0, 0.0, 1.0,
    ];
    let mut echoes = Vec::new();
    for e in 0..n_echoes {
        let (mut mag, mut phase) = (vec![0.0; nx * ny * nz], vec![0.0; nx * ny * nz]);
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let v = i + j * nx + k * nx * ny;
                    let (x, y, z) = (i as f64 - cx, j as f64 - cy, k as f64 - cz);
                    let r = (x * x + y * y + z * z).sqrt();
                    mag[v] = if r < 4.5 { 100.0 * (-0.02 * (e + 1) as f64).exp() + x } else { 1.0 };
                    phase[v] = (0.05 * (e + 1) as f64 * (x + 0.5 * y)).sin() * 3.0;
                }
            }
        }
        let p = dir.join(format!("acq-{acq}_echo{}_phase.nii", e + 1));
        let m = dir.join(format!("acq-{acq}_echo{}_mag.nii", e + 1));
        io::save_nifti_to_file(&p, &phase, DIMS, (1.0, 1.0, 1.0), &affine).unwrap();
        io::save_nifti_to_file(&m, &mag, DIMS, (1.0, 1.0, 1.0), &affine).unwrap();
        echoes.push((p, Some(m)));
    }
    let mut run = super::run_with_echoes(echoes);
    run.key = key(acq);
    run.dims = DIMS;
    run
}

/// The cheapest pipeline that still has every stage QSMxT wires: TKD for the inversion,
/// Laplacian unwrapping, an Otsu threshold of the magnitude for the mask, and V-SHARP with
/// small kernels. These tests are about grids, caching, outputs and provenance, which QSMxT
/// owns; the algorithms are QSM.rs's to test. The few that switch to QSMART, because its
/// step's own wiring is under test, get it cut down: two unpadded iLSQR iterations, one
/// Frangi scale and the smallest closing radii.
fn fast_config() -> PipelineConfig {
    use qsmxt_engine::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
    let mut config = PipelineConfig::default();
    config.inversion.algorithm = QsmAlgorithm::Tkd;
    config.field_mapping.unwrapping_algorithm = qsmxt_engine::UnwrappingAlgorithm::Laplacian;
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    config.bg_removal.vsharp.max_radius = 3.0;
    let q = &mut config.inversion.qsmart;
    q.ilsqr_max_iter = 2;
    q.ilsqr_pad_mm = 0.0;
    q.vasc_sphere_radius = 1;
    (q.frangi_scale_min, q.frangi_scale_max) = (1.0, 1.0);
    config
}

/// Masking for QSMART that differs from the default, as `qsmxt run` gives QSMART a mask of its
/// own (BET on the magnitude, eroded): a thresholded, eroded magnitude, without BET's cost.
fn qsmart_mask_sections() -> Vec<qsmxt_engine::masking::MaskSection> {
    use qsmxt_engine::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
    vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![MaskOp::Erode { iterations: 1 }],
    }]
}

fn resampling_config() -> PipelineConfig {
    let mut config = fast_config();
    config.pipeline.axial_resampling = true;
    config.pipeline.obliquity_threshold = 5.0;
    config
}

fn voxels(path: &Path) -> usize {
    io::read_nifti_file(path).unwrap().data.len()
}

const ACQUIRED: usize = DIMS.0 * DIMS.1 * DIMS.2;

/// Each run's methods text, record and planned graph (Mermaid and SVG) go in its workflow
/// directory. Once the runs are done, the dataset's methods and graph come from every current
/// record in the output directory: `--use-custom-masks` with no mask found for any run is said
/// as done ("where available" no longer applies), and no run differs. A run that decided
/// otherwise than the rest gets its methods text and graph in `changes/`. Processing one run
/// again (`--include`) keeps the others in the dataset; a run processed with other settings is
/// not one of its runs; `changes/` goes when no run differs.
#[test]
fn the_datasets_methods_come_from_its_runs_and_a_run_that_differs_is_kept_in_changes() {
    use qsmxt_engine::record::{Choice, Decision, Topic};
    let dir = tempfile::tempdir().unwrap();
    let runs: Vec<QsmRun> = ["a", "b", "c", "d"].iter().map(|acq| oblique_run(dir.path(), acq, 2)).collect();
    let mut config = fast_config();
    config.masking.custom_mask_tool = Some("nothing-here".into());
    let derivatives = dir.path().join("out");
    let output = DerivativeOutputs::new(&derivatives);
    fs_err::create_dir_all(&derivatives).unwrap();
    crate::dataset::write_dataset_files(&derivatives, dir.path(), &config, &runs[..3]).unwrap();
    assert!(output.dataset_graph_path().with_extension("svg").exists());
    for run in &runs[..3] {
        run_pipeline_cached(run, &config, &output, true, &|_| {}).unwrap();
        let key = &run.key;
        for path in [output.run_methods_path(key), output.run_record_path(key), output.run_graph_path(key, false),
                     output.run_graph_path(key, false).with_extension("svg")] {
            assert!(path.exists() && path.starts_with(derivatives.join("workflow")), "{}", path.display());
        }
    }
    let summarise = || crate::dataset::write_dataset_summary(&derivatives, &config).unwrap();
    let read = |p: std::path::PathBuf| fs_err::read_to_string(p).unwrap();
    summarise();
    let methods = read(derivatives.join("methods.md"));
    assert!(!methods.contains("Where available") && methods.contains("A brain mask was generated"), "{methods}");
    assert!(qsmxt_engine::methods::generate_methods(&config).contains("Where available"));
    assert_eq!(methods, read(output.run_methods_path(&runs[0].key)));
    assert!(!output.changes_dir().exists());
    let graph = read(output.dataset_graph_path());
    assert!(graph.contains("All 3 runs: "), "{graph}");

    // Run c decided otherwise (as if its supplied mask had been used): it alone differs.
    let c = &runs[2].key;
    let mut file: RunRecordFile = serde_json::from_str(&read(output.run_record_path(c))).unwrap();
    let mask = file.record.steps.iter_mut().find(|s| s.id == "mask").unwrap();
    mask.decisions = vec![Decision::new(Topic::MaskSource, Choice::Supplied, "Taken from the supplied mask.")];
    fs_err::write(output.run_record_path(c), serde_json::to_string(&file).unwrap()).unwrap();
    summarise();
    let methods = read(derivatives.join("methods.md"));
    assert!(methods.contains("Where available"), "{methods}");
    let changes = || -> Vec<String> {
        let mut v: Vec<String> = fs_err::read_dir(output.changes_dir()).map(|d| d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        v.sort();
        v
    };
    assert_eq!(changes().len(), 3, "{:?}", changes());
    let cname = c.basename();
    assert!(output.change_path(&cname, false).exists() && output.change_path(&cname, true).with_extension("svg").exists());
    assert!(read(output.change_path(&cname, false)).contains("The brain mask was supplied with the scan"));
    let before = (methods.clone(), read(output.dataset_graph_path()), changes());

    // Run a alone again (`--include`): the dataset is still a, b and c, and c's files stay.
    run_pipeline_cached(&runs[0], &config, &output, false, &|_| {}).unwrap();
    summarise();
    assert_eq!((read(derivatives.join("methods.md")), read(output.dataset_graph_path()), changes()), before);

    // Run d processed with other settings, and a run whose latest attempt did not finish, are
    // not current: neither counts.
    let mut other = config.clone();
    other.masking.custom_mask_tool = None;
    run_pipeline_cached(&runs[3], &other, &output, true, &|_| {}).unwrap();
    assert!(output.run_record_path(&runs[3].key).exists());
    let b_state = crate::pipeline::graph::state_file_path(&derivatives, &runs[1].key);
    let saved = read(b_state.clone());
    fs_err::write(&b_state, saved.replace("\"complete\"", "\"running\"")).unwrap();
    summarise();
    let graph = read(output.dataset_graph_path());
    assert!(graph.contains("1 of 2: supplied (nothing-here)"), "{graph}");
    fs_err::write(&b_state, saved).unwrap();

    // Run c again: it is like the rest, and changes/ goes.
    run_pipeline_cached(&runs[2], &config, &output, false, &|_| {}).unwrap();
    summarise();
    assert!(!output.changes_dir().exists());
    assert!(read(output.dataset_graph_path()).contains("All 3 runs: "));
}

/// ARLO and chi-separation read the acquired-grid source magnitudes and indexed past their end.
#[test]
fn r2star_is_computed_on_the_working_grid() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 3);
    let mut config = resampling_config();
    config.pipeline.do_t2starmap = true;
    config.pipeline.do_r2starmap = true;
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    run_pipeline_cached(&run, &config, &output, true, &|_| {}).unwrap();
    assert_eq!(voxels(&output.r2star_path(&run.key)), ACQUIRED);
}

/// Returning the outputs to the acquired grid rewrote the mask and magnitude in place, so a
/// re-run that recomputed the inversion cropped acquired-grid data against the working grid.
#[test]
fn a_rerun_after_returning_to_the_acquired_grid_reads_the_working_grid() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 3);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut config = resampling_config();
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();

    // A new inversion threshold: the inversion is recomputed.
    config.inversion.tkd.threshold *= 2.0;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
    assert_eq!(voxels(&output.mask_path(&run.key)), ACQUIRED);

    // Asking for the working grid on a re-run brings the working-grid outputs back.
    config.pipeline.output_space = qsmxt_engine::OutputSpace::Working;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    // ...under their working-space names (#275).
    assert_ne!(voxels(&output.published(&run.key).qsm_path(&run.key)), ACQUIRED);
    assert!(!output.qsm_path(&run.key).exists(), "the acquired-grid map was left behind");
}

/// Runs of one session share `anat/`, so returning one run to the acquired grid must leave the
/// others alone — they may be mid-reconstruction on their own working grid.
#[test]
fn returning_to_the_acquired_grid_leaves_other_runs_alone() {
    let dir = tempfile::tempdir().unwrap();
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let a = oblique_run(dir.path(), "a", 1);
    let mut working = resampling_config();
    working.pipeline.output_space = qsmxt_engine::OutputSpace::Working;
    run_pipeline_cached(&a, &working, &output, false, &|_| {}).unwrap();
    let a_mask = output.published(&a.key).mask_path(&a.key);
    let a_working = voxels(&a_mask);
    assert_ne!(a_working, ACQUIRED);

    // `acq-a_run-2` starts with `acq-a`'s basename, so it is the harder neighbour to tell apart.
    let mut b = oblique_run(dir.path(), "a", 1);
    b.key.run = Some("2".into());
    run_pipeline_cached(&b, &resampling_config(), &output, false, &|_| {}).unwrap();
    assert_eq!(voxels(&output.mask_path(&b.key)), ACQUIRED);
    assert_eq!(voxels(&a_mask), a_working, "run a's mask was resampled by run b");
}

/// Every published image carries a sidecar whose `GeneratedBy` names an activity that is in
/// `prov/`, and the provenance records QSM.rs by its commit rather than "unknown" (#278).
/// Resampled back to the acquired grid, two-pass, with SWI/SMWI and R2*, so the steps that
/// rewrite and combine maps are all exercised.
#[test]
fn every_published_image_has_resolvable_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 3);
    let mut config = resampling_config();
    config.pipeline.do_t2starmap = true;
    config.pipeline.do_r2starmap = true;
    config.pipeline.do_swi = true;
    config.swi.filtered_phase = true;
    config.pipeline.do_smwi = true;
    config.masking.two_pass = true;
    let deriv = dir.path().join("out");
    let output = DerivativeOutputs::new(&deriv);
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    crate::bids::bidsprov::write_provenance(&deriv, std::slice::from_ref(&run), &[], &output, "qsmxt run")
        .unwrap();

    let read = |p: &Path| -> serde_json::Value {
        serde_json::from_str(&fs_err::read_to_string(p).unwrap()).unwrap()
    };
    let act = read(&deriv.join("prov/prov-qsmxt_act.json"));
    let ent = read(&deriv.join("prov/prov-qsmxt_ent.json"));
    let mut images = 0;
    for entry in fs_err::read_dir(output.anat_dir(&run.key)).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "nii") {
            continue;
        }
        images += 1;
        let sidecar = path.with_extension("json");
        assert!(sidecar.exists(), "{} has no sidecar", path.display());
        let meta = read(&sidecar);
        let by = meta["GeneratedBy"].as_str()
            .unwrap_or_else(|| panic!("{} has no GeneratedBy", sidecar.display()));
        assert!(act.get(by).is_some(), "{}: GeneratedBy {by} is not in prov/", sidecar.display());
        assert_eq!(meta["QSMrsVersion"], crate::bids::bidsprov::QSM_RS_VERSION);
    }
    assert!(images >= 10, "only {images} published images: the run did not exercise enough");
    // SWI's filtered phase keeps its units through the return to the acquired grid.
    let phase = read(&output.swi_phase_path(&run.key).with_extension("json"));
    assert_eq!(phase["Units"], if run.echo_times.len() > 1 { "Hz" } else { "rad" }, "{phase}");
    // Every GeneratedBy an entity names resolves too.
    for (iri, e) in ent.as_object().unwrap() {
        if let Some(by) = e["GeneratedBy"].as_str() {
            assert!(act.get(by).is_some(), "entity {iri}: GeneratedBy {by} is not in prov/");
        }
    }
    // The resampled maps were used from their working-grid originals, which are linked to the
    // activities that made them.
    let resample = &act[format!("bids::prov/#output_space-{}", run.key.basename())];
    assert!(resample["Used"].as_array().unwrap().len() > 1, "{resample}");
    for dir_entry in fs_err::read_dir(deriv.join("prov")).unwrap() {
        let text = fs_err::read_to_string(dir_entry.unwrap().path()).unwrap();
        assert!(!text.contains("unknown"), "provenance still says unknown");
    }
    let soft = read(&deriv.join("prov/prov-qsmxt_soft.json"));
    let commit = env!("QSM_CORE_GIT_HASH");
    assert!(soft.as_object().unwrap().values().any(|a| a["Label"] == "QSM.rs"
        && a["CodeURL"].as_str().is_some_and(|u| u.ends_with(commit))), "{soft}");
}

/// A run where the head moved between echoes, reaching the edge of the field of view: after
/// motion correction some brain voxels are not covered by every echo. Background removal, the
/// inversion, both passes of a two-pass run and the published support all stay inside that
/// coverage, which is published (#277) — they used the full brain mask, where the field was
/// never computed. Switching motion correction off again re-runs what read it.
#[test]
fn reconstruction_stays_inside_the_motion_coverage() {
    let dims = (24usize, 20usize, 16usize);
    let (nx, ny, nz) = dims;
    let affine = [1.0, 0.0, 0.0, -12.0, 0.0, 1.0, 0.0, -10.0, 0.0, 0.0, 1.0, -8.0, 0.0, 0.0, 0.0, 1.0];
    let dir = tempfile::tempdir().unwrap();
    let mut echoes = Vec::new();
    for e in 0..3 {
        // The head slides 3 mm along x over the echoes, and fills x from edge to edge.
        let shift = 1.5 * e as f64;
        let (mut mag, mut phase) = (vec![0.0; nx * ny * nz], vec![0.0; nx * ny * nz]);
        for k in 0..nz { for j in 0..ny { for i in 0..nx {
            let v = i + j * nx + k * nx * ny;
            let (x, y, z) = (i as f64 - 11.5 - shift, j as f64 - 9.5, k as f64 - 7.5);
            let inside = (y * y / 64.0 + z * z / 36.0) < 1.0;
            mag[v] = if inside { 100.0 + 20.0 * (0.5 * x).sin() + 15.0 * (0.6 * y).cos() + 5.0 * z } else { 1.0 };
            phase[v] = (0.04 * (e + 1) as f64 * (x + 0.5 * y)).sin() * 2.0;
        }}}
        let p = dir.path().join(format!("echo{}_phase.nii", e + 1));
        let m = dir.path().join(format!("echo{}_mag.nii", e + 1));
        io::save_nifti_to_file(&p, &phase, dims, (1.0, 1.0, 1.0), &affine).unwrap();
        io::save_nifti_to_file(&m, &mag, dims, (1.0, 1.0, 1.0), &affine).unwrap();
        echoes.push((p, Some(m)));
    }
    let mut run = super::run_with_echoes(echoes);
    run.key = key("motion");
    run.dims = dims;
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut config = fast_config();
    config.motion_correction.enabled = true;
    config.masking.two_pass = true;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();

    let k = &run.key;
    let count = |m: &[u8]| m.iter().filter(|&&v| v != 0).count();
    let brain = load_mask(&output.mask_path(k)).unwrap();
    let coverage_path = output.motion_coverage_path(k);
    assert!(coverage_path.exists(), "the coverage mask is published");
    let coverage = load_mask(&coverage_path).unwrap();
    assert!(count(&coverage) < count(&brain),
            "the fixture has to lose coverage: {} of {}", count(&coverage), count(&brain));
    let inside = |name: &str, m: &[u8]| {
        let outside = m.iter().zip(&coverage).filter(|(&v, &c)| v != 0 && c == 0).count();
        assert_eq!(outside, 0, "{name} has {outside} voxels outside the motion coverage");
    };
    inside("background-removal mask", &load_mask(&output.bg_mask_path(k)).unwrap());
    inside("reliable-pass mask", &load_mask(&output.two_pass_mask_path(k)).unwrap());
    inside("reliable-pass background-removal mask", &load_mask(&output.reliable_bg_mask_path(k)).unwrap());
    inside("published support", &load_mask(&output.qsm_support_path(k)).unwrap());
    let chi: Vec<u8> = load_volume(&output.chi_raw_path(k)).unwrap().iter().map(|&c| u8::from(c != 0.0)).collect();
    inside("raw χ", &chi);

    // Motion correction off: the coverage mask is withdrawn and reconstruction runs on the
    // brain mask again, rather than reusing what was cached against the coverage.
    config.motion_correction.enabled = false;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    assert!(!coverage_path.exists(), "a stale coverage mask stays published");
    let bg = load_mask(&output.bg_mask_path(k)).unwrap();
    assert!(bg.iter().zip(&coverage).any(|(&v, &c)| v != 0 && c == 0),
            "background removal still ran on the coverage mask after motion correction was switched off");
}

/// Replacing an input file in place recomputes the run from `load` (#272); re-running without
/// touching anything recomputes nothing.
#[test]
fn touching_an_input_recomputes_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 2);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let config = fast_config();
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    let mtime = |p: PathBuf| fs_err::metadata(p).unwrap().modified().unwrap();
    let first = mtime(output.qsm_path(&run.key));
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    assert_eq!(mtime(output.qsm_path(&run.key)), first, "an untouched re-run recomputed χ");

    let phase = &run.echoes[1].phase_nifti;
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs_err::write(phase, fs_err::read(phase).unwrap()).unwrap();
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    assert_ne!(mtime(output.qsm_path(&run.key)), first, "a replaced input did not recompute χ");
}

/// `--clean-intermediates` treated the output-space step's files, which are the final
/// outputs, as intermediates and deleted them.
#[test]
fn cleaning_intermediates_keeps_the_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 1);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    run_pipeline_cached(&run, &resampling_config(), &output, false, &|_| {}).unwrap();
    crate::dataset::clean_run(&output, &run);
    assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
    assert!(!output.working_grid_dir(&run.key).exists());
    // Re-run with the same configuration: nothing is recomputed, and nothing is lost for want
    // of the working-grid copies that cleaning removed.
    let mtime = |p: PathBuf| fs_err::metadata(p).unwrap().modified().unwrap();
    let before = mtime(output.qsm_path(&run.key));
    run_pipeline_cached(&run, &resampling_config(), &output, false, &|_| {}).unwrap();
    assert_eq!(mtime(output.qsm_path(&run.key)), before, "the re-run recomputed χ");
    assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
    // A changed configuration recomputes what it needs, and lands on the acquired grid again.
    let mut changed = resampling_config();
    changed.inversion.tkd.threshold *= 2.0;
    run_pipeline_cached(&run, &changed, &output, false, &|_| {}).unwrap();
    assert_ne!(mtime(output.qsm_path(&run.key)), before);
    assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
}

/// QSMART's intermediates land in each pass's own step directory, are recorded as that step's
/// outputs, and go with the other intermediates when they are cleaned. The threshold its echo
/// fit resolved is recorded with the unwrap step.
#[test]
fn qsmart_keeps_its_intermediates_in_each_pass_step() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 3);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut config = fast_config();
    config.inversion.algorithm = QsmAlgorithm::Qsmart;
    config.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
    config.masking.two_pass = true;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    let prov: serde_json::Value = serde_json::from_str(
        &fs_err::read_to_string(output.provenance_path(&run.key, "unwrap")).unwrap()).unwrap();
    assert!(prov["parameters"]["qsmart_fit_threshold"].is_f64(), "{prov}");

    let names = [
        qsm_core::pipeline::qsmart::outputs::VESSEL_MASK,
        qsm_core::pipeline::qsmart::outputs::LOCAL_FIELD_STAGE1,
        qsm_core::pipeline::qsmart::outputs::CHI_STAGE1,
        qsm_core::pipeline::qsmart::outputs::LOCAL_FIELD_STAGE2,
        qsm_core::pipeline::qsmart::outputs::CHI_STAGE2,
    ];
    let state = PipelineState::load_or_create(&output.state_path(&run.key), &config, &run.key, false);
    let mut written = Vec::new();
    for step in ["qsmart", "qsmart-reliable"] {
        let recorded = &state.completed_steps[step].outputs;
        for name in names {
            let path = output.qsmart_intermediate_path(&run.key, step, name).unwrap();
            assert!(path.exists(), "{step}: {} not written", path.display());
            assert!(recorded.contains(&path), "{step}: {} not recorded", path.display());
            assert_eq!(voxels(&path), voxels(&output.mask_path(&run.key)), "on the working grid");
            written.push(path);
        }
    }
    crate::dataset::clean_run(&output, &run);
    assert!(written.iter().all(|p| !p.exists()), "cleaning keeps QSMART's intermediates");
}

/// Each inversion algorithm's step writes the same raw map. Switching to another and back in
/// the same directory (#373) recomputes the first, and leaves what a fresh run in a directory
/// of its own does: the same maps, the same sidecar, and no record of the other step.
#[test]
fn switching_the_inversion_and_back_gives_what_a_fresh_run_does() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 2);
    for two_pass in [false, true] {
        let mut config = fast_config();
        config.masking.two_pass = two_pass;
        let fresh = DerivativeOutputs::new(&dir.path().join(format!("fresh-{two_pass}")));
        run_pipeline_cached(&run, &config, &fresh, false, &|_| {}).unwrap();
        let output = DerivativeOutputs::new(&dir.path().join(format!("out-{two_pass}")));
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();

        let k = &run.key;
        let mut maps = vec![(output.qsm_path(k), fresh.qsm_path(k)), (output.qsm_support_path(k), fresh.qsm_support_path(k)),
                            (output.chi_raw_path(k), fresh.chi_raw_path(k))];
        if two_pass {
            maps.push((output.reliable_chi_raw_path(k), fresh.reliable_chi_raw_path(k)));
        }
        for (got, want) in maps {
            assert!(io::read_nifti_file(&got).unwrap().data == io::read_nifti_file(&want).unwrap().data,
                    "two-pass {two_pass}: {} is not what a fresh run makes", got.display());
        }
        let sidecar = |o: &DerivativeOutputs| read_json_object(&crate::bids::entities::sidecar_path(&o.qsm_path(k)).unwrap());
        assert_eq!(sidecar(&output), sidecar(&fresh), "two-pass {two_pass}");
        let state = PipelineState::load_or_create(&output.state_path(k), &config, k, false);
        assert!(!state.completed_steps.contains_key("qsmart"), "two-pass {two_pass}: QSMART's record outlived its map");
        assert!(!output.provenance_path(k, "qsmart").exists(), "two-pass {two_pass}: QSMART's provenance outlived its map");
    }
}

/// Switched from each inversion to each other one and back, the only provenance naming a raw
/// map as its output is that of the step that wrote it last. QSMART runs on its own mask, as
/// `qsmxt run` gives it by default, so switching to or from it remakes the mask first and the
/// state has already forgotten the other inversion by the time QSMART is recorded.
///
/// One directory per pass mode, taken through a sequence of inversions that holds every
/// "a, then b, then a again" (TKD, TGV and QSMART, each to each other one and back), and
/// checked after every step. The plan says which step each inversion is and what it writes;
/// the store records them as the engine does, and nothing is reconstructed: what is under test
/// is how `qsmxt run` keeps provenance, not the inversions (the engine's own
/// `switching_from_*_and_back` tests run those).
#[test]
fn only_the_last_inversion_is_named_the_raw_maps_maker() {
    use qsmxt_engine::plan::{Pass, Stage};
    use qsmxt_engine::store::{Completed, Item, StageStore};
    use QsmAlgorithm::{Qsmart as Q, Tgv as T, Tkd as K};
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 2);
    let k = &run.key;
    let sequence = [K, T, K, T, Q, T, Q, K, Q, K];
    for (a, b) in [(K, T), (T, K), (K, Q), (Q, K), (T, Q), (Q, T)] {
        assert!(sequence.windows(3).any(|w| w == [a, b, a]), "{a:?} → {b:?} → {a:?} is not in the sequence");
    }
    let step_of = |a: QsmAlgorithm| match a { K => "invert", T => "tgv", _ => "qsmart" };
    for two_pass in [false, true] {
        let mut config = fast_config();
        config.masking.two_pass = two_pass;
        let output = DerivativeOutputs::new(&dir.path().join(format!("two-pass-{two_pass}")));
        let state_path = output.state_path(k);
        let mut last_mask = None;
        for (i, &algorithm) in sequence.iter().enumerate() {
            config.inversion.algorithm = algorithm;
            config.masking.sections = if algorithm == Q { qsmart_mask_sections() } else { fast_config().masking.sections };
            let plan = qsmxt_engine::plan::plan(&config, &requirements::scan_facts(&run, &config));
            let mut store = FileStore {
                run: &run, config: &config, output: output.clone(), meta: None,
                state: PipelineState::load_or_create(&state_path, &config, k, false), state_path: state_path.clone(),
                started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(), metadata: None,
                working_grid_kept: false,
            };
            let mut record = |step: &qsmxt_engine::plan::Step, key: &str| {
                store.start(&step.name);
                store.record(&Completed {
                    step: &step.name, algorithm: step.algorithm.as_deref(), params: &serde_json::json!({}),
                    outcome: None, key, inputs: &step.reads, outputs: &step.outputs, decisions: &[],
                }).unwrap();
            };
            // The mask is remade when its recipe changes, which forgets what was downstream.
            let mask_key = format!("{:?}", config.masking.sections);
            if last_mask.as_ref() != Some(&mask_key) {
                record(plan.steps.iter().find(|s| s.stage == Stage::Mask(Pass::Main)).unwrap(), &mask_key);
                last_mask = Some(mask_key);
            }
            let passes: &[Pass] = if two_pass { &[Pass::Main, Pass::Reliable] } else { &[Pass::Main] };
            for &pass in passes {
                let step = plan.steps.iter()
                    .find(|s| matches!(s.stage, Stage::Inversion(p) | Stage::Tgv(p) | Stage::Qsmart(p) if p == pass))
                    .unwrap();
                assert_eq!(step.name, format!("{}{}", step_of(algorithm), pass.suffix()));
                assert!(step.outputs.contains(&Item::RawChi(pass)), "{}: {:?}", step.name, step.outputs);
                record(step, &format!("{algorithm:?}"));
            }
            let mut raws = vec![(output.chi_raw_path(k), step_of(algorithm).to_string())];
            if two_pass {
                raws.push((output.reliable_chi_raw_path(k), format!("{}-reliable", step_of(algorithm))));
            }
            for (raw, step) in raws {
                let makers: Vec<&str> = crate::bids::bidsprov::STEPS.iter().copied().filter(|&s| {
                    let Ok(text) = fs_err::read_to_string(output.provenance_path(k, s)) else { return false };
                    let prov: serde_json::Value = serde_json::from_str(&text).unwrap();
                    prov["outputs"].as_array().unwrap().iter().any(|o| o.as_str() == Some(&raw.display().to_string()))
                }).collect();
                assert_eq!(makers, [step.as_str()], "{:?}, two-pass {two_pass}", &sequence[..=i]);
            }
        }
    }
}

/// `qsmxt run` on an output directory where a χ-sepnet toolbox run finished, now asking for
/// the R2′ map (#502): the state file keeps what each step read, so the R2 step, computed for
/// the first time, leaves χ-separation cached on the route that estimates its own R2′. On
/// the route that reads the R2 step's R2′, computing it again makes χ-separation stale. A state written before steps kept
/// what they read falls back to the fixed table, which makes it stale either way. The steps
/// are recorded as the engine records them (the plan's reads and writes) and not computed:
/// the networks are not what is under test.
#[test]
fn asking_for_the_r2prime_map_reuses_chi_separation_unless_it_reads_it() {
    use qsmxt_engine::enums::ChiSepNetRoute;
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 3);
    let k = &run.key;
    for (route, legacy, kept) in [(ChiSepNetRoute::Toolbox, false, true), (ChiSepNetRoute::Native, false, false),
                                  (ChiSepNetRoute::Toolbox, true, false)] {
        let output = DerivativeOutputs::new(&dir.path().join(format!("{route}-{legacy}")));
        let state_path = output.state_path(k);
        let mut config = fast_config();
        config.pipeline.do_chi_separation = true;
        config.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        config.separation.chi_sepnet.route = route;
        fn store<'a>(run: &'a QsmRun, config: &'a PipelineConfig, output: &DerivativeOutputs, state_path: &Path) -> FileStore<'a> {
            FileStore {
                run, config, output: output.clone(), meta: None,
                state: PipelineState::load_or_create(state_path, config, &run.key, false), state_path: state_path.to_path_buf(),
                started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(), metadata: None,
                working_grid_kept: false,
            }
        }
        let record = |store: &mut FileStore, step: &qsmxt_engine::plan::Step, key: &str| {
            store.start(&step.name);
            store.record(&Completed {
                step: &step.name, algorithm: step.algorithm.as_deref(), params: &serde_json::json!({}),
                outcome: None, key, inputs: &step.reads, outputs: &step.outputs, decisions: &[],
            }).unwrap();
        };
        let plan = qsmxt_engine::plan::plan(&config, &requirements::scan_facts(&run, &config));
        let mut first = store(&run, &config, &output, &state_path);
        for step in plan.steps.iter().filter(|s| !matches!(s.stage, Stage::Load | Stage::Motion | Stage::OutputSpace)) {
            record(&mut first, step, "k");
        }
        assert!(first.state.completed_steps.contains_key("chi_separation"));
        drop(first);
        if legacy {
            let mut state = PipelineState::load_or_create(&state_path, &config, k, false);
            state.completed_steps.values_mut().for_each(|r| r.inputs = None);
            state.save(&state_path).unwrap();
        }

        config.pipeline.do_r2primemap = true;
        let plan = qsmxt_engine::plan::plan(&config, &requirements::scan_facts(&run, &config));
        let mut again = store(&run, &config, &output, &state_path);
        // The toolbox route took no R2 step until the map was asked for; the network-only route
        // reads the R2 step's R2′, which is computed again here under another key.
        assert_eq!(again.state.completed_steps.contains_key("r2_r2prime"), route == ChiSepNetRoute::Native);
        let r2 = plan.steps.iter().find(|s| s.stage == Stage::R2).unwrap();
        record(&mut again, r2, "k2");
        let cached: Vec<&str> = plan.steps.iter().map(|s| s.name.as_str())
            .filter(|s| again.state.completed_steps.contains_key(*s)).collect();
        assert_eq!(cached.contains(&"chi_separation"), kept, "{route}, legacy state {legacy}: {cached:?}");
        // Nothing else read the map. (R2PRIMEnet needs axial data, so the plan that asks for
        // its R2′ may resample, which the first did not.)
        for step in plan.steps.iter().filter(|s| !matches!(s.stage, Stage::Load | Stage::Motion | Stage::OutputSpace
                                                                   | Stage::ChiSeparation | Stage::ResampleAxial)) {
            assert!(cached.contains(&step.name.as_str()), "{route}, legacy state {legacy}: {} went stale: {cached:?}", step.name);
        }
    }
}

/// A recomputed map's sidecar says what was done to it, not to the map it replaced: re-run
/// without a reference, the map records no offset from the mean reference it had before.
#[test]
fn a_recomputed_map_keeps_nothing_from_the_sidecar_it_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 2);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut config = fast_config();
    let sidecar = || read_json_object(&crate::bids::entities::sidecar_path(&output.qsm_path(&run.key)).unwrap());
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    assert!(sidecar().contains_key("QsmReferenceOffsetPpm"));
    config.qsm.reference = qsmxt_engine::QsmReference::None;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    let fields = sidecar();
    assert_eq!(fields["QsmReference"], "none");
    assert!(!fields.contains_key("QsmReferenceOffsetPpm") && !fields.contains_key("QsmReferenceVoxels"), "{fields:?}");
}

/// End to end with V-SHARP: the published χ is 0 on the rim background removal eroded, its
/// mean over the published support is the reference zero, and the support is smaller than
/// the brain mask (or the test would prove nothing).
#[test]
fn the_published_map_is_zero_outside_its_published_support() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 3);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    run_pipeline_cached(&run, &fast_config(), &output, false, &|_| {}).unwrap();

    let chi = load_volume(&output.qsm_path(&run.key)).unwrap();
    let brain = load_mask(&output.mask_path(&run.key)).unwrap();
    let support = load_mask(&output.qsm_support_path(&run.key)).unwrap();
    let count = |m: &[u8]| m.iter().filter(|&&v| v != 0).count();
    assert!(count(&support) > 0 && count(&support) < count(&brain),
            "support {} vs brain {}", count(&support), count(&brain));
    assert!(support.iter().zip(&brain).all(|(&s, &b)| s <= b), "the support lies in the brain");
    for (i, (&c, &s)) in chi.iter().zip(&support).enumerate() {
        if s == 0 {
            assert_eq!(c, 0.0, "voxel {i} outside the support");
        }
    }
    let mean: f64 = chi.iter().zip(&support).filter(|(_, &s)| s != 0).map(|(&c, _)| c).sum::<f64>()
        / count(&support) as f64;
    assert!(mean.abs() < 1e-5, "mean over the support is {mean}");
}

/// The grid was cached with the first run's geometry, so a new threshold was ignored.
#[test]
fn a_new_obliquity_threshold_is_honoured_on_a_rerun() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 1);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let resampled = |output: &DerivativeOutputs| -> bool {
        let state: PipelineState = serde_json::from_str(
            &fs_err::read_to_string(output.state_path(&run.key)).unwrap()).unwrap();
        state.run_metadata.unwrap().source_geometry.is_some()
    };
    run_pipeline_cached(&run, &fast_config(), &output, false, &|_| {}).unwrap();
    assert!(!resampled(&output));
    run_pipeline_cached(&run, &resampling_config(), &output, false, &|_| {}).unwrap();
    assert!(resampled(&output));
    run_pipeline_cached(&run, &fast_config(), &output, false, &|_| {}).unwrap();
    assert!(!resampled(&output));
    assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
}

/// A windowed, resampled run in files: phase scaling and the Tukey window leave each echo on
/// the acquired grid, in files with that grid's header, the resampling writes the working
/// echoes, and a re-run reuses all three.
#[test]
fn the_phase_steps_leave_the_acquired_grid_echoes_in_files() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 2);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut config = resampling_config();
    config.pipeline.tukey_window = 0.2;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    let k = &run.key;
    let acquired = io::read_nifti_file(&run.echoes[0].phase_nifti).unwrap();
    for (step, item) in [("scale_phase", "phase"), ("tukey_window", "phase"), ("tukey_window", "mag")] {
        let path = if item == "phase" { output.acquired_grid_phase_path(k, step, 2) } else { output.acquired_grid_mag_path(k, step, 2) };
        let nii = io::read_nifti_file(&path).unwrap_or_else(|e| panic!("{step} {item}: {e}"));
        assert_eq!((nii.dims, nii.affine), (acquired.dims, acquired.affine), "{step} {item}");
    }
    assert!(!output.acquired_grid_mag_path(k, "scale_phase", 2).exists(), "the magnitude given is read where it is");
    let working = io::read_nifti_file(&output.phase_scaled_path(k, 2)).unwrap();
    assert_ne!(working.dims, acquired.dims, "the working echoes are on the axial grid");
    assert_eq!(voxels(&output.mag_path(k, 2)), working.data.len());
    let provenance = |step: &str| fs_err::read_to_string(output.provenance_path(k, step)).unwrap_or_else(|e| panic!("{step}: {e}"));
    let steps = ["scale_phase", "tukey_window", "resample_axial"];
    let before: Vec<String> = steps.iter().map(|s| provenance(s)).collect();
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    for (step, was) in steps.iter().zip(before) {
        assert_eq!(provenance(step), was, "{step} was recomputed");
    }
}

/// Returning a resampled run to the acquired grid moves a segmentation by majority vote, so
/// every value is still a label id, and rebuilds the SMWI minIPs on the acquired grid (#267).
#[test]
fn output_space_keeps_labels_valid_and_rebuilds_the_smwi_projections() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 2);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut config = resampling_config();
    config.pipeline.do_smwi = true;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    for contrast in qsmxt_engine::store::Contrast::BOTH {
        let mip = io::read_nifti_file(&output.smwi_mip_path(&run.key, contrast)).unwrap();
        let contrast = contrast.name();
        assert_eq!((mip.dims.0, mip.dims.1), (DIMS.0, DIMS.1),
                   "the {contrast} minIP is still on the working grid");
    }

    // A parcellation on the working grid, where the pipeline would have written one, and the
    // return to the acquired grid made to run again.
    let work = io::read_nifti_file(&output.working_grid_dir(&run.key)
        .join(output.mask_path(&run.key).file_name().unwrap())).unwrap();
    let ids = [0.0, 10.0, 11.0, 49.0, 50.0];
    let dseg: Vec<f64> = (0..work.data.len()).map(|v| ids[(v / 7) % ids.len()]).collect();
    save_nifti_like(&output.dseg_path(&run.key), &dseg, &work);
    let state_path = output.state_path(&run.key);
    let mut state: PipelineState = serde_json::from_str(&fs_err::read_to_string(&state_path).unwrap()).unwrap();
    state.completed_steps.remove("output_space");
    state.save(&state_path).unwrap();

    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    let back = io::read_nifti_file(&output.dseg_path(&run.key)).unwrap();
    assert_eq!(back.data.len(), ACQUIRED);
    assert!(back.data.iter().all(|v| ids.contains(v)), "a label was interpolated");
}

/// `--output-space working` on a resampled run names its outputs `space-axial` with a
/// `SpatialReference`; switching back to the acquired grid leaves no `space-axial` file behind
/// (#275).
#[test]
fn working_grid_outputs_are_named_in_their_space() {
    let dir = tempfile::tempdir().unwrap();
    let run = oblique_run(dir.path(), "a", 2);
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut config = resampling_config();
    config.pipeline.output_space = qsmxt_engine::OutputSpace::Working;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    let axial = output.in_space(Some("axial"));
    assert!(axial.qsm_path(&run.key).exists());
    assert!(!output.qsm_path(&run.key).exists(), "an unlabelled map on the working grid");
    assert_eq!(output.published(&run.key).qsm_path(&run.key), axial.qsm_path(&run.key));
    let sidecar: serde_json::Value = serde_json::from_str(&fs_err::read_to_string(
        crate::bids::entities::sidecar_path(&axial.qsm_path(&run.key)).unwrap()).unwrap()).unwrap();
    let reference = sidecar["SpatialReference"].as_str().unwrap();
    assert!(output.output_dir.join(reference).exists(), "{reference}");
    assert!(reference.ends_with("_part-mag_space-axial_T2starw.nii"), "{reference}");

    config.pipeline.output_space = qsmxt_engine::OutputSpace::Acquired;
    run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
    assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
    let left: Vec<String> = fs_err::read_dir(output.anat_dir(&run.key)).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("space-axial"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

fn save_nifti_like(path: &Path, data: &[f64], like: &qsm_core::io::NiftiData) {
    write_volume(path, data, like.dims, like.voxel_size, &like.affine).unwrap();
}

/// An oblique three-orientation group under the default `--output-space acquired` (#270).
///
/// The members' magnitudes are back on the acquired grid by the time the group is combined,
/// while their local fields are on the working grid; the group used to read the two from
/// different grids and fail. Now it reads both from the working grid, writes a referenced
/// COSMOS map on the reference member's acquired grid, caches it, and recomputes it when a
/// member's background removal changes.
#[test]
fn an_oblique_cosmos_group_lands_on_the_acquired_grid_referenced_and_cached() {
    let dir = tempfile::tempdir().unwrap();
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    // No sidecar `B0_dir` (one would stop the resample to axial), so the three share one
    // affine and their directions cannot be told apart: `force` reconstructs anyway, which is
    // all this test needs — it is about grids and caching, not the directions.
    let runs: Vec<QsmRun> = ["d1", "d2", "d3"].iter()
        .map(|acq| oblique_run(dir.path(), acq, 2)).collect();
    let mut config = resampling_config();
    config.multi_orientation.group_by = "acq".into();
    config.multi_orientation.force = true;
    config.multi_orientation.lambda = 1e-2;
    for r in &runs {
        run_pipeline_cached(r, &config, &output, false, &|_| {}).unwrap();
    }
    let members: Vec<&QsmRun> = runs.iter().collect();
    let group = crate::bids::orientation::OrientationGroup { label: "sub-1".into(), members: vec![0, 1, 2] };
    let combine = |config: &PipelineConfig| crate::pipeline::multiorient::reconstruct_group(
        &group, &members, config, &output, false, &|_| {}).unwrap();

    combine(&config);
    let key = crate::pipeline::multiorient::group_key(&members);
    let cosmos = output.cosmos_path(&key);
    assert_eq!(voxels(&cosmos), ACQUIRED, "the COSMOS map is on the acquired grid");
    let sidecar: serde_json::Value = serde_json::from_str(&fs_err::read_to_string(
        output.multiorient_sidecar_path(&key, "cosmos")).unwrap()).unwrap();
    assert_eq!(sidecar["QsmReference"], "mean", "{sidecar}");
    assert!(sidecar["QsmReferenceOffsetPpm"].is_number(), "{sidecar}");
    assert_eq!(sidecar["Space"], "acquired");

    let mtime = || fs_err::metadata(&cosmos).unwrap().modified().unwrap();
    let first = mtime();
    combine(&config);
    assert_eq!(mtime(), first, "an unchanged group was recomputed");

    // A member's background removal changes: its local field is rewritten, and the group
    // is recomputed from it.
    let mut other = config.clone();
    other.bg_removal.algorithm = BfAlgorithm::Sharp;
    run_pipeline_cached(&runs[1], &other, &output, false, &|_| {}).unwrap();
    combine(&config);
    assert_ne!(mtime(), first, "a member's new local field did not recompute the group");
}
