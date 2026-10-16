//! The settings schema covers the config: every leaf of `PipelineConfig` is a setting exactly
//! once (or is listed as not user-facing), every path reads and writes through the config's own
//! serde form and TOML, and every enum setting offers every value the config accepts.
//!
//! Whether the schema matches the TUI (labels, help, visibility) is tested on the TUI side,
//! `src/tui/schema_parity.rs`.

use std::collections::{BTreeMap, BTreeSet};

use qsmxt_engine::schema::{self, Setting, ValueType, ViewFacts, NOT_IN_SCHEMA};
use qsmxt_engine::PipelineConfig;
use serde_json::{json, Value};

/// Every leaf path of a config: values that are not objects (arrays count as one value).
fn leaves(v: &Value, prefix: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (k, x) in m {
                leaves(x, &if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") }, out);
            }
        }
        _ => out.push(prefix.to_string()),
    }
}

/// The default config with the optional values that are left out of its serialised form when
/// unset filled in, so their paths show up as leaves.
fn filled() -> PipelineConfig {
    let mut c = PipelineConfig::default();
    c.inversion.tile_size = Some(64);
    c.inversion.tile_halo = Some(8);
    c.qsm.reference_region = Some("thalamus".into());
    c
}

#[test]
fn every_config_leaf_is_a_setting_exactly_once_or_listed_as_not_user_facing() {
    let mut paths = Vec::new();
    leaves(&serde_json::to_value(filled()).unwrap(), "", &mut paths);
    let schema = schema::schema();

    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &schema {
        *count.entry(s.path).or_default() += 1;
    }
    let twice: Vec<_> = count.iter().filter(|(_, &n)| n > 1).map(|(p, _)| *p).collect();
    assert!(twice.is_empty(), "settings listed more than once: {twice:?}");

    let excluded: BTreeSet<&str> = NOT_IN_SCHEMA.iter().map(|(p, _)| *p).collect();
    let missing: Vec<_> = paths.iter()
        .filter(|p| !count.contains_key(p.as_str()) && !excluded.contains(p.as_str()))
        .collect();
    assert!(missing.is_empty(),
        "config settings missing from the schema: {missing:?}\n\
         Add them to crates/qsmxt-engine/src/schema/table.rs (and the TUI), or to NOT_IN_SCHEMA with why.");
    let both: Vec<_> = excluded.iter().filter(|p| count.contains_key(*p)).collect();
    assert!(both.is_empty(), "both in the schema and NOT_IN_SCHEMA: {both:?}");
    let leaf_set: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
    let stale: Vec<_> = count.keys().chain(excluded.iter()).filter(|p| !leaf_set.contains(*p)).collect();
    assert!(stale.is_empty(), "schema paths the config does not have: {stale:?}");
}

fn json_of(c: &PipelineConfig) -> Value {
    serde_json::to_value(c).unwrap()
}

/// A value of the setting's type other than `cur`, that the config accepts.
fn another(s: &Setting, cur: &Value) -> Vec<Value> {
    let num = |v: &Value| v.as_f64().unwrap_or(0.0);
    let pick_f = |x: f64| {
        let mut v = x * 0.5 + 0.25;
        if let Some(hi) = s.max { v = v.min(hi); }
        if let Some(lo) = s.min { v = v.max(lo); }
        if (v - x).abs() < 1e-12 { v = s.min.unwrap_or(0.0) + 0.125 }
        v
    };
    let mut out = match &s.value_type {
        ValueType::Bool => vec![json!(!cur.as_bool().unwrap())],
        ValueType::Int => vec![json!(cur.as_u64().unwrap_or(0) + 1)],
        ValueType::Float => vec![json!(pick_f(num(cur)))],
        ValueType::IntVec3 => vec![json!([7, 8, 9])],
        ValueType::FloatVec3 => vec![json!([1.5, 2.5, 3.5])],
        ValueType::Text => vec![json!("probe")],
        ValueType::Enum { options } | ValueType::EchoReference { options } => {
            options.iter().map(|o| o.value.clone()).filter(|v| v != cur).collect()
        }
        ValueType::MaskSections => vec![serde_json::to_value(qsmxt_engine::masking::hd_bet_mask_sections()).unwrap()],
        ValueType::MaskOps => vec![json!([{ "op": "erode", "iterations": 2 }])],
    };
    if s.optional && !cur.is_null() {
        out.push(Value::Null);
    }
    out
}

