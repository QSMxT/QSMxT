//! CLI command generation from PipelineConfig.

use crate::config::*;
use crate::enums::*;

/// Generate a `qsmxt run` CLI command from a pipeline configuration.
/// Compares against defaults and only emits flags that differ.
///
/// Settings that do not apply to the configuration (the TUI hides them; see
/// [`crate::schema::settings_view`]) are left out: they cannot change the run, so saying them
/// only makes the command longer and suggests they matter.
pub fn generate_command(config: &PipelineConfig) -> String {
    command_for(&crate::schema::prune_hidden(&config.normalised().0, &crate::schema::ViewFacts::default()))
}

fn command_for(config: &PipelineConfig) -> String {
    let d = PipelineConfig::default();
    let mut parts: Vec<String> = vec!["qsmxt".into(), "run".into(), "<bids_dir>".into()];

    // ── Multi-orientation (COSMOS / STI) ──
    // The pattern is what switches it on, so the other flags only appear alongside it.
    if config.multi_orientation.enabled() {
        let m = &config.multi_orientation;
        parts.push(format!("--orientation-group {}", shell_quote(&m.group_by)));
        parts.push(format!("--multi-orientation-algorithm {}", m.algorithm));
        emit_f64(&mut parts, "--multi-orientation-lambda", m.lambda, d.multi_orientation.lambda);
    }
    if config.multi_orientation.magnitude_weighting {
        let (m, dm) = (&config.multi_orientation, &d.multi_orientation);
        parts.push("--cosmos-magnitude-weighting".into());
        emit_f64(&mut parts, "--cosmos-tol", m.cosmos_tol, dm.cosmos_tol);
        if m.cosmos_max_iter != dm.cosmos_max_iter {
            parts.push(format!("--cosmos-max-iter {}", m.cosmos_max_iter));
        }
    }
    // Only matter with a grouping; without one they are pruned before they get here.
    if config.multi_orientation.force { parts.push("--multi-orientation-force".into()); }
    if !config.multi_orientation.register { parts.push("--no-orientation-registration".into()); }

    // ── Pipeline toggles ──
    if !config.pipeline.do_qsm { parts.push("--no-qsm".into()); }
    if config.pipeline.do_swi { parts.push("--do-swi".into()); }
    if config.pipeline.do_segmentation && !config.pipeline.do_analysis {
        parts.push("--do-segmentation".into());
    }
    if config.pipeline.do_analysis {
        // --do-analysis implies segmentation, so saying both adds noise.
        parts.push("--do-analysis".into());
    }
    if config.pipeline.do_segmentation || config.pipeline.do_analysis {
        let (c, dd) = (&config.segmentation, &d.segmentation);
        if c.version != dd.version { parts.push(format!("--synthseg-version {}", c.version)); }
        if let Some(crop) = c.crop { parts.push(format!("--synthseg-crop {crop}")); }
        if let Some(patch) = c.patch { parts.push(format!("--synthseg-patch {patch}")); }
        if !c.flip_averaging { parts.push("--no-synthseg-flip-averaging".into()); }
        if !c.topology_cleanup { parts.push("--no-synthseg-topology-cleanup".into()); }
        emit_f64(&mut parts, "--synthseg-sigma", c.sigma_smoothing, dd.sigma_smoothing);
    }
    // Said whenever set and a segmentation is made or read (otherwise pruned).
    match config.segmentation.custom_dseg_tool.as_deref() {
        Some("*") => parts.push("--use-custom-dseg".into()),
        Some(tool) => parts.push(format!("--use-custom-dseg {tool}")),
        None => {}
    }
    if config.pipeline.do_smwi {
        parts.push("--do-smwi".into());
        emit_f64(&mut parts, "--smwi-threshold", config.smwi.threshold_ppm, d.smwi.threshold_ppm);
        emit_f64(&mut parts, "--smwi-power", config.smwi.power, d.smwi.power);
        emit_usize(&mut parts, "--smwi-mip-window", config.smwi.mip_window, d.smwi.mip_window);
    }
    if config.pipeline.do_t2starmap { parts.push("--do-t2starmap".into()); }
    // R2*, R2 and R2' are chained: chi-separation may imply R2', and computing R2' implies R2*
    // (and R2, where it can be measured). Each flag is surfaced unless the rest of the config
    // implies it — asked of the same enforcement a run applies, so a flag the chosen method does
    // not imply (R2 under r2star-qsm, say) is never dropped.
    let implied = |clear: fn(&mut PipelineConfig), get: fn(&PipelineConfig) -> bool| {
        let mut c = config.clone();
        clear(&mut c);
        crate::config::enforce_separation_dependencies(&mut c);
        get(&c)
    };
    if config.pipeline.do_r2primemap
        && !implied(|c| c.pipeline.do_r2primemap = false, |c| c.pipeline.do_r2primemap) {
        parts.push("--do-r2primemap".into());
    }
    if config.pipeline.do_r2map && !implied(|c| c.pipeline.do_r2map = false, |c| c.pipeline.do_r2map) {
        parts.push("--do-r2map".into());
    }
    if config.pipeline.do_r2starmap
        && !implied(|c| c.pipeline.do_r2starmap = false, |c| c.pipeline.do_r2starmap) {
        parts.push("--do-r2starmap".into());
    }
    if config.pipeline.do_chi_separation { parts.push("--do-chisep".into()); }
    if let Some(tool) = &config.separation.custom_qsm_tool { parts.push(format!("--use-custom-qsm {}", tool)); }
    if let Some(tool) = &config.separation.custom_r2_tool { parts.push(format!("--use-custom-r2 {}", tool)); }
    if let Some(tool) = &config.separation.custom_r2prime_tool { parts.push(format!("--use-custom-r2prime {}", tool)); }
    // Only meaningful without a custom map, which always wins.
    if config.separation.r2prime_strategy != d.separation.r2prime_strategy
        && config.separation.custom_r2prime_tool.is_none()
    {
        parts.push(format!("--r2prime-strategy {}", config.separation.r2prime_strategy));
    }
    // R2PRIMEnet settings: only meaningful when R2' may be predicted.
    if config.separation.custom_r2prime_tool.is_none()
        && config.separation.r2prime_strategy != R2PrimeStrategy::Mese
    {
        let (n, dn) = (&config.separation.r2primenet, &d.separation.r2primenet);
        if n.patch != dn.patch {
            parts.push(format!("--r2primenet-patch {} {} {}", n.patch[0], n.patch[1], n.patch[2]));
        }
        if n.ignore_field_mismatch {
            parts.push("--r2primenet-ignore-field-mismatch".into());
        }
    }
    if config.pipeline.export_dicom { parts.push("--export-dicom".into()); }
    if config.pipeline.axial_resampling {
        parts.push("--axial-resampling".into());
        emit_f64(&mut parts, "--obliquity-threshold", config.pipeline.obliquity_threshold, d.pipeline.obliquity_threshold);
    }
    if config.pipeline.crop_to_mask != d.pipeline.crop_to_mask {
        parts.push("--crop-to-mask".to_string());
    }
    if config.pipeline.ignore_slice_gap {
        parts.push("--ignore-slice-gap".to_string());
    }
    if config.pipeline.fft_padding != d.pipeline.fft_padding {
        parts.push("--fft-padding".to_string());
    }
    emit_f64(&mut parts, "--crop-margin-mm", config.pipeline.crop_margin_mm, d.pipeline.crop_margin_mm);
    if config.pipeline.output_space != d.pipeline.output_space {
        parts.push(format!("--output-space {}", config.pipeline.output_space));
    }

    // ── Inhomogeneity ──
    if config.masking.inhomogeneity_correction != d.masking.inhomogeneity_correction {
        if config.masking.inhomogeneity_correction {
            parts.push("--inhomogeneity-correction".into());
        } else {
            parts.push("--no-inhomogeneity-correction".into());
        }
    }
    emit_f64(&mut parts, "--homogeneity-sigma-mm", config.homogeneity.sigma_mm, d.homogeneity.sigma_mm);
    emit_usize(&mut parts, "--homogeneity-nbox", config.homogeneity.nbox, d.homogeneity.nbox);

    // ── Custom masks (BYO from derivatives) ──
    if config.masking.custom_mask_tool != d.masking.custom_mask_tool {
        match config.masking.custom_mask_tool.as_deref() {
            Some("*") => parts.push("--use-custom-masks".into()),
            Some(tool) => parts.push(format!("--use-custom-masks {}", tool)),
            None => {}
        }
    }

    // ── Field mapping ──
    if config.field_mapping.phase_offset_removal != d.field_mapping.phase_offset_removal {
        parts.push(format!("--phase-offset-removal {}", config.field_mapping.phase_offset_removal));
    }
    emit_f64_arr3(&mut parts, "--phase-offset-sigma", &config.field_mapping.phase_offset_sigma, &d.field_mapping.phase_offset_sigma);
    emit_f64_arr3(&mut parts, "--coil-combination-sigma", &config.field_mapping.coil_combination_sigma, &d.field_mapping.coil_combination_sigma);
    if config.field_mapping.bipolar_correction { parts.push("--bipolar-correction".into()); }
    emit_enum(&mut parts, "--unwrapping-algorithm", &config.field_mapping.unwrapping_algorithm, &d.field_mapping.unwrapping_algorithm);
    emit_enum(&mut parts, "--laplacian-solver", &config.field_mapping.laplacian_solver, &d.field_mapping.laplacian_solver);
    if config.field_mapping.laplacian_solver == LaplacianSolver::Fft {
        let pad = config.field_mapping.laplacian_fft_pad;
        if pad != d.field_mapping.laplacian_fft_pad {
            parts.push(if pad[0] == pad[1] && pad[1] == pad[2] {
                format!("--laplacian-fft-pad {}", pad[0])
            } else {
                format!("--laplacian-fft-pad {} {} {}", pad[0], pad[1], pad[2])
            });
        }
    }
    // Motion correction (off by default).
    let (m, md) = (&config.motion_correction, &d.motion_correction);
    if m.enabled {
        parts.push("--motion-correction".into());
        emit_enum(&mut parts, "--motion-reference", &m.reference, &md.reference);
        if !m.chain { parts.push("--no-motion-chain".into()); }
        if m.chain { emit_usize(&mut parts, "--motion-chain-levels", m.chain_levels, md.chain_levels); }
        emit_f64(&mut parts, "--motion-min-ncc", m.min_ncc, md.min_ncc);
    }
    // Shared by motion correction and orientation co-registration; said when either registers.
    {
        let (r, rd) = (&config.registration, &d.registration);
        emit_usize(&mut parts, "--registration-levels", r.levels, rd.levels);
        emit_usize(&mut parts, "--registration-max-samples", r.max_samples, rd.max_samples);
        emit_usize(&mut parts, "--registration-max-iterations", r.max_iterations, rd.max_iterations);
        emit_usize(&mut parts, "--registration-max-evaluations", r.max_evaluations, rd.max_evaluations);
        emit_f64(&mut parts, "--registration-tolerance", r.tolerance, rd.tolerance);
        emit_f64(&mut parts, "--registration-rotation-step-deg", r.rotation_step_deg, rd.rotation_step_deg);
        emit_f64(&mut parts, "--registration-translation-step-mm", r.translation_step_mm, rd.translation_step_mm);
        emit_f64(&mut parts, "--registration-min-overlap", r.min_overlap, rd.min_overlap);
    }
    emit_enum(&mut parts, "--b0-estimation", &config.field_mapping.b0_estimation, &d.field_mapping.b0_estimation);
    emit_enum(&mut parts, "--b0-weight-type", &config.field_mapping.b0_weight_type, &d.field_mapping.b0_weight_type);
    if config.field_mapping.b0_weight_type == B0WeightType::AssumedDecay {
        emit_f64(&mut parts, "--b0-weight-assumed-t2star", config.field_mapping.b0_weight_assumed_t2star_ms, d.field_mapping.b0_weight_assumed_t2star_ms);
    }
    // The fits' shared settings, and the robust fit's own.
    if config.field_mapping.b0_estimation != B0Estimation::WeightedAvg {
        let (l, ld) = (&config.field_mapping.linear_fit, &d.field_mapping.linear_fit);
        if l.estimate_offset != ld.estimate_offset {
            parts.push(format!("--linear-fit-estimate-offset {}", l.estimate_offset));
        }
        emit_f64(&mut parts, "--linear-fit-reliability-threshold", l.reliability_threshold_percentile, ld.reliability_threshold_percentile);
    }
    if config.field_mapping.b0_estimation == B0Estimation::RobustFit {
        let (r, rd) = (&config.field_mapping.robust_fit, &d.field_mapping.robust_fit);
        emit_enum(&mut parts, "--robust-fit-loss", &r.loss, &rd.loss);
        if let Some(t) = r.tuning { parts.push(format!("--robust-fit-tuning {}", t)); }
        emit_usize(&mut parts, "--robust-fit-iterations", r.iterations, rd.iterations);
        emit_f64(&mut parts, "--robust-fit-downweight-level", r.downweight_level, rd.downweight_level);
        emit_f64(&mut parts, "--robust-fit-flag-ratio", r.flag_ratio, rd.flag_ratio);
    }

    // ROMEO params: every one that differs from the default, in whichever direction it differs.
    let r = &config.field_mapping.romeo;
    let rd = &d.field_mapping.romeo;
    // Each switch has a flag for the direction away from QSMxT's default.
    let mut emit_bool = |val: bool, default: bool, on: &str, off: &str| {
        if val != default { parts.push(if val { on } else { off }.to_string()); }
    };
    debug_assert!(rd.phase_coherence && rd.phase_gradient_coherence && rd.phase_linearity && rd.mag_coherence
        && !rd.mag_weight2 && !rd.bestpath && !rd.merge_regions && !rd.correct_regions,
        "a ROMEO default changed: check the flags below still exist for the new direction");
    emit_bool(r.individual, rd.individual, "--romeo-individual", "--no-romeo-individual");
    emit_bool(r.correct_global, rd.correct_global, "--romeo-correct-global", "--no-romeo-correct-global");
    emit_bool(r.phase_coherence, rd.phase_coherence, "--romeo-phase-coherence", "--no-romeo-phase-coherence");
    emit_bool(r.phase_gradient_coherence, rd.phase_gradient_coherence,
              "--romeo-phase-gradient-coherence", "--no-romeo-phase-gradient-coherence");
    emit_bool(r.phase_linearity, rd.phase_linearity, "--romeo-phase-linearity", "--no-romeo-phase-linearity");
    emit_bool(r.mag_coherence, rd.mag_coherence, "--romeo-mag-coherence", "--no-romeo-mag-coherence");
    emit_bool(r.mag_weight, rd.mag_weight, "--romeo-mag-weight", "--no-romeo-mag-weight");
    emit_bool(r.mag_weight2, rd.mag_weight2, "--romeo-mag-weight2", "--no-romeo-mag-weight2");
    emit_bool(r.bestpath, rd.bestpath, "--romeo-bestpath", "--no-romeo-bestpath");
    emit_bool(r.merge_regions, rd.merge_regions, "--romeo-merge-regions", "--no-romeo-merge-regions");
    emit_bool(r.correct_regions, rd.correct_regions, "--romeo-correct-regions", "--no-romeo-correct-regions");
    if !r.individual { emit_usize(&mut parts, "--romeo-template", r.template + 1, rd.template + 1); }
    emit_f64(&mut parts, "--romeo-temporal-uncertain-unwrapping", r.temporal_uncertain_unwrapping, rd.temporal_uncertain_unwrapping);
    emit_usize(&mut parts, "--romeo-max-seeds", r.max_seeds as usize, rd.max_seeds as usize);
    emit_f64(&mut parts, "--romeo-wrap-addition", r.wrap_addition, rd.wrap_addition);

    // ── Masking ──
    // A preset says in three words what spelling the sections out takes a line and a half to say,
    // so print the preset whenever the configuration is one. `masking_input` is emitted here
    // rather than with the other field-mapping flags because it only ever qualifies a preset.
    let masking_default = config.masking.sections == d.masking.sections
        && config.masking.combine == d.masking.combine
        && config.masking.refinements == d.masking.refinements;
    match crate::masking::masking_preset_command(&config.masking) {
        // The default recipe is a preset too; saying so adds noise rather than removing it. Except
        // under QSMART, where untouched masking means BET (see `apply_run_overrides`), so the
        // default recipe has to be said to be kept.
        _ if masking_default && config.inversion.algorithm != QsmAlgorithm::Qsmart => {}
        Some((preset, input)) => {
            parts.push(format!("--mask-preset {preset}"));
            if let Some(input) = input {
                parts.push(format!("--masking-input {input}"));
            }
        }
        None => {
            for section in &config.masking.sections {
                parts.push(format!("--mask {}", section.compact_spec()));
            }
            if config.masking.combine != d.masking.combine {
                parts.push(format!("--mask-combine {}", config.masking.combine));
            }
            for op in &config.masking.refinements {
                parts.push(format!("--mask-refine {}", op.compact_spec()));
            }
        }
    }

    // ── Two-pass ──
    // Off by default, so the bare flag is enough to turn it on; the reliable mask only needs
    // spelling out once it differs from the default recipe.
    if config.masking.two_pass {
        parts.push("--two-pass".into());
        if let Some(sections) = &config.masking.two_pass_sections {
            if *sections != crate::masking::default_two_pass_sections() {
                for section in sections {
                    parts.push(format!("--two-pass-mask {}", section.compact_spec()));
                }
            }
        }
    }

    // ── BET ──
    emit_f64(&mut parts, "--bet-fractional-intensity", config.bet.fractional_intensity, d.bet.fractional_intensity);
    emit_f64(&mut parts, "--bet-smoothness", config.bet.smoothness, d.bet.smoothness);
    emit_f64(&mut parts, "--bet-gradient-threshold", config.bet.gradient_threshold, d.bet.gradient_threshold);
    emit_usize(&mut parts, "--bet-iterations", config.bet.iterations, d.bet.iterations);
    emit_usize(&mut parts, "--bet-subdivisions", config.bet.subdivisions, d.bet.subdivisions);
    emit_f64(&mut parts, "--bet-voxel-scale", config.bet.voxel_scale, d.bet.voxel_scale);

    // ── Background removal ──
    emit_enum(&mut parts, "--bf-algorithm", &config.bg_removal.algorithm, &d.bg_removal.algorithm);
    match config.bg_removal.algorithm {
        BfAlgorithm::Vsharp => {
            if config.bg_removal.vsharp.deconvolve != d.bg_removal.vsharp.deconvolve {
                parts.push(format!("--vsharp-deconvolve {}", config.bg_removal.vsharp.deconvolve));
            }
            if config.bg_removal.vsharp.deconvolve {
                emit_f64(&mut parts, "--vsharp-threshold", config.bg_removal.vsharp.threshold, d.bg_removal.vsharp.threshold);
            }
            emit_f64(&mut parts, "--vsharp-max-radius", config.bg_removal.vsharp.max_radius, d.bg_removal.vsharp.max_radius);
            emit_f64(&mut parts, "--vsharp-min-radius", config.bg_removal.vsharp.min_radius, d.bg_removal.vsharp.min_radius);
            emit_enum(&mut parts, "--vsharp-grid", &config.bg_removal.vsharp.grid, &d.bg_removal.vsharp.grid);
            if config.bg_removal.vsharp.grid == VsharpGrid::Isotropic {
                emit_f64(&mut parts, "--vsharp-grid-spacing", config.bg_removal.vsharp.grid_spacing, d.bg_removal.vsharp.grid_spacing);
            }
        }
        BfAlgorithm::Pdf => { emit_f64(&mut parts, "--pdf-tol", config.bg_removal.pdf.tol, d.bg_removal.pdf.tol); }
        BfAlgorithm::Lbv => { emit_f64(&mut parts, "--lbv-tol", config.bg_removal.lbv.tol, d.bg_removal.lbv.tol); }
        BfAlgorithm::Ismv => {
            emit_f64(&mut parts, "--ismv-tol", config.bg_removal.ismv.tol, d.bg_removal.ismv.tol);
            emit_usize(&mut parts, "--ismv-max-iter", config.bg_removal.ismv.max_iter, d.bg_removal.ismv.max_iter);
            emit_f64(&mut parts, "--ismv-radius", config.bg_removal.ismv.radius, d.bg_removal.ismv.radius);
        }
        BfAlgorithm::Sharp => {
            emit_f64(&mut parts, "--sharp-threshold", config.bg_removal.sharp.threshold, d.bg_removal.sharp.threshold);
            emit_f64(&mut parts, "--sharp-radius", config.bg_removal.sharp.radius, d.bg_removal.sharp.radius);
        }
        BfAlgorithm::Resharp => {
            emit_f64(&mut parts, "--resharp-radius", config.bg_removal.resharp.radius, d.bg_removal.resharp.radius);
            emit_f64(&mut parts, "--resharp-tik-reg", config.bg_removal.resharp.tik_reg, d.bg_removal.resharp.tik_reg);
            emit_f64(&mut parts, "--resharp-tol", config.bg_removal.resharp.tol, d.bg_removal.resharp.tol);
            emit_usize(&mut parts, "--resharp-max-iter", config.bg_removal.resharp.max_iter, d.bg_removal.resharp.max_iter);
        }
        BfAlgorithm::Harperella => {
            emit_f64(&mut parts, "--harperella-radius", config.bg_removal.harperella.radius, d.bg_removal.harperella.radius);
            emit_usize(&mut parts, "--harperella-max-iter", config.bg_removal.harperella.max_iter, d.bg_removal.harperella.max_iter);
            emit_f64(&mut parts, "--harperella-tol", config.bg_removal.harperella.tol, d.bg_removal.harperella.tol);
        }
        BfAlgorithm::Iharperella => {
            emit_usize(&mut parts, "--iharperella-max-iter", config.bg_removal.iharperella.max_iter, d.bg_removal.iharperella.max_iter);
            emit_f64(&mut parts, "--iharperella-tol", config.bg_removal.iharperella.tol, d.bg_removal.iharperella.tol);
        }
        // BFRnet / iQFM (deep learning) have no user-tunable parameters.
        BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => {}
    }
    // mSMV boundary-shadow refinement is a post-step on any primary BFR (not part of the
    // match above); emit its flag + params only when enabled.
    if config.bg_removal.msmv_refine {
        parts.push("--msmv-refine".into());
        emit_f64(&mut parts, "--msmv-radius", config.bg_removal.msmv.radius, d.bg_removal.msmv.radius);
        emit_usize(&mut parts, "--msmv-maxk", config.bg_removal.msmv.maxk, d.bg_removal.msmv.maxk);
    }

    // ── QSM inversion ──
    emit_enum(&mut parts, "--qsm-algorithm", &config.inversion.algorithm, &d.inversion.algorithm);
    emit_inversion_params(&mut parts, config, &d, config.inversion.algorithm);

    // ── Chi-separation method + params (only the selected method's) ──
    if config.pipeline.do_chi_separation {
        emit_enum(&mut parts, "--chisep", &config.separation.algorithm, &d.separation.algorithm);
        let s = &config.separation;
        let ds = &d.separation;
        match config.separation.algorithm {
            SeparationAlgorithm::R2starQsm => {
                emit_f64(&mut parts, "--r2star-qsm-r-const-3t", s.r2star_qsm.r_const_3t, ds.r2star_qsm.r_const_3t);
            }
            SeparationAlgorithm::Decompose => {
                emit_usize(&mut parts, "--decompose-n-inner", s.decompose.n_inner, ds.decompose.n_inner);
                emit_f64(&mut parts, "--decompose-chi-bound", s.decompose.chi_bound, ds.decompose.chi_bound);
                emit_usize(&mut parts, "--decompose-max-lm-iter", s.decompose.max_lm_iter, ds.decompose.max_lm_iter);
            }
            SeparationAlgorithm::ChiSepIlsqr => {
                emit_f64(&mut parts, "--chi-sep-ilsqr-dr-pos", s.chi_sep_ilsqr.dr_pos, ds.chi_sep_ilsqr.dr_pos);
                emit_f64(&mut parts, "--chi-sep-ilsqr-dr-neg", s.chi_sep_ilsqr.dr_neg, ds.chi_sep_ilsqr.dr_neg);
                emit_f64(&mut parts, "--chi-sep-ilsqr-lambda1", s.chi_sep_ilsqr.lambda1, ds.chi_sep_ilsqr.lambda1);
                emit_f64(&mut parts, "--chi-sep-ilsqr-percentage", s.chi_sep_ilsqr.percentage, ds.chi_sep_ilsqr.percentage);
                emit_f64(&mut parts, "--chi-sep-ilsqr-r2p-min", s.chi_sep_ilsqr.r2p_min, ds.chi_sep_ilsqr.r2p_min);
                emit_f64(&mut parts, "--chi-sep-ilsqr-r2p-max", s.chi_sep_ilsqr.r2p_max, ds.chi_sep_ilsqr.r2p_max);
                emit_usize(&mut parts, "--chi-sep-ilsqr-max-iter", s.chi_sep_ilsqr.max_iter, ds.chi_sep_ilsqr.max_iter);
                emit_f64(&mut parts, "--chi-sep-ilsqr-tol", s.chi_sep_ilsqr.tol, ds.chi_sep_ilsqr.tol);
                emit_usize(&mut parts, "--chi-sep-ilsqr-cg-max-iter", s.chi_sep_ilsqr.cg_max_iter, ds.chi_sep_ilsqr.cg_max_iter);
                emit_f64(&mut parts, "--chi-sep-ilsqr-cg-tol", s.chi_sep_ilsqr.cg_tol, ds.chi_sep_ilsqr.cg_tol);
            }
            SeparationAlgorithm::ChiSepMedi => {
                emit_f64(&mut parts, "--chi-sep-medi-lambda-para", s.chi_sep_medi.lambda_para, ds.chi_sep_medi.lambda_para);
                emit_f64(&mut parts, "--chi-sep-medi-lambda-dia", s.chi_sep_medi.lambda_dia, ds.chi_sep_medi.lambda_dia);
                emit_f64(&mut parts, "--chi-sep-medi-lambda-cpl", s.chi_sep_medi.lambda_cpl, ds.chi_sep_medi.lambda_cpl);
                emit_f64(&mut parts, "--chi-sep-medi-dr-pos", s.chi_sep_medi.dr_pos, ds.chi_sep_medi.dr_pos);
                emit_f64(&mut parts, "--chi-sep-medi-dr-neg", s.chi_sep_medi.dr_neg, ds.chi_sep_medi.dr_neg);
                emit_f64(&mut parts, "--chi-sep-medi-percentage", s.chi_sep_medi.percentage, ds.chi_sep_medi.percentage);
                emit_f64(&mut parts, "--chi-sep-medi-cg-tol", s.chi_sep_medi.cg_tol, ds.chi_sep_medi.cg_tol);
                emit_usize(&mut parts, "--chi-sep-medi-cg-max-iter", s.chi_sep_medi.cg_max_iter, ds.chi_sep_medi.cg_max_iter);
                emit_usize(&mut parts, "--chi-sep-medi-max-iter", s.chi_sep_medi.max_iter, ds.chi_sep_medi.max_iter);
                emit_f64(&mut parts, "--chi-sep-medi-tol", s.chi_sep_medi.tol, ds.chi_sep_medi.tol);
            }
            SeparationAlgorithm::WaveSep => {
                emit_f64(&mut parts, "--wavesep-dr-pos", s.wavesep.dr_pos, ds.wavesep.dr_pos);
                emit_f64(&mut parts, "--wavesep-dr-neg", s.wavesep.dr_neg, ds.wavesep.dr_neg);
                emit_f64(&mut parts, "--wavesep-alpha", s.wavesep.alpha, ds.wavesep.alpha);
                emit_f64(&mut parts, "--wavesep-lambda", s.wavesep.lambda, ds.wavesep.lambda);
                emit_usize(&mut parts, "--wavesep-wavelet-order", s.wavesep.wavelet_order, ds.wavesep.wavelet_order);
                emit_usize(&mut parts, "--wavesep-max-iter", s.wavesep.max_iter, ds.wavesep.max_iter);
                emit_f64(&mut parts, "--wavesep-tol", s.wavesep.tol, ds.wavesep.tol);
            }
            SeparationAlgorithm::HcChisep => {
                emit_f64(&mut parts, "--hc-chisep-dr-pos-3t", s.hc_chisep.dr_pos_3t, ds.hc_chisep.dr_pos_3t);
                emit_f64(&mut parts, "--hc-chisep-bin-hz", s.hc_chisep.bin_hz, ds.hc_chisep.bin_hz);
            }
            SeparationAlgorithm::ChiSepNet => {
                let p = s.chi_sepnet.patch;
                if p != ds.chi_sepnet.patch {
                    parts.push(format!("--chi-sepnet-patch {} {} {}", p[0], p[1], p[2]));
                }
            }
            SeparationAlgorithm::SusepNet => {
                if let Some(p) = s.susep_net.patch {
                    parts.push(format!("--susep-net-patch {} {} {}", p[0], p[1], p[2]));
                }
            }
        }
    }

    // ── QSM reference ──
    // One flag carries both fields: the method, or the region spec that replaces it.
    let (spec, default_spec) = (config.qsm.reference_spec(), d.qsm.reference_spec());
    if spec != default_spec {
        parts.push(format!("--qsm-reference {spec}"));
    }

    // ── SWI params ──
    if config.pipeline.do_swi {
        if config.swi.unwrap != d.swi.unwrap { parts.push(format!("--swi-unwrapping {}", config.swi.unwrap)); }
        emit_f64_arr3(&mut parts, "--swi-hp-sigma", &config.swi.hp_sigma, &d.swi.hp_sigma);
        if config.swi.scaling != d.swi.scaling { parts.push(format!("--swi-scaling {}", config.swi.scaling)); }
        emit_f64(&mut parts, "--swi-strength", config.swi.strength, d.swi.strength);
        emit_usize(&mut parts, "--swi-mip-window", config.swi.mip_window, d.swi.mip_window);
    }

    parts.join(" ")
}

