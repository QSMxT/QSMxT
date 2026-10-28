use crate::config::*;
use crate::enums::*;
use crate::masking::*;
use crate::plan::{Pass, Plan, Stage};
use crate::record::{Choice, DatasetRecord, RunRecord, Topic};
use serde::{Deserialize, Serialize};

struct Citation {
    key: &'static str,
    text: &'static str,
}

const CITE_MCPC3DS: Citation = Citation {
    key: "eckstein2018",
    text: "Eckstein, K., et al. (2018). \"Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE).\" *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963",
};

const CITE_ROMEO: Citation = Citation {
    key: "dymerska2021",
    text: "Dymerska, B., et al. (2021). \"Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO).\" *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563",
};

const CITE_LAPLACIAN_UNWRAP: Citation = Citation {
    key: "schofield2003",
    text: "Schofield, M.A., Zhu, Y. (2003). \"Fast phase unwrapping algorithm for interferometric applications.\" *Optics Letters*, 28(14):1194-1196. https://doi.org/10.1364/OL.28.001194",
};

const CITE_UKB_QSM: Citation = Citation {
    key: "wang2022",
    text: "Wang, C., et al. (2022). \"Phenotypic and genetic associations of quantitative magnetic susceptibility in UK Biobank brain imaging.\" *Nature Neuroscience*, 25(6):818-831. https://doi.org/10.1038/s41593-022-01074-w",
};

const CITE_VSHARP: Citation = Citation {
    key: "wu2012",
    text: "Wu, B., et al. (2012). \"Whole brain susceptibility mapping using compressed sensing.\" *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000",
};

const CITE_SHARP: Citation = Citation {
    key: "schweser2011",
    text: "Schweser, F., et al. (2011). \"Quantitative imaging of intrinsic magnetic tissue properties using MRI signal phase.\" *NeuroImage*, 54(4):2789-2807. https://doi.org/10.1016/j.neuroimage.2010.10.070",
};

const CITE_PDF: Citation = Citation {
    key: "liu2011pdf",
    text: "Liu, T., et al. (2011). \"A novel background field removal method for MRI using projection onto dipole fields.\" *NMR in Biomedicine*, 24(9):1129-1136. https://doi.org/10.1002/nbm.1670",
};

const CITE_ISMV: Citation = Citation {
    key: "wen2014",
    text: "Wen, Y., et al. (2014). \"An iterative spherical mean value method for background field removal in MRI.\" *Magnetic Resonance in Medicine*, 72(4):1065-1071. https://doi.org/10.1002/mrm.24998",
};

const CITE_LBV: Citation = Citation {
    key: "zhou2014",
    text: "Zhou, D., et al. (2014). \"Background field removal by solving the Laplacian boundary value problem.\" *NMR in Biomedicine*, 27(3):312-319. https://doi.org/10.1002/nbm.3064",
};

const CITE_COSMOS: Citation = Citation {
    key: "liu2009cosmos",
    text: "Liu, T., Spincemaille, P., de Rochefort, L., Kressler, B., Wang, Y. (2009). \"Calculation of susceptibility through multiple orientation sampling (COSMOS): a method for conditioning the inverse problem from measured magnetic field map to susceptibility source image in MRI.\" *Magnetic Resonance in Medicine*, 61:196-204. https://doi.org/10.1002/mrm.21828",
};

const CITE_STI: Citation = Citation {
    key: "liu2010sti",
    text: "Liu, C. (2010). \"Susceptibility tensor imaging.\" *Magnetic Resonance in Medicine*, 63:1471-1477. https://doi.org/10.1002/mrm.22482",
};

const CITE_RTS: Citation = Citation {
    key: "kames2018",
    text: "Kames, C., Wiggermann, V., Rauscher, A. (2018). \"Rapid two-step dipole inversion for susceptibility mapping with sparsity priors.\" *NeuroImage*, 167:276-283. https://doi.org/10.1016/j.neuroimage.2017.11.018",
};

const CITE_TV: Citation = Citation {
    key: "bilgic2014tv",
    text: "Bilgic, B., et al. (2014). \"Fast quantitative susceptibility mapping with L1-regularization and automatic parameter selection.\" *Magnetic Resonance in Medicine*, 72(5):1444-1459. https://doi.org/10.1002/mrm.25029",
};

const CITE_TKD: Citation = Citation {
    key: "shmueli2009",
    text: "Shmueli, K., et al. (2009). \"Magnetic susceptibility mapping of brain tissue in vivo using MRI phase data.\" *Magnetic Resonance in Medicine*, 62(6):1510-1522. https://doi.org/10.1002/mrm.22135",
};

const CITE_TIKHONOV: Citation = Citation {
    key: "bilgic2014l2",
    text: "Bilgic, B., et al. (2014). \"Fast image reconstruction with L2-regularization.\" *Journal of Magnetic Resonance Imaging*, 40(1):181-191. https://doi.org/10.1002/jmri.24365",
};

const CITE_NLTV: Citation = Citation {
    key: "milovic2018",
    text: "Milovic, C., Bilgic, B., Zhao, B., Acosta-Cabronero, J., Tejos, C. (2018). \"Fast nonlinear susceptibility inversion with variational regularization.\" *Magnetic Resonance in Medicine*, 80(2):814-821. https://doi.org/10.1002/mrm.27073",
};

const CITE_NDI: Citation = Citation {
    key: "polak2020",
    text: "Polak, D., et al. (2020). \"Nonlinear dipole inversion (NDI) enables robust quantitative susceptibility mapping (QSM).\" NMR in Biomedicine, 33(12):e4271.",
};

const CITE_L1QSM: Citation = Citation {
    key: "milovic2022",
    text: "Milovic, C., et al. (2022). \"Comparison of parameter optimization methods for quantitative susceptibility mapping.\" Magnetic Resonance in Medicine, 87(3):1517-1531.",
};

const CITE_WHQSM: Citation = Citation {
    key: "milovic2019",
    text: "Milovic, C., et al. (2019). \"Weak-harmonic regularization for quantitative susceptibility mapping.\" Magnetic Resonance in Medicine, 81(2):1399-1411.",
};

const CITE_HDQSM: Citation = Citation {
    key: "lambert2022",
    text: "Lambert, M., et al. (2022). \"Hybrid data fidelity term approach for quantitative susceptibility mapping.\" Magnetic Resonance in Medicine, 88(4):1567-1583.",
};

const CITE_AMPPE: Citation = Citation {
    key: "huang2023amppe",
    text: "Huang, S., et al. (2023). \"Quantitative susceptibility mapping using approximate message passing with parameter estimation.\" *Magnetic Resonance in Medicine*, 90(4):1414-1430. https://doi.org/10.1002/mrm.29725",
};

const CITE_CHISEP: Citation = Citation {
    key: "shin2021chisep",
    text: "Shin, H.-G., et al. (2021). \"χ-separation: Magnetic susceptibility source separation toward iron and myelin mapping in the brain.\" *NeuroImage*, 240:118371. https://doi.org/10.1016/j.neuroimage.2021.118371",
};

const CITE_R2STARQSM: Citation = Citation {
    key: "dimov2022",
    text: "Dimov, A. V., et al. (2022). \"Susceptibility source separation from gradient echo data using magnitude decay modeling.\" *Journal of Neuroimaging*, 32(4):852-859. https://doi.org/10.1111/jon.13014",
};

const CITE_WAVESEP: Citation = Citation {
    key: "fang2023wavesep",
    text: "Fang, J., et al. (2023). \"Wavelet-based single-step susceptibility source separation (WaveSep).\" *Magnetic Resonance in Medicine* (χ-separation family).",
};

const CITE_DECOMPOSE: Citation = Citation {
    key: "chen2021decompose",
    text: "Chen, J., et al. (2021). \"DECOMPOSE-QSM: susceptibility source separation from a single gradient-echo acquisition.\" *Magnetic Resonance in Medicine*.",
};

const CITE_HCCHISEP: Citation = Citation {
    key: "stewart2026",
    text: "Stewart, A., et al. (2026). \"Hollow-cylinder susceptibility source separation (HC-ChiSep).\"",
};

const CITE_MEDI: Citation = Citation {
    key: "liu2011medi",
    text: "Liu, T., et al. (2011). \"Morphology enabled dipole inversion (MEDI) from a single-angle acquisition.\" *Magnetic Resonance in Medicine*, 66(3):777-783. https://doi.org/10.1002/mrm.22816",
};

const CITE_MEDI0: Citation = Citation {
    key: "liu2018medi0",
    text: "Liu, Z., et al. (2018). \"MEDI+0: Morphology enabled dipole inversion with automatic uniform cerebrospinal fluid zero reference for quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 79(5):2795-2803. https://doi.org/10.1002/mrm.26946",
};

const CITE_FIT_PPM_COMPLEX: Citation = Citation {
    key: "liu2013nonlinear",
    text: "Liu, T., et al. (2013). \"Nonlinear formulation of the magnetic field to source relationship for robust quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 69(2):467-476. https://doi.org/10.1002/mrm.24272",
};

const CITE_TFI: Citation = Citation {
    key: "liu2017tfi",
    text: "Liu, Z., et al. (2017). \"Preconditioned total field inversion (TFI) method for quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 78(1):303-315. https://doi.org/10.1002/mrm.26331",
};

const CITE_ILSQR: Citation = Citation {
    key: "li2015",
    text: "Li, W., et al. (2015). \"A method for estimating and removing streaking artifacts in quantitative susceptibility mapping.\" *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043",
};

const CITE_STAR: Citation = Citation {
    key: "wei2015star",
    text: "Wei, H., Dibb, R., Zhou, Y., Sun, Y., Xu, J., Wang, N., Liu, C. (2015). \"Streaking artifact reduction for quantitative susceptibility mapping of sources with large dynamic range.\" *NMR in Biomedicine*, 28(10):1294-1303. https://doi.org/10.1002/nbm.3383",
};

const CITE_TGV: Citation = Citation {
    key: "langkammer2015",
    text: "Langkammer, C., et al. (2015). \"Fast quantitative susceptibility mapping using 3D EPI and total generalized variation.\" *NeuroImage*, 111:622-630. https://doi.org/10.1016/j.neuroimage.2015.02.041",
};

const CITE_QSMART: Citation = Citation {
    key: "yaghmaie2021",
    text: "Yaghmaie, N., Syeda, W., et al. (2021). \"QSMART: Quantitative Susceptibility Mapping Artifact Reduction Technique.\" *NeuroImage*, 231:117701. https://doi.org/10.1016/j.neuroimage.2020.117701",
};

const CITE_N4: Citation = Citation {
    key: "tustison2010",
    text: "Tustison, N.J., Avants, B.B., Cook, P.A., Zheng, Y., Egan, A., Yushkevich, P.A., Gee, J.C. (2010). \"N4ITK: Improved N3 Bias Correction.\" *IEEE Transactions on Medical Imaging*, 29(6):1310-1320. https://doi.org/10.1109/TMI.2010.2046908",
};

const CITE_RESHARP: Citation = Citation {
    key: "sun2014",
    text: "Sun, H., Wilman, A.H. (2014). \"Background field removal using spherical mean value filtering and Tikhonov regularization.\" *Magnetic Resonance in Medicine*, 71(3):1151-1157. https://doi.org/10.1002/mrm.25032",
};

const CITE_HARPERELLA: Citation = Citation {
    key: "li2014",
    text: "Li, W., Avram, A.V., Wu, B., Xiao, X., Liu, C. (2014). \"Integrated Laplacian-based phase unwrapping and background phase removal for quantitative susceptibility mapping.\" *NMR in Biomedicine*, 27(2):219-227. https://doi.org/10.1002/nbm.3056",
};

const CITE_IHARPERELLA: Citation = Citation {
    key: "li2015iharp",
    text: "Li, W., Wu, B., Liu, C. (2015). \"iHARPERELLA: an improved method for integrated 3D phase unwrapping and background phase removal.\" *Proc. ISMRM* 23, p.3313.",
};

const CITE_XQSM: Citation = Citation {
    key: "gao2021xqsm",
    text: "Gao, Y., et al. (2021). \"xQSM: quantitative susceptibility mapping with octave convolutional and noise-regularized neural networks.\" *NMR in Biomedicine*, 34(3):e4461. https://doi.org/10.1002/nbm.4461",
};

const CITE_QSMNET: Citation = Citation {
    key: "yoon2018qsmnet",
    text: "Yoon, J., et al. (2018). \"Quantitative susceptibility mapping using deep neural network: QSMnet.\" *NeuroImage*, 179:199-206. https://doi.org/10.1016/j.neuroimage.2018.06.030",
};

const CITE_QSMNET_PLUS: Citation = Citation {
    key: "jung2020qsmnetplus",
    text: "Jung, W., et al. (2020). \"Exploring linearity of deep neural network trained QSM: QSMnet+.\" *NeuroImage*, 211:116579. https://doi.org/10.1016/j.neuroimage.2020.116579",
};

const CITE_AUTOQSM: Citation = Citation {
    key: "wei2019autoqsm",
    text: "Wei, H., et al. (2019). \"Learning-based single-step quantitative susceptibility mapping reconstruction without brain extraction (AutoQSM).\" *NeuroImage*, 202:116064. https://doi.org/10.1016/j.neuroimage.2019.116064",
};

const CITE_BFRNET: Citation = Citation {
    key: "zhang2023bfrnet",
    text: "Zhang, J., et al. (2023). \"BFRnet: A deep learning-based MR background field removal method for QSM.\" *Magnetic Resonance Imaging*, 100:15-24. https://github.com/sunhongfu/BFRnet",
};

const CITE_SUSEPNET: Citation = Citation {
    key: "li2025susepnet",
    text: "Li, Z., Gao, Y., Sun, H., et al. (2025). \"SUSEP-Net: deep learning susceptibility source separation.\" arXiv:2506.13293.",
};

const CITE_CHISEPNET: Citation = Citation {
    key: "kim2025chisepnet",
    text: "Kim, M., et al. (SNU-LIST). \"χ-sepnet: deep learning χ-separation network.\" https://github.com/SNU-LIST/chi-separation",
};

const CITE_QSMGAN: Citation = Citation {
    key: "chen2020qsmgan",
    text: "Chen, Y., et al. (2020). \"QSMGAN: improved quantitative susceptibility mapping using 3D generative adversarial networks with increased receptive field.\" *NeuroImage*, 207:116389. https://doi.org/10.1016/j.neuroimage.2019.116389",
};

const CITE_LPCNN: Citation = Citation {
    key: "lai2020lpcnn",
    text: "Lai, K.-W., et al. (2020). \"Learned proximal networks for quantitative susceptibility mapping (LPCNN).\" *MICCAI 2020*, LNCS 12262:125-135. https://doi.org/10.1007/978-3-030-59713-9_13",
};

const CITE_IR2QSM: Citation = Citation {
    key: "li2025ir2qsm",
    text: "Li, M., et al. (2025). \"IR2QSM: quantitative susceptibility mapping via deep neural networks with iterative reverse concatenations and recurrent modules.\" *Medical Physics*. https://doi.org/10.1002/mp.17747",
};

const CITE_MODLQSM: Citation = Citation {
    key: "feng2021modlqsm",
    text: "Feng, R., et al. (2021). \"MoDL-QSM: model-based deep learning for quantitative susceptibility mapping.\" *NeuroImage*, 240:118376. https://doi.org/10.1016/j.neuroimage.2021.118376",
};

const CITE_NEXTQSM: Citation = Citation {
    key: "cognolato2023nextqsm",
    text: "Cognolato, F., et al. (2023). \"NeXtQSM — a complete deep learning pipeline for data-consistent quantitative susceptibility mapping trained with hybrid data.\" *NeuroImage*, 271:119729. https://doi.org/10.1016/j.neuroimage.2022.119729",
};

const CITE_IQSM: Citation = Citation {
    key: "gao2022iqsm",
    text: "Gao, Y., et al. (2022). \"Instant tissue field and magnetic susceptibility mapping from MRI raw phase using Laplacian-enhanced deep neural networks (iQSM/iQFM).\" *NeuroImage*, 259:119410. https://doi.org/10.1016/j.neuroimage.2022.119410",
};

const CITE_IQSM_PLUS: Citation = Citation {
    key: "gao2024iqsmplus",
    text: "Gao, Y., et al. (2024). \"Plug-and-play latent feature editing for orientation-adaptive quantitative susceptibility mapping (iQSM+).\" *Medical Image Analysis*, 94:103160. https://doi.org/10.1016/j.media.2024.103160",
};

const CITE_MSMV: Citation = Citation {
    key: "roberts2024",
    text: "Roberts, A.G., et al. (2024). \"Maximum spherical mean value (mSMV) filtering for whole-brain quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 91(4):1586-1597. https://doi.org/10.1002/mrm.29963",
};

const CITE_CLEARSWI: Citation = Citation {
    key: "eckstein2021",
    text: "Eckstein, K., Bachrata, B., Hangel, G., Widhalm, G., Enzinger, C., Barth, M., Trattnig, S., Robinson, S.D. (2021). \"Improved susceptibility weighted imaging at ultra-high field using bipolar multi-echo acquisition and optimized image processing: CLEAR-SWI.\" *NeuroImage*, 237:118175. https://doi.org/10.1016/j.neuroimage.2021.118175",
};

const CITE_ARLO: Citation = Citation {
    key: "pei2015",
    text: "Pei, M., et al. (2015). \"Algorithm for fast monoexponential fitting based on Auto-Regression on Linear Operations (ARLO) of data.\" *Magnetic Resonance in Medicine*, 73(2):843-850. https://doi.org/10.1002/mrm.25137",
};

const CITE_OTSU: Citation = Citation {
    key: "otsu1979",
    text: "Otsu, N. (1979). \"A Threshold Selection Method from Gray-Level Histograms.\" *IEEE Transactions on Systems, Man, and Cybernetics*, 9(1):62-66. https://doi.org/10.1109/TSMC.1979.4310076",
};

const CITE_BET: Citation = Citation {
    key: "smith2002",
    text: "Smith, S.M. (2002). \"Fast robust automated brain extraction.\" *Human Brain Mapping*, 17(3):143-155. https://doi.org/10.1002/hbm.10062",
};

const CITE_HDBET: Citation = Citation {
    key: "isensee2019",
    text: "Isensee, F., Schell, M., Pflueger, I., et al. (2019). \"Automated brain extraction of multisequence MRI using artificial neural networks.\" *Human Brain Mapping*, 40(17):4952-4964. https://doi.org/10.1002/hbm.24750",
};

const CITE_RS2NET: Citation = Citation {
    key: "lin2024",
    text: "Lin, Y., Ding, Y., Chang, S., Ge, X., Sui, X., Jiang, Y. (2024). \"RS2-Net: An end-to-end deep learning framework for rodent skull stripping in multi-center brain MRI.\" *NeuroImage*, 298:120769. https://doi.org/10.1016/j.neuroimage.2024.120769",
};

const CITE_MINDGRAB: Citation = Citation {
    key: "fani2025",
    text: "Fani, A., Masoud, M., Rorden, C., Plis, S. (2025). \"MindGrab for BrainChop: Fast and Accurate Skull Stripping for Command Line and Browser.\" *arXiv*:2506.11860. https://doi.org/10.48550/arXiv.2506.11860",
};

const CITE_QSM_CONSENSUS: Citation = Citation {
    key: "bilgic2024consensus",
    text: "Bilgic, B., Costagli, M., Chan, K.-S., et al. (2024). \"Recommended implementation of quantitative susceptibility mapping for clinical research in the brain: A consensus of the ISMRM electro-magnetic tissue properties study group.\" *Magnetic Resonance in Medicine*, 91(5):1834-1862. https://doi.org/10.1002/mrm.30006",
};

const CITE_QSMCI_SIGNAL_EROSION: Citation = Citation {
    key: "qsmci",
    text: "QSM-CI: signal-gated mask erosion (QSM-CI harmonization masking, `hd-bet-qsmci`). https://github.com/QSMxT/QSM-CI",
};

const CITE_EPG: Citation = Citation {
    key: "weigel2015",
    text: "Weigel, M. (2015). \"Extended phase graphs: dephasing, RF pulses, and echoes - pure and simple.\" *Journal of Magnetic Resonance Imaging*, 41(2):266-295. https://doi.org/10.1002/jmri.24619",
};

const CITE_R2PRIMENET: Citation = Citation {
    key: "kim2025",
    text: "Kim, M., Ji, S., Lee, J., et al. (2025). \"chi-sepnet: Deep neural network for magnetic susceptibility source separation.\" *Human Brain Mapping*, 46(4):e70136. https://doi.org/10.1002/hbm.70136",
};

const CITE_SYNTHSEG: Citation = Citation {
    key: "billot2023",
    text: "Billot, B., Greve, D.N., Puonti, O., et al. (2023). \"SynthSeg: Segmentation of brain MRI scans of any contrast and resolution without retraining.\" *Medical Image Analysis*, 86:102789. https://doi.org/10.1016/j.media.2023.102789",
};

const CITE_SMWI: Citation = Citation {
    key: "gho2014",
    text: "Gho, S.-M., Liu, C., Li, W., et al. (2014). \"Susceptibility map-weighted imaging (SMWI) for neuroimaging.\" *Magnetic Resonance in Medicine*, 72(2):337-346. https://doi.org/10.1002/mrm.24920",
};

const CITE_LSQR: Citation = Citation {
    key: "schweser2010",
    text: "Schweser, F., Deistung, A., Lehr, B.W., Reichenbach, J.R. (2010). \"Differentiation between diamagnetic and paramagnetic cerebral lesions based on magnetic susceptibility mapping.\" *Medical Physics*, 37(10):5165-5178. https://doi.org/10.1118/1.3481505",
};

const CITE_HEIDI: Citation = Citation {
    key: "schweser2012",
    text: "Schweser, F., Sommer, K., Deistung, A., Reichenbach, J.R. (2012). \"Quantitative susceptibility mapping for investigating subtle susceptibility variations in the human brain.\" *NeuroImage*, 62(3):2083-2100. https://doi.org/10.1016/j.neuroimage.2012.05.067",
};

const CITE_TWOPASS: Citation = Citation {
    key: "stewart2022",
    text: "Stewart, A.W., Robinson, S.D., O'Brien, K., et al. (2022). \"QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping.\" *Magnetic Resonance in Medicine*, 87(3):1289-1300. https://doi.org/10.1002/mrm.29048",
};

const CITE_BIPOLAR: Citation = Citation {
    key: "eckstein2021phd",
    text: "Eckstein, K. (2021). \"Advanced Methods for Quantitative Susceptibility Mapping and Susceptibility Weighted Imaging.\" PhD thesis, Medical University of Vienna. https://doi.org/10.34726/hss.2021.43447",
};

const CITE_BIAS: Citation = Citation {
    key: "eckstein2019",
    text: "Eckstein, K., Trattnig, S., Robinson, S.D. (2019). \"A Simple Homogeneity Correction for Neuroimaging at 7T.\" *Proc. ISMRM 27th Annual Meeting*.",
};

const CITE_QSMXT: Citation = Citation {
    key: "stewart2026qsmxt",
    text: "Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT",
};

const CITE_QSMBLY: Citation = Citation {
    key: "stewart2026qsmbly",
    text: "Stewart, A. (2026). QSMbly: Browser-based Quantitative Susceptibility Mapping. https://github.com/astewartau/qsmbly",
};

