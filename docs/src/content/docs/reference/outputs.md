---
title: Output files
description: Every file QSMxT writes, what it is named, and which names are not (yet) BIDS.
---

`qsmxt run` writes a BIDS-Derivatives dataset to `<OUTPUT_DIR>/derivatives/qsmxt/`.
Every name comes from one naming module that the pipeline and the standalone
commands share, so `qsmxt invert sti -o <prefix>` writes the same entities and
suffixes the pipeline publishes for a run. Outputs are uncompressed NIfTI
(`.nii`), each with a JSON sidecar.

Below, `<run>` is the run's BIDS basename: its `sub-`, `ses-`, `acq-`, `rec-`,
`run-` and `inv-` entities, in BIDS order (`sub-01_ses-pre_run-2_inv-1`).

## Per-run outputs (`sub-<X>/[ses-<Y>/]anat/`)

| File | Contents | Produced by |
| --- | --- | --- |
| `<run>_Chimap.nii` | Susceptibility map (ppm), referenced | default |
| `<run>_mask.nii` | Brain mask | default |
| `<run>_desc-qsm_mask.nii` | The voxels the `Chimap` is defined on | default |
| `<run>_part-mag_T2starw.nii` | Combined (RSS) magnitude | default, with magnitude input |
| `<run>_desc-singlepass_Chimap.nii` | The conventional single-pass map of a two-pass run | `--two-pass` |
| `<run>_desc-reliable_mask.nii` | The reliable-phase mask of a two-pass run | `--two-pass` |
| `<run>_desc-motioncoverage_mask.nii` | The mask every motion-corrected echo still covers | `--motion-correction` |
| `<run>_swi.nii`, `<run>_minIP.nii` | SWI and its minimum-intensity projection | `--do-swi` |
| `<run>_desc-paramagnetic_smwi.nii`, `<run>_desc-diamagnetic_smwi.nii` | SMWI per contrast | `--do-smwi` |
| `<run>_desc-paramagneticsmwi_minIP.nii`, `<run>_desc-diamagneticsmwi_minIP.nii` | Their minimum-intensity projections | `--do-smwi` |
| `<run>_T2starmap.nii`, `<run>_R2starmap.nii` | T2\* (s) and R2\* (s⁻¹) | `--do-t2starmap`, `--do-r2starmap` |
| `<run>_R2map.nii` | R2 (s⁻¹) from a MESE | `--do-r2map` |
| `<run>_R2primemap.nii` | R2′ (s⁻¹) | `--do-r2primemap` |
| `<run>_desc-r2prime_mask.nii` | Where R2′ is a measurement | `--do-r2primemap` with a MESE |
| `<run>_desc-paramagnetic_Chimap.nii`, `<run>_desc-diamagnetic_Chimap.nii`, `<run>_desc-total_Chimap.nii` | χ-separation | `--do-chisep` |
| `<run>_dseg.nii`, `<run>_dseg.tsv` | SynthSeg parcellation and its label table | `--do-segmentation` |
| `<run>_desc-segmentation_stats.tsv`, `<run>_desc-<map>_stats.png` | Per-structure statistics and figures | `--do-analysis` |
| `<run>_rec-mcpc3ds_echo-<N>_part-{phase,mag}_<suffix>.nii` | MCPC-3D-S combined echoes of per-coil (`rec-uncombined`) data | per-coil input |

## Field map (`sub-<X>/[ses-<Y>/]fmap/`)

| File | Contents |
| --- | --- |
| `<run>_desc-total_fieldmap.nii` | The total (unwrapped, echo-combined) field in **Hz** (`Units: "Hz"`), before background removal |
| `<run>_desc-total_magnitude.nii` | The first echo's magnitude, the anatomical reference BIDS requires beside a field map |

BIDS allows no `rec-` or `inv-` on a field map, so a run that has them folds them
into its `acq-` label here (`sub-01_acq-swi_rec-uncombined` →
`sub-01_acq-swirecuncombined_desc-total_fieldmap.nii`).

Written whenever the run computes a field map (every inversion except iQSM, iQSM+
and single-echo TGV, which reconstruct from the wrapped phase). BIDS defines
`fieldmap` only under `fmap/` and in Hz, rad/s or T, which is why it is not the
ppm field the inversions read (that one stays in `workflow/`).

## Multi-orientation outputs

Named by the entities every member of the orientation group shares (the one that
varied is dropped), in `anat/`:

| File | Contents |
| --- | --- |
| `<group>_desc-cosmos_Chimap.nii` | COSMOS susceptibility map (ppm) |
| `<group>_desc-sti_Chimap.nii` | The STI tensor (ppm), 4D: its six independent components along the fourth axis, in the order the sidecar's `TensorComponents` lists (`X11 X12 X13 X22 X23 X33`) |
| `<group>_desc-mms_Chimap.nii`, `<group>_desc-msa_Chimap.nii` | STI mean magnetic susceptibility and anisotropy (ppm) |
| `<group>_desc-sti_evec.nii` | The STI principal eigenvector, 4D (i, j, k along the fourth axis) |

