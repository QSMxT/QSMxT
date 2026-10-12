# Changelog

Notes for the next release, collected as pull requests land. Each entry says whether default
output changes, so the release notes can lead with what makes results incomparable across the
upgrade.

## Unreleased

### Output changes with default settings

These come from QSM.rs (qsm-core is pinned by rev to QSM.rs main `440946b`):

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

### Changed

- `--qsmart-ilsqr-tol` is removed: QSMART's inner iLSQR uses `--ilsqr-tol`. New
  `--qsmart-ilsqr-pad-mm` (default 6 mm, STI Suite's) sets its zero-padding; the TUI has it as
  "iLSQR Pad (mm)" and shows the iLSQR tolerance under QSMART. A config file's
  `[inversion.qsmart] ilsqr_tol` is ignored.
- `--b0-weight-type assumed-decay` reaches qsm-core as the real assumed-decay weighting rather
  than TE weighting, so qsmbly and any other qsmxt-config consumer get the same field map as QSMxT.
- The FFT Laplacian solver (`--laplacian-solver fft`) is in every build; the `laplacian-fft`
  cargo feature is gone.
