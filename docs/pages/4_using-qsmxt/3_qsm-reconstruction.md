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

# Running QSMxT

Run the following to start QSMxT and interactively choose your pipeline settings:

```bash
qsmxt bids/ output_dir/
```

By default, QSMxT runs interactively to make choosing pipeline settings straightforward. 

If you wish to run QSMxT non-interactively, you may specify all settings via command-line arguments and run non-interactively via `--auto_yes`. For help with building the one-line command, start QSMxT interactively first. Before the pipeline runs, it will display the one-line command such as:

```bash
qsmxt bids/ output_dir/ --do_qsm --premade fast --do_segmentations --auto_yes
```

This example will run QSMxT non-interactively and produce QSM using the fast pipeline and segmentations.

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

