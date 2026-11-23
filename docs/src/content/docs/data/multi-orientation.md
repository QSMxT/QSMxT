---
title: Multiple head orientations
description: "The same head scanned at several orientations, for COSMOS or STI."
---

Scanning the same head at several orientations to the magnetic field lets QSMxT reconstruct
COSMOS (a better-conditioned χ map) or STI (a susceptibility tensor).

## What QSMxT does automatically

- Every scan still gets its own single-orientation χ map.
- Once you name which scans belong together (below), it combines them into one map per
  subject and session.
- It finds each orientation's field direction from `B0_dir` in the sidecar, from the image
  header, or, when the head was rotated inside the same slab, by registering the orientations
  to each other.
- It refuses, with a message, a group whose directions are too close to help, rather than
  return a map that looks fine but isn't.

## What it doesn't do / limits

- BIDS has no way to say "the same head at another orientation", so QSMxT doesn't group scans
  until you tell it to.
- Orientations already resampled into one frame before you got them no longer record their
  rotation: they need `B0_dir` in each sidecar.
- STI needs at least six orientations.
- The combination needs a background-removal step, so it doesn't work with inversions that
  skip it (TGV, TFI, QSMART and others).

## What you can do

1. Name the orientations so they share a label, for example `acq-dir1`, `acq-dir2`,
   `acq-dir3`.
2. Check the groups and their directions without running:

   ```sh
   qsmxt run study/bids --orientation-group '*acq-dir*' --dry
   ```

3. Run with the same `--orientation-group`. Add `--multi-orientation-algorithm sti` for STI.

In the TUI: the **Orientations** field on the Input tab.

The result is `desc-cosmos_Chimap` (or the `desc-sti_` maps) beside the single-orientation maps.

Every grouping form, what is refused and why, and the registration are in
[Multi-orientation (COSMOS and STI)](/QSMxT/reference/multi-orientation/).
