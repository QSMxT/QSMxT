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


/// Serializes tests that mutate the download-related environment variables
/// (`$QSMXT_EXAMPLE_CACHE`, `$QSMXT_EXAMPLE_BASE_URL`). Shared with the registry's
/// URL tests, which set the same variables.
#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the environment lock used by tests that set those variables.
#[cfg(test)]
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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
    if let Some(path) = cached(example) {
        log::debug!("using cached {}", path.display());
        return Ok(path);
    }

    let dir = cache_dir();
    fs::create_dir_all(&dir)?;
    let part = part_path(example);

    let got = download_to(&example.url(), example.bytes, &part, on_progress).inspect_err(|_| {
        let _ = fs::remove_file(&part);
    })?;

    install_verified(example, &part, &got)
}

/// The already-downloaded archive for `example`, if one is present and intact.
///
/// A cached file whose checksum does not match the registry is reported as absent, so the
/// caller re-fetches it — that covers a half-written file left by a killed process as well
/// as an archive replaced upstream.
pub fn cached(example: &Example) -> Option<PathBuf> {
    let dest = cache_path(example);
    if !dest.is_file() {
        return None;
    }
    match sha256_file(&dest) {
        Ok(got) if got == example.sha256 => Some(dest),
        Ok(_) => {
            log::warn!("cached {} failed its checksum — re-downloading", dest.display());
            None
        }
        Err(e) => {
            log::warn!("could not read cached {}: {e} — re-downloading", dest.display());
            None
        }
    }
}

/// Where a partial download lives before it is verified.
fn part_path(example: &Example) -> PathBuf {
    cache_dir().join(format!("{}.zip.part", example.id))
}

