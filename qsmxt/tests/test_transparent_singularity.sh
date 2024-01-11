#!/usr/bin/env bash
set -e

# install apptainer
sudo apt-get update
sudo apt-get install -y software-properties-common
sudo add-apt-repository -y ppa:apptainer/ppa
sudo apt-get update
sudo apt-get install -y apptainer

sudo rm -rf /tmp/QSMxT
cp -r . /tmp/QSMxT

echo "[DEBUG]: Install QSMxT via transparent-singularity"
export PROD_CONTAINER_VERSION=${TEST_CONTAINER_VERSION}
export PROD_CONTAINER_DATE=${TEST_CONTAINER_DATE}
sudo rm -rf qsmxt_*
/tmp/QSMxT/docs/_includes/transparent_singularity_install.sh

echo "[DEBUG]: cd qsmxt_* && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../"
cd qsmxt_* && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../

echo "[DEBUG]: which julia"
which julia

echo "[DEBUG]: remove executables we are replacing"
for f in {python3,python,qsmxt,dicom-sort,dicom-convert}; do
    sudo rm -rf qsmxt_*/${f}
done

echo "[DEBUG]: Install miniconda"
sudo rm -rf ~/miniconda3
/tmp/QSMxT/docs/_includes/miniconda_install.sh
export PATH="~/miniconda3/envs/qsmxt/bin:${PATH}"

echo "[DEBUG]: Install QSMxT via pip linked installation"
pip uninstall qsmxt -y
pip install -e /tmp/QSMxT

echo "[DEBUG]: Download test data"
pip install osfclient > /dev/null 2>&1
sudo rm -rf /tmp/osfstorage
osf -p ru43c clone /tmp > /dev/null 2>&1
sudo rm -rf /tmp/dicoms-unsorted
tar xf /tmp/osfstorage/dicoms-unsorted.tar -C /tmp 

echo "[DEBUG] dicom-sort /tmp/dicoms-unsorted /tmp/dicoms-sorted"
sudo rm -rf /tmp/dicoms-sorted
dicom-sort /tmp/dicoms-unsorted /tmp/dicoms-sorted

echo "[DEBUG] dicom-convert /tmp/dicoms-sorted /tmp/bids --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'"
sudo rm -rf /tmp/bids
dicom-convert /tmp/dicoms-sorted /tmp/bids --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'

echo "[DEBUG] qsmxt /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug"
sudo rm -rf /tmp/out
qsmxt /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

