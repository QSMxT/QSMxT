use log::info;
use super::common::{load_nifti, load_mask, save_nifti, save_mask};
use crate::cli::{
    BgremoveCommand, BgremoveCommonArgs, BgremoveBfrnetArgs, BgremoveSharpArgs, BgremoveVsharpArgs,
};

/// A local field (ppm), the geometry it is written with, and the mask it is defined on.
type Removed = (Vec<f64>, qsm_core::io::NiftiData, Vec<u8>);

/// What a background removal gives back: its common args, and the local field it made.
type Removal = (BgremoveCommonArgs, Removed);

/// Remove the background of `c.input` inside `c.mask` with `config` through qsm-core's pipeline
/// dispatcher ([`qsm_core::pipeline::run_bg_removal`]), as `qsmxt run` does. `scan` and
/// `magnitude` are the scan's field strength and echo times and every echo's magnitude, for the
/// methods that take them (PDF both, LBV the scan); `None` otherwise. Returns the local field (ppm), its geometry
/// and the mask it is defined on.
fn remove_with(
    c: &BgremoveCommonArgs, config: qsm_core::pipeline::BgRemovalConfig,
    scan: Option<&crate::cli::ScanParamArgs>, magnitude: &[std::path::PathBuf], name: &str,
) -> qsmxt_io::Result<Removed> {
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let (nx, ny, nz) = field_nifti.dims;
    let echo_times = scan.map(|s| s.echo_times.clone()).unwrap_or_default();
    let magnitude = if magnitude.is_empty() {
        None
    } else {
        let (stack, n_echoes) = super::common::load_echo_stack(magnitude, field_nifti.data.len())?;
        if n_echoes != echo_times.len() {
            return Err(qsmxt_io::error::QsmxtError::Config(format!(
                "{name} needs the magnitude of every echo: got {n_echoes} magnitude volume(s) for {} echo time(s)",
                echo_times.len())));
        }
        Some(stack)
    };
    info!("Background removal ({}, {}x{}x{})", name, nx, ny, nz);
    let metadata = qsm_core::pipeline::ScanMetadata {
        dims: field_nifti.dims,
        voxel_size: field_nifti.voxel_size,
        echo_times,
        // Only PDF (its noise map) and LBV (its tolerance's units) read it, with the echo times.
        field_strength: scan.map_or(f64::NAN, |s| s.field_strength),
        b0_direction: (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]),
        slice_geometry: None,
        affine: Some(field_nifti.affine),
    };
    let result = qsm_core::pipeline::run_bg_removal(
        &field_nifti.data, &mask, &metadata, &config, magnitude.as_deref(), &mut |_, _| {},
    ).map_err(|e| qsmxt_io::error::QsmxtError::Config(format!("{name}: {e}")))?;
    Ok((result.local_field_ppm, field_nifti, result.eroded_mask))
}

/// The background-removal config `qsmxt run` builds for `algorithm` with these settings flags
/// applied.
fn config(
    algorithm: qsmxt_engine::BfAlgorithm, settings: &[qsmxt_engine::schema::cli_apply::Given],
) -> qsmxt_io::Result<qsm_core::pipeline::BgRemovalConfig> {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.bg_removal.algorithm = algorithm;
    qsmxt_io::settings::apply(&mut cfg, settings)?;
    Ok(qsmxt_engine::to_pipeline_stages(&cfg).1)
}

/// HARPERELLA or iHARPERELLA of `args.input`: a total field in ppm through qsm-core's pipeline
/// dispatcher, or a wrapped echo in rad (with `--te` and `--field-strength`), which only the
/// method itself takes.
fn harperella_with(
    args: &HarperellaArgs, config: qsm_core::pipeline::BgRemovalConfig, name: &str,
) -> qsmxt_io::Result<(BgremoveCommonArgs, Removed)> {
    let common = BgremoveCommonArgs {
        input: args.input.clone(), mask: args.mask.clone(), output: args.output.clone(),
        b0_direction: vec![0.0, 0.0, 1.0], output_mask: args.output_mask.clone(),
    };
    let (Some(te_s), Some(b0_t)) = (args.te, args.field_strength) else {
        let out = remove_with(&common, config, None, &[], name)?;
        return Ok((common, out));
    };
    let phase_nifti = load_nifti(&args.input)?;
    let (mask, _) = load_mask(&args.mask)?;
    let grid = super::common::nifti_grid(&phase_nifti);
    info!("Background removal ({}, wrapped echo, {}x{}x{})", name, grid.nx(), grid.ny(), grid.nz());
    let input = qsm_core::bgremove::HarperellaInput::WrappedPhase { phase: &phase_nifti.data, te_s, b0_t };
    let (lf, em) = if config.algorithm == qsm_core::pipeline::BgRemovalAlgorithm::Harperella {
        qsm_core::bgremove::harperella(input, &mask, &grid, &config.harperella, |_, _| {})?
    } else {
        qsm_core::bgremove::iharperella(input, &mask, &grid, &config.iharperella, |_, _| {})?
    };
    Ok((common, (lf, phase_nifti, em)))
}

