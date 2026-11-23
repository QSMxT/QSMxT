---
title: DICOM export
description: "What --export-dicom writes, how the series are identified, and how values are scaled."
---

The steps are in [Export results as DICOM](/QSMxT/how-to/export-dicom/). This page is what
the export writes.

## Exporting results as DICOM

`--export-dicom` writes a DICOM series for every final map alongside the NIfTIs:

```sh
qsmxt run study/bids --export-dicom
```

Each map gets its own folder under the subject's `extra_files/`, one file per
slice:

```
study/bids/derivatives/qsmxt/
└── sub-01/
    ├── anat/
    │   └── sub-01_…_Chimap.nii
    └── extra_files/
        └── sub-01_Chimap_dicoms/
            ├── sub-01_Chimap_0001.dcm
            ├── sub-01_Chimap_0002.dcm
            └── …
```

Every final map the run produced is exported. The magnitude and χ need no extra flag; the
supplementary maps are exported once you've asked for them:

| Map | Produced by | Folder suffix / `--dicom-outputs` token |
| --- | --- | --- |
| Magnitude (the echoes combined) | any run with magnitude images | `magnitude` |
| Susceptibility (χ) | the default pipeline | `Chimap` |
| SWI | `--do-swi` | `swi` |
| SWI minIP | `--do-swi` | `minIP` |
| SWI filtered phase | `--swi-filtered-phase` | `desc-swi_phase` |
| Paramagnetic SMWI | `--do-smwi` | `desc-paramagnetic_smwi` |
| Diamagnetic SMWI | `--do-smwi` | `desc-diamagnetic_smwi` |
| Paramagnetic SMWI minIP | `--do-smwi` | `desc-paramagneticsmwi_minIP` |
| Diamagnetic SMWI minIP | `--do-smwi` | `desc-diamagneticsmwi_minIP` |
| T2\* | `--do-t2starmap` | `T2starmap` |
| R2\* | `--do-r2starmap` | `R2starmap` |
| R2 | `--do-r2map` | `R2map` |
| R2′ | `--do-r2primemap` | `R2primemap` |
| Paramagnetic χ | `--do-chisep` | `desc-paramagnetic_Chimap` |
| Diamagnetic χ | `--do-chisep` | `desc-diamagnetic_Chimap` |
| Total χ | `--do-chisep` | `desc-total_Chimap` |
| Segmentation (a DICOM SEG, below) | `--do-segmentation`, `--use-custom-dseg` | `dseg` |

To export only some of them, pass those tokens to `--dicom-outputs`
(case-insensitive), comma-separated or by repeating the flag; a name that is not
in the table is refused:

```sh
qsmxt run study/bids --do-chisep --export-dicom \
  --dicom-outputs chimap,desc-paramagnetic_chimap --dicom-outputs desc-diamagnetic_chimap
```

In the TUI, the Reconstruction tab's DICOM section lists the maps the settings
make, each ticked; untick the ones to leave out.

The [multi-orientation](/QSMxT/reference/multi-orientation/#multi-orientation-cosmos-and-sti)
reconstructions stay NIfTI-only.

## The segmentation as a DICOM SEG

The
[segmentation](/QSMxT/reference/referencing-and-stats/#segmentation-and-per-structure-statistics)
is written as one DICOM Segmentation object (`sub-01_dseg_dicoms/sub-01_dseg.dcm`):
each structure is a binary segment, drawn on a series exported with it on the
same grid, so a viewer overlays it there (OHIF, Weasis 4.3 or later, and 3D Slicer
with the QuantitativeReporting extension; check your PACS's DICOM conformance
statement for *Segmentation Storage*). The series is the first of these the export
writes: the magnitude, χ, SWI, T2\*, R2\*, R2, R2′, the χ-separation maps, SMWI (not
a minIP, which is a projection); the log names it. A `--dicom-outputs` that selects
`dseg` and none of those the run makes is refused before the run.
SynthSeg's structures are coded with the standard's codes for brain structures
(DICOM PS3.16 CID 7153 and CID 7140), with left and right as modifiers, so a viewer
can name them in any language. A supplied segmentation (`--use-custom-dseg`) need not
use FreeSurfer's numbering, so its labels are coded only as intracranial structures
and named from its `dseg.tsv`.

## Filing them beside the source study

By default the derived series carry a synthesised identity: patient fields are
left empty and a stable `StudyInstanceUID` is derived from the subject and
session. That's fine for a local viewer, but a PACS will file the maps as a study
of their own. Point `--source-dicom` at the original DICOMs and QSMxT inherits the
identity from the first readable DICOM it finds there — `PatientID`,
`StudyInstanceUID`, `FrameOfReferenceUID`, study date/time, accession number — so
the maps land beside the acquisition they came from:

```sh
qsmxt run study/bids --export-dicom --source-dicom /data/dicoms/sub-01
```

Without the original DICOMs to hand, a dataset converted with
[`qsmxt dicom-convert --keep-dicom-uids`](/QSMxT/reference/dicom-conversion/#keeping-the-studys-dicom-uids) does
nearly as well: its sidecars keep the study's `StudyInstanceUID` and
`FrameOfReferenceUID`, so the exported series file into the original study and
line up with its images, though with no patient fields. `--source-dicom` still
wins where it is given.

`SeriesInstanceUID` and `SOPInstanceUID` are always newly generated: these are
distinct derived series, never a rewrite of the originals. Every instance is
tagged `DERIVED\SECONDARY`, and each map gets a stable `SeriesNumber` so viewers
list them in a predictable order.

:::caution[Values are rescaled to integers]
DICOM pixel data is integer, so each volume is linearly rescaled into unsigned
16-bit with `RescaleSlope` and `RescaleIntercept` set to match. A viewer that
honours those tags recovers the true values (χ in ppm, R2\* in s⁻¹); one that
ignores them shows raw 0–65535 counts. The scaling is fitted per volume, so it is
not comparable across subjects — **for quantitative analysis use the NIfTIs**.
:::

## Geometry, encoding and failures

- Slice geometry (`ImageOrientationPatient`, `ImagePositionPatient`,
  `SliceLocation`, spacing) is read from the output NIfTI's affine, so the series
  overlays the source acquisition in a viewer and reflects any
  [oblique resampling](/QSMxT/background/geometry/#oblique-acquisitions) the run
  applied.
- Instances are uncompressed *MR Image Storage*, Explicit VR Little Endian. The acquisition's
  patient position, repetition time, echo train length, body part and laterality
  are filled in too: from `--source-dicom` when given, else from the run's BIDS
  sidecar (`PatientPosition`, `RepetitionTime`, `EchoTrainLength`, `BodyPart`,
  as dcm2niix and `qsmxt dicom-convert` write them), else left empty rather than
  guessed. The segmentation is a *Segmentation Storage* object.
- Export is best-effort: a failure is logged and the run still succeeds.
- `--export-dicom` is a pipeline setting, so it can live in a
  [config file](/QSMxT/reference/configuration/) as `export_dicom = true` under
  `[pipeline]`, and it carries through to [`qsmxt slurm`](/QSMxT/how-to/slurm/).
  `--source-dicom` and `--dicom-outputs` are flags on `qsmxt run` itself.
