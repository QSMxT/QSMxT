//! Shared helpers for standalone CLI commands.

use std::path::Path;
use qsm_core::nifti_io::{self, NiftiData};
use crate::error::QsmxtError;

/// Create a Grid from NIfTI metadata.
pub fn nifti_grid(nifti: &NiftiData) -> qsm_core::Grid {
    let (nx, ny, nz) = nifti.dims;
    let (vsx, vsy, vsz) = nifti.voxel_size;
    qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz)
}

/// Load a NIfTI file with error mapping.
pub fn load_nifti(path: &Path) -> crate::Result<NiftiData> {
    nifti_io::read_nifti_file(path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", path.display(), e)))
}

/// Load a binary mask from a NIfTI file (threshold at 0.5).
pub fn load_mask(path: &Path) -> crate::Result<(Vec<u8>, NiftiData)> {
    let nifti = load_nifti(path)?;
    let mask: Vec<u8> = nifti.data.iter().map(|&v| if v > 0.5 { 1u8 } else { 0u8 }).collect();
    Ok((mask, nifti))
}

/// Save a f64 volume to NIfTI, preserving geometry from a reference.
pub fn save_nifti(path: &Path, data: &[f64], reference: &NiftiData) -> crate::Result<()> {
    nifti_io::save_nifti_to_file(path, data, reference.dims, reference.voxel_size, &reference.affine)
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
    if inputs.len() != echo_times.len() {
        return Err(QsmxtError::Config(format!(
            "Number of inputs ({}) must match number of echo times ({})",
            inputs.len(), echo_times.len()
        )));
    }

    let mut magnitudes = Vec::new();
    for path in inputs {
        magnitudes.push(load_nifti(path)?);
    }

    let (nx, ny, nz) = magnitudes[0].dims;
    let n_voxels = nx * ny * nz;
    let n_echoes = magnitudes.len();
    let (mask, _) = load_mask(mask_path)?;

    let mut interleaved = vec![0.0f64; n_voxels * n_echoes];
    for (echo_idx, mag) in magnitudes.iter().enumerate() {
        for vox in 0..n_voxels {
            interleaved[vox * n_echoes + echo_idx] = mag.data[vox];
        }
    }

    let grid = nifti_grid(&magnitudes[0]);
    let (r2star_map, _s0_map) = qsm_core::utils::r2star_arlo(
        &interleaved, &mask, echo_times, &grid,
    );

    let reference = magnitudes.swap_remove(0);
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
