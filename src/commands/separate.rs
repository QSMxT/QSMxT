//! Standalone susceptibility source separation command.
//!
//! One subcommand per method (mirroring `qsmxt invert <algo>`): each exposes only the inputs and
//! parameters that method uses. Its parameters are its settings' `qsmxt run` flags, applied to
//! QSMxT's configuration as the run applies them, and the method's config is the one the run
//! builds ([`qsmxt_engine::bridge::to_separation_config`]). Multi-echo magnitude accepts either several 3D files or one 4D
//! file. Outputs are written as `{prefix}_desc-{paramagnetic,diamagnetic,total}_Chimap.nii`, the
//! pipeline's names (`bids::naming`).

use log::info;
use std::path::Path;
use qsm_core::io::NiftiData;
use qsm_core::pipeline::{
    SeparationInputs, SeparationConfig, SeparationResult, ScanMetadata, run_separation,
};
use super::common::{load_nifti, load_mask, save_nifti, load_echo_stack, load_multiecho_voxel_major};
use crate::cli::{
    SeparateCommand, SeparateCommonArgs, SeparateChiSepIlsqrArgs, SeparateChiSepMediArgs,
    SeparateChiSepNetArgs, SeparateDecomposeArgs, SeparateHcChisepArgs, SeparateR2starQsmArgs,
    SeparateSusepNetArgs, SeparateWavesepArgs,
};
use qsmxt_io::error::QsmxtError;

/// Write the three chi-separation maps under the output prefix.
fn save_result(output: &Path, result: &SeparationResult, reference: &NiftiData) -> qsmxt_io::Result<()> {
    use qsmxt_io::bids::naming;
    let para = naming::nifti_beside(output, &naming::chi_paramagnetic());
    let dia = naming::nifti_beside(output, &naming::chi_diamagnetic());
    let total = naming::nifti_beside(output, &naming::chi_total());
    // χ− is signed-negative in qsm-core (so χ_total = χ+ + χ−); the diamagnetic map is written as
    // its magnitude |χ−| so both source maps are positive. χ_total stays the signed net.
    let chi_dia: Vec<f64> = result.chi_neg.iter().map(|&v| v.abs()).collect();
    save_nifti(&para, &result.chi_pos, reference)?;
    save_nifti(&dia, &chi_dia, reference)?;
    save_nifti(&total, &result.chi_total, reference)?;
    info!("Saved paramagnetic → {}", para.display());
    info!("Saved diamagnetic  → {}", dia.display());
    info!("Saved total        → {}", total.display());
    Ok(())
}

/// Load the QSM + mask and build scan metadata from the common args + echo times.
fn load_common(
    c: &SeparateCommonArgs, echo_times: &[f64],
) -> qsmxt_io::Result<(NiftiData, Vec<u8>, ScanMetadata, usize)> {
    let qsm = load_nifti(&c.qsm)?;
    let (mask, _) = load_mask(&c.mask)?;
    let n_voxels = qsm.data.len();
    let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
    let metadata = qsmxt_engine::bridge::to_scan_metadata(
        qsm.dims, qsm.voxel_size, echo_times, c.field_strength, bdir, Some(qsm.affine),
    );
    Ok((qsm, mask, metadata, n_voxels))
}

/// Ensure the magnitude echo count matches the number of echo times.
fn check_echoes(n_echoes: usize, metadata: &ScanMetadata, method: &str) -> qsmxt_io::Result<()> {
    if metadata.echo_times.is_empty() {
        return Err(QsmxtError::Config(format!("{} needs --echo-times", method)));
    }
    if metadata.echo_times.len() != n_echoes {
        return Err(QsmxtError::Config(format!(
            "{}: {} echo times but the magnitude has {} echoes", method, metadata.echo_times.len(), n_echoes)));
    }
    Ok(())
}

/// QSMxT's configuration with `algorithm` as the separation and `settings` applied, as `qsmxt
/// run` applies them.
fn config(
    algorithm: qsmxt_engine::SeparationAlgorithm, settings: &[qsmxt_engine::schema::cli_apply::Given],
) -> qsmxt_io::Result<qsmxt_engine::PipelineConfig> {
    let mut cfg = qsmxt_engine::PipelineConfig::default();
    cfg.separation.algorithm = algorithm;
    qsmxt_io::settings::apply(&mut cfg, settings)?;
    Ok(cfg)
}

