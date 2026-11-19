//! The works the methods text cites, and how each is cited inline.

pub(super) struct Citation {
    pub(super) key: &'static str,
    pub(super) text: &'static str,
}

pub(super) const CITE_MCPC3DS: Citation = Citation {
    key: "eckstein2018",
    text: "Eckstein, K., et al. (2018). \"Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE).\" *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963",
};

pub(super) const CITE_ROMEO: Citation = Citation {
    key: "dymerska2021",
    text: "Dymerska, B., et al. (2021). \"Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO).\" *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563",
};

pub(super) const CITE_LAPLACIAN_UNWRAP: Citation = Citation {
    key: "schofield2003",
    text: "Schofield, M.A., Zhu, Y. (2003). \"Fast phase unwrapping algorithm for interferometric applications.\" *Optics Letters*, 28(14):1194-1196. https://doi.org/10.1364/OL.28.001194",
};

pub(super) const CITE_PRELUDE: Citation = Citation {
    key: "jenkinson2003",
    text: "Jenkinson, M. (2003). \"Fast, automated, N-dimensional phase-unwrapping algorithm.\" *Magnetic Resonance in Medicine*, 49(1):193-197. https://doi.org/10.1002/mrm.10354",
};

pub(super) const CITE_UKB_QSM: Citation = Citation {
    key: "wang2022",
    text: "Wang, C., et al. (2022). \"Phenotypic and genetic associations of quantitative magnetic susceptibility in UK Biobank brain imaging.\" *Nature Neuroscience*, 25(6):818-831. https://doi.org/10.1038/s41593-022-01074-w",
};

pub(super) const CITE_VSHARP: Citation = Citation {
    key: "wu2012",
    text: "Wu, B., et al. (2012). \"Whole brain susceptibility mapping using compressed sensing.\" *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000",
};

pub(super) const CITE_SHARP: Citation = Citation {
    key: "schweser2011",
    text: "Schweser, F., et al. (2011). \"Quantitative imaging of intrinsic magnetic tissue properties using MRI signal phase.\" *NeuroImage*, 54(4):2789-2807. https://doi.org/10.1016/j.neuroimage.2010.10.070",
};

pub(super) const CITE_PDF: Citation = Citation {
    key: "liu2011pdf",
    text: "Liu, T., et al. (2011). \"A novel background field removal method for MRI using projection onto dipole fields.\" *NMR in Biomedicine*, 24(9):1129-1136. https://doi.org/10.1002/nbm.1670",
};

pub(super) const CITE_ISMV: Citation = Citation {
    key: "wen2014",
    text: "Wen, Y., et al. (2014). \"An iterative spherical mean value method for background field removal in MRI.\" *Magnetic Resonance in Medicine*, 72(4):1065-1071. https://doi.org/10.1002/mrm.24998",
};

pub(super) const CITE_LBV: Citation = Citation {
    key: "zhou2014",
    text: "Zhou, D., et al. (2014). \"Background field removal by solving the Laplacian boundary value problem.\" *NMR in Biomedicine*, 27(3):312-319. https://doi.org/10.1002/nbm.3064",
};

pub(super) const CITE_COSMOS: Citation = Citation {
    key: "liu2009cosmos",
    text: "Liu, T., Spincemaille, P., de Rochefort, L., Kressler, B., Wang, Y. (2009). \"Calculation of susceptibility through multiple orientation sampling (COSMOS): a method for conditioning the inverse problem from measured magnetic field map to susceptibility source image in MRI.\" *Magnetic Resonance in Medicine*, 61:196-204. https://doi.org/10.1002/mrm.21828",
};

pub(super) const CITE_STI: Citation = Citation {
    key: "liu2010sti",
    text: "Liu, C. (2010). \"Susceptibility tensor imaging.\" *Magnetic Resonance in Medicine*, 63:1471-1477. https://doi.org/10.1002/mrm.22482",
};

pub(super) const CITE_RTS: Citation = Citation {
    key: "kames2018",
    text: "Kames, C., Wiggermann, V., Rauscher, A. (2018). \"Rapid two-step dipole inversion for susceptibility mapping with sparsity priors.\" *NeuroImage*, 167:276-283. https://doi.org/10.1016/j.neuroimage.2017.11.018",
};