/// The voxels a susceptibility map is defined on (`referencing::defined_support` in the main
/// crate), named once in full and as "the reconstruction mask" after that.
const RECON_MASK: &str =
    "the reconstruction mask (the brain mask less any rim eroded by background field removal)";
const RECON_MASK_AGAIN: &str = "the reconstruction mask";

// ─── Settings that differ from the defaults ───
//
// A methods sentence names its method and cites it. A parameter value is stated only when the
// run's setting differs from QSMxT's default for it: the defaults are what the citation and the
// QSMxT version already pin down, and listing them buries the choices the user actually made.

/// Whether two floats differ by more than rounding.
fn differs(a: f64, b: f64) -> bool {
    (a - b).abs() > 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// The non-default settings of one sentence, as prose fragments ("deconvolution enabled").
#[derive(Default)]
struct Changed(Vec<String>);

impl Changed {
    /// Add `text` when `differs` holds.
    fn add(&mut self, differs: bool, text: impl FnOnce() -> String) {
        if differs {
            self.0.push(text());
        }
    }

    /// Add every field of `cfg` that differs from `default` (see [`changed_fields`]).
    fn add_fields<T: serde::Serialize>(&mut self, cfg: &T, default: &T, skip: &[&str]) {
        self.0.extend(changed_fields(cfg, default, skip));
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `" with a, b and c"`, or nothing.
    fn with(&self) -> String {
        self.joined(" with ")
    }

    /// `", with a, b and c"`, or nothing — for the end of a sentence that already has commas.
    fn comma_with(&self) -> String {
        self.joined(", with ")
    }

    /// `"; a, b and c"`, or nothing — for inside a citation's parentheses.
    fn semi(&self) -> String {
        self.joined("; ")
    }

    /// `" (a, b and c)"`, or nothing.
    fn paren(&self) -> String {
        if self.0.is_empty() { String::new() } else { format!(" ({})", join_list(&self.0)) }
    }

    fn joined(&self, lead: &str) -> String {
        if self.0.is_empty() { String::new() } else { format!("{lead}{}", join_list(&self.0)) }
    }
}

/// How a parameter reads in prose.
enum Label {
    /// `"λ = 0.001"`.
    Eq(&'static str),
    /// `"λ = 0.001"` plus a unit: `"radius = 5 mm"`.
    Unit(&'static str, &'static str),
    /// `"500 iterations"`.
    Count(&'static str),
    /// `"at most 500 iterations"`.
    AtMost(&'static str),
    /// `"merit enabled"` / `"merit disabled"`.
    Toggle(&'static str),
}

/// The prose label of an algorithm parameter, by its config field name. Anything not listed reads
/// as its field name with spaces, which is still unambiguous next to the algorithm it belongs to.
fn param_label(field: &str) -> Label {
    use Label::*;
    match field {
        "tol" | "tolerance" => Eq("tolerance"),
        "cg_tol" => Eq("conjugate-gradient tolerance"),
        "tol_update" => Eq("update tolerance"),
        "tol_delta" => Eq("convergence tolerance"),
        "max_iter" | "max_iterations" => AtMost("iterations"),
        "cg_max_iter" => AtMost("conjugate-gradient iterations"),
        "max_iter_l1" => AtMost("L1 iterations"),
        "max_iter_l2" => AtMost("L2 iterations"),
        "lsmr_iter" => Count("LSMR iterations"),
        "max_iter_newton" => AtMost("Newton iterations"),
        "n_cg" => AtMost("conjugate-gradient iterations"),
        "iterations" => Count("iterations"),
        "erosions" => Count("mask erosions"),
        "continuation_steps" => Count("continuation steps"),
        "inner_iterations" => Count("inner iterations"),
        "denoise_iterations" => Count("denoising iterations"),
        "max_evaluations" => AtMost("evaluations"),
        "max_samples" => AtMost("samples"),
        "levels" => Count("pyramid levels"),
        "chain_levels" => Count("pyramid levels for chained registrations"),
        "chain" => Toggle("chained registration"),
        "lambda" => Eq("λ"),
        "lambda1" => Eq("λ1"),
        "lambda_para" => Eq("λ (paramagnetic)"),
        "lambda_dia" => Eq("λ (diamagnetic)"),
        "lambda_cpl" => Eq("λ (coupling)"),
        "alpha" => Eq("α"),
        "alpha0" => Eq("α0"),
        "alpha1" => Eq("α1"),
        "alpha_l2" => Eq("α (L2)"),
        "beta" => Eq("β"),
        "delta" => Eq("δ"),
        "mu" => Eq("μ"),
        "mu0" => Eq("μ0"),
        "mu1" => Eq("μ1"),
        "mu2" => Eq("μ2"),
        "mu3" => Eq("μ3"),
        "mu1_l2" => Eq("μ1 (L2)"),
        "muh" => Eq("μ (harmonic)"),
        "mu_min" => Eq("minimum μ"),
        "rho" => Eq("ρ"),
        "tau" => Eq("τ"),
        "gamma" => Eq("γ (ρ increase factor)"),
        "tol_newton" => Eq("Newton tolerance"),
        "weight" => Eq("data weight"),
        "space" => Eq("dipole kernel"),
        "n_pad" => Unit("zero-padding", "voxels"),
        "pad" => Unit("zero-padding", "voxels per side"),
        "threshold" => Eq("threshold"),
        "radius" | "smv_radius" => Unit("SMV radius", "mm"),
        "tik_reg" => Eq("Tikhonov weight"),
        "smv" => Toggle("SMV preprocessing"),
        "smv_shrink_mask" => Toggle("SMV mask shrinking"),
        "merit" => Toggle("MERIT"),
        "linear" => Toggle("the linear data term"),
        "zeropad" => Unit("zero-padding", "voxels at the end of each axis"),
        "reltol" => Eq("relative tolerance"),
        "percentage" => Eq("gradient-mask percentage"),
        "data_weighting" => Eq("data weighting mode"),
        "precond" => Eq("preconditioner weight"),
        "step_size" => Eq("step size"),
        "reg" => Eq("regularization operator"),
        "pad_mm" | "ilsqr_pad_mm" => Unit("zero-padding", "mm per side"),
        "dr_pos" => Eq("Dr+"),
        "dr_neg" => Eq("Dr−"),
        "dr_pos_3t" => Eq("Dr+ at 3 T"),
        "r_const_3t" => Eq("relaxometric constant at 3 T"),
        "sigma_mm" => Unit("σ", "mm"),
        "nbox" => Count("segmentation boxes per dimension"),
        "shrink_factor" => Eq("shrink factor"),
        "convergence_threshold" => Eq("convergence threshold"),
        "mesh" => Eq("initial B-spline mesh"),
        "spline_order" => Eq("B-spline order"),
        "bias_field_fwhm" => Eq("bias field FWHM"),
        "wiener_noise" => Eq("Wiener filter noise"),
        "histogram_bins" => Count("histogram bins"),
        "estimate_offset" => Toggle("phase-offset estimation"),
        "denoise" => Toggle("anisotropic-diffusion denoising"),
        "rotation_step_deg" => Unit("initial rotation step", "°"),
        "translation_step_mm" => Unit("initial translation step", "mm"),
        "min_overlap" => Eq("minimum overlap"),
        "crop" => Unit("centre crop", "voxels"),
        "topology_cleanup" => Toggle("topology cleanup"),
        "sigma_smoothing" => Unit("posterior smoothing σ", "voxels"),
        "patch" => Unit("patch", "voxels"),
        _ => Eq(""),
    }
}

/// A JSON value as prose: numbers as written, arrays as `a × b × c`, `null` as "automatic".
fn json_prose(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "automatic".to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(json_prose).collect::<Vec<_>>().join(" × "),
        serde_json::Value::Number(n) if !n.is_i64() && !n.is_u64() => {
            n.as_f64().map_or_else(|| n.to_string(), |f| format!("{f}"))
        }
        other => other.to_string(),
    }
}

fn json_differs(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => differs(x, y),
        _ => a != b,
    }
}

/// The fields of `cfg` that differ from `default`, as prose (`"λ = 0.002"`, `"at most 50
/// iterations"`). Works on any serializable parameter struct, so every algorithm's settings are
/// covered without a sentence of its own; `skip` names fields the caller words itself.
fn changed_fields<T: serde::Serialize>(cfg: &T, default: &T, skip: &[&str]) -> Vec<String> {
    let (Ok(serde_json::Value::Object(c)), Ok(serde_json::Value::Object(d))) =
        (serde_json::to_value(cfg), serde_json::to_value(default))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, v) in &c {
        if skip.contains(&k.as_str()) || d.get(k).is_some_and(|dv| !json_differs(v, dv)) {
            continue;
        }
        let value = json_prose(v);
        out.push(match param_label(k) {
            Label::Eq("") if v.is_boolean() => format!("{} {}", k.replace('_', " "),
                if v.as_bool() == Some(true) { "enabled" } else { "disabled" }),
            Label::Eq("") => format!("{} = {value}", k.replace('_', " ")),
            Label::Eq(l) => format!("{l} = {value}"),
            Label::Unit(l, "°") => format!("{l} = {value}°"),
            Label::Unit(l, u) => format!("{l} = {value} {u}"),
            Label::Count(noun) => format!("{value} {noun}"),
            Label::AtMost(noun) => format!("at most {value} {noun}"),
            Label::Toggle(l) => format!("{l} {}", if v.as_bool() == Some(true) { "enabled" } else { "disabled" }),
        });
    }
    out
}

/// The non-default settings of the selected dipole inversion. Deep-learning inversions have none
/// here (tiling is worded separately).
fn inversion_changes(config: &PipelineConfig, alg: QsmAlgorithm) -> Changed {
    let (i, d) = (&config.inversion, InversionConfig::default());
    let mut c = Changed::default();
    // The echo time the field is converted at is worded below, not as `te = …`.
    const TE: [&str; 1] = ["te"];
    match alg {
        QsmAlgorithm::Rts => c.add_fields(&i.rts, &d.rts, &[]),
        QsmAlgorithm::Tv => c.add_fields(&i.tv, &d.tv, &[]),
        QsmAlgorithm::Tkd => c.add_fields(&i.tkd, &d.tkd, &[]),
        QsmAlgorithm::Tsvd => c.add_fields(&i.tsvd, &d.tsvd, &[]),
        QsmAlgorithm::Tikhonov => c.add_fields(&i.tikhonov, &d.tikhonov, &[]),
        QsmAlgorithm::Nltv => c.add_fields(&i.nltv, &d.nltv, &TE),
        // mSMV's settings are worded with it (see `medi_msmv`).
        // The SMV and mSMV settings are worded with them (see `medi_smv` and `medi_msmv`); the
        // mask shrinking only applies to MEDI-SMV.
        QsmAlgorithm::Medi => {
            let smv_worded = i.medi.smv && !i.medi.msmv;
            let skip: Vec<&str> = MEDI_MSMV_FIELDS.iter().copied()
                .chain(if smv_worded { MEDI_SMV_FIELDS.as_slice() } else { &["smv_shrink_mask"] }.iter().copied())
                .chain(MEDI_CSF_FIELDS)
                .chain(TE)
                .collect();
            c.add_fields(&i.medi, &d.medi, &skip)
        }
        QsmAlgorithm::Tfi => c.add_fields(&i.tfi, &d.tfi, &TE),
        QsmAlgorithm::Ilsqr => c.add_fields(&i.ilsqr, &d.ilsqr, &[]),
        // STAR-QSM's result depends on the padding and on the echo time the field is converted
        // to phase at, so both are always given.
        QsmAlgorithm::Star => {
            let p = i.star.pad;
            c.0.push(format!("zero-padding of {} × {} × {} voxels per side", p[0], p[1], p[2]));
            c.0.push(format!("the field converted to phase at {}", echo_time_prose(i.star.te)));
        }
        QsmAlgorithm::Lsqr => c.add_fields(&i.lsqr, &d.lsqr, &[]),
        QsmAlgorithm::Heidi => {
            c.add_fields(&i.lsqr, &d.lsqr, &[]);
            c.add_fields(&i.heidi, &d.heidi, &TE);
        }
        QsmAlgorithm::Ndi => c.add_fields(&i.ndi, &d.ndi, &TE),
        QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv => c.add_fields(&i.fansi, &d.fansi, &TE),
        QsmAlgorithm::L1qsm => c.add_fields(&i.l1qsm, &d.l1qsm, &TE),
        QsmAlgorithm::Whqsm => c.add_fields(&i.whqsm, &d.whqsm, &TE),
        QsmAlgorithm::Hdqsm => c.add_fields(&i.hdqsm, &d.hdqsm, &[]),
        QsmAlgorithm::AmpPe => c.add_fields(&i.amp_pe, &d.amp_pe, &[]),
        QsmAlgorithm::Tgv => c.add_fields(&i.tgv, &d.tgv, &TE),
        _ => {}
    }
    // Stated only when it is not the method's own default (its reference code's). TGV's sentence
    // always says it.
    let default = PipelineConfig::default();
    let te = crate::stages::inversion::echo_time_choice(config, alg);
    if let Some(te) = te.filter(|&te| alg != QsmAlgorithm::Tgv && Some(te) != crate::stages::inversion::echo_time_choice(&default, alg)) {
        c.0.push(format!("the field converted to phase at {}", echo_time_prose(te)));
    }
    c
}

/// The echo time a method converts the field to phase at, as prose ("the sum of the echo times").
fn echo_time_prose(te: EchoTimeChoice) -> String {
    match te {
        EchoTimeChoice::First => "the first echo time".into(),
        EchoTimeChoice::Last => "the last echo time".into(),
        EchoTimeChoice::Spacing => "the echo spacing".into(),
        EchoTimeChoice::Sum => "the sum of the echo times".into(),
        EchoTimeChoice::Seconds(_) => format!("an echo time of {te} ms"),
    }
}

/// MEDI's mSMV settings, which [`medi_msmv`] words rather than the list of changed settings.
const MEDI_MSMV_FIELDS: [&str; 4] = ["msmv", "msmv_maxk", "msmv_vessel_radius", "msmv_prefilter"];

/// MEDI's SMV settings, which [`medi_smv`] words rather than the list of changed settings.
const MEDI_SMV_FIELDS: [&str; 3] = ["smv", "smv_radius", "smv_shrink_mask"];

/// MEDI-SMV, after MEDI's name and citation in the inversion's sentence: ", with the local field
/// high-passed by a 5 mm spherical mean value (SMV) filter within the mask shrunk by that
/// radius"; nothing for MEDI without SMV, MEDI-mSMV ([`medi_msmv`]) or another inversion.
fn medi_smv(config: &PipelineConfig) -> String {
    let m = &config.inversion.medi;
    if config.inversion.algorithm != QsmAlgorithm::Medi || !m.smv || m.msmv {
        return String::new();
    }
    format!(", with the local field high-passed by a {} mm spherical mean value (SMV) filter {}",
        m.smv_radius,
        if m.smv_shrink_mask { "within the mask shrunk by that radius" } else { "over the full mask" })
}

/// MEDI+0's settings, which [`medi_csf`] words rather than the list of changed settings.
const MEDI_CSF_FIELDS: [&str; 4] = ["csf", "lambda_csf", "csf_r2s_threshold", "csf_erode"];

/// MEDI+0's sentence, after the inversion's: "MEDI's zero reference to the ventricular CSF
/// (MEDI+0; Liu et al., 2018) was applied.", with its changed settings, when the runs applied it.
/// QSM.rs runs MEDI without the term on fewer than three echoes (the CSF is found from R2*; the
/// plan warns), and each run records which it did ([`Topic::MediCsf`]): nothing is said when no
/// run applied it, and where the runs differ, or nothing is known of them, the configuration's
/// wording says the condition ("to runs of at least three echoes"). Nothing with the CSF term off
/// or another inversion.
fn describe_medi_csf(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let (m, d) = (&config.inversion.medi, MediConfig::default());
    if config.inversion.algorithm != QsmAlgorithm::Medi || !m.csf {
        return;
    }
    if ran.only(Topic::MediCsf, &[Choice::NotApplied]) {
        return;
    }
    let scope = if ran.only(Topic::MediCsf, &[Choice::Applied]) { "" } else { " to runs of at least three echoes" };
    doc.cite(&CITE_MEDI0);
    let mut changed = Changed::default();
    changed.add(differs(m.lambda_csf, d.lambda_csf), || format!("λ_CSF = {}", m.lambda_csf));
    changed.add(differs(m.csf_r2s_threshold, d.csf_r2s_threshold), || format!("CSF below R2* = {} Hz", m.csf_r2s_threshold));
    changed.add(m.csf_erode != d.csf_erode, || "without eroding the mask first".to_string());
    doc.say(&both(Stage::Inversion), format!(
        "MEDI's zero reference to the ventricular CSF (MEDI+0; {}{}) was applied{scope}.",
        cite_inline(&CITE_MEDI0), changed.semi()));
}

/// MEDI-mSMV, after MEDI's name and citation in the inversion's sentence: ", with maximum
/// spherical mean value (mSMV) filtering of the local field (Roberts et al., 2024)", and its
/// changed settings; nothing for plain MEDI or another inversion.
fn medi_msmv(config: &PipelineConfig, doc: &mut Draft) -> String {
    let (m, d) = (&config.inversion.medi, MediConfig::default());
    if config.inversion.algorithm != QsmAlgorithm::Medi || !m.msmv {
        return String::new();
    }
    doc.cite(&CITE_MSMV);
    let mut changed = Changed::default();
    changed.add(m.msmv_maxk != d.msmv_maxk, || format!("at most {} boundary-correction passes", m.msmv_maxk.saturating_sub(1)));
    changed.add(m.msmv_vessel_radius != d.msmv_vessel_radius, || match m.msmv_vessel_radius {
        Some(r) => format!("vessels up to {r} voxels thick"),
        None => "vessel thicknesses from the voxel size".to_string(),
    });
    changed.add(m.msmv_prefilter != d.msmv_prefilter, || format!(
        "the SMV prefilter {}", if m.msmv_prefilter { "enabled" } else { "disabled" }));
    format!(", with maximum spherical mean value (mSMV) filtering of the local field ({}{})",
        cite_inline(&CITE_MSMV), changed.semi())
}

/// QSMART's settings that differ from their defaults, its inner inversion and vessel magnitude
/// aside (said apart). The vessel sizes carry their units.
fn qsmart_changes(config: &PipelineConfig) -> Changed {
    let (q, d) = (&config.inversion.qsmart, QsmartConfig::default());
    let mut c = Changed::default();
    let mut skip = vec!["inversion", "vessel_magnitude", "vessel_size_units"];
    skip.extend(QSMART_VESSEL_SIZE_KEYS);
    c.add_fields(q, &d, &skip);
    // In other units than the default, the sizes are said whatever they are: the default numbers
    // mean something else.
    let u = q.vessel_size_units;
    let other_units = u != d.vessel_size_units;
    c.add(other_units || q.vasc_sphere_radius != d.vasc_sphere_radius,
        || format!("a bottom-hat sphere radius of {} {u}", q.vasc_sphere_radius));
    c.add(other_units || differs(q.frangi_scale_min, d.frangi_scale_min) || differs(q.frangi_scale_max, d.frangi_scale_max)
            || differs(q.frangi_scale_ratio, d.frangi_scale_ratio),
        || format!("Frangi scales from {} to {} {u} in steps of {} {u}", q.frangi_scale_min, q.frangi_scale_max, q.frangi_scale_ratio));
    c
}

/// The non-default settings of the selected background-field remover. V-SHARP's are worded by
/// its own sentence.
fn bg_removal_changes(config: &PipelineConfig) -> Changed {
    let (b, d) = (&config.bg_removal, BgRemovalConfig::default());
    let mut c = Changed::default();
    match b.algorithm {
        BfAlgorithm::Pdf => c.add_fields(&b.pdf, &d.pdf, &[]),
        BfAlgorithm::Lbv => c.add_fields(&b.lbv, &d.lbv, &[]),
        BfAlgorithm::Ismv => c.add_fields(&b.ismv, &d.ismv, &[]),
        BfAlgorithm::Sharp => c.add_fields(&b.sharp, &d.sharp, &[]),
        BfAlgorithm::Resharp => c.add_fields(&b.resharp, &d.resharp, &[]),
        BfAlgorithm::Harperella => c.add_fields(&b.harperella, &d.harperella, &[]),
        BfAlgorithm::Iharperella => c.add_fields(&b.iharperella, &d.iharperella, &[]),
        BfAlgorithm::Vsharp | BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => {}
    }
    c
}

/// The non-default rigid-registration settings, shared by motion correction and
/// multi-orientation co-registration.
fn registration_changes(config: &PipelineConfig) -> Changed {
    let mut c = Changed::default();
    c.add_fields(&config.registration, &crate::motion::RegistrationConfig::default(), &[]);
    c
}

/// The non-default settings of the selected susceptibility-source separation.
fn separation_changes(config: &PipelineConfig) -> Changed {
    let (s, d) = (&config.separation, SeparationConfig::default());
    let mut c = Changed::default();
    match s.algorithm {
        SeparationAlgorithm::R2starQsm => c.add_fields(&s.r2star_qsm, &d.r2star_qsm, &[]),
        SeparationAlgorithm::Decompose => c.add_fields(&s.decompose, &d.decompose, &[]),
        // Its QSM's iLSQR is worded with that QSM.
        SeparationAlgorithm::ChiSepIlsqr => c.add_fields(&s.chi_sep_ilsqr, &d.chi_sep_ilsqr, &["qsm_ilsqr"]),
        SeparationAlgorithm::ChiSepMedi => c.add_fields(&s.chi_sep_medi, &d.chi_sep_medi, &[]),
        SeparationAlgorithm::WaveSep => c.add_fields(&s.wavesep, &d.wavesep, &[]),
        SeparationAlgorithm::HcChisep => c.add_fields(&s.hc_chisep, &d.hc_chisep, &[]),
        SeparationAlgorithm::ChiSepNet => {
            let [x, y, z] = s.chi_sepnet.patch;
            c.add(s.chi_sepnet != d.chi_sepnet, || format!("{x}×{y}×{z}-voxel sliding-window patches"));
        }
        SeparationAlgorithm::SusepNet => {
            if let Some([x, y, z]) = s.susep_net.patch {
                c.add(true, || format!("the network applied as a sliding window of {x}×{y}×{z}-voxel patches"));
            }
        }
    }
    c
}

/// One sentence of the methods paragraph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sentence {
    pub text: String,
    /// The steps it describes, by the ids [`crate::pipeline_graph::pipeline_graph`] gives their
    /// boxes (`mask`, `invert-reliable`): several where one method does the work of several
    /// steps (TGV removes the background and inverts the field), none for a sentence about the
    /// run as a whole.
    pub steps: Vec<String>,
    /// The works it cites, by [`Reference::key`].
    pub citations: Vec<String>,
}

/// A work the methods text cites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    /// A short name for it (`dymerska2021`).
    pub key: String,
    /// The reference as the list gives it, in Markdown.
    pub text: String,
}

/// The methods text sentence by sentence, each with the steps it describes: what a front end
/// draws beside the pipeline graph, so that a box and the sentences about it can point at each
/// other. [`Methods::to_markdown`] is the text [`generate_methods_for`] writes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Methods {
    pub sentences: Vec<Sentence>,
    /// In the order the paragraph first cites them.
    pub references: Vec<Reference>,
    /// What the run departed from in its methods' reference pipelines, said before the references.
    #[serde(default, skip_serializing_if = "Note::is_empty")]
    pub note: Note,
}

