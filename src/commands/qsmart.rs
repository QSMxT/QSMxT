//! Standalone QSMART reconstruction: total field (ppm) -> susceptibility (ppm).
//!
//! QSMART is a two-stage, vessel-aware method that performs its own background field removal, so it
//! consumes the TOTAL field rather than a local field. It is not a plain `invert`/`bgremove` step,
//! hence its own top-level command. This mirrors the `qsmxt run` QSMART stage on NIfTI in/out.

use std::path::{Path, PathBuf};

use log::info;
use qsm_core::io::NiftiData;

use super::common::{load_mask, load_nifti, load_nifti_4d, nifti_grid, save_nifti};
use crate::cli::QsmartArgs;
use qsmxt_io::error::QsmxtError;


/// The file name (without extension) `--intermediates` writes a QSMART intermediate under: the
/// artifact names (`localfield`, `chimap`) with the stage, as QSM-CI names them.
fn intermediate_file(name: &str) -> &str {
    use qsmxt_engine::stages::qsmart::outputs as q;
    match name {
        q::LOCAL_FIELD_STAGE1 => "localfield_stage1",
        q::CHI_STAGE1 => "chimap_stage1",
        q::LOCAL_FIELD_STAGE2 => "localfield_stage2",
        q::CHI_STAGE2 => "chimap_stage2",
        other => other,
    }
}

/// Writes each intermediate QSMART reports into `dir` as it is produced. The first failure is
/// kept and returned after the run (the observer cannot return one).
struct IntermediateWriter<'a> {
    dir: &'a Path,
    reference: &'a NiftiData,
    written: Vec<PathBuf>,
    error: Option<QsmxtError>,
}

impl IntermediateWriter<'_> {
    fn write(&mut self, name: &str, data: &[f64]) {
        if self.error.is_some() {
            return;
        }
        let path = self.dir.join(format!("{}.nii", intermediate_file(name)));
        match save_nifti(&path, data, self.reference) {
            Ok(()) => self.written.push(path),
            Err(e) => self.error = Some(e),
        }
    }
}

impl qsm_core::pipeline::Observer for IntermediateWriter<'_> {
    fn output(&mut self, name: &'static str, volume: qsm_core::pipeline::VolumeRef<'_>) {
        match volume {
            qsm_core::pipeline::VolumeRef::Real(v) => self.write(name, v),
            qsm_core::pipeline::VolumeRef::Mask(m) => {
                let v: Vec<f64> = m.iter().map(|&x| f64::from(x)).collect();
                self.write(name, &v);
            }
        }
    }
}

/// The QSMART pipeline configuration the flags describe, as `qsmxt run` would hold it.
fn pipeline_config(args: &QsmartArgs) -> qsmxt_io::Result<qsmxt_engine::PipelineConfig> {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.inversion.algorithm = qsmxt_engine::QsmAlgorithm::Qsmart;
    // The QSMART settings' flags, applied as `qsmxt run` applies them.
    qsmxt_io::settings::apply(&mut cfg, &args.qsmart_settings.0)?;
    // The inner iLSQR's arithmetic is iLSQR's own setting, as in `qsmxt run`.
    if let Some(p) = args.ilsqr_precision { cfg.inversion.ilsqr.precision = p; }
    Ok(cfg)
}

