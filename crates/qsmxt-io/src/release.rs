//! The QSMxT release this binary is and the latest one published: the version, the GitHub
//! release lookup and the TUI's background check for a newer release. `qsmxt update` installs.

use crate::error::QsmxtError;
use crate::version::strip_v;
use std::env;

/// The log target these messages have always had (they were the binary's `commands::update::check`).
const LOG_TARGET: &str = "qsmxt::commands::update::check";

pub const REPO: &str = "QSMxT/QSMxT";

/// Sent on every GitHub request: the API rejects requests that omit a User-Agent.
pub const USER_AGENT: &str = concat!("qsmxt/", env!("CARGO_PKG_VERSION"));

/// What the GitHub "latest release" endpoint tells us about a release.
#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub notes: String,
    pub html_url: String,
}

/// An HTTP client for the requests made on their own at TUI startup (the release check and
/// the announcements). Whatever the network does, a request gives up within a few seconds, so
/// no background thread is left waiting on a hung connection.
pub fn quick_agent() -> ureq::Agent {
    use std::time::Duration;
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .user_agent(USER_AGENT)
        .build()
}

/// Fetches latest release info from the GitHub API.
pub fn fetch_latest_release() -> crate::Result<Release> {
    fetch_latest_release_with(&ureq::agent())
}

/// [`fetch_latest_release`] through `agent`.
pub fn fetch_latest_release_with(agent: &ureq::Agent) -> crate::Result<Release> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);

    let mut req = agent.get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", USER_AGENT);
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        req = req.set("Authorization", &format!("token {}", token));
    }

    let body = req
        .call()
        .map_err(|e| {
            QsmxtError::Update(format!("Failed to fetch release info from GitHub: {}", e))
        })?
        .into_string()
        .map_err(|e| {
            QsmxtError::Update(format!("Failed to read GitHub API response: {}", e))
        })?;

    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        QsmxtError::Update(format!("Failed to parse GitHub API response: {}", e))
    })?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| QsmxtError::Update("No tag_name in release response".to_string()))?
        .to_string();

    Ok(Release {
        tag,
        notes: json["body"].as_str().unwrap_or("").to_string(),
        html_url: json["html_url"].as_str().unwrap_or("").to_string(),
    })
}

/// The version this binary was built as. Release CI stamps it from the git tag;
/// a plain `cargo build` leaves the [`DEV_VERSION`] placeholder.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The version in Cargo.toml, which release CI overwrites from the git tag. A build
/// carrying it is a local/dev build, and is never told it is out of date — every
/// release would otherwise look newer than it.
pub const DEV_VERSION: &str = "0.0.0";

pub fn is_dev_build() -> bool {
    strip_v(current_version()) == DEV_VERSION
}

/// Splits `vX.Y.Z-pre` into its numeric components and its pre-release suffix.
///
/// Missing components read as zero, so `9.2` and `9.2.0` compare equal. Anything
/// unparseable yields `None`, which callers treat as "can't tell" rather than guessing.
fn parse_version(s: &str) -> Option<([u64; 3], Option<String>)> {
    let s = strip_v(s.trim());
    // Build metadata (`+abc`) is not part of precedence; drop it before anything else.
    let s = s.split('+').next().unwrap_or(s);
    let (core, pre) = match s.split_once('-') {
        Some((core, pre)) => (core, Some(pre.to_string())),
        None => (s, None),
    };

    let mut parts = [0u64; 3];
    let mut seen = 0;
    for (i, piece) in core.split('.').enumerate() {
        if i >= 3 {
            return None; // not a version we understand; don't guess
        }
        parts[i] = piece.parse().ok()?;
        seen += 1;
    }
    if seen == 0 {
        return None;
    }
    Some((parts, pre))
}

/// Is `latest` a strictly newer version than `current`?
///
/// Returns false when either side is unparseable: an unrecognised tag must not be
/// allowed to nag the user into "updating" to something we cannot reason about. A
/// pre-release sorts below the matching release (`9.2.0-rc1` < `9.2.0`), per semver.
pub fn is_newer(latest: &str, current: &str) -> bool {
    compare_versions(latest, current) == Some(std::cmp::Ordering::Greater)
}

