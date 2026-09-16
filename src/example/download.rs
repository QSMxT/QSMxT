//! Fetch an example zip into a local cache, verifying its SHA-256.
//!
//! Mirrors the model-weight downloader in qsm-core (`models::download`), with one
//! difference: these archives are 100-225 MB, so the body is streamed to a `.part` file
//! and hashed as it arrives rather than buffered in memory.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::Example;
use crate::error::QsmxtError;

/// Max HTTP attempts for one archive (1 initial + 4 retries).
const MAX_ATTEMPTS: u32 = 5;

/// Read granularity while streaming the body.
const CHUNK: usize = 256 * 1024;

/// Where downloaded archives are kept between runs.
///
/// `$QSMXT_EXAMPLE_CACHE` overrides everything; otherwise the platform cache dir
/// (`$XDG_CACHE_HOME` or `$LOCALAPPDATA` on Windows, else `$HOME/.cache`).
pub fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("QSMXT_EXAMPLE_CACHE") {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(dir).join("qsmxt").join("examples");
    }
    #[cfg(windows)]
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(dir).join("qsmxt").join("examples");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("qsmxt").join("examples");
    }
    PathBuf::from(".qsmxt-examples")
}

/// The path an example's archive occupies in the cache, whether or not it exists.
pub fn cache_path(example: &Example) -> PathBuf {
    cache_dir().join(format!("{}.zip", example.id))
}

/// Lowercase hex SHA-256 of a file, read incrementally.
fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let mut out = String::with_capacity(64);
    for b in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    Ok(out)
}

/// Ensure `example`'s archive is present locally, downloading it if needed, and return
/// its path. `on_progress(downloaded, total)` is called as bytes arrive; it is not
/// called at all on a cache hit.
///
/// A cached file whose checksum does not match the registry is treated as stale and
/// re-fetched — that covers a half-written file left by a killed process as well as an
/// archive replaced upstream.
pub fn ensure(
    example: &Example,
    on_progress: &mut dyn FnMut(u64, u64),
) -> crate::Result<PathBuf> {
    let dest = cache_path(example);

    if dest.is_file() {
        match sha256_file(&dest) {
            Ok(got) if got == example.sha256 => {
                log::debug!("using cached {}", dest.display());
                return Ok(dest);
            }
            Ok(_) => log::warn!(
                "cached {} failed its checksum — re-downloading",
                dest.display()
            ),
            Err(e) => log::warn!("could not read cached {}: {e} — re-downloading", dest.display()),
        }
    }

    let dir = cache_dir();
    fs::create_dir_all(&dir)?;
    let part = dir.join(format!("{}.zip.part", example.id));

    let got = download_to(&example.url(), example.bytes, &part, on_progress).inspect_err(|_| {
        let _ = fs::remove_file(&part);
    })?;

    if got != example.sha256 {
        let _ = fs::remove_file(&part);
        return Err(QsmxtError::Example(format!(
            "checksum mismatch for '{}': expected {}, got {}. \
             The archive may have been replaced upstream or the download corrupted.",
            example.id, example.sha256, got
        )));
    }

    fs::rename(&part, &dest)?;
    Ok(dest)
}

/// Stream `url` into `dest`, returning the SHA-256 of what was written.
///
/// OSF throttles bursts of anonymous downloads with a **403** rather than a 429, so a
/// 403 is retried like a rate limit; a 404/401 fails immediately. Backoff is
/// 0.5s/1s/2s/4s plus jitter derived from the URL, so concurrent fetchers of different
/// archives desynchronise. Each attempt restarts from byte zero, so progress resets.
fn download_to(
    url: &str,
    expected_total: u64,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64, u64),
) -> crate::Result<String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match try_download(url, expected_total, dest, on_progress) {
            Ok(hash) => return Ok(hash),
            Err(e) if e.retryable && attempt < MAX_ATTEMPTS => {
                let base = 500u64 << (attempt - 1);
                let jitter = (url.bytes().map(u64::from).sum::<u64>() % 250) + attempt as u64 * 37;
                log::warn!(
                    "{} (attempt {attempt}/{MAX_ATTEMPTS}) — retrying in {:.1}s",
                    e.message,
                    (base + jitter) as f64 / 1000.0
                );
                std::thread::sleep(std::time::Duration::from_millis(base + jitter));
            }
            Err(e) => return Err(QsmxtError::Example(e.message)),
        }
    }
}

struct Attempt {
    message: String,
    retryable: bool,
}

fn try_download(
    url: &str,
    expected_total: u64,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64, u64),
) -> std::result::Result<String, Attempt> {
    let response = ureq::get(url).call().map_err(|e| match e {
        // A 403 here is OSF's anonymous-download throttle, not an authorization failure.
        ureq::Error::Status(code, _) => Attempt {
            message: format!("HTTP {code} fetching {url}"),
            retryable: matches!(code, 403 | 408 | 425 | 429) || (500..600).contains(&code),
        },
        ureq::Error::Transport(t) => Attempt {
            message: format!("transport error fetching {url}: {t}"),
            retryable: true,
        },
    })?;

    let total = if expected_total > 0 {
        expected_total
    } else {
        response
            .header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };

    let io_err = |e: std::io::Error| Attempt {
        message: format!("writing {}: {e}", dest.display()),
        retryable: false,
    };

    let mut file = fs::File::create(dest).map_err(io_err)?;
    let mut reader = response.into_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut downloaded = 0u64;

    on_progress(0, total);
    loop {
        let n = reader.read(&mut buf).map_err(|e| Attempt {
            message: format!("download interrupted after {downloaded} bytes: {e}"),
            retryable: true,
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(io_err)?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    file.sync_all().map_err(io_err)?;

    // A short body means the connection dropped cleanly mid-transfer; that is worth
    // another attempt, and catching it here gives a clearer message than a checksum
    // mismatch would.
    if expected_total > 0 && downloaded != expected_total {
        return Err(Attempt {
            message: format!("expected {expected_total} bytes, received {downloaded}"),
            retryable: true,
        });
    }

    let mut out = String::with_capacity(64);
    for b in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate the cache-dir environment variables.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn cache_dir_prefers_the_explicit_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("QSMXT_EXAMPLE_CACHE", "/tmp/qsmxt-example-test");
        assert_eq!(cache_dir(), PathBuf::from("/tmp/qsmxt-example-test"));
        std::env::remove_var("QSMXT_EXAMPLE_CACHE");
    }

    #[test]
    fn cache_path_is_named_for_the_example() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("QSMXT_EXAMPLE_CACHE", "/tmp/qsmxt-example-test");
        let p = cache_path(super::super::default_example());
        assert_eq!(p.file_name().unwrap(), "prisma-bridge-run1.zip");
        std::env::remove_var("QSMXT_EXAMPLE_CACHE");
    }

    #[test]
    fn sha256_file_matches_a_known_vector() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("empty");
        fs::write(&f, b"").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        fs::write(&f, b"abc").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
