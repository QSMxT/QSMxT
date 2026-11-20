use log::info;
use super::common::{load_nifti, load_mask, save_nifti};
use crate::cli::{InvertCommand, InvertCommonArgs};
use qsmxt_io::error::QsmxtError;
use super::common;

mod methods;
mod multi_orient;

/// The log target of everything `qsmxt invert` prints, kept as it was before the command was split
/// into submodules (a target is part of each log line).
const LOG_TARGET: &str = "qsmxt::commands::invert";

use multi_orient::{run_cosmos, run_sti};

/// The magnitude an inversion is given, as qsm-core's dispatcher asks for it.
enum Magnitude<'a> {
    /// None: the method takes no magnitude, or it is optional and was not given.
    None,
    /// The combined magnitude (root-sum-of-squares over the echoes given): the data weight.
    Combined(&'a [std::path::PathBuf]),
    /// Every echo's magnitude, stacked with the echo slowest (MEDI, TFI and HD-QSM).
    Echoes(&'a [std::path::PathBuf]),
}

/// Invert `c.input` inside `c.mask` with `config` through qsm-core's pipeline dispatcher
/// ([`qsm_core::pipeline::run_dipole_inversion`]), as `qsmxt run` does: the field in ppm and the
/// scan's field strength and echo times go in, and each method does its own unit conversion and
/// derives its own weights. `field_strength` and `echo_times` are the scan's (empty for a method
/// that uses neither). Weights of a deep-learning method are fetched (with a download bar) first.
fn invert_with(
    c: &InvertCommonArgs, field_strength: f64, echo_times: &[f64], magnitude: Magnitude,
    config: qsm_core::pipeline::InversionConfig, name: &str,
) -> qsmxt_io::Result<(Vec<f64>, qsm_core::io::NiftiData)> {
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let (nx, ny, nz) = field_nifti.dims;
    let n_voxels = field_nifti.data.len();
    if let Some(id) = config.algorithm.dl_model_id() {
        qsmxt_io::pipeline::runner::prefetch_weights(id, name)?;
    }
    let magnitude = match magnitude {
        Magnitude::None => None,
        Magnitude::Combined(paths) => super::common::load_combined_magnitude(paths, n_voxels)?,
        Magnitude::Echoes(paths) => {
            let (stack, n_echoes) = super::common::load_echo_stack(paths, n_voxels)?;
            if n_echoes != echo_times.len() {
                return Err(QsmxtError::Config(format!(
                    "{name} needs the magnitude of every echo: got {n_echoes} magnitude volume(s) for {} echo time(s)",
                    echo_times.len())));
            }
            Some(stack)
        }
    };
    info!("Dipole inversion ({}, {}x{}x{})", name, nx, ny, nz);
    let metadata = qsm_core::pipeline::ScanMetadata {
        dims: field_nifti.dims,
        voxel_size: field_nifti.voxel_size,
        echo_times: echo_times.to_vec(),
        field_strength,
        b0_direction: (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]),
        slice_geometry: None,
        affine: Some(field_nifti.affine),
    };
    let chi = qsm_core::pipeline::run_dipole_inversion(
        &field_nifti.data, &mask, &metadata, &config, magnitude.as_deref(), &mut |_, _| {},
    ).map_err(|e| QsmxtError::Config(format!("{}: {}", name, e)))?;
    Ok((chi, field_nifti))
}

/// LSQR is weighted by the magnitude, as the HEIDI package always weights it (it warns without
/// one: qsm_heidi computesusceptibility_heidi.m:821-822), so `--magnitude` is required.
fn require_magnitude(magnitude: &[std::path::PathBuf], name: &str) -> qsmxt_io::Result<()> {
    if magnitude.is_empty() {
        return Err(QsmxtError::Config(format!("{name} needs --magnitude (its data weight)")));
    }
    Ok(())
}

