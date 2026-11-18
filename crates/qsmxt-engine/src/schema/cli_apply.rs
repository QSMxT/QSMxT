//! A command line's settings flags applied to a configuration: each flag sets its registry
//! setting ([`super::Setting::cli_flag`]) to the value given, read by the setting's
//! [`ValueType`]; the flags that do something else are [`super::cli::NOT_ONE_SETTING`]'s.

use serde_json::Value;

use super::cli::{Flag, FlagAction, FlagParse, NOT_ONE_SETTING, PIPELINE_FLAGS};
use super::{settings, Setting, ValueType};
use crate::config::PipelineConfig;

/// A flag given on the command line, with its values as typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Given {
    pub flag: &'static Flag,
    pub values: Vec<String>,
}

impl Given {
    /// The flag `long` (without `--`) with `values`.
    pub fn new(long: &str, values: Vec<String>) -> Option<Given> {
        PIPELINE_FLAGS.iter().find(|f| f.long == long).map(|flag| Given { flag, values })
    }
}

/// The registry setting a flag sets, unless it is one of [`NOT_ONE_SETTING`].
pub fn setting_of(flag: &Flag) -> Option<&'static Setting> {
    if NOT_ONE_SETTING.contains(&flag.long) {
        return None;
    }
    settings().iter().find(|s| s.cli_flag.is_some_and(|f| f.trim_start_matches("--") == flag.long))
}

fn number(raw: &str) -> Result<Value, String> {
    if let Ok(i) = raw.parse::<i64>() {
        return Ok(Value::from(i));
    }
    raw.parse::<f64>().ok().filter(|x| x.is_finite()).map(Value::from).ok_or_else(|| format!("'{raw}' is not a number"))
}

fn json(v: Result<impl serde::Serialize, String>) -> Result<Value, String> {
    v.and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
}

/// The value `values` give `setting`.
pub fn value_of(setting: &Setting, flag: &Flag, values: &[String]) -> Result<Value, String> {
    let one = || values.first().cloned().unwrap_or_default();
    match flag.parse {
        FlagParse::EchoTime => return json(crate::EchoTimeChoice::parse(&one())),
        FlagParse::ChiBound => return json(crate::ChiBoundChoice::parse(&one())),
        FlagParse::NumberOrAuto if one().trim() == "auto" => return Ok(Value::Null),
        _ => {}
    }
    Ok(match &setting.value_type {
        // `--no-x` turns the setting off.
        ValueType::Bool if flag.action == FlagAction::SetTrue => Value::Bool(!flag.long.starts_with("no-")),
        ValueType::Bool => Value::Bool(one().parse::<bool>().map_err(|e| e.to_string())?),
        ValueType::Int | ValueType::Float => number(&one())?,
        ValueType::IntVec3 | ValueType::FloatVec3 | ValueType::IntList => {
            Value::Array(values.iter().map(|v| number(v)).collect::<Result<_, _>>()?)
        }
        ValueType::Enum { options } | ValueType::NumberOrChoice { options } => {
            let raw = one().trim().to_string();
            match options.iter().find(|o| o.value == Value::String(raw.clone())) {
                Some(o) => o.value.clone(),
                // The command line lists its own values (every inversion for QSMART's inner one,
                // where the registry offers the ones QSMART's authors use).
                None if !flag.values.is_empty() => Value::String(raw),
                None => number(&raw).map_err(|_| expected(setting, &raw))?,
            }
        }
        _ => Value::String(one()),
    })
}

/// "expected a, b or c, got 'x'": the error for a value that is none of `setting`'s options.
pub fn expected(setting: &Setting, got: &str) -> String {
    let names: Vec<String> = setting.value_type.options().iter()
        .map(|o| o.value.as_str().map_or_else(|| o.value.to_string(), str::to_string)).collect();
    let list = match names.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    };
    format!("expected {list}, got '{got}'")
}

/// The registry switch a flag is the opposite of (`--no-x` of `--x`, or `--x` of `--no-x`).
pub fn opposite_of(flag: &Flag) -> Option<(&'static Flag, &'static Setting)> {
    let other = match flag.long.strip_prefix("no-") {
        Some(on) => on.to_string(),
        None => format!("no-{}", flag.long),
    };
    let f = PIPELINE_FLAGS.iter().find(|f| f.long == other)?;
    setting_of(f).filter(|s| s.value_type == ValueType::Bool).map(|s| (f, s))
}

/// Apply the flags that set one registry setting each, in the order given, then the opposites
/// of registry switches (unless the switch itself was given too: it wins); returns the others.
pub fn apply_settings(config: &mut PipelineConfig, given: &[Given]) -> Result<Vec<Given>, String> {
    let mut rest = Vec::new();
    let mut opposites = Vec::new();
    for g in given {
        if setting_of(g.flag).is_none() {
            if let Some((other, setting)) = opposite_of(g.flag) {
                if !given.iter().any(|x| x.flag.long == other.long) {
                    opposites.push((setting, !g.flag.long.starts_with("no-")));
                }
                continue;
            }
        }
        let Some(setting) = setting_of(g.flag) else {
            rest.push(g.clone());
            continue;
        };
        let value = value_of(setting, g.flag, &g.values).map_err(|e| format!("--{}: {e}", g.flag.long))?;
        super::set(config, setting.path, value).map_err(|e| format!("--{}: {e}", g.flag.long))?;
    }
    for (setting, on) in opposites {
        super::set(config, setting.path, Value::Bool(on))?;
    }
    Ok(rest)
}
