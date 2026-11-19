use serde::{Deserialize, Serialize};
use std::fmt;

/// An engine enum whose every variant has one name, written once: `Variant => "name"`. That name
/// is the variant's serde name (`#[serde(rename)]`), what `as_str` returns and what `Display`
/// writes, so the config file, the CLI and the TUI cannot disagree. `ALL` lists the variants in
/// declaration order, the order the CLI help and the TUI list them. Doc comments and other
/// attributes (`#[default]`, `#[serde(alias)]`) pass through.
///
/// Enums whose name comes from elsewhere (qsm-core's `id()`), or that parse more than a name
/// (`EchoTimeChoice`, `ChiBoundChoice`), are written out by hand.
macro_rules! mirror_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* #[serde(rename = $text)] $variant ),+
        }
        impl $name {
            /// Every variant, in the order the CLI help and the TUI list them.
            pub const ALL: [$name; [$(stringify!($variant)),+].len()] = [$(Self::$variant),+];

            /// The name the CLI, the config file and the TUI use: the serde name.
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

mirror_enum! {
    /// `ALL` is every inversion in the order the CLI lists them (`qsmxt`'s CLI reference test holds
    /// it to `--qsm-algorithm`'s values, so a new variant cannot be left out).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum QsmAlgorithm {
        Rts => "rts", Tv => "tv", Tkd => "tkd", Tsvd => "tsvd", Tgv => "tgv", Tikhonov => "tikhonov",
        Nltv => "nltv", Medi => "medi", Tfi => "tfi", Ilsqr => "ilsqr", Star => "star", Lsqr => "lsqr",
        Qsmart => "qsmart",
        Ndi => "ndi", Fansi => "fansi",
        FansiTgv => "fansi-tgv",
        L1qsm => "l1qsm", Whqsm => "whqsm", Hdqsm => "hdqsm",
        AmpPe => "amp-pe",
        // Deep-learning dipole inversions (require qsm-core's `onnx` feature + downloadable weights).
        Xqsm => "xqsm", Qsmnet => "qsmnet",
        QsmnetPlus => "qsmnet-plus",
        Qsmnetme => "qsmnetme",
        Autoqsm => "autoqsm", Qsmgan => "qsmgan", Ir2qsm => "ir2qsm", Lpcnn => "lpcnn",
        ModlQsm => "modl-qsm",
        Nextqsm => "nextqsm",
        // End-to-end DL reconstructions from wrapped phase (like TGV: no separate BFR/unwrap).
        Iqsm => "iqsm",
        IqsmPlus => "iqsm-plus",
    }
}
impl QsmAlgorithm {
    /// A network that runs on overlapping tiles when `inversion.tile_size` is set. AutoQSM and
    /// QSMGAN run on fixed patches of their own and take no tiling.
    pub fn tileable(self) -> bool {
        matches!(
            self,
            Self::Xqsm | Self::Qsmnet | Self::QsmnetPlus | Self::Qsmnetme | Self::Ir2qsm | Self::Lpcnn | Self::ModlQsm
                | Self::Nextqsm | Self::Iqsm | Self::IqsmPlus
        )
    }
}

mirror_enum! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum SeparationAlgorithm {
        R2starQsm => "r2star-qsm",
        Decompose => "decompose",
        ChiSepIlsqr => "chi-sep-ilsqr",
        ChiSepMedi => "chi-sep-medi",
        WaveSep => "wavesep",
        HcChisep => "hc-chisep",
        // Deep-learning source separation (qsm-core `onnx` feature + downloadable weights).
        SusepNet => "susep-net",
        ChiSepNet => "chi-sepnet",
    }
}

impl SeparationAlgorithm {
    /// Whether the method reads an R2′ map (χ = f(QSM, R2′)): every one but R2*-QSM, which uses
    /// R2* directly, and DECOMPOSE-QSM, which fits the multi-echo magnitude itself. SUSEP-Net
    /// consumes [QSM, R2′, local field] → [χ+, χ−], so it reads R2′ too.
    pub fn reads_r2prime(self) -> bool {
        !matches!(self, SeparationAlgorithm::R2starQsm | SeparationAlgorithm::Decompose)
    }
}

mirror_enum! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum UnwrappingAlgorithm {
        Romeo => "romeo",
        Laplacian => "laplacian",
        Prelude => "prelude",
        /// MEDI's region growing (qsm-core's `UnwrappingAlgorithm::MediRegionGrowing`).
        MediRegionGrowing => "medi-region-growing",
    }
}

mirror_enum! {
    /// How PRELUDE labels its initial regions (qsm-core's `PreludeLabelling`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum PreludeLabelling {
        /// FSL prelude's default: slice by slice when the volume is over 64 voxels along x or y,
        /// otherwise over the whole volume.
        #[default]
        Auto => "auto",
        /// Slice by slice (FSL `--labelslices`).
        Slices => "slices",
        /// Over the whole volume (FSL `-f`).
        Volume => "volume",
    }
}
impl From<PreludeLabelling> for qsm_core::unwrap::PreludeLabelling {
    fn from(l: PreludeLabelling) -> Self {
        match l {
            PreludeLabelling::Auto => Self::Auto,
            PreludeLabelling::Slices => Self::Slices,
            PreludeLabelling::Volume => Self::Volume,
        }
    }
}

