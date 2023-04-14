#!/usr/bin/env bash
set -e

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
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
    pytest /tmp/QSMxT/tests/test_qsm_pipeline.py -s $@

#echo "Testing summary (will add images here later)" >> $GITHUB_STEP_SUMMARY
#echo "" >> $GITHUB_STEP_SUMMARY # this is a blank line
#echo "- Lets add a bullet point" >> $GITHUB_STEP_SUMMARY
#echo "- Lets add a second bullet point" >> $GITHUB_STEP_SUMMARY
#echo "- How about a third one?" >> $GITHUB_STEP_SUMMARY

