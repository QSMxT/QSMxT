use super::*;

/// A per-echo QSM is said plainly: each echo reconstructed on its own, the mean reported, and
/// no echo combination; DECOMPOSE takes each echo's map.
#[test]
fn a_per_echo_qsm_is_said_plainly() {
    let mut c = PipelineConfig::default();
    c.qsm.per_echo = true;
    let text = generate_methods(&c);
    assert!(text.contains("For multi-echo data, QSM was reconstructed separately for each echo"), "{text}");
    assert!(text.contains("the susceptibility map reported is the mean across echoes"), "{text}");
    assert!(!text.contains("B0 field map was estimated") && !text.contains("Phase offset removal"), "{text}");
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::Decompose;
    let text = generate_methods(&c);
    assert!(text.contains("Each echo's susceptibility map was used for the signal phase at that echo."), "{text}");
    c.qsm.per_echo = false;
    let text = generate_methods(&c);
    assert!(text.contains("The same susceptibility map was used for the signal phase at every echo.")
            && !text.contains("separately for each echo:"), "{text}");
}

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
    assert!(out.contains("31 FreeSurfer-labelled structures"), "out: {out}");
    assert!(out.contains("parcellated from the GRE magnitude"), "out: {out}");
    assert!(out.contains("10.1016/j.media.2023.102789"), "SynthSeg citation missing: {out}");
    assert!(!out.contains("10.1073/pnas.2216399120"), "v1 does not cite the 2.0 paper: {out}");
    assert!(!out.contains("summarised within every segmented structure"),
            "no statistics were asked for: {out}");

    config.pipeline.do_analysis = true;
    config.segmentation.version = crate::enums::SynthSegVersion::V2;
    let out = generate_methods(&config);
    assert!(out.contains("SynthSeg v2"), "out: {out}");
    assert!(out.contains("32 FreeSurfer-labelled structures"), "out: {out}");
    assert!(out.contains("10.1016/j.media.2023.102789") && out.contains("10.1073/pnas.2216399120"),
            "SynthSeg 2.0 cites both papers: {out}");
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
fn heidi_after_another_inversion_and_after_two_pass() {
    let mut config = PipelineConfig::default();
    config.inversion.algorithm = QsmAlgorithm::Rts;
    config.inversion.heidi.enabled = true;
    let out = generate_methods(&config);
    assert!(out.contains("with HEIDI") && out.contains("on the RTS map"), "out: {out}");
    config.masking.two_pass = true;
    let out = generate_methods(&config);
    assert!(out.contains("on the combined two-pass map"), "out: {out}");
    config.inversion.heidi.enabled = false;
    assert!(!generate_methods(&config).contains("HEIDI"));
}

#[test]
fn lsqr_and_heidi_are_named_and_cited() {
    let mut config = PipelineConfig::default();
    config.inversion.algorithm = QsmAlgorithm::Lsqr;
    let out = generate_methods(&config);
    assert!(out.contains("minimally regularised LSQR"), "out: {out}");
    assert!(out.contains("10.1118/1.3481505"), "LSQR citation missing: {out}");

    config.inversion.algorithm = QsmAlgorithm::Lsqr;
    config.inversion.heidi.enabled = true;
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
    assert!(out.contains("holes only at strong susceptibility sources"), "out: {out}");
    assert!(out.contains("10.1002/mrm.29048"), "two-pass citation missing: {out}");

    // It describes the reconstruction, so it belongs after inversion and before referencing.
    let inv = out.find("Dipole inversion was performed").expect("inversion sentence");
    let two = out.find("Two-pass artefact reduction").expect("two-pass sentence");
    let reference = out.find("referenced to the mean susceptibility").expect("referencing sentence");
    assert!(inv < two && two < reference, "out-of-order methods: {out}");
}