/// The methods text's Note section: the departures from the chosen methods' recommendations and
/// reference pipelines that a run kept ([`crate::recommendations::note`]), one sentence each.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// From what the methods' authors recommend, or QSM.rs measured to be better (or a combination
    /// known to fail).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub departures: Vec<String>,
    /// From what the methods' reference pipelines used, where their authors did not say it is
    /// needed or better.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<String>,
}

impl Note {
    pub fn is_empty(&self) -> bool {
        self.departures.is_empty() && self.differences.is_empty()
    }
}

impl Methods {
    /// The paragraph: the sentences, joined by spaces.
    pub fn paragraph(&self) -> String {
        self.sentences.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    /// The paragraph under a `# Methods` heading, then the Note, if there is one, under `## Note`,
    /// then the references under `## References`.
    pub fn to_markdown(&self) -> String {
        let mut out = format!("# Methods\n\n{}\n\n", self.paragraph());
        let n = &self.note;
        if !n.is_empty() {
            out.push_str("## Note\n\n");
            for (heading, lines) in [
                ("Departures from the methods' recommendations:", &n.departures),
                ("Differences from the methods' reference pipelines:", &n.differences),
            ] {
                if lines.is_empty() {
                    continue;
                }
                out.push_str(&format!("{heading}\n\n"));
                for l in lines {
                    out.push_str(&format!("- {l}\n"));
                }
                out.push('\n');
            }
        }
        if !self.references.is_empty() {
            out.push_str("## References\n\n");
            for r in &self.references {
                out.push_str(&format!("- {}\n", r.text));
            }
        }
        out.truncate(out.trim_end().len());
        out.push('\n');
        out
    }
}

/// The methods text as it is written: each sentence with the steps it describes and the works
/// cited since the one before (a work is cited before the sentence that names it), and the
/// reference list.
#[derive(Default)]
struct Draft {
    sentences: Vec<(String, Vec<Stage>, Vec<&'static str>)>,
    references: Vec<&'static Citation>,
    cited: Vec<&'static str>,
}

impl Draft {
    /// Cite `cite` in the next sentence, and list it if it is not listed yet.
    fn cite(&mut self, cite: &'static Citation) {
        if !self.references.iter().any(|c| c.key == cite.key) {
            self.references.push(cite);
        }
        if !self.cited.contains(&cite.key) {
            self.cited.push(cite.key);
        }
    }

    /// A sentence about `steps`, citing what was cited since the last one.
    fn say(&mut self, steps: &[Stage], text: impl Into<String>) {
        self.sentences.push((text.into(), steps.to_vec(), std::mem::take(&mut self.cited)));
    }

    /// The steps each sentence names: those `plan` has and its graph draws, or none without a
    /// plan.
    fn finish(self, config: &PipelineConfig, plan: Option<&Plan>) -> Methods {
        self.finish_drawn(|stage| plan.is_some_and(|p| {
            p.has(*stage) && !crate::pipeline_graph::left_out(p, config, *stage, None)
        }))
    }

    /// The steps each sentence names: those `drawn` says the graph draws.
    fn finish_drawn(self, drawn: impl Fn(&Stage) -> bool) -> Methods {
        let sentences = self.sentences.into_iter().map(|(text, stages, cites)| {
            let mut steps: Vec<String> = Vec::new();
            for name in stages.iter().filter(|s| drawn(s)).map(Stage::name) {
                if !steps.contains(&name) {
                    steps.push(name);
                }
            }
            Sentence { text, steps, citations: cites.into_iter().map(str::to_string).collect() }
        }).collect();
        let references = self.references.iter()
            .map(|c| Reference { key: c.key.to_string(), text: c.text.to_string() })
            .collect();
        Methods { sentences, references, note: Note::default() }
    }
}

/// A step two-pass runs once per pass, in both passes.
fn both(step: fn(Pass) -> Stage) -> [Stage; 2] {
    [step(Pass::Main), step(Pass::Reliable)]
}

/// The steps that reconstruct the susceptibility map, in both passes: the ones that run in the
/// reconstruction box on the padded grid.
fn reconstruction() -> Vec<Stage> {
    [both(Stage::BackgroundRemoval), both(Stage::Inversion), both(Stage::Tgv), both(Stage::Qsmart)].concat()
}

/// The steps two-pass adds: the reliable-phase mask, the reconstruction on it, and the
/// combination of the two maps.
fn two_pass_steps() -> Vec<Stage> {
    vec![Stage::Mask(Pass::Reliable), Stage::BackgroundRemoval(Pass::Reliable), Stage::Inversion(Pass::Reliable),
         Stage::Tgv(Pass::Reliable), Stage::Qsmart(Pass::Reliable), Stage::TwoPassCombination]
}

/// Generate a methods description and citation list from a pipeline configuration.
/// Uses "qsmxt" as the tool name by default.
pub fn generate_methods(config: &PipelineConfig) -> String {
    generate_methods_for(config, "qsmxt")
}

/// Generate methods for a specific tool (e.g. "qsmxt" or "QSMbly"): [`Methods::to_markdown`].
pub fn generate_methods_for(config: &PipelineConfig, tool: &str) -> String {
    let config = &config.normalised().0;
    let mut methods = draft(config, tool, Ran::default()).finish(config, None);
    methods.note = crate::recommendations::config_note(config);
    methods.to_markdown()
}

/// The methods text for a run of `plan` (made by [`crate::plan::plan`] from `config` and the
/// scan's facts), sentence by sentence, each naming the steps of `plan`'s pipeline graph it
/// describes. Its [`Methods::to_markdown`] is [`generate_methods_for`]'s text.
pub fn methods_structured(config: &PipelineConfig, plan: &Plan, tool: &str) -> Methods {
    let config = &config.normalised().0;
    let mut methods = draft(config, tool, Ran::default()).finish(config, Some(plan));
    methods.note = crate::recommendations::config_note(config);
    methods
}

/// The methods text of a run of `plan` that did what `record` says ([`crate::run::run`]'s
/// record): it says what was done, as [`methods_structured`] does for the plan, where the run
/// decided something the configuration does not (a supplied mask used, or not found and computed
/// instead; R2′ measured rather than predicted; a step that had nothing to work on). A run that
/// decided nothing gets [`methods_structured`]'s text.
pub fn methods_for_run(config: &PipelineConfig, plan: &Plan, record: &RunRecord, tool: &str) -> Methods {
    let config = &config.normalised().0;
    let merged = DatasetRecord::from(record);
    let mut methods = draft(config, tool, Ran(Some(&merged))).finish_drawn(|stage| {
        plan.has(*stage) && !crate::pipeline_graph::left_out(plan, config, *stage, record.resampled)
    });
    methods.note = crate::recommendations::note(&merged.recommendations, 1);
    methods
}

/// The methods text of a dataset's runs of `config`, merged into `record` ([`crate::record::merge`]):
/// a choice every run made is said as for one run; where the runs differ, the configuration's
/// wording ("where available") stands. Each sentence names the steps some run did something in.
pub fn methods_for_dataset(config: &PipelineConfig, record: &DatasetRecord, tool: &str) -> Methods {
    let config = &config.normalised().0;
    let mut methods = draft(config, tool, Ran(Some(record)))
        .finish_drawn(|stage| record.step(&stage.name()).is_some_and(|s| s.ran() > 0));
    methods.note = crate::recommendations::note(&record.recommendations, record.runs);
    methods
}

/// What the runs the text describes did ([`DatasetRecord`]; one run's is a dataset of one). With
/// nothing known, the text says what the configuration asks for.
#[derive(Clone, Copy, Default)]
struct Ran<'a>(Option<&'a DatasetRecord>);

impl Ran<'_> {
    /// Some run decided about `topic`, and every one that did chose one of `choices`.
    fn only(&self, topic: Topic, choices: &[Choice]) -> bool {
        self.0.is_some_and(|r| r.only(topic, choices))
    }

    /// The runs the text describes (none when nothing is known of them).
    fn runs(&self) -> usize {
        self.0.map_or(0, |r| r.runs)
    }

    /// The runs that chose `choice` about `topic`.
    fn count(&self, topic: Topic, choice: Choice) -> usize {
        self.0.map_or(0, |r| r.count(topic, choice))
    }

    /// The plan of some run had `stage`, and no run did anything in it.
    fn none_ran(&self, stage: Stage) -> bool {
        self.0.is_some_and(|r| r.none_ran(&stage.name()))
    }

    /// The runs that did something in `stage`.
    fn ran(&self, stage: Stage) -> usize {
        self.0.and_then(|r| r.step(&stage.name())).map_or(0, |s| s.ran())
    }
}

fn draft(config: &PipelineConfig, tool: &str, ran: Ran) -> Draft {
    let mut doc = Draft::default();

    let (cite, author_year) = match tool {
        "QSMbly" | "qsmbly" => (&CITE_QSMBLY, "Stewart, 2026"),
        _ => (&CITE_QSMXT, "Stewart, 2026"),
    };

    doc.cite(cite);
    if config.pipeline.do_qsm {
        doc.say(&[], format!("QSM processing was performed using {} ({}).", tool, author_year));
    } else {
        doc.say(&[], format!("MRI processing was performed using {} ({}).", tool, author_year));
    }

    // Masking (inhomogeneity correction is described inline)
    describe_masking(config, ran, &mut doc);

    // QSM-specific pipeline
    if config.pipeline.do_qsm {
        describe_reconstruction_grid(config, ran, &mut doc);
        match config.inversion.algorithm {
            QsmAlgorithm::Tgv => {
                // Routed by QSM.rs's `InversionInput`: one echo goes in as wrapped phase (TGV
                // unwraps it itself), several are field-mapped first and go in as the total field.
                // Neither route runs a separate background-removal stage.
                doc.cite(&CITE_TGV);
                doc.say(&both(Stage::Tgv), format!(
                    "QSM was computed using the Total Generalized Variation (TGV) algorithm \
                     (Langkammer et al., 2015), with background field removal and dipole inversion \
                     performed jointly. For single-echo data TGV was applied to the wrapped phase of \
                     the echo and also performed the phase unwrapping; for multi-echo data TGV was \
                     applied to the B0 field map, computed as described below, expressed as phase at \
                     {}. The regularisation weights are in radians at that echo time \
                     and the field strength, and the susceptibility was converted to ppm with them.",
                    echo_time_prose(config.inversion.tgv.te)));
                let changed = inversion_changes(config, QsmAlgorithm::Tgv);
                if !changed.is_empty() {
                    doc.say(&both(Stage::Tgv), format!("TGV was run{}.", changed.with()));
                }
                describe_field_mapping(config, ran, &mut doc);
            }
            QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => {
                // `InversionInput::WrappedPhase`: no field mapping and no background removal ran.
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                doc.cite(cite);
                doc.say(&both(Stage::Inversion), format!(
                    "QSM was reconstructed with {} ({}) from the wrapped phase of each echo, with \
                     phase unwrapping, background field removal and dipole inversion performed by the \
                     network.",
                    name, cite_inline(cite),
                ));
                say_tiling(config, Stage::Inversion, &mut doc);
            }
            QsmAlgorithm::Qsmart => {
                // QSMART consumes a pre-computed total field, so the field-mapping
                // stage (phase offset, unwrapping, B0 estimation) still runs and must
                // be described.
                describe_field_mapping(config, ran, &mut doc);
                let (inv_name, inv_cite) = inversion_name_cite(config.inversion.qsmart.inversion);
                doc.cite(&CITE_QSMART);
                doc.cite(inv_cite);
                doc.say(&both(Stage::Qsmart), format!(
                    "QSM was computed using QSMART (Yaghmaie et al., 2021), with spatially dependent \
                     filtering for background field removal, {} dipole inversion ({}), and Frangi \
                     vesselness-based tissue/vasculature separation{}.",
                    inv_name, cite_inline(inv_cite), qsmart_changes(config).comma_with(),
                ));
                // The magnitude vessels were detected on.
                match config.inversion.qsmart.vessel_magnitude {
                    QsmartVesselMagnitude::MeanN4 => {
                        doc.cite(&CITE_N4);
                        doc.say(&both(Stage::Qsmart), "Vessels were detected on the mean magnitude over echoes, \
                            bias-corrected with N4 (Tustison et al., 2010).".to_string());
                    }
                    QsmartVesselMagnitude::Combined => {
                        doc.say(&both(Stage::Qsmart), format!("Vessels were detected on the {}RSS-combined magnitude.",
                            if config.masking.inhomogeneity_correction { "inhomogeneity-corrected " } else { "" }));
                    }
                }
            }
            _ => {
                describe_field_mapping(config, ran, &mut doc);

                // Background removal — only in front of a local-field inversion (QSM.rs
                // `InversionInput`); the total-field methods remove or model it themselves.
                let total_field = !crate::bridge::inversion_input(config, 2).needs_bg_removal();
                if !total_field {
                    let alg = &config.bg_removal.algorithm;
                    let (name, cite) = bg_removal_name_cite(*alg);
                    doc.cite(cite);
                    if *alg == BfAlgorithm::Vsharp {
                        let (v, d) = (&config.bg_removal.vsharp, VsharpConfig::default());
                        let mut changed = Changed::default();
                        changed.add(differs(v.max_radius, d.max_radius) || differs(v.min_radius, d.min_radius)
                            || differs(v.radius_step, d.radius_step),
                            || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
                        changed.add(v.deconvolve != d.deconvolve, || format!(
                            "truncated-SVD deconvolution {}",
                            if v.deconvolve { "enabled" } else { "disabled" }));
                        // The threshold only exists for the deconvolution.
                        changed.add(v.deconvolve && differs(v.threshold, d.threshold),
                            || format!("a deconvolution threshold of {}", v.threshold));
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), changed.with()));
                        let resample = "the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution";
                        match v.grid {
                            VsharpGrid::Finest => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "For anisotropic voxels the V-SHARP background was estimated on an isotropic grid at the smallest acquired voxel dimension: {resample}. Isotropic acquisitions were processed on their acquired grid.")),
                            VsharpGrid::Isotropic if !differs(v.grid_spacing, d.grid_spacing) => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on an isotropic grid: {resample}.")),
                            VsharpGrid::Isotropic => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on a {} mm isotropic grid: {resample}.", v.grid_spacing)),
                            VsharpGrid::Native => {}
                        }
                    } else {
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), bg_removal_changes(config).with()));
                    }

                }

                // Dipole inversion
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                let target = if total_field { " on the total field within the brain mask" } else { "" };
                doc.cite(cite);
                let msmv = medi_msmv(config, &mut doc) + &medi_smv(config);
                let changes = inversion_changes(config, config.inversion.algorithm);
                doc.say(&both(Stage::Inversion), format!("Dipole inversion was performed{} using {} ({}){msmv}{}.",
                    target, name, cite_inline(cite),
                    if msmv.is_empty() { changes.with() } else { changes.comma_with() }));
                describe_medi_csf(config, ran, &mut doc);
                // Deep-learning tiling (memory-bounded, approximate) is noted when enabled — only
                // for the tileable networks (a stray --tile-size on a classical algorithm is ignored).
                if config.inversion.algorithm.tileable() {
                    say_tiling(config, Stage::Inversion, &mut doc);
                } else if config.runs_a_tileable_network() {
                    say_tiling(config, Stage::BackgroundRemoval, &mut doc);
                }
            }
        }

        // Two-pass wraps background removal and inversion, so it is described after both and
        // before referencing, which runs on the combined map.
        describe_two_pass(config, ran, &mut doc);

        // Referencing
        match config.qsm.reference {
            QsmReference::Mean => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility within \
                     {RECON_MASK}."));
            }
            QsmReference::None => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], "No susceptibility referencing was applied.".to_string());
            }
            QsmReference::Region => {
                let region = config.qsm.reference_region.as_deref().unwrap_or("");
                let pretty = region.replace('-', " ").replace(',', ", ");
                let plural = if region.contains(',') { "s" } else { "" };
                let labels = match config.segmentation.custom_dseg_tool.as_deref() {
                    Some(tool) => format!("'{tool}' segmentation label{plural}"),
                    None => format!("SynthSeg label{plural}"),
                };
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility in the \
                     {pretty} ({labels}), computed within {RECON_MASK}."
                ));
            }
        }
    }

    // A supplied segmentation, every run's or some runs'.
    let dseg_tool = config.segmentation.custom_dseg_tool.as_deref();
    let dseg_supplied = ran.count(Topic::SegmentationSource, Choice::Supplied) > 0;
    let dseg_all_supplied = ran.only(Topic::SegmentationSource, &[Choice::Supplied]);
    if config.pipeline.do_segmentation && dseg_supplied {
        let from = match dseg_tool { Some(t) if t != "*" => format!("'{t}' "), _ => String::new() };
        doc.say(&[Stage::Segmentation], if dseg_all_supplied {
            format!("Segmentations were sourced from the {from}BIDS derivatives.")
        } else {
            format!("Where available, segmentations were sourced from the {from}BIDS derivatives; \
                     otherwise they were generated as follows.")
        });
    }
    if config.pipeline.do_segmentation && !dseg_all_supplied && !ran.none_ran(Stage::Segmentation) {
        doc.cite(&CITE_SYNTHSEG);
        let labels = if config.segmentation.version == SynthSegVersion::V2 { 33 } else { 32 };
        let (s, d) = (&config.segmentation, SegmentationConfig::default());
        let mut changed = Changed::default();
        changed.add(s.flip_averaging != d.flip_averaging, || format!(
            "left-right flip averaging of the posteriors {}",
            if s.flip_averaging { "enabled" } else { "disabled" }));
        // The version names the weights (and the label count says what they produce); the
        // bring-your-own tool is not a SynthSeg setting.
        if let Some(patch) = s.patch {
            changed.add(true, || format!(
                "the network applied as a sliding window of {p}×{p}×{p}-voxel patches overlapping by at \
                 least half a patch, their posteriors Gaussian-weighted", p = patch.div_ceil(16) * 16));
        }
        changed.add_fields(s, &d, &["version", "flip_averaging", "custom_dseg_tool", "patch"]);
        doc.say(&[Stage::Segmentation], format!(
            "The brain was parcellated from the GRE magnitude using SynthSeg {} (Billot et al., \
             2023), yielding {} FreeSurfer-labelled structures{}.",
            config.segmentation.version, labels, changed.comma_with(),
        ));
    }
    if config.pipeline.do_analysis && !ran.none_ran(Stage::Analysis) {
        doc.say(&[Stage::Analysis], 
            "Each quantitative map was then summarised within every segmented structure (mean, \
             standard deviation, median, minimum, maximum and 5th/95th percentiles over the \
             structure's voxels), excluding voxels the map does not cover.".to_string());
    }

    if config.pipeline.do_smwi && !ran.none_ran(Stage::Smwi) {
        doc.cite(&CITE_SMWI);
        let (s, d) = (&config.smwi, SmwiConfig::default());
        let mut mask = Changed::default();
        mask.add(differs(s.threshold_ppm, d.threshold_ppm), || format!("|χ| threshold {:.2} ppm", s.threshold_ppm));
        mask.add(differs(s.power, d.power), || format!("mask raised to the power {}", s.power));
        let slab = if s.mip_window != d.mip_window { format!(" over {} slices", s.mip_window) } else { String::new() };
        doc.say(&[Stage::Smwi], format!(
            "Susceptibility map-weighted images were computed (Gho et al., 2014) by weighting the \
             magnitude with a mask derived from the susceptibility map{}, separately for \
             paramagnetic and diamagnetic sources, each followed by a minimum-intensity \
             projection{slab}.",
            mask.paren(),
        ));
    }

    // SWI
    if config.pipeline.do_swi {
        let (swi, d) = (&config.swi, SwiConfig::default());
        let mut changed: Vec<String> = Vec::new();
        match swi.unwrap {
            _ if swi.unwrap == d.unwrap => {}
            SwiUnwrap::Romeo => {
                changed.push("ROMEO phase unwrapping (Dymerska et al., 2021)".into());
                doc.cite(&CITE_ROMEO);
            }
            SwiUnwrap::LaplacianSlice => changed.push("slice-wise Laplacian phase unwrapping".into()),
            SwiUnwrap::Laplacian => changed.push("Laplacian phase unwrapping".into()),
        }
        if swi.hp_sigma.iter().zip(d.hp_sigma).any(|(a, b)| differs(*a, b)) {
            changed.push(format!(
                "a high-pass filter σ of {} voxels",
                swi.hp_sigma.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" × "),
            ));
        }
        if swi.scaling != d.scaling {
            changed.push(format!("{} phase-mask scaling", swi.scaling));
        }
        if differs(swi.strength, d.strength) {
            changed.push(format!("phase-mask strength {}", swi.strength));
        }
        let mip = if swi.mip_window != d.mip_window {
            format!("Minimum-intensity projections were taken over {}-slice slabs.", swi.mip_window)
        } else {
            "Minimum-intensity projections of these images were also computed.".to_string()
        };
        let using = if changed.is_empty() { String::new() } else { format!(" using {}", join_list(&changed)) };
        doc.cite(&CITE_CLEARSWI);
        doc.say(&[Stage::Swi], format!(
            "Susceptibility-weighted images were computed with CLEAR-SWI (Eckstein et al., 2021){using}. {mip}"
        ));
    }

    // T2*/R2*
    if config.pipeline.do_t2starmap && config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* and R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_t2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    }

    // R2 / R2'. Worth stating how R2' was obtained, not just that it was: a measured R2' and an
    // estimated one support different claims, and only one of them is a measurement.
    if config.pipeline.do_r2map || config.pipeline.do_r2primemap {
        if config.pipeline.do_r2map && ran.only(Topic::R2Source, &[Choice::Supplied]) {
            doc.say(&[Stage::R2], "R2 maps were sourced from the supplied BIDS derivatives.".to_string());
        } else if config.pipeline.do_r2map && !ran.only(Topic::R2Source, &[Choice::Unavailable]) {
            doc.cite(&CITE_EPG);
            doc.say(&[Stage::R2], 
                "R2 maps were fitted from multi-echo spin-echo magnitude data using an Extended \
                 Phase Graph dictionary (Weigel, 2015).".to_string());
        }
        if config.pipeline.do_r2primemap {
            // R2PRIMEnet's own settings; the field-strength override gets a sentence below.
            let mut r2pn = Changed::default();
            r2pn.add_fields(&config.separation.r2primenet, &R2primenetConfig::default(),
                &["ignore_field_mismatch"]);
            let r2pn = r2pn.semi();
            // What the runs that made an R2′ did, where they all did the same.
            let measured = ran.count(Topic::R2primeSource, Choice::Measured) > 0;
            let predicted = ran.count(Topic::R2primeSource, Choice::Predicted) > 0;
            let none_made = ran.only(Topic::R2primeSource, &[Choice::Unavailable]);
            match config.separation.custom_r2prime_tool.as_deref() {
                _ if none_made => {}
                // `auto` that only measured, or only predicted.
                None if config.separation.r2prime_strategy == R2PrimeStrategy::Auto && measured != predicted => {
                    if measured {
                        doc.say(&[Stage::R2], "R2' was computed as R2* - R2.".to_string());
                    } else {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., 2025{r2pn}).",
                        ));
                    }
                }
                Some(_) => doc.say(&[Stage::R2], 
                    "R2' maps were taken from the supplied BIDS derivatives.".to_string()),
                None => match config.separation.r2prime_strategy {
                    R2PrimeStrategy::Mese => doc.say(&[Stage::R2], 
                        "R2' was computed as R2* - R2.".to_string()),
                    R2PrimeStrategy::R2primenet => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., \
                             2025{r2pn}).",
                        ));
                    }
                    R2PrimeStrategy::Auto => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], if config.separation.r2primenet.ignore_field_mismatch {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and estimated from R2* with R2PRIMEnet \
                             (Kim et al., 2025{r2pn}) otherwise.")
                        } else {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and otherwise, at 3 T only, estimated \
                             from R2* with R2PRIMEnet (Kim et al., 2025{r2pn}).")
                        });
                    }
                },
            }
            // Both ways QSMxT computes an R2′ (R2* − R2 and R2PRIMEnet) set negative values to zero.
            if config.separation.custom_r2prime_tool.is_none() && !none_made {
                doc.say(&[Stage::R2], "Negative R2' values were set to zero.".to_string());
            }
            // Said where a run applied it off 3 T, or nothing is known of the runs.
            let off_field = ran.count(Topic::R2primenetCalibration, Choice::Uncalibrated) > 0
                || ran.0.is_none_or(|r| r.split(Topic::R2primeSource).is_empty());
            if config.separation.custom_r2prime_tool.is_none()
                && config.separation.r2prime_strategy != R2PrimeStrategy::Mese
                && config.separation.r2primenet.ignore_field_mismatch
                && off_field && !none_made
            {
                doc.say(&[Stage::R2], 
                    "R2PRIMEnet, whose weights were trained at 3 T, was also applied to data \
                     acquired at other field strengths."
                        .to_string());
            }
        }
    }

    // Chi-separation (susceptibility source separation)
    if config.pipeline.do_chi_separation {
        let (sep_name, sep_cite) = separation_name_cite(config.separation.algorithm);
        doc.cite(sep_cite);
        doc.say(&[Stage::ChiSeparation], format!(
            "Paramagnetic and diamagnetic susceptibility maps were computed using {} ({}) within {}{}.",
            sep_name, cite_inline(sep_cite),
            // Named in full by the referencing sentence, unless the map was left unreferenced.
            if config.pipeline.do_qsm && config.qsm.reference != QsmReference::None { RECON_MASK_AGAIN } else { RECON_MASK },
            separation_changes(config).comma_with(),
        ));
        // DECOMPOSE's signal phase comes from a QSM per echo; the pipeline has one QSM.
        if config.separation.algorithm == SeparationAlgorithm::Decompose {
            doc.say(&[Stage::ChiSeparation],
                "The same susceptibility map was used for the signal phase at every echo.".to_string());
        }
        // The networks' outputs as QSM.rs returns them.
        if matches!(config.separation.algorithm, SeparationAlgorithm::ChiSepNet | SeparationAlgorithm::SusepNet) {
            doc.say(&[Stage::ChiSeparation], "Negative χ+ and |χ−| values were set to zero.".to_string());
        }
        if crate::stages::separation::reconstructs_qsm(config) {
            let mut q = Changed::default();
            q.add_fields(&config.separation.chi_sep_ilsqr.qsm_ilsqr, &SeparationConfig::default().chi_sep_ilsqr.qsm_ilsqr, &[]);
            doc.cite(&CITE_ILSQR);
            doc.say(&[Stage::ChiSeparation], format!(
                "The susceptibility map separated was reconstructed from the local field by iLSQR ({}){}.",
                cite_inline(&CITE_ILSQR), q.with()));
        }
    }

    // Multi-orientation. Described after the per-orientation pipeline because that is the
    // order it runs in: each orientation is unwrapped and background-corrected on its own,
    // and only the local fields are combined.
    if config.multi_orientation.enabled() {
        match config.multi_orientation.algorithm {
            MultiOrientAlgorithm::Cosmos => {
                doc.cite(&CITE_COSMOS);
                doc.say(&[], 
                    "Local field maps from multiple head orientations, co-registered onto a common \
                     grid, were combined by Calculation Of Susceptibility through Multiple \
                     Orientation Sampling (COSMOS; Liu et al., 2009), using each orientation's B0 \
                     direction expressed in that common frame."
                        .to_string(),
                );
            }
            MultiOrientAlgorithm::Sti => {
                doc.cite(&CITE_STI);
                doc.say(&[], 
                    "Local field maps from multiple head orientations, co-registered onto a common \
                     grid, were combined by susceptibility tensor imaging (STI; Liu, 2010), \
                     yielding the six independent components of the symmetric susceptibility \
                     tensor along with mean magnetic susceptibility (MMS), magnetic susceptibility \
                     anisotropy (MSA) and the principal eigenvector."
                        .to_string(),
                );
            }
        }
        // How the orientations got onto one grid is part of the method, not an implementation
        // detail: it determines where the B0 directions came from, which is half the input.
        if config.multi_orientation.register {
            doc.say(&[], format!(
                "Orientations that were not already on a common grid were rigidly co-registered \
                 (6 degrees of freedom, normalized cross-correlation over a multi-resolution \
                 pyramid{}) onto the first orientation's grid using the magnitude images, and each \
                 orientation's B0 direction was derived from the recovered rotation.",
                registration_changes(config).semi(),
            ));
        } else {
            doc.say(&[], 
                "The orientations were taken to be already co-registered onto a common grid."
                    .to_string(),
            );
        }
        if differs(config.multi_orientation.lambda, MultiOrientationConfig::default().lambda) {
            doc.say(&[], format!(
                "The multi-orientation inversion was regularized with lambda = {}.",
                config.multi_orientation.lambda
            ));
        }
        let m = &config.multi_orientation;
        if m.algorithm == MultiOrientAlgorithm::Cosmos {
            if m.magnitude_weighting {
                let d = MultiOrientationConfig::default();
                let mut cg = Changed::default();
                cg.add(differs(m.cosmos_tol, d.cosmos_tol), || format!("tolerance {}", m.cosmos_tol));
                cg.add(m.cosmos_max_iter != d.cosmos_max_iter, || format!("at most {} iterations", m.cosmos_max_iter));
                doc.say(&[], format!(
                    "Each orientation's field was weighted by its magnitude, and the weighted \
                     least-squares problem solved by conjugate gradient{}.", cg.paren()));
            }
            doc.say(&[], 
                "The COSMOS map was referenced in the same way as the single-orientation maps, \
                 over the voxels covered by every orientation's background-field-removal mask."
                    .to_string(),
            );
        }
    }

    // DICOM export
    if config.pipeline.export_dicom {
        doc.say(&[], "Final maps were additionally exported as DICOM series.".to_string());
    }

    doc
}

