//! `qsmxt run`'s settings flags as data: what each is called, what it accepts and its help, in
//! the order `--help` lists them ([`PIPELINE_FLAGS`]). `qsmxt` builds its command line from this
//! table.
//!
//! A registry setting's flag is derived from its entry ([`super::table`]): `.flag("--x")` names it,
//! and the setting's type says what it takes ([`derive`]): a switch for a yes/no setting, one
//! number for a number, three for a vector, one of the options for a choice. The flag's help is
//! the entry's front-end help with its units, after the method it belongs to (`RTS: ...`), unless
//! `.cli(..)` words it for the command line; either way it ends with the setting's default, taken
//! from the registry ([`super::Setting::default`]), so the wording does not name it. The builder methods below
//! give the rest where it differs from what the type implies: the value's name, how it is read,
//! a value list of its own, the opposite switch (`.negated("--no-x", ..)`), and so on.
//!
//! The flags that set no one setting are [`OTHER_FLAGS`]. Every flag takes its place from the
//! registry's order ([`build`]), in `--help` and in a generated command.

use std::sync::LazyLock;

use super::{defs, Def, Ty};

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

// ─── A registry entry's flag ───

/// What a registry entry's flag says where the setting's type does not imply it ([`derive`]).
#[derive(Default)]
pub(super) struct FlagSpec {
    id: Option<&'static str>,
    pub(super) help: Option<&'static str>,
    value_names: Option<&'static [&'static str]>,
    num_args: Option<(usize, usize)>,
    parse: Option<FlagParse>,
    bool_value: bool,
    values: Option<&'static [&'static str]>,
    value_help: Vec<(&'static str, &'static str)>,
    value_aliases: Vec<(&'static str, &'static [&'static str])>,
    aliases: &'static [&'static str],
    default_missing: &'static [&'static str],
    conflicts: &'static [&'static str],
    requires: &'static [&'static str],
    attrs: &'static [&'static str],
    /// The opposite switch (`--no-x` of `--x`) and its help.
    negated: Option<(&'static str, &'static str)>,
    negated_conflicts: bool,
    /// The default as the flag's help gives it, where the flag counts otherwise than the config
    /// (`--romeo-template` counts echoes from 1).
    pub(super) default: Option<&'static str>,
}

impl Def {
    /// The flag's help, where the command line words it differently from the entry's help.
    pub(super) fn cli(mut self, help: &'static str) -> Self { self.cli.help = Some(help); self }
    /// The flag's id, where it is not its name with `_` for `-`.
    pub(super) fn cli_id(mut self, id: &'static str) -> Self { self.cli.id = Some(id); self }
    /// What `--help` calls the value (default: the id in capitals; `X Y Z` for three whole
    /// numbers; `TE` for an echo time).
    pub(super) fn value_name(self, name: &'static str) -> Self { self.value_names(one(name)) }
    pub(super) fn value_names(mut self, names: &'static [&'static str]) -> Self { self.cli.value_names = Some(names); self }
    /// How many values the flag takes, where it is not what the type takes.
    pub(super) fn num_args(mut self, fewest: usize, most: usize) -> Self { self.cli.num_args = Some((fewest, most)); self }
    /// How the value is read, where it is not what the type implies.
    pub(super) fn parse(mut self, p: FlagParse) -> Self { self.cli.parse = Some(p); self }
    /// A yes/no setting given as `--x true` or `--x false` rather than as a switch.
    pub(super) fn bool_value(mut self) -> Self { self.cli.bool_value = true; self }
    /// The values the flag lists, where they are not the setting's options.
    pub(super) fn cli_values(mut self, names: &'static [&'static str]) -> Self { self.cli.values = Some(names); self }
    pub(super) fn value_help(mut self, name: &'static str, help: &'static str) -> Self { self.cli.value_help.push((name, help)); self }
    /// Other spellings a listed value is accepted under, unlisted.
    pub(super) fn value_alias(mut self, name: &'static str, aliases: &'static [&'static str]) -> Self {
        self.cli.value_aliases.push((name, aliases));
        self
    }
    /// Other names the flag is accepted under, unlisted.
    pub(super) fn flag_aliases(mut self, a: &'static [&'static str]) -> Self { self.cli.aliases = a; self }
    /// The value the flag takes when it is given without one (it then takes none or one).
    pub(super) fn default_missing(mut self, v: &'static str) -> Self { self.cli.default_missing = one(v); self }
    /// The flags (by id) it cannot be given with, and those it needs.
    pub(super) fn conflicts(mut self, ids: &'static [&'static str]) -> Self { self.cli.conflicts = ids; self }
    pub(super) fn requires(mut self, ids: &'static [&'static str]) -> Self { self.cli.requires = ids; self }
    /// Parsing switches, by clap's names ([`Flag::attrs`]).
    pub(super) fn attrs(mut self, a: &'static [&'static str]) -> Self { self.cli.attrs = a; self }
    /// The switch's opposite (`--no-x` of `--x`, or `--x` of `--no-x`), with its help.
    pub(super) fn negated(mut self, long: &'static str, help: &'static str) -> Self { self.cli.negated = Some((long, help)); self }
    /// The default the flag's help names, where the flag counts otherwise than the config.
    pub(super) fn cli_default(mut self, words: &'static str) -> Self { self.cli.default = Some(words); self }
    /// The opposite cannot be given with the switch.
    pub(super) fn negated_conflicts(mut self) -> Self { self.cli.negated_conflicts = true; self }
}