mirror_enum! {
    /// Poisson solver behind Laplacian unwrapping (QSM.jl's `solver` names).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
    pub enum LaplacianSolver {
        /// Unweighted least squares with a Neumann-boundary DCT solve (Ghiglia & Romero, 1994),
        /// as QSM.jl's `:dct`.
        #[default]
        Dct => "dct",
        /// Schofield & Zhu (2003): sin/cos Laplacian with an FFT Poisson solve on the zero-padded
        /// volume (`field_mapping.laplacian_fft_pad`), reproducing STI Suite 3.0's `MRPhaseUnwrap`.
        #[serde(alias = "sti")]
        Fft => "fft",
        /// Schofield & Zhu sin/cos Laplacian with an FFT Poisson solve on the volume mirrored along
        /// each axis (qsm-core's `LaplacianSolver::Mirror`): QSMART's unwrapping, which the rule
        /// `qsmart.laplacian-solver` suggests for a QSMART run.
        Mirror => "mirror",
        /// MGPCG (QSM.jl; qsm-core's `LaplacianSolver::Mgpcg`): Laplacian unwrapping with LBV
        /// background removal in one step (Dirichlet boundary on the brain mask, solved by
        /// multigrid-preconditioned CG). QSM.jl follows it with V-SHARP.
        Mgpcg => "mgpcg",
    }
}

mirror_enum! {
    /// How the magnitude is corrected for inhomogeneity (`masking.inhomogeneity_method`), when
    /// `masking.inhomogeneity_correction` is on.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum InhomogeneityMethod {
        /// MriResearchTools' `makehomogeneous` (Eckstein et al., 2019), qsm-core's `makehomogeneous`
        /// with `homogeneity.*`.
        #[default]
        Makehomogeneous => "makehomogeneous",
        /// N4 (Tustison et al., 2010), qsm-core's `n4_bias_correction` with `n4.*`.
        N4 => "n4",
    }
}

/// The image geometry N4 is given when it corrects the magnitude (`masking.n4_geometry`;
/// qsm-core `N4Geometry`). N4 fits on a shrunk image whose voxel offsets ITK derives from the
/// header, so the geometry changes the correction.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum N4Geometry {
    /// The scan's header, as ANTs reads the acquired NIfTI.
    #[default]
    #[serde(rename = "scanner")]
    Scanner,
    /// The axis-aligned header MATLAB's `make_nii(img, vox)` writes, as QSMART hands N4 the
    /// magnitude.
    #[serde(rename = "make-nii")]
    MakeNii,
}
impl N4Geometry {
    /// Every variant, in the order the CLI help and the TUI list them.
    pub const ALL: [N4Geometry; 2] = [Self::Scanner, Self::MakeNii];

    /// The name the CLI, the config file and the TUI use (qsm-core's `N4Geometry::id`).
    pub fn as_str(self) -> &'static str {
        qsm_core::pipeline::N4Geometry::from(self).id()
    }
}
impl fmt::Display for N4Geometry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<N4Geometry> for qsm_core::pipeline::N4Geometry {
    fn from(g: N4Geometry) -> Self {
        use qsm_core::pipeline::N4Geometry as G;
        match g { N4Geometry::Scanner => G::Scanner, N4Geometry::MakeNii => G::MakeNii }
    }
}
impl From<qsm_core::pipeline::N4Geometry> for N4Geometry {
    fn from(g: qsm_core::pipeline::N4Geometry) -> Self {
        use qsm_core::pipeline::N4Geometry as G;
        match g { G::Scanner => Self::Scanner, G::MakeNii => Self::MakeNii }
    }
}

/// How a two-pass run makes and merges its second pass (`masking.two_pass_mode`; qsm-core
/// `pipeline::TwoPassMode`, which documents both and the evidence behind them).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TwoPassMode {
    /// Holes only at strong sources, and the second pass used only near them (the default).
    #[default]
    Local,
    /// The published algorithm: every voxel below the reliable mask's threshold is a hole, and
    /// the second pass is taken wherever it has a value.
    Original,
}
impl TwoPassMode {
    /// Every mode, in the order the CLI help and the TUI list them.
    pub const ALL: [TwoPassMode; 2] = [Self::Local, Self::Original];

    /// The name the CLI, the config file and the TUI use (qsm-core's `TwoPassMode::id`).
    pub fn as_str(self) -> &'static str {
        qsm_core::pipeline::TwoPassMode::from(self).id()
    }
}
impl fmt::Display for TwoPassMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<TwoPassMode> for qsm_core::pipeline::TwoPassMode {
    fn from(m: TwoPassMode) -> Self {
        use qsm_core::pipeline::TwoPassMode as M;
        match m { TwoPassMode::Local => M::Local, TwoPassMode::Original => M::Original }
    }
}

