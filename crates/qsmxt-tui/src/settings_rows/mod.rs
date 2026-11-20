//! Rows built from the engine's settings registry ([`qsmxt_engine::schema`]), and the settings
//! they edit, kept in a config rather than in fields of the form.
//!
//! A page built here lists its section's settings in the registry's order, each shown while
//! `settings_view` says it applies, with one widget per value type: a checkbox for a bool, a
//! select for a choice, a text row for anything typed. Labels, help and options are the
//! registry's TUI wording; a setting is indented once for each setting it is listed under on the
//! same page. A row is known to the rest of the TUI (focus, advice, Basic mode) by its setting's
//! path ([`FIELD_PATHS`]).
//!
//! The settings of the sections in [`STORED_SECTIONS`] live in [`SettingsStore`]: a config, plus
//! the text typed on their text rows. The two settings outside them are the Input tab's
//! orientation grouping and DICOM export's switch, which the run form keeps with its other run
//! options.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

use qsmxt_engine::schema::{self, EchoKind, Section, Setting, ValueType, ViewFacts};
use qsmxt_engine::PipelineConfig;

use super::app::{field_of_path, field_path, is_choice, App, Page, PipelineRow, RowContext, FIELD_PATHS};

/// The sections whose settings are kept in [`SettingsStore`] rather than in the form's fields.
pub const STORED_SECTIONS: &[Section] =
    &[Section::Geometry, Section::Masking, Section::Motion, Section::Background, Section::Reference, Section::Magnitude, Section::SwiSmwi,
      Section::Relaxometry, Section::Segmentation, Section::MultiOrientation, Section::FieldMap,
      Section::Inversion, Section::Separation];

/// Choices that one select picks with another: `(path, choice, other)`. The select lists the
/// setting's options but `choice`, then each of `other`'s, which picks `choice` with that value;
/// `other` has no row of its own. The QSM reference picker lists the regions with the methods.
const PICKERS: &[(&str, &str, &str)] = &[("qsm.reference", "region", "qsm.reference_region")];

fn picker(path: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    PICKERS.iter().find(|(p, ..)| *p == path)
}

/// Stored settings the form passes the run by rules of its own rather than while they apply
/// (`form_values` in [`super::command`]), as it did while they were fields of the form: the
/// inversion and MEDI's SMV mode always, background removal while its stage runs, HEIDI not for an
/// orientation group, QSM's switch when off. [`SettingsStore::changed`] leaves them out.
pub const PASSED_BY_THE_FORM: &[&str] =
    &["bg_removal.algorithm", "inversion.algorithm", "inversion.medi.smv", "inversion.heidi.enabled", "pipeline.do_qsm"];

mod layout;
mod rows;
mod store;
mod values;

use layout::*;
use values::*;

pub use rows::is_value_row;
pub use store::{stored_placeholder, SettingsStore, VisibleCache};
pub use values::{options, stored_path, with_values, Refused};
pub(crate) use values::{depth_of, setting};

#[cfg(test)]
mod tests;