pub(super) const CITE_TV: Citation = Citation {
    key: "bilgic2014tv",
    text: "Bilgic, B., et al. (2014). \"Fast quantitative susceptibility mapping with L1-regularization and automatic parameter selection.\" *Magnetic Resonance in Medicine*, 72(5):1444-1459. https://doi.org/10.1002/mrm.25029",
};

pub(super) const CITE_TKD: Citation = Citation {
    key: "shmueli2009",
    text: "Shmueli, K., et al. (2009). \"Magnetic susceptibility mapping of brain tissue in vivo using MRI phase data.\" *Magnetic Resonance in Medicine*, 62(6):1510-1522. https://doi.org/10.1002/mrm.22135",
};

pub(super) const CITE_TIKHONOV: Citation = Citation {
    key: "bilgic2014l2",
    text: "Bilgic, B., et al. (2014). \"Fast image reconstruction with L2-regularization.\" *Journal of Magnetic Resonance Imaging*, 40(1):181-191. https://doi.org/10.1002/jmri.24365",
};

pub(super) const CITE_NLTV: Citation = Citation {
    key: "milovic2018",
    text: "Milovic, C., Bilgic, B., Zhao, B., Acosta-Cabronero, J., Tejos, C. (2018). \"Fast nonlinear susceptibility inversion with variational regularization.\" *Magnetic Resonance in Medicine*, 80(2):814-821. https://doi.org/10.1002/mrm.27073",
};

pub(super) const CITE_NDI: Citation = Citation {
    key: "polak2020",
    text: "Polak, D., et al. (2020). \"Nonlinear dipole inversion (NDI) enables robust quantitative susceptibility mapping (QSM).\" NMR in Biomedicine, 33(12):e4271.",
};

pub(super) const CITE_L1QSM: Citation = Citation {
    key: "milovic2022",
    text: "Milovic, C., et al. (2022). \"Comparison of parameter optimization methods for quantitative susceptibility mapping.\" Magnetic Resonance in Medicine, 87(3):1517-1531.",
};

pub(super) const CITE_WHQSM: Citation = Citation {
    key: "milovic2019",
    text: "Milovic, C., et al. (2019). \"Weak-harmonic regularization for quantitative susceptibility mapping.\" Magnetic Resonance in Medicine, 81(2):1399-1411.",
};

pub(super) const CITE_HDQSM: Citation = Citation {
    key: "lambert2022",
    text: "Lambert, M., et al. (2022). \"Hybrid data fidelity term approach for quantitative susceptibility mapping.\" Magnetic Resonance in Medicine, 88(4):1567-1583.",
};

pub(super) const CITE_AMPPE: Citation = Citation {
    key: "huang2023amppe",
    text: "Huang, S., et al. (2023). \"Quantitative susceptibility mapping using approximate message passing with parameter estimation.\" *Magnetic Resonance in Medicine*, 90(4):1414-1430. https://doi.org/10.1002/mrm.29725",
};

pub(super) const CITE_CHISEP: Citation = Citation {
    key: "shin2021chisep",
    text: "Shin, H.-G., et al. (2021). \"χ-separation: Magnetic susceptibility source separation toward iron and myelin mapping in the brain.\" *NeuroImage*, 240:118371. https://doi.org/10.1016/j.neuroimage.2021.118371",
};

pub(super) const CITE_R2STARQSM: Citation = Citation {
    key: "dimov2022",
    text: "Dimov, A. V., et al. (2022). \"Susceptibility source separation from gradient echo data using magnitude decay modeling.\" *Journal of Neuroimaging*, 32(4):852-859. https://doi.org/10.1111/jon.13014",
};

pub(super) const CITE_WAVESEP: Citation = Citation {
    key: "fang2023wavesep",
    text: "Fang, J., et al. (2023). \"Wavelet-based single-step susceptibility source separation (WaveSep).\" *Magnetic Resonance in Medicine* (χ-separation family).",
};

