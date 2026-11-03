//! Phase scaling: every echo's phase in radians, on the working grid — from the echoes as given,
//! from real and imaginary parts, or combined from uncombined receive coils.

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::scan::RunMetadata;

/// The `scale_phase` step's parameters. `real_imaginary` is `Some(negate_phase)` for a run given
/// as real and imaginary parts: the sign convention follows from the sidecar, so it is a
/// parameter (and keys the cache).
pub fn params(meta: &RunMetadata, real_imaginary: Option<bool>, config: &PipelineConfig) -> serde_json::Value {
    let mut params = serde_json::json!({
        "resampled_to_axial": meta.source_geometry.is_some(),
        "dims": [meta.dims.0, meta.dims.1, meta.dims.2],
    });
    if let Some(negate_phase) = real_imaginary {
        params["input"] = serde_json::json!("real_imaginary");
        params["negate_phase"] = serde_json::json!(negate_phase);
    }
    add_tukey_window(&mut params, config);
    params
}

/// The k-space Tukey window's taper ratio, in a `scale_phase` step's parameters when it is on (a
/// run without it keeps its parameters, and its cache key, as they were).
fn add_tukey_window(params: &mut serde_json::Value, config: &PipelineConfig) {
    if config.pipeline.tukey_window > 0.0 {
        params["tukey_window"] = serde_json::json!(config.pipeline.tukey_window);
    }
}

/// The k-space Tukey window (`--tukey-window`, qsm-core `utils::tukey_window`) on one echo on
/// the acquired grid, in place: `phase` in radians. Nothing when the taper ratio is 0.
pub fn tukey_window(magnitude: &mut [f64], phase: &mut [f64], meta: &RunMetadata, config: &PipelineConfig) {
    let factor = config.pipeline.tukey_window;
    if factor > 0.0 {
        qsm_core::utils::tukey_window::tukey_window_mag_phase(magnitude, phase, acquired_grid(meta).dims, factor);
    }
}

/// The log line for a run that applies the Tukey window, if it does.
pub fn tukey_window_line(config: &PipelineConfig) -> Option<String> {
    (config.pipeline.tukey_window > 0.0).then(|| format!(
        "k-space Tukey window (taper ratio {}) applied to every echo's complex image", config.pipeline.tukey_window))
}

/// One echo on the working grid.
pub struct WorkingEcho {
    /// Phase in radians.
    pub phase: Vec<f64>,
    /// The magnitude that came with it: resampled with the phase when the run is resampled to an
    /// axial grid (uniform when none was given), otherwise as given.
    pub magnitude: Option<Vec<f64>>,
}

/// Bring one echo onto the working grid, in radians.
///
/// `phase` is the echo as stored (any range; rescaled to [−π, π]), or already in radians when
/// `in_radians` (real/imaginary input, converted before this). When the run is resampled to an
/// axial grid ([`RunMetadata::source_geometry`]), magnitude and phase are resampled together
/// through the complex domain, because interpolating wrapped phase on its own turns every wrap
/// into a band of wrong values. Without a magnitude every voxel is weighted equally, so the phase
/// still resamples correctly.
pub fn working_echo(
    mut phase: Vec<f64>, magnitude: Option<Vec<f64>>, in_radians: bool, meta: &RunMetadata,
) -> WorkingEcho {
    if !in_radians {
        qsm_core::pipeline::scale_phase_to_pi(&mut phase);
    }
    let Some(((nx, ny, nz), src_affine)) = meta.source_geometry else {
        return WorkingEcho { phase, magnitude };
    };
    let magnitude = magnitude.unwrap_or_else(|| vec![1.0; phase.len()]);
    let out = qsm_core::geometry::resample_complex_to_axial(
        &magnitude, &phase, nx, ny, nz, &src_affine,
        &qsm_core::geometry::AxialResampleParams::default(),
    );
    WorkingEcho { phase: out.phase, magnitude: Some(out.magnitude) }
}

/// Whether real/imaginary input from this manufacturer has its phase negated on conversion.
///
/// GE's real/imaginary pair gives phase of the opposite sign to the Siemens/Philips convention
/// the pipeline assumes: on one subject scanned on all three, high-pass field maps from GE
/// anti-correlate with the other two, which correlate with each other. Matched as dcm2niix and
/// the DICOM tag spell it ("GE", "GE MEDICAL SYSTEMS", "General Electric").
pub fn negates_real_imaginary_phase(manufacturer: Option<&str>) -> bool {
    let Some(m) = manufacturer else { return false };
    let m = m.trim().to_uppercase();
    m == "GE" || m.starts_with("GE ") || m.contains("GENERAL ELECTRIC")
}

