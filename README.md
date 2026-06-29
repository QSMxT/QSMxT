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
- **10 inversion algorithms**, 8 background-field methods, flexible masking,
  plus SWI / T2\* / R2\* outputs.
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

Full guides and the complete command reference are on the
**[documentation site](https://qsmxt.github.io/QSMxT/)** (or run `qsmxt --help`).

## Coming from QSMxT 8.x (Python)?

v9 is a ground-up rewrite in Rust focused on QSM reconstruction and DICOM/NIfTI →
BIDS conversion. Some 8.x workflows (segmentation, template building, group
analysis, NextQSM, the web UI) are not part of v9 — the Python line remains
available on the [`python-legacy`](https://github.com/QSMxT/QSMxT/tree/python-legacy)
branch and the [`v8.3.2`](https://github.com/QSMxT/QSMxT/releases/tag/v8.3.2) tag.
See [what changed](https://qsmxt.github.io/QSMxT/#coming-from-qsmxt-8x).

## Citation

> Stewart AW, Robinson SD, O'Brien K, et al. "QSMxT: Robust masking and artifact
> reduction for quantitative susceptibility mapping." *Magnetic Resonance in
> Medicine* 87.3 (2022): 1289–1300. <https://doi.org/10.1002/mrm.29048>

## License

[GPL-3.0-only](LICENSE).