pub(super) const CITE_DECOMPOSE: Citation = Citation {
    key: "chen2021decompose",
    text: "Chen, J., et al. (2021). \"Decompose quantitative susceptibility mapping (QSM) to sub-voxel diamagnetic and paramagnetic components based on gradient-echo MRI data.\" *NeuroImage*, 242:118477. https://doi.org/10.1016/j.neuroimage.2021.118477",
};

pub(super) const CITE_HCCHISEP: Citation = Citation {
    key: "stewart2026",
    text: "Stewart, A., et al. (2026). \"Hollow-cylinder susceptibility source separation (HC-ChiSep).\"",
};

pub(super) const CITE_MEDI: Citation = Citation {
    key: "liu2011medi",
    text: "Liu, T., et al. (2011). \"Morphology enabled dipole inversion (MEDI) from a single-angle acquisition.\" *Magnetic Resonance in Medicine*, 66(3):777-783. https://doi.org/10.1002/mrm.22816",
};

pub(super) const CITE_MEDI0: Citation = Citation {
    key: "liu2018medi0",
    text: "Liu, Z., et al. (2018). \"MEDI+0: Morphology enabled dipole inversion with automatic uniform cerebrospinal fluid zero reference for quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 79(5):2795-2803. https://doi.org/10.1002/mrm.26946",
};

pub(super) const CITE_FIT_PPM_COMPLEX: Citation = Citation {
    key: "liu2013nonlinear",
    text: "Liu, T., et al. (2013). \"Nonlinear formulation of the magnetic field to source relationship for robust quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 69(2):467-476. https://doi.org/10.1002/mrm.24272",
};

pub(super) const CITE_TFI: Citation = Citation {
    key: "liu2017tfi",
    text: "Liu, Z., et al. (2017). \"Preconditioned total field inversion (TFI) method for quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 78(1):303-315. https://doi.org/10.1002/mrm.26331",
};

pub(super) const CITE_ILSQR: Citation = Citation {
    key: "li2015",
    text: "Li, W., et al. (2015). \"A method for estimating and removing streaking artifacts in quantitative susceptibility mapping.\" *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043",
};

pub(super) const CITE_STAR: Citation = Citation {
    key: "wei2015star",
    text: "Wei, H., Dibb, R., Zhou, Y., Sun, Y., Xu, J., Wang, N., Liu, C. (2015). \"Streaking artifact reduction for quantitative susceptibility mapping of sources with large dynamic range.\" *NMR in Biomedicine*, 28(10):1294-1303. https://doi.org/10.1002/nbm.3383",
};

pub(super) const CITE_TGV: Citation = Citation {
    key: "langkammer2015",
    text: "Langkammer, C., et al. (2015). \"Fast quantitative susceptibility mapping using 3D EPI and total generalized variation.\" *NeuroImage*, 111:622-630. https://doi.org/10.1016/j.neuroimage.2015.02.041",
};

pub(super) const CITE_QSMART: Citation = Citation {
    key: "yaghmaie2021",
    text: "Yaghmaie, N., Syeda, W., et al. (2021). \"QSMART: Quantitative Susceptibility Mapping Artifact Reduction Technique.\" *NeuroImage*, 231:117701. https://doi.org/10.1016/j.neuroimage.2020.117701",
};

pub(super) const CITE_N4: Citation = Citation {
    key: "tustison2010",
    text: "Tustison, N.J., Avants, B.B., Cook, P.A., Zheng, Y., Egan, A., Yushkevich, P.A., Gee, J.C. (2010). \"N4ITK: Improved N3 Bias Correction.\" *IEEE Transactions on Medical Imaging*, 29(6):1310-1320. https://doi.org/10.1109/TMI.2010.2046908",
};

pub(super) const CITE_RESHARP: Citation = Citation {
    key: "sun2014",
    text: "Sun, H., Wilman, A.H. (2014). \"Background field removal using spherical mean value filtering and Tikhonov regularization.\" *Magnetic Resonance in Medicine*, 71(3):1151-1157. https://doi.org/10.1002/mrm.25032",
};

