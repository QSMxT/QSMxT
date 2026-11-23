---
title: Cite QSMxT and write the methods
description: "The paper to cite, and the methods paragraph QSMxT writes for you."
---

**Goal:** a methods paragraph and reference list for your paper.

## Steps

1. Open `derivatives/qsmxt/methods.md`. It describes what was done to your data, with the
   settings that differ from QSMxT's defaults, and lists the papers for each method that ran.
   Copy it into your methods section and edit it as you like.

   In the TUI, the **Methods** tab shows the same text for the current settings;
   **Copy methods text** (`c`) copies it.

2. Cite QSMxT:

   > Stewart AW, Robinson SD, O'Brien K, et al. "QSMxT: Robust masking and artifact
   > reduction for quantitative susceptibility mapping." *Magnetic Resonance in
   > Medicine* 87.3 (2022): 1289–1300. [doi.org/10.1002/mrm.29048](https://doi.org/10.1002/mrm.29048)

3. Cite the methods `methods.md` lists under its references.

4. Report the QSMxT version you ran (`qsmxt --version`; the output's
   `dataset_description.json` records it under `GeneratedBy`).

## Check it worked

The text matches what you ran: `methods.md` is written from what the runs actually did, not
only from the settings you asked for.

If some scans were processed differently from the rest (for example a supplied mask that
could not be used), `derivatives/qsmxt/changes/` holds a methods text for each of them.
Mention those in the paper too.

## Also useful

- `pipeline.svg`, beside `methods.md`, draws the pipeline step by step, for a figure or a
  supplement.
- `pipeline_config.toml` holds every setting, so others can rerun exactly what you ran with
  `--config`.
- `qsmxt run --dry` prints the methods text before you run anything.
