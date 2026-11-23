---
title: Settings (configuration file)
description: Save and reproduce pipeline settings with a TOML configuration file.
---

Anything you can set on the `qsmxt run` command line can also live in a TOML
configuration file. A config file makes runs reproducible and is the cleanest way
to drive batch and [HPC](/QSMxT/how-to/slurm/#hpc--slurm) processing.

## What's in it

The configuration captures the choice of algorithm at each pipeline stage along
with that algorithm's parameters — masking, phase-offset removal, unwrapping,
background-field removal, dipole inversion, and referencing. Because each
algorithm has its own parameters, the config is the place to tune things like
regularisation weights and iteration counts that aren't exposed as top-level
flags.

No default depends on the method: a file runs the methods and settings it says, and
choosing a method never changes another setting. Where a chosen method's reference
pipeline uses something else (PDF background removal for MEDI, for example), QSMxT says so
in the log, in `qsmxt run --dry` and in the methods text's Note, and the TUI offers it (see
[Recommendations](/QSMxT/background/recommendations/#recommendations-1)).

Multi-orientation reconstruction lives in its own `[multi_orientation]` section,
off unless `group_by` names a pattern:

```toml
[multi_orientation]
group_by = "*acq-dir*"   # entity name, glob, re:<regex> or runs:<groups> — empty = off
reference = ""           # glob or word picking each group's reference — empty = first by name
algorithm = "cosmos"     # or "sti"
lambda = 0.0
```

See [multi-orientation](/QSMxT/how-to/multi-orientation/#multi-orientation-cosmos-and-sti)
for what the pattern matches and what will be refused.

The combined magnitude is published unless `pipeline.do_magnitude = false`
(`--no-magnitude`); it is still made, and kept with the run's intermediates, for
SMWI, segmentation and the inversions that weight by it.

The inhomogeneity correction's method is `masking.inhomogeneity_method`, and each
method's settings have their own section: `[homogeneity]` for `makehomogeneous`
and `[n4]` for N4:

```toml
[masking]
inhomogeneity_correction = true
inhomogeneity_method = "n4"   # or "makehomogeneous" (the default)
n4_geometry = "scanner"       # or "make-nii" (QSMART's axis-aligned header)

[n4]
shrink_factor = 4
max_iterations = [50, 50, 50, 50]   # one entry per fitting level
convergence_threshold = 0.0
mesh = [1, 1, 1]
spline_order = 3
bias_field_fwhm = 0.15
wiener_noise = 0.01
histogram_bins = 200
```

See [inhomogeneity correction](/QSMxT/reference/methods/#inhomogeneity-correction)
for what each does.

The [Tukey window](/QSMxT/reference/methods/#tukey-windowing) on the echoes is
`pipeline.tukey_window`, its taper ratio (`--tukey-window`; 0, the default, applies none):

```toml
[pipeline]
tukey_window = 0.4   # as the χ-separation toolbox uses; 0 for Philips
```

With two-pass artefact reduction, the reliable mask's sections and how they combine are
`masking.two_pass_sections` and `masking.two_pass_combine` (`"or"` or `"and"`; left out, it is
the main mask's `masking.combine`), and how the second pass is made and merged is
`masking.two_pass_mode` (`"local"`, the default, or `"original"`, the published algorithm; see
[two-pass modes](/QSMxT/how-to/build-mask/#two-pass-modes)):

```toml
[masking]
two_pass = true
two_pass_combine = "and"
two_pass_mode = "original"
```

Laplacian unwrapping's settings are in `[field_mapping]`:

```toml
[field_mapping]
unwrapping_algorithm = "laplacian"
laplacian_solver = "mirror"   # or "dct" (the default), "fft", "mgpcg" (QSM.jl's MGPCG: unwrapping with LBV background removal in one step)
laplacian_unmasked = true     # the raw phase over the whole volume (default false)
laplacian_keep_outside = false # keep the masked phase's field over the whole volume (UK Biobank; --laplacian-keep-outside)
```

`laplacian_unmasked = true` is `--laplacian-unmasked`. PRELUDE's are in
`[field_mapping.prelude]`:

```toml
[field_mapping]
unwrapping_algorithm = "prelude"

[field_mapping.prelude]
labelling = "volume"          # or "auto" (the default), "slices"; --prelude-labelling
num_phase_partitions = 6      # default 8; --prelude-partitions
```

MEDI's region growing, `unwrapping_algorithm = "medi-region-growing"`, has no settings.
See
[phase unwrapping](/QSMxT/reference/methods/#phase-unwrapping) for what each does.

A method that converts the field to phase at one echo time has a `te` setting naming it:
`"first"`, `"last"`, `"spacing"` (TE2 − TE1), `"sum"` (of the echo times), or a given time
in seconds. On the command line the same choice is `first`, `last`, `spacing`, `sum`, or a
time in ms:

```toml
[inversion.star]
pad = [8, 8, 20]          # zero-padding per side, voxels (x y z)
te = "sum"                # or "first", "last", "spacing", or { seconds = 0.03 }

[inversion.medi]
te = "spacing"            # or "first", "last", "sum", or { seconds = 0.02 }

[inversion.whqsm]
weight = "flat"           # or "flat-in-mask", "max-normalised", "in-vivo"

[inversion.ndi]
weight_norm = "global-max"  # or "mask-p999"

[bg_removal.harperella]
rim = "whole"             # or "sti" (iHARPERELLA's default)

[bg_removal]
erode_voxels = 0          # erode the mask by N voxels before background removal; --bf-erode-voxels
polish_mask = false       # STI Suite's PolishMask after background removal; --bf-polish-mask
ukb_mask_trim = false     # UK Biobank's phase-reliability mask trimming around background removal; --bf-ukb-mask-trim
```

HEIDI after the inversion ([HEIDI streaking reduction](/QSMxT/reference/methods/#heidi-streaking-reduction))
is a switch under `[inversion.heidi]`; LSQR+HEIDI, HEIDI as published, is LSQR with it on:

```toml
[inversion]
algorithm = "lsqr"

[inversion.heidi]
enabled = true            # --heidi; with "lsqr", the same as --qsm-algorithm heidi
cone_threshold = 0.1
```

A file from an earlier QSMxT with `algorithm = "heidi"` reads as the above.

`te = { seconds = 0.03 }` under `[inversion.star]` is `--star-te 30`, and
`te = { seconds = 0.02 }` under `[inversion.medi]` is `--medi-te 20`. A setting the method derives from its others
(`inversion.tv.rho`, `inversion.nltv.rho`, FANSI's `mu1`, `alpha0` and `mu0`, L1-QSM's `mu1`,
HD-QSM's `mu1_l2`, iSMV's `radius`, χ-sep iLSQR's and χ-sep MEDI's `dr_pos` and `dr_neg`) is
left out of the file to take the derived value; on the command line it is `auto`. So is a limit
that is off by default (iSMV's `max_iter`, DECOMPOSE's
`lsqcurvefit.max_function_evaluations`).

Recommendations you chose not to follow (see
[Recommendations](/QSMxT/background/recommendations/#recommendations-1)) are kept in the
file, each with the value of its setting at the time. Only recommended, suggested and discouraged
findings are kept; one for a reference-pipeline difference is dropped when the file is read. A change to the setting brings the
recommendation back:

```toml
[[dismissed_recommendations]]
rule = "fansi.te-spacing"   # the rule's id, as `qsmxt run --dry` lists it
chosen = "first"            # the setting's value it was dismissed for
```

On the command line this is `--dismiss-recommendation fansi.te-spacing=first`. A file without
`dismissed_recommendations` has none.

## Files from earlier versions

A config file QSMxT writes starts with `schema_version`, the layout it was written in.
A file without it is read as follows, and what changed is logged:

- **A full config file an earlier QSMxT wrote** (`qsmxt init`, or a run's
  `pipeline_config.toml`) holds every setting, so it would otherwise keep the defaults of
  the release that wrote it as though they had been chosen. A setting holding one of an
  earlier release's defaults takes today's default instead. Any other value is kept.
- **Any other file** (written by hand, or a changed-settings file) holds what its author
  chose, so nothing is reset.
- **Settings whose meaning changed** are refused when they hold anything but today's
  default and their method runs: TGV's `alpha0`/`alpha1` (now in radians at the echo time
  `inversion.tgv.te` picks, the first by default, and the field strength), MEDI's and TFI's `lambda` (now MEDI_L1's data
  weight), and `field_mapping.phase_offset_sigma` (now `phase_offset_sigma_mm`, in mm).
  The message says what changed. Give the value in the new terms and add `schema_version`
  with the current version at the top, or remove the setting to use the default. When the
  method does not run, the value is dropped with a note.
- **QSMART's vessel sizes** (`inversion.qsmart.vasc_sphere_radius` and the Frangi
  scales) were in mm and are now in voxels by default (`vessel_size_units`). Sizes
  given without `vessel_size_units` keep their meaning, in mm, with a warning when QSMART
  runs; in a full config file, sizes at the earlier defaults take today's.
- Settings an earlier file does not have (each method's echo time `te`, WH-QSM's `weight`,
  NLTV's `rho` and `tol_newton`, RTS's `tau` and `gamma`, PDF's `n_cg`, `space` and `n_pad`,
  LBV's `depth`, `peel`, `n1`, `n2`, `n3` and `precision`, HARPERELLA's and iHARPERELLA's `pad`
  and `rim`, NDI's `weight_norm`, QSMART's `frangi_c_mode` and `frangi_c_factor`)
  take today's defaults. LBV's `tol` is kept as written: an earlier file's 1e-6 (the old
  solver's relative tolerance) now stops the multigrid's coarsest grid at 1e-6 radians, and the
  recommendations suggest LBV.m's 0.01.
- Settings that no longer exist are ignored, with a warning where that changes what a
  run does (for example `bg_removal.msmv_refine`: mSMV is now MEDI's option,
  `inversion.medi.msmv`).
- `field_mapping.linear_fit.reliability_threshold_percentile` never changed the field map
  and is no longer a setting: it is dropped with a note, whatever it holds, also from a
  file that has `schema_version`.
- `bg_removal.vsharp.min_radius` was also the step between V-SHARP's radii. A value kept
  from such a file brings that step with it as `bg_removal.vsharp.radius_step`, so the
  radii stay the same.
- TFI's `merit` did nothing in earlier files; it is now MERIT, on by default. An earlier
  value is dropped, with a warning where it turned MERIT off for a TFI run; keep it, with
  `schema_version` at the top, to turn MERIT off.

<!-- generated:settings start (from the settings registry: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

## All settings

Every setting a configuration file can hold, grouped as the TUI's tabs show them: where it
goes in the file (`a.b.c` is `c` under `[a.b]`), what it takes, its default, and the
`qsmxt run` option that sets it. A setting applies only where its method or step runs.

### Input

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `multi_orientation.group_by` | text | `""` | `--orientation-group` | Runs that are the same object at a different angle, matched by an entity (acq, run, rec, inv), a glob (\*acq-dir\*), re:&lt;regex&gt; or runs:&lt;groups&gt;; empty turns grouping off. How the orientations are combined is set under COSMOS/STI. `""` turns it off. |

### Preprocessing: Geometry

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.axial_resampling` | true or false | `false` | `--axial-resampling` | Resample oblique acquisitions onto an axial grid when they are tilted more than the threshold. Off: reconstruct on the acquired grid with the true B0 direction. |
| `pipeline.obliquity_threshold` | number, deg, at least 0 | `10.0` | `--obliquity-threshold` | Resample only data more oblique than this. |
| `pipeline.output_space` | choice | `"acquired"` | `--output-space` | The grid derivatives are written on, when an oblique run was resampled to axial. Values: `acquired`, `working`. |
| `pipeline.ignore_slice_gap` | true or false | `false` | `--ignore-slice-gap` | Reconstruct even if the sidecar's SliceThickness is thinner than the slice pitch (gapped 2D data gives a wrong map). |
| `pipeline.tukey_window` | number, from 0 to 1 | `0.0` | `--tukey-window` | Taper ratio of a k-space Tukey window applied to each echo's complex image before every other step: 0 applies none; the χ-separation toolbox uses 0.4, and 0 for Philips data. Needs magnitude. |
| `pipeline.crop_to_mask` | true or false | `false` | `--crop-to-mask` | Run the FFT-bound stages on a box around the mask (faster; outputs stay on the full grid). |
| `pipeline.crop_margin_mm` | number, mm | `32.0` | `--crop-margin-mm` | Margin around the mask; below about 32 mm results start to differ, and it must clear the largest SMV kernel. |
| `pipeline.fft_padding` | true or false | `false` | `--fft-padding` | Pad the grid to an FFT-friendly size before the FFT-bound stages (faster on awkward grids; shifts results slightly). |
| `field_mapping.coil_combination_sigma` | three numbers, voxels | `[10.0, 10.0, 5.0]` | `--coil-combination-sigma` | MCPC-3D-S smoothing of the per-coil phase offsets (x y z); only for uncombined multi-coil data. |

### Preprocessing: Masking

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `masking.sections` | mask sections | `phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` | — | The mask recipe: one or more sections, each an input image, a generator and its refinements, combined with OR or AND. |
| `masking.combine` | choice | `"or"` | — | How the sections are combined: OR keeps a voxel any section keeps, AND only one every section keeps. Values: `or`, `and`. |
| `masking.refinements` | mask steps | none | — | Refinement steps run on the combined mask, after the sections are combined. |
| `masking.custom_mask_tool` | text | unset | `--use-custom-masks` | Use a mask from &lt;bids&gt;/derivatives/&lt;tool&gt;/ instead of computing one: \* for any tool, or a tool name (e.g. bet); empty to always compute it. Where none is found, the mask is computed. May be left unset. |
| `masking.two_pass` | true or false | `false` | `--two-pass` | Reconstruct again on a mask with holes at strong susceptibility sources and merge the two — keeps streaking from those sources out of the rest of the brain, at roughly double the reconstruction time (doi:10.1002/mrm.29048). |
| `bet.smoothness` | number, greater than 0 | `1.0` | `--bet-smoothness` | BET surface smoothness factor (shared by every BET step). |
| `bet.gradient_threshold` | number | `0.0` | `--bet-gradient-threshold` | BET gradient threshold, from -1 to 1 (shared by every BET step). |
| `bet.iterations` | whole number | `1000` | `--bet-iterations` | BET surface evolution iterations (shared by every BET step). |
| `bet.subdivisions` | whole number | `4` | `--bet-subdivisions` | BET icosphere subdivision level (shared by every BET step). |
| `bet.orientation` | choice | `"fsl"` | `--bet-orientation` | Which way round BET sees the left-right axis (shared by every BET step). Values: `fsl`, `as-stored`. |
| `masking.two_pass_sections` | mask sections | unset | — | The mask the second pass reconstructs on: its holes are left unfilled. Empty = the main mask without its hole filling. May be left unset. |
| `masking.two_pass_combine` | choice | unset | `--two-pass-mask-combine` | How the reliable mask's sections are combined: OR keeps a voxel any section keeps, AND only one every section keeps. Empty = as the main mask's sections are. Values: `or`, `and`. May be left unset. |
| `masking.two_pass_mode` | choice | `"local"` | `--two-pass-mode` | How the second pass is made and merged: local (holes only at strong sources, second pass used near them) or original (the published algorithm). Values: `local`, `original`. |
| `masking.inhomogeneity_correction` | true or false | `true` | `--no-inhomogeneity-correction` | Correct the magnitude for receive-coil inhomogeneity: the echoes masking reads, and the combined magnitude. |
| `masking.inhomogeneity_method` | choice | `"makehomogeneous"` | `--inhomogeneity-method` | How the magnitude is corrected for inhomogeneity. Values: `makehomogeneous`, `n4`. |
| `homogeneity.sigma_mm` | number, mm | `7.0` | `--homogeneity-sigma-mm` | Smoothing sigma of the bias-field estimate. |
| `homogeneity.nbox` | whole number, at least 1 | `15` | `--homogeneity-nbox` | Boxes per dimension for the tissue segmentation the receive sensitivity is estimated from. |
| `n4.shrink_factor` | whole number, at least 1 | `4` | `--n4-shrink-factor` | Subsample the image by this factor before fitting the bias field. |
| `n4.max_iterations` | list of whole numbers | `[50, 50, 50, 50]` | `--n4-max-iterations` | Maximum iterations at each fitting level, one number a level (e.g. 50 50 50 50); the B-spline mesh doubles from one level to the next. |
| `n4.convergence_threshold` | number, at least 0 | `0.0` | `--n4-convergence-threshold` | Stop a level once the change in the bias field (its coefficient of variation) falls to this; 0 runs every iteration. |
| `n4.mesh` | three whole numbers, at least 1 | `[1, 1, 1]` | `--n4-mesh` | B-spline mesh elements per axis at the first level (x y z). |
| `n4.spline_order` | whole number, from 1 to 3 | `3` | `--n4-spline-order` | Order of the B-spline bias field: 1, 2 or 3. |
| `n4.bias_field_fwhm` | number, greater than 0 | `0.15` | `--n4-bias-field-fwhm` | FWHM of the Gaussian modelling the bias field's blurring of the log-intensity histogram. |
| `n4.wiener_noise` | number, at least 0 | `0.01` | `--n4-wiener-noise` | Noise term of the Wiener deconvolution of the histogram. |
| `n4.histogram_bins` | whole number, at least 2 | `200` | `--n4-histogram-bins` | Bins of the log-intensity histogram. |
| `masking.n4_geometry` | choice | `"scanner"` | `--n4-geometry` | The image geometry N4 is given, which decides where its shrunk grid falls. Values: `scanner`, `make-nii`. |

### Preprocessing: Motion

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `motion_correction.enabled` | true or false | `false` | `--motion-correction` | Rigidly co-register every echo onto a reference echo before any other stage (multi-echo, needs magnitude). |
| `motion_correction.reference` | echo | `"first"` | `--motion-reference` | The echo every other echo is aligned onto. Values: `first`, `last`, an echo number from 1, as text (`"1"`). |
| `motion_correction.chain` | true or false | `true` | `--no-motion-chain` | Seed each echo's registration from its neighbour's solution. |
| `motion_correction.chain_levels` | whole number | `3` | `--motion-chain-levels` | Pyramid levels for a seeded registration. |
| `motion_correction.min_ncc` | number | `0.3` | `--motion-min-ncc` | Normalised cross-correlation below which a registration is reported as suspect (0.3 for echoes; about 0.9 for repeats). |
| `registration.levels` | whole number | `5` | `--registration-levels` | Pyramid levels: the capture range; do not lower. |
| `registration.max_samples` | whole number | `200000` | `--registration-max-samples` | Voxels sampled per metric evaluation, per level. |
| `registration.max_iterations` | whole number | `20` | `--registration-max-iterations` | Powell cycles per level. |
| `registration.max_evaluations` | whole number | `5000` | `--registration-max-evaluations` | Metric evaluations across all levels (hard stop). |
| `registration.tolerance` | number | `0.00001` | `--registration-tolerance` | Relative cost improvement per cycle that counts as converged. |
| `registration.rotation_step_deg` | number, deg | `3.0` | `--registration-rotation-step-deg` | Initial rotation step at the finest level. |
| `registration.translation_step_mm` | number, mm | `3.0` | `--registration-translation-step-mm` | Initial translation step at the finest level. |
| `registration.min_overlap` | number | `0.25` | `--registration-min-overlap` | Smallest fraction of sampled voxels that must overlap. |

### QSM: Field map

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `field_mapping.unwrapping_algorithm` | choice | `"romeo"` | `--unwrapping-algorithm` | How the wrapped phase is unwrapped. Values: `romeo`, `laplacian`, `prelude`, `medi-region-growing`. |
| `field_mapping.prelude.labelling` | choice | `"auto"` | `--prelude-labelling` | How PRELUDE labels its initial regions. Values: `auto`, `slices`, `volume`. |
| `field_mapping.prelude.num_phase_partitions` | whole number, at least 1 | `8` | `--prelude-partitions` | Number of phase bins the initial regions are formed from. |
| `field_mapping.laplacian_solver` | choice | `"dct"` | `--laplacian-solver` | How Laplacian unwrapping's Poisson equation is solved. Values: `dct`, `fft`, `mirror`, `mgpcg`. |
| `field_mapping.laplacian_fft_pad` | three whole numbers, voxels | `[64, 64, 64]` | `--laplacian-fft-pad` | Zero-padding per side: one value for every axis, or three (x y z). |
| `field_mapping.laplacian_unmasked` | true or false | `false` | `--laplacian-unmasked` | Unwrap the raw phase over the whole volume, as QSMART does, instead of the phase set to zero outside the brain mask. |
| `field_mapping.laplacian_keep_outside` | true or false | `false` | `--laplacian-keep-outside` | Keep the field over the whole volume, not only inside the brain mask, as UK Biobank's pipeline hands it to V-SHARP (weighted-average combination). |
| `field_mapping.phase_offset_removal` | true or false | `true` | `--phase-offset-removal` | Remove the echoes' spatially varying phase offset with MCPC-3D-S (Eckstein et al., 2018) before unwrapping; multi-echo only. |
| `field_mapping.phase_offset_sigma_mm` | three numbers, mm | `[7.0, 7.0, 7.0]` | `--phase-offset-sigma-mm` | Gaussian smoothing sigma (x y z). |
| `field_mapping.bipolar_correction` | true or false | `false` | `--bipolar-correction` | Remove the linear phase artefact of bipolar readout gradients (needs at least 3 echoes; fewer: skipped). |
| `field_mapping.romeo.individual` | true or false | `false` | `--romeo-individual` | Unwrap each echo on its own (for motion or time series). Off = the template echo is unwrapped spatially and the others temporally from it. |
| `field_mapping.romeo.template` | whole number | `0` | `--romeo-template` | Echo used for spatial unwrapping, counted from 0. |
| `field_mapping.romeo.correct_global` | true or false | `false` | `--romeo-correct-global` | Correct inter-echo 2π offsets after unwrapping. |
| `field_mapping.romeo.phase_coherence` | true or false | `true` | `--no-romeo-phase-coherence` | Weight edges by neighbour phase coherence. |
| `field_mapping.romeo.phase_gradient_coherence` | true or false | `true` | `--no-romeo-phase-gradient-coherence` | Weight edges by inter-echo phase-gradient consistency (multi-echo). |
| `field_mapping.romeo.phase_linearity` | true or false | `true` | `--no-romeo-phase-linearity` | Weight edges by second-derivative smoothness. |
| `field_mapping.romeo.mag_coherence` | true or false | `true` | `--no-romeo-mag-coherence` | Weight edges by neighbour magnitude coherence. |
| `field_mapping.romeo.mag_weight` | true or false | `false` | `--romeo-mag-weight` | Penalise low-signal voxels. |
| `field_mapping.romeo.mag_weight2` | true or false | `false` | `--romeo-mag-weight2` | Penalise abnormally high signal and flow artefacts. |
| `field_mapping.romeo.bestpath` | true or false | `false` | `--romeo-bestpath` | Use Abdul-Rahman best-path weights instead of ROMEO weights. |
| `field_mapping.romeo.temporal_uncertain_unwrapping` | number | `0.0` | `--romeo-temporal-uncertain-unwrapping` | Re-unwrap voxels whose temporal unwrapping is uncertain. Quality threshold: Quality threshold (0, 1] below which a voxel is re-unwrapped. `0.0` turns it off. |
| `field_mapping.romeo.max_seeds` | whole number | `1` | `--romeo-max-seeds` | Maximum number of seed regions. |
| `field_mapping.romeo.merge_regions` | true or false | `false` | `--romeo-merge-regions` | Merge neighbouring regions after unwrapping. |
| `field_mapping.romeo.correct_regions` | true or false | `false` | `--romeo-correct-regions` | Shift each region by a multiple of 2π to bring its median nearest 0. |
| `field_mapping.romeo.wrap_addition` | number | `0.0` | `--romeo-wrap-addition` | Extra phase tolerance beyond π for neighbour differences, from 0 to π. |
| `qsm.per_echo` | true or false | `false` | `--qsm-per-echo` | Reconstruct a QSM from each echo on its own and report their mean, instead of one QSM from the echo-combined field map. |
| `field_mapping.b0_estimation` | choice | `"weighted-avg"` | `--b0-estimation` | How the unwrapped echoes are combined into one field map. Values: `weighted-avg`, `linear-fit`, `robust-fit`, `qsmart-echofit`, `complex-fit`. |
| `field_mapping.qsmart_fit_threshold_mode` | choice | `"absolute"` | `--qsmart-fit-threshold-mode` | How the fit residual at which a voxel is left out of QSMART's reconstruction is set. Values: `percentile`, `absolute`. |
| `field_mapping.qsmart_fit_percentile` | number, from 0 to 100 (not 0) | `99.0` | `--qsmart-fit-percentile` | Percentile of the blurred fit residual above which a voxel is left out of QSMART's reconstruction. |
| `field_mapping.qsmart_fit_threshold` | number, greater than 0 | `40.0` | `--qsmart-fit-threshold` | Blurred fit residual above which a voxel is left out of QSMART's reconstruction. |
| `field_mapping.b0_weight_type` | choice | `"phase-snr"` | `--b0-weight-type` | How each echo is weighted in the average. Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `field_mapping.b0_weight_assumed_t2star_ms` | number, ms, greater than 0 | `40.0` | `--b0-weight-assumed-t2star` | T2\* assumed to compute the fixed echo weights; no T2\* map is used. |
| `field_mapping.linear_fit.estimate_offset` | true or false | `true` | `--linear-fit-estimate-offset` | Fit a phase-offset term in the echo-time fit (robust fit: needs 4 echoes with it, 3 without). |
| `field_mapping.robust_fit.loss` | choice | `"tukey"` | `--robust-fit-loss` | The loss that down-weights an echo departing from the straight line through the others. Values: `tukey`, `huber`. |
| `field_mapping.robust_fit.tuning` | number, greater than 0 | unset | `--robust-fit-tuning` | Tuning radius in robust-scale units; empty for the conventional value (Tukey 4.685, Huber 1.345). May be left unset. |
| `field_mapping.robust_fit.iterations` | whole number | `3` | `--robust-fit-iterations` | Iteratively reweighted passes (0 reports only: the field map is the linear fit's). |
| `field_mapping.robust_fit.downweight_level` | number, from 0 to 1 (not 0) | `0.75` | `--robust-fit-downweight-level` | Reporting only: a weight below this counts as down-weighted. |
| `field_mapping.robust_fit.flag_ratio` | number, at least 1 | `3.0` | `--robust-fit-flag-ratio` | Reporting only: flag an echo whose outlier score exceeds this multiple of the best echo's. |
| `field_mapping.complex_fit.reltol` | number, at least 0 | `0.0001` | `--complex-fit-reltol` | The fit stops once its update is this fraction of the first estimate, over the whole volume. |
| `field_mapping.complex_fit.max_iter` | whole number | `30` | `--complex-fit-max-iter` | Gauss-Newton iterations at most. |

### QSM: Background

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `bg_removal.algorithm` | choice | `"vsharp"` | `--bf-algorithm` | How the background field, from sources outside the mask, is removed to leave the local field. Values: `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella`, `bfrnet`, `iqfm`. |
| `bg_removal.erode_voxels` | whole number, voxels, at least 0 | `0` | `--bf-erode-voxels` | Erode the brain mask before background removal; the field map itself is not masked again. |
| `bg_removal.polish_mask` | true or false | `false` | `--bf-polish-mask` | Keep the background-removal mask's largest connected component, holes filled, as STI Suite's PolishMask. |
| `bg_removal.ukb_mask_trim` | true or false | `false` | `--bf-ukb-mask-trim` | Trim the mask by the phase reliability of the first and last echo before and after background removal, as UK Biobank's pipeline does. |
| `bg_removal.vsharp.deconvolve` | true or false | `false` | `--vsharp-deconvolve` | Undo the SMV high-pass with a truncated-SVD deconvolution. |
| `bg_removal.vsharp.threshold` | number, greater than 0 | `0.05` | `--vsharp-threshold` | Deconvolution (TSVD) threshold. |
| `bg_removal.vsharp.max_radius` | number, mm | `12.0` | `--vsharp-max-radius` | Largest SMV kernel radius. |
| `bg_removal.vsharp.min_radius` | number, mm, greater than 0 | `2.0` | `--vsharp-min-radius` | Smallest SMV kernel radius. |
| `bg_removal.vsharp.radius_step` | number, mm, greater than 0 | `1.0` | `--vsharp-radius-step` | Step between SMV kernel radii, from the largest down to the smallest. |
| `bg_removal.vsharp.grid` | choice | `"isotropic"` | `--vsharp-grid` | The voxel grid V-SHARP runs on. Values: `native`, `finest`, `isotropic`. |
| `bg_removal.vsharp.grid_spacing` | number, mm, greater than 0 | `1.0` | `--vsharp-grid-spacing` | Isotropic working-grid spacing. |
| `bg_removal.pdf.tol` | number | `0.1` | `--pdf-tol` | Convergence tolerance, relative to the noise level. |
| `bg_removal.pdf.n_cg` | whole number, at least 1 | `30` | `--pdf-n-cg` | Maximum conjugate-gradient iterations. |
| `bg_removal.pdf.space` | choice | `"image"` | `--pdf-space` | The dipole kernel the background is fitted with. Values: `image`, `kspace`. |
| `bg_removal.pdf.n_pad` | whole number, voxels, at least 0 | `40` | `--pdf-n-pad` | Zero-padding around the mask's bounding box; 0 runs on the full volume. |
| `bg_removal.lbv.tol` | number, at least 0 | `0.01` | `--lbv-tol` | Stop the coarsest grid's relaxation when its summed absolute update falls below this. |
| `bg_removal.lbv.depth` | whole number, from 0 to 8 | unset | `--lbv-depth` | Coarsest multigrid level. Blank = automatic (log2 of the smallest dimension minus 3). May be left unset. |
| `bg_removal.lbv.peel` | whole number, voxels, at least 0 | `0` | `--lbv-peel` | Boundary layers removed from the mask beyond the first. |
| `bg_removal.lbv.n1` | whole number, at least 0 | `30` | `--lbv-n1` | Relaxations on each grid before the coarser grid. |
| `bg_removal.lbv.n2` | whole number, at least 0 | `100` | `--lbv-n2` | Relaxations on each grid after the coarser grid. |
| `bg_removal.lbv.n3` | whole number, at least 0 | `100` | `--lbv-n3` | Relaxations on the finest grid at the end. |
| `bg_removal.lbv.precision` | choice | `"double"` | `--lbv-precision` | Arithmetic of the multigrid solver. Values: `double`, `single`. |
| `bg_removal.ismv.variant` | choice | `"wen2014"` | `--ismv-variant` | Which implementation of iSMV runs. Values: `wen2014`, `qsmjl`. |
| `bg_removal.ismv.tol` | number, greater than 0 | `0.00005` | `--ismv-tol` | Stop when the relative change between iterates falls below this (QSM.jl: the residual relative to the first). |
| `bg_removal.ismv.max_iter` | whole number, at least 1 | unset | `--ismv-max-iter` | Maximum iterations. Blank = no limit. May be left unset. |
| `bg_removal.ismv.radius` | number, mm, greater than 0 | unset | `--ismv-radius` | SMV kernel radius. Blank = 6 × the smallest voxel dimension (QSM.jl: 2 × the largest). May be left unset. |
| `bg_removal.sharp.threshold` | number, greater than 0 | `0.03` | `--sharp-threshold` | Deconvolution threshold. |
| `bg_removal.sharp.radius` | number, mm | `4.0` | `--sharp-radius` | SMV kernel radius. |
| `bg_removal.resharp.radius` | number, mm | `3.0` | `--resharp-radius` | SMV kernel radius. |
| `bg_removal.resharp.tik_reg` | number | `0.0001` | `--resharp-tik-reg` | Tikhonov regularisation parameter. |
| `bg_removal.resharp.tol` | number | `1e-6` | `--resharp-tol` | CGS relative tolerance. |
| `bg_removal.resharp.max_iter` | whole number, at least 1 | `200` | `--resharp-max-iter` | Maximum CGS iterations. |
| `bg_removal.harperella.radius` | number, mm | `10.0` | `--harperella-radius` | SMV kernel radius. |
| `bg_removal.harperella.max_iter` | whole number | `40` | `--harperella-max-iter` | Maximum iterations. |
| `bg_removal.harperella.tol` | number | `0.001` | `--harperella-tol` | Convergence tolerance. |
| `bg_removal.harperella.pad` | three whole numbers, voxels | `[0, 0, 0]` | `--harperella-pad` | Zero-padding per side (x y z). Larger costs time and memory. |
| `bg_removal.harperella.rim` | choice | `"whole"` | `--harperella-rim` | Which Laplacian values in the brain's boundary rim are set to zero. Values: `whole`, `sti`. |
| `bg_removal.iharperella.max_iter` | whole number | `40` | `--iharperella-max-iter` | Maximum iterations. |
| `bg_removal.iharperella.tol` | number | `0.001` | `--iharperella-tol` | Convergence tolerance. |
| `bg_removal.iharperella.pad` | three whole numbers, voxels | `[0, 0, 0]` | `--iharperella-pad` | Zero-padding per side (x y z). Larger costs time and memory. |
| `bg_removal.iharperella.rim` | choice | `"sti"` | `--iharperella-rim` | Which Laplacian values in the brain's boundary rim are set to zero. Values: `whole`, `sti`. |

### QSM: Inversion

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.do_qsm` | true or false | `true` | `--no-qsm` | Enable QSM reconstruction (disable to only run the other outputs). |
| `inversion.algorithm` | choice | `"ilsqr"` | `--qsm-algorithm` | How the field is inverted into a susceptibility map. Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `star`, `lsqr`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `qsmnetme`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `inversion.rts.delta` | number, less than 2/3 | `0.15` | `--rts-delta` | Threshold for ill-conditioned k-space region. |
| `inversion.rts.mu` | number | `100000.0` | `--rts-mu` | Regularization parameter for well-conditioned region. |
| `inversion.rts.rho` | number | `10.0` | `--rts-rho` | ADMM penalty parameter. |
| `inversion.rts.tol` | number | `0.01` | `--rts-tol` | Convergence tolerance (relative change). |
| `inversion.rts.max_iter` | whole number | `20` | `--rts-max-iter` | Maximum ADMM iterations. |
| `inversion.rts.lsmr_iter` | whole number | `4` | `--rts-lsmr-iter` | LSMR iterations for step 1 (well-conditioned solve). |
| `inversion.rts.tau` | number | `0.7` | `--rts-tau` | Rho is raised when the primal residual falls by less than this factor. |
| `inversion.rts.gamma` | number | `5.0` | `--rts-gamma` | Factor rho is raised by. |
| `inversion.rts.pad` | three whole numbers, voxels, at least -1 | `[0, 0, 0]` | `--rts-pad` | Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded. |
| `inversion.tv.lambda` | number | `0.0003` | `--tv-lambda` | L1 regularisation weight (larger = smoother). |
| `inversion.tv.rho` | number, greater than 0 | unset | `--tv-rho` | ADMM penalty parameter. Blank = 100×lambda. May be left unset. |
| `inversion.tv.tol` | number | `0.001` | `--tv-tol` | Convergence tolerance. |
| `inversion.tv.max_iter` | whole number, at least 1 | `250` | `--tv-max-iter` | Maximum ADMM iterations. |
| `inversion.tv.pad` | three whole numbers, voxels, at least -1 | `[0, 0, 0]` | `--tv-pad` | Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded. |
| `inversion.tkd.threshold` | number, from 0 to 2/3 (not 0 or 2/3) | `0.25` | `--tkd-threshold` | Truncation threshold for k-space division (0.1-0.3); larger is smoother and underestimates χ more. |
| `inversion.tkd.pad` | three whole numbers, voxels, at least -1 | `[0, 0, 0]` | `--tkd-pad` | Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded. |
| `inversion.tsvd.threshold` | number, from 0 to 2/3 (not 2/3) | `0.15` | `--tsvd-threshold` | Truncation threshold for SVD (0.1-0.2). |
| `inversion.tsvd.pad` | three whole numbers, voxels, at least -1 | `[0, 0, 0]` | `--tsvd-pad` | Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded. |
| `inversion.tgv.iterations` | whole number, at least 1 | unset | `--tgv-iterations` | Primal-dual iterations. Blank = from the voxel size, as the reference does. May be left unset. |
| `inversion.tgv.erosions` | whole number | `3` | `--tgv-erosions` | Mask erosions before TGV solve. |
| `inversion.tgv.alpha1` | number, rad, at least 0 | `0.00212` | `--tgv-alpha1` | First-order TGV weight (gradient term), at the echo time and field strength. |
| `inversion.tgv.alpha0` | number, rad, at least 0 | `0.00636` | `--tgv-alpha0` | Second-order TGV weight (symmetric gradient term), at the echo time and field strength. |
| `inversion.tgv.step_size` | number | `3.0` | `--tgv-step-size` | Primal step-size multiplier. |
| `inversion.tgv.te` | echo time | `"first"` | `--tgv-te` | The echo time the field is converted to phase at (several echoes; a single echo is at its own); the weights are in radians at it. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.tikhonov.lambda` | number | `0.01` | `--tikhonov-lambda` | L2 regularisation weight. |
| `inversion.tikhonov.pad` | three whole numbers, voxels, at least -1 | `[0, 0, 0]` | `--tikhonov-pad` | Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded. |
| `inversion.nltv.lambda` | number | `0.001` | `--nltv-lambda` | Regularisation parameter. |
| `inversion.nltv.rho` | number, greater than 0 | unset | `--nltv-rho` | ADMM penalty parameter. Blank = 100×lambda. May be left unset. |
| `inversion.nltv.mu` | number | `1.0` | `--nltv-mu` | Penalty parameter. |
| `inversion.nltv.tol` | number | `0.001` | `--nltv-tol` | Convergence tolerance. |
| `inversion.nltv.max_iter` | whole number, at least 1 | `250` | `--nltv-max-iter` | Maximum ADMM iterations. |
| `inversion.nltv.tol_newton` | number | `1e-6` | `--nltv-tol-newton` | Convergence tolerance of the Newton iterations of the data-term subproblem. |
| `inversion.nltv.max_iter_newton` | whole number, at least 1 | `10` | `--nltv-newton-iter` | Maximum Newton iterations of the data-term subproblem. |
| `inversion.nltv.te` | echo time | `"first"` | `--nltv-te` | The echo time the field is converted to phase at. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.nltv.pad` | three whole numbers, voxels, at least -1 | `[0, 0, 0]` | `--nltv-pad` | Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded. |
| `inversion.medi.smv` | true or false | `true` | `--no-medi-smv` | High-pass the local field with a spherical mean value (SMV) filter, removing residual background near the brain edge (MEDI-SMV). |
| `inversion.medi.smv_radius` | number, mm | `5.0` | `--medi-smv-radius` | SMV radius (also mSMV's). |
| `inversion.medi.smv_shrink_mask` | true or false | `true` | `--no-medi-smv-shrink-mask` | With SMV, shrink the mask to where the SMV of the mask exceeds 0.999, dropping the edge band whose SMV reaches outside it. Off keeps the full mask. |
| `inversion.medi.msmv` | true or false | `false` | `--medi-msmv` | Filter the local field by maximum spherical mean value (mSMV) before MEDI, which removes residual background at the brain edge; needs at least three echoes. |
| `inversion.medi.msmv_maxk` | whole number, at least 1 | `5` | `--medi-msmv-maxk` | mSMV's boundary correction makes at most this many passes, less one. |
| `inversion.medi.msmv_vessel_radius` | number, voxels, greater than 0 | unset | `--medi-msmv-vessel-radius` | Largest vessel thickness of mSMV's vessel mask. Empty: 15 voxels, or 8 times the largest voxel dimension if that is more. May be left unset. |
| `inversion.medi.msmv_prefilter` | true or false | `true` | `--no-medi-msmv-prefilter` | SMV-filter the field before mSMV, as the authors do. Off diverges after PDF, V-SHARP or LBV (expert). |
| `inversion.medi.lambda` | number | `1000.0` | `--medi-lambda` | Data-fidelity weight (larger fits the field more closely). |
| `inversion.medi.percentage` | number | `0.9` | `--medi-percentage` | Fraction of voxels considered edges (0.0-1.0). |
| `inversion.medi.max_iter` | whole number, at least 1 | `10` | `--medi-max-iter` | Maximum outer iterations. |
| `inversion.medi.cg_max_iter` | whole number, at least 1 | `100` | `--medi-cg-max-iter` | Maximum conjugate gradient iterations. |
| `inversion.medi.cg_tol` | number | `0.01` | `--medi-cg-tol` | CG convergence tolerance. |
| `inversion.medi.tol` | number | `0.1` | `--medi-tol` | Outer convergence tolerance. |
| `inversion.medi.merit` | true or false | `true` | `--medi-merit` | Iteratively down-weight voxels whose residual stays large. |
| `inversion.medi.csf` | true or false | `true` | `--medi-csf` | Zero-reference the susceptibility to the ventricular CSF, found from R2\*, and keep it uniform there (MEDI+0); needs at least three echoes, with fewer MEDI runs without it. |
| `inversion.medi.lambda_csf` | number, at least 0 | `100.0` | `--medi-lambda-csf` | Weight of the CSF uniformity term. |
| `inversion.medi.csf_r2s_threshold` | number, Hz, greater than 0 | `5.0` | `--medi-csf-r2s-threshold` | Voxels below this R2\* are CSF candidates. |
| `inversion.medi.csf_erode` | true or false | `true` | `--medi-csf-erode` | Erode the mask by a 10 mm sphere before finding the CSF. |
| `inversion.medi.zeropad` | three whole numbers, voxels | `[0, 0, 0]` | `--medi-zeropad` | Voxels of zeros appended at the end of each axis before the inversion; the result is cropped back. |
| `inversion.medi.data_weighting` | choice | `1` | `--medi-data-weighting` | How the voxels of the field are weighted in the data term. Values: `0`, `1`. |
| `inversion.medi.te` | echo time | `"spacing"` | `--medi-te` | The echo time the field is converted to phase at. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.tfi.lambda` | number | `1000.0` | `--tfi-lambda` | Data-fidelity weight (larger fits the field more closely). |
| `inversion.tfi.precond` | number | `30.0` | `--tfi-precond` | Preconditioner value (susceptibility scaling outside the mask). |
| `inversion.tfi.percentage` | number | `0.9` | `--tfi-percentage` | Fraction of voxels considered edges (0.0-1.0). |
| `inversion.tfi.max_iter` | whole number, at least 1 | `10` | `--tfi-max-iter` | Maximum outer iterations. |
| `inversion.tfi.cg_max_iter` | whole number, at least 1 | `100` | `--tfi-cg-max-iter` | Maximum conjugate gradient iterations. |
| `inversion.tfi.cg_tol` | number | `0.01` | `--tfi-cg-tol` | CG convergence tolerance. |
| `inversion.tfi.tol` | number | `0.1` | `--tfi-tol` | Outer convergence tolerance. |
| `inversion.tfi.merit` | true or false | `true` | `--tfi-merit` | Iteratively down-weight voxels whose residual stays large. |
| `inversion.tfi.linear` | true or false | `true` | `--tfi-linear` | Fit the field linearly, as the TFI paper does; off uses MEDI's exponential data term. |
| `inversion.tfi.data_weighting` | choice | `1` | `--tfi-data-weighting` | How the voxels of the field are weighted in the data term. Values: `0`, `1`. |
| `inversion.tfi.te` | echo time | `"spacing"` | `--tfi-te` | The echo time the field is converted to phase at. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.ilsqr.tol` | number | `0.001` | `--ilsqr-tol` | Convergence tolerance. |
| `inversion.ilsqr.max_iter` | whole number | `100` | `--ilsqr-max-iter` | Maximum iterations. |
| `inversion.ilsqr.pad_mm` | number, mm, at least 0 | `12.0` | `--ilsqr-pad-mm` | Zero-padding per side. Larger costs time and memory. |
| `inversion.ilsqr.precision` | choice | `"double"` | `--ilsqr-precision` | Arithmetic of the two least-squares solves. Values: `double`, `single`. |
| `inversion.star.pad` | three whole numbers, voxels | `[8, 8, 20]` | `--star-pad` | Zero-padding per side (x y z). Larger costs time and memory. |
| `inversion.star.te` | echo time | `"sum"` | `--star-te` | The echo time the field is converted to phase at; STAR-QSM's result depends on it. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.lsqr.residual_weighting` | number | unset | `--lsqr-residual-weighting` | Weight w on the residual field, the only regularisation. Empty: scaled from B0; 0 drops the term and streaks badly. May be left unset. |
| `inversion.lsqr.fit_global_offset` | true or false | `true` | `--no-lsqr-global-offset` | Fit a single global field offset alongside χ. |
| `inversion.lsqr.tol` | number | `0.00001` | `--lsqr-tol` | LSQR convergence tolerance. |
| `inversion.lsqr.max_iter` | whole number, at least 1 | `400` | `--lsqr-max-iter` | Maximum LSQR iterations. |
| `inversion.heidi.enabled` | true or false | `false` | `--heidi` | Streaking reduction after the inversion: HEIDI keeps the map's well-conditioned k-space and refills the dipole cone under a homogeneity prior. With LSQR this is HEIDI as published. |
| `inversion.heidi.cone_threshold` | number, less than 2/3 | `0.1` | `--heidi-cone-threshold` | \|D(k)\| above which a coefficient is kept from the inversion's map unchanged. |
| `inversion.heidi.gradient_threshold` | number, ppm/mm | `0.003924908970520748` | `--heidi-gradient-threshold` | Field-gradient threshold; below it a direction reads as homogeneous. |
| `inversion.heidi.apply_laplacian_correction` | true or false | `true` | `--no-heidi-laplacian-correction` | Open the homogeneity masks where the field's Laplacian is large. |
| `inversion.heidi.laplacian_threshold` | number | `0.05943433583931419` | `--heidi-laplacian-threshold` | Field-Laplacian threshold (ppm per voxel²). |
| `inversion.heidi.gradient_mask_floor` | number | `0.0` | `--heidi-gradient-mask-floor` | TV weight kept where every direction reads as an edge (0 = upstream's binarised masks). |
| `inversion.heidi.continuation_steps` | whole number | `8` | `--heidi-continuation-steps` | NESTA continuation steps on μ. |
| `inversion.heidi.inner_iterations` | whole number | `500` | `--heidi-inner-iterations` | Accelerated-gradient iterations per continuation step. |
| `inversion.heidi.mu_min` | number, greater than 0 | `2.220446049250313e-16` | `--heidi-mu-min` | Final (smallest) smoothing parameter μ. |
| `inversion.heidi.tol` | number | `2.220446049250313e-16` | `--heidi-tol` | Relative objective change that ends an inner loop early (the default runs every iteration). |
| `inversion.heidi.te` | echo time | `"last"` | `--heidi-te` | The echo time the field is converted to phase at, in the conversion to HEIDI's units. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.heidi.denoise` | true or false | `true` | `--no-heidi-denoise` | Anisotropic-diffusion smoothing of the field before its gradients are taken. |
| `inversion.heidi.denoise_iterations` | whole number | `5` | `--heidi-denoise-iterations` | Anisotropic-diffusion iterations. |
| `inversion.heidi.denoise_time_step` | number | `0.05` | `--heidi-denoise-time-step` | Anisotropic-diffusion time step. |
| `inversion.heidi.denoise_conductance` | number | `1.0` | `--heidi-denoise-conductance` | Anisotropic-diffusion conductance. |
| `inversion.heidi.denoise_reference_itk` | true or false | `false` | `--heidi-denoise-reference-itk` | ITK 3.20's float arithmetic for the denoising, as the authors' code runs it (for comparison with it; the default computes its conductance scale in double precision). |
| `inversion.qsmart.inversion` | choice | `"ilsqr"` | `--qsmart-inversion` | The dipole inversion QSMART runs in each of its two stages. Values: `ilsqr`, `rts`, `tv`, `tkd`, `tsvd`, `tikhonov`, `nltv`, `medi`. |
| `inversion.qsmart.ilsqr_max_iter` | whole number, at least 1 | `50` | `--qsmart-ilsqr-max-iter` | Maximum iLSQR iterations per stage (tolerance: iLSQR's). |
| `inversion.qsmart.ilsqr_pad_mm` | number, mm, at least 0 | `6.0` | `--qsmart-ilsqr-pad-mm` | Zero-padding of the inner iLSQR solves, per side. |
| `inversion.qsmart.sdf_sigma1_stage1` | number, voxels | `10.0` | `--qsmart-sdf-sigma1-stage1` | Stage 1 SDF kernel sigma 1. |
| `inversion.qsmart.sdf_sigma2_stage1` | number, voxels | `10.0` | `--qsmart-sdf-sigma2-stage1` | Stage 1 SDF kernel sigma 2. |
| `inversion.qsmart.sdf_sigma1_stage2` | number, voxels | `8.0` | `--qsmart-sdf-sigma1-stage2` | Stage 2 SDF kernel sigma 1. |
| `inversion.qsmart.sdf_sigma2_stage2` | number, voxels | `2.0` | `--qsmart-sdf-sigma2-stage2` | Stage 2 SDF kernel sigma 2. |
| `inversion.qsmart.sdf_lower_lim` | number | `0.6` | `--qsmart-sdf-lower-lim` | SDF proximity lower limit. |
| `inversion.qsmart.sdf_curv_constant` | number | `500.0` | `--qsmart-sdf-curv-constant` | SDF curvature constant. |
| `inversion.qsmart.vessel_magnitude` | choice | `"mean-n4"` | `--qsmart-vessel-magnitude` | The magnitude vessels are detected on. Values: `mean-n4`, `combined`. |
| `inversion.qsmart.offset_grid` | choice | `"fft"` | `--qsmart-offset-grid` | The dipole grid QSMART's offset adjustment uses. Values: `fft`, `qsmart-reference`. |
| `inversion.qsmart.vessel_size_units` | choice | `"voxels"` | `--qsmart-vessel-units` | The units of the vasculature sphere radius and the Frangi scales. Values: `voxels`, `mm`. |
| `inversion.qsmart.vasc_sphere_radius` | whole number | `8` | `--qsmart-vasc-sphere-radius` | Bottom-hat sphere radius for vasculature detection, in the vessel size units. |
| `inversion.qsmart.frangi_scale_min` | number | `0.5` | `--qsmart-frangi-scale-min` | Frangi minimum vessel radius, in the vessel size units. |
| `inversion.qsmart.frangi_scale_max` | number | `6.0` | `--qsmart-frangi-scale-max` | Frangi maximum vessel radius, in the vessel size units. |
| `inversion.qsmart.frangi_scale_ratio` | number, greater than 0 | `0.5` | `--qsmart-frangi-scale-ratio` | Frangi scale step, in the vessel size units. |
| `inversion.qsmart.frangi_c_mode` | choice | `"absolute"` | `--qsmart-frangi-c-mode` | How Frangi's C is set: in magnitude units, or relative to the magnitude. Values: `absolute`, `relative`. |
| `inversion.qsmart.frangi_c` | number, greater than 0 | `500.0` | `--qsmart-frangi-c` | Frangi C noise threshold, in the vessel magnitude's intensity units. |
| `inversion.qsmart.frangi_c_factor` | number, greater than 0 | `0.25` | `--qsmart-frangi-c-factor` | Frangi C as this factor × the vessel magnitude's median inside the brain mask. |
| `inversion.ndi.tau` | number | `2.0` | `--ndi-tau` | Gradient-descent step size. |
| `inversion.ndi.alpha` | number | `0.00001` | `--ndi-alpha` | L2 regularisation weight. |
| `inversion.ndi.max_iter` | whole number, at least 1 | `100` | `--ndi-max-iter` | Number of iterations. |
| `inversion.ndi.auto_stop` | true or false | `false` | `--ndi-auto-stop` | Stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto). |
| `inversion.ndi.te` | echo time | `"first"` | `--ndi-te` | The echo time the field is converted to phase at. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.ndi.weight_norm` | choice | `"global-max"` | `--ndi-weight-norm` | What the magnitude is divided by to make the data weight. Values: `global-max`, `mask-p999`. |
| `inversion.fansi.alpha1` | number | `0.0002` | `--fansi-alpha1` | First-order (TV/TGV) L1 penalty weight. |
| `inversion.fansi.mu1` | number, greater than 0 | unset | `--fansi-mu1` | Gradient-consistency ADMM weight. Blank = 100×alpha1. May be left unset. |
| `inversion.fansi.mu2` | number | `1.0` | `--fansi-mu2` | Fidelity-consistency ADMM weight. |
| `inversion.fansi.alpha0` | number, at least 0 | unset | `--fansi-alpha0` | Second-order L1 penalty weight (nlTGV only). Blank = 2×alpha1. May be left unset. |
| `inversion.fansi.mu0` | number, greater than 0 | unset | `--fansi-mu0` | Second-order consistency ADMM weight (nlTGV only). Blank = 2×mu1. May be left unset. |
| `inversion.fansi.max_iter` | whole number, at least 1 | `150` | `--fansi-max-iter` | Number of outer ADMM iterations. |
| `inversion.fansi.tol_update` | number | `0.1` | `--fansi-tol-update` | Percent-update stopping tolerance. |
| `inversion.fansi.tol_delta` | number | `1e-6` | `--fansi-tol-delta` | Inner Newton convergence tolerance. |
| `inversion.fansi.te` | echo time | `"spacing"` | `--fansi-te` | The echo time the field is converted to phase at. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.l1qsm.alpha1` | number | `0.005` | `--l1qsm-alpha1` | Gradient (TV) L1 penalty weight. |
| `inversion.l1qsm.mu1` | number, greater than 0 | unset | `--l1qsm-mu1` | Gradient-consistency ADMM weight. Blank = 100×alpha1. May be left unset. |
| `inversion.l1qsm.mu2` | number | `1.0` | `--l1qsm-mu2` | Fidelity-consistency ADMM weight. |
| `inversion.l1qsm.mu3` | number | `1.0` | `--l1qsm-mu3` | L1 proximal ADMM weight. |
| `inversion.l1qsm.lambda` | number | `0.8` | `--l1qsm-lambda` | L1 fidelity strength. |
| `inversion.l1qsm.max_iter` | whole number, at least 1 | `50` | `--l1qsm-max-iter` | Number of outer ADMM iterations. |
| `inversion.l1qsm.tol_update` | number | `1.0` | `--l1qsm-tol-update` | Percent-update stopping tolerance. |
| `inversion.l1qsm.tol_delta` | number | `1e-6` | `--l1qsm-tol-delta` | Inner Newton convergence tolerance. |
| `inversion.l1qsm.te` | echo time | `"first"` | `--l1qsm-te` | The echo time the field is converted to phase at. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.whqsm.alpha1` | number | `0.0002` | `--whqsm-alpha1` | TV regularisation weight. |
| `inversion.whqsm.mu1` | number | `0.02` | `--whqsm-mu1` | ADMM penalty for TV splitting. |
| `inversion.whqsm.mu2` | number | `1.0` | `--whqsm-mu2` | ADMM penalty for data-fidelity splitting. |
| `inversion.whqsm.beta` | number | `150.0` | `--whqsm-beta` | Weak-harmonic ROI penalty. |
| `inversion.whqsm.muh` | number | `5.0` | `--whqsm-muh` | ADMM penalty for harmonic-field splitting. |
| `inversion.whqsm.max_iter` | whole number | `150` | `--whqsm-max-iter` | Maximum outer iterations. |
| `inversion.whqsm.tol_update` | number | `0.1` | `--whqsm-tol-update` | Percent-update stopping tolerance. |
| `inversion.whqsm.tol_delta` | number | `1e-6` | `--whqsm-tol-delta` | Inner Newton stopping tolerance. |
| `inversion.whqsm.weight` | choice | `"flat"` | `--whqsm-weight` | How the voxels of the field are weighted in the data term. Values: `flat`, `flat-in-mask`, `max-normalised`, `in-vivo`. |
| `inversion.whqsm.te` | echo time | `"first"` | `--whqsm-te` | The echo time the field is converted to phase at. Values: `first`, `last`, `spacing`, `sum`, a time in seconds, `{ seconds = … }`. |
| `inversion.hdqsm.alpha_l2` | number, at least 0 | `0.00003` | `--hdqsm-alpha-l2` | L2-stage TV weight. |
| `inversion.hdqsm.mu1_l2` | number, greater than 0 | unset | `--hdqsm-mu1-l2` | L2-stage gradient-consistency ADMM weight. Blank = 100×alpha L2. May be left unset. |
| `inversion.hdqsm.mu2` | number | `1.0` | `--hdqsm-mu2` | Fidelity consistency weight. |
| `inversion.hdqsm.max_iter_l1` | whole number | `20` | `--hdqsm-max-iter-l1` | Stage-1 (L1) iterations. |
| `inversion.hdqsm.max_iter_l2` | whole number | `80` | `--hdqsm-max-iter-l2` | Stage-2 (L2) iterations. |
| `inversion.hdqsm.tol_update` | number | `0.01` | `--hdqsm-tol-update` | Stage-2 percent-update stopping tolerance. |
| `inversion.amp_pe.wave_order` | whole number, from 1 to 4 | `1` | `--amp-pe-wave-order` | Daubechies wavelet order (1=db1, 2=db2). |
| `inversion.amp_pe.nlevel` | whole number, at most 63 | `3` | `--amp-pe-nlevel` | Wavelet decomposition levels. |
| `inversion.amp_pe.wave_pec` | number | `0.85` | `--amp-pe-wave-pec` | Morphology-mask energy retention fraction (0.0-1.0). |
| `inversion.amp_pe.simulated_te` | number, s | `0.008` | `--amp-pe-simulated-te` | Simulated echo time used to turn the field into phase. |
| `inversion.amp_pe.max_linearization_ite` | whole number, at least 1 | `25` | `--amp-pe-max-linearization-ite` | Linearisation iterations per stage. |
| `inversion.amp_pe.tikhonov_beta` | number | `0.02` | `--amp-pe-tikhonov-beta` | L2-seed Tikhonov weight. |
| `inversion.amp_pe.damp_rate_sig` | number | `0.01` | `--amp-pe-damp-rate-sig` | GAMP signal-update damping rate. |
| `inversion.amp_pe.damp_rate_par` | number | `0.1` | `--amp-pe-damp-rate-par` | Parameter-estimation learning rate (kappa). |
| `inversion.amp_pe.max_pe_spar_ite` | whole number | `5` | `--amp-pe-max-pe-spar-ite` | Inner sparse-reconstruction iterations. |
| `inversion.amp_pe.max_pe_est_ite` | whole number | `5` | `--amp-pe-max-pe-est-ite` | Inner parameter-estimation iterations. |
| `inversion.amp_pe.cvg_thd` | number | `1e-6` | `--amp-pe-cvg-thd` | GAMP inner convergence threshold. |
| `inversion.tile_size` | whole number, voxels | unset | `--tile-size` | Run the network on overlapping tiles: bounded memory, approximate. Off = whole volume. Tile size: Core size of each overlapping tile. Unset turns it off. |
| `inversion.tile_halo` | whole number, voxels | unset | `--tile-halo` | Context margin around each tile (empty = 8). May be left unset. |

### QSM: Multi-orientation

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `multi_orientation.algorithm` | choice | `"cosmos"` | `--multi-orientation-algorithm` | How the orientations of a group are combined into one map. Values: `cosmos`, `sti`. |
| `multi_orientation.lambda` | number | `0.0` | `--multi-orientation-lambda` | Regularisation weight; 0 is plain least squares. Raise it for 2 orientations or single-axis rotations. |
| `multi_orientation.reference` | text | `""` | `--orientation-reference` | Orientation the others are registered onto, whose grid and segmentation the group map uses: a glob over the run key (\*acq-Neutral\*) or a word in it. Blank = first by name. Pick the neutral head position. |
| `multi_orientation.magnitude_weighting` | true or false | `false` | `--cosmos-magnitude-weighting` | COSMOS groups: weight each orientation by its combined magnitude (conjugate-gradient solve). Off = closed-form least squares. |
| `multi_orientation.cosmos_tol` | number, greater than 0 | `1e-6` | `--cosmos-tol` | Conjugate-gradient tolerance of weighted COSMOS. |
| `multi_orientation.cosmos_max_iter` | whole number, at least 1 | `100` | `--cosmos-max-iter` | Conjugate-gradient iteration cap of weighted COSMOS. |
| `multi_orientation.register` | true or false | `true` | `--no-orientation-registration` | Co-register orientations that need it, with the registration settings below. Off = refuse such groups. |
| `multi_orientation.force` | true or false | `false` | `--multi-orientation-force` | Reconstruct even when the B0 directions look degenerate (wrong metadata gives a plausible wrong map). |

### QSM: Reference

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `qsm.reference` | choice | `"mean"` | `--qsm-reference` | What the susceptibility map is referenced to: the value that reads as zero. Values: `mean`, `none`, `region`. |
| `qsm.reference_region` | choice | unset | `--qsm-reference` | The SynthSeg region whose mean susceptibility is zero. Requires Segmentation. Values: `ventricles`, `cerebral-white-matter`, `cerebral-cortex`, `lateral-ventricle`, `inferior-lateral-ventricle`, `cerebellum-white-matter`, `cerebellum-cortex`, `thalamus`, `caudate`, `putamen`, `pallidum`, `3rd-ventricle`, `4th-ventricle`, `brain-stem`, `hippocampus`, `amygdala`, `csf`, `accumbens-area`, `ventral-dc`, `left-cerebral-white-matter`, `left-cerebral-cortex`, `left-lateral-ventricle`, `left-inferior-lateral-ventricle`, `left-cerebellum-white-matter`, `left-cerebellum-cortex`, `left-thalamus`, `left-caudate`, `left-putamen`, `left-pallidum`, `left-hippocampus`, `left-amygdala`, `left-accumbens-area`, `left-ventral-dc`, `right-cerebral-white-matter`, `right-cerebral-cortex`, `right-lateral-ventricle`, `right-inferior-lateral-ventricle`, `right-cerebellum-white-matter`, `right-cerebellum-cortex`, `right-thalamus`, `right-caudate`, `right-putamen`, `right-pallidum`, `right-hippocampus`, `right-amygdala`, `right-accumbens-area`, `right-ventral-dc`. May be left unset. |

### χ-separation

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.do_chi_separation` | true or false | `false` | `--do-chisep` | Split QSM into paramagnetic (χ+, iron) and diamagnetic (χ−, myelin) maps. |
| `separation.algorithm` | choice | `"r2star-qsm"` | `--chisep` | The χ-separation method, and the maps it reads. Values: `r2star-qsm`, `decompose`, `chi-sep-ilsqr`, `chi-sep-medi`, `wavesep`, `hc-chisep`, `susep-net`, `chi-sepnet`. |
| `separation.r2star_qsm.r_const_3t` | number, Hz/ppm | `274.0` | `--r2star-qsm-r-const-3t` | Relaxometric constant at 3 T (scales with B0). |
| `separation.decompose.n_inner` | whole number | `10` | `--decompose-n-inner` | Alternating 3-stage fit passes per voxel. |
| `separation.decompose.chi_bound` | number or choice, ppm, at least 0 | `0.5` | `--decompose-chi-bound` | The bound on \|χ\| in the fit: a fixed bound, or one from the field strength and echo spacing. Values: a number, or `echo-spacing`. |
| `separation.decompose.unwrap_phase` | true or false | `false` | `--decompose-unwrap-phase` | Unwrap the phase along the echoes. |
| `separation.decompose.fit` | choice | `"reference-code"` | `--decompose-fit` | The fit DECOMPOSE runs: its reference code's or its paper's. Values: `reference-code`, `paper`. |
| `separation.decompose.lsqcurvefit.max_iterations` | whole number, at least 1 | `400` | `--decompose-max-iterations` | Maximum solver iterations per fit (lsqcurvefit MaxIterations). |
| `separation.decompose.lsqcurvefit.max_function_evaluations` | whole number, at least 1 | unset | `--decompose-max-function-evaluations` | Maximum function evaluations per fit (lsqcurvefit MaxFunctionEvaluations). Blank = 100 × the fit's parameters. May be left unset. |
| `separation.decompose.lsqcurvefit.function_tolerance` | number, at least 0 | `1e-6` | `--decompose-function-tolerance` | Stop when the cost changes by less than this, relatively (lsqcurvefit FunctionTolerance). |
| `separation.decompose.lsqcurvefit.step_tolerance` | number, at least 0 | `1e-6` | `--decompose-step-tolerance` | Stop when the step is smaller than this, relatively (lsqcurvefit StepTolerance). |
| `separation.decompose.lsqcurvefit.optimality_tolerance` | number, at least 0 | `1e-6` | `--decompose-optimality-tolerance` | Stop when the first-order optimality falls below this (lsqcurvefit OptimalityTolerance). |
| `separation.decompose.lsqcurvefit.finite_difference_step` | number, greater than 0 | `1.4901161193847656e-8` | `--decompose-finite-difference-step` | Relative finite-difference step of the Jacobian (lsqcurvefit FiniteDifferenceStepSize). |
| `separation.decompose.lsqcurvefit.typical_x` | number, greater than 0 | `1.0` | `--decompose-typical-x` | Typical magnitude of each fitted value, for the finite differences (lsqcurvefit TypicalX). |
| `separation.chi_sep_ilsqr.dr_pos` | number, Hz/ppm, greater than 0 | unset | `--chi-sep-ilsqr-dr-pos` | Paramagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T. May be left unset. |
| `separation.chi_sep_ilsqr.dr_neg` | number, Hz/ppm, greater than 0 | unset | `--chi-sep-ilsqr-dr-neg` | Diamagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T. May be left unset. |
| `separation.chi_sep_ilsqr.pad` | three whole numbers, voxels | `[16, 16, 16]` | `--chi-sep-ilsqr-pad` | Zero-padding at both ends of each axis (x y z). |
| `separation.chi_sep_ilsqr.cg_max_iter_lsqr` | whole number | `50` | `--chi-sep-ilsqr-cg-max-iter-lsqr` | Maximum CG iterations of the LSQR step. |
| `separation.chi_sep_ilsqr.cg_tol_lsqr` | number, from 0 to 1 (not 0 or 1) | `0.02` | `--chi-sep-ilsqr-cg-tol-lsqr` | CG relative tolerance of the LSQR step. |
| `separation.chi_sep_ilsqr.cg_max_iter_sa` | whole number | `50` | `--chi-sep-ilsqr-cg-max-iter-sa` | Maximum CG iterations of the streaking-removal step, per source. |
| `separation.chi_sep_ilsqr.cg_tol_sa` | number, from 0 to 1 (not 0 or 1) | `0.02` | `--chi-sep-ilsqr-cg-tol-sa` | CG relative tolerance of the streaking-removal step. |
| `separation.chi_sep_ilsqr.d2_thr` | number, greater than 0 | `0.11` | `--chi-sep-ilsqr-d2-thr` | The streaking cone is where the dipole kernel \|D\| is below this. |
| `separation.chi_sep_ilsqr.prc_w_i_min` | number, from 0 to 100 | `60.0` | `--chi-sep-ilsqr-prc-w-i-min` | Percentile of the field's Laplacian at which the LSQR weight is 1. |
| `separation.chi_sep_ilsqr.prc_w_i_max` | number, from 0 to 100 | `99.8` | `--chi-sep-ilsqr-prc-w-i-max` | Percentile of the field's Laplacian at which the LSQR weight is 0. |
| `separation.chi_sep_ilsqr.prc_w_g_min` | number, from 0 to 100 | `50.0` | `--chi-sep-ilsqr-prc-w-g-min` | Percentile of each gradient's magnitude at which the streaking-removal weight is 1. |
| `separation.chi_sep_ilsqr.prc_w_g_max` | number, from 0 to 100 | `70.0` | `--chi-sep-ilsqr-prc-w-g-max` | Percentile of each gradient's magnitude at which the streaking-removal weight is 0. |
| `separation.chi_sep_ilsqr.qsm_ilsqr.max_iter` | whole number, at least 1 | `100` | `--chi-sep-ilsqr-qsm-max-iter` | Maximum iterations of the iLSQR that reconstructs the QSM separated. |
| `separation.chi_sep_ilsqr.qsm_ilsqr.tol` | number | `0.001` | `--chi-sep-ilsqr-qsm-tol` | Tolerance of the iLSQR that reconstructs the QSM separated. |
| `separation.chi_sep_ilsqr.qsm_ilsqr.pad_mm` | number, mm, at least 0 | `12.0` | `--chi-sep-ilsqr-qsm-pad-mm` | Zero-padding per side of the iLSQR that reconstructs the QSM separated. |
| `separation.chi_sep_ilsqr.qsm_ilsqr.precision` | choice | `"double"` | `--chi-sep-ilsqr-qsm-precision` | Arithmetic of the two least-squares solves of the iLSQR that reconstructs the QSM separated. Values: `double`, `single`. |
| `separation.chi_sep_medi.lambda` | number, at least 0 | `1.0` | `--chi-sep-medi-lambda` | Weight of the data terms against the L1 terms. |
| `separation.chi_sep_medi.lambda_csf` | number, at least 0 | `1.0` | `--chi-sep-medi-lambda-csf` | Weight of the CSF-uniformity term. |
| `separation.chi_sep_medi.dr_pos` | number, Hz/ppm, greater than 0 | unset | `--chi-sep-medi-dr-pos` | Paramagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T. May be left unset. |
| `separation.chi_sep_medi.dr_neg` | number, Hz/ppm, greater than 0 | unset | `--chi-sep-medi-dr-neg` | Diamagnetic relaxometric constant. Blank = 137 × B0 / 2.9 T. May be left unset. |
| `separation.chi_sep_medi.pad` | whole number, voxels | `5` | `--chi-sep-medi-pad` | Zero-padding at both ends of the third axis. |
| `separation.chi_sep_medi.tol` | number, at least 0 | `0.03` | `--chi-sep-medi-tol` | Stop when the relative change falls below this. |
| `separation.chi_sep_medi.max_iter` | whole number, at least 1 | `30` | `--chi-sep-medi-max-iter` | Maximum Gauss-Newton iterations. |
| `separation.chi_sep_medi.cg_tol` | number, at least 0 | `0.05` | `--chi-sep-medi-cg-tol` | Inner CG tolerance. |
| `separation.chi_sep_medi.cg_max_iter` | whole number | `100` | `--chi-sep-medi-cg-max-iter` | Inner CG max iterations. |
| `separation.chi_sep_medi.qsm_ilsqr.max_iter` | whole number, at least 1 | `100` | `--chi-sep-medi-qsm-max-iter` | Maximum iterations of the iLSQR that reconstructs the QSM χ-separation starts from. |
| `separation.chi_sep_medi.qsm_ilsqr.tol` | number | `0.001` | `--chi-sep-medi-qsm-tol` | Tolerance of the iLSQR that reconstructs the QSM χ-separation starts from. |
| `separation.chi_sep_medi.qsm_ilsqr.pad_mm` | number, mm, at least 0 | `12.0` | `--chi-sep-medi-qsm-pad-mm` | Zero-padding per side of the iLSQR that reconstructs the QSM χ-separation starts from. |
| `separation.chi_sep_medi.qsm_ilsqr.precision` | choice | `"double"` | `--chi-sep-medi-qsm-precision` | Arithmetic of the two least-squares solves of the iLSQR that reconstructs the QSM χ-separation starts from. Values: `double`, `single`. |
| `separation.chi_sep_medi.csf_r2star` | choice | `"arlo"` | `--chi-sep-medi-csf-r2star` | The R2\* fit the CSF mask is extracted from. Values: `arlo`, `nnls`. |
| `separation.wavesep.dr_pos` | number, Hz/ppm | `137.0` | `--wavesep-dr-pos` | Paramagnetic relaxivity. |
| `separation.wavesep.dr_neg` | number, Hz/ppm | `137.0` | `--wavesep-dr-neg` | Diamagnetic relaxivity. |
| `separation.wavesep.alpha` | number | `0.2` | `--wavesep-alpha` | Proximal-gradient step size. |
| `separation.wavesep.lambda` | number | `0.02` | `--wavesep-lambda` | Wavelet L1 soft-threshold weight. |
| `separation.wavesep.wavelet_order` | whole number, from 1 to 4 | `4` | `--wavesep-wavelet-order` | Daubechies order. |
| `separation.wavesep.max_iter` | whole number | `100` | `--wavesep-max-iter` | ISTA max iterations. |
| `separation.wavesep.tol` | number | `0.001` | `--wavesep-tol` | Relative-change stop tolerance. |
| `separation.hc_chisep.dr_pos_3t` | number, Hz/ppm, greater than 0 | `137.0` | `--hc-chisep-dr-pos-3t` | Paramagnetic relaxivity at 3T. |
| `separation.hc_chisep.bin_hz` | number, Hz | `0.25` | `--hc-chisep-bin-hz` | R2' bin width for the anchored grid search. |
| `separation.chi_sepnet.route` | choice | `"toolbox"` | `--chi-sepnet-route` | How χ-sepnet's inputs are prepared: the authors' route or the network alone. Values: `toolbox`, `native`. |
| `separation.chi_sepnet.patch` | three whole numbers, voxels, at least 16 | `[192, 192, 128]` | `--chi-sepnet-patch` | Sliding-window patch D H W, each a multiple of 16. |
| `separation.chi_sepnet.frame_patch` | three whole numbers, voxels, at least 16 | unset | `--chi-sepnet-frame-patch` | Patch size for running the networks piece by piece, which needs less memory, each a multiple of 16; empty runs the whole frame at once. May be left unset. |
| `separation.susep_net.patch` | three whole numbers, voxels, at least 8 | unset | `--susep-net-patch` | Patch size for running SUSEP-Net piece by piece, which needs less memory; empty runs the whole volume at once. May be left unset. |
| `separation.custom_qsm_tool` | text | unset | `--use-custom-qsm` | Use a χ map (Chimap) from &lt;bids&gt;/derivatives/&lt;tool&gt;/ instead of computing QSM, for χ-separation, the statistics and SMWI: \* for any tool, or a tool name; empty to compute it. May be left unset. |

### Other outputs: Magnitude

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.do_magnitude` | true or false | `true` | `--no-magnitude` | Publish the combined magnitude (part-mag_T2starw): the root sum of squares of the echoes, corrected for inhomogeneity when that is on. Off, it is still made for SMWI, segmentation or a magnitude-weighted inversion, and kept with the run's intermediates. |

### Other outputs: SWI/SMWI

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.do_swi` | true or false | `false` | `--do-swi` | Also compute susceptibility-weighted images with CLEAR-SWI, from every echo. |
| `swi.unwrap` | choice | `"laplacian"` | `--swi-unwrapping` | How the phase is unwrapped and high-pass filtered for SWI. Values: `laplacian`, `laplacian-slice`, `romeo`. |
| `swi.scaling` | choice | `"tanh"` | `--swi-scaling` | How the filtered phase is turned into the mask the magnitude is multiplied by. Values: `tanh`, `negative-tanh`, `positive`, `negative`, `triangular`. |
| `swi.strength` | number | `4.0` | `--swi-strength` | Phase scaling strength (higher = stronger phase contrast). |
| `swi.hp_sigma` | three numbers, voxels | `[4.0, 4.0, 0.0]` | `--swi-hp-sigma` | High-pass filter sigma per axis (x y z): background phase removal. Set z to 0 for thin axial slices. |
| `swi.mip_window` | whole number, slices, at least 1 | `7` | `--swi-mip-window` | Minimum-intensity-projection window. |
| `swi.filtered_phase` | true or false | `false` | `--swi-filtered-phase` | Also write the high-pass filtered phase the SWI is weighted by (desc-swi_phase): Hz for several echoes, radians for one. |
| `pipeline.do_smwi` | true or false | `false` | `--do-smwi` | Susceptibility map-weighted imaging — weights the magnitude by a mask built from χ, not from filtered phase. Needs QSM. |
| `smwi.threshold_ppm` | number, ppm | `1.0` | `--smwi-threshold` | \|χ\| at which the weighting mask reaches zero. 1 ppm targets strong sources; brain tissue wants far less. |
| `smwi.power` | number, at least 0 | `4.0` | `--smwi-power` | Power the mask is raised to (contrast strength). |
| `smwi.mip_window` | whole number, slices, at least 1 | `4` | `--smwi-mip-window` | Minimum-intensity-projection window. |

### Other outputs: Relaxometry

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.do_t2starmap` | true or false | `false` | `--do-t2starmap` | Compute T2\* relaxation map (requires 3+ echoes with magnitude). |
| `pipeline.do_r2starmap` | true or false | `false` | `--do-r2starmap` | Compute R2\* decay rate map (requires 3+ echoes with magnitude). |
| `pipeline.do_r2map` | true or false | `false` | `--do-r2map` | R2 map from a multi-echo spin-echo (MESE) acquisition via EPG. Required when χ-separation needs it. |
| `separation.custom_r2_tool` | text | unset | `--use-custom-r2` | Use an R2 map from &lt;bids&gt;/derivatives/&lt;tool&gt;/ instead of computing it from MESE: \* for any tool, or a tool name; empty to compute it. May be left unset. |
| `pipeline.do_r2primemap` | true or false | `false` | `--do-r2primemap` | R2' = R2\* − R2; needs GRE magnitude and a MESE acquisition, or R2PRIMEnet. Required when χ-separation needs it. |
| `separation.custom_r2prime_tool` | text | unset | `--use-custom-r2prime` | Use an R2' map from &lt;bids&gt;/derivatives/&lt;tool&gt;/ instead of computing it: \* for any tool, or a tool name; empty to compute it. May be left unset. |
| `separation.r2prime_strategy` | choice | `"auto"` | `--r2prime-strategy` | How R2' is obtained when no R2' map is supplied. Values: `auto`, `mese`, `r2primenet`. |
| `separation.r2primenet.ignore_field_mismatch` | true or false | `false` | `--r2primenet-ignore-field-mismatch` | Run R2PRIMEnet on non-3 T data (trained at 3 T only; the R2' is uncalibrated). Off: auto skips R2', r2primenet errors. |
| `separation.r2primenet.patch` | three whole numbers, voxels, at least 16 | `[192, 192, 128]` | `--r2primenet-patch` | Sliding-window patch D H W, each a multiple of 16. |

### Other outputs: Segmentation

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.do_segmentation` | true or false | `false` | `--do-segmentation` | Whole-brain parcellation: SynthSeg straight off the GRE magnitude, or one supplied in derivatives. Required by the statistics and a region reference: choosing one offers to turn it on (left off, the run turns it on itself). |
| `segmentation.custom_dseg_tool` | text | unset | `--use-custom-dseg` | Use a segmentation (dseg) from &lt;bids&gt;/derivatives/&lt;tool&gt;/ instead of running SynthSeg: \* for any tool, or a tool name. SynthSeg runs where none is found. May be left unset. |
| `segmentation.version` | choice | `"v1"` | `--synthseg-version` | Which SynthSeg model segments the brain. Values: `v1`, `v2`. |
| `segmentation.crop` | whole number, voxels, at least 1 | unset | `--synthseg-crop` | Cubic centre crop at 1 mm before inference; empty processes the whole volume. May be left unset. |
| `segmentation.patch` | whole number, voxels, at least 64 | unset | `--synthseg-patch` | Patch size for running SynthSeg piece by piece, which needs less memory; empty runs the whole volume at once. May be left unset. |
| `segmentation.flip_averaging` | true or false | `true` | `--no-synthseg-flip-averaging` | Average with a left-right flipped second pass (SynthSeg's default). Doubles inference time. |
| `segmentation.topology_cleanup` | true or false | `true` | `--no-synthseg-topology-cleanup` | Reset each topological class to its largest connected component. |
| `segmentation.sigma_smoothing` | number, voxels, at least 0 | `0.5` | `--synthseg-sigma` | Gaussian blur on the posteriors before the labels are taken. Sigma: Gaussian sigma of the posterior blur. `0.0` turns it off. |
| `pipeline.do_analysis` | true or false | `false` | `--do-analysis` | Summarise every map the run makes (χ, χ+/χ−, T2\*, R2\*, R2, R2') per structure into a TSV: mean, SD, median, min, max, 5th/95th percentiles. Requires segmentation, and QSM only if no other map is enabled. |

### Other outputs: Export

| Setting | Type | Default | Option | Description |
| --- | --- | --- | --- | --- |
| `pipeline.export_dicom` | true or false | `false` | `--export-dicom` | Also write final maps as DICOM series into each subject's extra_files/ folder. |

<!-- generated:settings end -->
