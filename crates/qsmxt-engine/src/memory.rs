//! Memory estimation for pipeline runs.
//!
//! Provides static estimation of peak memory usage per pipeline run based on
//! volume dimensions, number of echoes, and algorithm configuration. Used by
//! the executor to limit concurrent runs and avoid out-of-memory conditions.
//! Byte counts are `u64`, not `usize`: a run's peak can exceed 4 GB, which a 32-bit
//! (WebAssembly) `usize` cannot hold. Querying the host's memory is the front end's job.
//!
//! All estimates are derived from analysis of actual buffer allocations in
//! qsm_core algorithms. Memory usage is deterministic and scales linearly
//! with N = nx * ny * nz — no algorithm's memory grows with iteration count.

use crate::config::*;
use crate::enums::*;
use crate::masking::*;

// Element sizes in bytes
const F64: u64 = 8;
const F32: u64 = 4;
const U8: u64 = 1;

/// HD-BET's peak for the network itself, in bytes, with a sliding window of `patch` (`None`: one
/// pass over the whole volume): tract activations for one network input (~1 KiB per input
/// voxel) and the parsed model (~300 MB). A sliding window's input is its patch: ~3.6 GB peak
/// with the native 192×192×96, ~1.5 GB with 128×128×64. The whole volume's is the head's
/// bounding box at 1 mm, padded to multiples of 32×32×16, which does not depend on the acquired
/// resolution: 6.4–8.2 GB measured on 1 mm whole-head and 0.8×0.8×3 mm GRE (up to 160×256×256
/// voxels), so budget a 176×256×256 box. The scan's own working volumes come on top.
pub fn hd_bet_network_bytes(patch: Option<[usize; 3]>) -> u64 {
    const FULL_VOLUME_VOXELS: u64 = 256 * 256 * 176;
    let input_voxels = patch.map_or(FULL_VOLUME_VOXELS, |p| p.iter().map(|&d| d as u64).product());
    input_voxels * 1024 + 300 * 1024 * 1024
}

/// SynthSeg's peak in bytes on a `dims` volume (taken to be about 1 mm, the resolution SynthSeg
/// resamples to), with a sliding window of `patch` (`None`: the whole volume at once). The
/// network takes about 1 KB per voxel it sees at once, and the posteriors (one float per label
/// per voxel, blended and then cleaned up) about 200 B per padded voxel. Measured on a 184×256×128
/// scan with flip averaging: 6.8 GB on the whole volume, 3.25 GB with 128³ windows, 2.0 GB with
/// 96³.
pub fn synthseg_bytes(dims: [usize; 3], patch: Option<usize>) -> u64 {
    // qsm-core pads each axis to a multiple of 32, at least 128.
    let padded: [u64; 3] = std::array::from_fn(|a| (dims[a].div_ceil(32) * 32).max(128) as u64);
    let n_pad: u64 = padded.iter().product();
    let window: u64 = match patch {
        None => n_pad,
        Some(p) => {
            let p = (p.div_ceil(32) * 32) as u64;
            padded.iter().map(|&d| d.min(p)).product()
        }
    };
    window * 1000 + n_pad * 200
}

