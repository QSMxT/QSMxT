---
title: "χ-separation toolbox · χ-sepnet"
description: "χ-separation toolbox 1.1.3's pipeline in QSMxT: qsmxt run --pipeline chi-sepnet."
sidebar:
  order: 8
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline chi-sepnet
```

In the TUI: the Pipeline tab, **χ-separation toolbox · χ-sepnet**.

**Based on:** The χ-separation toolbox's script (Chisep_script.m) with χ-sepnet · Kim et al. 2025.

**Parity:** No whole-pipeline parity test yet ([QSM-CI#470](https://github.com/QSMxT/QSM-CI/issues/470)).

## Use it when

You want paramagnetic and diamagnetic maps from the χ-separation toolbox's network, from 3 T gradient-echo data alone or with a measured R2′.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Geometry | Tukey window in k-space (`--tukey-window`) | 0.4, for scans not from Philips | 0 | The χ-separation toolbox Tukey-windows the complex echoes in k-space before everything else, with a taper ratio of 0.4; its comment sets 0 for Philips only. |
| Masking | Mask | `magnitude,bet:0.50` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | The χ-separation toolbox and χ-sepnet mask with BET. (note 1) |
|  | Orientation (`--bet-orientation`) | As stored | As FSL reads it | The χ-separation toolbox masks with MEDI's BET.m, which hands the bet2 mex the MATLAB array, so bet2 sees the voxels as stored; FSL's own bet reverses x of a neurological file first. |
|  | Inhomogeneity correction (`--no-inhomogeneity-correction`) | off | on | The χ-separation toolbox runs BET on the magnitude as read, with no bias-field correction. |
| Field map | Phase linearity (`--no-romeo-phase-linearity`) | off | on | The χ-separation toolbox runs ROMEO's command line with a magnitude and its default weights, which leave out phase linearity. |
|  | Echo weighting (`--b0-weight-type`) | Assumed decay, for scans with two or more echoes | Phase SNR | The toolbox weights the echoes by TE·exp(−TE/T2\*) with T2\* = 40 ms. |
| χ-separation | Compute χ-separation (`--do-chisep`) | on | off | A χ-separation method runs with χ-separation on. |
|  | χ-separation method (`--chisep`) | χ-sepnet | R2\*-QSM | One of the choices that start this pipeline. |

Notes on the table:

1. The χ-separation toolbox runs MEDI's BET on the root-sum-of-squares magnitude over echoes. The χ-separation toolbox uses BET's mask as it is, without eroding it.

## Intentional differences from the original

None.

## Known gaps

None known.

## Citations

- Kim, M., et al. (2025). "χ-sepnet: Deep neural network for magnetic susceptibility source separation." *Human Brain Mapping*, 46(2):e70136. https://doi.org/10.1002/hbm.70136
- Shin, H.-G., et al. (2021). "χ-separation: Magnetic susceptibility source separation toward iron and myelin mapping in the brain." *NeuroImage*, 240:118371. https://doi.org/10.1016/j.neuroimage.2021.118371
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
