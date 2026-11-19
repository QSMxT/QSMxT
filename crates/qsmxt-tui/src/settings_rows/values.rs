//! Settings' values as the TUI shows and takes them: the widgets' defaults, the registry's
//! settings by path, select options, and the parsing and refusal of typed text.

use super::*;

/// The number a number-or-choice setting's select offers first, as its row words it:
/// `(path, label, help)`. Its value row edits the number.
pub(super) const NUMBER_OPTIONS: &[(&str, &str, &str)] = &[
    ("separation.decompose.chi_bound", "ppm", "A fixed bound, in ppm (Chi bound (ppm); default: 0.5)"),
];

/// Text rows that read "(empty)" rather than "(default)" while blank, as the run form's rows did
/// before their settings moved here.
pub(super) const READS_EMPTY: &[&str] = &[
    "swi.strength", "swi.hp_sigma", "swi.mip_window", "smwi.threshold_ppm", "smwi.power", "smwi.mip_window",
    "segmentation.crop", "segmentation.patch", "segmentation.sigma_smoothing",
];

/// Text rows that read blank at their default, which their placeholder names ("0 (none)").
pub(super) const BLANK_AT_DEFAULT: &[&str] = &["multi_orientation.lambda"];

/// Triples typed as one value for every axis, or three, and shown as one while all three agree.
pub(super) const ONE_VALUE_FOR_EVERY_AXIS: &[&str] = &["field_mapping.laplacian_fft_pad"];

/// Echo numbers the config counts from 0 and their rows from 1, as the command line does.
pub(super) const COUNTED_FROM_ONE: &[&str] = &["field_mapping.romeo.template"];

/// Help the TUI words otherwise than the registry: a row counting from 1 says so.
pub(super) const TUI_HELP: &[(&str, &str)] = &[("field_mapping.romeo.template", "Echo index for spatial unwrapping (1-indexed)")];

pub(super) fn tui_help(s: &Setting) -> &'static str {
    TUI_HELP.iter().find(|(p, _)| *p == s.path).map_or(s.tui_help, |(_, h)| h)
}

/// Makes a setting's value on a fresh form ([`FORM_DEFAULTS`]).
pub(super) type FormDefault = fn() -> Value;

/// Settings a fresh form holds otherwise than QSMxT's default, and so always passes on while they
/// apply. Unset, the reliable mask is the one the run resolves it to
/// (`default_two_pass_sections`); the form holds that mask, so that the mask builder shows and
/// edits it, and passes it on whenever two-pass is on, as it always has.
pub(super) const FORM_DEFAULTS: &[(&str, FormDefault)] = &[
    ("masking.two_pass_sections", || serde_json::to_value(qsmxt_engine::default_two_pass_sections()).unwrap_or_default()),
];

/// A setting's value on a fresh form: [`FORM_DEFAULTS`]'s, else QSMxT's default.
pub(super) fn form_default(path: &str) -> Value {
    FORM_DEFAULTS.iter().find(|(p, _)| *p == path).map_or_else(|| setting(path).default.clone(), |(_, v)| v())
}

/// Makes the value a checkbox takes when it is ticked ([`ON_VALUES`]).
pub(super) type OnValue = fn() -> Value;

/// The value a checkbox with a value takes when it is ticked, for one off by default.
pub(super) const ON_VALUES: &[(&str, OnValue)] = &[
    // ROMEO's own threshold for re-unwrapping, which QSMxT leaves off.
    ("field_mapping.romeo.temporal_uncertain_unwrapping",
     || Value::from(qsm_core::unwrap::RomeoParams::default().temporal_uncertain_unwrapping)),
    // Tiles of 128 voxels.
    ("inversion.tile_size", || Value::from(128)),
];

/// The value of a checkbox with a value when it is on and nothing else is chosen: its default,
/// or for one off by default, [`ON_VALUES`].
pub(super) fn on_value(s: &Setting) -> Value {
    ON_VALUES.iter().find(|(p, _)| *p == s.path).map_or_else(|| s.default.clone(), |(_, v)| v())
}

