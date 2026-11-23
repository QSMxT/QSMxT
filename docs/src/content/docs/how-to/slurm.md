---
title: Run on a cluster (SLURM)
description: "Run QSMxT over a cohort on an HPC cluster with SLURM."
---

**Goal:** process a whole cohort on a SLURM cluster, one job per scan.

## Steps

1. Install QSMxT in your home directory on the cluster, with the same one-line command as on
   a laptop ([Install](/QSMxT/start/install/)). No administrator is needed.

2. Convert and check the data (on a login node or a small interactive job):

   ```sh
   qsmxt dicom-convert /data/dicoms study/bids
   qsmxt validate study/bids
   ```

3. Write and submit the jobs. `--account` is the SLURM account to charge:

   ```sh
   qsmxt slurm study/bids --account myaccount --submit
   ```

   Any `qsmxt run` option works here too, for example `--config pipeline_config.toml` to use
   settings you tried on one subject first.

In the TUI, set the **Execution** tab to SLURM, fill in the account, and run.

## Check it worked

`squeue -u $USER` shows one job per scan and one aggregation job, which waits until every
scan's job has finished. When it is done, the output looks the same as a local
`qsmxt run`: maps under `derivatives/qsmxt/sub-*/anat/`, `methods.md` at the top. Each scan's
log is `derivatives/qsmxt/logs/<run>.log`; SLURM's own output is beside the job scripts in
`derivatives/qsmxt/slurm/`.

## If a job fails

1. Read its log in `derivatives/qsmxt/logs/`, and fix the cause.
2. Submit that scan's job again: `sbatch derivatives/qsmxt/slurm/qsmxt_<run>.sh`. Steps that
   already finished are not repeated.
3. Then submit the aggregation job yourself:
   `sbatch derivatives/qsmxt/slurm/qsmxt_aggregate.sh`.

## Options you may need

- `--partition` and `--time` (default 2 hours per job).
- `--mem`: memory per job in GB. By default each job asks for an estimate from its scan's size.
- Without `--submit`, the scripts are written but not submitted; submit them later with
  `bash derivatives/qsmxt/slurm/submit.sh`.

How the jobs divide the work, and what the aggregation job does, is in
[SLURM jobs](/QSMxT/reference/slurm/).
