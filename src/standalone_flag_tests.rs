//! The standalone commands take `qsmxt run`'s names for their settings flags
//! ([`crate::cli_settings::SettingsOf`]); the names they had before still work, as hidden
//! aliases, so scripts written against them keep running.

use clap::{Command, CommandFactory};

fn find<'a>(cmd: &'a Command, path: &str) -> &'a Command {
    path.split(' ').skip(1).fold(cmd, |c, name| {
        c.find_subcommand(name).unwrap_or_else(|| panic!("{path}: no subcommand {name}"))
    })
}

/// Every flag a standalone command took before (`tests/standalone_flags_before.txt`) is one of its
/// flags now, by name or as an alias.
#[test]
fn every_old_flag_still_parses() {
    let mut cli = crate::cli::Cli::command();
    cli.build();
    let mut missing = Vec::new();
    for line in include_str!("../tests/standalone_flags_before.txt").lines().filter(|l| !l.starts_with('#')) {
        let (path, flags) = line.split_once(": ").expect("<command>: <flags>");
        let cmd = find(&cli, path);
        for flag in flags.split(' ') {
            let known = cmd.get_arguments().any(|a| {
                a.get_long() == Some(flag) || a.get_all_aliases().is_some_and(|al| al.contains(&flag))
            });
            if !known {
                missing.push(format!("{path} --{flag}"));
            }
        }
    }
    assert!(missing.is_empty(), "no longer accepted:\n{}", missing.join("\n"));
}

/// An old name reaches the same setting as the `qsmxt run` name.
#[test]
fn an_old_name_sets_what_the_run_name_sets() {
    use clap::Parser;
    let rts = |flag: &str| {
        let cli = crate::cli::Cli::try_parse_from(["qsmxt", "invert", "rts", "f.nii", "-m", "m.nii", "-o", "o.nii", flag, "7"]).unwrap();
        let crate::cli::Command::Invert { command: crate::cli::InvertCommand::Rts(a) } = cli.command else { panic!("invert rts") };
        let mut c = qsmxt_engine::PipelineConfig::default();
        qsmxt_io::settings::apply(&mut c, &a.settings.0).unwrap();
        c.inversion.rts.max_iter
    };
    assert_eq!(rts("--max-iter"), 7);
    assert_eq!(rts("--rts-max-iter"), 7);
}
