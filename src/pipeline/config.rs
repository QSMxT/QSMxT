//! Pipeline configuration — re-exports from qsmxt-config library
//! plus qsmxt.rs-specific extensions (file I/O, CLI override mapping).

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
        cli::QsmAlgorithmArg::Ilsqr => QsmAlgorithm::Ilsqr,
        cli::QsmAlgorithmArg::Qsmart => QsmAlgorithm::Qsmart,
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
        if let Some(v) = args.tgv_params.tgv_iterations { config.inversion.tgv.iterations = v; }
        if let Some(v) = args.tgv_params.tgv_erosions { config.inversion.tgv.erosions = v; }
        if let Some(v) = args.tgv_params.tgv_alpha1 { config.inversion.tgv.alpha1 = v; }
        if let Some(v) = args.tgv_params.tgv_alpha0 { config.inversion.tgv.alpha0 = v; }
        if let Some(v) = args.tgv_params.tgv_step_size { config.inversion.tgv.step_size = v; }
        if let Some(v) = args.tgv_params.tgv_tol { config.inversion.tgv.tol = v; }
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
        assert!(cli::Cli::try_parse_from(&[
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
        assert!(cli::Cli::try_parse_from(&[
            "qsmxt", "run", "<bids>", "--mask-erosions", "3",
        ]).is_err());
    }
}
