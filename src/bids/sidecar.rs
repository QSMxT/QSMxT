use serde::Deserialize;
use std::path::Path;

use crate::error::QsmxtError;

/// Relevant fields from a BIDS JSON sidecar.
#[derive(Debug, Clone, Deserialize)]
pub struct QsmSidecar {
    #[serde(rename = "EchoTime")]
    pub echo_time: f64,

    #[serde(rename = "MagneticFieldStrength")]
    pub magnetic_field_strength: f64,

    #[serde(rename = "B0_dir")]
    pub b0_dir: Option<Vec<f64>>,
}

/// Read a BIDS JSON sidecar, extracting QSM-relevant fields.
pub fn read_sidecar(path: &Path) -> crate::Result<QsmSidecar> {
    let text = std::fs::read_to_string(path)?;

    // Parse as generic Value first to give better error messages
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| QsmxtError::SidecarParse {
            path: path.to_owned(),
            source: e,
        })?;

    let echo_time = value
        .get("EchoTime")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| QsmxtError::MissingSidecarField {
            field: "EchoTime".to_string(),
            path: path.to_owned(),
        })?;

    let magnetic_field_strength = value
        .get("MagneticFieldStrength")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| QsmxtError::MissingSidecarField {
            field: "MagneticFieldStrength".to_string(),
            path: path.to_owned(),
        })?;

    let b0_dir = value.get("B0_dir").and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_f64())
                .collect::<Vec<f64>>()
        })
    });

    Ok(QsmSidecar {
        echo_time,
        magnetic_field_strength,
        b0_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_json(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_read_sidecar_valid() {
        let f = write_json(r#"{"EchoTime": 0.02, "MagneticFieldStrength": 3.0}"#);
        let sc = read_sidecar(f.path()).unwrap();
        assert!((sc.echo_time - 0.02).abs() < 1e-10);
        assert!((sc.magnetic_field_strength - 3.0).abs() < 1e-10);
        assert!(sc.b0_dir.is_none());
    }

    #[test]
    fn test_read_sidecar_with_b0_dir() {
        let f = write_json(
            r#"{"EchoTime": 0.01, "MagneticFieldStrength": 7.0, "B0_dir": [0.0, 0.0, 1.0]}"#,
        );
        let sc = read_sidecar(f.path()).unwrap();
        let b0 = sc.b0_dir.unwrap();
        assert_eq!(b0.len(), 3);
        assert!((b0[2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_read_sidecar_missing_echo_time() {
        let f = write_json(r#"{"MagneticFieldStrength": 3.0}"#);
        let result = read_sidecar(f.path());
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("EchoTime"), "Error should mention EchoTime: {}", err);
    }

    #[test]
    fn test_read_sidecar_missing_field_strength() {
        let f = write_json(r#"{"EchoTime": 0.02}"#);
        let result = read_sidecar(f.path());
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("MagneticFieldStrength"),
            "Error should mention MagneticFieldStrength: {}",
            err
        );
    }

    #[test]
    fn test_read_sidecar_invalid_json() {
        let f = write_json("not valid json {{{");
        let result = read_sidecar(f.path());
        assert!(result.is_err());
    }
}
