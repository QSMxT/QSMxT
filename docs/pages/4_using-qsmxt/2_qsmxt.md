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
qsmxt YOUR_BIDS_DIRECTORY/
```

Then, follow the steps outlined below.

{: .note }
Most prompts will simply accept ENTER to use the default value.

{: .note }
QSMxT can also be run [non-interactively](#non-interactive-usage).

## Step 1: Select desired outputs

The first page will prompt you to select which outputs to generate (space-separated). 

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

If you need QSM and SWI, for example, you would enter `qsm swi`.

## Step 2: Select desired QSM pipeline

This page asks you to select which premade QSM pipeline you would like to start with.

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

{: .note }
You can still change any specific [settings](#step-3-settings-menu) later.

{: .note }
Pipelines with *assumes human brain* apply masking techniques optimized for human brain imaging only and should not be applied in other regions. You may also adjust the masking algorithm in the [settings](#step-3-settings-menu).

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

Run command: qsmxt bids/ --auto_yes

Enter a number to customize; enter 'run' to run: 
```

## Settings for QSM masking

### Use existing masks if available

QSMxT can use pre-existing masks if they are available and desired (`--use_existing_masks`). Masks must be included in a valid BIDS derivatives directory. You will be prompted to enter the name of the software used to generate the mask, and this must match the derivatives folder name (e.g. `my-masking-software`), or match using a pattern. The default pattern `*` will match any folder to find a mask. If multiple masks are found, the one whose path is alphabetically first will be chosen.

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

### Masking algorithm

There are two choices of masking algorithm:

