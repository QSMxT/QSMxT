//! Writing volumes out as NIfTI.

use std::path::Path;
use crate::error::QsmxtError;

/// Write an `f64` volume as NIfTI, creating the output directory if it does not exist.
///
/// `qsm_core::io::save_nifti` refuses a payload that does not match the dimensions it is handed,
/// so a header that promises voxels the file does not hold (issue #211) is an error here rather
/// than a file someone opens later. Every volume QSMxT writes goes out through this one door, so
/// that guarantee covers all of them.
pub fn write_volume(
    path: &Path,
    data: &[f64],
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    affine: &[f64; 16],
) -> crate::Result<()> {
    // A bare filename has an empty parent, which `create_dir_all` rejects.
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => std::fs::create_dir_all(parent)?,
        _ => {}
    }
    qsm_core::io::save_nifti_to_file(path, data, dims, voxel_size, affine)
        .map_err(QsmxtError::NiftiIo)
}

/// Write several volumes on one grid as a single 4D NIfTI, the volumes along the fourth axis in
/// the order given.
///
/// For outputs that are one quantity with several components (the STI tensor, its eigenvector):
/// QSM.rs writes only 3D volumes, so the header is QSM.rs's for the first volume with the fourth
/// dimension added, and the remaining volumes follow it — the layout NIfTI defines for 4D data.
pub fn write_volumes_4d(
    path: &Path,
    volumes: &[&[f64]],
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    affine: &[f64; 16],
) -> crate::Result<()> {
    let Some(first) = volumes.first() else {
        return Err(QsmxtError::NiftiIo(format!("{}: no volumes to write", path.display())));
    };
    let mut bytes = qsm_core::io::save_nifti(first, dims, voxel_size, affine)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {e}", path.display())))?;
    let n = dims.0 * dims.1 * dims.2;
    if let Some(v) = volumes.iter().find(|v| v.len() != n) {
        return Err(QsmxtError::NiftiIo(format!(
            "{}: cannot write a {}x{}x{} volume from {} values", path.display(), dims.0, dims.1, dims.2, v.len())));
    }
    let nt = i16::try_from(volumes.len())
        .map_err(|_| QsmxtError::NiftiIo(format!("{}: too many volumes", path.display())))?;
    // dim[0] = 4 dimensions, dim[4] = the volume count (pixdim[4] stays 1).
    bytes[40..42].copy_from_slice(&4i16.to_le_bytes());
    bytes[48..50].copy_from_slice(&nt.to_le_bytes());
    bytes.reserve(n * 4 * (volumes.len() - 1));
    for v in &volumes[1..] {
        for &x in v.iter() {
            bytes.extend_from_slice(&(x as f32).to_le_bytes());
        }
    }
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => std::fs::create_dir_all(parent)?,
        _ => {}
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFFINE: [f64; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    #[test]
    fn writes_a_4d_volume_that_reads_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.nii");
        let (a, b, c) = (vec![1.0; 24], vec![2.0; 24], vec![3.0; 24]);
        write_volumes_4d(&path, &[&a, &b, &c], (2, 3, 4), (1.0, 1.0, 1.0), &AFFINE).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let (data, dims, _, _) = qsm_core::io::load_nifti_4d(&bytes).unwrap();
        assert_eq!(dims, (2, 3, 4, 3));
        assert_eq!(data[0], 1.0);
        assert_eq!(data[24], 2.0);
        assert_eq!(data[71], 3.0);
        assert!(write_volumes_4d(&path, &[&a, &[1.0; 5]], (2, 3, 4), (1.0, 1.0, 1.0), &AFFINE).is_err());
    }

    #[test]
    fn writes_a_matching_volume_into_a_new_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("vol.nii");
        write_volume(&path, &vec![1.0; 4 * 5 * 6], (4, 5, 6), (1.0, 1.0, 1.0), &AFFINE).unwrap();
        let back = qsm_core::io::read_nifti_file(&path).unwrap();
        assert_eq!(back.dims, (4, 5, 6));
        assert_eq!(back.data.len(), 4 * 5 * 6);
    }

    /// The guard lives in qsm-core; this pins that QSMxT surfaces it instead of writing the file.
    #[test]
    fn a_payload_that_does_not_match_the_dimensions_never_reaches_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.nii");
        let err = write_volume(&path, &vec![1.0; 100], (4, 5, 6), (1.0, 1.0, 1.0), &AFFINE)
            .unwrap_err()
            .to_string();
        assert!(err.contains("short.nii"), "{err}");
        assert!(err.contains("120 voxels"), "{err}");
        assert!(!path.exists(), "nothing should be written for a mismatch");
    }

    #[test]
    fn writes_gzipped_output_too() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vol.nii.gz");
        write_volume(&path, &[2.0; 8], (2, 2, 2), (1.0, 1.0, 1.0), &AFFINE).unwrap();
        let back = qsm_core::io::read_nifti_file(&path).unwrap();
        assert_eq!(back.dims, (2, 2, 2));
        assert_eq!(back.data, vec![2.0; 8]);
    }
}
