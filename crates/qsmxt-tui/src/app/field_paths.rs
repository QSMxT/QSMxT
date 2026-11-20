//! The config setting each TUI row edits.
//!
//! A row's field name follows from its setting's path by one rule ([`PREFIXES`]), so a new
//! setting needs no edit here unless its row is named otherwise ([`NAMES`]).

use std::sync::LazyLock;

use qsmxt_engine::schema;

/// The config setting a TUI row edits, by the row's field name, in the settings' order; a setting
/// with a value row has two names. Rows that edit no config setting (the Execution tab, DICOM
/// export's run options, the mask preset, which writes the recipe) are not listed. The settings
/// tests (`schema_parity`) hold the TUI to it.
pub static FIELD_PATHS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(derive);

/// The TUI's short prefix for each group of settings. A row's field name is its setting's path
/// with the longest of these prefixes it starts with swapped for the short one, and its other
/// dots made underscores: `bg_removal.vsharp.threshold` is `vsharp_threshold`,
/// `homogeneity.sigma_mm` (no prefix here) `homogeneity_sigma_mm`.
const PREFIXES: &[(&str, &str)] = &[
    ("pipeline.", ""), ("masking.", ""), ("field_mapping.", ""), ("bg_removal.", ""), ("inversion.", ""),
    ("registration.", "reg_"), ("motion_correction.", "motion_"), ("multi_orientation.", "orientation_"),
    ("segmentation.", "synthseg_"), ("separation.", ""), ("separation.r2star_qsm.", "sep_r2starqsm_"),
    ("separation.decompose.", "sep_decompose_"), ("separation.decompose.lsqcurvefit.", "sep_decompose_"),
    ("separation.chi_sep_ilsqr.", "sep_ilsqr_"), ("separation.chi_sep_ilsqr.qsm_ilsqr.", "sep_ilsqr_qsm_"),
    ("separation.chi_sep_medi.", "sep_medi_"), ("separation.chi_sep_medi.qsm_ilsqr.", "sep_medi_qsm_"),
    ("separation.wavesep.", "sep_wavesep_"), ("separation.hc_chisep.", "sep_hcchisep_"),
    ("separation.chi_sepnet.", "sep_chisepnet_"), ("separation.susep_net.", "sep_susepnet_"),
];

/// The settings whose rows the rule ([`PREFIXES`]) does not name, with each of their rows' names
/// in order: the setting's own row, then its value's row (an echo time's milliseconds, the size
/// of a checkbox with a value), or a choice the TUI makes about it before either.
const NAMES: &[(&str, &[&str])] = &[
    ("motion_correction.enabled", &["motion_correction"]),
    ("motion_correction.reference", &["motion_reference", "motion_reference_echo"]),
    ("field_mapping.prelude.num_phase_partitions", &["prelude_partitions"]),
    ("field_mapping.romeo.temporal_uncertain_unwrapping", &["romeo_temporal_uncertain", "romeo_temporal_uncertain_unwrapping"]),
    ("bg_removal.algorithm", &["bf_algorithm"]), ("bg_removal.polish_mask", &["bf_polish_mask"]),
    ("bg_removal.ukb_mask_trim", &["bf_ukb_mask_trim"]), ("bg_removal.erode_voxels", &["bf_erode_voxels"]),
    ("inversion.algorithm", &["qsm_algorithm"]),
    ("inversion.nltv.te", &["nltv_te", "nltv_te_ms"]), ("inversion.medi.te", &["medi_te", "medi_te_ms"]),
    ("inversion.tfi.te", &["tfi_te", "tfi_te_ms"]), ("inversion.ndi.te", &["ndi_te", "ndi_te_ms"]),
    ("inversion.fansi.te", &["fansi_te", "fansi_te_ms"]), ("inversion.l1qsm.te", &["l1qsm_te", "l1qsm_te_ms"]),
    ("inversion.whqsm.te", &["whqsm_te", "whqsm_te_ms"]), ("inversion.heidi.te", &["heidi_te", "heidi_te_ms"]),
    ("inversion.tgv.te", &["tgv_te", "tgv_te_ms"]), ("inversion.star.te", &["star_te", "star_te_ms"]),
    ("inversion.nltv.max_iter_newton", &["nltv_newton_iter"]),
    ("inversion.qsmart.vessel_size_units", &["qsmart_vessel_units"]),
    ("inversion.tile_size", &["dl_tiling", "dl_tile_size"]), ("inversion.tile_halo", &["dl_tile_halo"]),
    ("multi_orientation.magnitude_weighting", &["cosmos_magnitude_weighting"]),
    ("multi_orientation.cosmos_tol", &["cosmos_tol"]), ("multi_orientation.cosmos_max_iter", &["cosmos_max_iter"]),
    ("multi_orientation.register", &["orientation_registration"]),
    ("multi_orientation.force", &["multi_orientation_force"]),
    ("separation.algorithm", &["separation_algorithm"]),
    ("separation.decompose.chi_bound", &["sep_decompose_chi_bound_mode", "sep_decompose_chi_bound"]),
    ("separation.decompose.lsqcurvefit.max_function_evaluations", &["sep_decompose_max_fevals"]),
    ("separation.decompose.lsqcurvefit.function_tolerance", &["sep_decompose_function_tol"]),
    ("separation.decompose.lsqcurvefit.step_tolerance", &["sep_decompose_step_tol"]),
    ("separation.decompose.lsqcurvefit.optimality_tolerance", &["sep_decompose_optimality_tol"]),
    ("separation.decompose.lsqcurvefit.finite_difference_step", &["sep_decompose_fd_step"]),
    ("smwi.threshold_ppm", &["smwi_threshold"]),
    ("segmentation.custom_dseg_tool", &["segmentation_source", "custom_dseg_tool"]),
    ("segmentation.sigma_smoothing", &["synthseg_smoothing", "synthseg_sigma"]),
];

