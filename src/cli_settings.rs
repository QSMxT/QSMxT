//! `qsmxt run`'s settings flags, built from the engine's flag table
//! ([`qsmxt_engine::schema::cli::PIPELINE_FLAGS`]).

// Not wired up yet: `run`, `slurm` and `aggregate` switch to it next (refactor P3).
#![cfg_attr(not(test), allow(dead_code))]

use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{Arg, ArgAction, Command};
use qsmxt_engine::schema::cli::{Flag, FlagAction, FlagParse, PIPELINE_FLAGS};
use qsmxt_engine::schema::cli_apply::Given;
use qsmxt_engine::PipelineConfig;

use crate::error::QsmxtError;

/// One flag as a clap argument.
pub fn arg(f: &Flag) -> Arg {
    let mut a = Arg::new(f.id).long(f.long).num_args(f.num_args.0..=f.num_args.1).action(match f.action {
        FlagAction::Set => ArgAction::Set,
        FlagAction::SetTrue => ArgAction::SetTrue,
        FlagAction::Append => ArgAction::Append,
    });
    if f.num_args.1 == usize::MAX {
        a = a.num_args(f.num_args.0..);
    }
    if !f.value_names.is_empty() {
        a = a.value_names(f.value_names);
    }
    if let Some(h) = f.help {
        a = a.help(h);
    }
    if let Some(h) = f.long_help {
        a = a.long_help(h);
    }
    if !f.aliases.is_empty() {
        a = a.aliases(f.aliases);
    }
    if !f.visible_aliases.is_empty() {
        a = a.visible_aliases(f.visible_aliases);
    }
    a = match f.parse {
        FlagParse::F64 => a.value_parser(clap::value_parser!(f64)),
        FlagParse::Usize => a.value_parser(clap::value_parser!(usize)),
        FlagParse::I64 => a.value_parser(clap::value_parser!(i64)),
        FlagParse::I32 => a.value_parser(clap::value_parser!(i32)),
        FlagParse::U8 => a.value_parser(clap::value_parser!(u8)),
        FlagParse::PositiveU64 => a.value_parser(clap::value_parser!(u64).range(1..)),
        FlagParse::NumberOrAuto => a.value_parser(crate::cli::parse_or_auto),
        FlagParse::EchoTime => a.value_parser(crate::cli::parse_echo_time),
        FlagParse::ChiBound => a.value_parser(crate::cli::parse_chi_bound),
        FlagParse::Named => {
            let setting = qsmxt_engine::schema::cli_apply::setting_of(f).expect("a named value is a registry setting's");
            a.value_parser(move |s: &str| {
                let found = setting.value_type.options().iter().any(|o| o.value.as_str() == Some(s.trim()));
                if found { Ok(s.to_string()) } else { Err(qsmxt_engine::schema::cli_apply::expected(setting, s)) }
            })
        }
        FlagParse::Text | FlagParse::Listed => a,
    };
    if !f.values.is_empty() {
        a = a.value_parser(PossibleValuesParser::new(f.values.iter().map(|v| {
            let mut p = PossibleValue::new(v.name).hide(v.hide).aliases(v.aliases.iter().copied());
            if let Some(h) = v.help {
                p = p.help(h);
            }
            p
        })));
    }
    if !f.defaults.is_empty() {
        a = a.default_values(f.defaults);
    }
    if !f.default_missing.is_empty() {
        a = a.default_missing_values(f.default_missing);
    }
    for c in f.conflicts {
        a = a.conflicts_with(c);
    }
    for r in f.requires {
        a = a.requires(r);
    }
    a.hide(f.has("hide"))
        .allow_hyphen_values(f.has("allow_hyphen_values"))
        .allow_negative_numbers(f.has("allow_negative_numbers"))
        .hide_possible_values(f.has("hide_possible_values"))
        .hide_default_value(f.has("hide_default_value"))
}

/// The settings flags given on the command line, in the table's order, with their values as
/// typed.
pub fn given(matches: &clap::ArgMatches) -> Vec<Given> {
    PIPELINE_FLAGS.iter().filter(|f| matches.value_source(f.id) == Some(clap::parser::ValueSource::CommandLine)).map(|flag| {
        let values = matches.get_raw(flag.id).map(|v| v.map(|s| s.to_string_lossy().into_owned()).collect()).unwrap_or_default();
        Given { flag, values }
    }).collect()
}

