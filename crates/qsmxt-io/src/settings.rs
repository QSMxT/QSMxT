//! `qsmxt run`'s settings applied to a configuration: the flags given (as the engine's flag table,
//! [`qsmxt_engine::schema::cli::PIPELINE_FLAGS`], names them) and the TUI's settings by path. The
//! binary parses the flags; the TUI passes its form's settings.

use qsmxt_engine::schema::cli_apply::Given;
use qsmxt_engine::PipelineConfig;

use crate::error::QsmxtError;

/// The log target these messages have always had (they were the binary's `cli_settings`).
const LOG_TARGET: &str = "qsmxt::cli_settings";

/// `qsmxt run`'s settings: the flags given and, from the TUI, its form's settings by path, set
/// before the flags (the TUI passes no flag but its dismissed recommendations).
#[derive(Debug, Default)]
pub struct RunSettings {
    pub given: Vec<Given>,
    pub values: Vec<(&'static str, serde_json::Value)>,
}

/// The registry flags applied here rather than as one value of one setting.
pub const APPLIED_HERE: &[&str] = &[
    "pipeline", "qsm-algorithm", "qsm-reference", "romeo-template", "synthseg-version",
    "qsmart-fit-threshold-mode", "qsmart-fit-percentile", "qsmart-fit-threshold", "laplacian-fft-pad",
];

/// The configuration `settings` make of `config`, as `qsmxt run` takes it.
pub fn build_config(mut config: PipelineConfig, settings: &RunSettings) -> crate::Result<PipelineConfig> {
    apply_normalised(&mut config, settings)?;
    crate::pipeline::config::validate_reference_region(&config)?;
    crate::pipeline::config::validate_segmentation(&config)?;
    crate::pipeline::config::validate_multi_orientation(&config)?;
    crate::pipeline::config::validate_inversion_input(&config)?;
    qsmxt_engine::schema::check_ranges(&config).map_err(|e| QsmxtError::Config(e.to_string()))?;
    Ok(config)
}

/// `settings` applied to `config`, with the outputs other settings need turned on.
pub fn apply_normalised(config: &mut PipelineConfig, settings: &RunSettings) -> crate::Result<()> {
    for (path, value) in &settings.values {
        qsmxt_engine::schema::set(config, path, value.clone()).map_err(QsmxtError::Config)?;
    }
    apply(config, &settings.given)?;
    crate::pipeline::config::normalise_logged(config);
    Ok(())
}

/// A `--qsmart-fit-threshold-mode` value, refused as the command line refuses it.
fn fit_threshold_mode(m: &str) -> crate::Result<qsmxt_engine::QsmartFitThresholdMode> {
    match m {
        "percentile" => Ok(qsmxt_engine::QsmartFitThresholdMode::Percentile),
        "absolute" => Ok(qsmxt_engine::QsmartFitThresholdMode::Absolute),
        _ => Err(QsmxtError::Config(format!("invalid variant: {m}"))),
    }
}

/// The values the registry's types and ranges do not check, refused with what to give instead.
fn check(given: &[Given]) -> crate::Result<()> {
    let err = QsmxtError::Config;
    for g in given {
        let v = g.values.first().map(String::as_str).unwrap_or_default();
        match g.flag.long {
            "obliquity-threshold" if v.parse::<f64>().is_ok_and(|x| x < 0.0) => return Err(err(format!(
                "--obliquity-threshold {v}: the threshold is in degrees and cannot be negative. Resampling \
                 is off unless --axial-resampling is given (a negative threshold no longer turns it off)"))),
            "ilsqr-pad-mm" => {
                let pad_mm = v.parse::<f64>().map_err(|e| err(e.to_string()))?;
                qsm_core::inversion::IlsqrParams { pad_mm, ..Default::default() }.validate()
                    .map_err(|e| err(format!("--ilsqr-pad-mm: {e}")))?;
            }
            "motion-reference" => {
                v.parse::<qsmxt_engine::MotionReference>().map_err(|e| err(format!("--motion-reference {v}: {e}")))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// `given` applied to `config`, as the flags give it (no outputs turned on for other settings).
pub fn apply(config: &mut PipelineConfig, given: &[Given]) -> crate::Result<()> {
    let err = QsmxtError::Config;
    let one = |long: &str| given.iter().find(|g| g.flag.long == long).and_then(|g| g.values.first()).map(String::as_str);
    let set = |c: &mut PipelineConfig, path: &str, v: serde_json::Value| qsmxt_engine::schema::set(c, path, v).map_err(err);
    let json = |s: &str| serde_json::Value::String(s.to_string());

    // A pipeline first, so that every other flag changes its settings.
    if let Some(id) = one("pipeline") {
        qsmxt_engine::pipelines::choose(config, id, &qsmxt_engine::schema::ViewFacts::default())
            .map_err(|e| err(format!("--pipeline {id}: {e}")))?;
    }
    // `heidi` is LSQR with HEIDI on.
    match one("qsm-algorithm") {
        Some("heidi") => {
            set(config, "inversion.algorithm", json("lsqr"))?;
            config.inversion.heidi.enabled = true;
        }
        Some(a) => set(config, "inversion.algorithm", json(a))?,
        None => {}
    }
    if one("qsmart-inversion") == Some("heidi") {
        return Err(err("--qsmart-inversion heidi: HEIDI is not one of QSMART's inner inversions (its two stages are its own recipe)".into()));
    }
    check(given)?;
    let settings: Vec<Given> = given.iter().filter(|g| !APPLIED_HERE.contains(&g.flag.long)).cloned().collect();
    let rest = qsmxt_engine::schema::cli_apply::apply_settings(config, &settings).map_err(err)?;
    config.multi_orientation.validate().map_err(err)?;
    // A pipeline that cannot run with the flags given after it (HEIDI's with --orientation-group)
    // is refused with the reason, as the TUI's Pipeline tab does not offer it.
    if let Some(id) = one("pipeline") {
        if let Some(why) = qsmxt_engine::pipelines::unavailable(config, id) {
            return Err(err(format!("--pipeline {id} is not available with --orientation-group: {why}")));
        }
    }

    apply_parsed_flags(config, given)?;
    apply_masking_flags(config, given, &rest)?;
    apply_dismissals(config, given)
}

/// The flags whose values are parsed here: the field-mapping values, the reference, SWI's filtered
/// phase and the SynthSeg version.
fn apply_parsed_flags(config: &mut PipelineConfig, given: &[Given]) -> crate::Result<()> {
    let err = QsmxtError::Config;
    let one = |long: &str| given.iter().find(|g| g.flag.long == long).and_then(|g| g.values.first()).map(String::as_str);
    let all = |long: &str| given.iter().find(|g| g.flag.long == long).map(|g| g.values.clone());

    if let Some(p) = all("laplacian-fft-pad") {
        let pad: Vec<usize> = p.iter().map(|v| v.parse()).collect::<Result<_, _>>().map_err(|e: std::num::ParseIntError| err(e.to_string()))?;
        config.field_mapping.laplacian_fft_pad = qsmxt_engine::pad3_from(&pad)
            .ok_or_else(|| err(format!("--laplacian-fft-pad {pad:?}: give one value or three (x y z)")))?;
    }
    if let Some(t) = one("romeo-template") {
        config.field_mapping.romeo.template = t.parse::<usize>().map_err(|e| err(e.to_string()))?.saturating_sub(1);
    }
    {
        let mode = one("qsmart-fit-threshold-mode").map(fit_threshold_mode).transpose()?;
        let number = |long: &str| one(long).map(|v| v.parse::<f64>().map_err(|e| err(e.to_string()))).transpose();
        crate::pipeline::config::apply_qsmart_fit_threshold(&mut config.field_mapping, mode,
            number("qsmart-fit-percentile")?, number("qsmart-fit-threshold")?);
    }
    // Anything but `mean` and `none` is a region, validated once the whole config exists.
    if let Some(a) = one("qsm-reference") {
        let (reference, region) = crate::pipeline::config::parse_reference_spec(a);
        config.qsm.reference = reference;
        config.qsm.reference_region = region;
    }
    // The filtered phase comes from the SWI computation, so asking for it asks for SWI.
    if one("swi-filtered-phase").is_some() || given.iter().any(|g| g.flag.long == "swi-filtered-phase") {
        config.pipeline.do_swi = true;
    }
    if let Some(v) = one("synthseg-version") {
        config.segmentation.version = qsmxt_engine::parse_synthseg_version(v)
            .ok_or_else(|| err(format!("--synthseg-version {v}: expected v1 or v2")))?;
    }
    Ok(())
}

/// The masking flags: a preset, sections, combine mode, refinements, two-pass, the masking input
/// and BET's values. `rest` is the flags the registry did not apply.
fn apply_masking_flags(config: &mut PipelineConfig, given: &[Given], rest: &[Given]) -> crate::Result<()> {
    let err = QsmxtError::Config;
    let one = |long: &str| given.iter().find(|g| g.flag.long == long).and_then(|g| g.values.first()).map(String::as_str);
    let all = |long: &str| given.iter().find(|g| g.flag.long == long).map(|g| g.values.clone());
    let set = |c: &mut PipelineConfig, path: &str, v: serde_json::Value| qsmxt_engine::schema::set(c, path, v).map_err(err);
    let json = |s: &str| serde_json::Value::String(s.to_string());

    let has = |long: &str| rest.iter().any(|g| g.flag.long == long);
    // A preset is a whole recipe: it replaces the combine mode and the refinements too.
    if let Some(name) = one("mask-preset") {
        let recipe = qsmxt_engine::masking::mask_preset(name).map_err(err)?;
        config.masking.sections = recipe.sections;
        config.masking.combine = recipe.combine;
        config.masking.refinements = recipe.refinements;
    }
    if let Some(sections) = all("mask") {
        let new_sections = crate::pipeline::config::parse_mask_sections(&sections, "--mask")?;
        if !new_sections.is_empty() {
            config.masking.sections = new_sections;
        }
    }
    if let Some(c) = one("mask-combine") {
        set(config, "masking.combine", json(c))?;
    }
    if let Some(refinements) = all("mask-refine") {
        let mut ops = Vec::new();
        for s in &refinements {
            let op = qsmxt_engine::masking::parse_mask_op(s).map_err(|e| err(format!("--mask-refine {s}: {e}")))?;
            if op.is_generator() {
                return Err(err(format!("--mask-refine {op}: it creates a mask rather than refining one; put it \
                                        in a --mask section instead")));
            }
            ops.push(op);
        }
        config.masking.refinements = ops;
    }
    // Giving a reliable mask, its combine mode or the two-pass mode asks for two-pass.
    if let Some(sections) = all("two-pass-mask") {
        let new_sections = crate::pipeline::config::parse_mask_sections(&sections, "--two-pass-mask")?;
        if !new_sections.is_empty() {
            for warning in crate::pipeline::config::hole_closing_warnings(&new_sections) {
                log::warn!(target: LOG_TARGET, "{warning}");
            }
            config.masking.two_pass_sections = Some(new_sections);
            config.masking.two_pass = true;
        }
    }
    if given.iter().any(|g| matches!(g.flag.long, "two-pass-mask-combine" | "two-pass-mode")) {
        config.masking.two_pass = true;
    }
    // The input of every configured section (a preset's or the default); `--mask` sections name
    // their own.
    if let Some(input) = one("masking-input") {
        if has("mask") {
            log::warn!(target: LOG_TARGET, "--masking-input is ignored when --mask is given (each --mask section names its own input)");
        } else {
            let input: qsmxt_engine::MaskingInput = serde_json::from_value(json(input)).map_err(|e| err(e.to_string()))?;
            for section in &mut config.masking.sections {
                section.input = input;
            }
        }
    }
    if let Some(v) = one("bet-fractional-intensity") {
        config.bet.fractional_intensity = v.parse().map_err(|e: std::num::ParseFloatError| err(e.to_string()))?;
    }
    if let Some(v) = one("bet-voxel-scale") {
        config.bet.voxel_scale = v.parse().map_err(|e: std::num::ParseFloatError| err(e.to_string()))?;
    }
    // BET's values live on each BET step of the recipe: pushed out last, so they reach the
    // sections a preset, `--mask` or a pipeline has just installed.
    let d = qsmxt_engine::BetConfig::default();
    if has("bet-fractional-intensity") || config.bet.fractional_intensity != d.fractional_intensity {
        let fi = config.bet.fractional_intensity;
        crate::pipeline::config::for_each_bet_op(&mut config.masking, |f, _| *f = fi);
    }
    if has("bet-voxel-scale") || config.bet.voxel_scale != d.voxel_scale {
        let scale = config.bet.voxel_scale;
        crate::pipeline::config::for_each_bet_op(&mut config.masking, |_, s| *s = scale);
    }
    Ok(())
}

/// `--dismiss-recommendation`'s rules, each refused if it names no rule or a reference rule.
fn apply_dismissals(config: &mut PipelineConfig, given: &[Given]) -> crate::Result<()> {
    let err = QsmxtError::Config;
    let all = |long: &str| given.iter().find(|g| g.flag.long == long).map(|g| g.values.clone());
    for d in all("dismiss-recommendation").unwrap_or_default() {
        let (rule, chosen) = d.split_once('=').ok_or_else(|| err(format!(
            "--dismiss-recommendation {d}: give the rule and the value it is dismissed for, as RULE=VALUE \
             (`qsmxt run --dry` lists them)")))?;
        if !qsmxt_engine::recommendations::is_rule(rule) {
            return Err(err(format!("--dismiss-recommendation {d}: there is no rule {rule}")));
        }
        if qsmxt_engine::recommendations::is_reference_rule(rule) {
            return Err(err(format!("--dismiss-recommendation {d}: {}",
                qsmxt_engine::recommendations::reference_rule_refusal(rule))));
        }
        config.dismissed_recommendations.retain(|x| x.rule != rule);
        config.dismissed_recommendations.push(qsmxt_engine::Dismissal { rule: rule.to_string(), chosen: chosen.to_string() });
    }
    Ok(())
}
