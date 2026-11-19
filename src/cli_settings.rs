//! `qsmxt run`'s settings flags, built from the engine's flag table
//! ([`qsmxt_engine::schema::cli::PIPELINE_FLAGS`]). qsmxt-io applies them ([`qsmxt_io::settings`]).

use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{Arg, ArgAction, Command};
use qsmxt_engine::schema::cli::{Flag, FlagAction, FlagParse, PIPELINE_FLAGS};
use qsmxt_engine::schema::cli_apply::Given;
pub use qsmxt_io::settings::{build_config, RunSettings};
#[cfg(test)]
pub use qsmxt_io::settings::{apply, apply_normalised};

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

/// `qsmxt run`'s settings flags, parsed: qsmxt-io's [`RunSettings`] (the flags given and, from
/// the TUI, its form's settings by path), as clap reads them.
#[derive(Debug, Default)]
pub struct SettingsArgs(pub RunSettings);

impl std::ops::Deref for SettingsArgs {
    type Target = RunSettings;
    fn deref(&self) -> &RunSettings {
        &self.0
    }
}

impl std::ops::DerefMut for SettingsArgs {
    fn deref_mut(&mut self) -> &mut RunSettings {
        &mut self.0
    }
}

impl From<RunSettings> for SettingsArgs {
    fn from(settings: RunSettings) -> Self {
        SettingsArgs(settings)
    }
}

impl SettingsArgs {
    /// Settings from flags as they are typed after `qsmxt run` (the tests' runs).
    #[cfg(test)]
    pub fn from_flags(argv: &[&str]) -> Self {
        let m = augment(Command::new("qsmxt")).try_get_matches_from(std::iter::once("qsmxt").chain(argv.iter().copied()))
            .unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        SettingsArgs(RunSettings { given: given(&m), values: Vec::new() })
    }

    /// These flags added, each in place of the same flag given before (the tests' runs).
    #[cfg(test)]
    pub fn add(&mut self, argv: &[&str]) -> &mut Self {
        let new = Self::from_flags(argv).0.given;
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
        Ok(SettingsArgs(RunSettings { given: given(matches), values: Vec::new() }))
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
        let src = include_str!("../crates/qsmxt-io/src/settings.rs");
        let apply = &src[src.find("pub fn apply(").unwrap()..src.len()];
        for f in super::PIPELINE_FLAGS.iter().filter(|f| setting_of(f).is_none() && opposite_of(f).is_none()) {
            assert!(apply.contains(&format!("\"{}\"", f.long)), "--{} is not applied", f.long);
        }
        for long in qsmxt_io::settings::APPLIED_HERE {
            assert!(apply.contains(&format!("\"{long}\"")), "--{long} is listed as applied here but is not");
        }
    }
}