pub(super) const CITE_HARPERELLA: Citation = Citation {
    key: "li2014",
    text: "Li, W., Avram, A.V., Wu, B., Xiao, X., Liu, C. (2014). \"Integrated Laplacian-based phase unwrapping and background phase removal for quantitative susceptibility mapping.\" *NMR in Biomedicine*, 27(2):219-227. https://doi.org/10.1002/nbm.3056",
};

pub(super) const CITE_IHARPERELLA: Citation = Citation {
    key: "li2015iharp",
    text: "Li, W., Wu, B., Liu, C. (2015). \"iHARPERELLA: an improved method for integrated 3D phase unwrapping and background phase removal.\" *Proc. ISMRM* 23, p.3313.",
};

pub(super) const CITE_XQSM: Citation = Citation {
    key: "gao2021xqsm",
    text: "Gao, Y., et al. (2021). \"xQSM: quantitative susceptibility mapping with octave convolutional and noise-regularized neural networks.\" *NMR in Biomedicine*, 34(3):e4461. https://doi.org/10.1002/nbm.4461",
};

pub(super) const CITE_QSMNET: Citation = Citation {
    key: "yoon2018qsmnet",
    text: "Yoon, J., et al. (2018). \"Quantitative susceptibility mapping using deep neural network: QSMnet.\" *NeuroImage*, 179:199-206. https://doi.org/10.1016/j.neuroimage.2018.06.030",
};

pub(super) const CITE_QSMNET_PLUS: Citation = Citation {
    key: "jung2020qsmnetplus",
    text: "Jung, W., et al. (2020). \"Exploring linearity of deep neural network trained QSM: QSMnet+.\" *NeuroImage*, 211:116579. https://doi.org/10.1016/j.neuroimage.2020.116579",
};

pub(super) const CITE_AUTOQSM: Citation = Citation {
    key: "wei2019autoqsm",
    text: "Wei, H., et al. (2019). \"Learning-based single-step quantitative susceptibility mapping reconstruction without brain extraction (AutoQSM).\" *NeuroImage*, 202:116064. https://doi.org/10.1016/j.neuroimage.2019.116064",
};

pub(super) const CITE_BFRNET: Citation = Citation {
    key: "zhang2023bfrnet",
    text: "Zhang, J., et al. (2023). \"BFRnet: A deep learning-based MR background field removal method for QSM.\" *Magnetic Resonance Imaging*, 100:15-24. https://github.com/sunhongfu/BFRnet",
};

pub(super) const CITE_SUSEPNET: Citation = Citation {
    key: "li2025susepnet",
    text: "Li, Z., Gao, Y., Sun, H., et al. (2025). \"SUSEP-Net: deep learning susceptibility source separation.\" arXiv:2506.13293.",
};

pub(super) const CITE_CHISEPNET: Citation = Citation {
    key: "kim2025chisepnet",
    text: "Kim, M., et al. (2025). \"χ-sepnet: Deep neural network for magnetic susceptibility source separation.\" *Human Brain Mapping*, 46(4):e70136. https://doi.org/10.1002/hbm.70136",
};

pub(super) const CITE_QSMGAN: Citation = Citation {
    key: "chen2020qsmgan",
    text: "Chen, Y., et al. (2020). \"QSMGAN: improved quantitative susceptibility mapping using 3D generative adversarial networks with increased receptive field.\" *NeuroImage*, 207:116389. https://doi.org/10.1016/j.neuroimage.2019.116389",
};

pub(super) const CITE_LPCNN: Citation = Citation {
    key: "lai2020lpcnn",
    text: "Lai, K.-W., et al. (2020). \"Learned proximal networks for quantitative susceptibility mapping (LPCNN).\" *MICCAI 2020*, LNCS 12262:125-135. https://doi.org/10.1007/978-3-030-59713-9_13",
};

pub(super) const CITE_IR2QSM: Citation = Citation {
    key: "li2025ir2qsm",
    text: "Li, M., et al. (2025). \"IR2QSM: quantitative susceptibility mapping via deep neural networks with iterative reverse concatenations and recurrent modules.\" *Medical Physics*. https://doi.org/10.1002/mp.17747",
};

