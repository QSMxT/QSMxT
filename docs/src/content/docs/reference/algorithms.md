---
title: Algorithms
description: The masking, unwrapping, background-field-removal, and dipole-inversion methods available in QSMxT.
---

Every stage of the pipeline is configurable. The numerical methods are provided
by [QSM.rs](https://github.com/astewartau/QSM.rs); QSMxT orchestrates them over
your BIDS data. Below are the valid values for each algorithm option.

## Masking

Choose a masking recipe with `--mask-preset`, and override the image it
operates on with `--masking-input`.

| Value | Method |
| --- | --- |
| `robust-threshold` | Otsu thresholding of the phase-quality map, refined with dilation, hole-filling and erosion (default) |
| `bet` | Brain Extraction Tool on the magnitude image |
| `hd-bet` | HD-BET deep-learning brain extraction on the magnitude image, followed by signal-gated erosion (the QSM-CI harmonization masking). Needs a deep-learning build; the weights (~120 MB, CC-BY-NC-4.0) are downloaded on first use |
| `bet-and-phase` | BET on the first-echo magnitude intersected with an Otsu-thresholded phase-quality map, then hole-filled and eroded (the masking recommended by the [ISMRM EMTP study group consensus](https://doi.org/10.1002/mrm.30006)) |

**Masking input** (`--masking-input`): `magnitude-first`, `magnitude`,
`magnitude-last`, `phase-quality`. For example,
`--mask-preset robust-threshold --masking-input magnitude` thresholds the
combined magnitude image instead of the phase-quality map.

For full control, compose custom mask sections with `--mask` (repeatable),
e.g. `--mask magnitude,bet:0.5,erode:2`. Each section names its input
followed by a generator and refinement operations.

- **Generators:** `threshold[:otsu|fixed:<v>|percentile:<p>]`, `bet[:<f>]`, and
  `hd-bet[:<X>x<Y>x<Z>|low-memory][:tta]`. HD-BET runs 192×192×96 sliding-window
  patches by default (about 4.5 GB peak). `low-memory` uses 128×128×64 patches
  (about 1.9 GB peak), and `tta` adds mirroring test-time augmentation, which is
  roughly 8× slower.
- **Refinements:** `erode[:<n>]`, `dilate[:<n>]`, `close[:<r>]`, `fill-holes[:<max>]`
  (`0`, or omitted, is automatic: holes up to 5% of the volume),
  `gaussian[:<sigma_mm>]`, and
  `signal-erode[:<threshold>[:<depth_cap>[:<global_erosions>[:<bias_sigma>[:<min_component>]]]]]`.
  `signal-erode` removes only boundary voxels whose bias-corrected magnitude is
  below `threshold` × the in-mask median (default 0.80). It works inward through
  sinus and skull-base signal dropout, never more than `depth_cap` voxels deep
  (default 5), after `global_erosions` plain erosions (default 1). Dark interior
  structures are never removed. `bias_sigma` (default 12) is the Gaussian scale
  in voxels of the receive-coil bias divided out first, and `min_component`
  (default 1000) keeps every connected component that size or larger rather than
  only the largest. In the TUI each of these is its own row under the step, so
  you can nudge them with ←/→ and watch the generated command update.

  `global_erosions` is not the same as putting an `erode` step in front of
  `signal-erode`: the gate and the coil-bias estimate are computed from the
  mask as it arrives, and the depth cap is measured from *that* surface, so
  these erosions spend the depth budget and leave the gate unchanged. An `erode`
  step beforehand shrinks the mask the gate is derived from and resets the depth
  budget. The default of 1 is the QSM-CI harmonization setting.

For example, `--mask magnitude,hd-bet:low-memory,signal-erode` is the `hd-bet`
preset with the low-memory patch size.

### Combining sections

With more than one `--mask` section, `--mask-combine` decides how they fold
together: `or` (the default) keeps a voxel any section keeps, and `and` keeps
only voxels every section keeps. `--mask-refine` (repeatable) then applies
refinement operations to the combined mask — which is where hole-filling
belongs, since filling a section's holes before an intersection is not the same
as filling the intersection's.

The `bet-and-phase` preset is exactly this:

```bash
qsmxt run bids/ \
  --mask magnitude-first,bet:0.50 \
  --mask phase-quality,threshold:otsu \
  --mask-combine and \
  --mask-refine fill-holes:0 \
  --mask-refine erode:1
```

BET bounds the head, the phase-quality threshold drops voxels whose phase
cannot be unwrapped reliably, and the holes their intersection leaves inside
the brain are filled afterwards.

To try this on files without a pipeline run, use
[`qsmxt mask preset bet-and-phase`, or `qsmxt mask and` / `or`](/QSMxT/reference/tools/#masking--qsmxt-mask)
on masks you already have.

## Oblique acquisitions

The dipole kernel is built in the voxel grid, so an acquisition whose slices
are tilted relative to B0 has to be handled explicitly. QSMxT applies the first
of these that fits:

1. **A deep-learning method forces resampling to axial.** These learned the
   dipole relationship from axially-acquired data and take no direction
   argument, so nothing else can help them.
2. **A `B0_dir` in the JSON sidecar wins.** It describes the acquisition better
   than the affine can.
3. **`--obliquity-threshold <degrees>` resamples to axial** when the obliquity
   exceeds it, as QSMxT 8.x did. Off by default (`-1`).
4. **Otherwise the kernel is rotated** — B0 is taken from the affine and used
   as-is. This is the default, and normally the one you want.

Rotating the kernel is preferred because it is exact and free: the dipole
kernel takes the field direction as a parameter, so pointing it the right way
costs nothing, keeps the acquired grid, and interpolates nothing.

The cost of the alternative is not small. A 256×288×48 UK Biobank SWI at 32.5°
obliquity resamples to 272×339×77, roughly twice the voxels, and took 459 s
against 246 s for the same reconstruction on the acquired grid. Resampled
outputs are returned to the acquired grid afterwards unless you ask otherwise;
see [Output space](#output-space).

### Which methods need axial data

| | behaviour on oblique data |
|---|---|
| all 18 classical dipole inversions, PDF, χ-sep iLSQR and MEDI | correct as acquired — B0 is an explicit parameter |
| SHARP, V-SHARP, RESHARP, iSMV, LBV, HARPERELLA, iHARPERELLA, BFRnet | unaffected — never uses B0 |
| all 11 deep-learning inversions, SUSEP-Net, χ-sepnet | **resampled first** — assumes B0 is +z |

Background removal by the spherical mean value property is genuinely
direction-independent rather than merely untested, which is why PDF is the only
background remover in the first row.

`qsmxt validate` reports what a dataset will do:

```
B0 direction: (0.05, 0.39, 0.92) (from the affine)
Obliquity:    32.5° (B0 22.9° off the slice normal)
```

Obliquity is `nibabel`'s definition (the norm of the per-axis angles), so 8.x
thresholds carry over. It is a combined measure rather than a tilt: a single
23° oblique acquisition scores ≈32° because two voxel axes move. The tilt in
brackets is the physical angle between B0 and the slice normal, which is what
the kernel cares about.

:::caution
Wrapped phase cannot be resampled on its own. Halfway between `+3.0` and `−3.0`
rad a linear interpolator returns `0.0`, where the answer is near `±π`, so
every wrap becomes a band of wrong values. The pipeline always resamples
magnitude and phase together; if you use `qsmxt resample` by hand, pass
`--phase` with `--magnitude` rather than resampling phase as a plain volume.
:::

Runs with a matching MESE acquisition (for R2′ / χ-separation) are not
resampled by the obliquity threshold — the MESE is read from BIDS on its own
grid, so moving only the GRE would leave the two inconsistent. Those runs use
the affine-derived B0 direction instead, and say so in the log. A
deep-learning method on an oblique run with a MESE has no good answer available
and is refused rather than guessed at.

## Output space

A run that was resampled to axial reconstructs on the resampled grid, and by
default its derivatives are returned to the grid the data arrived on. This
matters more than it sounds: a FLIRT matrix is defined in a space derived from
the image's dimensions and voxel sizes, so applying one to a volume on a
different grid gives a wrong registration rather than an error.

- `--output-space acquired` (default) writes derivatives on the acquired grid.
  Costs a second interpolation, so the result is slightly smoother than a
  reconstruction that was never resampled.
- `--output-space working` leaves them on the resampled grid.

Masks travel by nearest neighbour and phase travels with its magnitude through
the complex domain, for the same reason phase went out that way.

## Grid padding and cropping

Both of these trade reconstruction cost against the grid the FFT stages run on,
and both are off by default.

### `--fft-padding`

FFT cost is `O(N log N)` in the whole grid, and the transform is much faster on
sizes built from small prime factors. Resampling has no reason to land on one:
the UK Biobank grid above comes out 272×339×77, which is 2⁴·17, 3·113 and 7·11.
Growing it to 280×343×80 took one FFT from 131 ms to 71 ms for 8% more voxels.

Padding discards nothing, but it is **not** a no-op. The dipole kernel is
sampled at `k = n / (N·Δx)`, so changing `N` changes where k-space is sampled
and the deconvolution with it. Measured end to end on that acquisition, against
the same run unpadded:

| inversion | median difference | p99 | as % of dynamic range |
|---|---|---|---|
| TKD (direct) | 4e-7 ppm | 9.4e-3 ppm | 0.000% / 5.7% |
| WH-QSM (iterative) | 8.3e-5 ppm | 3.7e-3 ppm | 0.06% / 2.8% |

A direct inversion is unchanged for the typical voxel and moves only where the
deconvolution is ill-conditioned. An iterative one moves slightly everywhere,
since it is solving on a different grid. Neither is evidence that padding is
worse — a finer k-space sampling with the boundary further from the object has
reason to be better — but nothing here establishes that it is better, so it does
not change anyone's numbers unless asked for.

Note that an acquisition usually arrives on a friendly grid already: 256×288×48
is 2⁸, 2⁵·3² and 2⁴·3. Padding earns its keep on grids that resampling created.

### `--crop-to-mask`

Reconstructing only inside a box around the brain cut background removal from
11.5 s to 7.2 s and inversion from 6.0 s to 3.0 s on the same acquisition. But
the FFT stages were 5% of wall time there — HD-BET masking was 289 s of 338 s —
so the end-to-end saving was inside noise. Use it for grids with large empty
regions, not as a general speedup.

`--crop-margin-mm` defaults to 32. At 16 mm the reconstruction differed from the
uncropped one by 0.8% of dynamic range at the median and 5.1% at p99; at 32 mm
and 64 mm it was identical. The margin must also clear the largest SMV kernel in
use, and V-SHARP defaults to 12 mm.

## Coil combination

Runs whose phase is stored per receive coil (`coil-<NN>` entity, see
[Inputs](/QSMxT/reference/inputs/#uncombined-receive-coils)) are combined with
**MCPC-3D-S** (Eckstein et al., MRM 2018) before masking. The coil-summed
Hermitian inner product of the first two echoes gives the field evolution with the
coil phase offsets cancelled; it is unwrapped once (with the `--unwrapping-algorithm`
method), each coil's offset is estimated from its first echo and smoothed with
`--coil-combination-sigma` (default `10 10 5` voxels; the same masked box-filter
smoothing as MriResearchTools / QSMxT 8.x, so combined phase matches the 8.x
converter), and the channels are summed coherently with magnitude² weights. Multi-echo phase-offset removal in the field-mapping stage then runs on
the combined echoes as usual.

## Phase unwrapping

Set with `--unwrapping-algorithm`.

| Value | Method |
| --- | --- |
| `romeo` | Rapid Opensource Minimum-spanning-tree Echo Optimisation (default) |
| `laplacian` | Laplacian-based unwrapping |

## Background field removal

Set with `--bf-algorithm`.

| Value | Method |
| --- | --- |
| `vsharp` | Variable-kernel SHARP (default) |
| `pdf` | Projection onto Dipole Fields |
| `lbv` | Laplacian Boundary Value |
| `ismv` | Iterative Spherical Mean Value |
| `sharp` | Sophisticated Harmonic Artifact Reduction |
| `resharp` | Regularization-enabled SHARP |
| `harperella` | HARPERELLA |
| `iharperella` | Iterative HARPERELLA |

## Dipole inversion

Set with `--qsm-algorithm`. Two families are available: classical/iterative
reconstructions and deep-learning networks.

### Classical & iterative

| Value | Method |
| --- | --- |
| `rts` | RTS — Rapid Two-Step (default) |
| `tv` | Total Variation (TV-ADMM) |
| `tkd` | TKD — Thresholded K-space Division |
| `tsvd` | TSVD — Truncated Singular Value Decomposition |
| `tgv` | TGV — Total Generalized Variation |
| `tikhonov` | Tikhonov regularization |
| `nltv` | NLTV — Nonlinear Total Variation |
| `medi` | MEDI — Morphology-Enabled Dipole Inversion |
| `tfi` | TFI — Total Field Inversion |
| `ilsqr` | iLSQR |
| `lsqr` | Minimally regularised LSQR — HEIDI's seed |
| `heidi` | HEIDI — Homogeneity Enabled Incremental Dipole Inversion |
| `qsmart` | QSMART two-stage reconstruction |
| `ndi` | NDI — Nonlinear Dipole Inversion |
| `fansi` | FANSI — Nonlinear TV |
| `fansi-tgv` | FANSI — Nonlinear TGV |
| `l1qsm` | L1-QSM — L1 data fidelity |
| `whqsm` | WH-QSM — Weak-Harmonic |
| `hdqsm` | HD-QSM — Hybrid data fidelity |
| `amp-pe` | AMP-PE — Approximate Message Passing with Parameter Estimation |

### Deep learning

| Value | Method | Tileable |
| --- | --- | --- |
| `xqsm` | xQSM | ✓ |
| `qsmnet` | QSMnet | ✓ |
| `qsmnet-plus` | QSMnet+ | ✓ |
| `ir2qsm` | IR2QSM — unrolled | ✓ |
| `lpcnn` | LPCNN — learned-proximal | ✓ |
| `modl-qsm` | MoDL-QSM — model-based | ✓ |
| `nextqsm` | NeXtQSM — single-step | ✓ |
| `autoqsm` | AutoQSM — single-step (native patching) | |
| `qsmgan` | QSMGAN — GAN-refined (native patching) | |
| `iqsm` | iQSM — end-to-end from phase | |
| `iqsm-plus` | iQSM+ — orientation-adaptive end-to-end | |

Model weights are downloaded automatically on first use (and cached) from the
QSMxT weight registry on [Hugging Face](https://huggingface.co/qsmxt/qsm-onnx-weights).
Deep-learning support requires a build with the default `dl` feature; `iqsm`
and `iqsm-plus` reconstruct susceptibility directly from phase, so they replace
the background-removal + inversion stages rather than running after them.

:::tip
Not sure which to pick? The defaults (`threshold` → `romeo` → `vsharp` → `rts`)
are a robust, fast starting point for human brain GRE data. Use the
[TUI](/QSMxT/guides/running-interactively/) to experiment interactively.
:::

### Overlap-tiling for deep-learning inversion

Deep-learning inversions can run **overlap-tiled** to bound peak memory: the
volume is split into cubic patches (each a core plus a context halo), inferred
independently, and stitched back together. This is opt-in and applies to the
tileable networks above.

| Flag | Meaning |
| --- | --- |
| `--tile-size <N>` | Output core size per patch, in voxels — **presence enables tiling** |
| `--tile-halo <N>` | Context margin per side, in voxels (default 8) |

```bash
qsmxt run bids/ output/ --qsm-algorithm xqsm --tile-size 128 --tile-halo 8
```

Tiling is an approximation of the whole-volume network — results are close but
not bit-identical. Omit `--tile-size` to run the network over the whole volume.
The flags are ignored by classical algorithms and by the natively-patched
networks (`autoqsm`, `qsmgan`).

## Per-algorithm parameters

Each algorithm exposes its own parameters (regularisation weights, kernel sizes,
iteration counts, …). The easiest way to discover them — and to build a run
command — is the [TUI](/QSMxT/guides/running-interactively/): it exposes every
option with sensible defaults and shows the equivalent `qsmxt run` command live as
you change them. You can also set them in a
[configuration file](/QSMxT/reference/configuration/), or run the matching
[standalone tool](/QSMxT/reference/tools/) (e.g. `qsmxt invert tgv --help`) to
experiment directly.

## Multi-orientation (COSMOS and STI)

Single-orientation dipole inversion is ill-posed because the dipole kernel
vanishes on a pair of cones at the magic angle. Re-imaging the same object at a
different orientation moves those cones, so combining orientations conditions the
inverse problem at acquisition time rather than with a regulariser. COSMOS does
that for a scalar susceptibility; STI keeps the orientations but models
susceptibility as a rank-2 tensor, describing white-matter anisotropy instead of
averaging it away.

Both are off unless you name an **orientation group**.

### Naming the orientation group

BIDS has no entity meaning "the same object at a different orientation", so
nothing in the dataset can declare the intention to do COSMOS — you do, with
`--orientation-group`. The pattern's only job is to say which runs belong
together, and it takes three forms:

| Form | Example | Use it when |
| --- | --- | --- |
| Entity name | `acq` | Every run with that entity in the session is an orientation |
| Glob over the run key | `*acq-dir*` | The session also holds unrelated acquisitions |
| `re:` + regex with one capture | `re:.*-dir(\d+)_.*` | One session holds *two* orientation sets |

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

### B0 directions, and what will be refused

Each orientation contributes its B0 direction **in the common grid's voxel
frame**. QSMxT takes it from a sidecar `B0_dir` when one is present, and otherwise
derives it from the NIfTI affine.

The affine is only meaningful when the orientations were separately prescribed, so
the slab followed the head and the affines genuinely differ. Two very common
dataset shapes break that while leaving the affine perfectly readable:

- the head rotated inside an **identically prescribed slab** — every affine is the
  same, and the anatomy moved in voxel space instead;
- the orientations were **already resampled into a reference frame** before
  distribution, which is how `ds007958` ships.

In both, the affine yields the *same* direction for every orientation. Handed to
COSMOS, N identical directions collapse the closed form to an unregularised
single-orientation inversion — which does not error and does not look obviously
wrong. So QSMxT refuses rather than guesses:

- every direction came from the affines and they agree to within 3° → error naming
  the likely cause and the fix;
- the directions differ but span less than 3° → error;
- STI with fewer than six orientations, or with a coplanar direction set → error.

`--multi-orientation-force` overrides the check. Prefer fixing the metadata:
declare `B0_dir` in each orientation's sidecar.

### Registration

QSMxT has no registration of its own, so **the orientations must already sit on
one common grid**. Disagreeing dimensions or affines are an error, not a guess.
Co-register the orientations first (and, if your tool reports the rotation, write
the resulting B0 directions into each sidecar's `B0_dir`).

### Running it

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

In the [TUI](/QSMxT/guides/running-interactively/) the same pattern is the
**Orientations** field on the Input tab, below Include/Exclude, and it previews
the groups as you type.

`--multi-orientation-lambda` sets the regularisation: 0 (the default) is plain
least squares and is right whenever three or more orientations cover k-space; 1e-3
to 1e-2 helps with two orientations, or when the rotations share a single axis and
streaking survives.

### Outputs

Each member run still produces its own single-orientation `Chimap` — the first
thing to check when a COSMOS map looks wrong is whether the orientations were
actually aligned, and the per-orientation maps are how you see that. The combined
result drops the entity that varied:

| File | Contents |
| --- | --- |
| `…_desc-cosmos_Chimap.nii` | COSMOS susceptibility map (ppm) |
| `…_desc-mms_Chimap.nii` | STI mean magnetic susceptibility |
| `…_desc-msa_Chimap.nii` | STI magnetic susceptibility anisotropy |
| `…_desc-sti11_Chitensor.nii` … `sti33` | The six independent tensor components |
| `…_desc-pevx_Chitensor.nii` … `pevz` | Principal eigenvector, in voxel coordinates |

A `…_desc-cosmos_Chimap.json` sidecar records the source runs, the direction table
and where each direction came from — the half of the input that does not live in
the NIfTI files, and without which the result cannot be reproduced.

### Standalone

If you already have local field maps, skip BIDS entirely:

```bash
qsmxt invert cosmos -i f1.nii -i f2.nii -i f3.nii \
  --b0-direction 0 0 1 --b0-direction 0 0.5 0.866 --b0-direction 0.5 0 0.866 \
  --mask mask.nii --output cosmos.nii

qsmxt invert sti -i f1.nii … -i f6.nii --b0-dirs dirs.txt --mask mask.nii --output sti
```

`--b0-dirs` takes a text file with one `x y z` per line, in `--input` order.

## Susceptibility source separation

Enable with `--do-chisep` to split susceptibility into paramagnetic and
diamagnetic components, and choose the method with `--chisep`.

| Value | Method |
| --- | --- |
| `r2star-qsm` | R2\*-based magnitude decay modelling (default) |
| `decompose` | DECOMPOSE-QSM |
| `chi-sep-ilsqr` | χ-separation, iLSQR inversion |
| `chi-sep-medi` | χ-separation, MEDI inversion |
| `wavesep` | Wavelet-based single-step separation |
| `hc-chisep` | Hollow-cylinder χ-separation |
| `susep-net` | SUSEP-Net (deep learning) |
| `chi-sepnet` | χ-sepnet (deep learning) |

Only `r2star-qsm` and `decompose` run on a GRE acquisition alone. The rest are
based on R2', so they also need a multi-echo spin-echo acquisition or a
bring-your-own R2' map. See [Input data](/QSMxT/reference/inputs/).

Outputs are written as `desc-paramagnetic_Chimap`, `desc-diamagnetic_Chimap` and
`desc-total_Chimap`.

## Referencing

A susceptibility map has no absolute zero — only differences within it mean
anything — so every map is referenced to something. Choose what with
`--qsm-reference`:

| Value | What zero means |
| --- | --- |
| `mean` (default) | The mean χ inside the brain mask |
| `none` | Nothing is subtracted; raw reconstruction values |
| a region | The mean χ of a SynthSeg structure |

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
`--synthseg-version v2`; it is not the same ROI as `ventricles`, and it does not
exist on v1. Pick deliberately — the two give different numbers.
:::

A region reference implies `--do-segmentation`, since the region has to be
measured on a parcellation; `--use-custom-dseg` satisfies that instead. If the
region turns out to have no voxels inside the brain mask for a subject, that
subject's run **fails** rather than falling back to `mean` — a map that was
quietly referenced to something else looks fine and cannot be compared with the
rest of the cohort.

In the TUI the same setting appears in two places, editing one value: **QSM →
Referencing → QSM Reference**, and **Supplementary → QSM Reference**, beside the
SynthSeg settings it depends on. Picking a region there ticks *Segment
(SynthSeg)* and reveals its settings.

## Segmentation and per-structure statistics

`--do-segmentation` parcellates the brain straight from the GRE magnitude with
[SynthSeg](https://doi.org/10.1016/j.media.2023.102789). It is trained on
synthetic images of randomised contrast, so one set of weights segments a GRE
magnitude directly — no T1w acquisition, and no registration. Outputs are a
FreeSurfer-labelled `_dseg.nii` and the `_dseg.tsv` lookup table that names its
integers. Use `--synthseg-version v2` for the better (separately downloaded)
model, or `--use-custom-dseg <TOOL>` to bring your own parcellation from
`<bids>/derivatives/<TOOL>/`.

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
susceptibility map in sight. `--do-analysis` implies `--do-segmentation` (unless
you supply a `dseg`), and implies QSM only when nothing else would produce a map
at all.
