//! Every registry setting with a flag is said by the command when it is changed and applies:
//! a setting the command leaves out is lost when the command is run again.

use serde_json::{json, Value};

use crate::config::PipelineConfig;
use crate::schema::{self, Setting, ValueType, ViewFacts};

/// Values other than `s`'s default, to try in turn: a value can be refused by the setting's own
/// rules, or put back by normalisation, where another is not.
fn other_values(s: &Setting) -> Vec<Value> {
    let d = &s.default;
    let in_range = |x: f64| x.is_finite() && s.in_range(x);
    let numbers = |x: f64, whole: bool| -> Vec<f64> {
        let mut c = vec![x + 1.0, x - 1.0, x * 2.0, x / 2.0];
        c.extend(s.min.into_iter().chain(s.max).flat_map(|m| [m, m + 1.0, m - 1.0, m + 0.5]));
        c.into_iter().filter(|v| *v != x && in_range(*v) && (!whole || v.fract() == 0.0)).collect()
    };
    let whole = matches!(s.value_type, ValueType::Int | ValueType::IntVec3 | ValueType::IntList);
    // Whole numbers as integers: the config's counts do not read `3.0`.
    let num = |v: f64| if whole { json!(v as i64) } else { json!(v) };
    match &s.value_type {
        ValueType::Bool => vec![json!(!d.as_bool().unwrap_or(false))],
        ValueType::Int | ValueType::Float => numbers(d.as_f64().unwrap_or(1.0), whole).into_iter().map(num).collect(),
        ValueType::IntVec3 | ValueType::FloatVec3 | ValueType::IntList => {
            // An unset vector (`auto`) gets one of three numbers.
            let xs: Vec<f64> = d.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_else(|| vec![1.0; 3]);
            let first = xs.first().copied().unwrap_or(1.0);
            numbers(first, whole).into_iter().map(|v| Value::Array(xs.iter().map(|_| num(v)).collect())).collect()
        }
        ValueType::Text => vec![json!("acq"), json!("*")],
        ValueType::Enum { .. } | ValueType::EchoReference { .. } | ValueType::EchoTime { .. } | ValueType::NumberOrChoice { .. } => {
            let mut c: Vec<Value> = s.value_type.options().iter().map(|o| o.value.clone()).filter(|v| v != d).collect();
            c.extend([json!(2), json!({ "seconds": 0.012 }), json!(0.3)]);
            c
        }
        ValueType::MaskSections | ValueType::MaskOps => vec![],
    }
}

/// `start` with values of the choices above `s` (its parents, their parents...), every
/// combination, `start` first: one of them makes `s` apply.
fn ancestor_configs(s: &Setting, start: PipelineConfig) -> Vec<PipelineConfig> {
    let mut configs = vec![start];
    let mut parent = s.parent;
    while let Some(p) = parent {
        let ps = schema::setting(p).expect("a parent is a setting");
        let values: Vec<Value> = match &ps.value_type {
            ValueType::Bool => vec![json!(true), json!(false)],
            // The option the setting's path names first (`inversion.rts.tol` under `rts`): the
            // search is then quick.
            ValueType::Enum { options } => {
                let mut v: Vec<Value> = options.iter().map(|o| o.value.clone()).collect();
                let named = |v: &Value| v.as_str().is_some_and(|o| s.path.contains(&format!(".{}.", o.replace('-', "_"))));
                v.sort_by_key(|v| !named(v));
                v
            }
            ValueType::Text => vec![json!("acq")],
            // An optional number that switches its children on (the DL tile size).
            ValueType::Int if ps.optional => vec![json!(64)],
            _ => vec![],
        };
        configs = configs.iter().flat_map(|c| {
            std::iter::once(c.clone()).chain(values.iter().filter_map(|v| {
                let mut c = c.clone();
                schema::set(&mut c, p, v.clone()).ok().map(|_| c)
            }))
        }).collect();
        parent = ps.parent;
    }
    configs
}

