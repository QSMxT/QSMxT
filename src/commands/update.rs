use crate::cli::UpdateArgs;
use crate::error::QsmxtError;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::io::{self, Read, Write};
use std::process::Command;

const REPO: &str = "QSMxT/QSMxT";

/// Sent on every GitHub request: the API rejects requests that omit a User-Agent.
const USER_AGENT: &str = concat!("qsmxt/", env!("CARGO_PKG_VERSION"));

/// Read granularity while streaming a release archive.
const CHUNK: usize = 256 * 1024;

/// What the GitHub "latest release" endpoint tells us about a release.
#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub notes: String,
    pub html_url: String,
}

/// Fetches latest release info from the GitHub API.
pub fn fetch_latest_release() -> crate::Result<Release> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);

    let mut req = ureq::get(&url)
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

/// Strips a leading 'v' from a version string if present.
fn strip_v(s: &str) -> &str {
    s.strip_prefix('v').unwrap_or(s)
}

/// The version this binary was built as. Release CI stamps it from the git tag;
/// a plain `cargo build` leaves the [`DEV_VERSION`] placeholder.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The version in Cargo.toml, which release CI overwrites from the git tag. A build
/// carrying it is a local/dev build, and is never told it is out of date — every
/// release would otherwise look newer than it.
const DEV_VERSION: &str = "0.0.0";

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
    let (Some((l_core, l_pre)), Some((c_core, c_pre))) =
        (parse_version(latest), parse_version(current))
    else {
        return false;
    };

    match l_core.cmp(&c_core) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        // Same numbers: only a release is newer than a pre-release of itself.
        std::cmp::Ordering::Equal => l_pre.is_none() && c_pre.is_some(),
    }
}

/// Streams `url` into `dest`, reporting `(downloaded, total)` as bytes arrive.
///
/// Mirrors the example-dataset downloader: `total` is 0 when the server sends no
/// Content-Length, which callers render as an unbounded spinner rather than a bar.
fn download_to(
    url: &str,
    dest: &std::path::Path,
    on_progress: &mut dyn FnMut(u64, u64),
) -> crate::Result<()> {
    let response = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| QsmxtError::Update(format!("Failed to download {}: {}", url, e)))?;

    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let mut file = std::fs::File::create(dest).map_err(|e| {
        QsmxtError::Update(format!("Failed to create {}: {}", dest.display(), e))
    })?;
    let mut reader = response.into_reader();
    let mut buf = vec![0u8; CHUNK];
    let mut downloaded = 0u64;

    on_progress(0, total);
    loop {
        let n = reader.read(&mut buf).map_err(|e| {
            QsmxtError::Update(format!("Download interrupted after {} bytes: {}", downloaded, e))
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| {
            QsmxtError::Update(format!("Failed writing {}: {}", dest.display(), e))
        })?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    file.sync_all().map_err(|e| {
        QsmxtError::Update(format!("Failed writing {}: {}", dest.display(), e))
    })?;

    // A short body means the connection dropped cleanly mid-transfer. ureq normally
    // catches that while reading, so this is a backstop for a response whose length we
    // learn only at the end; either way it beats unpacking a truncated archive over
    // the running binary.
    if total > 0 && downloaded != total {
        return Err(QsmxtError::Update(format!(
            "Incomplete download: expected {} bytes, received {}",
            total, downloaded
        )));
    }
    Ok(())
}

/// Detects the install directory (directory containing the current executable).
fn install_dir() -> crate::Result<std::path::PathBuf> {
    let exe = env::current_exe().map_err(|e| {
        QsmxtError::Update(format!("Cannot determine current executable path: {}", e))
    })?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| QsmxtError::Update("Cannot determine install directory".to_string()))
}

/// Detects OS/arch target triple (matching install.sh conventions).
fn detect_target() -> crate::Result<&'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { return Ok("x86_64-unknown-linux-musl"); }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { return Ok("aarch64-unknown-linux-gnu"); }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { return Ok("x86_64-apple-darwin"); }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { return Ok("aarch64-apple-darwin"); }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { return Ok("x86_64-pc-windows-msvc"); }

    #[allow(unreachable_code)]
    Err(QsmxtError::Update(format!(
        "Unsupported platform: {} {}",
        env::consts::OS,
        env::consts::ARCH,
    )))
}

