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
| `<run>_desc-echo<n>_Chimap.nii` | Each echo's susceptibility map (ppm), referenced; the `Chimap` is their mean | `--qsm-per-echo true` |
| `<run>_part-mag_T2starw.nii` | Combined magnitude: the root sum of squares of the echoes' magnitudes, each corrected for inhomogeneity when that is on | default, with magnitude input; `--no-magnitude` leaves it out |
| `<run>_desc-singlepass_Chimap.nii` | The conventional single-pass map of a two-pass run | `--two-pass` |
| `<run>_desc-reliable_mask.nii` | The reliable-phase mask of a two-pass run | `--two-pass` |
| `<run>_desc-motioncoverage_mask.nii` | The mask every motion-corrected echo still covers | `--motion-correction` |
| `<run>_swi.nii`, `<run>_minIP.nii` | SWI and its minimum-intensity projection | `--do-swi` |
| `<run>_desc-swi_phase.nii` | The high-pass filtered phase the SWI is weighted by: CLEAR-SWI's Gaussian high-pass (in-plane by default) within its head mask from the first echo's magnitude, 0 outside it. **Hz** for multi-echo data, **radians** for one echo (the sidecar's `Units`) | `--swi-filtered-phase` |
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
| `methods.md` | A methods paragraph describing what the runs did, a Note listing where the runs departed from the chosen methods' recommendations and reference pipelines, and the references cited |
| `pipeline.svg`, `pipeline.mmd` | The pipeline the runs took, as an image and as a Mermaid graph: one box per step, an arrow for each thing one step reads from another, and on a box how many runs skipped the step, decided something in it, or set it up each way; a key explains the line styles used |
| `changes/` | Only when some runs were processed differently from the rest (a decision made as a run went, such as a supplied mask it could not use): each such run's methods text (`<run>_methods.md`) and pipeline as it ran (`<run>_desc-ran_pipeline.mmd`, `.svg`) |
| `pipeline_config.toml` | The resolved settings, reusable with `--config` |
| `qsmxt.log`, `logs/` | The log (and, under SLURM, each run job's log) |
| `desc-reference_summary.tsv` | What every susceptibility map was referenced to (see [The reference summary](#the-reference-summary)) |
| `desc-segmentation_stats.tsv`, `desc-<map>_stats.png` | Cohort statistics and figures (`--do-analysis`) |
| `prov/` | BIDS-Prov (BEP028) records of every step |
| `workflow/` | Cached intermediates (removed by `--clean-intermediates`), and per run its methods text, a record of what each step did and why (`*_runrecord.json`) and its pipeline graph |
| `slurm/` | Job scripts and their logs (`qsmxt slurm`) |
| `sub-<X>/extra_files/` | DICOM series (`--export-dicom`), and the segmentation as a DICOM SEG on one of them (the magnitude, else χ, else another map). The export follows the run and a failure does not fail it, so the methods mention it only for runs that wrote series. A browser page writes no DICOM: it turns the setting off when it reads a settings file, and tells you |

## Segmentation and per-structure statistics

The steps are in [Measure χ in brain structures](/QSMxT/how-to/results/#measure-χ-in-brain-structures).

`--do-segmentation` parcellates the brain straight from the GRE magnitude with
[SynthSeg](https://doi.org/10.1016/j.media.2023.102789). It is trained on
synthetic images of randomised contrast, so one set of weights segments a GRE
magnitude directly — no T1w acquisition, and no registration. Outputs are a
FreeSurfer-labelled `_dseg.nii` and the `_dseg.tsv` lookup table that names its
integers. `--use-custom-dseg <TOOL>` brings your own parcellation from
`<bids>/derivatives/<TOOL>/` instead.

`--synthseg-version` picks the weights: `v1` (SynthSeg 1.0, 32 labels, the
default) or `v2`, [SynthSeg 2.0](https://doi.org/10.1073/pnas.2216399120)
(Billot et al., PNAS 2023; 33 labels, adding a general CSF class). Both are the
authors' non-robust segmentation network; SynthSeg 2.0's robust, cortical
parcellation and QC networks are not available. The 2.0 weights come
from the author's own release (they are not in the SynthSeg repository) and are
downloaded on first use like the other networks. With `--use-custom-dseg`, the
version only selects the label table a supplied segmentation is read with.

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
susceptibility map in sight. `--do-analysis` requires `--do-segmentation` (unless
you supply a `dseg`), and QSM only when nothing else would produce a map at all;
left off, the run turns them on itself and logs it.

## DICOM series

The steps are in [Export results as DICOM](/QSMxT/how-to/export-dicom/). `--export-dicom`
writes a DICOM series for every final map alongside the NIfTIs:

```sh
qsmxt run study/bids --export-dicom
```

Each map gets its own folder under the subject's `extra_files/`, one file per
slice:

```
study/bids/derivatives/qsmxt/
└── sub-01/
    ├── anat/
    │   └── sub-01_…_Chimap.nii
    └── extra_files/
        └── sub-01_Chimap_dicoms/
            ├── sub-01_Chimap_0001.dcm
            ├── sub-01_Chimap_0002.dcm
            └── …
```

Every final map the run produced is exported. The magnitude and χ need no extra flag; the
supplementary maps are exported once you've asked for them:

| Map | Produced by | Folder suffix / `--dicom-outputs` token |
| --- | --- | --- |
| Magnitude (the echoes combined) | any run with magnitude images | `magnitude` |
| Susceptibility (χ) | the default pipeline | `Chimap` |
| SWI | `--do-swi` | `swi` |
| SWI minIP | `--do-swi` | `minIP` |
| SWI filtered phase | `--swi-filtered-phase` | `desc-swi_phase` |
| Paramagnetic SMWI | `--do-smwi` | `desc-paramagnetic_smwi` |
| Diamagnetic SMWI | `--do-smwi` | `desc-diamagnetic_smwi` |
| Paramagnetic SMWI minIP | `--do-smwi` | `desc-paramagneticsmwi_minIP` |
| Diamagnetic SMWI minIP | `--do-smwi` | `desc-diamagneticsmwi_minIP` |
| T2\* | `--do-t2starmap` | `T2starmap` |
| R2\* | `--do-r2starmap` | `R2starmap` |
| R2 | `--do-r2map` | `R2map` |
| R2′ | `--do-r2primemap` | `R2primemap` |
| Paramagnetic χ | `--do-chisep` | `desc-paramagnetic_Chimap` |
| Diamagnetic χ | `--do-chisep` | `desc-diamagnetic_Chimap` |
| Total χ | `--do-chisep` | `desc-total_Chimap` |
| Segmentation (a DICOM SEG, below) | `--do-segmentation`, `--use-custom-dseg` | `dseg` |

To export only some of them, pass those tokens to `--dicom-outputs`
(case-insensitive), comma-separated or by repeating the flag; a name that is not
in the table is refused:

```sh
qsmxt run study/bids --do-chisep --export-dicom \
  --dicom-outputs chimap,desc-paramagnetic_chimap --dicom-outputs desc-diamagnetic_chimap
```

In the TUI, the Reconstruction tab's DICOM section lists the maps the settings
make, each ticked; untick the ones to leave out.

The [multi-orientation](/QSMxT/reference/multi-orientation/#multi-orientation-cosmos-and-sti)
reconstructions stay NIfTI-only.

### The segmentation as a DICOM SEG

The
[segmentation](/QSMxT/reference/outputs/#segmentation-and-per-structure-statistics)
is written as one DICOM Segmentation object (`sub-01_dseg_dicoms/sub-01_dseg.dcm`):
each structure is a binary segment, drawn on a series exported with it on the
same grid, so a viewer overlays it there (OHIF, Weasis 4.3 or later, and 3D Slicer
with the QuantitativeReporting extension; check your PACS's DICOM conformance
statement for *Segmentation Storage*). The series is the first of these the export
writes: the magnitude, χ, SWI, T2\*, R2\*, R2, R2′, the χ-separation maps, SMWI (not
a minIP, which is a projection); the log names it. A `--dicom-outputs` that selects
`dseg` and none of those the run makes is refused before the run.
SynthSeg's structures are coded with the standard's codes for brain structures
(DICOM PS3.16 CID 7153 and CID 7140), with left and right as modifiers, so a viewer
can name them in any language. A supplied segmentation (`--use-custom-dseg`) need not
use FreeSurfer's numbering, so its labels are coded only as intracranial structures
and named from its `dseg.tsv`.

### Filing them beside the source study

By default the derived series carry a synthesised identity: patient fields are
left empty and a stable `StudyInstanceUID` is derived from the subject and
session. That's fine for a local viewer, but a PACS will file the maps as a study
of their own. Point `--source-dicom` at the original DICOMs and QSMxT inherits the
identity from the first readable DICOM it finds there — `PatientID`,
`StudyInstanceUID`, `FrameOfReferenceUID`, study date/time, accession number — so
the maps land beside the acquisition they came from:

```sh
qsmxt run study/bids --export-dicom --source-dicom /data/dicoms/sub-01
```

Without the original DICOMs to hand, a dataset converted with
[`qsmxt dicom-convert --keep-dicom-uids`](/QSMxT/how-to/convert-dicom/#keeping-the-studys-dicom-uids) does
nearly as well: its sidecars keep the study's `StudyInstanceUID` and
`FrameOfReferenceUID`, so the exported series file into the original study and
line up with its images, though with no patient fields. `--source-dicom` still
wins where it is given.

`SeriesInstanceUID` and `SOPInstanceUID` are always newly generated: these are
distinct derived series, never a rewrite of the originals. Every instance is
tagged `DERIVED\SECONDARY`, and each map gets a stable `SeriesNumber` so viewers
list them in a predictable order.

:::caution[Values are rescaled to integers]
DICOM pixel data is integer, so each volume is linearly rescaled into unsigned
16-bit with `RescaleSlope` and `RescaleIntercept` set to match. A viewer that
honours those tags recovers the true values (χ in ppm, R2\* in s⁻¹); one that
ignores them shows raw 0–65535 counts. The scaling is fitted per volume, so it is
not comparable across subjects — **for quantitative analysis use the NIfTIs**.
:::

### Geometry, encoding and failures

- Slice geometry (`ImageOrientationPatient`, `ImagePositionPatient`,
  `SliceLocation`, spacing) is read from the output NIfTI's affine, so the series
  overlays the source acquisition in a viewer and reflects any
  [oblique resampling](/QSMxT/background/geometry/#oblique-acquisitions) the run
  applied.
- Instances are uncompressed *MR Image Storage*, Explicit VR Little Endian. The acquisition's
  patient position, repetition time, echo train length, body part and laterality
  are filled in too: from `--source-dicom` when given, else from the run's BIDS
  sidecar (`PatientPosition`, `RepetitionTime`, `EchoTrainLength`, `BodyPart`,
  as dcm2niix and `qsmxt dicom-convert` write them), else left empty rather than
  guessed. The segmentation is a *Segmentation Storage* object.
- Export is best-effort: a failure is logged and the run still succeeds.
- `--export-dicom` is a pipeline setting, so it can live in a
  [config file](/QSMxT/reference/configuration/) as `export_dicom = true` under
  `[pipeline]`, and it carries through to [`qsmxt slurm`](/QSMxT/how-to/slurm/).
  `--source-dicom` and `--dicom-outputs` are flags on `qsmxt run` itself.

## Names BIDS does not define, and `.bidsignore`

Some outputs have no BIDS name yet. They keep descriptive names and are listed
in the derivatives' `.bidsignore`, so the dataset passes the BIDS validator:

| Name | Why |
| --- | --- |
| `R2primemap` | BIDS has no suffix for R2′ |
| `swi`, `minIP` | The suffixes proposed by BEP004 (SWI), not yet in a released BIDS |
| `smwi` | SMWI, named on the same BEP004 scheme |
| `phase` | SWI's filtered phase (`desc-swi_phase`); BIDS deprecates `phase` as a suffix for raw phase |
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
| `qsmxt swi … --filtered-phase -o <name>_swi.nii` | `<name>_desc-swi_phase.nii` beside it, unless `--filtered-phase-output` says otherwise |

## Output

```
study/bids/derivatives/qsmxt/
└── sub-01/
    ├── anat/
    │   ├── sub-01_…_Chimap.nii
    │   ├── sub-01_…_Chimap.json
    │   ├── sub-01_…_mask.nii
    │   ├── sub-01_…_desc-qsm_mask.nii
    │   └── sub-01_…_part-mag_T2starw.nii
    └── fmap/
        ├── sub-01_…_desc-total_fieldmap.nii
        └── sub-01_…_desc-total_magnitude.nii
```

Outputs are uncompressed NIfTI (`.nii`), each with a JSON sidecar recording how it
was made (`GeneratedBy`, units, and for susceptibility maps the reference). The
total field is published in Hz under `fmap/`. Every file and its name is listed
in [Output files](/QSMxT/reference/outputs/).

`mask` is the brain mask; `desc-qsm_mask` is the part of it the susceptibility map
is defined on — the brain mask less the rim that background-field removal eroded.
The `Chimap` is referenced over, and is 0 outside, `desc-qsm_mask`; the χ-separation
maps (`--do-chisep`) are fitted over it and are 0 outside it too.

At the top of `derivatives/qsmxt/`, `methods.md` describes the processing in
prose ready for a methods section, with citations for the exact methods used for
your data and parameters; `pipeline.svg` draws the same pipeline, one box per
step (`pipeline.mmd` is the same graph in [Mermaid](https://mermaid.js.org/));
`pipeline_config.toml` holds the resolved settings (reusable with `--config`),
and `qsmxt.log` the log.

Some things are only decided as a run goes: a mask supplied with
`--use-custom-masks` that cannot be used is computed instead, R2′ is measured
where a multi-echo spin-echo scan allows and estimated otherwise. Once every run
is done, `methods.md` and the graph are written from what the runs did: a choice
every run made is stated as done, and the graph's boxes say how many runs
skipped a step or decided something in it. They cover every run in the output
directory that completed with the current settings, whichever command
processed it, so re-running one subject with `--include` does not narrow them;
a run last processed with other settings, or whose last attempt did not finish,
is left out. A run processed differently from the
rest gets its own methods text and graph in `changes/` (`<run>_methods.md`,
`<run>_desc-ran_pipeline.mmd` and `.svg`); the folder is only there when some
run differs. Each run's methods text, a record of what each step did and why
(`*_runrecord.json`), and its graph are also kept in its `workflow/` directory.

`qsmxt run --dry` prints the pipeline graph and the methods text as planned,
without running anything or writing any file.

### The reference summary

Every susceptibility map is referenced (see
[Referencing](/QSMxT/background/pipeline/#referencing)), and each map's sidecar
records what with `QsmReference` (`mean`, `none`, or the region), `QsmReferenceOffsetPpm`
(the value subtracted), `QsmReferenceVoxels` (how many voxels it was measured over)
and, for a region, `QsmReferenceLabels` (its label ids). The same facts for every
map of every run are collected in `desc-reference_summary.tsv` at the top of the
derivatives:

| Column | Meaning |
| --- | --- |
| `subject`, `session`, `acquisition`, `reconstruction`, `inversion`, `run` | The run's BIDS entities |
| `map` | The map, by its BIDS suffix with any `desc-` (`Chimap`, `desc-singlepass_Chimap`, `desc-cosmos_Chimap`, …) |
| `reference` | What zero was pinned to (`QsmReference`) |
| `offset_ppm` | The value subtracted, in ppm (`QsmReferenceOffsetPpm`) |
| `n_voxels` | Voxels the reference was measured over (`QsmReferenceVoxels`) |

It is the file to scan for a subject whose reference region came out small or
empty.
