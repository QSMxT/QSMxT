use std::fs;
use std::path::{Path, PathBuf};

use crate::dicom::convert::{nifti_4d_size, nii_to_json_path};

/// What type of data a NIfTI file represents (mag vs phase).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NiftiPartType {
    Magnitude,
    Phase,
}

/// Metadata extracted from a NIfTI JSON sidecar (all fields optional).
#[derive(Debug, Clone)]
pub struct NiftiSidecarInfo {
    pub echo_time: Option<f64>,
    pub field_strength: Option<f64>,
    pub b0_dir: Option<Vec<f64>>,
    pub image_type: Option<NiftiPartType>,
}

/// Parameters for converting NIfTI files to BIDS.
pub struct NiftiToBidsParams {
    pub magnitude_files: Vec<PathBuf>,
    pub phase_files: Vec<PathBuf>,
    pub echo_times_s: Vec<f64>,
    pub field_strength: f64,
    pub b0_dir: Vec<f64>,
    pub output_dir: PathBuf,
}

/// Result of scanning a directory for NIfTI files.
pub struct NiftiScanResult {
    /// Magnitude files sorted by echo time.
    pub magnitude_files: Vec<PathBuf>,
    /// Phase files sorted by echo time.
    pub phase_files: Vec<PathBuf>,
    /// Echo times in seconds (from magnitude sidecars).
    pub echo_times_s: Vec<f64>,
    /// Field strength if found in any sidecar.
    pub field_strength: Option<f64>,
    /// B0 direction if found in any sidecar.
    pub b0_dir: Option<Vec<f64>>,
    /// Files that could not be classified.
    pub unclassified: Vec<PathBuf>,
}

/// Read a NIfTI JSON sidecar permissively — all fields optional.
pub fn read_nifti_sidecar(json_path: &Path) -> Option<NiftiSidecarInfo> {
    let text = fs::read_to_string(json_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;

    let echo_time = value.get("EchoTime").and_then(|v| v.as_f64());
    let field_strength = value.get("MagneticFieldStrength").and_then(|v| v.as_f64());
    let b0_dir = value.get("B0_dir").and_then(|v| {
        v.as_array().map(|arr| arr.iter().filter_map(|x| x.as_f64()).collect())
    });

    let image_type = value.get("ImageType").and_then(|v| {
        v.as_array().and_then(|arr| {
            let types: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_uppercase()))
                .collect();
            if types.iter().any(|t| t == "P" || t.contains("PHASE")) {
                Some(NiftiPartType::Phase)
            } else if types.iter().any(|t| t == "M" || t.contains("MAGNITUDE")) {
                Some(NiftiPartType::Magnitude)
            } else {
                None
            }
        })
    });

    Some(NiftiSidecarInfo {
        echo_time,
        field_strength,
        b0_dir,
        image_type,
    })
}

/// Scan a directory for NIfTI files and auto-classify using JSON sidecars.
pub fn scan_nifti_directory(dir: &Path) -> NiftiScanResult {
    let mut mag_entries: Vec<(PathBuf, f64)> = Vec::new();
    let mut phase_entries: Vec<(PathBuf, f64)> = Vec::new();
    let mut unclassified: Vec<PathBuf> = Vec::new();
    let mut field_strength: Option<f64> = None;
    let mut b0_dir: Option<Vec<f64>> = None;

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            return NiftiScanResult {
                magnitude_files: vec![],
                phase_files: vec![],
                echo_times_s: vec![],
                field_strength: None,
                b0_dir: None,
                unclassified: vec![],
            };
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".nii.gz") && !name.ends_with(".nii") {
            continue;
        }

        let json_path = nii_to_json_path(&path);
        if let Some(info) = read_nifti_sidecar(&json_path) {
            let et = info.echo_time.unwrap_or(0.0);
            if field_strength.is_none() {
                field_strength = info.field_strength;
            }
            if b0_dir.is_none() {
                b0_dir = info.b0_dir;
            }
            match info.image_type {
                Some(NiftiPartType::Magnitude) => mag_entries.push((path, et)),
                Some(NiftiPartType::Phase) => phase_entries.push((path, et)),
                None => unclassified.push(path),
            }
        } else {
            unclassified.push(path);
        }
    }

    // Sort by echo time
    mag_entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    phase_entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let echo_times_s: Vec<f64> = mag_entries.iter().map(|(_, et)| *et).collect();
    let magnitude_files = mag_entries.into_iter().map(|(p, _)| p).collect();
    let phase_files = phase_entries.into_iter().map(|(p, _)| p).collect();

    NiftiScanResult {
        magnitude_files,
        phase_files,
        echo_times_s,
        field_strength,
        b0_dir,
        unclassified,
    }
}

