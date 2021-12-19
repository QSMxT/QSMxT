#!/usr/bin/env bash
set -e

echo "[DEBUG] Install singularity 2.6.1 from neurodebian"
wget -O- http://neuro.debian.net/lists/focal.us-nh.full | sudo tee /etc/apt/sources.list.d/neurodebian.sources.list
# echo "[DEBUG] Install key"
# sudo apt-key adv --recv-keys --keyserver hkp://pool.sks-keyservers.net:80 0xA5D32F012649A5A9
# keyserver sometimes fails :( -> --allow-unauthenticated as a workaround?
echo "[DEBUG] sudo apt-get update --allow-insecure-repositories"
sudo apt-get update --allow-insecure-repositories > /dev/null 2>&1
echo "[DEBUG] sudo apt-get update --allow-unauthenticated"
sudo apt-get install --allow-unauthenticated singularity-container > /dev/null 2>&1
sudo apt install singularity-container > /dev/null 2>&1

cp -r . /tmp/QSMxT
# git clone https://github.com/QSMxT/QSMxT.git /tmp/QSMxT

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

# echo "[DEBUG]: testing the julia package install command from the README:"
# run_command=`cat /tmp/QSMxT/README.md | grep "using Pkg"`
# run_command="./julia -e 'using Pkg; Pkg.status(); Pkg.add(\"MriResearchTools\"); Pkg.add(\"ArgParse\"); Pkg.status()'"
# echo $run_command
# $run_command
# singularity exec  --pwd $PWD qsmxt_1.1.6_20210623.simg julia -e 'using Pkg; Pkg.status(); Pkg.add("MriResearchTools"); Pkg.add("ArgParse"); Pkg.status()'



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

conda_command=`cat /tmp/QSMxT/README.md | grep "conda install "`
echo $conda_command
yes | $conda_command

bidscoin_command=`cat /tmp/QSMxT/README.md | grep "pip install bidscoin"`
echo $bidscoin_command
$bidscoin_command > /dev/null 2>&1

echo "[DEBUG] starting run_0_dicomSort.py"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomConvert.py"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_1_dicomConvert.py /tmp/00_dicom /tmp/01_bids --auto_yes

echo "[DEBUG] starting run_2_qsm.py normal"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2
echo "[DEBUG] checking outputs of run_2_qsm.py normal"
[ -f /tmp/02_qsm_output/qsm_final/_run_run-1/sub-170705-134431-std-1312211075243167001_ses-1_run-1_part-phase_T2starw_scaled_qsm_000_composite_average.nii ] && echo "[DEBUG]. Test OK." || exit 1
[ -f /tmp/02_qsm_output/qsm_final/_run_run-1/sub-170706-160506-std-1312211075243167001_ses-1_run-1_part-phase_T2starw_scaled_qsm_000_composite_average.nii ] && echo "[DEBUG]. Test OK." || exit 1
rm -rf /tmp/02_qsm_output/qsm_final/

echo "[DEBUG] starting run_2_qsm.py phase consistency (tests julia)"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --masking phase-based
echo "[DEBUG] checking outputs of run_2_qsm.py phase consistency (tests julia)"
[ -f /tmp/02_qsm_output/qsm_final/_run_run-1/sub-170705-134431-std-1312211075243167001_ses-1_run-1_part-phase_T2starw_scaled_qsm_000_composite_average.nii ] && echo "[DEBUG]. Test OK." || exit 1
[ -f /tmp/02_qsm_output/qsm_final/_run_run-1/sub-170706-160506-std-1312211075243167001_ses-1_run-1_part-phase_T2starw_scaled_qsm_000_composite_average.nii ] && echo "[DEBUG]. Test OK." || exit 1
rm -rf /tmp/02_qsm_output/qsm_final/

echo "[DEBUG] starting run_4_template.py"
/usr/share/miniconda/bin/python3 /tmp/QSMxT/run_4_template.py /tmp/01_bids /tmp/02_qsm_output /tmp/04_template
echo "[DEBUG]: checking /tmp/04_template/workflow_template/datasink/out/test/results"
[ -d /tmp/04_template/workflow_template/datasink/out/test/results/ ] && echo "results exist." || exit 1

