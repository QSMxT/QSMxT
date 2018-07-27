#!/usr/bin/env bash
# install dependencies e.g. or skip if already there
git clone git@gitlab.com:uqsbollm/deploy_containers.git
mv deploy_containers packageNAME

# setup dependencies, ideally in .bashrc:
export SINGULARITY_BINDPATH="/data"
# Container in /data/lfs2/software/singularity/tgvqsm_amd_20180727
export PATH=$PATH:/data/lfs2/software/singularity/tgvqsm_amd_20180727
# Container in /data/lfs2/software/singularity/fsl_5p0p11_20180712
export PATH=$PATH:/data/lfs2/software/singularity/fsl_5p0p11_20180712

#setup a miniconda python environemnt e.g.
wget https://repo.continuum.io/miniconda/Miniconda3-latest-Linux-x86_64.sh
bash Miniconda3-latest-Linux-x86_64.sh

#install nipype
conda install --channel conda-forge nipype

