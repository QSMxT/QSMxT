---
title: Multi-orientation (COSMOS and STI)
description: "Combine acquisitions at several head orientations with COSMOS or STI."
---

Single-orientation dipole inversion is ill-posed because the dipole kernel
vanishes on a pair of cones at the magic angle. Re-imaging the same object at a
different orientation moves those cones, so combining orientations conditions the
inverse problem at acquisition time rather than with a regulariser. COSMOS does
that for a scalar susceptibility; STI keeps the orientations but models
susceptibility as a rank-2 tensor, describing white-matter anisotropy instead of
averaging it away.

Both are off unless you name an **orientation group**.

A run's methods and the dataset's describe the combination only for runs a group
actually combined. Before a run, as in `--dry` and the planned `methods.md`, the text
says "where a session held several head orientations". A browser page holds one scan,
so it has no groups: it clears these settings when it reads a settings file, and tells
you.

## Naming the orientation group

BIDS has no entity meaning "the same object at a different orientation", so
nothing in the dataset can declare the intention to do COSMOS — you do, with
`--orientation-group`. The pattern's only job is to say which runs belong
together, and it takes four forms:

| Form | Example | Use it when |
| --- | --- | --- |
| Entity name | `acq` | Every run with that entity in the session is an orientation |
| Glob over the run key | `*acq-dir*` | The session also holds unrelated acquisitions |
| `re:` + regex with one capture | `re:.*-dir(\d+)_.*` | One session holds *two* orientation sets |
| `runs:` + the groups' run keys | `runs:sub-01_acq-a_run-1_MEGRE,sub-01_acq-a_run-2_MEGRE` | You pick the runs by hand: what the TUI writes when you give runs their groups with ←/→. `,` between runs, `;` between groups, each group's reference first |

The glob form is usually what you want. A session like this:

```
sub-01_ses-01_acq-dir1_MEGRE     ┐
sub-01_ses-01_acq-dir2_MEGRE     ├─ *acq-dir* matches these three
sub-01_ses-01_acq-dir3_MEGRE     ┘
sub-01_ses-01_acq-highres_MEGRE  ← and leaves this one alone
```

groups all four under `acq` and only the intended three under `*acq-dir*`. The
glob language is the same one `--include` and `--exclude` use, and matches against
the full run key (`sub-01_ses-01_acq-dir2_MEGRE`), case-insensitively.

Groups never cross a subject or session, and a pattern matching only one run in a
session is not a group — that run is processed on its own.

