//! Wiring only: QSMxT's settings reach QSM.rs's rules, and the findings come back in QSMxT's
//! terms, applied and dismissed through the config. Which rules exist and what they say is
//! QSM.rs's (tested there).

use super::*;
use crate::schema::{self, ValueType, ViewFacts};

/// The facts a rule's conditions need: multi-echo, at most two echoes, a fine voxel size. `None`
/// for a rule that needs a browser (QSMxT's tests run natively).
fn facts_for(rule: &Rule) -> Option<Facts> {
    let mut f = Facts { echo_count: Some(4), field_strength: Some(3.0), voxel_size: Some((1.0, 1.0, 1.0)), dims: Some((64, 64, 32)) };
    for c in rule.when {
        let Condition::Fact { fact, is } = c else { continue };
        let Test::Range { min, max } = is else { return None };
        match fact {
            core::Fact::EchoCount => f.echo_count = Some(if max.is_some() { 2 } else { min.unwrap_or(2.0) as usize + 2 }),
            core::Fact::MinVoxelSize => f.voxel_size = Some((0.5, 0.5, 0.5)),
            core::Fact::FieldStrength => f.field_strength = Some(match (min, max) {
                (Some(m), _) => m + 0.5,
                (None, Some(m)) => m - 0.5,
                (None, None) => 3.0,
            }),
            _ => return None,
        }
    }
    Some(f)
}

/// Every value a QSM.rs setting can take in QSMxT, in QSM.rs's terms.
fn candidates(core_path: &str) -> Vec<Value> {
    let (path, kind) = setting_of(core_path).unwrap();
    let ids = |xs: &[&str]| xs.iter().map(|x| json!(x)).collect();
    match kind {
        Kind::MaskGenerator => ids(&["threshold", "bet", "hd-bet", "mindgrab"]),
        Kind::HdBetInference => ids(&["full-volume", "sliding-window", "low-memory"]),
        Kind::LaplacianSolver => ids(&["dct", "fft-12", "fft", "mirror"]),
        Kind::VsharpGrid => ids(&["native", "isotropic-finest", "isotropic-1mm"]),
        Kind::SynthSegInference => ids(&["full-volume", "low-memory"]),
        Kind::SynthSegPatch => vec![json!(96), json!(160)],
        Kind::Same => {
            let s = schema::setting(path).unwrap();
            match &s.value_type {
                ValueType::Bool => vec![json!(true), json!(false)],
                ValueType::Enum { .. } | ValueType::EchoTime { .. } => s.value_type.options().into_iter()
                    .map(|o| o.value.clone()).filter(|v| v.is_string()).collect(),
                // The named choices, and numbers.
                ValueType::NumberOrChoice { .. } => s.value_type.options().into_iter()
                    .map(|o| o.value.clone()).chain([json!(0.1), json!(0.5)]).collect(),
                ValueType::Int => vec![json!(3), json!(500)],
                ValueType::Float => vec![json!(0.5), json!(3), json!(20)],
                ValueType::IntVec3 => vec![json!([2, 2, 2]), json!([30, 30, 30])],
                ValueType::FloatVec3 => vec![json!([2, 2, 2]), json!([20, 20, 20])],
                other => panic!("{core_path}: no candidates for {other:?}"),
            }
        }
    }
}

fn passes(expect: &Test, v: &Value) -> bool {
    match expect {
        Test::OneOf { values } => values.iter().map(core_json).any(|x| x == *v),
        Test::NoneOf { values } => !values.iter().map(core_json).any(|x| x == *v),
        Test::Range { min, max } => v.as_f64().is_some_and(|x| min.is_none_or(|m| x >= m) && max.is_none_or(|m| x <= m)),
    }
}

