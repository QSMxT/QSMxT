---
title: Reference χ and per-structure statistics
description: "Reference susceptibility to a region, segment, and get per-structure statistics."
---

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

## Segmentation and per-structure statistics

`--do-segmentation` parcellates the brain straight from the GRE magnitude with
[SynthSeg](https://doi.org/10.1016/j.media.2023.102789). It is trained on
synthetic images of randomised contrast, so one set of weights segments a GRE
magnitude directly — no T1w acquisition, and no registration. Outputs are a
FreeSurfer-labelled `_dseg.nii` and the `_dseg.tsv` lookup table that names its
integers. `--use-custom-dseg <TOOL>` brings your own parcellation from
`<bids>/derivatives/<TOOL>/` instead.

`--synthseg-version` picks the weights: `v1` (SynthSeg 1.0, 32 labels, the
default) or `v2`, [SynthSeg 2.0](https://doi.org/10.1073/pnas.2216399120)
(Billot et al., PNAS 2023; 33 labels, adding a general CSF class). Both are the
authors' non-robust segmentation network; SynthSeg 2.0's robust, cortical
parcellation and QC networks are not available. The 2.0 weights come
from the author's own release (they are not in the SynthSeg repository) and are
downloaded on first use like the other networks. With `--use-custom-dseg`, the
version only selects the label table a supplied segmentation is read with.

`--do-analysis` then summarises **every quantitative map the run produced**
inside each structure — `Chimap`, the chi-separation maps, `T2starmap`,
`R2starmap`, `R2map` and `R2primemap` — and writes them as a tab-separated
table:

| Column | Meaning |
| --- | --- |
| `map` | The map summarised, by its BIDS suffix (e.g. `desc-paramagnetic_Chimap`) |
| `unit` | `ppm` for susceptibility, `s` for T2\*, `s-1` for the R2 family |
| `index`, `name` | FreeSurfer label id and structure name |
| `n_voxels` | Voxels the structure occupies in the segmentation |
| `n_valid` | Of those, the voxels the map actually covers — the ones summarised |
| `volume_mm3` | SynthSeg's partial-volume-aware volume (blank for a supplied `dseg`) |
| `mean`, `sd` | Mean and population standard deviation |
| `median`, `min`, `max` | Median and the extremes |
| `p5`, `p95` | 5th and 95th percentiles, nearest-rank (never interpolated) |

One row per map and structure, so the table keeps the same shape whatever the
run produced; pivot it to one-column-per-map in a line of pandas or R if you
prefer that. Voxels a map does not cover are excluded rather than averaged in as
zeros, which is why `n_valid` can be smaller than `n_voxels` — `R2map` only
exists where the spin-echo acquisition reached, for instance.

### Figures

Each table comes with a PNG per map — a ranked horizontal bar chart of the mean
per structure, with ±1 SD whiskers:

- `sub-<X>/anat/sub-<X>_desc-<map>_stats.png` — one run.
- `desc-<map>_stats.png` at the derivatives root — the cohort: each bar is the
  mean of the runs' means, and its whisker is the spread **across runs**, which
  is a different quantity from the per-run whisker (the spread of voxels inside
  one structure). Each figure's subtitle says which. Written only when more than
  one run contributed, since with a single run it would restate that run's
  figure with every whisker at zero.

Colour encodes sign where a map has both — paramagnetic and diamagnetic χ, with
a legend — and is a single hue where it does not, so colour never sits on a
figure encoding nothing. Values are printed only on the extremes; the axis and
the TSV carry the rest.

Two copies of the table are written:

- `sub-<X>/anat/sub-<X>_desc-segmentation_stats.tsv` — one run, next to the
  images it describes.
- `desc-segmentation_stats.tsv` at the top of the derivatives directory — every
  run in the dataset, each row prefixed with its BIDS entities (`subject`,
  `session`, `acquisition`, `reconstruction`, `inversion`, `run`). This is the
  one to open for a group analysis.

The table covers whatever the run produced, so it does not force you to compute
maps you did not ask for: `--no-qsm --do-r2starmap --do-segmentation
--do-analysis` is a valid request and gives you an R2\*/T2\* table with no
susceptibility map in sight. `--do-analysis` requires `--do-segmentation` (unless
you supply a `dseg`), and QSM only when nothing else would produce a map at all;
left off, the run turns them on itself and logs it.
