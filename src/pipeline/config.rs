//! Pipeline configuration — re-exports from qsmxt-engine library
//! plus qsmxt-specific extensions (file I/O, CLI override mapping).

use std::path::Path;
use crate::cli;
use crate::error::QsmxtError;

// Re-export everything from the library
pub use qsmxt_engine::*;

/// Load config from a TOML file.
pub fn load_config(path: &Path) -> crate::Result<PipelineConfig> {
    let text = std::fs::read_to_string(path)?;
    let (config, migration) = PipelineConfig::from_toml_noting(&text)
        .map_err(|e| QsmxtError::Config(format!("TOML parse error: {}", e)))?;
    for w in migration.warnings {
        log::warn!("{}: {w}", path.display());
    }
    for n in migration.notes {
        log::info!("{}: {n}", path.display());
    }
    Ok(config)
}

/// The recipe behind a `--mask-preset` value — shared by `run` and `qsmxt mask preset`.
pub fn mask_preset_recipe(preset: cli::MaskPresetArg) -> crate::Result<MaskRecipe> {
    let name = match preset {
        cli::MaskPresetArg::RobustThreshold => "robust-threshold",
        cli::MaskPresetArg::Bet => "bet",
        cli::MaskPresetArg::HdBet => "hd-bet",
        cli::MaskPresetArg::BetAndPhase => "bet-and-phase",
        cli::MaskPresetArg::Rs2Net => "rs2-net",
        cli::MaskPresetArg::MindGrab => "mindgrab",
        cli::MaskPresetArg::MouseBet => return Err(QsmxtError::Config(
            "the `mouse-bet` mask preset has been removed. For rodent data use `--mask-preset \
             rs2-net`; BET on scaled voxel sizes is still available as a mask step, e.g. \
             `--mask magnitude,bet:0.5:scale=10,erode:1`".to_string())),
    };
    Ok(mask_presets().into_iter().find(|(n, _)| *n == name)
        .map(|(_, r)| r)
        .expect("every MaskPresetArg is in mask_presets()"))
}

/// Parse `<input>,<op>,<op>,...` mask-section specs — the `--mask` and `--two-pass-mask` grammar.
///
/// An invalid piece is an error: the flag was asked for, and running with a different mask than
/// the one spelled out is worse than stopping. `flag` names the option in the error. A section
/// with no generator op gets an Otsu threshold, which is what its input implies.
fn parse_mask_sections(specs: &[String], flag: &str) -> crate::Result<Vec<MaskSection>> {
    let mut sections = Vec::new();
    for spec in specs {
        let parts: Vec<&str> = spec.split(',').collect();
        let input = parse_masking_input(parts[0]).ok_or_else(|| QsmxtError::Config(format!(
            "{flag} {spec}: '{}' is not a mask input", parts[0])))?;
        let mut ops: Vec<MaskOp> = Vec::new();
        for part in &parts[1..] {
            ops.push(parse_mask_op(part).map_err(|e| QsmxtError::Config(format!(
                "{flag} {spec}: invalid mask op '{part}': {e}")))?);
        }
        let generators = ops.iter().filter(|op| op.is_generator()).count();
        if generators > 1 {
            return Err(QsmxtError::Config(format!(
                "{flag} {spec}: a section makes one mask, so it takes one generator (threshold, \
                 bet, hd-bet, ...); give each its own {flag} section")));
        }
        let gen_idx = ops.iter().position(MaskOp::is_generator);
        let generator = if let Some(gi) = gen_idx { ops.remove(gi) } else {
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }
        };
        sections.push(MaskSection { input, generator, refinements: ops });
    }
    Ok(sections)
}

/// Whether a mask op closes the holes the reliable pass is built around.
///
/// Hole-filling is the obvious one; `close` bridges a hole's rim across it, and a Gaussian smooth
/// followed by re-thresholding swallows small ones. Any of them turns the reliable mask back into
/// the ordinary one.
fn closes_holes(op: &MaskOp) -> bool {
    matches!(op, MaskOp::FillHoles { .. } | MaskOp::Close { .. } | MaskOp::GaussianSmooth { .. })
}

/// Warnings for a reliable-phase mask whose refinements defeat the method.
///
/// Returned rather than logged so the rule can be tested: a `--two-pass-mask` that fills its own
/// holes still runs, and still costs a second reconstruction, but reproduces the first one.
fn hole_closing_warnings(sections: &[MaskSection]) -> Vec<String> {
    sections.iter()
        .filter(|s| s.refinements.iter().any(closes_holes))
        .map(|s| format!(
            "--two-pass-mask {}: this closes the holes two-pass reconstructs around, which \
             collapses the reliable pass onto the ordinary one",
            s.compact_spec(),
        ))
        .collect()
}

/// Map a CLI SWI unwrapping argument to the config enum (shared by `run` and `qsmxt swi`).
pub fn swi_unwrap_arg_to_config(a: cli::SwiUnwrapArg) -> qsmxt_engine::SwiUnwrap {
    match a {
        cli::SwiUnwrapArg::Laplacian => qsmxt_engine::SwiUnwrap::Laplacian,
        cli::SwiUnwrapArg::LaplacianSlice => qsmxt_engine::SwiUnwrap::LaplacianSlice,
        cli::SwiUnwrapArg::Romeo => qsmxt_engine::SwiUnwrap::Romeo,
    }
}

/// Map a CLI SWI scaling argument to the config enum — the one place `run` and `qsmxt swi` both
/// go through, so the two cannot disagree about what a value means.
pub fn swi_scaling_arg_to_config(a: cli::SwiScalingArg) -> qsmxt_engine::SwiScaling {
    use qsmxt_engine::SwiScaling as S;
    match a {
        cli::SwiScalingArg::Tanh => S::Tanh,
        cli::SwiScalingArg::NegativeTanh => S::NegativeTanh,
        cli::SwiScalingArg::Positive => S::Positive,
        cli::SwiScalingArg::Negative => S::Negative,
        cli::SwiScalingArg::Triangular => S::Triangular,
    }
}

