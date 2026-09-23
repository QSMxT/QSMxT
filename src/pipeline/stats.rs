//! Per-structure statistics over a segmentation.
//!
//! Every quantitative map a run produced is summarised inside each segmented structure, so the
//! parcellation turns into a table that can be opened in a spreadsheet or read by a stats package
//! without anyone writing a script to pull numbers out of NIfTIs.
//!
//! The table is long (tidy), not wide: one row per (map, structure). A run produces a different
//! set of maps depending on what was enabled and what data existed — a wide table would have to
//! grow a column per map, and every consumer would have to cope with the columns changing between
//! datasets. Long stays the same shape, and pivots to wide in one line of pandas or R.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::entities::AcquisitionKey;

/// Columns of the per-run table, in order.
///
/// `n_voxels` counts the structure's voxels in the segmentation; `n_valid` counts those that
/// actually entered the statistics. They differ when a map does not cover the whole structure —
/// R2 only exists where the MESE acquisition reached, and every map is zero outside the brain
/// mask while the parcellation may spill slightly past it.
pub const HEADER: &str = "map\tunit\tindex\tname\tn_voxels\tn_valid\tvolume_mm3\t\
                          mean\tsd\tmedian\tmin\tmax\tp5\tp95\n";

/// BIDS entities identifying the run a row came from, prepended to every row of the group table.
const GROUP_HEADER: &str = "subject\tsession\tacquisition\treconstruction\tinversion\trun\t";

/// The conventional susceptibility map, named as it is in the table.
pub const CHIMAP: &str = "Chimap";

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
        spec("desc-total_Chimap", "ppm", output.chi_sep_total_path(key)),
        spec("desc-paramagnetic_Chimap", "ppm", output.chi_para_path(key)),
        spec("desc-diamagnetic_Chimap", "ppm", output.chi_dia_path(key)),
        spec("T2starmap", "s", output.t2star_path(key)),
        spec("R2starmap", "s-1", output.r2star_path(key)),
        spec("R2map", "s-1", output.r2_path(key)),
        spec("R2primemap", "s-1", output.r2prime_path(key)),
    ]
}

/// Summary statistics for one structure's values in one map.
pub struct StructureStats {
    pub mean: f64,
    pub sd: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub p5: f64,
    pub p95: f64,
}

/// Summarise one structure's voxels. Sorts `vals` in place.
///
/// The SD is the population one: these are every voxel of the structure, not a sample drawn from
/// it, so there is no degree of freedom to lose. Percentiles use nearest-rank on the sorted values
/// rather than interpolating — an interpolated percentile invents a value no voxel had. Min and
/// max come free from the sort, and are worth reporting next to the percentiles: they say whether
/// the tail the percentiles cut off was a whisker or a cliff.
pub fn structure_stats(vals: &mut [f64]) -> StructureStats {
    assert!(!vals.is_empty(), "structure_stats needs at least one value");
    vals.sort_by(|a, b| a.partial_cmp(b).expect("non-finite values are filtered out"));
    let n = vals.len();
    let mean = vals.iter().sum::<f64>() / n as f64;
    let sd = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
    let pct = |q: f64| vals[(((n - 1) as f64) * q).round() as usize];
    StructureStats {
        mean, sd,
        median: pct(0.5),
        min: vals[0],
        max: vals[n - 1],
        p5: pct(0.05),
        p95: pct(0.95),
    }
}

/// Voxels of `map` grouped by the segmentation label they fall under, keeping only the values that
/// can be summarised.
///
/// Zero is dropped along with the non-finite values: every map is written zero where it has
/// nothing to say (outside the brain mask, or where a fit failed), and a structure that overlaps
/// such a region would otherwise have its mean pulled toward zero by voxels that were never
/// measured. A genuinely zero susceptibility is not distinguishable from those, but an exact
/// floating-point zero in a reconstructed map is the sentinel far more often than the measurement.
///
/// One pass over the volume, bucketing as it goes: scanning once per label instead would be a
/// hundred passes over several million voxels for each map.
fn values_by_label(map: &[f64], seg: &[f64]) -> HashMap<i32, Vec<f64>> {
    let mut by_label: HashMap<i32, Vec<f64>> = HashMap::new();
    for (&v, &l) in map.iter().zip(seg) {
        let id = l.round() as i32;
        if id == 0 {
            continue; // background
        }
        if v.is_finite() && v != 0.0 {
            by_label.entry(id).or_default().push(v);
        }
    }
    by_label
}

/// Voxel count per label in the segmentation, background excluded.
fn voxels_by_label(seg: &[f64]) -> HashMap<i32, usize> {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &l in seg {
        let id = l.round() as i32;
        if id != 0 {
            *counts.entry(id).or_default() += 1;
        }
    }
    counts
}

