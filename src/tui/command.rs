use std::path::PathBuf;

use crate::cli::*;
use crate::pipeline::config::{PipelineConfig, QsmAlgorithm, UnwrappingAlgorithm, BfAlgorithm, QsmReference, B0Estimation, B0WeightType};
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

    // Pipeline flags — delegate to qsmxt-config library
    {
        let config = config_from_app(app);
        let lib_cmd = qsmxt_config::generate_command(&config);
        // The library generates "qsmxt run <bids_dir> [flags]"
        // Strip the prefix and append only the flags
        let flags = lib_cmd.strip_prefix("qsmxt run <bids_dir>").unwrap_or(&lib_cmd).trim();
        if !flags.is_empty() {
            for flag in flags.split_whitespace() {
                parts.push(flag.to_string());
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
        push_if_changed(&mut parts, "--mem", &form.slurm_mem, &slurm_defaults.slurm_mem);
        push_if_changed(&mut parts, "--cpus-per-task", &form.slurm_cpus, &slurm_defaults.slurm_cpus);
        if form.slurm_submit {
            parts.push("--submit".to_string());
        }
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
    }

    parts.join(" ")
}

fn push_if_changed(parts: &mut Vec<String>, flag: &str, current: &str, default: &str) {
    if current.trim() != default.trim() {
        parts.push(format!("{} {}", flag, current.trim()));
    }
}

pub fn build_run_args(app: &App) -> crate::Result<RunArgs> {
    let form = &app.form;
    let ps = &app.pipeline_state;
    if form.bids_dir.is_empty() {
        return Err(crate::error::QsmxtError::Config(
            "BIDS directory is required".to_string(),
        ));
    }

    let qsm_options = [
        QsmAlgorithmArg::Rts,
        QsmAlgorithmArg::Tv,
        QsmAlgorithmArg::Tkd,
        QsmAlgorithmArg::Tsvd,
        QsmAlgorithmArg::Tgv,
        QsmAlgorithmArg::Tikhonov,
        QsmAlgorithmArg::Nltv,
        QsmAlgorithmArg::Medi,
        QsmAlgorithmArg::Ilsqr,
        QsmAlgorithmArg::Qsmart,
    ];
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
    ];
    Ok(RunArgs {
        bids_dir: expand_tilde(&form.bids_dir),
        output_dir: if form.output_dir.is_empty() { None } else { Some(expand_tilde(&form.output_dir)) },
        config: parse_optional_path(&form.config_file),
        include: app.filter_state.get_include_exclude().0,
        exclude: app.filter_state.get_include_exclude().1,
        num_echoes: parse_optional_usize(&app.filter_state.num_echoes),
        qsm_algorithm: Some(qsm_options[ps.qsm_algorithm]),
        unwrapping_algorithm: Some(unwrap_options[ps.unwrapping_algorithm]),
        bf_algorithm: Some(bf_options[ps.bf_algorithm]),
        masking_algorithm: None,
        masking_input: None,
        phase_offset_removal: Some(ps.phase_offset_removal),
        phase_offset_sigma: None,
        bipolar_correction: ps.bipolar_correction,
        romeo_individual: ps.romeo_individual,
        no_romeo_individual: !ps.romeo_individual,
        no_romeo_correct_global: !ps.romeo_correct_global,
        romeo_template: None,
        b0_estimation: None,
        b0_weight_type: None,
        bet_fractional_intensity: parse_optional_f64(&ps.bet_fractional_intensity),
        bet_smoothness: parse_optional_f64(&ps.bet_smoothness),
        bet_gradient_threshold: parse_optional_f64(&ps.bet_gradient_threshold),
        bet_iterations: parse_optional_usize(&ps.bet_iterations),
        bet_subdivisions: parse_optional_usize(&ps.bet_subdivisions),
        qsm_reference: match ps.qsm_reference {
            0 => Some(crate::cli::QsmReferenceArg::Mean),
            1 => Some(crate::cli::QsmReferenceArg::None),
            _ => None,
        },
        mask_erosions: None,
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
            tgv_step_size: None,
            tgv_tol: None,
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
            medi_merit: None,
            medi_smv: ps.medi_smv,
            medi_smv_radius: parse_optional_f64(&ps.medi_smv_radius),
            medi_data_weighting: None,
            medi_percentage: parse_optional_f64(&ps.medi_percentage),
            medi_cg_tol: parse_optional_f64(&ps.medi_cg_tol),
            medi_cg_max_iter: parse_optional_usize(&ps.medi_cg_max_iter),
            medi_max_iter: parse_optional_usize(&ps.medi_max_iter),
            medi_tol: parse_optional_f64(&ps.medi_tol),
        },
        ilsqr_params: crate::cli::IlsqrParamArgs {
            ilsqr_tol: parse_optional_f64(&ps.ilsqr_tol),
            ilsqr_max_iter: parse_optional_usize(&ps.ilsqr_max_iter),
        },
        qsmart_params: crate::cli::QsmartParamArgs {
            qsmart_ilsqr_tol: parse_optional_f64(&ps.qsmart_ilsqr_tol),
            qsmart_ilsqr_max_iter: parse_optional_usize(&ps.qsmart_ilsqr_max_iter),
            qsmart_vasc_sphere_radius: ps.qsmart_vasc_sphere_radius.trim().parse::<i32>().ok(),
            qsmart_sdf_spatial_radius: ps.qsmart_sdf_spatial_radius.trim().parse::<i32>().ok(),
            qsmart_inversion: if ps.qsm_algorithm == 9 {
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
        vsharp_params: crate::cli::VsharpParamArgs {
            vsharp_threshold: parse_optional_f64(&ps.vsharp_threshold),
            vsharp_max_radius: parse_optional_f64(&ps.vsharp_max_radius),
            vsharp_min_radius: parse_optional_f64(&ps.vsharp_min_radius),
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
            iharperella_radius: parse_optional_f64(&ps.iharperella_radius),
            iharperella_max_iter: parse_optional_usize(&ps.iharperella_max_iter),
            iharperella_tol: parse_optional_f64(&ps.iharperella_tol),
        },
        romeo_params: crate::cli::RomeoParamArgs {
            no_romeo_phase_gradient_coherence: !ps.romeo_phase_gradient_coherence,
            no_romeo_mag_coherence: !ps.romeo_mag_coherence,
            no_romeo_mag_weight: !ps.romeo_mag_weight,
        },
        swi_params: crate::cli::SwiParamArgs {
            swi_hp_sigma: {
                let x: Option<f64> = form.swi_hp_sigma_x.trim().parse().ok();
                let y: Option<f64> = form.swi_hp_sigma_y.trim().parse().ok();
                let z: Option<f64> = form.swi_hp_sigma_z.trim().parse().ok();
                match (x, y, z) {
                    (Some(a), Some(b), Some(c)) => Some(vec![a, b, c]),
                    _ => None,
                }
            },
            swi_scaling: {
                let scaling_options = ["tanh", "negative-tanh", "positive", "negative", "triangular"];
                Some(scaling_options.get(form.swi_scaling).unwrap_or(&"tanh").to_string())
            },
            swi_strength: parse_optional_f64(&form.swi_strength),
            swi_mip_window: parse_optional_usize(&form.swi_mip_window),
        },
        n_procs: parse_optional_usize(&form.n_procs),
        homogeneity_sigma_mm: None,
        homogeneity_nbox: None,
        linear_fit_reliability_threshold: None,
        no_qsm: !ps.do_qsm,
        do_swi: form.do_swi,
        do_t2starmap: form.do_t2starmap,
        do_r2starmap: form.do_r2starmap,
        export_dicom: form.export_dicom,
        source_dicom: None,
        dicom_outputs: None,
        inhomogeneity_correction: ps.inhomogeneity_correction,
        no_inhomogeneity_correction: !ps.inhomogeneity_correction,
        obliquity_threshold: parse_optional_f64(&ps.obliquity_threshold),
        mask_preset: None,
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
        dry: form.dry_run,
        debug: form.debug,
        mem_limit_gb: None,
        no_mem_limit: false,
        force: false,
        clean_intermediates: false,
    })
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

fn parse_optional_f64(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse().ok()
    }
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
        mem: form.slurm_mem.trim().parse().unwrap_or(32),
        cpus_per_task: form.slurm_cpus.trim().parse().unwrap_or(4),
        submit: form.slurm_submit,
        include,
        exclude,
        num_echoes: parse_optional_usize(&app.filter_state.num_echoes),
    })
}

