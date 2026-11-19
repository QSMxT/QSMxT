//! The TUI and the command line agree: the command the TUI prints parses (with `qsmxt`'s own
//! parser) back to the run it starts, and the lists the two offer are the same. Here rather than in
//! the TUI because the TUI has no command line.

use std::path::PathBuf;

use qsmxt_engine::schema::{self, ViewFacts};

use crate::tui::testutils::{build, build_command_string, build_slurm_args, config_from_app, explored, Act, App, DICOM_MAPS};

fn default_app() -> App {
    App::new()
}

fn explore() -> impl Iterator<Item = (&'static Vec<Act>, App)> {
    explored().iter().map(|acts| (acts, build(acts)))
}

fn facts(app: &App) -> ViewFacts {
    ViewFacts { num_echoes: app.filter_state.num_echoes.trim().parse().ok(), multi_coil: None, ..Default::default() }
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
        // The config the TUI's run takes: the outputs other settings need turned on, as the run
        // turns them on.
        let config = config_from_app(&app).unwrap().normalised().0;
        let cmd = qsmxt_engine::generate_command(&config);
        let argv: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
        let cli = parser.try_get_matches_from_mut(&argv).and_then(|m| crate::cli::Cli::from_arg_matches(&m))
            .unwrap_or_else(|e| panic!("{e}\n{cmd}"));
        let crate::cli::Command::Run(run) = cli.command else { panic!("expected run") };
        let mut rebuilt = qsmxt_engine::PipelineConfig::default();
        qsmxt_io::settings::apply_normalised(&mut rebuilt, &run.pipeline).unwrap();
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
        if fm.unwrapping_algorithm != qsmxt_engine::UnwrappingAlgorithm::Romeo {
            assert_eq!(fm.romeo.individual, qsmxt_engine::RomeoConfig::default().individual, "after {acts:?}");
        }
    }
    assert!(failures.is_empty(), "{} forms, e.g.:\n{}", failures.len(), failures[..failures.len().min(4)].join("\n"));
}


/// The command the TUI prints in SLURM mode carries the pipeline flags, so
/// `qsmxt slurm` must actually accept them — a copy-pasted command has to run.
#[test]
fn test_slurm_command_string_parses() {
    use clap::Parser;
    let mut app = default_app();
    app.form.bids_dir = "/bids".to_string();
    app.form.execution_mode = 1;
    app.form.slurm_account = "acct".to_string();
    app.set_qsm_algorithm(4); // tgv
    *app.field_text_mut("tgv_erosions").unwrap() = "2".into();

    let cmd = build_command_string(&app);
    let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
    let cli = crate::cli::Cli::try_parse_from(&argv)
        .unwrap_or_else(|e| panic!("TUI SLURM command did not parse: {}\ncmd: {}", e, cmd));
    match cli.command {
        crate::cli::Command::Slurm(args) => {
            let algorithm = args.pipeline.given.iter().find(|g| g.flag.long == "qsm-algorithm").map(|g| g.values.clone());
            assert_eq!(algorithm, Some(vec!["tgv".to_string()]));
            assert_eq!(args.account, "acct");
        }
        _ => panic!("expected the slurm subcommand"),
    }
}

/// Every execution setting that applies to a SLURM run reaches it: debug logging and DICOM
/// export's source directory and map list used to be local-only, dropped from the generated
/// jobs without a word. Checked on the arguments the TUI runs with and on the command it shows.
#[test]
fn slurm_mode_carries_debug_and_dicom_export() {
    use clap::Parser;
    let mut app = default_app();
    app.form.bids_dir = "/bids".to_string();
    app.form.execution_mode = 1;
    app.form.slurm_account = "acct".to_string();
    app.form.debug = true;
    app.form.export_dicom = true;
    app.form.source_dicom = "/dicom".to_string();
    // SWI makes swi and minIP; unticking minIP exports the magnitude, χ and SWI only.
    app.form.settings.set("pipeline.do_swi", true);
    app.form.dicom_unticked = vec!["minip".to_string()];

    let args = build_slurm_args(&app).unwrap();
    assert!(args.debug);
    assert_eq!(args.source_dicom, Some(PathBuf::from("/dicom")));
    assert_eq!(args.dicom_outputs, Some(vec!["magnitude".to_string(), "Chimap".to_string(), "swi".to_string()]));
    assert!(qsmxt_io::settings::build_config(Default::default(), &args.pipeline).unwrap().pipeline.export_dicom);

    let cmd = build_command_string(&app);
    let cli = crate::cli::Cli::try_parse_from(cmd.split_whitespace())
        .unwrap_or_else(|e| panic!("{e}\ncmd: {cmd}"));
    let crate::cli::Command::Slurm(shown) = cli.command else { panic!("not slurm: {cmd}") };
    assert_eq!((shown.debug, shown.source_dicom, shown.dicom_outputs),
               (args.debug, args.source_dicom, args.dicom_outputs), "{cmd}");

    // Without DICOM export its settings are not passed on.
    app.form.export_dicom = false;
    let args = build_slurm_args(&app).unwrap();
    assert_eq!((args.source_dicom, args.dicom_outputs), (None, None));
}


/// The checklist's maps are the ones `--dicom-outputs` accepts.
#[test]
fn the_checklist_and_the_cli_name_the_same_maps() {
    let mut ours: Vec<String> = DICOM_MAPS.iter().map(|m| m.suffix.to_lowercase()).collect();
    let mut cli: Vec<String> = crate::cli::DICOM_OUTPUT_TOKENS.iter().map(|t| t.to_string()).collect();
    ours.sort();
    cli.sort();
    assert_eq!(ours, cli);
}

/// `--dicom-outputs` takes one map per use or a comma list, and refuses unknown names.
#[test]
fn dicom_outputs_repeat_or_comma_separate() {
    use clap::Parser;
    let parse = |extra: &[&str]| {
        let mut argv = vec!["qsmxt", "run", "bids", "--export-dicom"];
        argv.extend_from_slice(extra);
        crate::cli::Cli::try_parse_from(argv)
    };
    for args in [&["--dicom-outputs", "chimap,swi"][..], &["--dicom-outputs", "Chimap", "--dicom-outputs", "SWI"][..]] {
        let crate::cli::Command::Run(r) = parse(args).unwrap().command else { panic!() };
        let got: Vec<String> = r.dicom_outputs.unwrap().iter().map(|s| s.to_lowercase()).collect();
        assert_eq!(got, ["chimap", "swi"], "{args:?}");
    }
    assert!(parse(&["--dicom-outputs", "chimapp"]).is_err(), "an unknown map is refused");
    // A space-separated list no longer swallows the BIDS directory.
    let argv = ["qsmxt", "run", "--export-dicom", "--dicom-outputs", "chimap", "bids"];
    let crate::cli::Command::Run(r) = crate::cli::Cli::try_parse_from(argv).unwrap().command else { panic!() };
    assert_eq!(r.bids_dir, std::path::PathBuf::from("bids"));
}

/// The CLI's `--mask-preset` values are `mask_presets()` (the TUI's preset row is checked against
/// the same list in the TUI).
#[test]
fn mask_preset_values_match_the_preset_list() {
    use clap::ValueEnum;
    let names: Vec<&str> = qsmxt_engine::mask_presets().iter().map(|(n, _)| *n).collect();
    let cli_names: Vec<String> = crate::cli::MaskPresetArg::value_variants().iter()
        .filter_map(|v| v.to_possible_value()).filter(|v| !v.is_hide_set())
        .map(|v| v.get_name().to_string()).collect();
    assert_eq!(cli_names, names, "--mask-preset values drifted from mask_presets()");
}