/// Makes the time an echo-time row starts at ([`ECHO_TIME_MS`]).
pub(super) type MsDefault = fn() -> String;

/// What a method's echo-time row starts at when a time is chosen: its default choice on
/// qsm-core's default echo times (blank if those give none).
pub(super) const ECHO_TIME_MS: &[(&str, MsDefault)] = &[
    ("inversion.nltv.te", || { let p = qsm_core::inversion::NltvParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.medi.te", || { let p = qsm_core::inversion::MediParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.tfi.te", || { let p = qsm_core::inversion::TfiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.ndi.te", || { let p = qsm_core::inversion::NdiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.fansi.te", || { let p = qsm_core::inversion::FansiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.l1qsm.te", || { let p = qsm_core::inversion::L1QsmParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.whqsm.te", || { let p = qsm_core::inversion::WhQsmParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.heidi.te", || { let p = qsm_core::inversion::HeidiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.star.te", || { let p = qsm_core::inversion::StarParams::default(); ms_default(p.te, &p.echo_times) }),
    // TGV's parameters carry no echo times: the ones qsm-core's NDI (also at the first echo by
    // default) starts from.
    ("inversion.tgv.te", || ms_default(qsm_core::inversion::TgvParams::default().te, &qsm_core::inversion::NdiParams::default().echo_times)),
];

pub(super) fn ms_default(te: qsm_core::EchoTime, echo_times: &[f64]) -> String {
    te.resolve(echo_times).map(|s| qsmxt_engine::EchoTimeChoice::Seconds(s).to_string()).unwrap_or_default()
}

/// The time an echo-time row starts at ([`ECHO_TIME_MS`]).
pub(super) fn echo_time_ms(path: &str) -> String {
    ECHO_TIME_MS.iter().find(|(p, _)| *p == path).map(|(_, f)| f()).unwrap_or_default()
}

/// The number of a number-or-choice setting when the number is chosen: its default, when that is
/// a number, else the number its value row's help names (none).
pub(super) fn number_default(s: &Setting) -> Value {
    if s.default.is_number() { s.default.clone() } else { Value::Null }
}

/// An echo time in ms, as the config holds it.
pub(super) fn echo_time_of_ms(text: &str) -> Option<Value> {
    text.trim().parse::<f64>().ok()?;
    serde_json::to_value(qsmxt_engine::EchoTimeChoice::parse(text).ok()?).ok()
}

/// Every setting, by path.
pub(super) fn by_path() -> &'static HashMap<&'static str, &'static Setting> {
    pub(super) static MAP: LazyLock<HashMap<&'static str, &'static Setting>> =
        LazyLock::new(|| schema::settings().iter().map(|s| (s.path, s)).collect());
    &MAP
}

pub(crate) fn setting(path: &str) -> &'static Setting {
    by_path().get(path).unwrap_or_else(|| panic!("no setting {path}"))
}

/// The config path of a row's field, when its setting is kept in [`SettingsStore`].
pub fn stored_path(field: &str) -> Option<&'static str> {
    field_path(field).filter(|p| STORED_SECTIONS.contains(&setting(p).section)).filter(|_| !TUI_CHOICES.contains(&field))
}

/// A select's options and their help, as the TUI words them, for the setting at `path`.
pub fn options(path: &str) -> (&'static [&'static str], &'static [&'static str]) {
    pub(super) type Options = (Vec<&'static str>, Vec<&'static str>);
    pub(super) static MAP: LazyLock<HashMap<&'static str, Options>> = LazyLock::new(|| {
        schema::settings().iter()
            .map(|s| {
                let mut opts = s.value_type.options();
                if let Some(&(_, choice, other)) = picker(s.path) {
                    opts.retain(|o| o.value != choice);
                    opts.extend(setting(other).value_type.options());
                }
                let mut labels: Vec<&'static str> = opts.iter().map(|o| o.label).collect();
                let mut help: Vec<&'static str> = opts.iter().map(|o| match s.value_type {
                    // The echo times are every method's; the row says which is this one's default.
                    ValueType::EchoTime { .. } if o.value == s.default =>
                        Box::leak(format!("{} (default)", tui_option_help(o)).into_boxed_str()),
                    _ => tui_option_help(o),
                }).collect();
                // A number, offered before the named choices.
                if let Some(&(_, label, h)) = NUMBER_OPTIONS.iter().find(|(p, ..)| *p == s.path) {
                    labels.insert(0, label);
                    help.insert(0, h);
                }
                // A name for a value with a switch turned on, listed after that value.
                for a in schema::aliases::PICKER_ALIASES.iter().filter(|a| a.setting == s.path) {
                    if let Some(i) = opts.iter().position(|o| o.value == a.value) {
                        labels.insert(i + 1, a.name);
                        help.insert(i + 1, a.help);
                    }
                }
                (s.path, (labels, help))
            })
            .collect()
    });
    let (labels, help) = &MAP[path];
    (labels, help)
}

