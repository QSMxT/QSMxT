//! The engine's settings schema (`qsmxt_engine::schema`) agrees with the TUI: the same labels and
//! help, the same sub-tabs, and the same settings shown for the same configuration.
//!
//! The TUI's form is explored from a fresh one by changing one choice at a time (every option
//! of every select, every checkbox, an orientation grouping and an echo limit) until no new
//! rows appear. In each form reached, the settings the TUI shows must be exactly the ones
//! `settings_view` reports visible for the config the form builds.

use std::collections::{BTreeMap, BTreeSet};

use qsmxt_engine::schema::{self, Section, Setting, ValueType, ViewFacts};

use super::app::*;
use super::command::config_from_app;


/// Rows that edit no config setting of their own.
const NOT_CONFIG: &[&str] = &[
    // Expands into the mask recipe (masking.sections and friends), which the recipe rows edit.
    "mask_preset",
    // Run options of `qsmxt run`, not settings of the config.
    "source_dicom", "dicom_outputs", "execution_mode", "dry_run", "n_procs", "mem_limit_gb", "slurm_account",
    "slurm_partition", "slurm_time", "slurm_mem", "slurm_cpus", "slurm_submit", "debug", "force", "clean_intermediates",
];

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
            if NOT_CONFIG.contains(field) {
                return vec![];
            }
            let path = path_of(field).unwrap_or_else(|| panic!("TUI row {field} is not mapped to a schema setting"));
            // The TUI's reference picker lists the regions with the two methods.
            if *field == "qsm_reference" && app.field_select(field) >= 2 {
                return vec![path, "qsm.reference_region"];
            }
            vec![path]
        }
        MaskOpInput { section } | MaskOpGenerator { section } | MaskOpGeneratorParam { section }
        | MaskOpThresholdValue { section } | MaskOpHdBetStep { section } | MaskOpBetVoxelScale { section }
        | MaskOpEntry { section, .. } | MaskOpSignalErodeParam { section, .. } | MaskOpAddStep { section } => vec![recipe(*section)],
        MaskOpAddSection { two_pass } => vec![if *two_pass { "masking.two_pass_sections" } else { "masking.sections" }],
        MaskCombineOp => vec!["masking.combine"],
        MaskCombinedHeader => vec!["masking.refinements"],
        MaskTwoPassHeader => vec!["masking.two_pass_sections"],
        Separator | Note { .. } | MaskSectionHeader { .. } | RunButton => vec![],
    }
}

fn section_of(page: Page) -> Option<Section> {
    Some(match page {
        Page::Geometry => Section::Geometry,
        Page::Masking => Section::Masking,
        Page::Motion => Section::Motion,
        Page::QsmOff | Page::Inversion => Section::Inversion,
        Page::FieldMap => Section::FieldMap,
        Page::Background => Section::Background,
        Page::MultiOrientation => Section::MultiOrientation,
        Page::Reference => Section::Reference,
        Page::Separation => Section::Separation,
        Page::SwiSmwi => Section::SwiSmwi,
        Page::Relaxometry => Section::Relaxometry,
        Page::Segmentation => Section::Segmentation,
        Page::Export => Section::Export,
        Page::Execution => return None,
    })
}

