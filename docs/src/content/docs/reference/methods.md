---
title: Methods index
description: "The methods QSMxT offers at each stage of the pipeline."
---

Every stage of the pipeline is configurable. The numerical methods are provided
by [QSM.rs](https://github.com/astewartau/QSM.rs); QSMxT orchestrates them over
your BIDS data. Below are the valid values for each algorithm option.

## Coil combination

Runs whose phase is stored per receive coil (`coil-<NN>` entity, see
[Inputs](/QSMxT/data/uncombined-coils/)) are combined with
**MCPC-3D-S** (Eckstein et al., MRM 2018) before masking. The coil-summed
Hermitian inner product of the first two echoes gives the field evolution with the
coil phase offsets cancelled; it is unwrapped once (with the `--unwrapping-algorithm`
method), each coil's offset is estimated from its first echo and smoothed with
`--coil-combination-sigma` (default `10 10 5` voxels; the same masked box-filter
smoothing as MriResearchTools), and the channels are summed coherently with magnitude² weights. Multi-echo phase-offset removal in the field-mapping stage then runs on
the combined echoes as usual.

## Inhomogeneity correction

With `--inhomogeneity-correction` (on by default; `--no-inhomogeneity-correction`
turns it off), the magnitude is corrected for the receive coils' intensity bias
before it is used. Each echo is corrected on its own, once, in one step that
corrects what the rest of the run reads:

- the echoes the mask reads: the first echo for a first-echo recipe, the last for a
  last-echo one, every echo for the RSS or phase quality (and for refinements after
  combining that read the magnitude);
- every echo, when the combined magnitude is made: it is the root sum of squares of
  the corrected echoes, the image an RSS mask section sees.

When nothing reads a corrected magnitude, nothing is corrected. Field mapping (ROMEO's
weights, MEDI), SWI and relaxometry read the uncorrected echoes. QSMART's vessel
magnitude (`--qsmart-vessel-magnitude mean-n4`) is made in the same step, with QSMART's
own N4. `--inhomogeneity-method` chooses how:

<!-- generated:inhomogeneity-methods start (from QSM.rs's method catalogue and the settings registry: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

| Value | Method | Description | Citation |
| --- | --- | --- | --- |
| `makehomogeneous` | makehomogeneous (default) | Divide by a smoothed estimate of the receive sensitivity (MriResearchTools' makehomogeneous). | Eckstein et al., 2019 |
| `n4` | N4 | N4 bias field correction, as ANTs' N4BiasFieldCorrection: a B-spline bias field fitted to sharpen the intensity histogram. | [Tustison et al., 2010](https://doi.org/10.1109/TMI.2010.2046908) |

<!-- generated:inhomogeneity-methods end -->

N4 fits a smooth B-spline bias field in the log domain, over several levels of increasing
resolution, so that the corrected image's intensity histogram is as sharp as possible.

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
| `--n4-geometry` | the image geometry N4 is given: `scanner` (the scan's header, as ANTs reads the acquired NIfTI) or `make-nii` (the axis-aligned header MATLAB's `make_nii` writes, as QSMART runs N4) | `scanner` |

The defaults are those of `N4BiasFieldCorrection` given only an input and an
output. N4 runs on the whole image, without a mask, because the corrected
magnitude is what the mask is made from. It fits the bias field on the image
shrunk by the shrink factor, so it needs at least twice that many voxels along
each axis (8 at the default of 4); a thinner acquisition is refused before
processing starts, naming the axis that is too short, and a lower `--n4-shrink-factor`
or `makehomogeneous` will run it.

Where the shrunk grid falls depends on the image's header, so the geometry N4
is given changes the correction slightly. QSMART writes the magnitude it hands
N4 with `make_nii`, whose header is axis-aligned at the voxel size;
`--pipeline qsmart` sets `--n4-geometry make-nii` to match.

The two methods give different images. `makehomogeneous` returns the magnitude
divided by its sensitivity estimate, about 1 in tissue; N4 keeps the image's own
intensity scale. Otsu and percentile thresholds, BET, the deep-learning brain
extractors and `signal-erode` do not depend on that scale, but a
`threshold:fixed:<v>` or `min-intensity:<v>` step on a magnitude input does:
choose `<v>` for the method in use.

`qsmxt homogeneity` runs either on a single image (`--inhomogeneity-method makehomogeneous|n4`,
with the same N4 options).

## Phase unwrapping

Set with `--unwrapping-algorithm`.

Phase offset removal cannot estimate an offset far from where the signal is
strong. Those voxels of the mask get no field: the field map and every later step
leave them out, and the mask they work in is saved beside the field map
(`desc-field_mask`; `--field-mask-output` for `qsmxt fieldmap`). The log and the
methods say how many there were.

<!-- generated:unwrapping-methods start (from QSM.rs's method catalogue and the settings registry: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

| Value | Method | Description | Citation |
| --- | --- | --- | --- |
| `romeo` | ROMEO (default) | ROMEO region-growing unwrapping. | [Dymerska et al., 2021](https://doi.org/10.1002/mrm.28563) |
| `laplacian` | Laplacian | Laplacian phase unwrapping (FFT-based). | [Schofield & Zhu, 2003](https://doi.org/10.1364/OL.28.001194) |
| `prelude` | PRELUDE | PRELUDE region-merging unwrapping, each echo on its own. | [Jenkinson, 2003](https://doi.org/10.1002/mrm.10354) |
| `medi-region-growing` | MEDI region growing | MEDI's magnitude-guided region-growing unwrapping; needs magnitude images. | [Liu et al., 2013](https://doi.org/10.1002/mrm.24272) |

<!-- generated:unwrapping-methods end -->

ROMEO is Rapid Opensource Minimum-spanning-tree Echo Optimisation. QSMART's reference
pipeline unwraps with `laplacian` (see [QSMART](#qsmart)).

With several echoes, ROMEO unwraps as it does by default: the first echo is unwrapped
spatially and the others temporally from it (`--romeo-template` picks another echo), with no
global 2π correction, re-unwrapping voxels whose temporal unwrapping is uncertain
(`--romeo-temporal-uncertain-unwrapping 0.5`, ROMEO.jl's default; 0 turns it off, as the
`romeo` command line does). `--romeo-individual` unwraps each echo on its own
instead, for data with motion between echoes or time series; `--romeo-correct-global` then
corrects each echo's 2π offset and the offsets between echoes. Individual unwrapping is less
robust: with the default phase-quality mask, part of the brain can end up a whole 2π cycle
off in some echoes.

The χ-separation toolbox and QSMnetME run ROMEO through its command line, which also uses
romeo3 weights (`--no-romeo-phase-linearity`). Choosing χ-separation iLSQR, MEDI or χ-sepnet,
or QSMnetME, shows this as the original authors' setting, and `--pipeline chi-sep-ilsqr`,
`chi-sep-medi` and `chi-sepnet` use it.

With `laplacian`, each echo is unwrapped on its own (the phase is set to zero outside
the brain mask first) and the echoes are then combined by
`--b0-estimation` / `--b0-weight-type`, the same options as with ROMEO. Phase offset
removal (MCPC-3D-S; Eckstein et al., 2018) and bipolar correction apply as with ROMEO:
Laplacian unwrapping removes only the harmonic part of each echo's phase offset, and PDF and
LBV keep the rest. With `--phase-offset-removal false` each echo is unwrapped as it is, as
STI Suite and QSMART do. Keeping the field over the whole volume (`--laplacian-keep-outside`,
or `--laplacian-unmasked` with `--b0-estimation qsmart-echofit`) needs offset removal off:
MCPC-3D-S defines the corrected phase only inside its mask.

`--laplacian-unmasked` (off by default) unwraps the raw phase over the whole volume
instead, as QSMART does. The combined field is still kept inside the mask, except with
`--b0-estimation qsmart-echofit`, which fits every voxel and keeps its field over the
whole volume. MEDI's complex fit (`--b0-estimation complex-fit`) unwraps its fitted phase
inside the mask either way.

`--laplacian-solver` picks the Poisson solver (QSM.jl's names):

| Value | Method |
| --- | --- |
| `dct` | Unweighted least squares with a Neumann-boundary DCT solve (Ghiglia & Romero, 1994), as QSM.jl `:dct` (default) |
| `fft` | Schofield & Zhu (2003): sin/cos Laplacian with an FFT Poisson solve on the volume zero-padded by `--laplacian-fft-pad` voxels per side (default 64) |
| `mirror` | The same, solved on the volume mirrored along each axis, as QSMART unwraps (QSMART's reference) |
| `mgpcg` | MGPCG (QSM.jl): Laplacian unwrapping with LBV background removal in one step. The Poisson equation is solved inside the brain mask with the phase held at zero outside it, by multigrid-preconditioned conjugate gradients, so the result depends on the mask. QSM.jl follows it with V-SHARP to remove remaining background; QSMxT's background removal step runs afterwards as usual |

`--laplacian-fft-pad` takes one value for every axis or three (x y z); a config
file's `field_mapping.laplacian_fft_pad` takes a number or a three-element list.
In the TUI,
choosing Laplacian unwrapping shows a "Solver" selector and an "Unmasked phase" toggle,
and `fft` a "FFT Pad (voxels)" field that accepts `64` or `64 64 32`.

With `prelude`, each echo is unwrapped on its own and whole turns are then aligned
between echoes; phase offset removal, bipolar correction and every `--b0-estimation`
apply as with ROMEO. Its settings:

| Option | Values | Default |
| --- | --- | --- |
| `--prelude-labelling` | `auto` (FSL prelude's default: slice by slice above 64 voxels in-plane, else over the whole volume), `slices` (slice by slice), `volume` (over the whole volume) | `auto` |
| `--prelude-partitions` | Number of phase bins the initial regions are formed from (at least 1) | `8` |

In the TUI, choosing PRELUDE shows a "Labelling" selector and a "Phase partitions" field.

With `medi-region-growing`, the phase is unwrapped by MEDI's region growing, which
needs magnitude images: it starts at the centre of the field of view and grows
through the voxels above 0.5 % of the brightest magnitude, brightest first. It takes
no mask: it grows over the whole field of view, and voxels it does not reach keep
their wrapped phase. With `--b0-estimation complex-fit` it unwraps MEDI's fit once,
ordered by the root-sum-of-squares magnitude of the echoes, which gives MEDI's own
field map. With the other B0 estimations each echo is unwrapped on its own, ordered
by that same magnitude, and whole turns are then aligned between echoes; phase
offset removal and bipolar correction apply as with ROMEO. It has no settings. With
`--b0-estimation complex-fit`, it is shown as the original authors' setting (MEDI's pipeline
pairs the two); ROMEO stays the default.

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
TUI's Methods › Graph sub-tab shows the same for the selected runs. Each run's record says
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

### Per-echo QSM

`--qsm-per-echo true` (off by default) reconstructs a QSM from each echo on its own instead of
combining the echoes into one field map. Each echo's phase is unwrapped with
`--unwrapping-algorithm` (ROMEO with that echo's magnitude) and divided by its own echo time,
and background field removal, dipole inversion and referencing then run on that echo's field
as they would on a single-echo scan, at its echo time. The run publishes each echo's map
(`<run>_desc-echo<n>_Chimap.nii`, with its `EchoTime` in the sidecar) and, as the QSM
(`<run>_Chimap.nii`), the mean of the echoes' maps with equal weights over the voxels where
every echo's map is defined, referenced as any QSM is.

Nothing combines or compares the echoes, so `--b0-estimation` and its settings, phase offset
removal and bipolar correction do not run (the TUI hides them). DECOMPOSE takes the echoes'
maps (see [Susceptibility source separation](/QSMxT/reference/source-separation/#methods)), and choosing
DECOMPOSE recommends this option. A single-echo scan's QSM is its echo's. Per-echo QSM is
refused before processing with the routes that reconstruct from every echo at once (TGV,
QSMART, iQSM, iQSM+ and iQFM), with two-pass artefact reduction, and with the χ-separation
methods that fit the local field.

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

It needs at least 4 echoes with the offset term, 3 without; fewer is an error rather than
a silent plain fit. Each run writes a per-echo report (down-weighted and rejected
fractions, median weight, median residual, outlier score, flagged echoes) to
`<field map>_robustfit.json` in the unwrap step's workflow folder, and the log names any
flagged echo. In the TUI it is the third "B0 estimation" option, with each parameter
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
evenly spaced echo times; a run that cannot meet these is refused before processing.
With `--bipolar-correction` it is MEDI's fit for bipolar readouts (`Fit_ppm_complex_bipolar`),
which fits a separate phase for the even echoes in every voxel.
In the TUI it is the "complex-fit" B0 estimation option, with its two parameters below it.

## Background field removal

Set with `--bf-algorithm`.

<!-- generated:background-methods start (from QSM.rs's method catalogue and the settings registry: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

| Value | Method | Description | Citation |
| --- | --- | --- | --- |
| `vsharp` | V-SHARP (default) | Variable-kernel SHARP (V-SHARP). | [Wu et al., 2012](https://doi.org/10.1002/mrm.23000) |
| `pdf` | PDF | Projection onto Dipole Fields (PDF). | [Liu et al., 2011a](https://doi.org/10.1002/nbm.1670) |
| `lbv` | LBV | Laplacian Boundary Value (LBV). | [Zhou et al., 2014](https://doi.org/10.1002/nbm.3064) |
| `ismv` | iSMV | Iterative Spherical Mean Value (iSMV). | [Wen et al., 2014](https://doi.org/10.1002/mrm.24998) |
| `sharp` | SHARP | SHARP (Sophisticated Harmonic Artifact Reduction). | [Schweser et al., 2011](https://doi.org/10.1016/j.neuroimage.2010.10.070) |
| `resharp` | RESHARP | Regularized SHARP (RESHARP) with Tikhonov. | [Sun & Wilman, 2014](https://doi.org/10.1002/mrm.24765) |
| `harperella` | HARPERELLA | HARPERELLA integrated unwrap+BFR. | [Li et al., 2014](https://doi.org/10.1002/nbm.3056) |
| `iharperella` | iHARPERELLA | Improved HARPERELLA (iHARPERELLA). | Li et al., 2015a |
| `bfrnet` | BFRnet | BFRnet deep-learning background removal. | [Zhu et al., 2023](https://doi.org/10.1016/j.zemedi.2022.08.001) |
| `iqfm` | iQFM | iQFM deep-learning joint unwrapping + background removal from phase. | [Gao et al., 2022](https://doi.org/10.1016/j.neuroimage.2022.119410) |

<!-- generated:background-methods end -->

BFRnet removes the background from the total field. iQFM is the local-field head of iQSM:
it unwraps and removes the background in one step, straight from the wrapped phase, so it
replaces field mapping as well, and is refused with inversions that do not take a local
field. Both need a deep-learning build; their weights are downloaded on first use.

Choosing an inversion changes no background removal: `--qsm-algorithm medi` alone runs the
general default (`vsharp`). MEDI's reference pipeline uses PDF, which the log, `--dry` and
the methods Note point out, and which the TUI offers when you choose MEDI.

Background removal only runs in front of an inversion that takes the local field.
The total-field inversions (`tfi`, `qsmart`, `autoqsm`, `nextqsm`, and `tgv` on
multi-echo data) and the wrapped-phase ones (`iqsm`,
`iqsm-plus`, single-echo `tgv`) remove or model the background themselves, so
`--bf-algorithm` has no effect with them and the TUI hides it.

PDF weights its fit by a noise map made from every echo's magnitude (MEDI toolbox
`Fit_ppm_complex`), so it needs the magnitude of every echo and evenly spaced echoes.
It runs at most `--pdf-n-cg` conjugate-gradient iterations (default 30), on the mask's
bounding box zero-padded by
`--pdf-n-pad` voxels (default 40; 0 runs on the full volume, and 1 is refused). `--pdf-space`
picks the dipole kernel: `image` (the FFT of the dipole sampled on the voxel grid, the
default) or `kspace` (the analytic k-space kernel). With PDF chosen, QSMxT recommends 300
iterations (rule `pdf.cg-iterations`): 30 can stop far from convergence, and 300 costs about
30 s more per 1 mm whole-brain scan.

LBV (Zhou et al. 2014) follows the authors' code (MEDI toolbox `LBV.m`): a full multigrid
solve of the Laplace equation inside the mask, with the field in radians over the echo spacing
(so it takes the field strength and echo times). Its settings are the original's:
`--lbv-tol` (default 0.01) stops the coarsest grid's relaxation when its summed absolute update
falls below it; `--lbv-depth` is the coarsest grid (default: log2 of the smallest dimension
minus 3); `--lbv-peel` removes more boundary layers from the mask (default 0);
`--lbv-n1`, `--lbv-n2` and `--lbv-n3` are the relaxations on each grid before and after the
coarser one and on the finest grid at the end (30, 100, 100). `--lbv-precision single`
computes in single precision as the original does (default: double). The local field is
defined on the mask minus its boundary layer, which the inversion takes as its mask.

HARPERELLA and iHARPERELLA zero-pad the field by `--harperella-pad` / `--iharperella-pad`
voxels per side, X Y Z (STI Suite's `padsize`, default `0 0 0`). Both set the Laplacian to zero
in the brain's boundary rim, where it holds the step at the mask edge rather than tissue:
`--harperella-rim` / `--iharperella-rim` `whole` zeroes every value in the rim, and `sti` the
20 % largest in magnitude, as STI Suite's `iHARPERELLA` does. HARPERELLA defaults to `whole`
and iHARPERELLA to `sti`.

iSMV (Wen et al. 2014) follows the authors' paper and code: a spherical mean value kernel of
radius 6 × the smallest voxel dimension (6 mm at 1 mm), iterated until the relative change
between iterates falls below 5 × 10⁻⁵ (`--ismv-tol`), with no iteration limit unless
`--ismv-max-iter` gives one. `--ismv-radius` sets the radius in mm instead. The local field is
zero outside the mask eroded by the kernel, and that eroded mask is what the inversion takes.
`--ismv-variant qsmjl` runs QSM.jl's `ismv` instead, whose automatic radius is 2 × the largest
voxel dimension and whose tolerance is the residual relative to the first; the tolerance and
iteration limit keep their settings (QSM.jl's own are `--ismv-tol 0.001 --ismv-max-iter 500`,
which the recommendations offer as QSM.jl's reference values when the variant is chosen).

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
In the TUI it is "Working grid" under V-SHARP, with "Grid
spacing" shown for `isotropic`.

## Tukey windowing

`--tukey-window R` multiplies each echo's complex image (magnitude and phase together) in
k-space by a Tukey window with taper ratio `R` (0 to 1), on the acquired grid, before every
other step: the mask, the magnitude, R2* and all phase processing see the windowed echoes. It is
the χ-separation toolbox's preprocessing, which uses 0.4, and 0 for Philips data. Off by default
(`0`), and it needs magnitude images. The windowed magnitudes are kept in double precision
(as are magnitudes made from coils or real and imaginary parts, and resampled ones), so the mask
is made from them as computed. With χ-separation MEDI, χ-separation iLSQR or χ-sepnet's
toolbox route, the value for the scanner's manufacturer (read from the sidecar's `Manufacturer`;
nothing is shown when it is not known) is shown as the original authors' setting. It is not preselected, and the setting changes only if you
apply it (or run with `--accept-reference-settings`).

## Dipole inversion

Set with `--qsm-algorithm`. Two families are available: classical/iterative
reconstructions and deep-learning networks.

### Classical & iterative

<!-- generated:inversion-methods start (from QSM.rs's method catalogue and the settings registry: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

| Value | Method | Description | Citation |
| --- | --- | --- | --- |
| `rts` | RTS | RTS (Rapid Two-Step) — fast two-step k-space inversion with streaking removal. | [Kames et al., 2018](https://doi.org/10.1016/j.neuroimage.2017.11.018) |
| `tv` | TV-ADMM | TV-ADMM (Total Variation) — ADMM inversion with a total-variation prior. | [Bilgic et al., 2014a](https://doi.org/10.1002/mrm.25029) |
| `tkd` | TKD | TKD (Thresholded K-space Division) — direct k-space division with thresholding. | [Shmueli et al., 2009](https://doi.org/10.1002/mrm.22135) |
| `tsvd` | TSVD | TSVD (Truncated SVD) — zeroes the small singular values of the dipole kernel. | [Shmueli et al., 2009](https://doi.org/10.1002/mrm.22135) |
| `tgv` | TGV-QSM | TGV (Total Generalized Variation) — single-step: unwrapping, background removal and inversion together. | [Langkammer et al., 2015](https://doi.org/10.1016/j.neuroimage.2015.02.041) |
| `tikhonov` | Tikhonov | Tikhonov — closed-form L2-regularized inversion. | [Bilgic et al., 2014b](https://doi.org/10.1002/jmri.24365) |
| `nltv` | NLTV | NLTV (Nonlinear Total Variation) — nonlinear data fidelity with a TV prior. | [Milovic et al., 2018](https://doi.org/10.1002/mrm.27073) |
| `medi` | MEDI | MEDI (Morphology Enabled Dipole Inversion) — magnitude-guided edge weighting. | [Liu et al., 2011b](https://doi.org/10.1002/mrm.22816) |
| `tfi` | TFI | TFI (Total Field Inversion) — inverts the total field directly (its own background removal). | [Liu et al., 2017](https://doi.org/10.1002/mrm.26331) |
| `ilsqr` | iLSQR (default) | iLSQR — iterative LSQR with streaking-artifact correction. | [Li et al., 2015b](https://doi.org/10.1016/j.neuroimage.2014.12.043) |
| `star` | STAR-QSM | STAR-QSM (Streaking Artifact Reduction) — sparse (L1) inversion from a k-space initial guess, for sources with a large susceptibility range. | [Wei et al., 2015](https://doi.org/10.1002/nbm.3383) |
| `lsqr` | LSQR | LSQR (minimally regularised) — one least-squares solve for χ plus a residual field; the residual weight is the only regularisation. With HEIDI ticked, LSQR+HEIDI (HEIDI as published). | [Schweser et al., 2010](https://doi.org/10.1118/1.3481505) |
| `qsmart` | QSMART | QSMART — two-stage spatially-dependent filtering plus inversion. | [Yaghmaie et al., 2021](https://doi.org/10.1016/j.neuroimage.2020.117701) |
| `ndi` | NDI | NDI (Nonlinear Dipole Inversion) — gradient-descent nonlinear fit. | [Polak et al., 2020](https://doi.org/10.1002/nbm.4271) |
| `fansi` | FANSI nonlinear TV | FANSI (Nonlinear TV) — fast nonlinear susceptibility inversion with a TV prior. | [Milovic et al., 2018](https://doi.org/10.1002/mrm.27073) |
| `fansi-tgv` | FANSI nonlinear TGV | FANSI (Nonlinear TGV) — FANSI with a total-generalized-variation prior. | [Milovic et al., 2018](https://doi.org/10.1002/mrm.27073) |
| `l1qsm` | L1-QSM | L1-QSM (L1 Data Fidelity) — L1 fidelity for streaking suppression. | [Milovic et al., 2022](https://doi.org/10.1002/mrm.28957) |
| `whqsm` | WH-QSM | WH-QSM (Weak-Harmonic QSM) — jointly fits a residual harmonic background field. | [Milovic et al., 2019](https://doi.org/10.1002/mrm.27483) |
| `hdqsm` | HD-QSM | HD-QSM (Hybrid Data Fidelity) — combined L1/L2 data fidelity. | [Lambert et al., 2022](https://doi.org/10.1002/mrm.29218) |
| `amp-pe` | AMP-PE | AMP-PE (Approximate Message Passing with Parameter Estimation) — self-tuning Bayesian inversion. | [Huang et al., 2023](https://doi.org/10.1002/mrm.29722) |

<!-- generated:inversion-methods end -->

`lsqr` is weighted by the magnitude, so it needs magnitude images. `--qsm-algorithm heidi`
is LSQR+HEIDI, HEIDI as published: `lsqr` with [`--heidi`](#heidi-streaking-reduction).

### HEIDI streaking reduction

`--heidi` (in the TUI, the HEIDI checkbox under the inversion) runs HEIDI after the dipole
inversion ([Schweser et al. 2012, *NeuroImage* 62:2083](https://doi.org/10.1016/j.neuroimage.2012.05.067)):
the map's well-conditioned k-space coefficients are kept, and the dipole cone is refilled by
total variation weighted by the local field's homogeneity. With `lsqr` this is HEIDI as
published, which `--qsm-algorithm heidi` (LSQR+HEIDI in the TUI's method list, and
`qsmxt invert heidi`) chooses in one go: a configuration stores it as `algorithm = "lsqr"`
under `[inversion]` and `enabled = true` under `[inversion.heidi]` (a file that says
`algorithm = "heidi"` reads as that, with a note). After another inversion it is the step the authors
call PostProcCone, and SEPIA's "Streaking reduction by HEIDI" (`qsmxt invert heidi --seed` runs
it on a map you already have).

HEIDI keeps what the inversion did outside the cone, so it suits a method with little or no
smoothing:

| Inversion | HEIDI after it |
| --- | --- |
| `lsqr` | HEIDI as published |
| `tkd`, `tsvd`, `tikhonov` (small λ), `ndi`, `rts` | suitable; QSMxT shows `lsqr` as the authors' setting, not preselected |
| `medi`, `tv`, `nltv`, the FANSI family, `ilsqr`, `star`, `amp-pe`, the deep-learning methods | QSMxT suggests `lsqr` (not preselected): these methods smooth their result, which also changes the parts of the data that HEIDI keeps as they are, and HEIDI cannot undo that. HEIDI's author recommends a method with little or no smoothing, such as LSQR or TKD |
| `tfi`, `tgv`, `qsmart`, `autoqsm`, `nextqsm`, `iqsm`, `iqsm-plus` | not offered: no local field |

The suggestions come up as recommendations (the TUI's pop-up, the log, `--dry`); QSMxT never
changes the inversion itself. A run is refused when TKD's or TSVD's threshold is above HEIDI's
cone threshold (`--heidi-cone-threshold`, 0.1): the coefficients they truncated would be kept as
data. HEIDI is not offered for multi-orientation reconstructions or χ-separation. With
two-pass artefact reduction both passes are inverted without it, and HEIDI runs once, on the
combined map.

### QSMART

`qsmart` reconstructs from the total field in two stages, with its own background
removal and vessel detection ([Yaghmaie et al. 2021, *NeuroImage* 231:117701](https://doi.org/10.1016/j.neuroimage.2020.117701)).
The authors' implementation prepares its inputs as below. QSMxT runs the settings you give,
so set these to match it; when you choose QSMART, the TUI offers them, and the log, `--dry`
and the methods Note list those that differ:

| Input | QSMART's reference | Set with |
| --- | --- | --- |
| Phase unwrapping | `laplacian` | `--unwrapping-algorithm` |
| Laplacian solver | `mirror` | `--laplacian-solver` |
| Phase that is unwrapped | each echo's raw phase, over the whole volume | `--laplacian-unmasked` |
| Echo combination | `qsmart-echofit`: a magnitude-weighted fit of phase against TE through the origin, which also marks the voxels whose blurred fit residual exceeds a threshold (QSMART v1.0's fixed 40 by default); QSMART leaves those out | `--b0-estimation`; the fixed residual with `--qsmart-fit-threshold`, or a percentile of the residual with `--qsmart-fit-percentile` (`--qsmart-fit-threshold-mode absolute\|percentile`) |
| Magnitude for vessel detection | `mean-n4`: the mean over echoes, N4 bias-corrected ([Tustison et al. 2010](https://doi.org/10.1109/TMI.2010.2046908)) | `--qsmart-vessel-magnitude combined` for the combined magnitude (root sum of squares, inhomogeneity-corrected unless `--no-inhomogeneity-correction`) |
| Vessel sizes (`--qsmart-vasc-sphere-radius`, `--qsmart-frangi-scale-*`) | voxels: 8, and scales 0.5 to 6 in steps of 0.5 | `--qsmart-vessel-units mm` to give them in mm, converted with the mean voxel size |
| Offset adjustment grid | `adjust_offset.m`'s own grid, half a step off the FFT grid on odd matrix sizes (the QSMART pipeline uses it) | `--qsmart-offset-grid qsmart-reference`; the default is the FFT grid (`fft`) |
| Frangi C | `absolute`: 500, in the vessel magnitude's intensity units | `--qsmart-frangi-c`; or `--qsmart-frangi-c-mode relative` for C = `--qsmart-frangi-c-factor` (default 0.25) × the vessel magnitude's median inside the brain mask |

QSMART's echo fit combines the echoes as the field mapping leaves them, so it also runs
after ROMEO unwrapping, with or without phase offset removal and bipolar correction; under
QSMART, the methods Note then lists the unwrapping as a departure from QSMART's reference.
With Laplacian unwrapping and `--laplacian-unmasked`, the fit and its field cover the whole
volume, as in QSMART. Chosen with another inversion, `qsmart-echofit`'s reliability map is
not used.

Frangi C is in the vessel magnitude's intensity units, and scanners scale their magnitude
very differently, so QSMART's C of 500 can label a few percent of one scan's brain as vessel
and over half of another's. `--qsmart-frangi-c-mode relative` sets C from the magnitude
itself, so the vessel detection is the same whatever the intensity scale.

A configuration file without `schema_version` that gives the vessel sizes without
`vessel_size_units` reads them in mm, and says so; add `vessel_size_units = "voxels"`
to read them in voxels. A full configuration file an earlier QSMxT wrote, with the
sizes at that release's defaults, takes today's defaults in voxels.

The echo fit's reliability map is kept in the unwrap step's workflow folder
(`*_desc-echofit_mask`), and the vessel-detection magnitude in QSMART's
(`*_part-mag_desc-vessels_T2starw`), beside its other intermediates: the vesselness,
vessel mask, surface curvature, each stage's proximity maps, SDF widths, local field
and χ, and the stages combined before the offset (listed in
[Output files](/QSMxT/reference/outputs/#qsmarts-intermediate-maps)). The blurred
residual the map was cut at (the value the percentile came to, or the fixed
threshold) is written to the log and recorded with the unwrap step's parameters
in its provenance, as `qsmart_fit_threshold`.

TGV replaces background removal and inversion. With one echo it also replaces unwrapping: it
takes that echo's wrapped phase, at its own echo time. With several echoes the field-mapping
stage runs as usual and TGV takes the combined B0 field map, expressed as phase at the echo
time `--tgv-te` picks (the first by default). Its weights `--tgv-alpha0` / `--tgv-alpha1` are
in the reference's units, radians at that echo time and the field strength, so the same values
regularise differently at another echo time or field strength. The default weights, α0 = 0.00636
and α1 = 0.00212, come from our evaluation on the 2016 QSM challenge and QSM Challenge 2.0 data
(the ratio α0/α1 = 3 at 1.5 times the strength of QuantitativeSusceptibilityMappingTGV.jl's
`regularization = 2`); `--tgv-alpha0 0.002 --tgv-alpha1 0.003` gives that package's weights.
Every other TGV setting is the package's default.

MEDI takes the local field from background removal (the authors' pipeline uses PDF), and with `--medi-smv` (the default) high-passes it with a spherical
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

RTS, TV, TKD, TSVD, Tikhonov and NLTV pad the field as QSM.jl does before their FFTs:
`--rts-pad`, `--tv-pad`, `--tkd-pad`, `--tsvd-pad`, `--tikhonov-pad` and `--nltv-pad` X Y Z
(default `0 0 0`) pad each axis by at least that many voxels and round it up to an
FFT-efficient size; `-1` leaves an axis unpadded.

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

- WH-QSM's data weight, `--whqsm-weight`: `flat` (1 over the whole volume, `WH_nlTV.m`'s
  default), `flat-in-mask` (1 inside the mask only), `max-normalised` (the magnitude over its
  maximum) or `in-vivo` (min(2 × magnitude / maximum, 1)). The last two need magnitude images.
- NDI divides the magnitude by `--ndi-weight-norm` to make its data weight: `global-max` (the
  magnitude's maximum over the whole image, as FANSI, the default) or `mask-p999` (its 99.9th
  percentile inside the mask, which is more consistent in vivo, where the maximum is often a
  bright scalp or vessel voxel outside the brain).
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

<!-- generated:deep-learning-inversion-methods start (from QSM.rs's method catalogue and the settings registry: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

| Value | Method | Description | Citation | Tileable |
| --- | --- | --- | --- | --- |
| `xqsm` | xQSM | Octave-convolution U-net dipole inversion. | [Gao et al., 2021](https://doi.org/10.1002/nbm.4461) | ✓ |
| `qsmnet` | QSMnet | 3D U-net dipole inversion. | [Yoon et al., 2018](https://doi.org/10.1016/j.neuroimage.2018.06.030) | ✓ |
| `qsmnet-plus` | QSMnet+ | QSMnet trained for a wider susceptibility range. | [Jung et al., 2020](https://doi.org/10.1016/j.neuroimage.2020.116619) | ✓ |
| `qsmnetme` | QSMnetME | The χ-separation toolbox's QSM network, trained on multi-echo data; rotates the field to B0 along z and resamples to 1 mm itself. | [Kim et al., 2025](https://doi.org/10.1002/hbm.70136) | ✓ |
| `autoqsm` | AutoQSM | Single-step reconstruction, no separate background removal. | [Wei et al., 2019](https://doi.org/10.1016/j.neuroimage.2019.116064) |  |
| `qsmgan` | QSMGAN | GAN-refined U-net dipole inversion. | [Chen et al., 2020](https://doi.org/10.1016/j.neuroimage.2019.116389) |  |
| `ir2qsm` | IR2QSM | Iterative unrolled U-net inversion. | [Li et al., 2025a](https://doi.org/10.1002/mp.17747) | ✓ |
| `lpcnn` | LPCNN | Learned-proximal unrolled inversion. | [Lai et al., 2020](https://doi.org/10.1007/978-3-030-59713-9_13) | ✓ |
| `modl-qsm` | MoDL-QSM | Model-based unrolled inversion (χ33/STI). | [Feng et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118376) | ✓ |
| `nextqsm` | NeXtQSM | Single-step reconstruction, no separate background removal. | [Cognolato et al., 2023](https://doi.org/10.1016/j.media.2022.102700) | ✓ |
| `iqsm` | iQSM | End-to-end reconstruction from wrapped phase. | [Gao et al., 2022](https://doi.org/10.1016/j.neuroimage.2022.119410) | ✓ |
| `iqsm-plus` | iQSM+ | Orientation-adaptive end-to-end reconstruction. | [Gao et al., 2024](https://doi.org/10.1016/j.media.2024.103160) | ✓ |

<!-- generated:deep-learning-inversion-methods end -->

Model weights are downloaded automatically on first use (and cached) from the
QSMxT weight registry on [Hugging Face](https://huggingface.co/qsmxt/qsm-onnx-weights).
Deep-learning support requires a build with the default `dl` feature. `iqsm`
and `iqsm-plus` reconstruct susceptibility directly from the wrapped phase, so
they replace unwrapping, echo combination, background removal and inversion.
`autoqsm` and `nextqsm` take the total field, so they replace background removal
and inversion. Standalone, they are `qsmxt invert iqsm` and `qsmxt invert iqsm-plus`,
and iQFM is `qsmxt bgremove iqfm`, each reading the 4D wrapped phase.
Each echo is reconstructed separately, and the echoes are combined with weights
(magnitude × TE)², the inverse of each echo's noise variance, as in the authors'
Python code and in both of their iQSM+ implementations. `qsmxt invert iqsm` and
`qsmxt bgremove iqfm` also offer `--echo-weighting mag-te2`, the magnitude × TE²
weighting of the authors' MATLAB code.

:::tip
Not sure which to pick? The defaults (`threshold` → `romeo` → `vsharp` → `ilsqr`)
are a robust starting point for human brain GRE data (`rts` is ~15x faster if
speed matters more than sensitivity to change). Use the
[TUI](/QSMxT/background/tui/) to experiment interactively.
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
command — is the [TUI](/QSMxT/background/tui/): it exposes every
option with sensible defaults and shows the equivalent `qsmxt run` command live as
you change them. You can also set them in a
[configuration file](/QSMxT/reference/configuration/), or run the matching
[step command](/QSMxT/reference/cli/) (e.g. `qsmxt invert tgv --help`) to
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

`--swi-filtered-phase` also writes the phase the phase mask is made from, `_desc-swi_phase`:
the high-pass filtered phase, combined across echoes into a frequency map in Hz, or in radians
for a single echo, and 0 outside CLEAR-SWI's head mask. It comes from the same computation as
the SWI, so it implies `--do-swi`.
