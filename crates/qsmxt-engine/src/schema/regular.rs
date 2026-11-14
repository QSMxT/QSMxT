//! The basic view of the settings: which settings a front end shows when it hides the advanced
//! ones. One rule for every front end (the TUI's Basic mode, qsmbly's steps), so a setting that
//! is set or problematic is never hidden in either.

use std::collections::BTreeSet;

use super::{get, settings, settings_view, Level, ViewFacts};
use crate::config::PipelineConfig;
use crate::recommendations::Strength;

/// What a basic view compares settings with: the chosen toolbox pipeline's values
/// ([`crate::pipelines::target`]) while `config` follows one, else `defaults` (QSMxT's, or the
/// browser's). A pipeline's own tuning values are then not changes, and only departures from it
/// show.
pub fn baseline(config: &PipelineConfig, facts: &ViewFacts, defaults: &PipelineConfig) -> PipelineConfig {
    crate::pipelines::followed(config, facts)
        .and_then(|id| crate::pipelines::target(config, id, facts).ok())
        .unwrap_or_else(|| defaults.clone())
}

/// The settings a basic view shows for `config`, by path: each setting that applies (see
/// [`settings_view`]) and
/// - is basic ([`Level::Basic`]), or
/// - differs from `baseline` (see [`baseline`]), or
/// - has a value error, or a finding the user should act on (required, recommended or
///   discouraged; a suggestion or an authors' setting is information, in the `!` details), or
/// - is the parent of one of those.
///
/// Every other setting that applies is advanced and hidden; a front end says how many.
pub fn regular_view(config: &PipelineConfig, baseline: &PipelineConfig, facts: &ViewFacts) -> BTreeSet<&'static str> {
    let all = settings();
    let mut shown = BTreeSet::new();
    for v in settings_view(config, facts).into_iter().filter(|v| v.visible) {
        let Some(s) = all.iter().find(|s| s.path == v.path) else { continue };
        let flagged = v.error.is_some()
            || v.recommendations.iter().any(|r| r.strength >= Strength::Discouraged);
        if s.level == Level::Basic || flagged || get(config, s.path) != get(baseline, s.path) {
            shown.insert(s.path);
        }
    }
    // A shown setting's parents show with it.
    let parents: Vec<&'static str> = shown.iter()
        .flat_map(|p| {
            std::iter::successors(all.iter().find(|s| s.path == *p).and_then(|s| s.parent), |p| {
                all.iter().find(|s| s.path == *p).and_then(|s| s.parent)
            })
        })
        .collect();
    shown.extend(parents);
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{set, Setting, ValueType};
    use serde_json::json;

    fn facts() -> ViewFacts {
        ViewFacts::default()
    }

    fn view(c: &PipelineConfig) -> BTreeSet<&'static str> {
        regular_view(c, &baseline(c, &facts(), &PipelineConfig::default()), &facts())
    }

    #[test]
    fn a_default_config_shows_only_basic_settings() {
        let shown = view(&PipelineConfig::default());
        for p in &shown {
            let s = settings().iter().find(|s| s.path == *p).unwrap();
            assert_eq!(s.level, Level::Basic, "{p}");
        }
        assert!(shown.contains("inversion.algorithm"));
        assert!(!shown.contains("inversion.ilsqr.tol"));
    }

    #[test]
    fn a_changed_advanced_setting_shows_with_its_parent() {
        let mut c = PipelineConfig::default();
        set(&mut c, "inversion.algorithm", json!("medi")).unwrap();
        assert!(!view(&c).contains("inversion.medi.lambda"));
        set(&mut c, "inversion.medi.lambda", json!(2000.0)).unwrap();
        assert!(view(&c).contains("inversion.medi.lambda"));
        set(&mut c, "inversion.medi.lambda_csf", json!(50.0)).unwrap();
        let shown = view(&c);
        assert!(shown.contains("inversion.medi.lambda_csf") && shown.contains("inversion.medi.csf"));
    }

    #[test]
    fn a_value_error_shows_the_setting() {
        let mut c = PipelineConfig::default();
        set(&mut c, "inversion.ilsqr.max_iter", json!(0)).unwrap();
        assert!(view(&c).contains("inversion.ilsqr.max_iter"));
    }

    #[test]
    fn a_pipelines_own_values_are_not_changes() {
        let f = facts();
        let mut c = PipelineConfig::default();
        crate::pipelines::choose(&mut c, "medi-toolbox", &f).unwrap();
        let base = baseline(&c, &f, &PipelineConfig::default());
        let shown = regular_view(&c, &base, &f);
        // Only the pipeline's recommendations show (PDF's iteration count): its own values do not.
        let flagged: BTreeSet<&str> = settings_view(&c, &f).into_iter()
            .filter(|v| v.recommendations.iter().any(|r| r.strength >= Strength::Discouraged))
            .map(|v| v.path)
            .collect();
        let advanced: Vec<_> = shown.iter()
            .filter(|p| settings().iter().any(|s| s.path == **p && s.level == Level::Advanced) && !flagged.contains(**p))
            .collect();
        assert!(advanced.is_empty(), "{advanced:?}");
        assert!(shown.contains("bg_removal.pdf.n_cg"), "the recommended PDF iterations show");
    }

    /// Every setting's level was decided: each choice (a switch, a list, a text input) is basic
    /// or listed here as an advanced choice, and numbers are advanced unless marked basic. A new
    /// choice fails this test until it is given a level.
    #[test]
    fn every_choice_has_a_reviewed_level() {
        const ADVANCED_CHOICES: &[&str] = &[
            "pipeline.ignore_slice_gap", "pipeline.crop_to_mask", "pipeline.fft_padding",
            "masking.combine", "bet.orientation", "masking.two_pass_combine", "masking.two_pass_background", "masking.n4_geometry",
            "motion_correction.reference", "motion_correction.chain",
            "field_mapping.prelude.labelling", "field_mapping.laplacian_solver", "field_mapping.laplacian_unmasked", "field_mapping.laplacian_keep_outside",
            "field_mapping.romeo.individual", "field_mapping.romeo.correct_global", "field_mapping.romeo.phase_coherence",
            "field_mapping.romeo.phase_gradient_coherence", "field_mapping.romeo.phase_linearity",
            "field_mapping.romeo.mag_coherence", "field_mapping.romeo.mag_weight", "field_mapping.romeo.mag_weight2",
            "field_mapping.romeo.bestpath", "field_mapping.romeo.merge_regions", "field_mapping.romeo.correct_regions",
            "field_mapping.qsmart_fit_threshold_mode", "field_mapping.b0_weight_type",
            "field_mapping.linear_fit.estimate_offset", "field_mapping.robust_fit.loss",
            "bg_removal.polish_mask", "bg_removal.ukb_mask_trim", "bg_removal.vsharp.deconvolve", "bg_removal.vsharp.grid", "bg_removal.pdf.space",
            "bg_removal.lbv.precision", "bg_removal.ismv.variant", "bg_removal.harperella.rim", "bg_removal.iharperella.rim",
            "inversion.tgv.te", "inversion.nltv.te", "inversion.medi.smv_shrink_mask", "inversion.medi.msmv_prefilter",
            "inversion.medi.merit", "inversion.medi.csf_erode", "inversion.medi.data_weighting", "inversion.medi.te",
            "inversion.tfi.merit", "inversion.tfi.data_weighting", "inversion.tfi.te", "inversion.ilsqr.precision",
            "inversion.star.te", "inversion.lsqr.fit_global_offset", "inversion.heidi.apply_laplacian_correction",
            "inversion.heidi.te", "inversion.heidi.denoise", "inversion.heidi.denoise_reference_itk",
            "inversion.qsmart.vessel_magnitude", "inversion.qsmart.offset_grid", "inversion.qsmart.vessel_size_units",
            "inversion.ndi.auto_stop", "inversion.ndi.te", "inversion.ndi.weight_norm", "inversion.fansi.te",
            "inversion.l1qsm.te", "inversion.whqsm.weight", "inversion.whqsm.te",
            "multi_orientation.magnitude_weighting", "multi_orientation.force",
            "separation.decompose.unwrap_phase", "separation.decompose.fit",
            "separation.chi_sep_ilsqr.qsm_ilsqr.precision", "separation.chi_sep_medi.qsm_ilsqr.precision",
            "separation.chi_sep_medi.csf_r2star", "swi.unwrap", "swi.scaling",
            "separation.r2primenet.ignore_field_mismatch", "segmentation.flip_averaging", "segmentation.topology_cleanup",
        ];
        let choice = |s: &Setting| matches!(s.value_type,
            ValueType::Bool | ValueType::Enum { .. } | ValueType::Text | ValueType::EchoReference { .. } | ValueType::EchoTime { .. });
        let unreviewed: Vec<_> = settings().iter()
            .filter(|s| s.level == Level::Advanced && choice(s) && !ADVANCED_CHOICES.contains(&s.path))
            .map(|s| s.path)
            .collect();
        assert!(unreviewed.is_empty(), "choices with no reviewed level (make them basic, or list them): {unreviewed:?}");
        let stale: Vec<_> = ADVANCED_CHOICES.iter()
            .filter(|p| !settings().iter().any(|s| s.path == **p && s.level == Level::Advanced && choice(s)))
            .collect();
        assert!(stale.is_empty(), "listed but not an advanced choice: {stale:?}");
    }
}
