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
| Masking | Mask | `magnitude,bet:0.50` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | MEDI's pipeline masks with FSL's brain extraction (fractional threshold 0.5) on the combined magnitude. (note 1) |
|  | Orientation (`--bet-orientation`) | As stored | As FSL reads it | The MEDI toolbox runs brain extraction on the image as stored, while FSL's own tool flips a neurologically stored image first; the two masks differ slightly at the brain's edge. |
|  | Inhomogeneity correction (`--no-inhomogeneity-correction`) | off | on | The MEDI toolbox runs brain extraction on the magnitude as read, with no bias-field correction. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | MEDI region growing | ROMEO | MEDI's pipeline unwraps its complex echo fit's field with its region-growing unwrapping, guided by the combined magnitude. |
|  | B0 estimation (`--b0-estimation`) | Complex fit | Weighted average | MEDI's field map and its noise map both come from its complex echo fit. PDF weights the field by the noise map from MEDI's complex echo fit. |
| Background | Background removal (`--bf-algorithm`) | PDF | V-SHARP | MEDI's pipeline inverts the PDF local field. |
| Inversion | QSM inversion (`--qsm-algorithm`) | MEDI | iLSQR | The MEDI toolbox's pipeline inverts the field with MEDI. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | MEDI references susceptibility to the ventricular CSF inside its inversion (with three or more echoes), and the pipeline does nothing after it. |

Notes on the table:

1. MEDI's PDF runs inside the brain extraction's mask. The MEDI toolbox runs its brain extraction on the magnitude combined over echoes. It uses the brain mask as it is, without eroding it.

## Intentional differences from the original

None.

## Known gaps

For bipolar readouts, turn on bipolar correction: the complex fit is then the MEDI toolbox's bipolar echo fit (Fit_ppm_complex_bipolar) ([QSMxT#564](https://github.com/QSMxT/QSMxT/issues/564)).

## Citations

- Liu, T., et al. (2011). "Morphology enabled dipole inversion (MEDI) from a single-angle acquisition: Comparison with COSMOS in human brain imaging." *Magnetic Resonance in Medicine*, 66(3):777-783. https://doi.org/10.1002/mrm.22816
- Liu, T., et al. (2011). "A novel background field removal method for MRI using projection onto dipole fields (PDF)." *NMR in Biomedicine*, 24(9):1129-1136. https://doi.org/10.1002/nbm.1670
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
