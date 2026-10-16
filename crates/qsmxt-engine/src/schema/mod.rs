//! A description of every user-facing setting, for front ends that render settings rather than
//! restate them.
//!
//! [`schema`] lists each setting of [`PipelineConfig`] a user can change: its config path (e.g.
//! `bg_removal.vsharp.max_radius`), label and help, value type, units and range, QSMxT's default
//! and the browser's, level (basic or advanced), where it sits (the
//! TUI's tabs and sub-tabs, in the TUI's order), the setting it belongs under, and its
//! `qsmxt run` option. [`settings_view`] says which settings apply to a given config, and which
//! of them another setting forces on. Labels, help and visibility are the TUI's, and the TUI's
//! tests check that they still are.
//!
//! The schema is static data plus two functions of a config, so it serialises to JSON
//! ([`schema_json`]) for front ends in other languages. Values are read and written by path with
//! [`get`] and [`set`], through the config's own serde form, so a path that round-trips here is
//! one the TOML config accepts.

mod table;
mod view;

use serde::Serialize;
use serde_json::Value;

use crate::config::PipelineConfig;

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
    /// What it does, without its link.
    pub help: String,
    /// A reference for it (a DOI link, or the method's page), when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
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
    /// An echo to align onto: one of `options`, where the option whose value is a number
    /// (`"1"`) stands for any 1-based echo number, written as a string (`"3"`).
    EchoReference { options: Vec<EnumOption> },
    /// A mask recipe: a list of mask sections (`masking.sections` in the config). Edited with a
    /// mask builder rather than a field.
    MaskSections,
    /// A list of mask refinement steps.
    MaskOps,
}

/// One user-facing setting.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Setting {
    /// Where the value lives in the config, dot-separated (`bg_removal.vsharp.max_radius`).
    pub path: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// For a setting the TUI shows as a checkbox with a value under it (`off` is set): the
    /// value's own label and help.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_label: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_help: Option<&'static str>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// QSMxT's default.
    pub default: Value,
    /// The default in the browser: QSMxT's, except where the browser profile changes it to fit
    /// the browser's memory.
    pub browser_default: Value,
    pub level: Level,
    pub tab: Tab,
    pub section: Section,
    /// Other sections that show the same setting (it is one value wherever it shows).
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
    /// Whether a browser run can use it (the settings that read a BIDS derivatives directory or
    /// write DICOM cannot).
    pub in_browser: bool,
}

/// Config leaves that are deliberately not in the schema, with why.
pub const NOT_IN_SCHEMA: &[(&str, &str)] = &[
    ("description", "a free-text note saved with the config, not a setting"),
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
    Enum(Vec<(Value, &'static str, &'static str)>),
    EchoReference(Vec<(Value, &'static str, &'static str)>),
    /// The SynthSeg regions ([`crate::regions::REGIONS`]).
    Regions,
    MaskSections,
    MaskOps,
}

/// A table entry.
pub(crate) struct Def {
    path: &'static str,
    section: Section,
    label: &'static str,
    help: &'static str,
    ty: Ty,
    value: Option<(&'static str, &'static str)>,
    basic: bool,
    units: Option<&'static str>,
    min: Option<f64>,
    max: Option<f64>,
    off: Option<Value>,
    optional: bool,
    flag: Option<&'static str>,
    parent: Option<&'static str>,
    also: &'static [Section],
    in_browser: bool,
    when: Option<fn(&view::Ctx) -> bool>,
}

pub(crate) fn d(path: &'static str, section: Section, label: &'static str, help: &'static str, ty: Ty) -> Def {
    Def {
        path, section, label, help, ty, value: None, basic: false, units: None, min: None, max: None,
        off: None, optional: false, flag: None, parent: None, also: &[], in_browser: true, when: None,
    }
}

pub(crate) fn o(value: impl Into<Value>, label: &'static str, help: &'static str) -> (Value, &'static str, &'static str) {
    (value.into(), label, help)
}

