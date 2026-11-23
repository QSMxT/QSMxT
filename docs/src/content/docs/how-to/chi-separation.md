---
title: Separate paramagnetic and diamagnetic χ
description: "Split susceptibility into its paramagnetic and diamagnetic parts."
---

**Goal:** separate maps of paramagnetic (for example iron) and diamagnetic (for example
myelin) susceptibility.

## Steps

1. Check what your data allows:

   ```sh
   qsmxt validate study/bids
   ```

   Multi-echo GRE alone is enough for the default method. A multi-echo spin-echo (MESE) scan
   of the same session shows as `MESE (spin-echo): present` and allows the methods built on R2′.

2. Run with separation turned on:

   ```sh
   qsmxt run study/bids --do-chisep
   ```

   This uses R2\*-QSM, which needs only the GRE. To choose another method:

   ```sh
   qsmxt run study/bids --do-chisep --chisep chi-sep-ilsqr
   ```

In the TUI: **Reconstruction › QSM › χ-separation**.

## Check it worked

Each scan has three new maps in `anat/`: `desc-paramagnetic_Chimap`, `desc-diamagnetic_Chimap`
and `desc-total_Chimap`. Iron-rich deep grey matter stands out in the paramagnetic map.

## If your data is different

- **No MESE:** only `r2star-qsm` and `decompose` use the GRE alone. The others predict R2′
  from R2\* with a network trained at 3 T only. See
  [MESE and R2′](/QSMxT/data/mese-r2prime/).
- **Not 3 T:** see [7 T and low field](/QSMxT/data/field-strength/).
- **Reproducing the χ-separation toolbox or DECOMPOSE exactly:** start from its
  [toolbox pipeline](/QSMxT/toolbox-pipelines/).

Every method, what it needs and its settings are in
[Susceptibility source separation](/QSMxT/reference/source-separation/).
