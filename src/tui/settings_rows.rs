//! Rows built from the engine's settings registry ([`qsmxt_engine::schema`]), and the settings
//! they edit, kept in a config rather than in fields of the form.
//!
//! A page built here lists its section's settings in the registry's order, each shown while
//! `settings_view` says it applies, with one widget per value type: a checkbox for a bool, a
//! select for a choice, a text row for anything typed. Labels, help and options are the
//! registry's TUI wording; a setting is indented once for each setting it is listed under on the
//! same page. The rows keep the form's field names ([`FIELD_PATHS`]), which the rest of the TUI
//! (focus, advice, Basic mode) knows them by.
//!
//! The settings of the sections in [`STORED_SECTIONS`] live in [`SettingsStore`]: a config, plus
//! the text typed on their text rows. Other settings still live in the form's own fields until
//! their sections move here.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

use qsmxt_engine::schema::{self, EchoKind, Section, Setting, ValueType, ViewFacts};
use qsmxt_engine::PipelineConfig;

use super::app::{field_of_path, field_path, App, Page, PipelineRow, RowContext, FIELD_PATHS};

/// The sections whose settings are kept in [`SettingsStore`] rather than in the form's fields.
pub const STORED_SECTIONS: &[Section] = &[Section::Geometry, Section::Motion, Section::Magnitude, Section::SwiSmwi];

/// A note a page makes before a setting's row, from the rows' context (none when there is nothing
/// to say).
type Note = fn(&RowContext) -> Option<String>;

/// A page whose rows come from the registry.
struct PageLayout {
    page: Page,
    section: Section,
    /// For a page that shows part of a section (SWI and SMWI share one): whether it shows the
    /// setting at this path.
    only: Option<fn(&str) -> bool>,
    /// Settings of another section repeated at the end of the page, each under a setting of the
    /// page: `(path, under)`, shown while `under` is on and the setting applies.
    repeats: &'static [(&'static str, &'static str)],
    /// A note before a setting's row, when there is one to make.
    notes: &'static [(&'static str, Note)],
    /// Settings shown as a block under a setting of the page they have no parent in:
    /// `(path prefix, under, heading)`, shown while `under` is on, after a note naming the block.
    blocks: &'static [(&'static str, &'static str, &'static str)],
}

fn is_swi(path: &str) -> bool {
    path == "pipeline.do_swi" || path.starts_with("swi.")
}

const LAYOUTS: &[PageLayout] = &[
    PageLayout { page: Page::Geometry, section: Section::Geometry, only: None, repeats: &[], notes: &[
        // A selected method that only works on axial data resamples whatever the switch says.
        ("pipeline.output_space", |ctx| ctx.axial_forced_by.as_ref()
            .map(|why| format!("  Oblique data is resampled to axial for {why}"))),
    ], blocks: &[] },
    PageLayout {
        page: Page::Motion, section: Section::Motion, only: None, repeats: &[], notes: &[],
        // The rigid registration serves motion correction and the co-registration of
        // orientations (shown there too), so it is no one setting's.
        blocks: &[("registration.", "motion_correction.enabled", "Rigid registration (advanced):")],
    },
    PageLayout { page: Page::Magnitude, section: Section::Magnitude, only: None, repeats: &[], notes: &[], blocks: &[] },
    PageLayout { page: Page::Swi, section: Section::SwiSmwi, only: Some(is_swi), repeats: &[], notes: &[], blocks: &[] },
    PageLayout {
        page: Page::Smwi, section: Section::SwiSmwi, only: Some(|p| !is_swi(p)),
        // SMWI reads a χ map, which may be supplied rather than computed.
        repeats: &[("separation.custom_qsm_tool", "pipeline.do_smwi")],
        notes: &[],
        blocks: &[],
    },
];

/// Text rows that read "(empty)" rather than "(default)" while blank, as the run form's rows did
/// before their settings moved here.
const READS_EMPTY: &[&str] = &[
    "swi.strength", "swi.hp_sigma", "swi.mip_window", "smwi.threshold_ppm", "smwi.power", "smwi.mip_window",
];

