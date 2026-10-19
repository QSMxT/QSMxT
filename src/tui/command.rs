use std::path::PathBuf;

use crate::cli::*;
use crate::pipeline::config::{PipelineConfig, QsmAlgorithm};
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
    match config_from_app(app) {
        Err(e) => parts.push(format!("<invalid setting: {e}>")),
        Ok(config) => {
        let lib_cmd = qsmxt_engine::generate_command(&config);
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
        push_dicom_export(&mut parts, form);
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
        push_dicom_export(&mut parts, form);
    }

    parts.join(" ")
}

/// DICOM export's run settings — the same in both modes (under SLURM each run's job exports).
fn push_dicom_export(parts: &mut Vec<String>, form: &super::app::RunForm) {
    if form.export_dicom {
        if let Some(p) = parse_optional_path(&form.source_dicom) {
            parts.push(format!("--source-dicom {}", p.display()));
        }
        if let Some(o) = parse_dicom_outputs(&form.dicom_outputs) {
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
        bids_dir: expand_tilde(&form.bids_dir),
        output_dir: if form.output_dir.is_empty() { None } else { Some(expand_tilde(&form.output_dir)) },
        config: parse_optional_path(&form.config_file),
        include: app.filter_state.get_include_exclude().0,
        exclude: app.filter_state.get_include_exclude().1,
        num_echoes: parse_optional_usize(&app.filter_state.num_echoes),
        n_procs: parse_optional_usize(&form.n_procs),
        source_dicom: if form.export_dicom { parse_optional_path(&form.source_dicom) } else { None },
        dicom_outputs: if form.export_dicom { parse_dicom_outputs(&form.dicom_outputs) } else { None },
        dry: form.dry_run,
        debug: form.debug,
        mem_limit_gb: form.mem_limit().0,
        no_mem_limit: form.mem_limit().1,
        force: form.force,
        clean_intermediates: form.clean_intermediates,
        // The worker mode is what `qsmxt slurm` generates; the TUI's local run is a whole run.
        no_dataset_steps: false,
        pipeline: pipeline_args_from_app(app),
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
/// The SWI unwrapping each entry of the TUI's SWI "Unwrapping" select runs, by index. A test
/// holds the select's labels to these.
pub const SWI_UNWRAP_ARGS: [crate::cli::SwiUnwrapArg; 3] = {
    use crate::cli::SwiUnwrapArg as U;
    [U::Laplacian, U::LaplacianSlice, U::Romeo]
};

/// The SWI scaling each entry of the TUI's SWI "Scaling" select runs, by index. A test holds
/// the select's labels to these.
pub const SWI_SCALING_ARGS: [crate::cli::SwiScalingArg; 5] = {
    use crate::cli::SwiScalingArg as S;
    [S::Tanh, S::NegativeTanh, S::Positive, S::Negative, S::Triangular]
};

pub const QSM_ALGO_ARGS: [QsmAlgorithmArg; 31] = [
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
        QsmAlgorithmArg::Lsqr,
        QsmAlgorithmArg::Heidi,
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

/// The algorithm the inversion row is currently showing.
///
/// Reads it out of [`QSM_ALGO_ARGS`] rather than a second list of its own: the row's index means
/// whatever that list says it means, and a parallel copy here would silently disagree the moment
/// an algorithm is inserted. Out-of-range indices fall back to the first entry instead of
/// panicking, so a stale index from a saved form can never take the TUI down.
pub fn selected_qsm_algorithm(ps: &super::app::PipelineFormState) -> QsmAlgorithm {
    let arg = QSM_ALGO_ARGS.get(ps.qsm_algorithm).copied().unwrap_or_else(default_qsm_algorithm_arg);
    crate::pipeline::config::qsm_algorithm_arg_to_config(arg)
}

pub fn pipeline_args_from_app(app: &App) -> PipelineArgs {
    let form = &app.form;
    let ps = &app.pipeline_state;

    let qsm_options = QSM_ALGO_ARGS;
    let unwrap_options = [UnwrapAlgorithmArg::Romeo, UnwrapAlgorithmArg::Laplacian];
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
    let sep_options = [
        SeparationAlgorithmArg::R2starQsm,
        SeparationAlgorithmArg::Decompose,
        SeparationAlgorithmArg::ChiSepIlsqr,
        SeparationAlgorithmArg::ChiSepMedi,
        SeparationAlgorithmArg::Wavesep,
        SeparationAlgorithmArg::HcChisep,
        SeparationAlgorithmArg::SusepNet,
        SeparationAlgorithmArg::ChiSepNet,
    ];

    // Multi-orientation settings live on the Input tab, next to the grouping they depend on.
    let (orientation_group, orientation_algorithm, orientation_lambda) =
        app.filter_state.orientation_args();
    // Settings that only apply in some configurations are passed only then, matching the rows
    // the TUI shows: a hidden value must not change the run.
    let grouped = orientation_group.is_some();
    let weighted_cosmos = grouped && orientation_algorithm == 0 && ps.cosmos_magnitude_weighting;
    // Rigid registration serves motion correction and the co-registration of orientations.
    let registration = ps.motion_correction || (grouped && ps.orientation_registration);
    let inputs = app.input_relevance();

    PipelineArgs {
        // Bounds-checked rather than indexed: the option list shrinks with the build (see #227).
        qsm_algorithm: Some(qsm_options.get(ps.qsm_algorithm).copied().unwrap_or_else(default_qsm_algorithm_arg)),
        multi_orientation_algorithm: orientation_group.as_ref().map(|_| {
            if orientation_algorithm == 1 {
                crate::cli::MultiOrientAlgorithmArg::Sti
            } else {
                crate::cli::MultiOrientAlgorithmArg::Cosmos
            }
        }),
        multi_orientation_lambda: orientation_lambda,
        orientation_group,
        multi_orientation_force: grouped && ps.multi_orientation_force,
        no_orientation_registration: grouped && !ps.orientation_registration,
        cosmos_magnitude_weighting: weighted_cosmos,
        cosmos_tol: if weighted_cosmos { parse_optional_f64(&ps.cosmos_tol) } else { None },
        cosmos_max_iter: if weighted_cosmos { parse_optional_usize(&ps.cosmos_max_iter) } else { None },
        unwrapping_algorithm: Some(unwrap_options[ps.unwrapping_algorithm]),
        // Only when the inversion has a background-removal stage: otherwise the choice has no
        // effect, and iQFM there is refused (#276).
        bf_algorithm: ps.inversion_needs_bg_removal().then(|| bf_options[ps.bf_algorithm]),
        masking_input: None,
        phase_offset_removal: Some(ps.phase_offset_removal),
        phase_offset_sigma: parse_optional_f64_vec3(&ps.phase_offset_sigma),
        coil_combination_sigma: parse_optional_f64_vec3(&ps.coil_combination_sigma),
        // Part of HIP phase offset removal, which only ROMEO unwrapping runs.
        bipolar_correction: ps.unwrapping_algorithm == 0 && ps.phase_offset_removal && ps.bipolar_correction,
        // Only when changed from the default, so a default form still yields a bare command.
        b0_estimation: match ps.b0_estimation {
            1 => Some(B0EstimationArg::LinearFit),
            2 => Some(B0EstimationArg::RobustFit),
            _ => None,
        },
        // The echo weights belong to the weighted average; the fits do not use them.
        b0_weight_type: [B0WeightTypeArg::PhaseSNR, B0WeightTypeArg::PhaseVar, B0WeightTypeArg::Average,
                         B0WeightTypeArg::TEs, B0WeightTypeArg::Mag, B0WeightTypeArg::AssumedDecay]
            .get(ps.b0_weight_type).copied().filter(|&w| ps.b0_estimation == 0 && w != B0WeightTypeArg::PhaseSNR),
        b0_weight_assumed_t2star: if ps.b0_estimation == 0 && ps.b0_weight_type == 5 { parse_optional_f64(&ps.b0_weight_assumed_t2star_ms) } else { None },
        // Only when Laplacian unwrapping is selected, and only when changed from the default.
        laplacian_solver: (ps.unwrapping_algorithm == 1 && ps.laplacian_solver == 1)
            .then_some(crate::cli::LaplacianSolverArg::Fft),
        laplacian_fft_pad: if ps.unwrapping_algorithm == 1 && ps.laplacian_solver == 1 {
            parse_pad3(&ps.laplacian_fft_pad)
                .filter(|&p| p != qsmxt_engine::default_laplacian_fft_pad())
                .map(|p| if p[0] == p[1] && p[1] == p[2] { vec![p[0]] } else { p.to_vec() })
        } else { None },
        // BET is edited per mask step in the TUI, so its settings travel inside the sections.
        bet_fractional_intensity: None,
        // The TUI's voxel scale lives on the mask recipe's BET step, so it travels in
        // `mask_sections_cli` below rather than as a recipe-wide flag.
        bet_voxel_scale: None,
        bet_smoothness: if ps.uses_bet() { parse_optional_f64(&ps.bet_smoothness) } else { None },
        bet_gradient_threshold: if ps.uses_bet() { parse_optional_f64(&ps.bet_gradient_threshold) } else { None },
        bet_iterations: if ps.uses_bet() { parse_optional_usize(&ps.bet_iterations) } else { None },
        bet_subdivisions: if ps.uses_bet() { parse_optional_usize(&ps.bet_subdivisions) } else { None },
        qsm_reference: Some(crate::tui::app::qsm_reference_spec(ps.qsm_reference)),
        rts_params: crate::cli::RtsParamArgs {
            rts_delta: parse_optional_f64(&ps.rts_delta),
            rts_mu: parse_optional_f64(&ps.rts_mu),
            rts_tol: parse_optional_f64(&ps.rts_tol),
            rts_rho: parse_optional_f64(&ps.rts_rho),
            rts_max_iter: parse_optional_usize(&ps.rts_max_iter),
            rts_lsmr_iter: parse_optional_usize(&ps.rts_lsmr_iter),
        },
        tv_params: crate::cli::TvParamArgs {
            tv_lambda: parse_optional_f64(&ps.tv_lambda),
            tv_rho: parse_optional_f64(&ps.tv_rho),
            tv_tol: parse_optional_f64(&ps.tv_tol),
            tv_max_iter: parse_optional_usize(&ps.tv_max_iter),
        },
        tkd_params: crate::cli::TkdParamArgs {
            tkd_threshold: parse_optional_f64(&ps.tkd_threshold),
        },
        tsvd_params: crate::cli::TsvdParamArgs {
            tsvd_threshold: parse_optional_f64(&ps.tsvd_threshold),
        },
        tgv_params: crate::cli::TgvParamArgs {
            tgv_iterations: parse_optional_usize(&ps.tgv_iterations),
            tgv_erosions: parse_optional_usize(&ps.tgv_erosions),
            tgv_alpha1: parse_optional_f64(&ps.tgv_alpha1),
            tgv_alpha0: parse_optional_f64(&ps.tgv_alpha0),
            tgv_step_size: parse_optional_f64(&ps.tgv_step_size),
            tgv_tol: parse_optional_f64(&ps.tgv_tol),
        },
        tikhonov_params: crate::cli::TikhonovParamArgs {
            tikhonov_lambda: parse_optional_f64(&ps.tikhonov_lambda),
        },
        nltv_params: crate::cli::NltvParamArgs {
            nltv_lambda: parse_optional_f64(&ps.nltv_lambda),
            nltv_mu: parse_optional_f64(&ps.nltv_mu),
            nltv_tol: parse_optional_f64(&ps.nltv_tol),
            nltv_max_iter: parse_optional_usize(&ps.nltv_max_iter),
            nltv_newton_iter: parse_optional_usize(&ps.nltv_newton_iter),
        },
        medi_params: crate::cli::MediParamArgs {
            medi_lambda: parse_optional_f64(&ps.medi_lambda),
            medi_merit: Some(ps.medi_merit),
            medi_smv: ps.medi_smv,
            no_medi_smv: !ps.medi_smv,
            medi_smv_radius: parse_optional_f64(&ps.medi_smv_radius),
            medi_data_weighting: ps.medi_data_weighting.trim().parse().ok(),
            medi_percentage: parse_optional_f64(&ps.medi_percentage),
            medi_cg_tol: parse_optional_f64(&ps.medi_cg_tol),
            medi_cg_max_iter: parse_optional_usize(&ps.medi_cg_max_iter),
            medi_max_iter: parse_optional_usize(&ps.medi_max_iter),
            medi_tol: parse_optional_f64(&ps.medi_tol),
        },
        tfi_params: crate::cli::TfiParamArgs {
            tfi_lambda: parse_optional_f64(&ps.tfi_lambda),
            tfi_precond: parse_optional_f64(&ps.tfi_precond),
            tfi_merit: Some(ps.tfi_merit),
            tfi_data_weighting: ps.tfi_data_weighting.trim().parse().ok(),
            tfi_percentage: parse_optional_f64(&ps.tfi_percentage),
            tfi_cg_tol: parse_optional_f64(&ps.tfi_cg_tol),
            tfi_cg_max_iter: parse_optional_usize(&ps.tfi_cg_max_iter),
            tfi_max_iter: parse_optional_usize(&ps.tfi_max_iter),
            tfi_tol: parse_optional_f64(&ps.tfi_tol),
        },
        lsqr_params: crate::cli::LsqrParamArgs {
            lsqr_residual_weighting: parse_optional_f64(&ps.lsqr_residual_weighting),
            no_lsqr_global_offset: !ps.lsqr_fit_global_offset,
            lsqr_tol: parse_optional_f64(&ps.lsqr_tol),
            lsqr_max_iter: parse_optional_usize(&ps.lsqr_max_iter),
        },
        heidi_params: crate::cli::HeidiParamArgs {
            heidi_cone_threshold: parse_optional_f64(&ps.heidi_cone_threshold),
            heidi_gradient_threshold: parse_optional_f64(&ps.heidi_gradient_threshold),
            no_heidi_laplacian_correction: !ps.heidi_apply_laplacian_correction,
            heidi_laplacian_threshold: parse_optional_f64(&ps.heidi_laplacian_threshold),
            heidi_gradient_mask_floor: parse_optional_f64(&ps.heidi_gradient_mask_floor),
            heidi_continuation_steps: parse_optional_usize(&ps.heidi_continuation_steps),
            heidi_inner_iterations: parse_optional_usize(&ps.heidi_inner_iterations),
            heidi_mu_min: parse_optional_f64(&ps.heidi_mu_min),
            heidi_tol: parse_optional_f64(&ps.heidi_tol),
            no_heidi_denoise: !ps.heidi_denoise,
            heidi_denoise_iterations: parse_optional_usize(&ps.heidi_denoise_iterations),
            heidi_denoise_time_step: parse_optional_f64(&ps.heidi_denoise_time_step),
            heidi_denoise_conductance: parse_optional_f64(&ps.heidi_denoise_conductance),
        },
        ilsqr_params: crate::cli::IlsqrParamArgs {
            ilsqr_tol: parse_optional_f64(&ps.ilsqr_tol),
            ilsqr_max_iter: parse_optional_usize(&ps.ilsqr_max_iter),
            ilsqr_pad_mm: parse_optional_f64(&ps.ilsqr_pad_mm),
        },
        qsmart_params: crate::cli::QsmartParamArgs {
            qsmart_ilsqr_max_iter: parse_optional_usize(&ps.qsmart_ilsqr_max_iter),
            qsmart_ilsqr_pad_mm: parse_optional_f64(&ps.qsmart_ilsqr_pad_mm),
            qsmart_vasc_sphere_radius: ps.qsmart_vasc_sphere_radius.trim().parse::<i32>().ok(),
            qsmart_sdf_spatial_radius: ps.qsmart_sdf_spatial_radius.trim().parse::<i32>().ok(),
            qsmart_inversion: if selected_qsm_algorithm(ps) == QsmAlgorithm::Qsmart {
                [
                    crate::cli::QsmAlgorithmArg::Ilsqr, crate::cli::QsmAlgorithmArg::Rts,
                    crate::cli::QsmAlgorithmArg::Tv, crate::cli::QsmAlgorithmArg::Tkd,
                    crate::cli::QsmAlgorithmArg::Tsvd, crate::cli::QsmAlgorithmArg::Tikhonov,
                    crate::cli::QsmAlgorithmArg::Nltv, crate::cli::QsmAlgorithmArg::Medi,
                ].get(ps.qsmart_inversion).copied()
            } else {
                None
            },
            qsmart_sdf_sigma1_stage1: parse_optional_f64(&ps.qsmart_sdf_sigma1_stage1),
            qsmart_sdf_sigma2_stage1: parse_optional_f64(&ps.qsmart_sdf_sigma2_stage1),
            qsmart_sdf_sigma1_stage2: parse_optional_f64(&ps.qsmart_sdf_sigma1_stage2),
            qsmart_sdf_sigma2_stage2: parse_optional_f64(&ps.qsmart_sdf_sigma2_stage2),
            qsmart_sdf_lower_lim: parse_optional_f64(&ps.qsmart_sdf_lower_lim),
            qsmart_sdf_curv_constant: parse_optional_f64(&ps.qsmart_sdf_curv_constant),
            qsmart_frangi_scale_min: parse_optional_f64(&ps.qsmart_frangi_scale_min),
            qsmart_frangi_scale_max: parse_optional_f64(&ps.qsmart_frangi_scale_max),
            qsmart_frangi_scale_ratio: parse_optional_f64(&ps.qsmart_frangi_scale_ratio),
            qsmart_frangi_c: parse_optional_f64(&ps.qsmart_frangi_c),
        },
        ndi_params: crate::cli::NdiParamArgs {
            ndi_tau: parse_optional_f64(&ps.ndi_tau),
            ndi_alpha: parse_optional_f64(&ps.ndi_alpha),
            ndi_max_iter: parse_optional_usize(&ps.ndi_max_iter),
            ndi_phase_scale: parse_optional_f64(&ps.ndi_phase_scale),
        },
        fansi_params: crate::cli::FansiParamArgs {
            fansi_alpha1: parse_optional_f64(&ps.fansi_alpha1),
            fansi_mu1: parse_optional_f64(&ps.fansi_mu1),
            fansi_mu2: parse_optional_f64(&ps.fansi_mu2),
            fansi_alpha0: parse_optional_f64(&ps.fansi_alpha0),
            fansi_mu0: parse_optional_f64(&ps.fansi_mu0),
            fansi_max_iter: parse_optional_usize(&ps.fansi_max_iter),
            fansi_tol_update: parse_optional_f64(&ps.fansi_tol_update),
            fansi_tol_delta: parse_optional_f64(&ps.fansi_tol_delta),
            fansi_phase_scale: parse_optional_f64(&ps.fansi_phase_scale),
        },
        l1qsm_params: crate::cli::L1qsmParamArgs {
            l1qsm_alpha1: parse_optional_f64(&ps.l1qsm_alpha1),
            l1qsm_mu1: parse_optional_f64(&ps.l1qsm_mu1),
            l1qsm_mu2: parse_optional_f64(&ps.l1qsm_mu2),
            l1qsm_mu3: parse_optional_f64(&ps.l1qsm_mu3),
            l1qsm_lambda: parse_optional_f64(&ps.l1qsm_lambda),
            l1qsm_max_iter: parse_optional_usize(&ps.l1qsm_max_iter),
            l1qsm_tol_update: parse_optional_f64(&ps.l1qsm_tol_update),
            l1qsm_tol_delta: parse_optional_f64(&ps.l1qsm_tol_delta),
            l1qsm_phase_scale: parse_optional_f64(&ps.l1qsm_phase_scale),
        },
        whqsm_params: crate::cli::WhqsmParamArgs {
            whqsm_alpha1: parse_optional_f64(&ps.whqsm_alpha1),
            whqsm_mu1: parse_optional_f64(&ps.whqsm_mu1),
            whqsm_mu2: parse_optional_f64(&ps.whqsm_mu2),
            whqsm_beta: parse_optional_f64(&ps.whqsm_beta),
            whqsm_muh: parse_optional_f64(&ps.whqsm_muh),
            whqsm_max_iter: parse_optional_usize(&ps.whqsm_max_iter),
            whqsm_tol_update: parse_optional_f64(&ps.whqsm_tol_update),
            whqsm_tol_delta: parse_optional_f64(&ps.whqsm_tol_delta),
            whqsm_phase_scale: parse_optional_f64(&ps.whqsm_phase_scale),
        },
        hdqsm_params: crate::cli::HdqsmParamArgs {
            hdqsm_alpha_l2: parse_optional_f64(&ps.hdqsm_alpha_l2),
            hdqsm_mu1_l2: parse_optional_f64(&ps.hdqsm_mu1_l2),
            hdqsm_mu2: parse_optional_f64(&ps.hdqsm_mu2),
            hdqsm_max_iter_l1: parse_optional_usize(&ps.hdqsm_max_iter_l1),
            hdqsm_max_iter_l2: parse_optional_usize(&ps.hdqsm_max_iter_l2),
            hdqsm_tol_update: parse_optional_f64(&ps.hdqsm_tol_update),
        },
        amp_pe_params: crate::cli::AmpPeParamArgs {
            amp_pe_wave_order: parse_optional_usize(&ps.amp_pe_wave_order),
            amp_pe_nlevel: parse_optional_usize(&ps.amp_pe_nlevel),
            amp_pe_wave_pec: parse_optional_f64(&ps.amp_pe_wave_pec),
            amp_pe_simulated_te: parse_optional_f64(&ps.amp_pe_simulated_te),
            amp_pe_max_linearization_ite: parse_optional_usize(&ps.amp_pe_max_linearization_ite),
            amp_pe_tikhonov_beta: parse_optional_f64(&ps.amp_pe_tikhonov_beta),
            amp_pe_damp_rate_sig: parse_optional_f64(&ps.amp_pe_damp_rate_sig),
            amp_pe_damp_rate_par: parse_optional_f64(&ps.amp_pe_damp_rate_par),
            amp_pe_max_pe_spar_ite: parse_optional_usize(&ps.amp_pe_max_pe_spar_ite),
            amp_pe_max_pe_est_ite: parse_optional_usize(&ps.amp_pe_max_pe_est_ite),
            amp_pe_cvg_thd: parse_optional_f64(&ps.amp_pe_cvg_thd),
        },
        motion_params: crate::cli::MotionParamArgs {
            motion_correction: ps.motion_correction,
            motion_reference: if !ps.motion_correction { None } else { match ps.motion_reference {
                1 => Some("last".to_string()),
                2 => Some(ps.motion_reference_echo.trim().to_string()),
                _ => None,
            } },
            no_motion_chain: ps.motion_correction && !ps.motion_chain,
            motion_chain_levels: if ps.motion_correction { parse_optional_usize(&ps.motion_chain_levels) } else { None },
            motion_min_ncc: if ps.motion_correction { parse_optional_f64(&ps.motion_min_ncc) } else { None },
            registration_levels: if registration { parse_optional_usize(&ps.reg_levels) } else { None },
            registration_max_samples: if registration { parse_optional_usize(&ps.reg_max_samples) } else { None },
            registration_max_iterations: if registration { parse_optional_usize(&ps.reg_max_iterations) } else { None },
            registration_max_evaluations: if registration { parse_optional_usize(&ps.reg_max_evaluations) } else { None },
            registration_tolerance: if registration { parse_optional_f64(&ps.reg_tolerance) } else { None },
            registration_rotation_step_deg: if registration { parse_optional_f64(&ps.reg_rotation_step_deg) } else { None },
            registration_translation_step_mm: if registration { parse_optional_f64(&ps.reg_translation_step_mm) } else { None },
            registration_min_overlap: if registration { parse_optional_f64(&ps.reg_min_overlap) } else { None },
        },
        separation_params: crate::cli::SeparationParamArgs {
            r2star_qsm_r_const_3t: parse_optional_f64(&ps.sep_r2starqsm_r_const_3t),
            decompose_n_inner: parse_optional_usize(&ps.sep_decompose_n_inner),
            decompose_chi_bound: parse_optional_f64(&ps.sep_decompose_chi_bound),
            decompose_max_lm_iter: parse_optional_usize(&ps.sep_decompose_max_lm_iter),
            chi_sep_ilsqr_dr_pos: parse_optional_f64(&ps.sep_ilsqr_dr_pos),
            chi_sep_ilsqr_dr_neg: parse_optional_f64(&ps.sep_ilsqr_dr_neg),
            chi_sep_ilsqr_lambda1: parse_optional_f64(&ps.sep_ilsqr_lambda1),
            chi_sep_ilsqr_percentage: parse_optional_f64(&ps.sep_ilsqr_percentage),
            chi_sep_ilsqr_r2p_min: parse_optional_f64(&ps.sep_ilsqr_r2p_min),
            chi_sep_ilsqr_r2p_max: parse_optional_f64(&ps.sep_ilsqr_r2p_max),
            chi_sep_ilsqr_max_iter: parse_optional_usize(&ps.sep_ilsqr_max_iter),
            chi_sep_ilsqr_tol: parse_optional_f64(&ps.sep_ilsqr_tol),
            chi_sep_ilsqr_cg_max_iter: parse_optional_usize(&ps.sep_ilsqr_cg_max_iter),
            chi_sep_ilsqr_cg_tol: parse_optional_f64(&ps.sep_ilsqr_cg_tol),
            chi_sep_medi_lambda_para: parse_optional_f64(&ps.sep_medi_lambda_para),
            chi_sep_medi_lambda_dia: parse_optional_f64(&ps.sep_medi_lambda_dia),
            chi_sep_medi_lambda_cpl: parse_optional_f64(&ps.sep_medi_lambda_cpl),
            chi_sep_medi_dr_pos: parse_optional_f64(&ps.sep_medi_dr_pos),
            chi_sep_medi_dr_neg: parse_optional_f64(&ps.sep_medi_dr_neg),
            chi_sep_medi_percentage: parse_optional_f64(&ps.sep_medi_percentage),
            chi_sep_medi_cg_tol: parse_optional_f64(&ps.sep_medi_cg_tol),
            chi_sep_medi_cg_max_iter: parse_optional_usize(&ps.sep_medi_cg_max_iter),
            chi_sep_medi_max_iter: parse_optional_usize(&ps.sep_medi_max_iter),
            chi_sep_medi_tol: parse_optional_f64(&ps.sep_medi_tol),
            wavesep_dr_pos: parse_optional_f64(&ps.sep_wavesep_dr_pos),
            wavesep_dr_neg: parse_optional_f64(&ps.sep_wavesep_dr_neg),
            wavesep_alpha: parse_optional_f64(&ps.sep_wavesep_alpha),
            wavesep_lambda: parse_optional_f64(&ps.sep_wavesep_lambda),
            wavesep_wavelet_order: parse_optional_usize(&ps.sep_wavesep_wavelet_order),
            wavesep_max_iter: parse_optional_usize(&ps.sep_wavesep_max_iter),
            wavesep_tol: parse_optional_f64(&ps.sep_wavesep_tol),
            hc_chisep_dr_pos_3t: parse_optional_f64(&ps.sep_hcchisep_dr_pos_3t),
            hc_chisep_bin_hz: parse_optional_f64(&ps.sep_hcchisep_bin_hz),
        },
        vsharp_params: crate::cli::VsharpParamArgs {
            vsharp_deconvolve: Some(ps.vsharp_deconvolve),
            vsharp_threshold: parse_optional_f64(&ps.vsharp_threshold),
            vsharp_max_radius: parse_optional_f64(&ps.vsharp_max_radius),
            vsharp_min_radius: parse_optional_f64(&ps.vsharp_min_radius),
            vsharp_grid: match ps.vsharp_grid {
                1 => Some(crate::cli::VsharpGridArg::Finest),
                2 => Some(crate::cli::VsharpGridArg::Isotropic),
                _ => None,
            },
            vsharp_grid_spacing: if ps.vsharp_grid == 2 { parse_optional_f64(&ps.vsharp_grid_spacing) } else { None },
        },
        pdf_params: crate::cli::PdfParamArgs {
            pdf_tol: parse_optional_f64(&ps.pdf_tol),
        },
        lbv_params: crate::cli::LbvParamArgs {
            lbv_tol: parse_optional_f64(&ps.lbv_tol),
        },
        ismv_params: crate::cli::IsmvParamArgs {
            ismv_tol: parse_optional_f64(&ps.ismv_tol),
            ismv_max_iter: parse_optional_usize(&ps.ismv_max_iter),
            ismv_radius: parse_optional_f64(&ps.ismv_radius),
        },
        sharp_params: crate::cli::SharpParamArgs {
            sharp_threshold: parse_optional_f64(&ps.sharp_threshold),
            sharp_radius: parse_optional_f64(&ps.sharp_radius),
        },
        resharp_params: crate::cli::ResharpParamArgs {
            resharp_radius: parse_optional_f64(&ps.resharp_radius),
            resharp_tik_reg: parse_optional_f64(&ps.resharp_tik_reg),
            resharp_tol: parse_optional_f64(&ps.resharp_tol),
            resharp_max_iter: parse_optional_usize(&ps.resharp_max_iter),
        },
        harperella_params: crate::cli::HarperellaParamArgs {
            harperella_radius: parse_optional_f64(&ps.harperella_radius),
            harperella_max_iter: parse_optional_usize(&ps.harperella_max_iter),
            harperella_tol: parse_optional_f64(&ps.harperella_tol),
        },
        iharperella_params: crate::cli::IharperellaParamArgs {
            iharperella_max_iter: parse_optional_usize(&ps.iharperella_max_iter),
            iharperella_tol: parse_optional_f64(&ps.iharperella_tol),
        },
        msmv_params: crate::cli::MsmvParamArgs {
            msmv_refine: ps.msmv_refine,
            msmv_radius: parse_optional_f64(&ps.msmv_radius),
            msmv_maxk: parse_optional_usize(&ps.msmv_maxk),
        },
        romeo_params: crate::cli::RomeoParamArgs {
            no_romeo_phase_coherence: !ps.romeo_phase_coherence,
            no_romeo_phase_gradient_coherence: !ps.romeo_phase_gradient_coherence,
            no_romeo_phase_linearity: !ps.romeo_phase_linearity,
            no_romeo_mag_coherence: !ps.romeo_mag_coherence,
            romeo_mag_weight: ps.romeo_mag_weight,
            no_romeo_mag_weight: false,
            romeo_mag_weight2: ps.romeo_mag_weight2,
            romeo_bestpath: ps.romeo_bestpath,
            romeo_template: if ps.unwrapping_algorithm != 0 || ps.romeo_individual { None } else { parse_optional_usize(&ps.romeo_template) },
            // ROMEO's own setting: not passed under Laplacian unwrapping.
            romeo_individual: ps.romeo_individual,
            no_romeo_individual: ps.unwrapping_algorithm == 0 && !ps.romeo_individual,
            romeo_correct_global: ps.romeo_correct_global,
            no_romeo_correct_global: !ps.romeo_correct_global,
            romeo_temporal_uncertain_unwrapping: if ps.romeo_temporal_uncertain { parse_optional_f64(&ps.romeo_temporal_uncertain_unwrapping) } else { Some(0.0) },
            romeo_max_seeds: ps.romeo_max_seeds.trim().parse::<u8>().ok(),
            romeo_merge_regions: ps.romeo_merge_regions,
            romeo_correct_regions: ps.romeo_correct_regions,
            romeo_wrap_addition: parse_optional_f64(&ps.romeo_wrap_addition),
        },
        swi_params: crate::cli::SwiParamArgs {
            swi_hp_sigma: form.swi_hp_sigma_values(),
            swi_unwrapping: Some(SWI_UNWRAP_ARGS[form.swi_unwrap]),
            swi_scaling: Some(SWI_SCALING_ARGS[form.swi_scaling]),
            swi_strength: parse_optional_f64(&form.swi_strength),
            swi_mip_window: parse_optional_usize(&form.swi_mip_window),
        },
        tiling_params: crate::cli::TilingParamArgs {
            tile_size: if ps.dl_tiling { parse_optional_usize(&ps.dl_tile_size) } else { None },
            tile_halo: if ps.dl_tiling { parse_optional_usize(&ps.dl_tile_halo) } else { None },
        },
        homogeneity_sigma_mm: if ps.inhomogeneity_correction { parse_optional_f64(&ps.homogeneity_sigma_mm) } else { None },
        homogeneity_nbox: if ps.inhomogeneity_correction { parse_optional_usize(&ps.homogeneity_nbox) } else { None },
        linear_fit_reliability_threshold: if ps.linear_fit_reliability { parse_optional_f64(&ps.linear_fit_reliability_threshold) } else { Some(0.0) },
        linear_fit_estimate_offset: Some(ps.linear_fit_estimate_offset),
        robust_fit_loss: (ps.b0_estimation == 2 && ps.robust_fit_loss == 1).then_some(crate::cli::RobustLossArg::Huber),
        robust_fit_tuning: if ps.b0_estimation == 2 { parse_optional_f64(&ps.robust_fit_tuning) } else { None },
        robust_fit_iterations: if ps.b0_estimation == 2 { parse_optional_usize(&ps.robust_fit_iterations) } else { None },
        robust_fit_downweight_level: if ps.b0_estimation == 2 { parse_optional_f64(&ps.robust_fit_downweight_level) } else { None },
        robust_fit_flag_ratio: if ps.b0_estimation == 2 { parse_optional_f64(&ps.robust_fit_flag_ratio) } else { None },
        no_qsm: !ps.do_qsm,
        do_swi: form.do_swi,
        do_smwi: form.do_smwi,
        do_segmentation: form.do_segmentation,
        do_analysis: form.do_analysis,
        segmentation_params: crate::cli::SegmentationParamArgs {
            synthseg_version: Some(if form.synthseg_version == 1 { "v2".into() } else { "v1".into() }),
            synthseg_crop: parse_optional_usize(&form.synthseg_crop),
            synthseg_patch: parse_optional_usize(&form.synthseg_patch),
            no_synthseg_flip_averaging: !form.synthseg_flip_averaging,
            no_synthseg_topology_cleanup: !form.synthseg_topology_cleanup,
            synthseg_sigma: if form.synthseg_smoothing { parse_optional_f64(&form.synthseg_sigma) } else { Some(0.0) },
            use_custom_dseg: Some(ps.custom_dseg_tool.trim()).filter(|t| !t.is_empty()).map(str::to_string),
        },
        smwi_params: crate::cli::SmwiParamArgs {
            smwi_threshold: parse_optional_f64(&form.smwi_threshold),
            smwi_power: parse_optional_f64(&form.smwi_power),
            smwi_mip_window: parse_optional_usize(&form.smwi_mip_window),
        },
        do_t2starmap: form.do_t2starmap,
        do_r2starmap: form.do_r2starmap,
        do_r2map: form.do_r2map,
        do_r2primemap: form.do_r2primemap,
        do_chi_separation: ps.do_chi_separation,
        chi_separation_algorithm: Some(sep_options[ps.separation_algorithm]),
        use_custom_qsm: tool(inputs.qsm, &ps.custom_qsm_tool),
        use_custom_r2: tool(inputs.r2, &ps.custom_r2_tool),
        use_custom_r2prime: tool(inputs.r2prime, &ps.custom_r2prime_tool),
        r2prime_strategy: inputs.r2prime_source.then_some(match ps.r2prime_strategy {
            1 => crate::cli::R2PrimeStrategyArg::Mese,
            2 => crate::cli::R2PrimeStrategyArg::R2primenet,
            _ => crate::cli::R2PrimeStrategyArg::Auto,
        }),
        r2primenet_ignore_field_mismatch: inputs.r2prime_source && ps.r2primenet_ignore_field_mismatch,
        r2primenet_patch: if inputs.r2prime_source { parse_usize_triple(&ps.r2primenet_patch).map(|p| p.to_vec()) } else { None },
        export_dicom: form.export_dicom,
        inhomogeneity_correction: ps.inhomogeneity_correction,
        no_inhomogeneity_correction: !ps.inhomogeneity_correction,
        axial_resampling: ps.axial_resampling,
        no_axial_resampling: false,
        obliquity_threshold: if ps.axial_resampling { parse_optional_f64(&ps.obliquity_threshold) } else { None },
        crop_to_mask: ps.crop_to_mask,
        ignore_slice_gap: ps.ignore_slice_gap,
        fft_padding: ps.fft_padding,
        crop_margin_mm: if ps.crop_to_mask { parse_optional_f64(&ps.crop_margin_mm) } else { None },
        output_space: Some(if ps.output_space == 1 { crate::cli::OutputSpaceArg::Working } else { crate::cli::OutputSpaceArg::Acquired }),
        mask_preset: None,
        use_custom_masks: if ps.custom_mask_tool.trim().is_empty() { None } else { Some(ps.custom_mask_tool.trim().to_string()) },
        mask_sections_cli: {
            let secs: Vec<String> = ps.mask_sections.iter().map(|section| {
                let mut parts = vec![format!("{}", section.input)];
                for op in &section.all_ops() {
                    parts.push(format!("{}", op));
                }
                parts.join(",")
            }).collect();
            if secs.is_empty() { None } else { Some(secs) }
        },
        // Only the non-default mode is worth spelling out on the command line.
        mask_combine: match ps.mask_combine_recipe().0 {
            crate::pipeline::config::MaskCombine::Or => None,
            crate::pipeline::config::MaskCombine::And => Some(crate::cli::MaskCombineArg::And),
        },
        mask_refinements_cli: match ps.mask_combine_recipe().1 {
            ops if ops.is_empty() => None,
            ops => Some(ops.iter().map(|op| format!("{op}")).collect()),
        },
        two_pass: ps.two_pass,
        // Spelled out whenever two-pass is on: this is the run's own argument list, not the command
        // shown to the user, so there is nothing to be gained by leaning on the default.
        two_pass_sections_cli: ps.two_pass.then(|| {
            ps.two_pass_sections.iter().map(|section| {
                std::iter::once(format!("{}", section.input))
                    .chain(section.all_ops().iter().map(|op| format!("{op}")))
                    .collect::<Vec<_>>()
                    .join(",")
            }).collect()
        }),
    }
}

/// A bring-your-own derivatives tool, when the run can use one and the field names it.
fn tool(relevant: bool, name: &str) -> Option<String> {
    Some(name.trim()).filter(|t| relevant && !t.is_empty()).map(str::to_string)
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

/// `--dicom-outputs` takes a comma-separated list; spaces are accepted too.
fn parse_dicom_outputs(s: &str) -> Option<Vec<String>> {
    let v: Vec<String> = s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty()).map(str::to_string).collect();
    (!v.is_empty()).then_some(v)
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

fn parse_optional_f64(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse().ok()
    }
}

/// Three whitespace-separated counts ("192 192 128"), or `None` if the field is not exactly that.
fn parse_usize_triple(s: &str) -> Option<[usize; 3]> {
    let v: Vec<usize> = s.split_whitespace().map(|t| t.parse().ok()).collect::<Option<_>>()?;
    <[usize; 3]>::try_from(v).ok()
}

/// "64" (every axis) or "64 64 32".
fn parse_pad3(s: &str) -> Option<[usize; 3]> {
    let v: Vec<usize> = s.split_whitespace().map(|t| t.parse().ok()).collect::<Option<_>>()?;
    qsmxt_engine::pad3_from(&v)
}

/// Three numbers, separated by spaces or commas; anything else is None.
fn parse_optional_f64_vec3(s: &str) -> Option<Vec<f64>> {
    let v: Vec<f64> = s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty()).map(|t| t.parse().ok()).collect::<Option<_>>()?;
    (v.len() == 3).then_some(v)
}

fn parse_optional_usize(s: &str) -> Option<usize> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse().ok()
    }
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
        bids_dir: expand_tilde(&form.bids_dir),
        output_dir: if form.output_dir.is_empty() { None } else { Some(expand_tilde(&form.output_dir)) },
        account: form.slurm_account.trim().to_string(),
        partition: if form.slurm_partition.trim().is_empty() { None } else { Some(form.slurm_partition.trim().to_string()) },
        config: parse_optional_path(&form.config_file),
        time: if form.slurm_time.trim().is_empty() { defaults.slurm_time.clone() } else { form.slurm_time.trim().to_string() },
        mem: parse_optional_usize(&form.slurm_mem),
        cpus_per_task: form.slurm_cpus.trim().parse().unwrap_or(4),
        submit: form.slurm_submit,
        include,
        exclude,
        num_echoes: parse_optional_usize(&app.filter_state.num_echoes),
        force: form.force,
        clean_intermediates: form.clean_intermediates,
        debug: form.debug,
        source_dicom: if form.export_dicom { parse_optional_path(&form.source_dicom) } else { None },
        dicom_outputs: if form.export_dicom { parse_dicom_outputs(&form.dicom_outputs) } else { None },
        pipeline: pipeline_args_from_app(app),
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
    config_from_app(app).map(|c| crate::pipeline::methods::generate_methods(&c)).map_err(|e| e.to_string())
}

pub fn config_from_app(app: &App) -> crate::Result<PipelineConfig> {
    let mut config = PipelineConfig::default();
    crate::pipeline::config::apply_run_overrides(&mut config, &pipeline_args_from_app(app))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::app::App;

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

    /// Each entry of the SWI unwrapping select runs the unwrapping it shows.
    #[test]
    fn swi_unwrap_select_runs_what_it_shows() {
        use qsmxt_engine::SwiUnwrap;
        assert_eq!(SWI_UNWRAP_ARGS.len(), SwiUnwrap::ALL.len());
        for (i, &expected) in SwiUnwrap::ALL.iter().enumerate() {
            let mut app = App::new();
            app.form.do_swi = true;
            app.form.swi_unwrap = i;
            assert_eq!(config_from_app(&app).unwrap().swi.unwrap, expected, "row {i}");
        }
        // The first entry is the default.
        assert_eq!(SwiUnwrap::ALL[0], qsmxt_engine::SwiConfig::default().unwrap);
    }

    /// Each entry of the SWI scaling select runs the scaling it shows, and every scaling reaches
    /// qsm-core's `PhaseScaling` unchanged (#266: `negative-tanh` used to run `tanh`).
    #[test]
    fn swi_scaling_select_runs_what_it_shows() {
        use qsmxt_engine::SwiScaling;
        assert_eq!(SWI_SCALING_ARGS.len(), SwiScaling::ALL.len());
        for (i, &expected) in SwiScaling::ALL.iter().enumerate() {
            let mut app = App::new();
            app.form.do_swi = true;
            app.form.swi_scaling = i;
            assert_eq!(config_from_app(&app).unwrap().swi.scaling, expected, "row {i}");
        }
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

    /// R2PRIMEnet's field-strength override and patch reach both the config and the CLI args —
    /// when R2' is computed; otherwise they have nothing to act on and are not passed.
    #[test]
    fn r2primenet_settings_reach_config_and_args() {
        let mut app = default_app();
        app.pipeline_state.toggle("r2primenet_ignore_field_mismatch");
        app.pipeline_state.r2primenet_patch = "128 128 64".into();
        let a = pipeline_args_from_app(&app);
        assert!(!a.r2primenet_ignore_field_mismatch && a.r2primenet_patch.is_none(), "no R2', nothing passed");

        let mut app = default_app();
        app.form.do_r2primemap = true;
        let d = qsmxt_engine::R2primenetConfig::default();
        assert_eq!(config_from_app(&app).unwrap().separation.r2primenet, d);
        app.pipeline_state.toggle("r2primenet_ignore_field_mismatch");
        app.pipeline_state.r2primenet_patch = "128 128 64".into();
        let c = config_from_app(&app).unwrap();
        assert!(c.separation.r2primenet.ignore_field_mismatch);
        assert_eq!(c.separation.r2primenet.patch, [128, 128, 64]);
        let a = pipeline_args_from_app(&app);
        assert!(a.r2primenet_ignore_field_mismatch);
        assert_eq!(a.r2primenet_patch, Some(vec![128, 128, 64]));
    }

    /// V-SHARP's working grid: native by default (off the command), spacing row only for isotropic.
    #[test]
    fn vsharp_working_grid() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_state.all_rows().iter().filter_map(|r| match r {
            crate::tui::app::PipelineRow::Param { field, .. } | crate::tui::app::PipelineRow::AlgoSelect { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(fields(&app).contains(&"vsharp_grid".to_string()));
        assert!(!fields(&app).contains(&"vsharp_grid_spacing".to_string()));
        assert_eq!(config_from_app(&app).unwrap().bg_removal.vsharp.grid, qsmxt_engine::VsharpGrid::Native);
        assert_eq!(pipeline_args_from_app(&app).vsharp_params.vsharp_grid, None);
        app.pipeline_state.set_select("vsharp_grid", 1);
        assert_eq!(config_from_app(&app).unwrap().bg_removal.vsharp.grid, qsmxt_engine::VsharpGrid::Finest);
        assert!(!fields(&app).contains(&"vsharp_grid_spacing".to_string()));
        app.pipeline_state.set_select("vsharp_grid", 2);
        assert!(fields(&app).contains(&"vsharp_grid_spacing".to_string()));
        app.pipeline_state.vsharp_grid_spacing = "0.8".into();
        let c = config_from_app(&app).unwrap();
        assert_eq!((c.bg_removal.vsharp.grid, c.bg_removal.vsharp.grid_spacing), (qsmxt_engine::VsharpGrid::Isotropic, 0.8));
        let a = pipeline_args_from_app(&app);
        assert_eq!(a.vsharp_params.vsharp_grid, Some(crate::cli::VsharpGridArg::Isotropic));
        assert_eq!(a.vsharp_params.vsharp_grid_spacing, Some(0.8));
    }

    /// V-SHARP's deconvolution is off by default, its threshold row only shows when it is on,
    /// and the toggle reaches both the config and the CLI args.
    #[test]
    fn vsharp_deconvolve_toggle() {
        let mut app = default_app();
        let shows_threshold = |app: &App| app.pipeline_state.all_rows().iter().any(|r|
            matches!(r, crate::tui::app::PipelineRow::Param { field: "vsharp_threshold", .. }));
        assert!(!config_from_app(&app).unwrap().bg_removal.vsharp.deconvolve);
        assert!(!shows_threshold(&app));
        app.pipeline_state.toggle("vsharp_deconvolve");
        assert!(shows_threshold(&app));
        assert!(config_from_app(&app).unwrap().bg_removal.vsharp.deconvolve);
        assert_eq!(pipeline_args_from_app(&app).vsharp_params.vsharp_deconvolve, Some(true));
    }

    /// The Laplacian solver and its pad are in the TUI (they used to be CLI-only): the rows
    /// follow the selection, and the values reach both the config and the CLI args.
    #[test]
    fn laplacian_solver_and_pad() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_state.all_rows().iter().filter_map(|r| match r {
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(!fields(&app).contains(&"laplacian_solver".to_string()), "ROMEO has no solver");
        app.pipeline_state.unwrapping_algorithm = 1; // laplacian
        assert!(fields(&app).contains(&"laplacian_solver".to_string()));
        assert!(!fields(&app).contains(&"laplacian_fft_pad".to_string()), "dct has no pad");
        assert_eq!(pipeline_args_from_app(&app).laplacian_solver, None, "default stays off the command");
        app.pipeline_state.laplacian_solver = 1; // fft
        assert!(fields(&app).contains(&"laplacian_fft_pad".to_string()));
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.laplacian_solver, qsmxt_engine::LaplacianSolver::Fft);
        assert_eq!(c.field_mapping.laplacian_fft_pad, [64; 3]);
        assert_eq!(pipeline_args_from_app(&app).laplacian_fft_pad, None, "default pad not emitted");
        app.pipeline_state.laplacian_fft_pad = "64 64 32".into();
        assert_eq!(config_from_app(&app).unwrap().field_mapping.laplacian_fft_pad, [64, 64, 32]);
        let a = pipeline_args_from_app(&app);
        assert!(matches!(a.laplacian_solver, Some(crate::cli::LaplacianSolverArg::Fft)));
        assert_eq!(a.laplacian_fft_pad, Some(vec![64, 64, 32]));
        app.pipeline_state.laplacian_fft_pad = "12".into();
        assert_eq!(config_from_app(&app).unwrap().field_mapping.laplacian_fft_pad, [12; 3]);
        assert_eq!(pipeline_args_from_app(&app).laplacian_fft_pad, Some(vec![12]));
    }

    /// The robust fit, its parameters and the shared fit settings are in the TUI and reach both
    /// the config and the CLI args.
    #[test]
    fn robust_fit_settings_reach_config_and_args() {
        let mut app = default_app();
        let fields = |app: &App| app.pipeline_state.all_rows().iter().filter_map(|r| match r {
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(!fields(&app).contains(&"robust_fit_loss".to_string()));
        app.pipeline_state.b0_estimation = 2;
        let f = fields(&app);
        for name in ["robust_fit_loss", "robust_fit_tuning", "robust_fit_iterations",
                     "robust_fit_downweight_level", "robust_fit_flag_ratio", "linear_fit_reliability_threshold"] {
            assert!(f.contains(&name.to_string()), "{name} missing: {f:?}");
        }
        assert_eq!(config_from_app(&app).unwrap().field_mapping.robust_fit, qsmxt_engine::RobustFitConfig::default());
        let ps = &mut app.pipeline_state;
        ps.robust_fit_loss = 1;
        ps.robust_fit_tuning = "2".into();
        ps.robust_fit_iterations = "0".into();
        ps.robust_fit_downweight_level = "0.5".into();
        ps.robust_fit_flag_ratio = "4".into();
        ps.linear_fit_reliability_threshold = "80".into();
        ps.toggle("linear_fit_estimate_offset");
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.b0_estimation, qsmxt_engine::B0Estimation::RobustFit);
        assert_eq!(c.field_mapping.robust_fit, qsmxt_engine::RobustFitConfig {
            loss: qsmxt_engine::RobustLoss::Huber, tuning: Some(2.0), iterations: 0,
            downweight_level: 0.5, flag_ratio: 4.0,
        });
        assert!(!c.field_mapping.linear_fit.estimate_offset);
        assert_eq!(c.field_mapping.linear_fit.reliability_threshold_percentile, 80.0);
        let a = pipeline_args_from_app(&app);
        assert_eq!(a.b0_estimation, Some(B0EstimationArg::RobustFit));
        assert_eq!(a.robust_fit_loss, Some(crate::cli::RobustLossArg::Huber));
        assert_eq!(a.robust_fit_tuning, Some(2.0));
        assert_eq!(a.robust_fit_iterations, Some(0));
        assert_eq!(a.linear_fit_reliability_threshold, Some(80.0));
        assert_eq!(a.linear_fit_estimate_offset, Some(false));
    }

    #[test]
    fn ignore_slice_gap_reaches_config_and_args() {
        let mut app = default_app();
        assert!(!config_from_app(&app).unwrap().pipeline.ignore_slice_gap);
        app.pipeline_state.toggle("ignore_slice_gap");
        assert!(config_from_app(&app).unwrap().pipeline.ignore_slice_gap);
        assert!(pipeline_args_from_app(&app).ignore_slice_gap);
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
        let fields = |app: &App| app.pipeline_state.all_rows().iter().filter_map(|r| match r {
            crate::tui::app::PipelineRow::AlgoSelect { field, .. }
            | crate::tui::app::PipelineRow::Param { field, .. }
            | crate::tui::app::PipelineRow::Toggle { field, .. } => Some(field.to_string()),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(fields(&app).contains(&"motion_correction".to_string()));
        assert!(!fields(&app).contains(&"reg_levels".to_string()),
                "registration serves motion correction and orientation groups, and neither is on");
        assert_eq!(config_from_app(&app).unwrap().motion_correction, qsmxt_engine::MotionCorrectionConfig::default());
        let ps = &mut app.pipeline_state;
        ps.toggle("motion_correction");
        ps.motion_reference = 2;
        ps.motion_reference_echo = "2".into();
        ps.toggle("motion_chain");
        ps.motion_min_ncc = "0.9".into();
        ps.reg_levels = "4".into();
        ps.reg_min_overlap = "0.5".into();
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
        let a = pipeline_args_from_app(&app).motion_params;
        assert!(a.motion_correction && a.no_motion_chain);
        assert_eq!(a.motion_reference.as_deref(), Some("2"));
        assert_eq!((a.motion_min_ncc, a.registration_levels, a.registration_min_overlap), (Some(0.9), Some(4), Some(0.5)));
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
        let shows_bfr = |app: &App| app.pipeline_state.all_rows().iter().any(|r|
            matches!(r, PipelineRow::AlgoSelect { field: "bf_algorithm", .. }));
        assert!(shows_bfr(&app));
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
        app.pipeline_state.toggle("orientation_registration");
        app.pipeline_state.toggle("multi_orientation_force");
        let a = pipeline_args_from_app(&app);
        assert!(!a.no_orientation_registration && !a.multi_orientation_force);

        let mut app = default_app();
        app.filter_state.orientation_group = "acq".into();
        app.sync_pipeline_mode();
        let c = config_from_app(&app).unwrap();
        assert!(c.multi_orientation.register && !c.multi_orientation.force);
        app.pipeline_state.toggle("orientation_registration");
        app.pipeline_state.toggle("multi_orientation_force");
        let c = config_from_app(&app).unwrap();
        assert!(!c.multi_orientation.register && c.multi_orientation.force);
        let a = pipeline_args_from_app(&app);
        assert!(a.no_orientation_registration && a.multi_orientation_force);
        let shows_reg = |app: &App| app.pipeline_state.all_rows().iter().any(|r|
            matches!(r, crate::tui::app::PipelineRow::Param { field: "reg_levels", .. }));
        assert!(!shows_reg(&app), "neither registration can run");
        app.pipeline_state.toggle("motion_correction");
        assert!(shows_reg(&app));
        // Co-registering a group's orientations runs it too.
        app.pipeline_state.toggle("motion_correction");
        app.pipeline_state.toggle("orientation_registration");
        assert!(shows_reg(&app));
        app.pipeline_state.ctx.orientation_group = false;
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
            let set = pipeline_args_from_app(&app).qsmart_params.qsmart_inversion.is_some();
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
                  "--linear-fit-reliability-threshold", "--tile-size", "--synthseg-sigma"] {
            assert!(!base.contains(f), "{f} in a default command: {base}");
        }
        app.pipeline_state.axial_resampling = true;
        assert!(build_command_string(&app).contains("--axial-resampling"));
        app.pipeline_state.obliquity_threshold = "20".into();
        assert!(build_command_string(&app).contains("--obliquity-threshold 20"));
        app.pipeline_state.axial_resampling = false;
        assert!(!build_command_string(&app).contains("--obliquity-threshold"), "no threshold without resampling");

        app.pipeline_state.romeo_temporal_uncertain = false;
        app.pipeline_state.linear_fit_reliability = false;
        app.pipeline_state.b0_estimation = 1; // linear fit: the one with a reliability mask
        app.form.synthseg_smoothing = false;
        app.form.do_segmentation = true;
        let cmd = build_command_string(&app);
        for f in ["--romeo-temporal-uncertain-unwrapping 0", "--linear-fit-reliability-threshold 0", "--synthseg-sigma 0"] {
            assert!(cmd.contains(f), "{f} missing: {cmd}");
        }
        app.pipeline_state.qsm_algorithm = crate::tui::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "xqsm").unwrap();
        assert!(!build_command_string(&app).contains("--tile-size"));
        app.pipeline_state.dl_tiling = true;
        assert!(build_command_string(&app).contains("--tile-size 128"));

        app.pipeline_state.qsm_algorithm = crate::tui::app::QSM_ALGO_OPTIONS.iter().position(|a| *a == "medi").unwrap();
        assert_eq!(app.pipeline_state.get_select("medi_data_weighting"), 1, "magnitude SNR by default");
        app.pipeline_state.set_select("medi_data_weighting", 0);
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
        app.pipeline_state.do_chi_separation = true;
        app.pipeline_state.separation_algorithm = 1; // decompose
        app.pipeline_state.sep_decompose_max_lm_iter = "77".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--do-chisep"));
        assert!(cmd.contains("--chisep decompose"));
        assert!(cmd.contains("--decompose-max-lm-iter 77"));
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
        app.pipeline_state.ilsqr_tol = "0.01".to_string();
        app.pipeline_state.ilsqr_pad_mm = "32".to_string();
        app.pipeline_state.axial_resampling = true;
        app.pipeline_state.obliquity_threshold = "5".to_string();
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
        app.pipeline_state.phase_offset_removal = false;
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--phase-offset-removal false"));
    }

    #[test]
    fn test_command_string_execution_flags() {
        let mut app = default_app();
        app.form.do_swi = true;
        app.form.do_t2starmap = true;
        app.form.do_r2starmap = true;
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
        app.pipeline_state.rts_delta = "0.2".to_string();
        app.pipeline_state.rts_mu = "1e5".to_string();
        app.pipeline_state.rts_tol = "1e-4".to_string();
        app.pipeline_state.tgv_iterations = "500".to_string();
        app.pipeline_state.tgv_erosions = "2".to_string();
        app.pipeline_state.tv_lambda = "0.001".to_string();
        app.pipeline_state.tkd_threshold = "0.15".to_string();
        app.form.n_procs = "8".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.pipeline.rts_params.rts_delta, Some(0.2));
        assert_eq!(args.pipeline.tgv_params.tgv_iterations, Some(500));
        assert_eq!(args.n_procs, Some(8));
    }

    #[test]
    fn test_build_run_args_flags() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.form.do_swi = true;
        app.form.do_t2starmap = true;
        app.form.do_r2starmap = true;
        app.pipeline_state.inhomogeneity_correction = true;
        app.form.dry_run = true;
        app.form.debug = true;
        let args = build_run_args(&app).unwrap();
        assert!(args.pipeline.do_swi);
        assert!(args.pipeline.do_t2starmap);
        assert!(args.pipeline.do_r2starmap);
        assert!(args.pipeline.inhomogeneity_correction);
        assert!(args.dry);
        assert!(args.debug);
        assert_eq!(args.pipeline.phase_offset_removal, Some(true)); // default mcpc3ds
    }

    #[test]
    fn test_build_run_args_phase_offset_removal_disabled() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.pipeline_state.phase_offset_removal = false;
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.pipeline.phase_offset_removal, Some(false));
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
        app.pipeline_state.axial_resampling = true;
        app.pipeline_state.obliquity_threshold = "5.0".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.pipeline.obliquity_threshold, Some(5.0));
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
    fn test_parse_optional_f64_empty() {
        assert_eq!(parse_optional_f64(""), None);
        assert_eq!(parse_optional_f64("  "), None);
    }

    #[test]
    fn test_parse_optional_f64_valid() {
        assert_eq!(parse_optional_f64("2.72"), Some(2.72));
        assert_eq!(parse_optional_f64("  1e-4  "), Some(1e-4));
    }

    #[test]
    fn test_parse_optional_f64_invalid() {
        assert_eq!(parse_optional_f64("abc"), None);
    }

    #[test]
    fn test_parse_optional_usize_empty() {
        assert_eq!(parse_optional_usize(""), None);
    }

    #[test]
    fn test_parse_optional_usize_valid() {
        assert_eq!(parse_optional_usize("42"), Some(42));
    }

    #[test]
    fn test_parse_optional_usize_invalid() {
        assert_eq!(parse_optional_usize("abc"), None);
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
        app.pipeline_state.rts_delta = "0.2".to_string();

        let cmd = build_command_string(&app);
        let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        let cli = crate::cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("TUI SLURM command did not parse: {}\ncmd: {}", e, cmd));
        match cli.command {
            crate::cli::Command::Slurm(args) => {
                assert_eq!(args.pipeline.qsm_algorithm, Some(crate::cli::QsmAlgorithmArg::Tgv));
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
        app.form.dicom_outputs = "Chimap, swi".to_string();

        let args = build_slurm_args(&app).unwrap();
        assert!(args.debug);
        assert_eq!(args.source_dicom, Some(PathBuf::from("/dicom")));
        assert_eq!(args.dicom_outputs, Some(vec!["Chimap".to_string(), "swi".to_string()]));
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
        app.pipeline_state.unwrapping_algorithm = 1; // laplacian
        app.pipeline_state.bf_algorithm = 1; // pdf
        app.pipeline_state.axial_resampling = true;
        app.pipeline_state.obliquity_threshold = "5".to_string();
        app.form.do_swi = true;

        let slurm = build_slurm_args(&app).unwrap();
        assert_eq!(slurm.pipeline.qsm_algorithm, Some(crate::cli::QsmAlgorithmArg::Tkd));
        assert_eq!(slurm.pipeline.unwrapping_algorithm, Some(crate::cli::UnwrapAlgorithmArg::Laplacian));
        assert_eq!(slurm.pipeline.bf_algorithm, Some(crate::cli::BfAlgorithmArg::Pdf));
        assert_eq!(slurm.pipeline.obliquity_threshold, Some(5.0));
        assert!(slurm.pipeline.do_swi);
    }

    /// The B0 echo-combination rows reach the run (they used to be dropped, so the TUI's weight
    /// type was ignored), including the assumed-decay weighting and its assumed T2*.
    #[test]
    fn test_run_args_carry_b0_combination() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        let run = build_run_args(&app).unwrap();
        assert_eq!(run.pipeline.b0_weight_type, None);
        assert_eq!(run.pipeline.b0_estimation, None);
        app.pipeline_state.unwrapping_algorithm = 1; // laplacian
        app.pipeline_state.b0_weight_type = 5; // assumed-decay
        app.pipeline_state.b0_weight_assumed_t2star_ms = "35".to_string();
        let mut cfg = PipelineConfig::default();
        crate::pipeline::config::apply_run_overrides(&mut cfg, &build_run_args(&app).unwrap().pipeline).unwrap();
        assert_eq!(cfg.field_mapping.b0_weight_type, crate::pipeline::config::B0WeightType::AssumedDecay);
        assert_eq!(cfg.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        let c = config_from_app(&app).unwrap();
        assert_eq!(c.field_mapping.b0_weight_type, crate::pipeline::config::B0WeightType::AssumedDecay);
        assert_eq!(c.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        app.pipeline_state.b0_estimation = 1;
        assert_eq!(build_run_args(&app).unwrap().pipeline.b0_estimation, Some(B0EstimationArg::LinearFit));
    }

    /// The pipeline the SLURM jobs get must be the one a local run would use.
    #[test]
    fn test_slurm_and_local_resolve_to_the_same_config() {
        let mut app = default_app();
        app.form.bids_dir = "/bids".to_string();
        app.form.slurm_account = "acct".to_string();
        app.pipeline_state.qsm_algorithm = 4; // tgv
        app.pipeline_state.unwrapping_algorithm = 1; // laplacian
        app.pipeline_state.bf_algorithm = 1; // pdf
        app.pipeline_state.bipolar_correction = true;

        let mut local = PipelineConfig::default();
        crate::pipeline::config::apply_run_overrides(&mut local, &build_run_args(&app).unwrap().pipeline).unwrap();
        let mut slurm = PipelineConfig::default();
        crate::pipeline::config::apply_run_overrides(&mut slurm, &build_slurm_args(&app).unwrap().pipeline).unwrap();

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
        crate::pipeline::config::apply_run_overrides(&mut back, &run.pipeline).unwrap();
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

    /// A different value of the same shape: each number in it moved (lists stay lists).
    fn bumped(v: &str) -> String {
        let bump = |t: &str| -> Option<String> {
            if let Ok(n) = t.parse::<i64>() { return Some((n + 1).to_string()); }
            let x = t.parse::<f64>().ok()?;
            Some(format!("{}", if x == 0.0 { 0.1 } else { x * 0.9 }))
        };
        let tokens: Vec<&str> = v.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).collect();
        match tokens.iter().map(|t| bump(t)).collect::<Option<Vec<_>>>() {
            Some(b) if !b.is_empty() => b.join(if v.contains(',') { "," } else { " " }),
            _ => "3".to_string(),
        }
    }

    /// Every state worth looking at: start from the defaults in each tab mode, and keep any
    /// select/toggle change that reveals a row not seen yet, until nothing new turns up.
    fn explore() -> Vec<App> {
        // A row counts as seen per inversion algorithm: the same row (an inner inversion's
        // parameters under QSMART, say) can mean a different flag under another one.
        let mut seen: BTreeSet<(&'static str, char, usize)> = BTreeSet::new();
        let mut states = Vec::new();
        let mut frontier = Vec::new();
        // The defaults, plus the starts whose rows only a non-row control reveals: the
        // multi-orientation settings (a grouping pattern on the Input tab, COSMOS or STI) and the
        // inputs of the relaxometry maps and statistics.
        type Start = fn(&mut App);
        let starts: &[Start] = &[
            |_| {},
            |a| a.filter_state.orientation_group = "acq".into(),
            |a| { a.filter_state.orientation_group = "acq".into(); a.filter_state.orientation_algorithm = 1; },
            |a| { a.form.do_r2primemap = true; a.form.do_r2map = true; a.form.do_analysis = true; },
        ];
        for start in starts {
            let mut app = App::new();
            // What a run needs before it can be built in either mode.
            app.form.bids_dir = "/bids".into();
            app.form.slurm_account = "acct".into();
            start(&mut app);
            frontier.push(app);
        }
        while let Some(app) = frontier.pop() {
            let rows = rows(&app);
            let alg = app.pipeline_state.qsm_algorithm;
            // A checkbox counts as seen per value too: a state where a forced output is free
            // (and so can be checked) is new even when its rows are not.
            let fresh = rows.iter().filter_map(row_key)
                .map(|(field, kind)| (field, if kind == 't' && app.field_toggle(field) { 'T' } else { kind }))
                .fold(false, |f, (field, kind)| seen.insert((field, kind, alg)) | f);
            if !fresh { continue; }
            for r in &rows {
                match r {
                    PipelineRow::AlgoSelect { field, options, .. } => {
                        for i in (0..options.len()).filter(|&i| i != app.field_select(field)) {
                            let mut a = clone_app(&app);
                            a.set_field_select(field, i);
                            frontier.push(a);
                        }
                    }
                    PipelineRow::Toggle { field, .. } => {
                        let mut a = clone_app(&app);
                        a.toggle_field(field);
                        if a.error_message.is_none() { frontier.push(a); }
                    }
                    _ => {}
                }
            }
            states.push(app);
        }
        states
    }

    fn clone_app(app: &App) -> App {
        let mut a = App::new();
        a.pipeline_state = app.pipeline_state.clone();
        a.form = app.form.clone();
        a.filter_state = app.filter_state.clone();
        a.active_tab = app.active_tab;
        a
    }

    /// What the run would do: the pipeline's resolved config (compared as data, so a value that
    /// changes nothing reads as no effect) and the run-level arguments of the active mode.
    fn run_of(app: &App) -> (serde_json::Value, String) {
        let config = serde_json::to_value(config_from_app(app).unwrap()).unwrap();
        let run_level = if app.form.execution_mode == 1 {
            let mut a = build_slurm_args(app).unwrap();
            a.pipeline = Default::default();
            format!("{a:?}")
        } else {
            let mut a = build_run_args(app).unwrap();
            a.pipeline = Default::default();
            format!("{a:?}")
        };
        (config, run_level)
    }

    #[test]
    fn every_run_flag_is_reachable_from_the_tui() {
        let mut covered: BTreeSet<String> = BTreeSet::new();
        // Problem -> the first change that showed it.
        let mut problems: BTreeMap<String, String> = BTreeMap::new();
        // A row is checked once per combination of the selections that change what it feeds.
        let mut checked: std::collections::HashSet<(&'static str, char, [usize; 4], [bool; 2])> = Default::default();
        let mut cli = <crate::cli::Cli as clap::CommandFactory>::command();
        cli.build();
        let mut round_trips: std::collections::HashMap<String, String> = Default::default();
        let mut check = |what: String, base: &App, app: &App, problems: &mut BTreeMap<String, String>| {
            let (before, after) = (config_from_app(base).unwrap(), config_from_app(app).unwrap());
            let (shown_before, shown) = (build_command_string(base), build_command_string(app));
            if run_of(base) == run_of(app) {
                problems.entry(format!("no effect on the run: {}", what.split('=').next().unwrap())).or_insert(what.clone());
            } else if shown_before == shown {
                let d = diff(&before, &after);
                problems.entry(format!("not in the command: {d:?}")).or_insert(what.clone());
            }
            let cmd = qsmxt_engine::generate_command(&after);
            // Parsing is most of a check's cost, and many changes produce a command already seen.
            let back = match round_trips.get(&cmd) {
                Some(b) => b.clone(),
                None => {
                    let b = qsmxt_engine::generate_command(&round_trip(&mut cli, &after));
                    round_trips.insert(cmd.clone(), b.clone());
                    b
                }
            };
            if back != cmd {
                let (a, b) = (flags_of(&cmd), flags_of(&back));
                problems.entry(format!("command does not reproduce itself: {:?}", a.symmetric_difference(&b).collect::<Vec<_>>()))
                    .or_insert(what.clone());
            }
            // Both ends are reachable states: a toggle explored from its checked side turns the
            // flag off, and the flag is still one the TUI emits.
            covered.extend(flags_of(&shown_before));
            covered.extend(flags_of(&shown));
        };
        for app in explore() {
            let rows = rows(&app);
            let ps = &app.pipeline_state;
            let qsmart = crate::tui::app::QSM_ALGO_OPTIONS[ps.qsm_algorithm] == "qsmart";
            let context = (
                [ps.qsm_algorithm, if qsmart { ps.qsmart_inversion } else { 0 },
                 if ps.do_chi_separation { ps.separation_algorithm } else { 0 }, app.form.execution_mode],
                [ps.motion_correction, ps.do_chi_separation],
            );
            for r in &rows {
                let Some((field, kind)) = row_key(r) else { continue };
                // A checkbox another option forces on refuses to change (and says why): it is
                // checked where it is free instead.
                if let PipelineRow::Toggle { field, .. } = r {
                    let mut a = clone_app(&app);
                    a.toggle_field(field);
                    if a.error_message.is_some() { continue; }
                }
                // The run form's settings (outputs, execution) do not feed the reconstruction:
                // they are checked per separation method and mode, not per inversion.
                let ctx = if app.form.text(field).is_some() || app.form.flag(field).is_some() || app.form.choice(field).is_some() {
                    ([0, 0, context.0[2], context.0[3]], [false, context.1[1]])
                } else {
                    context
                };
                if !checked.insert((field, kind, ctx.0, ctx.1)) { continue; }
                match r {
                    PipelineRow::AlgoSelect { field, options, .. } => {
                        let current = app.field_select(field);
                        // "custom" names a hand-edited mask; choosing it changes nothing by itself.
                        let custom = |i: usize| *field == "mask_preset" && i == crate::tui::app::MASK_PRESET_CUSTOM;
                        for i in (0..options.len()).filter(|&i| i != current && !custom(i)) {
                            let mut a = clone_app(&app);
                            a.set_field_select(field, i);
                            check(format!("{field}={i}"), &app, &a, &mut problems);
                        }
                    }
                    PipelineRow::Toggle { field, .. } => {
                        let mut a = clone_app(&app);
                        a.toggle_field(field);
                        check(format!("toggle {field}"), &app, &a, &mut problems);
                    }
                    PipelineRow::Param { field, .. } => {
                        let mut a = clone_app(&app);
                        // Paths and names rather than numbers: any non-empty value is a change.
                        let v = match (*field, a.field_text(field)) {
                            ("source_dicom" | "dicom_outputs" | "slurm_account" | "slurm_partition", _) => "x1".to_string(),
                            (_, v) => bumped(v),
                        };
                        *a.field_text_mut(field).unwrap_or_else(|| panic!("{field} not editable")) = v.clone();
                        check(format!("{field}={v}"), &app, &a, &mut problems);
                    }
                    _ => {}
                }
            }
        }

        // The controls that are not rows: the Input tab's filters and grouping, and the mask
        // editor. Each is (what, setup, change): the change is
        // checked against the state after `setup` alone.
        type Edit = fn(&mut App);
        let surfaces: &[(&str, Edit, Edit)] = &[
            ("orientation_group", |_| {}, |a| a.filter_state.orientation_group = "acq".into()),
            ("orientation_algorithm", |a| a.filter_state.orientation_group = "acq".into(), |a| a.filter_state.orientation_algorithm = 1),
            ("orientation_lambda", |a| a.filter_state.orientation_group = "acq".into(), |a| a.filter_state.orientation_lambda = "0.5".into()),
            ("bet_smoothness", |a| a.pipeline_state.apply_mask_preset(1), |a| a.pipeline_state.bet_smoothness = "2".into()),
            ("bet_gradient_threshold", |a| a.pipeline_state.apply_mask_preset(1), |a| a.pipeline_state.bet_gradient_threshold = "0.3".into()),
            ("bet_iterations", |a| a.pipeline_state.apply_mask_preset(1), |a| a.pipeline_state.bet_iterations = "500".into()),
            ("bet_subdivisions", |a| a.pipeline_state.apply_mask_preset(1), |a| a.pipeline_state.bet_subdivisions = "3".into()),
            ("cosmos_magnitude_weighting", |a| a.filter_state.orientation_group = "acq".into(), |a| a.pipeline_state.cosmos_magnitude_weighting = true),
            ("cosmos_tol", |a| { a.filter_state.orientation_group = "acq".into(); a.pipeline_state.cosmos_magnitude_weighting = true; }, |a| a.pipeline_state.cosmos_tol = "1e-4".into()),
            ("cosmos_max_iter", |a| { a.filter_state.orientation_group = "acq".into(); a.pipeline_state.cosmos_magnitude_weighting = true; }, |a| a.pipeline_state.cosmos_max_iter = "250".into()),
            ("mask sections", |_| {}, |a| {
                let mut extra = a.pipeline_state.mask_sections[0].clone();
                extra.input = crate::pipeline::config::MaskingInput::Magnitude;
                a.pipeline_state.mask_sections.push(extra);
            }),
            ("mask combine", |a| {
                let extra = a.pipeline_state.mask_sections[0].clone();
                a.pipeline_state.mask_sections.push(extra);
            }, |a| a.pipeline_state.toggle_mask_combine()),
            ("mask refinements", |a| {
                let extra = a.pipeline_state.mask_sections[0].clone();
                a.pipeline_state.mask_sections.push(extra);
            }, |a| a.pipeline_state.mask_combined_refinements.push(crate::pipeline::config::MaskOp::Erode { iterations: 1 })),
            ("masking input", |a| a.pipeline_state.set_select("mask_preset", 1), |a| {
                for s in &mut a.pipeline_state.mask_sections { s.input = crate::pipeline::config::MaskingInput::MagnitudeLast; }
            }),
            ("two-pass sections", |a| a.pipeline_state.two_pass = true, |a| {
                a.pipeline_state.two_pass_sections[0].refinements.push(crate::pipeline::config::MaskOp::Erode { iterations: 2 });
            }),
            // Run-level settings: not in the pipeline config, so compared on the run's arguments.
            ("config", |_| {}, |a| a.form.config_file = "c.toml".into()),
            ("include", |_| {}, |a| a.filter_state.include_pattern = "sub-1*".into()),
            ("exclude", |_| {}, |a| a.filter_state.exclude_pattern = "*pre*".into()),
            ("num_echoes", |_| {}, |a| a.filter_state.num_echoes = "2".into()),
            // The memory-limit row's third state (auto, a number, none).
            ("no_mem_limit", |_| {}, |a| a.form.mem_limit_gb = "none".into()),
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
            ("--romeo-individual", "restates the default"),
            ("--no-axial-resampling", "restates the default"),
            ("--romeo-correct-global", "restates the default"),
            ("--medi-smv", "restates the default"),
            ("--no-romeo-mag-weight", "restates the default (kept for compatibility)"),
            // The per-run half of a distributed run: the TUI's SLURM mode generates it (through
            // `qsmxt slurm`) and its local mode is a whole run.
            ("--no-dataset-steps", "worker mode, generated by qsmxt slurm"),
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
