//! The Reference section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("qsm.reference", Reference, "QSM reference", "", Enum(vec![
            o("mean", "mean", "Mean", "Subtract the mean susceptibility over the voxels the map is defined on (recommended)"),
            o("none", "none", "None", "No referencing (raw susceptibility values)"),
            o("region", "region", "Region", "Reference χ to the mean of a SynthSeg region (Reference region). Requires Segmentation"),
        ])).basic().flag("--qsm-reference")
        .gui("What the susceptibility map is referenced to: the value that reads as zero"),
        d("qsm.reference_region", Reference, "Reference region", "The SynthSeg region whose mean susceptibility is zero. Requires Segmentation", Regions).basic().optional().flag("--qsm-reference").parent("qsm.reference")
        .when(|x| x.c.qsm.reference == QsmReference::Region),
    ]
}
