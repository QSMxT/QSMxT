---
title: MESE and R2′
description: "Multi-echo spin-echo scans, for R2, R2′ and source separation."
---

A multi-echo spin-echo (MESE) scan of the same session gives R2, and with the GRE's R2\* it
gives R2′ = R2\* − R2. Most source-separation methods need R2′.

## What QSMxT does automatically

- A MESE named `*_echo-<N>_MESE.nii.gz` in the same subject and session's `anat/` folder is
  used for every GRE scan there. `qsmxt validate` lists it.
- It does not need the GRE's grid: a 2D stack or a smaller field of view is resampled onto the
  GRE's grid.
- R2 and R2′ are reported only where the MESE reached; elsewhere they are 0, and
  `desc-r2prime_mask` shows where R2′ is a measurement.
- Without a MESE, R2′ is predicted from R2\* by R2PRIMEnet (3 T only), and the R2′ map's
  sidecar says it was predicted.

## What it doesn't do / limits

- `qsmxt dicom-convert` does not name spin-echo scans: the MESE ends up in `extra_data/`.
- R2 needs at least three echoes, equally spaced, with the first at the echo spacing. Other
  MESE scans are refused or ignored.
- Source separation in the voxels the MESE did not reach is empty.

## What you can do

1. Move the MESE's echoes from `extra_data/` into `anat/`, named
   `sub-01_echo-1_MESE.nii.gz`, `sub-01_echo-2_MESE.nii.gz`, … with their `.json` sidecars.
2. Check with `qsmxt validate study/bids`: the run shows `MESE (spin-echo): present`.
3. Run with source separation: see
   [Separate paramagnetic and diamagnetic χ](/QSMxT/how-to/chi-separation/).

Have an R2 or R2′ map from another tool? Supply it instead
([Non-BIDS or partial data](/QSMxT/data/non-bids/)).

More: [Where R2′ comes from](/QSMxT/reference/source-separation/#where-r2-comes-from).