/// A config that chooses the rule's trigger (and its other conditions) and departs from it.
fn violating(rule: &Rule) -> PipelineConfig {
    let mut c = PipelineConfig::default();
    for cond in rule.when {
        let Condition::Setting { setting, is } = cond else { continue };
        if setting.starts_with("separation.") {
            c.pipeline.do_chi_separation = true;
        }
        if setting.starts_with("segmentation.") {
            c.pipeline.do_segmentation = true;
        }
        // A background-removal method's own setting is in play only with that method.
        if let Some((method, _)) = setting.strip_prefix("bg_removal.").and_then(|r| r.split_once('.')) {
            set(&mut c, "bg_removal.algorithm", &json!(method)).unwrap();
        }
        let v = match is {
            Test::OneOf { values } => core_json(&values[0]),
            other => candidates(setting).into_iter().find(|v| passes(other, v)).unwrap(),
        };
        set(&mut c, setting, &v).unwrap_or_else(|e| panic!("{}: trigger {setting} = {v}: {e}", rule.id));
    }
    let bad = candidates(rule.setting).into_iter().find(|v| !passes(&rule.expect, v))
        .unwrap_or_else(|| panic!("{}: nothing departs from it", rule.id));
    set(&mut c, rule.setting, &bad).unwrap_or_else(|e| panic!("{}: {} = {bad}: {e}", rule.id, rule.setting));
    c
}

/// The rules QSMxT can check here: those that do not need a browser, less those about a setting
/// QSMxT does not offer in the rule's combination (it hides the setting, and the run ignores it).
fn native_rules() -> impl Iterator<Item = (&'static Rule, Facts)> {
    const NOT_OFFERED: &[&str] = &["field-mapping.bipolar-not-with-laplacian"];
    core::RULES.iter().filter(|r| !NOT_OFFERED.contains(&r.id)).filter_map(|r| facts_for(r).map(|f| (r, f)))
}

fn known() -> Facts {
    Facts { echo_count: Some(4), field_strength: Some(3.0), voxel_size: Some((1.0, 1.0, 1.0)), dims: Some((64, 64, 32)) }
}

/// A QSM.rs setting at a QSMxT setting's own path needs no entry in the table.
#[test]
fn a_rule_setting_at_a_qsmxt_path_maps_to_it() {
    assert_eq!(setting_of("separation.decompose.chi_bound"), Some(("separation.decompose.chi_bound", Kind::Same)));
    assert_eq!(setting_of("no.such.setting"), None);
}

#[test]
fn every_rule_setting_is_a_qsmxt_setting() {
    for (core_path, _) in core::SETTINGS {
        let path = qsmxt_path(core_path).unwrap_or_else(|| panic!("QSM.rs's rule setting {core_path} has no QSMxT setting"));
        assert!(schema::setting(path).is_some(), "{core_path} → {path}, which is not in the schema");
    }
    for (core_path, _, _) in SETTINGS {
        assert!(core::SETTINGS.iter().any(|(p, _)| p == core_path), "{core_path} is not a QSM.rs rule setting");
    }
}

/// iQFM reaches the rules as iHARPERELLA, which must stay a choice no rule names.
#[test]
fn the_iqfm_placeholder_is_named_by_no_rule() {
    let names = |t: &Test| match t {
        Test::OneOf { values } | Test::NoneOf { values } => values.contains(&core::Value::Choice("iharperella")),
        Test::Range { .. } => false,
    };
    for r in core::RULES {
        let in_when = r.when.iter().any(|c| matches!(c, Condition::Setting { is, .. } if names(is)));
        assert!(!in_when && !names(&r.expect), "{} names iHARPERELLA", r.id);
    }
}

/// QSMxT's defaults depart from no recommendation. Its default inversion, iLSQR, differs from the
/// STI Suite reference pipeline in the unwrapping and (multi-echo) the echo combination: those are
/// `reference` findings, information only.
#[test]
fn defaults_depart_only_from_reference_choices() {
    let mut c = PipelineConfig::default();
    c.pipeline.do_segmentation = true;
    let ids = |facts: &Facts| recommendations(&c, facts).into_iter().map(|r| {
        assert_eq!(r.strength, Strength::Reference, "{r:?}");
        r.rule
    }).collect::<Vec<_>>();
    assert_eq!(ids(&Facts::default()), ["sti.unwrap-laplacian"]);
    assert_eq!(ids(&known()), ["sti.unwrap-laplacian", "sti.echo-weights"]);
}

