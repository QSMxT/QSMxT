#!/usr/bin/env bash
set -e 

echo "[DEBUG] which dcm2niix"
which dcm2niix

echo "[DEBUG] Download test data"
pip install osfclient > /dev/null 2>&1
osf -p ru43c clone . > /dev/null 2>&1
tar xf osfstorage/dicoms-unsorted.tar -C .

echo "[DEBUG] dicom-sort dicoms-unsorted dicoms-sorted"
dicom-sort dicoms-unsorted dicoms-sorted

echo "[DEBUG] dicom-convert dicoms-sorted bids-transparent-singularity --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'"
dicom-convert dicoms-sorted bids-transparent-singularity --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'

echo "[DEBUG] bids-transparent-singularity --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug"
qsmxt bids-transparent-singularity --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

rm -rf dicoms-unsorted/ dicoms-sorted/ bids-transparent-singularity/ 