/// Numbers compare by value: `7` and `7.0` are the same setting.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-12,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b)),
        _ => a == b,
    }
}

#[test]
fn every_setting_reads_writes_and_survives_toml() {
    for s in schema::schema() {
        let mut c = PipelineConfig::default();
        let cur = schema::get(&c, s.path).unwrap_or_else(|| panic!("{}: no such path", s.path));
        assert_eq!(cur, s.default, "{}: default", s.path);
        schema::set(&mut c, s.path, cur.clone()).unwrap_or_else(|e| panic!("{}: {e}", s.path));
        assert_eq!(json_of(&c), json_of(&PipelineConfig::default()), "{}: writing the default changed the config", s.path);

        for v in another(&s, &cur) {
            let mut c = PipelineConfig::default();
            schema::set(&mut c, s.path, v.clone()).unwrap_or_else(|e| panic!("{} = {v}: {e}", s.path));
            let got = schema::get(&c, s.path).unwrap();
            assert!(same(&got, &v), "{}: wrote {v}, read {got}", s.path);
            let back = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
            assert!(same(&schema::get(&back, s.path).unwrap(), &v), "{} = {v} does not survive TOML", s.path);
        }
    }
}

#[test]
fn a_value_the_config_rejects_is_refused_and_leaves_the_config_alone() {
    let mut c = PipelineConfig::default();
    assert!(schema::set(&mut c, "bg_removal.algorithm", json!("not-a-method")).is_err());
    assert!(schema::set(&mut c, "bg_removal.vsharp.max_radius", json!("twelve")).is_err());
    assert!(schema::set(&mut c, "bg_removal.vsharp.no_such_setting", json!(1)).is_err());
    assert!(schema::set(&mut c, "no_such_section.x", json!(1)).is_err());
    assert_eq!(json_of(&c), json_of(&PipelineConfig::default()));
}

/// Settings whose options are deliberately a subset of what the type accepts.
const SUBSET_OPTIONS: &[(&str, &str)] = &[
    ("inversion.qsmart.inversion", "QSMART's inner inversion takes only the classical single-step methods"),
];

/// The values serde names when it refuses one: "unknown variant `x`, expected one of `a`, `b`".
fn accepted_variants(path: &str) -> Option<BTreeSet<String>> {
    let mut c = PipelineConfig::default();
    let err = schema::set(&mut c, path, json!("__no_such_variant__")).err()?;
    let (_, list) = err.split_once("expected one of ").or_else(|| err.split_once("expected "))?;
    Some(list.split(", ").flat_map(|v| v.split(" or "))
        .filter_map(|v| v.split('`').nth(1).map(str::to_string)).collect())
}

#[test]
fn enum_settings_offer_every_value_the_config_accepts() {
    for s in schema::schema() {
        let ValueType::Enum { options } = &s.value_type else { continue };
        let offered: BTreeSet<String> = options.iter()
            .map(|o| o.value.as_str().map(str::to_string).unwrap_or_else(|| o.value.to_string())).collect();
        assert_eq!(offered.len(), options.len(), "{}: an option is listed twice", s.path);
        for o in options {
            let mut c = PipelineConfig::default();
            schema::set(&mut c, s.path, o.value.clone()).unwrap_or_else(|e| panic!("{} = {}: {e}", s.path, o.value));
        }
        if SUBSET_OPTIONS.iter().any(|(p, _)| *p == s.path) {
            continue;
        }
        if s.path == "qsm.reference_region" {
            let slugs: BTreeSet<String> = qsmxt_engine::regions::REGIONS.iter().map(|r| r.slug.to_string()).collect();
            assert_eq!(offered, slugs, "{}", s.path);
            continue;
        }
        // Integer-valued enums (MEDI/TFI data weighting) have no variant list to compare with.
        let Some(accepted) = accepted_variants(s.path).filter(|a| !a.is_empty()) else { continue };
        // An alias (`sti` for `dct`) reads back as the value it stands for.
        let canonical: BTreeSet<String> = accepted.iter().map(|v| {
            let mut c = PipelineConfig::default();
            schema::set(&mut c, s.path, json!(v)).unwrap();
            schema::get(&c, s.path).unwrap().as_str().unwrap().to_string()
        }).collect();
        assert_eq!(offered, canonical, "{}: options differ from the values the config accepts", s.path);
    }
}

