---
title: Running noninteractively
description: Drive QSMxT from the command line — convert DICOMs, run the QSM pipeline, and scale out to HPC.
---

For scripting, batch processing, and reproducible runs, drive QSMxT from the
command line. (Prefer to point and click? See
[Running interactively](/QSMxT/guides/running-interactively/) — everything below is
available there too.)

:::tip[Don't memorise flags]
The easiest way to build a `qsmxt run` command is to configure your pipeline in
the [TUI](/QSMxT/guides/running-interactively/) — it shows the equivalent command
and updates it as you go. Copy the generated command (and reuse its saved config)
and run it here.
:::

## Running the pipeline

`qsmxt run` is the heart of the tool: it discovers every phase/magnitude run in a
BIDS dataset and reconstructs a quantitative susceptibility map for each.

```sh
qsmxt run <BIDS_DIR> [OUTPUT_DIR]
```

- `<BIDS_DIR>` — input BIDS directory
- `[OUTPUT_DIR]` — defaults to `<BIDS_DIR>`; results go to
  `<OUTPUT_DIR>/derivatives/qsmxt/`

With no options, QSMxT runs with sensible defaults end to end. Need a BIDS dataset
first? See [Converting DICOMs to BIDS](#converting-dicoms-to-bids) below.

### The stages

Each run flows through the pipeline below. The method at every stage is
configurable — see [Algorithms](/QSMxT/reference/algorithms/).

1. **Masking** — generate a brain/region mask (`threshold` or `bet`).
2. **Phase offset removal** — remove receiver phase offsets on multi-echo data.
3. **Phase unwrapping** — `romeo` or `laplacian`.
4. **Echo combination / B0 mapping** — combine echoes into a field map
   (`--b0-estimation`, `--b0-weight-type`).
5. **Background field removal** — `vsharp`, `pdf`, `lbv`, and more.
6. **Dipole inversion** — `rts`, `tv`, `tgv`, `medi`, … (10 algorithms).
7. **Referencing** — reference the susceptibility values (e.g. to the mean).

### Choosing algorithms

```sh
qsmxt run study/bids \
  --mask-preset robust-threshold \
  --unwrapping-algorithm romeo \
  --bf-algorithm vsharp \
  --qsm-algorithm rts
```

| Option | Choices |
| --- | --- |
| `--qsm-algorithm` | `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `ilsqr`, `qsmart` |
| `--unwrapping-algorithm` | `romeo`, `laplacian` |
| `--bf-algorithm` | `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella` |
| `--mask-preset` | `robust-threshold`, `bet`, `hd-bet` |
| `--masking-input` | `magnitude-first`, `magnitude`, `magnitude-last`, `phase-quality` |

`--masking-input` overrides the image the mask is computed from and combines
with `--mask-preset` — e.g. `--mask-preset bet --masking-input magnitude-first`
runs BET on the first-echo magnitude. For fully custom masking, see `--mask`
in the [command reference](/QSMxT/reference/commands/).

### Selecting runs

Process only part of a dataset with glob patterns:

```sh
# Only these subjects
qsmxt run study/bids --include "sub-01*" "sub-02*"

# Everything except a session
qsmxt run study/bids --exclude "*ses-pilot*"

# Cap the number of echoes used
qsmxt run study/bids --num-echoes 4
```

### Multi-echo phase handling

```sh
# Receiver phase-offset removal (on by default for multi-echo)
qsmxt run study/bids --phase-offset-removal true

# Bipolar readout correction (needs ≥ 3 echoes)
qsmxt run study/bids --bipolar-correction
```

ROMEO unwrapping has several modes — per-echo (default) or template-based,
with inter-echo 2π correction. See `qsmxt run --help` for the full set of
`--romeo-*` flags.

### Using a configuration file

Reproduce a run exactly by saving settings to a TOML file and passing `--config`:

```sh
qsmxt init > pipeline.toml      # generate a starting config
qsmxt run study/bids --config pipeline.toml
```

See [Configuration](/QSMxT/reference/configuration/).

### Re-running

Intermediate results are cached to disk. Re-running the same dataset skips any
stage that already completed, so tweaking a late-stage option doesn't recompute
the whole pipeline.

### Output

```
study/bids/derivatives/qsmxt/
└── sub-01/
    └── anat/
        ├── sub-01_…_Chimap.nii.gz
        ├── sub-01_…_mask.nii.gz
        └── sub-01_…_desc-qsm_mask.nii.gz
```

`mask` is the brain mask; `desc-qsm_mask` is the part of it the susceptibility map
is defined on — the brain mask less the rim that background-field removal eroded.
The `Chimap` is referenced over, and is 0 outside, `desc-qsm_mask`.

A `references.txt` accompanies the outputs, citing the exact methods used for
your data and parameters.

### Exporting results as DICOM

Some workflows want the maps back in DICOM — to push them into a PACS, open them
in a clinical viewer, or hand them to a radiologist beside the source study.
`--export-dicom` writes a DICOM series for every final map alongside the NIfTIs:

```sh
qsmxt run study/bids --export-dicom
```

Each map gets its own folder under the subject's `extra_files/`, one file per
slice:

```
study/bids/derivatives/qsmxt/
└── sub-01/
    ├── anat/
    │   └── sub-01_…_Chimap.nii.gz
    └── extra_files/
        └── sub-01_Chimap_dicoms/
            ├── sub-01_Chimap_0001.dcm
            ├── sub-01_Chimap_0002.dcm
            └── …
```

Every final map the run produced is exported. χ needs no extra flag; the
supplementary maps are exported once you've asked for them:

| Map | Produced by | Folder suffix / `--dicom-outputs` token |
| --- | --- | --- |
| Susceptibility (χ) | the default pipeline | `Chimap` |
| SWI | `--do-swi` | `swi` |
| SWI minIP | `--do-swi` | `minIP` |
| T2\* | `--do-t2starmap` | `T2starmap` |
| R2\* | `--do-r2starmap` | `R2starmap` |
| R2 | `--do-r2map` | `R2map` |
| R2′ | `--do-r2primemap` | `R2primemap` |
| Paramagnetic χ | `--do-chisep` | `desc-paramagnetic_Chimap` |
| Diamagnetic χ | `--do-chisep` | `desc-diamagnetic_Chimap` |
| Total χ | `--do-chisep` | `desc-total_Chimap` |

To export only some of them, pass those tokens to `--dicom-outputs`
(case-insensitive, comma- or space-separated):

```sh
qsmxt run study/bids --do-chisep --export-dicom \
  --dicom-outputs chimap,desc-paramagnetic_chimap,desc-diamagnetic_chimap
```

SMWI, the [segmentation](/QSMxT/reference/algorithms/#segmentation-and-per-structure-statistics)
and the [multi-orientation](/QSMxT/reference/algorithms/#multi-orientation-cosmos-and-sti)
reconstructions stay NIfTI-only.

**Filing them beside the source study.** By default the derived series carry a synthesised identity: patient fields are
left empty and a stable `StudyInstanceUID` is derived from the subject and
session. That's fine for a local viewer, but a PACS will file the maps as a study
of their own. Point `--source-dicom` at the original DICOMs and QSMxT inherits the
identity from the first readable DICOM it finds there — `PatientID`,
`StudyInstanceUID`, `FrameOfReferenceUID`, study date/time, accession number — so
the maps land beside the acquisition they came from:

```sh
qsmxt run study/bids --export-dicom --source-dicom /data/dicoms/sub-01
```

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

A few more things worth knowing:

- Slice geometry (`ImageOrientationPatient`, `ImagePositionPatient`,
  `SliceLocation`, spacing) is read from the output NIfTI's affine, so the series
  overlays the source acquisition in a viewer and reflects any
  [oblique resampling](/QSMxT/reference/algorithms/#oblique-acquisitions) the run
  applied.
- Instances are uncompressed *MR Image Storage*, Explicit VR Little Endian.
- Export is best-effort: a failure is logged and the run still succeeds.
- `--export-dicom` is a pipeline setting, so it can live in a
  [config file](/QSMxT/reference/configuration/) as `export_dicom = true` under
  `[pipeline]`, and it carries through to [`qsmxt slurm`](#hpc--slurm).
  `--source-dicom` and `--dicom-outputs` are flags on `qsmxt run` itself.

This is the mirror image of [`dicom-convert`](#converting-dicoms-to-bids) below,
which brings DICOMs *in*. Both directions are built into the binary — no external
converter to install.

## Converting DICOMs to BIDS

QSMxT needs [BIDS](https://bids.neuroimaging.io/)-formatted input. The
`dicom-convert` command builds it for you from a directory of DICOMs, applying an
automatic best-guess classification to every series with no prompts — ideal for
scripting and batch jobs.

```sh
qsmxt dicom-convert <DICOM_DIR> <OUTPUT_DIR>
```

- `<DICOM_DIR>` — input directory, searched recursively
- `<OUTPUT_DIR>` — output BIDS directory

:::tip[Want to review as you go?]
For a new dataset, converting [interactively](/QSMxT/guides/running-interactively/)
in the TUI lets you **inspect the automatic classification and relabel anything it
got wrong** before writing. The command above is the same conversion, unattended.
:::

### Preview first with `--dry-run`

Before writing anything, inspect how QSMxT classifies your data:

```sh
qsmxt dicom-convert /data/dicoms study/bids --dry-run
```

Example output:

```
Detected 4 unique series (auto-classified):
  acq-SWI3mm   SWI_3mm   [Magnitude] 2×TEs=[9.4…19.7]ms (uncombined, 32 coils)
  acq-SWI3mm   SWI_3mm   [Magnitude] 2×TEs=[9.4…19.7]ms
  acq-SWI3mm   SWI_3mm   [Magnitude] 2×TEs=[9.4…19.7]ms (filtered)
  acq-SWI3mm   SWI_3mm   [Phase]     2×TEs=[9.4…19.7]ms
```

Each row is a unique series, deduplicated across subjects, so you review a
classification once even if many subjects share it.

### What it handles

- **Gradient-echo magnitude and phase** — split correctly even when a vendor packs
  both under a single `SeriesInstanceUID` (e.g. some Philips/GE exports).
- **Multi-echo** — echoes are detected and labelled `echo-N`; the dry run reports
  the echo count and TE range.
- **T1-weighted structurals** — MPRAGE / MP2RAGE / T1w series are recognised as
  `T1w`, not magnitude.
- **Individual coil images** — each coil element of an uncombined acquisition is
  converted separately and labelled `rec-uncombined_coil-NN`, instead of
  overwriting one another.
- **Derived reconstructions** — a scanner-filtered magnitude (e.g. a SWI-filtered
  series) is kept alongside the plain one with a `desc-filtered` label rather than
  colliding with it.

Output is a BIDS tree of `sub-*/anat/` NIfTIs with JSON sidecars (`EchoTime` in
seconds), ready for the pipeline. Already have NIfTIs? You can assemble a BIDS
dataset by hand — see [Input data](/QSMxT/reference/inputs/) for the expected
layout, and run [`qsmxt validate`](/QSMxT/reference/inputs/#checking-a-dataset)
to check it's pipeline-ready.

## HPC & SLURM

For large cohorts, QSMxT can fan work out across an HPC cluster instead of running
everything on one machine.

```sh
qsmxt slurm study/bids
```

This produces SLURM batch scripts for your dataset that you can submit with
`sbatch`. Each job runs the same pipeline as `qsmxt run`, so results are identical
to a local run — just distributed. Run `qsmxt slurm --help` for the available
options (partition, time limits, resource requests, and run selection).

`qsmxt slurm` accepts the same pipeline options as `qsmxt run` (algorithms,
masking, per-algorithm parameters, `--config`, …), so any `qsmxt run` command
becomes a SLURM one by swapping the subcommand and adding the scheduler flags:

```sh
qsmxt slurm study/bids --qsm-algorithm tgv --unwrapping-algorithm laplacian \
  --account myaccount --time 04:00:00
```

The resolved pipeline is written to
`<OUTPUT_DIR>/derivatives/qsmxt/pipeline_config.toml` and each generated job
runs against that file.

:::note
SLURM submission is available in the [TUI](/QSMxT/guides/running-interactively/)
too — you don't have to use the command line to scale out.
:::

### Parallelism on a single node

Even without a scheduler, a single `qsmxt run` is parallel. QSMxT processes runs
concurrently and detects available memory to choose a safe degree of parallelism
automatically, so it fills your cores without exhausting RAM.

### Caching plays nicely with retries

Because intermediate results are cached to disk, a job that is requeued or resumed
picks up where it left off rather than recomputing completed stages — handy when a
cluster preempts or time-limits a job.

### A typical cluster workflow

1. Convert and validate on a login node or small interactive job:
   ```sh
   qsmxt dicom-convert /data/dicoms study/bids
   qsmxt validate study/bids
   ```
2. Dial in settings (locally or via the [TUI](/QSMxT/guides/running-interactively/)) and save them:
   ```sh
   qsmxt init > pipeline.toml
   ```
3. Generate and submit jobs:
   ```sh
   qsmxt slurm study/bids
   sbatch …
   ```
