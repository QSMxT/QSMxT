use serde::{Deserialize, Serialize};
use crate::enums::*;
use crate::masking::*;

// ─── Macro for algorithm parameter structs ───
// Generates a serde struct with Default sourced from qsm-core.

macro_rules! param_config {
    ($name:ident from $core:path { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(default)]
        pub struct $name { $(pub $field: $ty),* }

        impl Default for $name {
            fn default() -> Self {
                #[allow(unused)]
                let p = <$core>::default();
                Self { $($field: p.$field as $ty),* }
            }
        }
    };
}

// ─── Algorithm parameter configs (each ~1-3 lines) ───

param_config!(RtsConfig from qsm_core::inversion::RtsParams {
    delta: f64, mu: f64, rho: f64, tol: f64, max_iter: usize, lsmr_iter: usize
});
param_config!(TvConfig from qsm_core::inversion::TvParams {
    lambda: f64, rho: f64, tol: f64, max_iter: usize
});
param_config!(TkdConfig from qsm_core::inversion::TkdParams { threshold: f64 });
param_config!(NltvConfig from qsm_core::inversion::NltvParams {
    lambda: f64, mu: f64, tol: f64, max_iter: usize, newton_iter: usize
});
param_config!(NdiConfig from qsm_core::inversion::NdiParams {
    tau: f64, alpha: f64, max_iter: usize, phase_scale: f64
});
param_config!(FansiConfig from qsm_core::inversion::FansiParams {
    alpha1: f64, mu1: f64, mu2: f64, alpha0: f64, mu0: f64, max_iter: usize,
    tol_update: f64, tol_delta: f64, phase_scale: f64
});
param_config!(L1qsmConfig from qsm_core::inversion::L1QsmParams {
    alpha1: f64, mu1: f64, mu2: f64, mu3: f64, lambda: f64, max_iter: usize,
    tol_update: f64, tol_delta: f64, phase_scale: f64
});
param_config!(WhqsmConfig from qsm_core::inversion::WhQsmParams {
    alpha1: f64, mu1: f64, mu2: f64, beta: f64, muh: f64, max_iter: usize,
    tol_update: f64, tol_delta: f64, phase_scale: f64
});
param_config!(HdqsmConfig from qsm_core::inversion::HdQsmParams {
    alpha_l2: f64, mu1_l2: f64, mu2: f64, max_iter_l1: usize, max_iter_l2: usize, tol_update: f64
});
param_config!(MediConfig from qsm_core::inversion::MediParams {
    lambda: f64, merit: bool, smv: bool, smv_radius: f64, data_weighting: i32,
    percentage: f64, cg_tol: f64, cg_max_iter: usize, max_iter: usize, tol: f64
});
param_config!(TfiConfig from qsm_core::inversion::TfiParams {
    lambda: f64, precond: f64, merit: bool, data_weighting: i32,
    percentage: f64, cg_tol: f64, cg_max_iter: usize, max_iter: usize, tol: f64
});
param_config!(AmpPeConfig from qsm_core::inversion::AmpPeParams {
    wave_order: usize, nlevel: usize, wave_pec: f64, simulated_te: f64,
    max_linearization_ite: usize, b0: f64, gyro_ratio: f64, damp_rate_sig: f64,
    damp_rate_par: f64, max_pe_spar_ite: usize, max_pe_est_ite: usize,
    cvg_thd: f64, tikhonov_beta: f64
});
param_config!(IlsqrConfig from qsm_core::inversion::IlsqrParams { tol: f64, max_iter: usize });