#[test]
fn the_schema_is_well_formed() {
    let schema = schema::schema();
    let paths: BTreeSet<&str> = schema.iter().map(|s| s.path).collect();
    for (i, s) in schema.iter().enumerate() {
        assert_eq!(s.order, i);
        assert!(!s.label.trim().is_empty(), "{}: no label", s.path);
        assert_eq!(s.label, s.label.trim(), "{}: label padding", s.path);
        let is_choice = matches!(s.value_type, ValueType::Enum { .. } | ValueType::EchoReference { .. });
        assert!(is_choice || !s.help.is_empty(), "{}: no help", s.path);
        if let Some(p) = s.parent {
            assert!(paths.contains(p), "{}: parent {p} is not a setting", s.path);
            assert_ne!(p, s.path);
        }
        assert!(!s.also_in.contains(&s.section), "{}", s.path);
        assert_eq!(s.section.tab(), s.tab, "{}", s.path);
        if let ValueType::Enum { options } | ValueType::EchoReference { options } = &s.value_type {
            assert!(!options.is_empty(), "{}", s.path);
            for o in options {
                assert!(!o.help.contains("https://"), "{}: a link left in help", s.path);
            }
        }
        if let Some(f) = s.cli_flag {
            assert!(f.starts_with("--"), "{}: {f}", s.path);
        }
        if let (Some(lo), Some(hi)) = (s.min, s.max) {
            assert!(lo < hi, "{}", s.path);
        }
    }
    let doc: Value = serde_json::from_str(&schema::schema_json()).unwrap();
    assert_eq!(doc["settings"].as_array().unwrap().len(), schema.len());
    assert_eq!(doc["sections"].as_array().unwrap().len(), schema::Section::ALL.len());
}

#[test]
fn a_default_config_shows_its_own_methods_settings_and_not_the_others() {
    let view = schema::settings_view(&PipelineConfig::default(), &ViewFacts::default());
    let shown = |p: &str| view.iter().find(|v| v.path == p).unwrap_or_else(|| panic!("{p}")).visible;
    assert!(shown("bg_removal.vsharp.max_radius"));
    assert!(!shown("bg_removal.pdf.tol"));
    assert!(shown("inversion.ilsqr.tol"));
    assert!(!shown("inversion.rts.delta"));
    assert!(!shown("swi.strength"));
    assert!(!shown("separation.algorithm"));
    assert!(!shown("multi_orientation.algorithm"));
    assert!(shown("multi_orientation.group_by"));
    // The view lists every setting, in schema order.
    let order: Vec<_> = schema::schema().iter().map(|s| s.path).collect();
    assert_eq!(view.iter().map(|v| v.path).collect::<Vec<_>>(), order);
}

#[test]
fn an_output_another_one_needs_reads_as_forced_by_it() {
    let mut c = PipelineConfig::default();
    c.pipeline.do_qsm = false;
    c.pipeline.do_smwi = true;
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = qsmxt_engine::SeparationAlgorithm::ChiSepIlsqr;
    let view = schema::settings_view(&c, &ViewFacts::default());
    let forced = |p: &str| view.iter().find(|v| v.path == p).unwrap().forced_by;
    assert_eq!(forced("pipeline.do_qsm"), Some("pipeline.do_smwi"));
    assert_eq!(forced("pipeline.do_r2primemap"), Some("pipeline.do_chi_separation"));
    assert_eq!(forced("pipeline.do_r2starmap"), Some("pipeline.do_chi_separation"));
    assert_eq!(forced("pipeline.do_r2map"), Some("pipeline.do_chi_separation"));
    assert_eq!(forced("pipeline.do_swi"), None);
    // Forced on, QSM's own settings apply.
    assert!(view.iter().find(|v| v.path == "inversion.algorithm").unwrap().visible);
}

#[test]
fn pruning_puts_hidden_settings_back_to_their_defaults() {
    let mut c = PipelineConfig::default();
    c.bg_removal.pdf.tol = 0.5; // PDF is not the selected method
    c.bg_removal.vsharp.max_radius = 10.0; // V-SHARP is
    let p = schema::prune_hidden(&c, &ViewFacts::default());
    assert_eq!(p.bg_removal.pdf.tol, PipelineConfig::default().bg_removal.pdf.tol);
    assert_eq!(p.bg_removal.vsharp.max_radius, 10.0);
}