pub fn execute(args: QsmartArgs) -> qsmxt_io::Result<()> {
    let field = load_nifti(&args.input)?;
    let (mask, _) = load_mask(&args.mask)?;
    let grid = nifti_grid(&field);
    let dims = (grid.nx(), grid.ny(), grid.nz());
    let voxel_size = (grid.vsx(), grid.vsy(), grid.vsz());
    let b0 = (args.b0_direction[0], args.b0_direction[1], args.b0_direction[2]);
    info!("QSMART ({}x{}x{})", dims.0, dims.1, dims.2);

    // Lower a QSMART pipeline config to the qsm-core inversion config, exactly like `qsmxt run`.
    let cfg = pipeline_config(&args)?;
    // The vessel sizes in voxels, converted from mm when given in mm, as `qsmxt run` does.
    // `--qsmart-offset-grid` is among the QSMART settings, as for `qsmxt run`.
    let inv_config = qsmxt_engine::stages::qsmart::vessel_sizes_in_voxels(&cfg, voxel_size);

    let scan_meta =
        qsmxt_engine::to_scan_metadata(dims, voxel_size, &[args.echo_time], args.field_strength, b0, Some(field.affine));

    let magnitude: Option<Vec<f64>> = match (&args.magnitude, cfg.inversion.qsmart.vessel_magnitude) {
        (Some(p), qsmxt_engine::QsmartVesselMagnitude::MeanN4) => {
            let echoes = load_nifti_4d(p)?.echoes;
            let refs: Vec<&[f64]> = echoes.iter().map(|e| e.as_slice()).collect();
            Some(qsm_core::utils::qsmart_vessel_magnitude(&refs, &scan_meta.grid())
                .map_err(|e| QsmxtError::Config(format!("--magnitude: {e}")))?)
        }
        (Some(p), qsmxt_engine::QsmartVesselMagnitude::Combined) => Some(load_nifti(p)?.data),
        (None, _) => None,
    };
    let reliability = match &args.reliability {
        Some(p) => Some(load_mask(p)?.0),
        None => None,
    };

    if let Some(dir) = &args.intermediates {
        fs_err::create_dir_all(dir)?;
    }
    let mut writer = args.intermediates.as_deref().map(|dir| IntermediateWriter {
        dir, reference: &field, written: Vec::new(), error: None,
    });
    if let (Some(w), Some(m), qsmxt_engine::QsmartVesselMagnitude::MeanN4) =
        (writer.as_mut(), &magnitude, cfg.inversion.qsmart.vessel_magnitude)
    {
        // QSMART's own vessel magnitude, made here from the echoes.
        w.write(qsmxt_engine::stages::qsmart::VESSEL_MAGNITUDE, m);
    }
    let mut progress = |_: usize, _: usize| {};
    let observer: &mut dyn qsm_core::pipeline::Observer = match writer.as_mut() {
        Some(w) => w,
        None => &mut progress,
    };
    let chi = qsm_core::pipeline::run_qsmart(
        &field.data,
        &mask,
        reliability.as_deref(),
        magnitude.as_deref(),
        &scan_meta,
        &inv_config,
        qsm_core::pipeline::QsmReference::None,
        observer,
    )
    .map_err(|e| QsmxtError::Config(format!("qsmart: {}", e)))?;
    if let Some(w) = writer {
        if let Some(e) = w.error {
            return Err(e);
        }
        for path in &w.written {
            info!("Intermediate saved to {}", path.display());
        }
    }

    save_nifti(&args.output, &chi, &field)?;
    info!("Susceptibility map saved to {}", args.output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    fn args(extra: &[&str]) -> crate::cli::QsmartArgs {
        let mut argv = vec!["qsmxt", "qsmart", "field.nii", "-m", "mask.nii", "-o", "chi.nii",
                            "--field-strength", "3", "--echo-time", "0.02"];
        argv.extend_from_slice(extra);
        match crate::cli::Cli::try_parse_from(argv).unwrap().command {
            crate::cli::Command::Qsmart(a) => a,
            _ => panic!("expected qsmart"),
        }
    }

    /// `--ilsqr-precision` reaches the inner iLSQR's parameters; without it, double as before.
    #[test]
    fn ilsqr_precision_reaches_the_inner_ilsqr() {
        use qsm_core::inversion::IlsqrPrecision as P;
        let inner = |a| {
            let cfg = super::pipeline_config(&a).unwrap();
            qsmxt_engine::stages::qsmart::vessel_sizes_in_voxels(&cfg, (1.0, 1.0, 1.0)).ilsqr.precision
        };
        assert_eq!(inner(args(&[])), P::Double);
        assert_eq!(inner(args(&["--ilsqr-precision", "single"])), P::Single);
        assert!(crate::cli::Cli::try_parse_from(["qsmxt", "qsmart", "f", "-m", "m", "-o", "o",
            "--field-strength", "3", "--echo-time", "0.02", "--ilsqr-precision", "half"]).is_err());
    }

    /// Each QSMART flag sets its setting to the value given, as the subcommand's own flags did
    /// before they came from the flag table; without them, the defaults. No range is checked here
    /// (a Frangi C of 0 is taken), as before.
    #[test]
    fn qsmart_flags_set_their_settings() {
        let q = |extra: &[&str]| super::pipeline_config(&args(extra)).unwrap().inversion.qsmart;
        let d = qsmxt_engine::QsmartConfig::default();
        assert_eq!(serde_json::to_value(q(&[])).unwrap(), serde_json::to_value(&d).unwrap());
        let c = q(&[
            "--qsmart-ilsqr-max-iter", "7", "--qsmart-ilsqr-pad-mm", "2.5", "--qsmart-vasc-sphere-radius", "5",
            "--qsmart-inversion", "tv", "--qsmart-sdf-sigma1-stage1", "11", "--qsmart-sdf-sigma2-stage1", "1.5",
            "--qsmart-sdf-sigma1-stage2", "9", "--qsmart-sdf-sigma2-stage2", "0.5", "--qsmart-sdf-lower-lim", "0.45",
            "--qsmart-sdf-curv-constant", "600", "--qsmart-frangi-scale-min", "1.5", "--qsmart-frangi-scale-max", "7",
            "--qsmart-frangi-scale-ratio", "1", "--qsmart-frangi-c", "0", "--qsmart-frangi-c-mode", "relative",
            "--qsmart-frangi-c-factor", "0.3", "--qsmart-vessel-units", "mm", "--qsmart-vessel-magnitude", "combined",
            "--qsmart-offset-grid", "qsmart-reference",
        ]);
        assert_eq!((c.ilsqr_max_iter, c.ilsqr_pad_mm, c.vasc_sphere_radius), (7, 2.5, 5));
        assert_eq!(c.inversion, qsmxt_engine::QsmAlgorithm::Tv);
        assert_eq!((c.sdf_sigma1_stage1, c.sdf_sigma2_stage1, c.sdf_sigma1_stage2, c.sdf_sigma2_stage2), (11.0, 1.5, 9.0, 0.5));
        assert_eq!((c.sdf_lower_lim, c.sdf_curv_constant), (0.45, 600.0));
        assert_eq!((c.frangi_scale_min, c.frangi_scale_max, c.frangi_scale_ratio), (1.5, 7.0, 1.0));
        assert_eq!((c.frangi_c, c.frangi_c_mode, c.frangi_c_factor), (0.0, qsmxt_engine::QsmartFrangiCMode::Relative, 0.3));
        assert_eq!(c.vessel_size_units, qsmxt_engine::VesselSizeUnits::Mm);
        assert_eq!(c.vessel_magnitude, qsmxt_engine::QsmartVesselMagnitude::Combined);
        assert_eq!(c.offset_grid, qsmxt_engine::QsmartOffsetGrid::QsmartReference);
        let c = q(&["--qsmart-frangi-c-mode", "absolute", "--qsmart-vessel-units", "voxels",
                    "--qsmart-vessel-magnitude", "mean-n4", "--qsmart-offset-grid", "fft"]);
        assert_eq!(c.frangi_c_mode, qsmxt_engine::QsmartFrangiCMode::Absolute);
        assert_eq!(c.vessel_size_units, qsmxt_engine::VesselSizeUnits::Voxels);
        assert_eq!(c.vessel_magnitude, qsmxt_engine::QsmartVesselMagnitude::MeanN4);
        assert_eq!(c.offset_grid, qsmxt_engine::QsmartOffsetGrid::Fft);
    }

    /// Every inner inversion `--qsmart-inversion` lists is the algorithm of that name, and `heidi`
    /// is refused with the words it always had.
    #[test]
    fn qsmart_inversion_values_name_their_algorithm() {
        use clap::CommandFactory;
        let cli = crate::cli::Cli::command();
        let qsmart = cli.find_subcommand("qsmart").expect("qsmxt qsmart");
        let arg = qsmart.get_arguments().find(|a| a.get_long() == Some("qsmart-inversion")).expect("--qsmart-inversion");
        let values: Vec<String> = arg.get_possible_values().iter().map(|v| v.get_name().to_string()).collect();
        assert_eq!(values.len(), 33);
        for v in values.iter().filter(|v| *v != "heidi") {
            let c = super::pipeline_config(&args(&["--qsmart-inversion", v])).unwrap();
            assert_eq!(serde_json::to_value(c.inversion.qsmart.inversion).unwrap(), serde_json::json!(v), "{v}");
        }
        let e = super::pipeline_config(&args(&["--qsmart-inversion", "heidi"])).unwrap_err();
        assert_eq!(e.to_string(), "Pipeline configuration error: --qsmart-inversion heidi: HEIDI is not one of \
            QSMART's inner inversions (its two stages are its own recipe)");
    }
}
