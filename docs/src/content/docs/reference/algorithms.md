---
title: Algorithms
description: The masking, unwrapping, background-field-removal, and dipole-inversion methods available in QSMxT.
---

Every stage of the pipeline is configurable. The numerical methods are provided
by [QSM.rs](https://github.com/astewartau/QSM.rs); QSMxT orchestrates them over
your BIDS data. Below are the valid values for each algorithm option.

## Masking

Choose a masking recipe with `--mask-preset`, and override the image it
operates on with `--masking-input`.

| Value | Method |
| --- | --- |
| `robust-threshold` | Otsu thresholding of the phase-quality map, refined with dilation, hole-filling and erosion (default) |
| `bet` | Brain Extraction Tool on the magnitude image |

**Masking input** (`--masking-input`): `magnitude-first`, `magnitude`,
`magnitude-last`, `phase-quality`. For example,
`--mask-preset robust-threshold --masking-input magnitude` thresholds the
combined magnitude image instead of the phase-quality map.

For full control, compose custom mask sections with `--mask` (repeatable),
e.g. `--mask magnitude,bet:0.5,erode:2`. Each section names its input
followed by a generator and refinement operations.

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

Set with `--qsm-algorithm`. Two families are available: classical/iterative
reconstructions and deep-learning networks.

### Classical & iterative

| Value | Method |
| --- | --- |
| `rts` | RTS — Rapid Two-Step (default) |
| `tv` | Total Variation (TV-ADMM) |
| `tkd` | TKD — Thresholded K-space Division |
| `tsvd` | TSVD — Truncated Singular Value Decomposition |
| `tgv` | TGV — Total Generalized Variation |
| `tikhonov` | Tikhonov regularization |
| `nltv` | NLTV — Nonlinear Total Variation |
| `medi` | MEDI — Morphology-Enabled Dipole Inversion |
| `tfi` | TFI — Total Field Inversion |
| `ilsqr` | iLSQR |
| `qsmart` | QSMART two-stage reconstruction |
| `ndi` | NDI — Nonlinear Dipole Inversion |
| `fansi` | FANSI — Nonlinear TV |
| `fansi-tgv` | FANSI — Nonlinear TGV |
| `l1qsm` | L1-QSM — L1 data fidelity |
| `whqsm` | WH-QSM — Weak-Harmonic |
| `hdqsm` | HD-QSM — Hybrid data fidelity |
| `amp-pe` | AMP-PE — Approximate Message Passing with Parameter Estimation |

### Deep learning

| Value | Method | Tileable |
| --- | --- | --- |
| `xqsm` | xQSM | ✓ |
| `qsmnet` | QSMnet | ✓ |
| `qsmnet-plus` | QSMnet+ | ✓ |
| `ir2qsm` | IR2QSM — unrolled | ✓ |
| `lpcnn` | LPCNN — learned-proximal | ✓ |
| `modl-qsm` | MoDL-QSM — model-based | ✓ |
| `nextqsm` | NeXtQSM — single-step | ✓ |
| `autoqsm` | AutoQSM — single-step (native patching) | |
| `qsmgan` | QSMGAN — GAN-refined (native patching) | |
| `iqsm` | iQSM — end-to-end from phase | |
| `iqsm-plus` | iQSM+ — orientation-adaptive end-to-end | |

Model weights are downloaded automatically on first use (and cached) from the
QSMxT weight registry on [Hugging Face](https://huggingface.co/qsmxt/qsm-onnx-weights).
Deep-learning support requires a build with the default `dl` feature; `iqsm`
and `iqsm-plus` reconstruct susceptibility directly from phase, so they replace
the background-removal + inversion stages rather than running after them.

:::tip
Not sure which to pick? The defaults (`threshold` → `romeo` → `vsharp` → `rts`)
are a robust, fast starting point for human brain GRE data. Use the
[TUI](/QSMxT/guides/running-interactively/) to experiment interactively.
:::

### Overlap-tiling for deep-learning inversion

Deep-learning inversions can run **overlap-tiled** to bound peak memory: the
volume is split into cubic patches (each a core plus a context halo), inferred
independently, and stitched back together. This is opt-in and applies to the
tileable networks above.

| Flag | Meaning |
| --- | --- |
| `--tile-size <N>` | Output core size per patch, in voxels — **presence enables tiling** |
| `--tile-halo <N>` | Context margin per side, in voxels (default 8) |

```bash
qsmxt run bids/ output/ --qsm-algorithm xqsm --tile-size 128 --tile-halo 8
```

Tiling is an approximation of the whole-volume network — results are close but
not bit-identical. Omit `--tile-size` to run the network over the whole volume.
The flags are ignored by classical algorithms and by the natively-patched
networks (`autoqsm`, `qsmgan`).

## Per-algorithm parameters

Each algorithm exposes its own parameters (regularisation weights, kernel sizes,
iteration counts, …). The easiest way to discover them — and to build a run
command — is the [TUI](/QSMxT/guides/running-interactively/): it exposes every
option with sensible defaults and shows the equivalent `qsmxt run` command live as
you change them. You can also set them in a
[configuration file](/QSMxT/reference/configuration/), or run the matching
[standalone tool](/QSMxT/reference/tools/) (e.g. `qsmxt invert tgv --help`) to
experiment directly.
