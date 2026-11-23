---
title: "MEDI toolbox"
description: "MEDI toolbox 2024.11.26's pipeline in QSMxT: qsmxt run --pipeline medi-toolbox."
sidebar:
  order: 1
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline medi-toolbox
```

In the TUI: the Pipeline tab, **MEDI toolbox**.

**Based on:** The MEDI toolbox's example pipeline (README.m) · Liu et al. 2011.

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-medi-cornell.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-medi-cornell) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with the MEDI toolbox's own pipeline: its complex echo fit, PDF and MEDI.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude,bet:0.50` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | MEDI's pipeline masks with FSL BET (bet2, f 0.5) on the root-sum-of-squares magnitude. (note 1) |
|  | Orientation (`--bet-orientation`) | As stored | As FSL reads it | MEDI's BET.m hands the bet2 mex the MATLAB array, so bet2 sees the voxels as stored; FSL's own bet reverses x of a neurological file first. The two masks differ at the brain's edge. |
|  | Inhomogeneity correction (`--no-inhomogeneity-correction`) | off | on | The MEDI toolbox runs BET on the magnitude as read, with no bias-field correction. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | MEDI region growing | ROMEO | MEDI's pipeline unwraps Fit_ppm_complex's field with its region-growing unwrapPhase on the root-sum-of-squares magnitude. |
|  | B0 estimation (`--b0-estimation`) | Complex fit | Weighted average | MEDI's field map and its noise weight N_std come from Fit_ppm_complex. PDF is weighted by Fit_ppm_complex's noise map N_std. |
| Background | Background removal (`--bf-algorithm`) | PDF | V-SHARP | MEDI's pipeline inverts the PDF local field. |
| Inversion | QSM inversion (`--qsm-algorithm`) | MEDI | iLSQR | One of the choices that start this pipeline. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | MEDI's pipeline references χ to the ventricular CSF inside MEDI_L1 (with three or more echoes), and not after it. |

Notes on the table:

1. MEDI's PDF runs inside the BET mask. The MEDI toolbox runs BET on the root-sum-of-squares magnitude over echoes. The MEDI toolbox uses BET's mask as it is, without eroding it.

## Intentional differences from the original

None.

## Known gaps

For bipolar readouts, the MEDI toolbox uses a bipolar echo fit, which is not supported yet. This pipeline always uses the monopolar fit ([QSMxT#564](https://github.com/QSMxT/QSMxT/issues/564)).

## Citations

- Liu, T., et al. (2011). "Morphology enabled dipole inversion (MEDI) from a single-angle acquisition: Comparison with COSMOS in human brain imaging." *Magnetic Resonance in Medicine*, 66(3):777-783. https://doi.org/10.1002/mrm.22816
- Liu, T., et al. (2011). "A novel background field removal method for MRI using projection onto dipole fields (PDF)." *NMR in Biomedicine*, 24(9):1129-1136. https://doi.org/10.1002/nbm.1670
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
