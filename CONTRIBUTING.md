# Contributing to QSMxT

QSMxT is a BIDS-native QSM pipeline. The workspace has four crates:

| Crate | Holds |
|---|---|
| `qsmxt-engine` (`crates/qsmxt-engine`) | No file I/O; builds for wasm32 and is what qsmbly runs. Configuration, the settings registry, plan, step graph, run logic, advice, command line, methods text. |
| `qsmxt-io` (`crates/qsmxt-io`) | The file side: BIDS, DICOM and NIfTI I/O, executors, the pipeline runner, update and news. |
| `qsmxt-tui` (`crates/qsmxt-tui`) | The TUI. |
| `qsmxt` (`src/`) | The binary: the command line (`src/cli/`) and one file per subcommand (`src/commands/`). |

Usage documentation is on the [documentation site](https://qsmxt.github.io/QSMxT/).

## Where code belongs

```
QSM.rs (qsm-core)  →  QSMxT (qsmxt-engine + qsmxt)  →  qsmbly (browser)
algorithms             wiring, config, CLI/TUI, BIDS     the engine in WebAssembly
```

- **[QSM.rs](https://github.com/astewartau/QSM.rs)** owns anything that inspects or transforms
  image data, the pipeline rules (what each method's reference pipeline uses) and all algorithm
  documentation: defaults and why, sources, deviations.
- **QSMxT** owns what runs, with which settings, on which files, and how it is reported:
  parameter checks, step linking, config, CLI/TUI, BIDS, provenance, methods text, caching and
  memory estimates.
- **[qsmbly](https://github.com/astewartau/qsmbly)** runs `qsmxt-engine` in the browser. New
  logic that needs no I/O goes in the engine so qsmbly gets it too.

Every issue has an **Owner** line and label (`owner: QSMxT` or `owner: QSM.rs`) and is filed in
the owner's repository. If both need changes, file it in QSM.rs and note the QSMxT follow-up.

## Principles

1. **No algorithms in QSMxT.** When qsm-core lacks something or gets it wrong, fix it in QSM.rs
   and move the pin; no QSMxT-side workaround. Simple numerics are fine here (region statistics,
   referencing, T2\* = 1/R2\*, unit conversions), as are test-only reference implementations.
2. **Defaults come from qsm-core's `*Params::default()`**, not copied literals. QSMxT's own
   recommended values are QSM.rs rules, not hidden defaults. Method names and citations come
   from QSM.rs's method catalogue (`qsm_core::catalogue`): fix a citation there.
   `methods/citations.rs` holds only works that are not a method's (QSMxT itself, consensus
   papers).
3. **Never change a setting silently.** Choosing one setting must not rewrite another. Don't add
   "this default depends on another setting" logic: express it as a QSM.rs rule. What a front
   end offers, and what starts ticked, is decided once in `qsmxt_engine::advice` for the TUI,
   qsmbly and `--accept-*`.
   - The advice pop-up is a checklist of changes. A recommendation is preselected; an original
     authors' setting or a suggestion preselects **Keep mine**; nothing is preselected when
     equally strong advice disagrees.
   - The setting the user just changed stands: the pop-up helps correct the *other* setting
     (choosing QSMART with HEIDI on offers turning HEIDI off, not undoing QSMART).
   - The CLI runs exactly the settings given, reports differences from the reference in the log
     and methods Note, applies them only with `--accept-recommendations` or
     `--accept-reference-settings`, and refuses a `Required` rule at plan time with a message
     naming the setting.
4. **One description of each setting.** The settings registry
   (`crates/qsmxt-engine/src/schema/table/<section>.rs`) gives each setting's label, help,
   bounds, level, place in the TUI and `qsmxt run` flag. The run and standalone flags, the TUI
   rows, qsmbly's forms and the docs' settings reference are derived from it; don't restate a
   setting anywhere else. Every option reaches every surface in the same PR (see
   [Change surfaces](#change-surfaces)).
5. **Nothing is skipped silently.** An output the data can't produce is an error before the first
   run starts. An output another setting needs is offered in the TUI's Required-change pop-up;
   a run not given it turns it on and logs which setting needs it.
6. **Old installs can still update.** `qsmxt update` must keep working from every release where it
   ever worked: keep the asset names (`qsmxt-<tag>-<target>.tar.gz`/`.zip`) and archive layout.
   `tests/release_asset_contract.rs` guards this.
7. **Settings files are read as written.** Don't add a migration or schema bump just to carry a
   new default; the release notes mention it. Where `migrate.rs` does change what it reads
   (renames, removals), each change is a note or warning.
8. **Toolbox pipelines replicate a reference toolbox** (parity checked on QSM-CI). They are
   defined in QSM.rs (`pipeline::reference_pipelines`) and applied by `pipelines.rs`; they are
   not QSMxT recommendations.

## Conventions

- **Layout.** The engine builds for wasm32 with and without `onnx` (a CI job checks both).
  Standalone subcommands call the same qsm-core dispatchers as `qsmxt run` and take its flag
  names (flag sets by config-path prefix, `flag_set!` in `src/cli/`).
- **Size budgets**, enforced in CI (`.github/scripts/size_budget.py --enforce`): a file's
  non-test code 800 lines, a test file 1,500, a function 120. Split a file or function that
  would go over; a ceiling in `.github/size-budget.json` (empty today) is added only in its own
  commit, with the reason in the commit message.
- **Names.** Config keys are snake_case by section (`bg_removal.vsharp.radius_step`); run flags
  are kebab-case with the method prefix (`--vsharp-radius-step`). Units go in names (`_mm`, `_s`,
  `_hz`); when a unit or meaning changes, rename the setting.
- **Errors** are complete sentences a front end shows as they are, naming the setting and what to
  do. Plan-time problems are `requirements::Problem` values. No panics on user input.
- **Formatting.** The code is not rustfmt-clean: don't reformat code you are not changing.
  Clippy with `-D warnings` is a CI gate.
- **Comments** say why, not what, and name the source of a reference value (toolbox, file,
  function). No history ("used to", "since v…") unless the code still handles that case.
  Algorithm details are documented in QSM.rs, not here.

### User-facing text

TUI labels and help, CLI help, logs, docs pages and the generated methods text:

- Sentence case; names keep their own case (ROMEO, MEDI, V-SHARP, R2\*, χ).
- Say plainly what was done or what to do; no defensive explanations.
- Don't mention previous QSMxT versions. Name other toolboxes (e.g. STI Suite) or reference
  pipelines (e.g. UK Biobank) only where relevant, not in passing. Rule sources shown to users
  cite only public material.
- Methods text states what was done ("we did x", never "rather than y"), phrases settings as
  settings ("with deconvolution disabled"), gives parameter values only where they differ from
  QSMxT's defaults, and gives useful facts about a tool rather than explaining what it is.

### Tests

Tests check wiring, not algorithms; QSM.rs's CI guarantees algorithm correctness.

- Test the plan, `--dry`, the step graph, cache keys, config → params mapping, output names,
  sidecars and methods text. None of these needs an algorithm to run.
- Never assert algorithm numerics here; move such a check to QSM.rs if it isn't covered there.
- Keep only a handful of end-to-end runs, on tiny synthetic data
  (`crates/qsmxt-io/src/testutils.rs`) with the fastest choices (TKD, simple thresholds). Use
  mocks where data must flow.
- Test names are sentences (`a_skipped_step_differs`). Never delete or weaken what a test checks;
  rewrite it to check the same thing more cheaply.
- Coverage stays at or above the 70% gate. CI should stay well under 5 minutes; fix the slow
  tail (over about 1 s), not only the slowest test.
- **Snapshots** (each crate's `tests/snapshots/`) record help text, flags, command lines,
  methods text, step cache keys, dry runs and TUI forms. A change that shows in one is a
  behaviour change: regenerate with `QSMXT_UPDATE_SNAPSHOTS=1`, in its own commit, and say why
  in the PR. A changed cache key recomputes every user's cached steps.
- **Push early.** Locally, build and run clippy (`-D warnings`) on the crates you changed, and
  run only the new or changed tests, if they take about 2 minutes at most. Don't run the whole
  `qsmxt` test suite locally; CI runs everything and any repeats. After a CI failure, rerun that
  test locally only if it is quick; otherwise push the fix. Regenerate docs and snapshots once,
  at the end. A re-measurement a doc asks for (e.g. memory calibration) does not hold up a PR:
  note it in the body.
- A test too slow to wait for locally (over about 2 minutes) gets
  `#[ignore = "slow: ..."]`: a local `cargo test` skips it and CI runs it with the rest of the
  suite (`--run-ignored all`, see `.github/scripts/nextest_filter.py`). Other `#[ignore]`d tests
  (fixtures, tools, tests needing model weights) stay out of the main CI run.

## Change surfaces

For every option added, renamed, removed or re-defaulted, check in the same PR (paths under
`crates/qsmxt-engine/src/` unless given):

- [ ] config struct (`config/<section>.rs`), and `enums.rs` for a new choice
- [ ] registry entry (`schema/table/<section>.rs`: label, help, bounds, `.of(..)`, `.flag(..)`)
      and `schema/regular.rs` (basic or advanced). The run and standalone flags and the TUI row
      follow from these.
- [ ] `bridge/` (each setting passed explicitly), `methods/`, `cache.rs` and step keys, the
      stage's provenance `params()` (`stages/<stage>.rs`), `requirements/`, `memory.rs`
- [ ] TUI only where a setting is not a plain registry row
      (`crates/qsmxt-tui/src/app/field_paths.rs`), with `schema_parity` passing
- [ ] coverage tests: `param_coverage`, `schema_coverage`, `config_path_literals`, and
      `catalogue_coverage` for a new method
- [ ] snapshots and generated docs regenerated (see [Documentation](#documentation)); the
      hand-written prose in `docs/` that names the setting
- [ ] qsmbly: anything beyond the schema, listed under "For qsmbly" in the PR body

Grep for every changed name and old default before opening the PR.

## Pull requests

- Never push to `main`. Branch from current `origin/main` (`feat/`, `fix/`, `docs/`, `ci/`,
  `perf/`, `chore/`), with one worktree and target directory per branch.
- **Title:** a plain sentence saying what changes; no `feat:` prefixes.
- **Body:** `Fixes #N`; what changed; behaviour users see, especially **at default settings**;
  the change-surfaces checklist; decisions taken and why; "For qsmbly" and follow-ups; checks run.
- Open as a draft. A PR is ready only when CI is green. Once it is marked ready, put further
  changes in a new PR unless the reviewer agrees.
- **Stacked PRs:** retarget children to `main` before the parent is merged.
- **qsm-core pin:** one pin in the root `Cargo.toml`'s `[workspace.dependencies]`, a QSM.rs
  `main` `rev` between releases and a `tag` for releases; `tests/qsm_core_pin.rs` checks it. A
  temporary pin to an unmerged QSM.rs PR must be moved to QSM.rs `main` before the QSMxT PR is
  ready.
- The maintainer merges, by squash.
- No `CHANGELOG.md` or release-notes files. The PR body is the record; release notes are drafted
  from it and published on the GitHub release.

## Releases

- Versions stay `0.0.0` (`[workspace.package]`); `release.yml` sets the version from the tag,
  builds every target and publishes `SHA256SUMS`.
- Releases are cut only with the maintainer's go, with QSM.rs pinned to a tag and parity checked
  on QSM-CI for whatever computation changed. Prefer one complete release over a quick
  follow-up or patch release.

## Documentation

- The site is Astro Starlight under `docs/`. `src/cli_reference.rs` fails when a page names a
  flag, command or algorithm that doesn't exist.
- Generated, never edited by hand: `reference/cli.md`, and the blocks between
  `<!-- generated:NAME start … -->` and `<!-- generated:NAME end -->` in `configuration.md`
  (from the registry), `algorithms.md` and `README.md` (method tables from QSM.rs's catalogue). Regenerate with
  `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference` and
  `QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs`.
- `README.md` covers what QSMxT is, install, quick start, the method tables and pointers to the
  site. No counts ("50+ algorithms").
