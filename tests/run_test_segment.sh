#!/usr/bin/env bash
set -e
cp -r . /tmp/QSMxT
container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`

docker pull $container

pip install osfclient
osf -p ru43c clone /tmp
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms

echo "[DEBUG] starting run_0_dicomSort.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomConvert.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_1_dicomConvert.py /tmp/00_dicom /tmp/01_bids --auto_yes

if [[ ! -f /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz ]]
then
    echo "[DEBUG] downloading anatomical files to test"
    osf -p bt4ez fetch TOMCAT_DIB/sub-01/ses-01_7T/anat/sub-01_ses-01_7T_T1w_defaced.nii.gz /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz
    osf -p bt4ez fetch TOMCAT_DIB/sub-02/ses-01_7T/anat/sub-02_ses-01_7T_T1w_defaced.nii.gz /tmp/sub-02_ses-01_7T_T1w_defaced.nii.gz
fi
sudo cp /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz /tmp/01_bids/sub-170705134431std1312211075243167001/ses-1/anat/sub-170705134431std1312211075243167001_ses-1_run-01_T1w.nii.gz
sudo cp /tmp/sub-02_ses-01_7T_T1w_defaced.nii.gz /tmp/01_bids/sub-170706160506std1312211075243167001/ses-1/anat/sub-170706160506std1312211075243167001_ses-1_run-01_T1w.nii.gz


echo "[DEBUG] starting run_3_segment.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_3_segment.py /tmp/01_bids /tmp/03_segmentation --t1_pattern '{subject}/{session}/anat/*{run}*T1w*nii*'

echo "[DEBUG] checking output of run_3_segment.py"
[ -f  /tmp/03_segmentation/t1_segmentations/sub-170705134431std1312211075243167001_ses-1_run-01_T1w_segmentation_nii.nii ] && echo "sub-170705134431std1312211075243167001_ses-1_run-01_T1w_segmentation_nii.nii exists." || exit 1
[ -f  /tmp/03_segmentation/t1_segmentations/sub-170706160506std1312211075243167001_ses-1_run-01_T1w_segmentation_nii.nii ] && echo "sub-170706160506std1312211075243167001_ses-1_run-01_T1w_segmentation_nii.nii exists." || exit 1
[ -f  /tmp/03_segmentation/qsm_segmentations/sub-170705134431std1312211075243167001_ses-1_run-01_T1w_segmentation_nii_trans.nii ] && echo "sub-170705134431std1312211075243167001_ses-1_run-01_T1w_segmentation_nii_trans.nii exists." || exit 1
[ -f  /tmp/03_segmentation/qsm_segmentations/sub-170706160506std1312211075243167001_ses-1_run-01_T1w_segmentation_nii_trans.nii ] && echo "sub-170706160506std1312211075243167001_ses-1_run-01_T1w_segmentation_nii_trans.nii exists." || exit 1

if [[ ! -d /tmp/02_qsm_output_precomputed ]]
then
    echo "[DEBUG] unzipped qsm outputs do not exist - unzipping them:"
    unzip /tmp/osfstorage/qsm_final.zip -d /tmp/02_qsm_output_precomputed
fi

echo "[DEBUG] starting run_5_analysis.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_5_analysis.py --labels_file /tmp/QSMxT/aseg_labels.csv --segmentations /tmp/03_segmentation/qsm_segmentations/*.nii --qsm_files /tmp/02_qsm_output_precomputed/qsm_final/*.nii --output_dir /tmp/05_analysis

echo "[DEBUG] checking output of run_5_analysis.py"
[ -f  /tmp/05_analysis/sub-170705134431std1312211075243167001_ses-1_run-01_T1w_segmentation_nii_trans.csv ] && echo "FILE exists." || exit 1
[ -f  /tmp/05_analysis/sub-170706160506std1312211075243167001_ses-1_run-01_T1w_segmentation_nii_trans.csv ] && echo "FILE exists." || exit 1
