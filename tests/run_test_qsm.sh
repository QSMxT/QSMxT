#!/usr/bin/env bash

container=vnmd/qsmxt_1.1.1:20210608

docker pull $container

cp -r . /tmp/QSMxT

pip install osfclient
osf -p ru43c clone /tmp
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms

echo "[DEBUG] starting run_0_dicomSort.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomToBids.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_1_dicomToBids.py /tmp/00_dicom /tmp/01_bids

echo "[DEBUG] starting run_2_qsm.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output

md5sum --check tests/test_hashes_qsm.txt