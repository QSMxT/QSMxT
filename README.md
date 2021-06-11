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

If you use QSMxT for a study, please cite https://doi.org/10.1101/2021.05.05.442850.

![QSMxT Process Diagram](diagram.png)

## Installation
### Simple install and start via VNM

A user friendly way of running QSMxT in Windows, Mac or Linux is via the Virtual Neuro Machine (VNM) provided by the NeuroDesk project:

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
    git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_1.1.2_20210611
    cd qsmxt_1.1.2_20210611
    ./run_transparent_singularity.sh --container qsmxt_1.1.2_20210611.simg
    ```

3. Clone the QSMxT repository:
    ```bash
    git clone https://github.com/QSMxT/QSMxT.git
    ```

4. Invoke QSMxT scripts directly (see usage instructions in section below). Use the `--pbs` flag with your account string to run on an HPC supporting PBS.

### Docker container

There is also a docker image available:

```
docker run -it vnmd/qsmxt_1.1.2:20210611
```

## QSMxT Usage
1. Convert DICOM data to BIDS:
    ```bash
    python3 /opt/QSMxT/run_0_dicomSort.py REPLACE_WITH_YOUR_DICOM_INPUT_DATA_DIRECTORY 00_dicom
    python3 /opt/QSMxT/run_1_dicomToBids.py 00_dicom 01_bids
    ```
After this step check if the data were correctly recognized and converted to BIDS. Otherwise make a copy of /opt/QSMxT/bidsmap.yaml - adjust based on provenance example in 01_bids/code/bidscoin/bidsmap.yaml (see for example what it detected under extra_files) - and run again with the parameter `--heuristic bidsmap.yaml`. If the data were acquired on a GE scanner the complex data needs to be corrected by applying an FFT shift, this can be done with `python /opt/QSMxT/run_1_fixGEphaseFFTshift.py 01_bids/sub*/ses*/anat/*_run-1_*.nii.gz` . 

2. Run QSM pipeline:
    ```bash
    python3 /opt/QSMxT/run_2_qsm.py 01_bids 02_qsm_output
    ```
3. Segment data (T1 and GRE):
    ```bash
    python3 /opt/QSMxT/run_3_segment.py 01_bids 03_segmentation
    ```
4. Build magnitude and QSM group template (only makes sense when you have more than about 30 participants):
    ```bash
    python3 /opt/QSMxT/run_4_template.py 01_bids 02_qsm_output 04_template
    ```
5. Export quantitative data to CSV using segmentations
    ```bash
    python3 /opt/QSMxT/run_6_analysis.py --labels_file /opt/QSMxT/aseg_labels.csv --segmentations 03_segmentation/qsm_segmentations/*.nii --qsm_files 02_qsm_output/qsm_final/*/*.nii --out_dir 06_analysis
    ```
6. Export quantitative data to CSV using a custom segmentation
    ```bash
    python3 /opt/QSMxT/run_6_analysis.py --segmentations my_segmentation.nii --qsm_files 04_qsm_template/qsm_transformed/*/*.nii --out_dir 07_analysis
    ```

## Common errors and workarounds
1. Return code: 137

If you run ` python3 /opt/QSMxT/run_2_qsm.py 01_bids 02_qsm_output` and you get this error:
```
Resampling phase data...
Killed
Return code: 137
``` 
This indicates insufficient memory for the pipeline to run. Check in your Docker settings if you provided sufficent RAM to your containers (e.g. a 0.75mm dataset requires around 20GB of memory)

2. RuntimeError: Insufficient resources available for job
This also indicates that there is not enough memory for the job to run. Try limiting the CPUs to about 6GB RAM per CPU 