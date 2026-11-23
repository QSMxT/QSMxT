---
title: "STI Suite · STAR-QSM"
description: "STI Suite 3.0's pipeline in QSMxT: qsmxt run --pipeline sti-suite-star."
sidebar:
  order: 3
---

<!-- generated:pipeline-page (from QSM.rs's toolbox pipelines and rules: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

```sh
qsmxt run study/bids --pipeline sti-suite-star
```

In the TUI: the Pipeline tab, **STI Suite · STAR-QSM**. Taken from STI Suite 3.0 (STI Suite 3.0 QSM_GUI, probed headless (QSM.rs star.rs and vsharp.rs docs)).

**Parity:** No whole-pipeline parity test yet ([QSM-CI#469](https://github.com/QSMxT/QSM-CI/issues/469)).

## Use it when

You want results comparable with STI Suite's QSM_GUI run with STAR-QSM.

## Steps and settings

Choosing it resets every reconstruction stage to QSMxT's defaults, then sets these. Every other setting keeps [its default](/QSMxT/reference/configuration/).

| Stage | Setting | This pipeline | QSMxT default | Why |
| --- | --- | --- | --- | --- |
| Field map | Phase unwrapping (`--unwrapping-algorithm`) | Laplacian | ROMEO | STI Suite's QSM_GUI unwraps each echo with MRPhaseUnwrap (Laplacian). |
|  | Laplacian solver (`--laplacian-solver`) | FFT | DCT | MRPhaseUnwrap is the FFT Schofield-Zhu solver with 12 voxels of padding. |
|  | Laplacian FFT padding (`--laplacian-fft-pad`) | 12 voxels | 64 voxels | MRPhaseUnwrap is the FFT Schofield-Zhu solver with 12 voxels of padding. |
|  | Laplacian unwrapping of the raw phase (`--laplacian-unmasked`) | on | off | STI Suite's QSM_GUI gives MRPhaseUnwrap the raw wrapped phase, not the phase masked first. |
|  | Phase offset removal (`--phase-offset-removal`) | off, for scans with two or more echoes | on | STI Suite's QSM_GUI unwraps each echo's phase as it is given, with no phase-offset removal first. |
|  | Echo weighting (`--b0-weight-type`) | Echo time, for scans with two or more echoes | Phase SNR | Σφ/ΣTE is the TE-weighted mean of φ/TE. |
| Background | Polish mask (`--bf-polish-mask`) | on | off | STI Suite's QSM_GUI polishes V-SHARP's mask (largest connected component, holes filled) and inverts the field inside it. |
| Inversion | QSM inversion (`--qsm-algorithm`) | STAR-QSM | iLSQR | One of the choices that start this pipeline. |
| Reference | QSM reference (`--qsm-reference`) | None | Mean | STI Suite's QSM_GUI keeps the inversion's χ as it is, without referencing it. |

## Intentional differences from the original

- **The B0 direction is taken from the scan's orientation.** STI Suite's GUI uses B0 along the slice axis unless its user types a direction in, and its manual asks for the scan's direction on oblique scans; on axial scans the two are the same.
- **V-SHARP's eroded mask is computed exactly, so it keeps a few isolated voxels inside the brain that STI Suite leaves out.** STI Suite leaves them out through rounding in its single-precision FFTs, which no other FFT matches and which does not depend on the data; on QSM-CI's in-vivo scan it was 4 of about 1.3 million voxels.
- **Phase is converted to field in ppm with the scanner's centre frequency (ImagingFrequency) where the scan's metadata records it, and with the nominal field strength only where it does not.** The centre frequency is the field the scanner actually ran at, while the nominal field strength the toolboxes are usually given (STI Suite, QSMART, UK Biobank's script) can be several per cent off: Siemens' 3 T systems run at about 2.89 T, and every susceptibility value scales with it.

## Known gaps

STI Suite's STAR-QSM path makes its brain mask with its own routine, which is not available yet. This pipeline uses the default mask instead ([QSM.rs#493](https://github.com/astewartau/QSM.rs/issues/493)).

## Citations

- Wei, H., Dibb, R., Zhou, Y., Sun, Y., Xu, J., Wang, N., Liu, C. (2015). "Streaking artifact reduction for quantitative susceptibility mapping of sources with large dynamic range." *NMR in Biomedicine*, 28(10):1294-1303. https://doi.org/10.1002/nbm.3383
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
