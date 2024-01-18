#!/usr/bin/env bash
set -e 

# === ACQUIRE LOCK ===

# Set a trap to ensure the lock file is removed even if the script exits unexpectedly
LOCK_FILE="${TEST_DIR}/qsmxt.lock"
trap 'rm -f ${LOCK_FILE}; echo "[DEBUG] Lock ${LOCK_FILE} released due to script exit"; exit' INT TERM EXIT

# Function to generate a random sleep time between MIN_WAIT_TIME and MAX_WAIT_TIME
MAX_WAIT_TIME=10
MIN_WAIT_TIME=5
function random_sleep_time() {
    echo $((RANDOM % (MAX_WAIT_TIME - MIN_WAIT_TIME + 1) + MIN_WAIT_TIME))
}

# Loop until the lock file can be acquired
echo "[DEBUG] Create ${TEST_DIR}..."
mkdir -p "${TEST_DIR}"

echo "[DEBUG] Checking for ${LOCK_FILE}..."
while true; do
    if [ ! -f "${LOCK_FILE}" ]; then
        touch "${LOCK_FILE}"
        echo "[DEBUG] ${LOCK_FILE} acquired"
        break
    else
        echo "[DEBUG] Another process is using resources, waiting..."
        sleep $(random_sleep_time)
    fi
done

echo "[DEBUG] TEST_DIR=${TEST_DIR}"
cd "${TEST_DIR}"

# === DETERMINE INSTALL TYPE ===
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 [docker|apptainer]"
    exit 1
fi
CONTAINER_TYPE=$1

# === GET CORRECT BRANCH OF QSMxT REPO ===
echo "GITHUB_HEAD_REF: ${GITHUB_HEAD_REF}"
echo "GITHUB_REF: ${GITHUB_REF}"
echo "GITHUB_REF##*/: ${GITHUB_REF##*/}"

if [ -n "${GITHUB_HEAD_REF}" ]; then
    echo "GITHUB_HEAD_REF DEFINED: ${GITHUB_HEAD_REF}"
    BRANCH=${GITHUB_HEAD_REF}
elif [ -n "${GITHUB_REF##*/}" ]; then
    echo "GITHUB_HEAD_REF UNDEFINED... USING GITHUB_REF##*/: ${GITHUB_REF##*/}"
    BRANCH=${GITHUB_REF##*/}
else
    echo "NEITHER GITHUB_HEAD_REF NOR GITHUB_REF DEFINED. ASSUMING MAIN."
    BRANCH=main
fi

echo "[DEBUG] Checking for existing QSMxT repository in ${TEST_DIR}/QSMxT..."
if [ -d "${TEST_DIR}/QSMxT" ]; then
    echo "[DEBUG] Repository already exists. Switching to the correct branch and resetting changes..."
    cd ${TEST_DIR}/QSMxT
    git fetch --all
    git reset --hard
else
    echo "[DEBUG] Repository does not exist. Cloning..."
    git clone "https://github.com/QSMxT/QSMxT.git" "${TEST_DIR}/QSMxT"
fi
echo "[DEBUG] Switching to branch ${BRANCH} and pulling latest changes"
cd ${TEST_DIR}/QSMxT
git checkout "${BRANCH}"
git pull origin "${BRANCH}"
cd "${TEST_DIR}"

echo "[DEBUG] Extracting version information from docs/_config.yml"
TEST_CONTAINER_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_VERSION' | awk '{print $2}')
TEST_CONTAINER_DATE=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_DATE' | awk '{print $2}')
DEPLOY_PACKAGE_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'DEPLOY_PACKAGE_VERSION' | awk '{print $2}')
TEST_PACKAGE_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'TEST_PACKAGE_VERSION' | awk '{print $2}')
PROD_PACKAGE_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'PROD_PACKAGE_VERSION' | awk '{print $2}')
REQUIRED_PACKAGE_VERSION="${!REQUIRED_VERSION_TYPE}"
echo "[DEBUG] TEST_CONTAINER_VERSION=${TEST_CONTAINER_VERSION}"
echo "[DEBUG] TEST_CONTAINER_DATE=${TEST_CONTAINER_DATE}"
echo "[DEBUG] DEPLOY_PACKAGE_VERSION=${DEPLOY_PACKAGE_VERSION}"
echo "[DEBUG] TEST_PACKAGE_VERSION=${TEST_PACKAGE_VERSION}"
echo "[DEBUG] PROD_PACKAGE_VERSION=${PROD_PACKAGE_VERSION}"
echo "[DEBUG] REQUIRED_PACKAGE_TYPE=${REQUIRED_VERSION_TYPE}"
echo "[DEBUG] REQUIRED_PACKAGE_VERSION=${REQUIRED_PACKAGE_VERSION}"

