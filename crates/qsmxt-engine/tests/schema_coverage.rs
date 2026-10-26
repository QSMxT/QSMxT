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
        ValueType::IntList => vec![json!([7, 8]), json!([9])],
        ValueType::FloatVec3 => vec![json!([1.5, 2.5, 3.5])],
        ValueType::Text => vec![json!("probe")],
        ValueType::Enum { .. } => s.value_type.options().iter().map(|o| o.value.clone()).filter(|v| v != cur).collect(),
        // The named echoes, and an echo by number.
        ValueType::EchoReference { .. } => s.value_type.options().iter().map(|o| o.value.clone())
            .chain([json!("3")]).filter(|v| v != cur).collect(),
        // The named echo times, and a time of the user's.
        ValueType::EchoTime { .. } => s.value_type.options().iter().map(|o| o.value.clone())
            .filter(|v| v.is_string()).chain([json!({ "seconds": 0.025 })]).filter(|v| v != cur).collect(),
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
    let default = json_of(&PipelineConfig::default());
    for s in schema::schema() {
        let mut c = PipelineConfig::default();
        let cur = schema::get(&c, s.path).unwrap_or_else(|| panic!("{}: no such path", s.path));
        assert_eq!(cur, s.default, "{}: default", s.path);
        schema::set(&mut c, s.path, cur.clone()).unwrap_or_else(|e| panic!("{}: {e}", s.path));
        assert_eq!(json_of(&c), default, "{}: writing the default changed the config", s.path);

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
        if matches!(s.value_type, ValueType::Enum { .. } | ValueType::EchoReference { .. } | ValueType::EchoTime { .. }) {
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
    assert_eq!(changed, ["inversion.tile_size", "inversion.tile_halo", "separation.chi_sepnet.patch",
        "separation.susep_net.patch", "separation.r2primenet.patch", "segmentation.patch"]);
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
            assert!(!SHORT.iter().any(|a| a.eq_ignore_ascii_case(w)), "{}: full label {:?} abbreviates {w:?}", s.path, s.full_label);
        }
    }
}

/// Whether a label is in sentence case: no word but the first is capitalised, except names
/// ("QSM", "V-SHARP", "iLSQR", "SUSEP-Net") and units ("Hz").
fn sentence_case(label: &str) -> bool {
    const NAMES: &[&str] = &["Hz", "Hz/ppm", "Dr+", "Dr−", "SUSEP-Net", "RS2-Net"];
    label.split(' ').enumerate().all(|(i, w)| {
        NAMES.contains(&w.trim_matches(|c: char| "(),:".contains(c)))
            || w.split('-').enumerate().all(|(j, p)| {
                let mut c = p.trim_start_matches('(').chars();
                let capitalised = c.next().is_some_and(char::is_uppercase)
                    && c.clone().any(char::is_lowercase) && !c.any(char::is_uppercase);
                (i == 0 && j == 0) || !capitalised
            })
    })
}