`chunk-` and `desc-` are rejected: QSMxT's filename parser does not distinguish
them, so grouping by one would silently merge two orientations into a single run.
Published multi-orientation datasets use `acq-` (for example OpenNeuro
[ds007958](https://openneuro.org/datasets/ds007958), an in-vivo 7T COSMOS set
named `acq-dir1/2/3`).

## B0 directions, and what will be refused

Each orientation contributes its B0 direction **in the common grid's voxel
frame**. There are exactly three ways to know it, and the direction table prints
which one was used:

| Source | When it applies |
|---|---|
| `sidecar` | the orientation's JSON declares `B0_dir`. Authoritative for any dataset shape, and what a curated multi-orientation dataset should ship |
| `affine` | derived from the NIfTI affine. Only meaningful when the orientations were separately prescribed, so the slab followed the head and the affines genuinely differ |
| `registration` | read off the rigid rotation that aligns this orientation to the reference. The object rotates and B0 does not, so that rotation *is* where B0 pointed relative to the common grid |

The affine route breaks on two very common dataset shapes while remaining
perfectly readable:

- the head rotated inside an **identically prescribed slab** — every affine is the
  same, and the anatomy moved in voxel space instead;
- the orientations were **already resampled into a reference frame** before
  distribution, which is how `ds007958` ships.

In both, the affine yields the *same* direction for every orientation. Handed to
COSMOS, N identical directions collapse the closed form to an unregularised
single-orientation inversion — which does not error and does not look obviously
wrong.

The first shape is now recovered rather than refused: the anatomy really did move,
so QSMxT registers the orientations and reads the directions off the rotations
(see [Registration](#registration) below). The second is a genuine dead end —
the orientations were resampled into one frame before you got them, so the images
no longer record the rotation — and it still needs a declared `B0_dir`.

What QSMxT still refuses rather than guesses:

- every direction came from the affines, they agree to within 3°, **and**
  registration could not change that (it was switched off, or it ran and found the
  orientations really are the same) → error naming the likely cause and the fix;
- the directions were *declared* and span less than 3° → error. The dataset is
  stating that the orientations are the same; contradicting it from the images
  would be guessing;
- COSMOS with directions too close to condition it → error. The closed form
  divides by the sum of the orientations' squared dipole kernels, so directions a
  few degrees apart leave the magic-angle cone almost as ill-conditioned as one
  orientation does, and with no regularisation the cone fills with amplified
  noise. QSMxT measures the share of a single orientation's cone (|D| < 0.1) the
  set still leaves ill-conditioned, and refuses above 25%. Three runs of one head
  position, re-registered, leave about 55% (their small movements between runs
  are 1–3°); two orientations pass from about 13° apart, and designed sets leave
  none. The log shows the share for every group
  (`3 orientations, spread 30°, rank 3, 0% of the magic-angle cone uncovered`);
- STI with fewer than six orientations, or with a coplanar direction set → error.
  Neither is something registration can fix: they are properties of the
  acquisition.

`--multi-orientation-force` overrides the check. Prefer fixing the metadata:
declare `B0_dir` in each orientation's sidecar.

## Registration

COSMOS and STI need the orientations on one grid, and QSMxT puts them there. One
member of each group is the reference, and every other member is rigidly
co-registered onto it before its local field is resampled across. The group map is
written on the reference's grid, and referenced and analysed on its segmentation.

By default the reference is the first member by name, which is rarely a deliberate
choice. Name it with `--orientation-reference` (`multi_orientation.reference`): a
glob over the run key, or a word that appears in it. It must match exactly one
member of every group, or the run is refused before it starts.

```bash
qsmxt run bids/ out/ --orientation-group acq --orientation-reference Neutral
```

Choose the neutral head position. Every other orientation is then the smallest
rotation away from it, which keeps registration within its capture range (below),
and the group map comes out in the usual view. With the reference at one extreme,
say chin-down-left, the opposite extreme can be twice as far away as it is from
neutral.

Two things to know about how it works:

- **It registers magnitude, never the field.** A local field is not an anatomical
  image — its contrast is the dipole response, which changes with orientation by
  construction, so correlating two orientations' fields would be matching the very
  thing the reconstruction is trying to measure. A group with no magnitude cannot
  be registered.
- **It only runs when the group needs it.** Already on one grid with declared
  `B0_dir`s, there is nothing to find and an interpolation would only blur, so the
  group passes through untouched. It runs when the members disagree on dimensions
  or affine, or when the only available directions came from affines that cannot
  tell the orientations apart.

The method is 6-DOF rigid (three rotations, three translations), maximising
normalised cross-correlation over a coarse-to-fine pyramid. NCC rather than mutual
information because the inputs are mono-modal — GRE magnitude against GRE
magnitude of one subject — so NCC's invariance to gain and offset is exactly the
invariance needed, at a fraction of the cost. What NCC cannot absorb is the
receive field: it is fixed to the coil, so when the head rotates the shading
rotates with the coil and not with the anatomy. The brain mask keeps most of that
out of the metric; if a group registers poorly, enabling inhomogeneity correction
on the magnitude is the next thing to try.

Each registration is reported, and recorded in the reconstruction's sidecar
alongside the directions it produced:

```
  co-registering: the affines are identical, so they cannot describe the rotations
  registration:
    sub-01_ses-01_acq-dir1  reference
    sub-01_ses-01_acq-dir2  rotation 17.61°  translation [  0.02  -0.11   0.04] mm  NCC 0.9475  overlap 1.00
    sub-01_ses-01_acq-dir3  rotation 24.99°  translation [ -0.05   0.08  -0.02] mm  NCC 0.9475  overlap 1.00
  directions:
    acq-dir1         B0 [ 0.000  0.000  1.000]  registration
    acq-dir2         B0 [-0.223  0.205  0.953]  registration
    acq-dir3         B0 [ 0.037 -0.421  0.906]  registration
    3 orientations, spread 29°, rank 3
```

How far apart the orientations may be: measured on an in-vivo-sized phantom,
registration recovers rotations across the whole range tested, 5° to 70°, to
within 0.01°. That is the search's capture range rather than a property of your
data, and it is set by the pyramid depth — worth knowing because it is finite.
Beyond it the failure is not subtle: the search lands in a local minimum tens of
degrees out, correlation collapses, and the group is refused by the floor below.
If that happens to the orientation farthest from the reference, a more central
`--orientation-reference` is the fix.

A correlation below 0.5 is treated as a failed registration and the group is
refused — a bad alignment produces a plausible-looking susceptibility map, which
is the failure this whole feature is built to avoid. Between 0.5 and 0.8 it runs
with a warning; check the per-orientation χ maps, which members produce anyway and
which are the fastest way to see whether the orientations really lined up.

`--no-orientation-registration` switches it off, for orientations you co-registered
externally and do not want interpolated a second time. A group that then still
needs registration is refused, which is what QSMxT did before it could register at
all.

## Running it

`--multi-orientation-algorithm` picks the reconstruction: `cosmos` (the default)
or `sti`.

```bash
qsmxt run bids/ out/ --orientation-group '*acq-dir*'
qsmxt run bids/ out/ --orientation-group acq --multi-orientation-algorithm sti
```

Check it before committing to a long run — `--dry` prints each group's direction
table, where each direction came from, and the verdict:

```
Multi-orientation (COSMOS), 1 group(s):
  sub-01_ses-01 (3 orientations) [ok]
      acq-dir1         B0 [ 0.000  0.000  1.000]  sidecar
      acq-dir2         B0 [ 0.000  0.500  0.866]  sidecar
      acq-dir3         B0 [ 0.500  0.000  0.866]  sidecar
      3 orientations, spread 41°, rank 3
```

A group that will be co-registered says so, and flags that the directions shown
are placeholders — they came off the affines and the run will replace them with
ones recovered from the rotations:

```
Multi-orientation (COSMOS), 1 group(s):
  sub-01_ses-01 (3 orientations) [will co-register]
      acq-dir1         B0 [ 0.000  0.000  1.000]  affine
      acq-dir2         B0 [ 0.000  0.000  1.000]  affine
      acq-dir3         B0 [ 0.000  0.000  1.000]  affine
      directions above are from the affines; they will be replaced by ones recovered from the registration against sub-01_ses-01_acq-dir1
```

`--dry` compares the *source* headers, while the run compares the derivative grids
it actually produces. Those differ when a stage resamples — an oblique acquisition
taken to an axial grid, say — so the forecast can be wrong, but only in the safe
direction: the decision is remade on the real grids, so a group that turns out to
need registration gets it.

In the [TUI](/QSMxT/background/tui/) the same pattern is the
**Orientations** field on the Input tab, below Include/Exclude, and it previews
the groups as you type.

`--multi-orientation-lambda` sets the regularisation: 0 (the default) is plain
least squares and is right whenever three or more orientations cover k-space; 1e-3
to 1e-2 helps with two orientations, or when the rotations share a single axis and
streaking survives.

`--cosmos-magnitude-weighting` (TUI: QSM → COSMOS/STI → COSMOS
Magnitude Weighting) weights each orientation's field by its combined magnitude, so
low-signal voxels count for less, and solves by conjugate gradient; `--cosmos-tol`
and `--cosmos-max-iter` (default 1e-6 and 100) set that solve, and are refused
without the weighting, where COSMOS is solved in closed form and would ignore them.

Each orientation contributes its *local* field, which only background-field
removal writes. A grouping with an inversion that has no such step (TFI, TGV,
QSMART, AutoQSM, NeXtQSM, iQSM, iQSM+) or with `--no-qsm` is refused
before processing.

## Outputs

Each member run still produces its own single-orientation `Chimap` — the first
thing to check when a COSMOS map looks wrong is whether the orientations were
actually aligned, and the per-orientation maps are how you see that. The combined
result drops the entity that varied:

| File | Contents |
| --- | --- |
| `…_desc-cosmos_Chimap.nii` | COSMOS susceptibility map (ppm) |
| `…_desc-mms_Chimap.nii` | STI mean magnetic susceptibility |
| `…_desc-msa_Chimap.nii` | STI magnetic susceptibility anisotropy |
| `…_desc-sti_Chimap.nii` | The STI tensor (ppm), 4D: the six independent components X11 X12 X13 X22 X23 X33 along the fourth axis, listed in its sidecar as `TensorComponents` |
| `…_desc-sti_evec.nii` | Principal eigenvector, 4D (i, j, k in voxel coordinates); `evec` is not a BIDS suffix and is listed in `.bidsignore` |

A `…_desc-cosmos_Chimap.json` sidecar records the source runs, the direction table
and where each direction came from — the half of the input that does not live in
the NIfTI files, and without which the result cannot be reproduced.

The COSMOS map is treated like any other susceptibility map:

- it is referenced with `--qsm-reference` over the voxels every orientation's
  background-removal mask covers (a region reference uses the reference
  orientation's segmentation), and the sidecar records the reference and offset;
- under `--output-space acquired` it is written on the reference orientation's
  acquired grid, the grid that orientation's own outputs are on;
- it is cached: a re-run reuses it until a member's local field, the directions or
  a setting changes;
- it is in BIDS-Prov, in `desc-reference_summary.tsv`, and, with `--do-analysis`,
  gets a per-structure table on the reference orientation's segmentation that the
  dataset-level `desc-segmentation_stats.tsv` includes.

The STI outputs are not referenced (a tensor has no single offset), and stay on the
working grid: returning a tensor to an oblique acquired grid needs it rotated as well
as resampled, which is not implemented, so a warning says so.

## Standalone

If you already have local field maps, skip BIDS entirely:

```bash
qsmxt invert cosmos -i f1.nii -i f2.nii -i f3.nii \
  --b0-direction 0 0 1 --b0-direction 0 0.5 0.866 --b0-direction 0.5 0 0.866 \
  --mask mask.nii --output cosmos.nii

qsmxt invert sti -i f1.nii … -i f6.nii --b0-dirs dirs.txt --mask mask.nii --output sti
```

`--b0-dirs` takes a text file with one `x y z` per line, in `--input` order.
