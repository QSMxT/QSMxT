use std::path::{Path, PathBuf};
use std::process::Command;

use log::info;

use crate::bids::discovery::QsmRun;
use crate::pipeline::config::PipelineConfig;

/// Shell-quote a string for safe interpolation into bash scripts.
/// Uses single quotes and escapes any embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Generate SLURM job scripts for all runs.
#[allow(clippy::too_many_arguments)]
pub fn generate_all_slurm(
    runs: &[QsmRun],
    bids_dir: &Path,
    output_dir: &Path,
    _config: &PipelineConfig,
    account: &str,
    partition: Option<&str>,
    time: &str,
    mem_gb: usize,
    cpus: usize,
) -> crate::Result<Vec<PathBuf>> {
    let derivatives_dir = output_dir.join("derivatives").join("qsmxt.rs");
    let slurm_dir = derivatives_dir.join("slurm");
    std::fs::create_dir_all(&slurm_dir)?;

    // Save config for SLURM jobs to reference
    let config_path = derivatives_dir.join("pipeline_config.toml");
    std::fs::write(&config_path, _config.to_toml().unwrap_or_default())?;

    let qsmxt_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("qsmxt"));

    let mut scripts = Vec::new();

    for run in runs {
        let job_name = format!("qsmxt_{}", run.key);
        let script_path = slurm_dir.join(format!("{}.sh", job_name));

        let partition_line = match partition {
            Some(p) => format!("#SBATCH --partition={}", p),
            None => String::new(),
        };

        let run_key = run.key.to_string();

        let script = format!(
            r#"#!/bin/bash
#SBATCH --job-name={job_name}
#SBATCH --account={account}
{partition_line}
#SBATCH --time={time}
#SBATCH --mem={mem}G
#SBATCH --cpus-per-task={cpus}
#SBATCH --output={job_name}_%j.out
#SBATCH --error={job_name}_%j.err

{binary} run {bids_dir} {output_dir} \
  --config {config} \
  --include {include} \
  --n-procs {cpus}
"#,
            job_name = job_name,
            account = account,
            partition_line = partition_line,
            time = time,
            mem = mem_gb,
            cpus = cpus,
            binary = shell_quote(&qsmxt_bin.display().to_string()),
            bids_dir = shell_quote(&bids_dir.display().to_string()),
            output_dir = shell_quote(&output_dir.display().to_string()),
            config = shell_quote(&config_path.display().to_string()),
            include = shell_quote(&run_key),
        );

        std::fs::write(&script_path, script)?;
        scripts.push(script_path);
    }

    Ok(scripts)
}

/// Submit SLURM scripts using sbatch.
pub fn submit_scripts(scripts: &[PathBuf]) -> crate::Result<()> {
    for script in scripts {
        info!("Submitting {}", script.display());
        let output = Command::new("sbatch")
            .arg(script)
            .output()
            .map_err(|e| crate::error::QsmxtError::Slurm(format!("sbatch failed: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            info!("  {}", stdout.trim());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::error::QsmxtError::Slurm(format!(
                "sbatch failed for {}: {}",
                script.display(),
                stderr
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::discovery::{EchoFiles, QsmRun};
    use crate::bids::entities::AcquisitionKey;

    fn dummy_run_with_key(subject: &str, session: Option<&str>) -> QsmRun {
        QsmRun {
            key: AcquisitionKey {
                subject: subject.to_string(),
                session: session.map(|s| s.to_string()),
                acquisition: None,
                reconstruction: None,
                inversion: None,
                run: None,
                suffix: "MEGRE".to_string(),
            },
            echoes: vec![EchoFiles {
                echo_number: 1,
                phase_nifti: PathBuf::from("fake.nii"),
                phase_json: PathBuf::from("fake.json"),
                magnitude_nifti: None,
                magnitude_json: None,
            }],
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02],
            b0_dir: (0.0, 0.0, 1.0),
            dims: (64, 64, 64),
            has_magnitude: false,
        }
    }

    #[test]
    fn test_slurm_script_contains_sbatch_directives() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None)];
        let config = PipelineConfig::default();
        let scripts = generate_all_slurm(
            &runs,
            Path::new("/bids"),
            dir.path(),
            &config,
            "myaccount",
            Some("gpu"),
            "02:00:00",
            32,
            4,
        )
        .unwrap();

        let content = std::fs::read_to_string(&scripts[0]).unwrap();
        assert!(content.contains("#SBATCH --job-name="));
        assert!(content.contains("#SBATCH --account=myaccount"));
        assert!(content.contains("#SBATCH --time=02:00:00"));
        assert!(content.contains("#SBATCH --mem=32G"));
        assert!(content.contains("#SBATCH --cpus-per-task=4"));
    }

    #[test]
    fn test_slurm_script_partition_omitted_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None)];
        let config = PipelineConfig::default();
        let scripts = generate_all_slurm(
            &runs,
            Path::new("/bids"),
            dir.path(),
            &config,
            "acct",
            None,
            "01:00:00",
            16,
            2,
        )
        .unwrap();

        let content = std::fs::read_to_string(&scripts[0]).unwrap();
        assert!(!content.contains("--partition"), "Partition should be omitted");
    }

    #[test]
    fn test_slurm_script_include_flag() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", Some("pre"))];
        let config = PipelineConfig::default();
        let scripts = generate_all_slurm(
            &runs,
            Path::new("/bids"),
            dir.path(),
            &config,
            "acct",
            None,
            "01:00:00",
            16,
            2,
        )
        .unwrap();

        let content = std::fs::read_to_string(&scripts[0]).unwrap();
        assert!(content.contains("--include 'sub-01_ses-pre_MEGRE'"), "Should contain --include with run key");
    }

    #[test]
    fn test_shell_quote_simple() {
        assert_eq!(shell_quote("hello"), "'hello'");
    }

    #[test]
    fn test_shell_quote_with_spaces() {
        assert_eq!(shell_quote("/path/with spaces/dir"), "'/path/with spaces/dir'");
    }

    #[test]
    fn test_shell_quote_with_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_slurm_script_paths_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let runs = vec![dummy_run_with_key("01", None)];
        let config = PipelineConfig::default();
        let scripts = generate_all_slurm(
            &runs,
            Path::new("/bids/my data"),
            dir.path(),
            &config,
            "acct",
            None,
            "01:00:00",
            16,
            2,
        )
        .unwrap();

        let content = std::fs::read_to_string(&scripts[0]).unwrap();
        assert!(content.contains("'/bids/my data'"), "Paths with spaces should be quoted");
    }
}
