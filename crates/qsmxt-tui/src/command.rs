use std::path::PathBuf;

use qsmxt_engine::schema::cli_apply::Given;

use qsmxt_engine::PipelineConfig;
use qsmxt_io::settings::RunSettings;
use super::app::App;

/// A local run the form starts: `qsmxt run`'s options, as plain data. The binary turns it into
/// its `qsmxt run` arguments and runs it.
#[derive(Debug)]
pub struct RunOptions {
    pub accept_recommendations: bool,
    pub accept_reference_settings: bool,
    pub bids_dir: PathBuf,
    pub output_dir: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub num_echoes: Option<usize>,
    pub n_procs: Option<usize>,
    pub source_dicom: Option<PathBuf>,
    pub dicom_outputs: Option<Vec<String>>,
    pub dry: bool,
    pub debug: bool,
    pub mem_limit_gb: Option<f64>,
    pub no_mem_limit: bool,
    pub force: bool,
    pub clean_intermediates: bool,
    pub no_dataset_steps: bool,
    pub pipeline: RunSettings,
}

/// A SLURM run the form starts: `qsmxt slurm`'s options, as plain data. The binary turns it into
/// its `qsmxt slurm` arguments and runs it.
#[derive(Debug)]
pub struct SlurmOptions {
    pub accept_recommendations: bool,
    pub accept_reference_settings: bool,
    pub bids_dir: PathBuf,
    pub output_dir: Option<PathBuf>,
    pub account: String,
    pub partition: Option<String>,
    pub config: Option<PathBuf>,
    pub time: String,
    pub mem: Option<usize>,
    pub cpus_per_task: usize,
    pub submit: bool,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub num_echoes: Option<usize>,
    pub force: bool,
    pub clean_intermediates: bool,
    pub debug: bool,
    pub source_dicom: Option<PathBuf>,
    pub dicom_outputs: Option<Vec<String>>,
    pub pipeline: RunSettings,
}

pub fn build_command_string(app: &App) -> String {
    let form = &app.form;
    let is_slurm = form.execution_mode == 1;
    let mut parts = vec![
        "qsmxt".to_string(),
        if is_slurm { "slurm".to_string() } else { "run".to_string() },
    ];

    // Positional args
    if form.bids_dir.is_empty() {
        parts.push("<bids_dir>".to_string());
    } else {
        parts.push(form.bids_dir.clone());
    }
    if !form.output_dir.is_empty() {
        parts.push(form.output_dir.clone());
    }

    // Config file
    if !form.config_file.is_empty() {
        parts.push(format!("--config {}", form.config_file));
    }

    // Filters (from tree selection / include/exclude patterns)
    let (include, exclude) = app.filter_state.get_include_exclude();
    if let Some(ref inc) = include {
        parts.push(format!("--include {}", inc.join(" ")));
    }
    if let Some(ref exc) = exclude {
        parts.push(format!("--exclude {}", exc.join(" ")));
    }
    if !app.filter_state.num_echoes.is_empty() {
        parts.push(format!("--num-echoes {}", app.filter_state.num_echoes));
    }

    // Pipeline flags — delegate to qsmxt-engine library
    match app.current_config().as_ref() {
        Err(e) => parts.push(format!("<invalid setting: {e}>")),
        Ok(config) => {
        let lib_cmd = qsmxt_engine::generate_command(config);
        // The library generates "qsmxt run <bids_dir> [flags]"
        // Strip the prefix and append only the flags
        let flags = lib_cmd.strip_prefix("qsmxt run <bids_dir>").unwrap_or(&lib_cmd).trim();
        if !flags.is_empty() {
            for flag in flags.split_whitespace() {
                parts.push(flag.to_string());
            }
        }
        }
    }

    // TUI-specific execution flags (not handled by the library)
    if is_slurm {
        // SLURM-specific flags
        if !form.slurm_account.trim().is_empty() {
            parts.push(format!("--account {}", form.slurm_account.trim()));
        } else {
            parts.push("--account <account>".to_string());
        }
        let slurm_defaults = super::app::RunForm::default();
        if !form.slurm_partition.trim().is_empty() {
            parts.push(format!("--partition {}", form.slurm_partition.trim()));
        }
        push_if_changed(&mut parts, "--time", &form.slurm_time, &slurm_defaults.slurm_time);
        if !form.slurm_mem.trim().is_empty() {
            parts.push(format!("--mem {}", form.slurm_mem.trim()));
        }
        push_if_changed(&mut parts, "--cpus-per-task", &form.slurm_cpus, &slurm_defaults.slurm_cpus);
        if form.slurm_submit {
            parts.push("--submit".to_string());
        }
        if form.force { parts.push("--force".to_string()); }
        if form.clean_intermediates { parts.push("--clean-intermediates".to_string()); }
        if form.debug { parts.push("--debug".to_string()); }
        push_dicom_export(&mut parts, app);
    } else {
        if form.dry_run {
            parts.push("--dry".to_string());
        }
        if form.debug {
            parts.push("--debug".to_string());
        }
        if !form.n_procs.trim().is_empty() {
            parts.push(format!("--n-procs {}", form.n_procs.trim()));
        }
        match form.mem_limit() {
            (Some(gb), _) => parts.push(format!("--mem-limit-gb {gb}")),
            (None, true) => parts.push("--no-mem-limit".to_string()),
            (None, false) => {}
        }
        if form.force { parts.push("--force".to_string()); }
        if form.clean_intermediates { parts.push("--clean-intermediates".to_string()); }
        push_dicom_export(&mut parts, app);
    }

    parts.join(" ")
}

/// DICOM export's run settings — the same in both modes (under SLURM each run's job exports).
fn push_dicom_export(parts: &mut Vec<String>, app: &App) {
    let form = &app.form;
    if form.export_dicom {
        if let Some(p) = parse_optional_path(&form.source_dicom) {
            parts.push(format!("--source-dicom {}", p.display()));
        }
        if let Some(o) = app.dicom_outputs() {
            parts.push(format!("--dicom-outputs {}", o.join(",")));
        }
    }
}

fn push_if_changed(parts: &mut Vec<String>, flag: &str, current: &str, default: &str) {
    if current.trim() != default.trim() {
        parts.push(format!("{} {}", flag, current.trim()));
    }
}

pub fn build_run_args(app: &App) -> qsmxt_io::Result<RunOptions> {
    let form = &app.form;
    if form.bids_dir.is_empty() {
        return Err(qsmxt_io::error::QsmxtError::Config(
            "BIDS directory is required".to_string(),
        ));
    }

    Ok(RunOptions {
        // The TUI resolves recommendations in its pop-up; the run applies none of its own.
        accept_recommendations: false,
        accept_reference_settings: false,
        bids_dir: expand_tilde(&form.bids_dir),
        output_dir: if form.output_dir.is_empty() { None } else { Some(expand_tilde(&form.output_dir)) },
        config: parse_optional_path(&form.config_file),
        include: app.filter_state.get_include_exclude().0,
        exclude: app.filter_state.get_include_exclude().1,
        num_echoes: typed("--num-echoes", &app.filter_state.num_echoes, "a whole number", count)?,
        n_procs: typed("--n-procs", &form.n_procs, "a whole number", count)?,
        source_dicom: if form.export_dicom { parse_optional_path(&form.source_dicom) } else { None },
        dicom_outputs: if form.export_dicom { app.dicom_outputs() } else { None },
        dry: form.dry_run,
        debug: form.debug,
        // A number of GB, or `none` for no limit (blank: auto).
        mem_limit_gb: typed("--mem-limit-gb", &form.mem_limit_gb, "a number of GB above 0, or none", |t| {
            if t.eq_ignore_ascii_case("none") { Some(None) } else { number(t).filter(|g| *g > 0.0).map(Some) }
        })?.flatten(),
        no_mem_limit: form.mem_limit().1,
        force: form.force,
        clean_intermediates: form.clean_intermediates,
        // The worker mode is what `qsmxt slurm` generates; the TUI's local run is a whole run.
        no_dataset_steps: false,
        pipeline: settings_args(app)?,
    })
}

/// The settings the form passes by rules of its own rather than while they apply, as it always
/// has: the inversion and MEDI's SMV mode always; HEIDI while ticked, but not for an orientation
/// group; the orientation grouping; background removal while the inversion has that stage and it
/// is not the default; QSM's switch and DICOM export while not the default. The form's rule for
/// which stages run, the inversion picker and QSM's own page read these.
fn form_values(app: &App) -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;
    let st = &app.form.settings;
    let group = app.filter_state.orientation_group_by();
    let mut out = vec![("inversion.algorithm", st.get("inversion.algorithm"))];
    if st.toggle("inversion.heidi.enabled") && group.is_none() {
        out.push(("inversion.heidi.enabled", json!(true)));
    }
    if let Some(g) = group {
        out.push(("multi_orientation.group_by", json!(g)));
    }
    // Only when the inversion has a background-removal stage: otherwise the choice has no
    // effect, and iQFM there is refused (#276).
    let bf = st.get("bg_removal.algorithm");
    if app.inversion_needs_bg_removal() && bf != super::settings_rows::setting("bg_removal.algorithm").default {
        out.push(("bg_removal.algorithm", bf));
    }
    out.push(("inversion.medi.smv", json!(st.toggle("inversion.medi.smv"))));
    if !st.toggle("pipeline.do_qsm") {
        out.push(("pipeline.do_qsm", json!(false)));
    }
    if app.form.export_dicom {
        out.push(("pipeline.export_dicom", json!(true)));
    }
    out
}

