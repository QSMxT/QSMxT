---
title: Convert DICOM to BIDS
description: "Turn a folder of DICOMs into a BIDS dataset QSMxT can run on."
---

## Converting DICOMs to BIDS

QSMxT needs [BIDS](https://bids.neuroimaging.io/)-formatted input. The
`dicom-convert` command builds it for you from a directory of DICOMs, applying an
automatic best-guess classification to every series with no prompts — ideal for
scripting and batch jobs.

```sh
qsmxt dicom-convert <DICOM_DIR> <OUTPUT_DIR>
```

- `<DICOM_DIR>` — input directory, searched recursively
- `<OUTPUT_DIR>` — output BIDS directory

:::tip[Want to review as you go?]
For a new dataset, converting [interactively](/QSMxT/background/tui/)
in the TUI lets you **inspect the automatic classification and relabel anything it
got wrong** before writing. The command above is the same conversion, unattended.
:::

### Preview first with `--dry-run`

Before writing anything, inspect how QSMxT classifies your data:

```sh
qsmxt dicom-convert /data/dicoms study/bids --dry-run
```

Example output:

```
Detected 6 unique series (auto-classified):
  acq-SWI3mm   SWI_3mm   [Magnitude] 2×TEs=[9.4…19.7]ms (uncombined, 32 coils) -> sub-01_acq-SWI3mm_rec-uncombined_coil-<NN>_echo-<N>_part-mag_MEGRE
  acq-SWI3mm   SWI_3mm   [Magnitude] 2×TEs=[9.4…19.7]ms -> sub-01_acq-SWI3mm_echo-<N>_part-mag_MEGRE
  acq-SWI3mm   SWI_3mm   [Magnitude] 2×TEs=[9.4…19.7]ms -> sub-01_acq-SWI3mm_rec-filtered_echo-<N>_part-mag_MEGRE
  acq-SWI3mm   SWI_3mm   [Phase]     2×TEs=[9.4…19.7]ms (uncombined, 32 coils) -> sub-01_acq-SWI3mm_rec-uncombined_coil-<NN>_echo-<N>_part-phase_MEGRE
  acq-SWI3mm   SWI_3mm   [Phase]     2×TEs=[9.4…19.7]ms -> sub-01_acq-SWI3mm_echo-<N>_part-phase_MEGRE
  acq-t1scout  t1_scout  [Extra] TE=2.0ms -> extra_data/1_t1scout
```

Each row is a unique series, deduplicated across subjects, so you review a
classification once even if many subjects share it. The arrow shows the name it
will get. Warnings follow the list: an incomplete export (missing slices, empty
files, magnitude and phase with different numbers of images), a series whose
files name different patients, and two original series that look like the same
images.

### Keeping the study's DICOM UIDs

By default the conversion is fully de-identified (dcm2niix's `-ba y`): the
sidecars carry the acquisition parameters but no patient fields, dates or DICOM
UIDs. `--keep-dicom-uids` keeps the study's `StudyInstanceUID`,
`SeriesInstanceUID`, `StudyID` and dates (dcm2niix's `-ba o`, which still strips
the patient's name, ID, birth date and the like) and adds the
`FrameOfReferenceUID`, under the DICOM keywords as BIDS recommends for metadata it
does not define:

```sh
qsmxt dicom-convert /data/dicoms study/bids --keep-dicom-uids
```

A later [DICOM export](/QSMxT/how-to/export-dicom/#exporting-results-as-dicom) then files its series into
the original study in a PACS, without `--source-dicom`.

:::caution[Not for sharing]
The UIDs name no one, but anyone holding the original study can link such a
dataset back to it, and through it to the patient. Keep a dataset converted with
`--keep-dicom-uids` inside your site; convert again without it to share.
:::

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
`.bidsignore`, ready for the pipeline. Already have NIfTIs? You can assemble a BIDS
dataset by hand — see [Input data](/QSMxT/reference/inputs/) for the expected
layout, and run [`qsmxt validate`](/QSMxT/reference/inputs/#checking-a-dataset)
to check it's pipeline-ready.

## What `qsmxt dicom-convert` writes

| Series | Name in `sub-*/[ses-*/]anat/` |
| --- | --- |
| Gradient-echo magnitude / phase / real / imaginary, one echo | `*_part-mag_T2starw` (`part-phase`, `part-real`, `part-imag`) |
| The same, several echoes | `*_echo-<N>_part-mag_MEGRE` |
| T1-weighted structural (MPRAGE, SPACE/TFE T1, MP2RAGE `UNI`) | `*_T1w` |
| MP2RAGE inversion images | `*_inv-<N>_MP2RAGE` |
| Scanner susceptibility map | `*_rec-scanner_Chimap` |
| Scanner T2\* / R2\* map | `*_rec-scanner_T2starmap`, `*_rec-scanner_R2starmap` |
| Scanner T1 map | `*_T1map` |
| Scanner SWI | `*_rec-swi_part-mag_T2starw` |
| Scanner minimum-intensity projection | `*_rec-mip_T2starw` |
| Another reconstruction of the same images | `*_rec-<label>_…`, e.g. `rec-filtered`, `rec-scanner` |

The plain original image of each kind is the one without a `rec-` label; that is
the one `qsmxt run` uses. The magnitude follows the phase: when the phase comes
from a particular reconstruction (ASPIRE's coil combination, say), the magnitude
from that reconstruction is the main one, and the scanner's own combination is
`rec-scanner`. A scanner map without a protocol name (Bruker's parametric maps) is
filed with the acquisition it was made from. A map made from a series that
is kept in `extra_data/` goes there too. When dcm2niix cannot separate the
echoes of a series into files, QSMxT converts each echo time separately, or, when
the echo tags are the same for every image, each volume (`rec-…volN`). Phase
sidecars get the `Units` BIDS requires: `rad` when the phase is already in
radians (as dcm2niix writes Philips phase), otherwise `arbitrary`. Any other series (scouts, reformats, FLAIR, diffusion,
Dixon, other vendor maps) is kept in `sub-*/[ses-*/]extra_data/` as dcm2niix
names it. These are not BIDS files, and `.bidsignore` lists `extra_data`. The
`.bidsignore` also lists the MP2RAGE inversion images, because BIDS requires
sidecar fields for them (such as `NumberShots`) that DICOM headers do not carry.