/// The grid the echoes were acquired on: the run's, or the one it was resampled from.
pub fn acquired_grid(meta: &RunMetadata) -> qsm_core::Grid {
    let (dims, voxel) = match meta.source_geometry {
        Some((d, a)) => (d, qsm_core::geometry::voxel_sizes_from_affine(&a)),
        None => (meta.dims, meta.voxel_size),
    };
    qsm_core::Grid::new(dims.0, dims.1, dims.2, voxel.0, voxel.1, voxel.2)
}

/// Real/imaginary echoes as magnitude and phase (radians) on the acquired grid, by
/// `qsm_core::complex::complex_to_mag_phase` (which also detects and undoes slice chopping, and
/// negates the phase when `negate_phase`), with what the log says about it.
pub fn real_imaginary(
    real: &[Vec<f64>], imaginary: &[Vec<f64>], meta: &RunMetadata, negate_phase: bool,
) -> (qsm_core::complex::MagPhaseEchoes, Vec<(log::Level, String)>) {
    let real_refs: Vec<&[f64]> = real.iter().map(|v| v.as_slice()).collect();
    let imag_refs: Vec<&[f64]> = imaginary.iter().map(|v| v.as_slice()).collect();
    let params = qsm_core::complex::ComplexToMagPhaseParams {
        chop: qsm_core::complex::ChopCorrection::Auto,
        negate_phase,
    };
    let out = qsm_core::complex::complex_to_mag_phase(&real_refs, &imag_refs, &acquired_grid(meta), &params);
    let mut log = Vec::new();
    if negate_phase {
        log.push((log::Level::Info, "Real/imaginary input: GE phase sign convention, phase negated".to_string()));
    }
    let c = &out.chopping;
    let axes: Vec<&str> = ["x", "y", "z"].into_iter().zip(c.corrected).filter(|(_, on)| *on).map(|(a, _)| a).collect();
    log.push((log::Level::Info, format!(
        "Real/imaginary input: neighbour phase coherence x {:+.3}, y {:+.3}, z {:+.3}; {}",
        c.coherence[0], c.coherence[1], c.coherence[2],
        if axes.is_empty() { "no slice chopping".to_string() } else { format!("slice chopping corrected along {}", axes.join(", ")) })));
    let ambiguous = c.ambiguous_axes();
    if !ambiguous.is_empty() {
        let names: Vec<&str> = ambiguous.iter().map(|&a| ["x", "y", "z"][a]).collect();
        log.push((log::Level::Warn, format!(
            "Real/imaginary input: slice-chopping detection is uncertain along {} (coherence near zero); \
             check the phase for a π jump between neighbouring slices", names.join(", "))));
    }
    (out, log)
}

/// What a real/imaginary conversion found, recorded beside the step's parameters.
pub fn slice_chopping_json(c: &qsm_core::complex::SliceChopping) -> serde_json::Value {
    serde_json::json!({
        "method": "complex_to_mag_phase",
        "detected": c.detected,
        "coherence": { "x": c.coherence[0], "y": c.coherence[1], "z": c.coherence[2] },
        "corrected": { "x": c.corrected[0], "y": c.corrected[1], "z": c.corrected[2] },
    })
}

/// The coil combination's parameters (the `scale_phase` step's, for uncombined coils).
pub fn coil_params(config: &PipelineConfig, meta: &RunMetadata, coil_numbers: &[u32]) -> serde_json::Value {
    let mut p = serde_json::json!({
        "coil_combination": "mcpc3ds",
        "n_coils": coil_numbers.len(),
        "n_echoes": meta.n_echoes,
        "coil_numbers": coil_numbers,
        "hip_echoes": [1, 2],
        "sigma": config.field_mapping.coil_combination_sigma,
        "hip_unwrapping": format!("{}", config.field_mapping.unwrapping_algorithm),
        "echo_times": meta.echo_times,
        "resampled_to_axial": meta.source_geometry.is_some(),
    });
    add_tukey_window(&mut p, config);
    // PRELUDE's settings reach the HIP unwrap (`hip_unwrap_method`), so they key the step; only
    // when PRELUDE runs, so other runs keep their keys.
    if let qsm_core::unwrap::UnwrapMethod::Prelude(pp) = hip_unwrap_method(config) {
        p["prelude"] = serde_json::json!({
            "labelling": pp.labelling.id(),
            "num_phase_partitions": pp.num_phase_partitions,
        });
    }
    p
}

