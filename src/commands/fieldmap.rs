//! Standalone B0 field mapping from multi-echo phase (NIfTI in, ppm out).
//!
//! Exposes the qsm-core field-mapping stage (`run_field_mapping`) directly, with
//! the full ROMEO + field-mapping parameter surface. Unlike `qsmxt unwrap` (a pure
//! single-volume unwrapper) this operates on 4D multi-echo phase and produces a B0
//! field map already in ppm.

use log::info;

use super::common::{load_mask, load_nifti_4d, save_nifti, MultiEcho};
use crate::cli::{
    B0EstimationArg, B0WeightTypeArg, FieldmapCommand, FieldmapCommonArgs,
};
use crate::error::QsmxtError;
use crate::pipeline::phase;

/// TE / B0 / geometry resolved from `--tes` / `--b0` / `--params`.
struct ScanParams {
    echo_times: Vec<f64>,
    field_strength: f64,
    voxel_size: Option<(f64, f64, f64)>,
    b0_direction: Option<(f64, f64, f64)>,
}

/// Minimal view of a `--params` JSON file (qsm-forward / qsm-ci style).
#[derive(serde::Deserialize, Default)]
struct ParamsJson {
    #[serde(rename = "TE")]
    te: Option<Vec<f64>>,
    #[serde(rename = "B0")]
    b0: Option<f64>,
    #[serde(rename = "voxel_size")]
    voxel_size: Option<Vec<f64>>,
    #[serde(rename = "B0_dir")]
    b0_dir: Option<Vec<f64>>,
}

fn resolve_scan_params(common: &FieldmapCommonArgs) -> crate::Result<ScanParams> {
    // Start from the JSON file (if any), then let explicit CLI flags override.
    let json: ParamsJson = if let Some(ref p) = common.params {
        let text = std::fs::read_to_string(p)
            .map_err(|e| QsmxtError::Config(format!("failed to read params file {}: {}", p.display(), e)))?;
        serde_json::from_str(&text)
            .map_err(|e| QsmxtError::Config(format!("failed to parse params file {}: {}", p.display(), e)))?
    } else {
        ParamsJson::default()
    };

    let echo_times = match (&common.tes, &json.te) {
        (Some(tes), _) => tes.clone(),
        (None, Some(te)) => te.clone(),
        (None, None) => {
            return Err(QsmxtError::Config(
                "no echo times: provide --tes <seconds...> or --params with a \"TE\" array".into(),
            ))
        }
    };
    if echo_times.is_empty() {
        return Err(QsmxtError::Config("echo times list is empty".into()));
    }

    let field_strength = common.b0.or(json.b0).ok_or_else(|| {
        QsmxtError::Config("no field strength: provide --b0/--field-strength or --params with \"B0\"".into())
    })?;

    let voxel_size = json.voxel_size.as_ref().and_then(|v| {
        if v.len() == 3 { Some((v[0], v[1], v[2])) } else { None }
    });
    let b0_direction = json.b0_dir.as_ref().and_then(|v| {
        if v.len() == 3 { Some((v[0], v[1], v[2])) } else { None }
    });

    Ok(ScanParams { echo_times, field_strength, voxel_size, b0_direction })
}

/// Map the shared CLI field-mapping flags onto the qsm-core config knobs.
fn apply_common_config(
    config: &mut qsm_core::pipeline::config::FieldMappingConfig,
    common: &FieldmapCommonArgs,
) {
    config.b0_estimation = match common.b0_estimation {
        B0EstimationArg::WeightedAvg => qsm_core::pipeline::config::B0EstimationMethod::WeightedAvg,
        B0EstimationArg::LinearFit => qsm_core::pipeline::config::B0EstimationMethod::LinearFit,
    };
    config.b0_weight_type = match common.b0_weight_type {
        B0WeightTypeArg::PhaseSNR => qsm_core::utils::B0WeightType::PhaseSNR,
        B0WeightTypeArg::PhaseVar => qsm_core::utils::B0WeightType::PhaseVar,
        B0WeightTypeArg::Average => qsm_core::utils::B0WeightType::Average,
        B0WeightTypeArg::TEs => qsm_core::utils::B0WeightType::TEs,
        B0WeightTypeArg::Mag => qsm_core::utils::B0WeightType::Mag,
    };
    if let Some(v) = common.linear_fit_reliability_threshold {
        config.linear_fit_params.reliability_threshold_percentile = v;
    }
    if let Some(v) = common.linear_fit_estimate_offset {
        config.linear_fit_params.estimate_offset = v;
    }
}

