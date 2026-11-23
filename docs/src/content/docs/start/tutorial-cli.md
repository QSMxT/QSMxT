---
title: "Tutorial: from the command line"
description: "Convert DICOMs and run the pipeline from the command line."
---

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
See [DICOM → BIDS](/QSMxT/how-to/convert-dicom/#converting-dicoms-to-bids).

(Already have a BIDS dataset? Skip to step 2.)

### 2. Run the pipeline

```sh
qsmxt run study/bids
```

QSMxT discovers every phase/magnitude run in the dataset and reconstructs a QSM
map for each, writing results to `study/bids/derivatives/qsmxt/`. With no
configuration it uses sensible defaults: ROMEO unwrapping, V-SHARP background
removal, and iLSQR dipole inversion. Pick algorithms inline or process a subset:

```sh
qsmxt run study/bids --qsm-algorithm rts --include "sub-01*"
```

See [Running noninteractively](/QSMxT/how-to/run-bids/).

## What you get

Outputs land under `derivatives/qsmxt/` as BIDS-compliant NIfTIs — one QSM map
per run, plus any [supplementary outputs](/QSMxT/reference/tools/) you enabled
(SWI, T2\*, R2\*, RSS-combined magnitude). A `methods.md` describes what ran and
lists citations for the exact methods your data and settings used.

## Next steps

- [Running interactively](/QSMxT/background/tui/) — the TUI workflow
- [Running noninteractively](/QSMxT/how-to/run-bids/) — every stage and option
- [Configuration](/QSMxT/reference/configuration/) — save settings to a TOML file
- [Input data](/QSMxT/reference/inputs/) — what your dataset needs to contain
- [Algorithms](/QSMxT/reference/methods/) — choose the right methods
