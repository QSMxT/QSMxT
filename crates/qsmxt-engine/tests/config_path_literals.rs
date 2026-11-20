//! Config paths written as strings (`"inversion.rts.delta"`, read and written through
//! `schema::get`/`set`, the TUI's rows and the settings flag sets) name settings that exist: a
//! typo or a renamed setting fails here, not only in whichever test happens to use the path.
//!
//! Every string literal in the workspace's non-test sources that starts with a top-level section
//! of the configuration and a dot is a config path: a registry setting, or a value or table the
//! default configuration has. One ending in a dot or an underscore (a flag set's
//! `"inversion.rts."`) names the settings it starts, so some setting must start with it.
//!
//! Two kinds of literal look like config paths and are not: QSM.rs's names for its settings
//! (the left side of `recommendations::mapping`'s table, which its own tests hold to QSM.rs), and
//! the names settings files had before (`migrate.rs`, whose job is to read them).

use std::path::{Path, PathBuf};

use qsmxt_engine::schema;
use qsmxt_engine::PipelineConfig;

/// The workspace's source files, without its tests: files under a `tests` directory or named
/// `tests.rs`, `*_tests.rs` or `testutils.rs`.
fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" {
                    walk(&path, out);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") && name != "testutils.rs"
                && name != "migrate.rs"
            {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    for dir in ["src", "crates/qsmxt-engine/src", "crates/qsmxt-io/src", "crates/qsmxt-tui/src"] {
        walk(&root.join(dir), &mut out);
    }
    out
}

/// `text` before its first `#[cfg(test)]`: the tests that close most files are left out.
fn non_test(text: &str) -> &str {
    text.find("#[cfg(test)]").map_or(text, |i| &text[..i])
}

/// The string literals of `text` (plain ones; raw strings and escapes are not config paths).
fn literals(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        out.push(&after[..end]);
        rest = &after[end + 1..];
    }
    out
}

fn lookup<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.').try_fold(v, |v, k| v.get(k))
}

/// QSM.rs's names for its settings: the first of each `("qsm-rs", "qsmxt", Mapping::..)`.
fn qsm_rs_names() -> Vec<String> {
    let text = include_str!("../src/recommendations/mapping.rs");
    text.lines().filter_map(|l| {
        let l = l.trim().strip_prefix("(\"")?;
        let (name, rest) = l.split_once('"')?;
        rest.contains("Mapping::").then(|| name.to_string())
    }).collect()
}

/// File names that read like config paths.
const NOT_PATHS: &[&str] = &["pipeline.mmd"];

#[test]
fn every_config_path_literal_is_a_setting() {
    let config = serde_json::to_value(PipelineConfig::default()).unwrap();
    let sections: Vec<String> = config.as_object().unwrap().keys().cloned().collect();
    let settings = schema::settings();
    let is_path = |s: &str| {
        let first = s.split('.').next().unwrap_or("");
        s.contains('.') && sections.iter().any(|k| k == first)
            && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
    };
    let qsm_rs = qsm_rs_names();
    let mut wrong = Vec::new();
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        for lit in literals(non_test(&text)) {
            let path = lit.trim_start_matches('!');
            if !is_path(path) || NOT_PATHS.contains(&path) {
                continue;
            }
            let known = if path.ends_with('.') || path.ends_with('_') {
                settings.iter().any(|s| s.path.starts_with(path))
            } else {
                settings.iter().any(|s| s.path == path) || lookup(&config, path).is_some()
                    || qsm_rs.iter().any(|q| q == path || q.starts_with(&format!("{path}.")))
            };
            if !known {
                wrong.push(format!("{}: \"{lit}\"", file.display()));
            }
        }
    }
    assert!(wrong.is_empty(), "not a config path:\n{}", wrong.join("\n"));
}

/// [`schema::get`] tells an unset optional setting (`null`) from a name the config does not have.
#[test]
fn get_refuses_an_unknown_leaf() {
    let c = PipelineConfig::default();
    assert_eq!(schema::get(&c, "inversion.rts.typo"), None);
    assert_eq!(schema::get(&c, "inversion.tile_size"), Some(serde_json::Value::Null));
    assert!(schema::get(&c, "inversion.rts.delta").is_some_and(|v| v.is_number()));
}
