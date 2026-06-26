//! Synthetic NIfTI and BIDS data generators for integration tests.
//!
//! Creates tiny (8×8×8) volumes with deterministic data — just enough for
//! algorithms to run without crashing. We test orchestration, not accuracy.

#![cfg(test)]

use std::path::{Path, PathBuf};

const NX: usize = 8;
const NY: usize = 8;
const NZ: usize = 8;
const N: usize = NX * NY * NZ;

const IDENTITY_AFFINE: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

const VOXEL_SIZE: (f64, f64, f64) = (1.0, 1.0, 1.0);

/// Write a synthetic magnitude volume (positive values, brighter in centre).
pub fn write_magnitude(path: &Path) {
    let data = magnitude_data();
    qsm_core::nifti_io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write magnitude");
}

/// Write a synthetic phase volume (values in 0..4096 range, will be scaled to [-π, π]).
pub fn write_phase(path: &Path) {
    let data = phase_data();
    qsm_core::nifti_io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write phase");
}

/// Write a synthetic binary mask (1 inside, 0 at single-voxel border).
pub fn write_mask(path: &Path) {
    let data: Vec<f64> = mask_data().iter().map(|&m| m as f64).collect();
    qsm_core::nifti_io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write mask");
}

/// Write a synthetic field map (small f64 values simulating local field in ppm).
pub fn write_field(path: &Path) {
    let mut data = vec![0.0f64; N];
    let mask = mask_data();
    for i in 0..N {
        if mask[i] == 1 {
            data[i] = ((i as f64) * 0.001).sin() * 0.1;
        }
    }
    qsm_core::nifti_io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write field");
}

/// Write a JSON sidecar with echo time and field strength.
pub fn write_sidecar(path: &Path, echo_time: f64, field_strength: f64) {
    let json = serde_json::json!({
        "EchoTime": echo_time,
        "MagneticFieldStrength": field_strength,
    });
    std::fs::write(path, serde_json::to_string_pretty(&json).unwrap())
        .expect("write sidecar");
}

// --- BIDS directory builders ---

/// Minimal single-echo BIDS dataset (T2starw suffix, like the minimal example).
pub fn create_single_echo_bids(root: &Path) -> PathBuf {
    let anat = root.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();

    write_phase(&anat.join("sub-1_part-phase_T2starw.nii"));
    write_sidecar(&anat.join("sub-1_part-phase_T2starw.json"), 0.02, 3.0);
    write_magnitude(&anat.join("sub-1_part-mag_T2starw.nii"));
    write_sidecar(&anat.join("sub-1_part-mag_T2starw.json"), 0.02, 3.0);

    root.to_path_buf()
}

/// Multi-echo BIDS dataset (MEGRE suffix, 3 echoes, like the multi-echo example).
pub fn create_multi_echo_bids(root: &Path) -> PathBuf {
    let anat = root.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();

    let echo_times = [0.004, 0.008, 0.012];
    for (i, &te) in echo_times.iter().enumerate() {
        let echo_num = i + 1;
        write_phase(&anat.join(format!("sub-1_echo-{}_part-phase_MEGRE.nii", echo_num)));
        write_sidecar(&anat.join(format!("sub-1_echo-{}_part-phase_MEGRE.json", echo_num)), te, 3.0);
        write_magnitude(&anat.join(format!("sub-1_echo-{}_part-mag_MEGRE.nii", echo_num)));
        write_sidecar(&anat.join(format!("sub-1_echo-{}_part-mag_MEGRE.json", echo_num)), te, 3.0);
    }

    root.to_path_buf()
}

/// Multi-session BIDS dataset.
pub fn create_multi_session_bids(root: &Path) -> PathBuf {
    let ses1 = root.join("sub-1/ses-pre/anat");
    let ses2 = root.join("sub-1/ses-post/anat");
    std::fs::create_dir_all(&ses1).unwrap();
    std::fs::create_dir_all(&ses2).unwrap();

    for (anat, ses) in [(&ses1, "ses-pre"), (&ses2, "ses-post")] {
        write_phase(&anat.join(format!("sub-1_{}_part-phase_T2starw.nii", ses)));
        write_sidecar(&anat.join(format!("sub-1_{}_part-phase_T2starw.json", ses)), 0.02, 3.0);
        write_magnitude(&anat.join(format!("sub-1_{}_part-mag_T2starw.nii", ses)));
        write_sidecar(&anat.join(format!("sub-1_{}_part-mag_T2starw.json", ses)), 0.02, 3.0);
    }

    root.to_path_buf()
}

/// Multi-acquisition BIDS dataset (two acquisitions, each with 2 echoes).
#[allow(dead_code)]
pub fn create_multi_acq_bids(root: &Path) -> PathBuf {
    let anat = root.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();

    for acq in ["mygrea", "mygreb"] {
        for echo in 1..=2 {
            let te = echo as f64 * 0.004;
            let base = format!("sub-1_acq-{}_echo-{}", acq, echo);
            write_phase(&anat.join(format!("{}_part-phase_MEGRE.nii", base)));
            write_sidecar(&anat.join(format!("{}_part-phase_MEGRE.json", base)), te, 3.0);
            write_magnitude(&anat.join(format!("{}_part-mag_MEGRE.nii", base)));
            write_sidecar(&anat.join(format!("{}_part-mag_MEGRE.json", base)), te, 3.0);
        }
    }

    root.to_path_buf()
}

// --- Data generators ---

fn magnitude_data() -> Vec<f64> {
    let mut data = vec![0.0f64; N];
    let cx = NX as f64 / 2.0;
    let cy = NY as f64 / 2.0;
    let cz = NZ as f64 / 2.0;
    for z in 0..NZ {
        for y in 0..NY {
            for x in 0..NX {
                let dx = x as f64 - cx;
                let dy = y as f64 - cy;
                let dz = z as f64 - cz;
                let r = (dx * dx + dy * dy + dz * dz).sqrt();
                let idx = x + y * NX + z * NX * NY;
                data[idx] = (1000.0 * (1.0 - r / cx).max(0.0)) + 100.0;
            }
        }
    }
    data
}

fn phase_data() -> Vec<f64> {
    (0..N)
        .map(|i| ((i as f64 * 7.3) % 4096.0).abs())
        .collect()
}

fn mask_data() -> Vec<u8> {
    let mut mask = vec![0u8; N];
    // 1 everywhere except single-voxel border
    for z in 1..NZ - 1 {
        for y in 1..NY - 1 {
            for x in 1..NX - 1 {
                mask[x + y * NX + z * NX * NY] = 1;
            }
        }
    }
    mask
}