/// A string made once for the table, which lives as long as the program.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn one(s: &'static str) -> &'static [&'static str] {
    Box::leak(Box::new([s]))
}

/// A setting's flag: what its type implies, then what its entry overrides.
fn derive(d: &'static Def) -> Flag {
    use FlagAction::*;
    use FlagParse as P;
    let s = &d.cli;
    let long = d.flag.expect("a flag").trim_start_matches("--");
    let id = s.id.unwrap_or_else(|| leak(long.replace('-', "_")));
    // A whole number whose range goes below zero (padding, where -1 means "derive it") is read
    // signed and may start with a hyphen.
    let signed = d.min.is_some_and(|m| m < 0.0) && matches!(d.ty, Ty::Int | Ty::IntVec3);
    let whole = if signed { P::I64 } else { P::Usize };
    let (action, num_args, parse) = match &d.ty {
        Ty::Bool if s.bool_value => (Set, (1, 1), P::Listed),
        Ty::Bool => (SetTrue, (0, 0), P::Listed),
        Ty::Int => (Set, (1, 1), whole),
        Ty::Float => (Set, (1, 1), P::F64),
        Ty::IntVec3 => (Append, (3, 3), whole),
        Ty::FloatVec3 => (Append, (3, 3), P::F64),
        Ty::IntList => (Append, (1, usize::MAX), P::Usize),
        Ty::Text | Ty::EchoReference(_) => (Set, (1, 1), P::Text),
        // Numbered choices (MEDI's data weighting) are given as their number.
        Ty::Enum(o) if o.iter().all(|(v, ..)| v.is_number()) => (Set, (1, 1), P::I32),
        Ty::Enum(_) | Ty::Regions => (Set, (1, 1), P::Listed),
        Ty::EchoTime => (Set, (1, 1), P::EchoTime),
        Ty::NumberOr(_) => (Set, (1, 1), P::ChiBound),
        Ty::MaskSections | Ty::MaskOps => panic!("{}: a mask recipe is given with its own flags", d.path),
    };
    let parse = s.parse.unwrap_or(parse);
    let num_args = s.num_args.unwrap_or(if s.default_missing.is_empty() { num_args } else { (0, 1) });
    let names: Vec<&'static str> = match (&d.ty, s.values) {
        _ if parse != P::Listed => vec![],
        (_, Some(v)) => v.to_vec(),
        (Ty::Bool, _) if s.bool_value => vec!["true", "false"],
        (Ty::Enum(o), _) => o.iter().map(|(v, ..)| v.as_str().expect("a named option")).collect(),
        _ => vec![],
    };
    let values: Vec<FlagValue> = names.into_iter().map(|name| FlagValue {
        name,
        help: s.value_help.iter().find(|(n, _)| *n == name).map(|(_, h)| *h),
        hide: false,
        aliases: s.value_aliases.iter().find(|(n, _)| *n == name).map_or(&[], |(_, a)| *a),
    }).collect();
    let value_names = s.value_names.unwrap_or(match d.ty {
        Ty::IntVec3 => &["X", "Y", "Z"],
        Ty::EchoTime => &["TE"],
        _ => one(leak(id.to_uppercase())),
    });
    let switch = action == SetTrue;
    Flag {
        id,
        long,
        aliases: s.aliases,
        visible_aliases: &[],
        value_names,
        num_args,
        action,
        parse,
        help: Some(flag_help(d, switch)),
        long_help: None,
        values: Box::leak(values.into_boxed_slice()),
        defaults: if switch { &["false"] } else { &[] },
        default_missing: if switch { &["true"] } else { s.default_missing },
        conflicts: s.conflicts,
        requires: s.requires,
        attrs: if s.attrs.is_empty() && signed { &["allow_negative_numbers"] } else { s.attrs },
    }
}

