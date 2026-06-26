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

:::tip
Rather than writing flags or TOML by hand, configure interactively in the
[TUI](/QSMxT/guides/running-interactively/) — it shows the equivalent `qsmxt run`
command live and saves the configuration to a TOML file. Set things up once, then
reuse the command and config from the CLI and on the cluster.
:::

## Validate before you run

```sh
qsmxt validate study/bids
```

`validate` checks that your dataset has the phase/magnitude pairs and sidecar
metadata the pipeline needs, so problems surface before a long run starts.