/// Each rule fires on a config that departs from it, in QSMxT's setting and terms, and applying
/// it resolves it.
#[test]
fn each_rule_fires_on_a_violation_and_applying_resolves_it() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for (rule, facts) in native_rules() {
        let mut c = violating(rule);
        let found = recommendations(&c, &facts);
        let Some(r) = found.iter().find(|r| r.rule == rule.id) else {
            failures.push(format!("{} does not fire on its violation: {:?}", rule.id, found.iter().map(|r| &r.rule).collect::<Vec<_>>()));
            continue;
        };
        assert_eq!(r.setting, qsmxt_path(rule.setting).unwrap(), "{}", rule.id);
        assert!(!r.current.is_empty() && !r.trigger.name.is_empty() && !r.recommended.name.is_empty(), "{r:?}");
        assert!(schema::get(&c, &r.trigger.setting).is_some(), "{}: trigger {} is not a setting", rule.id, r.trigger.setting);
        if let Err(e) = apply(&mut c, rule.id, None) {
            failures.push(format!("{}: {e}", rule.id));
            continue;
        }
        if recommendations(&c, &facts).iter().any(|r| r.rule == rule.id) {
            failures.push(format!("{} still fires after applying it", rule.id));
        }
        checked += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(checked > 75, "only {checked} rules checked");
}

fn view_facts(f: &Facts) -> ViewFacts {
    ViewFacts {
        num_echoes: f.echo_count, field_strength: f.field_strength,
        voxel_size: f.voxel_size.map(|(x, y, z)| [x, y, z]), dims: f.dims.map(|(x, y, z)| [x, y, z]),
        ..Default::default()
    }
}

/// A finding is on a setting the settings view shows, and is listed there; the always-on
/// reference values are there too.
#[test]
fn findings_reach_the_settings_view() {
    for (rule, facts) in native_rules() {
        let c = violating(rule);
        let view = schema::settings_view(&c, &view_facts(&facts));
        let shown = view.iter().find(|v| v.recommendations.iter().any(|r| r.rule == rule.id));
        let v = shown.unwrap_or_else(|| panic!("{} is not in the settings view", rule.id));
        assert!(v.visible && v.reference.iter().any(|r| r.rule == rule.id && r.departs), "{}", rule.id);
    }
    // At the defaults, a covered setting shows its reference value without a finding.
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Rts;
    let view = schema::settings_view(&c, &ViewFacts::default());
    let bg = view.iter().find(|v| v.path == "bg_removal.algorithm").unwrap();
    assert!(bg.recommendations.is_empty());
    let r = bg.reference.iter().find(|r| r.rule == "rts.bg-vsharp").expect("RTS's reference background removal is shown");
    assert!(!r.departs && r.recommended.name == "V-SHARP" && !r.source.is_empty(), "{r:?}");
}

/// A rule on the scan stays silent until the scan is known.
#[test]
fn scan_rules_wait_for_the_scan() {
    let rule = core::RULES.iter().find(|r| r.id == "medi-msmv.three-echoes").unwrap();
    let c = violating(rule);
    assert!(!recommendations(&c, &Facts::default()).iter().any(|r| r.rule == rule.id));
    let two = Facts { echo_count: Some(2), ..Default::default() };
    let r = recommendations(&c, &two).into_iter().find(|r| r.rule == rule.id).unwrap();
    assert_eq!((r.strength, r.needs_scan), (Strength::Required, true));
}

/// Without QSM, the reconstruction's rules are not in play.
#[test]
fn no_qsm_no_reconstruction_rules() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Rts;
    c.bg_removal.algorithm = BfAlgorithm::Pdf;
    assert!(!recommendations(&c, &Facts::default()).is_empty());
    c.pipeline.do_qsm = false;
    c.pipeline.do_swi = true;
    assert!(recommendations(&c, &Facts::default()).is_empty());
}

#[test]
fn reference_findings_are_information_only() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Rts;
    c.bg_removal.algorithm = BfAlgorithm::Pdf;
    let found = recommendations(&c, &Facts::default());
    let r = found.iter().find(|r| r.rule == "rts.bg-vsharp").unwrap();
    assert!(r.strength == Strength::Reference && !r.actionable());
    let before = c.clone();
    let (applied, _) = accept(&mut c, &found);
    assert!(applied.is_empty() && c == before, "--accept-recommendations leaves a reference departure");
    assert_eq!(r.note(), "Background removal used PDF; this differs from the RTS reference pipeline, which uses V-SHARP");
}