/// The reconstruction-grid switches, all off by default, so each is said only when on.
fn describe_reconstruction_grid(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let p = &config.pipeline;
    if p.ignore_slice_gap {
        doc.say(&[Stage::Load], "Acquisitions whose slices were separated by a gap were reconstructed as if the slices were contiguous.".to_string());
    }
    let space = match p.output_space {
        OutputSpace::Acquired => "",
        OutputSpace::Working => ", and the derivatives were written on the resampled grid",
    };
    // Resampled only because an algorithm needs axial data: the switches do not say so.
    if !p.axial_resampling && ran.count(Topic::Grid, Choice::ResampledForAlgorithm) > 0 {
        doc.say(&[Stage::Load, Stage::OutputSpace], format!(
            "Acquisitions whose B0 direction was more than {}° from the slice normal were resampled onto an \
             axial grid, with B0 along the slice normal, before reconstruction, as the algorithms used assume \
             it{space}.", crate::scan::AXIAL_ONLY_TOLERANCE_DEG));
    }
    // None of the runs was resampled: there is nothing to say.
    if p.axial_resampling && !ran.only(Topic::Grid, &[Choice::Acquired]) {
        let which = if differs(p.obliquity_threshold, DEFAULT_OBLIQUITY_THRESHOLD) {
            format!("Acquisitions more than {}° oblique", p.obliquity_threshold)
        } else {
            "Oblique acquisitions".to_string()
        };
        doc.say(&[Stage::Load, Stage::OutputSpace], format!(
            "{which} were resampled onto an axial grid, with B0 along the slice normal, before reconstruction{space}."));
    }
    if p.crop_to_mask {
        let margin = if differs(p.crop_margin_mm, PipelineToggles::default().crop_margin_mm) {
            format!(" with a {} mm margin", p.crop_margin_mm)
        } else {
            String::new()
        };
        doc.say(&reconstruction(), format!("Reconstruction was performed within a box around the brain mask{margin}."));
    }
    if p.fft_padding {
        doc.say(&reconstruction(), "Before the FFT-based stages, the grid was zero-padded to an FFT-efficient size.".to_string());
    }
}

/// "64" for a uniform pad, "64×64×32" otherwise.
fn describe_pad(p: [usize; 3]) -> String {
    if p[0] == p[1] && p[1] == p[2] { p[0].to_string() } else { format!("{}×{}×{}", p[0], p[1], p[2]) }
}

/// Describe the field-mapping stage: phase offset / bipolar correction, phase
/// unwrapping, and B0 estimation. Shared by the standard pipeline and QSMART
/// (which consumes a pre-computed total field, so these steps still run).
fn describe_field_mapping(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let fm = &config.field_mapping;
    let d = FieldMappingConfig::default();
    let m = &config.motion_correction;
    // Decided per run ([`Topic::MotionCorrection`]), worded as MEDI+0 is: applied to every run,
    // plainly; to none, that it was not; where the runs differ, or nothing is known of them, with
    // the condition.
    if m.enabled && ran.only(Topic::MotionCorrection, &[Choice::NotApplied]) {
        doc.say(&[Stage::Motion],
            "Motion correction was not applied: it needs at least two echoes with magnitude images, and no run had them.");
    } else if m.enabled {
        let scope = if ran.only(Topic::MotionCorrection, &[Choice::Applied]) { "" }
            else { ", in runs with more than one echo and magnitude images" };
        // The reference echo says what was done (which echo the others were moved onto), so it
        // is always named; the registration settings only when changed.
        let reference = match m.reference {
            crate::motion::MotionReference::First => "the first echo".to_string(),
            crate::motion::MotionReference::Last => "the last echo".to_string(),
            crate::motion::MotionReference::Echo(n) => format!("echo {n}"),
        };
        let mut changed = registration_changes(config);
        changed.add_fields(m, &crate::motion::MotionCorrectionConfig::default(),
            &["enabled", "reference", "min_ncc"]);
        doc.say(&[Stage::Motion], format!(
            "Before field mapping{scope}, every echo was rigidly co-registered to {reference} by maximising the normalised cross-correlation of the magnitude images over a multi-resolution pyramid{}, and the phase was resampled with it through the complex signal; the brain mask was restricted to voxels covered by every echo.",
            changed.paren()));
    }
    // Phase offset removal and/or bipolar correction (ROMEO only: the Laplacian path skips both)
    let romeo = fm.unwrapping_algorithm == UnwrappingAlgorithm::Romeo;
    let sigma = if fm.phase_offset_sigma_mm.iter().zip(d.phase_offset_sigma_mm).any(|(a, b)| differs(*a, b)) {
        format!("; smoothing σ = {} mm",
            fm.phase_offset_sigma_mm.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" × "))
    } else {
        String::new()
    };
    // MEDI's complex fit fits its own phase offset, and takes no bipolar correction.
    if fm.b0_estimation == B0Estimation::ComplexFit {
        describe_complex_fit(config, doc);
        return;
    }
    match (fm.phase_offset_removal && romeo, fm.bipolar_correction && romeo) {
        (true, true) => {
            doc.cite(&CITE_MCPC3DS);
            doc.cite(&CITE_BIPOLAR);
            doc.say(&[Stage::FieldMap], format!("Phase offset removal (Eckstein et al., 2018{sigma}) and bipolar gradient correction (Eckstein, 2021) were applied."));
        }
        (true, false) => {
            doc.cite(&CITE_MCPC3DS);
            doc.say(&[Stage::FieldMap], format!("Phase offset removal was performed using the HIP method (Eckstein et al., 2018{sigma})."));
        }
        (false, true) => {
            doc.cite(&CITE_BIPOLAR);
            doc.say(&[Stage::FieldMap], "Bipolar gradient correction (Eckstein, 2021) was applied.".to_string());
        }
        (false, false) => {}
    }

    // Unwrapping
    match fm.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => {
            let mode = if fm.romeo.individual == d.romeo.individual {
                ""
            } else if fm.romeo.individual {
                " in individual per-echo mode"
            } else {
                " in template-based temporal mode"
            };
            let mut changed = Changed::default();
            changed.add_fields(&fm.romeo, &d.romeo, &["individual"]);
            doc.cite(&CITE_ROMEO);
            doc.say(&[Stage::FieldMap], format!("Phase unwrapping was performed using ROMEO (Dymerska et al., 2021){mode}{}.", changed.with()));
        }
        UnwrappingAlgorithm::Laplacian => {
            let solver = match fm.laplacian_solver {
                _ if fm.laplacian_solver == d.laplacian_solver => String::new(),
                LaplacianSolver::Dct => " solved as unweighted least squares with a Neumann-boundary DCT (Ghiglia & Romero, 1994)".to_string(),
                LaplacianSolver::Fft => " with an FFT Poisson solve".to_string(),
                LaplacianSolver::Mirror => " with an FFT Poisson solve on the volume mirrored along each axis".to_string(),
            };
            // The phase is set to zero outside the brain mask first unless it is unwrapped raw.
            let whole = if fm.laplacian_unmasked { ", on the raw phase over the whole volume," } else { "" };
            // The padding only exists for the FFT solve.
            let pad = if fm.laplacian_solver == LaplacianSolver::Fft && fm.laplacian_fft_pad != d.laplacian_fft_pad {
                format!(" on the volume zero-padded by {} voxels", describe_pad(fm.laplacian_fft_pad))
            } else {
                String::new()
            };
            doc.cite(&CITE_LAPLACIAN_UNWRAP);
            doc.say(&[Stage::FieldMap], format!(
                "Phase unwrapping was performed separately for each echo{whole} using the Laplacian method (Schofield & Zhu, 2003){solver}{pad}."));
        }
    }

    // B0 estimation. The linear fit's settings are shared with the robust fit.
    let mut linear = Changed::default();
    linear.add_fields(&fm.linear_fit, &d.linear_fit, &[]);
    match fm.b0_estimation {
        B0Estimation::WeightedAvg => {
            if fm.b0_weight_type == B0WeightType::AssumedDecay {
                let t2star = if differs(fm.b0_weight_assumed_t2star_ms, d.b0_weight_assumed_t2star_ms) {
                    format!(" of {} ms", fm.b0_weight_assumed_t2star_ms)
                } else {
                    String::new()
                };
                doc.cite(&CITE_UKB_QSM);
                doc.say(&[Stage::FieldMap], format!(
                    "The B0 field map was estimated as a weighted average of the unwrapped echo phases with per-echo weights TE·exp(−TE/T2*), computed from the echo times and an assumed T2*{t2star} and identical in every voxel (Wang et al., 2022)."));
            } else {
                let mut changed = Changed::default();
                changed.add(fm.b0_weight_type != d.b0_weight_type, || format!("{} weighting", fm.b0_weight_type));
                doc.say(&[Stage::FieldMap], format!("The B0 field map was estimated as a weighted average of the unwrapped echo phases{}.", changed.with()));
            }
        }
        B0Estimation::LinearFit => {
            doc.say(&[Stage::FieldMap], format!("The B0 field map was estimated using magnitude-weighted linear fit of phase vs echo time{}.", linear.with()));
        }
        B0Estimation::RobustFit => {
            let (r, rd) = (&fm.robust_fit, &d.robust_fit);
            let conventional = match r.loss { RobustLoss::Tukey => 4.685, RobustLoss::Huber => 1.345 };
            let mut changed = Changed::default();
            changed.add(r.loss != rd.loss, || format!("a {} loss", if r.loss == RobustLoss::Tukey { "Tukey biweight" } else { "Huber" }));
            changed.add(r.tuning.is_some_and(|t| differs(t, conventional)), || format!("tuning constant {}", r.tuning.unwrap_or(conventional)));
            changed.0.extend(linear.0);
            doc.say(&[Stage::FieldMap], if r.iterations == 0 {
                format!("The B0 field map was estimated using magnitude-weighted linear fit of phase vs echo time, with per-echo residual statistics reported to flag echoes inconsistent with the fit{}.",
                    if changed.is_empty() { String::new() } else { format!("; {}", join_list(&changed.0)) })
            } else {
                changed.add(r.iterations != rd.iterations, || format!("{} iterations", r.iterations));
                format!("The B0 field map was estimated with a motion-robust magnitude-weighted linear fit of phase vs echo time by iteratively reweighted least squares{}.", changed.with())
            });
        }
        B0Estimation::QsmartEchofit => {
            doc.cite(&CITE_QSMART);
            let threshold = match fm.qsmart_fit_threshold() {
                qsm_core::utils::QsmartFitThreshold::Percentile(p) => format!("its {p}th percentile"),
                qsm_core::utils::QsmartFitThreshold::Absolute(t) => format!("{t}"),
            };
            // The voxels the fit marks unreliable are left out of QSMART only.
            let excluded = if config.inversion.algorithm == QsmAlgorithm::Qsmart {
                format!(", and voxels whose blurred fit residual exceeded {threshold} were excluded from the QSMART reconstruction")
            } else {
                String::new()
            };
            doc.say(&[Stage::FieldMap], format!(
                "The B0 field map was estimated by a magnitude-weighted fit of phase vs echo time through the origin, as in QSMART (Yaghmaie et al., 2021){excluded}."));
        }
        // Worded by describe_complex_fit, which returned above.
        B0Estimation::ComplexFit => {}
    }
}

/// MEDI's field map (QSM.rs `B0EstimationMethod::ComplexFit`): `Fit_ppm_complex` on the complex
/// echoes, then the fitted phase per echo spacing unwrapped and divided by the echo spacing. No
/// phase offsets are removed first (the fit has its own).
fn describe_complex_fit(config: &PipelineConfig, doc: &mut Draft) {
    let (fm, d) = (&config.field_mapping, FieldMappingConfig::default());
    let mut fit = Changed::default();
    fit.add_fields(&fm.complex_fit, &d.complex_fit, &[]);
    doc.cite(&CITE_FIT_PPM_COMPLEX);
    let unwrapping = match fm.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => {
            let mut changed = Changed::default();
            changed.add_fields(&fm.romeo, &d.romeo, &["individual", "template"]);
            doc.cite(&CITE_ROMEO);
            format!("ROMEO (Dymerska et al., 2021){}", changed.with())
        }
        UnwrappingAlgorithm::Laplacian => {
            let solver = if fm.laplacian_solver == d.laplacian_solver {
                String::new()
            } else {
                format!(" with the {} solver", fm.laplacian_solver)
            };
            doc.cite(&CITE_LAPLACIAN_UNWRAP);
            format!("the Laplacian method (Schofield & Zhu, 2003){solver}")
        }
    };
    doc.say(&[Stage::FieldMap], format!(
        "The B0 field map was estimated by a magnitude-weighted nonlinear fit of the complex echo signal (Fit_ppm_complex, MEDI toolbox; Liu et al., 2013{}); the fitted phase per echo spacing was unwrapped using {unwrapping} and divided by the echo spacing.",
        fit.semi()));
}

/// Prose for a list of mask refinement ops, in the order they run.
/// An op's settings are compared with what the op is when written without arguments
/// (`erode` is one iteration, `gaussian` is 4 mm), so only the ones the user chose are stated.
fn describe_refinements(ops: &[MaskOp], doc: &mut Draft) -> Vec<String> {
    let default_of = |spec: &str| parse_mask_op(spec).expect("bare mask op parses");
    ops.iter().map(|op| match op {
        MaskOp::Erode { iterations } | MaskOp::Dilate { iterations } => {
            let (name, spec) = if matches!(op, MaskOp::Erode { .. }) { ("erosion", "erode") } else { ("dilation", "dilate") };
            if *op == default_of(spec) {
                name.to_string()
            } else {
                format!("{name} ({iterations} iteration{})", if *iterations != 1 { "s" } else { "" })
            }
        }
        MaskOp::Close { radius } if *op != default_of("close") => format!("morphological closing (radius {radius} voxels)"),
        MaskOp::Close { .. } => "morphological closing".to_string(),
        MaskOp::FillHoles { max_size: 0 } => "hole-filling".to_string(),
        MaskOp::FillHoles { max_size } => format!("hole-filling (holes of at most {} voxels)", max_size),
        MaskOp::GaussianSmooth { sigma_mm } if *op != default_of("gaussian") => format!("Gaussian smoothing (σ = {sigma_mm} mm)"),
        MaskOp::GaussianSmooth { .. } => "Gaussian smoothing".to_string(),
        MaskOp::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } => {
            doc.cite(&CITE_QSMCI_SIGNAL_EROSION);
            let MaskOp::SignalErode {
                threshold: dt, depth_cap: dd, global_erosions: dg, bias_sigma: db, min_component: dm,
            } = MaskOp::signal_erode_default() else { unreachable!() };
            let mut changed = Changed::default();
            changed.add(*global_erosions != dg, || format!("{} plain erosion{} first", global_erosions, if *global_erosions != 1 { "s" } else { "" }));
            changed.add(differs(*threshold, dt), || format!("threshold {threshold:.2} of the median bias-corrected magnitude"));
            changed.add(*depth_cap != dd, || if *depth_cap == 0 { "no depth limit".to_string() } else { format!("at most {depth_cap} voxels deep") });
            changed.add(differs(*bias_sigma, db), || format!("bias-field σ = {bias_sigma}"));
            changed.add(*min_component != dm, || format!("components under {min_component} voxels discarded"));
            format!("signal-gated erosion (removal of low-signal boundary voxels{}; QSM-CI)",
                if changed.is_empty() { String::new() } else { format!(", {}", join_list(&changed.0)) })
        }
        _ => format!("{}", op),
    }).collect()
}

