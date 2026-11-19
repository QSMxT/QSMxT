//! The TUI's view preferences, kept per user in `~/.qsmxt/tui.json`: whether the
//! Reconstruction tab shows every setting (Advanced) or the basic ones (Basic), and whether the
//! one-time tip about switching has been shown. A view preference only: it is not part of a
//! config and changes no run.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    advanced: bool,
    basic_tip_shown: bool,
}

fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("QSMXT_TUI_FILE") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".qsmxt").join("tui.json"))
}

fn load() -> Prefs {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Best effort: a read-only home just means the choice is not remembered.
fn store(prefs: &Prefs) {
    let Some(path) = path() else { return };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(json) = serde_json::to_string(prefs) {
        let _ = std::fs::write(path, json);
    }
}

/// Whether the user chose Advanced mode (Basic unless they did).
pub fn advanced() -> bool {
    load().advanced
}

pub fn set_advanced(advanced: bool) {
    let mut p = load();
    p.advanced = advanced;
    store(&p);
}

/// The tip about Basic and Advanced mode, the first time the TUI opens with them (unless tips
/// are turned off by the environment); `None` after that.
pub fn basic_tip() -> Option<super::tips::Tip> {
    let set = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    let mut p = load();
    if p.basic_tip_shown || set(super::tips::OPT_OUT_ENV) || set("CI") {
        return None;
    }
    p.basic_tip_shown = true;
    store(&p);
    Some(super::tips::BASIC_ADVANCED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_until_advanced_is_chosen_and_the_tip_shows_once() {
        let _s = super::super::tips::TestSettings::new();
        assert!(!advanced());
        set_advanced(true);
        assert!(advanced());
        set_advanced(false);
        assert!(!advanced());
        assert!(basic_tip().is_some());
        assert!(basic_tip().is_none());
    }
}
