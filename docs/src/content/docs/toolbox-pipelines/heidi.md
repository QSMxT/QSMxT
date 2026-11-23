---
title: "HEIDI package"
description: "HEIDI package (qsm_heidi 4592631)'s pipeline in QSMxT: qsmxt run --pipeline heidi."
sidebar:
  order: 5
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline heidi
```

In the TUI: the Pipeline tab, **HEIDI package**.

**Based on:** The HEIDI package (qsm_heidi), LSQR then HEIDI · Schweser et al. 2012.

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Fheidi.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-heidi) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with the HEIDI package: LSQR, then HEIDI, from a SHARP local field.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Background | Background removal (`--bf-algorithm`) | SHARP | V-SHARP | The HEIDI package inverts a local field from SHARP. |
| Inversion | QSM inversion (`--qsm-algorithm`) | LSQR | iLSQR | The HEIDI package first inverts the local field with LSQR, a least-squares fit with almost no regularisation, which keeps the measured data that HEIDI builds on. |
|  | HEIDI streaking reduction (`--heidi`) | on | off | HEIDI then fills in the ill-conditioned part of the data from its surroundings, which reduces streaking. |
|  | ITK float arithmetic (`--heidi-denoise-reference-itk`) | on | off | The HEIDI package smooths the field with ITK's anisotropic diffusion filter, which computes in single precision. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | The HEIDI package saves its susceptibility map without referencing it. |

## Intentional differences from the original

- **Where the original toolbox converts phase to ppm with the nominal field strength (such as 3 T), this pipeline uses the scanner's centre frequency (ImagingFrequency) whenever the scan's metadata records it.** The centre frequency is the field the scanner actually ran at, while the nominal value can be several per cent off (Siemens' 3 T systems run at about 2.89 T), and every susceptibility value scales with it.

## Known gaps

None known.

## Citations

- Schweser, F., Sommer, K., Deistung, A., Reichenbach, J.R. (2012). "Quantitative susceptibility mapping for investigating subtle susceptibility variations in the human brain." *NeuroImage*, 62(3):2083-2100. https://doi.org/10.1016/j.neuroimage.2012.05.067
- Schweser, F., Deistung, A., Lehr, B.W., Reichenbach, J.R. (2010). "Differentiation between diamagnetic and paramagnetic cerebral lesions based on magnetic susceptibility mapping." *Medical Physics*, 37(10):5165-5178. https://doi.org/10.1118/1.3481505
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
