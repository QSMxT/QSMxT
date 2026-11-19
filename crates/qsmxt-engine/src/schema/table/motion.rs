//! The Motion section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("motion_correction.enabled", Motion, "Motion correction", "Rigidly co-register every echo onto a reference echo before any other stage (multi-echo, needs magnitude). Off by default", Bool).basic().flag("--motion-correction")
        .gui("Rigidly co-register every echo onto a reference echo before any other stage (multi-echo, needs magnitude)"),
        d("motion_correction.reference", Motion, "Reference", "", EchoReference(vec![
            o("first", "first", "First echo", "Align every echo onto the first (best SNR; the published procedure) (default)"),
            o("last", "last", "Last echo", "Align every echo onto the last"),
            o("1", "echo", "Echo number", "Align every echo onto a given echo (Reference echo)"),
        ]))
        .value("Reference echo", "1-based echo number the others are aligned onto").flag("--motion-reference").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled).full("Reference echo")
        .gui("The echo every other echo is aligned onto").gui_value("Echo number", "The echo the others are aligned onto, counted from 1"),
        d("motion_correction.chain", Motion, "Chain", "Seed each echo's registration from its neighbour's solution", Bool).flag("--no-motion-chain").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled).full("Chain registrations"),
        d("motion_correction.chain_levels", Motion, "Chain levels", "Pyramid levels for a seeded registration (default 3)", Int).flag("--motion-chain-levels").parent("motion_correction.chain")
        .when(|x| x.c.motion_correction.enabled && x.c.motion_correction.chain)
        .gui("Pyramid levels for a seeded registration"),
        d("motion_correction.min_ncc", Motion, "Min NCC", "NCC below which a registration is reported as suspect (0.3 for echoes; ~0.9 for repeats)", Float).flag("--motion-min-ncc").parent("motion_correction.enabled")
        .when(|x| x.c.motion_correction.enabled).full("Minimum NCC")
        .gui("Normalised cross-correlation below which a registration is reported as suspect (0.3 for echoes; about 0.9 for repeats)"),
        d("registration.levels", Motion, "Levels", "Pyramid levels: the capture range; do not lower (default 5)", Int).flag("--registration-levels").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Pyramid levels")
        .gui("Pyramid levels: the capture range; do not lower").context("Registration"),
        d("registration.max_samples", Motion, "Max samples", "Voxels sampled per metric evaluation, per level", Int).flag("--registration-max-samples").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Maximum samples").context("Registration"),
        d("registration.max_iterations", Motion, "Max iterations", "Powell cycles per level", Int).flag("--registration-max-iterations").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Maximum iterations").context("Registration"),
        d("registration.max_evaluations", Motion, "Max evaluations", "Metric evaluations across all levels (hard stop)", Int).flag("--registration-max-evaluations").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Maximum evaluations").context("Registration"),
        d("registration.tolerance", Motion, "Tolerance", "Relative cost improvement per cycle that counts as converged", Float).flag("--registration-tolerance").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).context("Registration"),
        d("registration.rotation_step_deg", Motion, "Rotation step (deg)", "Initial rotation step at the finest level", Float).units("deg").flag("--registration-rotation-step-deg").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Rotation step").context("Registration"),
        d("registration.translation_step_mm", Motion, "Translation step (mm)", "Initial translation step at the finest level", Float).units("mm").flag("--registration-translation-step-mm").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Translation step").context("Registration"),
        d("registration.min_overlap", Motion, "Min overlap", "Smallest fraction of sampled voxels that must overlap", Float).flag("--registration-min-overlap").also(&[MultiOrientation])
        .when(|x| x.c.motion_correction.enabled || (x.qsm && x.grouped && x.c.multi_orientation.register)).full("Minimum overlap").context("Registration"),
    ]
}
