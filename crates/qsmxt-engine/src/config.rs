use serde::{Deserialize, Serialize};
use crate::enums::*;
use crate::masking::*;

// ─── Macro for algorithm parameter structs ───
// Generates a serde struct with Default sourced from qsm-core. Each default goes through `Into`, so
// a field can be QSMxT's serde form of a qsm-core type (`te`: `EchoTime` → `EchoTimeChoice`).

macro_rules! param_config {
    ($name:ident from $core:path { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(default)]
        pub struct $name { $(pub $field: $ty),* }

        impl Default for $name {
            fn default() -> Self {
                #[allow(unused)]
                let p = <$core>::default();
                Self { $($field: p.$field.into()),* }
            }
        }
    };
}

// ─── Algorithm parameter configs (each ~1-3 lines) ───

param_config!(RtsConfig from qsm_core::inversion::RtsParams {
    delta: f64, mu: f64, rho: f64, tol: f64, max_iter: usize, lsmr_iter: usize, tau: f64, gamma: f64
});
// `rho`: `None` is qsm-core's 100·λ, so it follows λ.
param_config!(TvConfig from qsm_core::inversion::TvParams {
    lambda: f64, rho: Option<f64>, tol: f64, max_iter: usize
});
param_config!(TkdConfig from qsm_core::inversion::TkdParams { threshold: f64 });
// `te` (here and below): which of the scan's echo times the method converts the field from ppm to
// radians at; each method's default is its reference code's. The scan's echo times and field
// strength are not settings: qsm-core's dispatcher fills them in.
// NLTV's `rho`: `None` is qsm-core's 100·λ, so it follows λ.
param_config!(NltvConfig from qsm_core::inversion::NltvParams {
    lambda: f64, rho: Option<f64>, mu: f64, tol: f64, max_iter: usize, tol_newton: f64,
    max_iter_newton: usize, te: EchoTimeChoice
});
param_config!(NdiConfig from qsm_core::inversion::NdiParams {
    tau: f64, alpha: f64, max_iter: usize, auto_stop: bool, te: EchoTimeChoice
});
// FANSI's `mu1`, `alpha0` and `mu0`, L1-QSM's `mu1` and HD-QSM's `mu1_l2`: `None` is the value
// qsm-core derives from the method's other weights (FANSI: 100·α1, 2·α1, 2·μ1; L1-QSM: 100·α1;
// HD-QSM: 100·α_l2), so they follow those.
param_config!(FansiConfig from qsm_core::inversion::FansiParams {
    alpha1: f64, mu1: Option<f64>, mu2: f64, alpha0: Option<f64>, mu0: Option<f64>, max_iter: usize,
    tol_update: f64, tol_delta: f64, te: EchoTimeChoice
});
param_config!(L1qsmConfig from qsm_core::inversion::L1QsmParams {
    alpha1: f64, mu1: Option<f64>, mu2: f64, mu3: f64, lambda: f64, max_iter: usize,
    tol_update: f64, tol_delta: f64, te: EchoTimeChoice
});
// WH-QSM's `weight`: the data weight; any but `flat` is made from the magnitude.
param_config!(WhqsmConfig from qsm_core::inversion::WhQsmParams {
    alpha1: f64, mu1: f64, mu2: f64, beta: f64, muh: f64, max_iter: usize,
    tol_update: f64, tol_delta: f64, weight: WhqsmWeight, te: EchoTimeChoice
});
param_config!(HdqsmConfig from qsm_core::inversion::HdQsmParams {
    alpha_l2: f64, mu1_l2: Option<f64>, mu2: f64, max_iter_l1: usize, max_iter_l2: usize, tol_update: f64
});

/// MEDI (MEDI toolbox `MEDI_L1`), with its mSMV option.
///
/// `MediParams::msmv` is an `Option<MsmvParams>` in qsm-core; it is flattened here into an `msmv`
/// switch plus its settings, as HEIDI's `denoise` is, so it can be a checkbox and rows in the TUI
/// rather than a nested table. The mSMV radius is MEDI's `smv_radius`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MediConfig {
    pub lambda: f64,
    pub merit: bool,
    pub smv: bool,
    pub smv_radius: f64,
    /// With SMV, erode the mask to where the SMV of the mask exceeds 0.999, as `MEDI_L1` does.
    /// Off keeps the full mask. Ignored without SMV and with mSMV, which keeps the mask.
    pub smv_shrink_mask: bool,
    pub data_weighting: i32,
    pub percentage: f64,
    pub cg_tol: f64,
    pub cg_max_iter: usize,
    pub max_iter: usize,
    pub tol: f64,
    /// MEDI-mSMV (Roberts et al. 2024): the local field is filtered by mSMV, and MEDI fits it
    /// with the SMV-filtered dipole kernel. Needs at least three echoes. Off by default.
    pub msmv: bool,
    /// mSMV's boundary correction makes at most `msmv_maxk − 1` passes.
    pub msmv_maxk: usize,
    /// Largest vessel thickness (voxels) of mSMV's vessel mask; `None` is `msmv.m`'s
    /// `max(15, 8·max(voxel size))`.
    pub msmv_vessel_radius: Option<f64>,
    /// SMV-filter the field before mSMV, as the authors do. Off is only consistent for a field
    /// already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges.
    pub msmv_prefilter: bool,
    /// Which echo time the field is converted to radians at (MEDI's `delta_TE`).
    pub te: EchoTimeChoice,
}
impl Default for MediConfig {
    fn default() -> Self {
        let p = qsm_core::inversion::MediParams::default();
        let m = p.msmv.clone().unwrap_or_default();
        Self {
            lambda: p.lambda, merit: p.merit, smv: p.smv, smv_radius: p.smv_radius,
            smv_shrink_mask: p.smv_shrink_mask, data_weighting: p.data_weighting,
            percentage: p.percentage, cg_tol: p.cg_tol, cg_max_iter: p.cg_max_iter, max_iter: p.max_iter, tol: p.tol,
            msmv: p.msmv.is_some(), msmv_maxk: m.maxk, msmv_vessel_radius: m.vessel_radius,
            msmv_prefilter: m.prefilter, te: p.te.into(),
        }
    }
}
impl MediConfig {
    /// qsm-core's mSMV option for these settings: `None` unless `msmv` is on.
    pub fn msmv_params(&self) -> Option<qsm_core::inversion::MsmvParams> {
        self.msmv.then_some(qsm_core::inversion::MsmvParams {
            prefilter: self.msmv_prefilter,
            maxk: self.msmv_maxk,
            vessel_radius: self.msmv_vessel_radius,
        })
    }
}
param_config!(TfiConfig from qsm_core::inversion::TfiParams {
    lambda: f64, precond: f64, data_weighting: i32,
    percentage: f64, cg_tol: f64, cg_max_iter: usize, max_iter: usize, tol: f64, te: EchoTimeChoice
});
param_config!(AmpPeConfig from qsm_core::inversion::AmpPeParams {
    wave_order: usize, nlevel: usize, wave_pec: f64, simulated_te: f64,
    max_linearization_ite: usize, b0: f64, gyro_ratio: f64, damp_rate_sig: f64,
    damp_rate_par: f64, max_pe_spar_ite: usize, max_pe_est_ite: usize,
    cvg_thd: f64, tikhonov_beta: f64
});
param_config!(IlsqrConfig from qsm_core::inversion::IlsqrParams { tol: f64, max_iter: usize, pad_mm: f64 });
impl From<&qsm_core::inversion::IlsqrParams> for IlsqrConfig {
    fn from(p: &qsm_core::inversion::IlsqrParams) -> Self {
        Self { tol: p.tol, max_iter: p.max_iter, pad_mm: p.pad_mm }
    }
}
impl IlsqrConfig {
    /// qsm-core's parameters.
    pub fn params(&self) -> qsm_core::inversion::IlsqrParams {
        qsm_core::inversion::IlsqrParams { tol: self.tol, max_iter: self.max_iter, pad_mm: self.pad_mm }
    }
}

/// STAR-QSM (STI Suite's `QSM_star`).
///
/// `echo_times` and `b0` are absent on purpose: qsm-core's dispatcher fills them from the scan.
/// `te` is which of the echo times the field is converted to phase at; STAR-QSM's result
/// depends on that scale. Written out rather than generated by `param_config!` because `te` is
/// QSMxT's serde form of qsm-core's `EchoTime`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StarConfig {
    /// Zero padding per side in voxels, per axis (x y z).
    pub pad: [usize; 3],
    pub te: EchoTimeChoice,
}
impl Default for StarConfig {
    fn default() -> Self {
        let p = qsm_core::inversion::StarParams::default();
        Self { pad: p.pad, te: p.te.into() }
    }
}
impl StarConfig {
    /// Why the settings cannot be used, if that does not depend on the scan: a given echo time
    /// that is not a positive time. Against a scan's echo times, `StarParams::validate` decides
    /// ([`crate::requirements`]).
    pub fn validate(&self) -> Result<(), String> {
        match self.te {
            EchoTimeChoice::Seconds(s) if !(s.is_finite() && s > 0.0) => Err(format!(
                "STAR-QSM echo time: must be a finite time greater than 0 ms (got {} ms)", s * 1000.0)),
            _ => Ok(()),
        }
    }
}

/// Minimally regularised LSQR (Schweser 2010), and the seed HEIDI builds on.
///
/// `b0` and `mask_output` are absent on purpose: qsm-core's dispatcher sets `b0` from scan
/// metadata and forces `mask_output` off when HEIDI consumes the solution, so neither is the
/// user's to choose. Written out rather than generated by `param_config!` because `as` cannot
/// cast `Option<f64>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LsqrConfig {
    /// Weight `w` on the residual-field unknowns — the only thing regularising the solve.
    /// `None` uses qsm-core's `RESIDUAL_WEIGHTING_PER_TESLA * B0`.
    pub residual_weighting: Option<f64>,
    pub fit_global_offset: bool,
    pub tol: f64,
    pub max_iter: usize,
}
impl Default for LsqrConfig {
    fn default() -> Self {
        let p = qsm_core::inversion::LsqrQsmParams::default();
        Self {
            residual_weighting: p.residual_weighting,
            fit_global_offset: p.fit_global_offset,
            tol: p.tol,
            max_iter: p.max_iter,
        }
    }
}

/// HEIDI (Schweser 2012), chained on [`LsqrConfig`].
///
/// `HeidiParams::denoise` is an `Option<AnisotropicDiffusionParams>`; it is flattened here into a
/// `denoise` switch plus its three parameters, so it can be a checkbox and three numbers in the
/// TUI rather than a nested table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HeidiConfig {
    pub cone_threshold: f64,
    pub gradient_threshold: f64,
    pub apply_laplacian_correction: bool,
    pub laplacian_threshold: f64,
    pub gradient_mask_floor: f64,
    /// The dominant quality knob, and more is not better — see qsm-core's note. Worth sweeping.
    pub continuation_steps: usize,
    pub inner_iterations: usize,
    pub mu_min: f64,
    pub tol: f64,
    /// Anisotropic-diffusion smoothing of the field before its gradients are taken.
    pub denoise: bool,
    pub denoise_iterations: usize,
    pub denoise_time_step: f64,
    pub denoise_conductance: f64,
    /// Which echo time the field stands for the phase of, in the conversion to HEIDI's units.
    pub te: EchoTimeChoice,
}
impl Default for HeidiConfig {
    fn default() -> Self {
        let p = qsm_core::inversion::HeidiParams::default();
        let d = p.denoise.unwrap_or(qsm_core::utils::AnisotropicDiffusionParams {
            iterations: 5, time_step: 0.05, conductance: 1.0,
        });
        Self {
            cone_threshold: p.cone_threshold,
            gradient_threshold: p.gradient_threshold,
            apply_laplacian_correction: p.apply_laplacian_correction,
            laplacian_threshold: p.laplacian_threshold,
            gradient_mask_floor: p.gradient_mask_floor,
            continuation_steps: p.continuation_steps,
            inner_iterations: p.inner_iterations,
            mu_min: p.mu_min,
            tol: p.tol,
            denoise: p.denoise.is_some(),
            denoise_iterations: d.iterations,
            denoise_time_step: d.time_step,
            denoise_conductance: d.conductance,
            te: p.te.into(),
        }
    }
}

