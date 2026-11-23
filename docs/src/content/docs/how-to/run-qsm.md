---
title: Run QSM
description: "Make susceptibility maps from a BIDS dataset, in the TUI or on the command line."
---

**Goal:** a susceptibility map for every scan in your BIDS dataset. The defaults are meant to
work as they are; you don't need to choose any methods.

## In the TUI

1. Start it:

   ```sh
   qsmxt tui
   ```

2. On the **Input** tab, leave *Input mode* on **BIDS** and fill in the BIDS directory. The run
   tree lists the scans QSMxT found.
3. Press `F5` to run (or use the button on the **Execution** tab).

The TUI shows the matching `qsmxt run` command as you change settings, so you can copy it into
a script later.

## On the command line

1. Check what will run, without running anything:

   ```sh
   qsmxt run study/bids --dry
   ```

2. Run:

   ```sh
   qsmxt run study/bids
   ```

Results go to `study/bids/derivatives/qsmxt/`. To write them somewhere else, add the output
directory: `qsmxt run study/bids /path/to/output`.

## Check it worked

Each scan has a `*_Chimap.nii` under `derivatives/qsmxt/sub-*/anat/`, and the log
(`derivatives/qsmxt/qsmxt.log`) has no errors. [Look at your results](/QSMxT/how-to/results/) says what to
check.

## Optional

- **Only some scans:** `qsmxt run study/bids --include "sub-01*"` (or `--exclude`). In the TUI,
  use *Include* and *Exclude* on the Input tab.
- **Re-running** skips the steps that already finished, so changing a late setting is quick.
  `--force` starts again from scratch.
- **Save the settings:** every run writes them to `derivatives/qsmxt/pipeline_config.toml`.
  Run them again with `qsmxt run other/bids --config pipeline_config.toml`; flags given
  beside `--config` win.
- **Start from a published toolbox's pipeline** (MEDI, STI Suite, QSMART, …): see
  [Toolbox pipelines](/QSMxT/toolbox-pipelines/).

## If your data is different

QSMxT checks every scan before it starts and stops with a message if something it needs is
missing. The [Your data](/QSMxT/data/) pages say what it does automatically for single-echo,
oblique, 2D, 7 T, rodent and other data, and what you may want to change. Every setting is in
[Settings](/QSMxT/reference/configuration/).