/// The separation config `qsmxt run` builds for `algorithm` with `settings` applied.
fn separation(
    algorithm: qsmxt_engine::SeparationAlgorithm, settings: &[qsmxt_engine::schema::cli_apply::Given],
) -> qsmxt_io::Result<SeparationConfig> {
    Ok(qsmxt_engine::bridge::to_separation_config(&config(algorithm, settings)?))
}

/// What a separation gives back: the output prefix, the reference NIfTI, and the result.
type Separated = (std::path::PathBuf, NiftiData, SeparationResult);

pub fn execute(cmd: SeparateCommand) -> qsmxt_io::Result<()> {
    // Each arm loads its inputs, runs the method, and yields (output prefix, reference NIfTI,
    // result); the three maps are written once at the end (mirrors `invert`'s single-save tail).
    let (output, reference, result) = match cmd {
        SeparateCommand::R2starQsm(args) => r2star_qsm(args)?,
        SeparateCommand::Decompose(args) => decompose(args)?,
        SeparateCommand::ChiSepIlsqr(args) => chi_sep_ilsqr(args)?,
        SeparateCommand::ChiSepMedi(args) => chi_sep_medi(args)?,
        SeparateCommand::Wavesep(args) => wavesep(args)?,
        SeparateCommand::HcChisep(args) => hc_chisep(args)?,
        SeparateCommand::SusepNet(args) => susep_net(args)?,
        SeparateCommand::ChiSepNet(args) => chi_sep_net(args)?,
    };
    save_result(&output, &result, &reference)?;
    Ok(())
}

