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
            o("resharp", "resharp", "RESHARP", "Regularized SHARP (RESHARP) with Tikhonov — https://doi.org/10.1002/mrm.25032"),
            o("harperella", "harperella", "HARPERELLA", "HARPERELLA integrated unwrap+BFR — https://doi.org/10.1002/nbm.3056"),
            o("iharperella", "iharperella", "iHARPERELLA", "Improved HARPERELLA (iHARPERELLA) — Li et al., Proc. ISMRM 2015, p.3313"),
            o("bfrnet", "bfrnet", "BFRnet", "BFRnet deep-learning background removal (weights auto-downloaded) — https://github.com/sunhongfu/BFRnet"),
            o("iqfm", "iqfm", "iQFM", "iQFM deep-learning joint unwrapping + background removal from phase (weights auto-downloaded) — https://doi.org/10.1016/j.neuroimage.2022.119410"),
        ])).basic().flag("--bf-algorithm").full("Background removal")
        .gui("How the background field, from sources outside the mask, is removed to leave the local field"),
        d("bg_removal.erode_voxels", Background, "Erode mask", "Erode the brain mask after field mapping, before background removal (0 = none)", Int)
        .min(0.0).units("voxels").flag("--bf-erode-voxels")
        .when(|x| !x.bf("iqfm"))
        .gui("Erode the brain mask before background removal; the field map itself is not masked again"),
        d("bg_removal.polish_mask", Background, "Polish mask", "After background removal, keep the mask's largest connected component with its holes filled, as STI Suite's QSM_GUI does", Bool)
        .flag("--bf-polish-mask")
        .when(|x| !x.bf("iqfm"))
        .gui("Keep the background-removal mask's largest connected component, holes filled, as STI Suite's PolishMask"),
        d("bg_removal.ukb_mask_trim", Background, "UKB mask trim", "Before and after background removal, trim the mask of voxels with unreliable phase, as UK Biobank's pipeline does", Bool)
        .flag("--bf-ukb-mask-trim")
        .when(|x| !x.bf("iqfm"))
        .gui("Trim the mask by the phase reliability of the first and last echo before and after background removal, as UK Biobank's pipeline does"),
        d("bg_removal.vsharp.deconvolve", Background, "Deconvolve", "Undo the SMV high-pass with a truncated-SVD deconvolution (default: off)", Bool).flag("--vsharp-deconvolve").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("Undo the SMV high-pass with a truncated-SVD deconvolution"),
        // QSM.rs bgremove/vsharp.rs: at <= 0 the DC bin (1 - S = 0) is inverted, giving inf/NaN.
        d("bg_removal.vsharp.threshold", Background, "Threshold", "Deconvolution (TSVD) threshold", Float).above(0.0).flag("--vsharp-threshold").parent("bg_removal.vsharp.deconvolve")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.deconvolve),
        d("bg_removal.vsharp.max_radius", Background, "Max radius", "Largest SMV kernel radius (mm)", Float).units("mm").flag("--vsharp-max-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")).full("Maximum radius")
        .gui("Largest SMV kernel radius"),
        // QSM.rs VsharpParams::validate: min_radius and radius_step must be > 0.
        d("bg_removal.vsharp.min_radius", Background, "Min radius", "Smallest SMV kernel radius (mm)", Float).above(0.0).units("mm").flag("--vsharp-min-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp")).full("Minimum radius")
        .gui("Smallest SMV kernel radius"),
        d("bg_removal.vsharp.radius_step", Background, "Radius step", "Step between SMV kernel radii (mm)", Float).above(0.0).units("mm").flag("--vsharp-radius-step").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("Step between SMV kernel radii, from the largest down to the smallest"),
        d("bg_removal.vsharp.grid", Background, "Working grid", "", Enum(vec![
            o("native", "native", "Native", "The acquired voxel grid"),
            o("finest", "finest", "Finest", "An isotropic grid at the smallest voxel dimension, so no axis is coarsened; isotropic data stays on the acquired grid"),
            o("isotropic", "isotropic", "Isotropic", "An isotropic grid of Grid spacing mm; the background is interpolated back, the local field keeps native resolution (default)"),
        ])).flag("--vsharp-grid").parent("bg_removal.algorithm")
        .when(|x| x.bf("vsharp"))
        .gui("The voxel grid V-SHARP runs on"),
        // QSM.rs pipeline/bg_removal.rs refuses an isotropic spacing that is not > 0.
        d("bg_removal.vsharp.grid_spacing", Background, "Grid spacing", "Isotropic working-grid spacing (mm)", Float).above(0.0).units("mm").flag("--vsharp-grid-spacing").parent("bg_removal.vsharp.grid")
        .when(|x| x.bf("vsharp") && x.c.bg_removal.vsharp.grid == VsharpGrid::Isotropic)
        .gui("Isotropic working-grid spacing"),
        d("bg_removal.pdf.tol", Background, "Tolerance", "Convergence tolerance, relative to the noise level", Float).flag("--pdf-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")),
        d("bg_removal.pdf.n_cg", Background, "CG iter", "Maximum conjugate-gradient iterations", Int).min(1.0).flag("--pdf-n-cg").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")).full("CG iterations"),
        d("bg_removal.pdf.space", Background, "Kernel", "", Enum(vec![
            o("image", "image", "Image space", "The FFT of the dipole field sampled on the voxel grid (default)"),
            o("kspace", "kspace", "k-space", "The analytic k-space dipole kernel"),
        ])).flag("--pdf-space").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf"))
        .gui("The dipole kernel the background is fitted with"),
        // QSM.rs bgremove/pdf.rs refuses 1 (PdfConfig::validate).
        d("bg_removal.pdf.n_pad", Background, "Pad (voxels)", "Zero-padding around the mask's bounding box, voxels; 0 = the full volume", Int).min(0.0).units("voxels").flag("--pdf-n-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("pdf")).full("Padding")
        .gui("Zero-padding around the mask's bounding box; 0 runs on the full volume"),
        // QSM.rs LbvParams::validate: tol finite and >= 0, depth at most 8.
        d("bg_removal.lbv.tol", Background, "Tolerance", "Stop the coarsest grid's relaxation when its summed absolute update falls below this (radians over the echo spacing)", Float).min(0.0).flag("--lbv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv"))
        .gui("Stop the coarsest grid's relaxation when its summed absolute update falls below this"),
        d("bg_removal.lbv.depth", Background, "Depth", "Coarsest multigrid level. Blank = automatic (log2 of the smallest dimension minus 3)", Int).min(0.0).max(8.0).optional().flag("--lbv-depth").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")).full("Multigrid depth"),
        d("bg_removal.lbv.peel", Background, "Peel", "Boundary layers removed from the mask beyond the first", Int).min(0.0).units("voxels").flag("--lbv-peel").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")).full("Peeled layers"),
        d("bg_removal.lbv.n1", Background, "Relaxations before", "Relaxations on each grid before the coarser grid", Int).min(0.0).flag("--lbv-n1").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")),
        d("bg_removal.lbv.n2", Background, "Relaxations after", "Relaxations on each grid after the coarser grid", Int).min(0.0).flag("--lbv-n2").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")),
        d("bg_removal.lbv.n3", Background, "Final relaxations", "Relaxations on the finest grid at the end", Int).min(0.0).flag("--lbv-n3").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv")),
        d("bg_removal.lbv.precision", Background, "Precision", "", Enum(vec![
            o("double", "double", "Double", "Double precision (default)"),
            o("single", "single", "Single", "Single precision, as the original implementation computes"),
        ])).flag("--lbv-precision").parent("bg_removal.algorithm")
        .when(|x| x.bf("lbv"))
        .gui("Arithmetic of the multigrid solver"),
        d("bg_removal.ismv.variant", Background, "Variant", "", Enum(vec![
            o("wen2014", "wen2014", "Wen 2014", "The authors' paper and code (default)"),
            o("qsmjl", "qsmjl", "QSM.jl", "QSM.jl's implementation"),
        ])).flag("--ismv-variant").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv"))
        .gui("Which implementation of iSMV runs"),
        // QSM.rs IsmvParams::validate: finite and > 0.
        d("bg_removal.ismv.tol", Background, "Tolerance", "Stop when the relative change between iterates falls below this (QSM.jl: the residual relative to the first)", Float).above(0.0).flag("--ismv-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")),
        // QSM.rs bgremove/ismv.rs: 0 iterations give an all-zero local field.
        d("bg_removal.ismv.max_iter", Background, "Max iter", "Maximum iterations. Blank = no limit", Int).min(1.0).optional().flag("--ismv-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv")).full("Maximum iterations"),
        // QSM.rs IsmvParams::validate: finite and > 0.
        d("bg_removal.ismv.radius", Background, "Radius", "SMV kernel radius (mm). Blank = 6 × the smallest voxel dimension (QSM.jl: 2 × the largest)", Float).above(0.0).optional().units("mm").flag("--ismv-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("ismv"))
        .gui("SMV kernel radius. Blank = 6 × the smallest voxel dimension (QSM.jl: 2 × the largest)"),
        // QSM.rs bgremove/sharp.rs: at <= 0 the DC bin is divided by ~0, giving inf/NaN.
        d("bg_removal.sharp.threshold", Background, "Threshold", "Deconvolution threshold", Float).above(0.0).flag("--sharp-threshold").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp")),
        d("bg_removal.sharp.radius", Background, "Radius", "SMV kernel radius (mm)", Float).units("mm").flag("--sharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("sharp"))
        .gui("SMV kernel radius"),
        d("bg_removal.resharp.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--resharp-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp"))
        .gui("SMV kernel radius"),
        d("bg_removal.resharp.tik_reg", Background, "Tik reg", "Tikhonov regularization parameter", Float).flag("--resharp-tik-reg").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")).full("Tikhonov regularisation")
        .gui("Tikhonov regularisation parameter"),
        d("bg_removal.resharp.tol", Background, "Tolerance", "CGS relative tolerance", Float).flag("--resharp-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")),
        // QSM.rs solvers/cgs.rs: 0 iterations return x = 0, an all-zero local field.
        d("bg_removal.resharp.max_iter", Background, "Max iter", "Maximum CGS iterations", Int).min(1.0).flag("--resharp-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("resharp")).full("Maximum iterations"),
        d("bg_removal.harperella.radius", Background, "Radius", "SMV kernel radius in mm", Float).units("mm").flag("--harperella-radius").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella"))
        .gui("SMV kernel radius"),
        d("bg_removal.harperella.max_iter", Background, "Max iter", "Maximum iterations", Int).flag("--harperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")).full("Maximum iterations"),
        d("bg_removal.harperella.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--harperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")),
        d("bg_removal.harperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--harperella-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory"),
        d("bg_removal.harperella.rim", Background, "Boundary rim", "", Enum(vec![
            o("whole", "whole", "Whole", "Zero every Laplacian value in the brain's boundary rim (default)"),
            o("sti", "sti", "Largest 20 % (STI)", "Zero the 20 % largest Laplacian values in the rim, as STI Suite does"),
        ])).flag("--harperella-rim").parent("bg_removal.algorithm")
        .when(|x| x.bf("harperella"))
        .gui("Which Laplacian values in the brain's boundary rim are set to zero"),
        d("bg_removal.iharperella.max_iter", Background, "Max iter", "Maximum iterations", Int).flag("--iharperella-max-iter").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")).full("Maximum iterations"),
        d("bg_removal.iharperella.tol", Background, "Tolerance", "Convergence tolerance", Float).flag("--iharperella-tol").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")),
        d("bg_removal.iharperella.pad", Background, "Pad (voxels)", "Zero-padding per side, voxels (x y z). Larger costs time and memory (default 0 0 0)", IntVec3).units("voxels").flag("--iharperella-pad").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella")).full("Padding")
        .gui("Zero-padding per side (x y z). Larger costs time and memory"),
        d("bg_removal.iharperella.rim", Background, "Boundary rim", "", Enum(vec![
            o("whole", "whole", "Whole", "Zero every Laplacian value in the brain's boundary rim"),
            o("sti", "sti", "Largest 20 % (STI)", "Zero the 20 % largest Laplacian values in the rim, as STI Suite does (default)"),
        ])).flag("--iharperella-rim").parent("bg_removal.algorithm")
        .when(|x| x.bf("iharperella"))
        .gui("Which Laplacian values in the brain's boundary rim are set to zero"),
    ]
}