- **Threshold-based** (`--masking_algorithm threshold`)
  - required for the two-pass artefact reduction algorithm ([https://doi.org/10.1002/mrm.29048](https://doi.org/10.1002/mrm.29048))
  - required for applications other than in vivo human brain
  - more robust to severe pathology
- **BET (brain extraction tool)** (`--masking_algorithm bet`)
  - the standard in most QSM pipelines
  - robust in healthy human brains
  - paper: [https://doi.org/10.1002/hbm.10062](https://doi.org/10.1002/hbm.10062)
  - code: [https://github.com/liangfu/bet2](https://github.com/liangfu/bet2)

### Thresholding input

For the threshold-based [masking algorithm](#masking-algorithm), a mask can be generated from two possible inputs:

- **Magnitude** - use the MRI signal magnitude (`--masking_input magnitude`)
  - standard approach
  - requires magnitude images
- **Phase** - use a phase quality map (`--masking_input phase`)
  - phase quality map produced by ROMEO ([https://doi.org/10.1002/mrm.28563](https://doi.org/10.1002/mrm.28563))
  - measured between 0 and 100
  - some evidence that phase-based masks are more reliable near the brain boundary ([https://doi.org/10.1002/mrm.29368](https://doi.org/10.1002/mrm.29368))

### Two-pass artifact reduction

For the threshold-based [masking algorithm](#masking-algorithm), a two-pass artifact-reduction technique can be applied (`--two_pass on`; [https://doi.org/10.1002/mrm.29048](https://doi.org/10.1002/mrm.29048)).

The two-pass algorithm performs two QSM reconstructions - one using a threshold-based mask (with holes in high-susceptibility regions), and another using the same mask after applying a hole-filling algorithm. The final susceptibility map is a superposition of both results - the susceptibility map with holes is filled using values from the filled susceptibility map.

The two-pass algorithm:
- reduces artefacts, particularly near strong susceptibility sources
- sometimes requires tweaking of the mask to maintain accuracy in high-susceptibility regions
- single-pass results will still be included in the output
- doubles the runtime of the pipeline

### Threshold selection

For the threshold-based [masking algorithm](#masking-algorithm), a threshold must be determined.

A threshold can be hardcoded:
 - Use an integer to indicate an absolute signal intensity (e.g. `--threshold_value 20`)
 - Use a floating-point value from 0-1 to indicate a percentile of the per-echo signal histogram (e.g. `--threshold_value 0.3`)
 - Use two values to specify different thresholds for each pass in [two-pass QSM](#two-pass-artifact-reduction) (e.g. `--threshold_value 50 40`); the first value is used for the filled pass, and the second value is used for the mask with holes

A threshold may also be determined automatically using one of two algorithms:
  - **otsu**: Automate threshold selection using the Otsu algorithm ([https://doi.org/10.1109/TSMC.1979.4310076](https://doi.org/10.1109/TSMC.1979.4310076)) (`--threshold_algorithm otsu`)
  - **gaussian**: Automate threshold selection using a Gaussian algorithm ([https://doi.org/10.1016/j.compbiomed.2012.01.004](https://doi.org/10.1016/j.compbiomed.2012.01.004)) (`--threshold_algorithm gaussian`)

### Threshold algorithm factors

For the threshold-based [masking algorithm](#masking-algorithm) with [threshold values](#threshold-value) chosen using an algorithm, the thresholds can be multiplied by some factor. If [two-pass QSM](#two-pass-artifact-reduction) is enabled, two factors may be given - one for the filled susceptibility map and another for the unfilled map (e.g. `--threshold_algorithm_factor 1.5 1.3`).

### Hole-filling algorithm

For the threshold-based [masking algorithm](#masking-algorithm), holes can be filled using one of several options:

- **gaussian** (`--filling_algorithm gaussian`):
  - applies the scipy gaussian_filter function to the threshold mask
  - may fill some unwanted regions (e.g. connecting skull to brain)
- **morphological** (`--filling_algorithm morphological`):
  - applies the scipy binary_fill_holes function to the threshold mask
- **both** (`--filling_algorithm both`):
  - applies gaussian followed by morphological hole-filling to the threshold mask
- **bet** (`--filling_algorithm bet`):
  - uses a BET-derived mask as the filled mask

### Include a BET mask in hole-filling

For the threshold-based [masking algorithm](#masking-algorithm) using gaussian and/or morphological [hole-filling](#hole-filling-algorithm), a BET mask can also be generated and combined to ensure stubborn regions are filled (`--add_bet`).

### BET fractional intensity

The BET fractional intensity parameter can be customized (e.g. `--bet_fractional_intensity 0.5`).

### Erosions

The number of erosions applied to masks can be adjusted. Two values may be given if [two-pass QSM](#two-pass-artifact-reduction) is enabled (e.g. `--mask_erosions 3 0`). If two values are given, the first is for the filled mask and the second is for the unfilled mask.

## Settings for QSM phase processing

### Axial resampling

Most QSM algorithms require the slice direction to align with the magnetic field direction for optimal accuracy. Oblique acquisitions can be rotated and resampled to a true axial orientation to ensure this assumption holds (see https://doi.org/10.1002/mrm.29550).

QSMxT can perform this axial resampling when the measured obliquity is beyond a chosen threshold. Obliquity is measured using [nibabel](https://nipy.org/nibabel/reference/nibabel.affines.html#nibabel.affines.obliquity), which uses a measure derived from [AFNI's definition](https://github.com/afni/afni/blob/b6a9f7a21c1f3231ff09efbd861f8975ad48e525/src/thd_coords.c#L660-L698). If the measured obliquity is beyond the threshold, the volume will be resampled to axial (e.g. `--obliquity_threshold 10`). To disable axial resampling, set the threshold to -1.

### Multi-echo combination

Multi-echo acquisitions require a combination step. Data can be combined prior to QSM calculation by generating a field map using [ROMEO](https://doi.org/10.1002/mrm.28563) (`--combine_phase true`), or susceptibility maps can be averaged (`--combine_phase false`). 

### QSM Algorithm

Multiple QSM algorithms are available in QSMxT (e.g. `--qsm_algorithm rts`). Some QSM algorithms solve only the final dipole inversion step, while others include phase unwrapping and background field correction.

- **rts**: Rapid Two-Step QSM
  - [https://doi.org/10.1016/j.neuroimage.2017.11.018](https://doi.org/10.1016/j.neuroimage.2017.11.018)
  - Compatible with two-pass artefact reduction algorithm
  - Fast runtime
- **tv**: Fast quantitative susceptibility mapping with L1-regularization and automatic parameter selection
  - [https://doi.org/10.1002/mrm.25029](https://doi.org/10.1002/mrm.25029)
  - Compatible with two-pass artefact reduction algorithm
- **tgv**: Total Generalized Variation
  - [https://doi.org/10.1016/j.neuroimage.2015.02.041](https://doi.org/10.1016/j.neuroimage.2015.02.041)
  - Combined unwrapping, background field removal and dipole inversion
  - Compatible with two-pass artefact reduction algorithm
- **nextqsm**: NeXtQSM
  - [https://doi.org/10.1016/j.media.2022.102700](https://doi.org/10.1016/j.media.2022.102700)
  - Uses deep learning to solve the background field removal and dipole inversion steps
  - High memory requirements (>=12gb recommended)

### Phase unwrapping

Multiple phase unwraping algorithms are available in QSMxT (e.g. `--unwrapping_algorithm romeo`).

{: .note }
Laplacian phase unwrapping is forced to ROMEO if the multi-echo [`--combine_phase`](#multi-echo-combination) is enabled. This is because the phase combination technique is based on the field map derived from ROMEO.

- **romeo**: ([https://doi.org/10.1002/mrm.28563](https://doi.org/10.1002/mrm.28563))
  - quantitative
- **laplacian**: ([https://doi.org/10.1364/OL.28.001194](https://doi.org/10.1364/OL.28.001194); [https://doi.org/10.1002/nbm.3064](https://doi.org/10.1002/nbm.3064))
  - non-quantitative
  - popular for its numerical simplicity

### Background field removal

Multiple background field removal algorithms are available in QSMxT (e.g. `--bf_algorithm vsharp`).

- **vsharp**: V-SHARP algorithm ([https://doi.org/10.1002/mrm.23000](https://doi.org/10.1002/mrm.23000))
  - fast
  - involves a mask erosion step that impacts the next steps
  - less reliable with threshold-based masks
  - not compatible with artefact reduction algorithm
- **pdf**: Projection onto Dipole Fields algorithm ([https://doi.org/10.1002/nbm.1670](https://doi.org/10.1002/nbm.1670))
  - slower
  - more accurate
  - does not require an additional erosion step

### Susceptibility referencing

QSM is only able to estimate susceptibility in reference to some value. It is standard practice to choose a region whose mean value will serve as the reference susceptibility. By default, QSMxT uses the average susceptibility value of the whole volume, ignoring zero-values (`--qsm_reference mean`).

Alternatively, if segmentations are included in the [desired outputs](#step-1-select-desired-outputs), you can also select a set of segmentation IDs from which the average susceptibility will serve as the reference (e.g. `--qsm_reference 3 13 25`; see the [full list](https://github.com/QSMxT/QSMxT/blob/main/qsmxt/aseg_labels.csv) of possible labels).

# Example outputs

Given a BIDSs directory with various subjects, sessions, acquisitions and runs, the following is an example of the output from QSMxT, which is integrated within the existing BIDS directory as [BIDS derivatives](https://bids-specification.readthedocs.io/en/stable/derivatives/introduction.html).

This particular example includes QSM, R2\* maps, T2\* maps, segmentations and analyses:

```bash
bids/derivatives/qsmxt-2024-08-05-144311/
├── command.txt
├── pypeline.log
├── qsmxt.log
├── references.txt
├── settings.json
└── sub-1
    ├── anat
    │   ├── sub-1_Chimap.nii
    │   ├── sub-1_minIP.nii
    │   ├── sub-1_R2starmap.nii
    │   ├── sub-1_space-orig_dseg.nii
    │   ├── sub-1_space-qsm_dseg.nii
    │   ├── sub-1_swi.nii
    │   └── sub-1_T2starmap.nii
    └── extra_data
        ├── sub-1_desc-t1w-to-qsm_transform.mat
        └── sub-1_qsm-analysis
            ├── sub-1_desc-qsm-forward_Chimap_sub-1_space-qsm_desc-qsmxt-2024-08-05-144311_dseg_analysis.csv
            └── sub-1_desc-qsmxt-2024-08-05-144311_Chimap_sub-1_space-qsm_desc-qsmxt-2024-08-05-144311_dseg_analysis.csv
```

# Non-interactive usage

If you wish to run QSMxT non-interactively, you may specify all settings via command-line arguments and run non-interactively via `--auto_yes`. For help with building the one-line command, start QSMxT interactively first. Before the pipeline runs, it will display the one-line command such as:

This example will run QSMxT non-interactively and produce QSM using the fast pipeline and segmentations.

```bash
qsmxt bids/ --do_qsm --premade fast --do_segmentations --auto_yes
```

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

