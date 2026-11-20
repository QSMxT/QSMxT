//! The TUI shows what the engine's settings view says applies, and runs what it prints.
//!
//! The rows come from the registry, so their labels, help and options are the schema's by
//! construction. What is checked here is what the TUI still decides itself: which pages show
//! (a stage that does not run says why), its own conditions on rows, Basic mode, the mask builder
//! and the Input tab. Each check goes over every form [`super::explore`] reaches. That the command it
//! prints parses back to the run it makes is checked in the binary, which has the command line.

use std::collections::{BTreeMap, BTreeSet};

use qsmxt_engine::schema::{self, Section, Setting, ViewFacts};

use super::app::*;
use super::command::config_from_app;
use super::explore::{build, explored, Act, NOT_CONFIG};


fn path_of(field: &str) -> Option<&'static str> {
    field_path(field)
}

/// The config settings a row edits (none for a note, a header that edits nothing, the run button).
fn row_paths(app: &App, row: &PipelineRow) -> Vec<&'static str> {
    use PipelineRow::*;
    let recipe = |section: usize| {
        if section == MASK_COMBINED_SECTION { "masking.refinements" }
        else if is_two_pass_section(section) { "masking.two_pass_sections" }
        else { "masking.sections" }
    };
    match row {
        AlgoSelect { field, .. } | Param { field, .. } | Toggle { field, .. } => {
            if NOT_CONFIG.contains(field) || dicom_map(field).is_some() {
                return vec![];
            }
            let path = path_of(field).unwrap_or_else(|| panic!("TUI row {field} is not mapped to a schema setting"));
            // The TUI's reference picker lists the regions with the two methods.
            if *field == "qsm.reference" && app.field_select(field) >= 2 {
                return vec![path, "qsm.reference_region"];
            }
            vec![path]
        }
        MaskOpInput { section } | MaskOpGenerator { section } | MaskOpGeneratorParam { section }
        | MaskOpThresholdValue { section } | MaskOpHdBetStep { section } | MaskOpBetVoxelScale { section }
        | MaskOpEntry { section, .. } | MaskOpSignalErodeParam { section, .. } | MaskOpAddStep { section } => vec![recipe(*section)],
        MaskOpAddSection { two_pass } => vec![if *two_pass { "masking.two_pass_sections" } else { "masking.sections" }],
        MaskCombineOp { section } if is_two_pass_section(*section) => vec!["masking.two_pass_combine"],
        MaskCombineOp { .. } => vec!["masking.combine"],
        MaskCombinedHeader => vec!["masking.refinements"],
        MaskTwoPassHeader => vec!["masking.two_pass_sections"],
        // The mask recipe, collapsed to a line in Basic mode.
        Summary { .. } => vec!["masking.sections"],
        Separator | Note { .. } | MaskSectionHeader { .. } | RunButton | AdvancedFooter { .. } => vec![],
    }
}

fn section_of(page: Page) -> Option<Section> {
    Some(match page {
        Page::Geometry => Section::Geometry,
        Page::Masking => Section::Masking,
        Page::Motion => Section::Motion,
        // The schema puts the QSM switch with the inversion (also shown on QSM's other stages);
        // the TUI gives it a section of its own.
        Page::ComputeQsm => Section::Inversion,
        Page::Inversion => Section::Inversion,
        Page::FieldMap => Section::FieldMap,
        Page::Background => Section::Background,
        Page::MultiOrientation => Section::MultiOrientation,
        Page::Reference => Section::Reference,
        Page::Separation => Section::Separation,
        // One schema section, two TUI sections: SMWI needs χ, so it is in the QSM group.
        Page::Swi | Page::Smwi => Section::SwiSmwi,
        Page::Magnitude => Section::Magnitude,
        Page::Relaxometry => Section::Relaxometry,
        Page::Segmentation => Section::Segmentation,
        Page::Export => Section::Export,
        Page::Execution | Page::MethodsText | Page::Pipeline => return None,
    })
}

fn explore() -> impl Iterator<Item = (&'static Vec<Act>, App)> {
    explored().iter().map(|acts| (acts, build(acts)))
}

fn facts(app: &App) -> ViewFacts {
    ViewFacts { num_echoes: app.filter_state.num_echoes.trim().parse().ok(), multi_coil: None, ..Default::default() }
}

/// The settings the TUI shows now, with the sections it shows each in.
fn tui_shown(app: &App) -> BTreeMap<&'static str, BTreeSet<Section>> {
    let mut out: BTreeMap<&'static str, BTreeSet<Section>> = BTreeMap::new();
    // The Input tab's grouping, always there (not a row).
    out.entry("multi_orientation.group_by").or_default().insert(Section::Input);
    for (_, page, rows) in app.all_pages() {
        for r in &rows {
            for p in row_paths(app, r) {
                let e = out.entry(p).or_default();
                e.extend(section_of(page));
            }
        }
    }
    out
}

fn by_path() -> BTreeMap<&'static str, Setting> {
    schema::schema().into_iter().map(|s| (s.path, s)).collect()
}

