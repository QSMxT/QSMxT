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
- **Refinements:** `erode[:<n>]`, `dilate[:<n>]`, `close[:<r>]`, `fill-holes[:<max>]`,
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

To try combinations on masks you already have, without a pipeline run, use
[`qsmxt mask and` / `qsmxt mask or`](/QSMxT/reference/tools/#masking--qsmxt-mask).

## Oblique acquisitions

The dipole kernel is built in the voxel grid, so an acquisition whose slices
are tilted relative to B0 has to be handled explicitly. QSMxT applies the first
of these that fits:

1. **A `B0_dir` in the JSON sidecar wins.** It describes the acquisition better
   than the affine can.
2. **`--obliquity-threshold <degrees>` resamples to axial** when the obliquity
   exceeds it, as QSMxT 8.x did. Off by default (`-1`).
3. **Otherwise the kernel is rotated** — B0 is taken from the affine and used
   as-is. This is the default, and normally the one you want.

Rotating the kernel is preferred because it is exact and free: the dipole
kernel takes the field direction as a parameter, so pointing it the right way
costs nothing, keeps the acquired grid, and interpolates nothing. Resampling
exists for two reasons — reproducing 8.x output, and the deep-learning methods,
which have no direction input and must be given axial data.

The cost is not small. A 256×288×48 UK Biobank SWI at 32.5° obliquity resamples
to 272×339×77, roughly twice the voxels, and took 459 s against 246 s for the
same reconstruction on the acquired grid. The resampled outputs also land on a
different grid from the input, so any transform you already hold — a FLIRT
matrix, say — no longer describes them.

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

Runs with a matching MESE acquisition (for R2′ / χ-separation) are never
resampled — the MESE is read from BIDS on its own grid, so resampling only the
GRE would leave the two inconsistent. Those runs use the affine-derived B0
direction instead, and say so in the log.

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