// ─── Helpers ───

/// The flags for inversion `alg`'s own settings: the selected algorithm's, or QSMART's inner one.
fn emit_inversion_params(parts: &mut Vec<String>, config: &PipelineConfig, d: &PipelineConfig, alg: QsmAlgorithm) {
    match alg {
        QsmAlgorithm::Rts => {
            emit_f64(parts, "--rts-delta", config.inversion.rts.delta, d.inversion.rts.delta);
            emit_f64(parts, "--rts-mu", config.inversion.rts.mu, d.inversion.rts.mu);
            emit_f64(parts, "--rts-rho", config.inversion.rts.rho, d.inversion.rts.rho);
            emit_f64(parts, "--rts-tol", config.inversion.rts.tol, d.inversion.rts.tol);
            emit_usize(parts, "--rts-max-iter", config.inversion.rts.max_iter, d.inversion.rts.max_iter);
            emit_usize(parts, "--rts-lsmr-iter", config.inversion.rts.lsmr_iter, d.inversion.rts.lsmr_iter);
        }
        QsmAlgorithm::Tv => {
            emit_f64(parts, "--tv-lambda", config.inversion.tv.lambda, d.inversion.tv.lambda);
            emit_f64(parts, "--tv-rho", config.inversion.tv.rho, d.inversion.tv.rho);
            emit_f64(parts, "--tv-tol", config.inversion.tv.tol, d.inversion.tv.tol);
            emit_usize(parts, "--tv-max-iter", config.inversion.tv.max_iter, d.inversion.tv.max_iter);
        }
        QsmAlgorithm::Tkd => { emit_f64(parts, "--tkd-threshold", config.inversion.tkd.threshold, d.inversion.tkd.threshold); }
        QsmAlgorithm::Tsvd => { emit_f64(parts, "--tsvd-threshold", config.inversion.tsvd.threshold, d.inversion.tsvd.threshold); }
        QsmAlgorithm::Tikhonov => { emit_f64(parts, "--tikhonov-lambda", config.inversion.tikhonov.lambda, d.inversion.tikhonov.lambda); }
        QsmAlgorithm::Nltv => {
            emit_f64(parts, "--nltv-lambda", config.inversion.nltv.lambda, d.inversion.nltv.lambda);
            emit_f64(parts, "--nltv-mu", config.inversion.nltv.mu, d.inversion.nltv.mu);
            emit_f64(parts, "--nltv-tol", config.inversion.nltv.tol, d.inversion.nltv.tol);
            emit_usize(parts, "--nltv-max-iter", config.inversion.nltv.max_iter, d.inversion.nltv.max_iter);
            emit_usize(parts, "--nltv-newton-iter", config.inversion.nltv.newton_iter, d.inversion.nltv.newton_iter);
        }
        QsmAlgorithm::Medi => {
            emit_f64(parts, "--medi-lambda", config.inversion.medi.lambda, d.inversion.medi.lambda);
            emit_f64(parts, "--medi-percentage", config.inversion.medi.percentage, d.inversion.medi.percentage);
            emit_usize(parts, "--medi-max-iter", config.inversion.medi.max_iter, d.inversion.medi.max_iter);
            emit_usize(parts, "--medi-cg-max-iter", config.inversion.medi.cg_max_iter, d.inversion.medi.cg_max_iter);
            emit_f64(parts, "--medi-cg-tol", config.inversion.medi.cg_tol, d.inversion.medi.cg_tol);
            emit_f64(parts, "--medi-tol", config.inversion.medi.tol, d.inversion.medi.tol);
            emit_f64(parts, "--medi-smv-radius", config.inversion.medi.smv_radius, d.inversion.medi.smv_radius);
            if config.inversion.medi.merit != d.inversion.medi.merit {
                parts.push(format!("--medi-merit {}", config.inversion.medi.merit));
            }
            if config.inversion.medi.data_weighting != d.inversion.medi.data_weighting {
                parts.push(format!("--medi-data-weighting {}", config.inversion.medi.data_weighting));
            }
            if config.inversion.medi.smv != d.inversion.medi.smv {
                parts.push(if config.inversion.medi.smv { "--medi-smv" } else { "--no-medi-smv" }.into());
            }
        }
        QsmAlgorithm::Tfi => {
            emit_f64(parts, "--tfi-lambda", config.inversion.tfi.lambda, d.inversion.tfi.lambda);
            emit_f64(parts, "--tfi-precond", config.inversion.tfi.precond, d.inversion.tfi.precond);
            if config.inversion.tfi.merit != d.inversion.tfi.merit {
                parts.push(format!("--tfi-merit {}", config.inversion.tfi.merit));
            }
            if config.inversion.tfi.data_weighting != d.inversion.tfi.data_weighting {
                parts.push(format!("--tfi-data-weighting {}", config.inversion.tfi.data_weighting));
            }
            emit_f64(parts, "--tfi-percentage", config.inversion.tfi.percentage, d.inversion.tfi.percentage);
            emit_usize(parts, "--tfi-max-iter", config.inversion.tfi.max_iter, d.inversion.tfi.max_iter);
            emit_usize(parts, "--tfi-cg-max-iter", config.inversion.tfi.cg_max_iter, d.inversion.tfi.cg_max_iter);
            emit_f64(parts, "--tfi-cg-tol", config.inversion.tfi.cg_tol, d.inversion.tfi.cg_tol);
            emit_f64(parts, "--tfi-tol", config.inversion.tfi.tol, d.inversion.tfi.tol);
        }
        QsmAlgorithm::Ilsqr => {
            emit_f64(parts, "--ilsqr-tol", config.inversion.ilsqr.tol, d.inversion.ilsqr.tol);
            emit_usize(parts, "--ilsqr-max-iter", config.inversion.ilsqr.max_iter, d.inversion.ilsqr.max_iter);
            emit_f64(parts, "--ilsqr-pad-mm", config.inversion.ilsqr.pad_mm, d.inversion.ilsqr.pad_mm);
        }
        // HEIDI's seed is an LSQR solve, so it emits both groups.
        QsmAlgorithm::Lsqr | QsmAlgorithm::Heidi => {
            let (c, dd) = (&config.inversion.lsqr, &d.inversion.lsqr);
            if c.residual_weighting != dd.residual_weighting {
                if let Some(w) = c.residual_weighting {
                    parts.push(format!("--lsqr-residual-weighting {w}"));
                }
            }
            if c.fit_global_offset != dd.fit_global_offset && !c.fit_global_offset {
                parts.push("--no-lsqr-global-offset".into());
            }
            emit_f64(parts, "--lsqr-tol", c.tol, dd.tol);
            emit_usize(parts, "--lsqr-max-iter", c.max_iter, dd.max_iter);

            if config.inversion.algorithm == QsmAlgorithm::Heidi {
                let (c, dd) = (&config.inversion.heidi, &d.inversion.heidi);
                emit_f64(parts, "--heidi-cone-threshold", c.cone_threshold, dd.cone_threshold);
                emit_f64(parts, "--heidi-gradient-threshold", c.gradient_threshold, dd.gradient_threshold);
                if c.apply_laplacian_correction != dd.apply_laplacian_correction && !c.apply_laplacian_correction {
                    parts.push("--no-heidi-laplacian-correction".into());
                }
                emit_f64(parts, "--heidi-laplacian-threshold", c.laplacian_threshold, dd.laplacian_threshold);
                emit_f64(parts, "--heidi-gradient-mask-floor", c.gradient_mask_floor, dd.gradient_mask_floor);
                emit_usize(parts, "--heidi-continuation-steps", c.continuation_steps, dd.continuation_steps);
                emit_usize(parts, "--heidi-inner-iterations", c.inner_iterations, dd.inner_iterations);
                emit_f64(parts, "--heidi-mu-min", c.mu_min, dd.mu_min);
                emit_f64(parts, "--heidi-tol", c.tol, dd.tol);
                if c.denoise != dd.denoise && !c.denoise {
                    parts.push("--no-heidi-denoise".into());
                }
                if c.denoise {
                    emit_usize(parts, "--heidi-denoise-iterations", c.denoise_iterations, dd.denoise_iterations);
                    emit_f64(parts, "--heidi-denoise-time-step", c.denoise_time_step, dd.denoise_time_step);
                    emit_f64(parts, "--heidi-denoise-conductance", c.denoise_conductance, dd.denoise_conductance);
                }
            }
        }
        QsmAlgorithm::Tgv => {
            emit_usize(parts, "--tgv-iterations", config.inversion.tgv.iterations, d.inversion.tgv.iterations);
            emit_usize(parts, "--tgv-erosions", config.inversion.tgv.erosions, d.inversion.tgv.erosions);
            emit_f64(parts, "--tgv-alpha0", config.inversion.tgv.alpha0, d.inversion.tgv.alpha0);
            emit_f64(parts, "--tgv-alpha1", config.inversion.tgv.alpha1, d.inversion.tgv.alpha1);
            emit_f64(parts, "--tgv-step-size", config.inversion.tgv.step_size, d.inversion.tgv.step_size);
            emit_f64(parts, "--tgv-tol", config.inversion.tgv.tol, d.inversion.tgv.tol);
        }
        QsmAlgorithm::Qsmart => {
            let q = &config.inversion.qsmart;
            let dq = &d.inversion.qsmart;
            emit_enum(parts, "--qsmart-inversion", &q.inversion, &dq.inversion);
            emit_usize(parts, "--qsmart-ilsqr-max-iter", q.ilsqr_max_iter, dq.ilsqr_max_iter);
            emit_f64(parts, "--qsmart-ilsqr-pad-mm", q.ilsqr_pad_mm, dq.ilsqr_pad_mm);
            // The inner inversion's own settings (iLSQR's tolerance, RTS's, ...), which QSMART
            // reads from the same groups as a top-level run would. Its iteration cap is QSMART's.
            if q.inversion != QsmAlgorithm::Qsmart {
                emit_inversion_params(parts, config, d, q.inversion);
            }
            emit_i32(parts, "--qsmart-vasc-sphere-radius", q.vasc_sphere_radius, dq.vasc_sphere_radius);
            emit_i32(parts, "--qsmart-sdf-spatial-radius", q.sdf_spatial_radius, dq.sdf_spatial_radius);
            emit_f64(parts, "--qsmart-sdf-sigma1-stage1", q.sdf_sigma1_stage1, dq.sdf_sigma1_stage1);
            emit_f64(parts, "--qsmart-sdf-sigma2-stage1", q.sdf_sigma2_stage1, dq.sdf_sigma2_stage1);
            emit_f64(parts, "--qsmart-sdf-sigma1-stage2", q.sdf_sigma1_stage2, dq.sdf_sigma1_stage2);
            emit_f64(parts, "--qsmart-sdf-sigma2-stage2", q.sdf_sigma2_stage2, dq.sdf_sigma2_stage2);
            emit_f64(parts, "--qsmart-sdf-lower-lim", q.sdf_lower_lim, dq.sdf_lower_lim);
            emit_f64(parts, "--qsmart-sdf-curv-constant", q.sdf_curv_constant, dq.sdf_curv_constant);
            emit_f64(parts, "--qsmart-frangi-scale-min", q.frangi_scale_min, dq.frangi_scale_min);
            emit_f64(parts, "--qsmart-frangi-scale-max", q.frangi_scale_max, dq.frangi_scale_max);
            emit_f64(parts, "--qsmart-frangi-scale-ratio", q.frangi_scale_ratio, dq.frangi_scale_ratio);
            emit_f64(parts, "--qsmart-frangi-c", q.frangi_c, dq.frangi_c);
        }
        QsmAlgorithm::Ndi => {
            emit_f64(parts, "--ndi-tau", config.inversion.ndi.tau, d.inversion.ndi.tau);
            emit_f64(parts, "--ndi-alpha", config.inversion.ndi.alpha, d.inversion.ndi.alpha);
            emit_usize(parts, "--ndi-max-iter", config.inversion.ndi.max_iter, d.inversion.ndi.max_iter);
            emit_f64(parts, "--ndi-phase-scale", config.inversion.ndi.phase_scale, d.inversion.ndi.phase_scale);
        }
        QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv => {
            let fa = &config.inversion.fansi;
            let df = &d.inversion.fansi;
            emit_f64(parts, "--fansi-alpha1", fa.alpha1, df.alpha1);
            emit_f64(parts, "--fansi-mu1", fa.mu1, df.mu1);
            emit_f64(parts, "--fansi-mu2", fa.mu2, df.mu2);
            emit_f64(parts, "--fansi-alpha0", fa.alpha0, df.alpha0);
            emit_f64(parts, "--fansi-mu0", fa.mu0, df.mu0);
            emit_usize(parts, "--fansi-max-iter", fa.max_iter, df.max_iter);
            emit_f64(parts, "--fansi-tol-update", fa.tol_update, df.tol_update);
            emit_f64(parts, "--fansi-tol-delta", fa.tol_delta, df.tol_delta);
            emit_f64(parts, "--fansi-phase-scale", fa.phase_scale, df.phase_scale);
        }
        QsmAlgorithm::L1qsm => {
            let l = &config.inversion.l1qsm;
            let dl = &d.inversion.l1qsm;
            emit_f64(parts, "--l1qsm-alpha1", l.alpha1, dl.alpha1);
            emit_f64(parts, "--l1qsm-mu1", l.mu1, dl.mu1);
            emit_f64(parts, "--l1qsm-mu2", l.mu2, dl.mu2);
            emit_f64(parts, "--l1qsm-mu3", l.mu3, dl.mu3);
            emit_f64(parts, "--l1qsm-lambda", l.lambda, dl.lambda);
            emit_usize(parts, "--l1qsm-max-iter", l.max_iter, dl.max_iter);
            emit_f64(parts, "--l1qsm-tol-update", l.tol_update, dl.tol_update);
            emit_f64(parts, "--l1qsm-tol-delta", l.tol_delta, dl.tol_delta);
            emit_f64(parts, "--l1qsm-phase-scale", l.phase_scale, dl.phase_scale);
        }
        QsmAlgorithm::Whqsm => {
            let w = &config.inversion.whqsm;
            let dw = &d.inversion.whqsm;
            emit_f64(parts, "--whqsm-alpha1", w.alpha1, dw.alpha1);
            emit_f64(parts, "--whqsm-mu1", w.mu1, dw.mu1);
            emit_f64(parts, "--whqsm-mu2", w.mu2, dw.mu2);
            emit_f64(parts, "--whqsm-beta", w.beta, dw.beta);
            emit_f64(parts, "--whqsm-muh", w.muh, dw.muh);
            emit_usize(parts, "--whqsm-max-iter", w.max_iter, dw.max_iter);
            emit_f64(parts, "--whqsm-tol-update", w.tol_update, dw.tol_update);
            emit_f64(parts, "--whqsm-tol-delta", w.tol_delta, dw.tol_delta);
            emit_f64(parts, "--whqsm-phase-scale", w.phase_scale, dw.phase_scale);
        }
        QsmAlgorithm::Hdqsm => {
            let h = &config.inversion.hdqsm;
            let dh = &d.inversion.hdqsm;
            emit_f64(parts, "--hdqsm-alpha-l2", h.alpha_l2, dh.alpha_l2);
            emit_f64(parts, "--hdqsm-mu1-l2", h.mu1_l2, dh.mu1_l2);
            emit_f64(parts, "--hdqsm-mu2", h.mu2, dh.mu2);
            emit_usize(parts, "--hdqsm-max-iter-l1", h.max_iter_l1, dh.max_iter_l1);
            emit_usize(parts, "--hdqsm-max-iter-l2", h.max_iter_l2, dh.max_iter_l2);
            emit_f64(parts, "--hdqsm-tol-update", h.tol_update, dh.tol_update);
        }
        QsmAlgorithm::AmpPe => {
            let a = &config.inversion.amp_pe;
            let da = &d.inversion.amp_pe;
            // b0 (scan) and gyro_ratio (physical constant) are not run flags.
            emit_usize(parts, "--amp-pe-wave-order", a.wave_order, da.wave_order);
            emit_usize(parts, "--amp-pe-nlevel", a.nlevel, da.nlevel);
            emit_f64(parts, "--amp-pe-wave-pec", a.wave_pec, da.wave_pec);
            emit_f64(parts, "--amp-pe-simulated-te", a.simulated_te, da.simulated_te);
            emit_usize(parts, "--amp-pe-max-linearization-ite", a.max_linearization_ite, da.max_linearization_ite);
            emit_f64(parts, "--amp-pe-damp-rate-sig", a.damp_rate_sig, da.damp_rate_sig);
            emit_f64(parts, "--amp-pe-damp-rate-par", a.damp_rate_par, da.damp_rate_par);
            emit_usize(parts, "--amp-pe-max-pe-spar-ite", a.max_pe_spar_ite, da.max_pe_spar_ite);
            emit_usize(parts, "--amp-pe-max-pe-est-ite", a.max_pe_est_ite, da.max_pe_est_ite);
            emit_f64(parts, "--amp-pe-cvg-thd", a.cvg_thd, da.cvg_thd);
            emit_f64(parts, "--amp-pe-tikhonov-beta", a.tikhonov_beta, da.tikhonov_beta);
        }
        // Deep-learning inversions have no per-net params (weights + fixed graph), but the
        // tileable nets (routed through run_dipole_inversion) accept overlap-tiling options.
        // Presence of `--tile-size` enables tiling; absence = whole-volume.
        QsmAlgorithm::Xqsm | QsmAlgorithm::Qsmnet | QsmAlgorithm::QsmnetPlus
        | QsmAlgorithm::Ir2qsm | QsmAlgorithm::Lpcnn | QsmAlgorithm::ModlQsm
        | QsmAlgorithm::Nextqsm => {
            if let Some(sz) = config.inversion.tile_size {
                parts.push(format!("--tile-size {}", sz));
            }
            if let Some(h) = config.inversion.tile_halo {
                parts.push(format!("--tile-halo {}", h));
            }
        }
        // Natively patch-based (autoqsm/qsmgan) and phase-input (iqsm/iqsm+) nets: no tiling opts.
        QsmAlgorithm::Autoqsm | QsmAlgorithm::Qsmgan
        | QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => {}
    }
}

