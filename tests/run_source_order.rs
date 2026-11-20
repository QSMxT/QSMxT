//! Two orders in `qsmxt run`'s code that tests of its behaviour cannot see: the logger is up
//! before the flags are applied (whose warnings would otherwise go nowhere), and a dry run creates
//! nothing. Every option of `qsmxt run`, `qsmxt slurm` and `qsmxt aggregate` being read is the
//! compiler's to check: each `execute` takes its arguments apart, so an option nothing reads is an
//! unused variable, which CI's clippy refuses.

/// The logger must be up before the config is built, because building it is what validates the
/// CLI. Every "ignoring invalid ..." warning applying the flags emits went to a logger that did
/// not exist yet and was silently dropped — including the two-pass hole-filling warning, and the
/// `Discovered N run(s)` line. The check is positional because that is exactly the bug: both calls
/// were present, just in the wrong order.
#[test]
fn logging_is_initialised_before_the_config_is_validated() {
    let src = include_str!("../src/commands/run.rs");
    let body = &src[src.find("pub fn execute(").expect("execute() not found")..];

    let init = body.find("init_logging(").expect("run::execute must initialise logging");
    // The overrides are applied (and validated) inside prepare, by build_config.
    let overrides = body
        .find("prepare(")
        .expect("run::execute must prepare its config, applying the CLI overrides");

    assert!(
        init < overrides,
        "init_logging() must come before prepare(), or the warnings it emits are \
         written to a logger that does not exist yet and are silently dropped"
    );
}

/// A dry run must not create directories in the dataset — it reports what a real run would do.
#[test]
fn a_dry_run_does_not_create_the_derivatives_directory() {
    let src = include_str!("../src/commands/run.rs");
    let body = &src[src.find("pub fn execute(").expect("execute() not found")..];
    let guard = body.find("if dry {").expect("dry-run logging branch not found");
    let create = body.find("create_dir_all(&derivatives_dir)").expect("derivatives dir creation not found");
    let else_arm = body[guard..].find("} else").expect("dry-run else arm not found") + guard;
    assert!(
        create > else_arm,
        "creating derivatives/ must sit in the non-dry branch, so `--dry` writes nothing"
    );
}
