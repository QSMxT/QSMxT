//! A description of every user-facing setting, for front ends that render settings rather than
//! restate them.
//!
//! [`schema`] lists each setting of [`PipelineConfig`] a user can change: its config path (e.g.
//! `bg_removal.vsharp.max_radius`), labels and help, value type, units and range, QSMxT's default
//! and the browser's ([`PipelineConfig::browser`]), level (basic or advanced), where it sits (the
//! TUI's tabs and sub-tabs, in the TUI's order), the setting it belongs under, and its
//! `qsmxt run` option. [`settings_view`] says which settings apply to a given config, which of
//! them another setting forces on, and what is wrong with each value. [`mask_schema`] describes
//! the parts of a mask recipe.
//!
//! Each setting has the TUI's short label (`label`) and a full one for front ends with more room
//! (`full_label`), plus a `context` (the method it belongs to) where the label alone could be
//! another setting's. `help` is worded for any front end: it does not name the default or repeat
//! the units, which the schema carries. The TUI's own help and visibility are checked against the
//! schema by the TUI's tests.
//!
//! The schema is static data plus two functions of a config, so it serialises to JSON
//! ([`schema_json`]) for front ends in other languages. Values are read and written by path with
//! [`get`] and [`set`], through the config's own serde form, so a path that round-trips here is
//! one the TOML config accepts.

pub mod mask;
mod table;
mod view;

use serde::Serialize;
use serde_json::Value;

use crate::config::PipelineConfig;
use crate::store::Item;

pub use mask::{mask_schema, MaskSchema};
pub use view::{prune_hidden, settings_view, SettingView, ViewFacts};

/// A tab of the settings, as the TUI lays them out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tab {
    Input,
    Preprocessing,
    Qsm,
    Separation,
    Outputs,
}

impl Tab {
    /// Every tab with settings, in order.
    pub const ALL: [Tab; 5] = [Tab::Input, Tab::Preprocessing, Tab::Qsm, Tab::Separation, Tab::Outputs];

    /// The tab's title.
    pub fn label(self) -> &'static str {
        match self {
            Tab::Input => "Input",
            Tab::Preprocessing => "Preprocessing",
            Tab::Qsm => "QSM",
            Tab::Separation => "χ-separation",
            Tab::Outputs => "Other outputs",
        }
    }
}

/// Where a setting is shown: a sub-tab, or the whole tab for the tabs that have no sub-tabs
/// (Input, χ-separation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Section {
    Input,
    Geometry,
    Masking,
    Motion,
    FieldMap,
    Background,
    Inversion,
    MultiOrientation,
    Reference,
    Separation,
    SwiSmwi,
    Relaxometry,
    Segmentation,
    Export,
}

impl Section {
    /// Every section, in order.
    pub const ALL: [Section; 14] = [
        Section::Input, Section::Geometry, Section::Masking, Section::Motion, Section::FieldMap,
        Section::Background, Section::Inversion, Section::MultiOrientation, Section::Reference,
        Section::Separation, Section::SwiSmwi, Section::Relaxometry, Section::Segmentation,
        Section::Export,
    ];

    /// The tab the section is on.
    pub fn tab(self) -> Tab {
        use Section::*;
        match self {
            Input => Tab::Input,
            Geometry | Masking | Motion => Tab::Preprocessing,
            FieldMap | Background | Inversion | MultiOrientation | Reference => Tab::Qsm,
            Separation => Tab::Separation,
            SwiSmwi | Relaxometry | Segmentation | Export => Tab::Outputs,
        }
    }

    /// The sub-tab's title.
    pub fn label(self) -> &'static str {
        use Section::*;
        match self {
            Input => "Input",
            Geometry => "Geometry",
            Masking => "Masking",
            Motion => "Motion",
            FieldMap => "Field map",
            Background => "Background",
            Inversion => "Inversion",
            MultiOrientation => "Multi-orientation",
            Reference => "Reference",
            Separation => "χ-separation",
            SwiSmwi => "SWI/SMWI",
            Relaxometry => "Relaxometry",
            Segmentation => "Segmentation",
            Export => "Export",
        }
    }
}

/// Whether a setting is shown in a basic view, or only with every setting shown.
///
/// Basic settings say what a step does (whether it runs, which method); advanced ones tune how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Basic,
    Advanced,
}

/// One choice of an enumerated setting.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnumOption {
    /// The value as it is written in the config.
    pub value: Value,
    /// How the option is named in the TUI (usually the `qsmxt run` value).
    pub label: &'static str,
    /// Its display name: short, as the method is written ("V-SHARP", "iLSQR").
    pub name: String,
    /// What it does, without its link.
    pub help: String,
    /// A reference for it (a DOI link, or the method's page), when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The TUI's help for it, without its link (which also names the default).
    #[serde(skip)]
    pub tui_help: String,
}

