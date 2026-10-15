//! Phase scaling: every echo's phase in radians, on the working grid.

use crate::scan::RunMetadata;

/// The `scale_phase` step's parameters. `real_imaginary` is `Some(negate_phase)` for a run given
/// as real and imaginary parts: the sign convention follows from the sidecar, so it is a
/// parameter (and keys the cache).
pub fn params(meta: &RunMetadata, real_imaginary: Option<bool>) -> serde_json::Value {
    let mut params = serde_json::json!({
        "resampled_to_axial": meta.source_geometry.is_some(),
        "dims": [meta.dims.0, meta.dims.1, meta.dims.2],
    });
    if let Some(negate_phase) = real_imaginary {
        params["input"] = serde_json::json!("real_imaginary");
        params["negate_phase"] = serde_json::json!(negate_phase);
    }
    params
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

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(dims: (usize, usize, usize)) -> RunMetadata {
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        RunMetadata {
            dims, voxel_size: (1.0, 1.0, 1.0), affine, n_echoes: 1, echo_times: vec![0.01],
            b0_direction: (0.0, 0.0, 1.0), field_strength: 3.0, has_magnitude: true,
            source_geometry: None, slice_geometry: None,
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
    fn the_cache_key_names_real_imaginary_input() {
        let m = meta((2, 2, 2));
        assert!(params(&m, None).get("input").is_none());
        let p = params(&m, Some(true));
        assert_eq!(p["input"], "real_imaginary");
        assert_eq!(p["negate_phase"], true);
    }
}
