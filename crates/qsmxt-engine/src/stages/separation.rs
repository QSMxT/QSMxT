//! Susceptibility source separation (χ-separation): the susceptibility map split into its
//! paramagnetic (χ+) and diamagnetic (χ−) sources.

use crate::config::PipelineConfig;
use crate::enums::SeparationAlgorithm;
use crate::error::EngineError;
use crate::observe::Observer;
use crate::scan::{scan_metadata, RunMetadata};

/// The `chi_separation` step's parameters. A network's patch is there only when the network is
/// the method and the patch is not its default, so a run at the default keeps the cache key it
/// had before the setting existed.
pub fn params(config: &PipelineConfig, meta: &RunMetadata, has_mese: bool) -> serde_json::Value {
    let s = &config.separation;
    let d = crate::config::SeparationConfig::default();
    let mut params = serde_json::json!({
        "algorithm": format!("{}", s.algorithm),
        "echo_times": meta.echo_times,
        "field_strength": meta.field_strength,
        "has_mese": has_mese,
        "custom_qsm": s.custom_qsm_tool,
        "custom_r2prime": s.custom_r2prime_tool,
        // `domain` versions where the fit runs, so maps fitted over the whole brain mask by an
        // older release (the eroded rim taken as χ = 0) are recomputed rather than reused.
        "domain": "support",
    });
    let patch = match s.algorithm {
        SeparationAlgorithm::ChiSepNet if s.chi_sepnet != d.chi_sepnet => Some(serde_json::json!(s.chi_sepnet.patch)),
        SeparationAlgorithm::SusepNet if s.susep_net != d.susep_net => Some(serde_json::json!(s.susep_net.patch)),
        _ => None,
    };
    if let Some(patch) = patch {
        params["patch"] = patch;
    }
    // DECOMPOSE given a per-echo QSM's echoes' maps rather than the one QSM at every echo.
    if s.algorithm == SeparationAlgorithm::Decompose && s.custom_qsm_tool.is_none()
        && crate::bridge::per_echo(config, meta.n_echoes)
    {
        params["qsm_per_echo"] = serde_json::json!(true);
    }
    // DECOMPOSE's unwrapping only unwraps since QSM.rs#345: it no longer also bounds the χ search
    // at the echo spacing (now `chi_bound = "echo-spacing"`), so maps an older release fitted
    // with it on are recomputed rather than reused.
    if s.algorithm == SeparationAlgorithm::Decompose && s.decompose.unwrap_phase {
        params["unwrap"] = serde_json::json!("echoes");
    }
    params
}

/// Why the selected network's patch cannot be used, if it cannot.
pub fn check_patch(config: &PipelineConfig) -> Result<(), EngineError> {
    let s = &config.separation;
    match s.algorithm {
        SeparationAlgorithm::ChiSepNet => s.chi_sepnet.validate(),
        SeparationAlgorithm::SusepNet => s.susep_net.validate(),
        _ => Ok(()),
    }
    .map_err(EngineError::Config)
}

/// Whether the step reconstructs the QSM it separates: χ-sep iLSQR, unless a QSM is supplied, is
/// initialised with an iLSQR QSM of the same local field, as the χ-separation toolbox makes it
/// (QSM.rs `ChiSepIlsqrParams::qsm_ilsqr`).
pub fn reconstructs_qsm(config: &PipelineConfig) -> bool {
    config.separation.algorithm == SeparationAlgorithm::ChiSepIlsqr && config.separation.custom_qsm_tool.is_none()
}

/// The QSM χ-sep iLSQR is initialised with ([`reconstructs_qsm`]): iLSQR of `local_field` (ppm)
/// inside `mask`, with `separation.chi_sep_ilsqr.qsm_ilsqr`. Unreferenced, in ppm.
pub fn initial_qsm(
    local_field: &[f64], mask: &[u8], meta: &RunMetadata, config: &PipelineConfig, observer: &mut dyn Observer,
) -> Result<Vec<f64>, EngineError> {
    let (_, _, mut inversion, _) = crate::bridge::to_pipeline_stages(config);
    inversion.algorithm = qsm_core::pipeline::InversionAlgorithm::Ilsqr;
    inversion.ilsqr = crate::bridge::to_separation_config(config).chi_sep_ilsqr.qsm_ilsqr;
    qsm_core::pipeline::run_dipole_inversion(
        local_field, mask, &scan_metadata(meta), &inversion, None, &mut |done, total| observer.progress(done, total),
    ).map_err(|e| EngineError::Config(format!("chi-separation: the iLSQR QSM: {}", e)))
}

