---
title: "Tutorial: first QSM map in the TUI"
description: "From a DICOM or BIDS dataset to a susceptibility map with the interactive interface."
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
See [Running interactively](/QSMxT/background/tui/).

## No data yet? Fetch an example

`qsmxt example` downloads a real in-vivo subject and writes it out as a BIDS
dataset, ready to run:

```sh
qsmxt example study/bids
qsmxt run study/bids
```

That fetches `prisma-bridge-run1` (~103 MB): one subject, 5-echo 1 mm 3D
gradient-echo, acquired on a Siemens MAGNETOM Prisma Fit. The archive is cached
under `~/.cache/qsmxt/examples` (override with `$QSMXT_EXAMPLE_CACHE`), so
fetching it again into another directory costs nothing.

If OSF is unreachable from your network, set `$QSMXT_EXAMPLE_BASE_URL` to a mirror
holding the archives as `<id>.zip` — checksums are still enforced, so a mirror
serving the wrong bytes is rejected exactly as a corrupted download would be.

The data comes from a QSM harmonization acquisition in which the same subject was
scanned on two Siemens 3T scanners under four protocols, three runs each. List
everything available with:

```sh
qsmxt example --list
```

Because every acquisition is the same subject, several can share one dataset —
they differ only in their BIDS `ses-` (scanner), `acq-` (protocol) and `run-`
entities. Name more than one, or point a later call at an existing dataset to add
to it:

```sh
qsmxt example --name prisma-bridge-run1 --name cima-bridge-run1 study/bids
qsmxt example --name prisma-bridge-run2 study/bids   # adds a third to the same dataset
```

Acquisitions already in the dataset are left alone; pass `--force` to rewrite
them. Everything here is also available in the TUI under **Input mode → Example
dataset**.

:::note
`MagneticFieldStrength` in the sidecars is the accurate field derived from the
scanner's carrier frequency (about 2.89 T), not the nominal 3.0 T. QSM scales ppm
by this value, so the accurate one is the correct one. The sidecar records the
`ImagingFrequency` it came from.
:::
