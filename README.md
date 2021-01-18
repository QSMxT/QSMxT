We developed an open-source QSM processing framework, QSMxT, that provides a full QSM workflow including converting DICOM data to BIDS, a variety of robust masking strategies, phase unwrapping, background field correction, dipole inversion and region-of-interest analyses based on automated anatomical segmentations. We make all required external dependencies available in a reproducible and scalable analysis environment enabling users to process QSM data for large groups of participants on any operating system in a robust way. 

# 1) install and start
Windows/Mac:
- https://github.com/NeuroDesk/vnm/
- start QSMxT from Applications menu

Linux:
- singularity: https://sylabs.io/guides/3.7/user-guide/quick_start.html
- Australian Mirror: 
```
singularity shell https://swift.rc.nectar.org.au:8888/v1/AUTH_d6165cc7b52841659ce8644df1884d5e/singularityImages/qsmxt_1.0.0_20210113.simg
```
- US Mirror: 
```
singularity shell  https://objectstorage.us-ashburn-1.oraclecloud.com/n/nrrir2sdpmdp/b/neurodesk/o/qsmxt_1.0.0_20210113.simg
```
- European Mirror: 
```
singularity shell https://objectstorage.eu-zurich-1.oraclecloud.com/n/nrrir2sdpmdp/b/neurodesk/o/qsmxt_1.0.0_20210113.simg
```

# 2) run
Convert Dicom data to BIDS:
```
python3 /opt/QSMxT/run_0_dicomSort.py REPLACE_WITH_YOUR_DICOM_INPUT_DATA_DIRECTORY 00_dicom
python3 /opt/QSMxT/run_1_dicomToBids.py 00_dicom 01_bids
```
Run QSM pipeline:
```
python3 /opt/QSMxT/run_2_nipype_qsm.py 01_bids 02_qsm_output
```
Segment data (T1 and GRE):
```
python3 /opt/QSMxT/run_3_nipype_segment.py 01_bids 03_segmentation
```
Build magnitude group template:
```
python3 /opt/QSMxT/run_4_magnitude_template.py 01_bids 04_magnitude_template
```
Build QSM group template:
```
python3 /opt/QSMxT/run_5_qsm_template.py 02_qsm_output 04_magnitude_template 05_qsm_template
```

# 3) What if I get illegal instruction error?
Then the processor is not compatible and tgv_qsm needs to be compiled on the host and then used inside the image instead of the precompiled version:

on the host run and install in directory (e.g. /data/SOFTWARE):
```
wget https://repo.anaconda.com/miniconda/Miniconda2-4.6.14-Linux-x86_64.sh
bash Miniconda2-4.6.14-Linux-x86_64.sh 
conda install -c anaconda cython==0.25.2
conda install numpy
conda install pyparsing
pip install scipy==0.17.1 nibabel==2.1.0
wget http://www.neuroimaging.at/media/qsm/TGVQSM-plus.zip
unzip TGVQSM-plus.zip
cd TGVQSM-master-011045626121baa8bfdd6633929974c732ae35e3
python setup.py install
```

Start a singularity shell with the software diretory bound:
```
singularity shell -B /data:/data qsmxt_1.0.0_20210113.simg
```

and overwrite the PATH:
```
export PATH=/data/SOFTWARE/deepQSM/tgv_qsm/miniconda2/bin:$PATH
```

check (it should use the tgv_qsm binary outside the container, not in /miniconda2/...):
```
which tgv_qsm
```