/// One choice of an echo reference: a named echo, or any echo by its number.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EchoOption {
    #[serde(flatten)]
    pub option: EnumOption,
    #[serde(flatten)]
    pub kind: EchoKind,
}

/// What an echo-reference choice stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EchoKind {
    /// The option's own value (`"first"`).
    Named,
    /// Any echo by number, counted from `min`: the setting's value is the number as a string
    /// (`"3"`). The option's value is the number the choice starts at.
    EchoNumber { min: u32 },
}

/// What kind of value a setting holds.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ValueType {
    Bool,
    Int,
    Float,
    /// Three integers, `[x, y, z]`.
    IntVec3,
    /// Three numbers, `[x, y, z]`.
    FloatVec3,
    Text,
    Enum { options: Vec<EnumOption> },
    /// An echo to align onto: a named echo, or any echo by its number (see [`EchoKind`]).
    EchoReference { options: Vec<EchoOption> },
    /// A mask recipe: a list of mask sections (`masking.sections` in the config). Edited with a
    /// mask builder rather than a field.
    MaskSections,
    /// A list of mask refinement steps.
    MaskOps,
}

impl ValueType {
    /// The choices of an enum or echo-reference setting (none for other types).
    pub fn options(&self) -> Vec<&EnumOption> {
        match self {
            ValueType::Enum { options } => options.iter().collect(),
            ValueType::EchoReference { options } => options.iter().map(|o| &o.option).collect(),
            _ => vec![],
        }
    }
}

/// One user-facing setting.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Setting {
    /// Where the value lives in the config, dot-separated (`bg_removal.vsharp.max_radius`).
    pub path: &'static str,
    /// The TUI's label: short, to fit its column.
    pub label: &'static str,
    /// The label in full, without units, for a front end with room for it.
    pub full_label: &'static str,
    /// What the setting belongs to, for telling apart settings with the same label in one
    /// section: the method whose parameter it is ("PDF", "LBV"). Absent for a setting that is
    /// not one method's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<&'static str>,
    /// What it does. It does not name the default or the units, which are given separately.
    pub help: &'static str,
    /// For a setting shown as a checkbox with a value under it (`off` is set): the value's own
    /// label and help.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_label: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_help: Option<&'static str>,
    /// The TUI's help for the setting (its row's), and its value row's label and help.
    #[serde(skip)]
    pub tui_help: &'static str,
    #[serde(skip)]
    pub tui_value: Option<(&'static str, &'static str)>,
    #[serde(flatten)]
    pub value_type: ValueType,
    /// Whether the value may be `null` (unset: the setting's own fallback, given in its help).
    pub optional: bool,
    /// The value that turns the setting off, for a setting shown as a checkbox with a value.
    /// Absent for other settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub off: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Whether `min` itself is outside the range (the value must be greater than it).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclusive_min: bool,
    /// Whether `max` itself is outside the range.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclusive_max: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// QSMxT's default.
    pub default: Value,
    /// The default in the browser: QSMxT's, except where the browser profile
    /// ([`PipelineConfig::browser`]) changes it to fit the browser's memory.
    pub browser_default: Value,
    pub level: Level,
    pub tab: Tab,
    /// The setting's own section: where a front end that shows each setting once shows it.
    pub section: Section,
    /// Other sections the setting also bears on, where the TUI repeats it. It is one value
    /// wherever it shows, so a front end may show a reference to it there instead.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also_in: Vec<Section>,
    /// Position in the TUI's order, across all settings.
    pub order: usize,
    /// The setting this one is listed under (whose value decides whether it applies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<&'static str>,
    /// The `qsmxt run` option that sets it, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_flag: Option<&'static str>,
    /// How a browser front end offers it.
    pub browser: Browser,
    /// For a [`Browser::SuppliedFile`] setting: the item the supplied file fills.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Item>,
    /// For a [`Browser::SuppliedFile`] setting: its help, for the file input a browser shows in
    /// place of the setting's text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_help: Option<&'static str>,
}

/// How a browser front end offers a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Browser {
    /// As any other setting.
    Available,
    /// A map the user supplies instead of one the run computes or finds in a BIDS derivatives
    /// directory: the front end shows a file input, adds the file to the run's store, and sets
    /// the setting to [`BROWSER_SUPPLIED_TOOL`].
    SuppliedFile,
    /// Not offered in the browser yet.
    Unavailable,
}