/// Prose for a mask section's input image and generator op — "Otsu thresholding of the ROMEO
/// phase quality map", and so on. Shared by the main mask and the two-pass reliable mask, so the
/// same recipe never gets described two different ways.
fn describe_generator(
    section: &MaskSection, config: &PipelineConfig, doc: &mut Draft,
) -> String {
    // Input description — clarify which magnitude goes into ROMEO
    let corrected = config.masking.inhomogeneity_correction;
    let input_desc = match section.input {
        MaskingInput::PhaseQuality => {
            doc.cite(&CITE_ROMEO);
            if corrected {
                "the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude)"
            } else {
                "the ROMEO phase quality map (computed from phase data and the RSS-combined magnitude)"
            }
        }
        MaskingInput::Magnitude => {
            if corrected { "the inhomogeneity-corrected RSS-combined magnitude image" }
            else { "the RSS-combined magnitude image" }
        }
        MaskingInput::MagnitudeFirst => {
            if corrected { "the inhomogeneity-corrected first-echo magnitude image" }
            else { "the first-echo magnitude image" }
        }
        MaskingInput::MagnitudeLast => {
            if corrected { "the inhomogeneity-corrected last-echo magnitude image" }
            else { "the last-echo magnitude image" }
        }
    };

    match &section.generator {
        MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => {
            doc.cite(&CITE_OTSU);
            format!("Otsu thresholding (Otsu, 1979) of {}", input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => {
            format!("fixed thresholding (value={:.4}) of {}", value.unwrap_or(0.5), input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } => {
            format!("percentile thresholding ({}th percentile) of {}", value.unwrap_or(75.0), input_desc)
        }
        MaskOp::Bet { fractional_intensity, voxel_scale } => {
            doc.cite(&CITE_BET);
            let MaskOp::Bet { fractional_intensity: df, .. } = parse_mask_op("bet").expect("bare bet parses")
                else { unreachable!() };
            let mut changed = Changed::default();
            changed.add(differs(*fractional_intensity, df), || format!("f={fractional_intensity:.2}"));
            // The surface settings, from the shared BET config.
            let (b, d) = (&config.bet, crate::config::BetConfig::default());
            changed.add(differs(b.smoothness, d.smoothness), || format!("smoothness {}", b.smoothness));
            changed.add(differs(b.gradient_threshold, d.gradient_threshold), || format!("gradient threshold {}", b.gradient_threshold));
            changed.add(b.iterations != d.iterations, || format!("{} iterations", b.iterations));
            changed.add(b.subdivisions != d.subdivisions, || format!("icosphere subdivision level {}", b.subdivisions));
            // A scaled run did not see the acquisition's own geometry, which changes what BET's
            // millimetre-scale surface model was fitted to.
            changed.add(differs(*voxel_scale, crate::masking::bet_default_voxel_scale()),
                || format!("voxel sizes scaled by {voxel_scale}x"));
            format!("BET brain extraction (Smith, 2002{}) of {}", changed.semi(), input_desc)
        }
        MaskOp::Rs2Net { tta, tile_step } => {
            doc.cite(&CITE_RS2NET);
            let mut changed = Changed::default();
            changed.add(differs(*tile_step, crate::masking::rs2_net_default_tile_step()),
                || format!("sliding-window patches stepped by {:.0}% of the patch", tile_step * 100.0));
            changed.add(*tta, || "mirroring test-time augmentation".to_string());
            format!("RS2-Net deep-learning rodent brain extraction (Lin et al., 2024{}) of {}", changed.semi(), input_desc)
        }
        MaskOp::MindGrab { tta } => {
            doc.cite(&CITE_MINDGRAB);
            let mut changed = Changed::default();
            changed.add(*tta, || "left-right flip test-time augmentation".to_string());
            format!("MindGrab deep-learning brain extraction (Fani et al., 2025{}) of {}", changed.semi(), input_desc)
        }
        MaskOp::HdBet { patch, tta, tile_step } => {
            doc.cite(&CITE_HDBET);
            let mut changed = Changed::default();
            if let Some(p) = patch {
                changed.add(true, || format!("{}x{}x{}-voxel sliding-window patches", p[0], p[1], p[2]));
                changed.add(differs(*tile_step, crate::masking::hd_bet_default_tile_step()),
                    || format!("windows stepped by {:.0}% of the patch", tile_step * 100.0));
            }
            changed.add(*tta, || "mirroring test-time augmentation".to_string());
            format!("HD-BET deep-learning brain extraction (Isensee et al., 2019{}) of {}", changed.semi(), input_desc)
        }
        _ => format!("{} of {}", section.generator, input_desc),
    }
}

/// Prose for two-pass artefact reduction, including the reliable-pass mask recipe.
///
/// Named as "two-pass" throughout, matching both the paper and the `_desc-singlepass_` derivative
/// that carries the other pass's map.
fn describe_two_pass(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let not_applied = ran.count(Topic::TwoPass, Choice::NotApplied);
    if !config.masking.two_pass || (not_applied > 0 && ran.ran(Stage::Mask(Pass::Reliable)) == 0) {
        return;
    }
    doc.cite(&CITE_TWOPASS);

    let sections = config.masking.resolved_two_pass_sections();
    let mut descs = Vec::new();
    for section in &sections {
        let mut parts = vec![describe_generator(section, config, doc)];
        let refinements = describe_refinements(&section.refinements, doc);
        if !refinements.is_empty() {
            parts.push(format!("followed by {}", join_list(&refinements)));
        }
        descs.push(parts.join(", "));
    }
    let generated = if descs.len() == 1 {
        format!("generated using {}", descs[0])
    } else {
        let (op, how) = match config.masking.resolved_two_pass_combine() {
            MaskCombine::Or => ("OR", "union"),
            MaskCombine::And => ("AND", "intersection"),
        };
        format!("generated by combining {} mask sections ({op} operation, i.e. their {how}): {}",
                descs.len(), join_list(&descs))
    };
    let refined = describe_refinements(&config.masking.refinements, doc);
    let refined = if refined.is_empty() { String::new() } else { format!(" and refined by {}", join_list(&refined)) };

    doc.say(&two_pass_steps(), format!(
        "Two-pass artefact reduction (Stewart et al., 2022) was applied: background field removal \
         and dipole inversion were repeated against a second mask {generated}{refined}, whose holes \
         around strong susceptibility sources were left unfilled, and the two susceptibility maps \
         were combined by taking the second reconstruction wherever it was defined and the first \
         elsewhere.",
    ));
    if not_applied > 0 {
        doc.say(&two_pass_steps(), "It was not applied where the brain mask was sourced from the BIDS derivatives, \
                                    which leaves no recipe for the second mask.".to_string());
    }
}

fn describe_masking(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    if config.masking.sections.is_empty() {
        return;
    }

    // Prefer a bring-your-own mask from BIDS derivatives, where available. Every run having used
    // one, or none, says which.
    let tool = config.masking.custom_mask_tool.as_deref();
    let all_supplied = tool.is_some() && ran.only(Topic::MaskSource, &[Choice::Supplied]);
    let none_supplied = ran.only(Topic::MaskSource, &[Choice::Computed]);
    match tool {
        // One run: its mask was the one supplied with it.
        Some(tool) if all_supplied && ran.runs() == 1 => {
            let from = if tool == "*" { "its".to_string() } else { format!("the '{tool}'") };
            doc.say(&[Stage::Mask(Pass::Main)], format!("The brain mask was supplied with the scan, from {from} BIDS derivatives."));
        }
        Some(tool) if all_supplied => {
            let from = if tool == "*" { String::new() } else { format!("'{tool}' ") };
            doc.say(&[Stage::Mask(Pass::Main)], format!("Brain masks were sourced from the {from}BIDS derivatives."));
        }
        _ if none_supplied => {}
        Some("*") => doc.say(&[Stage::Mask(Pass::Main)], 
            "Where available, existing brain masks were sourced from the BIDS derivatives; \
             otherwise they were generated as follows.".to_string()),
        Some(tool) => doc.say(&[Stage::Mask(Pass::Main)], format!(
            "Where available, brain masks were sourced from the '{}' BIDS derivatives; \
             otherwise they were generated as follows.", tool)),
        None => {}
    }

    // Describe inhomogeneity correction if enabled, with context about what it affects.
    // The RSS-combined magnitude is used for both Magnitude and PhaseQuality masking inputs,
    // while MagnitudeFirst/MagnitudeLast use their specific echo magnitudes.
    if config.masking.inhomogeneity_correction {
        let mut changed = Changed::default();
        let correction = match config.masking.inhomogeneity_method {
            InhomogeneityMethod::Makehomogeneous => {
                doc.cite(&CITE_BIAS);
                changed.add_fields(&config.homogeneity, &HomogeneityConfig::default(), &[]);
                "Inhomogeneity correction (Eckstein et al., 2019"
            }
            InhomogeneityMethod::N4 => {
                doc.cite(&CITE_N4);
                let (n4, d) = (&config.n4, N4Config::default());
                if n4.max_iterations != d.max_iterations {
                    let levels: Vec<String> = n4.max_iterations.iter().map(|i| i.to_string()).collect();
                    let n = levels.len();
                    changed.0.push(format!("at most {} iterations at {n} fitting level{}",
                                           join_list(&levels), if n == 1 { "" } else { "s" }));
                }
                changed.add_fields(n4, &d, &["max_iterations"]);
                "N4 inhomogeneity correction (Tustison et al., 2010"
            }
        };
        let settings = changed.semi();

        let inputs: Vec<MaskingInput> = config.masking.sections.iter().map(|s| s.input).collect();
        let uses_rss = inputs.iter().any(|i| matches!(i, MaskingInput::Magnitude | MaskingInput::PhaseQuality));
        let uses_first = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeFirst));
        let uses_last = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeLast));

        if uses_rss && !uses_first && !uses_last {
            doc.say(&[Stage::Magnitude], format!("{correction}{settings}) was applied to the RSS-combined magnitude image."));
        } else if !uses_rss && (uses_first || uses_last) {
            let echo = if uses_first { "first" } else { "last" };
            doc.say(&[Stage::Magnitude], format!(
                "{correction}{settings}) was applied to the {}-echo magnitude image.",
                echo
            ));
        } else {
            doc.say(&[Stage::Magnitude], format!("{correction}{settings}) was applied to the magnitude data."));
        }
    }

    // No run generated a mask: there is no recipe to describe.
    if all_supplied {
        return;
    }

    let section_count = config.masking.sections.len();
    let mut section_descs = Vec::new();

    for section in &config.masking.sections {
        let mut parts = vec![describe_generator(section, config, doc)];

        // Refinements
        let refinement_descs = describe_refinements(&section.refinements, doc);

        if !refinement_descs.is_empty() {
            parts.push(format!("followed by {}", join_list(&refinement_descs)));
        }

        section_descs.push(parts.join(", "));
    }

    if section_count == 1 {
        doc.say(&[Stage::Mask(Pass::Main)], format!("A brain mask was generated using {}.", section_descs[0]));
    } else {
        let (op, how) = match config.masking.combine {
            MaskCombine::Or => ("OR", "union"),
            MaskCombine::And => ("AND", "intersection"),
        };
        doc.say(&[Stage::Mask(Pass::Main)], format!(
            "A brain mask was generated by combining {} mask sections ({} operation, i.e. their {}): {}.",
            section_count, op, how, join_list(&section_descs)
        ));
    }

    let combined_descs = describe_refinements(&config.masking.refinements, doc);
    if !combined_descs.is_empty() {
        doc.say(&[Stage::Mask(Pass::Main)], format!(
            "The {} mask was then refined by {}.",
            if section_count == 1 { "resulting" } else { "combined" },
            join_list(&combined_descs)
        ));
    }

    // Exactly the consensus recipe — say so, and cite it.
    let consensus = bet_and_phase_mask_recipe();
    if config.masking.sections == consensus.sections
        && config.masking.combine == consensus.combine
        && config.masking.refinements == consensus.refinements
    {
        doc.cite(&CITE_QSM_CONSENSUS);
        doc.say(&[Stage::Mask(Pass::Main)], 
            "This masking follows the recommendation of the ISMRM electro-magnetic tissue \
             properties study group consensus (Bilgic et al., 2024).".to_string());
    }
}

/// Display name + citation for a background-field removal algorithm.
fn bg_removal_name_cite(alg: BfAlgorithm) -> (&'static str, &'static Citation) {
    match alg {
        BfAlgorithm::Vsharp => ("V-SHARP", &CITE_VSHARP),
        BfAlgorithm::Pdf => ("PDF", &CITE_PDF),
        BfAlgorithm::Lbv => ("LBV", &CITE_LBV),
        BfAlgorithm::Ismv => ("iSMV", &CITE_ISMV),
        BfAlgorithm::Sharp => ("SHARP", &CITE_SHARP),
        BfAlgorithm::Resharp => ("RESHARP", &CITE_RESHARP),
        BfAlgorithm::Harperella => ("HARPERELLA", &CITE_HARPERELLA),
        BfAlgorithm::Iharperella => ("iHARPERELLA", &CITE_IHARPERELLA),
        BfAlgorithm::Bfrnet => ("BFRnet (deep-learning background removal)", &CITE_BFRNET),
        BfAlgorithm::Iqfm => ("iQFM (deep-learning joint unwrapping + background removal)", &CITE_IQSM),
    }
}

/// What the pipeline graph ([`crate::pipeline_graph`]) says about a step's algorithm, in the
/// methods text's words: so a box and the sentence describing the same step cannot disagree.
pub(crate) mod words {
    use super::*;

    /// The name the methods text gives an algorithm, without what follows it in parentheses:
    /// "RTS (Rapid Two-Step)" is "RTS" in a box.
    fn short(name: &'static str) -> String {
        name.split(" (").next().unwrap_or(name).to_string()
    }

    pub(crate) fn inversion(alg: QsmAlgorithm) -> String {
        short(inversion_name_cite(alg).0)
    }

    pub(crate) fn bg_removal(alg: BfAlgorithm) -> String {
        short(bg_removal_name_cite(alg).0)
    }

    pub(crate) fn separation(alg: SeparationAlgorithm) -> String {
        short(separation_name_cite(alg).0)
    }

    /// The selected inversion's settings that differ from their defaults (`"λ = 0.002"`).
    pub(crate) fn inversion_settings(config: &PipelineConfig, alg: QsmAlgorithm) -> Vec<String> {
        inversion_changes(config, alg).0
    }

    /// The selected background removal's settings: V-SHARP's kernel radii always (they are what
    /// says which V-SHARP ran), and otherwise what differs from the defaults.
    pub(crate) fn bg_removal_settings(config: &PipelineConfig) -> Vec<String> {
        let b = &config.bg_removal;
        if b.algorithm != BfAlgorithm::Vsharp {
            return bg_removal_changes(config).0;
        }
        let (v, d) = (&b.vsharp, VsharpConfig::default());
        let mut c = Changed::default();
        c.add(true, || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
        c.add(v.deconvolve != d.deconvolve, || format!(
            "truncated-SVD deconvolution {}", if v.deconvolve { "enabled" } else { "disabled" }));
        c.add(v.deconvolve && differs(v.threshold, d.threshold), || format!("deconvolution threshold = {}", v.threshold));
        c.0
    }

    pub(crate) fn separation_settings(config: &PipelineConfig) -> Vec<String> {
        separation_changes(config).0
    }

    pub(crate) fn registration_settings(config: &PipelineConfig) -> Vec<String> {
        registration_changes(config).0
    }

    /// ROMEO's settings that differ from their defaults; Laplacian unwrapping's solver, when it
    /// is not the default one, and the raw phase, when it is unwrapped unmasked.
    pub(crate) fn unwrapping_settings(config: &PipelineConfig) -> Vec<String> {
        let (fm, d) = (&config.field_mapping, FieldMappingConfig::default());
        let mut c = Changed::default();
        if fm.unwrapping_algorithm == UnwrappingAlgorithm::Romeo {
            c.add_fields(&fm.romeo, &d.romeo, &[]);
        } else {
            c.add(fm.laplacian_solver != d.laplacian_solver, || format!("{} solver", fm.laplacian_solver));
            c.add(fm.laplacian_unmasked != d.laplacian_unmasked, || "raw phase over the whole volume".to_string());
        }
        c.0
    }

    /// Each mask section as one line: its input, how the mask is made from it and how it is
    /// refined ("phase quality: Otsu threshold, dilation, hole-filling, erosion").
    pub(crate) fn mask_sections(sections: &[MaskSection]) -> Vec<String> {
        sections.iter().map(|section| {
            let input = match section.input {
                MaskingInput::PhaseQuality => "phase quality",
                MaskingInput::Magnitude => "magnitude",
                MaskingInput::MagnitudeFirst => "first-echo magnitude",
                MaskingInput::MagnitudeLast => "last-echo magnitude",
            };
            let generator = match &section.generator {
                MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => "Otsu threshold".to_string(),
                MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => format!("threshold {}", value.unwrap_or(0.5)),
                MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } =>
                    format!("{}th-percentile threshold", value.unwrap_or(75.0)),
                MaskOp::Bet { .. } => "BET".to_string(),
                MaskOp::HdBet { .. } => "HD-BET".to_string(),
                MaskOp::Rs2Net { .. } => "RS2-Net".to_string(),
                MaskOp::MindGrab { .. } => "MindGrab".to_string(),
                other => describe_refinements(std::slice::from_ref(other), &mut Draft::default()).join(", "),
            };
            let steps: Vec<String> = std::iter::once(generator)
                .chain(describe_refinements(&section.refinements, &mut Draft::default()))
                .collect();
            format!("{input}: {}", steps.join(", "))
        }).collect()
    }

    /// QSMART's settings that differ from their defaults (its inner inversion is said apart).
    pub(crate) fn qsmart_settings(config: &PipelineConfig) -> Vec<String> {
        let mut c = qsmart_changes(config);
        c.add(config.inversion.qsmart.vessel_magnitude == QsmartVesselMagnitude::Combined,
            || "vessels on the combined magnitude".to_string());
        c.0
    }
}

/// The network that reconstructs from the wrapped phase (iQFM, iQSM or iQSM+) a run uses, by
/// name, if any. Tiled, it only approximates its whole-volume result.
pub(crate) fn tiled_phase_network(config: &PipelineConfig) -> Option<&'static str> {
    match config.inversion.algorithm {
        QsmAlgorithm::Iqsm => Some("iQSM"),
        QsmAlgorithm::IqsmPlus => Some("iQSM+"),
        _ if config.bg_removal.algorithm == BfAlgorithm::Iqfm
            && !crate::bridge::skips_bgremove(config) => Some("iQFM"),
        _ => None,
    }
}

/// The sentence saying the networks ran on overlapping tiles, when `inversion.tile_size` is set.
/// The halo is said only when it is not the default the runner fills in. For iQFM, iQSM and
/// iQSM+ it also says that this approximates the whole-volume result.
fn say_tiling(config: &PipelineConfig, stage: fn(Pass) -> Stage, doc: &mut Draft) {
    let Some(core) = config.inversion.tile_size else { return };
    let halo = match config.inversion.tile_halo {
        Some(h) if h != DEFAULT_TILE_HALO => format!(", halo {h} voxels"),
        _ => String::new(),
    };
    doc.say(&both(stage), match tiled_phase_network(config) {
        Some(network) => format!(
            "{network} was run on overlapping tiles (tile size {core} voxels{halo}), which \
             approximates its whole-volume result.",
        ),
        None => format!("Network inference was run on overlapping tiles (tile size {core} voxels{halo})."),
    });
}

/// Display name + citation for a dipole inversion algorithm. Shared by the
/// standard pipeline's inversion sentence and QSMART's inner inversion.
/// Tgv/Qsmart are not valid dipole inversions; they fall back to iLSQR
/// defensively (not reached in practice).
fn inversion_name_cite(alg: QsmAlgorithm) -> (&'static str, &'static Citation) {
    match alg {
        QsmAlgorithm::Rts => ("RTS (Rapid Two-Step)", &CITE_RTS),
        QsmAlgorithm::Tv => ("Total Variation (TV-ADMM)", &CITE_TV),
        QsmAlgorithm::Tkd => ("TKD (Thresholded K-space Division)", &CITE_TKD),
        QsmAlgorithm::Tsvd => ("TSVD (Truncated Singular Value Decomposition)", &CITE_TKD),
        QsmAlgorithm::Tikhonov => ("Tikhonov regularization", &CITE_TIKHONOV),
        QsmAlgorithm::Nltv => ("NLTV (Nonlinear Total Variation)", &CITE_NLTV),
        QsmAlgorithm::Medi => ("MEDI (Morphology Enabled Dipole Inversion)", &CITE_MEDI),
        QsmAlgorithm::Tfi => ("TFI (Total Field Inversion)", &CITE_TFI),
        QsmAlgorithm::Ilsqr => ("iLSQR", &CITE_ILSQR),
        QsmAlgorithm::Star => ("STAR-QSM (Streaking Artifact Reduction)", &CITE_STAR),
        QsmAlgorithm::Lsqr => ("minimally regularised LSQR", &CITE_LSQR),
        QsmAlgorithm::Heidi => ("HEIDI (Homogeneity Enabled Incremental Dipole Inversion)", &CITE_HEIDI),
        QsmAlgorithm::Ndi => ("NDI (Nonlinear Dipole Inversion)", &CITE_NDI),
        QsmAlgorithm::Fansi => ("FANSI (Nonlinear TV)", &CITE_NLTV),
        QsmAlgorithm::FansiTgv => ("FANSI (Nonlinear TGV)", &CITE_NLTV),
        QsmAlgorithm::L1qsm => ("L1-QSM (L1 Data Fidelity)", &CITE_L1QSM),
        QsmAlgorithm::Whqsm => ("WH-QSM (Weak-Harmonic)", &CITE_WHQSM),
        QsmAlgorithm::Hdqsm => ("HD-QSM (Hybrid Data Fidelity)", &CITE_HDQSM),
        QsmAlgorithm::AmpPe => ("AMP-PE (Approximate Message Passing with Parameter Estimation)", &CITE_AMPPE),
        QsmAlgorithm::Xqsm => ("xQSM (deep-learning dipole inversion)", &CITE_XQSM),
        QsmAlgorithm::Qsmnet => ("QSMnet (deep-learning dipole inversion)", &CITE_QSMNET),
        QsmAlgorithm::QsmnetPlus => ("QSMnet+ (deep-learning dipole inversion)", &CITE_QSMNET_PLUS),
        QsmAlgorithm::Autoqsm => ("AutoQSM (single-step deep-learning reconstruction)", &CITE_AUTOQSM),
        QsmAlgorithm::Qsmgan => ("QSMGAN (GAN-refined deep-learning dipole inversion)", &CITE_QSMGAN),
        QsmAlgorithm::Ir2qsm => ("IR2QSM (unrolled deep-learning dipole inversion)", &CITE_IR2QSM),
        QsmAlgorithm::Lpcnn => ("LPCNN (learned-proximal deep-learning dipole inversion)", &CITE_LPCNN),
        QsmAlgorithm::ModlQsm => ("MoDL-QSM (model-based deep-learning dipole inversion)", &CITE_MODLQSM),
        QsmAlgorithm::Nextqsm => ("NeXtQSM (single-step deep-learning reconstruction)", &CITE_NEXTQSM),
        QsmAlgorithm::Iqsm => ("iQSM (end-to-end deep-learning reconstruction from phase)", &CITE_IQSM),
        QsmAlgorithm::IqsmPlus => ("iQSM+ (orientation-adaptive end-to-end deep-learning reconstruction)", &CITE_IQSM_PLUS),
        QsmAlgorithm::Tgv | QsmAlgorithm::Qsmart => ("iLSQR", &CITE_ILSQR),
    }
}

/// The χ-separation method's name as the methods text gives it.
pub(crate) fn separation_name(alg: SeparationAlgorithm) -> &'static str {
    separation_name_cite(alg).0
}

