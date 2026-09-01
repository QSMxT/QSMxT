//! Shared helpers for standalone CLI commands.

use std::path::{Path, PathBuf};
use qsm_core::io::{self, NiftiData};
use crate::error::QsmxtError;

/// Full NIfTI data plus its 4D dimensions `(nx, ny, nz, n_vol)`.
pub type Nifti4d = (Vec<f64>, (usize, usize, usize, usize));

/// Read a NIfTI (3D or 4D, `.nii`/`.nii.gz`) as its full data plus `(nx, ny, nz, n_vol)`.
/// Unlike [`load_nifti`], this keeps every volume of a 4D file (echo-major layout).
pub fn read_nifti_any(path: &Path) -> crate::Result<Nifti4d> {
    let bytes = std::fs::read(path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", path.display(), e)))?;
    let (data, dims4, _voxel, _affine) = io::load_nifti_4d(&bytes)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", path.display(), e)))?;
    Ok((data, dims4))
}

/// Load multi-echo magnitude from **either** multiple 3D files (one per echo) **or** a single 4D
/// file, returning voxel-major `(n_voxels * n_echoes)` data and the echo count. Dimensions are
/// validated against `n_voxels` with a clear error rather than a deep panic.
pub fn load_multiecho_voxel_major(paths: &[PathBuf], n_voxels: usize) -> crate::Result<(Vec<f64>, usize)> {
    if paths.is_empty() {
        return Err(QsmxtError::Config("no magnitude file provided".into()));
    }
    if paths.len() == 1 {
        // Single file — may be 3D (one echo) or 4D (all echoes).
        let (data, (nx, ny, nz, nt)) = read_nifti_any(&paths[0])?;
        let nvox = nx * ny * nz;
        if nvox != n_voxels {
            return Err(QsmxtError::Config(format!(
                "magnitude {} has {} voxels but the reference volume has {}",
                paths[0].display(), nvox, n_voxels)));
        }
        // load_nifti_4d gives echo-major (t slowest: data[e*n_voxels + v]).
        let mut out = vec![0.0f64; n_voxels * nt];
        for e in 0..nt {
            for v in 0..n_voxels {
                out[v * nt + e] = data[e * n_voxels + v];
            }
        }
        Ok((out, nt))
    } else {
        // One 3D file per echo.
        let n_echoes = paths.len();
        let mut out = vec![0.0f64; n_voxels * n_echoes];
        for (e, p) in paths.iter().enumerate() {
            let (data, (nx, ny, nz, nt)) = read_nifti_any(p)?;
            if nt != 1 {
                return Err(QsmxtError::Config(format!(
                    "{} is 4D ({} volumes) — pass a single 4D file OR one 3D file per echo, not a mix",
                    p.display(), nt)));
            }
            if nx * ny * nz != n_voxels {
                return Err(QsmxtError::Config(format!(
                    "magnitude echo {} ({}) has {} voxels but the reference volume has {}",
                    e + 1, p.display(), nx * ny * nz, n_voxels)));
            }
            for v in 0..n_voxels {
                out[v * n_echoes + e] = data[v];
            }
        }
        Ok((out, n_echoes))
    }
}

/// Load a magnitude as a single RSS-combined volume, handling a 3D file (used as-is) or a 4D
/// multi-echo file (root-sum-of-squares over echoes). For weighting inputs that want one volume.
pub fn load_magnitude_rss(path: &Path, n_voxels: usize) -> crate::Result<Vec<f64>> {
    let (multi, n_echoes) = load_multiecho_voxel_major(std::slice::from_ref(&path.to_path_buf()), n_voxels)?;
    if n_echoes == 1 {
        Ok(multi)
    } else {
        Ok(rss_over_echoes(&multi, n_voxels, n_echoes))
    }
}

/// Root-sum-of-squares over echoes of a voxel-major `(n_voxels, n_echoes)` magnitude.
pub fn rss_over_echoes(multi: &[f64], n_voxels: usize, n_echoes: usize) -> Vec<f64> {
    let mut out = vec![0.0f64; n_voxels];
    for (v, o) in out.iter_mut().enumerate() {
        let mut s = 0.0;
        for e in 0..n_echoes {
            let x = multi[v * n_echoes + e];
            s += x * x;
        }
        *o = s.sqrt();
    }
    out
}

/// Create a Grid from NIfTI metadata.
pub fn nifti_grid(nifti: &NiftiData) -> qsm_core::Grid {
    let (nx, ny, nz) = nifti.dims;
    let (vsx, vsy, vsz) = nifti.voxel_size;
    qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz)
}

