//! Export: the DICOM maps and their tick rows.

use super::*;

/// A map DICOM export can write: its tick row's field, the series suffix (`--dicom-outputs`
/// token), the label, and the output it belongs to (for "not made by these settings").
pub struct DicomMap {
    pub field: &'static str,
    pub suffix: &'static str,
    pub label: &'static str,
    pub group: &'static str,
}

/// Every map DICOM export writes, in series order ([`crate::dicom::export`]).
pub const DICOM_MAPS: [DicomMap; 17] = [
    DicomMap { field: "dicom_out_magnitude", suffix: "magnitude", label: "  Magnitude", group: "magnitude" },
    DicomMap { field: "dicom_out_chimap", suffix: "Chimap", label: "  χ (QSM)", group: "χ" },
    DicomMap { field: "dicom_out_swi", suffix: "swi", label: "  SWI", group: "SWI" },
    DicomMap { field: "dicom_out_minip", suffix: "minIP", label: "  SWI minIP", group: "SWI" },
    DicomMap { field: "dicom_out_swi_phase", suffix: "desc-swi_phase", label: "  SWI filtered phase", group: "SWI phase" },
    DicomMap { field: "dicom_out_smwi_para", suffix: "desc-paramagnetic_smwi", label: "  Paramagnetic SMWI", group: "SMWI" },
    DicomMap { field: "dicom_out_smwi_dia", suffix: "desc-diamagnetic_smwi", label: "  Diamagnetic SMWI", group: "SMWI" },
    DicomMap { field: "dicom_out_smwi_para_minip", suffix: "desc-paramagneticsmwi_minIP", label: "  Paramagnetic SMWI minIP", group: "SMWI" },
    DicomMap { field: "dicom_out_smwi_dia_minip", suffix: "desc-diamagneticsmwi_minIP", label: "  Diamagnetic SMWI minIP", group: "SMWI" },
    DicomMap { field: "dicom_out_t2starmap", suffix: "T2starmap", label: "  T2* map", group: "T2*" },
    DicomMap { field: "dicom_out_r2starmap", suffix: "R2starmap", label: "  R2* map", group: "R2*" },
    DicomMap { field: "dicom_out_r2map", suffix: "R2map", label: "  R2 map", group: "R2" },
    DicomMap { field: "dicom_out_r2primemap", suffix: "R2primemap", label: "  R2′ map", group: "R2′" },
    DicomMap { field: "dicom_out_chi_para", suffix: "desc-paramagnetic_Chimap", label: "  Paramagnetic χ", group: "χ-separation" },
    DicomMap { field: "dicom_out_chi_dia", suffix: "desc-diamagnetic_Chimap", label: "  Diamagnetic χ", group: "χ-separation" },
    DicomMap { field: "dicom_out_chi_total", suffix: "desc-total_Chimap", label: "  Total χ", group: "χ-separation" },
    DicomMap { field: "dicom_out_dseg", suffix: "dseg", label: "  Segmentation (DICOM SEG)", group: "segmentation" },
];

/// The Export map a tick row's field names.
pub fn dicom_map(field: &str) -> Option<&'static DicomMap> {
    DICOM_MAPS.iter().find(|m| m.field == field)
}

impl App {
    /// Export → DICOM: the switch, the source DICOM, and a tick per map the settings make.
    pub(super) fn export_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{Note, Param, Toggle};
        let mut rows = vec![Toggle { label: "Export DICOM", field: "export_dicom", help: "Also write final maps as DICOM series into each subject's extra_files/ folder" }];
        if self.form.export_dicom {
            rows.push(Param { label: "  Source DICOM dir", field: "source_dicom", help: "Original DICOM directory: the exported series inherit its patient/study identity (empty = none)" });
            rows.push(PipelineRow::Separator);
            rows.push(Note { text: "Maps to export".into() });
            let made: Vec<_> = DICOM_MAPS.iter().filter(|m| self.dicom_map_made(m)).collect();
            if made.is_empty() {
                rows.push(Note { text: "  None: these settings make no exportable map.".into() });
            }
            for m in made {
                rows.push(Toggle { label: m.label, field: m.field,
                    help: if m.field == "dicom_out_dseg" {
                        "The segmentation as one DICOM Segmentation object, its structures coded and coloured, drawn on an exported series on its grid: the magnitude, else χ, else another map"
                    } else {
                        "Space ticks or unticks the map. While every map is ticked all are exported, and a map turned on later is ticked too; a ticks them all"
                    } });
            }
            // The SEG is drawn on another exported series on its grid (the magnitude, χ, another
            // map): the run refuses a selection that leaves it none, and the checklist says so.
            let seg = dicom_map("dicom_out_dseg").expect("in the table");
            let reference_ticked = DICOM_MAPS.iter().any(|m| self.dicom_map_made(m) && self.field_toggle(m.field)
                && crate::dicom::export::SEG_REFERENCES.iter().any(|r| r.eq_ignore_ascii_case(m.suffix)));
            if self.dicom_map_made(seg) && self.field_toggle(seg.field) && !reference_ticked {
                rows.push(Note { text: "The segmentation needs a map to be drawn on: tick the".into() });
                rows.push(Note { text: "magnitude, χ or another map, or the run is refused.".into() });
            }
        }
        rows
    }

    /// Whether the current settings make an exportable map.
    pub(super) fn dicom_map_made(&self, m: &DicomMap) -> bool {
        match m.group {
            // The combined magnitude, when it is published.
            "magnitude" => self.form.settings.toggle("pipeline.do_magnitude"),
            "χ" => self.qsm_on(),
            "SWI" => self.form.settings.toggle("pipeline.do_swi"),
            "SWI phase" => self.form.settings.toggle("pipeline.do_swi") && self.form.settings.toggle("swi.filtered_phase"),
            "SMWI" => self.form.settings.toggle("pipeline.do_smwi"),
            "T2*" => self.output_made("do_t2starmap"),
            "R2*" => self.output_made("do_r2starmap"),
            "R2" => self.output_made("do_r2map"),
            "R2′" => self.output_made("do_r2primemap"),
            "χ-separation" => self.form.settings.toggle("pipeline.do_chi_separation"),
            "segmentation" => self.output_made("do_segmentation"),
            _ => false,
        }
    }

    /// `--dicom-outputs`: the maps made and ticked, when any made map is unticked; `None`
    /// exports every map made.
    pub fn dicom_outputs(&self) -> Option<Vec<String>> {
        let made: Vec<&DicomMap> = DICOM_MAPS.iter().filter(|m| self.dicom_map_made(m)).collect();
        let unticked = |m: &&DicomMap| self.form.dicom_unticked.iter().any(|u| u.eq_ignore_ascii_case(m.suffix));
        if !made.iter().any(unticked) {
            return None;
        }
        Some(made.into_iter().filter(|m| !unticked(m)).map(|m| m.suffix.to_string()).collect())
    }

    /// Tick or untick an Export map. The last ticked map is not unticked: that would export
    /// nothing, which Export DICOM itself says.
    pub(super) fn toggle_dicom_map(&mut self, m: &DicomMap) {
        let pos = self.form.dicom_unticked.iter().position(|u| u.eq_ignore_ascii_case(m.suffix));
        if let Some(i) = pos {
            self.form.dicom_unticked.remove(i);
            return;
        }
        let ticked = DICOM_MAPS.iter().filter(|d| self.dicom_map_made(d) && self.field_toggle(d.field)).count();
        if ticked <= 1 {
            self.error_message = Some("That is the only map left to export: untick Export DICOM instead.".to_string());
            return;
        }
        self.form.dicom_unticked.push(m.suffix.to_lowercase());
    }
}
