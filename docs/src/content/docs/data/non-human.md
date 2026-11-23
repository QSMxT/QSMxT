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
- **Background removal** with V-SHARP (the default) uses kernels of 2 to 12 mm on a 1 mm
  grid. In a mouse brain that is most of the brain.
- **Segmentation** (SynthSeg), and so per-structure statistics and a region reference, is
  trained on human brains only.
- **Deep-learning inversions** were trained on human data.
- QSMxT does not scale any of these for you.

## What you can do

- Mask with RS2-Net, made for rodent brains: `--mask-preset rs2-net` (needs a deep-learning
  build). Or with BET, presenting the image at human scale: `--mask-preset bet
  --bet-voxel-scale 10` (10 is usual for mouse).
- Size V-SHARP to the brain: `--vsharp-grid native`, with `--vsharp-max-radius` and
  `--vsharp-min-radius` in millimetres suited to it. Or use a background method without a
  fixed radius, such as PDF (`--bf-algorithm pdf`).
- Use a classical inversion (the default, iLSQR, is one).
- Reference χ to the mean (the default) or supply your own parcellation
  ([Non-BIDS or partial data](/QSMxT/data/non-bids/)).

Mask options are in [Mask recipes](/QSMxT/reference/masking/); background-removal settings
in [Settings](/QSMxT/reference/configuration/).