/// A setting's flag help: its `.cli(..)` wording, else the front-end help with its units, after
/// the method it belongs to (`RTS: ...`), then the default, from the setting's own (`(default: 0 0 0)`) unless
/// the text already says what it is. A switch is off unless given, so it names none.
fn flag_help(d: &'static Def, switch: bool) -> &'static str {
    let setting = super::settings().iter().find(|s| s.path == d.path).expect("every entry is a setting");
    let text = match d.cli.help {
        Some(h) => h.to_string(),
        None => {
            // The front-end help leaves the units to the schema; the command line has no other
            // place to say them.
            let help = d.gui_help.unwrap_or(d.help);
            let help = match setting.units {
                Some(u) if !help.contains(u) => format!("{help}, in {u}"),
                _ => help.to_string(),
            };
            match setting.context {
                // "RTS: ADMM penalty parameter", but "LSQR convergence tolerance" under LSQR.
                Some(context) if !help.starts_with(context) => format!("{context}: {help}"),
                _ => help,
            }
        }
    };
    let rendered = match d.cli.default {
        Some(words) => Some(words.to_string()),
        None if switch => None,
        None => render_default(&setting.default),
    };
    match rendered {
        Some(r) if !text.to_lowercase().contains("default") => leak(format!("{text} (default: {r})")),
        _ => leak(text),
    }
}

/// A default as the command line takes it: `0 0 0`, `0.0003`, `1e-5`, `vsharp`, `true`. `None`
/// for a value with no words of its own (unset, empty), which a setting's help describes.
pub(crate) fn render_default(v: &serde_json::Value) -> Option<String> {
    use serde_json::Value as V;
    let number = |x: f64| {
        if x.fract() == 0.0 && x.abs() < 1e15 {
            format!("{}", x as i64)
        } else {
            let plain = format!("{x}");
            if plain.len() <= 6 { plain } else { format!("{x:e}") }
        }
    };
    match v {
        V::Bool(b) => Some(b.to_string()),
        V::Number(n) => n.as_f64().map(number),
        V::String(s) if !s.is_empty() => Some(s.clone()),
        V::Array(a) if !a.is_empty() => a.iter().map(|x| x.as_f64().map(number)).collect::<Option<Vec<_>>>().map(|w| w.join(" ")),
        _ => None,
    }
}

/// The opposite of `switch`: `long`, with `help`.
fn opposite(switch: &Flag, long: &'static str, help: &'static str, conflicts: bool) -> Flag {
    let long = long.trim_start_matches("--");
    let id = leak(long.replace('-', "_"));
    Flag {
        id,
        long,
        aliases: &[],
        visible_aliases: &[],
        value_names: one(leak(id.to_uppercase())),
        num_args: (0, 0),
        action: FlagAction::SetTrue,
        parse: FlagParse::Listed,
        help: Some(help),
        long_help: None,
        values: &[],
        defaults: &["false"],
        default_missing: &["true"],
        conflicts: if conflicts { one(switch.id) } else { &[] },
        requires: &[],
        attrs: &[],
    }
}

/// Where the flags that set no one setting ([`OTHER_FLAGS`]) go among the registry's: each
/// before the flag of the setting named, or where that setting's flag would be when it has none.
/// `--pipeline` comes first and `--dismiss-recommendation` last.
const PLACES: &[(&str, &str)] = &[
    ("mask-preset", "masking.sections"), ("masking-input", "masking.sections"), ("mask", "masking.sections"),
    ("mask-combine", "masking.combine"), ("mask-refine", "masking.refinements"),
    ("two-pass-mask", "masking.two_pass_sections"),
    ("bet-fractional-intensity", "bet.smoothness"), ("bet-voxel-scale", "bet.smoothness"),
];

/// Every flag: the registry's in its order (the TUI's, which groups each method's settings after
/// the setting that chooses it), each opposite switch after its switch, and [`OTHER_FLAGS`] at
/// their [`PLACES`]. `--help` lists them in this order and a generated command says them in it.
fn build() -> Vec<Flag> {
    let other = |long: &str| *OTHER_FLAGS.iter().find(|f| f.long == long).unwrap_or_else(|| panic!("--{long} is no flag"));
    let mut flags: Vec<Flag> = vec![other("pipeline")];
    for d in defs() {
        flags.extend(PLACES.iter().filter(|(_, at)| *at == d.path).map(|(long, _)| other(long)));
        // A flag two entries name (the reference and its region) is the first one's.
        let Some(long) = d.flag.map(|l| l.trim_start_matches("--")) else { continue };
        if flags.iter().any(|f| f.long == long) {
            continue;
        }
        let f = derive(d);
        flags.push(f);
        if let Some((long, help)) = d.cli.negated {
            flags.push(opposite(&f, long, help, d.cli.negated_conflicts));
        }
    }
    flags.push(other("dismiss-recommendation"));
    for f in OTHER_FLAGS {
        assert_eq!(flags.iter().filter(|g| g.long == f.long).count(), 1, "--{} has one place", f.long);
    }
    flags
}

/// `qsmxt run`'s settings flags, in the order `--help` lists them: built once from the registry
/// and [`OTHER_FLAGS`] ([`build`]).
pub static PIPELINE_FLAGS: LazyLock<Vec<Flag>> = LazyLock::new(build);

/// The flags that set no one registry setting: `qsmxt` applies them itself (a pipeline to start
/// from, the mask grammar and presets, the BET overrides, dismissals).
pub const OTHER_FLAGS: &[Flag] = {
    use FlagAction::*;
    use FlagParse as P;
    use FlagValue as V;
    &[
        Flag { id: "pipeline", long: "pipeline", aliases: &[], visible_aliases: &[], value_names: &["PIPELINE"], num_args: (1, 1), action: Set, parse: P::Listed,
               help: Some("Start from a pipeline: qsmxt-defaults, or an original toolbox's (medi-toolbox, sti-suite-ilsqr, sti-suite-star, qsmart, heidi, chi-sep-ilsqr, chi-sep-medi, chi-sepnet, decompose). The stages it covers (masking, field mapping, background removal, inversion, referencing, χ-separation) go back to QSMxT's defaults and then take the toolbox's settings; the other flags given then change any setting"), long_help: None,
               values: &[V { name: "qsmxt-defaults", help: None, hide: false, aliases: &[] }, V { name: "medi-toolbox", help: None, hide: false, aliases: &[] }, V { name: "sti-suite-ilsqr", help: None, hide: false, aliases: &[] }, V { name: "sti-suite-star", help: None, hide: false, aliases: &[] }, V { name: "qsmart", help: None, hide: false, aliases: &[] }, V { name: "heidi", help: None, hide: false, aliases: &[] }, V { name: "chi-sep-ilsqr", help: None, hide: false, aliases: &[] }, V { name: "chi-sep-medi", help: None, hide: false, aliases: &[] }, V { name: "chi-sepnet", help: None, hide: false, aliases: &[] }, V { name: "decompose", help: None, hide: false, aliases: &[] }, V { name: "uk-biobank", help: None, hide: false, aliases: &[] }], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "masking_input", long: "masking-input", aliases: &[], visible_aliases: &[], value_names: &["MASKING_INPUT"], num_args: (1, 1), action: Set, parse: P::Listed,
               help: Some("Masking input image (overrides the input of the configured mask sections; combine with --mask-preset, e.g. --mask-preset robust-threshold --masking-input magnitude)"), long_help: None,
               values: &[V { name: "magnitude-first", help: None, hide: false, aliases: &[] }, V { name: "magnitude", help: None, hide: false, aliases: &[] }, V { name: "magnitude-last", help: None, hide: false, aliases: &[] }, V { name: "phase-quality", help: None, hide: false, aliases: &[] }], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "bet_fractional_intensity", long: "bet-fractional-intensity", aliases: &[], visible_aliases: &[], value_names: &["BET_FRACTIONAL_INTENSITY"], num_args: (1, 1), action: Set, parse: P::F64,
               help: Some("BET fractional intensity (0.0-1.0; smaller = larger brain). Applies to every BET step in the masking recipe, overriding a `bet:<f>` in a --mask spec"), long_help: None,
               values: &[], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "bet_voxel_scale", long: "bet-voxel-scale", aliases: &[], visible_aliases: &[], value_names: &["FACTOR"], num_args: (1, 1), action: Set, parse: P::F64,
               help: Some("Factor applied to the voxel sizes before BET runs. BET's surface model is tuned to a human brain in millimetres, so preclinical data has to be presented at human scale: 10 is the usual setting for mouse. 1 (the default) is the acquisition's own geometry. Applies to every BET step in the masking recipe"), long_help: None,
               values: &[], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "mask_preset", long: "mask-preset", aliases: &[], visible_aliases: &[], value_names: &["MASK_PRESET"], num_args: (1, 1), action: Set, parse: P::Listed,
               help: Some("Mask preset: robust-threshold, bet, hd-bet (HD-BET deep-learning brain extraction + signal-gated erosion; needs a deep-learning build), bet-and-phase (BET on the magnitude AND a thresholded phase-quality map, then hole-filled and eroded), rs2-net (RS2-Net deep-learning rodent brain extraction), or mindgrab (MindGrab deep-learning brain extraction for any contrast; needs a deep-learning build)"), long_help: None,
               values: &[V { name: "robust-threshold", help: None, hide: false, aliases: &[] }, V { name: "bet", help: None, hide: false, aliases: &[] }, V { name: "hd-bet", help: None, hide: false, aliases: &[] }, V { name: "bet-and-phase", help: None, hide: false, aliases: &[] }, V { name: "rs2-net", help: None, hide: false, aliases: &[] }, V { name: "mindgrab", help: None, hide: false, aliases: &[] }, V { name: "mouse-bet", help: Some("Removed. Still parsed, and hidden, so a script that asks for it gets an error that says what to use instead rather than clap's bare \"invalid value\""), hide: true, aliases: &[] }], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "mask_sections_cli", long: "mask", aliases: &[], visible_aliases: &[], value_names: &["MASK_SECTIONS_CLI"], num_args: (1, 1), action: Append, parse: P::Text,
               help: Some("Define a mask section (repeatable; multiple sections are combined with --mask-combine). Format: <input>,<generator>,<refinement1>,<refinement2>,... Example: phase-quality,threshold:otsu,dilate:2,fill-holes:0,erode:2 Example: magnitude,bet:0.5,erode:2 Example: magnitude-first,bet:0.2,min-intensity:100,majority,close:2   (QSMART's mask) Example: magnitude,hd-bet,signal-erode   (HD-BET: `hd-bet[:full|window|XxYxZ|low-memory][:tta][:step=F]`; signal-erode: `signal-erode[:threshold[:depth_cap[:global_erosions[:bias_sigma[:min_component]]]]]`)"), long_help: None,
               values: &[], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "mask_combine", long: "mask-combine", aliases: &[], visible_aliases: &[], value_names: &["MASK_COMBINE"], num_args: (1, 1), action: Set, parse: P::Listed,
               help: Some("How to combine multiple --mask sections: `or` (union, the default) or `and` (intersection — a voxel survives only where every section keeps it)"), long_help: None,
               values: &[V { name: "or", help: Some("Union — keep a voxel any section keeps (default)"), hide: false, aliases: &[] }, V { name: "and", help: Some("Intersection — keep only voxels every section keeps"), hide: false, aliases: &[] }], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "mask_refinements_cli", long: "mask-refine", aliases: &[], visible_aliases: &[], value_names: &["MASK_REFINEMENTS_CLI"], num_args: (1, 1), action: Append, parse: P::Text,
               help: Some("Refinement applied to the combined mask, after --mask-combine (repeatable). Takes the same refinement ops as --mask, e.g. `erode:2` or `fill-holes:0`; generators (threshold/bet/hd-bet) belong in a --mask section instead. Example: --mask-combine and --mask-refine fill-holes:0 --mask-refine erode:1"), long_help: None,
               values: &[], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "two_pass_sections_cli", long: "two-pass-mask", aliases: &[], visible_aliases: &[], value_names: &["TWO_PASS_SECTIONS_CLI"], num_args: (1, 1), action: Append, parse: P::Text,
               help: Some("Define the reliable-pass mask used by --two-pass (repeatable; same format as --mask). Defaults to `phase-quality,threshold:otsu` — an unfilled threshold on the phase-quality map. Its holes are the point, so refinements that close them (fill-holes, close, gaussian) defeat the method. Folded with --two-pass-mask-combine, then --mask-refine like the main mask, and always intersected with it"), long_help: None,
               values: &[], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
        Flag { id: "dismiss_recommendation", long: "dismiss-recommendation", aliases: &[], visible_aliases: &[], value_names: &["RULE=VALUE"], num_args: (1, 1), action: Append, parse: P::Text,
               help: Some("Stop offering a recommendation for the setting's current value: RULE=VALUE, the rule's id and the value it was dismissed for, as `qsmxt run --dry` lists them (e.g. field-mapping.phase-offsets=false). Repeatable. A change to the setting brings the recommendation back. A required one cannot be dismissed, and a reference-pipeline difference has nothing to dismiss"), long_help: None,
               values: &[], defaults: &[], default_missing: &[], conflicts: &[], requires: &[], attrs: &[] },
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registry setting's flag is in the table once, and every other flag is the opposite
    /// of a registry switch or one of [`OTHER_FLAGS`].
    #[test]
    fn the_table_and_the_registry_agree() {
        let registry: Vec<&str> = super::super::settings().iter().filter_map(|s| s.cli_flag).map(|f| f.trim_start_matches("--")).collect();
        let opposites: Vec<&str> = defs().iter().filter_map(|d| d.cli.negated).map(|(l, _)| l.trim_start_matches("--")).collect();
        let others: Vec<&str> = OTHER_FLAGS.iter().map(|f| f.long).collect();
        for r in &registry {
            assert_eq!(PIPELINE_FLAGS.iter().filter(|f| f.long == *r).count(), 1, "--{r}");
        }
        for f in PIPELINE_FLAGS.iter() {
            let kinds = [registry.contains(&f.long), opposites.contains(&f.long), others.contains(&f.long)];
            assert_eq!(kinds.iter().filter(|k| **k).count(), 1, "--{}", f.long);
        }
    }

    /// The flags come in the registry's order, each opposite after its switch, with
    /// `--pipeline` first and `--dismiss-recommendation` last.
    #[test]
    fn the_flags_follow_the_registry() {
        let longs: Vec<&str> = PIPELINE_FLAGS.iter().map(|f| f.long).collect();
        assert_eq!(longs.first(), Some(&"pipeline"));
        assert_eq!(longs.last(), Some(&"dismiss-recommendation"));
        let at = |l: &str| longs.iter().position(|x| *x == l).unwrap_or_else(|| panic!("--{l}"));
        let mut registry: Vec<&str> = Vec::new();
        for long in defs().iter().filter_map(|d| d.flag).map(|f| f.trim_start_matches("--")) {
            if !registry.contains(&long) {
                registry.push(long);
            }
        }
        for w in registry.windows(2) {
            assert!(at(w[0]) < at(w[1]), "--{} before --{}", w[0], w[1]);
        }
        for d in defs() {
            if let (Some(f), Some((no, _))) = (d.flag, d.cli.negated) {
                assert_eq!(at(no.trim_start_matches("--")), at(f.trim_start_matches("--")) + 1, "{no} after {f}");
            }
        }
    }
}
