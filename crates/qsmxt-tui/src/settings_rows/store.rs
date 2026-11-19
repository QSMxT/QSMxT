//! The settings store: the stored sections' values and the text typed on their rows.

use super::*;

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
        self.values.get(path).cloned().unwrap_or_else(|| form_default(path))
    }

    /// Set a value, dropping any text typed for it. A value the config refuses is left out.
    pub fn set(&mut self, path: &'static str, value: impl Into<Value>) {
        self.typed.remove(path);
        self.put(path, value.into());
    }

    /// Set a value, leaving any text typed for it.
    pub(super) fn put(&mut self, path: &'static str, value: Value) {
        if value == form_default(path) {
            self.values.remove(path);
        } else if config_with(&[(path, value.clone())]).is_some() {
            self.values.insert(path, value);
        }
    }

    /// A checkbox: a bool, or whether a setting with a value that turns it off is on.
    pub fn toggle(&self, path: &str) -> bool {
        match &setting(path).off {
            Some(off) => self.get(path) != *off,
            None => self.get(path).as_bool().unwrap_or(false),
        }
    }

    pub fn flip(&mut self, path: &'static str) {
        let on = self.toggle(path);
        self.switch(path, !on);
    }

    /// Tick or untick a checkbox. A setting with a value that turns it off keeps its value's text
    /// while it is off, and takes it again when it is turned back on (its default, when that
    /// text is no value).
    pub fn switch(&mut self, path: &'static str, on: bool) {
        let s = setting(path);
        let Some(off) = &s.off else { return self.set(path, on) };
        if on == self.toggle(path) {
            return;
        }
        if on {
            let v = self.typed.get(path).and_then(|t| parse(s, t)).filter(|v| v != off)
                .unwrap_or_else(|| on_value(s));
            self.put(path, v);
        } else {
            if !self.typed.contains_key(path) {
                let text = self.value_text(path);
                self.typed.insert(path, text);
            }
            self.put(path, off.clone());
        }
    }

    /// A choice's position in the setting's options. An echo reference given as an echo number
    /// is its echo-number option.
    pub fn select(&self, path: &str) -> usize {
        let v = self.get(path);
        let s = setting(path);
        if let Some(&(_, choice, other)) = picker(path) {
            let own = s.value_type.options().into_iter().filter(|o| o.value != choice).collect::<Vec<_>>();
            if v == choice {
                let w = self.get(other);
                return setting(other).value_type.options().iter().position(|o| o.value == w).map_or(0, |i| own.len() + i);
            }
            return own.iter().position(|o| o.value == v).unwrap_or(0);
        }
        let number = echo_number(s, &v).is_some();
        s.value_type.options().iter().position(|o| o.value == v).or_else(|| match &s.value_type {
            ValueType::EchoReference { options } =>
                options.iter().position(|o| number && matches!(o.kind, EchoKind::EchoNumber { .. })),
            // A given time: `{ "seconds": s }`.
            ValueType::EchoTime { options } if v.is_object() => options.iter().position(|o| o.kind == EchoKind::Time),
            _ => None,
        }).map(|i| i + usize::from(matches!(s.value_type, ValueType::NumberOrChoice { .. })))
        .unwrap_or(0)
    }

    pub fn set_select(&mut self, path: &'static str, index: usize) {
        if index == self.select(path) {
            return; // an echo number chosen again keeps its number
        }
        let s = setting(path);
        if let Some(&(_, choice, other)) = picker(path) {
            let own = s.value_type.options().into_iter().filter(|o| o.value != choice).collect::<Vec<_>>();
            match own.get(index) {
                Some(o) => {
                    self.set(path, o.value.clone());
                    self.set(other, setting(other).default.clone());
                }
                None => {
                    let others = setting(other).value_type.options();
                    if let Some(o) = others.get((index - own.len()).min(others.len().saturating_sub(1))) {
                        self.set(path, choice);
                        self.set(other, o.value.clone());
                    }
                }
            }
            return;
        }
        if let (ValueType::NumberOrChoice { .. }, Some(i)) = (&s.value_type, index.checked_sub(1)) {
            // A named choice; the number typed waits under it.
            let opts = s.value_type.options();
            if let Some(o) = opts.get(i.min(opts.len().saturating_sub(1))) {
                self.put(path, o.value.clone());
            }
            return;
        }
        if let ValueType::NumberOrChoice { .. } = s.value_type {
            // The number: the one typed, else the default's.
            let v = self.typed.get(path).and_then(|t| parse(s, t)).unwrap_or_else(|| number_default(s));
            return self.put(path, v);
        }
        let opts = s.value_type.options();
        if let Some(o) = opts.get(index.min(opts.len().saturating_sub(1))) {
            match &s.value_type {
                // The echo number typed waits under a named echo, for the number to be chosen again.
                ValueType::EchoReference { .. } => self.put(path, o.value.clone()),
                // A given time: the one typed, else the method's on the default echo times. The
                // time typed waits under a named echo time, as an echo number does.
                ValueType::EchoTime { options } if options[index.min(options.len() - 1)].kind == EchoKind::Time => {
                    let text = self.typed.get(path).cloned().unwrap_or_else(|| echo_time_ms(path));
                    if let Some(v) = echo_time_of_ms(&text).or_else(|| echo_time_of_ms(&echo_time_ms(path))) {
                        self.put(path, v);
                    }
                }
                ValueType::EchoTime { .. } => self.put(path, o.value.clone()),
                _ => self.set(path, o.value.clone()),
            }
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
    pub(super) fn value_text(&self, path: &str) -> String {
        let s = setting(path);
        let v = self.get(path);
        match &s.value_type {
            _ if BLANK_AT_DEFAULT.contains(&path) && v == s.default => String::new(),
            _ if ONE_VALUE_FOR_EVERY_AXIS.contains(&path) => match v.as_array() {
                Some(xs) if !xs.is_empty() && xs.iter().all(|x| *x == xs[0]) => text_of(&xs[0]),
                _ => text_of(&v),
            },
            // A given echo time, in ms; while a named one is chosen, the method's on the default
            // echo times.
            ValueType::EchoTime { .. } => serde_json::from_value::<qsmxt_engine::EchoTimeChoice>(v.clone()).ok()
                .filter(|te| matches!(te, qsmxt_engine::EchoTimeChoice::Seconds(_)))
                .map_or_else(|| echo_time_ms(path), |te| te.to_string()),
            // The number; while a named choice is chosen, the default's.
            ValueType::NumberOrChoice { .. } if !v.is_number() => text_of(&number_default(s)),
            _ if COUNTED_FROM_ONE.contains(&path) => v.as_u64().map_or_else(|| text_of(&v), |n| (n + 1).to_string()),
            ValueType::EchoReference { options } => echo_number(s, &v).map(|n| n.to_string()).unwrap_or_else(|| {
                options.iter().find_map(|o| match o.kind { EchoKind::EchoNumber { min } => Some(min.to_string()), _ => None })
                    .unwrap_or_default()
            }),
            _ => text_of(&v),
        }
    }

    /// Put a setting back to QSMxT's default. `value_row` resets only the value's row: an echo
    /// reference's number, an echo time's time or a number-or-choice's number (the choice leaves
    /// the number, as the two rows are reset one at a time), or the value of a checkbox with a
    /// value.
    pub fn reset(&mut self, path: &'static str, value_row: bool) {
        let s = setting(path);
        let echo = matches!(s.value_type,
            ValueType::EchoReference { .. } | ValueType::EchoTime { .. } | ValueType::NumberOrChoice { .. });
        if value_row || !echo {
            self.typed.remove(path);
        }
        if !value_row {
            self.values.remove(path);
        } else if s.off.is_some() && self.toggle(path) {
            // The value of a checkbox that is on: back to the default value, still on.
            self.put(path, on_value(s));
        } else if matches!(s.value_type, ValueType::NumberOrChoice { .. }) && self.get(path).is_number() {
            // The number: back to the default's, still a number.
            self.put(path, number_default(s));
        } else if matches!(s.value_type, ValueType::EchoTime { .. }) && self.get(path).is_object() {
            // A given echo time: back to the method's, still given.
            if let Some(v) = echo_time_of_ms(&echo_time_ms(path)) {
                self.put(path, v);
            }
        }
    }

    /// The stored settings that differ from QSMxT's defaults as the run takes them (those a fresh
    /// form holds otherwise, [`FORM_DEFAULTS`], included), typed text
    /// read as its value (a blank text leaves the default), and the settings whose typed text the
    /// run cannot take, with why: text that reads as no value of its setting ("abc" for a
    /// number), a number outside the setting's range or one the config cannot hold, or an echo
    /// number the setting does not take. The command line refuses all of them, and running with
    /// the default instead would change a setting behind the user's back.
    pub fn changed_or_refused(&self) -> (Vec<(&'static str, Value)>, Vec<Refused>) {
        let mut out: BTreeMap<&'static str, Value> = self.values.clone();
        out.retain(|p, _| !PASSED_BY_THE_FORM.contains(p));
        for (path, value) in FORM_DEFAULTS {
            out.entry(path).or_insert_with(value);
        }
        let mut refused = Vec::new();
        for (&path, text) in &self.typed {
            let s = setting(path);
            // A value kept while its checkbox is off is not taken.
            if s.off.as_ref().is_some_and(|off| self.get(path) == *off) {
                continue;
            }
            // A time typed is taken only while a given time is chosen, a number only while the
            // number is.
            if matches!(s.value_type, ValueType::EchoTime { .. }) && !self.get(path).is_object()
                || matches!(s.value_type, ValueType::NumberOrChoice { .. }) && !self.get(path).is_number()
            {
                continue;
            }
            if let ValueType::EchoReference { .. } = s.value_type {
                // Taken only while an echo by number is chosen, and as it is typed.
                if echo_number(s, &self.get(path)).is_none() {
                    continue;
                }
                let v = Value::from(text.trim());
                match config_try(&[(path, v.clone())]) {
                    Ok(_) => out.insert(path, v),
                    Err(e) => {
                        refused.push((path, format!("{} {}: {e}", s.cli_flag.unwrap_or(path), text.trim())));
                        out.remove(path)
                    }
                };
                continue;
            }
            let value = parse(s, text);
            if value.is_none() && !text.trim().is_empty() {
                let flag = s.cli_flag.unwrap_or(path);
                refused.push((path, format!("{flag} {}: not {}", text.trim(), what_it_takes(s))));
                out.remove(path);
                continue;
            }
            let Some(v) = value else {
                out.remove(path);
                continue;
            };
            // A number outside the setting's range, or one the config cannot hold (a negative
            // count), is refused as the run refuses it, rather than run as the default.
            let flag = s.cli_flag.unwrap_or(path);
            let numbers: Vec<f64> = match &v {
                Value::Array(xs) => xs.iter().filter_map(Value::as_f64).collect(),
                other => other.as_f64().into_iter().collect(),
            };
            let why = if numbers.iter().any(|&x| !s.in_range(x)) {
                Some(format!("must be {}", s.range_text()))
            } else {
                config_try(&[(path, v.clone())]).err().map(|e| match s.value_type {
                    ValueType::Int | ValueType::IntVec3 | ValueType::IntList if numbers.iter().any(|&x| x < 0.0) =>
                        "must be at least 0".to_string(),
                    _ => e,
                })
            };
            if let Some(why) = why {
                refused.push((path, format!("{flag} {}: {why}", text.trim())));
                out.remove(path);
                continue;
            }
            if v == s.default { out.remove(path) } else { out.insert(path, v) };
        }
        (out.into_iter().collect(), refused)
    }

    /// [`Self::changed_or_refused`]'s values.
    pub fn changed(&self) -> Vec<(&'static str, Value)> {
        self.changed_or_refused().0
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