/// Whether `algorithm` fits the local field.
pub fn uses_field(algorithm: SeparationAlgorithm) -> bool {
    matches!(algorithm,
        SeparationAlgorithm::ChiSepIlsqr | SeparationAlgorithm::ChiSepMedi
        | SeparationAlgorithm::SusepNet | SeparationAlgorithm::ChiSepNet)
}

/// What the inputs at hand lack for `algorithm`, by name: empty when it can run.
pub fn missing_inputs(
    algorithm: SeparationAlgorithm, inputs: &qsm_core::pipeline::SeparationInputs<'_>,
) -> Vec<&'static str> {
    let field = !inputs.local_field_ppm.is_empty();
    let (r2star, r2prime) = (inputs.r2star.is_some(), inputs.r2prime.is_some());
    let (multi, rss) = (inputs.magnitude_multi.is_some(), inputs.magnitude_rss.is_some());
    let needs: Vec<(bool, &'static str)> = match algorithm {
        SeparationAlgorithm::R2starQsm => vec![(r2star || multi, "R2* or multi-echo magnitude")],
        SeparationAlgorithm::Decompose => vec![(multi, "multi-echo magnitude")],
        // The χ-separation toolbox's `chi_sep_iLSQR` takes the magnitude but never uses it.
        SeparationAlgorithm::ChiSepIlsqr => vec![(field, "local field"), (r2prime, "R2'")],
        SeparationAlgorithm::ChiSepMedi => vec![
            (field, "local field"), (r2prime, "R2'"), (rss, "magnitude"),
            (inputs.n_std.is_some(), "noise map"), (inputs.mask_csf.is_some(), "CSF mask"),
        ],
        SeparationAlgorithm::WaveSep => vec![(r2prime, "R2'")],
        SeparationAlgorithm::HcChisep => vec![(r2prime, "R2'"), (multi, "multi-echo magnitude")],
        // SUSEP-Net / χ-sepnet (deep learning) consume [QSM, R2', local field] → [χ+, χ−].
        SeparationAlgorithm::SusepNet | SeparationAlgorithm::ChiSepNet =>
            vec![(field, "local field"), (r2prime, "R2'")],
    };
    needs.into_iter().filter(|(have, _)| !have).map(|(_, what)| what).collect()
}

/// The voxels χ-separation is fitted on and writes.
///
/// `published` is the support referencing wrote for the pipeline's own χ (`desc-qsm_mask`); with
/// none — a bring-your-own χ — it is the brain mask less the voxels `chi` leaves undefined (the
/// rule of [`crate::referencing::defined_support`]). `local_field`, for the methods that fit it,
/// limits the support further to where the field is defined: a two-pass χ is the union of both
/// passes' supports, but the local field is the main pass's alone.
pub fn separation_support(
    brain: &[u8], published: Option<&[u8]>, chi: &[f64], local_field: Option<&[f64]>,
) -> Vec<u8> {
    let mut support: Vec<u8> = match published {
        Some(p) => p.iter().zip(brain).map(|(&p, &b)| u8::from(p != 0 && b != 0)).collect(),
        None => crate::referencing::defined_support(&[brain], chi),
    };
    if let Some(field) = local_field {
        for (s, &f) in support.iter_mut().zip(field) {
            if f == 0.0 || !f.is_finite() {
                *s = 0;
            }
        }
    }
    support
}

/// `map` at each of `n_echoes` echoes, voxel-major `(n_voxels, n_echoes)`: the per-echo QSM
/// DECOMPOSE takes, from the pipeline's one QSM.
pub fn repeat_per_echo(map: &[f64], n_echoes: usize) -> Vec<f64> {
    map.iter().flat_map(|&v| std::iter::repeat_n(v, n_echoes)).collect()
}

