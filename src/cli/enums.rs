use clap::ValueEnum;

// ─── Shared enums (used by RunArgs) ───

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum QsmAlgorithmArg {
    Rts, Tv, Tkd, Tsvd, Tgv, Tikhonov, Nltv, Medi, Tfi, Ilsqr, Star, Lsqr,
    /// LSQR+HEIDI, HEIDI as published: LSQR with HEIDI after it (`--qsm-algorithm lsqr --heidi`)
    Heidi,
    Qsmart,
    Ndi, Fansi, FansiTgv, L1qsm, Whqsm, Hdqsm, AmpPe,
    // Deep-learning dipole inversions (weights downloaded on first use).
    Xqsm, Qsmnet, QsmnetPlus, Qsmnetme, Autoqsm, Qsmgan, Ir2qsm, Lpcnn, ModlQsm, Nextqsm,
    // End-to-end DL reconstructions from wrapped phase (no separate unwrap/BFR).
    Iqsm, IqsmPlus,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum ChiSepNetRouteArg {
    /// The authors' route (training frame, QSMnetME susceptibility channel)
    Toolbox,
    /// The network alone on the acquired grid, given the QSM
    Native,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum SwiUnwrapArg {
    /// Each echo with the Laplacian method, then combined
    Laplacian,
    /// Laplacian, slice by slice (2D multi-slice acquisitions)
    LaplacianSlice,
    /// All echoes together with ROMEO
    Romeo,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum SwiScalingArg {
    Tanh,
    #[value(alias = "negative_tanh")]
    NegativeTanh,
    Positive,
    Negative,
    Triangular,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum UnwrapAlgorithmArg {
    Romeo, Laplacian, Prelude, MediRegionGrowing,
}

/// `--prelude-labelling`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum PreludeLabellingArg {
    /// FSL prelude's default: slice by slice above 64 voxels in-plane, else over the whole volume
    Auto,
    /// Label regions slice by slice (FSL prelude --labelslices)
    Slices,
    /// Label regions over the whole volume (FSL prelude -f)
    Volume,
}

impl From<PreludeLabellingArg> for qsmxt_engine::PreludeLabelling {
    fn from(a: PreludeLabellingArg) -> Self {
        match a {
            PreludeLabellingArg::Auto => Self::Auto,
            PreludeLabellingArg::Slices => Self::Slices,
            PreludeLabellingArg::Volume => Self::Volume,
        }
    }
}

/// `qsmxt bgremove sharp --variant`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum SharpVariantArg {
    Sepia, Qsmjl,
}

impl From<SharpVariantArg> for qsm_core::bgremove::SharpVariant {
    fn from(a: SharpVariantArg) -> Self {
        match a {
            SharpVariantArg::Sepia => Self::Sepia,
            SharpVariantArg::Qsmjl => Self::QsmJl,
        }
    }
}

/// `qsmxt invert tfi --kernel`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum TfiKernelArg {
    Default, Original,
}

impl From<TfiKernelArg> for qsm_core::inversion::TfiKernel {
    fn from(a: TfiKernelArg) -> Self {
        match a {
            TfiKernelArg::Default => Self::Dft,
            TfiKernelArg::Original => Self::MediOddGrid,
        }
    }
}

/// `qsmxt invert tfi --precision`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum TfiPrecisionArg {
    Double, Single,
}

impl From<TfiPrecisionArg> for qsm_core::inversion::TfiPrecision {
    fn from(a: TfiPrecisionArg) -> Self {
        match a {
            TfiPrecisionArg::Double => Self::Double,
            TfiPrecisionArg::Single => Self::Single,
        }
    }
}

/// `qsmxt bgremove vsharp --ball`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum VsharpBallArg {
    Default, Qsmjl,
}

impl From<VsharpBallArg> for qsm_core::bgremove::VsharpBall {
    fn from(a: VsharpBallArg) -> Self {
        match a {
            VsharpBallArg::Default => Self::StiSuite,
            VsharpBallArg::Qsmjl => Self::QsmJl,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum VsharpGridArg {
    Native, Finest, Isotropic,
}

impl From<VsharpGridArg> for qsmxt_engine::VsharpGrid {
    fn from(a: VsharpGridArg) -> Self {
        match a {
            VsharpGridArg::Native => Self::Native,
            VsharpGridArg::Finest => Self::Finest,
            VsharpGridArg::Isotropic => Self::Isotropic,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum InhomogeneityMethodArg {
    /// Divide by a smoothed estimate of the receive sensitivity (MriResearchTools' makehomogeneous)
    Makehomogeneous,
    /// N4 bias field correction, as ANTs' N4BiasFieldCorrection
    N4,
}

impl From<InhomogeneityMethodArg> for qsmxt_engine::InhomogeneityMethod {
    fn from(a: InhomogeneityMethodArg) -> Self {
        match a {
            InhomogeneityMethodArg::Makehomogeneous => Self::Makehomogeneous,
            InhomogeneityMethodArg::N4 => Self::N4,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum LaplacianSolverArg {
    Dct,
    #[value(alias = "sti")]
    Fft,
    /// FFT solve on the volume mirrored along each axis, as QSMART unwraps
    Mirror,
    /// QSM.jl's MGPCG: Laplacian unwrapping with LBV background removal in one step
    Mgpcg,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum B0EstimationArg {
    WeightedAvg, LinearFit,
    /// Motion-robust (IRLS) linear fit: down-weights echoes that depart from a straight line in
    /// TE, voxel by voxel. Needs 4 echoes (3 with --linear-fit-estimate-offset false)
    RobustFit,
    /// QSMART's echo fit: a magnitude-weighted fit of the unwrapped phase vs TE through the
    /// origin, which also marks the voxels QSMART leaves out
    QsmartEchofit,
    /// MEDI's field map (Fit_ppm_complex): a magnitude-weighted fit of the complex echoes, then
    /// unwrapping of the fitted phase per echo spacing. Needs the magnitude and evenly spaced
    /// echoes; no phase offset removal is run first, and bipolar correction is refused
    ComplexFit,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum QsmartFitThresholdModeArg {
    Percentile, Absolute,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum VesselSizeUnitsArg {
    Voxels, Mm,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum QsmartFrangiCModeArg {
    Absolute, Relative,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum QsmartOffsetGridArg {
    Fft, QsmartReference,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum DecomposeFitArg {
    ReferenceCode, Paper,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum QsmartVesselMagnitudeArg {
    MeanN4, Combined,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum RobustLossArg {
    Tukey, Huber,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum B0WeightTypeArg {
    PhaseSNR, PhaseVar, Average,
    #[value(name = "tes")]
    TEs,
    Mag, AssumedDecay,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum MaskPresetArg {
    RobustThreshold, Bet, HdBet, BetAndPhase, Rs2Net,
    #[value(name = "mindgrab")]
    MindGrab,
    /// Removed. Still parsed, and hidden, so a script that asks for it gets an error that says
    /// what to use instead rather than clap's bare "invalid value".
    #[value(hide = true)]
    MouseBet,
}
