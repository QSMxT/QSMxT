---
title: Quick start
description: From a DICOM or BIDS dataset to quantitative susceptibility maps in minutes.
---

There are two ways to drive QSMxT: **interactively** in the TUI (recommended) or
**noninteractively** on the command line. Both take you from raw data to QSM maps.

## The recommended way: the TUI

Launch the interactive terminal interface and do everything on screen — convert
DICOMs, review the classification, configure the pipeline, and run it:

```sh
qsmxt tui
```

The TUI walks you through each step, with menus for every algorithm and sensible
defaults pre-filled. It's the best way to start, especially with a new dataset.
See [Running interactively](/QSMxT/guides/running-interactively/).

## The command-line way

Prefer to script it? The same workflow in two commands.

### 1. Convert DICOMs to BIDS

```sh
qsmxt dicom-convert /path/to/dicoms study/bids
```

QSMxT scans the directory recursively, classifies each series automatically
(gradient-echo magnitude and phase, echo count, individual coils,
combined/derived reconstructions), and writes a [BIDS](https://bids.neuroimaging.io/)
dataset. Add `--dry-run` to preview the classification without writing anything.
See [DICOM → BIDS](/QSMxT/guides/running-noninteractively/#converting-dicoms-to-bids).

(Already have a BIDS dataset? Skip to step 2.)

### 2. Run the pipeline

```sh
qsmxt run study/bids
```

QSMxT discovers every phase/magnitude run in the dataset and reconstructs a QSM
map for each, writing results to `study/bids/derivatives/qsmxt.rs/`. With no
configuration it uses sensible defaults: ROMEO unwrapping, V-SHARP background
removal, and RTS dipole inversion. Pick algorithms inline or process a subset:

```sh
qsmxt run study/bids --qsm-algorithm rts --include "sub-01*"
```

See [Running noninteractively](/QSMxT/guides/running-noninteractively/).

## No install? Try it in the browser

Want to experiment first without installing anything? **[qsmbly](https://qsmbly.neurodesk.org/)**
runs the same reconstruction algorithms entirely in your browser.

## What you get

Outputs land under `derivatives/qsmxt.rs/` as BIDS-compliant NIfTIs — one QSM map
per run, plus any [supplementary outputs](/QSMxT/reference/tools/) you enabled
(SWI, T2\*, R2\*, RSS-combined magnitude). A `references.txt` lists citations for
the exact methods your data and settings used.

## Next steps

- [Running interactively](/QSMxT/guides/running-interactively/) — the TUI workflow
- [Running noninteractively](/QSMxT/guides/running-noninteractively/) — every stage and option
- [Configuration](/QSMxT/reference/configuration/) — save settings to a TOML file
- [Algorithms](/QSMxT/reference/algorithms/) — choose the right methods