#[test]
fn labels_are_in_sentence_case() {
    for s in schema::schema() {
        let options = s.value_type.options();
        for l in [s.label, s.full_label].into_iter().chain(s.value_label).chain(s.context).chain(options.iter().map(|o| o.name.as_str())) {
            assert!(sentence_case(l), "{}: {l:?} is not in sentence case", s.path);
        }
    }
    let m = schema::mask_schema();
    for i in &m.inputs {
        assert!(sentence_case(i.label), "{:?} is not in sentence case", i.label);
    }
    for s in &m.steps {
        for l in std::iter::once(s.name).chain(s.params.iter().map(|p| p.label)) {
            assert!(sentence_case(l), "{}: {l:?} is not in sentence case", s.kind);
        }
    }
    assert!(!sentence_case("Background Removal") && sentence_case("V-SHARP max radius"));
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

/// Every method's echo time is an echo-time setting defaulting to its reference's choice; a given
/// time that is not positive is the setting's error, and so is a PDF padding of 1.
#[test]
fn each_methods_echo_time_and_pdf_padding_are_checked() {
    let defaults = [("nltv", "first"), ("medi", "spacing"), ("tfi", "spacing"), ("ndi", "first"),
                    ("fansi", "spacing"), ("l1qsm", "first"), ("whqsm", "first"), ("heidi", "last"), ("tgv", "first")];
    for (m, d) in defaults {
        let s = schema::setting(&format!("inversion.{m}.te")).unwrap_or_else(|| panic!("{m}"));
        assert!(matches!(s.value_type, ValueType::EchoTime { .. }), "{m}");
        assert_eq!(s.default, json!(d), "{m}");
    }
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = qsmxt_engine::QsmAlgorithm::Medi;
    c.inversion.medi.te = qsmxt_engine::EchoTimeChoice::Seconds(0.0);
    c.bg_removal.algorithm = qsmxt_engine::BfAlgorithm::Pdf;
    c.bg_removal.pdf.n_pad = 1;
    let view = schema::settings_view(&c, &Default::default());
    let error = |p: &str| view.iter().find(|v| v.path == p).and_then(|v| v.error.clone());
    assert!(error("inversion.medi.te").is_some_and(|e| e.contains("greater than 0 ms")), "{view:?}");
    assert!(error("bg_removal.pdf.n_pad").is_some_and(|e| e.contains("at least 2")), "{view:?}");
}

#[test]
fn an_echo_time_says_which_choice_is_a_time_of_the_users() {
    let s = schema::setting("inversion.star.te").unwrap();
    let ValueType::EchoTime { options } = &s.value_type else { panic!() };
    let kinds: Vec<_> = options.iter().map(|o| o.kind).collect();
    use schema::EchoKind::*;
    assert_eq!(kinds, [Named, Named, Named, Named, Time]);
    let values: Vec<_> = options.iter().map(|o| o.option.value.clone()).collect();
    assert_eq!(values, [json!("first"), json!("last"), json!("spacing"), json!("sum"), json!({ "seconds": null })]);
    assert_eq!(s.default, json!("sum"));
    let doc: Value = serde_json::to_value(&s).unwrap();
    assert_eq!(doc["type"], "echo-time");
    assert_eq!(doc["options"][4]["kind"], "time");
    let mut c = PipelineConfig::default();
    schema::set(&mut c, s.path, json!({ "seconds": 0.03 })).unwrap();
    assert_eq!(c.inversion.star.te, qsmxt_engine::EchoTimeChoice::Seconds(0.03));
    // A time that is not positive is the setting's error.
    c.inversion.algorithm = qsmxt_engine::QsmAlgorithm::Star;
    c.inversion.star.te = qsmxt_engine::EchoTimeChoice::Seconds(0.0);
    let view = schema::settings_view(&c, &Default::default());
    let te = view.iter().find(|v| v.path == s.path).unwrap();
    assert!(te.error.as_deref().is_some_and(|e| e.contains("greater than 0 ms")), "{te:?}");
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
    assert_eq!(error(&c, "inversion.tkd.threshold").as_deref(), Some("Must be from 0 to 2/3 (not 0 or 2/3)"));
    c.bg_removal.vsharp.max_radius = f64::NAN;
    assert_eq!(error(&c, "bg_removal.vsharp.max_radius").as_deref(), Some("Must be a finite number"));
    // A setting's own rule.
    c.pipeline.do_r2primemap = true;
    c.separation.r2primenet.patch = [192, 192, 120];
    assert_eq!(error(&c, "separation.r2primenet.patch").as_deref(), Some("R2PRIMEnet patch W = 120 must be a positive multiple of 16 (four pooling levels)"));
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = qsmxt_engine::SeparationAlgorithm::ChiSepNet;
    c.separation.chi_sepnet.patch = [128, 120, 64];
    assert_eq!(error(&c, "separation.chi_sepnet.patch").as_deref(), Some("χ-sepnet patch H = 120 must be a positive multiple of 16 (four pooling levels)"));
    c.separation.algorithm = qsmxt_engine::SeparationAlgorithm::SusepNet;
    c.separation.susep_net.patch = Some([128, 128, 12]);
    assert_eq!(error(&c, "separation.susep_net.patch").as_deref(), Some("SUSEP-Net patch W = 12 must be a positive multiple of 8 (three pooling levels)"));
    let doc = serde_json::to_value(schema::settings_view(&c, &ViewFacts::default())).unwrap();
    let tkd = doc.as_array().unwrap().iter().find(|v| v["path"] == "inversion.tkd.threshold").unwrap();
    assert_eq!(tkd["error"], "Must be from 0 to 2/3 (not 0 or 2/3)");
}

/// QSM.rs's parameter checks (`validate()`) reach the settings view on the setting they name,
/// in QSM.rs's words, and the plan-time check refuses the same config with the same message.
#[test]
fn qsm_rs_parameter_checks_reach_the_view_and_the_plan() {
    use qsmxt_engine::{BfAlgorithm, QsmAlgorithm, SeparationAlgorithm, VsharpGrid};
    type Case = (&'static str, fn(&mut PipelineConfig), &'static str);
    let cases: Vec<Case> = vec![
        ("bg_removal.vsharp.min_radius", |c| { c.bg_removal.algorithm = BfAlgorithm::Vsharp; c.bg_removal.vsharp.min_radius = 0.0; },
            "V-SHARP min_radius = 0 is out of range: must be finite and > 0 mm"),
        ("bg_removal.vsharp.grid_spacing", |c| {
            c.bg_removal.algorithm = BfAlgorithm::Vsharp;
            c.bg_removal.vsharp.grid = VsharpGrid::Isotropic;
            c.bg_removal.vsharp.grid_spacing = -1.0;
        }, "V-SHARP isotropic working grid spacing_mm = -1 is out of range: must be finite and > 0 mm"),
        ("inversion.qsmart.frangi_scale_ratio", |c| { c.inversion.algorithm = QsmAlgorithm::Qsmart; c.inversion.qsmart.frangi_scale_ratio = 0.0; },
            "Frangi scale_ratio = 0 is out of range: must be finite and > 0"),
        ("inversion.amp_pe.nlevel", |c| { c.inversion.algorithm = QsmAlgorithm::AmpPe; c.inversion.amp_pe.nlevel = 64; },
            "AMP-PE nlevel = 64 is out of range: must be <= 63"),
        ("inversion.amp_pe.wave_order", |c| { c.inversion.algorithm = QsmAlgorithm::AmpPe; c.inversion.amp_pe.wave_order = 5; },
            "AMP-PE wave_order = 5 is out of range: must be from 1 to 4 (db1–db4)"),
        ("inversion.ilsqr.pad_mm", |c| { c.inversion.algorithm = QsmAlgorithm::Ilsqr; c.inversion.ilsqr.pad_mm = -1.0; },
            "iLSQR padding must be finite and >= 0 mm, got -1"),
        ("separation.decompose.chi_bound", |c| {
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = SeparationAlgorithm::Decompose;
            c.separation.decompose.chi_bound = -0.1;
        }, "DECOMPOSE chi_bound = -0.1 is out of range: must be >= 0 ppm"),
        ("separation.wavesep.wavelet_order", |c| {
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = SeparationAlgorithm::WaveSep;
            c.separation.wavesep.wavelet_order = 0;
        }, "WaveSep wavelet_order = 0 is out of range: must be from 1 to 4 (db1–db4)"),
    ];
    for (path, edit, message) in cases {
        let mut c = PipelineConfig::default();
        edit(&mut c);
        let view = schema::settings_view(&c, &ViewFacts::default());
        let v = view.iter().find(|v| v.path == path).unwrap();
        assert!(v.visible, "{path} should apply");
        let shown = v.error.clone().unwrap_or_else(|| panic!("{path}: no error"));
        // The view gives what follows the subject, capitalised (or as QSM.rs wrote a lower-case name).
        let rest = message.split_once(": ").map_or(message, |(_, r)| r);
        assert!(shown.to_lowercase().starts_with(&rest.to_lowercase()), "{path}: {shown} vs {message}");
        let e = schema::check_ranges(&c).unwrap_err().to_string();
        assert!(e.contains(path) && e.contains(message), "{path}: {e}");
    }
    // QSMART's iLSQR padding is checked with iLSQR's own check.
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Qsmart;
    c.inversion.qsmart.ilsqr_pad_mm = -1.0;
    let e = schema::check_ranges(&c).unwrap_err().to_string();
    assert!(e.contains("inversion.qsmart.ilsqr_pad_mm (--qsmart-ilsqr-pad-mm): iLSQR padding must be finite"), "{e}");
}

/// A range's text never prints a bound to more than 4 significant digits (a bound built from a
/// constant, such as 2/3, would otherwise read 0.6666666666666666). The bounds themselves stay
/// exact.
#[test]
fn range_text_numbers_are_short() {
    let sig = |n: &str| -> usize {
        let digits: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
        let t = digits.trim_start_matches('0');
        // An integer's trailing zeros are not significant ("100").
        if n.contains('.') { t.len() } else { t.trim_end_matches('0').len() }
    };
    let texts: Vec<String> = schema::settings().iter().map(|s| s.range_text()).collect();
    for text in &texts {
        for word in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
            let word = word.trim_matches('.');
            if !word.is_empty() && word.chars().any(|c| c.is_ascii_digit()) && word.parse::<f64>().is_ok() {
                assert!(sig(word) <= 4, "`{word}` in {text}");
            }
        }
    }
    let tkd = schema::settings().iter().find(|s| s.path == "inversion.tkd.threshold").unwrap();
    assert_eq!(tkd.range_text(), "from 0 to 2/3 (not 0 or 2/3)");
    assert_eq!(tkd.max, Some(qsmxt_engine::qsm_core::kernels::MAX_ABS_DIPOLE));
}

