//! A slice series' geometry in DICOM patient coordinates, from a NIfTI affine.

/// Direction cosines, spacings, and per-slice position derived from the affine.
pub(super) struct SliceGeometry {
    /// ImageOrientationPatient: [row(x) cosines (i-dir), col(y) cosines (j-dir)], LPS.
    pub(super) orientation: [f64; 6],
    /// Position of voxel (0,0,0) in LPS mm.
    pub(super) origin: [f64; 3],
    /// Per-slice step vector (k-direction) in LPS mm.
    pub(super) slice_dir: [f64; 3],
    pub(super) row_spacing: f64,
    pub(super) col_spacing: f64,
    pub(super) slice_thickness: f64,
}

impl SliceGeometry {
    /// Derive geometry from a NIfTI (RAS) row-major 4x4 affine.
    ///
    /// NIfTI world coordinates are RAS; DICOM patient coordinates are LPS, so the
    /// x and y components are negated.
    pub(super) fn from_affine(affine: &[f64; 16], voxel: (f64, f64, f64)) -> Self {
        // Columns of the 3x3 direction/scale matrix (voxel-step vectors), RAS.
        let i_col = [affine[0], affine[4], affine[8]];
        let j_col = [affine[1], affine[5], affine[9]];
        let k_col = [affine[2], affine[6], affine[10]];
        let origin_ras = [affine[3], affine[7], affine[11]];

        let col_spacing = norm(&i_col).max(voxel.0);
        let row_spacing = norm(&j_col).max(voxel.1);
        let slice_thickness = norm(&k_col).max(voxel.2);

        let i_unit = to_lps(&unit(&i_col));
        let j_unit = to_lps(&unit(&j_col));
        let slice_dir = to_lps(&k_col);
        let origin = to_lps(&origin_ras);

        Self {
            orientation: [i_unit[0], i_unit[1], i_unit[2], j_unit[0], j_unit[1], j_unit[2]],
            origin,
            slice_dir,
            row_spacing,
            col_spacing,
            slice_thickness,
        }
    }

    pub(super) fn position(&self, k: usize) -> [f64; 3] {
        [
            self.origin[0] + self.slice_dir[0] * k as f64,
            self.origin[1] + self.slice_dir[1] * k as f64,
            self.origin[2] + self.slice_dir[2] * k as f64,
        ]
    }

    pub(super) fn slice_location(&self, k: usize) -> f64 {
        // Projection of the slice position onto the slice normal.
        let pos = self.position(k);
        dot(&pos, &unit(&self.slice_dir))
    }
}

fn to_lps(v: &[f64; 3]) -> [f64; 3] {
    [-v[0], -v[1], v[2]]
}

fn norm(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn unit(v: &[f64; 3]) -> [f64; 3] {
    let n = norm(v);
    if n <= f64::EPSILON {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
