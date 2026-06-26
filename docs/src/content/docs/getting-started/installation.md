---
title: Installation
description: Install QSMxT on Linux, macOS, or Windows — or build it from source.
---

QSMxT ships as a single self-contained binary. There is no environment to manage
and no toolbox to assemble.

:::tip[Just want to try it?]
You don't have to install anything to experiment with the algorithms —
**[qsmbly](https://qsmbly.neurodesk.org/)** runs the same QSM engine in your
browser.
:::

## Quick install

### Linux / macOS

```sh
curl -fsSL https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/QSMxT/QSMxT/main/install.ps1 | iex
```

The installer places the `qsmxt` binary on your `PATH` and installs a pinned copy
of [`dcm2niix`](https://github.com/rordenlab/dcm2niix) (used for DICOM → BIDS
conversion) into `~/.qsmxt/bin`.

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

## Keeping up to date

```sh
qsmxt update
```

This checks the [latest release](https://github.com/QSMxT/QSMxT/releases),
installs it if newer, and keeps the bundled `dcm2niix` in sync.

## Manual download

Grab the archive for your platform from the
[Releases](https://github.com/QSMxT/QSMxT/releases) page and extract `qsmxt`
somewhere on your `PATH`. Builds are provided for:

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
