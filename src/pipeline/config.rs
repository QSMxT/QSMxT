//! Pipeline configuration — re-exports from qsmxt-config library
//! plus qsmxt-specific extensions (file I/O, CLI override mapping).

use std::path::Path;
use crate::cli;
use crate::error::QsmxtError;

// Re-export everything from the library
pub use qsmxt_config::*;

/// Load config from a TOML file.
pub fn load_config(path: &Path) -> crate::Result<PipelineConfig> {
    let text = std::fs::read_to_string(path)?;
    PipelineConfig::from_toml(&text).map_err(|e| QsmxtError::Config(format!("TOML parse error: {}", e)))
}

/// Apply CLI overrides onto a config.
/// Map a CLI dipole-inversion algorithm argument to the config enum.
fn qsm_algorithm_arg_to_config(a: cli::QsmAlgorithmArg) -> QsmAlgorithm {
    match a {
        cli::QsmAlgorithmArg::Rts => QsmAlgorithm::Rts,
        cli::QsmAlgorithmArg::Tv => QsmAlgorithm::Tv,
        cli::QsmAlgorithmArg::Tkd => QsmAlgorithm::Tkd,
        cli::QsmAlgorithmArg::Tgv => QsmAlgorithm::Tgv,
        cli::QsmAlgorithmArg::Tikhonov => QsmAlgorithm::Tikhonov,
        cli::QsmAlgorithmArg::Nltv => QsmAlgorithm::Nltv,
        cli::QsmAlgorithmArg::Tsvd => QsmAlgorithm::Tsvd,
        cli::QsmAlgorithmArg::Medi => QsmAlgorithm::Medi,
        cli::QsmAlgorithmArg::Tfi => QsmAlgorithm::Tfi,
        cli::QsmAlgorithmArg::Ilsqr => QsmAlgorithm::Ilsqr,
        cli::QsmAlgorithmArg::Qsmart => QsmAlgorithm::Qsmart,
        cli::QsmAlgorithmArg::Ndi => QsmAlgorithm::Ndi,
        cli::QsmAlgorithmArg::Fansi => QsmAlgorithm::Fansi,
        cli::QsmAlgorithmArg::FansiTgv => QsmAlgorithm::FansiTgv,
        cli::QsmAlgorithmArg::L1qsm => QsmAlgorithm::L1qsm,
        cli::QsmAlgorithmArg::Whqsm => QsmAlgorithm::Whqsm,
        cli::QsmAlgorithmArg::Hdqsm => QsmAlgorithm::Hdqsm,
        cli::QsmAlgorithmArg::AmpPe => QsmAlgorithm::AmpPe,
        cli::QsmAlgorithmArg::Xqsm => QsmAlgorithm::Xqsm,
        cli::QsmAlgorithmArg::Qsmnet => QsmAlgorithm::Qsmnet,
        cli::QsmAlgorithmArg::QsmnetPlus => QsmAlgorithm::QsmnetPlus,
        cli::QsmAlgorithmArg::Autoqsm => QsmAlgorithm::Autoqsm,
        cli::QsmAlgorithmArg::Qsmgan => QsmAlgorithm::Qsmgan,
        cli::QsmAlgorithmArg::Ir2qsm => QsmAlgorithm::Ir2qsm,
        cli::QsmAlgorithmArg::Lpcnn => QsmAlgorithm::Lpcnn,
        cli::QsmAlgorithmArg::ModlQsm => QsmAlgorithm::ModlQsm,
        cli::QsmAlgorithmArg::Nextqsm => QsmAlgorithm::Nextqsm,
        cli::QsmAlgorithmArg::Iqsm => QsmAlgorithm::Iqsm,
        cli::QsmAlgorithmArg::IqsmPlus => QsmAlgorithm::IqsmPlus,
    }
}

fn separation_algorithm_arg_to_config(a: cli::SeparationAlgorithmArg) -> SeparationAlgorithm {
    match a {
        cli::SeparationAlgorithmArg::R2starQsm => SeparationAlgorithm::R2starQsm,
        cli::SeparationAlgorithmArg::Decompose => SeparationAlgorithm::Decompose,
        cli::SeparationAlgorithmArg::ChiSepIlsqr => SeparationAlgorithm::ChiSepIlsqr,
        cli::SeparationAlgorithmArg::ChiSepMedi => SeparationAlgorithm::ChiSepMedi,
        cli::SeparationAlgorithmArg::Wavesep => SeparationAlgorithm::WaveSep,
        cli::SeparationAlgorithmArg::HcChisep => SeparationAlgorithm::HcChisep,
        cli::SeparationAlgorithmArg::SusepNet => SeparationAlgorithm::SusepNet,
        cli::SeparationAlgorithmArg::ChiSepNet => SeparationAlgorithm::ChiSepNet,
    }
}