/// Settings the TUI edits with a widget of its own rather than a named row: the mask recipe,
/// the reference region picker and the orientation grouping.
const NO_ROW: &[&str] = &[
    "masking.sections", "masking.combine", "masking.refinements", "masking.two_pass_sections",
    "masking.two_pass_combine", "qsm.reference_region", "multi_orientation.group_by",
];

/// A row's field name by the rule: its setting's path with its group's prefix ([`PREFIXES`])
/// made short and its dots underscores.
fn rule_name(path: &str) -> String {
    let (long, short) = PREFIXES.iter().filter(|(p, _)| path.starts_with(p)).max_by_key(|(p, _)| p.len()).copied().unwrap_or(("", ""));
    format!("{short}{}", path[long.len()..].replace('.', "_"))
}

/// Every row's field name and the setting it edits, in the settings' order.
fn derive() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for path in schema::settings().iter().map(|s| s.path).filter(|p| !NO_ROW.contains(p)) {
        match NAMES.iter().find(|(p, _)| *p == path) {
            Some((_, names)) => out.extend(names.iter().map(|n| (*n, path))),
            // Built once, so the few hundred names are leaked once to live as long as the table.
            None => out.push((&*Box::leak(rule_name(path).into_boxed_str()), path)),
        }
    }
    out
}

/// The config setting a TUI row edits, by the row's field name ([`FIELD_PATHS`]).
pub fn field_path(field: &str) -> Option<&'static str> {
    FIELD_PATHS.iter().find(|(f, _)| *f == field).map(|(_, p)| *p)
}

/// The form field of a config setting path: [`field_path`]'s inverse.
pub fn field_of_path(path: &str) -> Option<&'static str> {
    FIELD_PATHS.iter().find(|(_, p)| *p == path).map(|(f, _)| *f)
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

    /// The tables name registry settings, every prefix names some row, and every entry of
    /// [`NAMES`] is one the rule gets wrong.
    #[test]
    fn the_tables_are_the_fewest_that_name_every_row() {
        let paths: Vec<&str> = schema::settings().iter().map(|s| s.path).collect();
        for path in NO_ROW.iter().chain(NAMES.iter().map(|(p, _)| p)) {
            assert!(paths.contains(path), "{path} is not a setting");
        }
        for (path, names) in NAMES {
            assert_ne!(*names, [rule_name(path).as_str()], "{path}'s row is named by the rule");
        }
        for (long, _) in PREFIXES {
            assert!(FIELD_PATHS.iter().any(|(_, p)| p.starts_with(long)), "no setting starts with {long}");
        }
    }
}
