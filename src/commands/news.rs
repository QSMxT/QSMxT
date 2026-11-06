//! Announcements ("news") the maintainers publish to QSMxT users.
//!
//! The announcements are `announcements.toml` at the root of the QSMxT repository (its header
//! describes the format, and how to publish one). The TUI fetches the file from GitHub on a
//! background thread at every start, with the same short timeouts as the release check, and
//! keeps the last copy it got in `~/.qsmxt/announcements.toml`, so the history can be read
//! offline. Which announcements have popped up already is kept in
//! `~/.qsmxt/announcements-seen.json`. `qsmxt news` prints them all.
//!
//! An ordinary announcement pops up once. A critical one pops up at every start while the
//! running version is in its `affects` range (or always, without one). An announcement with
//! `affects` is shown only to the versions in its range, and never to a development build (which
//! has no version to place in the range). Nothing here can fail the TUI: a network or parse error
//! means nothing new is shown.

use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

use super::update::{check, compare_versions, current_version, quick_agent};

/// Where the announcements are published: the file on QSMxT's main branch.
pub const DEFAULT_URL: &str = "https://raw.githubusercontent.com/QSMxT/QSMxT/main/announcements.toml";

/// Fetch the announcements from this URL instead (`file://` reads a local file), for testing an
/// announcement before it is published.
pub const URL_ENV: &str = "QSMXT_ANNOUNCEMENTS_URL";

/// Keep the saved announcements and the record of those already shown in this directory
/// instead of `~/.qsmxt`.
pub const DIR_ENV: &str = "QSMXT_ANNOUNCEMENTS_DIR";

/// The largest announcements file read; a hostile or broken server can't make us buffer more.
const MAX_BYTES: u64 = 1024 * 1024;

/// One announcement, as written in `announcements.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcement {
    /// Unique, never reused: it is what records the announcement as shown.
    pub id: String,
    /// `YYYY-MM-DD`; announcements are listed newest first.
    pub date: String,
    pub title: String,
    /// Plain text, wrapped to the screen; a blank line starts a new paragraph.
    pub body: String,
    #[serde(default)]
    pub link: Option<String>,
    /// Pops up at every start while it applies, rather than once.
    #[serde(default)]
    pub critical: bool,
    /// The versions it concerns; without it, every version.
    #[serde(default)]
    pub affects: Option<Affects>,
}

/// A version range, `from` (included) up to `fixed_in` (not included). Either end may be left
/// out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Affects {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub fixed_in: Option<String>,
}

/// The file: a list of `[[announcement]]` tables, possibly empty.
#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    announcement: Vec<Announcement>,
}

impl Announcement {
    /// Is `version` one this announcement is for? Always, without `affects`; otherwise only a
    /// released version inside the range (a development build, 0.0.0, is in none).
    pub fn applies_to(&self, version: &str) -> bool {
        let Some(affects) = &self.affects else { return true };
        if version.trim().trim_start_matches('v') == "0.0.0" {
            return false;
        }
        use std::cmp::Ordering::*;
        let after_from = affects.from.as_deref().is_none_or(|from| {
            matches!(compare_versions(version, from), Some(Greater | Equal))
        });
        let before_fix = affects.fixed_in.as_deref().is_none_or(|fixed| {
            compare_versions(version, fixed) == Some(Less)
        });
        after_from && before_fix
    }

    /// The range in words, e.g. "Affects versions from 10.0.0; fixed in 11.0.1."
    pub fn affects_text(&self) -> Option<String> {
        let a = self.affects.as_ref()?;
        match (&a.from, &a.fixed_in) {
            (Some(from), Some(fixed)) => Some(format!("Affects versions from {from}; fixed in {fixed}.")),
            (None, Some(fixed)) => Some(format!("Affects versions before {fixed}; fixed in {fixed}.")),
            (Some(from), None) => Some(format!("Affects versions from {from}.")),
            (None, None) => None,
        }
    }

