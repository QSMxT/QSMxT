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
| [`validate`](/QSMxT/reference/inputs/) | Validate BIDS dataset structure for QSM processing |
| [`dicom-convert`](/QSMxT/guides/running-noninteractively/#converting-dicoms-to-bids) | Convert a DICOM directory to BIDS with automatic classification |
| [`example`](/QSMxT/getting-started/quick-start/#no-data-yet-fetch-an-example) | Download an example BIDS dataset to try the pipeline on |
| [`slurm`](/QSMxT/guides/running-noninteractively/#hpc--slurm) | Generate SLURM job scripts for HPC execution: one job per run, then an aggregation job |
| [`aggregate`](/QSMxT/guides/running-noninteractively/#how-the-jobs-divide-the-work) | Run the dataset-level steps over runs processed with `run --no-dataset-steps` |
| [`tui`](/QSMxT/guides/running-interactively/) | Launch the interactive TUI |
| `update` | Check for and install the latest release |

## Standalone NIfTI tools

These operate directly on NIfTI files (in/out), independent of the BIDS
pipeline, and run the same code as the corresponding pipeline stage. See
[Standalone tools](/QSMxT/reference/tools/).

| Command | Description |
| --- | --- |
| `mask` | Masking: thresholds, BET, HD-BET, RS2-Net, MindGrab, the `--mask-preset` recipes, morphology, and/or |
| `combine` | Multi-coil phase combination (MCPC-3D-S) |
| `unwrap` | Phase unwrapping (ROMEO, Laplacian) |
| `fieldmap` | Multi-echo B0 field mapping from phase, in ppm |
| `bgremove` | Background field removal (V-SHARP, PDF, LBV, …, BFRnet) |
| `invert` | Dipole inversion: every classical and deep-learning method, plus COSMOS and STI |
| `separate` | Susceptibility source separation (paramagnetic / diamagnetic) |
| `qsmart` | QSMART vessel-aware reconstruction from the total field |
| `swi` | Susceptibility-weighted imaging with CLEAR-SWI (and minimum-intensity projection) |
| `segment` | SynthSeg whole-brain segmentation of a magnitude image |
| `r2star` | R2\* mapping from multi-echo magnitude |
| `t2star` | T2\* mapping from multi-echo magnitude |
| `r2` | R2 mapping from multi-echo spin-echo (MESE) magnitude |
| `r2prime` | R2′ = R2\* − R2, over the region R2 covers |
| `homogeneity` | Inhomogeneity correction on magnitude data |
| `resample` | Resample an oblique volume to axial orientation |
| `quality-map` | Compute a ROMEO phase-quality map |

Every command, subcommand and option, with accepted values and defaults, is
listed in the generated [CLI reference](/QSMxT/reference/cli/).

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
| `--mask-preset <P>` | Masking recipe (`robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab`) |
| `--mask <SECTION>` | Custom mask section, repeatable (e.g. `magnitude,bet:0.5,erode:2`) |
| `--mask-combine <or\|and>` | How multiple `--mask` sections combine (default `or`) |
| `--mask-refine <OP>` | Refinement applied to the combined mask, repeatable (e.g. `fill-holes:0`) |
| `--masking-input <A>` | Override the image the mask is derived from |
| `--phase-offset-removal <bool>` | Multi-echo phase-offset removal |
| `--bipolar-correction` | Bipolar readout correction (≥ 3 echoes) |
| `--romeo-*` | Fine-grained ROMEO unwrapping controls |
| `--do-chisep` / `--chisep <M>` | [Susceptibility source separation](/QSMxT/reference/algorithms/#susceptibility-source-separation) |
| `--do-segmentation` / `--do-analysis` | [SynthSeg parcellation and per-structure statistics](/QSMxT/reference/algorithms/#segmentation-and-per-structure-statistics) |
| `--qsm-reference <SPEC>` | [What χ's zero is pinned to](/QSMxT/reference/algorithms/#referencing): `mean`, `none`, or a parcellation region |
| `--export-dicom` | Also write each final map as a [DICOM series](/QSMxT/guides/running-noninteractively/#exporting-results-as-dicom) |
| `--source-dicom <DIR>` | Inherit patient/study identity from the original DICOMs when exporting |
| `--dicom-outputs <TOKEN>…` | Restrict the DICOM export to these maps (default: all produced) |

See [Algorithms](/QSMxT/reference/algorithms/) for what each algorithm option's
values do, and the [CLI reference](/QSMxT/reference/cli/) for every option of
every command.