fn separation_name_cite(alg: SeparationAlgorithm) -> (&'static str, &'static Citation) {
    match alg {
        SeparationAlgorithm::R2starQsm => ("R2*-QSM susceptibility source separation", &CITE_R2STARQSM),
        SeparationAlgorithm::Decompose => ("DECOMPOSE-QSM", &CITE_DECOMPOSE),
        SeparationAlgorithm::ChiSepIlsqr => ("χ-separation (iLSQR)", &CITE_CHISEP),
        SeparationAlgorithm::ChiSepMedi => ("χ-separation (MEDI)", &CITE_CHISEP),
        SeparationAlgorithm::WaveSep => ("WaveSep", &CITE_WAVESEP),
        SeparationAlgorithm::HcChisep => ("hollow-cylinder χ-separation (HC-ChiSep)", &CITE_HCCHISEP),
        SeparationAlgorithm::SusepNet => ("SUSEP-Net (deep-learning source separation)", &CITE_SUSEPNET),
        SeparationAlgorithm::ChiSepNet => ("χ-sepnet (deep-learning source separation)", &CITE_CHISEPNET),
    }
}

fn cite_inline(cite: &Citation) -> &'static str {
    match cite.key {
        "wu2012" => "Wu et al., 2012",
        "schweser2011" => "Schweser et al., 2011",
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
        "kim2025chisepnet" => "Kim et al. (SNU-LIST)",
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

fn join_list(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}, and {}", rest.join(", "), last)
        }
    }
}

