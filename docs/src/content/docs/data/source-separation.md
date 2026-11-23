---
title: Source separation and MESE
description: "Paramagnetic and diamagnetic χ, and multi-echo spin-echo scans for R2′."
---

Source separation splits susceptibility into its paramagnetic (for example iron) and
diamagnetic (for example myelin) parts. Most methods need R2′ = R2\* − R2, where R2 comes from a
multi-echo spin-echo (MESE) scan of the same session.

## What QSMxT does automatically

- A MESE named `*_echo-<N>_MESE.nii.gz` in the same subject and session's `anat/` folder is
  used for every GRE scan there. `qsmxt validate` lists it as `MESE (spin-echo): present`.
- The MESE does not need the GRE's grid: a 2D stack or a smaller field of view is resampled onto
  it.
- R2 and R2′ are reported only where the MESE reached; elsewhere they are 0, and
  `desc-r2prime_mask` shows where R2′ is a measurement.
- Without a MESE, R2′ is predicted from R2\* by R2PRIMEnet (3 T only), and the R2′ map's sidecar
  says it was predicted.

## What it doesn't do / limits

- `qsmxt dicom-convert` does not name spin-echo scans yet: the MESE ends up in `extra_data/`.
  Recognising it automatically is tracked in
  [QSMxT#624](https://github.com/QSMxT/QSMxT/issues/624).
- R2 needs at least three echoes, equally spaced, with the first at the echo spacing.
- Only `r2star-qsm` (the default) and `decompose` work from the GRE alone. The other methods
  need R2′, and are empty where the MESE did not reach.
- Away from 3 T, R2PRIMEnet is not used: see [7 T and low field](/QSMxT/data/field-strength/).

## What you can do

1. If you have a MESE, move its echoes from `extra_data/` into `anat/`, named
   `sub-01_echo-1_MESE.nii.gz`, `sub-01_echo-2_MESE.nii.gz`, … with their `.json` sidecars, and
   check with `qsmxt validate study/bids`.
2. Run with separation turned on:

   ```sh
   qsmxt run study/bids --do-chisep
   ```

   This uses R2\*-QSM, which needs only the GRE. Choose another method with `--chisep`, for
   example `--chisep chi-sep-ilsqr`. In the TUI: **Reconstruction › QSM › χ-separation**.
3. Each scan gets `desc-paramagnetic_Chimap`, `desc-diamagnetic_Chimap` and
   `desc-total_Chimap` in `anat/`. Iron-rich deep grey matter stands out in the paramagnetic map.

To reproduce the χ-separation toolbox or DECOMPOSE exactly, start from its
[toolbox pipeline](/QSMxT/toolbox-pipelines/). Have an R2 or R2′ map from another tool? Supply
it instead ([Non-BIDS or partial data](/QSMxT/data/non-bids/)).

Every method, what it needs, and where R2′ comes from:
[Source separation](/QSMxT/reference/source-separation/).
