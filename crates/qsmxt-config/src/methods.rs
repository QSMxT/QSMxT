use crate::config::*;
use crate::enums::*;
use crate::masking::*;

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

const CITE_MEDI: Citation = Citation {
    key: "liu2011medi",
    text: "Liu, T., et al. (2011). \"Morphology enabled dipole inversion (MEDI) from a single-angle acquisition.\" *Magnetic Resonance in Medicine*, 66(3):777-783. https://doi.org/10.1002/mrm.22816",
};

const CITE_ILSQR: Citation = Citation {
    key: "li2015",
    text: "Li, W., et al. (2015). \"A method for estimating and removing streaking artifacts in quantitative susceptibility mapping.\" *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043",
};

const CITE_TGV: Citation = Citation {
    key: "langkammer2015",
    text: "Langkammer, C., et al. (2015). \"Fast quantitative susceptibility mapping using 3D EPI and total generalized variation.\" *NeuroImage*, 111:622-630. https://doi.org/10.1016/j.neuroimage.2015.02.041",
};

const CITE_QSMART: Citation = Citation {
    key: "yaghmaie2021",
    text: "Yaghmaie, N., Syeda, W., et al. (2021). \"QSMART: Quantitative Susceptibility Mapping Artifact Reduction Technique.\" *NeuroImage*, 231:117701. https://doi.org/10.1016/j.neuroimage.2020.117701",
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

const CITE_CLEARSWI: Citation = Citation {
    key: "eckstein2024",
    text: "Eckstein, K., et al. (2024). \"CLEAR-SWI: Computational Efficient T2* Weighted Imaging.\" *Proc. ISMRM*.",
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

/// Generate a methods description and citation list from a pipeline configuration.
/// Uses "qsmxt.rs" as the tool name by default.
pub fn generate_methods(config: &PipelineConfig) -> String {
    generate_methods_for(config, "qsmxt.rs")
}

/// Generate methods for a specific tool (e.g. "qsmxt.rs" or "QSMbly").
pub fn generate_methods_for(config: &PipelineConfig, tool: &str) -> String {
    let mut sentences = Vec::new();
    let mut citations: Vec<&Citation> = Vec::new();

    let (cite, author_year) = match tool {
        "QSMbly" | "qsmbly" => (&CITE_QSMBLY, "Stewart, 2026"),
        _ => (&CITE_QSMXT, "Stewart, 2026"),
    };

    if config.pipeline.do_qsm {
        sentences.push(format!("QSM processing was performed using {} ({}).", tool, author_year));
    } else {
        sentences.push(format!("MRI processing was performed using {} ({}).", tool, author_year));
    }
    add_citation(&mut citations, cite);

    // Masking (inhomogeneity correction is described inline)
    describe_masking(config, &mut sentences, &mut citations);

    // QSM-specific pipeline
    if config.pipeline.do_qsm {
        match config.inversion.algorithm {
            QsmAlgorithm::Tgv => {
                sentences.push("QSM was computed using the Total Generalized Variation (TGV) algorithm (Langkammer et al., 2015), which performs phase unwrapping, background field removal, and dipole inversion in a single step.".to_string());
                add_citation(&mut citations, &CITE_TGV);
            }
            QsmAlgorithm::Qsmart => {
                // QSMART consumes a pre-computed total field, so the field-mapping
                // stage (phase offset, unwrapping, B0 estimation) still runs and must
                // be described.
                describe_field_mapping(config, &mut sentences, &mut citations);
                let (inv_name, inv_cite) = inversion_name_cite(config.inversion.qsmart.inversion);
                sentences.push(format!(
                    "QSM was computed using QSMART (Yaghmaie et al., 2021), a two-stage approach using \
                     spatially dependent filtering for background field removal, {} dipole inversion ({}), \
                     and Frangi vesselness-based tissue/vasculature separation.",
                    inv_name, cite_inline(inv_cite),
                ));
                add_citation(&mut citations, &CITE_QSMART);
                add_citation(&mut citations, inv_cite);
            }
            _ => {
                describe_field_mapping(config, &mut sentences, &mut citations);

                // Background removal
                {
                    let alg = &config.bg_removal.algorithm;
                    let (name, cite) = match alg {
                        BfAlgorithm::Vsharp => ("V-SHARP", &CITE_VSHARP),
                        BfAlgorithm::Pdf => ("PDF", &CITE_PDF),
                        BfAlgorithm::Lbv => ("LBV", &CITE_LBV),
                        BfAlgorithm::Ismv => ("iSMV", &CITE_ISMV),
                        BfAlgorithm::Sharp => ("SHARP", &CITE_SHARP),
                        BfAlgorithm::Resharp => ("RESHARP", &CITE_RESHARP),
                        BfAlgorithm::Harperella => ("HARPERELLA", &CITE_HARPERELLA),
                        BfAlgorithm::Iharperella => ("iHARPERELLA", &CITE_IHARPERELLA),
                    };
                    sentences.push(format!("Background field removal was performed using {} ({}).", name, cite_inline(cite)));
                    add_citation(&mut citations, cite);
                }

                // Dipole inversion
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                sentences.push(format!("Dipole inversion was performed using {} ({}).", name, cite_inline(cite)));
                add_citation(&mut citations, cite);
            }
        }

        // Referencing
        match config.qsm.reference {
            QsmReference::Mean => {
                sentences.push("The resulting susceptibility map was mean-referenced within the brain mask.".to_string());
            }
            QsmReference::None => {
                sentences.push("No susceptibility referencing was applied.".to_string());
            }
        }
    }

    // SWI
    if config.pipeline.do_swi {
        sentences.push("Susceptibility-weighted images were computed using CLEAR-SWI (Eckstein et al., 2024).".to_string());
        add_citation(&mut citations, &CITE_CLEARSWI);
    }

    // T2*/R2*
    if config.pipeline.do_t2starmap && config.pipeline.do_r2starmap {
        sentences.push("T2* and R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
        add_citation(&mut citations, &CITE_ARLO);
    } else if config.pipeline.do_t2starmap {
        sentences.push("T2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
        add_citation(&mut citations, &CITE_ARLO);
    } else if config.pipeline.do_r2starmap {
        sentences.push("R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
        add_citation(&mut citations, &CITE_ARLO);
    }

    // DICOM export
    if config.pipeline.export_dicom {
        sentences.push("Final maps were additionally exported as DICOM series.".to_string());
    }

    // Build output
    let mut out = String::new();
    out.push_str("# Methods\n\n");
    out.push_str(&sentences.join(" "));
    out.push_str("\n\n");

    // Citations
    if !citations.is_empty() {
        out.push_str("## References\n\n");
        for cite in &citations {
            out.push_str(&format!("- {}\n", cite.text));
        }
    }

    out
}

/// Describe the field-mapping stage: phase offset / bipolar correction, phase
/// unwrapping, and B0 estimation. Shared by the standard pipeline and QSMART
/// (which consumes a pre-computed total field, so these steps still run).
fn describe_field_mapping(config: &PipelineConfig, sentences: &mut Vec<String>, citations: &mut Vec<&Citation>) {
    // Phase offset removal and/or bipolar correction
    match (config.field_mapping.phase_offset_removal, config.field_mapping.bipolar_correction) {
        (true, true) => {
            sentences.push("Phase offset removal (Eckstein et al., 2018) and bipolar gradient correction (Eckstein, 2021) were applied.".to_string());
            add_citation(citations, &CITE_MCPC3DS);
            add_citation(citations, &CITE_BIPOLAR);
        }
        (true, false) => {
            sentences.push("Phase offset removal was performed using the HIP method (Eckstein et al., 2018).".to_string());
            add_citation(citations, &CITE_MCPC3DS);
        }
        (false, true) => {
            sentences.push("Bipolar gradient correction (Eckstein, 2021) was applied to remove readout-induced phase artefacts.".to_string());
            add_citation(citations, &CITE_BIPOLAR);
        }
        (false, false) => {}
    }

    // Unwrapping
    match config.field_mapping.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => {
            let mode = if config.field_mapping.romeo.individual { "individual per-echo" } else { "template-based temporal" };
            sentences.push(format!("Phase unwrapping was performed using ROMEO ({} mode) (Dymerska et al., 2021).", mode));
            add_citation(citations, &CITE_ROMEO);
        }
        UnwrappingAlgorithm::Laplacian => {
            sentences.push("Phase unwrapping was performed using the Laplacian method (Schofield & Zhu, 2003).".to_string());
            add_citation(citations, &CITE_LAPLACIAN_UNWRAP);
        }
    }

    // B0 estimation
    match config.field_mapping.b0_estimation {
        B0Estimation::WeightedAvg => {
            let wt = format!("{}", config.field_mapping.b0_weight_type);
            sentences.push(format!("The B0 field map was estimated using weighted averaging ({} weighting).", wt));
        }
        B0Estimation::LinearFit => {
            sentences.push("The B0 field map was estimated using magnitude-weighted linear fit of phase vs echo time.".to_string());
        }
    }
}

fn describe_masking(config: &PipelineConfig, sentences: &mut Vec<String>, citations: &mut Vec<&Citation>) {
    if config.masking.sections.is_empty() {
        return;
    }

    // Describe inhomogeneity correction if enabled, with context about what it affects.
    // The RSS-combined magnitude is used for both Magnitude and PhaseQuality masking inputs,
    // while MagnitudeFirst/MagnitudeLast use their specific echo magnitudes.
    if config.masking.inhomogeneity_correction {
        add_citation(citations, &CITE_BIAS);

        let inputs: Vec<MaskingInput> = config.masking.sections.iter().map(|s| s.input).collect();
        let uses_rss = inputs.iter().any(|i| matches!(i, MaskingInput::Magnitude | MaskingInput::PhaseQuality));
        let uses_first = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeFirst));
        let uses_last = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeLast));

        if uses_rss && !uses_first && !uses_last {
            sentences.push("Inhomogeneity correction (Eckstein et al., 2019) was applied to the RSS-combined magnitude image.".to_string());
        } else if !uses_rss && (uses_first || uses_last) {
            let echo = if uses_first { "first" } else { "last" };
            sentences.push(format!(
                "Inhomogeneity correction (Eckstein et al., 2019) was applied to the {}-echo magnitude image.",
                echo
            ));
        } else {
            sentences.push("Inhomogeneity correction (Eckstein et al., 2019) was applied to the magnitude data.".to_string());
        }
    }

    let section_count = config.masking.sections.len();
    let mut section_descs = Vec::new();

    for section in &config.masking.sections {
        let mut parts = Vec::new();

        // Input description — clarify which magnitude goes into ROMEO
        let input_desc = match section.input {
            MaskingInput::PhaseQuality => {
                add_citation(citations, &CITE_ROMEO);
                if config.masking.inhomogeneity_correction {
                    "the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude)"
                } else {
                    "the ROMEO phase quality map (computed from phase data and the RSS-combined magnitude)"
                }
            }
            MaskingInput::Magnitude => {
                if config.masking.inhomogeneity_correction {
                    "the inhomogeneity-corrected RSS-combined magnitude image"
                } else {
                    "the RSS-combined magnitude image"
                }
            }
            MaskingInput::MagnitudeFirst => {
                if config.masking.inhomogeneity_correction {
                    "the inhomogeneity-corrected first-echo magnitude image"
                } else {
                    "the first-echo magnitude image"
                }
            }
            MaskingInput::MagnitudeLast => {
                if config.masking.inhomogeneity_correction {
                    "the inhomogeneity-corrected last-echo magnitude image"
                } else {
                    "the last-echo magnitude image"
                }
            }
        };

        // Generator
        let gen_desc = match &section.generator {
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => {
                add_citation(citations, &CITE_OTSU);
                format!("Otsu thresholding (Otsu, 1979) of {}", input_desc)
            }
            MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => {
                format!("fixed thresholding (value={:.4}) of {}", value.unwrap_or(0.5), input_desc)
            }
            MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } => {
                format!("percentile thresholding ({}th percentile) of {}", value.unwrap_or(75.0), input_desc)
            }
            MaskOp::Bet { fractional_intensity } => {
                add_citation(citations, &CITE_BET);
                format!("BET brain extraction (Smith, 2002; f={:.2}) of {}", fractional_intensity, input_desc)
            }
            _ => format!("{} of {}", section.generator, input_desc),
        };
        parts.push(gen_desc);

        // Refinements
        let refinement_descs: Vec<String> = section.refinements.iter().map(|op| match op {
            MaskOp::Erode { iterations } => format!("erosion ({} iteration{})", iterations, if *iterations != 1 { "s" } else { "" }),
            MaskOp::Dilate { iterations } => format!("dilation ({} iteration{})", iterations, if *iterations != 1 { "s" } else { "" }),
            MaskOp::Close { radius } => format!("morphological closing (radius={})", radius),
            MaskOp::FillHoles { max_size: 0 } => "hole-filling".to_string(),
            MaskOp::FillHoles { max_size } => format!("hole-filling (max {} voxels)", max_size),
            MaskOp::GaussianSmooth { sigma_mm } => format!("Gaussian smoothing (sigma={:.1} mm)", sigma_mm),
            _ => format!("{}", op),
        }).collect();

        if !refinement_descs.is_empty() {
            parts.push(format!("followed by {}", join_list(&refinement_descs)));
        }

        section_descs.push(parts.join(", "));
    }

    if section_count == 1 {
        sentences.push(format!("A brain mask was generated using {}.", section_descs[0]));
    } else {
        sentences.push(format!(
            "A brain mask was generated by combining {} mask sections (OR operation): {}.",
            section_count,
            join_list(&section_descs)
        ));
    }
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
        QsmAlgorithm::Ilsqr => ("iLSQR", &CITE_ILSQR),
        QsmAlgorithm::Tgv | QsmAlgorithm::Qsmart => ("iLSQR", &CITE_ILSQR),
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
        "shmueli2009" => "Shmueli et al., 2009",
        "bilgic2014l2" => "Bilgic et al., 2014",
        "liu2011medi" => "Liu et al., 2011",
        "milovic2018" => "Milovic et al., 2018",
        "li2015" => "Li et al., 2015",
        "sun2014" => "Sun & Wilman, 2014",
        "li2014" => "Li et al., 2014",
        "li2015iharp" => "Li et al., 2015",
        "eckstein2021phd" => "Eckstein, 2021",
        _ => cite.key,
    }
}

