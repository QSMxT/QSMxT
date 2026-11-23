---
title: Oblique slices
description: "Acquisitions whose slices are tilted relative to the magnetic field."
---

Many acquisitions are tilted to follow the anatomy, so the slices are not perpendicular to the
main magnetic field.

## What QSMxT does automatically

- It reads the field direction from the image header (or from `B0_dir` in the sidecar, when
  there is one) and gives it to every method that takes one. Nothing is resampled, so the map
  stays on the acquired grid.
- Methods that assume the field is along the slice axis (most deep-learning inversions,
  among others) get the data resampled to an axial grid first, and their results are
  resampled back.

## What it doesn't do / limits

- Resampling interpolates, so a resampled map is slightly smoother.
- If the header's orientation is wrong (as after some format conversions), the field
  direction is wrong too, and QSMxT cannot tell.

## What you can do

- `qsmxt validate study/bids` shows each scan's field direction and how oblique it is.
- `--axial-resampling` resamples tilted scans to axial for every method (above
  `--obliquity-threshold`, 10° by default). It is off by default, and is rarely needed.
- `--output-space working` keeps resampled results on the axial grid.

Which methods need axial data, and what resampling costs, is in
[Geometry](/QSMxT/background/geometry/#oblique-acquisitions).