/// Move a verified `.part` file into its final cache slot, or delete it and report why.
///
/// `got` is the SHA-256 the download computed as the bytes arrived. Split out from
/// [`ensure`] so the verify-and-install decision is testable without a network fetch.
fn install_verified(example: &Example, part: &Path, got: &str) -> crate::Result<PathBuf> {
    if got != example.sha256 {
        let _ = fs::remove_file(part);
        return Err(QsmxtError::Example(format!(
            "checksum mismatch for '{}': expected {}, got {}. \
             The archive may have been replaced upstream or the download corrupted.",
            example.id, example.sha256, got
        )));
    }
    let dest = cache_path(example);
    fs::rename(part, &dest)?;
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
    use std::sync::atomic::Ordering;


    /// A registry entry pointing at a URL that is never reached: every test here
    /// exercises the cache, so a network hit would be a bug in the code under test.
    fn fake(sha: &'static str, bytes: u64) -> super::super::Example {
        super::super::Example {
            id: "test-acq",
            scanner: "prisma",
            acq: "bridge",
            run: 1,
            osf_id: "000000000000000000000000",
            bytes,
            sha256: sha,
        }
    }

    /// SHA-256 of b"abc".
    const ABC_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";


    /// A one-shot HTTP server on loopback that serves `body` to every request until
    /// dropped. Lets the download path — streaming, progress, retry classification,
    /// checksum verification — be exercised without reaching the internet.
    struct Server {
        base: String,
        shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Server {
        fn serve(body: Vec<u8>, status: &'static str) -> Self {
            use std::io::{BufRead, BufReader};
            use std::net::TcpListener;
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
                            // Drain the request head so the client is not left writing.
                            let mut reader = BufReader::new(sock.try_clone().unwrap());
                            let mut line = String::new();
                            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                                if line == "\r\n" || line == "\n" {
                                    break;
                                }
                                line.clear();
                            }
                            let head = format!(
                                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
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
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    #[test]
    fn ensure_downloads_verifies_and_caches_from_a_mirror() {
        let cache = CacheDir::new();
        let server = Server::serve(b"abc".to_vec(), "200 OK");
        std::env::set_var(super::super::BASE_URL_ENV, &server.base);

        let example = fake(ABC_SHA, 3);
        let mut seen: Vec<(u64, u64)> = Vec::new();
        let path = ensure(&example, &mut |done, total| seen.push((done, total))).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"abc");
        assert_eq!(path, cache.path().join("test-acq.zip"));
        assert_eq!(seen.first(), Some(&(0, 3)), "progress should start at zero of the total");
        assert_eq!(seen.last(), Some(&(3, 3)), "progress should finish at the total");
        assert!(!cache.path().join("test-acq.zip.part").exists());

        // A second call is served from the cache — the server is not consulted again.
        let mut calls = 0;
        ensure(&example, &mut |_, _| calls += 1).unwrap();
        assert_eq!(calls, 0);

        std::env::remove_var(super::super::BASE_URL_ENV);
    }

    #[test]
    fn ensure_rejects_a_mirror_serving_the_wrong_bytes() {
        let cache = CacheDir::new();
        let server = Server::serve(b"not the archive".to_vec(), "200 OK");
        std::env::set_var(super::super::BASE_URL_ENV, &server.base);

        // Declared size matches so the short-body guard does not fire first; the
        // checksum is what has to catch this.
        let example = fake(ABC_SHA, 15);
        let err = ensure(&example, &mut |_, _| {}).unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("checksum mismatch"), "{msg}");
        assert!(!cache.path().join("test-acq.zip").exists(), "bad bytes must not be cached");
        assert!(!cache.path().join("test-acq.zip.part").exists(), "no .part left behind");

        std::env::remove_var(super::super::BASE_URL_ENV);
    }

    #[test]
    fn ensure_detects_a_truncated_transfer() {
        let _cache = CacheDir::new();
        let server = Server::serve(b"ab".to_vec(), "200 OK");
        std::env::set_var(super::super::BASE_URL_ENV, &server.base);

        // Registry says 3 bytes, the mirror sends 2 — a connection that dropped cleanly
        // mid-transfer. Caught by size before the checksum, for a clearer message.
        let err = ensure(&fake(ABC_SHA, 3), &mut |_, _| {}).unwrap_err();
        assert!(err.to_string().contains("received"), "{err}");

        std::env::remove_var(super::super::BASE_URL_ENV);
    }

    #[test]
    fn a_permanent_http_error_is_not_retried() {
        let _cache = CacheDir::new();
        let server = Server::serve(Vec::new(), "404 Not Found");
        std::env::set_var(super::super::BASE_URL_ENV, &server.base);

        let started = std::time::Instant::now();
        let err = ensure(&fake(ABC_SHA, 3), &mut |_, _| {}).unwrap_err();

        assert!(err.to_string().contains("404"), "{err}");
        // Retrying a 404 would cost at least the 0.5+1+2+4s backoff ladder.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "a 404 should fail fast, took {:?}",
            started.elapsed()
        );

        std::env::remove_var(super::super::BASE_URL_ENV);
    }

    #[test]
    fn cached_reports_absent_when_the_checksum_does_not_match() {
        // A half-written file left by a killed process must not be trusted, and a cache
        // miss is how that gets re-fetched.
        let cache = CacheDir::new();
        let example = fake(ABC_SHA, 3);
        fs::write(cache.path().join("test-acq.zip"), b"truncated").unwrap();
        assert!(cached(&example).is_none());
    }

    #[test]
    fn cached_reports_absent_when_nothing_is_there() {
        let _cache = CacheDir::new();
        assert!(cached(&fake(ABC_SHA, 3)).is_none());
    }

    #[test]
    fn install_verified_moves_a_good_part_file_into_place() {
        let cache = CacheDir::new();
        let example = fake(ABC_SHA, 3);
        let part = cache.path().join("test-acq.zip.part");
        fs::write(&part, b"abc").unwrap();

        let dest = install_verified(&example, &part, ABC_SHA).unwrap();

        assert_eq!(dest, cache.path().join("test-acq.zip"));
        assert_eq!(fs::read(&dest).unwrap(), b"abc");
        assert!(!part.exists(), "the .part file should have been renamed away");
    }

    #[test]
    fn install_verified_rejects_and_deletes_a_mismatched_part_file() {
        let cache = CacheDir::new();
        let example = fake(ABC_SHA, 3);
        let part = cache.path().join("test-acq.zip.part");
        fs::write(&part, b"wrong bytes").unwrap();

        let err = install_verified(&example, &part, "deadbeef").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("checksum mismatch"), "{msg}");
        assert!(msg.contains(ABC_SHA), "the message should name the expected hash: {msg}");
        assert!(!part.exists(), "a corrupt .part file must not be left behind");
        assert!(!cache.path().join("test-acq.zip").exists(), "it must not be installed");
    }

    #[test]
    fn part_path_sits_beside_the_final_archive() {
        let cache = CacheDir::new();
        let example = fake(ABC_SHA, 3);
        assert_eq!(part_path(&example), cache.path().join("test-acq.zip.part"));
        assert_eq!(cache_path(&example), cache.path().join("test-acq.zip"));
    }

    struct CacheDir {
        _guard: std::sync::MutexGuard<'static, ()>,
        dir: tempfile::TempDir,
    }

    impl CacheDir {
        fn new() -> Self {
            let guard = super::env_lock();
            let dir = tempfile::tempdir().unwrap();
            std::env::set_var("QSMXT_EXAMPLE_CACHE", dir.path());
            Self { _guard: guard, dir }
        }
        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    impl Drop for CacheDir {
        fn drop(&mut self) {
            std::env::remove_var("QSMXT_EXAMPLE_CACHE");
        }
    }

    #[test]
    fn ensure_returns_a_good_cache_entry_without_downloading() {
        let cache = CacheDir::new();
        let example = fake(ABC_SHA, 3);
        fs::write(cache.path().join("test-acq.zip"), b"abc").unwrap();

        let mut progress_calls = 0;
        let path = ensure(&example, &mut |_, _| progress_calls += 1).unwrap();

        assert_eq!(path, cache.path().join("test-acq.zip"));
        assert_eq!(progress_calls, 0, "a cache hit must not report download progress");
    }



    #[test]
    fn cache_dir_falls_back_to_xdg_then_home() {
        let _guard = super::env_lock();
        std::env::remove_var("QSMXT_EXAMPLE_CACHE");
        let saved = std::env::var("XDG_CACHE_HOME").ok();

        std::env::set_var("XDG_CACHE_HOME", "/tmp/xdg-test");
        assert_eq!(cache_dir(), PathBuf::from("/tmp/xdg-test/qsmxt/examples"));

        std::env::remove_var("XDG_CACHE_HOME");
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(cache_dir(), PathBuf::from(home).join(".cache/qsmxt/examples"));
        }
        if let Some(v) = saved {
            std::env::set_var("XDG_CACHE_HOME", v);
        }
    }

    #[test]
    fn cache_dir_prefers_the_explicit_override() {
        let _guard = super::env_lock();
        std::env::set_var("QSMXT_EXAMPLE_CACHE", "/tmp/qsmxt-example-test");
        assert_eq!(cache_dir(), PathBuf::from("/tmp/qsmxt-example-test"));
        std::env::remove_var("QSMXT_EXAMPLE_CACHE");
    }

    #[test]
    fn cache_path_is_named_for_the_example() {
        let _guard = super::env_lock();
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
