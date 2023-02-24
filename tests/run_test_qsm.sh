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

container_path=`sudo docker run ${container} python -c "import os; print(os.environ['PATH'])"`
echo "[DEBUG] Running QSM pipeline tests..."
sudo docker run \
    --env BRANCH="${BRANCH}" \
    --env PYTHONPATH=/tmp/QSMxT \
    --env PATH=/tmp/QSMxT:/tmp/QSMxT/scripts:${container_path} \
    --env DOWNLOAD_URL="${DOWNLOAD_URL}" \
    --env DATA_PASS="${DATA_PASS}" \
    --env UPLOAD_URL="${UPLOAD_URL}" \
    -v /tmp:/tmp ${container} \
    pytest /tmp/QSMxT/tests/run_test_qsm.py -s

echo "Testing summary (will add images here later)" >> $GITHUB_STEP_SUMMARY
#echo "" >> $GITHUB_STEP_SUMMARY # this is a blank line
#echo "- Lets add a bullet point" >> $GITHUB_STEP_SUMMARY
#echo "- Lets add a second bullet point" >> $GITHUB_STEP_SUMMARY
#echo "- How about a third one?" >> $GITHUB_STEP_SUMMARY

