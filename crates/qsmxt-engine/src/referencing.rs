//! Referencing a susceptibility map to a parcellation region.
//!
//! `qsm_core::pipeline::apply_reference` covers the brain-mask mean and no referencing at all. It
//! cannot do this one: it zeroes everything outside the mask it is handed, so passing the region
//! as that mask would subtract the right number and then throw away the rest of the brain. The
//! region is a place to *measure* the offset, not the extent to keep.
//!
//! Every reference is measured and applied over the map's *support* — the voxels the
//! reconstruction actually defined — not over the brain mask. Background-field removal erodes
//! the mask (V-SHARP by its kernel radius, TGV by its erosions), and the raw map is 0 on that
//! eroded rim. Averaging over the brain mask pulls the offset toward zero by the rim's share of
//! the mask, and subtracting it there turns the rim into a shell of constant `-offset`. See
//! [`defined_support`].

use crate::config::PipelineConfig;
use crate::enums::QsmReference;
use crate::error::EngineError;

/// The reference step's parameters.
///
/// `domain` versions what the reference is measured over, so maps referenced over the brain mask
/// by an older release are re-referenced rather than reused from the cache. A region reference
/// depends on the parcellation as well as the map. Rather than a static `segmentation ->
/// reference` edge — which would invalidate referencing, and everything that reads the referenced
/// map, whenever any SynthSeg setting changed on a `mean` run — the dependency is carried here,
/// only when it is real: the region, its label ids, and `segmentation`, the cache key of the
/// segmentation that measured it.
pub fn params(config: &PipelineConfig, segmentation: Option<&str>) -> Result<serde_json::Value, EngineError> {
    let mut params = serde_json::json!({ "method": format!("{}", config.qsm.reference), "domain": "support" });
    if let Some((spec, ids)) = reference_region(config)? {
        params["region"] = serde_json::json!(spec);
        params["region_ids"] = serde_json::json!(ids);
        params["segmentation"] = serde_json::json!(segmentation);
    }
    Ok(params)
}

/// The region a `--qsm-reference <region>` measures the offset on, as written and as label ids;
/// `None` for the other references.
pub fn reference_region(config: &PipelineConfig) -> Result<Option<(String, Vec<i32>)>, EngineError> {
    if config.qsm.reference != QsmReference::Region {
        return Ok(None);
    }
    let spec = config.qsm.reference_region.clone().unwrap_or_default();
    let ids = crate::regions::resolve(&spec, config.segmentation.version)
        .map_err(|e| EngineError::Config(format!("--qsm-reference: {e}")))?;
    Ok(Some((spec, ids)))
}

/// A susceptibility map after referencing: the map, the support it was referenced over, and what
/// the reference removed (when that is a single number worth recording).
pub struct Referenced {
    pub chi: Vec<f64>,
    pub support: Vec<u8>,
    pub offset: Option<Offset>,
}

/// Reference `chi` with `--qsm-reference`, over the voxels it is defined on within `declared`
/// ([`defined_support`]); the map comes back 0 outside that support.
///
/// Used for every susceptibility map QSMxT publishes — each run's and the multi-orientation
/// (COSMOS) map — so all are referenced the same way. `dseg` must be on `chi`'s grid; it is needed
/// only for a region reference.
pub fn reference_chi(
    config: &PipelineConfig, chi: &[f64], declared: &[&[u8]], dseg: Option<&[f64]>,
) -> Result<Referenced, EngineError> {
    if let Some(m) = declared.iter().find(|m| m.len() != chi.len()) {
        return Err(EngineError::DimensionMismatch(format!(
            "support mask has {} voxels but the susceptibility map has {}", m.len(), chi.len())));
    }
    let support = defined_support(declared, chi);
    log::info!(
        "Referencing over the map's support: {} voxels",
        support.iter().filter(|&&m| m != 0).count(),
    );
    if let Some((spec, ids)) = reference_region(config)? {
        let dseg = dseg.ok_or_else(|| EngineError::Config(format!(
            "--qsm-reference {spec}: no segmentation to measure the region on")))?;
        if dseg.len() != chi.len() {
            return Err(EngineError::DimensionMismatch(format!(
                "segmentation has {} voxels but the susceptibility map has {}",
                dseg.len(), chi.len())));
        }
        let roi = region_mask(dseg, &ids);
        let (out, off) = reference_to_region(chi, &support, &roi, &spec)?;
        return Ok(Referenced { chi: out, support, offset: Some(off) });
    }
    let (_, _, _, ref_method_core) = crate::bridge::to_pipeline_stages(config);
    // What the mean reference removed is worth recording for the same reason a region's offset
    // is; `apply_reference` applies it without saying.
    let offset = (config.qsm.reference == QsmReference::Mean)
        .then(|| mask_mean(chi, &support))
        .flatten();
    let out = qsm_core::pipeline::apply_reference(chi, &support, ref_method_core);
    Ok(Referenced { chi: out, support, offset })
}

