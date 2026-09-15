//! `qsmxt mask`: build, refine and combine binary masks from the command line.
//!
//! Every subcommand is the first link of a chain: it does its own operation, then applies the
//! `--op` refinements in order. What each op *means* is defined once, in qsm-core's
//! `apply_mask_ops` / `build_mask_section` — the same code the pipeline runs — so a mask built
//! here matches the one a `--mask` section would produce.

use log::info;
use qsm_core::io::NiftiData;
use qsm_core::pipeline::config::ScanMetadata;
use super::common::{load_mask, load_nifti, save_mask};
use crate::cli::{MaskCombineCliArgs, MaskCommand, MaskCommonArgs, MaskPresetArgs};
use crate::error::QsmxtError;
use crate::pipeline::config::{
    mask_preset_recipe, parse_mask_op, to_mask_ops, to_mask_sections, to_scan_metadata,
    MaskCombine, MaskOp, MaskRecipe, MaskSection, MaskingInput,
};

/// qsm-core's view of a volume: the mask ops only need dims and voxel size.
fn scan_meta(nifti: &NiftiData) -> ScanMetadata {
    to_scan_metadata(nifti.dims, nifti.voxel_size, &[], 0.0, (0.0, 0.0, 1.0))
}

fn dims_str(nifti: &NiftiData) -> String {
    format!("{}x{}x{}", nifti.dims.0, nifti.dims.1, nifti.dims.2)
}

/// Fetch the weights of any deep-learning op (HD-BET) before running, with a progress bar — or
/// fail clearly on a build without the `dl` feature.
fn prefetch_dl_weights(ops: &[MaskOp]) -> crate::Result<()> {
    for id in to_mask_ops(ops).iter().filter_map(|op| op.dl_model_id()) {
        crate::pipeline::runner::prefetch_weights(id, id)?;
    }
    Ok(())
}

/// Apply refinements in order through qsm-core. `magnitude` is what `signal-erode` gates on;
/// the generating subcommands pass their input (it is the magnitude), the mask-to-mask ones
/// pass what `--magnitude` gave them, if anything.
fn refine(mask: Vec<u8>, ops: &[MaskOp], reference: &NiftiData, magnitude: Option<&[f64]>) -> crate::Result<Vec<u8>> {
    if let Some(op) = ops.iter().find(|op| op.is_generator()) {
        return Err(QsmxtError::Config(format!(
            "--op {op} creates a mask rather than refining one; use the matching `qsmxt mask` subcommand",
        )));
    }
    if magnitude.is_none() && ops.iter().any(|op| matches!(op, MaskOp::SignalErode { .. })) {
        return Err(QsmxtError::Config(
            "--op signal-erode needs the magnitude: use it with the otsu/value/percentile/bet/hd-bet \
             subcommands, whose input is the magnitude image, or pass --magnitude to `mask and`/`mask or`".into(),
        ));
    }
    if ops.is_empty() {
        return Ok(mask);
    }
    prefetch_dl_weights(ops)?;
    // Only generators read `input_data`, and they were rejected above.
    qsm_core::pipeline::apply_mask_ops(mask, &to_mask_ops(ops), magnitude.unwrap_or(&[]), magnitude, &scan_meta(reference))
        .map_err(|e| QsmxtError::Config(format!("{e}")))
}

/// Parse and apply the `--op` chain.
fn refine_with(mask: Vec<u8>, ops: &[String], reference: &NiftiData, magnitude: Option<&[f64]>) -> crate::Result<Vec<u8>> {
    let parsed = ops.iter().map(|s| parse_mask_op(s)).collect::<Result<Vec<_>, _>>()?;
    refine(mask, &parsed, reference, magnitude)
}

