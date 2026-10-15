//! Per-structure statistics over a segmentation: the tables as files. What goes in them is the
//! engine's ([`qsmxt_engine::stages::analysis`]); reading them back for the figures, and the
//! dataset-level table, are here.

use std::path::{Path, PathBuf};

pub use qsmxt_engine::stages::analysis::{is_susceptibility, map_rows, ReferenceInfo, CHIMAP, COSMOS_CHIMAP, HEADER};

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::entities::AcquisitionKey;

/// BIDS entities identifying the run a row came from, prepended to every row of the group table.
const GROUP_HEADER: &str = "subject\tsession\tacquisition\treconstruction\tinversion\trun\t";

/// One parsed row of a statistics table.
///
/// The tables are written as text and read back by the figure renderer rather than kept in
/// memory: the per-run tables are written inside each run's process, and the dataset-level view is
/// assembled afterwards from the files. Parsing our own output keeps one definition of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub map: String,
    pub unit: String,
    pub name: String,
    pub mean: f64,
    pub sd: f64,
    /// The structure's volume, when the segmentation carried posteriors to measure it from. A
    /// property of the parcellation rather than of the map, so it repeats across a run's rows.
    pub volume_mm3: Option<f64>,
    /// Spread of `volume_mm3` across the runs a cohort row was built from. In-memory only: a
    /// per-run table has one volume per structure and so no spread to report.
    pub volume_sd_mm3: Option<f64>,
    /// What the susceptibility map was referenced to, on the rows where that means something.
    pub reference: Option<String>,
    /// The offset referencing removed, in ppm.
    pub reference_offset_ppm: Option<f64>,
}

/// What a susceptibility map was referenced to, from its JSON sidecar, where referencing recorded
/// it.
pub fn reference_from_sidecar(sidecar: &Path) -> Option<ReferenceInfo> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(sidecar).ok()?).ok()?;
    ReferenceInfo::from_fields(v.as_object()?)
}

/// Read back a table written with [`HEADER`], ignoring any entity columns a group table prepends.
///
/// Columns are found by name rather than by position, so a table that grows a column still reads.
/// Malformed rows are skipped rather than failing: a figure is a convenience, and refusing to draw
/// one because a single row is odd would be worse than drawing the rest.
pub fn parse_tsv(text: &str) -> Vec<Row> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return Vec::new() };
    let cols: Vec<&str> = header.split('\t').collect();
    let idx = |name: &str| cols.iter().position(|c| *c == name);
    let (Some(i_map), Some(i_unit), Some(i_name), Some(i_mean), Some(i_sd)) =
        (idx("map"), idx("unit"), idx("name"), idx("mean"), idx("sd")) else { return Vec::new() };
    // Optional: a supplied segmentation has no posteriors, so its volume column is empty.
    let i_vol = idx("volume_mm3");
    let (i_ref, i_off) = (idx("reference"), idx("reference_offset_ppm"));

    lines
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            Some(Row {
                map: f.get(i_map)?.to_string(),
                unit: f.get(i_unit)?.to_string(),
                name: f.get(i_name)?.to_string(),
                mean: f.get(i_mean)?.parse().ok()?,
                sd: f.get(i_sd)?.parse().ok()?,
                volume_mm3: i_vol.and_then(|i| f.get(i)).and_then(|v| v.parse().ok()),
                volume_sd_mm3: None,
                reference: i_ref.and_then(|i| f.get(i))
                    .filter(|v| !v.trim().is_empty()).map(|v| v.to_string()),
                reference_offset_ppm: i_off.and_then(|i| f.get(i)).and_then(|v| v.parse().ok()),
            })
        })
        .collect()
}

/// A quantitative map to summarise: how it is named in the table, and its physical unit.
pub struct MapSpec {
    /// The BIDS suffix (with any `desc-`), so a row points unambiguously at a file in `anat/`.
    pub name: &'static str,
    pub unit: &'static str,
    pub path: PathBuf,
}

