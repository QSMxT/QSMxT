//! `qsmxt news`: the announcements ([`qsmxt_io::news`]) printed.

use qsmxt_io::news::{fetch, load, url, Loaded};

use super::update::current_version;

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
pub fn execute() -> qsmxt_io::Result<()> {
    let url = url();
    let loaded = load(|| fetch(&url));
    print!("{}", render(&loaded, &url, current_version()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsmxt_io::news::{TestDir, DEFAULT_URL, EXAMPLE};

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