fn r2star_qsm(args: SeparateR2starQsmArgs) -> qsmxt_io::Result<Separated> {
    let c = &args.common;
    let (qsm, mask, metadata, n) = load_common(c, &args.echo_times)?;
    info!("Chi-separation (r2star-qsm, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
    let r2star = args.r2star.as_ref().map(|p| load_nifti(p)).transpose()?;
    let mag = if args.magnitude.is_empty() {
        None
    } else {
        let (multi, ne) = load_multiecho_voxel_major(&args.magnitude, n)?;
        check_echoes(ne, &metadata, "r2star-qsm (R2* fit)")?;
        Some(multi)
    };
    if r2star.is_none() && mag.is_none() {
        return Err(QsmxtError::Config("r2star-qsm needs --r2star or --magnitude".into()));
    }
    let cfg = separation(qsmxt_engine::SeparationAlgorithm::R2starQsm, &args.settings.0)?;
    let inputs = SeparationInputs {
        local_field_ppm: &[], qsm: &qsm.data, qsm_per_echo: None, mask: &mask,
        r2prime: None, r2star: r2star.as_ref().map(|n| n.data.as_slice()),
        magnitude_rss: None, magnitude_multi: mag.as_deref(), se_magnitude_multi: None,
        n_std: None, mask_csf: None,
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((c.output.clone(), qsm, result))
}

fn decompose(args: SeparateDecomposeArgs) -> qsmxt_io::Result<Separated> {
    let c = &args.common;
    let (qsm, mask, metadata, n) = load_common(c, &args.echo_times)?;
    info!("Chi-separation (decompose, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
    let (multi, ne) = load_multiecho_voxel_major(&args.magnitude, n)?;
    check_echoes(ne, &metadata, "decompose")?;
    let (qsm_per_echo, ne_qsm) = load_multiecho_voxel_major(&args.qsm_per_echo, n)?;
    if ne_qsm != ne {
        return Err(QsmxtError::Config(format!(
            "decompose: --qsm-per-echo has {ne_qsm} echoes but the magnitude has {ne}")));
    }
    let cfg = separation(qsmxt_engine::SeparationAlgorithm::Decompose, &args.settings.0)?;
    let inputs = SeparationInputs {
        local_field_ppm: &[], qsm: &qsm.data, qsm_per_echo: Some(&qsm_per_echo), mask: &mask,
        r2prime: None, r2star: None, magnitude_rss: None,
        magnitude_multi: Some(&multi), se_magnitude_multi: None, n_std: None, mask_csf: None,
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((c.output.clone(), qsm, result))
}

fn chi_sep_ilsqr(args: SeparateChiSepIlsqrArgs) -> qsmxt_io::Result<Separated> {
    let c = &args.common;
    let (qsm, mask, metadata, _) = load_common(c, &[])?;
    info!("Chi-separation (chi-sep-ilsqr, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
    let local_field = load_nifti(&args.local_field)?;
    let r2prime = load_nifti(&args.r2prime)?;
    let cfg = separation(qsmxt_engine::SeparationAlgorithm::ChiSepIlsqr, &args.settings.0)?;
    let inputs = SeparationInputs {
        local_field_ppm: &local_field.data, qsm: &qsm.data, qsm_per_echo: None, mask: &mask,
        r2prime: Some(&r2prime.data), r2star: None,
        magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None, n_std: None, mask_csf: None,
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((c.output.clone(), qsm, result))
}

fn chi_sep_medi(args: SeparateChiSepMediArgs) -> qsmxt_io::Result<Separated> {
    let c = &args.common;
    let (qsm, mask, metadata, n) = load_common(c, &args.echo_times)?;
    info!("Chi-separation (chi-sep-medi, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
    let local_field = load_nifti(&args.local_field)?;
    let r2prime = load_nifti(&args.r2prime)?;
    let (echo_major, ne) = load_echo_stack(&args.magnitude, n)?;
    check_echoes(ne, &metadata, "chi-sep-medi")?;
    let mag_rss: Vec<f64> = (0..n)
        .map(|v| (0..ne).map(|e| echo_major[e * n + v].powi(2)).sum::<f64>().sqrt())
        .collect();
    let settings = config(qsmxt_engine::SeparationAlgorithm::ChiSepMedi, &args.settings.0)?;
    let (n_std, mask_csf) = match args.csf_mask.as_deref() {
        None => {
            let method = settings.separation.chi_sep_medi.csf_r2star;
            let (n_std, csf) = qsmxt_engine::stages::separation::chi_sep_medi_inputs(
                &echo_major, &args.echo_times, &mask, qsm.dims, qsm.voxel_size, method,
            ).map_err(|e| QsmxtError::Config(e.to_string()))?;
            info!("CSF mask of {} voxels from {} R2*", csf.iter().filter(|&&m| m != 0).count(), method.label());
            (n_std, csf)
        }
        Some(p) => {
            let n_std = qsm_core::utils::fit_ppm_complex_noise(&echo_major, ne);
            // An all-zero CSF mask turns the CSF term off in qsm-core, as the toolbox's
            // `mask_CSF = []` does.
            let csf = if p.as_os_str() == "none" {
                info!("No CSF mask: no CSF term");
                vec![0u8; n]
            } else {
                let (csf, _) = load_mask(p)?;
                if csf.len() != n {
                    return Err(QsmxtError::Config(format!(
                        "chi-sep-medi: the CSF mask has {} voxels, the QSM {}", csf.len(), n)));
                }
                info!("CSF mask of {} voxels from {}", csf.iter().filter(|&&m| m != 0).count(), p.display());
                csf
            };
            (n_std, csf)
        }
    };
    let cfg = qsmxt_engine::bridge::to_separation_config(&settings);
    let inputs = SeparationInputs {
        local_field_ppm: &local_field.data, qsm: &qsm.data, qsm_per_echo: None, mask: &mask,
        r2prime: Some(&r2prime.data), r2star: None,
        magnitude_rss: Some(&mag_rss), magnitude_multi: None, se_magnitude_multi: None,
        n_std: Some(&n_std), mask_csf: Some(&mask_csf),
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((c.output.clone(), qsm, result))
}

fn wavesep(args: SeparateWavesepArgs) -> qsmxt_io::Result<Separated> {
    let c = &args.common;
    let (qsm, mask, metadata, _) = load_common(c, &[])?;
    info!("Chi-separation (wavesep, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
    let r2prime = load_nifti(&args.r2prime)?;
    let cfg = separation(qsmxt_engine::SeparationAlgorithm::WaveSep, &args.settings.0)?;
    let inputs = SeparationInputs {
        local_field_ppm: &[], qsm: &qsm.data, qsm_per_echo: None, mask: &mask,
        r2prime: Some(&r2prime.data), r2star: None,
        magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None, n_std: None, mask_csf: None,
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((c.output.clone(), qsm, result))
}

fn hc_chisep(args: SeparateHcChisepArgs) -> qsmxt_io::Result<Separated> {
    let c = &args.common;
    let (qsm, mask, metadata, n) = load_common(c, &args.echo_times)?;
    info!("Chi-separation (hc-chisep, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
    let r2prime = load_nifti(&args.r2prime)?;
    let (multi, ne) = load_multiecho_voxel_major(&args.magnitude, n)?;
    check_echoes(ne, &metadata, "hc-chisep")?;
    let se_multi = if args.se_magnitude.is_empty() {
        None
    } else {
        Some(load_multiecho_voxel_major(&args.se_magnitude, n)?.0)
    };
    let mut cfg = separation(qsmxt_engine::SeparationAlgorithm::HcChisep, &args.settings.0)?;
    // The spin-echo acquisition's echo times are an input's, not a setting.
    cfg.hc_chisep.se_echo_times = args.se_echo_times.clone();
    let inputs = SeparationInputs {
        local_field_ppm: &[], qsm: &qsm.data, qsm_per_echo: None, mask: &mask,
        r2prime: Some(&r2prime.data), r2star: None, magnitude_rss: None,
        magnitude_multi: Some(&multi), se_magnitude_multi: se_multi.as_deref(), n_std: None, mask_csf: None,
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((c.output.clone(), qsm, result))
}

fn susep_net(args: SeparateSusepNetArgs) -> qsmxt_io::Result<Separated> {
    let c = &args.common;
    let (qsm, mask, metadata, _) = load_common(c, &[])?;
    info!("Chi-separation (susep-net, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
    qsmxt_io::pipeline::runner::prefetch_weights("susep-net", "susep-net")?;
    let local_field = load_nifti(&args.local_field)?;
    let r2prime = load_nifti(&args.r2prime)?;
    let cfg = separation(qsmxt_engine::SeparationAlgorithm::SusepNet, &args.settings.0)?;
    let inputs = SeparationInputs {
        local_field_ppm: &local_field.data, qsm: &qsm.data, qsm_per_echo: None, mask: &mask,
        r2prime: Some(&r2prime.data), r2star: None,
        magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None, n_std: None, mask_csf: None,
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((c.output.clone(), qsm, result))
}

fn chi_sep_net(args: SeparateChiSepNetArgs) -> qsmxt_io::Result<Separated> {
    use qsm_core::separation::ChiSepNetRoute;
    let cfg = separation(qsmxt_engine::SeparationAlgorithm::ChiSepNet, &args.settings.0)?;
    let chi_sepnet = &cfg.chi_sepnet;
    let route = chi_sepnet.route;
    // Before the weights download and the images load.
    chi_sepnet.validate().map_err(QsmxtError::Config)?;
    // The local field is the reference grid: the toolbox route reads no QSM.
    let local_field = load_nifti(&args.local_field)?;
    let (mask, _) = load_mask(&args.mask)?;
    let bdir = (args.b0_direction[0], args.b0_direction[1], args.b0_direction[2]);
    let metadata = qsmxt_engine::bridge::to_scan_metadata(
        local_field.dims, local_field.voxel_size, &[], args.field_strength, bdir, Some(local_field.affine),
    );
    info!("Chi-separation (chi-sepnet, {} route, {}x{}x{})", match route {
        ChiSepNetRoute::Toolbox => "toolbox", ChiSepNetRoute::Native => "native" },
        local_field.dims.0, local_field.dims.1, local_field.dims.2);
    let qsm = match (&args.qsm, route) {
        (Some(p), _) => Some(load_nifti(p)?),
        (None, ChiSepNetRoute::Native) => {
            return Err(QsmxtError::Config("chi-sepnet --chi-sepnet-route native needs --qsm".into()));
        }
        (None, ChiSepNetRoute::Toolbox) => None,
    };
    if route == ChiSepNetRoute::Native && args.r2prime.is_none() {
        return Err(QsmxtError::Config("chi-sepnet --chi-sepnet-route native needs --r2prime".into()));
    }
    let r2prime = args.r2prime.as_ref().map(|p| load_nifti(p)).transpose()?;
    let r2star = args.r2star.as_ref().map(|p| load_nifti(p)).transpose()?;
    qsmxt_io::pipeline::runner::prefetch_weights("chi-sepnet", "chi-sepnet")?;
    if route == ChiSepNetRoute::Toolbox {
        qsmxt_io::pipeline::runner::prefetch_weights("qsmnetme", "QSMnetME")?;
        if r2prime.is_none() {
            qsmxt_io::pipeline::runner::prefetch_weights("r2primenet", "R2PRIMEnet")?;
        }
    }
    // The toolbox route does not read the QSM; the dispatcher takes one of the grid's size.
    let zeros = vec![0.0; local_field.data.len()];
    let inputs = SeparationInputs {
        local_field_ppm: &local_field.data, qsm: qsm.as_ref().map_or(&zeros, |q| &q.data),
        qsm_per_echo: None, mask: &mask,
        r2prime: r2prime.as_ref().map(|m| m.data.as_slice()), r2star: r2star.as_ref().map(|m| m.data.as_slice()),
        magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None, n_std: None, mask_csf: None,
    };
    let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    Ok((args.output.clone(), local_field, result))
}
