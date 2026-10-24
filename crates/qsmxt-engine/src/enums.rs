use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum QsmAlgorithm {
    Rts, Tv, Tkd, Tsvd, Tgv, Tikhonov, Nltv, Medi, Tfi, Ilsqr, Lsqr, Heidi, Qsmart,
    Ndi, Fansi,
    #[serde(rename = "fansi-tgv")] FansiTgv,
    L1qsm, Whqsm, Hdqsm,
    #[serde(rename = "amp-pe")] AmpPe,
    // Deep-learning dipole inversions (require qsm-core's `onnx` feature + downloadable weights).
    Xqsm, Qsmnet,
    #[serde(rename = "qsmnet-plus")] QsmnetPlus,
    Autoqsm, Qsmgan, Ir2qsm, Lpcnn,
    #[serde(rename = "modl-qsm")] ModlQsm,
    Nextqsm,
    // End-to-end DL reconstructions from wrapped phase (like TGV: no separate BFR/unwrap).
    Iqsm,
    #[serde(rename = "iqsm-plus")] IqsmPlus,
}
impl QsmAlgorithm {
    /// Every inversion, in the order the CLI lists them (`qsmxt`'s CLI reference test holds this
    /// to `--qsm-algorithm`'s values, so a new variant cannot be left out).
    pub const ALL: [QsmAlgorithm; 31] = [
        Self::Rts, Self::Tv, Self::Tkd, Self::Tsvd, Self::Tgv, Self::Tikhonov, Self::Nltv,
        Self::Medi, Self::Tfi, Self::Ilsqr, Self::Lsqr, Self::Heidi, Self::Qsmart, Self::Ndi,
        Self::Fansi, Self::FansiTgv, Self::L1qsm, Self::Whqsm, Self::Hdqsm, Self::AmpPe,
        Self::Xqsm, Self::Qsmnet, Self::QsmnetPlus, Self::Autoqsm, Self::Qsmgan, Self::Ir2qsm,
        Self::Lpcnn, Self::ModlQsm, Self::Nextqsm, Self::Iqsm, Self::IqsmPlus,
    ];

    /// A network that runs on overlapping tiles when `inversion.tile_size` is set. AutoQSM and
    /// QSMGAN run on fixed patches of their own and take no tiling.
    pub fn tileable(self) -> bool {
        matches!(
            self,
            Self::Xqsm | Self::Qsmnet | Self::QsmnetPlus | Self::Ir2qsm | Self::Lpcnn | Self::ModlQsm
                | Self::Nextqsm | Self::Iqsm | Self::IqsmPlus
        )
    }
}
impl fmt::Display for QsmAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Rts => "rts", Self::Tv => "tv", Self::Tkd => "tkd", Self::Tsvd => "tsvd",
            Self::Tgv => "tgv", Self::Tikhonov => "tikhonov", Self::Nltv => "nltv",
            Self::Medi => "medi", Self::Tfi => "tfi", Self::Ilsqr => "ilsqr",
            Self::Lsqr => "lsqr", Self::Heidi => "heidi", Self::Qsmart => "qsmart",
            Self::Ndi => "ndi", Self::Fansi => "fansi", Self::FansiTgv => "fansi-tgv",
            Self::L1qsm => "l1qsm", Self::Whqsm => "whqsm", Self::Hdqsm => "hdqsm",
            Self::AmpPe => "amp-pe",
            Self::Xqsm => "xqsm", Self::Qsmnet => "qsmnet",
            Self::QsmnetPlus => "qsmnet-plus", Self::Autoqsm => "autoqsm",
            Self::Qsmgan => "qsmgan", Self::Ir2qsm => "ir2qsm", Self::Lpcnn => "lpcnn",
            Self::ModlQsm => "modl-qsm", Self::Nextqsm => "nextqsm",
            Self::Iqsm => "iqsm", Self::IqsmPlus => "iqsm-plus",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SeparationAlgorithm {
    #[serde(rename = "r2star-qsm")] R2starQsm,
    #[serde(rename = "decompose")] Decompose,
    #[serde(rename = "chi-sep-ilsqr")] ChiSepIlsqr,
    #[serde(rename = "chi-sep-medi")] ChiSepMedi,
    #[serde(rename = "wavesep")] WaveSep,
    #[serde(rename = "hc-chisep")] HcChisep,
    // Deep-learning source separation (qsm-core `onnx` feature + downloadable weights).
    #[serde(rename = "susep-net")] SusepNet,
    #[serde(rename = "chi-sepnet")] ChiSepNet,
}
impl fmt::Display for SeparationAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::R2starQsm => "r2star-qsm", Self::Decompose => "decompose",
            Self::ChiSepIlsqr => "chi-sep-ilsqr", Self::ChiSepMedi => "chi-sep-medi",
            Self::WaveSep => "wavesep", Self::HcChisep => "hc-chisep",
            Self::SusepNet => "susep-net", Self::ChiSepNet => "chi-sepnet",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UnwrappingAlgorithm { Romeo, Laplacian }

