use super::*;
use crate::enums::{BfAlgorithm, QsmAlgorithm, SeparationAlgorithm};

#[test]
fn dl_separation_forces_r2prime() {
    // SUSEP-Net and the network alone consume local field + QSM + R2', so they must force R2'.
    let mut native = PipelineConfig::default();
    native.separation.chi_sepnet.route = ChiSepNetRoute::Native;
    native.separation.algorithm = SeparationAlgorithm::ChiSepNet;
    let mut susep = PipelineConfig::default();
    susep.separation.algorithm = SeparationAlgorithm::SusepNet;
    for mut c in [native, susep] {
        c.pipeline.do_chi_separation = true;
        enforce_separation_dependencies(&mut c);
        assert!(c.pipeline.do_r2primemap, "{:?} must force R2'", c.separation.algorithm);
    }
}

/// χ-sepnet's toolbox route estimates its own R2′ from R2*, so it asks for R2* alone, and an
/// R2′ map stays the user's to ask for. R2′ only ever measured is forced as on any method.
#[test]
fn the_toolbox_route_needs_only_r2star() {
    let toolbox = |edit: fn(&mut PipelineConfig)| {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        edit(&mut c);
        enforce_separation_dependencies(&mut c);
        c.pipeline
    };
    let p = toolbox(|_| {});
    assert!(p.do_r2starmap && !p.do_r2primemap && !p.do_r2map, "{p:?}");
    let p = toolbox(|c| c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet);
    assert!(p.do_r2starmap && !p.do_r2primemap && !p.do_r2map, "{p:?}");
    let p = toolbox(|c| c.pipeline.do_r2primemap = true);
    assert!(p.do_r2primemap && p.do_r2starmap && p.do_r2map, "the user's request stands: {p:?}");
    let p = toolbox(|c| c.separation.r2prime_strategy = R2PrimeStrategy::Mese);
    assert!(p.do_r2primemap && p.do_r2starmap && p.do_r2map, "{p:?}");
    let p = toolbox(|c| c.separation.custom_r2prime_tool = Some("mine".into()));
    assert!(!p.do_r2primemap && !p.do_r2starmap && !p.do_r2map, "a supplied R2′ is used as it is: {p:?}");
}

/// χ-sep MEDI's defaults come from qsm-core rather than a copy here, so the config, schema,
/// TUI, command and methods all follow QSM.rs when it retunes them.
#[test]
fn chi_sep_medi_defaults_are_qsm_cores() {
    let ours = PipelineConfig::default().separation.chi_sep_medi;
    let core = qsm_core::separation::ChiSepMediParams::default();
    assert_eq!((ours.lambda, ours.lambda_csf, ours.dr_pos, ours.dr_neg, ours.pad),
               (core.lambda, core.lambda_csf, core.dr_pos, core.dr_neg, core.pad));
}

/// Configs written before `combine`/`refinements` existed must still load, and keep the
/// union behaviour they were written with.
#[test]
fn a_morphology_step_of_zero_is_refused_in_a_config_file() {
    let toml = "[masking]\n[[masking.sections]]\ninput = \"magnitude\"\ngenerator = { op = \"bet\", fractional_intensity = 0.5 }\n\
                refinements = [{ op = \"erode\", iterations = 0 }]\n";
    let e = PipelineConfig::from_toml(toml).unwrap_err().to_string();
    assert!(e.contains("masking.sections[0].refinements[0]: erode: iterations must be at least 1, got 0"), "{e}");
    assert!(PipelineConfig::from_toml(&toml.replace("iterations = 0", "iterations = 2")).is_ok());
    let e = PipelineConfig::from_toml("[masking]\nrefinements = [{ op = \"close\", radius = 0 }]\n").unwrap_err().to_string();
    assert!(e.contains("masking.refinements[0]: close: radius must be at least 1, got 0"), "{e}");
}