    /// The body's paragraphs, each as one line of text (the caller wraps them).
    pub fn paragraphs(&self) -> Vec<String> {
        self.body
            .split("\n\n")
            .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|p| !p.is_empty())
            .collect()
    }

    /// What is wrong with it, if anything: a missing id or title, a date that is not
    /// `YYYY-MM-DD`, or a version that cannot be compared.
    fn problem(&self) -> Option<String> {
        if self.id.trim().is_empty() {
            return Some(format!("{:?} has no id", self.title));
        }
        if self.title.trim().is_empty() {
            return Some(format!("{} has no title", self.id));
        }
        if chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d").is_err() {
            return Some(format!("{}: date {:?} is not YYYY-MM-DD", self.id, self.date));
        }
        let versions = self.affects.iter().flat_map(|a| [&a.from, &a.fixed_in]).flatten();
        for v in versions {
            if compare_versions(v, v).is_none() {
                return Some(format!("{}: {v:?} is not a version", self.id));
            }
        }
        None
    }
}

/// Parse an announcements file: the announcements, newest first, and what was wrong with any
/// that were left out. A file that is not valid TOML of this shape is an error.
fn parse_all(text: &str) -> Result<(Vec<Announcement>, Vec<String>), String> {
    let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut ids = HashSet::new();
    let mut problems = Vec::new();
    let mut items = Vec::new();
    for a in file.announcement {
        if let Some(p) = a.problem() {
            problems.push(p);
        } else if !ids.insert(a.id.clone()) {
            problems.push(format!("{}: the id is used twice", a.id));
        } else {
            items.push(a);
        }
    }
    // Newest first; of the same date, the one further down the file first.
    items.reverse();
    items.sort_by(|a, b| b.date.cmp(&a.date));
    Ok((items, problems))
}

/// Parse an announcements file, leaving out (silently) any announcement that is not valid.
pub fn parse(text: &str) -> Result<Vec<Announcement>, String> {
    let (items, problems) = parse_all(text)?;
    for p in problems {
        log::debug!("announcement left out: {p}");
    }
    Ok(items)
}

/// The announcements to pop up for `version`, in order: critical first, then newest first.
/// Ordinary ones pop up once (until their id is in `seen`); critical ones whenever they apply.
pub fn pop_ups(all: &[Announcement], version: &str, seen: &HashSet<String>) -> Vec<Announcement> {
    let mut due: Vec<Announcement> = all
        .iter()
        .filter(|a| a.applies_to(version) && (a.critical || !seen.contains(&a.id)))
        .cloned()
        .collect();
    // A stable sort: of the same date, `all`'s order (newest first) stands.
    due.sort_by(|a, b| b.critical.cmp(&a.critical).then_with(|| b.date.cmp(&a.date)));
    due
}

/// What a fetch came back with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    /// Every announcement, newest first.
    pub items: Vec<Announcement>,
    /// Why the published file could not be read, when it could not; `items` are then the copy
    /// saved the last time it could.
    pub error: Option<String>,
}

/// The URL the announcements are fetched from: [`DEFAULT_URL`], unless [`URL_ENV`] is set.
pub fn url() -> String {
    std::env::var(URL_ENV).ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| DEFAULT_URL.to_string())
}

/// Read the announcements file at `url`: over HTTP(S) with short timeouts, or a local file for a
/// `file://` URL.
pub fn fetch(url: &str) -> Result<String, String> {
    let mut text = String::new();
    if let Some(path) = url.strip_prefix("file://") {
        std::fs::File::open(path)
            .and_then(|f| f.take(MAX_BYTES).read_to_string(&mut text))
            .map_err(|e| format!("{path}: {e}"))?;
        return Ok(text);
    }
    let response = quick_agent().get(url).call().map_err(|e| e.to_string())?;
    response
        .into_reader()
        .take(MAX_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    Ok(text)
}

/// Get the announcements through `fetch`, and save what it got; when that fails (or what it got
/// does not parse), use the copy saved last time.
pub fn load(fetch: impl FnOnce() -> Result<String, String>) -> Loaded {
    let fetched = fetch().and_then(|text| parse(&text).map(|items| (text, items)));
    match fetched {
        Ok((text, items)) => {
            store_cache(&text);
            Loaded { items, error: None }
        }
        Err(e) => {
            log::debug!("announcements unavailable: {e}");
            Loaded { items: load_cached(), error: Some(e) }
        }
    }
}

/// The announcements saved by the last successful fetch (none, if there was none).
pub fn load_cached() -> Vec<Announcement> {
    cache_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| parse(&text).ok())
        .unwrap_or_default()
}

