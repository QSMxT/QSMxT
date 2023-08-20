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

echo "[DEBUG] dicom-sort /tmp/dicoms /tmp/dicoms-sorted"
dicom-sort /tmp/dicoms /tmp/dicoms-sorted

echo "[DEBUG] /usr/share/miniconda/bin/python3 /tmp/QSMxT/dicom_convert.py /tmp/dicoms-sorted /tmp/bids --auto_yes"
dicom-convert /tmp/dicoms-sorted /tmp/bids --auto_yes

echo "[DEBUG] /usr/share/miniconda/bin/python3 /tmp/QSMxT/qsmxt.py /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes"
qsmxt /tmp/bids /tmp/out --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

