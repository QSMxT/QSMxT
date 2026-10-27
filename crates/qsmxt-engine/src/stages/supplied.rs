//! Maps supplied with the scan (a susceptibility map, R2, R2′, a segmentation) brought onto the
//! run's working grid, as a supplied brain mask is ([`super::mask::conform_custom_mask`]) (#384).
//!
//! - A map on the working grid is used as it is.
//! - A map on the scan's acquired grid, in a run resampled to a cardinal-aligned grid, is
//!   resampled onto the working grid by qsm-core, as the echoes were: trilinear for continuous
//!   maps, by label vote for a segmentation.
//! - A map on any other grid covers a different field of view, and is refused before the run's
//!   first step ([`grid_problem`]), naming the file and both grids.

use crate::error::EngineError;
use crate::scan::RunMetadata;
use crate::store::Item;

/// How a supplied map reaches the run's working grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// It is on the working grid.
    AsIs,
    /// It is on the scan's acquired grid, and the run works on a resampled one.
    Resample,
    /// It is on a grid of its own.
    Refused,
}

/// How a map on the grid `dims`/`affine` reaches the working grid of `meta`.
pub fn fit(dims: (usize, usize, usize), affine: &[f64; 16], meta: &RunMetadata) -> Fit {
    match meta.source_geometry {
        Some((acquired, acquired_affine)) if super::r2::same_grid(dims, affine, acquired, &acquired_affine) => Fit::Resample,
        _ if dims == meta.dims => Fit::AsIs,
        _ => Fit::Refused,
    }
}

/// What `item` is, and the option that supplies it, for messages.
fn described(item: &Item) -> (&'static str, &'static str) {
    match item {
        Item::SuppliedChi => ("susceptibility map", "--use-custom-qsm"),
        Item::SuppliedR2 => ("R2 map", "--use-custom-r2"),
        Item::SuppliedR2prime => ("R2′ map", "--use-custom-r2prime"),
        Item::SuppliedSegmentation => ("segmentation", "--use-custom-dseg"),
        _ => ("map", "the supplied file"),
    }
}

fn grid(dims: (usize, usize, usize), affine: &[f64; 16]) -> String {
    let (vx, vy, vz) = qsm_core::geometry::voxel_sizes_from_affine(affine);
    format!("{}x{}x{} at {vx:.2}x{vy:.2}x{vz:.2} mm", dims.0, dims.1, dims.2)
}

/// Why the supplied `item` (`origin`, on the grid `dims`/`affine`) cannot be used on the run
/// whose grid is `meta`, if it cannot: a whole-sentence message naming the file and both grids.
pub fn grid_problem(
    item: &Item, origin: &str, dims: (usize, usize, usize), affine: &[f64; 16], meta: &RunMetadata,
) -> Option<String> {
    if fit(dims, affine, meta) != Fit::Refused {
        return None;
    }
    let (what, option) = described(item);
    let (acquired, acquired_affine) = meta.source_geometry.unwrap_or((meta.dims, meta.affine));
    let scan = grid(acquired, &acquired_affine);
    let place = if dims == acquired { " (the same matrix, placed or oriented differently)" } else { "" };
    Some(format!(
        "{option}: the supplied {what} {origin} is on a {} grid{place}, and this scan is on a {scan} grid. \
         Supply a {what} on the scan's grid.", grid(dims, affine)))
}