/// The derivatives "tool" a browser run names its supplied files by, so the generated command
/// reads them from `<bids>/derivatives/qsmbly/` when the run is repeated with QSMxT.
pub const BROWSER_SUPPLIED_TOOL: &str = "qsmbly";

/// Config leaves that are deliberately not in the schema, with why.
pub const NOT_IN_SCHEMA: &[(&str, &str)] = &[
    ("description", "a free-text note saved with the config, not a setting"),
    ("schema_version", "the settings-file layout the config was written with (crate::migrate), not a setting"),
    ("bg_removal.algorithm_chosen", "not a setting of its own: a front end sets it when the user picks a background removal (bg_removal.algorithm), so it is kept rather than replaced by the inversion's reference one"),
    ("masking.recipe_chosen", "not a setting of its own: a front end sets it when the user picks a mask preset or edits the recipe (masking.sections), so the recipe is kept rather than replaced by the inversion's default"),
    ("field_mapping.unwrapping_chosen", "not a setting of its own: a front end sets it when the user picks the unwrapping (field_mapping.unwrapping_algorithm), so it is kept rather than replaced by QSMART's default"),
    ("field_mapping.laplacian_solver_chosen", "not a setting of its own: a front end sets it when the user picks the Laplacian solver (field_mapping.laplacian_solver), so it is kept rather than replaced by QSMART's default"),
    ("field_mapping.b0_estimation_chosen", "not a setting of its own: a front end sets it when the user picks the B0 estimation (field_mapping.b0_estimation), so it is kept rather than replaced by QSMART's default"),
    ("bet.fractional_intensity", "set per BET step in the mask recipe (masking.sections); this is the --bet-fractional-intensity override applied to every step"),
    ("bet.voxel_scale", "set per BET step in the mask recipe (masking.sections); this is the --bet-voxel-scale override applied to every step"),
    ("inversion.tikhonov.reg", "config file only: no qsmxt run option or TUI row"),
    ("inversion.amp_pe.b0", "config file only: no qsmxt run option or TUI row"),
    ("inversion.amp_pe.gyro_ratio", "config file only: no qsmxt run option or TUI row"),
];

// ─── The table's building blocks ───

/// How a table entry's value is typed: [`ValueType`] before defaults are filled in.
pub(crate) enum Ty {
    Bool,
    Int,
    Float,
    IntVec3,
    FloatVec3,
    Text,
    Enum(Vec<Opt>),
    EchoReference(Vec<Opt>),
    /// The SynthSeg regions ([`crate::regions::REGIONS`]).
    Regions,
    MaskSections,
    MaskOps,
}