/// The default (local) two-pass says what was done: holes only at strong sources, LBV in the
/// rim, the second map used near the holes; the published algorithm keeps its own text.
#[test]
fn two_pass_methods_say_how_the_passes_were_made_and_merged() {
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    let out = generate_methods(&config);
    assert!(out.contains("holes only at strong susceptibility sources"), "out: {out}");
    assert!(out.contains("filled by LBV background field removal (Zhou et al., 2014)"), "out: {out}");
    assert!(out.contains("used near the holes and blended linearly into the first"), "out: {out}");
    config.masking.two_pass_mode = crate::enums::TwoPassMode::Original;
    let out = generate_methods(&config);
    assert!(out.contains("taking the second reconstruction wherever it was defined"), "out: {out}");
    assert!(!out.contains("filled by LBV"), "out: {out}");
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

/// χ-sep iLSQR and χ-sep MEDI say they reconstructed the QSM they separated, and any iLSQR
/// setting changed.
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
    assert!(generate_methods(&config).contains(&format!("{own}.")), "χ-sep MEDI's own settings, at their defaults");
    config.separation.chi_sep_medi.qsm_ilsqr.max_iter = 40;
    assert!(generate_methods(&config).contains(&format!("{own} with at most 40 iterations.")), "{}", generate_methods(&config));
    config.separation.algorithm = SeparationAlgorithm::Decompose;
    assert!(!generate_methods(&config).contains(own));
}

/// HC-ChiSep cites its own work and the hollow-cylinder model's paper, in the text and the
/// reference list.
#[test]
fn hc_chisep_methods_cite_both_works() {
    let mut config = PipelineConfig::default();
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = SeparationAlgorithm::HcChisep;
    let out = generate_methods(&config);
    assert!(out.contains("hollow-cylinder χ-separation (HC-ChiSep) (Stewart et al., 2026; Wharton & Bowtell, 2012)"), "{out}");
    let refs = out.split("## References").nth(1).expect("a reference list");
    assert!(refs.contains("- Stewart, A., et al. (2026). \"Hollow-cylinder susceptibility source separation (HC-ChiSep).\"\n"), "{refs}");
    assert!(refs.contains("- Wharton, S., Bowtell, R. (2012). \"Fiber orientation-dependent white matter contrast in gradient echo MRI.\" \
        *Proceedings of the National Academy of Sciences*, 109(45):18559-18564. https://doi.org/10.1073/pnas.1211075109"), "{refs}");
}

/// R2*-QSM's two Dimov 2022 papers are told apart as 2022a and 2022b, in the order the reference
/// list gives them, in the text, the reference list and the structured methods, whose keys stay
/// the works' own.
#[test]
fn r2star_qsms_two_dimov_papers_get_year_letters() {
    let mut config = PipelineConfig::default();
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = SeparationAlgorithm::R2starQsm;
    let out = generate_methods(&config);
    assert!(out.contains("R2*-QSM susceptibility source separation (Dimov et al., 2022a; Dimov et al., 2022b)"), "{out}");
    assert!(!out.contains("Dimov et al., 2022)") && !out.contains("(2022)."), "{out}");
    let refs = out.split("## References").nth(1).expect("a reference list");
    let a = refs.find("- Dimov, A. V., et al. (2022a). \"Susceptibility source separation from gradient echo data").expect(refs);
    let b = refs.find("- Dimov, A. V., et al. (2022b). \"Magnetic Susceptibility Source Separation Solely").expect(refs);
    assert!(a < b, "{refs}");

    let f = crate::plan::tests::facts();
    let m = methods_structured(&config, &crate::plan::plan(&config, &f), "qsmxt");
    assert_eq!(m.to_markdown(), out);
    let keys: Vec<&str> = m.references.iter().map(|r| r.key.as_str()).filter(|k| k.starts_with("dimov")).collect();
    let cited = m.sentences.iter().find(|s| s.text.contains("Dimov et al., 2022a")).expect("the separation sentence");
    assert_eq!(keys.len(), 2);
    for k in keys {
        assert!(!k.ends_with('a') && !k.ends_with('b') && cited.citations.iter().any(|c| c == k), "{k}");
    }
}

