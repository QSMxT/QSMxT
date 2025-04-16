#!/usr/bin/env bash
set -e 

echo "[DEBUG] which dcm2niix"
which dcm2niix

echo "[DEBUG] Download test data"
pip install osfclient > /dev/null 2>&1
osf -p ru43c clone . > /dev/null 2>&1
tar xf osfstorage/dicoms-unsorted.tar -C .

echo "[DEBUG] dicom-convert dicoms-unsorted bids-transparent-apptainer --auto_yes"
dicom-convert dicoms-unsorted bids-transparent-apptainer --auto_yes

echo "[DEBUG] bids-transparent-apptainer --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug"
qsmxt bids-transparent-apptainer --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

rm -rf dicoms-unsorted/ bids-transparent-apptainer/ 

