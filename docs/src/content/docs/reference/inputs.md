---
title: Input data
description: What a QSMxT-compatible BIDS dataset looks like, and which outputs each acquisition enables.
---

QSMxT reads [BIDS](https://bids.neuroimaging.io/) datasets. If you convert your
DICOMs with [`qsmxt dicom-convert`](/QSMxT/guides/running-noninteractively/#converting-dicoms-to-bids)
the layout is handled for you, and this page is mostly reference. If you are
assembling a dataset by hand, or wondering why an output did not appear, start
here.

## Checking a dataset

```sh
qsmxt validate study/bids
```

This reports every run it discovers, the echo times and field strength it read,
and which outputs your data can support. Run it before a long job:

```
  sub-01_acq-gre_MEGRE
    Echoes: 4
    Echo times: [0.004, 0.012, 0.02, 0.028] s
    Field strength: 3.0 T
    Magnitude: present
    MESE (spin-echo): not found
    Capabilities:
      QSM reconstruction:  yes
      R2*/T2* mapping:     yes
      SWI:                 yes
```

## Expected layout

The pipeline discovers a run from its phase images:

```
sub-*/[ses-*/]anat/*_part-phase_*.nii[.gz]
```

with matching JSON sidecars containing `EchoTime` and `MagneticFieldStrength`.
Multi-echo data uses the BIDS `echo-<N>` entity. Magnitude images sit alongside
as `*_part-mag_*`, and are optional for a basic QSM run but required by most
other outputs.

Two further acquisitions are recognised when present:

| Pattern | Provides |
| --- | --- |
| `sub-*/[ses-*/]anat/*_part-mag_*.nii[.gz]` | magnitude, for R2\*/T2\*, SWI and masking |
| `sub-*/[ses-*/]anat/*_echo-*_MESE.nii[.gz]` | multi-echo spin-echo, for R2 and hence R2' |

## What each output needs

| Output | Requires |
| --- | --- |
| QSM | phase (magnitude recommended, and required by some masking options) |
| SWI (`--do-swi`) | magnitude |
| R2\*/T2\* (`--do-r2starmap`, `--do-t2starmap`) | at least 3 echoes, magnitude |
| R2 (`--do-r2map`) | a MESE acquisition |
| R2' (`--do-r2primemap`) | R2\* and R2, so magnitude plus a MESE acquisition |

[Source separation](/QSMxT/reference/algorithms/#susceptibility-source-separation)
(`--do-chisep`) splits into two groups. The `r2star-qsm` and `decompose` methods
need only the GRE acquisition. Every other method is based on R2', so it needs a
MESE acquisition as well.

:::caution
If a requested output cannot be computed from your data, QSMxT logs a warning and
skips it rather than stopping, so the run still exits cleanly with that output
missing. `qsmxt validate` tells you in advance; if an output is unexpectedly
absent afterwards, check the log for `Skipping`.
:::

## Bringing your own inputs

QSMxT will use maps produced by other tools from `<bids>/derivatives/<TOOL>/`,
following the same `sub-*/[ses-*/]anat/` layout:

| Option | Supplies | Filename |
| --- | --- | --- |
| `--use-custom-masks [TOOL]` | brain mask | `*_mask.nii*` |
| `--use-custom-qsm <TOOL>` | susceptibility map | `*_Chimap.nii*` |
| `--use-custom-r2 <TOOL>` | R2 map | `*_R2map.nii*` |
| `--use-custom-r2prime <TOOL>` | R2' map | `*_R2primemap.nii*` |

Supplying R2' directly removes the MESE requirement for source separation, since
R2 is only needed to compute it. `qsmxt validate` lists the derivative tools it
can see for each run.
