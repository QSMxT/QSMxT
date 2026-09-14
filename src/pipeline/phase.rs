// Delegate to qsm-core's canonical implementations
pub use qsm_core::pipeline::scale_phase_to_pi;
pub use qsm_core::pipeline::rss_combine;

// Scan geometry lives in qsm-core so every host computes it the same way; see
// `qsm_core::geometry` for why the voxel scaling must be factored out of the affine before the
// B0 direction is taken, and why wrapped phase has to be resampled in the complex domain.
pub use qsm_core::geometry::{b0_direction_from_affine, resample_mask_to_axial};

/// Find the center of mass of a binary mask (for ROMEO seed point).
#[allow(dead_code)]
pub fn mask_center_of_mass(mask: &[u8], nx: usize, ny: usize, nz: usize) -> (usize, usize, usize) {
    let mut sx = 0.0f64;
    let mut sy = 0.0f64;
    let mut sz = 0.0f64;
    let mut count = 0.0f64;

    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                if mask[x + y * nx + z * nx * ny] > 0 {
                    sx += x as f64;
                    sy += y as f64;
                    sz += z as f64;
                    count += 1.0;
                }
            }
        }
    }

    if count < 1.0 {
        return (nx / 2, ny / 2, nz / 2);
    }

    (
        (sx / count) as usize,
        (sy / count) as usize,
        (sz / count) as usize,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // --- scale_phase_to_pi ---

    #[test]
    fn test_scale_phase_empty_array() {
        let mut data: Vec<f64> = vec![];
        scale_phase_to_pi(&mut data);
        assert!(data.is_empty());
    }

    #[test]
    fn test_scale_phase_already_in_pi_range() {
        let mut data = vec![-PI, 0.0, PI];
        let original = data.clone();
        scale_phase_to_pi(&mut data);
        // Should be unchanged (within tolerance)
        for (a, b) in data.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-10, "Data changed when already in range");
        }
    }

    #[test]
    fn test_scale_phase_rescales_0_to_4096() {
        let mut data = vec![0.0, 2048.0, 4096.0];
        scale_phase_to_pi(&mut data);
        assert!((data[0] - (-PI)).abs() < 1e-10, "Min should map to -PI");
        assert!((data[2] - PI).abs() < 1e-10, "Max should map to PI");
        assert!(data[1].abs() < 1e-10, "Midpoint should map to ~0");
    }

    #[test]
    fn test_scale_phase_nan_replaced_with_zero() {
        let mut data = vec![0.0, f64::NAN, 4096.0];
        scale_phase_to_pi(&mut data);
        // NaN was replaced with 0.0 before rescaling
        // 0.0 maps to -PI (it's the min of the finite values)
        assert!(data[1].is_finite(), "NaN should be replaced with finite value");
    }

    #[test]
    fn test_scale_phase_constant_value() {
        let mut data = vec![5.0, 5.0, 5.0];
        scale_phase_to_pi(&mut data);
        // Range < 1e-10, returns early without rescaling
        assert!((data[0] - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_scale_phase_all_nan() {
        let mut data = vec![f64::NAN, f64::NAN, f64::NAN];
        scale_phase_to_pi(&mut data);
        // All replaced with 0, range is 0, returns early
        for v in &data {
            assert!((v - 0.0).abs() < 1e-10);
        }
    }

    // --- b0_direction_from_affine ---

    #[test]
    fn test_b0_direction_identity_matrix() {
        let mut affine = [0.0f64; 16];
        affine[0] = 1.0;
        affine[5] = 1.0;
        affine[10] = 1.0;
        affine[15] = 1.0;
        let (bx, by, bz) = b0_direction_from_affine(&affine);
        assert!(bx.abs() < 1e-6, "bx should be ~0, got {}", bx);
        assert!(by.abs() < 1e-6, "by should be ~0, got {}", by);
        assert!((bz - 1.0).abs() < 1e-6, "bz should be ~1, got {}", bz);
    }

    #[test]
    fn test_b0_direction_singular_matrix() {
        let affine = [0.0f64; 16]; // All zeros, det=0
        let (bx, by, bz) = b0_direction_from_affine(&affine);
        assert!((bx - 0.0).abs() < 1e-10);
        assert!((by - 0.0).abs() < 1e-10);
        assert!((bz - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_b0_direction_scaled_matrix() {
        let mut affine = [0.0f64; 16];
        affine[0] = 2.0;
        affine[5] = 2.0;
        affine[10] = 2.0;
        affine[15] = 1.0;
        let (bx, by, bz) = b0_direction_from_affine(&affine);
        // Scaled identity, normalized result should still be (0, 0, 1)
        assert!(bx.abs() < 1e-6);
        assert!(by.abs() < 1e-6);
        assert!((bz - 1.0).abs() < 1e-6);
    }

    // --- mask_center_of_mass ---

    #[test]
    fn test_mask_center_of_mass_empty() {
        let mask = vec![0u8; 27]; // 3x3x3
        let (cx, cy, cz) = mask_center_of_mass(&mask, 3, 3, 3);
        assert_eq!((cx, cy, cz), (1, 1, 1), "Empty mask should return volume center");
    }

    #[test]
    fn test_mask_center_of_mass_single_voxel() {
        let mut mask = vec![0u8; 64]; // 4x4x4
        // Set voxel (2, 2, 2)
        mask[2 + 2 * 4 + 2 * 16] = 1;
        let (cx, cy, cz) = mask_center_of_mass(&mask, 4, 4, 4);
        assert_eq!((cx, cy, cz), (2, 2, 2));
    }

    #[test]
    fn test_mask_center_of_mass_symmetric() {
        let mut mask = vec![0u8; 27]; // 3x3x3
        // Set opposite corners: (0,0,0) and (2,2,2)
        mask[0] = 1;
        mask[2 + 2 * 3 + 2 * 9] = 1;
        let (cx, cy, cz) = mask_center_of_mass(&mask, 3, 3, 3);
        // CoM should be (1, 1, 1)
        assert_eq!((cx, cy, cz), (1, 1, 1));
    }

    // --- obliquity_from_affine ---

    #[test]
    fn test_obliquity_identity_is_zero() {
        let mut affine = [0.0f64; 16];
        affine[0] = 1.0;
        affine[5] = 1.0;
        affine[10] = 1.0;
        affine[15] = 1.0;
        let obliquity = qsm_core::geometry::obliquity_from_affine(&affine);
        assert!(obliquity < 0.01, "Identity should have ~0° obliquity, got {}", obliquity);
    }

    #[test]
    fn test_obliquity_scaled_identity_is_zero() {
        let mut affine = [0.0f64; 16];
        affine[0] = 2.0;
        affine[5] = 2.0;
        affine[10] = 2.0;
        affine[15] = 1.0;
        let obliquity = qsm_core::geometry::obliquity_from_affine(&affine);
        assert!(obliquity < 0.01, "Scaled identity should have ~0° obliquity, got {}", obliquity);
    }

    #[test]
    fn test_obliquity_rotated_is_nonzero() {
        // 45° rotation in XZ plane
        let angle = std::f64::consts::FRAC_PI_4;
        let c = angle.cos();
        let s = angle.sin();
        let mut affine = [0.0f64; 16];
        affine[0] = c;    // r00
        affine[2] = s;    // r02
        affine[5] = 1.0;  // r11
        affine[8] = -s;   // r20
        affine[10] = c;   // r22
        affine[15] = 1.0;
        let obliquity = qsm_core::geometry::obliquity_from_affine(&affine);
        assert!(obliquity > 40.0, "45° rotation should give ~45° obliquity, got {}", obliquity);
    }

    // --- resample_to_axial ---

    #[test]
    fn test_resample_identity_affine_preserves_data() {
        // 3x3x3 volume with identity affine
        let data: Vec<f64> = (0..27).map(|i| i as f64).collect();
        let mut affine = [0.0f64; 16];
        affine[0] = 1.0;
        affine[5] = 1.0;
        affine[10] = 1.0;
        affine[15] = 1.0;

        let result = qsm_core::geometry::resample_to_axial(&data, 3, 3, 3, &affine);
        // Identity should produce same dimensions
        assert_eq!(result.dims, (3, 3, 3));
        // Values at integer grid points should match
        for (i, (&orig, &resampled)) in data.iter().zip(result.data.iter()).enumerate() {
            assert!(
                (orig - resampled).abs() < 1e-6,
                "Mismatch at voxel {}: {} vs {}",
                i, orig, resampled
            );
        }
    }

    #[test]
    fn test_resample_axial_affine_is_diagonal() {
        let mut affine = [0.0f64; 16];
        // Rotated affine
        let angle = 0.3_f64; // ~17 degrees
        affine[0] = angle.cos();
        affine[2] = angle.sin();
        affine[5] = 1.0;
        affine[8] = -angle.sin();
        affine[10] = angle.cos();
        affine[15] = 1.0;

        let data = vec![1.0f64; 27]; // 3x3x3
        let result = qsm_core::geometry::resample_to_axial(&data, 3, 3, 3, &affine);

        // Output affine should be diagonal (cardinal-aligned)
        assert!((result.affine[1]).abs() < 1e-10, "Off-diagonal should be 0");
        assert!((result.affine[2]).abs() < 1e-10, "Off-diagonal should be 0");
        assert!((result.affine[4]).abs() < 1e-10, "Off-diagonal should be 0");
        assert!(result.affine[0] > 0.0, "Diagonal should be positive voxel size");
    }

    // --- resample_mask_to_axial ---

    #[test]
    fn test_resample_mask_identity_preserves() {
        let mask = vec![0u8, 1, 0, 1, 1, 1, 0, 1, 0]; // 3x3x1
        let mut affine = [0.0f64; 16];
        affine[0] = 1.0;
        affine[5] = 1.0;
        affine[10] = 1.0;
        affine[15] = 1.0;

        let result = resample_mask_to_axial(&mask, 3, 3, 1, &affine);
        // With identity affine, mask should be preserved
        assert_eq!(result.len(), mask.len());
        assert_eq!(result, mask);
    }
}
