//! `qsmxt run`'s settings flags as data: what each is called, what it accepts and its help, in
//! the order `--help` lists them ([`PIPELINE_FLAGS`]). `qsmxt` builds its command line from this
//! table; the settings they set are the registry's ([`super::setting`], by [`Flag::long`]).

pub use super::cli_flags::PIPELINE_FLAGS;

/// What giving a flag does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagAction {
    /// Takes its value(s); given twice, the last wins.
    Set,
    /// A switch: on when given.
    SetTrue,
    /// Takes its value(s), collecting every time it is given.
    Append,
}

/// How a flag's value is read, as the command line checks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagParse {
    /// A switch, or one of [`Flag::values`].
    Listed,
    F64,
    Usize,
    I64,
    I32,
    U8,
    /// A whole number of 1 or more.
    PositiveU64,
    Text,
    /// A number, or `auto` for the value the method derives.
    NumberOrAuto,
    /// An echo time ([`crate::EchoTimeChoice::parse`]).
    EchoTime,
    /// DECOMPOSE's χ bound ([`crate::ChiBoundChoice::parse`]).
    ChiBound,
    /// One of the registry setting's options, by name.
    Named,
}

/// A value a flag accepts from a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagValue {
    pub name: &'static str,
    pub help: Option<&'static str>,
    /// Still accepted, not listed (a removed value that gets an explanation).
    pub hide: bool,
    pub aliases: &'static [&'static str],
}

/// One flag of the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flag {
    /// Its id, which `conflicts` and `requires` name.
    pub id: &'static str,
    /// Without the leading `--`.
    pub long: &'static str,
    /// Hidden aliases.
    pub aliases: &'static [&'static str],
    pub visible_aliases: &'static [&'static str],
    pub value_names: &'static [&'static str],
    /// How many values it takes: (fewest, most); `usize::MAX` is unbounded.
    pub num_args: (usize, usize),
    pub action: FlagAction,
    pub parse: FlagParse,
    /// The short help (`-h`), and the long help (`--help`) where it differs.
    pub help: Option<&'static str>,
    pub long_help: Option<&'static str>,
    /// The values it accepts, when it accepts only some.
    pub values: &'static [FlagValue],
    pub defaults: &'static [&'static str],
    /// The value when it is given without one.
    pub default_missing: &'static [&'static str],
    /// The ids it cannot be given with, and those it needs.
    pub conflicts: &'static [&'static str],
    pub requires: &'static [&'static str],
    /// Parsing and help switches, by clap's names: `hide`, `allow_negative_numbers`,
    /// `allow_hyphen_values`, `hide_possible_values`, `hide_default_value`.
    pub attrs: &'static [&'static str],
}

impl Flag {
    pub fn has(&self, attr: &str) -> bool {
        self.attrs.contains(&attr)
    }
}

/// The flags that set something other than one registry setting to the value given: the
/// opposite direction of a registry switch, and the flags `qsmxt` applies itself (a pipeline to
/// start from, the mask grammar and presets, the BET overrides, dismissals).
pub const NOT_ONE_SETTING: &[&str] = &[
    // The opposite of a registry switch.
    "no-bf-polish-mask", "no-bf-ukb-mask-trim", "no-laplacian-unmasked", "no-laplacian-keep-outside",
    "no-heidi-denoise-reference-itk", "no-romeo-mag-weight", "no-romeo-individual", "no-romeo-correct-global",
    "no-axial-resampling", "medi-smv", "medi-smv-shrink-mask", "romeo-phase-linearity", "inhomogeneity-correction",
    // Applied by qsmxt itself.
    "pipeline", "masking-input", "bet-fractional-intensity", "bet-voxel-scale", "mask-preset", "mask", "mask-combine",
    "mask-refine", "two-pass-mask", "dismiss-recommendation",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registry setting's flag is in the table, and every flag in the table sets one
    /// registry setting unless it is listed in [`NOT_ONE_SETTING`].
    #[test]
    fn the_table_and_the_registry_agree() {
        let registry: Vec<&str> = super::super::settings().iter().filter_map(|s| s.cli_flag).map(|f| f.trim_start_matches("--")).collect();
        let longs: Vec<&str> = PIPELINE_FLAGS.iter().map(|f| f.long).collect();
        for r in &registry {
            assert!(longs.contains(r), "--{r} is a registry flag but not in PIPELINE_FLAGS");
        }
        for l in &longs {
            assert_eq!(registry.contains(l), !NOT_ONE_SETTING.contains(l), "--{l}");
        }
    }
}