/// How MCPC-3D-S unwraps the HIP when combining coils: with the run's unwrapper, and with
/// PRELUDE's configured settings when that is PRELUDE.
pub fn hip_unwrap_method(config: &PipelineConfig) -> qsm_core::unwrap::UnwrapMethod {
    match config.field_mapping.unwrapping_algorithm {
        crate::enums::UnwrappingAlgorithm::Romeo => qsm_core::unwrap::UnwrapMethod::Romeo,
        crate::enums::UnwrappingAlgorithm::Laplacian => qsm_core::unwrap::UnwrapMethod::Laplacian,
        crate::enums::UnwrappingAlgorithm::Prelude => {
            qsm_core::unwrap::UnwrapMethod::Prelude((&config.field_mapping.prelude).into())
        }
    }
}

/// Uncombined coils' echoes combined by MCPC-3D-S on the acquired grid: `phases[coil][echo]` in
/// radians and `magnitudes` likewise. Each coil's phase offset is estimated from the difference of
/// the first two echoes, unwrapped with the run's unwrapper and its settings
/// ([`hip_unwrap_method`]).
pub fn combine_coils(
    phases: &[Vec<Vec<f64>>], magnitudes: &[Vec<Vec<f64>>], meta: &RunMetadata, config: &PipelineConfig,
) -> Result<qsm_core::fieldmap::CoilCombinationResult, EngineError> {
    let unwrap = hip_unwrap_method(config);
    Ok(qsm_core::fieldmap::mcpc3ds_combine(
        phases, magnitudes, &meta.echo_times, config.field_mapping.coil_combination_sigma, [0, 1], unwrap,
        &acquired_grid(meta),
    )?)
}

/// One combined echo on the working grid: combined on the acquired grid (all coils share it),
/// then the single combined pair resampled rather than every coil's.
pub fn combined_echo(magnitude: &[f64], phase: &[f64], meta: &RunMetadata) -> WorkingEcho {
    match meta.source_geometry {
        Some((sd, sa)) => {
            let r = qsm_core::geometry::resample_complex_to_axial(
                magnitude, phase, sd.0, sd.1, sd.2, &sa, &qsm_core::geometry::AxialResampleParams::default());
            WorkingEcho { phase: r.phase, magnitude: Some(r.magnitude) }
        }
        None => WorkingEcho { phase: phase.to_vec(), magnitude: Some(magnitude.to_vec()) },
    }
}

