use std::path::PathBuf;

use crate::cli::*;
use crate::pipeline::config::PipelineConfig;
use super::app::App;

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

pub fn build_run_args(app: &App) -> crate::Result<RunArgs> {
    let form = &app.form;
    if form.bids_dir.is_empty() {
        return Err(crate::error::QsmxtError::Config(
            "BIDS directory is required".to_string(),
        ));
    }

    Ok(RunArgs {
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

/// Pipeline settings as they stand in the TUI form.
///
/// Shared by the local (`RunArgs`) and SLURM (`SlurmArgs`) paths so both
/// execute the pipeline the user configured, not the defaults.
/// The CLI algorithm argument for each entry of [`QSM_ALGO_OPTIONS`], in that order.
///
/// The TUI carries the user's choice as an index into `QSM_ALGO_OPTIONS`, and this is what turns it
/// back into an algorithm. The two lists must stay in step: insert an algorithm into one and not
/// the other and the TUI silently runs a *different* algorithm than the one on screen, with no
/// error anywhere. `qsm_algo_args_match_the_option_list` holds them together.
pub const QSM_ALGO_ARGS: [QsmAlgorithmArg; 32] = [
        QsmAlgorithmArg::Rts,
        QsmAlgorithmArg::Tv,
        QsmAlgorithmArg::Tkd,
        QsmAlgorithmArg::Tsvd,
        QsmAlgorithmArg::Tgv,
        QsmAlgorithmArg::Tikhonov,
        QsmAlgorithmArg::Nltv,
        QsmAlgorithmArg::Medi,
        QsmAlgorithmArg::Tfi,
        QsmAlgorithmArg::Ilsqr,
        QsmAlgorithmArg::Star,
        QsmAlgorithmArg::Lsqr,
        QsmAlgorithmArg::Qsmart,
        QsmAlgorithmArg::Ndi,
        QsmAlgorithmArg::Fansi,
        QsmAlgorithmArg::FansiTgv,
        QsmAlgorithmArg::L1qsm,
        QsmAlgorithmArg::Whqsm,
        QsmAlgorithmArg::Hdqsm,
        QsmAlgorithmArg::AmpPe,
        QsmAlgorithmArg::Xqsm,
        QsmAlgorithmArg::Qsmnet,
        QsmAlgorithmArg::QsmnetPlus,
        QsmAlgorithmArg::Qsmnetme,
        QsmAlgorithmArg::Autoqsm,
        QsmAlgorithmArg::Qsmgan,
        QsmAlgorithmArg::Ir2qsm,
        QsmAlgorithmArg::Lpcnn,
        QsmAlgorithmArg::ModlQsm,
        QsmAlgorithmArg::Nextqsm,
        QsmAlgorithmArg::Iqsm,
        QsmAlgorithmArg::IqsmPlus,
];

/// The inversion row's starting index: qsmxt-engine's default inversion, so the TUI and
/// `qsmxt run` without `--qsm-algorithm` mean the same algorithm.
pub fn default_qsm_algorithm_index() -> usize {
    let default = crate::pipeline::config::InversionConfig::default().algorithm;
    QSM_ALGO_ARGS.iter()
        .position(|a| crate::pipeline::config::qsm_algorithm_arg_to_config(*a) == default)
        .expect("the default inversion has a row in QSM_ALGO_ARGS")
}

fn default_qsm_algorithm_arg() -> QsmAlgorithmArg {
    QSM_ALGO_ARGS[default_qsm_algorithm_index()]
}


pub fn pipeline_args_from_app(app: &App) -> PipelineArgs {
    let form = &app.form;
    let ps = &app.pipeline_state;

    let qsm_options = QSM_ALGO_ARGS;
    let bf_options = [
        BfAlgorithmArg::Vsharp,
        BfAlgorithmArg::Pdf,
        BfAlgorithmArg::Lbv,
        BfAlgorithmArg::Ismv,
        BfAlgorithmArg::Sharp,
        BfAlgorithmArg::Resharp,
        BfAlgorithmArg::Harperella,
        BfAlgorithmArg::Iharperella,
        BfAlgorithmArg::Bfrnet,
        BfAlgorithmArg::Iqfm,
    ];

    // The orientation grouping is set on the Input tab; how a group is reconstructed is set from
    // the form's settings store ([`stored_values`]).
    let orientation_group = app.filter_state.orientation_group_by();
    let grouped = orientation_group.is_some();

    PipelineArgs {
        // The form holds every setting; the chosen pipeline is set on the config afterwards.
        pipeline: None,
        // Bounds-checked rather than indexed: the option list shrinks with the build (see #227).
        qsm_algorithm: Some(qsm_options.get(ps.qsm_algorithm).copied().unwrap_or_else(default_qsm_algorithm_arg)),
        // The HEIDI checkbox (with LSQR, LSQR+HEIDI); not offered for multi-orientation runs.
        heidi: ps.heidi_enabled && !grouped,
        multi_orientation_algorithm: None,
        multi_orientation_lambda: None,
        orientation_reference: None,
        orientation_group,
        multi_orientation_force: false,
        no_orientation_registration: false,
        cosmos_magnitude_weighting: false,
        cosmos_tol: None,
        cosmos_max_iter: None,
        // Field mapping is set from the form's settings store ([`stored_values`]).
        unwrapping_algorithm: None,
        // Only when the inversion has a background-removal stage: otherwise the choice has no
        // effect, and iQFM there is refused (#276). Said when not the general default.
        bf_algorithm: (ps.inversion_needs_bg_removal() && ps.bf_algorithm != 0)
            .then(|| bf_options[ps.bf_algorithm]),
        bf_erode_voxels: None,
        bf_polish_mask: false,
        no_bf_polish_mask: false,
        bf_ukb_mask_trim: false,
        no_bf_ukb_mask_trim: false,
        masking_input: None,
        phase_offset_removal: None,
        phase_offset_sigma_mm: None,
        coil_combination_sigma: None,
        bipolar_correction: false,
        b0_estimation: None,
        qsmart_fit_threshold_mode: None,
        qsmart_fit_percentile: None,
        qsmart_fit_threshold: None,
        b0_weight_type: None,
        b0_weight_assumed_t2star: None,
        laplacian_solver: None,
        laplacian_fft_pad: None,
        laplacian_unmasked: false,
        no_laplacian_unmasked: false,
        laplacian_keep_outside: false,
        no_laplacian_keep_outside: false,
        // BET is edited per mask step in the TUI, so its settings travel inside the sections; the
        // rest of the masking is set from the form's settings store ([`stored_values`]).
        bet_fractional_intensity: None,
        bet_voxel_scale: None,
        bet_smoothness: None,
        bet_gradient_threshold: None,
        bet_iterations: None,
        bet_subdivisions: None,
        bet_orientation: None,
        // Set from the form's settings store ([`stored_values`]).
        qsm_reference: None,
        qsm_per_echo: None,
        // The inversion's settings are set from the form's settings store ([`stored_values`]),
        // but MEDI's SMV mode, which the form's rule for which stages run reads.
        rts_params: Default::default(),
        tv_params: Default::default(),
        tkd_params: Default::default(),
        tsvd_params: Default::default(),
        tgv_params: Default::default(),
        tikhonov_params: Default::default(),
        nltv_params: Default::default(),
        medi_params: crate::cli::MediParamArgs { medi_smv: ps.medi_smv, no_medi_smv: !ps.medi_smv, ..Default::default() },
        tfi_params: Default::default(),
        lsqr_params: Default::default(),
        heidi_params: Default::default(),
        ilsqr_params: Default::default(),
        star_params: Default::default(),
        qsmart_params: Default::default(),
        ndi_params: Default::default(),
        fansi_params: Default::default(),
        l1qsm_params: Default::default(),
        whqsm_params: Default::default(),
        hdqsm_params: Default::default(),
        amp_pe_params: Default::default(),
        motion_params: Default::default(),
        // χ-separation's settings are set from the form's settings store ([`stored_values`]).
        separation_params: Default::default(),
        // Background removal's settings are set from the form's settings store ([`stored_values`]).
        vsharp_params: Default::default(),
        pdf_params: Default::default(),
        lbv_params: Default::default(),
        ismv_params: Default::default(),
        sharp_params: Default::default(),
        resharp_params: Default::default(),
        harperella_params: Default::default(),
        iharperella_params: Default::default(),
        // Field mapping is set from the form's settings store ([`stored_values`]).
        romeo_params: Default::default(),
        prelude_params: Default::default(),
        // Set from the form's settings store ([`config_from_app`]).
        swi_params: Default::default(),
        tiling_params: Default::default(),
        // The magnitude correction is set from the form's settings store ([`stored_values`]).
        inhomogeneity_method: None,
        homogeneity_sigma_mm: None,
        homogeneity_nbox: None,
        n4_params: Default::default(),
        linear_fit_estimate_offset: None,
        robust_fit_loss: None,
        robust_fit_tuning: None,
        robust_fit_iterations: None,
        robust_fit_downweight_level: None,
        robust_fit_flag_ratio: None,
        complex_fit_reltol: None,
        complex_fit_max_iter: None,
        no_qsm: !ps.do_qsm,
        do_swi: false,
        swi_filtered_phase: false,
        no_magnitude: false,
        do_smwi: false,
        // The segmentation's settings are set from the form's settings store ([`stored_values`]).
        do_segmentation: false,
        do_analysis: false,
        segmentation_params: Default::default(),
        smwi_params: Default::default(),
        // The relaxometry maps and R2' settings are set from the form's settings store ([`stored_values`]).
        do_t2starmap: false,
        do_r2starmap: false,
        do_r2map: false,
        do_r2primemap: false,
        do_chi_separation: false,
        chi_separation_algorithm: None,
        use_custom_qsm: None,
        use_custom_r2: None,
        use_custom_r2prime: None,
        r2prime_strategy: None,
        r2primenet_ignore_field_mismatch: false,
        r2primenet_patch: None,
        export_dicom: form.export_dicom,
        inhomogeneity_correction: false,
        no_inhomogeneity_correction: false,
        // Geometry is set from the form's settings store ([`stored_values`]).
        axial_resampling: false,
        no_axial_resampling: false,
        obliquity_threshold: None,
        crop_to_mask: false,
        ignore_slice_gap: false,
        fft_padding: false,
        tukey_window: None,
        crop_margin_mm: None,
        output_space: None,
        // The mask recipes, custom masks and two-pass are set from the form's settings store
        // ([`stored_values`]).
        mask_preset: None,
        use_custom_masks: None,
        mask_sections_cli: None,
        mask_combine: None,
        mask_refinements_cli: None,
        two_pass: false,
        two_pass_sections_cli: None,
        two_pass_mask_combine: None,
        two_pass_mode: None,
        dismiss_recommendation: (!ps.dismissed.is_empty())
            .then(|| ps.dismissed.iter().map(|d| format!("{}={}", d.rule, d.chosen)).collect()),
    }
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
fn typed<T>(flag: &str, text: &str, what: &str, read: impl Fn(&str) -> Option<T>) -> crate::Result<Option<T>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    read(t).map(Some).ok_or_else(|| crate::error::QsmxtError::Config(format!("{flag} {t}: not {what}")))
}

fn number(t: &str) -> Option<f64> {
    t.parse().ok().filter(|x: &f64| x.is_finite())
}

fn count(t: &str) -> Option<usize> {
    t.parse().ok()
}

pub fn build_slurm_args(app: &App) -> crate::Result<SlurmArgs> {
    let form = &app.form;
    if form.bids_dir.is_empty() {
        return Err(crate::error::QsmxtError::Config(
            "BIDS directory is required".to_string(),
        ));
    }
    if form.slurm_account.trim().is_empty() {
        return Err(crate::error::QsmxtError::Config(
            "SLURM account is required".to_string(),
        ));
    }

    let defaults = super::app::RunForm::default();
    let (include, exclude) = app.filter_state.get_include_exclude();
    Ok(SlurmArgs {
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

/// The PipelineConfig the TUI's current state runs with (methods preview, generated command).
///
/// Built from the same argument list the run gets ([`pipeline_args_from_app`]) through the same
/// override path as `qsmxt run`, so the preview, the shown command and the run cannot drift apart:
/// a setting the TUI has but the CLI cannot express simply has no effect here, which the parity
/// test (`parity::every_run_flag_is_reachable_from_the_tui`) catches.
///
/// An error is a setting the run would refuse (an unparseable mask section, say); the TUI shows
/// it in place of the command and the methods rather than previewing some other configuration.
/// The methods text (Markdown) for the current settings, or why the settings are invalid.
pub fn methods_text(app: &App) -> Result<String, String> {
    app.current_config().as_ref().as_ref().map(crate::pipeline::methods::generate_methods).map_err(|e| e.clone())
}

/// The settings of the form's settings store the run takes: those changed from their defaults
/// that apply. One left behind by an earlier choice (hidden now) is left at its default, so that
/// it does not change the run.
fn stored_values(app: &App, args: &PipelineArgs) -> crate::Result<Vec<(&'static str, serde_json::Value)>> {
    let settings = &app.form.settings;
    let (changed, refused) = settings.changed_or_refused();
    if changed.is_empty() && refused.is_empty() {
        return Ok(changed);
    }
    // Which apply is decided on the form's whole config. Should the fields' checks refuse a
    // stored setting (multi-orientation's, say, of a hidden COSMOS tolerance), the fields are
    // written first, so that the checks see only the stored settings that apply, as the run does.
    let mut config = settings.config_of(&changed);
    if crate::pipeline::config::apply_run_overrides_as_given(&mut config, args).is_err() {
        config = PipelineConfig::default();
        crate::pipeline::config::apply_run_overrides_as_given(&mut config, args)?;
        config = super::settings_rows::with_values(&config, &changed);
    }
    let visible = app.visible_settings(&config);
    // Typed text the run cannot take is an error while its setting applies, as on the command
    // line: the run is refused rather than run with something else.
    if let Some((_, why)) = refused.iter().find(|(p, _)| visible.contains(p)) {
        return Err(crate::error::QsmxtError::Config(why.clone()));
    }
    changed.into_iter().filter(|(p, _)| visible.contains(p))
        .map(|(p, v)| Ok((p, as_spelled(p, v)?)))
        .collect()
}

/// A mask recipe as the run has always taken it from the form: spelled as the command line
/// spells it (`--mask`, `--mask-refine`, `--two-pass-mask`) and read back. The spelling rounds
/// some numbers (a fixed threshold to 4 places, BET's fractional intensity to 2) and gives a fixed
/// or percentile threshold without a value its default; a step the command line cannot take is
/// refused as it would be there. Any other setting is as it is. Kept until the form's settings
/// are all the config's.
fn as_spelled(path: &str, value: serde_json::Value) -> crate::Result<serde_json::Value> {
    use crate::pipeline::config::{hole_closing_warnings, parse_mask_sections, MaskOp, MaskSection};
    let unreadable = |e: serde_json::Error| crate::error::QsmxtError::Config(format!("{path}: {e}"));
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
                    .map_err(|e| crate::error::QsmxtError::Config(format!("--mask-refine {s}: {e}")))?;
                if op.is_generator() {
                    return Err(crate::error::QsmxtError::Config(format!(
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

/// The run's settings from the form: its fields as `PipelineArgs`, and the settings it keeps in a
/// config ([`stored_values`]).
fn settings_args(app: &App) -> crate::Result<crate::cli_settings::SettingsArgs> {
    let args = pipeline_args_from_app(app);
    let values = stored_values(app, &args)?;
    Ok(crate::cli_settings::SettingsArgs::tui(args, values))
}

/// The config the form builds: the settings it keeps in a config ([`stored_values`]), then those
/// of its fields, which the form passes as the command line would and so leave the others as they
/// are. As the form gives them: an output another setting needs and the form leaves off is a
/// required change for the TUI to offer ([`qsmxt_engine::normalise::required_findings`]), which
/// the run would otherwise make itself.
pub fn config_from_app(app: &App) -> crate::Result<PipelineConfig> {
    let args = pipeline_args_from_app(app);
    let mut config = app.form.settings.config_of(&stored_values(app, &args)?);
    crate::pipeline::config::apply_run_overrides_as_given(&mut config, &args)?;
    // The pipeline chosen on the Pipeline tab, while the form still makes its choices.
    config.reference_pipeline = app.pipeline_state.reference_pipeline.clone()
        .filter(|id| qsmxt_engine::pipelines::chosen(&config, id));
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::app::App;

    /// The config a run gets from its arguments.
    fn config(args: &RunArgs) -> PipelineConfig {
        crate::cli_settings::build_config(Default::default(), &args.pipeline).unwrap()
    }

    /// The TUI holds the chosen algorithm as an index into `QSM_ALGO_OPTIONS`, and
    /// [`QSM_ALGO_ARGS`] is what turns it back into an algorithm. If the two lists drift — insert
    /// an algorithm into one and not the other — the TUI runs a different algorithm than the one
    /// it displays, silently. Nothing else would catch that.
    #[test]
    fn qsm_algo_args_match_the_option_list() {
        use crate::tui::app::QSM_ALGO_OPTIONS;
        assert_eq!(QSM_ALGO_ARGS.len(), QSM_ALGO_OPTIONS.len(),
                   "QSM_ALGO_ARGS and QSM_ALGO_OPTIONS have different lengths");
        for (i, arg) in QSM_ALGO_ARGS.iter().enumerate() {
            let name = format!("{}", crate::pipeline::config::qsm_algorithm_arg_to_config(*arg));
            assert_eq!(name, QSM_ALGO_OPTIONS[i],
                       "row {i} shows {:?} but would run {name}", QSM_ALGO_OPTIONS[i]);
        }
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
        use crate::tui::app::QSM_ALGO_OPTIONS;
        for (i, name) in QSM_ALGO_OPTIONS.iter().enumerate() {
            let mut app = default_app();
            app.pipeline_state.qsm_algorithm = i;
            let config = config_from_app(&app).unwrap();
            assert_eq!(&format!("{}", config.inversion.algorithm), name,
                       "row {i} shows {name} but configures {}", config.inversion.algorithm);
            let args = pipeline_args_from_app(&app);
            let arg = args.qsm_algorithm.expect("row sets an algorithm");
            assert_eq!(&format!("{}", crate::pipeline::config::qsm_algorithm_arg_to_config(arg)), name,
                       "row {i} shows {name} but passes a different --qsm-algorithm");
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
        let run = crate::cli_settings::build_config(Default::default(), &build_run_args(&app).unwrap().pipeline).unwrap();
        assert_eq!(run.separation.r2primenet, c.separation.r2primenet);
    }

    /// V-SHARP's working grid: the engine's by default (off the command), spacing row only for
    /// isotropic.
    #[test]
    fn vsharp_working_grid() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(|r| match r {
            crate::tui::app::PipelineRow::Param { field, .. } | crate::tui::app::PipelineRow::AlgoSelect { field, .. } => Some(field.to_string()),
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
            matches!(r, crate::tui::app::PipelineRow::Param { field: "vsharp_threshold", .. }));
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
            .any(|r| matches!(r, crate::tui::app::PipelineRow::AlgoSelect { field: "two_pass_mode", .. }));
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
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
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
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
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
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. }
            | crate::tui::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
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
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. }
            | crate::tui::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
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
            crate::tui::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
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
        let fields = |app: &App| app.pipeline_rows().iter().filter_map(crate::tui::app::row_field)
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

        let ps = &mut app.pipeline_state;
        ps.qsm_algorithm = crate::tui::app::QSM_ALGO_OPTIONS.iter().position(|o| *o == "medi").unwrap();
        assert!(fields(&app).contains(&"medi_lambda_csf".to_string()));
        *app.field_text_mut("medi_lambda_csf").unwrap() = "50".into();
        *app.field_text_mut("medi_zeropad").unwrap() = "2 0 2".into();
        app.toggle_field("medi_csf_erode");
        let m = config_from_app(&app).unwrap().inversion.medi;
        assert_eq!((m.csf, m.lambda_csf, m.csf_erode, m.zeropad), (true, 50.0, false, [2, 0, 2]));
        app.toggle_field("medi_csf");
        assert!(!fields(&app).contains(&"medi_lambda_csf".to_string()));
        assert!(!config_from_app(&app).unwrap().inversion.medi.csf);

        app.pipeline_state.qsm_algorithm = crate::tui::app::QSM_ALGO_OPTIONS.iter().position(|o| *o == "tfi").unwrap();
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
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
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
        assert!(crate::cli_settings::build_config(Default::default(), &build_run_args(&{
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
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. }
            | crate::tui::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
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
        let run = crate::cli_settings::build_config(Default::default(), &build_run_args(&app).unwrap().pipeline).unwrap();
        assert_eq!((run.motion_correction, run.registration), (m, reg));
    }

    /// Multi-orientation co-registration and force are in the TUI; the registration settings
    /// show while either registration can run.
    /// The TUI cannot produce iQFM (or any BFR) with an inversion that has no background-removal
    /// stage: the section is hidden and no `--bf-algorithm` is generated (#276). Which those are
    /// comes from QSM.rs (#265), so TFI is among them.
    #[test]
    fn background_removal_is_hidden_and_not_sent_for_total_field_inversions() {
        use crate::tui::app::{PipelineRow, QSM_ALGO_OPTIONS};
        // Every option parses as a config algorithm, or the check would silently say "needs BFR".
        for name in QSM_ALGO_OPTIONS {
            assert!(serde_json::from_value::<crate::pipeline::config::QsmAlgorithm>(serde_json::json!(name)).is_ok(), "{name}");
        }
        let mut app = default_app();
        let shows_bfr = |app: &App| app.pipeline_rows().iter().any(|r|
            matches!(r, PipelineRow::AlgoSelect { field: "bf_algorithm", .. }));
        assert!(shows_bfr(&app));
        // The default goes unsent; another one is sent.
        assert!(pipeline_args_from_app(&app).bf_algorithm.is_none());
        app.pipeline_state.set_select("bf_algorithm", 1);
        assert!(pipeline_args_from_app(&app).bf_algorithm.is_some());
        for name in ["tfi", "tgv", "qsmart", "autoqsm", "nextqsm", "iqsm"] {
            app.pipeline_state.qsm_algorithm = QSM_ALGO_OPTIONS.iter().position(|o| *o == name).unwrap();
            assert!(!shows_bfr(&app), "{name}");
            assert!(pipeline_args_from_app(&app).bf_algorithm.is_none(), "{name}");
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
        let run = crate::cli_settings::build_config(Default::default(), &build_run_args(&app).unwrap().pipeline).unwrap();
        assert!(!run.multi_orientation.register && run.multi_orientation.force);
        let shows_reg = |app: &App| app.pipeline_rows().iter().any(|r|
            matches!(r, crate::tui::app::PipelineRow::Param { field: "reg_levels", .. }));
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
        use crate::tui::app::QSM_ALGO_OPTIONS;
        let qsmart = QSM_ALGO_OPTIONS.iter().position(|o| *o == "qsmart").expect("qsmart");
        for (i, name) in QSM_ALGO_OPTIONS.iter().enumerate() {
            let mut app = default_app();
            app.pipeline_state.qsm_algorithm = i;
            app.form.settings.set("inversion.qsmart.inversion", "rts");
            let set = build_command_string(&app).contains("--qsmart-inversion rts");
            assert_eq!(set, i == qsmart, "qsmart_inversion set for {name} (row {i})");
        }
    }

    /// A saved form from an older version can carry an index past the end of today's list. That is
    /// a stale setting, not a reason to crash.
    #[test]
    fn an_out_of_range_inversion_index_falls_back_instead_of_panicking() {
        let mut app = default_app();
        app.pipeline_state.qsm_algorithm = 999;
        assert_eq!(format!("{}", config_from_app(&app).unwrap().inversion.algorithm), "ilsqr");
        assert_eq!(pipeline_args_from_app(&app).qsm_algorithm, Some(QsmAlgorithmArg::Ilsqr));
    }

    fn default_app() -> App {
        App::new()
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
        app.pipeline_state.qsm_algorithm = crate::tui::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "xqsm").unwrap();
        assert!(!build_command_string(&app).contains("--tile-size"));
        app.toggle_field("dl_tiling");
        assert!(build_command_string(&app).contains("--tile-size 128"));

        app.pipeline_state.qsm_algorithm = crate::tui::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "medi").unwrap();
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
        assert_eq!(args.pipeline.qsm_algorithm, Some(crate::cli::QsmAlgorithmArg::Ilsqr));
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
        app.pipeline_state.qsm_algorithm = 0; // rts
        *app.field_text_mut("rts_delta").unwrap() = "0.2".into();
        *app.field_text_mut("rts_mu").unwrap() = "1e5".into();
        *app.field_text_mut("rts_tol").unwrap() = "1e-4".into();
        app.form.n_procs = "8".to_string();
        let args = build_run_args(&app).unwrap();
        let rts = config(&args).inversion.rts;
        assert_eq!((rts.delta, rts.mu, rts.tol), (0.2, 1e5, 1e-4));
        assert_eq!(args.n_procs, Some(8));
        app.pipeline_state.qsm_algorithm = 4; // tgv
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
        let config = |args: &RunArgs| crate::cli_settings::build_config(Default::default(), &args.pipeline).unwrap();
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
        let config = crate::cli_settings::build_config(Default::default(), &args.pipeline).unwrap();
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
        assert!(app.mask().sections.iter().any(|s| matches!(s.generator, crate::pipeline::config::MaskOp::Bet { .. })));
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
        let err = |r: crate::Result<RunArgs>| r.expect_err("refused").to_string();
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

    /// The command the TUI prints in SLURM mode carries the pipeline flags, so
    /// `qsmxt slurm` must actually accept them — a copy-pasted command has to run.
    #[test]
    fn test_slurm_command_string_parses() {
        use clap::Parser;
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.form.execution_mode = 1;
        app.form.slurm_account = "acct".to_string();
        app.pipeline_state.qsm_algorithm = 4; // tgv
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
        assert!(args.pipeline.export_dicom);

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
        app.pipeline_state.qsm_algorithm = 2; // tkd
        app.set_field_select("unwrapping_algorithm", 1); // laplacian
        app.pipeline_state.bf_algorithm = 1; // pdf
        app.form.settings.set("pipeline.axial_resampling", true);
        *app.field_text_mut("obliquity_threshold").unwrap() = "5".into();
        app.form.settings.set("pipeline.do_swi", true);

        let slurm = build_slurm_args(&app).unwrap();
        assert_eq!(slurm.pipeline.qsm_algorithm, Some(crate::cli::QsmAlgorithmArg::Tkd));
        assert_eq!(slurm.pipeline.bf_algorithm, Some(crate::cli::BfAlgorithmArg::Pdf));
        let config = crate::cli_settings::build_config(Default::default(), &slurm.pipeline).unwrap();
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
        crate::cli_settings::apply_normalised(&mut cfg, &build_run_args(&app).unwrap().pipeline).unwrap();
        assert_eq!(cfg.field_mapping.b0_weight_type, crate::pipeline::config::B0WeightType::AssumedDecay);
        assert_eq!(cfg.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.b0_weight_type, crate::pipeline::config::B0WeightType::AssumedDecay);
        assert_eq!(c.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        app.set_field_select("b0_estimation", 1);
        assert_eq!(config(&build_run_args(&app).unwrap()).field_mapping.b0_estimation, qsmxt_engine::B0Estimation::LinearFit);
    }

    /// Choosing MEDI changes no other row: the background removal stays as it is, and the run
    /// takes what the page shows.
    #[test]
    fn choosing_medi_leaves_the_background_removal_alone() {
        use crate::cli::BfAlgorithmArg;
        use qsmxt_engine::BfAlgorithm;
        let medi = crate::tui::app::QSM_PICKER_OPTIONS.iter().position(|a| *a == "medi").unwrap();
        let bf = |name: &str| crate::tui::app::BF_OPTIONS.iter().position(|o| *o == name).unwrap();
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.pipeline_state.set_select("qsm_algorithm", medi);
        assert_eq!(app.pipeline_state.bf_algorithm, bf("vsharp"));
        assert_eq!(build_run_args(&app).unwrap().pipeline.bf_algorithm, None);
        assert_eq!(config_from_app(&app).unwrap().bg_removal.algorithm, BfAlgorithm::Vsharp);
        app.pipeline_state.set_select("bf_algorithm", bf("pdf"));
        assert_eq!(build_run_args(&app).unwrap().pipeline.bf_algorithm, Some(BfAlgorithmArg::Pdf));
        assert!(build_command_string(&app).contains("--bf-algorithm pdf"));
        app.reset_field("bf_algorithm");
        assert_eq!(app.pipeline_state.bf_algorithm, bf("vsharp"));
    }

    /// MEDI's mask shrinking is a row under SMV, and reaches the run.
    #[test]
    fn medi_smv_shrink_mask_reaches_the_run() {
        let medi = crate::tui::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "medi").unwrap();
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.pipeline_state.set_select("qsm_algorithm", medi);
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
        let qsmart = crate::tui::app::QSM_PICKER_OPTIONS.iter().position(|a| *a == "qsmart").unwrap();
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.pipeline_state.set_select("qsm_algorithm", qsmart);
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
        app.pipeline_state.qsm_algorithm = 4; // tgv
        app.set_field_select("unwrapping_algorithm", 1); // laplacian
        app.pipeline_state.bf_algorithm = 1; // pdf
        app.toggle_field("bipolar_correction");

        let mut local = PipelineConfig::default();
        crate::cli_settings::apply_normalised(&mut local, &build_run_args(&app).unwrap().pipeline).unwrap();
        let mut slurm = PipelineConfig::default();
        crate::cli_settings::apply_normalised(&mut slurm, &build_slurm_args(&app).unwrap().pipeline).unwrap();

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

/// Guard for "every `qsmxt run` flag is reachable from the TUI": changes every control the TUI
/// has (every row the QSM and separation tabs can show, and the other tabs' controls) and checks
/// that the change reaches the run, shows in the generated command, and that the command parses
/// back to itself; then that the flags those commands used cover every `run` flag.
#[cfg(test)]
mod parity {
    use super::*;
    use crate::tui::app::{App, PipelineRow};
    use clap::CommandFactory;
    use std::collections::{BTreeMap, BTreeSet};

    fn flags_of(cmd: &str) -> BTreeSet<String> {
        cmd.split_whitespace().filter(|t| t.starts_with("--")).map(str::to_string).collect()
    }

    /// A config, through its generated command and back. `cli` is built once by the caller:
    /// building clap's command is most of the cost of a parse.
    fn round_trip(cli: &mut clap::Command, c: &PipelineConfig) -> PipelineConfig {
        use clap::FromArgMatches;
        let cmd = qsmxt_engine::generate_command(c);
        let argv: Vec<&str> = cmd.split_whitespace().collect();
        let m = cli.try_get_matches_from_mut(&argv).unwrap_or_else(|e| panic!("{cmd}: {e}"));
        let parsed = crate::cli::Cli::from_arg_matches(&m).unwrap_or_else(|e| panic!("{cmd}: {e}"));
        let crate::cli::Command::Run(run) = parsed.command else { panic!("not run: {cmd}") };
        let mut back = PipelineConfig::default();
        crate::cli_settings::apply_normalised(&mut back, &run.pipeline).unwrap();
        back
    }

    /// The leaves where two configs differ, as dotted paths.
    fn diff(a: &PipelineConfig, b: &PipelineConfig) -> Vec<String> {
        fn walk(p: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut Vec<String>) {
            match (a, b) {
                (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
                    for k in x.keys().chain(y.keys()).collect::<BTreeSet<_>>() {
                        let n = serde_json::Value::Null;
                        walk(&format!("{p}.{k}"), x.get(k).unwrap_or(&n), y.get(k).unwrap_or(&n), out);
                    }
                }
                _ if a != b => out.push(format!("{p}: {a} vs {b}")),
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk("", &serde_json::to_value(a).unwrap(), &serde_json::to_value(b).unwrap(), &mut out);
        out
    }

    fn run_flags() -> BTreeSet<String> {
        let cli = crate::cli::Cli::command();
        let run = cli.find_subcommand("run").expect("run subcommand");
        run.get_arguments().filter_map(|a| a.get_long()).map(|l| format!("--{l}")).collect()
    }

    /// Every row the TUI shows in this state: each sub-tab of each tab, as it applies now.
    fn rows(app: &App) -> Vec<PipelineRow> {
        app.all_pages().into_iter().flat_map(|(_, _, rows)| rows).collect()
    }

    fn row_key(r: &PipelineRow) -> Option<(&'static str, char)> {
        match r {
            PipelineRow::AlgoSelect { field, .. } => Some((field, 's')),
            PipelineRow::Param { field, .. } => Some((field, 'p')),
            PipelineRow::Toggle { field, .. } => Some((field, 't')),
            _ => None,
        }
    }

    /// A different value of the same shape: each number in it moved up (`by` 1) or down (-1)
    /// (lists stay lists).
    fn bumped(v: &str, by: i64) -> String {
        let bump = |t: &str| -> Option<String> {
            if let Ok(n) = t.parse::<i64>() { return Some((n + by).to_string()); }
            let x = t.parse::<f64>().ok()?;
            Some(format!("{}", if x == 0.0 { 0.1 } else if by > 0 { x * 0.9 } else { x * 1.1 }))
        };
        let tokens: Vec<&str> = v.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).collect();
        match tokens.iter().map(|t| bump(t)).collect::<Option<Vec<_>>>() {
            Some(b) if !b.is_empty() => b.join(if v.contains(',') { "," } else { " " }),
            _ => "3".to_string(),
        }
    }

    /// What a state is: the parts of the app a control changes (everything else stays as
    /// [`App::new`] has it). Unlike an [`App`] it can be shared between threads.
    #[derive(Clone)]
    struct State {
        pipeline_state: crate::tui::app::PipelineFormState,
        form: crate::tui::app::RunForm,
        filter_state: crate::tui::app::FilterTreeState,
        active_tab: usize,
    }

    impl State {
        fn of(app: &App) -> Self {
            State {
                pipeline_state: app.pipeline_state.clone(),
                form: app.form.clone(),
                filter_state: app.filter_state.clone(),
                active_tab: app.active_tab,
            }
        }

        fn app(&self) -> App {
            let mut a = App::new();
            a.pipeline_state = self.pipeline_state.clone();
            a.form = self.form.clone();
            a.filter_state = self.filter_state.clone();
            a.active_tab = self.active_tab;
            a
        }
    }

    /// A state's identity, to tell when two paths reach the same one: a hash of everything a
    /// control changes, so the thousands of states explored are remembered in a few bytes each.
    fn state_key(app: &App) -> u64 {
        hash_of(&format!("{:?}{:?}{:?}{}", app.pipeline_state, app.form, app.filter_state, app.active_tab))
    }

    fn hash_of(text: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        h.finish()
    }

    fn clone_app(app: &App) -> App {
        State::of(app).app()
    }

    /// `f` of each item, in order, worked out on several threads (each with its own `init()`):
    /// `RUST_TEST_THREADS` of them when it is set, else up to four, so the test's memory stays
    /// bounded on a machine with many cores.
    fn par_map<T: Send, R: Send, S>(items: Vec<T>, init: impl Fn() -> S + Sync, f: impl Fn(&mut S, T) -> R + Sync) -> Vec<R> {
        let wanted = std::env::var("RUST_TEST_THREADS").ok().and_then(|n| n.parse::<usize>().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()).min(4));
        let threads = wanted.min(items.len()).max(1);
        let queue = std::sync::Mutex::new(items.into_iter().enumerate());
        let mut out: Vec<(usize, R)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads).map(|_| scope.spawn(|| {
                let mut state = init();
                let mut done = Vec::new();
                loop {
                    let next = queue.lock().unwrap().next();
                    let Some((i, item)) = next else { break };
                    done.push((i, f(&mut state, item)));
                }
                done
            })).collect();
            workers.into_iter().flat_map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
        });
        out.sort_unstable_by_key(|(i, _)| *i);
        out.into_iter().map(|(_, r)| r).collect()
    }

    /// One select or checkbox change.
    #[derive(Clone, Copy)]
    enum Change {
        Select(&'static str, usize),
        Toggle(&'static str),
    }

    impl Change {
        /// The changed state, or `None` for a checkbox the TUI refuses to change.
        fn apply(self, app: &App) -> Option<App> {
            let mut a = clone_app(app);
            match self {
                Change::Select(field, i) => a.set_field_select(field, i),
                Change::Toggle(field) => {
                    a.toggle_field(field);
                    if a.error_message.is_some() { return None; }
                }
            }
            Some(a)
        }
    }

    /// One change to check against the state it is made in: a select's option, a checkbox (as
    /// toggled), or a parameter's value.
    enum Check {
        Select(&'static str, usize),
        Toggle(&'static str, Box<State>),
        Param(&'static str),
    }

    /// Every state worth looking at, with its rows, handed to `visit` in turn: start from the
    /// defaults in each tab mode, and keep any select/toggle change that reveals a row not seen
    /// yet, until nothing new turns up. Nothing is kept of a state once it is visited, but its
    /// key: the caller keeps what it needs.
    fn explore(mut visit: impl FnMut(&App, Vec<PipelineRow>)) {
        type Seen = (&'static str, char, usize);
        // A row counts as seen per inversion algorithm: the same row (an inner inversion's
        // parameters under QSMART, say) can mean a different flag under another one. A checkbox
        // counts as seen per value too: a state where a forced output is free (and so can be
        // checked) is new even when its rows are not.
        let mut seen: std::collections::HashSet<Seen> = Default::default();
        let seen_keys = |app: &App, rows: &[PipelineRow]| -> Vec<Seen> {
            let alg = app.pipeline_state.qsm_algorithm;
            rows.iter().filter_map(row_key).map(|(field, kind)| {
                let kind = if kind == 't' && app.field_toggle(field) { 'T' } else { kind };
                (field, kind, alg)
            }).collect()
        };
        // The defaults, plus the starts whose rows only a non-row control reveals: the
        // multi-orientation settings (a grouping pattern on the Input tab, COSMOS or STI) and the
        // inputs of the relaxometry maps and statistics.
        type Start = fn(&mut App);
        let starts: &[Start] = &[
            |_| {},
            |a| a.filter_state.orientation_group = "acq".into(),
            |a| { a.filter_state.orientation_group = "acq".into(); a.set_field_select("orientation_algorithm", 1); },
            |a| { a.form.settings.set("pipeline.do_r2primemap", true); a.form.settings.set("pipeline.do_r2map", true); a.form.settings.set("pipeline.do_analysis", true); },
        ];
        let start_app = |i: usize| {
            let mut app = App::new();
            // What a run needs before it can be built in either mode.
            app.form.bids_dir = "/bids".into();
            app.form.slurm_account = "acct".into();
            starts[i](&mut app);
            app
        };
        // Every state reached, as the change that reached it from its parent (or the start it
        // is): a few bytes each, where the states themselves are tens of kilobytes. Only the
        // states whose changes were explored (the parents) are kept whole, and a state is
        // rebuilt from its parent when it is looked at.
        let mut nodes: Vec<Result<usize, (u32, Change)>> = Vec::new();
        let mut parents: std::collections::HashMap<u32, State> = Default::default();
        let rebuild = |nodes: &[Result<usize, (u32, Change)>], parents: &std::collections::HashMap<u32, State>, i: u32| -> App {
            match nodes[i as usize] {
                Ok(start) => start_app(start),
                Err((parent, change)) => change.apply(&parents[&parent].app()).expect("a change made once"),
            }
        };
        // The states already looked at. Different paths reach the same state (a select set back,
        // two toggles in either order); a repeat shows only rows its first visit marked seen.
        let mut visited: std::collections::HashSet<u64> = Default::default();
        // The states reached and not yet looked at, last first: each by its key, its node and
        // what it would mark seen (as numbers: the [`Seen`]s interned). A state reached again
        // replaces its earlier entry (`None` where it was): the later one would be looked at
        // first and the earlier then skipped, so the order states are looked at in is unchanged.
        type Entry = Option<(u64, u32, Box<[u16]>)>;
        let mut frontier: Vec<Entry> = Vec::new();
        let mut queued: std::collections::HashMap<u64, usize> = Default::default();
        let mut ids: std::collections::HashMap<Seen, u16> = Default::default();
        let mut by_id: Vec<Seen> = Vec::new();
        fn intern(ids: &mut std::collections::HashMap<Seen, u16>, by_id: &mut Vec<Seen>, keys: Vec<Seen>) -> Box<[u16]> {
            keys.into_iter().map(|k| *ids.entry(k).or_insert_with(|| { by_id.push(k); (by_id.len() - 1) as u16 })).collect()
        }
        fn push(frontier: &mut Vec<Entry>, queued: &mut std::collections::HashMap<u64, usize>, key: u64, node: u32, keys: Box<[u16]>) {
            if let Some(at) = queued.insert(key, frontier.len()) {
                frontier[at] = None;
            }
            frontier.push(Some((key, node, keys)));
        }
        for i in 0..starts.len() {
            nodes.push(Ok(i));
            let app = start_app(i);
            let keys = intern(&mut ids, &mut by_id, seen_keys(&app, &rows(&app)));
            push(&mut frontier, &mut queued, state_key(&app), (nodes.len() - 1) as u32, keys);
        }
        while let Some(entry) = frontier.pop() {
            let Some((key, node, keys)) = entry else { continue };
            queued.remove(&key);
            if !visited.insert(key) {
                continue;
            }
            let mut fresh = false;
            for &k in keys.iter() {
                fresh |= seen.insert(by_id[k as usize]);
            }
            if !fresh { continue; }
            let app = rebuild(&nodes, &parents, node);
            let rows = rows(&app);
            let changes: Vec<Change> = rows.iter().flat_map(|r| match r {
                PipelineRow::AlgoSelect { field, options, .. } => {
                    let current = app.field_select(field);
                    (0..options.len()).filter(|&i| i != current).map(|i| Change::Select(field, i)).collect()
                }
                PipelineRow::Toggle { field, .. } => vec![Change::Toggle(field)],
                _ => vec![],
            }).collect();
            let state = State::of(&app);
            let next = par_map(changes, || state.app(), |base, change| {
                let a = change.apply(base)?;
                let key = state_key(&a);
                // A state already looked at, or one that would mark nothing new seen (`seen` only
                // grows, so it would mark nothing new later either), would be skipped: no need to
                // keep it.
                if visited.contains(&key) { return None; }
                let keys = seen_keys(&a, &self::rows(&a));
                if keys.iter().all(|k| seen.contains(k)) { return None; }
                Some((key, change, keys))
            });
            let mut any = false;
            for (key, change, keys) in next.into_iter().flatten() {
                any = true;
                nodes.push(Err((node, change)));
                let keys = intern(&mut ids, &mut by_id, keys);
                push(&mut frontier, &mut queued, key, (nodes.len() - 1) as u32, keys);
            }
            if any { parents.insert(node, state); }
            visit(&app, rows);
        }
    }

    /// The run-level arguments of the active mode (the pipeline's part is the config's).
    fn run_level(app: &App) -> String {
        if app.form.execution_mode == 1 {
            let mut a = build_slurm_args(app).unwrap();
            a.pipeline = Default::default();
            format!("{a:?}")
        } else {
            let mut a = build_run_args(app).unwrap();
            a.pipeline = Default::default();
            format!("{a:?}")
        }
    }

    /// What a state runs and shows: its pipeline config (also as data, so a value that changes
    /// nothing reads as no effect) and its generated command. Worked out once per state, however
    /// many changes are checked against it.
    struct Outcome {
        config: PipelineConfig,
        data: serde_json::Value,
        shown: String,
    }

    impl Outcome {
        fn of(app: &App) -> Self {
            let config = config_from_app(app).unwrap();
            Outcome { data: serde_json::to_value(&config).unwrap(), shown: build_command_string(app), config }
        }
    }

    #[test]
    #[ignore = "slow: runs in CI; locally `cargo test -- --ignored every_run_flag_is_reachable_from_the_tui`"]
    fn every_run_flag_is_reachable_from_the_tui() {
        let mut covered: BTreeSet<String> = BTreeSet::new();
        // Problem -> the first change that showed it.
        let mut problems: BTreeMap<String, String> = BTreeMap::new();
        // A row is checked once per combination of the selections that change what it feeds.
        let mut checked: std::collections::HashSet<(&'static str, char, [usize; 4], [bool; 2])> = Default::default();
        let mut cli = <crate::cli::Cli as clap::CommandFactory>::command();
        cli.build();
        // Which rows of each state are checked (each with its toggled state, for a checkbox):
        // decided in order, so each combination is checked where it is first reached.
        type Rows = Vec<(PipelineRow, Option<State>)>;
        let mut jobs: Vec<(State, Rows)> = Vec::new();
        explore(|app, rows| {
            let ps = &app.pipeline_state;
            let qsmart = crate::tui::app::QSM_ALGO_OPTIONS[ps.qsm_algorithm] == "qsmart";
            let context = (
                [ps.qsm_algorithm, if qsmart { app.field_select("qsmart_inversion") } else { 0 },
                 if app.field_toggle("do_chi_separation") { app.field_select("separation_algorithm") } else { 0 },
                 app.form.execution_mode],
                [app.field_toggle("motion_correction"), app.field_toggle("do_chi_separation")],
            );
            let mut todo = Vec::new();
            for r in rows {
                let Some((field, kind)) = row_key(&r) else { continue };
                // The run form's settings (outputs, execution) do not feed the reconstruction:
                // they are checked per separation method and mode, not per inversion.
                let ctx = if app.form.text(field).is_some() || app.form.flag(field).is_some() || app.form.choice(field).is_some() {
                    ([0, 0, context.0[2], context.0[3]], [false, context.1[1]])
                } else {
                    context
                };
                let key = (field, kind, ctx.0, ctx.1);
                if checked.contains(&key) { continue; }
                // A checkbox another option needs refuses to be turned off (and says why), and
                // ticking it is the required change the run makes anyway, which changes nothing it
                // does: it is checked where it is free instead.
                let toggled = match r {
                    PipelineRow::Toggle { field, .. } if !app.field_toggle(field) && app.output_made(field) => continue,
                    PipelineRow::Toggle { field, .. } => match Change::Toggle(field).apply(app) {
                        Some(a) => Some(State::of(&a)),
                        None => continue,
                    },
                    _ => None,
                };
                checked.insert(key);
                todo.push((r, toggled));
            }
            if !todo.is_empty() {
                jobs.push((State::of(app), todo));
            }
        });
        // The pipeline's command per config (as data), and what each command parses back to:
        // generating and parsing are most of a check's cost, and many changes give a config or a
        // command already seen.
        let commands: std::sync::Mutex<std::collections::HashMap<u64, String>> = Default::default();
        let round_trips: std::sync::Mutex<std::collections::HashMap<u64, String>> = Default::default();
        let cached = |cache: &std::sync::Mutex<std::collections::HashMap<u64, String>>, key: u64, make: &mut dyn FnMut() -> String| {
            if let Some(v) = cache.lock().unwrap().get(&key) { return v.clone(); }
            let v = make();
            cache.lock().unwrap().insert(key, v.clone());
            v
        };
        // What each state runs and shows, worked out once however many changes are checked
        // against it.
        let bases = par_map(jobs.iter().map(|(s, _)| s).collect(), || (), |_, s| Outcome::of(&s.app()));
        // Every change, in order, each checked on its own (on every core).
        let mut changes: Vec<(usize, Check)> = Vec::new();
        for (j, (state, todo)) in jobs.iter().enumerate() {
            let base = state.app();
            for (r, toggled) in todo {
                match r {
                    PipelineRow::AlgoSelect { field, options, .. } => {
                        let current = base.field_select(field);
                        // "custom" names a hand-edited mask; choosing it changes nothing by itself.
                        let custom = |i: usize| *field == "mask_preset" && i == *crate::tui::app::MASK_PRESET_CUSTOM;
                        changes.extend((0..options.len()).filter(|&i| i != current && !custom(i)).map(|i| (j, Check::Select(field, i))));
                    }
                    PipelineRow::Toggle { field, .. } => changes.push((j, Check::Toggle(field, Box::new(toggled.clone().unwrap())))),
                    PipelineRow::Param { field, .. } => changes.push((j, Check::Param(field))),
                    _ => {}
                }
            }
        }
        let results = par_map(changes, || {
            let mut cli = <crate::cli::Cli as clap::CommandFactory>::command();
            cli.build();
            cli
        }, |cli, (j, change)| {
            let base = jobs[j].0.app();
            let (what, app) = match change {
                Check::Select(field, i) => (format!("{field}={i}"), Change::Select(field, i).apply(&base).unwrap()),
                Check::Toggle(field, toggled) => (format!("toggle {field}"), toggled.app()),
                Check::Param(field) => {
                    let mut a = clone_app(&base);
                    // Paths and names rather than numbers: any non-empty value is a change.
                    let was = a.field_text(field).into_owned();
                    let mut v = match (field, was.as_str()) {
                        ("source_dicom" | "dicom_outputs" | "slurm_account" | "slurm_partition", _) => "x1".to_string(),
                        // Empty is the whole volume; a patch is three numbers.
                        ("sep_susepnet_patch" | "sep_chisepnet_frame_patch", "") => "64 64 32".to_string(),
                        // SynthSeg's window is at least 64 voxels.
                        ("synthseg_patch", "") => "96".to_string(),
                        (_, v) => bumped(v, 1),
                    };
                    *a.field_text_mut(field).unwrap_or_else(|| panic!("{field} not editable")) = v.clone();
                    // A value at the top of its range moves down instead.
                    if config_from_app(&a).is_err() && config_from_app(&base).is_ok() {
                        v = bumped(&was, -1);
                        *a.field_text_mut(field).unwrap() = v.clone();
                    }
                    (format!("{field}={v}"), a)
                }
            };
            let (before, after) = (&bases[j], Outcome::of(&app));
            let mut found: Vec<(String, String)> = Vec::new();
            // The same run: the same config and the same run-level arguments.
            if after.data == before.data && run_level(&app) == run_level(&base) {
                found.push((format!("no effect on the run: {}", what.split('=').next().unwrap()), what.clone()));
            } else if before.shown == after.shown {
                let d = diff(&before.config, &after.config);
                found.push((format!("not in the command: {d:?}"), what.clone()));
            }
            let cmd = cached(&commands, hash_of(&after.data.to_string()), &mut || qsmxt_engine::generate_command(&after.config));
            let back = cached(&round_trips, hash_of(&cmd), &mut || qsmxt_engine::generate_command(&round_trip(cli, &after.config)));
            if back != cmd {
                let (a, b) = (flags_of(&cmd), flags_of(&back));
                found.push((format!("command does not reproduce itself: {:?}", a.symmetric_difference(&b).collect::<Vec<_>>()), what));
            }
            (j, found, flags_of(&after.shown))
        });
        let mut last = None;
        for (j, found, flags) in results {
            for (problem, what) in found {
                problems.entry(problem).or_insert(what);
            }
            // Both ends are reachable states: a toggle explored from its checked side turns the
            // flag off, and the flag is still one the TUI emits.
            if last != Some(j) {
                covered.extend(flags_of(&bases[j].shown));
                last = Some(j);
            }
            covered.extend(flags);
        }

        // The controls that are not rows: the Input tab's filters and grouping, and the mask
        // editor. Each is (what, setup, change): the change is
        // checked against the state after `setup` alone.
        type Edit = fn(&mut App);
        let surfaces: &[(&str, Edit, Edit)] = &[
            ("orientation_group", |_| {}, |a| a.filter_state.orientation_group = "acq".into()),
            ("orientation_algorithm", |a| a.filter_state.orientation_group = "acq".into(), |a| a.set_field_select("orientation_algorithm", 1)),
            ("orientation_lambda", |a| a.filter_state.orientation_group = "acq".into(), |a| *a.field_text_mut("orientation_lambda").unwrap() = "0.5".into()),
            ("orientation_reference", |a| a.filter_state.orientation_group = "acq".into(), |a| *a.field_text_mut("orientation_reference").unwrap() = "Neutral".into()),
            ("bet_smoothness", |a| a.set_field_select("mask_preset", 1), |a| *a.field_text_mut("bet_smoothness").unwrap() = "2".into()),
            ("bet_gradient_threshold", |a| a.set_field_select("mask_preset", 1), |a| *a.field_text_mut("bet_gradient_threshold").unwrap() = "0.3".into()),
            ("bet_iterations", |a| a.set_field_select("mask_preset", 1), |a| *a.field_text_mut("bet_iterations").unwrap() = "500".into()),
            ("bet_subdivisions", |a| a.set_field_select("mask_preset", 1), |a| *a.field_text_mut("bet_subdivisions").unwrap() = "3".into()),
            ("bet_orientation", |a| a.set_field_select("mask_preset", 1), |a| a.set_field_select("bet_orientation", 1)),
            ("cosmos_magnitude_weighting", |a| a.filter_state.orientation_group = "acq".into(), |a| a.form.settings.set("multi_orientation.magnitude_weighting", true)),
            ("cosmos_tol", |a| { a.filter_state.orientation_group = "acq".into(); a.form.settings.set("multi_orientation.magnitude_weighting", true); }, |a| *a.field_text_mut("cosmos_tol").unwrap() = "1e-4".into()),
            ("cosmos_max_iter", |a| { a.filter_state.orientation_group = "acq".into(); a.form.settings.set("multi_orientation.magnitude_weighting", true); }, |a| *a.field_text_mut("cosmos_max_iter").unwrap() = "250".into()),
            ("mask sections", |_| {}, |a| {
                let mut m = a.mask_mut();
                let mut extra = m.sections[0].clone();
                extra.input = crate::pipeline::config::MaskingInput::Magnitude;
                m.sections.push(extra);
            }),
            ("mask combine", |a| {
                let mut m = a.mask_mut();
                let extra = m.sections[0].clone();
                m.sections.push(extra);
            }, |a| a.mask_mut().toggle_combine(0)),
            ("mask refinements", |a| {
                let mut m = a.mask_mut();
                let extra = m.sections[0].clone();
                m.sections.push(extra);
            }, |a| a.mask_mut().refinements.push(crate::pipeline::config::MaskOp::Erode { iterations: 1 })),
            ("masking input", |a| a.set_field_select("mask_preset", 1), |a| {
                for s in &mut a.mask_mut().sections { s.input = crate::pipeline::config::MaskingInput::MagnitudeLast; }
            }),
            ("two-pass sections", |a| a.form.settings.set("masking.two_pass", true), |a| {
                a.mask_mut().two_pass_sections[0].refinements.push(crate::pipeline::config::MaskOp::Erode { iterations: 2 });
            }),
            ("two-pass combine", |a| {
                a.form.settings.set("masking.two_pass", true);
                let mut m = a.mask_mut();
                let extra = m.two_pass_sections[0].clone();
                m.two_pass_sections.push(extra);
            }, |a| a.mask_mut().toggle_combine(crate::tui::app::MASK_TWO_PASS_BASE + 1)),
            // Run-level settings: not in the pipeline config, so compared on the run's arguments.
            ("config", |_| {}, |a| a.form.config_file = "c.toml".into()),
            ("include", |_| {}, |a| a.filter_state.include_pattern = "sub-1*".into()),
            ("exclude", |_| {}, |a| a.filter_state.exclude_pattern = "*pre*".into()),
            ("num_echoes", |_| {}, |a| a.filter_state.num_echoes = "2".into()),
            // The memory-limit row's third state (auto, a number, none).
            ("no_mem_limit", |_| {}, |a| a.form.mem_limit_gb = "none".into()),
            // A recommendation dismissed in the pop-up (MEDI-mSMV without its prefilter is
            // discouraged).
            ("dismiss_recommendation", |a| {
                let at = |list: &[&str], v: &str| list.iter().position(|o| *o == v).unwrap();
                a.set_field_select("qsm_algorithm", at(&crate::tui::app::QSM_PICKER_OPTIONS, "medi"));
                a.form.settings.set("inversion.medi.msmv", true);
                a.form.settings.set("inversion.medi.msmv_prefilter", false);
            }, |a| a.pipeline_state.dismissed.push(qsmxt_engine::Dismissal { rule: "medi-msmv.prefilter".into(), chosen: "false".into() })),
        ];
        for (what, setup, change) in surfaces {
            let mut base = App::new();
            base.form.bids_dir = "/bids".into();
            setup(&mut base);
            let mut a = App::new();
            a.form.bids_dir = "/bids".into();
            setup(&mut a);
            change(&mut a);
            let (shown_before, shown) = (build_command_string(&base), build_command_string(&a));
            let run_changed = format!("{:?}", build_run_args(&base).unwrap()) != format!("{:?}", build_run_args(&a).unwrap());
            if !run_changed {
                problems.entry(format!("no effect on the run: {what}")).or_insert(what.to_string());
            } else if shown_before == shown {
                problems.entry(format!("not in the command: {what}")).or_insert(what.to_string());
            }
            let c = config_from_app(&a).unwrap();
            let (cmd, back) = (qsmxt_engine::generate_command(&c), qsmxt_engine::generate_command(&round_trip(&mut cli, &c)));
            if cmd != back {
                problems.entry(format!("command does not reproduce itself: {what}")).or_insert(what.to_string());
            }
            covered.extend(flags_of(&shown));
        }

        // Flags the TUI does not emit as such, each for a stated reason.
        let not_emitted: &[(&str, &str)] = &[
            // The TUI edits BET per mask step, so its settings travel inside `--mask` specs.
            ("--bet-fractional-intensity", "per BET step, in --mask"),
            ("--bet-voxel-scale", "per BET step, in --mask"),
            // The direction that restates QSMxT's default, so the command never needs it.
            ("--inhomogeneity-correction", "restates the default"),
            ("--no-romeo-individual", "restates the default"),
            ("--no-axial-resampling", "restates the default"),
            ("--no-romeo-correct-global", "restates the default"),
            ("--romeo-phase-linearity", "restates the default (for a pipeline that turns it off)"),
            ("--medi-smv", "restates the default"),
            ("--medi-smv-shrink-mask", "restates the default"),
            ("--no-romeo-mag-weight", "restates the default (kept for compatibility)"),
            // The per-run half of a distributed run: the TUI's SLURM mode generates it (through
            // `qsmxt slurm`) and its local mode is a whole run.
            ("--no-dataset-steps", "worker mode, generated by qsmxt slurm"),
            // The TUI resolves recommendations in its pop-up, before the run.
            ("--accept-recommendations", "the pop-up applies them"),
            ("--accept-reference-settings", "the pop-up applies them"),
            // Chosen on the Pipeline tab, which no settings row covers.
            ("--pipeline", "the Pipeline tab"),
            // Only against a pipeline that turns it on; QSMxT's default is off.
            ("--no-laplacian-unmasked", "restates the default"),
            // Likewise against the UK Biobank pipeline, which keeps the field outside the mask.
            ("--no-laplacian-keep-outside", "restates the default"),
            // Likewise against the HEIDI pipeline, which turns ITK's float arithmetic on.
            ("--no-heidi-denoise-reference-itk", "restates the default"),
            // Likewise against the STI Suite pipeline, which polishes the background-removal mask.
            ("--no-bf-polish-mask", "restates the default"),
            // Likewise against the UK Biobank pipeline, which trims the mask by phase reliability.
            ("--no-bf-ukb-mask-trim", "restates the default"),
        ];
        for (flag, why) in not_emitted {
            assert!(!covered.contains(*flag), "{flag} is allow-listed ({why}) but the TUI emits it: drop it from the list");
            covered.insert(flag.to_string());
        }
        let missing: Vec<_> = run_flags().into_iter().filter(|f| !covered.contains(f)).collect();
        eprintln!("MISSING ({}):\n{}", missing.len(), missing.join(" "));
        eprintln!("PROBLEMS ({}):\n{}", problems.len(),
            problems.iter().map(|(k, v)| format!("  {k}   <- {v}")).collect::<Vec<_>>().join("\n"));
        assert!(missing.is_empty() && problems.is_empty());
    }
}
