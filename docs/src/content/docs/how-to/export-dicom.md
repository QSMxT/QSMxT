---
title: Export results as DICOM
description: "Write QSMxT results back as DICOM series for PACS and viewers."
---

**Goal:** the maps as DICOM series, to open in a clinical viewer or send to a PACS beside the
original scan.

## Steps

1. Run with `--export-dicom`, pointing `--source-dicom` at the subject's original DICOMs so the
   new series file into the same study:

   ```sh
   qsmxt run study/bids --export-dicom --source-dicom /data/dicoms/sub-01
   ```

   Without `--source-dicom` the series still export, but a PACS files them as a study of their
   own.

2. Find the series under each subject's `extra_files/`:

   ```
   derivatives/qsmxt/sub-01/extra_files/sub-01_Chimap_dicoms/
   ```

3. Load that folder into your viewer or PACS.

In the TUI, tick **Export DICOM** in the Reconstruction tab's DICOM section, and choose the
source DICOM directory there.

## Check it worked

The χ series overlays the original images in the viewer. Values read in ppm in a viewer that
applies the DICOM rescale tags.

:::caution[Use the NIfTIs for numbers]
DICOM stores whole numbers, so each map is rescaled per volume. For quantitative analysis, use
the NIfTI files.
:::

## Options

- Every map the run made is exported. To export only some, list them:
  `--dicom-outputs chimap,swi`.
- Converted with [`qsmxt dicom-convert --keep-dicom-uids`](/QSMxT/how-to/convert-dicom/#keeping-the-studys-dicom-uids)? Then the series file into the
  original study even without `--source-dicom`.

What is exported, the names to use with `--dicom-outputs`, and how the series are identified
are in [Output files](/QSMxT/reference/outputs/#dicom-series).
