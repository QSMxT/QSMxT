use log::info;
use super::common::{load_nifti, load_mask, save_nifti, save_mask};
use crate::cli::{BgremoveCommand, BgremoveCommonArgs};

/// A local field (ppm), the geometry it is written with, and the mask it is defined on.
type Removed = (Vec<f64>, qsm_core::io::NiftiData, Vec<u8>);

/// Remove the background of `c.input` inside `c.mask` with `config` through qsm-core's pipeline
/// dispatcher ([`qsm_core::pipeline::run_bg_removal`]), as `qsmxt run` does. `scan` and
/// `magnitude` are the scan's field strength and echo times and every echo's magnitude, for the
/// method that takes them (PDF); `None` otherwise. Returns the local field (ppm), its geometry
/// and the mask it is defined on.
fn remove_with(
    c: &BgremoveCommonArgs, config: qsm_core::pipeline::BgRemovalConfig,
    scan: Option<&crate::cli::ScanParamArgs>, magnitude: &[std::path::PathBuf], name: &str,
) -> crate::Result<Removed> {
    let field_nifti = load_nifti(&c.input)?;
    let (mask, _) = load_mask(&c.mask)?;
    let (nx, ny, nz) = field_nifti.dims;
    let echo_times = scan.map(|s| s.echo_times.clone()).unwrap_or_default();
    let magnitude = if magnitude.is_empty() {
        None
    } else {
        let (stack, n_echoes) = super::common::load_echo_stack(magnitude, field_nifti.data.len())?;
        if n_echoes != echo_times.len() {
            return Err(crate::error::QsmxtError::Config(format!(
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
        // Only PDF reads it (for its noise map), and takes it with the echo times.
        field_strength: scan.map_or(f64::NAN, |s| s.field_strength),
        b0_direction: (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]),
        slice_geometry: None,
        affine: Some(field_nifti.affine),
    };
    let result = qsm_core::pipeline::run_bg_removal(
        &field_nifti.data, &mask, &metadata, &config, magnitude.as_deref(), &mut |_, _| {},
    ).map_err(|e| crate::error::QsmxtError::Config(format!("{name}: {e}")))?;
    Ok((result.local_field_ppm, field_nifti, result.eroded_mask))
}

/// A background-removal config for `algorithm`, with qsm-core's settings for every other method.
fn config_for(algorithm: qsm_core::pipeline::BgRemovalAlgorithm) -> qsm_core::pipeline::BgRemovalConfig {
    qsm_core::pipeline::BgRemovalConfig { algorithm, ..Default::default() }
}

/// HARPERELLA or iHARPERELLA of `args.input`: a total field in ppm through qsm-core's pipeline
/// dispatcher, or a wrapped echo in rad (with `--te` and `--field-strength`), which only the
/// method itself takes.
fn harperella_with(
    args: &HarperellaArgs, config: qsm_core::pipeline::BgRemovalConfig, name: &str,
) -> crate::Result<(BgremoveCommonArgs, Removed)> {
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

pub fn execute(cmd: BgremoveCommand) -> crate::Result<()> {
    use qsm_core::bgremove as bg;
    use qsm_core::pipeline::BgRemovalAlgorithm as A;
    let (common, (local_field_data, field_nifti, eroded_mask)) = match cmd {
        BgremoveCommand::Vsharp(args) => {
            // QSMxT's defaults (qsm-core's), the same as `qsmxt run`'s.
            let d = qsmxt_engine::VsharpConfig::default();
            let grid_cfg = qsmxt_engine::VsharpConfig {
                grid: args.grid.map(Into::into).unwrap_or(d.grid),
                grid_spacing: args.grid_spacing.unwrap_or(d.grid_spacing),
                ..d.clone()
            };
            let vsharp = bg::VsharpParams {
                threshold: args.threshold.unwrap_or(d.threshold),
                max_radius: args.max_radius.unwrap_or(d.max_radius),
                min_radius: args.min_radius.unwrap_or(d.min_radius),
                radius_step: args.radius_step.unwrap_or(d.radius_step),
                deconvolve: args.deconvolve.unwrap_or(d.deconvolve),
                ball: bg::VsharpBall::default(),
                grid: grid_cfg.core_grid(),
            };
            let config = qsm_core::pipeline::BgRemovalConfig { vsharp, ..config_for(A::Vsharp) };
            let out = remove_with(&args.common, config, None, &[], "V-SHARP")?;
            (args.common, out)
        }
        BgremoveCommand::Pdf(args) => {
            let d = bg::PdfParams::default();
            let pdf = bg::PdfParams {
                tol: args.tol.unwrap_or(d.tol),
                n_cg: args.n_cg.unwrap_or(d.n_cg),
                space: args.space.map_or(d.space, Into::into),
                n_pad: args.n_pad.unwrap_or(d.n_pad),
            };
            let config = qsm_core::pipeline::BgRemovalConfig { pdf, ..config_for(A::Pdf) };
            let out = remove_with(&args.common, config, Some(&args.scan), &args.magnitude, "PDF")?;
            (args.common, out)
        }
        BgremoveCommand::Lbv(args) => {
            let d = bg::LbvParams::default();
            let lbv = bg::LbvParams { tol: args.tol.unwrap_or(d.tol), ..d };
            let config = qsm_core::pipeline::BgRemovalConfig { lbv, ..config_for(A::Lbv) };
            let out = remove_with(&args.common, config, None, &[], "LBV")?;
            (args.common, out)
        }
        BgremoveCommand::Ismv(args) => {
            let d = bg::IsmvParams::default();
            let ismv = bg::IsmvParams {
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                radius: args.radius.unwrap_or(d.radius),
            };
            let config = qsm_core::pipeline::BgRemovalConfig { ismv, ..config_for(A::Ismv) };
            let out = remove_with(&args.common, config, None, &[], "iSMV")?;
            (args.common, out)
        }
        BgremoveCommand::Sharp(args) => {
            let d = bg::SharpParams::default();
            let sharp = bg::SharpParams {
                threshold: args.threshold.unwrap_or(d.threshold),
                radius: args.radius.unwrap_or(d.radius),
                ..d
            };
            let config = qsm_core::pipeline::BgRemovalConfig { sharp, ..config_for(A::Sharp) };
            let out = remove_with(&args.common, config, None, &[], "SHARP")?;
            (args.common, out)
        }
        BgremoveCommand::Resharp(args) => {
            let d = bg::ResharpParams::default();
            let resharp = bg::ResharpParams {
                radius: args.radius.unwrap_or(d.radius),
                tik_reg: args.tik_reg.unwrap_or(d.tik_reg),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
            };
            let config = qsm_core::pipeline::BgRemovalConfig { resharp, ..config_for(A::Resharp) };
            let out = remove_with(&args.common, config, None, &[], "RESHARP")?;
            (args.common, out)
        }
        BgremoveCommand::Bfrnet(args) => {
            // Fetch the ONNX weights with a download bar (cached under $QSM_MODEL_DIR / the cache).
            // On a build without the `dl` feature this errors here with a clear message.
            crate::pipeline::runner::prefetch_weights("bfrnet", "bfrnet")?;
            let out = remove_with(&args.common, config_for(A::Bfrnet), None, &[], "BFRnet")?;
            (args.common, out)
        }
        BgremoveCommand::Harperella(args) => {
            let d = bg::HarperellaParams::default();
            let harperella = bg::HarperellaParams {
                radius: args.radius.unwrap_or(d.radius),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
                pad: args.pad.as_deref().map_or(d.pad, |p| [p[0], p[1], p[2]]),
            };
            let config = qsm_core::pipeline::BgRemovalConfig { harperella, ..config_for(A::Harperella) };
            harperella_with(&HarperellaArgs {
                input: args.input, mask: args.mask, output: args.output, output_mask: args.output_mask,
                te: args.te, field_strength: args.field_strength,
            }, config, "HARPERELLA")?
        }
        BgremoveCommand::Iharperella(args) => {
            let d = bg::IharperellaParams::default();
            let iharperella = bg::IharperellaParams {
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
                pad: args.pad.as_deref().map_or(d.pad, |p| [p[0], p[1], p[2]]),
            };
            let config = qsm_core::pipeline::BgRemovalConfig { iharperella, ..config_for(A::Iharperella) };
            harperella_with(&HarperellaArgs {
                input: args.input, mask: args.mask, output: args.output, output_mask: args.output_mask,
                te: args.te, field_strength: args.field_strength,
            }, config, "iHARPERELLA")?
        }
    };

    save_nifti(&common.output, &local_field_data, &field_nifti)?;
    info!("Local field saved to {}", common.output.display());

    if let Some(ref mask_out) = common.output_mask {
        save_mask(mask_out, &eroded_mask, &field_nifti)?;
        info!("Eroded mask saved to {}", mask_out.display());
    }

    Ok(())
}