/// An option of a table entry: value, TUI label, display name and the TUI's help.
pub(crate) type Opt = (Value, &'static str, &'static str, &'static str);

/// A table entry.
pub(crate) struct Def {
    path: &'static str,
    section: Section,
    label: &'static str,
    help: &'static str,
    ty: Ty,
    value: Option<(&'static str, &'static str)>,
    /// The full label, where it is not `label`.
    full: Option<&'static str>,
    /// The front-end help, where it is not the TUI's `help`.
    gui_help: Option<&'static str>,
    /// The value row's front-end label and help, where they are not the TUI's.
    gui_value: Option<(&'static str, &'static str)>,
    context: Option<&'static str>,
    item: Option<Item>,
    browser_help: Option<&'static str>,
    basic: bool,
    units: Option<&'static str>,
    min: Option<f64>,
    max: Option<f64>,
    exclusive_min: bool,
    exclusive_max: bool,
    off: Option<Value>,
    optional: bool,
    flag: Option<&'static str>,
    parent: Option<&'static str>,
    also: &'static [Section],
    browser: Browser,
    when: Option<fn(&view::Ctx) -> bool>,
}

pub(crate) fn d(path: &'static str, section: Section, label: &'static str, help: &'static str, ty: Ty) -> Def {
    Def {
        path, section, label, help, ty, value: None, full: None, gui_help: None, gui_value: None, context: None,
        item: None, browser_help: None, basic: false, units: None, min: None, max: None, exclusive_min: false, exclusive_max: false,
        off: None, optional: false, flag: None, parent: None, also: &[], browser: Browser::Available, when: None,
    }
}

pub(crate) fn o(value: impl Into<Value>, label: &'static str, name: &'static str, help: &'static str) -> Opt {
    (value.into(), label, name, help)
}

impl Def {
    fn value(mut self, label: &'static str, help: &'static str) -> Self { self.value = Some((label, help)); self }
    /// The label in full (default: the TUI's label).
    fn full(mut self, label: &'static str) -> Self { self.full = Some(label); self }
    /// Help for any front end, where the TUI's names the default, repeats the units, or is
    /// missing.
    fn gui(mut self, help: &'static str) -> Self { self.gui_help = Some(help); self }
    /// The value row's label and help for any front end, where the TUI's differ.
    fn gui_value(mut self, label: &'static str, help: &'static str) -> Self { self.gui_value = Some((label, help)); self }
    /// What the setting belongs to, where it is not the method its path names (see
    /// [`context_of`]).
    fn context(mut self, c: &'static str) -> Self { self.context = Some(c); self }
    fn basic(mut self) -> Self { self.basic = true; self }
    fn units(mut self, u: &'static str) -> Self { self.units = Some(u); self }
    fn min(mut self, v: f64) -> Self { self.min = Some(v); self }
    fn max(mut self, v: f64) -> Self { self.max = Some(v); self }
    /// Greater than `v` (exclusive minimum).
    fn above(mut self, v: f64) -> Self { self.min = Some(v); self.exclusive_min = true; self }
    /// Less than `v` (exclusive maximum).
    fn below(mut self, v: f64) -> Self { self.max = Some(v); self.exclusive_max = true; self }
    fn off(mut self, v: Value) -> Self { self.optional |= v.is_null(); self.off = Some(v); self }
    fn optional(mut self) -> Self { self.optional = true; self }
    fn flag(mut self, f: &'static str) -> Self { self.flag = Some(f); self }
    fn parent(mut self, p: &'static str) -> Self { self.parent = Some(p); self }
    fn also(mut self, s: &'static [Section]) -> Self { self.also = s; self }
    /// Supplied as a file in the browser, filling `item`, with the file input's help.
    fn supplied_file(mut self, item: Item, help: &'static str) -> Self {
        self.browser = Browser::SuppliedFile;
        self.item = Some(item);
        self.browser_help = Some(help);
        self
    }
    fn unavailable_in_browser(mut self) -> Self { self.browser = Browser::Unavailable; self }
    fn when(mut self, f: fn(&view::Ctx) -> bool) -> Self { self.when = Some(f); self }
}

/// Split a trailing link off an option's help: "… — https://doi.org/x" → ("…", link).
fn split_link(help: &str) -> (String, Option<String>) {
    match help.find("https://") {
        Some(i) => {
            let url = help[i..].trim().trim_end_matches('.').to_string();
            let text = help[..i].trim_end().trim_end_matches(['—', '-', ':']).trim_end().to_string();
            (text, Some(url))
        }
        None => (help.to_string(), None),
    }
}

fn options(opts: &[Opt]) -> Vec<EnumOption> {
    opts.iter()
        .map(|(value, label, name, help)| {
            let (tui_help, url) = split_link(help);
            // The schema carries the default; an option's help need not say it is one.
            let help = tui_help.replace(" (default)", "");
            EnumOption { value: value.clone(), label, name: name.to_string(), help, url, tui_help }
        })
        .collect()
}

/// An echo reference's options: the one whose value is a number stands for any echo number.
fn echo_options(opts: &[Opt]) -> Vec<EchoOption> {
    options(opts)
        .into_iter()
        .map(|option| {
            let kind = match option.value.as_str().and_then(|v| v.parse::<u32>().ok()) {
                Some(min) => EchoKind::EchoNumber { min },
                None => EchoKind::Named,
            };
            EchoOption { option, kind }
        })
        .collect()
}

/// The method a setting's path names, as its parent's option names it: `bg_removal.pdf.tol`
/// under `bg_removal.algorithm` is PDF's. Walks up the parents to the first choice with an
/// option named like the path's middle part.
fn context_of(d: &Def, defs: &[Def]) -> Option<&'static str> {
    let parts: Vec<&str> = d.path.split('.').collect();
    let [_, method, _] = parts[..] else { return None };
    let method = method.replace('_', "-");
    let mut parent = d.parent;
    while let Some(p) = parent {
        let pd = defs.iter().find(|x| x.path == p)?;
        if let Ty::Enum(opts) = &pd.ty {
            if let Some((_, _, name, _)) = opts.iter().find(|(v, ..)| v.as_str() == Some(&method)) {
                return Some(name);
            }
        }
        parent = pd.parent;
    }
    None
}

/// The reference regions as options: the same help the TUI's reference picker gives them.
fn region_options() -> Vec<EnumOption> {
    crate::regions::REGIONS
        .iter()
        .map(|r| {
            let ids = r.ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
            let help = region_help(&r.label, r.ids.len(), &ids, r.v2_only);
            let mut name = r.label.clone();
            if let Some(first) = name.get(..1) {
                name = first.to_uppercase() + &name[1..];
            }
            EnumOption { value: Value::from(r.slug), label: r.slug, name, tui_help: help.clone(), help, url: None }
        })
        .collect()
}

/// The help of a reference region, as the TUI's reference picker words it.
pub fn region_help(label: &str, n_ids: usize, ids: &str, v2_only: bool) -> String {
    format!(
        "Reference χ to the mean of the {} (FreeSurfer {} {}){}. Enables SynthSeg",
        label,
        if n_ids == 1 { "label" } else { "labels" },
        ids,
        if v2_only { " — needs --synthseg-version v2" } else { "" },
    )
}

/// The settings table, built once. It is a constant of the build: nothing in it reads the
/// environment, the files on disk or anything else that can change while the program runs.
pub(crate) fn defs() -> &'static [Def] {
    static DEFS: std::sync::LazyLock<Vec<Def>> = std::sync::LazyLock::new(table::table);
    &DEFS
}

/// Every user-facing setting, in the TUI's order, built once: like [`defs`], it is a constant of
/// the build (its defaults are those of [`PipelineConfig::default`] and
/// [`PipelineConfig::browser`], which read nothing at run time). Command generation, the settings
/// view and range checks read it on every call, and rebuilding it each time was most of their cost.
pub fn settings() -> &'static [Setting] {
    static SETTINGS: std::sync::LazyLock<Vec<Setting>> = std::sync::LazyLock::new(build_schema);
    &SETTINGS
}

/// Every user-facing setting, in the TUI's order: an owned copy of [`settings`].
pub fn schema() -> Vec<Setting> {
    settings().to_vec()
}

fn build_schema() -> Vec<Setting> {
    let native = serde_json::to_value(PipelineConfig::default()).expect("the default config serialises");
    let browser = serde_json::to_value(PipelineConfig::browser()).expect("the browser config serialises");
    let defs = defs();
    let contexts: Vec<Option<&'static str>> = defs.iter().map(|d| d.context.or_else(|| context_of(d, defs))).collect();
    defs.iter()
        .zip(contexts)
        .enumerate()
        .map(|(order, (d, context))| {
            let value_type = match &d.ty {
                Ty::Bool => ValueType::Bool,
                Ty::Int => ValueType::Int,
                Ty::Float => ValueType::Float,
                Ty::IntVec3 => ValueType::IntVec3,
                Ty::FloatVec3 => ValueType::FloatVec3,
                Ty::Text => ValueType::Text,
                Ty::Enum(o) => ValueType::Enum { options: options(o) },
                Ty::EchoReference(o) => ValueType::EchoReference { options: echo_options(o) },
                Ty::Regions => ValueType::Enum { options: region_options() },
                Ty::MaskSections => ValueType::MaskSections,
                Ty::MaskOps => ValueType::MaskOps,
            };
            let step = matches!(d.ty, Ty::Int | Ty::IntVec3).then_some(1.0);
            Setting {
                path: d.path,
                label: d.label,
                full_label: d.full.unwrap_or(d.label),
                context,
                help: d.gui_help.unwrap_or(d.help),
                value_label: d.gui_value.or(d.value).map(|v| v.0),
                value_help: d.gui_value.or(d.value).map(|v| v.1),
                tui_help: d.help,
                tui_value: d.value,
                value_type,
                optional: d.optional,
                off: d.off.clone(),
                units: d.units,
                min: d.min,
                max: d.max,
                exclusive_min: d.exclusive_min,
                exclusive_max: d.exclusive_max,
                step,
                default: lookup(&native, d.path).cloned().unwrap_or(Value::Null),
                browser_default: lookup(&browser, d.path).cloned().unwrap_or(Value::Null),
                level: if d.basic { Level::Basic } else { Level::Advanced },
                tab: d.section.tab(),
                section: d.section,
                also_in: d.also.to_vec(),
                order,
                parent: d.parent,
                cli_flag: d.flag,
                browser: d.browser,
                item: d.item,
                browser_help: d.browser_help,
            }
        })
        .collect()
}

/// [`schema`] as JSON: `{ "tabs": [...], "sections": [...], "settings": [...], "mask": {...} }`,
/// the tab and section entries giving each one's id, title and (for sections) tab, and `mask`
/// the parts of a mask recipe ([`mask_schema`]).
pub fn schema_json() -> String {
    #[derive(Serialize)]
    struct TabEntry { id: Tab, label: &'static str }
    #[derive(Serialize)]
    struct SectionEntry { id: Section, label: &'static str, tab: Tab }
    #[derive(Serialize)]
    struct Doc { tabs: Vec<TabEntry>, sections: Vec<SectionEntry>, settings: Vec<Setting>, mask: MaskSchema }
    let doc = Doc {
        tabs: Tab::ALL.iter().map(|&t| TabEntry { id: t, label: t.label() }).collect(),
        sections: Section::ALL.iter().map(|&s| SectionEntry { id: s, label: s.label(), tab: s.tab() }).collect(),
        settings: settings().to_vec(),
        mask: mask_schema(),
    };
    serde_json::to_string(&doc).expect("the schema serialises")
}

/// The setting at `path`.
pub fn setting(path: &str) -> Option<Setting> {
    settings().iter().find(|s| s.path == path).cloned()
}

fn lookup<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |v, k| v.get(k))
}

/// The value at `path` in `config`, as it is written in the config (`null` for an unset
/// optional value); `None` if the config has no such path.
pub fn get(config: &PipelineConfig, path: &str) -> Option<Value> {
    let v = serde_json::to_value(config).ok()?;
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let obj = if parent.is_empty() { Some(&v) } else { lookup(&v, parent) }?;
    // An unset optional value is left out of the serialised config.
    obj.as_object().map(|o| o.get(key).cloned().unwrap_or(Value::Null))
}

/// Set the value at `path` in `config`. The change goes through the config's own
/// deserialisation, so a value the config file would reject is rejected here, and `config` is
/// left as it was.
pub fn set(config: &mut PipelineConfig, path: &str, value: Value) -> Result<(), String> {
    let mut v = serde_json::to_value(&*config).map_err(|e| e.to_string())?;
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let mut obj = &mut v;
    for k in parent.split('.').filter(|k| !k.is_empty()) {
        obj = obj.get_mut(k).ok_or_else(|| format!("no setting {path}"))?;
    }
    let map = obj.as_object_mut().ok_or_else(|| format!("no setting {path}"))?;
    // An unset optional value is left out of the serialised config, so a missing key is only an
    // unknown one when the schema does not know it either (serde would ignore it silently).
    if !map.contains_key(key) && !defs().iter().any(|d| d.path == path) {
        return Err(format!("no setting {path}"));
    }
    map.insert(key.to_string(), value);
    *config = serde_json::from_value(v).map_err(|e| format!("{path}: {e}"))?;
    Ok(())
}

impl Setting {
    /// Whether one number is inside the setting's range.
    pub fn in_range(&self, x: f64) -> bool {
        let lo = self.min.is_none_or(|m| if self.exclusive_min { x > m } else { x >= m });
        let hi = self.max.is_none_or(|m| if self.exclusive_max { x < m } else { x <= m });
        lo && hi
    }

    /// The range in words: "at least 0", "greater than 0", "from 0 to 1".
    pub fn range_text(&self) -> String {
        range_words(self.min, self.max, self.exclusive_min, self.exclusive_max)
    }
}

/// A bound as a range's text gives it: exact where that is short ("0", "0.5", "64"), a simple
/// fraction where the bound is one ("2/3" for the dipole kernel's largest |D|), and otherwise
/// rounded to 4 significant digits. Only the text: the comparison uses the bound itself.
pub(crate) fn range_number(v: f64) -> String {
    if v.fract() == 0.0 {
        return format!("{v:.0}");
    }
    let rounded = |v: f64| {
        let decimals = (3 - v.abs().log10().floor() as i32).max(0) as usize;
        let t = format!("{v:.decimals$}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let short = rounded(v);
    if short.parse::<f64>() == Ok(v) {
        return short;
    }
    for q in 2..=12u32 {
        let p = (v * q as f64).round();
        if p != 0.0 && (p / q as f64 - v).abs() <= 1e-12 * v.abs() {
            return format!("{p:.0}/{q}");
        }
    }
    short
}

/// A range in words: "at least 0", "greater than 0", "from 0 to 1".
pub(crate) fn range_words(min: Option<f64>, max: Option<f64>, exclusive_min: bool, exclusive_max: bool) -> String {
    let num = range_number;
    match (min, max) {
        (Some(lo), Some(hi)) => format!(
            "from {} to {}{}", num(lo), num(hi),
            match (exclusive_min, exclusive_max) {
                (false, false) => String::new(),
                (true, false) => format!(" (not {})", num(lo)),
                (false, true) => format!(" (not {})", num(hi)),
                (true, true) => format!(" (not {} or {})", num(lo), num(hi)),
            }),
        (Some(lo), None) => format!("{} {}", if exclusive_min { "greater than" } else { "at least" }, num(lo)),
        (None, Some(hi)) => format!("{} {}", if exclusive_max { "less than" } else { "at most" }, num(hi)),
        (None, None) => "any value".into(),
    }
}

/// Check every setting that applies to `config` against its range, and every number setting
/// that applies for being finite (a config file and the command line both accept `nan` and
/// `inf`), the parameter checks the settings view shows (QSM.rs's `validate()`s), and the mask recipe's steps ([`MaskingConfig::validate`](crate::config::MaskingConfig::validate)). A setting that does not apply (see [`settings_view`]) is not checked: it cannot
/// change the run.
///
/// The error names each setting out of range (its path and `qsmxt run` option), the value, and
/// the range.
pub fn check_ranges(config: &PipelineConfig) -> Result<(), crate::EngineError> {
    let applies: std::collections::HashSet<&str> = settings_view(config, &ViewFacts::default())
        .into_iter().filter(|v| v.visible).map(|v| v.path).collect();
    // The parameter checks the settings view shows (QSM.rs's message, which a run would
    // otherwise stop on), then the ranges of the settings they do not already refuse.
    let rules = view::rule_problems(config, |p| applies.contains(p));
    let ranges = out_of_range(config, |p| applies.contains(p) && !rules.iter().any(|(r, _)| *r == p));
    let mut problems: Vec<String> = rules.into_iter()
        .map(|(path, e)| {
            let flag = settings().iter().find(|s| s.path == path).and_then(|s| s.cli_flag)
                .map(|f| format!(" ({f})")).unwrap_or_default();
            format!("{path}{flag}: {e}")
        })
        .chain(ranges.into_iter().map(|p| p.described))
        .collect();
    // A mask step that would do nothing (an erosion of 0), as a config file and --mask refuse it.
    problems.extend(config.masking.validate().err());
    // Settings that cannot be run together (QSMART's echo fit without Laplacian unwrapping).
    if applies.contains("field_mapping.b0_estimation") {
        if let Err(e) = config.field_mapping.validate() {
            return Err(crate::EngineError::Config(e));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(crate::EngineError::Config(format!("setting out of range: {}", problems.join("; "))))
    }
}

/// What is wrong with one setting's value.
pub(crate) struct Problem {
    pub path: &'static str,
    /// For showing next to the setting: "Must be greater than 0".
    pub message: String,
    /// For an error message, naming the setting, its option and its value.
    pub described: String,
}

/// Each checked setting whose value is outside its range, or not a finite number.
pub(crate) fn out_of_range(config: &PipelineConfig, checked: impl Fn(&str) -> bool) -> Vec<Problem> {
    // Serialised, a non-finite number is `null`.
    let Ok(json) = serde_json::to_value(config) else { return vec![] };
    let mut problems = Vec::new();
    for s in settings() {
        let float = matches!(s.value_type, ValueType::Float | ValueType::FloatVec3);
        if !checked(s.path) || !(float || s.min.is_some() || s.max.is_some()) {
            continue;
        }
        let (parent, key) = s.path.rsplit_once('.').unwrap_or(("", s.path));
        let obj = if parent.is_empty() { Some(&json) } else { lookup(&json, parent) };
        let v = obj.and_then(|o| o.get(key)).cloned().unwrap_or(Value::Null);
        let flag = s.cli_flag.map(|f| format!(" ({f})")).unwrap_or_default();
        let values: Vec<Option<f64>> = match &v {
            Value::Array(a) => a.iter().map(Value::as_f64).collect(),
            Value::Null if s.optional => vec![],
            other => vec![other.as_f64()],
        };
        if float && values.iter().any(Option::is_none) {
            problems.push(Problem {
                path: s.path,
                message: "Must be a finite number".into(),
                described: format!("{}{flag} must be a finite number", s.path),
            });
        } else if values.iter().flatten().any(|&x| !s.in_range(x)) {
            let range = s.range_text();
            problems.push(Problem {
                path: s.path,
                message: format!("Must be {range}"),
                described: format!("{}{flag} is {v}; it must be {range}", s.path),
            });
        }
    }
    problems
}

#[cfg(test)]
mod range_tests {
    use super::*;

    /// A value just outside each end of the range, of the setting's type.
    fn outside(s: &Setting) -> Vec<f64> {
        let int = matches!(s.value_type, ValueType::Int | ValueType::IntVec3);
        let step = if int { 1.0 } else { 1e-6 };
        let mut out = Vec::new();
        if let Some(lo) = s.min {
            out.push(if s.exclusive_min { lo } else { lo - step });
        }
        if let Some(hi) = s.max {
            out.push(if s.exclusive_max { hi } else { hi + step });
        }
        out.retain(|&x| !int || x >= 0.0); // an unsigned setting cannot hold a negative
        out
    }

    fn with(path: &str, s: &Setting, x: f64) -> Option<PipelineConfig> {
        let v = match s.value_type {
            ValueType::Int => json!(x as u64),
            ValueType::IntVec3 => json!([x as u64, x as u64, x as u64]),
            ValueType::FloatVec3 => json!([x, x, x]),
            _ => json!(x),
        };
        let mut c = PipelineConfig::default();
        set(&mut c, path, v).ok().map(|_| c)
    }

    use serde_json::json;

    #[test]
    fn every_default_and_browser_default_is_in_range() {
        let checked = |_: &str| true;
        assert!(out_of_range(&PipelineConfig::default(), checked).is_empty());
        assert!(out_of_range(&PipelineConfig::browser(), checked).is_empty());
    }

    #[test]
    fn every_range_is_enforced_just_outside_it() {
        let mut tested = 0;
        for s in schema() {
            for x in outside(&s) {
                let c = with(s.path, &s, x).unwrap_or_else(|| panic!("{} = {x} does not fit the type", s.path));
                let problems = out_of_range(&c, |p| p == s.path);
                assert_eq!(problems.len(), 1, "{} = {x} was accepted", s.path);
                let p = &problems[0];
                assert!(p.described.contains(s.path) && p.described.contains(&s.range_text()), "{}", p.described);
                assert_eq!(p.path, s.path);
                assert_eq!(p.message, format!("Must be {}", s.range_text()));
                tested += 1;
            }
            // And the ends that are in the range are accepted.
            for x in [s.min.filter(|_| !s.exclusive_min), s.max.filter(|_| !s.exclusive_max)].into_iter().flatten() {
                let c = with(s.path, &s, x).unwrap();
                assert!(out_of_range(&c, |p| p == s.path).is_empty(), "{} = {x} was refused", s.path);
            }
        }
        assert!(tested > 40, "{tested}");
    }

    #[test]
    fn range_numbers_are_exact_when_short_and_rounded_otherwise() {
        assert_eq!(range_number(0.0), "0");
        assert_eq!(range_number(64.0), "64");
        assert_eq!(range_number(0.5), "0.5");
        assert_eq!(range_number(1e-4), "0.0001");
        assert_eq!(range_number(2.0 / 3.0), "2/3");
        assert_eq!(range_number(std::f64::consts::PI), "3.142");
        // A value just under 2/3 is still refused by the exact comparison.
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = crate::QsmAlgorithm::Tkd;
        c.inversion.tkd.threshold = 0.6667;
        assert!(check_ranges(&c).is_err());
        c.inversion.tkd.threshold = 0.6666;
        assert!(check_ranges(&c).is_ok());
    }

    #[test]
    fn a_number_that_is_not_finite_is_refused() {
        let mut c = PipelineConfig::default();
        c.bg_removal.vsharp.max_radius = f64::INFINITY;
        let e = check_ranges(&c).unwrap_err().to_string();
        assert!(e.contains("bg_removal.vsharp.max_radius (--vsharp-max-radius) must be a finite number"), "{e}");
        c.bg_removal.vsharp.max_radius = f64::NAN;
        assert!(check_ranges(&c).is_err());
    }

    #[test]
    fn the_error_names_the_setting_its_value_and_range() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = crate::QsmAlgorithm::Tkd;
        c.inversion.tkd.threshold = 0.0;
        let e = check_ranges(&c).unwrap_err().to_string();
        assert!(e.contains("inversion.tkd.threshold (--tkd-threshold) is 0.0; it must be from 0 to 2/3 (not 0 or 2/3)"), "{e}");
    }

    #[test]
    fn a_setting_that_does_not_apply_is_not_checked() {
        // TKD's threshold, under the default iLSQR inversion.
        let mut c = PipelineConfig::default();
        c.inversion.tkd.threshold = 0.0;
        assert!(check_ranges(&c).is_ok());
        assert!(check_ranges(&PipelineConfig::default()).is_ok());
    }

    /// QSMART's echo fit with ROMEO unwrapping, both chosen, is refused before the run, and the
    /// settings view shows why next to the B0 estimation.
    #[test]
    fn the_echo_fit_with_romeo_is_refused() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = crate::QsmAlgorithm::Qsmart;
        assert!(check_ranges(&c.normalised().0).is_ok());
        c.field_mapping.unwrapping_chosen = true;
        c.field_mapping.b0_estimation = crate::B0Estimation::QsmartEchofit;
        let n = c.normalised().0;
        let e = check_ranges(&n).unwrap_err().to_string();
        assert!(e.contains("QSMART's echo fit needs Laplacian unwrapping"), "{e}");
        let view = settings_view(&c, &ViewFacts::default());
        let b0 = view.iter().find(|v| v.path == "field_mapping.b0_estimation").unwrap();
        assert!(b0.error.as_deref().is_some_and(|m| m.contains("Laplacian")), "{:?}", b0.error);
    }
}
