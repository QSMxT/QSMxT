//! Snapshots of the command line (under `tests/snapshots/`):
//!
//! - `help/<command>.txt`: every command's `--help`.
//! - `args.txt`: every argument of every command: its flag, aliases, value names, number of
//!   values, action, default and accepted values.
//! - `run_flags.txt`: what each `qsmxt run` flag does to the configuration a run uses, given a
//!   sample value: every value a flag accepts from a list, else its default, else the first of a
//!   few samples the run accepts.
//!
//! Flag names, defaults and what they set must not change in a refactor. Regenerate with
//! `QSMXT_UPDATE_SNAPSHOTS=1 cargo test --bin qsmxt cli_snapshots` only for an intended change,
//! and say why in the PR.

use crate::snapshot;

use clap::{Arg, ArgAction, Command, CommandFactory, FromArgMatches};

use crate::cli::Cli;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots");

/// The version strings, which change with every commit, as placeholders.
fn normalise(text: &str) -> String {
    let mut text = text.replace(crate::commands::update::describe_version(), "<version>");
    text = text.replace(env!("QSM_CORE_DESCRIBE"), "<qsm-core>");
    text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") + "\n"
}

/// The command line, built as `qsmxt` builds it, with help rendered at a fixed width.
fn cli() -> Command {
    let mut cmd = Cli::command().term_width(100).max_term_width(100);
    cmd.build();
    cmd
}

/// Every command with its path (`qsmxt mask preset`), depth first, without clap's `help`.
fn commands(cmd: &Command, path: String, out: &mut Vec<(String, Command)>) {
    out.push((path.clone(), cmd.clone()));
    for sub in cmd.get_subcommands().filter(|s| s.get_name() != "help") {
        commands(sub, format!("{path} {}", sub.get_name()), out);
    }
}

fn all_commands() -> Vec<(String, Command)> {
    let mut out = Vec::new();
    commands(&cli(), "qsmxt".to_string(), &mut out);
    out
}

#[test]
fn every_commands_help() {
    for (path, mut cmd) in all_commands() {
        let help = cmd.render_long_help().to_string();
        snapshot::check(DIR, &format!("help/{}.txt", path.replace(' ', "_")), &normalise(&help));
    }
}

/// One argument as a line: what it is called and what it accepts.
pub(crate) fn describe(arg: &Arg) -> String {
    let mut s = match (arg.get_long(), arg.get_short()) {
        (Some(l), Some(c)) => format!("--{l} -{c}"),
        (Some(l), None) => format!("--{l}"),
        (None, Some(c)) => format!("-{c}"),
        (None, None) => format!("<{}>", arg.get_id()),
    };
    if let Some(a) = arg.get_all_aliases().filter(|a| !a.is_empty()) {
        s.push_str(&format!(" aliases={}", a.join(",")));
    }
    if let Some(a) = arg.get_all_short_aliases().filter(|a| !a.is_empty()) {
        s.push_str(&format!(" short_aliases={}", a.iter().collect::<String>()));
    }
    if let Some(v) = arg.get_value_names() {
        s.push_str(&format!(" value={}", v.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(",")));
    }
    if let Some(n) = arg.get_num_args() {
        s.push_str(&format!(" num_args={n:?}"));
    }
    s.push_str(&format!(" action={:?}", arg.get_action()));
    if let Some(d) = arg.get_value_delimiter() {
        s.push_str(&format!(" delimiter={d:?}"));
    }
    let defaults = arg.get_default_values();
    if !defaults.is_empty() {
        s.push_str(&format!(" default={}", defaults.iter().map(|d| d.to_string_lossy()).collect::<Vec<_>>().join(",")));
    }
    let possible = arg.get_possible_values();
    if !possible.is_empty() {
        let names: Vec<String> = possible.iter().map(|p| {
            let all = p.get_name_and_aliases().collect::<Vec<_>>().join("|");
            if p.is_hide_set() { format!("({all})") } else { all }
        }).collect();
        s.push_str(&format!(" values={}", names.join(",")));
    }
    for (set, name) in [(arg.is_required_set(), "required"), (arg.is_global_set(), "global"),
                        (arg.is_hide_set(), "hidden"), (arg.is_ignore_case_set(), "ignore_case"),
                        (arg.is_allow_hyphen_values_set(), "allow_hyphen_values"),
                        (arg.is_last_set(), "last"), (arg.is_exclusive_set(), "exclusive")] {
        if set {
            s.push(' ');
            s.push_str(name);
        }
    }
    s
}

