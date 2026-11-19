//! Setting ranges: a range in words, and the checks of a config's values against them.

use serde_json::Value;

use super::{lookup, settings, settings_view, view, Setting, ValueType, ViewFacts};
use crate::config::PipelineConfig;

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
    // Settings that cannot be run (QSMART's echo fit with a threshold qsm-core refuses).
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
    use crate::schema::{get, schema, set};

    /// A value just outside each end of the range, of the setting's type.
    fn outside(s: &Setting) -> Vec<f64> {
        let int = matches!(s.value_type, ValueType::Int | ValueType::IntVec3 | ValueType::IntList);
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
            ValueType::IntList => json!([x as u64, x as u64]),
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

    /// N4's settings are refused with QSM.rs's message (`N4Params::validate`), on the setting it
    /// names, when N4 is the method.
    #[test]
    fn n4_settings_are_refused_with_qsm_rs_message() {
        let mut c = PipelineConfig::default();
        c.n4.max_iterations.clear();
        assert!(check_ranges(&c).is_ok(), "makehomogeneous does not read N4's settings");
        c.masking.inhomogeneity_method = crate::InhomogeneityMethod::N4;
        let e = check_ranges(&c).unwrap_err().to_string();
        assert!(e.contains("n4.max_iterations (--n4-max-iterations): N4 max_iterations = []"), "{e}");
        c.n4.max_iterations = vec![10];
        c.n4.spline_order = 4;
        let e = check_ranges(&c).unwrap_err().to_string();
        assert!(e.contains("n4.spline_order (--n4-spline-order): N4 spline_order = 4 is out of range: must be 1, 2 or 3"), "{e}");
        assert_eq!(e.matches("spline_order").count(), 2, "said once: {e}");
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

    /// QSMART's echo fit runs on ROMEO's echoes, bipolar correction included: nothing refuses
    /// it, and nothing is switched for it.
    #[test]
    fn the_echo_fit_with_romeo_is_accepted() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = crate::QsmAlgorithm::Qsmart;
        c.field_mapping.b0_estimation = crate::B0Estimation::QsmartEchofit;
        c.field_mapping.bipolar_correction = true;
        let n = c.normalised().0;
        assert_eq!(n, c);
        assert!(check_ranges(&n).is_ok());
        let view = settings_view(&c, &ViewFacts::default());
        let b0 = view.iter().find(|v| v.path == "field_mapping.b0_estimation").unwrap();
        assert!(b0.error.is_none(), "{:?}", b0.error);
        // The unmasked unwrapping is a Laplacian setting: shown only with Laplacian unwrapping.
        let shown = |c: &PipelineConfig| settings_view(c, &ViewFacts::default()).iter()
            .any(|v| v.path == "field_mapping.laplacian_unmasked" && v.visible);
        assert!(!shown(&c));
        c.field_mapping.unwrapping_algorithm = crate::UnwrappingAlgorithm::Laplacian;
        assert!(shown(&c));
    }

    /// PRELUDE's settings are shown only with PRELUDE, ROMEO's only with ROMEO; phase offset
    /// removal and bipolar correction are shown with PRELUDE as with ROMEO. A partition count
    /// below 1 is refused.
    #[test]
    fn prelude_settings_are_gated_on_prelude() {
        let shown = |c: &PipelineConfig, path: &str| settings_view(c, &ViewFacts::default()).iter()
            .any(|v| v.path == path && v.visible);
        let mut c = PipelineConfig::default();
        assert!(!shown(&c, "field_mapping.prelude.labelling") && !shown(&c, "field_mapping.prelude.num_phase_partitions"));
        assert!(shown(&c, "field_mapping.romeo.individual"));
        c.field_mapping.unwrapping_algorithm = crate::UnwrappingAlgorithm::Prelude;
        for path in ["field_mapping.prelude.labelling", "field_mapping.prelude.num_phase_partitions",
                     "field_mapping.phase_offset_removal", "field_mapping.bipolar_correction"] {
            assert!(shown(&c, path), "{path}");
        }
        for path in ["field_mapping.romeo.individual", "field_mapping.romeo.correct_global", "field_mapping.laplacian_solver"] {
            assert!(!shown(&c, path), "{path}");
        }
        assert_eq!(get(&c, "field_mapping.prelude.labelling"), Some(serde_json::json!("auto")));
        set(&mut c, "field_mapping.prelude.labelling", serde_json::json!("volume")).unwrap();
        assert_eq!(c.field_mapping.prelude.labelling, crate::PreludeLabelling::Volume);
        assert!(check_ranges(&c).is_ok());
        c.field_mapping.prelude.num_phase_partitions = 0;
        assert!(check_ranges(&c).is_err());
    }
}
