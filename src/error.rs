use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum QsmxtError {
    #[error("BIDS discovery error: {0}")]
    BidsDiscovery(String),

    #[error("No phase files found for {subject}{session}")]
    NoPhaseFiles { subject: String, session: String },

    #[error("JSON sidecar missing required field '{field}' in {path}")]
    MissingSidecarField { field: String, path: PathBuf },

    #[error("JSON sidecar parse error for {path}: {source}")]
    SidecarParse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("NIfTI I/O error: {0}")]
    NiftiIo(String),

    #[error("Dimension mismatch: {0}")]
    DimensionMismatch(String),

    #[error("Pipeline configuration error: {0}")]
    Config(String),

    #[error("Algorithm error in {stage}: {message}")]
    Algorithm { stage: String, message: String },

    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    ConfigLib(#[from] qsmxt_config::ConfigError),

    #[error("SLURM submission error: {0}")]
    Slurm(String),

    #[error("Update error: {0}")]
    Update(String),

    #[error("DICOM conversion error: {0}")]
    Dicom(String),
}

pub type Result<T> = std::result::Result<T, QsmxtError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_bids_discovery() {
        let e = QsmxtError::BidsDiscovery("no files".to_string());
        assert_eq!(format!("{}", e), "BIDS discovery error: no files");
    }

    #[test]
    fn test_error_display_no_phase_files() {
        let e = QsmxtError::NoPhaseFiles {
            subject: "sub-01".to_string(),
            session: " ses-pre".to_string(),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("sub-01"));
        assert!(msg.contains("ses-pre"));
    }

    #[test]
    fn test_error_display_missing_sidecar_field() {
        let e = QsmxtError::MissingSidecarField {
            field: "EchoTime".to_string(),
            path: PathBuf::from("/data/sidecar.json"),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("EchoTime"));
        assert!(msg.contains("sidecar.json"));
    }

    #[test]
    fn test_error_display_nifti_io() {
        let e = QsmxtError::NiftiIo("bad header".to_string());
        assert_eq!(format!("{}", e), "NIfTI I/O error: bad header");
    }

    #[test]
    fn test_error_display_dimension_mismatch() {
        let e = QsmxtError::DimensionMismatch("phase vs mask".to_string());
        assert!(format!("{}", e).contains("phase vs mask"));
    }

    #[test]
    fn test_error_display_config() {
        let e = QsmxtError::Config("bad value".to_string());
        assert!(format!("{}", e).contains("bad value"));
    }

    #[test]
    fn test_error_display_algorithm() {
        let e = QsmxtError::Algorithm {
            stage: "unwrap".to_string(),
            message: "convergence failed".to_string(),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("unwrap"));
        assert!(msg.contains("convergence failed"));
    }

    #[test]
    fn test_error_display_slurm() {
        let e = QsmxtError::Slurm("sbatch not found".to_string());
        assert!(format!("{}", e).contains("sbatch not found"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let e: QsmxtError = io_err.into();
        assert!(format!("{}", e).contains("file missing"));
    }
}