/// How version `a` compares with version `b`, or `None` when either is unparseable.
///
/// A pre-release sorts below the matching release (`9.2.0-rc1` < `9.2.0`), per semver; two
/// pre-releases of the same version compare equal.
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let ((a_core, a_pre), (b_core, b_pre)) = (parse_version(a)?, parse_version(b)?);
    Some(match a_core.cmp(&b_core) {
        // Same numbers: only a release is newer than a pre-release of itself.
        Ordering::Equal => match (a_pre.is_some(), b_pre.is_some()) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            _ => Ordering::Equal,
        },
        other => other,
    })
}

/// Background update check used by the TUI.
///
/// Every launch asks GitHub, so a release is announced the first time QSMxT opens after it is
/// published. The check runs on its own thread with short timeouts ([`super::quick_agent`]), so
/// the UI draws its first frame without waiting on the network, and nothing waits for the thread
/// at exit. The last answer is kept on disk only as the fallback for a launch that cannot reach
/// GitHub (offline, rate-limited). Nothing here can fail the TUI: every error path degrades to
/// "no update to show".
pub mod check {
    use super::{current_version, fetch_latest_release_with, is_dev_build, is_newer, quick_agent, Release};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Set to any non-empty value to disable the check entirely.
    pub const OPT_OUT_ENV: &str = "QSMXT_NO_UPDATE_CHECK";

