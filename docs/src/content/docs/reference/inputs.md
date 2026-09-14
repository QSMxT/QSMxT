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

A MESE acquisition needs at least three echoes to fit R2. Fewer are ignored.

### Uncombined receive coils

Data exported per coil element (Siemens "save uncombined", as in UK Biobank SWI)
is recognised by the BIDS `coil-<NN>` entity that `qsmxt dicom-convert` writes:

```
sub-*/[ses-*/]anat/*_rec-uncombined_coil-01_echo-1_part-phase_MEGRE.nii.gz
sub-*/[ses-*/]anat/*_rec-uncombined_coil-01_echo-1_part-mag_MEGRE.nii.gz
...
```

All coils of one acquisition form a single run. Before anything else the pipeline
combines them with MCPC-3D-S (see
[Coil combination](/QSMxT/reference/algorithms/#coil-combination)), which needs
magnitude and phase for every coil and at least two echoes, and then continues on
the combined echoes. The combined echoes are also written to the derivatives as
`*_rec-mcpc3ds_echo-<N>_part-{phase,mag}_*`.

When the same acquisition is present both per coil and scanner-combined, the
per-coil run is used and the scanner-combined one is skipped (a log line says so):
the scanner's phase combination is generally not suitable for QSM. Pass
`--exclude "*rec-uncombined*"` to process the scanner-combined data instead.

## What each output needs

Phase is what a run is discovered from, so it is present for everything below.
This table lists what each output needs in addition to it.

| Output | Also requires |
| --- | --- |
| QSM | nothing, though magnitude is recommended and some masking options need it |
| SWI (`--do-swi`) | magnitude |
| R2\*/T2\* (`--do-r2starmap`, `--do-t2starmap`) | magnitude, at least 3 echoes |
| R2 (`--do-r2map`) | a MESE acquisition, at least 3 echoes |
| R2' (`--do-r2primemap`) | magnitude and a MESE acquisition, for R2\* and R2 |

[Source separation](/QSMxT/reference/algorithms/#susceptibility-source-separation)
(`--do-chisep`) splits into two groups. The `r2star-qsm` and `decompose` methods
work from multi-echo GRE magnitude alone. The other six are based on R2', so they
need a MESE acquisition as well.

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