#[test]
fn accepting_applies_the_strongest_actionable_finding() {
    let rule = core::RULES.iter().find(|r| r.id == "medi-msmv.prefilter").unwrap();
    let mut c = violating(rule);
    let facts = Facts::default();
    let found = recommendations(&c, &facts);
    assert!(found.iter().any(|r| r.rule == rule.id && r.strength == Strength::Discouraged));
    let (applied, conflicting) = accept(&mut c, &found);
    assert!(applied.iter().any(|r| r.rule == rule.id) && conflicting.is_empty(), "{applied:?}");
    assert!(c.inversion.medi.msmv_prefilter);
    // Dismissed, it is left as it is.
    let mut c = violating(rule);
    dismiss(&mut c, rule.id, &facts).unwrap();
    let found = recommendations(&c, &facts);
    assert!(found.iter().any(|r| r.rule == rule.id && r.dismissed));
    let (applied, _) = accept(&mut c, &found);
    assert!(!applied.iter().any(|r| r.rule == rule.id) && !c.inversion.medi.msmv_prefilter);
}

/// A dismissal holds for the value it was made for; a change to the setting brings the finding
/// back, and pruning drops the stale dismissal. It survives the config file and changes no hash.
#[test]
fn dismissals_hold_for_their_value() {
    let facts = Facts::default();
    let rule = core::RULES.iter().find(|r| r.id == "medi-msmv.prefilter").unwrap();
    let mut c = violating(rule);
    assert!(dismiss(&mut c, "medi.bg-pdf", &facts).is_err(), "only a finding can be dismissed");
    let hash = crate::cache::config_hash(&c);
    dismiss(&mut c, rule.id, &facts).unwrap();
    assert_eq!(c.dismissed_recommendations.len(), 1);
    assert_eq!(c.dismissed_recommendations[0].rule, rule.id);
    assert_eq!(crate::cache::config_hash(&c), hash, "a dismissal changes nothing a run computes");
    let toml = c.to_toml().unwrap();
    assert!(toml.contains("[[dismissed_recommendations]]"), "{toml}");
    assert_eq!(PipelineConfig::from_toml(&toml).unwrap(), c);
    assert!(recommendations(&c, &facts).iter().any(|r| r.rule == rule.id && r.dismissed));

    let prefilter = c.inversion.medi.msmv_prefilter;
    c.inversion.medi.msmv_prefilter = !prefilter;
    prune_dismissals(&mut c, &facts);
    assert!(c.dismissed_recommendations.is_empty());
    c.inversion.medi.msmv_prefilter = prefilter;
    assert!(recommendations(&c, &facts).iter().any(|r| r.rule == rule.id && !r.dismissed), "changed and back: offered again");
    // A config without dismissals (any earlier one) loads.
    assert!(PipelineConfig::from_toml("schema_version = 2\n").unwrap().dismissed_recommendations.is_empty());
}

/// A reference-pipeline difference is information only: there is nothing to dismiss, "Keep mine"
/// records nothing, and a file that holds a dismissal of one still loads, without it and
/// without a word.
#[test]
fn a_reference_difference_has_no_dismissal() {
    let facts = Facts::default();
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Rts;
    c.bg_removal.algorithm = BfAlgorithm::Pdf;
    assert!(is_reference_rule("rts.bg-vsharp") && !is_reference_rule("medi-msmv.prefilter"));
    assert_eq!(dismiss(&mut c, "rts.bg-vsharp", &facts),
               Err("rts.bg-vsharp is a reference-pipeline note, not a recommendation; there is nothing to dismiss".into()));
    let found = recommendations(&c, &facts);
    keep(&mut c.dismissed_recommendations, &found);
    assert!(c.dismissed_recommendations.is_empty(), "{:?}", c.dismissed_recommendations);

    let file = format!("{}\n[[dismissed_recommendations]]\nrule = \"rts.bg-vsharp\"\nchosen = \"pdf\"\n", c.to_toml().unwrap());
    let (read, migration) = PipelineConfig::from_toml_noting(&file).unwrap();
    assert!(read.dismissed_recommendations.is_empty());
    assert!(migration.warnings.is_empty() && migration.notes.is_empty(), "{migration:?}");
    let json = r#"{"dismissed_recommendations": [{"rule": "rts.bg-vsharp", "chosen": "pdf"}, {"rule": "medi-msmv.prefilter", "chosen": "true"}]}"#;
    let (read, _) = PipelineConfig::from_json_migrating(json).unwrap();
    assert_eq!(read.dismissed_recommendations, vec![Dismissal { rule: "medi-msmv.prefilter".into(), chosen: "true".into() }]);
    // One put in by hand is dropped by pruning.
    c.dismissed_recommendations.push(Dismissal { rule: "rts.bg-vsharp".into(), chosen: "pdf".into() });
    assert!(!recommendations(&c, &facts).iter().any(|r| r.dismissed));
    prune_dismissals(&mut c, &facts);
    assert!(c.dismissed_recommendations.is_empty());
}

