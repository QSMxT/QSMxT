//! The settings table: one entry per user-facing [`PipelineConfig`](crate::PipelineConfig) setting,
//! in the TUI's order, with its labels and help.
//!
//! An entry's label and help (and `.value(..)`) are the TUI's own wording (`src/tui/app.rs`); the
//! TUI's tests check that the two agree, so edit both together. `.full(..)`, `.gui(..)` and
//! `.gui_value(..)` give the wording for other front ends where the TUI's is abbreviated, names
//! the default, repeats the units, or is missing.
//!
//! `.flag("--x")` gives the setting a `qsmxt run` flag, which its type describes ([`super::cli`]):
//! `.cli(..)` words its help for the command line, and the other flag methods (`.value_name(..)`,
//! `.parse(..)`, `.negated(..)`, ...) say only where the flag differs from what the type implies.
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
use super::{d, o, Def, Section::*, Ty::*};
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
