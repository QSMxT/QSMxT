//! Referencing over the map's support rather than the brain mask.
//!
//! Background removal erodes the mask, and the raw map is 0 on that rim. Referencing over the
//! brain mask averaged those zeros into the offset and then wrote the rim out as a constant
//! `-offset` — a shell of about 10% of the brain at 1 mm with V-SHARP.

use crate::pipeline::runner::*;
use crate::bids::discovery::{discover_runs, DiscoveryFilter};
use crate::testutils;

/// The cheapest reconstruction: these tests are about how the input is read (the conversion,
/// its correction and its provenance), so TKD, Laplacian unwrapping and a magnitude threshold
/// stand in for the default algorithms.
fn config() -> PipelineConfig {
    use qsmxt_engine::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
    let mut config = PipelineConfig::default();
    config.inversion.algorithm = QsmAlgorithm::Tkd;
    config.field_mapping.unwrapping_algorithm = crate::pipeline::config::UnwrappingAlgorithm::Laplacian;
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    config.bg_removal.vsharp.max_radius = 3.0;
    config
}

fn run_on(bids: &Path, out: &Path) -> (QsmRun, DerivativeOutputs) {
    let run = discover_runs(bids, &DiscoveryFilter::default()).unwrap().remove(0);
    let output = DerivativeOutputs::new(out);
    run_pipeline_cached(&run, &config(), &output, true, &|_| {}).unwrap();
    (run, output)
}

/// Real/imaginary input, chopped along z as GE leaves it, reconstructs exactly what the same
/// echoes given as magnitude/phase do: the chop is found and undone before anything else
/// runs, and the provenance says so.
#[test]
fn chopped_real_imag_matches_mag_phase() {
    let dir = tempfile::tempdir().unwrap();
    let complex = testutils::create_real_imag_bids(&dir.path().join("ri"), true);
    let twin = testutils::create_mag_phase_twin_bids(&dir.path().join("mp"));
    let (ri_run, ri) = run_on(&complex, &dir.path().join("ri-out"));
    let (mp_run, mp) = run_on(&twin, &dir.path().join("mp-out"));

    for e in 1..=3 {
        let a = load_volume(&ri.phase_scaled_path(&ri_run.key, e)).unwrap();
        let b = load_volume(&mp.phase_scaled_path(&mp_run.key, e)).unwrap();
        let worst = a.iter().zip(&b)
            .map(|(x, y)| { let d = (x - y).rem_euclid(2.0 * std::f64::consts::PI); d.min(2.0 * std::f64::consts::PI - d) })
            .fold(0.0, f64::max);
        assert!(worst < 1e-6, "echo {e} phase differs by up to {worst} rad");
        let a = load_volume(&ri.mag_path(&ri_run.key, e)).unwrap();
        let b = load_volume(&mp.mag_path(&mp_run.key, e)).unwrap();
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-3 * y.abs().max(1.0)), "echo {e} magnitude differs");
    }
    let chi_ri = load_volume(&ri.qsm_path(&ri_run.key)).unwrap();
    let chi_mp = load_volume(&mp.qsm_path(&mp_run.key)).unwrap();
    let worst = chi_ri.iter().zip(&chi_mp).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
    assert!(worst < 1e-4, "χ differs by up to {worst} ppm");

    let prov: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ri.provenance_path(&ri_run.key, "scale_phase")).unwrap()).unwrap();
    let params = &prov["parameters"];
    assert_eq!(params["input"], "real_imaginary");
    let conv = &params["real_imaginary_conversion"];
    assert_eq!(conv["method"], "complex_to_mag_phase");
    assert_eq!(conv["corrected"], serde_json::json!({ "x": false, "y": false, "z": true }));
    assert!(conv["coherence"]["z"].as_f64().unwrap() < -0.5, "{conv}");
    assert!(prov["inputs"].as_array().unwrap().iter().any(|p| p.as_str().unwrap().contains("_part-imag_")));
}

#[test]
fn ge_real_imag_phase_is_negated() {
    // End to end: a GE sidecar gives the negated phase of the same echoes from elsewhere.
    let dir = tempfile::tempdir().unwrap();
    let ge = testutils::create_real_imag_bids(&dir.path().join("ge"), true);
    for p in glob::glob(&format!("{}/sub-1/anat/*.json", ge.display())).unwrap().flatten() {
        let mut j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        j["Manufacturer"] = serde_json::json!("GE");
        std::fs::write(&p, j.to_string()).unwrap();
    }
    let other = testutils::create_real_imag_bids(&dir.path().join("other"), true);
    let (ge_run, ge_out) = run_on(&ge, &dir.path().join("ge-out"));
    let (other_run, other_out) = run_on(&other, &dir.path().join("other-out"));
    let a = load_volume(&ge_out.phase_scaled_path(&ge_run.key, 1)).unwrap();
    let b = load_volume(&other_out.phase_scaled_path(&other_run.key, 1)).unwrap();
    let worst = a.iter().zip(&b)
        .map(|(x, y)| { let d = (x + y).rem_euclid(2.0 * std::f64::consts::PI); d.min(2.0 * std::f64::consts::PI - d) })
        .fold(0.0, f64::max);
    assert!(worst < 1e-6, "GE phase is not the negation: up to {worst} rad off");
    let prov: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ge_out.provenance_path(&ge_run.key, "scale_phase")).unwrap()).unwrap();
    assert_eq!(prov["parameters"]["negate_phase"], true);
}

/// An unchopped export converts with no correction, and a re-run is served from the cache
/// even though the provenance carries the detection outcome next to the parameters.
#[test]
fn unchopped_real_imag_is_not_corrected_and_caches() {
    let dir = tempfile::tempdir().unwrap();
    let complex = testutils::create_real_imag_bids(&dir.path().join("ri"), false);
    let (run, output) = run_on(&complex, &dir.path().join("out"));
    let prov_path = output.provenance_path(&run.key, "scale_phase");
    let prov: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&prov_path).unwrap()).unwrap();
    assert_eq!(prov["parameters"]["real_imaginary_conversion"]["corrected"],
        serde_json::json!({ "x": false, "y": false, "z": false }));

    let before = std::fs::metadata(&prov_path).unwrap().modified().unwrap();
    run_pipeline_cached(&run, &config(), &output, false, &|_| {}).unwrap();
    assert_eq!(std::fs::metadata(&prov_path).unwrap().modified().unwrap(), before, "scale_phase re-ran");
}
