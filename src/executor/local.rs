use log::{error, info};
use rayon::prelude::*;

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::pipeline::config::PipelineConfig;
use crate::pipeline::{memory, runner};

/// Configuration for local execution.
pub struct ExecutionConfig {
    /// Maximum number of threads (from --n-procs or auto-detected)
    pub n_procs: usize,
    /// Memory limit in bytes for concurrent run scheduling (None = no limit)
    pub mem_limit_bytes: Option<usize>,
    /// Force re-run, ignoring cached state
    pub force: bool,
    /// Remove intermediate files after completion
    pub clean_intermediates: bool,
}

/// Execute pipeline runs in parallel using Rayon.
///
/// When `mem_limit_bytes` is set, estimates per-run memory usage from
/// volume dimensions and limits concurrency to avoid exceeding the limit.
pub fn execute_local(
    runs: Vec<QsmRun>,
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    exec_config: &ExecutionConfig,
) -> Vec<crate::Result<()>> {
    let n_threads = compute_concurrency(&runs, config, exec_config);

    rayon::ThreadPoolBuilder::new()
        .num_threads(n_threads)
        .build_global()
        .ok();

    let total_start = std::time::Instant::now();

    let results: Vec<crate::Result<()>> = runs.par_iter()
        .map(|run| {
            info!("Processing {}", run.key);
            let run_start = std::time::Instant::now();

            let result = runner::run_pipeline_cached(
                run,
                config,
                output,
                exec_config.force,
                exec_config.clean_intermediates,
                &|msg| { info!("{}: {}", run.key, msg); },
            );

            let elapsed = run_start.elapsed();
            match &result {
                Ok(()) => {
                    info!("{}: Done ({:.1}s)", run.key, elapsed.as_secs_f64());
                    // Log final output paths
                    let final_paths: Vec<_> = [
                        output.qsm_path(&run.key),
                        output.mask_path(&run.key),
                        output.magnitude_path(&run.key),
                        output.swi_path(&run.key),
                        output.swi_mip_path(&run.key),
                        output.t2star_path(&run.key),
                        output.r2star_path(&run.key),
                    ]
                    .into_iter()
                    .filter(|p| p.exists())
                    .collect();
                    for path in &final_paths {
                        info!("{}: -> {}", run.key, path.display());
                    }
                }
                Err(e) => error!("{}: FAILED after {:.1}s - {}", run.key, elapsed.as_secs_f64(), e),
            }

            result
        })
        .collect();

    let total_elapsed = total_start.elapsed();
    let n_ok = results.iter().filter(|r| r.is_ok()).count();
    let n_fail = results.iter().filter(|r| r.is_err()).count();
    if n_fail == 0 {
        info!("All {} run(s) completed in {:.1}s", n_ok, total_elapsed.as_secs_f64());
    } else {
        info!("{} run(s) completed, {} failed in {:.1}s", n_ok, n_fail, total_elapsed.as_secs_f64());
    }

    results
}

/// Compute the effective number of concurrent threads based on memory constraints.
fn compute_concurrency(
    runs: &[QsmRun],
    config: &PipelineConfig,
    exec_config: &ExecutionConfig,
) -> usize {
    let Some(mem_limit) = exec_config.mem_limit_bytes else {
        return exec_config.n_procs;
    };

    if runs.is_empty() {
        return exec_config.n_procs;
    }

    // Use the maximum estimate across all runs to handle heterogeneous dimensions
    let per_run = runs
        .iter()
        .map(|run| {
            let (nx, ny, nz) = run.dims;
            memory::estimate_peak_memory_bytes(
                nx,
                ny,
                nz,
                run.echoes.len(),
                run.has_magnitude,
                config,
            )
        })
        .max()
        .unwrap_or(0);

    let max_by_memory = (mem_limit.checked_div(per_run))
        .map(|v| v.max(1))
        .unwrap_or(exec_config.n_procs);

    let effective = exec_config.n_procs.min(max_by_memory);

    info!(
        "Memory: {} per run (est.), {} available — {} concurrent run(s) (requested {})",
        memory::format_bytes(per_run),
        memory::format_bytes(mem_limit),
        effective,
        exec_config.n_procs,
    );

    effective
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::discovery::{EchoFiles, QsmRun};
    use crate::bids::entities::AcquisitionKey;
    use std::path::PathBuf;

    fn dummy_run(dims: (usize, usize, usize), n_echoes: usize) -> QsmRun {
        QsmRun {
            key: AcquisitionKey {
                subject: "01".to_string(),
                session: None,
                acquisition: None,
                reconstruction: None,
                inversion: None,
                run: None,
                suffix: "MEGRE".to_string(),
            },
            echoes: (0..n_echoes)
                .map(|i| EchoFiles {
                    echo_number: i as u32 + 1,
                    phase_nifti: PathBuf::from("fake.nii"),
                    phase_json: PathBuf::from("fake.json"),
                    magnitude_nifti: Some(PathBuf::from("fake_mag.nii")),
                    magnitude_json: Some(PathBuf::from("fake_mag.json")),
                })
                .collect(),
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02; n_echoes],
            b0_dir: (0.0, 0.0, 1.0),
            dims,
            has_magnitude: true,
        }
    }

    #[test]
    fn test_compute_concurrency_no_limit() {
        let runs = vec![dummy_run((64, 64, 64), 3)];
        let config = PipelineConfig::default();
        let exec = ExecutionConfig {
            n_procs: 8,
            mem_limit_bytes: None,
            force: false,
            clean_intermediates: false,
        };
        assert_eq!(compute_concurrency(&runs, &config, &exec), 8);
    }

    #[test]
    fn test_compute_concurrency_empty_runs() {
        let config = PipelineConfig::default();
        let exec = ExecutionConfig {
            n_procs: 8,
            mem_limit_bytes: Some(1024 * 1024 * 1024),
            force: false,
            clean_intermediates: false,
        };
        assert_eq!(compute_concurrency(&[], &config, &exec), 8);
    }

    #[test]
    fn test_compute_concurrency_memory_limited() {
        let runs = vec![dummy_run((256, 256, 256), 4)];
        let config = PipelineConfig::default();
        // Get the per-run estimate
        let per_run = memory::estimate_peak_memory_bytes(256, 256, 256, 4, true, &config);
        // Allow exactly 2 runs
        let exec = ExecutionConfig {
            n_procs: 16,
            mem_limit_bytes: Some(per_run * 2),
            force: false,
            clean_intermediates: false,
        };
        assert_eq!(compute_concurrency(&runs, &config, &exec), 2);
    }

    #[test]
    fn test_compute_concurrency_memory_caps_below_nprocs() {
        let runs = vec![dummy_run((256, 256, 256), 4)];
        let config = PipelineConfig::default();
        let per_run = memory::estimate_peak_memory_bytes(256, 256, 256, 4, true, &config);
        // Allow only 1 run worth of memory, but request 16 procs
        let exec = ExecutionConfig {
            n_procs: 16,
            mem_limit_bytes: Some(per_run),
            force: false,
            clean_intermediates: false,
        };
        assert_eq!(compute_concurrency(&runs, &config, &exec), 1);
    }
}
