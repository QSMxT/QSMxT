#!/usr/bin/env bash
set -e 

echo "GITHUB_BASE_REF: ${GITHUB_BASE_REF}"
echo "GITHUB_REF: ${GITHUB_REF}"
exit

echo "[DEBUG] Pulling QSMxT branch ${GIT_BRANCH}..."
git clone -b "${GIT_BRANCH}" "https://github.com/QSMxT/QSMxT.git" "/tmp/QSMxT"

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
echo "[DEBUG] Pulling QSMxT container ${container}..."
sudo docker pull "${container}"

echo "[DEBUG] Running QSM pipeline tests..."
sudo docker run --env PYTHONPATH=/tmp/QSMxT --env DATA_URL="${DATA_URL}" --env DATA_PASS="${DATA_PASS}" -v /tmp:/tmp "${container}" pytest /tmp/QSMxT/tests/run_test_qsm.py -s

