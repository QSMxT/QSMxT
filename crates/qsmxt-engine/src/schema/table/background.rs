//! The Background section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    [
        algorithm(),
        mask(),
        vsharp(),
        pdf(),
        lbv(),
        ismv(),
        sharp(),
        resharp(),
        harperella(),
        iharperella(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn algorithm() -> Vec<Def> {
    vec![
        d("bg_removal.algorithm", Background, "BG removal", "", Enum(vec![
            o("vsharp", "vsharp", "V-SHARP", "Variable-kernel SHARP (V-SHARP) — https://doi.org/10.1002/mrm.23000"),
            o("pdf", "pdf", "PDF", "Projection onto Dipole Fields (PDF) — https://doi.org/10.1002/nbm.1670"),
            o("lbv", "lbv", "LBV", "Laplacian Boundary Value (LBV) — https://doi.org/10.1002/nbm.3064"),
            o("ismv", "ismv", "iSMV", "Iterative Spherical Mean Value (iSMV) — https://doi.org/10.1002/mrm.24998"),
            o("sharp", "sharp", "SHARP", "SHARP (Sophisticated Harmonic Artifact Reduction) — https://doi.org/10.1016/j.neuroimage.2010.10.070"),
            o("resharp", "resharp", "RESHARP", "Regularized SHARP (RESHARP) with Tikhonov — https://doi.org/10.1002/mrm.24765"),
            o("harperella", "harperella", "HARPERELLA", "HARPERELLA integrated unwrap+BFR — https://doi.org/10.1002/nbm.3056"),
            o("iharperella", "iharperella", "iHARPERELLA", "Improved HARPERELLA (iHARPERELLA) — Li et al., Proc. ISMRM 2015, p.3313"),
            o("bfrnet", "bfrnet", "BFRnet", "BFRnet deep-learning background removal (weights auto-downloaded) — https://github.com/sunhongfu/BFRnet"),
            o("iqfm", "iqfm", "iQFM", "iQFM deep-learning joint unwrapping + background removal from phase (weights auto-downloaded) — https://doi.org/10.1016/j.neuroimage.2022.119410"),
        ])).basic().flag("--bf-algorithm").full("Background removal")
        .gui("How the background field, from sources outside the mask, is removed to leave the local field")
        .gui_option("iharperella", "Improved HARPERELLA (iHARPERELLA)")
        // The docs note below their table that the weights are downloaded on first use.
        .gui_option("bfrnet", "BFRnet deep-learning background removal")
        .gui_option("iqfm", "iQFM deep-learning joint unwrapping + background removal from phase")
        .cli("Background field removal algorithm")
        .value_help("bfrnet", "BFRnet deep-learning background removal (weights downloaded on first use)").value_help("iqfm", "iQFM deep-learning joint unwrapping + background removal from phase (weights downloaded on first use)"),
    ]
}

fn mask() -> Vec<Def> {
    vec![
        d("bg_removal.erode_voxels", Background, "Erode mask", "Erode the brain mask after field mapping, before background removal (0 = none)", Int)
        .min(0.0).units("voxels").flag("--bf-erode-voxels")
        .when(|x| !x.bf("iqfm"))
        .gui("Erode the brain mask before background removal; the field map itself is not masked again")
        .cli("Erode the brain mask by N voxels after field mapping, before background removal; the field map is not masked again (default: 0, none)")
        .value_name("N"),
        d("bg_removal.polish_mask", Background, "Polish mask", "After background removal, keep the mask's largest connected component with its holes filled, as STI Suite's QSM_GUI does", Bool)
        .flag("--bf-polish-mask")
        .when(|x| !x.bf("iqfm"))
        .gui("Keep the background-removal mask's largest connected component, holes filled, as STI Suite's PolishMask")
        .cli("After background removal, keep the mask's largest connected component with its holes filled and zero the local field outside it, as STI Suite's QSM_GUI does (PolishMask)")
        .negated("--no-bf-polish-mask", "Do not polish the background-removal mask (the default), after a `--pipeline` that does").negated_conflicts(),
        d("bg_removal.ukb_mask_trim", Background, "UK Biobank mask trim", "Before and after background removal, trim the mask of voxels with unreliable phase, as UK Biobank's pipeline does", Bool)
        .flag("--bf-ukb-mask-trim")
        .when(|x| !x.bf("iqfm"))
        .gui("Trim the mask by the phase reliability of the first and last echo before and after background removal, as UK Biobank's pipeline does")
        .cli("Trim the brain mask of voxels with unreliable phase before and after background removal, as UK Biobank's pipeline does (its phase-reliability maps of the first and last echo)")
        .negated("--no-bf-ukb-mask-trim", "Do not trim the mask by phase reliability (the default), after a `--pipeline` that does").negated_conflicts(),
    ]
}

fn vsharp() -> Vec<Def> {
    vec![
        d("bg_removal.vsharp.deconvolve", Background, "Deconvolve", "Undo the SMV high-pass with a truncated-SVD deconvolution (default: off)", Bool).flag("--vsharp-deconvolve")
        .of(Of::Background)
        .gui("Undo the SMV high-pass with a truncated-SVD deconvolution")
        .cli("Undo V-SHARP's SMV high-pass with a truncated-SVD deconvolution")
        .bool_value().value_name("BOOL"),
        // QSM.rs bgremove/vsharp.rs: at <= 0 the DC bin (1 - S = 0) is inverted, giving inf/NaN.
        d("bg_removal.vsharp.threshold", Background, "Threshold", "Deconvolution (TSVD) threshold", Float).above(0.0).flag("--vsharp-threshold").parent("bg_removal.vsharp.deconvolve")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.deconvolve)
        .cli("V-SHARP deconvolution (TSVD) threshold; only used with --vsharp-deconvolve true"),
        d("bg_removal.vsharp.max_radius", Background, "Max radius", "Largest SMV kernel radius (mm)", Float).units("mm").flag("--vsharp-max-radius")
        .of(Of::Background).full("Maximum radius")
        .gui("Largest SMV kernel radius"),
        // QSM.rs VsharpParams::validate: min_radius and radius_step must be > 0.
        d("bg_removal.vsharp.min_radius", Background, "Min radius", "Smallest SMV kernel radius (mm)", Float).above(0.0).units("mm").flag("--vsharp-min-radius")
        .of(Of::Background).full("Minimum radius")
        .gui("Smallest SMV kernel radius"),
        d("bg_removal.vsharp.radius_step", Background, "Radius step", "Step between SMV kernel radii (mm)", Float).above(0.0).units("mm").flag("--vsharp-radius-step")
        .of(Of::Background)
        .gui("Step between SMV kernel radii, from the largest down to the smallest")
        .cli("V-SHARP step between kernel radii in mm (default: 1); the radii run from --vsharp-max-radius down to --vsharp-min-radius")
        .value_name("MM"),
        d("bg_removal.vsharp.grid", Background, "Working grid", "", Enum(vec![
            o("native", "native", "Native", "The acquired voxel grid"),
            o("finest", "finest", "Finest", "An isotropic grid at the smallest voxel dimension, so no axis is coarsened; isotropic data stays on the acquired grid"),
            o("isotropic", "isotropic", "Isotropic", "An isotropic grid of Grid spacing mm; the background is interpolated back, the local field keeps native resolution (default)"),
        ])).flag("--vsharp-grid")
        .of(Of::Background)
        .gui("The voxel grid V-SHARP runs on")
        .cli("Grid V-SHARP estimates the background on: `isotropic` (default; spacing from --vsharp-grid-spacing), `native` (the acquired grid), or `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened). The local field keeps native resolution"),
        // QSM.rs pipeline/bg_removal.rs refuses an isotropic spacing that is not > 0.
        d("bg_removal.vsharp.grid_spacing", Background, "Grid spacing", "Isotropic working-grid spacing (mm)", Float).above(0.0).units("mm").flag("--vsharp-grid-spacing").parent("bg_removal.vsharp.grid")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.grid == VsharpGrid::Isotropic)
        .gui("Isotropic working-grid spacing")
        .cli("V-SHARP isotropic working-grid spacing in mm (default 1); used with --vsharp-grid isotropic")
        .value_name("MM"),
    ]
}

