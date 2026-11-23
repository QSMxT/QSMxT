---
title: Run on a BIDS dataset
description: "Run the QSM pipeline on a BIDS dataset from the command line."
---

For scripting, batch processing, and reproducible runs, drive QSMxT from the
command line. (Prefer to point and click? See
[Running interactively](/QSMxT/background/tui/) — everything below is
available there too.)

:::tip[Don't memorise flags]
The easiest way to build a `qsmxt run` command is to configure your pipeline in
the [TUI](/QSMxT/background/tui/) — it shows the equivalent command
and updates it as you go. Copy the generated command (and reuse its saved config)
and run it here.
:::

## Running the pipeline

`qsmxt run` is the heart of the tool: it discovers every phase/magnitude run in a
BIDS dataset and reconstructs a quantitative susceptibility map for each.

```sh
qsmxt run <BIDS_DIR> [OUTPUT_DIR]
```

- `<BIDS_DIR>` — input BIDS directory
- `[OUTPUT_DIR]` — defaults to `<BIDS_DIR>`; results go to
  `<OUTPUT_DIR>/derivatives/qsmxt/`

With no options, QSMxT runs with sensible defaults end to end. Need a BIDS dataset
first? See [Converting DICOMs to BIDS](/QSMxT/how-to/convert-dicom/#converting-dicoms-to-bids) below.

## Selecting runs

Process only part of a dataset with glob patterns:

```sh
# Only these subjects
qsmxt run study/bids --include "sub-01*" "sub-02*"

# Everything except a session
qsmxt run study/bids --exclude "*ses-pilot*"

# Cap the number of echoes used
qsmxt run study/bids --num-echoes 4
```

## Multi-echo phase handling

```sh
# Receiver phase-offset removal (on by default for multi-echo)
qsmxt run study/bids --phase-offset-removal true

# Bipolar readout correction (needs ≥ 3 echoes)
qsmxt run study/bids --bipolar-correction
```

ROMEO unwraps the first echo spatially and the others temporally from it
(`--romeo-template` picks another echo); `--romeo-individual` unwraps each echo on
its own, for motion between echoes, optionally with inter-echo 2π correction
(`--romeo-correct-global`). See `qsmxt run --help` for the full set of
`--romeo-*` flags.

## Using a configuration file

Reproduce a run exactly by saving settings to a TOML file and passing `--config`:

```sh
qsmxt init > pipeline.toml      # generate a starting config
qsmxt run study/bids --config pipeline.toml
```

See [Configuration](/QSMxT/reference/configuration/).

## Re-running

Intermediate results are cached to disk. Re-running the same dataset skips any
stage that already completed, so tweaking a late-stage option doesn't recompute
the whole pipeline.

## Generate a starting config

```sh
qsmxt init > pipeline.toml
```

`qsmxt init` writes a configuration pre-populated with the default settings,
ready to edit. Run `qsmxt init --help` for options.

## Use it

```sh
qsmxt run study/bids --config pipeline.toml
```

Command-line flags take precedence over values in the file, so you can keep a
base config and override individual settings per run:

```sh
qsmxt run study/bids --config pipeline.toml --qsm-algorithm tgv
```

## Validate before you run

```sh
qsmxt validate study/bids
```

`validate` checks that your dataset has the phase/magnitude pairs and sidecar
metadata the pipeline needs, so problems surface before a long run starts.
