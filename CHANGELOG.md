# Changelog

Notes for the next release, collected as pull requests land. Each entry says whether default
output changes, so the release notes can lead with what makes results incomparable across the
upgrade.

## Unreleased

### Output changes with default settings

- **iLSQR is the default dipole inversion, replacing RTS.** `qsmxt run` without
  `--qsm-algorithm`, the TUI, `qsmxt slurm` and `InversionConfig::default()` (so also TOML/JSON
  configs, including qsmbly's, that leave the algorithm unset) now use iLSQR, STI Suite's algorithm, with
  QSM.rs's STI Suite-matched parameters (`--ilsqr-tol`, `--ilsqr-max-iter`). On a 150-subject UK Biobank
  pilot iLSQR tracked age and longitudinal change better than RTS and matched UKB's own pipeline;
  RTS kept marginally higher iron test-retest ICC. RTS stays available as `--qsm-algorithm rts`
  and is ~15x faster. Every default run's χ changes.

- **`qsmxt slurm` now gives the same results as a local run** (#269). Each job used to run
  `qsmxt run --include <one run>`. So COSMOS/STI groups never formed (each job saw one member),
  and every job rewrote `qsmxt.log`, `desc-reference_summary.tsv`, the statistics table, cohort
  figures, `prov/`, `dataset_description.json` and `methods.md` from its own run alone (the last
  job to finish won). Each run job now runs only its run's pipeline (`qsmxt run --no-dataset-steps`,
  logging to `logs/<run>.log`). A final aggregation job (`qsmxt aggregate`, submitted with
  `--dependency=afterok:` on every run job) then does the dataset-level steps through the same
  code as `qsmxt run`. `--num-echoes`, `--force` and `--clean-intermediates` now reach the jobs.
  Each job's `--mem` is estimated from its run unless `--mem` is given. Job logs are written
  beside the scripts. `--submit` chains the jobs, and so does the generated `slurm/submit.sh`.
  The TUI's SLURM mode passes force and clean-up too.
- **Output names are valid BIDS, and the standalone commands use the pipeline's** (#275).
  Breaking renames (old → new):
  - STI: `desc-sti11_Chitensor` … `desc-sti33_Chitensor` (six files) → one 4D
    `desc-sti_Chimap` (components X11 X12 X13 X22 X23 X33 along the fourth axis, listed in its
    sidecar as `TensorComponents`); `desc-pev{x,y,z}_Chitensor` → one 4D `desc-sti_evec`
    (`.bidsignore`d: an eigenvector is not χ, and BIDS has no suffix for it).
    `desc-mms_Chimap` and `desc-msa_Chimap` are unchanged.
  - SMWI minIP: `desc-paramagnetic_minIP` / `desc-diamagnetic_minIP` →
    `desc-paramagneticsmwi_minIP` / `desc-diamagneticsmwi_minIP` (the `desc-` names what was
    projected, so it is not mistaken for the SWI's). `_swi`, `_minIP` and `desc-*_smwi` keep
    BEP004's proposed suffixes, and `_R2primemap` stays (no BIDS suffix for R2′); all are listed in
    `.bidsignore`.
  - `--output-space working` on a resampled (oblique) run: `<name>` → `space-axial` in the name
    (`sub-01_Chimap` → `sub-01_space-axial_Chimap`, `part-mag_T2starw` →
    `part-mag_space-axial_T2starw`), with a `SpatialReference` in each sidecar. Runs never
    resampled, and the default acquired grid, keep their names.
  - Runs with both `inv-` and `run-`: `…_inv-1_run-2_…` → `…_run-2_inv-1_…` (BIDS entity order).
  - Two runs that differ only by suffix (a `MEGRE` and a `GRE` in one session) wrote the same
    files; each now folds its suffix into `acq-` (`acq-MEGRE`, `acq-GRE`; with an existing `acq-fast`, `acq-fastGRE`). Runs without such
    a twin are unchanged.
  - `workflow/` intermediates: `_field-ppm` → `_desc-totalppm_fieldmap`, `_Chimap-raw` →
    `_desc-unreferenced_Chimap`, `_echo-N_phase-scaled` → `_echo-N_part-phase_desc-scaled_<suffix>`,
    `_echo-N_mag` → `_echo-N_part-mag_<suffix>`.
  - `qsmxt invert sti`: `<prefix>_tensor-11` … `_tensor-33`, `_mms`, `_msa`, `_pev-x/y/z` →
    `<prefix>_desc-sti_Chimap` (4D), `_desc-mms_Chimap`, `_desc-msa_Chimap`, `_desc-sti_evec` (4D).
    `qsmxt separate`: `<prefix>_paramagnetic/_diamagnetic/_total` →
    `<prefix>_desc-{paramagnetic,diamagnetic,total}_Chimap`. `qsmxt combine mcpc3ds`:
    `<prefix>_echo-N_part-{phase,mag}` → `<prefix>_rec-mcpc3ds_echo-N_part-{phase,mag}_<suffix>`
    (the inputs' suffix, `MEGRE` without one). `qsmxt swi --mip` without `--mip-output`:
    `out.mip.nii` → `out_minIP.nii` (`sub-01_swi.nii` → `sub-01_minIP.nii`).

  New: the total field is published as `fmap/<run>_desc-total_fieldmap.nii` in Hz
  (`Units: "Hz"`) whenever the run computes one, with the first echo's magnitude beside it as
  `fmap/<run>_desc-total_magnitude.nii` (BIDS defines `fieldmap` only under `fmap/`, and
  requires a magnitude with it). A field map takes no `rec-` or `inv-`, so a run that has them
  folds them into `acq-` there (`acq-swi_rec-uncombined` → `acq-swirecuncombined`). The MCPC-3D-S
  combined echoes' sidecars now carry `SkullStripped`. `.bidsignore` adds `extra_files`, `slurm` and the kept non-BIDS suffixes. The
  combined echoes of a per-coil run are now returned to the acquired grid with the other outputs.
  Outputs from an earlier release are removed and recomputed once (their state records names the
  pipeline no longer writes). Switching `--output-space` on a resampled run recomputes it.

- **Segmentations keep valid label ids on the acquired grid** (#267). Returning a resampled run
  to its acquired grid (`--output-space acquired`, the default) interpolated `_dseg.nii`
  trilinearly, because only files named `*mask*` were treated specially, which put invalid,
  non-integer labels on every boundary. Each published derivative now declares what it is
  (continuous map, mask, label image, wrapped phase, projection) and travels accordingly: labels
  by majority vote (QSM.rs `geometry::resample_labels_onto`), masks nearest-neighbour. The SMWI
  minIPs are rebuilt on the acquired grid from the resampled SMWI, as the SWI minIP already was
  (they stayed on the working grid). Outputs returned by an earlier release are redone once.
  Only resampled (oblique) runs change.

- **TFI inverts the total field** (#265). TFI is a total-field method, but QSMxT ran background
  removal first and gave it the V-SHARP local field. Which field an inversion takes now comes from
  QSM.rs (`InversionConfig::input`: local field, total field or wrapped phase), replacing two
  hand-kept lists that disagreed; TFI, MEDI+SMV, TGV, QSMART, AutoQSM and NeXtQSM get the total
  field and the brain mask with no background-removal stage, and the methods text says so.
  `--qsm-algorithm tfi` χ changes; other defaults do not.
- **iQFM with an inversion that does not take a local field is refused** (#276): it had no
  effect, and left field mapping off while the inversion read a total field that was never
  written (or a stale one from an earlier run). Any other non-default `--bf-algorithm` with such
  an inversion is a warning. The TUI hides background removal for those inversions and sends no
  `--bf-algorithm`.

- **V-SHARP no longer deconvolves by default** (`--vsharp-deconvolve false`, QSM.rs#160). The
  default background removal is now V-SHARP's SMV high-pass with the largest kernel that fits
  at each voxel (12 mm down to 1 mm), as STI Suite and the UK Biobank pipeline run it, instead
  of following it with a truncated-SVD deconvolution. On UK Biobank this raised longitudinal
  sensitivity in iron-rich structures without a significant loss of test-retest reliability;
  the deconvolution is exact only for noise-free backgrounds. Every default-configuration χ map
  changes. `--vsharp-deconvolve true` (TUI: V-SHARP → Deconvolve) reproduces v9.22.0, and the
  methods text says which ran.

- **Multi-echo TGV χ had the wrong scale** (fixed; no issue was filed). `stage_tgv` passed the
  combined B0 field map, which is in ppm, to a TGV solver that expected phase in rad at TE1, so
  multi-echo `--qsm-algorithm tgv` χ was too small by 2π·γ·B0·TE1 (about 4× at 3 T with TE1 = 5
  ms, and a different factor for every TE1/B0). TGV now gets the field in ppm for multi-echo data
  (`TgvInput::FieldPpm`, with a plain Laplacian instead of the sin/cos one) and the wrapped echo-1
  phase with TE1 and B0 for single-echo data (`TgvInput::WrappedPhase`), QSM.rs#170. A
  regression test checks the multi-echo and single-echo TGV χ scale against a phantom and RTS.
- **TGV α are in ppm** (QSM.rs#170), so one α means the same at every TE and field strength.
  The defaults are qsm-core's: α0 = 2.002e-4, α1 = 6.675e-5 (the old 0.0015 / 0.0005 rad
  defaults at a 7 T / 4 ms reference, where they were tuned; QSM.rs#175). That is 2.14× stronger
  than a 3 T / 20 ms reference would give, which a sweep favoured on the CI phantom and on UK
  Biobank. `--tgv-alpha0/1` and `[inversion.tgv] alpha0/alpha1` in a config file are read as
  ppm: divide an old rad value by 2π·42.5781·B0·TE. Single-echo TGV at the defaults changes
  wherever TE1/B0 is not 4 ms / 7 T. The methods text states the α and
  units. `qsmxt invert tgv` takes a field in ppm like the other `invert` algorithms and no longer
  has `--echo-time` / `--field-strength`.

These come from QSM.rs (qsm-core is pinned by rev to QSM.rs main `ad39808`):

- **Mask sections build their own magnitude from every echo** (QSM.rs#173, #178). QSMxT used to
  hand each section one magnitude: the inhomogeneity-corrected RSS, or a corrected first/last
  echo. Every echo is now passed, corrected one by one when `--inhomogeneity-correction` is on
  (the default), and each section takes echo 1, the RSS or the last echo from them; phase
  quality is weighted by the RSS. **The default mask changes slightly**: the RSS of corrected
  echoes instead of the corrected RSS. The mask cache key now includes `[homogeneity]`.
- **Algorithms refuse bad input instead of panicking** (QSM.rs#178): ARLO, EPG R2, R2′, RSS,
  MCPC-3D-S and TGV return an error naming what was wrong, reported as `Invalid input: …`.
  QSMxT's own checks (#273) still refuse these runs before processing.
- **iHARPERELLA has no radius** (QSM.rs#169): `--iharperella-radius` (and the TUI row) is gone;
  it never reached the algorithm, which has no SMV kernel. HARPERELLA in the pipeline is
  unchanged (it takes the total field in ppm). `qsmxt bgremove harperella|iharperella` now read
  a wrapped echo in rad when given `--te` and `--field-strength` (both or neither), and an
  unwrapped total field in ppm otherwise; the local field is written in ppm either way (it was
  in rad without `--te`/`--field-strength`).

- **iLSQR is now STI Suite 3.0's algorithm** (QSM.rs#163, #165). The streaking-stage tolerance
  defaults to 1e-3 (was 0.01), `max_iter` caps both LSQR solves (default 100), the pipeline pads
  by 64 mm, and the solve is always double precision. χ from `--qsm-algorithm ilsqr` differs from
  v9.22.0, and small fields of view are slower. QSMART's inner iLSQR changes with it.
- **χ-separation MEDI** (`--separation-algorithm chi-sep-medi`): D_r goes from 114/30 to 137/137
  Hz/ppm (QSM.rs#138), the value the other three methods already used.
- **R2′ is zero where R2 ≤ 0** (QSM.rs#138). R2 = 0 means nothing was measured there.
- **ROMEO without phase offset removal** (`--phase-offset-removal false`): the B0 map now honours
  `--b0-estimation` and `--b0-weight-type` (QSM.rs#162). It used to be a linear fit whatever was
  selected, so with the default (weighted average, phase-SNR) the field map changes.
- **Every NIfTI output carries a qform** (qform_code 1, pixdim[0] = qfac) as well as the sform
  (QSM.rs#133). qform-only inputs are read with their qform.
- **Oblique, anisotropic acquisitions** pick their axial working grid differently (QSM.rs#132,
  `axial_grid_for`/`ras_axes`), which can change results under `--obliquity-threshold` resampling.

### Added

- **Release archives are checksum-verified before they are installed** (#281). Each release now
  carries a `SHA256SUMS` file covering every archive. `qsmxt update`, `install.sh` and
  `install.ps1` download it, check the archive against it before extracting anything, and refuse
  to install when the manifest is missing, has no entry for the archive, or the digest differs.
  The installers accept `QSMXT_VERSION` to install a specific release.
- **`qsmxt update` refuses on a development build** (#281). A `0.0.0` build (from source) was
  offered every release as an update. It now says it is a dev build and exits without
  downloading; `qsmxt update --force` installs the latest release over it.
- **`qsmxt aggregate` and `qsmxt run --no-dataset-steps`** (#269), the two halves of a
  distributed run. They are what `qsmxt slurm` generates, and can be used by hand to split a
  dataset across machines. `aggregate` checks that every run completed under the same
  configuration and names any that did not.

- **Real/imaginary input** (BIDS `part-real` / `part-imag`). An acquisition with no
  `part-phase` files but real and imaginary ones (as GE exports them) is now discovered and
  processed: the first pipeline stage converts each echo to magnitude and phase with QSM.rs's
  `complex_to_mag_phase`, which also detects and undoes alternate-slice π chopping, one decision
  for all echoes. Real/imaginary data whose sidecar `Manufacturer` is GE has its phase negated,
  since GE's phase runs opposite to the Siemens/Philips convention. The neighbour coherence per axis and the corrected axes are logged and recorded
  in the `scale_phase` provenance. Before this, real/imaginary data was silently ignored.
  Per-coil real/imaginary input is refused. `qsmxt dicom-convert` no longer relabels the
  sidecar `ImageType` of GE real/imaginary series to `MAGNITUDE` / `PHASE` (they are written as
  `part-real` / `part-imag` with their ImageType intact), and no longer logs a GE "phase
  inversion" that nothing applied.

- **The R2′ sidecar records where the map came from** (#236): `R2primeSource` = `measured`
  (R2* − R2, with `R2primeMethod` naming where R2 came from), `predicted` (R2PRIMEnet, with
  `R2primenetTrainedFieldStrength` and `R2primenetCalibrated`, false off 3 T) or `supplied`
  (`--use-custom-r2prime`). A predicted R2′ looks like a measured one; only this tells them
  apart. Default image output does not change.

- **Motion-robust echo combination** (QSM.rs#141/#166), off by default: `--b0-estimation
  robust-fit`, with `--robust-fit-loss`, `--robust-fit-tuning`, `--robust-fit-iterations`,
  `--robust-fit-downweight-level`, `--robust-fit-flag-ratio` and the shared
  `--linear-fit-estimate-offset` / `--linear-fit-reliability-threshold`. All are in the TUI
  under B0 Estimation = robust-fit; the per-echo report is written next to the field map
  (`*_robustfit.json`) and flagged echoes are logged.
- The TUI now has the fits' reliability-threshold percentile, and its "Estimate Offset" toggle
  reaches the run (it used to reach only the generated command). `generate_command` now emits
  `--linear-fit-estimate-offset` / `--linear-fit-reliability-threshold` when they differ.
- **Slice-gap check** (QSM.rs#139), on by default. A 2D acquisition whose sidecar
  `SliceThickness` is thinner than the slice pitch is refused for QSM before any processing, since
  the FFT kernels assume contiguous slices; `--ignore-slice-gap` (TUI: General → Ignore Slice Gap)
  reconstructs anyway. **Behaviour change:** such runs used to reconstruct silently with a wrong
  map; non-QSM-only runs continue with a warning.
- **Retrospective motion correction** (QSM.rs#137), off by default: `--motion-correction` rigidly
  co-registers the echoes onto a reference echo before field mapping, with `--motion-reference`,
  `--no-motion-chain`, `--motion-chain-levels`, `--motion-min-ncc` and the `--registration-*`
  group. All are in the TUI (Field mapping → Motion Correction). Needs magnitude (an error
  without). Writes `<run>_motion.json`; the mask is restricted to voxels every echo covers.
- **Multi-orientation co-registration** (QSM.rs#134; supersedes draft #244). An
  `--orientation-group` whose members disagree on grid/affine, or whose only B0 directions come
  from indistinguishable affines, is now rigidly co-registered on magnitude (first member is the
  reference) and its B0 directions are recovered from the rotations, instead of being refused.
  Groups already on one grid with declared `B0_dir`s are untouched. Registration with NCC <= 0.5
  is refused; 0.5-0.8 warns. `--no-orientation-registration` restores the refusal;
  `--multi-orientation-force` and the shared `--registration-*` settings apply. All are in the
  TUI (General: Orientation Co-registration, Force Multi-orientation, Rigid registration rows).
  **Behaviour change:** such groups used to be refused.

- **R2PRIMEnet field-strength policy** (QSM.rs#138). R2PRIMEnet is trained at 3 T only. Away
  from 3 T, `--r2prime-strategy auto` now skips R2′ with a warning when there is no R2 to
  subtract, and an explicit `--r2prime-strategy r2primenet` stops with an error.
  `--r2primenet-ignore-field-mismatch` runs it anyway, with a warning and a methods-text note
  that the R2′ is uncalibrated. `--r2primenet-patch D H W` sets its sliding-window patch. Both
  are in the TUI under "R2' Source". **Output change:** v9.22.0 ran R2PRIMEnet at any field
  strength, so a non-3 T run with no MESE now has no R2′ (and no R2′-based χ-separation) unless
  the override is given.
- **R2′ coverage.** A measured R2′ is written with `_desc-r2prime_mask.nii`, the voxels where it
  is a measurement (brain mask ∩ MESE field of view ∩ R2 > 0), and the log says how much of the
  brain that is. `qsmxt r2prime` gains `--r2-coverage` and `--output-coverage`.

- **TUI/CLI parity.** Every `qsmxt run` flag is now reachable from the TUI. New TUI controls:
  Crop to Mask / Crop Margin / FFT Padding / Output Space / inhomogeneity Sigma and Box Passes /
  Use custom dseg (QSM → General); Coil Comb. Sigma and ROMEO Phase Coherence (Field mapping);
  TGV Step Size / Tolerance; MEDI and TFI MERIT / Data Weighting; the remaining AMP-PE settings;
  Source DICOM Dir / DICOM Outputs (Supplementary, under Export DICOM); Memory Limit / No Memory
  Limit / Force Re-run / Clean Intermediates (Execution, local runs). New flag `--no-medi-smv`
  (MEDI's SMV step is on by default and could not be turned off from the CLI).
- A guard test (`tui::command::parity`) walks every row the QSM and separation tabs can show, and
  the other tabs' controls, and fails if a `run` flag cannot be reached from the TUI, if a TUI
  control has no effect on the run, if a setting the run uses is missing from the generated
  command, or if the generated command does not reproduce the configuration.

### Changed

- **Docs and methods text match the code** (#282). The generated `methods.md` now describes the
  stages each inversion actually runs, routed by QSM.rs's `InversionConfig::input`: iQSM/iQSM+
  no longer claim field mapping or background removal, TGV is no longer "single-step" on
  multi-echo data (it is fed the field map, which is described), and SWI is described as what
  runs (echo-1 Laplacian unwrap, high-pass phase mask, Haacke et al. 2004) rather than as
  CLEAR-SWI. `--b0-weight-type`'s TE weighting is spelled `tes` in `--help` (it was `t-es`, with
  `tes` accepted as an alias). The docs gain a generated CLI reference (every command, option,
  value and default), an output-files page with the #275 names, `rs2-net`/`mouse-bet`/`bfrnet`/
  `iqfm`, every inversion and separation command, the R2′ strategy, the reference-summary
  columns, `.nii` (not `.nii.gz`) outputs, `methods.md` (not `references.txt`), and drop the claim
  that the TUI saves a config file. Tests fail if the CLI reference is stale, if a documented
  flag does not exist, if a command, algorithm value, mask preset expansion, inversion count or
  published file name is missing or wrong in the docs, or if the methods text describes
  background removal or unwrapping for an inversion route that does not run it.

- **`--b0-direction` accepts negative components** (and `--obliquity-threshold` accepts its
  documented `-1`). Clap read a value such as `-0.054523` as an unknown flag, so
  `qsmxt bgremove`, `invert` (including `cosmos`/`sti`), `separate` and `qsmart` failed with a
  usage error for any oblique acquisition whose B0 direction has a negative component, and
  `qsmxt run --obliquity-threshold -1` could not disable resampling. Default output does not
  change; commands that used to be refused now run.
- **The multi-orientation (COSMOS) map is a first-class output** (#270). It used to read
  members' magnitudes from `anat/` after they had been returned to the acquired grid while their
  local fields stayed on the working grid, so an oblique group failed under the default
  `--output-space acquired`; it now reads every member input from the working grid. The COSMOS
  map is referenced with `--qsm-reference` over the voxels every orientation covers (it was
  written raw), written on the reference orientation's acquired grid, cached (keyed on the
  members' local fields, directions and settings), computed before the reference summary and
  statistics so it appears in both, and with `--do-analysis` gets a per-structure table on the
  reference orientation's segmentation. A grouping whose inversion skips background removal (MEDI
  with SMV, TGV, QSMART, AutoQSM, NeXtQSM, iQSM/iQSM+) or with `--no-qsm` is refused before
  processing. New `--cosmos-magnitude-weighting`, `--cosmos-tol`, `--cosmos-max-iter` (TUI:
  pipeline tab, General; config `[multi_orientation]`). STI outputs stay unreferenced on the
  working grid, with a warning when the reference orientation was returned to its acquired grid.
  Default output changes for COSMOS: the map is now mean-referenced, and on the acquired grid for
  an oblique reference orientation.

- **A requested output the data cannot produce is an error, before anything runs** (#193,
  #273). SWI, SMWI, segmentation, R2*/T2*, R2, R2′ and χ-separation used to be dropped silently
  (or with a `Skipping` line mid-run) when a run lacked what they need, and the run exited 0.
  `qsmxt run` and `qsmxt slurm` now check every run first: an output you asked for that a run
  cannot produce stops the command with one error listing every such run and what is missing;
  an output another option turned on (R2* for `--chisep r2star-qsm`, R2/R2* for R2′,
  segmentation for `--do-analysis` or a region `--qsm-reference`) is skipped with a warning
  naming that option. A named `--use-custom-qsm/-r2/-r2prime/-dseg` derivative missing for a run
  counts as asked for. R2*/T2* (ARLO) now require equally spaced echo times and EPG R2 requires
  an equally spaced MESE whose first echo is at the echo spacing (each within 0.1 ms); unequal
  spacing used to give wrong R2*/T2* silently and make EPG panic. A single-echo uncombined-coil
  run is refused instead of panicking in MCPC-3D-S. `qsmxt validate` reports the same checks.
  Default output does not change; runs that used to finish with outputs missing now stop.

- **GE conversion no longer claims a correction it does not make** (#274). `dicom-convert`
  logged "GE data detected — phase inversion will be applied", and nothing inverted it; it also
  rewrote the sidecar `ImageType` of GE real/imaginary series to `MAGNITUDE`/`PHASE` while the
  files stayed `part-real`/`part-imag`. Both are gone. Each GE series now gets a warning saying
  what is not done: GE phase is not corrected for handedness or slice chopping, and real/imag
  series are not read by `qsmxt run` (both wait on QSM.rs#171). GE is detected from the start of
  `Manufacturer` (`GE ...`, `GENERAL ELECTRIC`), not any string containing "GE". Default output
  does not change; GE real/imag sidecars keep the scanner's `ImageType`.

- **Cached steps are keyed on the whole configuration they read** (#272). Each step's cache key
  was a hand-picked list of parameters, and the lists were incomplete: changing RTS `rho`, TV
  `tol`, most of MEDI, any TFI setting, MSMV, `--crop-to-mask`/`--fft-padding`/`--crop-margin-mm`,
  `--phase-offset-sigma`, most ROMEO settings or any χ-separation setting reused a stale result.
  Each step's key now includes the whole config section it reads (inversion, background removal,
  field mapping, separation, masking, SWI, SMWI, segmentation, referencing, motion correction),
  so a new config field is keyed automatically. Replacing an input file in place (size or
  modification time) recomputes the run. Invalidation gained the missing edges (`scale_phase`
  → magnitude/R2*/χ-separation, everything → the two-pass single-pass reference →
  statistics). Caches from earlier builds are recomputed once.

- **`--clean-intermediates` no longer deletes published outputs or the cache** (#271). It
  deleted every recorded output of a step not on a hard-coded list — including the
  `rec-mcpc3ds` combined echoes in `anat/` — removed each run's local field before the
  multi-orientation fan-in could read it (COSMOS/STI failed), and deleted the state file, so the
  next run recomputed everything. It now runs once, after every run and the multi-orientation
  step, deletes only images under `workflow/` (and the working-grid copies), and keeps the
  per-step provenance and the state file with the cleaned steps marked. An immediate re-run
  with the same configuration recomputes nothing; one with a changed configuration recomputes
  the cleaned intermediates it needs, and leaves a published step cached when its own
  parameters did not change. A re-run with an unchanged configuration also no longer
  re-resamples outputs already returned to the acquired grid.

- **Flags that were accepted and then ignored now act or are refused** (#279):
  - `--bet-fractional-intensity` reaches BET: it sets every BET step's fractional intensity in
    the masking recipe (as `--bet-voxel-scale` does), overriding a `bet:<f>` in `--mask`; a
    config file's `[bet] fractional_intensity` / `voxel_scale` reach the steps the same way.
    `--bet-smoothness`, `--bet-gradient-threshold`, `--bet-iterations` and `--bet-subdivisions`
    now reach BET too, since QSM.rs#180 lets a BET mask step carry all of BET's settings: each
    applies to every BET step of the recipe (CLI, `[bet]`, TUI masking section, methods text).
  - `qsmxt fieldmap romeo` starts from QSMxT's defaults, as `qsmxt run` does (ROMEO individual
    unwrapping and global correction on; it used qsm-core's, with both off), so the two give the
    same field map for the same inputs and flags. **The standalone tool's default output
    changes.** All ROMEO flags are read in one place (`RomeoParamArgs::apply_to`).
  - `qsmxt unwrap romeo` takes the same `--no-romeo-*` / `--romeo-*` flags as `run` (they reach
    ROMEO now); its own `--no-phase-gradient-coherence`, `--no-mag-coherence` and
    `--no-mag-weight` (which only reached a debug line) are gone, and the multi-echo flags are
    refused, since it unwraps one 3D phase.
  - `qsmxt segment --use-custom-dseg` is refused (`segment` computes a segmentation).
  - `qsmxt resample`: `--no-noise-fill`, `--phase-out` and `--magnitude-out` need `--phase`, and
    `--phase` cannot be combined with a positional volume or `--output` (one of each was
    ignored).
  - qsmxt-config: the unused `Mcpc3dsConfig` is deleted (`FieldMappingConfig::phase_offset_sigma`
    is the MCPC-3D-S phase-offset smoothing).
- **Motion correction: reconstruction stays inside the voxels every echo covers** (#277;
  **changes output with `--motion-correction`**). Field mapping used the coverage mask (the brain
  mask less what some co-registered echo no longer covers), but background removal, the
  inversion and the published `desc-qsm_mask` used the whole brain mask, where the field was
  never computed — biasing the background fit and putting χ ≈ 0 into the edge of the support.
  Both passes of a two-pass run now use it. It is published as `desc-motioncoverage_mask` (the
  brain mask stays `_mask`). Switching motion correction on or off now re-runs every stage that
  reads its output, and in general a recomputed step invalidates the cached steps downstream of
  it. `--clean-intermediates` only deletes files under `workflow/`.

- **`--swi-scaling negative-tanh` runs negative-tanh** (#266). `qsmxt run` matched the scaling as
  a string and only knew `negative_tanh`, so the spelling the help, the TUI and the generated
  command all use fell through to plain `tanh` without a word. The scaling is now one typed enum
  (`SwiScaling`) from the CLI (a clap value list: `tanh`, `negative-tanh`, `positive`, `negative`,
  `triangular`; `negative_tanh` is still accepted), through the config file (`[swi] scaling`;
  an unknown value no longer parses), to the TUI and qsm-core's `PhaseScaling`, mapped in one
  place for `run` and `qsmxt swi`. SWI changes only for runs that asked for `negative-tanh`.
  For qsmxt-config consumers (qsmbly): `SwiConfig::scaling` is now `SwiScaling`, not `String`.
- **A flag value the run cannot use is an error, not a warning** (#266 sweep). `qsmxt run`
  used to log "Ignoring ..." and carry on with the default for an unparseable `--mask` /
  `--two-pass-mask` section input or op, a `--mask-refine` that is a generator or does not
  parse, `--motion-reference`, a two-value `--laplacian-fft-pad` and `--synthseg-version`. Mask
  ops no longer read an unparseable number as the default (`erode:x` eroded once) and refuse
  extra arguments (`erode:2:3`), and a `--mask` section with two generators is refused rather
  than silently refining with the second. The TUI shows the error in place of the command and
  the methods.
- **`--synthseg-version v2` is refused until its weights exist** (#264). It always ran the v1
  network (the weight file was hard-coded) and read the output through v2's 33-label table.
  QSM.rs lists SynthSeg 2.0 but its ONNX export is not published, so a run that would segment
  with v2 (CLI, TUI or config file, including a region reference that turns segmentation on)
  now fails at start-up, as does `qsmxt segment --synthseg-version v2`, unless
  `synthseg-2.0.onnx` is supplied in `$QSM_MODEL_DIR`. The weight file now follows the version,
  so publishing the weights in QSM.rs is all that remains. With `--use-custom-dseg`, v2 still
  selects the label table a supplied segmentation is read with.

- **Provenance records the exact QSM.rs commit** (#278; no change to any image). The QSM.rs
  version was "unknown" in BIDS-Prov, the TUI header and `--version` because qsm-core is pinned
  by `rev`, not `tag`. It is now the tag when there is one, else `rev-<full 40-character commit>`
  read from `Cargo.lock`, and the build fails rather than record a git pin without a commit.
  It appears in `dataset_description.json` (a second `GeneratedBy` entry, `QSM.rs`, with a
  `CodeURL` to that commit), in BIDS-Prov (`prov-qsmxt_soft.json`, agent
  `bids::prov/#qsm-rs-<version>`), in every output sidecar (`QSMrsVersion`, beside
  `QSMxTVersion`), in the log, the TUI header and `qsmxt --version`.
- BIDS-Prov covers every step that writes a file: motion correction, segmentation, SMWI,
  R2/R2′, χ-separation, statistics, both passes and the combination of a two-pass run, the
  single-pass map, the return to the acquired grid and coil combination were missing, and the
  sidecars of some outputs named activities that were never written. Each published image's
  `GeneratedBy` is now the activity that last wrote it, read from the records rather than a
  hand-kept list, so it always resolves in `prov/` (for a map returned to the acquired grid,
  that is `output_space`, whose `Used` links back to the step that made the working-grid map).
  R2/R2′, the χ-separation maps and the COSMOS/STI outputs get sidecars; COSMOS/STI get their
  own activity, and provenance is now written after the multi-orientation step. A test keeps
  the list of recorded steps in step with the pipeline.
- **`desc-reference_summary.tsv` has a correct header and one row per referenced map** (#280; no
  change to any image). Its `reference` column was named `"                  reference"` (a
  lost line continuation left a run of spaces in the header), and a two-pass run's single-pass
  map was not listed. The columns are now `subject session acquisition reconstruction inversion
  run map reference offset_ppm n_voxels`, with a row for the `Chimap` and one for the
  `desc-singlepass_Chimap` of a two-pass run. The same lost continuations put long runs of spaces
  into several multi-orientation log and error messages; fixed.
- The per-structure statistics read each map's reference from that map's own sidecar. The
  χ-separation rows (`desc-paramagnetic/diamagnetic/total_Chimap`), which are never referenced,
  carried the main map's reference and offset, and the `desc-singlepass_Chimap` rows carried
  the combined map's offset instead of their own. The single-pass map's sidecar now records its
  own `QsmReference*` fields.

- The TUI's configuration (methods text, generated command) is now built from the same argument
  list the run uses, through the same overrides as `qsmxt run`, so the three can no longer
  disagree. They did: the generated command and methods text left out HEIDI/LSQR settings, the
  QSMART radii and deep-learning tiling, though the run used them.
- `generate_command` (shared with qsmbly) now emits every ROMEO setting, the QSMART inner
  inversion's own settings, MEDI/TFI MERIT and data weighting, the inhomogeneity-correction
  settings, and the multi-orientation, registration and custom-dseg settings whenever they were
  changed; under QSMART it states the mask when it is the default threshold recipe (QSMART's own
  default is BET).
- Fixed: `--medi-merit` and `--medi-data-weighting` were parsed but never applied.
- Fixed: the separation tab showed HC-χsep's parameters for SUSEP-Net and χ-sepnet, where they
  did nothing.
- `--qsmart-ilsqr-tol` is removed: QSMART's inner iLSQR uses `--ilsqr-tol`. New
  `--qsmart-ilsqr-pad-mm` (default 6 mm, STI Suite's) sets its zero-padding; the TUI has it as
  "iLSQR Pad (mm)" and shows the iLSQR tolerance under QSMART. A config file's
  `[inversion.qsmart] ilsqr_tol` is ignored.
- `--b0-weight-type assumed-decay` reaches qsm-core as the real assumed-decay weighting rather
  than TE weighting, so qsmbly and any other qsmxt-config consumer get the same field map as QSMxT.
- Field mapping is qsm-core's `run_field_mapping` (QSM.rs#161/#162), with no output change: a
  test holds it to QSMxT's previous implementation on every path that implementation computed
  itself, including a single Laplacian-unwrapped echo and the Laplacian linear fit, which
  qsm-core masks and aligns since QSM.rs#167. QSMxT no longer computes any of it itself.
- The FFT Laplacian solver (`--laplacian-solver fft`) is in every build; the `laplacian-fft`
  cargo feature is gone.
- The Laplacian solver and its FFT pad are in the TUI (Field mapping → Unwrapping = laplacian →
  Solver / FFT Pad). `--laplacian-fft-pad` (and `field_mapping.laplacian_fft_pad`) takes one
  value or three (x y z). Default unchanged (64 on every axis).
