# QSMxT: A Complete QSM Processing and Analysis Pipeline

![QSMxT Process Diagram](https://qsmxt.github.io/images/qsmxt-process-diagram.png)

QSMxT is an end-to-end software toolbox for QSM that excels at automatically reconstructing and processing large groups of participants using sensible defaults.

QSMxT produces:

 - Quantative Susceptibility Maps (QSM)
 - Anatomical segmentations in both the GRE/QSM and T1w spaces
 - Spreadsheets in CSV format with susceptibility statistics across brain regions of interest
 - A group space/template, including average QSM and GRE images across your cohort

QSMxT requires gradient-echo MRI images converted to the Brain Imaging Data Structure (BIDS). QSMxT also includes tools to convert DICOM or NIfTI images to BIDS.

## Installation
### Quickstart via Neurodesk

QSMxT can be accessed via [Neurodesk](https://neurodesk.org/), including for free without any installation via [Neurodesk Play](https://play.neurodesk.org/). Once started, QSMxT is available in Neurodesk's module system and via the applications menu.

#### Updating QSMxT in Neurodesk

To use the latest version of QSMxT within an older version of Neurodesk, use:

```
bash /neurocommand/local/fetch_and_run.sh qsmxt 3.2.1 20230818
```

### Docker container

If you prefer to use a Docker container, the following commands will install QSMxT locally:

**Windows:**
```
docker run -it -v C:/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_3.2.1:20230818
```

**Linux/Mac:**
```
docker run -it -v ~/neurodesktop-storage:/neurodesktop-storage vnmd/qsmxt_3.2.1:20230818
```

## QSMxT Usage

### Data conversion

QSMxT requires data conforming to the Brain Imaging Data Structure (BIDS). 

Use `dicom-sort` and `dicom-convert` to convert DICOMs to BIDS:
```bash
dicom-sort YOUR_DICOM_DIR/ dicoms-sorted/
dicom-convert dicoms-sorted/ bids/
```
Carefully read the output to ensure data were correctly recognized and converted. Crucially, the `dicom-convert` script needs to know which of your acquisitions are T2*-weighted and suitable for QSM, as well as which are T1-weighted and suitable for segmentation. It identifies this based on the DICOM `ProtocolName` field and looks for the patterns `*qsm*` and `*t2starw*` for T2*-weighted series and `t1w` for T1-weighted series. You can specify your own patterns using command-line arguments e.g.:

```bash
dicom-convert dicoms-sorted/ bids/ --t2starw_protocol_patterns '*gre*' --t1w_protocol_patterns '*mp2rage*'
```

To convert NIfTI to BIDS, use `nifti-convert`:
```bash
nifti-convert YOUR_NIFTI_DIR/ bids/
```
Carefully read the output to ensure data were correctly recognized and converted. The script will try to identify any important details from the filenames and from adjacent JSON header files, if available. It retrieves this information using customisable patterns and regular expressions which can be overridden using command-line arguments (see the output using the `--help` flag). If any information is missing, you will be prompted to fill out a CSV spreadsheet with the missing information before running the conversion script again using the same command. You can open the CSV file in a spreadsheet reader such as Microsoft Excel or LibreOffice Calc.

### Running QSMxT

Run the following to start QSMxT and interactively choose your pipeline settings:

```bash
qsmxt bids/ output_dir/
```

By default, QSMxT runs interactively to make choosing pipeline settings straightforward. 

If you wish to run QSMxT non-interactively, you may specify all settings via command-line arguments and run non-interactively via `--auto_yes`. For help with building the one-line command, start QSMxT interactively first. Before the pipeline runs, it will display the one-line command such as:

```bash
qsmxt bids/ output_dir/ --do_qsm --premade fast --do_segmentations --auto_yes
```

This example will run QSMxT non-interactively and produce QSM using the fast pipeline and segmentations.

### HPC installation via Transparent Singularity

The tools provided by the QSMxT container can be exposed and used using the QSMxT Singularity container coupled with the transparent singularity software provided by the Neurodesk project. Transparent singularity allows the QSMxT Python scripts to be run directly within the host OS's environment. This mode of execution is necessary for parallel execution via PBS.

1. Install [singularity](https://sylabs.io/guides/3.0/user-guide/quick_start.html)
   
2. Install the QSMxT container via [transparent singularity](https://github.com/neurodesk/transparent-singularity):

    ```bash
    git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_3.2.1_20230818
    cd qsmxt_3.2.1_20230818
    ./run_transparent_singularity.sh --container qsmxt_3.2.1_20230818.simg
    source activate_qsmxt_3.2.1_20230818.simg.sh
    ```
    
    - **NOTE:** You must have sufficient storage available in `$SINGULARITY_TMPDIR` (by default `/tmp`), `$SINGULARITY_CACHEDIR` (by default `$HOME/.singularity/cache`), and the repository directory to store the QSMxT container.

3. Clone the QSMxT repository:
    ```bash
    git clone https://github.com/QSMxT/QSMxT.git
    ```

4. Install miniconda with QSMxT:
```bash
wget https://repo.anaconda.com/miniconda/Miniconda3-4.7.12.1-Linux-x86_64.sh	
bash Miniconda3-4.7.12.1-Linux-x86_64.sh -b
source ~/.bashrc
conda create -n qsmxt python=3.8
conda activate qsmxt
pip install qsmxt
```

5. Invoke QSMxT python commands directly (see QSMxT Usage above). Use the `--pbs` and `--slurm` flags with your account string and group to run on an HPCs supporting PBS and SLURM.

### Bare metal installation

We recommend the use of software containers for reproducibility and ease-of-use. However, QSMxT can be installed manually. Please see the detailed instructions used to generate the container [here](https://github.com/NeuroDesk/neurocontainers/blob/master/recipes/qsmxt/build.sh).


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

