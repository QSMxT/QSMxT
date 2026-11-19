//! The Reference section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("qsm.reference", Reference, "QSM reference", "", Enum(vec![
            o("mean", "mean", "Mean", "Subtract the mean susceptibility over the voxels the map is defined on (recommended)"),
            o("none", "none", "None", "No referencing (raw susceptibility values)"),
            o("region", "region", "Region", "Reference χ to the mean of a SynthSeg region (Reference region). Requires Segmentation"),
        ])).basic().flag("--qsm-reference")
        .gui("What the susceptibility map is referenced to: the value that reads as zero")
        .cli("What the susceptibility map's zero is pinned to: `mean` (the mean over the voxels the map is defined on — the brain mask less the rim background-field removal eroded; the default), `none`, or a SynthSeg region — a structure with both sides merged (`thalamus`), one side (`left-thalamus`), the `ventricles` composite, a raw FreeSurfer id (`4`), or a comma-separated list of any of those. A region requires --do-segmentation (turned on automatically, and logged). An unrecognised name lists the valid ones")
        .parse(P::Text).value_name("SPEC"),
        d("qsm.reference_region", Reference, "Reference region", "The SynthSeg region whose mean susceptibility is zero. Requires Segmentation", Regions).basic().optional().flag("--qsm-reference").parent("qsm.reference")
        .when(|x| x.c.qsm.reference == QsmReference::Region),
    ]
}
