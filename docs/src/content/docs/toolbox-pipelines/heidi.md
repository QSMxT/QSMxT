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

In the TUI: the Pipeline tab, **HEIDI package**. Taken from HEIDI package (qsm_heidi 4592631) (qsm_heidi makefile and def.sh (4592631); Schweser et al. 2012 NeuroImage 62:2083).

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Fheidi.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-heidi) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with the HEIDI package: LSQR, then HEIDI, from a SHARP local field.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Background | Background removal (`--bf-algorithm`) | SHARP | V-SHARP | The authors' LSQR/HEIDI pipeline inverts a SHARP local field. |
| Inversion | QSM inversion (`--qsm-algorithm`) | LSQR | iLSQR | HEIDI's authors run it after LSQR. Other methods with little or no smoothing, such as this one, work as well: HEIDI needs the data outside the cone left as measured, and they leave it so. |
|  | HEIDI streaking reduction (`--heidi`) | on | off | HEIDI's authors run it after LSQR. Other methods with little or no smoothing, such as this one, work as well: HEIDI needs the data outside the cone left as measured, and they leave it so. |
|  | ITK float arithmetic (`--heidi-denoise-reference-itk`) | on | off | The HEIDI package denoises the field with ITK's GradientAnisotropicDiffusion executable, which works in float. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | The authors' LSQR/HEIDI pipeline saves HEIDI's χ in ppm without referencing it. |

## Intentional differences from the original

- **Phase is converted to field in ppm with the scanner's centre frequency (ImagingFrequency) where the scan's metadata records it, and with the nominal field strength only where it does not.** The centre frequency is the field the scanner actually ran at, while the nominal field strength the toolboxes are usually given (STI Suite, QSMART, UK Biobank's script) can be several per cent off: Siemens' 3 T systems run at about 2.89 T, and every susceptibility value scales with it.

## Known gaps

None known.

## Citations

- Schweser, F., Sommer, K., Deistung, A., Reichenbach, J.R. (2012). "Quantitative susceptibility mapping for investigating subtle susceptibility variations in the human brain." *NeuroImage*, 62(3):2083-2100. https://doi.org/10.1016/j.neuroimage.2012.05.067
- Schweser, F., Deistung, A., Lehr, B.W., Reichenbach, J.R. (2010). "Differentiation between diamagnetic and paramagnetic cerebral lesions based on magnetic susceptibility mapping." *Medical Physics*, 37(10):5165-5178. https://doi.org/10.1118/1.3481505
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
