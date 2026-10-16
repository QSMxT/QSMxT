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
        ValueType::Enum { .. } => s.value_type.options().iter().map(|o| o.value.clone()).filter(|v| v != cur).collect(),
        // The named echoes, and an echo by number.
        ValueType::EchoReference { .. } => s.value_type.options().iter().map(|o| o.value.clone())
            .chain([json!("3")]).filter(|v| v != cur).collect(),
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
        assert!(!s.help.is_empty(), "{}: no help", s.path);
        assert!(!s.full_label.trim().is_empty(), "{}: no full label", s.path);
        if let Some(p) = s.parent {
            assert!(paths.contains(p), "{}: parent {p} is not a setting", s.path);
            assert_ne!(p, s.path);
        }
        assert!(!s.also_in.contains(&s.section), "{}", s.path);
        assert_eq!(s.section.tab(), s.tab, "{}", s.path);
        if matches!(s.value_type, ValueType::Enum { .. } | ValueType::EchoReference { .. }) {
            let options = s.value_type.options();
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

#[test]
fn the_derivatives_tools_are_supplied_files_in_the_browser() {
    use schema::Browser;
    let by: BTreeMap<&str, Browser> = schema::schema().iter().map(|s| (s.path, s.browser)).collect();
    for p in ["masking.custom_mask_tool", "separation.custom_qsm_tool", "separation.custom_r2_tool",
              "separation.custom_r2prime_tool", "segmentation.custom_dseg_tool"] {
        assert_eq!(by[p], Browser::SuppliedFile, "{p}");
        // The value a browser run sets is one the setting takes.
        let mut c = PipelineConfig::default();
        schema::set(&mut c, p, json!(schema::BROWSER_SUPPLIED_TOOL)).unwrap();
    }
    assert_eq!(by["pipeline.export_dicom"], Browser::Unavailable);
    // A single run has no other orientations to group.
    assert_eq!(by["multi_orientation.group_by"], Browser::Unavailable);
    let others = by.values().filter(|&&b| b == Browser::Available).count();
    assert_eq!(others, by.len() - 7);
}

#[test]
fn every_supplied_file_setting_names_the_item_it_fills() {
    use qsmxt_engine::store::Item;
    use schema::Browser;
    let mut items = Vec::new();
    for s in schema::schema() {
        let supplied = s.browser == Browser::SuppliedFile;
        assert_eq!(s.item.is_some(), supplied, "{}: item", s.path);
        assert_eq!(s.browser_help.is_some(), supplied, "{}: browser help", s.path);
        if let Some(h) = s.browser_help {
            // Worded for a file input, not for the command line's tool names.
            assert!(!h.contains("derivatives") && !h.contains('*') && !h.contains("<tool>"), "{}: {h}", s.path);
        }
        items.extend(s.item);
    }
    assert_eq!(items, [Item::SuppliedMask, Item::SuppliedChi, Item::SuppliedR2, Item::SuppliedR2prime, Item::SuppliedSegmentation]);
    let doc: Value = serde_json::from_str(&schema::schema_json()).unwrap();
    let mask = doc["settings"].as_array().unwrap().iter().find(|s| s["path"] == "masking.custom_mask_tool").unwrap();
    assert_eq!(mask["item"], "SuppliedMask");
}

#[test]
fn browser_defaults_are_qsmxts_except_where_the_browser_profile_changes_them() {
    let changed: Vec<&str> = schema::schema().into_iter()
        .filter(|s| s.browser_default != s.default).map(|s| s.path).collect();
    assert_eq!(changed, ["inversion.tile_size", "inversion.tile_halo", "separation.r2primenet.patch"]);
    for s in schema::schema() {
        assert_eq!(Some(s.browser_default.clone()), schema::get(&PipelineConfig::browser(), s.path), "{}", s.path);
    }
}

#[test]
fn every_option_has_a_display_name() {
    for s in schema::schema() {
        let names: BTreeSet<&str> = s.value_type.options().iter().map(|o| o.name.as_str()).collect();
        for o in s.value_type.options() {
            assert!(!o.name.trim().is_empty(), "{}: option {} has no name", s.path, o.value);
        }
        assert_eq!(names.len(), s.value_type.options().len(), "{}: two options share a name", s.path);
    }
}

/// Every piece of help a front end shows for a setting.
fn helps(s: &Setting) -> Vec<String> {
    let mut out = vec![s.help.to_string()];
    out.extend(s.value_help.map(str::to_string));
    out.extend(s.browser_help.map(str::to_string));
    out.extend(s.value_type.options().iter().map(|o| o.help.clone()));
    out
}

#[test]
fn no_help_names_the_default() {
    // The schema carries QSMxT's default and the browser's, which can differ.
    for s in schema::schema() {
        for h in helps(&s) {
            let l = h.to_lowercase();
            let names_it = l.contains("(default") || l.contains("by default") || l.contains("default:")
                || l.split("default ").skip(1).any(|r| r.starts_with(|c: char| c.is_ascii_digit()));
            assert!(!names_it, "{}: {h:?}", s.path);
        }
    }
}

/// Whether `unit` is a word of `text` other than after a number ("32 mm" is a value, not the
/// units restated).
fn names_unit(text: &str, unit: &str) -> bool {
    let words = |u: &str| text.match_indices(u).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + u.len()..].chars().next();
        let word = before.is_none_or(|c| !c.is_alphanumeric()) && after.is_none_or(|c| !c.is_alphanumeric());
        let after_number = text[..i].trim_end().chars().next_back().is_some_and(|c| c.is_ascii_digit());
        word && !after_number
    });
    let spelled: &[&str] = match unit { "deg" => &["deg", "degrees", "°"], u => &[u] };
    spelled.iter().any(|u| words(u))
}

