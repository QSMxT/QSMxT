use super::*;

param_config!(LinearFitConfig from qsm_core::utils::LinearFitParams { estimate_offset: bool });
// MEDI's complex fit (`Fit_ppm_complex`): `reltol` stops the Gauss-Newton iterations once the
// update is this fraction of the first estimate, over the whole volume; `max_iter` caps them.
param_config!(ComplexFitConfig from qsm_core::fieldmap::FitPpmComplexParams {
    reltol: f64, max_iter: usize
});

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
            // ROMEO's own multi-echo default (ROMEO.jl's API and command line): the template echo is
            // unwrapped spatially and the others temporally from it, with no global correction.
            // Individual per-echo unwrapping with global correction across echoes stays an option
            // (motion, time series); with the phase-quality mask it can leave whole lobes a cycle
            // off in single echoes.
            individual: p.individual,
            correct_global: p.correct_global,
            template: p.template,
            phase_coherence: p.phase_coherence,
            phase_gradient_coherence: p.phase_gradient_coherence,
            phase_linearity: p.phase_linearity,
            mag_coherence: p.mag_coherence,
            mag_weight: p.mag_weight,
            mag_weight2: p.mag_weight2,
            bestpath: p.bestpath,
            // The command line's 0 (re-unwrapping of uncertain voxels off; experimental there):
            // QSM.rs does not yet match ROMEO.jl above 0.
            temporal_uncertain_unwrapping: 0.0,
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

/// PRELUDE's settings (qsm-core's `PreludeParams`). `correct_global` stays at qsm-core's default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PreludeConfig {
    /// Number of equal bins the phase range is cut into to form the initial regions.
    pub num_phase_partitions: usize,
    /// How the initial regions are labelled.
    pub labelling: PreludeLabelling,
}
impl Default for PreludeConfig {
    fn default() -> Self {
        let p = qsm_core::unwrap::PreludeParams::default();
        Self {
            num_phase_partitions: p.num_phase_partitions,
            labelling: match p.labelling {
                qsm_core::unwrap::PreludeLabelling::Auto => PreludeLabelling::Auto,
                qsm_core::unwrap::PreludeLabelling::Slices => PreludeLabelling::Slices,
                qsm_core::unwrap::PreludeLabelling::Volume => PreludeLabelling::Volume,
            },
        }
    }
}

impl From<&PreludeConfig> for qsm_core::unwrap::PreludeParams {
    fn from(p: &PreludeConfig) -> Self {
        Self {
            num_phase_partitions: p.num_phase_partitions,
            labelling: p.labelling.into(),
            ..Self::default()
        }
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
    /// Laplacian unwrapping on the raw phase over the whole volume, instead of on the phase zeroed
    /// outside the brain mask (qsm-core's `FieldMappingConfig::laplacian_unmasked`, off by
    /// default). With QSMART's echo fit the unwrapped echoes are then kept over the whole volume,
    /// as QSMART fits them. Only with Laplacian unwrapping; MEDI's complex fit unwraps its fitted
    /// phase inside the mask either way.
    pub laplacian_unmasked: bool,
    /// Keep the Laplacian-unwrapped field over the whole volume, not only inside the brain mask
    /// (qsm-core's `FieldMappingConfig::laplacian_keep_outside`, off by default): UK Biobank's
    /// pipeline hands V-SHARP its combined phase over the whole volume, and STI Suite's V-SHARP
    /// reads it outside the mask. Only with Laplacian unwrapping of the masked phase and a
    /// weighted-average combination.
    pub laplacian_keep_outside: bool,
    pub romeo: RomeoConfig,
    /// PRELUDE's settings, used with `unwrapping_algorithm = "prelude"`.
    #[serde(default)]
    pub prelude: PreludeConfig,
    /// MEDI's complex fit (`b0_estimation = "complex-fit"`).
    #[serde(default)]
    pub complex_fit: ComplexFitConfig,
    /// The linear fit's settings. `estimate_offset` is shared with the robust fit, where it means
    /// the same thing.
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
}

impl FieldMappingConfig {
    /// Why these settings cannot be run, if they cannot: QSMART's echo fit with a threshold
    /// qsm-core refuses (`QsmartFitThreshold::validate`). The echo fit takes any unwrapping, phase
    /// offset removal and bipolar correction; QSMART's own choices are the rules' to suggest.
    pub fn validate(&self) -> Result<(), String> {
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
/// `estimate_offset` is `linear_fit`'s.
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
            laplacian_unmasked: qsm_core::pipeline::FieldMappingConfig::default().laplacian_unmasked,
            laplacian_keep_outside: qsm_core::pipeline::FieldMappingConfig::default().laplacian_keep_outside,
            romeo: RomeoConfig::default(),
            prelude: PreludeConfig::default(),
            complex_fit: ComplexFitConfig::default(),
            linear_fit: LinearFitConfig::default(),
            robust_fit: RobustFitConfig::default(),
            qsmart_fit_threshold_mode: QsmartFitThresholdMode::default(),
            qsmart_fit_percentile: default_qsmart_fit_percentile(),
            qsmart_fit_threshold: default_qsmart_fit_threshold(),
        }
    }
}
