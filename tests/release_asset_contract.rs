//! Release-asset contract guard: every `qsmxt update` ever shipped must keep working.
//!
//! The updaters in v9.0.0 through v9.22.0 are frozen in users' binaries. They do not
//! list a release's assets; they ask `GET /repos/QSMxT/QSMxT/releases/latest` for
//! `tag_name`, treat any tag different from their own version as an update, and then
//! fetch one hard-coded URL:
//!
//! ```text
//! https://github.com/QSMxT/QSMxT/releases/download/<tag>/qsmxt-<tag>-<target>.<ext>
//! ```
//!
//! with `<ext>` = `tar.gz` (extracted with `tar xzf`) or, on Windows, `zip` (extracted
//! with `Expand-Archive`). They then expect `qsmxt` / `qsmxt.exe` at the archive ROOT
//! (and copy `dcm2niix` / `dcm2niix.exe` from the root into ~/.qsmxt/bin if present).
//! The targets they ask for are fixed per platform (see `OLD_UPDATER_TARGETS`).
//!
//! So `.github/workflows/release.yml` must keep producing exactly those archive names,
//! formats and layout for those targets, or every older install is stranded with no way
//! to update. Extra assets (such as `SHA256SUMS`) are invisible to old updaters and are
//! fine. This test is textual: it reads release.yml and fails if the packaging lines old
//! updaters depend on change. If you must change them, keep publishing a compatible
//! archive alongside the new one.

const RELEASE_YML: &str = include_str!("../.github/workflows/release.yml");
const UPDATE_RS: &str = include_str!("../src/commands/update.rs");

/// Targets the v9.0.0..=v9.22.0 updaters request (their `detect_target()`): Linux x86_64
/// asks for musl, Linux aarch64 for gnu. Windows on ARM was never supported by them.
const OLD_UPDATER_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

fn matrix_targets() -> Vec<String> {
    RELEASE_YML
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- target:"))
        .map(|t| t.trim().to_string())
        .collect()
}

/// Lines of the named step's `run:` block (up to the next step).
fn step_body(name: &str) -> String {
    let start = RELEASE_YML
        .find(&format!("- name: {}", name))
        .unwrap_or_else(|| panic!("release.yml has no step named {:?}", name));
    let rest = &RELEASE_YML[start + 1..];
    let end = rest.find("\n      - ").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn release_builds_every_target_old_updaters_request() {
    let targets = matrix_targets();
    for t in OLD_UPDATER_TARGETS {
        assert!(
            targets.iter().any(|m| m == t),
            "release.yml no longer builds {t}; qsmxt v9.x installs on that platform \
             request qsmxt-<tag>-{t}.* and could never update again"
        );
    }
}

#[test]
fn current_updater_targets_are_built() {
    // The updater in this tree must also only ask for archives the release publishes.
    let mut found = 0;
    for line in UPDATE_RS.lines() {
        if let Some(rest) = line.split("return Ok(\"").nth(1) {
            let target = rest.split('"').next().unwrap();
            found += 1;
            assert!(
                matrix_targets().iter().any(|m| m == target),
                "src/commands/update.rs requests {target}, which release.yml does not build"
            );
        }
    }
    assert!(found >= OLD_UPDATER_TARGETS.len(), "could not find detect_target() in update.rs");
}

#[test]
fn unix_archive_name_format_and_layout_are_stable() {
    let body = step_body("Package binary (Unix)");
    assert!(
        body.contains("cd target/${{ matrix.target }}/release"),
        "Unix packaging must archive from the release dir so files sit at the archive root"
    );
    assert!(
        body.contains("FILES=\"qsmxt\""),
        "the qsmxt binary must be at the archive root (old updaters look for <root>/qsmxt)"
    );
    assert!(
        body.contains("tar czf ../../../qsmxt-${{ github.ref_name }}-${{ matrix.target }}.tar.gz $FILES"),
        "Unix archives must stay gzip tarballs named qsmxt-<tag>-<target>.tar.gz"
    );
}

#[test]
fn windows_archive_name_format_and_layout_are_stable() {
    let body = step_body("Package binary (Windows)");
    assert!(body.contains("cd target/${{ matrix.target }}/release"));
    assert!(
        body.contains("FILES=\"qsmxt.exe\""),
        "qsmxt.exe must be at the archive root (old updaters look for <root>/qsmxt.exe)"
    );
    assert!(
        body.contains("7z a ../../../qsmxt-${{ github.ref_name }}-${{ matrix.target }}.zip $FILES"),
        "Windows archives must stay zip files named qsmxt-<tag>-<target>.zip"
    );
}

#[test]
fn archives_are_uploaded_under_those_names() {
    let body = step_body("Upload release asset");
    for ext in ["tar.gz", "zip"] {
        let name = format!("qsmxt-${{{{ github.ref_name }}}}-${{{{ matrix.target }}}}.{ext}");
        assert!(body.contains(&name), "Upload release asset no longer uploads {name}");
    }
}

#[test]
fn binary_version_is_stamped_from_the_tag() {
    // Old updaters compare CARGO_PKG_VERSION with the latest tag by string equality
    // (after stripping a leading 'v'). An unstamped binary would be offered the same
    // release forever.
    let body = step_body("Set version from release tag");
    assert!(body.contains("VERSION=\"${GITHUB_REF_NAME#v}\""));
    assert!(body.contains("Cargo.toml"));
}

#[test]
fn archive_name_in_updater_matches_release() {
    // The current updater builds the same name; keep both sides in lockstep.
    assert!(
        UPDATE_RS.contains("format!(\"qsmxt-{}-{}.{}\", tag, target, ext)"),
        "src/commands/update.rs archive_name() changed; release.yml and every v9.x \
         updater use qsmxt-<tag>-<target>.<ext>"
    );
}
