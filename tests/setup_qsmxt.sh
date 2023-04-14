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

echo "[DEBUG] Pulling QSMxT branch ${BRANCH}..."
git clone -b "${BRANCH}" "https://github.com/QSMxT/QSMxT.git" "/tmp/QSMxT"

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
echo "[DEBUG] Pulling QSMxT container ${container}..."
sudo docker pull "${container}" 

# tag it with 'qsmxt'
# Get the repository name and original tag
repository_name=$(echo "${container}" | cut -d ':' -f 1)
original_tag=$(echo "${container}" | cut -d ':' -f 2)
sudo docker tag "${container}" "${repository_name}:qsmxt"
sudo docker rmi "${container}"
docker images

