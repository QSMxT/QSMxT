//! The Geometry section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.axial_resampling", Geometry, "Axial resampling", "Resample oblique acquisitions onto an axial grid when they are tilted more than the threshold. Off: reconstruct on the acquired grid with the true B0 direction", Bool).basic().flag("--axial-resampling"),
        // QSMxT: --obliquity-threshold and the config file refuse a negative threshold (src/pipeline/config.rs, PipelineConfig::from_toml_migrating).
        d("pipeline.obliquity_threshold", Geometry, "Obliquity threshold (°)", "Resample only data more oblique than this, in degrees", Float).min(0.0).units("deg").flag("--obliquity-threshold").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling).full("Obliquity threshold")
        .gui("Resample only data more oblique than this"),
        d("pipeline.output_space", Geometry, "Output space", "", Enum(vec![
            o("acquired", "acquired", "Acquired grid", "Write derivatives back on the acquired grid when the run was resampled to axial (default)"),
            o("working", "working", "Working grid", "Leave derivatives on the resampled (axial) working grid, avoiding a second interpolation"),
        ])).basic().flag("--output-space").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling || x.axial_forced)
        .gui("The grid derivatives are written on, when an oblique run was resampled to axial"),
        d("pipeline.ignore_slice_gap", Geometry, "Ignore slice gap", "Reconstruct even if the sidecar's SliceThickness is thinner than the slice pitch (gapped 2D data gives a wrong map)", Bool).flag("--ignore-slice-gap"),
        d("pipeline.tukey_window", Geometry, "Tukey window", "Taper ratio of a k-space Tukey window applied to each echo's complex image before every other step: 0 applies none; the χ-separation toolbox uses 0.4, and 0 for Philips data. Needs magnitude", Float).min(0.0).max(1.0).flag("--tukey-window").full("Tukey window in k-space"),
        d("pipeline.crop_to_mask", Geometry, "Crop to mask", "Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid). Off by default", Bool).flag("--crop-to-mask")
        .when(|x| x.qsm)
        .gui("Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid)"),
        d("pipeline.crop_margin_mm", Geometry, "Crop margin (mm)", "Margin around the mask; below ~32 mm results start to differ, and it must clear the largest SMV kernel", Float).units("mm").flag("--crop-margin-mm").parent("pipeline.crop_to_mask")
        .when(|x| x.qsm && x.c.pipeline.crop_to_mask).full("Crop margin")
        .gui("Margin around the mask; below about 32 mm results start to differ, and it must clear the largest SMV kernel"),
        d("pipeline.fft_padding", Geometry, "FFT padding", "Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly). Off by default", Bool).flag("--fft-padding")
        .when(|x| x.qsm)
        .gui("Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly)"),
        d("field_mapping.coil_combination_sigma", Geometry, "Coil comb. sigma", "MCPC-3D-S smoothing of per-coil phase offsets, voxels (x y z); only for uncombined multi-coil data", FloatVec3).units("voxels").flag("--coil-combination-sigma")
        .when(|x| x.multi_coil).full("Coil combination sigma")
        .gui("MCPC-3D-S smoothing of the per-coil phase offsets (x y z); only for uncombined multi-coil data"),
    ]
}
