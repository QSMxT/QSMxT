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

echo "[DEBUG] Checking for existing QSMxT repository in /tmp/QSMxT..."
sudo rm -rf /tmp/QSMxT
if [ -d "/tmp/QSMxT" ]; then
    echo "[DEBUG] Repository already exists. Switching to the correct branch and pulling latest changes."
    cd /tmp/QSMxT
    git fetch --all
    git reset --hard
    git checkout "${BRANCH}"
    git pull origin "${BRANCH}"
else
    echo "[DEBUG] Repository does not exist. Cloning..."
    git clone "https://github.com/QSMxT/QSMxT.git" "/tmp/QSMxT"
fi

echo "[DEBUG] Extracting TEST_CONTAINER_VERSION and TEST_CONTAINER_DATE from docs/_config.yml"
TEST_CONTAINER_VERSION=$(cat /tmp/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_VERSION' | awk '{print $2}')
TEST_CONTAINER_DATE=$(cat /tmp/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_DATE' | awk '{print $2}')

echo "[DEBUG] Pulling QSMxT container vnmd/qsmxt:${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}..."
sudo docker pull "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}"

# Check if the container exists and its image version
CONTAINER_EXISTS=$(docker ps -a -q -f name=qsmxt-container)
if [ -n "${CONTAINER_EXISTS}" ]; then
    echo "[DEBUG] qsmxt-container already exists."
    CONTAINER_IMAGE=$(docker inspect qsmxt-container --format='{{.Config.Image}}' 2>/dev/null || echo "")
    if [ "${CONTAINER_IMAGE}" != "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}" ]; then
        echo "[DEBUG] Existing container has a different version. Stopping, removing it, and its image."
        docker stop qsmxt-container
        docker rm qsmxt-container
        docker rmi "${CONTAINER_IMAGE}"
    fi
fi

CONTAINER_EXISTS=$(docker ps -a -q -f name=qsmxt-container)
if [ ! -n "${CONTAINER_EXISTS}" ]; then
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
fi

CONTAINER_RUNNING=$(docker ps -q -f name=qsmxt-container)
if [ ! -n "${CONTAINER_RUNNING}" ]; then
    echo "[DEBUG] Starting QSMxT container"
    docker start qsmxt-container
fi

# Run the commands inside the container using docker exec
echo "[DEBUG] Checking if qsmxt is already installed as a linked installation"
QSMXT_LINKED_INSTALL=$(docker exec qsmxt-container pip list --format=freeze | grep 'qsmxt @' || echo "")

if [ -n "${QSMXT_LINKED_INSTALL}" ]; then
    echo "[DEBUG] qsmxt is already installed as a linked installation. No need to reinstall."
else
    echo "[DEBUG] qsmxt is not installed as a linked installation. Reinstalling..."
    docker exec qsmxt-container bash -c "pip uninstall qsmxt -y"
    docker exec qsmxt-container bash -c "pip install -e /tmp/QSMxT"
fi

# Test environment variables
echo "[DEBUG] Testing environment variables"
echo "--${OSF_TOKEN}--"
docker exec qsmxt-container bash -c "echo --\"${OSF_TOKEN}\"--"

