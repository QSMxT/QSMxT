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
impl fmt::Display for UnwrappingAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Romeo => "romeo", Self::Laplacian => "laplacian" })
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
pub enum B0Estimation { WeightedAvg, LinearFit }
impl fmt::Display for B0Estimation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::WeightedAvg => "weighted-avg", Self::LinearFit => "linear-fit" })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum B0WeightType {
    #[serde(rename = "phase-snr")] PhaseSNR,
    PhaseVar, Average,
    #[serde(rename = "tes")] TEs,
    Mag,
}
impl fmt::Display for B0WeightType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::PhaseSNR => "phase-snr", Self::PhaseVar => "phase-var",
            Self::Average => "average", Self::TEs => "tes", Self::Mag => "mag",
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
pub fn parse_synthseg_version(s: &str) -> Option<SynthSegVersion> {
    match s.trim() {
        "v1" => Some(SynthSegVersion::V1),
        "v2" => Some(SynthSegVersion::V2),
        _ => None,
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
