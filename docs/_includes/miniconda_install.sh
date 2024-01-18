wget https://repo.anaconda.com/miniconda/Miniconda3-4.7.12.1-Linux-x86_64.sh	
bash Miniconda3-4.7.12.1-Linux-x86_64.sh -b -p ${PROD_MINICONDA_PATH}
source ~/.bashrc
conda init bash
conda create -n qsmxt python=3.8
conda activate qsmxt
pip install qsmxt==${PROD_PACKAGE_VERSION}