/// Downloads and installs the release for the given tag.
fn install_release(tag: &str) -> crate::Result<()> {
    let target = detect_target()?;
    let dir = install_dir()?;

    #[cfg(target_os = "windows")]
    let archive_ext = "zip";
    #[cfg(not(target_os = "windows"))]
    let archive_ext = "tar.gz";

    let url = format!(
        "https://github.com/{}/releases/download/{}/qsmxt-{}-{}.{}",
        REPO, tag, tag, target, archive_ext
    );

    // Create temp dir
    let tmp = tempfile::tempdir().map_err(|e| {
        QsmxtError::Update(format!("Failed to create temp directory: {}", e))
    })?;

    let archive_path = tmp.path().join(format!("qsmxt.{}", archive_ext));

    // Download, drawing a progress bar. The release archive is tens of MB, so a
    // silent download looks like a hang on a slow connection.
    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template(
            "  {msg} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=> "),
    );
    bar.set_message(format!("qsmxt {}", tag));

    println!("Downloading qsmxt {}...", tag);
    download_to(&url, &archive_path, &mut |done, total| {
        // A server that sends no Content-Length leaves the length at 0, which
        // indicatif renders as a spinner-style bar rather than a misleading 100%.
        if total > 0 && bar.length() != Some(total) {
            bar.set_length(total);
        }
        bar.set_position(done);
    })?;
    bar.finish_and_clear();

    // Extract
    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("tar")
            .args(["xzf"])
            .arg(&archive_path)
            .arg("-C")
            .arg(tmp.path())
            .status()
            .map_err(|e| QsmxtError::Update(format!("Failed to extract archive: {}", e)))?;

        if !status.success() {
            return Err(QsmxtError::Update("Failed to extract archive".to_string()));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, use PowerShell to extract
        let status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}'",
                    archive_path.display(),
                    tmp.path().display()
                ),
            ])
            .status()
            .map_err(|e| QsmxtError::Update(format!("Failed to extract archive: {}", e)))?;

        if !status.success() {
            return Err(QsmxtError::Update("Failed to extract archive".to_string()));
        }
    }

    // Determine binary name
    #[cfg(target_os = "windows")]
    let bin_name = "qsmxt.exe";
    #[cfg(not(target_os = "windows"))]
    let bin_name = "qsmxt";

    let extracted = tmp.path().join(bin_name);
    let dest = dir.join(bin_name);

    if !extracted.exists() {
        return Err(QsmxtError::Update(format!(
            "Expected binary '{}' not found in archive",
            bin_name
        )));
    }

    // Install the new binary.
    //
    // We can't write over `dest` in place: when qsmxt is updating itself,
    // `dest` is the currently-running executable, and truncating/copying over
    // it fails with ETXTBSY ("Text file busy"). Instead we stage the new binary
    // in the *destination directory* (so it lands on the same filesystem) and
    // then `rename` it into place. rename(2) is atomic and is permitted even
    // while the old binary is still running — the running process keeps the old
    // inode and the path is repointed at the new file.
    let staged = dir.join(format!(".{}.new", bin_name));
    let _ = std::fs::remove_file(&staged); // discard any leftover from a prior run
    #[cfg(target_os = "windows")]
    {
        // Remove the old binary a previous self-update left behind (it was
        // locked while that qsmxt process was still running).
        let _ = std::fs::remove_file(dir.join(format!(".{}.old", bin_name)));
    }

    // Stage next to `dest`, falling back to sudo when the directory isn't
    // writable by the current user. std::fs::copy preserves the file mode, so
    // the staged binary keeps its executable bit.
    if std::fs::copy(&extracted, &staged).is_err() {
        #[cfg(not(target_os = "windows"))]
        {
            println!("Installing to {} (requires sudo)...", dir.display());
            let status = Command::new("sudo")
                .args(["cp", "-f"])
                .arg(&extracted)
                .arg(&staged)
                .status()
                .map_err(|e| {
                    QsmxtError::Update(format!("Failed to install with sudo: {}", e))
                })?;

            if !status.success() {
                return Err(QsmxtError::Update(
                    "Failed to stage binary (sudo cp failed)".to_string(),
                ));
            }
        }
        #[cfg(target_os = "windows")]
        {
            return Err(QsmxtError::Update(format!(
                "Failed to stage binary in {}",
                dir.display()
            )));
        }
    }

    // Atomically swap the staged binary into place. A plain rename only needs
    // write permission on the directory, so it succeeds even when `dest` is
    // owned by root from a prior sudo install; otherwise fall back to `sudo mv`.
    if std::fs::rename(&staged, &dest).is_err() {
        #[cfg(not(target_os = "windows"))]
        {
            let status = Command::new("sudo")
                .args(["mv", "-f"])
                .arg(&staged)
                .arg(&dest)
                .status()
                .map_err(|e| {
                    QsmxtError::Update(format!("Failed to install with sudo: {}", e))
                })?;

            if !status.success() {
                let _ = Command::new("sudo").args(["rm", "-f"]).arg(&staged).status();
                return Err(QsmxtError::Update(
                    "Failed to install binary (sudo mv failed)".to_string(),
                ));
            }
        }
        #[cfg(target_os = "windows")]
        {
            // Windows refuses to replace a running executable, but it does
            // allow *renaming* it. Move the running exe aside, then move the
            // staged binary into its place. The .old file stays locked until
            // this process exits; it is cleaned up on the next update.
            let old = dir.join(format!(".{}.old", bin_name));
            let _ = std::fs::remove_file(&old);
            if let Err(e) = std::fs::rename(&dest, &old) {
                let _ = std::fs::remove_file(&staged);
                return Err(QsmxtError::Update(format!(
                    "Failed to move current binary aside ({}): {}",
                    dest.display(),
                    e
                )));
            }
            if let Err(e) = std::fs::rename(&staged, &dest) {
                // Try to restore the old binary so the install isn't left broken.
                let _ = std::fs::rename(&old, &dest);
                let _ = std::fs::remove_file(&staged);
                return Err(QsmxtError::Update(format!(
                    "Failed to install binary to {}: {}",
                    dest.display(),
                    e
                )));
            }
        }
    }

    // Ensure executable on Unix
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&dest) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o111);
            let _ = std::fs::set_permissions(&dest, perms);
        }
    }

    // Install the bundled dcm2niix (if present in the archive) into ~/.qsmxt/bin,
    // mirroring the install scripts so `find_dcm2niix()` picks it up.
    install_bundled_dcm2niix(tmp.path());

    println!("Updated qsmxt to {} at {}", tag, dest.display());
    Ok(())
}

