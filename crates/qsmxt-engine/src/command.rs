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
    emit_enum(&mut parts, "--inhomogeneity-method", &config.masking.inhomogeneity_method, &d.masking.inhomogeneity_method);
    {
        let (n, dn) = (&config.n4, &d.n4);
        emit_usize(&mut parts, "--n4-shrink-factor", n.shrink_factor, dn.shrink_factor);
        if n.max_iterations != dn.max_iterations {
            let levels: Vec<String> = n.max_iterations.iter().map(|i| i.to_string()).collect();
            parts.push(format!("--n4-max-iterations {}", levels.join(" ")));
        }
        emit_f64(&mut parts, "--n4-convergence-threshold", n.convergence_threshold, dn.convergence_threshold);
        if n.mesh != dn.mesh {
            parts.push(format!("--n4-mesh {} {} {}", n.mesh[0], n.mesh[1], n.mesh[2]));
        }
        emit_usize(&mut parts, "--n4-spline-order", n.spline_order, dn.spline_order);
        emit_f64(&mut parts, "--n4-bias-field-fwhm", n.bias_field_fwhm, dn.bias_field_fwhm);
        emit_f64(&mut parts, "--n4-wiener-noise", n.wiener_noise, dn.wiener_noise);
        emit_usize(&mut parts, "--n4-histogram-bins", n.histogram_bins, dn.histogram_bins);
    }

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
    emit_f64_arr3(&mut parts, "--phase-offset-sigma-mm", &config.field_mapping.phase_offset_sigma_mm, &d.field_mapping.phase_offset_sigma_mm);
    emit_f64_arr3(&mut parts, "--coil-combination-sigma", &config.field_mapping.coil_combination_sigma, &d.field_mapping.coil_combination_sigma);
    if config.field_mapping.bipolar_correction { parts.push("--bipolar-correction".into()); }
    let fm = &config.field_mapping;
    emit_enum(&mut parts, "--unwrapping-algorithm", &fm.unwrapping_algorithm, &d.field_mapping.unwrapping_algorithm);
    emit_enum(&mut parts, "--laplacian-solver", &fm.laplacian_solver, &d.field_mapping.laplacian_solver);
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
    if fm.laplacian_unmasked { parts.push("--laplacian-unmasked".into()); }
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
    if config.field_mapping.b0_estimation == B0Estimation::QsmartEchofit {
        let (f, fd) = (&config.field_mapping, &d.field_mapping);
        emit_enum(&mut parts, "--qsmart-fit-threshold-mode", &f.qsmart_fit_threshold_mode, &fd.qsmart_fit_threshold_mode);
        match f.qsmart_fit_threshold_mode {
            QsmartFitThresholdMode::Percentile => emit_f64(&mut parts, "--qsmart-fit-percentile", f.qsmart_fit_percentile, fd.qsmart_fit_percentile),
            QsmartFitThresholdMode::Absolute => emit_f64(&mut parts, "--qsmart-fit-threshold", f.qsmart_fit_threshold, fd.qsmart_fit_threshold),
        }
    }
    emit_enum(&mut parts, "--b0-weight-type", &config.field_mapping.b0_weight_type, &d.field_mapping.b0_weight_type);
    if config.field_mapping.b0_weight_type == B0WeightType::AssumedDecay {
        emit_f64(&mut parts, "--b0-weight-assumed-t2star", config.field_mapping.b0_weight_assumed_t2star_ms, d.field_mapping.b0_weight_assumed_t2star_ms);
    }
    // The fits' shared settings, and the robust fit's own.
    if matches!(config.field_mapping.b0_estimation, B0Estimation::LinearFit | B0Estimation::RobustFit) {
        let (l, ld) = (&config.field_mapping.linear_fit, &d.field_mapping.linear_fit);
        if l.estimate_offset != ld.estimate_offset {
            parts.push(format!("--linear-fit-estimate-offset {}", l.estimate_offset));
        }
    }
    if config.field_mapping.b0_estimation == B0Estimation::RobustFit {
        let (r, rd) = (&config.field_mapping.robust_fit, &d.field_mapping.robust_fit);
        emit_enum(&mut parts, "--robust-fit-loss", &r.loss, &rd.loss);
        if let Some(t) = r.tuning { parts.push(format!("--robust-fit-tuning {}", t)); }
        emit_usize(&mut parts, "--robust-fit-iterations", r.iterations, rd.iterations);
        emit_f64(&mut parts, "--robust-fit-downweight-level", r.downweight_level, rd.downweight_level);
        emit_f64(&mut parts, "--robust-fit-flag-ratio", r.flag_ratio, rd.flag_ratio);
    }
    if config.field_mapping.b0_estimation == B0Estimation::ComplexFit {
        let (c, cd) = (&config.field_mapping.complex_fit, &d.field_mapping.complex_fit);
        emit_f64(&mut parts, "--complex-fit-reltol", c.reltol, cd.reltol);
        emit_usize(&mut parts, "--complex-fit-max-iter", c.max_iter, cd.max_iter);
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
        // The default recipe is a preset too; saying so adds noise rather than removing it.
        _ if masking_default => {}
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
        if let Some(combine) = config.masking.two_pass_combine {
            parts.push(format!("--two-pass-mask-combine {combine}"));
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
    if config.bg_removal.algorithm != d.bg_removal.algorithm {
        parts.push(format!("--bf-algorithm {}", config.bg_removal.algorithm));
    }
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
            emit_f64(&mut parts, "--vsharp-radius-step", config.bg_removal.vsharp.radius_step, d.bg_removal.vsharp.radius_step);
            emit_enum(&mut parts, "--vsharp-grid", &config.bg_removal.vsharp.grid, &d.bg_removal.vsharp.grid);
            if config.bg_removal.vsharp.grid == VsharpGrid::Isotropic {
                emit_f64(&mut parts, "--vsharp-grid-spacing", config.bg_removal.vsharp.grid_spacing, d.bg_removal.vsharp.grid_spacing);
            }
        }
        BfAlgorithm::Pdf => {
            let (p, dp) = (&config.bg_removal.pdf, &d.bg_removal.pdf);
            emit_f64(&mut parts, "--pdf-tol", p.tol, dp.tol);
            emit_usize(&mut parts, "--pdf-n-cg", p.n_cg, dp.n_cg);
            emit_enum(&mut parts, "--pdf-space", &p.space, &dp.space);
            emit_usize(&mut parts, "--pdf-n-pad", p.n_pad, dp.n_pad);
        }
        BfAlgorithm::Lbv => { emit_f64(&mut parts, "--lbv-tol", config.bg_removal.lbv.tol, d.bg_removal.lbv.tol); }
        BfAlgorithm::Ismv => {
            emit_enum(&mut parts, "--ismv-variant", &config.bg_removal.ismv.variant, &d.bg_removal.ismv.variant);
            emit_f64(&mut parts, "--ismv-tol", config.bg_removal.ismv.tol, d.bg_removal.ismv.tol);
            emit_opt_usize(&mut parts, "--ismv-max-iter", config.bg_removal.ismv.max_iter, d.bg_removal.ismv.max_iter);
            emit_opt_f64(&mut parts, "--ismv-radius", config.bg_removal.ismv.radius, d.bg_removal.ismv.radius);
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
            emit_usize_arr3(&mut parts, "--harperella-pad", &config.bg_removal.harperella.pad, &d.bg_removal.harperella.pad);
        }
        BfAlgorithm::Iharperella => {
            emit_usize(&mut parts, "--iharperella-max-iter", config.bg_removal.iharperella.max_iter, d.bg_removal.iharperella.max_iter);
            emit_f64(&mut parts, "--iharperella-tol", config.bg_removal.iharperella.tol, d.bg_removal.iharperella.tol);
            emit_usize_arr3(&mut parts, "--iharperella-pad", &config.bg_removal.iharperella.pad, &d.bg_removal.iharperella.pad);
        }
        // BFRnet / iQFM (deep learning) have no user-tunable parameters.
        BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => {}
    }

    // ── QSM inversion ──
    emit_enum(&mut parts, "--qsm-algorithm", &config.inversion.algorithm, &d.inversion.algorithm);
    emit_inversion_params(&mut parts, config, &d, config.inversion.algorithm);
    // Overlap tiling, for any network that takes it (the inversion, or BFRnet/iQFM in background
    // removal). Presence of `--tile-size` enables tiling; absence = whole-volume.
    if config.runs_a_tileable_network() {
        if let Some(sz) = config.inversion.tile_size {
            parts.push(format!("--tile-size {}", sz));
        }
        if let Some(h) = config.inversion.tile_halo {
            parts.push(format!("--tile-halo {}", h));
        }
    }

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
                if s.decompose.unwrap_phase != ds.decompose.unwrap_phase {
                    parts.push(format!("--decompose-unwrap-phase {}", s.decompose.unwrap_phase));
                }
                let (l, dl) = (&s.decompose.lsqcurvefit, &ds.decompose.lsqcurvefit);
                emit_usize(&mut parts, "--decompose-max-iterations", l.max_iterations, dl.max_iterations);
                emit_opt_usize(&mut parts, "--decompose-max-function-evaluations", l.max_function_evaluations, dl.max_function_evaluations);
                emit_f64(&mut parts, "--decompose-function-tolerance", l.function_tolerance, dl.function_tolerance);
                emit_f64(&mut parts, "--decompose-step-tolerance", l.step_tolerance, dl.step_tolerance);
                emit_f64(&mut parts, "--decompose-optimality-tolerance", l.optimality_tolerance, dl.optimality_tolerance);
                emit_f64(&mut parts, "--decompose-finite-difference-step", l.finite_difference_step, dl.finite_difference_step);
                emit_f64(&mut parts, "--decompose-typical-x", l.typical_x, dl.typical_x);
            }
            SeparationAlgorithm::ChiSepIlsqr => {
                let (c, dc) = (&s.chi_sep_ilsqr, &ds.chi_sep_ilsqr);
                emit_opt_f64(&mut parts, "--chi-sep-ilsqr-dr-pos", c.dr_pos, dc.dr_pos);
                emit_opt_f64(&mut parts, "--chi-sep-ilsqr-dr-neg", c.dr_neg, dc.dr_neg);
                emit_usize_arr3(&mut parts, "--chi-sep-ilsqr-pad", &c.pad, &dc.pad);
                emit_usize(&mut parts, "--chi-sep-ilsqr-cg-max-iter-lsqr", c.cg_max_iter_lsqr, dc.cg_max_iter_lsqr);
                emit_f64(&mut parts, "--chi-sep-ilsqr-cg-tol-lsqr", c.cg_tol_lsqr, dc.cg_tol_lsqr);
                emit_usize(&mut parts, "--chi-sep-ilsqr-cg-max-iter-sa", c.cg_max_iter_sa, dc.cg_max_iter_sa);
                emit_f64(&mut parts, "--chi-sep-ilsqr-cg-tol-sa", c.cg_tol_sa, dc.cg_tol_sa);
                emit_f64(&mut parts, "--chi-sep-ilsqr-d2-thr", c.d2_thr, dc.d2_thr);
                emit_f64(&mut parts, "--chi-sep-ilsqr-prc-w-i-min", c.prc_w_i_min, dc.prc_w_i_min);
                emit_f64(&mut parts, "--chi-sep-ilsqr-prc-w-i-max", c.prc_w_i_max, dc.prc_w_i_max);
                emit_f64(&mut parts, "--chi-sep-ilsqr-prc-w-g-min", c.prc_w_g_min, dc.prc_w_g_min);
                emit_f64(&mut parts, "--chi-sep-ilsqr-prc-w-g-max", c.prc_w_g_max, dc.prc_w_g_max);
                let (q, dq) = (&s.chi_sep_ilsqr.qsm_ilsqr, &ds.chi_sep_ilsqr.qsm_ilsqr);
                emit_usize(&mut parts, "--chi-sep-ilsqr-qsm-max-iter", q.max_iter, dq.max_iter);
                emit_f64(&mut parts, "--chi-sep-ilsqr-qsm-tol", q.tol, dq.tol);
                emit_f64(&mut parts, "--chi-sep-ilsqr-qsm-pad-mm", q.pad_mm, dq.pad_mm);
            }
            SeparationAlgorithm::ChiSepMedi => {
                let (c, dc) = (&s.chi_sep_medi, &ds.chi_sep_medi);
                emit_f64(&mut parts, "--chi-sep-medi-lambda", c.lambda, dc.lambda);
                emit_f64(&mut parts, "--chi-sep-medi-lambda-csf", c.lambda_csf, dc.lambda_csf);
                emit_opt_f64(&mut parts, "--chi-sep-medi-dr-pos", c.dr_pos, dc.dr_pos);
                emit_opt_f64(&mut parts, "--chi-sep-medi-dr-neg", c.dr_neg, dc.dr_neg);
                emit_usize(&mut parts, "--chi-sep-medi-pad", c.pad, dc.pad);
                emit_f64(&mut parts, "--chi-sep-medi-tol", c.tol, dc.tol);
                emit_usize(&mut parts, "--chi-sep-medi-max-iter", c.max_iter, dc.max_iter);
                emit_f64(&mut parts, "--chi-sep-medi-cg-tol", c.cg_tol, dc.cg_tol);
                emit_usize(&mut parts, "--chi-sep-medi-cg-max-iter", c.cg_max_iter, dc.cg_max_iter);
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

    // ── Dismissed recommendations ──
    // Only those that still hold: a dismissal whose setting has since changed is gone.
    let mut held = config.dismissed_recommendations.clone();
    if !held.is_empty() {
        let mut c = config.clone();
        crate::recommendations::prune_dismissals(&mut c, &crate::recommendations::Facts::default());
        held = c.dismissed_recommendations;
    }
    for x in &held {
        let arg = format!("{}={}", x.rule, x.chosen);
        // Rule ids and dismissed values are written in characters a shell leaves alone.
        let plain = arg.chars().all(|c| c.is_ascii_alphanumeric() || "-_.=,/".contains(c));
        parts.push(format!("--dismiss-recommendation {}", if plain { arg } else { shell_quote(&arg) }));
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
            emit_f64(parts, "--rts-tau", config.inversion.rts.tau, d.inversion.rts.tau);
            emit_f64(parts, "--rts-gamma", config.inversion.rts.gamma, d.inversion.rts.gamma);
            emit_i64_arr3(parts, "--rts-pad", &config.inversion.rts.pad, &d.inversion.rts.pad);
        }
        QsmAlgorithm::Tv => {
            emit_f64(parts, "--tv-lambda", config.inversion.tv.lambda, d.inversion.tv.lambda);
            emit_opt_f64(parts, "--tv-rho", config.inversion.tv.rho, d.inversion.tv.rho);
            emit_f64(parts, "--tv-tol", config.inversion.tv.tol, d.inversion.tv.tol);
            emit_usize(parts, "--tv-max-iter", config.inversion.tv.max_iter, d.inversion.tv.max_iter);
            emit_i64_arr3(parts, "--tv-pad", &config.inversion.tv.pad, &d.inversion.tv.pad);
        }
        QsmAlgorithm::Tkd => {
            emit_f64(parts, "--tkd-threshold", config.inversion.tkd.threshold, d.inversion.tkd.threshold);
            emit_i64_arr3(parts, "--tkd-pad", &config.inversion.tkd.pad, &d.inversion.tkd.pad);
        }
        QsmAlgorithm::Tsvd => {
            emit_f64(parts, "--tsvd-threshold", config.inversion.tsvd.threshold, d.inversion.tsvd.threshold);
            emit_i64_arr3(parts, "--tsvd-pad", &config.inversion.tsvd.pad, &d.inversion.tsvd.pad);
        }
        QsmAlgorithm::Tikhonov => {
            emit_f64(parts, "--tikhonov-lambda", config.inversion.tikhonov.lambda, d.inversion.tikhonov.lambda);
            emit_i64_arr3(parts, "--tikhonov-pad", &config.inversion.tikhonov.pad, &d.inversion.tikhonov.pad);
        }
        QsmAlgorithm::Nltv => {
            emit_f64(parts, "--nltv-lambda", config.inversion.nltv.lambda, d.inversion.nltv.lambda);
            emit_opt_f64(parts, "--nltv-rho", config.inversion.nltv.rho, d.inversion.nltv.rho);
            emit_f64(parts, "--nltv-mu", config.inversion.nltv.mu, d.inversion.nltv.mu);
            emit_f64(parts, "--nltv-tol", config.inversion.nltv.tol, d.inversion.nltv.tol);
            emit_usize(parts, "--nltv-max-iter", config.inversion.nltv.max_iter, d.inversion.nltv.max_iter);
            emit_f64(parts, "--nltv-tol-newton", config.inversion.nltv.tol_newton, d.inversion.nltv.tol_newton);
            emit_usize(parts, "--nltv-newton-iter", config.inversion.nltv.max_iter_newton, d.inversion.nltv.max_iter_newton);
            emit_enum(parts, "--nltv-te", &config.inversion.nltv.te, &d.inversion.nltv.te);
            emit_i64_arr3(parts, "--nltv-pad", &config.inversion.nltv.pad, &d.inversion.nltv.pad);
        }
        QsmAlgorithm::Medi => {
            emit_f64(parts, "--medi-lambda", config.inversion.medi.lambda, d.inversion.medi.lambda);
            emit_f64(parts, "--medi-percentage", config.inversion.medi.percentage, d.inversion.medi.percentage);
            emit_usize(parts, "--medi-max-iter", config.inversion.medi.max_iter, d.inversion.medi.max_iter);
            emit_usize(parts, "--medi-cg-max-iter", config.inversion.medi.cg_max_iter, d.inversion.medi.cg_max_iter);
            emit_f64(parts, "--medi-cg-tol", config.inversion.medi.cg_tol, d.inversion.medi.cg_tol);
            emit_f64(parts, "--medi-tol", config.inversion.medi.tol, d.inversion.medi.tol);
            emit_f64(parts, "--medi-smv-radius", config.inversion.medi.smv_radius, d.inversion.medi.smv_radius);
            if config.inversion.medi.smv_shrink_mask != d.inversion.medi.smv_shrink_mask {
                parts.push(if config.inversion.medi.smv_shrink_mask { "--medi-smv-shrink-mask" } else { "--no-medi-smv-shrink-mask" }.into());
            }
            if config.inversion.medi.merit != d.inversion.medi.merit {
                parts.push(format!("--medi-merit {}", config.inversion.medi.merit));
            }
            if config.inversion.medi.data_weighting != d.inversion.medi.data_weighting {
                parts.push(format!("--medi-data-weighting {}", config.inversion.medi.data_weighting));
            }
            {
                let (m, dm) = (&config.inversion.medi, &d.inversion.medi);
                if m.csf != dm.csf {
                    parts.push(format!("--medi-csf {}", m.csf));
                }
                if m.csf {
                    emit_f64(parts, "--medi-lambda-csf", m.lambda_csf, dm.lambda_csf);
                    emit_f64(parts, "--medi-csf-r2s-threshold", m.csf_r2s_threshold, dm.csf_r2s_threshold);
                    if m.csf_erode != dm.csf_erode {
                        parts.push(format!("--medi-csf-erode {}", m.csf_erode));
                    }
                }
                if m.zeropad != dm.zeropad {
                    parts.push(format!("--medi-zeropad {} {} {}", m.zeropad[0], m.zeropad[1], m.zeropad[2]));
                }
            }
            if config.inversion.medi.smv != d.inversion.medi.smv {
                parts.push(if config.inversion.medi.smv { "--medi-smv" } else { "--no-medi-smv" }.into());
            }
            let (m, dm) = (&config.inversion.medi, &d.inversion.medi);
            if m.msmv {
                parts.push("--medi-msmv".into());
                emit_usize(parts, "--medi-msmv-maxk", m.msmv_maxk, dm.msmv_maxk);
                emit_opt_f64(parts, "--medi-msmv-vessel-radius", m.msmv_vessel_radius, dm.msmv_vessel_radius);
                if !m.msmv_prefilter {
                    parts.push("--no-medi-msmv-prefilter".into());
                }
            }
            emit_enum(parts, "--medi-te", &m.te, &dm.te);
        }
        QsmAlgorithm::Tfi => {
            emit_f64(parts, "--tfi-lambda", config.inversion.tfi.lambda, d.inversion.tfi.lambda);
            emit_f64(parts, "--tfi-precond", config.inversion.tfi.precond, d.inversion.tfi.precond);
            if config.inversion.tfi.data_weighting != d.inversion.tfi.data_weighting {
                parts.push(format!("--tfi-data-weighting {}", config.inversion.tfi.data_weighting));
            }
            if config.inversion.tfi.merit != d.inversion.tfi.merit {
                parts.push(format!("--tfi-merit {}", config.inversion.tfi.merit));
            }
            if config.inversion.tfi.linear != d.inversion.tfi.linear {
                parts.push(format!("--tfi-linear {}", config.inversion.tfi.linear));
            }
            emit_f64(parts, "--tfi-percentage", config.inversion.tfi.percentage, d.inversion.tfi.percentage);
            emit_usize(parts, "--tfi-max-iter", config.inversion.tfi.max_iter, d.inversion.tfi.max_iter);
            emit_usize(parts, "--tfi-cg-max-iter", config.inversion.tfi.cg_max_iter, d.inversion.tfi.cg_max_iter);
            emit_f64(parts, "--tfi-cg-tol", config.inversion.tfi.cg_tol, d.inversion.tfi.cg_tol);
            emit_f64(parts, "--tfi-tol", config.inversion.tfi.tol, d.inversion.tfi.tol);
            emit_enum(parts, "--tfi-te", &config.inversion.tfi.te, &d.inversion.tfi.te);
        }
        QsmAlgorithm::Ilsqr => {
            emit_f64(parts, "--ilsqr-tol", config.inversion.ilsqr.tol, d.inversion.ilsqr.tol);
            emit_usize(parts, "--ilsqr-max-iter", config.inversion.ilsqr.max_iter, d.inversion.ilsqr.max_iter);
            emit_f64(parts, "--ilsqr-pad-mm", config.inversion.ilsqr.pad_mm, d.inversion.ilsqr.pad_mm);
        }
        QsmAlgorithm::Star => {
            let (c, dd) = (&config.inversion.star, &d.inversion.star);
            if c.pad != dd.pad {
                parts.push(format!("--star-pad {} {} {}", c.pad[0], c.pad[1], c.pad[2]));
            }
            emit_enum(parts, "--star-te", &c.te, &dd.te);
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
                emit_enum(parts, "--heidi-te", &c.te, &dd.te);
            }
        }
        QsmAlgorithm::Tgv => {
            emit_opt_usize(parts, "--tgv-iterations", config.inversion.tgv.iterations, d.inversion.tgv.iterations);
            emit_usize(parts, "--tgv-erosions", config.inversion.tgv.erosions, d.inversion.tgv.erosions);
            emit_f64(parts, "--tgv-alpha0", config.inversion.tgv.alpha0, d.inversion.tgv.alpha0);
            emit_f64(parts, "--tgv-alpha1", config.inversion.tgv.alpha1, d.inversion.tgv.alpha1);
            emit_f64(parts, "--tgv-step-size", config.inversion.tgv.step_size, d.inversion.tgv.step_size);
            emit_enum(parts, "--tgv-te", &config.inversion.tgv.te, &d.inversion.tgv.te);
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
            emit_enum(parts, "--qsmart-vessel-units", &q.vessel_size_units, &dq.vessel_size_units);
            emit_enum(parts, "--qsmart-vessel-magnitude", &q.vessel_magnitude, &dq.vessel_magnitude);
        }
        QsmAlgorithm::Ndi => {
            emit_f64(parts, "--ndi-tau", config.inversion.ndi.tau, d.inversion.ndi.tau);
            emit_f64(parts, "--ndi-alpha", config.inversion.ndi.alpha, d.inversion.ndi.alpha);
            emit_usize(parts, "--ndi-max-iter", config.inversion.ndi.max_iter, d.inversion.ndi.max_iter);
            if config.inversion.ndi.auto_stop != d.inversion.ndi.auto_stop {
                parts.push(format!("--ndi-auto-stop {}", config.inversion.ndi.auto_stop));
            }
            emit_enum(parts, "--ndi-te", &config.inversion.ndi.te, &d.inversion.ndi.te);
        }
        QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv => {
            let fa = &config.inversion.fansi;
            let df = &d.inversion.fansi;
            emit_f64(parts, "--fansi-alpha1", fa.alpha1, df.alpha1);
            emit_opt_f64(parts, "--fansi-mu1", fa.mu1, df.mu1);
            emit_f64(parts, "--fansi-mu2", fa.mu2, df.mu2);
            emit_opt_f64(parts, "--fansi-alpha0", fa.alpha0, df.alpha0);
            emit_opt_f64(parts, "--fansi-mu0", fa.mu0, df.mu0);
            emit_usize(parts, "--fansi-max-iter", fa.max_iter, df.max_iter);
            emit_f64(parts, "--fansi-tol-update", fa.tol_update, df.tol_update);
            emit_f64(parts, "--fansi-tol-delta", fa.tol_delta, df.tol_delta);
            emit_enum(parts, "--fansi-te", &fa.te, &df.te);
        }
        QsmAlgorithm::L1qsm => {
            let l = &config.inversion.l1qsm;
            let dl = &d.inversion.l1qsm;
            emit_f64(parts, "--l1qsm-alpha1", l.alpha1, dl.alpha1);
            emit_opt_f64(parts, "--l1qsm-mu1", l.mu1, dl.mu1);
            emit_f64(parts, "--l1qsm-mu2", l.mu2, dl.mu2);
            emit_f64(parts, "--l1qsm-mu3", l.mu3, dl.mu3);
            emit_f64(parts, "--l1qsm-lambda", l.lambda, dl.lambda);
            emit_usize(parts, "--l1qsm-max-iter", l.max_iter, dl.max_iter);
            emit_f64(parts, "--l1qsm-tol-update", l.tol_update, dl.tol_update);
            emit_f64(parts, "--l1qsm-tol-delta", l.tol_delta, dl.tol_delta);
            emit_enum(parts, "--l1qsm-te", &l.te, &dl.te);
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
            emit_enum(parts, "--whqsm-weight", &w.weight, &dw.weight);
            emit_enum(parts, "--whqsm-te", &w.te, &dw.te);
        }
        QsmAlgorithm::Hdqsm => {
            let h = &config.inversion.hdqsm;
            let dh = &d.inversion.hdqsm;
            emit_f64(parts, "--hdqsm-alpha-l2", h.alpha_l2, dh.alpha_l2);
            emit_opt_f64(parts, "--hdqsm-mu1-l2", h.mu1_l2, dh.mu1_l2);
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
        // Deep-learning inversions have no per-net params (weights + fixed graph); their
        // tiling is emitted below, with BFRnet's and iQFM's.
        QsmAlgorithm::Xqsm | QsmAlgorithm::Qsmnet | QsmAlgorithm::QsmnetPlus
        | QsmAlgorithm::Ir2qsm | QsmAlgorithm::Lpcnn | QsmAlgorithm::ModlQsm
        | QsmAlgorithm::Nextqsm | QsmAlgorithm::Autoqsm | QsmAlgorithm::Qsmgan
        | QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => {}
    }
}