/// Load a NIfTI file with error mapping.
pub fn load_nifti(path: &Path) -> crate::Result<NiftiData> {
    io::read_nifti_file(path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", path.display(), e)))
}

/// A multi-echo (4D) NIfTI volume: per-echo 3D data plus shared geometry.
pub struct MultiEcho {
    /// Per-echo volumes, each `nx*ny*nz` in NIfTI (Fortran) order.
    pub echoes: Vec<Vec<f64>>,
    /// Spatial dimensions (nx, ny, nz).
    pub dims: (usize, usize, usize),
    /// Voxel size in mm.
    pub voxel_size: (f64, f64, f64),
    /// 4x4 affine (row-major).
    pub affine: [f64; 16],
}

impl MultiEcho {
    /// A `NiftiData`-shaped geometry reference for [`save_nifti`], using the first echo.
    pub fn geometry_reference(&self) -> NiftiData {
        NiftiData {
            data: Vec::new(),
            dims: self.dims,
            voxel_size: self.voxel_size,
            affine: self.affine,
            scl_slope: 1.0,
            scl_inter: 0.0,
        }
    }
}

/// Load a multi-echo (4D) NIfTI file, splitting the 4th axis into per-echo volumes.
///
/// `read_nifti_file`/`load_nifti` only return the first volume of a 4D file, so this
/// uses `load_nifti_4d` and de-interleaves the flat `t`-major buffer.
pub fn load_nifti_4d(path: &Path) -> crate::Result<MultiEcho> {
    let bytes = std::fs::read(path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", path.display(), e)))?;
    let (data, (nx, ny, nz, nt), voxel_size, affine) = io::load_nifti_4d(&bytes)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", path.display(), e)))?;
    let n_voxels = nx * ny * nz;
    // load_nifti_4d lays the buffer out t-major: index = vox + t*n_voxels.
    let echoes: Vec<Vec<f64>> = (0..nt)
        .map(|t| data[t * n_voxels..(t + 1) * n_voxels].to_vec())
        .collect();
    Ok(MultiEcho { echoes, dims: (nx, ny, nz), voxel_size, affine })
}

/// Load a binary mask from a NIfTI file (threshold at 0.5).
pub fn load_mask(path: &Path) -> crate::Result<(Vec<u8>, NiftiData)> {
    let nifti = load_nifti(path)?;
    let mask: Vec<u8> = nifti.data.iter().map(|&v| if v > 0.5 { 1u8 } else { 0u8 }).collect();
    Ok((mask, nifti))
}

/// Save a f64 volume to NIfTI, preserving geometry from a reference.
pub fn save_nifti(path: &Path, data: &[f64], reference: &NiftiData) -> crate::Result<()> {
    io::save_nifti_to_file(path, data, reference.dims, reference.voxel_size, &reference.affine)
        .map_err(QsmxtError::NiftiIo)
}

/// Save a u8 mask as f64 NIfTI, preserving geometry from a reference.
pub fn save_mask(path: &Path, mask: &[u8], reference: &NiftiData) -> crate::Result<()> {
    let data: Vec<f64> = mask.iter().map(|&m| m as f64).collect();
    save_nifti(path, &data, reference)
}

/// Run a mask-to-mask operation: load → apply op → save.
pub fn run_mask_operation(
    input: &Path,
    output: &Path,
    op_name: &str,
    op: impl FnOnce(&[u8], &qsm_core::Grid) -> Vec<u8>,
) -> crate::Result<()> {
    let (mask, nifti) = load_mask(input)?;
    let grid = nifti_grid(&nifti);
    log::info!("{} ({}x{}x{})", op_name, grid.nx(), grid.ny(), grid.nz());
    let result = op(&mask, &grid);
    save_mask(output, &result, &nifti)?;
    log::info!("{} saved to {}", op_name, output.display());
    Ok(())
}

/// Load multiple NIfTI files, validate against echo times, interleave, and compute R2* via ARLO.
/// Returns the R2* map and the first magnitude NiftiData as geometry reference.
pub fn compute_r2star(
    inputs: &[std::path::PathBuf],
    mask_path: &Path,
    echo_times: &[f64],
) -> crate::Result<(Vec<f64>, NiftiData)> {
    // Reference geometry from the first input (first volume of a 4D file, or the first 3D echo).
    let reference = load_nifti(&inputs[0])?;
    let (nx, ny, nz) = reference.dims;
    let n_voxels = nx * ny * nz;
    // Accepts a single 4D file or several 3D files.
    let (interleaved, n_echoes) = load_multiecho_voxel_major(inputs, n_voxels)?;
    if n_echoes != echo_times.len() {
        return Err(QsmxtError::Config(format!(
            "{} magnitude echoes but {} echo times", n_echoes, echo_times.len())));
    }
    let (mask, _) = load_mask(mask_path)?;
    let grid = nifti_grid(&reference);
    let (r2star_map, _s0_map) = qsm_core::utils::r2star_arlo(
        &interleaved, &mask, echo_times, &grid,
    );
    Ok((r2star_map, reference))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutils;

    #[test]
    fn test_load_nifti() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mag.nii");
        testutils::write_magnitude(&path);
        let nifti = load_nifti(&path).unwrap();
        assert_eq!(nifti.dims, (8, 8, 8));
        assert_eq!(nifti.data.len(), 512);
    }

    #[test]
    fn test_load_mask() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mask.nii");
        testutils::write_mask(&path);
        let (mask, nifti) = load_mask(&path).unwrap();
        assert_eq!(mask.len(), 512);
        assert_eq!(nifti.dims, (8, 8, 8));
        // Border voxels should be 0, interior 1
        assert_eq!(mask[0], 0); // corner
        let center = 4 + 4 * 8 + 4 * 64;
        assert_eq!(mask[center], 1);
    }

    #[test]
    fn test_save_and_reload_nifti() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.nii");
        let dst = dir.path().join("dst.nii");
        testutils::write_magnitude(&src);
        let nifti = load_nifti(&src).unwrap();
        save_nifti(&dst, &nifti.data, &nifti).unwrap();
        let reloaded = load_nifti(&dst).unwrap();
        assert_eq!(reloaded.dims, nifti.dims);
        assert_eq!(reloaded.data.len(), nifti.data.len());
    }

    #[test]
    fn test_save_and_reload_mask() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("mask_src.nii");
        let dst = dir.path().join("mask_dst.nii");
        testutils::write_mask(&src);
        let (mask, nifti) = load_mask(&src).unwrap();
        save_mask(&dst, &mask, &nifti).unwrap();
        let (reloaded, _) = load_mask(&dst).unwrap();
        assert_eq!(reloaded, mask);
    }

    #[test]
    fn test_load_nifti_missing_file() {
        let result = load_nifti(Path::new("/nonexistent/file.nii"));
        assert!(result.is_err());
    }
}