/// The peak of a χ-separation network on top of the maps it reads, in bytes, on a `dims`
/// volume: tract's activations for one patch, the parsed model, and the network's own padded
/// input channels and overlap accumulators. χ-sepnet runs its `patch`; SUSEP-Net runs its
/// `patch`, or with none the whole volume (padded to multiples of 8) in one pass. Other methods:
/// 0. Measured natively, single-threaded, on a 184×256×128 volume (peak RSS, the maps in and out
/// included): χ-sepnet 4.5 GB at its authors' 192×192×128 patch and 1.7 GB at 128×128×64;
/// SUSEP-Net 12.5 GB on the whole volume and 3.1 GB at 128×128×64; on 136×136×72, SUSEP-Net
/// 2.8 GB at 128×128×64 and 2.0 GB at 96×96×64.
pub fn chi_separation_network_bytes(dims: [usize; 3], config: &SeparationConfig) -> u64 {
    // (activation bytes per patch voxel, parsed model and fixed overhead, pooling multiple)
    let (patch, per_voxel, fixed, multiple) = match config.algorithm {
        SeparationAlgorithm::ChiSepNet => (Some(config.chi_sepnet.patch), CHI_SEPNET_PER_VOXEL, CHI_SEPNET_FIXED, 16),
        SeparationAlgorithm::SusepNet => (config.susep_net.patch, SUSEP_NET_PER_VOXEL, SUSEP_NET_FIXED, 8),
        _ => return 0,
    };
    let round = |d: usize| d.div_ceil(multiple) * multiple;
    // The padded volume: each axis at least the patch (and, whole-volume, a multiple of 8).
    let padded: [u64; 3] = std::array::from_fn(|a| match patch {
        Some(p) => dims[a].max(p[a]) as u64,
        None => round(dims[a]) as u64,
    });
    let window: u64 = match patch {
        Some(p) => p.iter().map(|&d| d as u64).product(),
        None => padded.iter().product(),
    };
    // Three f32 input channels and three f64 accumulators over the padded volume.
    window * per_voxel + fixed + 36 * padded.iter().product::<u64>()
}

const CHI_SEPNET_PER_VOXEL: u64 = 760;
const CHI_SEPNET_FIXED: u64 = 350_000_000;
const SUSEP_NET_PER_VOXEL: u64 = 1_850;
const SUSEP_NET_FIXED: u64 = 800_000_000;

