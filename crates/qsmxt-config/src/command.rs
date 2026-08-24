//! CLI command generation from PipelineConfig.

use crate::config::*;
use crate::enums::*;

/// Generate a `qsmxt run` CLI command from a pipeline configuration.
/// Compares against defaults and only emits flags that differ.
pub fn generate_command(config: &PipelineConfig) -> String {
    let d = PipelineConfig::default();
    let mut parts: Vec<String> = vec!["qsmxt".into(), "run".into(), "<bids_dir>".into()];

    // ── Pipeline toggles ──
    if !config.pipeline.do_qsm { parts.push("--no-qsm".into()); }
    if config.pipeline.do_swi { parts.push("--do-swi".into()); }
    if config.pipeline.do_t2starmap { parts.push("--do-t2starmap".into()); }
    // R2*, R2 and R2' are chained: chi-separation implies R2' implies (R2 + R2*). Only surface each
    // flag when it is set on its own, so `--do-chisep` doesn't drag redundant relaxometry flags in.
    if config.pipeline.do_r2primemap && !config.pipeline.do_chi_separation {
        parts.push("--do-r2primemap".into());
    }
    if config.pipeline.do_r2map && !config.pipeline.do_chi_separation && !config.pipeline.do_r2primemap {
        parts.push("--do-r2map".into());
    }
    if config.pipeline.do_r2starmap && !config.pipeline.do_chi_separation && !config.pipeline.do_r2primemap {
        parts.push("--do-r2starmap".into());
    }
    if config.pipeline.do_chi_separation { parts.push("--do-chisep".into()); }
    if let Some(tool) = &config.separation.custom_qsm_tool { parts.push(format!("--use-custom-qsm {}", tool)); }
    if let Some(tool) = &config.separation.custom_r2_tool { parts.push(format!("--use-custom-r2 {}", tool)); }
    if let Some(tool) = &config.separation.custom_r2prime_tool { parts.push(format!("--use-custom-r2prime {}", tool)); }
    if config.pipeline.export_dicom { parts.push("--export-dicom".into()); }
    emit_f64(&mut parts, "--obliquity-threshold", config.pipeline.obliquity_threshold, d.pipeline.obliquity_threshold);

    // ── Inhomogeneity ──
    if config.masking.inhomogeneity_correction != d.masking.inhomogeneity_correction {
        if config.masking.inhomogeneity_correction {
            parts.push("--inhomogeneity-correction".into());
        } else {
            parts.push("--no-inhomogeneity-correction".into());
        }
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
    emit_f64_arr3(&mut parts, "--phase-offset-sigma", &config.field_mapping.phase_offset_sigma, &d.field_mapping.phase_offset_sigma);
    if config.field_mapping.bipolar_correction { parts.push("--bipolar-correction".into()); }
    emit_enum(&mut parts, "--unwrapping-algorithm", &config.field_mapping.unwrapping_algorithm, &d.field_mapping.unwrapping_algorithm);
    emit_enum(&mut parts, "--b0-estimation", &config.field_mapping.b0_estimation, &d.field_mapping.b0_estimation);
    emit_enum(&mut parts, "--b0-weight-type", &config.field_mapping.b0_weight_type, &d.field_mapping.b0_weight_type);

    // ROMEO params
    let r = &config.field_mapping.romeo;
    let rd = &d.field_mapping.romeo;
    if r.individual != rd.individual && !r.individual { parts.push("--no-romeo-individual".into()); }
    if !r.individual { emit_usize(&mut parts, "--romeo-template", r.template + 1, rd.template + 1); }
    if r.correct_global != rd.correct_global && !r.correct_global { parts.push("--no-romeo-correct-global".into()); }
    if r.phase_gradient_coherence != rd.phase_gradient_coherence && !r.phase_gradient_coherence { parts.push("--no-romeo-phase-gradient-coherence".into()); }
    if r.mag_coherence != rd.mag_coherence && !r.mag_coherence { parts.push("--no-romeo-mag-coherence".into()); }
    if r.mag_weight != rd.mag_weight && !r.mag_weight { parts.push("--no-romeo-mag-weight".into()); }

    // ── Masking ──
    if config.masking.sections != d.masking.sections {
        for section in &config.masking.sections {
            parts.push(format!("--mask {}", section));
        }
    }

    // ── BET ──
    emit_f64(&mut parts, "--bet-fractional-intensity", config.bet.fractional_intensity, d.bet.fractional_intensity);
    emit_f64(&mut parts, "--bet-smoothness", config.bet.smoothness, d.bet.smoothness);
    emit_f64(&mut parts, "--bet-gradient-threshold", config.bet.gradient_threshold, d.bet.gradient_threshold);
    emit_usize(&mut parts, "--bet-iterations", config.bet.iterations, d.bet.iterations);
    emit_usize(&mut parts, "--bet-subdivisions", config.bet.subdivisions, d.bet.subdivisions);

    // ── Background removal ──
    emit_enum(&mut parts, "--bf-algorithm", &config.bg_removal.algorithm, &d.bg_removal.algorithm);
    match config.bg_removal.algorithm {
        BfAlgorithm::Vsharp => {
            emit_f64(&mut parts, "--vsharp-threshold", config.bg_removal.vsharp.threshold, d.bg_removal.vsharp.threshold);
            emit_f64(&mut parts, "--vsharp-max-radius", config.bg_removal.vsharp.max_radius, d.bg_removal.vsharp.max_radius);
            emit_f64(&mut parts, "--vsharp-min-radius", config.bg_removal.vsharp.min_radius, d.bg_removal.vsharp.min_radius);
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
            emit_f64(&mut parts, "--iharperella-radius", config.bg_removal.iharperella.radius, d.bg_removal.iharperella.radius);
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
    match config.inversion.algorithm {
        QsmAlgorithm::Rts => {
            emit_f64(&mut parts, "--rts-delta", config.inversion.rts.delta, d.inversion.rts.delta);
            emit_f64(&mut parts, "--rts-mu", config.inversion.rts.mu, d.inversion.rts.mu);
            emit_f64(&mut parts, "--rts-rho", config.inversion.rts.rho, d.inversion.rts.rho);
            emit_f64(&mut parts, "--rts-tol", config.inversion.rts.tol, d.inversion.rts.tol);
            emit_usize(&mut parts, "--rts-max-iter", config.inversion.rts.max_iter, d.inversion.rts.max_iter);
            emit_usize(&mut parts, "--rts-lsmr-iter", config.inversion.rts.lsmr_iter, d.inversion.rts.lsmr_iter);
        }
        QsmAlgorithm::Tv => {
            emit_f64(&mut parts, "--tv-lambda", config.inversion.tv.lambda, d.inversion.tv.lambda);
            emit_f64(&mut parts, "--tv-rho", config.inversion.tv.rho, d.inversion.tv.rho);
            emit_f64(&mut parts, "--tv-tol", config.inversion.tv.tol, d.inversion.tv.tol);
            emit_usize(&mut parts, "--tv-max-iter", config.inversion.tv.max_iter, d.inversion.tv.max_iter);
        }
        QsmAlgorithm::Tkd => { emit_f64(&mut parts, "--tkd-threshold", config.inversion.tkd.threshold, d.inversion.tkd.threshold); }
        QsmAlgorithm::Tsvd => { emit_f64(&mut parts, "--tsvd-threshold", config.inversion.tsvd.threshold, d.inversion.tsvd.threshold); }
        QsmAlgorithm::Tikhonov => { emit_f64(&mut parts, "--tikhonov-lambda", config.inversion.tikhonov.lambda, d.inversion.tikhonov.lambda); }
        QsmAlgorithm::Nltv => {
            emit_f64(&mut parts, "--nltv-lambda", config.inversion.nltv.lambda, d.inversion.nltv.lambda);
            emit_f64(&mut parts, "--nltv-mu", config.inversion.nltv.mu, d.inversion.nltv.mu);
            emit_f64(&mut parts, "--nltv-tol", config.inversion.nltv.tol, d.inversion.nltv.tol);
            emit_usize(&mut parts, "--nltv-max-iter", config.inversion.nltv.max_iter, d.inversion.nltv.max_iter);
            emit_usize(&mut parts, "--nltv-newton-iter", config.inversion.nltv.newton_iter, d.inversion.nltv.newton_iter);
        }
        QsmAlgorithm::Medi => {
            emit_f64(&mut parts, "--medi-lambda", config.inversion.medi.lambda, d.inversion.medi.lambda);
            emit_f64(&mut parts, "--medi-percentage", config.inversion.medi.percentage, d.inversion.medi.percentage);
            emit_usize(&mut parts, "--medi-max-iter", config.inversion.medi.max_iter, d.inversion.medi.max_iter);
            emit_usize(&mut parts, "--medi-cg-max-iter", config.inversion.medi.cg_max_iter, d.inversion.medi.cg_max_iter);
            emit_f64(&mut parts, "--medi-cg-tol", config.inversion.medi.cg_tol, d.inversion.medi.cg_tol);
            emit_f64(&mut parts, "--medi-tol", config.inversion.medi.tol, d.inversion.medi.tol);
            emit_f64(&mut parts, "--medi-smv-radius", config.inversion.medi.smv_radius, d.inversion.medi.smv_radius);
            if config.inversion.medi.smv != d.inversion.medi.smv && config.inversion.medi.smv {
                parts.push("--medi-smv".into());
            }
        }
        QsmAlgorithm::Tfi => {
            emit_f64(&mut parts, "--tfi-lambda", config.inversion.tfi.lambda, d.inversion.tfi.lambda);
            emit_f64(&mut parts, "--tfi-precond", config.inversion.tfi.precond, d.inversion.tfi.precond);
            emit_f64(&mut parts, "--tfi-percentage", config.inversion.tfi.percentage, d.inversion.tfi.percentage);
            emit_usize(&mut parts, "--tfi-max-iter", config.inversion.tfi.max_iter, d.inversion.tfi.max_iter);
            emit_usize(&mut parts, "--tfi-cg-max-iter", config.inversion.tfi.cg_max_iter, d.inversion.tfi.cg_max_iter);
            emit_f64(&mut parts, "--tfi-cg-tol", config.inversion.tfi.cg_tol, d.inversion.tfi.cg_tol);
            emit_f64(&mut parts, "--tfi-tol", config.inversion.tfi.tol, d.inversion.tfi.tol);
        }
        QsmAlgorithm::Ilsqr => {
            emit_f64(&mut parts, "--ilsqr-tol", config.inversion.ilsqr.tol, d.inversion.ilsqr.tol);
            emit_usize(&mut parts, "--ilsqr-max-iter", config.inversion.ilsqr.max_iter, d.inversion.ilsqr.max_iter);
        }
        QsmAlgorithm::Tgv => {
            emit_usize(&mut parts, "--tgv-iterations", config.inversion.tgv.iterations, d.inversion.tgv.iterations);
            emit_usize(&mut parts, "--tgv-erosions", config.inversion.tgv.erosions, d.inversion.tgv.erosions);
            emit_f64(&mut parts, "--tgv-alpha0", config.inversion.tgv.alpha0, d.inversion.tgv.alpha0);
            emit_f64(&mut parts, "--tgv-alpha1", config.inversion.tgv.alpha1, d.inversion.tgv.alpha1);
            emit_f64(&mut parts, "--tgv-step-size", config.inversion.tgv.step_size, d.inversion.tgv.step_size);
            emit_f64(&mut parts, "--tgv-tol", config.inversion.tgv.tol, d.inversion.tgv.tol);
        }
        QsmAlgorithm::Qsmart => {
            let q = &config.inversion.qsmart;
            let dq = &d.inversion.qsmart;
            emit_enum(&mut parts, "--qsmart-inversion", &q.inversion, &dq.inversion);
            emit_f64(&mut parts, "--qsmart-ilsqr-tol", q.ilsqr_tol, dq.ilsqr_tol);
            emit_usize(&mut parts, "--qsmart-ilsqr-max-iter", q.ilsqr_max_iter, dq.ilsqr_max_iter);
            emit_i32(&mut parts, "--qsmart-vasc-sphere-radius", q.vasc_sphere_radius, dq.vasc_sphere_radius);
            emit_i32(&mut parts, "--qsmart-sdf-spatial-radius", q.sdf_spatial_radius, dq.sdf_spatial_radius);
            emit_f64(&mut parts, "--qsmart-sdf-sigma1-stage1", q.sdf_sigma1_stage1, dq.sdf_sigma1_stage1);
            emit_f64(&mut parts, "--qsmart-sdf-sigma2-stage1", q.sdf_sigma2_stage1, dq.sdf_sigma2_stage1);
            emit_f64(&mut parts, "--qsmart-sdf-sigma1-stage2", q.sdf_sigma1_stage2, dq.sdf_sigma1_stage2);
            emit_f64(&mut parts, "--qsmart-sdf-sigma2-stage2", q.sdf_sigma2_stage2, dq.sdf_sigma2_stage2);
            emit_f64(&mut parts, "--qsmart-sdf-lower-lim", q.sdf_lower_lim, dq.sdf_lower_lim);
            emit_f64(&mut parts, "--qsmart-sdf-curv-constant", q.sdf_curv_constant, dq.sdf_curv_constant);
            emit_f64(&mut parts, "--qsmart-frangi-scale-min", q.frangi_scale_min, dq.frangi_scale_min);
            emit_f64(&mut parts, "--qsmart-frangi-scale-max", q.frangi_scale_max, dq.frangi_scale_max);
            emit_f64(&mut parts, "--qsmart-frangi-scale-ratio", q.frangi_scale_ratio, dq.frangi_scale_ratio);
            emit_f64(&mut parts, "--qsmart-frangi-c", q.frangi_c, dq.frangi_c);
        }
        QsmAlgorithm::Ndi => {
            emit_f64(&mut parts, "--ndi-tau", config.inversion.ndi.tau, d.inversion.ndi.tau);
            emit_f64(&mut parts, "--ndi-alpha", config.inversion.ndi.alpha, d.inversion.ndi.alpha);
            emit_usize(&mut parts, "--ndi-max-iter", config.inversion.ndi.max_iter, d.inversion.ndi.max_iter);
            emit_f64(&mut parts, "--ndi-phase-scale", config.inversion.ndi.phase_scale, d.inversion.ndi.phase_scale);
        }
        QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv => {
            let fa = &config.inversion.fansi;
            let df = &d.inversion.fansi;
            emit_f64(&mut parts, "--fansi-alpha1", fa.alpha1, df.alpha1);
            emit_f64(&mut parts, "--fansi-mu1", fa.mu1, df.mu1);
            emit_f64(&mut parts, "--fansi-mu2", fa.mu2, df.mu2);
            emit_f64(&mut parts, "--fansi-alpha0", fa.alpha0, df.alpha0);
            emit_f64(&mut parts, "--fansi-mu0", fa.mu0, df.mu0);
            emit_usize(&mut parts, "--fansi-max-iter", fa.max_iter, df.max_iter);
            emit_f64(&mut parts, "--fansi-tol-update", fa.tol_update, df.tol_update);
            emit_f64(&mut parts, "--fansi-tol-delta", fa.tol_delta, df.tol_delta);
            emit_f64(&mut parts, "--fansi-phase-scale", fa.phase_scale, df.phase_scale);
        }
        QsmAlgorithm::L1qsm => {
            let l = &config.inversion.l1qsm;
            let dl = &d.inversion.l1qsm;
            emit_f64(&mut parts, "--l1qsm-alpha1", l.alpha1, dl.alpha1);
            emit_f64(&mut parts, "--l1qsm-mu1", l.mu1, dl.mu1);
            emit_f64(&mut parts, "--l1qsm-mu2", l.mu2, dl.mu2);
            emit_f64(&mut parts, "--l1qsm-mu3", l.mu3, dl.mu3);
            emit_f64(&mut parts, "--l1qsm-lambda", l.lambda, dl.lambda);
            emit_usize(&mut parts, "--l1qsm-max-iter", l.max_iter, dl.max_iter);
            emit_f64(&mut parts, "--l1qsm-tol-update", l.tol_update, dl.tol_update);
            emit_f64(&mut parts, "--l1qsm-tol-delta", l.tol_delta, dl.tol_delta);
            emit_f64(&mut parts, "--l1qsm-phase-scale", l.phase_scale, dl.phase_scale);
        }
        QsmAlgorithm::Whqsm => {
            let w = &config.inversion.whqsm;
            let dw = &d.inversion.whqsm;
            emit_f64(&mut parts, "--whqsm-alpha1", w.alpha1, dw.alpha1);
            emit_f64(&mut parts, "--whqsm-mu1", w.mu1, dw.mu1);
            emit_f64(&mut parts, "--whqsm-mu2", w.mu2, dw.mu2);
            emit_f64(&mut parts, "--whqsm-beta", w.beta, dw.beta);
            emit_f64(&mut parts, "--whqsm-muh", w.muh, dw.muh);
            emit_usize(&mut parts, "--whqsm-max-iter", w.max_iter, dw.max_iter);
            emit_f64(&mut parts, "--whqsm-tol-update", w.tol_update, dw.tol_update);
            emit_f64(&mut parts, "--whqsm-tol-delta", w.tol_delta, dw.tol_delta);
            emit_f64(&mut parts, "--whqsm-phase-scale", w.phase_scale, dw.phase_scale);
        }
        QsmAlgorithm::Hdqsm => {
            let h = &config.inversion.hdqsm;
            let dh = &d.inversion.hdqsm;
            emit_f64(&mut parts, "--hdqsm-alpha-l2", h.alpha_l2, dh.alpha_l2);
            emit_f64(&mut parts, "--hdqsm-mu1-l2", h.mu1_l2, dh.mu1_l2);
            emit_f64(&mut parts, "--hdqsm-mu2", h.mu2, dh.mu2);
            emit_usize(&mut parts, "--hdqsm-max-iter-l1", h.max_iter_l1, dh.max_iter_l1);
            emit_usize(&mut parts, "--hdqsm-max-iter-l2", h.max_iter_l2, dh.max_iter_l2);
            emit_f64(&mut parts, "--hdqsm-tol-update", h.tol_update, dh.tol_update);
        }
        QsmAlgorithm::AmpPe => {
            let a = &config.inversion.amp_pe;
            let da = &d.inversion.amp_pe;
            // b0 (scan) and gyro_ratio (physical constant) are not run flags.
            emit_usize(&mut parts, "--amp-pe-wave-order", a.wave_order, da.wave_order);
            emit_usize(&mut parts, "--amp-pe-nlevel", a.nlevel, da.nlevel);
            emit_f64(&mut parts, "--amp-pe-wave-pec", a.wave_pec, da.wave_pec);
            emit_f64(&mut parts, "--amp-pe-simulated-te", a.simulated_te, da.simulated_te);
            emit_usize(&mut parts, "--amp-pe-max-linearization-ite", a.max_linearization_ite, da.max_linearization_ite);
            emit_f64(&mut parts, "--amp-pe-damp-rate-sig", a.damp_rate_sig, da.damp_rate_sig);
            emit_f64(&mut parts, "--amp-pe-damp-rate-par", a.damp_rate_par, da.damp_rate_par);
            emit_usize(&mut parts, "--amp-pe-max-pe-spar-ite", a.max_pe_spar_ite, da.max_pe_spar_ite);
            emit_usize(&mut parts, "--amp-pe-max-pe-est-ite", a.max_pe_est_ite, da.max_pe_est_ite);
            emit_f64(&mut parts, "--amp-pe-cvg-thd", a.cvg_thd, da.cvg_thd);
            emit_f64(&mut parts, "--amp-pe-tikhonov-beta", a.tikhonov_beta, da.tikhonov_beta);
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
            // SUSEP-Net / χ-sepnet (deep learning) have no user-tunable parameters.
            SeparationAlgorithm::SusepNet | SeparationAlgorithm::ChiSepNet => {}
        }
    }

    // ── QSM reference ──
    emit_enum(&mut parts, "--qsm-reference", &config.qsm.reference, &d.qsm.reference);

    // ── SWI params ──
    if config.pipeline.do_swi {
        emit_f64_arr3(&mut parts, "--swi-hp-sigma", &config.swi.hp_sigma, &d.swi.hp_sigma);
        if config.swi.scaling != d.swi.scaling { parts.push(format!("--swi-scaling {}", config.swi.scaling)); }
        emit_f64(&mut parts, "--swi-strength", config.swi.strength, d.swi.strength);
        emit_usize(&mut parts, "--swi-mip-window", config.swi.mip_window, d.swi.mip_window);
    }

    parts.join(" ")
}

// ─── Helpers ───

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
    use super::*;

    #[test]
    fn test_default_config_minimal_command() {
        let config = PipelineConfig::default();
        let cmd = generate_command(&config);
        assert_eq!(cmd, "qsmxt run <bids_dir>");
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
        // iQFM background removal + χ-sepnet separation (deep-learning, no params).
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
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = PipelineConfig::default();
        let toml = config.to_toml().unwrap();
        let parsed = PipelineConfig::from_toml(&toml).unwrap();
        assert_eq!(parsed.inversion.algorithm, QsmAlgorithm::Rts);
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
            (QsmAlgorithm::Ilsqr, "ilsqr"),
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
ilsqr_tol = 0.01
ilsqr_max_iter = 50
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
    }

    #[test]
    fn test_json_roundtrip() {
        let config = PipelineConfig::default();
        let json = config.to_json().unwrap();
        let parsed: PipelineConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.inversion.algorithm, QsmAlgorithm::Rts);
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
        assert_eq!(config.inversion.algorithm, QsmAlgorithm::Rts);
        assert!(config.pipeline.do_qsm);
    }

    #[test]
    fn test_phase_offset_sigma() {
        let mut c = PipelineConfig::default();
        c.field_mapping.phase_offset_sigma = [10.0, 10.0, 5.0];
        let cmd = generate_command(&c);
        assert!(cmd.contains("--phase-offset-sigma 10 10 5"));
    }
}