fn pdf() -> Vec<Def> {
    vec![
        d("bg_removal.pdf.tol", Background, "Tolerance", "Convergence tolerance, relative to the noise level", Float).flag("--pdf-tol")
        .of(Of::Background)
        .cli("PDF tolerance, relative to the noise level"),
        d("bg_removal.pdf.n_cg", Background, "CG iter", "Maximum conjugate-gradient iterations", Int).min(1.0).flag("--pdf-n-cg")
        .of(Of::Background).full("CG iterations"),
        d("bg_removal.pdf.space", Background, "Kernel", "", Enum(vec![
            o("image", "image", "Image space", "The FFT of the dipole field sampled on the voxel grid (default)"),
            o("kspace", "kspace", "k-space", "The analytic k-space dipole kernel"),
        ])).flag("--pdf-space")
        .of(Of::Background)
        .gui("The dipole kernel the background is fitted with")
        .cli("PDF dipole kernel: image (the FFT of the dipole sampled on the voxel grid) or kspace (the analytic k-space kernel)")
        .parse(P::Named).value_name("SPACE"),
        // QSM.rs bgremove/pdf.rs refuses 1 (PdfConfig::validate).
        d("bg_removal.pdf.n_pad", Background, "Pad (voxels)", "Zero-padding around the mask's bounding box, voxels; 0 = the full volume", Int).min(0.0).units("voxels").flag("--pdf-n-pad")
        .of(Of::Background).full("Padding")
        .gui("Zero-padding around the mask's bounding box; 0 runs on the full volume")
        .cli("PDF zero-padding around the mask's bounding box, in voxels; 0 runs on the full volume")
        .value_name("VOXELS"),
    ]
}