/// Estimate peak memory usage in bytes for a single pipeline run.
///
/// The estimate accounts for:
/// - Loaded NIfTI data (phases + magnitudes) that persists across the pipeline
/// - Per-stage temporary allocations (masking, unwrapping, bg removal, inversion)
/// - SWI outputs if enabled
/// - A 1.3x safety margin for FFT scratch, allocator overhead, and fragmentation
///
/// # Arguments
/// * `nx`, `ny`, `nz` — Volume dimensions
/// * `n_echoes` — Number of echo times
/// * `n_coils` — Uncombined receive-coil channels (1 for combined data); MCPC-3D-S loads every
///   coil's phase and magnitude at once before the rest of the pipeline runs
/// * `has_magnitude` — Whether magnitude files are available
/// * `config` — Pipeline configuration (determines which algorithms are used)
pub fn estimate_peak_memory_bytes(
    nx: usize,
    ny: usize,
    nz: usize,
    n_echoes: usize,
    n_coils: usize,
    has_magnitude: bool,
    config: &PipelineConfig,
) -> u64 {
    let n = nx as u64 * ny as u64 * nz as u64;
    let (n_echoes, n_coils) = (n_echoes as u64, n_coils as u64);
    let n_mag = if has_magnitude { n_echoes } else { 0 };

    // MCPC-3D-S coil combination: all coils' phase + magnitude (f64), the HIP (2 volumes),
    // the unwrapped HIP, one offset volume, and the per-echo complex accumulators.
    let coil_combine_mem = if n_coils > 1 {
        (2 * n_coils * n_echoes + 4 + 2 * n_echoes) * n * F64
    } else {
        0
    };

    // Baseline: data that persists across the entire pipeline
    let baseline = n_echoes * n * F64  // phases (Vec<f64> per echo)
                 + n_mag * n * F64     // magnitudes (Vec<f64> per echo)
                 + 2 * n * U8;        // mask + working_mask

    // Masking stage (temporary) — worst case across all sections
    // BET is the most expensive: sorted nonzero vec (~N*8) + within_brain values (~N*8) + output (N)
    let has_bet = config.masking.sections.iter().any(|s| matches!(s.generator, MaskOp::Bet { .. }));
    // HD-BET: the network ([`hd_bet_network_bytes`]) and its f32 working volumes (~40 B/voxel).
    let hd_bet_mem = config.masking.sections.iter()
        .filter_map(|s| match &s.generator {
            MaskOp::HdBet { patch, .. } => Some(hd_bet_network_bytes(*patch)),
            _ => None,
        })
        .max()
        .map_or(0, |network| network + 40 * n);
    // MindGrab runs on the image conformed to 256³ at 1 mm whatever the acquisition, so its peak
    // does not depend on the volume: 7.3 GB measured on 256×288×48 GRE, with or without TTA (the
    // flipped pass runs after the first).
    const MINDGRAB_MEM: u64 = 7_500_000_000;
    let mindgrab_mem = if config.masking.sections.iter().any(|s| matches!(s.generator, MaskOp::MindGrab { .. })) {
        MINDGRAB_MEM
    } else {
        0
    };
    // Signal-gated erosion keeps a handful of f64 volumes (smoothed signal, bias estimate, depth).
    let has_signal_erode = config.masking.sections.iter()
        .any(|s| s.refinements.iter().any(|op| matches!(op, MaskOp::SignalErode { .. })));
    // SynthSeg runs on the combined magnitude after masking.
    let segmentation_mem = if config.pipeline.do_segmentation || config.pipeline.do_analysis {
        synthseg_bytes([nx, ny, nz], config.segmentation.patch)
    } else {
        0
    };
    let mask_mem = (if has_bet { 17 * n } else { 9 * n }).max(hd_bet_mem).max(mindgrab_mem)
        + if has_signal_erode { 8 * n * F64 } else { 0 };

    // SWI runs before QSM; its outputs persist through QSM stages
    let swi_persistent = if config.pipeline.do_swi { 16 * n } else { 0 }; // swi + mip results
    // CLEAR-SWI loads every echo's phase and magnitude and keeps each echo's filtered phase;
    // ROMEO adds its edge weights. Measured: 2 echoes 256×288×48 0.31 GB (Laplacian) / 0.76 GB
    // (ROMEO); 8 echoes 260×320×160 2.8 GB / 4.5 GB.
    let swi_temp = if config.pipeline.do_swi {
        let per_voxel = match config.swi.unwrap {
            crate::SwiUnwrap::Romeo => 4 * n_echoes + 20,
            _ => 3 * n_echoes + 8,
        };
        per_voxel * n * F64
    } else {
        0
    };

    let qsm_stage_mem = if config.inversion.algorithm == QsmAlgorithm::Tgv {
        // TGV path: single-step (no separate unwrap/bgremove)
        // TGV workspace: 35 Vec<f32> fields + additional sub-volumes
        // Worst case: bounding box = full volume (actual is often 30-70% of N)
        // phase_f32 input: 4N, TGV workspace: 154*N (f32), mask temps: 6N, output: 12N
        20 * n + 154 * n * F32 / F64 + 10 * 1024 * 1024 // ~97N + 10MB stencil
    } else if config.inversion.algorithm == QsmAlgorithm::Qsmart {
        // QSMART: ~2× standard pipeline (two-stage SDF + iLSQR) + vasculature buffers
        // Vasculature detection: Frangi vesselness + sphere filtering (~80N)
        // Two SDF passes (~80N each) + two iLSQR passes (~160N each) + combine step
        // Peak is during iLSQR (biggest single-step allocation)
        let unwrap_mem = estimate_unwrap_mem(n, n_echoes, config);
        let qsmart_mem = 240 * n; // iLSQR workspace + SDF input/output + vasculature
        unwrap_mem.max(qsmart_mem)
    } else {
        // Standard path: stages run sequentially
        estimate_standard_pipeline(n, n_echoes, config)
    };

    // Chi-separation + R2/R2' run as sequential stages after QSM. Peak load: the QSM (8N),
    // R2*/R2/R2' maps (~24N), multi-echo GRE + MESE magnitude voxel-major (~2·n_echoes·8N),
    // plus the EPG R2 dictionary/solver or the run_separation workspace (~80N for the CG methods).
    // The networks have no CG workspace; their own peak comes on top of the maps.
    let sep_net_mem = if config.pipeline.do_chi_separation {
        chi_separation_network_bytes([nx, ny, nz], &config.separation)
    } else {
        0
    };
    let sep_mem = if config.pipeline.do_chi_separation
        || config.pipeline.do_r2map
        || config.pipeline.do_r2primemap
    {
        let workspace = if sep_net_mem > 0 { 40 } else { 120 };
        (2 * n_echoes * F64 + workspace) * n + sep_net_mem
    } else {
        0
    };

    // Overall peak: baseline + max across all stages
    // Stages are sequential: masking → SWI → QSM → (R2/R2' + chi-separation)
    let peak = baseline
        + [
            coil_combine_mem,
            mask_mem,
            segmentation_mem,
            swi_temp + swi_persistent,
            qsm_stage_mem + swi_persistent,
            sep_mem,
        ]
        .into_iter()
        .max()
        .unwrap_or(0);

    // Safety margin: 1.3x for FFT scratch space, allocator overhead, fragmentation
    peak * 13 / 10
}

