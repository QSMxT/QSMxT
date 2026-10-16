//! Retrospective motion correction: every echo rigidly co-registered onto the reference echo
//! (estimated on magnitude, applied to phase through the complex signal), and the mask shrunk to
//! where every echo still has data.

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::scan::RunMetadata;

/// The `motion` step's parameters.
pub fn params(config: &PipelineConfig, meta: &RunMetadata) -> serde_json::Value {
    serde_json::json!({
        "motion_correction": config.motion_correction, "registration": config.registration,
        "n_echoes": meta.n_echoes,
    })
}

/// Correct every echo's `phases` and `magnitudes` for motion inside the brain `mask`.
pub fn correct(
    phases: &[&[f64]], magnitudes: &[&[f64]], mask: &[u8], meta: &RunMetadata, config: &PipelineConfig,
) -> Result<qsm_core::pipeline::MotionCorrectionResult, EngineError> {
    let scan = crate::bridge::to_scan_metadata(
        meta.dims, meta.voxel_size, &meta.echo_times, meta.field_strength, meta.b0_direction, Some(meta.affine));
    qsm_core::pipeline::run_motion_correction(
        phases, magnitudes, mask, &scan, &config.motion_correction.to_core(&config.registration), &mut |_, _| {},
    ).map_err(|e| EngineError::Config(format!("motion correction: {e}")))
}

/// The motion report: per echo (1-based) rotation, translation, largest tissue displacement,
/// NCC, overlap and whether the registration is suspect, plus the series' extremes.
pub fn report(r: &qsm_core::pipeline::MotionCorrectionResult) -> serde_json::Value {
    serde_json::json!({
        "reference_echo": r.motion.reference + 1,
        "max_displacement_mm": r.max_displacement_mm(),
        "rotation_spread_deg": r.rotation_spread_deg(),
        "echoes": r.motion.volumes.iter().map(|v| serde_json::json!({
            "echo": v.index + 1,
            "rotation_deg": v.rotation_deg,
            "translation_mm": v.translation_mm,
            "max_displacement_mm": v.max_displacement_mm,
            "ncc": v.ncc,
            "overlap": v.overlap,
            "suspect": v.suspect,
            "b0_direction": [v.bdir.0, v.bdir.1, v.bdir.2],
        })).collect::<Vec<_>>(),
    })
}

/// What the log says about a correction: each echo's motion, the series' extremes, any echo whose
/// registration may have failed, and how much the mask (`mask_in` before) shrank. Each line with
/// its level.
pub fn summary(r: &qsm_core::pipeline::MotionCorrectionResult, mask_in: &[u8]) -> Vec<(log::Level, String)> {
    let mut lines: Vec<(log::Level, String)> = r.motion.volumes.iter().map(|v| (log::Level::Info, format!(
        "  echo {}: rotation {:.2}°, translation {:.2} mm, max displacement {:.2} mm, NCC {:.3}, overlap {:.2}{}",
        v.index + 1, v.rotation_deg, v.translation_mm, v.max_displacement_mm, v.ncc, v.overlap,
        if v.suspect { "  [suspect registration]" } else { "" }))).collect();
    lines.push((log::Level::Info, format!(
        "Motion: max displacement {:.2} mm, B0 direction spread {:.2}° (field mapping uses the reference echo's)",
        r.max_displacement_mm(), r.rotation_spread_deg())));
    let suspect: Vec<String> = r.motion.volumes.iter().filter(|v| v.suspect).map(|v| (v.index + 1).to_string()).collect();
    if !suspect.is_empty() {
        lines.push((log::Level::Warn, format!(
            "Motion correction: the registration of echo(es) {} may have failed (low NCC or overlap); \
             check the motion report", suspect.join(", "))));
    }
    let count = |m: &[u8]| m.iter().filter(|&&v| v > 0).count();
    let (before, after) = (count(mask_in), count(&r.mask));
    if after < before {
        lines.push((log::Level::Info, format!("Mask restricted to voxels every echo covers: {before} -> {after} voxels")));
    }
    lines
}
