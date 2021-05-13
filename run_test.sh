#!/usr/bin/env bash

container=docker.pkg.github.com/neurodesk/caid/qsmxt_1.0.0:20210305

docker pull $container

cd /tmp
pip install osfclient
osf -p ru43c clone .
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d dicoms
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d dicoms
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d dicoms
unzip osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d dicoms

docker run -v /tmp:/tmp vnmd/qsmxt_1.0.0:20210305 python3 /opt/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom
docker run -v /tmp:/tmp vnmd/qsmxt_1.0.0:20210305 python3 /opt/QSMxT/run_1_dicomToBids.py /tmp/00_dicom /tmp/01_bids
docker run -v /tmp:/tmp vnmd/qsmxt_1.0.0:20210305 python3 /opt/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output

# don't test right now because of missing data:
# python3 /opt/QSMxT/run_3_segment.py 01_bids 03_segmentation
# python3 /opt/QSMxT/run_6_analysis.py --labels_file /opt/QSMxT/aseg_labels.csv --segmentations 03_segmentation/qsm_segmentation/*.nii --qsm_files 02_qsm_output/qsm_final/*.nii --out_dir 06_analysis
# python3 /opt/QSMxT/run_6_analysis --segmentations my_segmentation.nii --qsm_files 05_qsm_template/qsm_transformed/*/*.nii --out_dir 07_analysis

# don't test right now because it takes too long:
# docker run -it -v /tmp:/tmp vnmd/qsmxt_1.0.0:20210305 python3 /opt/QSMxT/run_4_magnitudeTemplate.py /tmp/01_bids /tmp/04_magnitude_template

cd -

md5sum --check test_hashes.txt