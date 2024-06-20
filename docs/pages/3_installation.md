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
bash /neurocommand/local/fetch_and_run.sh qsmxt {{ site.PROD_CONTAINER_VERSION }} {{ site.PROD_CONTAINER_DATE }}
```

## Docker container

If you prefer to use a Docker container, the following commands will install QSMxT locally:

**Windows:**
```
docker run -it -v C:/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_{{ site.PROD_CONTAINER_VERSION }}:{{ site.PROD_CONTAINER_DATE }}
```

**Linux/Mac:**
```
docker run -it -v ~/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_{{ site.PROD_CONTAINER_VERSION }}:{{ site.PROD_CONTAINER_DATE }}
```

## HPC installation

QSMxT can be installed on an HPC or Linux machine using [transparent singularity](https://github.com/neurodesk/transparent-singularity). Transparent singularity installs QSMxT using an Apptainer container and exposes the underlying tools to the host environment, which is necessary for HPCs using PBS Graph or SLURM. 

1. Install or load Singularity or [Apptainer](https://apptainer.org/docs/user/1.0/quick_start.html#quick-start) on your HPC. Test if it works by executing 'singularity --version'.
2. Install the QSMxT container via [transparent singularity](https://github.com/neurodesk/transparent-singularity):
  {% capture code_block_content %}{% include transparent_singularity_install.sh %}{% endcapture %}
  {% assign code_block_content = code_block_content | replace: '${PROD_CONTAINER_VERSION}', site.PROD_CONTAINER_VERSION %}
  {% assign code_block_content = code_block_content | replace: '${PROD_CONTAINER_DATE}', site.PROD_CONTAINER_DATE %}
  ```bash
  {{ code_block_content | strip }}
  ```
  - **NOTE:** You must have sufficient storage available in `$APPTAINER_TMPDIR` (by default `/tmp`), `$APPTAINER_CACHEDIR` (by default `$HOME/.apptainer/cache`), and the repository directory to store the QSMxT container.
3. Check if you have conda installed using 'which conda'. If it is installed you can skip the first steps of this:
  {% capture code_block_content %}{% include miniconda_install.sh %}{% endcapture %}
  {% assign code_block_content = code_block_content | replace: '${PROD_PACKAGE_VERSION}', site.PROD_PACKAGE_VERSION %}
  {% assign code_block_content = code_block_content | replace: '${PROD_MINICONDA_PATH}', site.PROD_MINICONDA_PATH %}
  ```bash
  {{ code_block_content | strip }}
  ```
4. Run QSMxT commands directly (see [using QSMxT](/QSMxT/using-qsmxt)). Use the `--pbs` and `--slurm` flags with your account string and group to run on an HPCs supporting PBS and SLURM.

## Bare metal installation

We recommend the use of software containers for reproducibility and ease of use. However, QSMxT can be installed manually. Please see the detailed instructions for generating the container [here](https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxt/build.sh).

## Older versions and release notes

You can find the software versions and container dates on our Docker page [here](https://hub.docker.com/search?q=qsmxt&sort=updated_at&order=desc). 

You can find detailed release notes on the GitHub releases page [here](https://github.com/QSMxT/QSMxT/releases).

<script>
$(document).ready(function(){
    $('[data-toggle="popover"]').popover();   
});
$("[data-toggle=popover]")
.popover({html:true})
</script>

