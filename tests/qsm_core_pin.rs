//! qsm-core pin guard: QSMxT and qsmxt-engine pin the same QSM.rs commit or tag, and Cargo.lock
//! builds it.
//!
//! During development both manifests pin QSM.rs by `rev = "<commit>"`; a release pins it by
//! `tag = "vX.Y.Z"`. A git dependency's line in Cargo.lock reads `git+<url>?rev=<rev>#<commit>`
//! (or `?tag=<tag>#<commit>`), and cargo builds `<commit>`, not what the query names. Editing only
//! the query part of the line (as a repin by hand can) leaves the build on the old commit with
//! nothing to show for it. This test is textual: it reads both manifests and the lockfile and
//! fails unless they name the same pin, and, for a rev pin, the lock builds that commit. A tag's
//! commit cannot be checked offline; build.rs records it in every output's provenance.

const ROOT_TOML: &str = include_str!("../Cargo.toml");
const ENGINE_TOML: &str = include_str!("../crates/qsmxt-engine/Cargo.toml");
const LOCK: &str = include_str!("../Cargo.lock");

#[derive(Debug, PartialEq)]
enum Pin {
    Rev(String),
    Tag(String),
}

fn is_full_hash(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// The `rev = "…"` or `tag = "…"` of the manifest's `qsm-core = { git = … }` dependency line.
fn manifest_pin(toml: &str, which: &str) -> Pin {
    let line = toml.lines().find(|l| l.trim_start().starts_with("qsm-core = {") && l.contains("QSM.rs"))
        .unwrap_or_else(|| panic!("{which}: no qsm-core git dependency"));
    let value = |key: &str| line.split(&format!("{key} = \"")).nth(1).map(|r| r.split('"').next().unwrap().to_string());
    match (value("rev"), value("tag")) {
        (Some(rev), None) => Pin::Rev(rev),
        (None, Some(tag)) => Pin::Tag(tag),
        _ => panic!("{which}: qsm-core should have exactly one of rev or tag: {line}"),
    }
}

#[test]
fn qsm_core_pin_is_the_same_in_both_manifests_and_the_lockfile() {
    let root = manifest_pin(ROOT_TOML, "Cargo.toml");
    let engine = manifest_pin(ENGINE_TOML, "crates/qsmxt-engine/Cargo.toml");
    assert_eq!(root, engine, "Cargo.toml and crates/qsmxt-engine/Cargo.toml pin different qsm-core versions");

    let sources: Vec<&str> = LOCK.lines()
        .filter_map(|l| l.strip_prefix("source = \"git+https://github.com/astewartau/QSM.rs.git?"))
        .collect();
    assert_eq!(sources.len(), 1, "Cargo.lock should hold one qsm-core source: {sources:?}");
    let (query, commit) = sources[0].trim_end_matches('"').split_once('#')
        .unwrap_or_else(|| panic!("Cargo.lock qsm-core source has no #commit: {}", sources[0]));
    assert!(is_full_hash(commit), "Cargo.lock's qsm-core #commit is not a full commit hash: {commit}");

    match root {
        Pin::Rev(rev) => {
            assert!(is_full_hash(&rev), "qsm-core rev is not a full commit hash: {rev}");
            assert_eq!(query, format!("rev={rev}"), "Cargo.lock's qsm-core source differs from the manifests' rev");
            assert_eq!(commit, rev, "Cargo.lock builds QSM.rs {commit}, not the pinned {rev}: update the #commit too (cargo update -p qsm-core)");
        }
        Pin::Tag(tag) => {
            assert!(tag.starts_with('v'), "qsm-core tag is not a release tag: {tag}");
            assert_eq!(query, format!("tag={tag}"), "Cargo.lock's qsm-core source differs from the manifests' tag (cargo update -p qsm-core)");
        }
    }
}
