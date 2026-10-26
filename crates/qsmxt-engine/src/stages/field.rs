//! Field mapping: every echo's wrapped phase → the total B0 field, in ppm and in Hz.

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::scan::{scan_metadata, RunMetadata};

/// Whether the echoes' phase offsets are removed before unwrapping (multi-echo, asked for, and
/// not Laplacian unwrapping, which removes them by construction, nor MEDI's complex fit, which
/// fits its own phase offset).
fn removes_phase_offsets(config: &PipelineConfig, meta: &RunMetadata) -> bool {
    meta.n_echoes > 1 && config.field_mapping.phase_offset_removal
        && config.field_mapping.unwrapping_algorithm != crate::enums::UnwrappingAlgorithm::Laplacian
        && config.field_mapping.b0_estimation != crate::enums::B0Estimation::ComplexFit
}

/// The algorithm the `unwrap` step is recorded and cached under: `phase_offset_removal` when the
/// offsets are removed first, the unwrapping algorithm otherwise.
pub fn algorithm(config: &PipelineConfig, meta: &RunMetadata) -> String {
    if removes_phase_offsets(config, meta) {
        "phase_offset_removal".into()
    } else {
        format!("{}", config.field_mapping.unwrapping_algorithm)
    }
}

/// The `unwrap` step's parameters.
pub fn params(config: &PipelineConfig, meta: &RunMetadata) -> serde_json::Value {
    let fm = &config.field_mapping;
    let laplacian = fm.unwrapping_algorithm == crate::enums::UnwrappingAlgorithm::Laplacian;
    let mut params = serde_json::json!({
        "n_echoes": meta.n_echoes,
        "phase_offset_removal": fm.phase_offset_removal,
        "bipolar_correction": fm.bipolar_correction,
        "romeo_individual": fm.romeo.individual,
        "romeo_correct_global": fm.romeo.correct_global,
        "echo_times": meta.echo_times,
        "field_strength": meta.field_strength,
        // The echo combination: changing it must not reuse a field map cached under another.
        "b0_estimation": format!("{}", fm.b0_estimation),
        "b0_weight_type": format!("{}", fm.b0_weight_type),
        "b0_weight_assumed_t2star_ms": fm.b0_weight_assumed_t2star_ms,
        "linear_fit_estimate_offset": fm.linear_fit.estimate_offset,
        "linear_fit_reliability_threshold": fm.linear_fit.reliability_threshold_percentile,
        "robust_fit": fm.robust_fit,
        "complex_fit": fm.complex_fit,
        // Per-echo masked Laplacian + echo combination (crate::fieldmap); bump on a change to
        // that path so field maps from the old one are recomputed.
        "laplacian_echo_combination": if laplacian { 2 } else { 0 },
        "laplacian_solver": format!("{}", fm.laplacian_solver),
        "laplacian_fft_pad": fm.laplacian_fft_pad,
    });
    if fm.b0_estimation == crate::enums::B0Estimation::QsmartEchofit {
        params["qsmart_fit_threshold"] = match fm.qsmart_fit_threshold() {
            qsm_core::utils::QsmartFitThreshold::Percentile(p) => serde_json::json!({ "percentile": p }),
            qsm_core::utils::QsmartFitThreshold::Absolute(t) => serde_json::json!({ "absolute": t }),
        };
    }
    params
}

/// The total field of a run.
pub struct FieldMap {
    /// The B0 field offset in ppm, zero outside the mask (QSMART's echo fit: over the whole
    /// volume, as QSMART fits it): what the inversions read.
    pub b0_field_ppm: Vec<f64>,
    /// The same field in Hz, as BIDS publishes a `fieldmap` (it takes Hz, rad/s or T, not ppm).
    pub total_field_hz: Vec<f64>,
    /// The robust echo fit's per-echo report ([`crate::fieldmap::robust_fit_report_json`]), when
    /// the echoes were combined by a robust fit.
    pub robust_fit_report: Option<serde_json::Value>,
    /// QSMART's reliability map `R_0` (1 = reliable), when the echoes were combined by QSMART's
    /// echo fit, over the whole volume: QSMART reconstructs where it and the brain mask are 1.
    pub reliability: Option<Vec<u8>>,
    /// The blurred fit residual `R_0` was cut at, when the echoes were combined by QSMART's echo
    /// fit: the value the percentile resolved to, or the fixed threshold.
    pub qsmart_fit_threshold: Option<f64>,
}

