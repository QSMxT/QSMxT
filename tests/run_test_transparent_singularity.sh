#!/usr/bin/env bash
set -e

echo "[DEBUG] Install singularity 2.6.1 from neurodebian"
wget -O- http://neuro.debian.net/lists/focal.us-nh.full | sudo tee /etc/apt/sources.list.d/neurodebian.sources.list
# echo "[DEBUG] Install key"
# sudo apt-key adv --recv-keys --keyserver hkp://pool.sks-keyservers.net:80 0xA5D32F012649A5A9
# keyserver sometimes fails :( -> --allow-unauthenticated as a workaround?
echo "[DEBUG] sudo apt-get update --allow-insecure-repositories"
sudo apt-get update --allow-insecure-repositories
echo "[DEBUG] sudo apt-get update --allow-unauthenticated"
sudo apt-get install --allow-unauthenticated singularity-container 
sudo apt install singularity-container

cp -r . /tmp/QSMxT

echo "[DEBUG]: testing the transparent singularity command from the README:"
clone_command=`cat /tmp/QSMxT/README.md | grep https://github.com/NeuroDesk/transparent-singularity`
cd_command=`cat /tmp/QSMxT/README.md | grep "cd qsmxt_"`
run_command=`cat /tmp/QSMxT/README.md | grep "run_transparent_singularity"`
source_command=`cat /tmp/QSMxT/README.md | grep "source activate_qsmxt_"`
echo $clone_command
$clone_command
echo $cd_command
$cd_command
echo $run_command
$run_command

pip install osfclient
osf -p ru43c clone /tmp
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms


echo "[DEBUG]: testing the python setup commands from the README:"

wget_command=`cat /tmp/QSMxT/README.md | grep "wget https://repo.anaconda.com/miniconda"`
echo $wget_command
$wget_command

bash_command=`cat /tmp/QSMxT/README.md | grep "bash Miniconda3"`
echo $bash_command
$bash_command

source ~/.bashrc
conda install -c conda-forge -y nipype

echo "[DEBUG] starting run_0_dicomSort.py"
python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomToBids.py"
python3 /tmp/QSMxT/run_1_dicomToBids.py /tmp/00_dicom /tmp/01_bids

echo "[DEBUG] starting run_2_qsm.py normal"
python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2
[ -f /tmp/02_qsm_output/qsm_final/_run_run-1/sub-170705134431std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm-filled_000_average.nii ] && echo "[DEBUG]. Test OK." || exit 1
[ -f /tmp/02_qsm_output/qsm_final/_run_run-1/sub-170706160506std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm-filled_000_average.nii ] && echo "[DEBUG]. Test OK." || exit 1
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_4_template.py"
python3 /tmp/QSMxT/run_4_template.py /tmp/01_bids /tmp/02_qsm_output /tmp/04_template