// ─── Chi-separation method params (cf/b0/se_echo_times are set from scan metadata) ───
param_config!(ChiSepIlsqrConfig from qsm_core::separation::ChiSepIlsqrParams {
    dr_pos: f64, dr_neg: f64, lambda1: f64, percentage: f64, r2p_min: f64, r2p_max: f64,
    max_iter: usize, tol: f64, cg_max_iter: usize, cg_tol: f64
});
param_config!(ChiSepMediConfig from qsm_core::separation::ChiSepParams {
    lambda_para: f64, lambda_dia: f64, lambda_cpl: f64, dr_pos: f64, dr_neg: f64,
    percentage: f64, cg_tol: f64, cg_max_iter: usize, max_iter: usize, tol: f64
});
param_config!(R2starQsmConfig from qsm_core::separation::R2starQsmParams { r_const_3t: f64 });
param_config!(WaveSepConfig from qsm_core::separation::WaveSepParams {
    dr_pos: f64, dr_neg: f64, alpha: f64, lambda: f64, wavelet_order: usize, max_iter: usize, tol: f64
});
param_config!(DecomposeConfig from qsm_core::separation::DecomposeParams {
    n_inner: usize, chi_bound: f64, max_lm_iter: usize
});
param_config!(HcChisepConfig from qsm_core::separation::HcChisepParams {
    dr_pos_3t: f64, bin_hz: f64
});

/// Chi-separation configuration (algorithm selector + per-method params).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SeparationConfig {
    pub algorithm: SeparationAlgorithm,
    pub chi_sep_ilsqr: ChiSepIlsqrConfig,
    pub chi_sep_medi: ChiSepMediConfig,
    pub r2star_qsm: R2starQsmConfig,
    pub wavesep: WaveSepConfig,
    pub decompose: DecomposeConfig,
    pub hc_chisep: HcChisepConfig,
    /// Bring-your-own input maps from `<bids>/derivatives/<tool>/…` (mirrors
    /// `masking.custom_mask_tool`). When set, the pipeline prefers the custom map over
    /// computing it. Empty/None = compute from the pipeline.
    #[serde(default)]
    pub custom_qsm_tool: Option<String>,
    #[serde(default)]
    pub custom_r2_tool: Option<String>,
    #[serde(default)]
    pub custom_r2prime_tool: Option<String>,
}
impl Default for SeparationConfig {
    fn default() -> Self {
        Self {
            algorithm: SeparationAlgorithm::R2starQsm,
            chi_sep_ilsqr: ChiSepIlsqrConfig::default(),
            chi_sep_medi: ChiSepMediConfig::default(),
            r2star_qsm: R2starQsmConfig::default(),
            wavesep: WaveSepConfig::default(),
            decompose: DecomposeConfig::default(),
            hc_chisep: HcChisepConfig::default(),
            custom_qsm_tool: None,
            custom_r2_tool: None,
            custom_r2prime_tool: None,
        }
    }
}

/// Force the relaxometry outputs the *selected* chi-separation method depends on.
///
/// Requirements are method-specific: `r2star-qsm` needs R2*; `decompose` needs neither; the
/// R2'-based methods (chi-sep-ilsqr/medi, wavesep, hc-chisep) need R2' (= R2* − R2), which in turn
/// needs R2 and R2* unless a custom R2'/R2 map is supplied. Enabling chi-separation turns on exactly
/// those maps. Keeps a `--do-chisep` run self-consistent, and is what the TUI validates against (it
/// must not let the user disable a map the current method requires).
pub fn enforce_separation_dependencies(config: &mut PipelineConfig) {
    if config.pipeline.do_chi_separation {
        match config.separation.algorithm {
            // Uses R2* directly (fit from the GRE magnitude); no R2/R2' needed.
            SeparationAlgorithm::R2starQsm => config.pipeline.do_r2starmap = true,
            // Fits the multi-echo magnitude signal directly; no relaxometry maps needed.
            SeparationAlgorithm::Decompose => {}
            // R2'-based methods: need R2' (unless a custom R2' map is supplied).
            SeparationAlgorithm::ChiSepIlsqr
            | SeparationAlgorithm::ChiSepMedi
            | SeparationAlgorithm::WaveSep
            | SeparationAlgorithm::HcChisep => {
                if config.separation.custom_r2prime_tool.is_none() {
                    config.pipeline.do_r2primemap = true;
                }
            }
        }
    }
    // R2' = R2* − R2 → computing it (no custom map) requires R2 and R2*.
    if config.pipeline.do_r2primemap && config.separation.custom_r2prime_tool.is_none() {
        if config.separation.custom_r2_tool.is_none() {
            config.pipeline.do_r2map = true;
        }
        config.pipeline.do_r2starmap = true;
    }
}
param_config!(VsharpConfig from qsm_core::bgremove::VsharpParams {
    threshold: f64, max_radius: f64, min_radius: f64
});
param_config!(PdfConfig from qsm_core::bgremove::PdfParams { tol: f64 });
param_config!(LbvConfig from qsm_core::bgremove::LbvParams { tol: f64 });
param_config!(IsmvConfig from qsm_core::bgremove::IsmvParams {
    tol: f64, max_iter: usize, radius: f64
});
param_config!(SharpConfig from qsm_core::bgremove::SharpParams { threshold: f64, radius: f64 });
param_config!(ResharpConfig from qsm_core::bgremove::ResharpParams {
    radius: f64, tik_reg: f64, tol: f64, max_iter: usize
});
param_config!(HarperellaConfig from qsm_core::bgremove::HarperellaParams {
    radius: f64, max_iter: usize, tol: f64
});
param_config!(BetConfig from qsm_core::bet::BetParams {
    fractional_intensity: f64, smoothness: f64, gradient_threshold: f64,
    iterations: usize, subdivisions: usize
});
param_config!(HomogeneityConfig from qsm_core::utils::HomogeneityParams {
    sigma_mm: f64, nbox: usize
});
param_config!(LinearFitConfig from qsm_core::utils::LinearFitParams {
    estimate_offset: bool, reliability_threshold_percentile: f64
});

