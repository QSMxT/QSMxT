//! Every flag that takes a value says in its `--help` what is used when it is not given: its
//! default (clap's `[default: X]`, or `(default: X)` in the text), or what happens when it is
//! omitted (`if omitted: …`). Left out are flags that must be given, files to read or write
//! (there is no default file) and the reviewed exceptions in [`EXEMPT`].

use std::path::PathBuf;

use clap::{Arg, Command, CommandFactory};

/// Value-taking flags whose help needs no default, by command (`*` for every command) and flag,
/// with the reason.
const EXEMPT: &[(&str, &str, &str)] = &[
    // `--op erode:2 --op close:1`: the operations to apply, none unless given.
    ("qsmxt mask *", "--op", "a list of operations, none unless given"),
];

/// Every command with its path (`qsmxt mask bet`), depth first, without clap's `help`.
fn commands(cmd: &Command, path: String, out: &mut Vec<(String, Command)>) {
    out.push((path.clone(), cmd.clone()));
    for sub in cmd.get_subcommands().filter(|s| s.get_name() != "help") {
        commands(sub, format!("{path} {}", sub.get_name()), out);
    }
}

fn all_commands() -> Vec<(String, Command)> {
    let mut cli = crate::cli::Cli::command();
    cli.build();
    let mut out = Vec::new();
    commands(&cli, "qsmxt".to_string(), &mut out);
    out
}

fn matches(pattern: &str, path: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => path.starts_with(prefix),
        None => pattern == path,
    }
}

fn exempt(path: &str, long: &str) -> bool {
    EXEMPT.iter().any(|(cmd, flag, _)| matches(cmd, path) && flag.trim_start_matches("--") == long)
}

/// A flag the rule applies to: one that takes a value, may be left out and is not a path.
fn needs_a_default(arg: &Arg) -> bool {
    let path = arg.get_value_parser().type_id() == clap::value_parser!(PathBuf).type_id();
    arg.get_long().is_some()
        && arg.get_action().takes_values()
        && !arg.is_required_set()
        && !arg.is_hide_set()
        && !path
}

/// Whether the help says what is used when the flag is not given.
fn says_what_omitting_does(arg: &Arg) -> bool {
    let shown = !arg.get_default_values().is_empty() && !arg.is_hide_default_value_set();
    let text = [arg.get_help(), arg.get_long_help()]
        .into_iter()
        .flatten()
        .map(|h| h.to_string().to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    shown || text.contains("default") || text.contains("omit")
}

#[test]
fn every_value_flag_says_what_is_used_when_it_is_not_given() {
    let mut silent = Vec::new();
    for (path, cmd) in all_commands() {
        for arg in cmd.get_arguments().filter(|a| needs_a_default(a)) {
            let long = arg.get_long().unwrap();
            if !says_what_omitting_does(arg) && !exempt(&path, long) {
                silent.push(format!("{path} --{long}"));
            }
        }
    }
    assert!(
        silent.is_empty(),
        "these flags' help names no default and does not say what happens when they are omitted:\n{}",
        silent.join("\n")
    );
}

/// Each exception still names a flag that would otherwise fail, so the list stays reviewed.
#[test]
fn every_exemption_is_still_needed() {
    let commands = all_commands();
    for (pattern, flag, why) in EXEMPT {
        let long = flag.trim_start_matches("--");
        let needed = commands.iter().filter(|(p, _)| matches(pattern, p)).any(|(_, cmd)| {
            cmd.get_arguments().any(|a| a.get_long() == Some(long) && needs_a_default(a) && !says_what_omitting_does(a))
        });
        assert!(needed, "{pattern} {flag} ({why}) is exempt but needs no exemption");
    }
}
