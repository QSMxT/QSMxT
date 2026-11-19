use super::*;

/// v9.22.0 configs carry `obliquity_threshold` alone, a negative value meaning "off": they keep
/// their meaning under `axial_resampling`, with a warning; a new config cannot go negative.
#[test]
fn legacy_obliquity_thresholds_map_onto_axial_resampling() {
    let (c, w) = PipelineConfig::from_toml_migrating("[pipeline]\nobliquity_threshold = -1.0\n").unwrap();
    assert!(!c.pipeline.axial_resampling && w.len() == 1, "{w:?}");
    assert_eq!(c.pipeline.obliquity_threshold, DEFAULT_OBLIQUITY_THRESHOLD);
    let (c, w) = PipelineConfig::from_toml_migrating("[pipeline]\nobliquity_threshold = 5\n").unwrap();
    assert!(c.pipeline.axial_resampling && w.len() == 1, "{w:?}");
    assert_eq!(c.pipeline.obliquity_threshold, 5.0);
    // Today's configs: both keys, no warning, round trip.
    let mut d = PipelineConfig::default();
    d.pipeline.axial_resampling = true;
    let (back, w) = PipelineConfig::from_toml_migrating(&d.to_toml().unwrap()).unwrap();
    assert!(w.is_empty() && back.pipeline.axial_resampling);
    let (back, w) = PipelineConfig::from_toml_migrating(&PipelineConfig::default().to_toml().unwrap()).unwrap();
    assert!(w.is_empty() && !back.pipeline.axial_resampling);
    assert!(PipelineConfig::from_toml("[pipeline]\naxial_resampling = true\nobliquity_threshold = -1.0\n").is_err());
    // Nothing said about either: off, at the default threshold.
    let c = PipelineConfig::from_toml("").unwrap();
    assert!(!c.pipeline.axial_resampling);
    assert_eq!(c.pipeline.obliquity_threshold, 10.0);
}
