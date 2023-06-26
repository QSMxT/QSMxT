#!/usr/bin/env bash
set -e

# install apptainer
sudo apt-get install -y software-properties-common
sudo add-apt-repository -y ppa:apptainer/ppa
sudo apt-get update
sudo apt-get install -y apptainer

cp -r . /tmp/QSMxT

echo "[DEBUG]: testing the transparent singularity command from the README:"
clone_command=`cat /tmp/QSMxT/README.md | grep https://github.com/NeuroDesk/transparent-singularity`
echo $clone_command
$clone_command

cd_command=`cat /tmp/QSMxT/README.md | grep "cd qsmxt_"`
echo $cd_command
$cd_command

run_command=`cat /tmp/QSMxT/README.md | grep "run_transparent_singularity"`
echo $run_command
$run_command

source_command=`cat /tmp/QSMxT/README.md | grep "source activate_qsmxt_"`
echo $source_command
$source_command

echo "[DEBUG]: check julia executable:"
cat julia

echo "[DEBUG]: download test data"
pip install osfclient > /dev/null 2>&1
osf -p ru43c clone /tmp > /dev/null 2>&1
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms > /dev/null 2>&1
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms > /dev/null 2>&1
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms > /dev/null 2>&1
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms > /dev/null 2>&1

echo "[DEBUG]: testing the python setup commands from the README:"

wget_command=`cat /tmp/QSMxT/README.md | grep "wget https://repo.anaconda.com/miniconda"`
echo $wget_command
$wget_command > /dev/null 2>&1

bash_command=`cat /tmp/QSMxT/README.md | grep "bash Miniconda3"`
echo $bash_command
$bash_command

source ~/.bashrc
export PATH=/tmp/QSMxT:/usr/share/miniconda/bin:$PATH
export PYTHONPATH=/tmp/QSMxT:$PYTHONPATH

pip_command=`cat /tmp/QSMxT/README.md | grep "pip install "`
echo $pip_command
$pip_command

echo "[DEBUG] starting run_0_dicomSort.py"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomConvert.py"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_1_dicomConvert.py /tmp/00_dicom /tmp/01_bids --auto_yes

echo "[DEBUG] starting run_2_qsm.py (fast pipeline)"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --auto_yes --premade fast

echo "[DEBUG] checking outputs of run_2_qsm.py normal"
ls /tmp/02_qsm_output/qsm_final/**/**
rm -rf /tmp/02_qsm_output/qsm_workflow/

echo "[DEBUG] starting run_4_template.py"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_4_template.py /tmp/01_bids /tmp/02_qsm_output /tmp/04_template

echo "[DEBUG]: checking /tmp/04_template/ for results"
ls /tmp/04_template/
[ -d /tmp/04_template/initial_average ] && echo "initial_average exists." || exit 1
[ -d /tmp/04_template/magnitude_template ] && echo "magnitude_template exists." || exit 1
[ -d /tmp/04_template/qsm_template ] && echo "qsm_template exists." || exit 1
[ -d /tmp/04_template/transforms ] && echo "transforms exists." || exit 1
[ -d /tmp/04_template/qsms_transformed ] && echo "qsms_transformed exists." || exit 1