/// AMP-PE's largest nlevel is QSM.rs's for the target (31 on wasm32), and TKD's threshold is
/// below the dipole kernel's largest |D|, as TSVD's and HEIDI's are.
#[test]
fn bounds_come_from_qsm_rs_constants() {
    let s = |p: &str| schema::settings().iter().find(|s| s.path == p).unwrap();
    assert_eq!(s("inversion.amp_pe.nlevel").max, Some(qsmxt_engine::qsm_core::inversion::amp_pe::MAX_NLEVEL as f64));
    for p in ["inversion.tkd.threshold", "inversion.tsvd.threshold", "inversion.heidi.cone_threshold", "inversion.rts.delta"] {
        assert_eq!((s(p).max, s(p).exclusive_max), (Some(qsmxt_engine::qsm_core::kernels::MAX_ABS_DIPOLE), true), "{p}");
    }
    let r = s("inversion.qsmart.frangi_scale_ratio");
    assert_eq!((r.min, r.exclusive_min), (Some(0.0), true));
}

/// The values serde names when it refuses one of type `T`.
fn variants_of<T: serde::de::DeserializeOwned>(probe: Value) -> BTreeSet<String> {
    let err = serde_json::from_value::<T>(probe).err().expect("the probe is refused").to_string();
    let (_, list) = err.split_once("expected one of ").expect(&err);
    list.split(", ").filter_map(|v| v.split('`').nth(1).map(str::to_string)).collect()
}