fn emit_f64(parts: &mut Vec<String>, flag: &str, val: f64, default: f64) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
fn emit_usize(parts: &mut Vec<String>, flag: &str, val: usize, default: usize) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
fn emit_i32(parts: &mut Vec<String>, flag: &str, val: i32, default: i32) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
fn emit_enum<T: PartialEq + std::fmt::Display>(parts: &mut Vec<String>, flag: &str, val: &T, default: &T) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
fn emit_f64_arr3(parts: &mut Vec<String>, flag: &str, val: &[f64; 3], default: &[f64; 3]) {
    if val != default { parts.push(format!("{} {} {} {}", flag, val[0], val[1], val[2])); }
}

#[cfg(test)]
mod tests {
    /// A mask supplied for the run (qsmbly writes it to derivatives/qsmbly/, `crate::supplied`)
    /// is read back with `--use-custom-masks qsmbly`.
    #[test]
    fn a_supplied_mask_is_read_from_its_tools_derivatives() {
        let mut config = crate::config::PipelineConfig::default();
        assert!(!super::generate_command(&config).contains("--use-custom-masks"));
        config.masking.custom_mask_tool = Some("qsmbly".into());
        assert!(super::generate_command(&config).contains("--use-custom-masks qsmbly"));
        config.masking.custom_mask_tool = Some("*".into());
        let command = super::generate_command(&config);
        assert!(command.contains("--use-custom-masks") && !command.contains("--use-custom-masks *"), "{command}");
    }

