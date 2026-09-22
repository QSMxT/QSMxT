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
    write_magnitude_decayed(path, 1.0);
}

/// The same volume scaled by `attenuation`, so a multi-echo series decays like a real one.
///
/// Identical echoes would fit R2* = 0 exactly, which makes every R2*/T2* assertion pass on a map
/// of zeros. A monoexponential decay gives the fit something to find.
pub fn write_magnitude_decayed(path: &Path, attenuation: f64) {
    let data: Vec<f64> = magnitude_data().iter().map(|v| v * attenuation).collect();
    qsm_core::io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write magnitude");
}

/// Write a synthetic phase volume (values in 0..4096 range, will be scaled to [-π, π]).
pub fn write_phase(path: &Path) {
    let data = phase_data();
    qsm_core::io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write phase");
}

/// Write a synthetic binary mask (1 inside, 0 at single-voxel border).
pub fn write_mask(path: &Path) {
    let data: Vec<f64> = mask_data().iter().map(|&m| m as f64).collect();
    qsm_core::io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write mask");
}

/// Write a binary mask that is 1 exactly where `keep(index)` says so, on the test grid.
/// For tests that need two masks whose overlap they control (e.g. `mask and` / `mask or`).
pub fn write_mask_where(path: &Path, keep: impl Fn(usize) -> bool) {
    let data: Vec<f64> = (0..N).map(|i| keep(i) as u8 as f64).collect();
    qsm_core::io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write mask");
}

