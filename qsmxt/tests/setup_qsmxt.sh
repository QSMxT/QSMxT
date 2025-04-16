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
echo "[DEBUG] Create ${TEST_DIR}"
mkdir -p "${TEST_DIR}"

echo "[DEBUG] Checking for ${LOCK_FILE}"
while true; do
    if [ ! -f "${LOCK_FILE}" ]; then
        touch "${LOCK_FILE}"
        echo "[DEBUG] ${LOCK_FILE} acquired"
        break
    else
        echo "[DEBUG] Another process is using resources, waiting"
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

echo "[DEBUG] Checking for existing QSMxT repository in ${TEST_DIR}/QSMxT"
if [ -d "${TEST_DIR}/QSMxT" ]; then
    echo "[DEBUG] Repository already exists. Switching to the correct branch and resetting changes"
    cd ${TEST_DIR}/QSMxT
    git fetch --all
    git reset --hard
else
    echo "[DEBUG] Repository does not exist. Cloning"
    git clone "https://github.com/QSMxT/QSMxT.git" "${TEST_DIR}/QSMxT"
fi
echo "[DEBUG] Switching to branch ${BRANCH} and pulling latest changes"
cd ${TEST_DIR}/QSMxT
git checkout "${BRANCH}"
git pull origin "${BRANCH}"
cd "${TEST_DIR}"

echo "[DEBUG] Extracting version information from docs/_config.yml"
export TEST_CONTAINER_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_VERSION' | awk '{print $2}')
export TEST_CONTAINER_DATE=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'TEST_CONTAINER_DATE' | awk '{print $2}')
export TEST_PACKAGE_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'TEST_PACKAGE_VERSION' | awk '{print $2}')
export TEST_PYTHON_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'TEST_PYTHON_VERSION' | awk '{print $2}')
export PROD_PACKAGE_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'PROD_PACKAGE_VERSION' | awk '{print $2}')
export PROD_PYTHON_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'PROD_PYTHON_VERSION' | awk '{print $2}')
export DEPLOY_PACKAGE_VERSION=$(cat ${TEST_DIR}/QSMxT/docs/_config.yml | grep 'DEPLOY_PACKAGE_VERSION' | awk '{print $2}')
export REQUIRED_PACKAGE_VERSION="${!REQUIRED_VERSION_TYPE}"
echo "[DEBUG] TEST_CONTAINER_VERSION=${TEST_CONTAINER_VERSION}"
echo "[DEBUG] TEST_CONTAINER_DATE=${TEST_CONTAINER_DATE}"
echo "[DEBUG] TEST_PACKAGE_VERSION=${TEST_PACKAGE_VERSION}"
echo "[DEBUG] TEST_PYTHON_VERSION=${TEST_PYTHON_VERSION}"
echo "[DEBUG] PROD_PACKAGE_VERSION=${PROD_PACKAGE_VERSION}"
echo "[DEBUG] PROD_PYTHON_VERSION=${PROD_PYTHON_VERSION}"
echo "[DEBUG] DEPLOY_PACKAGE_VERSION=${DEPLOY_PACKAGE_VERSION}"
echo "[DEBUG] REQUIRED_PACKAGE_TYPE=${REQUIRED_VERSION_TYPE}"
echo "[DEBUG] REQUIRED_PACKAGE_VERSION=${REQUIRED_PACKAGE_VERSION}"

