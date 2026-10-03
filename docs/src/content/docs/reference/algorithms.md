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
| all 11 deep-learning inversions, SUSEP-Net, χ-sepnet, R2PRIMEnet, iQFM | **resampled first** — assumes B0 is +z |

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

R2PRIMEnet is in the third row because it is a χ-sepnet-family network, trained
on the same axial data and taking no B0 direction. It only runs when R2′ is
being estimated rather than measured, so whether a run is resampled on its
account depends on whether that run has an R2 to subtract: a MESE (or
`--custom-r2-tool`) with `--r2prime-strategy auto` measures R2′ instead, and
nothing is resampled. iQFM is there because it is the local-field head of the
same network as iQSM.

A matching MESE acquisition no longer changes any of this. The MESE is resampled
onto whatever grid the run reconstructs on, so an oblique run with a MESE
resamples like any other, and a deep-learning method on one is no longer
refused. The MESE is also no longer required to share the GRE's grid at all: a
2D spin-echo stack with thicker slices or a smaller matrix is reconciled by its
affine, where it used to be dropped and R2′ silently never computed. Where the
MESE's field of view does not cover all of the brain, R2 and a measured R2′ are
reported only inside it, and the log says how much of the mask that was.

## R2′ coverage and field strength

A MESE is often a slab rather than a whole head, because whole-brain MESE is
expensive. R2′ is only defined where R2 exists, so the voxels the MESE did not
reach are left at zero rather than being filled with `R2* − 0 = R2*`, which
would report the whole of R2* as reversible and overstate R2′ by roughly the
tissue R2 (10–20 s⁻¹ at 3 T). Every χ-separation method built on that R2′
inherits the gap, so expect the derived maps to be empty there too. The pipeline
writes where its R2′ is a measurement as `_desc-r2prime_mask.nii`. What should
happen to the uncovered region instead is still open; see
[QSMxT#237](https://github.com/QSMxT/QSMxT/issues/237).

`qsmxt r2prime` applies the same rule standalone. It infers coverage from
`R2 > 0`, which is exact for an R2 fitted on the same grid. An R2 map resampled
from another grid is the case that needs help: interpolation across the slab
boundary leaves small non-zero values just outside the acquired slices, and
nothing in the map distinguishes them from tissue. Pass `--r2-coverage` with a
mask of where the acquisition actually reached, and `--output-coverage` to write
out which voxels ended up measured.

R2PRIMEnet, which predicts R2′ from R2* when there is no R2 to subtract, is
**trained at 3 T only**. R2* scales with field strength and the network takes
none as an input, so on 7 T data it returns a map of the right shape and the
right units with the wrong numbers, which nothing downstream can detect. So away
from 3 T (more than 0.3 T off):

| `--r2prime-strategy` | without `--r2primenet-ignore-field-mismatch` | with it |
| --- | --- | --- |
| `auto` (no R2 to subtract) | R2′ is skipped with a warning | R2PRIMEnet runs, with a warning |
| `r2primenet` | the run stops with an error | R2PRIMEnet runs, with a warning |

Treat any χ-separation built on a non-3 T R2PRIMEnet prediction as
uncalibrated; better, acquire a MESE or supply an R2′ with
`--use-custom-r2prime`. `--r2primenet-patch D H W` sets the sliding-window patch
(default 192 192 128; each a multiple of 16). In the TUI both are under the R2′
source row. The authors publish a 7 T variant that maps 7 T R2* to
3 T-equivalent R2′; it is not implemented yet.

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

With `laplacian`, each echo is unwrapped on its own (the phase is zeroed outside the
mask first, as UK Biobank calls STI Suite's unwrapper) and the echoes are then combined by
`--b0-estimation` / `--b0-weight-type`, the same options as with ROMEO. Phase offset
removal and bipolar correction are not applied, so use it on phase that is already
offset-free (single-channel or prescan-normalised reconstructions, MCPC-3D-S combined
coils).

`--laplacian-solver` picks the Poisson solver (QSM.jl's names):

| Value | Method |
| --- | --- |
| `dct` | Unweighted least squares with a Neumann-boundary DCT solve (Ghiglia & Romero, 1994), as QSM.jl `:dct` (default) |
| `fft` | Schofield & Zhu (2003): sin/cos Laplacian with an FFT Poisson solve on the volume zero-padded by `--laplacian-fft-pad` voxels per side (default 64, UK Biobank's), reproducing STI Suite 3.0's `MRPhaseUnwrap` |

`--laplacian-fft-pad` takes one value for every axis or three (x y z); a config
file's `field_mapping.laplacian_fft_pad` takes a number or a three-element list.
QSMxT's default of 64 is UK Biobank's; STI Suite's own default is 12. In the TUI,
choosing Laplacian unwrapping shows a "Solver" selector, and `fft` a "FFT Pad
(voxels)" field that accepts `64` or `64 64 32`.

## Motion correction

`--motion-correction` (off by default) rigidly co-registers every echo onto a
reference echo before field mapping: the registration is estimated on the
magnitude images (normalised cross-correlation, multi-resolution) and applied to
the phase through the complex signal, so wraps are not interpolated. The mask is
then restricted to voxels every echo still covers, and every later stage (field
mapping, SWI, T2\*/R2\*, inversion) reads the corrected echoes. It needs magnitude
images, and a run without them stops with an error; a single-echo run skips it.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--motion-reference` | `first` | echo the others are aligned onto: `first`, `last` or a 1-based echo number |
| `--no-motion-chain` | chained | register every echo from the identity rather than from its neighbour's solution |
| `--motion-chain-levels` | 3 | pyramid levels for a chained registration |
| `--motion-min-ncc` | 0.3 | NCC below which a registration is reported as suspect (≈0.9 suits repeats) |
| `--registration-levels` | 5 | pyramid levels: the capture range, not to be lowered |
| `--registration-max-samples` | 200000 | voxels sampled per metric evaluation |
| `--registration-max-iterations` | 20 | Powell cycles per level |
| `--registration-max-evaluations` | 5000 | metric evaluations, a hard stop |
| `--registration-tolerance` | 1e-5 | relative improvement that counts as converged |
| `--registration-rotation-step-deg` | 3 | initial rotation step |
| `--registration-translation-step-mm` | 3 | initial translation step |
| `--registration-min-overlap` | 0.25 | smallest overlapping fraction accepted |

The per-echo rotation, translation, largest tissue displacement, NCC, overlap and
B0 direction go to `<run>_motion.json` in the `motion` workflow folder, and the log
summarises them. Field mapping uses the reference echo's B0 direction; the log
reports how far the echoes' B0 directions spread. With `--motion-reference` other
than `first`, a mask made on the first echo's magnitude may sit offset from the
corrected echoes, and QSMxT warns. In the TUI it is the first row of the Field
mapping section, with every setting below it once switched on.

## Echo combination (B0 estimation)

Multi-echo field maps combine the unwrapped echoes with `--b0-estimation`:
`weighted-avg` (default) averages phase/TE across echoes with the weights chosen by
`--b0-weight-type`; `linear-fit` fits phase against TE (with an intercept unless
`--linear-fit-estimate-offset false`).

| `--b0-weight-type` | Weight on phase/TE |
| --- | --- |
| `phase-snr` | magnitude × TE (default) |
| `phase-var` | magnitude² × TE² |
| `average` | uniform |
| `tes` | TE |
| `mag` | magnitude |
| `assumed-decay` | TE² × exp(−TE/T2\*), i.e. TE × exp(−TE/T2\*) on the phase, with an assumed T2\* from `--b0-weight-assumed-t2star` (ms, default 40) |

`assumed-decay` is a fixed per-echo weighting computed from the echo times and an
*assumed* T2\*: every voxel gets the same weights, and neither a T2\* map nor the
magnitude is used. It is UK Biobank's echo-combination weighting, with T2\* = 40 ms for
every participant ([Wang et al. 2022, *Nat. Neurosci.* 25:818](https://doi.org/10.1038/s41593-022-01074-w)):
`--unwrapping-algorithm laplacian --b0-weight-type assumed-decay` (with
`--laplacian-solver fft` for STI Suite's solver) reproduces UK Biobank's field map, i.e.
per-echo Laplacian unwrapping and a TE·exp(−TE/T2\*)-weighted mean of the unwrapped phases.
When magnitude images are available, `phase-snr`, which uses the measured magnitude in each
voxel, is generally preferable.

### Motion-robust echo combination

`--b0-estimation robust-fit` (off by default) is the linear fit made robust to a bad
echo: iteratively reweighted least squares that, voxel by voxel, down-weights echoes
whose phase departs from a straight line in TE, as motion between echoes or local
signal loss leaves. Nothing is dropped; an echo is down-weighted only where it departs.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--robust-fit-loss` | `tukey` | `tukey` rejects an echo past the tuning radius (weight 0); `huber` caps its influence but never rejects |
| `--robust-fit-tuning` | conventional | tuning radius in robust-scale units (Tukey 4.685, Huber 1.345); smaller is more aggressive |
| `--robust-fit-iterations` | 3 | IRLS passes; `0` reports only (the field map is the linear fit's) |
| `--robust-fit-downweight-level` | 0.75 | reporting only: a weight below this counts as down-weighted |
| `--robust-fit-flag-ratio` | 3 | reporting only: flag an echo whose outlier score exceeds this × the best echo's |
| `--linear-fit-estimate-offset` | true | fit a phase-offset intercept (shared with `linear-fit`) |
| `--linear-fit-reliability-threshold` | 90 | percentile of the fit's reliability mask (shared; does not change the field map) |

It needs at least 4 echoes with the offset term, 3 without; fewer is an error rather than
a silent plain fit. Each run writes a per-echo report (down-weighted and rejected
fractions, median weight, median residual, outlier score, flagged echoes) to
`<field map>_robustfit.json` in the unwrap step's workflow folder, and the log names any
flagged echo. In the TUI it is the third "B0 Estimation" option, with each parameter
below it.

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

### V-SHARP and its deconvolution

V-SHARP (Wu et al. 2012) high-passes the field with spherical mean value (SMV)
kernels from `--vsharp-max-radius` (default 12 mm) down to `--vsharp-min-radius`
(default 1 mm), using at each voxel the largest kernel that fits inside the
mask. The published method then **deconvolves** that high-pass with a
truncated SVD (threshold `--vsharp-threshold`) to recover the local field the
SMV filter attenuated.

QSMxT skips the deconvolution by default (`--vsharp-deconvolve false`), as STI
Suite's `V_SHARP` and the UK Biobank QSM pipeline do. The deconvolution is exact
for a noise-free harmonic background, but on real field maps the TSVD inverse
amplifies low-frequency noise; on UK Biobank the longitudinal sensitivity of
iron-rich structures was higher without it, with no significant loss of
test–retest reliability. `--vsharp-deconvolve true` restores the behaviour of
QSMxT up to v9.22.0, and is what to use to compare against cohorts processed
with it. In the TUI it is the "Deconvolve" toggle under V-SHARP; the threshold
row only appears when it is on. The generated methods text says which variant
ran.

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
- STI with fewer than six orientations, or with a coplanar direction set → error.
  Neither is something registration can fix: they are properties of the
  acquisition.

`--multi-orientation-force` overrides the check. Prefer fixing the metadata:
declare `B0_dir` in each orientation's sidecar.

### Registration

COSMOS and STI need the orientations on one grid, and QSMxT puts them there. The
first member of a group is the reference, and every other member is rigidly
co-registered onto it before its local field is resampled across.

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

A correlation below 0.5 is treated as a failed registration and the group is
refused — a bad alignment produces a plausible-looking susceptibility map, which
is the failure this whole feature is built to avoid. Between 0.5 and 0.8 it runs
with a warning; check the per-orientation χ maps, which members produce anyway and
which are the fastest way to see whether the orientations really lined up.

`--no-orientation-registration` switches it off, for orientations you co-registered
externally and do not want interpolated a second time. A group that then still
needs registration is refused, which is what QSMxT did before it could register at
all.

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

The four non-network methods share a relaxometric constant D_r of 137 Hz/ppm,
from the original χ-separation paper (Shin et al. 2021), which was measured by
regressing R2' against a single-orientation QSM. The χ-sepnet paper (Kim et al.
2025) repeated that regression against COSMOS and got 114 Hz/ppm; that value is
baked into χ-sepnet and SUSEP-Net as a training normalisation and is not a
parameter there. If your χ_total is COSMOS-derived, 114 is the better match for
the analytic methods too, and `--chi-sep-ilsqr-dr-pos` and friends set it.
Up to QSMxT v9.22.0 `chi-sep-medi` defaulted to 114/30 while the other three
used 137/137, so results from it are not comparable across that boundary.

Outputs are written as `desc-paramagnetic_Chimap`, `desc-diamagnetic_Chimap` and
`desc-total_Chimap`.

Separation is fitted over the voxels the susceptibility map is defined on — its
support, written as `desc-qsm_mask` (see [Referencing](#referencing)) — not over
the whole brain mask: the rim that background-field removal eroded has no χ to
separate. The methods that also fit the local field are limited to where that
field is defined. R2' and R2\* are restricted to the same voxels before fitting,
and all three outputs are 0 outside them. With a bring-your-own QSM, the support
is the brain mask less the voxels where that map is 0.

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
`--synthseg-version v2`; it is not the same ROI as `ventricles`, and it does not
exist on v1. Pick deliberately — the two give different numbers.
:::

A region reference implies `--do-segmentation`, since the region has to be
measured on a parcellation; `--use-custom-dseg` satisfies that instead. If the
region turns out to have no voxels inside the support for a subject, that
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