/// The `QsmReference*` sidecar fields for a map referenced under `config`: the reference, and what
/// it removed and from how many voxels, when that is known.
pub fn sidecar_fields(
    config: &PipelineConfig, offset: Option<&Offset>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut obj = serde_json::Map::new();
    obj.insert("QsmReference".into(), serde_json::json!(config.qsm.reference_spec()));
    if let Some(off) = offset {
        obj.insert("QsmReferenceOffsetPpm".into(), serde_json::json!(off.ppm));
        obj.insert("QsmReferenceVoxels".into(), serde_json::json!(off.voxels));
    }
    if config.qsm.reference == QsmReference::Region {
        let spec = config.qsm.reference_region.clone().unwrap_or_default();
        if let Ok(ids) = crate::regions::resolve(&spec, config.segmentation.version) {
            obj.insert("QsmReferenceLabels".into(), serde_json::json!(ids));
        }
    }
    obj
}

/// The voxels a raw susceptibility map is defined on.
///
/// `declared` are the masks the reconstruction was run on — the background-removal mask, or the
/// brain mask when the inversion removed the background itself; two for a two-pass map, whose
/// support is their union. A voxel inside them that the reconstruction still left at exactly 0
/// (or non-finite) is excluded too: several inversions erode beyond the mask they are handed
/// (TGV by its `erosions`), and an exact 0.0 inside a reconstruction is masking, not a
/// measurement. This is the same rule the two-pass combination uses to decide where the
/// reliable pass produced a value.
pub fn defined_support(declared: &[&[u8]], chi: &[f64]) -> Vec<u8> {
    chi.iter().enumerate()
        .map(|(i, &c)| u8::from(c != 0.0 && c.is_finite() && declared.iter().any(|m| m[i] != 0)))
        .collect()
}

/// Subtract the region's mean susceptibility from the map's support.
///
/// `roi` selects where the offset is measured; `brain` — the map's support, see
/// [`defined_support`] — selects what is kept. Voxels outside it are written as zero, exactly as
/// the mean and none paths leave them.
///
/// Only voxels inside *both* masks count toward the offset: a parcellation is computed on the
/// magnitude and can spill past the region the reconstruction defined, and a structure's voxels
/// outside it hold no susceptibility to average.
///
/// Fails rather than falling back when the region is empty. A run that quietly reverts to a
/// different reference produces a map that looks fine and cannot be compared with its cohort —
/// the whole reason for referencing to tissue in the first place.
pub fn reference_to_region(
    chi: &[f64], brain: &[u8], roi: &[u8], region: &str,
) -> Result<(Vec<f64>, Offset), EngineError> {
    let mut sum = 0.0;
    let mut count = 0usize;
    for ((&c, &b), &r) in chi.iter().zip(brain).zip(roi) {
        if b > 0 && r > 0 && c.is_finite() {
            sum += c;
            count += 1;
        }
    }
    if count == 0 {
        return Err(EngineError::Config(format!(
            "QSM reference region `{region}` has no voxels where the susceptibility map is \
             defined, so there is nothing to reference to. The parcellation may have missed the \
             structure, or it may lie outside the brain mask or in the rim background-field \
             removal eroded. Check the segmentation, pick another region, or use \
             --qsm-reference mean"
        )));
    }
    if count < MIN_RELIABLE_VOXELS {
        log::warn!(
            "QSM reference region `{region}` covers only {count} voxels — the offset it gives is \
             noisy, and the map's zero with it"
        );
    }
    let offset = sum / count as f64;
    log::info!("Referencing χ to `{region}`: {count} voxels, offset {offset:.6} ppm");
    let out = chi.iter().zip(brain)
        .map(|(&c, &b)| if b > 0 { c - offset } else { 0.0 })
        .collect();
    Ok((out, Offset { ppm: offset, voxels: count }))
}

