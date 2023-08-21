#!/usr/bin/env bash
set -e

# Run the tests
echo "[DEBUG] Running QSM pipeline tests..."
docker exec qsmxt-container bash -c "pytest /tmp/QSMxT/qsmxt/tests/test_segmentation.py -s -k \"${@}\""

# Write test summary
if [ -f /tmp/GITHUB_STEP_SUMMARY.md ]; then
    echo "[DEBUG] Writing test summary..."
    cat /tmp/GITHUB_STEP_SUMMARY.md >> $GITHUB_STEP_SUMMARY
fi

# Stop and remove the container when you're done
echo "[DEBUG] Stopping and removing QSMxT container"
docker stop qsmxt-container
docker rm qsmxt-container