    /// A relaxometry map the selected χ-separation method does not need is still asked for on
    /// the command line: it used to be dropped whenever χ-separation was on.
    #[test]
    fn relaxometry_flags_are_dropped_only_when_implied() {
        use crate::enums::SeparationAlgorithm;
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::R2starQsm;
        c.pipeline.do_r2map = true;
        enforce_separation_dependencies(&mut c);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-r2map"), "R2 is not implied by r2star-qsm: {cmd}");
        assert!(!cmd.contains("--do-r2starmap"), "R2* is: {cmd}");
        // R2'-based separation implies R2' and with it R2* and R2: none spelled out.
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        enforce_separation_dependencies(&mut c);
        let cmd = generate_command(&c);
        for f in ["--do-r2primemap", "--do-r2map", "--do-r2starmap"] {
            assert!(!cmd.contains(f), "{f} is implied: {cmd}");
        }
    }

    use super::*;

    #[test]
    fn test_default_config_minimal_command() {
        let config = PipelineConfig::default();
        let cmd = generate_command(&config);
        assert_eq!(cmd, "qsmxt run <bids_dir>");
    }

    /// A preclinical run has to be reproducible from the command the TUI prints: the scaling
    /// shows up both as the recipe-wide flag and in the BET step's own spec.
    #[test]
    fn test_bet_voxel_scale_command() {
        let mut config = PipelineConfig::default();
        config.bet.voxel_scale = 10.0;
        config.masking.sections = vec![crate::masking::MaskSection {
            input: crate::masking::MaskingInput::Magnitude,
            generator: crate::masking::MaskOp::Bet { fractional_intensity: 0.5, voxel_scale: 10.0 },
            refinements: vec![],
        }];
        let cmd = generate_command(&config);
        assert!(cmd.contains("--bet-voxel-scale 10"), "cmd: {cmd}");
        assert!(cmd.contains("--mask magnitude,bet:0.50:scale=10"), "cmd: {cmd}");

        // The default is silent — the clinical command line is unchanged.
        let cmd = generate_command(&PipelineConfig::default());
        assert!(!cmd.contains("--bet-voxel-scale"), "cmd: {cmd}");
        assert!(!cmd.contains("scale="), "cmd: {cmd}");
    }

