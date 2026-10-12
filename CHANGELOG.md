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

### Changed

- `--qsmart-ilsqr-tol` is removed: QSMART's inner iLSQR uses `--ilsqr-tol`. New
  `--qsmart-ilsqr-pad-mm` (default 6 mm, STI Suite's) sets its zero-padding; the TUI has it as
  "iLSQR Pad (mm)" and shows the iLSQR tolerance under QSMART. A config file's
  `[inversion.qsmart] ilsqr_tol` is ignored.
- `--b0-weight-type assumed-decay` reaches qsm-core as the real assumed-decay weighting rather
  than TE weighting, so qsmbly and any other qsmxt-config consumer get the same field map as QSMxT.
- The FFT Laplacian solver (`--laplacian-solver fft`) is in every build; the `laplacian-fft`
  cargo feature is gone.
