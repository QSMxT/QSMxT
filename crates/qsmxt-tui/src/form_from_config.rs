//! The form for a config: the reverse of [`super::command::config_from_app`].
//!
//! The form keeps each setting as its row does (an option index, a checkbox, the text typed), so
//! a config is turned back into rows setting by setting: [`FIELD_PATHS`] says which row edits
//! which setting, and the setting's kind in the engine's schema says how its value is written on
//! the row. The rows that do not follow that pattern (two rows for one setting, a setting that is
//! two rows, the mask recipe, the Input tab's grouping) are set by hand below.

use serde_json::Value;

use qsmxt_engine::schema::{self, ValueType};
use qsmxt_engine::{ChiBoundChoice, EchoTimeChoice, PipelineConfig};

use super::app::*;

/// Rows set by [`App::set_special`] rather than by their setting's kind.
const SPECIAL: &[&str] = &[
    // The picker lists LSQR+HEIDI among the algorithms, so its index is not the setting's.
    "qsm_algorithm",
    // The two methods, then every region (`qsm.reference_region`).
    "qsm_reference",
    // 0-based in the config, 1-based on the row.
    "romeo_template",
    // A checkbox and a value for one setting, the checkbox off for one value of it.
    "dl_tiling", "dl_tile_size", "synthseg_smoothing", "synthseg_sigma",
    "romeo_temporal_uncertain", "romeo_temporal_uncertain_unwrapping",
    // Shows whether a dseg tool is named (`custom_dseg_tool`), which is the setting's own row.
    "segmentation_source",
];

/// A part of a config as the settings store holds it.
fn json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or_default()
}

/// The value at `path` in a config already serialised (`null` for an unset optional value).
fn value_at<'a>(config: &'a Value, path: &str) -> Option<&'a Value> {
    const NULL: &Value = &Value::Null;
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let obj = parent.split('.').filter(|k| !k.is_empty()).try_fold(config, |v, k| v.get(k))?;
    obj.as_object().map(|o| o.get(key).unwrap_or(NULL))
}

/// A value as a text row holds it: a number as the config writes it (64.0), a triple or list with
/// spaces between, and nothing for an unset value (the row's blank means its default).
fn text_of(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(xs) => xs.iter().map(text_of).collect::<Vec<_>>().join(" "),
        other => other.to_string(),
    }
}

/// The one value of a list whose values are all the same (`[12, 12, 12]`), which a row for a
/// value on each axis takes as one.
fn one_value(v: &Value) -> Option<&Value> {
    match v {
        Value::Array(xs) if xs.len() > 1 && xs.iter().all(|x| x == &xs[0]) => Some(&xs[0]),
        _ => None,
    }
}

impl App {
    /// Set the form so that it builds `config` again ([`super::command::config_from_app`]), for
    /// every setting the form can express.
    ///
    /// Only the rows of settings that differ are set: a value the form holds but does not pass on
    /// (hidden, because it does not apply) is left as it is, as when the user changes a setting by
    /// hand. A row can be hidden until another setting is set, so this goes round until the form
    /// builds `config` or nothing more changes. A value no row can hold (a mask preset run on a
    /// region the picker does not list, say) is left as the form has it.
    pub fn set_form_from_config(&mut self, config: &PipelineConfig) {
        let Ok(target) = serde_json::to_value(config) else { return };
        // Each round sets at least one more row whose setting it could not see before; a few
        // rounds are as deep as the rows hide one another.
        for _ in 0..4 {
            let now = super::command::config_from_app(self).ok().and_then(|c| serde_json::to_value(c).ok());
            if now.as_ref() == Some(&target) {
                return;
            }
            let differs = |path: &str| now.as_ref().is_none_or(|n| value_at(n, path) != value_at(&target, path));
            let before = self.form_print();
            for (field, path) in FIELD_PATHS.iter() {
                if !differs(path) || SPECIAL.contains(field) {
                    continue;
                }
                if let Some(v) = value_at(&target, path) {
                    self.set_row(field, path, v);
                }
            }
            self.set_special(config, &differs);
            if self.form_print() == before {
                return;
            }
        }
    }

    /// The settings of the form, to tell whether a round changed any.
    fn form_print(&self) -> String {
        format!("{:?}{:?}{:?}", self.form, self.pipeline_state, self.filter_state.orientation_group)
    }

