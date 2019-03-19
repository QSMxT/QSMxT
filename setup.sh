#!/usr/bin/env bash
# install dependencies (fsl, tgv, ...) using transparent singularity or skip if already there
https://github.com/CAIsr/transparent-singularity


#setup a miniconda python environemnt e.g.
wget https://repo.continuum.io/miniconda/Miniconda3-latest-Linux-x86_64.sh
bash Miniconda3-latest-Linux-x86_64.sh

#install heudiconv
pip install https://github.com/nipy/heudiconv/archive/master.zip
pip install git+git://github.com/mvdoc/dcmstack@bf/importsys


#install nipype
conda install --channel conda-forge nipype
pip install pydot
