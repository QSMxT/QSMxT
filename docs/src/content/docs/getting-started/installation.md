---
title: Installation
description: Install QSMxT on Linux, macOS, or Windows — or build it from source.
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

## Keeping up to date

```sh
qsmxt update
```

This checks the [latest release](https://github.com/QSMxT/QSMxT/releases),
installs it if newer, and keeps the bundled `dcm2niix` in sync. The download is
checked against the release's `SHA256SUMS` before anything is extracted; a
missing or mismatching checksum aborts the update and leaves the installed
binary untouched.

A build from source reports version `0.0.0` and has no release to compare
against, so `qsmxt update` refuses to replace it. Use `qsmxt update --force` to
install the latest release over it anyway.

`qsmxt tui` also checks for new releases in the background and offers the update
in a pop-up when one is available. The check runs on its own thread and its
result is cached for 24 hours, so it never delays startup. From the pop-up,
`Enter` installs the update, `Esc` defers it to the next launch, and `d` skips
that release for good.

Set `QSMXT_NO_UPDATE_CHECK=1` to turn the background check off entirely. It is
also skipped automatically when `CI` is set and in local development builds.

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
