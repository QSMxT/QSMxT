---
title: Bipolar readouts
description: "Multi-echo data acquired with alternating (bipolar) readout gradients."
---

Some multi-echo sequences read out alternate echoes in opposite directions (a bipolar
readout). This adds a phase that alternates between echoes and varies linearly across the
image.

## What QSMxT does automatically

- Nothing by default: QSMxT does not detect a bipolar readout, so bipolar correction is
  **off** unless you turn it on.
- Once on, it is skipped for scans with fewer than three echoes, and the log says so.

## What it doesn't do / limits

- It needs at least three echoes.
- It is part of the phase-offset removal (MCPC-3D-S), so it does nothing with
  `--phase-offset-removal false`; QSMxT asks you to turn that back on.
- It can't be used with MEDI's complex fit (`--b0-estimation complex-fit`). The
  [MEDI toolbox pipeline](/QSMxT/toolbox-pipelines/) uses the toolbox's monopolar fit.

## What you can do

- If your protocol uses bipolar readouts, turn the correction on:

  ```sh
  qsmxt run study/bids --bipolar-correction
  ```

  In the TUI: Reconstruction › QSM › Field map.
- Not sure? Check the protocol's readout mode (monopolar or bipolar).

The field-mapping options are in [Settings](/QSMxT/reference/configuration/).
