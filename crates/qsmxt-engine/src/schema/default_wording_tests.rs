//! Help text that names a setting's default names the one the registry has
//! ([`Setting::default`]), which comes from the config's `Default` and so from qsm-core's. A
//! default spelt out in prose is not updated when qsm-core's changes; this is what notices.

use regex::Regex;
use serde_json::Value;

use super::{defs, settings, Ty};

const NUM: &str = r"-?(?:\d+\.?\d*|\.\d+)(?:e-?\d+)?";

/// The values a text names as the default: `(default: X)`, `(default X)`, `; default X)`,
/// `Default: X`, `` `x` (default) `` and `off by default`, each as the words that follow.
fn named_defaults(text: &str) -> Vec<String> {
    static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
        [
            r"\((?:the )?default:? ([^);]*)",
            r"[;,] (?:the )?default:? ([^);]*)",
            r"(?:^|\. )Default:? ([^);]*)",
            r"`([^`]+)` \((?:the )?default\b",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    });
    let mut named: Vec<String> = PATTERNS.iter()
        .flat_map(|p| p.captures_iter(text).map(|c| c[1].trim().to_string()).collect::<Vec<_>>())
        .collect();
    if Regex::new(r"\b[Oo]ff by default").unwrap().is_match(text) {
        named.push("off".into());
    }
    named
}

fn same_number(a: &str, b: f64) -> bool {
    a.parse::<f64>().is_ok_and(|a| (a - b).abs() <= 1e-12 * b.abs().max(1.0))
}

/// Whether `words` (what a text says the default is) agree with `default`; `None` where they are
/// prose this cannot read (`auto`, `the first run in name order`, an unset value's fallback).
fn agrees(words: &str, default: &Value) -> Option<bool> {
    let num = Regex::new(&format!("^(?:{NUM})(?:\\s+{NUM})*")).unwrap();
    let numbers = |w: &str| num.find(w).map(|m| m.as_str().split_whitespace().map(str::to_string).collect::<Vec<_>>());
    let first_word = words.split([' ', ',', ';', '.']).next().unwrap_or("");
    match default {
        Value::Bool(b) => match first_word {
            "on" | "true" => Some(*b),
            "off" | "false" => Some(!*b),
            _ => None,
        },
        Value::Number(n) => {
            let n = n.as_f64().unwrap();
            numbers(words).map(|w| same_number(&w[0], n))
        }
        // Three values, or one for every axis.
        Value::Array(a) => {
            let want: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            numbers(words).map(|w| match w.len() {
                1 => want.iter().all(|&x| same_number(&w[0], x)),
                _ => w.len() == want.len() && w.iter().zip(&want).all(|(a, &b)| same_number(a, b)),
            })
        }
        Value::String(s) if !s.is_empty() && !first_word.is_empty() => Some(first_word == s),
        _ => None,
    }
}

#[test]
fn reads_what_a_text_names_as_the_default() {
    assert_eq!(named_defaults("TV lambda (default: 0.0003)"), ["0.0003"]);
    assert_eq!(named_defaults("Padding; -1 leaves an axis unpadded (default 0 0 0)"), ["0 0 0"]);
    assert_eq!(named_defaults("`fft` (default) or `qsmart-reference`"), ["fft"]);
    assert_eq!(named_defaults("Weights. Off by default"), ["off"]);
    assert_eq!(agrees("0.0003", &Value::from(0.0003)), Some(true));
    assert_eq!(agrees("0 0 0", &serde_json::json!([0, 0, 1])), Some(false));
    assert_eq!(agrees("64", &serde_json::json!([64, 64, 64])), Some(true));
    assert_eq!(agrees("off", &Value::Bool(true)), Some(false));
    assert_eq!(agrees("auto", &Value::Null), None);
}

/// Every default a setting's help names (TUI, front-end, command-line as written and as the flag
/// shows it, its value row's, and the option marked `(default)`) is the setting's default.
#[test]
fn help_never_names_another_default() {
    let mut wrong = Vec::new();
    for (s, d) in settings().iter().zip(defs()) {
        // A flag that counts otherwise than the config says its own default (`cli_default`).
        let cli = d.cli.help.filter(|_| d.cli.default.is_none());
        let texts = [Some(d.help), d.gui_help, cli, d.value.map(|v| v.1), d.gui_value.map(|v| v.1)];
        for text in texts.into_iter().flatten() {
            for words in named_defaults(text) {
                if agrees(&words, &s.default) == Some(false) {
                    wrong.push(format!("{}: says the default is `{words}`, it is {}: {text}", s.path, s.default));
                }
            }
        }
        let flag = d.flag.and_then(|l| super::cli::PIPELINE_FLAGS.iter().find(|f| f.long == l.trim_start_matches("--")));
        if let (Some(help), None) = (flag.and_then(|f| f.help), d.cli.default) {
            for words in named_defaults(help) {
                if agrees(&words, &s.default) == Some(false) {
                    wrong.push(format!("{}: the flag's help says the default is `{words}`, it is {}: {help}", s.path, s.default));
                }
            }
        }
        if let Ty::Enum(options) = &d.ty {
            for (value, label, _, help) in options {
                if help.contains("(default)") && value != &s.default {
                    wrong.push(format!("{}: marks `{label}` as the default, it is {}", s.path, s.default));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "help names a default the setting does not have:\n{}", wrong.join("\n"));
}
