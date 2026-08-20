//! Standalone susceptibility source separation command.
//!
//! One subcommand per method (mirroring `qsmxt invert <algo>`): each exposes only the inputs and
//! parameters that method uses. Parameter defaults come from qsm-core's `Default` impls (the single
//! source of truth) via `unwrap_or`. Multi-echo magnitude accepts either several 3D files or one 4D
//! file. Outputs are written as `{prefix}_paramagnetic/diamagnetic/total.nii`.

use log::info;
use std::path::Path;
use qsm_core::io::NiftiData;
use qsm_core::pipeline::{
    SeparationInputs, SeparationConfig, SeparationAlgorithm, SeparationResult, ScanMetadata, run_separation,
};
use super::common::{load_nifti, load_mask, save_nifti, load_multiecho_voxel_major, rss_over_echoes};
use crate::cli::{SeparateCommand, SeparateCommonArgs};
use crate::error::QsmxtError;

/// Write the three chi-separation maps under the output prefix.
fn save_result(output: &Path, result: &SeparationResult, reference: &NiftiData) -> crate::Result<()> {
    let prefix = output.to_string_lossy();
    let para = format!("{}_paramagnetic.nii", prefix);
    let dia = format!("{}_diamagnetic.nii", prefix);
    let total = format!("{}_total.nii", prefix);
    // χ− is signed-negative in qsm-core (so χ_total = χ+ + χ−); the diamagnetic map is written as
    // its magnitude |χ−| so both source maps are positive. χ_total stays the signed net.
    let chi_dia: Vec<f64> = result.chi_neg.iter().map(|&v| v.abs()).collect();
    save_nifti(Path::new(&para), &result.chi_pos, reference)?;
    save_nifti(Path::new(&dia), &chi_dia, reference)?;
    save_nifti(Path::new(&total), &result.chi_total, reference)?;
    info!("Saved paramagnetic → {}", para);
    info!("Saved diamagnetic  → {}", dia);
    info!("Saved total        → {}", total);
    Ok(())
}

/// Load the QSM + mask and build scan metadata from the common args + echo times.
fn load_common(
    c: &SeparateCommonArgs, echo_times: &[f64],
) -> crate::Result<(NiftiData, Vec<u8>, ScanMetadata, usize)> {
    let qsm = load_nifti(&c.qsm)?;
    let (mask, _) = load_mask(&c.mask)?;
    let n_voxels = qsm.data.len();
    let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
    let metadata = qsmxt_config::bridge::to_scan_metadata(
        qsm.dims, qsm.voxel_size, echo_times, c.field_strength, bdir,
    );
    Ok((qsm, mask, metadata, n_voxels))
}

/// Ensure the magnitude echo count matches the number of echo times.
fn check_echoes(n_echoes: usize, metadata: &ScanMetadata, method: &str) -> crate::Result<()> {
    if metadata.echo_times.is_empty() {
        return Err(QsmxtError::Config(format!("{} needs --echo-times", method)));
    }
    if metadata.echo_times.len() != n_echoes {
        return Err(QsmxtError::Config(format!(
            "{}: {} echo times but the magnitude has {} echoes", method, metadata.echo_times.len(), n_echoes)));
    }
    Ok(())
}

