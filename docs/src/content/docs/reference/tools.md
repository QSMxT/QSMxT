---
title: Standalone tools
description: Run individual QSM building blocks directly on NIfTI files, outside the BIDS pipeline.
---

Beyond the end-to-end pipeline, QSMxT exposes each building block as a standalone
command that reads and writes NIfTI files. These are handy for experimentation,
teaching, scripting, or slotting a single step into another workflow.

Every command takes NIfTI input and produces NIfTI output. Run
`qsmxt <command> --help` (and `qsmxt <command> <subcommand> --help`) for exact
arguments.

## Masking — `qsmxt mask`

Generate or refine binary masks.

| Subcommand | Operation |
| --- | --- |
| `otsu` | Otsu automatic thresholding |
| `value` | Fixed-value thresholding |
| `percentile` | Percentile thresholding |
| `bet` | Brain extraction (BET) |
| `hd-bet` | Deep-learning brain extraction (HD-BET); `--low-memory`, `--patch XxYxZ`, `--tta`. Weights are downloaded on first use |
| `preset` | Run a [`--mask-preset`](/QSMxT/reference/algorithms/#masking) recipe on an image: `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase` |
| `robust` | Alias for `preset robust-threshold` |
| `erode` / `dilate` | Morphological erosion / dilation |
| `close` | Morphological closing |
| `fill-holes` | Fill holes in a binary mask (`--max-size 0`, the default, is automatic: 5% of the volume) |
| `smooth` | Gaussian smooth (re-thresholded at 0.5) |
| `and` / `or` | Intersect / union two or more masks on the same grid |

Every subcommand is the first link of a chain: it does its own operation, then
applies any `--op` refinements in order — `erode:2`, `fill-holes:0`, `close:1`,
`gaussian:4.0`, `signal-erode`, the same spellings as a `--mask` section. So
`qsmxt mask erode m.nii -o out.nii --op fill-holes` erodes, then fills. What each
op does is defined once, in the same code the pipeline runs, so a mask built
here matches the one a `--mask` section would produce.

`and` and `or` are the standalone form of the pipeline's
[`--mask-combine`](/QSMxT/reference/algorithms/#combining-sections), for trying
combinations on masks you already have without a pipeline run. Their `--op`
refinements run on the *combined* mask, like `--mask-refine` does. The
consensus recipe by hand, step by step:

```sh
qsmxt quality-map phase_e1.nii -o quality.nii --magnitude mag_e1.nii
qsmxt mask bet mag_e1.nii -o bet.nii
qsmxt mask otsu quality.nii -o phase.nii
qsmxt mask and bet.nii phase.nii -o brain.nii --op fill-holes:0 --op erode:1
```

— or in one go, since it is a preset:

```sh
qsmxt mask preset bet-and-phase mag_e1.nii --quality quality.nii -o brain.nii
```

`preset` reads its input image for every section; sections that read phase
quality use `--quality` when given, and the input image otherwise, as
`run --masking-input` would. `--op signal-erode` needs a magnitude image: for
`and`/`or`, whose inputs are masks, pass it as `--magnitude`.

## Coil combination — `qsmxt combine`

Combine uncombined receive-coil channels into one phase and magnitude per echo
with MCPC-3D-S (`mcpc3ds`). Inputs are the per-coil, per-echo 3D NIfTIs; files
named with BIDS `coil-NN` / `echo-N` entities are ordered by those, otherwise pass
them coil-major with `--num-echoes`. Echo times come from `--tes` or the phase
sidecars.

```sh
qsmxt combine mcpc3ds \
  --phase sub-1/anat/*rec-uncombined*part-phase_MEGRE.nii.gz \
  --magnitude sub-1/anat/*rec-uncombined*part-mag_MEGRE.nii.gz \
  -o combined/sub-1_rec-mcpc3ds
# writes combined/sub-1_rec-mcpc3ds_echo-N_part-{phase,mag}.nii and _desc-mcpc3ds_mask.nii
```

## Phase unwrapping — `qsmxt unwrap`

Unwrap a wrapped phase image (`romeo` or `laplacian`).

## Background field removal — `qsmxt bgremove`

Remove background fields from an unwrapped field map (`vsharp`, `pdf`, `lbv`,
`ismv`, `sharp`, `resharp`, `harperella`, `iharperella`).

## Dipole inversion — `qsmxt invert`

Invert a local field map into a susceptibility map. One subcommand per method:

| Subcommand | Method |
| --- | --- |
| `rts` | Regularized Total Strength |
| `tv` | Total Variation (ADMM) |
| `tkd` | Truncated K-space Division |
| `tsvd` | Truncated SVD |
| `tgv` | Total Generalized Variation |
| `tikhonov` | Tikhonov regularization |
| `nltv` | Nonlocal Total Variation |
| `medi` | Morphology-Enabled Dipole Inversion |
| `ilsqr` | Iterative Least-Squares QR |
| `lsqr` | Minimally regularised LSQR |
| `heidi` | HEIDI — Homogeneity Enabled Incremental Dipole Inversion |
| `cosmos` | COSMOS — multi-orientation least squares (2+ orientations) |
| `sti` | Susceptibility tensor imaging (6+ orientations) |

```sh
# e.g. inspect the parameters of a method
qsmxt invert tgv --help
```

`cosmos` and `sti` take several `--input` field maps plus one `--b0-direction` per
orientation, and expect them already co-registered onto a common grid. See
[multi-orientation](/QSMxT/reference/algorithms/#multi-orientation-cosmos-and-sti).

## Supplementary maps

| Command | Output |
| --- | --- |
| `swi` | Susceptibility-weighted image (and minimum-intensity projection) |
| `r2star` | R2\* map from multi-echo magnitude |
| `t2star` | T2\* map from multi-echo magnitude |
| `quality-map` | ROMEO phase-quality map |

## Image utilities

| Command | Purpose |
| --- | --- |
| `homogeneity` | Correct intensity inhomogeneity on magnitude data |
| `resample` | Resample an oblique volume to axial orientation. Continuous data (magnitude, an unwrapped field, χ) as a positional argument; **wrapped phase needs `--phase` with `--magnitude`** so the interpolation goes through the complex domain |

```sh
# continuous data
qsmxt resample mag.nii -o mag_axial.nii

# wrapped phase, with its magnitude, interpolated as mag·e^{iφ}
qsmxt resample --phase phase.nii --magnitude mag.nii \
  --phase-out phase_axial.nii --magnitude-out mag_axial.nii
```

:::note
These tools share the same implementations the [pipeline](/QSMxT/guides/running-noninteractively/)
uses, so a step run standalone produces the same result as the corresponding
stage of `qsmxt run`.
:::