#[test]
fn every_argument() {
    let mut out = String::new();
    for (path, cmd) in all_commands() {
        out.push_str(&format!("\n## {path}\n"));
        for arg in cmd.get_arguments() {
            out.push_str(&describe(arg));
            out.push('\n');
        }
    }
    snapshot::check(DIR, "args.txt", &normalise(&out));
}

/// How a configuration is built from parsed command-line arguments.
type Build = fn(&clap::ArgMatches) -> Result<serde_json::Value, String>;

/// `qsmxt run`'s configuration from its flags, as `qsmxt` builds it.
fn build(matches: &clap::ArgMatches) -> Result<serde_json::Value, String> {
    let cli = Cli::from_arg_matches(matches).map_err(|e| format!("clap: {e}"))?;
    let crate::cli::Command::Run(run) = cli.command else { unreachable!() };
    let config = crate::commands::run::build_config(None, &run.pipeline).map_err(|e| format!("refused: {e}"))?;
    Ok(serde_json::to_value(&config).unwrap())
}

/// [`build`] from the flag table ([`crate::cli_settings`]).
fn build_from_table(matches: &clap::ArgMatches) -> Result<serde_json::Value, String> {
    let run = matches.subcommand_matches("run").unwrap();
    let config = crate::cli_settings::build_config(Default::default(), &crate::cli_settings::given(run))
        .map_err(|e| format!("refused: {e}"))?;
    Ok(serde_json::to_value(&config).unwrap())
}

/// The configuration `qsmxt run <bids> <flags>` runs with, as JSON, or why it is refused.
fn config_of(cmd: &mut Command, build: Build, flags: &[String]) -> Result<serde_json::Value, String> {
    let argv = ["qsmxt", "run", "/bids"].into_iter().map(String::from).chain(flags.iter().cloned());
    let matches = cmd.try_get_matches_from_mut(argv).map_err(|e| format!("clap: {}", e.to_string().trim_end()))?;
    build(&matches)
}

/// The flags clap says are missing ("the following required arguments were not provided").
fn missing_flags(error: &str) -> Vec<String> {
    if !error.contains("required arguments were not provided") {
        return Vec::new();
    }
    error.lines().filter_map(|l| l.trim().strip_prefix("--"))
        .map(|l| format!("--{}", l.split_whitespace().next().unwrap_or(l)))
        .collect()
}

/// The values a value parser lists in its error ("expected fsl or as-stored, got '2'").
fn expected_values(error: &str) -> Vec<String> {
    let Some(list) = error.split("expected ").nth(1).and_then(|r| r.split(", got").next()) else {
        return Vec::new();
    };
    list.split(", ").flat_map(|p| p.split(" or ")).map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && !v.contains(' ')).collect()
}

/// [`config_of`] for one flag and its values, with the flags clap says it needs added first.
/// Returns the command-line tail shown and the result.
fn try_flag(cmd: &mut Command, build: Build, flag: &str, values: &[String]) -> (String, Result<serde_json::Value, String>) {
    let mut flags: Vec<String> = std::iter::once(flag.to_string()).chain(values.iter().cloned()).collect();
    let mut shown = flags.join(" ");
    let mut config = config_of(cmd, build, &flags);
    if let Err(e) = &config {
        let missing = missing_flags(e);
        if !missing.is_empty() {
            shown = format!("{shown} (with {})", missing.join(" "));
            flags.extend(missing);
            config = config_of(cmd, build, &flags);
        }
    }
    let first_line = |e: String| e.lines().next().unwrap_or("").to_string();
    (shown, config.map_err(first_line))
}

