//! Referencing a susceptibility map to a parcellation region.
//!
//! `qsm_core::pipeline::apply_reference` covers the brain-mask mean and no referencing at all. It
//! cannot do this one: it zeroes everything outside the mask it is handed, so passing the region
//! as that mask would subtract the right number and then throw away the rest of the brain. The
//! region is a place to *measure* the offset, not the extent to keep.

use crate::error::QsmxtError;

/// Subtract the region's mean susceptibility from the whole brain.
///
/// `roi` selects where the offset is measured; `brain` selects what is kept. Voxels outside the
/// brain mask stay zero, exactly as the mean and none paths leave them.
///
/// Only voxels inside *both* masks count toward the offset: a parcellation is computed on the
/// magnitude and can spill past the brain mask the reconstruction used, and a structure's voxels
/// outside that mask hold no susceptibility to average.
///
/// Fails rather than falling back when the region is empty. A run that quietly reverts to a
/// different reference produces a map that looks fine and cannot be compared with its cohort —
/// the whole reason for referencing to tissue in the first place.
pub fn reference_to_region(
    chi: &[f64], brain: &[u8], roi: &[u8], region: &str,
) -> crate::Result<(Vec<f64>, Offset)> {
    let mut sum = 0.0;
    let mut count = 0usize;
    for ((&c, &b), &r) in chi.iter().zip(brain).zip(roi) {
        if b > 0 && r > 0 && c.is_finite() {
            sum += c;
            count += 1;
        }
    }
    if count == 0 {
        return Err(QsmxtError::Config(format!(
            "QSM reference region `{region}` has no voxels inside the brain mask, so there is \
             nothing to reference to. The parcellation may have missed the structure, or it may \
             lie outside the mask. Check the segmentation, pick another region, or use \
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

    /// The mean reference's offset has to be recoverable too, on the same terms as a region's.
    #[test]
    fn the_mask_mean_offset_is_reported() {
        assert_eq!(mask_mean(&[1.0, 3.0, 99.0], &[1, 1, 0]), Some(Offset { ppm: 2.0, voxels: 2 }));
        assert_eq!(mask_mean(&[1.0, f64::NAN], &[1, 1]), Some(Offset { ppm: 1.0, voxels: 1 }),
                   "non-finite voxels must not poison the offset");
        assert_eq!(mask_mean(&[1.0, 2.0], &[0, 0]), None, "an empty mask has no mean");
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
