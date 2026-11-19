use super::*;

// --- Recommendations (QSM.rs's pipeline rules) ---

/// TKD on multi-echo data without phase offset removal: ROMEO's MCPC-3D-S is recommended for
/// a weighted-average B0 from multi-echo data (`field-mapping.phase-offsets`).
fn departing_args(bids: PathBuf, out: PathBuf) -> RunArgs {
    let mut args = default_run_args(bids, out);
    args.pipeline.add(&["--qsm-algorithm", "tkd"]);
    args.pipeline.add(&["--phase-offset-removal", "false"]);
    args.pipeline.add(&["--masking-input", "magnitude-first"]);
    args.no_mem_limit = true;
    args
}

#[test]
fn dry_lists_the_findings_and_what_accepting_would_change() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    testutils::create_multi_echo_bids(&bids);
    let args = departing_args(bids.clone(), dir.path().join("out"));
    let config = super::run::build_config(None, &args.pipeline).unwrap();
    let runs = crate::bids::discovery::discover_runs(&bids, &Default::default()).unwrap();
    let recommendations = qsmxt_engine::recommendations::Accept { recommendations: true, reference: false };
    let text = super::recommend::dry_text(&config, &runs, recommendations);
    // The log warns of it, saying how to apply it.
    let logged = super::recommend::found_lines(&config, &runs);
    let line = logged.iter().find(|(_, l)| l.contains("(field-mapping.phase-offsets;")).unwrap_or_else(|| panic!("{logged:?}"));
    assert!(line.0 && line.1.ends_with("; --accept-recommendations applies it)") && !line.1.contains("--dismiss"), "{line:?}");
    // Where the rule comes from is in the log only, not in --dry.
    let source = qsm_core::pipeline::rules::RULES.iter().find(|r| r.id == "field-mapping.phase-offsets").unwrap().source;
    assert!(line.1.contains(source) && !text.contains(source), "{text}");
    let at = text.find("  recommended (--accept-recommendations applies them):").unwrap_or_else(|| panic!("{text}"));
    assert!(text[at..].contains("field-mapping.phase-offsets"), "{text}");
    assert!(text.contains("keep the choice with --dismiss-recommendation field-mapping.phase-offsets=false"), "{text}");
    assert!(text.contains("With --accept-recommendations, the run would use:")
        && text.contains("Phase offset removal: off → on (recommended; field-mapping.phase-offsets)"), "{text}");
    // Dismissed, it is listed as such and nothing would change.
    let mut dismissed = departing_args(bids.clone(), dir.path().join("out")).pipeline;
    dismissed.add(&["--dismiss-recommendation", "field-mapping.phase-offsets=false"]);
    let config = super::run::build_config(None, &dismissed).unwrap();
    let text = super::recommend::dry_text(&config, &runs, recommendations);
    // Dismissed, it is still a warning in the log, marked as kept.
    let logged = super::recommend::found_lines(&config, &runs);
    assert!(logged.iter().any(|(warn, l)| *warn && l.contains("(field-mapping.phase-offsets; dismissed, kept)")), "{logged:?}");
    assert!(text.contains("  dismissed (kept):") && text.contains("--accept-recommendations would change nothing."), "{text}");
    assert!(qsmxt_engine::generate_command(&config).contains("--dismiss-recommendation field-mapping.phase-offsets=false"));
    // An unknown rule is refused, and so is a reference-pipeline rule, which has nothing to
    // dismiss.
    dismissed.add(&["--dismiss-recommendation", "no.such-rule=1"]);
    assert!(super::run::build_config(None, &dismissed).is_err());
    dismissed.add(&["--dismiss-recommendation", "sti.unwrap-laplacian=romeo"]);
    let e = super::run::build_config(None, &dismissed).unwrap_err().to_string();
    assert!(e.contains("sti.unwrap-laplacian is a reference-pipeline note, not a recommendation; there is nothing to dismiss"), "{e}");
}

