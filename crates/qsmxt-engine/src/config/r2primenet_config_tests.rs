use super::R2primenetConfig;

#[test]
fn a_bad_patch_is_an_error_not_a_panic() {
    let d = R2primenetConfig::default();
    assert!(d.validate().is_ok());
    for bad in [[0, 192, 128], [100, 192, 128]] {
        let c = R2primenetConfig { patch: bad, ..d.clone() };
        assert!(c.validate().unwrap_err().contains("multiple of 16"));
    }
}

/// The patch settings' texts say "multiple of N", and their minimum is N: QSM.rs's pooling
/// multiple for each network.
#[test]
fn the_patch_texts_and_minimum_are_qsm_rs_pooling_multiple() {
    for (path, m) in [
        ("separation.r2primenet.patch", qsm_core::relaxometry::POOL_MULTIPLE),
        ("separation.chi_sepnet.patch", qsm_core::separation::chisepnet::POOL_MULTIPLE),
        ("separation.chi_sepnet.frame_patch", qsm_core::separation::chisepnet::POOL_MULTIPLE),
        ("separation.susep_net.patch", qsm_core::separation::susep_net::POOL_MULTIPLE),
    ] {
        let s = crate::schema::settings().iter().find(|s| s.path == path).unwrap();
        assert!(s.tui_help.contains(&format!("multiple of {m}")), "{path}: {}", s.tui_help);
        // A text that states the rule states this multiple.
        for text in [s.help, s.tui_help].into_iter().filter(|t| t.contains("multiple of")) {
            assert!(text.contains(&format!("multiple of {m}")), "{path}: {text}");
        }
        assert_eq!(s.min, Some(m as f64), "{path}");
    }
}
