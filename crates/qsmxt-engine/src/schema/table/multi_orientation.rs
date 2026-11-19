//! The Multi-orientation section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("multi_orientation.algorithm", MultiOrientation, "Combine as", "", Enum(vec![
            o("cosmos", "cosmos", "COSMOS", "COSMOS — scalar susceptibility from 2+ orientations, co-registered if needed, closed form in k-space"),
            o("sti", "sti", "STI", "STI — rank-2 susceptibility tensor; needs 6+ orientations that are not coplanar"),
        ])).basic().flag("--multi-orientation-algorithm").unavailable_in_browser().parent("multi_orientation.group_by").full("Combine as")
        .gui("How the orientations of a group are combined into one map")
        .cli("Which reconstruction to run over each orientation group")
        .value_help("cosmos", "Scalar susceptibility from 2+ orientations").value_help("sti", "Rank-2 susceptibility tensor from 6+ orientations"),
        d("multi_orientation.lambda", MultiOrientation, "Lambda", "Regularization (blank = 0, plain least squares). Raise for 2 orientations or single-axis rotations", Float).flag("--multi-orientation-lambda").unavailable_in_browser().parent("multi_orientation.group_by")
        .gui("Regularisation weight; 0 is plain least squares. Raise it for 2 orientations or single-axis rotations")
        .cli("Regularization for the multi-orientation reconstruction (COSMOS k-space L2, or STI ridge)"),
        d("multi_orientation.reference", MultiOrientation, "Reference orientation", "Orientation the others are registered onto, whose grid and segmentation the group map uses: a glob over the run key (*acq-Neutral*) or a word in it. Blank = first by name. Pick the neutral head position", Text).basic().flag("--orientation-reference").unavailable_in_browser().parent("multi_orientation.group_by")
        .cli("The orientation each group is registered onto, and whose grid and segmentation the group map uses: a glob over the run key (`*acq-Neutral*`) or a word in it. Must match one run per group. Default: the first run in name order")
        .value_name("PATTERN"),
        d("multi_orientation.magnitude_weighting", MultiOrientation, "COSMOS magnitude weighting", "COSMOS groups: weight each orientation by its combined magnitude (conjugate-gradient solve). Off = closed-form least squares", Bool).flag("--cosmos-magnitude-weighting").unavailable_in_browser().parent("multi_orientation.algorithm")
        .when(|x| x.cosmos)
        .cli("COSMOS: weight each orientation by its combined magnitude (conjugate-gradient solve)"),
        // QSMxT MultiOrientationConfig::validate: tolerance must be > 0.
        d("multi_orientation.cosmos_tol", MultiOrientation, "CG tolerance", "Conjugate-gradient tolerance of weighted COSMOS", Float).above(0.0).flag("--cosmos-tol").unavailable_in_browser().parent("multi_orientation.magnitude_weighting")
        .when(|x| x.cosmos && x.c.multi_orientation.magnitude_weighting)
        .cli("Magnitude-weighted COSMOS: conjugate-gradient tolerance (needs --cosmos-magnitude-weighting)"),
        // QSMxT MultiOrientationConfig::validate: at least one iteration.
        d("multi_orientation.cosmos_max_iter", MultiOrientation, "CG max iterations", "Conjugate-gradient iteration cap of weighted COSMOS", Int).min(1.0).flag("--cosmos-max-iter").unavailable_in_browser().parent("multi_orientation.magnitude_weighting")
        .when(|x| x.cosmos && x.c.multi_orientation.magnitude_weighting).full("CG maximum iterations")
        .cli("Magnitude-weighted COSMOS: conjugate-gradient iteration cap (needs --cosmos-magnitude-weighting)"),
        d("multi_orientation.register", MultiOrientation, "Co-register", "Co-register orientations that need it, with the registration settings below. Off = refuse such groups", Bool).basic().flag("--no-orientation-registration").unavailable_in_browser().parent("multi_orientation.group_by")
        .cli("Do not co-register the orientations in a group — use this when they were already registered externally and a second interpolation is not wanted. A group that still needs registration is then refused instead of reconstructed"),
        d("multi_orientation.force", MultiOrientation, "Force", "Reconstruct even when the B0 directions look degenerate (wrong metadata gives a plausible wrong map)", Bool).flag("--multi-orientation-force").unavailable_in_browser().parent("multi_orientation.group_by").full("Force reconstruction")
        .cli("Reconstruct an orientation group even when its B0 directions look degenerate"),
    ]
}