    #[test]
    fn test_changed_algorithm() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Tv;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--qsm-algorithm tv"));
    }

    #[test]
    fn test_dl_algorithms_command() {
        use crate::enums::{BfAlgorithm, QsmAlgorithm, SeparationAlgorithm};
        for a in [
            QsmAlgorithm::Xqsm, QsmAlgorithm::Qsmnet, QsmAlgorithm::QsmnetPlus,
            QsmAlgorithm::Autoqsm, QsmAlgorithm::Qsmgan, QsmAlgorithm::Ir2qsm,
            QsmAlgorithm::Lpcnn, QsmAlgorithm::ModlQsm, QsmAlgorithm::Nextqsm,
            QsmAlgorithm::Iqsm, QsmAlgorithm::IqsmPlus,
        ] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = a;
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--qsm-algorithm {}", a)), "missing {} in: {}", a, cmd);
        }
        // iQFM background removal + χ-sepnet separation (deep-learning; patches in the profile tests).
        let mut c = PipelineConfig::default();
        c.bg_removal.algorithm = BfAlgorithm::Iqfm;
        assert!(generate_command(&c).contains("--bf-algorithm iqfm"));
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        assert!(generate_command(&c).contains("--chisep chi-sepnet"));
    }

    #[test]
    fn test_dl_tiling_command() {
        use crate::enums::QsmAlgorithm;
        // Off by default: no tiling flags for a DL algorithm.
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--tile-size"), "got: {}", cmd);
        assert!(!cmd.contains("--tile-halo"), "got: {}", cmd);
        // Setting tile_size enables tiling; halo emitted only when set.
        c.inversion.tile_size = Some(64);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tile-size 64"), "got: {}", cmd);
        assert!(!cmd.contains("--tile-halo"), "got: {}", cmd);
        c.inversion.tile_halo = Some(4);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tile-size 64") && cmd.contains("--tile-halo 4"), "got: {}", cmd);
        // Native-patch nets ignore tiling options (no flags emitted).
        c.inversion.algorithm = QsmAlgorithm::Autoqsm;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--tile-size"), "autoqsm should not emit tiling: {}", cmd);
    }

    #[test]
    fn test_msmv_refine_command() {
        // Off by default → no mSMV flags.
        let mut config = PipelineConfig::default();
        assert!(!generate_command(&config).contains("--msmv"));
        // Enabled → the refine flag is emitted; a non-default param is emitted too.
        config.bg_removal.msmv_refine = true;
        config.bg_removal.msmv.maxk = 9;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--msmv-refine"), "got: {}", cmd);
        assert!(cmd.contains("--msmv-maxk 9"), "got: {}", cmd);
        // A default param is not emitted (only the refine flag + changed knobs).
        assert!(!cmd.contains("--msmv-radius"), "got: {}", cmd);
    }

    #[test]
    fn test_chi_separation_command() {
        // Default method (r2star-qsm) uses R2* directly — it forces R2* only, NOT R2/R2'.
        let mut config = PipelineConfig::default();
        config.pipeline.do_chi_separation = true;
        enforce_separation_dependencies(&mut config);
        assert!(config.pipeline.do_r2starmap);
        assert!(!config.pipeline.do_r2map && !config.pipeline.do_r2primemap);
        // Those maps are implied by --do-chisep and must not appear in the command.
        let cmd = generate_command(&config);
        assert!(cmd.contains("--do-chisep"));
        assert!(!cmd.contains("--do-r2starmap"), "r2starmap implied by chisep, got: {}", cmd);
        assert!(!cmd.contains("--do-r2map"));
        assert!(!cmd.contains("--do-r2primemap"));
        assert!(!cmd.contains("--chisep")); // default method → not emitted

        // An R2'-based method (wavesep) forces R2' → R2 → R2*, none of which should leak.
        let mut wc = PipelineConfig::default();
        wc.pipeline.do_chi_separation = true;
        wc.separation.algorithm = SeparationAlgorithm::WaveSep;
        enforce_separation_dependencies(&mut wc);
        assert!(wc.pipeline.do_r2primemap && wc.pipeline.do_r2map && wc.pipeline.do_r2starmap);
        let cmd = generate_command(&wc);
        assert!(cmd.contains("--chisep wavesep"));
        assert!(!cmd.contains("--do-r2map") && !cmd.contains("--do-r2primemap") && !cmd.contains("--do-r2starmap"));

        // Selecting a non-default method emits it; editing a param emits that too.
        config.separation.algorithm = SeparationAlgorithm::Decompose;
        config.separation.decompose.max_lm_iter = 99;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--chisep decompose"));
        assert!(cmd.contains("--decompose-max-lm-iter 99"));

        // A param for a non-selected method must NOT leak into the command.
        assert!(!cmd.contains("--wavesep-alpha"));
    }

    #[test]
    fn test_changed_rts_param() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Rts;
        config.inversion.rts.delta = 0.2;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--rts-delta 0.2"));
    }

    #[test]
    fn test_phase_offset_disabled() {
        let mut config = PipelineConfig::default();
        config.field_mapping.phase_offset_removal = false;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--phase-offset-removal false"));
    }

    #[test]
    fn test_bipolar_enabled() {
        let mut config = PipelineConfig::default();
        config.field_mapping.bipolar_correction = true;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--bipolar-correction"));
    }

    #[test]
    fn test_romeo_template_mode() {
        let mut config = PipelineConfig::default();
        config.field_mapping.romeo.individual = false;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--no-romeo-individual"));
    }

    #[test]
    fn test_swi_params_only_when_enabled() {
        let mut config = PipelineConfig::default();
        config.swi.strength = 8.0;
        // SWI off — no SWI flags
        let cmd = generate_command(&config);
        assert!(!cmd.contains("--swi-strength"));
        // SWI on — flag appears
        config.pipeline.do_swi = true;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--swi-strength 8"));
        assert!(!cmd.contains("--swi-unwrapping"), "{cmd}");
        config.swi.unwrap = crate::SwiUnwrap::Romeo;
        assert!(generate_command(&config).contains("--swi-unwrapping romeo"));
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = PipelineConfig::default();
        let toml = config.to_toml().unwrap();
        let parsed = PipelineConfig::from_toml(&toml).unwrap();
        assert_eq!(parsed.inversion.algorithm, QsmAlgorithm::Ilsqr);
        assert_eq!(parsed.field_mapping.unwrapping_algorithm, UnwrappingAlgorithm::Romeo);
        assert!(parsed.field_mapping.phase_offset_removal);
        assert!(parsed.field_mapping.romeo.individual);
    }

    #[test]
    fn test_all_inversion_algorithms() {
        for (alg, name) in [
            (QsmAlgorithm::Tv, "tv"), (QsmAlgorithm::Tkd, "tkd"),
            (QsmAlgorithm::Tsvd, "tsvd"), (QsmAlgorithm::Tgv, "tgv"),
            (QsmAlgorithm::Tikhonov, "tikhonov"), (QsmAlgorithm::Nltv, "nltv"),
            (QsmAlgorithm::Medi, "medi"), (QsmAlgorithm::Tfi, "tfi"),
            (QsmAlgorithm::Rts, "rts"),
            (QsmAlgorithm::Qsmart, "qsmart"),
            (QsmAlgorithm::Ndi, "ndi"), (QsmAlgorithm::Fansi, "fansi"),
            (QsmAlgorithm::FansiTgv, "fansi-tgv"), (QsmAlgorithm::L1qsm, "l1qsm"),
            (QsmAlgorithm::Whqsm, "whqsm"), (QsmAlgorithm::Hdqsm, "hdqsm"),
            (QsmAlgorithm::AmpPe, "amp-pe"),
        ] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--qsm-algorithm {}", name)), "missing algorithm flag for {}", name);
        }
    }

    #[test]
    fn test_all_bf_algorithms() {
        for (alg, name) in [
            (BfAlgorithm::Pdf, "pdf"), (BfAlgorithm::Lbv, "lbv"),
            (BfAlgorithm::Ismv, "ismv"), (BfAlgorithm::Sharp, "sharp"),
            (BfAlgorithm::Resharp, "resharp"), (BfAlgorithm::Harperella, "harperella"),
            (BfAlgorithm::Iharperella, "iharperella"),
        ] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--bf-algorithm {}", name)), "missing bf flag for {}", name);
        }
    }

    #[test]
    fn test_tv_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Tv;
        c.inversion.tv.lambda = 0.001;
        c.inversion.tv.max_iter = 100;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tv-lambda 0.001"));
        assert!(cmd.contains("--tv-max-iter 100"));
    }

    #[test]
    fn test_medi_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Medi;
        c.inversion.medi.lambda = 999.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--medi-lambda 999"));
    }

    #[test]
    fn test_tgv_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Tgv;
        c.inversion.tgv.iterations = 500;
        c.inversion.tgv.erosions = 2;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tgv-iterations 500"));
        assert!(cmd.contains("--tgv-erosions 2"));
    }

    #[test]
    fn test_qsmart_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        c.inversion.qsmart.vasc_sphere_radius = 10;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsmart-vasc-sphere-radius 10"));
    }

    #[test]
    fn test_qsmart_all_params_emitted() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        c.inversion.qsmart.inversion = QsmAlgorithm::Tv;
        c.inversion.qsmart.sdf_sigma1_stage1 = 11.0;
        c.inversion.qsmart.sdf_sigma2_stage1 = 12.0;
        c.inversion.qsmart.sdf_sigma1_stage2 = 13.0;
        c.inversion.qsmart.sdf_sigma2_stage2 = 14.0;
        c.inversion.qsmart.sdf_lower_lim = 0.5;
        c.inversion.qsmart.sdf_curv_constant = 600.0;
        c.inversion.qsmart.frangi_scale_min = 1.5;
        c.inversion.qsmart.frangi_scale_max = 7.0;
        c.inversion.qsmart.frangi_scale_ratio = 1.0;
        c.inversion.qsmart.frangi_c = 400.0;
        let cmd = generate_command(&c);
        for flag in [
            "--qsmart-inversion tv",
            "--qsmart-sdf-sigma1-stage1 11",
            "--qsmart-sdf-sigma2-stage1 12",
            "--qsmart-sdf-sigma1-stage2 13",
            "--qsmart-sdf-sigma2-stage2 14",
            "--qsmart-sdf-lower-lim 0.5",
            "--qsmart-sdf-curv-constant 600",
            "--qsmart-frangi-scale-min 1.5",
            "--qsmart-frangi-scale-max 7",
            "--qsmart-frangi-scale-ratio 1",
            "--qsmart-frangi-c 400",
        ] {
            assert!(cmd.contains(flag), "missing `{}` in: {}", flag, cmd);
        }
    }

    /// Mirrors the WASM command path: the TOML that qsmbly's ConfigBridge emits must
    /// parse (matching serde key names) and produce the QSMART flags.
    #[test]
    fn test_qsmart_toml_from_configbridge_generates_flags() {
        let toml = r#"
[inversion]
algorithm = "qsmart"

[inversion.qsmart]
inversion = "rts"
ilsqr_max_iter = 50
ilsqr_pad_mm = 8.0
vasc_sphere_radius = 8
sdf_spatial_radius = 8
sdf_sigma1_stage1 = 11.0
sdf_sigma2_stage1 = 12.0
sdf_sigma1_stage2 = 13.0
sdf_sigma2_stage2 = 14.0
sdf_lower_lim = 0.45
sdf_curv_constant = 600.0
frangi_scale_min = 1.5
frangi_scale_max = 7.0
frangi_scale_ratio = 1.0
frangi_c = 400.0
"#;
        let config = PipelineConfig::from_toml(toml).expect("ConfigBridge TOML must parse");
        let cmd = generate_command(&config);
        for flag in [
            "--qsm-algorithm qsmart", "--qsmart-inversion rts",
            "--qsmart-sdf-sigma1-stage1 11", "--qsmart-sdf-lower-lim 0.45",
            "--qsmart-frangi-scale-min 1.5", "--qsmart-frangi-c 400",
        ] {
            assert!(cmd.contains(flag), "missing `{}` in: {}", flag, cmd);
        }
        // The inner iLSQR's settings do not apply under an RTS inner inversion.
        assert!(!cmd.contains("--qsmart-ilsqr"), "{cmd}");
        let config = PipelineConfig::from_toml(&toml.replace(r#"inversion = "rts""#, r#"inversion = "ilsqr""#)).unwrap();
        assert!(generate_command(&config).contains("--qsmart-ilsqr-pad-mm 8"));
    }

    #[test]
    fn test_r2primenet_settings_round_trip() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--r2primenet"));
        c.separation.r2primenet.ignore_field_mismatch = true;
        c.separation.r2primenet.patch = [128, 128, 64];
        // Not said while R2' is not computed: R2PRIMEnet does not run.
        assert!(!generate_command(&c).contains("--r2primenet"));
        c.pipeline.do_r2primemap = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--r2primenet-ignore-field-mismatch"), "{cmd}");
        assert!(cmd.contains("--r2primenet-patch 128 128 64"), "{cmd}");
        // Meaningless when R2' is only ever measured.
        c.separation.r2prime_strategy = R2PrimeStrategy::Mese;
        assert!(!generate_command(&c).contains("--r2primenet"));
    }

    #[test]
    fn test_vsharp_grid_flags() {
        let mut c = PipelineConfig::default();
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--vsharp-grid"), "native is the default: {cmd}");
        c.bg_removal.vsharp.grid_spacing = 0.8;
        assert!(!generate_command(&c).contains("--vsharp-grid-spacing"), "unused unless isotropic");
        c.bg_removal.vsharp.grid = VsharpGrid::Isotropic;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--vsharp-grid isotropic") && cmd.contains("--vsharp-grid-spacing 0.8"), "{cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Finest;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--vsharp-grid finest") && !cmd.contains("--vsharp-grid-spacing"), "{cmd}");
    }

    #[test]
    fn test_vsharp_deconvolve_flag() {
        let mut c = PipelineConfig::default();
        assert!(!c.bg_removal.vsharp.deconvolve, "QSMxT's default is off");
        c.bg_removal.vsharp.threshold = 0.1;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--vsharp-deconvolve"), "{cmd}");
        assert!(!cmd.contains("--vsharp-threshold"), "unused without deconvolution: {cmd}");
        c.bg_removal.vsharp.deconvolve = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--vsharp-deconvolve true"), "{cmd}");
        assert!(cmd.contains("--vsharp-threshold 0.1"), "{cmd}");
    }

    #[test]
    fn test_robust_fit_flags_round_trip() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::RobustFit;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-estimation robust-fit"), "{cmd}");
        assert!(!cmd.contains("--robust-fit-"), "defaults are not emitted: {cmd}");
        c.field_mapping.robust_fit.loss = RobustLoss::Huber;
        c.field_mapping.robust_fit.tuning = Some(2.5);
        c.field_mapping.robust_fit.iterations = 5;
        c.field_mapping.robust_fit.downweight_level = 0.5;
        c.field_mapping.robust_fit.flag_ratio = 4.0;
        c.field_mapping.linear_fit.estimate_offset = false;
        c.field_mapping.linear_fit.reliability_threshold_percentile = 80.0;
        let cmd = generate_command(&c);
        for f in ["--robust-fit-loss huber", "--robust-fit-tuning 2.5", "--robust-fit-iterations 5",
                  "--robust-fit-downweight-level 0.5", "--robust-fit-flag-ratio 4",
                  "--linear-fit-estimate-offset false", "--linear-fit-reliability-threshold 80"] {
            assert!(cmd.contains(f), "missing {f}: {cmd}");
        }
        // The robust settings mean nothing to the weighted average.
        c.field_mapping.b0_estimation = B0Estimation::WeightedAvg;
        assert!(!generate_command(&c).contains("--robust-fit-"));
    }

    #[test]
    fn test_motion_correction_flags() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("motion"));
        c.motion_correction.enabled = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--motion-correction"), "{cmd}");
        assert!(!cmd.contains("--motion-reference") && !cmd.contains("--registration-"), "defaults: {cmd}");
        c.motion_correction.reference = crate::motion::MotionReference::Echo(2);
        c.motion_correction.chain = false;
        c.motion_correction.min_ncc = 0.9;
        c.registration.levels = 4;
        c.registration.min_overlap = 0.5;
        let cmd = generate_command(&c);
        for f in ["--motion-reference 2", "--no-motion-chain", "--motion-min-ncc 0.9",
                  "--registration-levels 4", "--registration-min-overlap 0.5"] {
            assert!(cmd.contains(f), "missing {f}: {cmd}");
        }
    }

    #[test]
    fn test_b0_estimation_linear_fit() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::LinearFit;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-estimation linear-fit"));
    }

    #[test]
    fn test_b0_weight_type() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_weight_type = B0WeightType::Average;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-weight-type average"));
    }

    #[test]
    fn test_laplacian_solver() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--laplacian-solver"));
        c.field_mapping.laplacian_solver = LaplacianSolver::Fft;
        // Only Laplacian unwrapping has a solver.
        assert!(!generate_command(&c).contains("--laplacian-solver"));
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--laplacian-solver fft"), "{cmd}");
        assert!(!cmd.contains("--laplacian-fft-pad"), "default pad is not emitted: {cmd}");
        c.field_mapping.laplacian_fft_pad = [12; 3];
        assert!(generate_command(&c).contains("--laplacian-fft-pad 12"));
        c.field_mapping.laplacian_fft_pad = [64, 64, 32];
        assert!(generate_command(&c).contains("--laplacian-fft-pad 64 64 32"));
    }

    #[test]
    fn test_laplacian_fft_pad_in_config_files() {
        let c: FieldMappingConfig = toml::from_str("laplacian_fft_pad = 12").unwrap();
        assert_eq!(c.laplacian_fft_pad, [12; 3]);
        let c: FieldMappingConfig = toml::from_str("laplacian_fft_pad = [64, 64, 32]").unwrap();
        assert_eq!(c.laplacian_fft_pad, [64, 64, 32]);
        assert!(toml::from_str::<FieldMappingConfig>("laplacian_fft_pad = [1, 2]").is_err());
        // What a config writes, it reads back.
        let mut d = PipelineConfig::default();
        d.field_mapping.laplacian_fft_pad = [64, 64, 32];
        let back = PipelineConfig::from_toml(&toml::to_string(&d).unwrap()).unwrap();
        assert_eq!(back.field_mapping.laplacian_fft_pad, [64, 64, 32]);
    }

    #[test]
    fn test_laplacian_solver_old_config_names() {
        // `laplacian_kernel = "sti"` was the (unreleased) earlier spelling.
        let c: FieldMappingConfig = toml::from_str("laplacian_kernel = \"sti\"").unwrap();
        assert_eq!(c.laplacian_solver, LaplacianSolver::Fft);
        assert_eq!(c.laplacian_fft_pad, [64; 3]);
    }

    #[test]
    fn test_b0_weight_type_assumed_decay() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_weight_type = B0WeightType::AssumedDecay;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-weight-type assumed-decay"), "{cmd}");
        assert!(!cmd.contains("--b0-weight-assumed-t2star"), "default T2* is not emitted: {cmd}");
        c.field_mapping.b0_weight_assumed_t2star_ms = 35.0;
        assert!(generate_command(&c).contains("--b0-weight-assumed-t2star 35"));
        // only meaningful with assumed-decay weighting
        c.field_mapping.b0_weight_type = B0WeightType::TEs;
        assert!(!generate_command(&c).contains("--b0-weight-assumed-t2star"));
        // The config spelling round-trips, and the never-released "t2star" is not accepted.
        assert_eq!(serde_json::from_str::<B0WeightType>("\"assumed-decay\"").unwrap(), B0WeightType::AssumedDecay);
        assert_eq!(B0WeightType::AssumedDecay.to_string(), "assumed-decay");
        assert!(serde_json::from_str::<B0WeightType>("\"t2star\"").is_err());
    }

    #[test]
    fn test_laplacian_unwrapping() {
        let mut c = PipelineConfig::default();
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--unwrapping-algorithm laplacian"));
    }

    #[test]
    fn test_romeo_correct_global_disabled() {
        let mut c = PipelineConfig::default();
        c.field_mapping.romeo.correct_global = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-romeo-correct-global"));
    }

    #[test]
    fn test_romeo_weight_flags() {
        let mut c = PipelineConfig::default();
        c.field_mapping.romeo.phase_gradient_coherence = false;
        c.field_mapping.romeo.mag_coherence = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-romeo-phase-gradient-coherence"));
        assert!(cmd.contains("--no-romeo-mag-coherence"));
    }

    #[test]
    fn test_mask_non_default() {
        let mut c = PipelineConfig::default();
        c.masking.sections[0].refinements = vec![
            crate::masking::MaskOp::Erode { iterations: 3 },
        ];
        let cmd = generate_command(&c);
        assert!(cmd.contains("--mask"));
        assert!(cmd.contains("erode:3"));
    }

    /// Off by default, so the bare flag says everything when the reliable mask is the default one.
    /// HEIDI's seed is an LSQR solve, so `--qsm-algorithm heidi` has to emit both groups; plain
    /// LSQR must not emit HEIDI's.
    #[test]
    fn segmentation_and_analysis_emit_their_flags() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--do-segmentation"));

        c.pipeline.do_segmentation = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-segmentation"), "{cmd}");
        assert!(!cmd.contains("--synthseg-version"), "the default version stays off: {cmd}");

        // --do-analysis implies segmentation, so emitting both would be noise.
        c.pipeline.do_analysis = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-analysis"), "{cmd}");
        assert!(!cmd.contains("--do-segmentation"), "implied, so not repeated: {cmd}");

        c.segmentation.version = crate::enums::SynthSegVersion::V2;
        c.segmentation.flip_averaging = false;
        c.segmentation.crop = Some(192);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--synthseg-version v2"), "{cmd}");
        assert!(cmd.contains("--no-synthseg-flip-averaging"), "{cmd}");
        assert!(cmd.contains("--synthseg-crop 192"), "{cmd}");
    }

    #[test]
    fn smwi_emits_its_flag_and_non_default_params() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--do-smwi"));

        c.pipeline.do_smwi = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-smwi"), "{cmd}");
        assert!(!cmd.contains("--smwi-threshold"), "defaults stay off: {cmd}");

        c.smwi.threshold_ppm = 0.2;
        c.smwi.mip_window = 7;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--smwi-threshold 0.2"), "{cmd}");
        assert!(cmd.contains("--smwi-mip-window 7"), "{cmd}");
        assert!(!cmd.contains("--smwi-power"), "an untouched parameter stays off: {cmd}");
    }

    #[test]
    fn heidi_emits_the_lsqr_group_too() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Lsqr;
        c.inversion.lsqr.tol = 1e-7;
        c.inversion.heidi.cone_threshold = 0.2;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-algorithm lsqr"), "{cmd}");
        assert!(cmd.contains("--lsqr-tol 0.0000001"), "{cmd}");
        assert!(!cmd.contains("--heidi-"), "plain LSQR should not emit HEIDI params: {cmd}");

        c.inversion.algorithm = QsmAlgorithm::Heidi;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-algorithm heidi"), "{cmd}");
        assert!(cmd.contains("--lsqr-tol 0.0000001"), "HEIDI's seed is LSQR: {cmd}");
        assert!(cmd.contains("--heidi-cone-threshold 0.2"), "{cmd}");
    }

    /// Defaults stay off the command line, and the two `no-` flags only appear when the switch is
    /// actually turned off.
    #[test]
    fn lsqr_and_heidi_defaults_are_silent() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Heidi;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--lsqr-tol"), "{cmd}");
        assert!(!cmd.contains("--no-lsqr-global-offset"), "{cmd}");
        assert!(!cmd.contains("--no-heidi-denoise"), "{cmd}");
        assert!(!cmd.contains("--heidi-denoise-iterations"), "defaults stay off: {cmd}");

        c.inversion.lsqr.fit_global_offset = false;
        c.inversion.heidi.denoise = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-lsqr-global-offset"), "{cmd}");
        assert!(cmd.contains("--no-heidi-denoise"), "{cmd}");
        // With denoising off, its three parameters are noise on the command line.
        assert!(!cmd.contains("--heidi-denoise-iterations"), "{cmd}");
    }

    #[test]
    fn two_pass_emits_a_bare_flag() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--two-pass"));

        c.masking.two_pass = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--two-pass"), "{cmd}");
        assert!(!cmd.contains("--two-pass-mask"), "default recipe should not be spelled out: {cmd}");

        // Explicitly setting the default recipe means the same thing as leaving it unset.
        c.masking.two_pass_sections = Some(crate::masking::default_two_pass_sections());
        assert!(!generate_command(&c).contains("--two-pass-mask"));
    }

    #[test]
    fn two_pass_spells_out_an_edited_reliable_mask() {
        let mut c = PipelineConfig::default();
        c.masking.two_pass = true;
        c.masking.two_pass_sections = Some(vec![crate::masking::MaskSection {
            input: crate::masking::MaskingInput::Magnitude,
            generator: crate::masking::MaskOp::Threshold {
                method: crate::masking::MaskThresholdMethod::Percentile,
                value: Some(30.0),
            },
            refinements: vec![],
        }]);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--two-pass"), "{cmd}");
        assert!(cmd.contains("--two-pass-mask magnitude,threshold:percentile:30.0"), "{cmd}");
    }

    #[test]
    fn test_pipeline_toggles() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_swi = true;
        c.pipeline.do_t2starmap = true;
        c.pipeline.do_r2starmap = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-qsm"));
        assert!(cmd.contains("--do-swi"));
        assert!(cmd.contains("--do-t2starmap"));
        assert!(cmd.contains("--do-r2starmap"));
    }

    #[test]
    fn test_inhomogeneity_disabled() {
        let mut c = PipelineConfig::default();
        c.masking.inhomogeneity_correction = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-inhomogeneity-correction"));
    }

    #[test]
    fn test_qsm_reference_none() {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::None;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-reference none"));

        // A region reference travels as the spec itself, not as the word "region".
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("left-thalamus,right-thalamus".to_string());
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-reference left-thalamus,right-thalamus"), "{cmd}");
        assert!(!cmd.contains("--qsm-reference region"), "{cmd}");
    }

    #[test]
    fn test_json_roundtrip() {
        let config = PipelineConfig::default();
        let json = config.to_json().unwrap();
        let parsed: PipelineConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.inversion.algorithm, QsmAlgorithm::Ilsqr);
    }

    #[test]
    fn test_partial_toml() {
        // Only specify one field — rest should use defaults
        let toml = r#"
[inversion]
algorithm = "tv"
"#;
        let config = PipelineConfig::from_toml(toml).unwrap();
        assert_eq!(config.inversion.algorithm, QsmAlgorithm::Tv);
        // Everything else should be default
        assert!(config.pipeline.do_qsm);
        assert_eq!(config.bg_removal.algorithm, BfAlgorithm::Vsharp);
        assert!(config.field_mapping.phase_offset_removal);
    }

    #[test]
    fn test_empty_toml() {
        let config = PipelineConfig::from_toml("").unwrap();
        assert_eq!(config.inversion.algorithm, QsmAlgorithm::Ilsqr);
        assert!(config.pipeline.do_qsm);
    }

    #[test]
    fn test_output_space() {
        let mut c = PipelineConfig::default();
        assert_eq!(c.pipeline.output_space, OutputSpace::Acquired, "acquired is the default");
        assert!(!generate_command(&c).contains("--output-space"));
        c.pipeline.output_space = OutputSpace::Working;
        // Only meaningful when the data is resampled.
        assert!(!generate_command(&c).contains("--output-space"));
        c.pipeline.axial_resampling = true;
        assert!(generate_command(&c).contains("--output-space working"));
    }

    #[test]
    fn test_crop_options() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--crop"));
        c.pipeline.crop_to_mask = true;
        c.pipeline.crop_margin_mm = 24.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--crop-to-mask"), "{cmd}");
        assert!(cmd.contains("--crop-margin-mm 24"), "{cmd}");
    }

    #[test]
    fn test_coil_combination_sigma() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--coil-combination-sigma"));
        c.field_mapping.coil_combination_sigma = [4.0, 4.0, 4.0];
        assert!(generate_command(&c).contains("--coil-combination-sigma 4 4 4"));
    }

    #[test]
    fn test_phase_offset_sigma() {
        let mut c = PipelineConfig::default();
        c.field_mapping.phase_offset_sigma = [10.0, 10.0, 5.0];
        let cmd = generate_command(&c);
        assert!(cmd.contains("--phase-offset-sigma 10 10 5"));
    }
}

