#!/usr/bin/env bash
set -e 

echo "GITHUB_HEAD_REF: ${GITHUB_HEAD_REF}"
echo "GITHUB_REF: ${GITHUB_REF}"
echo "GITHUB_REF##*/: ${GITHUB_REF##*/}"

if [ -n "${GITHUB_HEAD_REF}" ]; then
    echo "GITHUB_HEAD_REF DEFINED... USING IT."
    BRANCH=${GITHUB_HEAD_REF}
elif [ -n "${GITHUB_REF##*/}" ]; then
    echo "GITHUB_HEAD_REF UNDEFINED... USING GITHUB_REF##*/"
    BRANCH=${GITHUB_REF##*/}
else
    echo "NEITHER GITHUB_HEAD_REF NOR GITHUB_REF DEFINED. ASSUMING MAIN."
    BRANCH=main
fi

echo "[DEBUG] Checking for existing QSMxT repository in /storage/tmp/QSMxT..."
if [ -d "/storage/tmp/QSMxT" ]; then
    echo "[DEBUG] Repository already exists. Switching to the correct branch and resetting changes..."
    cd /storage/tmp/QSMxT
    git fetch --all
    git reset --hard
else
    echo "[DEBUG] Repository does not exist. Cloning..."
    git clone "https://github.com/QSMxT/QSMxT.git" "/storage/tmp/QSMxT"
fi
echo "[DEBUG] Switching to branch ${BRANCH} and pulling latest changes"
git checkout "${BRANCH}"
git pull origin "${BRANCH}"

echo "[DEBUG] Install QSMxT via transparent-singularity"
mkdir -p /storage/tmp/test-transparent-singularity
cd /storage/tmp/test-transparent-singularity
export PROD_CONTAINER_VERSION=${TEST_CONTAINER_VERSION}
export PROD_CONTAINER_DATE=${TEST_CONTAINER_DATE}
/storage/tmp/QSMxT/docs/_includes/transparent_singularity_install.sh

echo "[DEBUG] cd qsmxt_* && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../"
cd qsmxt_* && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../

echo "[DEBUG] which julia"
which julia

echo "[DEBUG] remove executables we are replacing"
for f in {python3,python,qsmxt,dicom-sort,dicom-convert}; do
    sudo rm -rf qsmxt_*/${f}
done

echo "[DEBUG] Install miniconda"
sudo rm -rf ~/miniconda3
/storage/tmp/QSMxT/docs/_includes/miniconda_install.sh
export PATH="~/miniconda3/envs/qsmxt/bin:${PATH}"

echo "[DEBUG] Install QSMxT via pip linked installation"
pip uninstall qsmxt -y
pip install -e /storage/tmp/QSMxT

echo "[DEBUG] Download test data"
pip install osfclient > /dev/null 2>&1
sudo rm -rf osfstorage
osf -p ru43c clone . > /dev/null 2>&1
sudo rm -rf dicoms-unsorted
tar xf osfstorage/dicoms-unsorted.tar -C .

echo "[DEBUG] dicom-sort dicoms-unsorted dicoms-sorted"
sudo rm -rf dicoms-sorted
dicom-sort dicoms-unsorted dicoms-sorted

echo "[DEBUG] dicom-convert dicoms-sorted bids --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'"
sudo rm -rf bids
dicom-convert dicoms-sorted bids --auto_yes --qsm_protocol_patterns '*qsm*' --t1w_protocol_patterns '*mp2rage*'

echo "[DEBUG] bids qsm --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug"
sudo rm -rf qsm
qsmxt bids qsm --premade fast --do_qsm --do_template --do_segmentation --do_analysis --auto_yes --debug