mirror_enum! {
    /// Grid V-SHARP estimates the background field on (qsm-core `VsharpGrid`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
    pub enum VsharpGrid {
        /// The acquired voxel grid.
        Native => "native",
        /// An isotropic grid at the smallest acquired voxel dimension (e.g. 0.8 mm for
        /// 0.8 × 0.8 × 3 mm), so no axis is coarsened; isotropic data stays on the acquired grid.
        Finest => "finest",
        /// An isotropic grid of `bg_removal.vsharp.grid_spacing` mm (default, 1 mm), anchored at the
        /// mask's bounding box; the background is interpolated back and the local field keeps native
        /// resolution.
        #[default]
        Isotropic => "isotropic",
    }
}

mirror_enum! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum BfAlgorithm {
        Vsharp => "vsharp", Pdf => "pdf", Lbv => "lbv", Ismv => "ismv", Sharp => "sharp", Resharp => "resharp",
        Harperella => "harperella", Iharperella => "iharperella",
        // Deep-learning background removal (qsm-core `onnx` feature + downloadable weights).
        Bfrnet => "bfrnet",
        // iQFM: joint DL unwrap + background removal from wrapped phase → local field.
        // A "field preparation" choice occupying the BG-removal slot (input is phase, handled
        // specially by the runner), not a total-field→local BFR.
        Iqfm => "iqfm",
    }
}
impl BfAlgorithm {
    /// A network that runs on overlapping tiles when `inversion.tile_size` is set.
    pub fn tileable(self) -> bool {
        matches!(self, Self::Bfrnet | Self::Iqfm)
    }
}

mirror_enum! {
    /// The kernel PDF fits the background with (qsm-core `DipoleKernelSpace`; `PDF.m`'s `space`).
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum PdfSpace {
        /// The FFT of the dipole field sampled on the voxel grid (`'imagespace'`).
        #[default]
        Image => "image",
        /// The analytic k-space kernel (`'kspace'`).
        Kspace => "kspace",
    }
}
impl From<PdfSpace> for qsm_core::bgremove::DipoleKernelSpace {
    fn from(s: PdfSpace) -> Self {
        use qsm_core::bgremove::DipoleKernelSpace as K;
        match s { PdfSpace::Image => K::ImageSpace, PdfSpace::Kspace => K::KSpace }
    }
}
impl From<qsm_core::bgremove::DipoleKernelSpace> for PdfSpace {
    fn from(k: qsm_core::bgremove::DipoleKernelSpace) -> Self {
        use qsm_core::bgremove::DipoleKernelSpace as K;
        match k { K::ImageSpace => Self::Image, K::KSpace => Self::Kspace }
    }
}

mirror_enum! {
    /// Which implementation of iSMV runs (qsm-core `IsmvVariant`).
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum IsmvVariant {
        /// The authors' paper and code (Wen et al. 2014; the lab's `iSMV.m`).
        #[default]
        Wen2014 => "wen2014",
        /// QSM.jl's `ismv`.
        Qsmjl => "qsmjl",
    }
}
impl From<IsmvVariant> for qsm_core::bgremove::IsmvVariant {
    fn from(v: IsmvVariant) -> Self {
        use qsm_core::bgremove::IsmvVariant as V;
        match v { IsmvVariant::Wen2014 => V::Wen2014, IsmvVariant::Qsmjl => V::QsmJl }
    }
}
impl From<qsm_core::bgremove::IsmvVariant> for IsmvVariant {
    fn from(v: qsm_core::bgremove::IsmvVariant) -> Self {
        use qsm_core::bgremove::IsmvVariant as V;
        match v { V::Wen2014 => Self::Wen2014, V::QsmJl => Self::Qsmjl }
    }
}

/// Which Laplacian values in HARPERELLA's / iHARPERELLA's boundary rim are set to zero (qsm-core
/// `HarperellaRim`). HARPERELLA defaults to the whole rim, iHARPERELLA to STI's rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HarperellaRim {
    /// Every rim value.
    #[serde(rename = "whole")]
    Whole,
    /// The 20 % of rim values largest in magnitude (STI Suite's `iHARPERELLA`).
    #[serde(rename = "sti")]
    Sti,
}
impl HarperellaRim {
    /// Every variant, in the order the CLI help and the TUI list them.
    pub const ALL: [HarperellaRim; 2] = [Self::Whole, Self::Sti];

    /// The name the CLI, the config file and the TUI use (qsm-core's `HarperellaRim::id`).
    pub fn as_str(self) -> &'static str {
        qsm_core::bgremove::HarperellaRim::from(self).id()
    }
}
impl fmt::Display for HarperellaRim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<HarperellaRim> for qsm_core::bgremove::HarperellaRim {
    fn from(r: HarperellaRim) -> Self {
        use qsm_core::bgremove::HarperellaRim as R;
        match r { HarperellaRim::Whole => R::Whole, HarperellaRim::Sti => R::StiLargest }
    }
}
impl From<qsm_core::bgremove::HarperellaRim> for HarperellaRim {
    fn from(r: qsm_core::bgremove::HarperellaRim) -> Self {
        use qsm_core::bgremove::HarperellaRim as R;
        match r { R::Whole => Self::Whole, R::StiLargest => Self::Sti }
    }
}

