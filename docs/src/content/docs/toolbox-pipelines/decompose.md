---
title: "DECOMPOSE"
description: "DECOMPOSE's pipeline in QSMxT: qsmxt run --pipeline decompose."
sidebar:
  order: 9
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline decompose
```

In the TUI: the Pipeline tab, **DECOMPOSE**.

**Based on:** The DECOMPOSE paper · Chen et al. 2021.

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-decompose.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-decompose) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want paramagnetic and diamagnetic maps from multi-echo gradient-echo data alone, with DECOMPOSE as its paper describes it.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude-first,bet:0.50` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | DECOMPOSE's authors mask with FSL's brain extraction. (note 1) |
|  | Inhomogeneity correction (`--no-inhomogeneity-correction`) | off | on | DECOMPOSE's authors run BET on the first echo's magnitude; no bias-field correction is described. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | DECOMPOSE's pipeline uses Laplacian unwrapping. STI Suite unwraps each echo with its Laplacian unwrapping. |
|  | Laplacian solver (`--laplacian-solver`) | FFT | DCT | STI Suite's Laplacian unwrapping is FFT-based, with 12 voxels of padding. |
|  | Laplacian FFT padding (`--laplacian-fft-pad`) | 12 voxels | 64 voxels | The same reason as *Laplacian solver* above. |
|  | Laplacian unwrapping of the raw phase (`--laplacian-unmasked`) | on | off | STI Suite's Laplacian unwrapping runs on the raw wrapped phase, not on phase masked first. |
|  | Per-echo QSM (`--qsm-per-echo`) | on | off | DECOMPOSE's authors reconstruct a susceptibility map for each echo, and state that this is necessary. |
| Background | Polish mask (`--bf-polish-mask`) | on | off | STI Suite keeps the largest connected piece of V-SHARP's mask, with its holes filled, and inverts the field inside it. |
| Inversion | QSM inversion (`--qsm-algorithm`) | STAR-QSM | iLSQR | DECOMPOSE's χ of each echo was reconstructed with STAR-QSM. |
|  | Padding (`--star-pad`) | 0 voxels | 8 8 20 voxels | DECOMPOSE's paper gives no settings for STAR-QSM, so it runs with STAR-QSM's own default of no padding. |
| χ-separation | Compute χ-separation (`--do-chisep`) | on | off | A χ-separation method runs with χ-separation on. |
|  | χ-separation method (`--chisep`) | DECOMPOSE-QSM | R2\*-QSM | DECOMPOSE separates the sources by fitting a three-compartment model to the multi-echo signal. |
|  | DECOMPOSE fit (`--decompose-fit`) | Paper | Reference code | DECOMPOSE's paper fits the compartment amplitudes (summing to 1) and R2\*₀ by linear least squares, and χ+ and χ− by nonlinear least squares. |

Notes on the table:

1. DECOMPOSE's authors run BET on the first echo's GRE magnitude. DECOMPOSE's authors extract the brain with BET; no erosion of its mask is described.

## Intentional differences from the original

- **Where the original toolbox converts phase to ppm with the nominal field strength (such as 3 T), this pipeline uses the scanner's centre frequency (ImagingFrequency) whenever the scan's metadata records it.** The centre frequency is the field the scanner actually ran at, while the nominal value can be several per cent off (Siemens' 3 T systems run at about 2.89 T), and every susceptibility value scales with it.
- **The steps before the fit follow the paper (Laplacian unwrapping, V-SHARP, STAR-QSM of each echo) at the functions' own settings, without the extra settings of the authors' analysis script.** The paper is the method as published and gives no settings for these steps; the authors' analysis script, which pads STAR-QSM by 12 voxels and runs V-SHARP with a 25 mm kernel on each echo, is a separate choice of settings.

## Known gaps

None known.

## Citations

- Chen, J., et al. (2021). "Decompose quantitative susceptibility mapping (QSM) to sub-voxel diamagnetic and paramagnetic components based on gradient-echo MRI data." *NeuroImage*, 242:118477. https://doi.org/10.1016/j.neuroimage.2021.118477
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
