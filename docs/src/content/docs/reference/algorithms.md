---
title: Algorithms
description: The masking, unwrapping, background-field-removal, and dipole-inversion methods available in QSMxT.
---

Every stage of the pipeline is configurable. The numerical methods are provided
by [QSM.rs](https://github.com/astewartau/QSM.rs); QSMxT orchestrates them over
your BIDS data. Below are the valid values for each algorithm option.

## Masking

Set with `--masking-algorithm`, and choose what it operates on with
`--masking-input`.

| Value | Method |
| --- | --- |
| `threshold` | Intensity thresholding (default) |
| `bet` | Brain Extraction Tool |

**Masking input** (`--masking-input`): `magnitude-first`, `magnitude`,
`magnitude-last`, `phase-quality`.

## Phase unwrapping

Set with `--unwrapping-algorithm`.

| Value | Method |
| --- | --- |
| `romeo` | Rapid Opensource Minimum-spanning-tree Echo Optimisation (default) |
| `laplacian` | Laplacian-based unwrapping |

## Background field removal

Set with `--bf-algorithm`.

| Value | Method |
| --- | --- |
| `vsharp` | Variable-kernel SHARP (default) |
| `pdf` | Projection onto Dipole Fields |
| `lbv` | Laplacian Boundary Value |
| `ismv` | Iterative Spherical Mean Value |
| `sharp` | Sophisticated Harmonic Artifact Reduction |
| `resharp` | Regularization-enabled SHARP |
| `harperella` | HARPERELLA |
| `iharperella` | Iterative HARPERELLA |

## Dipole inversion

Set with `--qsm-algorithm`. Ten methods are available, spanning fast direct
methods through to regularised iterative reconstructions.

| Value | Method |
| --- | --- |
| `rts` | Regularized Total Strength (default) |
| `tv` | Total Variation (ADMM) |
| `tkd` | Truncated K-space Division |
| `tsvd` | Truncated Singular Value Decomposition |
| `tgv` | Total Generalized Variation |
| `tikhonov` | Tikhonov regularization |
| `nltv` | Nonlocal Total Variation |
| `medi` | Morphology-Enabled Dipole Inversion |
| `ilsqr` | Iterative Least-Squares QR |
| `qsmart` | QSMART two-stage reconstruction |

:::tip
Not sure which to pick? The defaults (`threshold` → `romeo` → `vsharp` → `rts`)
are a robust, fast starting point for human brain GRE data. Use the
[TUI](/QSMxT/guides/running-interactively/) to experiment interactively.
:::

## Per-algorithm parameters

Each algorithm exposes its own parameters (regularisation weights, kernel sizes,
iteration counts, …). The easiest way to discover them — and to build a run
command — is the [TUI](/QSMxT/guides/running-interactively/): it exposes every
option with sensible defaults and shows the equivalent `qsmxt run` command live as
you change them. You can also set them in a
[configuration file](/QSMxT/reference/configuration/), or run the matching
[standalone tool](/QSMxT/reference/tools/) (e.g. `qsmxt invert tgv --help`) to
experiment directly.