pub fn execute(cmd: FieldmapCommand) -> crate::Result<()> {
    // Dispatch per algorithm; the algorithm-specific config is applied via a closure so the shared
    // run_field_mapping does the loading/validation once.
    match cmd {
        FieldmapCommand::Romeo(args) => {
            let phase_offset_removal = args.phase_offset_removal;
            let phase_offset_sigma = args.phase_offset_sigma.clone();
            let bipolar_correction = args.bipolar_correction;
            let romeo_params = args.romeo_params.to_romeo_params();
            run_field_mapping(
                &args.common,
                qsm_core::pipeline::config::UnwrappingAlgorithm::Romeo,
                &|c| {
                    if let Some(v) = phase_offset_removal { c.phase_offset_removal = v; }
                    if let Some(ref s) = phase_offset_sigma {
                        if s.len() == 3 { c.phase_offset_sigma = [s[0], s[1], s[2]]; }
                    }
                    c.bipolar_correction = bipolar_correction;
                    c.romeo_params = romeo_params.clone();
                },
            )
        }
        FieldmapCommand::Laplacian(args) => run_field_mapping(
            &args.common,
            qsm_core::pipeline::config::UnwrappingAlgorithm::Laplacian,
            &|_| {},
        ),
    }
}

fn run_field_mapping(
    common: &FieldmapCommonArgs,
    algorithm: qsm_core::pipeline::config::UnwrappingAlgorithm,
    apply_algo: &dyn Fn(&mut qsm_core::pipeline::config::FieldMappingConfig),
) -> crate::Result<()> {
    let params = resolve_scan_params(common)?;

    // ── Load 4D phase ──
    let phase_me: MultiEcho = load_nifti_4d(&common.input)?;
    let (nx, ny, nz) = phase_me.dims;
    let n_voxels = nx * ny * nz;
    let n_echoes = phase_me.echoes.len();

    if params.echo_times.len() != n_echoes {
        return Err(QsmxtError::Config(format!(
            "echo-time count ({}) does not match number of phase volumes ({})",
            params.echo_times.len(),
            n_echoes
        )));
    }

    // Scale each echo into [-pi, pi] to be robust to unscaled radian/int inputs.
    let mut phases: Vec<Vec<f64>> = phase_me.echoes.clone();
    for p in phases.iter_mut() {
        phase::scale_phase_to_pi(p);
    }
    let phase_slices: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();

    // ── Optional 4D magnitude ──
    let magnitudes: Option<Vec<Vec<f64>>> = if let Some(ref mag_path) = common.magnitude {
        let mag_me = load_nifti_4d(mag_path)?;
        if mag_me.echoes.len() != n_echoes {
            return Err(QsmxtError::Config(format!(
                "magnitude echo count ({}) does not match phase echo count ({})",
                mag_me.echoes.len(),
                n_echoes
            )));
        }
        Some(mag_me.echoes)
    } else {
        None
    };
    let mag_slices: Option<Vec<&[f64]>> =
        magnitudes.as_ref().map(|m| m.iter().map(|v| v.as_slice()).collect());
    let mag_option: Option<&[&[f64]]> = mag_slices.as_deref();

    // ── Mask ──
    let (mask, _) = load_mask(&common.mask)?;
    if mask.len() != n_voxels {
        return Err(QsmxtError::Config(format!(
            "mask voxel count ({}) does not match phase volume ({})",
            mask.len(),
            n_voxels
        )));
    }

    // ── Scan metadata ──
    let voxel_size = params.voxel_size.unwrap_or(phase_me.voxel_size);
    let b0_direction = params
        .b0_direction
        .unwrap_or_else(|| phase::b0_direction_from_affine(&phase_me.affine));
    let scan_meta = qsm_core::pipeline::config::ScanMetadata {
        dims: (nx, ny, nz),
        voxel_size,
        echo_times: params.echo_times.clone(),
        field_strength: params.field_strength,
        b0_direction,
    };

    // ── Field-mapping config ──
    let mut config = qsm_core::pipeline::config::FieldMappingConfig {
        unwrapping_algorithm: algorithm,
        ..Default::default()
    };
    apply_common_config(&mut config, common);
    apply_algo(&mut config);

    info!(
        "Field mapping ({:?}, {}x{}x{}, {} echoes, B0={:.2}T)",
        algorithm, nx, ny, nz, n_echoes, params.field_strength
    );

    let result = qsm_core::pipeline::run_field_mapping(
        &phase_slices,
        mag_option,
        &mask,
        &scan_meta,
        &config,
        &mut |_, _| {},
    )
    .map_err(|e| QsmxtError::Config(format!("field mapping: {}", e)))?;

    // run_field_mapping already restricts to the mask, but multiply defensively so
    // out-of-brain voxels are exactly zero (matching the qsm-ci masked-ppm output).
    let mut field_ppm = result.b0_field_ppm;
    for (v, &m) in field_ppm.iter_mut().zip(mask.iter()) {
        if m == 0 {
            *v = 0.0;
        }
    }

    let reference = phase_me.geometry_reference();
    save_nifti(&common.output, &field_ppm, &reference)?;
    info!("B0 field map (ppm) saved to {}", common.output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{B0EstimationArg, B0WeightTypeArg, FieldmapCommand, FieldmapCommonArgs, FieldmapRomeoArgs};
    use std::f64::consts::PI;
    use std::io::Write;

    const NX: usize = 8;
    const NY: usize = 8;
    const NZ: usize = 8;

    /// Write a minimal 4D float32 NIfTI (dim[0]=4) with per-echo volumes.
    fn write_nifti_4d(path: &std::path::Path, echoes: &[Vec<f64>]) {
        let nt = echoes.len();
        let mut header = [0u8; 348];
        header[0..4].copy_from_slice(&348i32.to_le_bytes());
        let dim: [i16; 8] = [4, NX as i16, NY as i16, NZ as i16, nt as i16, 1, 1, 1];
        for (i, &d) in dim.iter().enumerate() {
            let off = 40 + i * 2;
            header[off..off + 2].copy_from_slice(&d.to_le_bytes());
        }
        header[70..72].copy_from_slice(&16i16.to_le_bytes()); // FLOAT32
        header[72..74].copy_from_slice(&32i16.to_le_bytes()); // bitpix
        let pixdim: [f32; 8] = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        for (i, &p) in pixdim.iter().enumerate() {
            let off = 76 + i * 4;
            header[off..off + 4].copy_from_slice(&p.to_le_bytes());
        }
        header[108..112].copy_from_slice(&352.0f32.to_le_bytes()); // vox_offset
        header[112..116].copy_from_slice(&1.0f32.to_le_bytes());   // scl_slope
        header[254..256].copy_from_slice(&1i16.to_le_bytes());     // sform_code
        // Identity srow with unit voxel scaling
        header[280..284].copy_from_slice(&1.0f32.to_le_bytes());
        header[296 + 4..296 + 8].copy_from_slice(&1.0f32.to_le_bytes());
        header[312 + 8..312 + 12].copy_from_slice(&1.0f32.to_le_bytes());
        header[344..348].copy_from_slice(b"n+1\0");

        let mut buf = Vec::new();
        buf.write_all(&header).unwrap();
        buf.write_all(&[0u8; 4]).unwrap();
        for echo in echoes {
            for &v in echo {
                buf.write_all(&(v as f32).to_le_bytes()).unwrap();
            }
        }
        std::fs::write(path, &buf).unwrap();
    }

    /// A two-echo linear-phase field should be recovered to a known ppm value.
    #[test]
    fn test_fieldmap_romeo_recovers_linear_field() {
        let dir = tempfile::tempdir().unwrap();
        let phase_path = dir.path().join("phase.nii");
        let mask_path = dir.path().join("mask.nii");
        let out_path = dir.path().join("field.nii");

        let n = NX * NY * NZ;
        let b0 = 3.0;
        let tes = [0.005f64, 0.010];
        let freq_hz = 40.0;
        // phase(rad) = 2π f TE (no wrapping for these small values)
        let echoes: Vec<Vec<f64>> = tes
            .iter()
            .map(|&te| vec![2.0 * PI * freq_hz * te; n])
            .collect();
        write_nifti_4d(&phase_path, &echoes);

        // Full-ones mask (as a 3D volume).
        let mask_data = vec![1.0f64; n];
        qsm_core::io::save_nifti_to_file(
            &mask_path, &mask_data, (NX, NY, NZ), (1.0, 1.0, 1.0),
            &[1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0],
        ).unwrap();

        let common = FieldmapCommonArgs {
            input: phase_path,
            mask: mask_path,
            output: out_path.clone(),
            magnitude: None,
            tes: Some(tes.to_vec()),
            b0: Some(b0),
            params: None,
            b0_estimation: B0EstimationArg::WeightedAvg,
            b0_weight_type: B0WeightTypeArg::PhaseSNR,
            linear_fit_reliability_threshold: None,
            linear_fit_estimate_offset: None,
        };
        let args = FieldmapRomeoArgs {
            common,
            // Disable offset removal so the constant field is preserved (ROMEO path B).
            phase_offset_removal: Some(false),
            phase_offset_sigma: None,
            bipolar_correction: false,
            romeo_params: Default::default(),
        };

        execute(FieldmapCommand::Romeo(args)).unwrap();

        let out = qsm_core::io::read_nifti_file(&out_path).unwrap();
        assert_eq!(out.data.len(), n);

        let gamma = 42.576e6;
        let expected_ppm = freq_hz * 1e6 / (gamma * b0);
        let mean: f64 = out.data.iter().sum::<f64>() / n as f64;
        assert!(
            (mean - expected_ppm).abs() < 0.01,
            "expected ~{:.4} ppm, got {:.4}",
            expected_ppm, mean
        );
    }

    /// Params JSON supplies TE and B0 when CLI flags are omitted.
    #[test]
    fn test_resolve_scan_params_from_json() {
        let dir = tempfile::tempdir().unwrap();
        let params_path = dir.path().join("params.json");
        std::fs::write(&params_path, r#"{"TE":[0.004,0.008,0.012],"B0":7.0}"#).unwrap();

        let common = FieldmapCommonArgs {
            input: dir.path().join("phase.nii"),
            mask: dir.path().join("mask.nii"),
            output: dir.path().join("out.nii"),
            magnitude: None,
            tes: None,
            b0: None,
            params: Some(params_path),
            b0_estimation: B0EstimationArg::WeightedAvg,
            b0_weight_type: B0WeightTypeArg::PhaseSNR,
            linear_fit_reliability_threshold: None,
            linear_fit_estimate_offset: None,
        };
        let p = resolve_scan_params(&common).unwrap();
        assert_eq!(p.echo_times, vec![0.004, 0.008, 0.012]);
        assert_eq!(p.field_strength, 7.0);
    }

    /// Echo-count / TE-count mismatch must error clearly.
    #[test]
    fn test_fieldmap_te_count_mismatch_errors() {
        let dir = tempfile::tempdir().unwrap();
        let phase_path = dir.path().join("phase.nii");
        let mask_path = dir.path().join("mask.nii");
        let n = NX * NY * NZ;
        write_nifti_4d(&phase_path, &[vec![0.0; n], vec![0.1; n]]); // 2 echoes
        let mask_data = vec![1.0f64; n];
        qsm_core::io::save_nifti_to_file(
            &mask_path, &mask_data, (NX, NY, NZ), (1.0, 1.0, 1.0),
            &[1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0],
        ).unwrap();

        let common = FieldmapCommonArgs {
            input: phase_path,
            mask: mask_path,
            output: dir.path().join("out.nii"),
            magnitude: None,
            tes: Some(vec![0.005]), // only 1 TE for 2 echoes
            b0: Some(3.0),
            params: None,
            b0_estimation: B0EstimationArg::WeightedAvg,
            b0_weight_type: B0WeightTypeArg::PhaseSNR,
            linear_fit_reliability_threshold: None,
            linear_fit_estimate_offset: None,
        };
        let args = FieldmapRomeoArgs {
            common,
            phase_offset_removal: Some(false),
            phase_offset_sigma: None,
            bipolar_correction: false,
            romeo_params: Default::default(),
        };
        assert!(execute(FieldmapCommand::Romeo(args)).is_err());
    }
}
