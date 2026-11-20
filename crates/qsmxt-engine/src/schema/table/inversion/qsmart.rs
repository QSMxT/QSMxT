//! The Inversion section's QSMART settings.

use super::*;

pub(super) fn qsmart() -> Vec<Def> {
    vec![
        d("inversion.qsmart.inversion", Inversion, "Inner inversion", "", Enum(vec![
            o("ilsqr", "ilsqr", "iLSQR", "iLSQR (default) — QSMART's original inner inversion"),
            o("rts", "rts", "RTS", "Rapid Two-Step (RTS)"),
            o("tv", "tv", "TV-ADMM", "Total Variation ADMM (TV)"),
            o("tkd", "tkd", "TKD", "Truncated K-space Division (TKD)"),
            o("tsvd", "tsvd", "TSVD", "Truncated SVD (TSVD)"),
            o("tikhonov", "tikhonov", "Tikhonov", "Tikhonov L2 regularization"),
            o("nltv", "nltv", "NLTV", "Nonlinear Total Variation (NLTV)"),
            o("medi", "medi", "MEDI", "Morphology Enabled Dipole Inversion (MEDI)"),
        ])).basic().flag("--qsmart-inversion")
        .of(Of::Inversion)
        .gui("The dipole inversion QSMART runs in each of its two stages")
        .cli("QSMART inner dipole inversion algorithm")
        .cli_values(&["rts", "tv", "tkd", "tsvd", "tgv", "tikhonov", "nltv", "medi", "tfi", "ilsqr", "star", "lsqr", "heidi", "qsmart", "ndi", "fansi", "fansi-tgv", "l1qsm", "whqsm", "hdqsm", "amp-pe", "xqsm", "qsmnet", "qsmnet-plus", "qsmnetme", "autoqsm", "qsmgan", "ir2qsm", "lpcnn", "modl-qsm", "nextqsm", "iqsm", "iqsm-plus"])
        .value_help("heidi", "LSQR+HEIDI, HEIDI as published: LSQR with HEIDI after it (`--qsm-algorithm lsqr --heidi`)"),
        // QSM.rs inversion/ilsqr.rs: 0 iterations give an all-zero map.
        d("inversion.qsmart.ilsqr_max_iter", Inversion, "iLSQR max iter", "Maximum iLSQR iterations per stage (tolerance: iLSQR's)", Int).min(1.0).flag("--qsmart-ilsqr-max-iter").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")).full("iLSQR maximum iterations")
        .cli("QSMART iLSQR max iterations per stage. The iLSQR tolerance is --ilsqr-tol"),
        // QSM.rs IlsqrParams::validate (pipeline/qsmart.rs): pad must be >= 0.
        d("inversion.qsmart.ilsqr_pad_mm", Inversion, "iLSQR pad (mm)", "Zero-padding of the inner iLSQR solves, mm per side (default: 6)", Float).min(0.0).units("mm").flag("--qsmart-ilsqr-pad-mm").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")).full("iLSQR padding")
        .gui("Zero-padding of the inner iLSQR solves, per side")
        .cli("Zero-padding of QSMART's inner iLSQR solves, in mm per side")
        .value_name("MM"),
        d("inversion.qsmart.sdf_sigma1_stage1", Inversion, "SDF sigma1 (s1)", "Stage 1 SDF kernel sigma 1 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma1-stage1")
        .of(Of::Inversion).full("SDF sigma 1, stage 1")
        .gui("Stage 1 SDF kernel sigma 1")
        .cli("QSMART SDF sigma1, stage 1 (voxels)"),
        d("inversion.qsmart.sdf_sigma2_stage1", Inversion, "SDF sigma2 (s1)", "Stage 1 SDF kernel sigma 2 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma2-stage1")
        .of(Of::Inversion).full("SDF sigma 2, stage 1")
        .gui("Stage 1 SDF kernel sigma 2")
        .cli("QSMART SDF sigma2, stage 1 (voxels)"),
        d("inversion.qsmart.sdf_sigma1_stage2", Inversion, "SDF sigma1 (s2)", "Stage 2 SDF kernel sigma 1 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma1-stage2")
        .of(Of::Inversion).full("SDF sigma 1, stage 2")
        .gui("Stage 2 SDF kernel sigma 1")
        .cli("QSMART SDF sigma1, stage 2 (voxels)"),
        d("inversion.qsmart.sdf_sigma2_stage2", Inversion, "SDF sigma2 (s2)", "Stage 2 SDF kernel sigma 2 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma2-stage2")
        .of(Of::Inversion).full("SDF sigma 2, stage 2")
        .gui("Stage 2 SDF kernel sigma 2")
        .cli("QSMART SDF sigma2, stage 2 (voxels)"),
        d("inversion.qsmart.sdf_lower_lim", Inversion, "SDF lower lim", "SDF proximity lower limit", Float).flag("--qsmart-sdf-lower-lim")
        .of(Of::Inversion).full("SDF lower limit"),
        d("inversion.qsmart.sdf_curv_constant", Inversion, "SDF curv const", "SDF curvature constant", Float).flag("--qsmart-sdf-curv-constant")
        .of(Of::Inversion).full("SDF curvature constant"),
        d("inversion.qsmart.vessel_magnitude", Inversion, "Vessel magnitude", "", Enum(vec![
            o("mean-n4", "mean-n4", "Mean + N4", "Mean over echoes, N4 bias-corrected, as QSMART prepares it (default)"),
            o("combined", "combined", "Combined", "The combined magnitude: root sum of squares over echoes, inhomogeneity-corrected when that is on"),
        ])).flag("--qsmart-vessel-magnitude")
        .of(Of::Inversion).full("Vessel detection magnitude")
        .gui("The magnitude vessels are detected on")
        .cli("The magnitude QSMART detects vessels on: `mean-n4` (the mean over echoes, N4 bias-corrected, as QSMART prepares it; default) or `combined` (the root sum of squares over echoes, inhomogeneity-corrected unless --no-inhomogeneity-correction)"),
        d("inversion.qsmart.offset_grid", Inversion, "Offset grid", "", Enum(vec![
            o("fft", "fft", "FFT", "The FFT frequency grid (default)"),
            o("qsmart-reference", "qsmart-reference", "QSMART's", "The grid QSMART's adjust_offset.m builds, half a step off the FFT grid on odd matrix sizes, as the QSMART pipeline runs it"),
        ])).flag("--qsmart-offset-grid")
        .of(Of::Inversion).full("Offset adjustment grid")
        .gui("The dipole grid QSMART's offset adjustment uses")
        .cli("The dipole grid of QSMART's offset adjustment: `fft` (default) or `qsmart-reference` (`adjust_offset.m`'s own, half a step off the FFT grid on odd matrix sizes, as the QSMART pipeline runs it)"),
        d("inversion.qsmart.vessel_size_units", Inversion, "Vessel size units", "", Enum(vec![
            o("voxels", "voxels", "Voxels", "Vessel radius and scales in voxels, as QSMART gives them (default)"),
            o("mm", "mm", "mm", "Vessel radius and scales in mm, converted to voxels with the mean voxel size"),
        ])).flag("--qsmart-vessel-units")
        .of(Of::Inversion)
        .gui("The units of the vasculature sphere radius and the Frangi scales")
        .cli("Units of QSMART's vessel sizes (--qsmart-vasc-sphere-radius and the Frangi scales): `voxels` (default, as QSMART gives them) or `mm` (converted to voxels with the mean voxel size)"),
        d("inversion.qsmart.vasc_sphere_radius", Inversion, "Vasc radius", "Bottom-hat sphere radius for vasculature detection, in the vessel size units", Int).flag("--qsmart-vasc-sphere-radius")
        .of(Of::Inversion).full("Vasculature sphere radius")
        .cli("QSMART vasculature detection sphere radius, in --qsmart-vessel-units")
        .parse(P::I32),
        d("inversion.qsmart.frangi_scale_min", Inversion, "Frangi min", "Frangi minimum vessel radius, in the vessel size units", Float).flag("--qsmart-frangi-scale-min")
        .of(Of::Inversion).full("Frangi minimum scale")
        .cli("QSMART Frangi min vessel radius, in --qsmart-vessel-units"),
        d("inversion.qsmart.frangi_scale_max", Inversion, "Frangi max", "Frangi maximum vessel radius, in the vessel size units", Float).flag("--qsmart-frangi-scale-max")
        .of(Of::Inversion).full("Frangi maximum scale")
        .cli("QSMART Frangi max vessel radius, in --qsmart-vessel-units"),
        d("inversion.qsmart.frangi_scale_ratio", Inversion, "Frangi step", "Frangi scale step, in the vessel size units", Float).above(0.0).flag("--qsmart-frangi-scale-ratio")
        .of(Of::Inversion).full("Frangi scale step")
        .cli("QSMART Frangi scale step, in --qsmart-vessel-units"),
        d("inversion.qsmart.frangi_c_mode", Inversion, "Frangi C mode", "", Enum(vec![
            o("absolute", "absolute", "Absolute", "Frangi C in the vessel magnitude's intensity units, as QSMART gives it (default)"),
            o("relative", "relative", "Relative", "Frangi C as a factor of the vessel magnitude's median inside the brain mask, the same on every scanner's intensity scale"),
        ])).flag("--qsmart-frangi-c-mode")
        .of(Of::Inversion).full("Frangi C mode")
        .gui("How Frangi's C is set: in magnitude units, or relative to the magnitude")
        .cli("How QSMART's Frangi C is set: `absolute` (--qsmart-frangi-c, in magnitude units; default, as QSMART gives it) or `relative` (--qsmart-frangi-c-factor × the vessel magnitude's median inside the brain mask, the same on every scanner's intensity scale)"),
        d("inversion.qsmart.frangi_c", Inversion, "Frangi C", "Frangi C noise threshold, in the vessel magnitude's intensity units", Float).above(0.0).flag("--qsmart-frangi-c")
        .of(Of::Inversion).when(|x| x.c.inversion.qsmart.frangi_c_mode == QsmartFrangiCMode::Absolute)
        .cli("QSMART Frangi C noise threshold, in the vessel magnitude's intensity units (default 500, QSMART v1.0's); used with --qsmart-frangi-c-mode absolute"),
        d("inversion.qsmart.frangi_c_factor", Inversion, "Frangi C factor", "Frangi C as this factor × the vessel magnitude's median inside the brain mask", Float).above(0.0).flag("--qsmart-frangi-c-factor")
        .of(Of::Inversion).when(|x| x.c.inversion.qsmart.frangi_c_mode == QsmartFrangiCMode::Relative).full("Frangi C factor")
        .cli("QSMART's relative Frangi C: this factor × the vessel magnitude's median inside the brain mask (default 0.25); used with --qsmart-frangi-c-mode relative"),
    ]
}