// Tikhonov regularization operator (mirrors qsm_core::inversion::Regularization)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TikhonovReg { Identity, Gradient, Laplacian }

// Tikhonov config (special: carries the regularization operator enum)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TikhonovConfig {
    pub lambda: f64,
    pub reg: TikhonovReg,
}
impl Default for TikhonovConfig {
    fn default() -> Self {
        let p = qsm_core::inversion::TikhonovParams::default();
        Self {
            lambda: p.lambda,
            reg: match p.reg {
                qsm_core::inversion::Regularization::Identity => TikhonovReg::Identity,
                qsm_core::inversion::Regularization::Gradient => TikhonovReg::Gradient,
                qsm_core::inversion::Regularization::Laplacian => TikhonovReg::Laplacian,
            },
        }
    }
}

// MCPC-3D-S phase-offset smoothing sigma (voxels)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Mcpc3dsConfig {
    pub sigma: [f64; 3],
}
impl Default for Mcpc3dsConfig {
    fn default() -> Self {
        Self { sigma: qsm_core::utils::PhaseOffsetParams::default().sigma }
    }
}

// TGV needs special handling (f32→f64 conversion)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TgvConfig {
    pub iterations: usize,
    pub erosions: usize,
    pub alpha0: f64,
    pub alpha1: f64,
    pub step_size: f64,
    pub tol: f64,
}
impl Default for TgvConfig {
    fn default() -> Self {
        // Widen f32→f64 via the canonical decimal so e.g. 0.001f32 stays "0.001"
        // rather than the noisy exact double 0.0010000000474974513.
        let widen = |x: f32| x.to_string().parse::<f64>().unwrap_or(x as f64);
        let p = qsm_core::inversion::TgvParams::default();
        Self {
            iterations: p.iterations, erosions: p.erosions,
            alpha0: widen(p.alpha0), alpha1: widen(p.alpha1),
            step_size: widen(p.step_size), tol: widen(p.tol),
        }
    }
}

