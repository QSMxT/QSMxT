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

## Quickstart via Neurodesk

QSMxT can be accessed via [Neurodesk](https://neurodesk.org/), including for free, without any installation via [Neurodesk Play](https://play.neurodesk.org/). Once started, QSMxT is available in Neurodesk's module system and via the applications menu.

![Neurodesktop applications menu with QSMxT](/docs/images/neurodesktop-applications-menu.jpg)

### Updating QSMxT in Neurodesk

To use the latest version of QSMxT within an older version of Neurodesk, use:

```
bash /neurocommand/local/fetch_and_run.sh qsmxt {{ site.software_version }} {{ site.build_date }}
```

## Docker container

If you prefer to use a Docker container, the following commands will install QSMxT locally:

**Windows:**
```
docker run -it -v C:/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_{{ site.software_version }}:{{ site.build_date }}
```

**Linux/Mac:**
```
docker run -it -v ~/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_{{ site.software_version }}:{{ site.build_date }}
```

## HPC installation via Transparent Singularity

The tools provided by the QSMxT container can be exposed and used using the QSMxT Singularity container coupled with the transparent singularity software provided by the Neurodesk project. Transparent singularity allows the QSMxT Python scripts to be run directly within the host OS's environment. This mode of execution is necessary for parallel execution via PBS.

1. Install [singularity](https://sylabs.io/guides/3.0/user-guide/quick_start.html)
   
2. Install the QSMxT container via [transparent singularity](https://github.com/neurodesk/transparent-singularity):

    ```bash
    git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_{{ site.software_version }}_{{ site.build_date }}
    cd qsmxt_{{ site.software_version }}_{{ site.build_date }}
    ./run_transparent_singularity.sh --container qsmxt_{{ site.software_version }}_{{ site.build_date }}.simg
    source activate_qsmxt_{{ site.software_version }}_{{ site.build_date }}.simg.sh
    ```
    
    - **NOTE:** You must have sufficient storage available in `$SINGULARITY_TMPDIR` (by default `/tmp`), `$SINGULARITY_CACHEDIR` (by default `$HOME/.singularity/cache`), and the repository directory to store the QSMxT container.

3. Clone the QSMxT repository:
    ```bash
    git clone https://github.com/QSMxT/QSMxT.git
    ```

4. Install miniconda with QSMxT:
```bash
wget https://repo.anaconda.com/miniconda/Miniconda3-4.7.12.1-Linux-x86_64.sh	
bash Miniconda3-4.7.12.1-Linux-x86_64.sh -b
source ~/.bashrc
conda create -n qsmxt python=3.8
conda activate qsmxt
pip install qsmxt
```

5. Invoke QSMxT python commands directly (see QSMxT Usage above). Use the `--pbs` and `--slurm` flags with your account string and group to run on an HPCs supporting PBS and SLURM.

## Bare metal installation

We recommend the use of software containers for reproducibility and ease of use. However, QSMxT can be installed manually. Please see the detailed instructions for generating the container [here](https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxt/build.sh).

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

