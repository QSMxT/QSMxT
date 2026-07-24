//! RunArgs-coverage guard.
//!
//! Sibling of qsmxt-config's tests/param_coverage.rs, one layer up: every
//! `qsmxt run` CLI flag must actually be consumed by the run pipeline. v9
//! shipped --masking-algorithm and --masking-input parsed but silently
//! ignored; this test goes red if a RunArgs field exists that neither
//! apply_run_overrides() nor run::execute() reads.
//!
//! The check is textual (the binary crate has no lib target to introspect):
//! a field `foo` counts as consumed if `args.foo` appears in the consumer
//! sources. Fields read via destructuring would need this test updated.

#[test]
fn every_run_arg_is_consumed() {
    let cli_src = include_str!("../src/cli.rs");
    let consumers = concat!(
        include_str!("../src/pipeline/config.rs"),
        include_str!("../src/commands/run.rs"),
    );

    let start = cli_src.find("pub struct RunArgs {").expect("RunArgs struct not found");
    let body = &cli_src[start..];
    let end = body.find("\n}").expect("RunArgs struct end not found");
    let body = &body[..end];

    let mut missing = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("pub ") {
            if let Some((name, _)) = rest.split_once(':') {
                let name = name.trim();
                if !consumers.contains(&format!("args.{}", name)) {
                    missing.push(name.to_string());
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "RunArgs field(s) {missing:?} are parsed but never applied — wire them \
         into apply_run_overrides() or run::execute(), or remove the flag(s)"
    );
}
