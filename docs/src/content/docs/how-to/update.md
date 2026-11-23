---
title: Update QSMxT
description: "Install the latest release of QSMxT, and read the team's announcements."
---

**Goal:** run the latest release of QSMxT.

## Steps

1. Update:

   ```sh
   qsmxt update
   ```

   This installs the latest release if it is newer than yours, and updates the `dcm2niix`
   that QSMxT uses for DICOM conversion.

2. Check the version:

   ```sh
   qsmxt --version
   ```

In the TUI, a pop-up says when a new release is out: `Enter` updates, `Esc` closes it until
the next start.

## Check it worked

`qsmxt --version` shows the number of the
[latest release](https://github.com/QSMxT/QSMxT/releases).

## Announcements

The QSMxT team posts announcements, such as a problem found in a release and the version that
fixes it. The TUI shows a new one once when it starts; one about a problem that can give wrong
results shows every time, until you update past it. Press `n` on the TUI's tab bar to read them
all, or run:

```sh
qsmxt news
```

## Other cases

- **A build from source** reports version `0.0.0`, so `qsmxt update` will not replace it unless
  you add `--force`.
- **The development version:** `qsmxt update --main` builds the latest commit of `main` from
  source. It needs [Rust](https://rustup.rs) and takes several minutes the first time; later
  updates reuse the build in `~/.qsmxt/build` (emptied when it passes 10 GB). `main` may hold
  changes that are not yet released or documented. `qsmxt update --force` goes back to the
  latest release.
- **A particular release:** reinstall with `QSMXT_VERSION` set; see
  [Install](/QSMxT/start/install/#linux--macos).
- **No update check:** set `QSMXT_NO_UPDATE_CHECK=1`. This also turns off announcements. The
  check is skipped when `CI` is set.

## How it works

The download is checked against the release's `SHA256SUMS` before anything is extracted; a
missing or wrong checksum stops the update and leaves your `qsmxt` as it was. The TUI checks for
a release and for announcements in the background, so it never delays startup; offline it uses
the last answer it had. Announcements are saved in `~/.qsmxt/announcements.toml`, and the ones
already shown in `~/.qsmxt/announcements-seen.json`.
