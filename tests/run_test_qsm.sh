#!/usr/bin/env bash
set -e 

echo "GITHUB_HEAD_REF: ${GITHUB_HEAD_REF}"
echo "GITHUB_REF: ${GITHUB_REF}"
echo "GITHUB_REF##*/: ${GITHUB_REF##*/}"

if [ -n "${GITHUB_HEAD_REF}" ]; then
    echo "GITHUB_HEAD_REF DEFINED... USING IT."
    BRANCH=${GITHUB_HEAD_REF}
else
    echo "GITHUB_HEAD_REF UNDEFINED... USING GITHUB_REF##*/"
    BRANCH=${GITHUB_REF##*/}
fi

echo "[DEBUG] Pulling QSMxT branch ${BRANCH}..."
git clone -b "${BRANCH}" "https://github.com/QSMxT/QSMxT.git" "/tmp/QSMxT"

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
echo "[DEBUG] Pulling QSMxT container ${container}..."
sudo docker pull "${container}"

echo "[DEBUG] Running QSM pipeline tests..."
sudo docker run \
    --env PYTHONPATH=/tmp/QSMxT \
    --env DATA_URL="${DATA_URL}" \
    --env DATA_PASS="${DATA_PASS}" \
    --env UPLOAD_URL="${UPLOAD_URL}" \
    --env UPLOAD_PASS="${UPLOAD_PASS}" \
    --env -v /tmp:/tmp \
    "${container}" \
    pytest /tmp/QSMxT/tests/run_test_qsm.py -s

