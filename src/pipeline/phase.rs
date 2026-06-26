// Delegate to qsm-core's canonical implementations
pub use qsm_core::pipeline::scale_phase_to_pi;
pub use qsm_core::pipeline::rss_combine;

/// Compute B0 direction from NIfTI affine matrix.
///
/// B0 is assumed along z-axis in scanner coordinates [0, 0, 1].
/// Transform to voxel coordinates using inverse rotation matrix.
#[allow(dead_code)]
pub fn b0_direction_from_affine(affine: &[f64; 16]) -> (f64, f64, f64) {
    // Extract 3x3 rotation/scaling from affine
    let r00 = affine[0];
    let r01 = affine[1];
    let r02 = affine[2];
    let r10 = affine[4];
    let r11 = affine[5];
    let r12 = affine[6];
    let r20 = affine[8];
    let r21 = affine[9];
    let r22 = affine[10];

    // Compute inverse of 3x3 rotation matrix
    let det = r00 * (r11 * r22 - r12 * r21) - r01 * (r10 * r22 - r12 * r20)
        + r02 * (r10 * r21 - r11 * r20);

    if det.abs() < 1e-10 {
        return (0.0, 0.0, 1.0); // Fallback
    }

    let inv_det = 1.0 / det;

    // Inverse rotation applied to [0, 0, 1] (scanner z-axis)
    // Only need the third column of the inverse matrix
    let bx = (r01 * r12 - r02 * r11) * inv_det;
    let by = (r02 * r10 - r00 * r12) * inv_det;
    let bz = (r00 * r11 - r01 * r10) * inv_det;

    // Normalize
    let norm = (bx * bx + by * by + bz * bz).sqrt();
    if norm < 1e-10 {
        return (0.0, 0.0, 1.0);
    }

    (bx / norm, by / norm, bz / norm)
}

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

/// Compute the obliquity angle (in degrees) from a NIfTI affine matrix.
///
/// Returns the angle between the scanner z-axis and the closest cardinal axis
/// in voxel space. A perfectly axial acquisition returns 0.
pub fn obliquity_from_affine(affine: &[f64; 16]) -> f64 {
    // Extract 3x3 rotation/scaling
    let cols: [[f64; 3]; 3] = [
        [affine[0], affine[4], affine[8]],
        [affine[1], affine[5], affine[9]],
        [affine[2], affine[6], affine[10]],
    ];

    // Find the maximum absolute value in each column to determine the
    // "dominant axis". The obliquity is the worst-case angle from cardinal.
    let mut max_obliquity: f64 = 0.0;
    for col in &cols {
        let norm = (col[0] * col[0] + col[1] * col[1] + col[2] * col[2]).sqrt();
        if norm < 1e-10 {
            continue;
        }
        // For each column, find the component with the largest absolute value
        let max_component = col.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        // cos(angle) = max_component / norm
        let cos_angle = (max_component / norm).min(1.0);
        let angle_deg = cos_angle.acos().to_degrees();
        if angle_deg > max_obliquity {
            max_obliquity = angle_deg;
        }
    }
    max_obliquity
}

/// Trilinear interpolation at a floating-point voxel coordinate.
fn trilinear_sample(data: &[f64], nx: usize, ny: usize, nz: usize, x: f64, y: f64, z: f64) -> f64 {
    let x0 = (x.floor() as isize).max(0).min(nx as isize - 1) as usize;
    let y0 = (y.floor() as isize).max(0).min(ny as isize - 1) as usize;
    let z0 = (z.floor() as isize).max(0).min(nz as isize - 1) as usize;
    let x1 = (x0 + 1).min(nx - 1);
    let y1 = (y0 + 1).min(ny - 1);
    let z1 = (z0 + 1).min(nz - 1);

    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let fz = z - z0 as f64;

    let idx = |x: usize, y: usize, z: usize| x + y * nx + z * nx * ny;

    let c000 = data[idx(x0, y0, z0)];
    let c100 = data[idx(x1, y0, z0)];
    let c010 = data[idx(x0, y1, z0)];
    let c110 = data[idx(x1, y1, z0)];
    let c001 = data[idx(x0, y0, z1)];
    let c101 = data[idx(x1, y0, z1)];
    let c011 = data[idx(x0, y1, z1)];
    let c111 = data[idx(x1, y1, z1)];

    c000 * (1.0 - fx) * (1.0 - fy) * (1.0 - fz)
        + c100 * fx * (1.0 - fy) * (1.0 - fz)
        + c010 * (1.0 - fx) * fy * (1.0 - fz)
        + c110 * fx * fy * (1.0 - fz)
        + c001 * (1.0 - fx) * (1.0 - fy) * fz
        + c101 * fx * (1.0 - fy) * fz
        + c011 * (1.0 - fx) * fy * fz
        + c111 * fx * fy * fz
}