/// The supplied `item`'s `data` (from `origin`, on the grid `dims`/`affine`) on the working grid
/// of `meta` ([`fit`]). A map on a grid of its own is refused with [`grid_problem`]'s message.
pub fn conform(
    item: &Item, origin: &str, data: Vec<f64>, dims: (usize, usize, usize), affine: &[f64; 16], meta: &RunMetadata,
) -> Result<Vec<f64>, EngineError> {
    let refused = || EngineError::DimensionMismatch(grid_problem(item, origin, dims, affine, meta).unwrap_or_default());
    let n = |d: (usize, usize, usize)| d.0 * d.1 * d.2;
    if data.len() != n(dims) {
        let (what, _) = described(item);
        return Err(EngineError::DimensionMismatch(format!(
            "the supplied {what} {origin} has {} voxels, and its header says {}x{}x{}", data.len(), dims.0, dims.1, dims.2)));
    }
    match fit(dims, affine, meta) {
        Fit::AsIs => Ok(data),
        Fit::Resample if *item == Item::SuppliedSegmentation => {
            Ok(qsm_core::geometry::resample_labels_onto(&data, dims, affine, meta.dims, &meta.affine)?)
        }
        Fit::Resample => qsm_core::geometry::resample_onto(&data, dims, affine, meta.dims, &meta.affine).ok_or_else(refused),
        Fit::Refused => Err(refused()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagonal(vs: f64) -> [f64; 16] {
        [vs, 0.0, 0.0, 0.0, 0.0, vs, 0.0, 0.0, 0.0, 0.0, vs, 0.0, 0.0, 0.0, 0.0, 1.0]
    }

    /// A run on an 8³ grid, resampled from an acquired grid rotated about z when `resampled`.
    fn run(resampled: bool) -> (RunMetadata, (usize, usize, usize), [f64; 16]) {
        let mut meta = RunMetadata { dims: (8, 8, 8), voxel_size: (1.0, 1.0, 1.0), affine: diagonal(1.0),
                                     ..crate::stages::magnitude::tests::meta_4x4x4() };
        let (c, s) = (0.3f64.cos(), 0.3f64.sin());
        let rotated = [c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let acquired = (8, 8, 6);
        if resampled {
            meta.source_geometry = Some((acquired, rotated));
        }
        (meta, acquired, rotated)
    }

    #[test]
    fn a_map_on_the_working_grid_is_used_as_it_is() {
        let (meta, ..) = run(false);
        let data: Vec<f64> = (0..512).map(f64::from).collect();
        assert_eq!(conform(&Item::SuppliedChi, "chi.nii", data.clone(), (8, 8, 8), &diagonal(1.0), &meta).unwrap(), data);
    }

    #[test]
    fn a_map_on_the_acquired_grid_of_a_resampled_run_is_resampled_onto_the_working_grid() {
        let (meta, acquired, rotated) = run(true);
        let n = acquired.0 * acquired.1 * acquired.2;
        for item in [Item::SuppliedChi, Item::SuppliedR2, Item::SuppliedR2prime, Item::SuppliedSegmentation] {
            assert_eq!(fit(acquired, &rotated, &meta), Fit::Resample);
            let out = conform(&item, "map.nii", vec![17.0; n], acquired, &rotated, &meta).unwrap();
            assert_eq!(out.len(), 512, "{item:?}");
            if item == Item::SuppliedSegmentation {
                assert!(out.iter().all(|&v| v == 0.0 || v == 17.0), "labels stay labels");
            }
        }
    }

    #[test]
    fn a_map_on_a_grid_of_its_own_is_refused_naming_the_file_and_both_grids() {
        for resampled in [false, true] {
            let (meta, ..) = run(resampled);
            for item in [Item::SuppliedChi, Item::SuppliedR2, Item::SuppliedR2prime, Item::SuppliedSegmentation] {
                let message = grid_problem(&item, "small.nii", (4, 4, 4), &diagonal(1.0), &meta).unwrap();
                assert!(message.contains("small.nii") && message.contains("4x4x4") && message.contains("8x8x"), "{message}");
                let err = conform(&item, "small.nii", vec![0.0; 64], (4, 4, 4), &diagonal(1.0), &meta).unwrap_err();
                assert_eq!(err.to_string(), message);
            }
        }
        // The acquired matrix of a resampled run, moved.
        let (meta, acquired, mut moved) = run(true);
        moved[3] += 20.0;
        let message = grid_problem(&Item::SuppliedChi, "moved.nii", acquired, &moved, &meta).unwrap();
        assert!(message.contains("placed or oriented differently"), "{message}");
    }
}