    /// Set the row `field` to the value `v` of its setting at `path`, by the setting's kind.
    fn set_row(&mut self, field: &str, path: &str, v: &Value) {
        let Some(setting) = schema::settings().iter().find(|s| s.path == path) else { return };
        match &setting.value_type {
            ValueType::Bool => {
                if let Some(b) = v.as_bool() {
                    self.set_toggle(field, b);
                }
            }
            ValueType::Enum { options } => {
                // A select lists the setting's options in the schema's order: its row is built
                // from them.
                if let Some(i) = options.iter().position(|o| o.value == *v) {
                    self.set_field_select(field, i);
                }
            }
            // An echo time: the named times, then a time of the user's (`<method>_te_ms`).
            ValueType::EchoTime { .. } => {
                let Ok(te) = serde_json::from_value::<EchoTimeChoice>(v.clone()) else { return };
                if field.ends_with("_ms") {
                    if let EchoTimeChoice::Seconds(_) = te {
                        self.set_text(field, te.to_string());
                    }
                } else {
                    self.set_field_select(field, echo_time_index(te));
                }
            }
            // The echo to align onto: first, last, or an echo by number (`motion_reference_echo`).
            ValueType::EchoReference { .. } => {
                let Some(r) = v.as_str().and_then(|s| s.parse::<qsmxt_engine::motion::MotionReference>().ok()) else { return };
                match (field.ends_with("_echo"), r) {
                    (true, qsmxt_engine::motion::MotionReference::Echo(n)) => self.set_text(field, n.to_string()),
                    (true, _) => {}
                    (false, qsmxt_engine::motion::MotionReference::First) => self.set_field_select(field, 0),
                    (false, qsmxt_engine::motion::MotionReference::Last) => self.set_field_select(field, 1),
                    (false, qsmxt_engine::motion::MotionReference::Echo(_)) => self.set_field_select(field, 2),
                }
            }
            // DECOMPOSE's χ bound: a bound in ppm (`sep_decompose_chi_bound`), or from the echoes.
            ValueType::NumberOrChoice { .. } => {
                let Ok(b) = serde_json::from_value::<ChiBoundChoice>(v.clone()) else { return };
                if field.ends_with("_mode") {
                    self.set_field_select(field, chi_bound_index(b));
                } else if let ChiBoundChoice::Ppm(x) = b {
                    self.set_text(field, format!("{x}"));
                }
            }
            _ => {
                // The same value on every axis as one number, where the row takes it so ("12" for
                // the FFT's padding); a row that wants all three (STAR-QSM's pad) has them written out.
                if let Some(x) = one_value(v) {
                    self.set_text(field, text_of(x));
                    let built = super::command::config_from_app(self).ok().and_then(|c| schema::get(&c, path));
                    if built.as_ref() == Some(v) {
                        return;
                    }
                }
                self.set_text(field, text_of(v));
            }
        }
    }