/// Every leaf of `value` that differs from `base`, as `/json/pointer: before -> after`.
fn changes(base: &serde_json::Value, value: &serde_json::Value, path: &str, out: &mut Vec<String>) {
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

/// Values to try on a flag that has no list of values and no default, in order.
const SAMPLES: [&str; 8] = ["2", "0.5", "magnitude,threshold:otsu", "erode:1", "*", "sub-1", "x", "1,1,1"];

/// The sample values of a flag: each one becomes a line of the snapshot.
fn samples(arg: &Arg) -> Vec<Vec<String>> {
    if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count) {
        return vec![Vec::new()];
    }
    let n = arg.get_num_args().map_or(1, |r| r.min_values().max(1));
    let possible = arg.get_possible_values();
    if !possible.is_empty() {
        return possible.iter().map(|p| vec![p.get_name().to_string(); n]).collect();
    }
    let defaults = arg.get_default_values();
    if !defaults.is_empty() {
        return vec![defaults.iter().map(|d| d.to_string_lossy().into_owned()).collect()];
    }
    SAMPLES.iter().map(|s| vec![s.to_string(); n]).collect()
}

/// The snapshot's lines for one flag of `qsmxt run`.
fn flag_lines(cmd: &mut Command, build: Build, base: &serde_json::Value, arg: &Arg) -> String {
    let mut out = String::new();
    let Some(long) = arg.get_long() else { return out };
    if arg.is_positional()
        || matches!(arg.get_action(), ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version) {
        return out;
    }
    let flag = format!("--{long}");
    let free = arg.get_possible_values().is_empty() && arg.get_default_values().is_empty()
        && !matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count);
    let mut tries: Vec<_> = samples(arg).iter().map(|values| try_flag(cmd, build, &flag, values)).collect();
    if free {
        // A flag that takes any value shows the first sample the run accepts; else every value
        // its parser lists when it refuses one; else the first sample clap accepts, else the
        // first.
        let n = arg.get_num_args().map_or(1, |r| r.min_values().max(1));
        let listed = tries.iter().find_map(|(_, c)| c.as_ref().err().map(|e| expected_values(e)).filter(|v| !v.is_empty()));
        tries = if let Some(i) = tries.iter().position(|(_, c)| c.is_ok()) {
            vec![tries.swap_remove(i)]
        } else if let Some(listed) = listed {
            listed.iter().map(|v| try_flag(cmd, build, &flag, &vec![v.clone(); n])).collect()
        } else if let Some(i) = tries.iter().position(|(_, c)| !matches!(c, Err(e) if e.starts_with("clap:"))) {
            vec![tries.swap_remove(i)]
        } else {
            vec![tries.swap_remove(0)]
        };
    }
    for (shown, config) in tries {
        out.push_str(&shown);
        out.push('\n');
        match config {
            Ok(config) => {
                let mut lines = Vec::new();
                changes(base, &config, "", &mut lines);
                if lines.is_empty() {
                    out.push_str("    (no change)\n");
                }
                for l in lines {
                    out.push_str(&format!("    {l}\n"));
                }
            }
            Err(e) => out.push_str(&format!("    {e}\n")),
        }
    }
    out
}

/// Parsing every flag's samples takes a while unoptimised, so the flags are split over a few
/// threads, each with its own command.
#[test]
fn what_each_run_flag_sets() {
    const THREADS: usize = 4;
    let base = config_of(&mut cli(), build, &[]).expect("qsmxt run <bids> runs with the defaults");
    let args: Vec<Arg> = cli().find_subcommand("run").unwrap().get_arguments().cloned().collect();
    let chunk = args.len().div_ceil(THREADS);
    let parts: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = args.chunks(chunk).map(|part| {
            let base = &base;
            scope.spawn(move || {
                let mut cmd = cli();
                part.iter().map(|arg| flag_lines(&mut cmd, build, base, arg)).collect::<String>()
            })
        }).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    snapshot::check(DIR, "run_flags.txt", &normalise(&parts.concat()));
}

