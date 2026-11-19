/// `LsqrConfig::residual_weighting` is an `Option` whose `None` means "derive it from B0", and
/// it deliberately carries no `skip_serializing_if`: that attribute would hide the field from
/// the param-coverage guard, which reads the serialized keys of a default config and would then
/// report it as unexposed. The attribute is not needed either — `toml` omits a `None` at table
/// level on its own, which is what this pins.
#[test]
fn an_unset_residual_weighting_round_trips_through_toml() {
    let c = crate::config::PipelineConfig::default();
    assert!(c.inversion.lsqr.residual_weighting.is_none(), "None is the default");
    let t = c.to_toml().expect("TOML serialisation of the default config");
    assert!(!t.contains("residual_weighting"), "an unset Option should not be written: {t}");
    let back = crate::config::PipelineConfig::from_toml(&t).expect("round-trip");
    assert_eq!(back.inversion.lsqr.residual_weighting, None);

    // And a set one survives.
    let mut c2 = c.clone();
    c2.inversion.lsqr.residual_weighting = Some(0.25);
    let back2 = crate::config::PipelineConfig::from_toml(
        &c2.to_toml().expect("serialise")).expect("round-trip");
    assert_eq!(back2.inversion.lsqr.residual_weighting, Some(0.25));
}
