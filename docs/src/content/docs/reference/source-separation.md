---
title: Susceptibility source separation
description: Splitting a susceptibility map into paramagnetic and diamagnetic components, and the data each method needs.
---

A standard QSM reconstruction gives one susceptibility value per voxel, mixing
paramagnetic sources (largely iron) and diamagnetic ones (largely myelin) into a
single number. Source separation, also called χ-separation, estimates the two
components separately.

As elsewhere in QSMxT, the numerical methods come from
[QSM.rs](https://github.com/astewartau/QSM.rs); this page covers what QSMxT needs
from your BIDS dataset and what it writes back.

## Enabling it

```sh
qsmxt run study/bids --do-chisep
```

Pick a method with `--chisep`. Run `qsmxt run --help` for the current list, or
use the [TUI](/QSMxT/guides/running-interactively/), which shows the methods
alongside the maps each one requires.

## What your data needs

This is the part worth checking before you start, because methods differ in what
they can be computed from.

Some methods work from a multi-echo GRE acquisition alone, the same data a
standard QSM run uses. Others are based on R2', which QSMxT derives as
R2\* minus R2. R2\* is fitted from the GRE magnitude, but **R2 requires a
multi-echo spin-echo (MESE) acquisition**, so those methods will not run on GRE
data alone.

| Method | Additional data required |
| --- | --- |
| `r2star-qsm` (default) | none beyond the GRE acquisition |
| `decompose` | none beyond the GRE acquisition |
| `chi-sep-ilsqr` | MESE acquisition |
| `chi-sep-medi` | MESE acquisition |
| `wavesep` | MESE acquisition |
| `hc-chisep` | MESE acquisition |
| `susep-net` | MESE acquisition |
| `chi-sepnet` | MESE acquisition |

QSMxT enables the intermediate maps a method depends on automatically, so
`--do-chisep --chisep chi-sep-ilsqr` turns on R2, R2\* and R2' for you. You do
not need to request them individually.

### MESE data layout

A MESE acquisition is discovered at `sub-*/anat/*_MESE.nii*`, optionally under a
`ses-*` directory, and needs at least three echoes with `EchoTime` in its JSON
sidecars. Run [`qsmxt validate`](/QSMxT/reference/commands/) to check a dataset
before a full run.

:::caution
If a method needs a MESE acquisition and none is found, QSMxT logs a warning and
skips source separation rather than stopping, so the run still exits cleanly with
no separated maps written. If your outputs are missing, check the log for
`Skipping chi-separation`.
:::

## Bringing your own maps

If you already have a QSM, R2 or R2' map from another tool, point QSMxT at it
instead of recomputing. Each flag takes the name of a directory under
`<bids>/derivatives/`:

| Option | Supplies |
| --- | --- |
| `--use-custom-qsm <TOOL>` | susceptibility map (`*_Chimap.nii*`) |
| `--use-custom-r2 <TOOL>` | R2 map (`*_R2map.nii*`) |
| `--use-custom-r2prime <TOOL>` | R2' map (`*_R2primemap.nii*`) |

Supplying R2' directly removes the MESE requirement, since R2 is only needed to
compute it.

## Outputs

Three maps per run, under `derivatives/qsmxt/`:

```
sub-01/anat/
├── sub-01_…_desc-paramagnetic_Chimap.nii
├── sub-01_…_desc-diamagnetic_Chimap.nii
└── sub-01_…_desc-total_Chimap.nii
```

The accompanying `references.txt` cites the separation method used, along with
every other method in the run.

## Method parameters

Each method exposes its own parameters. As with the
[dipole inversion algorithms](/QSMxT/reference/algorithms/), the TUI is the
easiest way to discover them: it lists every option with its default and shows
the equivalent `qsmxt run` command as you change them.