#[test]
fn the_mask_schema_covers_every_input_step_and_preset() {
    use qsmxt_engine::masking::{mask_presets, MaskOp, MaskingInput};
    use qsmxt_engine::schema::mask::step_kind;
    let m = schema::mask_schema();

    let inputs: BTreeSet<String> = m.inputs.iter().map(|i| json!(i.value).as_str().unwrap().to_string()).collect();
    assert_eq!(inputs, variants_of::<MaskingInput>(json!("__none__")));

    let ops: BTreeSet<String> = m.steps.iter().map(|s| s.op.to_string()).collect();
    assert_eq!(ops, variants_of::<MaskOp>(json!({ "op": "__none__" })));
    let methods: BTreeSet<String> = m.steps.iter().filter(|s| s.op == "threshold")
        .map(|s| s.default["method"].as_str().unwrap().to_string()).collect();
    assert_eq!(methods, variants_of::<qsmxt_engine::masking::MaskThresholdMethod>(json!("__none__")));

    let kinds: BTreeSet<&str> = m.steps.iter().map(|s| s.kind).collect();
    assert_eq!(kinds.len(), m.steps.len(), "a step kind is listed twice");
    for s in &m.steps {
        let op: MaskOp = serde_json::from_value(s.default.clone()).unwrap_or_else(|e| panic!("{}: {e}", s.kind));
        assert_eq!(step_kind(&op), s.kind);
        assert_eq!(s.starts_section, op.is_generator(), "{}", s.kind);
        assert!(!s.name.is_empty() && !s.help.is_empty() && !s.label.is_empty(), "{}", s.kind);
        for p in &s.params {
            assert!(s.default.get(p.key).is_some(), "{}: {} is not a field of the step", s.kind, p.key);
            assert!(!p.label.is_empty() && !p.help.is_empty(), "{}.{}", s.kind, p.key);
            if let Some(x) = p.default.as_f64() {
                assert!(p.in_range(x), "{}.{}: the default {x} is out of range", s.kind, p.key);
            }
            if let Some(u) = p.units {
                assert!(!names_unit(p.help, u), "{}.{}: help repeats the units", s.kind, p.key);
            }
            if let Some(k) = p.only_with {
                assert!(s.params.iter().any(|q| q.key == k), "{}.{}: only with {k}?", s.kind, p.key);
            }
        }
        // Every field but the tag (and a threshold's method, which is the step type) is a
        // parameter; Otsu's unused value stays null.
        for (k, v) in s.default.as_object().unwrap() {
            if k == "op" || (s.op == "threshold" && k == "method") || v.is_null() && s.kind == "threshold-otsu" {
                continue;
            }
            assert!(s.params.iter().any(|p| p.key == k), "{}: {k} has no parameter", s.kind);
        }
    }
    // The step types that start a section come first.
    let first_refinement = m.steps.iter().position(|s| !s.starts_section).unwrap();
    assert!(m.steps[first_refinement..].iter().all(|s| !s.starts_section));

    let names: Vec<&str> = m.presets.iter().map(|p| p.value).collect();
    let expected: Vec<&str> = mask_presets().iter().map(|(n, _)| *n).collect();
    assert_eq!(names, expected);
    for p in &m.presets {
        let mut c = PipelineConfig::default();
        c.masking.sections = p.sections.clone();
        c.masking.combine = p.combine;
        c.masking.refinements = p.refinements.clone();
        let errors: Vec<_> = schema::settings_view(&c, &ViewFacts::default()).into_iter().filter_map(|v| v.error).collect();
        assert!(errors.is_empty(), "preset {}: {errors:?}", p.value);
    }
    // The whole document carries it.
    let doc: Value = serde_json::from_str(&schema::schema_json()).unwrap();
    assert_eq!(doc["mask"]["steps"].as_array().unwrap().len(), m.steps.len());
}