/// Split a 4D NIfTI file into separate 3D volumes.
/// Returns the paths of the generated 3D files.
pub fn split_4d_nifti(
    path: &Path,
    output_dir: &Path,
    base_name: &str,
) -> crate::Result<Vec<PathBuf>> {
    let bytes = fs::read(path).map_err(|e| {
        crate::error::QsmxtError::NiftiIo(format!("{}: {}", path.display(), e))
    })?;

    let (data, (nx, ny, nz, nt), voxel_size, affine) =
        qsm_core::nifti_io::load_nifti_4d(&bytes).map_err(crate::error::QsmxtError::NiftiIo)?;

    let vol_size = nx * ny * nz;
    let mut paths = Vec::with_capacity(nt);

    for t in 0..nt {
        let start = t * vol_size;
        let end = start + vol_size;
        let vol_data = &data[start..end];

        let out_path = output_dir.join(format!("{}_echo-{}.nii.gz", base_name, t + 1));
        qsm_core::nifti_io::save_nifti_to_file(
            &out_path,
            vol_data,
            (nx, ny, nz),
            voxel_size,
            &affine,
        )
        .map_err(crate::error::QsmxtError::NiftiIo)?;

        paths.push(out_path);
    }

    Ok(paths)
}

/// Convert NIfTI files to a BIDS directory structure.
///
/// Creates `output_dir/sub-01/anat/` with properly named files and JSON sidecars.
/// If any input file is 4D, it is split into separate 3D volumes first.
pub fn convert_to_bids(params: &NiftiToBidsParams) -> crate::Result<PathBuf> {
    let anat_dir = params.output_dir.join("sub-01").join("anat");
    fs::create_dir_all(&anat_dir)?;

    let n_mag = process_files(
        &params.magnitude_files,
        "mag",
        &anat_dir,
        &params.echo_times_s,
        params.field_strength,
        &params.b0_dir,
    )?;

    let n_phase = process_files(
        &params.phase_files,
        "phase",
        &anat_dir,
        &params.echo_times_s,
        params.field_strength,
        &params.b0_dir,
    )?;

    if n_mag != n_phase {
        log::warn!(
            "Magnitude file count ({}) differs from phase file count ({})",
            n_mag,
            n_phase
        );
    }

    Ok(params.output_dir.clone())
}

/// Process a list of files (mag or phase), handling 4D splitting, copying,
/// and generating BIDS-named files with JSON sidecars.
/// Returns the total number of echo files written.
fn process_files(
    files: &[PathBuf],
    part: &str,
    anat_dir: &Path,
    echo_times_s: &[f64],
    field_strength: f64,
    b0_dir: &[f64],
) -> crate::Result<usize> {
    let mut echo_idx = 0usize;

    for file in files {
        // Check if 4D
        if let Some(n_vols) = nifti_4d_size(file) {
            // Split into separate volumes in a temp location, then move
            let split_paths = split_4d_nifti(file, anat_dir, &format!("_tmp_split_{}", part))?;
            for split_path in &split_paths {
                echo_idx += 1;
                let bids_name = bids_filename(echo_idx, part);
                let dest = anat_dir.join(&bids_name);
                fs::rename(split_path, &dest)?;
                write_sidecar(
                    &dest,
                    echo_times_s.get(echo_idx - 1).copied(),
                    field_strength,
                    b0_dir,
                )?;
            }
            log::info!(
                "Split 4D file {} into {} volumes",
                file.display(),
                n_vols
            );
        } else {
            echo_idx += 1;
            let bids_name = bids_filename(echo_idx, part);
            let dest = anat_dir.join(&bids_name);
            fs::copy(file, &dest)?;
            write_sidecar(
                &dest,
                echo_times_s.get(echo_idx - 1).copied(),
                field_strength,
                b0_dir,
            )?;
        }
    }

    Ok(echo_idx)
}

/// Generate a BIDS filename for a given echo and part.
fn bids_filename(echo: usize, part: &str) -> String {
    format!(
        "sub-01_acq-nifti_echo-{}_part-{}_{}.nii.gz",
        echo, part, "MEGRE"
    )
}

