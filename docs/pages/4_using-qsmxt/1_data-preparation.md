---
layout: default
title: Data preparation
nav_order: 1
parent: Using QSMxT
permalink: /using-qsmxt/data-preparation
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# Data preparation

QSMxT requires <a href="https://bids.neuroimaging.io/" target="_blank" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Click to read about BIDS at https://bids.neuroimaging.io/.">BIDS</a>-conforming data. You can use `dicom-convert` and `nifti-convert` to convert your data to BIDS, depending on whether you have DICOM or NIfTI images. 

The data conversion tools packaged in QSMxT are only intended to support a subset of BIDS. This subset represents the minimum BIDS specification to enable QSM post-processing. The tools are not designed to convert, for example, Diffusion-Weighted Imaging (DWI), functional MRI (fMRI), T2-weighted imaging, or other imaging modalities. For QSM, relevant BIDS suffixes include T2starw and MEGRE, including magnitude and phase parts. T1-weighted imaging is also supported to enable segmentation and registration to the QSM space. Some derived data, including brain masks, may be used in some QSM pipelines and are also supported.

For example BIDS structures, see [BIDS Examples](#bids-examples).

## DICOM to BIDS

To convert to BIDS, use `dicom-convert`, which runs interactively:

```bash
dicom-convert dicoms/ bids/
```

To run `dicom-convert` non-interactively, use `--auto_yes`. Running it this way is less flexible because it does not provide the interfaces used to identify individual series from each acquisition and instead relies on heuristics, but these work for most data:

```bash
dicom-convert dicoms/ bids/ --auto_yes
```

## NIfTI to BIDS

To convert NIfTI to BIDS, use `nifti-convert`:

```bash
nifti-convert YOUR_NIFTI_DIR/ bids/
```

# UK BioBank Data

QSMxT can process and analyse UK BioBank data. Please see the [GitHib issue](https://github.com/QSMxT/QSMxT/issues/115#issuecomment-2017360415) for detailed instructions.

# BIDS Examples

The following are examples of valid BIDS structures suitable for processing using QSMxT.

Crucially, distinct groups of files suitable for a single QSM reconstruction are distinguished by their subject name, optional session directory, optional acquisition name, optional run number, and suffix.

## Minimal example (single subject, single session, single echo)

```
bids/
└── sub-1
    └── anat
        ├── sub-1_part-mag_T2starw.json
        ├── sub-1_part-mag_T2Starw.nii
        ├── sub-1_part-phase_T2Starw.json
        └── sub-1_part-phase_T2Starw.nii
```

## Minimal example including T1-weighted imaging for segmentation

```
bids/
└── sub-1
    └── anat
        ├── sub-1_part-mag_T2starw.json
        ├── sub-1_part-mag_T2Starw.nii
        ├── sub-1_part-phase_T2Starw.json
        ├── sub-1_part-phase_T2Starw.nii
        ├── sub-1_T1w.nii
        └── sub-1_T1w.json
```

## Minimal example including brain mask (for QSMxT's --use_existing_masks option)

```
bids/
├── derivatives
│   └── qsm-forward
│       └── sub-1
│           └── anat
│               └── sub-1_mask.nii
└── sub-1
    └── anat
        ├── sub-1_part-mag_T2starw.json
        ├── sub-1_part-mag_T2Starw.nii
        ├── sub-1_part-phase_T2Starw.json
        ├── sub-1_part-phase_T2Starw.nii
        ├── sub-1_T1w.nii
        └── sub-1_T1w.json
```

## Multi-echo example

```
bids/
└── sub-1
    └── anat
        ├── sub-1_echo-1_part-mag_MEGRE.json
        ├── sub-1_echo-1_part-mag_MEGRE.nii
        ├── sub-1_echo-1_part-phase_MEGRE.json
        ├── sub-1_echo-1_part-phase_MEGRE.nii
        ├── sub-1_echo-2_part-mag_MEGRE.json
        ├── sub-1_echo-2_part-mag_MEGRE.nii
        ├── sub-1_echo-2_part-phase_MEGRE.json
        └── sub-1_echo-2_part-phase_MEGRE.nii
```

## Multiple runs and acquisitions example

```
bids/
└── sub-1
    └── anat
        ├── sub-1_acq-mygrea_run-1_echo-1_part-mag_MEGRE.json
        ├── sub-1_acq-mygrea_run-1_echo-1_part-mag_MEGRE.nii
        ├── sub-1_acq-mygrea_run-1_echo-1_part-phase_MEGRE.json
        ├── sub-1_acq-mygrea_run-1_echo-1_part-phase_MEGRE.nii
        ├── sub-1_acq-mygrea_run-1_echo-2_part-mag_MEGRE.json
        ├── sub-1_acq-mygrea_run-1_echo-2_part-mag_MEGRE.nii
        ├── sub-1_acq-mygrea_run-1_echo-2_part-phase_MEGRE.json
        ├── sub-1_acq-mygrea_run-1_echo-2_part-phase_MEGRE.nii
        ├── sub-1_acq-mygrea_run-2_echo-1_part-mag_MEGRE.json
        ├── sub-1_acq-mygrea_run-2_echo-1_part-mag_MEGRE.nii
        ├── sub-1_acq-mygrea_run-2_echo-1_part-phase_MEGRE.json
        ├── sub-1_acq-mygrea_run-2_echo-1_part-phase_MEGRE.nii
        ├── sub-1_acq-mygrea_run-2_echo-2_part-mag_MEGRE.json
        ├── sub-1_acq-mygrea_run-2_echo-2_part-mag_MEGRE.nii
        ├── sub-1_acq-mygrea_run-2_echo-2_part-phase_MEGRE.json
        ├── sub-1_acq-mygrea_run-2_echo-2_part-phase_MEGRE.nii
        ├── sub-1_acq-mygreb_run-1_echo-1_part-mag_MEGRE.json
        ├── sub-1_acq-mygreb_run-1_echo-1_part-mag_MEGRE.nii
        ├── sub-1_acq-mygreb_run-1_echo-1_part-phase_MEGRE.json
        ├── sub-1_acq-mygreb_run-1_echo-1_part-phase_MEGRE.nii
        ├── sub-1_acq-mygreb_run-1_echo-2_part-mag_MEGRE.json
        ├── sub-1_acq-mygreb_run-1_echo-2_part-mag_MEGRE.nii
        ├── sub-1_acq-mygreb_run-1_echo-2_part-phase_MEGRE.json
        └── sub-1_acq-mygreb_run-1_echo-2_part-phase_MEGRE.nii
```

## Multiple sessions example

```
bids/
└── sub-2
    ├── ses-20231020
    │   └── anat
    │       ├── sub-2_ses-20231020_part-mag_T2starw.json
    │       ├── sub-2_ses-20231020_part-mag_T2Starw.nii
    │       ├── sub-2_ses-20231020_part-phase_T2Starw.json
    │       └── sub-2_ses-20231020_part-phase_T2Starw.nii
    └── ses-20231025
        └── anat
            ├── sub-2_ses-20231025_echo-1_part-mag_MEGRE.json
            ├── sub-2_ses-20231025_echo-1_part-mag_MEGRE.nii
            ├── sub-2_ses-20231025_echo-1_part-phase_MEGRE.json
            ├── sub-2_ses-20231025_echo-1_part-phase_MEGRE.nii
            ├── sub-2_ses-20231025_echo-2_part-mag_MEGRE.json
            ├── sub-2_ses-20231025_echo-2_part-mag_MEGRE.nii
            ├── sub-2_ses-20231025_echo-2_part-phase_MEGRE.json
            └── sub-2_ses-20231025_echo-2_part-phase_MEGRE.nii
```

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