/// Poisson solver behind Laplacian unwrapping (QSM.jl's `solver` names).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LaplacianSolver {
    /// Unweighted least squares with a Neumann-boundary DCT solve (Ghiglia & Romero, 1994),
    /// as QSM.jl's `:dct`.
    #[default]
    Dct,
    /// Schofield & Zhu (2003): sin/cos Laplacian with an FFT Poisson solve on the zero-padded
    /// volume (`field_mapping.laplacian_fft_pad`), reproducing STI Suite 3.0's `MRPhaseUnwrap`.
    #[serde(alias = "sti")]
    Fft,
    /// Schofield & Zhu sin/cos Laplacian with an FFT Poisson solve on the volume mirrored along
    /// each axis (qsm-core's `LaplacianSolver::Mirror`): QSMART's unwrapping, and its default
    /// when QSMART is the inversion.
    Mirror,
}
impl fmt::Display for LaplacianSolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Dct => "dct", Self::Fft => "fft", Self::Mirror => "mirror" })
    }
}
impl fmt::Display for UnwrappingAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Romeo => "romeo", Self::Laplacian => "laplacian" })
    }
}

/// How the magnitude is corrected for inhomogeneity (`masking.inhomogeneity_method`), when
/// `masking.inhomogeneity_correction` is on.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum InhomogeneityMethod {
    /// MriResearchTools' `makehomogeneous` (Eckstein et al., 2019), qsm-core's `makehomogeneous`
    /// with `homogeneity.*`.
    #[default]
    Makehomogeneous,
    /// N4 (Tustison et al., 2010), qsm-core's `n4_bias_correction` with `n4.*`.
    N4,
}
impl fmt::Display for InhomogeneityMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Makehomogeneous => "makehomogeneous", Self::N4 => "n4" })
    }
}