# docker container setup
if [ "${CONTAINER_TYPE}" = "docker" ]; then
    echo "[DEBUG] Pulling QSMxT container vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}..."
    docker pull "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}"

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
        echo "[DEBUG] Creating qsmxt-container..."
        docker create --name qsmxt-container -it \
            -v ${TEST_DIR}/:${TEST_DIR} \
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
    echo "[DEBUG] Checking if qsmxt is already installed"
    QSMXT_INSTALL_CHECK=$(docker exec qsmxt-container pip list | grep 'qsmxt')

    if [ -z "${QSMXT_INSTALL_CHECK}" ]; then
        echo "[DEBUG] QSMxT is not installed. Installing..."
        docker exec -e REQUIRED_VERSION_TYPE=${REQUIRED_VERSION_TYPE} qsmxt-container bash -c "pip install -e ${TEST_DIR}/QSMxT"
    else
        echo "[DEBUG] QSMxT is installed, checking for linked installation and version"
        QSMXT_INSTALL_PATH=$(docker exec qsmxt-container pip show qsmxt | grep 'Location:' | awk '{print $2}')
        QSMXT_VERSION=$(docker exec qsmxt-container bash -c "qsmxt --version")

        echo "[DEBUG] QSMxT installed at ${QSMXT_INSTALL_PATH}"
        echo "[DEBUG] ${QSMXT_VERSION}"

        if [ "${QSMXT_INSTALL_PATH}" = "${TEST_DIR}/QSMxT" ] && [[ "${QSMXT_VERSION}" == *"${TEST_PACKAGE_VERSION}"* ]]; then
            echo "[DEBUG] QSMxT is already installed as a linked installation and version matches."
        else
            echo "[DEBUG] QSMxT is not installed as a linked installation or version mismatch. Reinstalling..."
            docker exec qsmxt-container bash -c "pip uninstall qsmxt -y"
            docker exec -e REQUIRED_VERSION_TYPE=${REQUIRED_VERSION_TYPE} qsmxt-container bash -c "pip install -e ${TEST_DIR}/QSMxT"
        fi
    fi
fi

# apptainer container setup
if [ "${CONTAINER_TYPE}" = "apptainer" ]; then
    echo "[DEBUG] Installing apptainer..."
    sudo apt-get update
    sudo apt-get install -y software-properties-common
    sudo add-apt-repository -y ppa:apptainer/ppa
    sudo apt-get update
    sudo apt-get install -y apptainer

    export PROD_CONTAINER_VERSION=${TEST_CONTAINER_VERSION}
    export PROD_CONTAINER_DATE=${TEST_CONTAINER_DATE}
    export PROD_PACKAGE_VERSION=${TEST_CONTAINER_VERSION}

    echo "[DEBUG] Requires transparent-singularity installation ${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}"

    if [ ! -n "${TEST_DIR}/qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}/qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}.simg" ]; then
        echo "[DEBUG] Install QSMxT via transparent singularity..."
        ${TEST_DIR}/QSMxT/docs/_includes/transparent_singularity_install.sh
    else
        echo "[DEBUG] Existing installation found with correct version"
    fi

    echo "[DEBUG] cd ${TEST_DIR}/qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE} && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../"
    cd ${TEST_DIR}/qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE} && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../

    echo "[DEBUG] which julia"
    which julia

    echo "[DEBUG] remove executables we are replacing"
    for f in {python3,python,qsmxt,dicom-sort,dicom-convert}; do
        rm -rf ${TEST_DIR}/qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}/${f}
    done

    if [ ! -n "~/miniconda3" ]; then
        echo "[DEBUG] Install miniconda..."
        ${TEST_DIR}/QSMxT/docs/_includes/miniconda_install.sh
    else
        echo "[DEBUG] Existing miniconda installation found!"
    fi
    export PATH="~/miniconda3/envs/qsmxt/bin:${PATH}"

    echo "[DEBUG] which pip && which python"
    which pip && which python

    # Run the commands inside the container using docker exec
    echo "[DEBUG] Checking if qsmxt is already installed"
    QSMXT_INSTALL_CHECK=$(which qsmxt)

    if [ -z "${QSMXT_INSTALL_CHECK}" ]; then
        echo "[DEBUG] QSMxT is not installed. Installing..."
        pip install -e ${TEST_DIR}/QSMxT
    else
        echo "[DEBUG] QSMxT is installed, checking for linked installation and version"
        QSMXT_INSTALL_PATH=$(pip show qsmxt | grep 'Location:' | awk '{print $2}')
        QSMXT_VERSION=$(qsmxt --version)

        echo "[DEBUG] QSMxT installed at ${QSMXT_INSTALL_PATH}"
        echo "[DEBUG] ${QSMXT_VERSION}"

        if [ "${QSMXT_INSTALL_PATH}" = "${TEST_DIR}/QSMxT" ] && [[ "${QSMXT_VERSION}" == *"${REQUIRED_PACKAGE_VERSION}"* ]]; then
            echo "[DEBUG] QSMxT is already installed as a linked installation and version matches."
        else
            echo "[DEBUG] QSMxT is not installed as a linked installation or version mismatch. Reinstalling..."
            pip uninstall qsmxt -y
            pip install -e ${TEST_DIR}/QSMxT
            echo "[DEBUG] `qsmxt --version`"
        fi
    fi
fi

rm -f "${LOCK_FILE}"