pub(super) const CITE_MODLQSM: Citation = Citation {
    key: "feng2021modlqsm",
    text: "Feng, R., et al. (2021). \"MoDL-QSM: model-based deep learning for quantitative susceptibility mapping.\" *NeuroImage*, 240:118376. https://doi.org/10.1016/j.neuroimage.2021.118376",
};

pub(super) const CITE_NEXTQSM: Citation = Citation {
    key: "cognolato2023nextqsm",
    text: "Cognolato, F., et al. (2023). \"NeXtQSM — a complete deep learning pipeline for data-consistent quantitative susceptibility mapping trained with hybrid data.\" *NeuroImage*, 271:119729. https://doi.org/10.1016/j.neuroimage.2022.119729",
};

pub(super) const CITE_IQSM: Citation = Citation {
    key: "gao2022iqsm",
    text: "Gao, Y., et al. (2022). \"Instant tissue field and magnetic susceptibility mapping from MRI raw phase using Laplacian-enhanced deep neural networks (iQSM/iQFM).\" *NeuroImage*, 259:119410. https://doi.org/10.1016/j.neuroimage.2022.119410",
};

pub(super) const CITE_IQSM_PLUS: Citation = Citation {
    key: "gao2024iqsmplus",
    text: "Gao, Y., et al. (2024). \"Plug-and-play latent feature editing for orientation-adaptive quantitative susceptibility mapping (iQSM+).\" *Medical Image Analysis*, 94:103160. https://doi.org/10.1016/j.media.2024.103160",
};

pub(super) const CITE_MSMV: Citation = Citation {
    key: "roberts2024",
    text: "Roberts, A.G., et al. (2024). \"Maximum spherical mean value (mSMV) filtering for whole-brain quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 91(4):1586-1597. https://doi.org/10.1002/mrm.29963",
};

pub(super) const CITE_CLEARSWI: Citation = Citation {
    key: "eckstein2021",
    text: "Eckstein, K., Bachrata, B., Hangel, G., Widhalm, G., Enzinger, C., Barth, M., Trattnig, S., Robinson, S.D. (2021). \"Improved susceptibility weighted imaging at ultra-high field using bipolar multi-echo acquisition and optimized image processing: CLEAR-SWI.\" *NeuroImage*, 237:118175. https://doi.org/10.1016/j.neuroimage.2021.118175",
};

pub(super) const CITE_ARLO: Citation = Citation {
    key: "pei2015",
    text: "Pei, M., et al. (2015). \"Algorithm for fast monoexponential fitting based on Auto-Regression on Linear Operations (ARLO) of data.\" *Magnetic Resonance in Medicine*, 73(2):843-850. https://doi.org/10.1002/mrm.25137",
};

pub(super) const CITE_OTSU: Citation = Citation {
    key: "otsu1979",
    text: "Otsu, N. (1979). \"A Threshold Selection Method from Gray-Level Histograms.\" *IEEE Transactions on Systems, Man, and Cybernetics*, 9(1):62-66. https://doi.org/10.1109/TSMC.1979.4310076",
};

pub(super) const CITE_BET: Citation = Citation {
    key: "smith2002",
    text: "Smith, S.M. (2002). \"Fast robust automated brain extraction.\" *Human Brain Mapping*, 17(3):143-155. https://doi.org/10.1002/hbm.10062",
};

pub(super) const CITE_HDBET: Citation = Citation {
    key: "isensee2019",
    text: "Isensee, F., Schell, M., Pflueger, I., et al. (2019). \"Automated brain extraction of multisequence MRI using artificial neural networks.\" *Human Brain Mapping*, 40(17):4952-4964. https://doi.org/10.1002/hbm.24750",
};

pub(super) const CITE_RS2NET: Citation = Citation {
    key: "lin2024",
    text: "Lin, Y., Ding, Y., Chang, S., Ge, X., Sui, X., Jiang, Y. (2024). \"RS2-Net: An end-to-end deep learning framework for rodent skull stripping in multi-center brain MRI.\" *NeuroImage*, 298:120769. https://doi.org/10.1016/j.neuroimage.2024.120769",
};

