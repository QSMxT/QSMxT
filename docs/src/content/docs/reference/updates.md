---
title: Updates and announcements
description: "How qsmxt update works, the main branch, the update check and announcements."
---

The steps are in [Update QSMxT](/QSMxT/how-to/update/). This page is how updating works.

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

### Trying the main branch

```sh
qsmxt update --main
```

This builds the latest commit on the `main` branch from source and installs it in
place of your `qsmxt`. It needs the Rust toolchain (install it from
[rustup.rs](https://rustup.rs)) and takes several minutes the first time. The build
is kept in `~/.qsmxt/build`, so a later `qsmxt update --main` rebuilds only QSMxT itself
(and QSM.rs when main has moved to another QSM.rs commit), not its dependencies. The
cache is emptied when it grows past 10 GB; delete the folder to free the space sooner. Main may contain
changes that are not yet released or documented. `qsmxt --version` shows the
commit you are running, and running `qsmxt update --main` again moves to the
newest commit. To go back to the latest release, run `qsmxt update --force`.

`qsmxt tui` also checks for a new release each time it starts and offers the
update in a pop-up when one is available. The check runs in the background with
a short timeout, so it never delays startup and makes no difference without an
internet connection; offline, the result of the last check that succeeded is
used. From the pop-up, `Enter` installs the update and `Esc` closes it until the
next start.

Set `QSMXT_NO_UPDATE_CHECK=1` to turn the background check off entirely (this
also turns off the announcements below). It is also skipped automatically when
`CI` is set and in local development builds.

### Announcements

The QSMxT team publishes announcements, such as a problem found in a release and
the version that fixes it. `qsmxt tui` fetches them in the background when it
starts and shows a new one once in a pop-up. An announcement about a problem
that can give wrong results is shown each time the TUI starts, for as long as
the version you run is affected. Press `n` on the TUI's tab bar to read every
announcement, or run:

```sh
qsmxt news
```

The last announcements fetched are saved in `~/.qsmxt/announcements.toml`, so
they can be read offline, and the ones already shown are recorded in
`~/.qsmxt/announcements-seen.json`.

## Updates and announcements

When a newer release is available, the TUI says so in a pop-up when it starts: `Enter` updates
QSMxT and `Esc` closes the pop-up until the next start. Announcements from the QSMxT team (for
example a problem found in a release, and the version that fixes it) pop up once; one about a
problem that can give wrong results pops up each time the TUI starts until you update past it.
Only one pop-up opens at a time: important announcements first, then the update, then other
announcements. In an announcement, `↑`/`↓` scroll, `c` copies its link and `Enter` closes it.
Press `n` on the tab bar to read every announcement, newest first, or run `qsmxt news`. Both
checks run in the background and never delay startup; without an internet connection the
announcements saved last time are shown. See
[Keeping up to date](#keeping-up-to-date).
