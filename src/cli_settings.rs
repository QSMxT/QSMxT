//! `qsmxt run`'s settings flags, built from the engine's flag table
//! ([`qsmxt_engine::schema::cli::PIPELINE_FLAGS`]).

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

/// `qsmxt run`'s settings: the flags given and, from the TUI, its form's settings by path, set
/// before the flags (the TUI passes no flag but its dismissed recommendations).
#[derive(Debug, Default)]
pub struct SettingsArgs {
    pub given: Vec<Given>,
    pub values: Vec<(&'static str, serde_json::Value)>,
}

impl SettingsArgs {
    /// Settings from flags as they are typed after `qsmxt run` (the tests' runs).
    #[cfg(test)]
    pub fn from_flags(argv: &[&str]) -> Self {
        let m = augment(Command::new("qsmxt")).try_get_matches_from(std::iter::once("qsmxt").chain(argv.iter().copied()))
            .unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        SettingsArgs { given: given(&m), values: Vec::new() }
    }

    /// These flags added, each in place of the same flag given before (the tests' runs).
    #[cfg(test)]
    pub fn add(&mut self, argv: &[&str]) -> &mut Self {
        let new = Self::from_flags(argv).given;
        self.given.retain(|g| !new.iter().any(|n| n.flag.long == g.flag.long));
        self.given.extend(new);
        self.given.sort_by_key(|g| PIPELINE_FLAGS.iter().position(|f| f.long == g.flag.long));
        self
    }
}

impl clap::Args for SettingsArgs {
    fn augment_args(cmd: Command) -> Command {
        augment(cmd)
    }
    fn augment_args_for_update(cmd: Command) -> Command {
        augment(cmd)
    }
}

impl clap::FromArgMatches for SettingsArgs {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        Ok(SettingsArgs { given: given(matches), values: Vec::new() })
    }
    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        self.given = given(matches);
        Ok(())
    }
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
    "qsmart-fit-threshold-mode", "qsmart-fit-percentile", "qsmart-fit-threshold", "laplacian-fft-pad",
];

/// The configuration `settings` make of `config`, as `qsmxt run` takes it.
pub fn build_config(mut config: PipelineConfig, settings: &SettingsArgs) -> crate::Result<PipelineConfig> {
    apply_normalised(&mut config, settings)?;
    crate::pipeline::config::validate_reference_region(&config)?;
    crate::pipeline::config::validate_segmentation(&config)?;
    crate::pipeline::config::validate_multi_orientation(&config)?;
    crate::pipeline::config::validate_inversion_input(&config)?;
    qsmxt_engine::schema::check_ranges(&config).map_err(|e| QsmxtError::Config(e.to_string()))?;
    Ok(config)
}

