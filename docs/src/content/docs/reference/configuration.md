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

A method that converts the field to phase at one echo time has a `te` setting naming it:
`"first"`, `"last"`, `"spacing"` (TE2 − TE1), `"sum"` (of the echo times), or a given time
in seconds. On the command line the same choice is `first`, `last`, `spacing`, `sum`, or a
time in ms:

```toml
[inversion.medi]
te = "spacing"            # or "first", "last", "sum", or { seconds = 0.02 }

[inversion.whqsm]
weight = "flat"           # or "max-normalised", "in-vivo"
```

`te = { seconds = 0.02 }` is `--medi-te 20`. A setting the method derives from its others
(`inversion.tv.rho`, `inversion.nltv.rho`, FANSI's `mu1`, `alpha0` and `mu0`, L1-QSM's `mu1`,
HD-QSM's `mu1_l2`) is left out of the file to take the derived value; on the command line it is
`auto`.

:::tip
Rather than writing flags or TOML by hand, configure interactively in the
[TUI](/QSMxT/guides/running-interactively/) — it shows the equivalent `qsmxt run`
command live. Set things up once, then reuse the command on the CLI and on the
cluster; the `pipeline_config.toml` every run writes to `derivatives/qsmxt/` is a
config file you can pass back with `--config`.
:::

## Files from earlier versions

A config file QSMxT writes starts with `schema_version`, the layout it was written in.
A file without it is read as follows, and what changed is logged:

- **A full config file an earlier QSMxT wrote** (`qsmxt init`, or a run's
  `pipeline_config.toml`) holds every setting, so it would otherwise keep the defaults of
  the release that wrote it as though they had been chosen. A setting holding one of an
  earlier release's defaults takes today's default instead. Any other value is kept.
- **Any other file** (written by hand, or a changed-settings file) holds what its author
  chose, so nothing is reset.
- **Settings whose meaning changed** are refused when they hold anything but today's
  default and their method runs: TGV's `alpha0`/`alpha1` (now in radians at the echo time
  `inversion.tgv.te` picks, the first by default, and the field strength), MEDI's and TFI's `lambda` (now MEDI_L1's data
  weight), and `field_mapping.phase_offset_sigma` (now `phase_offset_sigma_mm`, in mm).
  The message says what changed. Give the value in the new terms and add `schema_version`
  with the current version at the top, or remove the setting to use the default. When the
  method does not run, the value is dropped with a note.
- Settings an earlier file does not have (each method's echo time `te`, WH-QSM's `weight`,
  NLTV's `rho` and `tol_newton`, RTS's `tau` and `gamma`, PDF's `n_cg`, `space` and `n_pad`,
  HARPERELLA's and iHARPERELLA's `pad`) take today's defaults.
- Settings that no longer exist are ignored, with a warning where that changes what a
  run does (for example `bg_removal.msmv_refine`: mSMV is now MEDI's option,
  `inversion.medi.msmv`).

## Validate before you run

```sh
qsmxt validate study/bids
```

`validate` checks that your dataset has the phase/magnitude pairs and sidecar
metadata the pipeline needs, so problems surface before a long run starts.