/// Write a JSON sidecar next to a NIfTI file.
fn write_sidecar(
    nii_path: &Path,
    echo_time_s: Option<f64>,
    field_strength: f64,
    b0_dir: &[f64],
) -> crate::Result<()> {
    let json_path = nii_to_json_path(nii_path);

    let mut map = serde_json::Map::new();
    if let Some(et) = echo_time_s {
        map.insert(
            "EchoTime".to_string(),
            serde_json::Value::from(et),
        );
    }
    map.insert(
        "MagneticFieldStrength".to_string(),
        serde_json::Value::from(field_strength),
    );
    if !b0_dir.is_empty() {
        map.insert(
            "B0_dir".to_string(),
            serde_json::Value::Array(b0_dir.iter().map(|&v| serde_json::Value::from(v)).collect()),
        );
    }

    let json = serde_json::to_string_pretty(&map)
        .map_err(|e| crate::error::QsmxtError::Config(format!("JSON serialization error: {}", e)))?;
    fs::write(&json_path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bids_filename() {
        assert_eq!(
            bids_filename(1, "mag"),
            "sub-01_acq-nifti_echo-1_part-mag_MEGRE.nii.gz"
        );
        assert_eq!(
            bids_filename(3, "phase"),
            "sub-01_acq-nifti_echo-3_part-phase_MEGRE.nii.gz"
        );
    }

    #[test]
    fn test_read_nifti_sidecar_full() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("test.json");
        fs::write(
            &json_path,
            r#"{
                "EchoTime": 0.02,
                "MagneticFieldStrength": 3.0,
                "B0_dir": [0.0, 0.0, 1.0],
                "ImageType": ["ORIGINAL", "PRIMARY", "M", "ND"]
            }"#,
        )
        .unwrap();

        let info = read_nifti_sidecar(&json_path).unwrap();
        assert!((info.echo_time.unwrap() - 0.02).abs() < 1e-10);
        assert!((info.field_strength.unwrap() - 3.0).abs() < 1e-10);
        assert_eq!(info.b0_dir.unwrap(), vec![0.0, 0.0, 1.0]);
        assert_eq!(info.image_type, Some(NiftiPartType::Magnitude));
    }

    #[test]
    fn test_read_nifti_sidecar_phase() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("test.json");
        fs::write(
            &json_path,
            r#"{"ImageType": ["ORIGINAL", "PRIMARY", "P", "ND"]}"#,
        )
        .unwrap();

        let info = read_nifti_sidecar(&json_path).unwrap();
        assert_eq!(info.image_type, Some(NiftiPartType::Phase));
        assert!(info.echo_time.is_none());
    }

    #[test]
    fn test_read_nifti_sidecar_missing() {
        let result = read_nifti_sidecar(Path::new("/nonexistent/path.json"));
        assert!(result.is_none());
    }

    #[test]
    fn test_write_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let nii_path = dir.path().join("test.nii.gz");
        fs::write(&nii_path, b"dummy").unwrap();

        write_sidecar(&nii_path, Some(0.02), 3.0, &[0.0, 0.0, 1.0]).unwrap();

        let json_path = dir.path().join("test.json");
        let text = fs::read_to_string(&json_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!((value["EchoTime"].as_f64().unwrap() - 0.02).abs() < 1e-10);
        assert!((value["MagneticFieldStrength"].as_f64().unwrap() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_write_sidecar_no_echo_time() {
        let dir = tempfile::tempdir().unwrap();
        let nii_path = dir.path().join("no_echo.nii.gz");
        fs::write(&nii_path, b"dummy").unwrap();

        write_sidecar(&nii_path, None, 7.0, &[0.0, 0.0, 1.0]).unwrap();

        let json_path = dir.path().join("no_echo.json");
        let text = fs::read_to_string(&json_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(value.get("EchoTime").is_none());
        assert!((value["MagneticFieldStrength"].as_f64().unwrap() - 7.0).abs() < 1e-10);
        assert_eq!(
            value["B0_dir"].as_array().unwrap().len(),
            3
        );
    }

    #[test]
    fn test_write_sidecar_empty_b0_dir() {
        let dir = tempfile::tempdir().unwrap();
        let nii_path = dir.path().join("no_b0.nii.gz");
        fs::write(&nii_path, b"dummy").unwrap();

        write_sidecar(&nii_path, Some(0.01), 3.0, &[]).unwrap();

        let json_path = dir.path().join("no_b0.json");
        let text = fs::read_to_string(&json_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(value.get("B0_dir").is_none());
        assert!((value["EchoTime"].as_f64().unwrap() - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_read_nifti_sidecar_phase_image_type() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("phase_test.json");
        fs::write(
            &json_path,
            r#"{
                "EchoTime": 0.005,
                "MagneticFieldStrength": 7.0,
                "ImageType": ["ORIGINAL", "PRIMARY", "PHASE"]
            }"#,
        )
        .unwrap();

        let info = read_nifti_sidecar(&json_path).unwrap();
        assert_eq!(info.image_type, Some(NiftiPartType::Phase));
        assert!((info.echo_time.unwrap() - 0.005).abs() < 1e-10);
        assert!((info.field_strength.unwrap() - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_scan_nifti_directory_classifies_mag_and_phase() {
        let dir = tempfile::tempdir().unwrap();

        // Create two magnitude NIfTI files with JSON sidecars
        let mag1 = dir.path().join("mag_echo1.nii.gz");
        let mag2 = dir.path().join("mag_echo2.nii.gz");
        crate::testutils::write_magnitude(&mag1);
        crate::testutils::write_magnitude(&mag2);
        fs::write(
            dir.path().join("mag_echo1.json"),
            r#"{"EchoTime": 0.008, "MagneticFieldStrength": 3.0, "B0_dir": [0.0, 0.0, 1.0], "ImageType": ["ORIGINAL", "PRIMARY", "M"]}"#,
        ).unwrap();
        fs::write(
            dir.path().join("mag_echo2.json"),
            r#"{"EchoTime": 0.004, "MagneticFieldStrength": 3.0, "ImageType": ["ORIGINAL", "PRIMARY", "MAGNITUDE"]}"#,
        ).unwrap();

        // Create one phase NIfTI file with JSON sidecar
        let phase1 = dir.path().join("phase_echo1.nii.gz");
        crate::testutils::write_phase(&phase1);
        fs::write(
            dir.path().join("phase_echo1.json"),
            r#"{"EchoTime": 0.004, "ImageType": ["ORIGINAL", "PRIMARY", "P"]}"#,
        ).unwrap();

        let result = scan_nifti_directory(dir.path());
        assert_eq!(result.magnitude_files.len(), 2);
        assert_eq!(result.phase_files.len(), 1);
        assert_eq!(result.unclassified.len(), 0);
        // Magnitude files should be sorted by echo time: 0.004 first, then 0.008
        assert_eq!(result.echo_times_s, vec![0.004, 0.008]);
        assert!((result.field_strength.unwrap() - 3.0).abs() < 1e-10);
        assert_eq!(result.b0_dir.unwrap(), vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_scan_nifti_directory_nonexistent() {
        let result = scan_nifti_directory(Path::new("/nonexistent/path/to/nifti"));
        assert!(result.magnitude_files.is_empty());
        assert!(result.phase_files.is_empty());
        assert!(result.echo_times_s.is_empty());
        assert!(result.field_strength.is_none());
        assert!(result.b0_dir.is_none());
        assert!(result.unclassified.is_empty());
    }

    #[test]
    fn test_scan_nifti_directory_no_json_sidecar() {
        let dir = tempfile::tempdir().unwrap();

        // Create NIfTI files without JSON sidecars
        let nii1 = dir.path().join("orphan1.nii.gz");
        let nii2 = dir.path().join("orphan2.nii.gz");
        crate::testutils::write_magnitude(&nii1);
        crate::testutils::write_phase(&nii2);

        let result = scan_nifti_directory(dir.path());
        assert!(result.magnitude_files.is_empty());
        assert!(result.phase_files.is_empty());
        assert_eq!(result.unclassified.len(), 2);
    }

    #[test]
    fn test_convert_to_bids() {
        let dir = tempfile::tempdir().unwrap();
        let input_dir = dir.path().join("input");
        let output_dir = dir.path().join("output");
        fs::create_dir_all(&input_dir).unwrap();

        // Create magnitude and phase input files
        let mag_path = input_dir.join("mag.nii.gz");
        let phase_path = input_dir.join("phase.nii.gz");
        crate::testutils::write_magnitude(&mag_path);
        crate::testutils::write_phase(&phase_path);

        let params = NiftiToBidsParams {
            magnitude_files: vec![mag_path],
            phase_files: vec![phase_path],
            echo_times_s: vec![0.02],
            field_strength: 3.0,
            b0_dir: vec![0.0, 0.0, 1.0],
            output_dir: output_dir.clone(),
        };

        let result = convert_to_bids(&params).unwrap();
        assert_eq!(result, output_dir);

        let anat_dir = output_dir.join("sub-01").join("anat");
        assert!(anat_dir.exists());

        // Check magnitude file and sidecar
        let mag_bids = anat_dir.join("sub-01_acq-nifti_echo-1_part-mag_MEGRE.nii.gz");
        assert!(mag_bids.exists(), "Magnitude BIDS file should exist");
        let mag_json = anat_dir.join("sub-01_acq-nifti_echo-1_part-mag_MEGRE.json");
        assert!(mag_json.exists(), "Magnitude JSON sidecar should exist");

        // Check phase file and sidecar
        let phase_bids = anat_dir.join("sub-01_acq-nifti_echo-1_part-phase_MEGRE.nii.gz");
        assert!(phase_bids.exists(), "Phase BIDS file should exist");
        let phase_json = anat_dir.join("sub-01_acq-nifti_echo-1_part-phase_MEGRE.json");
        assert!(phase_json.exists(), "Phase JSON sidecar should exist");

        // Verify sidecar content
        let text = fs::read_to_string(&mag_json).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!((value["EchoTime"].as_f64().unwrap() - 0.02).abs() < 1e-10);
        assert!((value["MagneticFieldStrength"].as_f64().unwrap() - 3.0).abs() < 1e-10);
        assert_eq!(
            value["B0_dir"].as_array().unwrap().len(),
            3
        );
    }
}
