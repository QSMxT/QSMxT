---
title: Coming from QSMxT 8.x
description: What changed between the Python QSMxT (8.x) and the Rust rewrite (v9) — command changes, and which workflows are no longer included.
---

QSMxT v9 is a complete rewrite in Rust: a single self-contained binary, with no
Python environment to manage. The reconstruction pipeline and DICOM/NIfTI
conversion are fully reimplemented and faster, and there is a new interactive TUI.

## Command changes

| 8.x | v9 |
| --- | --- |
| `qsmxt <bids>` | `qsmxt run <bids>` |
| `dicom-convert` | `qsmxt dicom-convert` |

The easiest way in is `qsmxt tui`, which walks you through conversion,
configuration, and running.

## Multi-coil data

8.x combined uncombined receive coils with MCPC-3D-S inside `dicom-convert`
(writing `_rec-mcpc3ds` files). In v9 the converter keeps the per-coil files
(`_rec-uncombined_coil-NN_*`) and `qsmxt run` combines them as its first stage;
`qsmxt combine mcpc3ds` does the same on loose files. See
[Inputs](/QSMxT/reference/inputs/#uncombined-receive-coils).

## Workflows not in v9

Some parts of the 8.x line are not included in v9:

- anatomical segmentation
- template and group-space building
- group statistics
- NextQSM
- the web UI

If you rely on those, 8.x is still available: see the
[8.x documentation](https://qsmxt.github.io/QSMxT/v8/), the
[`python-legacy` branch](https://github.com/QSMxT/QSMxT/tree/python-legacy) and the
[8.x releases](https://github.com/QSMxT/QSMxT/releases).