#[test]
fn hd_bet_inference_choices_are_the_patches_mask_names() {
    use qsmxt_engine::masking::{parse_mask_op, MaskOp};
    let choices = schema::mask_schema().hd_bet_inference;
    assert_eq!(choices.len(), 3);
    for c in &choices {
        let op = parse_mask_op(&format!("hd-bet:{}", c.label)).unwrap();
        let MaskOp::HdBet { patch, .. } = op else { panic!() };
        assert_eq!(json!(patch), c.value, "{}", c.label);
        assert_eq!(c.budget_bytes, qsmxt_engine::memory::hd_bet_network_bytes(patch));
    }
    // The whole volume costs most; the low-memory window least.
    assert!(choices[0].budget_bytes > choices[1].budget_bytes);
    assert!(choices[1].budget_bytes > choices[2].budget_bytes);
    // And the step's patch parameter offers the same choices.
    let hd = schema::mask::step("hd-bet").unwrap();
    let values: Vec<Value> = hd.params[0].value_type.options().iter().map(|o| o.value.clone()).collect();
    assert_eq!(values, choices.iter().map(|c| c.value.clone()).collect::<Vec<_>>());
}

#[test]
fn mask_parameter_ranges_match_what_mask_refuses() {
    use qsmxt_engine::masking::parse_mask_op;
    let param = |kind: &str, key: &str| schema::mask::step(kind).unwrap().params.into_iter().find(|p| p.key == key).unwrap();
    // `--mask` refuses these; the ranges say the same.
    for (kind, key, spec, ok) in [
        ("bet", "voxel_scale", "bet:0.5:scale={}", [(0.0, false), (1e-6, true), (10.0, true)]),
        ("hd-bet", "tile_step", "hd-bet:window:step={}", [(0.0, false), (1.0, true), (1.000001, false)]),
        ("rs2-net", "tile_step", "rs2-net:step={}", [(0.0, false), (1.0, true), (1.000001, false)]),
    ] {
        let p = param(kind, key);
        for (x, accepted) in ok {
            assert_eq!(parse_mask_op(&spec.replace("{}", &x.to_string())).is_ok(), accepted, "{spec} = {x}");
            assert_eq!(p.in_range(x), accepted, "{kind}.{key} = {x}");
        }
    }
    // Every bounded parameter, just outside its range, is an error on the recipe that names
    // the step and the parameter; at an inclusive bound, it is not.
    let mut tested = 0;
    for s in schema::mask_schema().steps {
        for p in &s.params {
            let int = matches!(p.value_type, ValueType::Int);
            let step = if int { 1.0 } else { 1e-6 };
            let mut cases = Vec::new();
            if let Some(lo) = p.min { cases.push((if p.exclusive_min { lo } else { lo - step }, false)); }
            if let Some(hi) = p.max { cases.push((if p.exclusive_max { hi } else { hi + step }, false)); }
            if let (Some(lo), false) = (p.min, p.exclusive_min) { cases.push((lo, true)); }
            if let (Some(hi), false) = (p.max, p.exclusive_max) { cases.push((hi, true)); }
            for (x, fine) in cases {
                if int && x < 0.0 { continue; } // an unsigned field cannot hold it
                let mut op = s.default.clone();
                op[p.key] = if int { json!(x as u64) } else { json!(x) };
                // A parameter that applies only with another one: set that one.
                if let Some(k) = p.only_with { op[k] = json!([128, 128, 64]); }
                let mut c = PipelineConfig::default();
                c.masking.sections = vec![qsmxt_engine::masking::MaskSection {
                    input: qsmxt_engine::masking::MaskingInput::Magnitude,
                    generator: if s.starts_section { serde_json::from_value(op.clone()).unwrap() } else { qsmxt_engine::masking::MaskOp::bet(0.5) },
                    refinements: if s.starts_section { vec![] } else { vec![serde_json::from_value(op).unwrap()] },
                }];
                let error = schema::settings_view(&c, &ViewFacts::default()).into_iter()
                    .find(|v| v.path == "masking.sections").unwrap().error;
                if fine {
                    assert_eq!(error, None, "{}.{} = {x}", s.kind, p.key);
                } else {
                    let e = error.unwrap_or_else(|| panic!("{}.{} = {x} was accepted", s.kind, p.key));
                    assert!(e.contains(s.name) && e.contains(p.label) && e.starts_with("Section 1, step "), "{e}");
                    tested += 1;
                }
            }
        }
    }
    assert!(tested >= 10, "{tested}");
}

