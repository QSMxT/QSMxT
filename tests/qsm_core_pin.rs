//! qsm-core pin guard: QSMxT and qsmxt-engine pin the same QSM.rs commit, and Cargo.lock
//! builds that commit.
//!
//! A git dependency's line in Cargo.lock reads `git+<url>?rev=<rev>#<commit>`, and cargo builds
//! `<commit>`, not `<rev>`. Editing only the `?rev=` part of the line (as a repin by hand can)
//! leaves the build on the old commit with nothing to show for it. This test is textual: it
//! reads both manifests and the lockfile and fails unless all four name the same commit.

const ROOT_TOML: &str = include_str!("../Cargo.toml");
const ENGINE_TOML: &str = include_str!("../crates/qsmxt-engine/Cargo.toml");
const LOCK: &str = include_str!("../Cargo.lock");

/// The `rev = "…"` of the manifest's `qsm-core = { git = … }` dependency line.
fn manifest_rev(toml: &str, which: &str) -> String {
    let line = toml.lines().find(|l| l.trim_start().starts_with("qsm-core = {") && l.contains("QSM.rs"))
        .unwrap_or_else(|| panic!("{which}: no qsm-core git dependency"));
    let rest = line.split("rev = \"").nth(1).unwrap_or_else(|| panic!("{which}: qsm-core has no rev: {line}"));
    rest.split('"').next().unwrap().to_string()
}

#[test]
fn qsm_core_rev_is_the_same_in_both_manifests_and_the_lockfile() {
    let root = manifest_rev(ROOT_TOML, "Cargo.toml");
    let engine = manifest_rev(ENGINE_TOML, "crates/qsmxt-engine/Cargo.toml");
    assert_eq!(root, engine, "Cargo.toml and crates/qsmxt-engine/Cargo.toml pin different qsm-core revs");
    assert!(root.len() == 40 && root.chars().all(|c| c.is_ascii_hexdigit()), "qsm-core rev is not a full commit hash: {root}");

    let sources: Vec<&str> = LOCK.lines()
        .filter_map(|l| l.strip_prefix("source = \"git+https://github.com/astewartau/QSM.rs.git?rev="))
        .collect();
    assert_eq!(sources.len(), 1, "Cargo.lock should hold one qsm-core source: {sources:?}");
    let (rev, commit) = sources[0].trim_end_matches('"').split_once('#')
        .unwrap_or_else(|| panic!("Cargo.lock qsm-core source has no #commit: {}", sources[0]));
    assert_eq!(rev, root, "Cargo.lock's ?rev= differs from the manifests' rev");
    assert_eq!(commit, root, "Cargo.lock builds QSM.rs {commit}, not the pinned {root}: update the #commit too (cargo update -p qsm-core)");
}
