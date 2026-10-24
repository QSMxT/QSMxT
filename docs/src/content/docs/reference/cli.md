---
title: CLI reference
description: Every qsmxt command, option, accepted value and default, generated from the binary.
---

<!-- Generated from qsmxt's own argument definitions by src/cli_reference.rs. Do not edit:
     run `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference` to regenerate. -->

This page is generated from the `qsmxt` binary's argument definitions, so it lists exactly
the commands and options the binary accepts. `qsmxt <command> --help` prints the same
text. For what the options *do*, see [Commands](/QSMxT/reference/commands/),
[Algorithms](/QSMxT/reference/algorithms/) and [Standalone tools](/QSMxT/reference/tools/).

## Commands

- [`qsmxt run`](#qsmxt-run)
- [`qsmxt init`](#qsmxt-init)
- [`qsmxt validate`](#qsmxt-validate)
- [`qsmxt dicom-convert`](#qsmxt-dicom-convert)
- [`qsmxt slurm`](#qsmxt-slurm)
- [`qsmxt aggregate`](#qsmxt-aggregate)
- [`qsmxt mask`](#qsmxt-mask)
- [`qsmxt combine`](#qsmxt-combine)
- [`qsmxt unwrap`](#qsmxt-unwrap)
- [`qsmxt fieldmap`](#qsmxt-fieldmap)
- [`qsmxt bgremove`](#qsmxt-bgremove)
- [`qsmxt invert`](#qsmxt-invert)
- [`qsmxt separate`](#qsmxt-separate)
- [`qsmxt qsmart`](#qsmxt-qsmart)
- [`qsmxt swi`](#qsmxt-swi)
- [`qsmxt segment`](#qsmxt-segment)
- [`qsmxt r2star`](#qsmxt-r2star)
- [`qsmxt t2star`](#qsmxt-t2star)
- [`qsmxt r2`](#qsmxt-r2)
- [`qsmxt r2prime`](#qsmxt-r2prime)
- [`qsmxt homogeneity`](#qsmxt-homogeneity)
- [`qsmxt resample`](#qsmxt-resample)
- [`qsmxt quality-map`](#qsmxt-quality-map)
- [`qsmxt example`](#qsmxt-example)
- [`qsmxt tui`](#qsmxt-tui)
- [`qsmxt update`](#qsmxt-update)

## `qsmxt run`

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
| `--qsm-algorithm <QSM_ALGORITHM>` | QSM dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--orientation-group <PATTERN>` | Group runs into multi-orientation sets and reconstruct each with COSMOS or STI. Takes an entity name (acq, run, rec, inv), a glob over the run key (`*acq-dir*`), or `re:` and a regex with one capture group. Orientations are co-registered if they need it. |
| `--no-orientation-registration` | Do not co-register the orientations in a group — use this when they were already registered externally and a second interpolation is not wanted. A group that still needs registration is then refused instead of reconstructed. |
| `--multi-orientation-algorithm <MULTI_ORIENTATION_ALGORITHM>` | Which reconstruction to run over each orientation group. Values: `cosmos`, `sti`. |
| `--multi-orientation-lambda <MULTI_ORIENTATION_LAMBDA>` | Regularization for the multi-orientation reconstruction (COSMOS k-space L2, or STI ridge) |
| `--multi-orientation-force` | Reconstruct an orientation group even when its B0 directions look degenerate. |
| `--cosmos-magnitude-weighting` | COSMOS: weight each orientation by its combined magnitude (conjugate-gradient solve) |
| `--cosmos-tol <COSMOS_TOL>` | Magnitude-weighted COSMOS: conjugate-gradient tolerance (needs --cosmos-magnitude-weighting) |
| `--cosmos-max-iter <COSMOS_MAX_ITER>` | Magnitude-weighted COSMOS: conjugate-gradient iteration cap (needs --cosmos-magnitude-weighting) |
| `--unwrapping-algorithm <UNWRAPPING_ALGORITHM>` | Unwrapping algorithm. Values: `romeo`, `laplacian`. |
| `--bf-algorithm <BF_ALGORITHM>` | Background field removal algorithm (default: vsharp; pdf with --qsm-algorithm medi, as in its authors' pipeline) Values: `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella`, `bfrnet`, `iqfm`. |
| `--masking-input <MASKING_INPUT>` | Masking input image (overrides the input of the configured mask sections; combine with --mask-preset, e.g. --mask-preset robust-threshold --masking-input magnitude) Values: `magnitude-first`, `magnitude`, `magnitude-last`, `phase-quality`. |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Enable phase offset removal for multi-echo data (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--coil-combination-sigma <COIL_COMBINATION_SIGMA>` | MCPC-3D-S coil combination: smoothing sigma for the per-coil phase offsets (3 values, in voxels; default 10 10 5, as in MriResearchTools). Only used for uncombined multi-coil runs. |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method. Values: `weighted-avg`, `linear-fit`, `robust-fit`. |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO. Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--laplacian-solver <LAPLACIAN_SOLVER>` | Poisson solver for `--unwrapping-algorithm laplacian`: `dct` (unweighted least squares, Neumann/DCT solve; default) or `fft` (Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume) Values: `dct`, `fft`. |
| `--laplacian-fft-pad <VOXELS>` | Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or three (x y z). Default 64. |
| `--bet-fractional-intensity <BET_FRACTIONAL_INTENSITY>` | BET fractional intensity (0.0-1.0; smaller = larger brain). Applies to every BET step in the masking recipe, overriding a `bet:<f>` in a --mask spec. |
| `--bet-smoothness <BET_SMOOTHNESS>` | BET surface smoothness factor. Applies to every BET step in the masking recipe. |
| `--bet-gradient-threshold <BET_GRADIENT_THRESHOLD>` | BET gradient threshold (-1 to 1). Applies to every BET step in the masking recipe. |
| `--bet-iterations <BET_ITERATIONS>` | BET surface evolution iterations. Applies to every BET step in the masking recipe. |
| `--bet-subdivisions <BET_SUBDIVISIONS>` | BET icosphere subdivision level. Applies to every BET step in the masking recipe. |
| `--bet-voxel-scale <FACTOR>` | Factor applied to the voxel sizes before BET runs. BET's surface model is tuned to a human brain in millimetres, so preclinical data has to be presented at human scale: 10 is the usual setting for mouse. 1 (the default) is the acquisition's own geometry. Applies to every BET step in the masking recipe. |
| `--qsm-reference <SPEC>` | What the susceptibility map's zero is pinned to: `mean` (the mean over the voxels the map is defined on — the brain mask less the rim background-field removal eroded; the default), `none`, or a SynthSeg region — a structure with both sides merged (`thalamus`), one side (`left-thalamus`), the `ventricles` composite, a raw FreeSurfer id (`4`), or a comma-separated list of any of those. A region implies --do-segmentation. An unrecognised name lists the valid ones. |
| `--rts-delta <RTS_DELTA>` | RTS delta parameter. |
| `--rts-mu <RTS_MU>` | RTS mu parameter. |
| `--rts-tol <RTS_TOL>` | RTS tolerance. |
| `--rts-rho <RTS_RHO>` | RTS rho (ADMM penalty) |
| `--rts-max-iter <RTS_MAX_ITER>` | RTS max iterations. |
| `--rts-lsmr-iter <RTS_LSMR_ITER>` | RTS LSMR iterations. |
| `--tv-lambda <TV_LAMBDA>` | TV lambda parameter. |
| `--tv-rho <TV_RHO>` | TV rho (ADMM penalty; default: 100 × lambda) |
| `--tv-tol <TV_TOL>` | TV tolerance. |
| `--tv-max-iter <TV_MAX_ITER>` | TV max iterations. |
| `--tkd-threshold <TKD_THRESHOLD>` | TKD threshold. |
| `--tsvd-threshold <TSVD_THRESHOLD>` | TSVD threshold. |
| `--tgv-iterations <TGV_ITERATIONS>` | TGV iterations (default: from the voxel size, as the reference does) |
| `--tgv-erosions <TGV_EROSIONS>` | TGV erosions. |
| `--tgv-alpha1 <TGV_ALPHA1>` | TGV alpha1 (first-order weight), in radians at the scan's first echo time and field strength (the reference's units) |
| `--tgv-alpha0 <TGV_ALPHA0>` | TGV alpha0 (second-order weight), in radians at the scan's first echo time and field strength (the reference's units) |
| `--tgv-step-size <TGV_STEP_SIZE>` | TGV primal step size multiplier. |
| `--tikhonov-lambda <TIKHONOV_LAMBDA>` | Tikhonov lambda. |
| `--nltv-lambda <NLTV_LAMBDA>` | NLTV lambda. |
| `--nltv-mu <NLTV_MU>` | NLTV mu (penalty parameter) |
| `--nltv-tol <NLTV_TOL>` | NLTV tolerance. |
| `--nltv-max-iter <NLTV_MAX_ITER>` | NLTV max iterations. |
| `--nltv-newton-iter <NLTV_NEWTON_ITER>` | NLTV maximum Newton iterations of the data-term subproblem. |
| `--medi-lambda <MEDI_LAMBDA>` | MEDI lambda: the data-fidelity weight (larger fits the field more closely) |
| `--medi-merit <MEDI_MERIT>` | MEDI: enable MERIT weighting. |
| `--medi-smv` | MEDI: high-pass the local field with a spherical mean value filter, removing residual background near the brain edge (MEDI-SMV; on by default) |
| `--no-medi-smv` | MEDI: no SMV high-pass of the local field. |
| `--medi-smv-radius <MEDI_SMV_RADIUS>` | MEDI SMV radius in mm (also mSMV's) |
| `--medi-smv-shrink-mask` | MEDI-SMV: shrink the mask to where the SMV of the mask exceeds 0.999 (on by default) |
| `--no-medi-smv-shrink-mask` | MEDI-SMV: keep the full mask, high-passing the field over it. |
| `--medi-msmv` | MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value before MEDI, which fits it with the SMV-filtered dipole kernel (it implies the SMV). Needs at least 3 echoes. |
| `--medi-msmv-maxk <MEDI_MSMV_MAXK>` | MEDI-mSMV: the boundary correction makes at most this many passes, less one. |
| `--medi-msmv-vessel-radius <MEDI_MSMV_VESSEL_RADIUS>` | MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 × the largest voxel dimension if that is more) |
| `--no-medi-msmv-prefilter` | MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges (expert) |
| `--medi-data-weighting <MEDI_DATA_WEIGHTING>` | MEDI: data weighting mode (0=uniform, 1=SNR) |
| `--medi-percentage <MEDI_PERCENTAGE>` | MEDI edge percentage (0.0-1.0) |
| `--medi-cg-tol <MEDI_CG_TOL>` | MEDI CG tolerance. |
| `--medi-cg-max-iter <MEDI_CG_MAX_ITER>` | MEDI CG max iterations. |
| `--medi-max-iter <MEDI_MAX_ITER>` | MEDI max outer iterations. |
| `--medi-tol <MEDI_TOL>` | MEDI outer tolerance. |
| `--tfi-lambda <TFI_LAMBDA>` | TFI lambda: the data-fidelity weight (larger fits the field more closely) |
| `--tfi-precond <TFI_PRECOND>` | TFI preconditioner value (susceptibility scaling outside the mask) |
| `--tfi-data-weighting <TFI_DATA_WEIGHTING>` | TFI: data weighting mode (0=uniform, 1=SNR) |
| `--tfi-percentage <TFI_PERCENTAGE>` | TFI edge percentage (0.0-1.0) |
| `--tfi-cg-tol <TFI_CG_TOL>` | TFI CG tolerance. |
| `--tfi-cg-max-iter <TFI_CG_MAX_ITER>` | TFI CG max iterations. |
| `--tfi-max-iter <TFI_MAX_ITER>` | TFI max outer iterations. |
| `--tfi-tol <TFI_TOL>` | TFI outer tolerance. |
| `--ilsqr-tol <ILSQR_TOL>` | iLSQR tolerance of the streaking-artefact solve. |
| `--ilsqr-max-iter <ILSQR_MAX_ITER>` | iLSQR max iterations of each LSQR solve. |
| `--ilsqr-pad-mm <MM>` | iLSQR zero-padding in mm per side. |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance. |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR max iterations. |
| `--heidi-cone-threshold <HEIDI_CONE_THRESHOLD>` | \|D(k)\| above which a coefficient is well-conditioned and kept from the LSQR seed. |
| `--heidi-gradient-threshold <HEIDI_GRADIENT_THRESHOLD>` | Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous. |
| `--no-heidi-laplacian-correction` | Do not open the homogeneity masks where the field's Laplacian is large. |
| `--heidi-laplacian-threshold <HEIDI_LAPLACIAN_THRESHOLD>` | Field-Laplacian threshold (ppm per voxel²) |
| `--heidi-gradient-mask-floor <HEIDI_GRADIENT_MASK_FLOOR>` | TV weight kept where every direction reads as an edge (0 reproduces upstream's shipped binarised masks) |
| `--heidi-continuation-steps <HEIDI_CONTINUATION_STEPS>` | NESTA continuation steps on μ. |
| `--heidi-inner-iterations <HEIDI_INNER_ITERATIONS>` | Accelerated-gradient iterations per continuation step. |
| `--heidi-mu-min <HEIDI_MU_MIN>` | Final (smallest) smoothing parameter μ. |
| `--heidi-tol <HEIDI_TOL>` | Relative objective change that ends an inner loop early (the default runs every iteration) |
| `--no-heidi-denoise` | Skip the anisotropic-diffusion smoothing of the field before its gradients are taken. |
| `--heidi-denoise-iterations <HEIDI_DENOISE_ITERATIONS>` | Anisotropic-diffusion iterations. |
| `--heidi-denoise-time-step <HEIDI_DENOISE_TIME_STEP>` | Anisotropic-diffusion time step. |
| `--heidi-denoise-conductance <HEIDI_DENOISE_CONDUCTANCE>` | Anisotropic-diffusion conductance. |
| `--qsmart-ilsqr-max-iter <QSMART_ILSQR_MAX_ITER>` | QSMART iLSQR max iterations per stage. The iLSQR tolerance is --ilsqr-tol. |
| `--qsmart-ilsqr-pad-mm <MM>` | Zero-padding of QSMART's inner iLSQR solves, in mm per side (default: 6) |
| `--qsmart-vasc-sphere-radius <QSMART_VASC_SPHERE_RADIUS>` | QSMART vasculature detection sphere radius. |
| `--qsmart-inversion <QSMART_INVERSION>` | QSMART inner dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--qsmart-sdf-sigma1-stage1 <QSMART_SDF_SIGMA1_STAGE1>` | QSMART SDF sigma1, stage 1 (voxels) |
| `--qsmart-sdf-sigma2-stage1 <QSMART_SDF_SIGMA2_STAGE1>` | QSMART SDF sigma2, stage 1 (voxels) |
| `--qsmart-sdf-sigma1-stage2 <QSMART_SDF_SIGMA1_STAGE2>` | QSMART SDF sigma1, stage 2 (voxels) |
| `--qsmart-sdf-sigma2-stage2 <QSMART_SDF_SIGMA2_STAGE2>` | QSMART SDF sigma2, stage 2 (voxels) |
| `--qsmart-sdf-lower-lim <QSMART_SDF_LOWER_LIM>` | QSMART SDF proximity lower limit. |
| `--qsmart-sdf-curv-constant <QSMART_SDF_CURV_CONSTANT>` | QSMART SDF curvature constant. |
| `--qsmart-frangi-scale-min <QSMART_FRANGI_SCALE_MIN>` | QSMART Frangi min vessel radius (mm) |
| `--qsmart-frangi-scale-max <QSMART_FRANGI_SCALE_MAX>` | QSMART Frangi max vessel radius (mm) |
| `--qsmart-frangi-scale-ratio <QSMART_FRANGI_SCALE_RATIO>` | QSMART Frangi scale step (mm) |
| `--qsmart-frangi-c <QSMART_FRANGI_C>` | QSMART Frangi C noise threshold. |
| `--ndi-tau <NDI_TAU>` | NDI gradient-descent step size (tau) |
| `--ndi-alpha <NDI_ALPHA>` | NDI L2 regularization weight (alpha) |
| `--ndi-max-iter <NDI_MAX_ITER>` | NDI max iterations. |
| `--ndi-auto-stop <NDI_AUTO_STOP>` | NDI: stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto); --ndi-max-iter is then the maximum. |
| `--fansi-alpha1 <FANSI_ALPHA1>` | FANSI first-order (TV/TGV) L1 penalty weight (alpha1) |
| `--fansi-mu1 <FANSI_MU1>` | FANSI gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--fansi-mu2 <FANSI_MU2>` | FANSI fidelity-consistency ADMM weight (mu2) |
| `--fansi-alpha0 <FANSI_ALPHA0>` | FANSI second-order L1 penalty weight (alpha0, nlTGV only; default: 2 × alpha1) |
| `--fansi-mu0 <FANSI_MU0>` | FANSI second-order consistency ADMM weight (mu0, nlTGV only; default: 2 × mu1) |
| `--fansi-max-iter <FANSI_MAX_ITER>` | FANSI outer ADMM iterations. |
| `--fansi-tol-update <FANSI_TOL_UPDATE>` | FANSI percent-update convergence tolerance. |
| `--fansi-tol-delta <FANSI_TOL_DELTA>` | FANSI inner Newton convergence tolerance. |
| `--l1qsm-alpha1 <L1QSM_ALPHA1>` | L1-QSM gradient (TV) L1 penalty weight (alpha1) |
| `--l1qsm-mu1 <L1QSM_MU1>` | L1-QSM gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--l1qsm-mu2 <L1QSM_MU2>` | L1-QSM fidelity-consistency ADMM weight (mu2) |
| `--l1qsm-mu3 <L1QSM_MU3>` | L1-QSM L1 proximal ADMM weight (mu3) |
| `--l1qsm-lambda <L1QSM_LAMBDA>` | L1-QSM L1 fidelity strength (lambda) |
| `--l1qsm-max-iter <L1QSM_MAX_ITER>` | L1-QSM outer ADMM iterations. |
| `--l1qsm-tol-update <L1QSM_TOL_UPDATE>` | L1-QSM percent-update convergence tolerance. |
| `--l1qsm-tol-delta <L1QSM_TOL_DELTA>` | L1-QSM inner Newton convergence tolerance. |
| `--whqsm-alpha1 <WHQSM_ALPHA1>` | WH-QSM TV regularization weight (alpha1) |
| `--whqsm-mu1 <WHQSM_MU1>` | WH-QSM ADMM penalty for TV splitting (mu1) |
| `--whqsm-mu2 <WHQSM_MU2>` | WH-QSM ADMM penalty for data-fidelity splitting (mu2) |
| `--whqsm-beta <WHQSM_BETA>` | WH-QSM weak-harmonic ROI penalty (beta) |
| `--whqsm-muh <WHQSM_MUH>` | WH-QSM ADMM penalty for harmonic-field splitting (muh) |
| `--whqsm-max-iter <WHQSM_MAX_ITER>` | WH-QSM max outer iterations. |
| `--whqsm-tol-update <WHQSM_TOL_UPDATE>` | WH-QSM percent-update convergence tolerance. |
| `--whqsm-tol-delta <WHQSM_TOL_DELTA>` | WH-QSM inner Newton convergence tolerance. |
| `--hdqsm-alpha-l2 <HDQSM_ALPHA_L2>` | HD-QSM L2-stage TV weight (alpha_l2) |
| `--hdqsm-mu1-l2 <HDQSM_MU1_L2>` | HD-QSM L2-stage gradient-consistency ADMM weight (mu1_l2; default: 100 × alpha_l2) |
| `--hdqsm-mu2 <HDQSM_MU2>` | HD-QSM fidelity consistency weight (mu2) |
| `--hdqsm-max-iter-l1 <HDQSM_MAX_ITER_L1>` | HD-QSM stage-1 (L1) iterations (max_iter_l1) |
| `--hdqsm-max-iter-l2 <HDQSM_MAX_ITER_L2>` | HD-QSM stage-2 (L2) iterations (max_iter_l2) |
| `--hdqsm-tol-update <HDQSM_TOL_UPDATE>` | HD-QSM stage-2 percent-update convergence tolerance. |
| `--amp-pe-wave-order <AMP_PE_WAVE_ORDER>` | AMP-PE Daubechies wavelet order (1=db1, 2=db2) |
| `--amp-pe-nlevel <AMP_PE_NLEVEL>` | AMP-PE wavelet decomposition levels. |
| `--amp-pe-wave-pec <AMP_PE_WAVE_PEC>` | AMP-PE morphology-mask energy retention fraction (0.0-1.0) |
| `--amp-pe-simulated-te <AMP_PE_SIMULATED_TE>` | AMP-PE simulated echo time (s) used to turn the field into phase. |
| `--amp-pe-max-linearization-ite <AMP_PE_MAX_LINEARIZATION_ITE>` | AMP-PE linearization iterations per stage. |
| `--amp-pe-damp-rate-sig <AMP_PE_DAMP_RATE_SIG>` | AMP-PE GAMP signal-update damping rate. |
| `--amp-pe-damp-rate-par <AMP_PE_DAMP_RATE_PAR>` | AMP-PE parameter-estimation learning rate (kappa) |
| `--amp-pe-max-pe-spar-ite <AMP_PE_MAX_PE_SPAR_ITE>` | AMP-PE inner sparse-reconstruction iterations. |
| `--amp-pe-max-pe-est-ite <AMP_PE_MAX_PE_EST_ITE>` | AMP-PE inner parameter-estimation iterations. |
| `--amp-pe-cvg-thd <AMP_PE_CVG_THD>` | AMP-PE GAMP inner convergence threshold. |
| `--amp-pe-tikhonov-beta <AMP_PE_TIKHONOV_BETA>` | AMP-PE L2-seed Tikhonov weight. |
| `--r2star-qsm-r-const-3t <R2STAR_QSM_R_CONST_3T>` | R2\*-QSM relaxometric constant at 3T (Hz/ppm) |
| `--decompose-n-inner <DECOMPOSE_N_INNER>` | DECOMPOSE inner alternating passes per voxel. |
| `--decompose-chi-bound <DECOMPOSE_CHI_BOUND>` | DECOMPOSE upper bound on \|χ\| in the fit (ppm) |
| `--decompose-max-lm-iter <DECOMPOSE_MAX_LM_ITER>` | DECOMPOSE Levenberg–Marquardt max iterations. |
| `--chi-sep-ilsqr-dr-pos <CHI_SEP_ILSQR_DR_POS>` | χ-sep iLSQR paramagnetic relaxometric constant (Hz/ppm) |
| `--chi-sep-ilsqr-dr-neg <CHI_SEP_ILSQR_DR_NEG>` | χ-sep iLSQR diamagnetic relaxometric constant (Hz/ppm) |
| `--chi-sep-ilsqr-lambda1 <CHI_SEP_ILSQR_LAMBDA1>` | χ-sep iLSQR L1 edge-masked TV weight. |
| `--chi-sep-ilsqr-percentage <CHI_SEP_ILSQR_PERCENTAGE>` | χ-sep iLSQR edge-mask keep fraction (0-1) |
| `--chi-sep-ilsqr-r2p-min <CHI_SEP_ILSQR_R2P_MIN>` | χ-sep iLSQR R2' reliability window lower (Hz) |
| `--chi-sep-ilsqr-r2p-max <CHI_SEP_ILSQR_R2P_MAX>` | χ-sep iLSQR R2' reliability window upper (Hz) |
| `--chi-sep-ilsqr-max-iter <CHI_SEP_ILSQR_MAX_ITER>` | χ-sep iLSQR outer Gauss-Newton iterations. |
| `--chi-sep-ilsqr-tol <CHI_SEP_ILSQR_TOL>` | χ-sep iLSQR outer relative-change tolerance. |
| `--chi-sep-ilsqr-cg-max-iter <CHI_SEP_ILSQR_CG_MAX_ITER>` | χ-sep iLSQR inner CG max iterations. |
| `--chi-sep-ilsqr-cg-tol <CHI_SEP_ILSQR_CG_TOL>` | χ-sep iLSQR inner CG relative tolerance. |
| `--chi-sep-medi-lambda-para <CHI_SEP_MEDI_LAMBDA_PARA>` | χ-sep MEDI paramagnetic L1 weight. |
| `--chi-sep-medi-lambda-dia <CHI_SEP_MEDI_LAMBDA_DIA>` | χ-sep MEDI diamagnetic L1 weight. |
| `--chi-sep-medi-lambda-cpl <CHI_SEP_MEDI_LAMBDA_CPL>` | χ-sep MEDI field/R2' coupling weight. |
| `--chi-sep-medi-dr-pos <CHI_SEP_MEDI_DR_POS>` | χ-sep MEDI paramagnetic relaxivity (Hz/ppm) |
| `--chi-sep-medi-dr-neg <CHI_SEP_MEDI_DR_NEG>` | χ-sep MEDI diamagnetic relaxivity (Hz/ppm) |
| `--chi-sep-medi-percentage <CHI_SEP_MEDI_PERCENTAGE>` | χ-sep MEDI edge-mask percentage (0-1) |
| `--chi-sep-medi-cg-tol <CHI_SEP_MEDI_CG_TOL>` | χ-sep MEDI inner CG tolerance. |
| `--chi-sep-medi-cg-max-iter <CHI_SEP_MEDI_CG_MAX_ITER>` | χ-sep MEDI inner CG max iterations. |
| `--chi-sep-medi-max-iter <CHI_SEP_MEDI_MAX_ITER>` | χ-sep MEDI outer Gauss-Newton iterations. |
| `--chi-sep-medi-tol <CHI_SEP_MEDI_TOL>` | χ-sep MEDI outer convergence tolerance. |
| `--wavesep-dr-pos <WAVESEP_DR_POS>` | WaveSep paramagnetic relaxivity (Hz/ppm) |
| `--wavesep-dr-neg <WAVESEP_DR_NEG>` | WaveSep diamagnetic relaxivity (Hz/ppm) |
| `--wavesep-alpha <WAVESEP_ALPHA>` | WaveSep proximal-gradient step size. |
| `--wavesep-lambda <WAVESEP_LAMBDA>` | WaveSep wavelet L1 soft-threshold weight. |
| `--wavesep-wavelet-order <WAVESEP_WAVELET_ORDER>` | WaveSep Daubechies wavelet order. |
| `--wavesep-max-iter <WAVESEP_MAX_ITER>` | WaveSep ISTA max iterations. |
| `--wavesep-tol <WAVESEP_TOL>` | WaveSep relative-change stop tolerance. |
| `--hc-chisep-dr-pos-3t <HC_CHISEP_DR_POS_3T>` | HC-ChiSep paramagnetic relaxivity at 3T (Hz/ppm) |
| `--hc-chisep-bin-hz <HC_CHISEP_BIN_HZ>` | HC-ChiSep R2' bin width for the anchored grid search (Hz) |
| `--chi-sepnet-patch <D> <H> <W>` | χ-sepnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--susep-net-patch <D> <H> <W>` | Run SUSEP-Net as a sliding window of patches this size in voxels, D H W, each a multiple of 8, so its memory follows the patch, not the volume. Omit to run the whole volume at once. |
| `--motion-correction` | Rigidly co-register every echo onto a reference echo (estimated on magnitude, applied to phase through the complex signal) before field mapping, and restrict the mask to voxels every echo covers. Off by default. Needs magnitude images. |
| `--motion-reference <first\|last\|N>` | Echo the others are aligned onto: `first` (default), `last`, or a 1-based echo number. |
| `--no-motion-chain` | Register every echo from the identity instead of seeding it from its neighbour's solution. |
| `--motion-chain-levels <MOTION_CHAIN_LEVELS>` | Pyramid levels for a seeded (chained) registration (default 3) |
| `--motion-min-ncc <MOTION_MIN_NCC>` | NCC below which an echo's registration is reported as suspect (default 0.3, set for echoes; ~0.9 suits repeats) |
| `--registration-levels <REGISTRATION_LEVELS>` | Registration pyramid levels: the capture range; do not lower (default 5) |
| `--registration-max-samples <REGISTRATION_MAX_SAMPLES>` | Registration: voxels sampled per metric evaluation, per level (default 200000) |
| `--registration-max-iterations <REGISTRATION_MAX_ITERATIONS>` | Registration: Powell cycles per level (default 20) |
| `--registration-max-evaluations <REGISTRATION_MAX_EVALUATIONS>` | Registration: metric evaluations across all levels, a hard stop (default 5000) |
| `--registration-tolerance <REGISTRATION_TOLERANCE>` | Registration: relative cost improvement per cycle that counts as converged (default 1e-5) |
| `--registration-rotation-step-deg <REGISTRATION_ROTATION_STEP_DEG>` | Registration: initial rotation step at the finest level, degrees (default 3) |
| `--registration-translation-step-mm <REGISTRATION_TRANSLATION_STEP_MM>` | Registration: initial translation step at the finest level, mm (default 3) |
| `--registration-min-overlap <REGISTRATION_MIN_OVERLAP>` | Registration: smallest fraction of sampled voxels that must overlap (default 0.25) |
| `--vsharp-deconvolve <BOOL>` | Undo V-SHARP's SMV high-pass with a truncated-SVD deconvolution (default: false) |
| `--vsharp-threshold <VSHARP_THRESHOLD>` | V-SHARP deconvolution (TSVD) threshold; only used with --vsharp-deconvolve true. |
| `--vsharp-max-radius <VSHARP_MAX_RADIUS>` | V-SHARP max kernel radius in mm. |
| `--vsharp-min-radius <VSHARP_MIN_RADIUS>` | V-SHARP min kernel radius in mm. |
| `--vsharp-grid <VSHARP_GRID>` | Grid V-SHARP estimates the background on: `native` (default), `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened), or `isotropic` (spacing from --vsharp-grid-spacing). The local field keeps native resolution. Values: `native`, `finest`, `isotropic`. |
| `--vsharp-grid-spacing <MM>` | V-SHARP isotropic working-grid spacing in mm (default 1); used with --vsharp-grid isotropic. |
| `--pdf-tol <PDF_TOL>` | PDF tolerance, relative to the noise level. |
| `--lbv-tol <LBV_TOL>` | LBV tolerance. |
| `--ismv-tol <ISMV_TOL>` | iSMV tolerance. |
| `--ismv-max-iter <ISMV_MAX_ITER>` | iSMV max iterations. |
| `--ismv-radius <ISMV_RADIUS>` | iSMV kernel radius in mm. |
| `--sharp-threshold <SHARP_THRESHOLD>` | SHARP threshold. |
| `--sharp-radius <SHARP_RADIUS>` | SHARP kernel radius in mm. |
| `--resharp-radius <RESHARP_RADIUS>` | RESHARP SMV kernel radius in mm. |
| `--resharp-tik-reg <RESHARP_TIK_REG>` | RESHARP Tikhonov regularization parameter. |
| `--resharp-tol <RESHARP_TOL>` | RESHARP CG convergence tolerance. |
| `--resharp-max-iter <RESHARP_MAX_ITER>` | RESHARP maximum CG iterations. |
| `--harperella-radius <HARPERELLA_RADIUS>` | HARPERELLA SMV kernel radius in mm. |
| `--harperella-max-iter <HARPERELLA_MAX_ITER>` | HARPERELLA maximum iterations. |
| `--harperella-tol <HARPERELLA_TOL>` | HARPERELLA convergence tolerance. |
| `--iharperella-max-iter <IHARPERELLA_MAX_ITER>` | iHARPERELLA maximum iterations. |
| `--iharperella-tol <IHARPERELLA_TOL>` | iHARPERELLA convergence tolerance. |
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) |
| `--romeo-individual` | ROMEO: use individual per-echo unwrapping (default) |
| `--no-romeo-individual` | ROMEO: use template-based temporal unwrapping (disables individual mode) |
| `--romeo-correct-global` | ROMEO: enable inter-echo 2π offset correction (default: off) |
| `--no-romeo-correct-global` | ROMEO: disable inter-echo 2π offset correction. |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0.5) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: maximum number of seed regions (default: 1) |
| `--romeo-merge-regions` | ROMEO: merge neighboring regions after unwrapping (default: off) |
| `--romeo-correct-regions` | ROMEO: correct each region's median to nearest 0 (default: off) |
| `--romeo-wrap-addition <ROMEO_WRAP_ADDITION>` | ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0) |
| `--swi-unwrapping <SWI_UNWRAPPING>` | SWI phase unwrapping before the echoes are combined [default: laplacian]. Values: `laplacian`, `laplacian-slice`, `romeo`. |
| `--swi-hp-sigma <SWI_HP_SIGMA>` | SWI high-pass filter sigma (3 values, in voxels) |
| `--swi-scaling <SWI_SCALING>` | SWI phase-mask scaling. Values: `tanh`, `negative-tanh`, `positive`, `negative`, `triangular`. |
| `--swi-strength <SWI_STRENGTH>` | SWI phase scaling strength. |
| `--swi-mip-window <SWI_MIP_WINDOW>` | SWI MIP window size in slices. |
| `--smwi-threshold <SMWI_THRESHOLD>` | SMWI susceptibility threshold in ppm — the \|χ\| at which the weighting mask reaches zero. The 1 ppm default follows SEPIA and targets strong sources; brain tissue wants far less. |
| `--smwi-power <SMWI_POWER>` | SMWI mask power (contrast strength) |
| `--smwi-mip-window <SMWI_MIP_WINDOW>` | SMWI mIP window size in slices. |
| `--synthseg-version <VERSION>` | SynthSeg weights generation: v1 (32 labels, ships with SynthSeg) or v2 (33 labels, adds CSF; weights not published yet, so refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR) |
| `--synthseg-crop <SYNTHSEG_CROP>` | Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32). Omit to process the whole volume, as the SynthSeg CLI does. |
| `--synthseg-patch <VOXELS>` | Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the volume. Omit to run the whole volume at once, as the SynthSeg CLI does. |
| `--no-synthseg-flip-averaging` | Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does. |
| `--no-synthseg-topology-cleanup` | Skip resetting each topological class to its largest connected component. |
| `--synthseg-sigma <SYNTHSEG_SIGMA>` | Gaussian blur on the posteriors, in voxels (0 disables it) |
| `--use-custom-dseg <TOOL>` | Prefer a bring-your-own segmentation from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ when present. Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--homogeneity-sigma-mm <HOMOGENEITY_SIGMA_MM>` | Inhomogeneity correction smoothing sigma in mm. |
| `--homogeneity-nbox <HOMOGENEITY_NBOX>` | Inhomogeneity correction box filter passes. |
| `--linear-fit-reliability-threshold <LINEAR_FIT_RELIABILITY_THRESHOLD>` | Linear and robust fit: percentile for the fit's reliability mask (default 90; 0 disables) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--no-qsm` | Skip QSM processing (only run supplementary outputs like SWI, T2\*, R2\*) |
| `--do-swi` | Also compute SWI. |
| `--do-segmentation` | Also segment the brain from the GRE magnitude with SynthSeg, writing a FreeSurfer-labelled `_dseg.nii` and its BIDS lookup table. Needs no T1w and no registration. Needs a deep-learning build. doi:10.1016/j.media.2023.102789. |
| `--do-analysis` | Also summarise every quantitative map (χ, χ+/χ−, T2\*, R2\*, R2, R2') per segmented structure into a TSV: mean, SD, median, min, max and the 5th/95th percentiles, one row per map and structure, plus a dataset-level table gathering all runs. Summarises whatever the run made — it implies --do-segmentation, and QSM only if no other map was asked for. |
| `--do-smwi` | Also compute SMWI (susceptibility map-weighted imaging): weights the magnitude by a mask built from the susceptibility map rather than from high-pass filtered phase, so the contrast stays where the source is. Needs χ, so it implies QSM. doi:10.1002/mrm.24920. |
| `--do-t2starmap` | Compute T2\* relaxation map from multi-echo magnitude data. |
| `--do-r2starmap` | Compute R2\* decay rate map from multi-echo magnitude data. |
| `--do-r2map` | Compute R2 map from a multi-echo spin-echo (MESE) acquisition (EPG) |
| `--do-r2primemap` | Compute R2' map (= R2\* − R2; needs GRE magnitude + a MESE acquisition) |
| `--do-chisep` | Compute chi-separation (paramagnetic/diamagnetic susceptibility maps) |
| `--chisep <CHI_SEPARATION_ALGORITHM>` | Chi-separation method (default: r2star-qsm) Values: `r2star-qsm`, `decompose`, `chi-sep-ilsqr`, `chi-sep-medi`, `wavesep`, `hc-chisep`, `susep-net`, `chi-sepnet`. |
| `--use-custom-qsm <TOOL>` | Use a bring-your-own QSM (Chimap) from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--use-custom-r2 <TOOL>` | Use a bring-your-own R2 map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--use-custom-r2prime <TOOL>` | Use a bring-your-own R2' map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--r2prime-strategy <R2PRIME_STRATEGY>` | How to obtain R2' when no custom map is supplied. `auto` (default) measures it as R2\* - R2 from a MESE acquisition when one is present and predicts it from R2\* with R2PRIMEnet when it is not; `mese` only ever measures it, producing nothing without spin-echo data; `r2primenet` always predicts it. A measurement and an estimate are not the same thing — the generated methods text says which route was configured. Values: `auto`, `mese`, `r2primenet`. |
| `--r2primenet-ignore-field-mismatch` | Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its weights were trained at; the predicted R2' is then uncalibrated. Without it, an explicit `--r2prime-strategy r2primenet` refuses a non-3 T run and `auto` skips R2' with a warning. |
| `--r2primenet-patch <D> <H> <W>` | R2PRIMEnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--export-dicom` | Also export final maps as DICOM series into each subject's extra_files/ folder. |
| `--inhomogeneity-correction` | Apply inhomogeneity correction to magnitude before masking. |
| `--no-inhomogeneity-correction` | Disable inhomogeneity correction. |
| `--axial-resampling` | Resample oblique acquisitions onto an axial grid when their obliquity exceeds --obliquity-threshold. Off by default (methods that only work on axial data resample regardless) |
| `--no-axial-resampling` | Do not resample oblique acquisitions to axial (overrides a config's axial_resampling) |
| `--obliquity-threshold <OBLIQUITY_THRESHOLD>` | Obliquity in degrees above which --axial-resampling resamples (default: 10) |
| `--crop-to-mask` | Reconstruct inside a box around the mask instead of the whole field of view. Cuts the FFT-bound stages roughly in proportion to the voxels removed, but those stages are a small share of a full run, so the end-to-end gain is usually modest. Off by default. Outputs are written on the full grid either way; this only changes where the arithmetic happens. Padding to an FFT-friendly size happens regardless. |
| `--ignore-slice-gap` | Reconstruct even when a 2D acquisition's sidecar `SliceThickness` is thinner than the slice pitch. Without it, QSM refuses such a run: the gap between slices is unsampled tissue, and the FFT kernels assume a contiguous grid, so the map would be wrong. |
| `--fft-padding` | Pad the grid outward to an FFT-friendly size before the FFT-bound stages. Much faster on an awkward grid, and discards nothing, but it changes where k-space is sampled and so shifts the reconstruction slightly. Off by default. |
| `--crop-margin-mm <CROP_MARGIN_MM>` | Margin in mm around the mask when --crop-to-mask is set (default 32). Below ~32 mm the reconstruction starts to differ from the uncropped one; it must also clear the largest SMV kernel in use, and V-SHARP defaults to 12 mm. |
| `--output-space <OUTPUT_SPACE>` | Grid to write derivatives on when the run was resampled to axial. `acquired` (default) puts them back where the data came from, so transforms you already hold still apply; `working` leaves them on the resampled grid and avoids a second interpolation. Values: `acquired`, `working`. |
| `--mask-preset <MASK_PRESET>` | Mask preset: robust-threshold, bet, hd-bet (HD-BET deep-learning brain extraction + signal-gated erosion; needs a deep-learning build), bet-and-phase (BET on the magnitude AND a thresholded phase-quality map, then hole-filled and eroded), rs2-net (RS2-Net deep-learning rodent brain extraction), or mindgrab (MindGrab deep-learning brain extraction for any contrast; needs a deep-learning build) Values: `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab`. |
| `--use-custom-masks <USE_CUSTOM_MASKS>` | Prefer a bring-your-own mask from BIDS derivatives when present (else compute it). Bare flag = first matching derivatives tool (alphabetical); give a name (e.g. `bet`) to restrict to `derivatives/<tool>/sub-*/anat/*_mask.nii*`. Always falls back. |
| `--mask <MASK_SECTIONS_CLI>` | Define a mask section (repeatable; multiple sections are combined with --mask-combine). Format: &lt;input&gt;,&lt;generator&gt;,&lt;refinement1&gt;,&lt;refinement2&gt;,... Example: phase-quality,threshold:otsu,dilate:2,fill-holes:0,erode:2 Example: magnitude,bet:0.5,erode:2 Example: magnitude,hd-bet,signal-erode (HD-BET: `hd-bet[:full\|window\|XxYxZ\|low-memory][:tta][:step=F]`; signal-erode: `signal-erode[:threshold[:depth_cap[:global_erosions[:bias_sigma[:min_component]]]]]`) |
| `--mask-combine <MASK_COMBINE>` | How to combine multiple --mask sections: `or` (union, the default) or `and` (intersection — a voxel survives only where every section keeps it) Values: `or`, `and`. |
| `--mask-refine <MASK_REFINEMENTS_CLI>` | Refinement applied to the combined mask, after --mask-combine (repeatable). Takes the same refinement ops as --mask, e.g. `erode:2` or `fill-holes:0`; generators (threshold/bet/hd-bet) belong in a --mask section instead. Example: --mask-combine and --mask-refine fill-holes:0 --mask-refine erode:1. |
| `--two-pass` | Two-pass artefact reduction: reconstruct a second time against a mask whose holes are left unfilled, and keep that reconstruction wherever it is defined. Strong susceptibility sources (air-tissue interfaces, haemorrhage, implants) make the phase around them unrecoverable; leaving those voxels out keeps their streaking from spreading, while the ordinary mask still supplies values inside the holes. Roughly doubles background removal and inversion time, and is not free of downsides — where the holes were not artefactual it can be worse than a single pass. Based on doi:10.1002/mrm.29048 (Stewart et al.). Off by default. The conventional single-pass map is kept alongside as `_desc-singlepass_Chimap.nii`. |
| `--two-pass-mask <TWO_PASS_SECTIONS_CLI>` | Define the reliable-pass mask used by --two-pass (repeatable; same format as --mask). Defaults to `phase-quality,threshold:otsu` — an unfilled threshold on the phase-quality map. Its holes are the point, so refinements that close them (fill-holes, close, gaussian) defeat the method. Folded with --mask-combine and --mask-refine like the main mask, and always intersected with it. |
| `--n-procs <N_PROCS>` | Number of parallel threads. |
| `--source-dicom <SOURCE_DICOM>` | Optional source DICOM directory; inherit patient/study identity from the original DICOMs when exporting (used with --export-dicom) |
| `--dicom-outputs <DICOM_OUTPUTS>` | Restrict DICOM export to these maps (default: all produced). Values: chimap, swi, minip, t2starmap, r2starmap, r2map, r2primemap, desc-paramagnetic_chimap, desc-diamagnetic_chimap, desc-total_chimap (used with --export-dicom) |
| `--dry` | Print the processing plan, the pipeline graph and the methods text without running anything or writing any file. |
| `--debug` | Enable debug logging. |
| `--mem-limit-gb <MEM_LIMIT_GB>` | Memory limit in GB for concurrent run scheduling (auto-detected if not specified) |
| `--no-mem-limit` | Disable memory-aware concurrency limiting. |
| `--force` | Force re-run, ignoring cached pipeline state. |
| `--clean-intermediates` | Once every run (and any multi-orientation combination) is done, delete the intermediate images under workflow/. Published outputs, per-step provenance and the cache state are kept: a re-run with the same configuration recomputes nothing, one with a changed configuration recomputes the intermediates it needs. |
| `--no-dataset-steps` | Process each run and stop: skip the dataset-level steps (multi-orientation reconstruction, summary tables, cohort figures, provenance, dataset_description.json, pipeline_config.toml, methods.md, the pipeline graph, changes/) and log to logs/&lt;run&gt;.log instead of qsmxt.log. `qsmxt aggregate` does those steps once every run is done; `qsmxt slurm` uses the pair. |

## `qsmxt init`

Generate a pipeline configuration file

```sh
qsmxt init [OPTIONS]
```

| Argument | Description |
| --- | --- |
| `-o, --output <OUTPUT>` | Output file path (prints to stdout if not specified) |

## `qsmxt validate`

Validate BIDS dataset structure for QSM processing

```sh
qsmxt validate [OPTIONS] <BIDS_DIR>
```

| Argument | Description |
| --- | --- |
| `<BIDS_DIR>` | Input BIDS directory. Required. |
| `--include <INCLUDE>` | Include only runs matching these glob patterns. |
| `--exclude <EXCLUDE>` | Exclude runs matching these glob patterns. |

## `qsmxt dicom-convert`

Convert a DICOM directory to BIDS using automatic series classification

```sh
qsmxt dicom-convert [OPTIONS] <DICOM_DIR> <OUTPUT_DIR>
```

| Argument | Description |
| --- | --- |
| `<DICOM_DIR>` | Input DICOM directory (searched recursively) Required. |
| `<OUTPUT_DIR>` | Output BIDS directory. Required. |
| `--dry-run` | Print the auto-detected series classification and exit without converting. |

## `qsmxt slurm`

Generate SLURM job scripts for HPC execution: one job per run, then an aggregation job

```sh
qsmxt slurm [OPTIONS] --account <ACCOUNT> <BIDS_DIR> [OUTPUT_DIR]
```

| Argument | Description |
| --- | --- |
| `<BIDS_DIR>` | Input BIDS directory. Required. |
| `<OUTPUT_DIR>` | Output directory (defaults to bids_dir; outputs go into &lt;dir&gt;/derivatives/qsmxt/) |
| `--account <ACCOUNT>` | SLURM account name. Required. |
| `--partition <PARTITION>` | SLURM partition. |
| `--config <CONFIG>` | Pipeline configuration file (TOML) |
| `--time <TIME>` | Wall time limit (e.g., 02:00:00) Default: `02:00:00`. |
| `--mem <MEM>` | Memory per job in GB [default: estimated per run from its size and the pipeline]. |
| `--cpus-per-task <CPUS_PER_TASK>` | CPUs per task. Default: `4`. |
| `--submit` | Auto-submit scripts via sbatch. |
| `--include <INCLUDE>` | Include only runs matching these glob patterns (e.g. "sub-1\*" "\*ses-pre\*") |
| `--exclude <EXCLUDE>` | Exclude runs matching these glob patterns (e.g. "\*mygrea\*") |
| `--num-echoes <NUM_ECHOES>` | Limit number of echoes to process. |
| `--force` | Force re-run, ignoring cached pipeline state (passed to every job) |
| `--clean-intermediates` | Once every run and the dataset-level steps are done, delete the intermediate images under workflow/ (done by the aggregation job) |
| `--debug` | Enable debug logging (in every job) |
| `--source-dicom <SOURCE_DICOM>` | Optional source DICOM directory; inherit patient/study identity from the original DICOMs when exporting (used with --export-dicom; each run's job exports its maps) |
| `--dicom-outputs <DICOM_OUTPUTS>` | Restrict DICOM export to these maps (default: all produced); values as for `qsmxt run` (used with --export-dicom) |
| `--qsm-algorithm <QSM_ALGORITHM>` | QSM dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--orientation-group <PATTERN>` | Group runs into multi-orientation sets and reconstruct each with COSMOS or STI. Takes an entity name (acq, run, rec, inv), a glob over the run key (`*acq-dir*`), or `re:` and a regex with one capture group. Orientations are co-registered if they need it. |
| `--no-orientation-registration` | Do not co-register the orientations in a group — use this when they were already registered externally and a second interpolation is not wanted. A group that still needs registration is then refused instead of reconstructed. |
| `--multi-orientation-algorithm <MULTI_ORIENTATION_ALGORITHM>` | Which reconstruction to run over each orientation group. Values: `cosmos`, `sti`. |
| `--multi-orientation-lambda <MULTI_ORIENTATION_LAMBDA>` | Regularization for the multi-orientation reconstruction (COSMOS k-space L2, or STI ridge) |
| `--multi-orientation-force` | Reconstruct an orientation group even when its B0 directions look degenerate. |
| `--cosmos-magnitude-weighting` | COSMOS: weight each orientation by its combined magnitude (conjugate-gradient solve) |
| `--cosmos-tol <COSMOS_TOL>` | Magnitude-weighted COSMOS: conjugate-gradient tolerance (needs --cosmos-magnitude-weighting) |
| `--cosmos-max-iter <COSMOS_MAX_ITER>` | Magnitude-weighted COSMOS: conjugate-gradient iteration cap (needs --cosmos-magnitude-weighting) |
| `--unwrapping-algorithm <UNWRAPPING_ALGORITHM>` | Unwrapping algorithm. Values: `romeo`, `laplacian`. |
| `--bf-algorithm <BF_ALGORITHM>` | Background field removal algorithm (default: vsharp; pdf with --qsm-algorithm medi, as in its authors' pipeline) Values: `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella`, `bfrnet`, `iqfm`. |
| `--masking-input <MASKING_INPUT>` | Masking input image (overrides the input of the configured mask sections; combine with --mask-preset, e.g. --mask-preset robust-threshold --masking-input magnitude) Values: `magnitude-first`, `magnitude`, `magnitude-last`, `phase-quality`. |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Enable phase offset removal for multi-echo data (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--coil-combination-sigma <COIL_COMBINATION_SIGMA>` | MCPC-3D-S coil combination: smoothing sigma for the per-coil phase offsets (3 values, in voxels; default 10 10 5, as in MriResearchTools). Only used for uncombined multi-coil runs. |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method. Values: `weighted-avg`, `linear-fit`, `robust-fit`. |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO. Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--laplacian-solver <LAPLACIAN_SOLVER>` | Poisson solver for `--unwrapping-algorithm laplacian`: `dct` (unweighted least squares, Neumann/DCT solve; default) or `fft` (Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume) Values: `dct`, `fft`. |
| `--laplacian-fft-pad <VOXELS>` | Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or three (x y z). Default 64. |
| `--bet-fractional-intensity <BET_FRACTIONAL_INTENSITY>` | BET fractional intensity (0.0-1.0; smaller = larger brain). Applies to every BET step in the masking recipe, overriding a `bet:<f>` in a --mask spec. |
| `--bet-smoothness <BET_SMOOTHNESS>` | BET surface smoothness factor. Applies to every BET step in the masking recipe. |
| `--bet-gradient-threshold <BET_GRADIENT_THRESHOLD>` | BET gradient threshold (-1 to 1). Applies to every BET step in the masking recipe. |
| `--bet-iterations <BET_ITERATIONS>` | BET surface evolution iterations. Applies to every BET step in the masking recipe. |
| `--bet-subdivisions <BET_SUBDIVISIONS>` | BET icosphere subdivision level. Applies to every BET step in the masking recipe. |
| `--bet-voxel-scale <FACTOR>` | Factor applied to the voxel sizes before BET runs. BET's surface model is tuned to a human brain in millimetres, so preclinical data has to be presented at human scale: 10 is the usual setting for mouse. 1 (the default) is the acquisition's own geometry. Applies to every BET step in the masking recipe. |
| `--qsm-reference <SPEC>` | What the susceptibility map's zero is pinned to: `mean` (the mean over the voxels the map is defined on — the brain mask less the rim background-field removal eroded; the default), `none`, or a SynthSeg region — a structure with both sides merged (`thalamus`), one side (`left-thalamus`), the `ventricles` composite, a raw FreeSurfer id (`4`), or a comma-separated list of any of those. A region implies --do-segmentation. An unrecognised name lists the valid ones. |
| `--rts-delta <RTS_DELTA>` | RTS delta parameter. |
| `--rts-mu <RTS_MU>` | RTS mu parameter. |
| `--rts-tol <RTS_TOL>` | RTS tolerance. |
| `--rts-rho <RTS_RHO>` | RTS rho (ADMM penalty) |
| `--rts-max-iter <RTS_MAX_ITER>` | RTS max iterations. |
| `--rts-lsmr-iter <RTS_LSMR_ITER>` | RTS LSMR iterations. |
| `--tv-lambda <TV_LAMBDA>` | TV lambda parameter. |
| `--tv-rho <TV_RHO>` | TV rho (ADMM penalty; default: 100 × lambda) |
| `--tv-tol <TV_TOL>` | TV tolerance. |
| `--tv-max-iter <TV_MAX_ITER>` | TV max iterations. |
| `--tkd-threshold <TKD_THRESHOLD>` | TKD threshold. |
| `--tsvd-threshold <TSVD_THRESHOLD>` | TSVD threshold. |
| `--tgv-iterations <TGV_ITERATIONS>` | TGV iterations (default: from the voxel size, as the reference does) |
| `--tgv-erosions <TGV_EROSIONS>` | TGV erosions. |
| `--tgv-alpha1 <TGV_ALPHA1>` | TGV alpha1 (first-order weight), in radians at the scan's first echo time and field strength (the reference's units) |
| `--tgv-alpha0 <TGV_ALPHA0>` | TGV alpha0 (second-order weight), in radians at the scan's first echo time and field strength (the reference's units) |
| `--tgv-step-size <TGV_STEP_SIZE>` | TGV primal step size multiplier. |
| `--tikhonov-lambda <TIKHONOV_LAMBDA>` | Tikhonov lambda. |
| `--nltv-lambda <NLTV_LAMBDA>` | NLTV lambda. |
| `--nltv-mu <NLTV_MU>` | NLTV mu (penalty parameter) |
| `--nltv-tol <NLTV_TOL>` | NLTV tolerance. |
| `--nltv-max-iter <NLTV_MAX_ITER>` | NLTV max iterations. |
| `--nltv-newton-iter <NLTV_NEWTON_ITER>` | NLTV maximum Newton iterations of the data-term subproblem. |
| `--medi-lambda <MEDI_LAMBDA>` | MEDI lambda: the data-fidelity weight (larger fits the field more closely) |
| `--medi-merit <MEDI_MERIT>` | MEDI: enable MERIT weighting. |
| `--medi-smv` | MEDI: high-pass the local field with a spherical mean value filter, removing residual background near the brain edge (MEDI-SMV; on by default) |
| `--no-medi-smv` | MEDI: no SMV high-pass of the local field. |
| `--medi-smv-radius <MEDI_SMV_RADIUS>` | MEDI SMV radius in mm (also mSMV's) |
| `--medi-smv-shrink-mask` | MEDI-SMV: shrink the mask to where the SMV of the mask exceeds 0.999 (on by default) |
| `--no-medi-smv-shrink-mask` | MEDI-SMV: keep the full mask, high-passing the field over it. |
| `--medi-msmv` | MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value before MEDI, which fits it with the SMV-filtered dipole kernel (it implies the SMV). Needs at least 3 echoes. |
| `--medi-msmv-maxk <MEDI_MSMV_MAXK>` | MEDI-mSMV: the boundary correction makes at most this many passes, less one. |
| `--medi-msmv-vessel-radius <MEDI_MSMV_VESSEL_RADIUS>` | MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 × the largest voxel dimension if that is more) |
| `--no-medi-msmv-prefilter` | MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges (expert) |
| `--medi-data-weighting <MEDI_DATA_WEIGHTING>` | MEDI: data weighting mode (0=uniform, 1=SNR) |
| `--medi-percentage <MEDI_PERCENTAGE>` | MEDI edge percentage (0.0-1.0) |
| `--medi-cg-tol <MEDI_CG_TOL>` | MEDI CG tolerance. |
| `--medi-cg-max-iter <MEDI_CG_MAX_ITER>` | MEDI CG max iterations. |
| `--medi-max-iter <MEDI_MAX_ITER>` | MEDI max outer iterations. |
| `--medi-tol <MEDI_TOL>` | MEDI outer tolerance. |
| `--tfi-lambda <TFI_LAMBDA>` | TFI lambda: the data-fidelity weight (larger fits the field more closely) |
| `--tfi-precond <TFI_PRECOND>` | TFI preconditioner value (susceptibility scaling outside the mask) |
| `--tfi-data-weighting <TFI_DATA_WEIGHTING>` | TFI: data weighting mode (0=uniform, 1=SNR) |
| `--tfi-percentage <TFI_PERCENTAGE>` | TFI edge percentage (0.0-1.0) |
| `--tfi-cg-tol <TFI_CG_TOL>` | TFI CG tolerance. |
| `--tfi-cg-max-iter <TFI_CG_MAX_ITER>` | TFI CG max iterations. |
| `--tfi-max-iter <TFI_MAX_ITER>` | TFI max outer iterations. |
| `--tfi-tol <TFI_TOL>` | TFI outer tolerance. |
| `--ilsqr-tol <ILSQR_TOL>` | iLSQR tolerance of the streaking-artefact solve. |
| `--ilsqr-max-iter <ILSQR_MAX_ITER>` | iLSQR max iterations of each LSQR solve. |
| `--ilsqr-pad-mm <MM>` | iLSQR zero-padding in mm per side. |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance. |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR max iterations. |
| `--heidi-cone-threshold <HEIDI_CONE_THRESHOLD>` | \|D(k)\| above which a coefficient is well-conditioned and kept from the LSQR seed. |
| `--heidi-gradient-threshold <HEIDI_GRADIENT_THRESHOLD>` | Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous. |
| `--no-heidi-laplacian-correction` | Do not open the homogeneity masks where the field's Laplacian is large. |
| `--heidi-laplacian-threshold <HEIDI_LAPLACIAN_THRESHOLD>` | Field-Laplacian threshold (ppm per voxel²) |
| `--heidi-gradient-mask-floor <HEIDI_GRADIENT_MASK_FLOOR>` | TV weight kept where every direction reads as an edge (0 reproduces upstream's shipped binarised masks) |
| `--heidi-continuation-steps <HEIDI_CONTINUATION_STEPS>` | NESTA continuation steps on μ. |
| `--heidi-inner-iterations <HEIDI_INNER_ITERATIONS>` | Accelerated-gradient iterations per continuation step. |
| `--heidi-mu-min <HEIDI_MU_MIN>` | Final (smallest) smoothing parameter μ. |
| `--heidi-tol <HEIDI_TOL>` | Relative objective change that ends an inner loop early (the default runs every iteration) |
| `--no-heidi-denoise` | Skip the anisotropic-diffusion smoothing of the field before its gradients are taken. |
| `--heidi-denoise-iterations <HEIDI_DENOISE_ITERATIONS>` | Anisotropic-diffusion iterations. |
| `--heidi-denoise-time-step <HEIDI_DENOISE_TIME_STEP>` | Anisotropic-diffusion time step. |
| `--heidi-denoise-conductance <HEIDI_DENOISE_CONDUCTANCE>` | Anisotropic-diffusion conductance. |
| `--qsmart-ilsqr-max-iter <QSMART_ILSQR_MAX_ITER>` | QSMART iLSQR max iterations per stage. The iLSQR tolerance is --ilsqr-tol. |
| `--qsmart-ilsqr-pad-mm <MM>` | Zero-padding of QSMART's inner iLSQR solves, in mm per side (default: 6) |
| `--qsmart-vasc-sphere-radius <QSMART_VASC_SPHERE_RADIUS>` | QSMART vasculature detection sphere radius. |
| `--qsmart-inversion <QSMART_INVERSION>` | QSMART inner dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--qsmart-sdf-sigma1-stage1 <QSMART_SDF_SIGMA1_STAGE1>` | QSMART SDF sigma1, stage 1 (voxels) |
| `--qsmart-sdf-sigma2-stage1 <QSMART_SDF_SIGMA2_STAGE1>` | QSMART SDF sigma2, stage 1 (voxels) |
| `--qsmart-sdf-sigma1-stage2 <QSMART_SDF_SIGMA1_STAGE2>` | QSMART SDF sigma1, stage 2 (voxels) |
| `--qsmart-sdf-sigma2-stage2 <QSMART_SDF_SIGMA2_STAGE2>` | QSMART SDF sigma2, stage 2 (voxels) |
| `--qsmart-sdf-lower-lim <QSMART_SDF_LOWER_LIM>` | QSMART SDF proximity lower limit. |
| `--qsmart-sdf-curv-constant <QSMART_SDF_CURV_CONSTANT>` | QSMART SDF curvature constant. |
| `--qsmart-frangi-scale-min <QSMART_FRANGI_SCALE_MIN>` | QSMART Frangi min vessel radius (mm) |
| `--qsmart-frangi-scale-max <QSMART_FRANGI_SCALE_MAX>` | QSMART Frangi max vessel radius (mm) |
| `--qsmart-frangi-scale-ratio <QSMART_FRANGI_SCALE_RATIO>` | QSMART Frangi scale step (mm) |
| `--qsmart-frangi-c <QSMART_FRANGI_C>` | QSMART Frangi C noise threshold. |
| `--ndi-tau <NDI_TAU>` | NDI gradient-descent step size (tau) |
| `--ndi-alpha <NDI_ALPHA>` | NDI L2 regularization weight (alpha) |
| `--ndi-max-iter <NDI_MAX_ITER>` | NDI max iterations. |
| `--ndi-auto-stop <NDI_AUTO_STOP>` | NDI: stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto); --ndi-max-iter is then the maximum. |
| `--fansi-alpha1 <FANSI_ALPHA1>` | FANSI first-order (TV/TGV) L1 penalty weight (alpha1) |
| `--fansi-mu1 <FANSI_MU1>` | FANSI gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--fansi-mu2 <FANSI_MU2>` | FANSI fidelity-consistency ADMM weight (mu2) |
| `--fansi-alpha0 <FANSI_ALPHA0>` | FANSI second-order L1 penalty weight (alpha0, nlTGV only; default: 2 × alpha1) |
| `--fansi-mu0 <FANSI_MU0>` | FANSI second-order consistency ADMM weight (mu0, nlTGV only; default: 2 × mu1) |
| `--fansi-max-iter <FANSI_MAX_ITER>` | FANSI outer ADMM iterations. |
| `--fansi-tol-update <FANSI_TOL_UPDATE>` | FANSI percent-update convergence tolerance. |
| `--fansi-tol-delta <FANSI_TOL_DELTA>` | FANSI inner Newton convergence tolerance. |
| `--l1qsm-alpha1 <L1QSM_ALPHA1>` | L1-QSM gradient (TV) L1 penalty weight (alpha1) |
| `--l1qsm-mu1 <L1QSM_MU1>` | L1-QSM gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--l1qsm-mu2 <L1QSM_MU2>` | L1-QSM fidelity-consistency ADMM weight (mu2) |
| `--l1qsm-mu3 <L1QSM_MU3>` | L1-QSM L1 proximal ADMM weight (mu3) |
| `--l1qsm-lambda <L1QSM_LAMBDA>` | L1-QSM L1 fidelity strength (lambda) |
| `--l1qsm-max-iter <L1QSM_MAX_ITER>` | L1-QSM outer ADMM iterations. |
| `--l1qsm-tol-update <L1QSM_TOL_UPDATE>` | L1-QSM percent-update convergence tolerance. |
| `--l1qsm-tol-delta <L1QSM_TOL_DELTA>` | L1-QSM inner Newton convergence tolerance. |
| `--whqsm-alpha1 <WHQSM_ALPHA1>` | WH-QSM TV regularization weight (alpha1) |
| `--whqsm-mu1 <WHQSM_MU1>` | WH-QSM ADMM penalty for TV splitting (mu1) |
| `--whqsm-mu2 <WHQSM_MU2>` | WH-QSM ADMM penalty for data-fidelity splitting (mu2) |
| `--whqsm-beta <WHQSM_BETA>` | WH-QSM weak-harmonic ROI penalty (beta) |
| `--whqsm-muh <WHQSM_MUH>` | WH-QSM ADMM penalty for harmonic-field splitting (muh) |
| `--whqsm-max-iter <WHQSM_MAX_ITER>` | WH-QSM max outer iterations. |
| `--whqsm-tol-update <WHQSM_TOL_UPDATE>` | WH-QSM percent-update convergence tolerance. |
| `--whqsm-tol-delta <WHQSM_TOL_DELTA>` | WH-QSM inner Newton convergence tolerance. |
| `--hdqsm-alpha-l2 <HDQSM_ALPHA_L2>` | HD-QSM L2-stage TV weight (alpha_l2) |
| `--hdqsm-mu1-l2 <HDQSM_MU1_L2>` | HD-QSM L2-stage gradient-consistency ADMM weight (mu1_l2; default: 100 × alpha_l2) |
| `--hdqsm-mu2 <HDQSM_MU2>` | HD-QSM fidelity consistency weight (mu2) |
| `--hdqsm-max-iter-l1 <HDQSM_MAX_ITER_L1>` | HD-QSM stage-1 (L1) iterations (max_iter_l1) |
| `--hdqsm-max-iter-l2 <HDQSM_MAX_ITER_L2>` | HD-QSM stage-2 (L2) iterations (max_iter_l2) |
| `--hdqsm-tol-update <HDQSM_TOL_UPDATE>` | HD-QSM stage-2 percent-update convergence tolerance. |
| `--amp-pe-wave-order <AMP_PE_WAVE_ORDER>` | AMP-PE Daubechies wavelet order (1=db1, 2=db2) |
| `--amp-pe-nlevel <AMP_PE_NLEVEL>` | AMP-PE wavelet decomposition levels. |
| `--amp-pe-wave-pec <AMP_PE_WAVE_PEC>` | AMP-PE morphology-mask energy retention fraction (0.0-1.0) |
| `--amp-pe-simulated-te <AMP_PE_SIMULATED_TE>` | AMP-PE simulated echo time (s) used to turn the field into phase. |
| `--amp-pe-max-linearization-ite <AMP_PE_MAX_LINEARIZATION_ITE>` | AMP-PE linearization iterations per stage. |
| `--amp-pe-damp-rate-sig <AMP_PE_DAMP_RATE_SIG>` | AMP-PE GAMP signal-update damping rate. |
| `--amp-pe-damp-rate-par <AMP_PE_DAMP_RATE_PAR>` | AMP-PE parameter-estimation learning rate (kappa) |
| `--amp-pe-max-pe-spar-ite <AMP_PE_MAX_PE_SPAR_ITE>` | AMP-PE inner sparse-reconstruction iterations. |
| `--amp-pe-max-pe-est-ite <AMP_PE_MAX_PE_EST_ITE>` | AMP-PE inner parameter-estimation iterations. |
| `--amp-pe-cvg-thd <AMP_PE_CVG_THD>` | AMP-PE GAMP inner convergence threshold. |
| `--amp-pe-tikhonov-beta <AMP_PE_TIKHONOV_BETA>` | AMP-PE L2-seed Tikhonov weight. |
| `--r2star-qsm-r-const-3t <R2STAR_QSM_R_CONST_3T>` | R2\*-QSM relaxometric constant at 3T (Hz/ppm) |
| `--decompose-n-inner <DECOMPOSE_N_INNER>` | DECOMPOSE inner alternating passes per voxel. |
| `--decompose-chi-bound <DECOMPOSE_CHI_BOUND>` | DECOMPOSE upper bound on \|χ\| in the fit (ppm) |
| `--decompose-max-lm-iter <DECOMPOSE_MAX_LM_ITER>` | DECOMPOSE Levenberg–Marquardt max iterations. |
| `--chi-sep-ilsqr-dr-pos <CHI_SEP_ILSQR_DR_POS>` | χ-sep iLSQR paramagnetic relaxometric constant (Hz/ppm) |
| `--chi-sep-ilsqr-dr-neg <CHI_SEP_ILSQR_DR_NEG>` | χ-sep iLSQR diamagnetic relaxometric constant (Hz/ppm) |
| `--chi-sep-ilsqr-lambda1 <CHI_SEP_ILSQR_LAMBDA1>` | χ-sep iLSQR L1 edge-masked TV weight. |
| `--chi-sep-ilsqr-percentage <CHI_SEP_ILSQR_PERCENTAGE>` | χ-sep iLSQR edge-mask keep fraction (0-1) |
| `--chi-sep-ilsqr-r2p-min <CHI_SEP_ILSQR_R2P_MIN>` | χ-sep iLSQR R2' reliability window lower (Hz) |
| `--chi-sep-ilsqr-r2p-max <CHI_SEP_ILSQR_R2P_MAX>` | χ-sep iLSQR R2' reliability window upper (Hz) |
| `--chi-sep-ilsqr-max-iter <CHI_SEP_ILSQR_MAX_ITER>` | χ-sep iLSQR outer Gauss-Newton iterations. |
| `--chi-sep-ilsqr-tol <CHI_SEP_ILSQR_TOL>` | χ-sep iLSQR outer relative-change tolerance. |
| `--chi-sep-ilsqr-cg-max-iter <CHI_SEP_ILSQR_CG_MAX_ITER>` | χ-sep iLSQR inner CG max iterations. |
| `--chi-sep-ilsqr-cg-tol <CHI_SEP_ILSQR_CG_TOL>` | χ-sep iLSQR inner CG relative tolerance. |
| `--chi-sep-medi-lambda-para <CHI_SEP_MEDI_LAMBDA_PARA>` | χ-sep MEDI paramagnetic L1 weight. |
| `--chi-sep-medi-lambda-dia <CHI_SEP_MEDI_LAMBDA_DIA>` | χ-sep MEDI diamagnetic L1 weight. |
| `--chi-sep-medi-lambda-cpl <CHI_SEP_MEDI_LAMBDA_CPL>` | χ-sep MEDI field/R2' coupling weight. |
| `--chi-sep-medi-dr-pos <CHI_SEP_MEDI_DR_POS>` | χ-sep MEDI paramagnetic relaxivity (Hz/ppm) |
| `--chi-sep-medi-dr-neg <CHI_SEP_MEDI_DR_NEG>` | χ-sep MEDI diamagnetic relaxivity (Hz/ppm) |
| `--chi-sep-medi-percentage <CHI_SEP_MEDI_PERCENTAGE>` | χ-sep MEDI edge-mask percentage (0-1) |
| `--chi-sep-medi-cg-tol <CHI_SEP_MEDI_CG_TOL>` | χ-sep MEDI inner CG tolerance. |
| `--chi-sep-medi-cg-max-iter <CHI_SEP_MEDI_CG_MAX_ITER>` | χ-sep MEDI inner CG max iterations. |
| `--chi-sep-medi-max-iter <CHI_SEP_MEDI_MAX_ITER>` | χ-sep MEDI outer Gauss-Newton iterations. |
| `--chi-sep-medi-tol <CHI_SEP_MEDI_TOL>` | χ-sep MEDI outer convergence tolerance. |
| `--wavesep-dr-pos <WAVESEP_DR_POS>` | WaveSep paramagnetic relaxivity (Hz/ppm) |
| `--wavesep-dr-neg <WAVESEP_DR_NEG>` | WaveSep diamagnetic relaxivity (Hz/ppm) |
| `--wavesep-alpha <WAVESEP_ALPHA>` | WaveSep proximal-gradient step size. |
| `--wavesep-lambda <WAVESEP_LAMBDA>` | WaveSep wavelet L1 soft-threshold weight. |
| `--wavesep-wavelet-order <WAVESEP_WAVELET_ORDER>` | WaveSep Daubechies wavelet order. |
| `--wavesep-max-iter <WAVESEP_MAX_ITER>` | WaveSep ISTA max iterations. |
| `--wavesep-tol <WAVESEP_TOL>` | WaveSep relative-change stop tolerance. |
| `--hc-chisep-dr-pos-3t <HC_CHISEP_DR_POS_3T>` | HC-ChiSep paramagnetic relaxivity at 3T (Hz/ppm) |
| `--hc-chisep-bin-hz <HC_CHISEP_BIN_HZ>` | HC-ChiSep R2' bin width for the anchored grid search (Hz) |
| `--chi-sepnet-patch <D> <H> <W>` | χ-sepnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--susep-net-patch <D> <H> <W>` | Run SUSEP-Net as a sliding window of patches this size in voxels, D H W, each a multiple of 8, so its memory follows the patch, not the volume. Omit to run the whole volume at once. |
| `--motion-correction` | Rigidly co-register every echo onto a reference echo (estimated on magnitude, applied to phase through the complex signal) before field mapping, and restrict the mask to voxels every echo covers. Off by default. Needs magnitude images. |
| `--motion-reference <first\|last\|N>` | Echo the others are aligned onto: `first` (default), `last`, or a 1-based echo number. |
| `--no-motion-chain` | Register every echo from the identity instead of seeding it from its neighbour's solution. |
| `--motion-chain-levels <MOTION_CHAIN_LEVELS>` | Pyramid levels for a seeded (chained) registration (default 3) |
| `--motion-min-ncc <MOTION_MIN_NCC>` | NCC below which an echo's registration is reported as suspect (default 0.3, set for echoes; ~0.9 suits repeats) |
| `--registration-levels <REGISTRATION_LEVELS>` | Registration pyramid levels: the capture range; do not lower (default 5) |
| `--registration-max-samples <REGISTRATION_MAX_SAMPLES>` | Registration: voxels sampled per metric evaluation, per level (default 200000) |
| `--registration-max-iterations <REGISTRATION_MAX_ITERATIONS>` | Registration: Powell cycles per level (default 20) |
| `--registration-max-evaluations <REGISTRATION_MAX_EVALUATIONS>` | Registration: metric evaluations across all levels, a hard stop (default 5000) |
| `--registration-tolerance <REGISTRATION_TOLERANCE>` | Registration: relative cost improvement per cycle that counts as converged (default 1e-5) |
| `--registration-rotation-step-deg <REGISTRATION_ROTATION_STEP_DEG>` | Registration: initial rotation step at the finest level, degrees (default 3) |
| `--registration-translation-step-mm <REGISTRATION_TRANSLATION_STEP_MM>` | Registration: initial translation step at the finest level, mm (default 3) |
| `--registration-min-overlap <REGISTRATION_MIN_OVERLAP>` | Registration: smallest fraction of sampled voxels that must overlap (default 0.25) |
| `--vsharp-deconvolve <BOOL>` | Undo V-SHARP's SMV high-pass with a truncated-SVD deconvolution (default: false) |
| `--vsharp-threshold <VSHARP_THRESHOLD>` | V-SHARP deconvolution (TSVD) threshold; only used with --vsharp-deconvolve true. |
| `--vsharp-max-radius <VSHARP_MAX_RADIUS>` | V-SHARP max kernel radius in mm. |
| `--vsharp-min-radius <VSHARP_MIN_RADIUS>` | V-SHARP min kernel radius in mm. |
| `--vsharp-grid <VSHARP_GRID>` | Grid V-SHARP estimates the background on: `native` (default), `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened), or `isotropic` (spacing from --vsharp-grid-spacing). The local field keeps native resolution. Values: `native`, `finest`, `isotropic`. |
| `--vsharp-grid-spacing <MM>` | V-SHARP isotropic working-grid spacing in mm (default 1); used with --vsharp-grid isotropic. |
| `--pdf-tol <PDF_TOL>` | PDF tolerance, relative to the noise level. |
| `--lbv-tol <LBV_TOL>` | LBV tolerance. |
| `--ismv-tol <ISMV_TOL>` | iSMV tolerance. |
| `--ismv-max-iter <ISMV_MAX_ITER>` | iSMV max iterations. |
| `--ismv-radius <ISMV_RADIUS>` | iSMV kernel radius in mm. |
| `--sharp-threshold <SHARP_THRESHOLD>` | SHARP threshold. |
| `--sharp-radius <SHARP_RADIUS>` | SHARP kernel radius in mm. |
| `--resharp-radius <RESHARP_RADIUS>` | RESHARP SMV kernel radius in mm. |
| `--resharp-tik-reg <RESHARP_TIK_REG>` | RESHARP Tikhonov regularization parameter. |
| `--resharp-tol <RESHARP_TOL>` | RESHARP CG convergence tolerance. |
| `--resharp-max-iter <RESHARP_MAX_ITER>` | RESHARP maximum CG iterations. |
| `--harperella-radius <HARPERELLA_RADIUS>` | HARPERELLA SMV kernel radius in mm. |
| `--harperella-max-iter <HARPERELLA_MAX_ITER>` | HARPERELLA maximum iterations. |
| `--harperella-tol <HARPERELLA_TOL>` | HARPERELLA convergence tolerance. |
| `--iharperella-max-iter <IHARPERELLA_MAX_ITER>` | iHARPERELLA maximum iterations. |
| `--iharperella-tol <IHARPERELLA_TOL>` | iHARPERELLA convergence tolerance. |
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) |
| `--romeo-individual` | ROMEO: use individual per-echo unwrapping (default) |
| `--no-romeo-individual` | ROMEO: use template-based temporal unwrapping (disables individual mode) |
| `--romeo-correct-global` | ROMEO: enable inter-echo 2π offset correction (default: off) |
| `--no-romeo-correct-global` | ROMEO: disable inter-echo 2π offset correction. |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0.5) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: maximum number of seed regions (default: 1) |
| `--romeo-merge-regions` | ROMEO: merge neighboring regions after unwrapping (default: off) |
| `--romeo-correct-regions` | ROMEO: correct each region's median to nearest 0 (default: off) |
| `--romeo-wrap-addition <ROMEO_WRAP_ADDITION>` | ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0) |
| `--swi-unwrapping <SWI_UNWRAPPING>` | SWI phase unwrapping before the echoes are combined [default: laplacian]. Values: `laplacian`, `laplacian-slice`, `romeo`. |
| `--swi-hp-sigma <SWI_HP_SIGMA>` | SWI high-pass filter sigma (3 values, in voxels) |
| `--swi-scaling <SWI_SCALING>` | SWI phase-mask scaling. Values: `tanh`, `negative-tanh`, `positive`, `negative`, `triangular`. |
| `--swi-strength <SWI_STRENGTH>` | SWI phase scaling strength. |
| `--swi-mip-window <SWI_MIP_WINDOW>` | SWI MIP window size in slices. |
| `--smwi-threshold <SMWI_THRESHOLD>` | SMWI susceptibility threshold in ppm — the \|χ\| at which the weighting mask reaches zero. The 1 ppm default follows SEPIA and targets strong sources; brain tissue wants far less. |
| `--smwi-power <SMWI_POWER>` | SMWI mask power (contrast strength) |
| `--smwi-mip-window <SMWI_MIP_WINDOW>` | SMWI mIP window size in slices. |
| `--synthseg-version <VERSION>` | SynthSeg weights generation: v1 (32 labels, ships with SynthSeg) or v2 (33 labels, adds CSF; weights not published yet, so refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR) |
| `--synthseg-crop <SYNTHSEG_CROP>` | Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32). Omit to process the whole volume, as the SynthSeg CLI does. |
| `--synthseg-patch <VOXELS>` | Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the volume. Omit to run the whole volume at once, as the SynthSeg CLI does. |
| `--no-synthseg-flip-averaging` | Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does. |
| `--no-synthseg-topology-cleanup` | Skip resetting each topological class to its largest connected component. |
| `--synthseg-sigma <SYNTHSEG_SIGMA>` | Gaussian blur on the posteriors, in voxels (0 disables it) |
| `--use-custom-dseg <TOOL>` | Prefer a bring-your-own segmentation from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ when present. Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--homogeneity-sigma-mm <HOMOGENEITY_SIGMA_MM>` | Inhomogeneity correction smoothing sigma in mm. |
| `--homogeneity-nbox <HOMOGENEITY_NBOX>` | Inhomogeneity correction box filter passes. |
| `--linear-fit-reliability-threshold <LINEAR_FIT_RELIABILITY_THRESHOLD>` | Linear and robust fit: percentile for the fit's reliability mask (default 90; 0 disables) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--no-qsm` | Skip QSM processing (only run supplementary outputs like SWI, T2\*, R2\*) |
| `--do-swi` | Also compute SWI. |
| `--do-segmentation` | Also segment the brain from the GRE magnitude with SynthSeg, writing a FreeSurfer-labelled `_dseg.nii` and its BIDS lookup table. Needs no T1w and no registration. Needs a deep-learning build. doi:10.1016/j.media.2023.102789. |
| `--do-analysis` | Also summarise every quantitative map (χ, χ+/χ−, T2\*, R2\*, R2, R2') per segmented structure into a TSV: mean, SD, median, min, max and the 5th/95th percentiles, one row per map and structure, plus a dataset-level table gathering all runs. Summarises whatever the run made — it implies --do-segmentation, and QSM only if no other map was asked for. |
| `--do-smwi` | Also compute SMWI (susceptibility map-weighted imaging): weights the magnitude by a mask built from the susceptibility map rather than from high-pass filtered phase, so the contrast stays where the source is. Needs χ, so it implies QSM. doi:10.1002/mrm.24920. |
| `--do-t2starmap` | Compute T2\* relaxation map from multi-echo magnitude data. |
| `--do-r2starmap` | Compute R2\* decay rate map from multi-echo magnitude data. |
| `--do-r2map` | Compute R2 map from a multi-echo spin-echo (MESE) acquisition (EPG) |
| `--do-r2primemap` | Compute R2' map (= R2\* − R2; needs GRE magnitude + a MESE acquisition) |
| `--do-chisep` | Compute chi-separation (paramagnetic/diamagnetic susceptibility maps) |
| `--chisep <CHI_SEPARATION_ALGORITHM>` | Chi-separation method (default: r2star-qsm) Values: `r2star-qsm`, `decompose`, `chi-sep-ilsqr`, `chi-sep-medi`, `wavesep`, `hc-chisep`, `susep-net`, `chi-sepnet`. |
| `--use-custom-qsm <TOOL>` | Use a bring-your-own QSM (Chimap) from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--use-custom-r2 <TOOL>` | Use a bring-your-own R2 map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--use-custom-r2prime <TOOL>` | Use a bring-your-own R2' map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--r2prime-strategy <R2PRIME_STRATEGY>` | How to obtain R2' when no custom map is supplied. `auto` (default) measures it as R2\* - R2 from a MESE acquisition when one is present and predicts it from R2\* with R2PRIMEnet when it is not; `mese` only ever measures it, producing nothing without spin-echo data; `r2primenet` always predicts it. A measurement and an estimate are not the same thing — the generated methods text says which route was configured. Values: `auto`, `mese`, `r2primenet`. |
| `--r2primenet-ignore-field-mismatch` | Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its weights were trained at; the predicted R2' is then uncalibrated. Without it, an explicit `--r2prime-strategy r2primenet` refuses a non-3 T run and `auto` skips R2' with a warning. |
| `--r2primenet-patch <D> <H> <W>` | R2PRIMEnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--export-dicom` | Also export final maps as DICOM series into each subject's extra_files/ folder. |
| `--inhomogeneity-correction` | Apply inhomogeneity correction to magnitude before masking. |
| `--no-inhomogeneity-correction` | Disable inhomogeneity correction. |
| `--axial-resampling` | Resample oblique acquisitions onto an axial grid when their obliquity exceeds --obliquity-threshold. Off by default (methods that only work on axial data resample regardless) |
| `--no-axial-resampling` | Do not resample oblique acquisitions to axial (overrides a config's axial_resampling) |
| `--obliquity-threshold <OBLIQUITY_THRESHOLD>` | Obliquity in degrees above which --axial-resampling resamples (default: 10) |
| `--crop-to-mask` | Reconstruct inside a box around the mask instead of the whole field of view. Cuts the FFT-bound stages roughly in proportion to the voxels removed, but those stages are a small share of a full run, so the end-to-end gain is usually modest. Off by default. Outputs are written on the full grid either way; this only changes where the arithmetic happens. Padding to an FFT-friendly size happens regardless. |
| `--ignore-slice-gap` | Reconstruct even when a 2D acquisition's sidecar `SliceThickness` is thinner than the slice pitch. Without it, QSM refuses such a run: the gap between slices is unsampled tissue, and the FFT kernels assume a contiguous grid, so the map would be wrong. |
| `--fft-padding` | Pad the grid outward to an FFT-friendly size before the FFT-bound stages. Much faster on an awkward grid, and discards nothing, but it changes where k-space is sampled and so shifts the reconstruction slightly. Off by default. |
| `--crop-margin-mm <CROP_MARGIN_MM>` | Margin in mm around the mask when --crop-to-mask is set (default 32). Below ~32 mm the reconstruction starts to differ from the uncropped one; it must also clear the largest SMV kernel in use, and V-SHARP defaults to 12 mm. |
| `--output-space <OUTPUT_SPACE>` | Grid to write derivatives on when the run was resampled to axial. `acquired` (default) puts them back where the data came from, so transforms you already hold still apply; `working` leaves them on the resampled grid and avoids a second interpolation. Values: `acquired`, `working`. |
| `--mask-preset <MASK_PRESET>` | Mask preset: robust-threshold, bet, hd-bet (HD-BET deep-learning brain extraction + signal-gated erosion; needs a deep-learning build), bet-and-phase (BET on the magnitude AND a thresholded phase-quality map, then hole-filled and eroded), rs2-net (RS2-Net deep-learning rodent brain extraction), or mindgrab (MindGrab deep-learning brain extraction for any contrast; needs a deep-learning build) Values: `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab`. |
| `--use-custom-masks <USE_CUSTOM_MASKS>` | Prefer a bring-your-own mask from BIDS derivatives when present (else compute it). Bare flag = first matching derivatives tool (alphabetical); give a name (e.g. `bet`) to restrict to `derivatives/<tool>/sub-*/anat/*_mask.nii*`. Always falls back. |
| `--mask <MASK_SECTIONS_CLI>` | Define a mask section (repeatable; multiple sections are combined with --mask-combine). Format: &lt;input&gt;,&lt;generator&gt;,&lt;refinement1&gt;,&lt;refinement2&gt;,... Example: phase-quality,threshold:otsu,dilate:2,fill-holes:0,erode:2 Example: magnitude,bet:0.5,erode:2 Example: magnitude,hd-bet,signal-erode (HD-BET: `hd-bet[:full\|window\|XxYxZ\|low-memory][:tta][:step=F]`; signal-erode: `signal-erode[:threshold[:depth_cap[:global_erosions[:bias_sigma[:min_component]]]]]`) |
| `--mask-combine <MASK_COMBINE>` | How to combine multiple --mask sections: `or` (union, the default) or `and` (intersection — a voxel survives only where every section keeps it) Values: `or`, `and`. |
| `--mask-refine <MASK_REFINEMENTS_CLI>` | Refinement applied to the combined mask, after --mask-combine (repeatable). Takes the same refinement ops as --mask, e.g. `erode:2` or `fill-holes:0`; generators (threshold/bet/hd-bet) belong in a --mask section instead. Example: --mask-combine and --mask-refine fill-holes:0 --mask-refine erode:1. |
| `--two-pass` | Two-pass artefact reduction: reconstruct a second time against a mask whose holes are left unfilled, and keep that reconstruction wherever it is defined. Strong susceptibility sources (air-tissue interfaces, haemorrhage, implants) make the phase around them unrecoverable; leaving those voxels out keeps their streaking from spreading, while the ordinary mask still supplies values inside the holes. Roughly doubles background removal and inversion time, and is not free of downsides — where the holes were not artefactual it can be worse than a single pass. Based on doi:10.1002/mrm.29048 (Stewart et al.). Off by default. The conventional single-pass map is kept alongside as `_desc-singlepass_Chimap.nii`. |
| `--two-pass-mask <TWO_PASS_SECTIONS_CLI>` | Define the reliable-pass mask used by --two-pass (repeatable; same format as --mask). Defaults to `phase-quality,threshold:otsu` — an unfilled threshold on the phase-quality map. Its holes are the point, so refinements that close them (fill-holes, close, gaussian) defeat the method. Folded with --mask-combine and --mask-refine like the main mask, and always intersected with it. |

## `qsmxt aggregate`

Run the dataset-level steps over runs already processed with `qsmxt run --no-dataset-steps` (multi-orientation, summary tables, provenance, clean-up)

```sh
qsmxt aggregate [OPTIONS] <BIDS_DIR> [OUTPUT_DIR]
```

| Argument | Description |
| --- | --- |
| `<BIDS_DIR>` | Input BIDS directory. Required. |
| `<OUTPUT_DIR>` | Output directory (defaults to bids_dir; outputs go into &lt;dir&gt;/derivatives/qsmxt/) |
| `--config <CONFIG>` | Pipeline configuration file (TOML): the one the runs were processed with. |
| `--include <INCLUDE>` | Include only runs matching these glob patterns (e.g. "sub-1\*" "\*ses-pre\*") |
| `--exclude <EXCLUDE>` | Exclude runs matching these glob patterns (e.g. "\*mygrea\*") |
| `--num-echoes <NUM_ECHOES>` | Limit number of echoes to process. |
| `--qsm-algorithm <QSM_ALGORITHM>` | QSM dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--orientation-group <PATTERN>` | Group runs into multi-orientation sets and reconstruct each with COSMOS or STI. Takes an entity name (acq, run, rec, inv), a glob over the run key (`*acq-dir*`), or `re:` and a regex with one capture group. Orientations are co-registered if they need it. |
| `--no-orientation-registration` | Do not co-register the orientations in a group — use this when they were already registered externally and a second interpolation is not wanted. A group that still needs registration is then refused instead of reconstructed. |
| `--multi-orientation-algorithm <MULTI_ORIENTATION_ALGORITHM>` | Which reconstruction to run over each orientation group. Values: `cosmos`, `sti`. |
| `--multi-orientation-lambda <MULTI_ORIENTATION_LAMBDA>` | Regularization for the multi-orientation reconstruction (COSMOS k-space L2, or STI ridge) |
| `--multi-orientation-force` | Reconstruct an orientation group even when its B0 directions look degenerate. |
| `--cosmos-magnitude-weighting` | COSMOS: weight each orientation by its combined magnitude (conjugate-gradient solve) |
| `--cosmos-tol <COSMOS_TOL>` | Magnitude-weighted COSMOS: conjugate-gradient tolerance (needs --cosmos-magnitude-weighting) |
| `--cosmos-max-iter <COSMOS_MAX_ITER>` | Magnitude-weighted COSMOS: conjugate-gradient iteration cap (needs --cosmos-magnitude-weighting) |
| `--unwrapping-algorithm <UNWRAPPING_ALGORITHM>` | Unwrapping algorithm. Values: `romeo`, `laplacian`. |
| `--bf-algorithm <BF_ALGORITHM>` | Background field removal algorithm (default: vsharp; pdf with --qsm-algorithm medi, as in its authors' pipeline) Values: `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella`, `bfrnet`, `iqfm`. |
| `--masking-input <MASKING_INPUT>` | Masking input image (overrides the input of the configured mask sections; combine with --mask-preset, e.g. --mask-preset robust-threshold --masking-input magnitude) Values: `magnitude-first`, `magnitude`, `magnitude-last`, `phase-quality`. |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Enable phase offset removal for multi-echo data (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--coil-combination-sigma <COIL_COMBINATION_SIGMA>` | MCPC-3D-S coil combination: smoothing sigma for the per-coil phase offsets (3 values, in voxels; default 10 10 5, as in MriResearchTools). Only used for uncombined multi-coil runs. |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method. Values: `weighted-avg`, `linear-fit`, `robust-fit`. |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) computed from the echo times and an assumed T2\* (`--b0-weight-assumed-t2star`), the same in every voxel: no T2\* map, no magnitude (Wang et al. 2022, Nat Neurosci 25:818). `phase-snr`, which uses the measured magnitude in each voxel, is generally preferable when magnitude is available. Applies to Laplacian unwrapping as well as ROMEO. Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` to compute its fixed echo weights (default 40; no T2\* map is used) |
| `--laplacian-solver <LAPLACIAN_SOLVER>` | Poisson solver for `--unwrapping-algorithm laplacian`: `dct` (unweighted least squares, Neumann/DCT solve; default) or `fft` (Schofield & Zhu sin/cos Laplacian, FFT solve on a zero-padded volume) Values: `dct`, `fft`. |
| `--laplacian-fft-pad <VOXELS>` | Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or three (x y z). Default 64. |
| `--bet-fractional-intensity <BET_FRACTIONAL_INTENSITY>` | BET fractional intensity (0.0-1.0; smaller = larger brain). Applies to every BET step in the masking recipe, overriding a `bet:<f>` in a --mask spec. |
| `--bet-smoothness <BET_SMOOTHNESS>` | BET surface smoothness factor. Applies to every BET step in the masking recipe. |
| `--bet-gradient-threshold <BET_GRADIENT_THRESHOLD>` | BET gradient threshold (-1 to 1). Applies to every BET step in the masking recipe. |
| `--bet-iterations <BET_ITERATIONS>` | BET surface evolution iterations. Applies to every BET step in the masking recipe. |
| `--bet-subdivisions <BET_SUBDIVISIONS>` | BET icosphere subdivision level. Applies to every BET step in the masking recipe. |
| `--bet-voxel-scale <FACTOR>` | Factor applied to the voxel sizes before BET runs. BET's surface model is tuned to a human brain in millimetres, so preclinical data has to be presented at human scale: 10 is the usual setting for mouse. 1 (the default) is the acquisition's own geometry. Applies to every BET step in the masking recipe. |
| `--qsm-reference <SPEC>` | What the susceptibility map's zero is pinned to: `mean` (the mean over the voxels the map is defined on — the brain mask less the rim background-field removal eroded; the default), `none`, or a SynthSeg region — a structure with both sides merged (`thalamus`), one side (`left-thalamus`), the `ventricles` composite, a raw FreeSurfer id (`4`), or a comma-separated list of any of those. A region implies --do-segmentation. An unrecognised name lists the valid ones. |
| `--rts-delta <RTS_DELTA>` | RTS delta parameter. |
| `--rts-mu <RTS_MU>` | RTS mu parameter. |
| `--rts-tol <RTS_TOL>` | RTS tolerance. |
| `--rts-rho <RTS_RHO>` | RTS rho (ADMM penalty) |
| `--rts-max-iter <RTS_MAX_ITER>` | RTS max iterations. |
| `--rts-lsmr-iter <RTS_LSMR_ITER>` | RTS LSMR iterations. |
| `--tv-lambda <TV_LAMBDA>` | TV lambda parameter. |
| `--tv-rho <TV_RHO>` | TV rho (ADMM penalty; default: 100 × lambda) |
| `--tv-tol <TV_TOL>` | TV tolerance. |
| `--tv-max-iter <TV_MAX_ITER>` | TV max iterations. |
| `--tkd-threshold <TKD_THRESHOLD>` | TKD threshold. |
| `--tsvd-threshold <TSVD_THRESHOLD>` | TSVD threshold. |
| `--tgv-iterations <TGV_ITERATIONS>` | TGV iterations (default: from the voxel size, as the reference does) |
| `--tgv-erosions <TGV_EROSIONS>` | TGV erosions. |
| `--tgv-alpha1 <TGV_ALPHA1>` | TGV alpha1 (first-order weight), in radians at the scan's first echo time and field strength (the reference's units) |
| `--tgv-alpha0 <TGV_ALPHA0>` | TGV alpha0 (second-order weight), in radians at the scan's first echo time and field strength (the reference's units) |
| `--tgv-step-size <TGV_STEP_SIZE>` | TGV primal step size multiplier. |
| `--tikhonov-lambda <TIKHONOV_LAMBDA>` | Tikhonov lambda. |
| `--nltv-lambda <NLTV_LAMBDA>` | NLTV lambda. |
| `--nltv-mu <NLTV_MU>` | NLTV mu (penalty parameter) |
| `--nltv-tol <NLTV_TOL>` | NLTV tolerance. |
| `--nltv-max-iter <NLTV_MAX_ITER>` | NLTV max iterations. |
| `--nltv-newton-iter <NLTV_NEWTON_ITER>` | NLTV maximum Newton iterations of the data-term subproblem. |
| `--medi-lambda <MEDI_LAMBDA>` | MEDI lambda: the data-fidelity weight (larger fits the field more closely) |
| `--medi-merit <MEDI_MERIT>` | MEDI: enable MERIT weighting. |
| `--medi-smv` | MEDI: high-pass the local field with a spherical mean value filter, removing residual background near the brain edge (MEDI-SMV; on by default) |
| `--no-medi-smv` | MEDI: no SMV high-pass of the local field. |
| `--medi-smv-radius <MEDI_SMV_RADIUS>` | MEDI SMV radius in mm (also mSMV's) |
| `--medi-smv-shrink-mask` | MEDI-SMV: shrink the mask to where the SMV of the mask exceeds 0.999 (on by default) |
| `--no-medi-smv-shrink-mask` | MEDI-SMV: keep the full mask, high-passing the field over it. |
| `--medi-msmv` | MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value before MEDI, which fits it with the SMV-filtered dipole kernel (it implies the SMV). Needs at least 3 echoes. |
| `--medi-msmv-maxk <MEDI_MSMV_MAXK>` | MEDI-mSMV: the boundary correction makes at most this many passes, less one. |
| `--medi-msmv-vessel-radius <MEDI_MSMV_VESSEL_RADIUS>` | MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 × the largest voxel dimension if that is more) |
| `--no-medi-msmv-prefilter` | MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges (expert) |
| `--medi-data-weighting <MEDI_DATA_WEIGHTING>` | MEDI: data weighting mode (0=uniform, 1=SNR) |
| `--medi-percentage <MEDI_PERCENTAGE>` | MEDI edge percentage (0.0-1.0) |
| `--medi-cg-tol <MEDI_CG_TOL>` | MEDI CG tolerance. |
| `--medi-cg-max-iter <MEDI_CG_MAX_ITER>` | MEDI CG max iterations. |
| `--medi-max-iter <MEDI_MAX_ITER>` | MEDI max outer iterations. |
| `--medi-tol <MEDI_TOL>` | MEDI outer tolerance. |
| `--tfi-lambda <TFI_LAMBDA>` | TFI lambda: the data-fidelity weight (larger fits the field more closely) |
| `--tfi-precond <TFI_PRECOND>` | TFI preconditioner value (susceptibility scaling outside the mask) |
| `--tfi-data-weighting <TFI_DATA_WEIGHTING>` | TFI: data weighting mode (0=uniform, 1=SNR) |
| `--tfi-percentage <TFI_PERCENTAGE>` | TFI edge percentage (0.0-1.0) |
| `--tfi-cg-tol <TFI_CG_TOL>` | TFI CG tolerance. |
| `--tfi-cg-max-iter <TFI_CG_MAX_ITER>` | TFI CG max iterations. |
| `--tfi-max-iter <TFI_MAX_ITER>` | TFI max outer iterations. |
| `--tfi-tol <TFI_TOL>` | TFI outer tolerance. |
| `--ilsqr-tol <ILSQR_TOL>` | iLSQR tolerance of the streaking-artefact solve. |
| `--ilsqr-max-iter <ILSQR_MAX_ITER>` | iLSQR max iterations of each LSQR solve. |
| `--ilsqr-pad-mm <MM>` | iLSQR zero-padding in mm per side. |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance. |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR max iterations. |
| `--heidi-cone-threshold <HEIDI_CONE_THRESHOLD>` | \|D(k)\| above which a coefficient is well-conditioned and kept from the LSQR seed. |
| `--heidi-gradient-threshold <HEIDI_GRADIENT_THRESHOLD>` | Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous. |
| `--no-heidi-laplacian-correction` | Do not open the homogeneity masks where the field's Laplacian is large. |
| `--heidi-laplacian-threshold <HEIDI_LAPLACIAN_THRESHOLD>` | Field-Laplacian threshold (ppm per voxel²) |
| `--heidi-gradient-mask-floor <HEIDI_GRADIENT_MASK_FLOOR>` | TV weight kept where every direction reads as an edge (0 reproduces upstream's shipped binarised masks) |
| `--heidi-continuation-steps <HEIDI_CONTINUATION_STEPS>` | NESTA continuation steps on μ. |
| `--heidi-inner-iterations <HEIDI_INNER_ITERATIONS>` | Accelerated-gradient iterations per continuation step. |
| `--heidi-mu-min <HEIDI_MU_MIN>` | Final (smallest) smoothing parameter μ. |
| `--heidi-tol <HEIDI_TOL>` | Relative objective change that ends an inner loop early (the default runs every iteration) |
| `--no-heidi-denoise` | Skip the anisotropic-diffusion smoothing of the field before its gradients are taken. |
| `--heidi-denoise-iterations <HEIDI_DENOISE_ITERATIONS>` | Anisotropic-diffusion iterations. |
| `--heidi-denoise-time-step <HEIDI_DENOISE_TIME_STEP>` | Anisotropic-diffusion time step. |
| `--heidi-denoise-conductance <HEIDI_DENOISE_CONDUCTANCE>` | Anisotropic-diffusion conductance. |
| `--qsmart-ilsqr-max-iter <QSMART_ILSQR_MAX_ITER>` | QSMART iLSQR max iterations per stage. The iLSQR tolerance is --ilsqr-tol. |
| `--qsmart-ilsqr-pad-mm <MM>` | Zero-padding of QSMART's inner iLSQR solves, in mm per side (default: 6) |
| `--qsmart-vasc-sphere-radius <QSMART_VASC_SPHERE_RADIUS>` | QSMART vasculature detection sphere radius. |
| `--qsmart-inversion <QSMART_INVERSION>` | QSMART inner dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--qsmart-sdf-sigma1-stage1 <QSMART_SDF_SIGMA1_STAGE1>` | QSMART SDF sigma1, stage 1 (voxels) |
| `--qsmart-sdf-sigma2-stage1 <QSMART_SDF_SIGMA2_STAGE1>` | QSMART SDF sigma2, stage 1 (voxels) |
| `--qsmart-sdf-sigma1-stage2 <QSMART_SDF_SIGMA1_STAGE2>` | QSMART SDF sigma1, stage 2 (voxels) |
| `--qsmart-sdf-sigma2-stage2 <QSMART_SDF_SIGMA2_STAGE2>` | QSMART SDF sigma2, stage 2 (voxels) |
| `--qsmart-sdf-lower-lim <QSMART_SDF_LOWER_LIM>` | QSMART SDF proximity lower limit. |
| `--qsmart-sdf-curv-constant <QSMART_SDF_CURV_CONSTANT>` | QSMART SDF curvature constant. |
| `--qsmart-frangi-scale-min <QSMART_FRANGI_SCALE_MIN>` | QSMART Frangi min vessel radius (mm) |
| `--qsmart-frangi-scale-max <QSMART_FRANGI_SCALE_MAX>` | QSMART Frangi max vessel radius (mm) |
| `--qsmart-frangi-scale-ratio <QSMART_FRANGI_SCALE_RATIO>` | QSMART Frangi scale step (mm) |
| `--qsmart-frangi-c <QSMART_FRANGI_C>` | QSMART Frangi C noise threshold. |
| `--ndi-tau <NDI_TAU>` | NDI gradient-descent step size (tau) |
| `--ndi-alpha <NDI_ALPHA>` | NDI L2 regularization weight (alpha) |
| `--ndi-max-iter <NDI_MAX_ITER>` | NDI max iterations. |
| `--ndi-auto-stop <NDI_AUTO_STOP>` | NDI: stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto); --ndi-max-iter is then the maximum. |
| `--fansi-alpha1 <FANSI_ALPHA1>` | FANSI first-order (TV/TGV) L1 penalty weight (alpha1) |
| `--fansi-mu1 <FANSI_MU1>` | FANSI gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--fansi-mu2 <FANSI_MU2>` | FANSI fidelity-consistency ADMM weight (mu2) |
| `--fansi-alpha0 <FANSI_ALPHA0>` | FANSI second-order L1 penalty weight (alpha0, nlTGV only; default: 2 × alpha1) |
| `--fansi-mu0 <FANSI_MU0>` | FANSI second-order consistency ADMM weight (mu0, nlTGV only; default: 2 × mu1) |
| `--fansi-max-iter <FANSI_MAX_ITER>` | FANSI outer ADMM iterations. |
| `--fansi-tol-update <FANSI_TOL_UPDATE>` | FANSI percent-update convergence tolerance. |
| `--fansi-tol-delta <FANSI_TOL_DELTA>` | FANSI inner Newton convergence tolerance. |
| `--l1qsm-alpha1 <L1QSM_ALPHA1>` | L1-QSM gradient (TV) L1 penalty weight (alpha1) |
| `--l1qsm-mu1 <L1QSM_MU1>` | L1-QSM gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--l1qsm-mu2 <L1QSM_MU2>` | L1-QSM fidelity-consistency ADMM weight (mu2) |
| `--l1qsm-mu3 <L1QSM_MU3>` | L1-QSM L1 proximal ADMM weight (mu3) |
| `--l1qsm-lambda <L1QSM_LAMBDA>` | L1-QSM L1 fidelity strength (lambda) |
| `--l1qsm-max-iter <L1QSM_MAX_ITER>` | L1-QSM outer ADMM iterations. |
| `--l1qsm-tol-update <L1QSM_TOL_UPDATE>` | L1-QSM percent-update convergence tolerance. |
| `--l1qsm-tol-delta <L1QSM_TOL_DELTA>` | L1-QSM inner Newton convergence tolerance. |
| `--whqsm-alpha1 <WHQSM_ALPHA1>` | WH-QSM TV regularization weight (alpha1) |
| `--whqsm-mu1 <WHQSM_MU1>` | WH-QSM ADMM penalty for TV splitting (mu1) |
| `--whqsm-mu2 <WHQSM_MU2>` | WH-QSM ADMM penalty for data-fidelity splitting (mu2) |
| `--whqsm-beta <WHQSM_BETA>` | WH-QSM weak-harmonic ROI penalty (beta) |
| `--whqsm-muh <WHQSM_MUH>` | WH-QSM ADMM penalty for harmonic-field splitting (muh) |
| `--whqsm-max-iter <WHQSM_MAX_ITER>` | WH-QSM max outer iterations. |
| `--whqsm-tol-update <WHQSM_TOL_UPDATE>` | WH-QSM percent-update convergence tolerance. |
| `--whqsm-tol-delta <WHQSM_TOL_DELTA>` | WH-QSM inner Newton convergence tolerance. |
| `--hdqsm-alpha-l2 <HDQSM_ALPHA_L2>` | HD-QSM L2-stage TV weight (alpha_l2) |
| `--hdqsm-mu1-l2 <HDQSM_MU1_L2>` | HD-QSM L2-stage gradient-consistency ADMM weight (mu1_l2; default: 100 × alpha_l2) |
| `--hdqsm-mu2 <HDQSM_MU2>` | HD-QSM fidelity consistency weight (mu2) |
| `--hdqsm-max-iter-l1 <HDQSM_MAX_ITER_L1>` | HD-QSM stage-1 (L1) iterations (max_iter_l1) |
| `--hdqsm-max-iter-l2 <HDQSM_MAX_ITER_L2>` | HD-QSM stage-2 (L2) iterations (max_iter_l2) |
| `--hdqsm-tol-update <HDQSM_TOL_UPDATE>` | HD-QSM stage-2 percent-update convergence tolerance. |
| `--amp-pe-wave-order <AMP_PE_WAVE_ORDER>` | AMP-PE Daubechies wavelet order (1=db1, 2=db2) |
| `--amp-pe-nlevel <AMP_PE_NLEVEL>` | AMP-PE wavelet decomposition levels. |
| `--amp-pe-wave-pec <AMP_PE_WAVE_PEC>` | AMP-PE morphology-mask energy retention fraction (0.0-1.0) |
| `--amp-pe-simulated-te <AMP_PE_SIMULATED_TE>` | AMP-PE simulated echo time (s) used to turn the field into phase. |
| `--amp-pe-max-linearization-ite <AMP_PE_MAX_LINEARIZATION_ITE>` | AMP-PE linearization iterations per stage. |
| `--amp-pe-damp-rate-sig <AMP_PE_DAMP_RATE_SIG>` | AMP-PE GAMP signal-update damping rate. |
| `--amp-pe-damp-rate-par <AMP_PE_DAMP_RATE_PAR>` | AMP-PE parameter-estimation learning rate (kappa) |
| `--amp-pe-max-pe-spar-ite <AMP_PE_MAX_PE_SPAR_ITE>` | AMP-PE inner sparse-reconstruction iterations. |
| `--amp-pe-max-pe-est-ite <AMP_PE_MAX_PE_EST_ITE>` | AMP-PE inner parameter-estimation iterations. |
| `--amp-pe-cvg-thd <AMP_PE_CVG_THD>` | AMP-PE GAMP inner convergence threshold. |
| `--amp-pe-tikhonov-beta <AMP_PE_TIKHONOV_BETA>` | AMP-PE L2-seed Tikhonov weight. |
| `--r2star-qsm-r-const-3t <R2STAR_QSM_R_CONST_3T>` | R2\*-QSM relaxometric constant at 3T (Hz/ppm) |
| `--decompose-n-inner <DECOMPOSE_N_INNER>` | DECOMPOSE inner alternating passes per voxel. |
| `--decompose-chi-bound <DECOMPOSE_CHI_BOUND>` | DECOMPOSE upper bound on \|χ\| in the fit (ppm) |
| `--decompose-max-lm-iter <DECOMPOSE_MAX_LM_ITER>` | DECOMPOSE Levenberg–Marquardt max iterations. |
| `--chi-sep-ilsqr-dr-pos <CHI_SEP_ILSQR_DR_POS>` | χ-sep iLSQR paramagnetic relaxometric constant (Hz/ppm) |
| `--chi-sep-ilsqr-dr-neg <CHI_SEP_ILSQR_DR_NEG>` | χ-sep iLSQR diamagnetic relaxometric constant (Hz/ppm) |
| `--chi-sep-ilsqr-lambda1 <CHI_SEP_ILSQR_LAMBDA1>` | χ-sep iLSQR L1 edge-masked TV weight. |
| `--chi-sep-ilsqr-percentage <CHI_SEP_ILSQR_PERCENTAGE>` | χ-sep iLSQR edge-mask keep fraction (0-1) |
| `--chi-sep-ilsqr-r2p-min <CHI_SEP_ILSQR_R2P_MIN>` | χ-sep iLSQR R2' reliability window lower (Hz) |
| `--chi-sep-ilsqr-r2p-max <CHI_SEP_ILSQR_R2P_MAX>` | χ-sep iLSQR R2' reliability window upper (Hz) |
| `--chi-sep-ilsqr-max-iter <CHI_SEP_ILSQR_MAX_ITER>` | χ-sep iLSQR outer Gauss-Newton iterations. |
| `--chi-sep-ilsqr-tol <CHI_SEP_ILSQR_TOL>` | χ-sep iLSQR outer relative-change tolerance. |
| `--chi-sep-ilsqr-cg-max-iter <CHI_SEP_ILSQR_CG_MAX_ITER>` | χ-sep iLSQR inner CG max iterations. |
| `--chi-sep-ilsqr-cg-tol <CHI_SEP_ILSQR_CG_TOL>` | χ-sep iLSQR inner CG relative tolerance. |
| `--chi-sep-medi-lambda-para <CHI_SEP_MEDI_LAMBDA_PARA>` | χ-sep MEDI paramagnetic L1 weight. |
| `--chi-sep-medi-lambda-dia <CHI_SEP_MEDI_LAMBDA_DIA>` | χ-sep MEDI diamagnetic L1 weight. |
| `--chi-sep-medi-lambda-cpl <CHI_SEP_MEDI_LAMBDA_CPL>` | χ-sep MEDI field/R2' coupling weight. |
| `--chi-sep-medi-dr-pos <CHI_SEP_MEDI_DR_POS>` | χ-sep MEDI paramagnetic relaxivity (Hz/ppm) |
| `--chi-sep-medi-dr-neg <CHI_SEP_MEDI_DR_NEG>` | χ-sep MEDI diamagnetic relaxivity (Hz/ppm) |
| `--chi-sep-medi-percentage <CHI_SEP_MEDI_PERCENTAGE>` | χ-sep MEDI edge-mask percentage (0-1) |
| `--chi-sep-medi-cg-tol <CHI_SEP_MEDI_CG_TOL>` | χ-sep MEDI inner CG tolerance. |
| `--chi-sep-medi-cg-max-iter <CHI_SEP_MEDI_CG_MAX_ITER>` | χ-sep MEDI inner CG max iterations. |
| `--chi-sep-medi-max-iter <CHI_SEP_MEDI_MAX_ITER>` | χ-sep MEDI outer Gauss-Newton iterations. |
| `--chi-sep-medi-tol <CHI_SEP_MEDI_TOL>` | χ-sep MEDI outer convergence tolerance. |
| `--wavesep-dr-pos <WAVESEP_DR_POS>` | WaveSep paramagnetic relaxivity (Hz/ppm) |
| `--wavesep-dr-neg <WAVESEP_DR_NEG>` | WaveSep diamagnetic relaxivity (Hz/ppm) |
| `--wavesep-alpha <WAVESEP_ALPHA>` | WaveSep proximal-gradient step size. |
| `--wavesep-lambda <WAVESEP_LAMBDA>` | WaveSep wavelet L1 soft-threshold weight. |
| `--wavesep-wavelet-order <WAVESEP_WAVELET_ORDER>` | WaveSep Daubechies wavelet order. |
| `--wavesep-max-iter <WAVESEP_MAX_ITER>` | WaveSep ISTA max iterations. |
| `--wavesep-tol <WAVESEP_TOL>` | WaveSep relative-change stop tolerance. |
| `--hc-chisep-dr-pos-3t <HC_CHISEP_DR_POS_3T>` | HC-ChiSep paramagnetic relaxivity at 3T (Hz/ppm) |
| `--hc-chisep-bin-hz <HC_CHISEP_BIN_HZ>` | HC-ChiSep R2' bin width for the anchored grid search (Hz) |
| `--chi-sepnet-patch <D> <H> <W>` | χ-sepnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--susep-net-patch <D> <H> <W>` | Run SUSEP-Net as a sliding window of patches this size in voxels, D H W, each a multiple of 8, so its memory follows the patch, not the volume. Omit to run the whole volume at once. |
| `--motion-correction` | Rigidly co-register every echo onto a reference echo (estimated on magnitude, applied to phase through the complex signal) before field mapping, and restrict the mask to voxels every echo covers. Off by default. Needs magnitude images. |
| `--motion-reference <first\|last\|N>` | Echo the others are aligned onto: `first` (default), `last`, or a 1-based echo number. |
| `--no-motion-chain` | Register every echo from the identity instead of seeding it from its neighbour's solution. |
| `--motion-chain-levels <MOTION_CHAIN_LEVELS>` | Pyramid levels for a seeded (chained) registration (default 3) |
| `--motion-min-ncc <MOTION_MIN_NCC>` | NCC below which an echo's registration is reported as suspect (default 0.3, set for echoes; ~0.9 suits repeats) |
| `--registration-levels <REGISTRATION_LEVELS>` | Registration pyramid levels: the capture range; do not lower (default 5) |
| `--registration-max-samples <REGISTRATION_MAX_SAMPLES>` | Registration: voxels sampled per metric evaluation, per level (default 200000) |
| `--registration-max-iterations <REGISTRATION_MAX_ITERATIONS>` | Registration: Powell cycles per level (default 20) |
| `--registration-max-evaluations <REGISTRATION_MAX_EVALUATIONS>` | Registration: metric evaluations across all levels, a hard stop (default 5000) |
| `--registration-tolerance <REGISTRATION_TOLERANCE>` | Registration: relative cost improvement per cycle that counts as converged (default 1e-5) |
| `--registration-rotation-step-deg <REGISTRATION_ROTATION_STEP_DEG>` | Registration: initial rotation step at the finest level, degrees (default 3) |
| `--registration-translation-step-mm <REGISTRATION_TRANSLATION_STEP_MM>` | Registration: initial translation step at the finest level, mm (default 3) |
| `--registration-min-overlap <REGISTRATION_MIN_OVERLAP>` | Registration: smallest fraction of sampled voxels that must overlap (default 0.25) |
| `--vsharp-deconvolve <BOOL>` | Undo V-SHARP's SMV high-pass with a truncated-SVD deconvolution (default: false) |
| `--vsharp-threshold <VSHARP_THRESHOLD>` | V-SHARP deconvolution (TSVD) threshold; only used with --vsharp-deconvolve true. |
| `--vsharp-max-radius <VSHARP_MAX_RADIUS>` | V-SHARP max kernel radius in mm. |
| `--vsharp-min-radius <VSHARP_MIN_RADIUS>` | V-SHARP min kernel radius in mm. |
| `--vsharp-grid <VSHARP_GRID>` | Grid V-SHARP estimates the background on: `native` (default), `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened), or `isotropic` (spacing from --vsharp-grid-spacing). The local field keeps native resolution. Values: `native`, `finest`, `isotropic`. |
| `--vsharp-grid-spacing <MM>` | V-SHARP isotropic working-grid spacing in mm (default 1); used with --vsharp-grid isotropic. |
| `--pdf-tol <PDF_TOL>` | PDF tolerance, relative to the noise level. |
| `--lbv-tol <LBV_TOL>` | LBV tolerance. |
| `--ismv-tol <ISMV_TOL>` | iSMV tolerance. |
| `--ismv-max-iter <ISMV_MAX_ITER>` | iSMV max iterations. |
| `--ismv-radius <ISMV_RADIUS>` | iSMV kernel radius in mm. |
| `--sharp-threshold <SHARP_THRESHOLD>` | SHARP threshold. |
| `--sharp-radius <SHARP_RADIUS>` | SHARP kernel radius in mm. |
| `--resharp-radius <RESHARP_RADIUS>` | RESHARP SMV kernel radius in mm. |
| `--resharp-tik-reg <RESHARP_TIK_REG>` | RESHARP Tikhonov regularization parameter. |
| `--resharp-tol <RESHARP_TOL>` | RESHARP CG convergence tolerance. |
| `--resharp-max-iter <RESHARP_MAX_ITER>` | RESHARP maximum CG iterations. |
| `--harperella-radius <HARPERELLA_RADIUS>` | HARPERELLA SMV kernel radius in mm. |
| `--harperella-max-iter <HARPERELLA_MAX_ITER>` | HARPERELLA maximum iterations. |
| `--harperella-tol <HARPERELLA_TOL>` | HARPERELLA convergence tolerance. |
| `--iharperella-max-iter <IHARPERELLA_MAX_ITER>` | iHARPERELLA maximum iterations. |
| `--iharperella-tol <IHARPERELLA_TOL>` | iHARPERELLA convergence tolerance. |
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) |
| `--romeo-individual` | ROMEO: use individual per-echo unwrapping (default) |
| `--no-romeo-individual` | ROMEO: use template-based temporal unwrapping (disables individual mode) |
| `--romeo-correct-global` | ROMEO: enable inter-echo 2π offset correction (default: off) |
| `--no-romeo-correct-global` | ROMEO: disable inter-echo 2π offset correction. |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0.5) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: maximum number of seed regions (default: 1) |
| `--romeo-merge-regions` | ROMEO: merge neighboring regions after unwrapping (default: off) |
| `--romeo-correct-regions` | ROMEO: correct each region's median to nearest 0 (default: off) |
| `--romeo-wrap-addition <ROMEO_WRAP_ADDITION>` | ROMEO: additional phase tolerance beyond π for neighbor differences [0,π] (default: 0.0) |
| `--swi-unwrapping <SWI_UNWRAPPING>` | SWI phase unwrapping before the echoes are combined [default: laplacian]. Values: `laplacian`, `laplacian-slice`, `romeo`. |
| `--swi-hp-sigma <SWI_HP_SIGMA>` | SWI high-pass filter sigma (3 values, in voxels) |
| `--swi-scaling <SWI_SCALING>` | SWI phase-mask scaling. Values: `tanh`, `negative-tanh`, `positive`, `negative`, `triangular`. |
| `--swi-strength <SWI_STRENGTH>` | SWI phase scaling strength. |
| `--swi-mip-window <SWI_MIP_WINDOW>` | SWI MIP window size in slices. |
| `--smwi-threshold <SMWI_THRESHOLD>` | SMWI susceptibility threshold in ppm — the \|χ\| at which the weighting mask reaches zero. The 1 ppm default follows SEPIA and targets strong sources; brain tissue wants far less. |
| `--smwi-power <SMWI_POWER>` | SMWI mask power (contrast strength) |
| `--smwi-mip-window <SMWI_MIP_WINDOW>` | SMWI mIP window size in slices. |
| `--synthseg-version <VERSION>` | SynthSeg weights generation: v1 (32 labels, ships with SynthSeg) or v2 (33 labels, adds CSF; weights not published yet, so refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR) |
| `--synthseg-crop <SYNTHSEG_CROP>` | Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32). Omit to process the whole volume, as the SynthSeg CLI does. |
| `--synthseg-patch <VOXELS>` | Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the volume. Omit to run the whole volume at once, as the SynthSeg CLI does. |
| `--no-synthseg-flip-averaging` | Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does. |
| `--no-synthseg-topology-cleanup` | Skip resetting each topological class to its largest connected component. |
| `--synthseg-sigma <SYNTHSEG_SIGMA>` | Gaussian blur on the posteriors, in voxels (0 disables it) |
| `--use-custom-dseg <TOOL>` | Prefer a bring-your-own segmentation from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ when present. Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--homogeneity-sigma-mm <HOMOGENEITY_SIGMA_MM>` | Inhomogeneity correction smoothing sigma in mm. |
| `--homogeneity-nbox <HOMOGENEITY_NBOX>` | Inhomogeneity correction box filter passes. |
| `--linear-fit-reliability-threshold <LINEAR_FIT_RELIABILITY_THRESHOLD>` | Linear and robust fit: percentile for the fit's reliability mask (default 90; 0 disables) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--no-qsm` | Skip QSM processing (only run supplementary outputs like SWI, T2\*, R2\*) |
| `--do-swi` | Also compute SWI. |
| `--do-segmentation` | Also segment the brain from the GRE magnitude with SynthSeg, writing a FreeSurfer-labelled `_dseg.nii` and its BIDS lookup table. Needs no T1w and no registration. Needs a deep-learning build. doi:10.1016/j.media.2023.102789. |
| `--do-analysis` | Also summarise every quantitative map (χ, χ+/χ−, T2\*, R2\*, R2, R2') per segmented structure into a TSV: mean, SD, median, min, max and the 5th/95th percentiles, one row per map and structure, plus a dataset-level table gathering all runs. Summarises whatever the run made — it implies --do-segmentation, and QSM only if no other map was asked for. |
| `--do-smwi` | Also compute SMWI (susceptibility map-weighted imaging): weights the magnitude by a mask built from the susceptibility map rather than from high-pass filtered phase, so the contrast stays where the source is. Needs χ, so it implies QSM. doi:10.1002/mrm.24920. |
| `--do-t2starmap` | Compute T2\* relaxation map from multi-echo magnitude data. |
| `--do-r2starmap` | Compute R2\* decay rate map from multi-echo magnitude data. |
| `--do-r2map` | Compute R2 map from a multi-echo spin-echo (MESE) acquisition (EPG) |
| `--do-r2primemap` | Compute R2' map (= R2\* − R2; needs GRE magnitude + a MESE acquisition) |
| `--do-chisep` | Compute chi-separation (paramagnetic/diamagnetic susceptibility maps) |
| `--chisep <CHI_SEPARATION_ALGORITHM>` | Chi-separation method (default: r2star-qsm) Values: `r2star-qsm`, `decompose`, `chi-sep-ilsqr`, `chi-sep-medi`, `wavesep`, `hc-chisep`, `susep-net`, `chi-sepnet`. |
| `--use-custom-qsm <TOOL>` | Use a bring-your-own QSM (Chimap) from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--use-custom-r2 <TOOL>` | Use a bring-your-own R2 map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--use-custom-r2prime <TOOL>` | Use a bring-your-own R2' map from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ for chi-separation. |
| `--r2prime-strategy <R2PRIME_STRATEGY>` | How to obtain R2' when no custom map is supplied. `auto` (default) measures it as R2\* - R2 from a MESE acquisition when one is present and predicts it from R2\* with R2PRIMEnet when it is not; `mese` only ever measures it, producing nothing without spin-echo data; `r2primenet` always predicts it. A measurement and an estimate are not the same thing — the generated methods text says which route was configured. Values: `auto`, `mese`, `r2primenet`. |
| `--r2primenet-ignore-field-mismatch` | Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its weights were trained at; the predicted R2' is then uncalibrated. Without it, an explicit `--r2prime-strategy r2primenet` refuses a non-3 T run and `auto` skips R2' with a warning. |
| `--r2primenet-patch <D> <H> <W>` | R2PRIMEnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors') |
| `--export-dicom` | Also export final maps as DICOM series into each subject's extra_files/ folder. |
| `--inhomogeneity-correction` | Apply inhomogeneity correction to magnitude before masking. |
| `--no-inhomogeneity-correction` | Disable inhomogeneity correction. |
| `--axial-resampling` | Resample oblique acquisitions onto an axial grid when their obliquity exceeds --obliquity-threshold. Off by default (methods that only work on axial data resample regardless) |
| `--no-axial-resampling` | Do not resample oblique acquisitions to axial (overrides a config's axial_resampling) |
| `--obliquity-threshold <OBLIQUITY_THRESHOLD>` | Obliquity in degrees above which --axial-resampling resamples (default: 10) |
| `--crop-to-mask` | Reconstruct inside a box around the mask instead of the whole field of view. Cuts the FFT-bound stages roughly in proportion to the voxels removed, but those stages are a small share of a full run, so the end-to-end gain is usually modest. Off by default. Outputs are written on the full grid either way; this only changes where the arithmetic happens. Padding to an FFT-friendly size happens regardless. |
| `--ignore-slice-gap` | Reconstruct even when a 2D acquisition's sidecar `SliceThickness` is thinner than the slice pitch. Without it, QSM refuses such a run: the gap between slices is unsampled tissue, and the FFT kernels assume a contiguous grid, so the map would be wrong. |
| `--fft-padding` | Pad the grid outward to an FFT-friendly size before the FFT-bound stages. Much faster on an awkward grid, and discards nothing, but it changes where k-space is sampled and so shifts the reconstruction slightly. Off by default. |
| `--crop-margin-mm <CROP_MARGIN_MM>` | Margin in mm around the mask when --crop-to-mask is set (default 32). Below ~32 mm the reconstruction starts to differ from the uncropped one; it must also clear the largest SMV kernel in use, and V-SHARP defaults to 12 mm. |
| `--output-space <OUTPUT_SPACE>` | Grid to write derivatives on when the run was resampled to axial. `acquired` (default) puts them back where the data came from, so transforms you already hold still apply; `working` leaves them on the resampled grid and avoids a second interpolation. Values: `acquired`, `working`. |
| `--mask-preset <MASK_PRESET>` | Mask preset: robust-threshold, bet, hd-bet (HD-BET deep-learning brain extraction + signal-gated erosion; needs a deep-learning build), bet-and-phase (BET on the magnitude AND a thresholded phase-quality map, then hole-filled and eroded), rs2-net (RS2-Net deep-learning rodent brain extraction), or mindgrab (MindGrab deep-learning brain extraction for any contrast; needs a deep-learning build) Values: `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab`. |
| `--use-custom-masks <USE_CUSTOM_MASKS>` | Prefer a bring-your-own mask from BIDS derivatives when present (else compute it). Bare flag = first matching derivatives tool (alphabetical); give a name (e.g. `bet`) to restrict to `derivatives/<tool>/sub-*/anat/*_mask.nii*`. Always falls back. |
| `--mask <MASK_SECTIONS_CLI>` | Define a mask section (repeatable; multiple sections are combined with --mask-combine). Format: &lt;input&gt;,&lt;generator&gt;,&lt;refinement1&gt;,&lt;refinement2&gt;,... Example: phase-quality,threshold:otsu,dilate:2,fill-holes:0,erode:2 Example: magnitude,bet:0.5,erode:2 Example: magnitude,hd-bet,signal-erode (HD-BET: `hd-bet[:full\|window\|XxYxZ\|low-memory][:tta][:step=F]`; signal-erode: `signal-erode[:threshold[:depth_cap[:global_erosions[:bias_sigma[:min_component]]]]]`) |
| `--mask-combine <MASK_COMBINE>` | How to combine multiple --mask sections: `or` (union, the default) or `and` (intersection — a voxel survives only where every section keeps it) Values: `or`, `and`. |
| `--mask-refine <MASK_REFINEMENTS_CLI>` | Refinement applied to the combined mask, after --mask-combine (repeatable). Takes the same refinement ops as --mask, e.g. `erode:2` or `fill-holes:0`; generators (threshold/bet/hd-bet) belong in a --mask section instead. Example: --mask-combine and --mask-refine fill-holes:0 --mask-refine erode:1. |
| `--two-pass` | Two-pass artefact reduction: reconstruct a second time against a mask whose holes are left unfilled, and keep that reconstruction wherever it is defined. Strong susceptibility sources (air-tissue interfaces, haemorrhage, implants) make the phase around them unrecoverable; leaving those voxels out keeps their streaking from spreading, while the ordinary mask still supplies values inside the holes. Roughly doubles background removal and inversion time, and is not free of downsides — where the holes were not artefactual it can be worse than a single pass. Based on doi:10.1002/mrm.29048 (Stewart et al.). Off by default. The conventional single-pass map is kept alongside as `_desc-singlepass_Chimap.nii`. |
| `--two-pass-mask <TWO_PASS_SECTIONS_CLI>` | Define the reliable-pass mask used by --two-pass (repeatable; same format as --mask). Defaults to `phase-quality,threshold:otsu` — an unfilled threshold on the phase-quality map. Its holes are the point, so refinements that close them (fill-holes, close, gaussian) defeat the method. Folded with --mask-combine and --mask-refine like the main mask, and always intersected with it. |
| `--debug` | Enable debug logging. |
| `--force` | Recompute the multi-orientation reconstructions, ignoring their cached state. |
| `--clean-intermediates` | Once the dataset-level steps are done, delete the intermediate images under workflow/ (as `qsmxt run --clean-intermediates`) |

## `qsmxt mask`

Masking operations

| Subcommand | Description |
| --- | --- |
| [`otsu`](#qsmxt-mask-otsu) | Otsu automatic thresholding |
| [`value`](#qsmxt-mask-value) | Fixed value thresholding |
| [`percentile`](#qsmxt-mask-percentile) | Percentile thresholding |
| [`bet`](#qsmxt-mask-bet) | Brain extraction (BET) |
| [`hd-bet`](#qsmxt-mask-hd-bet) | Deep-learning brain extraction (HD-BET; downloads weights on first use) |
| [`rs2-net`](#qsmxt-mask-rs2-net) | Deep-learning rodent brain extraction (RS2-Net; downloads weights on first use) |
| [`mindgrab`](#qsmxt-mask-mindgrab) | Deep-learning brain extraction for any contrast (MindGrab; downloads weights on first use) |
| [`preset`](#qsmxt-mask-preset) | Run a `--mask-preset` recipe on an image (robust-threshold, bet, hd-bet, bet-and-phase, rs2-net, mindgrab) |
| [`robust`](#qsmxt-mask-robust) | Alias for `preset robust-threshold` |
| [`erode`](#qsmxt-mask-erode) | Erode a binary mask |
| [`dilate`](#qsmxt-mask-dilate) | Dilate a binary mask |
| [`close`](#qsmxt-mask-close) | Morphological closing on a binary mask |
| [`fill-holes`](#qsmxt-mask-fill-holes) | Fill holes in a binary mask |
| [`smooth`](#qsmxt-mask-smooth) | Gaussian smooth a binary mask (re-thresholds at 0.5) |
| [`and`](#qsmxt-mask-and) | Intersect binary masks: keep voxels every input keeps (the `run --mask-combine and`) |
| [`or`](#qsmxt-mask-or) | Union binary masks: keep voxels any input keeps (the `run --mask-combine or`) |

### `qsmxt mask otsu`

Otsu automatic thresholding

```sh
qsmxt mask otsu [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask value`

Fixed value thresholding

```sh
qsmxt mask value [OPTIONS] --output <OUTPUT> --threshold <THRESHOLD> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--percentile <PERCENTILE>` | Percentile value (0-100) Required. |

### `qsmxt mask bet`

Brain extraction (BET)

```sh
qsmxt mask bet [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--fractional-intensity <FRACTIONAL_INTENSITY>` | Fractional intensity (0.0-1.0, smaller = larger brain) Default: `0.5`. |
| `--voxel-scale <FACTOR>` | Factor applied to the voxel sizes before BET runs — 10 for mouse data, 1 for the acquisition's own geometry (see --bet-voxel-scale). Prefer `mask rs2-net`, which masks rodent brains without the trick. |

### `qsmxt mask hd-bet`

Deep-learning brain extraction (HD-BET; downloads weights on first use)

```sh
qsmxt mask hd-bet [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--quality <QUALITY>` | Phase-quality map (from `qsmxt quality-map`) for the sections that read phase quality. Without it those sections read the input image instead, as `run --masking-input` would. |

### `qsmxt mask robust`

Alias for `preset robust-threshold`

```sh
qsmxt mask robust [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |

### `qsmxt mask erode`

Erode a binary mask

```sh
qsmxt mask erode [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
| `--radius <RADIUS>` | Closing radius. Default: `1`. |

### `qsmxt mask fill-holes`

Fill holes in a binary mask

```sh
qsmxt mask fill-holes [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input NIfTI file (an image for the generating subcommands, a mask for the rest) Required. |
| `-o, --output <OUTPUT>` | Output mask NIfTI file. Required. |
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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
| `--op <OPS>` | Further refinement, applied in order after the subcommand's own operation (repeatable). Examples: erode:2, dilate:1, fill-holes:0, close:1, gaussian:4.0, signal-erode (signal-gated erosion of low-signal boundary voxels; needs a magnitude input) |
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

## `qsmxt combine`

Multi-coil phase combination (uncombined receive-coil channels -&gt; combined echoes)

| Subcommand | Description |
| --- | --- |
| [`mcpc3ds`](#qsmxt-combine-mcpc3ds) | MCPC-3D-S (ASPIRE) coil combination from per-coil multi-echo phase + magnitude |

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
| `--hip-unwrapping <HIP_UNWRAPPING>` | Unwrapping used on the coil-summed HIP phase. Values: `romeo`, `laplacian`. Default: `romeo`. |

## `qsmxt unwrap`

Phase unwrapping

| Subcommand | Description |
| --- | --- |
| [`romeo`](#qsmxt-unwrap-romeo) | ROMEO region-growing unwrapping |
| [`laplacian`](#qsmxt-unwrap-laplacian) | Laplacian-based unwrapping |

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
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) |
| `--romeo-individual` | ROMEO: use individual per-echo unwrapping (default) |
| `--no-romeo-individual` | ROMEO: use template-based temporal unwrapping (disables individual mode) |
| `--romeo-correct-global` | ROMEO: enable inter-echo 2π offset correction (default: off) |
| `--no-romeo-correct-global` | ROMEO: disable inter-echo 2π offset correction. |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0.5) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: maximum number of seed regions (default: 1) |
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

## `qsmxt fieldmap`

B0 field mapping from multi-echo phase (NIfTI in, ppm out)

| Subcommand | Description |
| --- | --- |
| [`romeo`](#qsmxt-fieldmap-romeo) | ROMEO-based multi-echo field mapping (phase offset removal + unwrap + B0 fit) |
| [`laplacian`](#qsmxt-fieldmap-laplacian) | Laplacian-based multi-echo field mapping (per-echo unwrap + echo combination) |

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
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method. Values: `weighted-avg`, `linear-fit`, `robust-fit`. Default: `weighted-avg`. |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted-averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) from the echo times and an assumed T2\* (no T2\* map, no magnitude; Wang et al. 2022). `phase-snr` is generally preferable with magnitude. Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. Default: `phase-snr`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` (no T2\* map is used) Default: `40`. |
| `--linear-fit-reliability-threshold <LINEAR_FIT_RELIABILITY_THRESHOLD>` | Linear and robust fit: percentile for the fit's reliability mask (default 90; 0 disables) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--phase-offset-removal <PHASE_OFFSET_REMOVAL>` | Enable phase offset removal for multi-echo data (default: true) |
| `--phase-offset-sigma-mm <PHASE_OFFSET_SIGMA_MM>` | Phase offset removal smoothing sigma (3 values, in mm; default: 7 7 7, the ROMEO command line's) |
| `--bipolar-correction` | Enable bipolar gradient correction (requires &gt;= 3 echoes) |
| `--no-romeo-phase-coherence` | ROMEO: disable phase coherence weights (default: on) |
| `--no-romeo-phase-gradient-coherence` | ROMEO: disable phase gradient coherence weights (default: on) |
| `--no-romeo-phase-linearity` | ROMEO: disable phase linearity weights (default: on) |
| `--no-romeo-mag-coherence` | ROMEO: disable magnitude coherence weights (default: on) |
| `--romeo-mag-weight` | ROMEO: enable magnitude weighting (default: off) |
| `--no-romeo-mag-weight` | ROMEO: disable magnitude weighting (overrides --romeo-mag-weight; kept for compatibility) |
| `--romeo-mag-weight2` | ROMEO: enable second magnitude weighting (flow-artifact penalty; default: off) |
| `--romeo-bestpath` | ROMEO: use Best-path (Abdul-Rahman) weights instead of ROMEO weights (default: off) |
| `--romeo-template <ROMEO_TEMPLATE>` | ROMEO: template echo index (1-indexed, only for template mode) |
| `--romeo-individual` | ROMEO: use individual per-echo unwrapping (default) |
| `--no-romeo-individual` | ROMEO: use template-based temporal unwrapping (disables individual mode) |
| `--romeo-correct-global` | ROMEO: enable inter-echo 2π offset correction (default: off) |
| `--no-romeo-correct-global` | ROMEO: disable inter-echo 2π offset correction. |
| `--romeo-temporal-uncertain-unwrapping <ROMEO_TEMPORAL_UNCERTAIN_UNWRAPPING>` | ROMEO: temporal uncertain-unwrapping quality threshold [0,1] (0 disables; default: 0.5) |
| `--romeo-max-seeds <ROMEO_MAX_SEEDS>` | ROMEO: maximum number of seed regions (default: 1) |
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
| `--b0-estimation <B0_ESTIMATION>` | B0 field estimation method. Values: `weighted-avg`, `linear-fit`, `robust-fit`. Default: `weighted-avg`. |
| `--b0-weight-type <B0_WEIGHT_TYPE>` | B0 weighted-averaging weight type. `assumed-decay` gives each echo's phase a fixed weight TE·exp(−TE/T2\*) from the echo times and an assumed T2\* (no T2\* map, no magnitude; Wang et al. 2022). `phase-snr` is generally preferable with magnitude. Values: `phase-snr`, `phase-var`, `average`, `tes`, `mag`, `assumed-decay`. Default: `phase-snr`. |
| `--b0-weight-assumed-t2star <MS>` | T2\* in ms assumed by `--b0-weight-type assumed-decay` (no T2\* map is used) Default: `40`. |
| `--linear-fit-reliability-threshold <LINEAR_FIT_RELIABILITY_THRESHOLD>` | Linear and robust fit: percentile for the fit's reliability mask (default 90; 0 disables) |
| `--linear-fit-estimate-offset <LINEAR_FIT_ESTIMATE_OFFSET>` | Linear and robust fit: estimate a phase offset term (default: true). The robust fit needs 4 echoes with it, 3 without. |
| `--robust-fit-loss <ROBUST_FIT_LOSS>` | Motion-robust fit (`--b0-estimation robust-fit`): loss turning residuals into weights. `tukey` (default) rejects echoes past the tuning radius; `huber` caps but never rejects. Values: `tukey`, `huber`. |
| `--robust-fit-tuning <ROBUST_FIT_TUNING>` | Motion-robust fit: tuning radius in robust-scale units (default: the loss's conventional value, Tukey 4.685 / Huber 1.345). Smaller is more aggressive. |
| `--robust-fit-iterations <ROBUST_FIT_ITERATIONS>` | Motion-robust fit: IRLS passes (default 3; 0 = report only, the field is the linear fit's) |
| `--robust-fit-downweight-level <ROBUST_FIT_DOWNWEIGHT_LEVEL>` | Motion-robust fit (reporting only): a robust weight below this counts as down-weighted (default 0.75; in (0, 1]) |
| `--robust-fit-flag-ratio <ROBUST_FIT_FLAG_RATIO>` | Motion-robust fit (reporting only): flag an echo whose outlier score exceeds this many times the best echo's (default 3; at least 1) |
| `--laplacian-solver <LAPLACIAN_SOLVER>` | Poisson solver: `dct` (unweighted least squares, Neumann/DCT solve) or `fft` (Schofield & Zhu, FFT solve on a zero-padded volume) Values: `dct`, `fft`. Default: `dct`. |
| `--laplacian-fft-pad <VOXELS>` | Zero-padding in voxels per side for `--laplacian-solver fft`: one value for every axis, or three (x y z) Default: `64`. |

## `qsmxt bgremove`

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
| `--deconvolve <BOOL>` | Undo the SMV high-pass with a truncated-SVD deconvolution (default: false) |
| `--threshold <THRESHOLD>` | Deconvolution (TSVD) threshold; only used with --deconvolve true. |
| `--max-radius <MAX_RADIUS>` | Largest SMV kernel radius, in mm. |
| `--min-radius <MIN_RADIUS>` | Smallest SMV kernel radius, in mm. |
| `--grid <GRID>` | Grid the background is estimated on: `native` (default), `finest` (isotropic at the smallest voxel dimension, e.g. 0.8 mm for 0.8x0.8x3 mm; no axis is coarsened), or `isotropic` (spacing from --grid-spacing). The local field keeps native resolution. Values: `native`, `finest`, `isotropic`. |
| `--grid-spacing <MM>` | Isotropic working-grid spacing in mm (default 1); used with --grid isotropic. |

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
| `--tol <TOL>` | Tolerance, relative to the noise level. |

### `qsmxt bgremove lbv`

Laplacian Boundary Value

```sh
qsmxt bgremove lbv [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input total field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output local field NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--output-mask <OUTPUT_MASK>` | Output eroded mask (for algorithms that erode) |
| `--tol <TOL>` | Tolerance. |

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
| `--tol <TOL>` | Tolerance. |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--radius <RADIUS>` | Radius factor (multiplied by max voxel size) |

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
| `--threshold <THRESHOLD>` | Threshold. |
| `--radius <RADIUS>` | Radius factor (multiplied by min voxel size) |

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
| `--radius <RADIUS>` | SMV kernel radius in mm. |
| `--tik-reg <TIK_REG>` | Tikhonov regularization parameter. |
| `--tol <TOL>` | CG convergence tolerance. |
| `--max-iter <MAX_ITER>` | Maximum CG iterations. |

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
| `--radius <RADIUS>` | SMV kernel radius in mm. |
| `--max-iter <MAX_ITER>` | Maximum iterations. |
| `--tol <TOL>` | Convergence tolerance. |
| `--te <TE>` | Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped echo in rad (the published method); without both, it is an unwrapped total field in ppm. The local field is written in ppm either way. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength (T) of a wrapped-phase input — see --te. |

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
| `--max-iter <MAX_ITER>` | Maximum iterations. |
| `--tol <TOL>` | Convergence tolerance. |
| `--te <TE>` | Echo time (s) of a wrapped-phase input. With --field-strength, the input is one wrapped echo in rad, unwrapped by HARPERELLA's Laplacian; without both, it is an unwrapped total field in ppm, which iHARPERELLA takes as it is. The local field is written in ppm either way. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength (T) of a wrapped-phase input — see --te. |

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

## `qsmxt invert`

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
| [`lsqr`](#qsmxt-invert-lsqr) | Minimally regularised LSQR |
| [`heidi`](#qsmxt-invert-heidi) | HEIDI (Homogeneity Enabled Incremental Dipole Inversion), seeded by an LSQR solve |
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
| [`autoqsm`](#qsmxt-invert-autoqsm) | AutoQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use) |
| [`qsmgan`](#qsmxt-invert-qsmgan) | QSMGAN deep-learning dipole inversion (weights downloaded on first use) |
| [`ir2qsm`](#qsmxt-invert-ir2qsm) | IR2QSM deep-learning dipole inversion (weights downloaded on first use) |
| [`lpcnn`](#qsmxt-invert-lpcnn) | LPCNN deep-learning dipole inversion (weights downloaded on first use) |
| [`modl-qsm`](#qsmxt-invert-modl-qsm) | MoDL-QSM deep-learning dipole inversion → χ33/STI component (weights downloaded on first use) |
| [`nextqsm`](#qsmxt-invert-nextqsm) | NeXtQSM single-step reconstruction — input is the TOTAL field (weights downloaded on first use) |
| [`cosmos`](#qsmxt-invert-cosmos) | COSMOS — multi-orientation least squares over 2+ co-registered orientations |
| [`sti`](#qsmxt-invert-sti) | STI (susceptibility tensor imaging) — 6+ co-registered orientations to a rank-2 tensor |

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
| `--delta <DELTA>` | Delta parameter. |
| `--mu <MU>` | Mu parameter. |
| `--tol <TOL>` | Tolerance. |
| `--rho <RHO>` | Rho (ADMM penalty) |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--lsmr-iter <LSMR_ITER>` | LSMR iterations. |

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
| `--lambda <LAMBDA>` | Lambda parameter. |
| `--rho <RHO>` | Rho (ADMM penalty; default: 100 × lambda) |
| `--tol <TOL>` | Tolerance. |
| `--max-iter <MAX_ITER>` | Max iterations. |

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
| `--threshold <THRESHOLD>` | Threshold. |

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
| `--threshold <THRESHOLD>` | Threshold. |

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
| `--iterations <ITERATIONS>` | Iterations (default: from the voxel size, as the reference does) |
| `--erosions <EROSIONS>` | Erosions. |
| `--alpha1 <ALPHA1>` | Alpha1 (first-order weight), in radians at the first echo time and the field strength. |
| `--alpha0 <ALPHA0>` | Alpha0 (second-order weight), in radians at the first echo time and the field strength. |
| `--step-size <STEP_SIZE>` | Primal step size multiplier. |

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
| `--lambda <LAMBDA>` | Lambda. |

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
| `--lambda <LAMBDA>` | Lambda. |
| `--mu <MU>` | Mu (penalty parameter) |
| `--tol <TOL>` | Tolerance. |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--newton-iter <NEWTON_ITER>` | Maximum Newton iterations of the data-term subproblem. |

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
| `--lambda <LAMBDA>` | Lambda: the data-fidelity weight (larger fits the field more closely) |
| `--merit <MERIT>` | Enable MERIT weighting. |
| `--smv <SMV>` | High-pass the local field with a spherical mean value filter, removing residual background near the brain edge (MEDI-SMV) [default: on]. |
| `--smv-radius <SMV_RADIUS>` | SMV radius in mm (also mSMV's) |
| `--smv-shrink-mask <SMV_SHRINK_MASK>` | With SMV, shrink the mask to where the SMV of the mask exceeds 0.999; false keeps the full mask, high-passing the field over it [default: on]. |
| `--msmv` | MEDI-mSMV (Roberts et al. 2024): filter the local field by maximum spherical mean value first, and fit it with the SMV-filtered dipole kernel. Needs at least 3 echoes. |
| `--msmv-maxk <MSMV_MAXK>` | MEDI-mSMV: the boundary correction makes at most this many passes, less one. |
| `--msmv-vessel-radius <MSMV_VESSEL_RADIUS>` | MEDI-mSMV: largest vessel thickness of the vessel mask, in voxels (default: 15, or 8 × the largest voxel dimension if that is more) |
| `--no-msmv-prefilter` | MEDI-mSMV: no SMV prefilter before mSMV. Only consistent for a field already high-passed by the same SMV; after PDF, V-SHARP or LBV it diverges (expert) |
| `--data-weighting <DATA_WEIGHTING>` | Data weighting mode (0=uniform, 1=SNR) |
| `--percentage <PERCENTAGE>` | Edge percentage (0.0-1.0) |
| `--cg-tol <CG_TOL>` | CG tolerance. |
| `--cg-max-iter <CG_MAX_ITER>` | CG max iterations. |
| `--max-iter <MAX_ITER>` | Max outer iterations. |
| `--tol <TOL>` | Outer tolerance. |

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
| `--lambda <LAMBDA>` | Lambda: the data-fidelity weight (larger fits the field more closely) |
| `--precond <PRECOND>` | Preconditioner value (susceptibility scaling outside the mask) |
| `--data-weighting <DATA_WEIGHTING>` | Data weighting mode (0=uniform, 1=SNR) |
| `--percentage <PERCENTAGE>` | Edge percentage (0.0-1.0) |
| `--cg-tol <CG_TOL>` | CG tolerance. |
| `--cg-max-iter <CG_MAX_ITER>` | CG max iterations. |
| `--max-iter <MAX_ITER>` | Max outer iterations. |
| `--tol <TOL>` | Outer tolerance. |

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
| `--tol <TOL>` | Tolerance. |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--pad-mm <MM>` | Zero-padding in mm per side. |

### `qsmxt invert lsqr`

Minimally regularised LSQR

```sh
qsmxt invert lsqr [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI (optional row weight; its scale does not change the solution): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. |
| `--b0 <B0>` | B0 field strength in Tesla — sets the default residual weighting. Default: `3`. |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance. |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR max iterations. |

### `qsmxt invert heidi`

HEIDI (Homogeneity Enabled Incremental Dipole Inversion), seeded by an LSQR solve

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
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI (optional row weight of the LSQR seed): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. |
| `--lsqr-residual-weighting <LSQR_RESIDUAL_WEIGHTING>` | LSQR residual-field weight `w` — the only thing regularising the solve. Omit to use qsm-core's field-strength-scaled default; 0 drops the residual term and streaks badly. |
| `--no-lsqr-global-offset` | Do not fit a global field offset alongside χ. |
| `--lsqr-tol <LSQR_TOL>` | LSQR convergence tolerance. |
| `--lsqr-max-iter <LSQR_MAX_ITER>` | LSQR max iterations. |
| `--heidi-cone-threshold <HEIDI_CONE_THRESHOLD>` | \|D(k)\| above which a coefficient is well-conditioned and kept from the LSQR seed. |
| `--heidi-gradient-threshold <HEIDI_GRADIENT_THRESHOLD>` | Field-gradient threshold (ppm/mm); below it a direction reads as homogeneous. |
| `--no-heidi-laplacian-correction` | Do not open the homogeneity masks where the field's Laplacian is large. |
| `--heidi-laplacian-threshold <HEIDI_LAPLACIAN_THRESHOLD>` | Field-Laplacian threshold (ppm per voxel²) |
| `--heidi-gradient-mask-floor <HEIDI_GRADIENT_MASK_FLOOR>` | TV weight kept where every direction reads as an edge (0 reproduces upstream's shipped binarised masks) |
| `--heidi-continuation-steps <HEIDI_CONTINUATION_STEPS>` | NESTA continuation steps on μ. |
| `--heidi-inner-iterations <HEIDI_INNER_ITERATIONS>` | Accelerated-gradient iterations per continuation step. |
| `--heidi-mu-min <HEIDI_MU_MIN>` | Final (smallest) smoothing parameter μ. |
| `--heidi-tol <HEIDI_TOL>` | Relative objective change that ends an inner loop early (the default runs every iteration) |
| `--no-heidi-denoise` | Skip the anisotropic-diffusion smoothing of the field before its gradients are taken. |
| `--heidi-denoise-iterations <HEIDI_DENOISE_ITERATIONS>` | Anisotropic-diffusion iterations. |
| `--heidi-denoise-time-step <HEIDI_DENOISE_TIME_STEP>` | Anisotropic-diffusion time step. |
| `--heidi-denoise-conductance <HEIDI_DENOISE_CONDUCTANCE>` | Anisotropic-diffusion conductance. |

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
| `--tau <TAU>` | Gradient-descent step size (tau) |
| `--alpha <ALPHA>` | L2 regularization weight (alpha) |
| `--max-iter <MAX_ITER>` | Max iterations (the maximum, with --auto-stop) |
| `--auto-stop <AUTO_STOP>` | Stop automatically once noise starts to build up along the dipole cone (FANSI ndi_auto) |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Required. |
| `--echo-times <ECHO_TIMES>` | Echo times in seconds, every echo's, in order. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI, the data weight (required): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. Required. |

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
| `--alpha1 <ALPHA1>` | First-order (TV/TGV) L1 penalty weight (alpha1) |
| `--mu1 <MU1>` | Gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--mu2 <MU2>` | Fidelity-consistency ADMM weight (mu2) |
| `--alpha0 <ALPHA0>` | Second-order L1 penalty weight (alpha0, nlTGV only; default: 2 × alpha1) |
| `--mu0 <MU0>` | Second-order consistency ADMM weight (mu0, nlTGV only; default: 2 × mu1) |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--tol-update <TOL_UPDATE>` | Percent-update convergence tolerance. |
| `--tol-delta <TOL_DELTA>` | Inner Newton convergence tolerance. |

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
| `--alpha1 <ALPHA1>` | First-order (TGV gradient) L1 penalty weight (alpha1) |
| `--mu1 <MU1>` | Gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--mu2 <MU2>` | Fidelity-consistency ADMM weight (mu2) |
| `--alpha0 <ALPHA0>` | Second-order L1 penalty weight (alpha0; default: 2 × alpha1) |
| `--mu0 <MU0>` | Second-order consistency ADMM weight (mu0; default: 2 × mu1) |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--tol-update <TOL_UPDATE>` | Percent-update convergence tolerance. |
| `--tol-delta <TOL_DELTA>` | Inner Newton convergence tolerance. |

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
| `--alpha1 <ALPHA1>` | Gradient (TV) L1 penalty weight (alpha1) |
| `--mu1 <MU1>` | Gradient-consistency ADMM weight (mu1; default: 100 × alpha1) |
| `--mu2 <MU2>` | Fidelity-consistency ADMM weight (mu2) |
| `--mu3 <MU3>` | L1 proximal ADMM weight (mu3) |
| `--lambda <LAMBDA>` | L1 fidelity strength (lambda) |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--tol-update <TOL_UPDATE>` | Percent-update convergence tolerance. |
| `--tol-delta <TOL_DELTA>` | Inner Newton convergence tolerance. |

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
| `--alpha1 <ALPHA1>` | TV regularization weight (alpha1) |
| `--mu1 <MU1>` | ADMM penalty for TV splitting (mu1) |
| `--mu2 <MU2>` | ADMM penalty for data-fidelity splitting (mu2) |
| `--beta <BETA>` | Weak-harmonic ROI penalty (beta) |
| `--muh <MUH>` | ADMM penalty for harmonic-field splitting (muh) |
| `--max-iter <MAX_ITER>` | Max iterations. |
| `--tol-update <TOL_UPDATE>` | Percent-update convergence tolerance. |
| `--tol-delta <TOL_DELTA>` | Inner Newton convergence tolerance. |

### `qsmxt invert hdqsm`

Hybrid Data-Fidelity QSM

```sh
qsmxt invert hdqsm [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input local field NIfTI file. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output susceptibility map NIfTI file. Required. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--tile-size <TILE_SIZE>` | DL tiling: output core size per patch, in voxels (enables tiling) |
| `--tile-halo <TILE_HALO>` | DL tiling: context margin per side, in voxels (default 8) |
| `--magnitude <MAGNITUDE>` | Magnitude NIfTI (optional data weight): one 3D volume, a 4D one, or one per echo; several echoes are RSS-combined. |
| `--alpha-l2 <ALPHA_L2>` | L2-stage TV weight (alpha_l2) |
| `--mu1-l2 <MU1_L2>` | L2-stage gradient-consistency ADMM weight (mu1_l2; default: 100 × alpha_l2) |
| `--mu2 <MU2>` | Fidelity consistency weight (mu2) |
| `--max-iter-l1 <MAX_ITER_L1>` | Stage-1 (L1) iterations (max_iter_l1) |
| `--max-iter-l2 <MAX_ITER_L2>` | Stage-2 (L2) iterations (max_iter_l2) |
| `--tol-update <TOL_UPDATE>` | Stage-2 percent-update convergence tolerance. |

### `qsmxt invert amp-pe`

AMP-PE (Approximate Message Passing with Parameter Estimation)

```sh
qsmxt invert amp-pe [OPTIONS] --mask <MASK> --output <OUTPUT> <INPUT>
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
| `--b0 <B0>` | B0 field strength in Tesla (scales the simulated phase) Default: `3`. |
| `--wave-order <WAVE_ORDER>` | Daubechies wavelet order (1=db1, 2=db2) |
| `--nlevel <NLEVEL>` | Wavelet decomposition levels. |
| `--wave-pec <WAVE_PEC>` | Morphology-mask energy retention fraction (0.0-1.0) |
| `--simulated-te <SIMULATED_TE>` | Simulated echo time (s) used to turn the field into phase. |
| `--max-linearization-ite <MAX_LINEARIZATION_ITE>` | Linearization iterations per stage. |
| `--damp-rate-sig <DAMP_RATE_SIG>` | GAMP signal-update damping rate. |
| `--damp-rate-par <DAMP_RATE_PAR>` | Parameter-estimation learning rate (kappa) |
| `--max-pe-spar-ite <MAX_PE_SPAR_ITE>` | Inner sparse-reconstruction iterations. |
| `--max-pe-est-ite <MAX_PE_EST_ITE>` | Inner parameter-estimation iterations. |
| `--cvg-thd <CVG_THD>` | GAMP inner convergence threshold. |
| `--tikhonov-beta <TIKHONOV_BETA>` | L2-seed Tikhonov weight. |

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

## `qsmxt separate`

Susceptibility source separation

| Subcommand | Description |
| --- | --- |
| [`r2star-qsm`](#qsmxt-separate-r2star-qsm) | R2\*-QSM closed-form (Dimov 2022) — QSM + R2\* (GRE-only) |
| [`decompose`](#qsmxt-separate-decompose) | DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude (GRE-only) |
| [`chi-sep-ilsqr`](#qsmxt-separate-chi-sep-ilsqr) | χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM |
| [`chi-sep-medi`](#qsmxt-separate-chi-sep-medi) | χ-separation MEDI — local field + R2' + magnitude + QSM |
| [`wavesep`](#qsmxt-separate-wavesep) | WaveSep wavelet-L1 (Fang 2023) — QSM + R2' |
| [`hc-chisep`](#qsmxt-separate-hc-chisep) | Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude |
| [`susep-net`](#qsmxt-separate-susep-net) | SUSEP-Net deep-learning separation — QSM + R2' + local field (weights downloaded on first use) |
| [`chi-sepnet`](#qsmxt-separate-chi-sepnet) | χ-sepnet deep-learning separation — QSM + R2' + local field (weights downloaded on first use) |

### `qsmxt separate r2star-qsm`

R2\*-QSM closed-form (Dimov 2022) — QSM + R2\* (GRE-only)

```sh
qsmxt separate r2star-qsm [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> <--r2star <R2STAR>|--magnitude <MAGNITUDE>...>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--r2star <R2STAR>` | R2\* map NIfTI in Hz (else fit from --magnitude + --echo-times) |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude: one 4D file or several 3D files (space-separated). Fits R2\* if --r2star absent. |
| `--echo-times <ECHO_TIMES>` | GRE echo times in seconds (space- or comma-separated), required with --magnitude. |
| `--r-const-3t <R_CONST_3T>` | Relaxometric constant at 3T (Hz/ppm) |

### `qsmxt separate decompose`

DECOMPOSE-QSM signal-domain fit (Chen 2021) — QSM + multi-echo magnitude (GRE-only)

```sh
qsmxt separate decompose [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --magnitude <MAGNITUDE>... --echo-times <ECHO_TIMES>...
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude: one 4D file or several 3D files (space-separated) Required. |
| `--echo-times <ECHO_TIMES>` | GRE echo times in seconds (space- or comma-separated) Required. |
| `--n-inner <N_INNER>` | Inner alternating passes per voxel. |
| `--chi-bound <CHI_BOUND>` | Upper bound on \|χ\| in the fit (ppm) |
| `--max-lm-iter <MAX_LM_ITER>` | Levenberg–Marquardt max iterations. |

### `qsmxt separate chi-sep-ilsqr`

χ-separation iLSQR (Shin 2021) — local field + R2' + magnitude + QSM

```sh
qsmxt separate chi-sep-ilsqr [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --local-field <LOCAL_FIELD> --r2prime <R2PRIME> --magnitude <MAGNITUDE>...
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude for SNR weighting + edge mask: one 4D file or several 3D files (RSS-combined) Required. |
| `--dr-pos <DR_POS>` | Paramagnetic relaxometric constant (Hz/ppm) |
| `--dr-neg <DR_NEG>` | Diamagnetic relaxometric constant (Hz/ppm) |
| `--lambda1 <LAMBDA1>` | L1 edge-masked TV weight. |
| `--percentage <PERCENTAGE>` | Edge-mask keep fraction (0-1) |
| `--r2p-min <R2P_MIN>` | R2' reliability window lower (Hz) |
| `--r2p-max <R2P_MAX>` | R2' reliability window upper (Hz) |
| `--max-iter <MAX_ITER>` | Outer Gauss-Newton iterations. |
| `--tol <TOL>` | Outer relative-change tolerance. |
| `--cg-max-iter <CG_MAX_ITER>` | Inner CG max iterations. |
| `--cg-tol <CG_TOL>` | Inner CG relative tolerance. |

### `qsmxt separate chi-sep-medi`

χ-separation MEDI — local field + R2' + magnitude + QSM

```sh
qsmxt separate chi-sep-medi [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --local-field <LOCAL_FIELD> --r2prime <R2PRIME> --magnitude <MAGNITUDE>...
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--magnitude <MAGNITUDE>` | Magnitude for edge weighting: one 4D file or several 3D files (RSS-combined) Required. |
| `--lambda-para <LAMBDA_PARA>` | Paramagnetic L1 weight. |
| `--lambda-dia <LAMBDA_DIA>` | Diamagnetic L1 weight. |
| `--lambda-cpl <LAMBDA_CPL>` | Field/R2' coupling weight. |
| `--dr-pos <DR_POS>` | Paramagnetic relaxivity (Hz/ppm) |
| `--dr-neg <DR_NEG>` | Diamagnetic relaxivity (Hz/ppm) |
| `--percentage <PERCENTAGE>` | Edge-mask percentage (0-1) |
| `--cg-tol <CG_TOL>` | Inner CG tolerance. |
| `--cg-max-iter <CG_MAX_ITER>` | Inner CG max iterations. |
| `--max-iter <MAX_ITER>` | Outer Gauss-Newton iterations. |
| `--tol <TOL>` | Outer convergence tolerance. |

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
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--dr-pos <DR_POS>` | Paramagnetic relaxivity (Hz/ppm) |
| `--dr-neg <DR_NEG>` | Diamagnetic relaxivity (Hz/ppm) |
| `--alpha <ALPHA>` | Proximal-gradient step size. |
| `--lambda <LAMBDA>` | Wavelet L1 soft-threshold weight. |
| `--wavelet-order <WAVELET_ORDER>` | Daubechies wavelet order. |
| `--max-iter <MAX_ITER>` | ISTA max iterations. |
| `--tol <TOL>` | Relative-change stop tolerance. |

### `qsmxt separate hc-chisep`

Hollow-cylinder χ-separation (Stewart 2026) — QSM + R2' + multi-echo magnitude

```sh
qsmxt separate hc-chisep [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --r2prime <R2PRIME> --magnitude <MAGNITUDE>... --echo-times <ECHO_TIMES>...
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--magnitude <MAGNITUDE>` | Multi-echo magnitude: one 4D file or several 3D files (space-separated) Required. |
| `--echo-times <ECHO_TIMES>` | GRE echo times in seconds (space- or comma-separated) Required. |
| `--se-magnitude <SE_MAGNITUDE>` | Multi-echo spin-echo magnitude: one 4D file or several 3D files (optional) |
| `--se-echo-times <SE_ECHO_TIMES>` | Spin-echo times in seconds (comma-separated) for --se-magnitude. |
| `--dr-pos-3t <DR_POS_3T>` | Paramagnetic relaxivity at 3T (Hz/ppm) |
| `--bin-hz <BIN_HZ>` | R2' bin width for the anchored grid search (Hz) |

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
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--patch <D> <H> <W>` | Sliding-window patch in voxels, D H W. χ-sepnet: each a multiple of 16 (default: 192 192 128, the authors'). SUSEP-Net: each a multiple of 8 (default: the whole volume at once) |

### `qsmxt separate chi-sepnet`

χ-sepnet deep-learning separation — QSM + R2' + local field (weights downloaded on first use)

```sh
qsmxt separate chi-sepnet [OPTIONS] --qsm <QSM> --mask <MASK> --output <OUTPUT> --local-field <LOCAL_FIELD> --r2prime <R2PRIME>
```

| Argument | Description |
| --- | --- |
| `--qsm <QSM>` | Conventional QSM (χ_total) NIfTI in ppm. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output prefix; writes {prefix}_desc-paramagnetic_Chimap.nii, {prefix}_desc-diamagnetic_Chimap.nii and {prefix}_desc-total_Chimap.nii — the pipeline's names. Required. |
| `--field-strength <FIELD_STRENGTH>` | B0 field strength in Tesla. Default: `3`. |
| `--b0-direction <B0_DIRECTION>` | B0 direction (3 values) Default: `0 0 1`. |
| `--local-field <LOCAL_FIELD>` | Local (tissue) field NIfTI in ppm. Required. |
| `--r2prime <R2PRIME>` | R2' map NIfTI in Hz. Required. |
| `--patch <D> <H> <W>` | Sliding-window patch in voxels, D H W. χ-sepnet: each a multiple of 16 (default: 192 192 128, the authors'). SUSEP-Net: each a multiple of 8 (default: the whole volume at once) |

## `qsmxt qsmart`

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
| `--magnitude <MAGNITUDE>` | Combined magnitude NIfTI (optional; drives vasculature detection) |
| `--qsmart-ilsqr-max-iter <QSMART_ILSQR_MAX_ITER>` | QSMART iLSQR max iterations per stage. The iLSQR tolerance is --ilsqr-tol. |
| `--qsmart-ilsqr-pad-mm <MM>` | Zero-padding of QSMART's inner iLSQR solves, in mm per side (default: 6) |
| `--qsmart-vasc-sphere-radius <QSMART_VASC_SPHERE_RADIUS>` | QSMART vasculature detection sphere radius. |
| `--qsmart-inversion <QSMART_INVERSION>` | QSMART inner dipole inversion algorithm (default: ilsqr) Values: `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `tfi`, `ilsqr`, `lsqr`, `heidi`, `qsmart`, `ndi`, `fansi`, `fansi-tgv`, `l1qsm`, `whqsm`, `hdqsm`, `amp-pe`, `xqsm`, `qsmnet`, `qsmnet-plus`, `autoqsm`, `qsmgan`, `ir2qsm`, `lpcnn`, `modl-qsm`, `nextqsm`, `iqsm`, `iqsm-plus`. |
| `--qsmart-sdf-sigma1-stage1 <QSMART_SDF_SIGMA1_STAGE1>` | QSMART SDF sigma1, stage 1 (voxels) |
| `--qsmart-sdf-sigma2-stage1 <QSMART_SDF_SIGMA2_STAGE1>` | QSMART SDF sigma2, stage 1 (voxels) |
| `--qsmart-sdf-sigma1-stage2 <QSMART_SDF_SIGMA1_STAGE2>` | QSMART SDF sigma1, stage 2 (voxels) |
| `--qsmart-sdf-sigma2-stage2 <QSMART_SDF_SIGMA2_STAGE2>` | QSMART SDF sigma2, stage 2 (voxels) |
| `--qsmart-sdf-lower-lim <QSMART_SDF_LOWER_LIM>` | QSMART SDF proximity lower limit. |
| `--qsmart-sdf-curv-constant <QSMART_SDF_CURV_CONSTANT>` | QSMART SDF curvature constant. |
| `--qsmart-frangi-scale-min <QSMART_FRANGI_SCALE_MIN>` | QSMART Frangi min vessel radius (mm) |
| `--qsmart-frangi-scale-max <QSMART_FRANGI_SCALE_MAX>` | QSMART Frangi max vessel radius (mm) |
| `--qsmart-frangi-scale-ratio <QSMART_FRANGI_SCALE_RATIO>` | QSMART Frangi scale step (mm) |
| `--qsmart-frangi-c <QSMART_FRANGI_C>` | QSMART Frangi C noise threshold. |

## `qsmxt swi`

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
| `--swi-unwrapping <SWI_UNWRAPPING>` | SWI phase unwrapping before the echoes are combined [default: laplacian]. Values: `laplacian`, `laplacian-slice`, `romeo`. |
| `--swi-hp-sigma <SWI_HP_SIGMA>` | SWI high-pass filter sigma (3 values, in voxels) |
| `--swi-scaling <SWI_SCALING>` | SWI phase-mask scaling. Values: `tanh`, `negative-tanh`, `positive`, `negative`, `triangular`. |
| `--swi-strength <SWI_STRENGTH>` | SWI phase scaling strength. |
| `--swi-mip-window <SWI_MIP_WINDOW>` | SWI MIP window size in slices. |

## `qsmxt segment`

Whole-brain segmentation of a magnitude image with SynthSeg

```sh
qsmxt segment [OPTIONS] --output <OUTPUT> <MAGNITUDE>
```

| Argument | Description |
| --- | --- |
| `<MAGNITUDE>` | Input magnitude NIfTI file. Not a susceptibility map: SynthSeg reads anatomy, and χ has none of the tissue contrast it was trained to recognise. Required. |
| `-o, --output <OUTPUT>` | Output label NIfTI file (FreeSurfer label ids) Required. |
| `--lookup <LOOKUP>` | Also write the BIDS label lookup table (index/name) to this path. |
| `--synthseg-version <VERSION>` | SynthSeg weights generation: v1 (32 labels, ships with SynthSeg) or v2 (33 labels, adds CSF; weights not published yet, so refused unless synthseg-2.0.onnx is in $QSM_MODEL_DIR) |
| `--synthseg-crop <SYNTHSEG_CROP>` | Cubic centre-crop in voxels at 1 mm before inference (rounded up to a multiple of 32). Omit to process the whole volume, as the SynthSeg CLI does. |
| `--synthseg-patch <VOXELS>` | Run SynthSeg as a sliding window of half-overlapping cubes this many voxels on a side at 1 mm (at least 64, rounded up to a multiple of 32), so its memory follows the patch, not the volume. Omit to run the whole volume at once, as the SynthSeg CLI does. |
| `--no-synthseg-flip-averaging` | Skip the left-right flipped second pass. Halves inference time, as SynthSeg's --fast does. |
| `--no-synthseg-topology-cleanup` | Skip resetting each topological class to its largest connected component. |
| `--synthseg-sigma <SYNTHSEG_SIGMA>` | Gaussian blur on the posteriors, in voxels (0 disables it) |
| `--use-custom-dseg <TOOL>` | Prefer a bring-your-own segmentation from &lt;bids&gt;/derivatives/&lt;TOOL&gt;/ when present. Bare flag = first matching tool alphabetically. Always falls back to running SynthSeg. |

## `qsmxt r2star`

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

## `qsmxt t2star`

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

## `qsmxt r2`

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

## `qsmxt r2prime`

R2' mapping (R2' = R2\* - R2)

```sh
qsmxt r2prime [OPTIONS] --r2star <R2STAR> --r2 <R2> --mask <MASK> --output <OUTPUT>
```

| Argument | Description |
| --- | --- |
| `--r2star <R2STAR>` | R2\* map NIfTI in Hz. Required. |
| `--r2 <R2>` | R2 map NIfTI in Hz. Required. |
| `-m, --mask <MASK>` | Binary mask NIfTI file. Required. |
| `--r2-coverage <R2_COVERAGE>` | Binary mask of where the R2 acquisition reached, if it does not cover the whole mask (a MESE slab). Without it, coverage is inferred from R2 &gt; 0. |
| `-o, --output <OUTPUT>` | Output R2' map NIfTI file. Required. |
| `--output-coverage <OUTPUT_COVERAGE>` | Also write the mask of voxels where R2' is a measurement. |

## `qsmxt homogeneity`

Inhomogeneity correction on magnitude data

```sh
qsmxt homogeneity [OPTIONS] --output <OUTPUT> <INPUT>
```

| Argument | Description |
| --- | --- |
| `<INPUT>` | Input magnitude NIfTI file. Required. |
| `-o, --output <OUTPUT>` | Output corrected magnitude NIfTI file. Required. |
| `--sigma <SIGMA>` | Smoothing sigma in mm (default: 7.0) Default: `7`. |
| `--nbox <NBOX>` | Number of box filter passes for Gaussian approximation (default: 3) Default: `3`. |

## `qsmxt resample`

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
| `--no-noise-fill` | Do not fill the empty corners of the new grid with seeded noise. Only the --phase / --magnitude pair is noise-filled (it seeds ROMEO); a plain volume's corners are zero. |

## `qsmxt quality-map`

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

## `qsmxt example`

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

## `qsmxt tui`

Launch interactive TUI for pipeline configuration

```sh
qsmxt tui
```

## `qsmxt update`

Check for updates and optionally install the latest version

```sh
qsmxt update [OPTIONS]
```

| Argument | Description |
| --- | --- |
| `-y, --yes` | Update without prompting for confirmation. |
| `--force` | Replace a development build (version 0.0.0) with the latest release. Without it, `qsmxt update` refuses on a dev build, since every release would look newer. |