/// Configs one change away from the default, for a setting its parents alone do not make apply
/// (`of`): every value of every switch and choice of its section, and the mask recipes BET's
/// settings and the reliable mask's combine mode need.
fn one_change_away(of: &Setting) -> Vec<PipelineConfig> {
    let mut configs = Vec::new();
    for s in schema::settings().iter().filter(|s| s.section == of.section) {
        let values: Vec<Value> = match &s.value_type {
            ValueType::Bool => vec![json!(!s.default.as_bool().unwrap_or(false))],
            ValueType::Enum { options } => options.iter().map(|o| o.value.clone()).filter(|v| *v != s.default).collect(),
            _ => vec![],
        };
        for v in values {
            let mut c = PipelineConfig::default();
            if schema::set(&mut c, s.path, v).is_ok() {
                configs.push(c);
            }
        }
    }
    let bet = crate::masking::mask_preset("bet").expect("the bet preset");
    let mut c = PipelineConfig::default();
    (c.masking.sections, c.masking.combine, c.masking.refinements) = (bet.sections, bet.combine, bet.refinements);
    configs.push(c);
    let mut c = PipelineConfig::default();
    c.masking.two_pass = true;
    c.masking.two_pass_sections = Some([crate::masking::default_two_pass_sections(), crate::masking::default_two_pass_sections()].concat());
    configs.push(c);
    configs
}

/// Whether a command for `config` names one of `longs`. The command is the one
/// [`super::generate_command`] writes for a config that follows no toolbox pipeline: finding the
/// pipeline a config follows is most of its cost, and a pipeline's own settings are not said.
fn says(config: &PipelineConfig, longs: &[String]) -> bool {
    let pruned = schema::prune_hidden(&config.normalised().0, &ViewFacts::default());
    super::command_for(&pruned, &PipelineConfig::default(), None).iter().any(|w| longs.iter().any(|l| l == w))
}

/// Whether some config that makes `s` apply, with `s` changed from its default, says its flag.
fn said(s: &Setting) -> bool {
    let flag = s.cli_flag.expect("a flag");
    let longs: Vec<String> = std::iter::once(flag.to_string()).chain(
        crate::schema::cli::PIPELINE_FLAGS.iter()
            .filter(|f| crate::schema::cli_apply::opposite_of(f).is_some_and(|(o, _)| format!("--{}", o.long) == flag))
            .map(|f| format!("--{}", f.long))).collect();
    // The configs one change away are made only for a setting the default and its parents do
    // not show.
    let starts = std::iter::once(PipelineConfig::default()).chain(std::iter::once(s).flat_map(one_change_away));
    starts.flat_map(|start| ancestor_configs(s, start)).any(|base| {
        // Whether the setting applies hardly ever turns on its own value: asked once a config.
        schema::visible_settings(&base, &ViewFacts::default()).iter().any(|(p, shown)| *p == s.path && *shown)
            && other_values(s).into_iter().any(|v| {
                let mut c = base.clone();
                schema::set(&mut c, s.path, v).is_ok() && says(&c, &longs)
            })
    })
}

/// Every registry setting with a flag, changed where it applies, is in the command. (A flag two
/// settings share is the first one's.) Spread over a few threads: a command takes a while.
#[test]
fn every_changed_setting_is_said() {
    let mut settings: Vec<&Setting> = Vec::new();
    for s in schema::settings().iter().filter(|s| s.cli_flag.is_some()) {
        if !settings.iter().any(|x| x.cli_flag == s.cli_flag) {
            settings.push(s);
        }
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    let unsaid: Vec<&str> = std::thread::scope(|scope| {
        let handles: Vec<_> = settings.chunks(settings.len().div_ceil(threads))
            .map(|chunk| scope.spawn(move || chunk.iter().filter(|s| !said(s)).map(|s| s.path).collect::<Vec<_>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a thread")).collect()
    });
    assert!(unsaid.is_empty(), "changed but not in the command: {unsaid:#?}");
}