/// The recommendations dismissed in the pop-up, as `--dismiss-recommendation`: the one flag the
/// form passes, which keeps the run's checks of a dismissal and adds to a config file's.
fn dismissals(app: &App) -> Vec<Given> {
    let dismissed = &app.form.dismissed;
    if dismissed.is_empty() {
        return Vec::new();
    }
    let flag = qsmxt_engine::schema::cli::PIPELINE_FLAGS.iter().find(|f| f.long == "dismiss-recommendation")
        .expect("qsmxt run has --dismiss-recommendation");
    vec![Given { flag, values: dismissed.iter().map(|d| format!("{}={}", d.rule, d.chosen)).collect() }]
}

pub(super) fn expand_tilde(s: &str) -> PathBuf {
    let home = || std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("~"));
    if s == "~" {
        home()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(s)
    }
}

fn parse_optional_path(s: &str) -> Option<PathBuf> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(expand_tilde(trimmed))
    }
}

#[allow(dead_code)]
fn parse_optional_string_vec(s: &str) -> Option<Vec<String>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.split_whitespace().map(String::from).collect())
    }
}

/// The value of text typed on a row the TUI builds by hand: `None` when blank, which leaves the
/// run's default, else what it reads as. Text that reads as nothing is an error naming the flag
/// and the text, as the command line refuses it: the run never goes ahead with something other
/// than what was typed.
fn typed<T>(flag: &str, text: &str, what: &str, read: impl Fn(&str) -> Option<T>) -> qsmxt_io::Result<Option<T>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    read(t).map(Some).ok_or_else(|| qsmxt_io::error::QsmxtError::Config(format!("{flag} {t}: not {what}")))
}

fn number(t: &str) -> Option<f64> {
    t.parse().ok().filter(|x: &f64| x.is_finite())
}

fn count(t: &str) -> Option<usize> {
    t.parse().ok()
}

pub fn build_slurm_args(app: &App) -> qsmxt_io::Result<SlurmOptions> {
    let form = &app.form;
    if form.bids_dir.is_empty() {
        return Err(qsmxt_io::error::QsmxtError::Config(
            "BIDS directory is required".to_string(),
        ));
    }
    if form.slurm_account.trim().is_empty() {
        return Err(qsmxt_io::error::QsmxtError::Config(
            "SLURM account is required".to_string(),
        ));
    }

    let defaults = super::app::RunForm::default();
    let (include, exclude) = app.filter_state.get_include_exclude();
    Ok(SlurmOptions {
        accept_recommendations: false,
        accept_reference_settings: false,
        bids_dir: expand_tilde(&form.bids_dir),
        output_dir: if form.output_dir.is_empty() { None } else { Some(expand_tilde(&form.output_dir)) },
        account: form.slurm_account.trim().to_string(),
        partition: if form.slurm_partition.trim().is_empty() { None } else { Some(form.slurm_partition.trim().to_string()) },
        config: parse_optional_path(&form.config_file),
        time: if form.slurm_time.trim().is_empty() { defaults.slurm_time.clone() } else { form.slurm_time.trim().to_string() },
        mem: typed("--mem", &form.slurm_mem, "a whole number of GB", count)?,
        // Blank is `qsmxt slurm`'s default.
        cpus_per_task: typed("--cpus-per-task", &form.slurm_cpus, "a whole number", count)?.unwrap_or(4),
        submit: form.slurm_submit,
        include,
        exclude,
        num_echoes: typed("--num-echoes", &app.filter_state.num_echoes, "a whole number", count)?,
        force: form.force,
        clean_intermediates: form.clean_intermediates,
        debug: form.debug,
        source_dicom: if form.export_dicom { parse_optional_path(&form.source_dicom) } else { None },
        dicom_outputs: if form.export_dicom { app.dicom_outputs() } else { None },
        pipeline: settings_args(app)?,
    })
}

/// The methods text (Markdown) for the current settings, or why the settings are invalid.
pub fn methods_text(app: &App) -> Result<String, String> {
    app.current_config().as_ref().as_ref().map(qsmxt_io::pipeline::methods::generate_methods).map_err(|e| e.clone())
}