/// `text` (a [`Methods::to_markdown`]) without its `## Note` section.
#[cfg(test)]
fn without_note(text: &str) -> String {
    match text.find("## Note\n") {
        Some(start) => {
            let end = text[start..].find("## References").map_or(text.len(), |e| start + e);
            format!("{}{}", &text[..start], &text[end..])
        }
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The methods text without its Note: what was done, as the paragraph and references say it.
    fn paragraph(config: &PipelineConfig) -> String {
        without_note(&generate_methods(config))
    }

    /// Methods text is prose meant to be pasted into a paper, so a run of whitespace in the middle
    /// of a sentence is a defect. A Rust string continuation that loses its backslash produces
    /// exactly that, and every other assertion in this file still passes when it happens.
    #[test]
    fn methods_prose_has_no_stray_whitespace() {
        let mut config = PipelineConfig::default();
        config.masking.two_pass = true;
        config.pipeline.do_swi = true;
        config.pipeline.do_chi_separation = true;
        crate::config::enforce_separation_dependencies(&mut config);

        for line in generate_methods(&config).lines() {
            assert!(!line.contains("  "), "run of spaces in methods prose: {line:?}");
        }
    }

    /// A measured R2' and an estimated one support different claims, so the methods text has to
    /// distinguish them rather than just say R2' was computed.
    #[test]
    fn r2prime_methods_distinguish_measurement_from_estimate() {
        let cfg = |s| {
            let mut c = PipelineConfig::default();
            c.pipeline.do_r2primemap = true;
            c.separation.r2prime_strategy = s;
            crate::config::enforce_separation_dependencies(&mut c);
            c
        };

        let out = generate_methods(&cfg(crate::enums::R2PrimeStrategy::Mese));
        assert!(out.contains("R2' was computed as R2* - R2"), "out: {out}");
        assert!(out.contains("Negative R2' values were set to zero."), "out: {out}");
        assert!(!out.contains("R2PRIMEnet"), "nothing was estimated: {out}");
        // The measuring route reports its R2 fit, and cites EPG for it.
        assert!(out.contains("Extended Phase Graph"), "out: {out}");
        assert!(out.contains("10.1002/jmri.24619"), "EPG citation missing: {out}");

        let out = generate_methods(&cfg(crate::enums::R2PrimeStrategy::R2primenet));
        assert!(out.contains("estimated from the R2* map with R2PRIMEnet"), "out: {out}");
        assert!(out.contains("Negative R2' values were set to zero."), "out: {out}");
        assert!(out.contains("10.1002/hbm.70136"), "R2PRIMEnet citation missing: {out}");
        // Nothing measured R2 on this route, so it must not claim to have fitted one.
        assert!(!out.contains("Extended Phase Graph"), "out: {out}");

        let out = generate_methods(&cfg(crate::enums::R2PrimeStrategy::Auto));
        assert!(out.contains("where a multi-echo spin-echo acquisition was available"), "out: {out}");
        assert!(out.contains("estimated from R2* with R2PRIMEnet"), "out: {out}");

        // A supplied map is neither, and says so.
        let mut c = cfg(crate::enums::R2PrimeStrategy::Auto);
        c.separation.custom_r2prime_tool = Some("sepia".to_string());
        let out = generate_methods(&c);
        assert!(out.contains("taken from the supplied BIDS derivatives"), "out: {out}");
        assert!(!out.contains("R2PRIMEnet"), "out: {out}");
        assert!(!out.contains("Negative R2'"), "a supplied map is as supplied: {out}");
    }

    #[test]
    fn segmentation_and_analysis_methods_are_stated_and_cited() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_segmentation = true;
        let out = generate_methods(&config);
        assert!(out.contains("SynthSeg v1"), "out: {out}");
        assert!(out.contains("32 FreeSurfer-labelled structures"), "out: {out}");
        assert!(out.contains("parcellated from the GRE magnitude"), "out: {out}");
        assert!(out.contains("10.1016/j.media.2023.102789"), "SynthSeg citation missing: {out}");
        assert!(!out.contains("summarised within every segmented structure"),
                "no statistics were asked for: {out}");

        config.pipeline.do_analysis = true;
        config.segmentation.version = crate::enums::SynthSegVersion::V2;
        let out = generate_methods(&config);
        assert!(out.contains("SynthSeg v2"), "out: {out}");
        assert!(out.contains("33 FreeSurfer-labelled structures"), "out: {out}");
        assert!(out.contains("Each quantitative map was then summarised within every segmented structure"),
                "out: {out}");
        assert!(out.contains("5th/95th percentiles"), "out: {out}");
    }

    #[test]
    fn smwi_methods_name_its_parameters_and_cite_it() {
        let mut config = PipelineConfig::default();
        assert!(!generate_methods(&config).contains("map-weighted"));

        config.pipeline.do_smwi = true;
        config.smwi.threshold_ppm = 0.2;
        let out = generate_methods(&config);
        assert!(out.contains("Susceptibility map-weighted images"), "out: {out}");
        assert!(out.contains("0.20 ppm"), "the threshold must be stated: {out}");
        assert!(out.contains("paramagnetic and diamagnetic"), "out: {out}");
        assert!(out.contains("10.1002/mrm.24920"), "SMWI citation missing: {out}");
    }

    #[test]
    fn lsqr_and_heidi_are_named_and_cited() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Lsqr;
        let out = generate_methods(&config);
        assert!(out.contains("minimally regularised LSQR"), "out: {out}");
        assert!(out.contains("10.1118/1.3481505"), "LSQR citation missing: {out}");

        config.inversion.algorithm = QsmAlgorithm::Heidi;
        let out = generate_methods(&config);
        assert!(out.contains("HEIDI (Homogeneity Enabled Incremental Dipole Inversion)"), "out: {out}");
        assert!(out.contains("10.1016/j.neuroimage.2012.05.067"), "HEIDI citation missing: {out}");
    }

    /// The two-pass sentence has to name the second mask's recipe, say which pass wins where, and
    /// bring the MRM 2022 citation with it.
    #[test]
    fn two_pass_methods_describe_the_reliable_mask() {
        let mut config = PipelineConfig::default();
        assert!(!generate_methods(&config).contains("Two-pass"));

        config.masking.two_pass = true;
        let out = generate_methods(&config);
        assert!(out.contains("Two-pass artefact reduction (Stewart et al., 2022)"), "out: {out}");
        assert!(out.contains("Otsu thresholding (Otsu, 1979) of the ROMEO phase quality map"), "out: {out}");
        assert!(out.contains("left unfilled"), "out: {out}");
        assert!(out.contains("10.1002/mrm.29048"), "two-pass citation missing: {out}");

        // It describes the reconstruction, so it belongs after inversion and before referencing.
        let inv = out.find("Dipole inversion was performed").expect("inversion sentence");
        let two = out.find("Two-pass artefact reduction").expect("two-pass sentence");
        let reference = out.find("referenced to the mean susceptibility").expect("referencing sentence");
        assert!(inv < two && two < reference, "out-of-order methods: {out}");
    }

    /// An edited reliable mask must be described as configured, not as the default.
    #[test]
    fn two_pass_methods_follow_an_edited_reliable_mask() {
        let mut config = PipelineConfig::default();
        config.masking.two_pass = true;
        config.masking.two_pass_sections = Some(vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::bet(0.4),
            refinements: vec![MaskOp::Erode { iterations: 2 }],
        }]);
        let out = generate_methods(&config);
        assert!(out.contains("BET brain extraction (Smith, 2002; f=0.40)"), "out: {out}");
        assert!(out.contains("followed by erosion (2 iterations)"), "out: {out}");
        assert!(!out.contains("phase quality map, whose holes"), "out: {out}");
    }

    /// A reliable mask of several sections says how they were combined: its own mode when set,
    /// else the main mask's.
    #[test]
    fn two_pass_methods_say_how_the_reliable_sections_combine() {
        let mut config = PipelineConfig::default();
        config.masking.two_pass = true;
        let mut sections = default_two_pass_sections();
        sections.push(MaskSection { input: MaskingInput::Magnitude, generator: MaskOp::bet(0.4), refinements: vec![] });
        config.masking.two_pass_sections = Some(sections);
        let two_pass = |c: &PipelineConfig| {
            let out = paragraph(c);
            out[out.find("Two-pass artefact reduction").unwrap()..].split("elsewhere.").next().unwrap().to_string()
        };
        assert!(two_pass(&config).contains("second mask generated by combining 2 mask sections (OR operation, i.e. their union)"),
                "{}", two_pass(&config));
        config.masking.combine = MaskCombine::And;
        assert!(two_pass(&config).contains("(AND operation, i.e. their intersection)"), "{}", two_pass(&config));
        config.masking.two_pass_combine = Some(MaskCombine::Or);
        assert!(two_pass(&config).contains("(OR operation, i.e. their union)"), "{}", two_pass(&config));
    }

    /// A preclinical mask was not fitted to the acquisition's own geometry, which is exactly
    /// the kind of thing a methods section exists to state.
    #[test]
    fn scaled_bet_says_so_in_the_methods() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Bet { fractional_intensity: 0.5, voxel_scale: 10.0 },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("voxel sizes scaled by 10x"),
                "out: {out}");

        // Unscaled BET at the default f states neither.
        config.masking.sections[0].generator = MaskOp::bet(0.5);
        let out = generate_methods(&config);
        assert!(out.contains("BET brain extraction (Smith, 2002) of"), "out: {out}");
        assert!(!out.contains("voxel sizes scaled"), "out: {out}");
        assert!(!out.contains("f="), "out: {out}");
    }

    /// The `bet-and-phase` recipe names the operation, the intersection it means, and the
    /// consensus it comes from — with the citation in the reference list.
    #[test]
    fn test_consensus_masking_methods() {
        let recipe = bet_and_phase_mask_recipe();
        let mut config = PipelineConfig::default();
        config.masking.sections = recipe.sections.clone();
        config.masking.combine = recipe.combine;
        config.masking.refinements = recipe.refinements.clone();

        let out = generate_methods(&config);
        assert!(out.contains("AND operation, i.e. their intersection"), "out: {out}");
        assert!(out.contains("The combined mask was then refined by hole-filling"), "out: {out}");
        assert!(out.contains("electro-magnetic tissue properties study group consensus"), "out: {out}");
        assert!(out.contains("10.1002/mrm.30006"), "consensus citation missing: {out}");

        // A different recipe combined the same way is not the consensus one, and must not claim to be.
        config.masking.refinements.clear();
        let out = generate_methods(&config);
        assert!(out.contains("AND operation"));
        assert!(!out.contains("study group consensus"), "claimed the consensus for a modified recipe: {out}");
    }

    #[test]
    fn vsharp_methods_state_only_changed_settings() {
        let mut config = PipelineConfig::default();
        let out = generate_methods(&config);
        assert!(out.contains("Background field removal was performed using V-SHARP (Wu et al., 2012)."), "{out}");
        assert!(!out.contains("deconvolution"), "{out}");
        assert!(!out.contains("radii"), "{out}");
        config.bg_removal.vsharp.deconvolve = true;
        let out = generate_methods(&config);
        assert!(out.contains("V-SHARP (Wu et al., 2012) with truncated-SVD deconvolution enabled."), "{out}");
        assert!(!out.contains("deconvolution threshold"), "the default threshold is not stated: {out}");
        config.bg_removal.vsharp.threshold = 0.1;
        config.bg_removal.vsharp.max_radius = 8.0;
        let out = generate_methods(&config);
        assert!(out.contains("with SMV kernel radii from 8 to 2 mm in 1 mm steps, truncated-SVD deconvolution enabled, and a deconvolution threshold of 0.1."), "{out}");
    }

    /// χ-sep iLSQR says it reconstructed the QSM it separated, and any iLSQR setting changed.
    #[test]
    fn chi_sep_ilsqr_methods_state_its_own_qsm() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_chi_separation = true;
        config.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        let own = "The susceptibility map separated was reconstructed from the local field by iLSQR (Li et al., 2015)";
        let out = generate_methods(&config);
        assert!(out.contains(&format!("{own}.")), "{out}");
        config.separation.chi_sep_ilsqr.qsm_ilsqr.max_iter = 40;
        assert!(generate_methods(&config).contains(&format!("{own} with at most 40 iterations.")));
        config.separation.custom_qsm_tool = Some("qsmxt".into());
        assert!(!generate_methods(&config).contains(own));
        config.separation.custom_qsm_tool = None;
        config.separation.algorithm = SeparationAlgorithm::ChiSepMedi;
        assert!(!generate_methods(&config).contains(own));
    }

    #[test]
    fn vsharp_methods_describe_the_working_grid() {
        let mut config = PipelineConfig::default();
        let out = paragraph(&config);
        assert!(out.contains("estimated on an isotropic grid: the field"), "the default: {out}");
        config.bg_removal.vsharp.grid = VsharpGrid::Finest;
        let out = paragraph(&config);
        assert!(out.contains("estimated on an isotropic grid at the smallest acquired voxel dimension"), "{out}");
        config.bg_removal.vsharp.grid = VsharpGrid::Isotropic;
        config.bg_removal.vsharp.grid_spacing = 0.8;
        let out = paragraph(&config);
        assert!(out.contains("estimated on a 0.8 mm isotropic grid"), "{out}");
        assert!(!out.contains("For anisotropic voxels"), "{out}");
        config.bg_removal.vsharp.grid = VsharpGrid::Native;
        let out = paragraph(&config);
        assert!(!out.contains("isotropic grid"), "{out}");
    }

    #[test]
    fn ilsqr_methods_state_only_a_non_default_padding() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Ilsqr;
        let out = generate_methods(&config);
        assert!(!out.contains("zero-padding"), "the default padding is not stated: {out}");
        config.inversion.ilsqr.pad_mm = 32.0;
        let out = generate_methods(&config);
        assert!(out.contains("zero-padding = 32 mm per side"), "{out}");
    }

    /// A method's echo time is said only when it is not that method's default; the new settings
    /// read by name.
    #[test]
    fn methods_state_an_echo_time_only_when_it_is_not_the_default() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_methods(&config).contains("converted to phase"));
        config.inversion.medi.te = EchoTimeChoice::First;
        let out = generate_methods(&config);
        assert!(out.contains("the field converted to phase at the first echo time"), "{out}");
        assert!(!out.contains("te ="), "{out}");
        config.inversion.algorithm = QsmAlgorithm::Ndi;
        config.inversion.ndi.te = EchoTimeChoice::Seconds(0.01);
        assert!(generate_methods(&config).contains("converted to phase at an echo time of 10 ms"));
        config.inversion.algorithm = QsmAlgorithm::Rts;
        config.inversion.rts.gamma = 2.0;
        assert!(generate_methods(&config).contains("γ (ρ increase factor) = 2"));
        config.inversion.algorithm = QsmAlgorithm::Whqsm;
        config.inversion.whqsm.weight = WhqsmWeight::InVivo;
        assert!(generate_methods(&config).contains("data weight = in-vivo"));
        config.bg_removal.algorithm = BfAlgorithm::Pdf;
        config.bg_removal.pdf.n_cg = 30;
        assert!(generate_methods(&config).contains("at most 30 conjugate-gradient iterations"));
    }

    #[test]
    fn star_methods_state_the_padding_and_echo_time() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Star;
        let out = generate_methods(&config);
        assert!(out.contains("STAR-QSM (Streaking Artifact Reduction) (Wei et al., 2015) with zero-padding of 8 × 8 × 20 voxels per side and the field converted to phase at the sum of the echo times."), "{out}");
        assert!(out.contains("https://doi.org/10.1002/nbm.3383"), "{out}");
        config.inversion.star.te = EchoTimeChoice::Seconds(0.02);
        let out = generate_methods(&config);
        assert!(out.contains("converted to phase at an echo time of 20 ms"), "{out}");
    }

    #[test]
    fn test_default_methods() {
        let config = PipelineConfig::default();
        let out = generate_methods(&config);
        assert!(out.contains("QSM processing was performed using qsmxt"));
        assert!(out.contains("Phase offset removal"));
        assert!(out.contains("ROMEO"));
        assert!(out.contains("V-SHARP"));
        assert!(out.contains("iLSQR"));
        assert!(out.contains("referenced to the mean susceptibility within the reconstruction mask"), "{out}");
        assert!(out.contains("# Methods"));
        assert!(out.contains("## References"));
    }

    #[test]
    fn test_tgv_methods() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Tgv;
        let out = generate_methods(&config);
        assert!(out.contains("Total Generalized Variation (TGV)"));
        assert!(!out.contains("α0"), "default weights are not stated: {out}");
        assert!(!out.contains("TGV was run with"), "{out}");
        config.inversion.tgv.alpha0 = 0.123;
        config.inversion.tgv.erosions = 7;
        let out = generate_methods(&config);
        assert!(out.contains("TGV was run with 7 mask erosions and α0 = 0.123."), "{out}");
        // Multi-echo TGV is fed the field map, so field mapping is described (#282); background
        // removal never runs in front of it.
        assert!(out.contains("for multi-echo data TGV was applied to the B0 field map"), "{out}");
        assert!(out.contains("Phase unwrapping was performed"), "{out}");
        assert!(!out.contains("Background field removal was performed"), "{out}");
    }

    #[test]
    fn test_dl_tiling_methods() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Xqsm;
        // No tiling sentence when tiling is off.
        assert!(!generate_methods(&config).contains("overlapping tiles"));
        // Sentence appears (with the sizes) when tiling is enabled.
        config.inversion.tile_size = Some(64);
        config.inversion.tile_halo = Some(4);
        let out = generate_methods(&config);
        assert!(out.contains("overlapping tiles"), "got: {}", out);
        assert!(out.contains("tile size 64") && out.contains("halo 4"), "got: {}", out);
        assert!(!out.contains("approximates"), "only the phase networks say so: {out}");
        // The phase networks say that tiling approximates their whole-volume result.
        config.inversion.algorithm = QsmAlgorithm::Iqsm;
        let out = generate_methods(&config);
        assert!(out.contains("iQSM was run on overlapping tiles (tile size 64 voxels, halo 4 voxels), \
            which approximates its whole-volume result."), "got: {out}");
        config.inversion.algorithm = QsmAlgorithm::Rts;
        config.bg_removal.algorithm = BfAlgorithm::Iqfm;
        assert!(generate_methods(&config).contains("iQFM was run on overlapping tiles"), "iQFM");
    }

    #[test]
    fn test_qsmart_methods() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        // Default inner inversion is iLSQR — methods must say so (not the old hardcoded "TKD").
        let out = generate_methods(&config);
        assert!(out.contains("QSMART"));
        assert!(out.contains("iLSQR"), "QSMART methods should name the default inner inversion (iLSQR): {}", out);
        assert!(!out.contains("TKD inversion"), "QSMART methods must not hardcode TKD: {}", out);

        // Swapping the inner inversion must be reflected.
        config.inversion.qsmart.inversion = QsmAlgorithm::Tikhonov;
        let out = generate_methods(&config);
        assert!(out.contains("Tikhonov"), "QSMART methods should reflect the selected inner inversion: {}", out);
    }

    #[test]
    fn test_qsmart_methods_describe_field_mapping() {
        // QSMART runs the field-mapping stage upstream, so its methods must describe
        // unwrapping, B0 estimation, and phase offset removal (not just QSMART).
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        let out = generate_methods(&config);
        assert!(out.contains("ROMEO"), "QSMART methods should describe unwrapping: {}", out);
        assert!(out.contains("B0 field map"), "QSMART methods should describe B0 estimation: {}", out);
        assert!(out.contains("Phase offset removal"), "QSMART methods should describe phase offset removal: {}", out);
    }

    /// QSMART's own inputs, set as the authors' implementation prepares them, are said plainly and cited: each echo's
    /// raw phase unwrapped over the whole volume on the mirrored volume, the echo fit and the
    /// voxels it leaves out, and the N4-corrected mean magnitude vessels are detected on.
    #[test]
    fn qsmarts_own_inputs_are_described() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        config.field_mapping.laplacian_solver = LaplacianSolver::Mirror;
        config.field_mapping.laplacian_unmasked = true;
        config.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
        config.masking.sections = crate::masking::mask_presets().into_iter().find(|(n, _)| *n == "bet").unwrap().1.sections;
        let out = generate_methods(&config);
        for said in ["separately for each echo, on the raw phase over the whole volume, using the Laplacian method",
                     "on the volume mirrored along each axis",
                     "fit of phase vs echo time through the origin, as in QSMART (Yaghmaie et al., 2021)",
                     "voxels whose blurred fit residual exceeded its 99th percentile were excluded from the QSMART reconstruction",
                     "mean magnitude over echoes, bias-corrected with N4 (Tustison et al., 2010)",
                     "https://doi.org/10.1109/TMI.2010.2046908"] {
            assert!(out.contains(said), "{said:?} not in: {out}");
        }
        for unsaid in ["Phase offset removal", "ROMEO", "sphere radius", "STI", "Biobank"] {
            assert!(!out.contains(unsaid), "{unsaid:?} in: {out}");
        }
        // The overrides, said as what was done.
        config.inversion.qsmart.vessel_magnitude = QsmartVesselMagnitude::Combined;
        config.inversion.qsmart.vessel_size_units = VesselSizeUnits::Mm;
        config.field_mapping.qsmart_fit_threshold_mode = QsmartFitThresholdMode::Absolute;
        config.field_mapping.qsmart_fit_threshold = 30.0;
        let out = generate_methods(&config);
        for said in ["detected on the inhomogeneity-corrected RSS-combined magnitude",
                     "a bottom-hat sphere radius of 8 mm", "Frangi scales from 0.5 to 6 mm in steps of 0.5 mm",
                     "blurred fit residual exceeded 30 were excluded"] {
            assert!(out.contains(said), "{said:?} not in: {out}");
        }
        assert!(!out.contains("Tustison"), "{out}");
        // Sizes in voxels are said only when changed, with their units.
        config.inversion.qsmart.vessel_size_units = VesselSizeUnits::Voxels;
        config.inversion.qsmart.vasc_sphere_radius = 6;
        let out = generate_methods(&config);
        assert!(out.contains("a bottom-hat sphere radius of 6 voxels") && !out.contains("Frangi scales from"), "{out}");
        // The echo fit under another inversion: no voxels left out of anything.
        let mut other = PipelineConfig::default();
        other.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
        let out = generate_methods(&other);
        assert!(out.contains("as in QSMART") && !out.contains("excluded"), "{out}");
    }

    /// QSMART's echo fit after ROMEO, phase offset removal and bipolar correction: each step is
    /// said as it ran, and nothing about Laplacian unwrapping or the whole volume.
    #[test]
    fn the_echo_fit_after_romeo_says_what_ran() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        config.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
        config.field_mapping.bipolar_correction = true;
        config.field_mapping.laplacian_unmasked = true; // a Laplacian setting: ROMEO ignores it
        let out = paragraph(&config);
        for said in ["Phase offset removal (Eckstein et al., 2018) and bipolar gradient correction (Eckstein, 2021) were applied.",
                     "Phase unwrapping was performed using ROMEO (Dymerska et al., 2021).",
                     "a magnitude-weighted fit of phase vs echo time through the origin, as in QSMART (Yaghmaie et al., 2021)",
                     "excluded from the QSMART reconstruction"] {
            assert!(out.contains(said), "{said:?} not in: {out}");
        }
        for unsaid in ["Laplacian", "whole volume", "raw phase"] {
            assert!(!out.contains(unsaid), "{unsaid:?} in: {out}");
        }
    }

    #[test]
    fn test_nltv_cites_milovic_not_rts() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Nltv;
        let out = generate_methods(&config);
        assert!(out.contains("Milovic"), "NLTV must cite Milovic et al. 2018: {}", out);
        assert!(!out.contains("Rapid two-step"), "NLTV must not cite the RTS paper: {}", out);
    }

    #[test]
    fn robust_fit_methods() {
        let mut config = PipelineConfig::default();
        config.field_mapping.b0_estimation = B0Estimation::RobustFit;
        let out = generate_methods(&config);
        assert!(out.contains("motion-robust"), "{out}");
        assert!(out.contains("by iteratively reweighted least squares."), "{out}");
        assert!(!out.contains("Tukey") && !out.contains("tuning"), "defaults are not stated: {out}");
        config.field_mapping.robust_fit.loss = RobustLoss::Huber;
        let out = generate_methods(&config);
        assert!(out.contains("least squares with a Huber loss."), "{out}");
        // Huber's conventional constant is its default, so it is not a change.
        config.field_mapping.robust_fit.tuning = Some(1.345);
        assert!(!generate_methods(&config).contains("tuning"));
        config.field_mapping.robust_fit.tuning = Some(2.0);
        assert!(generate_methods(&config).contains("a Huber loss and tuning constant 2."));
    }

    #[test]
    fn motion_correction_methods() {
        let mut config = PipelineConfig::default();
        assert!(!generate_methods(&config).contains("co-registered"));
        config.motion_correction.enabled = true;
        assert!(generate_methods(&config).contains("rigidly co-registered to the first echo"));
    }

    #[test]
    fn test_laplacian_methods() {
        let mut config = PipelineConfig::default();
        config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let out = paragraph(&config);
        assert!(out.contains("Laplacian method"));
        assert!(out.contains("Schofield"));
        assert!(!out.contains("DCT"), "the default solver is not stated: {out}");
        config.field_mapping.laplacian_solver = LaplacianSolver::Fft;
        let out = paragraph(&config);
        assert!(out.contains("(Schofield & Zhu, 2003) with an FFT Poisson solve."), "{out}");
        assert!(!out.contains("zero-padded"), "the default padding is not stated: {out}");
        config.field_mapping.laplacian_fft_pad = [64, 64, 32];
        assert!(paragraph(&config).contains("with an FFT Poisson solve on the volume zero-padded by 64×64×32 voxels."));
        // The phase set to zero outside the mask first is the default, so it is not said; the raw
        // phase is.
        assert!(!paragraph(&config).contains("raw phase") && !paragraph(&config).contains("whole volume"));
        config.field_mapping.laplacian_unmasked = true;
        let out = paragraph(&config);
        assert!(out.contains("Phase unwrapping was performed separately for each echo, on the raw phase over the whole \
                              volume, using the Laplacian method (Schofield & Zhu, 2003) with an FFT Poisson solve"), "{out}");
        // The echo fit on the masked phase: nothing about the whole volume.
        config.field_mapping.laplacian_unmasked = false;
        config.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
        let out = paragraph(&config);
        assert!(out.contains("as in QSMART") && !out.contains("whole volume"), "{out}");
    }

    #[test]
    fn test_romeo_individual_mode() {
        // Individual per-echo unwrapping is the default, so the mode is not stated.
        let config = PipelineConfig::default();
        let out = generate_methods(&config);
        assert!(out.contains("Phase unwrapping was performed using ROMEO (Dymerska et al., 2021)."), "{out}");
        assert!(!out.contains("mode"), "{out}");
    }

    #[test]
    fn test_romeo_template_mode() {
        let mut config = PipelineConfig::default();
        config.field_mapping.romeo.individual = false;
        let out = generate_methods(&config);
        assert!(out.contains("using ROMEO (Dymerska et al., 2021) in template-based temporal mode."), "{out}");
    }

    #[test]
    fn test_bipolar_methods() {
        let mut config = PipelineConfig::default();
        config.field_mapping.bipolar_correction = true;
        let out = generate_methods(&config);
        assert!(out.contains("bipolar gradient correction"));
        assert!(out.contains("Eckstein, 2021"));
    }

    #[test]
    fn test_offset_and_bipolar_combined() {
        let mut config = PipelineConfig::default();
        config.field_mapping.bipolar_correction = true;
        let out = generate_methods(&config);
        assert!(out.contains("Phase offset removal") && out.contains("bipolar gradient correction"));
    }

    #[test]
    fn test_no_offset_removal() {
        let mut config = PipelineConfig::default();
        config.field_mapping.phase_offset_removal = false;
        let out = generate_methods(&config);
        assert!(!out.contains("Phase offset removal"));
    }

    #[test]
    fn test_b0_linear_fit_methods() {
        let mut config = PipelineConfig::default();
        config.field_mapping.b0_estimation = B0Estimation::LinearFit;
        let out = generate_methods(&config);
        assert!(out.contains("linear fit"));
    }

    #[test]
    fn test_b0_weighted_avg_methods() {
        let config = PipelineConfig::default();
        let out = generate_methods(&config);
        assert!(out.contains("The B0 field map was estimated as a weighted average of the unwrapped echo phases."), "{out}");
        assert!(!out.contains("phase-snr"), "the default weighting is not stated: {out}");
        let mut config = config;
        config.field_mapping.b0_weight_type = B0WeightType::Mag;
        assert!(generate_methods(&config).contains("unwrapped echo phases with mag weighting."));
    }

    #[test]
    fn test_laplacian_methods_skip_offset_removal() {
        let mut config = PipelineConfig::default();
        config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let out = generate_methods(&config);
        assert!(!out.contains("Phase offset removal"), "{out}");
        assert!(out.contains("separately for each echo"), "{out}");
    }

    #[test]
    fn test_b0_assumed_decay_methods() {
        let mut config = PipelineConfig::default();
        config.field_mapping.b0_weight_type = B0WeightType::AssumedDecay;
        let out = generate_methods(&config);
        assert!(out.contains("TE·exp(−TE/T2*), computed from the echo times and an assumed T2* and identical"), "{out}");
        assert!(out.contains("Wang, C., et al. (2022)"), "{out}");
        config.field_mapping.b0_weight_assumed_t2star_ms = 30.0;
        assert!(generate_methods(&config).contains("an assumed T2* of 30 ms"));
    }

    #[test]
    fn test_all_bf_algorithms_in_methods() {
        for (alg, expected) in [
            (BfAlgorithm::Vsharp, "V-SHARP"),
            (BfAlgorithm::Pdf, "PDF"),
            (BfAlgorithm::Lbv, "LBV"),
            (BfAlgorithm::Ismv, "iSMV"),
            (BfAlgorithm::Sharp, "SHARP"),
            (BfAlgorithm::Resharp, "RESHARP"),
            (BfAlgorithm::Harperella, "HARPERELLA"),
            (BfAlgorithm::Iharperella, "iHARPERELLA"),
            (BfAlgorithm::Bfrnet, "BFRnet"),
            (BfAlgorithm::Iqfm, "iQFM"),
            (BfAlgorithm::Iqfm, "Gao et al., 2022"),
        ] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            let out = generate_methods(&c);
            assert!(out.contains(expected), "missing {} for {:?}", expected, alg);
        }
    }

    #[test]
    fn test_dl_separation_methods() {
        for (alg, needle, cite) in [
            (SeparationAlgorithm::SusepNet, "SUSEP-Net", "Li et al., 2025"),
            (SeparationAlgorithm::ChiSepNet, "χ-sepnet", "Kim et al."),
        ] {
            let mut config = PipelineConfig::default();
            config.pipeline.do_chi_separation = true;
            config.separation.algorithm = alg;
            let out = generate_methods(&config);
            assert!(out.contains("Paramagnetic and diamagnetic susceptibility maps were computed using"), "{:?}: {}", alg, out);
            assert!(out.contains(needle), "missing {} for {:?}: {}", needle, alg, out);
            assert!(out.contains(cite), "missing citation for {:?}: {}", alg, out);
            assert!(out.contains("Negative χ+ and |χ−| values were set to zero."), "{:?}: {}", alg, out);
        }
        // The other separations return the signed maps.
        let mut config = PipelineConfig::default();
        config.pipeline.do_chi_separation = true;
        config.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        assert!(!generate_methods(&config).contains("Negative χ+"));
    }

    /// MEDI-mSMV is said with MEDI, after the background removal it takes the local field from,
    /// and cited; plain MEDI and other inversions say nothing of it.
    /// MEDI+0's CSF term is said with its condition and cited, and its changed settings are
    /// listed; off, nothing is said.
    #[test]
    fn test_medi_csf_methods() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        let out = generate_methods(&config);
        assert!(out.contains("MEDI's zero reference to the ventricular CSF (MEDI+0; Liu et al., 2018) was applied to runs of at least three echoes."), "{out}");
        assert!(out.contains("https://doi.org/10.1002/mrm.26946"), "cited: {out}");
        (config.inversion.medi.lambda_csf, config.inversion.medi.csf_erode) = (50.0, false);
        config.inversion.medi.zeropad = [2, 0, 2];
        let out = generate_methods(&config);
        assert!(out.contains("(MEDI+0; Liu et al., 2018; λ_CSF = 50 and without eroding the mask first)"), "{out}");
        assert!(out.contains("zero-padding = 2 × 0 × 2 voxels at the end of each axis") && !out.contains("csf"), "{out}");
        config.inversion.medi.csf = false;
        assert!(!generate_methods(&config).contains("MEDI+0"));
        config.inversion.algorithm = QsmAlgorithm::Tfi;
        config.inversion.tfi.linear = false;
        assert!(generate_methods(&config).contains("with the linear data term disabled"));
    }

    /// MEDI's complex fit is one sentence: the fit, cited, then the unwrapping of the fitted
    /// phase; no phase offset removal is said, as none is run.
    #[test]
    fn test_complex_fit_methods() {
        let mut config = PipelineConfig::default();
        config.field_mapping.b0_estimation = B0Estimation::ComplexFit;
        let out = generate_methods(&config);
        assert!(out.contains("The B0 field map was estimated by a magnitude-weighted nonlinear fit of the complex echo signal (Fit_ppm_complex, MEDI toolbox; Liu et al., 2013); the fitted phase per echo spacing was unwrapped using ROMEO (Dymerska et al., 2021) and divided by the echo spacing."), "{out}");
        assert!(!out.contains("Phase offset removal") && out.contains("https://doi.org/10.1002/mrm.24272"), "{out}");
        config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        config.field_mapping.complex_fit.max_iter = 12;
        let out = generate_methods(&config);
        assert!(out.contains("(Fit_ppm_complex, MEDI toolbox; Liu et al., 2013; at most 12 iterations); the fitted phase per echo spacing was unwrapped using the Laplacian method (Schofield & Zhu, 2003)"), "{out}");
    }

    #[test]
    fn test_medi_msmv_methods() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_methods(&config).contains("mSMV"));
        config.inversion.medi.msmv = true;
        let out = generate_methods(&config);
        assert!(out.contains("Background field removal was performed using"), "{out}");
        assert!(out.contains("MEDI (Morphology Enabled Dipole Inversion) (Liu et al., 2011), with maximum spherical mean value (mSMV) filtering of the local field (Roberts et al., 2024)"), "{out}");
        assert!(out.contains("Roberts, A.G."), "cited: {out}");
        config.inversion.medi.msmv_maxk = 9;
        config.inversion.medi.lambda = 500.0;
        let out = generate_methods(&config);
        assert!(out.contains("(Roberts et al., 2024; at most 8 boundary-correction passes), with λ = 500."), "{out}");
        config.inversion.algorithm = QsmAlgorithm::Rts;
        assert!(!generate_methods(&config).contains("mSMV"));
    }

    /// MEDI's route: the local field of the background removal given, then MEDI with its SMV
    /// high-pass, whose mask shrinking is said either way.
    #[test]
    fn medi_methods_say_pdf_and_the_smv_step() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        config.bg_removal.algorithm = BfAlgorithm::Pdf;
        let out = generate_methods(&config);
        assert!(out.contains("Background field removal was performed using PDF"), "{out}");
        assert!(out.contains("(Liu et al., 2011), with the local field high-passed by a 5 mm spherical mean value (SMV) filter within the mask shrunk by that radius."), "{out}");
        config.inversion.medi.smv_shrink_mask = false;
        config.inversion.medi.lambda = 500.0;
        let out = generate_methods(&config);
        assert!(out.contains("(SMV) filter over the full mask, with λ = 500."), "{out}");
        assert!(!out.contains("shrinking"), "{out}");
        // Without SMV the shrinking does not apply, and is not said.
        config.inversion.medi.smv = false;
        let out = generate_methods(&config);
        assert!(!out.contains("high-passed") && !out.contains("shrink"), "{out}");
        assert!(out.contains("SMV preprocessing disabled"), "{out}");
        // The general default is kept, and the Note says what MEDI's reference uses.
        config.bg_removal.algorithm = BfAlgorithm::Vsharp;
        let out = generate_methods(&config);
        assert!(out.contains("Background field removal was performed using V-SHARP"), "{out}");
        assert!(out.contains("MEDI reference pipeline, which uses PDF"), "{out}");
    }

    /// #282: for every inversion, the methods text describes background field removal exactly
    /// when the runner runs it, and phase unwrapping / echo combination exactly when it runs field
    /// mapping — both decided by QSM.rs's `InversionConfig::input`, as the runner decides them.
    #[test]
    fn methods_describe_exactly_the_stages_each_inversion_route_runs() {
        use crate::bridge::inversion_input;
        for alg in &QsmAlgorithm::ALL {
            for medi_smv in [false, true] {
                let mut config = PipelineConfig::default();
                config.inversion.algorithm = *alg;
                config.inversion.medi.smv = medi_smv;
                let out = generate_methods(&config);
                // The methods text does not know the echo count; multi-echo is the default case.
                let input = inversion_input(&config, 2);
                let bfr = out.contains("Background field removal was performed");
                assert_eq!(bfr, input.needs_bg_removal() && *alg != QsmAlgorithm::Qsmart,
                    "{alg} (medi.smv={medi_smv}): BFR described = {bfr}:\n{out}");
                if !input.needs_bg_removal() && *alg != QsmAlgorithm::Qsmart {
                    assert!(out.contains("background field removal and dipole inversion performed jointly")
                        || out.contains("background field removal and dipole inversion performed by the network")
                        || out.contains("Dipole inversion was performed on the total field"),
                        "{alg}: should say the inversion took the total field:\n{out}");
                }
                let unwrap = out.contains("Phase unwrapping was performed");
                assert_eq!(unwrap, input.needs_field_mapping(),
                    "{alg} (medi.smv={medi_smv}): unwrapping described = {unwrap}:\n{out}");
                // Single-echo TGV is the one wrapped-phase route of a multi-echo-capable method.
                if *alg == QsmAlgorithm::Tgv {
                    assert!(!inversion_input(&config, 1).needs_field_mapping());
                    assert!(out.contains("For single-echo data TGV was applied to the wrapped phase"), "{out}");
                }
            }
        }
    }

    #[test]
    fn test_swi_methods() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_swi = true;
        let out = generate_methods(&config);
        // Defaults: the method and its citation, no parameter values.
        assert!(out.contains("Susceptibility-weighted images were computed with CLEAR-SWI (Eckstein et al., 2021)."), "{out}");
        assert!(out.contains("CLEAR-SWI.\" *NeuroImage*"), "{out}");
        let sentence = out.split("Susceptibility-weighted images").nth(1).unwrap().split('.').next().unwrap();
        assert!(!sentence.contains("using"), "{sentence}");

        config.swi.unwrap = SwiUnwrap::Romeo;
        config.swi.strength = 6.0;
        config.swi.mip_window = 5;
        let out = generate_methods(&config);
        assert!(out.contains("CLEAR-SWI (Eckstein et al., 2021) using ROMEO phase unwrapping (Dymerska et al., 2021) \
                              and phase-mask strength 6. Minimum-intensity projections were taken over \
                              5-slice slabs."), "{out}");
        assert!(out.contains("Dymerska, B., et al. (2021)"), "{out}");
    }

    #[test]
    fn test_t2star_r2star_methods() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_t2starmap = true;
        config.pipeline.do_r2starmap = true;
        let out = generate_methods(&config);
        assert!(out.contains("T2* and R2*"));
        assert!(out.contains("ARLO"));
    }

    #[test]
    fn test_no_qsm() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_qsm = false;
        let out = generate_methods(&config);
        assert!(out.contains("MRI processing"));
        assert!(!out.contains("Dipole inversion"));
    }

    #[test]
    fn test_qsm_reference_none() {
        let mut config = PipelineConfig::default();
        config.qsm.reference = QsmReference::None;
        let out = generate_methods(&config);
        assert!(out.contains("No susceptibility referencing"));
    }

    /// A methods paragraph that does not say what the map was referenced to is not reproducible:
    /// the same pipeline referenced to CSF and to white matter gives different numbers.
    #[test]
    fn a_region_reference_is_named_in_the_methods() {
        let mut config = PipelineConfig::default();
        config.qsm.reference = QsmReference::Region;
        config.qsm.reference_region = Some("left-thalamus".to_string());
        let out = generate_methods(&config);
        assert!(out.contains("left thalamus"), "the region must be named: {out}");
        assert!(out.contains("left thalamus (SynthSeg label), computed within the reconstruction mask"),
            "and where it came from, and over which voxels: {out}");
        // The zero point is the region; nothing about zeroing may read as scoped to the region.
        assert!(!out.contains("set to zero"), "{out}");

        config.qsm.reference_region = Some("left-thalamus,right-thalamus".to_string());
        let out = generate_methods(&config);
        assert!(out.contains("left thalamus, right thalamus (SynthSeg labels)"), "{out}");

        config.segmentation.custom_dseg_tool = Some("freesurfer".to_string());
        let out = generate_methods(&config);
        assert!(out.contains("('freesurfer' segmentation labels)"), "{out}");
    }

    #[test]
    fn test_join_list() {
        assert_eq!(join_list(&[]), "");
        assert_eq!(join_list(&["a".into()]), "a");
        assert_eq!(join_list(&["a".into(), "b".into()]), "a and b");
        assert_eq!(join_list(&["a".into(), "b".into(), "c".into()]), "a, b, and c");
    }

    // ─── Masking description tests ───

    #[test]
    fn test_masking_otsu_threshold() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("Otsu thresholding"));
        assert!(out.contains("Otsu, 1979"));
    }

    #[test]
    fn test_masking_bet() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::bet(0.35),
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("BET brain extraction"));
        assert!(out.contains("Smith, 2002"));
        assert!(out.contains("f=0.35"));
    }

    #[test]
    fn test_masking_rs2_net() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::rs2_net_default(),
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("RS2-Net deep-learning rodent brain extraction (Lin et al., 2024) of"), "{out}");
        assert!(!out.contains("stepped by"), "the default step is not stated: {out}");
        assert!(out.contains("https://doi.org/10.1016/j.neuroimage.2024.120769"), "{out}");
        // Otsu is not cited by a run that never thresholded.
        assert!(!out.contains("Otsu, 1979"), "{out}");

        config.masking.sections[0].generator = MaskOp::Rs2Net { tta: true, tile_step: 0.5 };
        let tta = generate_methods(&config);
        assert!(tta.contains("mirroring test-time augmentation"), "{tta}");
    }

    #[test]
    fn test_masking_mindgrab() {
        let mut config = PipelineConfig::default();
        config.masking.sections = crate::masking::mindgrab_mask_sections();
        let out = generate_methods(&config);
        assert!(out.contains("MindGrab deep-learning brain extraction (Fani et al., 2025) of"), "{out}");
        assert!(!out.contains("test-time augmentation"), "TTA is off by default: {out}");
        assert!(out.contains("https://doi.org/10.48550/arXiv.2506.11860"), "{out}");
        assert!(!out.contains("Otsu, 1979"), "{out}");

        config.masking.sections[0].generator = MaskOp::MindGrab { tta: true };
        let tta = generate_methods(&config);
        assert!(tta.contains("MindGrab deep-learning brain extraction (Fani et al., 2025; left-right flip test-time augmentation) of"), "{tta}");
    }

    #[test]
    fn test_masking_hd_bet_signal_erode() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::hd_bet_default(),
            refinements: vec![MaskOp::signal_erode_default()],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("HD-BET deep-learning brain extraction (Isensee et al., 2019) of"), "{out}");
        assert!(out.contains("signal-gated erosion (removal of low-signal boundary voxels; QSM-CI)"), "{out}");
        assert!(!out.contains("0.80") && !out.contains("voxels deep"), "{out}");
        assert!(out.contains("https://doi.org/10.1002/hbm.24750"), "{out}");
        assert!(out.contains("https://github.com/QSMxT/QSM-CI"), "{out}");

        config.masking.sections[0].generator = MaskOp::HdBet { patch: Some(MaskOp::hd_bet_default_patch()), tta: true, tile_step: 0.25 };
        config.masking.sections[0].refinements = vec![parse_mask_op("signal-erode:0.7").unwrap()];
        let out = generate_methods(&config);
        assert!(out.contains("(Isensee et al., 2019; 192x192x96-voxel sliding-window patches, windows stepped by 25% of the patch, and mirroring test-time augmentation)"), "{out}");
        assert!(out.contains("signal-gated erosion (removal of low-signal boundary voxels, threshold 0.70 of the median bias-corrected magnitude; QSM-CI)"), "{out}");
    }

    #[test]
    fn test_masking_hd_bet_window() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::HdBet { patch: Some([128, 128, 64]), tta: true, tile_step: 0.5 },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("(Isensee et al., 2019; 128x128x64-voxel sliding-window patches and mirroring \
                              test-time augmentation)"), "{out}");
    }

    #[test]
    fn test_masking_fixed_threshold() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.3) },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("fixed thresholding"));
        assert!(out.contains("value=0.3"));
    }

    #[test]
    fn test_masking_percentile_threshold() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(80.0) },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("percentile thresholding"));
        assert!(out.contains("80th percentile"));
    }

    #[test]
    fn test_masking_phase_quality_input() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::PhaseQuality,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("ROMEO phase quality map"));
    }

    #[test]
    fn test_masking_magnitude_first_input() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::MagnitudeFirst,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("first-echo magnitude image"));
    }

    #[test]
    fn test_masking_magnitude_last_input() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::MagnitudeLast,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("last-echo magnitude image"));
    }

    #[test]
    fn test_masking_with_inhomogeneity() {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_correction = true;
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("Inhomogeneity correction"));
        assert!(out.contains("Eckstein et al., 2019"));
        assert!(out.contains("inhomogeneity-corrected RSS-combined magnitude"));
    }

    #[test]
    fn n4_is_named_and_cited_and_makehomogeneous_is_not() {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        let out = generate_methods(&config);
        assert!(out.contains("N4 inhomogeneity correction (Tustison et al., 2010) was applied"), "out: {out}");
        assert!(out.contains("N4ITK: Improved N3 Bias Correction"), "out: {out}");
        assert!(!out.contains("Eckstein et al., 2019") && !out.contains("A Simple Homogeneity Correction"), "out: {out}");
    }

    #[test]
    fn test_masking_inhomogeneity_first_echo() {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_correction = true;
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::MagnitudeFirst,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("first-echo magnitude image"));
        assert!(out.contains("inhomogeneity-corrected"));
    }

    #[test]
    fn test_masking_no_inhomogeneity() {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_correction = false;
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(!out.contains("Inhomogeneity correction"));
        assert!(out.contains("the RSS-combined magnitude image"));
        assert!(!out.contains("inhomogeneity-corrected"));
    }

    #[test]
    fn test_masking_refinements() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![
                MaskOp::Dilate { iterations: 1 },
                MaskOp::FillHoles { max_size: 0 },
                MaskOp::Erode { iterations: 1 },
            ],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("followed by"));
        assert!(out.contains("dilation"));
        assert!(out.contains("hole-filling"));
        assert!(out.contains("erosion"));
    }

    #[test]
    fn test_masking_erode_singular() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![MaskOp::Erode { iterations: 1 }],
        }];
        let out = generate_methods(&config);
        // One iteration is what a bare `erode` means, so it is not stated.
        assert!(out.contains("followed by erosion."), "{out}");
        assert!(!out.contains("iteration"), "{out}");
    }

    #[test]
    fn test_masking_erode_plural() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![MaskOp::Erode { iterations: 3 }],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("erosion (3 iterations)"));
    }

    #[test]
    fn test_masking_close_refinement() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![MaskOp::Close { radius: 5 }],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("morphological closing (radius 5 voxels)"), "{out}");
    }

    #[test]
    fn test_masking_gaussian_refinement() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![MaskOp::GaussianSmooth { sigma_mm: 2.5 }],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("Gaussian smoothing (σ = 2.5 mm)"), "{out}");
    }

    #[test]
    fn test_masking_fill_holes_nonzero() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![MaskOp::FillHoles { max_size: 500 }],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("hole-filling (holes of at most 500 voxels)"), "{out}");
    }

    #[test]
    fn test_masking_multiple_sections_or() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![
            MaskSection {
                input: MaskingInput::Magnitude,
                generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                refinements: vec![],
            },
            MaskSection {
                input: MaskingInput::PhaseQuality,
                generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                refinements: vec![],
            },
        ];
        let out = generate_methods(&config);
        assert!(out.contains("combining"));
        assert!(out.contains("2 mask sections"));
        assert!(out.contains("OR operation"));
    }

    #[test]
    fn test_masking_empty_sections() {
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![];
        let out = generate_methods(&config);
        assert!(!out.contains("A brain mask was generated"));
    }

    #[test]
    fn test_masking_inhomogeneity_mixed_inputs() {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_correction = true;
        config.masking.sections = vec![
            MaskSection {
                input: MaskingInput::Magnitude,
                generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                refinements: vec![],
            },
            MaskSection {
                input: MaskingInput::MagnitudeFirst,
                generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                refinements: vec![],
            },
        ];
        let out = generate_methods(&config);
        assert!(out.contains("applied to the magnitude data"));
    }

    // ─── Citation inline tests ───

    #[test]
    fn test_cite_inline_known_keys() {
        assert_eq!(cite_inline(&CITE_VSHARP), "Wu et al., 2012");
        assert_eq!(cite_inline(&CITE_SHARP), "Schweser et al., 2011");
        assert_eq!(cite_inline(&CITE_PDF), "Liu et al., 2011");
        assert_eq!(cite_inline(&CITE_ISMV), "Wen et al., 2014");
        assert_eq!(cite_inline(&CITE_LBV), "Zhou et al., 2014");
        assert_eq!(cite_inline(&CITE_RTS), "Kames et al., 2018");
        assert_eq!(cite_inline(&CITE_RESHARP), "Sun & Wilman, 2014");
        assert_eq!(cite_inline(&CITE_HARPERELLA), "Li et al., 2014");
        assert_eq!(cite_inline(&CITE_IHARPERELLA), "Li et al., 2015");
        assert_eq!(cite_inline(&CITE_BIPOLAR), "Eckstein, 2021");
    }

    #[test]
    fn test_cite_inline_unknown_key() {
        let unknown = Citation { key: "unknown2099", text: "Some text" };
        assert_eq!(cite_inline(&unknown), "unknown2099");
    }

    // ─── All inversion algorithms in methods ───

    #[test]
    fn test_all_inversion_algorithms_in_methods() {
        for (alg, expected) in [
            (QsmAlgorithm::Rts, "RTS"),
            (QsmAlgorithm::Tv, "Total Variation"),
            (QsmAlgorithm::Tkd, "TKD"),
            (QsmAlgorithm::Tsvd, "TSVD"),
            (QsmAlgorithm::Tikhonov, "Tikhonov"),
            (QsmAlgorithm::Nltv, "NLTV"),
            (QsmAlgorithm::Medi, "MEDI"),
            (QsmAlgorithm::Tfi, "TFI"),
            (QsmAlgorithm::Ilsqr, "iLSQR"),
            (QsmAlgorithm::Star, "STAR-QSM"),
            (QsmAlgorithm::Ndi, "NDI"),
            (QsmAlgorithm::Fansi, "FANSI (Nonlinear TV)"),
            (QsmAlgorithm::FansiTgv, "FANSI (Nonlinear TGV)"),
            (QsmAlgorithm::L1qsm, "L1-QSM"),
            (QsmAlgorithm::Whqsm, "WH-QSM"),
            (QsmAlgorithm::Hdqsm, "HD-QSM"),
            (QsmAlgorithm::AmpPe, "AMP-PE"),
            (QsmAlgorithm::Xqsm, "xQSM"),
            (QsmAlgorithm::Qsmnet, "QSMnet"),
            (QsmAlgorithm::QsmnetPlus, "QSMnet+"),
            (QsmAlgorithm::Autoqsm, "AutoQSM"),
            (QsmAlgorithm::Qsmgan, "QSMGAN"),
            (QsmAlgorithm::Ir2qsm, "IR2QSM"),
            (QsmAlgorithm::Lpcnn, "LPCNN"),
            (QsmAlgorithm::ModlQsm, "MoDL-QSM"),
            (QsmAlgorithm::Nextqsm, "NeXtQSM"),
            (QsmAlgorithm::Iqsm, "iQSM"),
            (QsmAlgorithm::IqsmPlus, "iQSM+"),
        ] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            let out = generate_methods(&c);
            // iQSM/iQSM+ reconstruct end to end from the wrapped phase: no separate inversion.
            let sentence = if matches!(alg, QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus) {
                "QSM was reconstructed with"
            } else {
                "Dipole inversion was performed"
            };
            assert!(out.contains(sentence), "missing dipole sentence for {:?}", alg);
            assert!(out.contains(expected), "missing '{}' for {:?}", expected, alg);
        }
    }

    // ─── Full pipeline test ───

    #[test]
    fn test_all_features_enabled() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_swi = true;
        config.pipeline.do_t2starmap = true;
        config.pipeline.do_r2starmap = true;
        let out = generate_methods(&config);
        assert!(out.contains("QSM processing"));
        assert!(out.contains("Susceptibility-weighted images"));
        assert!(out.contains("T2* and R2* maps"));
        assert!(out.contains("## References"));
    }

    #[test]
    fn test_t2star_only() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_t2starmap = true;
        let out = generate_methods(&config);
        assert!(out.contains("T2* maps were computed"));
        assert!(!out.contains("R2* maps"));
    }

    #[test]
    fn test_r2star_only() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_r2starmap = true;
        let out = generate_methods(&config);
        assert!(out.contains("R2* maps were computed"));
        assert!(!out.contains("T2* maps"));
    }
}

