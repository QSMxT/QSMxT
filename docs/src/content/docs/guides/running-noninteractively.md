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
  `<OUTPUT_DIR>/derivatives/qsmxt.rs/`

With no options, QSMxT runs with sensible defaults end to end. Need a BIDS dataset
first? See [Converting DICOMs to BIDS](#converting-dicoms-to-bids) below.

### The stages

Each run flows through the pipeline below. The method at every stage is
configurable — see [Algorithms](/QSMxT/reference/algorithms/).

1. **Masking** — generate a brain/region mask (`threshold` or `bet`).
2. **Phase offset removal** — remove receiver phase offsets on multi-echo data.
3. **Phase unwrapping** — `romeo` or `laplacian`.
4. **Echo combination / B0 mapping** — combine echoes into a field map.
5. **Background field removal** — `vsharp`, `pdf`, `lbv`, and more.
6. **Dipole inversion** — `rts`, `tv`, `tgv`, `medi`, … (10 algorithms).
7. **Referencing** — reference the susceptibility values (e.g. to the mean).

### Choosing algorithms

```sh
qsmxt run study/bids \
  --masking-algorithm threshold \
  --unwrapping-algorithm romeo \
  --bf-algorithm vsharp \
  --qsm-algorithm rts
```

| Option | Choices |
| --- | --- |
| `--qsm-algorithm` | `rts`, `tv`, `tkd`, `tsvd`, `tgv`, `tikhonov`, `nltv`, `medi`, `ilsqr`, `qsmart` |
| `--unwrapping-algorithm` | `romeo`, `laplacian` |
| `--bf-algorithm` | `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella` |
| `--masking-algorithm` | `bet`, `threshold` |
| `--masking-input` | `magnitude-first`, `magnitude`, `magnitude-last`, `phase-quality` |

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
study/bids/derivatives/qsmxt.rs/
└── sub-01/
    └── anat/
        └── sub-01_…_Chimap.nii.gz
```

A `references.txt` accompanies the outputs, citing the exact methods used for
your data and parameters.

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
dataset by hand — QSMxT only needs magnitude/phase pairs with valid sidecars; run
[`qsmxt validate`](/QSMxT/reference/commands/) to check it's pipeline-ready.

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