impl Def {
    fn value(mut self, label: &'static str, help: &'static str) -> Self { self.value = Some((label, help)); self }
    fn basic(mut self) -> Self { self.basic = true; self }
    fn units(mut self, u: &'static str) -> Self { self.units = Some(u); self }
    fn min(mut self, v: f64) -> Self { self.min = Some(v); self }
    fn max(mut self, v: f64) -> Self { self.max = Some(v); self }
    fn off(mut self, v: Value) -> Self { self.optional |= v.is_null(); self.off = Some(v); self }
    fn optional(mut self) -> Self { self.optional = true; self }
    fn flag(mut self, f: &'static str) -> Self { self.flag = Some(f); self }
    fn parent(mut self, p: &'static str) -> Self { self.parent = Some(p); self }
    fn also(mut self, s: &'static [Section]) -> Self { self.also = s; self }
    fn not_in_browser(mut self) -> Self { self.in_browser = false; self }
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

fn options(opts: &[(Value, &'static str, &'static str)]) -> Vec<EnumOption> {
    opts.iter()
        .map(|(value, label, help)| {
            let (help, url) = split_link(help);
            EnumOption { value: value.clone(), label, help, url }
        })
        .collect()
}

/// The reference regions as options: the same help the TUI's reference picker gives them.
fn region_options() -> Vec<EnumOption> {
    crate::regions::REGIONS
        .iter()
        .map(|r| {
            let ids = r.ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
            EnumOption {
                value: Value::from(r.slug),
                label: r.slug,
                help: region_help(&r.label, r.ids.len(), &ids, r.v2_only),
                url: None,
            }
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

/// Every user-facing setting, in the TUI's order.
pub fn schema() -> Vec<Setting> {
    let native = serde_json::to_value(PipelineConfig::default()).expect("the default config serialises");
    // The browser profile is not defined yet: the browser runs QSMxT's defaults.
    let browser = native.clone();
    table::table()
        .into_iter()
        .enumerate()
        .map(|(order, d)| {
            let value_type = match &d.ty {
                Ty::Bool => ValueType::Bool,
                Ty::Int => ValueType::Int,
                Ty::Float => ValueType::Float,
                Ty::IntVec3 => ValueType::IntVec3,
                Ty::FloatVec3 => ValueType::FloatVec3,
                Ty::Text => ValueType::Text,
                Ty::Enum(o) => ValueType::Enum { options: options(o) },
                Ty::EchoReference(o) => ValueType::EchoReference { options: options(o) },
                Ty::Regions => ValueType::Enum { options: region_options() },
                Ty::MaskSections => ValueType::MaskSections,
                Ty::MaskOps => ValueType::MaskOps,
            };
            let step = matches!(d.ty, Ty::Int | Ty::IntVec3).then_some(1.0);
            Setting {
                path: d.path,
                label: d.label,
                help: d.help,
                value_label: d.value.map(|v| v.0),
                value_help: d.value.map(|v| v.1),
                value_type,
                optional: d.optional,
                off: d.off.clone(),
                units: d.units,
                min: d.min,
                max: d.max,
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
                in_browser: d.in_browser,
            }
        })
        .collect()
}

/// [`schema`] as JSON: `{ "tabs": [...], "sections": [...], "settings": [...] }`, the tab and
/// section entries giving each one's id, title and (for sections) tab.
pub fn schema_json() -> String {
    #[derive(Serialize)]
    struct TabEntry { id: Tab, label: &'static str }
    #[derive(Serialize)]
    struct SectionEntry { id: Section, label: &'static str, tab: Tab }
    #[derive(Serialize)]
    struct Doc { tabs: Vec<TabEntry>, sections: Vec<SectionEntry>, settings: Vec<Setting> }
    let doc = Doc {
        tabs: Tab::ALL.iter().map(|&t| TabEntry { id: t, label: t.label() }).collect(),
        sections: Section::ALL.iter().map(|&s| SectionEntry { id: s, label: s.label(), tab: s.tab() }).collect(),
        settings: schema(),
    };
    serde_json::to_string(&doc).expect("the schema serialises")
}

/// The setting at `path`.
pub fn setting(path: &str) -> Option<Setting> {
    schema().into_iter().find(|s| s.path == path)
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
    if !map.contains_key(key) && !table::table().iter().any(|d| d.path == path) {
        return Err(format!("no setting {path}"));
    }
    map.insert(key.to_string(), value);
    *config = serde_json::from_value(v).map_err(|e| format!("{path}: {e}"))?;
    Ok(())
}