/// Two triggers can want different things of one setting: both are reported.
#[test]
fn findings_of_two_triggers_on_one_setting_are_both_reported() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Ilsqr;
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
    let found = recommendations(&c, &Facts::default());
    // iLSQR's reference unwraps with the Laplacian, χ-separation's with ROMEO (the default).
    assert!(found.iter().any(|r| r.rule == "sti.unwrap-laplacian"));
    c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
    let found = recommendations(&c, &Facts::default());
    assert!(found.iter().any(|r| r.rule == "chi-sep.unwrap-romeo"));
    assert!(!found.iter().any(|r| r.rule == "sti.unwrap-laplacian"));
}

#[test]
fn the_note_says_what_was_done() {
    let rule = core::RULES.iter().find(|r| r.id == "fansi.te-spacing").unwrap();
    let c = violating(rule);
    let facts = facts_for(rule).unwrap();
    let counts = |outcome: Option<Outcome>, runs: usize| recorded(&c, &facts).into_iter()
        .map(|r| crate::record::RecommendationCount { recorded: Recorded { outcome: outcome.unwrap_or(r.outcome), ..r }, runs })
        .collect::<Vec<_>>();
    let n = note(&counts(None, 1), 1);
    let line = n.departures.iter().find(|l| l.contains("recommends")).expect("a recommendation departed from");
    assert!(line.starts_with("Echo time") && line.ends_with('.'), "{line}");
    // What was done and what the method recommends; why, and where that is recorded, is for the log.
    let finding = recorded(&c, &facts).into_iter().find(|r| r.finding.rule == rule.id).unwrap().finding;
    assert!(!line.contains(rule.source) && !line.contains(&finding.reason), "{line}");
    assert!(finding.line().contains(rule.source) && finding.line().contains(&finding.reason));
    assert!(n.differences.iter().all(|l| l.contains("differs from the")), "{:?}", n.differences);
    // In some of a dataset's runs.
    assert!(note(&counts(None, 2), 5).departures[0].ends_with("; in 2 of 5 runs."));
    // Applied findings are not departures.
    assert!(note(&counts(Some(Outcome::Applied), 1), 1).is_empty());
}

/// MEDI with V-SHARP and a threshold mask, its mask chosen by the user.
fn medi_vsharp_threshold() -> PipelineConfig {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Medi;
    set(&mut c, "bg_removal.algorithm", &json!("vsharp")).unwrap();
    set(&mut c, "masking.generator", &json!("threshold")).unwrap();
    c
}

fn method(c: &PipelineConfig, name: &str) -> Trigger {
    chosen_methods(c, &Facts::default()).into_iter().find(|m| m.name == name)
        .unwrap_or_else(|| panic!("{name} is not chosen"))
}

/// A method's reference pipeline: its reference rules grouped under it, the differences listed,
/// applied all at once, or kept (which records nothing).
#[test]
fn reference_differences_of_a_chosen_method() {
    let facts = Facts::default();
    let c = medi_vsharp_threshold();
    let medi = method(&c, "MEDI");
    assert_eq!(medi.setting, "inversion.algorithm");
    assert_eq!(reference_title(&medi), "MEDI's reference pipeline");
    let diffs = reference_differences(&c, &facts, &medi);
    let rules: Vec<&str> = diffs.iter().map(|r| r.rule.as_str()).collect();
    assert!(rules.contains(&"medi.bg-pdf") && rules.contains(&"medi.mask-bet"), "{rules:?}");
    assert!(diffs.iter().all(|r| r.strength == Strength::Reference && r.trigger == medi));
    let groups = by_method(&reference_values(&c, &facts));
    assert!(groups.iter().all(|m| m.rules.iter().all(|r| r.trigger == m.method)));

    let mut applied = c.clone();
    apply_all(&mut applied, &diffs).unwrap();
    assert!(reference_differences(&applied, &facts, &medi).is_empty());
    assert_eq!(applied.bg_removal.algorithm, BfAlgorithm::Pdf);

    // Kept, nothing changes and nothing is recorded: a reference difference is information only.
    let mut kept = c.clone();
    keep(&mut kept.dismissed_recommendations, &diffs);
    assert_eq!(kept, c, "keeping changes no setting and records no dismissal");
    // Another method chosen, the differences no longer apply; choosing MEDI again offers them.
    kept.inversion.algorithm = QsmAlgorithm::Rts;
    assert!(!chosen_methods(&kept, &facts).contains(&medi));
    kept.inversion.algorithm = QsmAlgorithm::Medi;
    assert_eq!(reference_differences(&kept, &facts, &medi).len(), diffs.len());
}