# docker container setup
if [ "${CONTAINER_TYPE}" = "docker" ]; then
    echo "[DEBUG] Installing via docker"
    echo "[DEBUG] Pulling QSMxT container vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}"
    sudo docker pull "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}"

    # Check if the container exists and its image version
    echo "[DEBUG] Checking if container name qsmxt-container already exists"
    CONTAINER_EXISTS=$(sudo docker ps -a -q -f name=qsmxt-container)
    if [ -n "${CONTAINER_EXISTS}" ]; then
        echo "[DEBUG] qsmxt-container already exists."
        CONTAINER_IMAGE=$(sudo docker inspect qsmxt-container --format='{{.Config.Image}}' 2>/dev/null || echo "")
        if [ "${CONTAINER_IMAGE}" != "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}" ]; then
            echo "[DEBUG] Existing container image ${CONTAINER_IMAGE} has a different version than required version vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}"
            echo "[DEBUG] Stopping container"
            sudo docker stop qsmxt-container
            echo "[DEBUG] Removing container"
            sudo docker rm qsmxt-container
            echo "[DEBUG] Removing image ${CONTAINER_IMAGE}"
            sudo docker rmi -f "${CONTAINER_IMAGE}"
        fi
    fi

    echo "[DEBUG] Checking if qsmxt-container already exists"
    CONTAINER_EXISTS=$(sudo docker ps -a -q -f name=qsmxt-container)
    if [ ! -n "${CONTAINER_EXISTS}" ]; then
        echo "[DEBUG] Creating qsmxt-container using image vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}"
        sudo docker create --name qsmxt-container -it \
            -v ${TEST_DIR}/:${TEST_DIR} \
            -v /storage:/storage \
            --env WEBDAV_LOGIN="${WEBDAV_LOGIN}" \
            --env WEBDAV_PASSWORD="${WEBDAV_PASSWORD}" \
            --env FREEIMAGE_KEY="${FREEIMAGE_KEY}" \
            --env OSF_TOKEN="${OSF_TOKEN}" \
            --env OSF_USERNAME="${OSF_USERNAME}" \
            --env OSF_PASSWORD="${OSF_PASSWORD}" \
            --env GITHUB_STEP_SUMMARY="${GITHUB_STEP_SUMMARY}" \
            "vnmd/qsmxt_${TEST_CONTAINER_VERSION}:${TEST_CONTAINER_DATE}" \
            /bin/bash
    fi

    echo "[DEBUG] Checking if qsmxt-container is running"
    CONTAINER_RUNNING=$(sudo docker ps -q -f name=qsmxt-container)
    if [ ! -n "${CONTAINER_RUNNING}" ]; then
        echo "[DEBUG] Starting QSMxT container"
        sudo docker start qsmxt-container
    fi

    # Run the commands inside the container using docker exec
    echo "[DEBUG] Checking if qsmxt is already installed"
    QSMXT_INSTALL_CHECK=$(sudo docker exec qsmxt-container pip list | grep 'qsmxt' || true)

    if [ -z "${QSMXT_INSTALL_CHECK}" ]; then
        echo "[DEBUG] QSMxT is not installed. Installing."
        sudo docker exec -e REQUIRED_VERSION_TYPE=${REQUIRED_VERSION_TYPE} qsmxt-container bash -c "pip install -e ${TEST_DIR}/QSMxT"
    else
        echo "[DEBUG] QSMxT is installed, checking for linked installation and version"
        QSMXT_INSTALL_PATH=$(sudo docker exec qsmxt-container pip show qsmxt | grep 'Location:' | awk '{print $2}')
        QSMXT_VERSION=$(sudo docker exec qsmxt-container bash -c "qsmxt --version")
        QSMXT_VERSION=$(echo "$QSMXT_VERSION" | grep -oP 'v\K[0-9]+\.[0-9]+\.[0-9]+')

        echo "[DEBUG] QSMxT installed at ${QSMXT_INSTALL_PATH}"
        echo "[DEBUG] ${QSMXT_VERSION}"

        if [ "${QSMXT_INSTALL_PATH}" = "${TEST_DIR}/QSMxT" ] && [[ "${QSMXT_VERSION}" == *"${TEST_PACKAGE_VERSION}"* ]]; then
            echo "[DEBUG] QSMxT is already installed as a linked installation and version matches."
        else
            echo "[DEBUG] QSMxT is not installed as a linked installation or version mismatch. Reinstalling."
            sudo docker exec qsmxt-container bash -c "pip uninstall qsmxt -y"
            sudo docker exec -e REQUIRED_VERSION_TYPE=${REQUIRED_VERSION_TYPE} qsmxt-container bash -c "pip install -e ${TEST_DIR}/QSMxT"
        fi
    fi
fi

