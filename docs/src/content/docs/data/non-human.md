---
title: Rodent and other non-human data
description: "Preclinical, animal and ex vivo data."
---

QSMxT runs on any gradient-echo data, but several defaults are sized for the human brain in
millimetres.

## What QSMxT does automatically

- The default mask (a threshold of the phase quality) does not assume a human brain.
- The dipole inversion uses your voxel size and field direction, whatever the species.
- `qsmxt dicom-convert` reads Bruker DICOM exports, including their parametric maps.

## What it doesn't do / limits

- **Brain extraction** with BET, HD-BET and MindGrab expects a human brain.
- **Background removal** with V-SHARP (the default) uses kernels sized for human brains, so on
  a small brain it may leave background field behind.
- **Segmentation** (SynthSeg), and so per-structure statistics and a region reference, is
  trained on human brains only.
- **Deep-learning inversions** were trained on human data.
- QSMxT does not scale any of these for you.

## What you can do

- Mask with RS2-Net, made for rodent brains: `--mask-preset rs2-net` (needs a deep-learning
  build). Or with BET, presenting the image at human scale: `--mask-preset bet
  --bet-voxel-scale 10` (10 is usual for mouse).
- Check the background removal: look at the local field (`derivatives/qsmxt/workflow/sub-*/bgremove/*_localfield.nii`) or
  the χ map for smooth shading across the brain. If background remains, try PDF
  (`--bf-algorithm pdf`).
- Use a classical inversion (the default, iLSQR, is one).
- Reference χ to the mean (the default) or supply your own parcellation
  ([Non-BIDS or partial data](/QSMxT/data/non-bids/)).

Mask options are in [Mask recipes](/QSMxT/reference/masking/); background-removal settings
in [Settings](/QSMxT/reference/configuration/).
