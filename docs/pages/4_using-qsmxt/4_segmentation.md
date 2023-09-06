---
layout: default
title: T1/QSM Segmentation
nav_order: 4
parent: Using QSMxT
permalink: /using-qsmxt/segmentation
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# T1/QSM Segmentation

## Why use this pipeline?

This pipeline is useful if:

 - You have T1-weighted images and QSM results available
 - You wish to automatically segment your T1-weighted and QSM images
 - You wish to perform a quantitative analysis of your QSM results in anatomical regions of interest

## What does this pipeline do?

This pipeline will segment T1-weighted images before registering them to the T2\*-weighted space (the same space as QSM results). Segmentation is performed using FastSurfer with CPU processing. Registration is performed using ANTs via the `antsRegistrationSyNQuick.sh` script.

## Running the pipeline

Use the following command to initiate the segmentation pipeline (replacing `YOUR_BIDS_DIR` with your BIDS directory, `YOUR_QSM_DIR` with the top-level QSM output directory produced by QSMxT, and `segmentations` with your preferred output directory for segmentation results):

```bash
python3 /opt/QSMxT/run_3_segment.py YOUR_BIDS_DIR YOUR_QSM_DIR segmentations
```

## Parameter information

Several parameters can be customised for the segmentation (see `python3 /opt/QSMxT/run_3_segment.py --help`).

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