/// What was subtracted, and from how many voxels it was measured.
///
/// Reported per subject because it is a measurement in its own right — the susceptibility of the
/// reference tissue — and because it is what tells you whether a subject's reference was sound: an
/// offset far from its cohort, or measured on a handful of voxels, means a parcellation that went
/// wrong, not a brain that differs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offset {
    pub ppm: f64,
    pub voxels: usize,
}

/// The offset the brain-mask mean reference subtracts.
///
/// `qsm_core` applies that reference without saying what it removed, and the figure it removed is
/// worth keeping for the same reason a region offset is.
pub fn mask_mean(chi: &[f64], brain: &[u8]) -> Option<Offset> {
    let vals: Vec<f64> = chi.iter().zip(brain)
        .filter(|(c, &b)| b > 0 && c.is_finite())
        .map(|(&c, _)| c)
        .collect();
    (!vals.is_empty()).then(|| Offset {
        ppm: vals.iter().sum::<f64>() / vals.len() as f64,
        voxels: vals.len(),
    })
}

/// Below this many voxels, the region's mean is dominated by noise rather than by the tissue.
/// Picked as the order of magnitude at which a structure's mean stops being stable across
/// reasonable segmentations — a warning, not a limit, since a small ROI can still be deliberate.
const MIN_RELIABLE_VOXELS: usize = 50;