/// Every setting, by path.
fn by_path() -> &'static HashMap<&'static str, &'static Setting> {
    static MAP: LazyLock<HashMap<&'static str, &'static Setting>> =
        LazyLock::new(|| schema::settings().iter().map(|s| (s.path, s)).collect());
    &MAP
}

fn setting(path: &str) -> &'static Setting {
    by_path().get(path).unwrap_or_else(|| panic!("no setting {path}"))
}

/// The config path of a row's field, when its setting is kept in [`SettingsStore`].
pub fn stored_path(field: &str) -> Option<&'static str> {
    field_path(field).filter(|p| STORED_SECTIONS.contains(&setting(p).section))
}

/// A select's options and their help, as the TUI words them, for the setting at `path`.
fn options(path: &str) -> (&'static [&'static str], &'static [&'static str]) {
    type Options = (Vec<&'static str>, Vec<&'static str>);
    static MAP: LazyLock<HashMap<&'static str, Options>> = LazyLock::new(|| {
        schema::settings().iter()
            .map(|s| {
                let opts = s.value_type.options();
                (s.path, (opts.iter().map(|o| o.label).collect(), opts.iter().map(|o| o.tui_help.as_str()).collect()))
            })
            .collect()
    });
    let (labels, help) = &MAP[path];
    (labels, help)
}

