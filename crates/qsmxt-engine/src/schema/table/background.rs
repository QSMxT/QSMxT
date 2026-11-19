//! The Background section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
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
        .cli("Background field removal algorithm (default: vsharp)")
        .value_help("bfrnet", "BFRnet deep-learning background removal (weights downloaded on first use)").value_help("iqfm", "iQFM deep-learning joint unwrapping + background removal from phase (weights downloaded on first use)"),
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
        d("bg_removal.ukb_mask_trim", Background, "UKB mask trim", "Before and after background removal, trim the mask of voxels with unreliable phase, as UK Biobank's pipeline does", Bool)
        .flag("--bf-ukb-mask-trim")
        .when(|x| !x.bf("iqfm"))
        .gui("Trim the mask by the phase reliability of the first and last echo before and after background removal, as UK Biobank's pipeline does")
        .cli("Trim the brain mask of voxels with unreliable phase before and after background removal, as UK Biobank's pipeline does (its phase-reliability maps of the first and last echo)")
        .negated("--no-bf-ukb-mask-trim", "Do not trim the mask by phase reliability (the default), after a `--pipeline` that does").negated_conflicts(),
        d("bg_removal.vsharp.deconvolve", Background, "Deconvolve", "Undo the SMV high-pass with a truncated-SVD deconvolution (default: off)", Bool).flag("--vsharp-deconvolve").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("Undo the SMV high-pass with a truncated-SVD deconvolution")
        .cli("Undo V-SHARP's SMV high-pass with a truncated-SVD deconvolution (default: false)")
        .bool_value().value_name("BOOL"),
        // QSM.rs bgremove/vsharp.rs: at <= 0 the DC bin (1 - S = 0) is inverted, giving inf/NaN.
        d("bg_removal.vsharp.threshold", Background, "Threshold", "Deconvolution (TSVD) threshold", Float).above(0.0).flag("--vsharp-threshold").parent("bg_removal.vsharp.deconvolve")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.deconvolve)
        .cli("V-SHARP deconvolution (TSVD) threshold; only used with --vsharp-deconvolve true"),
        d("bg_removal.vsharp.max_radius", Background, "Max radius", "Largest SMV kernel radius (mm)", Float).units("mm").flag("--vsharp-max-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")).full("Maximum radius")
        .gui("Largest SMV kernel radius")
        .cli("V-SHARP max kernel radius in mm"),
        // QSM.rs VsharpParams::validate: min_radius and radius_step must be > 0.
        d("bg_removal.vsharp.min_radius", Background, "Min radius", "Smallest SMV kernel radius (mm)", Float).above(0.0).units("mm").flag("--vsharp-min-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")).full("Minimum radius")
        .gui("Smallest SMV kernel radius")
        .cli("V-SHARP min kernel radius in mm"),
        d("bg_removal.vsharp.radius_step", Background, "Radius step", "Step between SMV kernel radii (mm)", Float).above(0.0).units("mm").flag("--vsharp-radius-step").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("Step between SMV kernel radii, from the largest down to the smallest")
        .cli("V-SHARP step between kernel radii in mm (default: 1); the radii run from --vsharp-max-radius down to --vsharp-min-radius")
        .value_name("MM"),
        d("bg_removal.vsharp.grid", Background, "Working grid", "", Enum(vec![
            o("native", "native", "Native", "The acquired voxel grid"),
            o("finest", "finest", "Finest", "An isotropic grid at the smallest voxel dimension, so no axis is coarsened; isotropic data stays on the acquired grid"),
            o("isotropic", "isotropic", "Isotropic", "An isotropic grid of Grid spacing mm; the background is interpolated back, the local field keeps native resolution (default)"),
        ])).flag("--vsharp-grid").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("The voxel grid V-SHARP runs on")
        .cli("Grid V-SHARP estimates the background on: `isotropic` (default; spacing from --vsharp-grid-spacing), `native` (the acquired grid), or `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened). The local field keeps native resolution"),
        // QSM.rs pipeline/bg_removal.rs refuses an isotropic spacing that is not > 0.
        d("bg_removal.vsharp.grid_spacing", Background, "Grid spacing", "Isotropic working-grid spacing (mm)", Float).above(0.0).units("mm").flag("--vsharp-grid-spacing").parent("bg_removal.vsharp.grid")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.grid == VsharpGrid::Isotropic)
        .gui("Isotropic working-grid spacing")
        .cli("V-SHARP isotropic working-grid spacing in mm (default 1); used with --vsharp-grid isotropic")
        .value_name("MM"),
        d("bg_removal.pdf.tol", Background, "Tolerance", "Convergence tolerance, relative to the noise level", Float).flag("--pdf-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf"))
        .cli("PDF tolerance, relative to the noise level"),
        d("bg_removal.pdf.n_cg", Background, "CG iter", "Maximum conjugate-gradient iterations", Int).min(1.0).flag("--pdf-n-cg").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")).full("CG iterations")
        .cli("PDF maximum conjugate-gradient iterations (default: 30)"),
        d("bg_removal.pdf.space", Background, "Kernel", "", Enum(vec![
            o("image", "image", "Image space", "The FFT of the dipole field sampled on the voxel grid (default)"),
            o("kspace", "kspace", "k-space", "The analytic k-space dipole kernel"),
        ])).flag("--pdf-space").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf"))
        .gui("The dipole kernel the background is fitted with")
        .cli("PDF dipole kernel: image (the FFT of the dipole sampled on the voxel grid) or kspace (the analytic k-space kernel) (default: image)")
        .parse(P::Named).value_name("SPACE"),
        // QSM.rs bgremove/pdf.rs refuses 1 (PdfConfig::validate).
        d("bg_removal.pdf.n_pad", Background, "Pad (voxels)", "Zero-padding around the mask's bounding box, voxels; 0 = the full volume", Int).min(0.0).units("voxels").flag("--pdf-n-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")).full("Padding")
        .gui("Zero-padding around the mask's bounding box; 0 runs on the full volume")
        .cli("PDF zero-padding around the mask's bounding box, in voxels; 0 runs on the full volume (default: 40)")
        .value_name("VOXELS"),
        // QSM.rs LbvParams::validate: tol finite and >= 0, depth at most 8.
        d("bg_removal.lbv.tol", Background, "Tolerance", "Stop the coarsest grid's relaxation when its summed absolute update falls below this (radians over the echo spacing)", Float).min(0.0).flag("--lbv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv"))
        .gui("Stop the coarsest grid's relaxation when its summed absolute update falls below this")
        .cli("LBV tolerance: stop the coarsest grid's relaxation when its summed absolute update falls below this, in radians over the echo spacing (default: 0.01)"),
        d("bg_removal.lbv.depth", Background, "Depth", "Coarsest multigrid level. Blank = automatic (log2 of the smallest dimension minus 3)", Int).min(0.0).max(8.0).optional().flag("--lbv-depth").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")).full("Multigrid depth")
        .cli("LBV coarsest multigrid level, at most 8 (default: log2 of the smallest dimension minus 3)")
        .value_name("LEVEL"),
        d("bg_removal.lbv.peel", Background, "Peel", "Boundary layers removed from the mask beyond the first", Int).min(0.0).units("voxels").flag("--lbv-peel").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")).full("Peeled layers")
        .cli("LBV boundary layers removed from the mask beyond the first (default: 0)")
        .value_name("LAYERS"),
        d("bg_removal.lbv.n1", Background, "Relaxations before", "Relaxations on each grid before the coarser grid", Int).min(0.0).flag("--lbv-n1").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv"))
        .cli("LBV relaxations on each grid before the coarser grid (default: 30)"),
        d("bg_removal.lbv.n2", Background, "Relaxations after", "Relaxations on each grid after the coarser grid", Int).min(0.0).flag("--lbv-n2").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv"))
        .cli("LBV relaxations on each grid after the coarser grid (default: 100)"),
        d("bg_removal.lbv.n3", Background, "Final relaxations", "Relaxations on the finest grid at the end", Int).min(0.0).flag("--lbv-n3").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv"))
        .cli("LBV relaxations on the finest grid at the end (default: 100)"),
        d("bg_removal.lbv.precision", Background, "Precision", "", Enum(vec![
            o("double", "double", "Double", "Double precision (default)"),
            o("single", "single", "Single", "Single precision, as the original implementation computes"),
        ])).flag("--lbv-precision").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv"))
        .gui("Arithmetic of the multigrid solver")
        .cli("LBV arithmetic: double, or single as the original implementation (default: double)")
        .parse(P::Named).value_name("PRECISION"),
        d("bg_removal.ismv.variant", Background, "Variant", "", Enum(vec![
            o("wen2014", "wen2014", "Wen 2014", "The authors' paper and code (default)"),
            o("qsmjl", "qsmjl", "QSM.jl", "QSM.jl's implementation"),
        ])).flag("--ismv-variant").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv"))
        .gui("Which implementation of iSMV runs")
        .cli("iSMV implementation: wen2014 (the authors' paper and code) or qsmjl (QSM.jl's) (default: wen2014)")
        .parse(P::Named).value_name("VARIANT"),
        // QSM.rs IsmvParams::validate: finite and > 0.
        d("bg_removal.ismv.tol", Background, "Tolerance", "Stop when the relative change between iterates falls below this (QSM.jl: the residual relative to the first)", Float).above(0.0).flag("--ismv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv"))
        .cli("iSMV tolerance: the relative change between iterates it stops at (qsmjl: the residual relative to the first) (default: 5e-5)"),
        // QSM.rs bgremove/ismv.rs: 0 iterations give an all-zero local field.
        d("bg_removal.ismv.max_iter", Background, "Max iter", "Maximum iterations. Blank = no limit", Int).min(1.0).optional().flag("--ismv-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")).full("Maximum iterations")
        .cli("iSMV max iterations (default: no limit)"),
        // QSM.rs IsmvParams::validate: finite and > 0.
        d("bg_removal.ismv.radius", Background, "Radius", "SMV kernel radius (mm). Blank = 6 × the smallest voxel dimension (QSM.jl: 2 × the largest)", Float).above(0.0).optional().units("mm").flag("--ismv-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv"))
        .gui("SMV kernel radius. Blank = 6 × the smallest voxel dimension (QSM.jl: 2 × the largest)")
        .cli("iSMV kernel radius in mm, or auto for 6 × the smallest voxel dimension (qsmjl: 2 × the largest) (default: auto)")
        .parse(P::NumberOrAuto).value_name("MM"),
        // QSM.rs bgremove/sharp.rs: at <= 0 the DC bin is divided by ~0, giving inf/NaN.
        d("bg_removal.sharp.threshold", Background, "Threshold", "Deconvolution threshold", Float).above(0.0).flag("--sharp-threshold").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp"))
        .cli("SHARP threshold"),
        d("bg_removal.sharp.radius", Background, "Radius", "SMV kernel radius (mm)", Float).units("mm").flag("--sharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp"))
        .gui("SMV kernel radius")
        .cli("SHARP kernel radius in mm"),
        d("bg_removal.resharp.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--resharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp"))
        .gui("SMV kernel radius")
        .cli("RESHARP SMV kernel radius in mm"),
        d("bg_removal.resharp.tik_reg", Background, "Tik reg", "Tikhonov regularization parameter", Float).flag("--resharp-tik-reg").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")).full("Tikhonov regularisation")
        .gui("Tikhonov regularisation parameter")
        .cli("RESHARP Tikhonov regularization parameter"),
        d("bg_removal.resharp.tol", Background, "Tolerance", "CGS relative tolerance", Float).flag("--resharp-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp"))
        .cli("RESHARP conjugate gradients squared (CGS) relative tolerance"),
        // QSM.rs solvers/cgs.rs: 0 iterations return x = 0, an all-zero local field.
        d("bg_removal.resharp.max_iter", Background, "Max iter", "Maximum CGS iterations", Int).min(1.0).flag("--resharp-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")).full("Maximum iterations")
        .cli("RESHARP maximum CGS iterations"),
        d("bg_removal.harperella.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--harperella-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella"))
        .gui("SMV kernel radius")
        .cli("HARPERELLA SMV kernel radius in mm"),
        d("bg_removal.harperella.max_iter", Background, "Max iter", "Maximum iterations", Int).flag("--harperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")).full("Maximum iterations")
        .cli("HARPERELLA maximum iterations"),
        d("bg_removal.harperella.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--harperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella"))
        .cli("HARPERELLA convergence tolerance"),
        d("bg_removal.harperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--harperella-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory")
        .cli("HARPERELLA zero-padding in voxels per side, X Y Z (default: 0 0 0)"),
        d("bg_removal.harperella.rim", Background, "Boundary rim", "", Enum(vec![
            o("whole", "whole", "Whole", "Zero every Laplacian value in the brain's boundary rim (default)"),
            o("sti", "sti", "Largest 20 % (STI)", "Zero the 20 % largest Laplacian values in the rim, as STI Suite does"),
        ])).flag("--harperella-rim").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella"))
        .gui("Which Laplacian values in the brain's boundary rim are set to zero")
        .cli("HARPERELLA boundary rim: whole (zero every Laplacian value in the rim; default) or sti (zero the 20 % largest, as STI Suite does)")
        .parse(P::Named).value_name("RIM"),
        d("bg_removal.iharperella.max_iter", Background, "Max iter", "Maximum iterations", Int).flag("--iharperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")).full("Maximum iterations")
        .cli("iHARPERELLA maximum iterations"),
        d("bg_removal.iharperella.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--iharperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella"))
        .cli("iHARPERELLA convergence tolerance"),
        d("bg_removal.iharperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--iharperella-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory")
        .cli("iHARPERELLA zero-padding in voxels per side, X Y Z (default: 0 0 0)"),
        d("bg_removal.iharperella.rim", Background, "Boundary rim", "", Enum(vec![
            o("whole", "whole", "Whole", "Zero every Laplacian value in the brain's boundary rim"),
            o("sti", "sti", "Largest 20 % (STI)", "Zero the 20 % largest Laplacian values in the rim, as STI Suite does (default)"),
        ])).flag("--iharperella-rim").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella"))
        .gui("Which Laplacian values in the brain's boundary rim are set to zero")
        .cli("iHARPERELLA boundary rim: whole (zero every Laplacian value in the rim) or sti (zero the 20 % largest, as STI Suite does; default)")
        .parse(P::Named).value_name("RIM"),
    ]
}
