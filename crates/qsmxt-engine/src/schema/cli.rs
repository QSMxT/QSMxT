//! `qsmxt run`'s settings flags as data: what each is called, what it accepts and its help, in
//! the order `--help` lists them ([`PIPELINE_FLAGS`]). `qsmxt` builds its command line from this
//! table.
//!
//! A registry setting's flag is derived from its entry ([`super::table`]): `.flag("--x")` names it,
//! and the setting's type says what it takes ([`derive`]): a switch for a yes/no setting, one
//! number for a number, three for a vector, one of the options for a choice. The entry's help
//! is the flag's help unless `.cli(..)` words it for the command line. The builder methods below
//! give the rest where it differs from what the type implies: the value's name, how it is read,
//! a value list of its own, the opposite switch (`.negated("--no-x", ..)`), and so on.
//!
//! The flags that set no one setting are [`OTHER_FLAGS`]; [`CLI_ORDER`] puts every flag in its
//! place in `--help`.

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
    help: Option<&'static str>,
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
        help: Some(s.help.unwrap_or(d.gui_help.unwrap_or(d.help))),
        long_help: None,
        values: Box::leak(values.into_boxed_slice()),
        defaults: if switch { &["false"] } else { &[] },
        default_missing: if switch { &["true"] } else { s.default_missing },
        conflicts: s.conflicts,
        requires: s.requires,
        attrs: if s.attrs.is_empty() && signed { &["allow_negative_numbers"] } else { s.attrs },
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

/// Every flag, in [`CLI_ORDER`]. A registry flag it does not list follows the flag of the entry
/// before it in the registry, and an opposite its switch, so a new setting needs no entry there.
fn build() -> Vec<Flag> {
    // In the registry's order, each opposite after its switch. A flag two entries name (the
    // reference and its region) is the first one's.
    let mut registry: Vec<Flag> = Vec::new();
    for d in defs().iter().filter(|d| d.flag.is_some()) {
        if registry.iter().any(|f| d.flag.is_some_and(|l| l.trim_start_matches("--") == f.long)) {
            continue;
        }
        let f = derive(d);
        registry.push(f);
        if let Some((long, help)) = d.cli.negated {
            registry.push(opposite(&f, long, help, d.cli.negated_conflicts));
        }
    }
    let mut every: Vec<Flag> = OTHER_FLAGS.iter().copied().chain(registry.iter().copied()).collect();
    let mut ordered: Vec<Flag> = CLI_ORDER.iter().map(|long| {
        let i = every.iter().position(|f| f.long == *long).unwrap_or_else(|| panic!("CLI_ORDER lists --{long}, which is no flag or is listed twice"));
        every.remove(i)
    }).collect();
    for (i, f) in registry.iter().enumerate() {
        if !every.iter().any(|x| x.long == f.long) {
            continue;
        }
        let at = registry[..i].iter().rev().find_map(|p| ordered.iter().position(|o| o.long == p.long)).map_or(ordered.len(), |p| p + 1);
        ordered.insert(at, *f);
    }
    assert!(every.iter().all(|f| ordered.iter().any(|o| o.long == f.long)), "every flag is in --help");
    ordered
}

/// `qsmxt run`'s settings flags, in the order `--help` lists them: built once from the registry
/// and [`OTHER_FLAGS`] ([`build`]).
pub static PIPELINE_FLAGS: LazyLock<Vec<Flag>> = LazyLock::new(build);

