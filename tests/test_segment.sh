#!/usr/bin/env bash
set -e

sudo apt update
sudo apt install unzip -y

#  extract container version from README:
container=`cat README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`

sudo docker pull $container

if ! command -v osf &> /dev/null
then
    echo "[DEBUG] osfclient could not be found. Installing ..."
    sudo pip3 install osfclient
fi

if [[ ! -f osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip ]]
then
    echo "[DEBUG] osf files do not exist on filesystem. Downloading..."
    osf -p ru43c clone .
fi

if [[ ! -d dicoms ]]
then
    echo "[DEBUG] unzipped dicoms do not exist. Unzipping"
    unzip osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d dicoms
    unzip osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d dicoms
    unzip osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d dicoms
    unzip osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d dicoms
fi

echo "[DEBUG] starting run_0_dicomSort.py"
sudo docker run -v `pwd`:/tmp $container python3 /tmp/run_0_dicomSort.py /tmp/dicoms /tmp/dicoms-sorted

echo "[DEBUG] starting run_1_dicomConvert.py"
sudo docker run -v `pwd`:/tmp $container python3 /tmp/run_1_dicomConvert.py /tmp/dicoms-sorted /tmp/bids --auto_yes

if [[ ! -f sub-01_ses-01_7T_T1w_defaced.nii.gz ]]
then
    echo "[DEBUG] downloading anatomical files to test"
    osf -p bt4ez fetch TOMCAT_DIB/sub-01/ses-01_7T/anat/sub-01_ses-01_7T_T1w_defaced.nii.gz sub-01_ses-01_7T_T1w_defaced.nii.gz
    osf -p bt4ez fetch TOMCAT_DIB/sub-02/ses-01_7T/anat/sub-02_ses-01_7T_T1w_defaced.nii.gz sub-02_ses-01_7T_T1w_defaced.nii.gz
fi
sudo cp sub-01_ses-01_7T_T1w_defaced.nii.gz bids/sub-170705134431STD1312211075243167001/ses-1/anat/sub-170705134431STD1312211075243167001-1_run-01_T1w.nii.gz
sudo cp sub-02_ses-01_7T_T1w_defaced.nii.gz bids/sub-170706160506STD1312211075243167001/ses-1/anat/sub-170706160506STD1312211075243167001-1_run-01_T1w.nii.gz


echo "[DEBUG] starting run_3_segment.py"
sudo docker run -v `pwd`:/tmp $container python3 /tmp/run_3_segment.py /tmp/bids /tmp/segmentation --t1_pattern '{subject}/{session}/anat/*{run}*T1w*nii*' --n_procs 2

echo "[DEBUG] checking output of run_3_segment.py"
[ -f  segmentation/t1_segmentations/sub-170705134431STD1312211075243167001-1_run-01_T1w_segmentation_nii.nii ] && echo "sub-170705134431STD1312211075243167001-1_run-01_T1w_segmentation_nii.nii exists." || exit 1
[ -f  segmentation/t1_segmentations/sub-170706160506STD1312211075243167001-1_run-01_T1w_segmentation_nii.nii ] && echo "sub-170706160506STD1312211075243167001-1_run-01_T1w_segmentation_nii.nii exists." || exit 1
[ -f  segmentation/qsm_segmentations/sub-170705134431STD1312211075243167001-1_run-01_T1w_segmentation_nii_trans.nii ] && echo "sub-170705134431STD1312211075243167001-1_run-01_T1w_segmentation_nii_trans.nii exists." || exit 1
[ -f  segmentation/qsm_segmentations/sub-170706160506STD1312211075243167001-1_run-01_T1w_segmentation_nii_trans.nii ] && echo "sub-170706160506STD1312211075243167001-1_run-01_T1w_segmentation_nii_trans.nii exists." || exit 1

if [[ ! -d qsm_precomputed ]]
then
    echo "[DEBUG] unzipped qsm outputs do not exist - unzipping them:"
    unzip osfstorage/qsm_final.zip -d qsm_precomputed
fi

echo "[DEBUG] starting run_5_analysis.py"
sudo docker run -v `pwd`:/tmp $container python3 /tmp/run_5_analysis.py --labels_file /tmp/aseg_labels.csv --segmentations /tmp/segmentation/qsm_segmentations/*.nii --qsm_files /tmp/qsm_precomputed/qsm_final/*/*.nii* --output_dir analysis

echo "[DEBUG] checking output of run_5_analysis.py"
[ -f  analysis/sub-170705134431STD1312211075243167001-1_run-01_T1w_segmentation_nii_trans.csv ] && echo "FILE exists." || exit 1
[ -f  analysis/sub-170706160506STD1312211075243167001-1_run-01_T1w_segmentation_nii_trans.csv ] && echo "FILE exists." || exit 1