/// An option's help as the TUI shows it: with its link, which the registry keeps apart. Made once
/// per option, so kept for good.
pub(super) fn tui_option_help(o: &'static schema::EnumOption) -> &'static str {
    match &o.url {
        // After a sentence, the link follows it; after a name, a dash leads to it.
        Some(url) if o.tui_help.ends_with('.') => Box::leak(format!("{} {url}", o.tui_help).into_boxed_str()),
        Some(url) => Box::leak(format!("{} — {url}", o.tui_help).into_boxed_str()),
        None => o.tui_help.as_str(),
    }
}

/// `label` indented `depth` levels. Rows carry `&'static str` labels, so each indented label is
/// made once and kept; there are as many as the registry has labels, at most a few levels deep.
pub(super) fn indented(depth: usize, label: &'static str) -> &'static str {
    if depth == 0 {
        return label;
    }
    pub(super) static MADE: LazyLock<Mutex<HashMap<(usize, &'static str), &'static str>>> = LazyLock::new(Default::default);
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    made.entry((depth, label))
        .or_insert_with(|| Box::leak(format!("{}{label}", "  ".repeat(depth)).into_boxed_str()))
}

/// How many settings `s` is listed under on its own section's page.
pub(crate) fn depth_of(s: &Setting) -> usize {
    match s.parent.map(setting) {
        Some(p) if p.section == s.section => 1 + depth_of(p),
        _ => 0,
    }
}

/// Write `value` at `path` in a config being built as JSON.
pub(super) fn put(config: &mut Value, path: &str, value: Value) {
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
pub(super) fn config_with(values: &[(&'static str, Value)]) -> Option<PipelineConfig> {
    config_try(values).ok()
}

/// `config` with these values written onto it; a value it refuses is left as `config` has it.
pub fn with_values(config: &PipelineConfig, values: &[(&'static str, Value)]) -> PipelineConfig {
    let Ok(mut json) = serde_json::to_value(config) else { return config.clone() };
    for (p, v) in values {
        put(&mut json, p, v.clone());
    }
    serde_json::from_value(json).unwrap_or_else(|_| {
        let mut out = config.clone();
        for (p, v) in values {
            let _ = schema::set(&mut out, p, v.clone());
        }
        out
    })
}

/// [`config_with`], or why the config refuses the values.
pub(super) fn config_try(values: &[(&'static str, Value)]) -> Result<PipelineConfig, String> {
    let mut json = Value::Object(Default::default());
    for (p, v) in values {
        put(&mut json, p, v.clone());
    }
    serde_json::from_value(json).map_err(|e| e.to_string())
}

/// A value as its text row shows it: a number as Rust writes it (4, 0.5), a triple or list with
/// spaces between, nothing for an unset value.
pub(super) fn text_of(v: &Value) -> String {
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

/// The value text typed on a row of `s` stands for, or `None` for a blank or unreadable one. A
/// blank one leaves the setting at its default (as the command line does without the option);
/// an unreadable one is refused while it applies ([`SettingsStore::changed_or_refused`]).
pub(super) fn parse(s: &Setting, text: &str) -> Option<Value> {
    let text = text.trim();
    let numbers = || text.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty());
    let number = |t: &str| t.parse::<f64>().ok().filter(|x| x.is_finite()).map(Value::from);
    let int = |t: &str| t.parse::<u64>().ok().map(Value::from).or_else(|| t.parse::<i64>().ok().map(Value::from));
    match s.value_type {
        _ if text.is_empty() && !matches!(s.value_type, ValueType::Text) => None,
        // Typed from 1; 0 is the first echo too, as the command line takes it.
        ValueType::Int if COUNTED_FROM_ONE.contains(&s.path) => text.parse::<u64>().ok().map(|n| Value::from(n.saturating_sub(1))),
        ValueType::Int => int(text),
        ValueType::Float => number(text),
        ValueType::FloatVec3 => numbers().map(number).collect::<Option<Vec<_>>>().filter(|v| v.len() == 3).map(Value::from),
        ValueType::IntVec3 if ONE_VALUE_FOR_EVERY_AXIS.contains(&s.path) => {
            let v = text.split_whitespace().map(int).collect::<Option<Vec<_>>>()?;
            match v.len() {
                1 => Some(Value::from(vec![v[0].clone(); 3])),
                3 => Some(Value::from(v)),
                _ => None,
            }
        }
        // Whole-number triples are typed with spaces, as their rows always took them.
        ValueType::IntVec3 => text.split_whitespace().map(int).collect::<Option<Vec<_>>>().filter(|v| v.len() == 3).map(Value::from),
        ValueType::IntList => numbers().map(int).collect::<Option<Vec<_>>>().filter(|v| !v.is_empty()).map(Value::from),
        ValueType::Text if text.is_empty() => Some(if s.optional { Value::Null } else { Value::from("") }),
        ValueType::Text => Some(Value::from(text)),
        ValueType::EchoTime { .. } => echo_time_of_ms(text),
        ValueType::NumberOrChoice { .. } => number(text),
        // The echo number of an echo reference, written as the config writes it ("3").
        ValueType::EchoReference { .. } => echo_number(s, &Value::from(text)).map(|n| Value::from(n.to_string())),
        _ => None,
    }
}

/// A setting whose typed text the run cannot take, and why.
pub type Refused = (&'static str, String);

/// What a text row of `s` takes, as a message refusing other text words it ("not a number").
pub(super) fn what_it_takes(s: &Setting) -> &'static str {
    match s.value_type {
        ValueType::Int if COUNTED_FROM_ONE.contains(&s.path) => "an echo number",
        ValueType::Int => "a whole number",
        ValueType::Float | ValueType::NumberOrChoice { .. } => "a number",
        ValueType::FloatVec3 => "three numbers",
        ValueType::IntVec3 if ONE_VALUE_FOR_EVERY_AXIS.contains(&s.path) => "one whole number, or three",
        ValueType::IntVec3 => "three whole numbers",
        ValueType::IntList => "a list of whole numbers",
        ValueType::EchoTime { .. } => "a time in ms",
        _ => "a value this setting takes",
    }
}

/// The echo number an echo reference's value gives, when it gives one the setting takes.
pub(super) fn echo_number(s: &Setting, v: &Value) -> Option<u32> {
    let ValueType::EchoReference { options } = &s.value_type else { return None };
    let n: u32 = v.as_str()?.trim().parse().ok()?;
    options.iter().any(|o| matches!(o.kind, EchoKind::EchoNumber { min } if n >= min)).then_some(n)
}
