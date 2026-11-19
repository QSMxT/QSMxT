# Contributing to QSMxT

QSMxT is a BIDS-native QSM pipeline: the `qsmxt` command line and TUI, and `qsmxt-engine`, the
I/O-free crate with the configuration, planning, run logic, settings schema, command line and
methods text. Usage documentation is on the [documentation site](https://qsmxt.github.io/QSMxT/).

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
   recommended values are QSM.rs rules, not hidden defaults.
3. **Never change a setting silently.** Choosing one setting must not rewrite another. Don't add
   "this default depends on another setting" logic: express it as a QSM.rs rule.
   - The TUI offers rule findings in the advice pop-up, a checklist of changes. A recommendation
     is preselected; an original authors' setting preselects **Keep mine**; nothing is
     preselected when equally strong recommendations disagree.
   - The setting the user just changed stands: the pop-up helps correct the *other* setting
     (choosing QSMART with HEIDI on offers turning HEIDI off, not undoing QSMART).
   - The CLI runs exactly the settings given, reports differences from the reference in the log
     and methods Note, and refuses a hard requirement at plan time with a message naming the
     setting.
4. **Every option reaches every surface** in the same PR (see [Change surfaces](#change-surfaces)).
5. **Nothing is skipped silently.** An output the data can't produce is an error before the first
   run starts; one turned on by another option is a warning naming that option.
6. **Old installs can still update.** `qsmxt update` must keep working from every release where it
   ever worked: keep the asset names (`qsmxt-<tag>-<target>.tar.gz`/`.zip`) and archive layout.
   `tests/release_asset_contract.rs` guards this.
7. **Settings files are read as written.** Don't add a migration or schema bump just to carry a
   new default; the release notes mention it. Where `migrate.rs` does change what it reads
   (renames, removals), each change is a note or warning.
8. **Toolbox pipelines replicate a reference toolbox** (parity checked on QSM-CI). They are not
   QSMxT recommendations.

## Conventions

- **Layout.** The engine (`crates/qsmxt-engine/src`) has no file I/O and builds for wasm32, with
  and without `onnx`. The binary (`src/`) holds `cli.rs`, `commands/` (one file per
  subcommand), `tui/`, BIDS/DICOM/NIfTI I/O and the executors. Standalone subcommands call the
  same qsm-core dispatchers as `qsmxt run`.
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
- Keep only a handful of end-to-end runs, on tiny synthetic data (`src/testutils.rs`) with the
  fastest choices (TKD, simple thresholds). Use mocks where data must flow.
- Test names are sentences (`a_skipped_step_differs`). Never delete or weaken what a test checks;
  rewrite it to check the same thing more cheaply.
- Coverage stays at or above the 70% gate. CI should stay well under 5 minutes; fix the slow
  tail (over about 1 s), not only the slowest test.
- Locally, build, run clippy and run only the new or changed tests (quick ones). Push early and
  let CI run the full suite and any repeats.
- A test too slow to wait for locally (over about 2 minutes) gets
  `#[ignore = "slow: ..."]`: a local `cargo test` skips it and CI runs it with the rest of the
  suite (`--run-ignored all`, see `.github/scripts/nextest_filter.py`). Other `#[ignore]`d tests
  (fixtures, tools, tests needing model weights) stay out of the main CI run.

## Change surfaces

For every option added, renamed, removed or re-defaulted, check in the same PR:

- [ ] `qsmxt run` flag (`src/cli.rs`) and the standalone subcommand (`src/commands/*.rs`)
- [ ] TUI (`src/tui/`, with `schema_parity` passing)
- [ ] engine `config.rs`, `bridge.rs` (each setting passed explicitly), `schema/table.rs` (label,
      help, bounds, visibility) and `schema/regular.rs` (basic or advanced), `command.rs`, `methods.rs`, cache/step keys,
      `requirements.rs`, `memory.rs`
- [ ] docs `docs/src/content/docs/reference/*.md`, and `cli.md` regenerated with
      `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference`
- [ ] coverage tests: `param_coverage`, `schema_coverage`, `run_args_coverage`
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
- **qsm-core pin:** both `Cargo.toml` files pin the same QSM.rs `rev` (a `tag` for releases);
  `tests/qsm_core_pin.rs` checks it. A temporary pin to an unmerged QSM.rs PR must be moved to
  QSM.rs `main` before the QSMxT PR is ready.
- The maintainer merges, by squash.
- No `CHANGELOG.md` or release-notes files. The PR body is the record; release notes are drafted
  from it and published on the GitHub release.

## Releases

- Both `Cargo.toml` versions stay `0.0.0`; `release.yml` sets the version from the tag, builds
  every target and publishes `SHA256SUMS`.
- Releases are cut only with the maintainer's go, with QSM.rs pinned to a tag and parity checked
  on QSM-CI for whatever computation changed. Prefer one complete release over a quick
  follow-up or patch release.

## Documentation

- The site is Astro Starlight under `docs/`. `reference/cli.md` is generated; never edit it by
  hand. `src/cli_reference.rs` fails when a page names a flag, command or algorithm that doesn't
  exist.
- `README.md` covers what QSMxT is, install, quick start and pointers to the site.
