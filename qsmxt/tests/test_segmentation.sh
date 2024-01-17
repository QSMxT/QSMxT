#!/usr/bin/env bash
set -e

# Run the tests
echo "[DEBUG] Running QSM pipeline tests..."

docker exec qsmxt-container bash -c "pytest /storage/tmp/QSMxT/qsmxt/tests/test_segmentation.py -s -k \"${@}\""

# Write test summary
if [ -f /storage/tmp/GITHUB_STEP_SUMMARY.md ]; then
    echo "[DEBUG] Writing test summary..."
    cat /storage/tmp/GITHUB_STEP_SUMMARY.md >> $GITHUB_STEP_SUMMARY
fi