/// What NDI divides the magnitude by to make its data weight (qsm-core `NdiWeightNorm`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum NdiWeightNorm {
    /// The magnitude's maximum over the whole image (FANSI `script_invivo.m`).
    #[default]
    #[serde(rename = "global-max")]
    GlobalMax,
    /// The 99.9th percentile of the magnitude inside the mask.
    #[serde(rename = "mask-p999")]
    MaskP999,
}
impl NdiWeightNorm {
    /// Every variant, in the order the CLI help and the TUI list them.
    pub const ALL: [NdiWeightNorm; 2] = [Self::GlobalMax, Self::MaskP999];

    /// The name the CLI, the config file and the TUI use (qsm-core's `NdiWeightNorm::id`).
    pub fn as_str(self) -> &'static str {
        qsm_core::inversion::NdiWeightNorm::from(self).id()
    }
}
impl fmt::Display for NdiWeightNorm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<NdiWeightNorm> for qsm_core::inversion::NdiWeightNorm {
    fn from(n: NdiWeightNorm) -> Self {
        use qsm_core::inversion::NdiWeightNorm as N;
        match n { NdiWeightNorm::GlobalMax => N::GlobalMax, NdiWeightNorm::MaskP999 => N::MaskP999 }
    }
}
impl From<qsm_core::inversion::NdiWeightNorm> for NdiWeightNorm {
    fn from(n: qsm_core::inversion::NdiWeightNorm) -> Self {
        use qsm_core::inversion::NdiWeightNorm as N;
        match n { N::GlobalMax => Self::GlobalMax, N::MaskP999 => Self::MaskP999 }
    }
}

/// Which way round BET sees the image's left-right axis (qsm-core `BetOrientation`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum BetOrientation {
    /// As FSL's `bet` reads a NIfTI file: x reversed when the header is neurological.
    #[default]
    #[serde(rename = "fsl")]
    Fsl,
    /// The voxels in the order they are stored, as MEDI's `BET.m` hands its bet2 mex the array.
    #[serde(rename = "as-stored")]
    AsStored,
}
impl BetOrientation {
    /// Every variant, in the order the CLI help and the TUI list them.
    pub const ALL: [BetOrientation; 2] = [Self::Fsl, Self::AsStored];

    /// The name the CLI, the config file and the TUI use (qsm-core's `BetOrientation::id`).
    pub fn as_str(self) -> &'static str {
        qsm_core::bet::BetOrientation::from(self).id()
    }
}
impl fmt::Display for BetOrientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<BetOrientation> for qsm_core::bet::BetOrientation {
    fn from(o: BetOrientation) -> Self {
        use qsm_core::bet::BetOrientation as O;
        match o { BetOrientation::Fsl => O::Fsl, BetOrientation::AsStored => O::AsStored }
    }
}
impl From<qsm_core::bet::BetOrientation> for BetOrientation {
    fn from(o: qsm_core::bet::BetOrientation) -> Self {
        use qsm_core::bet::BetOrientation as O;
        match o { O::Fsl => Self::Fsl, O::AsStored => Self::AsStored }
    }
}

mirror_enum! {
    /// WH-QSM's data weight (qsm-core `WhQsmWeight`).
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum WhqsmWeight {
        /// 1 over the whole volume, no magnitude (`WH_nlTV.m`'s own default).
        #[default]
        Flat => "flat",
        /// 1 inside the mask only, no magnitude.
        FlatInMask => "flat-in-mask",
        /// The magnitude in the mask over its maximum. Needs the magnitude.
        MaxNormalised => "max-normalised",
        /// `min(2·magnitude/max, 1)` in the mask, as NDI weights. Needs the magnitude.
        InVivo => "in-vivo",
    }
}
impl From<WhqsmWeight> for qsm_core::inversion::WhQsmWeight {
    fn from(w: WhqsmWeight) -> Self {
        use qsm_core::inversion::WhQsmWeight as W;
        match w {
            WhqsmWeight::Flat => W::Flat, WhqsmWeight::FlatInMask => W::FlatInMask,
            WhqsmWeight::MaxNormalised => W::MaxNormalised, WhqsmWeight::InVivo => W::InVivo,
        }
    }
}
impl From<qsm_core::inversion::WhQsmWeight> for WhqsmWeight {
    fn from(w: qsm_core::inversion::WhQsmWeight) -> Self {
        use qsm_core::inversion::WhQsmWeight as W;
        match w {
            W::Flat => Self::Flat, W::FlatInMask => Self::FlatInMask,
            W::MaxNormalised => Self::MaxNormalised, W::InVivo => Self::InVivo,
        }
    }
}