/// HEIDI's cone fill on `seed`, a χ map (ppm) another inversion made of `c.input` inside
/// `c.mask` ([`qsm_core::pipeline::run_heidi_cone_fill`], as `qsmxt run --heidi` runs it).
fn heidi_on_seed(
    c: &InvertCommonArgs, seed: &std::path::Path, field_strength: f64, echo_times: &[f64],
    config: &qsm_core::pipeline::InversionConfig,
) -> qsmxt_io::Result<(Vec<f64>, qsm_core::io::NiftiData)> {
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let seed_nifti = load_nifti(seed)?;
    if seed_nifti.dims != field_nifti.dims {
        return Err(QsmxtError::Config(format!(
            "--seed {} is {:?} but the field is {:?}", seed.display(), seed_nifti.dims, field_nifti.dims)));
    }
    let (nx, ny, nz) = field_nifti.dims;
    info!("HEIDI on {} ({}x{}x{})", seed.display(), nx, ny, nz);
    let metadata = qsm_core::pipeline::ScanMetadata {
        dims: field_nifti.dims,
        voxel_size: field_nifti.voxel_size,
        echo_times: echo_times.to_vec(),
        field_strength,
        b0_direction: (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]),
        slice_geometry: None,
        affine: Some(field_nifti.affine),
    };
    let chi = qsm_core::pipeline::run_heidi_cone_fill(
        &seed_nifti.data, &field_nifti.data, &mask, &metadata, config, &mut |_, _| {},
    ).map_err(|e| QsmxtError::Config(format!("HEIDI: {e}")))?;
    Ok((chi, field_nifti))
}

/// QSMxT's configuration with `algorithm` as the inversion and the command's settings flags
/// (its tiling flags and `settings`) applied, as `qsmxt run` applies them.
pub(super) fn config(
    c: &InvertCommonArgs, algorithm: qsmxt_engine::QsmAlgorithm, settings: &[qsmxt_engine::schema::cli_apply::Given],
) -> qsmxt_io::Result<qsmxt_engine::PipelineConfig> {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.inversion.algorithm = algorithm;
    qsmxt_io::settings::apply(&mut cfg, &[c.tiling.0.as_slice(), settings].concat())?;
    Ok(cfg)
}

/// Run a deep-learning dipole inversion (the DL models are not exposed as standalone functions).
/// Weights are downloaded on first use. AutoQSM and NeXtQSM are single-step and expect the TOTAL
/// field as input; the others take a local (tissue) field.
fn run_dl_inversion(
    c: &InvertCommonArgs, field_strength: f64, algorithm: qsmxt_engine::QsmAlgorithm, name: &str,
) -> qsmxt_io::Result<(Vec<f64>, qsm_core::io::NiftiData)> {
    let cfg = config(c, algorithm, &[])?;
    invert_with(c, field_strength, &[], Magnitude::None, qsmxt_engine::to_pipeline_stages(&cfg).2, name)
}

/// The networks that work from every echo's wrapped phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PhaseNet {
    /// iQSM: χ (ppm).
    Iqsm,
    /// iQSM+: χ (ppm), with the B0 direction as a network input.
    IqsmPlus,
    /// iQFM: the local field (ppm).
    Iqfm,
}

impl PhaseNet {
    /// The model id (its weights) and the name it is logged under.
    fn id_and_name(self) -> (&'static str, &'static str) {
        match self {
            Self::Iqsm => ("iqsm", "iQSM"),
            Self::IqsmPlus => ("iqsm-plus", "iQSM+"),
            Self::Iqfm => ("iqfm", "iQFM"),
        }
    }
}

/// The inputs of a phase network, loaded and checked: every echo's phase scaled into [-π, π]
/// (as `qsmxt run` reads it), every echo's magnitude if given, the mask, and the scan metadata.
pub(super) struct PhaseNetInputs {
    pub phases: Vec<Vec<f64>>,
    pub magnitudes: Vec<Vec<f64>>,
    pub mask: Vec<u8>,
    pub metadata: qsm_core::pipeline::ScanMetadata,
    pub reference: qsm_core::io::NiftiData,
}

