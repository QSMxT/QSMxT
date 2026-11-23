---
title: Non-BIDS or partial data
description: "NIfTI files, missing sidecars, and masks or maps from other tools."
---

QSMxT runs on BIDS datasets. If you have NIfTI files rather than DICOMs, or want to use a mask
or map made by another tool, this page is for you.

## What QSMxT does automatically

- It finds a scan from its phase images, `sub-*/[ses-*/]anat/*_part-phase_*.nii[.gz]`, with
  magnitude beside them as `part-mag` and multiple echoes as `echo-<N>`.
- It reads `EchoTime` and `MagneticFieldStrength` from each image's JSON sidecar, and uses
  `ImagingFrequency`, `Manufacturer` and `SliceThickness` when they are there.
- It uses masks and maps from `derivatives/<TOOL>/` when you ask it to.

## What it doesn't do / limits

- A scan without `EchoTime` or `MagneticFieldStrength` can't be processed.
- A supplied map has to be on the scan's grid. One that isn't is refused (χ, R2, R2′) or
  replaced by one QSMxT makes, with a warning (a mask).

## What you can do

**NIfTI files:** arrange them as BIDS by hand:

```
study/bids/sub-01/anat/sub-01_echo-1_part-mag_MEGRE.nii.gz
study/bids/sub-01/anat/sub-01_echo-1_part-mag_MEGRE.json     ← {"EchoTime": 0.004, "MagneticFieldStrength": 3}
study/bids/sub-01/anat/sub-01_echo-1_part-phase_MEGRE.nii.gz
study/bids/sub-01/anat/sub-01_echo-1_part-phase_MEGRE.json
…
```

Then run `qsmxt validate study/bids`. The TUI's **Input mode → NIfTI -> BIDS** does this for
you from a folder of NIfTI files: it fills in the echo times and field strength from any
sidecars, and you type in what is missing.

**Your own mask or maps:** put them in `study/bids/derivatives/<TOOL>/sub-01/anat/` and name
the tool:

| Option | Supplies | File name ends |
|---|---|---|
| `--use-custom-masks <TOOL>` | brain mask | `_mask.nii` |
| `--use-custom-qsm <TOOL>` | χ map, for source separation | `_Chimap.nii` |
| `--use-custom-r2 <TOOL>` | R2 map | `_R2map.nii` |
| `--use-custom-r2prime <TOOL>` | R2′ map | `_R2primemap.nii` |
| `--use-custom-dseg <TOOL>` | parcellation | `_dseg.nii` |

**Single files, no dataset:** each pipeline step is also a command that works on NIfTI files
directly (`qsmxt invert`, `qsmxt bgremove`, …): see [Standalone tools](/QSMxT/reference/tools/).

More: [Input data layout](/QSMxT/reference/inputs/).
