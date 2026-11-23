---
title: Bipolar readouts
description: "Multi-echo data acquired with alternating (bipolar) readout gradients."
---

In a **bipolar** readout, the echoes of a multi-echo gradient echo are read with alternating
gradient polarity. This shortens the echo spacing, but it gives odd and even echoes opposite
phase errors, mostly a ramp along the readout direction. Unless these are corrected, the field
map treats them as field. Without correction, a bipolar scan's field map is wrong almost
everywhere, and susceptibility can be scaled several-fold.

## What QSMxT does automatically

- `qsmxt dicom-convert` records the readout polarity in each sidecar as
  `"ReadoutPolarity": "bipolar"` or `"monopolar"` when the DICOM header says it. Siemens
  headers do (the protocol's readout mode).
- When a sidecar says `"ReadoutPolarity": "bipolar"` and there are at least three echoes,
  bipolar correction is required. The TUI offers it as a *Required change*. `qsmxt run` turns
  it on itself, says so in the log, and lists it in the methods text's Note. If phase-offset
  removal is off, as in the STI Suite, QSMART and UK Biobank pipelines, it is turned on too,
  because bipolar correction runs as part of it.
- With MEDI's complex fit (the [MEDI toolbox pipeline](/QSMxT/toolbox-pipelines/medi-toolbox/)),
  bipolar correction uses [MEDI's bipolar echo fit](/QSMxT/reference/methods/#medis-complex-fit),
  which fits a separate phase for the even echoes.

## What it doesn't do

- GE and Philips DICOM headers do not say the readout polarity in a field QSMxT can read, and
  BIDS has no field for it. Data converted with plain dcm2niix lacks it too.
- Without the field, QSMxT checks the data. If the echoes show the odd/even pattern of a
  bipolar readout, `qsmxt run` warns you in the log, and front ends that check the data
  suggest bipolar correction. This check is not certain, so nothing is changed for you.
- Bipolar correction needs at least three echoes, and evenly spaced echoes for the data
  check. It does not apply to per-echo QSM.

## What you can do

- If you know your scan's readout is bipolar (the protocol's "readout mode", "bipolar", or
  "flyback off"), add `"ReadoutPolarity": "bipolar"` to the echo sidecars, or turn the
  correction on:

  ```sh
  qsmxt run study/bids --bipolar-correction
  ```

  In the TUI: *Bipolar correction* under Reconstruction › QSM › Field map.
- If QSMxT warns about a bipolar signature, check the protocol. If it is bipolar, do the same.
- Correcting data that has already been corrected (for example, by the scanner or an earlier
  pipeline) changes little and is harmless.

The field-mapping options are in [Settings](/QSMxT/reference/configuration/).
