use clap::ValueEnum;

// ─── Shared enums (used by RunArgs) ───

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum UnwrapAlgorithmArg {
    Romeo, Laplacian, Prelude, MediRegionGrowing,
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
