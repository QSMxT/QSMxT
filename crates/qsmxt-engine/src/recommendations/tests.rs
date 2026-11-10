//! Wiring only: QSMxT's settings reach QSM.rs's rules, and the findings come back in QSMxT's
//! terms, applied and dismissed through the config. Which rules exist and what they say is
//! QSM.rs's (tested there).

use super::*;
use crate::schema::{self, ValueType, ViewFacts};

/// The facts a rule's conditions need: multi-echo, at most two echoes, a fine voxel size. `None`
/// for a rule that needs a browser (QSMxT's tests run natively).
fn facts_for(rule: &Rule) -> Option<Facts> {
    let mut f = Facts {
        echo_count: Some(4), field_strength: Some(3.0), voxel_size: Some((1.0, 1.0, 1.0)), dims: Some((64, 64, 32)),
        manufacturer: None, measured_r2prime: None,
    };
    for c in rule.when {
        let Condition::Fact { fact, is } = c else { continue };
        // A vendor rule: the first manufacturer it names, as a sidecar spells it, or one it does
        // not name.
        if let (core::Fact::Manufacturer, Test::OneOf { values }) = (fact, is) {
            let Some(core::Value::Choice(m)) = values.first() else { return None };
            f.manufacturer = Some(m.to_uppercase());
            continue;
        }
        if let (core::Fact::Manufacturer, Test::NoneOf { values }) = (fact, is) {
            let m = ["siemens", "ge", "philips", "canon"].into_iter()
                .find(|m| !values.contains(&core::Value::Choice(m)))?;
            f.manufacturer = Some(m.to_uppercase());
            continue;
        }
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
        Mapping::MaskGenerator => ids(&["threshold", "bet", "hd-bet", "mindgrab"]),
        Mapping::HdBetInference => ids(&["full-volume", "sliding-window", "low-memory"]),
        Mapping::LaplacianSolver => ids(&["dct", "fft-12", "fft", "mirror"]),
        Mapping::VsharpGrid => ids(&["native", "isotropic-finest", "isotropic-1mm"]),
        Mapping::SynthSegInference => ids(&["full-volume", "low-memory"]),
        Mapping::SynthSegPatch => vec![json!(96), json!(160)],
        Mapping::MaskInput => ids(&["magnitude-first", "magnitude", "magnitude-last", "phase-quality"]),
        Mapping::Inhomogeneity => ids(&["none", "makehomogeneous", "n4"]),
        Mapping::BetFraction => vec![json!(0.2), json!(0.5)],
        Mapping::Erosion => vec![json!(0), json!(2)],
        Mapping::MinIntensity => vec![json!(0), json!(100)],
        Mapping::Majority => vec![json!(true), json!(false)],
        Mapping::CloseRadius => vec![json!(0), json!(2)],
        Mapping::QsmartFitThreshold => ids(&["absolute-40", "percentile-99"]),
        Mapping::Same => {
            let s = schema::setting(path).unwrap();
            match &s.value_type {
                ValueType::Bool => vec![json!(true), json!(false)],
                ValueType::Enum { .. } | ValueType::EchoTime { .. } => s.value_type.options().into_iter()
                    .map(|o| o.value.clone()).filter(|v| v.is_string()).collect(),
                // The named choices, and numbers.
                ValueType::NumberOrChoice { .. } => s.value_type.options().into_iter()
                    .map(|o| o.value.clone()).chain([json!(0.1), json!(0.5)]).collect(),
                ValueType::Int => vec![json!(3), json!(500)],
                // 0.1 last: HEIDI's default cone threshold, which its truncation rules condition on.
                ValueType::Float => vec![json!(0.5), json!(3), json!(20), json!(0.1)],
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
    // BET's fraction and orientation are in play only with a BET step.
    if rule.setting.starts_with("masking.bet.") {
        set(&mut c, "masking.generator", &json!("bet")).unwrap();
    }
    // N4's geometry is in play only with N4.
    if rule.setting == "masking.n4_geometry" {
        set(&mut c, "masking.inhomogeneity_correction", &json!("n4")).unwrap();
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
    Facts { echo_count: Some(4), field_strength: Some(3.0), voxel_size: Some((1.0, 1.0, 1.0)), dims: Some((64, 64, 32)), manufacturer: None, measured_r2prime: None }
}

/// A QSM.rs setting at a QSMxT setting's own path needs no entry in the table.
#[test]
fn a_rule_setting_at_a_qsmxt_path_maps_to_it() {
    assert_eq!(setting_of("separation.decompose.chi_bound"), Some(("separation.decompose.chi_bound", Mapping::Same)));
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

/// iQFM reaches the rules as HARPERELLA, which must stay a choice no rule names.
#[test]
fn the_iqfm_placeholder_is_named_by_no_rule() {
    let names = |t: &Test| match t {
        Test::OneOf { values } | Test::NoneOf { values } => values.contains(&core::Value::Choice("harperella")),
        Test::Range { .. } => false,
    };
    for r in core::RULES {
        let in_when = r.when.iter().any(|c| matches!(c, Condition::Setting { is, .. } if names(is)));
        assert!(!in_when && !names(&r.expect), "{} names HARPERELLA", r.id);
    }
}

/// QSMxT's defaults depart from no recommendation. Its default inversion, iLSQR, differs from the
/// STI Suite reference pipeline (its GUI) in the unwrapping, the iteration count, the precision and
/// (multi-echo) the echo combination: those are `reference` findings, information only, and the
/// methods Note leaves them out while the config follows QSMxT's defaults.
#[test]
fn defaults_depart_only_from_reference_choices() {
    let mut c = PipelineConfig::default();
    c.pipeline.do_segmentation = true;
    let ids = |facts: &Facts| recommendations(&c, facts).into_iter().map(|r| {
        assert_eq!(r.strength, Strength::Reference, "{r:?}");
        r.rule
    }).collect::<Vec<_>>();
    assert_eq!(ids(&Facts::default()), ["sti.unwrap-laplacian", "sti.ilsqr-iterations", "sti.ilsqr-single"]);
    assert_eq!(ids(&known()), ["sti.unwrap-laplacian", "sti.echo-weights", "sti.ilsqr-iterations", "sti.ilsqr-single"]);
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
        manufacturer: f.manufacturer.clone(), ..Default::default()
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

/// An earlier file's LBV tolerance (the old solver's 1e-6) loads as written, and the rule
/// `lbv.tolerance` notes LBV.m's 0.01: the setting is never rewritten.
#[test]
fn an_earlier_lbv_tolerance_is_kept_and_noted() {
    let (c, _) = PipelineConfig::from_toml_noting("[bg_removal]\nalgorithm = \"lbv\"\n[bg_removal.lbv]\ntol = 0.000001\n").unwrap();
    assert_eq!(c.bg_removal.lbv.tol, 1e-6);
    let found = recommendations(&c, &Facts::default());
    let r = found.iter().find(|r| r.rule == "lbv.tolerance").expect("lbv.tolerance noted");
    assert_eq!(r.strength, Strength::Reference);
    let mut at_reference = c.clone();
    at_reference.bg_removal.lbv.tol = 0.01;
    assert!(!recommendations(&at_reference, &Facts::default()).iter().any(|r| r.rule == "lbv.tolerance"));
}

#[test]
fn reference_findings_are_information_only() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Rts;
    c.bg_removal.algorithm = BfAlgorithm::Pdf;
    // At the suggested iteration count, so the only finding left is the reference difference.
    c.bg_removal.pdf.n_cg = 300;
    let found = recommendations(&c, &Facts::default());
    let r = found.iter().find(|r| r.rule == "rts.bg-vsharp").unwrap();
    assert!(r.strength == Strength::Reference && !r.actionable());
    let before = c.clone();
    let (applied, _) = accept(&mut c, &found, Accept { recommendations: true, reference: false });
    assert!(applied.is_empty() && c == before, "--accept-recommendations leaves a reference departure");
    assert_eq!(r.note(), "Background removal used PDF; this differs from the RTS reference pipeline, which uses V-SHARP");
    // --accept-reference-settings applies it.
    assert_eq!((r.kind, r.kind.label()), (Kind::Reference, "Original authors' setting"));
    let (applied, _) = accept(&mut c, &found, Accept { recommendations: false, reference: true });
    assert!(applied.iter().any(|a| a.rule == "rts.bg-vsharp") && c.bg_removal.algorithm == BfAlgorithm::Vsharp, "{applied:?}");
}

/// A suggestion (QSM.rs's `Strength::Suggested`: NDI's in-mask normaliser, WH-QSM's in-mask flat
/// weight) is offered with its reason, but `--accept-recommendations` leaves it, it can be kept,
/// and it is not a departure in the methods Note. iHARPERELLA's whole rim is a plain recommendation.
#[test]
fn suggestions_are_offered_but_not_applied() {
    for (alg, rule) in [(QsmAlgorithm::Ndi, "ndi.weight-norm-in-vivo"), (QsmAlgorithm::Whqsm, "whqsm.flat-in-mask-in-vivo")] {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = alg;
        let found = recommendations(&c, &Facts::default());
        let r = found.iter().find(|r| r.rule == rule).unwrap_or_else(|| panic!("{rule}: {found:?}"));
        assert!(r.strength == Strength::Suggested && r.kind == Kind::Reasoned && r.actionable(), "{rule}");
        assert!(!r.strength.preselected() && r.strength.dismissable(), "{rule}");
        let before = c.clone();
        let (applied, _) = accept(&mut c, &found, Accept { recommendations: true, reference: true });
        assert!(!applied.iter().any(|a| a.rule == rule) && c.inversion.ndi == before.inversion.ndi
            && c.inversion.whqsm == before.inversion.whqsm, "{rule}: {applied:?}");
        assert!(crate::recommendations::config_note(&c).departures.iter().all(|l| !l.contains("normalis") && !l.contains("weight")),
            "{rule}: {:?}", crate::recommendations::config_note(&c));
        apply(&mut c, rule, None).unwrap();
        assert!(!recommendations(&c, &Facts::default()).iter().any(|r| r.rule == rule && r.departs), "{rule}");
    }
    let mut c = PipelineConfig::default();
    c.bg_removal.algorithm = BfAlgorithm::Iharperella;
    let found = recommendations(&c, &Facts::default());
    let r = found.iter().find(|r| r.rule == "iharperella.whole-rim").unwrap();
    assert!(r.strength == Strength::Recommended && r.strength.preselected());
    accept(&mut c, &found, Accept { recommendations: true, reference: false });
    assert_eq!(c.bg_removal.iharperella.rim, HarperellaRim::Whole);
}

/// Each finding carries its kind, from QSM.rs: the authors' setting for a `reference` rule,
/// reasoned for every other; and the JSON a front end reads says so.
#[test]
fn findings_carry_their_kind() {
    for rule in core::RULES {
        let Some(facts) = facts_for(rule) else { continue };
        let c = violating(rule);
        let Some(r) = recommendations(&c, &facts).into_iter().find(|r| r.rule == rule.id) else { continue };
        assert_eq!(r.kind, Kind::from(rule.kind()), "{}", rule.id);
        assert_eq!(r.kind == Kind::Reference, r.strength == Strength::Reference, "{}", rule.id);
        assert_eq!(r.kind == Kind::Reasoned, r.actionable(), "{}: only reasoned findings are preselected", rule.id);
        assert_eq!(serde_json::to_value(&r).unwrap()["kind"], serde_json::to_value(r.kind).unwrap());
    }
    assert_eq!(Kind::Reasoned.label(), "Recommended");
}

/// A reasoned finding outranks an authors' setting on the same setting when both are accepted,
/// and the complex fit's unwrapper (MEDI's region growing) is applied only as an authors' setting.
#[test]
fn accepting_the_authors_settings_keeps_romeo_unless_asked() {
    let mut c = PipelineConfig::default();
    schema::set(&mut c, "field_mapping.b0_estimation", json!("complex-fit")).unwrap();
    // TKD has no unwrapper of its own (QSMxT's default iLSQR has STI Suite's Laplacian, which
    // would disagree with MEDI's region growing, and so neither would be applied).
    schema::set(&mut c, "inversion.algorithm", json!("tkd")).unwrap();
    let found = recommendations(&c, &known());
    let r = found.iter().find(|r| r.rule == "complex-fit.unwrap-region-growing").expect("the authors' unwrapper is shown");
    assert!(r.kind == Kind::Reference && !r.actionable());
    let mut kept = c.clone();
    accept(&mut kept, &found, Accept { recommendations: true, reference: false });
    assert_eq!(kept.field_mapping.unwrapping_algorithm, UnwrappingAlgorithm::Romeo, "ROMEO stays");
    accept(&mut c, &found, Accept { recommendations: true, reference: true });
    assert_eq!(c.field_mapping.unwrapping_algorithm, UnwrappingAlgorithm::MediRegionGrowing);
}

#[test]
fn accepting_applies_the_strongest_actionable_finding() {
    let rule = core::RULES.iter().find(|r| r.id == "medi-msmv.prefilter").unwrap();
    let mut c = violating(rule);
    let facts = Facts::default();
    let found = recommendations(&c, &facts);
    assert!(found.iter().any(|r| r.rule == rule.id && r.strength == Strength::Discouraged));
    let (applied, conflicting) = accept(&mut c, &found, Accept { recommendations: true, reference: false });
    assert!(applied.iter().any(|r| r.rule == rule.id) && conflicting.is_empty(), "{applied:?}");
    assert!(c.inversion.medi.msmv_prefilter);
    // Dismissed, it is left as it is.
    let mut c = violating(rule);
    dismiss(&mut c, rule.id, &facts).unwrap();
    let found = recommendations(&c, &facts);
    assert!(found.iter().any(|r| r.rule == rule.id && r.dismissed));
    let (applied, _) = accept(&mut c, &found, Accept { recommendations: true, reference: false });
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
    c.bg_removal.pdf.n_cg = 300; // no pdf.cg-iterations suggestion to keep
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
    c.inversion.algorithm = QsmAlgorithm::Medi;
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::Decompose;
    let found = recommendations(&c, &Facts::default());
    // MEDI's pipeline references nothing after the inversion, DECOMPOSE's the mean (the default).
    assert!(found.iter().any(|r| r.rule == "medi.reference-none"));
    assert!(!found.iter().any(|r| r.rule == "decompose.reference-mean"));
    c.qsm.reference = crate::enums::QsmReference::None;
    let found = recommendations(&c, &Facts::default());
    assert!(found.iter().any(|r| r.rule == "decompose.reference-mean"));
    assert!(!found.iter().any(|r| r.rule == "medi.reference-none"));
}

/// The χ-separation toolbox runs STI Suite's iLSQR but unwraps with ROMEO: STI Suite's
/// unwrapping rule does not apply under it, so only the toolbox's is reported.
#[test]
fn sti_suite_unwrapping_does_not_apply_under_the_chi_separation_toolbox() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Ilsqr;
    assert!(recommendations(&c, &Facts::default()).iter().any(|r| r.rule == "sti.unwrap-laplacian"));
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
    assert!(!recommendations(&c, &Facts::default()).iter().any(|r| r.rule.starts_with("sti.")));
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
    // BET's orientation is in play only once there is a BET step, which applying `medi.mask-bet`
    // put in: it is the one difference left, and applying it leaves none.
    let left = reference_differences(&applied, &facts, &medi);
    assert_eq!(left.iter().map(|r| r.rule.as_str()).collect::<Vec<_>>(), ["medi.bet-as-stored"]);
    apply_all(&mut applied, &left).unwrap();
    assert!(reference_differences(&applied, &facts, &medi).is_empty());
    assert_eq!(applied.bet.orientation, crate::enums::BetOrientation::AsStored);
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

/// MEDI with χ-separation by χ-sep, and LBV chosen: MEDI's authors use PDF, χ-sep's V-SHARP.
fn two_authors_disagree() -> (PipelineConfig, Vec<Recommendation>) {
    let mut c = PipelineConfig::default();
    c.pipeline.do_chi_separation = true;
    schema::set(&mut c, "inversion.algorithm", json!("medi")).unwrap();
    schema::set(&mut c, "separation.algorithm", json!("chi-sep-ilsqr")).unwrap();
    schema::set(&mut c, "bg_removal.algorithm", json!("lbv")).unwrap();
    let found = recommendations(&c, &known());
    let on_bg: Vec<&str> = found.iter().filter(|r| r.setting == "bg_removal.algorithm").map(|r| r.rule.as_str()).collect();
    assert!(on_bg.contains(&"medi.bg-pdf") && on_bg.contains(&"chi-sep.bg-vsharp"), "{on_bg:?}");
    (c, found)
}

/// FANSI with Laplacian unwrapping and the complex fit on several echoes: FANSI's measured
/// suggestion (discouraged, ROMEO) and the complex fit's authors' unwrapper want different values.
fn reasoned_and_authors_disagree() -> (PipelineConfig, Vec<Recommendation>) {
    let mut c = PipelineConfig::default();
    schema::set(&mut c, "inversion.algorithm", json!("fansi")).unwrap();
    schema::set(&mut c, "field_mapping.b0_estimation", json!("complex-fit")).unwrap();
    schema::set(&mut c, "field_mapping.unwrapping_algorithm", json!("laplacian")).unwrap();
    let found: Vec<Recommendation> = recommendations(&c, &known());
    let rules: Vec<&str> = found.iter().map(|r| r.rule.as_str()).collect();
    assert!(rules.contains(&"fansi.no-laplacian-multi-echo") && rules.contains(&"complex-fit.unwrap-region-growing"), "{rules:?}");
    (c, found)
}

/// A reasoned finding beats an authors' setting on one setting, in either order, and the setting
/// is not overwritten by whichever came last.
#[test]
fn apply_all_lets_a_reasoned_finding_beat_an_authors_setting() {
    let (c, found) = reasoned_and_authors_disagree();
    let mut backwards = found.clone();
    backwards.reverse();
    for findings in [found, backwards] {
        let mut applied = c.clone();
        let settled = apply_all(&mut applied, &findings).unwrap();
        assert_eq!(applied.field_mapping.unwrapping_algorithm, UnwrappingAlgorithm::Romeo);
        assert!(settled.outranked.iter().any(|r| r.rule == "complex-fit.unwrap-region-growing"), "{settled:?}");
        assert!(settled.apply.iter().any(|r| r.rule == "fansi.no-laplacian-multi-echo") && settled.conflicting.is_empty(), "{settled:?}");
    }
}

/// Two authors' settings that disagree leave the setting for the user: it keeps its value, nothing
/// is recorded, the setting stays marked, and `apply_all` reports them. The others are applied.
#[test]
fn apply_all_leaves_two_authors_settings_that_disagree_to_the_user() {
    let (c, found) = two_authors_disagree();
    let mut backwards = found.clone();
    backwards.reverse();
    for findings in [found, backwards] {
        let mut applied = c.clone();
        let settled = apply_all(&mut applied, &findings).unwrap();
        assert_eq!(applied.bg_removal.algorithm, BfAlgorithm::Lbv);
        let left: Vec<&str> = settled.conflicting.iter().map(|r| r.rule.as_str()).collect();
        assert!(left.contains(&"medi.bg-pdf") && left.contains(&"chi-sep.bg-vsharp"), "{left:?}");
        assert!(settled.apply.iter().all(|r| r.setting != "bg_removal.algorithm"), "{settled:?}");
        assert_eq!(applied.dismissed_recommendations, c.dismissed_recommendations);
        let after = recommendations(&applied, &known());
        assert!(after.iter().any(|r| r.setting == "bg_removal.algorithm" && r.departs), "still shown");
    }
}

/// `accept`, `apply_all` and `settle` are one answer: taking every kind, `accept` and `apply_all`
/// reach the same config, and report the same conflicts.
#[test]
fn accept_and_apply_all_give_one_answer() {
    for (c, found) in [two_authors_disagree(), reasoned_and_authors_disagree()] {
        let (mut accepted, mut all) = (c.clone(), c.clone());
        let (applied, conflicting) = accept(&mut accepted, &found, Accept { recommendations: true, reference: true });
        let settled = apply_all(&mut all, &found).unwrap();
        assert_eq!(accepted, all);
        assert_eq!((applied, conflicting), (settled.apply, settled.conflicting));
    }
}

/// Only what the user chose and is still open changes a setting: a finding not departed from, or
/// dismissed, changes nothing, nor do settings no finding is about.
#[test]
fn apply_all_changes_only_open_findings() {
    let rule = core::RULES.iter().find(|r| r.id == "medi-msmv.prefilter").unwrap();
    let c = violating(rule);
    let facts = Facts::default();
    let open = recommendations(&c, &facts).into_iter().find(|r| r.rule == rule.id).unwrap();

    let mut dismissed = open.clone();
    dismissed.dismissed = true;
    let mut followed = open.clone();
    followed.departs = false;
    for finding in [dismissed, followed] {
        let mut same = c.clone();
        let settled = apply_all(&mut same, &[finding]).unwrap();
        assert_eq!(same, c);
        assert_eq!(settled, Settled::default());
    }
    assert_eq!(apply_all(&mut c.clone(), &[]).unwrap(), Settled::default());

    let mut changed = c.clone();
    apply_all(&mut changed, &[open]).unwrap();
    assert!(changed.inversion.medi.msmv_prefilter);
    changed.inversion.medi.msmv_prefilter = c.inversion.medi.msmv_prefilter;
    assert_eq!(changed, c, "nothing else changes");
}

/// Findings that agree on a value are all applied, whatever their strengths; a weaker one that
/// wants the same value as the strongest is not left out.
#[test]
fn findings_that_agree_are_applied_together() {
    let mut c = PipelineConfig::default();
    schema::set(&mut c, "inversion.algorithm", json!("medi")).unwrap();
    schema::set(&mut c, "bg_removal.algorithm", json!("pdf")).unwrap();
    let found = recommendations(&c, &known());
    let masks: Vec<&str> = found.iter().filter(|r| r.setting == "masking.sections").map(|r| r.rule.as_str()).collect();
    assert!(masks.contains(&"medi.mask-bet") && masks.contains(&"pdf.mask-bet"), "{masks:?}");
    let settled = settle(&found);
    assert!(masks.iter().all(|m| settled.apply.iter().any(|r| r.rule == *m)), "{settled:?}");
    assert!(settled.conflicting.is_empty() && settled.outranked.is_empty());
}

/// A finding `apply` cannot carry out is an error naming why; the others are still tried.
#[test]
fn apply_all_tries_every_finding_and_lists_what_failed() {
    let rule = core::RULES.iter().find(|r| r.id == "medi-msmv.prefilter").unwrap();
    let mut c = violating(rule);
    let good = recommendations(&c, &Facts::default()).into_iter().find(|r| r.rule == rule.id).unwrap();
    let mut bad = good.clone();
    bad.rule = "no.such.rule".into();
    bad.setting = "no.such.setting".into();
    let e = apply_all(&mut c, &[bad, good]).unwrap_err();
    assert!(e.contains("no rule no.such.rule"), "{e}");
    assert!(c.inversion.medi.msmv_prefilter, "the other finding was still applied");
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
    assert_eq!(m, Marker { actionable: false,
        text: "QSMART: BET, first echo, intensity threshold 100, majority filter, closing radius 2, no erosion".into() });
    // Two methods whose references agree share the value.
    let mut c = medi_vsharp_threshold();
    set(&mut c, "bg_removal.algorithm", &json!("pdf")).unwrap();
    let m = marker(&reference_values(&c, &facts), "masking.sections").unwrap();
    assert_eq!(m.text, "MEDI, PDF: BET · MEDI: RSS magnitude, no erosion");
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

/// Ticking HEIDI (`inversion.heidi.enabled`, as qsmbly and the TUI set it) after any other
/// inversion creates an actionable finding on the inversion that recommends LSQR, in QSMxT's
/// terms and in the settings view; the config is not changed, and applying it chooses LSQR
/// with HEIDI still on.
#[test]
fn ticking_heidi_offers_lsqr_and_changes_nothing() {
    let facts = known();
    for alg in ["rts", "tkd", "tikhonov", "ndi", "medi", "tv", "ilsqr", "star"] {
        let mut c = PipelineConfig::default();
        schema::set(&mut c, "inversion.algorithm", json!(alg)).unwrap();
        schema::set(&mut c, "inversion.tkd.threshold", json!(0.1)).unwrap();
        let before = recommendations(&c, &facts);
        assert!(!before.iter().any(|r| r.rule.starts_with("heidi.")), "{alg}: {before:?}");
        schema::set(&mut c, "inversion.heidi.enabled", json!(true)).unwrap();
        let after = recommendations(&c, &facts);
        let r = after.iter().find(|r| r.setting == "inversion.algorithm" && r.rule.starts_with("heidi.") && r.actionable())
            .unwrap_or_else(|| panic!("{alg}: no LSQR offered: {after:?}"));
        assert_eq!(r.recommended.value, json!("lsqr"), "{alg}");
        assert!(matches!(r.strength, Strength::Recommended | Strength::Discouraged), "{alg}: {:?}", r.strength);
        if alg != "medi" && alg != "tv" && alg != "ilsqr" && alg != "star" {
            assert_eq!(r.strength, Strength::Recommended, "{alg}");
        }
        assert_eq!(schema::get(&c, "inversion.algorithm"), Some(json!(alg)), "{alg}: changed silently");
        let view = schema::settings_view(&c, &view_facts(&facts));
        assert!(view.iter().any(|v| v.path == "inversion.algorithm" && v.recommendations.iter().any(|x| x.rule == r.rule)), "{alg}");
        let rule = r.rule.clone();
        apply(&mut c, &rule, None).unwrap();
        assert_eq!((schema::get(&c, "inversion.algorithm"), c.inversion.heidi.enabled), (Some(json!("lsqr")), true), "{alg}");
        assert!(!recommendations(&c, &facts).iter().any(|r| r.rule.starts_with("heidi.seed")), "{alg}");
    }
}

/// A loaded scan's manufacturer reaches the vendor rules through its facts; example facts stay
/// silent, whatever manufacturer they name.
#[test]
fn the_vendor_rules_read_a_loaded_scans_manufacturer_only() {
    let mut c = PipelineConfig::default();
    c.pipeline.do_chi_separation = true;
    schema::set(&mut c, "separation.algorithm", json!("chi-sepnet")).unwrap();
    let scan = crate::scan::ScanFacts {
        voxel_size: Some((1.0, 1.0, 1.0)),
        manufacturer: Some("Siemens".into()),
        ..crate::plan::tests::facts()
    };
    let offered = |view: &ViewFacts| schema::settings_view(&c, view).into_iter()
        .find(|v| v.path == "pipeline.tukey_window").unwrap().recommendations.into_iter()
        .map(|r| r.rule).collect::<Vec<_>>();
    assert_eq!(offered(&ViewFacts::from(&scan)), ["chi-sepnet.tukey-not-philips"]);
    assert!(offered(&ViewFacts::of_example(&scan)).is_empty());
}

/// χ-sepnet's toolbox route offers the toolbox's Tukey window for the scan's manufacturer, as the
/// sidecar spells it, as the authors' setting, and changes nothing until applied: 0.4 for every
/// known manufacturer but Philips. The native route and an unknown manufacturer offer nothing.
/// χ-sep-MEDI and χ-sep-iLSQR offer it too.
#[test]
fn chi_sepnet_offers_the_vendor_tukey_window() {
    let mut c = PipelineConfig::default();
    c.pipeline.do_chi_separation = true;
    schema::set(&mut c, "separation.algorithm", json!("chi-sepnet")).unwrap();
    let tukey = |c: &PipelineConfig, m: Option<&str>| recommendations(c, &known().with_manufacturer(m.map(str::to_string)))
        .into_iter().filter(|r| r.setting == "pipeline.tukey_window").collect::<Vec<_>>();
    let r = tukey(&c, Some("SIEMENS")).pop().expect("Siemens: no Tukey window offered");
    assert_eq!((r.rule.as_str(), r.strength, r.recommended.value.clone()), ("chi-sepnet.tukey-not-philips", Strength::Reference, json!(0.4)));
    assert_eq!(r.kind, Kind::Reference);
    assert_eq!(tukey(&c, Some("GE MEDICAL SYSTEMS")).len(), 1);
    assert!(tukey(&c, Some("Philips Medical Systems")).is_empty(), "Philips: 0 is already the default");
    assert!(tukey(&c, None).is_empty());
    assert_eq!(tukey(&c, Some("Canon_MEC")).pop().map(|r| r.recommended.value), Some(json!(0.4)));
    assert_eq!(c.pipeline.tukey_window, 0.0, "changed silently");
    let view = schema::settings_view(&c, &ViewFacts { manufacturer: Some("Siemens".into()), ..view_facts(&known()) });
    assert!(view.iter().any(|v| v.path == "pipeline.tukey_window" && v.recommendations.iter().any(|x| x.rule == r.rule)));
    apply(&mut c, &r.rule, None).unwrap();
    assert_eq!(c.pipeline.tukey_window, 0.4);
    assert!(tukey(&c, Some("Siemens")).is_empty());
    assert_eq!(tukey(&c, Some("Philips")).pop().map(|r| r.recommended.value), Some(json!(0)));
    let mut native = PipelineConfig { ..c.clone() };
    native.pipeline.tukey_window = 0.0;
    schema::set(&mut native, "separation.chi_sepnet.route", json!("native")).unwrap();
    assert!(tukey(&native, Some("Siemens")).is_empty());
    for alg in ["chi-sep-medi", "chi-sep-ilsqr"] {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        schema::set(&mut c, "separation.algorithm", json!(alg)).unwrap();
        let r = tukey(&c, Some("Siemens")).pop().unwrap_or_else(|| panic!("{alg}: no Tukey window offered"));
        assert_eq!((r.rule.as_str(), r.kind, r.recommended.value.clone()), ("chi-sep.tukey-not-philips", Kind::Reference, json!(0.4)), "{alg}");
    }
}

/// The Note always says whose rule it is: the choice a discouraged value is discouraged with, and
/// why, and which reference pipeline a value differs from.
#[test]
fn notes_name_the_rule_they_come_from() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Medi;
    c.inversion.heidi.enabled = true;
    let found = recommendations(&c, &Facts::default());
    let r = found.iter().find(|r| r.rule == "heidi.seed-regularised").unwrap();
    assert_eq!(r.note(), format!("QSM inversion used MEDI, which is discouraged with HEIDI streaking reduction: {}", r.reason));

    let mut c = PipelineConfig::default();
    c.field_mapping.b0_estimation = crate::enums::B0Estimation::ComplexFit;
    c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
    let found = recommendations(&c, &Facts::default());
    let r = found.iter().find(|r| r.rule == "complex-fit.unwrap-region-growing").unwrap();
    assert_eq!(r.note(), "Phase unwrapping used Laplacian; this differs from the Complex fit reference pipeline, which uses MEDI region growing");
}

/// QSMART's mask refinements land in `brainmask.m`'s order (threshold, majority, closing) after
/// the generator, whatever order they are set in, and a user's own closing keeps its place; 0 and
/// off take them out again.
#[test]
fn qsmarts_mask_refinements_are_placed_in_brainmask_m_order() {
    let mut c = PipelineConfig::default();
    set(&mut c, "masking.generator", &json!("bet")).unwrap();
    let before = c.masking.sections[0].refinements.clone();
    for (path, v) in [("masking.close_radius", json!(2)), ("masking.majority", json!(true)), ("masking.min_intensity", json!(100))] {
        set(&mut c, path, &v).unwrap();
    }
    let mut want = vec![MaskOp::MinIntensity { value: 100.0 }, MaskOp::Majority, MaskOp::Close { radius: 2 }];
    want.extend(before.iter().cloned());
    assert_eq!(c.masking.sections[0].refinements, want);
    // Set again, nothing is duplicated.
    set(&mut c, "masking.min_intensity", &json!(50)).unwrap();
    set(&mut c, "masking.majority", &json!(true)).unwrap();
    assert_eq!(c.masking.sections[0].refinements[..2], [MaskOp::MinIntensity { value: 50.0 }, MaskOp::Majority]);
    for (path, v) in [("masking.close_radius", json!(0)), ("masking.majority", json!(false)), ("masking.min_intensity", json!(0))] {
        set(&mut c, path, &v).unwrap();
    }
    assert_eq!(c.masking.sections[0].refinements, before);
    // A closing already in the recipe keeps its place and takes the radius.
    c.masking.sections[0].refinements = vec![MaskOp::Dilate { iterations: 1 }, MaskOp::Close { radius: 1 }];
    set(&mut c, "masking.close_radius", &json!(2)).unwrap();
    assert_eq!(c.masking.sections[0].refinements, [MaskOp::Dilate { iterations: 1 }, MaskOp::Close { radius: 2 }]);
    // Named as QSMxT's recipe reads them.
    for (path, v, name) in [("masking.min_intensity", json!(100), "intensity threshold 100"), ("masking.majority", json!(true), "majority filter"),
                            ("masking.close_radius", json!(2), "closing radius 2"), ("masking.close_radius", json!(0), "no closing")] {
        let (q, kind) = setting_of(path).unwrap();
        assert_eq!((q, name_of(q, kind, &v).as_str()), ("masking.sections", name));
    }
    assert_eq!(setting_of("masking.bet.orientation"), Some(("bet.orientation", Mapping::Same)));
}
