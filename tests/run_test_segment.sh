#!/usr/bin/env bash

container=vnmd/qsmxt_1.1.1:20210608

docker pull $container

pip install osfclient


osf -p ru43c clone .
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d dicoms
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d dicoms
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d dicoms
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d dicoms

osf -p bt4ez fetch TOMCAT_DIB/sub-01/ses-01_7T/anat/sub-01_ses-01_7T_T1w_defaced.nii.gz

git clone https://github.com/QSMxT/QSMxT.git 

echo "[DEBUG] starting run_0_dicomSort.py"
docker run -v /tmp:/tmp $container python3 QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomToBids.py"
docker run -v /tmp:/tmp $container python3 QSMxT/run_1_dicomToBids.py /tmp/00_dicom /tmp/01_bids

sudo cp sub-01_ses-01_7T_T1w_defaced.nii.gz 01_bids/sub-170705134431std1312211075243167001/ses-1/anat/sub-170705134431std1312211075243167001_ses-1_T1w_run-1.nii.gz
sudo rm -rf 01_bids/sub-170706160506std1312211075243167001/ 

echo "[DEBUG] starting run_3_segment.py"
docker run -v /tmp:/tmp $container python3 QSMxT/run_3_segment.py /tmp/01_bids /tmp/03_segmentation


echo "[DEBUG] starting run_6_analysis.py"
python3 /opt/QSMxT/run_6_analysis.py --labels_file /opt/QSMxT/aseg_labels.csv --segmentations 03_segmentation/qsm_segmentation/*.nii --qsm_files 02_qsm_output/qsm_final/*.nii --out_dir 06_analysis


cd -

md5sum --check tests/test_hashes_segment.txt