//! The Segmentation section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.do_segmentation", Segmentation, "Segmentation", "Whole-brain parcellation: SynthSeg straight off the GRE magnitude, or one supplied in derivatives. Required by the statistics and a region reference: choosing one offers to turn it on (left off, the run turns it on itself)", Bool).basic().flag("--do-segmentation")
        .cli("Also segment the brain from the GRE magnitude with SynthSeg, writing a FreeSurfer-labelled `_dseg.nii` and its BIDS lookup table. Needs no T1w and no registration. Needs a deep-learning build. doi:10.1016/j.media.2023.102789"),
        d("segmentation.custom_dseg_tool", Segmentation, "Derivatives tool", "The dseg from <bids>/derivatives/<tool>/: * = any tool, or a tool name; SynthSeg runs where none is found", Text).basic().optional().flag("--use-custom-dseg").parent("pipeline.do_segmentation")
        .if_omitted("SynthSeg runs")
        .supplied_file(Item::SuppliedSegmentation, "A segmentation to use instead of running SynthSeg")
        .when(|x| x.segmentation_on).full("Custom segmentation")
        .gui("Use a segmentation (dseg) from <bids>/derivatives/<tool>/ instead of running SynthSeg: * for any tool, or a tool name. SynthSeg runs where none is found")
        .cli("Prefer a bring-your-own segmentation from <bids>/derivatives/<TOOL>/ when present. Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg")
        .default_missing("*").value_name("TOOL"),
        d("segmentation.version", Segmentation, "SynthSeg version", "", Enum(vec![
            o("v1", "v1", "SynthSeg 1", "v1: SynthSeg 1.0, 32 labels (the default). v2: SynthSeg 2.0, 33 labels, adds a general CSF class (Billot et al., PNAS 2023)"),
            o("v2", "v2", "SynthSeg 2", "v1: SynthSeg 1.0, 32 labels (the default). v2: SynthSeg 2.0, 33 labels, adds a general CSF class (Billot et al., PNAS 2023)"),
        ])).basic().flag("--synthseg-version").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .gui("Which SynthSeg model segments the brain")
        .cli("SynthSeg weights generation: v1 (SynthSeg 1.0, 32 labels; the default) or v2 (SynthSeg 2.0, 33 labels, adds a general CSF class; Billot et al., PNAS 2023)")
        .parse(P::Text).value_name("VERSION"),
        // QSM.rs segment/synthseg.rs: a crop of 0 gives the network an empty input.
        d("segmentation.crop", Segmentation, "Crop", "Cubic centre-crop in voxels at 1 mm before inference; empty processes the whole volume", Int).min(1.0).units("voxels").optional().flag("--synthseg-crop").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .gui("Cubic centre crop at 1 mm before inference; empty processes the whole volume")
        .cli("Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32). Omit to process the whole volume, as the SynthSeg CLI does"),
        // QSM.rs segment/synthseg.rs refuses a patch under 64 and rounds it up to a multiple of 32.
        d("segmentation.patch", Segmentation, "Sliding window", "Run the network on half-overlapping cubes of this many voxels at 1 mm (rounded up to a multiple of 32), so its memory follows the patch instead of the volume; empty runs the whole volume at once", Int).min(64.0).units("voxels").optional().flag("--synthseg-patch").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .gui("Patch size for running SynthSeg piece by piece, which needs less memory; empty runs the whole volume at once")
        .cli("Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the volume. Omit to run the whole volume at once, as the SynthSeg CLI does")
        .value_name("VOXELS"),
        d("segmentation.flip_averaging", Segmentation, "Flip averaging", "Average with a left-right flipped second pass (SynthSeg's default). Doubles inference time", Bool).flag("--no-synthseg-flip-averaging").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .cli("Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does"),
        d("segmentation.topology_cleanup", Segmentation, "Topology cleanup", "Reset each topological class to its largest connected component", Bool).flag("--no-synthseg-topology-cleanup").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none())
        .cli("Skip resetting each topological class to its largest connected component"),
        // QSM.rs segment/synthseg.rs refuses a negative sigma.
        d("segmentation.sigma_smoothing", Segmentation, "Posterior smoothing", "Gaussian blur on the posteriors before the labels are taken", Float).min(0.0)
        .value("Sigma (voxels)", "Gaussian sigma of the posterior blur, in voxels").units("voxels").off(json!(0.0)).flag("--synthseg-sigma").parent("pipeline.do_segmentation")
        .when(|x| x.segmentation_on && x.c.segmentation.custom_dseg_tool.is_none()).gui_value("Sigma", "Gaussian sigma of the posterior blur")
        .cli("Gaussian blur on the posteriors, in voxels (0 disables it)"),
        d("pipeline.do_analysis", Segmentation, "Per-structure statistics", "Summarise every map the run makes (χ, χ+/χ−, T2*, R2*, R2, R2') per structure into a TSV: mean, SD, median, min, max, 5th/95th percentiles. Requires segmentation, and QSM only if no other map is enabled", Bool).basic().flag("--do-analysis").full("Per-structure statistics")
        .cli("Also summarise every quantitative map (χ, χ+/χ−, T2*, R2*, R2, R2') per segmented structure into a TSV: mean, SD, median, min, max and the 5th/95th percentiles, one row per map and structure, plus a dataset-level table gathering all runs. Summarises whatever the run made — it requires --do-segmentation, and QSM only if no other map was asked for (each turned on automatically, and logged)"),
    ]
}