/// Every quantitative map this run may have produced, in the order they belong in the table.
///
/// Maps that were not produced are dropped by the caller, so enabling more of the pipeline adds
/// rows rather than changing the shape of the table. SWI/SMWI and the minIPs are deliberately
/// absent: their intensities are arbitrary, so a mean over a structure means nothing.
pub fn candidate_maps(output: &DerivativeOutputs, key: &AcquisitionKey) -> Vec<MapSpec> {
    let spec = |name: &'static str, unit: &'static str, path: PathBuf| MapSpec { name, unit, path };
    vec![
        spec(CHIMAP, "ppm", output.qsm_path(key)),
        spec("desc-singlepass_Chimap", "ppm", output.singlepass_qsm_path(key)),
        spec(COSMOS_CHIMAP, "ppm", output.cosmos_path(key)),
        spec("desc-total_Chimap", "ppm", output.chi_sep_total_path(key)),
        spec("desc-paramagnetic_Chimap", "ppm", output.chi_para_path(key)),
        spec("desc-diamagnetic_Chimap", "ppm", output.chi_dia_path(key)),
        spec("T2starmap", "s", output.t2star_path(key)),
        spec("R2starmap", "s-1", output.r2star_path(key)),
        spec("R2map", "s-1", output.r2_path(key)),
        spec("R2primemap", "s-1", output.r2prime_path(key)),
    ]
}

/// Concatenate every run's table into one dataset-level TSV, each row labelled with the BIDS
/// entities of the run it came from.
///
/// The per-run tables stay where BIDS wants them, next to the images they describe; this is the
/// file a group analysis actually opens. Returns the path when anything was written — a run whose
/// analysis was skipped simply contributes no rows.
pub fn write_group_table(
    derivatives_dir: &Path, output: &DerivativeOutputs, keys: &[&AcquisitionKey],
) -> crate::Result<Option<PathBuf>> {
    let mut body = String::new();
    for key in keys {
        let path = output.segmentation_stats_path(key);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let entities = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t",
            key.subject,
            key.session.as_deref().unwrap_or(""),
            key.acquisition.as_deref().unwrap_or(""),
            key.reconstruction.as_deref().unwrap_or(""),
            key.inversion.as_deref().unwrap_or(""),
            key.run.as_deref().unwrap_or(""),
        );
        for line in text.lines().skip(1).filter(|l| !l.is_empty()) {
            body.push_str(&entities);
            body.push_str(line);
            body.push('\n');
        }
    }
    if body.is_empty() {
        return Ok(None);
    }
    let out = derivatives_dir.join("desc-segmentation_stats.tsv");
    std::fs::write(&out, format!("{GROUP_HEADER}{HEADER}{body}"))?;
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is the interchange format between the run and the figure, so it has to read back.
    #[test]
    fn a_written_table_parses_back() {
        let table = format!("{HEADER}\
            Chimap\tppm\t10\tleft thalamus\t36\t36\t1187.4\t0.012000\t0.030000\t\
            0.011000\t-0.050000\t0.080000\t-0.030000\t0.060000\t\t\n");
        let rows = parse_tsv(&table);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], Row {
            map: "Chimap".into(), unit: "ppm".into(), name: "left thalamus".into(),
            mean: 0.012, sd: 0.03, volume_mm3: Some(1187.4), volume_sd_mm3: None,
            reference: None, reference_offset_ppm: None,
        });

        // A group table prepends entity columns; the figure must read those too.
        let group = format!("subject\tsession\t{HEADER}01\t\tChimap\tppm\t10\t\
            left thalamus\t36\t36\t\t0.012000\t0.030000\t0.011000\t-0.05\t0.08\t-0.03\t0.06\t\t\n");
        // The group sample below has an empty volume column, which must read as absent.
        let group_rows = parse_tsv(&group);
        assert_eq!(group_rows.len(), 1);
        assert_eq!(group_rows[0].volume_mm3, None, "an empty volume is absent, not zero");
        assert_eq!(group_rows[0].name, rows[0].name, "columns are found by name, not position");

        // Junk is skipped, not fatal.
        assert!(parse_tsv("").is_empty());
        assert!(parse_tsv(&format!("{HEADER}not\ta\trow\n")).is_empty());
    }
}
