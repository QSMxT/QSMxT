//! The SWI/SMWI section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.do_swi", SwiSmwi, "Compute SWI", "Also compute susceptibility-weighted images with CLEAR-SWI, from every echo", Bool).basic().flag("--do-swi")
        .cli("Also compute SWI"),
        d("swi.unwrap", SwiSmwi, "Unwrapping", "", Enum(vec![
            o("laplacian", "laplacian", "Laplacian", "Each echo unwrapped with the Laplacian method and high-pass filtered, then combined"),
            o("laplacian-slice", "laplacian-slice", "Laplacian per slice", "Laplacian unwrapping slice by slice, for 2D multi-slice acquisitions"),
            o("romeo", "romeo", "ROMEO", "All echoes unwrapped together with ROMEO, combined, then high-pass filtered"),
        ])).flag("--swi-unwrapping").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).full("SWI unwrapping")
        .gui("How the phase is unwrapped and high-pass filtered for SWI").context("SWI")
        .cli("SWI phase unwrapping before the echoes are combined [default: laplacian]")
        .value_help("laplacian", "Each echo with the Laplacian method, then combined").value_help("laplacian-slice", "Laplacian, slice by slice (2D multi-slice acquisitions)").value_help("romeo", "All echoes together with ROMEO"),
        d("swi.scaling", SwiSmwi, "Scaling", "", Enum(vec![
            o("tanh", "tanh", "Tanh", "Phase scaling type for SWI"),
            o("negative-tanh", "negative-tanh", "Negative tanh", "Phase scaling type for SWI"),
            o("positive", "positive", "Positive", "Phase scaling type for SWI"),
            o("negative", "negative", "Negative", "Phase scaling type for SWI"),
            o("triangular", "triangular", "Triangular", "Phase scaling type for SWI"),
        ])).flag("--swi-scaling").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).full("Phase scaling")
        .gui("How the filtered phase is turned into the mask the magnitude is multiplied by").context("SWI")
        .cli("SWI phase-mask scaling")
        .value_alias("negative-tanh", &["negative_tanh"]),
        d("swi.strength", SwiSmwi, "Strength", "Phase scaling strength (higher = stronger phase contrast)", Float).flag("--swi-strength").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).context("SWI"),
        d("swi.hp_sigma", SwiSmwi, "HP sigma (x y z)", "High-pass filter sigma per axis, in voxels (x y z): background phase removal. Set z to 0 for thin axial slices", FloatVec3).units("voxels").flag("--swi-hp-sigma").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).full("High-pass sigma")
        .gui("High-pass filter sigma per axis (x y z): background phase removal. Set z to 0 for thin axial slices").context("SWI")
        .cli("SWI high-pass filter sigma (3 values, in voxels)"),
        // QSM.rs swi.rs mip_geometry refuses a window of 0 (and one wider than the volume).
        d("swi.mip_window", SwiSmwi, "mIP window", "Minimum intensity projection window size in slices", Int).min(1.0).units("slices").flag("--swi-mip-window").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi)
        .gui("Minimum-intensity-projection window").context("SWI"),
        d("swi.filtered_phase", SwiSmwi, "Filtered phase", "Also write the high-pass filtered phase the SWI is weighted by (desc-swi_phase): Hz for several echoes, radians for one", Bool).basic().flag("--swi-filtered-phase").parent("pipeline.do_swi")
        .when(|x| x.c.pipeline.do_swi).full("SWI filtered phase").context("SWI")
        .cli("Also write CLEAR-SWI's high-pass filtered phase, the phase the SWI is weighted by (`_desc-swi_phase.nii`): Hz for several echoes, radians for one, 0 outside CLEAR-SWI's head mask. Implies --do-swi"),
        d("pipeline.do_smwi", SwiSmwi, "Compute SMWI", "Susceptibility map-weighted imaging — weights the magnitude by a mask built from χ, not from filtered phase. Needs QSM", Bool).basic().flag("--do-smwi")
        .cli("Also compute SMWI (susceptibility map-weighted imaging): weights the magnitude by a mask built from the susceptibility map rather than from high-pass filtered phase, so the contrast stays where the source is. Needs χ, so it requires QSM (turned on automatically, and logged). doi:10.1002/mrm.24920"),
        d("smwi.threshold_ppm", SwiSmwi, "Threshold", "|χ| in ppm at which the weighting mask reaches zero. The 1 ppm default targets strong sources; brain tissue wants far less", Float).units("ppm").flag("--smwi-threshold").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi)
        .gui("|χ| at which the weighting mask reaches zero. 1 ppm targets strong sources; brain tissue wants far less").context("SMWI")
        .cli("SMWI susceptibility threshold in ppm — the |χ| at which the weighting mask reaches zero. The 1 ppm default follows SEPIA and targets strong sources; brain tissue wants far less"),
        // QSM.rs swi.rs: w^power with w = 0 is inf for a negative power.
        d("smwi.power", SwiSmwi, "Power", "Power the mask is raised to (contrast strength)", Float).min(0.0).flag("--smwi-power").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi).context("SMWI")
        .cli("SMWI mask power (contrast strength)"),
        // QSM.rs swi.rs mip_geometry refuses a window of 0 (and one wider than the volume).
        d("smwi.mip_window", SwiSmwi, "mIP window", "Minimum-intensity-projection window in slices", Int).min(1.0).units("slices").flag("--smwi-mip-window").parent("pipeline.do_smwi")
        .when(|x| x.c.pipeline.do_smwi)
        .gui("Minimum-intensity-projection window").context("SMWI"),
    ]
}
