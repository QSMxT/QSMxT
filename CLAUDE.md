# Instructions for coding agents

Read [CONTRIBUTING.md](CONTRIBUTING.md) before changing anything and follow it. The rules most
often broken:

- No algorithms in QSMxT: fix or add them in QSM.rs and move the pin. Algorithm details are
  documented in QSM.rs, not here.
- Never change a setting silently. Method-dependent values are QSM.rs rules offered in a pop-up;
  the CLI runs exactly what it is given.
- Every option change reaches every surface in "Change surfaces" (CLI, subcommands, TUI, engine,
  docs, coverage tests, qsmbly) in the same PR.
- Tests check wiring on tiny data, never algorithm numerics. Locally run only a build, clippy and
  the new or changed tests; push early and let CI run the full suite, the `slow:` ignored tests
  and repeats.
- No changelog files and no temporary qsm-core pins in a ready PR. Never push to `main`; the
  maintainer merges.