// ─── Chi-separation method params (cf/b0/se_echo_times are set from scan metadata) ───
/// χ-separation (iLSQR). Written out rather than generated by `param_config!` because
/// `qsm_ilsqr` is a group of its own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChiSepIlsqrConfig {
    pub dr_pos: f64,
    pub dr_neg: f64,
    pub lambda1: f64,
    pub percentage: f64,
    pub r2p_min: f64,
    pub r2p_max: f64,
    pub max_iter: usize,
    pub tol: f64,
    pub cg_max_iter: usize,
    pub cg_tol: f64,
    /// The iLSQR of the susceptibility map χ-separation is initialised with, reconstructed from
    /// the local field (unless a QSM is supplied): the χ-separation toolbox's call by default.
    pub qsm_ilsqr: IlsqrConfig,
}
impl Default for ChiSepIlsqrConfig {
    fn default() -> Self {
        let p = qsm_core::separation::ChiSepIlsqrParams::default();
        Self {
            dr_pos: p.dr_pos, dr_neg: p.dr_neg, lambda1: p.lambda1, percentage: p.percentage,
            r2p_min: p.r2p_min, r2p_max: p.r2p_max, max_iter: p.max_iter, tol: p.tol,
            cg_max_iter: p.cg_max_iter, cg_tol: p.cg_tol,
            qsm_ilsqr: IlsqrConfig::from(&p.qsm_ilsqr),
        }
    }
}
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
    /// How R2′ is obtained when `custom_r2prime_tool` supplies nothing. A custom map always wins;
    /// this decides between measuring R2′ from a MESE acquisition and predicting it with
    /// R2PRIMEnet.
    #[serde(default)]
    pub r2prime_strategy: R2PrimeStrategy,
    /// R2PRIMEnet inference settings, for when R2′ is predicted rather than measured.
    #[serde(default)]
    pub r2primenet: R2primenetConfig,
    /// χ-sepnet inference settings.
    #[serde(default)]
    pub chi_sepnet: ChiSepNetConfig,
    /// SUSEP-Net inference settings.
    #[serde(default)]
    pub susep_net: SusepNetConfig,
}

/// χ-sepnet inference (`qsm_core::separation::ChiSepNetParams`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChiSepNetConfig {
    /// Sliding-window patch (D, H, W) in voxels; each a positive multiple of 16.
    pub patch: [usize; 3],
}
impl Default for ChiSepNetConfig {
    fn default() -> Self {
        let (d, h, w) = qsm_core::separation::ChiSepNetParams::default().patch;
        Self { patch: [d, h, w] }
    }
}
impl ChiSepNetConfig {
    /// Why the patch cannot be used, if it cannot (qsm-core refuses it).
    pub fn validate(&self) -> Result<(), String> {
        check_patch("χ-sepnet", &self.patch, 16)
    }
    /// qsm-core's parameters for these settings.
    pub fn params(&self) -> qsm_core::separation::ChiSepNetParams {
        let [d, h, w] = self.patch;
        qsm_core::separation::ChiSepNetParams { patch: (d, h, w) }
    }
}

/// SUSEP-Net inference (`qsm_core::separation::SusepNetParams`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct SusepNetConfig {
    /// Sliding-window patch (D, H, W) in voxels, each a positive multiple of 8; `None` (the
    /// default) runs the whole volume in one pass.
    pub patch: Option<[usize; 3]>,
}
impl SusepNetConfig {
    /// Why the patch cannot be used, if it cannot (qsm-core refuses it).
    pub fn validate(&self) -> Result<(), String> {
        self.patch.as_ref().map_or(Ok(()), |p| check_patch("SUSEP-Net", p, 8))
    }
    /// qsm-core's parameters for these settings.
    pub fn params(&self) -> qsm_core::separation::SusepNetParams {
        qsm_core::separation::SusepNetParams { patch: self.patch.map(|[d, h, w]| (d, h, w)) }
    }
}

/// A network patch every side of which must be a positive multiple of `multiple` (its pooling
/// levels).
fn check_patch(net: &str, patch: &[usize; 3], multiple: usize) -> Result<(), String> {
    match patch.iter().find(|&&d| d == 0 || d % multiple != 0) {
        Some(d) => Err(format!(
            "{net} patch {patch:?}: every dimension must be a positive multiple of {multiple} (got {d})")),
        None => Ok(()),
    }
}

/// R2PRIMEnet inference (`qsm_core::relaxometry::R2PrimeNetParams`, which only exists with
/// qsm-core's `onnx` feature; this crate builds without it, so the defaults are written out here
/// and the qsmxt binary's `dl` build checks them against qsm-core's).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct R2primenetConfig {
    /// Sliding-window patch (D, H, W) in voxels; each a positive multiple of 16.
    pub patch: [usize; 3],
    /// Run even though the acquisition is not at 3 T, the only field strength the weights were
    /// trained at. The R2′ is then uncalibrated. Off by default: an explicit
    /// `r2prime_strategy = "r2primenet"` then refuses a non-3 T run, and `auto` skips R2′.
    pub ignore_field_mismatch: bool,
}
impl Default for R2primenetConfig {
    fn default() -> Self {
        // qsm_core::relaxometry::{AUTHORS_PATCH, R2PrimeNetParams::default()}.
        Self { patch: [192, 192, 128], ignore_field_mismatch: false }
    }
}
/// R2PRIMEnet's patch multiple: QSM.rs's `relaxometry::POOL_MULTIPLE`. QSM.rs exports it only
/// with its `onnx` feature, so a build without the networks (which cannot run R2PRIMEnet, but
/// still shows and checks the setting) uses the same value written out.
#[cfg(feature = "onnx")]
pub const R2PRIMENET_POOL_MULTIPLE: usize = qsm_core::relaxometry::POOL_MULTIPLE;
#[cfg(not(feature = "onnx"))]
pub const R2PRIMENET_POOL_MULTIPLE: usize = 16;

impl R2primenetConfig {
    /// Why the patch cannot be used, if it cannot (qsm-core panics on it).
    pub fn validate(&self) -> Result<(), String> {
        const POOL_MULTIPLE: usize = R2PRIMENET_POOL_MULTIPLE;
        match self.patch.iter().find(|&&d| d == 0 || d % POOL_MULTIPLE != 0) {
            Some(d) => Err(format!(
                "R2PRIMEnet patch {:?}: every dimension must be a positive multiple of {POOL_MULTIPLE} (got {d})",
                self.patch)),
            None => Ok(()),
        }
    }
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
            r2prime_strategy: R2PrimeStrategy::default(),
            r2primenet: R2primenetConfig::default(),
            chi_sepnet: ChiSepNetConfig::default(),
            susep_net: SusepNetConfig::default(),
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
/// Per-structure statistics need a parcellation to average over and at least one map to average,
/// so asking for the analysis forces both. A bring-your-own dseg satisfies the parcellation.
///
/// The map side is satisfied by *any* quantitative map, not χ specifically: the table summarises
/// everything the run produced, so `--no-qsm --do-r2starmap --do-analysis` is a perfectly good
/// request and must not have QSM forced back on behind the user's back. Reconstruction is only
/// implied when nothing else would produce a map at all — the difference between filling in an
/// obvious omission and overriding a stated choice.
pub fn enforce_analysis_dependencies(config: &mut PipelineConfig) {
    if config.pipeline.do_analysis {
        if config.segmentation.custom_dseg_tool.is_none() {
            config.pipeline.do_segmentation = true;
        }
        if !produces_a_quantitative_map(config) {
            config.pipeline.do_qsm = true;
        }
    }
}

/// Whether the run will produce anything the per-structure table can summarise.
///
/// Chi-separation counts because it writes χ+/χ−, and a custom Chimap counts because the analysis
/// reads it from derivatives. The custom R2/R2' tools do not count on their own: those maps are
/// only pulled in when their own toggle asks for them.
fn produces_a_quantitative_map(config: &PipelineConfig) -> bool {
    config.pipeline.do_qsm
        || config.pipeline.do_t2starmap
        || config.pipeline.do_r2starmap
        || config.pipeline.do_r2map
        || config.pipeline.do_r2primemap
        || config.pipeline.do_chi_separation
        || config.separation.custom_qsm_tool.is_some()
}

/// A region reference is measured on the parcellation, so asking for one asks for a segmentation.
///
/// Enabling it here rather than erroring keeps `--qsm-reference thalamus` a complete instruction:
/// the thing the user asked for is the reference, and SynthSeg is how it gets measured. A
/// bring-your-own dseg satisfies it the same way it satisfies the statistics.
///
/// A `Region` with nothing to resolve is not a reference at all, so it falls back to the
/// brain-mask mean rather than referencing to an empty set. The CLI and TUI both set the two
/// fields together, so this only fires on a hand-edited config.
pub fn enforce_reference_dependencies(config: &mut PipelineConfig) {
    if config.qsm.reference != QsmReference::Region {
        return;
    }
    if config.qsm.reference_region.as_ref().is_none_or(|r| r.trim().is_empty()) {
        config.qsm.reference = QsmReference::Mean;
        config.qsm.reference_region = None;
        return;
    }
    if config.pipeline.do_qsm && config.segmentation.custom_dseg_tool.is_none() {
        config.pipeline.do_segmentation = true;
    }
}

/// SMWI weights the magnitude by the susceptibility map, so it cannot run without one.
///
/// Turning it on forces QSM, the same way chi-separation forces the relaxometry maps it consumes.
/// A custom QSM from derivatives counts: that is a susceptibility map too.
pub fn enforce_smwi_dependencies(config: &mut PipelineConfig) {
    if config.pipeline.do_smwi && config.separation.custom_qsm_tool.is_none() {
        config.pipeline.do_qsm = true;
    }
}

pub fn enforce_separation_dependencies(config: &mut PipelineConfig) {
    if config.pipeline.do_chi_separation {
        match config.separation.algorithm {
            // Uses R2* directly (fit from the GRE magnitude); no R2/R2' needed.
            SeparationAlgorithm::R2starQsm => config.pipeline.do_r2starmap = true,
            // Fits the multi-echo magnitude signal directly; no relaxometry maps needed.
            SeparationAlgorithm::Decompose => {}
            // R2'-based methods: need R2' (unless a custom R2' map is supplied).
            // SUSEP-Net consumes [QSM, R2', local field] → [χ+, χ−], so it needs R2' too.
            SeparationAlgorithm::ChiSepIlsqr
            | SeparationAlgorithm::ChiSepMedi
            | SeparationAlgorithm::WaveSep
            | SeparationAlgorithm::HcChisep
            | SeparationAlgorithm::SusepNet
            | SeparationAlgorithm::ChiSepNet => {
                if config.separation.custom_r2prime_tool.is_none() {
                    config.pipeline.do_r2primemap = true;
                }
            }
        }
    }
    // Computing R2' (no custom map) always needs R2*: it is the minuend of R2* − R2, and it is
    // also R2PRIMEnet's only input. R2 is needed only on the routes that can measure R2' —
    // predicting it from R2* does not subtract anything.
    if config.pipeline.do_r2primemap && config.separation.custom_r2prime_tool.is_none() {
        config.pipeline.do_r2starmap = true;
        let may_measure = config.separation.r2prime_strategy != R2PrimeStrategy::R2primenet;
        if may_measure && config.separation.custom_r2_tool.is_none() {
            config.pipeline.do_r2map = true;
        }
    }
}
/// V-SHARP. Written out rather than generated by `param_config!` because qsm-core's working grid
/// is an enum with a payload, held here as `grid` and `grid_spacing`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VsharpConfig {
    /// Undo the SMV high-pass with a truncated-SVD deconvolution (Wu et al. 2012). Off (the
    /// default) returns the SMV high-pass with the largest kernel that fits at each voxel, as
    /// STI Suite's V_SHARP does.
    pub deconvolve: bool,
    /// TSVD threshold of the deconvolution; unused when `deconvolve` is off.
    pub threshold: f64,
    pub max_radius: f64,
    pub min_radius: f64,
    /// Step between successive kernel radii, in mm: the radii are `max_radius`,
    /// `max_radius - radius_step`, ... down to `min_radius`.
    pub radius_step: f64,
    /// Grid the background is estimated on: `isotropic` at [`grid_spacing`](Self::grid_spacing)
    /// (the default), `native` (the acquired grid), or `finest` (isotropic at the smallest voxel
    /// dimension).
    pub grid: VsharpGrid,
    /// Isotropic working-grid spacing in mm; used only with `grid = "isotropic"`.
    pub grid_spacing: f64,
}
impl Default for VsharpConfig {
    fn default() -> Self {
        use qsm_core::bgremove::VsharpGrid as Core;
        let p = qsm_core::bgremove::VsharpParams::default();
        let (grid, grid_spacing) = match p.grid {
            Core::Native => (VsharpGrid::Native, qsm_core::bgremove::vsharp::STI_GRID_MM),
            Core::IsotropicFinest => (VsharpGrid::Finest, qsm_core::bgremove::vsharp::STI_GRID_MM),
            Core::Isotropic { spacing_mm } => (VsharpGrid::Isotropic, spacing_mm),
        };
        Self {
            deconvolve: p.deconvolve, threshold: p.threshold, max_radius: p.max_radius, min_radius: p.min_radius,
            radius_step: p.radius_step, grid, grid_spacing,
        }
    }
}
impl VsharpConfig {
    /// The qsm-core working grid.
    pub fn core_grid(&self) -> qsm_core::bgremove::VsharpGrid {
        match self.grid {
            VsharpGrid::Native => qsm_core::bgremove::VsharpGrid::Native,
            VsharpGrid::Finest => qsm_core::bgremove::VsharpGrid::IsotropicFinest,
            VsharpGrid::Isotropic => qsm_core::bgremove::VsharpGrid::Isotropic { spacing_mm: self.grid_spacing },
        }
    }
}
// PDF's `n_pad`: zero padding around the mask's bounding box, voxels per axis (0 = the full volume).
param_config!(PdfConfig from qsm_core::bgremove::PdfParams {
    tol: f64, n_cg: usize, space: PdfSpace, n_pad: usize
});
impl PdfConfig {
    /// Why the settings cannot be used, if they cannot: a padding of 1, which `PDF.m` cannot run
    /// with on odd extents (qsm-core refuses it).
    pub fn validate(&self) -> Result<(), String> {
        if self.n_pad == 1 {
            return Err("PDF padding: must be 0 (the full volume) or at least 2 voxels (got 1)".into());
        }
        Ok(())
    }
}
param_config!(LbvConfig from qsm_core::bgremove::LbvParams { tol: f64 });
param_config!(IsmvConfig from qsm_core::bgremove::IsmvParams {
    tol: f64, max_iter: usize, radius: f64
});
param_config!(SharpConfig from qsm_core::bgremove::SharpParams { threshold: f64, radius: f64 });
param_config!(ResharpConfig from qsm_core::bgremove::ResharpParams {
    radius: f64, tik_reg: f64, tol: f64, max_iter: usize
});
// `pad`: zero padding per side, voxels per axis (STI Suite's `padsize`).
param_config!(HarperellaConfig from qsm_core::bgremove::HarperellaParams {
    radius: f64, max_iter: usize, tol: f64, pad: [usize; 3]
});
// iHARPERELLA has no SMV kernel, so no radius (QSM.rs#169).
param_config!(IharperellaConfig from qsm_core::bgremove::IharperellaParams {
    max_iter: usize, tol: f64, pad: [usize; 3]
});
param_config!(BetConfig from qsm_core::bet::BetParams {
    fractional_intensity: f64, smoothness: f64, gradient_threshold: f64,
    iterations: usize, subdivisions: usize, voxel_scale: f64
});
param_config!(HomogeneityConfig from qsm_core::utils::HomogeneityParams {
    sigma_mm: f64, nbox: usize
});
// N4 runs without a mask here (on the magnitude masking is made from), so qsm-core's
// `rescale_intensities`, which applies only with a mask, has nothing to do and is left out.
param_config!(N4Config from qsm_core::utils::N4Params {
    shrink_factor: usize, max_iterations: Vec<usize>, convergence_threshold: f64, mesh: [usize; 3],
    spline_order: usize, bias_field_fwhm: f64, wiener_noise: f64, histogram_bins: usize
});
impl N4Config {
    /// The qsm-core parameters.
    pub fn core(&self) -> qsm_core::utils::N4Params {
        qsm_core::utils::N4Params {
            shrink_factor: self.shrink_factor,
            max_iterations: self.max_iterations.clone(),
            convergence_threshold: self.convergence_threshold,
            mesh: self.mesh,
            spline_order: self.spline_order,
            bias_field_fwhm: self.bias_field_fwhm,
            wiener_noise: self.wiener_noise,
            histogram_bins: self.histogram_bins,
            ..Default::default()
        }
    }