    /// The rows that are not one setting of one kind each.
    fn set_special(&mut self, config: &PipelineConfig, differs: &dyn Fn(&str) -> bool) {
        if differs("qsm.reference") || differs("qsm.reference_region") {
            let index = match config.qsm.reference {
                qsmxt_engine::QsmReference::Mean => Some(0),
                qsmxt_engine::QsmReference::None => Some(1),
                // Only a region the picker lists: one written by hand (`4,43`) has no row.
                qsmxt_engine::QsmReference::Region => config.qsm.reference_region.as_deref()
                    .and_then(|r| qsm_ref_options().iter().skip(2).position(|o| *o == r)).map(|i| i + 2),
            };
            if let Some(i) = index {
                self.set_field_select("qsm_reference", i);
            }
        }
        if differs("inversion.algorithm") {
            // The algorithm itself: whether HEIDI is on is its own setting (`heidi_enabled`).
            let name = config.inversion.algorithm.to_string();
            if QSM_ALGO_OPTIONS.contains(&name.as_str()) {
                self.form.settings.set("inversion.algorithm", name);
            }
        }
        let m = &config.masking;
        if differs("masking.sections") || differs("masking.combine") || differs("masking.refinements") {
            // The recipe's parts are set together, as a preset sets them, and the preset is the
            // one whose recipe this is, else a recipe edited by hand.
            self.pipeline_state.mask_preset = qsmxt_engine::mask_presets().iter()
                .position(|(_, r)| r.sections == m.sections && r.combine == m.combine && r.refinements == m.refinements)
                .unwrap_or(*MASK_PRESET_CUSTOM);
            let st = &mut self.form.settings;
            st.set("masking.sections", json(&m.sections));
            st.set("masking.combine", json(&m.combine));
            st.set("masking.refinements", json(&m.refinements));
        }
        // An unset reliable mask is the one the form holds by default: the form's own is kept.
        if differs("masking.two_pass_sections") && m.two_pass_sections.is_some() {
            self.form.settings.set("masking.two_pass_sections", json(&m.two_pass_sections));
        }
        if differs("masking.two_pass_combine") {
            self.form.settings.set("masking.two_pass_combine", json(&m.two_pass_combine));
        }
        if differs("dismissed_recommendations") {
            self.form.dismissed = config.dismissed_recommendations.clone();
        }
        if differs("reference_pipeline") {
            self.form.reference_pipeline = config.reference_pipeline.clone();
        }
        if differs("multi_orientation.group_by") {
            self.filter_state.orientation_group = config.multi_orientation.group_by.clone();
        }
        let romeo = &config.field_mapping.romeo;
        if differs("field_mapping.romeo.template") {
            self.form.settings.set("field_mapping.romeo.template", romeo.template);
        }
        if differs("field_mapping.romeo.temporal_uncertain_unwrapping") {
            // Off is a threshold of 0; the value row keeps its number while off.
            let st = &mut self.form.settings;
            match romeo.temporal_uncertain_unwrapping {
                t if t > 0.0 => st.set("field_mapping.romeo.temporal_uncertain_unwrapping", t),
                _ => st.switch("field_mapping.romeo.temporal_uncertain_unwrapping", false),
            }
        }
        if differs("inversion.tile_size") {
            // Off is no tile size; the size row keeps its number while off.
            let st = &mut self.form.settings;
            match config.inversion.tile_size {
                Some(n) => st.set("inversion.tile_size", n),
                None => st.switch("inversion.tile_size", false),
            }
        }
        let sigma = config.segmentation.sigma_smoothing;
        if differs("segmentation.sigma_smoothing") {
            // Off is a sigma of 0; the sigma row keeps its number while off.
            if sigma > 0.0 {
                self.form.settings.set("segmentation.sigma_smoothing", sigma);
            } else {
                self.form.settings.switch("segmentation.sigma_smoothing", false);
            }
        }
    }

    /// Tick or untick a checkbox as the setting has it. Not through [`App::toggle_field`], which
    /// refuses to turn off an output another setting needs: the config says what is on.
    fn set_toggle(&mut self, field: &str, on: bool) {
        if let Some(path) = super::settings_rows::stored_path(field) {
            self.form.settings.set(path, Value::Bool(on));
        } else if let Some(b) = self.form.flag_mut(field) {
            *b = on;
        }
    }

