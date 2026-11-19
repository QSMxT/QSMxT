use crate::cli::UpdateArgs;
use crate::error::QsmxtError;
pub use qsmxt_io::version::{describe_version, SOURCE_COMMIT};
use qsmxt_io::version::{strip_v, unreleased};
// The release lookup and the TUI's check live in qsmxt-io; under the same paths here.
pub use qsmxt_io::release::{current_version, fetch_latest_release, is_newer, Release, DEV_VERSION, REPO, USER_AGENT};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::io::{self, Read, Write};
use std::process::Command;

/// Read granularity while streaming a release archive.
const CHUNK: usize = 256 * 1024;

/// The main-branch commit this binary was built from by `qsmxt update --main`; empty for
/// release and local builds.
pub const BUILD_COMMIT: &str = env!("QSMXT_BUILD_COMMIT");

/// Appended to the version in `qsmxt --version`, so a main build, or any unreleased build from a
/// checkout, says which commit it is.
pub const BUILD_COMMIT_SUFFIX: &str = if !BUILD_COMMIT.is_empty() {
    const_format::concatcp!(" (main ", BUILD_COMMIT, ")")
} else if !SOURCE_COMMIT.is_empty() && unreleased(env!("CARGO_PKG_VERSION")) {
    const_format::concatcp!(" (rev ", SOURCE_COMMIT, ")")
} else {
    ""
};

/// The repository `qsmxt update --main` builds from.
const GIT_URL: &str = "https://github.com/QSMxT/QSMxT";

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

/// Name of the checksum manifest the release workflow attaches to every release.
/// One `<sha256>  <asset>` line per archive, in `sha256sum` format.
pub const CHECKSUMS_ASSET: &str = "SHA256SUMS";

/// Where a release's assets are downloaded from.
fn release_download_base(tag: &str) -> String {
    format!("https://github.com/{}/releases/download/{}", REPO, tag)
}

/// The archive name the release workflow publishes for `tag` on `target`.
fn archive_name(tag: &str, target: &str, ext: &str) -> String {
    format!("qsmxt-{}-{}.{}", tag, target, ext)
}

/// Looks up `asset` in a `SHA256SUMS` manifest and returns its lowercase hex digest.
///
/// Accepts the `sha256sum` text (`<hex>  <name>`) and binary (`<hex> *<name>`) forms and
/// ignores blank lines. A malformed line is an error rather than something to skip: a
/// manifest we cannot read is not one we can trust. So is an asset listed twice with
/// different digests.
pub fn expected_sha256(manifest: &str, asset: &str) -> crate::Result<String> {
    let mut found: Option<String> = None;
    for (i, line) in manifest.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let malformed = || {
            QsmxtError::Update(format!(
                "{} line {} is malformed (expected '<sha256>  <file>'): {:?}",
                CHECKSUMS_ASSET,
                i + 1,
                line
            ))
        };
        let (hex, rest) = line.split_once(' ').ok_or_else(malformed)?;
        let name = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('*')).ok_or_else(malformed)?;
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) || name.is_empty() {
            return Err(malformed());
        }
        if name == asset {
            let hex = hex.to_ascii_lowercase();
            if let Some(prev) = &found {
                if *prev != hex {
                    return Err(QsmxtError::Update(format!(
                        "{} lists {} twice with different checksums; refusing to install",
                        CHECKSUMS_ASSET, asset
                    )));
                }
            }
            found = Some(hex);
        }
    }
    found.ok_or_else(|| {
        QsmxtError::Update(format!(
            "{} for this release has no entry for {}; refusing to install an unverified binary",
            CHECKSUMS_ASSET, asset
        ))
    })
}

