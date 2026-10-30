# Contributing to QSMxT

QSMxT is a BIDS-native QSM pipeline: a command line (`qsmxt run` and standalone subcommands),
an interactive TUI, and `qsmxt-engine`, the I/O-free crate that holds the pipeline's
configuration, planning, run logic, settings schema, command generation and methods text. This
file is how the repository is run: the principles, the conventions the code follows, and the
processes for issues, pull requests, CI and releases. Usage documentation is on the
[documentation site](https://qsmxt.github.io/QSMxT/), not here.

## Related repositories

```
QSM.rs (qsm-core)  →  QSMxT (qsmxt-engine + qsmxt binary)  →  qsmbly (browser)
algorithms             wiring, config, CLI/TUI, BIDS           the engine in WebAssembly
```

- **[QSM.rs](https://github.com/astewartau/QSM.rs)** (`qsm-core`) holds every algorithm:
  unwrapping, echo combination, field mapping, background removal, dipole inversion, masking,
  resampling, coil combination, χ-separation, deep-learning models. It also holds the pipeline
  rules (what each method's reference pipeline uses) and the algorithm documentation: defaults
  and why, ports' sources, deviations from the reference code.
- **QSMxT** (this repository) wires those algorithms into a pipeline. It owns parameter checks,
  how steps link together, the config and its migration, the CLI and TUI, BIDS input and output,
  provenance, methods text, caching and memory estimates.
- **[qsmbly](https://github.com/astewartau/qsmbly)** runs `qsmxt-engine` (and, through its
  `qsm_core` re-export, qsm-core) in the browser. It gets the settings schema, plan checks and
  methods text from the engine, so anything it shows comes from here.

What belongs where: if it inspects or transforms image data, it is QSM.rs's. If it decides what
runs, with which settings, on which files, or how it is reported, it is QSMxT's. A browser-only
concern (UI, storage, workers) is qsmbly's.

## Principles

1. **No algorithms in QSMxT.** QSM.rs provides every algorithm; QSMxT only links them together.
   When qsm-core lacks something or gets it wrong, fix it in QSM.rs and move the pin; don't add a
   QSMxT-side workaround. *Why:* one implementation, tested once, shared with qsmbly. Simple
   numerics and ROI analysis (region statistics, referencing to a region, T2* = 1/R2*, cohort
   aggregation, unit conversions) are fine here. Test-only reference implementations are fine.
2. **Defaults come from qsm-core.** Take a default from the qsm-core `*Params::default()` it
   belongs to, never a copied literal. Where QSM.rs's default is the reference implementation's
   value, QSMxT follows it. *Why:* a copied number goes stale the next time QSM.rs changes it.
3. **Never change a setting silently.** Choosing one setting must not rewrite another (choosing
   MEDI does not switch background removal to PDF). A method's reference values are offered: the
   TUI's reference-pipeline pop-up (Apply preselected), and in the log, `--dry` and the methods
   Note for the CLI, which runs exactly the settings it is given. A hard requirement is refused at
   plan time with a message naming the setting; it is never fixed up. Don't add "this default
   depends on another setting" logic: express it as a QSM.rs rule and let the pop-up offer it.
   *Why:* users must be able to trust that the run did what they set.
4. **Every option reaches every surface.** An option added, renamed, removed or re-defaulted
   lands on the CLI, the standalone subcommand, the TUI, the engine, the docs and the coverage
   tests in the same PR (see [Change surfaces](#change-surfaces)). *Why:* a change in one place
   leaves the surfaces disagreeing.
5. **Nothing is skipped silently.** An output the user asked for that the data can't produce is
   an error for every run before the first one starts. An output another option turned on is a
   warning naming that option. *Why:* a run that quietly did less is worse than one that stops.
6. **Old settings files keep working.** A file written by any earlier release reads correctly
   through `migrate.rs`, with a note or warning for every change. *Why:* configs are how runs are
   reproduced.
7. **Old installs can still update.** `qsmxt update` must keep working from every release where it
   ever worked. *Why:* updaters are frozen inside users' binaries.
8. **Tests test wiring, not algorithms.** QSM.rs's CI guarantees algorithm correctness. QSMxT's
   tests check that each option gets the right inputs, settings and outputs. *Why:* that is what
   QSMxT can break, and it keeps CI fast.
9. **Every problem has an owner.** Each issue states whether QSMxT or QSM.rs owns the fix, and is
   filed in the owner's repository. *Why:* a fix in the wrong repo becomes a workaround.

## Conventions

### Layout

- `crates/qsmxt-engine/` — no file I/O, builds for `wasm32-unknown-unknown`. `config.rs`
  (`PipelineConfig`), `migrate.rs` (old files), `bridge.rs` (config → qsm-core params),
  `schema/` (the settings schema qsmbly renders; `table.rs` is the settings table), `command.rs`
  (the equivalent command line), `methods.rs` (methods text and citations), `requirements.rs`
  (plan-time checks), `recommendations.rs` (QSM.rs's rules applied to a config), `plan.rs` /
  `run.rs` / `stages/` (deciding and running steps), `graph.rs` and `cache.rs` (step keys and
  invalidation), `memory.rs` (memory estimates), `record.rs` (the run record).
- `src/` — the `qsmxt` binary: `cli.rs` (clap definitions), `commands/` (one file per
  subcommand), `tui/` (`app.rs` form state, `ui.rs` rendering, `schema_parity.rs`),
  `pipeline/runner.rs` (file I/O around the engine's run), `bids/`, `dicom/`, `nifti/`,
  `executor/` (local and SLURM), `cli_reference.rs` (the generated CLI page and the docs checks).
- Run logic moves from `src/pipeline/` into the engine over time. New logic that needs no I/O goes
  in the engine, so qsmbly gets it too.
- Standalone subcommands (`qsmxt invert`, `bgremove`, `fieldmap`, ...) call the same qsm-core
  pipeline dispatcher `qsmxt run` does (`run_dipole_inversion`, `run_bg_removal`, ...), not the
  algorithm functions directly, so the two can't disagree about inputs.
- Large test modules live next to their module as `<module>/tests.rs` or `<module>/*_tests.rs`.

### Code

- Rust 2021, stable toolchain. Clippy must pass with `-D warnings` on all targets.
- **Formatting:** the codebase is not rustfmt-clean and CI does not check formatting. Don't run
  `cargo fmt` over the workspace or over whole files you only touched in passing; match the
  surrounding style. Lines and comments wrap at about 100 columns.
- **Names:** config keys are snake_case, grouped by section (`bg_removal.vsharp.radius_step`).
  `qsmxt run` flags are kebab-case and prefixed with the method (`--vsharp-radius-step`); the
  standalone subcommand drops the prefix (`qsmxt bgremove vsharp --radius-step`).
- **Units in names** when a value has one: `_mm`, `_s`, `_ms`, `_hz`, `_gb` (`pad_mm`,
  `phase_offset_sigma_mm`, `mem_limit_gb`). When a setting's unit changes, rename it, so an old
  file can't be read with the wrong meaning.
- **Errors:**
  - Engine: `EngineError` (`Config` for settings or inputs the run can't use,
    `DimensionMismatch`, `InvalidInput` for what qsm-core refused, converted from
    `qsm_core::error::InputError`) and `ConfigError` for parsing and validation. Each message is
    complete as it stands; front ends show it as it is.
  - Binary: `QsmxtError`, one variant per kind.
  - Plan-time problems are `requirements::Problem` values (severity, the settings involved by
    config path, a reason and a whole-sentence message using schema labels), so qsmbly can word
    them itself. The CLI uses the `errors`/`warnings` strings of the same `Check`.
  - Messages name the run, the setting and what to do ("`--mask`", "needs magnitude images, and
    this run has none"). No panics on user input.
- **Feature flags:** the binary's `dl` (default) turns on ONNX models and weight download; the
  `aarch64-pc-windows-msvc` release is built without it and must still build. The engine forwards
  qsm-core's `simd`, `onnx` and `parallel`, all off by default, for qsmbly. `introspection` is
  test-only (param_coverage). Keep the engine building for wasm with and without `onnx`.
- **Dependencies:** add one only when it earns its place, and say why in a comment in
  `Cargo.toml` when it isn't obvious (see the `fontdue`/`png` note). Never add an image-processing
  dependency to QSMxT; that is QSM.rs's.

### Comments and docs in code

- **`//!` module docs** say what the module is for and the policy it implements, in a few short
  paragraphs, and point to the issue that set the policy (`(#193, #273)`). A module with rules
  (migration, requirements) lists them as bullets.
- **`///` item docs** on every public item and on non-obvious private ones: what the thing is or
  returns, in one or two full sentences, then anything a caller must know (units, ranges, what
  `None` means, who else reads it). Link related items with ``[`path`]``.
- **Inline comments explain why**, not what: the reason for a choice, the case it guards, what
  would break without it. Don't narrate the code. Density is low; most functions have none.
- **Sources.** Where a value or rule comes from a reference implementation, name it: toolbox,
  file and function or line (`MEDI toolbox Fit_ppm_complex`), or the QSM.rs item it mirrors
  (`qsm_core::utils::EPG_TE_TOLERANCE_S`). Algorithm decisions themselves are documented in
  QSM.rs, not here.
- **Units** go in the doc comment when the name can't carry them ("in seconds", "ppm").
- **Issue numbers** in comments are fine for the why (`#275`). No history: don't write what the
  code used to do or which release changed it, unless the code still handles that case (migration,
  the updater contract).
- Workflow files carry the same kind of comments: what a step guards and why it is there.

### User-facing text

This covers TUI labels and help, CLI help, log messages, docs pages and the generated methods
text.

- **Labels** are sentence case; names and acronyms keep their own case (ROMEO, MEDI, V-SHARP,
  iLSQR, R2*, χ). `schema_coverage` checks the schema's labels. Full labels are not abbreviated.
- **Say plainly what was done or what to do.** No hype, no defensive explanations.
- **Don't mention** previous QSMxT versions, other toolboxes as comparisons, experiments, or
  specific datasets or studies. Literature citations are fine and go in the references.
- **Methods text:**
  - states what was done ("we did x"), never what wasn't ("rather than y");
  - phrases settings as settings ("with deconvolution disabled");
  - gives parameter values only where they differ from QSMxT's defaults;
  - gives the useful facts about a tool, not what the tool is;
  - avoids ambiguous scope ("set to zero outside the brain mask", not "outside those voxels");
  - lists departures from a method's reference pipeline in the Note under the references.
- **Help-text defaults** are stated in the help ("default: 1 mm") and pinned to qsm-core's
  `Default` by a test, so they can't drift.

### TUI

- Every setting on the CLI is in the TUI with all its parameters; `schema_parity` checks the TUI
  against the engine schema (labels, help, visibility).
- Labels and help come from the engine schema, so the TUI, qsmbly and the docs agree.
- **Selectors:** a selector is an Enter-only picker when it has more than 3 options or when
  choosing it can open a reference-pipeline pop-up (which settings can comes from the rules data,
  not a list). On a picker row, ←/→ open the picker. Its value is drawn `[ medi ▾ ]` and the help
  line ends "— Enter: choose method" (or "Enter: choose <setting>"). Selectors with 3 or fewer
  options that aren't methods cycle with ←/→. The footer's key hints follow the focused row.
- Pop-ups: offered changes have Apply preselected; Esc changes nothing; "Keep mine" is the word for
  declining.
- The TUI shows the equivalent command line; `command.rs` renders it from the config, emitting
  only what differs from the defaults.

### Tests

- Test wiring: the plan, `--dry`, the step graph, cache keys, the config → params mapping,
  output names and sidecars, methods text, migration. None of these needs an algorithm to run.
- Never assert algorithm numerics. If a check really is about an algorithm's output, it belongs in
  QSM.rs (check QSM.rs doesn't already cover it).
- Keep only a handful of end-to-end runs, on tiny synthetic data (`src/testutils.rs`, 8³
  volumes) with the fastest choices (TKD, simple thresholds). Use mocks or stubs where data must
  flow through.
- Names are sentences saying what holds: `a_skipped_step_differs`,
  `the_profile_is_idempotent_and_keeps_what_the_user_set`.
- Don't delete or weaken what a test checks to make it pass or run faster; rewrite it to check
  the same thing more cheaply.
- Coverage guards, which a new option must satisfy:
  - `crates/qsmxt-engine/tests/param_coverage.rs`: every qsm-core `*Params` field is in its
    `*Config` or on the test's allow-list. Remove allow-list entries you resolve.
  - `crates/qsmxt-engine/tests/schema_coverage.rs`: every config leaf is a schema setting exactly
    once (or in `NOT_IN_SCHEMA`), and every enum value is offered.
  - `tests/run_args_coverage.rs`: every `qsmxt run` flag is read by the run.
  - `src/tui/schema_parity.rs`: the TUI matches the schema.
  - `src/cli_reference.rs`: `cli.md` is current, and every flag, command and algorithm the docs
    name exists.
  - `tests/release_asset_contract.rs`: release asset names and layout old updaters need.
  - `crates/qsmxt-engine/tests/memory_calibration.rs`: estimates against measured peaks.
- Line coverage must stay at 70% or more (the release gate); 90% is the target.
- Profile slow tests (`cargo nextest`) and fix the long tail over about 1 s, not only the slowest
  few. CI should stay under about 5 minutes.

## Change surfaces

For every option you add, rename, remove, or change the default or text of, go through all of
these in the same PR:

- [ ] `qsmxt run` flag in `src/cli.rs`
- [ ] the matching standalone subcommand in `src/commands/*.rs`
- [ ] the TUI row in `src/tui/app.rs` (and `schema_parity` passes)
- [ ] engine `config.rs`: field and default (from qsm-core's `Default`)
- [ ] engine `migrate.rs` if files written by an earlier release are affected (see below)
- [ ] engine `bridge.rs`: config → qsm-core params, each setting passed explicitly
      (`..Default::default()` only for scan parameters the pipeline fills in)
- [ ] engine `schema/table.rs`: the setting, its label, help, bounds and visibility
- [ ] engine `command.rs`: the rendered command line
- [ ] engine `methods.rs`: methods text and citations
- [ ] the cache / step key (`graph.rs`, `cache.rs`): a setting that changes a step's output is in
      its key
- [ ] engine `requirements.rs`: plan-time checks
- [ ] engine `memory.rs`: the estimate, re-measured if the memory a step uses changes
- [ ] docs: `docs/src/content/docs/reference/{algorithms,configuration,inputs,tools}.md` as
      relevant, and `cli.md` regenerated (below)
- [ ] coverage tests: `param_coverage` (remove resolved allow-list entries), `schema_coverage`,
      `run_args_coverage`
- [ ] qsmbly: anything it needs beyond the schema, listed in the PR body under "For qsmbly"

Before opening the PR, grep for every changed name and old default (flags, config keys, labels,
numbers in help text and docs).

### Settings files and migration

- `CONFIG_SCHEMA_VERSION` in `migrate.rs` is the layout of the files QSMxT writes, and every file
  it writes carries `schema_version`. Bump it when the layout changes in a way an older reader
  can't take.
- When a default changes, add the old value to `OLD_DEFAULTS`, checked against what each released
  `qsmxt init` wrote, so a full file from an earlier release takes today's default.
- When a setting's meaning or unit changes, rename it, and refuse an old non-default value (with
  a message saying what changed) when its method runs.
- Removed settings are dropped, with a warning where dropping one changes what a run does;
  renamed ones keep their value under the new name.
- Every change made while reading a file is a note or a warning; none is silent.
- qsmbly reads stored configs through `from_json_migrating`; keep it working.

## Processes

### Issues

- Every issue has an **Owner** line and an owner label: `owner: QSMxT` (wiring, config, CLI/TUI,
  BIDS, engine, parameter checks) or `owner: QSM.rs` (algorithms, image processing). A problem
  QSM.rs owns is filed in QSM.rs, with the QSMxT follow-up noted. If both repos need changes,
  file it in QSM.rs and note the QSMxT part.
- Labels:
  - type: `bug`, `enhancement`, `documentation`, `question`;
  - effort: `effort: small` (under half a day), `effort: medium` (a focused PR, a day or two),
    `effort: large` (design and several PRs, or cross-repo);
  - state: `outdated?` (looks fixed or superseded, proposed for closing), `needs info`.
- An issue proposed for closing gets a comment saying why before it is closed.

### Branches and worktrees

- Never push to `main`. Work on a branch named `<type>/<short-name>`: `feat/`, `fix/`, `chore/`,
  `docs/`, `perf/`, `ci/`.
- Use one git worktree per branch, and one `CARGO_TARGET_DIR` per worktree. Don't share a target
  directory between worktrees of this repo: cargo identifies the workspace crates by path, so
  worktrees overwrite each other's builds. If you seed a target directory from another one, run
  `cargo clean -p qsmxt -p qsmxt-engine` afterwards.

### Pull requests

- **Title:** a plain sentence saying what changes, as it will read in the history ("Echo time
  per method, and the method settings QSM.rs v0.40.0 added"). No `feat:` prefixes.
- **Body:**
  - `Fixes #N` for each issue it closes;
  - **What changed**, by item, with the names of the options, functions and files a reviewer
    needs;
  - **Behaviour changes users see**, and in particular anything that changes **at default
    settings**, called out as such;
  - the **per-surface checklist** (above), or a line saying why no surface changes;
  - **Decisions taken**: each design choice and why;
  - **For qsmbly** and **Follow-ups** where there are any, including QSM.rs issues it depends on;
  - the checks run (clippy, tests).
- Draft until CI is green and the body is complete.
- **Stacked PRs:** a PR may target another PR's branch while that one is open. Rebase it and
  **retarget it to `main` before merging**; merging into a branch that has already been squashed
  into `main` loses the commits.
- No `CHANGELOG.md` or release-notes files. The PR body is the record; release notes are drafted
  from merged PRs.

### CI

`.github/workflows/ci.yml` runs on every push to `main` and every PR (whatever its base):

| Job | Runs | Locally |
|---|---|---|
| Check, Clippy & Test | clippy; the whole suite under coverage with nextest; param_coverage; the 70% gate | `cargo clippy --workspace --all-targets -- -D warnings`<br>`cargo nextest run --workspace`<br>`cargo test -p qsmxt-engine --features introspection --test param_coverage` |
| WebAssembly check | the engine for wasm, with and without `onnx` | `cargo check -p qsmxt-engine --target wasm32-unknown-unknown --features simd`<br>`RUSTFLAGS="-C target-feature=+simd128" cargo check -p qsmxt-engine --target wasm32-unknown-unknown --features simd,onnx` |
| BIDS validator | a full-feature run's derivatives against the BIDS validator | `QSMXT_BIDS_FIXTURE_OUT=/tmp/bids cargo test --bin qsmxt -- --ignored --exact commands::integration_tests::bids_validator_fixture`, then `deno run -A jsr:@bids/validator@3.0.2 <dir>/derivatives/qsmxt` |

Coverage, as CI measures it:

```sh
cargo llvm-cov nextest --workspace --lcov --output-path target/lcov.info \
  --ignore-filename-regex '(^|/)src/(main|tui/mod|tui/ui|commands/update)\.rs$'
python3 .github/scripts/coverage_summary.py target/lcov.info
```

`install.yml` tests `install.sh` and `install.ps1` when they change. `docs.yml` builds and deploys
the documentation site on pushes to `main` that touch `docs/`.

### Merging

- Squash merges only. The squash commit is the PR title with `(#N)`.
- The maintainer merges. Agents never merge.

### Dependency on QSM.rs

- `qsm-core` is a git dependency, pinned by tag or by rev, in both `Cargo.toml` and
  `crates/qsmxt-engine/Cargo.toml`. The two pins must be identical, so qsmbly (which pins
  `qsmxt-engine`) resolves the same qsm-core.
- Moving the pin is its own PR, titled for what it brings ("Move qsm-core to QSM.rs v0.40.0; ..."),
  with every follow-up the new qsm-core needs (removed workarounds, new parameters through every
  surface). QSM.rs breaks its API freely; the pin PR catches up.
- When QSMxT needs something from QSM.rs, open a QSM.rs issue (or PR) and wait for it rather than
  working around it here.

### Releases

- The version in both `Cargo.toml` files stays `0.0.0`. Publishing a GitHub release runs
  `release.yml`, which sets the version from the tag, re-checks coverage, builds every target,
  publishes `SHA256SUMS`, and notifies qsmbly.
- Release notes are written at release time from the merged PRs and published on the GitHub
  release.
- **Don't break old updaters.** Every released `qsmxt update` downloads
  `qsmxt-<tag>-<target>.tar.gz` (or `.zip` on Windows) and expects `qsmxt` / `qsmxt.exe` at the
  archive root. Keep those names, formats and layout; extra assets are fine.
  `tests/release_asset_contract.rs` guards this. Before changing packaging or assets, check every
  released updater's asset selection and extraction.
- **qsmbly bump:** the `notify-qsmbly` job sends a `qsmxt-engine-release` dispatch to qsmbly,
  whose bump workflow opens a PR moving its `qsmxt-engine` pin. Anything qsmbly must change to
  take the release (new schema fields, migration calls, browser memory figures) is listed in the
  QSMxT PR bodies under "For qsmbly", so that PR can pick it up.
- Releases are cut by the maintainer.

### Documentation

- The site is Astro Starlight under `docs/` (`cd docs && npm ci && npm run build`).
- `reference/cli.md` is generated from the clap definitions. Don't edit it by hand; regenerate
  it with `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference`.
- The tests in `src/cli_reference.rs` fail when a docs page names a flag, command or algorithm the
  binary doesn't have, or gets the inversion counts or default wrong.
- `README.md` covers only what QSMxT is, install, quick start and pointers to the site. Guides,
  references and algorithm details go on the site.
- Algorithm details (deviations, reference quirks, determinism) are documented in QSM.rs, not on
  QSMxT's site.

## Before opening a PR

- [ ] The change is in the right repository (algorithms in QSM.rs) and has an owner.
- [ ] No setting changes another setting; reference values are offered, not applied.
- [ ] Every surface is updated, and the PR body has the checklist.
- [ ] Old settings files: `migrate.rs` updated if a default, name or meaning changed.
- [ ] Defaults come from qsm-core, not literals.
- [ ] User-facing text follows the rules above; `cli.md` regenerated.
- [ ] Tests check wiring on tiny data; no algorithm numerics.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo nextest run --workspace`
      pass; rustfmt only on what you changed.
- [ ] The PR body says what changes at default settings.