/// Start fetching on a background thread, for the TUI. `None` when the user (or CI) has turned
/// the background checks off; development builds still fetch. The receiver yields one
/// [`Loaded`]. The thread is detached: nothing waits for it, at startup or at exit.
// The TUI's tests never fetch (see `NewsState::start`).
#[cfg_attr(test, allow(dead_code))]
pub fn spawn() -> Option<mpsc::Receiver<Loaded>> {
    if check::is_opted_out() {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let url = url();
    std::thread::spawn(move || {
        let _ = tx.send(load(|| fetch(&url)));
    });
    Some(rx)
}

/// `~/.qsmxt`, or [`DIR_ENV`].
fn dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(DIR_ENV) {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".qsmxt"))
}

fn cache_path() -> Option<PathBuf> {
    dir().map(|d| d.join("announcements.toml"))
}

fn seen_path() -> Option<PathBuf> {
    dir().map(|d| d.join("announcements-seen.json"))
}

fn write(path: Option<PathBuf>, contents: &str) {
    let Some(path) = path else { return };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let _ = std::fs::write(path, contents);
}

fn store_cache(text: &str) {
    write(cache_path(), text);
}

/// The record of the announcements that have popped up.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    #[serde(default)]
    seen: Vec<String>,
}

/// The ids of the announcements that have popped up already.
pub fn load_seen() -> HashSet<String> {
    seen_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Seen>(&s).ok())
        .map(|s| s.seen.into_iter().collect())
        .unwrap_or_default()
}

/// Record that announcement `id` has popped up, so an ordinary one does not again.
pub fn mark_seen(id: &str) {
    let mut seen = seen_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Seen>(&s).ok())
        .unwrap_or_default();
    if seen.seen.iter().any(|s| s == id) {
        return;
    }
    seen.seen.push(id.to_string());
    if let Ok(json) = serde_json::to_string_pretty(&seen) {
        write(seen_path(), &json);
    }
}

/// Word-wrap `text` to `width` columns.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// What `qsmxt news` prints for `loaded`, fetched from `url`, to someone running `version`.
fn render(loaded: &Loaded, url: &str, version: &str) -> String {
    const WIDTH: usize = 78;
    let mut out = String::new();
    if let Some(e) = &loaded.error {
        // ureq's errors name the URL already; a file's do not.
        let e = if e.contains(url) { e.clone() } else { format!("{url}: {e}") };
        out.push_str(&format!("Could not read the announcements ({e}).\n"));
        if loaded.items.is_empty() {
            out.push_str("No announcements have been saved on this computer yet.\n");
            return out;
        }
        out.push_str("Showing the announcements saved the last time they could be read.\n\n");
    } else if loaded.items.is_empty() {
        out.push_str("There are no announcements.\n");
        return out;
    }
    let released = version.trim().trim_start_matches('v') != "0.0.0";
    for (i, a) in loaded.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let important = if a.critical { "  [important]" } else { "" };
        out.push_str(&format!("{}  {}{important}\n", a.date, a.title));
        for (j, p) in a.paragraphs().iter().enumerate() {
            if j > 0 {
                out.push('\n');
            }
            for l in wrap(p, WIDTH - 2) {
                out.push_str(&format!("  {l}\n"));
            }
        }
        if let Some(range) = a.affects_text() {
            let yours = match (released, a.applies_to(version)) {
                (false, _) => String::new(),
                (true, true) => format!(" This version ({version}) is affected."),
                (true, false) => format!(" This version ({version}) is not."),
            };
            for l in wrap(&format!("{range}{yours}"), WIDTH - 2) {
                out.push_str(&format!("  {l}\n"));
            }
        }
        if let Some(link) = &a.link {
            out.push_str(&format!("  {link}\n"));
        }
    }
    out
}

