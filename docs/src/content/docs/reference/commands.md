---
title: Commands
description: The full QSMxT command-line reference.
---

QSMxT is a single binary with a set of subcommands. Run `qsmxt <command> --help`
for the complete, authoritative options of any command.

## Pipeline & datasets

| Command | Description |
| --- | --- |
| [`run`](/QSMxT/guides/running-noninteractively/) | Run the full QSM pipeline on a BIDS dataset |
| `init` | Generate a pipeline configuration file (TOML) |
| `validate` | Validate BIDS dataset structure for QSM processing |
| [`dicom-convert`](/QSMxT/guides/running-noninteractively/#converting-dicoms-to-bids) | Convert a DICOM directory to BIDS with automatic classification |
| [`slurm`](/QSMxT/guides/running-noninteractively/#hpc--slurm) | Generate SLURM job scripts for HPC execution |
| [`tui`](/QSMxT/guides/running-interactively/) | Launch the interactive TUI |
| `update` | Check for and install the latest release |

## Standalone NIfTI tools

These operate directly on NIfTI files (in/out), independent of the BIDS
pipeline. See [Standalone tools](/QSMxT/reference/tools/).

| Command | Description |
| --- | --- |
| `mask` | Masking operations (Otsu, value, percentile, BET, morphology) |
| `unwrap` | Phase unwrapping |
| `bgremove` | Background field removal |
| `invert` | Dipole inversion |
| `swi` | Susceptibility-weighted imaging |
| `r2star` | R2\* mapping from multi-echo magnitude |
| `t2star` | T2\* mapping from multi-echo magnitude |
| `homogeneity` | Inhomogeneity correction on magnitude data |
| `resample` | Resample an oblique volume to axial orientation |
| `quality-map` | Compute a ROMEO phase-quality map |

## Global

```sh
qsmxt --help       # list all commands
qsmxt --version    # print version
qsmxt <cmd> --help # full help for a command
```

## `run` options at a glance

| Option | Purpose |
| --- | --- |
| `--config <FILE>` | Load settings from a TOML configuration |
| `--include <GLOB>…` / `--exclude <GLOB>…` | Select runs by pattern |
| `--num-echoes <N>` | Limit echoes processed |
| `--qsm-algorithm <A>` | Dipole inversion method |
| `--unwrapping-algorithm <A>` | Phase unwrapping method |
| `--bf-algorithm <A>` | Background field removal method |
| `--masking-algorithm <A>` | Masking method |
| `--masking-input <A>` | What the mask is derived from |
| `--phase-offset-removal <bool>` | Multi-echo phase-offset removal |
| `--bipolar-correction` | Bipolar readout correction (≥ 3 echoes) |
| `--romeo-*` | Fine-grained ROMEO unwrapping controls |

See [Algorithms](/QSMxT/reference/algorithms/) for the valid values of each
algorithm option.