#[test]
fn masking_combine_defaults_to_or_and_round_trips() {
    let legacy: MaskingConfig = toml::from_str("inhomogeneity_correction = true\n").unwrap();
    assert_eq!(legacy.combine, MaskCombine::Or);
    assert!(legacy.refinements.is_empty());

    let mut c = PipelineConfig::default();
    c.masking.combine = MaskCombine::And;
    c.masking.refinements = vec![MaskOp::FillHoles { max_size: 0 }];
    let loaded = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
    assert_eq!(loaded.masking.combine, MaskCombine::And);
    assert_eq!(loaded.masking.refinements, c.masking.refinements);
}

/// The changed settings alone, read back on the defaults, give the config again.
#[test]
fn to_toml_changed_writes_only_what_differs_and_roundtrips() {
    let d = PipelineConfig::default();
    assert_eq!(d.to_toml_changed().unwrap().trim(), "schema_version = 2", "nothing changed, only the version written");

    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Rts;
    c.inversion.rts.delta = 0.22;
    c.bg_removal.algorithm = BfAlgorithm::Pdf;
    c.pipeline.do_swi = true;
    let toml = c.to_toml_changed().unwrap();
    assert!(toml.contains("delta = 0.22") && toml.contains("do_swi = true"), "{toml}");
    assert!(!toml.contains("[inversion.tkd]") && !toml.contains("[masking]"), "only what changed: {toml}");
    assert!(toml.lines().count() < 15, "{toml}");
    let pruned = |c: &PipelineConfig| crate::schema::prune_hidden(c, &Default::default()).to_json().unwrap();
    assert_eq!(PipelineConfig::from_toml(&toml).unwrap().to_json().unwrap(), pruned(&c));

    // A setting that does not apply is left out: RTS's delta under another inversion.
    let mut hidden = c.clone();
    hidden.inversion.algorithm = QsmAlgorithm::Tkd;
    assert!(!hidden.to_toml_changed().unwrap().contains("delta"));

    // The browser profile reads back the same way: its changes that apply, and nothing else.
    let mut browser = PipelineConfig::browser();
    browser.inversion.algorithm = QsmAlgorithm::Xqsm;
    let toml = browser.to_toml_changed().unwrap();
    assert!(toml.contains("xqsm") && toml.len() * 4 < browser.to_toml().unwrap().len(), "{toml}");
    assert_eq!(PipelineConfig::from_toml(&toml).unwrap().to_json().unwrap(), pruned(&browser));
}

#[test]
fn to_toml_selected_prunes_and_roundtrips() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Rts;
    c.inversion.rts.delta = 0.22; // non-default, must survive
    c.bg_removal.algorithm = BfAlgorithm::Vsharp;
    c.bg_removal.vsharp.threshold = 0.05; // non-default, must survive

    let toml = c.to_toml_selected().unwrap();

    // Only the selected algorithm tables remain.
    assert!(toml.contains("[inversion.rts]"), "selected inversion kept");
    assert!(!toml.contains("[inversion.tkd]"), "unselected inversion pruned");
    assert!(!toml.contains("[inversion.medi]"), "unselected inversion pruned");
    assert!(toml.contains("[bg_removal.vsharp]"), "selected bg kept");
    assert!(!toml.contains("[bg_removal.pdf]"), "unselected bg pruned");
    // Non-algorithm sections are untouched.
    assert!(toml.contains("[masking]"));

    // Round-trips like any qsmxt config: selected values preserved, omitted = defaults.
    let loaded = PipelineConfig::from_toml(&toml).unwrap();
    assert_eq!(loaded.inversion.algorithm, QsmAlgorithm::Rts);
    assert_eq!(loaded.inversion.rts.delta, 0.22);
    assert_eq!(loaded.bg_removal.vsharp.threshold, 0.05);
    // Pruned algorithms came back as their defaults.
    assert_eq!(loaded.inversion.tkd, super::TkdConfig::default());
    assert_eq!(loaded.bg_removal.pdf, super::PdfConfig::default());
}
