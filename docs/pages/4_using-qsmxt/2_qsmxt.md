---
layout: default
title: Using QSMxT
nav_order: 2
parent: Using QSMxT
permalink: /using-qsmxt/qsmxt
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

# Example outputs

Given a BIDSs directory with various subjects, sessions, acquisitions and runs, the following is an example of the output from QSMxT.

This particular example includes outputs for QSM, R2\* maps, T2\* maps, segmentations and analyses.

```bash
qsm
├── command.txt
├── pypeline.log
├── qsmxt.log
├── references.txt
├── settings.json
├── qsm
│   ├── sub-1_ses-20231020_part-phase_T2Starw_romeo-unwrapped_normalized_vsharp_rts_ref.nii
│   ├── sub-2_ses-20231020_part-phase_T2Starw_romeo-unwrapped_normalized_vsharp_rts_ref.nii
│   ├── sub-2_ses-20231025_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_ref.nii
│   ├── sub-3_acq-mygre1_run-1_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_ref.nii
│   ├── sub-3_acq-mygre1_run-2_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_ref.nii
│   └── sub-3_acq-mygre2_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_ref.nii
├── r2s
│   ├── sub-2_ses-20231025_echo-1_part-mag_MEGRE_r2s.nii
│   ├── sub-3_acq-mygre1_run-1_echo-1_part-mag_MEGRE_r2s.nii
│   ├── sub-3_acq-mygre1_run-2_echo-1_part-mag_MEGRE_r2s.nii
│   └── sub-3_acq-mygre2_echo-1_part-mag_MEGRE_r2s.nii
├── t2s
│   ├── sub-2_ses-20231025_echo-1_part-mag_MEGRE_t2s.nii
│   ├── sub-3_acq-mygre1_run-1_echo-1_part-mag_MEGRE_t2s.nii
│   ├── sub-3_acq-mygre1_run-2_echo-1_part-mag_MEGRE_t2s.nii
│   └── sub-3_acq-mygre2_echo-1_part-mag_MEGRE_t2s.nii
├── segmentations
│   ├── qsm
│   │   ├── sub-1_ses-20231020_segmentation_trans.nii
│   │   ├── sub-2_ses-20231020_segmentation_trans.nii
│   │   ├── sub-2_ses-20231025_segmentation_trans.nii
│   │   ├── sub-3_acq-mygre1_run-1_segmentation_trans.nii
│   │   ├── sub-3_acq-mygre1_run-2_segmentation_trans.nii
│   │   └── sub-3_acq-mygre1_segmentation_trans.nii
│   └── t1w
│       ├── sub-1_ses-20231020_segmentation.nii
│       ├── sub-2_ses-20231020_segmentation.nii
│       ├── sub-2_ses-20231025_segmentation.nii
│       ├── sub-3_acq-mygre1_run-1_segmentation.nii
│       ├── sub-3_acq-mygre1_run-2_segmentation.nii
│       └── sub-3_acq-mygre1_segmentation.nii
└── analysis
    ├── sub-1_ses-20231020_part-phase_T2Starw_romeo-unwrapped_normalized_vsharp_rts_ref_csv.csv
    ├── sub-2_ses-20231020_part-phase_T2Starw_romeo-unwrapped_normalized_vsharp_rts_ref_csv.csv
    ├── sub-2_ses-20231025_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_ref_csv.csv
    ├── sub-3_acq-mygre1_run-1_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_csv.csv
    ├── sub-3_acq-mygre1_run-2_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_csv.csv
    └── sub-3_acq-mygre2_echo-1_part-phase_MEGRE_B0_normalized_vsharp_rts_csv.csv
```

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