/// One change to a form.
#[derive(Clone, Debug)]
enum Act {
    Toggle(&'static str),
    Select(&'static str, usize),
    /// An orientation grouping on the Input tab.
    Group,
    /// Limit the run to this many echoes.
    Echoes(usize),
}

fn build(acts: &[Act]) -> App {
    let mut app = App::new();
    for a in acts {
        match a {
            Act::Toggle(f) => app.toggle_field(f),
            Act::Select(f, i) => app.set_field_select(f, *i),
            Act::Group => app.filter_state.orientation_group = "acq".into(),
            Act::Echoes(n) => app.filter_state.num_echoes = n.to_string(),
        }
    }
    app.sync_pipeline_mode();
    app
}

/// Every form reached from a fresh one by changing one thing at a time, as long as each change
/// shows a row not seen before: the changes that make each (shortest first). Worked out once.
fn explored() -> &'static [Vec<Act>] {
    static FORMS: std::sync::OnceLock<Vec<Vec<Act>>> = std::sync::OnceLock::new();
    FORMS.get_or_init(|| {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut tried: BTreeSet<String> = BTreeSet::new();
        let mut queue: std::collections::VecDeque<Vec<Act>> =
            [vec![], vec![Act::Group], vec![Act::Echoes(1)], vec![Act::Group, Act::Echoes(1)]].into();
        let mut out = Vec::new();
        while let Some(acts) = queue.pop_front() {
            let app = build(&acts);
            let pages = app.all_pages();
            let mut new = acts.is_empty();
            for (_, page, rows) in &pages {
                for r in rows {
                    let key = match r {
                        PipelineRow::AlgoSelect { field, .. } | PipelineRow::Param { field, .. }
                        | PipelineRow::Toggle { field, .. } => format!("{page:?}|{field}"),
                        other => format!("{page:?}|{other:?}"),
                    };
                    new |= seen.insert(key);
                }
            }
            if !new {
                continue;
            }
            for (_, _, rows) in &pages {
                for r in rows {
                    let next: Vec<Act> = match *r {
                        PipelineRow::AlgoSelect { field, options, .. } if !NOT_CONFIG.contains(&field) || field == "mask_preset" =>
                            (0..options.len()).map(|i| Act::Select(field, i)).collect(),
                        PipelineRow::Toggle { field, .. } if !NOT_CONFIG.contains(&field) => vec![Act::Toggle(field)],
                        _ => vec![],
                    };
                    for a in next {
                        let mut n = acts.clone();
                        n.push(a);
                        if tried.insert(format!("{n:?}")) {
                            queue.push_back(n);
                        }
                    }
                }
            }
            out.push(acts);
        }
        out
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

/// Settings the schema words differently from their TUI row, and why.
const WORDED_DIFFERENTLY: &[(&str, &str)] = &[
    ("field_mapping.romeo.template", "the config counts echoes from 0, the TUI's row from 1"),
];

#[test]
fn every_setting_has_a_row_and_is_worded_as_the_tui_words_it() {
    let schema = by_path();
    // Every (label, help) each setting is shown with, anywhere.
    let mut worded: BTreeMap<&str, BTreeSet<(String, String)>> = BTreeMap::new();
    let mut options: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    for (_, app) in explore() {
        for (_, _, rows) in app.all_pages() {
            for r in &rows {
                let (field, label, help) = match r {
                    PipelineRow::Param { field, label, help } | PipelineRow::Toggle { field, label, help } => (*field, *label, help.to_string()),
                    PipelineRow::AlgoSelect { field, label, options: opts, help } => {
                        if let Some(p) = path_of(field) {
                            options.entry(p).or_insert_with(|| opts.iter().zip(help.iter())
                                .map(|(o, h)| (o.to_string(), h.to_string())).collect());
                        }
                        (*field, *label, String::new())
                    }
                    _ => continue,
                };
                if let Some(p) = path_of(field) {
                    worded.entry(p).or_default().insert((label.trim().to_string(), help));
                }
            }
        }
    }
    let mut failures = Vec::new();
    for (p, s) in &schema {
        if p.starts_with("masking.") && matches!(s.value_type, ValueType::MaskSections | ValueType::MaskOps)
            || *p == "masking.combine" || *p == "multi_orientation.group_by" || *p == "qsm.reference_region"
        {
            continue; // the mask builder, the Input tab and the reference picker: below
        }
        let Some(w) = worded.get(p) else {
            failures.push(format!("{p}: no TUI row shows it"));
            continue;
        };
        // The TUI's own help (the schema's `help` is worded for any front end).
        let main = (s.label.to_string(), s.tui_help.to_string());
        let value = s.tui_value.map(|(l, h)| (l.to_string(), h.to_string()));
        if !w.contains(&main) && !WORDED_DIFFERENTLY.iter().any(|(q, _)| q == p) {
            failures.push(format!("{p}: schema says {main:?}, the TUI {w:?}"));
        }
        if let Some(v) = value {
            if !w.contains(&v) {
                failures.push(format!("{p}: schema's value row says {v:?}, the TUI {w:?}"));
            }
        }
        let opts = s.value_type.options();
        if opts.is_empty() {
            continue;
        }
        let Some(tui) = options.get(p) else { continue };
        if *p == "qsm.reference" {
            // The TUI's picker: the two methods, then every region (the schema's region choice).
            let regions = schema["qsm.reference_region"].value_type.options();
            let picker: Vec<_> = opts.iter().take(2).chain(regions.iter()).collect();
            assert_eq!(picker.len(), tui.len(), "qsm.reference");
            for (o, (label, help)) in picker.iter().zip(tui) {
                if o.label != label || o.tui_help != *help {
                    failures.push(format!("{p}: option {} / {label}", o.label));
                }
            }
            continue;
        }
        if opts.len() != tui.len() {
            failures.push(format!("{p}: {} options, the TUI has {}", opts.len(), tui.len()));
            continue;
        }
        for (o, (label, help)) in opts.iter().zip(tui) {
            let help_matches = help.starts_with(o.tui_help.as_str()) && o.url.as_ref().is_none_or(|u| help.contains(u.as_str()));
            if o.label != label || !help_matches {
                failures.push(format!("{p}: option {:?} {:?} vs the TUI's {label:?} {help:?}", o.label, o.tui_help));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_tui_row_that_edits_the_config_is_a_setting() {
    let schema = by_path();
    for (field, path) in FIELD_PATHS {
        assert!(schema.contains_key(path), "{field} → {path} is not in the schema");
    }
    // And every row is mapped (row_paths panics on an unmapped one).
    for (_, app) in explore() {
        for (_, _, rows) in app.all_pages() {
            for r in &rows {
                row_paths(&app, r);
            }
        }
    }
}

#[test]
fn every_schema_option_is_a_qsmxt_run_option() {
    use clap::CommandFactory;
    let cli = crate::cli::Cli::command();
    let run = cli.get_subcommands().find(|c| c.get_name() == "run").expect("qsmxt run");
    let longs: BTreeSet<String> = run.get_arguments().filter_map(|a| a.get_long()).map(|l| format!("--{l}")).collect();
    for s in schema::schema() {
        if let Some(f) = s.cli_flag {
            assert!(longs.contains(f), "{}: {f} is not a qsmxt run option", s.path);
        }
    }
}

/// What [`schema::get`] reads at `path`, from a config already serialised.
fn value_at(config: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let obj = parent.split('.').filter(|k| !k.is_empty()).try_fold(config, |v, k| v.get(k))?;
    obj.as_object().map(|o| o.get(key).cloned().unwrap_or(serde_json::Value::Null))
}

/// The command the TUI prints runs what the TUI runs: parsed back into a config, it agrees with
/// the TUI's own config on every setting that applies. (Settings that do not apply are left out
/// of the command; they do not change the run.)
#[test]
fn the_printed_command_reproduces_the_tuis_run() {
    use clap::{CommandFactory, FromArgMatches};
    // Built once: building clap's command is most of the cost of a parse.
    let mut parser = crate::cli::Cli::command();
    parser.build();
    let settings = schema::schema();
    let mut failures = Vec::new();
    for (acts, app) in explore() {
        let config = config_from_app(&app).unwrap();
        let cmd = qsmxt_engine::generate_command(&config);
        let argv: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
        let cli = parser.try_get_matches_from_mut(&argv).and_then(|m| crate::cli::Cli::from_arg_matches(&m))
            .unwrap_or_else(|e| panic!("{e}\n{cmd}"));
        let crate::cli::Command::Run(run) = cli.command else { panic!("expected run") };
        let mut rebuilt = crate::pipeline::config::PipelineConfig::default();
        crate::pipeline::config::apply_run_overrides(&mut rebuilt, &run.pipeline).unwrap();
        let f = facts(&app);
        let (mut a, mut b) = (schema::prune_hidden(&config, &f), schema::prune_hidden(&rebuilt, &f));
        // An unset reliable mask means the main mask without its hole filling: compare the masks.
        for c in [&mut a, &mut b] {
            if c.masking.two_pass {
                c.masking.two_pass_sections = Some(c.masking.resolved_two_pass_sections());
            }
        }
        // `schema::get` of each setting, from one serialisation of each config.
        let (a, b) = (serde_json::to_value(&a).unwrap(), serde_json::to_value(&b).unwrap());
        let differ: Vec<_> = settings.iter()
            .filter(|s| value_at(&a, s.path) != value_at(&b, s.path)).map(|s| s.path).collect();
        if !differ.is_empty() {
            failures.push(format!("{differ:?} after {acts:?}\n  {cmd}"));
        }
        // The two leaks this used to allow: the TUI no longer passes either while it is hidden.
        let fm = &config.field_mapping;
        if fm.b0_estimation != qsmxt_engine::B0Estimation::WeightedAvg {
            assert_eq!(fm.b0_weight_type, qsmxt_engine::B0WeightType::PhaseSNR, "after {acts:?}");
        }
        if fm.unwrapping_algorithm == qsmxt_engine::UnwrappingAlgorithm::Laplacian {
            assert!(fm.romeo.individual, "after {acts:?}");
        }
    }
    assert!(failures.is_empty(), "{} forms, e.g.:\n{}", failures.len(), failures[..failures.len().min(4)].join("\n"));
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
        let op = PipelineFormState::default_mask_op(t).unwrap();
        assert_eq!(serde_json::to_value(&op).unwrap(), step(t).default, "{t}");
    }
    for t in MASK_OP_TYPES {
        step(t);
    }
    for t in PipelineFormState::default().available_op_types(0) {
        assert!(!step(t).starts_section, "{t} is offered as a refinement");
    }
    // Every step type the schema has, the editor can make.
    for s in &m.steps {
        assert!(MASK_GENERATOR_TYPES.contains(&s.label) || MASK_OP_TYPES.contains(&s.label), "{}", s.label);
    }
    // Every input the schema has, the editor offers, in its order.
    let inputs: Vec<_> = m.inputs.iter().map(|i| i.value).collect();
    assert_eq!(inputs, PipelineFormState::MASK_INPUT_SOURCES);
}
