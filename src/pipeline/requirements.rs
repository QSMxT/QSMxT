//! What each enabled output needs from a run's data: the engine's rules (re-exported), applied
//! to the runs discovered in a BIDS directory.

use crate::bids::discovery::QsmRun;
use crate::pipeline::config::PipelineConfig;

pub use qsmxt_engine::requirements::*;
pub use qsmxt_engine::scan::ScanFacts;

/// The facts about one run that decide what it can produce. Built from the discovered run; the
/// bring-your-own derivatives the config names are looked up here, not per stage.
pub fn scan_facts(run: &QsmRun, config: &PipelineConfig) -> ScanFacts {
    use crate::pipeline::runner::find_custom_derivative as find;
    let s = &config.separation;
    let found = |tool: Option<&str>, glob: &str, exclude: &[&str]| {
        tool.and_then(|t| find(run, t, glob, exclude)).is_some()
    };
    ScanFacts {
        n_echoes: run.echoes.len(),
        echo_times: run.echo_times.clone(),
        has_magnitude: run.has_magnitude,
        n_coils: run.coils.as_ref().map_or(1, |c| c.len()),
        field_strength: run.magnetic_field_strength,
        b0_dir: run.b0_dir,
        mese_echo_times: run.mese.as_ref().map(|m| m.echo_times.clone()),
        custom_dseg: found(config.segmentation.custom_dseg_tool.as_deref(), "*_dseg.nii*", &[]),
        custom_qsm: found(s.custom_qsm_tool.as_deref(), "*_Chimap.nii*", &["desc-"]),
        custom_r2: found(s.custom_r2_tool.as_deref(), "*_R2map.nii*", &[]),
        custom_r2prime: found(s.custom_r2prime_tool.as_deref(), "*_R2primemap.nii*", &[]),
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
