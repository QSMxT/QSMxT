#!/usr/bin/env bash
set -e

echo "[DEBUG] which dcm2niix"
which dcm2niix

# Run test in TEST_DIR where apptainer container has access
TEST_WORKDIR="${TEST_DIR}/test_workdir"
echo "[DEBUG] Creating test working directory: ${TEST_WORKDIR}"
mkdir -p "${TEST_WORKDIR}"
cd "${TEST_WORKDIR}"

echo "[DEBUG] Download test data"
pip install osfclient > /dev/null 2>&1
osf -p ru43c clone . > /dev/null 2>&1
tar xf osfstorage/dicoms-unsorted.tar -C .

echo "[DEBUG] dicom-convert dicoms-unsorted bids-transparent-apptainer --auto_yes"
dicom-convert dicoms-unsorted bids-transparent-apptainer --auto_yes

echo "[DEBUG] bids-transparent-apptainer --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug"
qsmxt bids-transparent-apptainer --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

echo "[DEBUG] Cleaning up test working directory: ${TEST_WORKDIR}"
cd /
rm -rf "${TEST_WORKDIR}" 

