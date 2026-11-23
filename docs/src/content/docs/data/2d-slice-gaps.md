---
title: 2D acquisitions and slice gaps
description: "2D multi-slice data, thick slices, and gaps between slices."
---

A 2D multi-slice acquisition often has thicker slices than in-plane voxels, and can leave a
gap between slices.

## What QSMxT does automatically

- It checks each scan for a gap: when the slice thickness in the sidecar (`SliceThickness`)
  is more than 1 % thinner than the distance between slices, QSM stops before processing and
  says so. The tissue in the gap was never measured, and QSM needs a continuous volume, so the
  map would look plausible but be wrong.
- Runs that only make SWI, T2\* or R2\* carry on, with a warning.
- Background removal with V-SHARP (the default) estimates the background on a 1 mm isotropic
  grid, where its spherical kernels are still spheres on thick slices, and keeps the local
  field at the acquired resolution.

## What it doesn't do / limits

- Without `SliceThickness` in the sidecar there is nothing to check, and the run goes ahead.
  `qsmxt dicom-convert` (dcm2niix) writes it.
- Thick slices limit what any QSM method can recover along the slice direction.

## What you can do

- Prefer a 3D acquisition for QSM where you can.
- `--ignore-slice-gap` reconstructs anyway, with a warning (TUI: Reconstruction ›
  Preprocessing › Geometry › Ignore slice gap). Only use it if you know the gap is small enough
  not to matter.

More: [How QSMxT reads your data](/QSMxT/background/input-handling/#slice-gaps-in-2d-acquisitions).