/// Fetches the release's `SHA256SUMS` manifest as text.
fn fetch_checksums(url: &str) -> crate::Result<String> {
    let response = ureq::get(url).set("User-Agent", USER_AGENT).call().map_err(|e| {
        QsmxtError::Update(format!(
            "Could not download {} ({}): {}. Refusing to install an unverified binary",
            CHECKSUMS_ASSET, url, e
        ))
    })?;
    // The manifest is a few hundred bytes; cap the read so a hostile server can't make
    // us buffer an unbounded body.
    let mut text = String::new();
    response
        .into_reader()
        .take(1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| QsmxtError::Update(format!("Failed to read {}: {}", CHECKSUMS_ASSET, e)))?;
    Ok(text)
}

/// Downloads `asset` from `base` into `dest` and verifies it against the release's
/// `SHA256SUMS` before returning.
///
/// The manifest is fetched first, so a release without one (or without an entry for this
/// archive) is refused before the archive is downloaded at all. On a checksum mismatch the
/// archive is deleted and an error returned; nothing has been extracted at that point.
fn download_verified(
    base: &str,
    asset: &str,
    dest: &std::path::Path,
    on_progress: &mut dyn FnMut(u64, u64),
) -> crate::Result<()> {
    let manifest = fetch_checksums(&format!("{}/{}", base, CHECKSUMS_ASSET))?;
    let expected = expected_sha256(&manifest, asset)?;

    download_to(&format!("{}/{}", base, asset), dest, on_progress)?;

    let actual = crate::example::download::sha256_file(dest).map_err(|e| {
        QsmxtError::Update(format!("Failed to checksum {}: {}", dest.display(), e))
    })?;
    if actual != expected {
        let _ = std::fs::remove_file(dest);
        return Err(QsmxtError::Update(format!(
            "Checksum mismatch for {}: {} says {}, the download is {}. \
             The archive was discarded and nothing was installed",
            asset, CHECKSUMS_ASSET, expected, actual
        )));
    }
    Ok(())
}

/// Downloads and installs the release for the given tag.
fn install_release(tag: &str) -> crate::Result<()> {
    let target = detect_target()?;

    #[cfg(target_os = "windows")]
    let archive_ext = "zip";
    #[cfg(not(target_os = "windows"))]
    let archive_ext = "tar.gz";

    let asset = archive_name(tag, target, archive_ext);

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
    let downloaded = download_verified(&release_download_base(tag), &asset, &archive_path, &mut |done, total| {
        // A server that sends no Content-Length leaves the length at 0, which
        // indicatif renders as a spinner-style bar rather than a misleading 100%.
        if total > 0 && bar.length() != Some(total) {
            bar.set_length(total);
        }
        bar.set_position(done);
    });
    bar.finish_and_clear();
    downloaded?;
    println!("Verified {} against the release's {}.", asset, CHECKSUMS_ASSET);

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
    if !extracted.exists() {
        return Err(QsmxtError::Update(format!(
            "Expected binary '{}' not found in archive",
            bin_name
        )));
    }

    let dest = install_binary(&extracted)?;

    // Install the bundled dcm2niix (if present in the archive) into ~/.qsmxt/bin,
    // mirroring the install scripts so `find_dcm2niix()` picks it up.
    install_bundled_dcm2niix(tmp.path());

    println!("Updated qsmxt to {} at {}", tag, dest.display());
    Ok(())
}

