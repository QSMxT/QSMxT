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

unzip /tmp/osfstorage/qsm_final.zip -d /tmp/02_qsm_output_precomputed/*

echo "[DEBUG] starting run_4_template.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_4_template.py /tmp/01_bids /tmp/02_qsm_output_precomputed/* /tmp/04_template

echo "[DEBUG] testing if outputs are there:"
echo "[DEBUG] ls /tmp:"
ls /tmp
echo "[DEBUG] ls /tmp/04_template:"
ls /tmp/04_template

echo "[DEBUG]: /tmp/04_template/magnitude_template"
[ -d /tmp/04_template/magnitude_template/ ] && echo "magnitude template exists." || exit 1

echo "[DEBUG]: /tmp/04_template/qsm_template"
[ -d /tmp/04_template/qsm_template/ ] && echo "qsm template exists." || exit 1

echo "[DEBUG]: /tmp/04_template/qsms_transformed"
[ -d /tmp/04_template/qsms_transformed/ ] && echo "transformed qsm images exist." || exit 1

echo "[DEBUG]: /tmp/04_template/transforms"
[ -d /tmp/04_template/transforms/ ] && echo "transforms exist." || exit 1

