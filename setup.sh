#!/usr/bin/env bash
# install dependencies (fsl, tgv, ...) using transparent singularity or skip if already there
# follow instructions here https://github.com/CAIsr/transparent-singularity or try this:
git clone https://github.com/CAIsr/transparent-singularity.git tgvqsm_fsl_5p0p11_intel_20180730.simg
cd tgvqsm_fsl_5p0p11_intel_20180730.simg
./run_transparent_singularity.sh tgvqsm_fsl_5p0p11_intel_20180730.simg


#setup a miniconda python environemnt e.g.
wget https://repo.continuum.io/miniconda/Miniconda3-latest-Linux-x86_64.sh
bash Miniconda3-latest-Linux-x86_64.sh

#logout or source .bashrc
source ~/.bashrc

#install heudiconv
pip install https://github.com/nipy/heudiconv/archive/master.zip
pip install git+git://github.com/mvdoc/dcmstack@bf/importsys


#install nipype
conda install --channel conda-forge nipype
pip install pydot
