use log::{error, info, warn};
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
    pub mem_limit_bytes: Option<u64>,
    /// What set the memory limit, for the log: e.g. " (within this job's or container's limit
    /// of 48.0 GB)" ([`crate::pipeline::memory::Available::describe`]); empty for the system's.
    pub mem_limit_source: String,
    /// Force re-run, ignoring cached state
    pub force: bool,
    /// Optional source DICOM directory for `--export-dicom` (metadata inheritance).
    pub source_dicom: Option<std::path::PathBuf>,
    /// Optional subset of maps to export as DICOM (`--dicom-outputs`).
    pub dicom_outputs: Option<Vec<String>>,
    /// The recommendations `--accept-recommendations` applied that each run had, by run key: its
    /// run record keeps them.
    pub applied: std::collections::HashMap<String, Vec<qsmxt_engine::recommendations::Recommendation>>,
}

/// Execute pipeline runs in parallel using Rayon.
///
/// When `mem_limit_bytes` is set, estimates per-run memory usage from
/// volume dimensions and limits concurrency to avoid exceeding the limit.
pub fn execute_local(
    runs: &[QsmRun],
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    exec_config: &ExecutionConfig,
) -> Vec<crate::Result<()>> {
    let n_threads = compute_concurrency(runs, config, exec_config);

    rayon::ThreadPoolBuilder::new()
        .num_threads(n_threads)
        .build_global()
        .ok();

    let total_start = std::time::Instant::now();

    let results: Vec<crate::Result<()>> = runs.par_iter()
        .map(|run| {
            info!("Processing {}", run.key);
            let run_start = std::time::Instant::now();

            let result = runner::run_pipeline_applied(
                run,
                config,
                output,
                exec_config.force,
                exec_config.applied.get(&run.key.to_string()).map_or(&[], |a| a.as_slice()),
                &|msg| { info!("{}: {}", run.key, msg); },
            );

            let elapsed = run_start.elapsed();
            match &result {
                Ok(()) => {
                    info!("{}: Done ({:.1}s)", run.key, elapsed.as_secs_f64());
                    // Log final output paths
                    let published = &output.published(&run.key);
                    let final_paths: Vec<_> = [
                        published.qsm_path(&run.key),
                        published.mask_path(&run.key),
                        published.magnitude_path(&run.key),
                        published.swi_path(&run.key),
                        published.swi_mip_path(&run.key),
                        published.t2star_path(&run.key),
                        published.r2star_path(&run.key),
                    ]
                    .into_iter()
                    .filter(|p| p.exists())
                    .collect();
                    for path in &final_paths {
                        info!("{}: -> {}", run.key, path.display());
                    }

                    // Optional DICOM export (best-effort: never fails the run).
                    if config.pipeline.export_dicom {
                        // χ-sepnet's toolbox route computes R2′ only on the scans that can measure it.
                        let facts = crate::pipeline::requirements::scan_facts(run, config);
                        let normalised = &config.normalised_for(&facts).0;
                        let opts = crate::dicom::export::DicomExportOptions {
                            export_swi: normalised.pipeline.do_swi,
                            export_smwi: normalised.pipeline.do_smwi,
                            export_t2star: normalised.pipeline.do_t2starmap,
                            export_r2star: normalised.pipeline.do_r2starmap,
                            export_r2: normalised.pipeline.do_r2map,
                            export_r2prime: normalised.pipeline.do_r2primemap,
                            export_chisep: normalised.pipeline.do_chi_separation,
                            source_dicom: exec_config.source_dicom.as_deref(),
                            outputs_filter: exec_config.dicom_outputs.as_deref(),
                            segmentation: segmentation_export(normalised, facts.custom_dseg),
                        };
                        let exported = match crate::dicom::export::export_run_dicoms(run, output, &opts) {
                            Ok(0) => {
                                warn!("{}: DICOM export wrote nothing: no map it was set to export was found", run.key);
                                false
                            }
                            Ok(_) => true,
                            Err(e) => {
                                error!("{}: DICOM export failed - {}", run.key, e);
                                false
                            }
                        };
                        // The methods say it only of a run that did it. The record is the one the
                        // run wrote, of the settings it was given.
                        if let Err(e) = runner::record_dicom_export(run, config, output, exported) {
                            warn!("{}: could not record its DICOM export: {}", run.key, e);
                        }
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
/// The DICOM SEG description of the run's segmentation, when it makes one (SynthSeg) or reads one
/// (`supplied`: a `dseg` from `--use-custom-dseg`'s tool was found for the scan; without one,
/// SynthSeg runs instead).
fn segmentation_export(config: &PipelineConfig, supplied: bool) -> Option<crate::dicom::export::SegmentationExport> {
    let seg = &config.segmentation;
    if !config.pipeline.do_segmentation && seg.custom_dseg_tool.is_none() {
        return None;
    }
    Some(match seg.custom_dseg_tool.as_deref() {
        Some(tool) if supplied => crate::dicom::export::SegmentationExport {
            synthseg: false,
            algorithm: if tool == "*" { "supplied segmentation".into() } else { tool.to_string() },
        },
        _ => crate::dicom::export::SegmentationExport {
            synthseg: true,
            algorithm: match seg.version {
                crate::pipeline::config::SynthSegVersion::V1 => "SynthSeg 1.0".into(),
                crate::pipeline::config::SynthSegVersion::V2 => "SynthSeg 2.0".into(),
            },
        },
    })
}

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
                run.coils.as_ref().map_or(1, |c| c.len()),
                run.has_magnitude,
                config,
            )
        })
        .max()
        .unwrap_or(0);

    let max_by_memory = (mem_limit.checked_div(per_run))
        .map(|v| v.max(1) as usize)
        .unwrap_or(exec_config.n_procs);

    let effective = exec_config.n_procs.min(max_by_memory);

    info!(
        "Memory: {} per run (est.), {} available{} — {} concurrent run(s) (requested {})",
        memory::format_bytes(per_run),
        memory::format_bytes(mem_limit),
        exec_config.mem_limit_source,
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

    /// The SEG is coded as SynthSeg's when SynthSeg made the segmentation, including when a
    /// supplied one was asked for and not found; generically when a supplied one was read.
    #[test]
    fn the_seg_says_where_the_segmentation_came_from() {
        let mut c = PipelineConfig::default();
        assert!(segmentation_export(&c, false).is_none(), "no segmentation, no SEG");
        c.pipeline.do_segmentation = true;
        let s = segmentation_export(&c, false).unwrap();
        assert!(s.synthseg && s.algorithm == "SynthSeg 1.0");
        c.pipeline.do_segmentation = false;
        c.segmentation.custom_dseg_tool = Some("fastsurfer".into());
        let s = segmentation_export(&c, true).unwrap();
        assert!(!s.synthseg && s.algorithm == "fastsurfer");
        assert!(segmentation_export(&c, false).unwrap().synthseg, "none found: SynthSeg ran instead");
    }

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
                    imaginary_nifti: None,
                })
                .collect(),
            coils: None,
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02; n_echoes],
            b0_dir: None,
            dims,
            has_magnitude: true,
            mese: None,
        }
    }

    #[test]
    fn test_compute_concurrency_no_limit() {
        let runs = vec![dummy_run((64, 64, 64), 3)];
        let config = PipelineConfig::default();
        let exec = ExecutionConfig {
            n_procs: 8,
            mem_limit_bytes: None,
            mem_limit_source: String::new(),
            force: false,
            source_dicom: None,
            dicom_outputs: None,
            applied: Default::default(),
        };
        assert_eq!(compute_concurrency(&runs, &config, &exec), 8);
    }

    #[test]
    fn test_compute_concurrency_empty_runs() {
        let config = PipelineConfig::default();
        let exec = ExecutionConfig {
            n_procs: 8,
            mem_limit_bytes: Some(1024 * 1024 * 1024),
            mem_limit_source: String::new(),
            force: false,
            source_dicom: None,
            dicom_outputs: None,
            applied: Default::default(),
        };
        assert_eq!(compute_concurrency(&[], &config, &exec), 8);
    }

    #[test]
    fn test_compute_concurrency_memory_limited() {
        let runs = vec![dummy_run((256, 256, 256), 4)];
        let config = PipelineConfig::default();
        // Get the per-run estimate
        let per_run = memory::estimate_peak_memory_bytes(256, 256, 256, 4, 1, true, &config);
        // Allow exactly 2 runs
        let exec = ExecutionConfig {
            n_procs: 16,
            mem_limit_bytes: Some(per_run * 2),
            mem_limit_source: String::new(),
            force: false,
            source_dicom: None,
            dicom_outputs: None,
            applied: Default::default(),
        };
        assert_eq!(compute_concurrency(&runs, &config, &exec), 2);
    }

    #[test]
    fn test_compute_concurrency_memory_caps_below_nprocs() {
        let runs = vec![dummy_run((256, 256, 256), 4)];
        let config = PipelineConfig::default();
        let per_run = memory::estimate_peak_memory_bytes(256, 256, 256, 4, 1, true, &config);
        // Allow only 1 run worth of memory, but request 16 procs
        let exec = ExecutionConfig {
            n_procs: 16,
            mem_limit_bytes: Some(per_run),
            mem_limit_source: String::new(),
            force: false,
            source_dicom: None,
            dicom_outputs: None,
            applied: Default::default(),
        };
        assert_eq!(compute_concurrency(&runs, &config, &exec), 1);
    }
}