// QSMART config
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct QsmartConfig {
    pub ilsqr_tol: f64,
    pub ilsqr_max_iter: usize,
    /// Bottom-hat sphere radius for vasculature detection, in mm (converted to voxels at run time)
    pub vasc_sphere_radius: i32,
    pub sdf_spatial_radius: i32,
    /// Inner dipole inversion algorithm used for both QSMART stages
    pub inversion: QsmAlgorithm,
    // SDF kernel sigmas (voxels)
    pub sdf_sigma1_stage1: f64,
    pub sdf_sigma2_stage1: f64,
    pub sdf_sigma1_stage2: f64,
    pub sdf_sigma2_stage2: f64,
    /// SDF proximity lower limit
    pub sdf_lower_lim: f64,
    /// SDF curvature constant
    pub sdf_curv_constant: f64,
    // Frangi vesselness, vessel radii in mm (converted to voxels at run time)
    pub frangi_scale_min: f64,
    pub frangi_scale_max: f64,
    pub frangi_scale_ratio: f64,
    /// Frangi C noise threshold (unitless)
    pub frangi_c: f64,
}
impl Default for QsmartConfig {
    fn default() -> Self {
        let p = qsm_core::utils::QsmartParams::default();
        Self {
            ilsqr_tol: p.ilsqr_tol, ilsqr_max_iter: p.ilsqr_max_iter,
            vasc_sphere_radius: p.vasc_sphere_radius, sdf_spatial_radius: p.sdf_spatial_radius,
            inversion: QsmAlgorithm::Ilsqr,
            sdf_sigma1_stage1: p.sdf_sigma1_stage1,
            sdf_sigma2_stage1: p.sdf_sigma2_stage1,
            sdf_sigma1_stage2: p.sdf_sigma1_stage2,
            sdf_sigma2_stage2: p.sdf_sigma2_stage2,
            sdf_lower_lim: p.sdf_lower_lim,
            sdf_curv_constant: p.sdf_curv_constant,
            frangi_scale_min: p.frangi_scale_range[0],
            frangi_scale_max: p.frangi_scale_range[1],
            frangi_scale_ratio: p.frangi_scale_ratio,
            frangi_c: p.frangi_c,
        }
    }
}

// ROMEO config
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RomeoConfig {
    pub individual: bool,
    pub correct_global: bool,
    pub template: usize,
    pub phase_coherence: bool,
    pub phase_gradient_coherence: bool,
    pub phase_linearity: bool,
    pub mag_coherence: bool,
    pub mag_weight: bool,
    pub mag_weight2: bool,
}
impl Default for RomeoConfig {
    fn default() -> Self {
        let p = qsm_core::unwrap::RomeoParams::default();
        Self {
            individual: true,       // qsmxt default (matches Julia qsm_romeo_B0)
            correct_global: true,   // qsmxt default
            template: p.template,
            phase_coherence: p.phase_coherence,
            phase_gradient_coherence: p.phase_gradient_coherence,
            phase_linearity: p.phase_linearity,
            mag_coherence: p.mag_coherence,
            mag_weight: p.mag_weight,
            mag_weight2: p.mag_weight2,
        }
    }
}

// SWI config (special: scaling is a string mapped from enum)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SwiConfig {
    pub hp_sigma: [f64; 3],
    pub scaling: String,
    pub strength: f64,
    pub mip_window: usize,
}
impl Default for SwiConfig {
    fn default() -> Self {
        let p = qsm_core::swi::SwiParams::default();
        Self {
            hp_sigma: p.hp_sigma,
            scaling: match p.scaling {
                qsm_core::swi::PhaseScaling::Tanh => "tanh",
                qsm_core::swi::PhaseScaling::NegativeTanh => "negative-tanh",
                qsm_core::swi::PhaseScaling::Positive => "positive",
                qsm_core::swi::PhaseScaling::Negative => "negative",
                qsm_core::swi::PhaseScaling::Triangular => "triangular",
            }.to_string(),
            strength: p.strength,
            mip_window: p.mip_window,
        }
    }
}

