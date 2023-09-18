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

QSMxT is available via [Neurodesk](https://neurodesk.org/). Using [Neurodesk Play](https://play.neurodesk.org/), you can access QSMxT in your browser without any installation.

Once started, QSMxT is available in Neurodesk's module system and via the applications menu:

![Neurodesktop applications menu with QSMxT](/QSMxT/images/neurodesktop-applications-menu.jpg)

### Updating QSMxT in Neurodesk

To use the latest version of QSMxT within an older version of Neurodesk, use:

```
bash /neurocommand/local/fetch_and_run.sh qsmxt {{ site.SOFTWARE_VERSION }} {{ site.BUILD_DATE }}
```

## Docker container

If you prefer to use a Docker container, the following commands will install QSMxT locally:

**Windows:**
```
docker run -it -v C:/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_{{ site.SOFTWARE_VERSION }}:{{ site.BUILD_DATE }}
```

**Linux/Mac:**
```
docker run -it -v ~/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_{{ site.SOFTWARE_VERSION }}:{{ site.BUILD_DATE }}
```

## HPC installation

QSMxT can be installed on an HPC or Linux machine using [transparent singularity](https://github.com/neurodesk/transparent-singularity). Transparent singularity installs QSMxT using a singularity container and exposes the underlying tools to the host environment, which is necessary for HPCs using PBS Graph or SLURM. 

1. Install or load [singularity](https://sylabs.io/guides/3.0/user-guide/quick_start.html) on your HPC
2. Install the QSMxT container via [transparent singularity](https://github.com/neurodesk/transparent-singularity):
  {% capture code_block_content %}{% include transparent_singularity_install.sh %}{% endcapture %}
  ```bash
  {{ code_block_content | strip }}
  ```
  - **NOTE:** You must have sufficient storage available in `$SINGULARITY_TMPDIR` (by default `/tmp`), `$SINGULARITY_CACHEDIR` (by default `$HOME/.singularity/cache`), and the repository directory to store the QSMxT container.
3. Install miniconda with QSMxT:
  {% capture code_block_content %}{% include miniconda_install.sh %}{% endcapture %}
  ```bash
  {{ code_block_content | strip }}
  ```
4. Run QSMxT commands directly (see [using QSMxT](/QSMxT/using-qsmxt)). Use the `--pbs` and `--slurm` flags with your account string and group to run on an HPCs supporting PBS and SLURM.

## Bare metal installation

We recommend the use of software containers for reproducibility and ease of use. However, QSMxT can be installed manually. Please see the detailed instructions for generating the container [here](https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxt/build.sh).

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

