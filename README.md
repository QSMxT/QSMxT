# QSMxT

A fast, BIDS-native Quantitative Susceptibility Mapping (QSM) pipeline and
interactive TUI, built in Rust. QSMxT takes you from raw scanner DICOMs to
quantitative susceptibility maps with a single self-contained binary — no
environment to manage, no toolbox sprawl.

## 📖 Documentation: **<https://qsmxt.github.io/QSMxT/>**

The website has everything: installation, guides, the command reference, and
algorithm details. This README is just a quick pointer.

## Highlights

- **End-to-end pipeline** — masking, phase unwrapping, echo combination,
  background field removal, dipole inversion, and referencing.
- **Interactive TUI** — convert, configure, and run from the terminal; it also
  shows the equivalent CLI command as you go.
- **DICOM → BIDS** — built-in conversion with automatic series classification.
- **50+ algorithms** — five masking presets, 10 background-field removers, 21
  classical and 11 deep-learning dipole inversions, 8 χ-separation methods, and
  multi-orientation COSMOS / STI.
- **More than χ** — CLEAR-SWI, SMWI, T2\* / R2\* and R2 / R2′ maps, plus
  SynthSeg segmentation with per-structure statistics.
- **Methods text** — every run writes a methods paragraph with the citations
  for exactly what ran.
- **Built for scale** — disk caching, memory-aware parallelism, and SLURM support.

