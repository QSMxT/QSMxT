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
