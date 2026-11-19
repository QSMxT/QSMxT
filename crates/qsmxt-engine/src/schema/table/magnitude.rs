//! The Magnitude section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.do_magnitude", Magnitude, "Combined magnitude", "Publish the combined magnitude (part-mag_T2starw): the root sum of squares of the echoes, corrected for inhomogeneity when that is on. Off, it is still made for SMWI, segmentation or a magnitude-weighted inversion, and kept with the run's intermediates", Bool).basic().flag("--no-magnitude"),
    ]
}
