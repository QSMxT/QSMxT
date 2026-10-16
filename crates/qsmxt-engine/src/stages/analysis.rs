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

use crate::store::{Contrast, Item};

/// Columns of the per-run table, in order.
///
/// `n_voxels` counts the structure's voxels in the segmentation; `n_valid` counts those that
/// actually entered the statistics. They differ when a map does not cover the whole structure —
/// R2 only exists where the MESE acquisition reached, and every map is zero outside the brain
/// mask while the parcellation may spill slightly past it.
pub const HEADER: &str = "map\tunit\tindex\tname\tn_voxels\tn_valid\tvolume_mm3\t\
                          mean\tsd\tmedian\tmin\tmax\tp5\tp95\t\
                          reference\treference_offset_ppm\n";

/// The conventional susceptibility map, named as it is in the table.
pub const CHIMAP: &str = "Chimap";
/// A multi-orientation group's COSMOS map.
pub const COSMOS_CHIMAP: &str = "desc-cosmos_Chimap";

/// Every quantitative map of a run that is summarised, by its name in the table (its BIDS suffix,
/// with any `desc-`, so a row points unambiguously at a file) and its unit, in the order they
/// belong in the table.
///
/// The step drops the maps that were not produced, so enabling more of the pipeline adds rows
/// rather than changing the shape of the table. SWI/SMWI and the minIPs are deliberately absent:
/// their intensities are arbitrary, so a mean over a structure means nothing.
pub const MAPS: [(Item, &str, &str); 9] = [
    (Item::Chi, CHIMAP, "ppm"),
    (Item::SinglePassChi, "desc-singlepass_Chimap", "ppm"),
    (Item::SeparatedTotalChi, "desc-total_Chimap", "ppm"),
    (Item::SeparatedChi(Contrast::Paramagnetic), "desc-paramagnetic_Chimap", "ppm"),
    (Item::SeparatedChi(Contrast::Diamagnetic), "desc-diamagnetic_Chimap", "ppm"),
    (Item::T2star, "T2starmap", "s"),
    (Item::R2star, "R2starmap", "s-1"),
    (Item::R2, "R2map", "s-1"),
    (Item::R2prime, "R2primemap", "s-1"),
];

/// The `analysis` step's parameters: which maps were summarised (by `identity`, what names each
/// in the store) against which segmentation, so computing R2* on a re-run adds its rows rather
/// than leaving the table as it was.
pub fn params(
    version: crate::enums::SynthSegVersion, segmentation: serde_json::Value,
    maps: &[(&str, serde_json::Value)],
) -> serde_json::Value {
    serde_json::json!({
        "version": format!("{version}"),
        "dseg": segmentation,
        "maps": maps.iter().map(|(name, identity)| serde_json::json!([name, identity])).collect::<Vec<_>>(),
    })
}

/// What a run's susceptibility map was referenced to.
///
/// Carried onto every susceptibility row so the table stands on its own: a spreadsheet of χ values
/// whose zero is undocumented cannot be compared with anyone else's, and the reader should not
/// have to find a sidecar to learn what it was.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceInfo {
    pub spec: String,
    pub offset_ppm: Option<f64>,
}

impl ReferenceInfo {
    /// Read it from what referencing recorded about a map ([`crate::referencing::sidecar_fields`]).
    pub fn from_fields(fields: &serde_json::Map<String, serde_json::Value>) -> Option<Self> {
        Some(Self {
            spec: fields.get("QsmReference")?.as_str()?.to_string(),
            offset_ppm: fields.get("QsmReferenceOffsetPpm").and_then(|x| x.as_f64()),
        })
    }

    /// How a figure says it.
    pub fn caption(&self) -> String {
        match self.offset_ppm {
            Some(o) => format!("Referenced to {} (offset {o:.4} ppm)", self.spec),
            None => format!("Referenced to {}", self.spec),
        }
    }
}