#[test]
fn units_are_not_repeated_in_labels_or_help() {
    for s in schema::schema() {
        let Some(u) = s.units else { continue };
        let mut texts = helps(&s);
        texts.push(s.full_label.to_string());
        texts.extend(s.value_label.map(str::to_string));
        for t in texts {
            assert!(!names_unit(&t, u), "{}: {t:?} repeats the units ({u})", s.path);
        }
    }
}

#[test]
fn full_labels_are_not_abbreviated() {
    const SHORT: &[&str] = &["Iter", "Ite", "Tol", "Comb.", "Inhomog.", "Tik", "Reg", "Lim", "Curv", "Const", "Vasc",
                             "Precond", "Mag", "BG", "Param", "Est", "Recon", "Max", "Min", "Pad", "cpl", "para", "dia", "Pec"];
    for s in schema::schema() {
        for w in s.full_label.split([' ', ',']) {
            assert!(!SHORT.contains(&w), "{}: full label {:?} abbreviates {w:?}", s.path, s.full_label);
        }
    }
}

#[test]
fn labels_are_unique_within_a_section_after_their_context() {
    let schema = schema::schema();
    for section in schema::Section::ALL {
        let mut seen: BTreeMap<(&str, Option<&str>), &str> = BTreeMap::new();
        for s in schema.iter().filter(|s| s.section == section || s.also_in.contains(&section)) {
            if let Some(other) = seen.insert((s.full_label, s.context), s.path) {
                panic!("{section:?}: {} and {other} are both {:?} ({:?})", s.path, s.full_label, s.context);
            }
        }
    }
    // A method's own parameters name it.
    let ctx = |p: &str| schema.iter().find(|s| s.path == p).unwrap().context;
    assert_eq!(ctx("bg_removal.pdf.tol"), Some("PDF"));
    assert_eq!(ctx("bg_removal.vsharp.threshold"), Some("V-SHARP"));
    assert_eq!(ctx("inversion.heidi.denoise_iterations"), Some("HEIDI"));
    assert_eq!(ctx("inversion.qsmart.ilsqr_max_iter"), Some("QSMART"));
    assert_eq!(ctx("inversion.amp_pe.nlevel"), Some("AMP-PE"));
    assert_eq!(ctx("pipeline.do_qsm"), None);
}

#[test]
fn the_echo_reference_says_which_choice_is_an_echo_number() {
    let s = schema::setting("motion_correction.reference").unwrap();
    let ValueType::EchoReference { options } = &s.value_type else { panic!() };
    let kinds: Vec<_> = options.iter().map(|o| o.kind).collect();
    use schema::EchoKind::*;
    assert_eq!(kinds, [Named, Named, EchoNumber { min: 1 }]);
    let doc: Value = serde_json::to_value(&s).unwrap();
    assert_eq!(doc["options"][0]["kind"], "named");
    assert_eq!(doc["options"][2]["kind"], "echo-number");
    assert_eq!(doc["options"][2]["min"], 1);
    // Any echo number is a value the setting takes.
    let mut c = PipelineConfig::default();
    schema::set(&mut c, s.path, json!("4")).unwrap();
}

#[test]
fn a_value_out_of_range_is_an_error_on_its_own_setting() {
    let error = |c: &PipelineConfig, p: &str| schema::settings_view(c, &ViewFacts::default())
        .into_iter().find(|v| v.path == p).unwrap().error;
    let mut c = PipelineConfig::default();
    assert!(schema::settings_view(&c, &ViewFacts::default()).iter().all(|v| v.error.is_none()));
    c.inversion.tkd.threshold = 0.0;
    // TKD is not the method, so its threshold does not apply.
    assert_eq!(error(&c, "inversion.tkd.threshold"), None);
    c.inversion.algorithm = qsmxt_engine::QsmAlgorithm::Tkd;
    assert_eq!(error(&c, "inversion.tkd.threshold").as_deref(), Some("Must be greater than 0"));
    c.bg_removal.vsharp.max_radius = f64::NAN;
    assert_eq!(error(&c, "bg_removal.vsharp.max_radius").as_deref(), Some("Must be a finite number"));
    // A setting's own rule.
    c.pipeline.do_r2primemap = true;
    c.separation.r2primenet.patch = [192, 192, 120];
    assert_eq!(error(&c, "separation.r2primenet.patch").as_deref(), Some("Every dimension must be a positive multiple of 16 (got 120)"));
    let doc = serde_json::to_value(schema::settings_view(&c, &ViewFacts::default())).unwrap();
    let tkd = doc.as_array().unwrap().iter().find(|v| v["path"] == "inversion.tkd.threshold").unwrap();
    assert_eq!(tkd["error"], "Must be greater than 0");
}