    /// Why the settings cannot be used, if they cannot: what qsm-core's `n4_bias_correction`
    /// asserts (at least one fitting level, a positive shrink factor and mesh, a spline order of
    /// 1 to 3, two histogram bins or more), and a positive, finite FWHM.
    pub fn validate(&self) -> Result<(), String> {
        let mut problems = Vec::new();
        if self.max_iterations.is_empty() {
            problems.push("--n4-max-iterations needs at least one fitting level".to_string());
        }
        if self.shrink_factor == 0 {
            problems.push("--n4-shrink-factor must be at least 1".into());
        }
        if self.mesh.contains(&0) {
            problems.push(format!("--n4-mesh must be at least 1 along each axis, not {:?}", self.mesh));
        }
        if !(1..=3).contains(&self.spline_order) {
            problems.push(format!("--n4-spline-order must be 1, 2 or 3, not {}", self.spline_order));
        }
        if self.histogram_bins < 2 {
            problems.push(format!("--n4-histogram-bins must be at least 2, not {}", self.histogram_bins));
        }
        if !(self.bias_field_fwhm > 0.0 && self.bias_field_fwhm.is_finite()) {
            problems.push(format!("--n4-bias-field-fwhm must be a positive number, not {}", self.bias_field_fwhm));
        }
        if !(self.wiener_noise >= 0.0 && self.wiener_noise.is_finite() && self.convergence_threshold.is_finite()) {
            problems.push("--n4-wiener-noise and --n4-convergence-threshold must be finite, and the noise not negative".into());
        }
        if problems.is_empty() { Ok(()) } else { Err(format!("N4: {}", problems.join("; "))) }
    }

    /// Why a `dims` grid is too small for N4, if it is: N4 fits the bias field on the image
    /// shrunk by `shrink_factor`, which needs at least two voxels along each axis.
    pub fn grid_problem(&self, dims: (usize, usize, usize)) -> Option<String> {
        let s = self.shrink_factor.max(1);
        let short = [dims.0, dims.1, dims.2].into_iter().any(|n| n / s < 2);
        short.then(|| format!(
            "N4 fits the bias field on the image shrunk by {s} (--n4-shrink-factor), which needs at \
             least {} voxels along each axis, and this run is {}x{}x{}. Lower --n4-shrink-factor, or \
             use --inhomogeneity-method makehomogeneous",
            2 * s, dims.0, dims.1, dims.2))
    }
}
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

// TGV needs special handling (f32→f64 conversion)
/// TGV-QSM (QuantitativeSusceptibilityMappingTGV.jl). `alpha0` and `alpha1` are in the
/// reference's units: radians, at the echo time `te` picks (the first by default) and the scan's
/// field strength.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TgvConfig {
    /// `None` takes the reference's count for the voxel size (and step size).
    pub iterations: Option<usize>,
    pub erosions: usize,
    pub alpha0: f64,
    pub alpha1: f64,
    pub step_size: f64,
    /// Which echo time a field combined over several echoes is converted to phase at; a single
    /// echo's wrapped phase is at its own.
    pub te: EchoTimeChoice,
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
            step_size: widen(p.step_size), te: p.te.into(),
        }
    }
}

// QSMART config
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct QsmartConfig {
    /// Iteration cap of QSMART's inner iLSQR solves. Its tolerance is `inversion.ilsqr.tol`.
    pub ilsqr_max_iter: usize,
    /// Zero-padding (mm per side) of QSMART's inner iLSQR solves. Default 6 mm, STI Suite's.
    pub ilsqr_pad_mm: f64,
    /// Bottom-hat sphere radius for vasculature detection, in `vessel_size_units`
    pub vasc_sphere_radius: i32,
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
    // Frangi vesselness, vessel radii in `vessel_size_units`
    pub frangi_scale_min: f64,
    pub frangi_scale_max: f64,
    pub frangi_scale_ratio: f64,
    /// Frangi C noise threshold (unitless)
    pub frangi_c: f64,
    /// The magnitude vessels are detected on: the mean over echoes, N4 bias-corrected (default),
    /// or the run's combined magnitude.
    pub vessel_magnitude: QsmartVesselMagnitude,
    /// The units of `vasc_sphere_radius` and the Frangi scales: voxels (default, as QSMART
    /// specifies them) or mm (converted with the mean voxel size).
    ///
    /// A file without `schema_version` that gives vessel sizes without this field was written
    /// when they were in mm, and reads as mm ([`crate::migrate`]).
    pub vessel_size_units: VesselSizeUnits,
}

/// The `[inversion.qsmart]` keys measured in `vessel_size_units`.
pub const QSMART_VESSEL_SIZE_KEYS: [&str; 4] =
    ["vasc_sphere_radius", "frangi_scale_min", "frangi_scale_max", "frangi_scale_ratio"];
impl Default for QsmartConfig {
    fn default() -> Self {
        let p = qsm_core::utils::QsmartParams::default();
        Self {
            ilsqr_max_iter: p.ilsqr_max_iter, ilsqr_pad_mm: p.ilsqr_pad_mm,
            vasc_sphere_radius: p.vasc_sphere_radius,
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
            vessel_magnitude: QsmartVesselMagnitude::default(),
            vessel_size_units: VesselSizeUnits::default(),
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
    pub bestpath: bool,
    pub temporal_uncertain_unwrapping: f64,
    pub max_seeds: u8,
    pub merge_regions: bool,
    pub correct_regions: bool,
    pub wrap_addition: f64,
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
            bestpath: p.bestpath,
            temporal_uncertain_unwrapping: p.temporal_uncertain_unwrapping,
            max_seeds: p.max_seeds,
            merge_regions: p.merge_regions,
            correct_regions: p.correct_regions,
            wrap_addition: p.wrap_addition,
        }
    }
}

impl From<&RomeoConfig> for qsm_core::unwrap::RomeoParams {
    fn from(r: &RomeoConfig) -> Self {
        Self {
            individual: r.individual,
            correct_global: r.correct_global,
            template: r.template,
            phase_coherence: r.phase_coherence,
            phase_gradient_coherence: r.phase_gradient_coherence,
            phase_linearity: r.phase_linearity,
            mag_coherence: r.mag_coherence,
            mag_weight: r.mag_weight,
            mag_weight2: r.mag_weight2,
            bestpath: r.bestpath,
            temporal_uncertain_unwrapping: r.temporal_uncertain_unwrapping,
            max_seeds: r.max_seeds,
            merge_regions: r.merge_regions,
            correct_regions: r.correct_regions,
            wrap_addition: r.wrap_addition,
        }
    }
}

/// The patch of a SynthSeg sliding window, or `None` for the whole volume.
pub(crate) fn synthseg_patch(inference: qsm_core::segment::SynthSegInference) -> Option<usize> {
    match inference {
        qsm_core::segment::SynthSegInference::FullVolume => None,
        qsm_core::segment::SynthSegInference::SlidingWindow { patch, .. } => Some(patch),
    }
}

/// SynthSeg's low-memory patch — qsm-core's `SynthSegInference::LOW_MEMORY`.
pub fn synthseg_low_memory_patch() -> usize {
    synthseg_patch(qsm_core::segment::SynthSegInference::LOW_MEMORY).expect("a sliding window")
}

