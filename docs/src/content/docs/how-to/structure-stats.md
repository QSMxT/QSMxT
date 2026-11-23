---
title: Measure χ in brain structures
description: "Segment the brain and get mean χ (and other maps) per structure."
---

**Goal:** a table of mean susceptibility in each brain structure, for every scan, ready for a
group analysis.

## Steps

1. Run with segmentation and statistics turned on:

   ```sh
   qsmxt run study/bids --do-segmentation --do-analysis
   ```

   The segmentation (SynthSeg) is made from the GRE magnitude itself, so no T1-weighted scan
   or registration is needed.

2. To compare values between subjects, reference χ to a structure rather than to the mean of
   the brain, for example:

   ```sh
   qsmxt run study/bids --do-segmentation --do-analysis --qsm-reference ventricles
   ```

3. Open `derivatives/qsmxt/desc-segmentation_stats.tsv`: one row per scan, map and structure,
   with the mean, SD, median and percentiles.

In the TUI: **Reconstruction › Other outputs › Segmentation** for the segmentation and
statistics, and **QSM › Reference** for the reference.

## Check it worked

- Each scan has a `*_dseg.nii`. Overlay it on the magnitude to check that the structures sit
  where they should.
- `desc-<map>_stats.png` at the top of the output is a bar chart of each structure's mean
  across scans.

## Notes

- The table covers every map the run made (χ, R2\*, the χ-separation maps, …), so turn on the
  others you want too.
- SynthSeg is trained on human brains: see
  [Rodent and other non-human data](/QSMxT/data/non-human/).
- To use your own parcellation instead, see [Non-BIDS or partial data](/QSMxT/data/non-bids/).

Every reference you can name, and every column of the table, is in
[Referencing and per-structure statistics](/QSMxT/reference/referencing-and-stats/).
