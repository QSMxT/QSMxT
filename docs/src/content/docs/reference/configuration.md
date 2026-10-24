---
title: Configuration
description: Save and reproduce pipeline settings with a TOML configuration file.
---

Anything you can set on the `qsmxt run` command line can also live in a TOML
configuration file. A config file makes runs reproducible and is the cleanest way
to drive batch and [HPC](/QSMxT/guides/running-noninteractively/#hpc--slurm) processing.

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

## What's in it

The configuration captures the choice of algorithm at each pipeline stage along
with that algorithm's parameters — masking, phase-offset removal, unwrapping,
background-field removal, dipole inversion, and referencing. Because each
algorithm has its own parameters, the config is the place to tune things like
regularisation weights and iteration counts that aren't exposed as top-level
flags.

Multi-orientation reconstruction lives in its own `[multi_orientation]` section,
off unless `group_by` names a pattern:

```toml
[multi_orientation]
group_by = "*acq-dir*"   # entity name, glob, or re:<regex> — empty = off
algorithm = "cosmos"     # or "sti"
lambda = 0.0
```

See [multi-orientation](/QSMxT/reference/algorithms/#multi-orientation-cosmos-and-sti)
for what the pattern matches and what will be refused.

:::tip
Rather than writing flags or TOML by hand, configure interactively in the
[TUI](/QSMxT/guides/running-interactively/) — it shows the equivalent `qsmxt run`
command live. Set things up once, then reuse the command on the CLI and on the
cluster; the `pipeline_config.toml` every run writes to `derivatives/qsmxt/` is a
config file you can pass back with `--config`.
:::

## Files from earlier versions

A config file QSMxT writes starts with `schema_version`, the layout it was written in.
A file without it, or with an older one, is brought up to date as it is read, with a
warning for each change:

- A setting at the default it had when the file was written takes today's default.
  A full config file holds every setting, so without this it would keep every old
  default as though it had been chosen.
- A setting whose meaning changed is refused if it holds anything else: TGV's
  `alpha0`/`alpha1` (now in radians at the scan's first echo time and field strength),
  MEDI's and TFI's `lambda` (now MEDI_L1's), and `field_mapping.phase_offset_sigma`
  (now `phase_offset_sigma_mm`, in mm). The message says what changed. Give the value in
  the new unit and add `schema_version` with the current version at the top, or remove
  the setting to use the default.
- Settings that no longer exist are ignored, with a warning where that changes what a
  run does (for example `bg_removal.msmv_refine`: mSMV is now MEDI's option,
  `inversion.medi.msmv`).

## Validate before you run

```sh
qsmxt validate study/bids
```

`validate` checks that your dataset has the phase/magnitude pairs and sidecar
metadata the pipeline needs, so problems surface before a long run starts.
