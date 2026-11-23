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

In the TUI: the Pipeline tab, **UK Biobank**. Taken from UK Biobank QSM (UK Biobank UKBiobank_QSM.m (git.fmrib.ox.ac.uk/cwang/uk_biobank_qsm_pipeline); Wang et al. 2022 Nat. Neurosci.; HD-BET in place of its registered T1 brain mask, an intentional difference).

**Parity:** [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-ukb.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-ukb) QSM-CI's parity check against the original toolbox, for the latest QSMxT release ([how parity is checked](/QSMxT/toolbox-pipelines/#parity)).

## Use it when

You want results comparable with UK Biobank's released susceptibility maps, from the SWI scan alone.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Masking | Mask | `magnitude,hd-bet:window,signal-erode` | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | UK Biobank's brain mask is its T1 structural mask registered to the SWI, which an SWI run does not have; HD-BET on the magnitude is the closest mask from the SWI alone (about 3.4 mm higher at the brain's inferior edge on UK Biobank data, and otherwise close). That comparison ran HD-BET in 192x192x96 sliding windows (step 0.5, no test-time augmentation) on the inhomogeneity-corrected magnitude, then signal-gated erosion. The UK Biobank mask comparison ran HD-BET in 192x192x96 sliding windows; on the whole volume its mask was 0.5-1 % smaller, mostly at the brain's inferior edge, where the comparison's head-size bias came from. |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | UK Biobank unwraps each echo with STI Suite's MRPhaseUnwrap (Laplacian). |
|  | Laplacian solver (`--laplacian-solver`) | FFT | DCT | UK Biobank calls MRPhaseUnwrap with padsize [64 64 64]: the FFT Schofield-Zhu solver with 64 voxels of padding. |
|  | Laplacian field kept outside the mask (`--laplacian-keep-outside`) | on | off | UK Biobank hands V_SHARP its combined unwrapped phase over the whole volume, which V_SHARP reads outside the mask it is given. |
|  | Phase offset removal (`--phase-offset-removal`) | off, for scans with two or more echoes | on | UK Biobank removes the phase offsets when it combines the coils (MCPC-3D-S) and unwraps the combined echoes with no further offset removal. |
|  | Echo weighting (`--b0-weight-type`) | Assumed decay, for scans with two or more echoes | Phase SNR | UK Biobank weights the echoes by TE·exp(−TE/T2\*) with T2\* = 40 ms (AssumedDecay, 0.04 s). |
| Background | UKB mask trim (`--bf-ukb-mask-trim`) | on | off | One of the choices that start this pipeline. |
| Inversion | Padding (`--ilsqr-pad-mm`) | 64 mm | 12 mm | UK Biobank calls QSM_iLSQR with padsize [64 64 64]. |
|  | Precision (`--ilsqr-precision`) | Single | Double | STI Suite's QSM_iLSQR runs both LSQR solves in single precision. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | UK Biobank keeps QSM_iLSQR's χ as it is, without referencing it. |

## Intentional differences from the original

- **The brain mask is HD-BET on the magnitude, in 192×192×96 sliding windows, followed by signal-gated erosion.** UK Biobank masks with its T1 structural brain mask registered to the SWI, which a run from the SWI alone does not have; on UK Biobank data HD-BET came closest to it of the masks an SWI alone can give (about 3.4 mm higher at the brain's inferior edge, otherwise close).
- **Phase is converted to field in ppm with the scanner's centre frequency (ImagingFrequency) where the scan's metadata records it, and with the nominal field strength only where it does not.** The centre frequency is the field the scanner actually ran at, while the nominal field strength the toolboxes are usually given (STI Suite, QSMART, UK Biobank's script) can be several per cent off: Siemens' 3 T systems run at about 2.89 T, and every susceptibility value scales with it.

## Known gaps

On UK Biobank's own scans (3 mm slices, oblique), V-SHARP's local field differs slightly from STI Suite's, which iLSQR amplifies to about 0.3-0.4 % of the susceptibility map; the cause is being investigated ([QSM.rs#491](https://github.com/astewartau/QSM.rs/issues/491)).

## Citations

- Wang, C., et al. (2022). "Phenotypic and genetic associations of quantitative magnetic susceptibility in UK Biobank brain imaging." *Nature Neuroscience*, 25(6):818-831. https://doi.org/10.1038/s41593-022-01074-w
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
- Isensee, F., Schell, M., Pflueger, I., et al. (2019). "Automated brain extraction of multisequence MRI using artificial neural networks." *Human Brain Mapping*, 40(17):4952-4964. https://doi.org/10.1002/hbm.24750
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
