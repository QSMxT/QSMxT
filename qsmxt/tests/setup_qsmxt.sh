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
    echo "NEITHER GITHUB_HEAD_REF NOR GITHUB_REF DEFINED. ASSUMING MASTER."
    BRANCH=master
fi

echo "[DEBUG] Pulling QSMxT repository (branch=${BRANCH})..."
git clone -b "${BRANCH}" "https://github.com/QSMxT/QSMxT.git" "/tmp/QSMxT"

echo "[DEBUG] Pulling QSMxT container ${container}..."
container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
sudo docker pull "${container}" 

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
    ${container} \
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