/// `qsmxt run`'s settings flags in the order `--help` lists them, by name: registry flags and
/// [`OTHER_FLAGS`]. A registry flag not listed here follows its registry neighbour ([`build`]).
pub const CLI_ORDER: &[&str] = &[
    "pipeline", "qsm-algorithm", "heidi", "orientation-group", "orientation-reference",
    "no-orientation-registration", "multi-orientation-algorithm", "multi-orientation-lambda",
    "multi-orientation-force", "cosmos-magnitude-weighting", "cosmos-tol", "cosmos-max-iter",
    "unwrapping-algorithm", "bf-algorithm", "bf-erode-voxels", "bf-polish-mask",
    "no-bf-polish-mask", "bf-ukb-mask-trim", "no-bf-ukb-mask-trim",
    "masking-input", "phase-offset-removal", "phase-offset-sigma-mm",
    "coil-combination-sigma", "bipolar-correction", "b0-estimation",
    "qsmart-fit-threshold-mode", "qsmart-fit-percentile", "qsmart-fit-threshold",
    "b0-weight-type", "b0-weight-assumed-t2star", "laplacian-solver", "laplacian-fft-pad", "laplacian-unmasked",
    "no-laplacian-unmasked", "laplacian-keep-outside", "no-laplacian-keep-outside",
    "bet-fractional-intensity", "bet-smoothness", "bet-gradient-threshold", "bet-iterations", "bet-subdivisions",
    "bet-voxel-scale", "bet-orientation", "qsm-reference", "qsm-per-echo",
    "rts-delta", "rts-mu", "rts-tol", "rts-rho", "rts-max-iter", "rts-lsmr-iter", "rts-tau", "rts-gamma", "rts-pad",
    "tv-lambda", "tv-rho", "tv-tol", "tv-max-iter", "tv-pad", "tkd-threshold", "tkd-pad",
    "tsvd-threshold", "tsvd-pad", "tgv-iterations", "tgv-erosions", "tgv-alpha1", "tgv-alpha0", "tgv-step-size",
    "tgv-te", "tikhonov-lambda", "tikhonov-pad", "nltv-lambda", "nltv-rho", "nltv-mu", "nltv-tol", "nltv-max-iter",
    "nltv-tol-newton", "nltv-newton-iter", "nltv-te", "nltv-pad",
    "medi-lambda", "medi-merit", "medi-csf", "medi-lambda-csf", "medi-csf-r2s-threshold", "medi-csf-erode",
    "medi-zeropad", "medi-smv", "no-medi-smv", "medi-smv-radius", "medi-smv-shrink-mask",
    "no-medi-smv-shrink-mask", "medi-msmv", "medi-msmv-maxk", "medi-msmv-vessel-radius",
    "no-medi-msmv-prefilter", "medi-data-weighting", "medi-percentage", "medi-cg-tol", "medi-cg-max-iter",
    "medi-max-iter", "medi-tol", "medi-te", "tfi-lambda", "tfi-precond", "tfi-merit", "tfi-linear",
    "tfi-data-weighting", "tfi-percentage", "tfi-cg-tol", "tfi-cg-max-iter", "tfi-max-iter", "tfi-tol", "tfi-te",
    "ilsqr-tol", "ilsqr-max-iter", "ilsqr-pad-mm", "ilsqr-precision",
    "star-pad", "star-te", "lsqr-residual-weighting", "no-lsqr-global-offset",
    "lsqr-tol", "lsqr-max-iter", "heidi-cone-threshold", "heidi-gradient-threshold",
    "no-heidi-laplacian-correction", "heidi-laplacian-threshold", "heidi-gradient-mask-floor",
    "heidi-continuation-steps", "heidi-inner-iterations", "heidi-mu-min", "heidi-tol",
    "no-heidi-denoise", "heidi-denoise-iterations", "heidi-denoise-time-step", "heidi-denoise-conductance",
    "heidi-denoise-reference-itk", "no-heidi-denoise-reference-itk",
    "heidi-te", "qsmart-ilsqr-max-iter", "qsmart-ilsqr-pad-mm", "qsmart-vasc-sphere-radius", "qsmart-inversion",
    "qsmart-sdf-sigma1-stage1", "qsmart-sdf-sigma2-stage1", "qsmart-sdf-sigma1-stage2", "qsmart-sdf-sigma2-stage2",
    "qsmart-sdf-lower-lim", "qsmart-sdf-curv-constant", "qsmart-frangi-scale-min", "qsmart-frangi-scale-max",
    "qsmart-frangi-scale-ratio", "qsmart-frangi-c", "qsmart-frangi-c-mode", "qsmart-frangi-c-factor",
    "qsmart-vessel-units", "qsmart-vessel-magnitude", "qsmart-offset-grid",
    "ndi-tau", "ndi-alpha", "ndi-max-iter", "ndi-auto-stop", "ndi-te", "ndi-weight-norm",
    "fansi-alpha1", "fansi-mu1", "fansi-mu2", "fansi-alpha0", "fansi-mu0", "fansi-max-iter", "fansi-tol-update",
    "fansi-tol-delta", "fansi-te", "l1qsm-alpha1", "l1qsm-mu1", "l1qsm-mu2", "l1qsm-mu3", "l1qsm-lambda",
    "l1qsm-max-iter", "l1qsm-tol-update", "l1qsm-tol-delta", "l1qsm-te",
    "whqsm-alpha1", "whqsm-mu1", "whqsm-mu2", "whqsm-beta", "whqsm-muh", "whqsm-max-iter", "whqsm-tol-update",
    "whqsm-tol-delta", "whqsm-weight", "whqsm-te", "hdqsm-alpha-l2", "hdqsm-mu1-l2", "hdqsm-mu2",
    "hdqsm-max-iter-l1", "hdqsm-max-iter-l2", "hdqsm-tol-update",
    "amp-pe-wave-order", "amp-pe-nlevel", "amp-pe-wave-pec", "amp-pe-simulated-te", "amp-pe-max-linearization-ite",
    "amp-pe-damp-rate-sig", "amp-pe-damp-rate-par", "amp-pe-max-pe-spar-ite", "amp-pe-max-pe-est-ite",
    "amp-pe-cvg-thd", "amp-pe-tikhonov-beta", "r2star-qsm-r-const-3t",
    "decompose-n-inner", "decompose-chi-bound", "decompose-unwrap-phase", "decompose-fit",
    "decompose-max-iterations", "decompose-max-function-evaluations", "decompose-function-tolerance",
    "decompose-step-tolerance", "decompose-optimality-tolerance", "decompose-finite-difference-step",
    "decompose-typical-x", "chi-sep-ilsqr-dr-pos", "chi-sep-ilsqr-dr-neg", "chi-sep-ilsqr-pad",
    "chi-sep-ilsqr-cg-max-iter-lsqr", "chi-sep-ilsqr-cg-tol-lsqr", "chi-sep-ilsqr-cg-max-iter-sa",
    "chi-sep-ilsqr-cg-tol-sa", "chi-sep-ilsqr-d2-thr", "chi-sep-ilsqr-prc-w-i-min", "chi-sep-ilsqr-prc-w-i-max",
    "chi-sep-ilsqr-prc-w-g-min", "chi-sep-ilsqr-prc-w-g-max", "chi-sep-ilsqr-qsm-max-iter", "chi-sep-ilsqr-qsm-tol",
    "chi-sep-ilsqr-qsm-pad-mm", "chi-sep-ilsqr-qsm-precision", "chi-sep-medi-lambda", "chi-sep-medi-lambda-csf",
    "chi-sep-medi-dr-pos", "chi-sep-medi-dr-neg", "chi-sep-medi-pad", "chi-sep-medi-tol", "chi-sep-medi-max-iter",
    "chi-sep-medi-cg-tol", "chi-sep-medi-cg-max-iter", "chi-sep-medi-qsm-max-iter", "chi-sep-medi-qsm-tol",
    "chi-sep-medi-qsm-pad-mm", "chi-sep-medi-qsm-precision", "chi-sep-medi-csf-r2star",
    "wavesep-dr-pos", "wavesep-dr-neg", "wavesep-alpha", "wavesep-lambda", "wavesep-wavelet-order",
    "wavesep-max-iter", "wavesep-tol", "hc-chisep-dr-pos-3t", "hc-chisep-bin-hz",
    "chi-sepnet-route", "chi-sepnet-patch", "chi-sepnet-frame-patch",
    "susep-net-patch", "motion-correction", "motion-reference", "no-motion-chain",
    "motion-chain-levels", "motion-min-ncc", "registration-levels", "registration-max-samples",
    "registration-max-iterations", "registration-max-evaluations", "registration-tolerance",
    "registration-rotation-step-deg", "registration-translation-step-mm", "registration-min-overlap",
    "vsharp-deconvolve", "vsharp-threshold", "vsharp-max-radius", "vsharp-min-radius", "vsharp-radius-step",
    "vsharp-grid", "vsharp-grid-spacing", "pdf-tol", "pdf-n-cg", "pdf-space", "pdf-n-pad",
    "lbv-tol", "lbv-depth", "lbv-peel", "lbv-n1", "lbv-n2", "lbv-n3", "lbv-precision",
    "ismv-variant", "ismv-tol", "ismv-max-iter", "ismv-radius", "sharp-threshold", "sharp-radius",
    "resharp-radius", "resharp-tik-reg", "resharp-tol", "resharp-max-iter",
    "harperella-radius", "harperella-max-iter", "harperella-tol", "harperella-pad", "harperella-rim",
    "iharperella-max-iter", "iharperella-tol", "iharperella-pad", "iharperella-rim",
    "no-romeo-phase-coherence", "no-romeo-phase-gradient-coherence", "no-romeo-phase-linearity",
    "romeo-phase-linearity", "no-romeo-mag-coherence", "romeo-mag-weight",
    "no-romeo-mag-weight", "romeo-mag-weight2", "romeo-bestpath", "romeo-template", "romeo-individual",
    "no-romeo-individual", "romeo-correct-global", "no-romeo-correct-global",
    "romeo-temporal-uncertain-unwrapping", "romeo-max-seeds", "romeo-merge-regions", "romeo-correct-regions",
    "romeo-wrap-addition", "prelude-labelling", "prelude-partitions",
    "swi-unwrapping", "swi-hp-sigma", "swi-scaling", "swi-strength", "swi-mip-window",
    "smwi-threshold", "smwi-power", "smwi-mip-window", "synthseg-version", "synthseg-crop", "synthseg-patch",
    "no-synthseg-flip-averaging", "no-synthseg-topology-cleanup",
    "synthseg-sigma", "use-custom-dseg", "tile-size", "tile-halo",
    "inhomogeneity-method", "homogeneity-sigma-mm", "homogeneity-nbox",
    "n4-shrink-factor", "n4-max-iterations", "n4-convergence-threshold", "n4-mesh", "n4-spline-order",
    "n4-bias-field-fwhm", "n4-wiener-noise", "n4-histogram-bins", "n4-geometry",
    "linear-fit-estimate-offset", "robust-fit-loss", "robust-fit-tuning", "robust-fit-iterations",
    "robust-fit-downweight-level", "robust-fit-flag-ratio", "complex-fit-reltol", "complex-fit-max-iter",
    "no-qsm", "do-swi", "swi-filtered-phase", "no-magnitude", "do-segmentation", "do-analysis", "do-smwi",
    "do-t2starmap", "do-r2starmap", "do-r2map", "do-r2primemap", "do-chisep",
    "chisep", "use-custom-qsm", "use-custom-r2", "use-custom-r2prime",
    "r2prime-strategy", "r2primenet-ignore-field-mismatch", "r2primenet-patch",
    "export-dicom", "inhomogeneity-correction", "no-inhomogeneity-correction",
    "axial-resampling", "no-axial-resampling", "obliquity-threshold",
    "crop-to-mask", "ignore-slice-gap", "fft-padding", "tukey-window",
    "crop-margin-mm", "output-space", "mask-preset", "use-custom-masks",
    "mask", "mask-combine", "mask-refine", "two-pass", "two-pass-mask", "two-pass-mask-combine", "two-pass-mode",
    "dismiss-recommendation",
];

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
}