#[cfg(test)]
mod multiorient_methods_tests {
    use super::*;

    #[test]
    fn nothing_is_said_when_grouping_is_off() {
        let text = generate_methods(&PipelineConfig::default());
        assert!(!text.contains("COSMOS"), "{text}");
        assert!(!text.contains("orientation"), "{text}");
    }

    #[test]
    fn cosmos_is_described_and_cited() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "*acq-dir*".into();
        let text = generate_methods(&config);
        assert!(text.contains("Multiple Orientation Sampling (COSMOS; Liu et al., 2009)"), "{text}");
        assert!(text.contains("10.1002/mrm.21828"), "the reference list should carry it: {text}");
        // Default lambda is zero, so it should not be claimed.
        assert!(!text.contains("lambda"), "{text}");
    }

    #[test]
    fn sti_is_described_and_cited() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        config.multi_orientation.algorithm = MultiOrientAlgorithm::Sti;
        config.multi_orientation.lambda = 1e-3;
        let text = generate_methods(&config);
        assert!(text.contains("susceptibility tensor imaging (STI; Liu, 2010)"), "{text}");
        assert!(text.contains("10.1002/mrm.22482"), "{text}");
        assert!(text.contains("lambda = 0.001"), "{text}");
    }

    /// How the orientations reached a common grid is part of the method — it is where the B0
    /// directions came from — so the paragraph has to say which of the two it was.
    #[test]
    fn the_methods_say_how_the_orientations_were_registered() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        let text = generate_methods(&config);
        assert!(text.contains("rigidly co-registered"), "{text}");
        assert!(text.contains("derived from the recovered rotation"), "{text}");

        config.multi_orientation.register = false;
        let text = generate_methods(&config);
        assert!(text.contains("taken to be already co-registered"), "{text}");
        assert!(!text.contains("rigidly co-registered"), "{text}");
    }

    /// The methods paragraph says what was done: no tool provenance (STI Suite, UK Biobank),
    /// no QSMxT version history, no "rather than". Literature citations in the reference list
    /// are fine, so only the paragraph itself is checked.
    #[test]
    fn methods_paragraph_has_no_provenance_or_contrasts() {
        use crate::enums::{B0WeightType, LaplacianSolver, UnwrappingAlgorithm};
        let mut configs = Vec::new();
        for alg in &QsmAlgorithm::ALL {
            for laplacian in [false, true] {
                let mut c = PipelineConfig::default();
                c.inversion.algorithm = *alg;
                c.pipeline.do_segmentation = true;
                c.pipeline.do_chi_separation = true;
                c.pipeline.do_r2primemap = true;
                c.pipeline.do_r2map = true;
                c.field_mapping.b0_weight_type = B0WeightType::AssumedDecay;
                if laplacian {
                    c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
                    c.field_mapping.laplacian_solver = LaplacianSolver::Fft;
                }
                c.qsm.reference = QsmReference::Region;
                c.qsm.reference_region = Some("ventricles".to_string());
                configs.push(c);
            }
        }
        for c in &configs {
            let out = generate_methods(c);
            let paragraph = out.split("\n## ").next().unwrap().to_lowercase();
            // "set to zero outside": no zeroing outside a mask is described (the non-negative
            // network and R2' outputs are, as negative values set to zero).
            for banned in ["sti suite", "biobank", "ukb", "qsmxt v", "rather than", "without the", "set to zero outside"] {
                assert!(!paragraph.contains(banned), "`{banned}` in methods:\n{paragraph}");
            }
        }
    }
}


/// Parameter values are stated only when they differ from QSMxT's defaults.
#[cfg(test)]
mod defaults_only_tests {
    use super::*;

    /// Fragments that only a stated parameter value produces.
    const VALUE_MARKERS: [&str; 16] = [
        "=", " mm", "iteration", "%", "slices", "slab", "weighting)", "scaling", "power", "σ",
        "radii", "threshold ", "tolerance", "oblique", " mode", "tuning",
    ];

    fn paragraph(config: &PipelineConfig) -> String {
        let out = generate_methods(config);
        out.split("\n## ").next().unwrap().to_string()
    }

    fn assert_no_values(config: &PipelineConfig, what: &str) {
        assert_no_values_except(config, what, &[]);
    }

    fn assert_no_values_except(config: &PipelineConfig, what: &str, allowed: &[&str]) {
        let text = paragraph(config);
        for m in VALUE_MARKERS.iter().filter(|m| !allowed.contains(m)) {
            assert!(!text.contains(m), "{what}: `{m}` in default methods:\n{text}");
        }
    }

    /// Every section, switched on with its default settings, names its method and no values.
    #[test]
    fn default_settings_state_no_parameter_values() {
        let base = || {
            let mut c = PipelineConfig::default();
            c.pipeline.do_swi = true;
            c.pipeline.do_smwi = true;
            c.pipeline.do_t2starmap = true;
            c.pipeline.do_r2starmap = true;
            c.pipeline.do_segmentation = true;
            c.pipeline.do_analysis = true;
            c.pipeline.axial_resampling = true;
            c.pipeline.crop_to_mask = true;
            c.pipeline.fft_padding = true;
            c.masking.two_pass = true;
            c.motion_correction.enabled = true;
            c.inversion.medi.msmv = true;
            c
        };
        assert_no_values(&base(), "everything on");
        for alg in &QsmAlgorithm::ALL {
            let mut c = base();
            c.inversion.algorithm = *alg;
            c.inversion.tile_size = Some(64);
            assert_no_values_except(&c, &format!("inversion {alg}"), &[]);
        }
        for alg in [BfAlgorithm::Pdf, BfAlgorithm::Lbv, BfAlgorithm::Ismv, BfAlgorithm::Sharp,
                    BfAlgorithm::Resharp, BfAlgorithm::Harperella, BfAlgorithm::Iharperella] {
            let mut c = base();
            c.bg_removal.algorithm = alg;
            assert_no_values(&c, &format!("background removal {alg:?}"));
        }
        for alg in [SeparationAlgorithm::R2starQsm, SeparationAlgorithm::Decompose,
                    SeparationAlgorithm::ChiSepIlsqr, SeparationAlgorithm::ChiSepMedi,
                    SeparationAlgorithm::WaveSep, SeparationAlgorithm::HcChisep] {
            let mut c = base();
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = alg;
            crate::config::enforce_separation_dependencies(&mut c);
            assert_no_values(&c, &format!("separation {alg:?}"));
        }
        for b0 in [B0Estimation::LinearFit, B0Estimation::RobustFit] {
            let mut c = base();
            c.field_mapping.b0_estimation = b0;
            assert_no_values(&c, &format!("B0 {b0:?}"));
        }
        let mut c = base();
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        c.field_mapping.b0_weight_type = B0WeightType::AssumedDecay;
        assert_no_values(&c, "laplacian + assumed decay");
        for (name, recipe) in mask_presets() {
            let mut c = base();
            c.masking.sections = recipe.sections;
            c.masking.combine = recipe.combine;
            c.masking.refinements = recipe.refinements;
            // A recipe's own refinement counts (the `bet` preset erodes twice) are part of the
            // recipe, not a setting with a default.
            assert_no_values_except(&c, &format!("mask preset {name}"), &["iteration"]);
        }
        let mut c = base();
        c.pipeline.do_r2primemap = true;
        c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet;
        assert_no_values(&c, "R2PRIMEnet");
        let mut c = base();
        c.multi_orientation.group_by = "acq".into();
        c.multi_orientation.magnitude_weighting = true;
        assert_no_values(&c, "COSMOS");
    }

    /// Changing one setting states exactly that setting.
    #[test]
    fn a_changed_setting_is_stated() {
        type Edit = fn(&mut PipelineConfig);
        let cases: Vec<(Edit, &str)> = vec![
            (|c| c.bg_removal.vsharp.min_radius = 3.0, "V-SHARP (Wu et al., 2012) with SMV kernel radii from 12 to 3 mm in 1 mm steps."),
            (|c| c.bg_removal.vsharp.radius_step = 2.0, "V-SHARP (Wu et al., 2012) with SMV kernel radii from 12 to 2 mm in 2 mm steps."),
            (|c| c.bg_removal.vsharp.deconvolve = true, "V-SHARP (Wu et al., 2012) with truncated-SVD deconvolution enabled."),
            (|c| { c.bg_removal.algorithm = BfAlgorithm::Ismv; c.bg_removal.ismv.radius = 7.5; },
                "iSMV (Wen et al., 2014) with SMV radius = 7.5 mm."),
            (|c| { c.bg_removal.algorithm = BfAlgorithm::Pdf; c.bg_removal.pdf.tol = 0.123; },
                "PDF (Liu et al., 2011) with tolerance = 0.123."),
            (|c| { c.inversion.algorithm = QsmAlgorithm::Medi; c.inversion.medi.msmv = true; c.inversion.medi.msmv_maxk = 9; },
                "(Roberts et al., 2024; at most 8 boundary-correction passes)."),
            (|c| c.inversion.ilsqr.max_iter = 123, "iLSQR (Li et al., 2015) with at most 123 iterations."),
            (|c| { c.inversion.algorithm = QsmAlgorithm::Rts; c.inversion.rts.delta = 0.37; },
                "(Kames et al., 2018) with δ = 0.37."),
            (|c| { c.inversion.algorithm = QsmAlgorithm::Medi; c.inversion.medi.merit = !MediConfig::default().merit; },
                "MERIT"),
            (|c| { c.inversion.algorithm = QsmAlgorithm::Xqsm; c.inversion.tile_size = Some(64); c.inversion.tile_halo = Some(4); },
                "(tile size 64 voxels, halo 4 voxels)"),
            (|c| { c.inversion.algorithm = QsmAlgorithm::Qsmart; c.inversion.qsmart.frangi_c = 123.0; },
                "tissue/vasculature separation, with frangi c = 123."),
            (|c| c.field_mapping.romeo.correct_global = false, "ROMEO (Dymerska et al., 2021) with correct global disabled."),
            (|c| c.field_mapping.phase_offset_sigma_mm = [3.0, 3.0, 3.0], "(Eckstein et al., 2018; smoothing σ = 3 × 3 × 3 mm)"),
            (|c| { c.field_mapping.b0_estimation = B0Estimation::LinearFit; c.field_mapping.linear_fit.estimate_offset = !LinearFitConfig::default().estimate_offset; },
                "echo time with phase-offset estimation"),
            (|c| { c.pipeline.axial_resampling = true; c.pipeline.obliquity_threshold = 5.0; },
                "Acquisitions more than 5° oblique were resampled"),
            (|c| { c.pipeline.axial_resampling = true; c.pipeline.output_space = crate::config::OutputSpace::Working; },
                "and the derivatives were written on the resampled grid."),
            (|c| { c.pipeline.crop_to_mask = true; c.pipeline.crop_margin_mm = 16.0; },
                "within a box around the brain mask with a 16 mm margin."),
            (|c| { c.motion_correction.enabled = true; c.registration.levels = 2; },
                "over a multi-resolution pyramid (2 pyramid levels)"),
            (|c| { c.homogeneity.sigma_mm = 5.0; }, "(Eckstein et al., 2019; σ = 5 mm)"),
            (|c| { c.masking.inhomogeneity_method = InhomogeneityMethod::N4; c.n4.max_iterations = vec![20, 10]; c.n4.shrink_factor = 2; },
                "N4 inhomogeneity correction (Tustison et al., 2010; at most 20 and 10 iterations at 2 fitting levels and shrink factor = 2) was applied"),
            (|c| c.masking.sections[0].refinements[0] = MaskOp::Dilate { iterations: 2 }, "followed by dilation (2 iterations), hole-filling, and erosion."),
            (|c| { c.pipeline.do_swi = true; c.swi.hp_sigma = [2.0, 2.0, 0.0]; },
                "CLEAR-SWI (Eckstein et al., 2021) using a high-pass filter σ of 2 × 2 × 0 voxels."),
            (|c| { c.pipeline.do_swi = true; c.swi.strength = 2.0; },
                "CLEAR-SWI (Eckstein et al., 2021) using phase-mask strength 2."),
            (|c| { c.pipeline.do_swi = true; c.swi.scaling = SwiScaling::Positive; },
                "CLEAR-SWI (Eckstein et al., 2021) using positive phase-mask scaling."),
            (|c| { c.pipeline.do_swi = true; c.swi.mip_window = 5; }, "taken over 5-slice slabs."),
            (|c| { c.pipeline.do_smwi = true; c.smwi.power = 2.5; },
                "derived from the susceptibility map (mask raised to the power 2.5), separately"),
            (|c| { c.pipeline.do_smwi = true; c.smwi.mip_window = 5; }, "minimum-intensity projection over 5 slices."),
            (|c| { c.pipeline.do_segmentation = true; c.segmentation.flip_averaging = !SegmentationConfig::default().flip_averaging; },
                "structures, with left-right flip averaging of the posteriors"),
            (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepNet; c.separation.chi_sepnet.patch = [128, 128, 64]; },
                ", with 128×128×64-voxel sliding-window patches."),
            (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::SusepNet; c.separation.susep_net.patch = Some([128, 128, 64]); },
                ", with the network applied as a sliding window of 128×128×64-voxel patches."),
            (|c| { c.pipeline.do_segmentation = true; c.segmentation.patch = Some(100); },
                "structures, with the network applied as a sliding window of 112×112×112-voxel patches overlapping by at least half a patch, their posteriors Gaussian-weighted."),
            (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepMedi; c.separation.chi_sep_medi.lambda_para = 1000.0; },
                "λ (paramagnetic) = 1000"),
            (|c| { c.pipeline.do_chi_separation = true; c.separation.r2star_qsm.r_const_3t = 123.0; },
                "with relaxometric constant at 3 T = 123."),
            (|c| { c.pipeline.do_r2primemap = true; c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet; c.separation.r2primenet.patch = [160, 160, 128]; },
                "(Kim et al., 2025; patch = 160 × 160 × 128 voxels)"),
            (|c| { c.multi_orientation.group_by = "acq".into(); c.multi_orientation.magnitude_weighting = true; c.multi_orientation.cosmos_max_iter = 77; },
                "solved by conjugate gradient (at most 77 iterations)."),
            (|c| { c.multi_orientation.group_by = "acq".into(); c.registration.levels = 2; },
                "multi-resolution pyramid; 2 pyramid levels)"),
        ];
        for (edit, expected) in cases {
            let mut c = PipelineConfig::default();
            edit(&mut c);
            crate::config::enforce_separation_dependencies(&mut c);
            let text = paragraph(&c);
            assert!(text.contains(expected), "expected `{expected}` in:\n{text}");
        }
    }

    #[test]
    fn floats_compare_with_a_tolerance() {
        assert!(!differs(0.1 + 0.2, 0.3));
        assert!(differs(0.3, 0.31));
        let mut c = PipelineConfig::default();
        c.bg_removal.vsharp.max_radius += 1e-12;
        assert!(!paragraph(&c).contains("radii"));
    }
}

#[cfg(test)]
mod structured_tests;
