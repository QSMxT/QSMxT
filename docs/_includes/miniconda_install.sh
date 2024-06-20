#Install Miniconda (skip if conda is already installed)
wget https://repo.anaconda.com/miniconda/Miniconda3-4.7.12.1-Linux-x86_64.sh	
bash Miniconda3-4.7.12.1-Linux-x86_64.sh -b -p ${PROD_MINICONDA_PATH}
source ${PROD_MINICONDA_PATH}/etc/profile.d/conda.sh
export PATH=${PROD_MINICONDA_PATH}/bin:${PATH}

#create conda environment for qsmxt
conda create -n qsmxt python=3.8
conda activate qsmxt
pip install qsmxt==${PROD_PACKAGE_VERSION}