fn emit_f64(parts: &mut Vec<String>, flag: &str, val: f64, default: f64) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
fn emit_usize(parts: &mut Vec<String>, flag: &str, val: usize, default: usize) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
/// An optional setting: said when it is set to something other than the default. Unset is the
/// value qsm-core derives, which the command line says by leaving the flag out.
fn emit_opt_f64(parts: &mut Vec<String>, flag: &str, val: Option<f64>, default: Option<f64>) {
    if let Some(v) = val.filter(|_| val != default) { parts.push(format!("{} {}", flag, v)); }
}
fn emit_opt_usize(parts: &mut Vec<String>, flag: &str, val: Option<usize>, default: Option<usize>) {
    if let Some(v) = val.filter(|_| val != default) { parts.push(format!("{} {}", flag, v)); }
}
fn emit_i32(parts: &mut Vec<String>, flag: &str, val: i32, default: i32) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
fn emit_enum<T: PartialEq + std::fmt::Display>(parts: &mut Vec<String>, flag: &str, val: &T, default: &T) {
    if val != default { parts.push(format!("{} {}", flag, val)); }
}
fn emit_usize_arr3(parts: &mut Vec<String>, flag: &str, val: &[usize; 3], default: &[usize; 3]) {
    if val != default { parts.push(format!("{} {} {} {}", flag, val[0], val[1], val[2])); }
}
fn emit_i64_arr3(parts: &mut Vec<String>, flag: &str, val: &[i64; 3], default: &[i64; 3]) {
    if val != default { parts.push(format!("{} {} {} {}", flag, val[0], val[1], val[2])); }
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
        // A classical inversion after a tiled background-removal network keeps them.
        c.inversion.algorithm = QsmAlgorithm::Tkd;
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Bfrnet;
        assert!(generate_command(&c).contains("--tile-size 64"), "BFRnet tiles");
        // The phase-input reconstructions tile too.
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Vsharp;
        c.inversion.algorithm = QsmAlgorithm::Iqsm;
        assert!(generate_command(&c).contains("--tile-size 64"), "iQSM tiles");
    }

    /// MEDI-mSMV is MEDI's option: said only with MEDI, its settings only when it is on and
    /// changed.
    #[test]
    fn test_medi_msmv_command() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_command(&config).contains("msmv"));
        config.inversion.medi.msmv = true;
        config.inversion.medi.msmv_maxk = 9;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--medi-msmv") && cmd.contains("--medi-msmv-maxk 9"), "got: {cmd}");
        assert!(!cmd.contains("--medi-msmv-vessel-radius") && !cmd.contains("prefilter"), "got: {cmd}");
        config.inversion.medi.msmv_vessel_radius = Some(12.0);
        config.inversion.medi.msmv_prefilter = false;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--medi-msmv-vessel-radius 12") && cmd.contains("--no-medi-msmv-prefilter"), "got: {cmd}");
        // Under another inversion it does not apply.
        config.inversion.algorithm = QsmAlgorithm::Rts;
        assert!(!generate_command(&config).contains("msmv"));
    }

    /// A command names the background removal whenever it is not the general default, under
    /// every inversion: nothing depends on the inversion.
    #[test]
    fn the_background_removal_is_named_unless_it_is_the_default() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_command(&config).contains("--bf-algorithm"), "{}", generate_command(&config));
        config.bg_removal.algorithm = BfAlgorithm::Pdf;
        assert!(generate_command(&config).contains("--bf-algorithm pdf"));
        config.inversion.algorithm = QsmAlgorithm::Rts;
        assert!(generate_command(&config).contains("--bf-algorithm pdf"));
    }

    #[test]
    fn medi_smv_shrink_mask_command() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_command(&config).contains("shrink"));
        config.inversion.medi.smv_shrink_mask = false;
        assert!(generate_command(&config).contains("--no-medi-smv-shrink-mask"));
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
        config.separation.decompose.lsqcurvefit.max_iterations = 99;
        config.separation.decompose.unwrap_phase = true;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--chisep decompose"));
        assert!(cmd.contains("--decompose-max-iterations 99") && cmd.contains("--decompose-unwrap-phase true"), "{cmd}");

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
        c.inversion.tv.lambda = 0.002;
        c.inversion.tv.max_iter = 100;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tv-lambda 0.002"));
        assert!(cmd.contains("--tv-max-iter 100"));
        // ρ unset follows λ (qsm-core's 100·λ): no flag. Set, it is said.
        assert!(!cmd.contains("--tv-rho"), "{cmd}");
        c.inversion.tv.rho = Some(0.5);
        assert!(generate_command(&c).contains("--tv-rho 0.5"));
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
        c.inversion.tgv.iterations = Some(500);
        c.inversion.tgv.erosions = 2;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tgv-iterations 500"));
        assert!(cmd.contains("--tgv-erosions 2"));
    }

    /// Each method's echo time, and the method settings from QSM.rs v0.40.0, are emitted only away
    /// from their defaults; a given echo time is written in ms.
    #[test]
    fn echo_times_and_method_settings_are_emitted_when_changed() {
        let methods = [
            QsmAlgorithm::Nltv, QsmAlgorithm::Medi, QsmAlgorithm::Tfi, QsmAlgorithm::Ndi,
            QsmAlgorithm::Fansi, QsmAlgorithm::FansiTgv, QsmAlgorithm::L1qsm, QsmAlgorithm::Whqsm,
            QsmAlgorithm::Heidi, QsmAlgorithm::Tgv,
        ];
        for alg in methods {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            let flag = if alg == QsmAlgorithm::FansiTgv { "fansi".to_string() } else { alg.to_string() };
            assert!(!generate_command(&c).contains("-te "), "{alg}: the default is not emitted");
            let i = &mut c.inversion;
            for te in [&mut i.nltv.te, &mut i.medi.te, &mut i.tfi.te, &mut i.ndi.te, &mut i.fansi.te,
                       &mut i.l1qsm.te, &mut i.whqsm.te, &mut i.heidi.te, &mut i.tgv.te] {
                *te = EchoTimeChoice::Seconds(0.007);
            }
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--{flag}-te 7")), "{alg}: {cmd}");
        }
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Whqsm;
        c.inversion.whqsm.weight = WhqsmWeight::InVivo;
        assert!(generate_command(&c).contains("--whqsm-weight in-vivo"));
        c.inversion.algorithm = QsmAlgorithm::Nltv;
        c.inversion.nltv.rho = Some(0.5);
        c.inversion.nltv.tol_newton = 1e-4;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--nltv-rho 0.5") && cmd.contains("--nltv-tol-newton 0.0001"), "{cmd}");
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.inversion.rts.tau = 0.5;
        c.inversion.rts.gamma = 2.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--rts-tau 0.5") && cmd.contains("--rts-gamma 2"), "{cmd}");
        c.bg_removal.algorithm = BfAlgorithm::Pdf;
        c.bg_removal.pdf.n_cg = 30;
        c.bg_removal.pdf.space = PdfSpace::Kspace;
        c.bg_removal.pdf.n_pad = 0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--pdf-n-cg 30") && cmd.contains("--pdf-space kspace") && cmd.contains("--pdf-n-pad 0"), "{cmd}");
        for (alg, flag) in [(BfAlgorithm::Harperella, "--harperella-pad"), (BfAlgorithm::Iharperella, "--iharperella-pad")] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            assert!(!generate_command(&c).contains(flag));
            c.bg_removal.harperella.pad = [2, 4, 6];
            c.bg_removal.iharperella.pad = [2, 4, 6];
            assert!(generate_command(&c).contains(&format!("{flag} 2 4 6")));
        }
    }

    /// The new settings in a config file: an echo time as a name or `{ seconds = … }`, the WH-QSM
    /// weight and PDF's kernel by their names, and the padding as three numbers.
    #[test]
    fn echo_times_and_method_settings_in_config_files() {
        let c: PipelineConfig = toml::from_str("[inversion.medi]\nte = \"first\"\n[inversion.tgv]\nte = { seconds = 0.03 }\n\
            [inversion.whqsm]\nweight = \"max-normalised\"\n[bg_removal.pdf]\nspace = \"kspace\"\n\
            [bg_removal.harperella]\npad = [1, 2, 3]\n").unwrap();
        assert_eq!(c.inversion.medi.te, EchoTimeChoice::First);
        assert_eq!(c.inversion.tgv.te, EchoTimeChoice::Seconds(0.03));
        assert_eq!(c.inversion.whqsm.weight, WhqsmWeight::MaxNormalised);
        assert_eq!(c.bg_removal.pdf.space, PdfSpace::Kspace);
        assert_eq!(c.bg_removal.harperella.pad, [1, 2, 3]);
        assert!(toml::from_str::<PipelineConfig>("[inversion.ndi]\nte = \"median\"\n").is_err());
        let back: PipelineConfig = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn test_qsmart_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        c.inversion.qsmart.vasc_sphere_radius = 10;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsmart-vasc-sphere-radius 10"));
    }

    /// Under QSMART the default mask goes unsaid, as elsewhere: it is not replaced.
    #[test]
    fn qsmart_keeps_the_default_mask() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        assert!(!generate_command(&c).contains("--mask"), "{}", generate_command(&c));
        c.masking.sections = crate::masking::mask_presets().into_iter().find(|(n, _)| *n == "bet").unwrap().1.sections;
        assert!(generate_command(&c).contains("--mask-preset bet"), "{}", generate_command(&c));
    }

    /// Under QSMART the command says the field mapping it was given, as elsewhere; QSMART's vessel
    /// options are said when changed.
    #[test]
    fn qsmart_field_mapping_and_vessel_options_are_in_the_command() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--unwrapping-algorithm") && !cmd.contains("--b0-estimation"), "{cmd}");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        c.field_mapping.laplacian_solver = LaplacianSolver::Mirror;
        c.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
        let cmd = generate_command(&c);
        for flag in ["--unwrapping-algorithm laplacian", "--laplacian-solver mirror", "--b0-estimation qsmart-echofit"] {
            assert!(cmd.contains(flag), "{cmd}");
        }
        assert!(!cmd.contains("--qsmart-fit-threshold") && !cmd.contains("--qsmart-vessel"), "{cmd}");
        assert!(!cmd.contains("--laplacian-unmasked"), "{cmd}");
        c.field_mapping.laplacian_unmasked = true;
        assert!(generate_command(&c).contains("--laplacian-unmasked"));
        c.field_mapping.qsmart_fit_percentile = 95.0;
        c.inversion.qsmart.vessel_magnitude = QsmartVesselMagnitude::Combined;
        c.inversion.qsmart.vessel_size_units = VesselSizeUnits::Mm;
        let cmd = generate_command(&c);
        for flag in ["--qsmart-fit-percentile 95", "--qsmart-vessel-magnitude combined", "--qsmart-vessel-units mm"] {
            assert!(cmd.contains(flag), "{cmd}");
        }
        assert!(!cmd.contains("--qsmart-fit-threshold"), "{cmd}");
        // A fixed threshold: the mode, and the threshold when changed; not the percentile.
        c.field_mapping.qsmart_fit_threshold_mode = QsmartFitThresholdMode::Absolute;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsmart-fit-threshold-mode absolute") && !cmd.contains("--qsmart-fit-percentile")
                && !cmd.contains("--qsmart-fit-threshold "), "{cmd}");
        c.field_mapping.qsmart_fit_threshold = 30.0;
        assert!(generate_command(&c).contains("--qsmart-fit-threshold 30"), "{}", generate_command(&c));
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
    fn test_vsharp_radius_step_flag() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--vsharp-radius-step"));
        c.bg_removal.vsharp.radius_step = 2.0;
        assert!(generate_command(&c).contains("--vsharp-radius-step 2"));
    }

    #[test]
    fn test_vsharp_grid_flags() {
        let mut c = PipelineConfig::default();
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--vsharp-grid"), "the default grid is not stated: {cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Native;
        c.bg_removal.vsharp.grid_spacing = 0.8;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--vsharp-grid native") && !cmd.contains("--vsharp-grid-spacing"), "unused unless isotropic: {cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Isotropic;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--vsharp-grid ") && cmd.contains("--vsharp-grid-spacing 0.8"), "{cmd}");
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
    fn test_complex_fit_flags() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::ComplexFit;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-estimation complex-fit") && !cmd.contains("--complex-fit-"), "{cmd}");
        assert!(!cmd.contains("--linear-fit-"), "the linear fit's settings mean nothing to it: {cmd}");
        (c.field_mapping.complex_fit.reltol, c.field_mapping.complex_fit.max_iter) = (1e-3, 12);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--complex-fit-reltol 0.001") && cmd.contains("--complex-fit-max-iter 12"), "{cmd}");
        c.field_mapping.b0_estimation = B0Estimation::WeightedAvg;
        assert!(!generate_command(&c).contains("--complex-fit-"));
    }

    #[test]
    fn test_medi_csf_and_tfi_flags() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Medi;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("csf") && !cmd.contains("zeropad"), "defaults are not emitted: {cmd}");
        (c.inversion.medi.lambda_csf, c.inversion.medi.csf_r2s_threshold, c.inversion.medi.csf_erode) = (50.0, 4.0, false);
        c.inversion.medi.zeropad = [2, 0, 2];
        let cmd = generate_command(&c);
        for f in ["--medi-lambda-csf 50", "--medi-csf-r2s-threshold 4", "--medi-csf-erode false", "--medi-zeropad 2 0 2"] {
            assert!(cmd.contains(f), "{f} missing: {cmd}");
        }
        c.inversion.medi.csf = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--medi-csf false") && !cmd.contains("--medi-lambda-csf"), "{cmd}");
        c.inversion.algorithm = QsmAlgorithm::Tfi;
        (c.inversion.tfi.merit, c.inversion.tfi.linear) = (false, false);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tfi-merit false") && cmd.contains("--tfi-linear false") && !cmd.contains("--medi-"), "{cmd}");
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
        let cmd = generate_command(&c);
        for f in ["--robust-fit-loss huber", "--robust-fit-tuning 2.5", "--robust-fit-iterations 5",
                  "--robust-fit-downweight-level 0.5", "--robust-fit-flag-ratio 4",
                  "--linear-fit-estimate-offset false"] {
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
    fn test_star_settings() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Star;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-algorithm star"), "{cmd}");
        assert!(!cmd.contains("--star-pad") && !cmd.contains("--star-te"), "defaults are not emitted: {cmd}");
        c.inversion.star.pad = [4, 4, 12];
        c.inversion.star.te = EchoTimeChoice::First;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--star-pad 4 4 12") && cmd.contains("--star-te first"), "{cmd}");
        c.inversion.star.te = EchoTimeChoice::Seconds(0.007);
        assert!(generate_command(&c).contains("--star-te 7"), "an echo time is given in ms");
    }

    #[test]
    fn star_te_in_config_files() {
        let c: StarConfig = toml::from_str("te = \"first\"").unwrap();
        assert_eq!(c.te, EchoTimeChoice::First);
        let c: StarConfig = toml::from_str("te = { seconds = 0.03 }").unwrap();
        assert_eq!(c.te, EchoTimeChoice::Seconds(0.03));
        assert!(toml::from_str::<StarConfig>("te = \"median\"").is_err());
        let mut d = PipelineConfig::default();
        d.inversion.star.te = EchoTimeChoice::Seconds(0.025);
        let back: PipelineConfig = toml::from_str(&toml::to_string(&d).unwrap()).unwrap();
        assert_eq!(back.inversion.star.te, EchoTimeChoice::Seconds(0.025));
    }

    #[test]
    fn echo_time_choice_parses_names_and_milliseconds() {
        for c in EchoTimeChoice::NAMED {
            assert_eq!(EchoTimeChoice::parse(&c.to_string()), Ok(c));
        }
        assert_eq!(EchoTimeChoice::parse("20"), Ok(EchoTimeChoice::Seconds(0.02)));
        assert!(EchoTimeChoice::parse("0").is_err() && EchoTimeChoice::parse("median").is_err());
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

    /// The unmasked Laplacian unwrapping is said only when it is on and Laplacian unwrapping
    /// runs, and a config file keeps it.
    #[test]
    fn test_laplacian_unmasked() {
        let mut c = PipelineConfig::default();
        c.field_mapping.laplacian_unmasked = true;
        assert!(!generate_command(&c).contains("--laplacian-unmasked"), "ROMEO has no Laplacian unwrapping");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        assert!(generate_command(&c).contains("--laplacian-unmasked"));
        c.field_mapping.laplacian_unmasked = false;
        assert!(!generate_command(&c).contains("--laplacian-unmasked"), "off is the default");
        let f: FieldMappingConfig = toml::from_str("laplacian_unmasked = true").unwrap();
        assert!(f.laplacian_unmasked);
        assert!(!toml::from_str::<FieldMappingConfig>("").unwrap().laplacian_unmasked);
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
        assert!(!cmd.contains("--two-pass-mask-combine"), "unset, it is the main mask's: {cmd}");
        // Its own combine mode, once it has sections to combine.
        c.masking.two_pass_combine = Some(crate::masking::MaskCombine::And);
        assert!(!generate_command(&c).contains("--two-pass-mask-combine"));
        c.masking.two_pass_sections.as_mut().unwrap().extend(crate::masking::default_two_pass_sections());
        assert!(generate_command(&c).contains("--two-pass-mask-combine and"));
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
    fn n4_and_its_settings_are_said_when_they_apply() {
        let mut c = PipelineConfig::default();
        c.n4.shrink_factor = 2;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--inhomogeneity-method") && !cmd.contains("--n4-"), "makehomogeneous hides N4's settings: {cmd}");
        c.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        c.homogeneity.sigma_mm = 5.0;
        c.n4.max_iterations = vec![20, 10];
        c.n4.mesh = [2, 2, 1];
        c.n4.wiener_noise = 0.02;
        let cmd = generate_command(&c);
        for f in ["--inhomogeneity-method n4", "--n4-shrink-factor 2", "--n4-max-iterations 20 10",
                  "--n4-mesh 2 2 1", "--n4-wiener-noise 0.02"] {
            assert!(cmd.contains(f), "{f} missing: {cmd}");
        }
        assert!(!cmd.contains("--homogeneity-sigma-mm") && !cmd.contains("--n4-spline-order"), "{cmd}");
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
        c.field_mapping.phase_offset_sigma_mm = [10.0, 10.0, 5.0];
        let cmd = generate_command(&c);
        assert!(cmd.contains("--phase-offset-sigma-mm 10 10 5"));
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
