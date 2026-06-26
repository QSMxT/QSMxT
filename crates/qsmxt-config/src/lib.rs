//! QSMxT Pipeline Configuration Library
//!
//! Shared configuration, command generation, and methods text for QSMxT tools.
//! Used by both qsmxt.rs (CLI/TUI) and qsmbly (browser WASM).

pub mod error;
pub mod enums;
pub mod masking;
pub mod config;
pub mod command;
pub mod methods;
pub mod bridge;

pub use config::*;
pub use enums::*;
pub use masking::*;
pub use error::{ConfigError, Result};
pub use command::generate_command;
pub use methods::generate_methods;
pub use bridge::{to_pipeline_stages, to_scan_metadata, to_mask_sections};
