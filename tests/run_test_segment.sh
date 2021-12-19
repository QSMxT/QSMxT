#!/usr/bin/env bash
set -e

# timeStamp=`date +"%Y-%m-%d-%T"`
# timeStamp="2021-06-18-03:53:58"
# echo ${timeStamp}
# git clone https://github.com/QSMxT/QSMxT.git /tmp/${timeStamp}/QSMxT

#  extract container version from README:
container=`cat /tmp/${timeStamp}/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`

sudo docker pull $container

if ! command -v osf &> /dev/null
then
    echo "[DEBUG] osfclient could not be found. Installing ..."
    sudo pip3 install osfclient
fi

if [[ ! -f /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip ]]
then
    echo "[DEBUG] osf files do not exist on filesystem. Downloading..."
    osf -p ru43c clone /tmp
fi

if [[ ! -d /tmp/dicoms ]]
then
    echo "[DEBUG] unzipped dicoms do not exist. Unzipping"
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
    unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
fi

echo "[DEBUG] starting run_0_dicomSort.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/${timeStamp}/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/${timeStamp}/00_dicom

echo "[DEBUG] starting run_1_dicomConvert.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/${timeStamp}/QSMxT/run_1_dicomConvert.py /tmp/${timeStamp}/00_dicom /tmp/${timeStamp}/01_bids --auto_yes

if [[ ! -f /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz ]]
then
    echo "[DEBUG] downloading anatomical files to test"
    osf -p bt4ez fetch TOMCAT_DIB/sub-01/ses-01_7T/anat/sub-01_ses-01_7T_T1w_defaced.nii.gz /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz
    osf -p bt4ez fetch TOMCAT_DIB/sub-02/ses-01_7T/anat/sub-02_ses-01_7T_T1w_defaced.nii.gz /tmp/sub-02_ses-01_7T_T1w_defaced.nii.gz
fi
sudo cp /tmp/sub-01_ses-01_7T_T1w_defaced.nii.gz /tmp/${timeStamp}/01_bids/sub-170705-134431-std-1312211075243167001/ses-1/anat/sub-170705134431std1312211075243167001_ses-1_T1w_run-1_magnitude.nii.gz
sudo cp /tmp/sub-02_ses-01_7T_T1w_defaced.nii.gz /tmp/${timeStamp}/01_bids/sub-170706-160506-std-1312211075243167001/ses-1/anat/sub-170706160506std1312211075243167001_ses-1_T1w_run-1_magnitude.nii.gz


echo "[DEBUG] starting run_3_segment.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/${timeStamp}/QSMxT/run_3_segment.py /tmp/${timeStamp}/01_bids /tmp/${timeStamp}/03_segmentation

[ -f  /tmp/${timeStamp}/03_segmentation/t1_segmentations/sub-170705-134431-std-1312211075243167001_ses-1_run-1_T1w_segmentation_nii.nii ] && echo "FILE exists." || exit 1
[ -f  /tmp/${timeStamp}/03_segmentation/t1_segmentations/sub-170706-160506-std-1312211075243167001_ses-1_run-1_T1w_segmentation_nii.nii ] && echo "FILE exists." || exit 1
[ -f  /tmp/${timeStamp}/03_segmentation/qsm_segmentations/sub-170705-134431-std-1312211075243167001_ses-1_run-1_T1w_segmentation_nii_trans.nii ] && echo "FILE exists." || exit 1
[ -f  /tmp/${timeStamp}/03_segmentation/qsm_segmentations/sub-170706-160506-std-1312211075243167001_ses-1_run-1_T1w_segmentation_nii_trans.nii ] && echo "FILE exists." || exit 1

if [[ ! -d /tmp/02_qsm_output_precomputed ]]
then
    echo "[DEBUG] unzipped qsm outputs do not exist - unzipping them:"
    unzip /tmp/osfstorage/qsm_final.zip -d /tmp/02_qsm_output_precomputed
fi

echo "[DEBUG] starting run_5_analysis.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/${timeStamp}/QSMxT/run_5_analysis.py --labels_file /tmp/${timeStamp}/QSMxT/aseg_labels.csv --segmentations /tmp/${timeStamp}/03_segmentation/qsm_segmentations/*.nii --qsm_files /tmp/02_qsm_output_precomputed/qsm_final/*/*.nii --out_dir /tmp/${timeStamp}/05_analysis

[ -f  /tmp/${timeStamp}/05_analysis/sub-170705-134431-std-1312211075243167001_ses-1_run-1_T1w_segmentation_nii_trans.csv ] && echo "FILE exists." || exit 1
[ -f  /tmp/${timeStamp}/05_analysis/sub-170706-160506-std-1312211075243167001_ses-1_run-1_T1w_segmentation_nii_trans.csv ] && echo "FILE exists." || exit 1