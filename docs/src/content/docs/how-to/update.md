---
title: Update QSMxT
description: "Install the latest release of QSMxT."
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

In the TUI, a pop-up tells you when a new release is out when it starts: press `Enter` to
update.

## Check it worked

`qsmxt --version` shows the number of the
[latest release](https://github.com/QSMxT/QSMxT/releases).

## Announcements

The QSMxT team posts announcements, such as a problem found in a release and the version that
fixes it. The TUI shows a new one when it starts. To read them all:

```sh
qsmxt news
```

## Other cases

- **A build from source:** `qsmxt update` will not replace it unless you add `--force`.
- **The development version:** `qsmxt update --main` builds the latest commit from source (it
  needs [Rust](https://rustup.rs)). `qsmxt update --force` goes back to the latest release.
- **A particular release:** reinstall with `QSMXT_VERSION` set; see
  [Install](/QSMxT/start/install/#linux--macos).
- **No update check:** set `QSMXT_NO_UPDATE_CHECK=1`.

How updating, the update check and announcements work is in
[Updates and announcements](/QSMxT/reference/updates/).
