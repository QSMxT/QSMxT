//! The Inversion section's MEDI settings.

use super::*;

pub(super) fn medi() -> Vec<Def> {
    vec![
        d("inversion.medi.smv", Inversion, "SMV mode", "High-pass the local field with a spherical mean value (SMV) filter, removing residual background near the brain edge (MEDI-SMV)", Bool).basic().flag("--no-medi-smv").parent("inversion.algorithm")
        .when(|x| x.inv("medi"))
        .cli("MEDI: no SMV high-pass of the local field")
        .conflicts(&["medi_smv"]).negated("--medi-smv", "MEDI: high-pass the local field with a spherical mean value filter, removing residual background near the brain edge (MEDI-SMV; on by default)"),
        d("inversion.medi.smv_radius", Inversion, "SMV radius", "SMV radius in mm (also mSMV's)", Float).units("mm").flag("--medi-smv-radius").parent("inversion.algorithm")
        .when(|x| x.inv("medi"))
        .gui("SMV radius (also mSMV's)")
        .cli("MEDI SMV radius in mm (also mSMV's)"),
        d("inversion.medi.smv_shrink_mask", Inversion, "Shrink mask", "With SMV, shrink the mask to where the SMV of the mask exceeds 0.999, dropping the edge band whose SMV reaches outside it. Off keeps the full mask", Bool).flag("--no-medi-smv-shrink-mask").parent("inversion.medi.smv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.smv && !x.c.inversion.medi.msmv).full("SMV mask shrinking")
        .cli("MEDI-SMV: keep the full mask, high-passing the field over it")
        .conflicts(&["medi_smv_shrink_mask"]).negated("--medi-smv-shrink-mask", "MEDI-SMV: shrink the mask to where the SMV of the mask exceeds 0.999 (on by default)"),
        d("inversion.medi.msmv", Inversion, "mSMV", "Filter the local field by maximum spherical mean value (MEDI-mSMV, Roberts 2024) before MEDI; needs at least 3 echoes", Bool).basic().flag("--medi-msmv").parent("inversion.algorithm")
        .when(|x| x.inv("medi")).full("MEDI-mSMV")
        .gui("Filter the local field by maximum spherical mean value (mSMV) before MEDI, which removes residual background at the brain edge; needs at least three echoes")
        .cli("MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value before MEDI, which fits it with the SMV-filtered dipole kernel (it implies the SMV). Needs at least 3 echoes"),
        // QSM.rs inversion/msmv.rs: the boundary correction makes at most maxk − 1 passes.
        d("inversion.medi.msmv_maxk", Inversion, "mSMV max K", "mSMV's boundary correction makes at most this many passes, less one", Int).min(1.0).flag("--medi-msmv-maxk").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv).full("mSMV maximum k")
        .cli("MEDI-mSMV: the boundary correction makes at most this many passes, less one")
        .requires(&["medi_msmv"]),
        d("inversion.medi.msmv_vessel_radius", Inversion, "mSMV vessel radius", "Largest vessel thickness of mSMV's vessel mask, in voxels. Blank = max(15, 8 × the largest voxel dimension)", Float).above(0.0).optional().units("voxels").flag("--medi-msmv-vessel-radius").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv)
        .gui("Largest vessel thickness of mSMV's vessel mask. Empty: 15 voxels, or 8 times the largest voxel dimension if that is more")
        .cli("MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 × the largest voxel dimension if that is more)")
        .requires(&["medi_msmv"]),
        d("inversion.medi.msmv_prefilter", Inversion, "mSMV prefilter", "SMV-filter the field before mSMV, as the authors do. Off diverges after PDF, V-SHARP or LBV (expert)", Bool).flag("--no-medi-msmv-prefilter").parent("inversion.medi.msmv")
        .when(|x| x.inv("medi") && x.c.inversion.medi.msmv)
        .cli("MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges (expert)")
        .requires(&["medi_msmv"]),
        d("inversion.medi.lambda", Inversion, "Lambda", "Data-fidelity weight (larger fits the field more closely)", Float).flag("--medi-lambda").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("Data-fidelity weight (larger fits the field more closely)")
        .cli("MEDI lambda: the data-fidelity weight (larger fits the field more closely)"),
        d("inversion.medi.percentage", Inversion, "Percentage", "Fraction of voxels considered edges (0.0-1.0)", Float).flag("--medi-percentage").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .cli("MEDI edge percentage (0.0-1.0)"),
        // QSM.rs inversion/medi.rs: 0 iterations give an all-zero map.
        d("inversion.medi.max_iter", Inversion, "Max iter", "Maximum outer iterations", Int).min(1.0).flag("--medi-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("Maximum iterations")
        .cli("MEDI max outer iterations"),
        // QSM.rs inversion/medi.rs: 0 CG iterations give an all-zero map.
        d("inversion.medi.cg_max_iter", Inversion, "CG max iter", "Maximum conjugate gradient iterations", Int).min(1.0).flag("--medi-cg-max-iter").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("CG maximum iterations")
        .cli("MEDI CG max iterations"),
        d("inversion.medi.cg_tol", Inversion, "CG tolerance", "CG convergence tolerance", Float).flag("--medi-cg-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .cli("MEDI CG tolerance"),
        d("inversion.medi.tol", Inversion, "Tolerance", "Outer convergence tolerance", Float).flag("--medi-tol").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .cli("MEDI outer tolerance"),
        d("inversion.medi.merit", Inversion, "MERIT", "Iteratively down-weight voxels whose residual stays large", Bool).flag("--medi-merit").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .cli("MEDI: enable MERIT weighting")
        .bool_value(),
        // QSM.rs inversion/medi.rs: the CSF term needs R2* from at least 3 echoes; with fewer, MEDI
        // runs without it (and the run says so, requirements::medi_csf_note).
        d("inversion.medi.csf", Inversion, "CSF reference", "Zero-reference χ to the ventricular CSF and keep it uniform there (MEDI+0, Liu 2018); needs at least 3 echoes, with fewer MEDI runs without it", Bool).basic().flag("--medi-csf").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi")).full("CSF zero reference (MEDI+0)")
        .gui("Zero-reference the susceptibility to the ventricular CSF, found from R2*, and keep it uniform there (MEDI+0); needs at least three echoes, with fewer MEDI runs without it")
        .cli("MEDI+0 (Liu et al. 2018): zero-reference χ to the ventricular CSF, found from R2*, and keep it uniform there (default: true). Needs at least 3 echoes; with fewer, MEDI runs without it")
        .bool_value(),
        d("inversion.medi.lambda_csf", Inversion, "Lambda CSF", "Weight of the CSF uniformity term", Float).min(0.0).flag("--medi-lambda-csf").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf).full("CSF weight")
        .cli("MEDI+0: weight of the CSF uniformity term (default: 100)"),
        d("inversion.medi.csf_r2s_threshold", Inversion, "CSF R2* threshold", "Voxels below this R2* are CSF candidates", Float).above(0.0).units("Hz").flag("--medi-csf-r2s-threshold").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf)
        .cli("MEDI+0: R2* in Hz below which a voxel is a CSF candidate (default: 5)"),
        d("inversion.medi.csf_erode", Inversion, "CSF erode", "Erode the mask by a 10 mm sphere before finding the CSF", Bool).flag("--medi-csf-erode").parent("inversion.medi.csf")
        .when(|x| (x.inv("medi") || x.qsmart_inner("medi")) && x.c.inversion.medi.csf).full("Erode before finding CSF")
        .cli("MEDI+0: erode the mask by a 10 mm sphere before finding the CSF (default: true)")
        .bool_value(),
        d("inversion.medi.zeropad", Inversion, "Zero padding", "Voxels of zeros appended at the end of x, y and z before the inversion; the result is cropped back (default 0 0 0)", IntVec3).units("voxels").flag("--medi-zeropad").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("Voxels of zeros appended at the end of each axis before the inversion; the result is cropped back")
        .cli("MEDI: voxels of zeros appended at the end of x, y and z before the inversion; the result is cropped back (default: 0 0 0)"),
        d("inversion.medi.data_weighting", Inversion, "Data weighting", "", Enum(vec![
            o(0, "uniform", "Uniform", "Every voxel of the field weighs the same in the data term"),
            o(1, "magnitude SNR", "Magnitude SNR", "Weight the data term by the magnitude SNR, so noisy voxels count for less"),
        ])).flag("--medi-data-weighting").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("How the voxels of the field are weighted in the data term")
        .cli("MEDI: data weighting mode (0=uniform, 1=SNR)"),
        d("inversion.medi.te", Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag("--medi-te").parent("inversion.algorithm")
        .when(|x| x.inv("medi") || x.qsmart_inner("medi"))
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms")
        .cli("MEDI: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing)"),
    ]
}
