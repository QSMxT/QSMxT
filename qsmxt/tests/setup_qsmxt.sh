#!/usr/bin/env bash
set -e 

# === SETUP LOCK FILE ===
LOCK_FILE="${TEST_DIR}/qsmxt.lock"
mkdir -p "${TEST_DIR}"

# === DETERMINE INSTALL TYPE ===
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 [docker|apptainer]"
    exit 1
fi
CONTAINER_TYPE=$1

echo "[DEBUG] TEST_DIR=${TEST_DIR}"
echo "[DEBUG] Process $$ attempting to acquire lock..."

# Use flock for atomic locking - this is the key improvement
# The entire critical section runs inside the lock
exec 200>"${LOCK_FILE}"
if ! flock -n 200; then
    echo "[DEBUG] Another process has the lock. Waiting..."
    # Wait up to 1800 seconds (30 minutes) for the lock
    if ! flock -w 1800 200; then
        echo "[ERROR] Could not acquire lock after 30 minutes. Exiting."
        exit 1
    fi
fi

echo "[DEBUG] Lock acquired by process $$"
cd "${TEST_DIR}"

# === VALIDATE ENVIRONMENT VARIABLES ===
# Check critical environment variables before proceeding
echo "[DEBUG] Validating environment variables..."
echo "[DEBUG] OSF_USERNAME=${OSF_USERNAME:-(empty)}"
echo "[DEBUG] OSF_TOKEN=${OSF_TOKEN:+set}${OSF_TOKEN:-(empty)}"
echo "[DEBUG] OSF_PASSWORD=${OSF_PASSWORD:+set}${OSF_PASSWORD:-(empty)}"

if [ -z "${OSF_USERNAME}" ]; then
    echo "[WARNING] OSF_USERNAME is empty - OSF operations may fail"
fi

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
get_config_value() {
    grep "^$1:" "${TEST_DIR}/QSMxT/docs/_config.yml" | awk '{print $2}'
}
export TEST_CONTAINER_VERSION=$(get_config_value 'TEST_CONTAINER_VERSION')
export TEST_CONTAINER_DATE=$(get_config_value 'TEST_CONTAINER_DATE')
export TEST_PACKAGE_VERSION=$(get_config_value 'TEST_PACKAGE_VERSION')
export TEST_PYTHON_VERSION=$(get_config_value 'TEST_PYTHON_VERSION')
export PROD_PACKAGE_VERSION=$(get_config_value 'PROD_PACKAGE_VERSION')
export PROD_PYTHON_VERSION=$(get_config_value 'PROD_PYTHON_VERSION')
export DEPLOY_PACKAGE_VERSION=$(get_config_value 'DEPLOY_PACKAGE_VERSION')
export REQUIRED_VERSION_TYPE=$(get_config_value 'REQUIRED_VERSION_TYPE')
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
    CREATE_CONTAINER=false

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
            CREATE_CONTAINER=true
        fi
    else
        CREATE_CONTAINER=true
    fi

    if [ "$CREATE_CONTAINER" = true ]; then
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
    echo "[DEBUG] Installing QSMxT with dev dependencies (will reinstall if already present)"
    sudo docker exec qsmxt-container bash -c "pip uninstall qsmxt -y"
    sudo docker exec -e REQUIRED_VERSION_TYPE=${REQUIRED_VERSION_TYPE} qsmxt-container bash -c "pip install -e ${TEST_DIR}/QSMxT[dev]"
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

    echo "[DEBUG] Installing QSMxT with dev dependencies (will reinstall if already present)"
    pip uninstall qsmxt -y
    pip install -e ${TEST_DIR}/QSMxT[dev]

    echo "[DEBUG] which julia && which dcm2niix"
    which julia && which dcm2niix

fi

# === GENERATE TEST DATA ===
# This section is inside the lock to prevent multiple processes from corrupting data
if [ -d "${TEST_DIR}/bids" ]; then
    echo "[DEBUG] BIDS directory ${TEST_DIR}/bids exists."
else
    echo "[DEBUG] BIDS directory ${TEST_DIR}/bids does not exist. Downloading dependencies..."
    
    # Use a separate lock for pip install to prevent concurrent pip operations
    PIP_LOCK="${TEST_DIR}/pip.lock"
    exec 201>"${PIP_LOCK}"
    if flock -n 201; then
        echo "[DEBUG] Installing Python dependencies..."
        pip install qsm-forward==0.22 osfclient
    else
        echo "[DEBUG] Waiting for pip operations to complete..."
        flock 201
        echo "[DEBUG] Pip lock released, continuing..."
    fi
    
    # Validate OSF credentials before attempting download
    if [ -z "${OSF_USERNAME}" ] && [ -z "${OSF_TOKEN}" ]; then
        echo "[ERROR] Neither OSF_USERNAME nor OSF_TOKEN is set. Cannot download data."
        echo "[ERROR] Please ensure GitHub secrets are properly configured."
        exit 1
    fi
    
    echo "[DEBUG] Pulling head phantom data from OSF..."
    echo "[DEBUG] OSF_USERNAME=${OSF_USERNAME:-(empty)}"
    echo "[DEBUG] OSF_TOKEN is ${OSF_TOKEN:+set}${OSF_TOKEN:-not set}"
    echo "[DEBUG] OSF_PASSWORD is ${OSF_PASSWORD:+set}${OSF_PASSWORD:-not set}"
    
    # Construct the osf command based on available credentials
    OSF_CMD="osf --project 9jc42"
    
    # Only add username if it's not empty
    if [ -n "${OSF_USERNAME}" ]; then
        OSF_CMD="${OSF_CMD} --username \"${OSF_USERNAME}\""
    fi
    
    OSF_CMD="${OSF_CMD} fetch data.tar \"${TEST_DIR}/data.tar\""
    
    echo "[DEBUG] Running: ${OSF_CMD}"
    
    # Export environment variables explicitly before running osf
    export OSF_TOKEN="${OSF_TOKEN}"
    export OSF_PASSWORD="${OSF_PASSWORD}"
    export OSF_USERNAME="${OSF_USERNAME}"
    
    # Run the command with eval to handle the quoted parameters correctly
    eval ${OSF_CMD}
    
    if [ $? -ne 0 ]; then
        echo "[ERROR] Failed to download data from OSF"
        exit 1
    fi

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

# The lock is automatically released when the script exits and file descriptor 200 is closed
echo "[DEBUG] Setup complete. Lock will be released on exit."
