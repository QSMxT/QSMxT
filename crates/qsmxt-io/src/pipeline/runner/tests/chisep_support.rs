//! χ-separation over the support χ is defined on, not the brain mask.
//!
//! Referencing writes χ as 0 on the rim background removal eroded. Fitting χ-separation over the
//! brain mask took that rim as measured χ = 0 and published para/dia maps there.

use crate::pipeline::runner::*;
use qsmxt_engine::stages::separation::zero_outside;
use super::reference_support::{fixture, meta, Fixture};

fn sep_config(alg: SeparationAlgorithm) -> PipelineConfig {
    let mut config = PipelineConfig::default();
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = alg;
    config
}

/// The fixture's run, with three echoes for χ-sep MEDI (its CSF mask is made from ARLO R2*).
fn sep_meta(config: &PipelineConfig) -> RunMetadata {
    let mut m = meta();
    if config.separation.algorithm == SeparationAlgorithm::ChiSepMedi {
        m.echo_times = vec![0.004, 0.008, 0.012];
        m.n_echoes = 3;
    }
    m
}

/// Echo `e`'s magnitude (from 1): a decay over the brain mask.
fn magnitude(f: &Fixture, e: usize) -> Vec<f64> {
    f.brain.iter().enumerate()
        .map(|(i, &b)| if b != 0 { (100.0 + 0.1 * (i % 5) as f64) * (-0.02 * e as f64).exp() } else { 0.0 })
        .collect()
}

/// Reference the fixture, add the relaxation, magnitude and local-field inputs (defined over
/// the whole brain mask, or the field over the support as background removal leaves it), and
/// run the `chi_separation` step. Returns the (para, dia, total) maps it wrote.
fn separate(f: &Fixture, config: &PipelineConfig) -> [Vec<f64>; 3] {
    let m = sep_meta(config);
    let k = &f.run.key;
    let over = |mask: &[u8], base: f64| -> Vec<f64> {
        mask.iter().enumerate()
            .map(|(i, &b)| if b != 0 { base + 0.1 * (i % 5) as f64 } else { 0.0 }).collect()
    };
    save_volume(&f.output.r2star_path(k), &over(&f.brain, 20.0), &m).unwrap();
    save_volume(&f.output.r2prime_path(k), &over(&f.brain, 5.0), &m).unwrap();
    for e in 1..=m.n_echoes {
        save_volume(&f.output.mag_path(k, e), &magnitude(f, e), &m).unwrap();
    }
    let field: Vec<f64> = over(&f.support, 0.0).iter().zip(&f.support)
        .map(|(&v, &s)| if s != 0 { 0.01 + 0.001 * v } else { 0.0 }).collect();
    save_volume(&f.output.local_field_path(k), &field, &m).unwrap();

    super::reference_support::reference(f, config);
    run_engine_step(&f.run, config, &f.output, &m, Stage::ChiSeparation).unwrap();
    [f.output.chi_para_path(k), f.output.chi_dia_path(k), f.output.chi_sep_total_path(k)]
        .map(|p| load_volume(&p).unwrap())
}

/// The method run directly on `mask`, with every input zeroed outside it (R2′ and R2* as the
/// method takes them) and the outputs
/// zeroed outside it: what the step must produce when it fits over `mask`.
fn direct(f: &Fixture, config: &PipelineConfig, mask: &[u8]) -> [Vec<f64>; 3] {
    let m = sep_meta(config);
    let k = &f.run.key;
    let masked = |p: PathBuf| -> Vec<f64> {
        let mut v = load_volume(&p).unwrap();
        zero_outside(mask, &mut v);
        v
    };
    // χ-sep iLSQR and χ-sep MEDI reconstruct their QSM from the local field inside its mask
    // (the background-removal mask, here the support), as the step does.
    let qsm = if qsmxt_engine::stages::separation::reconstructs_qsm(config) {
        let field = load_volume(&f.output.local_field_path(k)).unwrap();
        let mut q = qsmxt_engine::stages::separation::initial_qsm(&field, &f.support, &m, config, &mut ()).unwrap();
        zero_outside(mask, &mut q);
        q
    } else {
        masked(f.output.qsm_path(k))
    };
    let field = masked(f.output.local_field_path(k));
    // R2′ and R2* as the method takes them: unmasked for the χ-separation toolbox's methods.
    let relaxation = |p: PathBuf| -> Vec<f64> {
        let mut v = load_volume(&p).unwrap();
        qsmxt_engine::stages::separation::restrict_relaxation(config, mask, &mut v);
        v
    };
    let r2prime = relaxation(f.output.r2prime_path(k));
    let r2star = relaxation(f.output.r2star_path(k));
    let n_echoes = m.echo_times.len();
    let echoes: Vec<Vec<f64>> = (1..=n_echoes).map(|e| load_volume(&f.output.mag_path(k, e)).unwrap()).collect();
    let echo_major: Vec<f64> = echoes.concat();
    let multi: Vec<f64> = (0..mask.len()).flat_map(|v| echoes.iter().map(move |m| m[v])).collect();
    let rss: Vec<f64> = (0..mask.len()).map(|v| echoes.iter().map(|m| m[v] * m[v]).sum::<f64>().sqrt()).collect();
    let (n_std, csf) = if config.separation.algorithm == SeparationAlgorithm::ChiSepMedi {
        let (n, c) = qsmxt_engine::stages::separation::chi_sep_medi_inputs(
            &echo_major, &m.echo_times, mask, m.dims, m.voxel_size,
            config.separation.chi_sep_medi.csf_r2star).unwrap();
        (Some(n), Some(c))
    } else {
        (None, None)
    };
    let qsm_per_echo = qsmxt_engine::stages::separation::repeat_per_echo(&qsm, n_echoes);
    let inputs = qsm_core::pipeline::SeparationInputs {
        local_field_ppm: &field, qsm: &qsm, qsm_per_echo: Some(&qsm_per_echo), mask,
        r2prime: Some(&r2prime), r2star: Some(&r2star),
        magnitude_rss: Some(&rss), magnitude_multi: Some(&multi), se_magnitude_multi: None,
        n_std: n_std.as_deref(), mask_csf: csf.as_deref(),
    };
    let metadata = crate::pipeline::config::to_scan_metadata(
        m.dims, m.voxel_size, &m.echo_times, m.field_strength, m.b0_direction, Some(m.affine));
    let r = qsm_core::pipeline::run_separation(
        inputs, &metadata, &qsmxt_engine::bridge::to_separation_config(config), &mut |_, _| {},
    ).unwrap();
    let mut out = [r.chi_pos, r.chi_neg.iter().map(|v| v.abs()).collect(), r.chi_total];
    for map in out.iter_mut() {
        zero_outside(mask, map);
    }
    out
}