/// Build a PipelineConfig reflecting the current TUI state (for methods preview).
pub fn config_from_app(app: &App) -> PipelineConfig {
    let ps = &app.pipeline_state;
    let qsm_algorithms = [
        QsmAlgorithm::Rts, QsmAlgorithm::Tv, QsmAlgorithm::Tkd, QsmAlgorithm::Tsvd,
        QsmAlgorithm::Tgv, QsmAlgorithm::Tikhonov, QsmAlgorithm::Nltv, QsmAlgorithm::Medi,
        QsmAlgorithm::Ilsqr, QsmAlgorithm::Qsmart,
    ];
    let unwrap_algorithms = [UnwrappingAlgorithm::Romeo, UnwrappingAlgorithm::Laplacian];
    let bf_algorithms = [
        BfAlgorithm::Vsharp, BfAlgorithm::Pdf, BfAlgorithm::Lbv,
        BfAlgorithm::Ismv, BfAlgorithm::Sharp, BfAlgorithm::Resharp,
        BfAlgorithm::Harperella, BfAlgorithm::Iharperella,
    ];

    let qsm_algorithm = qsm_algorithms[ps.qsm_algorithm];
    let is_end_to_end = matches!(qsm_algorithm, QsmAlgorithm::Tgv | QsmAlgorithm::Qsmart);

    let mut config = PipelineConfig::default();
    config.pipeline.do_qsm = ps.do_qsm;
    config.pipeline.do_swi = app.form.do_swi;
    config.pipeline.do_t2starmap = app.form.do_t2starmap;
    config.pipeline.do_r2starmap = app.form.do_r2starmap;
    config.pipeline.export_dicom = app.form.export_dicom;
    config.masking.inhomogeneity_correction = ps.inhomogeneity_correction;
    config.inversion.algorithm = qsm_algorithm;
    if !is_end_to_end {
        config.field_mapping.unwrapping_algorithm = unwrap_algorithms[ps.unwrapping_algorithm];
        config.bg_removal.algorithm = bf_algorithms[ps.bf_algorithm];
    }
    config.field_mapping.phase_offset_removal = ps.phase_offset_removal;
    config.field_mapping.bipolar_correction = ps.bipolar_correction;
    config.field_mapping.romeo.individual = ps.romeo_individual;
    config.field_mapping.romeo.correct_global = ps.romeo_correct_global;
    config.field_mapping.romeo.template = ps.romeo_template.trim().parse::<usize>().unwrap_or(1).saturating_sub(1);
    config.field_mapping.b0_estimation = match ps.b0_estimation {
        0 => B0Estimation::WeightedAvg,
        _ => B0Estimation::LinearFit,
    };
    config.field_mapping.b0_weight_type = match ps.b0_weight_type {
        0 => B0WeightType::PhaseSNR,
        1 => B0WeightType::PhaseVar,
        2 => B0WeightType::Average,
        3 => B0WeightType::TEs,
        _ => B0WeightType::Mag,
    };
    config.qsm.reference = match ps.qsm_reference {
        0 => QsmReference::Mean,
        _ => QsmReference::None,
    };
    config.masking.sections = ps.mask_sections.clone();

    // Obliquity
    if let Ok(v) = ps.obliquity_threshold.trim().parse::<f64>() { config.pipeline.obliquity_threshold = v; }

    // Parse algorithm parameters from TUI form strings
    macro_rules! set_f64 { ($dst:expr, $src:expr) => { if let Ok(v) = $src.trim().parse::<f64>() { $dst = v; } } }
    macro_rules! set_usize { ($dst:expr, $src:expr) => { if let Ok(v) = $src.trim().parse::<usize>() { $dst = v; } } }

    // BET
    set_f64!(config.bet.fractional_intensity, ps.bet_fractional_intensity);
    set_f64!(config.bet.smoothness, ps.bet_smoothness);
    set_f64!(config.bet.gradient_threshold, ps.bet_gradient_threshold);
    set_usize!(config.bet.iterations, ps.bet_iterations);
    set_usize!(config.bet.subdivisions, ps.bet_subdivisions);

    // RTS
    set_f64!(config.inversion.rts.delta, ps.rts_delta);
    set_f64!(config.inversion.rts.mu, ps.rts_mu);
    set_f64!(config.inversion.rts.tol, ps.rts_tol);
    set_f64!(config.inversion.rts.rho, ps.rts_rho);
    set_usize!(config.inversion.rts.max_iter, ps.rts_max_iter);
    set_usize!(config.inversion.rts.lsmr_iter, ps.rts_lsmr_iter);

    // TV
    set_f64!(config.inversion.tv.lambda, ps.tv_lambda);
    set_f64!(config.inversion.tv.rho, ps.tv_rho);
    set_f64!(config.inversion.tv.tol, ps.tv_tol);
    set_usize!(config.inversion.tv.max_iter, ps.tv_max_iter);

    // TKD / TSVD
    set_f64!(config.inversion.tkd.threshold, ps.tkd_threshold);
    set_f64!(config.inversion.tsvd.threshold, ps.tsvd_threshold);

    // iLSQR
    set_f64!(config.inversion.ilsqr.tol, ps.ilsqr_tol);
    set_usize!(config.inversion.ilsqr.max_iter, ps.ilsqr_max_iter);

    // Tikhonov
    set_f64!(config.inversion.tikhonov.lambda, ps.tikhonov_lambda);

    // NLTV
    set_f64!(config.inversion.nltv.lambda, ps.nltv_lambda);
    set_f64!(config.inversion.nltv.mu, ps.nltv_mu);
    set_f64!(config.inversion.nltv.tol, ps.nltv_tol);
    set_usize!(config.inversion.nltv.max_iter, ps.nltv_max_iter);
    set_usize!(config.inversion.nltv.newton_iter, ps.nltv_newton_iter);

    // MEDI
    set_f64!(config.inversion.medi.lambda, ps.medi_lambda);
    set_usize!(config.inversion.medi.max_iter, ps.medi_max_iter);
    set_usize!(config.inversion.medi.cg_max_iter, ps.medi_cg_max_iter);
    set_f64!(config.inversion.medi.cg_tol, ps.medi_cg_tol);
    set_f64!(config.inversion.medi.tol, ps.medi_tol);
    set_f64!(config.inversion.medi.percentage, ps.medi_percentage);
    set_f64!(config.inversion.medi.smv_radius, ps.medi_smv_radius);
    config.inversion.medi.smv = ps.medi_smv;

    // TGV
    set_usize!(config.inversion.tgv.iterations, ps.tgv_iterations);
    set_usize!(config.inversion.tgv.erosions, ps.tgv_erosions);
    set_f64!(config.inversion.tgv.alpha1, ps.tgv_alpha1);
    set_f64!(config.inversion.tgv.alpha0, ps.tgv_alpha0);

    // QSMART
    set_f64!(config.inversion.qsmart.ilsqr_tol, ps.qsmart_ilsqr_tol);
    set_usize!(config.inversion.qsmart.ilsqr_max_iter, ps.qsmart_ilsqr_max_iter);
    let qsmart_inv_algorithms = [
        QsmAlgorithm::Ilsqr, QsmAlgorithm::Rts, QsmAlgorithm::Tv, QsmAlgorithm::Tkd,
        QsmAlgorithm::Tsvd, QsmAlgorithm::Tikhonov, QsmAlgorithm::Nltv, QsmAlgorithm::Medi,
    ];
    config.inversion.qsmart.inversion =
        qsmart_inv_algorithms.get(ps.qsmart_inversion).copied().unwrap_or(QsmAlgorithm::Ilsqr);
    set_f64!(config.inversion.qsmart.sdf_sigma1_stage1, ps.qsmart_sdf_sigma1_stage1);
    set_f64!(config.inversion.qsmart.sdf_sigma2_stage1, ps.qsmart_sdf_sigma2_stage1);
    set_f64!(config.inversion.qsmart.sdf_sigma1_stage2, ps.qsmart_sdf_sigma1_stage2);
    set_f64!(config.inversion.qsmart.sdf_sigma2_stage2, ps.qsmart_sdf_sigma2_stage2);
    set_f64!(config.inversion.qsmart.sdf_lower_lim, ps.qsmart_sdf_lower_lim);
    set_f64!(config.inversion.qsmart.sdf_curv_constant, ps.qsmart_sdf_curv_constant);
    set_f64!(config.inversion.qsmart.frangi_scale_min, ps.qsmart_frangi_scale_min);
    set_f64!(config.inversion.qsmart.frangi_scale_max, ps.qsmart_frangi_scale_max);
    set_f64!(config.inversion.qsmart.frangi_scale_ratio, ps.qsmart_frangi_scale_ratio);
    set_f64!(config.inversion.qsmart.frangi_c, ps.qsmart_frangi_c);

    // BG removal
    set_f64!(config.bg_removal.vsharp.threshold, ps.vsharp_threshold);
    set_f64!(config.bg_removal.vsharp.max_radius, ps.vsharp_max_radius);
    set_f64!(config.bg_removal.vsharp.min_radius, ps.vsharp_min_radius);
    set_f64!(config.bg_removal.pdf.tol, ps.pdf_tol);
    set_f64!(config.bg_removal.lbv.tol, ps.lbv_tol);
    set_f64!(config.bg_removal.ismv.tol, ps.ismv_tol);
    set_usize!(config.bg_removal.ismv.max_iter, ps.ismv_max_iter);
    set_f64!(config.bg_removal.ismv.radius, ps.ismv_radius);
    set_f64!(config.bg_removal.sharp.threshold, ps.sharp_threshold);
    set_f64!(config.bg_removal.sharp.radius, ps.sharp_radius);
    set_f64!(config.bg_removal.resharp.radius, ps.resharp_radius);
    set_f64!(config.bg_removal.resharp.tik_reg, ps.resharp_tik_reg);
    set_f64!(config.bg_removal.resharp.tol, ps.resharp_tol);
    set_usize!(config.bg_removal.resharp.max_iter, ps.resharp_max_iter);
    set_f64!(config.bg_removal.harperella.radius, ps.harperella_radius);
    set_usize!(config.bg_removal.harperella.max_iter, ps.harperella_max_iter);
    set_f64!(config.bg_removal.harperella.tol, ps.harperella_tol);
    set_f64!(config.bg_removal.iharperella.radius, ps.iharperella_radius);
    set_usize!(config.bg_removal.iharperella.max_iter, ps.iharperella_max_iter);
    set_f64!(config.bg_removal.iharperella.tol, ps.iharperella_tol);

    // Phase offset sigma
    if let Ok(vals) = ps.phase_offset_sigma.split_whitespace()
        .map(|s| s.parse::<f64>())
        .collect::<Result<Vec<_>, _>>() {
        if vals.len() == 3 { config.field_mapping.phase_offset_sigma = [vals[0], vals[1], vals[2]]; }
    }

    // ROMEO weight flags
    config.field_mapping.romeo.phase_gradient_coherence = ps.romeo_phase_gradient_coherence;
    config.field_mapping.romeo.mag_coherence = ps.romeo_mag_coherence;
    config.field_mapping.romeo.mag_weight = ps.romeo_mag_weight;

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::app::App;

    fn default_app() -> App {
        App::new()
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
        app.pipeline_state.bet_fractional_intensity = "0.3".to_string();
        app.pipeline_state.rts_delta = "0.2".to_string();
        app.pipeline_state.obliquity_threshold = "5".to_string();
        let cmd = build_command_string(&app);
        assert!(cmd.contains("--bet-fractional-intensity 0.3"));
        assert!(cmd.contains("--rts-delta 0.2"));
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
        assert_eq!(args.qsm_algorithm, Some(crate::cli::QsmAlgorithmArg::Rts));
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
        app.pipeline_state.bet_fractional_intensity = "0.3".to_string();
        app.pipeline_state.rts_delta = "0.2".to_string();
        app.pipeline_state.rts_mu = "1e5".to_string();
        app.pipeline_state.rts_tol = "1e-4".to_string();
        app.pipeline_state.tgv_iterations = "500".to_string();
        app.pipeline_state.tgv_erosions = "2".to_string();
        app.pipeline_state.tv_lambda = "0.001".to_string();
        app.pipeline_state.tkd_threshold = "0.15".to_string();
        app.form.n_procs = "8".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.bet_fractional_intensity, Some(0.3));
        assert_eq!(args.rts_params.rts_delta, Some(0.2));
        assert_eq!(args.tgv_params.tgv_iterations, Some(500));
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
        assert!(args.do_swi);
        assert!(args.do_t2starmap);
        assert!(args.do_r2starmap);
        assert!(args.inhomogeneity_correction);
        assert!(args.dry);
        assert!(args.debug);
        assert_eq!(args.phase_offset_removal, Some(true)); // default mcpc3ds
    }

    #[test]
    fn test_build_run_args_phase_offset_removal_disabled() {
        let mut app = default_app();
        app.form.bids_dir = "/b".to_string();
        app.form.output_dir = "/o".to_string();
        app.pipeline_state.phase_offset_removal = false;
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.phase_offset_removal, Some(false));
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
        app.pipeline_state.obliquity_threshold = "5.0".to_string();
        let args = build_run_args(&app).unwrap();
        assert_eq!(args.obliquity_threshold, Some(5.0));
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
        assert_eq!(args.mem, 32);
        assert_eq!(args.cpus_per_task, 4);
        assert!(!args.submit);
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
        let args = build_slurm_args(&app).unwrap();
        assert_eq!(args.output_dir, Some(PathBuf::from("/out")));
        assert_eq!(args.partition, Some("gpu".to_string()));
        assert_eq!(args.time, "04:00:00");
        assert_eq!(args.mem, 64);
        assert_eq!(args.cpus_per_task, 8);
        assert!(args.submit);
        assert_eq!(args.config, Some(PathBuf::from("config.toml")));
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
