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

This section will guide you through each of the steps needed to run QSMxT on a [converted](/QSMxT/using-qsmxt/data-preparation) <a href="https://bids.neuroimaging.io/" target="_blank" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Click to read about BIDS at https://bids.neuroimaging.io/.">BIDS</a> dataset. 

To begin, simply run:

```bash
qsmxt bids/ output_dir/
```

{: .new-title }
> Tip
>
> Most prompts will simply accept ENTER to use the default value.

{: .note }
QSMxT can also be run [non-interactively](#non-interactive-usage).

## Step 1: Select desired outputs

This page asks you to select which outputs you would like QSMxT to generate (space-separated). 

```
=== Desired outputs ===
 qsm: Quantitative Susceptibility Mapping (QSM)
 swi: Susceptibility Weighted Imaging (SWI)
 t2s: T2* maps
 r2s: R2* maps
 seg: Segmentations (requires qsm)
 analysis: QSM across segmented ROIs (requires qsm+seg)
 template: GRE group space + GRE/QSM templates (requires qsm)

Enter desired images (space-separated) [default - qsm]: 
```

If you need QSM and SWI, for example, you could enter `qsm swi`.

## Step 2: Select desired QSM pipeline

This page asks you to select which premade QSM pipeline you would like to use.

{: .note }
You can still change any specific [settings](#step-3-settings-menu) later.

{: .note }
Pipelines with *assumes human brain* apply masking techniques optimized for human brain imaging only and should not be applied in other regions. You may also adjust the masking algorithm in the [settings](#step-3-settings-menu).

```
=== Premade QSM pipelines ===
default: Default QSMxT settings (GRE)
gre: Applies suggested settings for 3D-GRE images
epi: Applies suggested settings for 3D-EPI images (assumes human brain)
bet: Applies a traditional BET-masking approach (artefact reduction unavailable; assumes human brain)
fast: Applies a set of fast algorithms
body: Applies suggested settings for non-brain applications
nextqsm: Applies suggested settings for running the NeXtQSM algorithm (assumes human brain)

Select a premade to begin [default - 'default']: 
```

## Step 3: Settings menu

This menu displays a summary of the chosen settings.

Enter an option from 1-4 to change any settings, or enter *run* to begin processing.

See the sections below for more details about the advanced settings in menus [3](#settings-for-qsm-masking) and [4](#settings-for-qsm-phase-processing).

```
=== QSMxT - Settings Menu ===

(1) Desired outputs:
 - Quantitative Susceptibility Mapping (QSM): Yes
 - Susceptibility Weighted Imaging (SWI): No
 - T2* mapping: No
 - R2* mapping: No
 - Segmentations: No
 - Analysis CSVs: No
 - GRE/QSM template space: No

(2) QSM pipeline: default

(3) [ADVANCED] QSM masking:
 - Use existing masks if available: No
 - Masking algorithm: threshold (phase-based)
   - Two-pass artefact reduction: Enabled
   - Threshold algorithm: otsu (x1.5 for single-pass; x1.3 for two-pass)
   - Hole-filling algorithm: morphological+gaussian
   - Erosions: 2 erosions for single-pass; 0 erosions for two-pass

(4) [ADVANCED] QSM phase processing:
 - Axial resampling: Enabled (obliquity threshold = 10)
 - Multi-echo combination: B0 mapping (using ROMEO)
 - Phase unwrapping: romeo
 - Background field removal: pdf
 - Dipole inversion: rts
 - Referencing: mean

Guidelines compliant! (see https://arxiv.org/abs/2307.02306)

Run command: qsmxt in out --auto_yes

Enter a number to customize; enter 'run' to run: 
```

### Settings for QSM masking

#### Use existing masks if available

QSMxT can use pre-existing masks if they are available and desired. Masks must be included in a valid BIDS derivatives directory. You will be prompted to enter the name of the software used to generate the mask, and this must match the derivatives folder name (e.g. `my-masking-software`), or match using a pattern. The default pattern `*` will match any folder to find a mask. If multiple masks are found, the one whose path is alphabetically first will be chosen.

An example of a valid BIDS directory with a mask is as follows:

```
bids/
├── derivatives
│   └── my-masking-software
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

#### Masking algorithm

There are two choices of masking algorithm:

```
threshold: 
     - required for the two-pass artefact reduction algorithm (https://doi.org/10.1002/mrm.29048)
     - required for applications other than in vivo human brain
     - more robust to severe pathology
bet: Applies the Brain Extraction Tool (standalone version)
     - the standard in most QSM pipelines
     - robust in healthy human brains
     - Paper: https://doi.org/10.1002/hbm.10062
     - Code: https://github.com/liangfu/bet2
```

##### Thresholding input

```
== Threshold input ==
Select the input to be used in the thresholding algorithm.

magnitude: use the MRI signal magnitude
  - standard approach
  - requires magnitude images
phase: use a phase quality map
  - phase quality map produced by ROMEO (https://doi.org/10.1002/mrm.28563)
  - measured between 0 and 100
  - some evidence that phase-based masks are more reliable near the brain boundary (https://doi.org/10.1002/mrm.29368)
```

##### Two-pass artifact reduction

```
== Two-pass Artefact Reduction ==
Select whether to use the two-pass artefact reduction algorithm (https://doi.org/10.1002/mrm.29048).

  - reduces artefacts, particularly near strong susceptibility sources
  - sometimes requires tweaking of the mask to maintain accuracy in high-susceptibility regions
  - single-pass results will still be included in the output
  - doubles the runtime of the pipeline
```

##### Threshold value

```
== Threshold value ==
Select an algorithm to automate threshold selection, or enter a custom threshold.

otsu: Automate threshold selection using the Otsu algorithm (https://doi.org/10.1109/TSMC.1979.4310076)
gaussian: Automate threshold selection using a Gaussian algorithm (https://doi.org/10.1016/j.compbiomed.2012.01.004)

Hardcoded threshold:
 - Use an integer to indicate an absolute signal intensity
 - Use a floating-point value from 0-1 to indicate a percentile of the per-echo signal histogram
 - Use two values to specify different thresholds for each pass in two-pass QSM
```

##### Threshold algorithm factors

```
== Threshold algorithm factors ==
The threshold algorithm can be tweaked by multiplying it by some factor.
Use two values to specify different factors for each pass in two-pass QSM
```

##### Filled mask algorithm

```
== Filled mask algorithm ==
Threshold-based masking requires an algorithm to create a filled mask.

gaussian:
 - applies the scipy gaussian_filter function to the threshold mask
 - may fill some unwanted regions (e.g. connecting skull to brain)
morphological:
 - applies the scipy binary_fill_holes function to the threshold mask
both:
 - applies both methods (gaussian followed by morphological) to the threshold mask
bet:
 - uses a BET mask as the filled mask
```

##### Include a BET mask in hole-filling

```
Include a BET mask in the hole-filling operation (yes or no) [default - no]: yes
```

##### BET fractional intensity

##### Erosions

```
== Erosions ==
The number of times to erode the mask.
Use two values to specify different erosion for each pass in two-pass QSM
```

### Settings for QSM phase processing

#### Axial resampling

#### Multi-echo combination

#### Phase unwrapping

#### Background field removal

#### Dipole inversion

#### Susceptibility referencing

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

# Non-interactive usage

If you wish to run QSMxT non-interactively, you may specify all settings via command-line arguments and run non-interactively via `--auto_yes`. For help with building the one-line command, start QSMxT interactively first. Before the pipeline runs, it will display the one-line command such as:

This example will run QSMxT non-interactively and produce QSM using the fast pipeline and segmentations.

```bash
qsmxt bids/ output_dir/ --do_qsm --premade fast --do_segmentations --auto_yes
```

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

