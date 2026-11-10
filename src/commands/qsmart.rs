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
use crate::error::QsmxtError;
use crate::pipeline::config;

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
fn pipeline_config(args: &QsmartArgs) -> crate::Result<config::PipelineConfig> {
    let mut cfg = config::PipelineConfig::default();
    cfg.inversion.algorithm = config::QsmAlgorithm::Qsmart;
    config::apply_qsmart_overrides(&mut cfg, &args.qsmart_params)?;
    // The inner iLSQR's arithmetic is iLSQR's own setting, as in `qsmxt run`.
    if let Some(p) = args.ilsqr_precision { cfg.inversion.ilsqr.precision = p; }
    Ok(cfg)
}

pub fn execute(args: QsmartArgs) -> crate::Result<()> {
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
        config::to_scan_metadata(dims, voxel_size, &[args.echo_time], args.field_strength, b0, Some(field.affine));

    let magnitude: Option<Vec<f64>> = match (&args.magnitude, cfg.inversion.qsmart.vessel_magnitude) {
        (Some(p), config::QsmartVesselMagnitude::MeanN4) => {
            let echoes = load_nifti_4d(p)?.echoes;
            let refs: Vec<&[f64]> = echoes.iter().map(|e| e.as_slice()).collect();
            Some(qsm_core::utils::qsmart_vessel_magnitude(&refs, &scan_meta.grid())
                .map_err(|e| QsmxtError::Config(format!("--magnitude: {e}")))?)
        }
        (Some(p), config::QsmartVesselMagnitude::Combined) => Some(load_nifti(p)?.data),
        (None, _) => None,
    };
    let reliability = match &args.reliability {
        Some(p) => Some(load_mask(p)?.0),
        None => None,
    };

    if let Some(dir) = &args.intermediates {
        std::fs::create_dir_all(dir)?;
    }
    let mut writer = args.intermediates.as_deref().map(|dir| IntermediateWriter {
        dir, reference: &field, written: Vec::new(), error: None,
    });
    if let (Some(w), Some(m), config::QsmartVesselMagnitude::MeanN4) =
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
}
