//! The rows of a page built from the registry, one widget per value type.

use super::*;

impl App {
    /// The settings that apply to `config`, from [`Self::visible_cache`] when it has them.
    pub(crate) fn visible_settings(&self, config: &PipelineConfig) -> Rc<HashSet<&'static str>> {
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
    pub(crate) fn registry_rows(&self, page: Page, ctx: &RowContext, config: Option<&PipelineConfig>) -> Option<Vec<PipelineRow>> {
        let layout = LAYOUTS.iter().find(|l| l.page == page);
        if layout.is_none() && page != Page::Masking {
            return None;
        }
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
        // The Masking page is the mask builder's ([`crate::mask_builder`]).
        let Some(layout) = layout else { return Some(self.masking_rows(config)) };
        let visible = self.visible_settings(config);
        let shows = |path: &str| visible.contains(path);
        let is_on = |path: &str| field_of_path(path).is_some_and(|f| self.field_toggle(f));
        let mut rows = Vec::new();
        let mut headed = HashSet::new();
        // A block's heading, once, then its settings under it.
        let mut block_row = |rows: &mut Vec<PipelineRow>, b: &Block, s: &'static Setting| {
            let depth = depth_of(setting(b.under)) + 1;
            if headed.insert(b.prefix) {
                rows.push(PipelineRow::Note { text: format!("{}{}", "  ".repeat(depth), b.heading).into() });
            }
            self.push_rows(rows, s, depth + 1);
        };
        let foreign = |b: &Block| schema::settings().iter().any(|t| t.path.starts_with(b.prefix) && t.section != layout.section);
        let mut order: Vec<&'static Setting> = layout.first.iter().map(|p| setting(p))
            .chain(schema::settings().iter().filter(|s| !layout.first.contains(&s.path))).collect();
        if let Some(arrange) = layout.arrange {
            arrange(self, &mut order);
        }
        for s in order {
            if s.section != layout.section || !layout.only.is_none_or(|f| f(s.path)) || !shows(s.path)
                || PICKERS.iter().any(|(.., other)| *other == s.path)
            {
                continue;
            }
            if let Some(b) = layout.blocks.iter().find(|b| s.path.starts_with(b.prefix)) {
                if is_on(b.under) {
                    block_row(&mut rows, b, s);
                }
                continue;
            }
            let depth = layout.depth(s);
            for (_, before) in layout.before.iter().filter(|(p, _)| *p == s.path) {
                rows.extend(before(self, ctx));
            }
            if !layout.shown_if.iter().filter(|(p, _)| *p == s.path).all(|(_, f)| f(self)) {
                continue;
            }
            match layout.reworded.iter().find(|(p, when, ..)| *p == s.path && when(self)) {
                Some(&(_, _, label, help)) => self.push_rows_worded(&mut rows, s, depth, label, help.unwrap_or(tui_help(s))),
                None => self.push_rows(&mut rows, s, depth),
            }
            for (_, after) in layout.after.iter().filter(|(p, _)| *p == s.path) {
                rows.extend(after(self, ctx));
            }
            // A block of another section follows the setting it is under.
            for b in layout.blocks.iter().filter(|b| b.under == s.path && foreign(b) && is_on(b.under)) {
                for t in schema::settings().iter().filter(|t| t.path.starts_with(b.prefix) && shows(t.path)) {
                    block_row(&mut rows, b, t);
                }
            }
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
    /// The rows of a setting, by its value type: one; for an echo reference given as an echo
    /// number, the select and the number's row under it; for a setting with a value that turns it
    /// off, a checkbox and, while it is on, the value's row under it.
    pub(crate) fn push_rows(&self, rows: &mut Vec<PipelineRow>, s: &'static Setting, depth: usize) {
        self.push_rows_worded(rows, s, depth, s.label, tui_help(s));
    }

    /// [`Self::push_rows`], the setting's own row labelled `label` with `help`.
    pub(super) fn push_rows_worded(&self, rows: &mut Vec<PipelineRow>, s: &'static Setting, depth: usize, label: &'static str, help: &'static str) {
        let field = row_field(s.path, 0).unwrap_or_else(|| panic!("{} has no row field", s.path));
        let label = indented(depth, label);
        rows.push(match s.value_type {
            _ if s.off.is_some() => PipelineRow::Toggle { label, field, help },
            ValueType::Bool => PipelineRow::Toggle { label, field, help },
            ValueType::Enum { .. } | ValueType::EchoReference { .. } | ValueType::EchoTime { .. }
            | ValueType::NumberOrChoice { .. } => {
                let (options, help) = options(s.path);
                PipelineRow::AlgoSelect { label, field, options, help }
            }
            ValueType::Int | ValueType::Float | ValueType::IntVec3 | ValueType::FloatVec3 | ValueType::IntList
            | ValueType::Text => PipelineRow::Param { label, field, help },
            _ => unreachable!("{}: no widget for its value type yet", s.path),
        });
        let chosen = |options: &[schema::EchoOption]| options.get(self.field_select(field)).map(|o| o.kind);
        let value_row = match &s.value_type {
            _ if s.off.is_some() => self.field_toggle(field),
            ValueType::EchoReference { options } => matches!(chosen(options), Some(EchoKind::EchoNumber { .. })),
            ValueType::EchoTime { options } => matches!(chosen(options), Some(EchoKind::Time)),
            ValueType::NumberOrChoice { .. } => self.field_select(field) == 0,
            _ => false,
        };
        if let (true, Some((label, help))) = (value_row, s.tui_value) {
            let field = row_field(s.path, 1).unwrap_or_else(|| panic!("{} has no value row field", s.path));
            rows.push(PipelineRow::Param { label: indented(depth + 1, label), field, help });
        }
    }
}

/// The field of the `nth` row editing a setting: its own row (0), or its value's row (1). A row of
/// a choice the TUI makes about the setting ([`TUI_CHOICES`]) is neither.
pub(super) fn row_field(path: &str, nth: usize) -> Option<&'static str> {
    FIELD_PATHS.iter().filter(|(f, p)| *p == path && !TUI_CHOICES.contains(f)).nth(nth).map(|(f, _)| *f)
}

/// Whether `field` is the value row of the setting at `path` (an echo reference's number, the
/// sigma of a checkbox with a value).
pub fn is_value_row(field: &str, path: &str) -> bool {
    row_field(path, 1) == Some(field)
}