/// Quote a pattern for a shell when it contains glob metacharacters.
///
/// An orientation-group pattern is almost always a glob (`*acq-dir*`), and an unquoted glob
/// in the displayed command is one the user's shell would expand against their working
/// directory before qsmxt ever saw it.
fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod multiorient_command_tests {
    use super::*;

    #[test]
    fn nothing_is_emitted_when_grouping_is_off() {
        let cmd = generate_command(&PipelineConfig::default());
        assert!(!cmd.contains("--orientation-group"), "{cmd}");
        assert!(!cmd.contains("--multi-orientation"), "{cmd}");
    }

    #[test]
    fn a_glob_pattern_is_quoted_so_the_shell_leaves_it_alone() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "*acq-dir*".into();
        let cmd = generate_command(&config);
        assert!(cmd.contains("--orientation-group '*acq-dir*'"), "{cmd}");
        assert!(cmd.contains("--multi-orientation-algorithm cosmos"), "{cmd}");
        // Lambda is at its default, so it should not add noise.
        assert!(!cmd.contains("--multi-orientation-lambda"), "{cmd}");
    }

    #[test]
    fn a_bare_entity_needs_no_quoting() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        config.multi_orientation.algorithm = MultiOrientAlgorithm::Sti;
        config.multi_orientation.lambda = 1e-3;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--orientation-group acq"), "{cmd}");
        assert!(cmd.contains("--multi-orientation-algorithm sti"), "{cmd}");
        assert!(cmd.contains("--multi-orientation-lambda 0.001"), "{cmd}");
    }
}