/// Grid V-SHARP estimates the background field on (qsm-core `VsharpGrid`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VsharpGrid {
    /// The acquired voxel grid (default).
    #[default]
    Native,
    /// An isotropic grid at the smallest acquired voxel dimension (e.g. 0.8 mm for
    /// 0.8 × 0.8 × 3 mm), so no axis is coarsened; isotropic data stays on the acquired grid.
    Finest,
    /// An isotropic grid of `bg_removal.vsharp.grid_spacing` mm, anchored at the mask's
    /// bounding box; the background is interpolated back and the local field keeps native
    /// resolution.
    Isotropic,
}
impl fmt::Display for VsharpGrid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Native => "native", Self::Finest => "finest", Self::Isotropic => "isotropic" })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BfAlgorithm {
    Vsharp, Pdf, Lbv, Ismv, Sharp, Resharp, Harperella, Iharperella,
    // Deep-learning background removal (qsm-core `onnx` feature + downloadable weights).
    Bfrnet,
    // iQFM: joint DL unwrap + background removal from wrapped phase → local field.
    // A "field preparation" choice occupying the BG-removal slot (input is phase, handled
    // specially by the runner), not a total-field→local BFR.
    Iqfm,
}
impl BfAlgorithm {
    /// A network that runs on overlapping tiles when `inversion.tile_size` is set.
    pub fn tileable(self) -> bool {
        matches!(self, Self::Bfrnet | Self::Iqfm)
    }
}
impl fmt::Display for BfAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Vsharp => "vsharp", Self::Pdf => "pdf", Self::Lbv => "lbv",
            Self::Ismv => "ismv", Self::Sharp => "sharp", Self::Resharp => "resharp",
            Self::Harperella => "harperella", Self::Iharperella => "iharperella",
            Self::Bfrnet => "bfrnet", Self::Iqfm => "iqfm",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum B0Estimation {
    WeightedAvg, LinearFit, RobustFit,
    /// QSMART's echo fit (qsm-core's `B0EstimationMethod::QsmartEchofit`): each echo
    /// Laplacian-unwrapped over the whole volume, then a magnitude-weighted fit of phase against
    /// TE through the origin, with QSMART's reliability map. Needs Laplacian unwrapping and no
    /// bipolar correction ([`crate::config::FieldMappingConfig::validate`]); the default when
    /// QSMART is the inversion.
    QsmartEchofit,
}
impl fmt::Display for B0Estimation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::WeightedAvg => "weighted-avg", Self::LinearFit => "linear-fit", Self::RobustFit => "robust-fit",
            Self::QsmartEchofit => "qsmart-echofit",
        })
    }
}

/// How QSMART's echo fit sets the residual threshold for its reliability map
/// (`field_mapping.qsmart_fit_threshold_mode`; qsm-core's `QsmartFitThreshold`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QsmartFitThresholdMode {
    /// A percentile of the blurred fit residual over the volume (`field_mapping.qsmart_fit_percentile`).
    Percentile,
    /// A fixed residual (`field_mapping.qsmart_fit_threshold`).
    Absolute,
}
impl Default for QsmartFitThresholdMode {
    fn default() -> Self {
        match qsm_core::utils::QsmartFitThreshold::default() {
            qsm_core::utils::QsmartFitThreshold::Percentile(_) => Self::Percentile,
            qsm_core::utils::QsmartFitThreshold::Absolute(_) => Self::Absolute,
        }
    }
}
impl fmt::Display for QsmartFitThresholdMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Percentile => "percentile", Self::Absolute => "absolute" })
    }
}

/// The magnitude QSMART detects vessels on (`inversion.qsmart.vessel_magnitude`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum QsmartVesselMagnitude {
    /// The mean over echoes, N4 bias-corrected (qsm-core's `qsmart_vessel_magnitude`), as
    /// QSMART's authors prepare it.
    #[default]
    MeanN4,
    /// The run's combined magnitude: the root sum of squares over echoes, inhomogeneity-corrected
    /// when `masking.inhomogeneity_correction` is on.
    Combined,
}
impl fmt::Display for QsmartVesselMagnitude {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::MeanN4 => "mean-n4", Self::Combined => "combined" })
    }
}

/// The units QSMART's vessel sizes (bottom-hat sphere radius, Frangi scales) are given in
/// (`inversion.qsmart.vessel_size_units`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VesselSizeUnits {
    /// Voxels, as QSMART specifies them.
    #[default]
    Voxels,
    /// Millimetres, converted to voxels with the mean voxel size.
    Mm,
}
impl fmt::Display for VesselSizeUnits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Voxels => "voxels", Self::Mm => "mm" })
    }
}