fn lbv() -> Vec<Def> {
    vec![
        // QSM.rs LbvParams::validate: tol finite and >= 0, depth at most 8.
        d("bg_removal.lbv.tol", Background, "Tolerance", "Stop the coarsest grid's relaxation when its summed absolute update falls below this (radians over the echo spacing)", Float).min(0.0).flag("--lbv-tol")
        .of(Of::Background)
        .gui("Stop the coarsest grid's relaxation when its summed absolute update falls below this")
        .cli("LBV tolerance: stop the coarsest grid's relaxation when its summed absolute update falls below this, in radians over the echo spacing"),
        d("bg_removal.lbv.depth", Background, "Depth", "Coarsest multigrid level. Blank = automatic (log2 of the smallest dimension minus 3)", Int).min(0.0).max(8.0).optional().flag("--lbv-depth")
        .of(Of::Background).full("Multigrid depth")
        .cli("LBV coarsest multigrid level, at most 8 (default: log2 of the smallest dimension minus 3)")
        .value_name("LEVEL"),
        d("bg_removal.lbv.peel", Background, "Peel", "Boundary layers removed from the mask beyond the first", Int).min(0.0).units("voxels").flag("--lbv-peel")
        .of(Of::Background).full("Peeled layers")
        .value_name("LAYERS"),
        d("bg_removal.lbv.n1", Background, "Relaxations before", "Relaxations on each grid before the coarser grid", Int).min(0.0).flag("--lbv-n1")
        .of(Of::Background),
        d("bg_removal.lbv.n2", Background, "Relaxations after", "Relaxations on each grid after the coarser grid", Int).min(0.0).flag("--lbv-n2")
        .of(Of::Background),
        d("bg_removal.lbv.n3", Background, "Final relaxations", "Relaxations on the finest grid at the end", Int).min(0.0).flag("--lbv-n3")
        .of(Of::Background),
        d("bg_removal.lbv.precision", Background, "Precision", "", Enum(vec![
            o("double", "double", "Double", "Double precision (default)"),
            o("single", "single", "Single", "Single precision, as the original implementation computes"),
        ])).flag("--lbv-precision")
        .of(Of::Background)
        .gui("Arithmetic of the multigrid solver")
        .cli("LBV arithmetic: double, or single as the original implementation")
        .parse(P::Named).value_name("PRECISION"),
    ]
}

fn ismv() -> Vec<Def> {
    vec![
        d("bg_removal.ismv.variant", Background, "Variant", "", Enum(vec![
            o("wen2014", "wen2014", "Wen 2014", "The authors' paper and code (default)"),
            o("qsmjl", "qsmjl", "QSM.jl", "QSM.jl's implementation"),
        ])).flag("--ismv-variant")
        .of(Of::Background)
        .gui("Which implementation of iSMV runs")
        .cli("iSMV implementation: wen2014 (the authors' paper and code) or qsmjl (QSM.jl's)")
        .parse(P::Named).value_name("VARIANT"),
        // QSM.rs IsmvParams::validate: finite and > 0.
        d("bg_removal.ismv.tol", Background, "Tolerance", "Stop when the relative change between iterates falls below this (QSM.jl: the residual relative to the first)", Float).above(0.0).flag("--ismv-tol")
        .of(Of::Background)
        .cli("iSMV tolerance: the relative change between iterates it stops at (qsmjl: the residual relative to the first)"),
        // QSM.rs bgremove/ismv.rs: 0 iterations give an all-zero local field.
        d("bg_removal.ismv.max_iter", Background, "Max iter", "Maximum iterations. Blank = no limit", Int).min(1.0).optional().flag("--ismv-max-iter")
        .of(Of::Background).full("Maximum iterations")
        .cli("iSMV max iterations (default: no limit)"),
        // QSM.rs IsmvParams::validate: finite and > 0.
        d("bg_removal.ismv.radius", Background, "Radius", "SMV kernel radius (mm). Blank = 6 × the smallest voxel dimension (QSM.jl: 2 × the largest)", Float).above(0.0).optional().units("mm").flag("--ismv-radius")
        .of(Of::Background)
        .gui("SMV kernel radius. Blank = 6 × the smallest voxel dimension (QSM.jl: 2 × the largest)")
        .cli("iSMV kernel radius in mm, or auto for 6 × the smallest voxel dimension (qsmjl: 2 × the largest) (default: auto)")
        .parse(P::NumberOrAuto).value_name("MM"),
    ]
}