pub(super) const CITE_MINDGRAB: Citation = Citation {
    key: "fani2025",
    text: "Fani, A., Masoud, M., Rorden, C., Plis, S. (2025). \"MindGrab for BrainChop: Fast and Accurate Skull Stripping for Command Line and Browser.\" *arXiv*:2506.11860. https://doi.org/10.48550/arXiv.2506.11860",
};

pub(super) const CITE_QSM_CONSENSUS: Citation = Citation {
    key: "bilgic2024consensus",
    text: "Bilgic, B., Costagli, M., Chan, K.-S., et al. (2024). \"Recommended implementation of quantitative susceptibility mapping for clinical research in the brain: A consensus of the ISMRM electro-magnetic tissue properties study group.\" *Magnetic Resonance in Medicine*, 91(5):1834-1862. https://doi.org/10.1002/mrm.30006",
};

pub(super) const CITE_QSMCI_SIGNAL_EROSION: Citation = Citation {
    key: "qsmci",
    text: "QSM-CI: signal-gated mask erosion (QSM-CI harmonization masking, `hd-bet-qsmci`). https://github.com/QSMxT/QSM-CI",
};

pub(super) const CITE_EPG: Citation = Citation {
    key: "weigel2015",
    text: "Weigel, M. (2015). \"Extended phase graphs: dephasing, RF pulses, and echoes - pure and simple.\" *Journal of Magnetic Resonance Imaging*, 41(2):266-295. https://doi.org/10.1002/jmri.24619",
};

pub(super) const CITE_R2PRIMENET: Citation = Citation {
    key: "kim2025",
    text: "Kim, M., Ji, S., Lee, J., et al. (2025). \"chi-sepnet: Deep neural network for magnetic susceptibility source separation.\" *Human Brain Mapping*, 46(4):e70136. https://doi.org/10.1002/hbm.70136",
};

pub(super) const CITE_SYNTHSEG: Citation = Citation {
    key: "billot2023",
    text: "Billot, B., Greve, D.N., Puonti, O., et al. (2023). \"SynthSeg: Segmentation of brain MRI scans of any contrast and resolution without retraining.\" *Medical Image Analysis*, 86:102789. https://doi.org/10.1016/j.media.2023.102789",
};

pub(super) const CITE_SYNTHSEG2: Citation = Citation {
    key: "billot2023pnas",
    text: "Billot, B., Magdamo, C., Cheng, Y., et al. (2023). \"Robust machine learning segmentation for large-scale analysis of heterogeneous clinical brain MRI datasets.\" *Proceedings of the National Academy of Sciences*, 120(9):e2216399120. https://doi.org/10.1073/pnas.2216399120",
};

pub(super) const CITE_SMWI: Citation = Citation {
    key: "gho2014",
    text: "Gho, S.-M., Liu, C., Li, W., et al. (2014). \"Susceptibility map-weighted imaging (SMWI) for neuroimaging.\" *Magnetic Resonance in Medicine*, 72(2):337-346. https://doi.org/10.1002/mrm.24920",
};

pub(super) const CITE_LSQR: Citation = Citation {
    key: "schweser2010",
    text: "Schweser, F., Deistung, A., Lehr, B.W., Reichenbach, J.R. (2010). \"Differentiation between diamagnetic and paramagnetic cerebral lesions based on magnetic susceptibility mapping.\" *Medical Physics*, 37(10):5165-5178. https://doi.org/10.1118/1.3481505",
};

pub(super) const CITE_HEIDI: Citation = Citation {
    key: "schweser2012",
    text: "Schweser, F., Sommer, K., Deistung, A., Reichenbach, J.R. (2012). \"Quantitative susceptibility mapping for investigating subtle susceptibility variations in the human brain.\" *NeuroImage*, 62(3):2083-2100. https://doi.org/10.1016/j.neuroimage.2012.05.067",
};

pub(super) const CITE_TWOPASS: Citation = Citation {
    key: "stewart2022",
    text: "Stewart, A.W., Robinson, S.D., O'Brien, K., et al. (2022). \"QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 87(3):1289-1300. https://doi.org/10.1002/mrm.29048",
};

