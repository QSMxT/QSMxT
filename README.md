We developed an open-source QSM processing framework, QSMxT, that provides a full QSM workflow including converting DICOM data to BIDS, a variety of robust masking strategies, phase unwrapping, background field correction, dipole inversion and region-of-interest analyses based on automated anatomical segmentations. We make all required external dependencies available in a reproducible and scalable analysis environment enabling users to process QSM data for large groups of participants on any operating system in a robust way. 

# 1) install and start
## Windows/Mac:
- https://github.com/NeuroDesk/vnm/
- start QSMxT from Applications menu

## Linux:
- singularity: https://sylabs.io/guides/3.7/user-guide/quick_start.html

### Download image
- Australian Mirror: 
```
curl https://swift.rc.nectar.org.au:8888/v1/AUTH_d6165cc7b52841659ce8644df1884d5e/singularityImages/qsmxt_1.0.0_20210122.simg -O
or
wget https://swift.rc.nectar.org.au:8888/v1/AUTH_d6165cc7b52841659ce8644df1884d5e/singularityImages/qsmxt_1.0.0_20210122.simg
```
- US Mirror: 
```
https://objectstorage.us-ashburn-1.oraclecloud.com/n/nrrir2sdpmdp/b/neurodesk/o/qsmxt_1.0.0_20210122.simg
```
- European Mirror: 
```
https://objectstorage.eu-zurich-1.oraclecloud.com/n/nrrir2sdpmdp/b/neurodesk/o/qsmxt_1.0.0_20210122.simg
```

### Run image
```bash
singularity shell qsmxt_1.0.0_20210122.simg

#alternatively mount additional data directories:
singularity shell -B /data:/data qsmxt_1.0.0_20210122.simg
```


# 2) run
Convert Dicom data to BIDS:
```bash
python3 /opt/QSMxT/run_0_dicomSort.py REPLACE_WITH_YOUR_DICOM_INPUT_DATA_DIRECTORY 00_dicom
python3 /opt/QSMxT/run_1_dicomToBids.py 00_dicom 01_bids
```
Run QSM pipeline:
```bash
python3 /opt/QSMxT/run_2_qsm.py 01_bids 02_qsm_output

#alternative when starting from custom bids structure:
python3 /opt/QSMxT/run_2_qsm.py 01_bids 02_qsm_output --input_magnitude_pattern swi/*mag*.nii* --input_phase_pattern swi/*phase*.nii*
```
Segment data (T1 and GRE):
```bash
python3 /opt/QSMxT/run_3_segment.py 01_bids 03_segmentation
```
Build magnitude group template:
```bash
python3 /opt/QSMxT/run_4_magnitudeTemplate.py 01_bids 04_magnitude_template
```
Build QSM group template:
```bash
python3 /opt/QSMxT/run_5_qsmTemplate.py 02_qsm_output 04_magnitude_template 05_qsm_template
```
