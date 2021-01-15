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
