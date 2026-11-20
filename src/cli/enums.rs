use clap::ValueEnum;

// ─── Shared enums (used by RunArgs) ───

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
pub enum DecomposeFitArg {
    ReferenceCode, Paper,
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

// ─── Conversions to the engine ───

/// The recipe behind a `--mask-preset` value — shared by `run` and `qsmxt mask preset`.
pub fn mask_preset_recipe(preset: MaskPresetArg) -> qsmxt_io::Result<qsmxt_engine::MaskRecipe> {
    use clap::ValueEnum;
    let name = preset.to_possible_value().expect("no MaskPresetArg is skipped");
    qsmxt_engine::masking::mask_preset(name.get_name()).map_err(qsmxt_io::error::QsmxtError::Config)
}

/// Map a CLI SWI unwrapping argument to the config enum (shared by `run` and `qsmxt swi`).
pub fn swi_unwrap_arg_to_config(a: SwiUnwrapArg) -> qsmxt_engine::SwiUnwrap {
    match a {
        SwiUnwrapArg::Laplacian => qsmxt_engine::SwiUnwrap::Laplacian,
        SwiUnwrapArg::LaplacianSlice => qsmxt_engine::SwiUnwrap::LaplacianSlice,
        SwiUnwrapArg::Romeo => qsmxt_engine::SwiUnwrap::Romeo,
    }
}

/// Map a CLI SWI scaling argument to the config enum — the one place `run` and `qsmxt swi` both
/// go through, so the two cannot disagree about what a value means.
pub fn swi_scaling_arg_to_config(a: SwiScalingArg) -> qsmxt_engine::SwiScaling {
    use qsmxt_engine::SwiScaling as S;
    match a {
        SwiScalingArg::Tanh => S::Tanh,
        SwiScalingArg::NegativeTanh => S::NegativeTanh,
        SwiScalingArg::Positive => S::Positive,
        SwiScalingArg::Negative => S::Negative,
        SwiScalingArg::Triangular => S::Triangular,
    }
}

impl From<QsmartFitThresholdModeArg> for qsmxt_engine::QsmartFitThresholdMode {
    fn from(m: QsmartFitThresholdModeArg) -> Self {
        match m {
            QsmartFitThresholdModeArg::Percentile => qsmxt_engine::QsmartFitThresholdMode::Percentile,
            QsmartFitThresholdModeArg::Absolute => qsmxt_engine::QsmartFitThresholdMode::Absolute,
        }
    }
}