/// The settings of the form's settings store the run takes: those changed from their defaults
/// that apply. One left behind by an earlier choice (hidden now) is left at its default, so that
/// it does not change the run.
fn stored_values(app: &App, own: &[(&'static str, serde_json::Value)], given: &[Given]) -> qsmxt_io::Result<Vec<(&'static str, serde_json::Value)>> {
    let settings = &app.form.settings;
    let (changed, refused) = settings.changed_or_refused();
    if changed.is_empty() && refused.is_empty() {
        return Ok(changed);
    }
    // Which apply is decided on the form's whole config. Should the checks refuse a stored
    // setting (multi-orientation's, say, of a hidden COSMOS tolerance), the form's own settings
    // are written first, so that the checks see only the stored settings that apply, as the run
    // does.
    let config = match with_form(settings.config_of(&changed), own, given) {
        Ok(config) => config,
        Err(_) => super::settings_rows::with_values(&with_form(PipelineConfig::default(), own, given)?, &changed),
    };
    let visible = app.visible_settings(&config);
    // Typed text the run cannot take is an error while its setting applies, as on the command
    // line: the run is refused rather than run with something else.
    if let Some((_, why)) = refused.iter().find(|(p, _)| visible.contains(p)) {
        return Err(qsmxt_io::error::QsmxtError::Config(why.clone()));
    }
    changed.into_iter().filter(|(p, _)| visible.contains(p))
        .map(|(p, v)| Ok((p, as_spelled(p, v)?)))
        .collect()
}

/// `config` with the form's own settings ([`form_values`]) and its flags, as given (no outputs
/// turned on for other settings).
fn with_form(mut config: PipelineConfig, own: &[(&'static str, serde_json::Value)], given: &[Given]) -> qsmxt_io::Result<PipelineConfig> {
    for (path, value) in own {
        qsmxt_engine::schema::set(&mut config, path, value.clone()).map_err(qsmxt_io::error::QsmxtError::Config)?;
    }
    qsmxt_io::settings::apply(&mut config, given)?;
    Ok(config)
}

/// A mask recipe as the run has always taken it from the form: spelled as the command line
/// spells it (`--mask`, `--mask-refine`, `--two-pass-mask`) and read back. The spelling rounds
/// some numbers (a fixed threshold to 4 places, BET's fractional intensity to 2) and gives a fixed
/// or percentile threshold without a value its default; a step the command line cannot take is
/// refused as it would be there. Any other setting is as it is. Kept until the form's settings
/// are all the config's.
fn as_spelled(path: &str, value: serde_json::Value) -> qsmxt_io::Result<serde_json::Value> {
    use qsmxt_io::pipeline::config::{hole_closing_warnings, parse_mask_sections, MaskOp, MaskSection};
    let unreadable = |e: serde_json::Error| qsmxt_io::error::QsmxtError::Config(format!("{path}: {e}"));
    let spell = |sections: &[MaskSection]| -> Vec<String> {
        sections.iter().map(|section| {
            std::iter::once(format!("{}", section.input))
                .chain(section.all_ops().iter().map(|op| format!("{op}")))
                .collect::<Vec<_>>()
                .join(",")
        }).collect()
    };
    match path {
        "masking.sections" => {
            let sections: Vec<MaskSection> = serde_json::from_value(value).map_err(unreadable)?;
            serde_json::to_value(parse_mask_sections(&spell(&sections), "--mask")?).map_err(unreadable)
        }
        "masking.two_pass_sections" => {
            let Some(sections) = serde_json::from_value::<Option<Vec<MaskSection>>>(value).map_err(unreadable)? else {
                return Ok(serde_json::Value::Null);
            };
            let sections = parse_mask_sections(&spell(&sections), "--two-pass-mask")?;
            for warning in hole_closing_warnings(&sections) {
                log::warn!("{warning}");
            }
            serde_json::to_value(Some(sections)).map_err(unreadable)
        }
        "masking.refinements" => {
            let ops: Vec<MaskOp> = serde_json::from_value(value).map_err(unreadable)?;
            let mut read = Vec::new();
            for op in ops {
                let s = format!("{op}");
                let op = qsmxt_engine::masking::parse_mask_op(&s)
                    .map_err(|e| qsmxt_io::error::QsmxtError::Config(format!("--mask-refine {s}: {e}")))?;
                if op.is_generator() {
                    return Err(qsmxt_io::error::QsmxtError::Config(format!(
                        "--mask-refine {op}: it creates a mask rather than refining one; put it \
                         in a --mask section instead")));
                }
                read.push(op);
            }
            serde_json::to_value(read).map_err(unreadable)
        }
        _ => Ok(value),
    }
}

/// The run's settings from the form: its own settings ([`form_values`]) and those it keeps in a
/// config ([`stored_values`]), by path, and its dismissals as the flag.
fn settings_args(app: &App) -> qsmxt_io::Result<RunSettings> {
    let (own, given) = (form_values(app), dismissals(app));
    let values = stored_values(app, &own, &given)?;
    Ok(RunSettings { values: own.into_iter().chain(values).collect(), given })
}

/// The config the form builds (methods preview, generated command): the settings it keeps in a
/// config ([`stored_values`]), then its own ([`form_values`]) and its dismissals, which are what
/// the run takes ([`settings_args`]), so the preview, the shown command and the run cannot drift
/// apart. An error is a setting the run would refuse (an unparseable mask section, say); the TUI
/// shows it in place of the command and the methods rather than previewing some other
/// configuration. As the form gives them: an
/// output another setting needs and the form leaves off is a required change for the TUI to
/// offer ([`qsmxt_engine::normalise::required_findings`]), which the run would otherwise make
/// itself.
pub fn config_from_app(app: &App) -> qsmxt_io::Result<PipelineConfig> {
    let (own, given) = (form_values(app), dismissals(app));
    let config = app.form.settings.config_of(&stored_values(app, &own, &given)?);
    let mut config = with_form(config, &own, &given)?;
    // The pipeline chosen on the Pipeline tab, while the form still makes its choices.
    config.reference_pipeline = app.form.reference_pipeline.clone()
        .filter(|id| qsmxt_engine::pipelines::chosen(&config, id));
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::app::App;

    /// The config a run gets from its arguments.
    fn config(args: &RunOptions) -> PipelineConfig {
        qsmxt_io::settings::build_config(Default::default(), &args.pipeline).unwrap()
    }

    /// The value the form passes the run for the setting at `path`, if it passes one.
    fn passed(app: &App, path: &str) -> Option<serde_json::Value> {
        settings_args(app).unwrap().values.into_iter().find(|(p, _)| *p == path).map(|(_, v)| v)
    }

    /// Each entry of the SWI selects runs the unwrapping or scaling it shows (#266:
    /// `negative-tanh` used to run `tanh`).
    #[test]
    fn swi_selects_run_what_they_show() {
        use qsmxt_engine::{SwiScaling, SwiUnwrap};
        for (i, &expected) in SwiUnwrap::ALL.iter().enumerate() {
            let mut app = App::new();
            app.form.settings.set("pipeline.do_swi", true);
            app.set_field_select("swi_unwrap", i);
            assert_eq!(config_from_app(&app).unwrap().swi.unwrap, expected, "row {i}");
        }
        for (i, &expected) in SwiScaling::ALL.iter().enumerate() {
            let mut app = App::new();
            app.form.settings.set("pipeline.do_swi", true);
            app.set_field_select("swi_scaling", i);
            assert_eq!(config_from_app(&app).unwrap().swi.scaling, expected, "row {i}");
        }
        // The first entry is the default.
        assert_eq!(SwiUnwrap::ALL[0], qsmxt_engine::SwiConfig::default().unwrap);
    }

    /// Every row of the inversion list must survive being selected.
    ///
    /// `config_from_app` used to carry its own copy of the algorithm list; when `lsqr` and `heidi`
    /// were added to `QSM_ALGO_OPTIONS` the copy stayed 29 long, so scrolling onto the last two
    /// rows panicked with "index out of bounds" and took the whole TUI down (issue #225). Walking
    /// the full list here catches any future drift at the only place that matters — what the row
    /// actually runs.
    #[test]
    fn every_inversion_row_builds_the_algorithm_it_shows() {
        use crate::app::QSM_ALGO_OPTIONS;
        for (i, name) in QSM_ALGO_OPTIONS.iter().enumerate() {
            let mut app = default_app();
            app.set_qsm_algorithm(i);
            let config = config_from_app(&app).unwrap();
            assert_eq!(&format!("{}", config.inversion.algorithm), name,
                       "row {i} shows {name} but configures {}", config.inversion.algorithm);
            assert_eq!(passed(&app, "inversion.algorithm"), Some(serde_json::json!(name)),
                       "row {i} shows {name} but passes a different algorithm");
        }
    }

    /// R2PRIMEnet's field-strength override and patch reach both the config and the run — when
    /// R2' is computed; otherwise they have nothing to act on and are not passed.
    #[test]
    fn r2primenet_settings_reach_config_and_args() {
        let mut app = default_app();
        app.toggle_field("r2primenet_ignore_field_mismatch");
        *app.field_text_mut("r2primenet_patch").unwrap() = "128 128 64".into();
        assert_eq!(config_from_app(&app).unwrap().separation.r2primenet, qsmxt_engine::R2primenetConfig::default(),
                   "no R2', nothing passed");

        let mut app = default_app();
        app.form.settings.set("pipeline.do_r2primemap", true);
        let d = qsmxt_engine::R2primenetConfig::default();
        assert_eq!(config_from_app(&app).unwrap().separation.r2primenet, d);
        app.toggle_field("r2primenet_ignore_field_mismatch");
        *app.field_text_mut("r2primenet_patch").unwrap() = "128 128 64".into();
        let c = config_from_app(&app).unwrap();
        assert!(c.separation.r2primenet.ignore_field_mismatch);
        assert_eq!(c.separation.r2primenet.patch, [128, 128, 64]);
        app.form.bids_dir = "/b".into();
        let run = qsmxt_io::settings::build_config(Default::default(), &build_run_args(&app).unwrap().pipeline).unwrap();
        assert_eq!(run.separation.r2primenet, c.separation.r2primenet);
    }

    /// V-SHARP's working grid: the engine's by default (off the command), spacing row only for
    /// isotropic.
    #[test]
    fn vsharp_working_grid() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::Param { field, .. } | crate::app::PipelineRow::AlgoSelect { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(fields(&app).contains(&"vsharp_grid".to_string()));
        let d = qsmxt_engine::VsharpConfig::default();
        assert_eq!(config_from_app(&app).unwrap().bg_removal.vsharp.grid, d.grid);
        let command = |app: &App| qsmxt_engine::generate_command(&config_from_app(app).unwrap());
        assert!(!command(&app).contains("--vsharp-grid "));
        app.set_field_select("vsharp_grid", 0);
        assert!(!fields(&app).contains(&"vsharp_grid_spacing".to_string()));
        assert_eq!(config_from_app(&app).unwrap().bg_removal.vsharp.grid, qsmxt_engine::VsharpGrid::Native);
        assert!(command(&app).contains("--vsharp-grid native"));
        app.set_field_select("vsharp_grid", 1);
        assert_eq!(config_from_app(&app).unwrap().bg_removal.vsharp.grid, qsmxt_engine::VsharpGrid::Finest);
        assert!(!fields(&app).contains(&"vsharp_grid_spacing".to_string()));
        app.set_field_select("vsharp_grid", 2);
        assert!(fields(&app).contains(&"vsharp_grid_spacing".to_string()));
        *app.field_text_mut("vsharp_grid_spacing").unwrap() = "0.8".into();
        let c = config_from_app(&app).unwrap();
        assert_eq!((c.bg_removal.vsharp.grid, c.bg_removal.vsharp.grid_spacing), (qsmxt_engine::VsharpGrid::Isotropic, 0.8));
        assert!(!command(&app).contains("--vsharp-grid "), "isotropic is the default");
        assert!(command(&app).contains("--vsharp-grid-spacing 0.8"));
    }

    /// V-SHARP's deconvolution is off by default, its threshold row only shows when it is on,
    /// and the toggle reaches both the config and the command.
    #[test]
    fn vsharp_deconvolve_toggle() {
        let mut app = default_app();
        let shows_threshold = |app: &App| app.pipeline_rows().iter().any(|r|
            matches!(r, crate::app::PipelineRow::Param { field: "vsharp_threshold", .. }));
        assert!(!config_from_app(&app).unwrap().bg_removal.vsharp.deconvolve);
        assert!(!shows_threshold(&app));
        app.toggle_field("vsharp_deconvolve");
        assert!(shows_threshold(&app));
        assert!(config_from_app(&app).unwrap().bg_removal.vsharp.deconvolve);
        assert!(qsmxt_engine::generate_command(&config_from_app(&app).unwrap()).contains("--vsharp-deconvolve"));
    }

    /// The two-pass mode is offered with two-pass, and reaches both the config and the command.
    #[test]
    fn two_pass_mode_setting() {
        let mut app = default_app();
        let shown = |app: &App| app.pipeline_rows().iter()
            .any(|r| matches!(r, crate::app::PipelineRow::AlgoSelect { field: "two_pass_mode", .. }));
        assert!(!shown(&app));
        app.toggle_field("two_pass");
        assert!(shown(&app));
        let command = |app: &App| qsmxt_engine::generate_command(&config_from_app(app).unwrap());
        assert!(!command(&app).contains("--two-pass-mode"), "the default stays off the command");
        assert_eq!(config_from_app(&app).unwrap().masking.two_pass_mode, qsmxt_engine::TwoPassMode::Local);
        app.set_field_select("two_pass_mode", 1);
        assert_eq!(config_from_app(&app).unwrap().masking.two_pass_mode, qsmxt_engine::TwoPassMode::Original);
        assert!(command(&app).contains("--two-pass-mode original"));
    }

    /// The Laplacian solver and its pad are in the TUI (they used to be CLI-only): the rows
    /// follow the selection, and the values reach both the config and the CLI args.
    #[test]
    fn inhomogeneity_method_and_n4_settings() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::AlgoSelect { field, .. }
            | crate::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(fields(&app).contains(&"inhomogeneity_method".to_string()));
        assert!(fields(&app).contains(&"homogeneity_sigma_mm".to_string()));
        assert!(!fields(&app).contains(&"n4_shrink_factor".to_string()), "makehomogeneous has no N4 rows");
        let command = |app: &App| qsmxt_engine::generate_command(&config_from_app(app).unwrap());
        assert!(!command(&app).contains("--inhomogeneity-method"), "default stays off the command");
        app.set_field_select("inhomogeneity_method", 1);
        assert!(fields(&app).contains(&"n4_max_iterations".to_string()));
        assert!(!fields(&app).contains(&"homogeneity_sigma_mm".to_string()));
        *app.field_text_mut("n4_max_iterations").unwrap() = "20 10".into();
        *app.field_text_mut("n4_mesh").unwrap() = "2 2 1".into();
        *app.field_text_mut("n4_shrink_factor").unwrap() = "2".into();
        *app.field_text_mut("homogeneity_sigma_mm").unwrap() = "3".into();
        assert!(fields(&app).contains(&"n4_geometry".to_string()));
        assert_eq!(config_from_app(&app).unwrap().masking.n4_geometry, qsmxt_engine::N4Geometry::Scanner);
        app.set_field_select("n4_geometry", 1);
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.masking.n4_geometry, qsmxt_engine::N4Geometry::MakeNii);
        assert_eq!(c.masking.inhomogeneity_method, qsmxt_engine::InhomogeneityMethod::N4);
        assert_eq!((c.n4.max_iterations.clone(), c.n4.mesh, c.n4.shrink_factor), (vec![20, 10], [2, 2, 1], 2));
        assert!(command(&app).contains("--inhomogeneity-method n4"));
        assert_eq!(c.homogeneity, qsmxt_engine::HomogeneityConfig::default(), "makehomogeneous's settings stay off with N4");
        app.toggle_field("inhomogeneity_correction");
        assert!(!fields(&app).contains(&"inhomogeneity_method".to_string()));
        assert_eq!(config_from_app(&app).unwrap().n4, qsmxt_engine::N4Config::default(), "off: N4's settings do not apply");
    }

    #[test]
    fn laplacian_solver_and_pad() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::AlgoSelect { field, .. }
            | crate::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        let command = |app: &App| qsmxt_engine::generate_command(&config_from_app(app).unwrap());
        assert!(!fields(&app).contains(&"laplacian_solver".to_string()), "ROMEO has no solver");
        app.set_field_select("unwrapping_algorithm", 1); // laplacian
        assert!(fields(&app).contains(&"laplacian_solver".to_string()));
        assert!(!fields(&app).contains(&"laplacian_fft_pad".to_string()), "dct has no pad");
        assert!(!command(&app).contains("--laplacian-solver"), "default stays off the command");
        app.set_field_select("laplacian_solver", 1); // fft
        assert!(fields(&app).contains(&"laplacian_fft_pad".to_string()));
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.laplacian_solver, qsmxt_engine::LaplacianSolver::Fft);
        assert_eq!(c.field_mapping.laplacian_fft_pad, [64; 3]);
        assert!(!command(&app).contains("--laplacian-fft-pad"), "default pad not emitted");
        *app.field_text_mut("laplacian_fft_pad").unwrap() = "64 64 32".into();
        assert_eq!(config_from_app(&app).unwrap().field_mapping.laplacian_fft_pad, [64, 64, 32]);
        assert!(command(&app).contains("--laplacian-solver fft") && command(&app).contains("--laplacian-fft-pad 64 64 32"), "{}", command(&app));
        // QSM.jl's MGPCG: no pad, reaches the config and the command.
        let mgpcg = super::super::settings_rows::options("field_mapping.laplacian_solver").0.iter().position(|o| *o == "mgpcg").unwrap();
        app.set_field_select("laplacian_solver", mgpcg);
        assert!(!fields(&app).contains(&"laplacian_fft_pad".to_string()), "mgpcg has no pad");
        assert_eq!(config_from_app(&app).unwrap().field_mapping.laplacian_solver, qsmxt_engine::LaplacianSolver::Mgpcg);
        assert!(command(&app).contains("--laplacian-solver mgpcg") && !command(&app).contains("--laplacian-fft-pad"));
    }

    /// The unmasked unwrapping is a Laplacian toggle, as the solver is: off by default and
    /// reaching the config and the command when on.
    #[test]
    fn prelude_settings() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::AlgoSelect { field, .. }
            | crate::app::PipelineRow::Param { field, .. }
            | crate::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(!fields(&app).contains(&"prelude_labelling".to_string()), "ROMEO has no PRELUDE settings");
        let command = |app: &App| qsmxt_engine::generate_command(&config_from_app(app).unwrap());
        app.set_field_select("unwrapping_algorithm", 2); // prelude
        let f = fields(&app);
        for shown in ["prelude_labelling", "prelude_partitions", "phase_offset_removal"] {
            assert!(f.contains(&shown.to_string()), "{shown}: {f:?}");
        }
        assert!(!f.iter().any(|f| f.starts_with("romeo_") || f.starts_with("laplacian_solver")), "{f:?}");
        assert!(command(&app).contains("--unwrapping-algorithm prelude"));
        assert!(!command(&app).contains("--prelude-"), "defaults stay off the command");
        app.set_field_select("prelude_labelling", 2);
        *app.field_text_mut("prelude_partitions").unwrap() = "6".into();
        app.toggle_field("bipolar_correction");
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.unwrapping_algorithm, qsmxt_engine::UnwrappingAlgorithm::Prelude);
        assert_eq!(c.field_mapping.prelude.labelling, qsmxt_engine::PreludeLabelling::Volume);
        assert_eq!(c.field_mapping.prelude.num_phase_partitions, 6);
        assert!(c.field_mapping.bipolar_correction, "bipolar correction applies with PRELUDE");
        // Back to ROMEO: PRELUDE's settings are not passed.
        app.set_field_select("unwrapping_algorithm", 0);
        assert!(!command(&app).contains("--prelude-"));
    }

    /// MEDI's region growing is the fourth unwrapper: it has no settings of its own, keeps phase
    /// offset removal and bipolar correction as ROMEO does, and reaches the command and config.
    #[test]
    fn medi_region_growing_choice() {
        let mut app = default_app();
        app.set_field_select("unwrapping_algorithm", 3); // medi-region-growing
        let f: Vec<String> = app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::AlgoSelect { field, .. }
            | crate::app::PipelineRow::Param { field, .. }
            | crate::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect();
        assert!(f.contains(&"phase_offset_removal".to_string()), "{f:?}");
        assert!(!f.iter().any(|f| f.starts_with("romeo_") || f.starts_with("prelude_") || f.starts_with("laplacian_")), "{f:?}");
        app.toggle_field("bipolar_correction");
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.unwrapping_algorithm, qsmxt_engine::UnwrappingAlgorithm::MediRegionGrowing);
        assert!(c.field_mapping.bipolar_correction);
    }

    #[test]
    fn laplacian_unmasked_toggle() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".into();
        let toggles = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(!toggles(&app).contains(&"laplacian_unmasked".to_string()), "ROMEO has no Laplacian unwrapping");
        app.form.settings.set("field_mapping.laplacian_unmasked", true);
        assert!(!config_from_app(&app).unwrap().field_mapping.laplacian_unmasked, "not passed under ROMEO");
        app.form.settings.set("field_mapping.laplacian_unmasked", false);
        app.set_field_select("unwrapping_algorithm", 1); // laplacian
        assert!(toggles(&app).contains(&"laplacian_unmasked".to_string()));
        assert!(!config_from_app(&app).unwrap().field_mapping.laplacian_unmasked);
        assert!(!build_command_string(&app).contains("--laplacian-unmasked"));
        app.toggle_field("laplacian_unmasked");
        assert!(config_from_app(&app).unwrap().field_mapping.laplacian_unmasked);
        assert!(build_command_string(&app).contains("--laplacian-unmasked"));
    }

    /// MEDI's complex fit, MEDI+0's CSF term with MEDI's zero padding, and TFI's MERIT and data
    /// term are in the TUI and reach the config; the complex fit hides phase offset removal.
    #[test]
    fn complex_fit_medi_csf_and_tfi_reach_the_config() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(crate::app::row_field)
            .map(str::to_string).collect::<Vec<_>>();
        assert!(fields(&app).contains(&"phase_offset_removal".to_string()));
        app.form.settings.set("field_mapping.b0_estimation", "complex-fit");
        let f = fields(&app);
        assert!(f.contains(&"complex_fit_reltol".to_string()) && !f.contains(&"phase_offset_removal".to_string())
            && !f.contains(&"linear_fit_estimate_offset".to_string()), "{f:?}");
        let c = config_from_app(&app).unwrap();
        assert_eq!((c.field_mapping.b0_estimation, c.field_mapping.complex_fit.clone()),
                   (qsmxt_engine::B0Estimation::ComplexFit, qsmxt_engine::ComplexFitConfig::default()));
        *app.field_text_mut("complex_fit_max_iter").unwrap() = "5".into();
        assert_eq!(config_from_app(&app).unwrap().field_mapping.complex_fit.max_iter, 5);

        app.set_qsm_algorithm(crate::app::QSM_ALGO_OPTIONS.iter().position(|o| *o == "medi").unwrap());
        assert!(fields(&app).contains(&"medi_lambda_csf".to_string()));
        *app.field_text_mut("medi_lambda_csf").unwrap() = "50".into();
        *app.field_text_mut("medi_zeropad").unwrap() = "2 0 2".into();
        app.toggle_field("medi_csf_erode");
        let m = config_from_app(&app).unwrap().inversion.medi;
        assert_eq!((m.csf, m.lambda_csf, m.csf_erode, m.zeropad), (true, 50.0, false, [2, 0, 2]));
        app.toggle_field("medi_csf");
        assert!(!fields(&app).contains(&"medi_lambda_csf".to_string()));
        assert!(!config_from_app(&app).unwrap().inversion.medi.csf);

        app.set_qsm_algorithm(crate::app::QSM_ALGO_OPTIONS.iter().position(|o| *o == "tfi").unwrap());
        app.toggle_field("tfi_merit");
        app.toggle_field("tfi_linear");
        let t = config_from_app(&app).unwrap().inversion.tfi;
        assert_eq!((t.merit, t.linear), (false, false));
    }

    /// The robust fit, its parameters and the shared fit settings are in the TUI and reach both
    /// the config and the command.
    #[test]
    fn robust_fit_settings_reach_config_and_args() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::AlgoSelect { field, .. }
            | crate::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(!fields(&app).contains(&"robust_fit_loss".to_string()));
        app.set_field_select("b0_estimation", 2);
        let f = fields(&app);
        for name in ["robust_fit_loss", "robust_fit_tuning", "robust_fit_iterations",
                     "robust_fit_downweight_level", "robust_fit_flag_ratio"] {
            assert!(f.contains(&name.to_string()), "{name} missing: {f:?}");
        }
        assert_eq!(config_from_app(&app).unwrap().field_mapping.robust_fit, qsmxt_engine::RobustFitConfig::default());
        app.set_field_select("robust_fit_loss", 1);
        for (field, text) in [("robust_fit_tuning", "2"), ("robust_fit_iterations", "0"),
                              ("robust_fit_downweight_level", "0.5"), ("robust_fit_flag_ratio", "4")] {
            *app.field_text_mut(field).unwrap() = text.into();
        }
        app.toggle_field("linear_fit_estimate_offset");
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.b0_estimation, qsmxt_engine::B0Estimation::RobustFit);
        assert_eq!(c.field_mapping.robust_fit, qsmxt_engine::RobustFitConfig {
            loss: qsmxt_engine::RobustLoss::Huber, tuning: Some(2.0), iterations: 0,
            downweight_level: 0.5, flag_ratio: 4.0,
        });
        assert!(!c.field_mapping.linear_fit.estimate_offset);
        let command = qsmxt_engine::generate_command(&c);
        for flag in ["--b0-estimation robust-fit", "--robust-fit-loss huber", "--robust-fit-tuning 2",
                     "--robust-fit-iterations 0", "--linear-fit-estimate-offset false"] {
            assert!(command.contains(flag), "{flag}: {command}");
        }
    }

    #[test]
    fn ignore_slice_gap_reaches_config_and_args() {
        let mut app = default_app();
        assert!(!config_from_app(&app).unwrap().pipeline.ignore_slice_gap);
        app.toggle_field("ignore_slice_gap");
        assert!(config_from_app(&app).unwrap().pipeline.ignore_slice_gap);
        assert!(qsmxt_io::settings::build_config(Default::default(), &build_run_args(&{
            let mut a = App::new(); a.form.bids_dir = "/b".into(); a.toggle_field("ignore_slice_gap"); a
        }).unwrap().pipeline).unwrap().pipeline.ignore_slice_gap);
        let mut c = config_from_app(&app).unwrap();
        assert!(qsmxt_engine::generate_command(&c).contains("--ignore-slice-gap"));
        c.pipeline.ignore_slice_gap = false;
        assert!(!qsmxt_engine::generate_command(&c).contains("--ignore-slice-gap"));
    }

    /// Motion correction and every registration setting reach the config and the CLI args, and
    /// its rows only appear once it is on.
    #[test]
    fn motion_correction_settings_reach_config_and_args() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::app::PipelineRow::AlgoSelect { field, .. }
            | crate::app::PipelineRow::Param { field, .. }
            | crate::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(fields(&app).contains(&"motion_correction".to_string()));
        assert!(!fields(&app).contains(&"reg_levels".to_string()),
                "registration serves motion correction and orientation groups, and neither is on");
        assert_eq!(config_from_app(&app).unwrap().motion_correction, qsmxt_engine::MotionCorrectionConfig::default());
        app.toggle_field("motion_correction");
        app.set_field_select("motion_reference", 2);
        *app.field_text_mut("motion_reference_echo").unwrap() = "2".into();
        app.toggle_field("motion_chain");
        *app.field_text_mut("motion_min_ncc").unwrap() = "0.9".into();
        *app.field_text_mut("reg_levels").unwrap() = "4".into();
        *app.field_text_mut("reg_min_overlap").unwrap() = "0.5".into();
        let f = fields(&app);
        for name in ["motion_reference", "motion_reference_echo", "motion_chain", "motion_min_ncc",
                     "reg_levels", "reg_max_samples", "reg_max_iterations", "reg_max_evaluations",
                     "reg_tolerance", "reg_rotation_step_deg", "reg_translation_step_mm", "reg_min_overlap"] {
            assert!(f.contains(&name.to_string()), "{name} missing: {f:?}");
        }
        assert!(!f.contains(&"motion_chain_levels".to_string()), "chain levels hidden when chain is off");
        let m = config_from_app(&app).unwrap().motion_correction;
        assert!(m.enabled && !m.chain);
        assert_eq!(m.reference, qsmxt_engine::MotionReference::Echo(2));
        let reg = config_from_app(&app).unwrap().registration;
        assert_eq!((m.min_ncc, reg.levels, reg.min_overlap), (0.9, 4, 0.5));
        app.form.bids_dir = "/b".into();
        let run = qsmxt_io::settings::build_config(Default::default(), &build_run_args(&app).unwrap().pipeline).unwrap();
        assert_eq!((run.motion_correction, run.registration), (m, reg));
    }

    /// Multi-orientation co-registration and force are in the TUI; the registration settings
    /// show while either registration can run.
    /// The TUI cannot produce iQFM (or any BFR) with an inversion that has no background-removal
    /// stage: the section is hidden and no `--bf-algorithm` is generated (#276). Which those are
    /// comes from QSM.rs (#265), so TFI is among them.
    #[test]
    fn background_removal_is_hidden_and_not_sent_for_total_field_inversions() {
        use crate::app::{PipelineRow, QSM_ALGO_OPTIONS};
        // Every option parses as a config algorithm, or the check would silently say "needs BFR".
        for name in QSM_ALGO_OPTIONS {
            assert!(serde_json::from_value::<qsmxt_engine::QsmAlgorithm>(serde_json::json!(name)).is_ok(), "{name}");
        }
        let mut app = default_app();
        let shows_bfr = |app: &App| app.pipeline_rows().iter().any(|r|
            matches!(r, PipelineRow::AlgoSelect { field: "bf_algorithm", .. }));
        assert!(shows_bfr(&app));
        // The default goes unsent; another one is sent.
        assert!(passed(&app, "bg_removal.algorithm").is_none());
        app.set_field_select("bf_algorithm", 1);
        assert!(passed(&app, "bg_removal.algorithm").is_some());
        for name in ["tfi", "tgv", "qsmart", "autoqsm", "nextqsm", "iqsm"] {
            app.set_qsm_algorithm(QSM_ALGO_OPTIONS.iter().position(|o| *o == name).unwrap());
            assert!(!shows_bfr(&app), "{name}");
            assert!(passed(&app, "bg_removal.algorithm").is_none(), "{name}");
        }
    }

    #[test]
    fn orientation_registration_and_force_reach_config_and_args() {
        let mut app = default_app();
        // Without a grouping they act on nothing: not passed on.
        app.toggle_field("orientation_registration");
        app.toggle_field("multi_orientation_force");
        let c = config_from_app(&app).unwrap();
        assert!(c.multi_orientation.register && !c.multi_orientation.force);

        let mut app = default_app();
        app.filter_state.orientation_group = "acq".into();
        app.sync_pipeline_mode();
        let c = config_from_app(&app).unwrap();
        assert!(c.multi_orientation.register && !c.multi_orientation.force);
        app.toggle_field("orientation_registration");
        app.toggle_field("multi_orientation_force");
        let c = config_from_app(&app).unwrap();
        assert!(!c.multi_orientation.register && c.multi_orientation.force);
        app.form.bids_dir = "/b".into();
        let run = qsmxt_io::settings::build_config(Default::default(), &build_run_args(&app).unwrap().pipeline).unwrap();
        assert!(!run.multi_orientation.register && run.multi_orientation.force);
        let shows_reg = |app: &App| app.pipeline_rows().iter().any(|r|
            matches!(r, crate::app::PipelineRow::Param { field: "reg_levels", .. }));
        assert!(!shows_reg(&app), "neither registration can run");
        app.toggle_field("motion_correction");
        assert!(shows_reg(&app));
        // Co-registering a group's orientations runs it too.
        app.toggle_field("motion_correction");
        app.toggle_field("orientation_registration");
        assert!(shows_reg(&app));
        app.filter_state.orientation_group.clear();
        assert!(!shows_reg(&app), "no group, nothing to co-register");
    }

    /// The inner-inversion choice belongs to QSMART and to nothing else. It used to be gated on a
    /// hardcoded row index, which `lsqr` and `heidi` shifted out from under it — QSMART silently
    /// lost its inner inversion, and LSQR picked one up.
    #[test]
    fn qsmart_inner_inversion_follows_qsmart_not_a_row_index() {
        use crate::app::QSM_ALGO_OPTIONS;
        let qsmart = QSM_ALGO_OPTIONS.iter().position(|o| *o == "qsmart").expect("qsmart");
        for (i, name) in QSM_ALGO_OPTIONS.iter().enumerate() {
            let mut app = default_app();
            app.set_qsm_algorithm(i);
            app.form.settings.set("inversion.qsmart.inversion", "rts");
            let set = build_command_string(&app).contains("--qsmart-inversion rts");
            assert_eq!(set, i == qsmart, "qsmart_inversion set for {name} (row {i})");
        }
    }

    fn default_app() -> App {
        App::new()
    }

    /// With a config file named on the form, the run takes the file's settings and the form's on
    /// top: those the form changed and that apply, the inversion and MEDI's SMV mode always,
    /// background removal and QSM's switch only when not the default. Any other setting the file
    /// gives stays the file's.
    #[test]
    fn the_forms_settings_apply_on_top_of_a_config_file() {
        use qsmxt_engine::{BfAlgorithm, QsmAlgorithm};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.toml");
        let smv = !qsm_core::inversion::MediParams::default().smv;
        std::fs::write(&path, format!(
            "[pipeline]\ndo_swi = true\n\n[swi]\nstrength = 3.0\n\n[inversion]\nalgorithm = \"tkd\"\n\n\
             [inversion.medi]\nsmv = {smv}\n\n[bg_removal]\nalgorithm = \"pdf\"\n\n[bg_removal.vsharp]\nmax_radius = 9.0\n")).unwrap();
        let mut app = default_app();
        app.form.bids_dir = "/bids".into();
        app.form.config_file = path.display().to_string();
        let run = |app: &App| {
            let args = build_run_args(app).unwrap();
            assert_eq!(args.config.as_deref(), Some(path.as_path()));
            qsmxt_io::settings::build_config(qsmxt_io::pipeline::config::load_config(&path).unwrap(), &args.pipeline).unwrap()
        };
        let c = run(&app);
        assert_eq!(c.inversion.algorithm, QsmAlgorithm::Ilsqr, "the form's inversion, always");
        assert_eq!(c.inversion.medi.smv, !smv, "the form's SMV mode, always");
        assert_eq!(c.bg_removal.algorithm, BfAlgorithm::Pdf, "the form's default background removal is not passed");
        assert!(c.pipeline.do_swi && c.swi.strength == 3.0, "settings the form left are the file's");
        assert_eq!(c.bg_removal.vsharp.max_radius, 9.0);
        assert!(c.pipeline.do_qsm);

        app.form.settings.set("pipeline.do_r2starmap", true);
        app.set_field_select("bf_algorithm", crate::app::BF_OPTIONS.iter().position(|o| *o == "lbv").unwrap());
        app.toggle_field("do_qsm");
        let c = run(&app);
        assert!(c.pipeline.do_r2starmap, "a setting the form changed is the form's");
        assert_eq!(c.bg_removal.algorithm, BfAlgorithm::Lbv);
        assert!(!c.pipeline.do_qsm);
        assert!(c.pipeline.do_swi && c.swi.strength == 3.0);
    }

    /// The "off" of a setting is a checkbox now, not a magic value typed into it: each one
    /// emits what the CLI means by off, and nothing until it is changed.
    #[test]
    fn checkboxes_replace_the_magic_off_values() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".into();
        let base = build_command_string(&app);
        for f in ["--axial-resampling", "--obliquity-threshold", "--romeo-temporal-uncertain-unwrapping",
                  "--tile-size", "--synthseg-sigma"] {
            assert!(!base.contains(f), "{f} in a default command: {base}");
        }
        app.form.settings.set("pipeline.axial_resampling", true);
        assert!(build_command_string(&app).contains("--axial-resampling"));
        *app.field_text_mut("obliquity_threshold").unwrap() = "20".into();
        assert!(build_command_string(&app).contains("--obliquity-threshold 20"));
        app.form.settings.set("pipeline.axial_resampling", false);
        assert!(!build_command_string(&app).contains("--obliquity-threshold"), "no threshold without resampling");

        // Off is ROMEO's default here; turned on, the threshold shown is passed.
        app.toggle_field("romeo_temporal_uncertain");
        app.form.settings.switch("segmentation.sigma_smoothing", false);
        app.form.settings.set("pipeline.do_segmentation", true);
        let cmd = build_command_string(&app);
        for f in ["--romeo-temporal-uncertain-unwrapping 0.5", "--synthseg-sigma 0"] {
            assert!(cmd.contains(f), "{f} missing: {cmd}");
        }
        app.set_qsm_algorithm(crate::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "xqsm").unwrap());
        assert!(!build_command_string(&app).contains("--tile-size"));
        app.toggle_field("dl_tiling");
        assert!(build_command_string(&app).contains("--tile-size 128"));

        app.set_qsm_algorithm(crate::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "medi").unwrap());
        assert_eq!(app.field_select("medi_data_weighting"), 1, "magnitude SNR by default");
        app.set_field_select("medi_data_weighting", 0);
        assert!(build_command_string(&app).contains("--medi-data-weighting 0"));
    }

    // --- build_command_string ---

    #[test]
    fn test_command_string_minimal() {
        let app = default_app();
        let cmd = build_command_string(&app);
        assert!(cmd.starts_with("qsmxt run"));
        assert!(cmd.contains("<bids_dir>"));
        assert!(!cmd.contains("<output_dir>"));
    }

    #[test]
    fn test_command_string_with_dirs() {
        let mut app = default_app();
        app.form.bids_dir = "/data/bids".to_string();
        app.form.output_dir = "/data/out".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("/data/bids"));
        assert!(cmd.contains("/data/out"));
        assert!(!cmd.contains("<bids_dir>"));
    }

    #[test]
    fn test_command_string_reflects_chi_separation_edits() {
        let mut app = default_app();
        // Enabling chi-separation + selecting decompose + editing a param must all show up.
        app.form.settings.set("pipeline.do_chi_separation", true);
        app.set_field_select("separation_algorithm", 1); // decompose
        *app.field_text_mut("sep_decompose_max_iterations").unwrap() = "77".into();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--do-chisep"));
        assert!(cmd.contains("--chisep decompose"));
        assert!(cmd.contains("--decompose-max-iterations 77"));
    }

    #[test]
    fn test_command_string_with_config() {
        let mut app = default_app();
        app.form.config_file = "my.toml".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--config my.toml"));
    }

    #[test]
    fn test_command_string_num_echoes() {
        let mut app = default_app();
        app.filter_state.num_echoes = "4".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--num-echoes 4"));
    }


    #[test]
    fn test_command_string_parameters() {
        let mut app = default_app();
        *app.field_text_mut("ilsqr_tol").unwrap() = "0.01".into();
        *app.field_text_mut("ilsqr_pad_mm").unwrap() = "32".into();
        app.form.settings.set("pipeline.axial_resampling", true);
        *app.field_text_mut("obliquity_threshold").unwrap() = "5".into();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--ilsqr-tol 0.01"));
        assert!(cmd.contains("--ilsqr-pad-mm 32"));
        assert!(cmd.contains("--obliquity-threshold 5"));
    }

    #[test]
    fn test_command_string_no_defaults_shown() {
        let app = default_app();
        let cmd = build_command_string(&app);
        // With no changes, only positional bids_dir should appear (output_dir is optional)
        assert!(cmd.starts_with("qsmxt run <bids_dir>"));
        assert!(!cmd.contains("--rts-delta"));
        assert!(!cmd.contains("--qsm-algorithm"));
        assert!(!cmd.contains("--n-procs"));
        assert!(!cmd.contains("--mask-op"));
    }

    #[test]
    fn test_command_string_phase_offset_removal() {
        let mut app = default_app();
        app.toggle_field("phase_offset_removal");
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--phase-offset-removal false"));
    }

    #[test]
    fn test_command_string_execution_flags() {
        let mut app = default_app();
        app.form.settings.set("pipeline.do_swi", true);
        app.form.settings.set("pipeline.do_t2starmap", true);
        app.form.settings.set("pipeline.do_r2starmap", true);
        app.form.dry_run = true;
        app.form.debug = true;
        app.form.n_procs = "4".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--do-swi"));
        assert!(cmd.contains("--do-t2starmap"));
        assert!(cmd.contains("--do-r2starmap"));
        // inhomogeneity_correction is true by default, so shouldn't appear
        assert!(!cmd.contains("--inhomogeneity-correction"));
        assert!(cmd.contains("--dry"));
        assert!(cmd.contains("--debug"));
        assert!(cmd.contains("--n-procs 4"));
    }


    // --- build_run_args ---

    #[test]
    fn test_build_run_args_error_when_empty() {
        let app = default_app();
        let result = build_run_args(&app);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_run_args_minimal() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.form.output_dir = "/out".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.bids_dir, PathBuf::from("/bids"));
        assert_eq!(args.output_dir, Some(PathBuf::from("/out")));
        assert_eq!(config(&args).inversion.algorithm, qsmxt_engine::QsmAlgorithm::Ilsqr);
    }

    #[test]
    fn test_build_run_args_num_echoes() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.filter_state.num_echoes = "4".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.num_echoes, Some(4));
    }

    #[test]
    fn test_build_run_args_numeric_params() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.set_qsm_algorithm(0); // rts
        *app.field_text_mut("rts_delta").unwrap() = "0.2".into();
        *app.field_text_mut("rts_mu").unwrap() = "1e5".into();
        *app.field_text_mut("rts_tol").unwrap() = "1e-4".into();
        app.form.n_procs = "8".to_string();
        let args = build_run_args(&app).unwrap();
        let rts = config(&args).inversion.rts;
        assert_eq!((rts.delta, rts.mu, rts.tol), (0.2, 1e5, 1e-4));
        assert_eq!(args.n_procs, Some(8));
        app.set_qsm_algorithm(4); // tgv
        *app.field_text_mut("tgv_iterations").unwrap() = "500".into();
        *app.field_text_mut("tgv_erosions").unwrap() = "2".into();
        let tgv = config(&build_run_args(&app).unwrap()).inversion.tgv;
        assert_eq!((tgv.iterations, tgv.erosions), (Some(500), 2));
    }

    #[test]
    fn test_build_run_args_flags() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.form.settings.set("pipeline.do_swi", true);
        app.form.settings.set("pipeline.do_t2starmap", true);
        app.form.settings.set("pipeline.do_r2starmap", true);
        app.form.settings.set("masking.inhomogeneity_correction", true);
        app.form.dry_run = true;
        app.form.debug = true;
        let args = build_run_args(&app).unwrap();
        let config = |args: &RunOptions| qsmxt_io::settings::build_config(Default::default(), &args.pipeline).unwrap();
        assert!(config(&args).pipeline.do_swi);
        assert!(!config(&args).swi.filtered_phase);
        assert!(config(&args).pipeline.do_t2starmap);
        assert!(config(&args).pipeline.do_r2starmap);
        assert!(config(&args).masking.inhomogeneity_correction);
        assert!(args.dry);
        app.form.settings.set("swi.filtered_phase", true);
        assert!(config(&build_run_args(&app).unwrap()).swi.filtered_phase);
        // Ticked under SWI and SWI then turned off: the setting no longer applies, so is not passed.
        app.form.settings.set("pipeline.do_swi", false);
        let off = config(&build_run_args(&app).unwrap());
        assert!(!off.swi.filtered_phase && !off.pipeline.do_swi);
        assert!(args.debug);
        assert!(config(&args).field_mapping.phase_offset_removal); // default mcpc3ds
    }

    #[test]
    fn test_build_run_args_phase_offset_removal_disabled() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.toggle_field("phase_offset_removal");
        let args = build_run_args(&app).unwrap();
        assert!(!config(&args).field_mapping.phase_offset_removal);
    }



    #[test]
    fn test_build_run_args_config_file() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.form.config_file = "pipeline.toml".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.config, Some(PathBuf::from("pipeline.toml")));
    }


    #[test]
    fn test_build_run_args_obliquity() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.form.settings.set("pipeline.axial_resampling", true);
        *app.field_text_mut("obliquity_threshold").unwrap() = "5.0".into();
        let args = build_run_args(&app).unwrap();
        let config = qsmxt_io::settings::build_config(Default::default(), &args.pipeline).unwrap();
        assert_eq!(config.pipeline.obliquity_threshold, 5.0);
    }

    // --- parse helpers ---

    #[test]
    fn test_parse_optional_path_empty() {
        assert_eq!(parse_optional_path(""), None);
        assert_eq!(parse_optional_path("  "), None);
    }

    #[test]
    fn test_parse_optional_path_value() {
        assert_eq!(parse_optional_path("/foo"), Some(PathBuf::from("/foo")));
        assert_eq!(parse_optional_path("  /bar  "), Some(PathBuf::from("/bar")));
    }

    #[test]
    fn test_parse_optional_string_vec_empty() {
        assert_eq!(parse_optional_string_vec(""), None);
        assert_eq!(parse_optional_string_vec("   "), None);
    }

    #[test]
    fn test_parse_optional_string_vec_values() {
        assert_eq!(
            parse_optional_string_vec("a b c"),
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

#[test]
    fn test_push_if_changed_same() {
        let mut parts = vec![];
        push_if_changed(&mut parts, "--flag", "val", "val");
        assert!(parts.is_empty());
    }

    #[test]
    fn test_push_if_changed_different() {
        let mut parts = vec![];
        push_if_changed(&mut parts, "--flag", "new", "old");
        assert_eq!(parts, vec!["--flag new"]);
    }

    // --- SLURM command string ---

    #[test]
    fn test_command_string_slurm_mode() {
        let mut app = default_app();
        app.form.execution_mode = 1; // SLURM
        app.form.bids_dir = "/bids".to_string();
        app.form.slurm_account = "myacct".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.starts_with("qsmxt slurm"));
        assert!(cmd.contains("--account myacct"));
    }

    #[test]
    fn test_command_string_slurm_all_fields() {
        let mut app = default_app();
        app.form.execution_mode = 1;
        app.form.bids_dir = "/bids".to_string();
        app.form.slurm_account = "acct".to_string();
        app.form.slurm_partition = "gpu".to_string();
        app.form.slurm_time = "04:00:00".to_string();
        app.form.slurm_mem = "64".to_string();
        app.form.slurm_cpus = "8".to_string();
        app.form.slurm_submit = true;
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--account acct"));
        assert!(cmd.contains("--partition gpu"));
        assert!(cmd.contains("--time 04:00:00"));
        assert!(cmd.contains("--mem 64"));
        assert!(cmd.contains("--cpus-per-task 8"));
        assert!(cmd.contains("--submit"));
    }

    #[test]
    fn test_command_string_slurm_defaults_omitted() {
        let mut app = default_app();
        app.form.execution_mode = 1;
        app.form.bids_dir = "/bids".to_string();
        app.form.slurm_account = "acct".to_string();
        // Default time/mem/cpus should not appear
        let cmd = build_command_string(&app);
        assert!(!cmd.contains("--time"));
        assert!(!cmd.contains("--mem"));
        assert!(!cmd.contains("--cpus-per-task"));
        assert!(!cmd.contains("--submit"));
        // Local-only flags should not appear
        assert!(!cmd.contains("--dry"));
        assert!(!cmd.contains("--n-procs"));
    }

    #[test]
    fn test_command_string_slurm_no_account_placeholder() {
        let mut app = default_app();
        app.form.execution_mode = 1;
        app.form.bids_dir = "/bids".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--account <account>"));
    }

    // --- build_slurm_args ---

    #[test]
    fn test_build_slurm_args_error_no_bids() {
        let app = default_app();
        assert!(build_slurm_args(&app).is_err());
    }

    #[test]
    fn test_build_slurm_args_error_no_account() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        assert!(build_slurm_args(&app).is_err());
    }

    /// Type `text` with `edit` and check the config is refused with `message`.
    fn refused(app: &mut App, edit: impl FnOnce(&mut App), message: &str) {
        edit(app);
        let err = config_from_app(app).expect_err(message).to_string();
        assert!(err.contains(message), "{err}");
        assert!(build_run_args(app).is_err(), "the run is refused too");
    }

    /// BET's rows refuse unreadable text while a mask step runs BET; blank is BET's default.
    #[test]
    fn unreadable_bet_value_is_refused() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".into();
        *app.field_text_mut("bet_smoothness").unwrap() = "abc".into();
        assert!(config_from_app(&app).is_ok(), "no BET step: it does not apply");
        app.set_field_select("mask_preset", 1);
        assert!(app.mask().sections.iter().any(|s| matches!(s.generator, qsmxt_engine::MaskOp::Bet { .. })));
        refused(&mut app, |_| {}, "--bet-smoothness abc: not a number");
        *app.field_text_mut("bet_smoothness").unwrap() = String::new();
        assert!(config_from_app(&app).is_ok(), "blank is the default");
        refused(&mut app, |a| *a.field_text_mut("bet_iterations").unwrap() = "1.5".into(), "--bet-iterations 1.5: not a whole number");
    }

    /// The homogeneity correction's rows refuse unreadable text while it is the method chosen.
    #[test]
    fn unreadable_homogeneity_value_is_refused() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".into();
        assert!(app.field_toggle("inhomogeneity_correction"));
        assert_eq!(app.field_select("inhomogeneity_method"), 0);
        refused(&mut app, |a| *a.field_text_mut("homogeneity_nbox").unwrap() = "x".into(), "--homogeneity-nbox x: not a whole number");
        app.toggle_field("inhomogeneity_correction");
        assert!(config_from_app(&app).is_ok(), "correction off: it does not apply");
    }

    /// N4's rows refuse unreadable text while N4 is the method chosen.
    #[test]
    fn unreadable_n4_value_is_refused() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".into();
        app.set_field_select("inhomogeneity_method", 1);
        refused(&mut app, |a| *a.field_text_mut("n4_mesh").unwrap() = "1 2".into(), "--n4-mesh 1 2: not three whole numbers");
        app.set_field_select("inhomogeneity_method", 0);
        assert!(config_from_app(&app).is_ok(), "the homogeneity method: N4's mesh does not apply");
    }

    /// The run options refuse unreadable text rather than run with the default; blank is the
    /// default.
    #[test]
    fn unreadable_run_options_are_refused() {
        let err = |r: qsmxt_io::Result<RunOptions>| r.expect_err("refused").to_string();
        let mut app = default_app();
        app.form.bids_dir = "/bids".into();
        app.form.n_procs = "abc".into();
        assert!(err(build_run_args(&app)).contains("--n-procs abc: not a whole number"));
        app.form.n_procs = String::new();
        app.form.mem_limit_gb = "lots".into();
        assert!(err(build_run_args(&app)).contains("--mem-limit-gb lots: not a number of GB above 0, or none"));
        app.form.mem_limit_gb = "0".into();
        assert!(build_run_args(&app).is_err(), "0 GB is no limit anyone means");
        app.form.mem_limit_gb = "none".into();
        assert!(build_run_args(&app).unwrap().no_mem_limit);
        app.filter_state.num_echoes = "two".into();
        assert!(err(build_run_args(&app)).contains("--num-echoes two: not a whole number"));
        app.filter_state.num_echoes = String::new();

        app.form.slurm_account = "acct".into();
        app.form.slurm_cpus = "eight".into();
        let e = build_slurm_args(&app).expect_err("refused").to_string();
        assert!(e.contains("--cpus-per-task eight: not a whole number"), "{e}");
        app.form.slurm_cpus = String::new();
        assert_eq!(build_slurm_args(&app).unwrap().cpus_per_task, 4, "blank is the default");
        app.form.slurm_mem = "64G".into();
        let e = build_slurm_args(&app).expect_err("refused").to_string();
        assert!(e.contains("--mem 64G: not a whole number of GB"), "{e}");
    }

    #[test]
    fn test_build_slurm_args_minimal() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.form.slurm_account = "acct".to_string();
        let args = build_slurm_args(&app).unwrap();
        assert_eq!(args.bids_dir, PathBuf::from("/bids"));
        assert_eq!(args.account, "acct");
        assert_eq!(args.output_dir, None);
        assert_eq!(args.partition, None);
        assert_eq!(args.time, "02:00:00");
        assert_eq!(args.mem, None, "empty: each job sized from the memory estimate");
        assert_eq!(args.cpus_per_task, 4);
        assert!(!args.submit);
        assert!(!args.force && !args.clean_intermediates);
    }

    #[test]
    fn test_build_slurm_args_full() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.form.output_dir = "/out".to_string();
        app.form.slurm_account = "acct".to_string();
        app.form.slurm_partition = "gpu".to_string();
        app.form.slurm_time = "04:00:00".to_string();
        app.form.slurm_mem = "64".to_string();
        app.form.slurm_cpus = "8".to_string();
        app.form.slurm_submit = true;
        app.form.config_file = "config.toml".to_string();
        app.form.force = true;
        app.form.clean_intermediates = true;
        let args = build_slurm_args(&app).unwrap();
        assert!(args.force && args.clean_intermediates, "force and clean-up reach the SLURM jobs");
        app.form.execution_mode = 1;
        let shown = build_command_string(&app);
        assert!(shown.contains("--force") && shown.contains("--clean-intermediates") && shown.contains("--mem 64"), "{shown}");
        assert_eq!(args.output_dir, Some(PathBuf::from("/out")));
        assert_eq!(args.partition, Some("gpu".to_string()));
        assert_eq!(args.time, "04:00:00");
        assert_eq!(args.mem, Some(64));
        assert_eq!(args.cpus_per_task, 8);
        assert!(args.submit);
        assert_eq!(args.config, Some(PathBuf::from("config.toml")));
    }

    /// Regression: SLURM mode used to drop every pipeline setting, so the
    /// generated jobs ran the defaults (romeo/threshold/sharp/rts) instead of
    /// the configured pipeline. The SLURM args must carry the same pipeline
    /// settings as the local args.
    #[test]
    fn test_build_slurm_args_carries_pipeline_settings() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.form.slurm_account = "acct".to_string();
        // TKD rather than TGV: TGV has no background-removal stage, so the TUI sends none (#276).
        app.set_qsm_algorithm(2); // tkd
        app.set_field_select("unwrapping_algorithm", 1); // laplacian
        app.set_field_select("bf_algorithm", 1); // pdf
        app.form.settings.set("pipeline.axial_resampling", true);
        *app.field_text_mut("obliquity_threshold").unwrap() = "5".into();
        app.form.settings.set("pipeline.do_swi", true);

        let slurm = build_slurm_args(&app).unwrap();
        let config = qsmxt_io::settings::build_config(Default::default(), &slurm.pipeline).unwrap();
        assert_eq!(config.inversion.algorithm, qsmxt_engine::QsmAlgorithm::Tkd);
        assert_eq!(config.bg_removal.algorithm, qsmxt_engine::BfAlgorithm::Pdf);
        assert_eq!(config.field_mapping.unwrapping_algorithm, qsmxt_engine::UnwrappingAlgorithm::Laplacian);
        assert_eq!(config.pipeline.obliquity_threshold, 5.0);
        assert!(config.pipeline.do_swi);
    }

    /// The B0 echo-combination rows reach the run (they used to be dropped, so the TUI's weight
    /// type was ignored), including the assumed-decay weighting and its assumed T2*.
    #[test]
    fn test_run_args_carry_b0_combination() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        let run = config(&build_run_args(&app).unwrap()).field_mapping;
        let d = qsmxt_engine::FieldMappingConfig::default();
        assert_eq!((run.b0_weight_type, run.b0_estimation), (d.b0_weight_type, d.b0_estimation));
        app.set_field_select("unwrapping_algorithm", 1); // laplacian
        app.set_field_select("b0_weight_type", 5); // assumed-decay
        *app.field_text_mut("b0_weight_assumed_t2star_ms").unwrap() = "35".to_string();
        let mut cfg = PipelineConfig::default();
        qsmxt_io::settings::apply_normalised(&mut cfg, &build_run_args(&app).unwrap().pipeline).unwrap();
        assert_eq!(cfg.field_mapping.b0_weight_type, qsmxt_engine::B0WeightType::AssumedDecay);
        assert_eq!(cfg.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.b0_weight_type, qsmxt_engine::B0WeightType::AssumedDecay);
        assert_eq!(c.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        app.set_field_select("b0_estimation", 1);
        assert_eq!(config(&build_run_args(&app).unwrap()).field_mapping.b0_estimation, qsmxt_engine::B0Estimation::LinearFit);
    }

    /// Choosing MEDI changes no other row: the background removal stays as it is, and the run
    /// takes what the page shows.
    #[test]
    fn choosing_medi_leaves_the_background_removal_alone() {
        use qsmxt_engine::BfAlgorithm;
        let medi = crate::app::QSM_PICKER_OPTIONS.iter().position(|a| *a == "medi").unwrap();
        let bf = |name: &str| crate::app::BF_OPTIONS.iter().position(|o| *o == name).unwrap();
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.set_field_select("qsm_algorithm", medi);
        assert_eq!(app.field_select("bf_algorithm"), bf("vsharp"));
        assert_eq!(passed(&app, "bg_removal.algorithm"), None);
        assert_eq!(config_from_app(&app).unwrap().bg_removal.algorithm, BfAlgorithm::Vsharp);
        app.set_field_select("bf_algorithm", bf("pdf"));
        assert_eq!(config(&build_run_args(&app).unwrap()).bg_removal.algorithm, BfAlgorithm::Pdf);
        assert!(build_command_string(&app).contains("--bf-algorithm pdf"));
        app.reset_field("bf_algorithm");
        assert_eq!(app.field_select("bf_algorithm"), bf("vsharp"));
    }

    /// MEDI's mask shrinking is a row under SMV, and reaches the run.
    #[test]
    fn medi_smv_shrink_mask_reaches_the_run() {
        let medi = crate::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "medi").unwrap();
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.set_field_select("qsm_algorithm", medi);
        assert!(config_from_app(&app).unwrap().inversion.medi.smv_shrink_mask);
        app.toggle_field("medi_smv_shrink_mask");
        assert!(!config_from_app(&app).unwrap().inversion.medi.smv_shrink_mask);
        assert!(build_command_string(&app).contains("--no-medi-smv-shrink-mask"));
    }

    /// Choosing QSMART changes neither the field mapping nor the mask; set, they reach the run,
    /// as do QSMART's vessel options and the echo fit's threshold.
    #[test]
    fn qsmart_leaves_the_field_mapping_and_mask_alone() {
        use qsmxt_engine::{B0Estimation, LaplacianSolver, UnwrappingAlgorithm, default_mask_sections};
        let qsmart = crate::app::QSM_PICKER_OPTIONS.iter().position(|a| *a == "qsmart").unwrap();
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.set_field_select("qsm_algorithm", qsmart);
        let selects = ["unwrapping_algorithm", "laplacian_solver", "b0_estimation"].map(|f| app.field_select(f));
        assert_eq!(selects, [0, 0, 0]);
        assert_eq!(app.mask().sections, default_mask_sections());
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping, qsmxt_engine::FieldMappingConfig::default());
        assert_eq!(c.masking.sections, default_mask_sections());

        app.set_field_select("unwrapping_algorithm", 1);
        app.set_field_select("laplacian_solver", 2);
        app.set_field_select("b0_estimation", 3);
        let fm = config_from_app(&app).unwrap().field_mapping;
        assert_eq!((fm.unwrapping_algorithm, fm.laplacian_solver, fm.b0_estimation),
                   (UnwrappingAlgorithm::Laplacian, LaplacianSolver::Mirror, B0Estimation::QsmartEchofit));
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--laplacian-solver mirror") && cmd.contains("--b0-estimation qsmart-echofit"), "{cmd}");

        app.set_field_select("qsmart_vessel_magnitude", 1);
        app.set_field_select("qsmart_vessel_units", 1);
        app.set_field_select("qsmart_fit_threshold_mode", 0);
        *app.field_text_mut("qsmart_fit_percentile").unwrap() = "95".into();
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.inversion.qsmart.vessel_magnitude, qsmxt_engine::QsmartVesselMagnitude::Combined);
        assert_eq!(c.inversion.qsmart.vessel_size_units, qsmxt_engine::VesselSizeUnits::Mm);
        assert_eq!(c.field_mapping.qsmart_fit_threshold(), qsm_core::utils::QsmartFitThreshold::Percentile(95.0));
        app.set_field_select("qsmart_fit_threshold_mode", 1);
        *app.field_text_mut("qsmart_fit_threshold").unwrap() = "30".into();
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.qsmart_fit_threshold(), qsm_core::utils::QsmartFitThreshold::Absolute(30.0));

        assert_eq!(c.inversion.qsmart.frangi_c(), qsm_core::utils::FrangiC::Absolute(qsm_core::utils::QSMART_FRANGI_C));
        app.set_field_select("qsmart_frangi_c_mode", 1);
        *app.field_text_mut("qsmart_frangi_c_factor").unwrap() = "0.3".into();
        assert_eq!(config_from_app(&app).unwrap().inversion.qsmart.frangi_c(), qsm_core::utils::FrangiC::RelativeToMedian(0.3));
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--qsmart-frangi-c-mode relative") && cmd.contains("--qsmart-frangi-c-factor 0.3"), "{cmd}");

        app.set_field_select("mask_preset", 1);
        assert!(build_command_string(&app).contains("--mask-preset bet"));
    }

    /// The pipeline the SLURM jobs get must be the one a local run would use.
    #[test]
    fn test_slurm_and_local_resolve_to_the_same_config() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.form.slurm_account = "acct".to_string();
        app.set_qsm_algorithm(4); // tgv
        app.set_field_select("unwrapping_algorithm", 1); // laplacian
        app.set_field_select("bf_algorithm", 1); // pdf
        app.toggle_field("bipolar_correction");

        let mut local = PipelineConfig::default();
        qsmxt_io::settings::apply_normalised(&mut local, &build_run_args(&app).unwrap().pipeline).unwrap();
        let mut slurm = PipelineConfig::default();
        qsmxt_io::settings::apply_normalised(&mut slurm, &build_slurm_args(&app).unwrap().pipeline).unwrap();

        assert_eq!(slurm.to_toml().unwrap(), local.to_toml().unwrap());
        // ... and it is not merely the default pipeline.
        assert_ne!(slurm.to_toml().unwrap(), PipelineConfig::default().to_toml().unwrap());
    }

    // --- output_dir optional ---

    #[test]
    fn test_build_run_args_output_dir_empty() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.output_dir, None);
    }

    #[test]
    fn test_command_string_output_dir_omitted_when_empty() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        let cmd = build_command_string(&app);
        assert_eq!(cmd.matches("/bids").count(), 1); // only bids_dir, no output_dir
    }
}