pub(super) fn load_phase_net_inputs(args: &crate::cli::PhaseNetArgs) -> qsmxt_io::Result<PhaseNetInputs> {
    let phase = super::common::load_nifti_4d(&args.input)?;
    let n_echoes = phase.echoes.len();
    if args.scan.echo_times.len() != n_echoes {
        return Err(QsmxtError::Config(format!(
            "{} has {n_echoes} echo(es) but {} echo time(s) were given",
            args.input.display(), args.scan.echo_times.len())));
    }
    let mut phases = phase.echoes.clone();
    qsm_core::pipeline::scale_phases_to_pi(&mut phases);
    let magnitudes = match &args.magnitude {
        None => Vec::new(),
        Some(path) => {
            let mag = super::common::load_nifti_4d(path)?;
            if mag.dims != phase.dims || mag.echoes.len() != n_echoes {
                return Err(QsmxtError::Config(format!(
                    "--magnitude {} is {:?} with {} echo(es), but the phase is {:?} with {n_echoes}",
                    path.display(), mag.dims, mag.echoes.len(), phase.dims)));
            }
            mag.echoes
        }
    };
    let (mask, mask_nifti) = load_mask(&args.mask)?;
    if mask_nifti.dims != phase.dims {
        return Err(QsmxtError::Config(format!(
            "mask {} is {:?} but the phase is {:?}", args.mask.display(), mask_nifti.dims, phase.dims)));
    }
    let metadata = qsm_core::pipeline::ScanMetadata {
        dims: phase.dims,
        voxel_size: phase.voxel_size,
        echo_times: args.scan.echo_times.clone(),
        field_strength: args.scan.field_strength,
        b0_direction: (args.b0_direction[0], args.b0_direction[1], args.b0_direction[2]),
        slice_geometry: None,
        affine: Some(phase.affine),
    };
    Ok(PhaseNetInputs { phases, magnitudes, mask, metadata, reference: phase.geometry_reference() })
}

/// Run iQSM, iQSM+ or iQFM on `args` as `qsmxt run` runs it (the raw map, no referencing), and
/// write the result.
///
/// `echo_weighting` is iQSM's and iQFM's (`--echo-weighting`); iQSM+ has none.
pub(super) fn run_phase_net(
    args: &crate::cli::PhaseNetArgs,
    net: PhaseNet,
    echo_weighting: qsm_core::inversion::IqsmEchoWeighting,
) -> qsmxt_io::Result<()> {
    let (id, name) = net.id_and_name();
    qsmxt_io::pipeline::runner::prefetch_weights(id, name)?;
    let inputs = load_phase_net_inputs(args)?;
    let (nx, ny, nz) = inputs.metadata.dims;
    info!("{name} ({}x{}x{}, {} echo(es))", nx, ny, nz, inputs.phases.len());
    let phases: Vec<&[f64]> = inputs.phases.iter().map(Vec::as_slice).collect();
    let magnitudes: Vec<&[f64]> = inputs.magnitudes.iter().map(Vec::as_slice).collect();
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    qsmxt_io::settings::apply(&mut cfg, &args.tiling.0)?;
    let tile = cfg.dl_tile();
    let none = qsm_core::pipeline::QsmReference::None;
    let out = match net {
        PhaseNet::Iqsm => qsm_core::pipeline::run_iqsm(&phases, &magnitudes, &inputs.mask, &inputs.metadata, none, tile, echo_weighting),
        PhaseNet::IqsmPlus => qsm_core::pipeline::run_iqsm_plus(&phases, &magnitudes, &inputs.mask, &inputs.metadata, none, tile),
        PhaseNet::Iqfm => qsm_core::pipeline::run_iqfm(&phases, &magnitudes, &inputs.mask, &inputs.metadata, tile, echo_weighting),
    }.map_err(|e| QsmxtError::Config(format!("{name}: {e}")))?;
    save_nifti(&args.output, &out, &inputs.reference)?;
    let what = if net == PhaseNet::Iqfm { "Local field" } else { "Susceptibility map" };
    info!("{what} saved to {}", args.output.display());
    Ok(())
}

