---
title: How the pipeline works
description: "The stages every run goes through, and which methods skip which stages."
---

## The stages

Each run flows through the pipeline below. The method at every stage is
configurable — see the [Methods index](/QSMxT/reference/methods/).

1. **Masking** — generate a brain/region mask (`--mask-preset`, or custom
   `--mask` sections).
2. **Phase offset removal** — remove receiver phase offsets on multi-echo data.
3. **Phase unwrapping** — `romeo`, `laplacian`, `prelude` or `medi-region-growing`.
4. **Echo combination / B0 mapping** — combine echoes into a field map
   (`--b0-estimation`, `--b0-weight-type`).
5. **Background field removal** — `vsharp`, `pdf`, `lbv`, and more.
6. **Dipole inversion** — `ilsqr` (default), `rts`, `tv`, `tgv`, `medi`, …, and HEIDI streaking reduction after them.
7. **Referencing** — reference the susceptibility values (e.g. to the mean).

Not every inversion runs every stage: total-field methods (TFI, QSMART, AutoQSM,
NeXtQSM, multi-echo TGV) skip background field removal,
and the wrapped-phase methods (iQSM, iQSM+, single-echo TGV) skip unwrapping and
echo combination too. See the [Methods index](/QSMxT/reference/methods/#dipole-inversion).

## Referencing

A susceptibility map has no absolute zero — only differences within it mean
anything — so every map is referenced to something. Choose what with
`--qsm-reference`:

| Value | What zero means |
| --- | --- |
| `mean` (default) | The mean χ over the voxels the map is defined on |
| `none` | Nothing is subtracted; raw reconstruction values |
| a region | The mean χ of a SynthSeg structure, over the voxels the map is defined on |

Every reference is measured over the map's **support** — the voxels the
reconstruction actually produced — not over the whole brain mask. Background-field
removal erodes the mask (V-SHARP by its kernel radius, a few millimetres), and
some inversions erode further (TGV by its `erosions`); the map has no values on
that rim. The final `Chimap` is 0 outside the support, and the support itself is
written beside it as `desc-qsm_mask` — a 0 in the map outside that mask means
"not reconstructed", not χ = 0. For a two-pass run it is the union of both
passes' supports.

`mean` needs no extra information, but it moves with whatever else is in the
mask: two subjects with different amounts of iron-rich tissue in the field of
view get different zeros. Referencing to a structure pins zero to tissue
instead, which is what makes values comparable between subjects and studies.

### Naming a region

```bash
qsmxt run bids/ --qsm-reference thalamus            # both sides merged
qsmxt run bids/ --qsm-reference left-thalamus       # one side
qsmxt run bids/ --qsm-reference cerebral-white-matter
qsmxt run bids/ --qsm-reference ventricles          # a named composite
qsmxt run bids/ --qsm-reference 4                   # a raw FreeSurfer id
qsmxt run bids/ --qsm-reference left-caudate,right-putamen   # your own list
```

A term is a structure name with both sides merged, one sided label, a raw
FreeSurfer id, or the `ventricles` composite (ids 4, 5, 14, 15, 43, 44 — the CSF
spaces a QSM paper usually means by "CSF reference"). Comma-separate terms to
use their union, so `thalamus` and `left-thalamus,right-thalamus` are the same
reference. Slugs are the parcellation's own label names, lowercased with spaces
as hyphens; an unrecognised name lists the valid ones rather than guessing.

:::caution[`csf` is a label, not the composite]
SynthSeg 2.0 has a label of its own named `csf` (id 24), which covers
extraventricular CSF too. `--qsm-reference csf` means *that label* and needs
`--synthseg-version v2` (see below); it is not the same ROI
as `ventricles`, and it does not exist on v1. Pick deliberately — the two give
different numbers.
:::

A region reference requires `--do-segmentation`, since the region has to be
measured on a parcellation; `--use-custom-dseg` satisfies that instead. Left off,
the run turns segmentation on itself and logs it; the TUI offers it as a required
change when the reference is chosen. If the
region turns out to have no voxels inside the support for a subject, that
subject's run **fails** rather than falling back to `mean` — a map that was
quietly referenced to something else looks fine and cannot be compared with the
rest of the cohort.

In the TUI the setting is **Reconstruction › QSM › Reference → QSM reference**. Picking a region
there turns segmentation on: **Other outputs › Segmentation** then reads as ticked,
with its source (SynthSeg or a dseg from derivatives) and settings.