#[test]
fn every_form_shows_the_settings_the_engine_says_apply() {
    let schema = by_path();
    assert!(explored().len() > 100, "explored only {} forms", explored().len());
    let mut failures = Vec::new();
    for (acts, app) in explore() {
        let config = config_from_app(&app).expect("the form builds a config");
        let shown = tui_shown(&app);
        let engine: BTreeSet<&str> = schema::settings_view(&config, &facts(&app))
            .into_iter().filter(|v| v.visible).map(|v| v.path).collect();
        let tui: BTreeSet<&str> = shown.keys().copied().collect();
        let only_tui: Vec<_> = tui.difference(&engine).collect();
        let only_engine: Vec<_> = engine.difference(&tui).collect();
        if !only_tui.is_empty() || !only_engine.is_empty() {
            failures.push(format!("after {acts:?}:\n  TUI only: {only_tui:?}\n  engine only: {only_engine:?}"));
        }
        for (p, sections) in &shown {
            let s = &schema[p];
            for sec in sections {
                if *sec != s.section && !s.also_in.contains(sec) {
                    failures.push(format!("{p} shows on {sec:?}, the schema puts it on {:?} {:?}", s.section, s.also_in));
                }
            }
        }
    }
    failures.dedup();
    assert!(failures.is_empty(), "{} mismatches, first ones:\n{}", failures.len(), failures[..failures.len().min(8)].join("\n"));
}

/// The mask recipe's parts: Basic mode shows the recipe as one line, or its whole editor once
/// it is edited by hand, rather than setting by setting.
const RECIPE: &[&str] = &["masking.sections", "masking.refinements", "masking.combine", "masking.two_pass_sections", "masking.two_pass_combine"];

#[test]
fn every_form_in_basic_mode_shows_the_settings_the_engine_says_basic_mode_shows() {
    let mut failures = Vec::new();
    for (acts, mut app) in explore() {
        app.advanced = false;
        let config = config_from_app(&app).expect("the form builds a config");
        let facts = app.view_facts();
        let base = schema::baseline(&config, &facts, &qsmxt_engine::PipelineConfig::default());
        let engine: BTreeSet<&str> = schema::regular_view(&config, &base, &facts).into_iter()
            .filter(|p| !RECIPE.contains(p)).collect();
        let tui: BTreeSet<&str> = tui_shown(&app).into_keys().filter(|p| !RECIPE.contains(p)).collect();
        let only_tui: Vec<_> = tui.difference(&engine).collect();
        let only_engine: Vec<_> = engine.difference(&tui).collect();
        if !only_tui.is_empty() || !only_engine.is_empty() {
            failures.push(format!("after {acts:?}:\n  TUI only: {only_tui:?}\n  engine only: {only_engine:?}"));
        }
    }
    failures.dedup();
    assert!(failures.is_empty(), "{} mismatches, first ones:\n{}", failures.len(), failures[..failures.len().min(8)].join("\n"));
}

/// Every setting has a row on some form the TUI reaches (what the flag-reachability test used to
/// ask of every flag): the mask builder, the Input tab and the reference picker have the ones
/// that are not rows of their own.
#[test]
fn every_setting_has_a_row() {
    let mut shown: BTreeSet<&str> = BTreeSet::new();
    for (_, app) in explore() {
        shown.extend(tui_shown(&app).into_keys());
    }
    // How a mask's sections combine shows once it has two, which the exploration does not add.
    let combine = ["masking.combine", "masking.two_pass_combine"];
    let missing: Vec<&str> = schema::settings().iter().map(|s| s.path)
        .filter(|p| !shown.contains(p) && !combine.contains(p)).collect();
    assert!(missing.is_empty(), "no TUI row shows {missing:?}");
}

/// The mask editor's step types are the mask schema's, and the ones it offers as a section's
/// first step are the ones that start one.
#[test]
fn the_mask_editors_steps_are_the_mask_schemas() {
    let m = schema::mask_schema();
    let step = |label: &str| m.steps.iter().find(|s| s.label == label).unwrap_or_else(|| panic!("{label} is not a mask step"));
    for t in MASK_GENERATOR_TYPES {
        assert!(step(t).starts_section, "{t}");
        // The default the editor adds is the schema's.
        let op = super::mask_builder::MaskRecipe::default_mask_op(t).unwrap();
        assert_eq!(serde_json::to_value(&op).unwrap(), step(t).default, "{t}");
    }
    for t in MASK_OP_TYPES {
        step(t);
    }
    for t in super::mask_builder::MaskRecipe::available_op_types() {
        assert!(!step(t).starts_section, "{t} is offered as a refinement");
    }
    // Every step type the schema has, the editor can make.
    for s in &m.steps {
        assert!(MASK_GENERATOR_TYPES.contains(&s.label) || MASK_OP_TYPES.contains(&s.label), "{}", s.label);
    }
    // Every input the schema has, the editor offers, in its order.
    let inputs: Vec<_> = m.inputs.iter().map(|i| i.value).collect();
    assert_eq!(inputs, super::mask_builder::MaskRecipe::MASK_INPUT_SOURCES);
}