/// Loss of the motion-robust echo fit (`b0_estimation = "robust-fit"`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RobustLoss {
    /// Rejects residuals past the tuning radius (weight 0). Conventional tuning 4.685.
    #[default]
    Tukey,
    /// Caps outliers' influence but never rejects. Conventional tuning 1.345.
    Huber,
}
impl fmt::Display for RobustLoss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Tukey => "tukey", Self::Huber => "huber" })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum B0WeightType {
    #[serde(rename = "phase-snr")] PhaseSNR,
    PhaseVar, Average,
    #[serde(rename = "tes")] TEs,
    Mag,
    /// Fixed per-echo weights `TE·exp(−TE/T2*)` on the phase, from the echo times and an
    /// *assumed* T2* (`field_mapping.b0_weight_assumed_t2star_ms`): the same weights in every
    /// voxel, with no T2* map and no magnitude. UK Biobank's echo combination (Wang et al. 2022,
    /// Nat. Neurosci. 25:818). `phase-snr` is generally preferable when magnitude is available.
    AssumedDecay,
}
impl fmt::Display for B0WeightType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::PhaseSNR => "phase-snr", Self::PhaseVar => "phase-var",
            Self::Average => "average", Self::TEs => "tes", Self::Mag => "mag",
            Self::AssumedDecay => "assumed-decay",
        })
    }
}

/// What the susceptibility map's zero is pinned to.
///
/// `Region` names a parcellation structure rather than carrying it, so this stays `Copy` and the
/// serde derives stay simple; the structure itself is [`QsmConfig::reference_region`]. The two
/// always travel together — see [`crate::config::enforce_reference_dependencies`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum QsmReference { Mean, None, Region }
impl fmt::Display for QsmReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Mean => "mean", Self::None => "none", Self::Region => "region",
        })
    }
}

/// SynthSeg weights generation. Mirrors `qsm_core::segment::SynthSegVersion`; it selects the label
/// table, so it has to match the weights actually being run.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SynthSegVersion {
    #[default]
    V1,
    V2,
}
impl fmt::Display for SynthSegVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::V1 => "v1", Self::V2 => "v2" })
    }
}
impl SynthSegVersion {
    /// The QSM.rs model-registry id of this version's weights: the version picks the weight file
    /// as well as the label table it is read through.
    pub fn weights_id(self) -> &'static str {
        match self { Self::V1 => "synthseg", Self::V2 => "synthseg-2.0" }
    }

    pub fn to_core(self) -> qsm_core::segment::SynthSegVersion {
        match self {
            Self::V1 => qsm_core::segment::SynthSegVersion::V1,
            Self::V2 => qsm_core::segment::SynthSegVersion::V2,
        }
    }
}
pub fn parse_synthseg_version(s: &str) -> Option<SynthSegVersion> {
    match s.trim() {
        "v1" => Some(SynthSegVersion::V1),
        "v2" => Some(SynthSegVersion::V2),
        _ => None,
    }
}

/// SWI phase-mask scaling (qsm-core's [`qsm_core::swi::PhaseScaling`]).
///
/// Typed end to end — CLI, config file, TUI — and mapped to qsm-core in one place, so a value
/// QSMxT does not recognise cannot parse rather than quietly running `tanh` (#266).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SwiScaling {
    #[default]
    Tanh,
    /// Older config files wrote it with an underscore.
    #[serde(alias = "negative_tanh")]
    NegativeTanh,
    Positive,
    Negative,
    Triangular,
}
impl SwiScaling {
    /// Every variant, in the order the CLI help and the TUI list them.
    pub const ALL: [SwiScaling; 5] =
        [Self::Tanh, Self::NegativeTanh, Self::Positive, Self::Negative, Self::Triangular];

