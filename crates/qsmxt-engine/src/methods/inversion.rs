//! Methods prose for dipole inversion: the inversion's name, citation and changed settings, MEDI's
//! variants, deep-learning tiling, and HEIDI.

use super::*;

/// The non-default settings of the selected dipole inversion. Deep-learning inversions have none
/// here (tiling is worded separately).
pub(super) fn inversion_changes(config: &PipelineConfig, alg: QsmAlgorithm) -> Changed {
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
        QsmAlgorithm::Ndi => {
            c.add_fields(&i.ndi, &d.ndi, &["te", "weight_norm"]);
            c.add(i.ndi.weight_norm != d.ndi.weight_norm, || match i.ndi.weight_norm {
                NdiWeightNorm::GlobalMax => "the magnitude weight normalised by the magnitude's maximum".to_string(),
                NdiWeightNorm::MaskP999 => "the magnitude weight normalised by the magnitude's 99.9th percentile within the brain mask".to_string(),
            });
        }
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
pub(super) fn echo_time_prose(te: EchoTimeChoice) -> String {
    match te {
        EchoTimeChoice::First => "the first echo time".into(),
        EchoTimeChoice::Last => "the last echo time".into(),
        EchoTimeChoice::Spacing => "the echo spacing".into(),
        EchoTimeChoice::Sum => "the sum of the echo times".into(),
        EchoTimeChoice::Seconds(_) => format!("an echo time of {te} ms"),
    }
}

/// MEDI's mSMV settings, which [`medi_msmv`] words rather than the list of changed settings.
pub(super) const MEDI_MSMV_FIELDS: [&str; 4] = ["msmv", "msmv_maxk", "msmv_vessel_radius", "msmv_prefilter"];

/// MEDI's SMV settings, which [`medi_smv`] words rather than the list of changed settings.
pub(super) const MEDI_SMV_FIELDS: [&str; 3] = ["smv", "smv_radius", "smv_shrink_mask"];

/// MEDI-SMV, after MEDI's name and citation in the inversion's sentence: ", with the local field
/// high-passed by a 5 mm spherical mean value (SMV) filter within the mask shrunk by that
/// radius"; nothing for MEDI without SMV, MEDI-mSMV ([`medi_msmv`]) or another inversion.
pub(super) fn medi_smv(config: &PipelineConfig) -> String {
    let m = &config.inversion.medi;
    if config.inversion.algorithm != QsmAlgorithm::Medi || !m.smv || m.msmv {
        return String::new();
    }
    format!(", with the local field high-passed by a {} mm spherical mean value (SMV) filter {}",
        m.smv_radius,
        if m.smv_shrink_mask { "within the mask shrunk by that radius" } else { "over the full mask" })
}

/// MEDI+0's settings, which [`medi_csf`] words rather than the list of changed settings.
pub(super) const MEDI_CSF_FIELDS: [&str; 4] = ["csf", "lambda_csf", "csf_r2s_threshold", "csf_erode"];

/// MEDI+0's sentence, after the inversion's: "MEDI's zero reference to the ventricular CSF
/// (MEDI+0; Liu et al., 2018) was applied.", with its changed settings, when the runs applied it.
/// QSM.rs runs MEDI without the term on fewer than three echoes (the CSF is found from R2*; the
/// plan warns), and each run records which it did ([`Topic::MediCsf`]): nothing is said when no
/// run applied it, and where the runs differ, or nothing is known of them, the configuration's
/// wording says the condition ("to runs of at least three echoes"). Nothing with the CSF term off
/// or another inversion.
pub(super) fn describe_medi_csf(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
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
pub(super) fn medi_msmv(config: &PipelineConfig, doc: &mut Draft) -> String {
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

/// HEIDI's sentence, when it ran: after the inversion (each echo's, for a per-echo QSM), or on a
/// two-pass run's combined map.
pub(super) fn describe_heidi(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let i = &config.inversion;
    if !config.pipeline.do_qsm || !i.heidi.enabled {
        return;
    }
    doc.cite(&CITE_HEIDI);
    // As [`describe_two_pass`] decides whether two-pass ran.
    let two_pass = config.masking.two_pass
        && !(ran.count(Topic::TwoPass, Choice::NotApplied) > 0 && ran.ran(Stage::Mask(Pass::Reliable)) == 0);
    let what = match (i.algorithm, two_pass) {
        (_, true) => "the combined two-pass map".to_string(),
        (QsmAlgorithm::Lsqr, false) => "the LSQR map, as published".to_string(),
        (alg, false) => format!("the {} map", words::inversion(alg)),
    };
    let mut changed = Changed::default();
    changed.add_fields(&i.heidi, &InversionConfig::default().heidi, &["enabled", "te"]);
    if i.heidi.te != InversionConfig::default().heidi.te {
        changed.0.push(format!("the field converted to phase at {}", echo_time_prose(i.heidi.te)));
    }
    let stages: &[Stage] = if two_pass { &[Stage::TwoPassCombination] } else { &[Stage::Inversion(Pass::Main)] };
    doc.say(stages, format!(
        "Streaking was reduced with HEIDI (Homogeneity Enabled Incremental Dipole Inversion) ({}) on {what}: its well-conditioned k-space coefficients \
         were kept, and the dipole cone was filled by total variation weighted by the field's \
         homogeneity{}.",
        cite_inline(&CITE_HEIDI), changed.comma_with()));
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
pub(super) fn say_tiling(config: &PipelineConfig, stage: fn(Pass) -> Stage, doc: &mut Draft) {
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
pub(super) fn inversion_name_cite(alg: QsmAlgorithm) -> (&'static str, &'static Citation) {
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
        QsmAlgorithm::Qsmnetme => ("QSMnetME (deep-learning dipole inversion)", &CITE_CHISEPNET),
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
