use crate::cli::UpdateArgs;
use crate::error::QsmxtError;
use std::env;
use std::io::{self, Write};
use std::process::Command;

const REPO: &str = "QSMxT/QSMxT";

/// Fetches latest release info from GitHub API. Returns (tag, release_notes, html_url).
fn fetch_latest_release() -> crate::Result<(String, String, String)> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);

    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "-H", "Accept: application/vnd.github+json"]);
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        cmd.args(["-H", &format!("Authorization: token {}", token)]);
    }
    cmd.arg(&url);

    let output = cmd.output().map_err(|e| {
        QsmxtError::Update(format!("Failed to run curl: {}", e))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(QsmxtError::Update(format!(
            "Failed to fetch release info from GitHub: {}",
            stderr.trim()
        )));
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        QsmxtError::Update(format!("Failed to parse GitHub API response: {}", e))
    })?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| QsmxtError::Update("No tag_name in release response".to_string()))?
        .to_string();

    let notes = json["body"].as_str().unwrap_or("").to_string();
    let html_url = json["html_url"].as_str().unwrap_or("").to_string();

    Ok((tag, notes, html_url))
}

/// Strips a leading 'v' from a version string if present.
fn strip_v(s: &str) -> &str {
    s.strip_prefix('v').unwrap_or(s)
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

    println!("Downloading qsmxt {}...", tag);

    // Create temp dir
    let tmp = tempfile::tempdir().map_err(|e| {
        QsmxtError::Update(format!("Failed to create temp directory: {}", e))
    })?;

    let archive_path = tmp.path().join(format!("qsmxt.{}", archive_ext));

    // Download
    let status = Command::new("curl")
        .args(["-fsSL", &url, "-o"])
        .arg(&archive_path)
        .status()
        .map_err(|e| QsmxtError::Update(format!("Failed to run curl: {}", e)))?;

    if !status.success() {
        return Err(QsmxtError::Update(format!(
            "Failed to download release from {}",
            url
        )));
    }

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

    let (latest_tag, notes, html_url) = fetch_latest_release()?;
    let latest_version = strip_v(&latest_tag);

    if latest_version == strip_v(current_version) {
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