/// Whether a map is a susceptibility map, and so carries a reference.
///
/// R2* has no reference to speak of, and stamping one on its rows would suggest it did.
pub fn is_susceptibility(map: &str) -> bool {
    map == CHIMAP || map.ends_with("_Chimap")
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

/// The rows summarising one map (`name`, in `unit`) over the segmentation, in label-table order.
///
/// `volumes` is SynthSeg's own partial-volume-aware figure — the sum of the soft posteriors, which
/// is not the same as counting labelled voxels, so `n_voxels` is reported alongside it rather than
/// in its place. A supplied segmentation has no posteriors, so its volume is left empty rather
/// than guessed at from the voxel count.
pub fn map_rows(
    name: &str,
    unit: &str,
    map: &[f64],
    seg: &[f64],
    labels: &qsm_core::segment::SynthSegLabels,
    volumes: Option<&[f64]>,
    reference: Option<&ReferenceInfo>,
) -> String {
    // Only susceptibility rows carry it; on an R2* row it would be a claim about nothing.
    let (ref_spec, ref_off) = match reference.filter(|_| is_susceptibility(name)) {
        Some(r) => (r.spec.clone(), r.offset_ppm.map(|o| format!("{o:.6}")).unwrap_or_default()),
        None => (String::new(), String::new()),
    };
    let by_label = values_by_label(map, seg);
    let counts = voxels_by_label(seg);
    let mut rows = String::new();
    for (ch, (&id, label)) in labels.ids.iter().zip(labels.names).enumerate() {
        if id == 0 {
            continue;
        }
        let Some(vals) = by_label.get(&id) else { continue };
        let mut vals = vals.clone();
        let st = structure_stats(&mut vals);
        let vol = volumes.and_then(|v| v.get(ch)).map(|v| format!("{v:.1}")).unwrap_or_default();
        rows.push_str(&format!(
            "{name}\t{unit}\t{id}\t{label}\t{}\t{}\t{vol}\t\
             {:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{ref_spec}\t{ref_off}\n",
            counts.get(&id).copied().unwrap_or(0), vals.len(),
            st.mean, st.sd, st.median, st.min, st.max, st.p5, st.p95,
        ));
    }
    rows
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

    /// Volumes arrive as a bare array indexed by the network's output channel and are matched to
    /// labels by position. A misalignment there would give every structure a plausible volume
    /// belonging to a different one — invisible unless each is checked against its own label.
    #[test]
    fn each_label_gets_its_own_volume() {
        let labels = qsm_core::segment::SynthSegVersion::V2.labels();
        // One voxel of every label, so every row is present.
        let seg: Vec<f64> = labels.ids.iter().map(|&i| i as f64).collect();
        let map = vec![1.0; seg.len()];
        // A distinct volume per channel, derived from the channel index.
        let volumes: Vec<f64> = (0..labels.ids.len()).map(|c| 1000.0 + c as f64).collect();

        let rows = map_rows(CHIMAP, "ppm", &map, &seg, labels, Some(&volumes), None);

        let mut seen = 0;
        for line in rows.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let id: i32 = f[2].parse().unwrap();
            let ch = labels.ids.iter().position(|&i| i == id).unwrap();
            assert_eq!(f[6], format!("{:.1}", volumes[ch]),
                       "label {id} ({}) got channel {}'s volume", f[3], f[6]);
            seen += 1;
        }
        assert_eq!(seen, labels.ids.len() - 1, "every label but the background gets a row");

        // And volumes really do differ per structure — a constant column would pass the check
        // above while telling the reader nothing.
        let vols: std::collections::HashSet<&str> =
            rows.lines().map(|l| l.split('\t').nth(6).unwrap()).collect();
        assert_eq!(vols.len(), seen, "each structure should carry its own volume");
    }

    /// A row has to carry its map's identity and unit, or a long table cannot be pivoted back.
    #[test]
    fn rows_name_their_map_and_unit() {
        let labels = qsm_core::segment::SynthSegVersion::V2.labels();
        let id = labels.ids.iter().copied().find(|&i| i != 0).expect("a non-background label");
        let seg = vec![id as f64; 4];
        let map = vec![0.1, 0.2, 0.3, 0.4];
        let rows = map_rows("R2starmap", "s-1", &map, &seg, labels, None, None);
        let first = rows.lines().next().expect("one row per present label");
        let cols: Vec<&str> = first.split('\t').collect();
        assert_eq!(cols.len(), HEADER.trim_end().split('\t').count(),
                   "row width must match the header");
        assert_eq!((cols[0], cols[1]), ("R2starmap", "s-1"));
        assert_eq!(cols[6], "", "volume is left empty when there are no posteriors");
        assert_eq!((cols[14], cols[15]), ("", ""), "R2* has no reference to claim");

        // A susceptibility row does carry it, on every row, so the table stands on its own.
        let r = ReferenceInfo { spec: "thalamus".into(), offset_ppm: Some(0.0125) };
        let rows = map_rows(CHIMAP, "ppm", &map, &seg, labels, None, Some(&r));
        let cols: Vec<&str> = rows.lines().next().unwrap().split('\t').collect();
        assert_eq!(cols[14], "thalamus");
        assert_eq!(cols[15], "0.012500");
        assert_eq!(r.caption(), "Referenced to thalamus (offset 0.0125 ppm)");
    }
}