/// A generating subcommand: run `generator` on the input image, then the `--op` chain. The input
/// doubles as the magnitude for `bet`/`hd-bet`/`signal-erode`, which is right when it is one.
fn generate(common: &MaskCommonArgs, generator: MaskOp) -> crate::Result<()> {
    let nifti = load_nifti(&common.input)?;
    if let MaskOp::Threshold { method: crate::pipeline::config::MaskThresholdMethod::Otsu, .. } = &generator {
        info!("Otsu threshold: {:.4}", qsm_core::utils::otsu_threshold(&nifti.data, 256));
    }
    info!("{} ({})", generator, dims_str(&nifti));
    prefetch_dl_weights(std::slice::from_ref(&generator))?;
    let section = MaskSection { input: MaskingInput::Magnitude, generator, refinements: vec![] };
    let core = to_mask_sections(&[section]);
    let mask = qsm_core::pipeline::build_mask_section(&core[0], &nifti.data, Some(&nifti.data), &scan_meta(&nifti))
        .map_err(|e| QsmxtError::Config(format!("{e}")))?;
    let mask = refine_with(mask, &common.ops, &nifti, Some(&nifti.data))?;
    save_and_log(common, &mask, &nifti)
}

/// A mask-to-mask subcommand: `op` on the input mask, then the `--op` chain.
fn refine_file(common: &MaskCommonArgs, op: MaskOp) -> crate::Result<()> {
    let (mask, nifti) = load_mask(&common.input)?;
    info!("{} ({})", op, dims_str(&nifti));
    let mut ops = vec![op];
    ops.extend(common.ops.iter().map(|s| parse_mask_op(s)).collect::<Result<Vec<_>, _>>()?);
    let mask = refine(mask, &ops, &nifti, None)?;
    save_and_log(common, &mask, &nifti)
}

/// The HD-BET op for `qsmxt mask hd-bet` (`--patch` wins over `--low-memory`).
fn hd_bet_op(args: &crate::cli::MaskHdBetArgs) -> crate::Result<MaskOp> {
    let mut spec = String::from("hd-bet");
    if let Some(p) = &args.patch { spec += &format!(":{p}"); } else if args.low_memory { spec += ":low-memory"; }
    if args.tta { spec += ":tta"; }
    if let Some(step) = args.tile_step { spec += &format!(":step={step}"); }
    Ok(parse_mask_op(&spec)?)
}

/// Run a whole `--mask-preset` recipe on files: each section reads the input image, or
/// `--quality` for the phase-quality sections when it is given; the sections fold with the
/// recipe's combine mode; the recipe's own refinements run on the result, then the `--op` chain.
fn preset(args: &MaskPresetArgs, recipe: MaskRecipe) -> crate::Result<()> {
    let magnitude = load_nifti(&args.common.input)?;
    let quality = args.quality.as_deref().map(load_nifti).transpose()?;
    if let Some(q) = &quality {
        if q.dims != magnitude.dims {
            return Err(QsmxtError::Config(format!(
                "--quality {} is {:?} but {} is {:?}",
                args.quality.as_ref().unwrap().display(), q.dims, args.common.input.display(), magnitude.dims,
            )));
        }
    }

    let meta = scan_meta(&magnitude);
    let core = to_mask_sections(&recipe.sections);
    let mut combined: Option<Vec<u8>> = None;
    for (section, core_section) in recipe.sections.iter().zip(&core) {
        let (image, what): (&[f64], &str) = match (section.input, &quality) {
            (MaskingInput::PhaseQuality, Some(q)) => (&q.data, "the phase-quality map"),
            (MaskingInput::PhaseQuality, None) => (&magnitude.data, "the input image (no --quality given)"),
            _ => (&magnitude.data, "the input image"),
        };
        info!("{} of {}", section.generator, what);
        prefetch_dl_weights(&section.all_ops())?;
        let mask = qsm_core::pipeline::build_mask_section(core_section, image, Some(&magnitude.data), &meta)
            .map_err(|e| QsmxtError::Config(format!("{e}")))?;
        combined = Some(match combined {
            None => mask,
            Some(mut acc) => { recipe.combine.accumulate(&mut acc, &mask); acc }
        });
    }
    let mask = combined.expect("presets have at least one section");
    if recipe.sections.len() > 1 {
        info!("Combined {} sections with {}", recipe.sections.len(), recipe.combine);
    }
    let mask = refine(mask, &recipe.refinements, &magnitude, Some(&magnitude.data))?;
    let mask = refine_with(mask, &args.common.ops, &magnitude, Some(&magnitude.data))?;
    save_and_log(&args.common, &mask, &magnitude)
}

