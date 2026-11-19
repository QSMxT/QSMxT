//! The pipeline configuration: qsmxt-engine's, with the checks and mappings a run's settings go
//! through before it starts. The binary's `pipeline::config` adds the command-line mapping.

use std::path::Path;

use crate::error::QsmxtError;

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

/// Parse `<input>,<op>,<op>,...` mask-section specs — the `--mask` and `--two-pass-mask` grammar.
///
/// An invalid piece is an error: the flag was asked for, and running with a different mask than
/// the one spelled out is worse than stopping. `flag` names the option in the error. A section
/// with no generator op gets an Otsu threshold, which is what its input implies.
pub fn parse_mask_sections(specs: &[String], flag: &str) -> crate::Result<Vec<MaskSection>> {
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
pub fn hole_closing_warnings(sections: &[MaskSection]) -> Vec<String> {
    sections.iter()
        .filter(|s| s.refinements.iter().any(closes_holes))
        .map(|s| format!(
            "--two-pass-mask {}: this closes the holes two-pass reconstructs around, which \
             collapses the reliable pass onto the ordinary one",
            s.compact_spec(),
        ))
        .collect()
}

/// QSMART's echo-fit threshold flags onto `fm`: the mode as given, or else the mode of the value
/// given (a percentile, or a fixed residual); each value as given.
pub fn apply_qsmart_fit_threshold(
    fm: &mut FieldMappingConfig, mode: Option<QsmartFitThresholdMode>,
    percentile: Option<f64>, threshold: Option<f64>,
) {
    let implied = match (percentile, threshold) {
        (Some(_), None) => Some(QsmartFitThresholdMode::Percentile),
        (None, Some(_)) => Some(QsmartFitThresholdMode::Absolute),
        _ => None,
    };
    if let Some(m) = mode.or(implied) { fm.qsmart_fit_threshold_mode = m; }
    if let Some(p) = percentile { fm.qsmart_fit_percentile = p; }
    if let Some(t) = threshold { fm.qsmart_fit_threshold = t; }
}

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

/// Turn on the outputs other settings need ([`PipelineConfig::normalised`]), each logged at info
/// level in plain words ("Segmentation turned on: Reference = ventricles needs it (...)") and
/// recorded in [`PipelineConfig::required_changes`] for the methods Note. These are QSMxT's own
/// required changes, which the run makes itself; QSM.rs's `required` rules are refused at plan
/// time instead ([`qsmxt_engine::requirements`]).
pub fn normalise_logged(config: &mut PipelineConfig) {
    let (normalised, adjustments) = config.normalised();
    for a in &adjustments {
        log::info!("{}", a.message);
    }
    *config = normalised;
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
pub fn for_each_bet_op(masking: &mut MaskingConfig, f: impl Fn(&mut f64, &mut f64)) {
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