/// Result of resampling a volume to axial orientation.
pub struct ResampledVolume {
    pub data: Vec<f64>,
    pub dims: (usize, usize, usize),
    pub voxel_size: (f64, f64, f64),
    pub affine: [f64; 16],
}

/// Resample a volume from oblique orientation to axial (cardinal-aligned).
///
/// Computes the bounding box in world coordinates, creates a new grid with
/// voxel axes aligned to scanner axes, and trilinear-interpolates the data.
/// The output affine is diagonal (cardinal-aligned) with the same voxel sizes.
pub fn resample_to_axial(
    data: &[f64],
    nx: usize, ny: usize, nz: usize,
    affine: &[f64; 16],
) -> ResampledVolume {
    // Extract rotation/scaling columns and translation
    let r = [
        [affine[0], affine[1], affine[2]],
        [affine[4], affine[5], affine[6]],
        [affine[8], affine[9], affine[10]],
    ];
    let t = [affine[3], affine[7], affine[11]];

    // Compute voxel sizes from column norms
    let vsx = (r[0][0] * r[0][0] + r[1][0] * r[1][0] + r[2][0] * r[2][0]).sqrt();
    let vsy = (r[0][1] * r[0][1] + r[1][1] * r[1][1] + r[2][1] * r[2][1]).sqrt();
    let vsz = (r[0][2] * r[0][2] + r[1][2] * r[1][2] + r[2][2] * r[2][2]).sqrt();

    // Find world-space bounding box by transforming all 8 corners
    let corners_vox: [(f64, f64, f64); 8] = [
        (0.0, 0.0, 0.0),
        (nx as f64 - 1.0, 0.0, 0.0),
        (0.0, ny as f64 - 1.0, 0.0),
        (0.0, 0.0, nz as f64 - 1.0),
        (nx as f64 - 1.0, ny as f64 - 1.0, 0.0),
        (nx as f64 - 1.0, 0.0, nz as f64 - 1.0),
        (0.0, ny as f64 - 1.0, nz as f64 - 1.0),
        (nx as f64 - 1.0, ny as f64 - 1.0, nz as f64 - 1.0),
    ];

    let mut world_min = [f64::INFINITY; 3];
    let mut world_max = [f64::NEG_INFINITY; 3];
    for &(vi, vj, vk) in &corners_vox {
        for d in 0..3 {
            let w = r[d][0] * vi + r[d][1] * vj + r[d][2] * vk + t[d];
            if w < world_min[d] { world_min[d] = w; }
            if w > world_max[d] { world_max[d] = w; }
        }
    }

    // New grid dimensions using original voxel sizes
    let new_nx = ((world_max[0] - world_min[0]) / vsx).ceil() as usize + 1;
    let new_ny = ((world_max[1] - world_min[1]) / vsy).ceil() as usize + 1;
    let new_nz = ((world_max[2] - world_min[2]) / vsz).ceil() as usize + 1;

    // New affine: diagonal (cardinal-aligned), translation = world_min
    let new_affine = [
        vsx, 0.0, 0.0, world_min[0],
        0.0, vsy, 0.0, world_min[1],
        0.0, 0.0, vsz, world_min[2],
        0.0, 0.0, 0.0, 1.0,
    ];

    // Compute inverse of original affine's 3x3 rotation for world→voxel mapping
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);

    let inv_det = 1.0 / det;
    let inv_r = [
        [
            (r[1][1] * r[2][2] - r[1][2] * r[2][1]) * inv_det,
            (r[0][2] * r[2][1] - r[0][1] * r[2][2]) * inv_det,
            (r[0][1] * r[1][2] - r[0][2] * r[1][1]) * inv_det,
        ],
        [
            (r[1][2] * r[2][0] - r[1][0] * r[2][2]) * inv_det,
            (r[0][0] * r[2][2] - r[0][2] * r[2][0]) * inv_det,
            (r[0][2] * r[1][0] - r[0][0] * r[1][2]) * inv_det,
        ],
        [
            (r[1][0] * r[2][1] - r[1][1] * r[2][0]) * inv_det,
            (r[0][1] * r[2][0] - r[0][0] * r[2][1]) * inv_det,
            (r[0][0] * r[1][1] - r[0][1] * r[1][0]) * inv_det,
        ],
    ];

    // Resample: for each new voxel, find its world coord, map to original voxel space
    let mut new_data = vec![0.0f64; new_nx * new_ny * new_nz];
    for nk in 0..new_nz {
        for nj in 0..new_ny {
            for ni in 0..new_nx {
                // World coordinate of new voxel
                let wx = world_min[0] + ni as f64 * vsx;
                let wy = world_min[1] + nj as f64 * vsy;
                let wz = world_min[2] + nk as f64 * vsz;

                // Map to original voxel space: inv_R * (world - translation)
                let dx = wx - t[0];
                let dy = wy - t[1];
                let dz = wz - t[2];
                let ox = inv_r[0][0] * dx + inv_r[0][1] * dy + inv_r[0][2] * dz;
                let oy = inv_r[1][0] * dx + inv_r[1][1] * dy + inv_r[1][2] * dz;
                let oz = inv_r[2][0] * dx + inv_r[2][1] * dy + inv_r[2][2] * dz;

                // Check bounds (with small margin for interpolation)
                if ox >= -0.5 && ox <= nx as f64 - 0.5
                    && oy >= -0.5 && oy <= ny as f64 - 0.5
                    && oz >= -0.5 && oz <= nz as f64 - 0.5
                {
                    new_data[ni + nj * new_nx + nk * new_nx * new_ny] =
                        trilinear_sample(data, nx, ny, nz, ox, oy, oz);
                }
            }
        }
    }

    ResampledVolume {
        data: new_data,
        dims: (new_nx, new_ny, new_nz),
        voxel_size: (vsx, vsy, vsz),
        affine: new_affine,
    }
}

