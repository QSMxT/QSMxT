//! QSMxT's file side, for the `qsmxt` binary: BIDS discovery and derivatives, DICOM and NIfTI
//! conversion and export, the run's file store, provenance and figures, multi-orientation runs,
//! example datasets and the local and SLURM executors.
//!
//! What runs and how is decided in `qsmxt-engine`, which has no file I/O; this crate reads and
//! writes the files. It has no terminal UI and no command-line parsing.

pub mod bids;
pub mod dataset;
pub mod dicom;
pub mod error;
pub mod example;
pub mod executor;
pub mod multiorient;
pub mod nifti;
pub mod pipeline;
pub mod settings;
#[cfg(any(test, feature = "testutils"))]
pub mod testutils;
pub mod version;

pub use error::Result;