/// Apply QSMART parameter overrides onto a config. Shared by the `run` pipeline and the standalone
/// `qsmxt qsmart` command so both honour the same flags.
pub fn apply_qsmart_overrides(config: &mut PipelineConfig, p: &cli::QsmartParamArgs) {
    if let Some(v) = p.qsmart_ilsqr_tol { config.inversion.qsmart.ilsqr_tol = v; }
    if let Some(v) = p.qsmart_ilsqr_max_iter { config.inversion.qsmart.ilsqr_max_iter = v; }
    if let Some(v) = p.qsmart_vasc_sphere_radius { config.inversion.qsmart.vasc_sphere_radius = v; }
    if let Some(v) = p.qsmart_sdf_spatial_radius { config.inversion.qsmart.sdf_spatial_radius = v; }
    if let Some(a) = p.qsmart_inversion { config.inversion.qsmart.inversion = qsm_algorithm_arg_to_config(a); }
    if let Some(v) = p.qsmart_sdf_sigma1_stage1 { config.inversion.qsmart.sdf_sigma1_stage1 = v; }
    if let Some(v) = p.qsmart_sdf_sigma2_stage1 { config.inversion.qsmart.sdf_sigma2_stage1 = v; }
    if let Some(v) = p.qsmart_sdf_sigma1_stage2 { config.inversion.qsmart.sdf_sigma1_stage2 = v; }
    if let Some(v) = p.qsmart_sdf_sigma2_stage2 { config.inversion.qsmart.sdf_sigma2_stage2 = v; }
    if let Some(v) = p.qsmart_sdf_lower_lim { config.inversion.qsmart.sdf_lower_lim = v; }
    if let Some(v) = p.qsmart_sdf_curv_constant { config.inversion.qsmart.sdf_curv_constant = v; }
    if let Some(v) = p.qsmart_frangi_scale_min { config.inversion.qsmart.frangi_scale_min = v; }
    if let Some(v) = p.qsmart_frangi_scale_max { config.inversion.qsmart.frangi_scale_max = v; }
    if let Some(v) = p.qsmart_frangi_scale_ratio { config.inversion.qsmart.frangi_scale_ratio = v; }
    if let Some(v) = p.qsmart_frangi_c { config.inversion.qsmart.frangi_c = v; }
}

