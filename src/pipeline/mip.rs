//! Geometry of the SWI minimum-intensity projection.

use crate::error::QsmxtError;

/// Dimensions and affine for a sliding minimum-intensity projection over `window` slices.
///
/// `qsm_core::swi::create_mip` slides the window along k and emits one slice per position, so the
/// projection is `window - 1` slices shorter than the volume it came from. Writing it with the
/// original dimensions leaves a header that claims more voxels than the file holds (issue #211).
///
/// Each output slice stands for the *centre* of its slab, so the origin moves `(window - 1) / 2`
/// slices along the slice direction. The shift follows the affine's third column rather than
/// world z, which keeps it right for an oblique acquisition; an even window lands the origin on a
/// half-slice offset, as the convention implies.
pub fn mip_geometry(
    dims: (usize, usize, usize),
    affine: &[f64; 16],
    window: usize,
) -> crate::Result<((usize, usize, usize), [f64; 16])> {
    let (nx, ny, nz) = dims;
    if window == 0 {
        return Err(QsmxtError::Config(
            "SWI minIP window must be at least 1 slice".into(),
        ));
    }
    if window > nz {
        return Err(QsmxtError::Config(format!(
            "SWI minIP window of {} slices is deeper than the {}-slice volume — \
             pass --swi-mip-window with a value of {} or less",
            window, nz, nz,
        )));
    }

    let mut mip_affine = *affine;
    let slabs = (window - 1) as f64 / 2.0;
    for row in 0..3 {
        // Row-major 4x4: column 2 is the slice direction, column 3 the origin.
        mip_affine[row * 4 + 3] = affine[row * 4 + 3] + affine[row * 4 + 2] * slabs;
    }

    Ok(((nx, ny, nz - window + 1), mip_affine))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: [f64; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    #[test]
    fn shortens_the_slice_axis_by_the_window() {
        let (dims, _) = mip_geometry((32, 32, 32), &IDENTITY, 7).unwrap();
        assert_eq!(dims, (32, 32, 26));
    }

    #[test]
    fn a_window_of_one_changes_nothing() {
        let (dims, affine) = mip_geometry((4, 5, 6), &IDENTITY, 1).unwrap();
        assert_eq!(dims, (4, 5, 6));
        assert_eq!(affine, IDENTITY);
    }

    #[test]
    fn a_window_of_the_full_depth_leaves_one_slice() {
        let (dims, affine) = mip_geometry((4, 5, 6), &IDENTITY, 6).unwrap();
        assert_eq!(dims, (4, 5, 1));
        // The single slab is centred on slice 2.5 of the original volume.
        assert_eq!(affine[11], 2.5);
    }

    #[test]
    fn centres_each_slab_along_z() {
        let mut affine = IDENTITY;
        affine[10] = 2.0; // 2 mm slices
        affine[11] = 10.0;
        let (_, mip_affine) = mip_geometry((4, 4, 16), &affine, 7).unwrap();
        // Three slices of 2 mm past the first slab's first slice.
        assert_eq!(mip_affine[11], 16.0);
        // Only the origin moves.
        assert_eq!(mip_affine[..11], affine[..11]);
    }

    #[test]
    fn an_even_window_lands_on_a_half_slice() {
        let (_, mip_affine) = mip_geometry((4, 4, 16), &IDENTITY, 4).unwrap();
        assert_eq!(mip_affine[11], 1.5);
    }

    #[test]
    fn shifts_along_the_slice_direction_of_an_oblique_affine() {
        // Slice direction tilted 45° in the y/z plane, 2 mm slices.
        let s = 2.0 / 2f64.sqrt();
        let affine = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, s, 0.0,
            0.0, 0.0, s, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let (dims, mip_affine) = mip_geometry((8, 8, 20), &affine, 7).unwrap();
        assert_eq!(dims, (8, 8, 14));
        // Both y and z move, by three slice steps along the tilted direction.
        assert!((mip_affine[7] - 3.0 * s).abs() < 1e-12, "{:?}", mip_affine);
        assert!((mip_affine[11] - 3.0 * s).abs() < 1e-12, "{:?}", mip_affine);
        assert_eq!(mip_affine[3], 0.0);
    }

    #[test]
    fn rejects_a_zero_window() {
        let err = mip_geometry((4, 4, 4), &IDENTITY, 0).unwrap_err().to_string();
        assert!(err.contains("at least 1 slice"), "{err}");
    }

    #[test]
    fn rejects_a_window_deeper_than_the_volume() {
        let err = mip_geometry((4, 4, 4), &IDENTITY, 5).unwrap_err().to_string();
        assert!(err.contains("deeper than the 4-slice volume"), "{err}");
    }
}