/// Replaces the running qsmxt with the binary at `extracted`, and returns where it went.
fn install_binary(extracted: &std::path::Path) -> crate::Result<std::path::PathBuf> {
    let dir = install_dir()?;

    #[cfg(target_os = "windows")]
    let bin_name = "qsmxt.exe";
    #[cfg(not(target_os = "windows"))]
    let bin_name = "qsmxt";

    let dest = dir.join(bin_name);

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
    if std::fs::copy(extracted, &staged).is_err() {
        #[cfg(not(target_os = "windows"))]
        {
            println!("Installing to {} (requires sudo)...", dir.display());
            let status = Command::new("sudo")
                .args(["cp", "-f"])
                .arg(extracted)
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

    Ok(dest)
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
    if args.main {
        return execute_main_with(args.yes, BUILD_COMMIT, find_cargo, fetch_main_commit, build_main, |built| {
            install_binary(built).map(|_| ())
        });
    }
    execute_with(args, current_version(), fetch_latest_release, install_release)
}

/// Finds `cargo`: on `PATH`, or where rustup puts it (`~/.cargo/bin`), which a shell
/// opened before rustup was installed doesn't have on its `PATH` yet.
fn find_cargo() -> Option<std::path::PathBuf> {
    let works = |cargo: &std::path::Path| {
        Command::new(cargo)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let on_path = std::path::PathBuf::from("cargo");
    if works(&on_path) {
        return Some(on_path);
    }
    let cargo_home = env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
            Some(std::path::PathBuf::from(home).join(".cargo"))
        })?;
    let rustup_cargo = cargo_home.join("bin").join(if cfg!(windows) { "cargo.exe" } else { "cargo" });
    works(&rustup_cargo).then_some(rustup_cargo)
}

/// Is `s` a full 40-character git commit hash?
fn is_commit_hash(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Asks GitHub for the commit at the head of main.
fn fetch_main_commit() -> crate::Result<String> {
    let url = format!("https://api.github.com/repos/{}/commits/main", REPO);
    let mut req = ureq::get(&url)
        // Asks for the bare commit hash rather than the whole commit as JSON.
        .set("Accept", "application/vnd.github.sha")
        .set("User-Agent", USER_AGENT);
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        req = req.set("Authorization", &format!("token {}", token));
    }
    let sha = req
        .call()
        .map_err(|e| QsmxtError::Update(format!("Failed to look up the main branch on GitHub: {}", e)))?
        .into_string()
        .map_err(|e| QsmxtError::Update(format!("Failed to read GitHub API response: {}", e)))?;
    let sha = sha.trim().to_ascii_lowercase();
    if !is_commit_hash(&sha) {
        return Err(QsmxtError::Update(format!("GitHub returned an unexpected commit for main: {:?}", sha)));
    }
    Ok(sha)
}

/// The `cargo install` arguments that build `commit` into `root/bin`, keeping cargo's build
/// in `target` (else cargo builds in a directory of its own that it deletes afterwards).
///
/// `--locked` builds with main's own Cargo.lock, so the result has exactly the QSM.rs
/// commit main pins. The ONNX engine doesn't build for Windows on ARM, so that build
/// leaves the deep-learning models out, as its release build does.
fn cargo_install_args(commit: &str, root: &std::path::Path, target: Option<&std::path::Path>) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = ["install", "--git", GIT_URL, "--rev", commit, "--locked", "--root"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.push(root.as_os_str().to_owned());
    if let Some(target) = target {
        args.push("--target-dir".into());
        args.push(target.as_os_str().to_owned());
    }
    if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        args.push("--no-default-features".into());
    }
    args.push("qsmxt".into());
    args
}

/// Where `qsmxt update --main` keeps cargo's build between updates, `~/.qsmxt/build`, so an
/// update rebuilds only what changed: QSMxT itself, and QSM.rs when main pins another commit.
/// The dependencies (the ONNX engine among them) are built once. `$QSMXT_BUILD_CACHE`
/// overrides it. Without a home directory, cargo builds from scratch as it does by default.
fn build_cache_dir() -> Option<std::path::PathBuf> {
    if let Some(p) = env::var_os("QSMXT_BUILD_CACHE") {
        return Some(p.into());
    }
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
    Some(std::path::PathBuf::from(home).join(".qsmxt").join("build"))
}

/// The build cache's size above which it is emptied before a build.
const BUILD_CACHE_LIMIT_BYTES: u64 = 10 << 30;