# apptainer container setup
if [ "${CONTAINER_TYPE}" = "apptainer" ]; then
    echo "[DEBUG] Installing apptainer"
    sudo apt-get update
    sudo apt-get install -y software-properties-common
    sudo add-apt-repository -y ppa:apptainer/ppa
    sudo apt-get update
    sudo apt-get install -y apptainer

    echo "[DEBUG] Requires transparent-apptainer installation qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}"
    export PROD_CONTAINER_VERSION=${TEST_CONTAINER_VERSION}
    export PROD_CONTAINER_DATE=${TEST_CONTAINER_DATE}
    export PROD_PACKAGE_VERSION=${PROD_PACKAGE_VERSION}
    export PROD_PYTHON_VERSION=${TEST_PYTHON_VERSION}

    if [ ! -f "${TEST_DIR}/qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}/qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg" ]; then
        echo "[DEBUG] Install QSMxT via transparent apptainer"
        ${TEST_DIR}/QSMxT/docs/_includes/transparent_apptainer_install.sh
    else
        echo "[DEBUG] Existing installation found"
    fi

    echo "[DEBUG] cd ${TEST_DIR}/qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE} && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../"
    cd ${TEST_DIR}/qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE} && source activate_qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}.simg.sh && cd ../
    export PATH="${TEST_DIR}/qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}:${PATH}"
    echo "[DEBUG] Updated path: ${PATH}"

    echo "[DEBUG] which julia && which dcm2niix"
    which julia && which dcm2niix

    echo "[DEBUG] remove executables we are replacing"
    for f in {python3,python,qsmxt,dicom-convert,nifti-convert}; do
        rm -rf ${TEST_DIR}/qsmxt_${TEST_CONTAINER_VERSION}_${TEST_CONTAINER_DATE}/${f}
    done

    echo "[DEBUG] which conda && which python && which pip"
    which conda && which python && which pip

    echo "[DEBUG] which qsmxt && which dicom-convert && which nifti-convert"
    which qsmxt && which dicom-convert && which nifti-convert

    export PROD_MINIFORGE_PATH="${TEST_DIR}/miniforge3"
    echo "[DEBUG] Checking for existing miniforge installation"
    if [ ! -d ${PROD_MINIFORGE_PATH} ]; then
        echo "[DEBUG] Install miniforge"
        ${TEST_DIR}/QSMxT/docs/_includes/miniforge_install.sh
    else
        echo "[DEBUG] Existing miniforge installation found!"
    fi
    source ${PROD_MINIFORGE_PATH}/etc/profile.d/conda.sh
    export PATH=${PROD_MINIFORGE_PATH}/bin:${PATH}

    echo "[DEBUG] which conda && which python && which pip"
    which conda && which python && which pip

    echo "[DEBUG] conda activate qsmxt"
    conda activate qsmxt

    echo "[DEBUG] which conda && which python && which pip"
    which conda && which python && which pip

    echo "[DEBUG] which qsmxt && which dicom-convert && which nifti-convert"
    which qsmxt && which dicom-convert && which nifti-convert
    
    echo "[DEBUG] python --version && pip --version"
    python --version && pip --version

    echo "[DEBUG] pip install --upgrade pip"
    pip install --upgrade pip

    echo "[DEBUG] Checking if qsmxt is already installed"
    QSMXT_INSTALL_CHECK=$(pip list | grep qsmxt || true)

    if [ -z "${QSMXT_INSTALL_CHECK}" ]; then
        echo "[DEBUG] QSMxT is not installed. Installing."
        pip install -e ${TEST_DIR}/QSMxT
    else
        echo "[DEBUG] Getting QSMxT location and version"
        QSMXT_INSTALL_PATH=$(pip show qsmxt | grep 'Location:' | awk '{print $2}')
        QSMXT_VERSION=$(qsmxt --version)
        QSMXT_VERSION=$(echo "$QSMXT_VERSION" | grep -oP 'v\K[0-9]+\.[0-9]+\.[0-9]+')
        echo "[DEBUG] QSMxT installed at ${QSMXT_INSTALL_PATH}"
        echo "[DEBUG] ${QSMXT_VERSION}"

        if [[ ! "${QSMXT_VERSION}" == *"${REQUIRED_PACKAGE_VERSION}"* ]]; then
            echo "[DEBUG] QSMxT is not installed as a linked installation or version mismatch. Reinstalling."
            pip uninstall qsmxt -y
            pip install -e ${TEST_DIR}/QSMxT
            echo "[DEBUG] `qsmxt --version`"
        fi
    fi

    echo "[DEBUG] which julia && which dcm2niix"
    which julia && dcm2niix

fi

# === GENERATE TEST DATA ===
if [ -d "${TEST_DIR}/bids" ]; then
    echo "[DEBUG] BIDS directory ${TEST_DIR}/bids exists."
else
    echo "[DEBUG] BIDS directory ${TEST_DIR}/bids does not exist. Downloading dependencies..."
    pip install qsm-forward==0.22 osfclient

    echo "[DEBUG] Pulling head phantom data from OSF..."
    echo osf --project "9jc42" --username "${OSF_USERNAME}" fetch data.tar "${TEST_DIR}/data.tar"
    osf --project "9jc42" --username "${OSF_USERNAME}" fetch data.tar "${TEST_DIR}/data.tar"

    echo "[DEBUG] Extracting..."
    tar xf "${TEST_DIR}/data.tar"

    echo "[DEBUG] Removing tar file..."
    rm "${TEST_DIR}/data.tar"

    echo "[DEBUG] Running forward model..."
    qsm-forward head "${TEST_DIR}/data" "${TEST_DIR}/bids" --subject 1 --session 1 --TR 0.0075 --TEs 0.0035 --flip_angle 40 --suffix T1w
    qsm-forward head "${TEST_DIR}/data" "${TEST_DIR}/bids" --subject 1 --session 1 --TR 0.05 --TEs 0.012 0.020 --flip_angle 15 --suffix MEGRE --save-phase
    qsm-forward head "${TEST_DIR}/data" "${TEST_DIR}/bids" --subject 1 --session 2 --TR 0.05 --TEs 0.012 0.020 --flip_angle 15 --suffix MEGRE --save-phase
    echo "[DEBUG] Data generation complete!"
fi

# === REMOVE LOCK FILE ===
echo "[DEBUG] Removing lockfile..."
rm -f "${LOCK_FILE}"