/// The registry flags applied here rather than as one value of one setting.
const APPLIED_HERE: &[&str] = &[
    "pipeline", "qsm-algorithm", "qsm-reference", "romeo-template", "synthseg-version",
    "qsmart-fit-threshold-mode", "qsmart-fit-percentile", "qsmart-fit-threshold",
];

/// The configuration `given` makes of `config`, as `qsmxt run` takes it.
pub fn build_config(mut config: PipelineConfig, given: &[Given]) -> crate::Result<PipelineConfig> {
    apply(&mut config, given)?;
    crate::pipeline::config::normalise_logged(&mut config);
    crate::pipeline::config::validate_reference_region(&config)?;
    crate::pipeline::config::validate_segmentation(&config)?;
    crate::pipeline::config::validate_multi_orientation(&config)?;
    crate::pipeline::config::validate_inversion_input(&config)?;
    qsmxt_engine::schema::check_ranges(&config).map_err(|e| QsmxtError::Config(e.to_string()))?;
    Ok(config)
}

/// `given` applied to `config`, as the flags give it (no outputs turned on for other settings).
pub fn apply(config: &mut PipelineConfig, given: &[Given]) -> crate::Result<()> {
    let err = QsmxtError::Config;
    let one = |long: &str| given.iter().find(|g| g.flag.long == long).and_then(|g| g.values.first()).map(String::as_str);
    let all = |long: &str| given.iter().find(|g| g.flag.long == long).map(|g| g.values.clone());
    let set = |c: &mut PipelineConfig, path: &str, v: serde_json::Value| qsmxt_engine::schema::set(c, path, v).map_err(err);
    let json = |s: &str| serde_json::Value::String(s.to_string());

    // A pipeline first, so that every other flag changes its settings.
    if let Some(id) = one("pipeline") {
        qsmxt_engine::pipelines::choose(config, id, &qsmxt_engine::schema::ViewFacts::default()).map_err(err)?;
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
    let settings: Vec<Given> = given.iter().filter(|g| !APPLIED_HERE.contains(&g.flag.long)).cloned().collect();
    let rest = qsmxt_engine::schema::cli_apply::apply_settings(config, &settings).map_err(err)?;
    config.multi_orientation.validate().map_err(err)?;

    if let Some(t) = one("romeo-template") {
        config.field_mapping.romeo.template = t.parse::<usize>().map_err(|e| err(e.to_string()))?.saturating_sub(1);
    }
    {
        use clap::ValueEnum;
        let mode = one("qsmart-fit-threshold-mode").map(|m| crate::cli::QsmartFitThresholdModeArg::from_str(m, false).map_err(err)).transpose()?;
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
                log::warn!("{warning}");
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
            log::warn!("--masking-input is ignored when --mask is given (each --mask section names its own input)");
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

/// `cmd` with every settings flag added, in the table's order.
pub fn augment(cmd: Command) -> Command {
    cmd.args(PIPELINE_FLAGS.iter().map(arg))
}

#[cfg(test)]
mod tests {
    use clap::{Args, Command};

    /// The flags built from the table are today's `PipelineArgs`: the same arguments, and the same
    /// help.
    #[test]
    fn the_table_builds_pipeline_args() {
        let build = |mut c: Command| { c = c.term_width(100).max_term_width(100); c.build(); c };
        let mut derived = build(crate::cli::PipelineArgs::augment_args(Command::new("t")));
        let mut table = build(super::augment(Command::new("t")));
        let lines = |c: &Command| c.get_arguments().map(crate::cli_snapshots::describe).collect::<Vec<_>>();
        let (d, t) = (lines(&derived), lines(&table));
        for (a, b) in d.iter().zip(&t) {
            assert_eq!(a, b);
        }
        assert_eq!(d.len(), t.len());
        // From the usage on: derive adds the struct's doc as the command's about.
        let from_usage = |h: String| h[h.find("Usage:").unwrap()..].to_string();
        assert_eq!(from_usage(derived.render_long_help().to_string()), from_usage(table.render_long_help().to_string()));
        assert_eq!(from_usage(derived.render_help().to_string()), from_usage(table.render_help().to_string()));
    }
}