/// Maps flat CLI flags to nested config fields.
pub fn apply_run_overrides(config: &mut PipelineConfig, args: &cli::RunArgs) {
        // ── Inversion algorithm ──
        if let Some(a) = args.qsm_algorithm {
            config.inversion.algorithm = qsm_algorithm_arg_to_config(a);
        }

        // ── Unwrapping ──
        if let Some(a) = args.unwrapping_algorithm {
            config.field_mapping.unwrapping_algorithm = match a {
                cli::UnwrapAlgorithmArg::Romeo => UnwrappingAlgorithm::Romeo,
                cli::UnwrapAlgorithmArg::Laplacian => UnwrappingAlgorithm::Laplacian,
            };
        }

        // ── Background removal ──
        if let Some(a) = args.bf_algorithm {
            config.bg_removal.algorithm = match a {
                cli::BfAlgorithmArg::Vsharp => BfAlgorithm::Vsharp,
                cli::BfAlgorithmArg::Pdf => BfAlgorithm::Pdf,
                cli::BfAlgorithmArg::Lbv => BfAlgorithm::Lbv,
                cli::BfAlgorithmArg::Ismv => BfAlgorithm::Ismv,
                cli::BfAlgorithmArg::Sharp => BfAlgorithm::Sharp,
                cli::BfAlgorithmArg::Resharp => BfAlgorithm::Resharp,
                cli::BfAlgorithmArg::Harperella => BfAlgorithm::Harperella,
                cli::BfAlgorithmArg::Iharperella => BfAlgorithm::Iharperella,
                cli::BfAlgorithmArg::Bfrnet => BfAlgorithm::Bfrnet,
                cli::BfAlgorithmArg::Iqfm => BfAlgorithm::Iqfm,
            };
        }

        // ── Field mapping ──
        if let Some(v) = args.phase_offset_removal { config.field_mapping.phase_offset_removal = v; }
        if args.bipolar_correction { config.field_mapping.bipolar_correction = true; }
        if args.romeo_individual { config.field_mapping.romeo.individual = true; }
        if args.no_romeo_individual { config.field_mapping.romeo.individual = false; }
        if args.no_romeo_correct_global { config.field_mapping.romeo.correct_global = false; }
        if let Some(t) = args.romeo_template {
            config.field_mapping.romeo.template = if t > 0 { t - 1 } else { 0 };
        }
        if let Some(a) = args.b0_estimation {
            config.field_mapping.b0_estimation = match a {
                cli::B0EstimationArg::WeightedAvg => B0Estimation::WeightedAvg,
                cli::B0EstimationArg::LinearFit => B0Estimation::LinearFit,
            };
        }
        if let Some(a) = args.b0_weight_type {
            config.field_mapping.b0_weight_type = match a {
                cli::B0WeightTypeArg::PhaseSNR => B0WeightType::PhaseSNR,
                cli::B0WeightTypeArg::PhaseVar => B0WeightType::PhaseVar,
                cli::B0WeightTypeArg::Average => B0WeightType::Average,
                cli::B0WeightTypeArg::TEs => B0WeightType::TEs,
                cli::B0WeightTypeArg::Mag => B0WeightType::Mag,
            };
        }
        if let Some(ref s) = args.phase_offset_sigma {
            if s.len() == 3 { config.field_mapping.phase_offset_sigma = [s[0], s[1], s[2]]; }
        }

        // ── ROMEO weights ──
        if args.romeo_params.no_romeo_phase_gradient_coherence { config.field_mapping.romeo.phase_gradient_coherence = false; }
        if args.romeo_params.no_romeo_mag_coherence { config.field_mapping.romeo.mag_coherence = false; }
        if args.romeo_params.no_romeo_mag_weight { config.field_mapping.romeo.mag_weight = false; }

        // ── QSM reference ──
        if let Some(a) = args.qsm_reference {
            config.qsm.reference = match a {
                cli::QsmReferenceArg::Mean => QsmReference::Mean,
                cli::QsmReferenceArg::None => QsmReference::None,
            };
        }

        // ── BET ──
        if let Some(v) = args.bet_fractional_intensity { config.bet.fractional_intensity = v; }
        if let Some(v) = args.bet_smoothness { config.bet.smoothness = v; }
        if let Some(v) = args.bet_gradient_threshold { config.bet.gradient_threshold = v; }
        if let Some(v) = args.bet_iterations { config.bet.iterations = v; }
        if let Some(v) = args.bet_subdivisions { config.bet.subdivisions = v; }

        // ── Inversion params ──
        if let Some(v) = args.rts_params.rts_delta { config.inversion.rts.delta = v; }
        if let Some(v) = args.rts_params.rts_mu { config.inversion.rts.mu = v; }
        if let Some(v) = args.rts_params.rts_tol { config.inversion.rts.tol = v; }
        if let Some(v) = args.rts_params.rts_rho { config.inversion.rts.rho = v; }
        if let Some(v) = args.rts_params.rts_max_iter { config.inversion.rts.max_iter = v; }
        if let Some(v) = args.rts_params.rts_lsmr_iter { config.inversion.rts.lsmr_iter = v; }
        if let Some(v) = args.tv_params.tv_lambda { config.inversion.tv.lambda = v; }
        if let Some(v) = args.tv_params.tv_rho { config.inversion.tv.rho = v; }
        if let Some(v) = args.tv_params.tv_tol { config.inversion.tv.tol = v; }
        if let Some(v) = args.tv_params.tv_max_iter { config.inversion.tv.max_iter = v; }
        if let Some(v) = args.tkd_params.tkd_threshold { config.inversion.tkd.threshold = v; }
        if let Some(v) = args.tsvd_params.tsvd_threshold { config.inversion.tsvd.threshold = v; }
        if let Some(v) = args.ilsqr_params.ilsqr_tol { config.inversion.ilsqr.tol = v; }
        if let Some(v) = args.ilsqr_params.ilsqr_max_iter { config.inversion.ilsqr.max_iter = v; }
        if let Some(v) = args.tikhonov_params.tikhonov_lambda { config.inversion.tikhonov.lambda = v; }
        if let Some(v) = args.nltv_params.nltv_lambda { config.inversion.nltv.lambda = v; }
        if let Some(v) = args.nltv_params.nltv_mu { config.inversion.nltv.mu = v; }
        if let Some(v) = args.nltv_params.nltv_tol { config.inversion.nltv.tol = v; }
        if let Some(v) = args.nltv_params.nltv_max_iter { config.inversion.nltv.max_iter = v; }
        if let Some(v) = args.nltv_params.nltv_newton_iter { config.inversion.nltv.newton_iter = v; }
        if let Some(v) = args.medi_params.medi_lambda { config.inversion.medi.lambda = v; }
        if let Some(v) = args.medi_params.medi_max_iter { config.inversion.medi.max_iter = v; }
        if let Some(v) = args.medi_params.medi_cg_max_iter { config.inversion.medi.cg_max_iter = v; }
        if let Some(v) = args.medi_params.medi_cg_tol { config.inversion.medi.cg_tol = v; }
        if let Some(v) = args.medi_params.medi_tol { config.inversion.medi.tol = v; }
        if let Some(v) = args.medi_params.medi_percentage { config.inversion.medi.percentage = v; }
        if let Some(v) = args.medi_params.medi_smv_radius { config.inversion.medi.smv_radius = v; }
        if args.medi_params.medi_smv { config.inversion.medi.smv = true; }
        if let Some(v) = args.tfi_params.tfi_lambda { config.inversion.tfi.lambda = v; }
        if let Some(v) = args.tfi_params.tfi_precond { config.inversion.tfi.precond = v; }
        if let Some(v) = args.tfi_params.tfi_data_weighting { config.inversion.tfi.data_weighting = v; }
        if let Some(v) = args.tfi_params.tfi_percentage { config.inversion.tfi.percentage = v; }
        if let Some(v) = args.tfi_params.tfi_cg_tol { config.inversion.tfi.cg_tol = v; }
        if let Some(v) = args.tfi_params.tfi_cg_max_iter { config.inversion.tfi.cg_max_iter = v; }
        if let Some(v) = args.tfi_params.tfi_max_iter { config.inversion.tfi.max_iter = v; }
        if let Some(v) = args.tfi_params.tfi_tol { config.inversion.tfi.tol = v; }
        if let Some(v) = args.tfi_params.tfi_merit { config.inversion.tfi.merit = v; }
        if let Some(v) = args.tgv_params.tgv_iterations { config.inversion.tgv.iterations = v; }
        if let Some(v) = args.tgv_params.tgv_erosions { config.inversion.tgv.erosions = v; }
        if let Some(v) = args.tgv_params.tgv_alpha1 { config.inversion.tgv.alpha1 = v; }
        if let Some(v) = args.tgv_params.tgv_alpha0 { config.inversion.tgv.alpha0 = v; }
        if let Some(v) = args.tgv_params.tgv_step_size { config.inversion.tgv.step_size = v; }
        if let Some(v) = args.tgv_params.tgv_tol { config.inversion.tgv.tol = v; }
        // Deep-learning overlap-tiling (opt-in): presence of --tile-size enables it.
        if let Some(v) = args.tiling_params.tile_size { config.inversion.tile_size = Some(v); }
        if let Some(v) = args.tiling_params.tile_halo { config.inversion.tile_halo = Some(v); }
        if let Some(v) = args.ndi_params.ndi_tau { config.inversion.ndi.tau = v; }
        if let Some(v) = args.ndi_params.ndi_alpha { config.inversion.ndi.alpha = v; }
        if let Some(v) = args.ndi_params.ndi_max_iter { config.inversion.ndi.max_iter = v; }
        if let Some(v) = args.ndi_params.ndi_phase_scale { config.inversion.ndi.phase_scale = v; }
        if let Some(v) = args.fansi_params.fansi_alpha1 { config.inversion.fansi.alpha1 = v; }
        if let Some(v) = args.fansi_params.fansi_mu1 { config.inversion.fansi.mu1 = v; }
        if let Some(v) = args.fansi_params.fansi_mu2 { config.inversion.fansi.mu2 = v; }
        if let Some(v) = args.fansi_params.fansi_alpha0 { config.inversion.fansi.alpha0 = v; }
        if let Some(v) = args.fansi_params.fansi_mu0 { config.inversion.fansi.mu0 = v; }
        if let Some(v) = args.fansi_params.fansi_max_iter { config.inversion.fansi.max_iter = v; }
        if let Some(v) = args.fansi_params.fansi_tol_update { config.inversion.fansi.tol_update = v; }
        if let Some(v) = args.fansi_params.fansi_tol_delta { config.inversion.fansi.tol_delta = v; }
        if let Some(v) = args.fansi_params.fansi_phase_scale { config.inversion.fansi.phase_scale = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_alpha1 { config.inversion.l1qsm.alpha1 = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_mu1 { config.inversion.l1qsm.mu1 = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_mu2 { config.inversion.l1qsm.mu2 = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_mu3 { config.inversion.l1qsm.mu3 = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_lambda { config.inversion.l1qsm.lambda = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_max_iter { config.inversion.l1qsm.max_iter = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_tol_update { config.inversion.l1qsm.tol_update = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_tol_delta { config.inversion.l1qsm.tol_delta = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_phase_scale { config.inversion.l1qsm.phase_scale = v; }
        if let Some(v) = args.whqsm_params.whqsm_alpha1 { config.inversion.whqsm.alpha1 = v; }
        if let Some(v) = args.whqsm_params.whqsm_mu1 { config.inversion.whqsm.mu1 = v; }
        if let Some(v) = args.whqsm_params.whqsm_mu2 { config.inversion.whqsm.mu2 = v; }
        if let Some(v) = args.whqsm_params.whqsm_beta { config.inversion.whqsm.beta = v; }
        if let Some(v) = args.whqsm_params.whqsm_muh { config.inversion.whqsm.muh = v; }
        if let Some(v) = args.whqsm_params.whqsm_max_iter { config.inversion.whqsm.max_iter = v; }
        if let Some(v) = args.whqsm_params.whqsm_tol_update { config.inversion.whqsm.tol_update = v; }
        if let Some(v) = args.whqsm_params.whqsm_tol_delta { config.inversion.whqsm.tol_delta = v; }
        if let Some(v) = args.whqsm_params.whqsm_phase_scale { config.inversion.whqsm.phase_scale = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_alpha_l2 { config.inversion.hdqsm.alpha_l2 = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_mu1_l2 { config.inversion.hdqsm.mu1_l2 = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_mu2 { config.inversion.hdqsm.mu2 = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_max_iter_l1 { config.inversion.hdqsm.max_iter_l1 = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_max_iter_l2 { config.inversion.hdqsm.max_iter_l2 = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_tol_update { config.inversion.hdqsm.tol_update = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_wave_order { config.inversion.amp_pe.wave_order = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_nlevel { config.inversion.amp_pe.nlevel = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_wave_pec { config.inversion.amp_pe.wave_pec = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_simulated_te { config.inversion.amp_pe.simulated_te = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_max_linearization_ite { config.inversion.amp_pe.max_linearization_ite = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_damp_rate_sig { config.inversion.amp_pe.damp_rate_sig = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_damp_rate_par { config.inversion.amp_pe.damp_rate_par = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_max_pe_spar_ite { config.inversion.amp_pe.max_pe_spar_ite = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_max_pe_est_ite { config.inversion.amp_pe.max_pe_est_ite = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_cvg_thd { config.inversion.amp_pe.cvg_thd = v; }
        if let Some(v) = args.amp_pe_params.amp_pe_tikhonov_beta { config.inversion.amp_pe.tikhonov_beta = v; }
        apply_qsmart_overrides(config, &args.qsmart_params);

        // ── Background removal params ──
        if let Some(v) = args.vsharp_params.vsharp_threshold { config.bg_removal.vsharp.threshold = v; }
        if let Some(v) = args.vsharp_params.vsharp_max_radius { config.bg_removal.vsharp.max_radius = v; }
        if let Some(v) = args.vsharp_params.vsharp_min_radius { config.bg_removal.vsharp.min_radius = v; }
        if let Some(v) = args.pdf_params.pdf_tol { config.bg_removal.pdf.tol = v; }
        if let Some(v) = args.lbv_params.lbv_tol { config.bg_removal.lbv.tol = v; }
        if let Some(v) = args.ismv_params.ismv_tol { config.bg_removal.ismv.tol = v; }
        if let Some(v) = args.ismv_params.ismv_max_iter { config.bg_removal.ismv.max_iter = v; }
        if let Some(v) = args.ismv_params.ismv_radius { config.bg_removal.ismv.radius = v; }
        if let Some(v) = args.sharp_params.sharp_threshold { config.bg_removal.sharp.threshold = v; }
        if let Some(v) = args.sharp_params.sharp_radius { config.bg_removal.sharp.radius = v; }
        if let Some(v) = args.resharp_params.resharp_radius { config.bg_removal.resharp.radius = v; }
        if let Some(v) = args.resharp_params.resharp_tik_reg { config.bg_removal.resharp.tik_reg = v; }
        if let Some(v) = args.resharp_params.resharp_tol { config.bg_removal.resharp.tol = v; }
        if let Some(v) = args.resharp_params.resharp_max_iter { config.bg_removal.resharp.max_iter = v; }
        if let Some(v) = args.harperella_params.harperella_radius { config.bg_removal.harperella.radius = v; }
        if let Some(v) = args.harperella_params.harperella_max_iter { config.bg_removal.harperella.max_iter = v; }
        if let Some(v) = args.harperella_params.harperella_tol { config.bg_removal.harperella.tol = v; }
        if let Some(v) = args.iharperella_params.iharperella_radius { config.bg_removal.iharperella.radius = v; }
        if let Some(v) = args.iharperella_params.iharperella_max_iter { config.bg_removal.iharperella.max_iter = v; }
        if let Some(v) = args.iharperella_params.iharperella_tol { config.bg_removal.iharperella.tol = v; }
        if args.msmv_params.msmv_refine { config.bg_removal.msmv_refine = true; }
        if let Some(v) = args.msmv_params.msmv_radius { config.bg_removal.msmv.radius = v; }
        if let Some(v) = args.msmv_params.msmv_maxk { config.bg_removal.msmv.maxk = v; }

        // ── SWI ──
        if let Some(ref s) = args.swi_params.swi_hp_sigma {
            if s.len() == 3 { config.swi.hp_sigma = [s[0], s[1], s[2]]; }
        }
        if let Some(ref v) = args.swi_params.swi_scaling { config.swi.scaling = v.clone(); }
        if let Some(v) = args.swi_params.swi_strength { config.swi.strength = v; }
        if let Some(v) = args.swi_params.swi_mip_window { config.swi.mip_window = v; }

        // ── Homogeneity ──
        if let Some(v) = args.homogeneity_sigma_mm { config.homogeneity.sigma_mm = v; }
        if let Some(v) = args.homogeneity_nbox { config.homogeneity.nbox = v; }

        // ── Linear fit ──
        if let Some(v) = args.linear_fit_reliability_threshold {
            config.field_mapping.linear_fit.reliability_threshold_percentile = v;
        }

        // ── Pipeline toggles ──
        if args.no_qsm { config.pipeline.do_qsm = false; }
        if args.do_swi { config.pipeline.do_swi = true; }
        if args.do_t2starmap { config.pipeline.do_t2starmap = true; }
        if args.do_r2starmap { config.pipeline.do_r2starmap = true; }
        if args.do_r2map { config.pipeline.do_r2map = true; }
        if args.do_r2primemap { config.pipeline.do_r2primemap = true; }
        if args.do_chi_separation { config.pipeline.do_chi_separation = true; }
        if let Some(a) = args.chi_separation_algorithm {
            config.separation.algorithm = separation_algorithm_arg_to_config(a);
        }
        if let Some(t) = &args.use_custom_qsm { config.separation.custom_qsm_tool = Some(t.clone()); }
        if let Some(t) = &args.use_custom_r2 { config.separation.custom_r2_tool = Some(t.clone()); }
        if let Some(t) = &args.use_custom_r2prime { config.separation.custom_r2prime_tool = Some(t.clone()); }
        let sp = &args.separation_params;
        if let Some(v) = sp.r2star_qsm_r_const_3t { config.separation.r2star_qsm.r_const_3t = v; }
        if let Some(v) = sp.decompose_n_inner { config.separation.decompose.n_inner = v; }
        if let Some(v) = sp.decompose_chi_bound { config.separation.decompose.chi_bound = v; }
        if let Some(v) = sp.decompose_max_lm_iter { config.separation.decompose.max_lm_iter = v; }
        if let Some(v) = sp.chi_sep_ilsqr_dr_pos { config.separation.chi_sep_ilsqr.dr_pos = v; }
        if let Some(v) = sp.chi_sep_ilsqr_dr_neg { config.separation.chi_sep_ilsqr.dr_neg = v; }
        if let Some(v) = sp.chi_sep_ilsqr_lambda1 { config.separation.chi_sep_ilsqr.lambda1 = v; }
        if let Some(v) = sp.chi_sep_ilsqr_percentage { config.separation.chi_sep_ilsqr.percentage = v; }
        if let Some(v) = sp.chi_sep_ilsqr_r2p_min { config.separation.chi_sep_ilsqr.r2p_min = v; }
        if let Some(v) = sp.chi_sep_ilsqr_r2p_max { config.separation.chi_sep_ilsqr.r2p_max = v; }
        if let Some(v) = sp.chi_sep_ilsqr_max_iter { config.separation.chi_sep_ilsqr.max_iter = v; }
        if let Some(v) = sp.chi_sep_ilsqr_tol { config.separation.chi_sep_ilsqr.tol = v; }
        if let Some(v) = sp.chi_sep_ilsqr_cg_max_iter { config.separation.chi_sep_ilsqr.cg_max_iter = v; }
        if let Some(v) = sp.chi_sep_ilsqr_cg_tol { config.separation.chi_sep_ilsqr.cg_tol = v; }
        if let Some(v) = sp.chi_sep_medi_lambda_para { config.separation.chi_sep_medi.lambda_para = v; }
        if let Some(v) = sp.chi_sep_medi_lambda_dia { config.separation.chi_sep_medi.lambda_dia = v; }
        if let Some(v) = sp.chi_sep_medi_lambda_cpl { config.separation.chi_sep_medi.lambda_cpl = v; }
        if let Some(v) = sp.chi_sep_medi_dr_pos { config.separation.chi_sep_medi.dr_pos = v; }
        if let Some(v) = sp.chi_sep_medi_dr_neg { config.separation.chi_sep_medi.dr_neg = v; }
        if let Some(v) = sp.chi_sep_medi_percentage { config.separation.chi_sep_medi.percentage = v; }
        if let Some(v) = sp.chi_sep_medi_cg_tol { config.separation.chi_sep_medi.cg_tol = v; }
        if let Some(v) = sp.chi_sep_medi_cg_max_iter { config.separation.chi_sep_medi.cg_max_iter = v; }
        if let Some(v) = sp.chi_sep_medi_max_iter { config.separation.chi_sep_medi.max_iter = v; }
        if let Some(v) = sp.chi_sep_medi_tol { config.separation.chi_sep_medi.tol = v; }
        if let Some(v) = sp.wavesep_dr_pos { config.separation.wavesep.dr_pos = v; }
        if let Some(v) = sp.wavesep_dr_neg { config.separation.wavesep.dr_neg = v; }
        if let Some(v) = sp.wavesep_alpha { config.separation.wavesep.alpha = v; }
        if let Some(v) = sp.wavesep_lambda { config.separation.wavesep.lambda = v; }
        if let Some(v) = sp.wavesep_wavelet_order { config.separation.wavesep.wavelet_order = v; }
        if let Some(v) = sp.wavesep_max_iter { config.separation.wavesep.max_iter = v; }
        if let Some(v) = sp.wavesep_tol { config.separation.wavesep.tol = v; }
        if let Some(v) = sp.hc_chisep_dr_pos_3t { config.separation.hc_chisep.dr_pos_3t = v; }
        if let Some(v) = sp.hc_chisep_bin_hz { config.separation.hc_chisep.bin_hz = v; }
        // Chi-separation depends on R2*/R2/R2'; enabling it implies computing them
        // (a custom R2' or R2 map, when supplied, is used instead — see the runner).
        enforce_separation_dependencies(config);
        if args.export_dicom { config.pipeline.export_dicom = true; }
        if args.no_inhomogeneity_correction { config.masking.inhomogeneity_correction = false; }
        else if args.inhomogeneity_correction { config.masking.inhomogeneity_correction = true; }
        if let Some(tool) = &args.use_custom_masks { config.masking.custom_mask_tool = Some(tool.clone()); }
        if let Some(v) = args.obliquity_threshold { config.pipeline.obliquity_threshold = v; }

        // ── Mask sections ──
        if let Some(preset) = args.mask_preset {
            config.masking.sections = match preset {
                cli::MaskPresetArg::RobustThreshold => default_mask_sections(),
                cli::MaskPresetArg::Bet => vec![MaskSection {
                    input: MaskingInput::Magnitude,
                    generator: MaskOp::Bet { fractional_intensity: 0.5 },
                    refinements: vec![MaskOp::Erode { iterations: 2 }],
                }],
            };
        }
        if let Some(ref sections) = args.mask_sections_cli {
            let mut new_sections = Vec::new();
            for s in sections {
                let parts: Vec<&str> = s.split(',').collect();
                if parts.is_empty() { continue; }
                let input = match parse_masking_input(parts[0]) {
                    Some(i) => i,
                    None => { log::warn!("Ignoring invalid mask section input: '{}'", parts[0]); continue; }
                };
                let mut ops: Vec<MaskOp> = Vec::new();
                for part in &parts[1..] {
                    match parse_mask_op(part) {
                        Ok(op) => ops.push(op),
                        Err(e) => log::warn!("Ignoring invalid mask op '{}': {}", part, e),
                    }
                }
                let gen_idx = ops.iter().position(|op| matches!(op, MaskOp::Threshold { .. } | MaskOp::Bet { .. }));
                let generator = if let Some(gi) = gen_idx { ops.remove(gi) } else {
                    MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }
                };
                new_sections.push(MaskSection { input, generator, refinements: ops });
            }
            if !new_sections.is_empty() { config.masking.sections = new_sections; }
        }

        // ── Masking input / erosion overrides ──
        // These rewrite the configured sections (default or --mask-preset). Full
        // --mask sections already spell out their own input and refinements, so
        // the overrides don't apply there.
        if let Some(input) = args.masking_input {
            if args.mask_sections_cli.is_some() {
                log::warn!("--masking-input is ignored when --mask is given (each --mask section names its own input)");
            } else {
                let input = match input {
                    cli::MaskInputArg::MagnitudeFirst => MaskingInput::MagnitudeFirst,
                    cli::MaskInputArg::Magnitude => MaskingInput::Magnitude,
                    cli::MaskInputArg::MagnitudeLast => MaskingInput::MagnitudeLast,
                    cli::MaskInputArg::PhaseQuality => MaskingInput::PhaseQuality,
                };
                for section in &mut config.masking.sections { section.input = input; }
            }
        }

        // QSMART has no internal mask erosion (unlike V-SHARP), so a loose threshold
        // mask leaks non-brain phase into the global dipole inversion and produces
        // streaking. Default QSMART to a BET mask when masking is untouched; otherwise
        // warn if the user-chosen mask isn't BET-based.
        if config.inversion.algorithm == QsmAlgorithm::Qsmart {
            let untouched = args.mask_preset.is_none()
                && args.mask_sections_cli.is_none()
                && args.masking_input.is_none()
                && config.masking.sections == default_mask_sections();
            if untouched {
                log::info!("QSMART: defaulting to BET brain mask (override with --mask)");
                config.masking.sections = qsmart_default_mask_sections();
            } else if !config.masking.sections.iter().all(|s| matches!(s.generator, MaskOp::Bet { .. })) {
                log::warn!(
                    "QSMART needs a tight brain mask; the configured mask is not BET-based and \
                     may cause streaking artifacts. Consider --mask-preset bet or \
                     --mask magnitude,bet:0.5,erode:2."
                );
            }
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn dl_qsm_args_map_to_config() {
        use crate::cli::QsmAlgorithmArg as A;
        for a in [
            A::Xqsm, A::Qsmnet, A::QsmnetPlus, A::Autoqsm, A::Qsmgan, A::Ir2qsm,
            A::Lpcnn, A::ModlQsm, A::Nextqsm, A::Iqsm, A::IqsmPlus,
        ] {
            // Each DL arg maps to a distinct config algorithm whose Display is a non-empty id.
            let alg = qsm_algorithm_arg_to_config(a);
            assert!(!alg.to_string().is_empty());
        }
        // Separation + BG-removal DL args map too.
        assert_eq!(
            separation_algorithm_arg_to_config(crate::cli::SeparationAlgorithmArg::ChiSepNet),
            SeparationAlgorithm::ChiSepNet
        );
    }

    /// A generated QSMART command (all params) must parse back through the CLI and
    /// round-trip the QSMART config values via apply_run_overrides.
    #[test]
    fn qsmart_generated_command_roundtrips() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        config.inversion.qsmart.inversion = QsmAlgorithm::Tv;
        config.inversion.qsmart.sdf_sigma1_stage1 = 11.0;
        config.inversion.qsmart.sdf_lower_lim = 0.45;
        config.inversion.qsmart.frangi_scale_min = 1.5;
        config.inversion.qsmart.frangi_scale_max = 7.0;
        config.inversion.qsmart.frangi_scale_ratio = 1.0;
        config.inversion.qsmart.frangi_c = 333.0;

        let cmd = generate_command(&config);
        // Tokens after `qsmxt run <bids_dir>`.
        let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        let cli = cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("generated command did not parse: {}\ncmd: {}", e, cmd));

        let run_args = match cli.command {
            cli::Command::Run(a) => a,
            _ => panic!("expected Run subcommand"),
        };
        let mut rebuilt = PipelineConfig::default();
        apply_run_overrides(&mut rebuilt, &run_args);

        assert_eq!(rebuilt.inversion.algorithm, QsmAlgorithm::Qsmart);
        assert_eq!(rebuilt.inversion.qsmart.inversion, QsmAlgorithm::Tv);
        assert_eq!(rebuilt.inversion.qsmart.sdf_sigma1_stage1, 11.0);
        assert_eq!(rebuilt.inversion.qsmart.sdf_lower_lim, 0.45);
        assert_eq!(rebuilt.inversion.qsmart.frangi_scale_min, 1.5);
        assert_eq!(rebuilt.inversion.qsmart.frangi_scale_max, 7.0);
        assert_eq!(rebuilt.inversion.qsmart.frangi_scale_ratio, 1.0);
        assert_eq!(rebuilt.inversion.qsmart.frangi_c, 333.0);
    }

    fn config_from_cli(argv: &[&str]) -> PipelineConfig {
        let cli = cli::Cli::try_parse_from(argv).expect("parse");
        let run_args = match cli.command {
            cli::Command::Run(a) => a,
            _ => panic!("expected Run subcommand"),
        };
        let mut config = PipelineConfig::default();
        apply_run_overrides(&mut config, &run_args);
        config
    }

    #[test]
    fn qsmart_defaults_to_bet_mask() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart"]);
        assert_eq!(c.masking.sections, qsmart_default_mask_sections());
        assert_ne!(c.masking.sections, default_mask_sections());
    }

    #[test]
    fn qsmart_respects_explicit_mask() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart",
            "--mask", "phase-quality,threshold:otsu",
        ]);
        assert_ne!(c.masking.sections, qsmart_default_mask_sections());
    }

    #[test]
    fn qsmart_respects_mask_preset() {
        // robust-threshold preset is an explicit choice; don't override it with BET.
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart",
            "--mask-preset", "robust-threshold",
        ]);
        assert_ne!(c.masking.sections, qsmart_default_mask_sections());
        assert_eq!(c.masking.sections, default_mask_sections());
    }

    #[test]
    fn non_qsmart_keeps_default_mask() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "rts"]);
        assert_eq!(c.masking.sections, default_mask_sections());
    }

    #[test]
    fn masking_algorithm_flag_is_rejected() {
        // Removed in favour of --mask-preset / --mask; must be a parse error,
        // not silently ignored.
        assert!(cli::Cli::try_parse_from([
            "qsmxt", "run", "<bids>", "--masking-algorithm", "bet",
        ]).is_err());
    }

    #[test]
    fn masking_input_overrides_default_sections() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--masking-input", "magnitude"]);
        assert!(c.masking.sections.iter().all(|s| s.input == MaskingInput::Magnitude));
        // Generator and refinements keep the default recipe.
        let d = default_mask_sections();
        assert_eq!(c.masking.sections[0].generator, d[0].generator);
        assert_eq!(c.masking.sections[0].refinements, d[0].refinements);
    }

    #[test]
    fn masking_input_combines_with_mask_preset() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>",
            "--mask-preset", "bet",
            "--masking-input", "magnitude-first",
        ]);
        assert_eq!(c.masking.sections.len(), 1);
        assert_eq!(c.masking.sections[0].input, MaskingInput::MagnitudeFirst);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { .. }));
    }

    #[test]
    fn masking_input_ignored_with_explicit_mask_sections() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>",
            "--mask", "phase-quality,threshold:otsu",
            "--masking-input", "magnitude",
        ]);
        assert_eq!(c.masking.sections[0].input, MaskingInput::PhaseQuality);
    }

    #[test]
    fn mask_erosions_flag_is_rejected() {
        // Removed alongside --masking-algorithm; erosion belongs in --mask
        // sections (erode:N).
        assert!(cli::Cli::try_parse_from([
            "qsmxt", "run", "<bids>", "--mask-erosions", "3",
        ]).is_err());
    }
}