## Working-grid outputs

A run resampled to an axial grid (an oblique acquisition with
`--axial-resampling`, or a deep-learning method) is returned to the acquired
grid by default, under the names above. With `--output-space working` its
outputs stay on the axial grid and say so: every name carries `space-axial`
(`<run>_space-axial_Chimap.nii`, `<run>_part-mag_space-axial_T2starw.nii`) and
every sidecar a `SpatialReference` naming that run's
`part-mag_space-axial_T2starw` image. A run that was never resampled keeps its
plain names either way.

## Runs that differ only by suffix

Two runs in one session that differ only by their suffix (a `MEGRE` and a `GRE`)
would write the same files. Each such run gets its suffix folded into its `acq-`
label (`sub-01_acq-MEGRE_Chimap.nii`, `sub-01_acq-GRE_Chimap.nii`;
with an existing `acq-fast`, `acq-fastGRE`). `acq-` is the entity BIDS defines for
telling apart acquisitions of one modality with different parameters, and it
keeps the names valid. A run with no such twin keeps its name.

## Dataset-level files (`derivatives/qsmxt/`)

| File | Contents |
| --- | --- |
| `dataset_description.json`, `README` | BIDS-Derivatives description, with `GeneratedBy` and `SourceDatasets` |
| `methods.md` | A methods paragraph describing what the runs did, with citations |
| `pipeline.svg`, `pipeline.mmd` | The pipeline the runs took, as an image and as a Mermaid graph: one box per step, an arrow for each thing one step reads from another, and on a box how many runs skipped the step or decided something in it |
| `changes/` | Only when some runs were processed differently from the rest (a decision made as a run went, such as a supplied mask it could not use): each such run's methods text (`<run>_methods.md`) and pipeline as it ran (`<run>_desc-ran_pipeline.mmd`, `.svg`) |
| `pipeline_config.toml` | The resolved settings, reusable with `--config` |
| `qsmxt.log`, `logs/` | The log (and, under SLURM, each run job's log) |
| `desc-reference_summary.tsv` | What every susceptibility map was referenced to (see [Running noninteractively](/QSMxT/guides/running-noninteractively/#the-reference-summary)) |
| `desc-segmentation_stats.tsv`, `desc-<map>_stats.png` | Cohort statistics and figures (`--do-analysis`) |
| `prov/` | BIDS-Prov (BEP028) records of every step |
| `workflow/` | Cached intermediates (removed by `--clean-intermediates`), and per run its methods text, a record of what each step did and why (`*_runrecord.json`) and its pipeline graph |
| `slurm/` | Job scripts and their logs (`qsmxt slurm`) |
| `sub-<X>/extra_files/` | DICOM series (`--export-dicom`) |

## Names BIDS does not define, and `.bidsignore`

Some outputs have no BIDS name yet. They keep descriptive names and are listed
in the derivatives' `.bidsignore`, so the dataset passes the BIDS validator:

| Name | Why |
| --- | --- |
| `R2primemap` | BIDS has no suffix for R2′ |
| `swi`, `minIP` | The suffixes proposed by BEP004 (SWI), not yet in a released BIDS |
| `smwi` | SMWI, named on the same BEP004 scheme |
| `evec` | BIDS has no suffix for an orientation field; an eigenvector is not a susceptibility, so it is not a `Chimap` |
| `*_stats.tsv`, `*_stats.png`, `desc-reference_summary.tsv` | `stats` is not a BIDS suffix |
| `workflow`, `prov`, `logs`, `slurm`, `extra_files`, `changes`, `methods.md`, `pipeline.mmd`, `pipeline.svg`, `pipeline_config.toml`, `*.log` | Not BIDS data files |

## Standalone commands

The standalone commands take an output prefix and append the pipeline's tail, so
given a run's basename as the prefix they write exactly the pipeline's names:

| Command | Writes |
| --- | --- |
| `qsmxt invert sti -o <prefix>` | `<prefix>_desc-sti_Chimap.nii` (4D) and its `.json`, `<prefix>_desc-mms_Chimap.nii`, `<prefix>_desc-msa_Chimap.nii`, `<prefix>_desc-sti_evec.nii` (4D) |
| `qsmxt separate … -o <prefix>` | `<prefix>_desc-paramagnetic_Chimap.nii`, `<prefix>_desc-diamagnetic_Chimap.nii`, `<prefix>_desc-total_Chimap.nii` |
| `qsmxt combine mcpc3ds -o <prefix>` | `<prefix>_rec-mcpc3ds_echo-<N>_part-{phase,mag}_<suffix>.nii` (`rec-mcpc3ds` replaces a `rec-` in the prefix; `<suffix>` is the inputs', `MEGRE` without one) and `<prefix>_desc-mcpc3ds_mask.nii` |
| `qsmxt swi … --mip -o <name>_swi.nii` | `<name>_minIP.nii` beside it, unless `--mip-output` says otherwise |