/// Write a volume of `value` on a grid one slice shorter than the test grid — for checking that
/// operations reject inputs that are not on the same grid.
pub fn write_mismatched_volume(path: &Path, value: f64) {
    let data = vec![value; NX * NY * (NZ - 1)];
    qsm_core::io::save_nifti_to_file(path, &data, (NX, NY, NZ - 1), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write volume");
}

/// Voxel count of the synthetic test grid.
pub const N_VOXELS: usize = N;

/// Write a synthetic field map (small f64 values simulating local field in ppm).
pub fn write_field(path: &Path) {
    let mut data = vec![0.0f64; N];
    let mask = mask_data();
    for i in 0..N {
        if mask[i] == 1 {
            data[i] = ((i as f64) * 0.001).sin() * 0.1;
        }
    }
    qsm_core::io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write field");
}

/// Write a synthetic discrete segmentation on the test grid: a handful of real FreeSurfer ids,
/// each covering a slab of slices, with a background rim so label 0 is exercised too.
pub fn write_dseg(path: &Path) {
    // Left/right thalamus, caudate and putamen — ids SynthSeg's label table carries.
    const IDS: [f64; 6] = [10.0, 11.0, 12.0, 49.0, 50.0, 51.0];
    let mut data = vec![0.0f64; N];
    for z in 1..NZ - 1 {
        for y in 1..NY - 1 {
            for x in 1..NX - 1 {
                data[z * NY * NX + y * NX + x] = IDS[z % IDS.len()];
            }
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    qsm_core::io::save_nifti_to_file(path, &data, (NX, NY, NZ), VOXEL_SIZE, &IDENTITY_AFFINE)
        .expect("write dseg");
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

/// A sidecar that may declare `B0_dir` — the field that makes an orientation set
/// reconstructable when every affine is identical.
pub fn write_sidecar_with_b0(
    path: &Path, echo_time: f64, field_strength: f64, b0_dir: Option<(f64, f64, f64)>,
) {
    let mut json = serde_json::json!({
        "EchoTime": echo_time,
        "MagneticFieldStrength": field_strength,
    });
    if let Some((x, y, z)) = b0_dir {
        json["B0_dir"] = serde_json::json!([x, y, z]);
    }
    std::fs::write(path, serde_json::to_string_pretty(&json).unwrap()).expect("write sidecar");
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
    // A plausible grey-matter R2* at 3 T, so the echoes decay instead of repeating.
    const R2STAR_HZ: f64 = 30.0;
    for (i, &te) in echo_times.iter().enumerate() {
        let echo_num = i + 1;
        write_phase(&anat.join(format!("sub-1_echo-{}_part-phase_MEGRE.nii", echo_num)));
        write_sidecar(&anat.join(format!("sub-1_echo-{}_part-phase_MEGRE.json", echo_num)), te, 3.0);
        write_magnitude_decayed(
            &anat.join(format!("sub-1_echo-{}_part-mag_MEGRE.nii", echo_num)),
            (-R2STAR_HZ * te).exp());
        write_sidecar(&anat.join(format!("sub-1_echo-{}_part-mag_MEGRE.json", echo_num)), te, 3.0);
    }

    root.to_path_buf()
}

/// Three-orientation BIDS dataset in the shape real COSMOS data ships in: `acq-dir1/2/3`
/// on one shared grid with a declared `B0_dir`, plus an unrelated `acq-highres` in the same
/// session that a grouping pattern must not sweep in.
pub fn create_multi_orientation_bids(root: &Path) -> PathBuf {
    let anat = root.join("sub-1/ses-1/anat");
    std::fs::create_dir_all(&anat).unwrap();

    let dirs = [(0.0, 0.0, 1.0), (0.0, 0.5, 0.866), (0.5, 0.0, 0.866)];
    for (i, &b0) in dirs.iter().enumerate() {
        let acq = format!("acq-dir{}", i + 1);
        write_phase(&anat.join(format!("sub-1_ses-1_{acq}_echo-1_part-phase_MEGRE.nii")));
        write_sidecar_with_b0(
            &anat.join(format!("sub-1_ses-1_{acq}_echo-1_part-phase_MEGRE.json")), 0.012, 3.0, Some(b0),
        );
        write_magnitude(&anat.join(format!("sub-1_ses-1_{acq}_echo-1_part-mag_MEGRE.nii")));
        write_sidecar_with_b0(
            &anat.join(format!("sub-1_ses-1_{acq}_echo-1_part-mag_MEGRE.json")), 0.012, 3.0, Some(b0),
        );
    }

    write_phase(&anat.join("sub-1_ses-1_acq-highres_echo-1_part-phase_MEGRE.nii"));
    write_sidecar(&anat.join("sub-1_ses-1_acq-highres_echo-1_part-phase_MEGRE.json"), 0.012, 3.0);
    write_magnitude(&anat.join("sub-1_ses-1_acq-highres_echo-1_part-mag_MEGRE.nii"));
    write_sidecar(&anat.join("sub-1_ses-1_acq-highres_echo-1_part-mag_MEGRE.json"), 0.012, 3.0);

    root.to_path_buf()
}

/// Uncombined multi-coil BIDS dataset (MEGRE, 2 echoes, `n_coils` channels with the
/// `rec-uncombined_coil-NN` entities) plus the scanner-combined echoes of the same acquisition —
/// the layout `qsmxt dicom-convert` produces for a Siemens "save uncombined" SWI export.
pub fn create_multi_coil_bids(root: &Path, n_coils: u32) -> PathBuf {
    let anat = root.join("sub-1/ses-1/anat");
    std::fs::create_dir_all(&anat).unwrap();
    let echo_times = [0.009, 0.020];
    for (i, &te) in echo_times.iter().enumerate() {
        let echo = i + 1;
        for part in ["phase", "mag"] {
            let base = format!("sub-1_ses-1_acq-swi_echo-{}_part-{}_MEGRE", echo, part);
            if part == "phase" { write_phase(&anat.join(format!("{}.nii", base))); } else { write_magnitude(&anat.join(format!("{}.nii", base))); }
            write_sidecar(&anat.join(format!("{}.json", base)), te, 3.0);
            for coil in 1..=n_coils {
                let base = format!("sub-1_ses-1_acq-swi_rec-uncombined_coil-{:02}_echo-{}_part-{}_MEGRE", coil, echo, part);
                if part == "phase" { write_phase(&anat.join(format!("{}.nii", base))); } else { write_magnitude(&anat.join(format!("{}.nii", base))); }
                write_sidecar(&anat.join(format!("{}.json", base)), te, 3.0);
            }
        }
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

/// Synthetic multi-orientation data: a susceptibility phantom forward-projected through the
/// dipole kernel at several B0 directions.
///
/// Unlike the 8³ fixtures above, these are big enough that a reconstruction can be checked
/// for *accuracy*, not just for running without crashing — the whole point of COSMOS is that
/// combining orientations recovers what one orientation cannot, and a test that never looks
/// at the numbers would not notice if the orientations were being ignored.
pub mod multiorient {
    use std::path::Path;

    pub const MO_N: usize = 24;
    pub const MO_DIMS: (usize, usize, usize) = (MO_N, MO_N, MO_N);
    pub const MO_VOXEL: (f64, f64, f64) = (1.0, 1.0, 1.0);
    pub const MO_AFFINE: [f64; 16] = super::IDENTITY_AFFINE;

    pub fn grid() -> qsm_core::Grid {
        qsm_core::Grid::new(MO_N, MO_N, MO_N, MO_VOXEL.0, MO_VOXEL.1, MO_VOXEL.2)
    }

    /// Three spheres of different susceptibility inside a spherical "brain".
    pub fn chi_phantom() -> Vec<f64> {
        let c = MO_N as f64 / 2.0;
        let mut chi = vec![0.0; MO_N * MO_N * MO_N];
        let blobs = [
            ((c - 4.0, c, c), 3.0, 0.15),
            ((c + 4.0, c + 2.0, c), 2.5, -0.10),
            ((c, c - 4.0, c + 3.0), 2.0, 0.20),
        ];
        for z in 0..MO_N {
            for y in 0..MO_N {
                for x in 0..MO_N {
                    let p = (x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5);
                    for &((bx, by, bz), r, value) in &blobs {
                        let d = ((p.0 - bx).powi(2) + (p.1 - by).powi(2) + (p.2 - bz).powi(2)).sqrt();
                        if d < r {
                            chi[x + y * MO_N + z * MO_N * MO_N] = value;
                        }
                    }
                }
            }
        }
        chi
    }

    /// A spherical mask well inside the volume, so the reconstruction is scored away from the
    /// FFT wrap-around at the edges.
    pub fn mask() -> Vec<u8> {
        let c = MO_N as f64 / 2.0;
        let mut m = vec![0u8; MO_N * MO_N * MO_N];
        for z in 0..MO_N {
            for y in 0..MO_N {
                for x in 0..MO_N {
                    let d = ((x as f64 + 0.5 - c).powi(2)
                        + (y as f64 + 0.5 - c).powi(2)
                        + (z as f64 + 0.5 - c).powi(2))
                    .sqrt();
                    if d < c - 4.0 {
                        m[x + y * MO_N + z * MO_N * MO_N] = 1;
                    }
                }
            }
        }
        m
    }

    /// Forward-project `chi` through the dipole kernel for one B0 direction.
    pub fn forward_field(chi: &[f64], bdir: (f64, f64, f64)) -> Vec<f64> {
        let kernel = qsm_core::kernels::dipole::dipole_kernel(&grid(), bdir);
        qsm_core::fft::apply_real_kernel(chi, &kernel, MO_N, MO_N, MO_N)
    }

    pub fn write(path: &Path, data: &[f64]) {
        qsm_core::io::save_nifti_to_file(path, data, MO_DIMS, MO_VOXEL, &MO_AFFINE)
            .expect("write multi-orientation volume");
    }

    pub fn write_mask(path: &Path) {
        let data: Vec<f64> = mask().iter().map(|&m| m as f64).collect();
        write(path, &data);
    }

    /// Normalised RMS error between `got` and `want` inside `mask`.
    pub fn nrmse(got: &[f64], want: &[f64], mask: &[u8]) -> f64 {
        let (mut num, mut den) = (0.0, 0.0);
        for i in 0..want.len() {
            if mask[i] == 1 {
                num += (got[i] - want[i]).powi(2);
                den += want[i].powi(2);
            }
        }
        (num / den.max(1e-30)).sqrt()
    }
}