/// A work cited in two sentences is one work: its in-text citation gets no letter.
#[test]
fn a_work_cited_twice_gets_no_year_letter() {
    let mut config = PipelineConfig::default();
    config.masking.two_pass = true;
    let out = generate_methods(&config);
    assert_eq!(out.matches("(Otsu, 1979)").count(), 2, "{out}");
    assert!(!out.contains("1979a") && out.contains("Otsu, N. (1979)."), "{out}");
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
    assert!(!generate_methods(&config).contains("normalised by"));
    config.inversion.ndi.weight_norm = NdiWeightNorm::MaskP999;
    assert!(generate_methods(&config).contains("the magnitude weight normalised by the magnitude's 99.9th percentile within the brain mask"));
    config.inversion.algorithm = QsmAlgorithm::Rts;
    config.inversion.rts.gamma = 2.0;
    assert!(generate_methods(&config).contains("γ (ρ increase factor) = 2"));
    config.inversion.algorithm = QsmAlgorithm::Whqsm;
    config.inversion.whqsm.weight = WhqsmWeight::InVivo;
    assert!(generate_methods(&config).contains("data weight = in-vivo"));
    config.inversion.whqsm.weight = WhqsmWeight::FlatInMask;
    assert!(generate_methods(&config).contains("data weight = flat-in-mask"));
    for (alg, rim, text) in [(BfAlgorithm::Harperella, HarperellaRim::Sti, "the 20 % largest Laplacian values"),
                             (BfAlgorithm::Iharperella, HarperellaRim::Whole, "every Laplacian value")] {
        let mut config = PipelineConfig::default();
        config.bg_removal.algorithm = alg;
        assert!(!generate_methods(&config).contains("boundary rim"), "{alg}: the default is not stated");
        config.bg_removal.harperella.rim = rim;
        config.bg_removal.iharperella.rim = rim;
        let out = generate_methods(&config);
        assert!(out.contains(&format!("{text} in the brain's boundary rim set to zero")), "{out}");
    }
    config.bg_removal.algorithm = BfAlgorithm::Pdf;
    config.bg_removal.pdf.n_cg = 300;
    assert!(generate_methods(&config).contains("at most 300 conjugate-gradient iterations"));
    assert!(!generate_methods(&config).contains("was eroded by"));
    config.bg_removal.erode_voxels = 3;
    assert!(generate_methods(&config).contains("Before background field removal, the brain mask was eroded by 3 voxels."));
    config.bg_removal.erode_voxels = 1;
    assert!(generate_methods(&config).contains("eroded by 1 voxel."));
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

/// QSM.jl's MGPCG solver is described with its boundary condition, which also removes the
/// background, and cited.
#[test]
fn mgpcg_laplacian_solver_is_described() {
    let mut config = PipelineConfig::default();
    config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
    config.field_mapping.laplacian_solver = LaplacianSolver::Mgpcg;
    let out = generate_methods(&config);
    for said in ["using the Laplacian method (Schofield & Zhu, 2003), solved inside the brain mask",
                 "multigrid-preconditioned conjugate gradients as in QSM.jl",
                 "equivalent to LBV background removal (Zhou et al., 2014)",
                 "https://doi.org/10.1002/nbm.3064"] {
        assert!(out.contains(said), "missing {said:?}: {out}");
    }
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
    // QSMART unwraps each echo's raw phase, with no phase-offset removal first.
    config.field_mapping.phase_offset_removal = false;
    config.masking.sections = crate::masking::mask_presets().into_iter().find(|(n, _)| *n == "bet").unwrap().1.sections;
    let out = generate_methods(&config);
    for said in ["separately for each echo, on the raw phase over the whole volume, using the Laplacian method",
                 "on the volume mirrored along each axis",
                 "fit of phase vs echo time through the origin, as in QSMART (Yaghmaie et al., 2021)",
                 "voxels whose blurred fit residual exceeded 40 were excluded from the QSMART reconstruction",
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
fn test_romeo_default_mode() {
    // Template unwrapping is the default, so the mode is not stated.
    let config = PipelineConfig::default();
    let out = generate_methods(&config);
    assert!(out.contains("Phase unwrapping was performed using ROMEO (Dymerska et al., 2021)."), "{out}");
    assert!(!out.contains("mode"), "{out}");
}

#[test]
fn test_romeo_individual_mode() {
    let mut config = PipelineConfig::default();
    config.field_mapping.romeo.individual = true;
    let out = generate_methods(&config);
    assert!(out.contains("using ROMEO (Dymerska et al., 2021) in individual per-echo mode."), "{out}");
}

/// MEDI's region growing is stated plainly and cited, on the per-echo path and after the
/// complex fit; phase offset removal is said as with ROMEO.
#[test]
fn test_medi_region_growing_methods() {
    let mut config = PipelineConfig::default();
    config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::MediRegionGrowing;
    let out = generate_methods(&config);
    assert!(out.contains("Phase unwrapping was performed separately for each echo using MEDI's magnitude-guided region growing (Liu et al., 2013)."), "{out}");
    assert!(out.contains("https://doi.org/10.1002/mrm.24272"), "{out}");
    assert!(out.contains("Phase offset removal"), "{out}");
    for unsaid in ["using ROMEO", "mexUnwrap", "STI", "Biobank"] {
        assert!(!out.contains(unsaid), "{unsaid:?} in {out}");
    }
    assert!(words::unwrapping_settings(&config).is_empty());
    config.field_mapping.b0_estimation = B0Estimation::ComplexFit;
    let out = generate_methods(&config);
    assert!(out.contains("the fitted phase per echo spacing was unwrapped using MEDI's magnitude-guided region growing (unwrapPhase, MEDI toolbox) and divided by the echo spacing."), "{out}");
}

/// PRELUDE at its defaults is stated plainly and cited; its settings are said only when
/// changed. Phase offset removal and bipolar correction are said as with ROMEO.
#[test]
fn test_prelude_methods() {
    let mut config = PipelineConfig::default();
    config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Prelude;
    let out = generate_methods(&config);
    assert!(out.contains("Phase unwrapping was performed separately for each echo using PRELUDE (Jenkinson, 2003)."), "{out}");
    assert!(out.contains("Jenkinson, M. (2003). \"Fast, automated, N-dimensional phase-unwrapping algorithm.\""), "{out}");
    assert!(out.contains("https://doi.org/10.1002/mrm.10354"), "{out}");
    assert!(out.contains("Phase offset removal"), "{out}");
    for unsaid in ["using ROMEO", "partition", "labelled", "FSL", "STI", "Biobank"] {
        assert!(!out.contains(unsaid), "{unsaid:?} in {out}");
    }
    config.field_mapping.bipolar_correction = true;
    assert!(generate_methods(&config).contains("bipolar gradient correction"));

    config.field_mapping.prelude.num_phase_partitions = 6;
    let out = generate_methods(&config);
    assert!(out.contains("using PRELUDE (Jenkinson, 2003) with 6 phase partitions."), "{out}");
    config.field_mapping.prelude.labelling = PreludeLabelling::Volume;
    let out = generate_methods(&config);
    assert!(out.contains("using PRELUDE (Jenkinson, 2003) with 6 phase partitions and regions labelled over the whole volume."), "{out}");
    config.field_mapping.prelude.num_phase_partitions = 8;
    config.field_mapping.prelude.labelling = PreludeLabelling::Slices;
    let out = generate_methods(&config);
    assert!(out.contains("using PRELUDE (Jenkinson, 2003) with regions labelled slice by slice."), "{out}");
    assert_eq!(words::unwrapping_settings(&config), ["regions labelled slice by slice"]);
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
fn test_laplacian_methods_say_offset_removal() {
    let mut config = PipelineConfig::default();
    config.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
    let out = generate_methods(&config);
    assert!(out.contains("separately for each echo"), "{out}");
    let without = { config.field_mapping.phase_offset_removal = false; generate_methods(&config) };
    assert_ne!(out, without, "offset removal before Laplacian unwrapping is said: {out}");
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
    // PDF's and MEDI's papers are both "Liu et al., 2011": lettered in the order they are listed.
    assert!(out.contains("Background field removal was performed using PDF (Liu et al., 2011a)"), "{out}");
    assert!(out.contains("Liu, T., et al. (2011a). \"A novel background field removal method") && out.contains("Liu, T., et al. (2011b). \"Morphology enabled"), "{out}");
    assert!(out.contains("(Liu et al., 2011b), with the local field high-passed by a 5 mm spherical mean value (SMV) filter within the mask shrunk by that radius."), "{out}");
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
    assert!(!out.contains("filtered phase"), "{out}");

    // The filtered phase, when written: what it is and its units.
    config.swi.filtered_phase = true;
    let out = generate_methods(&config);
    assert!(out.contains("The filtered phase that weights these images, CLEAR-SWI's phase after an in-plane \
                          Gaussian high-pass filter within a head mask from the first echo's magnitude, was also \
                          saved, in Hz for multi-echo data and in radians for single-echo data."), "{out}");
    config.swi.unwrap = SwiUnwrap::Laplacian;
    config.swi.hp_sigma = [4.0, 4.0, 2.0];
    assert!(generate_methods(&config).contains("phase after a Gaussian high-pass filter"));
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

/// The Tukey window is said only when it is on, with its taper ratio, for any output.
#[test]
fn the_tukey_window_is_said_when_on() {
    let mut config = PipelineConfig::default();
    assert!(!generate_methods(&config).contains("Tukey"));
    config.pipeline.tukey_window = 0.4;
    assert!(generate_methods(&config).contains("multiplied in k-space by a Tukey window with a taper ratio of 0.4"));
    config.pipeline.do_qsm = false;
    config.pipeline.do_r2starmap = true;
    assert!(generate_methods(&config).contains("Tukey window"));
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

// ─── Citation inline tests ───

#[test]
fn test_cite_inline_known_keys() {
    let inline = |stage, id| cite_inline(&method(stage, id));
    assert_eq!(inline(MethodStage::BackgroundRemoval, "vsharp"), "Wu et al., 2012");
    assert_eq!(inline(MethodStage::BackgroundRemoval, "sharp"), "Schweser et al., 2011");
    assert_eq!(inline(MethodStage::BackgroundRemoval, "pdf"), "Liu et al., 2011");
    assert_eq!(inline(MethodStage::BackgroundRemoval, "ismv"), "Wen et al., 2014");
    assert_eq!(inline(MethodStage::BackgroundRemoval, "lbv"), "Zhou et al., 2014");
    assert_eq!(inline(MethodStage::Inversion, "rts"), "Kames et al., 2018");
    assert_eq!(inline(MethodStage::BackgroundRemoval, "resharp"), "Sun & Wilman, 2014");
    assert_eq!(inline(MethodStage::BackgroundRemoval, "harperella"), "Li et al., 2014");
    assert_eq!(inline(MethodStage::BackgroundRemoval, "iharperella"), "Li et al., 2015");
    assert_eq!(inline(MethodStage::PhaseCorrection, "bipolar-correction"), "Eckstein, 2021");
}

#[test]
fn test_cite_inline_joins_several_works() {
    let a = Citation { key: "a2001", text: "A".into(), inline: "A et al., 2001".into() };
    let b = Citation { key: "b2002", text: "B".into(), inline: "B & C, 2002".into() };
    assert_eq!(cite_inline(std::slice::from_ref(&a)), "A et al., 2001");
    assert_eq!(cite_inline(&[a.clone(), b.clone()]), "A et al., 2001; B & C, 2002");
    let a2 = Citation { key: "a2001b", text: "A2".into(), inline: "A et al., 2001".into() };
    assert_eq!(cite_inline(&[a, a2, b]), "A et al., 2001; B & C, 2002");
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
        (QsmAlgorithm::Qsmnetme, "QSMnetME"),
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
