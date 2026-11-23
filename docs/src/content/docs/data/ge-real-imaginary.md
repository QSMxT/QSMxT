---
title: GE and real/imaginary data
description: "Data exported as real and imaginary images, and GE phase."
---

Some scanners, GE in particular, export the complex image as **real and imaginary** parts
instead of magnitude and phase.

## What QSMxT does automatically

- `qsmxt dicom-convert` names them `part-real` and `part-imag`.
- `qsmxt run` uses them when a scan has no phase images: it computes magnitude and phase from
  them before anything else, so you don't need a separate magnitude.
- It finds and undoes the sign flip some reconstructions apply to every other slice.
- For GE data (the sidecar's `Manufacturer`), it flips the sign of the phase, since GE records
  phase with the opposite sign to Siemens and Philips.

## What it doesn't do

- **GE phase images** (`part-phase`) are not corrected for the sign or the slice flip yet.
  `qsmxt dicom-convert` warns on each one.
- Real and imaginary images per receive coil are not supported.

## What you can do

- With GE data, export real and imaginary images where the scanner offers them, and use those.
- If you only have GE phase, check the sign of the map: iron-rich deep grey matter (globus
  pallidus, putamen) should be bright. If it is dark, the sign is reversed.

How the conversion works is in [How QSMxT reads your data](/QSMxT/background/input-handling/#real-and-imaginary-images).
