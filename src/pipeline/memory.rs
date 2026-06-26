//! Memory estimation for pipeline runs.
//!
//! Provides static estimation of peak memory usage per pipeline run based on
//! volume dimensions, number of echoes, and algorithm configuration. Used by
//! the executor to limit concurrent runs and avoid out-of-memory conditions.
//!
//! All estimates are derived from analysis of actual buffer allocations in
//! qsm_core algorithms. Memory usage is deterministic and scales linearly
//! with N = nx * ny * nz — no algorithm's memory grows with iteration count.

use super::config::*;

// Element sizes in bytes
const F64: usize = 8;
const F32: usize = 4;
const U8: usize = 1;

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
/// * `has_magnitude` — Whether magnitude files are available
/// * `config` — Pipeline configuration (determines which algorithms are used)
pub fn estimate_peak_memory_bytes(
    nx: usize,
    ny: usize,
    nz: usize,
    n_echoes: usize,
    has_magnitude: bool,
    config: &PipelineConfig,
) -> usize {
    let n = nx * ny * nz;
    let n_mag = if has_magnitude { n_echoes } else { 0 };

    // Baseline: data that persists across the entire pipeline
    let baseline = n_echoes * n * F64  // phases (Vec<f64> per echo)
                 + n_mag * n * F64     // magnitudes (Vec<f64> per echo)
                 + 2 * n * U8;        // mask + working_mask

    // Masking stage (temporary) — worst case across all sections
    // BET is the most expensive: sorted nonzero vec (~N*8) + within_brain values (~N*8) + output (N)
    let has_bet = config.masking.sections.iter().any(|s| matches!(s.generator, MaskOp::Bet { .. }));
    let mask_mem = if has_bet { 17 * n } else { 9 * n };

    // SWI runs before QSM; its outputs persist through QSM stages
    let swi_persistent = if config.pipeline.do_swi { 16 * n } else { 0 }; // swi + mip results
    let swi_temp = if config.pipeline.do_swi { 32 * n } else { 0 };       // highpass + mask + output

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

    // Overall peak: baseline + max across all stages
    // Stages are sequential: masking → SWI → QSM
    let peak = baseline
        + [
            mask_mem,
            swi_temp + swi_persistent,
            qsm_stage_mem + swi_persistent,
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
fn estimate_standard_pipeline(n: usize, n_echoes: usize, config: &PipelineConfig) -> usize {
    // Phase unwrapping / echo combination
    let unwrap_mem = estimate_unwrap_mem(n, n_echoes, config);

    // Background field removal
    // Input: field_ppm (8N) + bg_mask (N) carry forward from unwrap stage
    let bg_carry = 9 * n;
    let bg_temp = match config.bg_removal.algorithm {
        // V-SHARP: persistent field_fft (16N) + per-radius buffers (kernel/mask/filtered FFTs)
        BfAlgorithm::Vsharp => 80 * n,
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
        
    };

    // Dipole inversion
    // Input: local_field (8N) + eroded_mask (N) carry forward from bg stage
    let inv_carry = 9 * n;
    let inv_temp = match config.inversion.algorithm {
        // TKD: dipole kernel + inverse + complex field + output
        QsmAlgorithm::Tkd => 40 * n,
        // TSVD: same as TKD (closed-form k-space solution with different thresholding)
        QsmAlgorithm::Tsvd => 40 * n,
        // Tikhonov: similar to TKD (closed-form k-space solution)
        QsmAlgorithm::Tikhonov => 40 * n,
        // iLSQR: iterative LSQR with dipole kernel + workspace buffers
        QsmAlgorithm::Ilsqr => 160 * n,
        // TV-ADMM: kernels + 9 ADMM buffers + FFT workspace + inv_a
        QsmAlgorithm::Tv => 120 * n,
        // NLTV: similar to TV + reweighting buffers
        QsmAlgorithm::Nltv => 140 * n,
        // RTS: TV buffers + field_fft + residual (LSMR step)
        QsmAlgorithm::Rts => 160 * n,
        // MEDI: magnitude + edge weights + CG buffers
        QsmAlgorithm::Medi => 180 * n,
        QsmAlgorithm::Tgv => unreachable!("TGV handled separately"),
        QsmAlgorithm::Qsmart => unreachable!("QSMART handled separately"),
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
fn estimate_unwrap_mem(n: usize, n_echoes: usize, config: &PipelineConfig) -> usize {
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

/// Get available system memory in bytes.
///
/// On Linux, reads `MemAvailable` from `/proc/meminfo`.
/// Falls back to 8 GB if detection fails or on unsupported platforms.
pub fn available_memory_bytes() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
            for line in contents.lines() {
                if let Some(rest) = line.strip_prefix("MemAvailable:") {
                    if let Some(kb_str) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    // Fallback: 8 GB
    8 * 1024 * 1024 * 1024
}

/// Get current process resident memory (RSS) in bytes.
///
/// On Linux, reads `VmRSS` from `/proc/self/status`.
/// Returns 0 if detection fails or on unsupported platforms.
pub fn process_rss_bytes() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    if let Some(kb_str) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    0
}

/// Format a byte count as a human-readable string (e.g. "3.4 GB").
pub fn format_bytes(bytes: usize) -> String {
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
    use crate::pipeline::config::QsmAlgorithm;

    fn default_config() -> PipelineConfig {
        PipelineConfig::default()
    }

    #[test]
    fn test_more_echoes_more_memory() {
        let config = default_config();
        let est_1 = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        let est_4 = estimate_peak_memory_bytes(128, 128, 128, 4, true, &config);
        assert!(est_4 > est_1, "4 echoes ({}) should use more memory than 1 ({})", est_4, est_1);
    }

    #[test]
    fn test_swi_adds_memory() {
        let mut config = default_config();
        config.pipeline.do_swi = false;
        let est_no_swi = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        config.pipeline.do_swi = true;
        let est_swi = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        assert!(est_swi >= est_no_swi, "SWI ({}) should use at least as much memory as no SWI ({})", est_swi, est_no_swi);
    }

    #[test]
    fn test_tkd_less_than_rts() {
        let mut config = default_config();
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        let est_tkd = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        config.inversion.algorithm = QsmAlgorithm::Rts;
        let est_rts = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        assert!(est_tkd < est_rts, "TKD ({}) should use less memory than RTS ({})", est_tkd, est_rts);
    }

    #[test]
    fn test_lbv_less_than_vsharp() {
        let mut config = default_config();
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        config.bg_removal.algorithm = BfAlgorithm::Lbv;
        let est_lbv = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        config.bg_removal.algorithm = BfAlgorithm::Vsharp;
        let est_vsharp = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        assert!(est_lbv < est_vsharp, "LBV ({}) should use less memory than V-SHARP ({})", est_lbv, est_vsharp);
    }

    #[test]
    fn test_reasonable_range() {
        // A 64^3 single-echo minimal pipeline should use less than 1 GB
        let mut config = default_config();
        config.inversion.algorithm = QsmAlgorithm::Tkd;
        config.bg_removal.algorithm = BfAlgorithm::Lbv;
        config.pipeline.do_swi = false;
        let est = estimate_peak_memory_bytes(64, 64, 64, 1, false, &config);
        let one_gb = 1024 * 1024 * 1024;
        assert!(est < one_gb, "64^3 minimal pipeline should use < 1 GB, got {}", format_bytes(est));
        assert!(est > 0, "Estimate should be positive");
    }

    #[test]
    fn test_typical_brain_estimate() {
        // 256x256x176, 4 echoes, default GRE (RTS + PDF + ROMEO)
        let config = default_config();
        let est = estimate_peak_memory_bytes(256, 256, 176, 4, true, &config);
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
        let est = estimate_peak_memory_bytes(128, 128, 128, 1, true, &config);
        assert!(est > 0, "TGV estimate should be positive");
    }

    #[test]
    fn test_no_magnitude_less_memory() {
        let config = default_config();
        let est_mag = estimate_peak_memory_bytes(128, 128, 128, 4, true, &config);
        let est_no_mag = estimate_peak_memory_bytes(128, 128, 128, 4, false, &config);
        assert!(est_no_mag < est_mag, "No magnitude ({}) should use less memory than with magnitude ({})", est_no_mag, est_mag);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024), "3.5 GB");
        assert_eq!(format_bytes(500 * 1024 * 1024), "500 MB");
    }

    #[test]
    fn test_available_memory_positive() {
        let mem = available_memory_bytes();
        assert!(mem > 0, "Available memory should be positive");
    }
}
