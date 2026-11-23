---
title: "χ-separation toolbox · MEDI"
description: "χ-separation toolbox 1.1.3's pipeline in QSMxT: qsmxt run --pipeline chi-sep-medi."
sidebar:
  order: 7
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline chi-sep-medi
```

In the TUI: the Pipeline tab, **χ-separation toolbox · MEDI**.

**Based on:** The χ-separation toolbox's script (Chisep_script.m) with MEDI · Shin et al. 2021.

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-chisep-medi.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-chisep-medi) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want paramagnetic and diamagnetic maps comparable with the χ-separation toolbox's script, solved with MEDI, and have an R2′ map or a multi-echo spin-echo scan.

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
| Reference | QSM reference (`--qsm-reference`) | None | Mean | The χ-separation toolbox's script uses its iLSQR QSM as QSM_iLSQR returns it, without referencing it. |
| χ-separation | Compute χ-separation (`--do-chisep`) | on | off | A χ-separation method runs with χ-separation on. |
|  | χ-separation method (`--chisep`) | χ-separation (MEDI) | R2\*-QSM | One of the choices that start this pipeline. |
|  | CSF mask's R2\* (`--chi-sep-medi-csf-r2star`) | NNLS | ARLO | The χ-separation toolbox's script finds its CSF mask from an R2\* fitted by NNLS; with the echo times written as the script documents them, it never uses ARLO. |
| Relaxometry | R2' source (`--r2prime-strategy`) | MESE | Automatic | The χ-separation toolbox takes a measured R2′, from a multi-echo spin-echo scan or a supplied map. |

Notes on the table:

1. The χ-separation toolbox runs MEDI's BET on the root-sum-of-squares magnitude over echoes. The χ-separation toolbox uses BET's mask as it is, without eroding it.

## Intentional differences from the original

- **The QSM that the separation starts from is converted with the same centre frequency as the separation itself.** The toolbox's script gives that QSM step (STI Suite's iLSQR) a field strength typed separately from its centre frequency, which can be the nominal one; one value keeps the two steps consistent.

## Known gaps

None known.

## Citations

- Shin, H.-G., et al. (2021). "χ-separation: Magnetic susceptibility source separation toward iron and myelin mapping in the brain." *NeuroImage*, 240:118371. https://doi.org/10.1016/j.neuroimage.2021.118371
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