pub fn execute(cmd: InvertCommand) -> qsmxt_io::Result<()> {
    use qsmxt_engine::QsmAlgorithm as A;
    // COSMOS and STI take N inputs (and STI writes many outputs), so they do not fit the
    // single-input/single-output shape the rest of this match shares.
    let cmd = match cmd {
        InvertCommand::Cosmos(args) => return run_cosmos(args),
        InvertCommand::Sti(args) => return run_sti(args),
        InvertCommand::Iqsm(args) => return run_phase_net(&args.net, PhaseNet::Iqsm, args.echo_weighting.into()),
        InvertCommand::IqsmPlus(args) => return run_phase_net(&args, PhaseNet::IqsmPlus, Default::default()),
        other => other,
    };

    let (common, (chi_data, field_nifti)) = match cmd {
        InvertCommand::Rts(args) => methods::rts(args)?,
        InvertCommand::Tv(args) => methods::tv(args)?,
        InvertCommand::Tkd(args) => methods::tkd(args)?,
        InvertCommand::Tsvd(args) => methods::tsvd(args)?,
        InvertCommand::Ilsqr(args) => methods::ilsqr(args)?,
        InvertCommand::Star(args) => methods::star(args)?,
        InvertCommand::Lsqr(args) => methods::lsqr(args)?,
        InvertCommand::Heidi(args) => methods::heidi(args)?,
        InvertCommand::Tikhonov(args) => methods::tikhonov(args)?,
        InvertCommand::Nltv(args) => methods::nltv(args)?,
        InvertCommand::Ndi(args) => methods::ndi(args)?,
        InvertCommand::Fansi(args) => methods::fansi(args)?,
        InvertCommand::FansiTgv(args) => methods::fansi_tgv(args)?,
        InvertCommand::L1qsm(args) => methods::l1qsm(args)?,
        InvertCommand::Whqsm(args) => methods::whqsm(args)?,
        InvertCommand::Hdqsm(args) => methods::hdqsm(args)?,
        InvertCommand::AmpPe(args) => methods::amp_pe(args)?,
        InvertCommand::Medi(args) => methods::medi(args)?,
        InvertCommand::Tfi(args) => methods::tfi(args)?,
        InvertCommand::Tgv(args) => methods::tgv(args)?,
        InvertCommand::Xqsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Xqsm, "xQSM")?; (args.common, out) }
        InvertCommand::Qsmnet(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Qsmnet, "QSMnet")?; (args.common, out) }
        InvertCommand::QsmnetPlus(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::QsmnetPlus, "QSMnet+")?; (args.common, out) }
        InvertCommand::Qsmnetme(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Qsmnetme, "QSMnetME")?; (args.common, out) }
        InvertCommand::Autoqsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Autoqsm, "AutoQSM")?; (args.common, out) }
        InvertCommand::Qsmgan(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Qsmgan, "QSMGAN")?; (args.common, out) }
        InvertCommand::Ir2qsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Ir2qsm, "IR2QSM")?; (args.common, out) }
        InvertCommand::Lpcnn(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Lpcnn, "LPCNN")?; (args.common, out) }
        InvertCommand::ModlQsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::ModlQsm, "MoDL-QSM")?; (args.common, out) }
        InvertCommand::Nextqsm(args) => { let out = run_dl_inversion(&args.common, args.field_strength, A::Nextqsm, "NeXtQSM")?; (args.common, out) }
        // Returned above; repeated here only to keep the match exhaustive.
        InvertCommand::Cosmos(_) | InvertCommand::Sti(_) | InvertCommand::Iqsm(_)
        | InvertCommand::IqsmPlus(_) => unreachable!("dispatched above"),
    };

    save_nifti(&common.output, &chi_data, &field_nifti)?;
    info!("Susceptibility map saved to {}", common.output.display());
    Ok(())
}
