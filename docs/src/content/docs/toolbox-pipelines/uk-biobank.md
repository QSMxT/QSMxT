---
title: "UK Biobank"
description: "UK Biobank QSM's pipeline in QSMxT: qsmxt run --pipeline uk-biobank."
sidebar:
  order: 10
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline uk-biobank
```

In the TUI: the Pipeline tab, **UK Biobank**.

**Based on:** UK Biobank's QSM pipeline (UKBiobank_QSM.m) · Wang et al. 2022.

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-ukb.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-ukb) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with UK Biobank's released susceptibility maps, from the SWI scan alone.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude,hd-bet:window,signal-erode` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | UK Biobank's own mask comes from its T1 scan, which an SWI run does not have; HD-BET on the magnitude came closest to it of the masks the SWI alone can give. HD-BET runs in 192×192×96 sliding windows, as in the comparison that chose it; on the whole volume its mask was slightly smaller at the brain's inferior edge. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | UK Biobank unwraps each echo with STI Suite's Laplacian unwrapping. |
|  | Laplacian solver (`--laplacian-solver`) | FFT | DCT | UK Biobank's Laplacian unwrapping is FFT-based, with 64 voxels of padding. |
|  | Laplacian field kept outside the mask (`--laplacian-keep-outside`) | on | off | UK Biobank gives V-SHARP the unwrapped field over the whole volume, and V-SHARP reads it outside the mask. |
|  | Phase offset removal (`--phase-offset-removal`) | off, for scans with two or more echoes | on | UK Biobank removes phase offsets when it combines the coils, and unwraps the combined echoes with no further offset removal. |
|  | Echo weighting (`--b0-weight-type`) | Assumed decay, for scans with two or more echoes | Phase SNR | UK Biobank weights the echoes by TE·exp(−TE/T2\*) with T2\* = 40 ms. |
| Background | UKB mask trim (`--bf-ukb-mask-trim`) | on | off | UK Biobank trims its mask to voxels with reliable phase, before and after background removal. |
| Inversion | Padding (`--ilsqr-pad-mm`) | 64 mm | 12 mm | UK Biobank pads iLSQR's grid by 64 mm per side. |
|  | Precision (`--ilsqr-precision`) | Single | Double | STI Suite's iLSQR runs its least-squares solves in single precision. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | UK Biobank keeps iLSQR's susceptibility as it is, without referencing it. |

## Intentional differences from the original

- **Where the original toolbox converts phase to ppm with the nominal field strength (such as 3 T), this pipeline uses the scanner's centre frequency (ImagingFrequency) whenever the scan's metadata records it.** The centre frequency is the field the scanner actually ran at, while the nominal value can be several per cent off (Siemens' 3 T systems run at about 2.89 T), and every susceptibility value scales with it.
- **The brain mask is HD-BET on the magnitude, in 192×192×96 sliding windows, followed by signal-gated erosion.** UK Biobank masks with its T1 structural brain mask registered to the SWI, which a run from the SWI alone does not have; on UK Biobank data HD-BET came closest to it of the masks an SWI alone can give (about 3.4 mm higher at the brain's inferior edge, otherwise close).

## Known gaps

On UK Biobank's own scans (3 mm slices, oblique), V-SHARP's local field differs slightly from STI Suite's, which iLSQR amplifies to about 0.3-0.4 % of the susceptibility map ([QSM.rs#491](https://github.com/astewartau/QSM.rs/issues/491)).

## Citations

- Wang, C., et al. (2022). "Phenotypic and genetic associations of quantitative magnetic susceptibility in UK Biobank brain imaging." *Nature Neuroscience*, 25(6):818-831. https://doi.org/10.1038/s41593-022-01074-w
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
- Isensee, F., Schell, M., Pflueger, I., et al. (2019). "Automated brain extraction of multisequence MRI using artificial neural networks." *Human Brain Mapping*, 40(17):4952-4964. https://doi.org/10.1002/hbm.24750
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
