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
| `bet` | Brain Extraction Tool on the magnitude image, then two erosions |
| `hd-bet` | HD-BET deep-learning brain extraction on the magnitude image, followed by signal-gated erosion (the QSM-CI harmonization masking). Needs a deep-learning build; the weights (~120 MB, CC-BY-NC-4.0) are downloaded on first use |
| `bet-and-phase` | BET on the first-echo magnitude intersected with the `robust-threshold` mask of the phase-quality map, then hole-filled and eroded (the masking recommended by the [ISMRM EMTP study group consensus](https://doi.org/10.1002/mrm.30006)) |
| `rs2-net` | RS2-Net deep-learning **rodent** brain extraction (Lin et al., 2024) on the magnitude image, with no refinements: it follows the inner skull directly. Needs a deep-learning build; the weights are downloaded on first use |
| `mindgrab` | MindGrab deep-learning brain extraction (Fani et al., 2025) on the magnitude image, with no refinements. Works on any contrast; the image is resliced to a 256³ 1 mm grid for the network using its NIfTI affine, and the mask is returned on the original grid. Needs a deep-learning build; the weights are downloaded on first use |

Each preset is exactly the custom mask sections below (see the next
paragraphs for the grammar), so a preset can be used as the starting point for
your own recipe:

| Preset | Equivalent flags |
| --- | --- |
| `robust-threshold` | `--mask phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` |
| `bet` | `--mask magnitude,bet:0.50,erode:2` |
| `hd-bet` | `--mask magnitude,hd-bet,signal-erode` |
| `bet-and-phase` | `--mask magnitude-first,bet:0.50 --mask phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1 --mask-combine and --mask-refine fill-holes:0 --mask-refine erode:1` |
| `rs2-net` | `--mask magnitude,rs2-net` |
| `mindgrab` | `--mask magnitude,mindgrab` |

**Masking input** (`--masking-input`): `magnitude-first`, `magnitude`,
`magnitude-last`, `phase-quality`. For example,
`--mask-preset robust-threshold --masking-input magnitude` thresholds the
combined magnitude image instead of the phase-quality map.

For full control, compose custom mask sections with `--mask` (repeatable),
e.g. `--mask magnitude,bet:0.5,erode:2`. Each section names its input
followed by a generator and refinement operations.

- **Generators:** `threshold[:otsu|fixed:<v>|percentile:<p>]`, `bet[:<f>][:scale=<s>]`,
  `hd-bet[:full|window|<X>x<Y>x<Z>|low-memory][:tta][:step=<s>]`, `rs2-net[:tta][:step=<s>]` and `mindgrab[:tta]`. HD-BET runs on the
  whole volume by default (`full`; about 6–8 GB measured). `window` runs 192×192×96
  sliding-window patches instead (about 3.6–4.5 GB measured, about twice as long),
  `low-memory` 128×128×64 patches (about 1.5–1.9 GB measured), and `step=` sets the
  window stride. `tta` adds mirroring test-time augmentation, which is roughly
  8× slower. For `mindgrab`, `tta` adds a left–right flipped pass, which doubles
  the run time.
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
  --mask phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1 \
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

## Inhomogeneity correction

With `--inhomogeneity-correction` (on by default; `--no-inhomogeneity-correction`
turns it off), the magnitude is corrected for the receive coils' intensity bias
before it is used: the combined magnitude (for masking, ROMEO's weights and SWI)
and each echo's magnitude that a mask section reads. `--inhomogeneity-method`
chooses how:

| Value | Method |
| --- | --- |
| `makehomogeneous` | Divides the magnitude by a smoothed estimate of the receive sensitivity (MriResearchTools' `makehomogeneous`; Eckstein et al., 2019) (default) |
| `n4` | N4 bias field correction ([Tustison et al., 2010](https://doi.org/10.1109/TMI.2010.2046908)), as ANTs' `N4BiasFieldCorrection`: a smooth B-spline bias field, fitted in the log domain over several levels of increasing resolution so that the corrected image's intensity histogram is as sharp as possible |

`makehomogeneous` takes `--homogeneity-sigma-mm` and `--homogeneity-nbox`. N4
takes:

| Option | Meaning | Default |
| --- | --- | --- |
| `--n4-shrink-factor` | subsample the image by this factor before fitting | 4 |
| `--n4-max-iterations` | maximum iterations at each fitting level, one number a level; the B-spline mesh doubles from one level to the next | `50 50 50 50` |
| `--n4-convergence-threshold` | stop a level once the change in the bias field (its coefficient of variation) falls to this | 0 (every iteration runs) |
| `--n4-mesh` | B-spline mesh elements per axis at the first level | `1 1 1` |
| `--n4-spline-order` | order of the B-spline, 1 to 3 | 3 |
| `--n4-bias-field-fwhm` | FWHM of the Gaussian modelling the bias field's blurring of the log-intensity histogram | 0.15 |
| `--n4-wiener-noise` | noise term of the Wiener deconvolution of that histogram | 0.01 |
| `--n4-histogram-bins` | bins of the histogram | 200 |

The defaults are those of `N4BiasFieldCorrection` given only an input and an
output. N4 runs on the whole image, without a mask, because the corrected
magnitude is what the mask is made from. It fits the bias field on the image
shrunk by the shrink factor, so it needs at least twice that many voxels along
each axis (8 at the default of 4); a thinner acquisition is refused before
processing starts, naming the smallest axis, and a lower `--n4-shrink-factor`
or `makehomogeneous` will run it.

The two methods give different images. `makehomogeneous` returns the magnitude
divided by its sensitivity estimate, about 1 in tissue; N4 keeps the image's own
intensity scale. Otsu and percentile thresholds, BET, the deep-learning brain
extractors and `signal-erode` do not depend on that scale, but a
`threshold:fixed:<v>` step on a magnitude input does: choose `<v>` for the
method in use.

`qsmxt homogeneity` runs either on a single image (`--method makehomogeneous|n4`,
with the same N4 options).

## Oblique acquisitions

The dipole kernel is built in the voxel grid, so an acquisition whose slices
are tilted relative to B0 has to be handled explicitly. QSMxT applies the first
of these that fits:

1. **A deep-learning method forces resampling to axial.** These learned the
   dipole relationship from axially-acquired data and take no direction
   argument, so nothing else can help them.
2. **A `B0_dir` in the JSON sidecar wins.** It describes the acquisition better
   than the affine can.
3. **`--axial-resampling` resamples to axial** when the obliquity exceeds
   `--obliquity-threshold` (degrees, default 10). Off by default. A negative
   threshold is refused; a config file that gives `obliquity_threshold`
   without `axial_resampling` is read with a deprecation warning: negative as
   off, otherwise on at that threshold.
4. **Otherwise the kernel is rotated** — B0 is taken from the affine and used
   as-is. This is the default, and normally the one you want.

Rotating the kernel is preferred because it is exact and free: the dipole
kernel takes the field direction as a parameter, so pointing it the right way
costs nothing, keeps the acquired grid, and interpolates nothing.

The cost of the alternative is not small. A 256×288×48 acquisition at 32.5°
obliquity resamples to 272×339×77, roughly twice the voxels, and took 459 s
against 246 s for the same reconstruction on the acquired grid. Resampled
outputs are returned to the acquired grid afterwards unless you ask otherwise;
see [Output space](#output-space).

### Which methods need axial data

| | behaviour on oblique data |
|---|---|
| all 21 classical dipole inversions, PDF, χ-sep iLSQR and MEDI | correct as acquired — B0 is an explicit parameter |
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

Obliquity is `nibabel`'s definition (the norm of the per-axis angles). It is a combined measure rather than a tilt: a single
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
`--use-custom-r2`) with `--r2prime-strategy auto` measures R2′ instead, and
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

## Where R2′ comes from

R2′ (`--do-r2primemap`, and every R2′-based [source separation](#susceptibility-source-separation))
comes from one of three places, chosen by `--r2prime-strategy`:

| `--r2prime-strategy` | R2′ is |
| --- | --- |
| `auto` (default) | **measured** as R2\* − R2 when the run has an R2 (a matched MESE, or `--use-custom-r2`), otherwise **predicted** from R2\* by R2PRIMEnet |
| `mese` | only ever measured; a run without an R2 gets no R2′, and asking for R2′ or an R2′-based separation on it is an error |
| `r2primenet` | always predicted by R2PRIMEnet, even when a MESE is present |

`--use-custom-r2prime <TOOL>` bypasses all three and reads an R2′ map from
`derivatives/<TOOL>/`. So with the default `auto`, an R2′-based separation does not
need a MESE: without one it runs on R2PRIMEnet's prediction, which is trained at
3 T only (see below), and the R2′ sidecar's `R2primeSource` says which it was.

Both measured and predicted R2′ maps have negative values set to zero.

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
| `auto` (no R2 to subtract) | no R2′: an error if R2′ or an R2′-based separation was asked for | R2PRIMEnet runs, with a warning |
| `r2primenet` | the run stops with an error | R2PRIMEnet runs, with a warning |

The R2′ map's JSON sidecar says where it came from: `R2primeSource` is
`measured` (R2* − R2), `predicted` (R2PRIMEnet, with `R2primenetCalibrated` false
away from 3 T) or `supplied` (`--use-custom-r2prime`), with `R2primeMethod`
naming how.

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
- `--output-space working` leaves them on the resampled grid. Their names then
  carry `space-axial` (`sub-01_space-axial_Chimap.nii`) and each sidecar a
  `SpatialReference` (the run's `part-mag_space-axial_T2starw`), so they cannot
  be mistaken for acquired-grid outputs. A run that was never resampled keeps
  its plain names under either setting.

Masks travel by nearest neighbour and phase travels with its magnitude through
the complex domain, for the same reason phase went out that way.

## Grid padding and cropping

All three trade reconstruction cost against the grid the FFT stages run on.
`--fft-padding` and `--crop-to-mask` are off by default; iLSQR always pads.

### `--fft-padding`

FFT cost is `O(N log N)` in the whole grid, and the transform is much faster on
sizes built from small prime factors. Resampling has no reason to land on one:
the grid above comes out 272×339×77, which is 2⁴·17, 3·113 and 7·11.
Growing it to 280×343×80 took one FFT from 131 ms to 71 ms for 8% more voxels.

Padding discards nothing, but it is **not** a no-op. The dipole kernel is
sampled at `k = n / (N·Δx)`, so changing `N` changes where k-space is sampled
and the deconvolution with it. Measured end to end on that acquisition, against
the same run unpadded:

| inversion | median difference | p99 | as % of dynamic range |
|---|---|---|---|
| TKD (direct) | 4e-7 ppm | 9.4e-3 ppm | 0.000% / 5.7% |

A direct inversion is unchanged for the typical voxel and moves only where the
deconvolution is ill-conditioned. An iterative one solves on a different grid,
so it can move slightly everywhere. Neither is evidence that padding is
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

### `--ilsqr-pad-mm`

iLSQR crops the field to the mask's bounding box and zero-pads it before
solving. Each axis is padded to the next multiple of 16 above the crop plus
`2 × pad / voxel size`, and both LSQR solves run on that grid. The default is
12 mm per side.

The padding changes χ slightly, because the dipole convolution is circular on
the padded grid. Larger padding costs time and memory roughly in proportion to
the padded grid. Too little padding lets a thin slab or small field of view wrap
onto itself.

## Coil combination

Runs whose phase is stored per receive coil (`coil-<NN>` entity, see
[Inputs](/QSMxT/reference/inputs/#uncombined-receive-coils)) are combined with
**MCPC-3D-S** (Eckstein et al., MRM 2018) before masking. The coil-summed
Hermitian inner product of the first two echoes gives the field evolution with the
coil phase offsets cancelled; it is unwrapped once (with the `--unwrapping-algorithm`
method), each coil's offset is estimated from its first echo and smoothed with
`--coil-combination-sigma` (default `10 10 5` voxels; the same masked box-filter
smoothing as MriResearchTools), and the channels are summed coherently with magnitude² weights. Multi-echo phase-offset removal in the field-mapping stage then runs on
the combined echoes as usual.

## Phase unwrapping

Set with `--unwrapping-algorithm`.

| Value | Method |
| --- | --- |
| `romeo` | Rapid Opensource Minimum-spanning-tree Echo Optimisation (default) |
| `laplacian` | Laplacian-based unwrapping (default with QSMART) |

With `laplacian`, each echo is unwrapped on its own (the phase is zeroed outside the
mask first) and the echoes are then combined by
`--b0-estimation` / `--b0-weight-type`, the same options as with ROMEO. Phase offset
removal and bipolar correction are not applied, so use it on phase that is already
offset-free (single-channel or prescan-normalised reconstructions, MCPC-3D-S combined
coils).

`--laplacian-solver` picks the Poisson solver (QSM.jl's names):

| Value | Method |
| --- | --- |
| `dct` | Unweighted least squares with a Neumann-boundary DCT solve (Ghiglia & Romero, 1994), as QSM.jl `:dct` (default) |
| `fft` | Schofield & Zhu (2003): sin/cos Laplacian with an FFT Poisson solve on the volume zero-padded by `--laplacian-fft-pad` voxels per side (default 64) |
| `mirror` | The same, solved on the volume mirrored along each axis, as QSMART unwraps (default with QSMART) |

`--laplacian-fft-pad` takes one value for every axis or three (x y z); a config
file's `field_mapping.laplacian_fft_pad` takes a number or a three-element list.
In the TUI,
choosing Laplacian unwrapping shows a "Solver" selector, and `fft` a "FFT Pad
(voxels)" field that accepts `64` or `64 64 32`.

## Motion correction

`--motion-correction` (off by default) rigidly co-registers every echo onto a
reference echo before field mapping: the registration is estimated on the
magnitude images (normalised cross-correlation, multi-resolution) and applied to
the phase through the complex signal, so wraps are not interpolated. The mask is
then restricted to voxels every echo still covers, and every later stage (field
mapping, SWI, T2\*/R2\*, inversion) reads the corrected echoes. It needs at least
two echoes with magnitude images, so it is decided per run: it runs where it can
and is skipped where it cannot, and the rest of the run goes ahead. Before
processing (and with `--dry`), `qsmxt run` says how many runs it will run for and
which it skips and why, e.g. "Motion correction will run for 3 of 5 run(s);
skipped for sub-02_ses-1_MEGRE (one echo) and sub-04_MEGRE (no magnitude)". The
TUI's Pipeline tab shows the same for the selected runs. Each run's record says
whether it was applied, and where only some runs had it, the methods text says
it was applied to runs with more than one echo and magnitude images.

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
`--linear-fit-estimate-offset false`); `robust-fit` is the motion-robust variant
described below; `qsmart-echofit` is QSMART's echo fit, described with
[QSMART](#qsmart).

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
magnitude is used ([Wang et al. 2022, *Nat. Neurosci.* 25:818](https://doi.org/10.1038/s41593-022-01074-w)).
With `--unwrapping-algorithm laplacian` it gives per-echo Laplacian unwrapping and a
TE·exp(−TE/T2\*)-weighted mean of the unwrapped phases. When magnitude images are available, `phase-snr`, which uses the measured magnitude in each
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

### MEDI's complex fit

`--b0-estimation complex-fit` (off by default) is the MEDI toolbox's field map,
`Fit_ppm_complex` ([Liu et al. 2013, *MRM* 69:467](https://doi.org/10.1002/mrm.24272)):
a magnitude-weighted Gauss-Newton fit of the complex echoes that gives the field as
phase per echo spacing, wrapped. That phase is then unwrapped with
`--unwrapping-algorithm` (ROMEO is given the root-sum-of-squares of the echo
magnitudes; MEDI's own region-growing unwrapper is not available) and divided by the
echo spacing. A single echo gives its phase over its echo time.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--complex-fit-reltol` | 1e-4 | stop once the update is this fraction of the first estimate, over the whole volume |
| `--complex-fit-max-iter` | 30 | Gauss-Newton iterations at most |

The fit has its own phase offset, so phase offset removal is not run, whatever
`--phase-offset-removal` says. It needs the magnitude of every echo and increasing,
evenly spaced echo times, and bipolar correction is refused (MEDI's fit for bipolar
readouts is not available); a run that cannot meet these is refused before processing.
In the TUI it is the "complex-fit" B0 Estimation option, with its two parameters below it.

## Background field removal

Set with `--bf-algorithm`.

| Value | Method |
| --- | --- |
| `vsharp` | Variable-kernel SHARP (default) |
| `pdf` | Projection onto Dipole Fields (default with `--qsm-algorithm medi`) |
| `lbv` | Laplacian Boundary Value |
| `ismv` | Iterative Spherical Mean Value |
| `sharp` | Sophisticated Harmonic Artifact Reduction |
| `resharp` | Regularization-enabled SHARP |
| `harperella` | HARPERELLA |
| `iharperella` | Improved HARPERELLA |
| `bfrnet` | BFRnet — deep-learning background removal from the total field. Needs a deep-learning build; weights downloaded on first use |
| `iqfm` | iQFM — deep-learning joint unwrapping and background removal straight from the wrapped phase (the local-field head of iQSM). Replaces field mapping as well; refused with inversions that do not take a local field. Weights downloaded on first use |

With `--qsm-algorithm medi` and no `--bf-algorithm`, background removal is PDF, as in
the MEDI authors' pipeline; the TUI shows it so until another is picked. A background
removal you choose, `vsharp` included, is kept.

Background removal only runs in front of an inversion that takes the local field.
The total-field inversions (`tfi`, `qsmart`, `autoqsm`, `nextqsm`, and `tgv` on
multi-echo data) and the wrapped-phase ones (`iqsm`,
`iqsm-plus`, single-echo `tgv`) remove or model the background themselves, so
`--bf-algorithm` has no effect with them and the TUI hides it.

PDF weights its fit by a noise map made from every echo's magnitude (MEDI toolbox
`Fit_ppm_complex`), so it needs the magnitude of every echo and evenly spaced echoes.
It runs at most `--pdf-n-cg` conjugate-gradient iterations (default 300), on the mask's
bounding box zero-padded by
`--pdf-n-pad` voxels (default 40; 0 runs on the full volume, and 1 is refused). `--pdf-space`
picks the dipole kernel: `image` (the FFT of the dipole sampled on the voxel grid, the
default) or `kspace` (the analytic k-space kernel).

HARPERELLA and iHARPERELLA zero-pad the field by `--harperella-pad` / `--iharperella-pad`
voxels per side, X Y Z (STI Suite's `padsize`, default `0 0 0`).

### V-SHARP and its deconvolution

V-SHARP (Wu et al. 2012) high-passes the field with spherical mean value (SMV)
kernels from `--vsharp-max-radius` down to `--vsharp-min-radius` in steps of
`--vsharp-radius-step` (12 mm to 2 mm in 1 mm steps by default), using at each
voxel the largest kernel that fits inside the mask. The published method then **deconvolves** that high-pass with a
truncated SVD (threshold `--vsharp-threshold`) to recover the local field the
SMV filter attenuated.

The deconvolution is disabled by default (`--vsharp-deconvolve false`). It is
exact for a noise-free harmonic background, but on real field maps the TSVD
inverse amplifies low-frequency noise. `--vsharp-deconvolve true` enables it.
In the TUI it is the "Deconvolve" toggle under V-SHARP; the threshold
row only appears when it is on. The generated methods text says which variant
ran.

### V-SHARP working grid

The SMV kernels are balls on the voxel grid. On strongly anisotropic voxels
(e.g. 0.8 × 0.8 × 3 mm) they are crude along the thick axis: a 1–2 mm ball
is a single-slice disk. `--vsharp-grid` chooses the grid the background is
estimated on:

- `isotropic` (default): an isotropic grid of `--vsharp-grid-spacing` mm
  (default 1), the same whatever the acquisition resolution. Axes finer than the
  spacing are coarsened, including the mask edge the background is estimated
  from.
- `native`: the acquired grid.
- `finest`: an isotropic grid at the smallest acquired voxel dimension (0.8 mm
  for 0.8 × 0.8 × 3 mm), so no axis is coarsened. The working grid is larger by
  the anisotropy ratio, so it costs memory and time on very fine or very
  anisotropic data. Isotropic data stays on the acquired grid.

On an isotropic grid the field and mask are cropped to the mask's bounding box
and resampled trilinearly; the background estimated there is interpolated back,
so the local field keeps the acquired resolution.
In the TUI it is "Working Grid" under V-SHARP, with "Grid
Spacing" shown for `isotropic`.

## Dipole inversion

Set with `--qsm-algorithm`. Two families are available: classical/iterative
reconstructions and deep-learning networks.

### Classical & iterative

| Value | Method |
| --- | --- |
| `rts` | RTS — Rapid Two-Step |
| `tv` | Total Variation (TV-ADMM) |
| `tkd` | TKD — Thresholded K-space Division |
| `tsvd` | TSVD — Truncated Singular Value Decomposition |
| `tgv` | TGV — Total Generalized Variation |
| `tikhonov` | Tikhonov regularization |
| `nltv` | NLTV — Nonlinear Total Variation |
| `medi` | MEDI — Morphology-Enabled Dipole Inversion |
| `tfi` | TFI — Total Field Inversion |
| `ilsqr` | iLSQR — iterative LSQR with streaking-artefact correction (default) |
| `star` | STAR-QSM — Streaking Artifact Reduction |
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

### QSMART

`qsmart` reconstructs from the total field in two stages, with its own background
removal and vessel detection ([Yaghmaie et al. 2021, *NeuroImage* 231:117701](https://doi.org/10.1016/j.neuroimage.2020.117701)).
With QSMART, QSMxT prepares its inputs the way the authors' implementation does,
unless you choose otherwise:

| Input | Default with QSMART | Change with |
| --- | --- | --- |
| Phase unwrapping | `laplacian`, each echo over the whole volume | `--unwrapping-algorithm` |
| Laplacian solver | `mirror` | `--laplacian-solver` |
| Echo combination | `qsmart-echofit`: a magnitude-weighted fit of phase against TE through the origin, which also marks the voxels whose blurred fit residual exceeds the 99th percentile; QSMART leaves those out | `--b0-estimation`; the threshold with `--qsmart-fit-percentile`, or a fixed residual with `--qsmart-fit-threshold` (`--qsmart-fit-threshold-mode percentile\|absolute`) |
| Magnitude for vessel detection | `mean-n4`: the mean over echoes, N4 bias-corrected ([Tustison et al. 2010](https://doi.org/10.1109/TMI.2010.2046908)) | `--qsmart-vessel-magnitude combined` for the combined magnitude (root sum of squares, inhomogeneity-corrected unless `--no-inhomogeneity-correction`) |
| Vessel sizes (`--qsmart-vasc-sphere-radius`, `--qsmart-frangi-scale-*`) | voxels: 8, and scales 0.5 to 6 in steps of 0.5 | `--qsmart-vessel-units mm` to give them in mm, converted with the mean voxel size |

An option you give explicitly is kept, including a general default (for example
`--unwrapping-algorithm romeo`); the TUI likewise keeps a choice you make. QSMART's
echo fit needs Laplacian unwrapping, so `--b0-estimation qsmart-echofit` with
`--unwrapping-algorithm romeo` is refused before the run starts. Chosen with
another inversion, `qsmart-echofit` brings Laplacian unwrapping with the `mirror`
solver unless you choose otherwise, and its reliability map is not used.

A configuration file without `schema_version` that gives the vessel sizes without
`vessel_size_units` reads them in mm, and says so; add `vessel_size_units = "voxels"`
to read them in voxels. A full configuration file an earlier QSMxT wrote, with the
sizes at that release's defaults, takes today's defaults in voxels.

The echo fit's reliability map is kept in the unwrap step's workflow folder
(`*_desc-echofit_mask`), and the vessel-detection magnitude in QSMART's
(`*_part-mag_desc-vessels_T2starw`), beside its other intermediates.

TGV replaces background removal and inversion. With one echo it also replaces unwrapping: it
takes that echo's wrapped phase, at its own echo time. With several echoes the field-mapping
stage runs as usual and TGV takes the combined B0 field map, expressed as phase at the echo
time `--tgv-te` picks (the first by default). Its weights `--tgv-alpha0` / `--tgv-alpha1` are
in the reference's units, radians at that echo time and the field strength, so the same values
regularise differently at another echo time or field strength.

MEDI takes the local field from background removal (PDF unless you choose another, as in
the authors' pipeline), and with `--medi-smv` (the default) high-passes it with a spherical
mean value filter of radius `--medi-smv-radius` (5 mm). With SMV, the mask is shrunk to
where the filtered mask exceeds 0.999, about one SMV radius in from its edge;
`--no-medi-smv-shrink-mask` keeps the full mask and high-passes the field over it.
`--medi-msmv` replaces that step with maximum spherical mean value (mSMV)
filtering (Roberts et al., 2024), which removes residual background at the brain edge without
eroding the mask; it needs at least three echoes. MEDI and TFI weight the data by a noise map
and their edges by a magnitude they make from every echo's magnitude, so they need the
magnitude of every echo and evenly spaced echoes.

STAR-QSM (Wei et al., 2015) takes the local field. It divides the field by the dipole kernel
in k-space, with a floor on small kernel values, for an initial guess, and refines that with a
sparse (L1) solve. Both run on the field zero-padded by `--star-pad` voxels per side (default
`8 8 20`). Its result depends on the scale of the field in radians, so it converts the field to
phase at the echo time `--star-te` picks: `first`, `last`, `spacing` (TE2 − TE1), `sum` (the
sum of the echo times, the default, as for a phase summed over the echoes), or a time in ms.
Changing the padding or the echo time changes the result.

#### Echo time

Several methods take the field in ppm and convert it to phase in radians at one echo time.
A field combined over several echoes has no single echo time, and the choice scales the phase,
so it can change the result. Each such method has a `--<method>-te` setting: `first`, `last`,
`spacing` (TE2 − TE1), `sum` (of the echo times), or a time in ms. With one echo every named
choice is that echo's time. The default is what the method's reference code uses:

| Method | Setting | Default |
| --- | --- | --- |
| NDI, NLTV, L1-QSM, WH-QSM, TGV | `--ndi-te`, `--nltv-te`, `--l1qsm-te`, `--whqsm-te`, `--tgv-te` | `first` |
| MEDI, TFI, FANSI (`fansi`, `fansi-tgv`) | `--medi-te`, `--tfi-te`, `--fansi-te` | `spacing` |
| HEIDI | `--heidi-te` | `last` |
| STAR-QSM | `--star-te` | `sum` |

The choice has to resolve from the scan's echo times: `spacing` needs them in increasing order.
A run that cannot use the choice is refused before processing.

#### Other method settings

- WH-QSM's data weight, `--whqsm-weight`: `flat` (1 in the mask, the default), `max-normalised`
  (the magnitude over its maximum) or `in-vivo` (min(2 × magnitude / maximum, 1)). The last two
  need magnitude images.
- RTS raises its ADMM penalty ρ by `--rts-gamma` (default 5) whenever the primal residual falls
  by less than a factor of `--rts-tau` (default 0.7).
- NLTV's ADMM penalty `--nltv-rho` and TV's `--tv-rho` default to 100 × λ. NLTV's data-term
  subproblem runs at most `--nltv-newton-iter` Newton iterations, to the tolerance
  `--nltv-tol-newton`.
- FANSI's `--fansi-mu1`, `--fansi-alpha0` and `--fansi-mu0`, L1-QSM's `--l1qsm-mu1` and
  HD-QSM's `--hdqsm-mu1-l2` default to values derived from the method's other weights (100 ×
  α1, 2 × α1, 2 × μ1; 100 × α1; 100 × α L2), as the reference code derives them.

A setting derived from others is `auto` on the command line (`--tv-rho auto`), left out of the
config file, and blank in the TUI.

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
Deep-learning support requires a build with the default `dl` feature. `iqsm`
and `iqsm-plus` reconstruct susceptibility directly from the wrapped phase, so
they replace unwrapping, echo combination, background removal and inversion.
`autoqsm` and `nextqsm` take the total field, so they replace background removal
and inversion.

:::tip
Not sure which to pick? The defaults (`threshold` → `romeo` → `vsharp` → `ilsqr`)
are a robust starting point for human brain GRE data (`rts` is ~15x faster if
speed matters more than sensitivity to change). Use the
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

### MEDI's CSF zero reference and TFI's data term

MEDI runs with the ventricular CSF zero reference of MEDI+0
([Liu et al. 2018, *MRM* 79:2795](https://doi.org/10.1002/mrm.26946)), as the MEDI
toolbox's pipeline does: the CSF is found from R2\* (ARLO, on the echo magnitudes) inside
the brain mask, a term weighted by `--medi-lambda-csf` keeps χ uniform there, and χ is
referenced to its mean there. It needs at least three echoes; with fewer, MEDI runs
without it and the run says so.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--medi-csf` | true | the CSF zero reference |
| `--medi-lambda-csf` | 100 | weight of the CSF uniformity term |
| `--medi-csf-r2s-threshold` | 5 | R2\* in Hz below which a voxel is a CSF candidate |
| `--medi-csf-erode` | true | erode the mask by a 10 mm sphere before finding the CSF |
| `--medi-zeropad` | 0 0 0 | voxels of zeros appended at the end of x, y and z before the inversion; the result is cropped back |

QSMxT's own referencing (`--qsm-reference`) still runs after the inversion.

TFI runs MERIT (`--tfi-merit`, true) and fits the field linearly, as the TFI paper does
(`--tfi-linear`, true); `--tfi-linear false` uses MEDI_L1's exponential data term instead.

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

How far apart the orientations may be: measured on an in-vivo-sized phantom,
registration recovers rotations across the whole range tested, 5° to 70°, to
within 0.01°. That is the search's capture range rather than a property of your
data, and it is set by the pyramid depth — worth knowing because it is finite.
Beyond it the failure is not subtle: the search lands in a local minimum tens of
degrees out, correlation collapses, and the group is refused by the floor below.

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

In the [TUI](/QSMxT/guides/running-interactively/) the same pattern is the
**Orientations** field on the Input tab, below Include/Exclude, and it previews
the groups as you type.

`--multi-orientation-lambda` sets the regularisation: 0 (the default) is plain
least squares and is right whenever three or more orientations cover k-space; 1e-3
to 1e-2 helps with two orientations, or when the rotations share a single axis and
streaking survives.

`--cosmos-magnitude-weighting` (TUI: QSM → Multi-orientation → COSMOS
Magnitude Weighting) weights each orientation's field by its combined magnitude, so
low-signal voxels count for less, and solves by conjugate gradient; `--cosmos-tol`
and `--cosmos-max-iter` (default 1e-6 and 100) set that solve, and are refused
without the weighting, where COSMOS is solved in closed form and would ignore them.

Each orientation contributes its *local* field, which only background-field
removal writes. A grouping with an inversion that has no such step (TFI, TGV,
QSMART, AutoQSM, NeXtQSM, iQSM, iQSM+) or with `--no-qsm` is refused
before processing.

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
based on R2′, which comes from a multi-echo spin-echo acquisition, a
bring-your-own R2′ map or, with the default `--r2prime-strategy auto` and neither
of those, R2PRIMEnet's prediction from R2\* (3 T only); see
[Where R2′ comes from](#where-r2-comes-from) and [Input data](/QSMxT/reference/inputs/).

`chi-sep-ilsqr` is initialised with a QSM that the χ-separation step
reconstructs from the local field by iLSQR, as the χ-separation toolbox does,
rather than with the run's own QSM: 100 iterations and 12 mm padding per side by
default (`--chi-sep-ilsqr-qsm-max-iter`, `--chi-sep-ilsqr-qsm-tol`,
`--chi-sep-ilsqr-qsm-pad-mm`). With `--use-custom-qsm` it takes the supplied QSM
instead.

The four non-network methods share a relaxometric constant D_r of 137 Hz/ppm,
from the original χ-separation paper (Shin et al. 2021), which was measured by
regressing R2' against a single-orientation QSM. The χ-sepnet paper (Kim et al.
2025) repeated that regression against COSMOS and got 114 Hz/ppm; that value is
baked into χ-sepnet and SUSEP-Net as a training normalisation and is not a
parameter there. If your χ_total is COSMOS-derived, 114 is the better match for
the analytic methods too, and `--chi-sep-ilsqr-dr-pos` and friends set it.

χ-sepnet runs as a sliding window of 192×192×128-voxel patches;
`--chi-sepnet-patch D H W` changes it (each a multiple of 16). SUSEP-Net runs
the whole volume in one pass; `--susep-net-patch D H W` runs it as a sliding
window of that size instead (each a multiple of 8), so its memory follows the
patch rather than the volume.

χ-sepnet and SUSEP-Net set negative χ+ and |χ−| values to zero.

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
`--synthseg-version v2` (see below: v2's weights are not published yet, so this
needs a v2 segmentation supplied with `--use-custom-dseg`); it is not the same ROI
as `ventricles`, and it does not exist on v1. Pick deliberately — the two give
different numbers.
:::

A region reference implies `--do-segmentation`, since the region has to be
measured on a parcellation; `--use-custom-dseg` satisfies that instead. If the
region turns out to have no voxels inside the support for a subject, that
subject's run **fails** rather than falling back to `mean` — a map that was
quietly referenced to something else looks fine and cannot be compared with the
rest of the cohort.

In the TUI the setting is **QSM → Reference → QSM Reference**. Picking a region
there turns segmentation on: **Other outputs → Segmentation** then reads as ticked,
with its source (SynthSeg or a dseg from derivatives) and settings.

## Susceptibility-weighted imaging

`--do-swi` computes SWI with CLEAR-SWI (Eckstein et al., 2021) from every echo. The magnitudes
are combined by root sum of squares, divided by a receive-sensitivity estimate and scaled with a
softplus curve. Each echo's phase is unwrapped (`--swi-unwrapping`: `laplacian` by default,
`laplacian-slice` for 2D multi-slice data, or `romeo`), high-pass filtered (`--swi-hp-sigma`,
voxels) and the echoes are combined into a frequency map weighted by magnitude × TE. The phase
mask (`--swi-scaling`, `--swi-strength`) multiplies the magnitude, and `_minIP` is the
minimum-intensity projection over `--swi-mip-window` slices. The phase is filtered within
CLEAR-SWI's own head mask, made from the first echo's magnitude, not within the brain mask, so
the SWI covers the head. A single echo gives single-echo SWI.

## Segmentation and per-structure statistics

`--do-segmentation` parcellates the brain straight from the GRE magnitude with
[SynthSeg](https://doi.org/10.1016/j.media.2023.102789). It is trained on
synthetic images of randomised contrast, so one set of weights segments a GRE
magnitude directly — no T1w acquisition, and no registration. Outputs are a
FreeSurfer-labelled `_dseg.nii` and the `_dseg.tsv` lookup table that names its
integers. `--use-custom-dseg <TOOL>` brings your own parcellation from
`<bids>/derivatives/<TOOL>/` instead.

`--synthseg-version v2` selects SynthSeg 2.0 (33 labels, adding a general CSF
class). Its ONNX weights are not published in QSM.rs's model registry yet, so a
run that would segment with v2 is refused at start-up unless `synthseg-2.0.onnx`
is supplied in `$QSM_MODEL_DIR`. With `--use-custom-dseg`, v2 only selects the
label table a supplied segmentation is read with, and needs no weights.

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
