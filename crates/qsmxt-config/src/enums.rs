use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum QsmAlgorithm {
    Rts, Tv, Tkd, Tsvd, Tgv, Tikhonov, Nltv, Medi, Tfi, Ilsqr, Qsmart,
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
            Self::Medi => "medi", Self::Tfi => "tfi", Self::Ilsqr => "ilsqr", Self::Qsmart => "qsmart",
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum QsmReference { Mean, None }
impl fmt::Display for QsmReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Mean => "mean", Self::None => "none" })
    }
}
