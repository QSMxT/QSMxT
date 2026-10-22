//! Multi-echo B0 field mapping: wrapped phase per echo → B0 field map in ppm.
//!
//! QSMxT's entry point for the field-mapping stage, shared by `qsmxt run` and `qsmxt fieldmap`.
//! It is qsm-core's [`run_field_mapping`](qsm_core::pipeline::run_field_mapping) behind QSMxT's
//! stricter input checks. Everything this module used to do itself, qsm-core now does on its
//! own path (QSM.rs#161/#162/#166), where QSM.rs's tests check it:
//!
//! * **Laplacian unwrapping** unwraps each echo on its own (phase zeroed outside the mask, as STI
//!   Suite's `MRPhaseUnwrap` is called by UK Biobank), aligns the constant a Laplacian unwrap
//!   leaves undetermined across echoes, and combines the echoes with the configured estimator,
//!   so `--b0-estimation` and `--b0-weight-type` mean the same as with ROMEO.
//! * **`assumed-decay` weighting** is qsm-core's `B0WeightType::AssumedDecay`.
//! * The **FFT Laplacian solver** is qsm-core's `LaplacianSolver::Fft`.
//!
//! * A **single Laplacian echo** is masked and demeaned, and **`linear-fit`** aligns the
//!   Laplacian per-echo constants first (QSM.rs#167).
//!
//! * **QSMART's echo fit** (`--b0-estimation qsmart-echofit`) is qsm-core's
//!   `B0EstimationMethod::QsmartEchofit`: each echo Laplacian-unwrapped over the whole, unmasked
//!   volume, then QSMART's fit, which also returns its reliability map `R_0`
//!   ([`FieldMappingResult::reliability`]). Its field is not masked.
//!
//! The UK Biobank recipe (per-echo Laplacian unwrapping as STI Suite's `MRPhaseUnwrap`, then a
//! mean weighted by `TE·exp(−TE/T2*)` with an assumed T2* of 40 ms) is
//! `--unwrapping-algorithm laplacian --laplacian-solver fft --b0-weight-type assumed-decay`.

use qsm_core::pipeline::config::{
    B0EstimationMethod, FieldMappingConfig, PipelineError, ScanMetadata,
};
pub use qsm_core::pipeline::FieldMappingResult;
use qsm_core::utils::B0WeightType;

/// QSMxT's input checks, stricter than qsm-core's: a magnitude per echo, every volume and the
/// mask the size of the grid, and positive echo times.
fn validate(
    phases: &[&[f64]], mags: Option<&[&[f64]]>, mask: &[u8], meta: &ScanMetadata,
    config: &FieldMappingConfig,
) -> Result<(), PipelineError> {
    let n = meta.dims.0 * meta.dims.1 * meta.dims.2;
    if phases.is_empty() {
        return Err(PipelineError::InvalidInput("no phase echoes provided".into()));
    }
    if meta.echo_times.len() != phases.len() {
        return Err(PipelineError::DimensionMismatch { expected: phases.len(), got: meta.echo_times.len() });
    }
    if let Some(m) = mags {
        if m.len() != phases.len() {
            return Err(PipelineError::DimensionMismatch { expected: phases.len(), got: m.len() });
        }
    }
    let lens = phases.iter().chain(mags.unwrap_or(&[])).map(|v| v.len()).chain(std::iter::once(mask.len()));
    if let Some(got) = lens.into_iter().find(|&l| l != n) {
        return Err(PipelineError::DimensionMismatch { expected: n, got });
    }
    if meta.echo_times.iter().any(|&te| !te.is_finite() || te <= 0.0) {
        return Err(PipelineError::InvalidInput("echo times must be positive".into()));
    }
    if let B0EstimationMethod::WeightedAvg { weighting: B0WeightType::AssumedDecay { t2star_s } } = config.b0_estimation {
        if !(t2star_s.is_finite() && t2star_s > 0.0) {
            return Err(PipelineError::InvalidConfig(format!(
                "assumed T2* for the assumed-decay B0 weighting must be positive, got {t2star_s} s")));
        }
    }
    Ok(())
}

/// Run field mapping: per-echo wrapped phase (rad, in [−π, π]) → B0 field in ppm, zero outside
/// `mask`, with everything (unwrapping, Laplacian solver, B0 estimation and its weighting)
/// taken from `config`. `magnitudes` = `None` weights every echo and voxel equally.
pub fn run_field_mapping(
    phases: &[&[f64]],
    magnitudes: Option<&[&[f64]]>,
    mask: &[u8],
    meta: &ScanMetadata,
    config: &FieldMappingConfig,
) -> Result<FieldMappingResult, PipelineError> {
    validate(phases, magnitudes, mask, meta, config)?;
    qsm_core::pipeline::run_field_mapping(phases, magnitudes, mask, meta, config, &mut |_, _| {})
}