/// Fold masks together with `mode`, then apply `--op` refinements to the result — the standalone
/// form of what `qsmxt run --mask-combine` does between `--mask` sections.
fn combine(args: MaskCombineCliArgs, mode: MaskCombine) -> crate::Result<()> {
    let (mut combined, reference) = load_mask(&args.inputs[0])?;
    for path in &args.inputs[1..] {
        let (mask, nifti) = load_mask(path)?;
        if nifti.dims != reference.dims {
            return Err(QsmxtError::Config(format!(
                "{} is {:?} but {} is {:?} — masks must be on the same grid",
                args.inputs[0].display(), reference.dims, path.display(), nifti.dims,
            )));
        }
        mode.accumulate(&mut combined, &mask);
    }

    // Only `signal-erode` reads it, and only if asked for.
    let magnitude = args.magnitude.as_deref().map(load_nifti).transpose()?;
    if let Some(mag) = &magnitude {
        if mag.dims != reference.dims {
            return Err(QsmxtError::Config(format!(
                "--magnitude {} is {:?} but the masks are {:?}",
                args.magnitude.as_ref().unwrap().display(), mag.dims, reference.dims,
            )));
        }
    }

    let before: usize = combined.iter().map(|&m| m as usize).sum();
    info!("Combined {} masks with {} ({} voxels)", args.inputs.len(), mode, before);
    let mask = refine_with(combined, &args.ops, &reference, magnitude.as_ref().map(|m| m.data.as_slice()))?;

    save_mask(&args.output, &mask, &reference)?;
    log_saved(&args.output, &mask);
    Ok(())
}

pub fn execute(cmd: MaskCommand) -> crate::Result<()> {
    use crate::pipeline::config::MaskThresholdMethod;
    match cmd {
        MaskCommand::Otsu(args) => generate(&args.common, MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }),
        MaskCommand::Value(args) => generate(&args.common, MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(args.threshold) }),
        MaskCommand::Percentile(args) => generate(&args.common, MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(args.percentile) }),
        MaskCommand::Bet(args) => generate(&args.common, MaskOp::Bet { fractional_intensity: args.fractional_intensity }),
        MaskCommand::HdBet(args) => { let op = hd_bet_op(&args)?; generate(&args.common, op) }
        MaskCommand::Preset(args) => { let recipe = mask_preset_recipe(args.preset); preset(&args, recipe) }
        MaskCommand::Robust(args) => preset(
            &MaskPresetArgs { preset: crate::cli::MaskPresetArg::RobustThreshold, common: args.common, quality: None },
            mask_preset_recipe(crate::cli::MaskPresetArg::RobustThreshold),
        ),
        MaskCommand::Erode(args) => refine_file(&args.common, MaskOp::Erode { iterations: args.iterations }),
        MaskCommand::Dilate(args) => refine_file(&args.common, MaskOp::Dilate { iterations: args.iterations }),
        MaskCommand::Close(args) => refine_file(&args.common, MaskOp::Close { radius: args.radius }),
        MaskCommand::FillHoles(args) => refine_file(&args.common, MaskOp::FillHoles { max_size: args.max_size }),
        MaskCommand::Smooth(args) => refine_file(&args.common, MaskOp::GaussianSmooth { sigma_mm: args.sigma }),
        MaskCommand::And(args) => combine(args, MaskCombine::And),
        MaskCommand::Or(args) => combine(args, MaskCombine::Or),
    }
}

fn log_saved(path: &std::path::Path, mask: &[u8]) {
    let count: usize = mask.iter().map(|&m| m as usize).sum();
    info!("Mask saved to {} ({} voxels, {:.1}%)", path.display(), count, 100.0 * count as f64 / mask.len() as f64);
}

fn save_and_log(common: &MaskCommonArgs, mask: &[u8], nifti: &NiftiData) -> crate::Result<()> {
    save_mask(&common.output, mask, nifti)?;
    log_saved(&common.output, mask);
    Ok(())
}
