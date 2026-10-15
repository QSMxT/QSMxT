//! QSMxT's run engine.
//!
//! Pipeline configuration, command generation and methods text for QSMxT, with no file I/O, so it
//! also builds for the browser. Used by both qsmxt (CLI/TUI) and qsmbly (browser WASM). The run
//! logic in `qsmxt`'s `src/pipeline/` moves here in stages.

/// The qsm-core this crate is built against. Downstream consumers (qsmbly's WASM crate) use
/// algorithms through this re-export rather than depending on qsm-core themselves, so the
/// pipeline configuration and the algorithms always come from a single qsm-core version.
/// qsm-core's `simd`, `onnx` and `parallel` features are forwarded as features of this crate.
pub use qsm_core;

pub mod error;
pub mod enums;
pub mod masking;
pub mod regions;
pub mod config;
pub mod command;
pub mod methods;
pub mod bridge;
pub mod motion;

pub use config::*;
pub use enums::*;
pub use masking::*;
pub use motion::{MotionCorrectionConfig, MotionReference, RegistrationConfig};
pub use error::{ConfigError, Result};
pub use command::generate_command;
pub use methods::generate_methods;
pub use bridge::{to_pipeline_stages, to_scan_metadata, to_mask_sections, to_mask_ops};