mirror_enum! {
    /// Arithmetic of iLSQR's two LSQR solves (qsm-core `IlsqrPrecision`).
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum IlsqrPrecision {
        /// Double precision throughout (the default).
        #[default]
        Double => "double",
        /// Single precision, as STI Suite computes its solves.
        Single => "single",
    }
}
impl From<IlsqrPrecision> for qsm_core::inversion::IlsqrPrecision {
    fn from(p: IlsqrPrecision) -> Self {
        use qsm_core::inversion::IlsqrPrecision as P;
        match p { IlsqrPrecision::Double => P::Double, IlsqrPrecision::Single => P::Single }
    }
}
impl From<qsm_core::inversion::IlsqrPrecision> for IlsqrPrecision {
    fn from(p: qsm_core::inversion::IlsqrPrecision) -> Self {
        use qsm_core::inversion::IlsqrPrecision as P;
        match p { P::Double => Self::Double, P::Single => Self::Single }
    }
}

mirror_enum! {
    /// Arithmetic of LBV's multigrid solver (qsm-core `LbvPrecision`).
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum LbvPrecision {
        /// Double precision throughout (the default).
        #[default]
        Double => "double",
        /// Single precision, as the original implementation computes.
        Single => "single",
    }
}
impl From<LbvPrecision> for qsm_core::bgremove::LbvPrecision {
    fn from(p: LbvPrecision) -> Self {
        use qsm_core::bgremove::LbvPrecision as P;
        match p { LbvPrecision::Double => P::Double, LbvPrecision::Single => P::Single }
    }
}
impl From<qsm_core::bgremove::LbvPrecision> for LbvPrecision {
    fn from(p: qsm_core::bgremove::LbvPrecision) -> Self {
        use qsm_core::bgremove::LbvPrecision as P;
        match p { P::Double => Self::Double, P::Single => Self::Single }
    }
}

impl EchoTimeChoice {
    /// Why the choice cannot be used whatever the scan, if it cannot: a given echo time that is not
    /// a positive time. Against a scan's echo times, qsm-core's `EchoTime::resolve` decides
    /// ([`crate::requirements`]).
    pub fn validate(&self) -> Result<(), String> {
        match *self {
            Self::Seconds(s) if !(s.is_finite() && s > 0.0) => Err(format!(
                "echo time: must be a finite time greater than 0 ms (got {} ms)", s * 1000.0)),
            _ => Ok(()),
        }
    }
}

mirror_enum! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum B0Estimation {
        WeightedAvg => "weighted-avg", LinearFit => "linear-fit", RobustFit => "robust-fit",
        /// QSMART's echo fit (qsm-core's `B0EstimationMethod::QsmartEchofit`): a magnitude-weighted
        /// fit of phase against TE through the origin, on the echoes as the configured unwrapping,
        /// phase offset removal and bipolar correction leave them, with QSMART's reliability map.
        /// QSMART's own route (Laplacian unwrapping of the raw phase with the mirror solver) is what
        /// the rules suggest for a QSMART run.
        QsmartEchofit => "qsmart-echofit",
        /// MEDI's field map (`Fit_ppm_complex`): a fit of the complex echoes, then unwrapping of the
        /// fitted phase per echo spacing (QSM.rs `B0EstimationMethod::ComplexFit`).
        ComplexFit => "complex-fit",
    }
}

mirror_enum! {
    /// How QSMART's echo fit sets the residual threshold for its reliability map
    /// (`field_mapping.qsmart_fit_threshold_mode`; qsm-core's `QsmartFitThreshold`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
    pub enum QsmartFitThresholdMode {
        /// A percentile of the blurred fit residual over the volume (`field_mapping.qsmart_fit_percentile`).
        Percentile => "percentile",
        /// A fixed residual (`field_mapping.qsmart_fit_threshold`).
        Absolute => "absolute",
    }
}
impl Default for QsmartFitThresholdMode {
    fn default() -> Self {
        match qsm_core::utils::QsmartFitThreshold::default() {
            qsm_core::utils::QsmartFitThreshold::Percentile(_) => Self::Percentile,
            qsm_core::utils::QsmartFitThreshold::Absolute(_) => Self::Absolute,
        }
    }
}

mirror_enum! {
    /// The dipole grid of QSMART's offset adjustment (`inversion.qsmart.offset_grid`; QSM.rs's
    /// `QsmartOffsetGrid`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum QsmartOffsetGrid {
        /// The FFT frequency grid (default).
        #[default]
        Fft => "fft",
        /// The grid QSMART's `adjust_offset.m` builds (half a step off the FFT grid on odd matrix
        /// sizes), as the QSMART pipeline runs it.
        QsmartReference => "qsmart-reference",
    }
}

