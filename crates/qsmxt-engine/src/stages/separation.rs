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
        SeparationAlgorithm::ChiSepIlsqr | SeparationAlgorithm::ChiSepMedi =>
            vec![(field, "local field"), (r2prime, "R2'"), (rss, "magnitude")],
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
        assert!(e.contains("multiple of 16 (got 120)"), "{e}");
        c.separation.algorithm = SeparationAlgorithm::SusepNet;
        let e = check_patch(&c).unwrap_err().to_string();
        assert!(e.contains("multiple of 8 (got 4)"), "{e}");
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
            local_field_ppm: &[], qsm: &qsm, qsm_echoes: None, mask: &mask, r2prime: None, r2star: None,
            magnitude_rss: None, magnitude_multi: None, se_magnitude_multi: None,
        };
        assert_eq!(missing_inputs(SeparationAlgorithm::ChiSepIlsqr, &inputs), ["local field", "R2'", "magnitude"]);
        let r2prime = [1.0; 4];
        inputs.r2prime = Some(&r2prime);
        assert_eq!(missing_inputs(SeparationAlgorithm::WaveSep, &inputs), Vec::<&str>::new());
        assert_eq!(missing_inputs(SeparationAlgorithm::R2starQsm, &inputs), ["R2* or multi-echo magnitude"]);
    }
}