/// χ-sep MEDI's noise map and CSF mask, as the χ-separation toolbox's script makes them: `N_std`
/// from every echo's magnitude (MEDI's `Fit_ppm_complex` noise), and the CSF mask from ARLO R2*
/// inside `mask` (MEDI's `extract_CSF`, at its defaults). `echo_major` holds the echo magnitudes,
/// one volume of `dims` voxels after another, at `echo_times` (s).
pub fn chi_sep_medi_inputs(
    echo_major: &[f64], echo_times: &[f64], mask: &[u8], dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
) -> Result<(Vec<f64>, Vec<u8>), EngineError> {
    let n_std = qsm_core::utils::fit_ppm_complex_noise(echo_major, echo_times.len());
    let r2star = qsm_core::utils::r2star::arlo(echo_major, echo_times, mask.len())
        .map_err(|e| EngineError::Config(format!("chi-separation (MEDI): the CSF mask's R2*: {e}")))?;
    let (nx, ny, nz) = dims;
    let (vx, vy, vz) = voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    let d = qsm_core::inversion::MediParams::default();
    let csf = qsm_core::inversion::extract_csf(&r2star, mask, &grid, d.csf_r2s_threshold, d.csf_erode);
    Ok((n_std, csf))
}

/// Write 0 wherever `support` is 0.
pub fn zero_outside(support: &[u8], map: &mut [f64]) {
    for (v, &s) in map.iter_mut().zip(support) {
        if s == 0 {
            *v = 0.0;
        }
    }
}

/// The separated maps: χ+, |χ−| and the net χ, each 0 outside the support it was fitted on.
pub struct Separated {
    pub paramagnetic: Vec<f64>,
    /// The diamagnetic map as a magnitude, so both source maps are positive.
    pub diamagnetic: Vec<f64>,
    /// The signed net susceptibility, χ+ + χ−.
    pub total: Vec<f64>,
}

