//! Golden files of what every form the TUI can reach builds (under `tests/snapshots/forms/`):
//! the config, the command it prints and its methods text.
//!
//! The screen snapshots (`snapshot_tests`) draw a fresh form only. Here every form
//! [`explored`] reaches (each option of each select, each checkbox, from a fresh form, one
//! change at a time) is written down as what it builds, so moving the TUI onto another form
//! state (the refactor's P4) cannot change what any of them runs without a file changing.
//!
//! Each form is the changes that make it, then
//! - its config, as the JSON pointers that differ from a fresh form's config (or the error);
//! - its command, as the options added to or dropped from a fresh form's command;
//! - its methods text, as a hash (the config already says what changed; the hash says the text
//!   followed it).
//!
//! The forms are filed by their first change, so a new select option or checkbox changes the
//! files it belongs to rather than shifting every one. Regenerate with
//! `QSMXT_UPDATE_SNAPSHOTS=1 cargo test -p qsmxt-tui form_snapshots -- --ignored` only for an
//! intended change, and say why in the PR.

use std::collections::BTreeMap;

use crate::snapshot;

use super::app::{App, PipelineRow};
use super::command::{build_command_string, config_from_app, methods_text};
use super::explore::{build, explored, Act};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/forms");

/// The 64-bit FNV-1a hash of `text`. Std's hasher may change between Rust versions; this one
/// cannot, so a golden file written once stays right.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// The option a select shows at `index` on `app`'s form, as the select lists it.
fn option_label(app: &App, field: &str, index: usize) -> String {
    app.all_pages().into_iter()
        .flat_map(|(_, _, rows)| rows)
        .find_map(|r| match r {
            PipelineRow::AlgoSelect { field: f, options, .. } if f == field => options.get(index).map(|o| o.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| format!("#{index}"))
}

/// The changes that make a form, in words: each select's option by its label, as it was offered
/// on the form before that change.
fn describe(acts: &[Act]) -> Vec<String> {
    (0..acts.len()).map(|i| match &acts[i] {
        Act::Toggle(f) => format!("toggle {f}"),
        Act::Select(f, n) => format!("{f} = {}", option_label(&build(&acts[..i]), f, *n)),
        Act::Group => "group orientations by acq".to_string(),
        Act::Echoes(n) => format!("first {n} echo(es)"),
    }).collect()
}

/// The file a form goes in: its first change's setting.
fn file_of(acts: &[Act]) -> String {
    match acts.first() {
        None => "fresh".to_string(),
        Some(Act::Toggle(f) | Act::Select(f, _)) => f.to_string(),
        Some(Act::Group) => "orientation_group".to_string(),
        Some(Act::Echoes(_)) => "num_echoes".to_string(),
    }
}

/// A command split into its options (each `--flag` with its values), the program and positional
/// arguments first.
fn options(cmd: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in cmd.split(' ') {
        match out.last_mut() {
            Some(last) if !word.starts_with("--") => {
                last.push(' ');
                last.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out
}

/// How `cmd` differs from `base`: the options dropped and added, or the whole command when the
/// options are the same but in another order.
fn command_changes(base: &str, cmd: &str) -> Vec<String> {
    let (b, c) = (options(base), options(cmd));
    let dropped: Vec<String> = b.iter().filter(|o| !c.contains(o)).map(|o| format!("- {o}")).collect();
    let added: Vec<String> = c.iter().filter(|o| !b.contains(o)).map(|o| format!("+ {o}")).collect();
    if dropped.is_empty() && added.is_empty() && base != cmd {
        return vec![format!("= {cmd}")];
    }
    dropped.into_iter().chain(added).collect()
}

/// What a form builds, as the golden file shows it.
struct Built {
    config: Result<serde_json::Value, String>,
    command: String,
    methods: Result<String, String>,
}

fn built(app: &App) -> Built {
    Built {
        config: config_from_app(app).map(|c| serde_json::to_value(&c).unwrap()).map_err(|e| e.to_string()),
        command: build_command_string(app),
        methods: methods_text(app),
    }
}

fn methods_line(methods: &Result<String, String>) -> String {
    match methods {
        Ok(text) => format!("{:016x} ({} lines)", fnv1a(text), text.lines().count()),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
#[ignore = "slow: builds every form the TUI can reach (about 40 s in a debug build)"]
fn every_reachable_form_builds_what_it_built() {
    let fresh = built(&build(&[]));
    let base_config = fresh.config.as_ref().expect("a fresh form builds a config");
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    files.insert("fresh".to_string(), format!(
        "# A fresh form; every other file lists changes from this.\n\ncommand: {}\nmethods: {}\nconfig:\n{}\n",
        fresh.command, methods_line(&fresh.methods), serde_json::to_string_pretty(base_config).unwrap()));
    // Each form by its changes alone (not its place in the exploration), so a form found or lost
    // elsewhere does not renumber every file.
    for acts in explored().iter().skip(1) {
        let b = built(&build(acts));
        let out = files.entry(file_of(acts)).or_default();
        out.push_str(&format!("## {}\n", describe(acts).join(", ")));
        match &b.config {
            Ok(config) => {
                let mut changed = Vec::new();
                snapshot::changes(base_config, config, "", &mut changed);
                if changed.is_empty() {
                    out.push_str("config: unchanged\n");
                } else {
                    out.push_str("config:\n");
                    for c in changed {
                        out.push_str(&format!("  {c}\n"));
                    }
                }
            }
            Err(e) => out.push_str(&format!("config: error: {e}\n")),
        }
        let cmd = command_changes(&fresh.command, &b.command);
        if cmd.is_empty() {
            out.push_str("command: unchanged\n");
        } else {
            out.push_str("command:\n");
            for c in cmd {
                out.push_str(&format!("  {c}\n"));
            }
        }
        out.push_str(&format!("methods: {}\n\n", methods_line(&b.methods)));
    }
    for (name, text) in &files {
        snapshot::check(DIR, &format!("{name}.txt"), text);
    }
    // A file left from a select or checkbox that no longer exists would never be checked.
    if std::env::var_os(snapshot::UPDATE).is_none() {
        for entry in std::fs::read_dir(DIR).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().trim_end_matches(".txt").to_string();
            assert!(files.contains_key(&name), "{DIR}/{name}.txt lists no form the TUI reaches now: delete it");
        }
    }
}

#[test]
fn fnv1a_is_the_standard_hash() {
    // The published test vectors of 64-bit FNV-1a.
    assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a("foobar"), 0x8594_4171_f739_67e8);
}
