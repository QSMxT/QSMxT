//! The Export section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        // DICOM export is not in the browser yet (astewartau/qsmbly#164).
        d("pipeline.export_dicom", Export, "Export DICOM", "Also write final maps as DICOM series into each subject's extra_files/ folder", Bool).basic().flag("--export-dicom").unavailable_in_browser(),
    ]
}
