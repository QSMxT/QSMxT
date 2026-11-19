//! What this build is: the QSMxT version or commit, as the log, the TUI and the provenance a run
//! writes describe it. The commit comes from the build script (`build.rs`).

/// Strips a leading 'v' from a version string if present.
pub fn strip_v(s: &str) -> &str {
    s.strip_prefix('v').unwrap_or(s)
}

/// The QSMxT commit the build was made from: `qsmxt update --main`'s, else the source checkout's;
/// empty when neither is known.
pub const SOURCE_COMMIT: &str = env!("QSMXT_SOURCE_COMMIT");

/// The QSM.rs this build links, as the build script describes it (its release tag, or its commit).
pub const QSM_CORE_DESCRIBE: &str = env!("QSM_CORE_DESCRIBE");

/// What this build is, in the log, the TUI and the provenance a run writes: the release's tag
/// (`v10.1.0`), or for an unreleased build (0.0.0 until the release workflow sets the version from
/// the tag) `rev-` and the commit it was built from — both as QSM.rs is described. Comparing
/// versions (`qsmxt update`) uses [`current_version`], which is unchanged.
pub fn describe_version() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| describe(env!("CARGO_PKG_VERSION"), SOURCE_COMMIT))
}

/// Version 0.0.0: what Cargo.toml says until the release workflow sets it from the tag.
pub const fn unreleased(version: &str) -> bool {
    let (v, u) = (version.as_bytes(), b"0.0.0");
    if v.len() != u.len() {
        return false;
    }
    let mut i = 0;
    while i < v.len() {
        if v[i] != u[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn describe(version: &str, commit: &str) -> String {
    if !unreleased(version) {
        format!("v{}", strip_v(version))
    } else if !commit.is_empty() {
        format!("rev-{commit}")
    } else {
        version.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A release says its version; an unreleased build the commit it was built from, or 0.0.0
    /// when that is not known.
    #[test]
    fn unreleased_builds_are_described_by_their_commit() {
        let c = "17b871647026eb9a80b57e1e3c37d06361f53c49";
        assert_eq!(describe("0.0.0", c), format!("rev-{c}"));
        assert_eq!(describe("10.1.0", c), "v10.1.0");
        assert_eq!(describe("v10.1.0", c), "v10.1.0");
        assert_eq!(describe("0.0.0", ""), "0.0.0");
        assert!(unreleased("0.0.0") && !unreleased("0.0.01") && !unreleased("10.0.0"));
    }
}