/// Voxels of the parcellation carrying any of `ids`.
pub fn region_mask(dseg: &[f64], ids: &[i32]) -> Vec<u8> {
    dseg.iter()
        .map(|&v| u8::from(ids.contains(&(v.round() as i32))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The offset comes from the region; the map that comes back is the whole brain.
    #[test]
    fn the_region_sets_the_offset_and_the_brain_is_kept() {
        let chi = vec![1.0, 3.0, 10.0, 20.0];
        let brain = vec![1, 1, 1, 0];
        let roi = vec![1, 1, 0, 0];
        let (out, off) = reference_to_region(&chi, &brain, &roi, "test").unwrap();
        // Offset is mean(1, 3) = 2, subtracted everywhere inside the brain mask.
        assert_eq!(out, vec![-1.0, 1.0, 8.0, 0.0]);
        assert_eq!(off, Offset { ppm: 2.0, voxels: 2 }, "the offset is reported, not just applied");
        // The region's own mean is zero afterwards — that is what referencing to it means.
        assert!((out[0] + out[1]).abs() < 1e-12);
    }

    /// A region that spills outside the brain mask must not average in voxels the reconstruction
    /// never wrote, or the offset is pulled toward zero by however far it spilled.
    #[test]
    fn voxels_outside_the_brain_mask_do_not_set_the_offset() {
        let chi = vec![4.0, 4.0, 0.0, 0.0];
        let brain = vec![1, 1, 0, 0];
        let roi = vec![1, 1, 1, 1];
        let (out, off) = reference_to_region(&chi, &brain, &roi, "test").unwrap();
        assert_eq!(out, vec![0.0, 0.0, 0.0, 0.0], "offset should be 4.0, not 2.0");
        assert_eq!(off.ppm, 4.0);
        assert_eq!(off.voxels, 2, "only the voxels inside both masks count");
    }

    /// An empty region fails loudly rather than silently referencing to something else.
    #[test]
    fn an_empty_region_is_an_error() {
        let err = reference_to_region(&[1.0, 2.0], &[1, 1], &[0, 0], "left-thalamus").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("left-thalamus"), "{msg}");
        assert!(msg.contains("no voxels"), "{msg}");

        // A region that exists only outside the brain mask is just as empty.
        assert!(reference_to_region(&[1.0, 2.0], &[1, 0], &[0, 1], "x").is_err());
    }

    /// The support is where the reconstruction wrote a value: inside a declared mask, and not
    /// left at exactly 0 by an inversion that eroded further than the mask it was handed.
    #[test]
    fn the_support_excludes_eroded_and_unwritten_voxels() {
        let chi = [0.1, 0.0, 0.2, f64::NAN, 0.3, 0.4];
        let bg = [1, 1, 1, 1, 0, 0];
        assert_eq!(defined_support(&[&bg], &chi), vec![1, 0, 1, 0, 0, 0]);
    }

    /// A two-pass map takes each voxel from one pass or the other, so its support is the union of
    /// both passes' supports.
    #[test]
    fn a_two_pass_support_is_the_union_of_both_passes() {
        let chi = [0.1, 0.2, 0.3, 0.4];
        let main = [1, 1, 0, 0];
        let reliable = [0, 1, 1, 0];
        assert_eq!(defined_support(&[&main, &reliable], &chi), vec![1, 1, 1, 0]);
    }

    /// The mean reference's offset has to be recoverable too, on the same terms as a region's.
    #[test]
    fn the_mask_mean_offset_is_reported() {
        assert_eq!(mask_mean(&[1.0, 3.0, 99.0], &[1, 1, 0]), Some(Offset { ppm: 2.0, voxels: 2 }));
        assert_eq!(mask_mean(&[1.0, f64::NAN], &[1, 1]), Some(Offset { ppm: 1.0, voxels: 1 }),
                   "non-finite voxels must not poison the offset");
        assert_eq!(mask_mean(&[1.0, 2.0], &[0, 0]), None, "an empty mask has no mean");
    }

    /// The reference's cache key names the region only for a region reference, and the
    /// segmentation it was measured on with it.
    #[test]
    fn only_a_region_reference_keys_on_the_segmentation() {
        let mut config = PipelineConfig::default();
        let mean = params(&config, Some("abc")).unwrap();
        assert_eq!(mean["domain"], "support");
        assert!(mean.get("segmentation").is_none(), "a mean reference reads no parcellation");

        config.qsm.reference = QsmReference::Region;
        config.qsm.reference_region = Some("thalamus".into());
        let region = params(&config, Some("abc")).unwrap();
        assert_eq!(region["segmentation"], "abc");
        assert!(!region["region_ids"].as_array().unwrap().is_empty());

        config.qsm.reference_region = Some("no-such-structure".into());
        assert!(format!("{}", params(&config, None).unwrap_err()).starts_with("--qsm-reference:"));
    }

    /// A map and a support of different sizes are refused as the dimension error they are.
    #[test]
    fn a_mis_sized_support_is_a_dimension_error() {
        let config = PipelineConfig::default();
        let err = reference_chi(&config, &[1.0, 2.0], &[&[1u8]], None).err().unwrap();
        assert!(matches!(err, EngineError::DimensionMismatch(_)), "{err}");
        let ok = reference_chi(&config, &[1.0, 3.0, 0.0], &[&[1u8, 1, 1]], None).unwrap();
        assert_eq!(ok.chi, vec![-1.0, 1.0, 0.0]);
        assert_eq!(ok.offset, Some(Offset { ppm: 2.0, voxels: 2 }));
    }

    /// The mask is built from the label ids, and a label not asked for is not in it.
    #[test]
    fn region_mask_selects_exactly_the_named_labels() {
        let dseg = vec![0.0, 10.0, 49.0, 11.0, 10.0];
        assert_eq!(region_mask(&dseg, &[10, 49]), vec![0, 1, 1, 0, 1]);
        assert_eq!(region_mask(&dseg, &[11]), vec![0, 0, 0, 1, 0]);
        assert_eq!(region_mask(&dseg, &[]), vec![0, 0, 0, 0, 0]);
    }
}