// ─── Top-level config and section structs ───

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub description: String,
    pub pipeline: PipelineToggles,
    pub field_mapping: FieldMappingConfig,
    pub masking: MaskingConfig,
    pub bg_removal: BgRemovalConfig,
    pub inversion: InversionConfig,
    pub separation: SeparationConfig,
    pub qsm: QsmConfig,
    pub swi: SwiConfig,
    pub bet: BetConfig,
    pub homogeneity: HomogeneityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PipelineToggles {
    pub do_qsm: bool,
    pub do_swi: bool,
    pub do_t2starmap: bool,
    pub do_r2starmap: bool,
    /// Compute an R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG.
    pub do_r2map: bool,
    /// Compute an R2' map (Hz) = R2* − R2 (needs GRE magnitude + a MESE acquisition).
    pub do_r2primemap: bool,
    pub do_chi_separation: bool,
    pub export_dicom: bool,
    pub obliquity_threshold: f64,
}
impl Default for PipelineToggles {
    fn default() -> Self {
        Self { do_qsm: true, do_swi: false, do_t2starmap: false, do_r2starmap: false, do_r2map: false, do_r2primemap: false, do_chi_separation: false, export_dicom: false, obliquity_threshold: -1.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FieldMappingConfig {
    pub phase_offset_removal: bool,
    pub phase_offset_sigma: [f64; 3],
    pub bipolar_correction: bool,
    pub unwrapping_algorithm: UnwrappingAlgorithm,
    pub b0_estimation: B0Estimation,
    pub b0_weight_type: B0WeightType,
    pub romeo: RomeoConfig,
    pub linear_fit: LinearFitConfig,
}
impl Default for FieldMappingConfig {
    fn default() -> Self {
        Self {
            phase_offset_removal: true,
            phase_offset_sigma: qsm_core::utils::PhaseOffsetParams::default().sigma,
            bipolar_correction: false,
            unwrapping_algorithm: UnwrappingAlgorithm::Romeo,
            b0_estimation: B0Estimation::WeightedAvg,
            b0_weight_type: B0WeightType::PhaseSNR,
            romeo: RomeoConfig::default(),
            linear_fit: LinearFitConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MaskingConfig {
    pub inhomogeneity_correction: bool,
    pub sections: Vec<MaskSection>,
    /// Prefer a bring-your-own mask from BIDS derivatives when present, falling back to `sections`.
    /// `Some("*")` = first matching derivatives tool alphabetically; `Some("bet")` = that tool only;
    /// `None` = always compute the mask from `sections`.
    #[serde(default)]
    pub custom_mask_tool: Option<String>,
}
impl Default for MaskingConfig {
    fn default() -> Self {
        Self { inhomogeneity_correction: true, sections: default_mask_sections(), custom_mask_tool: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BgRemovalConfig {
    pub algorithm: BfAlgorithm,
    pub vsharp: VsharpConfig,
    pub pdf: PdfConfig,
    pub lbv: LbvConfig,
    pub ismv: IsmvConfig,
    pub sharp: SharpConfig,
    pub resharp: ResharpConfig,
    pub harperella: HarperellaConfig,
    pub iharperella: HarperellaConfig,
}
impl Default for BgRemovalConfig {
    fn default() -> Self {
        Self {
            algorithm: BfAlgorithm::Vsharp,
            vsharp: VsharpConfig::default(), pdf: PdfConfig::default(),
            lbv: LbvConfig::default(), ismv: IsmvConfig::default(),
            sharp: SharpConfig::default(), resharp: ResharpConfig::default(),
            harperella: HarperellaConfig::default(), iharperella: HarperellaConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct InversionConfig {
    pub algorithm: QsmAlgorithm,
    pub rts: RtsConfig,
    pub tv: TvConfig,
    pub tkd: TkdConfig,
    pub tsvd: TkdConfig,
    pub tikhonov: TikhonovConfig,
    pub nltv: NltvConfig,
    pub medi: MediConfig,
    pub tfi: TfiConfig,
    pub ilsqr: IlsqrConfig,
    pub tgv: TgvConfig,
    pub qsmart: QsmartConfig,
    pub ndi: NdiConfig,
    pub fansi: FansiConfig,
    pub l1qsm: L1qsmConfig,
    pub whqsm: WhqsmConfig,
    pub hdqsm: HdqsmConfig,
    pub amp_pe: AmpPeConfig,
}
impl Default for InversionConfig {
    fn default() -> Self {
        Self {
            algorithm: QsmAlgorithm::Rts,
            rts: RtsConfig::default(), tv: TvConfig::default(),
            tkd: TkdConfig::default(), tsvd: TkdConfig::default(),
            tikhonov: TikhonovConfig::default(), nltv: NltvConfig::default(),
            medi: MediConfig::default(), tfi: TfiConfig::default(), ilsqr: IlsqrConfig::default(),
            tgv: TgvConfig::default(), qsmart: QsmartConfig::default(),
            ndi: NdiConfig::default(), fansi: FansiConfig::default(),
            l1qsm: L1qsmConfig::default(), whqsm: WhqsmConfig::default(),
            hdqsm: HdqsmConfig::default(), amp_pe: AmpPeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct QsmConfig {
    pub reference: QsmReference,
}
impl Default for QsmConfig {
    fn default() -> Self { Self { reference: QsmReference::Mean } }
}


impl PipelineConfig {
    pub fn from_toml(s: &str) -> crate::Result<Self> {
        toml::from_str(s).map_err(|e| crate::error::ConfigError::Parse(format!("{}", e)))
    }

    pub fn to_toml(&self) -> crate::Result<String> {
        toml::to_string_pretty(self).map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }

    pub fn to_json(&self) -> crate::Result<String> {
        serde_json::to_string(self).map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }

    /// Like [`to_toml`](Self::to_toml), but prunes the `inversion` and `bg_removal`
    /// tables down to the selected algorithm only (keeping `algorithm` + its one
    /// sub-table). The result still round-trips through [`from_toml`](Self::from_toml):
    /// the omitted algorithms simply fall back to their `#[serde(default)]` values.
    /// Useful for a compact, human-facing config that drops every unselected
    /// algorithm's parameters.
    pub fn to_toml_selected(&self) -> crate::Result<String> {
        let mut value = toml::Value::try_from(self)
            .map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))?;
        if let toml::Value::Table(root) = &mut value {
            prune_to_selected_algorithm(root, "inversion");
            prune_to_selected_algorithm(root, "bg_removal");
        }
        toml::to_string_pretty(&value)
            .map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }
}

/// In `root[section]`, drop every nested algorithm table except the one whose key
/// matches that section's `algorithm` value. Keeps `algorithm` and any non-table keys.
/// No-op if the section or its `algorithm` field is absent.
fn prune_to_selected_algorithm(root: &mut toml::value::Table, section: &str) {
    let Some(toml::Value::Table(sub)) = root.get_mut(section) else { return };
    let Some(selected) = sub.get("algorithm").and_then(|v| v.as_str()).map(str::to_owned) else { return };
    let to_remove: Vec<String> = sub
        .iter()
        .filter(|(k, v)| k.as_str() != "algorithm" && v.is_table() && k.as_str() != selected)
        .map(|(k, _)| k.clone())
        .collect();
    for k in &to_remove {
        sub.remove(k);
    }
}

#[cfg(test)]
mod selected_toml_tests {
    use super::*;
    use crate::enums::{BfAlgorithm, QsmAlgorithm};

    #[test]
    fn to_toml_selected_prunes_and_roundtrips() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.inversion.rts.delta = 0.22; // non-default, must survive
        c.bg_removal.algorithm = BfAlgorithm::Vsharp;
        c.bg_removal.vsharp.threshold = 0.05; // non-default, must survive

        let toml = c.to_toml_selected().unwrap();

        // Only the selected algorithm tables remain.
        assert!(toml.contains("[inversion.rts]"), "selected inversion kept");
        assert!(!toml.contains("[inversion.tkd]"), "unselected inversion pruned");
        assert!(!toml.contains("[inversion.medi]"), "unselected inversion pruned");
        assert!(toml.contains("[bg_removal.vsharp]"), "selected bg kept");
        assert!(!toml.contains("[bg_removal.pdf]"), "unselected bg pruned");
        // Non-algorithm sections are untouched.
        assert!(toml.contains("[masking]"));

        // Round-trips like any qsmxt config: selected values preserved, omitted = defaults.
        let loaded = PipelineConfig::from_toml(&toml).unwrap();
        assert_eq!(loaded.inversion.algorithm, QsmAlgorithm::Rts);
        assert_eq!(loaded.inversion.rts.delta, 0.22);
        assert_eq!(loaded.bg_removal.vsharp.threshold, 0.05);
        // Pruned algorithms came back as their defaults.
        assert_eq!(loaded.inversion.tkd, super::TkdConfig::default());
        assert_eq!(loaded.bg_removal.pdf, super::PdfConfig::default());
    }
}
