//! The Geometry section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.axial_resampling", Geometry, "Axial resampling", "Resample oblique acquisitions onto an axial grid when they are tilted more than the threshold. Off: reconstruct on the acquired grid with the true B0 direction", Bool).basic().flag("--axial-resampling")
        .cli("Resample oblique acquisitions onto an axial grid when their obliquity exceeds --obliquity-threshold. Off by default (methods that only work on axial data resample regardless)")
        .negated("--no-axial-resampling", "Do not resample oblique acquisitions to axial (overrides a config's axial_resampling)").negated_conflicts(),
        // QSMxT: --obliquity-threshold and the config file refuse a negative threshold (PipelineConfig::from_toml_migrating).
        d("pipeline.obliquity_threshold", Geometry, "Obliquity threshold (°)", "Resample only data more oblique than this, in degrees", Float).min(0.0).units("deg").flag("--obliquity-threshold").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling).full("Obliquity threshold")
        .gui("Resample only data more oblique than this")
        .cli("Obliquity in degrees above which --axial-resampling resamples")
        .attrs(&["allow_negative_numbers"]),
        d("pipeline.output_space", Geometry, "Output space", "", Enum(vec![
            o("acquired", "acquired", "Acquired grid", "Write derivatives back on the acquired grid when the run was resampled to axial (default)"),
            o("working", "working", "Working grid", "Leave derivatives on the resampled (axial) working grid, avoiding a second interpolation"),
        ])).basic().flag("--output-space").parent("pipeline.axial_resampling")
        .when(|x| x.c.pipeline.axial_resampling || x.axial_forced)
        .gui("The grid derivatives are written on, when an oblique run was resampled to axial")
        .cli("Grid to write derivatives on when the run was resampled to axial. `acquired` (default) puts them back where the data came from, so transforms you already hold still apply; `working` leaves them on the resampled grid and avoids a second interpolation"),
        d("pipeline.ignore_slice_gap", Geometry, "Ignore slice gap", "Reconstruct even if the sidecar's SliceThickness is thinner than the slice pitch (gapped 2D data gives a wrong map)", Bool).flag("--ignore-slice-gap")
        .cli("Reconstruct even when a 2D acquisition's sidecar `SliceThickness` is thinner than the slice pitch. Without it, QSM refuses such a run: the gap between slices is unsampled tissue, and the FFT kernels assume a contiguous grid, so the map would be wrong"),
        d("pipeline.tukey_window", Geometry, "Tukey window", "Taper ratio of a k-space Tukey window applied to each echo's complex image before every other step: 0 applies none; the χ-separation toolbox uses 0.4, and 0 for Philips data. Needs magnitude", Float).min(0.0).max(1.0).flag("--tukey-window").full("Tukey window in k-space")
        .cli("Apply a k-space Tukey window with this taper ratio (0 to 1) to each echo's complex image before every other step, as the χ-separation toolbox does: 0.4, and 0 for Philips data. Default 0 (none). Needs magnitude images"),
        d("pipeline.crop_to_mask", Geometry, "Crop to mask", "Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid). Off by default", Bool).flag("--crop-to-mask")
        .when(|x| x.qsm)
        .gui("Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid)")
        .cli("Reconstruct inside a box around the mask instead of the whole field of view. Cuts the FFT-bound stages roughly in proportion to the voxels removed, but those stages are a small share of a full run, so the end-to-end gain is usually modest. Off by default. Outputs are written on the full grid either way; this only changes where the arithmetic happens. Padding to an FFT-friendly size happens regardless"),
        d("pipeline.crop_margin_mm", Geometry, "Crop margin (mm)", "Margin around the mask; below ~32 mm results start to differ, and it must clear the largest SMV kernel", Float).units("mm").flag("--crop-margin-mm").parent("pipeline.crop_to_mask")
        .when(|x| x.qsm && x.c.pipeline.crop_to_mask).full("Crop margin")
        .gui("Margin around the mask; below about 32 mm results start to differ, and it must clear the largest SMV kernel")
        .cli("Margin in mm around the mask when --crop-to-mask is set (default 32). Below ~32 mm the reconstruction starts to differ from the uncropped one; it must also clear the largest SMV kernel in use, and V-SHARP defaults to 12 mm"),
        d("pipeline.fft_padding", Geometry, "FFT padding", "Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly). Off by default", Bool).flag("--fft-padding")
        .when(|x| x.qsm)
        .gui("Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly)")
        .cli("Pad the grid outward to an FFT-friendly size before the FFT-bound stages. Much faster on an awkward grid, and discards nothing, but it changes where k-space is sampled and so shifts the reconstruction slightly. Off by default"),
        d("field_mapping.coil_combination_sigma", Geometry, "Coil comb. sigma", "MCPC-3D-S smoothing of per-coil phase offsets, voxels (x y z); only for uncombined multi-coil data", FloatVec3).units("voxels").flag("--coil-combination-sigma")
        .when(|x| x.multi_coil).full("Coil combination sigma")
        .gui("MCPC-3D-S smoothing of the per-coil phase offsets (x y z); only for uncombined multi-coil data")
        .cli("MCPC-3D-S coil combination: smoothing sigma for the per-coil phase offsets (3 values, in voxels; default 10 10 5, as in MriResearchTools). Only used for uncombined multi-coil runs"),
    ]
}