All reconstruction algorithms are provided by
[QSM.rs](https://github.com/astewartau/QSM.rs).

## Install

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.ps1 | iex
```

No install needed to experiment — [try it in your browser](https://qsmbly.neurodesk.org/).
See the [installation guide](https://qsmxt.github.io/QSMxT/getting-started/installation/)
for manual downloads, building from source, and details.

## Quick start

```sh
# Easiest: do everything interactively (convert, configure, run)
qsmxt tui

# Or on the command line:
qsmxt dicom-convert /path/to/dicoms study/bids   # DICOM → BIDS
qsmxt run study/bids                             # run the pipeline
```

No data to hand? Fetch a real in-vivo subject and reconstruct it:

```sh
qsmxt example study/bids                         # download an example dataset
qsmxt run study/bids                             # run the pipeline
```

Full guides and the complete command reference are on the
**[documentation site](https://qsmxt.github.io/QSMxT/)** (or run `qsmxt --help`).

<!-- generated:algorithms start (from QSM.rs's method catalogue: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

## Algorithms

The methods QSMxT offers at each stage, all implemented in
[QSM.rs](https://github.com/astewartau/QSM.rs), and the papers to cite for them.

### Masking

| Method | Citation |
| --- | --- |
| Otsu | [Otsu, 1979](https://doi.org/10.1109/TSMC.1979.4310076) |
| Fixed threshold | — |
| Percentile threshold | — |
| STI Suite GUI threshold | — |
| BET | [Smith, 2002](https://doi.org/10.1002/hbm.10062) |
| HD-BET | [Isensee et al., 2019](https://doi.org/10.1002/hbm.24750) |
| MindGrab | [Fani et al., 2025](https://doi.org/10.48550/arXiv.2506.11860) |
| RS2-Net | [Lin et al., 2024](https://doi.org/10.1016/j.neuroimage.2024.120769) |
| Erosion | — |
| Dilation | — |
| Closing | — |
| Minimum intensity | — |
| Majority filter | — |
| Largest connected component | — |
| Hole filling (imfill) | — |
| Hole filling | — |
| Gaussian smoothing | — |
| Signal-gated erosion | — |

### Inhomogeneity correction

| Method | Citation |
| --- | --- |
| makehomogeneous | Eckstein et al., 2019 |
| N4 | [Tustison et al., 2010](https://doi.org/10.1109/TMI.2010.2046908) |

### Phase correction

| Method | Citation |
| --- | --- |
| MCPC-3D-S | [Eckstein et al., 2018](https://doi.org/10.1002/mrm.26963) |
| Bipolar correction | [Eckstein, 2021](https://doi.org/10.34726/hss.2021.43447) |

### Phase unwrapping

| Method | Citation |
| --- | --- |
| ROMEO | [Dymerska et al., 2021](https://doi.org/10.1002/mrm.28563) |
| Laplacian | [Schofield & Zhu, 2003](https://doi.org/10.1364/OL.28.001194) |
| PRELUDE | [Jenkinson, 2003](https://doi.org/10.1002/mrm.10354) |
| MEDI region growing | [Liu et al., 2013](https://doi.org/10.1002/mrm.24272) |

### Echo combination

| Method | Citation |
| --- | --- |
| Weighted average | — |
| Linear fit | — |
| Robust fit | — |
| QSMART echo fit | [Yaghmaie et al., 2021](https://doi.org/10.1016/j.neuroimage.2020.117701) |
| Nonlinear complex fit | [Liu et al., 2013](https://doi.org/10.1002/mrm.24272) |

### Background field removal

| Method | Citation |
| --- | --- |
| V-SHARP | [Wu et al., 2012](https://doi.org/10.1002/mrm.23000) |
| PDF | [Liu et al., 2011a](https://doi.org/10.1002/nbm.1670) |
| LBV | [Zhou et al., 2014](https://doi.org/10.1002/nbm.3064) |
| iSMV | [Wen et al., 2014](https://doi.org/10.1002/mrm.24998) |
| SHARP | [Schweser et al., 2011](https://doi.org/10.1016/j.neuroimage.2010.10.070) |
| RESHARP | [Sun & Wilman, 2014](https://doi.org/10.1002/mrm.24765) |
| HARPERELLA | [Li et al., 2014](https://doi.org/10.1002/nbm.3056) |
| iHARPERELLA | Li et al., 2015a |
| BFRnet | [Zhu et al., 2023](https://doi.org/10.1016/j.zemedi.2022.08.001) |
| iQFM | [Gao et al., 2022](https://doi.org/10.1016/j.neuroimage.2022.119410) |

### Dipole inversion

| Method | Citation |
| --- | --- |
| RTS | [Kames et al., 2018](https://doi.org/10.1016/j.neuroimage.2017.11.018) |
| TV-ADMM | [Bilgic et al., 2014a](https://doi.org/10.1002/mrm.25029) |
| TKD | [Shmueli et al., 2009](https://doi.org/10.1002/mrm.22135) |
| TSVD | [Shmueli et al., 2009](https://doi.org/10.1002/mrm.22135) |
| TGV-QSM | [Langkammer et al., 2015](https://doi.org/10.1016/j.neuroimage.2015.02.041) |
| Tikhonov | [Bilgic et al., 2014b](https://doi.org/10.1002/jmri.24365) |
| NLTV | [Milovic et al., 2018](https://doi.org/10.1002/mrm.27073) |
| MEDI | [Liu et al., 2011b](https://doi.org/10.1002/mrm.22816) |
| TFI | [Liu et al., 2017](https://doi.org/10.1002/mrm.26331) |
| iLSQR | [Li et al., 2015b](https://doi.org/10.1016/j.neuroimage.2014.12.043) |
| STAR-QSM | [Wei et al., 2015](https://doi.org/10.1002/nbm.3383) |
| LSQR | [Schweser et al., 2010](https://doi.org/10.1118/1.3481505) |
| QSMART | [Yaghmaie et al., 2021](https://doi.org/10.1016/j.neuroimage.2020.117701) |
| NDI | [Polak et al., 2020](https://doi.org/10.1002/nbm.4271) |
| FANSI nonlinear TV | [Milovic et al., 2018](https://doi.org/10.1002/mrm.27073) |
| FANSI nonlinear TGV | [Milovic et al., 2018](https://doi.org/10.1002/mrm.27073) |
| L1-QSM | [Milovic et al., 2022](https://doi.org/10.1002/mrm.28957) |
| WH-QSM | [Milovic et al., 2019](https://doi.org/10.1002/mrm.27483) |
| HD-QSM | [Lambert et al., 2022](https://doi.org/10.1002/mrm.29218) |
| AMP-PE | [Huang et al., 2023](https://doi.org/10.1002/mrm.29722) |

### Dipole inversion: deep learning

| Method | Citation |
| --- | --- |
| xQSM | [Gao et al., 2021](https://doi.org/10.1002/nbm.4461) |
| QSMnet | [Yoon et al., 2018](https://doi.org/10.1016/j.neuroimage.2018.06.030) |
| QSMnet+ | [Jung et al., 2020](https://doi.org/10.1016/j.neuroimage.2020.116619) |
| QSMnetME | [Kim et al., 2025](https://doi.org/10.1002/hbm.70136) |
| AutoQSM | [Wei et al., 2019](https://doi.org/10.1016/j.neuroimage.2019.116064) |
| QSMGAN | [Chen et al., 2020](https://doi.org/10.1016/j.neuroimage.2019.116389) |
| IR2QSM | [Li et al., 2025a](https://doi.org/10.1002/mp.17747) |
| LPCNN | [Lai et al., 2020](https://doi.org/10.1007/978-3-030-59713-9_13) |
| MoDL-QSM | [Feng et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118376) |
| NeXtQSM | [Cognolato et al., 2023](https://doi.org/10.1016/j.media.2022.102700) |
| iQSM | [Gao et al., 2022](https://doi.org/10.1016/j.neuroimage.2022.119410) |
| iQSM+ | [Gao et al., 2024](https://doi.org/10.1016/j.media.2024.103160) |

### Additions to the inversion

| Method | Citation |
| --- | --- |
| MEDI+0 | [Liu et al., 2018](https://doi.org/10.1002/mrm.26946) |
| mSMV | [Roberts et al., 2024](https://doi.org/10.1002/mrm.29963) |
| HEIDI | [Schweser et al., 2012](https://doi.org/10.1016/j.neuroimage.2012.05.067) |
| Two-pass artefact reduction | [Stewart et al., 2022](https://doi.org/10.1002/mrm.29048) |

### Multi-orientation

| Method | Citation |
| --- | --- |
| COSMOS | [Liu et al., 2009](https://doi.org/10.1002/mrm.21828) |
| STI | [Liu, 2010](https://doi.org/10.1002/mrm.22482) |

### Susceptibility source separation

| Method | Citation |
| --- | --- |
| R2\*-QSM | [Dimov et al., 2022a](https://doi.org/10.1111/jon.13014); [Dimov et al., 2022b](https://doi.org/10.3390/tomography8030127) |
| DECOMPOSE-QSM | [Chen et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118477) |
| χ-separation (iLSQR) | [Shin et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118371) |
| χ-separation (MEDI) | [Shin et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118371) |
| WaveSep | [Fang et al., 2023](https://doi.org/10.1007/978-3-031-44858-4_6) |
| HC-ChiSep | Stewart et al., 2026; [Wharton & Bowtell, 2012](https://doi.org/10.1073/pnas.1211075109) |
| SUSEP-Net | [Li et al., 2025b](https://doi.org/10.48550/arXiv.2506.13293) |
| χ-sepnet | [Kim et al., 2025](https://doi.org/10.1002/hbm.70136) |

### Relaxometry

| Method | Citation |
| --- | --- |
| ARLO | [Pei et al., 2015](https://doi.org/10.1002/mrm.25137) |
| EPG R2 mapping | [Weigel, 2015](https://doi.org/10.1002/jmri.24619) |
| R2PRIMEnet | [Kim et al., 2025](https://doi.org/10.1002/hbm.70136) |

### Susceptibility-weighted imaging

| Method | Citation |
| --- | --- |
| CLEAR-SWI | [Eckstein et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118175) |
| SMWI | [Gho et al., 2014](https://doi.org/10.1002/mrm.24920) |

### Segmentation

| Method | Citation |
| --- | --- |
| SynthSeg 1.0 | [Billot et al., 2023a](https://doi.org/10.1016/j.media.2023.102789) |
| SynthSeg 2.0 | [Billot et al., 2023a](https://doi.org/10.1016/j.media.2023.102789); [Billot et al., 2023b](https://doi.org/10.1073/pnas.2216399120) |

<!-- generated:algorithms end -->

## Coming from QSMxT 8.x (Python)?

v9 is a ground-up rewrite in Rust. Template and group-space building and the web
UI are not part of v9 — the Python line remains available on the
[`python-legacy`](https://github.com/QSMxT/QSMxT/tree/python-legacy) branch and the
[`v8.3.4`](https://github.com/QSMxT/QSMxT/releases/tag/v8.3.4) release.
See [what changed](https://qsmxt.github.io/QSMxT/getting-started/coming-from-8x/).

## Citation

> Stewart AW, Robinson SD, O'Brien K, et al. "QSMxT: Robust masking and artifact
> reduction for quantitative susceptibility mapping." *Magnetic Resonance in
> Medicine* 87.3 (2022): 1289–1300. <https://doi.org/10.1002/mrm.29048>

## License

[GPL-3.0-only](LICENSE).