/// `label` indented `depth` levels. Rows carry `&'static str` labels, so each indented label is
/// made once and kept; there are as many as the registry has labels, at most a few levels deep.
fn indented(depth: usize, label: &'static str) -> &'static str {
    if depth == 0 {
        return label;
    }
    static MADE: LazyLock<Mutex<HashMap<(usize, &'static str), &'static str>>> = LazyLock::new(Default::default);
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    made.entry((depth, label))
        .or_insert_with(|| Box::leak(format!("{}{label}", "  ".repeat(depth)).into_boxed_str()))
}

/// How many settings `s` is listed under on its own section's page.
fn depth_of(s: &Setting) -> usize {
    match s.parent.map(setting) {
        Some(p) if p.section == s.section => 1 + depth_of(p),
        _ => 0,
    }
}

/// Write `value` at `path` in a config being built as JSON.
fn put(config: &mut Value, path: &str, value: Value) {
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let mut obj = config;
    for k in parent.split('.').filter(|k| !k.is_empty()) {
        obj = obj.as_object_mut().expect("an object").entry(k).or_insert_with(|| Value::Object(Default::default()));
    }
    if let Some(map) = obj.as_object_mut() {
        map.insert(key.to_string(), value);
    }
}

/// The config with these values and QSMxT's defaults elsewhere, if it accepts them all. Built
/// from the values alone (every section of the config fills in its own defaults), so it costs
/// as little as there are values.
fn config_with(values: &[(&'static str, Value)]) -> Option<PipelineConfig> {
    let mut json = Value::Object(Default::default());
    for (p, v) in values {
        put(&mut json, p, v.clone());
    }
    serde_json::from_value(json).ok()
}

/// A value as its text row shows it: a number as Rust writes it (4, 0.5), a triple or list with
/// spaces between, nothing for an unset value.
fn text_of(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(xs) => xs.iter().map(text_of).collect::<Vec<_>>().join(" "),
        Value::Number(n) => match (n.as_u64(), n.as_i64(), n.as_f64()) {
            (Some(u), _, _) => u.to_string(),
            (_, Some(i), _) => i.to_string(),
            (_, _, Some(f)) => format!("{f}"),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// The value text typed on a row of `s` stands for, or `None` for a blank or unreadable one,
/// which leaves the setting at its default (as the command line does without the option).
fn parse(s: &Setting, text: &str) -> Option<Value> {
    let text = text.trim();
    let numbers = || text.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty());
    let number = |t: &str| t.parse::<f64>().ok().filter(|x| x.is_finite()).map(Value::from);
    let int = |t: &str| t.parse::<u64>().ok().map(Value::from).or_else(|| t.parse::<i64>().ok().map(Value::from));
    match s.value_type {
        _ if text.is_empty() && !matches!(s.value_type, ValueType::Text) => None,
        ValueType::Int => int(text),
        ValueType::Float => number(text),
        ValueType::FloatVec3 => numbers().map(number).collect::<Option<Vec<_>>>().filter(|v| v.len() == 3).map(Value::from),
        ValueType::IntVec3 => numbers().map(int).collect::<Option<Vec<_>>>().filter(|v| v.len() == 3).map(Value::from),
        ValueType::IntList => numbers().map(int).collect::<Option<Vec<_>>>().filter(|v| !v.is_empty()).map(Value::from),
        ValueType::Text if text.is_empty() => Some(if s.optional { Value::Null } else { Value::from("") }),
        ValueType::Text => Some(Value::from(text)),
        // The echo number of an echo reference, written as the config writes it ("3").
        ValueType::EchoReference { .. } => echo_number(s, &Value::from(text)).map(|n| Value::from(n.to_string())),
        _ => None,
    }
}

/// The echo number an echo reference's value gives, when it gives one the setting takes.
fn echo_number(s: &Setting, v: &Value) -> Option<u32> {
    let ValueType::EchoReference { options } = &s.value_type else { return None };
    let n: u32 = v.as_str()?.trim().parse().ok()?;
    options.iter().any(|o| matches!(o.kind, EchoKind::EchoNumber { min } if n >= min)).then_some(n)
}

/// The settings of [`STORED_SECTIONS`]: the values that differ from QSMxT's defaults, and the
/// text typed on their text rows, by path. A typed text stands for its value (see
/// [`SettingsStore::config`]), so the row shows what was typed, a half-typed number included.
#[derive(Clone, Debug, Default)]
pub struct SettingsStore {
    values: BTreeMap<&'static str, Value>,
    typed: BTreeMap<&'static str, String>,
}

impl SettingsStore {
    /// A setting's value (not counting typed text).
    pub fn get(&self, path: &str) -> Value {
        self.values.get(path).cloned().unwrap_or_else(|| setting(path).default.clone())
    }

    /// Set a value, dropping any text typed for it. A value the config refuses is left out.
    pub fn set(&mut self, path: &'static str, value: impl Into<Value>) {
        let value = value.into();
        self.typed.remove(path);
        if value == setting(path).default {
            self.values.remove(path);
        } else if config_with(&[(path, value.clone())]).is_some() {
            self.values.insert(path, value);
        }
    }

    pub fn toggle(&self, path: &str) -> bool {
        self.get(path).as_bool().unwrap_or(false)
    }

    pub fn flip(&mut self, path: &'static str) {
        let on = self.toggle(path);
        self.set(path, !on);
    }

    /// A choice's position in the setting's options. An echo reference given as an echo number
    /// is its echo-number option.
    pub fn select(&self, path: &str) -> usize {
        let v = self.get(path);
        let s = setting(path);
        let number = echo_number(s, &v).is_some();
        s.value_type.options().iter().position(|o| o.value == v).or_else(|| {
            let ValueType::EchoReference { options } = &s.value_type else { return None };
            options.iter().position(|o| number && matches!(o.kind, EchoKind::EchoNumber { .. }))
        }).unwrap_or(0)
    }

    pub fn set_select(&mut self, path: &'static str, index: usize) {
        if index == self.select(path) {
            return; // an echo number chosen again keeps its number
        }
        let opts = setting(path).value_type.options();
        if let Some(o) = opts.get(index.min(opts.len().saturating_sub(1))) {
            self.set(path, o.value.clone());
        }
    }

    /// A text row's text: what was typed, else the value's.
    pub fn text(&self, path: &str) -> Cow<'_, str> {
        match self.typed.get(path) {
            Some(t) => Cow::Borrowed(t),
            None => Cow::Owned(self.value_text(path)),
        }
    }

    /// The text of a row, to type into.
    pub fn text_mut(&mut self, path: &'static str) -> &mut String {
        if !self.typed.contains_key(path) {
            let text = self.value_text(path);
            self.typed.insert(path, text);
        }
        self.typed.get_mut(path).expect("just inserted")
    }

    /// A value as its text row shows it. An echo reference's row shows its echo number: the
    /// value's, or the first the setting takes while a named echo is chosen.
    fn value_text(&self, path: &str) -> String {
        let s = setting(path);
        let v = self.get(path);
        match &s.value_type {
            ValueType::EchoReference { options } => echo_number(s, &v).map(|n| n.to_string()).unwrap_or_else(|| {
                options.iter().find_map(|o| match o.kind { EchoKind::EchoNumber { min } => Some(min.to_string()), _ => None })
                    .unwrap_or_default()
            }),
            _ => text_of(&v),
        }
    }

    /// Put a setting back to QSMxT's default.
    pub fn reset(&mut self, path: &'static str) {
        self.typed.remove(path);
        self.values.remove(path);
    }

    /// The stored settings that differ from their defaults as the run takes them, typed text
    /// read as its value (a blank or unreadable text leaves the default; so does a value the
    /// config refuses, such as a typed number too large for its setting).
    pub fn changed(&self) -> Vec<(&'static str, Value)> {
        let mut out: BTreeMap<&'static str, Value> = self.values.clone();
        for (&path, text) in &self.typed {
            let s = setting(path);
            match parse(s, text).filter(|v| *v != s.default && config_with(&[(path, v.clone())]).is_some()) {
                Some(v) => out.insert(path, v),
                None => out.remove(path),
            };
        }
        out.into_iter().collect()
    }

    /// The config with these of the stored settings ([`Self::changed`]) and QSMxT's defaults
    /// elsewhere.
    pub fn config_of(&self, values: &[(&'static str, Value)]) -> PipelineConfig {
        config_with(values).unwrap_or_default()
    }

    /// The config of the stored settings, QSMxT's defaults elsewhere.
    pub fn config(&self) -> PipelineConfig {
        self.config_of(&self.changed())
    }
}

/// The placeholder of a blank text row whose setting is stored, when it is one.
pub fn stored_placeholder(path: &str) -> &'static str {
    if READS_EMPTY.contains(&path) { "(empty)" } else { "(default)" }
}

/// The settings that apply to a config ([`schema::visible_settings`]), kept until the config or
/// the facts change: several pages are built for one config.
pub type VisibleCache = RefCell<Option<(PipelineConfig, ViewFacts, Rc<HashSet<&'static str>>)>>;

impl App {
    /// The settings that apply to `config`, from [`Self::visible_cache`] when it has them.
    pub(super) fn visible_settings(&self, config: &PipelineConfig) -> Rc<HashSet<&'static str>> {
        let facts = self.row_facts();
        if let Some((c, f, v)) = &*self.visible_cache.borrow() {
            if c == config && *f == facts {
                return v.clone();
            }
        }
        let visible: Rc<HashSet<&'static str>> = Rc::new(schema::visible_settings(config, &facts).into_iter()
            .filter(|(_, v)| *v).map(|(p, _)| p).collect());
        *self.visible_cache.borrow_mut() = Some((config.clone(), facts, visible.clone()));
        visible
    }

    /// What the rows' visibility is decided from: the echo limit and whether the selected runs
    /// are uncombined multi-coil data.
    pub fn row_facts(&self) -> ViewFacts {
        ViewFacts {
            num_echoes: self.filter_state.num_echoes.trim().parse().ok(),
            multi_coil: Some(self.multi_coil()),
            ..Default::default()
        }
    }

    /// The rows of `page` when they come from the registry; `None` for a page still built by
    /// hand.
    pub(super) fn registry_rows(&self, page: Page, ctx: &RowContext, config: Option<&PipelineConfig>) -> Option<Vec<PipelineRow>> {
        let layout = LAYOUTS.iter().find(|l| l.page == page)?;
        // A form that builds no config (a value the run refuses) still shows its rows, as its
        // stored settings have them.
        let stored;
        let config = match config {
            Some(c) => c,
            None => {
                stored = self.form.settings.config();
                &stored
            }
        };
        let visible = self.visible_settings(config);
        let shows = |path: &str| visible.contains(path);
        let is_on = |path: &str| field_of_path(path).is_some_and(|f| self.field_toggle(f));
        let mut rows = Vec::new();
        let mut headed = HashSet::new();
        for s in schema::settings() {
            if s.section != layout.section || !layout.only.is_none_or(|f| f(s.path)) || !shows(s.path) {
                continue;
            }
            let block = layout.blocks.iter().find(|(prefix, ..)| s.path.starts_with(prefix));
            let depth = match block {
                Some(&(prefix, under, heading)) => {
                    if !is_on(under) {
                        continue;
                    }
                    let depth = depth_of(setting(under)) + 1;
                    if headed.insert(prefix) {
                        rows.push(PipelineRow::Note { text: format!("{}{heading}", "  ".repeat(depth)).into() });
                    }
                    depth + 1
                }
                None => depth_of(s),
            };
            for (_, note) in layout.notes.iter().filter(|(p, _)| *p == s.path) {
                rows.extend(note(ctx).map(|text| PipelineRow::Note { text: text.into() }));
            }
            self.push_rows(&mut rows, s, depth);
        }
        for (path, under) in layout.repeats {
            if is_on(under) && shows(path) {
                self.push_rows(&mut rows, setting(path), depth_of(setting(under)) + 1);
            }
        }
        Some(rows)
    }
}

impl App {
    /// The rows of a setting, by its value type: one, or for an echo reference given as an echo
    /// number, the select and the number's row under it.
    fn push_rows(&self, rows: &mut Vec<PipelineRow>, s: &'static Setting, depth: usize) {
        let field = field_of_path(s.path).unwrap_or_else(|| panic!("{} has no row field", s.path));
        let label = indented(depth, s.label);
        rows.push(match s.value_type {
            ValueType::Bool => PipelineRow::Toggle { label, field, help: s.tui_help },
            ValueType::Enum { .. } | ValueType::EchoReference { .. } => {
                let (options, help) = options(s.path);
                PipelineRow::AlgoSelect { label, field, options, help }
            }
            ValueType::Int | ValueType::Float | ValueType::IntVec3 | ValueType::FloatVec3 | ValueType::IntList
            | ValueType::Text => PipelineRow::Param { label, field, help: s.tui_help },
            _ => unreachable!("{}: no widget for its value type yet", s.path),
        });
        if let ValueType::EchoReference { options } = &s.value_type {
            let chosen = options.get(self.field_select(field)).map(|o| o.kind);
            if let (Some(EchoKind::EchoNumber { .. }), Some((label, help))) = (chosen, s.tui_value) {
                rows.push(PipelineRow::Param { label: indented(depth + 1, label), field: value_field(s.path), help });
            }
        }
    }
}

/// The field of a setting's value row: the second row editing its path (an echo reference's
/// number).
fn value_field(path: &str) -> &'static str {
    FIELD_PATHS.iter().filter(|(_, p)| *p == path).nth(1).map(|(f, _)| *f)
        .unwrap_or_else(|| panic!("{path} has no value row field"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::command::config_from_app;

    /// A text row shows what was typed; the run takes it when it reads as the setting's value,
    /// and the default when it does not (a half-typed number).
    #[test]
    fn typed_text_stands_for_its_value() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_swi", true);
        *app.field_text_mut("swi_strength").unwrap() = "2.5".into();
        assert_eq!(config_from_app(&app).unwrap().swi.strength, 2.5);
        *app.field_text_mut("swi_strength").unwrap() = "2.".into();
        assert_eq!(app.field_text("swi_strength"), "2.");
        assert_eq!(config_from_app(&app).unwrap().swi.strength, 2.0);
        *app.field_text_mut("swi_strength").unwrap() = "x".into();
        assert_eq!(config_from_app(&app).unwrap().swi.strength, qsmxt_engine::SwiConfig::default().strength);
    }

    /// A stored setting that no longer applies is left at its default, though the form keeps it.
    #[test]
    fn a_hidden_setting_is_left_at_its_default() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_swi", true);
        app.set_field_select("swi_scaling", 3);
        assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Negative);
        app.toggle_field("do_swi");
        assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Tanh);
        app.toggle_field("do_swi");
        assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Negative);
    }
}