/// The original authors' settings are said apart from recommendations: information in the
/// log, their own group in `--dry`, and applied by `--accept-reference-settings` only, not by
/// `--accept-recommendations`. With the complex fit, MEDI's region-growing unwrapper is the
/// authors' setting; ROMEO stays.
#[test]
fn authors_settings_are_applied_only_when_asked() {
    use qsmxt_engine::recommendations::Accept;
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    testutils::create_multi_echo_bids(&bids);
    let mut args = departing_args(bids.clone(), dir.path().join("out"));
    args.pipeline.given.retain(|g| g.flag.long != "phase-offset-removal");
    args.pipeline.add(&["--b0-estimation", "complex-fit"]);
    let config = super::run::build_config(None, &args.pipeline).unwrap();
    let runs = crate::bids::discovery::discover_runs(&bids, &Default::default()).unwrap();
    let logged = super::recommend::found_lines(&config, &runs);
    let line = logged.iter().find(|(_, l)| l.contains("(complex-fit.unwrap-region-growing;")).unwrap_or_else(|| panic!("{logged:?}"));
    assert!(!line.0 && line.1.starts_with("Differs from the original authors' setting")
        && line.1.ends_with("; --accept-reference-settings applies it)"), "{line:?}");
    let both = Accept { recommendations: true, reference: true };
    let none = super::recommend::dry_text(&config, &runs, Accept::default());
    let at = none.find("  original authors' settings (not recommendations; --accept-reference-settings applies them):")
        .unwrap_or_else(|| panic!("{none}"));
    assert!(none[at..].contains("complex-fit.unwrap-region-growing"), "{none}");
    let recommended = super::recommend::dry_text(&config, &runs, Accept { recommendations: true, reference: false });
    assert!(!recommended.contains("; complex-fit.unwrap-region-growing)"), "{recommended}");
    let authors = super::recommend::dry_text(&config, &runs, both);
    assert!(authors.contains("With --accept-recommendations --accept-reference-settings, the run would use:")
        && authors.contains("(original authors' setting; complex-fit.unwrap-region-growing)"), "{authors}");
    let mut c = config.clone();
    let accepted = super::recommend::accept(&mut c, &runs, Accept { recommendations: false, reference: true });
    assert_eq!(c.field_mapping.unwrapping_algorithm, qsmxt_engine::enums::UnwrappingAlgorithm::MediRegionGrowing);
    assert!(accepted.applied.iter().all(|r| r.kind == qsmxt_engine::recommendations::Kind::Reference), "{:?}", accepted.applied);
    let mut c = config.clone();
    super::recommend::accept(&mut c, &runs, Accept { recommendations: true, reference: false });
    assert_eq!(c.field_mapping.unwrapping_algorithm, config.field_mapping.unwrapping_algorithm, "ROMEO stays");
}

/// The fastest tiny pipeline, with a recommendation accepted: the run takes the recommended
/// value, its record keeps the finding as applied, the methods have no Note for it, and the
/// end-of-run summary says so. Without accepting, the Note says what was done.
#[test]
fn accepted_recommendations_reach_the_record_and_summary() {
    let dir = tempfile::tempdir().unwrap();
    let bids = dir.path().join("bids");
    testutils::create_multi_echo_bids(&bids);
    let runs = crate::bids::discovery::discover_runs(&bids, &Default::default()).unwrap();

    let out = dir.path().join("accepted");
    let mut args = departing_args(bids.clone(), out.clone());
    args.dry = false;
    args.accept_recommendations = true;
    super::run::execute(args).unwrap();
    let deriv = out.join("derivatives/qsmxt");
    let config = qsmxt_engine::PipelineConfig::from_toml(&std::fs::read_to_string(deriv.join("pipeline_config.toml")).unwrap()).unwrap();
    assert!(config.field_mapping.phase_offset_removal, "the run took the recommended value");
    let records = super::run::current_run_records(&deriv, &config);
    let (_, file) = records.first().expect("a run record");
    let applied = file.record.recommendations.iter().find(|r| r.finding.rule == "field-mapping.phase-offsets").unwrap();
    assert_eq!(applied.outcome, qsmxt_engine::recommendations::Outcome::Applied);
    let methods = std::fs::read_to_string(deriv.join("methods.md")).unwrap();
    assert!(!methods.contains("Departures from the methods' recommendations"), "{methods}");
    let accepted = super::recommend::Accepted { applied: vec![applied.finding.clone()], ..Default::default() };
    let summary = super::recommend::summary(&config, &runs, &accepted);
    assert!(summary.iter().any(|l| l.starts_with("  applied (recommended): Phase offset removal: on (was off)")), "{summary:?}");

    let out = dir.path().join("kept");
    let mut args = departing_args(bids, out.clone());
    args.dry = false;
    super::run::execute(args).unwrap();
    let methods = std::fs::read_to_string(out.join("derivatives/qsmxt/methods.md")).unwrap();
    let note = &methods[methods.find("## Note").unwrap_or_else(|| panic!("{methods}"))..];
    assert!(note.contains("Departures from the methods' recommendations:")
        && note.contains("- Phase offset removal was off; Weighted average recommends turning it on.\n"), "{note}");
    assert!(methods.find("## Note").unwrap() < methods.find("## References").unwrap(), "the Note is before the references");
}
