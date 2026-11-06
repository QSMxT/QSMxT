use std::fs;

/// What `Cargo.lock` says about the qsm-core (QSM.rs) dependency.
struct QsmCore {
    /// Full 40-character commit hash; `None` for a git source without one.
    commit: Option<String>,
    /// The tag it is pinned to, when it is pinned by tag.
    tag: Option<String>,
    /// Repository URL without the `git+` prefix, the query or `.git`; `None` for a local
    /// `path =` dependency (which has no `source` line).
    repo: Option<String>,
}

/// Find the `qsm-core` package in `Cargo.lock` and read its `source` line, e.g.
/// `source = "git+https://github.com/astewartau/QSM.rs.git?rev=<rev>#<full hash>"`.
fn qsm_core_from_lock(lock: &str) -> Option<QsmCore> {
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "name = \"qsm-core\"" {
            continue;
        }
        for line in lines.by_ref() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("[[package]]") {
                break;
            }
            let Some(source) = line.strip_prefix("source = \"").and_then(|s| s.strip_suffix('"')) else {
                continue;
            };
            let source = source.strip_prefix("git+").unwrap_or(source);
            let (url, commit) = source.split_once('#').unwrap_or((source, ""));
            let (repo, query) = url.split_once('?').unwrap_or((url, ""));
            let tag = query.split('&').find_map(|kv| kv.strip_prefix("tag=")).map(str::to_string);
            let commit = (commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit()))
                .then(|| commit.to_string());
            return Some(QsmCore { commit, tag, repo: Some(repo.trim_end_matches(".git").to_string()) });
        }
        return Some(QsmCore { commit: None, tag: None, repo: None });
    }
    None
}

fn main() {
    let lock = fs::read_to_string("Cargo.lock").expect("build.rs: cannot read Cargo.lock");
    let core = qsm_core_from_lock(&lock).expect("build.rs: no qsm-core package in Cargo.lock");

    // The exact QSM.rs commit behind every map has to be recoverable from the outputs, so a git
    // dependency without a full commit hash fails the build rather than recording "unknown".
    let (commit, version, describe, code_url) = match (&core.repo, &core.commit) {
        // A local `path =` dependency, used while a QSM.rs change is unreleased.
        (None, _) => ("local".to_string(), "local".to_string(), "local".to_string(), String::new()),
        (Some(repo), Some(commit)) => {
            let version = core.tag.clone().unwrap_or_else(|| format!("rev-{commit}"));
            let describe = match &core.tag {
                Some(tag) => format!("{tag} ({commit})"),
                None => version.clone(),
            };
            (commit.clone(), version, describe, format!("{repo}/tree/{commit}"))
        }
        (Some(repo), None) => panic!(
            "build.rs: Cargo.lock pins qsm-core from {repo} without a full commit hash; \
             provenance needs one — pin it by `rev` or `tag` and let cargo resolve the lock"
        ),
    };
    // Full 40-character commit (or "local").
    println!("cargo:rustc-env=QSM_CORE_GIT_HASH={commit}");
    // The tag when pinned by tag, else `rev-<full commit>` (or "local").
    println!("cargo:rustc-env=QSM_CORE_VERSION={version}");
    // For people: `<tag> (<commit>)`, or `rev-<commit>`.
    println!("cargo:rustc-env=QSM_CORE_DESCRIBE={describe}");
    // Browsable source at exactly that commit (empty for a local dependency).
    println!("cargo:rustc-env=QSM_CORE_CODE_URL={code_url}");

    // The QSMxT commit a `qsmxt update --main` build was made from, which that command passes
    // in; empty for release and local builds. Anything but a full commit hash is dropped.
    let build_commit = std::env::var("QSMXT_BUILD_COMMIT").unwrap_or_default();
    let build_commit = if build_commit.len() == 40 && build_commit.chars().all(|c| c.is_ascii_hexdigit()) {
        build_commit
    } else {
        String::new()
    };
    println!("cargo:rustc-env=QSMXT_BUILD_COMMIT={build_commit}");
    println!("cargo:rerun-if-env-changed=QSMXT_BUILD_COMMIT");

    // The QSMxT commit this build was made from, so an unreleased build (version 0.0.0 until the
    // release workflow sets it from the tag) can say which one it is: the commit `qsmxt update
    // --main` passed in, else the source checkout's own, else empty (a source tarball).
    let source_commit = if build_commit.is_empty() { git(&["rev-parse", "HEAD"]).unwrap_or_default() } else { build_commit };
    let source_commit = if source_commit.len() == 40 && source_commit.chars().all(|c| c.is_ascii_hexdigit()) {
        source_commit
    } else {
        String::new()
    };
    println!("cargo:rustc-env=QSMXT_SOURCE_COMMIT={source_commit}");
    // Rebuild the string when the checkout moves: HEAD, the branch it points to, packed refs.
    let mut watched = vec!["HEAD".to_string(), "packed-refs".to_string()];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(branch);
    }
    for path in watched {
        if let Some(path) = git(&["rev-parse", "--path-format=absolute", "--git-path", &path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

/// `git <args>` in the source directory, trimmed; `None` when git or the repository is missing.
fn git(args: &[&str]) -> Option<String> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}