fn add_citation<'a>(citations: &mut Vec<&'a Citation>, cite: &'a Citation) {
    if !citations.iter().any(|c| c.key == cite.key) {
        citations.push(cite);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_methods() {
        let config = PipelineConfig::default();
        let out = generate_methods(&config);
        assert!(out.contains("QSM processing was performed using qsmxt.rs"));
        assert!(out.contains("Phase offset removal"));
        assert!(out.contains("ROMEO"));
        assert!(out.contains("V-SHARP"));
        assert!(out.contains("RTS"));
        assert!(out.contains("mean-referenced"));
        assert!(out.contains("# Methods"));
        assert!(out.contains("## References"));
    }

    #[test]
    fn test_tgv_methods() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Tgv;
        let out = generate_methods(&config);
        assert!(out.contains("Total Generalized Variation (TGV)"));
        assert!(!out.contains("Phase unwrapping was performed"));
        assert!(!out.contains("Background field removal"));
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

    #[test]
    fn test_nltv_cites_milovic_not_rts() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Nltv;
        let out = generate_methods(&config);
        assert!(out.contains("Milovic"), "NLTV must cite Milovic et al. 2018: {}", out);
        assert!(!out.contains("Rapid two-step"), "NLTV must not cite the RTS paper: {}", out);
    }

    #[test]
    fn test_laplacian_methods() {
        let mut config = PipelineConfig::default();
        config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let out = generate_methods(&config);
        assert!(out.contains("Laplacian method"));
        assert!(out.contains("Schofield"));
    }

    #[test]
    fn test_romeo_individual_mode() {
        let config = PipelineConfig::default();
        let out = generate_methods(&config);
        assert!(out.contains("individual per-echo"));
    }

    #[test]
    fn test_romeo_template_mode() {
        let mut config = PipelineConfig::default();
        config.field_mapping.romeo.individual = false;
        let out = generate_methods(&config);
        assert!(out.contains("template-based temporal"));
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
        assert!(out.contains("weighted averaging"));
        assert!(out.contains("phase-snr"));
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
        ] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            let out = generate_methods(&c);
            assert!(out.contains(expected), "missing {} for {:?}", expected, alg);
        }
    }

    #[test]
    fn test_swi_methods() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_swi = true;
        let out = generate_methods(&config);
        assert!(out.contains("CLEAR-SWI"));
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
            generator: MaskOp::Bet { fractional_intensity: 0.35 },
            refinements: vec![],
        }];
        let out = generate_methods(&config);
        assert!(out.contains("BET brain extraction"));
        assert!(out.contains("Smith, 2002"));
        assert!(out.contains("f=0.35"));
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
        assert!(out.contains("erosion (1 iteration)"));
        assert!(!out.contains("iterations)"));
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
        assert!(out.contains("morphological closing (radius=5)"));
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
        assert!(out.contains("Gaussian smoothing (sigma=2.5 mm)"));
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
        assert!(out.contains("hole-filling (max 500 voxels)"));
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
            (QsmAlgorithm::Ilsqr, "iLSQR"),
        ] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            let out = generate_methods(&c);
            assert!(out.contains("Dipole inversion was performed using"), "missing dipole sentence for {:?}", alg);
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
        assert!(out.contains("CLEAR-SWI"));
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
