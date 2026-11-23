---
title: Uncombined coil data
description: "Data saved as a separate image for each receive coil."
---

Some protocols save a separate image for every receive-coil element (Siemens "save
uncombined").

## What QSMxT does automatically

- `qsmxt dicom-convert` names each coil's images with a `coil-<NN>` label (and
  `rec-uncombined` when the scanner's combined images are there too), and lists them in
  `.bidsignore` so the BIDS validator accepts the dataset.
- `qsmxt run` combines the coils with MCPC-3D-S before anything else and then carries on as
  usual. The combined echoes are saved as `*_rec-mcpc3ds_echo-<N>_part-{phase,mag}_*`.
- When a scan is there both per coil and scanner-combined, QSMxT uses the per-coil images
  and skips the combined ones (the log says so): the scanner's phase combination is often
  not good enough for QSM.

## What it doesn't do / limits

- Coil combination needs magnitude and phase for every coil, and at least two echoes.
- Per-coil real and imaginary images are not supported.

## What you can do

- To use the scanner's combined images instead, leave the per-coil ones out:
  `--exclude "*rec-uncombined*"`.
- `--coil-combination-sigma` sets how much the per-coil phase offsets are smoothed (TUI:
  Reconstruction › Preprocessing › Geometry).

More: [How QSMxT reads your data](/QSMxT/background/input-handling/#uncombined-receive-coils),
and the combination method in the [Methods index](/QSMxT/reference/methods/#coil-combination).
