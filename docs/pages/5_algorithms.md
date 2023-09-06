---
layout: default
title: Algorithms
nav_order: 5
permalink: /algorithms
---

<head>
  <link rel="stylesheet" href="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/css/bootstrap.min.css">
  <script src="https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
  <script src="https://maxcdn.bootstrapcdn.com/bootstrap/3.4.1/js/bootstrap.min.js"></script>
</head>

# Algorithms

## How were the algorithms chosen?

Many QSM algorithms have been proposed in recent years, with each having unique advantages and disadvantages. However, most algorithms are written in languages that are difficult to automate across large and varied datasets, and/or require proprietary licensing. We chose algorithms implemented in languages that were possible to run within open-source and containerised environments.

## Can you include my preferred algorithm in QSMxT? 

If you are able to provide or point us to an implementation of a QSM algorithm in a language that can be run in a command-line environment, along with a justified use-case, we would gladly work with you to integrate it. Feel free to open an issue on GitHub with your request. We can also accept contributions in the form of pull requests to the GitHub repository if you are able to integrate it yourself. 

## Algorithms

### QSM Reconstruction

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

### DICOM Sorting
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **Pipeline implementation:** Weston A. alex-weston-13/sort_dicoms.py. GitHub; 2020. https://gist.github.com/alex-weston-13/4dae048b423f1b4cb9828734a4ec8b83
- **Pydicom package:** Mason D, scaramallion, mrbean-bremen, et al. Pydicom/Pydicom: Pydicom 2.3.0. Zenodo; 2022. doi:10.5281/zenodo.6394735

### DICOM to BIDS Conversion
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **dcm2niix software:** Li X, Morgan PS, Ashburner J, Smith J, Rorden C. The first step for neuroimaging data analysis: DICOM to NIfTI conversion. J Neurosci Methods. 2016;264:47-56. doi:10.1016/j.jneumeth.2016.03.001
- **BIDS:** Gorgolewski KJ, Auer T, Calhoun VD, et al. The brain imaging data structure, a format for organizing and describing outputs of neuroimaging experiments. Sci Data. 2016;3(1):160044. doi:10.1038/sdata.2016.44

### NIfTI to BIDS Conversion
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **BIDS:** Gorgolewski KJ, Auer T, Calhoun VD, et al. The brain imaging data structure, a format for organizing and describing outputs of neuroimaging experiments. Sci Data. 2016;3(1):160044. doi:10.1038/sdata.2016.44

### Segmentation
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT
- **FastSurfer:** Henschel L, Conjeti S, Estrada S, Diers K, Fischl B, Reuter M. FastSurfer - A fast and accurate deep learning based neuroimaging pipeline. NeuroImage. 2020;219:117012. doi:10.1016/j.neuroimage.2020.117012
- **Advanced Normalization Tools (ANTs)**: Avants BB, Tustison NJ, Johnson HJ. Advanced Normalization Tools. GitHub; 2022. https://github.com/ANTsX/ANTs
- **Nipype package:** Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013
- **Numpy package:** Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2
- **Nibabel package:** Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel

### Template-building
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
- **Advanced Normalization Tools (ANTs):** Avants BB, Tustison NJ, Johnson HJ. Advanced Normalization Tools. GitHub; 2022. https://github.com/ANTsX/ANTs")
- **Nipype package:** Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")

### Analysis
- **Pipeline implementation:** Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
- **Nibabel package:** Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
- **Numpy package:** Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2")

