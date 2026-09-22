use log::info;

use super::common::{load_nifti, nifti_grid, save_nifti};
use crate::cli::SegmentArgs;
use crate::error::QsmxtError;
use crate::pipeline::config::{parse_synthseg_version, SegmentationConfig, SynthSegVersion};

/// `qsmxt segment` — run SynthSeg on one magnitude image.
///
/// The pipeline stage is the usual way in; this exists to try the network on a single volume, or to
/// segment data that never went through a QSM run.
pub fn execute(args: SegmentArgs) -> crate::Result<()> {
    let mut cfg = SegmentationConfig::default();
    if let Some(ref v) = args.params.synthseg_version {
        cfg.version = parse_synthseg_version(v).ok_or_else(|| {
            QsmxtError::Config(format!("--synthseg-version '{v}': expected v1 or v2"))
        })?;
    }
    if let Some(v) = args.params.synthseg_crop {
        cfg.crop = Some(v);
    }
    if args.params.no_synthseg_flip_averaging {
        cfg.flip_averaging = false;
    }
    if args.params.no_synthseg_topology_cleanup {
        cfg.topology_cleanup = false;
    }
    if let Some(v) = args.params.synthseg_sigma {
        cfg.sigma_smoothing = v;
    }

    let nifti = load_nifti(&args.magnitude)?;
    let grid = nifti_grid(&nifti);
    info!(
        "Segmenting with SynthSeg {} ({}x{}x{})",
        cfg.version, grid.nx(), grid.ny(), grid.nz()
    );

    let (labels, _volumes) = run(&nifti.data, &grid, &nifti.affine, &cfg)?;
    // FreeSurfer ids are small integers, so they travel through the f64 writer exactly.
    let as_f64: Vec<f64> = labels.iter().map(|&l| l as f64).collect();
    save_nifti(&args.output, &as_f64, &nifti)?;
    info!("Wrote {}", args.output.display());

    if let Some(ref lookup) = args.lookup {
        let table = core_version(cfg.version).labels();
        let mut out = String::from("index\tname\n");
        for (id, name) in table.ids.iter().zip(table.names) {
            out.push_str(&format!("{id}\t{name}\n"));
        }
        if let Some(parent) = lookup.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(lookup, out)?;
        info!("Wrote {}", lookup.display());
    }
    Ok(())
}

fn core_version(v: SynthSegVersion) -> qsm_core::segment::SynthSegVersion {
    match v {
        SynthSegVersion::V1 => qsm_core::segment::SynthSegVersion::V1,
        SynthSegVersion::V2 => qsm_core::segment::SynthSegVersion::V2,
    }
}

#[cfg(feature = "dl")]
fn run(
    magnitude: &[f64], grid: &qsm_core::Grid, affine: &[f64; 16], cfg: &SegmentationConfig,
) -> crate::Result<(Vec<i32>, Vec<f64>)> {
    crate::pipeline::runner::prefetch_weights("synthseg", "segment")?;
    let params = qsm_core::segment::SynthSegParams {
        version: core_version(cfg.version),
        crop: cfg.crop,
        flip_averaging: cfg.flip_averaging,
        topology_cleanup: cfg.topology_cleanup,
        sigma_smoothing: cfg.sigma_smoothing,
    };
    let weights = qsm_core::models::primary_weight("synthseg")
        .map_err(|e| QsmxtError::Config(format!("synthseg weights: {e}")))?;
    let out = qsm_core::segment::synthseg(magnitude, grid, affine, &weights, &params, |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("synthseg: {e}")))?;
    Ok((out.labels, out.volumes))
}

#[cfg(not(feature = "dl"))]
fn run(
    _magnitude: &[f64], _grid: &qsm_core::Grid, _affine: &[f64; 16], _cfg: &SegmentationConfig,
) -> crate::Result<(Vec<i32>, Vec<f64>)> {
    Err(QsmxtError::Config(
        "qsmxt segment needs a deep-learning build: SynthSeg runs an ONNX network, which this \
         binary was compiled without (--no-default-features)"
            .into(),
    ))
}