mirror_enum! {
    /// DECOMPOSE's fit (`separation.decompose.fit`; QSM.rs's `DecomposeParams::fit_id`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum DecomposeFit {
        /// Tim Ho's script (lsqcurvefit for every stage, magnitudes scaled by their global maximum),
        /// which QSM-CI's reference container runs (default).
        #[default]
        ReferenceCode => "reference-code",
        /// Chen et al. 2021's fit: the amplitudes with C+ + C− + C0 = 1 and R2*0 by linear least
        /// squares, χ± by lsqnonlin, each voxel scaled to S(0) = 1 (as the DECOMPOSE pipeline runs it).
        Paper => "paper",
    }
}

mirror_enum! {
    /// The magnitude QSMART detects vessels on (`inversion.qsmart.vessel_magnitude`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum QsmartVesselMagnitude {
        /// The mean over echoes, N4 bias-corrected (qsm-core's `qsmart_vessel_magnitude`), as
        /// QSMART's authors prepare it.
        #[default]
        MeanN4 => "mean-n4",
        /// The run's combined magnitude: the root sum of squares over echoes, inhomogeneity-corrected
        /// when `masking.inhomogeneity_correction` is on.
        Combined => "combined",
    }
}

mirror_enum! {
    /// How QSMART's Frangi c is set (`inversion.qsmart.frangi_c_mode`; QSM.rs's `FrangiC`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum QsmartFrangiCMode {
        /// `frangi_c`, in the vessel magnitude's intensity units, as QSMART v1.0 (default, 500).
        #[default]
        Absolute => "absolute",
        /// `frangi_c_factor` × the median of the vessel magnitude inside the brain mask, so the vessel
        /// mask does not depend on how the scanner scales its magnitude.
        Relative => "relative",
    }
}

mirror_enum! {
    /// The units QSMART's vessel sizes (bottom-hat sphere radius, Frangi scales) are given in
    /// (`inversion.qsmart.vessel_size_units`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum VesselSizeUnits {
        /// Voxels, as QSMART specifies them.
        #[default]
        Voxels => "voxels",
        /// Millimetres, converted to voxels with the mean voxel size.
        Mm => "mm",
    }
}

mirror_enum! {
    /// Loss of the motion-robust echo fit (`b0_estimation = "robust-fit"`).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub enum RobustLoss {
        /// Rejects residuals past the tuning radius (weight 0). Conventional tuning 4.685.
        #[default]
        Tukey => "tukey",
        /// Caps outliers' influence but never rejects. Conventional tuning 1.345.
        Huber => "huber",
    }
}

mirror_enum! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum B0WeightType {
        PhaseSNR => "phase-snr",
        PhaseVar => "phase-var", Average => "average",
        TEs => "tes",
        Mag => "mag",
        /// Fixed per-echo weights `TE·exp(−TE/T2*)` on the phase, from the echo times and an
        /// *assumed* T2* (`field_mapping.b0_weight_assumed_t2star_ms`): the same weights in every
        /// voxel, with no T2* map and no magnitude. UK Biobank's echo combination (Wang et al. 2022,
        /// Nat. Neurosci. 25:818). `phase-snr` is generally preferable when magnitude is available.
        AssumedDecay => "assumed-decay",
    }
}

mirror_enum! {
    /// What the susceptibility map's zero is pinned to.
    ///
    /// `Region` names a parcellation structure rather than carrying it, so this stays `Copy` and the
    /// serde derives stay simple; the structure itself is [`QsmConfig::reference_region`]. The two
    /// always travel together — see [`crate::config::enforce_reference_dependencies`].
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum QsmReference { Mean => "mean", None => "none", Region => "region" }
}

