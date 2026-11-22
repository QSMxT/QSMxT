# Instructions for coding agents

Read [CONTRIBUTING.md](CONTRIBUTING.md) before changing anything and follow it. The rules most
often broken:

- No algorithms in QSMxT: fix or add them in QSM.rs and move the pin. Algorithm details,
  method names and citations live in QSM.rs, not here.
- Never change a setting silently. Method-dependent values are QSM.rs rules offered through
  `qsmxt_engine::advice`; the CLI runs exactly what it is given.
- A setting is described once, in the settings registry (`schema/table/`); CLI flags, TUI rows
  and the docs' settings reference derive from it. Go through "Change surfaces" in the same PR.
- Snapshots and generated docs are regenerated, never hand-edited; a changed snapshot goes in
  its own commit with the reason in the PR. Keep files and functions within the size budgets.
- Tests check wiring on tiny data, never algorithm numerics. Push early: locally run only a
  build, clippy and the new or changed tests (about 2 minutes at most); CI runs the full suite,
  the `slow:` ignored tests and repeats.
- No changelog files and no temporary qsm-core pins in a ready PR. Never push to `main`; the
  maintainer merges.
