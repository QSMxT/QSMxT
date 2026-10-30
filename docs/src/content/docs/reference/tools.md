---
title: Standalone tools
description: Run individual QSM building blocks directly on NIfTI files, outside the BIDS pipeline.
---

Beyond the end-to-end pipeline, QSMxT exposes each building block as a standalone
command that reads and writes NIfTI files. These are handy for experimentation,
teaching, scripting, or slotting a single step into another workflow.

Every command takes NIfTI input and produces NIfTI output. Run
`qsmxt <command> --help` (and `qsmxt <command> <subcommand> --help`) for exact
arguments.

## Masking — `qsmxt mask`

Generate or refine binary masks.

| Subcommand | Operation |
| --- | --- |
| `otsu` | Otsu automatic thresholding |
| `value` | Fixed-value thresholding |
| `percentile` | Percentile thresholding |
| `bet` | Brain extraction (BET) |
| `hd-bet` | Deep-learning brain extraction (HD-BET); whole volume by default; `--window`, `--low-memory` or `--patch XxYxZ` for sliding-window patches, `--tta`. Weights are downloaded on first use |
| `rs2-net` | Deep-learning **rodent** brain extraction (RS2-Net); `--tta`, `--tile-step`. Weights are downloaded on first use |
| `mindgrab` | Deep-learning brain extraction for any contrast (MindGrab); `--tta`. Needs the image's NIfTI affine. Weights are downloaded on first use |
| `preset` | Run a [`--mask-preset`](/QSMxT/reference/algorithms/#masking) recipe on an image: `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab` |
| `robust` | Alias for `preset robust-threshold` |
| `erode` / `dilate` | Morphological erosion / dilation |
| `close` | Morphological closing |
| `fill-holes` | Fill holes in a binary mask (`--max-size 0`, the default, is automatic: 5% of the volume) |
| `smooth` | Gaussian smooth (re-thresholded at 0.5) |
| `and` / `or` | Intersect / union two or more masks on the same grid |

Every subcommand is the first link of a chain: it does its own operation, then
applies any `--op` refinements in order — `erode:2`, `fill-holes:0`, `close:1`,
`gaussian:4.0`, `signal-erode`, the same spellings as a `--mask` section. So
`qsmxt mask erode m.nii -o out.nii --op fill-holes` erodes, then fills. What each
op does is defined once, in the same code the pipeline runs, so a mask built
here matches the one a `--mask` section would produce.

`and` and `or` are the standalone form of the pipeline's
[`--mask-combine`](/QSMxT/reference/algorithms/#combining-sections), for trying
combinations on masks you already have without a pipeline run. Their `--op`
refinements run on the *combined* mask, like `--mask-refine` does. The
consensus recipe by hand, step by step:

```sh
qsmxt quality-map phase_e1.nii -o quality.nii --magnitude mag_e1.nii
qsmxt mask bet mag_e1.nii -o bet.nii
qsmxt mask otsu quality.nii -o phase.nii
qsmxt mask and bet.nii phase.nii -o brain.nii --op fill-holes:0 --op erode:1
```

— or in one go, since it is a preset:

```sh
qsmxt mask preset bet-and-phase mag_e1.nii --quality quality.nii -o brain.nii
```

`preset` reads its input image for every section; sections that read phase
quality use `--quality` when given, and the input image otherwise, as
`run --masking-input` would. `--op signal-erode` needs a magnitude image: for
`and`/`or`, whose inputs are masks, pass it as `--magnitude`.

## Coil combination — `qsmxt combine`

Combine uncombined receive-coil channels into one phase and magnitude per echo
with MCPC-3D-S (`mcpc3ds`). Inputs are the per-coil, per-echo 3D NIfTIs; files
named with BIDS `coil-NN` / `echo-N` entities are ordered by those, otherwise pass
them coil-major with `--num-echoes`. Echo times come from `--tes` or the phase
sidecars.

```sh
qsmxt combine mcpc3ds \
  --phase sub-1/anat/*rec-uncombined*part-phase_MEGRE.nii.gz \
  --magnitude sub-1/anat/*rec-uncombined*part-mag_MEGRE.nii.gz \
  -o combined/sub-1_rec-uncombined
# writes combined/sub-1_rec-mcpc3ds_echo-N_part-{phase,mag}_MEGRE.nii and
#        combined/sub-1_rec-uncombined_desc-mcpc3ds_mask.nii — the pipeline's names
```

## Phase unwrapping — `qsmxt unwrap`

Unwrap a wrapped phase image (`romeo` or `laplacian`).

## Field mapping — `qsmxt fieldmap`

Turn multi-echo wrapped phase into one B0 field map in ppm, as the pipeline's
field-mapping stage does: `romeo` (phase-offset removal, ROMEO unwrapping and the
B0 fit) or `laplacian` (per-echo Laplacian unwrapping, then echo combination).

With `--b0-estimation complex-fit` (MEDI's fit of the complex echoes, which needs
`--magnitude` and evenly spaced echoes), `--complex-fit-output` also saves the fit
before spatial unwrapping, the phase accrued per echo spacing (radians, wrapped), and
`--complex-fit-noise-output` its noise map, the standard deviation of that phase
(radians per echo spacing). MEDI calls these `iFreq_raw` and `N_std`.

## Background field removal — `qsmxt bgremove`

Remove background fields from an unwrapped total field map in ppm:

| Subcommand | Method |
| --- | --- |
| `vsharp` | Variable-radius SHARP |
| `pdf` | Projection onto Dipole Fields |
| `lbv` | Laplacian Boundary Value |
| `ismv` | Iterative Spherical Mean Value |
| `sharp` | SHARP |
| `resharp` | Regularised SHARP |
| `harperella` | HARPERELLA, joint unwrapping and background removal |
| `iharperella` | Improved HARPERELLA |
| `bfrnet` | BFRnet deep-learning background removal (weights downloaded on first use) |

iQFM, the pipeline's `--bf-algorithm iqfm`, has no standalone command.

`qsmxt bgremove harperella` and `iharperella` read one wrapped echo in rad when
given `--te` and `--field-strength`, or an unwrapped total field in ppm without them
(which iHARPERELLA takes as it is), and write the local field in ppm.

`qsmxt bgremove pdf` takes every echo's magnitude (`--magnitude`, one 4D file or one
file per echo), `--echo-times` and `--field-strength`: PDF weights its fit by a noise
map made from them.

`qsmxt bgremove vsharp` takes `--deconvolve <true|false>` (default `false`, as
`qsmxt run`), `--threshold` (used only with deconvolution),
`--max-radius`/`--min-radius`/`--radius-step` in mm, and `--grid <isotropic|native|finest>` with
`--grid-spacing` in mm (the working grid, as `qsmxt run`'s `--vsharp-grid`).

## Dipole inversion — `qsmxt invert`

Invert a field map into a susceptibility map. One subcommand per method; most
take the local field, and `tfi`, `autoqsm` and `nextqsm` the **total** field:

| Subcommand | Method |
| --- | --- |
| `rts` | Rapid Two-Step |
| `tv` | Total Variation (ADMM) |
| `tkd` | Truncated K-space Division |
| `tsvd` | Truncated SVD |
| `tgv` | Total Generalized Variation |
| `tikhonov` | Tikhonov regularization |
| `nltv` | Nonlinear Total Variation |
| `medi` | Morphology-Enabled Dipole Inversion |
| `tfi` | Total Field Inversion (total field in) |
| `ilsqr` | Iterative Least-Squares QR |
| `star` | STAR-QSM — Streaking Artifact Reduction |
| `lsqr` | Minimally regularised LSQR |
| `heidi` | HEIDI — Homogeneity Enabled Incremental Dipole Inversion |
| `ndi` | Nonlinear Dipole Inversion |
| `fansi` / `fansi-tgv` | FANSI nonlinear TV / TGV |
| `l1qsm` | L1 data-fidelity QSM |
| `whqsm` | Weak-harmonic QSM |
| `hdqsm` | Hybrid data-fidelity QSM |
| `amp-pe` | Approximate Message Passing with Parameter Estimation |
| `xqsm`, `qsmnet`, `qsmnet-plus`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm` | Deep-learning dipole inversions (weights downloaded on first use) |
| `autoqsm`, `nextqsm` | Deep-learning single-step reconstructions from the total field |
| `cosmos` | COSMOS — multi-orientation least squares (2+ orientations) |
| `sti` | Susceptibility tensor imaging (6+ orientations) |

QSMART is its own command, `qsmxt qsmart`, since it takes the total field and
runs its own two-stage reconstruction. To prepare its inputs as QSMART's authors do, map
the field with `qsmxt fieldmap laplacian --laplacian-solver mirror --laplacian-unmasked
--b0-estimation qsmart-echofit --reliability-output r0.nii`, then pass
`--reliability r0.nii` and every echo's magnitude (4D) as `--magnitude`. iQSM and iQSM+, which reconstruct from
wrapped phase, are pipeline-only (`run --qsm-algorithm iqsm`).

Each subcommand runs its method as `qsmxt run` does, through QSM.rs's pipeline: the
field goes in in ppm, and the methods that work in radians (`ndi`, `nltv`, `fansi`,
`fansi-tgv`, `l1qsm`, `whqsm`, `medi`, `tfi`, `heidi`, `tgv`, `star`) convert it themselves
with `--field-strength` and `--echo-times` (every echo's). `ndi`, `nltv`, `fansi`,
`fansi-tgv` and `l1qsm` need `--magnitude` as their data weight; `medi` and `tfi`
need the magnitude of every echo (one 4D file or one file per echo). MEDI-mSMV is
`qsmxt invert medi --msmv`.

```sh
# e.g. inspect the parameters of a method
qsmxt invert tgv --help
```

`cosmos` and `sti` take several `--input` field maps plus one `--b0-direction` per
orientation, and expect them already co-registered onto a common grid. See
[multi-orientation](/QSMxT/reference/algorithms/#multi-orientation-cosmos-and-sti).

## Source separation — `qsmxt separate`

Split a susceptibility map into paramagnetic and diamagnetic sources, writing
`<prefix>_desc-paramagnetic_Chimap.nii`, `_desc-diamagnetic_Chimap.nii` and
`_desc-total_Chimap.nii` (the pipeline's names):

| Subcommand | Method and inputs |
| --- | --- |
| `r2star-qsm` | R2\*-QSM closed form — QSM + R2\* |
| `decompose` | DECOMPOSE-QSM — a QSM per echo (`--qsm-per-echo`) + multi-echo magnitude. `qsmxt run --qsm-per-echo true` gives it each echo's map; without it, its one QSM at every echo |
| `chi-sep-ilsqr` | χ-separation, iLSQR — local field + R2′ + QSM (`qsmxt run` reconstructs this QSM from the local field by iLSQR; here it is the one given) |
| `chi-sep-medi` | χ-separation, MEDI — local field + R2′ + multi-echo magnitude (`--magnitude`, `--echo-times`) + QSM |
| `wavesep` | WaveSep — QSM + R2′ |
| `hc-chisep` | Hollow-cylinder χ-separation — QSM + R2′ + multi-echo magnitude |
| `susep-net` | SUSEP-Net (deep learning) — QSM + R2′ + local field |
| `chi-sepnet` | χ-sepnet (deep learning) — QSM + R2′ + local field |

## Supplementary maps

| Command | Output |
| --- | --- |
| `swi` | Susceptibility-weighted image (CLEAR-SWI) from one or more echoes (`--phase`, `--magnitude`, `--tes`), and with `--mip` its minimum-intensity projection (named like the pipeline's: `sub-01_swi.nii` → `sub-01_minIP.nii`, unless `--mip-output` is given) |
| `r2star` | R2\* map from multi-echo magnitude |
| `t2star` | T2\* map from multi-echo magnitude |
| `r2` | R2 map from multi-echo spin-echo (MESE) magnitude (EPG fit) |
| `r2prime` | R2' map from an R2\* and an R2 map, over the region R2 covers |
| `segment` | SynthSeg whole-brain segmentation of a magnitude image (`_dseg.nii` and its `.tsv` lookup) |
| `quality-map` | ROMEO phase-quality map |

`r2prime` computes `R2' = R2* - R2` only where R2 was actually measured, because
a multi-echo spin-echo acquisition is often a slab and `R2' = R2* - 0 = R2*`
outside it would put the whole of R2\* forward as reversible. Coverage is
inferred from `R2 > 0`, which is exact for an R2 fitted on the same grid; for an
R2 map resampled from another one, interpolation leaves plausible-looking
non-zero values just past its field of view, so say where it reached:

```sh
qsmxt r2prime --r2star r2star.nii --r2 r2.nii -m mask.nii -o r2prime.nii \
  --r2-coverage mese_fov.nii --output-coverage r2prime_coverage.nii
```

## Image utilities

| Command | Purpose |
| --- | --- |
| `homogeneity` | Correct intensity inhomogeneity on magnitude data, with `makehomogeneous` (default) or N4 (`--method n4`) |
| `resample` | Resample an oblique volume to axial orientation. Continuous data (magnitude, an unwrapped field, χ) as a positional argument; **wrapped phase needs `--phase` with `--magnitude`** so the interpolation goes through the complex domain |

```sh
# continuous data
qsmxt resample mag.nii -o mag_axial.nii

# wrapped phase, with its magnitude, interpolated as mag·e^{iφ}
qsmxt resample --phase phase.nii --magnitude mag.nii \
  --phase-out phase_axial.nii --magnitude-out mag_axial.nii
```

:::note
These tools share the same implementations the [pipeline](/QSMxT/guides/running-noninteractively/)
uses, so a step run standalone produces the same result as the corresponding
stage of `qsmxt run`.
:::