fn assert_zero_outside(maps: &[Vec<f64>; 3], support: &[u8]) {
    for (name, map) in ["para", "dia", "total"].iter().zip(maps) {
        for (i, (&v, &s)) in map.iter().zip(support).enumerate() {
            if s == 0 {
                assert_eq!(v, 0.0, "{name} voxel {i} lies outside the support");
            }
        }
    }
}

fn assert_close(a: &[Vec<f64>; 3], b: &[Vec<f64>; 3]) {
    for (name, (x, y)) in ["para", "dia", "total"].iter().zip(a.iter().zip(b)) {
        for (i, (&p, &q)) in x.iter().zip(y).enumerate() {
            assert!((p - q).abs() <= 1e-6 * (1.0 + q.abs()), "{name} voxel {i}: {p} vs {q}");
        }
    }
}

/// A voxel-wise method: 0 on the eroded rim, fitted everywhere on the support.
#[test]
fn r2star_qsm_is_zero_on_the_rim_and_fitted_over_the_support() {
    let f = fixture();
    let config = sep_config(SeparationAlgorithm::R2starQsm);
    let out = separate(&f, &config);
    assert!(f.brain.iter().zip(&f.support).any(|(&b, &s)| b != 0 && s == 0), "the fixture has a rim");
    assert_zero_outside(&out, &f.support);
    for (i, &s) in f.support.iter().enumerate() {
        if s != 0 {
            assert!(out[0][i] != 0.0 || out[1][i] != 0.0, "support voxel {i} was not fitted");
        }
    }
    assert_close(&out, &direct(&f, &config, &f.support));
}

/// χ-separation fits the support, not the brain mask. χ-sep MEDI, with the noise map and CSF
/// mask the step makes and the iLSQR QSM it reconstructs, matches the method run on the
/// support. On this fixture its result is
/// its voxelwise initial guess, so WaveSep, whose wavelet fit is spatial, shows that the
/// domain matters: run over the brain mask it gives a different answer.
///
/// The fixture's R2' and R2* cover the whole brain mask, rim included. χ-sep MEDI takes them
/// as they are on the rim, as the χ-separation toolbox's script passes them, and its result
/// depends on them there; the other methods get them zeroed outside the support.
#[test]
fn chi_separation_fits_the_support_not_the_brain_mask() {
    let f = fixture();
    let config = sep_config(SeparationAlgorithm::ChiSepMedi);
    let out = separate(&f, &config);
    assert_zero_outside(&out, &f.support);
    assert_close(&out, &direct(&f, &config, &f.support));

    let f = fixture();
    let config = sep_config(SeparationAlgorithm::WaveSep);
    let out = separate(&f, &config);
    assert_zero_outside(&out, &f.support);
    assert_close(&out, &direct(&f, &config, &f.support));
    let over_brain = direct(&f, &config, &f.brain);
    let differs = out[2].iter().zip(&over_brain[2]).zip(&f.support)
        .any(|((&a, &b), &s)| s != 0 && (a - b).abs() > 1e-6);
    assert!(differs, "fitting over the brain mask must change the support's values");
}


/// A rerun of a map fitted over the brain mask by an older release is not reused.
#[test]
fn the_cache_key_records_the_fit_domain() {
    let f = fixture();
    let config = sep_config(SeparationAlgorithm::R2starQsm);
    separate(&f, &config);
    let prov: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        f.output.provenance_path(&f.run.key, "chi_separation")).unwrap()).unwrap();
    assert_eq!(prov["parameters"]["domain"], "support");
    let support = f.output.qsm_support_path(&f.run.key).display().to_string();
    assert!(prov["inputs"].as_array().unwrap().iter().any(|p| p == &support), "{prov}");
}
