# Changelog

Notes for the next release, collected as pull requests land. Each entry says whether default
output changes, so the release notes can lead with what makes results incomparable across the
upgrade.

## Unreleased

### Output changes with default settings

- **V-SHARP no longer deconvolves by default** (`--vsharp-deconvolve false`, QSM.rs#160). The
  default background removal is now V-SHARP's SMV high-pass with the largest kernel that fits
  at each voxel (12 mm down to 1 mm), as STI Suite and the UK Biobank pipeline run it, instead
  of following it with a truncated-SVD deconvolution. On UK Biobank this raised longitudinal
  sensitivity in iron-rich structures without a significant loss of test-retest reliability;
  the deconvolution is exact only for noise-free backgrounds. Every default-configuration χ map
  changes. `--vsharp-deconvolve true` (TUI: V-SHARP → Deconvolve) reproduces v9.22.0, and the
  methods text says which ran.

- **Multi-echo TGV χ had the wrong scale** (fixed; no issue was filed). `stage_tgv` passed the
  combined B0 field map, which is in ppm, to a TGV solver that expected phase in rad at TE1, so
  multi-echo `--qsm-algorithm tgv` χ was too small by 2π·γ·B0·TE1 (about 4× at 3 T with TE1 = 5
  ms, and a different factor for every TE1/B0). TGV now gets the field in ppm for multi-echo data
  (`TgvInput::FieldPpm`, with a plain Laplacian instead of the sin/cos one) and the wrapped echo-1
  phase with TE1 and B0 for single-echo data (`TgvInput::WrappedPhase`), QSM.rs#170. A
  regression test checks the multi-echo and single-echo TGV χ scale against a phantom and RTS.
- **TGV α are in ppm** (QSM.rs#170), so one α means the same at every TE and field strength.
  The defaults are qsm-core's: α0 = 2.002e-4, α1 = 6.675e-5 (the old 0.0015 / 0.0005 rad
  defaults at a 7 T / 4 ms reference, where they were tuned; QSM.rs#175). That is 2.14× stronger
  than a 3 T / 20 ms reference would give, which a sweep favoured on the CI phantom and on UK
  Biobank. `--tgv-alpha0/1` and `[inversion.tgv] alpha0/alpha1` in a config file are read as
  ppm: divide an old rad value by 2π·42.5781·B0·TE. Single-echo TGV at the defaults changes
  wherever TE1/B0 is not 4 ms / 7 T. The methods text states the α and
  units. `qsmxt invert tgv` takes a field in ppm like the other `invert` algorithms and no longer
  has `--echo-time` / `--field-strength`.

These come from QSM.rs (qsm-core is pinned by rev to QSM.rs main `c78dfc9`):

- **iLSQR is now STI Suite 3.0's algorithm** (QSM.rs#163, #165). The streaking-stage tolerance
  defaults to 1e-3 (was 0.01), `max_iter` caps both LSQR solves (default 100), the pipeline pads
  by 64 mm, and the solve is always double precision. χ from `--qsm-algorithm ilsqr` differs from
  v9.22.0, and small fields of view are slower. QSMART's inner iLSQR changes with it.
- **χ-separation MEDI** (`--separation-algorithm chi-sep-medi`): D_r goes from 114/30 to 137/137
  Hz/ppm (QSM.rs#138), the value the other three methods already used.
- **R2′ is zero where R2 ≤ 0** (QSM.rs#138). R2 = 0 means nothing was measured there.
- **ROMEO without phase offset removal** (`--phase-offset-removal false`): the B0 map now honours
  `--b0-estimation` and `--b0-weight-type` (QSM.rs#162). It used to be a linear fit whatever was
  selected, so with the default (weighted average, phase-SNR) the field map changes.
- **Every NIfTI output carries a qform** (qform_code 1, pixdim[0] = qfac) as well as the sform
  (QSM.rs#133). qform-only inputs are read with their qform.
- **Oblique, anisotropic acquisitions** pick their axial working grid differently (QSM.rs#132,
  `axial_grid_for`/`ras_axes`), which can change results under `--obliquity-threshold` resampling.

### Added

- **Motion-robust echo combination** (QSM.rs#141/#166), off by default: `--b0-estimation
  robust-fit`, with `--robust-fit-loss`, `--robust-fit-tuning`, `--robust-fit-iterations`,
  `--robust-fit-downweight-level`, `--robust-fit-flag-ratio` and the shared
  `--linear-fit-estimate-offset` / `--linear-fit-reliability-threshold`. All are in the TUI
  under B0 Estimation = robust-fit; the per-echo report is written next to the field map
  (`*_robustfit.json`) and flagged echoes are logged.
- The TUI now has the fits' reliability-threshold percentile, and its "Estimate Offset" toggle
  reaches the run (it used to reach only the generated command). `generate_command` now emits
  `--linear-fit-estimate-offset` / `--linear-fit-reliability-threshold` when they differ.
- **Slice-gap check** (QSM.rs#139), on by default. A 2D acquisition whose sidecar
  `SliceThickness` is thinner than the slice pitch is refused for QSM before any processing, since
  the FFT kernels assume contiguous slices; `--ignore-slice-gap` (TUI: General → Ignore Slice Gap)
  reconstructs anyway. **Behaviour change:** such runs used to reconstruct silently with a wrong
  map; non-QSM-only runs continue with a warning.
- **Retrospective motion correction** (QSM.rs#137), off by default: `--motion-correction` rigidly
  co-registers the echoes onto a reference echo before field mapping, with `--motion-reference`,
  `--no-motion-chain`, `--motion-chain-levels`, `--motion-min-ncc` and the `--registration-*`
  group. All are in the TUI (Field mapping → Motion Correction). Needs magnitude (an error
  without). Writes `<run>_motion.json`; the mask is restricted to voxels every echo covers.
- **Multi-orientation co-registration** (QSM.rs#134; supersedes draft #244). An
  `--orientation-group` whose members disagree on grid/affine, or whose only B0 directions come
  from indistinguishable affines, is now rigidly co-registered on magnitude (first member is the
  reference) and its B0 directions are recovered from the rotations, instead of being refused.
  Groups already on one grid with declared `B0_dir`s are untouched. Registration with NCC <= 0.5
  is refused; 0.5-0.8 warns. `--no-orientation-registration` restores the refusal;
  `--multi-orientation-force` and the shared `--registration-*` settings apply. All are in the
  TUI (General: Orientation Co-registration, Force Multi-orientation, Rigid registration rows).
  **Behaviour change:** such groups used to be refused.

- **R2PRIMEnet field-strength policy** (QSM.rs#138). R2PRIMEnet is trained at 3 T only. Away
  from 3 T, `--r2prime-strategy auto` now skips R2′ with a warning when there is no R2 to
  subtract, and an explicit `--r2prime-strategy r2primenet` stops with an error.
  `--r2primenet-ignore-field-mismatch` runs it anyway, with a warning and a methods-text note
  that the R2′ is uncalibrated. `--r2primenet-patch D H W` sets its sliding-window patch. Both
  are in the TUI under "R2' Source". **Output change:** v9.22.0 ran R2PRIMEnet at any field
  strength, so a non-3 T run with no MESE now has no R2′ (and no R2′-based χ-separation) unless
  the override is given.
- **R2′ coverage.** A measured R2′ is written with `_desc-r2prime_mask.nii`, the voxels where it
  is a measurement (brain mask ∩ MESE field of view ∩ R2 > 0), and the log says how much of the
  brain that is. `qsmxt r2prime` gains `--r2-coverage` and `--output-coverage`.

- **TUI/CLI parity.** Every `qsmxt run` flag is now reachable from the TUI. New TUI controls:
  Crop to Mask / Crop Margin / FFT Padding / Output Space / inhomogeneity Sigma and Box Passes /
  Use custom dseg (QSM → General); Coil Comb. Sigma and ROMEO Phase Coherence (Field mapping);
  TGV Step Size / Tolerance; MEDI and TFI MERIT / Data Weighting; the remaining AMP-PE settings;
  Source DICOM Dir / DICOM Outputs (Supplementary, under Export DICOM); Memory Limit / No Memory
  Limit / Force Re-run / Clean Intermediates (Execution, local runs). New flag `--no-medi-smv`
  (MEDI's SMV step is on by default and could not be turned off from the CLI).
- A guard test (`tui::command::parity`) walks every row the QSM and separation tabs can show, and
  the other tabs' controls, and fails if a `run` flag cannot be reached from the TUI, if a TUI
  control has no effect on the run, if a setting the run uses is missing from the generated
  command, or if the generated command does not reproduce the configuration.

### Changed

- **Provenance records the exact QSM.rs commit** (#278; no change to any image). The QSM.rs
  version was "unknown" in BIDS-Prov, the TUI header and `--version` because qsm-core is pinned
  by `rev`, not `tag`. It is now the tag when there is one, else `rev-<full 40-character commit>`
  read from `Cargo.lock`, and the build fails rather than record a git pin without a commit.
  It appears in `dataset_description.json` (a second `GeneratedBy` entry, `QSM.rs`, with a
  `CodeURL` to that commit), in BIDS-Prov (`prov-qsmxt_soft.json`, agent
  `bids::prov/#qsm-rs-<version>`), in every output sidecar (`QSMrsVersion`, beside
  `QSMxTVersion`), in the log, the TUI header and `qsmxt --version`.
- BIDS-Prov covers every step that writes a file: motion correction, segmentation, SMWI,
  R2/R2′, χ-separation, statistics, both passes and the combination of a two-pass run, the
  single-pass map, the return to the acquired grid and coil combination were missing, and the
  sidecars of some outputs named activities that were never written. Each published image's
  `GeneratedBy` is now the activity that last wrote it, read from the records rather than a
  hand-kept list, so it always resolves in `prov/` (for a map returned to the acquired grid,
  that is `output_space`, whose `Used` links back to the step that made the working-grid map).
  R2/R2′, the χ-separation maps and the COSMOS/STI outputs get sidecars; COSMOS/STI get their
  own activity, and provenance is now written after the multi-orientation step. A test keeps
  the list of recorded steps in step with the pipeline.

- The TUI's configuration (methods text, generated command) is now built from the same argument
  list the run uses, through the same overrides as `qsmxt run`, so the three can no longer
  disagree. They did: the generated command and methods text left out HEIDI/LSQR settings, the
  QSMART radii and deep-learning tiling, though the run used them.
- `generate_command` (shared with qsmbly) now emits every ROMEO setting, the QSMART inner
  inversion's own settings, MEDI/TFI MERIT and data weighting, the inhomogeneity-correction
  settings, and the multi-orientation, registration and custom-dseg settings whenever they were
  changed; under QSMART it states the mask when it is the default threshold recipe (QSMART's own
  default is BET).
- Fixed: `--medi-merit` and `--medi-data-weighting` were parsed but never applied.
- Fixed: the separation tab showed HC-χsep's parameters for SUSEP-Net and χ-sepnet, where they
  did nothing.
- `--qsmart-ilsqr-tol` is removed: QSMART's inner iLSQR uses `--ilsqr-tol`. New
  `--qsmart-ilsqr-pad-mm` (default 6 mm, STI Suite's) sets its zero-padding; the TUI has it as
  "iLSQR Pad (mm)" and shows the iLSQR tolerance under QSMART. A config file's
  `[inversion.qsmart] ilsqr_tol` is ignored.
- `--b0-weight-type assumed-decay` reaches qsm-core as the real assumed-decay weighting rather
  than TE weighting, so qsmbly and any other qsmxt-config consumer get the same field map as QSMxT.
- Field mapping is qsm-core's `run_field_mapping` (QSM.rs#161/#162), with no output change: a
  test holds it to QSMxT's previous implementation on every path that implementation computed
  itself, including a single Laplacian-unwrapped echo and the Laplacian linear fit, which
  qsm-core masks and aligns since QSM.rs#167. QSMxT no longer computes any of it itself.
- The FFT Laplacian solver (`--laplacian-solver fft`) is in every build; the `laplacian-fft`
  cargo feature is gone.
- The Laplacian solver and its FFT pad are in the TUI (Field mapping → Unwrapping = laplacian →
  Solver / FFT Pad). `--laplacian-fft-pad` (and `field_mapping.laplacian_fft_pad`) takes one
  value or three (x y z). Default unchanged (64 on every axis).