mirror_enum! {
    /// SynthSeg weights generation. Mirrors `qsm_core::segment::SynthSegVersion`; it selects the label
    /// table, so it has to match the weights actually being run.
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum SynthSegVersion {
        #[default]
        V1 => "v1",
        V2 => "v2",
    }
}
impl SynthSegVersion {
    /// The QSM.rs model-registry id of this version's weights: the version picks the weight file
    /// as well as the label table it is read through.
    pub fn weights_id(self) -> &'static str {
        self.to_core().model_id()
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

mirror_enum! {
    /// SWI phase-mask scaling (qsm-core's [`qsm_core::swi::PhaseScaling`]).
    ///
    /// Typed end to end — CLI, config file, TUI — and mapped to qsm-core in one place, so a value
    /// QSMxT does not recognise cannot parse rather than quietly running `tanh` (#266).
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum SwiScaling {
        #[default]
        Tanh => "tanh",
        /// Older config files wrote it with an underscore.
        #[serde(alias = "negative_tanh")]
        NegativeTanh => "negative-tanh",
        Positive => "positive",
        Negative => "negative",
        Triangular => "triangular",
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

mirror_enum! {
    /// How CLEAR-SWI unwraps the phase before combining the echoes.
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum SwiUnwrap {
        /// Each echo with the Laplacian method, high-pass filtered, then combined.
        #[default]
        Laplacian => "laplacian",
        /// As `Laplacian`, slice by slice (2D multi-slice acquisitions).
        LaplacianSlice => "laplacian-slice",
        /// All echoes together with ROMEO, combined, then high-pass filtered.
        Romeo => "romeo",
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

mirror_enum! {
    /// How R2′ is obtained when no custom map is supplied.
    ///
    /// R2′ = R2* − R2 is a *measurement*, and it needs a spin-echo acquisition to measure R2 from.
    /// R2PRIMEnet predicts it from the GRE-derived R2* instead, which is an *estimate* — so which of
    /// these ran changes how a result should be reported, and the methods text says which it was.
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum R2PrimeStrategy {
        /// Measure it from a MESE acquisition when one is present; fall back to R2PRIMEnet when it is
        /// not, so a gradient-echo-only dataset still gets the R2′ that χ-separation needs.
        #[default]
        Auto => "auto",
        /// Measure it, or produce nothing. The pre-R2PRIMEnet behaviour, and the choice for anyone who
        /// would rather have no R2′ than an estimated one.
        Mese => "mese",
        /// Predict it from R2*, even when a MESE acquisition is available.
        R2primenet => "r2primenet",
    }
}
mirror_enum! {
    /// Which route χ-sepnet's inputs take (qsm-core `ChiSepNetRoute`).
    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub enum ChiSepNetRoute {
        /// The authors' route: the inputs in the networks' training frame (B0 along z, 1 mm), the QSM
        /// channel from QSMnetME, R2′ from a measured map or from R2* by R2PRIMEnet, mapped back.
        #[default]
        Toolbox => "toolbox",
        /// The network alone on the acquired grid, given the run's QSM and R2′, as a sliding window.
        Native => "native",
    }
}
/// The R2* χ-separation MEDI's CSF mask is extracted from (`separation.chi_sep_medi.csf_r2star`;
/// qsm-core `ChiSepCsfR2star`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChiSepCsfR2star {
    /// MEDI's ARLO.
    #[default]
    Arlo,
    /// The χ-separation toolbox's `r2star_nnls`, which its script runs for echo times written as
    /// a row (QSM.rs rule chi-sep-medi.csf-r2star-nnls).
    Nnls,
}
impl ChiSepCsfR2star {
    /// Every variant, in the order the CLI help and the TUI list them.
    pub const ALL: [ChiSepCsfR2star; 2] = [Self::Arlo, Self::Nnls];

    /// The name the CLI, the config file and the TUI use (qsm-core's `ChiSepCsfR2star::id`).
    pub fn as_str(self) -> &'static str {
        qsm_core::separation::ChiSepCsfR2star::from(self).id()
    }

    /// The method's name in log lines and the methods text.
    pub fn label(self) -> &'static str {
        match self { Self::Arlo => "ARLO", Self::Nnls => "NNLS" }
    }
}
impl fmt::Display for ChiSepCsfR2star {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<ChiSepCsfR2star> for qsm_core::separation::ChiSepCsfR2star {
    fn from(m: ChiSepCsfR2star) -> Self {
        use qsm_core::separation::ChiSepCsfR2star as M;
        match m { ChiSepCsfR2star::Arlo => M::Arlo, ChiSepCsfR2star::Nnls => M::Nnls }
    }
}
impl From<qsm_core::separation::ChiSepCsfR2star> for ChiSepCsfR2star {
    fn from(m: qsm_core::separation::ChiSepCsfR2star) -> Self {
        use qsm_core::separation::ChiSepCsfR2star as M;
        match m { M::Arlo => Self::Arlo, M::Nnls => Self::Nnls }
    }
}

pub fn parse_chi_sepnet_route(s: &str) -> Option<ChiSepNetRoute> {
    match s.trim() {
        "toolbox" => Some(ChiSepNetRoute::Toolbox),
        "native" => Some(ChiSepNetRoute::Native),
        _ => None,
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

/// Which echo time a method converts the field from ppm to radians at (qsm-core `EchoTime`).
///
/// In the config it is `"first"`, `"last"`, `"spacing"`, `"sum"` or `{ seconds = … }`; on the
/// command line it is one of those names or a time in milliseconds ([`EchoTimeChoice::parse`]).
/// Each method that takes it has its own default, the one its reference code uses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EchoTimeChoice {
    /// The first echo time.
    First,
    /// The last echo time.
    Last,
    /// The echo spacing, TE2 − TE1 (the echo time for one echo).
    Spacing,
    /// The sum of the echo times.
    Sum,
    /// A given echo time, in seconds.
    Seconds(f64),
}
impl EchoTimeChoice {
    /// The named choices, in the order the CLI help and the TUI list them.
    pub const NAMED: [EchoTimeChoice; 4] = [Self::First, Self::Last, Self::Spacing, Self::Sum];

    /// A command-line value: `first`, `last`, `spacing`, `sum`, or an echo time in milliseconds.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "first" => Ok(Self::First),
            "last" => Ok(Self::Last),
            "spacing" => Ok(Self::Spacing),
            "sum" => Ok(Self::Sum),
            other => match other.parse::<f64>() {
                Ok(ms) if ms.is_finite() && ms > 0.0 => Ok(Self::Seconds(ms / 1000.0)),
                _ => Err(format!(
                    "expected first, last, spacing, sum or an echo time in ms (> 0), got '{other}'"
                )),
            },
        }
    }
}
impl fmt::Display for EchoTimeChoice {
    /// The command-line form: the choice's name, or the time in milliseconds.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::First => f.write_str("first"),
            Self::Last => f.write_str("last"),
            Self::Spacing => f.write_str("spacing"),
            Self::Sum => f.write_str("sum"),
            // Rounded to the nanosecond, so 7 ms is not shown as 7.000000000000001.
            Self::Seconds(s) => write!(f, "{}", (s * 1e9).round() / 1e6),
        }
    }
}
impl From<EchoTimeChoice> for qsm_core::EchoTime {
    fn from(c: EchoTimeChoice) -> Self {
        use qsm_core::EchoTime as E;
        match c {
            EchoTimeChoice::First => E::First,
            EchoTimeChoice::Last => E::Last,
            EchoTimeChoice::Spacing => E::Spacing,
            EchoTimeChoice::Sum => E::Sum,
            EchoTimeChoice::Seconds(s) => E::Seconds(s),
        }
    }
}
/// DECOMPOSE's bound on |χ| in the fit (qsm-core `DecomposeChiBound`).
///
/// In the config and on the command line it is a number in ppm (`0.5`, the authors' and the
/// default) or `"echo-spacing"`: π / ((2/3)·γ·B0·ΔTE_max), from the scan's field strength and
/// echo times ([`ChiBoundChoice::parse`]). A settings file from before the choice existed held the
/// number, and reads as it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChiBoundChoice {
    /// A fixed bound, in ppm.
    Ppm(f64),
    /// From the field strength and the echo spacing.
    EchoSpacing,
}
impl ChiBoundChoice {
    /// The named choices, in the order the CLI help and the TUI list them (after a bound in ppm).
    pub const NAMED: [ChiBoundChoice; 1] = [Self::EchoSpacing];

