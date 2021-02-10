We developed an open-source QSM processing framework, QSMxT, that provides a full QSM workflow including converting DICOM data to BIDS, a variety of robust masking strategies, phase unwrapping, background field correction, dipole inversion and region-of-interest analyses based on automated anatomical segmentations. We make all required external dependencies available in a reproducible and scalable analysis environment enabling users to process QSM data for large groups of participants on any operating system in a robust way. 

# 1) Install and start
## Standard install using VNM virtual desktop (Windows/Mac/Linux):
1. Install VNM: https://github.com/NeuroDesk/vnm/
2. Start QSMxT from the applications menu in the VNM desktop

## Alternate install for native linux (no virtual desktop):
1. Install singularity: https://sylabs.io/guides/3.7/user-guide/quick_start.html

2. Download QSMxT singularity image:

   ```
   curl <url> -O
   ```
   
   or
   
   ```
   wget <mirror>
	```
	
- Australian Mirror: https://swift.rc.nectar.org.au:8888/v1/AUTH_d6165cc7b52841659ce8644df1884d5e/singularityImages/qsmxt_1.0.0_20210122.simg  
	- US Mirror: https://objectstorage.us-ashburn-1.oraclecloud.com/n/nrrir2sdpmdp/b/neurodesk/o/qsmxt_1.0.0_20210122.simg
	- European Mirror: https://objectstorage.eu-zurich-1.oraclecloud.com/n/nrrir2sdpmdp/b/neurodesk/o/qsmxt_1.0.0_20210122.simg
	
3. Run singularity image

    ```bash
    singularity shell qsmxt_1.0.0_20210122.simg

    # alternative launch to mount additional data directories:
    singularity shell -B /data:/data qsmxt_1.0.0_20210122.simg
    ```

# 2) QSMxT Usage
1. Convert Dicom data to BIDS:
    ```bash
    python3 /opt/QSMxT/run_0_dicomSort.py REPLACE_WITH_YOUR_DICOM_INPUT_DATA_DIRECTORY 00_dicom
    python3 /opt/QSMxT/run_1_dicomToBids.py 00_dicom 01_bids
    ```
2. Run QSM pipeline:
    ```bash
    python3 /opt/QSMxT/run_2_qsm.py 01_bids 02_qsm_output

    #alternative when starting from custom bids structure:
    python3 /opt/QSMxT/run_2_qsm.py 01_bids 02_qsm_output --input_magnitude_pattern swi/*mag*.nii* --input_phase_pattern swi/*phase*.nii*
    ```
3. Segment data (T1 and GRE):
    ```bash
    python3 /opt/QSMxT/run_3_segment.py 01_bids 03_segmentation
    ```
4. Build magnitude group template:
    ```bash
    python3 /opt/QSMxT/run_4_magnitudeTemplate.py 01_bids 04_magnitude_template
    ```
5. Build QSM group template:
    ```bash
    python3 /opt/QSMxT/run_5_qsmTemplate.py 02_qsm_output 04_magnitude_template 05_qsm_template
    ```

# Parallel processing via PBS

On a high-performance compute system (HPC), PBS can be used instead of MultiProc for execution of `run_2_qsm.py`, `run_3_segment.py`, `run_4_magnitudeTemplate.py` and `run_5_qsmTemplate.py` for much greater parallelisation. However, PBS commands cannot be reliably invoked from inside the container, and so this requires execution from the HPC's native environment. To achieve this, a different install and run process is required via [transparent-singularity](https://github.com/CAIsr/transparent-singularity).

Follow the [instructions](https://github.com/CAIsr/transparent-singularity) to install the latest QSMxT container via transparent-singularity.

Clone the QSMxT repository:
```bash
git clone https://github.com/QSMxT/QSMxT.git
```

Invoke QSMxT scripts directly, and use the `--pbs` flag along with your PBS account string. 
```bash
cd QSMxT
python3 run_2_qsm.py bids qsm --pbs ACCOUNT_STRING
```