/// Copies the extracted dcm2niix (if any) into the qsmxt bin dir (`~/.qsmxt/bin`).
/// Best-effort: a missing dcm2niix (e.g. on ARM targets) or copy failure only
/// produces a warning, since the binary update itself already succeeded.
fn install_bundled_dcm2niix(extract_dir: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let dcm_name = "dcm2niix.exe";
    #[cfg(not(target_os = "windows"))]
    let dcm_name = "dcm2niix";

    let src = extract_dir.join(dcm_name);
    if !src.exists() {
        return; // no bundled dcm2niix for this target
    }

    let Some(bin_dir) = crate::dicom::convert::qsmxt_bin_dir() else {
        eprintln!("Warning: could not determine ~/.qsmxt/bin; skipping bundled dcm2niix install");
        return;
    };

    if let Err(e) = std::fs::create_dir_all(&bin_dir) {
        eprintln!("Warning: failed to create {}: {}", bin_dir.display(), e);
        return;
    }

    let dest = bin_dir.join(dcm_name);
    if let Err(e) = std::fs::copy(&src, &dest) {
        eprintln!("Warning: failed to install bundled dcm2niix to {}: {}", dest.display(), e);
        return;
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&dest) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o111);
            let _ = std::fs::set_permissions(&dest, perms);
        }
    }

    println!("Installed bundled dcm2niix to {}", dest.display());
}

