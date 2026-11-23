#[test]
fn dump() {
    let c = qsmxt_engine::config::PipelineConfig::default();
    std::fs::write("/scratch/user/uqaste15/src/catchup2/old-defaults.toml", c.to_toml().unwrap()).unwrap();
}