/// `qsmxt news`: fetch the announcements and print them, newest first; offline, print the copy
/// saved last time and say so.
pub fn execute() -> crate::Result<()> {
    let url = url();
    let loaded = load(|| fetch(&url));
    print!("{}", render(&loaded, &url, current_version()));
    Ok(())
}

/// Serializes tests that point [`DIR_ENV`] or [`URL_ENV`] somewhere: both are process-global.
#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Holds [`env_lock`] and points [`DIR_ENV`] at a temporary directory until dropped.
#[cfg(test)]
pub struct TestDir {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: tempfile::TempDir,
}

#[cfg(test)]
impl TestDir {
    pub fn new() -> Self {
        let guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var(DIR_ENV, dir.path());
        std::env::remove_var(URL_ENV);
        TestDir { _guard: guard, dir }
    }
}

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        std::env::remove_var(DIR_ENV);
        std::env::remove_var(URL_ENV);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An announcement with the given id, date and flags, for tests.
    pub(crate) fn item(id: &str, date: &str, critical: bool, affects: Option<(&str, &str)>) -> Announcement {
        Announcement {
            id: id.into(),
            date: date.into(),
            title: format!("Title {id}"),
            body: format!("Body of {id}."),
            link: None,
            critical,
            affects: affects.map(|(from, fixed)| Affects {
                from: (!from.is_empty()).then(|| from.to_string()),
                fixed_in: (!fixed.is_empty()).then(|| fixed.to_string()),
            }),
        }
    }

    const EXAMPLE: &str = r#"
[[announcement]]
id = "2026-11-01-old"
date = "2026-11-01"
title = "An older note"
body = "Plain text."

