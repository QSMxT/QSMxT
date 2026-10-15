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

/// Why the run logic refused a run: an input or a combination of settings it cannot use. The
/// message is complete as it stands, and front ends show it as it is (`qsmxt` as a pipeline
/// configuration error).
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("{0}")]
    Config(String),
}
