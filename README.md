# QSMxT: A Complete QSM Processing Framework

QSMxT is a complete and end-to-end QSM processing and analysis framework that excels at automatically reconstructing and processing QSM for large groups of participants. 

QSMxT provides pipelines implemented in Python that:

1. Automatically convert DICOM data to the Brain Imaging Data Structure (BIDS)
2. Automatically reconstruct QSM, including steps for:
   1. Robust masking without anatomical priors
   2. Phase unwrapping (Laplacian based)
   3. Background field removal + dipole inversion (`tgv_qsm`)
   4. Multi-echo combination
3. Automatically generate a common group space for the whole study, as well as average magnitude and QSM images that facilitate group-level analyses.
4. Automatically segment T1w data and register them to the QSM space to extract quantitative values in anatomical regions of interest.
5. Export quantitative data to CSV for all subjects using the automated segmentations, or a custom segmentation in the group space.

QSMxT's containerised implementation makes all required external dependencies available in a reproducible and scalable way, supporting MacOS, Windows and Linux, and with options for parallel processing via PBS systems.

![QSMxT Process Diagram](diagram.png)

## Installation
### Simple install and start via VNM

A user friendly way of running QSMxT in Windows is via the Virtual Neuro Machine (VNM) provided by the NeuroDesk project:

1. Install [Docker](https://www.docker.com/)
2. Install [VNM](https://github.com/NeuroDesk/vnm/)
3. Run the VNM container and open it in a browser window at http://localhost:6080/
4. Start QSMxT from the applications menu in the VNM desktop
   (*VNM Neuroimaging* > *Quantitative Imaging* > *qsmxt*)
3. Follow the QSMxT usage instructions in the section below. Note that the `/vnm` folder in VNM is shared with the host OS for data sharing purposes (usually in `~/vnm` or `C:/vnm`). Begin by copying your DICOM data into a folder in this directory on the host OS, then reach the folder in VNM by entering `cd /vnm` into the QSMxT window.

### Linux installation via Transparent Singularity (supports PBS)

The tools provided by the QSMxT container can be exposed and used without VNM using the QSMxT Singularity container coupled with the transparent singularity software provided by the Neurodesk project. Transparent singularity allows the QSMxT Python scripts to be run directly within the host OS's environment. This mode of execution is necessary for parallel execution via PBS.

1. Install [singularity](https://sylabs.io/guides/3.0/user-guide/quick_start.html)
   
2. Install the QSMxT container via [transparent singularity](https://github.com/neurodesk/transparent-singularity):

    ```bash
    git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_1.0.0_20210305
    cd qsmxt_1.0.0_20210305
    ./run_transparent_singularity.sh --container qsmxt_1.0.0_20210305.simg
    ```

3. Clone the QSMxT repository:
    ```bash
    git clone https://github.com/QSMxT/QSMxT.git
    ```

4. Invoke QSMxT scripts directly (see usage instructions in section below). Use the `--pbs` flag with your account string to run on an HPC supporting PBS.

### Docker container

There is also a docker image available:

```
docker run -it vnmd/qsmxt_1.0.0:20210305
```

## QSMxT Usage
1. Convert DICOM data to BIDS:
    ```bash
    python3 /opt/QSMxT/run_0_dicomSort.py REPLACE_WITH_YOUR_DICOM_INPUT_DATA_DIRECTORY 00_dicom
    python3 /opt/QSMxT/run_1_dicomToBids.py 00_dicom 01_bids
    ```
2. Run QSM pipeline:
    ```bash
    python3 /opt/QSMxT/run_2_qsm.py 01_bids 02_qsm_output
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
6. Export quantitative data to CSV using segmentations
    ```bash
    python3 /opt/QSMxT/run_6_analysis.py --labels_file /opt/QSMxT/aseg_labels.csv --segmentations 03_segmentation/qsm_segmentation/*.nii --qsm_files 02_qsm_output/qsm_final/*.nii --out_dir 06_analysis
    ```
7. Export quantitative data to CSV using a custom segmentation
    ```bash
    python3 /opt/QSMxT/run_6_analysis --segmentations my_segmentation.nii --qsm_files 05_qsm_template/qsm_transformed/*/*.nii --out_dir 07_analysis
    ```