/// `settings` applied to `config`, with the outputs other settings need turned on.
pub fn apply_normalised(config: &mut PipelineConfig, settings: &SettingsArgs) -> crate::Result<()> {
    for (path, value) in &settings.values {
        qsmxt_engine::schema::set(config, path, value.clone()).map_err(QsmxtError::Config)?;
    }
    apply(config, &settings.given)?;
    crate::pipeline::config::normalise_logged(config);
    Ok(())
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
    let all = |long: &str| given.iter().find(|g| g.flag.long == long).map(|g| g.values.clone());
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

    if let Some(p) = all("laplacian-fft-pad") {
        let pad: Vec<usize> = p.iter().map(|v| v.parse()).collect::<Result<_, _>>().map_err(|e: std::num::ParseIntError| err(e.to_string()))?;
        config.field_mapping.laplacian_fft_pad = qsmxt_engine::pad3_from(&pad)
            .ok_or_else(|| err(format!("--laplacian-fft-pad {pad:?}: give one value or three (x y z)")))?;
    }
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
    use clap::Command;

    /// Values the flags refuse are refused with the words `qsmxt run` has always used.
    #[test]
    fn refusals_read_as_before() {
        let refusal = |argv: &str| -> String {
            let argv: Vec<&str> = argv.split(' ').collect();
            let m = super::augment(Command::new("t")).try_get_matches_from(std::iter::once("t").chain(argv.iter().copied()));
            let e = match m {
                Ok(m) => super::apply(&mut Default::default(), &super::given(&m)).expect_err("refused").to_string(),
                Err(e) => e.to_string(),
            };
            e.lines().next().unwrap_or("").to_string()
        };
        let config = "Pipeline configuration error: ";
        for (argv, said) in [
            ("--obliquity-threshold -1", format!("{config}--obliquity-threshold -1: the threshold is in degrees and cannot be negative. \
                Resampling is off unless --axial-resampling is given (a negative threshold no longer turns it off)")),
            ("--laplacian-fft-pad 2 3", format!("{config}--laplacian-fft-pad [2, 3]: give one value or three (x y z)")),
            ("--ilsqr-pad-mm -5", "error: unexpected argument '-5' found".into()),
            ("--motion-reference x", format!("{config}--motion-reference x: motion reference must be first, last or an echo number from 1, got \"x\"")),
            ("--mask magnitude,frobnicate:1", format!("{config}--mask magnitude,frobnicate:1: invalid mask op 'frobnicate:1': Config parse error: \
                Unknown mask-op: 'frobnicate'")),
            ("--mask-refine bet", format!("{config}--mask-refine bet:0.50:scale=1: it creates a mask rather than refining one; put it in a \
                --mask section instead")),
            ("--two-pass-mask nonsense", format!("{config}--two-pass-mask nonsense: 'nonsense' is not a mask input")),
            ("--dismiss-recommendation x", format!("{config}--dismiss-recommendation x: give the rule and the value it is dismissed for, as \
                RULE=VALUE (`qsmxt run --dry` lists them)")),
            ("--dismiss-recommendation nope=1", format!("{config}--dismiss-recommendation nope=1: there is no rule nope")),
            ("--qsmart-inversion heidi", format!("{config}--qsmart-inversion heidi: HEIDI is not one of QSMART's inner inversions (its two \
                stages are its own recipe)")),
            ("--synthseg-version v3", format!("{config}--synthseg-version v3: expected v1 or v2")),
            ("--medi-te 0", "error: invalid value '0' for '--medi-te <TE>': expected first, last, spacing, sum or an echo time in ms (> 0), \
                got '0'".into()),
            ("--tv-rho x", "error: invalid value 'x' for '--tv-rho <RHO>': expected a number or auto, got 'x'".into()),
            ("--lbv-precision quad", "error: invalid value 'quad' for '--lbv-precision <PRECISION>': expected double or single, got 'quad'".into()),
            ("--mask-preset mouse-bet", format!("{config}the `mouse-bet` mask preset has been removed. For rodent data use `--mask-preset \
                rs2-net`; BET on scaled voxel sizes is still available as a mask step, e.g. `--mask magnitude,bet:0.5:scale=10,erode:1`")),
            ("--pipeline heidi --orientation-group acq", format!("{config}--pipeline heidi is not available with \
                --orientation-group: HEIDI doesn't run on multi-orientation data")),
            ("--cosmos-tol 1", format!("{config}--cosmos-tol and --cosmos-max-iter set the conjugate gradient of magnitude-weighted COSMOS; \
                without --cosmos-magnitude-weighting COSMOS is solved in closed form and they would be ignored")),
        ] {
            assert_eq!(refusal(argv), said, "{argv}");
        }
    }

    /// Every registry setting's flag is a `qsmxt run` flag.
    #[test]
    fn every_settings_flag_is_a_qsmxt_run_flag() {
        use clap::CommandFactory;
        let cli = crate::cli::Cli::command();
        let run = cli.get_subcommands().find(|c| c.get_name() == "run").expect("qsmxt run");
        let longs: Vec<String> = run.get_arguments().filter_map(|a| a.get_long()).map(|l| format!("--{l}")).collect();
        for s in qsmxt_engine::schema::schema() {
            if let Some(f) = s.cli_flag {
                assert!(longs.iter().any(|l| l == f), "{}: {f} is not a qsmxt run option", s.path);
            }
        }
    }

    /// Every flag that sets no registry setting, and is not the opposite of a registry switch, is
    /// applied here by name: none is parsed and ignored.
    #[test]
    fn every_flag_is_applied() {
        use qsmxt_engine::schema::cli_apply::{opposite_of, setting_of};
        let src = include_str!("cli_settings.rs");
        let apply = &src[src.find("pub fn apply(").unwrap()..src.find("#[cfg(test)]\nmod tests").unwrap()];
        for f in super::PIPELINE_FLAGS.iter().filter(|f| setting_of(f).is_none() && opposite_of(f).is_none()) {
            assert!(apply.contains(&format!("\"{}\"", f.long)), "--{} is not applied", f.long);
        }
        for long in super::APPLIED_HERE {
            assert!(apply.contains(&format!("\"{long}\"")), "--{long} is listed as applied here but is not");
        }
    }
}