/// With Laplacian unwrapping, QSMART's and iLSQR's reference pipelines offer the unmasked
/// unwrapping, in QSMxT's setting, and applying it sets only that.
#[test]
fn the_reference_pipelines_offer_the_unmasked_unwrapping() {
    let facts = Facts::default();
    for (alg, name, rule) in [(QsmAlgorithm::Qsmart, "QSMART", "qsmart.unwrap-unmasked"),
                              (QsmAlgorithm::Ilsqr, "iLSQR", "sti.unwrap-unmasked")] {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = alg;
        let unmasked = |c: &PipelineConfig| reference_values(c, &facts).into_iter().find(|r| r.rule == rule);
        assert!(unmasked(&c).is_none(), "{rule}: ROMEO has no Laplacian unwrapping");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let diffs = reference_differences(&c, &facts, &method(&c, name));
        let r = diffs.iter().find(|r| r.rule == rule).unwrap_or_else(|| panic!("{rule} not offered: {diffs:?}"));
        assert_eq!((r.setting.as_str(), &r.recommended.value), ("field_mapping.laplacian_unmasked", &json!(true)));
        let mut applied = c.clone();
        apply(&mut applied, rule, None).unwrap();
        assert!(applied.field_mapping.laplacian_unmasked);
        let mut back = applied.clone();
        back.field_mapping.laplacian_unmasked = false;
        assert_eq!(back, c, "{rule}: nothing else changes");
        assert!(!unmasked(&applied).unwrap().departs);
    }
}

/// A setting's marker names whose rule it departs from and what that rule uses.
#[test]
fn markers_name_the_method() {
    let facts = Facts::default();
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Qsmart;
    set(&mut c, "masking.generator", &json!("threshold")).unwrap();
    let view = reference_values(&c, &facts);
    let m = marker(&view, "masking.sections").unwrap();
    assert_eq!(m, Marker { actionable: false, text: "QSMART: BET".into() });
    // Two methods whose references agree share the value.
    let mut c = medi_vsharp_threshold();
    set(&mut c, "bg_removal.algorithm", &json!("pdf")).unwrap();
    let m = marker(&reference_values(&c, &facts), "masking.sections").unwrap();
    assert_eq!(m.text, "MEDI, PDF: BET");
    // An offered finding is flagged and named first.
    let rule = core::RULES.iter().find(|r| r.id == "medi-msmv.prefilter").unwrap();
    let c = violating(rule);
    let m = marker(&reference_values(&c, &facts), "inversion.medi.msmv_prefilter").unwrap();
    assert!(m.actionable && m.text.starts_with("MEDI: "), "{m:?}");
    // The settings view carries it.
    let view = schema::settings_view(&c, &view_facts(&facts));
    assert_eq!(view.iter().find(|v| v.path == "inversion.medi.msmv_prefilter").unwrap().marker.as_ref(), Some(&m));
    assert!(marker(&reference_values(&PipelineConfig::default(), &facts), "inversion.algorithm").is_none());
}

/// The settings whose choice can open a reference pipeline pop-up come from the rules.
#[test]
fn reference_triggers_are_the_method_settings() {
    let t = reference_trigger_settings();
    for path in ["inversion.algorithm", "bg_removal.algorithm", "separation.algorithm"] {
        assert!(t.contains(&path), "{path} not in {t:?}");
    }
    assert!(!t.contains(&"bg_removal.vsharp.max_radius"));
}
