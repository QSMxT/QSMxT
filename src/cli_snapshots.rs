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

/// The configuration `qsmxt run <bids> <flags>` runs with, as JSON, or why it is refused.
fn config_of(cmd: &mut Command, flags: &[String]) -> Result<serde_json::Value, String> {
    let argv = ["qsmxt", "run", "/bids"].into_iter().map(String::from).chain(flags.iter().cloned());
    let matches = cmd.try_get_matches_from_mut(argv).map_err(|e| format!("clap: {}", e.to_string().trim_end()))?;
    let cli = Cli::from_arg_matches(&matches).map_err(|e| format!("clap: {e}"))?;
    let crate::cli::Command::Run(run) = cli.command else { unreachable!() };
    let config = crate::commands::run::build_config(None, &run.pipeline).map_err(|e| format!("refused: {e}"))?;
    Ok(serde_json::to_value(&config).unwrap())
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
fn try_flag(cmd: &mut Command, flag: &str, values: &[String]) -> (String, Result<serde_json::Value, String>) {
    let mut flags: Vec<String> = std::iter::once(flag.to_string()).chain(values.iter().cloned()).collect();
    let mut shown = flags.join(" ");
    let mut config = config_of(cmd, &flags);
    if let Err(e) = &config {
        let missing = missing_flags(e);
        if !missing.is_empty() {
            shown = format!("{shown} (with {})", missing.join(" "));
            flags.extend(missing);
            config = config_of(cmd, &flags);
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
fn flag_lines(cmd: &mut Command, base: &serde_json::Value, arg: &Arg) -> String {
    let mut out = String::new();
    let Some(long) = arg.get_long() else { return out };
    if arg.is_positional()
        || matches!(arg.get_action(), ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version) {
        return out;
    }
    let flag = format!("--{long}");
    let free = arg.get_possible_values().is_empty() && arg.get_default_values().is_empty()
        && !matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count);
    let mut tries: Vec<_> = samples(arg).iter().map(|values| try_flag(cmd, &flag, values)).collect();
    if free {
        // A flag that takes any value shows the first sample the run accepts; else every value
        // its parser lists when it refuses one; else the first sample clap accepts, else the
        // first.
        let n = arg.get_num_args().map_or(1, |r| r.min_values().max(1));
        let listed = tries.iter().find_map(|(_, c)| c.as_ref().err().map(|e| expected_values(e)).filter(|v| !v.is_empty()));
        tries = if let Some(i) = tries.iter().position(|(_, c)| c.is_ok()) {
            vec![tries.swap_remove(i)]
        } else if let Some(listed) = listed {
            listed.iter().map(|v| try_flag(cmd, &flag, &vec![v.clone(); n])).collect()
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
    let base = config_of(&mut cli(), &[]).expect("qsmxt run <bids> runs with the defaults");
    let args: Vec<Arg> = cli().find_subcommand("run").unwrap().get_arguments().cloned().collect();
    let chunk = args.len().div_ceil(THREADS);
    let parts: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = args.chunks(chunk).map(|part| {
            let base = &base;
            scope.spawn(move || {
                let mut cmd = cli();
                part.iter().map(|arg| flag_lines(&mut cmd, base, arg)).collect::<String>()
            })
        }).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    snapshot::check(DIR, "run_flags.txt", &normalise(&parts.concat()));
}

/// The command `generate_command` renders for `qsmxt run <bids> <flags>`'s configuration, and
/// the one the hand-written renderer gave, when they differ other than by the old one also
/// saying settings the configuration hides (which it did after a pipeline that sets them
/// differently: `--pipeline sti-suite-ilsqr --unwrapping-algorithm romeo` said
/// `--laplacian-solver dct`).
fn rendered_differently(cmd: &mut Command, flags: &[String]) -> Option<String> {
    use qsmxt_engine::schema::{cli::PIPELINE_FLAGS, cli_apply::{opposite_of, setting_of}, prune_hidden, settings_view, ViewFacts};
    let config: qsmxt_engine::PipelineConfig = serde_json::from_value(config_of(cmd, flags).ok()?).unwrap();
    let (new, old) = (qsmxt_engine::generate_command(&config), qsmxt_engine::command::generate_command_legacy(&config));
    if new == old {
        return None;
    }
    let facts = ViewFacts::default();
    let hidden: Vec<&str> = settings_view(&prune_hidden(&config.normalised().0, &facts), &facts).into_iter()
        .filter(|v| !v.visible).map(|v| v.path).collect();
    let is_hidden = |part: &str| {
        let long = part.split(' ').next().unwrap_or("");
        PIPELINE_FLAGS.iter().find(|f| f.long == long)
            .and_then(|f| setting_of(f).or_else(|| opposite_of(f).map(|(_, s)| s)))
            .is_some_and(|s| hidden.contains(&s.path))
    };
    // The new command is the old one with hidden settings left out.
    let mut new_parts = new.split(" --").peekable();
    let only_hidden_dropped = old.split(" --").all(|part| {
        if new_parts.peek() == Some(&part) {
            new_parts.next();
            true
        } else {
            is_hidden(part)
        }
    }) && new_parts.next().is_none();
    (!only_hidden_dropped).then(|| format!("{}\n  new: {new}\n  old: {old}", flags.join(" ")))
}

/// Every flag's samples after each of `contexts`, compared over a few threads.
fn compare_renderers(contexts: &[Vec<String>], first_sample_only: bool) {
    const THREADS: usize = 4;
    let args: Vec<Arg> = cli().find_subcommand("run").unwrap().get_arguments()
        .filter(|a| a.get_long().is_some() && !a.is_positional()
            && !matches!(a.get_action(), ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version))
        .cloned().collect();
    let mut runs: Vec<Vec<String>> = Vec::new();
    for context in contexts {
        runs.push(context.clone());
        for arg in &args {
            let samples = samples(arg);
            let samples = if first_sample_only { &samples[..1] } else { &samples[..] };
            for values in samples {
                runs.push(context.iter().cloned().chain(std::iter::once(format!("--{}", arg.get_long().unwrap())))
                    .chain(values.iter().cloned()).collect());
            }
        }
    }
    let chunk = runs.len().div_ceil(THREADS);
    let diffs: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = runs.chunks(chunk).map(|part| scope.spawn(move || {
            let mut cmd = cli();
            part.iter().filter_map(|flags| rendered_differently(&mut cmd, flags)).collect::<Vec<_>>()
        })).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    assert!(diffs.is_empty(), "{} of {} commands differ:\n{}", diffs.len(), runs.len(),
        diffs.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
}

/// The command rendered from the flag table is the hand-written renderer's, for every flag's
/// samples on QSMxT's defaults.
#[test]
fn the_renderer_matches_the_legacy_one() {
    compare_renderers(&[Vec::new()], false);
}

/// ... and after every pipeline, method and switch that shows settings of its own.
#[test]
#[ignore = "slow: every flag after each of ~100 contexts"]
fn the_renderer_matches_the_legacy_one_in_every_context() {
    let split = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
    let mut contexts: Vec<Vec<String>> = qsmxt_engine::pipelines::ids().iter().map(|id| split(&format!("--pipeline {id}"))).collect();
    let values = |long: &str| qsmxt_engine::schema::cli::PIPELINE_FLAGS.iter().find(|f| f.long == long).unwrap()
        .values.iter().filter(|v| !v.hide).map(|v| v.name).collect::<Vec<_>>();
    for (long, before) in [("qsm-algorithm", ""), ("qsmart-inversion", "--qsm-algorithm qsmart "), ("bf-algorithm", ""),
                           ("chisep", "--do-chisep "), ("b0-estimation", ""), ("unwrapping-algorithm", ""),
                           ("laplacian-solver", "--unwrapping-algorithm laplacian "), ("b0-weight-type", ""),
                           ("qsmart-fit-threshold-mode", "--b0-estimation qsmart-echofit "), ("vsharp-grid", ""),
                           ("chi-sepnet-route", "--do-chisep --chisep chi-sepnet "), ("r2prime-strategy", "--do-chisep --chisep chi-sep-ilsqr "),
                           ("inhomogeneity-method", ""), ("two-pass-mode", "--two-pass "), ("multi-orientation-algorithm", "--orientation-group acq ")] {
        for v in values(long) {
            contexts.push(split(&format!("{before}--{long} {v}")));
        }
    }
    for c in ["--do-swi", "--do-smwi", "--do-segmentation", "--do-analysis", "--motion-correction", "--heidi", "--two-pass",
              "--orientation-group acq", "--orientation-group acq --cosmos-magnitude-weighting", "--qsm-algorithm medi --medi-msmv",
              "--qsm-algorithm medi --medi-csf false", "--axial-resampling", "--romeo-individual", "--heidi --no-heidi-denoise",
              "--vsharp-deconvolve true", "--do-chisep --chisep decompose --decompose-fit lsqcurvefit", "--do-t2starmap",
              "--qsm-algorithm lsqr", "--pipeline heidi --heidi-denoise-reference-itk", "--pipeline sti-suite-ilsqr --laplacian-unmasked",
              "--qsm-algorithm iqsm", "--bf-algorithm bfrnet", "--mask-preset robust-threshold", "--two-pass --two-pass-mask magnitude,threshold:otsu"] {
        contexts.push(split(c));
    }
    compare_renderers(&contexts, true);
}