    /// The name the CLI, the config file and the TUI use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tanh => "tanh", Self::NegativeTanh => "negative-tanh", Self::Positive => "positive",
            Self::Negative => "negative", Self::Triangular => "triangular",
        }
    }
}
impl fmt::Display for SwiScaling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<SwiScaling> for qsm_core::swi::PhaseScaling {
    fn from(s: SwiScaling) -> Self {
        use qsm_core::swi::PhaseScaling as P;
        match s {
            SwiScaling::Tanh => P::Tanh,
            SwiScaling::NegativeTanh => P::NegativeTanh,
            SwiScaling::Positive => P::Positive,
            SwiScaling::Negative => P::Negative,
            SwiScaling::Triangular => P::Triangular,
        }
    }
}
impl From<qsm_core::swi::PhaseScaling> for SwiScaling {
    fn from(p: qsm_core::swi::PhaseScaling) -> Self {
        use qsm_core::swi::PhaseScaling as P;
        match p {
            P::Tanh => Self::Tanh,
            P::NegativeTanh => Self::NegativeTanh,
            P::Positive => Self::Positive,
            P::Negative => Self::Negative,
            P::Triangular => Self::Triangular,
        }
    }
}

/// How CLEAR-SWI unwraps the phase before combining the echoes.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SwiUnwrap {
    /// Each echo with the Laplacian method, high-pass filtered, then combined.
    #[default]
    Laplacian,
    /// As `Laplacian`, slice by slice (2D multi-slice acquisitions).
    LaplacianSlice,
    /// All echoes together with ROMEO, combined, then high-pass filtered.
    Romeo,
}
impl SwiUnwrap {
    /// Every variant, in the order the CLI help and the TUI list them.
    pub const ALL: [SwiUnwrap; 3] = [Self::Laplacian, Self::LaplacianSlice, Self::Romeo];

    /// The name the CLI, the config file and the TUI use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Laplacian => "laplacian", Self::LaplacianSlice => "laplacian-slice", Self::Romeo => "romeo",
        }
    }
}
impl fmt::Display for SwiUnwrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<SwiUnwrap> for qsm_core::swi::SwiUnwrap {
    fn from(s: SwiUnwrap) -> Self {
        use qsm_core::swi::SwiUnwrap as U;
        match s {
            SwiUnwrap::Laplacian => U::Laplacian,
            SwiUnwrap::LaplacianSlice => U::LaplacianSlice,
            SwiUnwrap::Romeo => U::Romeo,
        }
    }
}
impl From<qsm_core::swi::SwiUnwrap> for SwiUnwrap {
    fn from(u: qsm_core::swi::SwiUnwrap) -> Self {
        use qsm_core::swi::SwiUnwrap as U;
        match u {
            U::Laplacian => Self::Laplacian,
            U::LaplacianSlice => Self::LaplacianSlice,
            U::Romeo => Self::Romeo,
        }
    }
}

/// How R2′ is obtained when no custom map is supplied.
///
/// R2′ = R2* − R2 is a *measurement*, and it needs a spin-echo acquisition to measure R2 from.
/// R2PRIMEnet predicts it from the GRE-derived R2* instead, which is an *estimate* — so which of
/// these ran changes how a result should be reported, and the methods text says which it was.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum R2PrimeStrategy {
    /// Measure it from a MESE acquisition when one is present; fall back to R2PRIMEnet when it is
    /// not, so a gradient-echo-only dataset still gets the R2′ that χ-separation needs.
    #[default]
    Auto,
    /// Measure it, or produce nothing. The pre-R2PRIMEnet behaviour, and the choice for anyone who
    /// would rather have no R2′ than an estimated one.
    Mese,
    /// Predict it from R2*, even when a MESE acquisition is available.
    R2primenet,
}
impl fmt::Display for R2PrimeStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Auto => "auto", Self::Mese => "mese", Self::R2primenet => "r2primenet",
        })
    }
}
pub fn parse_r2prime_strategy(s: &str) -> Option<R2PrimeStrategy> {
    match s.trim() {
        "auto" => Some(R2PrimeStrategy::Auto),
        "mese" => Some(R2PrimeStrategy::Mese),
        "r2primenet" => Some(R2PrimeStrategy::R2primenet),
        _ => None,
    }
}