/// Keep the build cache from growing without bound. Each commit is checked out at a path of its
/// own, so cargo never reuses QSMxT's own crates from an earlier update: their outputs are
/// removed before each build (they are rebuilt either way). Outputs of dependencies that are no
/// longer used (an earlier QSM.rs pin) still collect, so a cache past
/// [`BUILD_CACHE_LIMIT_BYTES`] is emptied and rebuilt once.
fn prune_build_cache(dir: &std::path::Path) {
    fn size(p: &std::path::Path) -> u64 {
        match std::fs::symlink_metadata(p) {
            Ok(m) if m.is_dir() => std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| size(&e.path())).sum(),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    if size(dir) > BUILD_CACHE_LIMIT_BYTES {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    let ours = |name: &str| ["qsmxt-", "qsmxt_engine-", "libqsmxt_engine-"].iter().any(|p| name.starts_with(p));
    for sub in ["deps", ".fingerprint", "build", "incremental"] {
        for e in std::fs::read_dir(dir.join("release").join(sub)).into_iter().flatten().flatten() {
            if e.file_name().to_str().is_some_and(ours) {
                let p = e.path();
                let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            }
        }
    }
}

/// Builds `commit` with `cargo install` into a temporary directory, then hands the built
/// binary to `install`. Cargo's output goes straight to the terminal; its build is kept in
/// [`build_cache_dir`] for the next update.
fn build_main(
    cargo: &std::path::Path,
    commit: &str,
    install: &mut dyn FnMut(&std::path::Path) -> crate::Result<()>,
) -> crate::Result<()> {
    let root = tempfile::tempdir()
        .map_err(|e| QsmxtError::Update(format!("Failed to create temp directory: {}", e)))?;
    // With the temporary bin directory on cargo's PATH, cargo doesn't tell the user to add
    // it to theirs: the binary is moved out of it before anyone could run it from there.
    let mut path = vec![root.path().join("bin")];
    path.extend(env::var_os("PATH").iter().flat_map(env::split_paths));
    let path = env::join_paths(path)
        .map_err(|e| QsmxtError::Update(format!("Failed to set PATH for cargo: {}", e)))?;
    let cache = build_cache_dir();
    if let Some(dir) = &cache {
        prune_build_cache(dir);
    }
    let status = Command::new(cargo)
        .args(cargo_install_args(commit, root.path(), cache.as_deref()))
        .env("QSMXT_BUILD_COMMIT", commit)
        .env("PATH", path)
        .status()
        .map_err(|e| QsmxtError::Update(format!("Failed to run {}: {}", cargo.display(), e)))?;
    if !status.success() {
        return Err(QsmxtError::Update(
            "Building main failed (see cargo's output above); the installed qsmxt was left as it was".to_string(),
        ));
    }
    let built = root.path().join("bin").join(if cfg!(windows) { "qsmxt.exe" } else { "qsmxt" });
    if !built.exists() {
        return Err(QsmxtError::Update(format!("cargo finished but {} is missing", built.display())));
    }
    install(&built)
}

/// `qsmxt update --main`, with the toolchain lookup, the GitHub lookup, the build and the
/// installer injected, so the decision logic can be tested without any of them.
fn execute_main_with(
    yes: bool,
    running_commit: &str,
    find_cargo: impl FnOnce() -> Option<std::path::PathBuf>,
    fetch_commit: impl FnOnce() -> crate::Result<String>,
    build: impl FnOnce(&std::path::Path, &str, &mut dyn FnMut(&std::path::Path) -> crate::Result<()>) -> crate::Result<()>,
    mut install: impl FnMut(&std::path::Path) -> crate::Result<()>,
) -> crate::Result<()> {
    if running_commit.is_empty() {
        println!("Current version: {}", current_version());
    } else {
        println!("Current version: main at {}", running_commit);
    }

    // Before the network: with no toolchain there is nothing to do with the answer.
    let cargo = find_cargo().ok_or_else(|| {
        QsmxtError::Update(
            "`qsmxt update --main` builds QSMxT from source, which needs the Rust toolchain. \
             Install it from https://rustup.rs, then run this again."
                .to_string(),
        )
    })?;

    println!("Checking the main branch...");
    let commit = fetch_commit()?;

    if commit == running_commit {
        println!("You are already running the latest main ({}).", commit);
        return Ok(());
    }

    println!("Latest main: {} ({}/commit/{})", commit, GIT_URL, commit);

    let should_update = if yes {
        true
    } else {
        print!("Build it from source and install it in place of this qsmxt? This takes several minutes. [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    };
    if !should_update {
        println!("Update cancelled.");
        return Ok(());
    }

    build(&cargo, &commit, &mut install)?;
    println!("Updated qsmxt to main at {}. Run `qsmxt update --force` to go back to the latest release.", commit);
    Ok(())
}

/// [`execute`] with the version, the release lookup and the installer injected, so the
/// decision logic can be tested without the network.
fn execute_with(
    args: UpdateArgs,
    current_version: &str,
    fetch: impl FnOnce() -> crate::Result<Release>,
    install: impl FnOnce(&str) -> crate::Result<()>,
) -> crate::Result<()> {
    let dev = strip_v(current_version) == DEV_VERSION;

    println!("Current version: {}", current_version);

    // A dev build carries the 0.0.0 placeholder, so every release looks newer. Replacing
    // a locally built binary with a release is rarely what was meant; refuse before
    // touching the network unless the user asked for it.
    if dev && !args.force {
        return Err(QsmxtError::Update(format!(
            "This is a development build (version {}), not a release, so there is nothing \
             to compare against. Refusing to replace it; run `qsmxt update --force` to \
             install the latest release over it, or `qsmxt update --main` to build the \
             latest main.",
            DEV_VERSION
        )));
    }

    println!("Checking for updates...");

    let Release { tag: latest_tag, notes, html_url } = fetch()?;

    // Compare numerically, not textually: a version newer than the latest release must
    // not be offered a "downgrade" dressed up as an update. A forced dev build skips the
    // comparison: the user asked for the latest release whatever it is.
    if !dev && !is_newer(&latest_tag, current_version) {
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
        install(&latest_tag)?;
    } else {
        println!("Update cancelled.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    impl Server {
        /// Serves `bodies` in order, one per request (the last repeats). Lets one base
        /// URL answer the manifest request and then the archive request.
        fn serve_seq(bodies: Vec<Vec<u8>>) -> Self {
            use std::io::{BufRead, BufReader};
            use std::net::TcpListener;
            use std::sync::atomic::Ordering;
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stop = std::sync::Arc::clone(&shutdown);
            let handle = std::thread::spawn(move || {
                let mut next = 0usize;
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
                            let body = &bodies[next.min(bodies.len() - 1)];
                            next += 1;
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = sock.write_all(head.as_bytes());
                            let _ = sock.write_all(body);
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

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    const ASSET: &str = "qsmxt-v9.23.0-x86_64-unknown-linux-musl.tar.gz";

    #[test]
    fn checksum_manifest_lookup_accepts_text_and_binary_forms() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let manifest = format!(
            "{a}  qsmxt-v9.23.0-aarch64-apple-darwin.tar.gz\r\n\n{b} *{ASSET}\n"
        );
        assert_eq!(expected_sha256(&manifest, ASSET).unwrap(), "b".repeat(64));
        assert_eq!(
            expected_sha256(&manifest, "qsmxt-v9.23.0-aarch64-apple-darwin.tar.gz").unwrap(),
            a
        );
    }

    #[test]
    fn checksum_manifest_without_the_asset_is_refused() {
        let manifest = format!("{}  qsmxt-v9.23.0-x86_64-apple-darwin.tar.gz\n", "a".repeat(64));
        let err = expected_sha256(&manifest, ASSET).unwrap_err().to_string();
        assert!(err.contains("no entry for") && err.contains(ASSET), "{err}");
        // A prefix of the name is not the name.
        let manifest = format!("{}  {}.sig\n", "a".repeat(64), ASSET);
        assert!(expected_sha256(&manifest, ASSET).is_err());
        assert!(expected_sha256("", ASSET).is_err(), "an empty manifest verifies nothing");
    }

    #[test]
    fn a_malformed_or_contradictory_checksum_manifest_is_refused() {
        for bad in [
            format!("{} {}\n", "a".repeat(64), ASSET),          // single space, no mode char
            format!("{}  {}\n", "a".repeat(63), ASSET),         // short digest
            format!("{}  {}\n", "g".repeat(64), ASSET),         // not hex
            "<html>404 Not Found</html>\n".to_string(),         // an error page
        ] {
            let err = expected_sha256(&bad, ASSET).unwrap_err().to_string();
            assert!(err.contains("malformed"), "{bad:?}: {err}");
        }
        let twice = format!("{}  {ASSET}\n{}  {ASSET}\n", "a".repeat(64), "b".repeat(64));
        let err = expected_sha256(&twice, ASSET).unwrap_err().to_string();
        assert!(err.contains("twice"), "{err}");
    }

    /// The acceptance test for #281: an archive whose bytes don't match the release's
    /// SHA256SUMS is refused, deleted, and never extracted.
    #[test]
    fn a_tampered_archive_is_refused_before_anything_is_extracted() {
        let genuine = b"the genuine release archive".to_vec();
        let tampered = b"the tampered release archive".to_vec();
        let manifest = format!("{}  {}\n", sha256_hex(&genuine), ASSET).into_bytes();

        let archive = Server::serve(tampered.clone(), tampered.len());
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("qsmxt.tar.gz");

        // A server that returns the archive bytes for every request: what it serves as
        // the manifest is garbage, so that is refused before the archive is fetched...
        let err = download_verified(&archive.base, ASSET, &dest, &mut |_, _| {}).unwrap_err();
        assert!(err.to_string().contains("malformed"), "{err}");
        assert!(!dest.exists(), "the archive must not even be downloaded");

        // ...and with a valid manifest naming the genuine digest, the tampered archive is
        // downloaded, found not to match, and discarded.
        let both = Server::serve_seq(vec![manifest, tampered]);
        let err = download_verified(&both.base, ASSET, &dest, &mut |_, _| {}).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Checksum mismatch") && msg.contains(ASSET), "{msg}");
        assert!(msg.contains(&sha256_hex(&genuine)), "the message names the expected digest: {msg}");
        assert!(!dest.exists(), "a mismatching archive must be deleted, not left to extract");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "nothing may be extracted or staged after a mismatch"
        );
    }

    #[test]
    fn a_genuine_archive_passes_verification() {
        let genuine = vec![42u8; 100 * 1024];
        let manifest = format!("{}  {}\n", sha256_hex(&genuine), ASSET).into_bytes();
        let server = Server::serve_seq(vec![manifest, genuine.clone()]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("qsmxt.tar.gz");
        download_verified(&server.base, ASSET, &dest, &mut |_, _| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), genuine);
    }

    #[test]
    fn a_release_without_a_checksum_manifest_is_refused() {
        // Nothing is listening: the manifest can't be fetched, so nothing is downloaded.
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("qsmxt.tar.gz");
        let err = download_verified("http://127.0.0.1:1", ASSET, &dest, &mut |_, _| {}).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(CHECKSUMS_ASSET) && msg.contains("unverified"), "{msg}");
        assert!(!dest.exists());
    }

    /// The acceptance test for #281's second half: a 0.0.0 build refuses and exits
    /// without asking GitHub anything.
    #[test]
    fn a_dev_build_refuses_to_update_without_touching_the_network() {
        let err = execute_with(
            crate::cli::UpdateArgs { yes: true, force: false, main: false },
            "0.0.0",
            || panic!("a dev build must not look up the latest release"),
            |_| panic!("a dev build must not install anything"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("development build") && err.contains("--force"), "{err}");
    }

    #[test]
    fn a_forced_dev_build_installs_the_latest_release() {
        let mut installed = None;
        execute_with(
            crate::cli::UpdateArgs { yes: true, force: true, main: false },
            "0.0.0",
            || Ok(Release { tag: "v9.23.0".into(), notes: String::new(), html_url: String::new() }),
            |tag| {
                installed = Some(tag.to_string());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(installed.as_deref(), Some("v9.23.0"));
    }

    #[test]
    fn a_release_build_is_not_offered_an_older_or_equal_release() {
        execute_with(
            crate::cli::UpdateArgs { yes: true, force: false, main: false },
            "9.23.0",
            || Ok(Release { tag: "v9.23.0".into(), notes: String::new(), html_url: String::new() }),
            |_| panic!("already up to date"),
        )
        .unwrap();
    }

    const MAIN: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn update_main_without_cargo_points_at_rustup_before_touching_the_network() {
        let err = execute_main_with(
            true,
            "",
            || None,
            || panic!("no toolchain: nothing to look up"),
            |_, _, _| panic!("no toolchain: nothing to build"),
            |_| panic!("no toolchain: nothing to install"),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("Rust toolchain") && err.contains("https://rustup.rs"), "{err}");
    }

    #[test]
    fn update_main_builds_the_latest_commit_and_installs_the_result() {
        let mut built = None;
        let mut installed = None;
        execute_main_with(
            true,
            "",
            || Some("/opt/cargo".into()),
            || Ok(MAIN.to_string()),
            |cargo, commit, install| {
                built = Some((cargo.to_path_buf(), commit.to_string()));
                install(std::path::Path::new("/tmp/build/bin/qsmxt"))
            },
            |bin| {
                installed = Some(bin.to_path_buf());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(built, Some(("/opt/cargo".into(), MAIN.to_string())));
        assert_eq!(installed, Some("/tmp/build/bin/qsmxt".into()));
    }

    #[test]
    fn update_main_does_not_rebuild_the_commit_already_running() {
        execute_main_with(
            true,
            MAIN,
            || Some("cargo".into()),
            || Ok(MAIN.to_string()),
            |_, _, _| panic!("already on main's latest commit"),
            |_| panic!("already on main's latest commit"),
        )
        .unwrap();
    }

    #[test]
    fn update_main_builds_that_exact_commit_from_the_lock_file() {
        let args = cargo_install_args(MAIN, std::path::Path::new("/tmp/root"), Some(std::path::Path::new("/tmp/cache")));
        let args: Vec<_> = args.iter().map(|a| a.to_str().unwrap()).collect();
        let expected_prefix =
            ["install", "--git", GIT_URL, "--rev", MAIN, "--locked", "--root", "/tmp/root", "--target-dir", "/tmp/cache"];
        assert_eq!(args[..expected_prefix.len()], expected_prefix);
        assert_eq!(args.last(), Some(&"qsmxt"));
        let args = cargo_install_args(MAIN, std::path::Path::new("/tmp/root"), None);
        assert!(!args.iter().any(|a| a == "--target-dir"), "no cache, cargo's own");
    }

    /// QSMxT's own outputs go before each build and the dependencies' stay; a cache past the
    /// limit is emptied.
    #[test]
    fn the_build_cache_keeps_the_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let deps = dir.path().join("release").join("deps");
        let prints = dir.path().join("release").join(".fingerprint");
        std::fs::create_dir_all(&deps).unwrap();
        std::fs::create_dir_all(prints.join("qsmxt-0123abcd")).unwrap();
        for f in ["qsmxt-0123abcd", "libqsmxt_engine-0123abcd.rlib", "libtract_core-0123abcd.rlib", "libqsm_core-0123abcd.rlib"] {
            std::fs::write(deps.join(f), b"x").unwrap();
        }
        prune_build_cache(dir.path());
        let left: Vec<String> = std::fs::read_dir(&deps).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        assert!(left.contains(&"libtract_core-0123abcd.rlib".to_string()) && left.contains(&"libqsm_core-0123abcd.rlib".to_string()), "{left:?}");
        assert!(!left.iter().any(|f| f.starts_with("qsmxt-") || f.starts_with("libqsmxt_engine-")), "{left:?}");
        assert!(!prints.join("qsmxt-0123abcd").exists());
    }

    #[test]
    fn update_main_and_force_cannot_be_combined() {
        use clap::Parser;
        assert!(crate::cli::Cli::try_parse_from(["qsmxt", "update", "--main", "--force"]).is_err());
        assert!(crate::cli::Cli::try_parse_from(["qsmxt", "update", "--main"]).is_ok());
    }
}
