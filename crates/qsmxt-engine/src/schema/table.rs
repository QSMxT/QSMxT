//! The settings table: one entry per user-facing [`PipelineConfig`](crate::PipelineConfig) setting,
//! in the TUI's order, with its labels and help.
//!
//! An entry's label and help (and `.value(..)`) are the TUI's own wording (`src/tui/app.rs`); the
//! TUI's tests check that the two agree, so edit both together. `.full(..)`, `.gui(..)` and
//! `.gui_value(..)` give the wording for other front ends where the TUI's is abbreviated, names
//! the default, repeats the units, or is missing.
//!
//! `.flag("--x")` gives the setting a `qsmxt run` flag, which its type describes ([`super::cli`]):
//! `.cli(..)` words its help for the command line (the default is added from the setting's), and
//! the other flag methods (`.value_name(..)`,
//! `.parse(..)`, `.negated(..)`, ...) say only where the flag differs from what the type implies.
//!
//! A method's parameter is `.of(..)` the method choice it belongs to: listed under the choice
//! and shown while the method its path names is chosen. Settings many methods have (padding,
//! echo time, tolerance, iteration cap) are made by the helpers at the end of this file.
//!
//! Each section's entries are in `table/<section>.rs`.

mod input;
mod geometry;
mod masking;
mod motion;
mod field_map;
mod background;
mod inversion;
mod multi_orientation;
mod reference;
mod separation;
mod magnitude;
mod swi_smwi;
mod relaxometry;
mod segmentation;
mod export;

use serde_json::{json, Value};

use super::cli::FlagParse as P;
use super::{d, o, Def, Of, Section, Section::*, Ty::*};
use crate::enums::*;
use crate::store::Item;
use qsm_core::utils::wavelet::SUPPORTED_ORDERS as WAVELET_ORDERS;

/// Every section's entries, in the TUI's order.
pub(super) fn table() -> Vec<Def> {
    [
        input::defs(),
        geometry::defs(),
        masking::defs(),
        motion::defs(),
        field_map::defs(),
        background::defs(),
        inversion::defs(),
        multi_orientation::defs(),
        reference::defs(),
        separation::defs(),
        magnitude::defs(),
        swi_smwi::defs(),
        relaxometry::defs(),
        segmentation::defs(),
        export::defs(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

// ─── Settings many methods have ───

/// A method's zero-padding before its FFTs (`inversion.<method>.pad`). QSM.rs utils/padding.rs
/// with_qsmjl_padding: < 0 leaves an axis unpadded.
fn fft_pad(path: &'static str, flag: &'static str) -> Def {
    d(path, Inversion, "Pad (voxels)", "Zero-padding before the FFTs, voxels (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded (default 0 0 0)", IntVec3)
        .min(-1.0).units("voxels").flag(flag).full("Padding")
        .gui("Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded")
}

/// The echo time a method converts the field to phase at (`inversion.<method>.te`); `method`
/// names it in the flag's help.
fn te(path: &'static str, flag: &'static str, method: &'static str) -> Def {
    d(path, Inversion, "Echo time", "", EchoTime)
        .value("Echo time (ms)", "Echo time the field is converted to phase at, ms").flag(flag)
        .gui("The echo time the field is converted to phase at").gui_value("Echo time", "The echo time the field is converted to phase at, in ms")
        .cli(Box::leak(format!("{method}: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms").into_boxed_str()))
}

/// A method's convergence tolerance.
fn tol(path: &'static str, section: Section, flag: &'static str) -> Def {
    d(path, section, "Tolerance", "Convergence tolerance", Float).flag(flag)
}

/// A method's iteration cap, `help` saying which iterations ("Maximum ADMM iterations").
fn max_iter(path: &'static str, section: Section, help: &'static str, flag: &'static str) -> Def {
    d(path, section, "Max iter", help, Int).flag(flag).full("Maximum iterations")
}