/// Log what the motion-robust fit found: one line per echo, and a warning naming flagged echoes.
pub fn log_robust_fit_report(report: &qsm_core::pipeline::RobustFitReport) {
    let q = &report.quality;
    log::info!("Robust echo fit over {} voxels (echo: down-weighted / rejected fraction, median weight, \
                median |residual| rad, outlier score):", q.voxels_examined);
    for e in 0..q.median_weight.len() {
        log::info!("  echo {}: {:.3} / {:.3}, {:.3}, {:.4}, {:.4}", e + 1,
            q.downweighted_fraction[e], q.rejected_fraction[e], q.median_weight[e],
            q.median_abs_residual_rad[e], q.outlier_score[e]);
    }
    if !q.flagged.is_empty() {
        let names: Vec<String> = q.flagged.iter().map(|e| (e + 1).to_string()).collect();
        log::warn!("Robust echo fit flagged echo(es) {} as inconsistent with the rest (motion or \
                    signal loss?); they were down-weighted where they departed, not dropped",
                   names.join(", "));
    }
}

/// The robust fit's per-echo report as JSON (echoes 1-based), for the field map's sidecar.
pub fn robust_fit_report_json(report: &qsm_core::pipeline::RobustFitReport) -> serde_json::Value {
    let q = &report.quality;
    serde_json::json!({
        "voxels_examined": q.voxels_examined,
        "downweighted_fraction": q.downweighted_fraction,
        "rejected_fraction": q.rejected_fraction,
        "median_weight": q.median_weight,
        "median_abs_residual_rad": q.median_abs_residual_rad,
        "outlier_score": q.outlier_score,
        "flagged_echoes": q.flagged.iter().map(|e| e + 1).collect::<Vec<_>>(),
    })
}

