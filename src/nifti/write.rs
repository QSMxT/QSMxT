//! Writing volumes out as NIfTI, with the header and the payload kept in step.

use std::path::Path;
use crate::error::QsmxtError;

/// Write an `f64` volume as NIfTI, refusing any payload that does not match `dims`.
///
/// A NIfTI header states a matrix size and every reader trusts it, so a writer that accepts a
/// mismatched buffer produces a file that opens fine and then fails — or shows nothing — the
/// moment something reads the voxels (issue #211). Every volume QSMxT writes goes out through
/// here, so a mismatch is an error where it is made rather than a corrupt file on disk.
pub fn write_volume(
    path: &Path,
    data: &[f64],
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    affine: &[f64; 16],
) -> crate::Result<()> {
    let (nx, ny, nz) = dims;
    let expected = nx * ny * nz;
    if data.len() != expected {
        return Err(QsmxtError::DimensionMismatch(format!(
            "refusing to write {}: the header would say {}x{}x{} ({} voxels) but {} values were given",
            path.display(), nx, ny, nz, expected, data.len(),
        )));
    }
    // A bare filename has an empty parent, which `create_dir_all` rejects.
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => std::fs::create_dir_all(parent)?,
        _ => {}
    }
    qsm_core::io::save_nifti_to_file(path, data, dims, voxel_size, affine)
        .map_err(QsmxtError::NiftiIo)
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
    fn writes_a_matching_volume() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("vol.nii");
        write_volume(&path, &vec![1.0; 4 * 5 * 6], (4, 5, 6), (1.0, 1.0, 1.0), &AFFINE).unwrap();
        let back = qsm_core::io::read_nifti_file(&path).unwrap();
        assert_eq!(back.dims, (4, 5, 6));
        assert_eq!(back.data.len(), 4 * 5 * 6);
    }

    #[test]
    fn rejects_a_short_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.nii");
        let err = write_volume(&path, &vec![1.0; 100], (4, 5, 6), (1.0, 1.0, 1.0), &AFFINE)
            .unwrap_err()
            .to_string();
        assert!(err.contains("120 voxels"), "{err}");
        assert!(err.contains("100 values"), "{err}");
        assert!(!path.exists(), "nothing should be written for a mismatch");
    }

    #[test]
    fn rejects_a_long_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.nii");
        assert!(write_volume(&path, &vec![1.0; 200], (4, 5, 6), (1.0, 1.0, 1.0), &AFFINE).is_err());
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
