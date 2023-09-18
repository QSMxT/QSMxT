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

echo "[DEBUG]: Sourcing ~/.bashrc to update paths after transparent-singularity install"
source ~/.bashrc

echo "[DEBUG]: Download test data"
pip install osfclient > /dev/null 2>&1
osf -p ru43c clone /tmp > /dev/null 2>&1
tar xf /tmp/osfstorage/dicoms-unsorted.tar -C /tmp 

echo "[DEBUG]: Install miniconda"
/tmp/QSMxT/docs/_includes/miniconda_install.sh

echo "[DEBUG] dicom-sort /tmp/dicoms-unsorted /tmp/dicoms-sorted"
dicom-sort /tmp/dicoms-unsorted /tmp/dicoms-sorted

echo "[DEBUG] dicom-convert /tmp/dicoms-sorted /tmp/bids --auto_yes --t2starw_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'"
dicom-convert /tmp/dicoms-sorted /tmp/bids --auto_yes --t2starw_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'

echo "[DEBUG] qsmxt /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug"
qsmxt /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

