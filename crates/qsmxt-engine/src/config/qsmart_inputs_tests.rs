use super::*;

/// Vessel sizes given without their units were written in mm and read as mm (said under
/// QSMART); with their units, or not given at all, they read as written, in voxels by default.
#[test]
fn vessel_sizes_without_units_read_as_mm() {
    let old = "[inversion]\nalgorithm = \"qsmart\"\n\n[inversion.qsmart]\nvasc_sphere_radius = 8\nfrangi_scale_max = 6.0\n";
    let (c, warnings) = PipelineConfig::from_toml_migrating(old).unwrap();
    assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Mm);
    assert!(warnings.len() == 1 && warnings[0].contains("vessel_size_units"), "{warnings:?}");
    assert!(!warnings[0].contains("--"), "names settings, not flags");
    // Under another inversion the sizes do nothing, so nothing is said.
    let (c, warnings) = PipelineConfig::from_toml_migrating(&old.replace("qsmart\"", "rts\"")).unwrap();
    assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Mm);
    assert!(warnings.is_empty());
    // Units given: as written.
    let (c, warnings) = PipelineConfig::from_toml_migrating(&format!("{old}vessel_size_units = \"voxels\"\n")).unwrap();
    assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Voxels);
    assert!(warnings.is_empty());
    // No sizes: the defaults, in voxels.
    let (c, _) = PipelineConfig::from_toml_migrating("[inversion]\nalgorithm = \"qsmart\"\n").unwrap();
    assert_eq!(c.inversion.qsmart, QsmartConfig::default());
    assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Voxels);
}

/// Every file QSMxT writes reads back with the units it was written with: the full one, and
/// the changed-settings one, which carries the units wherever it carries a size.
#[test]
fn written_configs_keep_their_vessel_units() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Qsmart;
    c.inversion.qsmart.vasc_sphere_radius = 6;
    for units in [VesselSizeUnits::Voxels, VesselSizeUnits::Mm] {
        c.inversion.qsmart.vessel_size_units = units;
        for text in [c.to_toml().unwrap(), c.to_toml_changed().unwrap()] {
            let (back, warnings) = PipelineConfig::from_toml_migrating(&text).unwrap();
            assert_eq!(back.inversion.qsmart.vessel_size_units, units, "{text}");
            assert_eq!(back.inversion.qsmart.vasc_sphere_radius, 6);
            assert!(warnings.is_empty(), "{warnings:?}");
        }
    }
    // Without a size, the changed-settings file needs no units.
    let mut d = PipelineConfig::default();
    d.inversion.algorithm = QsmAlgorithm::Qsmart;
    assert!(!d.to_toml_changed().unwrap().contains("vessel_size_units"));
}

/// QSMART's echo fit runs with any unwrapping, phase offset removal and bipolar correction;
/// only a threshold qsm-core refuses is refused.
#[test]
fn the_echo_fit_takes_any_unwrapping() {
    let mut fm = FieldMappingConfig { b0_estimation: B0Estimation::QsmartEchofit, ..Default::default() };
    assert_eq!(fm.unwrapping_algorithm, UnwrappingAlgorithm::Romeo);
    assert!(fm.validate().is_ok());
    fm.bipolar_correction = true;
    assert!(fm.validate().is_ok());
    fm.phase_offset_removal = false;
    assert!(fm.validate().is_ok());
    fm.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
    fm.laplacian_unmasked = true;
    assert!(fm.validate().is_ok());
    fm.qsmart_fit_threshold = 0.0;
    assert!(fm.validate().is_err());
    fm.qsmart_fit_threshold = 40.0;
    fm.qsmart_fit_threshold_mode = crate::enums::QsmartFitThresholdMode::Percentile;
    fm.qsmart_fit_percentile = 0.0;
    assert!(fm.validate().is_err());
    assert!(FieldMappingConfig::default().validate().is_ok());
}