[[announcement]]
id = "2026-11-06-bug"
date = "2026-11-06"
title = "A bug in 10.x"
body = """
First paragraph,
over two lines.

Second paragraph."""
link = "https://example.com/bug"
critical = true
affects = { from = "10.0.0", fixed_in = "11.0.1" }
"#;

    /// The file in the repository is what the published one will be: it must parse, and every
    /// announcement in it must be valid (an invalid one would be left out without a word).
    #[test]
    fn the_committed_file_parses_and_every_announcement_is_valid() {
        let text = include_str!("../../announcements.toml");
        let (_, problems) = parse_all(text).expect("announcements.toml must be valid TOML");
        assert!(problems.is_empty(), "announcements.toml: {problems:?}");
    }

    #[test]
    fn every_field_parses_and_the_list_is_newest_first() {
        let items = parse(EXAMPLE).unwrap();
        assert_eq!(items.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["2026-11-06-bug", "2026-11-01-old"]);
        let bug = &items[0];
        assert!(bug.critical);
        assert_eq!(bug.link.as_deref(), Some("https://example.com/bug"));
        assert_eq!(bug.paragraphs(), ["First paragraph, over two lines.", "Second paragraph."]);
        assert_eq!(bug.affects_text().as_deref(), Some("Affects versions from 10.0.0; fixed in 11.0.1."));
        let old = &items[1];
        assert!(!old.critical && old.link.is_none() && old.affects.is_none(), "optional fields default");
    }

    #[test]
    fn an_empty_or_comment_only_file_has_no_announcements() {
        assert_eq!(parse("").unwrap(), []);
        assert_eq!(parse("# nothing yet\nannouncement = []\n").unwrap(), []);
    }

    #[test]
    fn a_broken_file_is_an_error_and_a_bad_announcement_is_left_out() {
        assert!(parse("[[announcement]\nid = ").is_err());
        assert!(parse("[[announcement]]\nid = \"x\"\n").is_err(), "date, title and body are required");
        let text = format!(
            "{EXAMPLE}\n[[announcement]]\nid = \"bad-date\"\ndate = \"6 Nov\"\ntitle = \"t\"\nbody = \"b\"\n\
             [[announcement]]\nid = \"bad-version\"\ndate = \"2026-11-07\"\ntitle = \"t\"\nbody = \"b\"\naffects = {{ fixed_in = \"soon\" }}\n\
             [[announcement]]\nid = \"2026-11-01-old\"\ndate = \"2026-11-08\"\ntitle = \"t\"\nbody = \"b\"\n"
        );
        let (items, problems) = parse_all(&text).unwrap();
        assert_eq!(items.len(), 2, "{items:?}");
        assert_eq!(problems.len(), 3, "{problems:?}");
        // Fields a later version adds are ignored rather than breaking this one.
        assert_eq!(parse("[[announcement]]\nid=\"a\"\ndate=\"2026-01-01\"\ntitle=\"t\"\nbody=\"b\"\nnew_field=1\n").unwrap().len(), 1);
    }

    #[test]
    fn a_version_range_includes_from_and_excludes_fixed_in() {
        let a = item("a", "2026-11-06", false, Some(("10.0.0", "11.0.1")));
        assert!(!a.applies_to("9.9.9"));
        assert!(a.applies_to("10.0.0"));
        assert!(a.applies_to("v10.4.2"));
        assert!(a.applies_to("11.0.0"));
        assert!(!a.applies_to("11.0.1"));
        assert!(!a.applies_to("12.0.0"));
        assert!(a.applies_to("11.0.1-rc1"), "a pre-release of the fix does not have it yet");
        assert!(!a.applies_to("not-a-version"));

        let open_start = item("b", "2026-11-06", false, Some(("", "11.0.1")));
        assert!(open_start.applies_to("1.0.0") && !open_start.applies_to("11.0.1"));
        let open_end = item("c", "2026-11-06", false, Some(("10.0.0", "")));
        assert!(open_end.applies_to("99.0.0") && !open_end.applies_to("9.0.0"));
    }

    #[test]
    fn a_dev_build_sees_only_announcements_without_a_range() {
        assert!(item("a", "2026-11-06", false, None).applies_to("0.0.0"));
        assert!(!item("b", "2026-11-06", false, Some(("", "99.0.0"))).applies_to("0.0.0"));
        assert!(!item("c", "2026-11-06", true, Some(("0.0.0", ""))).applies_to("0.0.0"));
    }

    /// Ordinary announcements pop up once; critical ones every time while they apply; critical
    /// first, then newest first.
    #[test]
    fn pop_ups_are_once_except_critical_and_critical_come_first() {
        let all = parse(&format!(
            "{EXAMPLE}\n[[announcement]]\nid = \"new\"\ndate = \"2026-11-07\"\ntitle = \"t\"\nbody = \"b\"\n\
             [[announcement]]\nid = \"other-version\"\ndate = \"2026-11-08\"\ntitle = \"t\"\nbody = \"b\"\naffects = {{ from = \"12.0.0\" }}\n"
        ))
        .unwrap();
        let ids = |v: Vec<Announcement>| v.into_iter().map(|a| a.id).collect::<Vec<_>>();

        let none = HashSet::new();
        assert_eq!(ids(pop_ups(&all, "10.2.0", &none)), ["2026-11-06-bug", "new", "2026-11-01-old"]);

        let seen: HashSet<String> = ["2026-11-06-bug", "new", "2026-11-01-old"].map(String::from).into();
        assert_eq!(ids(pop_ups(&all, "10.2.0", &seen)), ["2026-11-06-bug"], "only the critical one returns");
        assert!(pop_ups(&all, "11.0.1", &seen).is_empty(), "updating past the fix ends it");
        assert_eq!(ids(pop_ups(&all, "0.0.0", &none)), ["new", "2026-11-01-old"], "a dev build: no ranged ones");
    }

    #[test]
    fn seen_ids_are_remembered_across_starts() {
        let t = TestDir::new();
        assert!(load_seen().is_empty());
        mark_seen("a");
        mark_seen("b");
        mark_seen("a");
        assert_eq!(load_seen(), ["a", "b"].map(String::from).into());
        let saved = std::fs::read_to_string(t.dir.path().join("announcements-seen.json")).unwrap();
        assert_eq!(saved.matches("\"a\"").count(), 1, "an id is recorded once: {saved}");
    }

    #[test]
    fn seen_ids_survive_a_missing_directory_and_a_corrupt_file() {
        let t = TestDir::new();
        std::env::set_var(DIR_ENV, t.dir.path().join("x").join("y"));
        mark_seen("a");
        assert!(load_seen().contains("a"));
        std::fs::write(t.dir.path().join("x").join("y").join("announcements-seen.json"), "{ not json").unwrap();
        assert!(load_seen().is_empty());
        mark_seen("b");
        assert_eq!(load_seen(), ["b"].map(String::from).into());
    }

    #[test]
    fn a_fetch_is_saved_and_used_when_the_next_one_fails() {
        let _t = TestDir::new();
        let first = load(|| Ok(EXAMPLE.to_string()));
        assert_eq!(first.items.len(), 2);
        assert!(first.error.is_none());

        let offline = load(|| Err("offline".into()));
        assert_eq!(offline.items, first.items, "the saved copy stands in");
        assert_eq!(offline.error.as_deref(), Some("offline"));

        // A published file that does not parse is no better than none: the saved copy is used.
        let broken = load(|| Ok("[[announcement]".into()));
        assert_eq!(broken.items, first.items);
        assert!(broken.error.is_some());
        assert_eq!(load_cached(), first.items, "and is not overwritten");
    }

    #[test]
    fn offline_with_nothing_saved_there_is_nothing() {
        let _t = TestDir::new();
        let loaded = load(|| Err("offline".into()));
        assert!(loaded.items.is_empty());
    }

    #[test]
    fn a_file_url_is_read_and_an_unreachable_one_fails_fast() {
        let t = TestDir::new();
        let path = t.dir.path().join("news.toml");
        std::fs::write(&path, EXAMPLE).unwrap();
        assert_eq!(fetch(&format!("file://{}", path.display())).unwrap(), EXAMPLE);
        assert!(fetch("file:///no/such/file.toml").is_err());

        let start = std::time::Instant::now();
        assert!(fetch("http://127.0.0.1:1/announcements.toml").is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(6));
    }

    #[test]
    fn the_url_can_be_overridden() {
        let _t = TestDir::new();
        assert_eq!(url(), DEFAULT_URL);
        std::env::set_var(URL_ENV, "file:///tmp/a.toml");
        assert_eq!(url(), "file:///tmp/a.toml");
    }

    /// `qsmxt news` with a local file: every announcement, newest first, with its range.
    #[test]
    fn news_prints_every_announcement_newest_first() {
        let t = TestDir::new();
        let path = t.dir.path().join("news.toml");
        std::fs::write(&path, EXAMPLE).unwrap();
        let url = format!("file://{}", path.display());
        let out = render(&load(|| fetch(&url)), &url, "10.2.0");
        let bug = out.find("A bug in 10.x  [important]").expect(&out);
        let old = out.find("2026-11-01  An older note").expect(&out);
        assert!(bug < old, "newest first:\n{out}");
        assert!(out.contains("  First paragraph, over two lines.\n\n  Second paragraph.\n"), "{out}");
        assert!(out.contains("fixed in 11.0.1. This version (10.2.0) is\n  affected."), "{out}");
        assert!(out.contains("  https://example.com/bug\n"), "{out}");
        assert!(!out.contains("Could not"), "{out}");
    }

    /// `qsmxt news` offline: says so, and prints the saved copy (or that there is none).
    #[test]
    fn news_offline_says_so_and_prints_the_saved_copy() {
        let _t = TestDir::new();
        let url = "http://127.0.0.1:1/announcements.toml";
        let out = render(&load(|| fetch(url)), url, "10.2.0");
        assert!(out.contains("Could not read the announcements (http://127.0.0.1:1"), "{out}");
        assert!(out.contains("No announcements have been saved"), "{out}");

        load(|| Ok(EXAMPLE.to_string()));
        let out = render(&load(|| fetch(url)), url, "10.2.0");
        assert!(out.contains("Showing the announcements saved the last time"), "{out}");
        assert!(out.contains("A bug in 10.x"), "{out}");
    }

    #[test]
    fn news_with_none_published_says_so() {
        let out = render(&Loaded::default(), DEFAULT_URL, "10.2.0");
        assert_eq!(out, "There are no announcements.\n");
    }
}
