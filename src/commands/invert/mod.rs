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

/// An inversion config for `algorithm`, with qsm-core's settings for every other method.
fn config_for(algorithm: qsm_core::pipeline::InversionAlgorithm) -> qsm_core::pipeline::InversionConfig {
    qsm_core::pipeline::InversionConfig { algorithm, ..Default::default() }
}

/// Run a deep-learning dipole inversion (the DL models are not exposed as standalone functions).
/// Weights are downloaded on first use. AutoQSM and NeXtQSM are single-step and expect the TOTAL
/// field as input; the others take a local (tissue) field.
fn run_dl_inversion(
    c: &InvertCommonArgs, field_strength: f64,
    algorithm: qsm_core::pipeline::InversionAlgorithm, name: &str,
) -> qsmxt_io::Result<(Vec<f64>, qsm_core::io::NiftiData)> {
    // Opt-in overlap-tiling: presence of --tile-size enables it; halo defaults to 8.
    let tile = c.tiling_params.tile_size.map(|core| (core, c.tiling_params.tile_halo.unwrap_or(8)));
    let config = qsm_core::pipeline::InversionConfig { algorithm, tile, ..Default::default() };
    invert_with(c, field_strength, &[], Magnitude::None, config, name)
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
    let tile = args.tiling_params.tile_size.map(|core| (core, args.tiling_params.tile_halo.unwrap_or(8)));
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

/// Build `LsqrQsmParams` from the shared `--lsqr-*` group.
///
/// Defaults come from [`LsqrConfig`] — the same struct `qsmxt run` starts from — so the two
/// surfaces cannot drift apart. `b0` and `mask_output` are the dispatcher's (as in `qsmxt run`).
fn lsqr_params(args: &crate::cli::LsqrParamArgs) -> qsm_core::inversion::LsqrQsmParams {
    let d = qsmxt_engine::LsqrConfig::default();
    qsm_core::inversion::LsqrQsmParams {
        // `None` here is meaningful — it selects qsm-core's field-strength-scaled default — so an
        // unset flag keeps the `None` rather than collapsing to a concrete 3T number.
        residual_weighting: args.lsqr_residual_weighting.or(d.residual_weighting),
        fit_global_offset: d.fit_global_offset && !args.no_lsqr_global_offset,
        tol: args.lsqr_tol.unwrap_or(d.tol),
        max_iter: args.lsqr_max_iter.unwrap_or(d.max_iter),
        ..Default::default()
    }
}

/// QSM.jl's padding from `--pad X Y Z` (clap takes exactly three), or the default.
fn pad3(given: Option<&[i64]>, default: [i64; 3]) -> [i64; 3] {
    match given {
        Some(&[x, y, z]) => [x, y, z],
        _ => default,
    }
}

/// STAR-QSM's settings from `qsmxt invert star`'s flags. The field strength and echo times are
/// the scan's; qsm-core's dispatcher fills them in.
fn star_params(args: &crate::cli::InvertStarArgs) -> qsm_core::inversion::StarParams {
    let d = qsm_core::inversion::StarParams::default();
    qsm_core::inversion::StarParams {
        pad: args.pad.as_deref().map_or(d.pad, |p| [p[0], p[1], p[2]]),
        te: args.te.map_or(d.te, Into::into),
        ..d
    }
}

/// Build `HeidiParams` from the `--heidi-*` group.
///
/// [`HeidiConfig`] is the pipeline's flattening of qsm-core's optional denoise struct into a
/// switch plus three values; taking the defaults from it keeps `invert heidi` and
/// `run --qsm-algorithm heidi` on one set of numbers.
fn heidi_params(args: &crate::cli::HeidiParamArgs) -> qsm_core::inversion::HeidiParams {
    let d = qsmxt_engine::HeidiConfig::default();
    qsm_core::inversion::HeidiParams {
        cone_threshold: args.heidi_cone_threshold.unwrap_or(d.cone_threshold),
        gradient_threshold: args.heidi_gradient_threshold.unwrap_or(d.gradient_threshold),
        apply_laplacian_correction: d.apply_laplacian_correction
            && !args.no_heidi_laplacian_correction,
        laplacian_threshold: args.heidi_laplacian_threshold.unwrap_or(d.laplacian_threshold),
        gradient_mask_floor: args.heidi_gradient_mask_floor.unwrap_or(d.gradient_mask_floor),
        continuation_steps: args.heidi_continuation_steps.unwrap_or(d.continuation_steps),
        inner_iterations: args.heidi_inner_iterations.unwrap_or(d.inner_iterations),
        mu_min: args.heidi_mu_min.unwrap_or(d.mu_min),
        tol: args.heidi_tol.unwrap_or(d.tol),
        denoise: (d.denoise && !args.no_heidi_denoise).then(|| {
            qsm_core::utils::AnisotropicDiffusionParams {
                iterations: args.heidi_denoise_iterations.unwrap_or(d.denoise_iterations),
                time_step: args.heidi_denoise_time_step.unwrap_or(d.denoise_time_step),
                conductance: args.heidi_denoise_conductance.unwrap_or(d.denoise_conductance),
                reference_itk: d.denoise_reference_itk || args.heidi_denoise_reference_itk,
            }
        }),
        te: args.heidi_te.unwrap_or(d.te).into(),
        // Field strength and echo times are the scan's; the dispatcher fills them in.
        ..Default::default()
    }
}

pub fn execute(cmd: InvertCommand) -> qsmxt_io::Result<()> {
    use qsm_core::pipeline::InversionAlgorithm as A;
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

#[cfg(test)]
mod tests {
    use super::{heidi_params, lsqr_params, star_params};
    use crate::cli::{HeidiParamArgs, LsqrParamArgs};

    /// With no flags set, the `invert` surface must reproduce qsm-core's own defaults — the same
    /// parameters `qsmxt run --qsm-algorithm lsqr` would build. A hand-written `unwrap_or` chain
    /// is exactly where a default silently drifts from the library's.
    #[test]
    fn unset_flags_give_the_qsm_core_defaults() {
        let d = qsm_core::inversion::LsqrQsmParams::default();
        let p = lsqr_params(&LsqrParamArgs::default());
        assert_eq!(p.residual_weighting, d.residual_weighting);
        assert_eq!(p.fit_global_offset, d.fit_global_offset);
        assert_eq!(p.tol, d.tol);
        assert_eq!(p.max_iter, d.max_iter);

        let h = qsm_core::inversion::HeidiParams::default();
        let p = heidi_params(&HeidiParamArgs::default());
        assert_eq!(p.cone_threshold, h.cone_threshold);
        assert_eq!(p.gradient_threshold, h.gradient_threshold);
        assert_eq!(p.apply_laplacian_correction, h.apply_laplacian_correction);
        assert_eq!(p.laplacian_threshold, h.laplacian_threshold);
        assert_eq!(p.gradient_mask_floor, h.gradient_mask_floor);
        assert_eq!(p.continuation_steps, h.continuation_steps);
        assert_eq!(p.inner_iterations, h.inner_iterations);
        assert_eq!(p.mu_min, h.mu_min);
        assert_eq!(p.tol, h.tol);
        assert_eq!(p.te, h.te);
        let (a, b) = (p.denoise.unwrap(), h.denoise.unwrap());
        assert_eq!((a.iterations, a.time_step, a.conductance), (b.iterations, b.time_step, b.conductance));
    }

    /// `qsmxt invert star` with no flags gives qsm-core's STAR-QSM defaults; `--pad` and `--te`
    /// reach its parameters.
    #[test]
    fn star_flags_map_to_its_params() {
        use clap::Parser;
        let parse = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "invert", "star", "f.nii", "-m", "m.nii", "-o", "o.nii",
                                "--field-strength", "3", "--echo-times", "0.01", "0.02"];
            argv.extend(extra);
            match crate::cli::Cli::try_parse_from(argv).unwrap().command {
                crate::cli::Command::Invert { command: crate::cli::InvertCommand::Star(a) } => a,
                _ => panic!("expected invert star"),
            }
        };
        let (p, d) = (star_params(&parse(&[])), qsm_core::inversion::StarParams::default());
        assert_eq!((p.pad, p.te), (d.pad, d.te));
        let p = star_params(&parse(&["--pad", "2", "4", "6", "--te", "spacing"]));
        assert_eq!((p.pad, p.te), ([2, 4, 6], qsm_core::EchoTime::Spacing));
    }

    /// `--lsqr-residual-weighting` is `Option<f64>` in qsm-core because `None` means
    /// "scale with field strength" — collapsing it to a concrete number would silently pin the
    /// weight to 3T behaviour on every other scanner.
    #[test]
    fn an_unset_residual_weighting_stays_none() {
        assert_eq!(lsqr_params(&LsqrParamArgs::default()).residual_weighting, None);
        let explicit = LsqrParamArgs { lsqr_residual_weighting: Some(0.0), ..Default::default() };
        assert_eq!(lsqr_params(&explicit).residual_weighting, Some(0.0));
    }

    /// The denoise switch is a negative flag over a `Some(..)` default: `--no-heidi-denoise` has
    /// to clear the whole struct, not just zero its fields.
    #[test]
    fn no_denoise_clears_the_diffusion_params() {
        let off = HeidiParamArgs { no_heidi_denoise: true, ..Default::default() };
        assert!(heidi_params(&off).denoise.is_none());
        let tuned = HeidiParamArgs { heidi_denoise_iterations: Some(9), ..Default::default() };
        assert_eq!(heidi_params(&tuned).denoise.unwrap().iterations, 9);
    }
}
