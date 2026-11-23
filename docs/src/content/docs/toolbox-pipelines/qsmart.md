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

In the TUI: the Pipeline tab, **QSMART**. Taken from QSMART (QSMART QSMART.m; Yaghmaie et al. 2021 NeuroImage 231:117701).

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-qsmart.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-qsmart) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with QSMART's own pipeline, which treats the cortex next to the skull and the veins separately.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude-first,bet:0.20,min-intensity:100,majority,close:2` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | QSMART's authors mask with FSL bet2 -f 0.2 on the N4-corrected first-echo magnitude. QSMART runs BET on the first echo's magnitude (root-sum-of-squares over coils). QSMART runs bet2 with -f 0.2. QSMART removes the BET mask's voxels whose N4-corrected magnitude is below 100 (QSMART v1.0's mag_threshold). QSMART smooths its mask with bwmorph3(mask, 'majority'). QSMART closes its mask with a sphere of radius 2 voxels (sph_radius1). QSMART keeps the brain's edge instead of eroding it away. Laplacian unwrapping is not thrown off by the unreliable phase there, and QSMART's echo fit supplies the reliability map it uses to handle it. With a path-based unwrapper, errors at the edge become artefacts across the cortex. |
|  | Inhomogeneity correction method (`--inhomogeneity-method`) | N4 | makehomogeneous | QSMART corrects the first echo's magnitude with ANTs' N4BiasFieldCorrection before BET. |
|  | Image geometry (`--n4-geometry`) | make_nii | Scanner | QSMART hands N4 the magnitude in a file written by make_nii, whose axis-aligned header decides where N4's shrunk grid falls. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | QSMART keeps the brain's edge instead of eroding it away. Laplacian unwrapping is not thrown off by the unreliable phase there, and QSMART's echo fit supplies the reliability map it uses to handle it. With a path-based unwrapper, errors at the edge become artefacts across the cortex. |
|  | Laplacian solver (`--laplacian-solver`) | Mirror | DCT | QSMART's lapunwrap.m solves on the mirrored volume. |
|  | Laplacian unwrapping of the raw phase (`--laplacian-unmasked`) | on | off | QSMART unwraps each echo's raw phase over the whole volume, and its echo fit fits every voxel. |
|  | Phase offset removal (`--phase-offset-removal`) | off, for scans with two or more echoes | on | QSMART unwraps each echo's raw phase, with no phase-offset removal first. |
|  | B0 estimation (`--b0-estimation`) | QSMART echo fit | Weighted average | QSMART keeps the brain's edge instead of eroding it away. Laplacian unwrapping is not thrown off by the unreliable phase there, and QSMART's echo fit supplies the reliability map it uses to handle it. With a path-based unwrapper, errors at the edge become artefacts across the cortex. |
| Inversion | QSM inversion (`--qsm-algorithm`) | QSMART | iLSQR | One of the choices that start this pipeline. |
|  | Precision (`--ilsqr-precision`) | Single | Double | QSMART's two stages call STI Suite's QSM_iLSQR, which runs both LSQR solves in single precision. |
|  | Offset adjustment grid (`--qsmart-offset-grid`) | QSMART's | FFT | QSMART's adjust_offset.m builds its dipole kernel on its own grid (-N/2:N/2-1), half a step off the FFT grid on odd matrix sizes. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | QSMART's result is adjust_offset's combined χ, which nothing references afterwards. |

## Intentional differences from the original

- **One gyromagnetic ratio is used throughout.** QSMART converts the field to rad/s with 2.675e8 rad/s/T and STI Suite's iLSQR inside it converts back with 2π·42.575 MHz/T, which lowers its susceptibility by 0.002 %; the mismatch is not part of the method.
- **Each echo's Laplacian-unwrapped phase has no arbitrary constant added.** QSMART's Laplacian unwrapping leaves each echo a small constant set by FFT rounding, which cannot be matched and is not part of the method; through the echo fit it moved the total field by 0.24 % on QSM-CI's in-vivo scan.
- **Phase is converted to field in ppm with the scanner's centre frequency (ImagingFrequency) where the scan's metadata records it, and with the nominal field strength only where it does not.** The centre frequency is the field the scanner actually ran at, while the nominal field strength the toolboxes are usually given (STI Suite, QSMART, UK Biobank's script) can be several per cent off: Siemens' 3 T systems run at about 2.89 T, and every susceptibility value scales with it.

## Known gaps

None known.

## Citations

- Yaghmaie, N., Syeda, W., et al. (2021). "QSMART: Quantitative Susceptibility Mapping Artifact Reduction Technique." *NeuroImage*, 231:117701. https://doi.org/10.1016/j.neuroimage.2020.117701
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
