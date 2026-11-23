---
title: Install
description: "Install QSMxT on Linux, macOS, or Windows, or build it from source."
---

QSMxT ships as a single self-contained binary. There is no environment to manage
and no toolbox to assemble.

## Quick install

### Linux / macOS

```sh
curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.ps1 | iex
```

The installer needs no administrator rights. It installs `qsmxt` to
`~/.local/bin` (or `/usr/local/bin` when run as root), adds that directory to your
`PATH` in your shell's startup file if it isn't already there, and installs a pinned copy
of [`dcm2niix`](https://github.com/rordenlab/dcm2niix) (used for DICOM → BIDS
conversion) into `~/.qsmxt/bin`. If you already have a `qsmxt` on your `PATH` that
you can write to, the installer upgrades that copy where it is. Before extracting anything it checks the
archive against the release's `SHA256SUMS` file and stops if the file is
missing, has no entry for the archive, or gives a different checksum.

To install a specific release rather than the latest, set `QSMXT_VERSION`
(for example `QSMXT_VERSION=v9.23.0`) before running the installer.

To install somewhere else, set `INSTALL_DIR`. A directory you can't write to, such
as `/usr/local/bin` for a system-wide install, makes the installer use `sudo`:

```sh
curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.sh | INSTALL_DIR=/usr/local/bin sh
```

Set `QSMXT_NO_MODIFY_PATH=1` to leave your shell startup files alone; the installer
then prints the line to add yourself.

### HPC clusters and shared machines

The default install works without `sudo`, so on a cluster you install `qsmxt` into
your home directory with the same one-line command. A home directory shared with the
compute nodes makes `qsmxt` available in your jobs as well, and `qsmxt update` keeps
it current with no administrator involved.

:::note
Prebuilt `dcm2niix` is bundled for **x86_64 Linux, macOS, and x86_64 Windows**.
On **ARM Linux (aarch64)** and **ARM Windows** there is no upstream `dcm2niix`
build — install it yourself and make sure it is on your `PATH`. At runtime QSMxT
resolves `dcm2niix` in this order: `$QSMXT_DCM2NIIX` → `~/.qsmxt/bin` → next to
the `qsmxt` binary → `PATH`.
:::

## Verify

```sh
qsmxt --version
qsmxt --help
```

## Manual download

Grab the archive for your platform from the
[Releases](https://github.com/QSMxT/QSMxT/releases) page and extract `qsmxt`
somewhere on your `PATH`. Each release also carries a `SHA256SUMS` file; check
your download with `sha256sum -c SHA256SUMS --ignore-missing` (Linux),
`shasum -a 256 -c SHA256SUMS --ignore-missing` (macOS) or
`Get-FileHash -Algorithm SHA256` (Windows). Builds are provided for:

- `x86_64` / `aarch64` Linux (gnu + musl)
- `x86_64` / `aarch64` macOS
- `x86_64` / `aarch64` Windows

## From source

Requires a recent Rust toolchain (edition 2021+).

```sh
git clone https://github.com/QSMxT/QSMxT
cd QSMxT
cargo build --release
# binary at target/release/qsmxt
```

## Uninstall

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/uninstall.sh | sh
```

```powershell
# Windows PowerShell
irm https://raw.githubusercontent.com/QSMxT/QSMxT/main/uninstall.ps1 | iex
```

## Coming from QSMxT 8.x

QSMxT v9 is a complete rewrite in Rust: a single self-contained binary, with no
Python environment to manage. The reconstruction pipeline and DICOM/NIfTI
conversion are fully reimplemented and faster, and there is a new interactive TUI.

### Command changes

| 8.x | v9 |
| --- | --- |
| `qsmxt <bids>` | `qsmxt run <bids>` |
| `dicom-convert` | `qsmxt dicom-convert` |
| `--export_dicoms` | [`--export-dicom`](/QSMxT/how-to/export-dicom/) |

### Multi-coil data

8.x combined uncombined receive coils with MCPC-3D-S inside `dicom-convert`
(writing `_rec-mcpc3ds` files). In v9 the converter keeps the per-coil files
(`_rec-uncombined_coil-NN_*`) and `qsmxt run` combines them as its first stage;
`qsmxt combine mcpc3ds` does the same on loose files. See
[Uncombined coil data](/QSMxT/data/uncombined-coils/).

### Workflows not in v9

Some parts of the 8.x line are not included in v9:

- template and group-space building
- the web UI

(NeXtQSM is in v9, as `--qsm-algorithm nextqsm`.)

Anatomical segmentation is back, by a different route: v9 parcellates the GRE
magnitude directly with SynthSeg rather than registering a T1w atlas, and
summarises every quantitative map per structure. See
[Measure χ in brain structures](/QSMxT/how-to/results/#measure-χ-in-brain-structures).

If you rely on those, 8.x is still available: see the
[8.x documentation](https://qsmxt.github.io/QSMxT/v8/), the
[`python-legacy` branch](https://github.com/QSMxT/QSMxT/tree/python-legacy) and the
[8.x releases](https://github.com/QSMxT/QSMxT/releases).
