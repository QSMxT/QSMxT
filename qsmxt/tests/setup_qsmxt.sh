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

echo "[DEBUG] Pulling QSMxT repository (branch=${BRANCH})..."
git clone -b "${BRANCH}" "https://github.com/QSMxT/QSMxT.git" "/tmp/QSMxT"

echo "[DEBUG] Extracting TEST_CONTAINER_VERSION and TEST_CONTAINER_DATE from docs/_config.yml"
TEST_CONTAINER_VERSION=$(cat /tmp/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_VERSION' | awk '{print $2}')
TEST_CONTAINER_DATE=$(cat /tmp/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_DATE' | awk '{print $2}')

echo "[DEBUG] Pulling QSMxT container vnmd/qsmxt:${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}..."
sudo docker pull "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}"

# Create and start the container with a bash shell
echo "[DEBUG] Starting QSMxT container"
docker create --name qsmxt-container -it \
    -v /tmp/:/tmp \
    --env WEBDAV_LOGIN="${WEBDAV_LOGIN}" \
    --env WEBDAV_PASSWORD="${WEBDAV_PASSWORD}" \
    --env FREEIMAGE_KEY="${FREEIMAGE_KEY}" \
    --env OSF_TOKEN="${OSF_TOKEN}" \
    --env OSF_USER="${OSF_USER}" \
    --env OSF_PASS="${OSF_PASS}" \
    "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}" \
    /bin/bash
docker start qsmxt-container

# Run the commands inside the container using docker exec
echo "[DEBUG] Replacing qsmxt pip package with repository version"
docker exec qsmxt-container bash -c "pip uninstall qsmxt -y"
docker exec qsmxt-container bash -c "pip install -e /tmp/QSMxT"

# Test environment variables
echo "[DEBUG] Testing environment variables"
echo "--${OSF_TOKEN}--"
docker exec qsmxt-container bash -c "echo --\"${OSF_TOKEN}\"--"