/// The (i)HARPERELLA arguments both subcommands share.
struct HarperellaArgs {
    input: std::path::PathBuf,
    mask: std::path::PathBuf,
    output: std::path::PathBuf,
    output_mask: Option<std::path::PathBuf>,
    te: Option<f64>,
    field_strength: Option<f64>,
}

pub fn execute(cmd: BgremoveCommand) -> qsmxt_io::Result<()> {
    use qsmxt_engine::BfAlgorithm as B;
    // iQFM reads every echo's phase rather than a field, so it does not share the match below.
    let cmd = match cmd {
        BgremoveCommand::Iqfm(args) => {
            return super::invert::run_phase_net(&args.net, super::invert::PhaseNet::Iqfm, args.echo_weighting.into())
        }
        other => other,
    };
    let (common, (local_field_data, field_nifti, eroded_mask)) = match cmd {
        BgremoveCommand::Vsharp(args) => vsharp(args)?,
        BgremoveCommand::Pdf(args) => {
            let out = remove_with(&args.common, config(B::Pdf, &args.settings.0)?, Some(&args.scan), &args.magnitude, "PDF")?;
            (args.common, out)
        }
        // LBV's tolerance is in radians over the echo spacing, so it takes the scan's field
        // strength and echo times.
        BgremoveCommand::Lbv(args) => {
            let out = remove_with(&args.common, config(B::Lbv, &args.settings.0)?, Some(&args.scan), &[], "LBV")?;
            (args.common, out)
        }
        BgremoveCommand::Ismv(args) => {
            let out = remove_with(&args.common, config(B::Ismv, &args.settings.0)?, None, &[], "iSMV")?;
            (args.common, out)
        }
        BgremoveCommand::Sharp(args) => sharp(args)?,
        BgremoveCommand::Resharp(args) => {
            let out = remove_with(&args.common, config(B::Resharp, &args.settings.0)?, None, &[], "RESHARP")?;
            (args.common, out)
        }
        BgremoveCommand::Bfrnet(args) => bfrnet(args)?,
        BgremoveCommand::Iqfm(_) => unreachable!("handled above"),
        BgremoveCommand::Harperella(args) => harperella_with(&HarperellaArgs {
            input: args.input, mask: args.mask, output: args.output, output_mask: args.output_mask,
            te: args.te, field_strength: args.field_strength,
        }, config(B::Harperella, &args.settings.0)?, "HARPERELLA")?,
        BgremoveCommand::Iharperella(args) => harperella_with(&HarperellaArgs {
            input: args.input, mask: args.mask, output: args.output, output_mask: args.output_mask,
            te: args.te, field_strength: args.field_strength,
        }, config(B::Iharperella, &args.settings.0)?, "iHARPERELLA")?,
    };

    save_nifti(&common.output, &local_field_data, &field_nifti)?;
    info!("Local field saved to {}", common.output.display());

    if let Some(ref mask_out) = common.output_mask {
        save_mask(mask_out, &eroded_mask, &field_nifti)?;
        info!("Eroded mask saved to {}", mask_out.display());
    }

    Ok(())
}

fn vsharp(args: BgremoveVsharpArgs) -> qsmxt_io::Result<Removal> {
    let mut config = config(qsmxt_engine::BfAlgorithm::Vsharp, &args.settings.0)?;
    // The SMV ball of one implementation or the other, for comparison with it: not a setting.
    if let Some(ball) = args.ball { config.vsharp.ball = ball.into(); }
    let out = remove_with(&args.common, config, None, &[], "V-SHARP")?;
    Ok((args.common, out))
}

fn sharp(args: BgremoveSharpArgs) -> qsmxt_io::Result<Removal> {
    let mut config = config(qsmxt_engine::BfAlgorithm::Sharp, &args.settings.0)?;
    // QSM.jl's variant, for comparison with it: not a setting. Its own defaults (18 × the
    // smallest voxel dimension, 0.05), which depend on the grid, stand for what is not given.
    if let Some(crate::cli::SharpVariantArg::Qsmjl) = args.variant {
        let d = qsm_core::bgremove::SharpParams::qsmjl(&super::common::nifti_grid(&load_nifti(&args.common.input)?));
        let given = |long: &str| args.settings.0.iter().any(|g| g.flag.long == long);
        config.sharp = qsm_core::bgremove::SharpParams {
            threshold: if given("sharp-threshold") { config.sharp.threshold } else { d.threshold },
            radius: if given("sharp-radius") { config.sharp.radius } else { d.radius },
            variant: d.variant,
        };
    }
    let out = remove_with(&args.common, config, None, &[], "SHARP")?;
    Ok((args.common, out))
}

fn bfrnet(args: BgremoveBfrnetArgs) -> qsmxt_io::Result<Removal> {
    // Fetch the ONNX weights with a download bar (cached under $QSM_MODEL_DIR / the cache).
    // On a build without the `dl` feature this errors here with a clear message.
    qsmxt_io::pipeline::runner::prefetch_weights("bfrnet", "bfrnet")?;
    let out = remove_with(&args.common, config(qsmxt_engine::BfAlgorithm::Bfrnet, &[])?, None, &[], "BFRnet")?;
    Ok((args.common, out))
}
