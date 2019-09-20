PROJECT ID - Name

# 1) install dependencies
install fsl and tgv qsm using transparent singularity or local
follow instructions here https://github.com/CAIsr/transparent-singularity or try this:
```
git clone https://github.com/CAIsr/transparent-singularity.git tgvqsm_fsl_5p0p11_intel_20180730.simg
cd tgvqsm_fsl_5p0p11_intel_20180730.simg
./run_transparent_singularity.sh tgvqsm_fsl_5p0p11_intel_20180730.simg
```

setup a miniconda python environemnt e.g.
```
wget https://repo.continuum.io/miniconda/Miniconda3-latest-Linux-x86_64.sh
bash Miniconda3-latest-Linux-x86_64.sh
```

logout or source .bashrc
```
source ~/.bashrc
```

install heudiconv
```
pip install https://github.com/nipy/heudiconv/archive/master.zip
```

install dcm2niix
```
git clone https://github.com/rordenlab/dcm2niix.git
cd dcm2niix
mkdir build && cd build
cmake ..
make
```
add dcm2niix path to your path in .bashrc

install nipype
```
conda install --channel conda-forge nipype
```

# 2) run
- change heuristic.py file to match your protocol settings
- convert dicom to bids using heudiconv by running run_1_dicomConversionToBids.sh
- use nipype to run QSM pipeline by running run_2_nipype_qsm.py (adjust subject names inside the script)