/// The mask MCPC-3D-S estimated the coil offsets in, on the working grid.
pub fn combination_mask(mask: &[u8], meta: &RunMetadata) -> Vec<u8> {
    match meta.source_geometry {
        Some((sd, sa)) => qsm_core::geometry::resample_mask_to_axial(mask, sd.0, sd.1, sd.2, &sa),
        None => mask.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(dims: (usize, usize, usize)) -> RunMetadata {
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        RunMetadata {
            dims, voxel_size: (1.0, 1.0, 1.0), affine, n_echoes: 1, echo_times: vec![0.01],
            b0_direction: (0.0, 0.0, 1.0), field_strength: 3.0, has_magnitude: true,
            source_geometry: None, slice_geometry: None, orientation: None,
        }
    }

    #[test]
    fn scanner_units_are_rescaled_and_the_magnitude_passes_through() {
        let m = meta((3, 1, 1));
        let out = working_echo(vec![0.0, 2048.0, 4096.0], Some(vec![1.0, 2.0, 3.0]), false, &m);
        assert!((out.phase[0] + std::f64::consts::PI).abs() < 1e-12);
        assert!((out.phase[2] - std::f64::consts::PI).abs() < 1e-12);
        assert_eq!(out.magnitude, Some(vec![1.0, 2.0, 3.0]));

        let radians = vec![0.5, -0.5, 1.0];
        assert_eq!(working_echo(radians.clone(), None, true, &m).phase, radians, "already radians");
    }

    #[test]
    fn ge_real_imaginary_phase_is_negated() {
        for m in ["GE", "GE MEDICAL SYSTEMS", "General Electric Healthcare", " ge "] {
            assert!(negates_real_imaginary_phase(Some(m)), "{m}");
        }
        for m in ["Siemens", "Philips", "GEMINI", "SIEMENS HEALTHINEERS"] {
            assert!(!negates_real_imaginary_phase(Some(m)), "{m}");
        }
        assert!(!negates_real_imaginary_phase(None));
    }

    /// PRELUDE's labelling and partitions reach MCPC-3D-S's HIP unwrap and key the step; other
    /// unwrappers' keys carry no PRELUDE settings.
    #[test]
    fn coil_combination_runs_prelude_with_the_configured_settings() {
        use crate::enums::{PreludeLabelling, UnwrappingAlgorithm};
        let (nx, ny, nz) = (12, 6, 4);
        let mut m = meta((nx, ny, nz));
        m.n_echoes = 2;
        m.echo_times = vec![0.005, 0.010];
        let n = nx * ny * nz;
        // Two coils, a field steep enough that the HIP wraps, and per-coil offsets.
        type Coils = Vec<Vec<Vec<f64>>>;
        let (phases, mags): (Coils, Coils) = (0..2)
            .map(|c| {
                let p = m.echo_times.iter().map(|&te| (0..n).map(|i| {
                    let t = 500.0 * ((i % nx) as f64 - 5.5) * te + 0.7 * c as f64;
                    t.sin().atan2(t.cos())
                }).collect()).collect();
                let a = (0..2).map(|_| (0..n).map(|i| 1.0 + ((i + c) % 3) as f64).collect()).collect();
                (p, a)
            })
            .unzip();
        let mut cfg = PipelineConfig::default();
        cfg.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Prelude;
        assert_eq!(hip_unwrap_method(&cfg), qsm_core::unwrap::UnwrapMethod::Prelude(Default::default()));
        let default_run = combine_coils(&phases, &mags, &m, &cfg).unwrap();
        let default_key = coil_params(&cfg, &m, &[1, 2]);
        assert_eq!(default_key["prelude"]["num_phase_partitions"], 8);

        cfg.field_mapping.prelude.num_phase_partitions = 1;
        cfg.field_mapping.prelude.labelling = PreludeLabelling::Volume;
        let want = qsm_core::unwrap::PreludeParams {
            num_phase_partitions: 1,
            labelling: qsm_core::unwrap::PreludeLabelling::Volume,
            ..Default::default()
        };
        assert_eq!(hip_unwrap_method(&cfg), qsm_core::unwrap::UnwrapMethod::Prelude(want));
        let set_run = combine_coils(&phases, &mags, &m, &cfg).unwrap();
        assert_ne!(set_run.phases, default_run.phases, "the settings did not reach the HIP unwrap");
        let key = coil_params(&cfg, &m, &[1, 2]);
        assert_eq!(key["prelude"], serde_json::json!({"labelling": "volume", "num_phase_partitions": 1}));
        assert_ne!(key, default_key);

        cfg.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Romeo;
        assert!(coil_params(&cfg, &m, &[1, 2]).get("prelude").is_none());
    }

    /// The Tukey window keys the step only when it is on, and windows the echo on the acquired
    /// grid as qsm-core does.
    #[test]
    fn the_tukey_window_keys_the_step_and_reaches_qsm_core() {
        let m = meta((6, 5, 4));
        let mut c = PipelineConfig::default();
        assert!(params(&m, None, &c).get("tukey_window").is_none());
        assert!(coil_params(&c, &m, &[1, 2]).get("tukey_window").is_none());
        let (mut mag, mut phase) = (vec![1.0; 120], (0..120).map(|i| (i % 7) as f64 * 0.3 - 1.0).collect::<Vec<f64>>());
        let (m0, p0) = (mag.clone(), phase.clone());
        tukey_window(&mut mag, &mut phase, &m, &c);
        assert_eq!((&mag, &phase), (&m0, &p0), "0 applies none");
        assert!(tukey_window_line(&c).is_none());

        c.pipeline.tukey_window = 0.4;
        assert_eq!(params(&m, None, &c)["tukey_window"], 0.4);
        assert_eq!(coil_params(&c, &m, &[1, 2])["tukey_window"], 0.4);
        tukey_window(&mut mag, &mut phase, &m, &c);
        let (mut want_m, mut want_p) = (m0, p0);
        qsm_core::utils::tukey_window::tukey_window_mag_phase(&mut want_m, &mut want_p, (6, 5, 4), 0.4);
        assert_eq!((mag, phase), (want_m, want_p));
        assert!(tukey_window_line(&c).unwrap().contains("0.4"));
    }

    #[test]
    fn the_cache_key_names_real_imaginary_input() {
        let m = meta((2, 2, 2));
        let c = PipelineConfig::default();
        assert!(params(&m, None, &c).get("input").is_none());
        let p = params(&m, Some(true), &c);
        assert_eq!(p["input"], "real_imaginary");
        assert_eq!(p["negate_phase"], true);
    }
}
