//! RunArgs/PipelineArgs-coverage guard.
//!
//! Sibling of qsmxt-config's tests/param_coverage.rs, one layer up: every
//! `qsmxt run` CLI flag must actually be consumed by the run pipeline. v9
//! shipped --masking-algorithm and --masking-input parsed but silently
//! ignored; this test goes red if a RunArgs or PipelineArgs field exists that
//! neither apply_run_overrides() nor run::execute() reads.
//!
//! The check is textual (the binary crate has no lib target to introspect):
//! a field `foo` counts as consumed if `args.foo` appears in the consumer
//! sources. Fields read via destructuring would need this test updated.

fn struct_fields(cli_src: &str, name: &str) -> Vec<String> {
    let start = cli_src
        .find(&format!("pub struct {} {{", name))
        .unwrap_or_else(|| panic!("{} struct not found", name));
    let body = &cli_src[start..];
    let end = body.find("\n}").unwrap_or_else(|| panic!("{} struct end not found", name));
    body[..end]
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub "))
        .filter_map(|rest| rest.split_once(':'))
        .map(|(name, _)| name.trim().to_string())
        .collect()
}

#[test]
fn every_run_arg_is_consumed() {
    let cli_src = include_str!("../src/cli.rs");
    let consumers = concat!(
        include_str!("../src/pipeline/config.rs"),
        include_str!("../src/commands/run.rs"),
    );

    let mut fields = struct_fields(cli_src, "RunArgs");
    fields.extend(struct_fields(cli_src, "PipelineArgs"));

    let missing: Vec<_> = fields
        .into_iter()
        .filter(|name| !consumers.contains(&format!("args.{}", name)))
        .collect();
    assert!(
        missing.is_empty(),
        "RunArgs/PipelineArgs field(s) {missing:?} are parsed but never applied — wire them \
         into apply_run_overrides() or run::execute(), or remove the flag(s)"
    );
}

/// The SLURM path must apply the same pipeline overrides as the local path —
/// otherwise generated jobs silently run the defaults (issue: TUI SLURM mode
/// wrote a default pipeline_config.toml regardless of the chosen settings).
#[test]
fn slurm_flattens_and_applies_pipeline_args() {
    let cli_src = include_str!("../src/cli.rs");
    let slurm_start = cli_src.find("pub struct SlurmArgs {").expect("SlurmArgs not found");
    let slurm_body = &cli_src[slurm_start..];
    let slurm_body = &slurm_body[..slurm_body.find("\n}").expect("SlurmArgs end not found")];
    assert!(
        slurm_body.contains("pub pipeline: PipelineArgs"),
        "SlurmArgs must flatten PipelineArgs so `qsmxt slurm` takes the same pipeline flags as `qsmxt run`"
    );

    let slurm_cmd = include_str!("../src/commands/slurm.rs");
    assert!(
        slurm_cmd.contains("apply_run_overrides(&mut config, &args.pipeline)"),
        "slurm::execute() must apply the pipeline overrides before generating job scripts"
    );
}

/// The logger must be up before the config is built, because building it is what validates the
/// CLI. Every "ignoring invalid ..." warning `apply_run_overrides` emits went to a logger that did
/// not exist yet and was silently dropped — including the two-pass hole-filling warning, and the
/// `Discovered N run(s)` line. The check is positional because that is exactly the bug: both calls
/// were present, just in the wrong order.
#[test]
fn logging_is_initialised_before_the_config_is_validated() {
    let src = include_str!("../src/commands/run.rs");
    let body = &src[src.find("pub fn execute(").expect("execute() not found")..];

    let init = body.find("init_logging(").expect("run::execute must initialise logging");
    let overrides = body
        .find("apply_run_overrides(")
        .expect("run::execute must apply CLI overrides");

    assert!(
        init < overrides,
        "init_logging() must come before apply_run_overrides(), or the warnings it emits are \
         written to a logger that does not exist yet and are silently dropped"
    );
}

/// A dry run must not create directories in the dataset — it reports what a real run would do.
#[test]
fn a_dry_run_does_not_create_the_derivatives_directory() {
    let src = include_str!("../src/commands/run.rs");
    let body = &src[src.find("pub fn execute(").expect("execute() not found")..];
    let guard = body.find("if args.dry {").expect("dry-run logging branch not found");
    let create = body.find("create_dir_all(&derivatives_dir)").expect("derivatives dir creation not found");
    let else_arm = body[guard..].find("} else {").expect("dry-run else arm not found") + guard;
    assert!(
        create > else_arm,
        "creating derivatives/ must sit in the non-dry branch, so `--dry` writes nothing"
    );
}
