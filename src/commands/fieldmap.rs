//! Standalone B0 field mapping from multi-echo phase (NIfTI in, ppm out).
//!
//! Exposes the field-mapping stage (`run_field_mapping`) directly, with the field-mapping
//! settings' `qsmxt run` flags. Unlike `qsmxt unwrap` (a pure
//! single-volume unwrapper) this operates on 4D multi-echo phase and produces a B0
//! field map already in ppm.

use log::info;

use super::common::{load_mask, load_nifti_4d, save_nifti, MultiEcho};
use crate::cli::{FieldmapCommand, FieldmapCommonArgs};
use qsmxt_io::error::QsmxtError;

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

fn resolve_scan_params(common: &FieldmapCommonArgs) -> qsmxt_io::Result<ScanParams> {
    // Start from the JSON file (if any), then let explicit CLI flags override.
    let json: ParamsJson = if let Some(ref p) = common.params {
        let text = fs_err::read_to_string(p)
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

pub fn execute(cmd: FieldmapCommand) -> qsmxt_io::Result<()> {
    use qsmxt_engine::UnwrappingAlgorithm as U;
    match cmd {
        FieldmapCommand::Romeo(args) => run_field_mapping(&args.common, U::Romeo, &args.settings.0),
        FieldmapCommand::Laplacian(args) => run_field_mapping(&args.common, U::Laplacian, &args.settings.0),
        FieldmapCommand::Prelude(args) => run_field_mapping(&args.common, U::Prelude, &args.settings.0),
        FieldmapCommand::MediRegionGrowing(args) => {
            if args.common.magnitude.is_none() {
                return Err(QsmxtError::Config(
                    "fieldmap medi-region-growing needs --magnitude: MEDI's region growing unwraps in \
                     order of the magnitude".into()));
            }
            run_field_mapping(&args.common, U::MediRegionGrowing, &[])
        }
    }
}

/// QSMxT's configuration with `algorithm` as the unwrapping, and the common flags and the
/// method's (`method`) applied, as `qsmxt run` applies them. The field-mapping stage is the one
/// `qsmxt run` builds from it ([`qsmxt_engine::to_pipeline_stages`]), so the standalone tool
/// gives the field map the pipeline gives for the same inputs and flags (#279).
fn config(
    common: &FieldmapCommonArgs, algorithm: qsmxt_engine::UnwrappingAlgorithm,
    method: &[qsmxt_engine::schema::cli_apply::Given],
) -> qsmxt_io::Result<qsmxt_engine::PipelineConfig> {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.field_mapping.unwrapping_algorithm = algorithm;
    qsmxt_io::settings::apply(&mut cfg, &[common.settings.0.as_slice(), method].concat())?;
    Ok(cfg)
}

fn run_field_mapping(
    common: &FieldmapCommonArgs, algorithm: qsmxt_engine::UnwrappingAlgorithm,
    method: &[qsmxt_engine::schema::cli_apply::Given],
) -> qsmxt_io::Result<()> {
    let params = resolve_scan_params(common)?;
    let cfg = config(common, algorithm, method)?;
    if (common.complex_fit_output.is_some() || common.complex_fit_noise_output.is_some())
        && cfg.field_mapping.b0_estimation != qsmxt_engine::B0Estimation::ComplexFit
    {
        return Err(QsmxtError::Config(
            "--complex-fit-output and --complex-fit-noise-output need --b0-estimation complex-fit".into()));
    }

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

    // Into radians in [-pi, pi], with one conversion for every echo (scale_phases_to_pi).
    let mut phases: Vec<Vec<f64>> = phase_me.echoes.clone();
    qsm_core::pipeline::scale_phases_to_pi(&mut phases);
    let phase_slices: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();

    // ── Optional 4D magnitude ──
    let magnitudes = load_magnitudes(common, n_echoes)?;
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
    let scan_meta = scan_metadata(&params, &phase_me);

    let config = qsmxt_engine::to_pipeline_stages(&cfg).0;

    info!(
        "Field mapping ({:?}, {}x{}x{}, {} echoes, B0={:.2}T)",
        algorithm, nx, ny, nz, n_echoes, params.field_strength
    );

    let field = qsmxt_engine::fieldmap::run_field_mapping(
        &phase_slices,
        mag_option,
        &mask,
        &scan_meta,
        &config,
    )
    .map_err(|e| QsmxtError::Config(format!("field mapping: {}", e)))?;
    if let Some(report) = &field.robust_fit {
        qsmxt_engine::fieldmap::log_robust_fit_report(report);
    }
    if let Some(t) = field.qsmart_fit_threshold {
        qsmxt_engine::fieldmap::log_qsmart_fit_threshold(t);
    }
    let mut field_ppm = field.b0_field_ppm;
    // Where the field is defined: the mask less the voxels phase offset removal could not
    // estimate an offset for (qsm-core's `valid_mask`).
    qsmxt_engine::fieldmap::log_field_mask(&mask, &field.valid_mask);

    // run_field_mapping already restricts to that mask, but multiply defensively so the voxels
    // outside it are exactly zero (matching the qsm-ci masked-ppm output).
    for (v, &m) in field_ppm.iter_mut().zip(field.valid_mask.iter()) {
        if m == 0 {
            *v = 0.0;
        }
    }

    let reference = phase_me.geometry_reference();
    save_nifti(&common.output, &field_ppm, &reference)?;
    info!("B0 field map (ppm) saved to {}", common.output.display());
    save_complex_fit(common, field.complex_fit.as_ref(), &reference)?;
    if let Some(path) = &common.reliability_output {
        let reliability = field.reliability.as_ref().ok_or_else(|| QsmxtError::Config(
            "--reliability-output: only --b0-estimation qsmart-echofit makes a reliability map".into()))?;
        super::common::save_mask(path, reliability, &reference)?;
        info!("Echo-fit reliability map saved to {}", path.display());
    }
    if let Some(path) = &common.field_mask_output {
        super::common::save_mask(path, &field.valid_mask, &reference)?;
        info!("Field mask saved to {}", path.display());
    }
    Ok(())
}

/// The optional 4D magnitude, one volume per phase echo.
fn load_magnitudes(common: &FieldmapCommonArgs, n_echoes: usize) -> qsmxt_io::Result<Option<Vec<Vec<f64>>>> {
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
    Ok(magnitudes)
}

/// The scan metadata from the resolved scan parameters, with the phase's header for what they
/// leave out.
fn scan_metadata(params: &ScanParams, phase_me: &MultiEcho) -> qsm_core::pipeline::config::ScanMetadata {
    let voxel_size = params.voxel_size.unwrap_or(phase_me.voxel_size);
    let b0_direction = params
        .b0_direction
        .unwrap_or_else(|| qsm_core::geometry::b0_direction_from_affine(&phase_me.affine));
    qsm_core::pipeline::config::ScanMetadata {
        dims: phase_me.dims,
        voxel_size,
        echo_times: params.echo_times.clone(),
        field_strength: params.field_strength,
        b0_direction,
        slice_geometry: None,
        affine: Some(phase_me.affine),
    }
}

/// `--complex-fit-output` / `--complex-fit-noise-output`: MEDI's complex fit itself, as QSM.rs's
/// field mapping returns it (`FieldMappingResult::complex_fit`, the fit the field map above was
/// unwrapped from), before unwrapping, in MEDI's units: radians per echo spacing.
fn save_complex_fit(
    common: &FieldmapCommonArgs,
    fit: Option<&qsm_core::utils::FitPpmComplexResult>,
    reference: &qsm_core::io::NiftiData,
) -> qsmxt_io::Result<()> {
    if common.complex_fit_output.is_none() && common.complex_fit_noise_output.is_none() {
        return Ok(());
    }
    let fit = fit.ok_or_else(|| QsmxtError::Config(
        "--complex-fit-output and --complex-fit-noise-output need --b0-estimation complex-fit".into()))?;
    for (path, data, what) in [
        (&common.complex_fit_output, &fit.field, "Complex fit (rad per echo spacing)"),
        (&common.complex_fit_noise_output, &fit.noise, "Complex fit noise map (rad per echo spacing)"),
    ] {
        if let Some(path) = path {
            save_nifti(path, data, reference)?;
            info!("{what} saved to {}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{FieldmapCommand, FieldmapCommonArgs, FieldmapRomeoArgs};
    use crate::cli_settings::SettingsOf;
    use std::f64::consts::PI;
    use std::io::Write;

    const NX: usize = 8;
    const NY: usize = 8;
    const NZ: usize = 8;

    /// `qsmxt fieldmap romeo` and `qsmxt run` build the same field-mapping configuration from
    /// the same flags (#279: the standalone tool started from qsm-core's ROMEO defaults, with
    /// individual unwrapping and global correction off, where `run` has both on).
    #[test]
    fn fieldmap_romeo_matches_run_for_the_same_flags() {
        use clap::Parser;
        let flag_sets: &[&[&str]] = &[
            &[],
            &["--no-romeo-phase-linearity", "--romeo-max-seeds", "3", "--no-romeo-correct-global"],
            &["--phase-offset-sigma-mm", "2", "2", "1", "--romeo-mag-weight", "--no-romeo-individual",
              "--romeo-template", "2"],
            // QSMART's echo fit on ROMEO's echoes, bipolar correction included.
            &["--b0-estimation", "qsmart-echofit", "--bipolar-correction", "--qsmart-fit-percentile", "95"],
        ];
        for flags in flag_sets {
            let mut fm = vec!["qsmxt", "fieldmap", "romeo", "p.nii", "--mask", "m.nii", "--output", "o.nii",
                              "--tes", "0.005", "0.01", "--b0", "3"];
            fm.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&fm).unwrap();
            let crate::cli::Command::Fieldmap { command: FieldmapCommand::Romeo(args) } = cli.command else { panic!() };
            let standalone = qsmxt_engine::to_pipeline_stages(
                &super::config(&args.common, qsmxt_engine::UnwrappingAlgorithm::Romeo, &args.settings.0).unwrap()).0;

            let mut run = vec!["qsmxt", "run", "bids", "--unwrapping-algorithm", "romeo"];
            run.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&run).unwrap();
            let crate::cli::Command::Run(r) = cli.command else { panic!() };
            let mut config = qsmxt_engine::PipelineConfig::default();
            crate::cli_settings::apply_normalised(&mut config, &r.pipeline).unwrap();
            let pipeline = qsmxt_engine::to_pipeline_stages(&config).0;
            assert_eq!(format!("{standalone:?}"), format!("{pipeline:?}"), "flags {flags:?}");
        }
    }

    /// `qsmxt fieldmap laplacian` and `qsmxt run --unwrapping-algorithm laplacian` build the same
    /// field-mapping configuration from the same flags, the unmasked unwrapping included.
    #[test]
    fn fieldmap_laplacian_matches_run_for_the_same_flags() {
        use clap::Parser;
        let flag_sets: &[&[&str]] = &[
            &[],
            &["--laplacian-unmasked"],
            &["--laplacian-solver", "mirror", "--laplacian-unmasked", "--b0-estimation", "qsmart-echofit"],
            &["--laplacian-solver", "fft", "--laplacian-fft-pad", "12", "--b0-estimation", "linear-fit"],
            &["--laplacian-solver", "mgpcg", "--laplacian-unmasked"],
            // Phase offsets are removed before Laplacian unwrapping too (QSM.rs#484).
            &["--phase-offset-removal", "false"],
            &["--phase-offset-sigma-mm", "3", "3", "3", "--bipolar-correction"],
        ];
        for flags in flag_sets {
            let mut fm = vec!["qsmxt", "fieldmap", "laplacian", "p.nii", "--mask", "m.nii", "--output", "o.nii",
                              "--tes", "0.005", "0.01", "--b0", "3"];
            fm.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&fm).unwrap();
            let crate::cli::Command::Fieldmap { command: FieldmapCommand::Laplacian(args) } = cli.command else { panic!() };
            let standalone = qsmxt_engine::to_pipeline_stages(
                &super::config(&args.common, qsmxt_engine::UnwrappingAlgorithm::Laplacian, &args.settings.0).unwrap()).0;
            assert_eq!(standalone.laplacian_unmasked, flags.contains(&"--laplacian-unmasked"), "flags {flags:?}");

            let mut run = vec!["qsmxt", "run", "bids", "--unwrapping-algorithm", "laplacian"];
            run.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&run).unwrap();
            let crate::cli::Command::Run(r) = cli.command else { panic!() };
            let mut config = qsmxt_engine::PipelineConfig::default();
            crate::cli_settings::apply_normalised(&mut config, &r.pipeline).unwrap();
            let pipeline = qsmxt_engine::to_pipeline_stages(&config).0;
            assert_eq!(format!("{standalone:?}"), format!("{pipeline:?}"), "flags {flags:?}");
        }
    }

    /// `qsmxt fieldmap medi-region-growing` and `qsmxt run --unwrapping-algorithm
    /// medi-region-growing` build the same field-mapping configuration from the same flags; the
    /// standalone command refuses to run without `--magnitude`.
    #[test]
    fn fieldmap_medi_region_growing_matches_run_for_the_same_flags() {
        use clap::Parser;
        let flag_sets: &[&[&str]] = &[
            &[],
            &["--b0-estimation", "complex-fit"],
            &["--phase-offset-sigma-mm", "2", "2", "1", "--bipolar-correction"],
            &["--phase-offset-removal", "false", "--b0-estimation", "linear-fit"],
        ];
        for flags in flag_sets {
            let mut fm = vec!["qsmxt", "fieldmap", "medi-region-growing", "p.nii", "--mask", "m.nii", "--output", "o.nii",
                              "--magnitude", "mag.nii", "--tes", "0.005", "0.01", "--b0", "3"];
            fm.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&fm).unwrap();
            let crate::cli::Command::Fieldmap { command: FieldmapCommand::MediRegionGrowing(args) } = cli.command else { panic!() };
            let standalone = qsmxt_engine::to_pipeline_stages(
                &super::config(&args.common, qsmxt_engine::UnwrappingAlgorithm::MediRegionGrowing, &[]).unwrap()).0;
            let mut run = vec!["qsmxt", "run", "bids", "--unwrapping-algorithm", "medi-region-growing"];
            run.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&run).unwrap();
            let crate::cli::Command::Run(r) = cli.command else { panic!() };
            let mut config = qsmxt_engine::PipelineConfig::default();
            crate::cli_settings::apply_normalised(&mut config, &r.pipeline).unwrap();
            let pipeline = qsmxt_engine::to_pipeline_stages(&config).0;
            assert_eq!(format!("{standalone:?}"), format!("{pipeline:?}"), "flags {flags:?}");
        }
        let cli = crate::cli::Cli::try_parse_from(["qsmxt", "fieldmap", "medi-region-growing", "p.nii", "--mask", "m.nii",
            "--output", "o.nii", "--tes", "0.005", "0.01", "--b0", "3"]).unwrap();
        let crate::cli::Command::Fieldmap { command } = cli.command else { panic!() };
        let err = super::execute(command).unwrap_err().to_string();
        assert!(err.contains("needs --magnitude"), "{err}");
    }

    /// `qsmxt fieldmap prelude` and `qsmxt run --unwrapping-algorithm prelude` build the same
    /// field-mapping configuration from the same flags, PRELUDE's settings included.
    #[test]
    fn fieldmap_prelude_matches_run_for_the_same_flags() {
        use clap::Parser;
        let flag_sets: &[&[&str]] = &[
            &[],
            &["--prelude-labelling", "volume", "--prelude-partitions", "6"],
            &["--prelude-labelling", "slices", "--phase-offset-sigma-mm", "2", "2", "1", "--bipolar-correction"],
            &["--phase-offset-removal", "false", "--b0-estimation", "linear-fit"],
        ];
        for flags in flag_sets {
            let mut fm = vec!["qsmxt", "fieldmap", "prelude", "p.nii", "--mask", "m.nii", "--output", "o.nii",
                              "--tes", "0.005", "0.01", "--b0", "3"];
            fm.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&fm).unwrap();
            let crate::cli::Command::Fieldmap { command: FieldmapCommand::Prelude(args) } = cli.command else { panic!() };
            let standalone = qsmxt_engine::to_pipeline_stages(
                &super::config(&args.common, qsmxt_engine::UnwrappingAlgorithm::Prelude, &args.settings.0).unwrap()).0;
            assert_eq!(standalone.unwrapping_algorithm, qsm_core::pipeline::config::UnwrappingAlgorithm::Prelude);

            let mut run = vec!["qsmxt", "run", "bids", "--unwrapping-algorithm", "prelude"];
            run.extend_from_slice(flags);
            let cli = crate::cli::Cli::try_parse_from(&run).unwrap();
            let crate::cli::Command::Run(r) = cli.command else { panic!() };
            let mut config = qsmxt_engine::PipelineConfig::default();
            crate::cli_settings::apply_normalised(&mut config, &r.pipeline).unwrap();
            let pipeline = qsmxt_engine::to_pipeline_stages(&config).0;
            assert_eq!(format!("{standalone:?}"), format!("{pipeline:?}"), "flags {flags:?}");
        }
        let cli = crate::cli::Cli::try_parse_from(["qsmxt", "fieldmap", "prelude", "p.nii", "--mask", "m.nii",
            "--output", "o.nii", "--prelude-labelling", "volume", "--prelude-partitions", "6"]).unwrap();
        let crate::cli::Command::Fieldmap { command: FieldmapCommand::Prelude(args) } = cli.command else { panic!() };
        let cfg = super::config(&args.common, qsmxt_engine::UnwrappingAlgorithm::Prelude, &args.settings.0).unwrap();
        assert_eq!(qsmxt_engine::to_pipeline_stages(&cfg).0.prelude_params, qsm_core::unwrap::PreludeParams {
            num_phase_partitions: 6, labelling: qsm_core::unwrap::PreludeLabelling::Volume,
            ..Default::default()
        });
        // No partitions is refused at the command line.
        assert!(crate::cli::Cli::try_parse_from(["qsmxt", "fieldmap", "prelude", "p.nii", "--mask", "m.nii",
            "--output", "o.nii", "--prelude-partitions", "0"]).is_err());
    }

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
        fs_err::write(path, &buf).unwrap();
    }

    /// A two-echo linear-phase field should be recovered to a known ppm value.
    #[test]
    fn test_fieldmap_romeo_recovers_linear_field() {
        let dir = tempfile::tempdir().unwrap();
        let phase_path = dir.path().join("phase.nii");
        let mask_path = dir.path().join("mask.nii");
        let out_path = dir.path().join("field.nii");
        let field_mask_path = dir.path().join("field_mask.nii");

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
            reliability_output: None, field_mask_output: Some(field_mask_path.clone()), complex_fit_output: None, complex_fit_noise_output: None,
            // Disable offset removal so the constant field is preserved (ROMEO path B).
            settings: SettingsOf::from_flags(&["--phase-offset-removal", "false"]),
        };
        let args = FieldmapRomeoArgs { common, settings: Default::default() };

        execute(FieldmapCommand::Romeo(args)).unwrap();

        let out = qsm_core::io::read_nifti_file(&out_path).unwrap();
        assert_eq!(out.data.len(), n);
        // Without offset removal the field is defined over the whole mask.
        assert!(qsm_core::io::read_nifti_file(&field_mask_path).unwrap().data.iter().all(|&v| v == 1.0));

        let expected_ppm = freq_hz * 1e6 / (qsm_core::constants::GAMMA_BAR_HZ_PER_T * b0);
        let mean: f64 = out.data.iter().sum::<f64>() / n as f64;
        assert!(
            (mean - expected_ppm).abs() < 0.01,
            "expected ~{:.4} ppm, got {:.4}",
            expected_ppm, mean
        );
    }

    /// `--complex-fit-output` and `--complex-fit-noise-output` write MEDI's fit before
    /// unwrapping, in radians per echo spacing: here the phase grows by 0.6 rad per echo, so the
    /// fit is 0.6 everywhere and its noise map is positive. Without the complex fit they are
    /// refused.
    #[test]
    fn test_fieldmap_writes_the_complex_fit_and_its_noise() {
        use clap::Parser;
        let dir = tempfile::tempdir().unwrap();
        let p = |n: &str| dir.path().join(n).display().to_string();
        let n = NX * NY * NZ;
        write_nifti_4d(&dir.path().join("phase.nii"), &[vec![0.6; n], vec![1.2; n], vec![1.8; n]]);
        write_nifti_4d(&dir.path().join("mag.nii"), &[vec![1.0; n], vec![0.8; n], vec![0.6; n]]);
        qsm_core::io::save_nifti_to_file(
            &dir.path().join("mask.nii"), &vec![1.0f64; n], (NX, NY, NZ), (1.0, 1.0, 1.0),
            &[1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0],
        ).unwrap();
        let run = |estimation: &str| {
            let cli = crate::cli::Cli::try_parse_from([
                "qsmxt", "fieldmap", "laplacian", &p("phase.nii"), "-m", &p("mask.nii"), "-o", &p("field.nii"),
                "--magnitude", &p("mag.nii"), "--tes", "0.005", "0.01", "0.015", "--b0", "3",
                "--b0-estimation", estimation,
                "--complex-fit-output", &p("raw.nii"), "--complex-fit-noise-output", &p("n_std.nii"),
            ]).unwrap();
            let crate::cli::Command::Fieldmap { command } = cli.command else { panic!() };
            execute(command)
        };
        run("complex-fit").unwrap();
        let raw = qsm_core::io::read_nifti_file(&dir.path().join("raw.nii")).unwrap().data;
        let noise = qsm_core::io::read_nifti_file(&dir.path().join("n_std.nii")).unwrap().data;
        assert!(raw.iter().all(|&v| (v - 0.6).abs() < 1e-5), "{:?}", &raw[..4]);
        assert!(noise.iter().all(|&v| v > 0.0 && v.is_finite()), "{:?}", &noise[..4]);

        let err = run("weighted-avg").unwrap_err();
        assert!(format!("{err}").contains("--b0-estimation complex-fit"), "{err}");
    }

    /// Params JSON supplies TE and B0 when CLI flags are omitted.
    #[test]
    fn test_resolve_scan_params_from_json() {
        let dir = tempfile::tempdir().unwrap();
        let params_path = dir.path().join("params.json");
        fs_err::write(&params_path, r#"{"TE":[0.004,0.008,0.012],"B0":7.0}"#).unwrap();

        let common = FieldmapCommonArgs {
            input: dir.path().join("phase.nii"),
            mask: dir.path().join("mask.nii"),
            output: dir.path().join("out.nii"),
            magnitude: None,
            tes: None,
            b0: None,
            params: Some(params_path),
            reliability_output: None, field_mask_output: None, complex_fit_output: None, complex_fit_noise_output: None,
            settings: Default::default(),
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
            reliability_output: None, field_mask_output: None, complex_fit_output: None, complex_fit_noise_output: None,
            settings: Default::default(),
        };
        let args = FieldmapRomeoArgs { common, settings: Default::default() };
        assert!(execute(FieldmapCommand::Romeo(args)).is_err());
    }
}
