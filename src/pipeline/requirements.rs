//! What each enabled output needs from a run's data: the engine's rules (re-exported), applied
//! to the runs discovered in a BIDS directory.

use crate::bids::discovery::QsmRun;
use crate::pipeline::config::PipelineConfig;

pub use qsmxt_engine::requirements::*;
pub use qsmxt_engine::scan::ScanFacts;

/// `config` for `run`: a pipeline chosen on the command line (`--pipeline`, applied before any scan
/// was read) completed with the scan's facts ([`qsmxt_engine::pipelines::on_scan`]), with the
/// changes it made, for the log. Unchanged when no pipeline was chosen.
pub fn run_config(run: &QsmRun, config: &PipelineConfig) -> crate::Result<(PipelineConfig, Vec<qsmxt_engine::pipelines::Change>)> {
    let facts = scan_facts(run, config);
    qsmxt_engine::pipelines::on_scan(config, &qsmxt_engine::schema::ViewFacts::from(&facts))
        .map_err(crate::error::QsmxtError::Config)
}

/// The config the runs will run, for text about them all (the dry run's methods, graph and
/// settings): each run's [`run_config`] when they all agree, else `config` as given.
pub fn planned_config(config: &PipelineConfig, runs: &[QsmRun]) -> PipelineConfig {
    let completed: Vec<PipelineConfig> = runs.iter().filter_map(|r| run_config(r, config).ok().map(|(c, _)| c)).collect();
    let same = |a: &PipelineConfig, b: &PipelineConfig| serde_json::to_value(a).ok() == serde_json::to_value(b).ok();
    match completed.first() {
        Some(first) if completed.len() == runs.len() && completed.iter().all(|c| same(c, first)) => first.clone(),
        _ => config.clone(),
    }
}

/// How the run's phase is converted to ppm, for the run listing: the centre frequency when its
/// sidecar gives `ImagingFrequency` (`, ppm from ImagingFrequency 123.200 MHz = 2.894 T`), else
/// a note that the nominal field strength is used.
pub fn centre_frequency_note(run: &QsmRun) -> String {
    let f = run.echoes.first()
        .and_then(|e| crate::bids::sidecar::read_sidecar(&e.phase_json).ok())
        .and_then(|sc| sc.imaging_frequency)
        .filter(|f| f.is_finite() && *f > 0.0);
    match f {
        Some(f) => format!(", ppm from ImagingFrequency {f:.3} MHz = {:.4} T",
            qsmxt_engine::scan::working_field_strength(run.magnetic_field_strength, Some(f))),
        None => ", no ImagingFrequency: ppm from the nominal field".to_string(),
    }
}

/// The facts about one run that decide what it can produce. Built from the discovered run; the
/// bring-your-own derivatives the config names are looked up here, not per stage.
pub fn scan_facts(run: &QsmRun, config: &PipelineConfig) -> ScanFacts {
    use crate::pipeline::runner::find_custom_derivative as find;
    let s = &config.separation;
    let found = |tool: Option<&str>, glob: &str, exclude: &[&str]| {
        tool.and_then(|t| find(run, t, glob, exclude)).is_some()
    };
    // The first echo's sidecar (already read once by discovery, so it parses).
    let sidecar = run.echoes.first().and_then(|e| crate::bids::sidecar::read_sidecar(&e.phase_json).ok());
    let imaging_frequency = sidecar.as_ref().and_then(|sc| sc.imaging_frequency).filter(|f| f.is_finite() && *f > 0.0);
    ScanFacts {
        dims: run.dims,
        voxel_size: None,
        n_echoes: run.echoes.len(),
        echo_times: run.echo_times.clone(),
        has_magnitude: run.has_magnitude,
        n_coils: run.coils.as_ref().map_or(1, |c| c.len()),
        real_imaginary: run.echoes.first().is_some_and(|e| e.imaginary_nifti.is_some()),
        field_strength: qsmxt_engine::scan::working_field_strength(run.magnetic_field_strength, imaging_frequency),
        nominal_field_strength: imaging_frequency.map(|_| run.magnetic_field_strength),
        imaging_frequency,
        b0_dir: run.b0_dir,
        mese_echo_times: run.mese.as_ref().map(|m| m.echo_times.clone()),
        custom_mask: config.masking.custom_mask_tool.as_deref()
            .and_then(|tool| crate::pipeline::runner::find_custom_mask(run, tool)).is_some(),
        custom_dseg: found(config.segmentation.custom_dseg_tool.as_deref(), "*_dseg.nii*", &[]),
        custom_qsm: found(s.custom_qsm_tool.as_deref(), "*_Chimap.nii*", &["desc-"]),
        custom_r2: found(s.custom_r2_tool.as_deref(), "*_R2map.nii*", &[]),
        custom_r2prime: found(s.custom_r2prime_tool.as_deref(), "*_R2primemap.nii*", &[]),
        manufacturer: sidecar.and_then(|sc| sc.manufacturer),
    }
}

/// Check every run before any is processed. Warnings are logged; errors, all of them at once,
/// fail the command.
pub fn check_runs(runs: &[QsmRun], config: &PipelineConfig) -> crate::Result<()> {
    let mut check = Check::default();
    for run in runs {
        check_scan(&run.key.to_string(), &scan_facts(run, config), config, &mut check);
    }
    Ok(check.finish()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::discovery::{EchoFiles, QsmRun};
    use crate::bids::entities::AcquisitionKey;
    use std::path::PathBuf;

    fn run(echo_times: &[f64], has_magnitude: bool) -> QsmRun {
        QsmRun {
            key: AcquisitionKey {
                subject: "01".into(), session: None, acquisition: None, reconstruction: None,
                inversion: None, run: None, suffix: "MEGRE".into(),
            },
            coils: None,
            echoes: (1..=echo_times.len()).map(|n| EchoFiles {
                echo_number: n as u32,
                phase_json: PathBuf::from("p.json"),
                phase_nifti: PathBuf::from("/nonexistent/sub-01/anat/p.nii"),
                imaginary_nifti: None,
                magnitude_json: None,
                magnitude_nifti: None,
            }).collect(),
            magnetic_field_strength: 3.0,
            echo_times: echo_times.to_vec(),
            b0_dir: None,
            dims: (4, 4, 4),
            has_magnitude,
            mese: None,
        }
    }

    #[test]
    fn check_runs_reports_every_problem_at_once() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_swi = true;
        c.pipeline.do_r2starmap = true;
        let err = check_runs(&[run(&[0.005, 0.010], false)], &c).unwrap_err().to_string();
        assert!(err.contains("sub-01") && err.contains("--do-swi") && err.contains("--do-r2starmap"), "{err}");
        assert!(check_runs(&[run(&[0.005, 0.010, 0.015, 0.020], true)], &c).is_ok());
    }

    /// A custom derivative the config names but the dataset does not have is not found.
    #[test]
    fn a_named_custom_derivative_that_is_not_there_is_not_found() {
        let mut c = PipelineConfig::default();
        c.separation.custom_r2_tool = Some("mytool".into());
        let f = scan_facts(&run(&[0.005, 0.010, 0.015], true), &c);
        assert!(!f.custom_r2 && !f.custom_dseg);
        assert_eq!(f.n_coils, 1);
        assert_eq!(f.n_echoes, 3);
    }
}
