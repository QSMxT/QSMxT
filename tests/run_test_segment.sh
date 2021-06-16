#!/usr/bin/env bash
set -e

sudo docker pull $container

if ! command -v osfclient &> /dev/null
then
    echo "osfclient could not be found"
    sudo pip3 install osfclient
fi

if [[ ! -f /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip ]]
then
    echo "osfiles do not exist on your filesystem."
    osf -p ru43c clone /tmp
fi

if [[ ! -d /tmp/dicoms ]]
then
    echo "[DEBUG] unzipped dicoms do not exist"
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
fi

echo "[DEBUG] starting run_0_dicomSort.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomToBids.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_1_dicomToBids.py /tmp/00_dicom /tmp/01_bids

echo "[DEBUG] downloading anatomical files to test"
osf -p bt4ez fetch TOMCAT_DIB/sub-01/ses-01_7T/anat/sub-01_ses-01_7T_T1w_defaced.nii.gz /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz
sudo mv /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz /tmp/01_bids/sub-170705134431std1312211075243167001/ses-1/anat/sub-170705134431std1312211075243167001_ses-1_T1w_run-1_magnitude.nii.gz
sudo rm -rf 01_bids/sub-170706160506std1312211075243167001/ 

echo "[DEBUG] starting run_3_segment.py"
echo "[DEBUG] live patching fastsurfer interface to limit threads:"

sed -i 's/16/1/g' /tmp/QSMxT/interfaces/nipype_interface_fastsurfer.py

sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_3_segment.py /tmp/01_bids /tmp/03_segmentation

if [[ ! -d /tmp/02_qsm_output_precomputed ]]
then
    echo "[DEBUG] unzipped qsm outputs do not exist - unzipping them:"
    unzip /tmp/osfstorage/qsm_final.zip -d /tmp/02_qsm_output_precomputed
fi

echo "[DEBUG] starting run_5_analysis.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_5_analysis.py --labels_file /tmp/QSMxT/aseg_labels.csv --segmentations /tmp/03_segmentation/qsm_segmentations/*.nii --qsm_files /tmp/02_qsm_output_precomputed/qsm_final/*/*.nii --out_dir /tmp/06_analysis

[ -f  /tmp/03_segmentation/t1_segmentations/aparc.DKTatlas+aseg.deep_nii.nii ] && echo "$FILE exist." || exit 1
[ -f  /tmp/03_segmentation/qsm_segmentations/aparc.DKTatlas+aseg.deep_nii_trans.nii ] && echo "$FILE exist." || exit 1