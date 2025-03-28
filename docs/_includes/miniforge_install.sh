#Install Miniforge in the current directory. Make sure this directory has enough free space. (skip if conda is already installed)
wget https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-Linux-x86_64.sh
bash Miniforge3-Linux-x86_64.sh -b -p ${PROD_MINIFORGE_PATH}
source ${PROD_MINIFORGE_PATH}/etc/profile.d/conda.sh
export PATH=${PROD_MINIFORGE_PATH}/bin/:${PATH}

#create conda environment for qsmxt
conda create -n qsmxt python=${PROD_PYTHON_VERSION}
conda activate qsmxt
pip install qsmxt==${PROD_PACKAGE_VERSION}
