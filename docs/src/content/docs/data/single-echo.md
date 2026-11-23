---
title: Single-echo data
description: "Gradient-echo data with one echo."
---

QSM works on a single echo, but several steps need more than one.

## What QSMxT does automatically

- `qsmxt dicom-convert` names a single-echo scan `T2starw` (several echoes are `MEGRE`).
- The field is the unwrapped phase divided by the echo time; there are no echoes to combine.
- Steps that need several echoes are skipped for that scan, and the log and methods text say
  so: phase-offset removal, motion correction, bipolar correction, and MEDI's CSF term.
- An output you asked for that needs several echoes (R2\* or T2\* need at least three) stops
  the run before processing, with a message naming the scan.

## What it doesn't do / limits

- With one echo, the receiver's phase offset cannot be separated from the field.
- No R2\*/T2\* maps, so no R2\*-based source separation (`r2star-qsm`) and no DECOMPOSE.
- Uncombined coil data needs at least two echoes to be combined.

## What you can do

- Run with the defaults: they work on a single echo.
- Methods that work from the wrapped phase directly also suit single-echo data: iQSM, iQSM+
  and TGV (`--qsm-algorithm tgv`), which unwraps, removes the background and inverts in one step.
- For new acquisitions, several echoes give a better field map and allow R2\*.

What each output needs is in
[Input data layout](/QSMxT/reference/inputs/#what-each-output-needs).