/// Resample a binary mask to axial using nearest-neighbor interpolation.
#[allow(dead_code)]
pub fn resample_mask_to_axial(
    mask: &[u8],
    nx: usize, ny: usize, nz: usize,
    affine: &[f64; 16],
) -> Vec<u8> {
    let mask_f64: Vec<f64> = mask.iter().map(|&m| m as f64).collect();
    let resampled = resample_to_axial(&mask_f64, nx, ny, nz, affine);
    resampled.data.iter().map(|&v| if v > 0.5 { 1u8 } else { 0u8 }).collect()
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
        let obliquity = obliquity_from_affine(&affine);
        assert!(obliquity < 0.01, "Identity should have ~0° obliquity, got {}", obliquity);
    }

    #[test]
    fn test_obliquity_scaled_identity_is_zero() {
        let mut affine = [0.0f64; 16];
        affine[0] = 2.0;
        affine[5] = 2.0;
        affine[10] = 2.0;
        affine[15] = 1.0;
        let obliquity = obliquity_from_affine(&affine);
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
        let obliquity = obliquity_from_affine(&affine);
        assert!(obliquity > 40.0, "45° rotation should give ~45° obliquity, got {}", obliquity);
    }

    // --- trilinear_sample ---

    #[test]
    fn test_trilinear_at_grid_point() {
        // 2x2x2 volume with values 0..7
        let data: Vec<f64> = (0..8).map(|i| i as f64).collect();
        let val = trilinear_sample(&data, 2, 2, 2, 0.0, 0.0, 0.0);
        assert!((val - 0.0).abs() < 1e-10);
        let val = trilinear_sample(&data, 2, 2, 2, 1.0, 1.0, 1.0);
        assert!((val - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_trilinear_midpoint() {
        // 2x2x2 all zeros except (1,1,1)=8
        let mut data = vec![0.0f64; 8];
        data[1 + 2 + 4] = 8.0;
        // Midpoint (0.5, 0.5, 0.5) should be 8 * 0.5 * 0.5 * 0.5 = 1.0
        let val = trilinear_sample(&data, 2, 2, 2, 0.5, 0.5, 0.5);
        assert!((val - 1.0).abs() < 1e-10, "Expected 1.0, got {}", val);
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

        let result = resample_to_axial(&data, 3, 3, 3, &affine);
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
        let result = resample_to_axial(&data, 3, 3, 3, &affine);

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
