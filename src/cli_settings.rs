//! `qsmxt run`'s settings flags, built from the engine's flag table
//! ([`qsmxt_engine::schema::cli::PIPELINE_FLAGS`]).

// Not wired up yet: `run`, `slurm` and `aggregate` switch to it next (refactor P3).
#![cfg_attr(not(test), allow(dead_code))]

use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{Arg, ArgAction, Command};
use qsmxt_engine::schema::cli::{Flag, FlagAction, PIPELINE_FLAGS};

/// One flag as a clap argument.
pub fn arg(f: &Flag) -> Arg {
    let mut a = Arg::new(f.id).long(f.long).num_args(f.num_args.0..=f.num_args.1).action(match f.action {
        FlagAction::Set => ArgAction::Set,
        FlagAction::SetTrue => ArgAction::SetTrue,
        FlagAction::Append => ArgAction::Append,
    });
    if f.num_args.1 == usize::MAX {
        a = a.num_args(f.num_args.0..);
    }
    if !f.value_names.is_empty() {
        a = a.value_names(f.value_names);
    }
    if let Some(h) = f.help {
        a = a.help(h);
    }
    if let Some(h) = f.long_help {
        a = a.long_help(h);
    }
    if !f.aliases.is_empty() {
        a = a.aliases(f.aliases);
    }
    if !f.visible_aliases.is_empty() {
        a = a.visible_aliases(f.visible_aliases);
    }
    if !f.values.is_empty() {
        a = a.value_parser(PossibleValuesParser::new(f.values.iter().map(|v| {
            let mut p = PossibleValue::new(v.name).hide(v.hide).aliases(v.aliases.iter().copied());
            if let Some(h) = v.help {
                p = p.help(h);
            }
            p
        })));
    }
    if !f.defaults.is_empty() {
        a = a.default_values(f.defaults);
    }
    if !f.default_missing.is_empty() {
        a = a.default_missing_values(f.default_missing);
    }
    for c in f.conflicts {
        a = a.conflicts_with(c);
    }
    for r in f.requires {
        a = a.requires(r);
    }
    a.hide(f.has("hide"))
        .allow_hyphen_values(f.has("allow_hyphen_values"))
        .allow_negative_numbers(f.has("allow_negative_numbers"))
        .hide_possible_values(f.has("hide_possible_values"))
        .hide_default_value(f.has("hide_default_value"))
}

/// `cmd` with every settings flag added, in the table's order.
pub fn augment(cmd: Command) -> Command {
    cmd.args(PIPELINE_FLAGS.iter().map(arg))
}

#[cfg(test)]
mod tests {
    use clap::{Args, Command};

    /// The flags built from the table are today's `PipelineArgs`: the same arguments, and the same
    /// help.
    #[test]
    fn the_table_builds_pipeline_args() {
        let build = |mut c: Command| { c = c.term_width(100).max_term_width(100); c.build(); c };
        let mut derived = build(crate::cli::PipelineArgs::augment_args(Command::new("t")));
        let mut table = build(super::augment(Command::new("t")));
        let lines = |c: &Command| c.get_arguments().map(crate::cli_snapshots::describe).collect::<Vec<_>>();
        let (d, t) = (lines(&derived), lines(&table));
        for (a, b) in d.iter().zip(&t) {
            assert_eq!(a, b);
        }
        assert_eq!(d.len(), t.len());
        // From the usage on: derive adds the struct's doc as the command's about.
        let from_usage = |h: String| h[h.find("Usage:").unwrap()..].to_string();
        assert_eq!(from_usage(derived.render_long_help().to_string()), from_usage(table.render_long_help().to_string()));
        assert_eq!(from_usage(derived.render_help().to_string()), from_usage(table.render_help().to_string()));
    }
}
