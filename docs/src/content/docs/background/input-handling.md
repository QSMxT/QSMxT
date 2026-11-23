---
title: How QSMxT reads your data
description: "What the pipeline does to real/imaginary, per-coil, GE and 2D data before it starts."
---

Most data needs nothing from you: QSMxT reads what the scanner wrote and prepares it before
the pipeline starts. This page says what it does, for the cases the [Your data](/QSMxT/data/)
pages mention.

## Real and imaginary images

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

## Uncombined receive coils

Data exported per coil element (Siemens "save uncombined")
is recognised by a `coil-<NN>` entity, which `qsmxt dicom-convert` writes. BIDS has
no entity for a coil element yet, so this is a QSMxT extension: the converted
dataset's `.bidsignore` lists `*_coil-*` so that the BIDS validator accepts the
dataset.

```
sub-*/[ses-*/]anat/*_rec-uncombined_coil-01_echo-1_part-phase_MEGRE.nii.gz
sub-*/[ses-*/]anat/*_rec-uncombined_coil-01_echo-1_part-mag_MEGRE.nii.gz
...
```

All coils of one acquisition form a single run. Before anything else the pipeline
combines them with MCPC-3D-S (see
[Coil combination](/QSMxT/reference/methods/#coil-combination)), which needs
magnitude and phase for every coil and at least two echoes, and then continues on
the combined echoes. The combined echoes are also written to the derivatives as
`*_rec-mcpc3ds_echo-<N>_part-{phase,mag}_*`.

When the same acquisition is present both per coil and scanner-combined, the
per-coil run is used and the scanner-combined one is skipped (a log line says so):
the scanner's phase combination is generally not suitable for QSM. Pass
`--exclude "*rec-uncombined*"` to process the scanner-combined data instead.

## GE data

GE real/imaginary series (`part-real`/`part-imag`) are read by `qsmxt run`: each
echo is converted to magnitude and phase, slice chopping is undone, and the phase
is negated, as GE records it with the opposite sign (see
[Real and imaginary images](#real-and-imaginary-images)).

GE phase series (`part-phase`) are not yet corrected for handedness or slice
chopping, and `qsmxt dicom-convert` warns on each one. Until that correction lands
(QSM.rs#171), use the real/imaginary export where there is one, or check the sign
of χ from GE phase against a known structure, such as iron-rich deep grey matter,
which should be paramagnetic.

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
(TUI: Preprocessing → Geometry → Ignore slice gap) reconstructs anyway, with a warning. Runs that
only make SWI, T2\*/R2\* or other non-QSM outputs continue with a warning. Without
`SliceThickness` nothing can be checked, and the run proceeds; dcm2niix writes it.

## Field strength and centre frequency

Every run needs `EchoTime` and `MagneticFieldStrength` in its sidecars. Phase is converted to
ppm with the scanner's centre frequency, `ImagingFrequency`, when a sidecar gives it (dcm2niix
writes it), and with γ̄ times `MagneticFieldStrength` otherwise. The centre frequency is the
field the scanner actually ran at (2.89 T on many Siemens "3 T" scanners), so it is the
more accurate of the two. The methods text says which one each run used.
