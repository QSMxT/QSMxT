---
title: Convert DICOM to BIDS
description: "Turn a folder of DICOMs into a BIDS dataset QSMxT can run on."
---

**Goal:** turn the DICOMs from your scanner into a [BIDS](https://bids.neuroimaging.io/)
dataset, which is what QSMxT runs on. This is the same for every scanner.

## Steps

1. Preview how QSMxT will name each series. Nothing is written yet:

   ```sh
   qsmxt dicom-convert /path/to/dicoms study/bids --dry-run
   ```

   Each line is one series and the name it will get. Your gradient-echo scan should appear
   as `part-mag` and `part-phase` images (`MEGRE` for several echoes, `T2starw` for one).
   Anything else goes to `extra_data/`.

2. Convert:

   ```sh
   qsmxt dicom-convert /path/to/dicoms study/bids
   ```

3. Check that QSMxT finds your scans:

   ```sh
   qsmxt validate study/bids
   ```

In the TUI, choose **Input mode → DICOM -> BIDS**, fill in the DICOM directory and the output
directory, check the series list (you can relabel a series there), and press
**Convert to BIDS**.

## Check it worked

`qsmxt validate` lists every run it found, with its echo times and field strength, and says
`QSM reconstruction: yes`. If a scan you expected is missing, it was probably classified as
something else: look for it in `sub-*/extra_data/`.

## If your data is different

Most data converts with no changes. These pages say what QSMxT does for particular data:

- [GE and real/imaginary data](/QSMxT/data/ge-real-imaginary/)
- [Uncombined coil data](/QSMxT/data/uncombined-coils/)
- [MESE for R2′ and source separation](/QSMxT/data/mese-r2prime/): the conversion does not name a
  spin-echo scan for you.
- [NIfTI files, not DICOM](/QSMxT/data/non-bids/)

Every series type the conversion recognises, and the name it gets, is in
[DICOM conversion](/QSMxT/reference/dicom-conversion/).

**Next:** [Run QSM](/QSMxT/how-to/run-qsm/).
