---
layout: default
title: QSM Reconstruction
nav_order: 3
parent: Using QSMxT
permalink: /using-qsmxt/qsm-reconstruction
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# QSM Reconstruction

To run the QSM pipeline, data must first conform to the BIDS specification (see [data preparation](/using-qsmxt/data-preparation)). Each subject must have correctly combined T2\*-weighted phase images available.

Use the following command to initiate the QSM pipeline:

```bash
python3 /opt/QSMxT/run_2_qsm.py YOUR_BIDS_DIR qsm
```

For details about the underlying algorithm used in the reconstruction, see [QSMxT Algorithms](/algorithms#qsm-reconstruction).

## Parameter information

Several parameters can be customised for the reconstruction (see `python3 /opt/QSMxT/run_2_qsm.py --help`).

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

