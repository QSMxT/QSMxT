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

   Each line is one series and the name it will get, for example:

   ```
   acq-SWI3mm   SWI_3mm   [Magnitude] 2×TEs=[9.4…19.7]ms -> sub-01_acq-SWI3mm_echo-<N>_part-mag_MEGRE
   acq-SWI3mm   SWI_3mm   [Phase]     2×TEs=[9.4…19.7]ms -> sub-01_acq-SWI3mm_echo-<N>_part-phase_MEGRE
   acq-t1scout  t1_scout  [Extra] TE=2.0ms -> extra_data/1_t1scout
   ```

   Your gradient-echo scan should appear as `part-mag` and `part-phase` images (`MEGRE` for
   several echoes, `T2starw` for one). Anything else goes to `extra_data/`. Warnings after the
   list point out incomplete exports.

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
- [Source separation and MESE](/QSMxT/data/source-separation/): the conversion does not name a
  spin-echo scan for you.
- [NIfTI files, not DICOM](/QSMxT/data/non-bids/)

Every series type the conversion recognises is in
[How QSMxT reads your data](/QSMxT/background/input-handling/#what-it-handles), and the name each
gets in [Input data layout](/QSMxT/reference/inputs/#what-qsmxt-dicom-convert-writes).

## Keeping the study's DICOM UIDs

By default the conversion removes everything that identifies the patient or the study. If you
will send results back to a PACS, add `--keep-dicom-uids`. It keeps the study's UIDs and dates,
still without the patient's name, ID or birth date, so a later
[DICOM export](/QSMxT/how-to/export-dicom/) files its series into the original study:

```sh
qsmxt dicom-convert /data/dicoms study/bids --keep-dicom-uids
```

In the TUI, tick *Keep DICOM UIDs* above *Convert to BIDS*.

:::caution[Not for sharing]
The UIDs name no one, but anyone holding the original study can link such a
dataset back to it, and through it to the patient. Keep a dataset converted with
`--keep-dicom-uids` inside your site; convert again without it to share.
:::

**Next:** [Run QSM on your data](/QSMxT/how-to/run-qsm/).
