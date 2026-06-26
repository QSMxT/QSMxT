---
title: Running interactively
description: Convert data, configure, and run the QSM pipeline from QSMxT's interactive terminal interface.
---

The interactive terminal interface (TUI) is the recommended way to use QSMxT. It
takes you through the whole workflow — converting DICOMs, configuring the
pipeline, and running it — without writing a single command flag or config file.

```sh
qsmxt tui
```

## What you can do

- **Convert DICOMs to BIDS** — point at a DICOM directory, review the automatic
  series classification, relabel anything that needs it, and convert. (See
  [DICOM → BIDS](/QSMxT/guides/running-noninteractively/#converting-dicoms-to-bids) for what the classifier handles.)
- **Point at a dataset** and let QSMxT discover the phase/magnitude runs.
- **Choose algorithms** for masking, unwrapping, background-field removal, and
  dipole inversion from menus — no need to memorise flag names or valid values.
- **Adjust parameters** for each stage, with sensible defaults pre-filled.
- **Copy the equivalent command** — as you configure, the TUI shows the matching
  `qsmxt run` command and updates it live, and saves your settings to a config
  file. It's the easiest way to build a run command without memorising flags.
- **Run the pipeline** and watch progress without leaving the terminal.
- **Submit to a cluster** — generate and launch SLURM jobs for large cohorts
  directly from the TUI; scaling out doesn't require the command line.

The TUI writes the same configuration the CLI consumes, so anything you set up
interactively can be saved and reproduced [noninteractively](/QSMxT/guides/running-noninteractively/)
with `qsmxt run --config` — or by copying the generated command.

:::tip[Prefer the browser?]
You can try the same reconstruction algorithms with no installation at all in
**[qsmbly](https://qsmbly.neurodesk.org/)** — a browser build of the QSM engine.
It's a great way to experiment before processing a full dataset with QSMxT.
:::

:::note
The TUI is ideal for exploring a new dataset and dialling in parameters. Once
you're happy, capture the configuration to a TOML file and reuse it
[noninteractively](/QSMxT/guides/running-noninteractively/) for scripted batch
processing.
:::
