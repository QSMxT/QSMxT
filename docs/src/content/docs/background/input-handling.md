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

GE real/imaginary series are handled as [above](#real-and-imaginary-images), with the phase
negated. GE phase series (`part-phase`) are not yet corrected for handedness or slice
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

## DICOM conversion

`qsmxt dicom-convert` classifies every series with no prompts, then hands the images to
dcm2niix. It de-identifies with dcm2niix's `-ba y`; `--keep-dicom-uids` uses `-ba o`, which keeps
`StudyInstanceUID`, `SeriesInstanceUID`, `StudyID` and the dates, and adds the
`FrameOfReferenceUID`, under the DICOM keywords as BIDS recommends for metadata it does not
define.

### What it handles

- **Gradient-echo magnitude, phase, real and imaginary images** — from the DICOM
  image type, GE's private image-type tag, or the Enhanced DICOM frame type; split
  correctly even when a vendor packs them under a single `SeriesInstanceUID`
  (some Philips and GE exports). Only gradient-echo sequences are labelled this way.
- **Echoes** — a single-echo acquisition is named `T2starw`, a multi-echo one
  `MEGRE` with `echo-N`. Series exported one echo at a time are numbered by echo
  time.
- **Runs and acquisitions** — series of one protocol acquired together are one
  run. A repeat with the same settings is `run-2`; a repeat with different
  settings (echo times, TR, flip angle, matrix, slice thickness, voxel size,
  receive coil) is a separate acquisition, `acq-<protocol>2`.
- **The scanner's own reconstructions** — kept next to the images they came from,
  each with its own name: susceptibility map `rec-scanner_Chimap`, T2\*/R2\* maps
  `rec-scanner_T2starmap`/`rec-scanner_R2starmap`, SWI `rec-swi_part-mag_T2starw`,
  minimum-intensity projection `rec-mip_T2starw`, filtered magnitude
  `rec-filtered`. Other reconstructions of the same images get a `rec-` label from
  their image type or description; the magnitude from the same reconstruction as
  the phase is the main one.
- **T1-weighted structurals** — MPRAGE, SPACE/TFE T1, MP2RAGE `UNI` images and
  other T1w series are `T1w`; MP2RAGE inversion images are `inv-1`/`inv-2`
  `MP2RAGE`, and its T1 map `T1map`.
- **Individual coil images** — each coil element of an uncombined acquisition is
  converted separately with a `coil-NN` label (`rec-uncombined` when the
  scanner-combined images are there too). A series with a single coil element is
  the combined image.
- **Duplicates** — the same images exported twice are converted once; two
  different original reconstructions of one acquisition keep the first and put
  the other in `extra_data/`.
- **Everything else** — scouts, reformats, FLAIR, diffusion, Dixon and other
  vendor maps are kept in `sub-*/[ses-*/]extra_data/` as dcm2niix names them,
  outside BIDS (listed in `.bidsignore`). Reports, presentation states and other
  non-image files are listed and skipped.
- **DICOM files without the DICOM file header** (older exports) are read as well.

No two series are ever written to the same file: all names are decided before
anything is written, and a name two series would share is reported as an error
naming both. A series dcm2niix cannot convert leaves nothing behind.

Output is a BIDS tree of `sub-*/[ses-*/]anat/` NIfTIs with JSON sidecars
(`EchoTime` in seconds), a `dataset_description.json`, a README and a
`.bidsignore`, ready for the pipeline.
