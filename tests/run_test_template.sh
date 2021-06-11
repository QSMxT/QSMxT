#!/usr/bin/env bash

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

unzip /tmp/osfstorage/qsm_final.zip -d /tmp/02_qsm_output_precomputed

echo "[DEBUG] starting run_4_template.py"
docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_4_template.py /tmp/01_bids /tmp/02_qsm_output_precomputed /tmp/04_template

echo "[DEBUG] testing if outputs are there:"
echo "[DEBUG] ls /tmp:"
ls /tmp
echo "[DEBUG] ls /tmp/04_template:"
ls /tmp/04_template
echo "[DEBUG] ls /tmp/04_template/workflow_template:"
ls /tmp/04_template/workflow_template
echo "[DEBUG] ls /tmp/04_template/workflow_template/datasink:"
ls /tmp/04_template/workflow_template/datasink
echo "[DEBUG] ls /tmp/04_template/workflow_template/datasink/out:"
ls /tmp/04_template/workflow_template/datasink/out
echo "[DEBUG] ls /tmp/04_template/workflow_template/datasink/out/test:"
ls /tmp/04_template/workflow_template/datasink/out/test
echo "[DEBUG] ls /tmp/04_template/workflow_template/datasink/out/test/results:"
ls /tmp/04_template/workflow_template/datasink/out/test/results

echo "[DEBUG]: /tmp/04_template/workflow_template/datasink/out/test/results/PassiveTemplate/_ReshapeAveragePassiveImageWithShapeUpdate0/AVG_QSMWARP_AVG_QSM.nii.gz"
[ -f /tmp/04_template/workflow_template/datasink/out/test/results/PassiveTemplate/_ReshapeAveragePassiveImageWithShapeUpdate0/AVG_QSMWARP_AVG_QSM.nii.gz ] && echo "$FILE exist." || exit 1
echo "[DEBUG]: /tmp/04_template/workflow_template/datasink/out/test/results/PreRegisterAverage/average.nii"
[ -f /tmp/04_template/workflow_template/datasink/out/test/results/PreRegisterAverage/average.nii ] && echo "$FILE exist." || exit 1
echo "[DEBUG]: /tmp/04_template/workflow_template/datasink/out/test/results/PrimaryTemplate/iteration02_Reshaped.nii.gz"
[ -f /tmp/04_template/workflow_template/datasink/out/test/results/PrimaryTemplate/iteration02_Reshaped.nii.gz ] && echo "$FILE exist." || exit 1