/// The flag table's configuration ([`crate::cli_settings::build_config`]) is today's, flag by
/// flag: every `run` flag's lines in `run_flags.txt` come out the same.
#[test]
fn the_flag_table_sets_what_each_run_flag_sets() {
    let mut cmd = cli();
    let base = config_of(&mut cmd, build, &[]).unwrap();
    let args: Vec<Arg> = cmd.find_subcommand("run").unwrap().get_arguments().cloned().collect();
    let table: Vec<&str> = qsmxt_engine::schema::cli::PIPELINE_FLAGS.iter().map(|f| f.long).collect();
    let mut differ = Vec::new();
    for arg in args.iter().filter(|a| a.get_long().is_some_and(|l| table.contains(&l))) {
        let (old, new) = (flag_lines(&mut cmd, build, &base, arg), flag_lines(&mut cmd, build_from_table, &base, arg));
        if old != new {
            differ.push(format!("--- old\n{old}+++ new\n{new}"));
        }
    }
    assert!(differ.is_empty(), "{} flags differ:\n{}", differ.len(), differ.join("\n"));
}

/// [`the_flag_table_sets_what_each_run_flag_sets`] for flags together: each pipeline with each
/// flag's first sample, and combinations whose order matters.
#[test]
#[ignore = "slow: about 5 min unoptimised; until the old apply goes (refactor P3)"]
fn the_flag_table_sets_what_flags_set_together() {
    let mut cmd = cli();
    let args: Vec<Arg> = cmd.find_subcommand("run").unwrap().get_arguments().cloned().collect();
    let table: Vec<&str> = qsmxt_engine::schema::cli::PIPELINE_FLAGS.iter().map(|f| f.long).collect();
    let mut lines: Vec<Vec<String>> = Vec::new();
    for p in qsmxt_engine::pipelines::ids() {
        for arg in args.iter().filter(|a| a.get_long().is_some_and(|l| table.contains(&l) && l != "pipeline")) {
            let first = samples(arg).into_iter().next().unwrap_or_default();
            lines.push(["--pipeline", p].iter().map(|s| s.to_string())
                .chain(std::iter::once(format!("--{}", arg.get_long().unwrap()))).chain(first).collect());
        }
    }
    for l in [
        "--mask-preset bet --masking-input magnitude-last --bet-fractional-intensity 0.3",
        "--mask magnitude,bet:0.4 --masking-input magnitude --bet-voxel-scale 2",
        "--pipeline medi-toolbox --medi-smv --inhomogeneity-correction",
        "--no-inhomogeneity-correction --inhomogeneity-correction",
        "--qsmart-fit-percentile 90 --qsmart-fit-threshold-mode absolute",
        "--qsmart-fit-threshold 0.1",
        "--two-pass-mask magnitude,threshold:otsu --two-pass-mode original --mask-preset hd-bet",
        "--qsm-algorithm heidi --no-heidi-denoise",
        "--pipeline heidi --qsm-algorithm lsqr",
        "--qsm-reference thalamus --synthseg-version v2",
        "--romeo-template 3 --no-romeo-mag-weight --romeo-phase-linearity",
        "--dismiss-recommendation medi.bg-pdf=vsharp --dismiss-recommendation medi.bg-pdf=lbv",
    ] {
        lines.push(l.split(' ').map(String::from).collect());
    }
    let mut differ = Vec::new();
    for flags in &lines {
        let (old, new) = (config_of(&mut cmd, build, flags), config_of(&mut cmd, build_from_table, flags));
        if old != new {
            differ.push(format!("{}\n  old {:?}\n  new {:?}", flags.join(" "), old.map(|_| "…"), new.map(|_| "…")));
        }
    }
    assert!(differ.is_empty(), "{} of {} differ:\n{}", differ.len(), lines.len(), differ.join("\n"));
}
