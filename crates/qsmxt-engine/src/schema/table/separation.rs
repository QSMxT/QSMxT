//! The χ-separation section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.do_chi_separation", Separation, "Compute χ-separation", "Split QSM into paramagnetic (χ+, iron) and diamagnetic (χ−, myelin) maps", Bool).basic().flag("--do-chisep")
        .cli("Compute chi-separation (paramagnetic/diamagnetic susceptibility maps)")
        .cli_id("do_chi_separation"),
        d("separation.algorithm", Separation, "Method", "", Enum(vec![
            o("r2star-qsm", "r2star-qsm", "R2*-QSM", "R2*-QSM closed-form (Dimov 2022) — QSM + R2*, GRE-only"),
            o("decompose", "decompose", "DECOMPOSE-QSM", "DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude, GRE-only"),
            o("chi-sep-ilsqr", "chi-sep-ilsqr", "χ-separation (iLSQR)", "χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM"),
            o("chi-sep-medi", "chi-sep-medi", "χ-separation (MEDI)", "χ-separation MEDI — local field + R2' + magnitude"),
            o("wavesep", "wavesep", "WaveSep", "WaveSep wavelet-L1 (Fang 2023) — QSM + R2'"),
            o("hc-chisep", "hc-chisep", "Hollow-cylinder χ-separation", "Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude"),
            o("susep-net", "susep-net", "SUSEP-Net", "SUSEP-Net deep-learning separation — QSM + R2' + local field (weights auto-downloaded)"),
            o("chi-sepnet", "chi-sepnet", "χ-sepnet", "χ-sepnet deep-learning separation — QSM + R2' + local field (weights auto-downloaded)"),
        ])).basic().flag("--chisep").parent("pipeline.do_chi_separation")
        .when(|x| x.c.pipeline.do_chi_separation).full("χ-separation method")
        .gui("The χ-separation method, and the maps it reads")
        .gui_option("r2star-qsm", "R2*-QSM closed-form — QSM + R2*, GRE-only")
        .gui_option("decompose", "DECOMPOSE-QSM signal-domain fit — QSM + multi-echo magnitude, GRE-only")
        .gui_option("chi-sep-ilsqr", "χ-separation iLSQR — local field + R2' + magnitude + QSM")
        .gui_option("wavesep", "WaveSep wavelet-L1 — QSM + R2'")
        .gui_option("hc-chisep", "Hollow-cylinder χ-separation — QSM + R2' + multi-echo magnitude")
        .cli("Chi-separation method (default: r2star-qsm)")
        .cli_id("chi_separation_algorithm")
        .value_help("r2star-qsm", "R2*-QSM closed-form (Dimov 2022) — QSM + R2* (GRE-only)").value_help("decompose", "DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude (GRE-only)").value_help("chi-sep-ilsqr", "χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM").value_help("chi-sep-medi", "χ-separation MEDI — local field + R2' + magnitude").value_help("wavesep", "WaveSep wavelet-L1 (Fang 2023) — QSM + R2'").value_help("hc-chisep", "Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude").value_help("susep-net", "SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use)").value_help("chi-sepnet", "χ-sepnet deep-learning separation — QSM + R2' + local field (weights downloaded on first use)"),
        d("separation.r2star_qsm.r_const_3t", Separation, "Relaxometric const @3T", "Hz/ppm at 3T (scales with B0)", Float).units("Hz/ppm").flag("--r2star-qsm-r-const-3t").parent("separation.algorithm")
        .when(|x| x.sep("r2star-qsm")).full("Relaxometric constant at 3 T")
        .gui("Relaxometric constant at 3 T (scales with B0)")
        .cli("R2*-QSM relaxometric constant at 3T (Hz/ppm)"),
        d("separation.decompose.n_inner", Separation, "Inner passes", "Alternating 3-stage fit passes per voxel", Int).flag("--decompose-n-inner").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Inner passes")
        .cli("DECOMPOSE inner alternating passes per voxel"),
        // QSM.rs separation/decompose.rs: a negative bound makes lb > ub and f64::clamp panics.
        d("separation.decompose.chi_bound", Separation, "Chi bound", "", NumberOr(vec![
            o("echo-spacing", "echo-spacing", "From the echo spacing",
              "The largest |χ| whose phase changes by less than π between echoes, from the field strength and echo spacing"),
        ])).value("Chi bound (ppm)", "Upper bound on |χ| in the fit, ppm")
        .min(0.0).units("ppm").flag("--decompose-chi-bound").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("χ bound")
        .gui("The bound on |χ| in the fit: a fixed bound, or one from the field strength and echo spacing")
        .gui_value("χ bound", "A fixed bound on |χ| in the fit")
        .cli("DECOMPOSE bound on |χ| in the fit: a number in ppm (default: 0.5), or echo-spacing to compute it from the field strength and echo spacing")
        .value_name("BOUND"),
        d("separation.decompose.unwrap_phase", Separation, "Unwrap phase", "Unwrap the phase along the echoes (default: off, as in the authors' code)", Bool).flag("--decompose-unwrap-phase").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Echo-phase unwrapping")
        .gui("Unwrap the phase along the echoes")
        .cli("DECOMPOSE: unwrap the phase along the echoes (default: false, as in the authors' code)")
        .bool_value().value_name("BOOL"),
        d("separation.decompose.fit", Separation, "Fit", "", Enum(vec![
            o("reference-code", "reference-code", "Reference code", "Tim Ho's script: lsqcurvefit for every stage, magnitudes scaled by their global maximum (default)"),
            o("paper", "paper", "Paper", "Chen et al. 2021: amplitudes summing to one and R2*0 by linear least squares, χ± by lsqnonlin, each voxel scaled to S(0) = 1"),
        ])).flag("--decompose-fit").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("DECOMPOSE fit")
        .gui("The fit DECOMPOSE runs: its reference code's or its paper's")
        .cli("DECOMPOSE's fit: `reference-code` (Tim Ho's script; default) or `paper` (Chen et al. 2021: amplitudes summing to one, per-voxel S(0) = 1 scale), as the DECOMPOSE pipeline runs it"),
        // Each stage's fit: MATLAB lsqcurvefit's options (trust-region-reflective).
        d("separation.decompose.lsqcurvefit.max_iterations", Separation, "Max iter", "Maximum solver iterations per fit (lsqcurvefit MaxIterations)", Int).min(1.0).flag("--decompose-max-iterations").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Maximum iterations")
        .cli("DECOMPOSE: maximum solver iterations of each fit (lsqcurvefit MaxIterations; default: 400)"),
        d("separation.decompose.lsqcurvefit.max_function_evaluations", Separation, "Max evaluations", "Maximum function evaluations per fit (lsqcurvefit MaxFunctionEvaluations). Blank = 100 × the fit's parameters", Int).min(1.0).optional().flag("--decompose-max-function-evaluations").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Maximum function evaluations")
        .cli("DECOMPOSE: maximum function evaluations of each fit (lsqcurvefit MaxFunctionEvaluations; default: 100 × the fit's parameters)"),
        d("separation.decompose.lsqcurvefit.function_tolerance", Separation, "Function tol", "Stop when the cost changes by less than this, relatively (lsqcurvefit FunctionTolerance)", Float).min(0.0).flag("--decompose-function-tolerance").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Function tolerance")
        .cli("DECOMPOSE: relative cost change each fit stops at (lsqcurvefit FunctionTolerance; default: 1e-6)"),
        d("separation.decompose.lsqcurvefit.step_tolerance", Separation, "Step tol", "Stop when the step is smaller than this, relatively (lsqcurvefit StepTolerance)", Float).min(0.0).flag("--decompose-step-tolerance").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Step tolerance")
        .cli("DECOMPOSE: relative step each fit stops at (lsqcurvefit StepTolerance; default: 1e-6)"),
        d("separation.decompose.lsqcurvefit.optimality_tolerance", Separation, "Optimality tol", "Stop when the first-order optimality falls below this (lsqcurvefit OptimalityTolerance)", Float).min(0.0).flag("--decompose-optimality-tolerance").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Optimality tolerance")
        .cli("DECOMPOSE: first-order optimality each fit stops at (lsqcurvefit OptimalityTolerance; default: 1e-6)"),
        d("separation.decompose.lsqcurvefit.finite_difference_step", Separation, "FD step", "Relative finite-difference step of the Jacobian (lsqcurvefit FiniteDifferenceStepSize)", Float).above(0.0).flag("--decompose-finite-difference-step").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Finite-difference step")
        .cli("DECOMPOSE: relative finite-difference step of the Jacobian (lsqcurvefit FiniteDifferenceStepSize; default: √eps)"),
        d("separation.decompose.lsqcurvefit.typical_x", Separation, "Typical x", "Typical magnitude of each fitted value, for the finite differences (lsqcurvefit TypicalX)", Float).above(0.0).flag("--decompose-typical-x").parent("separation.algorithm")
        .when(|x| x.sep("decompose")).full("Typical x")
        .cli("DECOMPOSE: typical magnitude of each fitted value (lsqcurvefit TypicalX; default: 1)"),
        // QSM.rs ChiSepIlsqrParams::validate: D_r finite and > 0; cone > 0; CG tolerances in (0, 1);
        // percentiles 0 <= min < max <= 100 (the pairs are checked by qsm-core).
        d("separation.chi_sep_ilsqr.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T", Float).above(0.0).optional().units("Hz/ppm").flag("--chi-sep-ilsqr-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Paramagnetic constant (Dr+)")
        .cli("χ-sep iLSQR paramagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto)")
        .parse(P::NumberOrAuto).value_name("HZ_PER_PPM"),
        d("separation.chi_sep_ilsqr.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T", Float).above(0.0).optional().units("Hz/ppm").flag("--chi-sep-ilsqr-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Diamagnetic constant (Dr−)")
        .cli("χ-sep iLSQR diamagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto)")
        .parse(P::NumberOrAuto).value_name("HZ_PER_PPM"),
        d("separation.chi_sep_ilsqr.pad", Separation, "Pad (voxels)", "Zero-padding at both ends of each axis, voxels (x y z) (default 16 16 16)", IntVec3).units("voxels").flag("--chi-sep-ilsqr-pad").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Padding")
        .gui("Zero-padding at both ends of each axis (x y z)")
        .cli("χ-sep iLSQR zero-padding at both ends of each axis in voxels, X Y Z (default: 16 16 16)"),
        d("separation.chi_sep_ilsqr.cg_max_iter_lsqr", Separation, "LSQR CG iter", "Maximum CG iterations of the LSQR step", Int).flag("--chi-sep-ilsqr-cg-max-iter-lsqr").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("LSQR CG maximum iterations")
        .cli("χ-sep iLSQR: maximum CG iterations of the LSQR step (default: 50)"),
        d("separation.chi_sep_ilsqr.cg_tol_lsqr", Separation, "LSQR CG tol", "CG relative tolerance of the LSQR step", Float).above(0.0).below(1.0).flag("--chi-sep-ilsqr-cg-tol-lsqr").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("LSQR CG tolerance")
        .cli("χ-sep iLSQR: CG relative tolerance of the LSQR step (default: 0.02)"),
        d("separation.chi_sep_ilsqr.cg_max_iter_sa", Separation, "Streaking CG iter", "Maximum CG iterations of the streaking-removal step, per source", Int).flag("--chi-sep-ilsqr-cg-max-iter-sa").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Streaking-removal CG maximum iterations")
        .cli("χ-sep iLSQR: maximum CG iterations of the streaking-removal step (default: 50)"),
        d("separation.chi_sep_ilsqr.cg_tol_sa", Separation, "Streaking CG tol", "CG relative tolerance of the streaking-removal step", Float).above(0.0).below(1.0).flag("--chi-sep-ilsqr-cg-tol-sa").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Streaking-removal CG tolerance")
        .cli("χ-sep iLSQR: CG relative tolerance of the streaking-removal step (default: 0.02)"),
        d("separation.chi_sep_ilsqr.d2_thr", Separation, "Cone threshold", "The streaking cone is where the dipole kernel |D| is below this", Float).above(0.0).flag("--chi-sep-ilsqr-d2-thr").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Streaking cone threshold")
        .cli("χ-sep iLSQR: the streaking cone is where |D| is below this (default: 0.11)"),
        d("separation.chi_sep_ilsqr.prc_w_i_min", Separation, "Laplacian weight min %", "Percentile of the field's Laplacian at which the LSQR weight is 1", Float).min(0.0).max(100.0).flag("--chi-sep-ilsqr-prc-w-i-min").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Laplacian-weight lower percentile")
        .cli("χ-sep iLSQR: percentile of the field's Laplacian at which the LSQR weight is 1 (default: 60)"),
        d("separation.chi_sep_ilsqr.prc_w_i_max", Separation, "Laplacian weight max %", "Percentile of the field's Laplacian at which the LSQR weight is 0", Float).min(0.0).max(100.0).flag("--chi-sep-ilsqr-prc-w-i-max").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Laplacian-weight upper percentile")
        .cli("χ-sep iLSQR: percentile of the field's Laplacian at which the LSQR weight is 0 (default: 99.8)"),
        d("separation.chi_sep_ilsqr.prc_w_g_min", Separation, "Gradient weight min %", "Percentile of each gradient's magnitude at which the streaking-removal weight is 1", Float).min(0.0).max(100.0).flag("--chi-sep-ilsqr-prc-w-g-min").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Gradient-weight lower percentile")
        .cli("χ-sep iLSQR: percentile of each gradient's magnitude at which the streaking-removal weight is 1 (default: 50)"),
        d("separation.chi_sep_ilsqr.prc_w_g_max", Separation, "Gradient weight max %", "Percentile of each gradient's magnitude at which the streaking-removal weight is 0", Float).min(0.0).max(100.0).flag("--chi-sep-ilsqr-prc-w-g-max").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr")).full("Gradient-weight upper percentile")
        .cli("χ-sep iLSQR: percentile of each gradient's magnitude at which the streaking-removal weight is 0 (default: 70)"),
        // The QSM χ-sep iLSQR is initialised with, reconstructed unless one is supplied. QSM.rs
        // inversion/ilsqr.rs: 0 iterations give an all-zero map; IlsqrParams::validate: pad >= 0.
        d("separation.chi_sep_ilsqr.qsm_ilsqr.max_iter", Separation, "QSM max iter", "Maximum iterations of the iLSQR that reconstructs the QSM separated", Int).min(1.0).flag("--chi-sep-ilsqr-qsm-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (iLSQR)").full("QSM maximum iterations")
        .cli("χ-sep iLSQR: max iterations of the iLSQR that reconstructs, from the local field, the QSM χ-separation is initialised with (default: 100; not used with --use-custom-qsm)"),
        d("separation.chi_sep_ilsqr.qsm_ilsqr.tol", Separation, "QSM tolerance", "Tolerance of the iLSQR that reconstructs the QSM separated", Float).flag("--chi-sep-ilsqr-qsm-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (iLSQR)")
        .cli("χ-sep iLSQR: tolerance of that QSM's iLSQR"),
        d("separation.chi_sep_ilsqr.qsm_ilsqr.pad_mm", Separation, "QSM pad (mm)", "Zero-padding per side of the iLSQR that reconstructs the QSM separated", Float).min(0.0).units("mm").flag("--chi-sep-ilsqr-qsm-pad-mm").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (iLSQR)").full("QSM padding")
        .cli("χ-sep iLSQR: zero-padding of that QSM's iLSQR, in mm per side (default: 12)")
        .value_name("MM"),
        d("separation.chi_sep_ilsqr.qsm_ilsqr.precision", Separation, "QSM precision", "", Enum(vec![
            o("double", "double", "Double", "Double precision (default)"),
            o("single", "single", "Single", "Single precision, as the original implementation computes the solves; slower and uses more memory"),
        ])).flag("--chi-sep-ilsqr-qsm-precision").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-ilsqr") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (iLSQR)")
        .gui("Arithmetic of the two least-squares solves of the iLSQR that reconstructs the QSM separated")
        .cli("χ-sep iLSQR: arithmetic of that QSM's two LSQR solves: double, or single as the original implementation (default: double)")
        .parse(P::Named).value_name("PRECISION"),
        // QSM.rs ChiSepMediParams::validate: λ, λ_CSF and the tolerances finite and >= 0; D_r finite
        // and > 0; at least 1 iteration.
        d("separation.chi_sep_medi.lambda", Separation, "Lambda", "Weight of the data terms against the L1 terms", Float).min(0.0).flag("--chi-sep-medi-lambda").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi"))
        .cli("χ-sep MEDI weight of the data terms against the L1 terms (default: 1)"),
        d("separation.chi_sep_medi.lambda_csf", Separation, "Lambda CSF", "Weight of the CSF-uniformity term", Float).min(0.0).flag("--chi-sep-medi-lambda-csf").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("CSF lambda")
        .cli("χ-sep MEDI weight of the CSF-uniformity term (default: 1)"),
        d("separation.chi_sep_medi.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T", Float).above(0.0).optional().units("Hz/ppm").flag("--chi-sep-medi-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Paramagnetic constant (Dr+)")
        .cli("χ-sep MEDI paramagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto)")
        .parse(P::NumberOrAuto).value_name("HZ_PER_PPM"),
        d("separation.chi_sep_medi.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T", Float).above(0.0).optional().units("Hz/ppm").flag("--chi-sep-medi-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Diamagnetic constant (Dr−)")
        .cli("χ-sep MEDI diamagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto)")
        .parse(P::NumberOrAuto).value_name("HZ_PER_PPM"),
        d("separation.chi_sep_medi.pad", Separation, "Pad (voxels)", "Zero-padding at both ends of the third axis, voxels", Int).units("voxels").flag("--chi-sep-medi-pad").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Padding")
        .gui("Zero-padding at both ends of the third axis")
        .cli("χ-sep MEDI zero-padding at both ends of the third axis, in voxels (default: 5)"),
        d("separation.chi_sep_medi.tol", Separation, "Tolerance", "Stop when the relative change falls below this", Float).min(0.0).flag("--chi-sep-medi-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi"))
        .cli("χ-sep MEDI: stop when the relative change falls below this (default: 0.03)"),
        d("separation.chi_sep_medi.max_iter", Separation, "Max iter", "Maximum Gauss-Newton iterations", Int).min(1.0).flag("--chi-sep-medi-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("Maximum iterations")
        .cli("χ-sep MEDI maximum Gauss-Newton iterations (default: 30)"),
        d("separation.chi_sep_medi.cg_tol", Separation, "CG tolerance", "Inner CG tolerance", Float).min(0.0).flag("--chi-sep-medi-cg-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi"))
        .cli("χ-sep MEDI inner CG tolerance (default: 0.05)"),
        d("separation.chi_sep_medi.cg_max_iter", Separation, "CG max iter", "Inner CG max iterations", Int).flag("--chi-sep-medi-cg-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).full("CG maximum iterations")
        .cli("χ-sep MEDI inner CG max iterations (default: 100)"),
        // The QSM χ-sep MEDI is initialised with, reconstructed unless one is supplied: the same
        // iLSQR the χ-separation toolbox makes for both of its iterative methods.
        d("separation.chi_sep_medi.qsm_ilsqr.max_iter", Separation, "QSM max iter", "Maximum iterations of the iLSQR that reconstructs the QSM χ-separation starts from", Int).min(1.0).flag("--chi-sep-medi-qsm-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (MEDI)").full("QSM maximum iterations")
        .cli("χ-sep MEDI: max iterations of the iLSQR that reconstructs, from the local field, the QSM χ-separation is initialised with (default: 100; not used with --use-custom-qsm)"),
        d("separation.chi_sep_medi.qsm_ilsqr.tol", Separation, "QSM tolerance", "Tolerance of the iLSQR that reconstructs the QSM χ-separation starts from", Float).flag("--chi-sep-medi-qsm-tol").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (MEDI)")
        .cli("χ-sep MEDI: tolerance of that QSM's iLSQR"),
        d("separation.chi_sep_medi.qsm_ilsqr.pad_mm", Separation, "QSM pad (mm)", "Zero-padding per side of the iLSQR that reconstructs the QSM χ-separation starts from", Float).min(0.0).units("mm").flag("--chi-sep-medi-qsm-pad-mm").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (MEDI)").full("QSM padding")
        .cli("χ-sep MEDI: zero-padding of that QSM's iLSQR, in mm per side (default: 12)")
        .value_name("MM"),
        d("separation.chi_sep_medi.qsm_ilsqr.precision", Separation, "QSM precision", "", Enum(vec![
            o("double", "double", "Double", "Double precision (default)"),
            o("single", "single", "Single", "Single precision, as the original implementation computes the solves; slower and uses more memory"),
        ])).flag("--chi-sep-medi-qsm-precision").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi") && x.c.separation.custom_qsm_tool.is_none()).context("χ-separation (MEDI)")
        .gui("Arithmetic of the two least-squares solves of the iLSQR that reconstructs the QSM χ-separation starts from")
        .cli("χ-sep MEDI: arithmetic of that QSM's two LSQR solves: double, or single as the original implementation (default: double)")
        .parse(P::Named).value_name("PRECISION"),
        d("separation.chi_sep_medi.csf_r2star", Separation, "CSF R2*", "", Enum(vec![
            o("arlo", "arlo", "ARLO", "MEDI's ARLO (default)"),
            o("nnls", "nnls", "NNLS", "The χ-separation toolbox's NNLS fit (r2star_nnls), which its script runs"),
        ])).flag("--chi-sep-medi-csf-r2star").parent("separation.algorithm")
        .when(|x| x.sep("chi-sep-medi")).context("χ-separation (MEDI)").full("CSF mask's R2*")
        .gui("The R2* fit the CSF mask is extracted from")
        .cli("χ-sep MEDI: the R2* its CSF mask is extracted from: arlo (MEDI's ARLO; the default) or nnls (the χ-separation toolbox's r2star_nnls, which its script runs)")
        .parse(P::Named).value_name("METHOD"),
        d("separation.wavesep.dr_pos", Separation, "Dr+ (Hz/ppm)", "Paramagnetic relaxivity", Float).units("Hz/ppm").flag("--wavesep-dr-pos").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Paramagnetic constant (Dr+)")
        .cli("WaveSep paramagnetic relaxivity (Hz/ppm)"),
        d("separation.wavesep.dr_neg", Separation, "Dr- (Hz/ppm)", "Diamagnetic relaxivity", Float).units("Hz/ppm").flag("--wavesep-dr-neg").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Diamagnetic constant (Dr−)")
        .cli("WaveSep diamagnetic relaxivity (Hz/ppm)"),
        d("separation.wavesep.alpha", Separation, "Alpha", "Proximal-gradient step size", Float).flag("--wavesep-alpha").parent("separation.algorithm")
        .when(|x| x.sep("wavesep"))
        .cli("WaveSep proximal-gradient step size"),
        d("separation.wavesep.lambda", Separation, "Lambda", "Wavelet L1 soft-threshold weight", Float).flag("--wavesep-lambda").parent("separation.algorithm")
        .when(|x| x.sep("wavesep"))
        .cli("WaveSep wavelet L1 soft-threshold weight"),
        // QSM.rs utils/wavelet.rs: only the SUPPORTED_ORDERS (db1–db4) exist.
        d("separation.wavesep.wavelet_order", Separation, "Wavelet order", "Daubechies order", Int).min(*WAVELET_ORDERS.start() as f64).max(*WAVELET_ORDERS.end() as f64).flag("--wavesep-wavelet-order").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Wavelet order")
        .cli("WaveSep Daubechies wavelet order"),
        d("separation.wavesep.max_iter", Separation, "Max iter", "ISTA max iterations", Int).flag("--wavesep-max-iter").parent("separation.algorithm")
        .when(|x| x.sep("wavesep")).full("Maximum iterations")
        .cli("WaveSep ISTA max iterations"),
        d("separation.wavesep.tol", Separation, "Tolerance", "Relative-change stop tolerance", Float).flag("--wavesep-tol").parent("separation.algorithm")
        .when(|x| x.sep("wavesep"))
        .cli("WaveSep relative-change stop tolerance"),
        // QSM.rs separation/hc_chisep.rs: r2prime/dr_pos is inf at 0; below 0 the calibration window is empty.
        d("separation.hc_chisep.dr_pos_3t", Separation, "Dr+ @3T (Hz/ppm)", "Paramagnetic relaxivity at 3T", Float).above(0.0).units("Hz/ppm").flag("--hc-chisep-dr-pos-3t").parent("separation.algorithm")
        .when(|x| x.sep("hc-chisep")).full("Paramagnetic constant at 3 T (Dr+)")
        .cli("HC-ChiSep paramagnetic relaxivity at 3T (Hz/ppm)"),
        d("separation.hc_chisep.bin_hz", Separation, "R2' bin (Hz)", "R2' bin width for anchored grid search", Float).units("Hz").flag("--hc-chisep-bin-hz").parent("separation.algorithm")
        .when(|x| x.sep("hc-chisep")).full("R2' bin width")
        .gui("R2' bin width for the anchored grid search")
        .cli("HC-ChiSep R2' bin width for the anchored grid search (Hz)"),
        // QSM.rs ChiSepNetParams::validate: each side a positive multiple of chisepnet::POOL_MULTIPLE
        // (16). The texts' "16" is pinned to it by a test.
        d("separation.chi_sepnet.route", Separation, "Route", "", Enum(vec![
            o("toolbox", "toolbox", "Toolbox (default)", "The authors' route: the inputs in the network's training frame (B0 along z, 1 mm), the QSM channel from QSMnetME, R2' measured or predicted from R2* by R2PRIMEnet, the maps resampled back"),
            o("native", "native", "Network only", "The network alone on the acquired grid, given this run's QSM and R2', as a sliding window"),
        ])).basic().flag("--chi-sepnet-route").parent("separation.algorithm")
        .when(|x| x.sep("chi-sepnet")).full("χ-sepnet route")
        .gui("How χ-sepnet's inputs are prepared: the authors' route or the network alone")
        .cli("χ-sepnet's input route. `toolbox` (default), the authors': the inputs resampled to 1 mm with B0 along z, the susceptibility channel computed by QSMnetME, R2' measured or predicted from R2* by R2PRIMEnet, the maps resampled back. `native`: the network alone on the acquired grid, given this run's QSM and R2'")
        .value_help("toolbox", "The authors' route (training frame, QSMnetME susceptibility channel)").value_help("native", "The network alone on the acquired grid, given the QSM"),
        d("separation.chi_sepnet.patch", Separation, "Patch", "Sliding-window patch D H W in voxels, each a multiple of 16 (default 192 192 128)", IntVec3).min(qsm_core::separation::chisepnet::POOL_MULTIPLE as f64).units("voxels").flag("--chi-sepnet-patch").parent("separation.chi_sepnet.route")
        .when(|x| x.sep("chi-sepnet") && x.c.separation.chi_sepnet.route == crate::ChiSepNetRoute::Native).full("χ-sepnet patch")
        .gui("Sliding-window patch D H W, each a multiple of 16")
        .cli("χ-sepnet (`--chi-sepnet-route native`) sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors')")
        .value_names(&["D", "H", "W"]),
        d("separation.chi_sepnet.frame_patch", Separation, "Sliding window", "Run the networks on patches D H W of the 1 mm frame, each a multiple of 16, so memory follows the patch; empty runs the whole frame at once", IntVec3).min(qsm_core::separation::chisepnet::POOL_MULTIPLE as f64).units("voxels").optional().flag("--chi-sepnet-frame-patch").parent("separation.chi_sepnet.route")
        .when(|x| x.sep("chi-sepnet") && x.c.separation.chi_sepnet.route == crate::ChiSepNetRoute::Toolbox).full("χ-sepnet sliding window")
        .gui("Patch size for running the networks piece by piece, which needs less memory, each a multiple of 16; empty runs the whole frame at once")
        .cli("χ-sepnet (`--chi-sepnet-route toolbox`): run the networks as a sliding window of patches this size over the 1 mm frame, D H W, each a multiple of 16, so memory follows the patch. Omit to run the whole frame at once, as the authors do")
        .value_names(&["D", "H", "W"]),
        // QSM.rs SusepNetParams::validate: each side a positive multiple of susep_net::POOL_MULTIPLE
        // (8). The texts' "8" is pinned to it by a test.
        d("separation.susep_net.patch", Separation, "Sliding window", "Run the network on patches D H W in voxels, each a multiple of 8, so its memory follows the patch instead of the volume; empty runs the whole volume at once", IntVec3).min(qsm_core::separation::susep_net::POOL_MULTIPLE as f64).units("voxels").optional().flag("--susep-net-patch").parent("separation.algorithm")
        .when(|x| x.sep("susep-net")).full("SUSEP-Net sliding window")
        .gui("Patch size for running SUSEP-Net piece by piece, which needs less memory; empty runs the whole volume at once")
        .cli("Run SUSEP-Net as a sliding window of patches this size in voxels, D H W, each a multiple of 8, so its memory follows the patch, not the volume. Omit to run the whole volume at once")
        .value_names(&["D", "H", "W"]),
        d("separation.custom_qsm_tool", Separation, "Custom QSM tool", "Use a Chimap from <bids>/derivatives/<tool>/ instead of recomputing QSM (χ-separation, statistics, SMWI); blank = compute, * = any tool", Text).basic().optional().flag("--use-custom-qsm").also(&[SwiSmwi, Segmentation])
        .supplied_file(Item::SuppliedChi, "A susceptibility (χ) map to use instead of computing QSM, for χ-separation, the statistics and SMWI")
        .when(|x| x.reads_qsm).full("Custom QSM")
        .gui("Use a χ map (Chimap) from <bids>/derivatives/<tool>/ instead of computing QSM, for χ-separation, the statistics and SMWI: * for any tool, or a tool name; empty to compute it")
        .cli("Use a bring-your-own QSM (Chimap) from <bids>/derivatives/<TOOL>/ for chi-separation")
        .value_name("TOOL"),
    ]
}
