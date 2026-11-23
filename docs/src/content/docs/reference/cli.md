---
title: Command line
description: "Every qsmxt command and option, with its accepted values and defaults."
---

QSMxT is a single binary with a set of commands: `qsmxt run` and its companions run the
pipeline on a BIDS dataset, and the others run one step of it on NIfTI files. The options and
their descriptions on this page are generated from the binary, so they are exactly what it
accepts; `qsmxt <command> --help` prints the same text. Every `qsmxt run` option can also be
set in a [settings file](/QSMxT/reference/configuration/).

The step commands (`mask` to `quality-map`) read and write NIfTI files and run the same code as
the pipeline's stages. A method's settings are `qsmxt run`'s flags, with the same names,
defaults and meaning (`qsmxt invert rts --rts-delta`, `qsmxt bgremove pdf --pdf-tol`), so a
step gives the same result here as in a pipeline.

<!-- generated:cli-commands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

| Command | What it does |
| --- | --- |
| [`qsmxt run`](#qsmxt-run) | Run the full QSM pipeline on a BIDS dataset |
| [`qsmxt init`](#qsmxt-init) | Generate a pipeline configuration file |
| [`qsmxt validate`](#qsmxt-validate) | Validate BIDS dataset structure for QSM processing |
| [`qsmxt dicom-convert`](#qsmxt-dicom-convert) | Convert a DICOM directory to BIDS using automatic series classification |
| [`qsmxt slurm`](#qsmxt-slurm) | Generate SLURM job scripts for HPC execution: one job per run, then an aggregation job |
| [`qsmxt aggregate`](#qsmxt-aggregate) | Run the dataset-level steps over runs already processed with `qsmxt run --no-dataset-steps` (multi-orientation, summary tables, provenance, clean-up) |
| [`qsmxt mask`](#qsmxt-mask) | Masking operations |
| [`qsmxt combine`](#qsmxt-combine) | Multi-coil phase combination (uncombined receive-coil channels -&gt; combined echoes) |
| [`qsmxt unwrap`](#qsmxt-unwrap) | Phase unwrapping |
| [`qsmxt fieldmap`](#qsmxt-fieldmap) | B0 field mapping from multi-echo phase (NIfTI in, ppm out) |
| [`qsmxt bgremove`](#qsmxt-bgremove) | Background field removal |
| [`qsmxt invert`](#qsmxt-invert) | Dipole inversion |
| [`qsmxt separate`](#qsmxt-separate) | Susceptibility source separation |
| [`qsmxt qsmart`](#qsmxt-qsmart) | QSMART vessel-aware reconstruction: total field -&gt; susceptibility |
| [`qsmxt swi`](#qsmxt-swi) | Susceptibility-weighted imaging |
| [`qsmxt segment`](#qsmxt-segment) | Whole-brain segmentation of a magnitude image with SynthSeg |
| [`qsmxt r2star`](#qsmxt-r2star) | R2\* mapping from multi-echo magnitude data |
| [`qsmxt t2star`](#qsmxt-t2star) | T2\* mapping from multi-echo magnitude data |
| [`qsmxt r2`](#qsmxt-r2) | R2 mapping from multi-echo spin-echo (MESE) magnitude data (EPG) |
| [`qsmxt r2prime`](#qsmxt-r2prime) | R2' mapping: measured as R2\* - R2, or estimated from R2\* by R2PRIMEnet |
| [`qsmxt homogeneity`](#qsmxt-homogeneity) | Inhomogeneity correction on magnitude data (makehomogeneous or N4) |
| [`qsmxt resample`](#qsmxt-resample) | Resample oblique volume to axial orientation |
| [`qsmxt quality-map`](#qsmxt-quality-map) | Compute ROMEO phase quality map |
| [`qsmxt example`](#qsmxt-example) | Download an example BIDS dataset to try the pipeline on |
| [`qsmxt tui`](#qsmxt-tui) | Launch interactive TUI for pipeline configuration |
| [`qsmxt update`](#qsmxt-update) | Check for updates and optionally install the latest version |
| [`qsmxt news`](#qsmxt-news) | Show the announcements from the QSMxT team, newest first |

<!-- generated:cli-commands end -->

## `qsmxt run`

<!-- generated:cli-run start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Run the full QSM pipeline on a BIDS dataset

```sh
qsmxt run [OPTIONS] <BIDS_DIR> [OUTPUT_DIR]
```

| Argument | Description |
| --- | --- |
| `<BIDS_DIR>` | Input BIDS directory. Required. |
| `<OUTPUT_DIR>` | Output directory (defaults to bids_dir; outputs go into &lt;dir&gt;/derivatives/qsmxt/) |
| `--config <CONFIG>` | Pipeline configuration file (TOML) |
| `--include <INCLUDE>` | Include only runs matching these glob patterns (e.g. "sub-1\*" "\*ses-pre\*") |
| `--exclude <EXCLUDE>` | Exclude runs matching these glob patterns (e.g. "\*mygrea\*") |
| `--num-echoes <NUM_ECHOES>` | Limit number of echoes to process. |
| `--pipeline <PIPELINE>` | Start from a pipeline: qsmxt-defaults, or an original toolbox's (medi-toolbox, sti-suite-ilsqr, sti-suite-star, qsmart, heidi, chi-sep-ilsqr, chi-sep-medi, chi-sepnet, decompose). The stages it covers (masking, field mapping, background removal, inversion, referencing, χ-separation) go back to QSMxT's defaults and then take the toolbox's settings; the other flags given then change any setting. Values: `qsmxt-defaults`, `medi-toolbox`, `sti-suite-ilsqr`, `sti-suite-star`, `qsmart`, `heidi`, `chi-sep-ilsqr`, `chi-sep-medi`, `chi-sepnet`, `decompose`, `uk-biobank`. |
| `--orientation-group <PATTERN>` | Group runs into multi-orientation sets and reconstruct each with COSMOS or STI. Takes an entity name (acq, run, rec, inv), a glob over the run key (`*acq-dir*`), `re:` and a regex with one capture group, or `runs:` and the groups' run keys (`,` between runs, `;` between groups, each group's reference first). Orientations are co-registered if they need it. |
| `--axial-resampling` | Resample oblique acquisitions onto an axial grid when their obliquity exceeds --obliquity-threshold. Off by default (methods that only work on axial data resample regardless) |
| `--no-axial-resampling` | Do not resample oblique acquisitions to axial (overrides a config's axial_resampling) |
| `--obliquity-threshold <OBLIQUITY_THRESHOLD>` | Obliquity in degrees above which --axial-resampling resamples (default: 10) |
| `--output-space <OUTPUT_SPACE>` | Grid to write derivatives on when the run was resampled to axial. `acquired` (default) puts them back where the data came from, so transforms you already hold still apply; `working` leaves them on the resampled grid and avoids a second interpolation. Values: `acquired`, `working`. |
| `--ignore-slice-gap` | Reconstruct even when a 2D acquisition's sidecar `SliceThickness` is thinner than the slice pitch. Without it, QSM refuses such a run: the gap between slices is unsampled tissue, and the FFT kernels assume a contiguous grid, so the map would be wrong. |
| `--tukey-window <TUKEY_WINDOW>` | Apply a k-space Tukey window with this taper ratio (0 to 1) to each echo's complex image before every other step, as the χ-separation toolbox does: 0.4, and 0 for Philips data. Default 0 (none). Needs magnitude images. |
| `--crop-to-mask` | Reconstruct inside a box around the mask instead of the whole field of view. Cuts the FFT-bound stages roughly in proportion to the voxels removed, but those stages are a small share of a full run, so the end-to-end gain is usually modest. Off by default. Outputs are written on the full grid either way; this only changes where the arithmetic happens. Padding to an FFT-friendly size happens regardless. |
| `--crop-margin-mm <CROP_MARGIN_MM>` | Margin in mm around the mask when --crop-to-mask is set (default 32). Below ~32 mm the reconstruction starts to differ from the uncropped one; it must also clear the largest SMV kernel in use, and V-SHARP defaults to 12 mm. |
| `--fft-padding` | Pad the grid outward to an FFT-friendly size before the FFT-bound stages. Much faster on an awkward grid, and discards nothing, but it changes where k-space is sampled and so shifts the reconstruction slightly. Off by default. |
| `--coil-combination-sigma <COIL_COMBINATION_SIGMA>` | MCPC-3D-S coil combination: smoothing sigma for the per-coil phase offsets (3 values, in voxels; default 10 10 5, as in MriResearchTools). Only used for uncombined multi-coil runs. |
| `--mask-preset <MASK_PRESET>` | Mask preset: robust-threshold, bet, hd-bet (HD-BET deep-learning brain extraction + signal-gated erosion; needs a deep-learning build), bet-and-phase (BET on the magnitude AND a thresholded phase-quality map, then hole-filled and eroded), rs2-net (RS2-Net deep-learning rodent brain extraction), or mindgrab (MindGrab deep-learning brain extraction for any contrast; needs a deep-learning build) Values: `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab`. |
| `--masking-input <MASKING_INPUT>` | Masking input image (overrides the input of the configured mask sections; combine with --mask-preset, e.g. --mask-preset robust-threshold --masking-input magnitude) Values: `magnitude-first`, `magnitude`, `magnitude-last`, `phase-quality`. |
| `--mask <MASK_SECTIONS_CLI>` | Define a mask section (repeatable; multiple sections are combined with --mask-combine). Format: &lt;input&gt;,&lt;generator&gt;,&lt;refinement1&gt;,&lt;refinement2&gt;,... Example: phase-quality,threshold:otsu,dilate:2,fill-holes:0,erode:2 Example: magnitude,bet:0.5,erode:2 Example: magnitude-first,bet:0.2,min-intensity:100,majority,close:2 (QSMART's mask) Example: magnitude,hd-bet,signal-erode (HD-BET: `hd-bet[:full\|window\|XxYxZ\|low-memory][:tta][:step=F]`; signal-erode: `signal-erode[:threshold[:depth_cap[:global_erosions[:bias_sigma[:min_component]]]]]`) |
| `--mask-combine <MASK_COMBINE>` | How to combine multiple --mask sections: `or` (union, the default) or `and` (intersection — a voxel survives only where every section keeps it) Values: `or`, `and`. |
| `--mask-refine <MASK_REFINEMENTS_CLI>` | Refinement applied to the combined mask, after --mask-combine (repeatable). Takes the same refinement ops as --mask, e.g. `erode:2` or `fill-holes:0`; generators (threshold/bet/hd-bet) belong in a --mask section instead. Example: --mask-combine and --mask-refine fill-holes:0 --mask-refine erode:1. |
| `--use-custom-masks <USE_CUSTOM_MASKS>` | Prefer a bring-your-own mask from BIDS derivatives when present (else compute it). Bare flag = first matching derivatives tool (alphabetical); give a name (e.g. `bet`) to restrict to `derivatives/<tool>/sub-*/anat/*_mask.nii*`. Always falls back. |
| `--two-pass` | Two-pass artefact reduction: reconstruct a second time against a mask with holes at strong susceptibility sources, and merge the two reconstructions (how: --two-pass-mode). Strong sources (haemorrhage, calcification, implants) make the phase around them unrecoverable; leaving those voxels out keeps their streaking from spreading, while the ordinary mask still supplies values inside the holes. Roughly doubles background removal and inversion time, and is not free of downsides — where the holes were not artefactual it can be worse than a single pass. Based on doi:10.1002/mrm.29048 (Stewart et al.). Off by default. The conventional single-pass map is kept alongside as `_desc-singlepass_Chimap.nii`. |
| `--bet-fractional-intensity <BET_FRACTIONAL_INTENSITY>` | BET fractional intensity (0.0-1.0; smaller = larger brain). Applies to every BET step in the masking recipe, overriding a `bet:<f>` in a --mask spec. |
| `--bet-voxel-scale <FACTOR>` | Factor applied to the voxel sizes before BET runs. BET's surface model is tuned to a human brain in millimetres, so preclinical data has to be presented at human scale: 10 is the usual setting for mouse. 1 (the default) is the acquisition's own geometry. Applies to every BET step in the masking recipe. |
| `--bet-smoothness <BET_SMOOTHNESS>` | BET surface smoothness factor. Applies to every BET step in the masking recipe (default: 1) |
| `--bet-gradient-threshold <BET_GRADIENT_THRESHOLD>` | BET gradient threshold (-1 to 1). Applies to every BET step in the masking recipe (default: 0) |
| `--bet-iterations <BET_ITERATIONS>` | BET surface evolution iterations. Applies to every BET step in the masking recipe (default: 1000) |
| `--bet-subdivisions <BET_SUBDIVISIONS>` | BET icosphere subdivision level. Applies to every BET step in the masking recipe (default: 4) |
| `--bet-orientation <ORIENTATION>` | Which way round BET sees the left-right axis: `fsl` (the default; reversed for a neurologically stored image, as FSL's bet reads a NIfTI file) or `as-stored` (the voxels in the order they are stored, as MEDI's BET.m runs bet2). Applies to every BET step in the masking recipe. |
| `--two-pass-mask <TWO_PASS_SECTIONS_CLI>` | Define the reliable-pass mask used by --two-pass (repeatable; same format as --mask). Defaults to `phase-quality,threshold:otsu` — an unfilled threshold on the phase-quality map. Its holes are the point, so refinements that close them (fill-holes, close, gaussian) defeat the method. Folded with --two-pass-mask-combine, then --mask-refine like the main mask, and always intersected with it. |
| `--two-pass-mask-combine <TWO_PASS_MASK_COMBINE>` | How to combine multiple --two-pass-mask sections: `or` (union) or `and` (intersection). Defaults to the main mask's --mask-combine. Turns on --two-pass. Values: `or`, `and`. |
| `--two-pass-mode <TWO_PASS_MODE>` | How --two-pass makes and merges its second pass. `local` (default): only holes with a core of very low phase quality are kept (strong sources, not veins or the pallidum), background removal leaves no eroded rim around them, and the second pass is used within 6 mm of them, blended into the first out to 12 mm. `original`: the published algorithm (doi:10.1002/mrm.29048), every voxel outside the reliable mask is a hole and the second pass is used wherever it is defined. Turns on --two-pass. Values: `local`, `original`. |
| `--no-inhomogeneity-correction` | Disable inhomogeneity correction. |
| `--inhomogeneity-correction` | Apply inhomogeneity correction to magnitude before masking. |
| `--inhomogeneity-method <INHOMOGENEITY_METHOD>` | Inhomogeneity correction method: `makehomogeneous` (default; --homogeneity-\*) or `n4` (N4 bias field correction, as ANTs' N4BiasFieldCorrection; --n4-\*) Values: `makehomogeneous`, `n4`. |
| `--homogeneity-sigma-mm <HOMOGENEITY_SIGMA_MM>` | Inhomogeneity correction smoothing sigma in mm (makehomogeneous) (default: 7) |
| `--homogeneity-nbox <HOMOGENEITY_NBOX>` | makehomogeneous: boxes per dimension for the tissue segmentation the receive sensitivity is estimated from (default: 15) |
| `--n4-shrink-factor <N4_SHRINK_FACTOR>` | N4: Subsample the image by this factor before fitting the bias field (default: 4) |
| `--n4-max-iterations <ITERATIONS>` | N4: maximum iterations at each fitting level, one number a level (default 50 50 50 50); the B-spline mesh doubles from one level to the next. |
| `--n4-convergence-threshold <N4_CONVERGENCE_THRESHOLD>` | N4: stop a level once the change in the bias field (its coefficient of variation) falls to this (default 0: every iteration runs) |
| `--n4-mesh <X> <Y> <Z>` | N4: B-spline mesh elements per axis at the first level (x y z) (default: 1 1 1) |
| `--n4-spline-order <N4_SPLINE_ORDER>` | N4: order of the B-spline bias field, 1 to 3 (default: 3) |
| `--n4-bias-field-fwhm <N4_BIAS_FIELD_FWHM>` | N4: FWHM of the Gaussian modelling the bias field's blurring of the log-intensity histogram (default: 0.15) |
| `--n4-wiener-noise <N4_WIENER_NOISE>` | N4: Noise term of the Wiener deconvolution of the histogram (default: 0.01) |
| `--n4-histogram-bins <N4_HISTOGRAM_BINS>` | N4: Bins of the log-intensity histogram (default: 200) |
| `--n4-geometry <GEOMETRY>` | N4: the image geometry it is given: `scanner` (the scan's header, as ANTs reads the acquired NIfTI; the default) or `make-nii` (the axis-aligned header MATLAB's make_nii writes, as QSMART runs N4). N4's shrunk grid falls where the header puts it. |
| `--motion-correction` | Rigidly co-register every echo onto a reference echo (estimated on magnitude, applied to phase through the complex signal) before field mapping, and restrict the mask to voxels every echo covers. Off by default. Needs magnitude images. |
| `--motion-reference <first\|last\|N>` | Echo the others are aligned onto: `first` (default), `last`, or a 1-based echo number. |
| `--no-motion-chain` | Register every echo from the identity instead of seeding it from its neighbour's solution. |
| `--motion-chain-levels <MOTION_CHAIN_LEVELS>` | Pyramid levels for a seeded (chained) registration (default: 3) |
| `--motion-min-ncc <MOTION_MIN_NCC>` | NCC below which an echo's registration is reported as suspect (default 0.3, set for echoes; ~0.9 suits repeats) |
| `--registration-levels <REGISTRATION_LEVELS>` | Registration: Pyramid levels: the capture range; do not lower (default: 5) |
| `--registration-max-samples <REGISTRATION_MAX_SAMPLES>` | Registration: Voxels sampled per metric evaluation, per level (default: 200000) |
| `--registration-max-iterations <REGISTRATION_MAX_ITERATIONS>` | Registration: Powell cycles per level (default: 20) |
| `--registration-max-evaluations <REGISTRATION_MAX_EVALUATIONS>` | Registration: metric evaluations across all levels, a hard stop (default: 5000) |
| `--registration-tolerance <REGISTRATION_TOLERANCE>` | Registration: Relative cost improvement per cycle that counts as converged (default: 1e-5) |
| `--registration-rotation-step-deg <REGISTRATION_ROTATION_STEP_DEG>` | Registration: initial rotation step at the finest level, degrees (default: 3) |
| `--registration-translation-step-mm <REGISTRATION_TRANSLATION_STEP_MM>` | Registration: initial translation step at the finest level, mm (default: 3) |
| `--registration-min-overlap <REGISTRATION_MIN_OVERLAP>` | Registration: Smallest fraction of sampled voxels that must overlap (default: 0.25) |
| `--unwrapping-algorithm <UNWRAPPING_ALGORITHM>` | Unwrapping algorithm (default: romeo) Values: `romeo`, `laplacian`, `prelude`, `medi-region-growing`. |
| `--prelude-labelling <PRELUDE_LABELLING>` | PRELUDE: how the initial regions are labelled (default: auto) Values: `auto`, `slices`, `volume`. |
| `--prelude-partitions <N>` | PRELUDE: number of phase partitions the initial regions are formed from (default: 8) |
| `--laplacian-solver <LAPLACIAN_SOLVER>` | Poisson solver for `--unwrapping-algorithm laplacian`: `dct` (unweighted least squares, Neumann/DCT solve; default), `fft` (Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume), `mirror` (the same on the volume mirrored along each axis, as QSMART unwraps) or `mgpcg` (QSM.jl's MGPCG: Laplacian unwrapping with LBV background removal in one step; QSM.jl follows it with V-SHARP to remove remaining background) Values: `dct`, `fft`, `mirror`, `mgpcg`. |
| `--laplacian-fft-pad <VOXELS>` | Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or three (x y z). Default 64. |
| `--laplacian-unmasked` | `--unwrapping-algorithm laplacian`: unwrap the raw phase over the whole volume, instead of the phase set to zero outside the brain mask, as QSMART does. With `--b0-estimation qsmart-echofit` the field map is then kept over the whole volume. |
| `--no-laplacian-unmasked` | `--unwrapping-algorithm laplacian`: unwrap the phase set to zero outside the brain mask (the default), after a `--pipeline` that unwraps the whole volume. |
| `--laplacian-keep-outside` | `--unwrapping-algorithm laplacian` with a weighted-average echo combination: keep the field over the whole volume, not only inside the brain mask, as UK Biobank's pipeline hands it to V-SHARP. |
| `--no-laplacian-keep-outside` | Keep the Laplacian field inside the brain mask only (the default), after a `--pipeline` that keeps it over the whole volume. |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Remove the phase offset of multi-echo data with MCPC-3D-S (Eckstein et al., 2018) before unwrapping (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--romeo-individual` | ROMEO: unwrap each echo on its own (individual mode, for motion or time series) |
| `--no-romeo-individual` | ROMEO: unwrap the template echo spatially and the others temporally from it (default) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) (default: 1) |
| `--romeo-correct-global` | ROMEO: correct global 2π offsets, of each echo and between echoes (default: off) |
| `--no-romeo-correct-global` | ROMEO: do not correct global 2π offsets (default) |
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on; the romeo command line's romeo3 weights) |
| `--romeo-phase-linearity` | ROMEO: enable phase linearity weights (default: on; for a --pipeline that turns them off) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: Maximum number of seed regions (default: 1) |
| `--romeo-merge-regions` | ROMEO: merge neighboring regions after unwrapping (default: off) |
| `--romeo-correct-regions` | ROMEO: correct each region's median to nearest 0 (default: off) |
| `--romeo-wrap-addition <ROMEO_WRAP_ADDITION>` | ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0) |
| `--qsm-per-echo <BOOL>` | Reconstruct a QSM from each echo on its own (each echo's phase unwrapped and divided by its echo time, then background removal, dipole inversion and referencing per echo) and publish each echo's map (`desc-echo<n>_Chimap`) and their mean as the QSM (`Chimap`), instead of one QSM from the echo-combined field map. The echo combination (--b0-estimation), phase-offset removal and bipolar correction do not run. DECOMPOSE takes the echoes' maps. Multi-echo only; not with TGV, QSMART, iQSM, iQFM, two-pass, or χ-separation methods that fit the local field (default: false) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method (default: weighted-avg) Values: `weighted-avg`, `linear-fit`, `robust-fit`, `qsmart-echofit`, `complex-fit`. |
| `--qsmart-fit-threshold-mode <QSMART_FIT_THRESHOLD_MODE>` | `--b0-estimation qsmart-echofit`: how the fit residual at which a voxel is left out of QSMART's reconstruction is set: a percentile of the blurred residual over the volume (`--qsmart-fit-percentile`), or a fixed residual (`--qsmart-fit-threshold`; default, QSMART v1.0's 40). Without it, giving one of those two selects its mode. Values: `percentile`, `absolute`. |
| `--qsmart-fit-percentile <QSMART_FIT_PERCENTILE>` | `--b0-estimation qsmart-echofit`: the percentile of the blurred fit residual, in (0, 100], above which a voxel is left out of QSMART's reconstruction (default: 99) |
| `--qsmart-fit-threshold <QSMART_FIT_THRESHOLD>` | `--b0-estimation qsmart-echofit`: a fixed blurred fit residual above which a voxel is left out of QSMART's reconstruction, instead of a percentile (40 when the mode is `absolute` and none is given) (default: 40) |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO (default: phase-snr) Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--complex-fit-reltol <COMPLEX_FIT_RELTOL>` | Complex fit (`--b0-estimation complex-fit`): stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4) |
| `--complex-fit-max-iter <COMPLEX_FIT_MAX_ITER>` | Complex fit: Gauss-Newton iterations at most (default: 30) |
| `--bf-algorithm <BF_ALGORITHM>` | Background field removal algorithm (default: vsharp) Values: `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella`, `bfrnet`, `iqfm`. |
| `--bf-erode-voxels <N>` | Erode the brain mask by N voxels after field mapping, before background removal; the field map is not masked again (default: 0, none) |
| `--bf-polish-mask` | After background removal, keep the mask's largest connected component with its holes filled and zero the local field outside it, as STI Suite's QSM_GUI does (PolishMask) |
| `--no-bf-polish-mask` | Do not polish the background-removal mask (the default), after a `--pipeline` that does. |
| `--bf-ukb-mask-trim` | Trim the brain mask of voxels with unreliable phase before and after background removal, as UK Biobank's pipeline does (its phase-reliability maps of the first and last echo) |
| `--no-bf-ukb-mask-trim` | Do not trim the mask by phase reliability (the default), after a `--pipeline` that does. |
| `--vsharp-deconvolve <BOOL>` | Undo V-SHARP's SMV high-pass with a truncated-SVD deconvolution (default: false) |
| `--vsharp-threshold <VSHARP_THRESHOLD>` | V-SHARP deconvolution (TSVD) threshold; only used with --vsharp-deconvolve true (default: 0.05) |
| `--vsharp-max-radius <VSHARP_MAX_RADIUS>` | V-SHARP: Largest SMV kernel radius, in mm (default: 12) |
| `--vsharp-min-radius <VSHARP_MIN_RADIUS>` | V-SHARP: Smallest SMV kernel radius, in mm (default: 2) |
| `--vsharp-radius-step <MM>` | V-SHARP step between kernel radii in mm (default: 1); the radii run from --vsharp-max-radius down to --vsharp-min-radius. |
| `--vsharp-grid <VSHARP_GRID>` | Grid V-SHARP estimates the background on: `isotropic` (default; spacing from --vsharp-grid-spacing), `native` (the acquired grid), or `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened). The local field keeps native resolution. Values: `native`, `finest`, `isotropic`. |
| `--vsharp-grid-spacing <MM>` | V-SHARP isotropic working-grid spacing in mm (default 1); used with --vsharp-grid isotropic. |
| `--pdf-tol <PDF_TOL>` | PDF tolerance, relative to the noise level (default: 0.1) |
| `--pdf-n-cg <PDF_N_CG>` | PDF: Maximum conjugate-gradient iterations (default: 30) |
| `--pdf-space <SPACE>` | PDF dipole kernel: image (the FFT of the dipole sampled on the voxel grid) or kspace (the analytic k-space kernel) (default: image) |
| `--pdf-n-pad <VOXELS>` | PDF zero-padding around the mask's bounding box, in voxels; 0 runs on the full volume (default: 40) |
| `--lbv-tol <LBV_TOL>` | LBV tolerance: stop the coarsest grid's relaxation when its summed absolute update falls below this, in radians over the echo spacing (default: 0.01) |
| `--lbv-depth <LEVEL>` | LBV coarsest multigrid level, at most 8 (default: log2 of the smallest dimension minus 3) |
| `--lbv-peel <LAYERS>` | LBV: Boundary layers removed from the mask beyond the first, in voxels (default: 0) |
| `--lbv-n1 <LBV_N1>` | LBV: Relaxations on each grid before the coarser grid (default: 30) |
| `--lbv-n2 <LBV_N2>` | LBV: Relaxations on each grid after the coarser grid (default: 100) |
| `--lbv-n3 <LBV_N3>` | LBV: Relaxations on the finest grid at the end (default: 100) |
| `--lbv-precision <PRECISION>` | LBV arithmetic: double, or single as the original implementation (default: double) |
| `--ismv-variant <VARIANT>` | iSMV implementation: wen2014 (the authors' paper and code) or qsmjl (QSM.jl's) (default: wen2014) |
| `--ismv-tol <ISMV_TOL>` | iSMV tolerance: the relative change between iterates it stops at (qsmjl: the residual relative to the first) (default: 5e-5) |
| `--ismv-max-iter <ISMV_MAX_ITER>` | iSMV max iterations (default: no limit) |
| `--ismv-radius <MM>` | iSMV kernel radius in mm, or auto for 6 × the smallest voxel dimension (qsmjl: 2 × the largest) (default: auto) |
| `--sharp-threshold <SHARP_THRESHOLD>` | SHARP: Deconvolution threshold (default: 0.03) |
| `--sharp-radius <SHARP_RADIUS>` | SHARP: SMV kernel radius, in mm (default: 4) |
| `--resharp-radius <RESHARP_RADIUS>` | RESHARP: SMV kernel radius, in mm (default: 3) |
| `--resharp-tik-reg <RESHARP_TIK_REG>` | RESHARP: Tikhonov regularisation parameter (default: 0.0001) |
| `--resharp-tol <RESHARP_TOL>` | RESHARP conjugate gradients squared (CGS) relative tolerance (default: 1e-6) |
| `--resharp-max-iter <RESHARP_MAX_ITER>` | RESHARP: Maximum CGS iterations (default: 200) |
| `--harperella-radius <HARPERELLA_RADIUS>` | HARPERELLA: SMV kernel radius, in mm (default: 10) |
| `--harperella-max-iter <HARPERELLA_MAX_ITER>` | HARPERELLA: Maximum iterations (default: 40) |
| `--harperella-tol <HARPERELLA_TOL>` | HARPERELLA: Convergence tolerance (default: 0.001) |
| `--harperella-pad <X> <Y> <Z>` | HARPERELLA zero-padding in voxels per side, X Y Z (default: 0 0 0) |
| `--harperella-rim <RIM>` | HARPERELLA boundary rim: whole (zero every Laplacian value in the rim; default) or sti (zero the 20 % largest, as STI Suite does) |
| `--iharperella-max-iter <IHARPERELLA_MAX_ITER>` | iHARPERELLA: Maximum iterations (default: 40) |
| `--iharperella-tol <IHARPERELLA_TOL>` | iHARPERELLA: Convergence tolerance (default: 0.001) |
| `--iharperella-pad <X> <Y> <Z>` | iHARPERELLA zero-padding in voxels per side, X Y Z (default: 0 0 0) |
| `--iharperella-rim <RIM>` | iHARPERELLA boundary rim: whole (zero every Laplacian value in the rim) or sti (zero the 20 % largest, as STI Suite does; default) |
| `--no-qsm` | Skip QSM processing (only run supplementary outputs like SWI, T2\*, R2\*) |
| `--qsm-algorithm <QSM_ALGORITHM>` | QSM dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `star`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `qsmnetme`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--rts-delta <RTS_DELTA>` | RTS: Threshold for ill-conditioned k-space region (default: 0.15) |
| `--rts-mu <RTS_MU>` | RTS: Regularization parameter for well-conditioned region (default: 100000) |
| `--rts-rho <RTS_RHO>` | RTS rho (ADMM penalty) (default: 10) |
| `--rts-tol <RTS_TOL>` | RTS: Convergence tolerance (relative change) (default: 0.01) |
| `--rts-max-iter <RTS_MAX_ITER>` | RTS: Maximum ADMM iterations (default: 20) |
| `--rts-lsmr-iter <RTS_LSMR_ITER>` | RTS: LSMR iterations for step 1 (well-conditioned solve) (default: 4) |
| `--rts-tau <RTS_TAU>` | RTS: rho is raised when the primal residual falls by less than this factor (tau) (default: 0.7) |
| `--rts-gamma <RTS_GAMMA>` | RTS: the factor rho is raised by (gamma) (default: 5) |
| `--rts-pad <X> <Y> <Z>` | RTS: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |
| `--tv-lambda <TV_LAMBDA>` | TV-ADMM: L1 regularisation weight (larger = smoother) (default: 0.0003) |
| `--tv-rho <RHO>` | TV rho (ADMM penalty), or auto for 100 × lambda (default: auto) |
| `--tv-tol <TV_TOL>` | TV-ADMM: Convergence tolerance (default: 0.001) |
| `--tv-max-iter <TV_MAX_ITER>` | TV-ADMM: Maximum ADMM iterations (default: 250) |
| `--tv-pad <X> <Y> <Z>` | TV-ADMM: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |
| `--tkd-threshold <TKD_THRESHOLD>` | TKD threshold on \|D\|; larger is smoother and underestimates χ more (default: 0.25) |
| `--tkd-pad <X> <Y> <Z>` | TKD: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |
| `--tsvd-threshold <TSVD_THRESHOLD>` | TSVD: Truncation threshold for SVD (0.1-0.2) (default: 0.15) |
| `--tsvd-pad <X> <Y> <Z>` | TSVD: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |
| `--tgv-iterations <TGV_ITERATIONS>` | TGV iterations (default: from the voxel size, as the reference does) |
| `--tgv-erosions <TGV_EROSIONS>` | TGV: Mask erosions before TGV solve (default: 3) |
| `--tgv-alpha1 <TGV_ALPHA1>` | TGV alpha1 (first-order weight), in radians at the echo time (--tgv-te) and field strength (the reference's units) (default: 0.00212; QuantitativeSusceptibilityMappingTGV.jl's regularization 2 is 0.003) |
| `--tgv-alpha0 <TGV_ALPHA0>` | TGV alpha0 (second-order weight), in radians at the echo time (--tgv-te) and field strength (the reference's units) (default: 0.00636; QuantitativeSusceptibilityMappingTGV.jl's regularization 2 is 0.002) |
| `--tgv-step-size <TGV_STEP_SIZE>` | TGV: Primal step-size multiplier (default: 3) |
| `--tgv-te <TE>` | TGV: the echo time a field from several echoes is converted to phase at, and its weights are in radians at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first). A single echo is at its own. |
| `--tikhonov-lambda <TIKHONOV_LAMBDA>` | Tikhonov: L2 regularisation weight (default: 0.01) |
| `--tikhonov-pad <X> <Y> <Z>` | Tikhonov: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |
| `--nltv-lambda <NLTV_LAMBDA>` | NLTV: Regularisation parameter (default: 0.001) |
| `--nltv-rho <RHO>` | NLTV rho (ADMM penalty), or auto for 100 × lambda (default: auto) |
| `--nltv-mu <NLTV_MU>` | NLTV mu (penalty parameter) (default: 1) |
| `--nltv-tol <NLTV_TOL>` | NLTV: Convergence tolerance (default: 0.001) |
| `--nltv-max-iter <NLTV_MAX_ITER>` | NLTV: Maximum ADMM iterations (default: 250) |
| `--nltv-tol-newton <NLTV_TOL_NEWTON>` | NLTV: Convergence tolerance of the Newton iterations of the data-term subproblem (default: 1e-6) |
| `--nltv-newton-iter <NLTV_NEWTON_ITER>` | NLTV: Maximum Newton iterations of the data-term subproblem (default: 10) |
| `--nltv-te <TE>` | NLTV: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |
| `--nltv-pad <X> <Y> <Z>` | NLTV: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |
| `--no-medi-smv` | MEDI: no SMV high-pass of the local field. |
| `--medi-smv` | MEDI: high-pass the local field with a spherical mean value filter, removing residual background near the brain edge (MEDI-SMV; on by default) |
| `--medi-smv-radius <MEDI_SMV_RADIUS>` | MEDI SMV radius in mm (also mSMV's) (default: 5) |
| `--no-medi-smv-shrink-mask` | MEDI-SMV: keep the full mask, high-passing the field over it. |
| `--medi-smv-shrink-mask` | MEDI-SMV: shrink the mask to where the SMV of the mask exceeds 0.999 (on by default) |
| `--medi-msmv` | MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value before MEDI, which fits it with the SMV-filtered dipole kernel (it implies the SMV). Needs at least 3 echoes. |
| `--medi-msmv-maxk <MEDI_MSMV_MAXK>` | MEDI-mSMV: the boundary correction makes at most this many passes, less one (default: 5) |
| `--medi-msmv-vessel-radius <MEDI_MSMV_VESSEL_RADIUS>` | MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 × the largest voxel dimension if that is more) |
| `--no-medi-msmv-prefilter` | MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges (expert) |
| `--medi-lambda <MEDI_LAMBDA>` | MEDI lambda: the data-fidelity weight (larger fits the field more closely) (default: 1000) |
| `--medi-percentage <MEDI_PERCENTAGE>` | MEDI edge percentage (0.0-1.0) (default: 0.9) |
| `--medi-max-iter <MEDI_MAX_ITER>` | MEDI: Maximum outer iterations (default: 10) |
| `--medi-cg-max-iter <MEDI_CG_MAX_ITER>` | MEDI: Maximum conjugate gradient iterations (default: 100) |
| `--medi-cg-tol <MEDI_CG_TOL>` | MEDI: CG convergence tolerance (default: 0.01) |
| `--medi-tol <MEDI_TOL>` | MEDI: Outer convergence tolerance (default: 0.1) |
| `--medi-merit <MEDI_MERIT>` | MEDI: enable MERIT weighting (default: true) |
| `--medi-csf <MEDI_CSF>` | MEDI+0 (Liu et al. 2018): zero-reference χ to the ventricular CSF, found from R2\*, and keep it uniform there (default: true). Needs at least 3 echoes; with fewer, MEDI runs without it. |
| `--medi-lambda-csf <MEDI_LAMBDA_CSF>` | MEDI+0: weight of the CSF uniformity term (default: 100) |
| `--medi-csf-r2s-threshold <MEDI_CSF_R2S_THRESHOLD>` | MEDI+0: R2\* in Hz below which a voxel is a CSF candidate (default: 5) |
| `--medi-csf-erode <MEDI_CSF_ERODE>` | MEDI+0: erode the mask by a 10 mm sphere before finding the CSF (default: true) |
| `--medi-zeropad <X> <Y> <Z>` | MEDI: voxels of zeros appended at the end of x, y and z before the inversion; the result is cropped back (default: 0 0 0) |
| `--medi-data-weighting <MEDI_DATA_WEIGHTING>` | MEDI: data weighting mode (0=uniform, 1=SNR) (default: 1) |
| `--medi-te <TE>` | MEDI: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing) |
| `--tfi-lambda <TFI_LAMBDA>` | TFI lambda: the data-fidelity weight (larger fits the field more closely) (default: 1000) |
| `--tfi-precond <TFI_PRECOND>` | TFI: Preconditioner value (susceptibility scaling outside the mask) (default: 30) |
| `--tfi-percentage <TFI_PERCENTAGE>` | TFI edge percentage (0.0-1.0) (default: 0.9) |
| `--tfi-max-iter <TFI_MAX_ITER>` | TFI: Maximum outer iterations (default: 10) |
| `--tfi-cg-max-iter <TFI_CG_MAX_ITER>` | TFI: Maximum conjugate gradient iterations (default: 100) |
| `--tfi-cg-tol <TFI_CG_TOL>` | TFI: CG convergence tolerance (default: 0.01) |
| `--tfi-tol <TFI_TOL>` | TFI: Outer convergence tolerance (default: 0.1) |
| `--tfi-merit <TFI_MERIT>` | TFI: enable MERIT weighting (default: true) |
| `--tfi-linear <TFI_LINEAR>` | TFI: fit the field linearly, as the TFI paper does (default: true); false uses MEDI's exponential data term. |
| `--tfi-data-weighting <TFI_DATA_WEIGHTING>` | TFI: data weighting mode (0=uniform, 1=SNR) (default: 1) |
| `--tfi-te <TE>` | TFI: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing) |
| `--ilsqr-tol <ILSQR_TOL>` | iLSQR tolerance of the streaking-artefact solve (default: 0.001) |
| `--ilsqr-max-iter <ILSQR_MAX_ITER>` | iLSQR max iterations of each LSQR solve (default: 100) |
| `--ilsqr-pad-mm <MM>` | iLSQR zero-padding in mm per side (default: 12) |
| `--ilsqr-precision <PRECISION>` | iLSQR arithmetic of the two LSQR solves: double, or single as the original implementation (default: double) |
| `--star-pad <X> <Y> <Z>` | STAR-QSM zero-padding in voxels per side, X Y Z (default: 8 8 20) |
| `--star-te <TE>` | Echo time STAR-QSM converts the field to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: sum) |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance (default: 1e-5) |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR: Maximum LSQR iterations (default: 400) |
| `--heidi` | HEIDI streaking reduction after the inversion: keep the map's well-conditioned k-space and refill the dipole cone under a homogeneity prior (`--heidi-*` sets it up). With LSQR this is HEIDI as published (`--qsm-algorithm heidi`); with two-pass it runs once, on the combined map. |
| `--heidi-cone-threshold <HEIDI_CONE_THRESHOLD>` | \|D(k)\| above which a coefficient is well-conditioned and kept from the seed map (default: 0.1) |
| `--heidi-gradient-threshold <HEIDI_GRADIENT_THRESHOLD>` | Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous (default: 3.924908970520748e-3) |
| `--no-heidi-laplacian-correction` | Do not open the homogeneity masks where the field's Laplacian is large. |
| `--heidi-laplacian-threshold <HEIDI_LAPLACIAN_THRESHOLD>` | HEIDI: Field-Laplacian threshold (ppm per voxel²) (default: 5.943433583931419e-2) |
| `--heidi-gradient-mask-floor <HEIDI_GRADIENT_MASK_FLOOR>` | TV weight kept where every direction reads as an edge (0 reproduces upstream's shipped binarised masks) (default: 0) |
| `--heidi-continuation-steps <HEIDI_CONTINUATION_STEPS>` | HEIDI: NESTA continuation steps on μ (default: 8) |
| `--heidi-inner-iterations <HEIDI_INNER_ITERATIONS>` | HEIDI: Accelerated-gradient iterations per continuation step (default: 500) |
| `--heidi-mu-min <HEIDI_MU_MIN>` | HEIDI: Final (smallest) smoothing parameter μ (default: 2.220446049250313e-16) |
| `--heidi-tol <HEIDI_TOL>` | HEIDI: Relative objective change that ends an inner loop early (the default runs every iteration) |
| `--heidi-te <TE>` | HEIDI: the echo time the field stands for the phase of, in the conversion to HEIDI's units: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: last) |
| `--no-heidi-denoise` | Skip the anisotropic-diffusion smoothing of the field before its gradients are taken. |
| `--heidi-denoise-iterations <HEIDI_DENOISE_ITERATIONS>` | HEIDI: Anisotropic-diffusion iterations (default: 5) |
| `--heidi-denoise-time-step <HEIDI_DENOISE_TIME_STEP>` | HEIDI: Anisotropic-diffusion time step (default: 0.05) |
| `--heidi-denoise-conductance <HEIDI_DENOISE_CONDUCTANCE>` | HEIDI: Anisotropic-diffusion conductance (default: 1) |
| `--heidi-denoise-reference-itk` | HEIDI: ITK 3.20's float arithmetic for the denoising, as the authors' code runs it (for comparison with it; the default computes its conductance scale in double precision) |
| `--no-heidi-denoise-reference-itk` | The denoising in double precision, after a pipeline that turns ITK's float arithmetic on (the HEIDI pipeline) |
| `--qsmart-inversion <QSMART_INVERSION>` | QSMART inner dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `star`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `qsmnetme`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--qsmart-ilsqr-max-iter <QSMART_ILSQR_MAX_ITER>` | QSMART iLSQR max iterations per stage. The iLSQR tolerance is --ilsqr-tol (default: 50) |
| `--qsmart-ilsqr-pad-mm <MM>` | Zero-padding of QSMART's inner iLSQR solves, in mm per side (default: 6) |
| `--qsmart-sdf-sigma1-stage1 <QSMART_SDF_SIGMA1_STAGE1>` | QSMART SDF sigma1, stage 1 (voxels) (default: 10) |
| `--qsmart-sdf-sigma2-stage1 <QSMART_SDF_SIGMA2_STAGE1>` | QSMART SDF sigma2, stage 1 (voxels) (default: 10) |
| `--qsmart-sdf-sigma1-stage2 <QSMART_SDF_SIGMA1_STAGE2>` | QSMART SDF sigma1, stage 2 (voxels) (default: 8) |
| `--qsmart-sdf-sigma2-stage2 <QSMART_SDF_SIGMA2_STAGE2>` | QSMART SDF sigma2, stage 2 (voxels) (default: 2) |
| `--qsmart-sdf-lower-lim <QSMART_SDF_LOWER_LIM>` | QSMART: SDF proximity lower limit (default: 0.6) |
| `--qsmart-sdf-curv-constant <QSMART_SDF_CURV_CONSTANT>` | QSMART: SDF curvature constant (default: 500) |
| `--qsmart-vessel-magnitude <QSMART_VESSEL_MAGNITUDE>` | The magnitude QSMART detects vessels on: `mean-n4` (the mean over echoes, N4 bias-corrected, as QSMART prepares it; default) or `combined` (the root sum of squares over echoes, inhomogeneity-corrected unless --no-inhomogeneity-correction) Values: `mean-n4`, `combined`. |
| `--qsmart-offset-grid <QSMART_OFFSET_GRID>` | The dipole grid of QSMART's offset adjustment: `fft` (default) or `qsmart-reference` (`adjust_offset.m`'s own, half a step off the FFT grid on odd matrix sizes, as the QSMART pipeline runs it) Values: `fft`, `qsmart-reference`. |
| `--qsmart-vessel-units <QSMART_VESSEL_UNITS>` | Units of QSMART's vessel sizes (--qsmart-vasc-sphere-radius and the Frangi scales): `voxels` (default, as QSMART gives them) or `mm` (converted to voxels with the mean voxel size) Values: `voxels`, `mm`. |
| `--qsmart-vasc-sphere-radius <QSMART_VASC_SPHERE_RADIUS>` | QSMART vasculature detection sphere radius, in --qsmart-vessel-units (default: 8) |
| `--qsmart-frangi-scale-min <QSMART_FRANGI_SCALE_MIN>` | QSMART Frangi min vessel radius, in --qsmart-vessel-units (default: 0.5) |
| `--qsmart-frangi-scale-max <QSMART_FRANGI_SCALE_MAX>` | QSMART Frangi max vessel radius, in --qsmart-vessel-units (default: 6) |
| `--qsmart-frangi-scale-ratio <QSMART_FRANGI_SCALE_RATIO>` | QSMART Frangi scale step, in --qsmart-vessel-units (default: 0.5) |
| `--qsmart-frangi-c-mode <QSMART_FRANGI_C_MODE>` | How QSMART's Frangi C is set: `absolute` (--qsmart-frangi-c, in magnitude units; default, as QSMART gives it) or `relative` (--qsmart-frangi-c-factor × the vessel magnitude's median inside the brain mask, the same on every scanner's intensity scale) Values: `absolute`, `relative`. |
| `--qsmart-frangi-c <QSMART_FRANGI_C>` | QSMART Frangi C noise threshold, in the vessel magnitude's intensity units (default 500, QSMART v1.0's); used with --qsmart-frangi-c-mode absolute. |
| `--qsmart-frangi-c-factor <QSMART_FRANGI_C_FACTOR>` | QSMART's relative Frangi C: this factor × the vessel magnitude's median inside the brain mask (default 0.25); used with --qsmart-frangi-c-mode relative. |
| `--ndi-tau <NDI_TAU>` | NDI gradient-descent step size (tau) (default: 2) |
| `--ndi-alpha <NDI_ALPHA>` | NDI L2 regularization weight (alpha) (default: 1e-5) |
| `--ndi-max-iter <NDI_MAX_ITER>` | NDI: Number of iterations (default: 100) |
| `--ndi-auto-stop <NDI_AUTO_STOP>` | NDI: stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto); --ndi-max-iter is then the maximum (default: false) |
| `--ndi-te <TE>` | NDI: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |
| `--ndi-weight-norm <NORM>` | NDI magnitude-weight normaliser: global-max (the magnitude's maximum, as FANSI; default) or mask-p999 (its 99.9th percentile inside the mask; more consistent in vivo) |
| `--fansi-alpha1 <FANSI_ALPHA1>` | FANSI first-order (TV/TGV) L1 penalty weight (alpha1) (default: 0.0002) |
| `--fansi-mu1 <MU1>` | FANSI gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto) |
| `--fansi-mu2 <FANSI_MU2>` | FANSI fidelity-consistency ADMM weight (mu2) (default: 1) |
| `--fansi-alpha0 <ALPHA0>` | FANSI second-order L1 penalty weight (alpha0, nlTGV only), or auto for 2 × alpha1 (default: auto) |
| `--fansi-mu0 <MU0>` | FANSI second-order consistency ADMM weight (mu0, nlTGV only), or auto for 2 × mu1 (default: auto) |
| `--fansi-max-iter <FANSI_MAX_ITER>` | FANSI: Number of outer ADMM iterations (default: 150) |
| `--fansi-tol-update <FANSI_TOL_UPDATE>` | FANSI: Percent-update stopping tolerance (default: 0.1) |
| `--fansi-tol-delta <FANSI_TOL_DELTA>` | FANSI: Inner Newton convergence tolerance (default: 1e-6) |
| `--fansi-te <TE>` | FANSI (nlTV and nlTGV): the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing) |
| `--l1qsm-alpha1 <L1QSM_ALPHA1>` | L1-QSM gradient (TV) L1 penalty weight (alpha1) (default: 0.005) |
| `--l1qsm-mu1 <MU1>` | L1-QSM gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto) |
| `--l1qsm-mu2 <L1QSM_MU2>` | L1-QSM fidelity-consistency ADMM weight (mu2) (default: 1) |
| `--l1qsm-mu3 <L1QSM_MU3>` | L1-QSM L1 proximal ADMM weight (mu3) (default: 1) |
| `--l1qsm-lambda <L1QSM_LAMBDA>` | L1-QSM L1 fidelity strength (lambda) (default: 0.8) |
| `--l1qsm-max-iter <L1QSM_MAX_ITER>` | L1-QSM: Number of outer ADMM iterations (default: 50) |
| `--l1qsm-tol-update <L1QSM_TOL_UPDATE>` | L1-QSM: Percent-update stopping tolerance (default: 1) |
| `--l1qsm-tol-delta <L1QSM_TOL_DELTA>` | L1-QSM: Inner Newton convergence tolerance (default: 1e-6) |
| `--l1qsm-te <TE>` | L1-QSM: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |
| `--whqsm-alpha1 <WHQSM_ALPHA1>` | WH-QSM TV regularization weight (alpha1) (default: 0.0002) |
| `--whqsm-mu1 <WHQSM_MU1>` | WH-QSM ADMM penalty for TV splitting (mu1) (default: 0.02) |
| `--whqsm-mu2 <WHQSM_MU2>` | WH-QSM ADMM penalty for data-fidelity splitting (mu2) (default: 1) |
| `--whqsm-beta <WHQSM_BETA>` | WH-QSM weak-harmonic ROI penalty (beta) (default: 150) |
| `--whqsm-muh <WHQSM_MUH>` | WH-QSM ADMM penalty for harmonic-field splitting (muh) (default: 5) |
| `--whqsm-max-iter <WHQSM_MAX_ITER>` | WH-QSM: Maximum outer iterations (default: 150) |
| `--whqsm-tol-update <WHQSM_TOL_UPDATE>` | WH-QSM: Percent-update stopping tolerance (default: 0.1) |
| `--whqsm-tol-delta <WHQSM_TOL_DELTA>` | WH-QSM: Inner Newton stopping tolerance (default: 1e-6) |
| `--whqsm-weight <WEIGHT>` | WH-QSM data weight: flat (1 over the whole volume, WH_nlTV.m's default), flat-in-mask (1 inside the mask only), max-normalised (the magnitude over its maximum) or in-vivo (min(2 × magnitude / maximum, 1)); the last two need magnitude images (default: flat) |
| `--whqsm-te <TE>` | WH-QSM: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |
| `--hdqsm-alpha-l2 <HDQSM_ALPHA_L2>` | HD-QSM L2-stage TV weight (alpha_l2) (default: 3e-5) |
| `--hdqsm-mu1-l2 <MU1_L2>` | HD-QSM L2-stage gradient-consistency ADMM weight (mu1_l2), or auto for 100 × alpha_l2 (default: auto) |
| `--hdqsm-mu2 <HDQSM_MU2>` | HD-QSM fidelity consistency weight (mu2) (default: 1) |
| `--hdqsm-max-iter-l1 <HDQSM_MAX_ITER_L1>` | HD-QSM stage-1 (L1) iterations (max_iter_l1) (default: 20) |
| `--hdqsm-max-iter-l2 <HDQSM_MAX_ITER_L2>` | HD-QSM stage-2 (L2) iterations (max_iter_l2) (default: 80) |
| `--hdqsm-tol-update <HDQSM_TOL_UPDATE>` | HD-QSM stage-2 percent-update convergence tolerance (default: 0.01) |
| `--amp-pe-wave-order <AMP_PE_WAVE_ORDER>` | AMP-PE: Daubechies wavelet order (1=db1, 2=db2) (default: 1) |
| `--amp-pe-nlevel <AMP_PE_NLEVEL>` | AMP-PE: Wavelet decomposition levels (default: 3) |
| `--amp-pe-wave-pec <AMP_PE_WAVE_PEC>` | AMP-PE: Morphology-mask energy retention fraction (0.0-1.0) (default: 0.85) |
| `--amp-pe-simulated-te <AMP_PE_SIMULATED_TE>` | AMP-PE simulated echo time (s) used to turn the field into phase (default: 0.008) |
| `--amp-pe-max-linearization-ite <AMP_PE_MAX_LINEARIZATION_ITE>` | AMP-PE: Linearisation iterations per stage (default: 25) |
| `--amp-pe-tikhonov-beta <AMP_PE_TIKHONOV_BETA>` | AMP-PE: L2-seed Tikhonov weight (default: 0.02) |
| `--amp-pe-damp-rate-sig <AMP_PE_DAMP_RATE_SIG>` | AMP-PE: GAMP signal-update damping rate (default: 0.01) |
| `--amp-pe-damp-rate-par <AMP_PE_DAMP_RATE_PAR>` | AMP-PE: Parameter-estimation learning rate (kappa) (default: 0.1) |
| `--amp-pe-max-pe-spar-ite <AMP_PE_MAX_PE_SPAR_ITE>` | AMP-PE: Inner sparse-reconstruction iterations (default: 5) |
| `--amp-pe-max-pe-est-ite <AMP_PE_MAX_PE_EST_ITE>` | AMP-PE: Inner parameter-estimation iterations (default: 5) |
| `--amp-pe-cvg-thd <AMP_PE_CVG_THD>` | AMP-PE: GAMP inner convergence threshold (default: 1e-6) |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--multi-orientation-algorithm <MULTI_ORIENTATION_ALGORITHM>` | Which reconstruction to run over each orientation group (default: cosmos) Values: `cosmos`, `sti`. |
| `--multi-orientation-lambda <MULTI_ORIENTATION_LAMBDA>` | Regularization for the multi-orientation reconstruction (COSMOS k-space L2, or STI ridge) (default: 0) |
| `--orientation-reference <PATTERN>` | The orientation each group is registered onto, and whose grid and segmentation the group map uses: a glob over the run key (`*acq-Neutral*`) or a word in it. Must match one run per group. Default: the first run in name order. |
| `--cosmos-magnitude-weighting` | COSMOS: weight each orientation by its combined magnitude (conjugate-gradient solve) |
| `--cosmos-tol <COSMOS_TOL>` | Magnitude-weighted COSMOS: conjugate-gradient tolerance (needs --cosmos-magnitude-weighting) (default: 1e-6) |
| `--cosmos-max-iter <COSMOS_MAX_ITER>` | Magnitude-weighted COSMOS: conjugate-gradient iteration cap (needs --cosmos-magnitude-weighting) (default: 100) |
| `--no-orientation-registration` | Do not co-register the orientations in a group — use this when they were already registered externally and a second interpolation is not wanted. A group that still needs registration is then refused instead of reconstructed. |
| `--multi-orientation-force` | Reconstruct an orientation group even when its B0 directions look degenerate. |
| `--qsm-reference <SPEC>` | What the susceptibility map's zero is pinned to: `mean` (the mean over the voxels the map is defined on — the brain mask less the rim background-field removal eroded; the default), `none`, or a SynthSeg region — a structure with both sides merged (`thalamus`), one side (`left-thalamus`), the `ventricles` composite, a raw FreeSurfer id (`4`), or a comma-separated list of any of those. A region requires --do-segmentation (turned on automatically, and logged). An unrecognised name lists the valid ones. |
| `--do-chisep` | Compute chi-separation (paramagnetic/diamagnetic susceptibility maps) |
| `--chisep <CHI_SEPARATION_ALGORITHM>` | Chi-separation method (default: r2star-qsm) Values: `r2star-qsm`, `decompose`, `chi-sep-ilsqr`, `chi-sep-medi`, `wavesep`, `hc-chisep`, `susep-net`, `chi-sepnet`. |
| `--r2star-qsm-r-const-3t <R2STAR_QSM_R_CONST_3T>` | R2\*-QSM relaxometric constant at 3T (Hz/ppm) (default: 274) |
| `--decompose-n-inner <DECOMPOSE_N_INNER>` | DECOMPOSE inner alternating passes per voxel (default: 10) |
| `--decompose-chi-bound <BOUND>` | DECOMPOSE bound on \|χ\| in the fit: a number in ppm (default: 0.5), or echo-spacing to compute it from the field strength and echo spacing. |
| `--decompose-unwrap-phase <BOOL>` | DECOMPOSE: unwrap the phase along the echoes (default: false, as in the authors' code) |
| `--decompose-fit <DECOMPOSE_FIT>` | DECOMPOSE's fit: `reference-code` (Tim Ho's script; default) or `paper` (Chen et al. 2021: amplitudes summing to one, per-voxel S(0) = 1 scale), as the DECOMPOSE pipeline runs it. Values: `reference-code`, `paper`. |
| `--decompose-max-iterations <DECOMPOSE_MAX_ITERATIONS>` | DECOMPOSE: maximum solver iterations of each fit (lsqcurvefit MaxIterations; default: 400) |
| `--decompose-max-function-evaluations <DECOMPOSE_MAX_FUNCTION_EVALUATIONS>` | DECOMPOSE: maximum function evaluations of each fit (lsqcurvefit MaxFunctionEvaluations; default: 100 × the fit's parameters) |
| `--decompose-function-tolerance <DECOMPOSE_FUNCTION_TOLERANCE>` | DECOMPOSE: relative cost change each fit stops at (lsqcurvefit FunctionTolerance; default: 1e-6) |
| `--decompose-step-tolerance <DECOMPOSE_STEP_TOLERANCE>` | DECOMPOSE: relative step each fit stops at (lsqcurvefit StepTolerance; default: 1e-6) |
| `--decompose-optimality-tolerance <DECOMPOSE_OPTIMALITY_TOLERANCE>` | DECOMPOSE: first-order optimality each fit stops at (lsqcurvefit OptimalityTolerance; default: 1e-6) |
| `--decompose-finite-difference-step <DECOMPOSE_FINITE_DIFFERENCE_STEP>` | DECOMPOSE: relative finite-difference step of the Jacobian (lsqcurvefit FiniteDifferenceStepSize; default: √eps) |
| `--decompose-typical-x <DECOMPOSE_TYPICAL_X>` | DECOMPOSE: typical magnitude of each fitted value (lsqcurvefit TypicalX; default: 1) |
| `--chi-sep-ilsqr-dr-pos <HZ_PER_PPM>` | χ-sep iLSQR paramagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-ilsqr-dr-neg <HZ_PER_PPM>` | χ-sep iLSQR diamagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-ilsqr-pad <X> <Y> <Z>` | χ-sep iLSQR zero-padding at both ends of each axis in voxels, X Y Z (default: 16 16 16) |
| `--chi-sep-ilsqr-cg-max-iter-lsqr <CHI_SEP_ILSQR_CG_MAX_ITER_LSQR>` | χ-sep iLSQR: maximum CG iterations of the LSQR step (default: 50) |
| `--chi-sep-ilsqr-cg-tol-lsqr <CHI_SEP_ILSQR_CG_TOL_LSQR>` | χ-sep iLSQR: CG relative tolerance of the LSQR step (default: 0.02) |
| `--chi-sep-ilsqr-cg-max-iter-sa <CHI_SEP_ILSQR_CG_MAX_ITER_SA>` | χ-sep iLSQR: maximum CG iterations of the streaking-removal step (default: 50) |
| `--chi-sep-ilsqr-cg-tol-sa <CHI_SEP_ILSQR_CG_TOL_SA>` | χ-sep iLSQR: CG relative tolerance of the streaking-removal step (default: 0.02) |
| `--chi-sep-ilsqr-d2-thr <CHI_SEP_ILSQR_D2_THR>` | χ-sep iLSQR: the streaking cone is where \|D\| is below this (default: 0.11) |
| `--chi-sep-ilsqr-prc-w-i-min <CHI_SEP_ILSQR_PRC_W_I_MIN>` | χ-sep iLSQR: percentile of the field's Laplacian at which the LSQR weight is 1 (default: 60) |
| `--chi-sep-ilsqr-prc-w-i-max <CHI_SEP_ILSQR_PRC_W_I_MAX>` | χ-sep iLSQR: percentile of the field's Laplacian at which the LSQR weight is 0 (default: 99.8) |
| `--chi-sep-ilsqr-prc-w-g-min <CHI_SEP_ILSQR_PRC_W_G_MIN>` | χ-sep iLSQR: percentile of each gradient's magnitude at which the streaking-removal weight is 1 (default: 50) |
| `--chi-sep-ilsqr-prc-w-g-max <CHI_SEP_ILSQR_PRC_W_G_MAX>` | χ-sep iLSQR: percentile of each gradient's magnitude at which the streaking-removal weight is 0 (default: 70) |
| `--chi-sep-ilsqr-qsm-max-iter <CHI_SEP_ILSQR_QSM_MAX_ITER>` | χ-sep iLSQR: max iterations of the iLSQR that reconstructs, from the local field, the QSM χ-separation is initialised with (default: 100; not used with --use-custom-qsm) |
| `--chi-sep-ilsqr-qsm-tol <CHI_SEP_ILSQR_QSM_TOL>` | χ-sep iLSQR: tolerance of that QSM's iLSQR (default: 0.001) |
| `--chi-sep-ilsqr-qsm-pad-mm <MM>` | χ-sep iLSQR: zero-padding of that QSM's iLSQR, in mm per side (default: 12) |
| `--chi-sep-ilsqr-qsm-precision <PRECISION>` | χ-sep iLSQR: arithmetic of that QSM's two LSQR solves: double, or single as the original implementation (default: double) |
| `--chi-sep-medi-lambda <CHI_SEP_MEDI_LAMBDA>` | χ-sep MEDI weight of the data terms against the L1 terms (default: 1) |
| `--chi-sep-medi-lambda-csf <CHI_SEP_MEDI_LAMBDA_CSF>` | χ-sep MEDI weight of the CSF-uniformity term (default: 1) |
| `--chi-sep-medi-dr-pos <HZ_PER_PPM>` | χ-sep MEDI paramagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-medi-dr-neg <HZ_PER_PPM>` | χ-sep MEDI diamagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-medi-pad <CHI_SEP_MEDI_PAD>` | χ-sep MEDI zero-padding at both ends of the third axis, in voxels (default: 5) |
| `--chi-sep-medi-tol <CHI_SEP_MEDI_TOL>` | χ-sep MEDI: stop when the relative change falls below this (default: 0.03) |
| `--chi-sep-medi-max-iter <CHI_SEP_MEDI_MAX_ITER>` | χ-sep MEDI maximum Gauss-Newton iterations (default: 30) |
| `--chi-sep-medi-cg-tol <CHI_SEP_MEDI_CG_TOL>` | χ-sep MEDI inner CG tolerance (default: 0.05) |
| `--chi-sep-medi-cg-max-iter <CHI_SEP_MEDI_CG_MAX_ITER>` | χ-sep MEDI inner CG max iterations (default: 100) |
| `--chi-sep-medi-qsm-max-iter <CHI_SEP_MEDI_QSM_MAX_ITER>` | χ-sep MEDI: max iterations of the iLSQR that reconstructs, from the local field, the QSM χ-separation is initialised with (default: 100; not used with --use-custom-qsm) |
| `--chi-sep-medi-qsm-tol <CHI_SEP_MEDI_QSM_TOL>` | χ-sep MEDI: tolerance of that QSM's iLSQR (default: 0.001) |
| `--chi-sep-medi-qsm-pad-mm <MM>` | χ-sep MEDI: zero-padding of that QSM's iLSQR, in mm per side (default: 12) |
| `--chi-sep-medi-qsm-precision <PRECISION>` | χ-sep MEDI: arithmetic of that QSM's two LSQR solves: double, or single as the original implementation (default: double) |
| `--chi-sep-medi-csf-r2star <METHOD>` | χ-sep MEDI: the R2\* its CSF mask is extracted from: arlo (MEDI's ARLO; the default) or nnls (the χ-separation toolbox's r2star_nnls, which its script runs) |
| `--wavesep-dr-pos <WAVESEP_DR_POS>` | WaveSep paramagnetic relaxivity (Hz/ppm) (default: 137) |
| `--wavesep-dr-neg <WAVESEP_DR_NEG>` | WaveSep diamagnetic relaxivity (Hz/ppm) (default: 137) |
| `--wavesep-alpha <WAVESEP_ALPHA>` | WaveSep: Proximal-gradient step size (default: 0.2) |
| `--wavesep-lambda <WAVESEP_LAMBDA>` | WaveSep: Wavelet L1 soft-threshold weight (default: 0.02) |
| `--wavesep-wavelet-order <WAVESEP_WAVELET_ORDER>` | WaveSep: Daubechies order (default: 4) |
| `--wavesep-max-iter <WAVESEP_MAX_ITER>` | WaveSep: ISTA max iterations (default: 100) |
| `--wavesep-tol <WAVESEP_TOL>` | WaveSep: Relative-change stop tolerance (default: 0.001) |
| `--hc-chisep-dr-pos-3t <HC_CHISEP_DR_POS_3T>` | HC-ChiSep paramagnetic relaxivity at 3T (Hz/ppm) (default: 137) |
| `--hc-chisep-bin-hz <HC_CHISEP_BIN_HZ>` | HC-ChiSep R2' bin width for the anchored grid search (Hz) (default: 0.25) |
| `--chi-sepnet-route <CHI_SEPNET_ROUTE>` | χ-sepnet's input route. `toolbox` (default), the authors': the inputs resampled to 1 mm with B0 along z, the susceptibility channel computed by QSMnetME, R2' measured or predicted from R2\* by R2PRIMEnet, the maps resampled back. `native`: the network alone on the acquired grid, given this run's QSM and R2'. Values: `toolbox`, `native`. |
| `--chi-sepnet-patch <D> <H> <W>` | χ-sepnet (`--chi-sepnet-route native`) sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--chi-sepnet-frame-patch <D> <H> <W>` | χ-sepnet (`--chi-sepnet-route toolbox`): run the networks as a sliding window of patches this size over the 1 mm frame, D H W, each a multiple of 16, so memory follows the patch. Omit to run the whole frame at once, as the authors do. |
| `--susep-net-patch <D> <H> <W>` | Run SUSEP-Net as a sliding window of patches this size in voxels, D H W, each a multiple of 8, so its memory follows the patch, not the volume. Omit to run the whole volume at once. |
| `--use-custom-qsm <TOOL>` | Use a bring-your-own QSM (Chimap) from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--no-magnitude` | Do not publish the combined magnitude (part-mag_T2starw). It is still computed, and kept with the run's intermediates, when a step reads it (SMWI, segmentation, a magnitude-weighted inversion) |
| `--do-swi` | Also compute SWI. |
| `--swi-unwrapping <SWI_UNWRAPPING>` | SWI phase unwrapping before the echoes are combined [default: laplacian]. Values: `laplacian`, `laplacian-slice`, `romeo`. |
| `--swi-scaling <SWI_SCALING>` | SWI phase-mask scaling (default: tanh) Values: `tanh`, `negative-tanh`, `positive`, `negative`, `triangular`. |
| `--swi-strength <SWI_STRENGTH>` | SWI: Phase scaling strength (higher = stronger phase contrast) (default: 4) |
| `--swi-hp-sigma <SWI_HP_SIGMA>` | SWI high-pass filter sigma (3 values, in voxels) (default: 4 4 0) |
| `--swi-mip-window <SWI_MIP_WINDOW>` | SWI: Minimum-intensity-projection window, in slices (default: 7) |
| `--swi-filtered-phase` | Also write CLEAR-SWI's high-pass filtered phase, the phase the SWI is weighted by (`_desc-swi_phase.nii`): Hz for several echoes, radians for one, 0 outside CLEAR-SWI's head mask. Implies --do-swi. |
| `--do-smwi` | Also compute SMWI (susceptibility map-weighted imaging): weights the magnitude by a mask built from the susceptibility map rather than from high-pass filtered phase, so the contrast stays where the source is. Needs χ, so it requires QSM (turned on automatically, and logged). doi:10.1002/mrm.24920. |
| `--smwi-threshold <SMWI_THRESHOLD>` | SMWI susceptibility threshold in ppm — the \|χ\| at which the weighting mask reaches zero. The 1 ppm default follows SEPIA and targets strong sources; brain tissue wants far less. |
| `--smwi-power <SMWI_POWER>` | SMWI mask power (contrast strength) (default: 4) |
| `--smwi-mip-window <SMWI_MIP_WINDOW>` | SMWI: Minimum-intensity-projection window, in slices (default: 4) |
| `--do-t2starmap` | Compute T2\* relaxation map from multi-echo magnitude data. |
| `--do-r2starmap` | Compute R2\* decay rate map from multi-echo magnitude data. |
| `--do-r2map` | Compute R2 map from a multi-echo spin-echo (MESE) acquisition (EPG) |
| `--use-custom-r2 <TOOL>` | Use a bring-your-own R2 map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--do-r2primemap` | Compute R2' map (= R2\* − R2; needs GRE magnitude + a MESE acquisition) |
| `--use-custom-r2prime <TOOL>` | Use a bring-your-own R2' map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--r2prime-strategy <R2PRIME_STRATEGY>` | How to obtain R2' when no custom map is supplied. `auto` (default) measures it as R2\* - R2 from a MESE acquisition when one is present and predicts it from R2\* with R2PRIMEnet when it is not; `mese` only ever measures it, producing nothing without spin-echo data; `r2primenet` always predicts it. A measurement and an estimate are not the same thing — the generated methods text says which route was configured. Values: `auto`, `mese`, `r2primenet`. |
| `--r2primenet-ignore-field-mismatch` | Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its weights were trained at; the predicted R2' is then uncalibrated. Without it, an explicit `--r2prime-strategy r2primenet` refuses a non-3 T run and `auto` skips R2' with a warning. |
| `--r2primenet-patch <D> <H> <W>` | R2PRIMEnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--do-segmentation` | Also segment the brain from the GRE magnitude with SynthSeg, writing a FreeSurfer-labelled `_dseg.nii` and its BIDS lookup table. Needs no T1w and no registration. Needs a deep-learning build. doi:10.1016/j.media.2023.102789. |
| `--use-custom-dseg <TOOL>` | Prefer a bring-your-own segmentation from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ when present. Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg. |
| `--synthseg-version <VERSION>` | SynthSeg weights generation: v1 (SynthSeg 1.0, 32 labels; the default) or v2 (SynthSeg 2.0, 33 labels, adds a general CSF class; Billot et al., PNAS 2023) |
| `--synthseg-crop <SYNTHSEG_CROP>` | Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32). Omit to process the whole volume, as the SynthSeg CLI does. |
| `--synthseg-patch <VOXELS>` | Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the volume. Omit to run the whole volume at once, as the SynthSeg CLI does. |
| `--no-synthseg-flip-averaging` | Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does. |
| `--no-synthseg-topology-cleanup` | Skip resetting each topological class to its largest connected component. |
| `--synthseg-sigma <SYNTHSEG_SIGMA>` | Gaussian blur on the posteriors, in voxels (0 disables it) (default: 0.5) |
| `--do-analysis` | Also summarise every quantitative map (χ, χ+/χ−, T2\*, R2\*, R2, R2') per segmented structure into a TSV: mean, SD, median, min, max and the 5th/95th percentiles, one row per map and structure, plus a dataset-level table gathering all runs. Summarises whatever the run made — it requires --do-segmentation, and QSM only if no other map was asked for (each turned on automatically, and logged) |
| `--export-dicom` | Also export final maps as DICOM series into each subject's extra_files/ folder. |
| `--dismiss-recommendation <RULE=VALUE>` | Stop offering a recommendation for the setting's current value: RULE=VALUE, the rule's id and the value it was dismissed for, as `qsmxt run --dry` lists them (e.g. field-mapping.phase-offsets=false). Repeatable. A change to the setting brings the recommendation back. A required one cannot be dismissed, and a reference-pipeline difference has nothing to dismiss. |
| `--n-procs <N_PROCS>` | Number of parallel threads. |
| `--source-dicom <SOURCE_DICOM>` | Optional source DICOM directory; inherit patient/study identity from the original DICOMs when exporting (used with --export-dicom) |
| `--dicom-outputs <DICOM_OUTPUTS>` | Restrict DICOM export to these maps (default: all produced); repeat the flag or comma-separate them (used with --export-dicom) Values: `magnitude`, `chimap`, `swi`, `minip`, `desc-swi_phase`, `desc-paramagnetic_smwi`, `desc-diamagnetic_smwi`, `desc-paramagneticsmwi_minip`, `desc-diamagneticsmwi_minip`, `t2starmap`, `r2starmap`, `r2map`, `r2primemap`, `desc-paramagnetic_chimap`, `desc-diamagnetic_chimap`, `desc-total_chimap`, `dseg`. |
| `--dry` | Print the processing plan, the pipeline graph and the methods text without running anything or writing any file. Also lists what the configuration departs from in the chosen methods' reference pipelines, for every run's scan, and each covered setting's reference value; with --accept-recommendations or --accept-reference-settings, the settings that would change. |
| `--accept-recommendations` | Apply the recommended value of every reasoned recommendation that is not dismissed (required, recommended and discouraged findings, each with its reason) before running. The original authors' settings are not applied; see --accept-reference-settings. Two equally strong recommendations that disagree on a setting are left for you to choose. Without it, no setting is ever changed. |
| `--accept-reference-settings` | Apply the original authors' settings of the chosen methods (what their code, paper or README does, where no reason is given that it is better) before running, so the run follows the methods' reference pipelines. Two methods whose authors' settings disagree on a setting are left for you to choose; a reasoned recommendation on the same setting wins when --accept-recommendations is also given. |
| `--force` | Force re-run, ignoring cached pipeline state. |
| `--debug` | Enable debug logging. |
| `--clean-intermediates` | Once every run (and any multi-orientation combination) is done, delete the intermediate images under workflow/. Published outputs, per-step provenance and the cache state are kept: a re-run with the same configuration recomputes nothing, one with a changed configuration recomputes the intermediates it needs. |
| `--mem-limit-gb <MEM_LIMIT_GB>` | Memory limit in GB for concurrent run scheduling (if not specified: the memory available, within a Slurm job's or container's memory limit when there is one) |
| `--no-mem-limit` | Disable memory-aware concurrency limiting. |
| `--no-dataset-steps` | Process each run and stop: skip the dataset-level steps (multi-orientation reconstruction, summary tables, cohort figures, provenance, dataset_description.json, pipeline_config.toml, methods.md, the pipeline graph, changes/) and log to logs/&lt;run&gt;.log instead of qsmxt.log. `qsmxt aggregate` does those steps once every run is done; `qsmxt slurm` uses the pair. |

<!-- generated:cli-run end -->

The steps are in [Run QSMxT on your data](/QSMxT/how-to/run-qsm/). `qsmxt run`, `slurm`, `aggregate` and `validate` exit with status 1 when the BIDS directory does not exist or has no runs to process (none at all, or none that match `--include`, `--exclude` and `--num-echoes`).

## `qsmxt init`

<!-- generated:cli-init start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Generate a pipeline configuration file

```sh
qsmxt init [OPTIONS]
```

| Argument | Description |
| --- | --- |
| `-o, --output <OUTPUT>` | Output file path (prints to stdout if not specified) |

<!-- generated:cli-init end -->

## `qsmxt validate`

<!-- generated:cli-validate start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Validate BIDS dataset structure for QSM processing

```sh
qsmxt validate [OPTIONS] <BIDS_DIR>
```

| Argument | Description |
| --- | --- |
| `<BIDS_DIR>` | Input BIDS directory. Required. |
| `--include <INCLUDE>` | Include only runs matching these glob patterns (e.g. "sub-1\*" "\*ses-pre\*") |
| `--exclude <EXCLUDE>` | Exclude runs matching these glob patterns (e.g. "\*mygrea\*") |

<!-- generated:cli-validate end -->

## `qsmxt dicom-convert`

<!-- generated:cli-dicom-convert start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Convert a DICOM directory to BIDS using automatic series classification

```sh
qsmxt dicom-convert [OPTIONS] <DICOM_DIR> <OUTPUT_DIR>
```

| Argument | Description |
| --- | --- |
| `<DICOM_DIR>` | Input DICOM directory (searched recursively) Required. |
| `<OUTPUT_DIR>` | Output BIDS directory. Required. |
| `--dry-run` | Print the auto-detected series classification and exit without converting. |
| `--keep-dicom-uids` | Keep the study's DICOM UIDs (Study, Series and Frame of Reference) and dates in the sidecars, so `qsmxt run --export-dicom` files its series into the original study. Patient identity is still removed, but the UIDs link the dataset to the original study: don't share such a dataset outside your site. |

<!-- generated:cli-dicom-convert end -->

The steps are in [Convert DICOM to BIDS](/QSMxT/how-to/convert-dicom/); the names it writes are in [Input data layout](/QSMxT/reference/inputs/#what-qsmxt-dicom-convert-writes).

## `qsmxt slurm`

<!-- generated:cli-slurm start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Generate SLURM job scripts for HPC execution: one job per run, then an aggregation job

```sh
qsmxt slurm [OPTIONS] --account <ACCOUNT> <BIDS_DIR> [OUTPUT_DIR]
```

It also takes every [`qsmxt run`](#qsmxt-run) option except `--dry`, `--mem-limit-gb`, `--n-procs`, `--no-dataset-steps`, `--no-mem-limit`. Those are listed there; below are its own.

| Argument | Description |
| --- | --- |
| `<BIDS_DIR>` | Input BIDS directory. Required. |
| `<OUTPUT_DIR>` | Output directory (defaults to bids_dir; outputs go into &lt;dir&gt;/derivatives/qsmxt/) |
| `--account <ACCOUNT>` | SLURM account name. Required. |
| `--partition <PARTITION>` | SLURM partition. |
| `--time <TIME>` | Wall time limit (e.g., 02:00:00) Default: `02:00:00`. |
| `--mem <MEM>` | Memory per job in GB [default: estimated per run from its size and the pipeline]. |
| `--cpus-per-task <CPUS_PER_TASK>` | CPUs per task. Default: `4`. |
| `--submit` | Auto-submit scripts via sbatch. |

<!-- generated:cli-slurm end -->

The steps, and what the jobs do, are in [Run on a cluster](/QSMxT/how-to/slurm/).

## `qsmxt aggregate`

<!-- generated:cli-aggregate start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Run the dataset-level steps over runs already processed with `qsmxt run --no-dataset-steps` (multi-orientation, summary tables, provenance, clean-up)

```sh
qsmxt aggregate [OPTIONS] <BIDS_DIR> [OUTPUT_DIR]
```

It also takes every [`qsmxt run`](#qsmxt-run) option except `--accept-recommendations`, `--accept-reference-settings`, `--dicom-outputs`, `--dry`, `--mem-limit-gb`, `--n-procs`, `--no-dataset-steps`, `--no-mem-limit`, `--source-dicom`. Those are listed there; below are its own.

| Argument | Description |
| --- | --- |
| `<BIDS_DIR>` | Input BIDS directory. Required. |
| `<OUTPUT_DIR>` | Output directory (defaults to bids_dir; outputs go into &lt;dir&gt;/derivatives/qsmxt/) |

<!-- generated:cli-aggregate end -->

What it does is in [Run on a cluster](/QSMxT/how-to/slurm/#what-the-jobs-do).

## `qsmxt mask`

<!-- generated:cli-mask start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Masking operations

| Subcommand | Description |
| --- | --- |
| [`otsu`](#qsmxt-mask-otsu) | Otsu automatic thresholding |
| [`value`](#qsmxt-mask-value) | Fixed value thresholding |
| [`percentile`](#qsmxt-mask-percentile) | Percentile thresholding |
| [`sti-gui`](#qsmxt-mask-sti-gui) | STI Suite QSM_GUI's thresholding: the level its thresh_tool starts at (Otsu's threshold of the middle slice, scaled to that slice's maximum) |
| [`bet`](#qsmxt-mask-bet) | Brain extraction (BET) |
| [`hd-bet`](#qsmxt-mask-hd-bet) | Deep-learning brain extraction (HD-BET; downloads weights on first use) |
| [`rs2-net`](#qsmxt-mask-rs2-net) | Deep-learning rodent brain extraction (RS2-Net; downloads weights on first use) |
| [`mindgrab`](#qsmxt-mask-mindgrab) | Deep-learning brain extraction for any contrast (MindGrab; downloads weights on first use) |
| [`preset`](#qsmxt-mask-preset) | Run a `--mask-preset` recipe on an image (robust-threshold, bet, hd-bet, bet-and-phase, rs2-net, mindgrab) |
| [`robust`](#qsmxt-mask-robust) | Alias for `preset robust-threshold` |
| [`erode`](#qsmxt-mask-erode) | Erode a binary mask |
| [`dilate`](#qsmxt-mask-dilate) | Dilate a binary mask |
| [`close`](#qsmxt-mask-close) | Morphological closing on a binary mask |
| [`majority`](#qsmxt-mask-majority) | 3x3x3 majority filter on a binary mask (MATLAB's bwmorph3 'majority') |
| [`largest-component`](#qsmxt-mask-largest-component) | Keep only the largest 26-connected component of a binary mask (MATLAB's bwlabeln) |
| [`imfill`](#qsmxt-mask-imfill) | Fill every hole of a binary mask, whatever its size (MATLAB's imfill 'holes') |
| [`fill-holes`](#qsmxt-mask-fill-holes) | Fill holes in a binary mask |
| [`smooth`](#qsmxt-mask-smooth) | Gaussian smooth a binary mask (re-thresholds at 0.5) |
| [`and`](#qsmxt-mask-and) | Intersect binary masks: keep voxels every input keeps (the `run --mask-combine and`) |
| [`or`](#qsmxt-mask-or) | Union binary masks: keep voxels any input keeps (the `run --mask-combine or`) |

<!-- generated:cli-mask end -->

Every subcommand is the first link of a chain: it does its own operation, then
applies any `--op` refinements in order — `erode:2`, `fill-holes:0`, `close:1`,
`gaussian:4.0`, `majority`, `min-intensity:100`, `signal-erode`, the same spellings as a `--mask` section. So
`qsmxt mask erode m.nii -o out.nii --op fill-holes` erodes, then fills. What each
op does is defined once, in the same code the pipeline runs, so a mask built
here matches the one a `--mask` section would produce.

`and` and `or` are the standalone form of the pipeline's
[`--mask-combine`](/QSMxT/reference/masking/#combining-sections), for trying
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

The subcommands that make a mask from an image (`otsu` to `robust`) take a 3D
image, or a 4D multi-echo magnitude, whose echoes they combine by
root-sum-of-squares first, as `qsmxt run` does for a `magnitude` mask section. The
subcommands that take masks refuse a 4D file.

`preset` reads its input image for every section; sections that read phase
quality use `--quality` when given, and the input image otherwise, as
`run --masking-input` would; `--bet-orientation` sets its BET steps' orientation.
`--op signal-erode` needs a magnitude image, and `--op min-intensity` the image
whose intensity it reads: for `and`/`or`, whose inputs are masks, pass it as
`--magnitude`.

<!-- generated:cli-mask-subcommands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

### `qsmxt mask otsu`

Otsu automatic thresholding

```sh
qsmxt mask otsu [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask value`

Fixed value thresholding

```sh
qsmxt mask value [OPTIONS] --output <OUTPUT> --threshold <THRESHOLD> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--threshold <THRESHOLD>` | Threshold value. Required. |

### `qsmxt mask percentile`

Percentile thresholding

```sh
qsmxt mask percentile [OPTIONS] --output <OUTPUT> --percentile <PERCENTILE> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--percentile <PERCENTILE>` | Percentile value (0-100) Required. |

### `qsmxt mask sti-gui`

STI Suite QSM_GUI's thresholding: the level its thresh_tool starts at (Otsu's threshold of the middle slice, scaled to that slice's maximum)

```sh
qsmxt mask sti-gui [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask bet`

Brain extraction (BET)

```sh
qsmxt mask bet [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--fractional-intensity <FRACTIONAL_INTENSITY>` | Fractional intensity (0.0-1.0, smaller = larger brain) Default: `0.5`. |
| `--voxel-scale <FACTOR>` | Factor applied to the voxel sizes before BET runs — 10 for mouse data, 1 for the acquisition's own geometry (see --bet-voxel-scale). Prefer `mask rs2-net`, which masks rodent brains without the trick. |
| `--bet-smoothness <BET_SMOOTHNESS>` | BET surface smoothness factor. Applies to every BET step in the masking recipe (default: 1) |
| `--bet-gradient-threshold <BET_GRADIENT_THRESHOLD>` | BET gradient threshold (-1 to 1). Applies to every BET step in the masking recipe (default: 0) |
| `--bet-iterations <BET_ITERATIONS>` | BET surface evolution iterations. Applies to every BET step in the masking recipe (default: 1000) |
| `--bet-subdivisions <BET_SUBDIVISIONS>` | BET icosphere subdivision level. Applies to every BET step in the masking recipe (default: 4) |
| `--bet-orientation <ORIENTATION>` | Which way round BET sees the left-right axis: `fsl` (the default; reversed for a neurologically stored image, as FSL's bet reads a NIfTI file) or `as-stored` (the voxels in the order they are stored, as MEDI's BET.m runs bet2). Applies to every BET step in the masking recipe. |

### `qsmxt mask hd-bet`

Deep-learning brain extraction (HD-BET; downloads weights on first use)

```sh
qsmxt mask hd-bet [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--window` | Run in 192x192x96 sliding-window patches (about 3.6–4.5 GB measured) instead of on the whole volume (the default; about 6–8 GB measured, and about twice as fast) |
| `--low-memory` | Use 128x128x64 sliding-window patches (about 1.5–1.9 GB measured) |
| `--patch <PATCH>` | Sliding-window patch size XxYxZ in voxels at 1 mm (multiples of 32x32x16) |
| `--tta` | 8-fold mirroring test-time augmentation (about 8x slower) |
| `--tile-step <TILE_STEP>` | Sliding-window step as a fraction of the patch, in (0, 1]. Larger steps mean fewer patches and a shorter run: 0.5 (default) is HD-BET's own, 1.0 abuts the patches. Implies --window. |

### `qsmxt mask rs2-net`

Deep-learning rodent brain extraction (RS2-Net; downloads weights on first use)

```sh
qsmxt mask rs2-net [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--tta` | 8-fold mirroring test-time augmentation (about 8x slower) |
| `--tile-step <TILE_STEP>` | Sliding-window step as a fraction of the patch, in (0, 1]. Larger steps mean fewer patches and a shorter run: 0.5 (default) is nnU-Net's own, 1.0 abuts the patches. |

### `qsmxt mask mindgrab`

Deep-learning brain extraction for any contrast (MindGrab; downloads weights on first use)

```sh
qsmxt mask mindgrab [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--tta` | Left-right flip test-time augmentation (about 2x slower) |

### `qsmxt mask preset`

Run a `--mask-preset` recipe on an image (robust-threshold, bet, hd-bet, bet-and-phase, rs2-net, mindgrab)

```sh
qsmxt mask preset [OPTIONS] --output <OUTPUT> <PRESET> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<PRESET>` | Which recipe — the same names as `run --mask-preset`. Values: `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab`. Required. |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--quality <QUALITY>` | Phase-quality map (from `qsmxt quality-map`) for the sections that read phase quality. Without it those sections read the input image instead, as `run --masking-input` would. |
| `--phase <PHASE>` | Phase image (3-D, or 4-D multi-echo): the mask leaves out voxels where the scanner recorded no phase (a constant value in every echo), as `qsmxt run` does for every recipe. A phase-quality map rates that flat phase as reliable, so give this with `--quality`. |
| `--bet-smoothness <BET_SMOOTHNESS>` | BET surface smoothness factor. Applies to every BET step in the masking recipe (default: 1) |
| `--bet-gradient-threshold <BET_GRADIENT_THRESHOLD>` | BET gradient threshold (-1 to 1). Applies to every BET step in the masking recipe (default: 0) |
| `--bet-iterations <BET_ITERATIONS>` | BET surface evolution iterations. Applies to every BET step in the masking recipe (default: 1000) |
| `--bet-subdivisions <BET_SUBDIVISIONS>` | BET icosphere subdivision level. Applies to every BET step in the masking recipe (default: 4) |
| `--bet-orientation <ORIENTATION>` | Which way round BET sees the left-right axis: `fsl` (the default; reversed for a neurologically stored image, as FSL's bet reads a NIfTI file) or `as-stored` (the voxels in the order they are stored, as MEDI's BET.m runs bet2). Applies to every BET step in the masking recipe. |

### `qsmxt mask robust`

Alias for `preset robust-threshold`

```sh
qsmxt mask robust [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask erode`

Erode a binary mask

```sh
qsmxt mask erode [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--iterations <ITERATIONS>` | Number of erosion iterations. Default: `1`. |

### `qsmxt mask dilate`

Dilate a binary mask

```sh
qsmxt mask dilate [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--iterations <ITERATIONS>` | Number of dilation iterations. Default: `1`. |

### `qsmxt mask close`

Morphological closing on a binary mask

```sh
qsmxt mask close [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--radius <RADIUS>` | Closing radius. Default: `1`. |

### `qsmxt mask majority`

3x3x3 majority filter on a binary mask (MATLAB's bwmorph3 'majority')

```sh
qsmxt mask majority [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask largest-component`

Keep only the largest 26-connected component of a binary mask (MATLAB's bwlabeln)

```sh
qsmxt mask largest-component [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask imfill`

Fill every hole of a binary mask, whatever its size (MATLAB's imfill 'holes')

```sh
qsmxt mask imfill [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask fill-holes`

Fill holes in a binary mask

```sh
qsmxt mask fill-holes [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--max-size <MAX_SIZE>` | Maximum hole size in voxels (0 = automatic, 5% of the volume — what every preset uses) Default: `0`. |

### `qsmxt mask smooth`

Gaussian smooth a binary mask (re-thresholds at 0.5)

```sh
qsmxt mask smooth [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, majority (3x3x3 majority filter), largest-component, imfill (every hole), min-intensity:100 (remove voxels below 100 in the input image; needs an image input), signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--sigma <SIGMA>` | Gaussian sigma in mm. Default: `4`. |

### `qsmxt mask and`

Intersect binary masks: keep voxels every input keeps (the `run --mask-combine and`)

```sh
qsmxt mask and [OPTIONS] --output <OUTPUT> [INPUTS] [INPUTS]...
```

| Argument | Description |
| --- | --- |
| `<INPUTS>` | Input mask NIfTI files (two or more, on the same grid) |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Refinement operation applied to the combined mask (repeatable, applied in order) — the `run --mask-refine` of this command. Examples: fill-holes:0, erode:1, close:1. |
| `--magnitude <MAGNITUDE>` | Magnitude image, needed only by `--op signal-erode`. |

### `qsmxt mask or`

Union binary masks: keep voxels any input keeps (the `run --mask-combine or`)

```sh
qsmxt mask or [OPTIONS] --output <OUTPUT> [INPUTS] [INPUTS]...
```

| Argument | Description |
| --- | --- |
| `<INPUTS>` | Input mask NIfTI files (two or more, on the same grid) |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Refinement operation applied to the combined mask (repeatable, applied in order) — the `run --mask-refine` of this command. Examples: fill-holes:0, erode:1, close:1. |
| `--magnitude <MAGNITUDE>` | Magnitude image, needed only by `--op signal-erode`. |

<!-- generated:cli-mask-subcommands end -->

## `qsmxt combine`

<!-- generated:cli-combine start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Multi-coil phase combination (uncombined receive-coil channels -&gt; combined echoes)

| Subcommand | Description |
| --- | --- |
| [`mcpc3ds`](#qsmxt-combine-mcpc3ds) | MCPC-3D-S (ASPIRE) coil combination from per-coil multi-echo phase + magnitude |

<!-- generated:cli-combine end -->

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

<!-- generated:cli-combine-subcommands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

### `qsmxt combine mcpc3ds`

MCPC-3D-S (ASPIRE) coil combination from per-coil multi-echo phase + magnitude

```sh
qsmxt combine mcpc3ds [OPTIONS] --phase <PHASE>... --magnitude <MAGNITUDE>... --output <OUTPUT>
```

| Argument | Description |
| --- | --- |
| `--phase <PHASE>` | Per-coil, per-echo wrapped phase NIfTI files (3D each). Files carrying BIDS `coil-NN` and `echo-N` entities are ordered by those; otherwise they are taken coil-major in the given order and `--num-echoes` is required. Required. |
| `--magnitude <MAGNITUDE>` | Per-coil, per-echo magnitude NIfTI files, same layout as --phase. Required. |
| `-o, --output <OUTPUT>` | Output prefix: writes &lt;prefix&gt;_rec-mcpc3ds_echo-N_part-phase_&lt;suffix&gt;.nii, &lt;prefix&gt;_rec-mcpc3ds_echo-N_part-mag_&lt;suffix&gt;.nii and &lt;prefix&gt;_desc-mcpc3ds_mask.nii — the pipeline's names. &lt;suffix&gt; is the inputs' BIDS suffix (MEGRE when they carry none) Required. |
| `--tes <TES>` | Echo times in seconds, one per echo (read from the phase JSON sidecars when omitted) |
| `--num-echoes <NUM_ECHOES>` | Number of echoes when the file names carry no `echo-N` entity. |
| `--sigma <X> <Y> <Z>` | Smoothing sigma (voxels) for the per-coil phase offsets (default 10 10 5) |
| `--hip-unwrapping <HIP_UNWRAPPING>` | Unwrapping used on the coil-summed HIP phase. Values: `romeo`, `laplacian`, `prelude`, `medi-region-growing`. Default: `romeo`. |
| `--prelude-labelling <PRELUDE_LABELLING>` | PRELUDE: how the initial regions are labelled (default: auto) Values: `auto`, `slices`, `volume`. |
| `--prelude-partitions <N>` | PRELUDE: number of phase partitions the initial regions are formed from (default: 8) |

<!-- generated:cli-combine-subcommands end -->

## `qsmxt unwrap`

<!-- generated:cli-unwrap start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Phase unwrapping

| Subcommand | Description |
| --- | --- |
| [`romeo`](#qsmxt-unwrap-romeo) | ROMEO region-growing unwrapping |
| [`laplacian`](#qsmxt-unwrap-laplacian) | Laplacian-based unwrapping |
| [`prelude`](#qsmxt-unwrap-prelude) | PRELUDE region-merging unwrapping |
| [`medi-region-growing`](#qsmxt-unwrap-medi-region-growing) | MEDI's magnitude-guided region-growing unwrapping (takes no mask) |

<!-- generated:cli-unwrap end -->

<!-- generated:cli-unwrap-subcommands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

### `qsmxt unwrap romeo`

ROMEO region-growing unwrapping

```sh
qsmxt unwrap romeo [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input wrapped phase NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output unwrapped phase NIfTI file. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude image (improves quality) |
| `--romeo-individual` | ROMEO: unwrap each echo on its own (individual mode, for motion or time series) |
| `--no-romeo-individual` | ROMEO: unwrap the template echo spatially and the others temporally from it (default) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) (default: 1) |
| `--romeo-correct-global` | ROMEO: correct global 2π offsets, of each echo and between echoes (default: off) |
| `--no-romeo-correct-global` | ROMEO: do not correct global 2π offsets (default) |
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on; the romeo command line's romeo3 weights) |
| `--romeo-phase-linearity` | ROMEO: enable phase linearity weights (default: on; for a --pipeline that turns them off) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: Maximum number of seed regions (default: 1) |
| `--romeo-merge-regions` | ROMEO: merge neighboring regions after unwrapping (default: off) |
| `--romeo-correct-regions` | ROMEO: correct each region's median to nearest 0 (default: off) |
| `--romeo-wrap-addition <ROMEO_WRAP_ADDITION>` | ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0) |

### `qsmxt unwrap laplacian`

Laplacian-based unwrapping

```sh
qsmxt unwrap laplacian --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input wrapped phase NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output unwrapped phase NIfTI file. Required. |

### `qsmxt unwrap prelude`

PRELUDE region-merging unwrapping

```sh
qsmxt unwrap prelude [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input wrapped phase NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output unwrapped phase NIfTI file. Required. |
| `--prelude-labelling <PRELUDE_LABELLING>` | PRELUDE: how the initial regions are labelled (default: auto) Values: `auto`, `slices`, `volume`. |
| `--prelude-partitions <N>` | PRELUDE: number of phase partitions the initial regions are formed from (default: 8) |

### `qsmxt unwrap medi-region-growing`

MEDI's magnitude-guided region-growing unwrapping (takes no mask)

```sh
qsmxt unwrap medi-region-growing --magnitude <MAGNITUDE> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input wrapped phase NIfTI file. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI file (3D): the order in which the volume is unwrapped. Required. Required. |
| `-o, --output <OUTPUT>` | Output unwrapped phase NIfTI file. Required. |

<!-- generated:cli-unwrap-subcommands end -->

## `qsmxt fieldmap`

<!-- generated:cli-fieldmap start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

B0 field mapping from multi-echo phase (NIfTI in, ppm out)

| Subcommand | Description |
| --- | --- |
| [`romeo`](#qsmxt-fieldmap-romeo) | ROMEO-based multi-echo field mapping (phase offset removal + unwrap + B0 fit) |
| [`laplacian`](#qsmxt-fieldmap-laplacian) | Laplacian-based multi-echo field mapping (per-echo unwrap + echo combination) |
| [`prelude`](#qsmxt-fieldmap-prelude) | PRELUDE-based multi-echo field mapping (phase offset removal + per-echo unwrap + B0 fit) |
| [`medi-region-growing`](#qsmxt-fieldmap-medi-region-growing) | MEDI region-growing field mapping (phase offset removal + per-echo unwrap + B0 fit; with `--b0-estimation complex-fit`, MEDI's field map). Needs `--magnitude` |

<!-- generated:cli-fieldmap end -->

Turn multi-echo wrapped phase into one B0 field map in ppm, as the pipeline's
field-mapping stage does: `romeo` (phase-offset removal, ROMEO unwrapping and the
B0 fit), `laplacian` (per-echo Laplacian unwrapping, then echo combination),
`prelude` (phase-offset removal, per-echo PRELUDE unwrapping and the B0 fit) or
`medi-region-growing` (the same with MEDI's region growing; needs `--magnitude`).
`qsmxt fieldmap medi-region-growing --b0-estimation complex-fit` is MEDI's field map.

With `--b0-estimation complex-fit` (MEDI's fit of the complex echoes, which needs
`--magnitude` and evenly spaced echoes), `--complex-fit-output` also saves the fit
before spatial unwrapping, the phase accrued per echo spacing (radians, wrapped), and
`--complex-fit-noise-output` its noise map, the standard deviation of that phase
(radians per echo spacing). MEDI calls these `iFreq_raw` and `N_std`.

<!-- generated:cli-fieldmap-subcommands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

### `qsmxt fieldmap romeo`

ROMEO-based multi-echo field mapping (phase offset removal + unwrap + B0 fit)

```sh
qsmxt fieldmap romeo [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input multi-echo wrapped phase NIfTI file (4D: x,y,z,echo) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output B0 field map NIfTI file (in ppm) Required. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude NIfTI file (4D; improves weighting) |
| `--tes <TES>` | Echo times in seconds (space-separated, one per echo) |
| `--b0 <B0>` | Field strength in Tesla (alias: --field-strength) |
| `--params <PARAMS>` | JSON params file: {"TE":[..seconds], "B0":&lt;tesla&gt;, "voxel_size":[..], "B0_dir":[..]}. --tes / --b0 override values from this file when both are present. |
| `--reliability-output <RELIABILITY_OUTPUT>` | `--b0-estimation qsmart-echofit`: also save its reliability map (1 = reliable) here, for `qsmxt qsmart --reliability`. |
| `--field-mask-output <FIELD_MASK_OUTPUT>` | Also save the mask the field map is defined in here: the mask, less the voxels where phase offset removal could not estimate the phase offset (the field is 0 there). Pass it to `qsmxt bgremove` and the inversions in place of the mask. |
| `--complex-fit-output <FILE>` | `--b0-estimation complex-fit`: also save the fit itself here, before spatial unwrapping: the phase accrued per echo spacing (radians, wrapped to [-π, π]; MEDI's `iFreq_raw`) |
| `--complex-fit-noise-output <FILE>` | `--b0-estimation complex-fit`: also save the fit's noise map here: the standard deviation of the phase per echo spacing, in radians (MEDI's `N_std`) |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Remove the phase offset of multi-echo data with MCPC-3D-S (Eckstein et al., 2018) before unwrapping (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method (default: weighted-avg) Values: `weighted-avg`, `linear-fit`, `robust-fit`, `qsmart-echofit`, `complex-fit`. |
| `--qsmart-fit-threshold-mode <QSMART_FIT_THRESHOLD_MODE>` | `--b0-estimation qsmart-echofit`: how the fit residual at which a voxel is left out of QSMART's reconstruction is set: a percentile of the blurred residual over the volume (`--qsmart-fit-percentile`), or a fixed residual (`--qsmart-fit-threshold`; default, QSMART v1.0's 40). Without it, giving one of those two selects its mode. Values: `percentile`, `absolute`. |
| `--qsmart-fit-percentile <QSMART_FIT_PERCENTILE>` | `--b0-estimation qsmart-echofit`: the percentile of the blurred fit residual, in (0, 100], above which a voxel is left out of QSMART's reconstruction (default: 99) |
| `--qsmart-fit-threshold <QSMART_FIT_THRESHOLD>` | `--b0-estimation qsmart-echofit`: a fixed blurred fit residual above which a voxel is left out of QSMART's reconstruction, instead of a percentile (40 when the mode is `absolute` and none is given) (default: 40) |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO (default: phase-snr) Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--complex-fit-reltol <COMPLEX_FIT_RELTOL>` | Complex fit (`--b0-estimation complex-fit`): stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4) |
| `--complex-fit-max-iter <COMPLEX_FIT_MAX_ITER>` | Complex fit: Gauss-Newton iterations at most (default: 30) |
| `--romeo-individual` | ROMEO: unwrap each echo on its own (individual mode, for motion or time series) |
| `--no-romeo-individual` | ROMEO: unwrap the template echo spatially and the others temporally from it (default) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) (default: 1) |
| `--romeo-correct-global` | ROMEO: correct global 2π offsets, of each echo and between echoes (default: off) |
| `--no-romeo-correct-global` | ROMEO: do not correct global 2π offsets (default) |
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on; the romeo command line's romeo3 weights) |
| `--romeo-phase-linearity` | ROMEO: enable phase linearity weights (default: on; for a --pipeline that turns them off) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: Maximum number of seed regions (default: 1) |
| `--romeo-merge-regions` | ROMEO: merge neighboring regions after unwrapping (default: off) |
| `--romeo-correct-regions` | ROMEO: correct each region's median to nearest 0 (default: off) |
| `--romeo-wrap-addition <ROMEO_WRAP_ADDITION>` | ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0) |

### `qsmxt fieldmap laplacian`

Laplacian-based multi-echo field mapping (per-echo unwrap + echo combination)

```sh
qsmxt fieldmap laplacian [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input multi-echo wrapped phase NIfTI file (4D: x,y,z,echo) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output B0 field map NIfTI file (in ppm) Required. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude NIfTI file (4D; improves weighting) |
| `--tes <TES>` | Echo times in seconds (space-separated, one per echo) |
| `--b0 <B0>` | Field strength in Tesla (alias: --field-strength) |
| `--params <PARAMS>` | JSON params file: {"TE":[..seconds], "B0":&lt;tesla&gt;, "voxel_size":[..], "B0_dir":[..]}. --tes / --b0 override values from this file when both are present. |
| `--reliability-output <RELIABILITY_OUTPUT>` | `--b0-estimation qsmart-echofit`: also save its reliability map (1 = reliable) here, for `qsmxt qsmart --reliability`. |
| `--field-mask-output <FIELD_MASK_OUTPUT>` | Also save the mask the field map is defined in here: the mask, less the voxels where phase offset removal could not estimate the phase offset (the field is 0 there). Pass it to `qsmxt bgremove` and the inversions in place of the mask. |
| `--complex-fit-output <FILE>` | `--b0-estimation complex-fit`: also save the fit itself here, before spatial unwrapping: the phase accrued per echo spacing (radians, wrapped to [-π, π]; MEDI's `iFreq_raw`) |
| `--complex-fit-noise-output <FILE>` | `--b0-estimation complex-fit`: also save the fit's noise map here: the standard deviation of the phase per echo spacing, in radians (MEDI's `N_std`) |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Remove the phase offset of multi-echo data with MCPC-3D-S (Eckstein et al., 2018) before unwrapping (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method (default: weighted-avg) Values: `weighted-avg`, `linear-fit`, `robust-fit`, `qsmart-echofit`, `complex-fit`. |
| `--qsmart-fit-threshold-mode <QSMART_FIT_THRESHOLD_MODE>` | `--b0-estimation qsmart-echofit`: how the fit residual at which a voxel is left out of QSMART's reconstruction is set: a percentile of the blurred residual over the volume (`--qsmart-fit-percentile`), or a fixed residual (`--qsmart-fit-threshold`; default, QSMART v1.0's 40). Without it, giving one of those two selects its mode. Values: `percentile`, `absolute`. |
| `--qsmart-fit-percentile <QSMART_FIT_PERCENTILE>` | `--b0-estimation qsmart-echofit`: the percentile of the blurred fit residual, in (0, 100], above which a voxel is left out of QSMART's reconstruction (default: 99) |
| `--qsmart-fit-threshold <QSMART_FIT_THRESHOLD>` | `--b0-estimation qsmart-echofit`: a fixed blurred fit residual above which a voxel is left out of QSMART's reconstruction, instead of a percentile (40 when the mode is `absolute` and none is given) (default: 40) |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO (default: phase-snr) Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--complex-fit-reltol <COMPLEX_FIT_RELTOL>` | Complex fit (`--b0-estimation complex-fit`): stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4) |
| `--complex-fit-max-iter <COMPLEX_FIT_MAX_ITER>` | Complex fit: Gauss-Newton iterations at most (default: 30) |
| `--laplacian-solver <LAPLACIAN_SOLVER>` | Poisson solver for `--unwrapping-algorithm laplacian`: `dct` (unweighted least squares, Neumann/DCT solve; default), `fft` (Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume), `mirror` (the same on the volume mirrored along each axis, as QSMART unwraps) or `mgpcg` (QSM.jl's MGPCG: Laplacian unwrapping with LBV background removal in one step; QSM.jl follows it with V-SHARP to remove remaining background) Values: `dct`, `fft`, `mirror`, `mgpcg`. |
| `--laplacian-fft-pad <VOXELS>` | Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or three (x y z). Default 64. |
| `--laplacian-unmasked` | `--unwrapping-algorithm laplacian`: unwrap the raw phase over the whole volume, instead of the phase set to zero outside the brain mask, as QSMART does. With `--b0-estimation qsmart-echofit` the field map is then kept over the whole volume. |
| `--no-laplacian-unmasked` | `--unwrapping-algorithm laplacian`: unwrap the phase set to zero outside the brain mask (the default), after a `--pipeline` that unwraps the whole volume. |
| `--laplacian-keep-outside` | `--unwrapping-algorithm laplacian` with a weighted-average echo combination: keep the field over the whole volume, not only inside the brain mask, as UK Biobank's pipeline hands it to V-SHARP. |
| `--no-laplacian-keep-outside` | Keep the Laplacian field inside the brain mask only (the default), after a `--pipeline` that keeps it over the whole volume. |

### `qsmxt fieldmap prelude`

PRELUDE-based multi-echo field mapping (phase offset removal + per-echo unwrap + B0 fit)

```sh
qsmxt fieldmap prelude [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input multi-echo wrapped phase NIfTI file (4D: x,y,z,echo) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output B0 field map NIfTI file (in ppm) Required. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude NIfTI file (4D; improves weighting) |
| `--tes <TES>` | Echo times in seconds (space-separated, one per echo) |
| `--b0 <B0>` | Field strength in Tesla (alias: --field-strength) |
| `--params <PARAMS>` | JSON params file: {"TE":[..seconds], "B0":&lt;tesla&gt;, "voxel_size":[..], "B0_dir":[..]}. --tes / --b0 override values from this file when both are present. |
| `--reliability-output <RELIABILITY_OUTPUT>` | `--b0-estimation qsmart-echofit`: also save its reliability map (1 = reliable) here, for `qsmxt qsmart --reliability`. |
| `--field-mask-output <FIELD_MASK_OUTPUT>` | Also save the mask the field map is defined in here: the mask, less the voxels where phase offset removal could not estimate the phase offset (the field is 0 there). Pass it to `qsmxt bgremove` and the inversions in place of the mask. |
| `--complex-fit-output <FILE>` | `--b0-estimation complex-fit`: also save the fit itself here, before spatial unwrapping: the phase accrued per echo spacing (radians, wrapped to [-π, π]; MEDI's `iFreq_raw`) |
| `--complex-fit-noise-output <FILE>` | `--b0-estimation complex-fit`: also save the fit's noise map here: the standard deviation of the phase per echo spacing, in radians (MEDI's `N_std`) |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Remove the phase offset of multi-echo data with MCPC-3D-S (Eckstein et al., 2018) before unwrapping (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method (default: weighted-avg) Values: `weighted-avg`, `linear-fit`, `robust-fit`, `qsmart-echofit`, `complex-fit`. |
| `--qsmart-fit-threshold-mode <QSMART_FIT_THRESHOLD_MODE>` | `--b0-estimation qsmart-echofit`: how the fit residual at which a voxel is left out of QSMART's reconstruction is set: a percentile of the blurred residual over the volume (`--qsmart-fit-percentile`), or a fixed residual (`--qsmart-fit-threshold`; default, QSMART v1.0's 40). Without it, giving one of those two selects its mode. Values: `percentile`, `absolute`. |
| `--qsmart-fit-percentile <QSMART_FIT_PERCENTILE>` | `--b0-estimation qsmart-echofit`: the percentile of the blurred fit residual, in (0, 100], above which a voxel is left out of QSMART's reconstruction (default: 99) |
| `--qsmart-fit-threshold <QSMART_FIT_THRESHOLD>` | `--b0-estimation qsmart-echofit`: a fixed blurred fit residual above which a voxel is left out of QSMART's reconstruction, instead of a percentile (40 when the mode is `absolute` and none is given) (default: 40) |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO (default: phase-snr) Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--complex-fit-reltol <COMPLEX_FIT_RELTOL>` | Complex fit (`--b0-estimation complex-fit`): stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4) |
| `--complex-fit-max-iter <COMPLEX_FIT_MAX_ITER>` | Complex fit: Gauss-Newton iterations at most (default: 30) |
| `--prelude-labelling <PRELUDE_LABELLING>` | PRELUDE: how the initial regions are labelled (default: auto) Values: `auto`, `slices`, `volume`. |
| `--prelude-partitions <N>` | PRELUDE: number of phase partitions the initial regions are formed from (default: 8) |

### `qsmxt fieldmap medi-region-growing`

MEDI region-growing field mapping (phase offset removal + per-echo unwrap + B0 fit; with `--b0-estimation complex-fit`, MEDI's field map). Needs `--magnitude`

```sh
qsmxt fieldmap medi-region-growing [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input multi-echo wrapped phase NIfTI file (4D: x,y,z,echo) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output B0 field map NIfTI file (in ppm) Required. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude NIfTI file (4D; improves weighting) |
| `--tes <TES>` | Echo times in seconds (space-separated, one per echo) |
| `--b0 <B0>` | Field strength in Tesla (alias: --field-strength) |
| `--params <PARAMS>` | JSON params file: {"TE":[..seconds], "B0":&lt;tesla&gt;, "voxel_size":[..], "B0_dir":[..]}. --tes / --b0 override values from this file when both are present. |
| `--reliability-output <RELIABILITY_OUTPUT>` | `--b0-estimation qsmart-echofit`: also save its reliability map (1 = reliable) here, for `qsmxt qsmart --reliability`. |
| `--field-mask-output <FIELD_MASK_OUTPUT>` | Also save the mask the field map is defined in here: the mask, less the voxels where phase offset removal could not estimate the phase offset (the field is 0 there). Pass it to `qsmxt bgremove` and the inversions in place of the mask. |
| `--complex-fit-output <FILE>` | `--b0-estimation complex-fit`: also save the fit itself here, before spatial unwrapping: the phase accrued per echo spacing (radians, wrapped to [-π, π]; MEDI's `iFreq_raw`) |
| `--complex-fit-noise-output <FILE>` | `--b0-estimation complex-fit`: also save the fit's noise map here: the standard deviation of the phase per echo spacing, in radians (MEDI's `N_std`) |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Remove the phase offset of multi-echo data with MCPC-3D-S (Eckstein et al., 2018) before unwrapping (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method (default: weighted-avg) Values: `weighted-avg`, `linear-fit`, `robust-fit`, `qsmart-echofit`, `complex-fit`. |
| `--qsmart-fit-threshold-mode <QSMART_FIT_THRESHOLD_MODE>` | `--b0-estimation qsmart-echofit`: how the fit residual at which a voxel is left out of QSMART's reconstruction is set: a percentile of the blurred residual over the volume (`--qsmart-fit-percentile`), or a fixed residual (`--qsmart-fit-threshold`; default, QSMART v1.0's 40). Without it, giving one of those two selects its mode. Values: `percentile`, `absolute`. |
| `--qsmart-fit-percentile <QSMART_FIT_PERCENTILE>` | `--b0-estimation qsmart-echofit`: the percentile of the blurred fit residual, in (0, 100], above which a voxel is left out of QSMART's reconstruction (default: 99) |
| `--qsmart-fit-threshold <QSMART_FIT_THRESHOLD>` | `--b0-estimation qsmart-echofit`: a fixed blurred fit residual above which a voxel is left out of QSMART's reconstruction, instead of a percentile (40 when the mode is `absolute` and none is given) (default: 40) |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO (default: phase-snr) Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--complex-fit-reltol <COMPLEX_FIT_RELTOL>` | Complex fit (`--b0-estimation complex-fit`): stop once the update is this fraction of the first estimate, over the whole volume (default 1e-4) |
| `--complex-fit-max-iter <COMPLEX_FIT_MAX_ITER>` | Complex fit: Gauss-Newton iterations at most (default: 30) |

<!-- generated:cli-fieldmap-subcommands end -->

## `qsmxt bgremove`

<!-- generated:cli-bgremove start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Background field removal

| Subcommand | Description |
| --- | --- |
| [`vsharp`](#qsmxt-bgremove-vsharp) | Variable radius SHARP (V-SHARP) |
| [`pdf`](#qsmxt-bgremove-pdf) | Projection onto Dipole Fields |
| [`lbv`](#qsmxt-bgremove-lbv) | Laplacian Boundary Value |
| [`ismv`](#qsmxt-bgremove-ismv) | Iterative Spherical Mean Value |
| [`sharp`](#qsmxt-bgremove-sharp) | Spherical Harmonic Array Reconstruction Procedure |
| [`resharp`](#qsmxt-bgremove-resharp) | Regularized SHARP (RESHARP) with Tikhonov regularization |
| [`harperella`](#qsmxt-bgremove-harperella) | HARPERELLA integrated phase unwrapping + background removal |
| [`iharperella`](#qsmxt-bgremove-iharperella) | Improved HARPERELLA (iHARPERELLA) |
| [`bfrnet`](#qsmxt-bgremove-bfrnet) | BFRnet deep-learning background removal (weights downloaded on first use) |
| [`iqfm`](#qsmxt-bgremove-iqfm) | iQFM deep-learning joint unwrapping and background removal — input is every echo's WRAPPED phase (weights downloaded on first use) |

<!-- generated:cli-bgremove end -->

`qsmxt bgremove iqfm` reads every echo's **wrapped phase** (one 4D file) instead of a
field, with `--echo-times`, `--field-strength` and, to weight the echoes, the
magnitude (`--magnitude`, 4D), and writes the local field in ppm.

`qsmxt bgremove harperella` and `iharperella` read one wrapped echo in rad when
given `--te` and `--field-strength`, or an unwrapped total field in ppm without them
(which iHARPERELLA takes as it is), and write the local field in ppm.

`qsmxt bgremove pdf` takes every echo's magnitude (`--magnitude`, one 4D file or one
file per echo), `--echo-times` and `--field-strength`: PDF weights its fit by a noise
map made from them.

`qsmxt bgremove sharp --variant qsmjl` runs QSM.jl's SHARP (at QSM.jl's defaults, 18 × the smallest
voxel dimension and 0.05, unless `--sharp-radius`/`--sharp-threshold` say otherwise), and `qsmxt bgremove vsharp
--ball qsmjl` QSM.jl's SMV ball, for comparison with that implementation; `qsmxt run`
always runs the defaults.

<!-- generated:cli-bgremove-subcommands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

### `qsmxt bgremove vsharp`

Variable radius SHARP (V-SHARP)

```sh
qsmxt bgremove vsharp [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--ball <BALL>` | SMV ball: `default` (the voxels within r + 1/6 mm, centre excluded; a ball fits where all of it is in the mask) or `qsmjl` (QSM.jl's: within r, centre included) Values: `default`, `qsmjl`. |
| `--vsharp-deconvolve <BOOL>` | Undo V-SHARP's SMV high-pass with a truncated-SVD deconvolution (default: false) |
| `--vsharp-threshold <VSHARP_THRESHOLD>` | V-SHARP deconvolution (TSVD) threshold; only used with --vsharp-deconvolve true (default: 0.05) |
| `--vsharp-max-radius <VSHARP_MAX_RADIUS>` | V-SHARP: Largest SMV kernel radius, in mm (default: 12) |
| `--vsharp-min-radius <VSHARP_MIN_RADIUS>` | V-SHARP: Smallest SMV kernel radius, in mm (default: 2) |
| `--vsharp-radius-step <MM>` | V-SHARP step between kernel radii in mm (default: 1); the radii run from --vsharp-max-radius down to --vsharp-min-radius. |
| `--vsharp-grid <VSHARP_GRID>` | Grid V-SHARP estimates the background on: `isotropic` (default; spacing from --vsharp-grid-spacing), `native` (the acquired grid), or `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened). The local field keeps native resolution. Values: `native`, `finest`, `isotropic`. |
| `--vsharp-grid-spacing <MM>` | V-SHARP isotropic working-grid spacing in mm (default 1); used with --vsharp-grid isotropic. |

### `qsmxt bgremove pdf`

Projection onto Dipole Fields

```sh
qsmxt bgremove pdf [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude of every echo: one 4D NIfTI, or one 3D NIfTI per echo in echo order. Required. |
| `--pdf-tol <PDF_TOL>` | PDF tolerance, relative to the noise level (default: 0.1) |
| `--pdf-n-cg <PDF_N_CG>` | PDF: Maximum conjugate-gradient iterations (default: 30) |
| `--pdf-space <SPACE>` | PDF dipole kernel: image (the FFT of the dipole sampled on the voxel grid) or kspace (the analytic k-space kernel) (default: image) |
| `--pdf-n-pad <VOXELS>` | PDF zero-padding around the mask's bounding box, in voxels; 0 runs on the full volume (default: 40) |

### `qsmxt bgremove lbv`

Laplacian Boundary Value

```sh
qsmxt bgremove lbv [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--lbv-tol <LBV_TOL>` | LBV tolerance: stop the coarsest grid's relaxation when its summed absolute update falls below this, in radians over the echo spacing (default: 0.01) |
| `--lbv-depth <LEVEL>` | LBV coarsest multigrid level, at most 8 (default: log2 of the smallest dimension minus 3) |
| `--lbv-peel <LAYERS>` | LBV: Boundary layers removed from the mask beyond the first, in voxels (default: 0) |
| `--lbv-n1 <LBV_N1>` | LBV: Relaxations on each grid before the coarser grid (default: 30) |
| `--lbv-n2 <LBV_N2>` | LBV: Relaxations on each grid after the coarser grid (default: 100) |
| `--lbv-n3 <LBV_N3>` | LBV: Relaxations on the finest grid at the end (default: 100) |
| `--lbv-precision <PRECISION>` | LBV arithmetic: double, or single as the original implementation (default: double) |

### `qsmxt bgremove ismv`

Iterative Spherical Mean Value

```sh
qsmxt bgremove ismv [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--ismv-variant <VARIANT>` | iSMV implementation: wen2014 (the authors' paper and code) or qsmjl (QSM.jl's) (default: wen2014) |
| `--ismv-tol <ISMV_TOL>` | iSMV tolerance: the relative change between iterates it stops at (qsmjl: the residual relative to the first) (default: 5e-5) |
| `--ismv-max-iter <ISMV_MAX_ITER>` | iSMV max iterations (default: no limit) |
| `--ismv-radius <MM>` | iSMV kernel radius in mm, or auto for 6 × the smallest voxel dimension (qsmjl: 2 × the largest) (default: auto) |

### `qsmxt bgremove sharp`

Spherical Harmonic Array Reconstruction Procedure

```sh
qsmxt bgremove sharp [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--variant <VARIANT>` | Implementation: sepia (SEPIA's SHARP.m, the default) or qsmjl (QSM.jl's `sharp`, with its own defaults) Values: `sepia`, `qsmjl`. |
| `--sharp-threshold <SHARP_THRESHOLD>` | SHARP: Deconvolution threshold (default: 0.03) |
| `--sharp-radius <SHARP_RADIUS>` | SHARP: SMV kernel radius, in mm (default: 4) |

### `qsmxt bgremove resharp`

Regularized SHARP (RESHARP) with Tikhonov regularization

```sh
qsmxt bgremove resharp [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--resharp-radius <RESHARP_RADIUS>` | RESHARP: SMV kernel radius, in mm (default: 3) |
| `--resharp-tik-reg <RESHARP_TIK_REG>` | RESHARP: Tikhonov regularisation parameter (default: 0.0001) |
| `--resharp-tol <RESHARP_TOL>` | RESHARP conjugate gradients squared (CGS) relative tolerance (default: 1e-6) |
| `--resharp-max-iter <RESHARP_MAX_ITER>` | RESHARP: Maximum CGS iterations (default: 200) |

### `qsmxt bgremove harperella`

HARPERELLA integrated phase unwrapping + background removal

```sh
qsmxt bgremove harperella [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI: a wrapped echo in rad (with --te and --field-strength) or a total field in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--te <TE>` | Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped echo in rad (the published method); without both, it is an unwrapped total field in ppm. The local field is written in ppm either way. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength (T) of a wrapped-phase input — see --te. |
| `--harperella-radius <HARPERELLA_RADIUS>` | HARPERELLA: SMV kernel radius, in mm (default: 10) |
| `--harperella-max-iter <HARPERELLA_MAX_ITER>` | HARPERELLA: Maximum iterations (default: 40) |
| `--harperella-tol <HARPERELLA_TOL>` | HARPERELLA: Convergence tolerance (default: 0.001) |
| `--harperella-pad <X> <Y> <Z>` | HARPERELLA zero-padding in voxels per side, X Y Z (default: 0 0 0) |
| `--harperella-rim <RIM>` | HARPERELLA boundary rim: whole (zero every Laplacian value in the rim; default) or sti (zero the 20 % largest, as STI Suite does) |

### `qsmxt bgremove iharperella`

Improved HARPERELLA (iHARPERELLA)

```sh
qsmxt bgremove iharperella [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI: a wrapped echo in rad (with --te and --field-strength) or a total field in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--te <TE>` | Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped echo in rad, unwrapped by HARPERELLA's Laplacian; without both, it is an unwrapped total field in ppm, which iHARPERELLA takes as it is. The local field is written in ppm either way. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength (T) of a wrapped-phase input — see --te. |
| `--iharperella-max-iter <IHARPERELLA_MAX_ITER>` | iHARPERELLA: Maximum iterations (default: 40) |
| `--iharperella-tol <IHARPERELLA_TOL>` | iHARPERELLA: Convergence tolerance (default: 0.001) |
| `--iharperella-pad <X> <Y> <Z>` | iHARPERELLA zero-padding in voxels per side, X Y Z (default: 0 0 0) |
| `--iharperella-rim <RIM>` | iHARPERELLA boundary rim: whole (zero every Laplacian value in the rim) or sti (zero the 20 % largest, as STI Suite does; default) |

### `qsmxt bgremove bfrnet`

BFRnet deep-learning background removal (weights downloaded on first use)

```sh
qsmxt bgremove bfrnet [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |

### `qsmxt bgremove iqfm`

iQFM deep-learning joint unwrapping and background removal — input is every echo's WRAPPED phase (weights downloaded on first use)

```sh
qsmxt bgremove iqfm [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Wrapped phase NIfTI file in radians: 4D (x,y,z,echo), or 3D for one echo. Required. |
| `-m, --mask <MASK>` | Binary brain mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output NIfTI file in ppm (susceptibility for iqsm/iqsm-plus, the local field for iqfm) Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI file with the same echoes as the phase (4D), to weight the echo combination by magnitude; without it the echoes are weighted by TE² alone. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--echo-weighting <ECHO_WEIGHTING>` | Echo weighting of the multi-echo combination: `mag2-te2` weights each echo by (magnitude × TE)², the inverse of its noise variance, as in the authors' Python code; `mag-te2` by magnitude × TE², as in the authors' MATLAB code. Values: `mag-te2`, `mag2-te2`. Default: `mag2-te2`. |

<!-- generated:cli-bgremove-subcommands end -->

## `qsmxt invert`

<!-- generated:cli-invert start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Dipole inversion

| Subcommand | Description |
| --- | --- |
| [`rts`](#qsmxt-invert-rts) | Rapid Two-Step (RTS) |
| [`tv`](#qsmxt-invert-tv) | Total Variation (ADMM) |
| [`tkd`](#qsmxt-invert-tkd) | Truncated K-space Division |
| [`tsvd`](#qsmxt-invert-tsvd) | Truncated Singular Value Decomposition |
| [`tgv`](#qsmxt-invert-tgv) | Total Generalized Variation |
| [`tikhonov`](#qsmxt-invert-tikhonov) | Tikhonov regularization |
| [`nltv`](#qsmxt-invert-nltv) | Nonlinear Total Variation |
| [`medi`](#qsmxt-invert-medi) | Morphology Enabled Dipole Inversion |
| [`tfi`](#qsmxt-invert-tfi) | Total Field Inversion (preconditioned). Input is the TOTAL field (not a local field) |
| [`ilsqr`](#qsmxt-invert-ilsqr) | Iterative Least Squares QR |
| [`star`](#qsmxt-invert-star) | STAR-QSM (Streaking Artifact Reduction): sparse inversion from a k-space initial guess |
| [`lsqr`](#qsmxt-invert-lsqr) | Minimally regularised LSQR |
| [`heidi`](#qsmxt-invert-heidi) | LSQR+HEIDI (Homogeneity Enabled Incremental Dipole Inversion), or HEIDI on another inversion's map (`--seed`) |
| [`ndi`](#qsmxt-invert-ndi) | Nonlinear Dipole Inversion |
| [`fansi`](#qsmxt-invert-fansi) | FANSI Nonlinear Total Variation |
| [`fansi-tgv`](#qsmxt-invert-fansi-tgv) | FANSI Nonlinear Total Generalized Variation |
| [`l1qsm`](#qsmxt-invert-l1qsm) | L1 Data-Fidelity QSM |
| [`whqsm`](#qsmxt-invert-whqsm) | Weak-Harmonic QSM |
| [`hdqsm`](#qsmxt-invert-hdqsm) | Hybrid Data-Fidelity QSM |
| [`amp-pe`](#qsmxt-invert-amp-pe) | AMP-PE (Approximate Message Passing with Parameter Estimation) |
| [`xqsm`](#qsmxt-invert-xqsm) | xQSM deep-learning dipole inversion (weights downloaded on first use) |
| [`qsmnet`](#qsmxt-invert-qsmnet) | QSMnet deep-learning dipole inversion (weights downloaded on first use) |
| [`qsmnet-plus`](#qsmxt-invert-qsmnet-plus) | QSMnet+ deep-learning dipole inversion (weights downloaded on first use) |
| [`qsmnetme`](#qsmxt-invert-qsmnetme) | QSMnetME deep-learning dipole inversion, the χ-separation toolbox's QSM network (weights downloaded on first use) |
| [`autoqsm`](#qsmxt-invert-autoqsm) | AutoQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use) |
| [`qsmgan`](#qsmxt-invert-qsmgan) | QSMGAN deep-learning dipole inversion (weights downloaded on first use) |
| [`ir2qsm`](#qsmxt-invert-ir2qsm) | IR2QSM deep-learning dipole inversion (weights downloaded on first use) |
| [`lpcnn`](#qsmxt-invert-lpcnn) | LPCNN deep-learning dipole inversion (weights downloaded on first use) |
| [`modl-qsm`](#qsmxt-invert-modl-qsm) | MoDL-QSM deep-learning dipole inversion → χ33/STI component (weights downloaded on first use) |
| [`nextqsm`](#qsmxt-invert-nextqsm) | NeXtQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use) |
| [`iqsm`](#qsmxt-invert-iqsm) | iQSM single-step reconstruction — input is every echo's WRAPPED phase (weights downloaded on first use) |
| [`iqsm-plus`](#qsmxt-invert-iqsm-plus) | iQSM+ orientation-adaptive single-step reconstruction — input is every echo's WRAPPED phase; echoes are combined with (magnitude × TE)² weights (weights downloaded on first use) |
| [`cosmos`](#qsmxt-invert-cosmos) | COSMOS — multi-orientation least squares over 2+ co-registered orientations |
| [`sti`](#qsmxt-invert-sti) | STI (susceptibility tensor imaging) — 6+ co-registered orientations to a rank-2 tensor |

<!-- generated:cli-invert end -->

Most methods take the local field; `tfi`, `autoqsm` and `nextqsm` take the **total** field.

`iqsm` and
`iqsm-plus` read every echo's wrapped phase (one 4D file) instead of a field, with
`--echo-times`, `--field-strength`, `--b0-direction` (an input of iQSM+'s network) and,
to weight the echoes, the magnitude (`--magnitude`, 4D).

Each subcommand runs its method as `qsmxt run` does, through QSM.rs's pipeline: the
field goes in in ppm, and the methods that work in radians (`ndi`, `nltv`, `fansi`,
`fansi-tgv`, `l1qsm`, `whqsm`, `medi`, `tfi`, `heidi`, `tgv`, `star`) convert it themselves
with `--field-strength` and `--echo-times` (every echo's). `ndi`, `nltv`, `fansi`,
`fansi-tgv`, `l1qsm`, `lsqr` and `heidi` (without `--seed`) need `--magnitude` as their data
weight; `medi` and `tfi`
need the magnitude of every echo (one 4D file or one file per echo). MEDI-mSMV is
`qsmxt invert medi --medi-msmv`.

`qsmxt invert heidi` is LSQR+HEIDI, HEIDI as published. `qsmxt invert heidi --seed chi.nii`
instead runs HEIDI on a map another inversion made of the same field, as `qsmxt run --heidi`
does after it ([HEIDI streaking reduction](/QSMxT/reference/methods/#heidi-streaking-reduction)
says which inversions suit it); `--lsqr-*` and `--magnitude` then do nothing.

`qsmxt invert tgv --reference-divergence --reference-edge-erosion --reference-output-box`
reproduces QuantitativeSusceptibilityMappingTGV.jl's divergence, mask erosion and output
box, for comparison with that package; `qsmxt run` always uses the exact adjoint, erodes
from the array's edge too, and returns χ inside the eroded mask only.

`cosmos` and `sti` take several `--input` field maps plus one `--b0-direction` per
orientation, and expect them already co-registered onto a common grid. See
[multi-orientation](/QSMxT/reference/multi-orientation/).

<!-- generated:cli-invert-subcommands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

### `qsmxt invert rts`

Rapid Two-Step (RTS)

```sh
qsmxt invert rts [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--rts-delta <RTS_DELTA>` | RTS: Threshold for ill-conditioned k-space region (default: 0.15) |
| `--rts-mu <RTS_MU>` | RTS: Regularization parameter for well-conditioned region (default: 100000) |
| `--rts-rho <RTS_RHO>` | RTS rho (ADMM penalty) (default: 10) |
| `--rts-tol <RTS_TOL>` | RTS: Convergence tolerance (relative change) (default: 0.01) |
| `--rts-max-iter <RTS_MAX_ITER>` | RTS: Maximum ADMM iterations (default: 20) |
| `--rts-lsmr-iter <RTS_LSMR_ITER>` | RTS: LSMR iterations for step 1 (well-conditioned solve) (default: 4) |
| `--rts-tau <RTS_TAU>` | RTS: rho is raised when the primal residual falls by less than this factor (tau) (default: 0.7) |
| `--rts-gamma <RTS_GAMMA>` | RTS: the factor rho is raised by (gamma) (default: 5) |
| `--rts-pad <X> <Y> <Z>` | RTS: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |

### `qsmxt invert tv`

Total Variation (ADMM)

```sh
qsmxt invert tv [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--tv-lambda <TV_LAMBDA>` | TV-ADMM: L1 regularisation weight (larger = smoother) (default: 0.0003) |
| `--tv-rho <RHO>` | TV rho (ADMM penalty), or auto for 100 × lambda (default: auto) |
| `--tv-tol <TV_TOL>` | TV-ADMM: Convergence tolerance (default: 0.001) |
| `--tv-max-iter <TV_MAX_ITER>` | TV-ADMM: Maximum ADMM iterations (default: 250) |
| `--tv-pad <X> <Y> <Z>` | TV-ADMM: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |

### `qsmxt invert tkd`

Truncated K-space Division

```sh
qsmxt invert tkd [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--tkd-threshold <TKD_THRESHOLD>` | TKD threshold on \|D\|; larger is smoother and underestimates χ more (default: 0.25) |
| `--tkd-pad <X> <Y> <Z>` | TKD: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |

### `qsmxt invert tsvd`

Truncated Singular Value Decomposition

```sh
qsmxt invert tsvd [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--tsvd-threshold <TSVD_THRESHOLD>` | TSVD: Truncation threshold for SVD (0.1-0.2) (default: 0.15) |
| `--tsvd-pad <X> <Y> <Z>` | TSVD: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |

### `qsmxt invert tgv`

Total Generalized Variation

```sh
qsmxt invert tgv [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--reference-divergence` | Use QuantitativeSusceptibilityMappingTGV.jl's divergence in the w update rather than the exact adjoint of the symmetric gradient. Changes the result only on anisotropic voxels; with --reference-edge-erosion and --reference-output-box it reproduces that package, for comparison with it. |
| `--reference-edge-erosion` | Erode the mask as QuantitativeSusceptibilityMappingTGV.jl does, not from the array's edge. Changes the result only where the mask reaches an array face. |
| `--reference-output-box` | Return χ over QuantitativeSusceptibilityMappingTGV.jl's whole output box, not only inside the eroded mask: adds the rim of up to two voxels just outside it, where no data constrain χ. |
| `--tgv-iterations <TGV_ITERATIONS>` | TGV iterations (default: from the voxel size, as the reference does) |
| `--tgv-erosions <TGV_EROSIONS>` | TGV: Mask erosions before TGV solve (default: 3) |
| `--tgv-alpha1 <TGV_ALPHA1>` | TGV alpha1 (first-order weight), in radians at the echo time (--tgv-te) and field strength (the reference's units) (default: 0.00212; QuantitativeSusceptibilityMappingTGV.jl's regularization 2 is 0.003) |
| `--tgv-alpha0 <TGV_ALPHA0>` | TGV alpha0 (second-order weight), in radians at the echo time (--tgv-te) and field strength (the reference's units) (default: 0.00636; QuantitativeSusceptibilityMappingTGV.jl's regularization 2 is 0.002) |
| `--tgv-step-size <TGV_STEP_SIZE>` | TGV: Primal step-size multiplier (default: 3) |
| `--tgv-te <TE>` | TGV: the echo time a field from several echoes is converted to phase at, and its weights are in radians at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first). A single echo is at its own. |

### `qsmxt invert tikhonov`

Tikhonov regularization

```sh
qsmxt invert tikhonov [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--tikhonov-lambda <TIKHONOV_LAMBDA>` | Tikhonov: L2 regularisation weight (default: 0.01) |
| `--tikhonov-pad <X> <Y> <Z>` | Tikhonov: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |

### `qsmxt invert nltv`

Nonlinear Total Variation

```sh
qsmxt invert nltv [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. Required. |
| `--nltv-lambda <NLTV_LAMBDA>` | NLTV: Regularisation parameter (default: 0.001) |
| `--nltv-rho <RHO>` | NLTV rho (ADMM penalty), or auto for 100 × lambda (default: auto) |
| `--nltv-mu <NLTV_MU>` | NLTV mu (penalty parameter) (default: 1) |
| `--nltv-tol <NLTV_TOL>` | NLTV: Convergence tolerance (default: 0.001) |
| `--nltv-max-iter <NLTV_MAX_ITER>` | NLTV: Maximum ADMM iterations (default: 250) |
| `--nltv-tol-newton <NLTV_TOL_NEWTON>` | NLTV: Convergence tolerance of the Newton iterations of the data-term subproblem (default: 1e-6) |
| `--nltv-newton-iter <NLTV_NEWTON_ITER>` | NLTV: Maximum Newton iterations of the data-term subproblem (default: 10) |
| `--nltv-te <TE>` | NLTV: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |
| `--nltv-pad <X> <Y> <Z>` | NLTV: Zero-padding before the FFTs (x y z): at least this many, rounded up to an FFT-efficient size; -1 leaves an axis unpadded, in voxels (default: 0 0 0) |

### `qsmxt invert medi`

Morphology Enabled Dipole Inversion

```sh
qsmxt invert medi [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude of every echo: one 4D NIfTI, or one 3D NIfTI per echo in echo order. Required. |
| `--no-medi-smv` | MEDI: no SMV high-pass of the local field. |
| `--medi-smv` | MEDI: high-pass the local field with a spherical mean value filter, removing residual background near the brain edge (MEDI-SMV; on by default) |
| `--medi-smv-radius <MEDI_SMV_RADIUS>` | MEDI SMV radius in mm (also mSMV's) (default: 5) |
| `--no-medi-smv-shrink-mask` | MEDI-SMV: keep the full mask, high-passing the field over it. |
| `--medi-smv-shrink-mask` | MEDI-SMV: shrink the mask to where the SMV of the mask exceeds 0.999 (on by default) |
| `--medi-msmv` | MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value before MEDI, which fits it with the SMV-filtered dipole kernel (it implies the SMV). Needs at least 3 echoes. |
| `--medi-msmv-maxk <MEDI_MSMV_MAXK>` | MEDI-mSMV: the boundary correction makes at most this many passes, less one (default: 5) |
| `--medi-msmv-vessel-radius <MEDI_MSMV_VESSEL_RADIUS>` | MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 × the largest voxel dimension if that is more) |
| `--no-medi-msmv-prefilter` | MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges (expert) |
| `--medi-lambda <MEDI_LAMBDA>` | MEDI lambda: the data-fidelity weight (larger fits the field more closely) (default: 1000) |
| `--medi-percentage <MEDI_PERCENTAGE>` | MEDI edge percentage (0.0-1.0) (default: 0.9) |
| `--medi-max-iter <MEDI_MAX_ITER>` | MEDI: Maximum outer iterations (default: 10) |
| `--medi-cg-max-iter <MEDI_CG_MAX_ITER>` | MEDI: Maximum conjugate gradient iterations (default: 100) |
| `--medi-cg-tol <MEDI_CG_TOL>` | MEDI: CG convergence tolerance (default: 0.01) |
| `--medi-tol <MEDI_TOL>` | MEDI: Outer convergence tolerance (default: 0.1) |
| `--medi-merit <MEDI_MERIT>` | MEDI: enable MERIT weighting (default: true) |
| `--medi-csf <MEDI_CSF>` | MEDI+0 (Liu et al. 2018): zero-reference χ to the ventricular CSF, found from R2\*, and keep it uniform there (default: true). Needs at least 3 echoes; with fewer, MEDI runs without it. |
| `--medi-lambda-csf <MEDI_LAMBDA_CSF>` | MEDI+0: weight of the CSF uniformity term (default: 100) |
| `--medi-csf-r2s-threshold <MEDI_CSF_R2S_THRESHOLD>` | MEDI+0: R2\* in Hz below which a voxel is a CSF candidate (default: 5) |
| `--medi-csf-erode <MEDI_CSF_ERODE>` | MEDI+0: erode the mask by a 10 mm sphere before finding the CSF (default: true) |
| `--medi-zeropad <X> <Y> <Z>` | MEDI: voxels of zeros appended at the end of x, y and z before the inversion; the result is cropped back (default: 0 0 0) |
| `--medi-data-weighting <MEDI_DATA_WEIGHTING>` | MEDI: data weighting mode (0=uniform, 1=SNR) (default: 1) |
| `--medi-te <TE>` | MEDI: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing) |

### `qsmxt invert tfi`

Total Field Inversion (preconditioned). Input is the TOTAL field (not a local field)

```sh
qsmxt invert tfi [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude of every echo: one 4D NIfTI, or one 3D NIfTI per echo in echo order. Required. |
| `--kernel <KERNEL>` | Dipole kernel on a grid with an odd number of voxels along an axis: default samples the dipole on the Fourier grid; original samples it half a frequency step off, as the original implementation does, for comparison with it. Identical on even grids (default: default) Values: `default`, `original`. |
| `--precision <PRECISION>` | Arithmetic: double, or single as the original implementation runs, for comparison with it (default: double) Values: `double`, `single`. |
| `--tfi-lambda <TFI_LAMBDA>` | TFI lambda: the data-fidelity weight (larger fits the field more closely) (default: 1000) |
| `--tfi-precond <TFI_PRECOND>` | TFI: Preconditioner value (susceptibility scaling outside the mask) (default: 30) |
| `--tfi-percentage <TFI_PERCENTAGE>` | TFI edge percentage (0.0-1.0) (default: 0.9) |
| `--tfi-max-iter <TFI_MAX_ITER>` | TFI: Maximum outer iterations (default: 10) |
| `--tfi-cg-max-iter <TFI_CG_MAX_ITER>` | TFI: Maximum conjugate gradient iterations (default: 100) |
| `--tfi-cg-tol <TFI_CG_TOL>` | TFI: CG convergence tolerance (default: 0.01) |
| `--tfi-tol <TFI_TOL>` | TFI: Outer convergence tolerance (default: 0.1) |
| `--tfi-merit <TFI_MERIT>` | TFI: enable MERIT weighting (default: true) |
| `--tfi-linear <TFI_LINEAR>` | TFI: fit the field linearly, as the TFI paper does (default: true); false uses MEDI's exponential data term. |
| `--tfi-data-weighting <TFI_DATA_WEIGHTING>` | TFI: data weighting mode (0=uniform, 1=SNR) (default: 1) |
| `--tfi-te <TE>` | TFI: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing) |

### `qsmxt invert ilsqr`

Iterative Least Squares QR

```sh
qsmxt invert ilsqr [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--ilsqr-tol <ILSQR_TOL>` | iLSQR tolerance of the streaking-artefact solve (default: 0.001) |
| `--ilsqr-max-iter <ILSQR_MAX_ITER>` | iLSQR max iterations of each LSQR solve (default: 100) |
| `--ilsqr-pad-mm <MM>` | iLSQR zero-padding in mm per side (default: 12) |
| `--ilsqr-precision <PRECISION>` | iLSQR arithmetic of the two LSQR solves: double, or single as the original implementation (default: double) |

### `qsmxt invert star`

STAR-QSM (Streaking Artifact Reduction): sparse inversion from a k-space initial guess

```sh
qsmxt invert star [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--star-pad <X> <Y> <Z>` | STAR-QSM zero-padding in voxels per side, X Y Z (default: 8 8 20) |
| `--star-te <TE>` | Echo time STAR-QSM converts the field to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: sum) |

### `qsmxt invert lsqr`

Minimally regularised LSQR

```sh
qsmxt invert lsqr [OPTIONS] --mask <MASK> --output <OUTPUT> --magnitude <MAGNITUDE>... --b0 <B0> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, LSQR's row weight as the HEIDI package's (its scale does not change the solution): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. Required. |
| `--b0 <B0>` | B0 field strength in Tesla — sets the default residual weighting. Required. |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance (default: 1e-5) |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR: Maximum LSQR iterations (default: 400) |

### `qsmxt invert heidi`

LSQR+HEIDI (Homogeneity Enabled Incremental Dipole Inversion), or HEIDI on another inversion's map (`--seed`)

```sh
qsmxt invert heidi [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, the row weight of the LSQR seed as the HEIDI package's (not needed with --seed): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. |
| `--seed <SEED>` | A χ map (ppm) another inversion made of this field, to fill the cone of instead of running LSQR (`--lsqr-*` and `--magnitude` then do nothing). HEIDI suits a seed with little or no smoothing (TKD and TSVD with a threshold no larger than the cone threshold, small-λ Tikhonov, NDI, RTS); a method that smooths its result also changes the parts of the data HEIDI keeps as they are, and HEIDI cannot undo that. |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance (default: 1e-5) |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR: Maximum LSQR iterations (default: 400) |
| `--heidi-cone-threshold <HEIDI_CONE_THRESHOLD>` | \|D(k)\| above which a coefficient is well-conditioned and kept from the seed map (default: 0.1) |
| `--heidi-gradient-threshold <HEIDI_GRADIENT_THRESHOLD>` | Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous (default: 3.924908970520748e-3) |
| `--no-heidi-laplacian-correction` | Do not open the homogeneity masks where the field's Laplacian is large. |
| `--heidi-laplacian-threshold <HEIDI_LAPLACIAN_THRESHOLD>` | HEIDI: Field-Laplacian threshold (ppm per voxel²) (default: 5.943433583931419e-2) |
| `--heidi-gradient-mask-floor <HEIDI_GRADIENT_MASK_FLOOR>` | TV weight kept where every direction reads as an edge (0 reproduces upstream's shipped binarised masks) (default: 0) |
| `--heidi-continuation-steps <HEIDI_CONTINUATION_STEPS>` | HEIDI: NESTA continuation steps on μ (default: 8) |
| `--heidi-inner-iterations <HEIDI_INNER_ITERATIONS>` | HEIDI: Accelerated-gradient iterations per continuation step (default: 500) |
| `--heidi-mu-min <HEIDI_MU_MIN>` | HEIDI: Final (smallest) smoothing parameter μ (default: 2.220446049250313e-16) |
| `--heidi-tol <HEIDI_TOL>` | HEIDI: Relative objective change that ends an inner loop early (the default runs every iteration) |
| `--heidi-te <TE>` | HEIDI: the echo time the field stands for the phase of, in the conversion to HEIDI's units: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: last) |
| `--no-heidi-denoise` | Skip the anisotropic-diffusion smoothing of the field before its gradients are taken. |
| `--heidi-denoise-iterations <HEIDI_DENOISE_ITERATIONS>` | HEIDI: Anisotropic-diffusion iterations (default: 5) |
| `--heidi-denoise-time-step <HEIDI_DENOISE_TIME_STEP>` | HEIDI: Anisotropic-diffusion time step (default: 0.05) |
| `--heidi-denoise-conductance <HEIDI_DENOISE_CONDUCTANCE>` | HEIDI: Anisotropic-diffusion conductance (default: 1) |
| `--heidi-denoise-reference-itk` | HEIDI: ITK 3.20's float arithmetic for the denoising, as the authors' code runs it (for comparison with it; the default computes its conductance scale in double precision) |
| `--no-heidi-denoise-reference-itk` | The denoising in double precision, after a pipeline that turns ITK's float arithmetic on (the HEIDI pipeline) |

### `qsmxt invert ndi`

Nonlinear Dipole Inversion

```sh
qsmxt invert ndi [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. Required. |
| `--ndi-tau <NDI_TAU>` | NDI gradient-descent step size (tau) (default: 2) |
| `--ndi-alpha <NDI_ALPHA>` | NDI L2 regularization weight (alpha) (default: 1e-5) |
| `--ndi-max-iter <NDI_MAX_ITER>` | NDI: Number of iterations (default: 100) |
| `--ndi-auto-stop <NDI_AUTO_STOP>` | NDI: stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto); --ndi-max-iter is then the maximum (default: false) |
| `--ndi-te <TE>` | NDI: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |
| `--ndi-weight-norm <NORM>` | NDI magnitude-weight normaliser: global-max (the magnitude's maximum, as FANSI; default) or mask-p999 (its 99.9th percentile inside the mask; more consistent in vivo) |

### `qsmxt invert fansi`

FANSI Nonlinear Total Variation

```sh
qsmxt invert fansi [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. Required. |
| `--fansi-alpha1 <FANSI_ALPHA1>` | FANSI first-order (TV/TGV) L1 penalty weight (alpha1) (default: 0.0002) |
| `--fansi-mu1 <MU1>` | FANSI gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto) |
| `--fansi-mu2 <FANSI_MU2>` | FANSI fidelity-consistency ADMM weight (mu2) (default: 1) |
| `--fansi-alpha0 <ALPHA0>` | FANSI second-order L1 penalty weight (alpha0, nlTGV only), or auto for 2 × alpha1 (default: auto) |
| `--fansi-mu0 <MU0>` | FANSI second-order consistency ADMM weight (mu0, nlTGV only), or auto for 2 × mu1 (default: auto) |
| `--fansi-max-iter <FANSI_MAX_ITER>` | FANSI: Number of outer ADMM iterations (default: 150) |
| `--fansi-tol-update <FANSI_TOL_UPDATE>` | FANSI: Percent-update stopping tolerance (default: 0.1) |
| `--fansi-tol-delta <FANSI_TOL_DELTA>` | FANSI: Inner Newton convergence tolerance (default: 1e-6) |
| `--fansi-te <TE>` | FANSI (nlTV and nlTGV): the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing) |

### `qsmxt invert fansi-tgv`

FANSI Nonlinear Total Generalized Variation

```sh
qsmxt invert fansi-tgv [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. Required. |
| `--fansi-alpha1 <FANSI_ALPHA1>` | FANSI first-order (TV/TGV) L1 penalty weight (alpha1) (default: 0.0002) |
| `--fansi-mu1 <MU1>` | FANSI gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto) |
| `--fansi-mu2 <FANSI_MU2>` | FANSI fidelity-consistency ADMM weight (mu2) (default: 1) |
| `--fansi-alpha0 <ALPHA0>` | FANSI second-order L1 penalty weight (alpha0, nlTGV only), or auto for 2 × alpha1 (default: auto) |
| `--fansi-mu0 <MU0>` | FANSI second-order consistency ADMM weight (mu0, nlTGV only), or auto for 2 × mu1 (default: auto) |
| `--fansi-max-iter <FANSI_MAX_ITER>` | FANSI: Number of outer ADMM iterations (default: 150) |
| `--fansi-tol-update <FANSI_TOL_UPDATE>` | FANSI: Percent-update stopping tolerance (default: 0.1) |
| `--fansi-tol-delta <FANSI_TOL_DELTA>` | FANSI: Inner Newton convergence tolerance (default: 1e-6) |
| `--fansi-te <TE>` | FANSI (nlTV and nlTGV): the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: spacing) |

### `qsmxt invert l1qsm`

L1 Data-Fidelity QSM

```sh
qsmxt invert l1qsm [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. Required. |
| `--l1qsm-alpha1 <L1QSM_ALPHA1>` | L1-QSM gradient (TV) L1 penalty weight (alpha1) (default: 0.005) |
| `--l1qsm-mu1 <MU1>` | L1-QSM gradient-consistency ADMM weight (mu1), or auto for 100 × alpha1 (default: auto) |
| `--l1qsm-mu2 <L1QSM_MU2>` | L1-QSM fidelity-consistency ADMM weight (mu2) (default: 1) |
| `--l1qsm-mu3 <L1QSM_MU3>` | L1-QSM L1 proximal ADMM weight (mu3) (default: 1) |
| `--l1qsm-lambda <L1QSM_LAMBDA>` | L1-QSM L1 fidelity strength (lambda) (default: 0.8) |
| `--l1qsm-max-iter <L1QSM_MAX_ITER>` | L1-QSM: Number of outer ADMM iterations (default: 50) |
| `--l1qsm-tol-update <L1QSM_TOL_UPDATE>` | L1-QSM: Percent-update stopping tolerance (default: 1) |
| `--l1qsm-tol-delta <L1QSM_TOL_DELTA>` | L1-QSM: Inner Newton convergence tolerance (default: 1e-6) |
| `--l1qsm-te <TE>` | L1-QSM: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |

### `qsmxt invert whqsm`

Weak-Harmonic QSM

```sh
qsmxt invert whqsm [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, for a magnitude data weight (--weight): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. |
| `--whqsm-alpha1 <WHQSM_ALPHA1>` | WH-QSM TV regularization weight (alpha1) (default: 0.0002) |
| `--whqsm-mu1 <WHQSM_MU1>` | WH-QSM ADMM penalty for TV splitting (mu1) (default: 0.02) |
| `--whqsm-mu2 <WHQSM_MU2>` | WH-QSM ADMM penalty for data-fidelity splitting (mu2) (default: 1) |
| `--whqsm-beta <WHQSM_BETA>` | WH-QSM weak-harmonic ROI penalty (beta) (default: 150) |
| `--whqsm-muh <WHQSM_MUH>` | WH-QSM ADMM penalty for harmonic-field splitting (muh) (default: 5) |
| `--whqsm-max-iter <WHQSM_MAX_ITER>` | WH-QSM: Maximum outer iterations (default: 150) |
| `--whqsm-tol-update <WHQSM_TOL_UPDATE>` | WH-QSM: Percent-update stopping tolerance (default: 0.1) |
| `--whqsm-tol-delta <WHQSM_TOL_DELTA>` | WH-QSM: Inner Newton stopping tolerance (default: 1e-6) |
| `--whqsm-weight <WEIGHT>` | WH-QSM data weight: flat (1 over the whole volume, WH_nlTV.m's default), flat-in-mask (1 inside the mask only), max-normalised (the magnitude over its maximum) or in-vivo (min(2 × magnitude / maximum, 1)); the last two need magnitude images (default: flat) |
| `--whqsm-te <TE>` | WH-QSM: the echo time the field is converted to phase at: first, last, spacing (TE2 - TE1), sum (of the echo times), or a time in ms (default: first) |

### `qsmxt invert hdqsm`

Hybrid Data-Fidelity QSM

```sh
qsmxt invert hdqsm [OPTIONS] --mask <MASK> --output <OUTPUT> --echo-times <ECHO_TIMES>... --magnitude <MAGNITUDE>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude of every echo (the data weight is their echo-time-weighted combination): one 4D NIfTI, or one 3D NIfTI per echo in echo order. Required. |
| `--hdqsm-alpha-l2 <HDQSM_ALPHA_L2>` | HD-QSM L2-stage TV weight (alpha_l2) (default: 3e-5) |
| `--hdqsm-mu1-l2 <MU1_L2>` | HD-QSM L2-stage gradient-consistency ADMM weight (mu1_l2), or auto for 100 × alpha_l2 (default: auto) |
| `--hdqsm-mu2 <HDQSM_MU2>` | HD-QSM fidelity consistency weight (mu2) (default: 1) |
| `--hdqsm-max-iter-l1 <HDQSM_MAX_ITER_L1>` | HD-QSM stage-1 (L1) iterations (max_iter_l1) (default: 20) |
| `--hdqsm-max-iter-l2 <HDQSM_MAX_ITER_L2>` | HD-QSM stage-2 (L2) iterations (max_iter_l2) (default: 80) |
| `--hdqsm-tol-update <HDQSM_TOL_UPDATE>` | HD-QSM stage-2 percent-update convergence tolerance (default: 0.01) |

### `qsmxt invert amp-pe`

AMP-PE (Approximate Message Passing with Parameter Estimation)

```sh
qsmxt invert amp-pe [OPTIONS] --mask <MASK> --output <OUTPUT> --b0 <B0> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI (data weight and morphology image): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. |
| `--b0 <B0>` | B0 field strength in Tesla (scales the simulated phase) Required. |
| `--amp-pe-wave-order <AMP_PE_WAVE_ORDER>` | AMP-PE: Daubechies wavelet order (1=db1, 2=db2) (default: 1) |
| `--amp-pe-nlevel <AMP_PE_NLEVEL>` | AMP-PE: Wavelet decomposition levels (default: 3) |
| `--amp-pe-wave-pec <AMP_PE_WAVE_PEC>` | AMP-PE: Morphology-mask energy retention fraction (0.0-1.0) (default: 0.85) |
| `--amp-pe-simulated-te <AMP_PE_SIMULATED_TE>` | AMP-PE simulated echo time (s) used to turn the field into phase (default: 0.008) |
| `--amp-pe-max-linearization-ite <AMP_PE_MAX_LINEARIZATION_ITE>` | AMP-PE: Linearisation iterations per stage (default: 25) |
| `--amp-pe-tikhonov-beta <AMP_PE_TIKHONOV_BETA>` | AMP-PE: L2-seed Tikhonov weight (default: 0.02) |
| `--amp-pe-damp-rate-sig <AMP_PE_DAMP_RATE_SIG>` | AMP-PE: GAMP signal-update damping rate (default: 0.01) |
| `--amp-pe-damp-rate-par <AMP_PE_DAMP_RATE_PAR>` | AMP-PE: Parameter-estimation learning rate (kappa) (default: 0.1) |
| `--amp-pe-max-pe-spar-ite <AMP_PE_MAX_PE_SPAR_ITE>` | AMP-PE: Inner sparse-reconstruction iterations (default: 5) |
| `--amp-pe-max-pe-est-ite <AMP_PE_MAX_PE_EST_ITE>` | AMP-PE: Inner parameter-estimation iterations (default: 5) |
| `--amp-pe-cvg-thd <AMP_PE_CVG_THD>` | AMP-PE: GAMP inner convergence threshold (default: 1e-6) |

### `qsmxt invert xqsm`

xQSM deep-learning dipole inversion (weights downloaded on first use)

```sh
qsmxt invert xqsm [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert qsmnet`

QSMnet deep-learning dipole inversion (weights downloaded on first use)

```sh
qsmxt invert qsmnet [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert qsmnet-plus`

QSMnet+ deep-learning dipole inversion (weights downloaded on first use)

```sh
qsmxt invert qsmnet-plus [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert qsmnetme`

QSMnetME deep-learning dipole inversion, the χ-separation toolbox's QSM network (weights downloaded on first use)

```sh
qsmxt invert qsmnetme [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert autoqsm`

AutoQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use)

```sh
qsmxt invert autoqsm [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert qsmgan`

QSMGAN deep-learning dipole inversion (weights downloaded on first use)

```sh
qsmxt invert qsmgan [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert ir2qsm`

IR2QSM deep-learning dipole inversion (weights downloaded on first use)

```sh
qsmxt invert ir2qsm [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert lpcnn`

LPCNN deep-learning dipole inversion (weights downloaded on first use)

```sh
qsmxt invert lpcnn [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert modl-qsm`

MoDL-QSM deep-learning dipole inversion → χ33/STI component (weights downloaded on first use)

```sh
qsmxt invert modl-qsm [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert nextqsm`

NeXtQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use)

```sh
qsmxt invert nextqsm [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (scan metadata) Default: `3`. |

### `qsmxt invert iqsm`

iQSM single-step reconstruction — input is every echo's WRAPPED phase (weights downloaded on first use)

```sh
qsmxt invert iqsm [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Wrapped phase NIfTI file in radians: 4D (x,y,z,echo), or 3D for one echo. Required. |
| `-m, --mask <MASK>` | Binary brain mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output NIfTI file in ppm (susceptibility for iqsm/iqsm-plus, the local field for iqfm) Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI file with the same echoes as the phase (4D), to weight the echo combination by magnitude; without it the echoes are weighted by TE² alone. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--echo-weighting <ECHO_WEIGHTING>` | Echo weighting of the multi-echo combination: `mag2-te2` weights each echo by (magnitude × TE)², the inverse of its noise variance, as in the authors' Python code; `mag-te2` by magnitude × TE², as in the authors' MATLAB code. Values: `mag-te2`, `mag2-te2`. Default: `mag2-te2`. |

### `qsmxt invert iqsm-plus`

iQSM+ orientation-adaptive single-step reconstruction — input is every echo's WRAPPED phase; echoes are combined with (magnitude × TE)² weights (weights downloaded on first use)

```sh
qsmxt invert iqsm-plus [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-times <ECHO_TIMES>... <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Wrapped phase NIfTI file in radians: 4D (x,y,z,echo), or 3D for one echo. Required. |
| `-m, --mask <MASK>` | Binary brain mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output NIfTI file in ppm (susceptibility for iqsm/iqsm-plus, the local field for iqfm) Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI file with the same echoes as the phase (4D), to weight the echo combination by magnitude; without it the echoes are weighted by TE² alone. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |

### `qsmxt invert cosmos`

COSMOS — multi-orientation least squares over 2+ co-registered orientations

```sh
qsmxt invert cosmos [OPTIONS] --input <INPUTS> --mask <MASK> --output <OUTPUT>
```

| Argument | Description |
| --- | --- |
| `-i, --input <INPUTS>` | Local field NIfTI in ppm — repeat once per orientation, all on one common grid. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction for one orientation in the common grid's voxel frame (3 values) — repeat once per --input, in the same order. Omitted, each volume's own affine is used instead, which is only valid when the orientations were separately prescribed. |
| `--b0-dirs <B0_DIRS>` | Text file of B0 directions, one "x y z" per line, in --input order. |
| `-m, --mask <MASK>` | Binary mask NIfTI file, on the common grid. Required. |
| `--force` | Reconstruct even when the B0 directions look degenerate. A degenerate set yields a plausible-looking map that is not the method you asked for; prefer fixing the metadata. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--lambda <LAMBDA>` | L2 regularization on the k-space denominator. 0 (default) is plain least squares and is right for 3+ covering orientations; 1e-3..1e-2 helps with 2, or single-axis rotations. |
| `--magnitude <MAGNITUDES>` | Magnitude NIfTI for one orientation — repeat once per --input to use the magnitude-weighted formulation (conjugate gradient) instead of the closed form. |
| `--tol <TOL>` | Conjugate-gradient tolerance (weighted formulation only) |
| `--max-iter <MAX_ITER>` | Maximum conjugate-gradient iterations (weighted formulation only) |

### `qsmxt invert sti`

STI (susceptibility tensor imaging) — 6+ co-registered orientations to a rank-2 tensor

```sh
qsmxt invert sti [OPTIONS] --input <INPUTS> --mask <MASK> --output <OUTPUT>
```

| Argument | Description |
| --- | --- |
| `-i, --input <INPUTS>` | Local field NIfTI in ppm — repeat once per orientation, all on one common grid. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction for one orientation in the common grid's voxel frame (3 values) — repeat once per --input, in the same order. Omitted, each volume's own affine is used instead, which is only valid when the orientations were separately prescribed. |
| `--b0-dirs <B0_DIRS>` | Text file of B0 directions, one "x y z" per line, in --input order. |
| `-m, --mask <MASK>` | Binary mask NIfTI file, on the common grid. Required. |
| `--force` | Reconstruct even when the B0 directions look degenerate. A degenerate set yields a plausible-looking map that is not the method you asked for; prefer fixing the metadata. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-sti_Chimap.nii (the 4D tensor, components 11 12 13 22 23 33), {prefix}_desc-mms_Chimap.nii, {prefix}_desc-msa_Chimap.nii and {prefix}_desc-sti_evec.nii (the 4D principal eigenvector) — the pipeline's names. Required. |
| `--lambda <LAMBDA>` | Ridge term on the 6x6 normal equations at each k-point. Raise when orientations are few, clustered or near-coplanar. |

<!-- generated:cli-invert-subcommands end -->

## `qsmxt separate`

<!-- generated:cli-separate start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Susceptibility source separation

| Subcommand | Description |
| --- | --- |
| [`r2star-qsm`](#qsmxt-separate-r2star-qsm) | R2\*-QSM closed-form (Dimov 2022) — QSM + R2\* (GRE-only) |
| [`decompose`](#qsmxt-separate-decompose) | DECOMPOSE-QSM signal-domain fit (Chen 2021) — a QSM per echo + multi-echo magnitude (GRE-only) |
| [`chi-sep-ilsqr`](#qsmxt-separate-chi-sep-ilsqr) | χ-separation iLSQR (Shin 2021) — local field + R2' + QSM |
| [`chi-sep-medi`](#qsmxt-separate-chi-sep-medi) | χ-separation MEDI — local field + R2' + multi-echo magnitude + QSM |
| [`wavesep`](#qsmxt-separate-wavesep) | WaveSep wavelet-L1 (Fang 2023) — QSM + R2' |
| [`hc-chisep`](#qsmxt-separate-hc-chisep) | Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude |
| [`susep-net`](#qsmxt-separate-susep-net) | SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use) |
| [`chi-sepnet`](#qsmxt-separate-chi-sepnet) | χ-sepnet deep-learning separation — local field + R2' or R2\* (weights downloaded on first use) |

<!-- generated:cli-separate end -->

Split a susceptibility map into paramagnetic and diamagnetic sources, writing
`<prefix>_desc-paramagnetic_Chimap.nii`, `_desc-diamagnetic_Chimap.nii` and
`_desc-total_Chimap.nii` (the pipeline's names):

What each method takes:

| Subcommand | Method and inputs |
| --- | --- |
| `r2star-qsm` | R2\*-QSM closed form — QSM + R2\* |
| `decompose` | DECOMPOSE-QSM — a QSM per echo (`--qsm-per-echo`) + multi-echo magnitude. `qsmxt run --qsm-per-echo true` gives it each echo's map; without it, its one QSM at every echo |
| `chi-sep-ilsqr` | χ-separation, iLSQR — local field + R2′ + QSM (`qsmxt run` reconstructs this QSM from the local field by iLSQR; here it is the one given) |
| `chi-sep-medi` | χ-separation, MEDI — local field + R2′ + multi-echo magnitude (`--magnitude`, `--echo-times`) + QSM |
| `wavesep` | WaveSep — QSM + R2′ |
| `hc-chisep` | Hollow-cylinder χ-separation — QSM + R2′ + multi-echo magnitude |
| `susep-net` | SUSEP-Net (deep learning) — QSM + R2′ + local field |
| `chi-sepnet` | χ-sepnet (deep learning) — local field + R2′ or R2* (the authors' route; `--chi-sepnet-route native` takes a QSM too) |

Every method that uses the field strength requires `--field-strength`: all but `wavesep`
and `susep-net`, and `chi-sepnet` only with `--r2star` (R2PRIMEnet checks it against its 3 T
training).

<!-- generated:cli-separate-subcommands start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

### `qsmxt separate r2star-qsm`

R2\*-QSM closed-form (Dimov 2022) — QSM + R2\* (GRE-only)

```sh
qsmxt separate r2star-qsm [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> <--r2star <R2STAR>|--magnitude <MAGNITUDE>...>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--r2star <R2STAR>` | R2\* map NIfTI in Hz (else fit from --magnitude + --echo-times) |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude: one 4D file or several 3D files (space-separated). Fits R2\* if --r2star absent. |
| `--echo-times <ECHO_TIMES>` | GRE echo times in seconds (space- or comma-separated), required with --magnitude. |
| `--r2star-qsm-r-const-3t <R2STAR_QSM_R_CONST_3T>` | R2\*-QSM relaxometric constant at 3T (Hz/ppm) (default: 274) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |

### `qsmxt separate decompose`

DECOMPOSE-QSM signal-domain fit (Chen 2021) — a QSM per echo + multi-echo magnitude (GRE-only)

```sh
qsmxt separate decompose [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --magnitude <MAGNITUDE>... --qsm-per-echo <QSM_PER_ECHO>... --echo-times <ECHO_TIMES>... --field-strength <FIELD_STRENGTH>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude: one 4D file or several 3D files (space-separated) Required. |
| `--qsm-per-echo <QSM_PER_ECHO>` | QSM of each echo (ppm), which sets each echo's phase: one 4D file or one 3D file per echo (space-separated; the same file may be given for every echo) Required. |
| `--echo-times <ECHO_TIMES>` | GRE echo times in seconds (space- or comma-separated) Required. |
| `--decompose-n-inner <DECOMPOSE_N_INNER>` | DECOMPOSE inner alternating passes per voxel (default: 10) |
| `--decompose-chi-bound <BOUND>` | DECOMPOSE bound on \|χ\| in the fit: a number in ppm (default: 0.5), or echo-spacing to compute it from the field strength and echo spacing. |
| `--decompose-unwrap-phase <BOOL>` | DECOMPOSE: unwrap the phase along the echoes (default: false, as in the authors' code) |
| `--decompose-fit <DECOMPOSE_FIT>` | DECOMPOSE's fit: `reference-code` (Tim Ho's script; default) or `paper` (Chen et al. 2021: amplitudes summing to one, per-voxel S(0) = 1 scale), as the DECOMPOSE pipeline runs it. Values: `reference-code`, `paper`. |
| `--decompose-max-iterations <DECOMPOSE_MAX_ITERATIONS>` | DECOMPOSE: maximum solver iterations of each fit (lsqcurvefit MaxIterations; default: 400) |
| `--decompose-max-function-evaluations <DECOMPOSE_MAX_FUNCTION_EVALUATIONS>` | DECOMPOSE: maximum function evaluations of each fit (lsqcurvefit MaxFunctionEvaluations; default: 100 × the fit's parameters) |
| `--decompose-function-tolerance <DECOMPOSE_FUNCTION_TOLERANCE>` | DECOMPOSE: relative cost change each fit stops at (lsqcurvefit FunctionTolerance; default: 1e-6) |
| `--decompose-step-tolerance <DECOMPOSE_STEP_TOLERANCE>` | DECOMPOSE: relative step each fit stops at (lsqcurvefit StepTolerance; default: 1e-6) |
| `--decompose-optimality-tolerance <DECOMPOSE_OPTIMALITY_TOLERANCE>` | DECOMPOSE: first-order optimality each fit stops at (lsqcurvefit OptimalityTolerance; default: 1e-6) |
| `--decompose-finite-difference-step <DECOMPOSE_FINITE_DIFFERENCE_STEP>` | DECOMPOSE: relative finite-difference step of the Jacobian (lsqcurvefit FiniteDifferenceStepSize; default: √eps) |
| `--decompose-typical-x <DECOMPOSE_TYPICAL_X>` | DECOMPOSE: typical magnitude of each fitted value (lsqcurvefit TypicalX; default: 1) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |

### `qsmxt separate chi-sep-ilsqr`

χ-separation iLSQR (Shin 2021) — local field + R2' + QSM

```sh
qsmxt separate chi-sep-ilsqr [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --local-field <LOCAL_FIELD> --r2prime <R2PRIME> --field-strength <FIELD_STRENGTH>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--chi-sep-ilsqr-dr-pos <HZ_PER_PPM>` | χ-sep iLSQR paramagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-ilsqr-dr-neg <HZ_PER_PPM>` | χ-sep iLSQR diamagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-ilsqr-pad <X> <Y> <Z>` | χ-sep iLSQR zero-padding at both ends of each axis in voxels, X Y Z (default: 16 16 16) |
| `--chi-sep-ilsqr-cg-max-iter-lsqr <CHI_SEP_ILSQR_CG_MAX_ITER_LSQR>` | χ-sep iLSQR: maximum CG iterations of the LSQR step (default: 50) |
| `--chi-sep-ilsqr-cg-tol-lsqr <CHI_SEP_ILSQR_CG_TOL_LSQR>` | χ-sep iLSQR: CG relative tolerance of the LSQR step (default: 0.02) |
| `--chi-sep-ilsqr-cg-max-iter-sa <CHI_SEP_ILSQR_CG_MAX_ITER_SA>` | χ-sep iLSQR: maximum CG iterations of the streaking-removal step (default: 50) |
| `--chi-sep-ilsqr-cg-tol-sa <CHI_SEP_ILSQR_CG_TOL_SA>` | χ-sep iLSQR: CG relative tolerance of the streaking-removal step (default: 0.02) |
| `--chi-sep-ilsqr-d2-thr <CHI_SEP_ILSQR_D2_THR>` | χ-sep iLSQR: the streaking cone is where \|D\| is below this (default: 0.11) |
| `--chi-sep-ilsqr-prc-w-i-min <CHI_SEP_ILSQR_PRC_W_I_MIN>` | χ-sep iLSQR: percentile of the field's Laplacian at which the LSQR weight is 1 (default: 60) |
| `--chi-sep-ilsqr-prc-w-i-max <CHI_SEP_ILSQR_PRC_W_I_MAX>` | χ-sep iLSQR: percentile of the field's Laplacian at which the LSQR weight is 0 (default: 99.8) |
| `--chi-sep-ilsqr-prc-w-g-min <CHI_SEP_ILSQR_PRC_W_G_MIN>` | χ-sep iLSQR: percentile of each gradient's magnitude at which the streaking-removal weight is 1 (default: 50) |
| `--chi-sep-ilsqr-prc-w-g-max <CHI_SEP_ILSQR_PRC_W_G_MAX>` | χ-sep iLSQR: percentile of each gradient's magnitude at which the streaking-removal weight is 0 (default: 70) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |

### `qsmxt separate chi-sep-medi`

χ-separation MEDI — local field + R2' + multi-echo magnitude + QSM

```sh
qsmxt separate chi-sep-medi [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --local-field <LOCAL_FIELD> --r2prime <R2PRIME> --magnitude <MAGNITUDE>... --echo-times <ECHO_TIMES>... --field-strength <FIELD_STRENGTH>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude: one 4D file or one 3D file per echo (space-separated) Required. |
| `--echo-times <ECHO_TIMES>` | GRE echo times in seconds (space- or comma-separated) Required. |
| `--csf-mask <FILE\|none>` | CSF mask NIfTI for the CSF-uniformity term, or `none` for no CSF term (the toolbox's GUI without a CSF mask) (default: the CSF found from R2\* of the echoes, as the toolbox's script does; see --csf-r2star) |
| `--chi-sep-medi-lambda <CHI_SEP_MEDI_LAMBDA>` | χ-sep MEDI weight of the data terms against the L1 terms (default: 1) |
| `--chi-sep-medi-lambda-csf <CHI_SEP_MEDI_LAMBDA_CSF>` | χ-sep MEDI weight of the CSF-uniformity term (default: 1) |
| `--chi-sep-medi-dr-pos <HZ_PER_PPM>` | χ-sep MEDI paramagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-medi-dr-neg <HZ_PER_PPM>` | χ-sep MEDI diamagnetic relaxometric constant (Hz/ppm), or auto for 137 × B0 / 2.9 T (default: auto) |
| `--chi-sep-medi-pad <CHI_SEP_MEDI_PAD>` | χ-sep MEDI zero-padding at both ends of the third axis, in voxels (default: 5) |
| `--chi-sep-medi-tol <CHI_SEP_MEDI_TOL>` | χ-sep MEDI: stop when the relative change falls below this (default: 0.03) |
| `--chi-sep-medi-max-iter <CHI_SEP_MEDI_MAX_ITER>` | χ-sep MEDI maximum Gauss-Newton iterations (default: 30) |
| `--chi-sep-medi-cg-tol <CHI_SEP_MEDI_CG_TOL>` | χ-sep MEDI inner CG tolerance (default: 0.05) |
| `--chi-sep-medi-cg-max-iter <CHI_SEP_MEDI_CG_MAX_ITER>` | χ-sep MEDI inner CG max iterations (default: 100) |
| `--chi-sep-medi-csf-r2star <METHOD>` | χ-sep MEDI: the R2\* its CSF mask is extracted from: arlo (MEDI's ARLO; the default) or nnls (the χ-separation toolbox's r2star_nnls, which its script runs) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |

### `qsmxt separate wavesep`

WaveSep wavelet-L1 (Fang 2023) — QSM + R2'

```sh
qsmxt separate wavesep [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --r2prime <R2PRIME>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--wavesep-dr-pos <WAVESEP_DR_POS>` | WaveSep paramagnetic relaxivity (Hz/ppm) (default: 137) |
| `--wavesep-dr-neg <WAVESEP_DR_NEG>` | WaveSep diamagnetic relaxivity (Hz/ppm) (default: 137) |
| `--wavesep-alpha <WAVESEP_ALPHA>` | WaveSep: Proximal-gradient step size (default: 0.2) |
| `--wavesep-lambda <WAVESEP_LAMBDA>` | WaveSep: Wavelet L1 soft-threshold weight (default: 0.02) |
| `--wavesep-wavelet-order <WAVESEP_WAVELET_ORDER>` | WaveSep: Daubechies order (default: 4) |
| `--wavesep-max-iter <WAVESEP_MAX_ITER>` | WaveSep: ISTA max iterations (default: 100) |
| `--wavesep-tol <WAVESEP_TOL>` | WaveSep: Relative-change stop tolerance (default: 0.001) |

### `qsmxt separate hc-chisep`

Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude

```sh
qsmxt separate hc-chisep [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --r2prime <R2PRIME> --magnitude <MAGNITUDE>... --echo-times <ECHO_TIMES>... --field-strength <FIELD_STRENGTH>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude: one 4D file or several 3D files (space-separated) Required. |
| `--echo-times <ECHO_TIMES>` | GRE echo times in seconds (space- or comma-separated) Required. |
| `--se-magnitude <SE_MAGNITUDE>` | Multi-echo spin-echo magnitude: one 4D file or several 3D files (optional) |
| `--se-echo-times <SE_ECHO_TIMES>` | Spin-echo times in seconds (comma-separated) for --se-magnitude. |
| `--hc-chisep-dr-pos-3t <HC_CHISEP_DR_POS_3T>` | HC-ChiSep paramagnetic relaxivity at 3T (Hz/ppm) (default: 137) |
| `--hc-chisep-bin-hz <HC_CHISEP_BIN_HZ>` | HC-ChiSep R2' bin width for the anchored grid search (Hz) (default: 0.25) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |

### `qsmxt separate susep-net`

SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use)

```sh
qsmxt separate susep-net [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --local-field <LOCAL_FIELD> --r2prime <R2PRIME>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--susep-net-patch <D> <H> <W>` | Run SUSEP-Net as a sliding window of patches this size in voxels, D H W, each a multiple of 8, so its memory follows the patch, not the volume. Omit to run the whole volume at once. |

### `qsmxt separate chi-sepnet`

χ-sepnet deep-learning separation — local field + R2' or R2\* (weights downloaded on first use)

```sh
qsmxt separate chi-sepnet [OPTIONS] --mask <MASK> --output <OUTPUT> --local-field <LOCAL_FIELD> <--r2prime <R2PRIME>|--r2star <R2STAR>>
```

| Argument | Description |
| --- | --- |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla; required with --r2star (R2PRIMEnet refuses a field strength other than 3 T unless --r2primenet-ignore-field-mismatch is given). χ-sepnet itself does not use it. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz (χ-sepnet-R2') |
| `--r2star <R2STAR>` | R2\* map NIfTI in Hz (χ-sepnet-R2\*: R2' predicted by R2PRIMEnet; `--route toolbox` only). Needs --field-strength. |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm; `--route native` only (the toolbox route makes its own with QSMnetME) |
| `--chi-sepnet-route <CHI_SEPNET_ROUTE>` | χ-sepnet's input route. `toolbox` (default), the authors': the inputs resampled to 1 mm with B0 along z, the susceptibility channel computed by QSMnetME, R2' measured or predicted from R2\* by R2PRIMEnet, the maps resampled back. `native`: the network alone on the acquired grid, given this run's QSM and R2'. Values: `toolbox`, `native`. |
| `--chi-sepnet-patch <D> <H> <W>` | χ-sepnet (`--chi-sepnet-route native`) sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--chi-sepnet-frame-patch <D> <H> <W>` | χ-sepnet (`--chi-sepnet-route toolbox`): run the networks as a sliding window of patches this size over the 1 mm frame, D H W, each a multiple of 16, so memory follows the patch. Omit to run the whole frame at once, as the authors do. |
| `--r2primenet-ignore-field-mismatch` | Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its weights were trained at; the predicted R2' is then uncalibrated. Without it, an explicit `--r2prime-strategy r2primenet` refuses a non-3 T run and `auto` skips R2' with a warning. |

<!-- generated:cli-separate-subcommands end -->

## `qsmxt qsmart`

<!-- generated:cli-qsmart start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

QSMART vessel-aware reconstruction: total field -&gt; susceptibility

```sh
qsmxt qsmart [OPTIONS] --mask <MASK> --output <OUTPUT> --field-strength <FIELD_STRENGTH> --echo-time <ECHO_TIME> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file (ppm) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-time <ECHO_TIME>` | Echo time in seconds. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI (optional; drives vasculature detection). With --qsmart-vessel-magnitude mean-n4 (the default) every echo's magnitude (4D), averaged and N4 bias-corrected; with `combined`, a combined magnitude (3D), used as it is. |
| `--reliability <RELIABILITY>` | QSMART's reliability map from its echo fit (`qsmxt fieldmap laplacian --b0-estimation qsmart-echofit --reliability-output`): both stages are reconstructed inside it. |
| `--intermediates <DIR>` | Also write QSMART's intermediate maps into this directory, as &lt;name&gt;.nii: vessel_mask (1 = vessel), localfield_stage1, chimap_stage1, localfield_stage2 and chimap_stage2 (ppm), and vessel_magnitude when QSMART makes its own (mean-n4) |
| `--ilsqr-precision <PRECISION>` | Arithmetic of the inner iLSQR's two LSQR solves (`--qsmart-inversion ilsqr`): double, or single as STI Suite's QSM_iLSQR, which QSMART calls (default: double; `qsmxt run --ilsqr-precision`) |
| `--qsmart-inversion <QSMART_INVERSION>` | QSMART inner dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `star`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `qsmnetme`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--qsmart-ilsqr-max-iter <QSMART_ILSQR_MAX_ITER>` | QSMART iLSQR max iterations per stage. The iLSQR tolerance is --ilsqr-tol (default: 50) |
| `--qsmart-ilsqr-pad-mm <MM>` | Zero-padding of QSMART's inner iLSQR solves, in mm per side (default: 6) |
| `--qsmart-sdf-sigma1-stage1 <QSMART_SDF_SIGMA1_STAGE1>` | QSMART SDF sigma1, stage 1 (voxels) (default: 10) |
| `--qsmart-sdf-sigma2-stage1 <QSMART_SDF_SIGMA2_STAGE1>` | QSMART SDF sigma2, stage 1 (voxels) (default: 10) |
| `--qsmart-sdf-sigma1-stage2 <QSMART_SDF_SIGMA1_STAGE2>` | QSMART SDF sigma1, stage 2 (voxels) (default: 8) |
| `--qsmart-sdf-sigma2-stage2 <QSMART_SDF_SIGMA2_STAGE2>` | QSMART SDF sigma2, stage 2 (voxels) (default: 2) |
| `--qsmart-sdf-lower-lim <QSMART_SDF_LOWER_LIM>` | QSMART: SDF proximity lower limit (default: 0.6) |
| `--qsmart-sdf-curv-constant <QSMART_SDF_CURV_CONSTANT>` | QSMART: SDF curvature constant (default: 500) |
| `--qsmart-vessel-magnitude <QSMART_VESSEL_MAGNITUDE>` | The magnitude QSMART detects vessels on: `mean-n4` (the mean over echoes, N4 bias-corrected, as QSMART prepares it; default) or `combined` (the root sum of squares over echoes, inhomogeneity-corrected unless --no-inhomogeneity-correction) Values: `mean-n4`, `combined`. |
| `--qsmart-offset-grid <QSMART_OFFSET_GRID>` | The dipole grid of QSMART's offset adjustment: `fft` (default) or `qsmart-reference` (`adjust_offset.m`'s own, half a step off the FFT grid on odd matrix sizes, as the QSMART pipeline runs it) Values: `fft`, `qsmart-reference`. |
| `--qsmart-vessel-units <QSMART_VESSEL_UNITS>` | Units of QSMART's vessel sizes (--qsmart-vasc-sphere-radius and the Frangi scales): `voxels` (default, as QSMART gives them) or `mm` (converted to voxels with the mean voxel size) Values: `voxels`, `mm`. |
| `--qsmart-vasc-sphere-radius <QSMART_VASC_SPHERE_RADIUS>` | QSMART vasculature detection sphere radius, in --qsmart-vessel-units (default: 8) |
| `--qsmart-frangi-scale-min <QSMART_FRANGI_SCALE_MIN>` | QSMART Frangi min vessel radius, in --qsmart-vessel-units (default: 0.5) |
| `--qsmart-frangi-scale-max <QSMART_FRANGI_SCALE_MAX>` | QSMART Frangi max vessel radius, in --qsmart-vessel-units (default: 6) |
| `--qsmart-frangi-scale-ratio <QSMART_FRANGI_SCALE_RATIO>` | QSMART Frangi scale step, in --qsmart-vessel-units (default: 0.5) |
| `--qsmart-frangi-c-mode <QSMART_FRANGI_C_MODE>` | How QSMART's Frangi C is set: `absolute` (--qsmart-frangi-c, in magnitude units; default, as QSMART gives it) or `relative` (--qsmart-frangi-c-factor × the vessel magnitude's median inside the brain mask, the same on every scanner's intensity scale) Values: `absolute`, `relative`. |
| `--qsmart-frangi-c <QSMART_FRANGI_C>` | QSMART Frangi C noise threshold, in the vessel magnitude's intensity units (default 500, QSMART v1.0's); used with --qsmart-frangi-c-mode absolute. |
| `--qsmart-frangi-c-factor <QSMART_FRANGI_C_FACTOR>` | QSMART's relative Frangi C: this factor × the vessel magnitude's median inside the brain mask (default 0.25); used with --qsmart-frangi-c-mode relative. |

<!-- generated:cli-qsmart end -->

`qsmxt qsmart` takes the total field and runs QSMART's own two-stage reconstruction. To prepare its inputs as QSMART's authors do, map
the field with `qsmxt fieldmap laplacian --laplacian-solver mirror --laplacian-unmasked
--b0-estimation qsmart-echofit --reliability-output r0.nii`, then pass
`--reliability r0.nii` and every echo's magnitude (4D) as `--magnitude`. `--intermediates DIR` also
writes QSMART's own maps into `DIR` as `<name>.nii`: `vessel_mask` (1 = vessel),
`localfield_stage1`, `chimap_stage1`, `localfield_stage2`, `chimap_stage2`, and
`vessel_magnitude` when QSMART makes its own from the echoes.
`--qsmart-offset-grid qsmart-reference` combines the two stages on the k-space grid of the
authors' code instead of the FFT grid; the two differ only when a matrix dimension is odd, and
the option is there for comparing with their code. `--ilsqr-precision single` runs the inner
iLSQR's two LSQR solves in single precision, as STI Suite's `QSM_iLSQR`, which QSMART calls,
does (`--pipeline qsmart` does the same).

## `qsmxt swi`

<!-- generated:cli-swi start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Susceptibility-weighted imaging

```sh
qsmxt swi [OPTIONS] --phase <PHASE>... --magnitude <MAGNITUDE>... --output <OUTPUT>
```

| Argument | Description |
| --- | --- |
| `--phase <PHASE>` | Wrapped phase NIfTI files, one per echo, in echo order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI files, one per echo, in the same order as --phase. Required. |
| `--tes <TES>` | Echo times in seconds, one per echo (read from the phase JSON sidecars when omitted; not needed for a single echo) |
| `-m, --mask <MASK>` | Mask the phase is filtered within (default: CLEAR-SWI's own, from the first echo's magnitude) |
| `-o, --output <OUTPUT>` | Output SWI NIfTI file. Required. |
| `--mip` | Also compute minimum intensity projection. |
| `--mip-output <MIP_OUTPUT>` | Output path for MIP (default: the --output name with its `swi` suffix replaced by `minIP`, or `_minIP` appended — `sub-01_swi.nii` → `sub-01_minIP.nii`, as the pipeline names them) |
| `--filtered-phase` | Also write CLEAR-SWI's high-pass filtered phase, the phase the SWI is weighted by: Hz for several echoes, radians for one, 0 outside the mask (`qsmxt run --swi-filtered-phase`) |
| `--filtered-phase-output <FILTERED_PHASE_OUTPUT>` | Output path for the filtered phase (default: the --output name with its `swi` suffix replaced by `desc-swi_phase`, or `_desc-swi_phase` appended — `sub-01_swi.nii` → `sub-01_desc-swi_phase.nii`, as the pipeline names it). Implies --filtered-phase. |
| `--swi-unwrapping <SWI_UNWRAPPING>` | SWI phase unwrapping before the echoes are combined [default: laplacian]. Values: `laplacian`, `laplacian-slice`, `romeo`. |
| `--swi-scaling <SWI_SCALING>` | SWI phase-mask scaling (default: tanh) Values: `tanh`, `negative-tanh`, `positive`, `negative`, `triangular`. |
| `--swi-strength <SWI_STRENGTH>` | SWI: Phase scaling strength (higher = stronger phase contrast) (default: 4) |
| `--swi-hp-sigma <SWI_HP_SIGMA>` | SWI high-pass filter sigma (3 values, in voxels) (default: 4 4 0) |
| `--swi-mip-window <SWI_MIP_WINDOW>` | SWI: Minimum-intensity-projection window, in slices (default: 7) |

<!-- generated:cli-swi end -->

## `qsmxt segment`

<!-- generated:cli-segment start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Whole-brain segmentation of a magnitude image with SynthSeg

```sh
qsmxt segment [OPTIONS] --output <OUTPUT> <MAGNITUDE>
```

| Argument | Description |
| --- | --- |
| `<MAGNITUDE>` | Input magnitude NIfTI file. Not a susceptibility map: SynthSeg reads anatomy, and χ has none of the tissue contrast it was trained to recognise. Required. |
| `-o, --output <OUTPUT>` | Output label NIfTI file (FreeSurfer label ids) Required. |
| `--lookup <LOOKUP>` | Also write the BIDS label lookup table (index/name) to this path. |
| `--use-custom-dseg <TOOL>` | Prefer a bring-your-own segmentation from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ when present. Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg. |
| `--synthseg-version <VERSION>` | SynthSeg weights generation: v1 (SynthSeg 1.0, 32 labels; the default) or v2 (SynthSeg 2.0, 33 labels, adds a general CSF class; Billot et al., PNAS 2023) |
| `--synthseg-crop <SYNTHSEG_CROP>` | Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32). Omit to process the whole volume, as the SynthSeg CLI does. |
| `--synthseg-patch <VOXELS>` | Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the volume. Omit to run the whole volume at once, as the SynthSeg CLI does. |
| `--no-synthseg-flip-averaging` | Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does. |
| `--no-synthseg-topology-cleanup` | Skip resetting each topological class to its largest connected component. |
| `--synthseg-sigma <SYNTHSEG_SIGMA>` | Gaussian blur on the posteriors, in voxels (0 disables it) (default: 0.5) |

<!-- generated:cli-segment end -->

## `qsmxt r2star`

<!-- generated:cli-r2star start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

R2\* mapping from multi-echo magnitude data

```sh
qsmxt r2star --mask <MASK> --output <OUTPUT> --echo-times <ECHO_TIMES> <ECHO_TIMES> <ECHO_TIMES>... <INPUTS>...
```

| Argument | Description |
| --- | --- |
| `<INPUTS>` | Multi-echo magnitude: one 4D file or several 3D files (3+ echoes; space-separated) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output R2\* map NIfTI file. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds (must match number of inputs) Required. |

<!-- generated:cli-r2star end -->

## `qsmxt t2star`

<!-- generated:cli-t2star start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

T2\* mapping from multi-echo magnitude data

```sh
qsmxt t2star --mask <MASK> --output <OUTPUT> --echo-times <ECHO_TIMES> <ECHO_TIMES> <ECHO_TIMES>... <INPUTS>...
```

| Argument | Description |
| --- | --- |
| `<INPUTS>` | Multi-echo magnitude: one 4D file or several 3D files (3+ echoes; space-separated) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output T2\* map NIfTI file. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds (must match number of inputs) Required. |

<!-- generated:cli-t2star end -->

## `qsmxt r2`

<!-- generated:cli-r2 start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

R2 mapping from multi-echo spin-echo (MESE) magnitude data (EPG)

```sh
qsmxt r2 [OPTIONS] --mask <MASK> --output <OUTPUT> --echo-times <ECHO_TIMES>... <INPUTS>...
```

| Argument | Description |
| --- | --- |
| `<INPUTS>` | Multi-echo spin-echo (MESE) magnitude: one 4D file or several 3D files (space-separated) Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output R2 map NIfTI file. Required. |
| `--echo-times <ECHO_TIMES>` | Spin-echo times in seconds (space- or comma-separated) Required. |
| `--t1 <T1>` | Assumed T1 in seconds (EPG dictionary) |
| `--b1-map <B1_MAP>` | Optional B1 map NIfTI (refocusing efficiency); fitted from the dictionary if absent. |

<!-- generated:cli-r2 end -->

## `qsmxt r2prime`

<!-- generated:cli-r2prime start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

R2' mapping: measured as R2\* - R2, or estimated from R2\* by R2PRIMEnet

```sh
qsmxt r2prime [OPTIONS] --r2star <R2STAR> --mask <MASK> --output <OUTPUT>
```

| Argument | Description |
| --- | --- |
| `--r2star <R2STAR>` | R2\* map NIfTI in Hz. Required. |
| `--r2 <R2>` | R2 map NIfTI in Hz: R2' is measured, as R2\* - R2. |
| `--r2primenet` | Estimate R2' from the R2\* map with R2PRIMEnet (deep learning, trained at 3 T; weights downloaded on first use) instead of measuring it from an R2 map. Needs --field-strength. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `--r2-coverage <R2_COVERAGE>` | Binary mask of where the R2 acquisition reached, if it does not cover the whole mask (a MESE slab). Without it, coverage is inferred from R2 &gt; 0. |
| `-o, --output <OUTPUT>` | Output R2' map NIfTI file. Required. |
| `--output-coverage <OUTPUT_COVERAGE>` | Also write the mask of voxels where R2' is a measurement. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla (--r2primenet: R2PRIMEnet refuses a field strength other than 3 T unless --r2primenet-ignore-field-mismatch is given) |
| `--r2primenet-ignore-field-mismatch` | Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its weights were trained at. The R2' is then uncalibrated. |
| `--r2primenet-patch <D> <H> <W>` | R2PRIMEnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128) |

<!-- generated:cli-r2prime end -->

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

Without an R2 map, `--r2primenet` estimates R2' from R2\* with R2PRIMEnet, as
`qsmxt run --r2prime-strategy r2primenet` does. It needs `--field-strength`: the
network is trained at 3 T and refuses another field strength unless
`--r2primenet-ignore-field-mismatch` is given. An estimate is not a measurement.

```sh
qsmxt r2prime --r2star r2star.nii --r2primenet --field-strength 3 -m mask.nii -o r2prime.nii
```

## `qsmxt homogeneity`

<!-- generated:cli-homogeneity start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Inhomogeneity correction on magnitude data (makehomogeneous or N4)

```sh
qsmxt homogeneity [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input magnitude NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output corrected magnitude NIfTI file. Required. |
| `--inhomogeneity-method <INHOMOGENEITY_METHOD>` | Inhomogeneity correction method: `makehomogeneous` (default; --homogeneity-\*) or `n4` (N4 bias field correction, as ANTs' N4BiasFieldCorrection; --n4-\*) Values: `makehomogeneous`, `n4`. |
| `--homogeneity-sigma-mm <HOMOGENEITY_SIGMA_MM>` | Inhomogeneity correction smoothing sigma in mm (makehomogeneous) (default: 7) |
| `--homogeneity-nbox <HOMOGENEITY_NBOX>` | makehomogeneous: boxes per dimension for the tissue segmentation the receive sensitivity is estimated from (default: 15) |
| `--n4-shrink-factor <N4_SHRINK_FACTOR>` | N4: Subsample the image by this factor before fitting the bias field (default: 4) |
| `--n4-max-iterations <ITERATIONS>` | N4: maximum iterations at each fitting level, one number a level (default 50 50 50 50); the B-spline mesh doubles from one level to the next. |
| `--n4-convergence-threshold <N4_CONVERGENCE_THRESHOLD>` | N4: stop a level once the change in the bias field (its coefficient of variation) falls to this (default 0: every iteration runs) |
| `--n4-mesh <X> <Y> <Z>` | N4: B-spline mesh elements per axis at the first level (x y z) (default: 1 1 1) |
| `--n4-spline-order <N4_SPLINE_ORDER>` | N4: order of the B-spline bias field, 1 to 3 (default: 3) |
| `--n4-bias-field-fwhm <N4_BIAS_FIELD_FWHM>` | N4: FWHM of the Gaussian modelling the bias field's blurring of the log-intensity histogram (default: 0.15) |
| `--n4-wiener-noise <N4_WIENER_NOISE>` | N4: Noise term of the Wiener deconvolution of the histogram (default: 0.01) |
| `--n4-histogram-bins <N4_HISTOGRAM_BINS>` | N4: Bins of the log-intensity histogram (default: 200) |
| `--n4-geometry <GEOMETRY>` | N4: the image geometry it is given: `scanner` (the scan's header, as ANTs reads the acquired NIfTI; the default) or `make-nii` (the axis-aligned header MATLAB's make_nii writes, as QSMART runs N4). N4's shrunk grid falls where the header puts it. |

<!-- generated:cli-homogeneity end -->

## `qsmxt resample`

<!-- generated:cli-resample start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Resample oblique volume to axial orientation

```sh
qsmxt resample [OPTIONS] [INPUT]
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file. Treated as continuous data (magnitude, an unwrapped field map, chi). For wrapped phase pass --phase instead, with its magnitude, so the interpolation can go through the complex domain and survive the wraps. |
| `-o, --output <OUTPUT>` | Output resampled NIfTI file (for the positional input) |
| `--phase <PHASE>` | Wrapped phase to resample together with --magnitude (complex-domain interpolation) |
| `--magnitude <MAGNITUDE>` | Magnitude paired with --phase. |
| `--phase-out <PHASE_OUT>` | Where to write the resampled phase (default: &lt;phase stem&gt;_axial.nii) |
| `--magnitude-out <MAGNITUDE_OUT>` | Where to write the resampled magnitude (default: &lt;magnitude stem&gt;_axial.nii) |
| `--noise-fill` | Fill the empty corners of the new grid with seeded noise, as QSMxT 8.x did. Only the --phase / --magnitude pair can be noise-filled; by default the corners are zero. |

<!-- generated:cli-resample end -->

Wrapped phase needs `--phase` with `--magnitude`, so the interpolation goes through the complex domain; continuous data (magnitude, an unwrapped field, χ) is a positional argument.

```sh
# continuous data
qsmxt resample mag.nii -o mag_axial.nii

# wrapped phase, with its magnitude, interpolated as mag·e^{iφ}
qsmxt resample --phase phase.nii --magnitude mag.nii \
  --phase-out phase_axial.nii --magnitude-out mag_axial.nii
```

## `qsmxt quality-map`

<!-- generated:cli-quality-map start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Compute ROMEO phase quality map

```sh
qsmxt quality-map [OPTIONS] --output <OUTPUT> <PHASE>
```

| Argument | Description |
| --- | --- |
| `<PHASE>` | Input phase NIfTI file (first echo) Required. |
| `-o, --output <OUTPUT>` | Output quality map NIfTI file. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude image (improves quality estimation) |
| `--phase2 <PHASE2>` | Second echo phase image (improves quality estimation) |
| `--te1 <TE1>` | Echo time of first phase in seconds. Default: `0.02`. |
| `--te2 <TE2>` | Echo time of second phase in seconds (if --phase2 provided) Default: `0.04`. |

<!-- generated:cli-quality-map end -->

## `qsmxt example`

<!-- generated:cli-example start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Download an example BIDS dataset to try the pipeline on

```sh
qsmxt example [OPTIONS] [OUTPUT_DIR]
```

| Argument | Description |
| --- | --- |
| `<OUTPUT_DIR>` | Directory to create or add the BIDS dataset in [default: qsmxt-example]. |
| `-n, --name <NAME>` | Which acquisition to fetch; repeat to add several to the one dataset [default: prisma-bridge-run1]. |
| `--list` | List the available acquisitions and exit. |
| `--force` | Rewrite acquisitions that are already present in the dataset. |

<!-- generated:cli-example end -->

That fetches `prisma-bridge-run1` (~103 MB): one subject, 5-echo 1 mm 3D
gradient-echo, acquired on a Siemens MAGNETOM Prisma Fit. The archive is cached
under `~/.cache/qsmxt/examples` (override with `$QSMXT_EXAMPLE_CACHE`), so
fetching it again into another directory costs nothing.

If OSF is unreachable from your network, set `$QSMXT_EXAMPLE_BASE_URL` to a mirror
holding the archives as `<id>.zip` — checksums are still enforced, so a mirror
serving the wrong bytes is rejected exactly as a corrupted download would be.

The data comes from a QSM harmonization acquisition in which the same subject was
scanned on two Siemens 3T scanners under four protocols, three runs each. List
everything available with:

```sh
qsmxt example --list
```

Because every acquisition is the same subject, several can share one dataset —
they differ only in their BIDS `ses-` (scanner), `acq-` (protocol) and `run-`
entities. Name more than one, or point a later call at an existing dataset to add
to it:

```sh
qsmxt example --name prisma-bridge-run1 --name cima-bridge-run1 study/bids
qsmxt example --name prisma-bridge-run2 study/bids   # adds a third to the same dataset
```

Acquisitions already in the dataset are left alone; pass `--force` to rewrite
them. In the TUI: **Input mode → Example dataset**.

## `qsmxt tui`

<!-- generated:cli-tui start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Launch interactive TUI for pipeline configuration

```sh
qsmxt tui
```

<!-- generated:cli-tui end -->

## `qsmxt update`

<!-- generated:cli-update start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Check for updates and optionally install the latest version

```sh
qsmxt update [OPTIONS]
```

| Argument | Description |
| --- | --- |
| `-y, --yes` | Update without prompting for confirmation. |
| `--force` | Replace a development build (version 0.0.0) with the latest release. Without it, `qsmxt update` refuses on a dev build, since every release would look newer. |
| `--main` | Build the latest commit on the main branch from source and install it in place of this qsmxt. Needs the Rust toolchain (https://rustup.rs). The build is kept in ~/.qsmxt/build, so the next update rebuilds only what changed. `qsmxt update --force` goes back to the latest release. |

<!-- generated:cli-update end -->

See [Update QSMxT](/QSMxT/how-to/update/).

## `qsmxt news`

<!-- generated:cli-news start (from the qsmxt binary's argument definitions: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference) -->

Show the announcements from the QSMxT team, newest first

```sh
qsmxt news
```

<!-- generated:cli-news end -->

See [Update QSMxT](/QSMxT/how-to/update/#announcements).
