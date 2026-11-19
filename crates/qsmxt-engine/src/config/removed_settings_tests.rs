use super::*;

/// Settings files from earlier QSMxT versions still carry two settings that did nothing, TFI's
/// `merit` (a setting again since, MERIT, on by default) and QSMART's `sdf_spatial_radius`
/// (removed). They load, and the old values are ignored.
#[test]
fn configs_with_removed_settings_still_load() {
    let old = "[inversion]\nalgorithm = \"tfi\"\n\n[inversion.tfi]\nmerit = true\nprecond = 20.0\n\n\
               [inversion.qsmart]\nsdf_spatial_radius = 8\nvasc_sphere_radius = 5\n";
    let (c, warnings) = PipelineConfig::from_toml_migrating(old).expect("an old config loads");
    assert!(warnings.len() == 1 && warnings[0].starts_with("inversion.tfi.merit = true had no effect"), "{warnings:?}");
    assert_eq!(c.inversion.algorithm, QsmAlgorithm::Tfi);
    assert_eq!(c.inversion.tfi.precond, 20.0);
    assert_eq!(c.inversion.qsmart.vasc_sphere_radius, 5);
    // Saving it again drops them.
    let saved: toml::Table = toml::from_str(&c.to_toml().unwrap()).unwrap();
    let inversion = saved["inversion"].as_table().unwrap();
    assert_eq!(inversion["tfi"]["merit"].as_bool(), Some(qsm_core::inversion::TfiParams::default().merit));
    assert!(!inversion["qsmart"].as_table().unwrap().contains_key("sdf_spatial_radius"));
}
