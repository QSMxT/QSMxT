---
layout: default
title: Installation
nav_order: 3
permalink: /installation
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# Installation

## Quick-start via Neurodesk (Windows, MacOS and Linux)

QSMxT is bundled with <a href="https://neurodesk.org/" target="_blank" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="An interactive analysis environment for Neuroimaging. Click to navigate.">Neurodesk</a>, which runs on Windows, MacOS, and Linux. We recommend this method for most users. 

Once Neurodesktop is installed an open, QSMxT can be accessed through the Applications menu:

![Neurodesktop applications menu with QSMxT](/images/neurodesktop-applications-menu.jpg)

## Docker container (Windows, MacOS and Linux)

You can run QSMxT via a Docker container, which is also compatible with Windows, MacOS and Linux. You will first need to install Docker, before running the following command:

### Windows users

 ```bash
docker run -it -v ~/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_1.1.11:20220526
 ```

### MacOS and Linux users

 ```bash
docker run -it -v C:/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_1.1.11:20220526
 ```

## Transparent Singularity (Linux and HPCs)

An alternative installation for Linux users that also supports HPCs is described in this section.

A singularity container is provided for Linux and HPC use, which coupled with the transparent singularity software provided by the Neurodesk project, allows QSMxT and its dependencies to be invoked <a href="https://neurodesk.org/" target="_blank" data-placement="top" data-toggle="popover" data-trigger="hover focus" data-content="Outside of the container's environment; as though QSMxT and its dependencies were installed natively.">transparently</a>. This mode of execution is necessary for parallel execution via PBS.

1. Install singularity

2. Install the QSMxT container via transparent singularity:

  ```bash
  git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_1.1.11_20220526
  cd qsmxt_1.1.11_20220526
  ./run_transparent_singularity.sh --container qsmxt_1.1.11_20220526.simg
  source activate_qsmxt_1.1.11_20220526.simg.sh
  ```

3. Clone the QSMxT repository:

  ```bash
  git clone https://github.com/QSMxT/QSMxT.git
  ```

4. Install miniconda with nipype:

  ```bash
  wget https://repo.anaconda.com/miniconda/Miniconda3-4.7.12.1-Linux-x86_64.sh	
  bash Miniconda3-4.7.12.1-Linux-x86_64.sh -b
  source ~/.bashrc
  conda create -n qsmxt python=3.6
  conda activate qsmxt
  conda install -c conda-forge nipype=1.6.0 scipy=1.8.0
  pip install bidscoin
  ```

5. Invoke QSMxT python scripts directly (see [QSMxT Usage](/using-qsmxt)). Use the `--pbs` flag with your account string to run on an HPC supporting PBS.


## Bare metal installation

We do not recommend installing QSMxT's dependencies manually, and we instead advocate for the use of software containers for reproducibility and ease-of-use. However, we provide a list of the dependencies below if you wish to run QSMxT natively. This was tested in Ubuntu 18.04.

You need:

- TGV-QSM running in miniconda 2
- fsl version=6.0.4
- ants version=2.3.4
- dcm2niix latest version from github
- miniconda version=4.7.12.1 with python 3.6 for nipype 1.6.0 pytorch 1.2.0 and torchvision 0.4.0 niflow-nipype1-workflows
- FastSurfer https://github.com/Deep-MI/FastSurfer.git
- Bru2Nii v1.0.20180303 https://github.com/neurolabusc/Bru2Nii/releases/download/v1.0.20180303/Bru2_Linux.zip
- julia-1.6.1 with ArgParse and MriResearchTools

Here is the detailed instruction that you could replicate: https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxtbase/build.sh and then on top https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxt/build.sh

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