fn sharp() -> Vec<Def> {
    vec![
        // QSM.rs bgremove/sharp.rs: at <= 0 the DC bin is divided by ~0, giving inf/NaN.
        d("bg_removal.sharp.threshold", Background, "Threshold", "Deconvolution threshold", Float).above(0.0).flag("--sharp-threshold")
        .of(Of::Background),
        d("bg_removal.sharp.radius", Background, "Radius", "SMV kernel radius (mm)", Float).units("mm").flag("--sharp-radius")
        .of(Of::Background)
        .gui("SMV kernel radius"),
    ]
}

fn resharp() -> Vec<Def> {
    vec![
        d("bg_removal.resharp.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--resharp-radius")
        .of(Of::Background)
        .gui("SMV kernel radius"),
        d("bg_removal.resharp.tik_reg", Background, "Tik reg", "Tikhonov regularization parameter", Float).flag("--resharp-tik-reg")
        .of(Of::Background).full("Tikhonov regularisation")
        .gui("Tikhonov regularisation parameter"),
        d("bg_removal.resharp.tol", Background, "Tolerance", "CGS relative tolerance", Float).flag("--resharp-tol")
        .of(Of::Background)
        .cli("RESHARP conjugate gradients squared (CGS) relative tolerance"),
        // QSM.rs solvers/cgs.rs: 0 iterations return x = 0, an all-zero local field.
        max_iter("bg_removal.resharp.max_iter", Background, "Maximum CGS iterations", "--resharp-max-iter").min(1.0)
        .of(Of::Background),
    ]
}

fn harperella() -> Vec<Def> {
    vec![
        d("bg_removal.harperella.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--harperella-radius")
        .of(Of::Background)
        .gui("SMV kernel radius"),
        max_iter("bg_removal.harperella.max_iter", Background, "Maximum iterations", "--harperella-max-iter")
        .of(Of::Background),
        tol("bg_removal.harperella.tol", Background, "--harperella-tol")
        .of(Of::Background),
        d("bg_removal.harperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--harperella-pad")
        .of(Of::Background).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory")
        .cli("HARPERELLA zero-padding in voxels per side, X Y Z"),
        d("bg_removal.harperella.rim", Background, "Boundary rim", "", Enum(vec![
            o("whole", "whole", "Whole", "Zero every Laplacian value in the brain's boundary rim (default)"),
            o("sti", "sti", "Largest 20 % (STI)", "Zero the 20 % largest Laplacian values in the rim, as STI Suite does"),
        ])).flag("--harperella-rim")
        .of(Of::Background)
        .gui("Which Laplacian values in the brain's boundary rim are set to zero")
        .cli("HARPERELLA boundary rim: whole (zero every Laplacian value in the rim; default) or sti (zero the 20 % largest, as STI Suite does)")
        .parse(P::Named).value_name("RIM"),
    ]
}

fn iharperella() -> Vec<Def> {
    vec![
        max_iter("bg_removal.iharperella.max_iter", Background, "Maximum iterations", "--iharperella-max-iter")
        .of(Of::Background),
        tol("bg_removal.iharperella.tol", Background, "--iharperella-tol")
        .of(Of::Background),
        d("bg_removal.iharperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--iharperella-pad")
        .of(Of::Background).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory")
        .cli("iHARPERELLA zero-padding in voxels per side, X Y Z"),
        d("bg_removal.iharperella.rim", Background, "Boundary rim", "", Enum(vec![
            o("whole", "whole", "Whole", "Zero every Laplacian value in the brain's boundary rim"),
            o("sti", "sti", "Largest 20 % (STI)", "Zero the 20 % largest Laplacian values in the rim, as STI Suite does (default)"),
        ])).flag("--iharperella-rim")
        .of(Of::Background)
        .gui("Which Laplacian values in the brain's boundary rim are set to zero")
        .cli("iHARPERELLA boundary rim: whole (zero every Laplacian value in the rim) or sti (zero the 20 % largest, as STI Suite does; default)")
        .parse(P::Named).value_name("RIM"),
    ]
}