    /// A release newer than what is running.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UpdateInfo {
        pub tag: String,
        pub html_url: String,
    }

    /// What we persist between runs: the last answer and when we got it. Fields this version
    /// no longer writes (an earlier `dismissed_tag`) are ignored when read.
    #[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
    struct Cache {
        #[serde(default)]
        checked_at: u64,
        #[serde(default)]
        latest_tag: String,
        #[serde(default)]
        html_url: String,
    }

    /// Serializes tests that point `$QSMXT_UPDATE_CACHE` at a temp file. The variable is
    /// process-global, so every test module that sets it must take *this* lock — the TUI
    /// tests do too. Mirrors `example::download::env_lock`.
    #[cfg(test)]
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(test)]
    pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn now_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }

    /// Where the cache lives: beside the bundled tools in `~/.qsmxt`.
    ///
    /// `$QSMXT_UPDATE_CACHE` overrides it, which is what the tests use to keep off the
    /// real home directory.
    pub fn cache_path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("QSMXT_UPDATE_CACHE") {
            return Some(PathBuf::from(p));
        }
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        Some(PathBuf::from(home).join(".qsmxt").join("update-check.json"))
    }

    fn load() -> Cache {
        let Some(path) = cache_path() else { return Cache::default() };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn store(cache: &Cache) {
        let Some(path) = cache_path() else { return };
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        if let Ok(json) = serde_json::to_string(cache) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Is the check turned off, by the user or for CI? (Announcements follow the same switch.)
    pub fn is_opted_out() -> bool {
        std::env::var_os(OPT_OUT_ENV).is_some_and(|v| !v.is_empty())
            || std::env::var_os("CI").is_some_and(|v| !v.is_empty())
    }

    /// Should we skip the check altogether?
    ///
    /// Dev builds carry the 0.0.0 placeholder, so every release would look newer; CI
    /// runs are not interactive and should not spend a request on this.
    pub fn is_disabled() -> bool {
        is_opted_out() || is_dev_build()
    }

    /// Start the check on a background thread.
    ///
    /// Returns `None` when checking is disabled. Otherwise the receiver yields exactly
    /// one message: `Some(info)` if a newer release exists, `None` if not (or if the
    /// check failed with nothing cached — an unreachable GitHub is not worth reporting).
    /// The thread is detached: nothing joins it, so a slow network never delays exit.
    pub fn spawn() -> Option<mpsc::Receiver<Option<UpdateInfo>>> {
        if is_disabled() {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(run(|| fetch_latest_release_with(&quick_agent())));
        });
        Some(rx)
    }

    /// The check itself: ask GitHub (through `fetch`) and remember the answer; when that
    /// fails, fall back to the last answer we had.
    fn run(fetch: impl FnOnce() -> crate::Result<Release>) -> Option<UpdateInfo> {
        match fetch() {
            Ok(release) => {
                store(&Cache {
                    checked_at: now_secs(),
                    latest_tag: release.tag.clone(),
                    html_url: release.html_url.clone(),
                });
                newer_than_running(&release.tag, &release.html_url)
            }
            Err(e) => {
                // Offline, rate-limited, or GitHub is down: use what we last heard, silently.
                log::debug!(target: super::LOG_TARGET, "update check failed: {e}");
                let cached = load();
                if cached.latest_tag.is_empty() {
                    return None;
                }
                newer_than_running(&cached.latest_tag, &cached.html_url)
            }
        }
    }

    fn newer_than_running(tag: &str, html_url: &str) -> Option<UpdateInfo> {
        is_newer(tag, current_version()).then(|| UpdateInfo {
            tag: tag.to_string(),
            html_url: html_url.to_string(),
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::error::QsmxtError;

        fn release(tag: &str) -> crate::Result<Release> {
            Ok(Release { tag: tag.into(), notes: String::new(), html_url: format!("https://example.com/{tag}") })
        }

        fn offline() -> crate::Result<Release> {
            Err(QsmxtError::Update("offline".into()))
        }

        /// Point the cache at a file in `dir` holding `tag`, checked a minute ago.
        fn seed(dir: &std::path::Path, tag: &str) {
            std::env::set_var("QSMXT_UPDATE_CACHE", dir.join("update-check.json"));
            store(&Cache {
                checked_at: now_secs().saturating_sub(60),
                latest_tag: tag.to_string(),
                html_url: format!("https://example.com/{tag}"),
            });
        }

        /// A release published since the last check is announced on the very next launch: a
        /// fresh cache does not stand in for asking GitHub.
        #[test]
        fn every_launch_asks_github_even_with_a_fresh_cache() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            seed(dir.path(), "v0.0.0");

            let got = run(|| release("v99.0.0")).expect("the new release should be reported");
            assert_eq!(got.tag, "v99.0.0");
            assert_eq!(load().latest_tag, "v99.0.0", "the answer is remembered for offline launches");

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// Offline, the last answer we had is used.
        #[test]
        fn an_unreachable_github_falls_back_to_the_cache() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            seed(dir.path(), "v99.0.0");

            let got = run(offline).expect("the cached release should be reported");
            assert_eq!(got.tag, "v99.0.0");
            assert_eq!(got.html_url, "https://example.com/v99.0.0");

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// Offline with nothing cached, or with a cached release that is not newer, there is
        /// nothing to say.
        #[test]
        fn offline_without_a_newer_cached_release_reports_nothing() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            std::env::set_var("QSMXT_UPDATE_CACHE", dir.path().join("update-check.json"));
            assert!(run(offline).is_none());
            seed(dir.path(), "v0.0.0");
            assert!(run(offline).is_none());
            assert!(run(|| release("v0.0.0")).is_none());

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// A cache written when a release could be skipped still loads; the skip no longer
        /// applies, so the release is offered.
        #[test]
        fn a_cache_with_a_skipped_release_still_loads() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("update-check.json");
            std::fs::write(
                &path,
                r#"{"checked_at":1,"latest_tag":"v99.0.0","html_url":"https://example.com/r","dismissed_tag":"v99.0.0"}"#,
            )
            .unwrap();
            std::env::set_var("QSMXT_UPDATE_CACHE", &path);

            assert_eq!(load().latest_tag, "v99.0.0");
            assert_eq!(run(offline).map(|i| i.tag).as_deref(), Some("v99.0.0"));

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// The cache directory is created when it does not exist yet.
        #[test]
        fn the_cache_is_written_into_a_missing_directory() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a").join("b").join("update-check.json");
            std::env::set_var("QSMXT_UPDATE_CACHE", &path);

            run(|| release("v9.23.0"));
            assert!(path.is_file(), "the cache file should have been created");

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// A corrupt cache is ignored, and replaced by the next answer.
        #[test]
        fn a_corrupt_cache_file_is_ignored_rather_than_fatal() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("update-check.json");
            std::fs::write(&path, b"{ not json").unwrap();
            std::env::set_var("QSMXT_UPDATE_CACHE", &path);

            assert!(run(offline).is_none());
            run(|| release("v9.23.0"));
            assert_eq!(load().latest_tag, "v9.23.0");

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// A request that can't connect gives up rather than hanging the thread: nothing is
        /// listening on port 1, and the agent's timeouts bound the wait either way.
        #[test]
        fn the_quick_agent_fails_fast() {
            let start = std::time::Instant::now();
            assert!(quick_agent().get("http://127.0.0.1:1/").call().is_err());
            assert!(start.elapsed() < std::time::Duration::from_secs(6));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared with the other tests here: `$QSMXT_UPDATE_CACHE` is process-global.
    use check::env_lock;

    #[test]
    fn is_newer_compares_numerically_not_textually() {
        // The bug a string compare has: 9.9.0 sorts after 9.10.0 lexically.
        assert!(is_newer("v9.10.0", "9.9.0"));
        assert!(!is_newer("v9.9.0", "9.10.0"));
        assert!(is_newer("v10.0.0", "9.22.0"));
        assert!(is_newer("v9.22.1", "9.22.0"));
        assert!(is_newer("v9.23.0", "9.22.9"));
    }

    #[test]
    fn is_newer_is_false_for_the_same_or_older_release() {
        assert!(!is_newer("v9.22.0", "9.22.0"));
        assert!(!is_newer("9.22.0", "v9.22.0"), "the leading v must not matter");
        assert!(!is_newer("v9.21.0", "9.22.0"));
        // Running something newer than the latest release (a local build of main)
        // must not be offered a downgrade.
        assert!(!is_newer("v9.22.0", "9.23.0"));
    }

    #[test]
    fn is_newer_treats_a_prerelease_as_below_its_release() {
        assert!(is_newer("v9.22.0", "9.22.0-rc1"));
        assert!(!is_newer("v9.22.0-rc1", "9.22.0"));
        assert!(is_newer("v9.23.0-rc1", "9.22.0"));
        // Build metadata is not part of precedence.
        assert!(!is_newer("v9.22.0+abc", "9.22.0"));
    }

    #[test]
    fn is_newer_refuses_to_guess_at_an_unparseable_tag() {
        // Better to stay quiet than to nag the user toward something we can't reason about.
        assert!(!is_newer("nightly", "9.22.0"));
        assert!(!is_newer("v9.22.0", "not-a-version"));
        assert!(!is_newer("v9.22.0.1", "9.22.0"), "four components is not a tag we know");
        assert!(!is_newer("", "9.22.0"));
    }

    #[test]
    fn compare_versions_orders_releases_and_prereleases() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("10.0.0", "v10.0.0"), Some(Equal));
        assert_eq!(compare_versions("9.10.0", "9.9.0"), Some(Greater));
        assert_eq!(compare_versions("11.0.1-rc1", "11.0.1"), Some(Less));
        assert_eq!(compare_versions("11.0.1-rc1", "11.0.1-rc2"), Some(Equal));
        assert_eq!(compare_versions("soon", "11.0.1"), None);
    }

    #[test]
    fn short_versions_pad_with_zeros() {
        assert!(!is_newer("v9.22", "9.22.0"));
        assert!(is_newer("v9.23", "9.22.5"));
    }

    #[test]
    fn a_dev_build_is_recognised_and_silenced_in_the_tui() {
        // Cargo.toml carries 0.0.0 and release CI overwrites it from the git tag.
        assert!(is_dev_build(), "the test binary is built from the unstamped Cargo.toml");

        // `is_newer` itself does not special-case that: running `qsmxt update` on a
        // local build is an explicit request and should install the real release.
        assert!(is_newer("v9.22.0", DEV_VERSION));

        // The silencing is the TUI's, so a dev build is never *nagged* unprompted.
        let _guard = env_lock();
        assert!(check::is_disabled());
    }

    #[test]
    fn the_check_is_disabled_for_dev_builds_and_on_opt_out() {
        let _guard = env_lock();
        // This test binary is a dev build, which is itself a disabling condition.
        assert!(check::is_disabled());

        std::env::set_var(check::OPT_OUT_ENV, "1");
        assert!(check::is_disabled());
        std::env::remove_var(check::OPT_OUT_ENV);
    }
}