pub fn execute(args: UpdateArgs) -> crate::Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");

    println!("Current version: {}", current_version);
    println!("Checking for updates...");

    let Release { tag: latest_tag, notes, html_url } = fetch_latest_release()?;

    // Compare numerically, not textually: a dev build or a version newer than the
    // latest release must not be offered a "downgrade" dressed up as an update.
    if !is_newer(&latest_tag, current_version) {
        println!("You are already running the latest version ({}).", current_version);
        return Ok(());
    }

    println!("New version available: {} -> {}", current_version, latest_tag);

    if !html_url.is_empty() {
        println!("Release: {}", html_url);
    }

    if !notes.is_empty() {
        println!();
        println!("Release notes:");
        println!("{}", notes);
        println!();
    }

    let should_update = if args.yes {
        true
    } else {
        print!("Do you want to update? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    };

    if should_update {
        install_release(&latest_tag)?;
    } else {
        println!("Update cancelled.");
    }

    Ok(())
}

/// Background update check used by the TUI.
///
/// Two things keep this off the startup path. The check runs on its own thread, so the
/// UI draws its first frame without waiting on the network; and the answer is cached on
/// disk for [`TTL_SECS`], so the common case is a single small file read and no HTTP at
/// all. Nothing here can fail the TUI: every error path degrades to "no update to show".
pub mod check {
    use super::{fetch_latest_release, is_dev_build, is_newer, current_version};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// How long a check result is reused before we ask GitHub again.
    const TTL_SECS: u64 = 24 * 60 * 60;

    /// Set to any non-empty value to disable the check entirely.
    pub const OPT_OUT_ENV: &str = "QSMXT_NO_UPDATE_CHECK";

    /// A release newer than what is running.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UpdateInfo {
        pub tag: String,
        pub html_url: String,
    }

    /// What we persist between runs: the last answer, when we got it, and any release
    /// the user asked not to be reminded about.
    #[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
    struct Cache {
        #[serde(default)]
        checked_at: u64,
        #[serde(default)]
        latest_tag: String,
        #[serde(default)]
        html_url: String,
        /// A tag the user dismissed with "don't remind me"; suppressed until a newer
        /// release supersedes it.
        #[serde(default)]
        dismissed_tag: String,
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

    /// Should we skip the check altogether?
    ///
    /// Dev builds carry the 0.0.0 placeholder, so every release would look newer; CI
    /// runs are not interactive and should not spend a request on this.
    pub fn is_disabled() -> bool {
        if std::env::var_os(OPT_OUT_ENV).is_some_and(|v| !v.is_empty()) {
            return true;
        }
        if std::env::var_os("CI").is_some_and(|v| !v.is_empty()) {
            return true;
        }
        is_dev_build()
    }

    /// Record that the user does not want reminding about `tag` again.
    pub fn dismiss(tag: &str) {
        let mut cache = load();
        cache.dismissed_tag = tag.to_string();
        store(&cache);
    }

    /// Has the user dismissed `tag`?
    pub fn is_dismissed(tag: &str) -> bool {
        !tag.is_empty() && load().dismissed_tag == tag
    }

    /// Start the check on a background thread.
    ///
    /// Returns `None` when checking is disabled. Otherwise the receiver yields exactly
    /// one message: `Some(info)` if a newer release exists, `None` if not (or if the
    /// check failed — an unreachable GitHub is not worth reporting to the user).
    pub fn spawn() -> Option<mpsc::Receiver<Option<UpdateInfo>>> {
        if is_disabled() {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(run());
        });
        Some(rx)
    }

    /// The check itself: serve from cache when it is fresh, otherwise ask GitHub.
    fn run() -> Option<UpdateInfo> {
        let cached = load();
        let age = now_secs().saturating_sub(cached.checked_at);
        if age < TTL_SECS && !cached.latest_tag.is_empty() {
            return newer_than_running(&cached.latest_tag, &cached.html_url);
        }

        let release = match fetch_latest_release() {
            Ok(r) => r,
            Err(e) => {
                // Offline, rate-limited, or GitHub is down: stay silent.
                log::debug!("update check failed: {e}");
                return None;
            }
        };

        store(&Cache {
            checked_at: now_secs(),
            latest_tag: release.tag.clone(),
            html_url: release.html_url.clone(),
            dismissed_tag: cached.dismissed_tag,
        });

        newer_than_running(&release.tag, &release.html_url)
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

        /// Write a cache entry `age_secs` old and point the check at it.
        fn seed(dir: &std::path::Path, age_secs: u64, tag: &str) {
            let path = dir.join("update-check.json");
            std::env::set_var("QSMXT_UPDATE_CACHE", &path);
            store(&Cache {
                checked_at: now_secs().saturating_sub(age_secs),
                latest_tag: tag.to_string(),
                html_url: format!("https://example.com/{tag}"),
                dismissed_tag: String::new(),
            });
        }

        /// The point of the cache: a launch inside the TTL answers from disk and makes no
        /// HTTP request at all. The sentinel tag proves it — a real fetch would return the
        /// actual latest release, which is not v99.0.0.
        #[test]
        fn a_fresh_cache_answers_without_touching_the_network() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            seed(dir.path(), 60, "v99.0.0");

            let got = run().expect("a newer release should be reported");
            assert_eq!(got.tag, "v99.0.0", "the answer must come from the cache");
            assert_eq!(got.html_url, "https://example.com/v99.0.0");

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// A cached release that is not newer than what is running yields nothing, so the
        /// TUI stays silent rather than prompting every launch for 24 hours.
        #[test]
        fn a_fresh_cache_of_an_older_release_reports_nothing() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            seed(dir.path(), 60, "v0.0.0");

            assert!(run().is_none());

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }

        /// Dismissing a release must not discard the cached result, or the next launch
        /// would spend an HTTP request re-learning what it already knew.
        #[test]
        fn dismissing_preserves_the_cached_result() {
            let _guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            seed(dir.path(), 60, "v99.0.0");

            dismiss("v99.0.0");

            let cache = load();
            assert_eq!(cache.latest_tag, "v99.0.0", "the check result should survive");
            assert_ne!(cache.checked_at, 0, "the timestamp should survive");
            assert_eq!(cache.dismissed_tag, "v99.0.0");

            std::env::remove_var("QSMXT_UPDATE_CACHE");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared with the TUI tests: `$QSMXT_UPDATE_CACHE` is process-global.
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

    #[test]
    fn a_dismissed_tag_is_remembered_and_superseded_by_a_newer_one() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("QSMXT_UPDATE_CACHE", dir.path().join("update-check.json"));

        assert!(!check::is_dismissed("v9.23.0"), "nothing dismissed yet");

        check::dismiss("v9.23.0");
        assert!(check::is_dismissed("v9.23.0"));
        // A later release is a different tag, so the user hears about it again.
        assert!(!check::is_dismissed("v9.24.0"));

        std::env::remove_var("QSMXT_UPDATE_CACHE");
    }

    #[test]
    fn dismiss_survives_a_missing_cache_directory() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        // Two levels that do not exist yet — dismiss must create them, not silently fail.
        let path = dir.path().join("a").join("b").join("update-check.json");
        std::env::set_var("QSMXT_UPDATE_CACHE", &path);

        check::dismiss("v9.23.0");
        assert!(path.is_file(), "the cache file should have been created");
        assert!(check::is_dismissed("v9.23.0"));

        std::env::remove_var("QSMXT_UPDATE_CACHE");
    }

    #[test]
    fn a_corrupt_cache_file_is_ignored_rather_than_fatal() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        std::fs::write(&path, b"{ not json").unwrap();
        std::env::set_var("QSMXT_UPDATE_CACHE", &path);

        assert!(!check::is_dismissed("v9.23.0"));
        // And it can be overwritten with something valid.
        check::dismiss("v9.23.0");
        assert!(check::is_dismissed("v9.23.0"));

        std::env::remove_var("QSMXT_UPDATE_CACHE");
    }

    /// A loopback HTTP server serving a fixed body, so the download path can be
    /// exercised without reaching the network. Mirrors the one in `example::download`.
    struct Server {
        base: String,
        shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Server {
        fn serve(body: Vec<u8>, declared_len: usize) -> Self {
            use std::io::{BufRead, BufReader};
            use std::net::TcpListener;
            use std::sync::atomic::Ordering;
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stop = std::sync::Arc::clone(&shutdown);

            let handle = std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut sock, _)) => {
                            sock.set_nonblocking(false).ok();
                            let mut reader = BufReader::new(sock.try_clone().unwrap());
                            let mut line = String::new();
                            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                                if line == "\r\n" || line == "\n" {
                                    break;
                                }
                                line.clear();
                            }
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                declared_len
                            );
                            let _ = sock.write_all(head.as_bytes());
                            let _ = sock.write_all(&body);
                            let _ = sock.flush();
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self { base, shutdown, handle: Some(handle) }
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    #[test]
    fn download_to_writes_the_body_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("archive.tar.gz");
        let body = vec![7u8; 300 * 1024]; // spans more than one CHUNK read
        let server = Server::serve(body.clone(), body.len());

        let mut seen: Vec<(u64, u64)> = Vec::new();
        download_to(&server.base, &dest, &mut |done, total| seen.push((done, total))).unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(seen.first(), Some(&(0, body.len() as u64)), "starts at zero of the total");
        assert_eq!(
            seen.last(),
            Some(&(body.len() as u64, body.len() as u64)),
            "finishes at the total"
        );
        assert!(seen.len() > 2, "progress should be reported as chunks arrive, got {:?}", seen);
    }

    #[test]
    fn download_to_rejects_a_truncated_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("archive.tar.gz");
        // Declares 500 bytes, sends 10: a connection that dropped cleanly mid-transfer.
        // Installing that would mean unpacking a truncated archive over the binary.
        let server = Server::serve(vec![1u8; 10], 500);

        let err = download_to(&server.base, &dest, &mut |_, _| {}).unwrap_err();
        let msg = err.to_string();
        // ureq enforces Content-Length itself, so this surfaces as an interrupted read
        // rather than the size check below it; either way the update is aborted and
        // the message says how far it got.
        assert!(msg.contains("interrupted") || msg.contains("Incomplete download"), "{msg}");
        assert!(msg.contains("10 bytes"), "the message should say how far it got: {msg}");
    }

    #[test]
    fn download_to_reports_an_http_error_rather_than_writing_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("archive.tar.gz");
        // Nothing is listening on this port.
        let err = download_to("http://127.0.0.1:1/nope", &dest, &mut |_, _| {}).unwrap_err();
        assert!(err.to_string().contains("Failed to download"), "{err}");
        assert!(!dest.exists(), "a failed download must not leave a file behind");
    }
}