#[test]
fn a_morphology_step_of_zero_is_refused_by_the_run_and_the_view() {
    use qsmxt_engine::masking::MaskOp;
    let mut c = PipelineConfig::default();
    c.masking.sections[0].refinements[0] = MaskOp::Dilate { iterations: 0 };
    let e = schema::check_ranges(&c).unwrap_err().to_string();
    assert!(e.contains("masking.sections[0].refinements[0]: dilate: iterations must be at least 1, got 0"), "{e}");
    let view = schema::settings_view(&c, &ViewFacts::default());
    let error = view.iter().find(|v| v.path == "masking.sections").unwrap().error.clone().unwrap();
    assert_eq!(error, "Section 1, step 2 (Dilate): Iterations must be at least 1");
    let dilate = schema::mask::step("dilate").unwrap();
    assert_eq!((dilate.params[0].min, dilate.params[0].exclusive_min), (Some(1.0), false));
}

/// The mask recipe carries no warning of QSMxT's own under QSMART: QSMART's reference mask is a
/// pipeline rule's, shown as the setting's marker.
#[test]
fn the_mask_recipe_under_qsmart_has_a_reference_marker_not_a_warning() {
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = qsmxt_engine::QsmAlgorithm::Qsmart;
    let view = schema::settings_view(&c, &ViewFacts::default());
    let mask = view.iter().find(|v| v.path == "masking.sections").unwrap();
    assert_eq!(mask.warning, None);
    assert_eq!(mask.marker.as_ref().map(|m| m.text.as_str()), Some("QSMART: BET"));
}