/// The rows summarising one map over the segmentation, in label-table order.
///
/// `volumes` is SynthSeg's own partial-volume-aware figure — the sum of the soft posteriors, which
/// is not the same as counting labelled voxels, so `n_voxels` is reported alongside it rather than
/// in its place. A supplied segmentation has no posteriors, so its volume is left empty rather
/// than guessed at from the voxel count.
pub fn map_rows(
    spec: &MapSpec,
    map: &[f64],
    seg: &[f64],
    labels: &qsm_core::segment::SynthSegLabels,
    volumes: Option<&[f64]>,
) -> String {
    let by_label = values_by_label(map, seg);
    let counts = voxels_by_label(seg);
    let mut rows = String::new();
    for (ch, (&id, name)) in labels.ids.iter().zip(labels.names).enumerate() {
        if id == 0 {
            continue;
        }
        let Some(vals) = by_label.get(&id) else { continue };
        let mut vals = vals.clone();
        let st = structure_stats(&mut vals);
        let vol = volumes.and_then(|v| v.get(ch)).map(|v| format!("{v:.1}")).unwrap_or_default();
        rows.push_str(&format!(
            "{}\t{}\t{id}\t{name}\t{}\t{}\t{vol}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\n",
            spec.name, spec.unit,
            counts.get(&id).copied().unwrap_or(0), vals.len(),
            st.mean, st.sd, st.median, st.min, st.max, st.p5, st.p95,
        ));
    }
    rows
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

    /// Percentile indexing is easy to get subtly wrong, and a wrong p5/p95 looks entirely
    /// plausible in a results table.
    #[test]
    fn structure_stats_summarise_a_structure() {
        // 0..=100: every statistic has an exact expected value.
        let mut vals: Vec<f64> = (0..=100).map(|i| i as f64).collect();
        let st = structure_stats(&mut vals);
        assert_eq!(st.median, 50.0);
        assert_eq!(st.mean, 50.0);
        assert_eq!(st.p5, 5.0);
        assert_eq!(st.p95, 95.0);
        assert_eq!((st.min, st.max), (0.0, 100.0));

        // Population SD of 0..=100 is sqrt((n²-1)/12) = sqrt(850).
        assert!((st.sd - 850f64.sqrt()).abs() < 1e-9, "sd was {}", st.sd);

        // Unsorted input is sorted in place, and the answer does not depend on the order.
        let mut shuffled = vec![95.0, 5.0, 50.0, 0.0, 100.0];
        let a = structure_stats(&mut shuffled);
        let mut sorted = vec![0.0, 5.0, 50.0, 95.0, 100.0];
        let b = structure_stats(&mut sorted);
        assert_eq!((a.median, a.p5, a.p95, a.min, a.max), (b.median, b.p5, b.p95, b.min, b.max));

        // A single voxel: every statistic is that voxel, and the SD is zero rather than NaN.
        let st = structure_stats(&mut [0.25]);
        assert_eq!((st.median, st.mean, st.p5, st.p95, st.min, st.max),
                   (0.25, 0.25, 0.25, 0.25, 0.25, 0.25));
        assert_eq!(st.sd, 0.0);

        // Percentiles are real observations, never interpolated between two voxels.
        let mut two = vec![0.0, 1.0];
        let st = structure_stats(&mut two);
        assert!(st.median == 0.0 || st.median == 1.0, "median was interpolated: {}", st.median);
    }

    /// Uncovered voxels must not be summarised: a structure the map only half covers should report
    /// the covered half, and say so in `n_valid` rather than silently averaging in zeros.
    #[test]
    fn uncovered_voxels_are_excluded_but_still_counted() {
        let seg = vec![0.0, 3.0, 3.0, 3.0, 3.0];
        let map = vec![9.9, 1.0, 3.0, 0.0, f64::NAN];
        let by_label = values_by_label(&map, &seg);
        assert_eq!(by_label[&3], vec![1.0, 3.0], "zero and NaN voxels must not be summarised");
        assert!(!by_label.contains_key(&0), "background is never a structure");
        assert_eq!(voxels_by_label(&seg)[&3], 4, "n_voxels counts the label, not the valid values");
    }

    /// A row has to carry its map's identity and unit, or a long table cannot be pivoted back.
    #[test]
    fn rows_name_their_map_and_unit() {
        let labels = qsm_core::segment::SynthSegVersion::V2.labels();
        let id = labels.ids.iter().copied().find(|&i| i != 0).expect("a non-background label");
        let seg = vec![id as f64; 4];
        let map = vec![0.1, 0.2, 0.3, 0.4];
        let spec = MapSpec { name: "R2starmap", unit: "s-1", path: PathBuf::new() };
        let rows = map_rows(&spec, &map, &seg, labels, None);
        let first = rows.lines().next().expect("one row per present label");
        let cols: Vec<&str> = first.split('\t').collect();
        assert_eq!(cols.len(), HEADER.trim_end().split('\t').count(),
                   "row width must match the header");
        assert_eq!((cols[0], cols[1]), ("R2starmap", "s-1"));
        assert_eq!(cols[6], "", "volume is left empty when there are no posteriors");
    }
}
