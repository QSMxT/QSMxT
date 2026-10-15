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
