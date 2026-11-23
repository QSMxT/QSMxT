---
title: Look at your results
description: "Where the susceptibility maps are, and how to check them."
---

**Goal:** find your susceptibility maps and check that they look right.

## Steps

1. Open the output folder. For each scan:

   ```
   study/bids/derivatives/qsmxt/
   ├── sub-01/anat/
   │   ├── sub-01_…_Chimap.nii            ← the susceptibility map, in ppm
   │   ├── sub-01_…_mask.nii              ← the brain mask
   │   ├── sub-01_…_desc-qsm_mask.nii     ← where the map has values
   │   └── sub-01_…_part-mag_T2starw.nii  ← the combined magnitude
   ├── methods.md                         ← what was done, with citations
   └── qsmxt.log
   ```

2. Open the `Chimap.nii` in any NIfTI viewer (FSLeyes, ITK-SNAP, MRIcroGL, …), with a window
   of about −0.1 to 0.1 ppm.
3. Overlay the `mask.nii` on the magnitude to check that it covers the brain.

## Check it worked

- Iron-rich deep grey matter (globus pallidus, putamen, red nucleus, substantia nigra) and
  veins are **bright** (paramagnetic, positive χ).
- White matter is slightly **dark** (diamagnetic).
- No smooth shading across the brain and no sharp bands at the brain's edge: those are
  background field that was not removed, often from a mask that is too large.

If the deep grey matter is dark, the phase has the opposite sign to what QSMxT assumed; see
[GE and real/imaginary data](/QSMxT/data/ge-real-imaginary/).

A 0 in the `Chimap` outside `desc-qsm_mask` means "not reconstructed", not χ = 0: the map is
undefined in the thin rim the background removal takes off the edge of the mask.

## Other outputs

Other maps appear only when you ask for them: SWI (`--do-swi`), T2\* and R2\*
(`--do-t2starmap`, `--do-r2starmap`), paramagnetic and diamagnetic χ ([`--do-chisep`](/QSMxT/data/source-separation/)), a
segmentation and per-structure statistics ([below](#measure-χ-in-brain-structures)). Every file and
its name is in [Output files](/QSMxT/reference/outputs/).

## Measure χ in brain structures

For a table of mean χ (and the other maps) in each brain structure, ready for a group
analysis:

1. Run with segmentation and statistics turned on:

   ```sh
   qsmxt run study/bids --do-segmentation --do-analysis
   ```

   The segmentation (SynthSeg) is made from the GRE magnitude, so no T1-weighted scan is
   needed.

2. To compare values between subjects, reference χ to a structure rather than to the mean of
   the brain, for example:

   ```sh
   qsmxt run study/bids --do-segmentation --do-analysis --qsm-reference ventricles
   ```

3. Open `derivatives/qsmxt/desc-segmentation_stats.tsv`: one row per scan, map and structure,
   with the mean, SD, median and percentiles.

In the TUI: **Reconstruction › Other outputs › Segmentation**, and **QSM › Reference**.

Check that each scan's `*_dseg.nii` sits on the anatomy when overlaid on the magnitude. The table
covers every map the run made (R2\*, the χ-separation maps, …). SynthSeg is trained on human
brains ([non-human data](/QSMxT/data/non-human/)); to use your own parcellation, see
[Non-BIDS or partial data](/QSMxT/data/non-bids/). The regions you can name are in
[How the pipeline works](/QSMxT/background/pipeline/#naming-a-region), and the table's columns
in [Output files](/QSMxT/reference/outputs/#segmentation-and-per-structure-statistics).

## If something looks wrong

- **Streaks around a bleed, calcification or implant:** run again with `--two-pass`
  (TUI: Reconstruction › Masking); see [Mask recipes](/QSMxT/reference/masking/#two-pass-modes).
- **The mask cuts off brain, or includes skull:** try another `--mask-preset`; see
  [Mask recipes](/QSMxT/reference/masking/).
- **A scan has artefacts only on some slices, or looks stretched:** see
  [2D acquisitions and slice gaps](/QSMxT/data/2d-slice-gaps/) and
  [Oblique slices](/QSMxT/data/oblique/).

**Next:** [Cite QSMxT and write the methods](/QSMxT/how-to/cite/).
