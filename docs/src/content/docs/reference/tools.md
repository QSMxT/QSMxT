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
| `robust` | Robust threshold (Otsu + dilate + fill-holes + erode) |
| `erode` / `dilate` | Morphological erosion / dilation |
| `close` | Morphological closing |
| `fill-holes` | Fill holes in a binary mask |
| `smooth` | Gaussian smooth (re-thresholded at 0.5) |

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

```sh
# e.g. inspect the parameters of a method
qsmxt invert tgv --help
```

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
| `resample` | Resample an oblique volume to axial orientation |

:::note
These tools share the same implementations the [pipeline](/QSMxT/guides/running-noninteractively/)
uses, so a step run standalone produces the same result as the corresponding
stage of `qsmxt run`.
:::
