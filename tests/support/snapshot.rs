//! Golden-file snapshots: the refactor's safety net.
//!
//! A snapshot is a text file under a crate's `tests/snapshots/`. A test renders what it checks
//! (help text, a flag's effect on the config, a command line, methods text, step cache keys, a
//! TUI screen) and [`check`] compares it with the file. A change that shows up in a snapshot is a
//! behaviour change: regenerate with `QSMXT_UPDATE_SNAPSHOTS=1` and say why in the PR.
//!
//! Shared by the crates' tests through `#[path]`, so every crate writes and checks snapshots the
//! same way.

#![allow(dead_code)]

use std::path::Path;

/// The variable that rewrites snapshots instead of checking them.
pub const UPDATE: &str = "QSMXT_UPDATE_SNAPSHOTS";

/// Checks `actual` against `<dir>/<name>`, or writes it there when [`UPDATE`] is set. A missing
/// file is a failure: a new snapshot is written on purpose.
pub fn check(dir: &str, name: &str, actual: &str) {
    let path = Path::new(dir).join(name);
    if std::env::var_os(UPDATE).is_some() {
        if std::fs::read_to_string(&path).ok().as_deref() != Some(actual) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, actual).unwrap();
        }
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("no snapshot {}: run with {UPDATE}=1 to write it", path.display())
    });
    if expected != actual {
        panic!("{} differs from what the code now gives:\n{}\nIf the change is intended, run with \
                {UPDATE}=1 and say why in the PR.", path.display(), first_difference(&expected, actual));
    }
}

/// The first line where the two differ, with a few lines of context.
fn first_difference(expected: &str, actual: &str) -> String {
    let (e, a): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let i = e.iter().zip(&a).position(|(x, y)| x != y).unwrap_or(e.len().min(a.len()));
    let from = i.saturating_sub(3);
    let mut out = String::new();
    for (j, line) in e.iter().enumerate().take(i).skip(from) {
        out.push_str(&format!("  {:>5} | {line}\n", j + 1));
    }
    for (j, line) in e.iter().enumerate().skip(i).take(5) {
        out.push_str(&format!("- {:>5} | {line}\n", j + 1));
    }
    for (j, line) in a.iter().enumerate().skip(i).take(5) {
        out.push_str(&format!("+ {:>5} | {line}\n", j + 1));
    }
    out.push_str(&format!("(snapshot {} lines, now {} lines)", e.len(), a.len()));
    out
}

/// Every leaf of `value` that differs from `base`, as `/json/pointer: before -> after`.
pub fn changes(base: &serde_json::Value, value: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    use serde_json::Value::Object;
    match (base, value) {
        (Object(b), Object(v)) => {
            for (k, vv) in v {
                changes(b.get(k).unwrap_or(&serde_json::Value::Null), vv, &format!("{path}/{k}"), out);
            }
            for (k, bv) in b.iter().filter(|(k, _)| !v.contains_key(*k)) {
                changes(bv, &serde_json::Value::Null, &format!("{path}/{k}"), out);
            }
        }
        (b, v) if b != v => out.push(format!("{path}: {b} -> {v}")),
        _ => {}
    }
}
