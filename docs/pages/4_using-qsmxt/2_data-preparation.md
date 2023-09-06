---
layout: default
title: Data preparation
nav_order: 2
parent: Using QSMxT
permalink: /using-qsmxt/data-preparation
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# Data preparation

QSMxT requires <a href="https://bids.neuroimaging.io/" target="_blank" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Click to read about BIDS at https://bids.neuroimaging.io/.">BIDS</a>-organised data. If your data does not conform to BIDS, you can use the tools `dicom-sort`, `dicom-convert` and `nifti-convert` to bring your images into this format, depending on whether you have DICOMs or NIfTI images.

## I have DICOMs

Use `dicom-sort` and `dicom-convert` to convert DICOMs to BIDS:

```bash
dicom-sort YOUR_DICOM_DIR/ dicoms-sorted/
dicom-convert dicoms-sorted/ bids/
```

Carefully read the output to ensure data were correctly recognized and converted. Crucially, the `dicom-convert` script needs to know which of your acquisitions are T2*-weighted and suitable for QSM, and which are T1-weighted and suitable for segmentation. It identifies this based on the DICOM `ProtocolName` field and looks for the patterns `*qsm*` and `*t2starw*` for the T2*-weighted series and `t1w` for the T1-weighted series. You can specify your patterns using command-line arguments, e.g.:

```bash
dicom-convert dicoms-sorted/ bids/ --t2starw_protocol_patterns '*gre*' --t1w_protocol_patterns '*mp2rage*'
```

## I have NIfTI files

To convert NIfTI to BIDS, use `nifti-convert`:

```bash
nifti-convert YOUR_NIFTI_DIR/ bids/
```

Carefully read the output to ensure data were correctly recognized. The script will write a .CSV spreadsheet to file to be filled with BIDS entity information and NIfTI JSON information. If you are unsure how to complete the spreadsheet, please see [anatomical imaging data](https://bids-specification.readthedocs.io/en/stable/04-modality-specific-files/01-magnetic-resonance-imaging-data.html#anatomy-imaging-data) section on the BIDS specification for details about each entity in the `anat` datatype. Note that `nifti-convert` currently only supports the BIDS `anat` datatype, which is sufficient for studies in QSM. Ensure you also fill the `MagneticFieldStrength` and `EchoTime` fields, which are necessary for QSM calculation. Once you have filled the spreadsheet, run the script again to complete the conversion.

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

