---
title: "STI Suite · iLSQR"
description: "STI Suite 3.0's pipeline in QSMxT: qsmxt run --pipeline sti-suite-ilsqr."
sidebar:
  order: 2
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline sti-suite-ilsqr
```

In the TUI: the Pipeline tab, **STI Suite · iLSQR**.

**Based on:** STI Suite's QSM_GUI, iLSQR path · Li et al. 2015.

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-sti.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-sti) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with STI Suite's QSM_GUI run with iLSQR.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude-last,threshold:sti-gui,largest-component,imfill` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | STI Suite's GUI masks the magnitude above the level its threshold tool starts at. (note 1) |
|  | Inhomogeneity correction (`--no-inhomogeneity-correction`) | off | on | STI Suite's GUI thresholds the magnitude as loaded, with no bias-field correction. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | STI Suite unwraps each echo with its Laplacian unwrapping. |
|  | Laplacian solver (`--laplacian-solver`) | FFT | DCT | STI Suite's Laplacian unwrapping is FFT-based, with 12 voxels of padding. |
|  | Laplacian FFT padding (`--laplacian-fft-pad`) | 12 voxels | 64 voxels | The same reason as *Laplacian solver* above. |
|  | Laplacian unwrapping of the raw phase (`--laplacian-unmasked`) | on | off | STI Suite's Laplacian unwrapping runs on the raw wrapped phase, not on phase masked first. |
|  | Phase offset removal (`--phase-offset-removal`) | off, for scans with two or more echoes | on | STI Suite unwraps each echo's phase as it is, with no phase-offset removal first. |
|  | Echo weighting (`--b0-weight-type`) | Echo time, for scans with two or more echoes | Phase SNR | Dividing the summed phase by the summed echo times weights each echo's field by its echo time. |
| Background | Polish mask (`--bf-polish-mask`) | on | off | STI Suite keeps the largest connected piece of V-SHARP's mask, with its holes filled, and inverts the field inside it. |
| Inversion | Maximum iterations (`--ilsqr-max-iter`) | 40 | 100 | STI Suite's GUI runs iLSQR for 40 iterations. |
|  | Precision (`--ilsqr-precision`) | Single | Double | STI Suite's iLSQR runs its least-squares solves in single precision. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | STI Suite keeps the inversion's susceptibility as it is, without referencing it. |

Notes on the table:

1. STI Suite's GUI thresholds the last echo's magnitude. STI Suite's GUI keeps the largest connected piece of the thresholded mask. STI Suite's GUI fills every hole in that piece. STI Suite's GUI does not erode its mask; V-SHARP erodes it later.

## Intentional differences from the original

- **Where the original toolbox converts phase to ppm with the nominal field strength (such as 3 T), this pipeline uses the scanner's centre frequency (ImagingFrequency) whenever the scan's metadata records it.** The centre frequency is the field the scanner actually ran at, while the nominal value can be several per cent off (Siemens' 3 T systems run at about 2.89 T), and every susceptibility value scales with it.
- **The B0 direction is taken from the scan's orientation.** STI Suite's GUI uses B0 along the slice axis unless its user types a direction in, and its manual asks for the scan's direction on oblique scans; on axial scans the two are the same.
- **V-SHARP's eroded mask is computed exactly, so it keeps a few isolated voxels inside the brain that STI Suite leaves out.** STI Suite leaves them out through rounding in its single-precision FFTs, which no other FFT matches and which does not depend on the data; on QSM-CI's in-vivo scan it was 4 of about 1.3 million voxels.

## Known gaps

None known.

## Citations

- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