/// Estimate peak temporary memory for the standard pipeline path.
///
/// Returns the maximum memory across the sequential stages:
/// unwrap/combination → background removal → dipole inversion.
fn estimate_standard_pipeline(n: u64, n_echoes: u64, config: &PipelineConfig) -> u64 {
    // Phase unwrapping / echo combination
    let unwrap_mem = estimate_unwrap_mem(n, n_echoes, config);

    // Background field removal
    // Input: field_ppm (8N) + bg_mask (N) carry forward from unwrap stage
    let bg_carry = 9 * n;
    let bg_temp = match config.bg_removal.algorithm {
        // V-SHARP: persistent field_fft (16N) + per-radius buffers (kernel/mask/filtered FFTs)
        // A 1 mm isotropic working grid about doubles it: the resampled copies plus V-SHARP on the
        // cropped working grid (UKB 0.8x0.8x3 mm: 0.37 -> 0.78 GB). The finest-voxel grid grows
        // with the anisotropy ratio (3.75x the voxels for UKB); budget for ratios up to ~4.
        BfAlgorithm::Vsharp => match config.bg_removal.vsharp.grid {
            crate::VsharpGrid::Native => 80 * n,
            crate::VsharpGrid::Isotropic => 160 * n,
            crate::VsharpGrid::Finest => 400 * n,
        },
        // PDF: dipole kernel + masks + LSMR state + FFT workspace
        BfAlgorithm::Pdf => 112 * n,
        // LBV: minimal — no FFT, Gauss-Seidel in-place. interior/boundary bools + copies
        BfAlgorithm::Lbv => 26 * n,
        // iSMV: kernel + kernel_fft + masks + field copies + per-iteration FFT
        BfAlgorithm::Ismv => 96 * n,
        // SHARP: similar to V-SHARP but single radius
        BfAlgorithm::Sharp => 60 * n,
        // RESHARP: SMV kernel + CG solver state
        BfAlgorithm::Resharp => 80 * n,
        // HARPERELLA: iterative Laplacian solver + SMV kernel
        BfAlgorithm::Harperella => 60 * n,
        // iHARPERELLA: similar to HARPERELLA
        BfAlgorithm::Iharperella => 60 * n,
        // BFRnet / iQFM: ONNX U-Net activations dominate; padded field + network working set
        BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => 120 * n,
    };

    // Dipole inversion
    // Input: local_field (8N) + eroded_mask (N) carry forward from bg stage
    let inv_carry = 9 * n;
    // With overlap-tiling on, a DL net's activations are bounded by one patch, not the whole
    // volume — use the padded patch voxel count for the tileable nets (never more than `n`).
    let dl_voxels = match config.inversion.tile_size {
        Some(core) => {
            let halo = config.inversion.tile_halo.unwrap_or(8);
            ((core + 2 * halo) as u64).saturating_pow(3).min(n)
        }
        None => n,
    };
    let inv_temp = match config.inversion.algorithm {
        // TKD: dipole kernel + inverse + complex field + output
        QsmAlgorithm::Tkd => 40 * n,
        // TSVD: same as TKD (closed-form k-space solution with different thresholding)
        QsmAlgorithm::Tsvd => 40 * n,
        // Tikhonov: similar to TKD (closed-form k-space solution)
        QsmAlgorithm::Tikhonov => 40 * n,
        // iLSQR: iterative LSQR with dipole kernel + workspace buffers
        QsmAlgorithm::Ilsqr => 160 * n,
        // LSQR (Schweser 2010): like iLSQR, but the unknown vector is ~2n — χ over the grid plus a
        // residual-field unknown per masked voxel — and LSQR keeps several vectors that size.
        QsmAlgorithm::Lsqr => 200 * n,
        // HEIDI: an LSQR solve for the seed, then NESTA/TV continuation on top of it.
        QsmAlgorithm::Heidi => 280 * n,
        // TV-ADMM: kernels + 9 ADMM buffers + FFT workspace + inv_a
        QsmAlgorithm::Tv => 120 * n,
        // NLTV: similar to TV + reweighting buffers
        QsmAlgorithm::Nltv => 140 * n,
        // RTS: TV buffers + field_fft + residual (LSMR step)
        QsmAlgorithm::Rts => 160 * n,
        // MEDI: magnitude + edge weights + CG buffers
        QsmAlgorithm::Medi => 180 * n,
        // TFI: preconditioned total-field CG, similar footprint to MEDI
        QsmAlgorithm::Tfi => 180 * n,
        QsmAlgorithm::Tgv => unreachable!("TGV handled separately"),
        QsmAlgorithm::Qsmart => unreachable!("QSMART handled separately"),
        // NDI: gradient-descent with dipole kernel + a few field buffers
        QsmAlgorithm::Ndi => 120 * n,
        // FANSI nlTV / nlTGV: ADMM with gradient + consistency buffers
        QsmAlgorithm::Fansi => 140 * n,
        QsmAlgorithm::FansiTgv => 140 * n,
        // L1-QSM: ADMM with extra L1 proximal buffer
        QsmAlgorithm::L1qsm => 140 * n,
        // WH-QSM: ADMM with additional harmonic-field buffers
        QsmAlgorithm::Whqsm => 140 * n,
        // HD-QSM: two-stage L1->L2, closed-form + ADMM buffers
        QsmAlgorithm::Hdqsm => 120 * n,
        // AMP-PE: GAMP over complex wavelet coefficients + padded FFT workspace
        QsmAlgorithm::AmpPe => 260 * n,
        // Deep-learning inversions: ONNX U-Net activations dominate. QSMnet/QSMnet+ are the
        // heaviest (deeper 3D U-Net, /16 padding); xQSM/AutoQSM are lighter patch/octave nets.
        // `dl_voxels` == `n` unless tiling is enabled (then it's one patch); autoqsm/qsmgan patch
        // natively (unaffected by the tiling flag) and iqsm/iqsm+ take a separate phase path.
        QsmAlgorithm::Qsmnet | QsmAlgorithm::QsmnetPlus => 200 * dl_voxels,
        QsmAlgorithm::Xqsm => 120 * dl_voxels,
        QsmAlgorithm::Autoqsm => 120 * n,
        // Other DL nets: unrolled/patch/single-step U-Nets. NeXtQSM (two U-Nets + unroll)
        // and iQSM/iQSM+ (per-echo passes) are the heaviest; the rest are moderate.
        QsmAlgorithm::Nextqsm => 220 * dl_voxels,
        QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => 200 * n,
        QsmAlgorithm::Qsmgan => 140 * n,
        QsmAlgorithm::Ir2qsm | QsmAlgorithm::Lpcnn | QsmAlgorithm::ModlQsm => 140 * dl_voxels,
    };

    // Peak is the maximum across the three sequential stages
    [
        unwrap_mem,
        bg_carry + bg_temp,
        inv_carry + inv_temp,
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

/// Estimate peak temporary memory for phase unwrapping / echo combination.
fn estimate_unwrap_mem(n: u64, n_echoes: u64, config: &PipelineConfig) -> u64 {
    let unwrap_per_echo = match config.field_mapping.unwrapping_algorithm {
        // Laplacian: d2u (8N) + d2u_masked (8N) + FFT complex (16N) + result (8N)
        UnwrappingAlgorithm::Laplacian => 40 * n,
        // ROMEO: weights (3N) + phase clone (8N) + mask clone (N)
        UnwrappingAlgorithm::Romeo => 12 * n,
    };

    if n_echoes > 1 && config.field_mapping.phase_offset_removal {
        // Phase offset removal: HIP (16N) + per-echo ROMEO (~12N, sequential) +
        // smoothing (32N) + corrected_phases (n_echoes*8N) + offsets (16N)
        (n_echoes * 8 + 56) * n
    } else if n_echoes > 1 {
        // Independent unwrap per echo, stored: n_echoes unwrapped volumes
        // + linear fit outputs (field + offset + residual = 24N)
        n_echoes * 8 * n + unwrap_per_echo + 32 * n
    } else {
        // Single echo: unwrapped (8N) + field_rads (8N) + field_ppm (8N)
        24 * n + unwrap_per_echo
    }
}

/// Format a byte count as a human-readable string (e.g. "3.4 GB").
pub fn format_bytes(bytes: u64) -> String {
    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 1.0 {
        format!("{:.1} GB", gb)
    } else {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{:.0} MB", mb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> PipelineConfig {
        PipelineConfig::default()
    }

    #[test]
    fn test_dl_algorithms_estimate_memory() {
        for a in [
            QsmAlgorithm::Xqsm, QsmAlgorithm::Qsmnet, QsmAlgorithm::QsmnetPlus,
            QsmAlgorithm::Autoqsm, QsmAlgorithm::Qsmgan, QsmAlgorithm::Ir2qsm,
            QsmAlgorithm::Lpcnn, QsmAlgorithm::ModlQsm, QsmAlgorithm::Nextqsm,
            QsmAlgorithm::Iqsm, QsmAlgorithm::IqsmPlus,
        ] {
            let mut c = default_config();
            c.inversion.algorithm = a;
            assert!(estimate_peak_memory_bytes(8, 8, 8, 1, 1, false, &c) > 0, "{:?}", a);
        }
        for b in [BfAlgorithm::Bfrnet, BfAlgorithm::Iqfm] {
            let mut c = default_config();
            c.bg_removal.algorithm = b;
            assert!(estimate_peak_memory_bytes(8, 8, 8, 1, 1, false, &c) > 0, "{:?}", b);
        }
    }

    /// The χ-separation networks count, the browser profile's patches shrink them, and a patch
    /// bounds SUSEP-Net whatever the volume.
    #[test]
    fn chi_separation_networks_count_and_a_patch_bounds_them() {
        let est = |c: &PipelineConfig| estimate_peak_memory_bytes(184, 256, 128, 5, 1, true, c);
        for alg in [SeparationAlgorithm::ChiSepNet, SeparationAlgorithm::SusepNet] {
            let mut c = default_config();
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
            let classical = est(&c);
            c.separation.algorithm = alg;
            let native = est(&c);
            assert!(native > classical, "{alg}: {native} vs {classical}");
            c.apply_browser_profile();
            assert!(est(&c) < native, "{alg}");
            // The network's own share at the browser patch, against the measured ~1.4 / ~1.9 GB.
            let net = chi_separation_network_bytes([184, 256, 128], &c.separation);
            assert!(net > 1_000_000_000 && net < 2_500_000_000, "{alg}: {net}");
        }
        let mut c = SeparationConfig { algorithm: SeparationAlgorithm::SusepNet, ..Default::default() };
        let whole = chi_separation_network_bytes([184, 256, 128], &c);
        assert!(whole > 10_000_000_000, "{whole}");
        c.susep_net.patch = Some([96, 96, 64]);
        let small = chi_separation_network_bytes([184, 256, 128], &c);
        let big = chi_separation_network_bytes([368, 512, 256], &c);
        assert!(big - small < 2_000_000_000, "{small} {big}");
        assert_eq!(chi_separation_network_bytes([184, 256, 128], &SeparationConfig::default()), 0);
    }

    #[test]
    fn synthseg_counts_and_a_window_bounds_it() {
        let mut c = default_config();
        let without = estimate_peak_memory_bytes(184, 256, 128, 5, 1, true, &c);
        c.pipeline.do_segmentation = true;
        let whole = estimate_peak_memory_bytes(184, 256, 128, 5, 1, true, &c);
        assert!(whole > 6_000_000_000 && whole > without, "{whole}");
        c.segmentation.patch = Some(128);
        let window = estimate_peak_memory_bytes(184, 256, 128, 5, 1, true, &c);
        assert!(window * 3 < whole * 2, "{window} vs {whole}");
        // A volume one window covers costs what the whole volume costs, near enough.
        assert!(synthseg_bytes([100, 100, 100], Some(128)) <= synthseg_bytes([100, 100, 100], None));
    }

    #[test]
    fn test_hd_bet_mask_memory() {
        use crate::{hd_bet_mask_sections, MaskOp};
        let mut c = default_config();
        let base = estimate_peak_memory_bytes(164, 205, 205, 4, 1, true, &c);
        c.masking.sections = hd_bet_mask_sections();
        let full = estimate_peak_memory_bytes(164, 205, 205, 4, 1, true, &c);
        c.masking.sections[0].generator =
            MaskOp::HdBet { patch: Some(MaskOp::hd_bet_default_patch()), tta: false, tile_step: hd_bet_default_tile_step() };
        let native = estimate_peak_memory_bytes(164, 205, 205, 4, 1, true, &c);
        c.masking.sections[0].generator =
            MaskOp::HdBet { patch: Some([128, 128, 64]), tta: false, tile_step: hd_bet_default_tile_step() };
        let low = estimate_peak_memory_bytes(164, 205, 205, 4, 1, true, &c);
        // HD-BET dominates the pipeline peak (measured for masking alone: ~6.4–8.2 GB on the
        // whole volume, ~3.6 GB with the native patch).
        assert!(full > 8_000_000_000 && full > native, "full {full}, native {native}");
        assert!(native > 3_500_000_000 && native > base, "native {native}, base {base}");
        assert!(low < native, "low-memory patch ({low}) should estimate below native ({native})");
    }

    #[test]
    fn test_mindgrab_mask_memory() {
        use crate::mindgrab_mask_sections;
        let mut c = default_config();
        let base = estimate_peak_memory_bytes(256, 288, 48, 2, 1, true, &c);
        c.masking.sections = mindgrab_mask_sections();
        let mg = estimate_peak_memory_bytes(256, 288, 48, 2, 1, true, &c);
        // Measured: 7.3 GB peak for the whole run on this grid, MindGrab being the peak.
        assert!(mg > 7_300_000_000 && mg > base, "mindgrab {mg}, base {base}");
    }

    #[test]
    fn test_more_echoes_more_memory() {
        let config = default_config();
        let est_1 = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        let est_4 = estimate_peak_memory_bytes(128, 128, 128, 4, 1, true, &config);
        assert!(est_4 > est_1, "4 echoes ({}) should use more memory than 1 ({})", est_4, est_1);
    }

    #[test]
    fn test_swi_adds_memory() {
        let mut config = default_config();
        config.pipeline.do_swi = false;
        let est_no_swi = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        config.pipeline.do_swi = true;
        let est_swi = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        assert!(est_swi >= est_no_swi, "SWI ({}) should use at least as much memory as no SWI ({})", est_swi, est_no_swi);
    }

    #[test]
    fn test_tkd_less_than_rts() {
        let mut config = default_config();
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        let est_tkd = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        config.inversion.algorithm = QsmAlgorithm::Rts;
        let est_rts = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        assert!(est_tkd < est_rts, "TKD ({}) should use less memory than RTS ({})", est_tkd, est_rts);
    }

    #[test]
    fn test_lbv_less_than_vsharp() {
        let mut config = default_config();
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        config.bg_removal.algorithm = BfAlgorithm::Lbv;
        let est_lbv = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        config.bg_removal.algorithm = BfAlgorithm::Vsharp;
        let est_vsharp = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        assert!(est_lbv < est_vsharp, "LBV ({}) should use less memory than V-SHARP ({})", est_lbv, est_vsharp);
    }

    #[test]
    fn test_reasonable_range() {
        // A 64^3 single-echo minimal pipeline should use less than 1 GB
        let mut config = default_config();
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        config.bg_removal.algorithm = BfAlgorithm::Lbv;
        config.pipeline.do_swi = false;
        let est = estimate_peak_memory_bytes(64, 64, 64, 1, 1, false, &config);
        let one_gb = 1024 * 1024 * 1024;
        assert!(est < one_gb, "64^3 minimal pipeline should use < 1 GB, got {}", format_bytes(est));
        assert!(est > 0, "Estimate should be positive");
    }

    #[test]
    fn test_uncombined_coils_raise_estimate() {
        let config = default_config();
        let one = estimate_peak_memory_bytes(256, 288, 48, 2, 1, true, &config);
        let many = estimate_peak_memory_bytes(256, 288, 48, 2, 32, true, &config);
        // 32 coils x 2 echoes x (phase + magnitude) f64 volumes dominate
        assert!(many > one + 32 * 2 * 2 * 256 * 288 * 48 * 8 / 2, "{} vs {}", many, one);
    }

    #[test]
    fn test_typical_brain_estimate() {
        // 256x256x176, 4 echoes, default GRE (ROMEO + V-SHARP + iLSQR)
        let config = default_config();
        let est = estimate_peak_memory_bytes(256, 256, 176, 4, 1, true, &config);
        let gb = est as f64 / (1024.0 * 1024.0 * 1024.0);
        // Should be in the range of 2-6 GB for a typical brain
        assert!(gb > 1.0, "Typical brain should use > 1 GB, got {:.1} GB", gb);
        assert!(gb < 10.0, "Typical brain should use < 10 GB, got {:.1} GB", gb);
    }

    #[test]
    fn test_tgv_path() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Tgv;
        config.field_mapping.phase_offset_removal = false;
        let est = estimate_peak_memory_bytes(128, 128, 128, 1, 1, true, &config);
        assert!(est > 0, "TGV estimate should be positive");
    }

    #[test]
    fn test_no_magnitude_less_memory() {
        let config = default_config();
        let est_mag = estimate_peak_memory_bytes(128, 128, 128, 4, 1, true, &config);
        let est_no_mag = estimate_peak_memory_bytes(128, 128, 128, 4, 1, false, &config);
        assert!(est_no_mag < est_mag, "No magnitude ({}) should use less memory than with magnitude ({})", est_no_mag, est_mag);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024), "3.5 GB");
        assert_eq!(format_bytes(500 * 1024 * 1024), "500 MB");
    }
}
