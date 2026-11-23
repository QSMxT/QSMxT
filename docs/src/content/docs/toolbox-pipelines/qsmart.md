---
title: "QSMART"
description: "QSMART's pipeline in QSMxT: qsmxt run --pipeline qsmart."
sidebar:
  order: 4
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline qsmart
```

In the TUI: the Pipeline tab, **QSMART**.

**Based on:** The QSMART toolbox (QSMART.m) · Yaghmaie et al. 2021.

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-qsmart.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-qsmart) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with QSMART's own pipeline, which treats the cortex next to the skull and the veins separately.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude-first,bet:0.20,min-intensity:100,majority,close:2` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | QSMART's authors mask with FSL's brain extraction on the bias-corrected first echo. (note 1) |
|  | Inhomogeneity correction method (`--inhomogeneity-method`) | N4 | makehomogeneous | QSMART corrects the first echo's magnitude with ANTs' N4 before brain extraction. |
|  | Image geometry (`--n4-geometry`) | make_nii | Scanner | QSMART gives N4 the magnitude with a simplified, axis-aligned image header, which slightly changes N4's correction. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | Laplacian unwrapping is not thrown off by the unreliable phase at the brain's edge, which QSMART keeps in its mask. |
|  | Laplacian solver (`--laplacian-solver`) | Mirror | DCT | QSMART's Laplacian unwrapping solves on a mirrored copy of the volume. |
|  | Laplacian unwrapping of the raw phase (`--laplacian-unmasked`) | on | off | QSMART unwraps each echo's raw phase over the whole volume, and its echo fit fits every voxel. |
|  | Phase offset removal (`--phase-offset-removal`) | off, for scans with two or more echoes | on | QSMART unwraps each echo's raw phase, with no phase-offset removal first. |
|  | B0 estimation (`--b0-estimation`) | QSMART echo fit | Weighted average | QSMART's echo fit gives the reliability map QSMART uses at the brain's edge, which it keeps in its mask. |
| Inversion | QSM inversion (`--qsm-algorithm`) | QSMART | iLSQR | QSMART inverts the total field in two stages, one for the brain's edge and veins and one for the rest. |
|  | Precision (`--ilsqr-precision`) | Single | Double | QSMART inverts with STI Suite's iLSQR, which runs its least-squares solves in single precision. |
|  | Offset adjustment grid (`--qsmart-offset-grid`) | QSMART's | FFT | QSMART's final offset adjustment builds its dipole kernel half a step off the FFT grid on odd matrix sizes. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | QSMART's result is its combined susceptibility map, which nothing references afterwards. |

Notes on the table:

1. QSMART runs brain extraction on the first echo's magnitude. It runs brain extraction with a fractional threshold of 0.2. It removes mask voxels whose bias-corrected magnitude is below 100. It smooths its mask with a 3×3×3 majority filter. It closes its mask with a ball of radius 2 voxels. It keeps the brain's edge rather than eroding it, and relies on Laplacian unwrapping and its echo fit to handle the unreliable phase there.

## Intentional differences from the original

- **Where the original toolbox converts phase to ppm with the nominal field strength (such as 3 T), this pipeline uses the scanner's centre frequency (ImagingFrequency) whenever the scan's metadata records it.** The centre frequency is the field the scanner actually ran at, while the nominal value can be several per cent off (Siemens' 3 T systems run at about 2.89 T), and every susceptibility value scales with it.
- **One gyromagnetic ratio is used throughout.** QSMART converts the field to rad/s with 2.675e8 rad/s/T and STI Suite's iLSQR inside it converts back with 2π·42.575 MHz/T, which lowers its susceptibility by 0.002 %; the mismatch is not part of the method.
- **Each echo's Laplacian-unwrapped phase has no arbitrary constant added.** QSMART's Laplacian unwrapping leaves each echo a small constant set by FFT rounding, which cannot be matched and is not part of the method; through the echo fit it moved the total field by 0.24 % on QSM-CI's in-vivo scan.

## Known gaps

None known.

## Citations

- Yaghmaie, N., Syeda, W., et al. (2021). "QSMART: Quantitative Susceptibility Mapping Artifact Reduction Technique." *NeuroImage*, 231:117701. https://doi.org/10.1016/j.neuroimage.2020.117701
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