pub fn execute(cmd: SeparateCommand) -> crate::Result<()> {
    // Each arm loads its inputs, runs the method, and yields (output prefix, reference NIfTI,
    // result); the three maps are written once at the end (mirrors `invert`'s single-save tail).
    let (output, reference, result) = match cmd {
        SeparateCommand::R2starQsm(args) => {
            let c = &args.common;
            let (qsm, mask, metadata, n) = load_common(c, &args.echo_times)?;
            info!("Chi-separation (r2star-qsm, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
            let d = qsm_core::separation::R2starQsmParams::default();
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
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::R2starQsm,
                r2star_qsm: qsm_core::separation::R2starQsmParams {
                    r_const_3t: args.r_const_3t.unwrap_or(d.r_const_3t), ..d
                },
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &[], qsm: &qsm.data, mask: &mask,
                r2prime: None, r2star: r2star.as_ref().map(|n| n.data.as_slice()),
                magnitude_rss: None, magnitude_multi: mag.as_deref(), se_magnitude_multi: None,
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
        SeparateCommand::Decompose(args) => {
            let c = &args.common;
            let (qsm, mask, metadata, n) = load_common(c, &args.echo_times)?;
            info!("Chi-separation (decompose, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
            let (multi, ne) = load_multiecho_voxel_major(&args.magnitude, n)?;
            check_echoes(ne, &metadata, "decompose")?;
            let d = qsm_core::separation::DecomposeParams::default();
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::Decompose,
                decompose: qsm_core::separation::DecomposeParams {
                    n_inner: args.n_inner.unwrap_or(d.n_inner),
                    chi_bound: args.chi_bound.unwrap_or(d.chi_bound),
                    max_lm_iter: args.max_lm_iter.unwrap_or(d.max_lm_iter),
                    ..d
                },
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &[], qsm: &qsm.data, mask: &mask,
                r2prime: None, r2star: None, magnitude_rss: None,
                magnitude_multi: Some(&multi), se_magnitude_multi: None,
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
        SeparateCommand::ChiSepIlsqr(args) => {
            let c = &args.common;
            let (qsm, mask, metadata, n) = load_common(c, &[])?;
            info!("Chi-separation (chi-sep-ilsqr, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
            let local_field = load_nifti(&args.local_field)?;
            let r2prime = load_nifti(&args.r2prime)?;
            let (multi, ne) = load_multiecho_voxel_major(&args.magnitude, n)?;
            let mag_rss = rss_over_echoes(&multi, n, ne);
            let d = qsm_core::separation::ChiSepIlsqrParams::default();
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::ChiSepIlsqr,
                chi_sep_ilsqr: qsm_core::separation::ChiSepIlsqrParams {
                    dr_pos: args.dr_pos.unwrap_or(d.dr_pos),
                    dr_neg: args.dr_neg.unwrap_or(d.dr_neg),
                    lambda1: args.lambda1.unwrap_or(d.lambda1),
                    percentage: args.percentage.unwrap_or(d.percentage),
                    r2p_min: args.r2p_min.unwrap_or(d.r2p_min),
                    r2p_max: args.r2p_max.unwrap_or(d.r2p_max),
                    max_iter: args.max_iter.unwrap_or(d.max_iter),
                    tol: args.tol.unwrap_or(d.tol),
                    cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
                    cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
                    ..d
                },
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &local_field.data, qsm: &qsm.data, mask: &mask,
                r2prime: Some(&r2prime.data), r2star: None,
                magnitude_rss: Some(&mag_rss), magnitude_multi: None, se_magnitude_multi: None,
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
        SeparateCommand::ChiSepMedi(args) => {
            let c = &args.common;
            let (qsm, mask, metadata, n) = load_common(c, &[])?;
            info!("Chi-separation (chi-sep-medi, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
            let local_field = load_nifti(&args.local_field)?;
            let r2prime = load_nifti(&args.r2prime)?;
            let (multi, ne) = load_multiecho_voxel_major(&args.magnitude, n)?;
            let mag_rss = rss_over_echoes(&multi, n, ne);
            let d = qsm_core::separation::ChiSepParams::default();
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::ChiSepMedi,
                chi_sep_medi: qsm_core::separation::ChiSepParams {
                    lambda_para: args.lambda_para.unwrap_or(d.lambda_para),
                    lambda_dia: args.lambda_dia.unwrap_or(d.lambda_dia),
                    lambda_cpl: args.lambda_cpl.unwrap_or(d.lambda_cpl),
                    dr_pos: args.dr_pos.unwrap_or(d.dr_pos),
                    dr_neg: args.dr_neg.unwrap_or(d.dr_neg),
                    percentage: args.percentage.unwrap_or(d.percentage),
                    cg_tol: args.cg_tol.unwrap_or(d.cg_tol),
                    cg_max_iter: args.cg_max_iter.unwrap_or(d.cg_max_iter),
                    max_iter: args.max_iter.unwrap_or(d.max_iter),
                    tol: args.tol.unwrap_or(d.tol),
                    ..d
                },
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &local_field.data, qsm: &qsm.data, mask: &mask,
                r2prime: Some(&r2prime.data), r2star: None,
                magnitude_rss: Some(&mag_rss), magnitude_multi: None, se_magnitude_multi: None,
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
        SeparateCommand::Wavesep(args) => {
            let c = &args.common;
            let (qsm, mask, metadata, _) = load_common(c, &[])?;
            info!("Chi-separation (wavesep, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
            let r2prime = load_nifti(&args.r2prime)?;
            let d = qsm_core::separation::WaveSepParams::default();
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::WaveSep,
                wavesep: qsm_core::separation::WaveSepParams {
                    dr_pos: args.dr_pos.unwrap_or(d.dr_pos),
                    dr_neg: args.dr_neg.unwrap_or(d.dr_neg),
                    alpha: args.alpha.unwrap_or(d.alpha),
                    lambda: args.lambda.unwrap_or(d.lambda),
                    wavelet_order: args.wavelet_order.unwrap_or(d.wavelet_order),
                    max_iter: args.max_iter.unwrap_or(d.max_iter),
                    tol: args.tol.unwrap_or(d.tol),
                },
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &[], qsm: &qsm.data, mask: &mask,
                r2prime: Some(&r2prime.data), r2star: None,
                magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None,
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
        SeparateCommand::HcChisep(args) => {
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
            let se_echo_times = args.se_echo_times.clone();
            let d = qsm_core::separation::HcChisepParams::default();
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::HcChisep,
                hc_chisep: qsm_core::separation::HcChisepParams {
                    se_echo_times,
                    dr_pos_3t: args.dr_pos_3t.unwrap_or(d.dr_pos_3t),
                    bin_hz: args.bin_hz.unwrap_or(d.bin_hz),
                    ..d
                },
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &[], qsm: &qsm.data, mask: &mask,
                r2prime: Some(&r2prime.data), r2star: None, magnitude_rss: None,
                magnitude_multi: Some(&multi), se_magnitude_multi: se_multi.as_deref(),
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
        SeparateCommand::SusepNet(args) => {
            let c = &args.common;
            let (qsm, mask, metadata, _) = load_common(c, &[])?;
            info!("Chi-separation (susep-net, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
            crate::pipeline::runner::prefetch_weights("susep-net", "susep-net")?;
            let local_field = load_nifti(&args.local_field)?;
            let r2prime = load_nifti(&args.r2prime)?;
            // SUSEP-Net has no user-tunable parameters; weights download on first use.
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::SusepNet,
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &local_field.data, qsm: &qsm.data, mask: &mask,
                r2prime: Some(&r2prime.data), r2star: None,
                magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None,
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
        SeparateCommand::ChiSepNet(args) => {
            let c = &args.common;
            let (qsm, mask, metadata, _) = load_common(c, &[])?;
            info!("Chi-separation (chi-sepnet, {}x{}x{})", qsm.dims.0, qsm.dims.1, qsm.dims.2);
            crate::pipeline::runner::prefetch_weights("chi-sepnet", "chi-sepnet")?;
            let local_field = load_nifti(&args.local_field)?;
            let r2prime = load_nifti(&args.r2prime)?;
            // χ-sepnet has no user-tunable parameters; weights download on first use.
            let cfg = SeparationConfig {
                algorithm: SeparationAlgorithm::ChiSepNet,
                ..Default::default()
            };
            let inputs = SeparationInputs {
                local_field_ppm: &local_field.data, qsm: &qsm.data, mask: &mask,
                r2prime: Some(&r2prime.data), r2star: None,
                magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None,
            };
            let result = run_separation(inputs, &metadata, &cfg, &mut |_, _| {})
                .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
            (c.output.clone(), qsm, result)
        }
    };
    save_result(&output, &result, &reference)?;
    Ok(())
}
