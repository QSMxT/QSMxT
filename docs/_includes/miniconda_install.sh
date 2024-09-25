#Install Miniforge in the current directory. Make sure this directory has enough free space. (skip if conda is already installed)
wget https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-Linux-x86_64.sh
bash Miniforge3-Linux-x86_64.sh -b -p $PWD/miniforge3
source $PWD/miniforge3/etc/profile.d/conda.sh
export PATH=$PWD/miniforge3/bin/:${PATH}

#create conda environment for qsmxt
conda create -n qsmxt python=3.8
conda activate qsmxt
pip install qsmxt==${PROD_PACKAGE_VERSION}