/// The sliding window `patch` asks for, its neighbours overlapping by at least half a patch (as
/// qsm-core's low-memory window does).
pub fn synthseg_inference(patch: Option<usize>) -> qsm_core::segment::SynthSegInference {
    use qsm_core::segment::SynthSegInference as I;
    patch.map_or(I::FullVolume, |patch| I::SlidingWindow { patch, overlap: patch / 2 })
}

/// Whole-brain segmentation via SynthSeg, and the per-structure statistics drawn from it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SegmentationConfig {
    /// Weights generation: `v1` (32 labels, ships with SynthSeg) or `v2` (33 labels, adds CSF).
    /// Selects the label table, so it must match the weights being run.
    pub version: SynthSegVersion,
    /// Cubic centre-crop in voxels at 1 mm before inference, rounded up to a multiple of 32.
    /// `None` processes the whole volume, as the SynthSeg CLI does.
    pub crop: Option<usize>,
    /// Average the posteriors with a left-right flipped second pass. SynthSeg's default; doubles
    /// inference time.
    pub flip_averaging: bool,
    /// Reset each topological class to its largest connected component.
    pub topology_cleanup: bool,
    /// Gaussian blur on the posteriors, in voxels. Zero disables it.
    pub sigma_smoothing: f64,
    /// Run the network as a sliding window of cubes this many voxels on a side at 1 mm (at least
    /// 64, rounded up to a multiple of 16), overlapping by at least half a patch and
    /// Gaussian-blended, so its memory
    /// follows the patch instead of the volume. `None` runs the whole volume at once, as the
    /// SynthSeg CLI does.
    #[serde(default)]
    pub patch: Option<usize>,
    /// Prefer a bring-your-own segmentation from `<bids>/derivatives/<tool>/…` over running
    /// SynthSeg, mirroring `masking.custom_mask_tool`. Always falls back to computing one.
    #[serde(default)]
    pub custom_dseg_tool: Option<String>,
}
impl Default for SegmentationConfig {
    fn default() -> Self {
        let p = qsm_core::segment::SynthSegParams::default();
        Self {
            version: match p.version {
                qsm_core::segment::SynthSegVersion::V1 => SynthSegVersion::V1,
                qsm_core::segment::SynthSegVersion::V2 => SynthSegVersion::V2,
            },
            crop: p.crop,
            flip_averaging: p.flip_averaging,
            topology_cleanup: p.topology_cleanup,
            sigma_smoothing: p.sigma_smoothing,
            patch: synthseg_patch(p.inference),
            custom_dseg_tool: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SmwiConfig {
    /// |χ| (ppm) at which the weighting mask reaches zero.
    pub threshold_ppm: f64,
    /// Power the mask is raised to — contrast strength.
    pub power: f64,
    pub mip_window: usize,
}
impl Default for SmwiConfig {
    fn default() -> Self {
        let p = qsm_core::swi::SmwiParams::default();
        Self { threshold_ppm: p.threshold_ppm, power: p.power, mip_window: p.mip_window }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SwiConfig {
    /// Phase unwrapping before the echoes are combined.
    pub unwrap: SwiUnwrap,
    pub hp_sigma: [f64; 3],
    pub scaling: SwiScaling,
    pub strength: f64,
    pub mip_window: usize,
}
impl SwiConfig {
    /// The CLEAR-SWI parameters this config runs with: qsm-core's defaults for what the config
    /// does not expose.
    pub fn to_params(&self) -> qsm_core::swi::SwiParams {
        qsm_core::swi::SwiParams {
            unwrap: self.unwrap.into(),
            hp_sigma: self.hp_sigma,
            scaling: self.scaling.into(),
            strength: self.strength,
            mip_window: self.mip_window,
            ..Default::default()
        }
    }
}
impl Default for SwiConfig {
    fn default() -> Self {
        let p = qsm_core::swi::SwiParams::default();
        Self {
            unwrap: p.unwrap.into(),
            hp_sigma: p.hp_sigma,
            scaling: p.scaling.into(),
            strength: p.strength,
            mip_window: p.mip_window,
        }
    }
}

// ─── Top-level config and section structs ───

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub description: String,
    /// The settings-file layout this config was written with ([`crate::migrate`]): an older file
    /// is brought up to date when it is read.
    pub schema_version: crate::migrate::SchemaVersion,
    pub pipeline: PipelineToggles,
    pub field_mapping: FieldMappingConfig,
    pub masking: MaskingConfig,
    pub bg_removal: BgRemovalConfig,
    pub inversion: InversionConfig,
    pub separation: SeparationConfig,
    pub qsm: QsmConfig,
    pub swi: SwiConfig,
    pub smwi: SmwiConfig,
    pub segmentation: SegmentationConfig,
    pub bet: BetConfig,
    pub homogeneity: HomogeneityConfig,
    /// N4 inhomogeneity correction (`masking.inhomogeneity_method = "n4"`).
    #[serde(default)]
    pub n4: N4Config,
    #[serde(default)]
    pub multi_orientation: MultiOrientationConfig,
    /// Rigid co-registration of the echoes before field mapping. Off by default.
    #[serde(default)]
    pub motion_correction: crate::motion::MotionCorrectionConfig,
    /// Rigid registration settings, shared by motion correction and multi-orientation
    /// co-registration (`--registration-*`).
    #[serde(default)]
    pub registration: crate::motion::RegistrationConfig,
}

/// Multi-orientation reconstruction (COSMOS / STI) over a group of runs.
///
/// Off unless `group_by` names a pattern. BIDS has no entity meaning "same object, different
/// orientation", so the dataset cannot declare the intention — the user does, by setting
/// this. The pattern's only job is to say *which runs* belong together; see
/// `crate::bids::orientation` in the qsmxt crate for its three tiers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MultiOrientationConfig {
    /// Orientation-group pattern: an entity name (`acq`, `run`, `rec`, `inv`), a glob over the
    /// run key (`*acq-dir*`), or `re:` and a regex with one capture group. Empty = off.
    pub group_by: String,
    /// Which reconstruction to run over each group.
    pub algorithm: MultiOrientAlgorithm,
    /// Regularization: COSMOS's k-space L2 weight, or STI's per-k-point ridge term. 0 is plain
    /// least squares, which is right whenever the orientations cover k-space.
    pub lambda: f64,
    /// Reconstruct even when the direction set looks degenerate. The check exists because a
    /// degenerate set produces a plausible map that is not the method asked for, so this
    /// should stay off unless the metadata is known-good and the check is known-wrong.
    pub force: bool,
    /// Allow QSMxT to rigidly co-register the orientations onto the first member's grid.
    ///
    /// On by default, and it costs nothing when there is nothing to do: registration runs only
    /// when the group actually needs it — the members disagree on dimensions or affine, or the
    /// only directions available came from affines that cannot tell the orientations apart.
    /// A group already on one grid with declared `B0_dir`s is left alone.
    ///
    /// Turn it off when the orientations were co-registered externally (FLIRT, ANTs) and you do
    /// not want a second interpolation on top. Then a group that still needs registration is
    /// refused rather than reconstructed, which is the behaviour QSMxT had before it could
    /// register at all.
    #[serde(default = "default_true")]
    pub register: bool,
    /// COSMOS only: weight each orientation's field by its combined magnitude and solve by
    /// conjugate gradient (`qsm_core::inversion::cosmos_weighted`), so low-signal voxels count
    /// for less. Off is the closed-form, unweighted least squares.
    pub magnitude_weighting: bool,
    /// Conjugate-gradient tolerance of magnitude-weighted COSMOS; unused without weighting.
    pub cosmos_tol: f64,
    /// Conjugate-gradient iteration cap of magnitude-weighted COSMOS; unused without weighting.
    pub cosmos_max_iter: usize,
}

fn default_true() -> bool {
    true
}

impl Default for MultiOrientationConfig {
    fn default() -> Self {
        Self {
            group_by: String::new(),
            algorithm: MultiOrientAlgorithm::Cosmos,
            lambda: 0.0,
            force: false,
            register: true,
            magnitude_weighting: false,
            cosmos_tol: qsm_core::inversion::CosmosParams::default().tol,
            cosmos_max_iter: qsm_core::inversion::CosmosParams::default().max_iter,
        }
    }
}

impl MultiOrientationConfig {
    /// Whether multi-orientation grouping is switched on.
    pub fn enabled(&self) -> bool {
        let g = self.group_by.trim();
        !g.is_empty() && !g.eq_ignore_ascii_case("none")
    }

    /// Why the COSMOS solver settings cannot be used as given, if they cannot: the CG settings
    /// only drive magnitude-weighted COSMOS, and STI has no magnitude weighting.
    pub fn validate(&self) -> Result<(), String> {
        let d = Self::default();
        if self.magnitude_weighting && self.algorithm == MultiOrientAlgorithm::Sti {
            return Err("magnitude weighting (--cosmos-magnitude-weighting) is a COSMOS option; \
                        STI has none".into());
        }
        if !self.magnitude_weighting
            && (self.cosmos_tol != d.cosmos_tol || self.cosmos_max_iter != d.cosmos_max_iter)
        {
            return Err("--cosmos-tol and --cosmos-max-iter set the conjugate gradient of \
                        magnitude-weighted COSMOS; without --cosmos-magnitude-weighting COSMOS \
                        is solved in closed form and they would be ignored".into());
        }
        if self.cosmos_tol <= 0.0 || !self.cosmos_tol.is_finite() || self.cosmos_max_iter == 0 {
            return Err(format!("COSMOS tolerance {} and iteration cap {} must be positive",
                               self.cosmos_tol, self.cosmos_max_iter));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MultiOrientAlgorithm {
    /// Scalar susceptibility from 2+ orientations, closed form in k-space.
    #[default]
    Cosmos,
    /// Rank-2 susceptibility tensor from 6+ orientations.
    Sti,
}

impl std::fmt::Display for MultiOrientAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiOrientAlgorithm::Cosmos => write!(f, "cosmos"),
            MultiOrientAlgorithm::Sti => write!(f, "sti"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PipelineToggles {
    pub do_qsm: bool,
    pub do_swi: bool,
    /// Susceptibility map-weighted imaging — weights the magnitude by a mask built from χ rather
    /// than from filtered phase. Needs a susceptibility map, so it implies QSM.
    #[serde(default)]
    pub do_smwi: bool,
    /// Whole-brain parcellation of the GRE magnitude via SynthSeg (`_dseg.nii`).
    #[serde(default)]
    pub do_segmentation: bool,
    /// Per-structure statistics over the segmentation, for every quantitative map the run made.
    /// Needs a segmentation, so it implies one; and needs at least one map, so it implies QSM only
    /// when nothing else would produce one.
    #[serde(default)]
    pub do_analysis: bool,
    pub do_t2starmap: bool,
    pub do_r2starmap: bool,
    /// Compute an R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG.
    pub do_r2map: bool,
    /// Compute an R2' map (Hz) = R2* − R2 (needs GRE magnitude + a MESE acquisition).
    pub do_r2primemap: bool,
    pub do_chi_separation: bool,
    pub export_dicom: bool,
    /// Resample oblique acquisitions onto an axial grid when their obliquity exceeds
    /// `obliquity_threshold`. Off by default: the classical methods take the B0 direction and are
    /// correct on the acquired grid. Methods that only work on axial data resample regardless.
    pub axial_resampling: bool,
    /// Degrees of obliquity above which `axial_resampling` resamples (non-negative).
    pub obliquity_threshold: f64,
    /// Reconstruct inside a box around the mask rather than the whole field of view. Pure speed
    /// on paper, but it moves the FFT's periodic boundary closer to the object, so it is opt-in.
    #[serde(default)]
    pub crop_to_mask: bool,
    /// Margin in millimetres around the mask when `crop_to_mask` is set. Measured on a UK
    /// Biobank acquisition, 16 mm moved chi by 0.8% of dynamic range at the median and 5.1% at
    /// p99; 32 mm and 64 mm were identical to the uncropped reconstruction. 32 mm is the
    /// smallest margin that costs nothing in accuracy.
    /// Grow the grid outward to an FFT-friendly size before the FFT-bound stages.
    ///
    /// Off by default. It discards nothing, but it is not a no-op: the dipole kernel is sampled
    /// at `k = n / (N dx)`, so changing `N` changes where k-space is sampled and the
    /// deconvolution along with it. Measured on a resampled UK Biobank grid, a TKD
    /// reconstruction was unchanged at the median (4e-7 ppm) with a tail reaching 5.7% of
    /// dynamic range at p99; WH-QSM, being iterative, moved 0.06% at the median. Worth having
    /// where an awkward grid makes the FFT expensive, but not worth changing everyone's numbers
    /// for by default.
    #[serde(default)]
    pub fft_padding: bool,
    #[serde(default = "default_crop_margin_mm")]
    pub crop_margin_mm: f64,
    /// Which grid the derivatives are written on when the run was resampled to axial. `Acquired`
    /// (the default) puts them back where the data came from, so transforms the caller already
    /// holds still apply; `Working` leaves them on the resampled grid.
    #[serde(default)]
    pub output_space: OutputSpace,
    /// Reconstruct even when the sidecar's `SliceThickness` says the slices leave a gap (a 2D
    /// multi-slice acquisition thinner than its pitch). Off by default: the FFT kernels assume a
    /// contiguous grid, so a gapped volume gives a wrong susceptibility map.
    #[serde(default)]
    pub ignore_slice_gap: bool,
}
impl Default for PipelineToggles {
    fn default() -> Self {
        Self { do_qsm: true, do_swi: false, do_smwi: false,
            do_segmentation: false, do_analysis: false, do_t2starmap: false, do_r2starmap: false, do_r2map: false, do_r2primemap: false, do_chi_separation: false, export_dicom: false, axial_resampling: false, obliquity_threshold: DEFAULT_OBLIQUITY_THRESHOLD,
            crop_to_mask: false, fft_padding: false, crop_margin_mm: default_crop_margin_mm(),
            output_space: OutputSpace::Acquired, ignore_slice_gap: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FieldMappingConfig {
    pub phase_offset_removal: bool,
    /// Gaussian smoothing sigma of the MCPC-3D-S phase offsets, in mm per axis (divided by the
    /// voxel size to give voxels). Default 7 mm, the ROMEO command line's.
    pub phase_offset_sigma_mm: [f64; 3],
    /// Smoothing sigma (voxels) for the per-coil phase offsets in MCPC-3D-S coil combination
    /// of uncombined multi-coil runs. Default [10, 10, 5] = MriResearchTools / QSMxT 8.x.
    #[serde(default = "default_coil_combination_sigma")]
    pub coil_combination_sigma: [f64; 3],
    pub bipolar_correction: bool,
    pub unwrapping_algorithm: UnwrappingAlgorithm,
    pub b0_estimation: B0Estimation,
    pub b0_weight_type: B0WeightType,
    /// T2* (ms) assumed by `b0_weight_type = "assumed-decay"` to compute its fixed per-echo
    /// weights (no T2* map is used). Default 40 ms, UK Biobank's.
    #[serde(default = "default_b0_weight_assumed_t2star_ms")]
    pub b0_weight_assumed_t2star_ms: f64,
    /// Poisson solver used by `unwrapping_algorithm = "laplacian"`.
    #[serde(default, alias = "laplacian_kernel")]
    pub laplacian_solver: LaplacianSolver,
    /// Zero-padding (voxels per side) of the `fft` solver, per axis (x, y, z). Default 64 on
    /// every axis, UK Biobank's (qsm-core's own default is STI Suite's 12). A config file may give
    /// one number for all three.
    #[serde(default = "default_laplacian_fft_pad", deserialize_with = "deserialize_pad3")]
    pub laplacian_fft_pad: [usize; 3],
    pub romeo: RomeoConfig,
    /// The linear fit's settings. `estimate_offset` and `reliability_threshold_percentile` are
    /// shared with the robust fit, where they mean the same thing.
    pub linear_fit: LinearFitConfig,
    /// The motion-robust echo fit (`b0_estimation = "robust-fit"`).
    #[serde(default)]
    pub robust_fit: RobustFitConfig,
    /// How QSMART's echo fit (`b0_estimation = "qsmart-echofit"`) sets the residual threshold
    /// at which a voxel is left out of QSMART's reconstruction: a percentile of the blurred fit
    /// residual (default, as QSMART's authors run it) or a fixed residual
    /// (`qsm_core::utils::QsmartFitThreshold`).
    #[serde(default)]
    pub qsmart_fit_threshold_mode: QsmartFitThresholdMode,
    /// The percentile, in (0, 100], with `qsmart_fit_threshold_mode = "percentile"`. Default
    /// qsm-core's (`QSMART_FIT_PERCENTILE`, 99).
    #[serde(default = "default_qsmart_fit_percentile")]
    pub qsmart_fit_percentile: f64,
    /// The fixed residual threshold with `qsmart_fit_threshold_mode = "absolute"`. Default
    /// qsm-core's (`QSMART_FIT_THRESHOLD`, 40).
    #[serde(default = "default_qsmart_fit_threshold")]
    pub qsmart_fit_threshold: f64,
    /// The user chose `unwrapping_algorithm`, `laplacian_solver` or `b0_estimation`, so the run
    /// keeps it as it is. Left unset, the general default gives way to the inversion's own where
    /// it has one: QSMART's Laplacian unwrapping with the `mirror` solver and its echo fit
    /// ([`PipelineConfig::normalised`]). As [`MaskingConfig::recipe_chosen`]: normalising keeps
    /// a mark only where it decides something, and it is written out only when set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unwrapping_chosen: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub laplacian_solver_chosen: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub b0_estimation_chosen: bool,
}

impl FieldMappingConfig {
    /// Why these settings cannot be run together, if they cannot: QSMART's echo fit follows its
    /// own Laplacian unwrapping. (It has no bipolar correction either, but bipolar correction
    /// belongs to ROMEO's phase offset removal and is not applied with Laplacian unwrapping.)
    pub fn validate(&self) -> Result<(), String> {
        if self.b0_estimation == B0Estimation::QsmartEchofit && self.unwrapping_algorithm != UnwrappingAlgorithm::Laplacian {
            return Err(format!(
                "B0 estimation by QSMART's echo fit needs Laplacian unwrapping, not {}: choose Laplacian \
                 unwrapping, or another B0 estimation", self.unwrapping_algorithm.to_string().to_uppercase()));
        }
        if self.b0_estimation == B0Estimation::QsmartEchofit {
            self.qsmart_fit_threshold().validate()?;
        }
        Ok(())
    }

    /// QSMART's echo-fit threshold as qsm-core takes it: the setting `qsmart_fit_threshold_mode`
    /// selects.
    pub fn qsmart_fit_threshold(&self) -> qsm_core::utils::QsmartFitThreshold {
        match self.qsmart_fit_threshold_mode {
            QsmartFitThresholdMode::Percentile => qsm_core::utils::QsmartFitThreshold::Percentile(self.qsmart_fit_percentile),
            QsmartFitThresholdMode::Absolute => qsm_core::utils::QsmartFitThreshold::Absolute(self.qsmart_fit_threshold),
        }
    }
}

/// Motion-robust echo fit (`qsm_core::fieldmap::RobustFitParams`): an IRLS fit of phase against
/// TE that down-weights echoes departing from a straight line, voxel by voxel. Its
/// `estimate_offset` and `reliability_threshold_percentile` are `linear_fit`'s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RobustFitConfig {
    pub loss: RobustLoss,
    /// Tuning radius in robust-scale units; `None` = the loss's conventional value
    /// (Tukey 4.685, Huber 1.345).
    pub tuning: Option<f64>,
    /// IRLS passes; 0 = report only (the field map is the linear fit's).
    pub iterations: usize,
    /// Reporting only: a weight below this counts as down-weighted.
    pub downweight_level: f64,
    /// Reporting only: flag an echo whose outlier score exceeds this × the best echo's.
    pub flag_ratio: f64,
}
impl Default for RobustFitConfig {
    fn default() -> Self {
        let p = qsm_core::fieldmap::RobustFitParams::default();
        Self {
            loss: match p.loss {
                qsm_core::fieldmap::RobustLoss::Tukey => RobustLoss::Tukey,
                qsm_core::fieldmap::RobustLoss::Huber => RobustLoss::Huber,
            },
            tuning: p.tuning,
            iterations: p.iterations,
            downweight_level: p.downweight_level,
            flag_ratio: p.flag_ratio,
        }
    }
}
pub fn default_b0_weight_assumed_t2star_ms() -> f64 { 40.0 }
pub fn default_qsmart_fit_percentile() -> f64 {
    match qsm_core::utils::QsmartFitThreshold::default() {
        qsm_core::utils::QsmartFitThreshold::Percentile(p) => p,
        qsm_core::utils::QsmartFitThreshold::Absolute(_) => qsm_core::utils::QSMART_FIT_PERCENTILE,
    }
}
pub fn default_qsmart_fit_threshold() -> f64 {
    match qsm_core::utils::QsmartFitThreshold::default() {
        qsm_core::utils::QsmartFitThreshold::Absolute(t) => t,
        qsm_core::utils::QsmartFitThreshold::Percentile(_) => qsm_core::utils::QSMART_FIT_THRESHOLD,
    }
}
pub fn default_laplacian_fft_pad() -> [usize; 3] { [64; 3] }

/// A per-axis count given as one number for every axis or as three.
pub fn pad3_from(values: &[usize]) -> Option<[usize; 3]> {
    match *values {
        [n] => Some([n; 3]),
        [x, y, z] => Some([x, y, z]),
        _ => None,
    }
}

fn deserialize_pad3<'de, D: serde::Deserializer<'de>>(d: D) -> Result<[usize; 3], D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Pad { One(usize), Many(Vec<usize>) }
    let v = match Pad::deserialize(d)? { Pad::One(n) => vec![n], Pad::Many(v) => v };
    pad3_from(&v).ok_or_else(|| serde::de::Error::custom(format!(
        "laplacian_fft_pad: give one number or three, got {v:?}")))
}
fn default_coil_combination_sigma() -> [f64; 3] { [10.0, 10.0, 5.0] }

fn default_crop_margin_mm() -> f64 { 32.0 }

/// Grid the derivatives are written on after an axial resampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OutputSpace {
    /// The grid the data was acquired on — a second interpolation, but the space any
    /// precomputed transform expects.
    #[default]
    Acquired,
    /// The grid the pipeline reconstructed on.
    Working,
}

impl std::fmt::Display for OutputSpace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputSpace::Acquired => write!(f, "acquired"),
            OutputSpace::Working => write!(f, "working"),
        }
    }
}

impl Default for FieldMappingConfig {
    fn default() -> Self {
        Self {
            phase_offset_removal: true,
            phase_offset_sigma_mm: qsm_core::pipeline::FieldMappingConfig::default().phase_offset_sigma_mm,
            coil_combination_sigma: default_coil_combination_sigma(),
            bipolar_correction: false,
            unwrapping_algorithm: UnwrappingAlgorithm::Romeo,
            b0_estimation: B0Estimation::WeightedAvg,
            b0_weight_type: B0WeightType::PhaseSNR,
            b0_weight_assumed_t2star_ms: default_b0_weight_assumed_t2star_ms(),
            laplacian_solver: LaplacianSolver::Dct,
            laplacian_fft_pad: default_laplacian_fft_pad(),
            romeo: RomeoConfig::default(),
            linear_fit: LinearFitConfig::default(),
            robust_fit: RobustFitConfig::default(),
            qsmart_fit_threshold_mode: QsmartFitThresholdMode::default(),
            qsmart_fit_percentile: default_qsmart_fit_percentile(),
            qsmart_fit_threshold: default_qsmart_fit_threshold(),
            unwrapping_chosen: false,
            laplacian_solver_chosen: false,
            b0_estimation_chosen: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MaskingConfig {
    pub inhomogeneity_correction: bool,
    /// How the magnitude is corrected for inhomogeneity: `makehomogeneous` (default) or `n4`.
    #[serde(default)]
    pub inhomogeneity_method: InhomogeneityMethod,
    pub sections: Vec<MaskSection>,
    /// How `sections` fold into one mask: `or` (union, the default) or `and` (intersection).
    #[serde(default)]
    pub combine: MaskCombine,
    /// Refinements applied to the combined mask, after `combine` — e.g. filling the holes an
    /// intersection leaves inside the brain. Generator ops are rejected here.
    #[serde(default)]
    pub refinements: Vec<MaskOp>,
    /// Prefer a bring-your-own mask from BIDS derivatives when present, falling back to `sections`.
    /// `Some("*")` = first matching derivatives tool alphabetically; `Some("bet")` = that tool only;
    /// `None` = always compute the mask from `sections`.
    #[serde(default)]
    pub custom_mask_tool: Option<String>,
    /// Reconstruct a second time against a mask whose holes are intact, and keep that
    /// reconstruction wherever it is defined. See [`two_pass_sections`](Self::two_pass_sections).
    #[serde(default)]
    pub two_pass: bool,
    /// The reliable-pass mask, when `two_pass` is set. `None` = [`default_two_pass_sections`]
    /// (an Otsu threshold on the phase-quality map, unfilled).
    ///
    /// An ordinary list of mask sections, folded with the same `combine` and `refinements` as the
    /// main mask, so anything expressible for one mask is expressible for this one.
    #[serde(default)]
    pub two_pass_sections: Option<Vec<MaskSection>>,
    /// The user chose `sections` (picked a preset, the default one included, or edited the
    /// recipe), so the run keeps them as they are. Left unset, the general default recipe gives
    /// way to the inversion's own default where it has one: QSMART's BET mask
    /// ([`crate::masking::default_mask_sections_for`], applied by
    /// [`PipelineConfig::normalised`]).
    ///
    /// Normalising keeps it set only where it decides something (QSMART with the general default
    /// recipe), and it is written out only when set, so a saved configuration says it only then.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recipe_chosen: bool,
}
impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            inhomogeneity_correction: true,
            inhomogeneity_method: InhomogeneityMethod::default(),
            sections: default_mask_sections(),
            combine: MaskCombine::default(),
            refinements: vec![],
            custom_mask_tool: None,
            two_pass: false,
            two_pass_sections: None,
            recipe_chosen: false,
        }
    }
}

impl MaskingConfig {
    /// Why a step of the recipe cannot be run, if one cannot ([`MaskOp::validate`]), naming
    /// where the step is.
    pub fn validate(&self) -> Result<(), String> {
        let sections = |name: &str, list: &[MaskSection]| -> Result<(), String> {
            for (i, s) in list.iter().enumerate() {
                s.generator.validate().map_err(|e| format!("masking.{name}[{i}].generator: {e}"))?;
                for (j, op) in s.refinements.iter().enumerate() {
                    op.validate().map_err(|e| format!("masking.{name}[{i}].refinements[{j}]: {e}"))?;
                }
            }
            Ok(())
        };
        sections("sections", &self.sections)?;
        for (j, op) in self.refinements.iter().enumerate() {
            op.validate().map_err(|e| format!("masking.refinements[{j}]: {e}"))?;
        }
        if let Some(two) = &self.two_pass_sections {
            sections("two_pass_sections", two)?;
        }
        Ok(())
    }

    /// The reliable-pass mask sections: the configured ones, or the default recipe.
    pub fn resolved_two_pass_sections(&self) -> Vec<MaskSection> {
        self.two_pass_sections.clone().unwrap_or_else(default_two_pass_sections)
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
    pub iharperella: IharperellaConfig,
    /// The user chose `algorithm`, so the run keeps it. Left unset, the general default gives way
    /// to the inversion's own reference background removal where it has one: PDF for MEDI
    /// (QSM.rs `InversionConfig::reference_bg_removal`, applied by
    /// [`PipelineConfig::normalised`]).
    ///
    /// Normalising keeps it set only where it decides something (MEDI with the general default),
    /// and it is written out only when set, as [`MaskingConfig::recipe_chosen`] is. Last, so the
    /// cache key can drop it without reordering the rest.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub algorithm_chosen: bool,
}
impl Default for BgRemovalConfig {
    fn default() -> Self {
        Self {
            algorithm: BfAlgorithm::Vsharp,
            vsharp: VsharpConfig::default(), pdf: PdfConfig::default(),
            lbv: LbvConfig::default(), ismv: IsmvConfig::default(),
            sharp: SharpConfig::default(), resharp: ResharpConfig::default(),
            harperella: HarperellaConfig::default(), iharperella: IharperellaConfig::default(),
            algorithm_chosen: false,
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
    pub star: StarConfig,
    pub tgv: TgvConfig,
    pub qsmart: QsmartConfig,
    pub ndi: NdiConfig,
    pub fansi: FansiConfig,
    pub l1qsm: L1qsmConfig,
    pub whqsm: WhqsmConfig,
    pub hdqsm: HdqsmConfig,
    pub amp_pe: AmpPeConfig,
    /// Shared by `lsqr` and `heidi` — HEIDI's seed is an LSQR solve.
    pub lsqr: LsqrConfig,
    pub heidi: HeidiConfig,
    /// Overlap-tiling for the deep-learning networks (bounded memory). `tile_size` = the output
    /// core per patch (voxels); its presence enables tiling. `tile_halo` = context margin per side
    /// (voxels; a default is used when omitted). `None` = whole-volume (the default). Applies to
    /// every network [`QsmAlgorithm::tileable`] or [`BfAlgorithm::tileable`] names, including
    /// BFRnet and iQFM in background removal; ignored by classical algorithms and the
    /// natively-patch-based nets (autoqsm/qsmgan).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile_halo: Option<usize>,
}
/// Context margin per side (voxels) of a tiled deep-learning inversion when `tile_halo` is
/// omitted — qsm-core's `TileConfig` default.
pub const DEFAULT_TILE_HALO: usize = 8;

/// iLSQR with QSM.rs's STI Suite-matched parameters is the default inversion: on the UK Biobank
/// pilot it tracks age and longitudinal change better than RTS, matching UKB's own pipeline.
impl Default for InversionConfig {
    fn default() -> Self {
        Self {
            algorithm: QsmAlgorithm::Ilsqr,
            rts: RtsConfig::default(), tv: TvConfig::default(),
            tkd: TkdConfig::default(), tsvd: TkdConfig::default(),
            tikhonov: TikhonovConfig::default(), nltv: NltvConfig::default(),
            medi: MediConfig::default(), tfi: TfiConfig::default(), ilsqr: IlsqrConfig::default(),
            star: StarConfig::default(), tgv: TgvConfig::default(), qsmart: QsmartConfig::default(),
            ndi: NdiConfig::default(), fansi: FansiConfig::default(),
            l1qsm: L1qsmConfig::default(), whqsm: WhqsmConfig::default(),
            hdqsm: HdqsmConfig::default(), amp_pe: AmpPeConfig::default(),
            lsqr: LsqrConfig::default(), heidi: HeidiConfig::default(),
            tile_size: None, tile_halo: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct QsmConfig {
    pub reference: QsmReference,
    /// The parcellation region the map is referenced to, when `reference = region`.
    ///
    /// Held as the spec the user wrote (`thalamus`, `left-thalamus,right-caudate`, `4,43`) rather
    /// than resolved ids: the ids depend on the SynthSeg generation, which the user can still
    /// change after setting this, and an error naming what they typed beats one naming a number
    /// they never saw. Resolved by `qsmxt_engine::regions::resolve` where it is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_region: Option<String>,
}
impl Default for QsmConfig {
    fn default() -> Self { Self { reference: QsmReference::Mean, reference_region: None } }
}

impl QsmConfig {
    /// The reference as a single `--qsm-reference` value: `mean`, `none`, or the region spec.
    pub fn reference_spec(&self) -> String {
        match (self.reference, self.reference_region.as_deref()) {
            (QsmReference::Region, Some(r)) => r.to_string(),
            (other, _) => other.to_string(),
        }
    }
}


/// The obliquity (degrees) above which `axial_resampling` resamples, unless set.
pub const DEFAULT_OBLIQUITY_THRESHOLD: f64 = 10.0;

impl PipelineConfig {
    /// The overlap tiling the deep-learning networks run with, `(core, halo)` in voxels, or
    /// `None` for whole-volume: `inversion.tile_size`, with the default halo when none is set.
    pub fn dl_tile(&self) -> Option<(usize, usize)> {
        self.inversion.tile_size.map(|core| (core, self.inversion.tile_halo.unwrap_or(DEFAULT_TILE_HALO)))
    }

    /// Whether a network this run uses takes the tiling: a tileable inversion, or a tileable
    /// background removal (BFRnet, iQFM) in a run that removes the background.
    pub fn runs_a_tileable_network(&self) -> bool {
        self.inversion.algorithm.tileable()
            || (self.bg_removal.algorithm.tileable() && !crate::bridge::skips_bgremove(self))
    }
}

impl PipelineConfig {
    pub fn from_toml(s: &str) -> crate::Result<Self> {
        Self::from_toml_migrating(s).map(|(c, _)| c)
    }

    /// Parse a TOML config, mapping settings older QSMxT versions wrote onto today's, with a
    /// warning for each mapping the caller should show.
    ///
    /// QSMxT up to v9.22.0 had no `axial_resampling`: a negative `obliquity_threshold` meant
    /// "never resample", any other value "resample above it". Such a config (a threshold and no
    /// `axial_resampling`) keeps its meaning. A config that has `axial_resampling` must give a
    /// non-negative threshold.
    pub fn from_toml_migrating(s: &str) -> crate::Result<(Self, Vec<String>)> {
        Self::from_toml_noting(s).map(|(c, m)| (c, m.warnings.into_iter().chain(m.notes).collect()))
    }

    /// [`from_toml_migrating`](Self::from_toml_migrating), with the warnings apart from the notes
    /// that a setting took today's default ([`crate::migrate`]), for a caller that shows them
    /// differently.
    pub fn from_toml_noting(s: &str) -> crate::Result<(Self, crate::migrate::Migration)> {
        let parse_err = |e: toml::de::Error| crate::error::ConfigError::Parse(format!("{}", e));
        let mut value: toml::Table = toml::from_str(s).map_err(parse_err)?;
        let mut notes = crate::migrate::Migration::default();
        // Before anything is added to the file: what an older file holds says which release
        // wrote it.
        crate::migrate::migrate(&mut value, &mut notes)?;
        let warnings = &mut notes.warnings;
        if let Some(toml::Value::Table(p)) = value.get_mut("pipeline") {
            let threshold = p.get("obliquity_threshold").and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
            match (threshold, p.contains_key("axial_resampling")) {
                (Some(t), false) => {
                    let on = t >= 0.0;
                    p.insert("axial_resampling".into(), toml::Value::Boolean(on));
                    if !on {
                        p.insert("obliquity_threshold".into(), toml::Value::Float(DEFAULT_OBLIQUITY_THRESHOLD));
                    }
                    warnings.push(if on {
                        format!("pipeline.obliquity_threshold = {t} without axial_resampling is deprecated: \
                                 read as axial_resampling = true, obliquity_threshold = {t}")
                    } else {
                        format!("pipeline.obliquity_threshold = {t} is deprecated: a negative threshold \
                                 meant no resampling, read as axial_resampling = false")
                    });
                }
                (Some(t), true) if t < 0.0 => {
                    return Err(crate::error::ConfigError::Parse(format!(
                        "pipeline.obliquity_threshold = {t}: the threshold is in degrees and cannot be negative; \
                         set axial_resampling = false to turn resampling off")));
                }
                _ => {}
            }
        }
        let config: Self = toml::Value::Table(value).try_into().map_err(parse_err)?;
        config.masking.validate().map_err(crate::error::ConfigError::Parse)?;
        Ok((config, notes))
    }

    /// Parse a JSON config (as a front end keeps one, e.g. qsmbly's saved settings), bringing one
    /// without `schema_version` up to date as [`from_toml_noting`](Self::from_toml_noting) does a
    /// settings file. A config with `schema_version` is read as it is.
    pub fn from_json_migrating(s: &str) -> crate::Result<(Self, crate::migrate::Migration)> {
        let parse_err = |e: String| crate::error::ConfigError::Parse(e);
        let json: serde_json::Value = serde_json::from_str(s).map_err(|e| parse_err(e.to_string()))?;
        if json.get("schema_version").is_some() {
            let config = serde_json::from_value(json).map_err(|e| parse_err(e.to_string()))?;
            return Ok((config, Default::default()));
        }
        // TOML has no null: an unset setting is simply absent, which reads the same.
        fn drop_nulls(v: serde_json::Value) -> serde_json::Value {
            match v {
                serde_json::Value::Object(m) => serde_json::Value::Object(
                    m.into_iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k, drop_nulls(v))).collect()),
                serde_json::Value::Array(a) => serde_json::Value::Array(a.into_iter().map(drop_nulls).collect()),
                v => v,
            }
        }
        let table = toml::Table::try_from(drop_nulls(json)).map_err(|e| parse_err(e.to_string()))?;
        let text = toml::to_string(&table).map_err(|e| parse_err(e.to_string()))?;
        Self::from_toml_noting(&text)
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

impl PipelineConfig {
    /// Only the settings that differ from QSMxT's defaults, as a QSMxT configuration file: a short
    /// config that says what was changed. Settings that do not apply to this config are left out
    /// ([`prune_hidden`](crate::schema::prune_hidden)), as the `qsmxt run` command leaves them
    /// out. `qsmxt run --config` fills in the rest with the defaults, so it reads back as this
    /// config, less those settings.
    pub fn to_toml_changed(&self) -> crate::Result<String> {
        let ser = |c: &PipelineConfig| toml::Value::try_from(c)
            .map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)));
        let pruned = crate::schema::prune_hidden(self, &crate::schema::ViewFacts::default());
        let (toml::Value::Table(mut mine), toml::Value::Table(defaults)) = (ser(&pruned)?, ser(&PipelineConfig::default())?) else {
            return Err(crate::error::ConfigError::Serialize("a config is not a TOML table".into()));
        };
        keep_changed(&mut mine, &defaults);
        // Always said, so the file reads back as today's settings rather than an older version's.
        mine.insert("schema_version".into(), toml::Value::Integer(crate::migrate::CONFIG_SCHEMA_VERSION as i64));
        toml::to_string_pretty(&mine).map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }
}

/// Drop from `mine` every value equal to `base`'s, and every table left empty; a value `base` does
/// not have stays. Arrays (a mask recipe's sections) are compared whole.
fn keep_changed(mine: &mut toml::value::Table, base: &toml::value::Table) {
    mine.retain(|key, value| match (value, base.get(key)) {
        (toml::Value::Table(sub), Some(toml::Value::Table(base_sub))) => {
            keep_changed(sub, base_sub);
            !sub.is_empty()
        }
        (value, Some(base_value)) => value != base_value,
        (_, None) => true,
    });
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
    use crate::enums::{BfAlgorithm, QsmAlgorithm, SeparationAlgorithm};

    #[test]
    fn dl_separation_forces_r2prime() {
        // χ-sepnet (like susep-net) consumes local field + QSM + R2', so it must force R2'.
        for alg in [SeparationAlgorithm::SusepNet, SeparationAlgorithm::ChiSepNet] {
            let mut c = PipelineConfig::default();
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = alg;
            enforce_separation_dependencies(&mut c);
            assert!(c.pipeline.do_r2primemap, "{:?} must force R2'", alg);
        }
    }

    /// χ-sep MEDI's defaults come from qsm-core rather than a copy here, so the config, schema,
    /// TUI, command and methods all follow QSM.rs when it retunes them.
    #[test]
    fn chi_sep_medi_defaults_are_qsm_cores() {
        let ours = PipelineConfig::default().separation.chi_sep_medi;
        let core = qsm_core::separation::ChiSepParams::default();
        assert_eq!((ours.lambda_para, ours.lambda_dia, ours.lambda_cpl),
                   (core.lambda_para, core.lambda_dia, core.lambda_cpl));
    }

    /// Configs written before `combine`/`refinements` existed must still load, and keep the
    /// union behaviour they were written with.
    #[test]
    fn a_morphology_step_of_zero_is_refused_in_a_config_file() {
        let toml = "[masking]\n[[masking.sections]]\ninput = \"magnitude\"\ngenerator = { op = \"bet\", fractional_intensity = 0.5 }\n\
                    refinements = [{ op = \"erode\", iterations = 0 }]\n";
        let e = PipelineConfig::from_toml(toml).unwrap_err().to_string();
        assert!(e.contains("masking.sections[0].refinements[0]: erode: iterations must be at least 1, got 0"), "{e}");
        assert!(PipelineConfig::from_toml(&toml.replace("iterations = 0", "iterations = 2")).is_ok());
        let e = PipelineConfig::from_toml("[masking]\nrefinements = [{ op = \"close\", radius = 0 }]\n").unwrap_err().to_string();
        assert!(e.contains("masking.refinements[0]: close: radius must be at least 1, got 0"), "{e}");
    }

    #[test]
    fn masking_combine_defaults_to_or_and_round_trips() {
        let legacy: MaskingConfig = toml::from_str("inhomogeneity_correction = true\n").unwrap();
        assert_eq!(legacy.combine, MaskCombine::Or);
        assert!(legacy.refinements.is_empty());

        let mut c = PipelineConfig::default();
        c.masking.combine = MaskCombine::And;
        c.masking.refinements = vec![MaskOp::FillHoles { max_size: 0 }];
        let loaded = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
        assert_eq!(loaded.masking.combine, MaskCombine::And);
        assert_eq!(loaded.masking.refinements, c.masking.refinements);
    }

    /// The changed settings alone, read back on the defaults, give the config again.
    #[test]
    fn to_toml_changed_writes_only_what_differs_and_roundtrips() {
        let d = PipelineConfig::default();
        assert_eq!(d.to_toml_changed().unwrap().trim(), "schema_version = 2", "nothing changed, only the version written");

        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.inversion.rts.delta = 0.22;
        c.bg_removal.algorithm = BfAlgorithm::Pdf;
        c.pipeline.do_swi = true;
        let toml = c.to_toml_changed().unwrap();
        assert!(toml.contains("delta = 0.22") && toml.contains("do_swi = true"), "{toml}");
        assert!(!toml.contains("[inversion.tkd]") && !toml.contains("[masking]"), "only what changed: {toml}");
        assert!(toml.lines().count() < 15, "{toml}");
        let pruned = |c: &PipelineConfig| crate::schema::prune_hidden(c, &Default::default()).to_json().unwrap();
        assert_eq!(PipelineConfig::from_toml(&toml).unwrap().to_json().unwrap(), pruned(&c));

        // A setting that does not apply is left out: RTS's delta under another inversion.
        let mut hidden = c.clone();
        hidden.inversion.algorithm = QsmAlgorithm::Tkd;
        assert!(!hidden.to_toml_changed().unwrap().contains("delta"));

        // The browser profile reads back the same way: its changes that apply, and nothing else.
        let mut browser = PipelineConfig::browser();
        browser.inversion.algorithm = QsmAlgorithm::Xqsm;
        let toml = browser.to_toml_changed().unwrap();
        assert!(toml.contains("xqsm") && toml.len() * 4 < browser.to_toml().unwrap().len(), "{toml}");
        assert_eq!(PipelineConfig::from_toml(&toml).unwrap().to_json().unwrap(), pruned(&browser));
    }

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

#[cfg(test)]
mod r2primenet_config_tests {
    use super::R2primenetConfig;

    #[test]
    fn a_bad_patch_is_an_error_not_a_panic() {
        let d = R2primenetConfig::default();
        assert!(d.validate().is_ok());
        for bad in [[0, 192, 128], [100, 192, 128]] {
            let c = R2primenetConfig { patch: bad, ..d.clone() };
            assert!(c.validate().unwrap_err().contains("multiple of 16"));
        }
    }

    /// The setting's texts say "multiple of 16"; that is QSM.rs's pooling multiple, its minimum.
    #[test]
    fn the_patch_texts_and_minimum_are_qsm_rs_pooling_multiple() {
        let m = super::R2PRIMENET_POOL_MULTIPLE;
        let s = crate::schema::settings().iter().find(|s| s.path == "separation.r2primenet.patch").unwrap();
        for text in [s.help, s.tui_help] {
            assert!(text.contains(&format!("multiple of {m}")), "{text}");
        }
        assert_eq!(s.min, Some(m as f64));
    }
}

#[cfg(test)]
mod r2prime_strategy_tests {
    use crate::config::{enforce_separation_dependencies, PipelineConfig};
    use crate::enums::R2PrimeStrategy;

    /// R2' needs R2* on every route — it is the minuend of R2* - R2 and R2PRIMEnet's only input.
    /// R2 is needed only where R2' can actually be *measured*; forcing a MESE-derived R2 map for a
    /// run that is going to predict R2' instead would be work with nothing to consume it.
    #[test]
    fn only_the_measuring_routes_force_r2() {
        let cfg = |s| {
            let mut c = PipelineConfig::default();
            c.pipeline.do_r2primemap = true;
            c.separation.r2prime_strategy = s;
            enforce_separation_dependencies(&mut c);
            c
        };

        for s in [R2PrimeStrategy::Auto, R2PrimeStrategy::Mese] {
            let c = cfg(s);
            assert!(c.pipeline.do_r2map, "{s} can measure R2', so it needs R2");
            assert!(c.pipeline.do_r2starmap, "{s} needs R2*");
        }

        let c = cfg(R2PrimeStrategy::R2primenet);
        assert!(!c.pipeline.do_r2map, "predicting R2' subtracts nothing, so it needs no R2");
        assert!(c.pipeline.do_r2starmap, "R2* is R2PRIMEnet's input");

        // A supplied R2' map means none of it has to be computed.
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2primemap = true;
        c.separation.custom_r2prime_tool = Some("*".to_string());
        enforce_separation_dependencies(&mut c);
        assert!(!c.pipeline.do_r2map);
        assert!(!c.pipeline.do_r2starmap);
    }

    /// Chi-separation forces R2', which in turn must not drag in R2 on the predicting route.
    #[test]
    fn chi_separation_inherits_the_strategy() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepIlsqr;
        c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet;
        enforce_separation_dependencies(&mut c);
        assert!(c.pipeline.do_r2primemap, "the method consumes R2'");
        assert!(c.pipeline.do_r2starmap);
        assert!(!c.pipeline.do_r2map, "R2PRIMEnet needs no spin-echo R2");
    }

    #[test]
    fn auto_is_the_default() {
        assert_eq!(PipelineConfig::default().separation.r2prime_strategy, R2PrimeStrategy::Auto);
    }
}

#[cfg(test)]
mod reference_dependency_tests {
    use crate::config::{enforce_reference_dependencies, PipelineConfig};
    use crate::enums::QsmReference;

    /// A region reference is measured on the parcellation, so asking for one asks for SynthSeg.
    /// Erroring instead would make `--qsm-reference thalamus` an incomplete instruction.
    #[test]
    fn a_region_reference_turns_segmentation_on() {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".to_string());
        enforce_reference_dependencies(&mut c);
        assert!(c.pipeline.do_segmentation);
        assert_eq!(c.qsm.reference, QsmReference::Region, "the reference itself is untouched");

        // A supplied dseg is a parcellation too — no need to compute one.
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".to_string());
        c.segmentation.custom_dseg_tool = Some("freesurfer".to_string());
        enforce_reference_dependencies(&mut c);
        assert!(!c.pipeline.do_segmentation, "a supplied dseg needs no SynthSeg run");

        // No reconstruction, nothing to reference: segmentation is not forced for its own sake.
        let mut c = PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".to_string());
        enforce_reference_dependencies(&mut c);
        assert!(!c.pipeline.do_segmentation);
    }

    /// `mean` and `none` force nothing, and a hand-edited config naming no region is not a
    /// reference at all — it falls back rather than referencing to an empty set.
    #[test]
    fn the_other_methods_force_nothing_and_an_empty_region_falls_back() {
        for method in [QsmReference::Mean, QsmReference::None] {
            let mut c = PipelineConfig::default();
            c.qsm.reference = method;
            enforce_reference_dependencies(&mut c);
            assert!(!c.pipeline.do_segmentation, "{method} should force nothing");
            assert_eq!(c.qsm.reference, method);
        }

        for empty in [None, Some(String::new()), Some("  ".to_string())] {
            let mut c = PipelineConfig::default();
            c.qsm.reference = QsmReference::Region;
            c.qsm.reference_region = empty.clone();
            enforce_reference_dependencies(&mut c);
            assert_eq!(c.qsm.reference, QsmReference::Mean, "{empty:?} is not a reference");
            assert_eq!(c.qsm.reference_region, None);
            assert!(!c.pipeline.do_segmentation);
        }
    }

    /// One flag carries both fields, so the pair has to collapse back to a single value.
    #[test]
    fn the_spec_round_trips_through_one_value() {
        let mut c = PipelineConfig::default();
        assert_eq!(c.qsm.reference_spec(), "mean");
        c.qsm.reference = QsmReference::None;
        assert_eq!(c.qsm.reference_spec(), "none");
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("left-thalamus,4".to_string());
        assert_eq!(c.qsm.reference_spec(), "left-thalamus,4");
    }

    /// The region has to survive a config file, or `--config` silently drops the reference.
    #[test]
    fn the_region_survives_a_toml_round_trip() {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("ventricles".to_string());
        let back = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
        assert_eq!(back.qsm.reference, QsmReference::Region);
        assert_eq!(back.qsm.reference_region.as_deref(), Some("ventricles"));

        // And an old config with no region key still loads.
        let old = PipelineConfig::from_toml("[qsm]\nreference = \"mean\"\n").unwrap();
        assert_eq!(old.qsm.reference, QsmReference::Mean);
        assert_eq!(old.qsm.reference_region, None);
    }
}

#[cfg(test)]
mod analysis_dependency_tests {
    /// The statistics need something to average over and something to average, so asking for them
    /// forces both — and a supplied input satisfies its own side without forcing a recomputation.
    #[test]
    fn analysis_forces_segmentation_and_qsm() {
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_analysis = true;
        crate::config::enforce_analysis_dependencies(&mut c);
        assert!(c.pipeline.do_segmentation);
        assert!(c.pipeline.do_qsm);

        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_analysis = true;
        c.segmentation.custom_dseg_tool = Some("freesurfer".to_string());
        c.separation.custom_qsm_tool = Some("*".to_string());
        crate::config::enforce_analysis_dependencies(&mut c);
        assert!(!c.pipeline.do_segmentation, "a supplied dseg needs no SynthSeg run");
        assert!(!c.pipeline.do_qsm, "a supplied Chimap needs no reconstruction");

        // Any map satisfies the map side: the table summarises whatever the run made, so asking
        // for R2* and the statistics must not quietly switch reconstruction back on.
        for other in ["t2star", "r2star", "r2", "r2prime", "chisep"] {
            let mut c = crate::config::PipelineConfig::default();
            c.pipeline.do_qsm = false;
            c.pipeline.do_analysis = true;
            match other {
                "t2star" => c.pipeline.do_t2starmap = true,
                "r2star" => c.pipeline.do_r2starmap = true,
                "r2" => c.pipeline.do_r2map = true,
                "r2prime" => c.pipeline.do_r2primemap = true,
                _ => c.pipeline.do_chi_separation = true,
            }
            crate::config::enforce_analysis_dependencies(&mut c);
            assert!(!c.pipeline.do_qsm, "{other} is a map to summarise — QSM must stay off");
            assert!(c.pipeline.do_segmentation, "{other} still needs something to average over");
        }

        // Segmentation on its own forces nothing: it reads the magnitude, not χ.
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_segmentation = true;
        crate::config::enforce_analysis_dependencies(&mut c);
        assert!(!c.pipeline.do_qsm);
    }
}

#[cfg(test)]
mod smwi_dependency_tests {
    /// SMWI weights the magnitude by χ, so asking for it without a susceptibility map is a request
    /// that cannot be honoured. Turning it on forces QSM rather than silently producing nothing.
    #[test]
    fn smwi_forces_qsm() {
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_smwi = true;
        crate::config::enforce_smwi_dependencies(&mut c);
        assert!(c.pipeline.do_qsm, "SMWI needs a susceptibility map");

        // A bring-your-own Chimap is a susceptibility map too — no need to recompute one.
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_smwi = true;
        c.separation.custom_qsm_tool = Some("*".to_string());
        crate::config::enforce_smwi_dependencies(&mut c);
        assert!(!c.pipeline.do_qsm, "a custom Chimap should not force a reconstruction");

        // And it leaves an ordinary run alone.
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        crate::config::enforce_smwi_dependencies(&mut c);
        assert!(!c.pipeline.do_qsm);
    }
}

#[cfg(test)]
mod option_field_tests {
    /// `LsqrConfig::residual_weighting` is an `Option` whose `None` means "derive it from B0", and
    /// it deliberately carries no `skip_serializing_if`: that attribute would hide the field from
    /// the param-coverage guard, which reads the serialized keys of a default config and would then
    /// report it as unexposed. The attribute is not needed either — `toml` omits a `None` at table
    /// level on its own, which is what this pins.
    #[test]
    fn an_unset_residual_weighting_round_trips_through_toml() {
        let c = crate::config::PipelineConfig::default();
        assert!(c.inversion.lsqr.residual_weighting.is_none(), "None is the default");
        let t = c.to_toml().expect("TOML serialisation of the default config");
        assert!(!t.contains("residual_weighting"), "an unset Option should not be written: {t}");
        let back = crate::config::PipelineConfig::from_toml(&t).expect("round-trip");
        assert_eq!(back.inversion.lsqr.residual_weighting, None);

        // And a set one survives.
        let mut c2 = c.clone();
        c2.inversion.lsqr.residual_weighting = Some(0.25);
        let back2 = crate::config::PipelineConfig::from_toml(
            &c2.to_toml().expect("serialise")).expect("round-trip");
        assert_eq!(back2.inversion.lsqr.residual_weighting, Some(0.25));
    }
}

#[cfg(test)]
mod axial_resampling_tests {
    use super::*;

    /// v9.22.0 configs carry `obliquity_threshold` alone, a negative value meaning "off": they keep
    /// their meaning under `axial_resampling`, with a warning; a new config cannot go negative.
    #[test]
    fn legacy_obliquity_thresholds_map_onto_axial_resampling() {
        let (c, w) = PipelineConfig::from_toml_migrating("[pipeline]\nobliquity_threshold = -1.0\n").unwrap();
        assert!(!c.pipeline.axial_resampling && w.len() == 1, "{w:?}");
        assert_eq!(c.pipeline.obliquity_threshold, DEFAULT_OBLIQUITY_THRESHOLD);
        let (c, w) = PipelineConfig::from_toml_migrating("[pipeline]\nobliquity_threshold = 5\n").unwrap();
        assert!(c.pipeline.axial_resampling && w.len() == 1, "{w:?}");
        assert_eq!(c.pipeline.obliquity_threshold, 5.0);
        // Today's configs: both keys, no warning, round trip.
        let mut d = PipelineConfig::default();
        d.pipeline.axial_resampling = true;
        let (back, w) = PipelineConfig::from_toml_migrating(&d.to_toml().unwrap()).unwrap();
        assert!(w.is_empty() && back.pipeline.axial_resampling);
        let (back, w) = PipelineConfig::from_toml_migrating(&PipelineConfig::default().to_toml().unwrap()).unwrap();
        assert!(w.is_empty() && !back.pipeline.axial_resampling);
        assert!(PipelineConfig::from_toml("[pipeline]\naxial_resampling = true\nobliquity_threshold = -1.0\n").is_err());
        // Nothing said about either: off, at the default threshold.
        let c = PipelineConfig::from_toml("").unwrap();
        assert!(!c.pipeline.axial_resampling);
        assert_eq!(c.pipeline.obliquity_threshold, 10.0);
    }
}

#[cfg(test)]
mod removed_settings_tests {
    use super::*;

    /// Settings files from earlier QSMxT versions still carry two settings since removed, TFI's
    /// `merit` and QSMART's `sdf_spatial_radius` (neither did anything). They load, and the
    /// settings are ignored.
    #[test]
    fn configs_with_removed_settings_still_load() {
        let old = "[inversion]\nalgorithm = \"tfi\"\n\n[inversion.tfi]\nmerit = true\nprecond = 20.0\n\n\
                   [inversion.qsmart]\nsdf_spatial_radius = 8\nvasc_sphere_radius = 5\n";
        let (c, warnings) = PipelineConfig::from_toml_migrating(old).expect("an old config loads");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(c.inversion.algorithm, QsmAlgorithm::Tfi);
        assert_eq!(c.inversion.tfi.precond, 20.0);
        assert_eq!(c.inversion.qsmart.vasc_sphere_radius, 5);
        // Saving it again drops them.
        let saved: toml::Table = toml::from_str(&c.to_toml().unwrap()).unwrap();
        let inversion = saved["inversion"].as_table().unwrap();
        assert!(!inversion["tfi"].as_table().unwrap().contains_key("merit"));
        assert!(!inversion["qsmart"].as_table().unwrap().contains_key("sdf_spatial_radius"));
    }
}

#[cfg(test)]
mod qsmart_inputs_tests {
    use super::*;

    /// Vessel sizes given without their units were written in mm and read as mm (said under
    /// QSMART); with their units, or not given at all, they read as written, in voxels by default.
    #[test]
    fn vessel_sizes_without_units_read_as_mm() {
        let old = "[inversion]\nalgorithm = \"qsmart\"\n\n[inversion.qsmart]\nvasc_sphere_radius = 8\nfrangi_scale_max = 6.0\n";
        let (c, warnings) = PipelineConfig::from_toml_migrating(old).unwrap();
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Mm);
        assert!(warnings.len() == 1 && warnings[0].contains("vessel_size_units"), "{warnings:?}");
        assert!(!warnings[0].contains("--"), "names settings, not flags");
        // Under another inversion the sizes do nothing, so nothing is said.
        let (c, warnings) = PipelineConfig::from_toml_migrating(&old.replace("qsmart\"", "rts\"")).unwrap();
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Mm);
        assert!(warnings.is_empty());
        // Units given: as written.
        let (c, warnings) = PipelineConfig::from_toml_migrating(&format!("{old}vessel_size_units = \"voxels\"\n")).unwrap();
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Voxels);
        assert!(warnings.is_empty());
        // No sizes: the defaults, in voxels.
        let (c, _) = PipelineConfig::from_toml_migrating("[inversion]\nalgorithm = \"qsmart\"\n").unwrap();
        assert_eq!(c.inversion.qsmart, QsmartConfig::default());
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Voxels);
    }

    /// Every file QSMxT writes reads back with the units it was written with: the full one, and
    /// the changed-settings one, which carries the units wherever it carries a size.
    #[test]
    fn written_configs_keep_their_vessel_units() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        c.inversion.qsmart.vasc_sphere_radius = 6;
        for units in [VesselSizeUnits::Voxels, VesselSizeUnits::Mm] {
            c.inversion.qsmart.vessel_size_units = units;
            for text in [c.to_toml().unwrap(), c.to_toml_changed().unwrap()] {
                let (back, warnings) = PipelineConfig::from_toml_migrating(&text).unwrap();
                assert_eq!(back.inversion.qsmart.vessel_size_units, units, "{text}");
                assert_eq!(back.inversion.qsmart.vasc_sphere_radius, 6);
                assert!(warnings.is_empty(), "{warnings:?}");
            }
        }
        // Without a size, the changed-settings file needs no units.
        let mut d = PipelineConfig::default();
        d.inversion.algorithm = QsmAlgorithm::Qsmart;
        assert!(!d.to_toml_changed().unwrap().contains("vessel_size_units"));
    }

    #[test]
    fn the_echo_fit_needs_laplacian_unwrapping() {
        let mut fm = FieldMappingConfig { b0_estimation: B0Estimation::QsmartEchofit, ..Default::default() };
        let e = fm.validate().unwrap_err();
        assert!(e.contains("Laplacian") && e.contains("ROMEO") && !e.contains("--"), "{e}");
        fm.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        fm.bipolar_correction = true; // ROMEO's; not applied with Laplacian unwrapping
        assert!(fm.validate().is_ok());
        assert!(FieldMappingConfig::default().validate().is_ok());
    }
}
