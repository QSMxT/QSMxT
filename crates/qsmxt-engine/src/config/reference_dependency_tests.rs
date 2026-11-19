use crate::config::{enforce_reference_dependencies, PipelineConfig};
use crate::enums::QsmReference;

/// A region reference is measured on the parcellation, so asking for one asks for SynthSeg.
/// Erroring instead would make `--qsm-reference thalamus` an incomplete instruction.
#[test]
fn a_region_reference_turns_segmentation_on() {
    let mut c = PipelineConfig::default();
    c.qsm.reference = QsmReference::Region;
    c.qsm.reference_region = Some("thalamus".to_string());
    enforce_reference_dependencies(&mut c);
    assert!(c.pipeline.do_segmentation);
    assert_eq!(c.qsm.reference, QsmReference::Region, "the reference itself is untouched");

    // A supplied dseg is a parcellation too — no need to compute one.
    let mut c = PipelineConfig::default();
    c.qsm.reference = QsmReference::Region;
    c.qsm.reference_region = Some("thalamus".to_string());
    c.segmentation.custom_dseg_tool = Some("freesurfer".to_string());
    enforce_reference_dependencies(&mut c);
    assert!(!c.pipeline.do_segmentation, "a supplied dseg needs no SynthSeg run");

    // No reconstruction, nothing to reference: segmentation is not forced for its own sake.
    let mut c = PipelineConfig::default();
    c.pipeline.do_qsm = false;
    c.qsm.reference = QsmReference::Region;
    c.qsm.reference_region = Some("thalamus".to_string());
    enforce_reference_dependencies(&mut c);
    assert!(!c.pipeline.do_segmentation);
}

/// `mean` and `none` force nothing, and a hand-edited config naming no region is not a
/// reference at all — it falls back rather than referencing to an empty set.
#[test]
fn the_other_methods_force_nothing_and_an_empty_region_is_left_for_the_error() {
    for method in [QsmReference::Mean, QsmReference::None] {
        let mut c = PipelineConfig::default();
        c.qsm.reference = method;
        enforce_reference_dependencies(&mut c);
        assert!(!c.pipeline.do_segmentation, "{method} should force nothing");
        assert_eq!(c.qsm.reference, method);
    }

    for empty in [None, Some(String::new()), Some("  ".to_string())] {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = empty.clone();
        enforce_reference_dependencies(&mut c);
        // Not rewritten to the mean: the settings view and the command line refuse it.
        assert_eq!(c.qsm.reference, QsmReference::Region, "{empty:?} is left as it is");
        assert_eq!(c.qsm.reference_region, empty);
        assert!(!c.pipeline.do_segmentation);
        let view = crate::schema::settings_view(&c, &crate::schema::ViewFacts::default());
        let error = view.iter().find(|v| v.path == "qsm.reference_region").and_then(|v| v.error.clone());
        assert!(error.is_some(), "{empty:?} is refused");
    }
}

/// One flag carries both fields, so the pair has to collapse back to a single value.
#[test]
fn the_spec_round_trips_through_one_value() {
    let mut c = PipelineConfig::default();
    assert_eq!(c.qsm.reference_spec(), "mean");
    c.qsm.reference = QsmReference::None;
    assert_eq!(c.qsm.reference_spec(), "none");
    c.qsm.reference = QsmReference::Region;
    c.qsm.reference_region = Some("left-thalamus,4".to_string());
    assert_eq!(c.qsm.reference_spec(), "left-thalamus,4");
}

/// The region has to survive a config file, or `--config` silently drops the reference.
#[test]
fn the_region_survives_a_toml_round_trip() {
    let mut c = PipelineConfig::default();
    c.qsm.reference = QsmReference::Region;
    c.qsm.reference_region = Some("ventricles".to_string());
    let back = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
    assert_eq!(back.qsm.reference, QsmReference::Region);
    assert_eq!(back.qsm.reference_region.as_deref(), Some("ventricles"));

    // And an old config with no region key still loads.
    let old = PipelineConfig::from_toml("[qsm]\nreference = \"mean\"\n").unwrap();
    assert_eq!(old.qsm.reference, QsmReference::Mean);
    assert_eq!(old.qsm.reference_region, None);
}
