//! The config setting each TUI row edits.
//!
//! A setting's row is named by its config path (`bg_removal.vsharp.threshold`). A setting shown
//! with a value under it (an echo time's milliseconds, the size of a checkbox with a value) has a
//! second row for the value, named by the path and [`VALUE_ROW`]. Rows that edit no config setting
//! (the Execution tab, DICOM export's run options, the mask preset, which writes the recipe), a
//! setting the run form holds ([`FORM_ROWS`]) and a choice the TUI makes about a setting
//! ([`CHOICES`]) have names of their own.

use std::sync::LazyLock;

use qsmxt_engine::schema;

/// The config setting a TUI row edits, by the row's field name, in the settings' order. The
/// settings tests (`schema_parity`) hold the TUI to it.
pub static FIELD_PATHS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(derive);

/// What a setting's value row is named by after its path: `inversion.medi.te#value`.
pub const VALUE_ROW: &str = "#value";

/// Choices the TUI makes about a setting before its own row, and the setting: where a
/// segmentation comes from, which is the supplied one's tool when it is not SynthSeg.
const CHOICES: &[(&str, &str)] = &[("segmentation_source", "segmentation.custom_dseg_tool")];

/// Settings whose row is a field of the run form, named by the field: exporting DICOM, which the
/// run form keeps with the export's own options.
const FORM_ROWS: &[(&str, &str)] = &[("export_dicom", "pipeline.export_dicom")];

/// Whether `field` is a choice the TUI makes about a setting ([`CHOICES`]), not the setting's row.
pub fn is_choice(field: &str) -> bool {
    CHOICES.iter().any(|(f, _)| *f == field)
}

/// Settings the TUI edits with a widget of its own rather than a named row: the mask recipe,
/// the reference region picker and the orientation grouping.
const NO_ROW: &[&str] = &[
    "masking.sections", "masking.combine", "masking.refinements", "masking.two_pass_sections",
    "masking.two_pass_combine", "qsm.reference_region", "multi_orientation.group_by",
];

/// Every row's field name and the setting it edits, in the settings' order.
fn derive() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for s in schema::settings().iter().filter(|s| !NO_ROW.contains(&s.path)) {
        out.extend(CHOICES.iter().filter(|(_, p)| *p == s.path).copied());
        out.push(FORM_ROWS.iter().find(|(_, p)| *p == s.path).copied().unwrap_or((s.path, s.path)));
        if s.tui_value.is_some() {
            // Built once, so these few names are leaked once to live as long as the table.
            out.push((&*Box::leak(format!("{}{VALUE_ROW}", s.path).into_boxed_str()), s.path));
        }
    }
    out
}

/// The config setting a TUI row edits, by the row's field name ([`FIELD_PATHS`]).
pub fn field_path(field: &str) -> Option<&'static str> {
    FIELD_PATHS.iter().find(|(f, _)| *f == field).map(|(_, p)| *p)
}

/// The form field of a config setting path's own row: [`field_path`]'s inverse.
pub fn field_of_path(path: &str) -> Option<&'static str> {
    FIELD_PATHS.iter().find(|(f, p)| *p == path && !is_choice(f) && !f.ends_with(VALUE_ROW)).map(|(f, _)| *f)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No two rows share a field name.
    #[test]
    fn field_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (field, path) in FIELD_PATHS.iter() {
            assert!(seen.insert(field), "{field} names two rows (one edits {path})");
        }
    }

    /// The tables name registry settings.
    #[test]
    fn the_tables_name_settings() {
        let paths: Vec<&str> = schema::settings().iter().map(|s| s.path).collect();
        for path in NO_ROW.iter().chain(CHOICES.iter().chain(FORM_ROWS).map(|(_, p)| p)) {
            assert!(paths.contains(path), "{path} is not a setting");
        }
    }
}
