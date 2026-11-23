//! Relaxometry: T2* and R2* from the multi-echo magnitude.

use crate::error::EngineError;
use crate::scan::RunMetadata;

/// The `t2star_r2star` step's parameters.
pub fn t2star_r2star_params(meta: &RunMetadata) -> serde_json::Value {
    serde_json::json!({
        "n_echoes": meta.n_echoes,
        "echo_times": meta.echo_times,
    })
}

/// Every echo's magnitude, voxel-major (`[voxel0_echo0, voxel0_echo1, ..., voxel1_echo0, ...]`):
/// the layout the voxel-wise fits take. Filled one echo at a time, so only one echo is held
/// besides it.
pub struct VoxelMajor {
    pub data: Vec<f64>,
    n_voxels: usize,
    n_echoes: usize,
}

impl VoxelMajor {
    pub fn new(n_voxels: usize, n_echoes: usize) -> Self {
        Self { data: vec![0.0; n_voxels * n_echoes], n_voxels, n_echoes }
    }

    /// Put echo `e` (0-based) in place. `what` names the echo in the error when it is not on the
    /// grid.
    pub fn set(&mut self, e: usize, echo: &[f64], what: &str) -> Result<(), EngineError> {
        if echo.len() != self.n_voxels {
            return Err(EngineError::Config(format!(
                "{what} has {} voxels, but this run is processed on a grid of {}", echo.len(), self.n_voxels)));
        }
        for (vox, &v) in echo.iter().enumerate() {
            self.data[vox * self.n_echoes + e] = v;
        }
        Ok(())
    }
}

/// R2* (Hz) by ARLO from every echo's magnitude (`magnitudes`, voxel-major) inside `mask`, and
/// T2* (s) from it.
pub fn t2star_r2star(magnitudes: &[f64], mask: &[u8], meta: &RunMetadata) -> Result<(Vec<f64>, Vec<f64>), EngineError> {
    let ((nx, ny, nz), (vx, vy, vz)) = (meta.dims, meta.voxel_size);
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    // PROBE: r2star_arlo removed; arlo() is echo-major, unmasked, no S0, no log-linear fallback (QSM.rs#264)
    let _ = &grid;
    let n = mask.len(); let ne = meta.echo_times.len();
    let echo_major: Vec<f64> = (0..ne).flat_map(|e| (0..n).map(move |v| magnitudes[v * ne + e])).collect();
    let mut r2star = qsm_core::r2star::arlo(&echo_major, &meta.echo_times, n)?;
    for (r, &m) in r2star.iter_mut().zip(mask) { if m == 0 { *r = 0.0; } }
    // R2* is 0 outside the mask, so T2* is too.
    let t2star = qsm_core::r2star::t2star_from_r2star(&r2star);
    Ok((r2star, t2star))
}