/// Hz per ppm of field at `field_strength` T: γ̄·B0·10⁻⁶, with the γ̄ QSM.rs converts by
/// (42.576 MHz/T), so the published Hz map is exactly the ppm one the inversions read.
pub fn hz_per_ppm(field_strength: f64) -> f64 {
    42.576e6 * field_strength * 1e-6
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsm_core::pipeline::config::UnwrappingAlgorithm;
    use qsm_core::unwrap::LaplacianSolver;
    use qsm_core::utils::{B0WeightType as W, EchoQuality, LinearFitParams};
    use std::f64::consts::PI;

    const N: usize = 8;

    fn meta(tes: &[f64]) -> ScanMetadata {
        ScanMetadata {
            dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), echo_times: tes.to_vec(),
            field_strength: 3.0, b0_direction: (0.0, 0.0, 1.0), slice_geometry: None, affine: None,
        }
    }

    /// Per-echo wrapped phase of a smooth field that wraps at the later echoes, and a magnitude
    /// decaying with T2* = 30 ms.
    fn echoes(tes: &[f64]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
        let wrap = |x: f64| (x + PI).rem_euclid(2.0 * PI) - PI;
        let n = N * N * N;
        let (mut ph, mut mg) = (vec![vec![0.0; n]; tes.len()], vec![vec![0.0; n]; tes.len()]);
        for k in 0..N { for j in 0..N { for i in 0..N {
            let idx = i + j * N + k * N * N;
            let omega = 600.0 * (PI * (i as f64 + 0.5) / N as f64).cos() + 300.0 * (PI * (j + k) as f64 / N as f64).sin();
            for (e, &te) in tes.iter().enumerate() {
                ph[e][idx] = wrap(te * omega);
                mg[e][idx] = (1.0 + 0.5 * (i as f64 / N as f64)) * (-te / 0.030).exp();
            }
        }}}
        (ph, mg)
    }

    fn refs(v: &[Vec<f64>]) -> Vec<&[f64]> { v.iter().map(|x| x.as_slice()).collect() }

    /// The entry point is qsm-core's field mapping with the config as given: on each unwrapping
    /// path, Laplacian solver and B0 estimator, with and without magnitude, its field is
    /// qsm-core's, and only the robust fit returns a report. How good each field is, QSM.rs's
    /// tests check (src/pipeline/field_mapping.rs, src/unwrap/laplacian.rs).
    #[test]
    fn settings_reach_qsm_core() {
        let tes = [0.004, 0.008, 0.012, 0.016];
        let (ph, mg) = echoes(&tes);
        let (ph, mg) = (refs(&ph), refs(&mg));
        let mask = vec![1u8; N * N * N];
        let lap = FieldMappingConfig { unwrapping_algorithm: UnwrappingAlgorithm::Laplacian, ..Default::default() };
        let weighted = |w| B0EstimationMethod::WeightedAvg { weighting: w };
        let configs = [
            lap.clone(),
            FieldMappingConfig { laplacian_solver: LaplacianSolver::Fft { pad: [2; 3] }, ..lap.clone() },
            FieldMappingConfig { b0_estimation: weighted(W::AssumedDecay { t2star_s: 0.040 }), ..lap.clone() },
            FieldMappingConfig { b0_estimation: B0EstimationMethod::LinearFit(LinearFitParams::default()), ..lap },
            FieldMappingConfig::default(),
            FieldMappingConfig { phase_offset_removal: false, b0_estimation: weighted(W::TEs), ..Default::default() },
            FieldMappingConfig {
                phase_offset_removal: false, b0_estimation: B0EstimationMethod::RobustFit(Default::default()),
                ..Default::default()
            },
        ];
        for cfg in &configs {
            for mags in [Some(&mg[..]), None] {
                let ours = run_field_mapping(&ph, mags, &mask, &meta(&tes), cfg).unwrap();
                let core = qsm_core::pipeline::run_field_mapping(&ph, mags, &mask, &meta(&tes), cfg, &mut |_, _| {}).unwrap();
                let d = ours.b0_field_ppm.iter().zip(&core.b0_field_ppm).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
                assert!(d < 1e-12, "{cfg:?}, magnitude {}: differs from qsm-core's by {d} ppm", mags.is_some());
                let robust = matches!(cfg.b0_estimation, B0EstimationMethod::RobustFit(_));
                assert_eq!(ours.robust_fit.is_some(), robust, "{cfg:?}");
            }
        }
    }

    /// The robust fit's report in the field map's sidecar numbers echoes from 1; the rest is
    /// passed through as qsm-core gives it.
    #[test]
    fn the_robust_fit_report_numbers_echoes_from_one() {
        let report = qsm_core::pipeline::RobustFitReport {
            quality: EchoQuality {
                downweighted_fraction: vec![0.01, 0.02, 0.6], rejected_fraction: vec![0.0; 3],
                median_weight: vec![1.0, 0.98, 0.4], median_abs_residual_rad: vec![0.01, 0.02, 0.5],
                outlier_score: vec![0.1, 0.2, 9.0], voxels_examined: 512, flagged: vec![0, 2],
            },
            robust_weights: vec![],
        };
        let json = robust_fit_report_json(&report);
        assert_eq!(json["flagged_echoes"], serde_json::json!([1, 3]));
        assert_eq!(json["voxels_examined"], serde_json::json!(512));
        assert_eq!(json["outlier_score"], serde_json::json!([0.1, 0.2, 9.0]));
        assert_eq!(json["downweighted_fraction"], serde_json::json!([0.01, 0.02, 0.6]));
        log_robust_fit_report(&report);
    }

    /// QSMxT's own checks refuse what qsm-core would take or fail on later: no echoes, echo
    /// times or magnitudes that do not match the echoes, a volume or mask off the grid,
    /// non-positive echo times, and a non-positive assumed T2*.
    #[test]
    fn rejects_bad_inputs() {
        let n = N * N * N;
        let p: &[f64] = &vec![0.0; n];
        let mask = vec![1u8; n];
        let cfg = FieldMappingConfig::default();
        let bad = |ph: &[&[f64]], mg: Option<&[&[f64]]>, mask: &[u8], tes: &[f64], cfg: &FieldMappingConfig| {
            validate(ph, mg, mask, &meta(tes), cfg).is_err() && run_field_mapping(ph, mg, mask, &meta(tes), cfg).is_err()
        };
        assert!(validate(&[p], Some(&[p]), &mask, &meta(&[0.01]), &cfg).is_ok());
        assert!(bad(&[], None, &mask, &[], &cfg));
        assert!(bad(&[p], None, &mask, &[0.01, 0.02], &cfg));
        assert!(bad(&[p], Some(&[p, p]), &mask, &[0.01], &cfg));
        assert!(bad(&[&p[1..]], None, &mask, &[0.01], &cfg));
        assert!(bad(&[p], Some(&[&p[1..]]), &mask, &[0.01], &cfg));
        assert!(bad(&[p], None, &mask[1..], &[0.01], &cfg));
        assert!(bad(&[p], None, &mask, &[0.0], &cfg));
        assert!(bad(&[p], None, &mask, &[f64::NAN], &cfg));
        let assumed = B0EstimationMethod::WeightedAvg { weighting: W::AssumedDecay { t2star_s: 0.0 } };
        assert!(bad(&[p], None, &mask, &[0.01], &FieldMappingConfig { b0_estimation: assumed, ..cfg }));
    }

    #[test]
    fn hz_per_ppm_is_the_gyromagnetic_ratio_at_the_field() {
        assert!((hz_per_ppm(3.0) - 127.728).abs() < 1e-9);
    }
}
