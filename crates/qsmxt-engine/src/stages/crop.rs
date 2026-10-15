//! The box the FFT-based stages reconstruct in.

use crate::config::PipelineConfig;
use crate::scan::RunMetadata;

/// The box the FFT-based stages (background removal, dipole inversion) reconstruct in, for a
/// stage working inside `mask`. `stage` names it in the log.
///
/// Two levers on the same cost, both opt-in, for different reasons.
///
/// `--fft-padding` grows the grid to a size `rustfft` likes. It discards nothing and an awkward
/// grid can get most of a transform's time back, but it changes where the dipole kernel is
/// sampled in k-space, so the reconstruction shifts slightly. `--crop-to-mask` reconstructs only
/// around the brain, which is faster still but genuinely discards field. See `qsm_core::crop`.
pub fn crop_box_for(
    config: &PipelineConfig, meta: &RunMetadata, mask: &[u8], stage: &str,
) -> qsm_core::crop::CropBox {
    let cropping = config.pipeline.crop_to_mask;
    let margin = config.pipeline.crop_margin_mm;
    let b = reconstruction_box(
        cropping, config.pipeline.fft_padding, mask, meta.dims, meta.voxel_size, margin,
    );

    if b.dims == meta.dims {
        return b;
    }
    if cropping {
        log::info!(
            "{}: reconstructing in {}x{}x{} instead of {}x{}x{} ({:.1}x fewer voxels, {:.0} mm margin)",
            stage, b.dims.0, b.dims.1, b.dims.2,
            b.full_dims.0, b.full_dims.1, b.full_dims.2, b.reduction(), margin,
        );
    } else {
        log::info!(
            "{}: padding {}x{}x{} to {}x{}x{} for a cheaper FFT",
            stage, b.full_dims.0, b.full_dims.1, b.full_dims.2,
            b.dims.0, b.dims.1, b.dims.2,
        );
    }
    b
}

/// Which box, given the setting — split out from the logging so it can be tested directly.
///
/// Falls back to the full grid when the chosen box would not change it, since copying volumes in
/// and out only pays for itself if the grid actually moves.
pub fn reconstruction_box(
    crop_to_mask: bool,
    fft_padding: bool,
    mask: &[u8],
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    margin_mm: f64,
) -> qsm_core::crop::CropBox {
    let b = if crop_to_mask {
        qsm_core::crop::crop_box_for_mask(mask, dims, voxel_size, margin_mm)
    } else if fft_padding {
        qsm_core::crop::fft_pad_box(dims)
    } else {
        qsm_core::crop::CropBox::full(dims)
    };
    if b.dims == dims {
        qsm_core::crop::CropBox::full(dims)
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::reconstruction_box;

    /// The grid a 32.5-degree oblique UK Biobank SWI resamples to. Awkward on every axis:
    /// 272 = 2^4 * 17, 339 = 3 * 113, 77 = 7 * 11.
    const UKB_RESAMPLED: (usize, usize, usize) = (272, 339, 77);

    #[test]
    fn reconstruction_box_pads_to_a_friendly_size_when_asked() {
        let mask = vec![1u8; 8];
        let b = reconstruction_box(false, true, &mask, UKB_RESAMPLED, (0.8, 0.8, 3.0), 32.0);
        assert_eq!(b.dims, (280, 343, 80), "every awkward axis should grow");
        assert!(b.dims.0 >= UKB_RESAMPLED.0 && b.dims.1 >= UKB_RESAMPLED.1 && b.dims.2 >= UKB_RESAMPLED.2,
                "padding must never discard a voxel");
    }

    #[test]
    fn reconstruction_box_leaves_an_already_friendly_grid_alone() {
        let mask = vec![1u8; 8];
        let friendly = (256, 288, 48); // 2^8, 2^5*3^2, 2^4*3
        let b = reconstruction_box(false, true, &mask, friendly, (0.8, 0.8, 3.0), 32.0);
        assert_eq!(b.dims, friendly, "nothing to gain, so no copy");
        assert_eq!(b.origin, (0, 0, 0));
    }

    #[test]
    fn reconstruction_box_does_not_pad_by_default() {
        let mask = vec![1u8; 8];
        let b = reconstruction_box(false, false, &mask, UKB_RESAMPLED, (0.8, 0.8, 3.0), 32.0);
        assert_eq!(b.dims, UKB_RESAMPLED,
                   "padding changes where k-space is sampled, so it must be asked for");
        assert_eq!(b.origin, (0, 0, 0));
    }

    #[test]
    fn reconstruction_box_crops_to_the_mask_when_asked() {
        // A small blob in the middle of a large grid: the case cropping is actually for.
        let dims = (64, 64, 64);
        let mut mask = vec![0u8; dims.0 * dims.1 * dims.2];
        for k in 30..34 {
            for j in 30..34 {
                for i in 30..34 {
                    mask[i + j * dims.0 + k * dims.0 * dims.1] = 1;
                }
            }
        }
        let cropped = reconstruction_box(true, false, &mask, dims, (1.0, 1.0, 1.0), 4.0);
        assert!(cropped.dims.0 < dims.0, "a 4-voxel blob with a 4 mm margin should shrink 64");
        let padded = reconstruction_box(false, true, &mask, dims, (1.0, 1.0, 1.0), 4.0);
        assert_eq!(padded.dims, dims, "padding ignores the mask; 64 is already friendly");
    }
}
