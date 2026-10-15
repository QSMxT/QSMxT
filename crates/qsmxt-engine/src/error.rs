use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Config parse error: {0}")]
    Parse(String),
    #[error("Config validation error: {0}")]
    Validation(String),
    #[error("Config serialization error: {0}")]
    Serialize(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// Why the run logic refused a run, or a stage its inputs. Each message is complete as it stands,
/// and front ends show it as it is; the variant says what kind of problem it is (`qsmxt` maps
/// each to its own error of the same kind).
#[derive(Error, Debug)]
pub enum EngineError {
    /// A combination of settings, or an input, the run cannot use.
    #[error("{0}")]
    Config(String),
    /// A volume whose size does not match the grid the run is processed on.
    #[error("{0}")]
    DimensionMismatch(String),
    /// QSM.rs refused a stage's input (too few echoes, echo times it cannot use, mismatched
    /// buffers). The message names what was wrong and with what values.
    #[error("{0}")]
    InvalidInput(String),
}

impl From<qsm_core::error::InputError> for EngineError {
    fn from(e: qsm_core::error::InputError) -> Self {
        EngineError::InvalidInput(e.0)
    }
}
