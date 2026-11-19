/// SMWI weights the magnitude by χ, so asking for it without a susceptibility map is a request
/// that cannot be honoured. Turning it on forces QSM rather than silently producing nothing.
#[test]
fn smwi_forces_qsm() {
    let mut c = crate::config::PipelineConfig::default();
    c.pipeline.do_qsm = false;
    c.pipeline.do_smwi = true;
    crate::config::enforce_smwi_dependencies(&mut c);
    assert!(c.pipeline.do_qsm, "SMWI needs a susceptibility map");

    // A bring-your-own Chimap is a susceptibility map too — no need to recompute one.
    let mut c = crate::config::PipelineConfig::default();
    c.pipeline.do_qsm = false;
    c.pipeline.do_smwi = true;
    c.separation.custom_qsm_tool = Some("*".to_string());
    crate::config::enforce_smwi_dependencies(&mut c);
    assert!(!c.pipeline.do_qsm, "a custom Chimap should not force a reconstruction");

    // And it leaves an ordinary run alone.
    let mut c = crate::config::PipelineConfig::default();
    c.pipeline.do_qsm = false;
    crate::config::enforce_smwi_dependencies(&mut c);
    assert!(!c.pipeline.do_qsm);
}