/// Map a CLI dipole-inversion algorithm argument to the config enum.
pub fn qsm_algorithm_arg_to_config(a: cli::QsmAlgorithmArg) -> QsmAlgorithm {
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
        cli::QsmAlgorithmArg::Lsqr => QsmAlgorithm::Lsqr,
        cli::QsmAlgorithmArg::Heidi => QsmAlgorithm::Heidi,
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

/// Apply the motion-correction flags onto a config.
pub fn apply_motion_overrides(config: &mut PipelineConfig, p: &cli::MotionParamArgs) -> crate::Result<()> {
    let m = &mut config.motion_correction;
    if p.motion_correction { m.enabled = true; }
    if let Some(ref r) = p.motion_reference {
        m.reference = r.parse()
            .map_err(|e| QsmxtError::Config(format!("--motion-reference {r}: {e}")))?;
    }
    if p.no_motion_chain { m.chain = false; }
    if let Some(v) = p.motion_chain_levels { m.chain_levels = v; }
    if let Some(v) = p.motion_min_ncc { m.min_ncc = v; }
    let r = &mut config.registration;
    if let Some(v) = p.registration_levels { r.levels = v; }
    if let Some(v) = p.registration_max_samples { r.max_samples = v; }
    if let Some(v) = p.registration_max_iterations { r.max_iterations = v; }
    if let Some(v) = p.registration_max_evaluations { r.max_evaluations = v; }
    if let Some(v) = p.registration_tolerance { r.tolerance = v; }
    if let Some(v) = p.registration_rotation_step_deg { r.rotation_step_deg = v; }
    if let Some(v) = p.registration_translation_step_mm { r.translation_step_mm = v; }
    if let Some(v) = p.registration_min_overlap { r.min_overlap = v; }
    Ok(())
}

/// Apply QSMART parameter overrides onto a config. Shared by the `run` pipeline and the standalone
/// `qsmxt qsmart` command so both honour the same flags.
pub fn apply_qsmart_overrides(config: &mut PipelineConfig, p: &cli::QsmartParamArgs) {
    if let Some(v) = p.qsmart_ilsqr_pad_mm { config.inversion.qsmart.ilsqr_pad_mm = v; }
    if let Some(v) = p.qsmart_ilsqr_max_iter { config.inversion.qsmart.ilsqr_max_iter = v; }
    if let Some(v) = p.qsmart_vasc_sphere_radius { config.inversion.qsmart.vasc_sphere_radius = v; }
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
///
/// Shared by `qsmxt run` and `qsmxt slurm` (both flatten `PipelineArgs`), so a
/// pipeline configured once resolves identically in either execution mode.
/// Split a `--qsm-reference` value into the method and, when it names one, the region spec.
///
/// `mean` and `none` are the two reserved words; everything else is a region, which is why they
/// are matched case-insensitively and nothing else is guessed at here. Whether the region exists
/// is [`validate_reference_region`]'s job — it needs the SynthSeg generation, which this function
/// is called too early to know.
pub fn parse_reference_spec(spec: &str) -> (QsmReference, Option<String>) {
    match spec.trim().to_lowercase().as_str() {
        "mean" => (QsmReference::Mean, None),
        "none" => (QsmReference::None, None),
        _ => (QsmReference::Region, Some(spec.trim().to_string())),
    }
}

/// Check that a region reference names labels the selected SynthSeg generation actually has.
///
/// Called once the config is complete, before any data is touched: a mistyped region is otherwise
/// a run that finishes hours later having referenced every map to something nobody asked for.
/// Whether SynthSeg `version`'s weights can be had: published in QSM.rs's model registry, or
/// supplied locally (`$QSM_MODEL_DIR`, or already in the model cache).
pub fn synthseg_weights_available(version: SynthSegVersion) -> crate::Result<()> {
    let id = version.weights_id();
    let spec = qsm_core::models::find_model(id).ok_or_else(|| QsmxtError::Config(format!(
        "SynthSeg {version}: QSM.rs has no model '{id}'")))?;
    if spec.is_available() || spec.files.iter().all(qsm_core::models::weight_file_present) {
        return Ok(());
    }
    Err(QsmxtError::Config(format!(
        "SynthSeg {version} is not available yet: its weights ({}) are not published in QSM.rs's \
         model registry. Use --synthseg-version v1, or put the exported file in $QSM_MODEL_DIR",
        spec.files.iter().map(|f| f.name).collect::<Vec<_>>().join(", "))))
}

/// Refuse a configuration that would run SynthSeg with weights that do not exist (#264): the run
/// would otherwise fail per subject, or — as it did — run v1's weights through v2's label table.
/// A supplied segmentation (`--use-custom-dseg`) needs no weights, only the version's label table.
pub fn validate_segmentation(config: &PipelineConfig) -> crate::Result<()> {
    let runs_synthseg = config.pipeline.do_segmentation && config.segmentation.custom_dseg_tool.is_none();
    if runs_synthseg {
        synthseg_weights_available(config.segmentation.version)
            .map_err(|e| QsmxtError::Config(format!("--synthseg-version {}: {e}", config.segmentation.version)))?;
    }
    Ok(())
}

pub fn validate_reference_region(config: &PipelineConfig) -> crate::Result<()> {
    if config.qsm.reference != QsmReference::Region {
        return Ok(());
    }
    let spec = config.qsm.reference_region.as_deref().unwrap_or_default();
    let ids = qsmxt_engine::regions::resolve(spec, config.segmentation.version)
        .map_err(|e| QsmxtError::Config(format!("--qsm-reference: {e}")))?;
    log::info!(
        "Referencing χ to `{spec}` (SynthSeg {} label{} {})",
        config.segmentation.version,
        if ids.len() == 1 { "" } else { "s" },
        ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "),
    );
    Ok(())
}

///
/// A value that cannot be parsed is an error, never a warning and a default: the flag was asked
/// for, and a run with some other setting would be worse than no run.
pub fn apply_run_overrides(config: &mut PipelineConfig, args: &cli::PipelineArgs) -> crate::Result<()> {
        // ── Inversion algorithm ──
        if let Some(a) = args.qsm_algorithm {
            config.inversion.algorithm = qsm_algorithm_arg_to_config(a);
        }

        // ── Multi-orientation (COSMOS / STI) ──
        // The pattern is what switches the feature on; the rest only matter once it is.
        if let Some(ref p) = args.orientation_group {
            config.multi_orientation.group_by = p.clone();
        }
        if let Some(a) = args.multi_orientation_algorithm {
            config.multi_orientation.algorithm = match a {
                cli::MultiOrientAlgorithmArg::Cosmos => MultiOrientAlgorithm::Cosmos,
                cli::MultiOrientAlgorithmArg::Sti => MultiOrientAlgorithm::Sti,
            };
        }
        if let Some(l) = args.multi_orientation_lambda {
            config.multi_orientation.lambda = l;
        }
        if args.multi_orientation_force {
            config.multi_orientation.force = true;
        }
        if args.no_orientation_registration {
            config.multi_orientation.register = false;
        }
        if args.cosmos_magnitude_weighting {
            config.multi_orientation.magnitude_weighting = true;
        }
        if let Some(v) = args.cosmos_tol { config.multi_orientation.cosmos_tol = v; }
        if let Some(v) = args.cosmos_max_iter { config.multi_orientation.cosmos_max_iter = v; }
        config.multi_orientation.validate().map_err(QsmxtError::Config)?;

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
        if let Some(a) = args.b0_estimation {
            config.field_mapping.b0_estimation = match a {
                cli::B0EstimationArg::WeightedAvg => B0Estimation::WeightedAvg,
                cli::B0EstimationArg::LinearFit => B0Estimation::LinearFit,
                cli::B0EstimationArg::RobustFit => B0Estimation::RobustFit,
            };
        }
        if let Some(a) = args.b0_weight_type {
            config.field_mapping.b0_weight_type = match a {
                cli::B0WeightTypeArg::PhaseSNR => B0WeightType::PhaseSNR,
                cli::B0WeightTypeArg::PhaseVar => B0WeightType::PhaseVar,
                cli::B0WeightTypeArg::Average => B0WeightType::Average,
                cli::B0WeightTypeArg::TEs => B0WeightType::TEs,
                cli::B0WeightTypeArg::Mag => B0WeightType::Mag,
                cli::B0WeightTypeArg::AssumedDecay => B0WeightType::AssumedDecay,
            };
        }
        if let Some(v) = args.b0_weight_assumed_t2star { config.field_mapping.b0_weight_assumed_t2star_ms = v; }
        if let Some(k) = args.laplacian_solver {
            config.field_mapping.laplacian_solver = match k {
                cli::LaplacianSolverArg::Dct => LaplacianSolver::Dct,
                cli::LaplacianSolverArg::Fft => LaplacianSolver::Fft,
            };
        }
        if let Some(ref p) = args.laplacian_fft_pad {
            config.field_mapping.laplacian_fft_pad = qsmxt_engine::pad3_from(p).ok_or_else(|| {
                QsmxtError::Config(format!("--laplacian-fft-pad {p:?}: give one value or three (x y z)"))
            })?;
        }
        if let Some(ref s) = args.coil_combination_sigma {
            if s.len() == 3 { config.field_mapping.coil_combination_sigma = [s[0], s[1], s[2]]; }
        }
        if let Some(ref s) = args.phase_offset_sigma_mm {
            if s.len() == 3 { config.field_mapping.phase_offset_sigma_mm = [s[0], s[1], s[2]]; }
        }

        // ── ROMEO weights and unwrapping options ──
        args.romeo_params.apply_to(&mut config.field_mapping.romeo);

        // ── QSM reference ──
        // Anything that is not `mean` or `none` is a region spec; whether it names real labels
        // depends on the SynthSeg generation, which later arguments can still change, so it is
        // validated once the whole config exists (see `validate_reference_region`).
        if let Some(ref a) = args.qsm_reference {
            let (reference, region) = parse_reference_spec(a);
            config.qsm.reference = reference;
            config.qsm.reference_region = region;
        }

        // ── BET ──
        if let Some(v) = args.bet_fractional_intensity { config.bet.fractional_intensity = v; }
        if let Some(v) = args.bet_smoothness { config.bet.smoothness = v; }
        if let Some(v) = args.bet_gradient_threshold { config.bet.gradient_threshold = v; }
        if let Some(v) = args.bet_iterations { config.bet.iterations = v; }
        if let Some(v) = args.bet_subdivisions { config.bet.subdivisions = v; }
        if let Some(v) = args.bet_voxel_scale { config.bet.voxel_scale = v; }

        // ── Inversion params ──
        if let Some(v) = args.rts_params.rts_delta { config.inversion.rts.delta = v; }
        if let Some(v) = args.rts_params.rts_mu { config.inversion.rts.mu = v; }
        if let Some(v) = args.rts_params.rts_tol { config.inversion.rts.tol = v; }
        if let Some(v) = args.rts_params.rts_rho { config.inversion.rts.rho = v; }
        if let Some(v) = args.rts_params.rts_max_iter { config.inversion.rts.max_iter = v; }
        if let Some(v) = args.rts_params.rts_lsmr_iter { config.inversion.rts.lsmr_iter = v; }
        if let Some(v) = args.rts_params.rts_tau { config.inversion.rts.tau = v; }
        if let Some(v) = args.rts_params.rts_gamma { config.inversion.rts.gamma = v; }
        if let Some(v) = args.tv_params.tv_lambda { config.inversion.tv.lambda = v; }
        if let Some(v) = args.tv_params.tv_rho { config.inversion.tv.rho = v.value(); }
        if let Some(v) = args.tv_params.tv_tol { config.inversion.tv.tol = v; }
        if let Some(v) = args.tv_params.tv_max_iter { config.inversion.tv.max_iter = v; }
        if let Some(v) = args.tkd_params.tkd_threshold { config.inversion.tkd.threshold = v; }
        if let Some(v) = args.tsvd_params.tsvd_threshold { config.inversion.tsvd.threshold = v; }
        if let Some(v) = args.ilsqr_params.ilsqr_tol { config.inversion.ilsqr.tol = v; }
        if let Some(v) = args.ilsqr_params.ilsqr_max_iter { config.inversion.ilsqr.max_iter = v; }
        if let Some(v) = args.ilsqr_params.ilsqr_pad_mm {
            qsm_core::inversion::IlsqrParams { pad_mm: v, ..Default::default() }
                .validate()
                .map_err(|e| QsmxtError::Config(format!("--ilsqr-pad-mm: {e}")))?;
            config.inversion.ilsqr.pad_mm = v;
        }
        // LSQR's params serve HEIDI too — its seed is an LSQR solve.
        if let Some(v) = args.lsqr_params.lsqr_residual_weighting { config.inversion.lsqr.residual_weighting = Some(v); }
        if args.lsqr_params.no_lsqr_global_offset { config.inversion.lsqr.fit_global_offset = false; }
        if let Some(v) = args.lsqr_params.lsqr_tol { config.inversion.lsqr.tol = v; }
        if let Some(v) = args.lsqr_params.lsqr_max_iter { config.inversion.lsqr.max_iter = v; }
        if let Some(v) = args.heidi_params.heidi_cone_threshold { config.inversion.heidi.cone_threshold = v; }
        if let Some(v) = args.heidi_params.heidi_gradient_threshold { config.inversion.heidi.gradient_threshold = v; }
        if args.heidi_params.no_heidi_laplacian_correction { config.inversion.heidi.apply_laplacian_correction = false; }
        if let Some(v) = args.heidi_params.heidi_laplacian_threshold { config.inversion.heidi.laplacian_threshold = v; }
        if let Some(v) = args.heidi_params.heidi_gradient_mask_floor { config.inversion.heidi.gradient_mask_floor = v; }
        if let Some(v) = args.heidi_params.heidi_continuation_steps { config.inversion.heidi.continuation_steps = v; }
        if let Some(v) = args.heidi_params.heidi_inner_iterations { config.inversion.heidi.inner_iterations = v; }
        if let Some(v) = args.heidi_params.heidi_mu_min { config.inversion.heidi.mu_min = v; }
        if let Some(v) = args.heidi_params.heidi_tol { config.inversion.heidi.tol = v; }
        if args.heidi_params.no_heidi_denoise { config.inversion.heidi.denoise = false; }
        if let Some(v) = args.heidi_params.heidi_denoise_iterations { config.inversion.heidi.denoise_iterations = v; }
        if let Some(v) = args.heidi_params.heidi_denoise_time_step { config.inversion.heidi.denoise_time_step = v; }
        if let Some(v) = args.heidi_params.heidi_denoise_conductance { config.inversion.heidi.denoise_conductance = v; }
        if let Some(v) = args.heidi_params.heidi_te { config.inversion.heidi.te = v; }
        if let Some(v) = args.tikhonov_params.tikhonov_lambda { config.inversion.tikhonov.lambda = v; }
        if let Some(v) = args.nltv_params.nltv_lambda { config.inversion.nltv.lambda = v; }
        if let Some(v) = args.nltv_params.nltv_rho { config.inversion.nltv.rho = v.value(); }
        if let Some(v) = args.nltv_params.nltv_mu { config.inversion.nltv.mu = v; }
        if let Some(v) = args.nltv_params.nltv_tol { config.inversion.nltv.tol = v; }
        if let Some(v) = args.nltv_params.nltv_max_iter { config.inversion.nltv.max_iter = v; }
        if let Some(v) = args.nltv_params.nltv_newton_iter { config.inversion.nltv.max_iter_newton = v; }
        if let Some(v) = args.nltv_params.nltv_tol_newton { config.inversion.nltv.tol_newton = v; }
        if let Some(v) = args.nltv_params.nltv_te { config.inversion.nltv.te = v; }
        if let Some(v) = args.medi_params.medi_lambda { config.inversion.medi.lambda = v; }
        if let Some(v) = args.medi_params.medi_max_iter { config.inversion.medi.max_iter = v; }
        if let Some(v) = args.medi_params.medi_cg_max_iter { config.inversion.medi.cg_max_iter = v; }
        if let Some(v) = args.medi_params.medi_cg_tol { config.inversion.medi.cg_tol = v; }
        if let Some(v) = args.medi_params.medi_tol { config.inversion.medi.tol = v; }
        if let Some(v) = args.medi_params.medi_te { config.inversion.medi.te = v; }
        if let Some(v) = args.medi_params.medi_percentage { config.inversion.medi.percentage = v; }
        if let Some(v) = args.medi_params.medi_smv_radius { config.inversion.medi.smv_radius = v; }
        if args.medi_params.medi_smv { config.inversion.medi.smv = true; }
        if args.medi_params.no_medi_smv { config.inversion.medi.smv = false; }
        if args.medi_params.medi_msmv { config.inversion.medi.msmv = true; }
        if let Some(v) = args.medi_params.medi_msmv_maxk { config.inversion.medi.msmv_maxk = v; }
        if let Some(v) = args.medi_params.medi_msmv_vessel_radius { config.inversion.medi.msmv_vessel_radius = Some(v); }
        if args.medi_params.no_medi_msmv_prefilter { config.inversion.medi.msmv_prefilter = false; }
        // Parsed but never applied before the TUI parity sweep: `--medi-merit` and
        // `--medi-data-weighting` did nothing.
        if let Some(v) = args.medi_params.medi_merit { config.inversion.medi.merit = v; }
        if let Some(v) = args.medi_params.medi_data_weighting { config.inversion.medi.data_weighting = v; }
        if let Some(v) = args.tfi_params.tfi_lambda { config.inversion.tfi.lambda = v; }
        if let Some(v) = args.tfi_params.tfi_precond { config.inversion.tfi.precond = v; }
        if let Some(v) = args.tfi_params.tfi_data_weighting { config.inversion.tfi.data_weighting = v; }
        if let Some(v) = args.tfi_params.tfi_percentage { config.inversion.tfi.percentage = v; }
        if let Some(v) = args.tfi_params.tfi_cg_tol { config.inversion.tfi.cg_tol = v; }
        if let Some(v) = args.tfi_params.tfi_cg_max_iter { config.inversion.tfi.cg_max_iter = v; }
        if let Some(v) = args.tfi_params.tfi_max_iter { config.inversion.tfi.max_iter = v; }
        if let Some(v) = args.tfi_params.tfi_tol { config.inversion.tfi.tol = v; }
        if let Some(v) = args.tfi_params.tfi_te { config.inversion.tfi.te = v; }
        if let Some(v) = args.tgv_params.tgv_iterations { config.inversion.tgv.iterations = Some(v); }
        if let Some(v) = args.tgv_params.tgv_erosions { config.inversion.tgv.erosions = v; }
        if let Some(v) = args.tgv_params.tgv_alpha1 { config.inversion.tgv.alpha1 = v; }
        if let Some(v) = args.tgv_params.tgv_alpha0 { config.inversion.tgv.alpha0 = v; }
        if let Some(v) = args.tgv_params.tgv_step_size { config.inversion.tgv.step_size = v; }
        if let Some(v) = args.tgv_params.tgv_te { config.inversion.tgv.te = v; }
        // Deep-learning overlap-tiling (opt-in): presence of --tile-size enables it.
        if let Some(v) = args.tiling_params.tile_size { config.inversion.tile_size = Some(v); }
        if let Some(v) = args.tiling_params.tile_halo { config.inversion.tile_halo = Some(v); }
        if let Some(v) = args.ndi_params.ndi_tau { config.inversion.ndi.tau = v; }
        if let Some(v) = args.ndi_params.ndi_alpha { config.inversion.ndi.alpha = v; }
        if let Some(v) = args.ndi_params.ndi_max_iter { config.inversion.ndi.max_iter = v; }
        if let Some(v) = args.ndi_params.ndi_auto_stop { config.inversion.ndi.auto_stop = v; }
        if let Some(v) = args.ndi_params.ndi_te { config.inversion.ndi.te = v; }
        if let Some(v) = args.fansi_params.fansi_alpha1 { config.inversion.fansi.alpha1 = v; }
        if let Some(v) = args.fansi_params.fansi_mu1 { config.inversion.fansi.mu1 = v.value(); }
        if let Some(v) = args.fansi_params.fansi_mu2 { config.inversion.fansi.mu2 = v; }
        if let Some(v) = args.fansi_params.fansi_alpha0 { config.inversion.fansi.alpha0 = v.value(); }
        if let Some(v) = args.fansi_params.fansi_mu0 { config.inversion.fansi.mu0 = v.value(); }
        if let Some(v) = args.fansi_params.fansi_max_iter { config.inversion.fansi.max_iter = v; }
        if let Some(v) = args.fansi_params.fansi_tol_update { config.inversion.fansi.tol_update = v; }
        if let Some(v) = args.fansi_params.fansi_tol_delta { config.inversion.fansi.tol_delta = v; }
        if let Some(v) = args.fansi_params.fansi_te { config.inversion.fansi.te = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_alpha1 { config.inversion.l1qsm.alpha1 = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_mu1 { config.inversion.l1qsm.mu1 = v.value(); }
        if let Some(v) = args.l1qsm_params.l1qsm_mu2 { config.inversion.l1qsm.mu2 = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_mu3 { config.inversion.l1qsm.mu3 = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_lambda { config.inversion.l1qsm.lambda = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_max_iter { config.inversion.l1qsm.max_iter = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_tol_update { config.inversion.l1qsm.tol_update = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_tol_delta { config.inversion.l1qsm.tol_delta = v; }
        if let Some(v) = args.l1qsm_params.l1qsm_te { config.inversion.l1qsm.te = v; }
        if let Some(v) = args.whqsm_params.whqsm_alpha1 { config.inversion.whqsm.alpha1 = v; }
        if let Some(v) = args.whqsm_params.whqsm_mu1 { config.inversion.whqsm.mu1 = v; }
        if let Some(v) = args.whqsm_params.whqsm_mu2 { config.inversion.whqsm.mu2 = v; }
        if let Some(v) = args.whqsm_params.whqsm_beta { config.inversion.whqsm.beta = v; }
        if let Some(v) = args.whqsm_params.whqsm_muh { config.inversion.whqsm.muh = v; }
        if let Some(v) = args.whqsm_params.whqsm_max_iter { config.inversion.whqsm.max_iter = v; }
        if let Some(v) = args.whqsm_params.whqsm_tol_update { config.inversion.whqsm.tol_update = v; }
        if let Some(v) = args.whqsm_params.whqsm_tol_delta { config.inversion.whqsm.tol_delta = v; }
        if let Some(v) = args.whqsm_params.whqsm_weight { config.inversion.whqsm.weight = v; }
        if let Some(v) = args.whqsm_params.whqsm_te { config.inversion.whqsm.te = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_alpha_l2 { config.inversion.hdqsm.alpha_l2 = v; }
        if let Some(v) = args.hdqsm_params.hdqsm_mu1_l2 { config.inversion.hdqsm.mu1_l2 = v.value(); }
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
        if let Some(v) = args.vsharp_params.vsharp_deconvolve { config.bg_removal.vsharp.deconvolve = v; }
        if let Some(v) = args.vsharp_params.vsharp_threshold { config.bg_removal.vsharp.threshold = v; }
        if let Some(v) = args.vsharp_params.vsharp_max_radius { config.bg_removal.vsharp.max_radius = v; }
        if let Some(v) = args.vsharp_params.vsharp_min_radius { config.bg_removal.vsharp.min_radius = v; }
        if let Some(v) = args.vsharp_params.vsharp_grid { config.bg_removal.vsharp.grid = v.into(); }
        if let Some(v) = args.vsharp_params.vsharp_grid_spacing { config.bg_removal.vsharp.grid_spacing = v; }
        if let Some(v) = args.pdf_params.pdf_tol { config.bg_removal.pdf.tol = v; }
        if let Some(v) = args.pdf_params.pdf_n_cg { config.bg_removal.pdf.n_cg = v; }
        if let Some(v) = args.pdf_params.pdf_space { config.bg_removal.pdf.space = v; }
        if let Some(v) = args.pdf_params.pdf_n_pad { config.bg_removal.pdf.n_pad = v; }
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
        if let Some(&[x, y, z]) = args.harperella_params.harperella_pad.as_deref() { config.bg_removal.harperella.pad = [x, y, z]; }
        if let Some(v) = args.iharperella_params.iharperella_max_iter { config.bg_removal.iharperella.max_iter = v; }
        if let Some(v) = args.iharperella_params.iharperella_tol { config.bg_removal.iharperella.tol = v; }
        if let Some(&[x, y, z]) = args.iharperella_params.iharperella_pad.as_deref() { config.bg_removal.iharperella.pad = [x, y, z]; }

        // ── SWI ──
        if let Some(ref s) = args.swi_params.swi_hp_sigma {
            if s.len() == 3 { config.swi.hp_sigma = [s[0], s[1], s[2]]; }
        }
        if let Some(v) = args.swi_params.swi_unwrapping { config.swi.unwrap = swi_unwrap_arg_to_config(v); }
        if let Some(v) = args.swi_params.swi_scaling { config.swi.scaling = swi_scaling_arg_to_config(v); }
        if let Some(v) = args.swi_params.swi_strength { config.swi.strength = v; }
        if let Some(v) = args.swi_params.swi_mip_window { config.swi.mip_window = v; }

        // ── Homogeneity ──
        if let Some(v) = args.homogeneity_sigma_mm { config.homogeneity.sigma_mm = v; }
        if let Some(v) = args.homogeneity_nbox { config.homogeneity.nbox = v; }

        // ── Linear fit ──
        if let Some(v) = args.linear_fit_reliability_threshold {
            config.field_mapping.linear_fit.reliability_threshold_percentile = v;
        }
        if let Some(l) = args.robust_fit_loss {
            config.field_mapping.robust_fit.loss = match l {
                cli::RobustLossArg::Tukey => RobustLoss::Tukey,
                cli::RobustLossArg::Huber => RobustLoss::Huber,
            };
        }
        if let Some(v) = args.robust_fit_tuning { config.field_mapping.robust_fit.tuning = Some(v); }
        if let Some(v) = args.robust_fit_iterations { config.field_mapping.robust_fit.iterations = v; }
        if let Some(v) = args.robust_fit_downweight_level { config.field_mapping.robust_fit.downweight_level = v; }
        if let Some(v) = args.robust_fit_flag_ratio { config.field_mapping.robust_fit.flag_ratio = v; }
        if let Some(v) = args.linear_fit_estimate_offset {
            config.field_mapping.linear_fit.estimate_offset = v;
        }

        // ── Pipeline toggles ──
        if args.no_qsm { config.pipeline.do_qsm = false; }
        if args.do_swi { config.pipeline.do_swi = true; }
        if args.do_smwi { config.pipeline.do_smwi = true; }
        if args.do_segmentation { config.pipeline.do_segmentation = true; }
        if args.do_analysis { config.pipeline.do_analysis = true; }
        let sg = &args.segmentation_params;
        if let Some(ref v) = sg.synthseg_version {
            config.segmentation.version = parse_synthseg_version(v).ok_or_else(|| {
                QsmxtError::Config(format!("--synthseg-version {v}: expected v1 or v2"))
            })?;
        }
        if let Some(v) = sg.synthseg_crop { config.segmentation.crop = Some(v); }
        if let Some(v) = sg.synthseg_patch { config.segmentation.patch = Some(v); }
        if sg.no_synthseg_flip_averaging { config.segmentation.flip_averaging = false; }
        if sg.no_synthseg_topology_cleanup { config.segmentation.topology_cleanup = false; }
        if let Some(v) = sg.synthseg_sigma { config.segmentation.sigma_smoothing = v; }
        if let Some(ref tool) = sg.use_custom_dseg { config.segmentation.custom_dseg_tool = Some(tool.clone()); }
        if let Some(v) = args.smwi_params.smwi_threshold { config.smwi.threshold_ppm = v; }
        if let Some(v) = args.smwi_params.smwi_power { config.smwi.power = v; }
        if let Some(v) = args.smwi_params.smwi_mip_window { config.smwi.mip_window = v; }
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
        if let Some(v) = args.r2prime_strategy {
            config.separation.r2prime_strategy = match v {
                cli::R2PrimeStrategyArg::Auto => R2PrimeStrategy::Auto,
                cli::R2PrimeStrategyArg::Mese => R2PrimeStrategy::Mese,
                cli::R2PrimeStrategyArg::R2primenet => R2PrimeStrategy::R2primenet,
            };
        }
        if args.r2primenet_ignore_field_mismatch { config.separation.r2primenet.ignore_field_mismatch = true; }
        if let Some(p) = &args.r2primenet_patch {
            if let [d, h, w] = p[..] { config.separation.r2primenet.patch = [d, h, w]; }
        }
        apply_motion_overrides(config, &args.motion_params)?;
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
        if let Some(&[d, h, w]) = sp.chi_sepnet_patch.as_deref() { config.separation.chi_sepnet.patch = [d, h, w]; }
        if let Some(&[d, h, w]) = sp.susep_net_patch.as_deref() { config.separation.susep_net.patch = Some([d, h, w]); }
        // ── Mask sections ──
        // Any of these is the user choosing the recipe, which the run then keeps as it is: left
        // unchosen, normalising gives an inversion with a default mask of its own (QSMART) that one.
        if args.mask_preset.is_some() || args.mask_sections_cli.is_some() || args.masking_input.is_some() {
            config.masking.recipe_chosen = true;
        }
        if let Some(preset) = args.mask_preset {
            // A preset is a whole recipe — it replaces the combine mode and the post-combine
            // refinements too, not just the sections.
            let recipe = mask_preset_recipe(preset)?;
            config.masking.sections = recipe.sections;
            config.masking.combine = recipe.combine;
            config.masking.refinements = recipe.refinements;
        }
        if let Some(ref sections) = args.mask_sections_cli {
            let new_sections = parse_mask_sections(sections, "--mask")?;
            if !new_sections.is_empty() { config.masking.sections = new_sections; }
        }
        if let Some(combine) = args.mask_combine {
            config.masking.combine = match combine {
                cli::MaskCombineArg::Or => MaskCombine::Or,
                cli::MaskCombineArg::And => MaskCombine::And,
            };
        }
        if let Some(ref refinements) = args.mask_refinements_cli {
            let mut ops: Vec<MaskOp> = Vec::new();
            for s in refinements {
                let op = parse_mask_op(s).map_err(|e| QsmxtError::Config(format!(
                    "--mask-refine {s}: {e}")))?;
                if op.is_generator() {
                    return Err(QsmxtError::Config(format!(
                        "--mask-refine {op}: it creates a mask rather than refining one; put it \
                         in a --mask section instead")));
                }
                ops.push(op);
            }
            config.masking.refinements = ops;
        }

        // ── Two-pass ──
        // Giving a reliable mask implies the method: asking for one and not getting the second
        // pass would be the more surprising reading of `--two-pass-mask ... ` on its own.
        if let Some(ref sections) = args.two_pass_sections_cli {
            let new_sections = parse_mask_sections(sections, "--two-pass-mask")?;
            if !new_sections.is_empty() {
                for warning in hole_closing_warnings(&new_sections) {
                    log::warn!("{warning}");
                }
                config.masking.two_pass_sections = Some(new_sections);
                config.masking.two_pass = true;
            }
        }
        if args.two_pass { config.masking.two_pass = true; }

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

        // The outputs other outputs need (χ-separation's relaxometry maps, the statistics'
        // segmentation, ...) are switched on, and QSMART's BET mask installed where the recipe
        // was left alone, as every front end has them. Nothing below sets a setting the rules read
        // (the BET push only fills in the BET steps the recipe already has).
        let (normalised, adjustments) = config.normalised();
        for a in &adjustments {
            if a.setting == "masking.sections" {
                log::info!("{} (choose another mask with --mask-preset or --mask)", a.message);
            } else {
                log::debug!("{}", a.message);
            }
        }
        *config = normalised;
        if args.export_dicom { config.pipeline.export_dicom = true; }
        if args.no_inhomogeneity_correction { config.masking.inhomogeneity_correction = false; }
        else if args.inhomogeneity_correction { config.masking.inhomogeneity_correction = true; }
        if let Some(tool) = &args.use_custom_masks { config.masking.custom_mask_tool = Some(tool.clone()); }
        if args.axial_resampling { config.pipeline.axial_resampling = true; }
        if args.no_axial_resampling { config.pipeline.axial_resampling = false; }
        if let Some(v) = args.obliquity_threshold {
            if v < 0.0 {
                return Err(QsmxtError::Config(format!(
                    "--obliquity-threshold {v}: the threshold is in degrees and cannot be negative. Resampling \
                     is off unless --axial-resampling is given (a negative threshold no longer turns it off)")));
            }
            config.pipeline.obliquity_threshold = v;
        }
        if args.crop_to_mask { config.pipeline.crop_to_mask = true; }
        if args.ignore_slice_gap { config.pipeline.ignore_slice_gap = true; }
        if args.fft_padding { config.pipeline.fft_padding = true; }
        if let Some(v) = args.crop_margin_mm { config.pipeline.crop_margin_mm = v; }
        if let Some(v) = args.output_space {
            config.pipeline.output_space = match v {
                crate::cli::OutputSpaceArg::Acquired => OutputSpace::Acquired,
                crate::cli::OutputSpaceArg::Working => OutputSpace::Working,
            };
        }

        // QSMART's own default mask (BET) is the engine's to install, when normalising; this only
        // warns, with the options that would help, when the mask the run takes may not suit it.
        if let Some(warning) = qsmxt_engine::masking::mask_warning(config) {
            log::warn!("{warning}. Consider --mask-preset bet, --mask-preset hd-bet, --mask-preset \
                        mindgrab or --mask magnitude,bet:0.5,erode:2.");
        }

        // ── BET ──
        // `config.bet` is the one-value-per-parameter view of BET, but the values BET actually
        // runs with live on each `MaskOp::Bet` in the recipe. Push them out last, so they reach
        // the sections a preset, `--mask` or the QSMART default has just installed — and so
        // `--bet-fractional-intensity` / `--bet-voxel-scale` (or a `[bet]` value in a config
        // file) are the whole recipe's answer, whether or not a `--mask` spec also spelled one out.
        let d = qsmxt_engine::BetConfig::default();
        if args.bet_fractional_intensity.is_some() || config.bet.fractional_intensity != d.fractional_intensity {
            let fi = config.bet.fractional_intensity;
            for_each_bet_op(&mut config.masking, |f, _| *f = fi);
        }
        if args.bet_voxel_scale.is_some() || config.bet.voxel_scale != d.voxel_scale {
            let scale = config.bet.voxel_scale;
            for_each_bet_op(&mut config.masking, |_, s| *s = scale);
        }
        Ok(())
}

/// Refuse a background-removal choice the inversion would never use (#276).
///
/// Background removal, iQFM included, produces a local field, and only a local-field inversion
/// reads one (QSM.rs `InversionInput`). With a total-field or wrapped-phase inversion iQFM has
/// no effect, and used to leave field mapping off while the inversion read a total field that
/// was never written (or a stale one from an earlier run). Asked-for iQFM there is an error;
/// another non-default method is a warning, since nothing else depends on it.
pub fn validate_inversion_input(config: &PipelineConfig) -> crate::Result<()> {
    if !config.pipeline.do_qsm || !qsmxt_engine::bridge::skips_bgremove(config) {
        return Ok(());
    }
    let alg = config.inversion.algorithm;
    let what = format!("--qsm-algorithm {alg}");
    let takes = if qsmxt_engine::bridge::inversion_input(config, 2).needs_field_mapping() {
        "the total field and removes the background itself"
    } else {
        "the wrapped phase"
    };
    match config.bg_removal.algorithm {
        BfAlgorithm::Iqfm => Err(QsmxtError::Config(format!(
            "--bf-algorithm iqfm produces a local field, and {what} takes {takes}, so iQFM would \
             have no effect. Choose another inversion, or drop --bf-algorithm iqfm."))),
        b if b != qsmxt_engine::BgRemovalConfig::default().algorithm => {
            log::warn!("--bf-algorithm {b} has no effect: {what} takes {takes}");
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Refuse a multi-orientation configuration whose members cannot contribute (#270).
///
/// COSMOS and STI combine each orientation's *local* field, which only background-field removal
/// writes. An inversion that skips that step (TFI, TGV, QSMART, the end-to-end networks), or a
/// run with reconstruction off, leaves every member without one, so the group
/// would fail after every member had been processed. Said before any processing instead.
pub fn validate_multi_orientation(config: &PipelineConfig) -> crate::Result<()> {
    if !config.multi_orientation.enabled() {
        return Ok(());
    }
    if !config.pipeline.do_qsm {
        return Err(QsmxtError::Config(
            "--orientation-group combines each orientation's local field, which only the QSM \
             reconstruction produces; it cannot be used with --no-qsm".into()));
    }
    if qsmxt_engine::bridge::skips_bgremove(config) {
        return Err(QsmxtError::Config(format!(
            "--orientation-group combines each orientation's local field, and --qsm-algorithm \
             {} has no background-field removal step, so no orientation would have one. Use an \
             inversion that takes a local field (e.g. rts, tv, tkd, nltv, ilsqr, medi).",
            config.inversion.algorithm,
        )));
    }
    Ok(())
}


/// Run `f(fractional_intensity, voxel_scale)` on every BET step in a masking recipe — the
/// sections, the refinements that run on the combined mask, and the two-pass sections, wherever
/// BET appears as a generator or a refinement. One value for the whole recipe: a run where one
/// BET step saw mouse-scale voxels and another saw the acquisition's would be a configuration
/// mistake, not a feature.
fn for_each_bet_op(masking: &mut MaskingConfig, f: impl Fn(&mut f64, &mut f64)) {
    let set = |op: &mut MaskOp| {
        if let MaskOp::Bet { fractional_intensity, voxel_scale } = op { f(fractional_intensity, voxel_scale); }
    };
    let sections = masking.two_pass_sections.iter_mut().flatten()
        .chain(masking.sections.iter_mut());
    for section in sections {
        set(&mut section.generator);
        for op in &mut section.refinements { set(op); }
    }
    for op in &mut masking.refinements { set(op); }
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
        apply_run_overrides(&mut rebuilt, &run_args.pipeline).unwrap();

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
        apply_run_overrides(&mut config, &run_args.pipeline).unwrap();
        config
    }

    /// A value the run cannot use is an error, not a warning followed by a default (#266's sweep):
    /// each of these used to be logged and ignored, so the run went ahead with something else.
    #[test]
    fn unusable_flag_values_are_refused() {
        let cases: &[&[&str]] = &[
            &["--mask", "magnitud,threshold:otsu"],
            &["--mask", "magnitude,threshhold:otsu"],
            &["--mask", "magnitude,threshold:otsu,bet"],
            &["--two-pass", "--two-pass-mask", "phase-quality,nonsense"],
            &["--mask-refine", "erode:x"],
            &["--mask-refine", "erode:2:3"],
            &["--mask", "magnitude,threshold:fixed:abc"],
            &["--mask-refine", "threshold:otsu"],
            &["--motion-correction", "--motion-reference", "middle"],
            &["--laplacian-fft-pad", "8", "8"],
            &["--ilsqr-pad-mm", "NaN"],
            &["--do-segmentation", "--synthseg-version", "v3"],
        ];
        for extra in cases {
            let mut argv = vec!["qsmxt", "run", "<bids>"];
            argv.extend_from_slice(extra);
            let cli = cli::Cli::try_parse_from(&argv).expect("clap accepts it; the override must not");
            let cli::Command::Run(run_args) = cli.command else { panic!("expected run") };
            let mut config = PipelineConfig::default();
            let err = apply_run_overrides(&mut config, &run_args.pipeline);
            assert!(err.is_err(), "{extra:?} was accepted");
            let msg = format!("{}", err.unwrap_err());
            assert!(msg.contains(extra[extra.len() - 2]) || msg.contains(extra[extra.len() - 1]),
                    "{extra:?}: the error should name the flag or value: {msg}");
        }
    }

    /// SynthSeg 2.0's weights are not published, so asking to run it is refused up front rather
    /// than reading v1's network output through v2's label table (#264). The weight file follows
    /// the version, and a supplied segmentation needs no weights at all.
    #[test]
    fn synthseg_v2_is_refused_until_its_weights_exist() {
        assert_eq!(SynthSegVersion::V1.weights_id(), "synthseg");
        assert_eq!(SynthSegVersion::V2.weights_id(), "synthseg-2.0");
        let v2 = qsm_core::models::find_model("synthseg-2.0").expect("QSM.rs lists SynthSeg 2.0");
        let v2_present = v2.is_available()
            || v2.files.iter().all(qsm_core::models::weight_file_present);

        let v1 = config_from_cli(&["qsmxt", "run", "<bids>", "--do-segmentation"]);
        assert!(validate_segmentation(&v1).is_ok());

        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--do-segmentation", "--synthseg-version", "v2"]);
        if v2_present {
            assert!(validate_segmentation(&c).is_ok());
        } else {
            let e = format!("{}", validate_segmentation(&c).unwrap_err());
            assert!(e.contains("synthseg-2.0.onnx") && e.contains("v1") && e.contains("QSM_MODEL_DIR"), "{e}");
        }
        // A region reference turns segmentation on, so it is refused the same way.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-reference", "csf", "--synthseg-version", "v2"]);
        assert!(c.pipeline.do_segmentation);
        assert_eq!(validate_segmentation(&c).is_ok(), v2_present);
        // A supplied v2 segmentation is read through v2's label table and needs no weights.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--do-segmentation", "--synthseg-version", "v2",
                                  "--use-custom-dseg", "*"]);
        assert!(validate_segmentation(&c).is_ok());
    }

    #[test]
    fn assumed_decay_b0_weighting_from_cli() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--b0-weight-type", "assumed-decay",
            "--b0-weight-assumed-t2star", "35",
        ]);
        assert_eq!(c.field_mapping.b0_weight_type, B0WeightType::AssumedDecay);
        assert_eq!(c.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        let d = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-weight-type", "assumed-decay"]);
        assert_eq!(d.field_mapping.b0_weight_assumed_t2star_ms, 40.0);
        // The never-released `t2star` spelling is gone, not aliased.
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--b0-weight-type", "t2star"]).is_err());
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--b0-weight-t2star", "40"]).is_err());
    }

    #[test]
    fn laplacian_fft_pad_takes_one_value_or_three() {
        let base = ["qsmxt", "run", "<bids>", "--unwrapping-algorithm", "laplacian", "--laplacian-solver", "fft"];
        let with = |extra: &[&str]| config_from_cli(&[&base[..], extra].concat());
        assert_eq!(with(&[]).field_mapping.laplacian_fft_pad, [64; 3]);
        assert_eq!(with(&["--laplacian-fft-pad", "12"]).field_mapping.laplacian_fft_pad, [12; 3]);
        assert_eq!(with(&["--laplacian-fft-pad", "64", "64", "32"]).field_mapping.laplacian_fft_pad, [64, 64, 32]);
        // Two values is neither: refused (see `unusable_flag_values_are_refused`).
    }

    /// Each method's `--<method>-te`, and the method settings QSM.rs v0.40.0 added, reach the
    /// config; `auto` puts a derived setting back to its derived value.
    #[test]
    fn echo_times_and_method_settings_from_cli() {
        use qsmxt_engine::EchoTimeChoice as E;
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--nltv-te", "last", "--medi-te", "first",
            "--tfi-te", "sum", "--ndi-te", "spacing", "--fansi-te", "first", "--l1qsm-te", "12.5",
            "--whqsm-te", "last", "--heidi-te", "first", "--tgv-te", "spacing"]);
        let i = &c.inversion;
        assert_eq!([i.nltv.te, i.medi.te, i.tfi.te, i.ndi.te, i.fansi.te, i.l1qsm.te, i.whqsm.te, i.heidi.te, i.tgv.te],
                   [E::Last, E::First, E::Sum, E::Spacing, E::First, E::Seconds(0.0125), E::Last, E::First, E::Spacing]);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--whqsm-weight", "in-vivo", "--nltv-rho", "0.2",
            "--nltv-tol-newton", "1e-5", "--rts-tau", "0.5", "--rts-gamma", "2", "--pdf-n-cg", "30",
            "--pdf-space", "kspace", "--pdf-n-pad", "0", "--harperella-pad", "1", "2", "3",
            "--iharperella-pad", "4", "5", "6", "--fansi-mu1", "0.1", "--tv-rho", "0.3"]);
        assert_eq!(c.inversion.whqsm.weight, qsmxt_engine::WhqsmWeight::InVivo);
        assert_eq!((c.inversion.nltv.rho, c.inversion.nltv.tol_newton), (Some(0.2), 1e-5));
        assert_eq!((c.inversion.rts.tau, c.inversion.rts.gamma), (0.5, 2.0));
        assert_eq!((c.bg_removal.pdf.n_cg, c.bg_removal.pdf.space, c.bg_removal.pdf.n_pad),
                   (30, qsmxt_engine::PdfSpace::Kspace, 0));
        assert_eq!((c.bg_removal.harperella.pad, c.bg_removal.iharperella.pad), ([1, 2, 3], [4, 5, 6]));
        assert_eq!((c.inversion.fansi.mu1, c.inversion.tv.rho), (Some(0.1), Some(0.3)));
        // `auto` overrides a value from a config file with the derived one.
        let args = |argv: &[&str]| match cli::Cli::try_parse_from(argv).unwrap().command {
            cli::Command::Run(r) => r.pipeline,
            _ => unreachable!(),
        };
        let mut c = PipelineConfig::default();
        (c.inversion.tv.rho, c.inversion.nltv.rho, c.inversion.hdqsm.mu1_l2) = (Some(1.0), Some(1.0), Some(1.0));
        apply_run_overrides(&mut c, &args(&["qsmxt", "run", "<bids>", "--tv-rho", "auto", "--nltv-rho", "auto",
            "--hdqsm-mu1-l2", "auto"])).unwrap();
        assert_eq!((c.inversion.tv.rho, c.inversion.nltv.rho, c.inversion.hdqsm.mu1_l2), (None, None, None));
        for bad in [["--medi-te", "median"], ["--whqsm-weight", "heavy"], ["--pdf-space", "real"], ["--tv-rho", "x"]] {
            assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", bad[0], bad[1]]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ilsqr_pad_mm_from_cli() {
        let d = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert_eq!(d.inversion.ilsqr.pad_mm, qsm_core::inversion::IlsqrParams::default().pad_mm);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--ilsqr-pad-mm", "32"]);
        assert_eq!(c.inversion.ilsqr.pad_mm, 32.0);
        assert_eq!(c.inversion.qsmart.ilsqr_pad_mm, d.inversion.qsmart.ilsqr_pad_mm, "QSMART keeps its own");
        assert_eq!(config_from_cli(&["qsmxt", "run", "<bids>", "--ilsqr-pad-mm", "0"]).inversion.ilsqr.pad_mm, 0.0);
    }

    #[test]
    fn robust_fit_from_cli() {
        let d = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-estimation", "robust-fit"]);
        assert_eq!(d.field_mapping.b0_estimation, B0Estimation::RobustFit);
        assert_eq!(d.field_mapping.robust_fit, RobustFitConfig::default());
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-estimation", "robust-fit",
            "--robust-fit-loss", "huber", "--robust-fit-tuning", "2", "--robust-fit-iterations", "4",
            "--robust-fit-downweight-level", "0.6", "--robust-fit-flag-ratio", "5",
            "--linear-fit-estimate-offset", "false", "--linear-fit-reliability-threshold", "0"]);
        assert_eq!(c.field_mapping.robust_fit, RobustFitConfig {
            loss: RobustLoss::Huber, tuning: Some(2.0), iterations: 4, downweight_level: 0.6, flag_ratio: 5.0,
        });
        assert!(!c.field_mapping.linear_fit.estimate_offset);
        assert_eq!(c.field_mapping.linear_fit.reliability_threshold_percentile, 0.0);
    }

    #[test]
    fn ignore_slice_gap_from_cli() {
        assert!(!config_from_cli(&["qsmxt", "run", "<bids>"]).pipeline.ignore_slice_gap);
        assert!(config_from_cli(&["qsmxt", "run", "<bids>", "--ignore-slice-gap"]).pipeline.ignore_slice_gap);
    }

    #[test]
    fn motion_correction_from_cli() {
        let d = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert!(!d.motion_correction.enabled, "off by default");
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--motion-correction",
            "--motion-reference", "3", "--no-motion-chain", "--motion-chain-levels", "2",
            "--motion-min-ncc", "0.9", "--registration-levels", "4", "--registration-max-samples", "1000",
            "--registration-max-iterations", "7", "--registration-max-evaluations", "99",
            "--registration-tolerance", "0.001", "--registration-rotation-step-deg", "2",
            "--registration-translation-step-mm", "1.5", "--registration-min-overlap", "0.4"]);
        let m = &c.motion_correction;
        assert!(m.enabled && !m.chain);
        assert_eq!(m.reference, qsmxt_engine::MotionReference::Echo(3));
        assert_eq!((m.chain_levels, m.min_ncc), (2, 0.9));
        let r = &c.registration;
        assert_eq!((r.levels, r.max_samples, r.max_iterations, r.max_evaluations), (4, 1000, 7, 99));
        assert_eq!((r.tolerance, r.rotation_step_deg, r.translation_step_mm, r.min_overlap), (0.001, 2.0, 1.5, 0.4));
        let l = config_from_cli(&["qsmxt", "run", "<bids>", "--motion-correction", "--motion-reference", "last"]);
        assert_eq!(l.motion_correction.reference, qsmxt_engine::MotionReference::Last);
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
    fn the_removed_mouse_bet_preset_says_what_to_use_instead() {
        let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--mask-preset", "mouse-bet"])
            .expect("still parsed, so it can be explained");
        let cli::Command::Run(run_args) = cli.command else { panic!("expected Run") };
        let err = apply_run_overrides(&mut PipelineConfig::default(), &run_args.pipeline)
            .expect_err("mouse-bet was removed").to_string();
        assert!(err.contains("removed") && err.contains("rs2-net") && err.contains("scale="), "{err}");
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
        assert!(c.masking.recipe_chosen, "the choice is in the config, so it is saved");
    }

    /// Which flags choose the mask under QSMART (and so keep it), which leave QSMART's BET in
    /// place, and that the BET options still reach the BET step the engine installs.
    #[test]
    fn qsmart_mask_flags_choose_the_mask() {
        let qsmart = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart"];
            argv.extend_from_slice(extra);
            config_from_cli(&argv)
        };
        let c = qsmart(&[]);
        assert_eq!(c.masking.sections, qsmart_default_mask_sections());
        assert!(!c.masking.recipe_chosen);
        // `--masking-input` chooses the recipe: with the default's own input it is kept as it is.
        let c = qsmart(&["--masking-input", "phase-quality"]);
        assert_eq!(c.masking.sections, default_mask_sections());
        assert!(c.masking.recipe_chosen);
        let c = qsmart(&["--masking-input", "magnitude"]);
        assert!(c.masking.sections.iter().all(|s| s.input == MaskingInput::Magnitude
            && matches!(s.generator, MaskOp::Threshold { .. })));
        // `--mask` spelling out the default recipe keeps it too.
        let spec = default_mask_sections()[0].compact_spec();
        let c = qsmart(&["--mask", &spec]);
        assert_eq!(c.masking.sections, default_mask_sections());
        // The BET options reach QSMART's BET; the combine options alone do not choose the recipe.
        let c = qsmart(&["--bet-fractional-intensity", "0.3", "--mask-refine", "erode:1"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { fractional_intensity, .. }
                         if fractional_intensity == 0.3));
        assert_eq!(c.masking.sections[0].refinements, qsmart_default_mask_sections()[0].refinements);
        assert!(!c.masking.recipe_chosen);
    }

    /// A QSMART run's saved config reloads as the same run, the kept threshold included, and so
    /// does the command it generates.
    #[test]
    fn qsmart_mask_choice_survives_the_saved_config_and_the_command() {
        let no_flags = |mut c: PipelineConfig| {
            let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>"]).unwrap();
            let cli::Command::Run(r) = cli.command else { panic!() };
            apply_run_overrides(&mut c, &r.pipeline).unwrap();
            c
        };
        for extra in [&[][..], &["--mask-preset", "robust-threshold"], &["--mask-preset", "bet"],
                      &["--masking-input", "phase-quality"], &["--bet-fractional-intensity", "0.3"]] {
            let mut argv = vec!["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart"];
            argv.extend_from_slice(extra);
            let c = config_from_cli(&argv);
            let reloaded = no_flags(PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap());
            assert_eq!(reloaded, c, "{extra:?}");
            let cmd = qsmxt_engine::generate_command(&c);
            let args: Vec<&str> = cmd.split_whitespace().collect();
            assert_eq!(config_from_cli(&args), c, "{extra:?}: {cmd}");
        }
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

    /// `--bet-voxel-scale` has to reach the BET step the recipe actually runs, whichever
    /// recipe that is — qsm-core reads `voxel_scale` off the op, not off `config.bet`.
    #[test]
    fn bet_voxel_scale_reaches_every_bet_step() {
        // `--mask-preset bet`, resolved after the flag is read.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "bet",
                                  "--bet-voxel-scale", "10"]);
        assert_eq!(c.bet.voxel_scale, 10.0);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 10.0));

        // A BET step anywhere in a `--mask` section, and one among the combined refinements.
        let c = config_from_cli(&["qsmxt", "run", "<bids>",
                                  "--mask", "magnitude,bet:0.4,erode:2",
                                  "--mask", "phase-quality,threshold:otsu",
                                  "--bet-voxel-scale", "10"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 10.0));

        // The QSMART default mask is installed later still, and is BET.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart",
                                  "--bet-voxel-scale", "10"]);
        assert_eq!(c.masking.sections, qsmart_default_mask_sections().into_iter()
                   .map(|mut s| { s.generator = MaskOp::Bet { fractional_intensity: 0.5, voxel_scale: 10.0 }; s })
                   .collect::<Vec<_>>());
    }

    /// `--bet-fractional-intensity` used to be stored in `config.bet` and never reach a BET step
    /// (#279). It is now every BET step's fractional intensity, like `--bet-voxel-scale`, and a
    /// config file's `[bet]` values reach the steps the same way. BET's other parameters cannot
    /// travel on a mask step, so they are refused rather than ignored.
    #[test]
    fn bet_fractional_intensity_reaches_every_bet_step() {
        let fi = |c: &PipelineConfig| -> Vec<f64> {
            c.masking.sections.iter().map(|s| &s.generator).chain(c.masking.refinements.iter())
                .filter_map(|op| match op { MaskOp::Bet { fractional_intensity, .. } => Some(*fractional_intensity), _ => None })
                .collect()
        };
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask", "magnitude,bet:0.4,erode:2",
                                  "--mask", "magnitude,bet", "--bet-fractional-intensity", "0.3"]);
        assert_eq!(fi(&c), vec![0.3, 0.3]);
        // Without the flag, each step keeps its own.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask", "magnitude,bet:0.4"]);
        assert_eq!(fi(&c), vec![0.4]);
        // From a config file.
        let mut file = PipelineConfig::from_toml("[bet]\nfractional_intensity = 0.35\n").unwrap();
        file.masking.sections = vec![MaskSection { input: MaskingInput::MagnitudeFirst, generator: MaskOp::bet(0.5), refinements: vec![] }];
        let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>"]).unwrap();
        let cli::Command::Run(r) = cli.command else { panic!() };
        apply_run_overrides(&mut file, &r.pipeline).unwrap();
        assert_eq!(fi(&file), vec![0.35]);
        // BET's surface settings reach the run's config (and from there every BET step;
        // QSM.rs#180), from the command line and from a config file alike.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--bet-smoothness", "2", "--bet-gradient-threshold",
                                  "-0.2", "--bet-iterations", "500", "--bet-subdivisions", "3"]);
        assert_eq!((c.bet.smoothness, c.bet.gradient_threshold, c.bet.iterations, c.bet.subdivisions),
                   (2.0, -0.2, 500, 3));
        let c = PipelineConfig::from_toml("[bet]\nsmoothness = 2.0\niterations = 500\n").unwrap();
        assert_eq!((c.bet.smoothness, c.bet.iterations), (2.0, 500));
    }

    /// The flag is the whole recipe's answer: it also wins over a `scale=` spelled out in
    /// `--mask`, so the command a saved config generates (which carries both) round-trips.
    #[test]
    fn bet_voxel_scale_flag_wins_over_a_spelled_out_scale() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>",
                                  "--mask", "magnitude,bet:0.4:scale=2",
                                  "--bet-voxel-scale", "10"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 10.0));
        // Without the flag, the per-step `scale=` stands on its own.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask", "magnitude,bet:0.4:scale=2"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 2.0));
        assert_eq!(c.bet.voxel_scale, 1.0, "config.bet is untouched by a per-step scale");
    }

    /// Default: nothing anywhere is scaled, so no existing run changes behaviour.
    #[test]
    fn bet_voxel_scale_defaults_to_the_acquired_geometry() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "bet-and-phase"]);
        assert_eq!(c.bet.voxel_scale, 1.0);
        for section in &c.masking.sections {
            for op in section.all_ops() {
                if let MaskOp::Bet { voxel_scale, .. } = op {
                    assert_eq!(voxel_scale, 1.0);
                }
            }
        }
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
    fn hd_bet_mask_preset_and_sections() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "hd-bet"]);
        assert_eq!(c.masking.sections, hd_bet_mask_sections());
        // --mask: hd-bet is recognised as the generator wherever it appears in the section.
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--mask", "magnitude,signal-erode:0.7,hd-bet:low-memory",
        ]);
        assert_eq!(c.masking.sections[0].generator,
                   MaskOp::HdBet { patch: Some([128, 128, 64]), tta: false, tile_step: hd_bet_default_tile_step() });
        assert!(matches!(c.masking.sections[0].refinements[..], [MaskOp::SignalErode { threshold, .. }] if threshold == 0.7));
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

    /// Round-trip a config's masking through `generate_command` and back.
    fn masking_round_trip(config: &PipelineConfig) -> (String, MaskingConfig) {
        let cmd = generate_command(config);
        let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        let cli = cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("generated command did not parse: {e}\ncmd: {cmd}"));
        let run_args = match cli.command { cli::Command::Run(a) => a, _ => panic!("expected Run") };
        let mut rebuilt = PipelineConfig::default();
        apply_run_overrides(&mut rebuilt, &run_args.pipeline).unwrap();
        (cmd, rebuilt.masking)
    }

    /// A preclinical recipe survives the trip out to a command line and back. The command
    /// carries the scaling twice (the flag and the BET step's own `scale=`); both say 10, so
    /// it does not matter which one the rebuild reads.
    #[test]
    fn generated_command_round_trips_a_scaled_bet_recipe() {
        let mut config = PipelineConfig::default();
        config.bet.voxel_scale = 10.0;
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Bet { fractional_intensity: 0.4, voxel_scale: 10.0 },
            refinements: vec![MaskOp::Erode { iterations: 2 }],
        }];
        let (cmd, masking) = masking_round_trip(&config);
        assert_eq!(masking.sections, config.masking.sections, "cmd: {cmd}");
    }

    /// The generated `qsmxt run` command has to carry the combine mode and the post-combine
    /// steps, or a hand-built intersection silently runs as a union.
    #[test]
    fn generated_command_round_trips_an_and_recipe() {
        // Not one of the presets — this is the path that spells the sections out.
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![
            MaskSection { input: MaskingInput::Magnitude,
                          generator: MaskOp::bet(0.4), refinements: vec![] },
            MaskSection { input: MaskingInput::Magnitude,
                          generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                          refinements: vec![] },
        ];
        config.masking.combine = MaskCombine::And;
        config.masking.refinements = vec![MaskOp::FillHoles { max_size: 0 }];

        let (cmd, masking) = masking_round_trip(&config);
        assert!(cmd.contains("--mask-combine and"), "cmd: {cmd}");
        assert!(cmd.contains("--mask-refine fill-holes:0"), "cmd: {cmd}");
        assert_eq!(masking.sections, config.masking.sections);
        assert_eq!(masking.combine, config.masking.combine);
        assert_eq!(masking.refinements, config.masking.refinements);
    }

    /// Choosing a preset should read as choosing a preset, not as its expansion — and it still
    /// has to rebuild the same masking.
    #[test]
    fn generated_command_names_the_preset() {
        for (preset, recipe) in mask_presets() {
            let mut config = PipelineConfig::default();
            config.masking.sections = recipe.sections.clone();
            config.masking.combine = recipe.combine;
            config.masking.refinements = recipe.refinements.clone();

            let (cmd, masking) = masking_round_trip(&config);
            if preset == "robust-threshold" {
                // The default: saying it adds noise rather than removing it.
                assert!(!cmd.contains("--mask"), "default masking should print nothing: {cmd}");
            } else {
                assert!(cmd.contains(&format!("--mask-preset {preset}")), "cmd: {cmd}");
                assert!(!cmd.contains("--mask "), "preset should replace the expansion: {cmd}");
                assert!(!cmd.contains("--mask-refine"), "preset should replace the expansion: {cmd}");
            }
            assert_eq!(masking.sections, recipe.sections, "{preset} did not rebuild");
            assert_eq!(masking.combine, recipe.combine, "{preset} did not rebuild");
            assert_eq!(masking.refinements, recipe.refinements, "{preset} did not rebuild");
        }
    }

    /// A preset whose sections all read a different image is still a preset, plus one flag.
    #[test]
    fn generated_command_names_the_preset_with_an_input_override() {
        let mut config = PipelineConfig::default();
        config.masking.sections = default_mask_sections();
        for section in &mut config.masking.sections { section.input = MaskingInput::Magnitude; }

        let (cmd, masking) = masking_round_trip(&config);
        assert!(cmd.contains("--mask-preset robust-threshold"), "cmd: {cmd}");
        assert!(cmd.contains("--masking-input magnitude"), "cmd: {cmd}");
        assert_eq!(masking.sections, config.masking.sections);

        // Editing a parameter as well takes it back to the spelled-out form.
        config.masking.sections[0].refinements.push(MaskOp::Erode { iterations: 3 });
        let (cmd, masking) = masking_round_trip(&config);
        assert!(cmd.contains("--mask magnitude,"), "cmd: {cmd}");
        assert_eq!(masking.sections, config.masking.sections);
    }

    #[test]
    fn mask_combine_and_refine_from_cli() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>",
            "--mask", "magnitude-first,bet:0.5",
            "--mask", "phase-quality,threshold:otsu",
            "--mask-combine", "and",
            "--mask-refine", "fill-holes:0",
            "--mask-refine", "erode:1",
        ]);
        assert_eq!(c.masking.sections.len(), 2);
        assert_eq!(c.masking.combine, MaskCombine::And);
        assert_eq!(c.masking.refinements,
                   vec![MaskOp::FillHoles { max_size: 0 }, MaskOp::Erode { iterations: 1 }]);

        // Default is unchanged when the flags are absent.
        let c = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert_eq!(c.masking.combine, MaskCombine::Or);
        assert!(c.masking.refinements.is_empty());
    }

    /// An erosion, dilation or closing of 0 would do nothing: refused, naming the step.
    #[test]
    fn a_morphology_step_of_zero_is_refused_on_the_command_line() {
        for (flag, spec) in [("--mask", "magnitude,bet:0.5,erode:0"), ("--mask-refine", "dilate:0"),
                             ("--two-pass-mask", "phase-quality,threshold:otsu,close:0")] {
            let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", flag, spec]).expect("parse");
            let cli::Command::Run(run) = cli.command else { panic!("expected run") };
            let e = apply_run_overrides(&mut PipelineConfig::default(), &run.pipeline).unwrap_err().to_string();
            assert!(e.contains("must be at least 1, got 0"), "{flag} {spec}: {e}");
        }
    }

    #[test]
    fn two_pass_is_off_unless_asked_for() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert!(!c.masking.two_pass);
        assert!(c.masking.two_pass_sections.is_none());

        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass"]);
        assert!(c.masking.two_pass);
        // Left unset, so the default recipe is what resolves — and the generated command stays
        // a bare --two-pass.
        assert!(c.masking.two_pass_sections.is_none());
        assert_eq!(c.masking.resolved_two_pass_sections(), default_two_pass_sections());
    }

    /// Spelling out a reliable mask is an unambiguous request for the method; making the user
    /// pass `--two-pass` as well would only be a way to get it silently ignored.
    #[test]
    fn two_pass_mask_implies_two_pass() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--two-pass-mask", "magnitude,threshold:percentile:30",
        ]);
        assert!(c.masking.two_pass);
        let sections = c.masking.two_pass_sections.expect("sections");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].input, MaskingInput::Magnitude);
        assert!(matches!(sections[0].generator,
                         MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(v) } if v == 30.0));
    }

    /// Two-pass leaves the main mask alone — it adds a second one rather than replacing the first.
    #[test]
    fn two_pass_does_not_disturb_the_main_mask() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass"]);
        assert_eq!(c.masking.sections, default_mask_sections());
        assert_eq!(c.masking.combine, MaskCombine::Or);
        assert!(c.masking.refinements.is_empty());
    }

    /// A reliable mask that fills its own holes reconstructs the same region as the main pass. It
    /// is honoured (the user may have a reason) but warned about rather than silently doubling the
    /// run time for nothing.
    #[test]
    fn two_pass_mask_keeps_hole_filling_the_user_asked_for() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--two-pass-mask", "phase-quality,threshold:otsu,fill-holes:0",
        ]);
        let sections = c.masking.two_pass_sections.expect("sections");
        assert_eq!(sections[0].refinements, vec![MaskOp::FillHoles { max_size: 0 }]);

        // ...and the user is told, rather than paying for a second reconstruction in silence.
        let warnings = super::hole_closing_warnings(&sections);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("closes the holes"), "{}", warnings[0]);
        assert!(warnings[0].contains("fill-holes"), "the message should quote the recipe: {}", warnings[0]);
    }

    /// Every op that can swallow a hole has to trigger the warning, not just the obvious one —
    /// `close` bridges a hole's rim and a Gaussian smooth re-thresholds small ones away.
    #[test]
    fn every_hole_closing_op_is_warned_about() {
        let section = |op: MaskOp| MaskSection {
            input: MaskingInput::PhaseQuality,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![op],
        };
        for op in [MaskOp::FillHoles { max_size: 0 }, MaskOp::Close { radius: 1 },
                   MaskOp::GaussianSmooth { sigma_mm: 4.0 }] {
            assert_eq!(super::hole_closing_warnings(&[section(op.clone())]).len(), 1, "{op} should warn");
        }
        // Ops that only shrink or grow the mask leave its holes intact.
        for op in [MaskOp::Erode { iterations: 1 }, MaskOp::Dilate { iterations: 1 }] {
            assert!(super::hole_closing_warnings(&[section(op.clone())]).is_empty(), "{op} should not warn");
        }
        // The default recipe must be silent, or the warning is noise.
        assert!(super::hole_closing_warnings(&default_two_pass_sections()).is_empty());
    }

    /// A generator in --mask-refine has nothing to refine — it would throw the combined mask
    /// away. It is dropped with a warning rather than silently replacing the mask.
    #[test]
    fn mask_refine_takes_refinements() {
        // A generator is refused (see `unusable_flag_values_are_refused`); refinements are kept.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-refine", "erode:2"]);
        assert_eq!(c.masking.refinements, vec![MaskOp::Erode { iterations: 2 }]);
    }

    #[test]
    fn bet_and_phase_preset_is_a_whole_recipe() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "bet-and-phase"]);
        let recipe = bet_and_phase_mask_recipe();
        assert_eq!(c.masking.sections, recipe.sections);
        assert_eq!(c.masking.combine, recipe.combine);
        assert_eq!(c.masking.refinements, recipe.refinements);

        // A single-section preset over a two-section config clears the combine mode and the
        // post-combine steps, rather than leaving them to apply to one section.
        let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--mask-preset", "bet"]).expect("parse");
        let run_args = match cli.command { cli::Command::Run(a) => a, _ => panic!("expected Run") };
        let mut config = PipelineConfig {
            masking: MaskingConfig { sections: recipe.sections, combine: recipe.combine,
                                     refinements: recipe.refinements, ..MaskingConfig::default() },
            ..PipelineConfig::default()
        };
        apply_run_overrides(&mut config, &run_args.pipeline).unwrap();
        assert_eq!(config.masking.combine, MaskCombine::Or);
        assert!(config.masking.refinements.is_empty());
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

    /// A browser run is reproducible as a command: the browser profile's values reach the
    /// generated `qsmxt run` command, and the command rebuilds them.
    #[test]
    fn browser_profile_round_trips_through_the_command() {
        use qsmxt_engine::profile::{BROWSER_R2PRIMENET_PATCH, BROWSER_TILE_HALO, BROWSER_TILE_SIZE};
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        c.masking.sections = qsmxt_engine::masking::hd_bet_mask_sections();
        c.pipeline.do_r2primemap = true;
        c.separation.r2prime_strategy = qsmxt_engine::R2PrimeStrategy::R2primenet;
        c.apply_browser_profile();

        let cmd = generate_command(&c);
        let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        let cli = cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("generated command did not parse: {e}\ncmd: {cmd}"));
        let cli::Command::Run(run_args) = cli.command else { panic!("expected Run") };
        let mut rebuilt = PipelineConfig::default();
        apply_run_overrides(&mut rebuilt, &run_args.pipeline).unwrap();

        assert_eq!(rebuilt.inversion.tile_size, Some(BROWSER_TILE_SIZE), "{cmd}");
        assert_eq!(rebuilt.inversion.tile_halo, Some(BROWSER_TILE_HALO), "{cmd}");
        assert_eq!(rebuilt.separation.r2primenet.patch, BROWSER_R2PRIMENET_PATCH, "{cmd}");
        assert_eq!(rebuilt.masking.sections, c.masking.sections, "{cmd}");
        assert_eq!(generate_command(&rebuilt), cmd);
    }

    /// The browser's R2PRIMEnet patch is the one qsm-core sizes for a WASM heap.
    #[cfg(feature = "dl")]
    #[test]
    fn browser_r2primenet_patch_is_qsm_cores_wasm_patch() {
        let (x, y, z) = qsm_core::relaxometry::WASM_PATCH;
        assert_eq!(qsmxt_engine::profile::BROWSER_R2PRIMENET_PATCH, [x, y, z]);
    }
}
