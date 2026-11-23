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
See [Convert DICOM to BIDS](/QSMxT/how-to/convert-dicom/).

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

See [Run QSM](/QSMxT/how-to/run-qsm/) and [Look at your results](/QSMxT/how-to/results/).

## What you get

Outputs land under `derivatives/qsmxt/` as BIDS-compliant NIfTIs — one QSM map
per run, plus any [supplementary outputs](/QSMxT/reference/tools/) you enabled
(SWI, T2\*, R2\*, RSS-combined magnitude). A `methods.md` describes what ran and
lists citations for the exact methods your data and settings used.

## Next steps

- [Look at your results](/QSMxT/how-to/results/): where the maps are, and what to check
- [Cite QSMxT and write the methods](/QSMxT/how-to/cite/)
- [Your data](/QSMxT/data/): what QSMxT does for GE, single-echo, 2D, 7 T, rodent and other data
- [Settings](/QSMxT/reference/configuration/): every setting, and saving them to a TOML file
