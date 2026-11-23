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

In the TUI: the Pipeline tab, **DECOMPOSE**. Taken from DECOMPOSE (Chen et al. 2021 NeuroImage 242:118477 (DECOMPOSE), §2.2, §3.6, §4.1).

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-decompose.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-decompose) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want paramagnetic and diamagnetic maps from multi-echo gradient-echo data alone, with DECOMPOSE as its paper describes it.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude-first,bet:0.50` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | DECOMPOSE's pipeline masks with FSL BET. DECOMPOSE's authors run BET on the first echo's GRE magnitude. DECOMPOSE's authors extract the brain with BET; no erosion of its mask is described. |
|  | Inhomogeneity correction (`--no-inhomogeneity-correction`) | off | on | DECOMPOSE's authors run BET on the first echo's magnitude; no bias-field correction is described. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | DECOMPOSE's pipeline uses Laplacian unwrapping. STI Suite's QSM_GUI unwraps each echo with MRPhaseUnwrap (Laplacian). |
|  | Laplacian solver (`--laplacian-solver`) | FFT | DCT | MRPhaseUnwrap is the FFT Schofield-Zhu solver with 12 voxels of padding. |
|  | Laplacian FFT padding (`--laplacian-fft-pad`) | 12 voxels | 64 voxels | MRPhaseUnwrap is the FFT Schofield-Zhu solver with 12 voxels of padding. |
|  | Laplacian unwrapping of the raw phase (`--laplacian-unmasked`) | on | off | STI Suite's QSM_GUI gives MRPhaseUnwrap the raw wrapped phase, not the phase masked first. |
|  | Per-echo QSM (`--qsm-per-echo`) | on | off | One of the choices that start this pipeline. |
| Background | Polish mask (`--bf-polish-mask`) | on | off | STI Suite's QSM_GUI polishes V-SHARP's mask (largest connected component, holes filled) and inverts the field inside it. |
| Inversion | QSM inversion (`--qsm-algorithm`) | STAR-QSM | iLSQR | DECOMPOSE's χ of each echo was reconstructed with STAR-QSM. |
|  | Padding (`--star-pad`) | 0 voxels | 8 8 20 voxels | DECOMPOSE's paper reconstructs each echo "with functions in STI Suite V3.0" and gives no parameters, so QSM_star runs as the function does without 'padsize': no padding (not the GUI's [8 8 20]). |
| χ-separation | Compute χ-separation (`--do-chisep`) | on | off | A χ-separation method runs with χ-separation on. |
|  | χ-separation method (`--chisep`) | DECOMPOSE-QSM | R2\*-QSM | One of the choices that start this pipeline. |
|  | DECOMPOSE fit (`--decompose-fit`) | Paper | Reference code | DECOMPOSE's paper fits the amplitudes with C+ + C− + C₀ = 1 and the amplitudes and R2\*₀ by linear least squares (lsqlin), χ± by lsqnonlin; the data are scaled to S(0) = 1 per voxel (QSM.rs's reading: the paper does not say how). |

## Intentional differences from the original

- **The steps before the fit follow the paper (Laplacian unwrapping, V-SHARP, STAR-QSM of each echo) at the functions' own settings, without the extra settings of the authors' analysis script.** The paper is the method as published and gives no settings for these steps; the authors' analysis script, which pads STAR-QSM by 12 voxels and runs V-SHARP with a 25 mm kernel on each echo, is a separate choice of settings.
- **Phase is converted to field in ppm with the scanner's centre frequency (ImagingFrequency) where the scan's metadata records it, and with the nominal field strength only where it does not.** The centre frequency is the field the scanner actually ran at, while the nominal field strength the toolboxes are usually given (STI Suite, QSMART, UK Biobank's script) can be several per cent off: Siemens' 3 T systems run at about 2.89 T, and every susceptibility value scales with it.

## Known gaps

None known.

## Citations

- Chen, J., et al. (2021). "Decompose quantitative susceptibility mapping (QSM) to sub-voxel diamagnetic and paramagnetic components based on gradient-echo MRI data." *NeuroImage*, 242:118477. https://doi.org/10.1016/j.neuroimage.2021.118477
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
