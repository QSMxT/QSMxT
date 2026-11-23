---
title: Run on a cluster (SLURM)
description: "Run QSMxT over a cohort on an HPC cluster with SLURM."
---

## HPC & SLURM

For large cohorts, QSMxT can fan work out across an HPC cluster instead of running
everything on one machine.

```sh
qsmxt slurm study/bids --account myaccount
```

`--account` (the SLURM account to charge) is required. This writes SLURM batch
scripts for your dataset to `<OUTPUT_DIR>/derivatives/qsmxt/slurm/`, and with
`--submit` submits them. Run
`qsmxt slurm --help` for the available options (partition, time limits,
resource requests, and run selection).

`qsmxt slurm` accepts the same pipeline options as `qsmxt run` (algorithms,
masking, per-algorithm parameters, `--config`, `--num-echoes`, `--force`,
`--clean-intermediates`, `--debug`, and DICOM export's `--source-dicom` and
`--dicom-outputs`, …), so any `qsmxt run` command becomes a SLURM one by
swapping the subcommand and adding the scheduler flags:

```sh
qsmxt slurm study/bids --qsm-algorithm tgv --unwrapping-algorithm laplacian \
  --account myaccount --time 04:00:00
```

The settings are checked, and every run is checked against them, before any
script is written, so a bad setting is reported once rather than by every job.
The resolved pipeline is written to
`<OUTPUT_DIR>/derivatives/qsmxt/pipeline_config.toml` and every generated job
runs against that file.

### How the jobs divide the work

- **One job per run** (`qsmxt_<run>.sh`) runs
  `qsmxt run --no-dataset-steps --include <run>`: that run's pipeline and
  nothing else. It logs to `logs/<run>.log` and writes nothing at dataset level.
- **One aggregation job** (`qsmxt_aggregate.sh`) runs `qsmxt aggregate` over
  all the runs once every run job has succeeded
  (`--dependency=afterok:<all run job ids>`). It does what a local `qsmxt run`
  does after its runs finish:
  - multi-orientation (COSMOS/STI) reconstruction across the runs of each group;
  - `desc-reference_summary.tsv`, the dataset-level statistics table and cohort
    figures;
  - BIDS-Prov records under `prov/`;
  - `dataset_description.json`, `pipeline_config.toml`, `methods.md`, `pipeline.mmd`/`.svg`, `changes/`;
  - `qsmxt.log`, which collects every run's log followed by its own;
  - `--clean-intermediates`, if you asked for it.

The result is the same dataset a local `qsmxt run` produces, apart from
timestamps and the command recorded in the provenance (`qsmxt slurm …`).

`--submit` submits the run jobs and then the aggregation job with the
dependency. Without it, `bash <OUTPUT_DIR>/derivatives/qsmxt/slurm/submit.sh`
does the same.

Each job's memory request is estimated from its run's size and the pipeline.
The aggregation job is sized for the largest multi-orientation group. `--mem`
sets one figure for every job instead, with a warning for any run estimated to
need more.

If a run job fails, SLURM never starts the aggregation job: with `afterok` it
stays pending (or is cancelled, depending on the cluster's settings). Fix the
cause, re-run that run's script (`sbatch slurm/qsmxt_<run>.sh`; its finished
steps are cached), then submit `slurm/qsmxt_aggregate.sh` yourself. `qsmxt
aggregate` refuses any run that has not completed under the same
configuration, and names it.

:::note
SLURM submission is available in the [TUI](/QSMxT/background/tui/)
too — you don't have to use the command line to scale out.
:::

### Parallelism on a single node

Even without a scheduler, a single `qsmxt run` is parallel. QSMxT processes runs
concurrently and detects available memory to choose a safe degree of parallelism
automatically, so it fills your cores without exhausting RAM. Inside a Slurm job or a
container, the job's or container's memory limit counts, not the whole machine's memory:
the log's `Memory:` line says which applied. `--mem-limit-gb` sets the limit yourself.

`qsmxt run`, `slurm`, `aggregate` and `validate` exit with an error (status 1) when the
BIDS directory does not exist, or when it has no runs to process (none at all, or none
that match `--include`, `--exclude` and `--num-echoes`).

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
2. Dial in settings (locally or via the [TUI](/QSMxT/background/tui/)) and save them:
   ```sh
   qsmxt init > pipeline.toml
   ```
3. Generate and submit the jobs (one per run, then the aggregation job):
   ```sh
   qsmxt slurm study/bids --config pipeline.toml --account myaccount --submit
   ```
