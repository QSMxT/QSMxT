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

A MESE acquisition needs at least three echoes to fit R2. Fewer are ignored. It
does not have to be on the same grid as the GRE — a 2D stack with thicker slices
or a smaller matrix is resampled onto whatever grid the run reconstructs on,
using its affine. R2 is reported only where the MESE's field of view reaches.

### Real and imaginary images

Some scanners (GE in particular) export the complex image as real and imaginary
parts instead of magnitude and phase. These are recognised by the BIDS
`part-real` / `part-imag` entities, which `qsmxt dicom-convert` writes:

```
sub-*/[ses-*/]anat/*_echo-1_part-real_MEGRE.nii.gz
sub-*/[ses-*/]anat/*_echo-1_part-imag_MEGRE.nii.gz
...
```

They are used only when the acquisition has no `part-phase` images. Before
anything else the pipeline converts each echo to magnitude and phase. Some
reconstructions flip the sign of every other slice, which shows up as a π phase
jump between neighbouring slices. The conversion detects this along each axis
and undoes it. It makes one decision for all echoes, and the log records which
axes it corrected. When the sidecar's `Manufacturer` is GE the phase is also
negated, because GE's phase sign is the opposite of the Siemens and Philips
convention the pipeline assumes. A `part-mag` image is not needed, because the magnitude comes
from the real and imaginary parts. Per-coil real/imaginary data is not supported.

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

## GE data

GE phase is not yet corrected for handedness or slice chopping, and GE
real/imaginary series (`part-real`/`part-imag`) are converted but not read by
`qsmxt run`. `qsmxt dicom-convert` warns on each GE phase, real or imaginary series. Until the
correction lands (QSM.rs#171), check the sign of χ from GE data against a known
structure, such as iron-rich deep grey matter, which should be paramagnetic.

## Slice gaps in 2D acquisitions

A 2D multi-slice acquisition can leave a gap between slices: the excited slice
(`SliceThickness` in the sidecar) is thinner than the slice-to-slice pitch (the
voxel size). The tissue in the gap was never sampled, and the FFT kernels used by
background-field removal and dipole inversion assume a contiguous grid, so the map
is wrong while still looking like a susceptibility map.

QSMxT reads `SliceThickness`, `MRAcquisitionType` and `SliceEncodingDirection` from
the first echo's sidecar. When the acquisition is 2D (or the thickness differs from
the pitch) and the slices are more than 1% thinner than the pitch, a run that asks
for QSM stops before any processing with a message saying so. `--ignore-slice-gap`
(TUI: Preprocessing → Geometry → Ignore Slice Gap) reconstructs anyway, with a warning. Runs that
only make SWI, T2\*/R2\* or other non-QSM outputs continue with a warning. Without
`SliceThickness` nothing can be checked, and the run proceeds; dcm2niix writes it.

## What each output needs

Phase is what a run is discovered from, so it is present for everything below.
This table lists what each output needs in addition to it.

| Output | Also requires |
| --- | --- |
| QSM | nothing, though magnitude is recommended and some masking options need it |
| SWI (`--do-swi`), SMWI (`--do-smwi`) | magnitude |
| Segmentation (`--do-segmentation`), and so `--do-analysis` and a region `--qsm-reference` | magnitude, or a dseg via `--use-custom-dseg` |
| R2\*/T2\* (`--do-r2starmap`, `--do-t2starmap`) | magnitude, at least 3 echoes, equally spaced (within 0.1 ms) |
| R2 (`--do-r2map`) | a MESE acquisition with at least 3 equally spaced echoes, the first at the echo spacing (TE₁ = ESP) |
| R2' (`--do-r2primemap`) | R2\* and R2 (above). Reported only where the MESE reached; without one, predicted from R2\* by R2PRIMEnet, which is 3 T only |

ARLO, which fits R2\*, integrates the decay with a single echo spacing, and the EPG
fit behind R2 simulates a spin-echo train with echoes at multiples of the spacing.
Bipolar or flyback readouts with uneven spacing, or an extra long echo, would give
wrong maps rather than noisy ones, so they are refused.

[Source separation](/QSMxT/reference/algorithms/#susceptibility-source-separation)
(`--do-chisep`) splits into two groups. The `r2star-qsm` and `decompose` methods
work from multi-echo GRE magnitude alone (`r2star-qsm` needs what R2\* needs). The
other six are based on R2', so they need a MESE acquisition as well, or R2PRIMEnet
at 3 T.

An uncombined-coil run needs at least two echoes for MCPC-3D-S, whatever is asked for.

:::caution
Before any run is processed, QSMxT checks every run against what was asked for:

- An output **you asked for** that a run's data cannot produce stops the command
  with an error that lists every such run and what it is missing. Nothing is
  processed. Narrow the runs with `--include`/`--exclude`, or drop the option.
- An output that **another option turned on** (R2\* for `--chisep r2star-qsm`,
  R2 and R2\* for R2', segmentation for `--do-analysis`) is skipped with a warning
  that names the option that implied it.

A named `--use-custom-*` derivative that is missing for a run counts as asked for.
`qsmxt validate` shows the same checks per run without processing anything.
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

A bring-your-own R2 or R2' map that only covers part of the brain carries the
same caveat as a MESE slab: leave the uncovered voxels at zero rather than
filling them, since a zero R2 is read as an absent measurement and excluded,
while a small non-zero one is taken at face value.
