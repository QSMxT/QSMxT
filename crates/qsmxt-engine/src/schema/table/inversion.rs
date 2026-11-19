//! The Inversion section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.do_qsm", Inversion, "Compute QSM", "Enable QSM reconstruction (disable to only run the other outputs)", Bool).basic().flag("--no-qsm").also(&[FieldMap, Background, MultiOrientation, Reference]),
        d("inversion.algorithm", Inversion, "QSM inversion", "", Enum(vec![
            o("rts", "rts", "RTS", "RTS (Rapid Two-Step) — fast two-step k-space inversion with streaking removal. https://doi.org/10.1016/j.neuroimage.2017.11.018"),
            o("tv", "tv", "TV-ADMM", "TV-ADMM (Total Variation) — ADMM inversion with a total-variation prior. https://doi.org/10.1002/mrm.25029"),
            o("tkd", "tkd", "TKD", "TKD (Thresholded K-space Division) — direct k-space division with thresholding. https://doi.org/10.1002/mrm.22135"),
            o("tsvd", "tsvd", "TSVD", "TSVD (Truncated SVD) — zeroes the small singular values of the dipole kernel. https://doi.org/10.1002/mrm.22135"),
            o("tgv", "tgv", "TGV", "TGV (Total Generalized Variation) — single-step: unwrapping, background removal and inversion together. https://doi.org/10.1016/j.neuroimage.2015.02.041"),
            o("tikhonov", "tikhonov", "Tikhonov", "Tikhonov — closed-form L2-regularized inversion. https://doi.org/10.1002/jmri.24365"),
            o("nltv", "nltv", "NLTV", "NLTV (Nonlinear Total Variation) — nonlinear data fidelity with a TV prior. https://doi.org/10.1002/mrm.27073"),
            o("medi", "medi", "MEDI", "MEDI (Morphology Enabled Dipole Inversion) — magnitude-guided edge weighting. https://doi.org/10.1002/mrm.22816"),
            o("tfi", "tfi", "TFI", "TFI (Total Field Inversion) — inverts the total field directly (its own background removal). https://doi.org/10.1002/mrm.26331"),
            o("ilsqr", "ilsqr", "iLSQR", "iLSQR (default) — iterative LSQR with streaking-artifact correction. https://doi.org/10.1016/j.neuroimage.2014.12.043"),
            o("star", "star", "STAR-QSM", "STAR-QSM (Streaking Artifact Reduction) — sparse (L1) inversion from a k-space initial guess, for sources with a large susceptibility range. https://doi.org/10.1002/nbm.3383"),
            o("lsqr", "lsqr", "LSQR", "LSQR (minimally regularised) — one least-squares solve for χ plus a residual field; the residual weight is the only regularisation. With HEIDI ticked, LSQR+HEIDI (HEIDI as published). https://doi.org/10.1118/1.3481505"),
            o("qsmart", "qsmart", "QSMART", "QSMART — two-stage spatially-dependent filtering plus inversion. https://doi.org/10.1016/j.neuroimage.2020.117701"),
            o("ndi", "ndi", "NDI", "NDI (Nonlinear Dipole Inversion) — gradient-descent nonlinear fit. https://doi.org/10.1002/nbm.4271"),
            o("fansi", "fansi", "FANSI (TV)", "FANSI (Nonlinear TV) — fast nonlinear susceptibility inversion with a TV prior. https://doi.org/10.1002/mrm.27073"),
            o("fansi-tgv", "fansi-tgv", "FANSI (TGV)", "FANSI (Nonlinear TGV) — FANSI with a total-generalized-variation prior. https://doi.org/10.1002/mrm.27073"),
            o("l1qsm", "l1qsm", "L1-QSM", "L1-QSM (L1 Data Fidelity) — L1 fidelity for streaking suppression. https://doi.org/10.1002/mrm.29057"),
            o("whqsm", "whqsm", "WH-QSM", "WH-QSM (Weak-Harmonic QSM) — jointly fits a residual harmonic background field. https://doi.org/10.1002/mrm.27483"),
            o("hdqsm", "hdqsm", "HD-QSM", "HD-QSM (Hybrid Data Fidelity) — combined L1/L2 data fidelity. https://doi.org/10.1002/mrm.29296"),
            o("amp-pe", "amp-pe", "AMP-PE", "AMP-PE (Approximate Message Passing with Parameter Estimation) — self-tuning Bayesian inversion. https://doi.org/10.1002/mrm.29725"),
            o("xqsm", "xqsm", "xQSM", "xQSM (deep learning; weights auto-downloaded) — octave-convolution U-net dipole inversion. https://doi.org/10.1002/nbm.4461"),
            o("qsmnet", "qsmnet", "QSMnet", "QSMnet (deep learning; weights auto-downloaded) — 3D U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2018.06.030"),
            o("qsmnet-plus", "qsmnet-plus", "QSMnet+", "QSMnet+ (deep learning; weights auto-downloaded) — QSMnet trained for a wider susceptibility range. https://doi.org/10.1016/j.neuroimage.2020.116579"),
            o("qsmnetme", "qsmnetme", "QSMnetME", "QSMnetME (deep learning; weights auto-downloaded) — the χ-separation toolbox's QSM network, trained on multi-echo data; rotates the field to B0 along z and resamples to 1 mm itself. https://doi.org/10.1002/hbm.70136"),
            o("autoqsm", "autoqsm", "AutoQSM", "AutoQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2019.116064"),
            o("qsmgan", "qsmgan", "QSMGAN", "QSMGAN (deep learning; weights auto-downloaded) — GAN-refined U-net dipole inversion. https://doi.org/10.1016/j.neuroimage.2019.116389"),
            o("ir2qsm", "ir2qsm", "IR2QSM", "IR2QSM (deep learning; weights auto-downloaded) — iterative unrolled U-net inversion. https://doi.org/10.1002/mp.17747"),
            o("lpcnn", "lpcnn", "LPCNN", "LPCNN (deep learning; weights auto-downloaded) — learned-proximal unrolled inversion. https://doi.org/10.1007/978-3-030-59713-9_13"),
            o("modl-qsm", "modl-qsm", "MoDL-QSM", "MoDL-QSM (deep learning; weights auto-downloaded) — model-based unrolled inversion (χ33/STI). https://doi.org/10.1016/j.neuroimage.2021.118376"),
            o("nextqsm", "nextqsm", "NeXtQSM", "NeXtQSM (deep learning; weights auto-downloaded) — single-step reconstruction, no separate background removal. https://doi.org/10.1016/j.neuroimage.2022.119729"),
            o("iqsm", "iqsm", "iQSM", "iQSM (deep learning; weights auto-downloaded) — end-to-end reconstruction from wrapped phase. https://doi.org/10.1016/j.neuroimage.2022.119410"),
            o("iqsm-plus", "iqsm-plus", "iQSM+", "iQSM+ (deep learning; weights auto-downloaded) — orientation-adaptive end-to-end reconstruction. https://doi.org/10.1016/j.media.2024.103160"),
        ])).basic().flag("--qsm-algorithm")
        .gui("How the field is inverted into a susceptibility map"),
        // QSM.rs inversion/rts.rs: no |D| exceeds 2/3, so delta >= 2/3 empties the mask and the map is all zero.
        d("inversion.rts.delta", Inversion, "Delta", "Threshold for ill-conditioned k-space region", Float).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--rts-delta").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.mu", Inversion, "Mu", "Regularization parameter for well-conditioned region", Float).flag("--rts-mu").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.rho", Inversion, "Rho", "ADMM penalty parameter", Float).flag("--rts-rho").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.tol", Inversion, "Tolerance", "Convergence tolerance (relative change)", Float).flag("--rts-tol").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.max_iter", Inversion, "Max iter", "Maximum ADMM iterations", Int).flag("--rts-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")).full("Maximum iterations"),
        d("inversion.rts.lsmr_iter", Inversion, "LSMR iter", "LSMR iterations for step 1 (well-conditioned solve)", Int).flag("--rts-lsmr-iter").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")).full("LSMR iterations"),
        d("inversion.rts.tau", Inversion, "Tau", "Rho is raised when the primal residual falls by less than this factor", Float).flag("--rts-tau").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        d("inversion.rts.gamma", Inversion, "Gamma", "Factor rho is raised by", Float).flag("--rts-gamma").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")),
        // QSM.rs utils/padding.rs with_qsmjl_padding: < 0 leaves an axis unpadded.
        d("inversion.rts.pad", Inversion, "Pad (voxels)", "Zero-padding before the FFTs, voxels (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded (default 0 0 0)", IntVec3).min(-1.0).units("voxels").flag("--rts-pad").parent("inversion.algorithm")
        .when(|x| x.inv("rts") || x.qsmart_inner("rts")).full("Padding")
        .gui("Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded"),
        d("inversion.tv.lambda", Inversion, "Lambda", "L1 regularization weight (larger = smoother)", Float).flag("--tv-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv"))
        .gui("L1 regularisation weight (larger = smoother)"),
        d("inversion.tv.rho", Inversion, "Rho", "ADMM penalty parameter. Blank = 100×lambda", Float).above(0.0).optional().flag("--tv-rho").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        d("inversion.tv.tol", Inversion, "Tolerance", "Convergence tolerance", Float).flag("--tv-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")),
        // QSM.rs inversion/tv.rs: 0 iterations give an all-zero map.
        d("inversion.tv.max_iter", Inversion, "Max iter", "Maximum ADMM iterations", Int).min(1.0).flag("--tv-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")).full("Maximum iterations"),
        // QSM.rs utils/padding.rs with_qsmjl_padding: < 0 leaves an axis unpadded.
        d("inversion.tv.pad", Inversion, "Pad (voxels)", "Zero-padding before the FFTs, voxels (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded (default 0 0 0)", IntVec3).min(-1.0).units("voxels").flag("--tv-pad").parent("inversion.algorithm")
        .when(|x| x.inv("tv") || x.qsmart_inner("tv")).full("Padding")
        .gui("Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded"),
        // QSM.rs inversion/tkd.rs: 1/threshold on the DC bin (D = 0) gives inf/NaN at <= 0.
        d("inversion.tkd.threshold", Inversion, "Threshold", "Truncation threshold for k-space division (0.1-0.3); larger is smoother and underestimates χ more", Float).above(0.0).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--tkd-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("tkd") || x.qsmart_inner("tkd")),
        // QSM.rs utils/padding.rs with_qsmjl_padding: < 0 leaves an axis unpadded.
        d("inversion.tkd.pad", Inversion, "Pad (voxels)", "Zero-padding before the FFTs, voxels (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded (default 0 0 0)", IntVec3).min(-1.0).units("voxels").flag("--tkd-pad").parent("inversion.algorithm")
        .when(|x| x.inv("tkd") || x.qsmart_inner("tkd")).full("Padding")
        .gui("Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded"),
        // QSM.rs inversion/tkd.rs: < 0 inverts D = 0 (NaN); >= 2/3 zeroes every bin.
        d("inversion.tsvd.threshold", Inversion, "Threshold", "Truncation threshold for SVD (0.1-0.2)", Float).min(0.0).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--tsvd-threshold").parent("inversion.algorithm")
        .when(|x| x.inv("tsvd") || x.qsmart_inner("tsvd")),
        // QSM.rs utils/padding.rs with_qsmjl_padding: < 0 leaves an axis unpadded.
        d("inversion.tsvd.pad", Inversion, "Pad (voxels)", "Zero-padding before the FFTs, voxels (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded (default 0 0 0)", IntVec3).min(-1.0).units("voxels").flag("--tsvd-pad").parent("inversion.algorithm")
        .when(|x| x.inv("tsvd") || x.qsmart_inner("tsvd")).full("Padding")
        .gui("Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded"),
        // QSM.rs inversion/tgv.rs: 0 iterations give an all-zero map.
        d("inversion.tgv.iterations", Inversion, "Iterations", "Primal-dual iterations. Blank = from the voxel size, as the reference does", Int).min(1.0).optional().flag("--tgv-iterations").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.erosions", Inversion, "Erosions", "Mask erosions before TGV solve", Int).flag("--tgv-erosions").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        // QSM.rs inversion/tgv.rs project_linf3: a negative weight divides by a zero norm (NaN).
        d("inversion.tgv.alpha1", Inversion, "Alpha1", "First-order TGV weight (gradient term), in radians at the echo time and field strength", Float).min(0.0).units("rad").flag("--tgv-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("tgv"))
        .gui("First-order TGV weight (gradient term), at the echo time and field strength"),
        // QSM.rs inversion/tgv.rs project_linf6: a negative weight divides by a zero norm (NaN).
        d("inversion.tgv.alpha0", Inversion, "Alpha0", "Second-order TGV weight (symmetric gradient term), in radians at the echo time and field strength", Float).min(0.0).units("rad").flag("--tgv-alpha0").parent("inversion.algorithm")
        .when(|x| x.inv("tgv"))
        .gui("Second-order TGV weight (symmetric gradient term), at the echo time and field strength"),
        d("inversion.tgv.step_size", Inversion, "Step size", "Primal step-size multiplier", Float).flag("--tgv-step-size").parent("inversion.algorithm")
        .when(|x| x.inv("tgv")),
        d("inversion.tgv.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--tgv-te").parent("inversion.algorithm")
        .when(|x| x.inv("tgv"))
        .gui("The echo time the field is converted to phase at (several echoes; a single echo is at its own); the weights are in radians at it").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.tikhonov.lambda", Inversion, "Lambda", "L2 regularization weight", Float).flag("--tikhonov-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tikhonov") || x.qsmart_inner("tikhonov"))
        .gui("L2 regularisation weight"),
        // QSM.rs utils/padding.rs with_qsmjl_padding: < 0 leaves an axis unpadded.
        d("inversion.tikhonov.pad", Inversion, "Pad (voxels)", "Zero-padding before the FFTs, voxels (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded (default 0 0 0)", IntVec3).min(-1.0).units("voxels").flag("--tikhonov-pad").parent("inversion.algorithm")
        .when(|x| x.inv("tikhonov") || x.qsmart_inner("tikhonov")).full("Padding")
        .gui("Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded"),
        d("inversion.nltv.lambda", Inversion, "Lambda", "Regularization parameter", Float).flag("--nltv-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv"))
        .gui("Regularisation parameter"),
        d("inversion.nltv.rho", Inversion, "Rho", "ADMM penalty parameter. Blank = 100×lambda", Float).above(0.0).optional().flag("--nltv-rho").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.mu", Inversion, "Mu", "Penalty parameter", Float).flag("--nltv-mu").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        d("inversion.nltv.tol", Inversion, "Tolerance", "Convergence tolerance", Float).flag("--nltv-tol").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")),
        // QSM.rs inversion/nltv.rs: 0 iterations give an all-zero map.
        d("inversion.nltv.max_iter", Inversion, "Max iter", "Maximum ADMM iterations", Int).min(1.0).flag("--nltv-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")).full("Maximum iterations"),
        d("inversion.nltv.tol_newton", Inversion, "Newton tol", "Convergence tolerance of the Newton iterations of the data-term subproblem", Float).flag("--nltv-tol-newton").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")).full("Newton tolerance"),
        // QSM.rs inversion/nltv.rs: 0 Newton iterations give an all-zero map.
        d("inversion.nltv.max_iter_newton", Inversion, "Newton iter", "Maximum Newton iterations of the data-term subproblem", Int).min(1.0).flag("--nltv-newton-iter").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")).full("Newton iterations"),
        d("inversion.nltv.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--nltv-te").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        // QSM.rs utils/padding.rs with_qsmjl_padding: < 0 leaves an axis unpadded.
        d("inversion.nltv.pad", Inversion, "Pad (voxels)", "Zero-padding before the FFTs, voxels (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded (default 0 0 0)", IntVec3).min(-1.0).units("voxels").flag("--nltv-pad").parent("inversion.algorithm")
        .when(|x| x.inv("nltv") || x.qsmart_inner("nltv")).full("Padding")
        .gui("Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded"),
        d("inversion.medi.smv", Inversion, "SMV mode", "High-pass the local field with a spherical mean value (SMV) filter, removing residual background near the brain edge (MEDI-SMV)", Bool).basic().flag("--no-medi-smv").parent("inversion.algorithm")
        .when(|x| x.inv("medi")),
        d("inversion.medi.smv_radius", Inversion, "SMV radius", "SMV radius in mm (also mSMV's)", Float).units("mm").flag("--medi-smv-radius").parent("inversion.algorithm")
        .when(|x| x.inv("medi"))
        .gui("SMV radius (also mSMV's)"),
        d("inversion.medi.smv_shrink_mask", Inversion, "Shrink mask", "With SMV, shrink the mask to where the SMV of the mask exceeds 0.999, dropping the edge band whose SMV reaches outside it. Off keeps the full mask", Bool).flag("--no-medi-smv-shrink-mask").parent("inversion.medi.smv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.smv && !x.c.inversion.medi.msmv).full("SMV mask shrinking"),
        d("inversion.medi.msmv", Inversion, "mSMV", "Filter the local field by maximum spherical mean value (MEDI-mSMV, Roberts 2024) before MEDI; needs at least 3 echoes", Bool).basic().flag("--medi-msmv").parent("inversion.algorithm")
        .when(|x| x.inv("medi")).full("MEDI-mSMV")
        .gui("Filter the local field by maximum spherical mean value (mSMV) before MEDI, which removes residual background at the brain edge; needs at least three echoes"),
        // QSM.rs inversion/msmv.rs: the boundary correction makes at most maxk − 1 passes.
        d("inversion.medi.msmv_maxk", Inversion, "mSMV max K", "mSMV's boundary correction makes at most this many passes, less one", Int).min(1.0).flag("--medi-msmv-maxk").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv).full("mSMV maximum k"),
        d("inversion.medi.msmv_vessel_radius", Inversion, "mSMV vessel radius", "Largest vessel thickness of mSMV's vessel mask, in voxels. Blank = max(15, 8 × the largest voxel dimension)", Float).above(0.0).optional().units("voxels").flag("--medi-msmv-vessel-radius").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv)
        .gui("Largest vessel thickness of mSMV's vessel mask. Empty: 15 voxels, or 8 times the largest voxel dimension if that is more"),
        d("inversion.medi.msmv_prefilter", Inversion, "mSMV prefilter", "SMV-filter the field before mSMV, as the authors do. Off diverges after PDF, V-SHARP or LBV (expert)", Bool).flag("--no-medi-msmv-prefilter").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv),
        d("inversion.medi.lambda", Inversion, "Lambda", "Data-fidelity weight (larger fits the field more closely)", Float).flag("--medi-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("Data-fidelity weight (larger fits the field more closely)"),
        d("inversion.medi.percentage", Inversion, "Percentage", "Fraction of voxels considered edges (0.0-1.0)", Float).flag("--medi-percentage").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        // QSM.rs inversion/medi.rs: 0 iterations give an all-zero map.
        d("inversion.medi.max_iter", Inversion, "Max iter", "Maximum outer iterations", Int).min(1.0).flag("--medi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("Maximum iterations"),
        // QSM.rs inversion/medi.rs: 0 CG iterations give an all-zero map.
        d("inversion.medi.cg_max_iter", Inversion, "CG max iter", "Maximum conjugate gradient iterations", Int).min(1.0).flag("--medi-cg-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("CG maximum iterations"),
        d("inversion.medi.cg_tol", Inversion, "CG tolerance", "CG convergence tolerance", Float).flag("--medi-cg-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.tol", Inversion, "Tolerance", "Outer convergence tolerance", Float).flag("--medi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        d("inversion.medi.merit", Inversion, "MERIT", "Iteratively down-weight voxels whose residual stays large", Bool).flag("--medi-merit").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")),
        // QSM.rs inversion/medi.rs: the CSF term needs R2* from at least 3 echoes; with fewer, MEDI
        // runs without it (and the run says so, requirements::medi_csf_note).
        d("inversion.medi.csf", Inversion, "CSF reference", "Zero-reference χ to the ventricular CSF and keep it uniform there (MEDI+0, Liu 2018); needs at least 3 echoes, with fewer MEDI runs without it", Bool).basic().flag("--medi-csf").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("CSF zero reference (MEDI+0)")
        .gui("Zero-reference the susceptibility to the ventricular CSF, found from R2*, and keep it uniform there (MEDI+0); needs at least three echoes, with fewer MEDI runs without it"),
        d("inversion.medi.lambda_csf", Inversion, "Lambda CSF", "Weight of the CSF uniformity term", Float).min(0.0).flag("--medi-lambda-csf").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf).full("CSF weight"),
        d("inversion.medi.csf_r2s_threshold", Inversion, "CSF R2* threshold", "Voxels below this R2* are CSF candidates", Float).above(0.0).units("Hz").flag("--medi-csf-r2s-threshold").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf),
        d("inversion.medi.csf_erode", Inversion, "CSF erode", "Erode the mask by a 10 mm sphere before finding the CSF", Bool).flag("--medi-csf-erode").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf).full("Erode before finding CSF"),
        d("inversion.medi.zeropad", Inversion, "Zero padding", "Voxels of zeros appended at the end of x, y and z before the inversion; the result is cropped back (default 0 0 0)", IntVec3).units("voxels").flag("--medi-zeropad").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("Voxels of zeros appended at the end of each axis before the inversion; the result is cropped back"),
        d("inversion.medi.data_weighting", Inversion, "Data weighting", "", Enum(vec![
            o(0, "uniform", "Uniform", "Every voxel of the field weighs the same in the data term"),
            o(1, "magnitude SNR", "Magnitude SNR", "Weight the data term by the magnitude SNR, so noisy voxels count for less"),
        ])).flag("--medi-data-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("How the voxels of the field are weighted in the data term"),
        d("inversion.medi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--medi-te").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.tfi.lambda", Inversion, "Lambda", "Data-fidelity weight (larger fits the field more closely)", Float).flag("--tfi-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("Data-fidelity weight (larger fits the field more closely)"),
        d("inversion.tfi.precond", Inversion, "Precond", "Preconditioner value (susceptibility scaling outside the mask)", Float).flag("--tfi-precond").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")).full("Preconditioner"),
        d("inversion.tfi.percentage", Inversion, "Percentage", "Fraction of voxels considered edges (0.0-1.0)", Float).flag("--tfi-percentage").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        // QSM.rs inversion/tfi.rs: 0 iterations give an all-zero map.
        d("inversion.tfi.max_iter", Inversion, "Max iter", "Maximum outer iterations", Int).min(1.0).flag("--tfi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")).full("Maximum iterations"),
        // QSM.rs inversion/tfi.rs: 0 CG iterations give an all-zero map.
        d("inversion.tfi.cg_max_iter", Inversion, "CG max iter", "Maximum conjugate gradient iterations", Int).min(1.0).flag("--tfi-cg-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")).full("CG maximum iterations"),
        d("inversion.tfi.cg_tol", Inversion, "CG tolerance", "CG convergence tolerance", Float).flag("--tfi-cg-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.tol", Inversion, "Tolerance", "Outer convergence tolerance", Float).flag("--tfi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.merit", Inversion, "MERIT", "Iteratively down-weight voxels whose residual stays large", Bool).flag("--tfi-merit").parent("inversion.algorithm")
        .when(|x| x.inv("tfi")),
        d("inversion.tfi.linear", Inversion, "Linear data term", "Fit the field linearly, as the TFI paper does (default); off uses MEDI's exponential data term", Bool).basic().flag("--tfi-linear").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("Fit the field linearly, as the TFI paper does; off uses MEDI's exponential data term"),
        d("inversion.tfi.data_weighting", Inversion, "Data weighting", "", Enum(vec![
            o(0, "uniform", "Uniform", "Every voxel of the field weighs the same in the data term"),
            o(1, "magnitude SNR", "Magnitude SNR", "Weight the data term by the magnitude SNR, so noisy voxels count for less"),
        ])).flag("--tfi-data-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("How the voxels of the field are weighted in the data term"),
        d("inversion.tfi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--tfi-te").parent("inversion.algorithm")
        .when(|x| x.inv("tfi"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.ilsqr.tol", Inversion, "Tolerance", "Convergence tolerance", Float).flag("--ilsqr-tol").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr") || x.qsmart_inner("ilsqr")),
        d("inversion.ilsqr.max_iter", Inversion, "Max iter", "Maximum iterations", Int).flag("--ilsqr-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr")).full("Maximum iterations"),
        // QSM.rs IlsqrParams::validate (inversion/ilsqr.rs): pad must be >= 0.
        d("inversion.ilsqr.pad_mm", Inversion, "Pad (mm)", "Zero-padding per side, mm. Larger costs time and memory", Float).min(0.0).units("mm").flag("--ilsqr-pad-mm").parent("inversion.algorithm")
        .when(|x| x.inv("ilsqr")).full("Padding")
        .gui("Zero-padding per side. Larger costs time and memory"),
        d("inversion.ilsqr.precision", Inversion, "Precision", "", Enum(vec![
            o("double", "double", "Double", "Double precision (default)"),
            o("single", "single", "Single", "Single precision, as the original implementation computes the solves; slower and uses more memory"),
        ])).flag("--ilsqr-precision").parent("inversion.algorithm")
        // QSMART's iLSQR stages take it too.
        .when(|x| x.inv("ilsqr") || x.qsmart_inner("ilsqr"))
        .gui("Arithmetic of the two least-squares solves"),
        d("inversion.star.pad", Inversion, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 8 8 20)", IntVec3).units("voxels").flag("--star-pad").parent("inversion.algorithm")
        .when(|x| x.inv("star")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory"),
        d("inversion.star.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--star-te").parent("inversion.algorithm")
        .when(|x| x.inv("star"))
        .gui("The echo time the field is converted to phase at; STAR-QSM's result depends on it").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.lsqr.residual_weighting", Inversion, "Residual weight", "Weight w on the residual field — the only regularisation. Blank = scaled from B0; 0 drops the term and streaks badly", Float).optional().flag("--lsqr-residual-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr"))
        .gui("Weight w on the residual field, the only regularisation. Empty: scaled from B0; 0 drops the term and streaks badly"),
        d("inversion.lsqr.fit_global_offset", Inversion, "Fit global offset", "Fit a single global field offset alongside χ", Bool).flag("--no-lsqr-global-offset").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr")),
        d("inversion.lsqr.tol", Inversion, "LSQR tolerance", "LSQR convergence tolerance", Float).flag("--lsqr-tol").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr")),
        // QSM.rs inversion/ilsqr.rs lsqr: 0 iterations return x = 0, an all-zero map.
        d("inversion.lsqr.max_iter", Inversion, "LSQR max iter", "Maximum LSQR iterations", Int).min(1.0).flag("--lsqr-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("lsqr")).full("LSQR maximum iterations"),
        // Shown wherever HEIDI can follow the inversion, and wherever it is on (so it can be
        // turned off after a change to a method it cannot follow).
        d("inversion.heidi.enabled", Inversion, "HEIDI", "Streaking reduction after the inversion: keep the map's well-conditioned k-space and refill the dipole cone under a homogeneity prior (HEIDI). With LSQR this is HEIDI as published", Bool).basic().flag("--heidi").parent("inversion.algorithm")
        .when(|x| x.heidi_offered() || x.heidi())
        .full("HEIDI streaking reduction")
        .gui("Streaking reduction after the inversion: HEIDI keeps the map's well-conditioned k-space and refills the dipole cone under a homogeneity prior. With LSQR this is HEIDI as published"),
        // QSM.rs inversion/heidi.rs: no |D| exceeds 2/3, so the cone is empty and the map is all zero.
        d("inversion.heidi.cone_threshold", Inversion, "Cone threshold", "|D(k)| above which a coefficient is kept from the inversion's map unchanged", Float).below(qsm_core::kernels::MAX_ABS_DIPOLE).flag("--heidi-cone-threshold").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()),
        d("inversion.heidi.gradient_threshold", Inversion, "Gradient threshold", "Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous", Float).units("ppm/mm").flag("--heidi-gradient-threshold").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi())
        .gui("Field-gradient threshold; below it a direction reads as homogeneous"),
        d("inversion.heidi.apply_laplacian_correction", Inversion, "Laplacian correction", "Open the homogeneity masks where the field's Laplacian is large", Bool).flag("--no-heidi-laplacian-correction").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()),
        d("inversion.heidi.laplacian_threshold", Inversion, "Laplacian threshold", "Field-Laplacian threshold (ppm per voxel²)", Float).flag("--heidi-laplacian-threshold").parent("inversion.heidi.apply_laplacian_correction").context("HEIDI")
        .when(|x| x.heidi() && x.c.inversion.heidi.apply_laplacian_correction),
        d("inversion.heidi.gradient_mask_floor", Inversion, "Gradient mask floor", "TV weight kept where every direction reads as an edge (0 = upstream's binarised masks)", Float).flag("--heidi-gradient-mask-floor").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()),
        d("inversion.heidi.continuation_steps", Inversion, "Continuation steps", "NESTA continuation steps on μ", Int).flag("--heidi-continuation-steps").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()),
        d("inversion.heidi.inner_iterations", Inversion, "Inner iterations", "Accelerated-gradient iterations per continuation step", Int).flag("--heidi-inner-iterations").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()),
        // QSM.rs inversion/heidi.rs: (mu_min/mu0)^(1/steps) is 0 or NaN at <= 0, so the TV fill never runs.
        d("inversion.heidi.mu_min", Inversion, "Mu min", "Final (smallest) smoothing parameter μ", Float).above(0.0).flag("--heidi-mu-min").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()).full("Final mu"),
        d("inversion.heidi.tol", Inversion, "HEIDI tolerance", "Relative objective change that ends an inner loop early (the default runs every iteration)", Float).flag("--heidi-tol").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()),
        d("inversion.heidi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--heidi-te").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi())
        .gui("The echo time the field is converted to phase at, in the conversion to HEIDI's units").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.heidi.denoise", Inversion, "Denoise field", "Anisotropic-diffusion smoothing of the field before its gradients are taken", Bool).flag("--no-heidi-denoise").parent("inversion.heidi.enabled").context("HEIDI")
        .when(|x| x.heidi()),
        d("inversion.heidi.denoise_iterations", Inversion, "Denoise iter", "Anisotropic-diffusion iterations", Int).flag("--heidi-denoise-iterations").parent("inversion.heidi.denoise").context("HEIDI")
        .when(|x| x.heidi() && x.c.inversion.heidi.denoise).full("Denoise iterations"),
        d("inversion.heidi.denoise_time_step", Inversion, "Time step", "Anisotropic-diffusion time step", Float).flag("--heidi-denoise-time-step").parent("inversion.heidi.denoise").context("HEIDI")
        .when(|x| x.heidi() && x.c.inversion.heidi.denoise),
        d("inversion.heidi.denoise_conductance", Inversion, "Conductance", "Anisotropic-diffusion conductance", Float).flag("--heidi-denoise-conductance").parent("inversion.heidi.denoise").context("HEIDI")
        .when(|x| x.heidi() && x.c.inversion.heidi.denoise),
        d("inversion.heidi.denoise_reference_itk", Inversion, "ITK float arithmetic", "ITK 3.20's float arithmetic for the denoising, as the authors' code runs it (for comparison with it; the default computes its conductance scale in double precision)", Bool).flag("--heidi-denoise-reference-itk").parent("inversion.heidi.denoise").context("HEIDI")
        .when(|x| x.heidi() && x.c.inversion.heidi.denoise),
        d("inversion.qsmart.inversion", Inversion, "Inner inversion", "", Enum(vec![
            o("ilsqr", "ilsqr", "iLSQR", "iLSQR (default) — QSMART's original inner inversion"),
            o("rts", "rts", "RTS", "Rapid Two-Step (RTS)"),
            o("tv", "tv", "TV-ADMM", "Total Variation ADMM (TV)"),
            o("tkd", "tkd", "TKD", "Truncated K-space Division (TKD)"),
            o("tsvd", "tsvd", "TSVD", "Truncated SVD (TSVD)"),
            o("tikhonov", "tikhonov", "Tikhonov", "Tikhonov L2 regularization"),
            o("nltv", "nltv", "NLTV", "Nonlinear Total Variation (NLTV)"),
            o("medi", "medi", "MEDI", "Morphology Enabled Dipole Inversion (MEDI)"),
        ])).basic().flag("--qsmart-inversion").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart"))
        .gui("The dipole inversion QSMART runs in each of its two stages"),
        // QSM.rs inversion/ilsqr.rs: 0 iterations give an all-zero map.
        d("inversion.qsmart.ilsqr_max_iter", Inversion, "iLSQR max iter", "Maximum iLSQR iterations per stage (tolerance: iLSQR's)", Int).min(1.0).flag("--qsmart-ilsqr-max-iter").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")).full("iLSQR maximum iterations"),
        // QSM.rs IlsqrParams::validate (pipeline/qsmart.rs): pad must be >= 0.
        d("inversion.qsmart.ilsqr_pad_mm", Inversion, "iLSQR pad (mm)", "Zero-padding of the inner iLSQR solves, mm per side (default: 6)", Float).min(0.0).units("mm").flag("--qsmart-ilsqr-pad-mm").parent("inversion.qsmart.inversion")
        .when(|x| x.qsmart_inner("ilsqr")).full("iLSQR padding")
        .gui("Zero-padding of the inner iLSQR solves, per side"),
        d("inversion.qsmart.sdf_sigma1_stage1", Inversion, "SDF sigma1 (s1)", "Stage 1 SDF kernel sigma 1 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma1-stage1").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 1, stage 1")
        .gui("Stage 1 SDF kernel sigma 1"),
        d("inversion.qsmart.sdf_sigma2_stage1", Inversion, "SDF sigma2 (s1)", "Stage 1 SDF kernel sigma 2 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma2-stage1").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 2, stage 1")
        .gui("Stage 1 SDF kernel sigma 2"),
        d("inversion.qsmart.sdf_sigma1_stage2", Inversion, "SDF sigma1 (s2)", "Stage 2 SDF kernel sigma 1 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma1-stage2").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 1, stage 2")
        .gui("Stage 2 SDF kernel sigma 1"),
        d("inversion.qsmart.sdf_sigma2_stage2", Inversion, "SDF sigma2 (s2)", "Stage 2 SDF kernel sigma 2 (voxels)", Float).units("voxels").flag("--qsmart-sdf-sigma2-stage2").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF sigma 2, stage 2")
        .gui("Stage 2 SDF kernel sigma 2"),
        d("inversion.qsmart.sdf_lower_lim", Inversion, "SDF lower lim", "SDF proximity lower limit", Float).flag("--qsmart-sdf-lower-lim").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF lower limit"),
        d("inversion.qsmart.sdf_curv_constant", Inversion, "SDF curv const", "SDF curvature constant", Float).flag("--qsmart-sdf-curv-constant").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("SDF curvature constant"),
        d("inversion.qsmart.vessel_magnitude", Inversion, "Vessel magnitude", "", Enum(vec![
            o("mean-n4", "mean-n4", "Mean + N4", "Mean over echoes, N4 bias-corrected, as QSMART prepares it (default)"),
            o("combined", "combined", "Combined", "The combined magnitude: root sum of squares over echoes, inhomogeneity-corrected when that is on"),
        ])).flag("--qsmart-vessel-magnitude").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Vessel detection magnitude")
        .gui("The magnitude vessels are detected on"),
        d("inversion.qsmart.offset_grid", Inversion, "Offset grid", "", Enum(vec![
            o("fft", "fft", "FFT", "The FFT frequency grid (default)"),
            o("qsmart-reference", "qsmart-reference", "QSMART's", "The grid QSMART's adjust_offset.m builds, half a step off the FFT grid on odd matrix sizes, as the QSMART pipeline runs it"),
        ])).flag("--qsmart-offset-grid").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Offset adjustment grid")
        .gui("The dipole grid QSMART's offset adjustment uses"),
        d("inversion.qsmart.vessel_size_units", Inversion, "Vessel size units", "", Enum(vec![
            o("voxels", "voxels", "Voxels", "Vessel radius and scales in voxels, as QSMART gives them (default)"),
            o("mm", "mm", "mm", "Vessel radius and scales in mm, converted to voxels with the mean voxel size"),
        ])).flag("--qsmart-vessel-units").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart"))
        .gui("The units of the vasculature sphere radius and the Frangi scales"),
        d("inversion.qsmart.vasc_sphere_radius", Inversion, "Vasc radius", "Bottom-hat sphere radius for vasculature detection, in the vessel size units", Int).flag("--qsmart-vasc-sphere-radius").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Vasculature sphere radius"),
        d("inversion.qsmart.frangi_scale_min", Inversion, "Frangi min", "Frangi minimum vessel radius, in the vessel size units", Float).flag("--qsmart-frangi-scale-min").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Frangi minimum scale"),
        d("inversion.qsmart.frangi_scale_max", Inversion, "Frangi max", "Frangi maximum vessel radius, in the vessel size units", Float).flag("--qsmart-frangi-scale-max").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Frangi maximum scale"),
        d("inversion.qsmart.frangi_scale_ratio", Inversion, "Frangi step", "Frangi scale step, in the vessel size units", Float).above(0.0).flag("--qsmart-frangi-scale-ratio").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Frangi scale step"),
        d("inversion.qsmart.frangi_c_mode", Inversion, "Frangi C mode", "", Enum(vec![
            o("absolute", "absolute", "Absolute", "Frangi C in the vessel magnitude's intensity units, as QSMART gives it (default)"),
            o("relative", "relative", "Relative", "Frangi C as a factor of the vessel magnitude's median inside the brain mask, the same on every scanner's intensity scale"),
        ])).flag("--qsmart-frangi-c-mode").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart")).full("Frangi C mode")
        .gui("How Frangi's C is set: in magnitude units, or relative to the magnitude"),
        d("inversion.qsmart.frangi_c", Inversion, "Frangi C", "Frangi C noise threshold, in the vessel magnitude's intensity units", Float).above(0.0).flag("--qsmart-frangi-c").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart") && x.c.inversion.qsmart.frangi_c_mode == QsmartFrangiCMode::Absolute),
        d("inversion.qsmart.frangi_c_factor", Inversion, "Frangi C factor", "Frangi C as this factor × the vessel magnitude's median inside the brain mask", Float).above(0.0).flag("--qsmart-frangi-c-factor").parent("inversion.algorithm")
        .when(|x| x.inv("qsmart") && x.c.inversion.qsmart.frangi_c_mode == QsmartFrangiCMode::Relative).full("Frangi C factor"),
        d("inversion.ndi.tau", Inversion, "Tau", "Gradient-descent step size", Float).flag("--ndi-tau").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.ndi.alpha", Inversion, "Alpha", "L2 regularization weight", Float).flag("--ndi-alpha").parent("inversion.algorithm")
        .when(|x| x.inv("ndi"))
        .gui("L2 regularisation weight"),
        // QSM.rs inversion/ndi.rs: 0 iterations give an all-zero map.
        d("inversion.ndi.max_iter", Inversion, "Max iter", "Number of iterations", Int).min(1.0).flag("--ndi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")).full("Iterations"),
        d("inversion.ndi.auto_stop", Inversion, "Auto stop", "Stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto)", Bool).flag("--ndi-auto-stop").parent("inversion.algorithm")
        .when(|x| x.inv("ndi")),
        d("inversion.ndi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--ndi-te").parent("inversion.algorithm")
        .when(|x| x.inv("ndi"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.ndi.weight_norm", Inversion, "Weight normaliser", "", Enum(vec![
            o("global-max", "global-max", "Global maximum", "The magnitude's maximum over the whole image, as FANSI (default)"),
            o("mask-p999", "mask-p999", "Brain 99.9th percentile", "The magnitude's 99.9th percentile inside the mask; more consistent in vivo"),
        ])).flag("--ndi-weight-norm").parent("inversion.algorithm")
        .when(|x| x.inv("ndi"))
        .gui("What the magnitude is divided by to make the data weight"),
        d("inversion.fansi.alpha1", Inversion, "Alpha1", "First-order (TV/TGV) L1 penalty weight", Float).flag("--fansi-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.mu1", Inversion, "Mu1", "Gradient-consistency ADMM weight. Blank = 100×alpha1", Float).above(0.0).optional().flag("--fansi-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.mu2", Inversion, "Mu2", "Fidelity-consistency ADMM weight", Float).flag("--fansi-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.alpha0", Inversion, "Alpha0", "Second-order L1 penalty weight (nlTGV only). Blank = 2×alpha1", Float).min(0.0).optional().flag("--fansi-alpha0").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        d("inversion.fansi.mu0", Inversion, "Mu0", "Second-order consistency ADMM weight (nlTGV only). Blank = 2×mu1", Float).above(0.0).optional().flag("--fansi-mu0").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI"),
        // QSM.rs inversion/fansi.rs: 0 iterations give an all-zero map.
        d("inversion.fansi.max_iter", Inversion, "Max iter", "Number of outer ADMM iterations", Int).min(1.0).flag("--fansi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).full("Iterations").context("FANSI"),
        d("inversion.fansi.tol_update", Inversion, "Tol update", "Percent-update stopping tolerance", Float).flag("--fansi-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).full("Update tolerance").context("FANSI"),
        d("inversion.fansi.tol_delta", Inversion, "Tol delta", "Inner Newton convergence tolerance", Float).flag("--fansi-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).full("Newton tolerance").context("FANSI"),
        d("inversion.fansi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--fansi-te").parent("inversion.algorithm")
        .when(|x| x.inv("fansi") || x.inv("fansi-tgv")).context("FANSI")
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.l1qsm.alpha1", Inversion, "Alpha1", "Gradient (TV) L1 penalty weight", Float).flag("--l1qsm-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu1", Inversion, "Mu1", "Gradient-consistency ADMM weight. Blank = 100×alpha1", Float).above(0.0).optional().flag("--l1qsm-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu2", Inversion, "Mu2", "Fidelity-consistency ADMM weight", Float).flag("--l1qsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.mu3", Inversion, "Mu3", "L1 proximal ADMM weight", Float).flag("--l1qsm-mu3").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        d("inversion.l1qsm.lambda", Inversion, "Lambda", "L1 fidelity strength", Float).flag("--l1qsm-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")),
        // QSM.rs inversion/l1qsm.rs: 0 iterations give an all-zero map.
        d("inversion.l1qsm.max_iter", Inversion, "Max iter", "Number of outer ADMM iterations", Int).min(1.0).flag("--l1qsm-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")).full("Iterations"),
        d("inversion.l1qsm.tol_update", Inversion, "Tol update", "Percent-update stopping tolerance", Float).flag("--l1qsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")).full("Update tolerance"),
        d("inversion.l1qsm.tol_delta", Inversion, "Tol delta", "Inner Newton convergence tolerance", Float).flag("--l1qsm-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm")).full("Newton tolerance"),
        d("inversion.l1qsm.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--l1qsm-te").parent("inversion.algorithm")
        .when(|x| x.inv("l1qsm"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        d("inversion.whqsm.alpha1", Inversion, "Alpha1", "TV regularization weight", Float).flag("--whqsm-alpha1").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm"))
        .gui("TV regularisation weight"),
        d("inversion.whqsm.mu1", Inversion, "Mu1", "ADMM penalty for TV splitting", Float).flag("--whqsm-mu1").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.mu2", Inversion, "Mu2", "ADMM penalty for data-fidelity splitting", Float).flag("--whqsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.beta", Inversion, "Beta", "Weak-harmonic ROI penalty", Float).flag("--whqsm-beta").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")),
        d("inversion.whqsm.muh", Inversion, "Muh", "ADMM penalty for harmonic-field splitting", Float).flag("--whqsm-muh").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Mu H"),
        d("inversion.whqsm.max_iter", Inversion, "Max iter", "Maximum outer iterations", Int).flag("--whqsm-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Maximum iterations"),
        d("inversion.whqsm.tol_update", Inversion, "Tol update", "Percent-update stopping tolerance", Float).flag("--whqsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Update tolerance"),
        d("inversion.whqsm.tol_delta", Inversion, "Tol delta", "Inner Newton stopping tolerance", Float).flag("--whqsm-tol-delta").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm")).full("Newton tolerance"),
        d("inversion.whqsm.weight", Inversion, "Data weight", "", Enum(vec![
            o("flat", "flat", "Flat", "1 over the whole volume, no magnitude, as WH_nlTV.m (default)"),
            o("flat-in-mask", "flat-in-mask", "Flat in mask", "1 inside the mask only, no magnitude"),
            o("max-normalised", "max-normalised", "Max-normalised", "The magnitude over its maximum in the mask; needs magnitude images"),
            o("in-vivo", "in-vivo", "In vivo", "min(2 × magnitude / maximum, 1) in the mask, as NDI weights; needs magnitude images"),
        ])).flag("--whqsm-weight").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm"))
        .gui("How the voxels of the field are weighted in the data term"),
        d("inversion.whqsm.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--whqsm-te").parent("inversion.algorithm")
        .when(|x| x.inv("whqsm"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms"),
        // QSM.rs inversion/hdqsm.rs: sqrt(alpha_l2) is NaN below 0.
        d("inversion.hdqsm.alpha_l2", Inversion, "Alpha L2", "L2-stage TV weight", Float).min(0.0).flag("--hdqsm-alpha-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        // QSM.rs inversion/hdqsm.rs: sqrt(mu1_l2) is NaN below 0 and alpha/mu1 infinite at 0.
        d("inversion.hdqsm.mu1_l2", Inversion, "Mu1 L2", "L2-stage gradient-consistency ADMM weight. Blank = 100×alpha L2", Float).above(0.0).optional().flag("--hdqsm-mu1-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.mu2", Inversion, "Mu2", "Fidelity consistency weight", Float).flag("--hdqsm-mu2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")),
        d("inversion.hdqsm.max_iter_l1", Inversion, "Max iter L1", "Stage-1 (L1) iterations", Int).flag("--hdqsm-max-iter-l1").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")).full("L1 iterations"),
        d("inversion.hdqsm.max_iter_l2", Inversion, "Max iter L2", "Stage-2 (L2) iterations", Int).flag("--hdqsm-max-iter-l2").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")).full("L2 iterations"),
        d("inversion.hdqsm.tol_update", Inversion, "Tol update", "Stage-2 percent-update stopping tolerance", Float).flag("--hdqsm-tol-update").parent("inversion.algorithm")
        .when(|x| x.inv("hdqsm")).full("Update tolerance"),
        // QSM.rs utils/wavelet.rs: only the SUPPORTED_ORDERS (db1–db4) exist.
        d("inversion.amp_pe.wave_order", Inversion, "Wave order", "Daubechies wavelet order (1=db1, 2=db2)", Int).min(*WAVELET_ORDERS.start() as f64).max(*WAVELET_ORDERS.end() as f64).flag("--amp-pe-wave-order").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Wavelet order"),
        // QSM.rs inversion/amp_pe.rs: 1 << nlevel overflows at 64.
        d("inversion.amp_pe.nlevel", Inversion, "N level", "Wavelet decomposition levels", Int).max(qsm_core::inversion::amp_pe::MAX_NLEVEL as f64).flag("--amp-pe-nlevel").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Wavelet levels"),
        d("inversion.amp_pe.wave_pec", Inversion, "Wave pec", "Morphology-mask energy retention fraction (0.0-1.0)", Float).flag("--amp-pe-wave-pec").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Energy retention"),
        d("inversion.amp_pe.simulated_te", Inversion, "Simulated TE", "Simulated echo time (s) used to turn the field into phase", Float).units("s").flag("--amp-pe-simulated-te").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe"))
        .gui("Simulated echo time used to turn the field into phase"),
        // QSM.rs inversion/amp_pe.rs: 0 iterations give an all-zero map.
        d("inversion.amp_pe.max_linearization_ite", Inversion, "Max linearization ite", "Linearization iterations per stage", Int).min(1.0).flag("--amp-pe-max-linearization-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Linearisation iterations")
        .gui("Linearisation iterations per stage"),
        d("inversion.amp_pe.tikhonov_beta", Inversion, "Tikhonov beta", "L2-seed Tikhonov weight", Float).flag("--amp-pe-tikhonov-beta").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")),
        d("inversion.amp_pe.damp_rate_sig", Inversion, "Damp rate (signal)", "GAMP signal-update damping rate", Float).flag("--amp-pe-damp-rate-sig").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Signal damping rate"),
        d("inversion.amp_pe.damp_rate_par", Inversion, "Damp rate (params)", "Parameter-estimation learning rate (kappa)", Float).flag("--amp-pe-damp-rate-par").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Parameter damping rate"),
        d("inversion.amp_pe.max_pe_spar_ite", Inversion, "Sparse recon ite", "Inner sparse-reconstruction iterations", Int).flag("--amp-pe-max-pe-spar-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Sparse reconstruction iterations"),
        d("inversion.amp_pe.max_pe_est_ite", Inversion, "Param est ite", "Inner parameter-estimation iterations", Int).flag("--amp-pe-max-pe-est-ite").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Parameter estimation iterations"),
        d("inversion.amp_pe.cvg_thd", Inversion, "Convergence", "GAMP inner convergence threshold", Float).flag("--amp-pe-cvg-thd").parent("inversion.algorithm")
        .when(|x| x.inv("amp-pe")).full("Convergence threshold"),
        d("inversion.tile_size", Inversion, "Tiled inference", "Run the network on overlapping tiles: bounded memory, approximate. Off = whole volume", Int)
        .value("Tile size", "Overlap-tiling core size in voxels").units("voxels").off(Value::Null).flag("--tile-size").parent("inversion.algorithm")
        .when(|x| x.tileable).gui_value("Tile size", "Core size of each overlapping tile"),
        d("inversion.tile_halo", Inversion, "Tile halo", "Overlap-tiling context margin in voxels (blank = 8)", Int).units("voxels").optional().flag("--tile-halo").parent("inversion.tile_size")
        .when(|x| x.tileable && x.c.inversion.tile_size.is_some())
        .gui("Context margin around each tile (empty = 8)"),
    ]
}