/// Separate `inputs` (already restricted to [`separation_support`]) with the configured method;
/// `se_echo_times` are the MESE's, for the method that reads its echoes. A network's weights
/// must be ready.
pub fn separate(
    inputs: qsm_core::pipeline::SeparationInputs<'_>, se_echo_times: Option<&[f64]>, meta: &RunMetadata,
    config: &PipelineConfig, observer: &mut dyn Observer,
) -> Result<Separated, EngineError> {
    let support = inputs.mask;
    check_patch(config)?;
    let mut sep_config = crate::bridge::to_separation_config(config);
    if let Some(tes) = se_echo_times {
        sep_config.hc_chisep.se_echo_times = tes.to_vec();
    }
    let mut result = qsm_core::pipeline::run_separation(
        inputs, &scan_metadata(meta), &sep_config, &mut |done, total| observer.progress(done, total),
    ).map_err(|e| EngineError::Config(format!("chi-separation: {}", e)))?;
    // Not every method zeroes outside the mask it is given (the networks predict everywhere).
    for map in [&mut result.chi_pos, &mut result.chi_neg, &mut result.chi_total] {
        zero_outside(support, map);
    }
    // χ− is signed-negative in qsm-core (so χ_total = χ+ + χ−); the diamagnetic map is written as
    // its magnitude |χ−|. χ_total stays the signed net susceptibility.
    let diamagnetic = result.chi_neg.iter().map(|&v| v.abs()).collect();
    Ok(Separated { paramagnetic: result.chi_pos, diamagnetic, total: result.chi_total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separation_support_rules() {
        let brain = [1u8, 1, 1, 1, 0];
        let chi = [0.1, 0.0, 0.2, 0.3, 0.4];
        // The published support wins over χ's own zeros (a referenced voxel can land on 0.0), and
        // never reaches past the brain mask.
        assert_eq!(separation_support(&brain, Some(&[1, 1, 0, 1, 1]), &chi, None), vec![1, 1, 0, 1, 0]);
        // No published support (bring-your-own χ): the brain mask less χ's undefined voxels.
        assert_eq!(separation_support(&brain, None, &chi, None), vec![1, 0, 1, 1, 0]);
        // The field-based methods also need the local field.
        let field = [0.01, 0.01, 0.01, 0.0, 0.01];
        assert_eq!(separation_support(&brain, Some(&[1, 1, 1, 1, 0]), &chi, Some(&field)), vec![1, 1, 1, 0, 0]);
    }

    /// The cache key carries a network's patch only when it is that network's run and not its
    /// default; the default keeps the key of a run made before the setting existed.
    #[test]
    fn the_key_carries_a_non_default_patch_only() {
        let meta = crate::stages::magnitude::tests::meta_4x4x4();
        let mut c = PipelineConfig::default();
        let key = |c: &PipelineConfig| params(c, &meta, false);
        for alg in [SeparationAlgorithm::ChiSepNet, SeparationAlgorithm::SusepNet] {
            c.separation.algorithm = alg;
            assert!(key(&c).get("patch").is_none(), "{alg}");
        }
        c.separation.chi_sepnet.patch = [128, 128, 64];
        c.separation.susep_net.patch = Some([64, 64, 64]);
        c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        assert_eq!(key(&c)["patch"], serde_json::json!([128, 128, 64]));
        c.separation.algorithm = SeparationAlgorithm::SusepNet;
        assert_eq!(key(&c)["patch"], serde_json::json!([64, 64, 64]));
        c.separation.algorithm = SeparationAlgorithm::WaveSep;
        assert!(key(&c).get("patch").is_none(), "another method's key ignores the patches");
    }

    #[test]
    fn a_patch_the_network_cannot_take_is_refused() {
        let mut c = PipelineConfig::default();
        c.separation.chi_sepnet.patch = [128, 120, 64];
        c.separation.susep_net.patch = Some([128, 128, 4]);
        assert!(check_patch(&c).is_ok(), "not the method");
        c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        let e = check_patch(&c).unwrap_err().to_string();
        assert!(e.contains("χ-sepnet patch H = 120 must be a positive multiple of 16"), "{e}");
        c.separation.algorithm = SeparationAlgorithm::SusepNet;
        let e = check_patch(&c).unwrap_err().to_string();
        assert!(e.contains("SUSEP-Net patch W = 4 must be a positive multiple of 8"), "{e}");
        // The browser's are fine, and the configs map onto qsm-core's defaults.
        let b = PipelineConfig::browser();
        assert!(b.separation.chi_sepnet.validate().is_ok() && b.separation.susep_net.validate().is_ok());
        let d = crate::config::SeparationConfig::default();
        assert_eq!(d.chi_sepnet.params(), qsm_core::separation::ChiSepNetParams::default());
        assert_eq!(d.susep_net.params(), qsm_core::separation::SusepNetParams::default());
    }

    #[test]
    fn a_method_names_every_input_it_lacks() {
        let (qsm, mask) = ([0.0; 4], [1u8; 4]);
        let mut inputs = qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &[], qsm: &qsm, qsm_per_echo: None, mask: &mask, r2prime: None, r2star: None,
            magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None, n_std: None, mask_csf: None,
        };
        assert_eq!(missing_inputs(SeparationAlgorithm::ChiSepIlsqr, &inputs), ["local field", "R2'"]);
        assert_eq!(missing_inputs(SeparationAlgorithm::ChiSepMedi, &inputs),
                   ["local field", "R2'", "magnitude", "noise map", "CSF mask"]);
        let r2prime = [1.0; 4];
        inputs.r2prime = Some(&r2prime);
        assert_eq!(missing_inputs(SeparationAlgorithm::WaveSep, &inputs), Vec::<&str>::new());
        assert_eq!(missing_inputs(SeparationAlgorithm::R2starQsm, &inputs), ["R2* or multi-echo magnitude"]);
    }

    /// DECOMPOSE's cached maps from before unwrapping stopped bounding the χ search are not
    /// reused with unwrapping on; with it off, and for the χ bound, the config's own values key it.
    #[test]
    fn decomposes_unwrapping_versions_its_cache_key() {
        let meta = crate::stages::magnitude::tests::meta_4x4x4();
        let mut c = PipelineConfig::default();
        c.separation.algorithm = SeparationAlgorithm::Decompose;
        assert!(params(&c, &meta, false).get("unwrap").is_none());
        c.separation.decompose.unwrap_phase = true;
        assert_eq!(params(&c, &meta, false)["unwrap"], "echoes");
        let key = |c: &PipelineConfig| crate::cache::step_cache_key("chi_separation", None, &params(c, &meta, false), c);
        let fixed = key(&c);
        c.separation.decompose.chi_bound = crate::ChiBoundChoice::EchoSpacing;
        assert_ne!(key(&c), fixed);
    }

    #[test]
    fn decompose_gets_the_qsm_at_every_echo() {
        assert_eq!(repeat_per_echo(&[1.0, 2.0], 3), [1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
    }
}