/// Map the total field from every echo's wrapped phase (radians) on the run's working grid, inside
/// `mask` ([`crate::fieldmap::run_field_mapping`], with everything from `config`). `magnitudes`
/// holds one per echo, or none (every echo and voxel weighted equally).
pub fn map_field(
    phases: &[&[f64]], magnitudes: &[&[f64]], mask: &[u8], meta: &RunMetadata,
    config: &PipelineConfig,
) -> Result<FieldMap, EngineError> {
    let fm = &config.field_mapping;
    let unwrap_name = format!("{}", fm.unwrapping_algorithm);
    if fm.b0_estimation == crate::enums::B0Estimation::ComplexFit {
        log::info!("Field mapping: complex fit of {} echoes (Fit_ppm_complex), then {} unwrapping of the fitted phase",
            meta.n_echoes, unwrap_name);
    } else if removes_phase_offsets(config, meta) {
        log::info!("Field mapping: offset removal + {} unwrapping ({} echoes)", unwrap_name, meta.n_echoes);
    } else if meta.n_echoes > 1 {
        log::info!("Field mapping: {} unwrapping ({} echoes)", unwrap_name, meta.n_echoes);
    } else {
        log::info!("Phase unwrapping ({}, single echo)", unwrap_name);
    }
    if fm.b0_estimation == crate::enums::B0Estimation::QsmartEchofit {
        let threshold = match fm.qsmart_fit_threshold() {
            qsm_core::utils::QsmartFitThreshold::Percentile(p) => format!("the {p}th percentile of the blurred residual"),
            qsm_core::utils::QsmartFitThreshold::Absolute(t) => format!("a residual of {t}"),
        };
        log::info!("Laplacian unwrapping per echo over the whole volume ({} solver), then QSMART's echo fit \
                    (reliability threshold: {threshold})", fm.laplacian_solver);
    } else if fm.unwrapping_algorithm == crate::enums::UnwrappingAlgorithm::Laplacian && meta.n_echoes > 1 {
        log::info!("Laplacian unwrapping per echo ({} solver), then {} ({} weighting)",
            fm.laplacian_solver, fm.b0_estimation, fm.b0_weight_type);
    }

    let (fm_config, _, _, _) = crate::bridge::to_pipeline_stages(config);
    let mags = if magnitudes.is_empty() { None } else { Some(magnitudes) };
    let field = crate::fieldmap::run_field_mapping(phases, mags, mask, &scan_metadata(meta), &fm_config)
        .map_err(|e| EngineError::Config(format!("field mapping: {}", e)))?;

    // QSM.rs's γ̄, so the published Hz map is exactly the ppm one the inversions read.
    let total_field_hz = qsm_core::pipeline::ppm_to_hz(&field.b0_field_ppm, meta.field_strength);
    let robust_fit_report = field.robust_fit.as_ref().map(|report| {
        crate::fieldmap::log_robust_fit_report(report);
        crate::fieldmap::robust_fit_report_json(report)
    });
    if let Some(t) = field.qsmart_fit_threshold {
        crate::fieldmap::log_qsmart_fit_threshold(t);
    }
    Ok(FieldMap {
        b0_field_ppm: field.b0_field_ppm, total_field_hz, robust_fit_report, reliability: field.reliability,
        qsmart_fit_threshold: field.qsmart_fit_threshold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::UnwrappingAlgorithm;

    fn meta(n_echoes: usize) -> RunMetadata {
        let mut m = crate::stages::magnitude::tests::meta_4x4x4();
        m.n_echoes = n_echoes;
        m.echo_times = (1..=n_echoes).map(|e| 0.005 * e as f64).collect();
        m
    }

    #[test]
    fn the_unwrap_step_is_named_for_offset_removal_only_when_it_happens() {
        let mut config = PipelineConfig::default();
        config.field_mapping.phase_offset_removal = true;
        config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Romeo;
        assert_eq!(algorithm(&config, &meta(3)), "phase_offset_removal");
        assert_eq!(algorithm(&config, &meta(1)), "romeo", "a single echo has no offsets to remove");
        config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        assert_eq!(algorithm(&config, &meta(3)), "laplacian");
        assert_eq!(params(&config, &meta(3))["laplacian_echo_combination"], 2);
    }

    #[test]
    fn the_published_field_is_the_ppm_field_in_hz() {
        let m = meta(1);
        let n = 64;
        let phase = vec![0.3f64; n];
        let mag = vec![1.0f64; n];
        let mask = vec![1u8; n];
        let config = PipelineConfig::default();
        let f = map_field(&[&phase], &[&mag], &mask, &m, &config).unwrap();
        let k = qsm_core::constants::GAMMA_BAR_HZ_PER_T * m.field_strength * 1e-6;
        assert!(f.b0_field_ppm.iter().zip(&f.total_field_hz).all(|(p, h)| *h == p * k));
        assert!(f.robust_fit_report.is_none());
        assert!(f.qsmart_fit_threshold.is_none());

        let err = map_field(&[&phase], &[&mag], &mask[..10], &m, &config).err().unwrap();
        assert!(format!("{err}").starts_with("field mapping:"), "{err}");
    }
}