    fn set_text(&mut self, field: &str, text: String) {
        if let Some(s) = self.field_text_mut(field) {
            *s = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::command::config_from_app;
    use super::super::explore::{build, explored};

    /// The settings two configs differ in, as JSON pointers.
    fn diff(a: &PipelineConfig, b: &PipelineConfig) -> Vec<String> {
        fn walk(p: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
            match (a, b) {
                (Value::Object(x), Value::Object(y)) => {
                    let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
                    for k in keys {
                        walk(&format!("{p}/{k}"), x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), out);
                    }
                }
                _ if a != b => out.push(format!("{p}: {a} vs {b}")),
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk("", &serde_json::to_value(a).unwrap(), &serde_json::to_value(b).unwrap(), &mut out);
        out
    }

    /// What a fresh form set from `config` builds differs from `config` in.
    fn round_trip(config: &PipelineConfig) -> Vec<String> {
        let mut app = App::new_for_trial();
        app.set_form_from_config(config);
        match config_from_app(&app) {
            Ok(back) => diff(config, &back),
            Err(e) => vec![format!("the form does not build: {e}")],
        }
    }

    /// Every form the TUI reaches builds a config that, set on a fresh form, builds itself again.
    #[test]
    #[ignore = "slow: builds every form the TUI can reach (about 45 s in a debug build)"]
    fn every_explored_forms_config_round_trips() {
        let mut failures = Vec::new();
        for acts in explored() {
            let config = config_from_app(&build(acts)).expect("the form builds a config");
            let d = round_trip(&config);
            if !d.is_empty() {
                failures.push(format!("after {acts:?}: {d:?}"));
            }
        }
        assert!(failures.is_empty(), "{} of {} forms, e.g.:\n{}", failures.len(), explored().len(),
                failures[..failures.len().min(8)].join("\n"));
    }

    /// Every text row, changed from its default on a form that shows it, round-trips: numbers,
    /// triples and lists are written back as the row reads them.
    #[test]
    #[ignore = "slow: builds every form the TUI can reach (about 40 s in a debug build)"]
    fn every_text_rows_value_round_trips() {
        let mut done = std::collections::BTreeSet::new();
        let mut failures = Vec::new();
        for acts in explored() {
            let app = build(acts);
            for (_, _, rows) in app.all_pages() {
                for row in rows {
                    let PipelineRow::Param { field, .. } = row else { continue };
                    if field_path(field).is_none() || !done.insert(field) {
                        continue;
                    }
                    let mut changed = build(acts);
                    let Some(text) = changed.field_text_mut(field) else { continue };
                    let was = text.clone();
                    *text = match (field, text.as_str()) {
                        // Empty is the whole volume; a patch is three numbers.
                        ("sep_susepnet_patch" | "sep_chisepnet_frame_patch", "") => "64 64 32".to_string(),
                        // SynthSeg's window is at least 64 voxels.
                        ("synthseg_patch", "") => "96".to_string(),
                        (_, t) => bumped(t, 1),
                    };
                    // A value at the top of its range moves down instead.
                    if config_from_app(&changed).is_err() {
                        *changed.field_text_mut(field).unwrap() = bumped(&was, -1);
                    }
                    let config = config_from_app(&changed).expect("the form builds a config");
                    let d = round_trip(&config);
                    if !d.is_empty() {
                        failures.push(format!("{field} after {acts:?}: {d:?}"));
                    }
                }
            }
        }
        assert!(done.len() > 200, "only {} text rows", done.len());
        assert!(failures.is_empty(), "{} rows:\n{}", failures.len(), failures.join("\n"));
    }

    /// A row's text with each number moved off its value, up (`by` 1) or down (-1); an empty row
    /// becomes 3.
    fn bumped(v: &str, by: i64) -> String {
        let bump = |t: &str| -> Option<String> {
            if let Ok(n) = t.parse::<i64>() { return Some((n + by).to_string()); }
            let x = t.parse::<f64>().ok()?;
            Some(format!("{}", if x == 0.0 { 0.1 } else if by > 0 { x * 0.9 } else { x * 1.1 }))
        };
        let tokens: Vec<&str> = v.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).collect();
        match tokens.iter().map(|t| bump(t)).collect::<Option<Vec<_>>>() {
            Some(b) if !b.is_empty() => b.join(" "),
            _ => "3".to_string(),
        }
    }

    /// Each toolbox pipeline's settings, chosen on a fresh form, round-trip.
    #[test]
    fn every_reference_pipeline_round_trips() {
        let facts = qsmxt_engine::schema::ViewFacts { num_echoes: Some(4), ..Default::default() };
        let mut failures = Vec::new();
        for id in qsmxt_engine::pipelines::ids() {
            let config = qsmxt_engine::pipelines::target(&PipelineConfig::default(), id, &facts).expect("a pipeline");
            let d = round_trip(&config);
            if !d.is_empty() {
                failures.push(format!("{id}: {d:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Setting a form to the config it already builds changes nothing on it, hidden values
    /// included.
    #[test]
    fn a_form_set_to_its_own_config_is_unchanged() {
        use super::super::explore::Act;
        let forms: [&[Act]; 4] = [
            &[],
            &[Act::Select("qsm_algorithm", 7)],
            &[Act::Group, Act::Toggle("cosmos_magnitude_weighting")],
            &[Act::Toggle("two_pass"), Act::Select("mask_preset", 1)],
        ];
        for acts in forms {
            let mut app = build(acts);
            // Hidden unless COSMOS is weighted: not passed on, so the config cannot say it.
            *app.field_text_mut("cosmos_tol").unwrap() = "5".into();
            let before = app.form_print();
            let config = config_from_app(&app).unwrap();
            app.set_form_from_config(&config);
            assert_eq!(before, app.form_print(), "after {acts:?}");
        }
    }
}
