#!/usr/bin/env bash
set -e

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
container_path=`sudo docker run ${container} python -c "import os; print(os.environ['PATH'])"`

echo "[DEBUG] Getting QSMxT dir..."
QSMXT_DIR=`sudo docker run \
    --env BRANCH="${BRANCH}" \
    --env WEBDAV_LOGIN="${WEBDAV_LOGIN}" \
    --env WEBDAV_PASSWORD="${WEBDAV_PASSWORD}" \
    --env FREEIMAGE_KEY="${FREEIMAGE_KEY}" \
    --env OSF_TOKEN="${OSF_TOKEN}" \
    -v /tmp:/tmp ${container} \
    get-qsmxt-dir`

echo "[DEBUG] Running template-buildling pipeline tests..."
sudo docker run \
    --env BRANCH="${BRANCH}" \
    --env WEBDAV_LOGIN="${WEBDAV_LOGIN}" \
    --env WEBDAV_PASSWORD="${WEBDAV_PASSWORD}" \
    --env FREEIMAGE_KEY="${FREEIMAGE_KEY}" \
    --env OSF_TOKEN="${OSF_TOKEN}" \
    -v /tmp:/tmp ${container} \
    pytest ${QSMXT_DIR}/tests/test_template.py -s -k "${@}"

if [ -f /tmp/GITHUB_STEP_SUMMARY.md ]; then
    cat /tmp/GITHUB_STEP_SUMMARY.md >> $GITHUB_STEP_SUMMARY
fi

