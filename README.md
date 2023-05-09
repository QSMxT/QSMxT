# QSMxT: A Complete QSM Processing and Analysis Pipeline

![QSMxT Process Diagram](https://qsmxt.github.io/images/qsmxt-process-diagram.png)

QSMxT is an end-to-end software toolbox for QSM that excels at automatically reconstructing and processing QSM across large groups of participants using sensible defaults.

QSMxT provides pipelines implemented in Python that:

1. Automatically convert unorganised DICOM or NIfTI data to the Brain Imaging Data Structure (BIDS)
2. Automatically reconstruct QSM, including steps for:
   1. Masking
   2. Phase unwrapping
   3. Background field removal
   4. Dipole inversion
   5. Multi-echo combination
3. Automatically generate a common group space for the cohort, as well as average magnitude and QSM images that facilitate group-level analyses.
4. Automatically segment T1w data and register them to the QSM space to extract quantitative values in anatomical regions of interest.
5. Export quantitative data to CSV for all subjects using the automated segmentations, or a custom segmentation in the group space (we recommend [ITK-SNAP](http://www.itksnap.org/pmwiki/pmwiki.php) to perform manual segmenations).

For a list of algorithms QSMxT uses, see the [Reference List](#references-and-algorithm-list).

QSMxT's containerised implementation via Docker and Singularity makes all required external dependencies available in a reproducible and scalable way, supporting MacOS, Windows and Linux, and with options for parallel processing via multiple processors, or via HPC systems using the Singularity container. QSMxT is also available on [Neurodesk](https://neurodesk.org), which makes the Singularity container available from the applications menu without installing anything. Neurodesk containers such as QSMxT can be pulled into Google Colab to write and share reproducible QSM notebooks ([example](https://bit.ly/qsmxt)).

If you use QSMxT for a study, please cite https://onlinelibrary.wiley.com/doi/10.1002/mrm.29048 (or the preprint https://doi.org/10.1101/2021.05.05.442850), along with the list of citations provided in the `references.txt` file that is created alongside the QSMxT outputs.


## Installation
### Install and start via Neurodesk project

A user friendly way of running QSMxT in Windows, Mac or Linux is via the Neurodesk project:

1. Install [Docker](https://www.docker.com/)
2. Install [Neurodesktop](https://neurodesk.github.io)
3. Run the Neurodesktop container and access the interface through your browser
4. Start QSMxT from the applications menu in the desktop
   (*Neurodesk* > *Quantitative Imaging* > *qsmxt*)
3. Follow the QSMxT usage instructions in the section below. Note that the `/neurodesktop-storage` folder is shared with the host OS for data sharing purposes (usually in `~/neurodesktop-storage` or `C:/neurodesktop-storage`). Begin by copying your DICOM data (or NIfTI data) into a folder in this directory on the host OS, then reach the folder by entering `cd /neurodesktop-storage` into the QSMxT window.

#### Updating QSMxT within Neurodesk

To use the latest version of the QSMxT container within an older version of Neurodesk, use:

```
bash /neurocommand/local/fetch_and_run.sh qsmxt 2.1.0 20230509
```

### Docker container

There is also a docker image available:

For Windows:
```
docker run -it -v C:/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_2.1.0:20230509
```
For Linux/Mac:
```
docker run -it -v ~/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_2.1.0:20230509
```

## QSMxT Usage
1. Convert DICOM or NIfTI data to BIDS:
    ```bash
    # DICOM TO BIDS (recommended)
    run_0_dicomSort.py REPLACE_WITH_YOUR_DICOM_INPUT_DATA_DIRECTORY 00_dicom
    run_1_dicomConvert.py 00_dicom 01_bids

    # NIFTI TO BIDS (if DICOMs are not available)
    run_1_niftiConvert.py REPLACE_WITH_YOUR_NIFTI_INPUT_DATA_DIRECTORY 01_bids
    ```
    - If converting from DICOMs, carefully read the output of the `run_1_dicomConvert.py` script to ensure data were correctly recognized and converted. You can also pass command line arguments to identify the acquisition protocol names, e.g. `run_1_dicomConvert.py 00_dicom 01_bids --t2starw_protocol_patterns *gre* --t1w_protocol_patterns *mp2rage*`.

    - If converting from NIfTI, carefully read the output of the `run_1_niftiConvert.py` script to ensure data were correctly recognized and converted. The script will try to identify any important details from the filenames and from adjacent JSON header files, if available. It retrieves this information using customisable patterns and regular expressions which can be overridden using command-line arguments (see the output using the `--help` flag). If any information is missing, you will be prompted to fill out a CSV spreadsheet with the missing information before running the conversion script again using the same command. You can open the CSV file in a spreadsheet reader such as Microsoft Excel or LibreOffice Calc.

2. Run QSM pipeline:
    ```bash
    run_2_qsm.py 01_bids 02_qsm_output
    ```
3. Segment data (T1 and GRE):
    ```bash
    run_3_segment.py 01_bids 03_segmentation
    ```
4. Build magnitude and QSM group template (only makes sense when you have more than about 30 participants):
    ```bash
    run_4_template.py 01_bids 02_qsm_output 04_template
    ```
5. Export quantitative data to CSV using segmentations
    ```bash
    run_5_analysis.py --labels_file /opt/QSMxT/aseg_labels.csv --segmentations 03_segmentation/qsm_segmentations/*.nii --qsm_files 02_qsm_output/qsm_final/*/*.nii --out_dir 06_analysis
    ```
6. Export quantitative data to CSV using a custom segmentation
    ```bash
    run_5_analysis.py --segmentations my_segmentation.nii --qsm_files 04_qsm_template/qsm_transformed/*/*.nii --out_dir 07_analysis
    ```

## Common errors and workarounds
1. Return code: 137

If you run `run_2_qsm.py 01_bids 02_qsm_output` and you get this error:
```
Resampling phase data...
Killed
Return code: 137
``` 
This indicates insufficient memory for the pipeline to run. Check in your Docker settings if you provided sufficent RAM to your containers (e.g. a 0.75mm dataset requires around 20GB of memory)

2. RuntimeError: Insufficient resources available for job
This also indicates that there is not enough memory for the job to run. Try limiting the CPUs to about 6GB RAM per CPU. You can try inserting the option `--n_procs 1` into the commands to limit the processing to one thread, e.g.:
```bash
run_2_qsm.py 01_bids 02_qsm_output --n_procs 1
```

3. If you are getting the error "Insufficient memory to run QSMxT (xxx GB available; 6GB needed)
This means there is not enough memory available. Troubleshoot advice when running this via Neurodesk is here: https://neurodesk.github.io/docs/neurodesktop/troubleshooting/#i-got-an-error-message-x-killed-or-not-enough-memory

### Linux installation via Transparent Singularity (supports PBS and High Performance Computing)

The tools provided by the QSMxT container can be exposed and used using the QSMxT Singularity container coupled with the transparent singularity software provided by the Neurodesk project. Transparent singularity allows the QSMxT Python scripts to be run directly within the host OS's environment. This mode of execution is necessary for parallel execution via PBS.

1. Install [singularity](https://sylabs.io/guides/3.0/user-guide/quick_start.html)
   
2. Install the QSMxT container via [transparent singularity](https://github.com/neurodesk/transparent-singularity):

    ```bash
    git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_2.1.0_20230509
    cd qsmxt_2.1.0_20230509
    ./run_transparent_singularity.sh --container qsmxt_2.1.0_20230509.simg
    source activate_qsmxt_2.1.0_20230509.simg.sh
    ```
    
    - **NOTE:** You must have sufficient storage available in `$SINGULARITY_TMPDIR` (by default `/tmp`), `$SINGULARITY_CACHEDIR` (by default `$HOME/.singularity/cache`), and the repository directory to store the QSMxT container.

3. Clone the QSMxT repository:
    ```bash
    git clone https://github.com/QSMxT/QSMxT.git
    ```

4. Install miniconda with nipype:
```bash
wget https://repo.anaconda.com/miniconda/Miniconda3-4.7.12.1-Linux-x86_64.sh	
bash Miniconda3-4.7.12.1-Linux-x86_64.sh -b
source ~/.bashrc
conda create -n qsmxt python=3.8
conda activate qsmxt
pip install psutil datetime networkx==2.8.8 nipype nibabel nilearn scipy scikit-image pydicom osfclient pytest seaborn git+https://github.com/astewartau/cloudstor.git
```

5. Invoke QSMxT python scripts directly (see QSMxT Usage above). Use the `--pbs` flag with your account string to run on an HPC supporting PBS.

### Bare metal installation
Although we do not recommend installing the dependencies manually and we advocate the use of software containers for reproducibility and ease-of-use, you can install everything by hand. These are the dependencies required and this was tested in Ubuntu 18.04: 

You need:
- TGV-QSM v1.0 running in miniconda 2
- bet2 (https://github.com/liangfu/bet2)
- ANTs version=2.3.4
- dcm2niix (https://github.com/rordenlab/dcm2niix)
- miniconda version=4.7.12.1 with python3.8 and pip packages psutil, datetime, nipype, nibabel, nilearn, scipy, scikit-image, pydicom, osfclient, cloudstor (https://github.com/astewartau/cloudstor), pytest and seaborn
- FastSurfer (https://github.com/Deep-MI/FastSurfer.git)
- Bru2Nii v1.0.20180303 (https://github.com/neurolabusc/Bru2Nii/releases/download/v1.0.20180303/Bru2_Linux.zip)
- julia-1.6.1 with ArgParse, MriResearchTools, QSM.jl, FFTW and RomeoApp (see https://github.com/korbinian90/RomeoApp.jl)

Here is the detailed instruction that you could replicate: https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxtbase/build.sh and then on top https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxt/build.sh


# References and algorithm list

## QSM pipeline

- **QSMxT:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **Two-pass Artefact Reduction Algorithm:** Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048
- **Inhomogeneity correction:** Eckstein K, Trattnig S, Simon DR. A Simple homogeneity correction for neuroimaging at 7T. In: Proc. Intl. Soc. Mag. Reson. Med. International Society for Magnetic Resonance in Medicine; 2019. Abstract 2716. https://index.mirasmart.com/ISMRM2019/PDFfiles/2716.html
- **Masking algorithm - BET:** Smith SM. Fast robust automated brain extraction. Human Brain Mapping. 2002;17(3):143-155. doi:10.1002/hbm.10062
- **Masking algorithm - BET:** Liangfu Chen. liangfu/bet2 - Standalone Brain Extraction Tool. GitHub; 2015. https://github.com/liangfu/bet2
- **Threshold selection algorithm - gaussian:** Balan AGR, Traina AJM, Ribeiro MX, Marques PMA, Traina Jr. C. Smart histogram analysis applied to the skull-stripping problem in T1-weighted MRI. Computers in Biology and Medicine. 2012;42(5):509-522. doi:10.1016/j.compbiomed.2012.01.004
- **Threshold selection algorithm - Otsu:** Otsu, N. (1979). A threshold selection method from gray-level histograms. IEEE transactions on systems, man, and cybernetics, 9(1), 62-66. doi:10.1109/TSMC.1979.4310076
- **Unwrapping algorithm - Laplacian:** Schofield MA, Zhu Y. Fast phase unwrapping algorithm for interferometric applications. Optics letters. 2003 Jul 15;28(14):1194-6. doi:10.1364/OL.28.001194
- **Unwrapping algorithm - ROMEO:** Dymerska B, Eckstein K, Bachrata B, et al. Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO). Magnetic Resonance in Medicine. 2021;85(4):2294-2308. doi:10.1002/mrm.28563
- **Background field removal - V-SHARP:** Wu B, Li W, Guidon A et al. Whole brain susceptibility mapping using compressed sensing. Magnetic resonance in medicine. 2012 Jan;67(1):137-47. doi:10.1002/mrm.23000
- **Background field removal - PDF:** Liu, T., Khalidov, I., de Rochefort et al. A novel background field removal method for MRI using projection onto dipole fields. NMR in Biomedicine. 2011 Nov;24(9):1129-36. doi:10.1002/nbm.1670
- **QSM algorithm - NeXtQSM:** Cognolato, F., O'Brien, K., Jin, J. et al. (2022). NeXtQSM—A complete deep learning pipeline for data-consistent Quantitative Susceptibility Mapping trained with hybrid data. Medical Image Analysis, 102700. doi:10.1016/j.media.2022.102700
- **QSM algorithm - RTS:** Kames C, Wiggermann V, Rauscher A. Rapid two-step dipole inversion for susceptibility mapping with sparsity priors. Neuroimage. 2018 Feb 15;167:276-83. doi:10.1016/j.neuroimage.2017.11.018
- **QSM algorithm - TV:** Bilgic B, Fan AP, Polimeni JR, Cauley SF, Bianciardi M, Adalsteinsson E, Wald LL, Setsompop K. Fast quantitative susceptibility mapping with L1‐regularization and automatic parameter selection. Magnetic resonance in medicine. 2014 Nov;72(5):1444-59
- **QSM algorithm - TGV-QSM:** Langkammer C, Bredies K, Poser BA, et al. Fast quantitative susceptibility mapping using 3D EPI and total generalized variation. NeuroImage. 2015;111:622-630. doi:10.1016/j.neuroimage.2015.02.041
- **MriResearchTools package:** Eckstein K. korbinian90/MriResearchTools.jl. GitHub; 2022. https://github.com/korbinian90/MriResearchTools.jl
- **Nibabel package:** Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel
- **Scipy package:** Virtanen P, Gommers R, Oliphant TE, et al. SciPy 1.0: fundamental algorithms for scientific computing in Python. Nat Methods. 2020;17(3):261-272. doi:10.1038/s41592-019-0686-2
- **Numpy package:** Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2
- **Nipype package:** Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013

## DICOM Sorting
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **Pipeline implementation:** Weston A. alex-weston-13/sort_dicoms.py. GitHub; 2020. https://gist.github.com/alex-weston-13/4dae048b423f1b4cb9828734a4ec8b83
- **Pydicom package:** Mason D, scaramallion, mrbean-bremen, et al. Pydicom/Pydicom: Pydicom 2.3.0. Zenodo; 2022. doi:10.5281/zenodo.6394735

## BIDS Conversion
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **dcm2niix software:** Li X, Morgan PS, Ashburner J, Smith J, Rorden C. The first step for neuroimaging data analysis: DICOM to NIfTI conversion. J Neurosci Methods. 2016;264:47-56. doi:10.1016/j.jneumeth.2016.03.001
- **BIDS:** Gorgolewski KJ, Auer T, Calhoun VD, et al. The brain imaging data structure, a format for organizing and describing outputs of neuroimaging experiments. Sci Data. 2016;3(1):160044. doi:10.1038/sdata.2016.44

## Segmentation pipeline
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **FastSurfer:** Henschel L, Conjeti S, Estrada S, Diers K, Fischl B, Reuter M. FastSurfer - A fast and accurate deep learning based neuroimaging pipeline. NeuroImage. 2020;219:117012. doi:10.1016/j.neuroimage.2020.117012
- **Advanced Normalization Tools (ANTs)**: Avants BB, Tustison NJ, Johnson HJ. Advanced Normalization Tools. GitHub; 2022. https://github.com/ANTsX/ANTs
- **Nipype package:** Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013
- **Numpy package:** Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2
- **Nibabel package:** Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel

## Template-building pipeline
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
- **Advanced Normalization Tools (ANTs):** Avants BB, Tustison NJ, Johnson HJ. Advanced Normalization Tools. GitHub; 2022. https://github.com/ANTsX/ANTs")
- **Nipype package:** Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")

## Analysis pipeline
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
- **Nibabel package:** Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
- **Numpy package:** Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2")