    /// A command-line value: `echo-spacing`, or a bound in ppm. Whether a number is a bound that
    /// can be used is [`validate`](Self::validate)'s to say, as for a config file.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "echo-spacing" => Ok(Self::EchoSpacing),
            other => other.parse::<f64>().map(Self::Ppm)
                .map_err(|_| format!("expected echo-spacing or a bound in ppm, got '{other}'")),
        }
    }

    /// Why the bound cannot be used whatever the scan, if it cannot (qsm-core's check of a fixed
    /// bound); one from the echo spacing is checked against the scan by qsm-core.
    pub fn validate(&self) -> Result<(), String> {
        let p = qsm_core::separation::DecomposeParams { chi_bound: (*self).into(), ..Default::default() };
        p.validate().map_err(|e| e.0)
    }

    /// The bound in ppm at `b0` (T) and `echo_times` (s), as qsm-core resolves it.
    pub fn resolve(self, b0: f64, echo_times: &[f64]) -> Option<f64> {
        qsm_core::separation::DecomposeChiBound::from(self).resolve(b0, echo_times).ok()
    }
}
impl fmt::Display for ChiBoundChoice {
    /// The command-line form: `echo-spacing`, or the bound in ppm.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ppm(x) => write!(f, "{x}"),
            Self::EchoSpacing => f.write_str("echo-spacing"),
        }
    }
}
impl Serialize for ChiBoundChoice {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Ppm(x) => s.serialize_f64(*x),
            Self::EchoSpacing => s.serialize_str("echo-spacing"),
        }
    }
}
impl<'de> Deserialize<'de> for ChiBoundChoice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw { N(f64), S(String) }
        match Raw::deserialize(d)? {
            Raw::N(x) => Ok(Self::Ppm(x)),
            Raw::S(s) => Self::parse(&s).map_err(serde::de::Error::custom),
        }
    }
}
impl From<ChiBoundChoice> for qsm_core::separation::DecomposeChiBound {
    fn from(c: ChiBoundChoice) -> Self {
        use qsm_core::separation::DecomposeChiBound as B;
        match c {
            ChiBoundChoice::Ppm(x) => B::Ppm(x),
            ChiBoundChoice::EchoSpacing => B::EchoSpacing,
        }
    }
}
impl From<qsm_core::separation::DecomposeChiBound> for ChiBoundChoice {
    fn from(b: qsm_core::separation::DecomposeChiBound) -> Self {
        use qsm_core::separation::DecomposeChiBound as B;
        match b {
            B::Ppm(x) => Self::Ppm(x),
            B::EchoSpacing => Self::EchoSpacing,
        }
    }
}

impl From<qsm_core::EchoTime> for EchoTimeChoice {
    fn from(e: qsm_core::EchoTime) -> Self {
        use qsm_core::EchoTime as E;
        match e {
            E::First => Self::First,
            E::Last => Self::Last,
            E::Spacing => Self::Spacing,
            E::Sum => Self::Sum,
            E::Seconds(s) => Self::Seconds(s),
        }
    }
}
