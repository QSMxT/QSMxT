//! "Did you know?" tips, shown in a pop-up when the TUI opens.
//!
//! Whether tips are shown, and which one comes next, is kept per user in
//! `~/.qsmxt/tips.json`. Tips can be turned off from the pop-up itself and back on by
//! pressing `?` in the TUI. `QSMXT_NO_TIPS` turns them off without touching the file
//! (e.g. for a shared install), and CI never shows them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Set (to anything non-empty) to never show a tip at startup.
pub const OPT_OUT_ENV: &str = "QSMXT_NO_TIPS";

/// One tip. Text inside backticks is drawn as a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tip {
    pub text: &'static str,
    pub link: Option<&'static str>,
}

const fn tip(text: &'static str, link: Option<&'static str>) -> Tip {
    Tip { text, link }
}

pub const TIPS: &[Tip] = &[
    tip(
        "QSMxT can run on the MRI scanner. QSMxT Recon reconstructs QSM as part of the scan, so the maps arrive with the images.",
        Some("https://qsmxt.github.io/recon/"),
    ),
    tip(
        "Every step of QSMxT is also a command of its own. Try `qsmxt mask bet`, `qsmxt unwrap` or `qsmxt invert`, and see `qsmxt --help` for the full list.",
        None,
    ),
    tip("You can update QSMxT from the terminal with `qsmxt update`.", None),
    tip(
        "You can run QSMxT's algorithms in your web browser, with nothing to install, using QSMbly.",
        Some("https://qsmbly.neurodesk.org/"),
    ),
    tip(
        "All of QSMxT's algorithms are benchmarked on QSM-CI, so you can see how they compare.",
        Some("https://qsmxt.github.io/QSM-CI/"),
    ),
    tip(
        "The panel below the settings shows the `qsmxt run` command for your current settings. Copy it to run the same pipeline from a script.",
        None,
    ),
    tip(
        "The Methods tab (8) writes a methods paragraph for your current settings. Press `c` there to copy it.",
        None,
    ),
    tip(
        "The Pipeline tab (7) shows every step your settings run, the memory a run needs and the models it downloads. Press `s` there to save it as an SVG.",
        None,
    ),
    tip(
        "You can try QSMxT without data of your own. Set Input Mode to Example dataset, or run `qsmxt example`.",
        None,
    ),
    tip(
        "QSMxT can write its results as DICOM for a PACS or a clinical viewer. Tick Export DICOM on the Other outputs tab, or pass `--export-dicom`.",
        None,
    ),
    tip(
        "QSMxT can run a large cohort on a SLURM cluster. Choose SLURM on the Execution tab, or use `qsmxt slurm`.",
        None,
    ),
    tip(
        "Intermediate results are cached, so changing a late-stage option and running again does not recompute the earlier steps.",
        None,
    ),
    tip(
        "Every run saves its settings to `derivatives/qsmxt/pipeline_config.toml`. Load it with `--config`, or the Config file field on the Input tab, to repeat a run exactly.",
        None,
    ),
    tip("`qsmxt validate` checks a BIDS dataset before you run anything on it.", None),
    tip(
        "QSMxT does more than QSM: SWI and SMWI, R2*, T2*, R2 and R2' maps, χ-separation and SynthSeg segmentation are on the Other outputs and χ-separation tabs.",
        None,
    ),
    tip(
        "The QSMxT documentation is online.",
        Some("https://qsmxt.github.io/QSMxT/"),
    ),
    tip(
        "All of QSMxT's algorithms come from QSM.rs, an open-source QSM library.",
        Some("https://astewartau.github.io/QSM.rs/"),
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    enabled: bool,
    /// Index into [`TIPS`] of the tip to show next.
    next: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: true, next: 0 }
    }
}

fn settings_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("QSMXT_TIPS_FILE") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".qsmxt").join("tips.json"))
}

fn load() -> Settings {
    let Some(path) = settings_path() else { return Settings::default() };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Best effort: a read-only home just means the setting is not remembered.
fn store(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(json) = serde_json::to_string(settings) {
        let _ = std::fs::write(path, json);
    }
}

/// Does the user want a tip when the TUI opens? The saved setting only; see
/// [`startup_tip`] for the environment overrides.
pub fn enabled() -> bool {
    load().enabled
}

/// Turn startup tips on or off.
pub fn set_enabled(enabled: bool) {
    let mut settings = load();
    settings.enabled = enabled;
    store(&settings);
}

/// The next tip in turn, moving the saved position on so the next call shows another.
pub fn next_tip() -> Tip {
    let mut settings = load();
    let tip = TIPS[settings.next % TIPS.len()];
    settings.next = (settings.next + 1) % TIPS.len();
    store(&settings);
    tip
}

/// The tip to show as the TUI opens, or `None` when tips are off.
pub fn startup_tip() -> Option<Tip> {
    let set = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    if set(OPT_OUT_ENV) || set("CI") || !enabled() {
        return None;
    }
    Some(next_tip())
}

#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Point the settings at a fresh temporary file for the guard's lifetime.
#[cfg(test)]
pub struct TestSettings {
    _dir: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl TestSettings {
    pub fn new() -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("QSMXT_TIPS_FILE", dir.path().join("tips.json"));
        std::env::remove_var(OPT_OUT_ENV);
        std::env::remove_var("CI");
        TestSettings { _dir: dir, _guard: guard }
    }
}

#[cfg(test)]
impl Drop for TestSettings {
    fn drop(&mut self) {
        std::env::remove_var("QSMXT_TIPS_FILE");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tips_are_on_until_turned_off_and_the_choice_is_remembered() {
        let _s = TestSettings::new();
        assert!(startup_tip().is_some());
        set_enabled(false);
        assert!(startup_tip().is_none());
        set_enabled(true);
        assert!(startup_tip().is_some());
    }

    #[test]
    fn each_launch_shows_the_next_tip_and_wraps_around() {
        let _s = TestSettings::new();
        let seen: Vec<Tip> = (0..TIPS.len() + 1).map(|_| next_tip()).collect();
        assert_eq!(&seen[..TIPS.len()], TIPS);
        assert_eq!(seen[TIPS.len()], TIPS[0]);
    }

    #[test]
    fn the_environment_opt_out_wins_over_the_saved_setting() {
        let _s = TestSettings::new();
        std::env::set_var(OPT_OUT_ENV, "1");
        assert!(startup_tip().is_none());
        std::env::remove_var(OPT_OUT_ENV);
    }

    #[test]
    fn an_unreadable_settings_file_falls_back_to_tips_on() {
        let _s = TestSettings::new();
        std::fs::write(std::env::var_os("QSMXT_TIPS_FILE").unwrap(), "not json").unwrap();
        assert!(enabled());
    }

    #[test]
    fn backticks_are_balanced() {
        for t in TIPS {
            assert_eq!(t.text.matches('`').count() % 2, 0, "{}", t.text);
        }
    }
}
