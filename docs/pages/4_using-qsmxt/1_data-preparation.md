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

QSMxT requires <a href="https://bids.neuroimaging.io/" target="_blank" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Click to read about BIDS at https://bids.neuroimaging.io/.">BIDS</a>-conforming data. You can use `dicom-sort`, `dicom-convert` and `nifti-convert` to convert your data to BIDS, depending on whether you have DICOM or NIfTI images. 

The data conversion tools packaged in QSMxT are only intended to work with a subset of BIDS. This subset represents the minimum BIDS specification to enable QSM post-processing. The tools are not designed to convert, for example, Diffusion-Weighted Imaging (DWI), functional MRI (fMRI), T2-weighted imaging, or other imaging modalities. Any images not needed for QSM will be left in the extra_data directory for the relevant subject-session folder. For QSM, relevant BIDS suffixes include T2starw and MEGRE, including magnitude and phase parts. T1-weighted imaging is also supported to enable segmentation and registration to the QSM space. Some derived data including brain masks may be used in some QSM pipelines and are also supported.

For example BIDS structures, see [BIDS Examples](#bids-examples).

## DICOM to BIDS

To convert DICOM to BIDS using `dicom-convert`, your data must first be organized into folders by subject, session, and series. For example:

```bash
dicoms/
├── sub-1
│   └── ses-20190606
│       ├── 3_t1_mprage_sag_p2
│       ├── 6_gre_qsm_5echoes_Iso1mm
│       └── 7_gre_qsm_5echoes_Iso1mm
├── sub-2
│   └── ses-20190528
│       ├── 10_gre_qsm_5echoes_Iso1mm
│       ├── 11_gre_qsm_5echoes_Iso1mm
│       └── 3_t1_mprage_sag_p2
```

To automatically sort your DICOM images, use `dicom-sort`:

```bash
dicom-sort YOUR_DICOM_DIR/ dicoms-sorted/
```

To convert to BIDS, use `dicom-convert`:

```bash
dicom-convert dicoms-sorted/ bids/
```

Carefully read the output to ensure data were correctly recognized and converted. Crucially, the `dicom-convert` script needs to know which of your acquisitions are intended for QSM, and which are T1-weighted and suitable for segmentation. It identifies this based on the DICOM `ProtocolName` field and looks for the patterns `*qsm*` and `*t2starw*` for the QSM-intended series and `t1w` for the T1-weighted series. You can specify your patterns using command-line arguments, e.g.:

```bash
dicom-convert dicoms-sorted/ bids/ --t2starw_protocol_patterns '*gre*' --t1w_protocol_patterns '*mp2rage*'
```

## NIfTI to BIDS

To convert NIfTI to BIDS, use `nifti-convert`:

```bash
nifti-convert YOUR_NIFTI_DIR/ bids/
```

Carefully read the output to ensure data were correctly recognized. The script will write a .CSV spreadsheet to file to be filled with BIDS entity information and NIfTI JSON information. If you are unsure how to complete the spreadsheet, please see [anatomical imaging data](https://bids-specification.readthedocs.io/en/stable/04-modality-specific-files/01-magnetic-resonance-imaging-data.html#anatomy-imaging-data) section on the BIDS specification for details about each entity in the `anat` datatype. Note that `nifti-convert` currently only supports the BIDS `anat` datatype, which is sufficient for studies in QSM. Ensure you also fill the `MagneticFieldStrength` and `EchoTime` fields, which are necessary for QSM calculation. Once you have filled the spreadsheet, run the script again to complete the conversion.


# BIDS Examples

The following are examples of valid BIDS structures that are suitable for processing using QSMxT.

Crucially, distinct groups of files suitable for a single QSM reconstruction are distinguished from one another by their subject name, optional session directory, optional acquisition name, optional run number, and suffix.

## Minimal example (single subject, single session, single echo)

```
bids/
├── sub-1
│   └── anat
│       ├── sub-1_part-mag_T2starw.json
│       ├── sub-1_part-mag_T2Starw.nii
│       ├── sub-1_part-phase_T2Starw.json
│       └── sub-1_part-phase_T2Starw.nii
```

## Minimal example including T1-weighted imaging for segmentation

```
bids/
├── sub-1
│   └── anat
│       ├── sub-1_part-mag_T2starw.json
│       ├── sub-1_part-mag_T2Starw.nii
│       ├── sub-1_part-phase_T2Starw.json
│       ├── sub-1_part-phase_T2Starw.nii
│       ├── sub-1_T1w.nii
│       └── sub-1_T1w.json
```

## Minimal example including brain mask (for QSMxT's --use_existing_masks option)

```
bids/
├── derivatives
│   └── qsm-forward
│       └── sub-1
│           └── anat
│               └── sub-1_mask.nii
├── sub-1
│   └── anat
│       ├── sub-1_part-mag_T2starw.json
│       ├── sub-1_part-mag_T2Starw.nii
│       ├── sub-1_part-phase_T2Starw.json
│       ├── sub-1_part-phase_T2Starw.nii
│       ├── sub-1_T1w.nii
│       └── sub-1_T1w.json
```

## Multi-echo example

```
bids/
├── sub-1
│   └── anat
│       ├── sub-1_echo-1_part-mag_MEGRE.json
│       ├── sub-1_echo-1_part-mag_MEGRE.nii
│       ├── sub-1_echo-1_part-phase_MEGRE.json
│       ├── sub-1_echo-1_part-phase_MEGRE.nii
│       ├── sub-1_echo-2_part-mag_MEGRE.json
│       ├── sub-1_echo-2_part-mag_MEGRE.nii
│       ├── sub-1_echo-2_part-phase_MEGRE.json
│       └── sub-1_echo-2_part-phase_MEGRE.nii
```

## Multiple runs and acquisitions example

```
bids/
├── sub-1
│   └── anat
│       ├── sub-1_acq-mygrea_echo-1_run-1_part-mag_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-1_run-1_part-mag_MEGRE.nii
│       ├── sub-1_acq-mygrea_echo-1_run-1_part-phase_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-1_run-1_part-phase_MEGRE.nii
│       ├── sub-1_acq-mygrea_echo-2_run-1_part-mag_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-2_run-1_part-mag_MEGRE.nii
│       ├── sub-1_acq-mygrea_echo-2_run-1_part-phase_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-2_run-1_part-phase_MEGRE.nii
│       ├── sub-1_acq-mygrea_echo-1_run-2_part-mag_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-1_run-2_part-mag_MEGRE.nii
│       ├── sub-1_acq-mygrea_echo-1_run-2_part-phase_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-1_run-2_part-phase_MEGRE.nii
│       ├── sub-1_acq-mygrea_echo-2_run-2_part-mag_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-2_run-2_part-mag_MEGRE.nii
│       ├── sub-1_acq-mygrea_echo-2_run-2_part-phase_MEGRE.json
│       ├── sub-1_acq-mygrea_echo-2_run-2_part-phase_MEGRE.nii
│       ├── sub-1_acq-mygreb_echo-1_run-1_part-mag_MEGRE.json
│       ├── sub-1_acq-mygreb_echo-1_run-1_part-mag_MEGRE.nii
│       ├── sub-1_acq-mygreb_echo-1_run-1_part-phase_MEGRE.json
│       ├── sub-1_acq-mygreb_echo-1_run-1_part-phase_MEGRE.nii
│       ├── sub-1_acq-mygreb_echo-2_run-1_part-mag_MEGRE.json
│       ├── sub-1_acq-mygreb_echo-2_run-1_part-mag_MEGRE.nii
│       ├── sub-1_acq-mygreb_echo-2_run-1_part-phase_MEGRE.json
│       └── sub-1_acq-mygreb_echo-2_run-1_part-phase_MEGRE.nii
```

## Multiple sessions example

```
bids/
├── sub-2
│   ├── ses-20231020
│   │   └── anat
│   │       ├── sub-2_ses-20231020_part-mag_T2starw.json
│   │       ├── sub-2_ses-20231020_part-mag_T2Starw.nii
│   │       ├── sub-2_ses-20231020_part-phase_T2Starw.json
│   │       ├── sub-2_ses-20231020_part-phase_T2Starw.nii
│   └── ses-20231025
│       └── anat
│           ├── sub-2_ses-20231025_echo-1_part-mag_MEGRE.json
│           ├── sub-2_ses-20231025_echo-1_part-mag_MEGRE.nii
│           ├── sub-2_ses-20231025_echo-1_part-phase_MEGRE.json
│           ├── sub-2_ses-20231025_echo-1_part-phase_MEGRE.nii
│           ├── sub-2_ses-20231025_echo-2_part-mag_MEGRE.json
│           ├── sub-2_ses-20231025_echo-2_part-mag_MEGRE.nii
│           ├── sub-2_ses-20231025_echo-2_part-phase_MEGRE.json
│           ├── sub-2_ses-20231025_echo-2_part-phase_MEGRE.nii
```

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

