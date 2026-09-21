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
