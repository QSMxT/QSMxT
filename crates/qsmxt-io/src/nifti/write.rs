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

/// Write an `f64` volume as a double-precision NIfTI (datatype FLOAT64), creating the output
/// directory if it does not exist: for the intermediates a later step must read as they were
/// computed ([`qsmxt_engine::Item::kept_in_double`]). The header is [`write_volume`]'s (qsm-core's),
/// with the datatype and bit depth changed; a `.nii.gz` path is gzipped.
pub fn write_volume_f64(
    path: &Path,
    data: &[f64],
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    affine: &[f64; 16],
) -> crate::Result<()> {
    /// NIfTI-1: `datatype` and `bitpix` (i16 each), `vox_offset` (f32).
    const DATATYPE: usize = 70;
    const BITPIX: usize = 72;
    const VOX_OFFSET: usize = 108;
    const DT_FLOAT64: i16 = 64;
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => std::fs::create_dir_all(parent)?,
        _ => {}
    }
    let fail = |e: String| QsmxtError::NiftiIo(format!("{}: {e}", path.display()));
    // qsm-core's header (and its check that the payload matches the dimensions), float32 data.
    let mut bytes = qsm_core::io::save_nifti(data, dims, voxel_size, affine).map_err(fail)?;
    let offset = f32::from_le_bytes(bytes[VOX_OFFSET..VOX_OFFSET + 4].try_into().expect("4 bytes")) as usize;
    bytes.truncate(offset);
    bytes[DATATYPE..DATATYPE + 2].copy_from_slice(&DT_FLOAT64.to_le_bytes());
    bytes[BITPIX..BITPIX + 2].copy_from_slice(&64i16.to_le_bytes());
    bytes.reserve(data.len() * 8);
    for v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    if path.to_string_lossy().ends_with(".nii.gz") {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&bytes)?;
        bytes = gz.finish()?;
    }
    std::fs::write(path, &bytes)?;
    Ok(())
}

/// Write several volumes on one grid as a single 4D NIfTI, the volumes along the fourth axis in
/// the order given, creating the output directory if it does not exist.
///
/// For outputs that are one quantity with several components (the STI tensor, its eigenvector).
/// The header and the compression (a `.nii.gz` path is gzipped) are `qsm_core::io`'s, the same as
/// [`write_volume`]'s, and a volume of the wrong length is an error rather than a file.
pub fn write_volumes_4d(
    path: &Path,
    volumes: &[&[f64]],
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    affine: &[f64; 16],
) -> crate::Result<()> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => std::fs::create_dir_all(parent)?,
        _ => {}
    }
    qsm_core::io::save_nifti_4d_to_file(path, volumes, dims, voxel_size, affine)
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

    /// Byte 123 of an uncompressed NIfTI-1 header is `xyzt_units`.
    fn xyzt_units(path: &Path) -> u8 {
        std::fs::read(path).unwrap()[123]
    }

    #[test]
    fn a_gz_4d_path_is_gzipped_and_both_writers_carry_the_units() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("t.nii.gz");
        let (a, b) = (vec![1.0; 24], vec![2.0; 24]);
        write_volumes_4d(&path, &[&a, &b], (2, 3, 4), (1.0, 1.0, 1.0), &AFFINE).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(&[0x1f, 0x8b]), "a .nii.gz path must be gzipped");
        let (data, dims, _, _) = qsm_core::io::load_nifti_4d(&bytes).unwrap();
        assert_eq!(dims, (2, 3, 4, 2));
        assert_eq!(data[24], 2.0);

        let (p4, p3) = (dir.path().join("t.nii"), dir.path().join("v.nii"));
        write_volumes_4d(&p4, &[&a, &b], (2, 3, 4), (1.0, 1.0, 1.0), &AFFINE).unwrap();
        write_volume(&p3, &a, (2, 3, 4), (1.0, 1.0, 1.0), &AFFINE).unwrap();
        assert_eq!(xyzt_units(&p4), qsm_core::io::NIFTI_XYZT_UNITS);
        assert_eq!(xyzt_units(&p3), qsm_core::io::NIFTI_XYZT_UNITS);
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

    /// Double precision reads back bit for bit (plain and gzipped), where float32 rounds; the
    /// geometry is the float32 writer's.
    #[test]
    fn a_double_precision_volume_reads_back_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<f64> = (0..24).map(|i| 1.0 / 3.0 + i as f64 * 1e-9).collect();
        assert!(data.iter().any(|&v| v as f32 as f64 != v));
        let affine = [0.5, 0.0, 0.0, -3.0, 0.0, 2.0, 0.0, 4.0, 0.0, 0.0, 1.5, 7.0, 0.0, 0.0, 0.0, 1.0];
        for name in ["d.nii", "d.nii.gz"] {
            let path = dir.path().join("nested").join(name);
            write_volume_f64(&path, &data, (2, 3, 4), (0.5, 2.0, 1.5), &affine).unwrap();
            let back = qsm_core::io::read_nifti_file(&path).unwrap();
            assert_eq!(back.data, data, "{name}");
            let single = dir.path().join(format!("s-{name}"));
            write_volume(&single, &data, (2, 3, 4), (0.5, 2.0, 1.5), &affine).unwrap();
            let s = qsm_core::io::read_nifti_file(&single).unwrap();
            assert_ne!(s.data, data);
            assert_eq!((back.dims, back.voxel_size, back.affine), (s.dims, s.voxel_size, s.affine), "{name}");
        }
        assert!(write_volume_f64(&dir.path().join("short.nii"), &[1.0; 5], (2, 3, 4), (1.0, 1.0, 1.0), &AFFINE).is_err());
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
