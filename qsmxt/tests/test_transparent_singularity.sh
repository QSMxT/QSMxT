#!/usr/bin/env bash
set -e

# install apptainer
sudo apt-get install -y software-properties-common
sudo add-apt-repository -y ppa:apptainer/ppa
sudo apt-get update
sudo apt-get install -y apptainer

cp -r . /tmp/QSMxT

echo "[DEBUG]: Install QSMxT via transparent-singularity"
/tmp/QSMxT/docs/_includes/transparent_singularity_install.sh

echo "[DEBUG]: cd qsmxt_* && source activate_qsmxt_${SOFTWARE_VERSION}_${BUILD_DATE}.simg.sh && cd ../"
cd qsmxt_* && source activate_qsmxt_${SOFTWARE_VERSION}_${BUILD_DATE}.simg.sh && cd ../

echo "[DEBUG]: which tgv_qsm"
which tgv_qsm

echo "[DEBUG]: Install miniconda (excluding pip install qsmxt)"
/tmp/QSMxT/docs/_includes/miniconda_install.sh

echo "[DEBUG]: source ~/.bashrc"
source ~/.bashrc

echo "[DEBUG] Print active conda environment"
conda info --envs
#export PATH="~/miniconda3/envs/qsmxt/bin:${PATH}"

echo "[DEBUG]: conda init bash"
conda init bash
source ~/.bashrc

echo "[DEBUG]: conda activate qsmxt"
conda activate qsmxt

echo "[DEBUG]: which pip"
which pip

echo "[DEBUG]: which python"
which python

echo "[DEBUG]: Install QSMxT via pip linked installation"
pip uninstall qsmxt -y
pip install -e /tmp/QSMxT

echo "[DEBUG]: Download test data"
pip install osfclient > /dev/null 2>&1
osf -p ru43c clone /tmp > /dev/null 2>&1
tar xf /tmp/osfstorage/dicoms-unsorted.tar -C /tmp 

echo "[DEBUG] dicom-sort /tmp/dicoms-unsorted /tmp/dicoms-sorted"
dicom-sort /tmp/dicoms-unsorted /tmp/dicoms-sorted

echo "[DEBUG] dicom-convert /tmp/dicoms-sorted /tmp/bids --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'"
dicom-convert /tmp/dicoms-sorted /tmp/bids --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'

echo "[DEBUG] qsmxt /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug"
qsmxt /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