pub(super) const CITE_BIPOLAR: Citation = Citation {
    key: "eckstein2021phd",
    text: "Eckstein, K. (2021). \"Advanced Methods for Quantitative Susceptibility Mapping and Susceptibility Weighted Imaging.\" PhD thesis, Medical University of Vienna. https://doi.org/10.34726/hss.2021.43447",
};

pub(super) const CITE_BIAS: Citation = Citation {
    key: "eckstein2019",
    text: "Eckstein, K., Trattnig, S., Robinson, S.D. (2019). \"A Simple Homogeneity Correction for Neuroimaging at 7T.\" *Proc. ISMRM 27th Annual Meeting*.",
};

pub(super) const CITE_QSMXT: Citation = Citation {
    key: "stewart2026qsmxt",
    text: "Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT",
};

pub(super) const CITE_QSMBLY: Citation = Citation {
    key: "stewart2026qsmbly",
    text: "Stewart, A. (2026). QSMbly: Browser-based Quantitative Susceptibility Mapping. https://github.com/astewartau/qsmbly",
};

pub(super) fn cite_inline(cite: &Citation) -> &'static str {
    match cite.key {
        "wu2012" => "Wu et al., 2012",
        "wang2022" => "Wang et al., 2022",
        "schweser2011" => "Schweser et al., 2011",
        "schweser2010" => "Schweser et al., 2010",
        "schweser2012" => "Schweser et al., 2012",
        "liu2011pdf" => "Liu et al., 2011",
        "wen2014" => "Wen et al., 2014",
        "zhou2014" => "Zhou et al., 2014",
        "kames2018" => "Kames et al., 2018",
        "bilgic2014tv" => "Bilgic et al., 2014",
        "bilgic2024consensus" => "Bilgic et al., 2024",
        "shmueli2009" => "Shmueli et al., 2009",
        "bilgic2014l2" => "Bilgic et al., 2014",
        "liu2011medi" => "Liu et al., 2011",
        "wei2015star" => "Wei et al., 2015",
        "liu2017tfi" => "Liu et al., 2017",
        "liu2018medi0" => "Liu et al., 2018",
        "liu2013nonlinear" => "Liu et al., 2013",
        "milovic2018" => "Milovic et al., 2018",
        "polak2020" => "Polak et al., 2020",
        "milovic2022" => "Milovic et al., 2022",
        "milovic2019" => "Milovic et al., 2019",
        "lambert2022" => "Lambert et al., 2022",
        "huang2023amppe" => "Huang et al., 2023",
        "shin2021chisep" => "Shin et al., 2021",
        "dimov2022" => "Dimov et al., 2022",
        "fang2023wavesep" => "Fang et al., 2023",
        "chen2021decompose" => "Chen et al., 2021",
        "stewart2026" => "Stewart et al., 2026",
        "li2015" => "Li et al., 2015",
        "sun2014" => "Sun & Wilman, 2014",
        "li2014" => "Li et al., 2014",
        "li2015iharp" => "Li et al., 2015",
        "roberts2024" => "Roberts et al., 2024",
        "gao2021xqsm" => "Gao et al., 2021",
        "yoon2018qsmnet" => "Yoon et al., 2018",
        "jung2020qsmnetplus" => "Jung et al., 2020",
        "wei2019autoqsm" => "Wei et al., 2019",
        "zhang2023bfrnet" => "Zhang et al., 2023",
        "li2025susepnet" => "Li et al., 2025",
        "kim2025chisepnet" => "Kim et al., 2025",
        "chen2020qsmgan" => "Chen et al., 2020",
        "lai2020lpcnn" => "Lai et al., 2020",
        "li2025ir2qsm" => "Li et al., 2025",
        "feng2021modlqsm" => "Feng et al., 2021",
        "cognolato2023nextqsm" => "Cognolato et al., 2023",
        "gao2022iqsm" => "Gao et al., 2022",
        "gao2024iqsmplus" => "Gao et al., 2024",
        "eckstein2021phd" => "Eckstein, 2021",
        _ => cite.key,
    }
}
