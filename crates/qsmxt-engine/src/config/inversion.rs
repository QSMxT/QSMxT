use super::*;

// ─── Algorithm parameter configs (each ~1-3 lines) ───

// `pad` (RTS, TV, TKD, TSVD, Tikhonov, NLTV): QSM.jl's zero padding before the FFTs, voxels per
// axis: a negative value leaves that axis unpadded, otherwise it is padded by at least that many
// voxels and rounded up to an FFT-efficient size (QSM.jl's `pad`; default 0 0 0).
param_config!(RtsConfig from qsm_core::inversion::RtsParams {
    delta: f64, mu: f64, rho: f64, tol: f64, max_iter: usize, lsmr_iter: usize, tau: f64, gamma: f64,
    pad: [i64; 3]
});
// `rho`: `None` is qsm-core's 100·λ, so it follows λ.
param_config!(TvConfig from qsm_core::inversion::TvParams {
    lambda: f64, rho: Option<f64>, tol: f64, max_iter: usize, pad: [i64; 3]
});
param_config!(TkdConfig from qsm_core::inversion::TkdParams { threshold: f64, pad: [i64; 3] });
param_config!(TsvdConfig from qsm_core::inversion::TsvdParams { threshold: f64, pad: [i64; 3] });
// `te` (here and below): which of the scan's echo times the method converts the field from ppm to
// radians at; each method's default is its reference code's. The scan's echo times and field
// strength are not settings: qsm-core's dispatcher fills them in.
// NLTV's `rho`: `None` is qsm-core's 100·λ, so it follows λ.
param_config!(NltvConfig from qsm_core::inversion::NltvParams {
    lambda: f64, rho: Option<f64>, mu: f64, tol: f64, max_iter: usize, tol_newton: f64,
    max_iter_newton: usize, te: EchoTimeChoice, pad: [i64; 3]
});
// NDI's `weight_norm`: what the magnitude is divided by to make the data weight.
param_config!(NdiConfig from qsm_core::inversion::NdiParams {
    tau: f64, alpha: f64, max_iter: usize, auto_stop: bool, te: EchoTimeChoice, weight_norm: NdiWeightNorm
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
    /// MEDI+0 (Liu et al. 2018): zero-reference the χ map to the ventricular CSF, found from
    /// ARLO R2* on the echo magnitudes, with a CSF-uniformity term in the cost. On by default, as
    /// in MEDI's pipeline. Needs at least three echoes; with fewer, MEDI runs without it.
    pub csf: bool,
    /// Weight of the CSF term (`lambda_CSF`)
    pub lambda_csf: f64,
    /// R2* (Hz) below which a voxel is a CSF candidate (`extract_CSF`'s `thresh_R2s`)
    pub csf_r2s_threshold: f64,
    /// Erode the mask by a 10 mm SMV before finding the CSF (`extract_CSF`'s `flag_erode`)
    pub csf_erode: bool,
    /// Voxels of zeros appended at the end of x, y and z before the inversion (`MEDI_L1`'s
    /// `zeropad`); the result is cropped back.
    pub zeropad: [usize; 3],
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
            csf: p.csf, lambda_csf: p.lambda_csf, csf_r2s_threshold: p.csf_r2s_threshold,
            csf_erode: p.csf_erode, zeropad: [p.zeropad.0, p.zeropad.1, p.zeropad.2],
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
// TFI's `merit`: MEDI_L1's MERIT reweighting; `linear`: the linear data term of Liu et al. 2017
// (eq. 5) rather than MEDI_L1's exponential one.
param_config!(TfiConfig from qsm_core::inversion::TfiParams {
    merit: bool, linear: bool,
    lambda: f64, precond: f64, data_weighting: i32,
    percentage: f64, cg_tol: f64, cg_max_iter: usize, max_iter: usize, tol: f64, te: EchoTimeChoice
});
param_config!(AmpPeConfig from qsm_core::inversion::AmpPeParams {
    wave_order: usize, nlevel: usize, wave_pec: f64, simulated_te: f64,
    max_linearization_ite: usize, b0: f64, gyro_ratio: f64, damp_rate_sig: f64,
    damp_rate_par: f64, max_pe_spar_ite: usize, max_pe_est_ite: usize,
    cvg_thd: f64, tikhonov_beta: f64
});
// `precision`: the LSQR solves in double (default) or in STI Suite's single precision.
param_config!(IlsqrConfig from qsm_core::inversion::IlsqrParams {
    tol: f64, max_iter: usize, pad_mm: f64, precision: IlsqrPrecision
});
impl From<&qsm_core::inversion::IlsqrParams> for IlsqrConfig {
    fn from(p: &qsm_core::inversion::IlsqrParams) -> Self {
        Self { tol: p.tol, max_iter: p.max_iter, pad_mm: p.pad_mm, precision: p.precision.into() }
    }
}
impl IlsqrConfig {
    /// qsm-core's parameters.
    pub fn params(&self) -> qsm_core::inversion::IlsqrParams {
        qsm_core::inversion::IlsqrParams {
            tol: self.tol, max_iter: self.max_iter, pad_mm: self.pad_mm, precision: self.precision.into(),
        }
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

/// HEIDI (Schweser 2012): streaking reduction after the dipole inversion. Its cone fill keeps
/// the well-conditioned k-space of the inversion's χ and refills the dipole cone under a
/// field-homogeneity-weighted TV prior. With `inversion.algorithm = "lsqr"` this is HEIDI as
/// published (LSQR+HEIDI; `heidi` on the command line and in the method picker); after any other
/// local-field inversion it is QSM.rs's `heidi_cone_fill`, whose seed rules (`heidi.*`) say which
/// methods suit it. With two-pass it runs once, on the combined map.
///
/// `HeidiParams::denoise` is an `Option<AnisotropicDiffusionParams>`; it is flattened here into a
/// `denoise` switch plus its three parameters, so it can be a checkbox and three numbers in the
/// TUI rather than a nested table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HeidiConfig {
    /// Run HEIDI after the inversion (QSM.rs `InversionConfig::apply_heidi`). Off by default.
    pub enabled: bool,
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
    /// ITK 3.20's float arithmetic for the denoising (QSM.rs `reference_itk`), as the authors'
    /// code runs it: for comparison with it.
    pub denoise_reference_itk: bool,
    /// Which echo time the field stands for the phase of, in the conversion to HEIDI's units.
    pub te: EchoTimeChoice,
}
impl Default for HeidiConfig {
    fn default() -> Self {
        let p = qsm_core::inversion::HeidiParams::default();
        let d = p.denoise.unwrap_or(qsm_core::utils::AnisotropicDiffusionParams {
            iterations: 5, time_step: 0.05, conductance: 1.0, reference_itk: false,
        });
        Self {
            enabled: false,
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
            denoise_reference_itk: d.reference_itk,
            te: p.te.into(),
        }
    }
}

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
    /// QSM.jl's zero padding before the FFTs (see `RtsConfig`).
    pub pad: [i64; 3],
}
impl Default for TikhonovConfig {
    fn default() -> Self {
        let p = qsm_core::inversion::TikhonovParams::default();
        Self {
            lambda: p.lambda,
            pad: p.pad,
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
    /// Frangi's c in the vessel magnitude's intensity units, used when `frangi_c_mode` is
    /// `absolute` (default 500, QSMART v1.0's).
    pub frangi_c: f64,
    /// How Frangi's c is set: `absolute` (`frangi_c`; default) or `relative` (`frangi_c_factor` ×
    /// the vessel magnitude's median inside the brain mask).
    pub frangi_c_mode: QsmartFrangiCMode,
    /// The factor of a relative Frangi c (default 0.25).
    pub frangi_c_factor: f64,
    /// The magnitude vessels are detected on: the mean over echoes, N4 bias-corrected (default),
    /// or the run's combined magnitude.
    pub vessel_magnitude: QsmartVesselMagnitude,
    /// The dipole grid of the offset adjustment: the FFT grid (default), or `adjust_offset.m`'s
    /// own (`qsmart-reference`), as the QSMART pipeline runs it.
    #[serde(default)]
    pub offset_grid: QsmartOffsetGrid,
    /// The units of `vasc_sphere_radius` and the Frangi scales: voxels (default, as QSMART
    /// specifies them) or mm (converted with the mean voxel size).
    ///
    /// A file without `schema_version` that gives vessel sizes without this field was written
    /// when they were in mm, and reads as mm ([`crate::migrate`]).
    pub vessel_size_units: VesselSizeUnits,
}

impl QsmartConfig {
    /// QSM.rs's Frangi c for these settings: `frangi_c` or `frangi_c_factor`, by `frangi_c_mode`.
    pub fn frangi_c(&self) -> qsm_core::utils::FrangiC {
        match self.frangi_c_mode {
            QsmartFrangiCMode::Absolute => qsm_core::utils::FrangiC::Absolute(self.frangi_c),
            QsmartFrangiCMode::Relative => qsm_core::utils::FrangiC::RelativeToMedian(self.frangi_c_factor),
        }
    }
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
            frangi_c: p.frangi_c.value(),
            frangi_c_mode: QsmartFrangiCMode::default(),
            frangi_c_factor: qsm_core::utils::QSMART_RELATIVE_FRANGI_C,
            vessel_magnitude: QsmartVesselMagnitude::default(),
            offset_grid: QsmartOffsetGrid::default(),
            vessel_size_units: VesselSizeUnits::default(),
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
    pub tsvd: TsvdConfig,
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
    /// LSQR, also HEIDI's reference seed.
    pub lsqr: LsqrConfig,
    /// HEIDI after the inversion ([`HeidiConfig::enabled`]) and its settings.
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
            tkd: TkdConfig::default(), tsvd: TsvdConfig::default(),
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
    /// Reconstruct a QSM from each echo on its own instead of one from the echo-combined field
    /// (qsm-core's `QsmPipelineConfig::qsm_per_echo`): each echo's phase is unwrapped and divided
    /// by its own echo time, its background removed and its field inverted and referenced, and
    /// the mean of the echoes' maps is the QSM ([`crate::bridge::per_echo`]). The echo combination
    /// (`field_mapping.b0_estimation`), phase-offset removal and bipolar correction then do not
    /// run. DECOMPOSE takes the echoes' maps. Off by default.
    pub per_echo: bool,
}
impl Default for QsmConfig {
    fn default() -> Self { Self { reference: QsmReference::Mean, reference_region: None, per_echo: false } }
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
